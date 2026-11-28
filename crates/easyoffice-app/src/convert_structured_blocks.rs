//! The paragraph and block parts of the structured document JSON (PD-401), both ways.
//!
//! Paragraph: `"tabs"` (`[{"pos","align","leader"}]`), `"border"` (`top`/`right`/`bottom`/`left` of `{"style","eighths",
//! "color","space"}`), `"shade"` (fill color), `"dropCap"` (`{"lines","margin"}`), `"columnBreak"`, `"columnLook"`
//! (`{"separator","narrow"}`), `"textBoxes"` (`[{"widthEmu","heightEmu","wrap","fill","border","paragraphs"}]`) and
//! `"styleName"`. Run: `"math"` (the OMML of an equation). Document: `"styles"` (the styles of the user) and
//! `"docColumnSeparator"` / `"docColumnNarrow"` next to `"docColumns"`. A value that is not one the formats keep (see
//! `easyoffice_formats::rich_word_blocks`) rejects the save instead of being dropped.

use std::fmt::Write;

use easyoffice_formats::json_out::push_json_string as push_json_str;
use easyoffice_formats::rich_word_blocks::{
    checked_omml, is_reserved_style_name, normalize_tab_stops, RichBorderLine, RichBorderStyle,
    RichColumnLook, RichDropCap, RichNarrowColumn, RichParaBorders, RichParagraphWord,
    RichStyleDef, RichTabAlign, RichTabLeader, RichTabStop, RichTextBox, MAX_CUSTOM_STYLES,
    MAX_MATHS, MAX_TAB_POSITION_TWIPS, MAX_TAB_STOPS, MAX_TEXT_BOXES, MAX_TEXT_BOX_EMU,
    MAX_TEXT_BOX_PARAGRAPHS,
};
use easyoffice_formats::{
    EasyOfficeError, RichAlignment, RichDocument, RichImageWrap, RichParagraph, RichRun,
};

use super::{
    as_array, as_object, field, invalid, opt_bool, opt_bounded, opt_color, opt_font,
    opt_size_half_pt, opt_str, parse_alignment, parse_paragraph, Budget, JsonObject,
};
use crate::line_spacing_json::{parse_line_spacing, write_line_spacing_json};

/// The OMML of a run (`"math"`): checked against the list of elements and bounds of the reader. A save with an equation
/// that fails the check is refused.
pub(crate) fn parse_run_math(
    run: &JsonObject,
    budget: &mut Budget,
) -> Result<Option<String>, EasyOfficeError> {
    let Some(raw) = opt_str(run, "math")?.filter(|m| !m.is_empty()) else {
        return Ok(None);
    };
    budget.maths += 1;
    if budget.maths > MAX_MATHS {
        return Err(invalid("구조화 저장: 수식이 문서당 한도를 넘었습니다."));
    }
    checked_omml(raw)
        .map(|(omml, _)| Some(omml))
        .ok_or_else(|| invalid("구조화 저장: 'math'는 크기·깊이 한도 안의 OMML 수식이어야 합니다."))
}

fn enum_of<T>(
    obj: &JsonObject,
    key: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>, EasyOfficeError> {
    match opt_str(obj, key)?.filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(name) => parse(name)
            .map(Some)
            .ok_or_else(|| invalid(format!("구조화 저장: '{key}'의 알 수 없는 값 '{name}'"))),
    }
}

fn parse_tabs(obj: &JsonObject) -> Result<Vec<RichTabStop>, EasyOfficeError> {
    let Some(value) = field(obj, "tabs") else {
        return Ok(Vec::new());
    };
    let items = as_array(value, "tabs")?;
    if items.len() > MAX_TAB_STOPS {
        return Err(invalid("구조화 저장: 탭 정지가 문단당 한도를 넘었습니다."));
    }
    let mut stops = Vec::with_capacity(items.len());
    for item in items {
        let stop = as_object(item, "tabs")?;
        stops.push(RichTabStop {
            pos_twips: opt_bounded(stop, "pos", 0, i64::from(MAX_TAB_POSITION_TWIPS))?
                .ok_or_else(|| invalid("구조화 저장: 탭 정지에는 'pos'가 필요합니다."))?
                as u32,
            align: enum_of(stop, "align", RichTabAlign::from_wire_name)?.unwrap_or_default(),
            leader: enum_of(stop, "leader", RichTabLeader::from_wire_name)?.unwrap_or_default(),
        });
    }
    Ok(normalize_tab_stops(stops))
}

