use easyoffice_formats::EasyOfficeError;

/// Target desktop WebView host platform for Tauri v2 cross-platform shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopPlatform {
    MacOsWkWebView,
    WindowsWebView2,
}

impl DesktopPlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::WindowsWebView2
        } else {
            Self::MacOsWkWebView
        }
    }

    pub fn os_label(&self) -> &'static str {
        match self {
            Self::MacOsWkWebView => "macOS (WKWebView)",
            Self::WindowsWebView2 => "Windows (WebView2)",
        }
    }

    pub fn primary_shortcut_modifier(&self) -> &'static str {
        match self {
            Self::MacOsWkWebView => "Cmd",
            Self::WindowsWebView2 => "Ctrl",
        }
    }
}

/// Core office editor modules with semantic accent tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeModuleKind {
    Document,
    Hwp,
    Sheet,
    Slide,
    Pdf,
}

impl OfficeModuleKind {
    pub fn badge_code(&self) -> &'static str {
        match self {
            Self::Document => "DOC",
            Self::Hwp => "HWP",
            Self::Sheet => "SHEET",
            Self::Slide => "SLIDE",
            Self::Pdf => "PDF",
        }
    }

    pub fn accent_hex(&self) -> &'static str {
        match self {
            Self::Document => "#5C6BC0",
            Self::Hwp => "#6E56CF",
            Self::Sheet => "#0A705A",
            Self::Slide => "#D96B27",
            Self::Pdf => "#D9383A",
        }
    }
}

/// Signature Lavender design system color tokens (`docs/design-system.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureLavenderPalette {
    pub primary_accent: &'static str,
    pub brand_hover: &'static str,
    pub light_scaffold: &'static str,
    pub light_surface: &'static str,
    pub light_border: &'static str,
    pub dark_scaffold: &'static str,
    pub dark_surface: &'static str,
    pub dark_border: &'static str,
    pub ink_primary: &'static str,
}

impl Default for SignatureLavenderPalette {
    fn default() -> Self {
        Self {
            primary_accent: "#5C6BC0",
            brand_hover: "#4A58A9",
            light_scaffold: "#F7F8FC",
            light_surface: "#FFFFFF",
            light_border: "#E1E4EE",
            dark_scaffold: "#141724",
            dark_surface: "#1B2032",
            dark_border: "#2C3452",
            ink_primary: "#1F2340",
        }
    }
}

/// Page orientation for Document / HWP layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// 2D rectangle in points (`pt`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl LayoutRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }

    pub fn center_x(&self) -> f64 {
        self.x + self.width * 0.5
    }

    pub fn center_y(&self) -> f64 {
        self.y + self.height * 0.5
    }
}

/// KS X 6101 / HWP 5.0 official header and approval box layout metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct OfficialApprovalBoxLayout {
    pub agency_banner_rect: LayoutRect,
    pub approval_cells: Vec<LayoutRect>,
    pub collaboration_cells: Vec<LayoutRect>,
}

/// Deterministic A4 Document & HWP/HWPX page layout calculator.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentPageLayout {
    pub orientation: PageOrientation,
    pub page_width_pt: f64,
    pub page_height_pt: f64,
    pub margin_pt: f64,
    pub column_count: usize,
    pub column_gap_pt: f64,
    pub column_width_pt: f64,
}

impl DocumentPageLayout {
    pub const A4_WIDTH_PT: f64 = 595.0;
    pub const A4_HEIGHT_PT: f64 = 842.0;
    pub const DEFAULT_MARGIN_PT: f64 = 54.0;
    pub const COLUMN_GAP_PT: f64 = 18.0;

