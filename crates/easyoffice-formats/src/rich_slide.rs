//! Coordinate-based slide deck model (`PD-257`) with fail-closed resource budgets,
//! aspect-ratio normalization, master layouts, transitions, and 5 element kinds.

use std::collections::HashSet;

use crate::rich_document::normalize_hex_color;
use crate::rich_slide_ext::{DeckSettings, SlideExt};
use crate::sheet_chart::{ChartKind, ChartLegendPos};
use crate::{DecodedSlide, EasyOfficeError, RichImage};

pub const EMU_PER_PT: f64 = 12_700.0;
pub const PT_PER_CM: f64 = 72.0 / 2.54;
pub const PT_PER_MM: f64 = 72.0 / 25.4;
pub const PT_PER_IN: f64 = 72.0;

pub const MAX_SLIDES_PER_DECK: usize = 2_000;
pub const MAX_ELEMENTS_PER_SLIDE: usize = 256;
pub const MAX_ELEMENTS_PER_DECK: usize = 8_192;
pub const MAX_SLIDE_TEXT_BYTES: usize = 32_768;
pub const MAX_SLIDE_TABLE_ROWS: usize = 64;
pub const MAX_SLIDE_TABLE_COLS: usize = 32;
pub const MAX_SLIDE_CHART_SERIES: usize = 16;
pub const MAX_SLIDE_CHART_POINTS: usize = 512;
pub const MAX_SLIDE_TOTAL_IMAGE_BYTES: usize = 128 * 1024 * 1024;
pub const MIN_SLIDE_FONT_PT: f64 = 6.0;
pub const MAX_SLIDE_FONT_PT: f64 = 200.0;
pub const DEFAULT_SLIDE_FONT_PT: f64 = 18.0;
pub const MAX_SLIDE_STROKE_PT: f64 = 24.0;
pub const DEFAULT_SLIDE_STROKE_PT: f64 = 1.5;
pub const MAX_SLIDE_COORD_PT: f64 = 4_000.0;

/// Slide canvas aspect ratio and canonical point / EMU dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideAspect {
    #[default]
    Widescreen16x9,
    Standard4x3,
    HandoutA4,
}

impl SlideAspect {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Widescreen16x9 => "16:9",
            Self::Standard4x3 => "4:3",
            Self::HandoutA4 => "A4",
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
            None
        }
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

impl SlideAspect {
    pub const fn dimensions_pt(self) -> (f64, f64) {
        match self {
            Self::Widescreen16x9 => (960.0, 540.0),
            Self::Standard4x3 => (720.0, 540.0),
            Self::HandoutA4 => (841.89, 595.28),
        }
    }

    pub const fn dimensions_emu(self) -> (u64, u64) {
        match self {
            Self::Widescreen16x9 => (12_192_000, 6_858_000),
            Self::Standard4x3 => (9_144_000, 6_858_000),
            Self::HandoutA4 => (10_692_000, 7_560_000),
        }
    }

    pub const fn dimensions_cm(self) -> (f64, f64) {
        match self {
            Self::Widescreen16x9 => (33.867, 19.05),
            Self::Standard4x3 => (25.40, 19.05),
            Self::HandoutA4 => (29.70, 21.00),
        }
    }

    pub fn from_dimensions_oriented_pt(w_pt: f64, h_pt: f64) -> (Self, SlideOrientation, f64, f64) {
        if !w_pt.is_finite() || !h_pt.is_finite() || w_pt <= 1.0 || h_pt <= 1.0 {
            return (Self::Widescreen16x9, SlideOrientation::Landscape, 1.0, 1.0);
        }
        let orientation = if h_pt > w_pt + 1.0 {
            SlideOrientation::Portrait
        } else {
            SlideOrientation::Landscape
        };
        let (maj, min) = if orientation == SlideOrientation::Portrait {
            (h_pt, w_pt)
        } else {
            (w_pt, h_pt)
        };
        let ratio = maj / min;
        let aspect = if ratio < (4.0 / 3.0 + 841.89 / 595.28) * 0.5 {
            Self::Standard4x3
        } else if ratio < (841.89 / 595.28 + 16.0 / 9.0) * 0.5 {
            Self::HandoutA4
        } else {
            Self::Widescreen16x9
        };
        let (base_w, base_h) = aspect.dimensions_pt();
        let (target_w, target_h) = if orientation == SlideOrientation::Portrait {
            (base_h, base_w)
        } else {
            (base_w, base_h)
        };
        let (sx, sy) = if (w_pt - target_w).abs() < 0.6 && (h_pt - target_h).abs() < 0.6 {
            (1.0, 1.0)
        } else {
            (target_w / w_pt, target_h / h_pt)
        };
        (aspect, orientation, sx, sy)
    }