fn parse_border_side(
    obj: &JsonObject,
    key: &str,
) -> Result<Option<RichBorderLine>, EasyOfficeError> {
    let Some(value) = field(obj, key) else {
        return Ok(None);
    };
    let side = as_object(value, key)?;
    RichBorderLine {
        style: enum_of(side, "style", RichBorderStyle::from_wire_name)?.unwrap_or_default(),
        eighths: opt_bounded(side, "eighths", 0, 255)?.unwrap_or(4) as u8,
        color: opt_color(side, "color")?,
        space_pt: opt_bounded(side, "space", 0, 255)?.unwrap_or(0) as u8,
    }
    .checked()
    .map(Some)
    .ok_or_else(|| {
        invalid(format!(
            "구조화 저장: 문단 테두리 '{key}'가 범위를 벗어났습니다."
        ))
    })
}

fn parse_borders(obj: &JsonObject) -> Result<Option<RichParaBorders>, EasyOfficeError> {
    let Some(value) = field(obj, "border") else {
        return Ok(None);
    };
    let sides = as_object(value, "border")?;
    let borders = RichParaBorders {
        top: parse_border_side(sides, "top")?,
        right: parse_border_side(sides, "right")?,
        bottom: parse_border_side(sides, "bottom")?,
        left: parse_border_side(sides, "left")?,
    };
    Ok((!borders.is_empty()).then_some(borders))
}

fn parse_drop_cap(obj: &JsonObject) -> Result<Option<RichDropCap>, EasyOfficeError> {
    let Some(value) = field(obj, "dropCap") else {
        return Ok(None);
    };
    let cap = as_object(value, "dropCap")?;
    RichDropCap {
        lines: opt_bounded(cap, "lines", 0, 255)?.unwrap_or(3) as u8,
        in_margin: opt_bool(cap, "margin")?,
    }
    .checked()
    .map(Some)
    .ok_or_else(|| invalid("구조화 저장: 'dropCap.lines'는 2~10이어야 합니다."))
}

/// `"columnLook"` of a paragraph, or the document's `"docColumnSeparator"` and `"docColumnNarrow"`.
fn parse_look(
    obj: &JsonObject,
    separator_key: &str,
    narrow_key: &str,
) -> Result<Option<RichColumnLook>, EasyOfficeError> {
    let look = RichColumnLook {
        separator: opt_bool(obj, separator_key)?,
        narrow: enum_of(obj, narrow_key, RichNarrowColumn::from_wire_name)?.unwrap_or_default(),
    };
    Ok((!look.is_default()).then_some(look))
}

/// The look of the columns the document-wide column setting gives (`docColumns` is the count).
pub(crate) fn parse_doc_column_look(
    entries: &JsonObject,
) -> Result<Option<RichColumnLook>, EasyOfficeError> {
    parse_look(entries, "docColumnSeparator", "docColumnNarrow")
}

fn parse_text_box(item: &JsonObject, budget: &mut Budget) -> Result<RichTextBox, EasyOfficeError> {
    if budget.in_text_box {
        return Err(invalid(
            "구조화 저장: 텍스트 상자 안에 텍스트 상자를 둘 수 없습니다.",
        ));
    }
    budget.text_boxes += 1;
    if budget.text_boxes > MAX_TEXT_BOXES {
        return Err(invalid(
            "구조화 저장: 텍스트 상자가 문서당 한도를 넘었습니다.",
        ));
    }
    let items = as_array(
        field(item, "paragraphs")
            .ok_or_else(|| invalid("구조화 저장: 텍스트 상자에는 'paragraphs'가 필요합니다."))?,
        "paragraphs",
    )?;
    if items.len() > MAX_TEXT_BOX_PARAGRAPHS {
        return Err(invalid(
            "구조화 저장: 텍스트 상자의 문단이 한도를 넘었습니다.",
        ));
    }
    let mut paragraphs = Vec::with_capacity(items.len());
    budget.in_text_box = true;
    for paragraph in items {
        let mut parsed = parse_paragraph(as_object(paragraph, "paragraphs")?, budget);
        if let Ok(p) = parsed.as_mut() {
            p.word.column_look = None;
        }
        paragraphs.push(parsed?);
    }
    budget.in_text_box = false;
    let side = |key: &str| -> Result<u32, EasyOfficeError> {
        Ok(opt_bounded(item, key, 0, i64::from(MAX_TEXT_BOX_EMU))?.unwrap_or(0) as u32)
    };
    RichTextBox {
        paragraphs,
        width_emu: side("widthEmu")?,
        height_emu: side("heightEmu")?,
        wrap: enum_of(item, "wrap", RichImageWrap::from_wire_name)?
            .unwrap_or(RichImageWrap::Square),
        fill: opt_color(item, "fill")?,
        border: opt_bool(item, "border")?,
    }
    .checked()
    .ok_or_else(|| invalid("구조화 저장: 텍스트 상자의 크기가 범위를 벗어났습니다."))
}

