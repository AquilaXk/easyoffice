//! Measurement harness for `docs/performance.md`, not a check. Every case is one `#[ignore]` test
//! that reads its case name and input from the environment and prints one `BENCH ...` line, so the
//! driver (`scripts/bench-run.mjs`) can run it as a fresh process N times under `/usr/bin/time -l`
//! and take the median time and the peak RSS.
//!
//! `EASYOFFICE_BENCH_CASE` is one of `save_verify`, `preview_embedded`, `preview_render`,
//! `thumb_embed`, `pdf_doc`, `pdf_svgs`, `doc_pdf_engine`, `doc_pdf_shell` (a DOCX/ODT/RTF/MD/TXT file as a PDF by the engine's writer, and by HWPX + rhwp layout, PD-164); `EASYOFFICE_BENCH_INPUT` is a document, or a folder of page SVGs for `pdf_svgs`. A name with the
//! prefix `ref:` (`ref:save_verify`, `ref:preview`, `ref:pdf`, `ref:pdf_svgs`) runs the path the
//! reference app takes for the same job with the same rhwp (parse and convert to the editable form,
//! first-page SVG, `svgs_to_pdf_with_options`), printed as `tool=ref-repro`.
//! `EASYOFFICE_BENCH_PDF_OUT` (a file path) keeps the PDF of the `pdf_*` cases.
//! `EASYOFFICE_BENCH_PDF_SETTINGS` (`rotation,watermark,stamp`, each optional) sets the page settings of the `doc_pdf_*` cases.
//! `EASYOFFICE_BENCH_REPEAT` repeats the timed part inside one process (for a sampling profiler; the
//! line shows the last run).

use crate::doc_pdf::hwpx_to_pdf;
use crate::pdf_export::{verify_exported_pdf, PdfAssembler};
use crate::pdf_font_fallbacks::add_font_fallbacks;
use crate::pdf_fonts::shared_font_db;
use crate::preview::extract_thumbnail;
use crate::save_thumbnail::{render_png_sized, with_first_page};
use crate::save_verify::verify_document_bytes;
use easyoffice_formats::{
    DecodedContent, DocumentFormat, PdfWriteOptions, RichDocument, UniversalFormatConverter,
};
use rhwp::DocumentCore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use usvg::fontdb::Database;

const SYNTHETIC_PAGES: usize = 300;
/// Longer side of a start-window card picture (`preview/render.rs` `CARD_SIDE_PX`).
const CARD_SIDE_PX: u32 = 480;

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn bundled_font_dirs() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/rhwp/ttfs/opensource");
    if dir.is_dir() {
        vec![dir]
    } else {
        Vec::new()
    }
}

/// The reference app's `editable_core_from_bytes`: parse, then convert to the editable form.
fn editable_core(bytes: &[u8]) -> DocumentCore {
    let mut core = DocumentCore::from_bytes(bytes).expect("parse");
    core.convert_to_editable_native().expect("convert");
    core
}

/// The reference app's PDF step: rhwp's own page converter with the bundled face as every fallback.
fn rhwp_pdf(pages: &[String]) -> Vec<u8> {
    let options = rhwp::renderer::pdf::PdfExportOptions {
        fallback_serif: "Noto Sans KR".to_string(),
        fallback_sans: "Noto Sans KR".to_string(),
        fallback_mono: "Noto Sans KR".to_string(),
        font_paths: bundled_font_dirs(),
        embed_text: true,
        ..Default::default()
    };
    rhwp::renderer::pdf::svgs_to_pdf_with_options(pages, &options).expect("pdf")
}

/// The reference app writes the PDF through a temporary file in the target folder.
fn write_beside(input: &Path, pdf: &[u8]) -> f64 {
    let t = Instant::now();
    let dir = std::env::temp_dir().join(format!("easyoffice-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join(input.file_name().unwrap()), pdf).expect("write");
    std::fs::remove_dir_all(&dir).expect("cleanup");
    ms(t)
}

struct Line {
    tool: &'static str,
    case: String,
    input: String,
    total: f64,
    phases: Vec<(&'static str, f64)>,
    out_bytes: usize,
    pages: usize,
}

impl Line {
    fn print(&self) {
        let phases: Vec<String> = self
            .phases
            .iter()
            .map(|(name, value)| format!("{name}={value:.1}"))
            .collect();
        println!(
            "BENCH tool={} case={} input={} total_ms={:.1} out_bytes={} pages={} phases={}",
            self.tool,
            self.case,
            self.input,
            self.total,
            self.out_bytes,
            self.pages,
            phases.join(",")
        );
    }
}

fn page_svgs(dir: &Path) -> Vec<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("svg folder")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("svg"))
        .collect()
}

fn with_fallbacks(pages: &[String]) -> Vec<String> {
    pages
        .iter()
        .map(|svg| add_font_fallbacks(svg).into_owned())
        .collect()
}

/// The rich document a DOCX/ODT/RTF/MD/TXT file reads as.
fn rich_document(input: &Path, bytes: &[u8]) -> RichDocument {
    let name = input.file_name().unwrap().to_string_lossy();
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(&name)).expect("decode");
    match decoded.content {
        DecodedContent::Document(doc) => doc,
        _ => panic!("{name} is no document"),
    }
}