    pub fn from_emu_oriented(cx: u64, cy: u64) -> (Self, SlideOrientation, f64, f64) {
        Self::from_dimensions_oriented_pt(cx as f64 / EMU_PER_PT, cy as f64 / EMU_PER_PT)
    }

    /// Maps arbitrary slide dimensions `(w_pt, h_pt)` to the nearest [`SlideAspect`] and returns
    /// `(aspect, scale_x, scale_y)` for normalizing coordinates into that aspect's point space.
    pub fn from_dimensions_pt(w_pt: f64, h_pt: f64) -> (Self, f64, f64) {
        if !w_pt.is_finite() || !h_pt.is_finite() || w_pt <= 1.0 || h_pt <= 1.0 {
            return (Self::Widescreen16x9, 1.0, 1.0);
        }
        let ratio = w_pt / h_pt;
        let aspect = if ratio < (4.0 / 3.0 + 841.89 / 595.28) * 0.5 {
            Self::Standard4x3
        } else if ratio < (841.89 / 595.28 + 16.0 / 9.0) * 0.5 {
            Self::HandoutA4
        } else {
            Self::Widescreen16x9
        };
        let (target_w, target_h) = aspect.dimensions_pt();
        if (w_pt - target_w).abs() < 0.6 && (h_pt - target_h).abs() < 0.6 {
            (aspect, 1.0, 1.0)
        } else {
            (aspect, target_w / w_pt, target_h / h_pt)
        }
    }

    pub fn from_emu(cx: u64, cy: u64) -> (Self, f64, f64) {
        Self::from_dimensions_pt(cx as f64 / EMU_PER_PT, cy as f64 / EMU_PER_PT)
    }
}

/// Slide master layout preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideMasterLayout {
    TitleSlide,
    #[default]
    TitleAndContent,
    SectionHeader,
    TwoColumn,
    Blank,
}

impl SlideMasterLayout {
    pub const ALL: [Self; 5] = [
        Self::TitleSlide,
        Self::TitleAndContent,
        Self::SectionHeader,
        Self::TwoColumn,
        Self::Blank,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TitleSlide => "titleSlide",
            Self::TitleAndContent => "titleAndContent",
            Self::SectionHeader => "sectionHeader",
            Self::TwoColumn => "twoColumn",
            Self::Blank => "blank",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("titleSlide") || t == "title" {
            Some(Self::TitleSlide)
        } else if t.eq_ignore_ascii_case("titleAndContent") || t == "obj" {
            Some(Self::TitleAndContent)
        } else if t.eq_ignore_ascii_case("sectionHeader") || t == "secHead" {
            Some(Self::SectionHeader)
        } else if t.eq_ignore_ascii_case("twoColumn") || t == "twoObj" {
            Some(Self::TwoColumn)
        } else if t.eq_ignore_ascii_case("blank") {
            Some(Self::Blank)
        } else {
            None
        }
    }

    pub const fn ooxml_type(self) -> &'static str {
        match self {
            Self::TitleSlide => "title",
            Self::TitleAndContent => "obj",
            Self::SectionHeader => "secHead",
            Self::TwoColumn => "twoObj",
            Self::Blank => "blank",
        }
    }

    pub const fn ooxml_name(self) -> &'static str {
        match self {
            Self::TitleSlide => "제목 슬라이드",
            Self::TitleAndContent => "제목 및 내용",
            Self::SectionHeader => "구역 머리글",
            Self::TwoColumn => "두 개의 내용",
            Self::Blank => "빈 화면",
        }
    }

    pub const fn layout_index(self) -> usize {
        match self {
            Self::TitleSlide => 1,
            Self::TitleAndContent => 2,
            Self::SectionHeader => 3,
            Self::TwoColumn => 4,
            Self::Blank => 5,
        }
    }

    pub fn from_ooxml_type(t: &str) -> Self {
        match t.trim() {
            "title" | "ctrTitle" => Self::TitleSlide,
            "secHead" => Self::SectionHeader,
            "twoObj" | "twoTxTwoObj" => Self::TwoColumn,
            "blank" => Self::Blank,
            _ => Self::TitleAndContent,
        }
    }
}

