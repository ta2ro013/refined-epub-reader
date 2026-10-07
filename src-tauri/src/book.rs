use epub::archive::EpubArchive;
use epub::doc::{EpubDoc, EpubVersion, ResourceItem};
use roxmltree::{Document, Node, ParsingOptions};
use std::io::{Read, Seek, SeekFrom};
use std::{fs::File, io::BufReader, path::Path};

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BookError {
    Unreadable,
    Invalid,
    Unsupported,
    MissingResource,
}

impl Book<BufReader<File>> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, BookError> {
        let file = File::open(path).map_err(|_| BookError::Unreadable)?;
        Self::from_reader(BufReader::new(file))
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct TocEntry {
    pub label: String,
    pub path: Option<String>,
    pub fragment: Option<String>,
    pub children: Vec<TocEntry>,
}

#[derive(Debug, serde::Serialize)]
pub struct Resource {
    pub path: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub struct Book<R: Read + Seek> {
    doc: EpubDoc<R>,
    pub title: String,
    pub chapters: Vec<String>,
    pub toc: Vec<TocEntry>,
}

impl<R: Read + Seek> Book<R> {
    pub fn from_reader(mut reader: R) -> Result<Self, BookError> {
        {
            let mut archive =
                EpubArchive::from_reader(&mut reader).map_err(|_| BookError::Invalid)?;
            if archive
                .get_entry("mimetype")
                .map_err(|_| BookError::Invalid)?
                != b"application/epub+zip"
            {
                return Err(BookError::Invalid);
            }
            if archive
                .files
                .iter()
                .any(|path| path == "META-INF/encryption.xml")
            {
                let encryption = archive
                    .get_entry_as_str("META-INF/encryption.xml")
                    .map_err(|_| BookError::Invalid)?;
                let xml = Document::parse(&encryption).map_err(|_| BookError::Invalid)?;
                if xml
                    .descendants()
                    .any(|node| node.has_tag_name("EncryptedData"))
                {
                    return Err(BookError::Unsupported);
                }
            }
            let container = archive
                .get_entry_as_str("META-INF/container.xml")
                .map_err(|_| BookError::Invalid)?;
            let container = Document::parse(&container).map_err(|_| BookError::Invalid)?;
            let root = container
                .descendants()
                .find(|node| node.has_tag_name("rootfile"))
                .and_then(|node| node.attribute("full-path"))
                .ok_or(BookError::Invalid)?;
            // Validate the archive path before handing it to the EPUB parser.
            let (root, _) = resolve_href("", root)?;
            let package = archive
                .get_entry_as_str(root)
                .map_err(|_| BookError::Invalid)?;
            check_package(&package)?;
        }
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|_| BookError::Unreadable)?;
        let mut doc = EpubDoc::from_reader(reader).map_err(|_| BookError::Invalid)?;
        let title = doc
            .get_title()
            .filter(|title| !title.trim().is_empty())
            .ok_or(BookError::Invalid)?;
        let chapters = doc
            .spine
            .iter()
            .filter(|item| item.linear)
            .map(|item| {
                doc.resources
                    .get(&item.idref)
                    .ok_or(BookError::Invalid)
                    .and_then(resource_path)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if chapters.is_empty() {
            return Err(BookError::Invalid);
        }
        let toc = read_toc(&mut doc)?;
        validate_toc(&toc, &doc)?;
        Ok(Self {
            doc,
            title,
            chapters,
            toc,
        })
    }

    pub fn resource(&mut self, path: &str) -> Result<Resource, BookError> {
        let mime = self
            .doc
            .resources
            .values()
            .find(|resource| resource_path(resource).is_ok_and(|candidate| candidate == path))
            .map(|resource| resource.mime.clone())
            .ok_or(BookError::MissingResource)?;
        let bytes = self
            .doc
            .get_resource_by_path(path)
            .ok_or(BookError::MissingResource)?;
        Ok(Resource {
            path: path.to_string(),
            mime,
            bytes,
        })
    }

    pub fn chapter(&mut self, index: usize) -> Result<String, BookError> {
        let path = self
            .chapters
            .get(index)
            .ok_or(BookError::MissingResource)?
            .clone();
        let resource = self.resource(&path)?;
        if resource.mime != "application/xhtml+xml" {
            return Err(BookError::Unsupported);
        }
        let xhtml = String::from_utf8(resource.bytes).map_err(|_| BookError::Invalid)?;
        let xml = parse_xml(&xhtml)?;
        if !xml
            .root_element()
            .has_tag_name(("http://www.w3.org/1999/xhtml", "html"))
        {
            return Err(BookError::Invalid);
        }
        for node in xml.descendants().filter(|node| node.is_element()) {
            if node.attribute("style").is_some_and(vertical_css)
                || (node.has_tag_name("style") && vertical_stylesheet(&node_text(node), &xml))
            {
                return Err(BookError::Unsupported);
            }
            if node.has_tag_name("audio") || node.has_tag_name("video") {
                return Err(BookError::Unsupported);
            }
        }
        let css_paths = self
            .doc
            .resources
            .values()
            .filter(|resource| resource.mime == "text/css")
            .map(resource_path)
            .collect::<Result<Vec<_>, _>>()?;
        for path in css_paths {
            let css =
                String::from_utf8(self.resource(&path)?.bytes).map_err(|_| BookError::Invalid)?;
            if vertical_stylesheet(&css, &xml) {
                return Err(BookError::Unsupported);
            }
        }
        Ok(xhtml)
    }
}

fn css_without_comments(css: &str) -> String {
    let mut without_comments = String::new();
    let mut remaining = css;
    while let Some((before, comment)) = remaining.split_once("/*") {
        without_comments.push_str(before);
        let Some((_, after)) = comment.split_once("*/") else {
            remaining = "";
            break;
        };
        remaining = after;
    }
    without_comments.push_str(remaining);
    without_comments
}

fn vertical_css(css: &str) -> bool {
    vertical_declarations(&css_without_comments(css))
}

fn vertical_declarations(css: &str) -> bool {
    css.to_ascii_lowercase()
        .split([';', '{', '}'])
        .any(|declaration| {
            declaration
                .split_once(':')
                .is_some_and(|(property, value)| {
                    matches!(
                        property.trim(),
                        "writing-mode" | "-epub-writing-mode" | "-webkit-writing-mode"
                    ) && matches!(
                        value.trim().split(['!', ' ', '\t', '\n', '\r']).next(),
                        Some("vertical-rl" | "vertical-lr" | "tb-rl" | "tb-lr")
                    )
                })
        })
}

// Only exempt selectors whose absence can be established without a CSS engine.
// Unknown selectors, nesting and malformed rules retain the previous rejection.
fn unused_class_selector(selector: &str, xml: &Document<'_>) -> bool {
    let Some((tag, classes)) = selector.trim().split_once('.') else {
        return false;
    };
    let identifier = |value: &str| {
        !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !value.starts_with(|c: char| c.is_ascii_digit())
    };
    if (!tag.is_empty() && !identifier(tag)) || !classes.split('.').all(identifier) {
        return false;
    }
    !xml.descendants()
        .filter(|node| node.is_element())
        .any(|node| {
            (tag.is_empty() || node.tag_name().name() == tag)
                && classes.split('.').all(|class| {
                    node.attribute("class")
                        .unwrap_or("")
                        .split_whitespace()
                        .any(|value| value == class)
                })
        })
}

// Braces inside strings are not rule boundaries. Escaped identifiers are never
// exempted by unused_class_selector, so their interpretation stays conservative.
fn css_brace(css: &str, closing: bool) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 1usize;
    for (index, c) in css.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if c == active {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == '{' {
            if !closing {
                return Some(index);
            }
            depth += 1;
        } else if c == '}' && closing {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn vertical_stylesheet(css: &str, xml: &Document<'_>) -> bool {
    let css = css_without_comments(css);
    let mut remaining = css.as_str();
    while let Some(open) = css_brace(remaining, false) {
        let selector = remaining[..open].rsplit(';').next().unwrap_or("").trim();
        let body = &remaining[open + 1..];
        let Some(close) = css_brace(body, true) else {
            return vertical_declarations(remaining);
        };
        let declarations = &body[..close];
        if vertical_declarations(declarations)
            && (css_brace(declarations, false).is_some()
                || !selector
                    .split(',')
                    .all(|selector| unused_class_selector(selector, xml)))
        {
            return true;
        }
        remaining = &body[close + 1..];
    }
    vertical_declarations(remaining)
}

fn validate_toc<R: Read + Seek>(entries: &[TocEntry], doc: &EpubDoc<R>) -> Result<(), BookError> {
    for entry in entries {
        if entry.path.as_ref().is_some_and(|path| {
            !doc.resources
                .values()
                .any(|resource| resource_path(resource).is_ok_and(|candidate| candidate == *path))
        }) {
            return Err(BookError::Invalid);
        }
        validate_toc(&entry.children, doc)?;
    }
    Ok(())
}

fn check_package(package: &str) -> Result<(), BookError> {
    let xml = Document::parse(package).map_err(|_| BookError::Invalid)?;
    let root = xml.root_element();
    if !root.has_tag_name("package") {
        return Err(BookError::Invalid);
    }
    match root.attribute("version") {
        Some("2.0" | "3.0") => {}
        Some(_) => return Err(BookError::Unsupported),
        None => return Err(BookError::Invalid),
    }
    let manifest = child(root, "manifest").ok_or(BookError::Invalid)?;
    let spine = child(root, "spine").ok_or(BookError::Invalid)?;
    for item in spine.children().filter(|node| node.has_tag_name("itemref")) {
        let id = item.attribute("idref").ok_or(BookError::Invalid)?;
        if !manifest
            .children()
            .any(|node| node.has_tag_name("item") && node.attribute("id") == Some(id))
        {
            return Err(BookError::Invalid);
        }
    }
    for node in root.descendants().filter(|node| node.is_element()) {
        if node.has_tag_name("meta") {
            let value = node_text(node);
            if (node.attribute("property") == Some("rendition:layout") && value == "pre-paginated")
                || (node.attribute("property") == Some("rendition:writing-mode")
                    && value.starts_with("vertical"))
                || (node.attribute("name") == Some("fixed-layout")
                    && node.attribute("content") == Some("true"))
            {
                return Err(BookError::Unsupported);
            }
        }
        if node.attribute("properties").is_some_and(|value| {
            value
                .split_whitespace()
                .any(|property| property == "rendition:layout-pre-paginated")
        }) || node
            .attribute("media-type")
            .is_some_and(|mime| mime.starts_with("audio/") || mime.starts_with("video/"))
        {
            return Err(BookError::Unsupported);
        }
    }
    Ok(())
}

fn read_toc<R: Read + Seek>(doc: &mut EpubDoc<R>) -> Result<Vec<TocEntry>, BookError> {
    let is_nav = doc.version == EpubVersion::Version3_0;
    let id = if is_nav {
        doc.get_nav_id().ok_or(BookError::Invalid)?
    } else {
        let package = doc
            .get_resource_str_by_path(doc.root_file.clone())
            .ok_or(BookError::Invalid)?;
        let package = Document::parse(&package).map_err(|_| BookError::Invalid)?;
        package
            .descendants()
            .find(|node| node.has_tag_name("spine"))
            .and_then(|node| node.attribute("toc"))
            .ok_or(BookError::Invalid)?
            .to_string()
    };
    let resource = doc.resources.get(&id).ok_or(BookError::Invalid)?;
    let path = resource.path.to_string_lossy().replace('\\', "/");
    let xml = doc
        .get_resource_str_by_path(&path)
        .ok_or(BookError::MissingResource)?;
    let xml = parse_xml(&xml)?;
    let root = if is_nav {
        xml.descendants()
            .find(|node| {
                node.has_tag_name("nav")
                    && node
                        .attribute(("http://www.idpf.org/2007/ops", "type"))
                        .is_some_and(|value| value.split_whitespace().any(|token| token == "toc"))
            })
            .and_then(|node| child(node, "ol"))
    } else {
        xml.descendants().find(|node| node.has_tag_name("navMap"))
    }
    .ok_or(BookError::Invalid)?;
    parse_toc(root, &path, is_nav)
}

fn child<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|child| child.has_tag_name(tag))
}

fn node_text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|node| node.is_text())
        .filter_map(|node| node.text())
        .collect::<String>()
        .trim()
        .to_string()
}

fn parse_xml(xml: &str) -> Result<Document<'_>, BookError> {
    // EPUB 2 XHTML/NCX commonly declares a DTD. No external resolver is installed.
    Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|_| BookError::Invalid)
}

