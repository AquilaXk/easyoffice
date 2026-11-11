//! Watermark and approval stamp of a rich-document PDF, drawn into the page SVG (PD-167).
//!
//! The engine's PDF writer puts these two labels into the page content as marker text. Pages that
//! come from rhwp's layout are SVG, so the same two page settings are drawn there instead: the
//! watermark as large grey text across every page, the stamp as a red framed label in the top
//! right corner of the first page. Both are text, so they stay selectable in the PDF. A page whose
//! size cannot be read gets no marks and the export fails (the caller then writes the engine's
//! PDF, which has them), because a PDF that silently lacks a requested watermark is wrong.

use easyoffice_formats::PdfWriteOptions;
use std::fmt::Write;

/// Width of a Latin character relative to a Hangul one (1 em), for fitting a label to a width.
const LATIN_EM: f32 = 0.6;
/// The watermark's longest side as a share of the page diagonal.
const WATERMARK_SPAN: f32 = 0.6;
const WATERMARK_MIN_PX: f32 = 8.0;
const WATERMARK_MAX_PX: f32 = 120.0;
const STAMP_PX: f32 = 16.0;
const STAMP_MARGIN_PX: f32 = 24.0;
const STAMP_PAD_PX: f32 = 8.0;
const FONT: &str = "sans-serif";

/// Draws the marks `options` asks for into `svg`, the page with index `page` (0 is the first).
pub fn mark_page(svg: &mut String, page: usize, options: &PdfWriteOptions) -> Result<(), String> {
    let stamp = options.approval_stamp.as_deref().filter(|_| page == 0);
    if options.watermark.is_none() && stamp.is_none() {
        return Ok(());
    }
    let (width, height) = root_size(svg).ok_or("쪽 크기를 읽지 못했습니다.")?;
    let end = svg
        .rfind("</svg>")
        .ok_or("쪽 SVG의 끝을 찾지 못했습니다.")?;
    let mut marks = String::new();
    if let Some(label) = &options.watermark {
        watermark(&mut marks, label, width, height);
    }
    if let Some(label) = stamp {
        stamp_box(&mut marks, label, width);
    }
    svg.insert_str(end, &marks);
    Ok(())
}

/// `(width, height)` in user units from the root `<svg>` tag.
fn root_size(svg: &str) -> Option<(f32, f32)> {
    let start = svg.find("<svg")?;
    let tag = &svg[start..start + svg[start..].find('>')?];
    let size = |name: &str| -> Option<f32> {
        let key = format!(" {name}=\"");
        let from = tag.find(&key)? + key.len();
        let value: f32 = tag[from..from + tag[from..].find('"')?].parse().ok()?;
        (value.is_finite() && value > 0.0).then_some(value)
    };
    Some((size("width")?, size("height")?))
}

