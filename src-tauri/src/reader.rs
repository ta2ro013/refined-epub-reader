use crate::book::{Book, BookError, Resource, TocEntry};
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
};
use tauri::http::{header, Method, Request, Response};

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderError {
    Unreadable,
    Invalid,
    Unsupported,
    MissingResource,
    StaleBook,
    Busy,
    Forbidden,
    Internal,
}

impl From<BookError> for ReaderError {
    fn from(error: BookError) -> Self {
        match error {
            BookError::Unreadable => Self::Unreadable,
            BookError::Invalid => Self::Invalid,
            BookError::Unsupported => Self::Unsupported,
            BookError::MissingResource => Self::MissingResource,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct BookInfo {
    pub id: String,
    pub title: String,
    pub chapters: Vec<String>,
    pub toc: Vec<TocEntry>,
    pub resource_base: String,
}

pub struct Reader<R: Read + Seek> {
    current: Option<(String, Book<R>)>,
    generation: u64,
}

impl<R: Read + Seek> Default for Reader<R> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ReaderState {
    reader: Mutex<Reader<BufReader<File>>>,
    pub selection: SelectionGate,
}

impl Default for ReaderState {
    fn default() -> Self {
        Self {
            reader: Mutex::new(Reader::new()),
            selection: SelectionGate::default(),
        }
    }
}

impl ReaderState {
    pub fn select_path(&self, path: Option<&Path>) -> Result<Option<BookInfo>, ReaderError> {
        let selected = path.map(Book::open);
        self.reader
            .lock()
            .map_err(|_| ReaderError::Internal)?
            .select(selected)
    }

    pub fn chapter(&self, id: &str, index: usize) -> Result<String, ReaderError> {
        self.reader
            .lock()
            .map_err(|_| ReaderError::Internal)?
            .chapter(id, index)
    }

    pub fn respond(&self, webview: &str, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        match self.reader.lock() {
            Ok(mut reader) => reader.respond(webview, request),
            Err(_) => error_response(ReaderError::Internal),
        }
    }
}

impl<R: Read + Seek> Reader<R> {
    pub fn new() -> Self {
        Self {
            current: None,
            generation: 0,
        }
    }

    pub fn select(
        &mut self,
        selected: Option<Result<Book<R>, BookError>>,
    ) -> Result<Option<BookInfo>, ReaderError> {
        let Some(selected) = selected else {
            return Ok(None);
        };
        let mut book = selected.map_err(ReaderError::from)?;
        book.chapter(0).map_err(ReaderError::from)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ReaderError::Internal)?;
        let id = generation.to_string();
        let info = BookInfo {
            id: id.clone(),
            title: book.title.clone(),
            chapters: book.chapters.clone(),
            toc: copy_toc(&book.toc),
            resource_base: format!(
                "{}/{id}/",
                if cfg!(target_os = "windows") {
                    "http://book.localhost"
                } else {
                    "book://localhost"
                }
            ),
        };
        self.current = Some((id, book));
        self.generation = generation;
        Ok(Some(info))
    }

    pub fn chapter(&mut self, id: &str, index: usize) -> Result<String, ReaderError> {
        let (current_id, book) = self.current.as_mut().ok_or(ReaderError::StaleBook)?;
        if current_id != id {
            return Err(ReaderError::StaleBook);
        }
        book.chapter(index).map_err(ReaderError::from)
    }

    pub fn respond(&mut self, webview: &str, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        if webview != "main" {
            return finish_response(error_response(ReaderError::Forbidden), request, None);
        }
        let origin = match request.headers().get(header::ORIGIN) {
            Some(value) => match value.to_str() {
                Ok(origin) if allowed_origin(origin) => Some(origin),
                _ => return finish_response(error_response(ReaderError::Forbidden), request, None),
            },
            None => None,
        };
        if request.method() != Method::GET && request.method() != Method::HEAD {
            let mut response = error_response(ReaderError::Invalid);
            *response.status_mut() = tauri::http::StatusCode::METHOD_NOT_ALLOWED;
            response
                .headers_mut()
                .insert(header::ALLOW, "GET, HEAD".parse().unwrap());
            return finish_response(response, request, origin);
        }
        let result = (|| {
            let scheme = request.uri().scheme_str();
            let authority = request.uri().authority().map(|value| value.as_str());
            if !matches!(
                (scheme, authority),
                (Some("book"), Some("localhost"))
                    | (Some("http" | "https"), Some("book.localhost"))
            ) {
                return Err(ReaderError::Invalid);
            }
            let (id, path) = request
                .uri()
                .path()
                .strip_prefix('/')
                .and_then(|path| path.split_once('/'))
                .ok_or(ReaderError::Invalid)?;
            let path = decode_resource_path(path)?;
            let (current_id, book) = self.current.as_mut().ok_or(ReaderError::StaleBook)?;
            if id != current_id {
                return Err(ReaderError::StaleBook);
            }
            let resource = book.resource(&path).map_err(ReaderError::from)?;
            // Chapter XHTML is delivered through the validated chapter command,
            // never as an executable document at the resource origin.
            if !matches!(
                resource.mime.as_str(),
                "text/css" | "image/png" | "image/jpeg" | "image/gif"
            ) {
                return Err(ReaderError::Unsupported);
            }
            Ok(resource)
        })();
        let response = match result {
            Ok(Resource { mime, bytes, .. }) => Response::builder()
                .header("Content-Type", mime)
                .header("Cache-Control", "no-store")
                .header("X-Content-Type-Options", "nosniff")
                .body(bytes)
                .expect("resource MIME was checked against a fixed allowlist"),
            Err(error) => error_response(error),
        };
        finish_response(response, request, origin)
    }
}

fn finish_response(
    mut response: Response<Vec<u8>>,
    request: &Request<Vec<u8>>,
    origin: Option<&str>,
) -> Response<Vec<u8>> {
    if let Some(origin) = origin {
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            origin.parse().expect("origin is a fixed allowlisted value"),
        );
    }
    response
        .headers_mut()
        .insert(header::VARY, "Origin".parse().unwrap());
    let length = response.body().len();
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, length.to_string().parse().unwrap());
    if request.method() == Method::HEAD {
        response.body_mut().clear();
    }
    response
}

