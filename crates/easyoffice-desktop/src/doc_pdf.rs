//! A rich document as a PDF drawn by rhwp's page layout (PD-164).
//!
//! The engine writes the document as HWPX (headings, runs, lists, tables, pictures). rhwp, the
//! layout engine of the Hangul editor, lays that HWPX out into pages and draws each as SVG, and the
//! searchable export of the editor (`pdf_export.rs`, PD-101) turns the SVGs into PDF pages with
//! embedded font subsets. So a converted DOCX/ODT/RTF/Markdown/text file gets the same pages the
//! editor prints, instead of the engine writer's plain text lines. The page settings of the engine
//! writer stay: the rotation is the page's `/Rotate`, the watermark and the stamp are drawn into the
//! page SVG (`doc_pdf_marks.rs`).
//!
//! Pages are laid out and converted a batch at a time, so the SVGs of a long document are never
//! alive together. rhwp's layout recurses with the document's nesting and the platform's default
//! thread stack is small, so the work runs on one thread with a large stack; a panic in the layout
//! comes back as an error, and the caller writes the engine's PDF instead.

use crate::doc_pdf_marks::mark_page;
use crate::pdf_export::{PdfAssembler, BATCH_PAGES, MAX_PAGES};
use easyoffice_formats::PdfWriteOptions;
use rhwp::DocumentCore;
use std::sync::Arc;
use std::thread;
use usvg::fontdb::Database;

/// Layout recurses with the document's nesting (same size as the start window's render worker).
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;

/// The PDF of `hwpx`, a document the engine wrote, with the page settings of `options`.
pub fn hwpx_to_pdf(
    hwpx: &[u8],
    options: &PdfWriteOptions,
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, String> {
    thread::scope(|scope| {
        thread::Builder::new()
            .name("doc-pdf".into())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, || render(hwpx, options, fonts))
            .map_err(|err| format!("PDF 작업을 시작하지 못했습니다: {err}"))?
            .join()
            .map_err(|_| "문서 배치 중 오류가 났습니다.".to_string())?
    })
}

fn render(
    hwpx: &[u8],
    options: &PdfWriteOptions,
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, String> {
    let core = DocumentCore::from_bytes(hwpx)
        .map_err(|err| format!("문서를 배치하지 못했습니다: {err}"))?;
    let pages = core.page_count() as usize;
    if pages == 0 || pages > MAX_PAGES {
        return Err(format!(
            "쪽 수({pages})가 PDF로 내보낼 수 있는 범위가 아닙니다."
        ));
    }
    let mut assembler = PdfAssembler::new(fonts, true);
    assembler.set_rotation(options.rotation_degrees);
    let mut batch = Vec::with_capacity(BATCH_PAGES);
    for page in 0..pages {
        let mut svg = core
            .render_page_svg_native(page as u32)
            .map_err(|err| format!("{}쪽을 그리지 못했습니다: {err}", page + 1))?;
        mark_page(&mut svg, page, options)?;
        batch.push(svg);
        if batch.len() == BATCH_PAGES || page + 1 == pages {
            assembler.add_pages(&batch)?;
            batch.clear();
        }
    }
    assembler.finish()
}

#[cfg(test)]
#[path = "doc_pdf_tests.rs"]
mod tests;
