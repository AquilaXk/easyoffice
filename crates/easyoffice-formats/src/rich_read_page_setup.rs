//! The page setup of a DOCX section (`w:pgSz`, `w:pgMar`) as the page frame keeps it.

use crate::xml_stream::{attr, Attrs};
use crate::{RichBinding, RichBindingKind, RichPageMargins, RichPaper};

/// `w:pgMar` as page margins: the four distances that place the body, header and footer present (a negative top
/// or bottom, an exact margin in Word, counts by its size) and within the bounds; a missing side margin is
/// the word processor's inch.
pub(crate) fn page_margins(attrs: &Attrs<'_>) -> Option<RichPageMargins> {
    const DEFAULT_SIDE_TWIPS: u32 = 1440;
    let twips = |key: &str| {
        let v = attr(attrs, key)?.trim().parse::<i64>().ok()?;
        u32::try_from(v.abs()).ok()
    };
    let margins = RichPageMargins {
        top: twips("w:top")?,
        bottom: twips("w:bottom")?,
        header: twips("w:header")?,
        footer: twips("w:footer")?,
        left: twips("w:left").unwrap_or(DEFAULT_SIDE_TWIPS),
        right: twips("w:right").unwrap_or(DEFAULT_SIDE_TWIPS),
    };
    margins.within_bounds().then_some(margins)
}

/// `w:gutter` of `w:pgMar` in twips (0 when it is absent or out of bounds).
pub(crate) fn page_gutter(attrs: &Attrs<'_>) -> u32 {
    attr(attrs, "w:gutter")
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|v| *v <= crate::MAX_PAGE_MARGIN_TWIPS)
        .unwrap_or(0)
}

/// The binding of a section: its gutter and, from `word/settings.xml`, whether the pages face each other
/// (`w:mirrorMargins`) or the gutter is at the top (`w:gutterAtTop`).
pub(crate) fn binding_of(gutter: u32, mirror: bool, at_top: bool) -> RichBinding {
    let kind = if mirror {
        RichBindingKind::Duplex
    } else if at_top {
        RichBindingKind::Top
    } else {
        RichBindingKind::Single
    };
    RichBinding { kind, gutter }
}

/// `w:pgSz` as the paper as shown (`w:w` and `w:h` are the sides of the page as it is laid out).
pub(crate) fn page_paper(attrs: &Attrs<'_>) -> Option<RichPaper> {
    let side = |key: &str| {
        attr(attrs, key)?
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|v| *v > 0)
    };
    Some(RichPaper {
        width: side("w:w")?,
        height: side("w:h")?,
    })
}

/// The page setup of the first section as far as it fits: a paper that cannot hold its margins, or has a side
/// out of bounds, is dropped with them.
pub(crate) fn fitting_setup(
    margins: Option<RichPageMargins>,
    paper: Option<RichPaper>,
    binding: RichBinding,
) -> (Option<RichPageMargins>, Option<RichPaper>) {
    let bare = RichPageMargins {
        top: 0,
        bottom: 0,
        header: 0,
        footer: 0,
        left: 0,
        right: 0,
    };
    match (margins, paper) {
        (Some(m), Some(p)) if !p.holds_bound(&m, binding) => (None, None),
        (None, Some(p)) if !p.holds(&bare) => (None, None),
        other => other,
    }
}
