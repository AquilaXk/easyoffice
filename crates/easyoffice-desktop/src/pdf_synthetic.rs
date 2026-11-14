//! Synthetic bold and italic for the exported PDF (PD-131).
//!
//! A page asks for bold or italic text, but the face usvg picks for it is often a regular upright
//! one: the bundled Noto Sans KR is Regular only, and a document face that is not installed falls
//! back to it. Without help the text would come out in a normal weight and upright. This module
//! decides per `<text>` element, from the faces usvg really selected, whether to add what the face
//! lacks to the SVG before the PDF conversion, as the word processor's own PDF output does:
//!
//! * bold: a stroke in the fill colour, 2% of the font size wide, painted after the fill. svg2pdf
//!   writes fill and stroke of one colour as a single glyph run (`Tr 2`), so the text is still
//!   emitted once and searches and copies exactly;
//! * italic: a shear of 0.25 about the text's baseline, with the layout and the advances unchanged.
//!
//! A real bold or italic face is never emphasised again, and mixed text (bold and regular faces in
//! one element, several colours, vertical text, text on a path) is left as it is: shearing or
//! stroking the whole element would also change the parts that are right.
//!
//! Ported from the rhwp PDF renderer (`src/renderer/pdf_synthetic_bold.rs`, whole file, and
//! `src/renderer/pdf_synthetic_italic.rs`, whole file; rhwp 0.8.7 at `1a76570`; MIT, Copyright (c)
//! 2025-2026 Edward Kim, see THIRD_PARTY_NOTICES.md). Changes: both decisions come from one parse
//! of the page and one edit pass, so a page costs one parse when nothing needs synthesis (the port
//! parses a page up to three times) and two when it does; the work is skipped for a page that does
//! not ask for bold or italic at all (detected in a single-pass 32-bit ASCII-lowercased word scan);
//! glyph face flags (`Faces`) and XML node request flags (`own | inherited | inside`) are packed
//! into single-byte bitmasks; an element count, a one-pass id prefix and a duplicate id check keep
//! a hostile page from costing more than its size; nothing panics on odd markup.

use std::borrow::Cow;
use std::collections::HashMap;
use usvg::fontdb::{Database, Style};

const SVG_NS: &str = "http://www.w3.org/2000/svg";
/// Weights from this one on count as bold (the CSS "semi-bold" step), as usvg's own bold test.
const BOLD_WEIGHT: u16 = 600;
/// Width of the synthetic bold stroke in em: the ratio the word processor's PDF output uses for
/// regular-only faces (2.66 pt at 133 pt).
const BOLD_STROKE_EM: f32 = 0.02;
/// Slant of the synthetic italic (about 14 degrees).
const SHEAR: f32 = 0.25;
/// Most `<text>` elements of a page that are looked at. A page with more is converted as it is:
/// the styling is an improvement, not a requirement, and the cost must not grow with hostile input.
const MAX_TEXTS: usize = 50_000;
/// Start of the ids given to elements that have none.
const ID_PREFIX: &str = "eo-pdf-style-";

// Bit-packed glyph face flags (`Faces`).
const FACE_PLAIN_WEIGHT: u8 = 1 << 0;
const FACE_BOLD: u8 = 1 << 1;
const FACE_UPRIGHT: u8 = 1 << 2;
const FACE_SLANTED: u8 = 1 << 3;

// Bit-packed XML node emphasis request flags.
const NODE_OWN: u8 = 1 << 0;
const NODE_INHERITED: u8 = 1 << 1;
const NODE_INSIDE: u8 = 1 << 2;

// 32-bit little-endian ASCII-lowercased word constants for single-pass emphasis detection.
const WORD_BOLD: u32 = u32::from_le_bytes(*b"bold");
const WORD_ITAL: u32 = u32::from_le_bytes(*b"ital");
const WORD_OBLI: u32 = u32::from_le_bytes(*b"obli");
const WORD_FONT: u32 = u32::from_le_bytes(*b"font");
const HALF_IC: u16 = u16::from_le_bytes(*b"ic");

