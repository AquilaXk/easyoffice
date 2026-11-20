//! Tests of what the result of an edit keeps of the original besides the pages, and of the
//! warnings for what it cannot keep (PD-266). The files are written by hand so that every entry
//! under test is exactly the one the test names.

use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::hand_written_with_xref;
use crate::pdf_edit_session::EditSession;
use crate::pdf_edit_wire::Built;
use std::sync::Arc;
use usvg::fontdb::Database;

fn engine() -> InProcessEngine {
    InProcessEngine::new(Arc::new(Database::new()))
}

/// Opens `file` for editing and writes all its pages again.
pub(crate) fn rebuilt(file: Vec<u8>) -> Built {
    let engine = engine();
    let session = EditSession::open(file, &engine).expect("the file opens");
    session.render(None, &engine).expect("the file is written")
}

/// A one-page document. `catalog` and `annots` are added to the catalog and to the page's
/// `/Annots`; `more` are the objects from number 5 on.
pub(crate) fn one_page(catalog: &str, annots: &str, more: &[String]) -> Vec<u8> {
    one_page_with(catalog, "", "0 0 1 rg 10 10 80 80 re f", annots, more)
}

/// The same with the page's `/Resources` entry (a whole `/Resources << ... >>` or nothing) and its
/// content stream.
pub(crate) fn one_page_with(
    catalog: &str,
    resources: &str,
    content: &str,
    annots: &str,
    more: &[String],
) -> Vec<u8> {
    let mut objects = vec![
        (1, format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>")),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R \
                 {resources} /Annots [{annots}] >>"
            ),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ),
    ];
    objects.extend((5u32..).zip(more.iter().cloned()));
    hand_written_with_xref(&objects, "")
}

fn square(rect: &str) -> String {
    format!("<< /Type /Annot /Subtype /Square /Rect [{rect}] /C [1 0 0] >>")
}

#[test]
fn annotations_the_result_cannot_write_are_reported_by_kind() {
    let more = vec![
        square("10 10 50 50"),
        "<< /Type /Annot /Subtype /Line /Rect [10 60 80 90] /L [10 60 80 90] >>".to_string(),
        square("100 100 150 150"),
        "<< /Type /Annot /Subtype /Link /Rect [0 0 20 20] /A << /S /URI /URI (https://example.org/) >> >>"
            .to_string(),
    ];
    let built = rebuilt(one_page("", "5 0 R 6 0 R 7 0 R 8 0 R", &more));
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 3개는 저장하지 못했습니다 (Square 2, Line 1)."]
    );
}

#[test]
fn a_file_without_losses_has_no_warnings() {
    let more = vec![
        "<< /Type /Annot /Subtype /Link /Rect [0 0 20 20] /A << /S /URI /URI (https://example.org/) >> >>"
            .to_string(),
    ];
    assert_eq!(
        rebuilt(one_page("", "5 0 R", &more)).warnings,
        Vec::<String>::new()
    );
}

#[test]
fn catalog_entries_that_are_not_written_are_reported() {
    let more = vec!["<< /Type /StructTreeRoot >>".to_string()];
    let catalog =
        "/StructTreeRoot 5 0 R /Names << /JavaScript << /Names [] >> >> /Collection << /View /T >>";
    let built = rebuilt(one_page(catalog, "", &more));
    assert_eq!(
        built.warnings,
        [
            "태그 구조 트리(접근성 정보)는 저장하지 않았습니다.",
            "문서 스크립트는 보안을 위해 저장하지 않았습니다.",
            "PDF 포트폴리오 구성은 저장하지 않았습니다."
        ]
    );
}

#[test]
fn a_signature_field_is_reported_and_not_written() {
    let more = vec![
        "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sign) /Rect [10 10 90 40] >>".to_string(),
    ];
    let built = rebuilt(one_page(
        "/AcroForm << /Fields [5 0 R] /SigFlags 3 >>",
        "5 0 R",
        &more,
    ));
    assert_eq!(
        built.warnings,
        ["전자서명 등 지원하지 않는 양식 필드 1개는 저장하지 못했습니다."]
    );
}

