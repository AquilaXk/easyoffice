//! `audit_security` looks for XML declarations only in XML parts. Text, Markdown, CSV and PDF
//! bodies that quote an HTML document type line are ordinary content and open normally.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{base64_codec, PdfDocumentModel, PdfPageModel, PdfSerializer};

const HTML_DOCTYPE: &str = "<!DOCTYPE html>\n<html><body>예시</body></html>\n";

fn audit(file_name: &str, bytes: &[u8]) -> Result<String, easyoffice_formats::EasyOfficeError> {
    let payload = format!(
        r#"{{"fileName":"{file_name}","containerBase64":"{}"}}"#,
        base64_codec::encode(bytes)
    );
    EasyOfficeIpcRouter::dispatch("audit_security", &payload)
}

fn assert_safe(file_name: &str, bytes: &[u8]) {
    let verdict = audit(file_name, bytes).unwrap_or_else(|e| panic!("{file_name}: {e:?}"));
    assert!(verdict.contains("\"verdict\":\"SAFE\""), "{verdict}");
}

#[test]
fn text_formats_that_quote_a_doctype_open() {
    let md = format!("# 예제\n\n```html\n{HTML_DOCTYPE}```\n");
    assert_safe("note.md", md.as_bytes());
    assert_safe("note.txt", HTML_DOCTYPE.as_bytes());
    assert_safe(
        "table.csv",
        format!("a,b\n\"{HTML_DOCTYPE}\",2\n").as_bytes(),
    );
    assert_safe("table.tsv", format!("a\tb\n{HTML_DOCTYPE}\t2\n").as_bytes());
    assert_safe(
        "doc.rtf",
        br"{\rtf1\ansi <!DOCTYPE html> <!ENTITY x> text\par}",
    );
}

#[test]
fn pdf_pages_and_watermark_that_quote_a_doctype_open() {
    let model = PdfDocumentModel {
        title: "예제".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: HTML_DOCTYPE.to_string(),
            approval_stamp: None,
        }],
        watermark_text: Some("<!ENTITY x \"y\">".to_string()),
    };
    assert_safe("page.pdf", &PdfSerializer::serialize(&model));
}

#[test]
fn xml_parts_with_a_real_declaration_are_still_refused() {
    let json = |xml: &str| {
        format!(
            r#"{{"xml":{xml:?},"entries":[{{"name":"a.xml","compressedSize":10,"uncompressedSize":20}}]}}"#
        )
    };
    for xml in [
        "<!DOCTYPE r [<!ENTITY e SYSTEM \"file:///etc/passwd\">]><r>&e;</r>",
        "<!-- ok --><!doctype r><r/>",
        "<r><!  ENTITY e \"v\"></r>",
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("audit_security", &json(xml)).is_err(),
            "{xml}"
        );
    }
    for xml in [
        "<r><!-- <!DOCTYPE html> --></r>",
        "<r><![CDATA[<!DOCTYPE html>]]></r>",
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("audit_security", &json(xml)).is_ok(),
            "{xml}"
        );
    }
}