/// What happened to the text elements of one page; for tests and diagnostics, no text or names.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub bold: usize,
    pub italic: usize,
    /// Elements that ask for bold or italic and could not be synthesised safely.
    pub unsupported: usize,
}

enum Bold {
    Unchanged,
    Stroke {
        color: usvg::Color,
        opacity: f32,
        width: f32,
    },
    Unsupported,
}

enum Italic {
    Unchanged,
    /// Shear about this baseline (y of the glyphs in the element's own coordinates).
    Shear(f32),
    Unsupported,
}

struct Decision {
    bold: Bold,
    italic: Italic,
}

/// Bit-packed faces of the glyphs of one element (`FACE_*` bits): whether some are regular/upright
/// and some bold/slanted, counted over glyphs that are not white space. `None` when a glyph's face
/// is not in the database.
#[derive(Clone, Copy)]
struct Faces(u8);

impl Faces {
    #[inline]
    const fn has(self, bit: u8) -> bool {
        (self.0 & bit) != 0
    }
}

fn faces_of(text: &usvg::Text, db: &Database) -> Option<Faces> {
    let mut bits = 0u8;
    for span in text.layouted().iter().filter(|span| span.visible) {
        for glyph in &span.positioned_glyphs {
            if glyph.text.chars().all(char::is_whitespace) {
                continue;
            }
            let face = db.face(glyph.font)?;
            bits |= if face.weight.0 >= BOLD_WEIGHT {
                FACE_BOLD
            } else {
                FACE_PLAIN_WEIGHT
            };
            bits |= if face.style == Style::Normal {
                FACE_UPRIGHT
            } else {
                FACE_SLANTED
            };
        }
    }
    Some(Faces(bits))
}

/// Bold decision of an element that asks for a bold weight in some visible span.
fn decide_bold(text: &usvg::Text, faces: Option<Faces>) -> Bold {
    let Some(faces) = faces else {
        return Bold::Unsupported;
    };
    if !faces.has(FACE_PLAIN_WEIGHT) {
        return Bold::Unchanged;
    }
    // The page's text runs are uniform clusters. Arbitrary mixed text must never embolden a real
    // bold face, or a regular span together with a fallback.
    if faces.has(FACE_BOLD) || visible_spans(text).any(|span| span.font().weight() < BOLD_WEIGHT) {
        return Bold::Unsupported;
    }
    let mut paint: Option<(usvg::Color, f32, f32)> = None;
    for span in text.layouted().iter().filter(|span| span.visible) {
        let Some(fill) = &span.fill else {
            return Bold::Unsupported;
        };
        let usvg::Paint::Color(color) = fill.paint() else {
            return Bold::Unsupported;
        };
        if span.stroke.is_some() {
            return Bold::Unchanged;
        }
        let current = (*color, fill.opacity().get(), span.font_size.get());
        if paint.is_some_and(|previous| previous != current) {
            return Bold::Unsupported;
        }
        paint = Some(current);
    }
    match paint {
        Some((color, opacity, size)) => Bold::Stroke {
            color,
            opacity,
            width: size * BOLD_STROKE_EM,
        },
        None => Bold::Unchanged,
    }
}

fn visible_spans(text: &usvg::Text) -> impl Iterator<Item = &usvg::TextSpan> {
    text.chunks()
        .iter()
        .flat_map(|chunk| chunk.spans())
        .filter(|span| span.is_visible())
}

