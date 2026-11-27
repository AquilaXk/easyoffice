//! Page setup (`TP.DocumentArchive` dimensions/margins, `TP.SettingsArchive` duplex facing pages)
//! and section header/footer 3-zone templates (`TP.SectionArchive` 10011, `TP.HeaderFooterList`
//! 10143, `TSWP.NumberAttachmentArchive` 2043).
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig; see THIRD_PARTY_NOTICES.md),
//! `src/pages.rs`, `src/create.rs`.

use super::body::{alignment_value, clean, look_of, para_look_of};
use super::package::Blueprint;
use super::styles::{default_half_points, Styles};
use super::wire::{msg, reference, string, varint, Fld};
use crate::pages_schema::{header_footer, number_attachment, section, settings, storage, ty};
use crate::{
    EasyOfficeError, RichAlignment, RichBindingKind, RichDocument, RichField, RichPageFrame,
    RichPageMargins, RichPaper, RichParagraph, MAX_PAGE_FRAME_PARAGRAPHS,
};

/// Default ISO A4 in PostScript points (210 x 297 mm), 20 mm margins, 12.5 mm header and 15 mm footer insets.
const DEFAULT_PAGE_WIDTH: f32 = 595.28;
const DEFAULT_PAGE_HEIGHT: f32 = 841.89;
const DEFAULT_MARGIN: f32 = 56.692_913;
const DEFAULT_HEADER_INSET: f32 = 35.433_07;
const DEFAULT_FOOTER_INSET: f32 = 42.519_684;

pub(super) struct ResolvedPageSetup {
    pub page_width_pt: f32,
    pub page_height_pt: f32,
    pub left_pt: f32,
    pub right_pt: f32,
    pub top_pt: f32,
    pub bottom_pt: f32,
    pub header_pt: f32,
    pub footer_pt: f32,
    pub body_width_pt: f32,
    pub settings_id: Option<u64>,
}

pub(super) fn resolve_page_setup(
    bp: &mut Blueprint,
    frame: &RichPageFrame,
) -> Result<ResolvedPageSetup, EasyOfficeError> {
    let settings_id = if frame.binding.kind == RichBindingKind::Duplex {
        let doc_comp = bp.document();
        Some(bp.add(
            doc_comp,
            ty::SETTINGS,
            &[varint(settings::BODY, 1), varint(settings::FACING_PAGES, 1)],
        )?)
    } else {
        None
    };

    if frame.paper.is_some() || frame.margins.is_some() {
        let paper = frame.paper.unwrap_or(RichPaper::A4);
        let margins = frame.margins.unwrap_or(RichPageMargins::PAGES_DEFAULT);
        if paper.holds_bound(&margins, frame.binding) {
            let to_pt = |tw: u32| tw as f32 / 20.0;
            let (page_width_pt, page_height_pt) = if paper == RichPaper::A4 {
                (DEFAULT_PAGE_WIDTH, DEFAULT_PAGE_HEIGHT)
            } else {
                (to_pt(paper.width), to_pt(paper.height))
            };
            let (left_pt, right_pt, top_pt, bottom_pt, header_pt, footer_pt) =
                if margins == RichPageMargins::PAGES_DEFAULT {
                    (
                        DEFAULT_MARGIN,
                        DEFAULT_MARGIN,
                        DEFAULT_MARGIN,
                        DEFAULT_MARGIN,
                        DEFAULT_HEADER_INSET,
                        DEFAULT_FOOTER_INSET,
                    )
                } else {
                    (
                        to_pt(margins.left),
                        to_pt(margins.right),
                        to_pt(margins.top),
                        to_pt(margins.bottom),
                        to_pt(margins.header),
                        to_pt(margins.footer),
                    )
                };
            return Ok(ResolvedPageSetup {
                page_width_pt,
                page_height_pt,
                left_pt,
                right_pt,
                top_pt,
                bottom_pt,
                header_pt,
                footer_pt,
                body_width_pt: (page_width_pt - left_pt - right_pt).max(36.0),
                settings_id,
            });
        }
    }

    Ok(ResolvedPageSetup {
        page_width_pt: DEFAULT_PAGE_WIDTH,
        page_height_pt: DEFAULT_PAGE_HEIGHT,
        left_pt: DEFAULT_MARGIN,
        right_pt: DEFAULT_MARGIN,
        top_pt: DEFAULT_MARGIN,
        bottom_pt: DEFAULT_MARGIN,
        header_pt: DEFAULT_HEADER_INSET,
        footer_pt: DEFAULT_FOOTER_INSET,
        body_width_pt: DEFAULT_PAGE_WIDTH - 2.0 * DEFAULT_MARGIN,
        settings_id,
    })
}

