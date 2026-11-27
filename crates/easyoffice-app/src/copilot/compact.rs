//! 모듈별 고밀도 문맥 압축(SHEET 동형 수식·TSV 격자, BASE 파이프 행, PDF 창, DOC/HWP 희소 슬롯, SLIDE).
// 참조 구현: DUrayev/SpreadsheetLLM-compression (MIT)의 구조 앵커·동형 수식/TSV 압축,
// tribgames/mixdog (Apache-2.0, d5ab088efcc2680f0bb4c43448255ca1653fb3c6)의 문맥 직렬화.

use easyoffice_formats::EasyOfficeError;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::context::{
    BaseContext, DocContext, PdfContext, SelectionContext, SheetContext, SlideItem,
};
use super::delta::Tier2Included;
use super::prompt::{
    escape_context_text, push_line_with_limit, push_omitted, req_context_byte_budget,
    write_common_context_header, write_doc_selection_header, write_windowed,
};
use super::types::{
    CopilotModule, CopilotRunRequest, MAX_PROMPT_BASE_RECORDS, MAX_PROMPT_CONTEXT_BYTES,
    MAX_PROMPT_DOC_PARAGRAPHS, MAX_PROMPT_SHEET_CELLS, MAX_PROMPT_SLIDES,
};
use super::window::{select_window, Line};

const DOC_LINE_AFFIX_BYTES: usize = "  [] \n".len();
const OMIT_NOTICE_RESERVE: usize = 96;

const fn decimal_digits(mut n: usize) -> usize {
    let mut digits = 1;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    digits
}

fn selection_too_large() -> EasyOfficeError {
    EasyOfficeError::InvalidArgument("선택한 범위가 너무 커서 처리할 수 없습니다.".to_string())
}

/// `DOC`·`HWP` 일반 요청에서 프롬프트에 다 실리지 못한 문단(명시적 생략·200문단 초과·희소 미적재 슬롯·예산 절단)이
/// 있는지 판별한다. 실제 내용이 빈 문단(`Some("")`)은 미적재 슬롯(`None`)과 구분되어 false를 유지한다.
pub fn doc_has_unread_blocks(req: &CopilotRunRequest) -> bool {
    let Some(doc) = req.context.doc.as_ref() else {
        return false;
    };
    if doc.omitted_paragraphs > 0
        || doc.paragraphs.len() > MAX_PROMPT_DOC_PARAGRAPHS
        || doc.paragraphs.iter().any(Option::is_none)
    {
        return true;
    }
    let budget = req_context_byte_budget(req);
    let mut header = String::with_capacity(96);
    write_common_context_header(&mut header, req);
    if req.lightweight || req.command.is_some() {
        write_doc_selection_header(&mut header, &req.selection);
    }
    let mut used = header.len();
    for (i, slot) in doc.paragraphs.iter().enumerate() {
        if let Some(p) = slot {
            if !p.trim().is_empty() {
                let next_len =
                    DOC_LINE_AFFIX_BYTES + decimal_digits(i) + escape_context_text(p).len();
                if used + next_len > budget {
                    return true;
                }
                used += next_len;
            }
        }
    }
    false
}

/// 현재 요청에 읽기 도구가 실제로 필요한지 판별한다.
/// DOC/HWP 일반 대화는 `read_doc_blocks`를 상시 노출해 200문단·예산 경계 토글에 의한 캐시 무효화를 막는다.
pub fn has_read_tools(req: &CopilotRunRequest) -> bool {
    if req.lightweight || req.command.is_some() {
        return false;
    }
    matches!(
        req.module,
        CopilotModule::Doc
            | CopilotModule::Hwp
            | CopilotModule::Sheet
            | CopilotModule::Pdf
            | CopilotModule::Base
    )
}