/// What a paragraph of the payload carries of PD-401.
pub(crate) fn parse_paragraph_blocks(
    obj: &JsonObject,
    budget: &mut Budget,
) -> Result<RichParagraphWord, EasyOfficeError> {
    let mut text_boxes = Vec::new();
    if let Some(value) = field(obj, "textBoxes") {
        for item in as_array(value, "textBoxes")? {
            text_boxes.push(parse_text_box(as_object(item, "textBoxes")?, budget)?);
        }
    }
    let style_name = opt_str(obj, "styleName")?
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    if style_name.as_deref().is_some_and(|n| {
        n.chars().count() > easyoffice_formats::rich_word_blocks::MAX_STYLE_NAME_CHARS
    }) {
        return Err(invalid("구조화 저장: 'styleName'이 너무 깁니다."));
    }
    Ok(RichParagraphWord {
        tab_stops: parse_tabs(obj)?,
        borders: parse_borders(obj)?,
        shading: opt_color(obj, "shade")?,
        drop_cap: parse_drop_cap(obj)?,
        column_break_before: opt_bool(obj, "columnBreak")?,
        column_look: match field(obj, "columnLook") {
            None => None,
            Some(v) => parse_look(as_object(v, "columnLook")?, "separator", "narrow")?,
        },
        text_boxes,
        style_name,
    })
}

/// `"styles"`: the styles of the user, each checked; a name the word processor owns, a repeated name and more than
/// [`MAX_CUSTOM_STYLES`] reject the save.
pub(crate) fn parse_doc_styles(entries: &JsonObject) -> Result<Vec<RichStyleDef>, EasyOfficeError> {
    let Some(value) = field(entries, "styles") else {
        return Ok(Vec::new());
    };
    let items = as_array(value, "styles")?;
    if items.len() > MAX_CUSTOM_STYLES {
        return Err(invalid("구조화 저장: 스타일이 문서당 한도를 넘었습니다."));
    }
    let mut styles: Vec<RichStyleDef> = Vec::with_capacity(items.len());
    for item in items {
        let obj = as_object(item, "styles")?;
        let def = RichStyleDef {
            // White space around the name is not part of it (the page takes the name the same way).
            name: opt_str(obj, "name")?.unwrap_or_default().trim().to_string(),
            font: opt_font(obj, "font")?,
            size_half_pt: opt_size_half_pt(obj)?,
            bold: opt_bool(obj, "bold")?,
            italic: opt_bool(obj, "italic")?,
            underline: opt_bool(obj, "underline")?,
            color: opt_color(obj, "color")?,
            alignment: parse_alignment(obj)?,
            left_indent_twips: opt_bounded(obj, "leftIndentTwips", 0, 288_000)?.map(|v| v as u32),
            first_line_indent_twips: opt_bounded(obj, "firstLineIndentTwips", -288_000, 288_000)?
                .map(|v| v as i32),
            space_before_twips: opt_bounded(obj, "spaceBeforeTwips", 0, 8000)?.map(|v| v as u32),
            space_after_twips: opt_bounded(obj, "spaceAfterTwips", 0, 8000)?.map(|v| v as u32),
            line_spacing: parse_line_spacing(obj)?,
        };
        let name = def.name.clone();
        let def = def
            .checked()
            .filter(|_| !is_reserved_style_name(&name) && !styles.iter().any(|s| s.name == name))
            .ok_or_else(|| invalid(format!("구조화 저장: 사용할 수 없는 스타일 이름 '{name}'")))?;
        styles.push(def);
    }
    Ok(styles)
}

// ---------------------------------------------------------------- open JSON

fn write_line(out: &mut String, key: &str, line: &RichBorderLine) {
    let _ = write!(
        out,
        "\"{key}\":{{\"style\":\"{}\",\"eighths\":{},\"space\":{}",
        line.style.wire_name(),
        line.eighths,
        line.space_pt
    );
    if let Some(color) = &line.color {
        let _ = write!(out, ",\"color\":\"{color}\"");
    }
    out.push('}');
}

/// `"math"` of a run that holds an equation.
pub(crate) fn write_run_math(out: &mut String, run: &RichRun) {
    if let Some(math) = &run.math {
        out.push_str(",\"math\":");
        push_json_str(out, math);
    }
}

