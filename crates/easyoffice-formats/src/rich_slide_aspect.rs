//! Slide page size (`PD-257`, custom sizes `PD-402`): the three presets, a custom size in EMU and
//! the page orientation, with the point, EMU and centimetre dimensions each file format needs.
//!
//! Reference implementation (3-step check, PD-402): (1) no maintained crate reads a `p:sldSz`
//! into a page model, (2) the deck model already carried the presets, so the enum grew one
//! variant, (3) the size table follows PptxGenJS `src/pptxgen.ts` lines 315-324 (`LAYOUT_4x3`
//! 9144000x6858000, `LAYOUT_16x9` 9144000x5143500, `LAYOUT_16x10` 9144000x5715000, `LAYOUT_WIDE`
//! 12192000x6858000; MIT, Copyright (c) 2015-present Brent Ely, commit 3c9ec1b687c1) and its
//! `src/gen-xml.ts` line 1802 (`<p:sldSz cx cy/>` without a `type` for a custom size). The range
//! check is ours: PptxGenJS accepts any number, a hostile file must not.

use crate::rich_slide::{EMU_PER_PT, MAX_SLIDE_COORD_PT};

/// Shortest and longest side of a custom slide, in EMU (1 inch up to the coordinate cap of
/// [`MAX_SLIDE_COORD_PT`]); a file or a request outside it is rejected, never clamped.
pub const MIN_SLIDE_SIZE_EMU: u64 = 914_400;
pub const MAX_SLIDE_SIZE_EMU: u64 = (MAX_SLIDE_COORD_PT * EMU_PER_PT) as u64;
const EMU_PER_CM: f64 = 360_000.0;
const PRESET_TOLERANCE_PT: f64 = 0.6;
/// Ratio of the long to the short side below which an out-of-range size is read as 4:3 or A4.
const RATIO_4X3_A4: f64 = (4.0 / 3.0 + 841.89 / 595.28) * 0.5;
const RATIO_A4_16X9: f64 = (841.89 / 595.28 + 16.0 / 9.0) * 0.5;

/// Slide canvas aspect ratio and canonical point / EMU dimensions.
///
/// `Custom` carries the exact `p:sldSz` of a file whose size is none of the presets, as
/// `(cx, cy)` EMU of the landscape page (a portrait page keeps the long side in `cx` and
/// is turned by [`SlideOrientation`], like the presets). Its wire form is `custom:<cx>x<cy>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideAspect {
    #[default]
    Widescreen16x9,
    Standard4x3,
    HandoutA4,
    Custom {
        cx: u32,
        cy: u32,
    },
}

impl SlideAspect {
    pub const PRESETS: [Self; 3] = [Self::Widescreen16x9, Self::Standard4x3, Self::HandoutA4];