pub use crate::rich_slide_ext::SlideTransition;

/// Horizontal text alignment inside a slide textbox or shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideTextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl SlideTextAlign {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("left")
            || t == "l"
            || t == "start"
            || t.eq_ignore_ascii_case("justify")
            || t == "just"
        {
            Some(Self::Left)
        } else if t.eq_ignore_ascii_case("center") || t == "ctr" {
            Some(Self::Center)
        } else if t.eq_ignore_ascii_case("right") || t == "r" || t == "end" {
            Some(Self::Right)
        } else {
            None
        }
    }

    pub const fn ooxml_algn(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "ctr",
            Self::Right => "r",
        }
    }
}

/// Placeholder semantic role for a slide textbox element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlidePlaceholderRole {
    Title,
    Subtitle,
    Body,
}

impl SlidePlaceholderRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Subtitle => "subtitle",
            Self::Body => "body",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("title") {
            Some(Self::Title)
        } else if t.eq_ignore_ascii_case("subtitle") {
            Some(Self::Subtitle)
        } else if t.eq_ignore_ascii_case("body") {
            Some(Self::Body)
        } else {
            None
        }
    }
}

/// Geometric shape preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideShapeType {
    #[default]
    Rect,
    RoundRect,
    Ellipse,
    RightArrow,
    Callout,
    Diamond,
}

impl SlideShapeType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::RoundRect => "roundRect",
            Self::Ellipse => "ellipse",
            Self::RightArrow => "rightArrow",
            Self::Callout => "callout",
            Self::Diamond => "diamond",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("rect") || t.eq_ignore_ascii_case("rectangle") {
            Some(Self::Rect)
        } else if t.eq_ignore_ascii_case("roundRect") || t.eq_ignore_ascii_case("round-rectangle") {
            Some(Self::RoundRect)
        } else if t.eq_ignore_ascii_case("ellipse") {
            Some(Self::Ellipse)
        } else if t.eq_ignore_ascii_case("rightArrow") || t.eq_ignore_ascii_case("right-arrow") {
            Some(Self::RightArrow)
        } else if t.eq_ignore_ascii_case("callout")
            || t.eq_ignore_ascii_case("wedgeRectCallout")
            || t.eq_ignore_ascii_case("rectangular-callout")
        {
            Some(Self::Callout)
        } else if t.eq_ignore_ascii_case("diamond") {
            Some(Self::Diamond)
        } else {
            None
        }
    }

    pub const fn ooxml_prst(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::RoundRect => "roundRect",
            Self::Ellipse => "ellipse",
            Self::RightArrow => "rightArrow",
            Self::Callout => "wedgeRectCallout",
            Self::Diamond => "diamond",
        }
    }

    pub const fn odf_type(self) -> &'static str {
        match self {
            Self::Rect => "rectangle",
            Self::RoundRect => "round-rectangle",
            Self::Ellipse => "ellipse",
            Self::RightArrow => "right-arrow",
            Self::Callout => "rectangular-callout",
            Self::Diamond => "diamond",
        }
    }
}

/// Element bounding box in typographic points (`pt`), origin at top-left of the slide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlideBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl SlideBox {
    pub fn validate(self) -> Result<(), EasyOfficeError> {
        if !self.x.is_finite() || !self.y.is_finite() || !self.w.is_finite() || !self.h.is_finite()
        {
            return Err(EasyOfficeError::InvalidArgument(
                "슬라이드 개체 좌표에 유한하지 않은 값이 포함되어 있습니다.".to_string(),
            ));
        }
        if !(-MAX_SLIDE_COORD_PT..=MAX_SLIDE_COORD_PT).contains(&self.x)
            || !(-MAX_SLIDE_COORD_PT..=MAX_SLIDE_COORD_PT).contains(&self.y)
            || !(0.0..=MAX_SLIDE_COORD_PT).contains(&self.w)
            || !(0.0..=MAX_SLIDE_COORD_PT).contains(&self.h)
        {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "슬라이드 개체 좌표/크기({:.1}, {:.1}, {:.1}x{:.1})가 허용 범위(-{MAX_SLIDE_COORD_PT}..={MAX_SLIDE_COORD_PT}, 0..={MAX_SLIDE_COORD_PT})를 벗어났습니다.",
                self.x, self.y, self.w, self.h
            )));
        }
        Ok(())
    }

    pub fn scaled(self, sx: f64, sy: f64) -> Self {
        Self {
            x: (self.x * sx).clamp(-MAX_SLIDE_COORD_PT, MAX_SLIDE_COORD_PT),
            y: (self.y * sy).clamp(-MAX_SLIDE_COORD_PT, MAX_SLIDE_COORD_PT),
            w: (self.w * sx).clamp(0.0, MAX_SLIDE_COORD_PT),
            h: (self.h * sy).clamp(0.0, MAX_SLIDE_COORD_PT),
        }
    }
}