/// The paragraph keys of PD-401 of the open JSON, each when the paragraph has it. `write_paragraph` writes a paragraph of a
/// text box.
pub(crate) fn write_paragraph_blocks(
    out: &mut String,
    p: &RichParagraph,
    write_paragraph: fn(&mut String, &RichParagraph),
) {
    let w = &p.word;
    if !w.tab_stops.is_empty() {
        out.push_str(",\"tabs\":[");
        for (i, s) in w.tab_stops.iter().enumerate() {
            let _ = write!(
                out,
                "{}{{\"pos\":{},\"align\":\"{}\",\"leader\":\"{}\"}}",
                if i > 0 { "," } else { "" },
                s.pos_twips,
                s.align.wire_name(),
                s.leader.wire_name()
            );
        }
        out.push(']');
    }
    if let Some(b) = w.borders.as_ref().filter(|b| !b.is_empty()) {
        out.push_str(",\"border\":{");
        let mut first = true;
        for (key, side) in [
            ("top", &b.top),
            ("right", &b.right),
            ("bottom", &b.bottom),
            ("left", &b.left),
        ] {
            if let Some(line) = side {
                if !first {
                    out.push(',');
                }
                first = false;
                write_line(out, key, line);
            }
        }
        out.push('}');
    }
    if let Some(fill) = &w.shading {
        let _ = write!(out, ",\"shade\":\"{fill}\"");
    }
    if let Some(cap) = w.drop_cap {
        let _ = write!(
            out,
            ",\"dropCap\":{{\"lines\":{},\"margin\":{}}}",
            cap.lines, cap.in_margin
        );
    }
    if w.column_break_before {
        out.push_str(",\"columnBreak\":true");
    }
    if let Some(look) = w.column_look {
        let _ = write!(
            out,
            ",\"columnLook\":{{\"separator\":{},\"narrow\":\"{}\"}}",
            look.separator,
            look.narrow.wire_name()
        );
    }
    if !w.text_boxes.is_empty() {
        out.push_str(",\"textBoxes\":[");
        for (i, b) in w.text_boxes.iter().enumerate() {
            let _ = write!(
                out,
                "{}{{\"widthEmu\":{},\"heightEmu\":{},\"wrap\":\"{}\",\"border\":{}",
                if i > 0 { "," } else { "" },
                b.width_emu,
                b.height_emu,
                b.wrap.wire_name(),
                b.border
            );
            if let Some(fill) = &b.fill {
                let _ = write!(out, ",\"fill\":\"{fill}\"");
            }
            out.push_str(",\"paragraphs\":[");
            for (j, inner) in b.paragraphs.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                write_paragraph(out, inner);
            }
            out.push_str("]}");
        }
        out.push(']');
    }
    if let Some(name) = &w.style_name {
        out.push_str(",\"styleName\":");
        push_json_str(out, name);
    }
}

/// `,"styles":[…]` and the column look of the open JSON, each when the document has it.
pub(crate) fn write_doc_blocks(out: &mut String, doc: &RichDocument) {
    if !doc.styles.is_empty() {
        out.push_str(",\"styles\":[");
        for (i, s) in doc.styles.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            push_json_str(out, &s.name);
            if let Some(font) = &s.font {
                out.push_str(",\"font\":");
                push_json_str(out, font);
            }
            for (key, value) in [
                ("sizeHalfPt", s.size_half_pt.map(i64::from)),
                ("leftIndentTwips", s.left_indent_twips.map(i64::from)),
                (
                    "firstLineIndentTwips",
                    s.first_line_indent_twips.map(i64::from),
                ),
                ("spaceBeforeTwips", s.space_before_twips.map(i64::from)),
                ("spaceAfterTwips", s.space_after_twips.map(i64::from)),
            ] {
                if let Some(v) = value {
                    let _ = write!(out, ",\"{key}\":{v}");
                }
            }
            for (key, on) in [
                ("bold", s.bold),
                ("italic", s.italic),
                ("underline", s.underline),
            ] {
                if on {
                    let _ = write!(out, ",\"{key}\":true");
                }
            }
            if let Some(color) = &s.color {
                let _ = write!(out, ",\"color\":\"{color}\"");
            }
            if let Some(a) = s.alignment {
                let token = match a {
                    RichAlignment::Left => "left",
                    RichAlignment::Center => "center",
                    RichAlignment::Right => "right",
                    RichAlignment::Justify => "justify",
                };
                let _ = write!(out, ",\"align\":\"{token}\"");
            }
            if let Some(spacing) = s.line_spacing {
                write_line_spacing_json(out, spacing);
            }
            out.push('}');
        }
        out.push(']');
    }
}