fn table_entry(index: u64, object_id: Option<u64>) -> Fld {
    let mut fields = vec![varint(storage::ENTRY_INDEX, index)];
    if let Some(id) = object_id {
        fields.push(reference(storage::ENTRY_OBJECT, id));
    }
    msg(storage::TABLE_ENTRIES, fields)
}

fn zero_entry(number: u32) -> Fld {
    msg(
        number,
        vec![msg(1, vec![varint(1, 0), varint(2, 0), varint(3, 0)])],
    )
}

fn write_zone_storage(
    bp: &mut Blueprint,
    styles: &mut Styles,
    paras: &[&RichParagraph],
    fallback_title: &str,
) -> Result<Option<u64>, EasyOfficeError> {
    if paras.is_empty() {
        return Ok(None);
    }
    let doc_comp = bp.document();
    let mut text = String::new();
    let mut offset = 0u64;
    let mut para_entries: Vec<Fld> = Vec::new();
    let mut char_entries: Vec<(u64, Option<u64>)> = Vec::new();
    let mut att_entries: Vec<Fld> = Vec::new();
    let mut current_char: Option<u64> = None;
    let mut written = 0usize;

    for para in paras.iter().take(MAX_PAGE_FRAME_PARAGRAPHS) {
        let has_content = para
            .runs
            .iter()
            .any(|r| r.field.is_some() || !clean(&r.text).trim().is_empty());
        if !has_content {
            continue;
        }
        if written > 0 {
            text.push('\n');
            offset += 1;
        }
        written += 1;

        let mut plook = para_look_of(para);
        if plook.alignment.is_none() {
            plook.alignment = para.alignment.map(alignment_value);
        }
        let pstyle = styles.paragraph_variation(bp, styles.body, plook)?;
        para_entries.push(table_entry(offset, Some(pstyle)));

        for run in &para.runs {
            let look = look_of(run, default_half_points(0));
            let wanted = if look.is_plain() {
                None
            } else {
                Some(styles.character(bp, look)?)
            };
            if let Some(field_kind) = run.field {
                match field_kind {
                    RichField::PageNumber | RichField::TotalPages => {
                        if wanted != current_char {
                            char_entries.push((offset, wanted));
                            current_char = wanted;
                        }
                        let kind = match field_kind {
                            RichField::PageNumber => number_attachment::KIND_PAGE_NUMBER,
                            RichField::TotalPages => number_attachment::KIND_PAGE_COUNT,
                            RichField::FileName => unreachable!(),
                        };
                        let num_att = bp.add(
                            doc_comp,
                            ty::NUMBER_ATTACHMENT,
                            &[varint(number_attachment::KIND, kind)],
                        )?;
                        att_entries.push(table_entry(offset, Some(num_att)));
                        text.push('\u{FFFC}');
                        offset += 1;
                    }
                    RichField::FileName => {
                        let fallback = if fallback_title.trim().is_empty() {
                            "[파일 이름]"
                        } else {
                            fallback_title.trim()
                        };
                        let piece = clean(fallback);
                        if !piece.is_empty() {
                            if wanted != current_char {
                                char_entries.push((offset, wanted));
                                current_char = wanted;
                            }
                            text.push_str(&piece);
                            offset += piece.encode_utf16().count() as u64;
                        }
                    }
                }
                continue;
            }
            let piece = clean(&run.text);
            if piece.is_empty() {
                continue;
            }
            if wanted != current_char {
                char_entries.push((offset, wanted));
                current_char = wanted;
            }
            text.push_str(&piece);
            offset += piece.encode_utf16().count() as u64;
        }
    }

    if written == 0 {
        return Ok(None);
    }

    let mut fields = vec![
        varint(1, 3),
        reference(2, styles.stylesheet),
        string(storage::TEXT, &text),
        msg(storage::PARA_STYLE_TABLE, para_entries),
        zero_entry(storage::LIST_LEVEL_TABLE),
        msg(
            storage::LIST_STYLE_TABLE,
            vec![table_entry(0, Some(styles.list))],
        ),
    ];
    if !char_entries.is_empty() {
        if char_entries[0].0 != 0 {
            char_entries.insert(0, (0, None));
        }
        fields.push(msg(
            storage::CHAR_STYLE_TABLE,
            char_entries
                .into_iter()
                .map(|(at, id)| table_entry(at, id))
                .collect(),
        ));
    }
    if !att_entries.is_empty() {
        fields.push(msg(storage::ATTACHMENT_TABLE, att_entries));
    }
    fields.push(varint(10, 1));
    fields.push(msg(
        storage::LAYOUT_STYLE_TABLE,
        vec![table_entry(0, Some(styles.body_column))],
    ));
    fields.push(zero_entry(14));
    fields.push(zero_entry(24));
    Ok(Some(bp.add(doc_comp, ty::STORAGE, &fields)?))
}

