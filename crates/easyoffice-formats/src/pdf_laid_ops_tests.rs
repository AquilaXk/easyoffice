//! The operators of the canvas are what `format!` with `{:.2}` wrote before `pdf_fixed.rs` (PD-397): the same
//! text, byte for byte, for coordinates of every shape a page has.

use super::*;
use std::fmt::Write;

fn coordinates() -> Vec<f64> {
    let mut values = vec![
        0.0,
        -0.0,
        0.005,
        0.125,
        0.5,
        0.75,
        1.5,
        12.345,
        36.0,
        595.276,
        841.89,
        -3.2071,
        1e6 + 0.5,
    ];
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..40 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        values.push((state >> 20) as f64 / 8192.0 - 200.0);
    }
    values
}

#[test]
fn every_operator_reads_as_the_format_macro_wrote_it() {
    let rgb = 0x2563EB;
    let colour = rgb_ops(rgb);
    let values = coordinates();
    let mut canvas = Canvas::new(595.0, 842.0);
    let mut expected: Vec<String> = Vec::new();
    for window in values.windows(5) {
        let (x, y, w, h, width) = (window[0], window[1], window[2], window[3], window[4]);
        canvas.fill_rect(x, y, w, h, rgb);
        expected.push(format!("q {colour} rg {x:.2} {y:.2} {w:.2} {h:.2} re f Q"));
        canvas.stroke_rect(x, y, w, h, rgb, width);
        expected.push(format!(
            "q {width:.2} w {colour} RG {x:.2} {y:.2} {w:.2} {h:.2} re S Q"
        ));
        canvas.stroke_line(x, y, w, h, rgb, width);
        expected.push(format!(
            "q {width:.2} w {colour} RG {x:.2} {y:.2} m {w:.2} {h:.2} l S Q"
        ));
        canvas.push_clip_rect(x, y, w, h);
        expected.push(format!("q {x:.2} {y:.2} {w:.2} {h:.2} re W n"));
        canvas.line(x, y, w, h);
        expected.push(format!("{x:.2} {y:.2} m {w:.2} {h:.2} l S"));
        let pts = [(x, y), (w, h), (width, x), (y, w)];
        canvas.stroke_polyline(&pts, rgb, width);
        let mut line = format!(
            "q {width:.2} w 1 J 1 j {colour} RG {:.2} {:.2} m",
            pts[0].0, pts[0].1
        );
        for (px, py) in &pts[1..] {
            let _ = write!(line, " {px:.2} {py:.2} l");
        }
        expected.push(line + " S Q");
        canvas.fill_polygon(&pts[..3], rgb);
        let mut shape = format!("q {colour} rg {:.2} {:.2} m", pts[0].0, pts[0].1);
        for (px, py) in &pts[1..3] {
            let _ = write!(shape, " {px:.2} {py:.2} l");
        }
        expected.push(shape + " h f Q");
        canvas.text(x, y, width.abs(), "Total 합계");
        canvas.text(x, y, width.abs(), "plain");
    }
    let texts: Vec<_> = canvas
        .ops
        .iter()
        .filter(|op| op.starts_with("BT"))
        .collect();
    assert_eq!(texts.len(), (values.len() - 4) * 2);
    for pair in texts.chunks(2) {
        assert!(
            pair[0].contains(" Tf ") && pair[1].contains(" Tf "),
            "{pair:?}"
        );
    }
    let drawn: Vec<_> = canvas
        .ops
        .iter()
        .filter(|op| !op.starts_with("BT"))
        .collect();
    assert_eq!(drawn.len(), expected.len());
    for (got, want) in drawn.iter().zip(&expected) {
        assert_eq!(got.as_str(), want.as_str());
    }
}

/// The text object of a line the Korean font shows whole and of an ASCII line, as `format!` wrote them.
#[test]
fn the_text_object_reads_as_the_format_macro_wrote_it() {
    use crate::pdf_writer::{text_ops, tj_operand};
    for (x, y, size) in [
        (36.0, 700.5, 10.0),
        (0.0, 0.005, 9.995),
        (595.275_590_551_181, 841.889_763_779_527_6, 11.0),
    ] {
        for text in ["plain", "한글 합계", "A (1) \\ B"] {
            let (font, operand) = tj_operand(text);
            assert_eq!(
                text_ops(x, y, size, text),
                format!("BT {font} {size:.2} Tf {x:.2} {y:.2} Td {operand} Tj ET")
            );
        }
    }
}