    pub fn compute(
        orientation: PageOrientation,
        column_count: usize,
        margin_pt: f64,
    ) -> Result<Self, EasyOfficeError> {
        if !(1..=4).contains(&column_count) {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "다단 구성 개수는 1..=4 범위여야 합니다: {column_count}"
            )));
        }
        let (page_width_pt, page_height_pt) = match orientation {
            PageOrientation::Portrait => (Self::A4_WIDTH_PT, Self::A4_HEIGHT_PT),
            PageOrientation::Landscape => (Self::A4_HEIGHT_PT, Self::A4_WIDTH_PT),
        };
        let content_width = (page_width_pt - margin_pt * 2.0).max(120.0);
        let total_gaps = Self::COLUMN_GAP_PT * ((column_count - 1) as f64);
        let column_width_pt = (content_width - total_gaps) / (column_count as f64);

        Ok(Self {
            orientation,
            page_width_pt,
            page_height_pt,
            margin_pt,
            column_count,
            column_gap_pt: Self::COLUMN_GAP_PT,
            column_width_pt,
        })
    }

    /// Computes 200-character Korean manuscript grid (원고지 20열 × 10행) cell coordinates.
    pub fn compute_manuscript_grid_200(&self) -> Vec<LayoutRect> {
        let cols = 20usize;
        let rows = 10usize;
        let usable_w = self.page_width_pt - self.margin_pt * 2.0;
        let cell_w = usable_w / (cols as f64);
        let cell_h = cell_w;
        let start_x = self.margin_pt;
        let start_y = self.margin_pt + 72.0;

        let mut cells = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                cells.push(LayoutRect::new(
                    start_x + (c as f64) * cell_w,
                    start_y + (r as f64) * cell_h,
                    cell_w,
                    cell_h,
                ));
            }
        }
        cells
    }

    /// Computes KS X 6101 official document header and 4-slot approval/collaboration box geometry.
    pub fn compute_ksx6101_official_header(&self, slot_count: usize) -> OfficialApprovalBoxLayout {
        let slots = slot_count.clamp(1, 6);
        let usable_w = self.page_width_pt - self.margin_pt * 2.0;
        let agency_banner_rect = LayoutRect::new(self.margin_pt, self.margin_pt, usable_w, 42.0);
        let slot_w = 58.0;
        let slot_h = 48.0;
        let right_edge = self.page_width_pt - self.margin_pt;
        let start_x = right_edge - (slots as f64) * slot_w;
        let approval_y = self.margin_pt + 50.0;
        let collab_y = approval_y + slot_h + 6.0;

        let mut approval_cells = Vec::with_capacity(slots);
        let mut collaboration_cells = Vec::with_capacity(slots);
        for idx in 0..slots {
            let x = start_x + (idx as f64) * slot_w;
            approval_cells.push(LayoutRect::new(x, approval_y, slot_w, slot_h));
            collaboration_cells.push(LayoutRect::new(x, collab_y, slot_w, 34.0));
        }

        OfficialApprovalBoxLayout {
            agency_banner_rect,
            approval_cells,
            collaboration_cells,
        }
    }
}

/// Master color themes for presentation slide decks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideDeckTheme {
    SignatureLavender,
    CorporateNavy,
    ForestEmerald,
    WarmAmber,
    SlateMono,
}

impl SlideDeckTheme {
    pub fn fill_hex(&self) -> &'static str {
        match self {
            Self::SignatureLavender => "#EEF0FF",
            Self::CorporateNavy => "#E8EEF7",
            Self::ForestEmerald => "#E6F4EA",
            Self::WarmAmber => "#FEF3C7",
            Self::SlateMono => "#F1F5F9",
        }
    }

    pub fn border_hex(&self) -> &'static str {
        match self {
            Self::SignatureLavender => "#5C6BC0",
            Self::CorporateNavy => "#1E40AF",
            Self::ForestEmerald => "#047857",
            Self::WarmAmber => "#D97706",
            Self::SlateMono => "#334155",
        }
    }
}

/// Smart alignment snap guide emitted when dragging or aligning slide elements.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapGuide {
    pub is_vertical: bool,
    pub position_pt: f64,
    pub reason: String,
}

