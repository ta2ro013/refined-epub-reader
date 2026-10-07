pub mod book;
pub mod reader;

#[cfg(test)]
mod book_test_support;
#[cfg(test)]
mod book_tests;
#[cfg(test)]
mod reader_tests;

use reader::{BookInfo, ReaderError, ReaderState};
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn select_book<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    window: tauri::WebviewWindow<R>,
) -> Result<Option<BookInfo>, ReaderError> {
    if !reader::allowed_window(&window) {
        return Err(ReaderError::Forbidden);
    }
    let state = Arc::clone(app.state::<Arc<ReaderState>>().inner());
    let permit = state.selection.begin()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let dialog = app.dialog().file().add_filter("EPUB", &["epub"]);
        #[cfg(desktop)]
        let dialog = dialog.set_parent(&window);
        let path = dialog
            .blocking_pick_file()
            .map(|file| file.into_path())
            .transpose()
            .map_err(|_| ReaderError::Unreadable)?;
        state.select_path(path.as_deref())
    })
    .await
    .map_err(|_| ReaderError::Internal)?
}

#[tauri::command]
async fn read_chapter<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    state: tauri::State<'_, Arc<ReaderState>>,
    book_id: String,
    index: usize,
) -> Result<String, ReaderError> {
    if !reader::allowed_window(&window) {
        return Err(ReaderError::Forbidden);
    }
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || state.chapter(&book_id, index))
        .await
        .map_err(|_| ReaderError::Internal)?
}

pub(crate) fn configure_builder<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
) -> tauri::Builder<R> {
    builder
        .manage(Arc::new(ReaderState::default()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("book", |context, request, responder| {
            let state = Arc::clone(context.app_handle().state::<Arc<ReaderState>>().inner());
            let webview = context.webview_label().to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(state.respond(&webview, &request))
            });
        })
        .invoke_handler(tauri::generate_handler![greet, select_book, read_chapter])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_builder(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
