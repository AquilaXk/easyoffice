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
//! Pages are laid out and drawn as SVG on one thread (rhwp's layout recurses with the document's
//! nesting and the platform's default thread stack is small, so that thread has a large stack; a
//! panic in it comes back as an error) while the calling thread converts the previous batch into
//! PDF pages, so the SVG drawing and the conversion overlap. Only a few batches are alive at once,
//! the SVGs of a long document never are. The caller writes the engine's PDF when this fails.

use crate::doc_pdf_marks::mark_page;
use crate::pdf_export::{PdfAssembler, BATCH_PAGES, MAX_PAGES};
use easyoffice_formats::PdfWriteOptions;
use rhwp::DocumentCore;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;
use usvg::fontdb::Database;

/// Largest HWPX laid out (bytes), the limit of the start window's render and of the Quick Look
/// extensions. The layout cannot be cancelled and runs over the whole document before the first
/// page is known, so a larger document takes the engine's PDF.
pub const MAX_HWPX_BYTES: usize = 50 * 1024 * 1024;
/// Layout recurses with the document's nesting (same size as the start window's render worker).
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;
/// Batches drawn ahead of the conversion. One keeps the drawing thread busy while a batch is
/// converted; a longer queue only holds more SVG text.
const BATCHES_AHEAD: usize = 1;

type Batch = Result<Vec<String>, String>;

/// The PDF of `hwpx`, a document the engine wrote, with the page settings of `options`.
pub fn hwpx_to_pdf(
    hwpx: &[u8],
    options: &PdfWriteOptions,
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, String> {
    if hwpx.len() > MAX_HWPX_BYTES {
        return Err("문서가 쪽 배치로 그릴 수 있는 크기를 넘습니다.".to_string());
    }
    thread::scope(|scope| {
        let (send, batches) = sync_channel::<Batch>(BATCHES_AHEAD);
        let drawing = thread::Builder::new()
            .name("doc-pdf-layout".into())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, move || draw_pages(hwpx, options, &send))
            .map_err(|err| format!("PDF 작업을 시작하지 못했습니다: {err}"))?;
        // A page conversion that panics is an error too: the caller has the engine's PDF to fall
        // back on, a panic would only fail the request.
        let converted = catch_unwind(AssertUnwindSafe(|| {
            convert_batches(&batches, options, fonts)
        }))
        .unwrap_or_else(|_| Err("쪽을 PDF로 바꾸는 중 오류가 났습니다.".to_string()));
        // A failed conversion ends the drawing too: its next send finds nobody receiving.
        drop(batches);
        drawing
            .join()
            .map_err(|_| "문서 배치 중 오류가 났습니다.".to_string())?;
        converted
    })
}

/// Converts the batches as they arrive and finishes the PDF.
fn convert_batches(
    batches: &Receiver<Batch>,
    options: &PdfWriteOptions,
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, String> {
    let mut assembler = PdfAssembler::new(fonts, true);
    assembler.set_rotation(options.rotation_degrees);
    for batch in batches {
        assembler.add_pages(&batch?)?;
    }
    assembler.finish()
}

/// Lays `hwpx` out and sends its pages, [`BATCH_PAGES`] at a time. A failure is sent as the last
/// message; a receiver that is gone (the conversion failed) ends the drawing quietly.
fn draw_pages(hwpx: &[u8], options: &PdfWriteOptions, send: &SyncSender<Batch>) {
    if let Err(reason) = try_draw_pages(hwpx, options, send) {
        let _ = send.send(Err(reason));
    }
}

fn try_draw_pages(
    hwpx: &[u8],
    options: &PdfWriteOptions,
    send: &SyncSender<Batch>,
) -> Result<(), String> {
    let core = DocumentCore::from_bytes(hwpx)
        .map_err(|err| format!("문서를 배치하지 못했습니다: {err}"))?;
    let pages = core.page_count() as usize;
    if pages == 0 || pages > MAX_PAGES {
        return Err(format!(
            "쪽 수({pages})가 PDF로 내보낼 수 있는 범위가 아닙니다."
        ));
    }
    let mut batch = Vec::with_capacity(BATCH_PAGES);
    for page in 0..pages {
        let mut svg = core
            .render_page_svg_native(page as u32)
            .map_err(|err| format!("{}쪽을 그리지 못했습니다: {err}", page + 1))?;
        mark_page(&mut svg, page, options)?;
        batch.push(svg);
        if batch.len() == BATCH_PAGES || page + 1 == pages {
            let full = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_PAGES));
            if send.send(Ok(full)).is_err() {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "doc_pdf_tests.rs"]
mod tests;
