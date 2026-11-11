//! The stamp and watermark laid over the pages of an edited PDF (PD-166).
//!
//! The text is not drawn here. It is written as a small SVG with the text as real text, and the
//! searchable-export pipeline of the shell (`pdf_export.rs`: `svg2pdf` with the bundled Noto Sans KR
//! and the system fonts) turns it into a one-page PDF; that page is copied into the result as a
//! form and placed over the original page. So Korean text works on a machine without any Korean
//! font, embedded font subsets and a text map come from code that is already tested, and no font
//! handling is added.
//!
//! The overlay is drawn upright. A page that is displayed rotated (`/Rotate`) is not: the
//! placement matrix ([`upright_matrix`]) turns the overlay back into the page's own space.

use crate::pdf_edit_budget::MAX_OVERLAY_CHARS;

/// Space between the stamp and the edges of the page (points).
const STAMP_MARGIN: f32 = 36.0;
/// Height of the stamp box (points).
const STAMP_HEIGHT: f32 = 44.0;
const STAMP_FONT_SIZE: f32 = 20.0;
/// What the text of the stamp is wide at least (points).
const STAMP_MIN_WIDTH: f32 = 96.0;
const WATERMARK_MIN_SIZE: f32 = 12.0;
const WATERMARK_MAX_SIZE: f32 = 96.0;
/// Share of the page diagonal the watermark text may take.
const WATERMARK_SPAN: f32 = 0.62;
const WATERMARK_ANGLE: i32 = -45;
const FONT_FAMILY: &str = "Noto Sans KR, sans-serif";
/// SVG pixels per point (the export pipeline reads a pixel as 0.75 point).
const PX_PER_PT: f32 = 96.0 / 72.0;

pub const MSG_OVERLAY_TEXT: &str = "스탬프와 워터마크 문구는 줄바꿈 없이 100자 이내여야 합니다.";

/// What is laid over the pages: a watermark on every page and a stamp on the first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overlay {
    pub watermark: Option<String>,
    pub stamp: Option<String>,
}

impl Overlay {
    /// Checks and trims the texts the page sent; an empty text means "none".
    pub fn new(watermark: Option<&str>, stamp: Option<&str>) -> Result<Self, String> {
        Ok(Self {
            watermark: clean_text(watermark)?,
            stamp: clean_text(stamp)?,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.watermark.is_none() && self.stamp.is_none()
    }

    /// The SVG of an overlay for a page that is shown `width` x `height` points (rotation applied),
    /// or `None` when nothing is to be drawn on it. Only the first page carries the stamp.
    pub fn svg(&self, width: f32, height: f32, first_page: bool) -> Option<String> {
        let stamp = self.stamp.as_deref().filter(|_| first_page);
        if self.watermark.is_none() && stamp.is_none() {
            return None;
        }
        let mut body = String::new();
        if let Some(text) = &self.watermark {
            body.push_str(&watermark_element(text, width, height));
        }
        if let Some(text) = stamp {
            body.push_str(&stamp_elements(text, width));
        }
        Some(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {width} {height}">{body}</svg>"#,
            width * PX_PER_PT,
            height * PX_PER_PT
        ))
    }
}

fn clean_text(text: Option<&str>) -> Result<Option<String>, String> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > MAX_OVERLAY_CHARS || text.chars().any(char::is_control) {
        return Err(MSG_OVERLAY_TEXT.to_string());
    }
    Ok(Some(text.to_string()))
}

/// Width of `text` in em, roughly: Hangul and other wide characters count one, the rest 0.55.
fn width_in_em(text: &str) -> f32 {
    text.chars()
        .map(|c| if c.is_ascii() { 0.55 } else { 1.0 })
        .sum::<f32>()
        .max(1.0)
}

fn watermark_element(text: &str, width: f32, height: f32) -> String {
    let (cx, cy) = (width / 2.0, height / 2.0);
    let diagonal = width.hypot(height);
    let size = (diagonal * WATERMARK_SPAN / width_in_em(text))
        .clamp(WATERMARK_MIN_SIZE, WATERMARK_MAX_SIZE);
    // The baseline sits a little under the centre so the line looks centred.
    let baseline = cy + size * 0.35;
    format!(
        r##"<text x="{cx}" y="{baseline}" font-family="{FONT_FAMILY}" font-size="{size}" fill="#808080" fill-opacity="0.28" text-anchor="middle" transform="rotate({WATERMARK_ANGLE} {cx} {cy})">{}</text>"##,
        xml_escape(text)
    )
}

