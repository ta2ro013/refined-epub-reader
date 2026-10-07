use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

pub struct Fixture {
    pub version: &'static str,
    pub metadata: String,
    pub extra_manifest: String,
    pub spine_attributes: String,
    pub spine_items: String,
    pub entries: Vec<(String, Vec<u8>)>,
}

impl Fixture {
    pub fn new(version: &'static str) -> Self {
        let mut fixture = Self {
            version,
            metadata: String::new(),
            extra_manifest: String::new(),
            spine_attributes: String::new(),
            spine_items: r#"<itemref idref="one"/><itemref idref="two"/>"#.into(),
            entries: vec![
                ("OPS/text/one.xhtml".into(), br#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body><p id="start">First</p></body></html>"#.to_vec()),
                ("OPS/text/two.xhtml".into(), br#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Two</title></head><body><p id="part">Second</p></body></html>"#.to_vec()),
            ],
        };
        if version == "2.0" {
            fixture.extra_manifest =
                r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#.into();
            fixture.spine_attributes = r#"toc="ncx""#.into();
            fixture.entries.push(("OPS/toc.ncx".into(), br#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint id="two" playOrder="1"><navLabel><text>Second</text></navLabel><content src="text/two.xhtml#part"/><navPoint id="one" playOrder="2"><navLabel><text>First</text></navLabel><content src="text/one.xhtml#start"/></navPoint></navPoint></navMap></ncx>"#.to_vec()));
        } else {
            fixture.extra_manifest = r#"<item id="nav" href="navigation/nav.xhtml" properties="nav" media-type="application/xhtml+xml"/>"#.into();
            fixture.entries.push(("OPS/navigation/nav.xhtml".into(), br#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="../text/two.xhtml#part">Second</a><ol><li><a href="../text/one.xhtml#start">First</a></li></ol></li></ol></nav></body></html>"#.to_vec()));
        }
        fixture
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let mut write = |path: &str, content: &[u8]| {
            zip.start_file(path, options).unwrap();
            zip.write_all(content).unwrap();
        };
        write("mimetype", b"application/epub+zip");
        write("META-INF/container.xml", br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#);
        let opf = format!(
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="{}" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">test-book</dc:identifier><dc:title>読書テスト</dc:title><dc:language>ja</dc:language>{}</metadata><manifest><item id="one" href="text/one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="text/two.xhtml" media-type="application/xhtml+xml"/>{}</manifest><spine {}>{}</spine></package>"#,
            self.version,
            self.metadata,
            self.extra_manifest,
            self.spine_attributes,
            self.spine_items
        );
        write("OPS/book.opf", opf.as_bytes());
        for (path, bytes) in &self.entries {
            write(path, bytes);
        }
        zip.finish().unwrap().into_inner()
    }
}