    /// A custom size from EMU sides; `None` when either side is outside the allowed range.
    pub fn custom(cx: u64, cy: u64) -> Option<Self> {
        let ok = |v: u64| (MIN_SLIDE_SIZE_EMU..=MAX_SLIDE_SIZE_EMU).contains(&v);
        (ok(cx) && ok(cy)).then(|| Self::Custom {
            cx: cx as u32,
            cy: cy as u32,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Widescreen16x9 => "16:9",
            Self::Standard4x3 => "4:3",
            Self::HandoutA4 => "A4",
            Self::Custom { .. } => "custom",
        }
    }

    /// The string of the structured-save wire: the preset name, or `custom:<cx>x<cy>`.
    pub fn to_wire(self) -> String {
        match self {
            Self::Custom { cx, cy } => format!("custom:{cx}x{cy}"),
            preset => preset.as_str().to_string(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t == "16:9" || t.eq_ignore_ascii_case("widescreen16x9") {
            Some(Self::Widescreen16x9)
        } else if t == "4:3" || t.eq_ignore_ascii_case("standard4x3") {
            Some(Self::Standard4x3)
        } else if t.eq_ignore_ascii_case("a4") || t.eq_ignore_ascii_case("handouta4") {
            Some(Self::HandoutA4)
        } else {
            let (w, h) = t.strip_prefix("custom:")?.split_once('x')?;
            Self::custom(w.parse().ok()?, h.parse().ok()?)
        }
    }

    pub const fn dimensions_pt(self) -> (f64, f64) {
        match self {
            Self::Widescreen16x9 => (960.0, 540.0),
            Self::Standard4x3 => (720.0, 540.0),
            Self::HandoutA4 => (841.89, 595.28),
            Self::Custom { cx, cy } => (cx as f64 / EMU_PER_PT, cy as f64 / EMU_PER_PT),
        }
    }

    pub const fn dimensions_emu(self) -> (u64, u64) {
        match self {
            Self::Widescreen16x9 => (12_192_000, 6_858_000),
            Self::Standard4x3 => (9_144_000, 6_858_000),
            Self::HandoutA4 => (10_692_000, 7_560_000),
            Self::Custom { cx, cy } => (cx as u64, cy as u64),
        }
    }

    pub const fn dimensions_cm(self) -> (f64, f64) {
        match self {
            Self::Widescreen16x9 => (33.867, 19.05),
            Self::Standard4x3 => (25.40, 19.05),
            Self::HandoutA4 => (29.70, 21.00),
            Self::Custom { cx, cy } => (cx as f64 / EMU_PER_CM, cy as f64 / EMU_PER_CM),
        }
    }

    /// The size a file gives as `(aspect, orientation, scale_x, scale_y)`. A size equal to a
    /// preset (within 0.6 pt) keeps the preset; any other size inside the allowed range is
    /// kept exactly as [`SlideAspect::Custom`] with no scaling. A size outside the range falls
    /// back to the nearest preset with the scales that fit the content into it.
    pub fn from_dimensions_oriented_pt(w_pt: f64, h_pt: f64) -> (Self, SlideOrientation, f64, f64) {
        if !w_pt.is_finite() || !h_pt.is_finite() || w_pt <= 1.0 || h_pt <= 1.0 {
            return (Self::Widescreen16x9, SlideOrientation::Landscape, 1.0, 1.0);
        }
        let orientation = if h_pt > w_pt + 1.0 {
            SlideOrientation::Portrait
        } else {
            SlideOrientation::Landscape
        };
        let portrait = orientation == SlideOrientation::Portrait;
        let (maj, min) = if portrait { (h_pt, w_pt) } else { (w_pt, h_pt) };
        let exact = Self::PRESETS.into_iter().find(|p| {
            let (pw, ph) = p.dimensions_pt();
            (maj - pw).abs() < PRESET_TOLERANCE_PT && (min - ph).abs() < PRESET_TOLERANCE_PT
        });
        if let Some(preset) = exact {
            return (preset, orientation, 1.0, 1.0);
        }
        // A portrait page keeps its long side in `cx` and is turned by `orientation`.
        if let Some(custom) = Self::custom(
            (maj * EMU_PER_PT).round() as u64,
            (min * EMU_PER_PT).round() as u64,
        ) {
            return (custom, orientation, 1.0, 1.0);
        }
        let ratio = maj / min;
        let aspect = if ratio < RATIO_4X3_A4 {
            Self::Standard4x3
        } else if ratio < RATIO_A4_16X9 {
            Self::HandoutA4
        } else {
            Self::Widescreen16x9
        };
        let (base_w, base_h) = aspect.dimensions_pt();
        let (target_w, target_h) = if portrait {
            (base_h, base_w)
        } else {
            (base_w, base_h)
        };
        (aspect, orientation, target_w / w_pt, target_h / h_pt)
    }

    pub fn from_emu_oriented(cx: u64, cy: u64) -> (Self, SlideOrientation, f64, f64) {
        Self::from_dimensions_oriented_pt(cx as f64 / EMU_PER_PT, cy as f64 / EMU_PER_PT)
    }
}

/// Slide orientation (`landscape` default, or `portrait`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideOrientation {
    #[default]
    Landscape,
    Portrait,
}

impl SlideOrientation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Landscape => "landscape",
            Self::Portrait => "portrait",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("landscape") {
            Some(Self::Landscape)
        } else if t.eq_ignore_ascii_case("portrait") {
            Some(Self::Portrait)
        } else {
            None
        }
    }
}