/// Series values inside a slide chart element.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideChartSeries {
    pub name: String,
    pub values: Vec<f64>,
}

/// Positioned slide canvas element (kept in Z-order).
#[derive(Debug, Clone, PartialEq)]
pub enum SlideElement {
    TextBox {
        id: String,
        rect: SlideBox,
        text: String,
        font_size_pt: f64,
        bold: bool,
        italic: bool,
        underline: bool,
        align: SlideTextAlign,
        color: Option<String>,
        fill: Option<String>,
        role: Option<SlidePlaceholderRole>,
    },
    Shape {
        id: String,
        rect: SlideBox,
        shape_type: SlideShapeType,
        text: String,
        fill: Option<String>,
        stroke: Option<String>,
        stroke_width_pt: f64,
        color: Option<String>,
        font_size_pt: f64,
        bold: bool,
        italic: bool,
        underline: bool,
        align: SlideTextAlign,
    },
    Image {
        id: String,
        rect: SlideBox,
        image: RichImage,
        alt: String,
    },
    Table {
        id: String,
        rect: SlideBox,
        rows: Vec<Vec<String>>,
        header_row: bool,
        fill: Option<String>,
    },
    Chart {
        id: String,
        rect: SlideBox,
        chart_type: ChartKind,
        title: String,
        categories: Vec<String>,
        series: Vec<SlideChartSeries>,
        legend: ChartLegendPos,
    },
}

impl SlideElement {
    pub fn id(&self) -> &str {
        match self {
            Self::TextBox { id, .. }
            | Self::Shape { id, .. }
            | Self::Image { id, .. }
            | Self::Table { id, .. }
            | Self::Chart { id, .. } => id,
        }
    }

    pub fn rect(&self) -> SlideBox {
        match self {
            Self::TextBox { rect, .. }
            | Self::Shape { rect, .. }
            | Self::Image { rect, .. }
            | Self::Table { rect, .. }
            | Self::Chart { rect, .. } => *rect,
        }
    }

