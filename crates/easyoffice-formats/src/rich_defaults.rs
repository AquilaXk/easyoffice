//! Document-wide defaults of an opened file (PD-363).
//!
//! A paragraph or run that names no value of its own takes the document default: the DOCX
//! `w:docDefaults` merged with the Normal style, the ODT `style:default-style` of the paragraph
//! family merged with the `Standard` style, the RTF default paragraph and the default font. A word
//! processor keeps these when it saves a file it did not create, so [`RichDocument::defaults`]
//! carries them from a reader to a writer. The Korean defaults of PD-307 are only for a new
//! document, which has no defaults of its own (`None`).
//!
//! [`RichDocument::defaults`]: crate::RichDocument::defaults

use crate::rich_document::RichAlignment;
use crate::rich_page_frame::{
    DOC_DEFAULT_AFTER_TWIPS, DOC_DEFAULT_FONT, DOC_DEFAULT_HALF_PT, DOC_DEFAULT_LINE_240THS,
    DOC_DEFAULT_TAB_STOP_TWIPS,
};

/// The size, in half-points, a file that names none gets: 10 pt, the base of OOXML and RTF.
const FALLBACK_HALF_PT: u16 = 20;
/// The tab stop, in twips, a file that names none gets: half an inch.
const FALLBACK_TAB_STOP_TWIPS: u32 = 720;
/// One line, in 240ths of a line.
const SINGLE_LINE_240THS: u32 = 240;

/// Defaults of a paragraph and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichDefaults {
    /// Latin typeface; `None` when the file names none.
    pub font: Option<String>,
    /// East Asian typeface; `None` when the file names none.
    pub east_asian_font: Option<String>,
    /// Body size in half-points.
    pub size_half_points: u16,
    /// `None` is the start edge (left in a left-to-right text).
    pub alignment: Option<RichAlignment>,
    pub space_before_twips: u32,
    pub space_after_twips: u32,
    /// Line height in 240ths of a line (`240` is single spacing).
    pub line_240ths: u32,
    pub tab_stop_twips: u32,
}

impl Default for RichDefaults {
    /// What a file means that names no default at all: unspaced single lines at the start edge.
    fn default() -> Self {
        Self {
            font: None,
            east_asian_font: None,
            size_half_points: FALLBACK_HALF_PT,
            alignment: None,
            space_before_twips: 0,
            space_after_twips: 0,
            line_240ths: SINGLE_LINE_240THS,
            tab_stop_twips: FALLBACK_TAB_STOP_TWIPS,
        }
    }
}

impl RichDefaults {
    /// The Korean defaults of a new document (PD-307): `맑은 고딕` 10 pt, justified, 8 pt after a
    /// paragraph, 108% lines, 40 pt tab stop.
    pub fn korean() -> Self {
        Self {
            font: Some(DOC_DEFAULT_FONT.to_string()),
            east_asian_font: Some(DOC_DEFAULT_FONT.to_string()),
            size_half_points: DOC_DEFAULT_HALF_PT,
            alignment: Some(RichAlignment::Justify),
            space_before_twips: 0,
            space_after_twips: DOC_DEFAULT_AFTER_TWIPS,
            line_240ths: DOC_DEFAULT_LINE_240THS,
            tab_stop_twips: DOC_DEFAULT_TAB_STOP_TWIPS,
        }
    }

    /// The defaults a writer writes for `doc`: its own, or the Korean ones of a new document.
    pub(crate) fn of(doc: &crate::RichDocument) -> Self {
        doc.defaults.clone().unwrap_or_else(Self::korean)
    }

    /// Line height as a percentage, rounded to the nearest percent.
    pub(crate) fn line_percent(&self) -> u32 {
        (self.line_240ths * 100 + 120) / SINGLE_LINE_240THS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_defaults_are_the_pd307_values() {
        let k = RichDefaults::korean();
        assert_eq!(k.font.as_deref(), Some("맑은 고딕"));
        assert_eq!(
            (k.size_half_points, k.space_after_twips, k.line_240ths),
            (20, 160, 259)
        );
        assert_eq!(
            (k.alignment, k.tab_stop_twips),
            (Some(RichAlignment::Justify), 800)
        );
        assert_eq!(k.line_percent(), 108);
    }

    #[test]
    fn a_file_without_defaults_means_single_unspaced_start_aligned_lines() {
        let d = RichDefaults::default();
        assert_eq!(
            (d.line_240ths, d.space_after_twips, d.alignment),
            (240, 0, None)
        );
        assert_eq!(d.line_percent(), 100);
    }
}