fn write_three_zone_strip(
    bp: &mut Blueprint,
    styles: &mut Styles,
    field_num: u32,
    paras: &[RichParagraph],
    fallback_title: &str,
    out: &mut Vec<Fld>,
) -> Result<bool, EasyOfficeError> {
    let mut zones: [Vec<&RichParagraph>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for p in paras.iter().take(MAX_PAGE_FRAME_PARAGRAPHS) {
        let slot = match p.alignment {
            Some(RichAlignment::Center) => 1,
            Some(RichAlignment::Right) => 2,
            _ => 0,
        };
        zones[slot].push(p);
    }
    let mut ids = [None; 3];
    let mut any = false;
    for (i, z) in zones.iter().enumerate() {
        ids[i] = write_zone_storage(bp, styles, z, fallback_title)?;
        any |= ids[i].is_some();
    }
    for slot_id in ids {
        match slot_id {
            Some(id) => out.push(reference(field_num, id)),
            None => out.push(msg(field_num, vec![varint(1, 0)])),
        }
    }
    Ok(any)
}

fn write_template_list(
    bp: &mut Blueprint,
    styles: &mut Styles,
    headers: &[RichParagraph],
    footers: &[RichParagraph],
    fallback_title: &str,
) -> Result<(u64, bool), EasyOfficeError> {
    let mut fields = Vec::with_capacity(6);
    let h_any = write_three_zone_strip(
        bp,
        styles,
        header_footer::HEADERS,
        headers,
        fallback_title,
        &mut fields,
    )?;
    let f_any = write_three_zone_strip(
        bp,
        styles,
        header_footer::FOOTERS,
        footers,
        fallback_title,
        &mut fields,
    )?;
    let doc_comp = bp.document();
    let tpl_id = bp.add(doc_comp, ty::HEADER_FOOTER_LIST, &fields)?;
    Ok((tpl_id, h_any || f_any))
}

/// Writes `TP.SectionArchive` (10011) and its `TP.HeaderFooterList` (10143) templates when
/// `doc.page_frame` carries headers, footers or first/even-odd flags.
pub(super) fn write_section(
    bp: &mut Blueprint,
    styles: &mut Styles,
    doc: &RichDocument,
) -> Result<Option<u64>, EasyOfficeError> {
    let frame = &doc.page_frame;
    if frame.is_empty() {
        return Ok(None);
    }
    let title = doc.title.as_str();
    let (odd_tpl, odd_any) = write_template_list(bp, styles, &frame.header, &frame.footer, title)?;
    let (first_tpl, first_any) = if frame.first_page_different {
        let (id, any) =
            write_template_list(bp, styles, &frame.first_header, &frame.first_footer, title)?;
        (Some(id), any)
    } else {
        (None, false)
    };
    let (even_tpl, even_any) = if frame.even_odd_different {
        let (id, any) =
            write_template_list(bp, styles, &frame.even_header, &frame.even_footer, title)?;
        (Some(id), any)
    } else {
        (None, false)
    };

    if !odd_any
        && !first_any
        && !even_any
        && !frame.first_page_different
        && !frame.even_odd_different
    {
        return Ok(None);
    }

    let mut sec_fields = Vec::new();
    if frame.first_page_different {
        sec_fields.push(varint(section::FIRST_PAGE_DIFFERENT, 1));
    }
    if frame.even_odd_different {
        sec_fields.push(varint(section::EVEN_ODD_PAGES_DIFFERENT, 1));
    }
    if let Some(fid) = first_tpl {
        sec_fields.push(reference(section::FIRST_TEMPLATE_PAGE, fid));
    }
    if let Some(eid) = even_tpl {
        sec_fields.push(reference(section::EVEN_TEMPLATE_PAGE, eid));
    }
    sec_fields.push(reference(section::ODD_TEMPLATE_PAGE, odd_tpl));
    if frame.first_page_different && !first_any {
        sec_fields.push(varint(section::FIRST_PAGE_HIDES_HEADER_FOOTER, 1));
    }

    let doc_comp = bp.document();
    Ok(Some(bp.add(doc_comp, ty::SECTION, &sec_fields)?))
}