/// Italic decision of an element that asks for a slanted style in some visible span.
fn decide_italic(text: &usvg::Text, faces: Option<Faces>) -> Italic {
    let Some(faces) = faces else {
        return Italic::Unsupported;
    };
    if !faces.has(FACE_UPRIGHT) {
        return Italic::Unchanged;
    }
    // One transform for the whole element is right only when every visible glyph needs the same
    // synthesis on one horizontal baseline. Genuine italic faces and upright parts of mixed text
    // are never sheared.
    let mut baseline: Option<f32> = None;
    let mut uniform = true;
    for span in text.layouted().iter().filter(|span| span.visible) {
        for glyph in &span.positioned_glyphs {
            if glyph.text.chars().all(char::is_whitespace) {
                continue;
            }
            let ts = glyph.transform();
            uniform &= ts.kx == 0.0
                && ts.ky == 0.0
                && ts.ty.is_finite()
                && baseline.is_none_or(|y| (y - ts.ty).abs() < 0.001);
            baseline.get_or_insert(ts.ty);
        }
    }
    if faces.has(FACE_SLANTED)
        || visible_spans(text).any(|span| span.font().style() == usvg::FontStyle::Normal)
        || !uniform
        || text.writing_mode() != usvg::WritingMode::LeftToRight
        || text
            .chunks()
            .iter()
            .any(|chunk| !matches!(chunk.text_flow(), usvg::TextFlow::Linear))
    {
        return Italic::Unsupported;
    }
    baseline.map_or(Italic::Unchanged, Italic::Shear)
}

fn collect(group: &usvg::Group, db: &Database, decisions: &mut HashMap<String, Decision>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => collect(group, db, decisions),
            usvg::Node::Text(text) => {
                let bold = visible_spans(text).any(|span| span.font().weight() >= BOLD_WEIGHT);
                let italic =
                    visible_spans(text).any(|span| span.font().style() != usvg::FontStyle::Normal);
                let faces = (bold || italic).then(|| faces_of(text, db)).flatten();
                let mut decision = Decision {
                    bold: if bold {
                        decide_bold(text, faces)
                    } else {
                        Bold::Unchanged
                    },
                    italic: if italic {
                        decide_italic(text, faces)
                    } else {
                        Italic::Unchanged
                    },
                };
                // Two elements with one id cannot be told apart: neither is touched.
                if decisions.contains_key(text.id()) {
                    decision = Decision {
                        bold: Bold::Unsupported,
                        italic: Italic::Unsupported,
                    };
                }
                decisions.insert(text.id().to_owned(), decision);
            }
            _ => {}
        }
    }
}

