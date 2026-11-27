//! The body text storage: paragraphs, inline table/picture attachments (`\u{FFFC}`), lists,
//! hyperlinks, multi-column layouts and character/paragraph variations.
//!
//! Text and formatting are stored apart. Field 3 is the text (paragraphs separated by `\n`);
//! everything else is a table of entries anchored to UTF-16 positions: paragraph styles (field 5),
//! list levels (field 6), list styles (field 7), character styles (field 8), drawable attachments
//! (field 9), smartfield hyperlinks (field 11), column layout styles (field 12) and section
//! templates (field 17).
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig), `FORMAT.md` "Text — TSWP.StorageArchive"
//! (454-594행)와 "The paragraph-style table has an entry for every paragraph"(728-770행),
//! `src/create.rs` 의 `body_storage`(1058-1088행), `src/drawable.rs`, `src/text.rs`.

use std::collections::HashMap;

use super::package::Blueprint;
use super::styles::{default_half_points, CharLook, ListLook, ParaLook, Styles};
use super::tables::{write_table_attachment, TableWriteBudget};
use super::wire::{float, msg, reference, string, varint, Fld};
use crate::pages_schema::{drawable, geometry, hyperlink, image, storage, ty};
use crate::rich_image::check_document_images;
use crate::rich_notes::{collect_note_ordinals, is_deleted_run};
use crate::rich_table::safe_cell_link_url;
use crate::{
    normalize_hex_color, EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichField,
    RichImage, RichImageFormat, RichLineSpacing, RichListKind, RichNumberEnclosure,
    RichNumberFormat, RichParagraph, RichRun, MAX_LINE_SPACING_TWIPS, MAX_PARA_SPACING_TWIPS,
};

const TYPE_STORAGE: u32 = ty::STORAGE;
/// The reader's own ceilings (`rich_read_pages`): a document it would refuse is not written.
pub(super) const MAX_PARAGRAPHS: usize = 200_000;
pub(super) const MAX_RUNS: usize = 1_000_000;
const MAX_MARK_ENTRIES: usize = 50_000;
const MAX_FONT_NAME_BYTES: usize = 128;
const MAX_HALF_POINTS: u16 = 1_999;
const MAX_INDENT_TWIPS: i32 = 20_000;
const MAX_SPACING_PERCENT: u16 = 1_000;
const EMU_PER_POINT: f32 = 12_700.0;
/// Line break inside a paragraph (what the apps write for Shift+Return).
const LINE_BREAK: char = '\u{2028}';

/// Text as the storage holds it: breaks become line breaks, controls and the object marker go.
pub(super) fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(LINE_BREAK);
            }
            '\n' | '\u{B}' | '\u{C}' | '\u{2028}' | '\u{2029}' => out.push(LINE_BREAK),
            '\t' => out.push(c),
            '\u{FFFC}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Validates a hyperlink target against Pages `TSWP.HyperlinkFieldArchive` schemes (`http`,
/// `https`, `mailto`, `tel`, `ftp`); in-document `#_Toc<N>` anchors are excluded.
pub(super) fn valid_pages_link(raw: &str) -> Option<&str> {
    let url = safe_cell_link_url(raw)?;
    (!url.starts_with('#') && url.len() <= hyperlink::MAX_URL_BYTES).then_some(url)
}

fn parse_rgb(raw: &str) -> Option<[u8; 3]> {
    let hex = normalize_hex_color(raw)?;
    let ch = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some([ch(0)?, ch(2)?, ch(4)?])
}

/// What the writer still drops from a run or a paragraph (counted for the save warning).
pub(super) fn run_loses_format(run: &RichRun) -> bool {
    !run.advanced.is_none()
        || run
            .link
            .as_deref()
            .is_some_and(|u| valid_pages_link(u).is_none())
        || run.field == Some(RichField::FileName)
        || run
            .font
            .as_deref()
            .is_some_and(|f| f.trim().is_empty() || f.trim().len() > MAX_FONT_NAME_BYTES)
        || run.color.as_deref().is_some_and(|c| parse_rgb(c).is_none())
        || run
            .highlight
            .as_deref()
            .is_some_and(|h| parse_rgb(h).is_none())
        || run
            .size_half_pt
            .is_some_and(|s| s == 0 || s > MAX_HALF_POINTS)
}