/// The page settings named by `EASYOFFICE_BENCH_PDF_SETTINGS`, none when it is unset.
fn pdf_settings() -> PdfWriteOptions {
    let value = std::env::var("EASYOFFICE_BENCH_PDF_SETTINGS").unwrap_or_default();
    let mut parts = value.split(',');
    let mut label = || {
        parts
            .next()
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    PdfWriteOptions {
        rotation_degrees: label().and_then(|text| text.parse().ok()).unwrap_or(0),
        watermark: label(),
        approval_stamp: label(),
    }
}

/// Keeps the PDF of a `doc_pdf_*` case when `EASYOFFICE_BENCH_PDF_OUT` names a file.
fn keep_pdf(pdf: &[u8]) {
    if let Some(out) = std::env::var_os("EASYOFFICE_BENCH_PDF_OUT") {
        std::fs::write(out, pdf).expect("write the PDF");
    }
}

fn run_pdf(pages: &[String], fonts: &Arc<Database>) -> Vec<u8> {
    let mut assembler = PdfAssembler::new(fonts, true);
    assembler.add_pages(pages).expect("pages");
    let pdf = assembler.finish().expect("pdf");
    verify_exported_pdf(&pdf, pages.len(), true).expect("verified");
    // `EASYOFFICE_BENCH_PDF_OUT` keeps the PDF, to compare the output of two builds byte for byte.
    if let Some(out) = std::env::var_os("EASYOFFICE_BENCH_PDF_OUT") {
        std::fs::write(out, &pdf).expect("write the PDF");
    }
    pdf
}

fn run_reference_case(case: &str, input: &Path) -> Line {
    let name = input.file_name().unwrap().to_string_lossy().to_string();
    let started = Instant::now();
    let mut phases = Vec::new();
    let (out_bytes, pages) = match case {
        "save_verify" | "preview" => {
            let bytes = std::fs::read(input).expect("read");
            phases.push(("read", ms(started)));
            let t = Instant::now();
            let core = editable_core(&bytes);
            phases.push(("open", ms(t)));
            let out_bytes = if case == "preview" {
                let t = Instant::now();
                let svg = core.render_page_svg_native(0).expect("svg");
                phases.push(("svg0", ms(t)));
                svg.len()
            } else {
                bytes.len()
            };
            (out_bytes, core.page_count() as usize)
        }
        "pdf" => {
            let bytes = std::fs::read(input).expect("read");
            let t = Instant::now();
            let core = editable_core(&bytes);
            phases.push(("open", ms(t)));
            let t = Instant::now();
            let pages: Vec<String> = (0..core.page_count())
                .map(|page| {
                    let svg = core.render_page_svg_native(page).expect("svg");
                    add_font_fallbacks(&svg).into_owned()
                })
                .collect();
            phases.push(("svg", ms(t)));
            let t = Instant::now();
            let pdf = rhwp_pdf(&pages);
            phases.push(("pdf", ms(t)));
            phases.push(("write", write_beside(input, &pdf)));
            (pdf.len(), pages.len())
        }
        "pdf_svgs" => {
            let pages = page_svgs(input);
            let t = Instant::now();
            let pages = with_fallbacks(&pages);
            phases.push(("fallbacks", ms(t)));
            let t = Instant::now();
            let pdf = rhwp_pdf(&pages);
            phases.push(("pdf", ms(t)));
            phases.push(("write", write_beside(&input.join("pages.pdf"), &pdf)));
            (pdf.len(), pages.len())
        }
        other => panic!("unknown reference case {other}"),
    };
    Line {
        tool: "ref-repro",
        case: case.to_string(),
        input: name,
        total: ms(started),
        phases,
        out_bytes,
        pages,
    }
}

fn run_case(case: &str, input: &Path) -> Line {
    let name = input.file_name().unwrap().to_string_lossy().to_string();
    let extension = input
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let started = Instant::now();
    let mut phases = Vec::new();
    let (out_bytes, pages) = match case {
        "save_verify" => {
            let bytes = std::fs::read(input).expect("read");
            phases.push(("read", ms(started)));
            let t = Instant::now();
            verify_document_bytes(&extension, &bytes).expect("verified");
            phases.push(("verify", ms(t)));
            (bytes.len(), 0)
        }
        "preview_embedded" => {
            let mut file = std::fs::File::open(input).expect("open");
            let found = extract_thumbnail(&mut file);
            (found.map_or(0, |found| found.bytes.len()), 0)
        }
        "preview_render" => {
            let bytes = std::fs::read(input).expect("read");
            let t = Instant::now();
            let fonts = shared_font_db(&bundled_font_dirs());
            phases.push(("fonts", ms(t)));
            let t = Instant::now();
            let core = DocumentCore::from_bytes(&bytes).expect("parse");
            phases.push(("parse", ms(t)));
            let t = Instant::now();
            let svg = core.render_page_svg_native(0).expect("svg");
            phases.push(("svg0", ms(t)));
            let t = Instant::now();
            let png = render_png_sized(&svg, &fonts, CARD_SIDE_PX).expect("png");
            phases.push(("png", ms(t)));
            (png.len(), 1)
        }
        "thumb_embed" => {
            let bytes = std::fs::read(input).expect("read");
            let t = Instant::now();
            let fonts = shared_font_db(&bundled_font_dirs());
            phases.push(("fonts", ms(t)));
            let t = Instant::now();
            let core = DocumentCore::from_bytes(&bytes).expect("parse");
            phases.push(("parse", ms(t)));
            let t = Instant::now();
            let svg = core.render_page_svg_native(0).expect("svg");
            phases.push(("svg0", ms(t)));
            let t = Instant::now();
            let embedded = with_first_page(&extension, &bytes, &svg, &fonts).expect("embedded");
            phases.push(("embed", ms(t)));
            (embedded.len(), 1)
        }
        "pdf_doc" => {
            let bytes = std::fs::read(input).expect("read");
            let t = Instant::now();
            let fonts = shared_font_db(&bundled_font_dirs());
            phases.push(("fonts", ms(t)));
            let t = Instant::now();
            let core = DocumentCore::from_bytes(&bytes).expect("parse");
            phases.push(("open", ms(t)));
            let t = Instant::now();
            let pages: Vec<String> = (0..core.page_count())
                .map(|page| core.render_page_svg_native(page).expect("svg"))
                .collect();
            phases.push(("svg", ms(t)));
            let t = Instant::now();
            let pages = with_fallbacks(&pages);
            phases.push(("fallbacks", ms(t)));
            let t = Instant::now();
            let pdf = run_pdf(&pages, &fonts);
            phases.push(("pdf", ms(t)));
            (pdf.len(), pages.len())
        }
        "pdf_svgs" => {
            let pages = page_svgs(input);
            let t = Instant::now();
            let fonts = shared_font_db(&bundled_font_dirs());
            phases.push(("fonts", ms(t)));
            let t = Instant::now();
            let pages = with_fallbacks(&pages);
            phases.push(("fallbacks", ms(t)));
            let t = Instant::now();
            let pdf = run_pdf(&pages, &fonts);
            phases.push(("pdf", ms(t)));
            (pdf.len(), pages.len())
        }
        "doc_pdf_engine" => {
            let bytes = std::fs::read(input).expect("read");
            let t = Instant::now();
            let doc = rich_document(input, &bytes);
            phases.push(("decode", ms(t)));
            let t = Instant::now();
            let pdf = UniversalFormatConverter::encode_rich_pdf(&doc, &pdf_settings());
            phases.push(("pdf", ms(t)));
            keep_pdf(&pdf);
            (pdf.len(), 0)
        }
        "doc_pdf_shell" => {
            let bytes = std::fs::read(input).expect("read");
            let t = Instant::now();
            let fonts = shared_font_db(&bundled_font_dirs());
            phases.push(("fonts", ms(t)));
            let t = Instant::now();
            let doc = rich_document(input, &bytes);
            phases.push(("decode", ms(t)));
            let t = Instant::now();
            let hwpx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
                .expect("hwpx");
            phases.push(("hwpx", ms(t)));
            let t = Instant::now();
            let pdf = hwpx_to_pdf(&hwpx, &pdf_settings(), &fonts).expect("pdf");
            phases.push(("pdf", ms(t)));
            keep_pdf(&pdf);
            (pdf.len(), 0)
        }
        other => panic!("unknown case {other}"),
    };
    Line {
        tool: "eo",
        case: case.to_string(),
        input: name,
        total: ms(started),
        phases,
        out_bytes,
        pages,
    }
}

/// One run of the case named by the environment, `EASYOFFICE_BENCH_REPEAT` times.
#[test]
#[ignore = "measurement, run by scripts/bench-run.mjs"]
fn bench_case() {
    let case = std::env::var("EASYOFFICE_BENCH_CASE").expect("EASYOFFICE_BENCH_CASE");
    let input =
        PathBuf::from(std::env::var("EASYOFFICE_BENCH_INPUT").expect("EASYOFFICE_BENCH_INPUT"));
    let repeat: usize = std::env::var("EASYOFFICE_BENCH_REPEAT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    for _ in 0..repeat {
        match case.strip_prefix("ref:") {
            Some(reference) => run_reference_case(reference, &input).print(),
            None => run_case(&case, &input).print(),
        }
    }
}

/// Writes the 300 synthetic report pages of the PD-114 measurement as `page-NNN.svg`, so the
/// reference-app harness converts the same input.
#[test]
#[ignore = "measurement input, run by scripts/bench-run.mjs"]
fn bench_dump_synthetic_pages() {
    let dir =
        PathBuf::from(std::env::var("EASYOFFICE_BENCH_INPUT").expect("EASYOFFICE_BENCH_INPUT"));
    std::fs::create_dir_all(&dir).expect("dir");
    for page in 0..SYNTHETIC_PAGES {
        let svg = crate::pdf_export::tests::report_page_svg(page);
        std::fs::write(dir.join(format!("page-{page:03}.svg")), svg).expect("write");
    }
}