/// 16:9 widescreen slide stage layout calculator (`840.0 × 472.5 pt`) with non-overlapping zones.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideStageLayout {
    pub stage_width_pt: f64,
    pub stage_height_pt: f64,
    pub upper_text_zone_max_y: f64,
    pub lower_element_zone_min_y: f64,
    pub theme: SlideDeckTheme,
}

impl SlideStageLayout {
    pub const STAGE_WIDTH_PT: f64 = 840.0;
    pub const STAGE_HEIGHT_PT: f64 = 472.5;
    pub const UPPER_TEXT_ZONE_MAX_Y: f64 = 215.0;
    pub const LOWER_ELEMENT_ZONE_MIN_Y: f64 = 240.0;

    pub fn new(theme: SlideDeckTheme) -> Self {
        Self {
            stage_width_pt: Self::STAGE_WIDTH_PT,
            stage_height_pt: Self::STAGE_HEIGHT_PT,
            upper_text_zone_max_y: Self::UPPER_TEXT_ZONE_MAX_Y,
            lower_element_zone_min_y: Self::LOWER_ELEMENT_ZONE_MIN_Y,
            theme,
        }
    }

    /// Clamps a canvas element into the lower non-overlapping object zone (`y >= 240.0 pt`).
    pub fn clamp_element_to_lower_stage(&self, rect: LayoutRect) -> LayoutRect {
        let clamped_w = rect.width.clamp(40.0, self.stage_width_pt - 48.0);
        let clamped_h = rect
            .height
            .clamp(32.0, self.stage_height_pt - self.lower_element_zone_min_y - 16.0);
        let clamped_x = rect.x.clamp(24.0, self.stage_width_pt - clamped_w - 24.0);
        let clamped_y = rect.y.clamp(
            self.lower_element_zone_min_y,
            self.stage_height_pt - clamped_h - 16.0,
        );
        LayoutRect::new(clamped_x, clamped_y, clamped_w, clamped_h)
    }

    pub fn verify_non_overlapping(&self, elements: &[LayoutRect]) -> bool {
        elements
            .iter()
            .all(|el| el.y >= self.lower_element_zone_min_y && el.max_y() <= self.stage_height_pt)
    }

    pub fn compute_snap_guides(
        &self,
        moving: &LayoutRect,
        siblings: &[LayoutRect],
        tolerance_pt: f64,
    ) -> Vec<SnapGuide> {
        let mut guides = Vec::new();
        let stage_cx = self.stage_width_pt * 0.5;
        let stage_cy = self.stage_height_pt * 0.5;

        if (moving.center_x() - stage_cx).abs() <= tolerance_pt {
            guides.push(SnapGuide {
                is_vertical: true,
                position_pt: stage_cx,
                reason: "stage-center-x".to_string(),
            });
        }
        if (moving.center_y() - stage_cy).abs() <= tolerance_pt {
            guides.push(SnapGuide {
                is_vertical: false,
                position_pt: stage_cy,
                reason: "stage-center-y".to_string(),
            });
        }

        for sib in siblings {
            if (moving.x - sib.x).abs() <= tolerance_pt {
                guides.push(SnapGuide {
                    is_vertical: true,
                    position_pt: sib.x,
                    reason: "sibling-left-edge".to_string(),
                });
            }
            if (moving.y - sib.y).abs() <= tolerance_pt {
                guides.push(SnapGuide {
                    is_vertical: false,
                    position_pt: sib.y,
                    reason: "sibling-top-edge".to_string(),
                });
            }
        }

        guides
    }
}

