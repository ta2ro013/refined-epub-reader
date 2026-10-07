use crate::{
    book::{Book, BookError},
    book_test_support::Fixture,
    reader::{Reader, ReaderError, ReaderState, SelectionGate},
};
use std::io::Cursor;
use tauri::http::Request;

fn book(version: &'static str) -> Book<Cursor<Vec<u8>>> {
    Book::from_reader(Cursor::new(Fixture::new(version).bytes())).unwrap()
}

#[test]
fn selection_returns_book_information_and_first_chapter() {
    for version in ["2.0", "3.0"] {
        let mut reader = Reader::new();
        let info = reader.select(Some(Ok(book(version)))).unwrap().unwrap();
        assert!(!info.id.is_empty());
        assert_eq!(info.title, "読書テスト");
        assert_eq!(info.chapters, ["OPS/text/one.xhtml", "OPS/text/two.xhtml"]);
        assert_eq!(info.toc[0].label, "Second");
        assert!(reader.chapter(&info.id, 0).unwrap().contains("First"));
        assert!(reader.chapter(&info.id, 1).unwrap().contains("Second"));
    }
}

#[test]
fn cancellation_preserves_the_current_book_and_identifier() {
    let mut reader = Reader::new();
    assert!(reader.select(None).unwrap().is_none());
    let first = reader.select(Some(Ok(book("3.0")))).unwrap().unwrap();
    assert!(reader.select(None).unwrap().is_none());
    assert!(reader.chapter(&first.id, 0).unwrap().contains("First"));
}

#[test]
fn failed_first_chapter_does_not_replace_the_current_book() {
    let mut reader = Reader::new();
    let first = reader.select(Some(Ok(book("3.0")))).unwrap().unwrap();
    let mut fixture = Fixture::new("3.0");
    fixture.entries[0].1 = vec![0xff];
    let broken = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert!(matches!(
        reader.select(Some(Ok(broken))),
        Err(ReaderError::Invalid)
    ));
    assert!(reader.chapter(&first.id, 0).unwrap().contains("First"));
}

#[test]
fn failed_selection_keeps_the_current_book_and_error_category() {
    let mut reader = Reader::new();
    let first = reader.select(Some(Ok(book("3.0")))).unwrap().unwrap();
    for (error, expected) in [
        (BookError::Unreadable, ReaderError::Unreadable),
        (BookError::Invalid, ReaderError::Invalid),
        (BookError::Unsupported, ReaderError::Unsupported),
        (BookError::MissingResource, ReaderError::MissingResource),
    ] {
        assert!(matches!(reader.select(Some(Err(error))), Err(actual) if actual == expected));
        assert!(reader.chapter(&first.id, 0).unwrap().contains("First"));
    }
}

#[test]
fn replacement_invalidates_old_identifiers_and_returns_the_new_content() {
    let mut reader = Reader::new();
    let old = reader.select(Some(Ok(book("3.0")))).unwrap().unwrap();
    let mut fixture = Fixture::new("3.0");
    let xhtml =
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>Replacement book</body></html>"#;
    fixture.entries[0].1 = xhtml.as_bytes().to_vec();
    let new = reader
        .select(Some(Ok(
            Book::from_reader(Cursor::new(fixture.bytes())).unwrap()
        )))
        .unwrap()
        .unwrap();
    assert_ne!(old.id, new.id);
    assert_eq!(reader.chapter(&old.id, 0), Err(ReaderError::StaleBook));
    assert_eq!(reader.chapter(&new.id, 0).unwrap(), xhtml);
    assert_eq!(
        reader.chapter(&new.id, 99),
        Err(ReaderError::MissingResource)
    );
    assert_eq!(reader.chapter("unknown", 0), Err(ReaderError::StaleBook));
}

