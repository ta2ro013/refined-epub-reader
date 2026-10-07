use crate::{
    book::{Book, BookError},
    book_test_support::Fixture,
};
use std::io::Cursor;

#[test]
fn reads_title_and_spine_for_epub2_and_epub3() {
    for version in ["2.0", "3.0"] {
        let book = Book::from_reader(Cursor::new(Fixture::new(version).bytes())).unwrap();
        assert_eq!(book.title, "読書テスト");
        assert_eq!(book.chapters, ["OPS/text/one.xhtml", "OPS/text/two.xhtml"]);
    }
}

#[test]
fn keeps_nested_toc_order_relative_paths_and_fragments() {
    for version in ["2.0", "3.0"] {
        let book = Book::from_reader(Cursor::new(Fixture::new(version).bytes())).unwrap();
        assert_eq!(book.toc.len(), 1);
        let parent = &book.toc[0];
        assert_eq!(parent.label, "Second");
        assert_eq!(parent.path.as_deref(), Some("OPS/text/two.xhtml"));
        assert_eq!(parent.fragment.as_deref(), Some("part"));
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].label, "First");
        assert_eq!(
            parent.children[0].path.as_deref(),
            Some("OPS/text/one.xhtml")
        );
        assert_eq!(parent.children[0].fragment.as_deref(), Some("start"));
    }
}

#[test]
fn reads_xhtml_ruby_images_and_css_without_changing_content() {
    let mut fixture = Fixture::new("3.0");
    let xhtml = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>日本語</title><link rel=\"stylesheet\" href=\"../styles/book.css\"/></head><body><ruby>漢字<rt>かんじ</rt></ruby><img src=\"../images/picture.png\" alt=\"見本\"/></body></html>";
    fixture.entries[0].1 = xhtml.as_bytes().to_vec();
    let css = b"body { line-height: 1.8; }";
    fixture
        .extra_manifest
        .push_str(r#"<item id="css" href="styles/book.css" media-type="text/css"/>"#);
    fixture
        .entries
        .push(("OPS/styles/book.css".into(), css.to_vec()));
    for (name, mime, bytes) in [
        ("picture.png", "image/png", &b"\x89PNG\r\n\x1a\n"[..]),
        ("picture.jpg", "image/jpeg", &b"\xff\xd8\xff"[..]),
        ("picture.gif", "image/gif", &b"GIF89a"[..]),
    ] {
        fixture.extra_manifest.push_str(&format!(
            r#"<item id="{name}" href="images/{name}" media-type="{mime}"/>"#
        ));
        fixture
            .entries
            .push((format!("OPS/images/{name}"), bytes.to_vec()));
    }
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapter(0).unwrap(), xhtml);
    assert_eq!(book.resource("OPS/styles/book.css").unwrap().bytes, css);
    for (name, mime) in [
        ("picture.png", "image/png"),
        ("picture.jpg", "image/jpeg"),
        ("picture.gif", "image/gif"),
    ] {
        let resource = book.resource(&format!("OPS/images/{name}")).unwrap();
        assert_eq!(resource.mime, mime);
        assert_eq!(
            resource.bytes,
            fixture
                .entries
                .iter()
                .find(|(path, _)| path.ends_with(name))
                .unwrap()
                .1
        );
    }
}

#[test]
fn rejects_unknown_epub_versions() {
    let result = Book::from_reader(Cursor::new(Fixture::new("4.0").bytes()));
    assert!(matches!(result, Err(BookError::Unsupported)));
}

#[test]
fn rejects_fixed_layout_and_vertical_package_declarations() {
    for metadata in [
        r#"<meta property="rendition:layout">pre-paginated</meta>"#,
        r#"<meta property="rendition:writing-mode">vertical-rl</meta>"#,
        r#"<meta name="fixed-layout" content="true"/>"#,
    ] {
        let mut fixture = Fixture::new("3.0");
        fixture.metadata = metadata.into();
        assert!(
            matches!(
                Book::from_reader(Cursor::new(fixture.bytes())),
                Err(BookError::Unsupported)
            ),
            "{metadata}"
        );
    }
}