    pub fn validate(&self) -> Result<(), EasyOfficeError> {
        self.rect().validate()?;
        if self.id().len() > MAX_SLIDE_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "슬라이드 개체 ID 길이가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
        match self {
            Self::TextBox {
                text,
                font_size_pt,
                color,
                fill,
                ..
            } => {
                check_text_len(text, "글상자 텍스트")?;
                check_font_pt(*font_size_pt)?;
                check_opt_hex(color.as_deref(), "color")?;
                check_opt_hex(fill.as_deref(), "fill")?;
            }
            Self::Shape {
                text,
                fill,
                stroke,
                stroke_width_pt,
                color,
                font_size_pt,
                ..
            } => {
                check_text_len(text, "도형 텍스트")?;
                check_font_pt(*font_size_pt)?;
                if !stroke_width_pt.is_finite()
                    || !(0.0..=MAX_SLIDE_STROKE_PT).contains(stroke_width_pt)
                {
                    return Err(EasyOfficeError::InvalidArgument(format!(
                        "도형 선 두께는 0~{MAX_SLIDE_STROKE_PT}pt여야 합니다."
                    )));
                }
                check_opt_hex(fill.as_deref(), "fill")?;
                check_opt_hex(stroke.as_deref(), "stroke")?;
                check_opt_hex(color.as_deref(), "color")?;
            }
            Self::Image { alt, .. } => {
                check_text_len(alt, "이미지 대체 텍스트")?;
            }
            Self::Table { rows, fill, .. } => {
                check_opt_hex(fill.as_deref(), "fill")?;
                if rows.is_empty() || rows.len() > MAX_SLIDE_TABLE_ROWS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "슬라이드 표 행 수({})는 1~{MAX_SLIDE_TABLE_ROWS} 범위여야 합니다.",
                        rows.len()
                    )));
                }
                let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
                if cols == 0 || cols > MAX_SLIDE_TABLE_COLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "슬라이드 표 열 수({cols})는 1~{MAX_SLIDE_TABLE_COLS} 범위여야 합니다."
                    )));
                }
                for row in rows {
                    for cell in row {
                        check_text_len(cell, "슬라이드 표 셀")?;
                    }
                }
            }
            Self::Chart {
                title,
                categories,
                series,
                ..
            } => {
                check_text_len(title, "슬라이드 차트 제목")?;
                if categories.len() > MAX_SLIDE_CHART_POINTS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "슬라이드 차트 항목 수({})가 상한({MAX_SLIDE_CHART_POINTS})을 초과했습니다.",
                        categories.len()
                    )));
                }
                for cat in categories {
                    check_text_len(cat, "슬라이드 차트 항목")?;
                }
                if series.is_empty() || series.len() > MAX_SLIDE_CHART_SERIES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "슬라이드 차트 계열 수({})는 1~{MAX_SLIDE_CHART_SERIES} 범위여야 합니다.",
                        series.len()
                    )));
                }
                for s in series {
                    check_text_len(&s.name, "슬라이드 차트 계열 이름")?;
                    if s.values.len() > MAX_SLIDE_CHART_POINTS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "슬라이드 차트 데이터 포인트 수({})가 상한({MAX_SLIDE_CHART_POINTS})을 초과했습니다.",
                            s.values.len()
                        )));
                    }
                    if s.values.iter().any(|v| !v.is_finite()) {
                        return Err(EasyOfficeError::InvalidArgument(
                            "슬라이드 차트 값에는 유한한 숫자만 허용됩니다.".to_string(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

/// One slide with legacy title/bullets/notes plus master layout, transition, and positioned elements.
#[derive(Debug, Clone, PartialEq)]
pub struct RichSlide {
    pub title: String,
    pub bullets: Vec<String>,
    pub notes: String,
    pub layout: SlideMasterLayout,
    pub transition: SlideTransition,
    pub elements: Vec<SlideElement>,
    pub ext: SlideExt,
}

impl Default for RichSlide {
    fn default() -> Self {
        Self {
            title: String::new(),
            bullets: Vec::new(),
            notes: String::new(),
            layout: SlideMasterLayout::TitleAndContent,
            transition: SlideTransition::None,
            elements: Vec::new(),
            ext: SlideExt::default(),
        }
    }
}

/// Presentation deck carrying aspect ratio, slideshow mode, and rich slides.
#[derive(Debug, Clone, PartialEq)]
pub struct RichSlideDeck {
    pub title: String,
    pub is_slideshow: bool,
    pub aspect: SlideAspect,
    pub orientation: SlideOrientation,
    pub slides: Vec<RichSlide>,
    pub elements_rejected: u32,
    pub settings: DeckSettings,
}

impl Default for RichSlideDeck {
    fn default() -> Self {
        Self {
            title: String::new(),
            is_slideshow: false,
            aspect: SlideAspect::Widescreen16x9,
            orientation: SlideOrientation::Landscape,
            slides: Vec::new(),
            elements_rejected: 0,
            settings: DeckSettings::default(),
        }
    }
}

impl RichSlideDeck {
    pub const fn stage_dimensions_pt(&self) -> (f64, f64) {
        let (w, h) = self.aspect.dimensions_pt();
        match self.orientation {
            SlideOrientation::Landscape => (w, h),
            SlideOrientation::Portrait => (h, w),
        }
    }

    pub const fn stage_dimensions_emu(&self) -> (u64, u64) {
        let (w, h) = self.aspect.dimensions_emu();
        match self.orientation {
            SlideOrientation::Landscape => (w, h),
            SlideOrientation::Portrait => (h, w),
        }
    }

    pub const fn stage_dimensions_cm(&self) -> (f64, f64) {
        let (w, h) = self.aspect.dimensions_cm();
        match self.orientation {
            SlideOrientation::Landscape => (w, h),
            SlideOrientation::Portrait => (h, w),
        }
    }

    /// Builds a [`RichSlideDeck`] from legacy `(title, bullets, notes)` triples.
    pub fn from_legacy_slides(
        title: impl Into<String>,
        is_slideshow: bool,
        slides: Vec<DecodedSlide>,
    ) -> Self {
        Self {
            title: title.into(),
            is_slideshow,
            aspect: SlideAspect::Widescreen16x9,
            orientation: SlideOrientation::Landscape,
            slides: slides
                .into_iter()
                .map(|(s_title, bullets, notes)| RichSlide {
                    title: s_title,
                    bullets,
                    notes,
                    layout: SlideMasterLayout::TitleAndContent,
                    transition: SlideTransition::None,
                    elements: Vec::new(),
                    ext: SlideExt::default(),
                })
                .collect(),
            elements_rejected: 0,
            settings: DeckSettings::default(),
        }
    }

    /// Projects each slide down to `(title, bullets, notes)` for legacy consumers.
    pub fn to_legacy_slides(&self) -> Vec<DecodedSlide> {
        self.slides
            .iter()
            .map(|slide| {
                let title = if !slide.title.is_empty() {
                    slide.title.clone()
                } else {
                    slide
                        .elements
                        .iter()
                        .find_map(|el| match el {
                            SlideElement::TextBox {
                                role: Some(SlidePlaceholderRole::Title),
                                text,
                                ..
                            } if !text.trim().is_empty() => Some(text.clone()),
                            _ => None,
                        })
                        .unwrap_or_default()
                };
                let bullets = if !slide.bullets.is_empty() {
                    slide.bullets.clone()
                } else {
                    slide
                        .elements
                        .iter()
                        .filter_map(|el| match el {
                            SlideElement::TextBox {
                                role: Some(SlidePlaceholderRole::Body),
                                text,
                                ..
                            } if !text.trim().is_empty() => Some(text.clone()),
                            _ => None,
                        })
                        .collect()
                };
                (title, bullets, slide.notes.clone())
            })
            .collect()
    }

    /// Enforces all fail-closed slide, element, image, table, chart, and text budgets.
    pub fn validate(&self) -> Result<(), EasyOfficeError> {
        if self.slides.len() > MAX_SLIDES_PER_DECK {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "슬라이드 수({})가 허용 상한({MAX_SLIDES_PER_DECK})을 초과했습니다.",
                self.slides.len()
            )));
        }
        check_text_len(&self.title, "프레젠테이션 제목")?;
        let mut total_elements = 0usize;
        let (mut total_image_bytes, mut seen_bg) = (0usize, HashSet::new());
        for slide in &self.slides {
            check_text_len(&slide.title, "슬라이드 제목")?;
            check_text_len(&slide.notes, "슬라이드 발표자 노트")?;
            for bullet in &slide.bullets {
                check_text_len(bullet, "슬라이드 본문")?;
            }
            if slide.elements.len() > MAX_ELEMENTS_PER_SLIDE {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "슬라이드당 개체 수({})가 상한({MAX_ELEMENTS_PER_SLIDE})을 초과했습니다.",
                    slide.elements.len()
                )));
            }
            total_elements = total_elements.saturating_add(slide.elements.len());
            if total_elements > MAX_ELEMENTS_PER_DECK {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "덱 전체 개체 수({total_elements})가 상한({MAX_ELEMENTS_PER_DECK})을 초과했습니다."
                )));
            }
            for el in &slide.elements {
                el.validate()?;
                if let SlideElement::Image { image, .. } = el {
                    total_image_bytes = total_image_bytes.saturating_add(image.bytes().len());
                }
            }
            let ids: HashSet<&str> = slide.elements.iter().map(SlideElement::id).collect();
            slide.ext.validate(&ids)?;
            total_image_bytes =
                total_image_bytes.saturating_add(slide.ext.new_background_bytes(&mut seen_bg));
            if total_image_bytes > MAX_SLIDE_TOTAL_IMAGE_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "슬라이드 전체 이미지 크기({total_image_bytes}B)가 상한({MAX_SLIDE_TOTAL_IMAGE_BYTES}B)을 초과했습니다."
                )));
            }
        }
        Ok(())
    }
}