fn stamp_elements(text: &str, width: f32) -> String {
    let box_width = (width_in_em(text) * STAMP_FONT_SIZE + 32.0).max(STAMP_MIN_WIDTH);
    let left = (width - STAMP_MARGIN - box_width).max(STAMP_MARGIN / 2.0);
    let top = STAMP_MARGIN;
    let (cx, baseline) = (
        left + box_width / 2.0,
        top + STAMP_HEIGHT / 2.0 + STAMP_FONT_SIZE * 0.35,
    );
    format!(
        r##"<rect x="{left}" y="{top}" width="{box_width}" height="{STAMP_HEIGHT}" rx="6" fill="none" stroke="#C0392B" stroke-width="2.5" stroke-opacity="0.85"/><text x="{cx}" y="{baseline}" font-family="{FONT_FAMILY}" font-size="{STAMP_FONT_SIZE}" fill="#C0392B" fill-opacity="0.85" text-anchor="middle">{}</text>"##,
        xml_escape(text)
    )
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// The matrix that carries an upright overlay (origin at the lower left of what is seen) into the
/// space of a page whose own size is `width` x `height` and that is displayed turned `rotation`
/// degrees clockwise (`/Rotate`: 0, 90, 180 or 270).
pub fn upright_matrix(rotation: u16, width: f32, height: f32) -> [f32; 6] {
    match rotation % 360 {
        90 => [0.0, 1.0, -1.0, 0.0, width, 0.0],
        180 => [-1.0, 0.0, 0.0, -1.0, width, height],
        270 => [0.0, -1.0, 1.0, 0.0, 0.0, height],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: [f32; 6], x: f32, y: f32) -> (f32, f32) {
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    /// Where a point of the unrotated page lands on screen when the page is turned clockwise.
    fn displayed(rotation: u16, x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
        match rotation {
            90 => (y, w - x),
            180 => (w - x, h - y),
            270 => (h - y, x),
            _ => (x, y),
        }
    }

    #[test]
    fn the_upright_matrix_undoes_the_display_rotation() {
        let (w, h) = (200.0, 300.0);
        for rotation in [0u16, 90, 180, 270] {
            let m = upright_matrix(rotation, w, h);
            for (x, y) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h), (50.0, 120.0)] {
                // An upright point of the overlay, carried into the page and displayed again, is
                // where it started: the overlay is not turned on screen.
                let (px, py) = apply(m, x, y);
                let (sx, sy) = displayed(rotation, px, py, w, h);
                assert!(
                    (sx - x).abs() < 1e-3 && (sy - y).abs() < 1e-3,
                    "{rotation}: ({x},{y}) -> ({sx},{sy})"
                );
            }
        }
    }

    #[test]
    fn texts_are_trimmed_limited_and_empty_means_none() {
        assert_eq!(Overlay::new(None, None).unwrap(), Overlay::default());
        assert_eq!(
            Overlay::new(Some("  "), Some("")).unwrap(),
            Overlay::default()
        );
        let overlay = Overlay::new(Some(" 대외비 "), Some("승인필")).unwrap();
        assert_eq!(overlay.watermark.as_deref(), Some("대외비"));
        assert!(Overlay::new(Some(&"가".repeat(MAX_OVERLAY_CHARS)), None).is_ok());
        assert!(Overlay::new(Some(&"가".repeat(MAX_OVERLAY_CHARS + 1)), None).is_err());
        assert!(Overlay::new(Some("a\nb"), None).is_err());
        assert!(Overlay::new(None, Some("a\u{0}b")).is_err());
    }

    #[test]
    fn markup_in_the_text_stays_text() {
        let overlay = Overlay::new(Some(r#"<script>&"x"</script>"#), None).unwrap();
        let svg = overlay.svg(300.0, 400.0, true).unwrap();
        assert!(svg.contains("&lt;script&gt;&amp;&quot;x&quot;&lt;/script&gt;"));
        assert!(!svg.contains("<script>"));
    }

    #[test]
    fn only_the_first_page_carries_the_stamp() {
        let overlay = Overlay::new(Some("대외비"), Some("승인필")).unwrap();
        assert!(overlay.svg(300.0, 400.0, true).unwrap().contains("<rect"));
        let later = overlay.svg(300.0, 400.0, false).unwrap();
        assert!(!later.contains("<rect") && later.contains("대외비"));
        let stamp_only = Overlay::new(None, Some("승인필")).unwrap();
        assert!(stamp_only.svg(300.0, 400.0, false).is_none());
        assert!(Overlay::default().svg(300.0, 400.0, true).is_none());
    }

    #[test]
    fn the_svg_is_in_points_and_parses() {
        let overlay = Overlay::new(Some("CONFIDENTIAL"), Some("승인필")).unwrap();
        let svg = overlay.svg(595.0, 842.0, true).unwrap();
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("valid SVG");
        // 595 pt at 96 px per 72 pt.
        assert!((tree.size().width() - 595.0 * PX_PER_PT).abs() < 0.01);
    }
}