fn selected_span_matches_loaded(
    doc: &DocContext,
    selection: &SelectionContext,
    included_indices: &BTreeSet<usize>,
) -> bool {
    let sel = doc.selected_text.trim();
    if sel.is_empty() {
        return true;
    }
    let start = selection.paragraph_index.unwrap_or(0);
    let end = selection.paragraph_end.unwrap_or(start);
    if start <= end && end < doc.paragraphs.len() {
        let mut all_loaded = true;
        let mut parts = Vec::with_capacity(end - start + 1);
        for idx in start..=end {
            if !included_indices.contains(&idx) {
                all_loaded = false;
                break;
            }
            match doc.paragraphs.get(idx) {
                Some(Some(t)) => parts.push(t.as_str()),
                _ => {
                    all_loaded = false;
                    break;
                }
            }
        }
        if all_loaded {
            let joined = parts.join("\n");
            if joined == doc.selected_text || joined.trim() == sel {
                return true;
            }
        }
    }
    included_indices.iter().any(|&idx| {
        matches!(
            doc.paragraphs.get(idx),
            Some(Some(t)) if t == &doc.selected_text || t.trim() == sel
        )
    })
}

pub fn write_doc_context(
    out: &mut String,
    doc: &DocContext,
    selection: &SelectionContext,
    command: bool,
) -> Result<(), EasyOfficeError> {
    let mut tier2 = Tier2Included::default();
    write_doc_context_with_budget(
        out,
        doc,
        selection,
        command,
        false,
        &mut tier2,
        MAX_PROMPT_CONTEXT_BYTES,
    )
}

pub fn write_doc_context_with_budget(
    out: &mut String,
    doc: &DocContext,
    selection: &SelectionContext,
    command: bool,
    separate_volatile: bool,
    tier2: &mut Tier2Included,
    budget: usize,
) -> Result<(), EasyOfficeError> {
    let mut included = BTreeSet::new();
    if command {
        let start = selection.paragraph_index.unwrap_or(0);
        let span = start..=selection.paragraph_end.unwrap_or(start);
        let lines: Vec<Line> = doc
            .paragraphs
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                let p = slot.as_deref()?;
                if p.trim().is_empty() && !span.contains(&i) {
                    return None;
                }
                Some(Line {
                    index: i,
                    text: format!("  [{i}] {}", escape_context_text(p)),
                })
            })
            .collect();
        let natural_end = doc.paragraphs.len();
        let (tail, kept_ids) = write_windowed(out, &lines, &span, natural_end, "문단")
            .ok_or_else(selection_too_large)?;
        included.extend(kept_ids);
        let cut = doc.paragraphs.len().saturating_sub(tail);
        push_omitted(out, cut + doc.omitted_paragraphs, "문단");
    } else {
        let mut consumed = 0usize;
        let mut unseen_sparse = 0usize;
        for (i, slot) in doc
            .paragraphs
            .iter()
            .enumerate()
            .take(MAX_PROMPT_DOC_PARAGRAPHS)
        {
            match slot {
                None => {
                    unseen_sparse += 1;
                    consumed = i + 1;
                }
                Some(p) => {
                    if !p.trim().is_empty()
                        && !push_line_with_limit(
                            out,
                            &format!("  [{i}] {}", escape_context_text(p)),
                            budget,
                        )
                    {
                        break;
                    }
                    included.insert(i);
                    consumed = i + 1;
                }
            }
        }
        let cut = doc.paragraphs.len().saturating_sub(consumed);
        push_omitted(out, cut + doc.omitted_paragraphs + unseen_sparse, "문단");
    }
    let matches_loaded = selected_span_matches_loaded(doc, selection, &included);
    tier2.doc_selected_matches_loaded = matches_loaded;
    tier2.doc_paras.extend(included);
    if !separate_volatile && !doc.selected_text.trim().is_empty() && !matches_loaded {
        let _ = writeln!(
            out,
            "선택 텍스트: {}",
            escape_context_text(&doc.selected_text)
        );
    }
    Ok(())
}

fn col_to_index(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut acc = 0u32;
    for b in letters.bytes() {
        let u = b.to_ascii_uppercase();
        if !u.is_ascii_uppercase() {
            return None;
        }
        acc = acc * 26 + u32::from(u - b'A' + 1);
    }
    (1..=16_384).contains(&acc).then_some(acc)
}