pub fn pt_to_emu(pt: f64) -> u64 {
    (pt.clamp(0.0, MAX_SLIDE_COORD_PT) * EMU_PER_PT).round() as u64
}

pub fn pt_to_signed_emu(pt: f64) -> i64 {
    (pt.clamp(-MAX_SLIDE_COORD_PT, MAX_SLIDE_COORD_PT) * EMU_PER_PT).round() as i64
}

/// Validates slide/deck/image resource limits before appending a decoded [`SlideElement`].
pub(crate) fn push_checked_slide_element(
    el: SlideElement,
    els: &mut Vec<SlideElement>,
    total_el: &mut usize,
    total_img_bytes: &mut usize,
) -> Result<(), EasyOfficeError> {
    if els.len() >= MAX_ELEMENTS_PER_SLIDE {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드당 개체 수가 상한({MAX_ELEMENTS_PER_SLIDE})을 초과했습니다."
        )));
    }
    *total_el += 1;
    if *total_el > MAX_ELEMENTS_PER_DECK {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "덱 전체 개체 수가 상한({MAX_ELEMENTS_PER_DECK})을 초과했습니다."
        )));
    }
    if let SlideElement::Image { image, .. } = &el {
        *total_img_bytes = total_img_bytes.saturating_add(image.bytes().len());
        if *total_img_bytes > MAX_SLIDE_TOTAL_IMAGE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "슬라이드 전체 이미지 크기가 상한(128 MiB)을 초과했습니다.".to_string(),
            ));
        }
    }
    els.push(el);
    Ok(())
}