pub(super) fn paragraph_loses_format(p: &RichParagraph) -> bool {
    p.page_break_before
        || p.bookmark.is_some()
        || (p.list == Some(RichListKind::Task) && !(1..=6).contains(&p.heading_level))
        || (p.list == Some(RichListKind::Ordered)
            && p.list_number.is_some_and(|n| {
                n.format == RichNumberFormat::HangulSyllable
                    && n.enclosure != RichNumberEnclosure::Dot
            }))
        || p.left_indent_twips
            .is_some_and(|t| t > MAX_INDENT_TWIPS as u32)
        || p.right_indent_twips
            .is_some_and(|t| t > MAX_INDENT_TWIPS as u32)
        || p.first_line_indent_twips
            .is_some_and(|t| t.saturating_abs() > MAX_INDENT_TWIPS)
        || p.line_spacing.is_some_and(|s| !line_spacing_fits(s))
}

pub(super) fn drawable_archive_fields(w_pt: f32, h_pt: f32, description: Option<&str>) -> Vec<Fld> {
    let mut fields = vec![msg(
        drawable::GEOMETRY,
        vec![
            msg(geometry::POSITION, vec![float(1, 0.0), float(2, 0.0)]),
            msg(geometry::SIZE, vec![float(1, w_pt), float(2, h_pt)]),
        ],
    )];
    if let Some(desc) = description.map(str::trim).filter(|s| !s.is_empty()) {
        fields.push(string(drawable::DESCRIPTION, desc));
    }
    fields
}

/// The look of a run; `default_half_points` is the size its paragraph draws it at when the run names none.
pub(super) fn look_of(run: &RichRun, default_half_points: u16) -> CharLook {
    let size = run.size_half_pt.or(Some(default_half_points));
    CharLook {
        tracking_permille: run.letter_spacing_twips.map_or(0, |t| {
            i32::try_from(crate::rich_char_spacing::to_permille(t, size)).unwrap_or(0)
        }),
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        strike: run.strike,
        size_half_pt: run.size_half_pt.filter(|s| *s > 0 && *s <= MAX_HALF_POINTS),
        // The variant's PostScript name when the run has one (PD-368), else the font name.
        font: run
            .font_face
            .as_deref()
            .and_then(crate::font_face)
            .or_else(|| {
                run.font
                    .as_deref()
                    .map(str::trim)
                    .filter(|f| !f.is_empty() && f.len() <= MAX_FONT_NAME_BYTES)
                    .map(str::to_string)
            }),
        color: run.color.as_deref().and_then(parse_rgb),
        highlight: run.highlight.as_deref().and_then(parse_rgb),
        script: CharLook::script_code(run.script),
    }
}

pub(super) fn alignment_value(alignment: RichAlignment) -> u64 {
    match alignment {
        RichAlignment::Left => 0,
        RichAlignment::Right => 1,
        RichAlignment::Center => 2,
        RichAlignment::Justify => 3,
    }
}

pub(super) fn para_look_of(p: &RichParagraph) -> ParaLook {
    ParaLook {
        alignment: p.alignment.map(alignment_value),
        first_line_indent_twips: p
            .first_line_indent_twips
            .filter(|t| *t != 0 && t.saturating_abs() <= MAX_INDENT_TWIPS),
        left_indent_twips: p
            .left_indent_twips
            .filter(|t| *t > 0 && *t <= MAX_INDENT_TWIPS as u32),
        right_indent_twips: p
            .right_indent_twips
            .filter(|t| *t > 0 && *t <= MAX_INDENT_TWIPS as u32),
        line_spacing: p
            .line_spacing
            .filter(|s| *s != RichLineSpacing::Multiple(100) && line_spacing_fits(*s)),
        space_before_twips: p
            .space_before_twips
            .filter(|t| *t > 0 && *t <= MAX_PARA_SPACING_TWIPS),
        space_after_twips: p
            .space_after_twips
            .filter(|t| *t > 0 && *t <= MAX_PARA_SPACING_TWIPS),
    }
}