/// A file of `pages` pages, each with a free text, an ink and a stamp annotation, two layers (one
/// hidden), page labels and ten attachments: what the carry reads and writes, at the size of a
/// long report.
fn bench_file(pages: usize) -> Vec<u8> {
    let first_page = 3u32;
    let first_extra = first_page + pages as u32;
    let kids: Vec<String> = (0..pages)
        .map(|n| format!("{} 0 R", first_page + n as u32))
        .collect();
    // Objects after the pages: 2 layers, 1 appearance form, 1 embedded file, 10 file specs, then
    // three annotations for each page.
    let (layer_on, layer_off, form, data) = (
        first_extra,
        first_extra + 1,
        first_extra + 2,
        first_extra + 3,
    );
    let specs: Vec<u32> = (0..10).map(|n| first_extra + 4 + n).collect();
    let annots_at = first_extra + 14;
    let names: String = specs
        .iter()
        .enumerate()
        .map(|(n, s)| format!("(f{n:02}.txt) {s} 0 R "))
        .collect();
    let catalog = format!(
        "/OCProperties << /OCGs [{layer_on} 0 R {layer_off} 0 R] /D << /OFF [{layer_off} 0 R] >> >> \
         /PageLabels << /Nums [0 << /S /r >> 4 << /S /D >>] >> \
         /Names << /EmbeddedFiles << /Names [{names}] >> >>"
    );
    let mut objects = vec![
        (1, format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>")),
        (
            2,
            format!(
                "<< /Type /Pages /Kids [{}] /Count {pages} >>",
                kids.join(" ")
            ),
        ),
    ];
    for n in 0..pages as u32 {
        let at = annots_at + n * 3;
        objects.push((
            first_page + n,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Annots [{at} 0 R {} 0 R {} 0 R] >>",
                at + 1,
                at + 2
            ),
        ));
    }
    let content = "0 1 0 rg 0 0 100 60 re f";
    objects.push((layer_on, "<< /Type /OCG /Name (On) >>".to_string()));
    objects.push((layer_off, "<< /Type /OCG /Name (Off) >>".to_string()));
    objects.push((
        form,
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 60] /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ));
    objects.push((
        data,
        "<< /Type /EmbeddedFile /Length 5 >>\nstream\nhello\nendstream".to_string(),
    ));
    for spec in &specs {
        objects.push((
            *spec,
            format!("<< /Type /Filespec /F (a) /EF << /F {data} 0 R >> >>"),
        ));
    }
    for _ in 0..pages {
        for kind in ["FreeText", "Ink", "Stamp"] {
            let extra = if kind == "Ink" {
                "/InkList [[10 10 50 50 90 10]]"
            } else {
                ""
            };
            objects.push((
                objects.len() as u32 + 1,
                format!("<< /Type /Annot /Subtype /{kind} /Rect [10 10 110 70] {extra} /AP << /N {form} 0 R >> /OC {layer_on} 0 R >>"),
            ));
        }
    }
    hand_written_with_xref(&objects, "")
}

#[test]
fn bench_pd266_carry_cost() {
    let pages = 300;
    let file = bench_file(pages);
    let engine = engine();
    let session = EditSession::open(file.clone(), &engine).expect("the file opens");
    let built = session.render(None, &engine).expect("warm-up");
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);

    let parsed: std::collections::BTreeMap<usize, hayro_syntax::Pdf> = [(
        0usize,
        crate::pdf_edit_inspect::open_pdf(&Arc::new(file)).unwrap(),
    )]
    .into();
    let plan: Vec<crate::pdf_edit_wire::PagePlan> = (0..pages)
        .map(|index| crate::pdf_edit_wire::PagePlan {
            source: 0,
            index,
            turn: 0,
        })
        .collect();
    let deadline = crate::pdf_edit_fixtures::deadline();
    let runs = 10u32;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        crate::pdf_edit_carry::Carry::plan(&parsed, &plan, &deadline).unwrap();
    }
    let planning = started.elapsed() / runs;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        session.render(None, &engine).unwrap();
    }
    let whole = started.elapsed() / runs;
    eprintln!(
        "[PD-266 bench] {pages} pages x (3 annotations + layers + labels + attachments): scan avg={planning:?}, whole build avg={whole:?} ({} B)",
        built.bytes.len()
    );
}
