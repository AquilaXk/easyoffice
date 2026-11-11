//! The first page of a recent PDF, drawn by the PDF worker process (PD-179).
//!
//! A PDF that cannot be trusted may make a renderer allocate gigabytes or run for minutes, and a
//! thread cannot be stopped from outside (PD-170 measured both). So the page is drawn in the same
//! worker process the PDF edit uses (`pdf_edit_worker.rs`), which the shell kills at a deadline
//! or when it holds too much memory, and which first runs the file through the edit's budgets
//! (size, streams, pages, encryption). This module is the shell's side: it hands the worker the
//! bytes, and the Korean font for text the file does not embed (the worker has no fonts), and takes
//! back a PNG or a refusal.

use super::bounded::RenderError;
use crate::pdf_edit_budget::MSG_BUSY;
use crate::pdf_edit_supervisor::Supervisor;
use crate::pdf_edit_wire::MAX_RENDER_FONT_BYTES;
use std::sync::{Arc, OnceLock};
use usvg::fontdb::{Database, Family, Query};

/// The bytes of the font the generic `sans-serif` name resolves to (the bundled Noto Sans KR),
/// read once: every PDF that names a CJK font it does not embed is drawn with it.
fn korean_font(fonts: &Database) -> Option<Arc<Vec<u8>>> {
    static FONT: OnceLock<Option<Arc<Vec<u8>>>> = OnceLock::new();
    FONT.get_or_init(|| {
        let query = Query {
            families: &[Family::SansSerif],
            ..Query::default()
        };
        let id = fonts.query(&query)?;
        fonts
            .with_face_data(id, |data, index| {
                (index == 0 && data.len() <= MAX_RENDER_FONT_BYTES).then(|| Arc::new(data.to_vec()))
            })
            .flatten()
    })
    .clone()
}

/// The PNG of the first page of `pdf`.
pub fn first_page_png(pdf: Vec<u8>, fonts: &Database) -> Result<Vec<u8>, RenderError> {
    let exe = std::env::current_exe().map_err(|_| RenderError::Panicked)?;
    let font = korean_font(fonts);
    Supervisor::for_preview(exe)
        .render_first_page(&Arc::new(pdf), font.as_deref().map_or(&[], Vec::as_slice))
        .map_err(|sentence| {
            if sentence == MSG_BUSY {
                RenderError::Busy
            } else {
                RenderError::Unreadable
            }
        })
}