/// Whether the page mentions bold or italic at all, case-insensitively and loosely (an attribute
/// or a style declaration), in a single 32-bit word-folded pass: a page that does not is parsed
/// once and untouched.
fn asks_for_emphasis(svg: &str) -> bool {
    let bytes = svg.as_bytes();
    let len = bytes.len();
    if len < 4 {
        return false;
    }
    let end = len - 3;
    let mut i = 0;
    while i < end {
        let w =
            u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) | 0x2020_2020;
        if w == WORD_BOLD {
            return true;
        }
        if w == WORD_ITAL && i + 6 <= len {
            let tail = u16::from_le_bytes([bytes[i + 4], bytes[i + 5]]) | 0x2020;
            if tail == HALF_IC {
                return true;
            }
        }
        if w == WORD_OBLI
            && i + 7 <= len
            && (bytes[i + 4] | 0x20) == b'q'
            && (bytes[i + 5] | 0x20) == b'u'
            && (bytes[i + 6] | 0x20) == b'e'
        {
            return true;
        }
        if w == WORD_FONT && i + 11 <= len && bytes[i + 4] == b'-' {
            let w2 = u32::from_le_bytes([bytes[i + 5], bytes[i + 6], bytes[i + 7], bytes[i + 8]])
                | 0x2020_2020;
            let tail = u16::from_le_bytes([bytes[i + 9], bytes[i + 10]]) | 0x2020;
            if w2 == u32::from_le_bytes(*b"weig")
                && tail == u16::from_le_bytes(*b"ht")
                && bytes[i + 11..]
                    .iter()
                    .find(|b| !matches!(b, b'=' | b':' | b'"' | b'\'' | b' '))
                    .is_some_and(|b| (b'6'..=b'9').contains(b))
            {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Position just after the tag name of an element, where an attribute can be inserted.
fn attribute_position(svg: &str, node: roxmltree::Node<'_, '_>) -> Option<usize> {
    let start = node.range().start.checked_add(1)?;
    let name_len = svg
        .get(start..)?
        .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')?;
    Some(start + name_len)
}

/// `svg` with each `(position, text)` inserted. Equal positions keep their order.
fn insertions(svg: &str, mut edits: Vec<(usize, String)>) -> String {
    edits.sort_by_key(|edit| edit.0);
    let mut result =
        String::with_capacity(svg.len() + edits.iter().map(|e| e.1.len()).sum::<usize>());
    let mut cursor = 0;
    for (at, value) in edits {
        result.push_str(&svg[cursor..at]);
        result.push_str(&value);
        cursor = at;
    }
    result.push_str(&svg[cursor..]);
    result
}

/// An id prefix no element's id starts with: [`ID_PREFIX`] plus one more `_` than the longest run
/// of `_` that follows it in any existing id (one pass).
fn free_prefix(xml: &roxmltree::Document) -> String {
    let longest = xml
        .descendants()
        .filter_map(|node| node.attribute("id")?.strip_prefix(ID_PREFIX))
        .map(|rest| rest.bytes().take_while(|b| *b == b'_').count())
        .max();
    let mut prefix = String::from(ID_PREFIX);
    if let Some(longest) = longest {
        prefix.extend(std::iter::repeat_n('_', longest + 1));
    }
    prefix
}

fn style_declaration(color: usvg::Color, opacity: f32, width: f32) -> String {
    format!(
        "stroke:rgb({},{},{});stroke-opacity:{opacity};stroke-width:{width};stroke-linejoin:round;paint-order:fill stroke",
        color.red, color.green, color.blue
    )
}

#[inline]
fn mentions(attribute: Option<&str>, word: &str) -> bool {
    let word_bytes = word.as_bytes();
    attribute.is_some_and(|value| {
        value
            .as_bytes()
            .windows(word_bytes.len())
            .any(|w| w.eq_ignore_ascii_case(word_bytes))
    })
}

/// The edits one element needs, and what they add to `report`.
fn edits_for(
    svg: &str,
    node: roxmltree::Node<'_, '_>,
    decision: &Decision,
    report: &mut Report,
    edits: &mut Vec<(usize, String)>,
) {
    match &decision.bold {
        Bold::Unchanged => {}
        Bold::Unsupported => report.unsupported += 1,
        Bold::Stroke {
            color,
            opacity,
            width,
        } => {
            // A stroke somewhere inside would be overridden by, or fight, the one added here.
            let inner_stroke = node.descendants().skip(1).any(|child| {
                child.attribute("stroke").is_some() || mentions(child.attribute("style"), "stroke")
            });
            let declaration = style_declaration(*color, *opacity, *width);
            let edit = match node.attribute_node("style") {
                // An inline style overrides an inherited stroke and keeps the existing style.
                Some(attr) => Some((attr.range_value().end, format!(";{declaration}"))),
                None => attribute_position(svg, node)
                    .map(|at| (at, format!(" style=\"{declaration}\""))),
            };
            match edit {
                Some(edit) if !inner_stroke => {
                    edits.push(edit);
                    report.bold += 1;
                }
                _ => report.unsupported += 1,
            }
        }
    }
    match &decision.italic {
        Italic::Unchanged => {}
        Italic::Unsupported => report.unsupported += 1,
        Italic::Shear(baseline) => {
            // A CSS transform would win over the attribute written here (the editor's own text
            // uses attributes), so the shear would be claimed but not drawn.
            let matrix = format!("matrix(1 0 -{SHEAR} 1 {} 0)", baseline * SHEAR);
            let edit = match node.attribute_node("transform") {
                Some(attr) => Some((attr.range_value().end, format!(" {matrix}"))),
                None => {
                    attribute_position(svg, node).map(|at| (at, format!(" transform=\"{matrix}\"")))
                }
            };
            match edit {
                Some(edit) if !mentions(node.attribute("style"), "transform") => {
                    edits.push(edit);
                    report.italic += 1;
                }
                _ => report.unsupported += 1,
            }
        }
    }
}

/// Whether one attribute asks for a bold weight or a slanted style.
fn attribute_asks(name: &str, value: &str) -> bool {
    match name {
        "font-weight" => {
            let value = value.trim();
            value.eq_ignore_ascii_case("bold")
                || value.eq_ignore_ascii_case("bolder")
                || value
                    .parse::<u16>()
                    .is_ok_and(|weight| weight >= BOLD_WEIGHT)
        }
        "font-style" => asks_for_emphasis(value),
        "style" | "font" => asks_for_emphasis(value),
        _ => false,
    }
}

/// For each of `texts`, whether the element, an element above it (the font properties are
/// inherited) or one inside it (a `<tspan>`) asks for bold or italic. Uses a single bit-packed
/// `Vec<u8>` (`NODE_OWN | NODE_INHERITED | NODE_INSIDE`) so attributes are inspected once per node.
fn text_requests(xml: &roxmltree::Document, texts: &[roxmltree::Node]) -> Vec<bool> {
    let nodes: Vec<_> = xml.descendants().collect();
    let slots = nodes.len() + 1;
    let mut flags = vec![0u8; slots];
    // Forward pass in document order: inspect attributes once and propagate `NODE_INHERITED` down.
    for node in &nodes {
        let id = node.id().get_usize();
        let own = node
            .attributes()
            .any(|attr| attribute_asks(attr.name(), attr.value()));
        let above = node
            .parent()
            .is_some_and(|parent| (flags[parent.id().get_usize()] & NODE_INHERITED) != 0);
        let mut f = if own {
            NODE_OWN | NODE_INHERITED | NODE_INSIDE
        } else {
            0
        };
        if above {
            f |= NODE_INHERITED;
        }
        flags[id] = f;
    }
    // Reverse pass: propagate `NODE_INSIDE` up without re-inspecting attributes.
    for node in nodes.iter().rev() {
        let id = node.id().get_usize();
        if (flags[id] & NODE_INSIDE) != 0 {
            if let Some(parent) = node.parent() {
                flags[parent.id().get_usize()] |= NODE_INSIDE;
            }
        }
    }
    texts
        .iter()
        .map(|text| (flags[text.id().get_usize()] & (NODE_INHERITED | NODE_INSIDE)) != 0)
        .collect()
}

/// Whether the page has a style sheet: classes could then give any element a bold weight, so no
/// element can be ruled out by its attributes.
fn has_style_sheet(xml: &roxmltree::Document) -> bool {
    xml.descendants()
        .any(|node| node.has_tag_name((SVG_NS, "style")))
}

/// Parses a page SVG into a tree whose bold and italic text is synthesised where the selected
/// faces lack it.
///
/// A page that asks for no emphasis (or has more than [`MAX_TEXTS`] text elements) is parsed once,
/// as it is. Otherwise a reduced copy of the page is parsed to see which faces usvg selects for
/// the elements that ask: the other text elements and the pictures are hidden in that copy (they
/// are the expensive part of a parse and decide nothing), and each element gets an id to be found
/// by. Only the page as it is, or with the edits the decisions call for, is parsed in full, so a
/// page costs one full parse and a small one.
pub fn parse_page(svg: &str, options: &usvg::Options<'_>) -> Result<(usvg::Tree, Report), String> {
    // The page comes from the page script: a declaration would make the parser expand entities
    // without a bound, and no page the editor draws has one.
    easyoffice_formats::XmlSanitizer::validate_safe_xml(svg).map_err(|e| e.to_string())?;
    crate::svg_budget::check_svg_structure(svg).map_err(str::to_string)?;
    let parse = |source: &str| usvg::Tree::from_str(source, options).map_err(|e| e.to_string());
    let mut report = Report::default();
    if !asks_for_emphasis(svg) {
        return Ok((parse(svg)?, report));
    }
    let xml = roxmltree::Document::parse(svg).map_err(|e| e.to_string())?;
    let texts: Vec<_> = xml
        .descendants()
        .filter(|node| node.has_tag_name((SVG_NS, "text")))
        .take(MAX_TEXTS + 1)
        .collect();
    let hide = !has_style_sheet(&xml);
    let asks = if hide {
        text_requests(&xml, &texts)
    } else {
        vec![true; texts.len()]
    };
    if texts.len() > MAX_TEXTS || !asks.contains(&true) {
        return Ok((parse(svg)?, report));
    }

    let prefix = free_prefix(&xml);
    let mut generated = 0;
    let ids: Vec<Cow<str>> = texts
        .iter()
        .map(|text| match text.attribute("id") {
            Some(id) => Cow::Borrowed(id),
            None => {
                generated += 1;
                Cow::Owned(format!("{prefix}{}", generated - 1))
            }
        })
        .collect();
    let mut probe_edits = Vec::new();
    for ((text, id), asks) in texts.iter().zip(&ids).zip(&asks) {
        let Some(at) = attribute_position(svg, *text) else {
            continue;
        };
        if text.attribute("id").is_none() {
            probe_edits.push((at, format!(" id=\"{id}\"")));
        }
        if !asks && text.attribute("display").is_none() {
            probe_edits.push((at, " display=\"none\"".to_string()));
        }
    }
    // The pictures decide nothing about the faces, and the options carry the page's picture budget,
    // which the full parse spends again: a picture decoded here would be charged twice.
    for picture in xml
        .descendants()
        .filter(|node| node.has_tag_name((SVG_NS, "image")))
        .filter(|node| node.attribute("display").is_none())
    {
        if let Some(at) = attribute_position(svg, picture) {
            probe_edits.push((at, " display=\"none\"".to_string()));
        }
    }
    let probe = insertions(svg, probe_edits);
    let probe_tree = parse(&probe)?;
    let mut decisions = HashMap::new();
    collect(probe_tree.root(), probe_tree.fontdb(), &mut decisions);

    let mut edits = Vec::new();
    for ((text, id), asks) in texts.iter().zip(&ids).zip(&asks) {
        if let (true, Some(decision)) = (asks, decisions.get(id.as_ref())) {
            edits_for(svg, *text, decision, &mut report, &mut edits);
        }
    }
    if edits.is_empty() {
        Ok((parse(svg)?, report))
    } else {
        Ok((parse(&insertions(svg, edits))?, report))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_page_that_asks_for_emphasis_is_analysed() {
        for yes in [
            r#"<text font-weight="bold">A</text>"#,
            r#"<text font-weight='700'>A</text>"#,
            r#"<text style="font-weight: 900">A</text>"#,
            r#"<text style="FONT-WEIGHT:600">A</text>"#,
            r#"<text font-style="italic">A</text>"#,
            r#"<text font-style="Oblique">A</text>"#,
        ] {
            assert!(asks_for_emphasis(yes), "{yes}");
        }
        for no in [
            r#"<text font-weight="500">A</text>"#,
            r#"<text font-weight="normal">A</text>"#,
            r#"<text font-weight="400" x="600">A</text>"#,
            r#"<text>A</text>"#,
            "",
        ] {
            assert!(!asks_for_emphasis(no), "{no}");
        }
    }

    #[test]
    fn insertions_keep_the_order_of_edits_at_one_position() {
        let edited = insertions(
            "<text/>",
            vec![(5, " b".into()), (5, " c".into()), (2, " a".into())],
        );
        assert_eq!(edited, "<t aext b c/>");
    }

    #[test]
    fn the_id_prefix_is_longer_than_any_run_after_it() {
        let svg = format!(
            r#"<svg xmlns="{SVG_NS}"><g id="{ID_PREFIX}"/><g id="{ID_PREFIX}____x"/><g id="other"/></svg>"#
        );
        let xml = roxmltree::Document::parse(&svg).expect("xml");
        assert_eq!(free_prefix(&xml), format!("{ID_PREFIX}_____"));
        let empty = format!(r#"<svg xmlns="{SVG_NS}"/>"#);
        let none = roxmltree::Document::parse(&empty).expect("xml");
        assert_eq!(free_prefix(&none), ID_PREFIX);
    }
}