#[test]
fn rejects_encrypted_books() {
    let mut fixture = Fixture::new("3.0");
    fixture.entries.push(("META-INF/encryption.xml".into(), br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><EncryptedData xmlns="http://www.w3.org/2001/04/xmlenc#"><CipherData><CipherReference URI="OPS/text/one.xhtml"/></CipherData></EncryptedData></encryption>"#.to_vec()));
    assert!(matches!(
        Book::from_reader(Cursor::new(fixture.bytes())),
        Err(BookError::Unsupported)
    ));
}

#[test]
fn rejects_audio_and_video_resources() {
    for mime in ["audio/mpeg", "video/mp4"] {
        let mut fixture = Fixture::new("3.0");
        fixture.extra_manifest.push_str(&format!(
            r#"<item id="media" href="media.bin" media-type="{mime}"/>"#
        ));
        fixture.entries.push(("OPS/media.bin".into(), vec![0]));
        assert!(matches!(
            Book::from_reader(Cursor::new(fixture.bytes())),
            Err(BookError::Unsupported)
        ));
    }
}

#[test]
fn reports_invalid_archives_and_missing_resources() {
    assert!(matches!(
        Book::from_reader(Cursor::new(b"not a zip".to_vec())),
        Err(BookError::Invalid)
    ));
    let mut fixture = Fixture::new("3.0");
    fixture
        .entries
        .retain(|(path, _)| path != "OPS/text/one.xhtml");
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapter(0), Err(BookError::MissingResource));
    assert_eq!(book.chapter(99), Err(BookError::MissingResource));
    assert!(matches!(
        book.resource("OPS/missing.png"),
        Err(BookError::MissingResource)
    ));
    assert!(matches!(
        book.resource("META-INF/container.xml"),
        Err(BookError::MissingResource)
    ));
}

#[test]
fn opens_local_files_and_distinguishes_unreadable_from_invalid() {
    let path = std::env::temp_dir().join(format!(
        "refined-reader-{}-fixture.epub",
        std::process::id()
    ));
    std::fs::write(&path, Fixture::new("3.0").bytes()).unwrap();
    let result = Book::open(&path);
    std::fs::write(&path, b"not a zip").unwrap();
    let invalid = Book::open(&path);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(result.unwrap().title, "読書テスト");
    assert!(matches!(invalid, Err(BookError::Invalid)));
    assert!(matches!(Book::open(&path), Err(BookError::Unreadable)));
}

#[test]
fn rejects_vertical_css_and_inline_writing_modes() {
    for style in [
        "body { writing-mode: vertical-rl; }",
        "body { -epub-writing-mode: vertical-lr; }",
    ] {
        let mut fixture = Fixture::new("3.0");
        fixture
            .extra_manifest
            .push_str(r#"<item id="css" href="styles/book.css" media-type="text/css"/>"#);
        fixture
            .entries
            .push(("OPS/styles/book.css".into(), style.as_bytes().to_vec()));
        let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
        assert!(matches!(book.chapter(0), Err(BookError::Unsupported)));
    }
    let mut fixture = Fixture::new("3.0");
    fixture.entries[0].1 = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body style="writing-mode: vertical-rl"><p>Text</p></body></html>"#.to_vec();
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapter(0), Err(BookError::Unsupported));
}

#[test]
fn rejects_broken_xhtml_and_utf8_when_reading_a_chapter() {
    for bytes in [
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body>broken"#.to_vec(),
        vec![0xff],
    ] {
        let mut fixture = Fixture::new("3.0");
        fixture.entries[0].1 = bytes;
        let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
        assert_eq!(book.chapter(0), Err(BookError::Invalid));
    }
}

#[test]
fn rejects_broken_and_missing_navigation_documents() {
    for version in ["2.0", "3.0"] {
        let mut fixture = Fixture::new(version);
        let path = if version == "2.0" {
            "OPS/toc.ncx"
        } else {
            "OPS/navigation/nav.xhtml"
        };
        fixture
            .entries
            .iter_mut()
            .find(|(name, _)| name == path)
            .unwrap()
            .1 = b"<broken>".to_vec();
        assert!(matches!(
            Book::from_reader(Cursor::new(fixture.bytes())),
            Err(BookError::Invalid)
        ));
        fixture.entries.retain(|(name, _)| name != path);
        assert!(matches!(
            Book::from_reader(Cursor::new(fixture.bytes())),
            Err(BookError::MissingResource)
        ));
    }
}

#[test]
fn rejects_toc_links_that_escape_the_archive_or_have_no_resource() {
    for href in [
        "../../../outside.xhtml",
        "../text/missing.xhtml",
        "https://example.com/book.xhtml",
    ] {
        let mut fixture = Fixture::new("3.0");
        let nav = fixture
            .entries
            .iter_mut()
            .find(|(path, _)| path.ends_with("nav.xhtml"))
            .unwrap();
        nav.1 = String::from_utf8(nav.1.clone())
            .unwrap()
            .replace("../text/two.xhtml#part", href)
            .into_bytes();
        assert!(
            matches!(
                Book::from_reader(Cursor::new(fixture.bytes())),
                Err(BookError::Invalid)
            ),
            "{href}"
        );
    }
}

#[test]
fn preserves_navigation_groups_without_a_link() {
    let mut fixture = Fixture::new("3.0");
    let nav = fixture
        .entries
        .iter_mut()
        .find(|(path, _)| path.ends_with("nav.xhtml"))
        .unwrap();
    nav.1 = String::from_utf8(nav.1.clone())
        .unwrap()
        .replace(
            "<a href=\"../text/two.xhtml#part\">Second</a>",
            "<span>Part One</span>",
        )
        .into_bytes();
    let book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.toc[0].label, "Part One");
    assert_eq!(book.toc[0].path, None);
    assert_eq!(book.toc[0].children[0].label, "First");
}

