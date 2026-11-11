//! A host with a page renderer (the desktop shell) takes over the PDF of a rich document: the
//! engine hands it the document as HWPX and the page settings, and keeps its own PDF as the
//! fallback when the renderer fails or when there is none (the `--serve` preview).

use easyoffice_app::{EasyOfficeIpcRouter, RichPdfRenderer};
use easyoffice_formats::{
    DocumentFormat, PdfSerializer, PdfWriteOptions, RichBlock, RichDocument, RichParagraph,
    RichRun, UniversalFormatConverter,
};
use std::cell::RefCell;

const DOCUMENT: &str = r#""blocks":[{"type":"paragraph","runs":[{"text":"본문 문장","bold":true}]},{"type":"table","rows":[[{"text":"가"},{"text":"나"}]]}]"#;

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn pdf_of(response: &str) -> Vec<u8> {
    EasyOfficeIpcRouter::decode_base64(json_str_field(response, "convertedBase64")).unwrap()
}

/// A renderer that records what it was given and answers with a PDF whose text says so.
/// What a renderer was asked to draw: the HWPX and the page settings.
type Request = (Vec<u8>, PdfWriteOptions);

struct Recorder {
    seen: RefCell<Vec<Request>>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            seen: RefCell::new(Vec::new()),
        }
    }

    fn render(&self, hwpx: &[u8], options: &PdfWriteOptions) -> Result<Vec<u8>, String> {
        self.seen
            .borrow_mut()
            .push((hwpx.to_vec(), options.clone()));
        let doc = RichDocument {
            title: "렌더러".into(),
            blocks: vec![RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("rendered by the host")],
                ..RichParagraph::default()
            })],
            official_agency: None,
        };
        Ok(UniversalFormatConverter::encode_rich_pdf(&doc, &PdfWriteOptions::default()).0)
    }
}

fn dispatch(payload: &str, renderer: &RichPdfRenderer<'_>) -> String {
    EasyOfficeIpcRouter::dispatch_with_pdf_renderer("convert_document_format", payload, renderer)
        .unwrap()
}

fn text_of(pdf: &[u8]) -> String {
    PdfSerializer::parse(pdf)
        .unwrap()
        .pages
        .iter()
        .map(|page| page.body_text.clone())
        .collect()
}

#[test]
fn a_document_saved_as_pdf_is_rendered_by_the_host_from_its_hwpx() {
    let host = Recorder::new();
    let payload = format!(
        r#"{{"sourceFormat":"docx","targetFormat":"pdf","title":"보고서",{DOCUMENT},"pdf":{{"rotationDegrees":90,"watermark":"대외비","stamp":"승인필"}}}}"#
    );
    let response = dispatch(&payload, &|hwpx, options| host.render(hwpx, options));

    assert_eq!(text_of(&pdf_of(&response)), "rendered by the host");
    assert!(
        !response.contains("기본 PDF"),
        "no fallback notice: {response}"
    );
    let seen = host.seen.borrow();
    assert_eq!(seen.len(), 1);
    let (hwpx, options) = &seen[0];
    assert_eq!(&hwpx[..2], b"PK", "an HWPX package");
    let decoded = UniversalFormatConverter::decode_rich(hwpx, Some("hwpx")).unwrap();
    assert_eq!(decoded.title, "보고서");
    assert_eq!(
        options,
        &PdfWriteOptions {
            rotation_degrees: 90,
            watermark: Some("대외비".into()),
            approval_stamp: Some("승인필".into()),
            highlight: None,
        }
    );
}

#[test]
fn a_failing_host_falls_back_to_the_engine_pdf_with_a_notice() {
    let payload =
        format!(r#"{{"sourceFormat":"docx","targetFormat":"pdf","title":"보고서",{DOCUMENT}}}"#);
    let response = dispatch(&payload, &|_, _| Err("글꼴을 읽지 못했습니다".to_string()));

    assert!(text_of(&pdf_of(&response)).contains("본문 문장"));
    assert!(response.contains("기본 PDF"), "{response}");
    assert!(response.contains("글꼴을 읽지 못했습니다"), "{response}");
}

#[test]
fn without_a_host_the_engine_pdf_is_written_unchanged() {
    let payload =
        format!(r#"{{"sourceFormat":"docx","targetFormat":"pdf","title":"보고서",{DOCUMENT}}}"#);
    let plain = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let host = Recorder::new();
    let hosted = dispatch(&payload, &|h, o| host.render(h, o));
    assert_ne!(pdf_of(&plain), pdf_of(&hosted));
    assert!(text_of(&pdf_of(&plain)).contains("본문 문장"));
    assert!(!plain.contains("기본 PDF"), "{plain}");
}

#[test]
fn only_documents_that_do_not_come_from_a_pdf_use_the_host() {
    let host = Recorder::new();
    let render = |h: &[u8], o: &PdfWriteOptions| host.render(h, o);
    // The PDF studio saves its own pages: the engine keeps the page markers it reads back.
    let studio =
        format!(r#"{{"sourceFormat":"pdf","targetFormat":"pdf","title":"결재",{DOCUMENT}}}"#);
    dispatch(&studio, &render);
    // Other targets and non-document content never reach the host.
    let docx =
        format!(r#"{{"sourceFormat":"docx","targetFormat":"odt","title":"문서",{DOCUMENT}}}"#);
    dispatch(&docx, &render);
    let sheet = r#"{"sourceFormat":"xlsx","targetFormat":"pdf","title":"시트","sheets":[{"name":"S","cells":[{"ref":"A1","raw":"1"}]}]}"#;
    dispatch(sheet, &render);
    assert!(host.seen.borrow().is_empty());
}

/// A highlight is a PDF annotation object the engine writes (with notes for a phrase that is not
/// in the text); the host draws pages, not annotations, so a request with one stays with the engine.
#[test]
fn a_highlight_request_stays_with_the_engine_writer() {
    let host = Recorder::new();
    let payload = format!(
        r##"{{"sourceFormat":"docx","targetFormat":"pdf","title":"보고서",{DOCUMENT},"pdf":{{"highlight":{{"text":"본문","kind":"highlight","color":"#FFFF00"}}}}}}"##
    );
    let response = dispatch(&payload, &|h, o| host.render(h, o));
    assert!(host.seen.borrow().is_empty());
    let pdf = pdf_of(&response);
    assert!(
        String::from_utf8_lossy(&pdf).contains("/Subtype /Highlight"),
        "the engine's annotation"
    );
}

#[test]
fn a_source_file_saved_as_pdf_is_rendered_by_the_host() {
    let docx = UniversalFormatConverter::encode_rich(
        &RichDocument {
            title: "파일".into(),
            blocks: vec![RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("원본 문장")],
                ..RichParagraph::default()
            })],
            official_agency: None,
        },
        DocumentFormat::Docx,
        None,
    )
    .unwrap();
    let payload = format!(
        r#"{{"fileName":"파일.docx","sourceBase64":"{}","targetFormat":"pdf"}}"#,
        EasyOfficeIpcRouter::encode_base64(&docx)
    );
    let host = Recorder::new();
    let response = dispatch(&payload, &|h, o| host.render(h, o));
    assert_eq!(text_of(&pdf_of(&response)), "rendered by the host");
    assert_eq!(host.seen.borrow().len(), 1);
}
