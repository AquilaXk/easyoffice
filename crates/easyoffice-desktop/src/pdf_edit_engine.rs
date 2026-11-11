//! What reads and writes PDFs for the edit commands (PD-164): a worker process in the shell, the
//! reader and writer themselves in the tests.
//!
//! [`ChildEngine`] is the one the shell uses: every request goes to a worker process
//! (`pdf_edit_supervisor.rs`) that is killed at its deadline or memory limit. The stamp and
//! watermark forms are made here, in the shell, from SVG with the bundled and system fonts
//! (`pdf_export.rs`); their text is the user's own and the worker only receives the finished PDF.

use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_supervisor::Supervisor;
use crate::pdf_edit_wire::{BuildHead, PagePlan};
use crate::pdf_export::{shared_font_db, PdfAssembler};
use std::path::PathBuf;
use std::sync::Arc;

/// Checks PDFs and writes the result of an edit.
pub trait PdfEngine {
    /// The number of pages of `bytes`, if it is a PDF that passes every budget.
    fn inspect(&self, bytes: &Arc<Vec<u8>>) -> Result<usize, String>;

    /// The PDF made of `pages` of `sources`, with `overlay` laid over them.
    fn build(
        &self,
        sources: &[Arc<Vec<u8>>],
        pages: &[PagePlan],
        overlay: &Overlay,
    ) -> Result<Vec<u8>, String>;
}

/// The one-page PDF of the stamp and watermark text for a page shown `width` x `height` points.
pub fn overlay_pdf(
    font_dirs: &[PathBuf],
    overlay: &Overlay,
    width: f32,
    height: f32,
    stamp: bool,
) -> Result<Vec<u8>, String> {
    let svg = overlay
        .svg(width, height, stamp)
        .ok_or_else(|| "스탬프와 워터마크를 만들 수 없습니다.".to_string())?;
    let fonts = shared_font_db(font_dirs);
    let mut assembler = PdfAssembler::new(&fonts, true);
    assembler.add_pages(&[svg])?;
    assembler.finish()
}

/// The shell's engine: a worker process per request.
pub struct ChildEngine {
    supervisor: Supervisor,
    font_dirs: Vec<PathBuf>,
}

impl ChildEngine {
    /// `font_dirs` are the folders of the shared font database (see `pdf_job::pdf_font_dirs`).
    pub fn new(font_dirs: Vec<PathBuf>) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|err| {
            eprintln!("pdf edit: the executable could not be found: {err}");
            crate::pdf_edit_budget::MSG_WORKER_DIED.to_string()
        })?;
        Ok(Self {
            supervisor: Supervisor::new(exe),
            font_dirs,
        })
    }
}

impl PdfEngine for ChildEngine {
    fn inspect(&self, bytes: &Arc<Vec<u8>>) -> Result<usize, String> {
        self.supervisor.inspect(bytes)
    }

    fn build(
        &self,
        sources: &[Arc<Vec<u8>>],
        pages: &[PagePlan],
        overlay: &Overlay,
    ) -> Result<Vec<u8>, String> {
        let head = BuildHead {
            sources: sources.len(),
            pages: pages.to_vec(),
            watermark: overlay.watermark.clone(),
            stamp: overlay.stamp.clone(),
        };
        self.supervisor
            .build(sources, &head, &mut |width, height, stamp| {
                overlay_pdf(&self.font_dirs, overlay, width, height, stamp)
            })
    }
}

/// The reader and writer called directly, for the tests of everything above the process boundary
/// (the process boundary has its own tests in `tests/pdf_isolation.rs`).
#[cfg(test)]
pub struct InProcessEngine {
    fonts: Arc<usvg::fontdb::Database>,
}

#[cfg(test)]
impl InProcessEngine {
    pub fn new(fonts: Arc<usvg::fontdb::Database>) -> Self {
        Self { fonts }
    }
}

#[cfg(test)]
impl PdfEngine for InProcessEngine {
    fn inspect(&self, bytes: &Arc<Vec<u8>>) -> Result<usize, String> {
        let deadline = crate::pdf_edit_budget::Deadline::after(std::time::Duration::from_secs(60));
        crate::pdf_edit_inspect::inspect(bytes, &deadline).map(|info| info.pages)
    }

    fn build(
        &self,
        sources: &[Arc<Vec<u8>>],
        pages: &[PagePlan],
        overlay: &Overlay,
    ) -> Result<Vec<u8>, String> {
        let deadline = crate::pdf_edit_budget::Deadline::after(std::time::Duration::from_secs(60));
        let fonts = Arc::clone(&self.fonts);
        crate::pdf_edit_build::build(
            sources,
            pages,
            overlay,
            &mut |width, height, stamp| {
                let svg = overlay.svg(width, height, stamp).ok_or("no overlay")?;
                let mut assembler = PdfAssembler::new(&fonts, true);
                assembler.add_pages(&[svg])?;
                assembler.finish()
            },
            &deadline,
        )
    }
}