#[test]
fn accepts_horizontal_css_and_standard_xhtml_doctype() {
    let mut fixture = Fixture::new("2.0");
    let mut xhtml = String::from_utf8(fixture.entries[0].1.clone()).unwrap();
    xhtml.insert_str(0, "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">");
    fixture.entries[0].1 = xhtml.as_bytes().to_vec();
    fixture
        .extra_manifest
        .push_str(r#"<item id="css" href="styles/book.css" media-type="text/css"/>"#);
    fixture.entries.push((
        "OPS/styles/book.css".into(),
        b"/* writing-mode: vertical-rl; */ body { writing-mode: horizontal-tb; }".to_vec(),
    ));
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapter(0).unwrap(), xhtml);
}

#[test]
fn rejects_vertical_css_with_compact_important() {
    let mut fixture = Fixture::new("3.0");
    fixture.entries[0].1 = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body style="writing-mode:vertical-rl!important"><p>Text</p></body></html>"#.to_vec();
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapter(0), Err(BookError::Unsupported));
}

#[test]
fn excludes_non_linear_spine_items() {
    let mut fixture = Fixture::new("3.0");
    fixture.spine_items = r#"<itemref idref="two" linear="no"/><itemref idref="one"/>"#.into();
    let book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapters, ["OPS/text/one.xhtml"]);
}

#[test]
fn rejects_invalid_spine_references_instead_of_silently_skipping_them() {
    for items in [
        r#"<itemref/><itemref idref="one"/>"#,
        r#"<itemref idref="missing"/><itemref idref="one"/>"#,
    ] {
        let mut fixture = Fixture::new("3.0");
        fixture.spine_items = items.into();
        assert!(
            matches!(
                Book::from_reader(Cursor::new(fixture.bytes())),
                Err(BookError::Invalid)
            ),
            "{items}"
        );
    }
}

#[test]
fn resolves_percent_encoded_toc_paths_to_manifest_resources() {
    let mut fixture = Fixture::new("3.0");
    let nav = fixture
        .entries
        .iter_mut()
        .find(|(path, _)| path.ends_with("nav.xhtml"))
        .unwrap();
    nav.1 = String::from_utf8(nav.1.clone())
        .unwrap()
        .replace("../text/two.xhtml#part", "../text/%74wo.xhtml#part")
        .into_bytes();
    let book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.toc[0].path.as_deref(), Some("OPS/text/two.xhtml"));
}