#[derive(Clone, Default)]
pub struct SelectionGate(Arc<AtomicBool>);

pub struct SelectionPermit(Arc<AtomicBool>);

impl SelectionGate {
    pub fn begin(&self) -> Result<SelectionPermit, ReaderError> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ReaderError::Busy)?;
        Ok(SelectionPermit(Arc::clone(&self.0)))
    }
}

impl Drop for SelectionPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn allowed_origin(origin: &str) -> bool {
    matches!(
        origin,
        "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
    ) || (cfg!(debug_assertions) && origin == "http://localhost:1420")
}

pub fn allowed_window<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> bool {
    if window.label() != "main" {
        return false;
    }
    let Ok(url) = window.url() else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    allowed_origin(&format!("{}://{host}{port}", url.scheme()))
}

fn decode_resource_path(path: &str) -> Result<String, ReaderError> {
    let mut decoded = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes
                .next()
                .and_then(|value| (value as char).to_digit(16))
                .ok_or(ReaderError::Invalid)?;
            let low = bytes
                .next()
                .and_then(|value| (value as char).to_digit(16))
                .ok_or(ReaderError::Invalid)?;
            let byte = (high * 16 + low) as u8;
            if byte == b'/' || byte == b'\\' {
                return Err(ReaderError::Invalid);
            }
            decoded.push(byte);
        } else {
            decoded.push(byte);
        }
    }
    let path = String::from_utf8(decoded).map_err(|_| ReaderError::Invalid)?;
    if path.starts_with('/')
        || path.contains(['\\', ':', '?'])
        || path.chars().any(char::is_control)
    {
        return Err(ReaderError::Invalid);
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or(ReaderError::Invalid)?;
            }
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(ReaderError::Invalid);
    }
    Ok(parts.join("/"))
}

pub fn error_response(error: ReaderError) -> Response<Vec<u8>> {
    let status = match error {
        ReaderError::StaleBook | ReaderError::MissingResource => 404,
        ReaderError::Forbidden => 403,
        ReaderError::Busy => 409,
        ReaderError::Unsupported => 415,
        ReaderError::Unreadable | ReaderError::Internal => 500,
        ReaderError::Invalid => 400,
    };
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(serde_json::to_vec(&error).expect("error enum is serializable"))
        .expect("constant error response headers are valid")
}

fn copy_toc(entries: &[TocEntry]) -> Vec<TocEntry> {
    entries
        .iter()
        .map(|entry| TocEntry {
            label: entry.label.clone(),
            path: entry.path.clone(),
            fragment: entry.fragment.clone(),
            children: copy_toc(&entry.children),
        })
        .collect()
}
