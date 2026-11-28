//! The paper, margins and binding of a document's page setup (moved out of `rich_document.rs`, which keeps the
//! re-exports).

/// Page margins in twips, as a word processor's page setup gives them: `top` and `bottom` are the distances
/// from the page edge to the body (a taller header pushes the body down), `header` and `footer` the distances
/// from the page edge to the header and footer, `left` and `right` the distances to the body at the sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichPageMargins {
    pub top: u32,
    pub bottom: u32,
    pub header: u32,
    pub footer: u32,
    pub left: u32,
    pub right: u32,
}

/// The paper as it is shown, in twips: a landscape page is wider than it is high.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichPaper {
    pub width: u32,
    pub height: u32,
}

/// Where the pages are bound: one-sided (`Single`), facing pages (`Duplex`, the gutter alternates sides), at the
/// top, or as a folded booklet (`BookFold`, two pages on each sheet, DOCX `w:bookFoldPrinting`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RichBindingKind {
    #[default]
    Single,
    Duplex,
    Top,
    BookFold,
}

/// The binding of the pages: its kind and the gutter in twips, added to the margin on the bound side (the reference
/// editor's page setup `제본`, `rhwp-studio/src/ui/page-setup-dialog.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RichBinding {
    pub kind: RichBindingKind,
    pub gutter: u32,
}

/// Largest page margin or paper side a document keeps, in twips (22 inches).
pub const MAX_PAGE_MARGIN_TWIPS: u32 = 31_680;
/// Least room the margins leave for the text on a page, in twips (10 mm, the limit of the reference editor's
/// page setup `rhwp-studio/src/core/page-body-limits.ts`).
pub const MIN_PAGE_BODY_TWIPS: u32 = 567;

impl RichPageMargins {
    /// The new-document page setup of the reference editor (A4: top 20 mm and bottom 15 mm margins, header
    /// and footer areas of 15 mm, side margins of 30 mm; `rhwp` `src/model/page.rs:36-51`, areas at
    /// `:238-269`), in this form: the header starts 20 mm from the top and the body 35 mm; the footer ends
    /// 15 mm from the bottom and the body 30 mm.
    pub const HWP_DEFAULT: Self = Self {
        top: 1984,
        bottom: 1701,
        header: 1134,
        footer: 850,
        left: 1701,
        right: 1701,
    };

    /// New-document page setup of the Korean word processor (A4: top 30 mm, other sides 25.4 mm,
    /// header 15 mm, footer 17.5 mm).
    pub const DOC_DEFAULT: Self = Self {
        top: 1701,
        bottom: 1440,
        header: 851,
        footer: 992,
        left: 1440,
        right: 1440,
    };

    /// ISO A4 blank document margins of the reference Pages app (`20 mm` sides/top/bottom, `12.5 mm` header, `15 mm` footer).
    pub const PAGES_DEFAULT: Self = Self {
        top: 1134,
        bottom: 1134,
        header: 709,
        footer: 850,
        left: 1134,
        right: 1134,
    };

    /// Whether every value is within [`MAX_PAGE_MARGIN_TWIPS`].
    pub fn within_bounds(&self) -> bool {
        [
            self.top,
            self.bottom,
            self.header,
            self.footer,
            self.left,
            self.right,
        ]
        .iter()
        .all(|v| *v <= MAX_PAGE_MARGIN_TWIPS)
    }
}

impl RichPaper {
    pub const A4: Self = Self {
        width: 11_906,
        height: 16_838,
    };

    /// Whether the sides are within [`MAX_PAGE_MARGIN_TWIPS`] and the `margins` leave at least
    /// [`MIN_PAGE_BODY_TWIPS`] of text width and height. The reference editor refuses a setup whose body
    /// would vanish.
    pub fn holds(&self, margins: &RichPageMargins) -> bool {
        self.holds_bound(margins, RichBinding::default())
    }

    /// [`Self::holds`] with the gutter, which takes room from the width (as the reference editor's body-size check
    /// counts it) and from the height of a page bound at the top.
    pub fn holds_bound(&self, margins: &RichPageMargins, binding: RichBinding) -> bool {
        let room = |side: u32, a: u32, b: u32| {
            side <= MAX_PAGE_MARGIN_TWIPS
                && side.saturating_sub(a).saturating_sub(b) >= MIN_PAGE_BODY_TWIPS
        };
        let (side_gutter, top_gutter) = match binding.kind {
            RichBindingKind::Top => (0, binding.gutter),
            _ => (binding.gutter, 0),
        };
        binding.gutter <= MAX_PAGE_MARGIN_TWIPS
            && margins.within_bounds()
            && room(
                self.width,
                margins.left.saturating_add(side_gutter),
                margins.right,
            )
            && room(
                self.height,
                margins.top.saturating_add(top_gutter),
                margins.bottom,
            )
    }
}