/// Width of `text` in em, counting a Latin character as [`LATIN_EM`] and any other as one.
fn em_width(text: &str) -> f32 {
    text.chars()
        .map(|c| if c.is_ascii() { LATIN_EM } else { 1.0 })
        .sum::<f32>()
        .max(LATIN_EM)
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn watermark(out: &mut String, label: &str, width: f32, height: f32) {
    let diagonal = width.hypot(height);
    let size =
        (diagonal * WATERMARK_SPAN / em_width(label)).clamp(WATERMARK_MIN_PX, WATERMARK_MAX_PX);
    let (cx, cy) = (width / 2.0, height / 2.0);
    // The baseline sits about a third of the size below the visual middle of the line.
    let baseline = cy + size * 0.35;
    let _ = write!(
        out,
        "<text x=\"{cx:.1}\" y=\"{baseline:.1}\" font-family=\"{FONT}\" font-size=\"{size:.1}\" \
         font-weight=\"bold\" fill=\"#808080\" fill-opacity=\"0.25\" text-anchor=\"middle\" \
         transform=\"rotate(-45 {cx:.1} {cy:.1})\">"
    );
    escape(label, out);
    out.push_str("</text>");
}

fn stamp_box(out: &mut String, label: &str, width: f32) {
    let room = (width - 2.0 * (STAMP_MARGIN_PX + STAMP_PAD_PX)).max(WATERMARK_MIN_PX);
    let size = STAMP_PX.min(room / em_width(label));
    let box_width = em_width(label) * size + 2.0 * STAMP_PAD_PX;
    let box_height = size + 2.0 * STAMP_PAD_PX;
    let left = (width - STAMP_MARGIN_PX - box_width).max(0.0);
    let top = STAMP_MARGIN_PX;
    let _ = write!(
        out,
        "<rect x=\"{left:.1}\" y=\"{top:.1}\" width=\"{box_width:.1}\" height=\"{box_height:.1}\" \
         rx=\"3\" fill=\"none\" stroke=\"#C00000\" stroke-width=\"2\"/>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"{FONT}\" font-size=\"{size:.1}\" \
         font-weight=\"bold\" fill=\"#C00000\" text-anchor=\"middle\">",
        left + box_width / 2.0,
        top + STAMP_PAD_PX + size * 0.85,
    );
    escape(label, out);
    out.push_str("</text>");
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"793.7\" height=\"1122.5\" viewBox=\"0 0 793.7 1122.5\">\n<rect width=\"793.7\" height=\"1122.5\" fill=\"#fff\"/>\n</svg>\n";

    fn options(watermark: Option<&str>, stamp: Option<&str>) -> PdfWriteOptions {
        PdfWriteOptions {
            rotation_degrees: 0,
            watermark: watermark.map(str::to_string),
            approval_stamp: stamp.map(str::to_string),
            ..PdfWriteOptions::default()
        }
    }

    fn marked(page: usize, options: &PdfWriteOptions) -> String {
        let mut svg = PAGE.to_string();
        mark_page(&mut svg, page, options).unwrap();
        svg
    }

    #[test]
    fn no_marks_leave_the_page_alone() {
        assert_eq!(marked(0, &options(None, None)), PAGE);
        // The stamp belongs to the first page only.
        assert_eq!(marked(1, &options(None, Some("승인필"))), PAGE);
    }

    #[test]
    fn the_watermark_is_on_every_page_and_the_stamp_on_the_first() {
        let both = options(Some("대외비"), Some("승인필"));
        let first = marked(0, &both);
        let later = marked(3, &both);
        assert!(first.contains(">대외비</text>") && first.contains(">승인필</text>"));
        assert!(later.contains(">대외비</text>") && !later.contains("승인필"));
        for svg in [&first, &later] {
            assert!(svg.ends_with("</svg>\n"), "marks go before the closing tag");
            usvg::Tree::from_str(svg, &usvg::Options::default()).expect("a valid SVG");
        }
    }

    #[test]
    fn labels_are_escaped_and_fitted_to_the_page() {
        let svg = marked(
            0,
            &options(Some("A&B <기밀> \"x\""), Some("가".repeat(200).as_str())),
        );
        assert!(svg.contains("A&amp;B &lt;기밀&gt; &quot;x&quot;"));
        let roxml = roxmltree::Document::parse(&svg).expect("well-formed");
        let stamp_rect = roxml
            .descendants()
            .find(|node| node.has_tag_name("rect") && node.attribute("stroke").is_some())
            .unwrap();
        let left: f32 = stamp_rect.attribute("x").unwrap().parse().unwrap();
        let width: f32 = stamp_rect.attribute("width").unwrap().parse().unwrap();
        assert!(left >= 0.0 && left + width <= 793.7, "{left} + {width}");
    }

    #[test]
    fn a_page_without_a_readable_size_is_refused() {
        for bad in [
            "<svg></svg>",
            "<svg width=\"0\" height=\"5\"></svg>",
            "<svg width=\"5\" height=\"5\">",
        ] {
            let mut svg = bad.to_string();
            assert!(
                mark_page(&mut svg, 0, &options(Some("x"), None)).is_err(),
                "{bad}"
            );
            assert_eq!(svg, bad);
        }
    }
}