pub fn emu_to_pt(emu: f64) -> f64 {
    emu / EMU_PER_PT
}

/// Parses an ODF/CSS length (`120pt`, `4.2cm`, `42mm`, `1.5in`, `96px`, or bare pt) into points.
pub fn parse_length_pt(raw: &str) -> Option<f64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let (num_str, factor) = if let Some(n) = s.strip_suffix("pt") {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix("cm") {
        (n, PT_PER_CM)
    } else if let Some(n) = s.strip_suffix("mm") {
        (n, PT_PER_MM)
    } else if let Some(n) = s.strip_suffix("in") {
        (n, PT_PER_IN)
    } else if let Some(n) = s.strip_suffix("px") {
        (n, 0.75)
    } else {
        (s, 1.0)
    };
    let v = num_str.trim().parse::<f64>().ok()?;
    (v.is_finite()).then_some(v * factor)
}

/// Normalizes a table to a rectangle (`1..=64` rows x `1..=32` cols) padded with empty strings.
pub fn normalize_table_rows(
    mut rows: Vec<Vec<String>>,
) -> Result<Vec<Vec<String>>, EasyOfficeError> {
    if rows.is_empty() || rows.len() > MAX_SLIDE_TABLE_ROWS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 표 행 수({})는 1~{MAX_SLIDE_TABLE_ROWS} 범위여야 합니다.",
            rows.len()
        )));
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 || cols > MAX_SLIDE_TABLE_COLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 표 열 수({cols})는 1~{MAX_SLIDE_TABLE_COLS} 범위여야 합니다."
        )));
    }
    for row in &mut rows {
        for cell in row.iter() {
            check_text_len(cell, "슬라이드 표 셀")?;
        }
        if row.len() < cols {
            row.resize(cols, String::new());
        }
    }
    Ok(rows)
}

fn check_text_len(text: &str, label: &str) -> Result<(), EasyOfficeError> {
    if text.len() > MAX_SLIDE_TEXT_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "{label} 길이({}B)가 상한({MAX_SLIDE_TEXT_BYTES}B)을 초과했습니다.",
            text.len()
        )));
    }
    Ok(())
}

fn check_font_pt(pt: f64) -> Result<(), EasyOfficeError> {
    if !pt.is_finite() || !(MIN_SLIDE_FONT_PT..=MAX_SLIDE_FONT_PT).contains(&pt) {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "슬라이드 글자 크기는 {MIN_SLIDE_FONT_PT}~{MAX_SLIDE_FONT_PT}pt여야 합니다."
        )));
    }
    Ok(())
}

fn check_opt_hex(val: Option<&str>, field: &str) -> Result<(), EasyOfficeError> {
    if let Some(raw) = val {
        if normalize_hex_color(raw).is_none() {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "슬라이드 '{field}'는 RRGGBB/#RRGGBB 색이어야 합니다: {raw}"
            )));
        }
    }
    Ok(())
}
