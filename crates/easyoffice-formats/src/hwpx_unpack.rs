//! Flat-payload section unpacking (`HwpxCodec::unpack`) with equation (`hp:equation` -> `$$LaTeX$$`),
//! shape (`hp:drawText`, `hp:caption`, `hp:textart`) and footnote/endnote (`hp:footNote`,
//! `hp:endNote`) separation under [`DecodeBudget`].

use crate::decode_budget::DecodeBudget;
use crate::hwpx_codec::{HwpxCodec, HwpxPayload};
use crate::office_codecs::{checked_part, checked_zip_xml_entries};
use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
    replace_tag_blocks, strip_tag_blocks,
};
use crate::{EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};

const MAX_EQUATION_BYTES: usize = 64 * 1024;
const INLINE_HP_TAGS: [(&str, &str); 5] = [
    ("tab", "\t"),
    ("lineBreak", "\n"),
    ("nbSpace", " "),
    ("fwSpace", " "),
    ("hyphen", "-"),
];

fn match_inline_hp_tag(after_hp: &str) -> Option<(&'static str, &'static str, &str)> {
    for &(tag, replacement) in &INLINE_HP_TAGS {
        if let Some(after) = after_hp.strip_prefix(tag) {
            if after.starts_with([' ', '\t', '\n', '\r', '/', '>']) {
                return Some((tag, replacement, after));
            }
        }
    }
    None
}

fn unescape_hp_t(raw: &str) -> String {
    if !raw.contains("<hp:") {
        return XmlSanitizer::unescape(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find("<hp:") {
        out.push_str(&XmlSanitizer::unescape(&rest[..pos]));
        let tail = &rest[pos + 4..];
        let Some((tag, replacement, after_name)) = match_inline_hp_tag(tail) else {
            out.push_str("<hp:");
            rest = tail;
            continue;
        };
        out.push_str(replacement);
        if let Some(end) = after_name.find('>') {
            let after_open = &after_name[end + 1..];
            if !after_name[..end].ends_with('/') {
                let close = format!("</hp:{tag}>");
                if let Some(after_close) = after_open.strip_prefix(&close) {
                    rest = after_close;
                    continue;
                }
            }
            rest = after_open;
        } else {
            rest = "";
        }
    }
    if !rest.is_empty() {
        out.push_str(&XmlSanitizer::unescape(rest));
    }
    out
}

/// A `Contents/section*.xml` part of a package whose XML entries were checked as a whole.
pub(crate) fn section_xml<'a>(
    parts: &[(&str, &'a str)],
    path: &str,
) -> Result<&'a str, EasyOfficeError> {
    checked_part(parts, path).ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(format!(
            "HWPX 구역 엔트리 '{path}'를 읽을 수 없습니다."
        ))
    })
}

pub(crate) fn unpack_hwpx_bytes(bytes: &[u8]) -> Result<HwpxPayload, EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    let parts = checked_zip_xml_entries(&entries)?;
    let (_header_xml, title, is_template, official_agency, approval_lines) =
        HwpxCodec::extract_header_and_metadata(&parts)?;

    let mut sec_entries: Vec<(usize, &crate::ZipArchiveEntry)> = entries
        .iter()
        .filter_map(|e| {
            let idx = e
                .path
                .strip_prefix("Contents/section")
                .and_then(|s| s.strip_suffix(".xml"))
                .and_then(|s| s.parse::<usize>().ok())?;
            Some((idx, e))
        })
        .collect();
    sec_entries.sort_by_key(|(idx, _)| *idx);

    if sec_entries.is_empty() {
        return Err(EasyOfficeError::CorruptedContainer(
            "HWPX 필수 엔트리 'Contents/section0.xml'이 누락되었습니다.".to_string(),
        ));
    }

    let mut tables = Vec::new();
    let mut paragraphs = Vec::new();
    let mut footnotes = Vec::new();
    let mut endnotes = Vec::new();
    let mut budget = DecodeBudget::new();
    budget.charge(title.len(), "HWPX 제목")?;

    for (_, sec_entry) in sec_entries {
        let sec_xml = section_xml(&parts, &sec_entry.path)?;

        for tbl_body in extract_tag_bodies(sec_xml, "hp:tbl") {
            let rows =
                extract_hwpx_table_rows(tbl_body, &mut footnotes, &mut endnotes, &mut budget)?;
            for r in &rows {
                for c in r {
                    budget.charge_cell(c.len(), "HWPX 표 셀")?;
                }
            }
            tables.push(rows);
            let tbl_without_tc = strip_tag_blocks(tbl_body, "hp:tc");
            collect_sub_paragraphs(&tbl_without_tc, "hp:caption", &mut paragraphs, &mut budget)?;
            let tbl_no_cap = strip_tag_blocks(&tbl_without_tc, "hp:caption");
            append_hwpx_equations(&tbl_no_cap, &mut paragraphs, &mut budget)?;
        }

        let sec_tables_replaced = replace_tag_blocks(sec_xml, "hp:tbl", "<hp:tbl_Removed/>");
        for p_body in extract_tag_bodies(&sec_tables_replaced, "hp:p") {
            budget.charge_block(0, "HWPX 문단")?;
            let has_table = p_body.contains("<hp:tbl_Removed/>");
            extract_paragraph_with_nested(
                p_body,
                has_table,
                &mut paragraphs,
                &mut footnotes,
                &mut endnotes,
                &mut budget,
            )?;
        }
        let outside_p = strip_tag_blocks(&sec_tables_replaced, "hp:p");
        append_hwpx_equations(&outside_p, &mut paragraphs, &mut budget)?;
    }

    paragraphs.extend(footnotes);
    paragraphs.extend(endnotes);

    Ok(HwpxPayload {
        title,
        is_template,
        official_agency,
        approval_lines,
        paragraphs,
        tables,
    })
}