fn resource_fixture() -> Fixture {
    let mut fixture = Fixture::new("3.0");
    fixture.extra_manifest.push_str(r#"<item id="css" href="styles/%62ook.css" media-type="text/css"/><item id="image" href="images/picture.png" media-type="image/png"/>"#);
    fixture.entries.push((
        "OPS/styles/book.css".into(),
        b"body { color: navy; }".to_vec(),
    ));
    fixture.entries.push((
        "OPS/images/picture.png".into(),
        b"\x89PNG\r\n\x1a\n".to_vec(),
    ));
    fixture
}

#[test]
fn protocol_serves_css_and_image_bytes_on_native_and_windows_urls() {
    let mut reader = Reader::new();
    let info = reader
        .select(Some(Ok(Book::from_reader(Cursor::new(
            resource_fixture().bytes(),
        ))
        .unwrap())))
        .unwrap()
        .unwrap();
    for base in [
        "book://localhost",
        "http://book.localhost",
        "https://book.localhost",
    ] {
        for (path, mime, bytes) in [
            (
                "OPS/styles/book.css",
                "text/css",
                &b"body { color: navy; }"[..],
            ),
            (
                "OPS/images/picture.png",
                "image/png",
                &b"\x89PNG\r\n\x1a\n"[..],
            ),
        ] {
            let request = Request::builder()
                .uri(format!("{base}/{}/{path}", info.id))
                .body(Vec::new())
                .unwrap();
            let response = reader.respond("main", &request);
            assert_eq!(response.status(), 200);
            assert_eq!(response.headers()["content-type"], mime);
            assert_eq!(response.body(), bytes);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        }
    }
}

fn reader_with_resources() -> (Reader<Cursor<Vec<u8>>>, String) {
    let mut reader = Reader::new();
    let info = reader
        .select(Some(Ok(Book::from_reader(Cursor::new(
            resource_fixture().bytes(),
        ))
        .unwrap())))
        .unwrap()
        .unwrap();
    (reader, info.id)
}

#[test]
fn protocol_rejects_untrusted_webviews_and_origins() {
    let (mut reader, id) = reader_with_resources();
    let uri = format!("book://localhost/{id}/OPS/styles/book.css");
    let request = Request::builder().uri(&uri).body(Vec::new()).unwrap();
    assert_eq!(reader.respond("other", &request).status(), 403);
    for origin in [
        "https://example.com",
        "null",
        "http://localhost:9999",
        "http://tauri.localhost.evil",
    ] {
        let request = Request::builder()
            .uri(&uri)
            .header("Origin", origin)
            .body(Vec::new())
            .unwrap();
        let response = reader.respond("main", &request);
        assert_eq!(response.status(), 403, "{origin}");
        assert!(!response
            .headers()
            .contains_key("access-control-allow-origin"));
    }
}

#[test]
fn protocol_sets_cors_only_for_the_actual_app_origin() {
    let (mut reader, id) = reader_with_resources();
    for origin in [
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ] {
        let request = Request::builder()
            .uri(format!("book://localhost/{id}/OPS/styles/book.css"))
            .header("Origin", origin)
            .body(Vec::new())
            .unwrap();
        let response = reader.respond("main", &request);
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["access-control-allow-origin"], origin);
        assert_eq!(response.headers()["vary"], "Origin");
    }
}

#[test]
fn development_origin_is_not_enabled_in_release_builds() {
    let (mut reader, id) = reader_with_resources();
    let request = Request::builder()
        .uri(format!("book://localhost/{id}/OPS/styles/book.css"))
        .header("Origin", "http://localhost:1420")
        .body(Vec::new())
        .unwrap();
    let response = reader.respond("main", &request);
    assert_eq!(
        response.status(),
        if cfg!(debug_assertions) { 200 } else { 403 }
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .map(|value| value.to_str().unwrap()),
        if cfg!(debug_assertions) {
            Some("http://localhost:1420")
        } else {
            None
        }
    );
}

#[test]
fn protocol_checks_the_scheme_and_host() {
    let (mut reader, id) = reader_with_resources();
    for base in [
        "file://localhost",
        "book://other",
        "http://example.com",
        "https://book.localhost.evil",
        "http://book.localhost:1234",
    ] {
        let request = Request::builder()
            .uri(format!("{base}/{id}/OPS/styles/book.css"))
            .body(Vec::new())
            .unwrap();
        assert_eq!(reader.respond("main", &request).status(), 400, "{base}");
    }
}

#[test]
fn protocol_checks_methods_and_head_preserves_length_without_a_body() {
    let (mut reader, id) = reader_with_resources();
    let uri = format!("book://localhost/{id}/OPS/styles/book.css");
    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    let response = reader.respond("main", &request);
    assert_eq!(response.status(), 405);
    assert_eq!(response.headers()["allow"], "GET, HEAD");
    let request = Request::builder()
        .method("HEAD")
        .uri(&uri)
        .body(Vec::new())
        .unwrap();
    let response = reader.respond("main", &request);
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "text/css");
    assert_eq!(
        response.headers()["content-length"],
        b"body { color: navy; }".len().to_string()
    );
    assert!(response.body().is_empty());
}