/// ISO 32000-1 A4 PDF Studio visible upper/middle coordinate layout (`y = 804.0..452.0`).
#[derive(Debug, Clone, PartialEq)]
pub struct PdfStudioLayout {
    pub page_width_pt: f64,
    pub page_height_pt: f64,
    pub header_banner_rect: LayoutRect,
    pub body_text_top_y: f64,
    pub body_text_bottom_y: f64,
    pub highlight_band_rect: LayoutRect,
    pub approval_stamp_rect: LayoutRect,
    pub acroform_field_rect: LayoutRect,
    pub signature_box_rect: LayoutRect,
    pub watermark_anchor: (f64, f64),
    pub stamp_label: String,
}

impl Default for PdfStudioLayout {
    fn default() -> Self {
        Self::compute("APPROVED")
    }
}

impl PdfStudioLayout {
    pub const MIN_VISIBLE_Y: f64 = 452.0;
    pub const MAX_VISIBLE_Y: f64 = 804.0;

    pub fn compute(stamp_label: &str) -> Self {
        Self {
            page_width_pt: 595.28,
            page_height_pt: 841.89,
            header_banner_rect: LayoutRect::new(36.0, 754.0, 523.0, 50.0),
            body_text_top_y: 728.0,
            body_text_bottom_y: 638.0,
            highlight_band_rect: LayoutRect::new(48.0, 607.0, 328.0, 14.0),
            approval_stamp_rect: LayoutRect::new(386.0, 576.0, 160.0, 46.0),
            acroform_field_rect: LayoutRect::new(48.0, 535.0, 240.0, 20.0),
            signature_box_rect: LayoutRect::new(48.0, 452.0, 280.0, 46.0),
            watermark_anchor: (320.0, 746.0),
            stamp_label: stamp_label.to_string(),
        }
    }

    pub fn is_within_upper_middle_viewport(&self) -> bool {
        self.signature_box_rect.y >= Self::MIN_VISIBLE_Y
            && self.header_banner_rect.max_y() <= Self::MAX_VISIBLE_Y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_lavender_palette_and_document_layout() {
        let palette = SignatureLavenderPalette::default();
        assert_eq!(palette.primary_accent, "#5C6BC0");
        assert_eq!(palette.light_scaffold, "#F7F8FC");
        assert_eq!(palette.dark_scaffold, "#141724");

        let portrait = DocumentPageLayout::compute(PageOrientation::Portrait, 2, 54.0).unwrap();
        assert_eq!(portrait.page_width_pt, 595.0);
        assert_eq!(portrait.page_height_pt, 842.0);
        assert!((portrait.column_width_pt - 234.5).abs() < 1e-6);

        let landscape = DocumentPageLayout::compute(PageOrientation::Landscape, 3, 54.0).unwrap();
        assert_eq!(landscape.page_width_pt, 842.0);
        assert_eq!(landscape.page_height_pt, 595.0);

        let grid = portrait.compute_manuscript_grid_200();
        assert_eq!(grid.len(), 200);

        let official = portrait.compute_ksx6101_official_header(4);
        assert_eq!(official.approval_cells.len(), 4);
        assert_eq!(official.collaboration_cells.len(), 4);
    }

    #[test]
    fn test_slide_stage_non_overlap_and_pdf_studio_coordinates() {
        let stage = SlideStageLayout::new(SlideDeckTheme::SignatureLavender);
        let overlapping = LayoutRect::new(60.0, 120.0, 220.0, 110.0);
        let clamped = stage.clamp_element_to_lower_stage(overlapping);
        assert!(clamped.y >= SlideStageLayout::LOWER_ELEMENT_ZONE_MIN_Y);
        assert!(stage.verify_non_overlapping(&[clamped]));

        let centered = LayoutRect::new(310.0, 250.0, 220.0, 100.0);
        let guides = stage.compute_snap_guides(&centered, &[], 4.0);
        assert!(guides.iter().any(|g| g.reason == "stage-center-x"));

        let pdf_layout = PdfStudioLayout::compute("APPROVED");
        assert!(pdf_layout.is_within_upper_middle_viewport());
        assert_eq!(pdf_layout.stamp_label, "APPROVED");
    }
}