fn collect_sub_paragraphs(
    xml: &str,
    tag: &str,
    out: &mut Vec<String>,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    for body in extract_tag_bodies(xml, tag) {
        for p in extract_tag_bodies(body, "hp:p") {
            let clean = strip_nested_containers(p);
            let mut sub = Vec::new();
            extract_hwpx_paragraphs_and_eqs(&clean, &mut sub, budget)?;
            for s in sub {
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
        }
    }
    Ok(())
}

fn collect_textarts(
    xml: &str,
    out: &mut Vec<String>,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    for (attrs, _) in extract_tags_or_self_closing_with_attrs(xml, "hp:textart") {
        if let Some(raw) = extract_attr_from_fragment(attrs, "text") {
            let unescaped = XmlSanitizer::unescape(raw);
            let trimmed = unescaped.trim();
            if !trimmed.is_empty() {
                budget.charge(trimmed.len(), "HWPX 글맵시")?;
                out.push(trimmed.to_string());
            }
        }
    }
    Ok(())
}

fn strip_nested_containers(xml: &str) -> String {
    let mut s = strip_tag_blocks(xml, "hp:footNote");
    s = strip_tag_blocks(&s, "hp:endNote");
    s = strip_tag_blocks(&s, "hp:drawText");
    s = strip_tag_blocks(&s, "hp:caption");
    s = strip_tag_blocks(&s, "hp:header");
    strip_tag_blocks(&s, "hp:footer")
}

fn extract_paragraph_with_nested(
    p_body: &str,
    has_table: bool,
    paragraphs: &mut Vec<String>,
    footnotes: &mut Vec<String>,
    endnotes: &mut Vec<String>,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    collect_sub_paragraphs(p_body, "hp:footNote", footnotes, budget)?;
    collect_sub_paragraphs(p_body, "hp:endNote", endnotes, budget)?;
    let mut inline_shapes = Vec::new();
    collect_sub_paragraphs(p_body, "hp:drawText", &mut inline_shapes, budget)?;
    collect_textarts(p_body, &mut inline_shapes, budget)?;
    collect_sub_paragraphs(p_body, "hp:caption", &mut inline_shapes, budget)?;

    let clean_p = strip_nested_containers(p_body);
    if !is_structural_carrier(&clean_p, has_table || !inline_shapes.is_empty()) {
        extract_hwpx_paragraphs_and_eqs(&clean_p, paragraphs, budget)?;
    }
    paragraphs.extend(inline_shapes);
    Ok(())
}

/// A paragraph that only anchors a table/shape or carries section/column properties has no text.
fn is_structural_carrier(p_body: &str, had_anchored_object: bool) -> bool {
    let carries_props = p_body.contains("<hp:secPr") || p_body.contains("<hp:ctrl");
    (had_anchored_object || carries_props)
        && extract_tags_or_self_closing_with_attrs(p_body, "hp:t").is_empty()
        && !p_body.contains("<hp:equation")
}

fn extract_hwpx_paragraphs_and_eqs(
    p_body: &str,
    out: &mut Vec<String>,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    let p_without_eq = strip_tag_blocks(p_body, "hp:equation");
    let text = extract_tag_bodies(&p_without_eq, "hp:t")
        .into_iter()
        .map(unescape_hp_t)
        .collect::<Vec<_>>()
        .join("");
    if !text.is_empty() || !p_body.contains("<hp:equation") {
        budget.charge(text.len(), "HWPX 문단 텍스트")?;
        out.push(text);
    }
    append_hwpx_equations(p_body, out, budget)
}

fn extract_hwpx_cell_text(
    tc_body: &str,
    footnotes: &mut Vec<String>,
    endnotes: &mut Vec<String>,
    budget: &mut DecodeBudget,
) -> Result<String, EasyOfficeError> {
    collect_sub_paragraphs(tc_body, "hp:footNote", footnotes, budget)?;
    collect_sub_paragraphs(tc_body, "hp:endNote", endnotes, budget)?;
    let tc_clean = strip_tag_blocks(&strip_tag_blocks(tc_body, "hp:footNote"), "hp:endNote");
    let cell_paras = extract_tag_bodies(&tc_clean, "hp:p");
    let mut parts = Vec::new();
    if !cell_paras.is_empty() {
        for p in cell_paras {
            let mut sub_shapes = Vec::new();
            collect_sub_paragraphs(p, "hp:drawText", &mut sub_shapes, budget)?;
            collect_textarts(p, &mut sub_shapes, budget)?;
            collect_sub_paragraphs(p, "hp:caption", &mut sub_shapes, budget)?;
            let p_clean = strip_nested_containers(p);
            if !is_structural_carrier(&p_clean, !sub_shapes.is_empty()) {
                extract_hwpx_paragraphs_and_eqs(&p_clean, &mut parts, budget)?;
            }
            parts.extend(sub_shapes);
        }
        let tc_no_p = strip_tag_blocks(&tc_clean, "hp:p");
        append_hwpx_equations(&tc_no_p, &mut parts, budget)?;
    } else {
        let tc_no_eq = strip_tag_blocks(&tc_clean, "hp:equation");
        let direct_t = extract_tag_bodies(&tc_no_eq, "hp:t");
        let text_nodes = if !direct_t.is_empty() {
            direct_t
        } else {
            extract_tag_bodies(&tc_no_eq, "hp:cellText")
        };
        let t = text_nodes
            .into_iter()
            .map(unescape_hp_t)
            .collect::<Vec<_>>()
            .join("");
        if !t.is_empty() || !tc_clean.contains("<hp:equation") {
            parts.push(t);
        }
        append_hwpx_equations(&tc_clean, &mut parts, budget)?;
    }
    Ok(parts.join("\n"))
}

fn extract_hwpx_table_rows(
    tbl_body: &str,
    footnotes: &mut Vec<String>,
    endnotes: &mut Vec<String>,
    budget: &mut DecodeBudget,
) -> Result<Vec<Vec<String>>, EasyOfficeError> {
    let mut rows = Vec::new();
    let tr_bodies = extract_tag_bodies(tbl_body, "hp:tr");
    let row_slices = if !tr_bodies.is_empty() {
        tr_bodies
    } else {
        vec![tbl_body]
    };
    for tr_body in row_slices {
        let mut cells = Vec::new();
        for tc_body in extract_tag_bodies(tr_body, "hp:tc") {
            cells.push(extract_hwpx_cell_text(
                tc_body, footnotes, endnotes, budget,
            )?);
        }
        rows.push(cells);
    }
    Ok(rows)
}

fn append_hwpx_equations(
    xml: &str,
    out: &mut Vec<String>,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    for eq_body in extract_tag_bodies(xml, "hp:equation") {
        for raw_script in extract_tag_bodies(eq_body, "hp:script") {
            if raw_script.len() > MAX_EQUATION_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWPX 수식 스크립트 크기 한도를 넘었습니다.".to_string(),
                ));
            }
            let script = XmlSanitizer::unescape(raw_script);
            let trimmed = script.trim();
            if !trimmed.is_empty() {
                let latex = crate::hwp_eqedit::convert_eqedit_to_latex(trimmed)?;
                let formatted = format!("$${latex}$$");
                budget.charge(formatted.len(), "HWPX 수식 텍스트")?;
                out.push(formatted);
            }
        }
    }
    Ok(())
}
