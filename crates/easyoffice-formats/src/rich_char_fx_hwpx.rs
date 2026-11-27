//! HWPX (KS X 6101) words of the text effects of PD-376: the underline shape and colour (`hh:underline` `type`, `shape`,
//! `color`) and the emphasis mark (`hh:charPr` `symMark`).
//!
//! Reference implementation: rhwp (https://github.com/edwardkim/rhwp, MIT, Copyright (c) 2025-2026 Edward Kim), the 13 line
//! shape tokens (`src/serializer/hwpx/char_shape_tables/line_shape.rs` lines 3-17), the 7 emphasis tokens
//! (`char_shape_tables/emphasis.rs` lines 3-11), the underline type tokens (`char_shape_tables/underline.rs` lines 6-26) and the
//! readers of `type`, `shape` and `symMark` (`src/parser/hwpx/header.rs` lines 630-642 and 734-768). Pinned commit
//! 1a76570e833917d15817415a53c09ad61ab3203f. The token tables are ported; the rest is written for this model: rhwp keeps an
//! unchecked colour word and an unbounded shape number, here the 17 underline names and the 5 marks are indexes, a colour is
//! six hex digits or none, and every read value is checked.
//!
//! The 17 `ST_Underline` names and the 13 line shapes only partly coincide, so both directions are a fixed table (PD-379):
//! the heavy names and `words` take the plain shape, and the four shapes with no name (`CIRCLE`, `SLIM_THICK`, `THICK_SLIM`,
//! `SLIM_THICK_SLIM`) take the nearest one. Likewise the 5 marks and the 7 tokens.

use crate::rich_char_fx::{TextFx, MAX_EMPHASIS, MAX_UNDERLINE_STYLE};
use crate::rich_document::normalize_hex_color;
use crate::xml_stream::{attr, Attrs};

/// The line shapes of `hh:underline@shape` in the order of the reference table (id 0 is the plain line).
const LINE_SHAPES: [&str; 13] = [
    "SOLID",
    "DASH",
    "DOT",
    "DASH_DOT",
    "DASH_DOT_DOT",
    "LONG_DASH",
    "CIRCLE",
    "DOUBLE_SLIM",
    "SLIM_THICK",
    "THICK_SLIM",
    "SLIM_THICK_SLIM",
    "WAVE",
    "DOUBLE_WAVE",
];
/// The line shape id of each underline name (`single`, `words`, `double`, `thick`, `dotted`, `dottedHeavy`, `dash`,
/// `dashedHeavy`, `dashLong`, `dashLongHeavy`, `dotDash`, `dashDotHeavy`, `dotDotDash`, `dashDotDotHeavy`, `wave`,
/// `wavyHeavy`, `wavyDouble`).
const SHAPE_OF_STYLE: [u8; 17] = [0, 0, 7, 0, 2, 2, 1, 1, 5, 5, 3, 3, 4, 4, 11, 11, 12];
/// The underline name of each line shape: `CIRCLE` is dotted, a thick and thin pair is thick, the triple line is double.
const STYLE_OF_SHAPE: [u8; 13] = [0, 6, 4, 10, 12, 8, 4, 2, 3, 3, 2, 14, 16];

/// The `symMark` tokens in the order of the reference table (id 0 is none).
const SYM_MARKS: [&str; 7] = [
    "NONE",
    "DOT_ABOVE",
    "RING_ABOVE",
    "TILDE",
    "CARON",
    "SIDE",
    "COLON",
];
/// The `symMark` id of each emphasis (none, dot, comma, circle, dot below): the accent marks (`TILDE`, `CARON`) are the comma,
/// and the dot below is the side dot.
const MARK_OF_EMPHASIS: [u8; 5] = [0, 1, 3, 2, 5];
/// The emphasis of each `symMark` id: `COLON` is two dots, so a dot.
const EMPHASIS_OF_MARK: [u8; 7] = [0, 1, 3, 2, 2, 4, 1];

/// `hh:underline@type` tokens that draw a line under the text (the reference parser reads only these; `TOP` is a line above
/// the text, which the model has no place for, so it is drawn below).
fn is_underline_type(token: &str) -> bool {
    matches!(token, "BOTTOM" | "TOP")
}