fn col_name(mut col: u32) -> String {
    let mut buf = Vec::with_capacity(4);
    while col > 0 {
        let rem = ((col - 1) % 26) as u8;
        buf.push(b'A' + rem);
        col = (col - 1) / 26;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap_or_default()
}

pub(super) fn parse_cell_coord(coord: &str) -> Option<(u32, u32)> {
    let clean = coord.trim().trim_start_matches('$');
    let split = clean.find(|c: char| c.is_ascii_digit())?;
    let (col_part, row_part) = clean.split_at(split);
    let col = col_to_index(col_part)?;
    let row_clean = row_part.trim_start_matches('$');
    let row = row_clean.parse::<u32>().ok()?;
    (1..=1_048_576).contains(&row).then_some((row, col))
}

pub(super) fn parse_range_bounds(range: &str) -> Option<((u32, u32), (u32, u32))> {
    let trimmed = range.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (a, b) = trimmed.split_once(':').unwrap_or((trimmed, trimmed));
    let (r1, c1) = parse_cell_coord(a)?;
    let (r2, c2) = parse_cell_coord(b)?;
    Some(((r1.min(r2), c1.min(c2)), (r1.max(r2), c1.max(c2))))
}

/// 수식 문자열의 상대 행 번호를 기준 행 `base_row`에 대한 `{r}`/`{r±k}` 표기로 정규화한다.
fn normalize_row_formula(formula: &str, base_row: u32) -> String {
    let bytes = formula.as_bytes();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut i = 0usize;
    let mut in_quotes = false;
    let mut in_single_quotes = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' && !in_single_quotes {
            in_quotes = !in_quotes;
            out.push('"');
            i += 1;
            continue;
        }
        if b == b'\'' && !in_quotes {
            in_single_quotes = !in_single_quotes;
            out.push('\'');
            i += 1;
            continue;
        }
        if !in_quotes && !in_single_quotes && b.is_ascii_alphabetic() {
            let prev_ok = i == 0
                || (bytes[i - 1] == b'$'
                    && (i == 1
                        || !(bytes[i - 2].is_ascii_alphanumeric()
                            || bytes[i - 2] == b'_'
                            || bytes[i - 2] == b'.')))
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$'
                    || bytes[i - 1] == b'.');
            if prev_ok {
                let col_start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() && i - col_start < 3 {
                    i += 1;
                }
                if i < bytes.len() && bytes[i].is_ascii_digit() {
                    let row_start = i;
                    while i < bytes.len() && bytes[i].is_ascii_digit() && i - row_start < 7 {
                        i += 1;
                    }
                    let next_ok = i == bytes.len()
                        || !(bytes[i].is_ascii_alphanumeric()
                            || bytes[i] == b'('
                            || bytes[i] == b'_'
                            || bytes[i] == b'!'
                            || bytes[i] == b'.'
                            || bytes[i] == b'\'');
                    if next_ok {
                        if let Ok(r) = std::str::from_utf8(&bytes[row_start..i])
                            .unwrap_or("")
                            .parse::<i64>()
                        {
                            let delta = r - i64::from(base_row);
                            if (-32..=32).contains(&delta) {
                                out.push_str(&formula[col_start..row_start]);
                                match delta {
                                    0 => out.push_str("{r}"),
                                    d if d > 0 => {
                                        let _ = write!(out, "{{r+{d}}}");
                                    }
                                    d => {
                                        let _ = write!(out, "{{r{d}}}");
                                    }
                                }
                                continue;
                            }
                        }
                    }
                    out.push_str(&formula[col_start..i]);
                    continue;
                }
                out.push_str(&formula[col_start..i]);
                continue;
            }
        }
        let ch = formula[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

pub(super) fn format_cell_display(raw: &str, value: &str) -> String {
    let raw_esc = escape_context_text(raw);
    let val_esc = escape_context_text(value);
    if raw_esc == val_esc || value.is_empty() {
        raw_esc.into_owned()
    } else if raw.is_empty() {
        val_esc.into_owned()
    } else {
        format!("{raw_esc}={val_esc}")
    }
}

pub fn write_sheet_context(out: &mut String, sheet: &SheetContext, selection: &SelectionContext) {
    let mut coords = BTreeSet::new();
    write_sheet_context_with_budget(
        out,
        sheet,
        selection,
        false,
        &mut coords,
        MAX_PROMPT_CONTEXT_BYTES,
    );
}

pub fn write_sheet_context_with_budget(
    out: &mut String,
    sheet: &SheetContext,
    selection: &SelectionContext,
    separate_volatile: bool,
    emitted_coords: &mut BTreeSet<(u32, u32)>,
    budget: usize,
) {
    let sel_bounds = selection.range.as_deref().and_then(parse_range_bounds);
    let mut parsed_cells: Vec<(u32, u32, usize)> = Vec::with_capacity(sheet.cells.len());
    let mut min_r = u32::MAX;
    let mut max_r = 0u32;
    let mut min_c = u32::MAX;
    let mut max_c = 0u32;

    for (idx, cell) in sheet.cells.iter().enumerate() {
        if let Some((r, c)) = parse_cell_coord(&cell.coord) {
            min_r = min_r.min(r);
            max_r = max_r.max(r);
            min_c = min_c.min(c);
            max_c = max_c.max(c);
            parsed_cells.push((r, c, idx));
        }
    }

    let derived_range = if min_r <= max_r && min_c <= max_c {
        format!("{}{min_r}:{}{max_c}", col_name(min_c), col_name(max_c))
    } else {
        "-".to_string()
    };
    let used_range = sheet.used_range.as_deref().unwrap_or(&derived_range);
    let total = sheet
        .total_cells
        .unwrap_or(sheet.cells.len())
        .max(sheet.cells.len());

    let mut summary = format!("시트: {}", escape_context_text(&sheet.name));
    if !sheet.sheet_tabs.is_empty() {
        let tabs: Vec<Cow<'_, str>> = sheet
            .sheet_tabs
            .iter()
            .map(|t| escape_context_text(t))
            .collect();
        let _ = write!(summary, " | 탭: {}", tabs.join(", "));
    }
    let _ = write!(
        summary,
        " | 사용범위: {} ({}칸)",
        escape_context_text(used_range),
        total
    );
    push_line_with_limit(out, &summary, budget);

    // 일반 대화 Tier 2는 선택 범위와 무관하게 좌표 순으로 결정론적 고정하고, 단발 요청만 선택 범위를 우선 배치한다.
    let mut prioritized = parsed_cells;
    if separate_volatile {
        prioritized.sort_by_key(|&(r, c, idx)| (r, c, idx));
    } else {
        prioritized.sort_by_key(|&(r, c, idx)| {
            let in_sel = sel_bounds.is_some_and(|((r1, c1), (r2, c2))| {
                (r1..=r2).contains(&r) && (c1..=c2).contains(&c)
            });
            (!in_sel, r, c, idx)
        });
    }
    let capped: Vec<(u32, u32, usize)> = prioritized
        .into_iter()
        .take(MAX_PROMPT_SHEET_CELLS)
        .collect();

    // 동형 수식 탐지: 같은 열에서 연속 행 3개 이상이 같은 {r} 수식 패턴이면 압축한다.
    let mut grid: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut cols_used: BTreeSet<u32> = BTreeSet::new();
    let mut col_cells: BTreeMap<u32, Vec<(u32, usize)>> = BTreeMap::new();
    for &(r, c, idx) in &capped {
        grid.entry(r).or_default().insert(c, idx);
        cols_used.insert(c);
        col_cells.entry(c).or_default().push((r, idx));
    }

    let mut iso_collapsed: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut iso_notes: Vec<String> = Vec::new();
    for (c, mut list) in col_cells {
        list.sort_by_key(|&(r, _)| r);
        let mut run_start = 0usize;
        while run_start < list.len() {
            let (r0, idx0) = list[run_start];
            let raw0 = sheet.cells[idx0].raw.trim();
            if !raw0.starts_with('=') {
                run_start += 1;
                continue;
            }
            let tpl0 = normalize_row_formula(raw0, r0);
            let mut run_end = run_start + 1;
            while run_end < list.len() {
                let (rn, idxn) = list[run_end];
                let (prev_r, _) = list[run_end - 1];
                let rawn = sheet.cells[idxn].raw.trim();
                if rn != prev_r + 1
                    || !rawn.starts_with('=')
                    || normalize_row_formula(rawn, rn) != tpl0
                {
                    break;
                }
                run_end += 1;
            }
            if run_end - run_start >= 3 {
                let cname = col_name(c);
                let r_last = list[run_end - 1].0;
                iso_notes.push(format!(
                    "동형수식: {cname}{r0}:{cname}{r_last} => {}",
                    escape_context_text(&tpl0)
                ));
                for &(rr, _) in &list[run_start + 1..run_end] {
                    iso_collapsed.insert((rr, c));
                }
            }
            run_start = run_end;
        }
    }

    for note in &iso_notes {
        if !push_line_with_limit(out, note, budget) {
            break;
        }
    }

    let num_rows = grid.len();
    let num_cols = cols_used.len();
    let use_tsv = num_rows > 0
        && num_cols > 0
        && num_cols <= 12
        && capped.len() * 100 >= num_rows * num_cols * 35;

    let mut shown = 0usize;
    if use_tsv {
        let cols: Vec<u32> = cols_used.into_iter().collect();
        let header_cols: Vec<String> = cols.iter().map(|&c| col_name(c)).collect();
        let _ = push_line_with_limit(out, &format!("열: {}", header_cols.join("\t")), budget);
        for (r, row_map) in &grid {
            let mut parts: Vec<String> = Vec::with_capacity(cols.len());
            for &c in &cols {
                if let Some(&idx) = row_map.get(&c) {
                    let cell = &sheet.cells[idx];
                    let text = if iso_collapsed.contains(&(*r, c)) && !cell.value.is_empty() {
                        escape_context_text(&cell.value).into_owned()
                    } else {
                        format_cell_display(&cell.raw, &cell.value)
                    };
                    parts.push(text.replace(['\t', '\r', '\n'], " "));
                } else {
                    parts.push(String::new());
                }
            }
            while parts.last().is_some_and(String::is_empty) {
                parts.pop();
            }
            if !push_line_with_limit(out, &format!("  [{r}] {}", parts.join("\t")), budget) {
                break;
            }
            for &c in row_map.keys() {
                emitted_coords.insert((*r, c));
            }
            shown += row_map.len();
        }
    } else {
        for &(r, c, idx) in &capped {
            let cell = &sheet.cells[idx];
            let display = if iso_collapsed.contains(&(r, c)) && !cell.value.is_empty() {
                escape_context_text(&cell.value).into_owned()
            } else {
                format_cell_display(&cell.raw, &cell.value)
            };
            if !push_line_with_limit(
                out,
                &format!("  {}: {display}", escape_context_text(&cell.coord)),
                budget,
            ) {
                break;
            }
            emitted_coords.insert((r, c));
            shown += 1;
        }
    }
    push_omitted(out, total.saturating_sub(shown), "칸");
}

pub fn write_base_context(out: &mut String, base: &BaseContext) {
    write_base_context_with_budget(out, base, MAX_PROMPT_CONTEXT_BYTES);
}

pub fn write_base_context_with_budget(out: &mut String, base: &BaseContext, budget: usize) {
    let tables: Vec<Cow<'_, str>> = base.tables.iter().map(|t| escape_context_text(t)).collect();
    if !push_line_with_limit(out, &format!("테이블 목록: {}", tables.join(", ")), budget) {
        push_omitted(out, tables.len(), "테이블");
    }
    let _ = writeln!(
        out,
        "현재 테이블: {} (레코드 {}건)",
        escape_context_text(&base.table),
        base.total_records
    );
    let field_list: Vec<String> = base
        .fields
        .iter()
        .map(|f| {
            format!(
                "{}({})",
                escape_context_text(&f.name),
                escape_context_text(&f.kind)
            )
        })
        .collect();
    if !push_line_with_limit(out, &format!("필드: {}", field_list.join(", ")), budget) {
        push_omitted(out, field_list.len(), "필드");
    }
    let mut shown = 0usize;
    for (idx, row) in base
        .records
        .iter()
        .take(MAX_PROMPT_BASE_RECORDS)
        .enumerate()
    {
        let cells: Vec<String> = row
            .iter()
            .map(|val| {
                escape_context_text(val)
                    .replace('|', "/")
                    .replace(['\r', '\n'], " ")
            })
            .collect();
        let line = format!("  [{idx}] {}", cells.join(" | "));
        if !push_line_with_limit(out, &line, budget) {
            break;
        }
        shown += 1;
    }
    push_omitted(out, base.total_records.saturating_sub(shown), "건");
}

pub(super) fn parse_pdf_page_prefix(line: &str) -> Option<usize> {
    let rest = line.trim_start().strip_prefix('[')?;
    let idx = rest.find("쪽]")?;
    rest[..idx].trim().parse::<usize>().ok().filter(|&p| p >= 1)
}

pub fn write_pdf_context(out: &mut String, pdf: &PdfContext) {
    let mut pages = BTreeSet::new();
    write_pdf_context_with_budget(out, pdf, false, &mut pages, MAX_PROMPT_CONTEXT_BYTES);
}

pub fn write_pdf_context_with_budget(
    out: &mut String,
    pdf: &PdfContext,
    separate_volatile: bool,
    emitted_pages: &mut BTreeSet<usize>,
    context_limit: usize,
) {
    if separate_volatile {
        let _ = writeln!(out, "전체 쪽 수: {}", pdf.page_count);
    } else {
        let _ = writeln!(
            out,
            "전체 쪽 수: {}, 현재 쪽: {}",
            pdf.page_count,
            pdf.active_page.unwrap_or(1)
        );
    }
    let raw_lines: Vec<&str> = pdf.text.lines().collect();
    if raw_lines.is_empty() {
        return;
    }
    let anchor = if separate_volatile {
        pdf.anchor_page.or(pdf.active_page).unwrap_or(1).max(1)
    } else {
        pdf.active_page.unwrap_or(1).max(1)
    };
    let mut current_page = 1usize;
    let mut saw_real_page = false;
    let mut lines: Vec<Line> = Vec::with_capacity(raw_lines.len());
    let mut line_pages: Vec<Option<usize>> = Vec::with_capacity(raw_lines.len());
    let mut target_start: Option<usize> = None;
    let mut target_end: Option<usize> = None;

    for (idx, raw) in raw_lines.iter().enumerate() {
        let is_omitted_banner = raw.starts_with("[앞 ") && raw.ends_with("쪽 생략]");
        if let Some(p) = parse_pdf_page_prefix(raw) {
            current_page = p;
            saw_real_page = true;
        }
        let assigned_page = if is_omitted_banner {
            None
        } else if saw_real_page || !raw.trim().is_empty() {
            Some(current_page)
        } else {
            None
        };
        if !is_omitted_banner && current_page == anchor {
            if target_start.is_none() {
                target_start = Some(idx);
            }
            target_end = Some(idx);
        }
        lines.push(Line {
            index: idx,
            text: escape_context_text(raw).into_owned(),
        });
        line_pages.push(assigned_page);
    }

    let start = target_start.unwrap_or(0);
    let end = target_end.unwrap_or(start);
    let budget = context_limit.saturating_sub(out.len() + OMIT_NOTICE_RESERVE);
    if let Some((before, selected, after)) = select_window(&lines, &(start..=end), budget) {
        let cut_front = lines
            .iter()
            .filter(|l| l.index < start)
            .count()
            .saturating_sub(before.len());
        if cut_front > 0 {
            let _ = writeln!(out, "[앞 {cut_front}줄 생략]");
        }
        for line in before.iter().chain(selected).chain(after) {
            out.push_str(&line.text);
            out.push('\n');
            if let Some(p) = line_pages.get(line.index).copied().flatten() {
                emitted_pages.insert(p);
            }
        }
        let full_after = lines.iter().filter(|l| l.index > end).count();
        let cut_after = full_after.saturating_sub(after.len());
        push_omitted(out, cut_after, "줄");
    } else {
        // 선택 쪽 자체가 예산을 넘으면 앞선 쪽을 건너뛰고 현재 쪽의 앞부분을 UTF-8 경계까지 실어 보낸다.
        let limit = context_limit.saturating_sub(OMIT_NOTICE_RESERVE);
        if start > 0 {
            if start == 2
                && lines[0].text.ends_with("쪽 생략]")
                && lines[1].text.is_empty()
                && out.len() + lines[0].text.len() + 2 <= limit
            {
                out.push_str(&lines[0].text);
                out.push_str("\n\n");
            } else if out.len() + 32 <= limit {
                let _ = writeln!(out, "[앞 {start}줄 생략]");
            }
        }
        let mut shown = 0usize;
        let mut clipped_first = false;
        for line in &lines[start..] {
            if out.len() + line.text.len() < limit {
                out.push_str(&line.text);
                out.push('\n');
                if let Some(p) = line_pages.get(line.index).copied().flatten() {
                    emitted_pages.insert(p);
                }
                shown += 1;
            } else {
                if shown == 0 {
                    let room = limit.saturating_sub(out.len() + 1);
                    let cut = super::cache::clamp_utf8_boundary(&line.text, room);
                    if cut > 0 {
                        out.push_str(&line.text[..cut]);
                        out.push('\n');
                        if let Some(p) = line_pages.get(line.index).copied().flatten() {
                            emitted_pages.insert(p);
                        }
                        clipped_first = true;
                    }
                }
                break;
            }
        }
        let omitted = lines.len().saturating_sub(start + shown);
        push_omitted(
            out,
            if clipped_first {
                omitted.max(1)
            } else {
                omitted
            },
            "줄",
        );
    }
}

pub(super) fn slide_line(i: usize, s: &SlideItem) -> String {
    let mut line = format!("  [{i}] 제목={}", escape_context_text(&s.title));
    if !s.bullets.is_empty() {
        let bullets: Vec<Cow<'_, str>> = s.bullets.iter().map(|b| escape_context_text(b)).collect();
        let _ = write!(line, " | 항목={bullets:?}");
    }
    if !s.notes.trim().is_empty() {
        let _ = write!(line, " | 노트={}", escape_context_text(&s.notes));
    }
    if !s.layout.trim().is_empty() && s.layout.trim() != "titleAndContent" {
        let _ = write!(line, " | 레이아웃={}", escape_context_text(&s.layout));
    }
    if !s.elements.is_empty() {
        let els: Vec<String> = s
            .elements
            .iter()
            .map(|el| {
                format!(
                    "{}:{}({},{},{}x{}):{}",
                    escape_context_text(&el.id),
                    escape_context_text(&el.kind),
                    el.x,
                    el.y,
                    el.w,
                    el.h,
                    escape_context_text(&el.text)
                )
            })
            .collect();
        let _ = write!(line, " | 개체=[{}]", els.join(", "));
    }
    line
}

fn inactive_slide_line(i: usize, s: &SlideItem) -> String {
    let mut parts = Vec::with_capacity(2);
    if !s.bullets.is_empty() {
        parts.push(format!("항목 {}개", s.bullets.len()));
    }
    if !s.elements.is_empty() {
        parts.push(format!("개체 {}개", s.elements.len()));
    }
    if parts.is_empty() {
        format!("  [{i}] 제목={}", escape_context_text(&s.title))
    } else {
        format!(
            "  [{i}] 제목={} ({})",
            escape_context_text(&s.title),
            parts.join(", ")
        )
    }
}

pub fn write_slide_context(
    out: &mut String,
    slides: &[SlideItem],
    target: Option<usize>,
) -> Result<(), EasyOfficeError> {
    let mut expanded = BTreeSet::new();
    write_slide_context_with_budget(
        out,
        slides,
        0,
        None,
        false,
        target,
        &mut expanded,
        MAX_PROMPT_CONTEXT_BYTES,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn write_slide_context_with_budget(
    out: &mut String,
    slides: &[SlideItem],
    active_index: usize,
    anchor_index: Option<usize>,
    separate_volatile: bool,
    target: Option<usize>,
    expanded: &mut BTreeSet<usize>,
    budget: usize,
) -> Result<(), EasyOfficeError> {
    let Some(target) = target else {
        let pinned_active = if separate_volatile {
            anchor_index.unwrap_or(active_index)
        } else {
            active_index
        };
        let eff_active = if pinned_active < slides.len() {
            pinned_active
        } else {
            0
        };
        let multi = slides.len() > 1;
        let mut shown = 0usize;
        for (i, s) in slides.iter().enumerate().take(MAX_PROMPT_SLIDES) {
            let full = !multi || i == eff_active;
            let line = if full {
                slide_line(i, s)
            } else {
                inactive_slide_line(i, s)
            };
            if !push_line_with_limit(out, &line, budget) {
                break;
            }
            if full {
                expanded.insert(i);
            }
            shown += 1;
        }
        push_omitted(out, slides.len() - shown, "슬라이드");
        return Ok(());
    };
    let lines: Vec<Line> = slides
        .iter()
        .enumerate()
        .map(|(i, s)| Line {
            index: i,
            text: slide_line(i, s),
        })
        .collect();
    let natural_end = slides.len();
    let (tail, _) = write_windowed(out, &lines, &(target..=target), natural_end, "슬라이드")
        .ok_or_else(selection_too_large)?;
    push_omitted(out, slides.len().saturating_sub(tail), "슬라이드");
    Ok(())
}