/// Whether the line spacing is one the writer stores as it is: a multiple of at least 1% and at
/// most [`MAX_SPACING_PERCENT`], or a length in `MAX_LINE_SPACING_TWIPS` (zero is no spacing at
/// all, except between lines where it is the single line).
fn line_spacing_fits(spacing: RichLineSpacing) -> bool {
    match spacing {
        RichLineSpacing::Multiple(pct) => (1..=MAX_SPACING_PERCENT).contains(&pct),
        RichLineSpacing::AtLeast(twips) | RichLineSpacing::Exact(twips) => {
            (1..=MAX_LINE_SPACING_TWIPS).contains(&twips)
        }
        RichLineSpacing::Between(twips) => twips <= MAX_LINE_SPACING_TWIPS,
    }
}

fn table(number: u32, entries: Vec<Fld>) -> Fld {
    msg(number, entries)
}

fn entry(index: u64, style: Option<u64>) -> Fld {
    let mut fields = vec![varint(1, index)];
    fields.extend(style.map(|id| reference(2, id)));
    msg(1, fields)
}

fn level_entry(index: u64, level: u64) -> Fld {
    msg(1, vec![varint(1, index), varint(2, level)])
}

fn zero_entry(number: u32) -> Fld {
    table(
        number,
        vec![msg(1, vec![varint(1, 0), varint(2, 0), varint(3, 0)])],
    )
}

fn push_run_change(entries: &mut Vec<(u64, u64)>, offset: u64, value: u64) {
    if entries.last().map(|(_, v)| *v) != Some(value) {
        if let Some((last_off, last_val)) = entries.last_mut() {
            if *last_off == offset {
                *last_val = value;
                return;
            }
        }
        entries.push((offset, value));
    }
}

fn write_image_attachment<'a>(
    bp: &mut Blueprint,
    styles: &Styles,
    img: &'a RichImage,
    image_data: &mut HashMap<&'a [u8], u64>,
    next_image_num: &mut usize,
) -> Result<u64, EasyOfficeError> {
    let raw = img.bytes();
    let data_id = if let Some(&existing) = image_data.get(raw) {
        existing
    } else {
        *next_image_num += 1;
        let ext = match img.format() {
            RichImageFormat::Png => "png",
            RichImageFormat::Jpeg => "jpg",
        };
        let name = format!("image-{}.{ext}", *next_image_num);
        let id = bp.add_data(name, raw.to_vec());
        image_data.insert(raw, id);
        id
    };

    let (w_emu, h_emu) = img.display_emu();
    let w_pt = (w_emu as f32 / EMU_PER_POINT).max(0.75);
    let h_pt = (h_emu as f32 / EMU_PER_POINT).max(0.75);
    let drawable_fields = drawable_archive_fields(w_pt, h_pt, Some(img.alt()));

    let doc_comp = bp.document();
    let img_obj = bp.add(
        doc_comp,
        ty::IMAGE,
        &[
            msg(image::DRAWABLE, drawable_fields),
            reference(3, styles.image),
            msg(
                9,
                vec![
                    float(1, (img.width_px() as f32 * 0.75).max(0.75)),
                    float(2, (img.height_px() as f32 * 0.75).max(0.75)),
                ],
            ),
            msg(image::DATA, vec![varint(1, data_id)]),
        ],
    )?;
    bp.add(doc_comp, ty::ATTACHMENT, &[reference(1, img_obj)])
}