fn index_of(table: &[&str], token: Option<&str>) -> Option<usize> {
    let token = token?;
    table.iter().position(|t| *t == token)
}

impl TextFx {
    /// The fields HWPX stores: the underline style and colour of an underlined run, and the emphasis. Two runs that differ
    /// only in the rest share one `hh:charPr`.
    pub(crate) fn hwpx_kept(&self, underlined: bool) -> Self {
        Self {
            emphasis: self.emphasis.min(MAX_EMPHASIS),
            underline_style: if underlined {
                self.underline_style.min(MAX_UNDERLINE_STYLE)
            } else {
                0
            },
            underline_color: if underlined {
                self.underline_color
            } else {
                None
            },
            ..Self::default()
        }
    }

    /// `hh:underline@shape` of an underlined run.
    pub(crate) fn hwpx_line_shape(&self) -> &'static str {
        let style = usize::from(self.underline_style.min(MAX_UNDERLINE_STYLE));
        LINE_SHAPES[usize::from(SHAPE_OF_STYLE[style])]
    }

    /// `hh:charPr@symMark`.
    pub(crate) fn hwpx_sym_mark(&self) -> &'static str {
        let emphasis = usize::from(self.emphasis.min(MAX_EMPHASIS));
        SYM_MARKS[usize::from(MARK_OF_EMPHASIS[emphasis])]
    }

    /// Reads `hh:charPr@symMark`; an unknown token is no mark.
    pub(crate) fn read_hwpx_sym_mark(&mut self, token: Option<&str>) {
        self.emphasis = index_of(&SYM_MARKS, token).map_or(0, |i| EMPHASIS_OF_MARK[i]);
    }

    /// Reads an `hh:underline` element and returns whether the run is underlined. An unknown `type` is no line, an unknown
    /// `shape` is the plain line, and a `color` that is not six hex digits, or that is the text colour (`text_color`, `None`
    /// black), is automatic.
    pub(crate) fn read_hwpx_underline(
        &mut self,
        attrs: &Attrs<'_>,
        text_color: Option<&str>,
    ) -> bool {
        self.underline_style = 0;
        self.underline_color = None;
        if !attr(attrs, "type")
            .as_deref()
            .is_some_and(is_underline_type)
        {
            return false;
        }
        let shape = attr(attrs, "shape");
        self.underline_style =
            index_of(&LINE_SHAPES, shape.as_deref()).map_or(0, |i| STYLE_OF_SHAPE[i]);
        let color = attr(attrs, "color")
            .as_deref()
            .and_then(normalize_hex_color)
            .filter(|c| c.as_str() != text_color.unwrap_or("000000"));
        self.underline_color = color.and_then(|c| u32::from_str_radix(&c, 16).ok());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_cover_every_name_and_every_token() {
        for (style, shape) in SHAPE_OF_STYLE.iter().enumerate() {
            assert!(usize::from(*shape) < LINE_SHAPES.len(), "{style}");
            // The name a shape reads back as writes the same shape again.
            assert_eq!(
                SHAPE_OF_STYLE[usize::from(STYLE_OF_SHAPE[usize::from(*shape)])],
                *shape,
                "{style}"
            );
        }
        for style in STYLE_OF_SHAPE {
            assert!(style <= MAX_UNDERLINE_STYLE);
        }
        for (emphasis, mark) in MARK_OF_EMPHASIS.iter().enumerate() {
            assert_eq!(usize::from(EMPHASIS_OF_MARK[usize::from(*mark)]), emphasis);
        }
        assert!(EMPHASIS_OF_MARK.iter().all(|e| *e <= MAX_EMPHASIS));
        assert_eq!(SYM_MARKS[0], "NONE");
    }

    #[test]
    fn a_stale_style_is_not_kept_for_a_run_without_a_line() {
        let fx = TextFx {
            underline_style: 14,
            underline_color: Some(0xFF_0000),
            emphasis: 3,
            hidden: true,
            ..TextFx::default()
        };
        assert_eq!(
            fx.hwpx_kept(false),
            TextFx {
                emphasis: 3,
                ..TextFx::default()
            }
        );
        assert_eq!(fx.hwpx_kept(true).underline_style, 14);
        assert!(!fx.hwpx_kept(true).hidden);
    }
}