fn parse_toc(root: Node<'_, '_>, base: &str, is_nav: bool) -> Result<Vec<TocEntry>, BookError> {
    root.children()
        .filter(|node| node.has_tag_name(if is_nav { "li" } else { "navPoint" }))
        .map(|node| {
            let (label, href, children) = if is_nav {
                let label = child(node, "a")
                    .or_else(|| child(node, "span"))
                    .ok_or(BookError::Invalid)?;
                let href = if label.has_tag_name("a") {
                    Some(label.attribute("href").ok_or(BookError::Invalid)?)
                } else {
                    None
                };
                (node_text(label), href, child(node, "ol"))
            } else {
                let label = child(node, "navLabel")
                    .and_then(|node| child(node, "text"))
                    .ok_or(BookError::Invalid)?;
                let content = child(node, "content").ok_or(BookError::Invalid)?;
                (
                    node_text(label),
                    Some(content.attribute("src").ok_or(BookError::Invalid)?),
                    Some(node),
                )
            };
            let (path, fragment) = match href {
                Some(href) => {
                    let (path, fragment) = resolve_href(base, href)?;
                    (Some(path), fragment)
                }
                None => (None, None),
            };
            Ok(TocEntry {
                label,
                path,
                fragment,
                children: children
                    .map(|root| parse_toc(root, base, is_nav))
                    .transpose()?
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn resolve_href(base: &str, href: &str) -> Result<(String, Option<String>), BookError> {
    let (path, fragment) = href
        .split_once('#')
        .map_or((href, None), |(path, fragment)| {
            (path, Some(fragment.to_string()))
        });
    let path = decode_uri_path(path)?;
    if path.starts_with('/')
        || path.contains(':')
        || path.contains('\\')
        || path.contains('?')
        || path.contains('\0')
    {
        return Err(BookError::Invalid);
    }
    let combined = if path.is_empty() {
        base.to_string()
    } else {
        format!(
            "{}/{}",
            base.rsplit_once('/').map_or("", |(parent, _)| parent),
            path
        )
    };
    let mut parts = Vec::new();
    for part in combined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or(BookError::Invalid)?;
            }
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        return Err(BookError::Invalid);
    }
    Ok((parts.join("/"), fragment))
}

fn resource_path(resource: &ResourceItem) -> Result<String, BookError> {
    resolve_href("", &resource.path.to_string_lossy().replace('\\', "/")).map(|(path, _)| path)
}

fn decode_uri_path(path: &str) -> Result<String, BookError> {
    let mut decoded = Vec::with_capacity(path.len());
    let mut bytes = path.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes
                .next()
                .and_then(|byte| (byte as char).to_digit(16))
                .ok_or(BookError::Invalid)?;
            let low = bytes
                .next()
                .and_then(|byte| (byte as char).to_digit(16))
                .ok_or(BookError::Invalid)?;
            decoded.push((high * 16 + low) as u8);
        } else {
            decoded.push(byte);
        }
    }
    String::from_utf8(decoded).map_err(|_| BookError::Invalid)
}