/// Writes the body storage into the `Document` component under `body_id` and returns `body_id`.
pub(super) fn write_body<'a>(
    bp: &mut Blueprint,
    styles: &mut Styles,
    doc: &'a RichDocument,
    body_id: u64,
    body_width_pt: f32,
    section_id: Option<u64>,
) -> Result<u64, EasyOfficeError> {
    check_document_images(doc)?;
    let (notes, ordinals) = collect_note_ordinals(doc);
    let doc_comp = bp.document();

    let mut text = String::new();
    let mut offset = 0u64;
    let mut paragraph_entries: Vec<Fld> = Vec::new();
    let mut list_levels: Vec<(u64, u64)> = Vec::new();
    let mut list_styles: Vec<(u64, u64)> = Vec::new();
    let mut layout_styles: Vec<(u64, u64)> = Vec::new();
    let mut character_entries: Vec<(u64, Option<u64>)> = Vec::new();
    let mut attachment_entries: Vec<Fld> = Vec::new();
    let mut smartfield_entries: Vec<Fld> = Vec::new();

    let mut current_char: Option<u64> = None;
    let mut active_link: Option<&str> = None;
    let mut lines = 0usize;
    let mut runs = 0usize;
    let mut any_list_level = false;
    let mut table_budget = TableWriteBudget::default();
    let mut image_data: HashMap<&'a [u8], u64> = HashMap::new();
    let mut next_image_num = 0usize;

    let close_link = |active: &mut Option<&str>, entries: &mut Vec<Fld>, at: u64| {
        if active.take().is_some() {
            entries.push(entry(at, None));
        }
    };

    for block in doc.body_blocks() {
        match block {
            RichBlock::Paragraph(paragraph) => {
                lines += 1;
                if lines > MAX_PARAGRAPHS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 쓰기: 문단 수가 열기 한도를 넘었습니다".to_string(),
                    ));
                }
                if lines > 1 {
                    close_link(&mut active_link, &mut smartfield_entries, offset);
                    text.push('\n');
                    offset += 1;
                }

                let base = match usize::from(paragraph.heading_level).min(6) {
                    0 => styles.body,
                    level => styles.headings[level - 1],
                };
                let pstyle = styles.paragraph_variation(bp, base, para_look_of(paragraph))?;
                paragraph_entries.push(entry(offset, Some(pstyle)));

                let (lvl, lstyle) = match paragraph.list {
                    None => (0u64, styles.list),
                    Some(RichListKind::Bullet | RichListKind::Task) => {
                        let lvl = u64::from(paragraph.list_level.min(8));
                        any_list_level |= lvl > 0;
                        (lvl, styles.list_variation(bp, ListLook::Bullet)?)
                    }
                    Some(RichListKind::Ordered) => {
                        let lvl = u64::from(paragraph.list_level.min(8));
                        any_list_level |= lvl > 0;
                        let code =
                            ListLook::ordered_code(paragraph.list_number.unwrap_or_default());
                        (lvl, styles.list_variation(bp, ListLook::Ordered(code))?)
                    }
                };
                push_run_change(&mut list_levels, offset, lvl);
                push_run_change(&mut list_styles, offset, lstyle);

                let cstyle = match paragraph.columns.filter(|c| c.count >= 2) {
                    Some(cols) => styles.column_variation(bp, cols, body_width_pt)?,
                    None => styles.body_column,
                };
                push_run_change(&mut layout_styles, offset, cstyle);

                for run in &paragraph.runs {
                    if is_deleted_run(run) {
                        continue;
                    }
                    let piece = match run.note_ref {
                        Some(key) => format!("[{}]", ordinals.get(&key).copied().unwrap_or(1)),
                        None => clean(&run.text),
                    };
                    if piece.is_empty() {
                        continue;
                    }
                    runs += 1;
                    if runs > MAX_RUNS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "Pages 쓰기: 글자 구간 수가 열기 한도를 넘었습니다".to_string(),
                        ));
                    }
                    let mut look = look_of(run, default_half_points(paragraph.heading_level));
                    if run.note_ref.is_some() {
                        look.script = CharLook::script_code(crate::RichScript::Superscript);
                    }
                    let wanted = if look.is_plain() {
                        None
                    } else {
                        Some(styles.character(bp, look)?)
                    };
                    if wanted != current_char {
                        character_entries.push((offset, wanted));
                        current_char = wanted;
                    }

                    let wanted_link = run.link.as_deref().and_then(valid_pages_link);
                    if wanted_link != active_link {
                        if active_link.is_some() {
                            smartfield_entries.push(entry(offset, None));
                        }
                        if let Some(url) = wanted_link {
                            if smartfield_entries.len() + 2 > MAX_MARK_ENTRIES {
                                return Err(EasyOfficeError::ResourceBudgetExceeded(
                                    "Pages 쓰기: 하이퍼링크 수가 열기 한도를 넘었습니다"
                                        .to_string(),
                                ));
                            }
                            let hid =
                                bp.add(doc_comp, ty::HYPERLINK, &[string(hyperlink::URL, url)])?;
                            smartfield_entries.push(entry(offset, Some(hid)));
                        }
                        active_link = wanted_link;
                    }

                    text.push_str(&piece);
                    offset += piece.encode_utf16().count() as u64;
                }
                close_link(&mut active_link, &mut smartfield_entries, offset);
            }
            RichBlock::Table(tbl) => {
                let Some(att_id) =
                    write_table_attachment(bp, styles, tbl, body_width_pt, &mut table_budget)?
                else {
                    continue;
                };
                lines += 1;
                if lines > MAX_PARAGRAPHS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 쓰기: 문단 수가 열기 한도를 넘었습니다".to_string(),
                    ));
                }
                if lines > 1 {
                    close_link(&mut active_link, &mut smartfield_entries, offset);
                    text.push('\n');
                    offset += 1;
                }
                paragraph_entries.push(entry(offset, Some(styles.body)));
                push_run_change(&mut list_levels, offset, 0);
                push_run_change(&mut list_styles, offset, styles.list);
                push_run_change(&mut layout_styles, offset, styles.body_column);
                if current_char.is_some() {
                    character_entries.push((offset, None));
                    current_char = None;
                }
                attachment_entries.push(entry(offset, Some(att_id)));
                text.push('\u{FFFC}');
                offset += 1;

                for ci in tbl.validated_cell_images() {
                    lines += 1;
                    if lines > MAX_PARAGRAPHS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "Pages 쓰기: 문단 수가 열기 한도를 넘었습니다".to_string(),
                        ));
                    }
                    close_link(&mut active_link, &mut smartfield_entries, offset);
                    text.push('\n');
                    offset += 1;
                    let img_att_id = write_image_attachment(
                        bp,
                        styles,
                        &ci.image,
                        &mut image_data,
                        &mut next_image_num,
                    )?;
                    paragraph_entries.push(entry(offset, Some(styles.body)));
                    push_run_change(&mut list_levels, offset, 0);
                    push_run_change(&mut list_styles, offset, styles.list);
                    push_run_change(&mut layout_styles, offset, styles.body_column);
                    attachment_entries.push(entry(offset, Some(img_att_id)));
                    text.push('\u{FFFC}');
                    offset += 1;
                }
            }
            RichBlock::Image(img) => {
                lines += 1;
                if lines > MAX_PARAGRAPHS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 쓰기: 문단 수가 열기 한도를 넘었습니다".to_string(),
                    ));
                }
                if lines > 1 {
                    close_link(&mut active_link, &mut smartfield_entries, offset);
                    text.push('\n');
                    offset += 1;
                }
                let att_id =
                    write_image_attachment(bp, styles, img, &mut image_data, &mut next_image_num)?;
                paragraph_entries.push(entry(offset, Some(styles.body)));
                push_run_change(&mut list_levels, offset, 0);
                push_run_change(&mut list_styles, offset, styles.list);
                push_run_change(&mut layout_styles, offset, styles.body_column);
                if current_char.is_some() {
                    character_entries.push((offset, None));
                    current_char = None;
                }
                attachment_entries.push(entry(offset, Some(att_id)));
                text.push('\u{FFFC}');
                offset += 1;
            }
        }
    }
    for (note, ord) in notes {
        lines += 1;
        if lines > MAX_PARAGRAPHS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 쓰기: 문단 수가 열기 한도를 넘었습니다".to_string(),
            ));
        }
        if lines > 1 {
            close_link(&mut active_link, &mut smartfield_entries, offset);
            text.push('\n');
            offset += 1;
        }
        paragraph_entries.push(entry(offset, Some(styles.body)));
        push_run_change(&mut list_levels, offset, 0);
        push_run_change(&mut list_styles, offset, styles.list);
        push_run_change(&mut layout_styles, offset, styles.body_column);
        if current_char.is_some() {
            character_entries.push((offset, None));
            current_char = None;
        }
        let note_text = clean(&format!("[{ord}] {}", note.text.replace('\n', " ")));
        text.push_str(&note_text);
        offset += note_text.encode_utf16().count() as u64;
    }

    if lines == 0 {
        paragraph_entries.push(entry(0, Some(styles.body)));
        list_styles.push((0, styles.list));
        layout_styles.push((0, styles.body_column));
    }

    let mut fields = vec![varint(1, 0), reference(2, styles.stylesheet)];
    if !text.is_empty() {
        fields.push(string(storage::TEXT, &text));
    }
    fields.push(table(storage::PARA_STYLE_TABLE, paragraph_entries));
    if any_list_level {
        if list_levels.first().map(|(i, _)| *i) != Some(0) {
            list_levels.insert(0, (0, 0));
        }
        fields.push(table(
            storage::LIST_LEVEL_TABLE,
            list_levels
                .into_iter()
                .map(|(at, lvl)| level_entry(at, lvl))
                .collect(),
        ));
    } else {
        fields.push(zero_entry(storage::LIST_LEVEL_TABLE));
    }
    if list_styles.first().map(|(i, _)| *i) != Some(0) {
        list_styles.insert(0, (0, styles.list));
    }
    fields.push(table(
        storage::LIST_STYLE_TABLE,
        list_styles
            .into_iter()
            .map(|(at, id)| entry(at, Some(id)))
            .collect(),
    ));
    if !character_entries.is_empty() {
        if character_entries[0].0 != 0 {
            character_entries.insert(0, (0, None));
        }
        fields.push(table(
            storage::CHAR_STYLE_TABLE,
            character_entries
                .into_iter()
                .map(|(at, id)| entry(at, id))
                .collect(),
        ));
    }
    if !attachment_entries.is_empty() {
        fields.push(table(storage::ATTACHMENT_TABLE, attachment_entries));
    }
    fields.push(varint(10, 1));
    if !smartfield_entries.is_empty() {
        fields.push(table(storage::SMARTFIELD_TABLE, smartfield_entries));
    }
    if layout_styles.first().map(|(i, _)| *i) != Some(0) {
        layout_styles.insert(0, (0, styles.body_column));
    }
    fields.push(table(
        storage::LAYOUT_STYLE_TABLE,
        layout_styles
            .into_iter()
            .map(|(at, id)| entry(at, Some(id)))
            .collect(),
    ));
    fields.push(zero_entry(14));
    if let Some(sec_id) = section_id {
        fields.push(table(storage::SECTION_TABLE, vec![entry(0, Some(sec_id))]));
    }
    fields.push(zero_entry(24));

    bp.put(doc_comp, body_id, TYPE_STORAGE, &fields)?;
    Ok(body_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_and_controls_are_cleaned() {
        assert_eq!(
            clean("a\r\nb\nc\u{0}d\u{FFFC}e\tf"),
            "a\u{2028}b\u{2028}cde\tf"
        );
    }

    #[test]
    fn colors_parse_from_six_hex_digits_only() {
        let run = |c: &str| RichRun {
            color: Some(c.to_string()),
            ..RichRun::plain("x")
        };
        assert_eq!(look_of(&run("FF8000"), 22).color, Some([255, 128, 0]));
        assert_eq!(look_of(&run("#ff8000"), 22).color, Some([255, 128, 0]));
        assert_eq!(look_of(&run("FF80"), 22).color, None);
        assert_eq!(look_of(&run("GG0000"), 22).color, None);
        assert!(look_of(&RichRun::plain("x"), 22).is_plain());
    }
}