#[test]
fn reads_percent_encoded_unicode_manifest_paths() {
    let mut fixture = Fixture::new("3.0");
    fixture.extra_manifest.push_str(
        r#"<item id="unicode" href="text/caf%C3%A9.xhtml" media-type="application/xhtml+xml"/>"#,
    );
    fixture.spine_items = r#"<itemref idref="unicode"/>"#.into();
    let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Café</title></head><body><p>Unicode chapter: café</p></body></html>"#;
    fixture
        .entries
        .push(("OPS/text/café.xhtml".into(), xhtml.as_bytes().to_vec()));
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapters, ["OPS/text/café.xhtml"]);
    assert_eq!(book.chapter(0).unwrap(), xhtml);
}

#[test]
fn reads_a_chapter_with_a_percent_encoded_stylesheet_path() {
    let mut fixture = Fixture::new("3.0");
    fixture
        .extra_manifest
        .push_str(r#"<item id="css" href="styles/%62ook.css" media-type="text/css"/>"#);
    fixture.entries.push((
        "OPS/styles/book.css".into(),
        b"body { writing-mode: horizontal-tb; }".to_vec(),
    ));
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert!(book.chapter(0).unwrap().contains("First"));
    assert_eq!(
        book.resource("OPS/styles/book.css").unwrap().mime,
        "text/css"
    );
}

#[test]
fn rejects_vertical_css_with_a_percent_encoded_stylesheet_path() {
    let mut fixture = Fixture::new("3.0");
    fixture
        .extra_manifest
        .push_str(r#"<item id="css" href="styles/%62ook.css" media-type="text/css"/>"#);
    fixture.entries.push((
        "OPS/styles/book.css".into(),
        b"body { writing-mode: vertical-rl; }".to_vec(),
    ));
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapter(0), Err(BookError::Unsupported));
}

#[test]
fn accepts_unused_vertical_class_rules_in_shared_css() {
    for version in ["2.0", "3.0"] {
        for css in [
            ".hltr { writing-mode: horizontal-tb; } .vrtl { writing-mode: vertical-rl; }",
            "html.vrtl, body.vrtl { -epub-writing-mode: vertical-rl; }",
        ] {
            let mut fixture = Fixture::new(version);
            fixture
                .extra_manifest
                .push_str(r#"<item id="css" href="shared.css" media-type="text/css"/>"#);
            fixture
                .entries
                .push(("OPS/shared.css".into(), css.as_bytes().to_vec()));
            let expected = String::from_utf8(fixture.entries[0].1.clone()).unwrap();
            let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
            assert_eq!(book.chapter(0), Ok(expected), "{version}: {css}");
        }
    }
}

#[test]
fn accepts_unused_vertical_class_rules_in_style_elements() {
    let mut fixture = Fixture::new("3.0");
    let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>.vrtl { writing-mode: vertical-rl; }</style></head><body><p>Horizontal</p></body></html>"#;
    fixture.entries[0].1 = xhtml.as_bytes().to_vec();
    let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
    assert_eq!(book.chapter(0), Ok(xhtml.to_string()));
}

#[test]
fn keeps_rejecting_applied_and_complex_vertical_rules() {
    for (class, css) in [
        ("vrtl", ".vrtl { writing-mode: vertical-rl; }"),
        ("vrtl", "body.vrtl { -epub-writing-mode: vertical-lr; }"),
        ("vrtl extra", ".vrtl.extra { writing-mode: vertical-rl; }"),
        (
            "vrtl",
            ".vrtl { content: \"}\"; writing-mode: vertical-rl; }",
        ),
        ("", ".unused, body { writing-mode: vertical-rl; }"),
        ("", "body:not(.vrtl) { writing-mode: vertical-rl; }"),
        (
            "",
            "@media screen { .unused { writing-mode: vertical-rl; } }",
        ),
        ("", ".unused { & { writing-mode: vertical-rl; } }"),
        ("", ".unused { writing-mode: vertical-rl;"),
    ] {
        let mut fixture = Fixture::new("3.0");
        fixture.entries[0].1 = format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body class="{class}"><p>Text</p></body></html>"#).into_bytes();
        fixture
            .extra_manifest
            .push_str(r#"<item id="css" href="shared.css" media-type="text/css"/>"#);
        fixture
            .entries
            .push(("OPS/shared.css".into(), css.as_bytes().to_vec()));
        let mut book = Book::from_reader(Cursor::new(fixture.bytes())).unwrap();
        assert_eq!(
            book.chapter(0),
            Err(BookError::Unsupported),
            "{class}: {css}"
        );
    }
}
