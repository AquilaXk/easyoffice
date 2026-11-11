//! Which way a recent file's first page is made when the file carries no preview picture, and
//! how much of the file is read for it (PD-142, PD-179).

use super::render::MAX_RENDER_INPUT_BYTES;
use crate::windows::WindowKind;
use std::path::Path;

/// How much of a text file is read: a card shows its first lines, and a text file that fits a
/// window can be 10 MiB.
pub const TEXT_PREFIX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A Hangul document: rhwp lays out the page (PD-142).
    Hangul,
    /// A PDF: the worker process draws the page (PD-179).
    Pdf,
    /// A package or binary document of the other editors: the card is drawn from its content.
    Document,
    /// CSV, TSV, plain text and Markdown: only the start of the file is read.
    Text,
}

impl Source {
    /// The way for the file at `path`, by its extension; `None` for a file no window opens.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        Some(match WindowKind::for_extension(extension)? {
            WindowKind::Hwp => Self::Hangul,
            WindowKind::Pdf => Self::Pdf,
            _ if is_text(extension) => Self::Text,
            WindowKind::Doc | WindowKind::Sheet | WindowKind::Slide => Self::Document,
            WindowKind::Start | WindowKind::Conv => return None,
        })
    }

    /// Largest file that is drawn. The other editors' windows refuse a file over their limit, so
    /// the card does not draw one either (the page's own limit, `WindowKind::max_open_bytes`).
    pub fn max_file_bytes(self) -> u64 {
        match self {
            Self::Hangul => MAX_RENDER_INPUT_BYTES as u64,
            Self::Pdf => WindowKind::Pdf.max_open_bytes(),
            Self::Document | Self::Text => WindowKind::Doc.max_open_bytes(),
        }
    }

    /// How many bytes are read of a file that is within [`Source::max_file_bytes`].
    pub fn read_bytes(self) -> usize {
        match self {
            Self::Text => TEXT_PREFIX_BYTES,
            other => other.max_file_bytes() as usize,
        }
    }
}

fn is_text(extension: &str) -> bool {
    ["csv", "tsv", "txt", "md", "markdown"]
        .iter()
        .any(|text| extension.eq_ignore_ascii_case(text))
}

/// `bytes` cut after its last line break when it is a prefix of a longer file, so no line (and no
/// multi-byte character) is cut in the middle; a prefix without a line break is kept whole.
pub fn whole_lines(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(at) = bytes.iter().rposition(|&b| b == b'\n') {
        bytes.truncate(at + 1);
    }
    bytes
}