#[test]
fn protocol_decodes_unicode_and_literal_percent_names_exactly_once() {
    let mut fixture = resource_fixture();
    fixture.extra_manifest.push_str(r#"<item id="unicode" href="images/caf%C3%A9.png" media-type="image/png"/><item id="percent" href="images/%2574wo.png" media-type="image/png"/><item id="other" href="images/two.png" media-type="image/png"/>"#);
    fixture.entries.extend([
        ("OPS/images/café.png".into(), b"unicode image".to_vec()),
        ("OPS/images/%74wo.png".into(), b"percent image".to_vec()),
        ("OPS/images/two.png".into(), b"wrong image".to_vec()),
    ]);
    let mut reader = Reader::new();
    let info = reader
        .select(Some(Ok(
            Book::from_reader(Cursor::new(fixture.bytes())).unwrap()
        )))
        .unwrap()
        .unwrap();
    for (path, expected) in [
        ("OPS/images/caf%C3%A9.png", &b"unicode image"[..]),
        ("OPS/images/%2574wo.png", &b"percent image"[..]),
    ] {
        let request = Request::builder()
            .uri(format!("book://localhost/{}/{path}", info.id))
            .body(Vec::new())
            .unwrap();
        let response = reader.respond("main", &request);
        assert_eq!(response.status(), 200, "{path}");
        assert_eq!(response.body(), expected);
    }
}

#[test]
fn protocol_rejects_escape_paths_and_malformed_encoding() {
    let (mut reader, id) = reader_with_resources();
    for path in [
        "../../outside.png",
        "%2e%2e/%2e%2e/outside.png",
        "OPS/%ZZ.png",
        "OPS/%ff.png",
        "OPS/%00.png",
        "OPS/%5Cpicture.png",
        "OPS/%2Fpicture.png",
        "OPS/a:b.png",
    ] {
        let request = Request::builder()
            .uri(format!("book://localhost/{id}/{path}"))
            .body(Vec::new())
            .unwrap();
        assert_eq!(reader.respond("main", &request).status(), 400, "{path}");
    }
}

#[test]
fn protocol_does_not_serve_active_documents_or_scripts() {
    let mut fixture = resource_fixture();
    fixture
        .extra_manifest
        .push_str(r#"<item id="script" href="script.js" media-type="application/javascript"/>"#);
    fixture
        .entries
        .push(("OPS/script.js".into(), b"alert('book script')".to_vec()));
    let mut reader = Reader::new();
    let info = reader
        .select(Some(Ok(
            Book::from_reader(Cursor::new(fixture.bytes())).unwrap()
        )))
        .unwrap()
        .unwrap();
    for path in ["OPS/text/one.xhtml", "OPS/script.js"] {
        let request = Request::builder()
            .uri(format!("book://localhost/{}/{path}", info.id))
            .body(Vec::new())
            .unwrap();
        assert_eq!(reader.respond("main", &request).status(), 415, "{path}");
    }
}

#[test]
fn protocol_does_not_replace_a_stale_request_with_the_new_books_resource() {
    let (mut reader, old) = reader_with_resources();
    let mut fixture = resource_fixture();
    fixture
        .entries
        .iter_mut()
        .find(|(path, _)| path.ends_with("book.css"))
        .unwrap()
        .1 = b"body { color: green; }".to_vec();
    let new = reader
        .select(Some(Ok(
            Book::from_reader(Cursor::new(fixture.bytes())).unwrap()
        )))
        .unwrap()
        .unwrap();
    let request = Request::builder()
        .uri(format!("book://localhost/{old}/OPS/styles/book.css"))
        .body(Vec::new())
        .unwrap();
    let response = reader.respond("main", &request);
    assert_eq!(response.status(), 404);
    assert_eq!(response.body(), br#""stale_book""#);
    let request = Request::builder()
        .uri(format!("book://localhost/{}/OPS/styles/book.css", new.id))
        .body(Vec::new())
        .unwrap();
    assert_eq!(
        reader.respond("main", &request).body(),
        b"body { color: green; }"
    );
}

#[test]
fn only_one_selection_can_run_and_the_gate_reopens_when_finished() {
    let gate = SelectionGate::default();
    let permit = gate.begin().unwrap();
    let other = gate.clone();
    assert!(matches!(other.begin(), Err(ReaderError::Busy)));
    drop(permit);
    let permit = other.begin().unwrap();
    assert!(matches!(gate.begin(), Err(ReaderError::Busy)));
    drop(permit);
    assert!(gate.begin().is_ok());
}

#[test]
fn resource_base_has_a_separate_book_identifier() {
    let mut reader = Reader::new();
    let first = reader.select(Some(Ok(book("3.0")))).unwrap().unwrap();
    let second = reader.select(Some(Ok(book("3.0")))).unwrap().unwrap();
    let base = if cfg!(target_os = "windows") {
        "http://book.localhost"
    } else {
        "book://localhost"
    };
    assert_eq!(second.resource_base, format!("{base}/{}/", second.id));
    assert_ne!(first.resource_base, second.resource_base);
}

#[test]
fn protocol_missing_resources_return_errors_instead_of_empty_success() {
    let (mut reader, id) = reader_with_resources();
    for path in ["OPS/missing.png", "META-INF/container.xml"] {
        let request = Request::builder()
            .uri(format!("book://localhost/{id}/{path}"))
            .body(Vec::new())
            .unwrap();
        let response = reader.respond("main", &request);
        assert_eq!(response.status(), 404);
        assert_eq!(response.body(), br#""missing_resource""#);
    }
    let mut fixture = resource_fixture();
    fixture
        .entries
        .retain(|(path, _)| !path.ends_with("picture.png"));
    let info = reader
        .select(Some(Ok(
            Book::from_reader(Cursor::new(fixture.bytes())).unwrap()
        )))
        .unwrap()
        .unwrap();
    let request = Request::builder()
        .uri(format!(
            "book://localhost/{}/OPS/images/picture.png",
            info.id
        ))
        .body(Vec::new())
        .unwrap();
    assert_eq!(reader.respond("main", &request).status(), 404);
}

struct LocalBook(std::path::PathBuf);

impl LocalBook {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "refined-reader-source-{}-{number}.epub",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for LocalBook {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn local_file_selection_distinguishes_cancellation_invalid_and_unreadable() {
    let state = ReaderState::default();
    let valid = LocalBook::new(&Fixture::new("3.0").bytes());
    let info = state.select_path(Some(&valid.0)).unwrap().unwrap();
    assert!(state.select_path(None).unwrap().is_none());
    let invalid = LocalBook::new(b"not a zip");
    assert!(matches!(
        state.select_path(Some(&invalid.0)),
        Err(ReaderError::Invalid)
    ));
    std::fs::remove_file(&invalid.0).unwrap();
    assert!(matches!(
        state.select_path(Some(&invalid.0)),
        Err(ReaderError::Unreadable)
    ));
    assert!(state.chapter(&info.id, 0).unwrap().contains("First"));
}

#[test]
fn registered_chapter_command_reads_the_selected_book_through_ipc() {
    use tauri::Manager;
    let app = crate::configure_builder(tauri::test::mock_builder())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let file = LocalBook::new(&Fixture::new("3.0").bytes());
    let info = app
        .state::<std::sync::Arc<ReaderState>>()
        .select_path(Some(&file.0))
        .unwrap()
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "read_chapter".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: webview.url().unwrap(),
            body: tauri::ipc::InvokeBody::Json(serde_json::json!({"bookId": info.id, "index": 1})),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .unwrap();
    assert!(response.deserialize::<String>().unwrap().contains("Second"));
}

fn ipc(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    tauri::test::get_ipc_response(
        webview,
        tauri::webview::InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: webview.url().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
}

#[test]
fn registered_selection_command_rejects_a_second_selection_before_opening_a_dialog() {
    use tauri::Manager;
    let app = crate::configure_builder(tauri::test::mock_builder())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let state = app.state::<std::sync::Arc<ReaderState>>();
    let _permit = state.selection.begin().unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        ipc(&webview, "select_book", serde_json::json!({})).unwrap_err(),
        serde_json::json!("busy")
    );
}

#[test]
fn reader_commands_are_not_available_from_other_windows() {
    let app = crate::configure_builder(tauri::test::mock_builder())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "other", Default::default())
        .build()
        .unwrap();
    for command in ["select_book", "read_chapter"] {
        assert_eq!(
            ipc(
                &webview,
                command,
                serde_json::json!({"bookId": "1", "index": 0})
            )
            .unwrap_err(),
            serde_json::json!("forbidden")
        );
    }
}

#[test]
fn chapter_errors_are_serialized_through_the_registered_command() {
    use tauri::Manager;
    let app = crate::configure_builder(tauri::test::mock_builder())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let file = LocalBook::new(&Fixture::new("3.0").bytes());
    let info = app
        .state::<std::sync::Arc<ReaderState>>()
        .select_path(Some(&file.0))
        .unwrap()
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (id, index, error) in [
        ("unknown", 0, "stale_book"),
        (info.id.as_str(), 99, "missing_resource"),
    ] {
        assert_eq!(
            ipc(
                &webview,
                "read_chapter",
                serde_json::json!({"bookId": id, "index": index})
            )
            .unwrap_err(),
            serde_json::json!(error)
        );
    }
}

#[test]
fn stylesheet_query_suffix_does_not_change_the_archive_resource() {
    let (mut reader, id) = reader_with_resources();
    let request = Request::builder()
        .uri(format!(
            "book://localhost/{id}/OPS/styles/book.css?version=1"
        ))
        .body(Vec::new())
        .unwrap();
    let response = reader.respond("main", &request);
    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), b"body { color: navy; }");
}

#[test]
fn head_errors_preserve_status_and_length_without_a_response_body() {
    let (mut reader, id) = reader_with_resources();
    let request = Request::builder()
        .method("HEAD")
        .uri(format!("book://localhost/{id}/OPS/styles/book.css"))
        .body(Vec::new())
        .unwrap();
    let response = reader.respond("other", &request);
    assert_eq!(response.status(), 403);
    assert!(response.body().is_empty());
    assert_eq!(
        response.headers()["content-length"],
        br#""forbidden""#.len().to_string()
    );
}
