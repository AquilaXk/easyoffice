//! App IPC, stateless evaluation, and export-time metadata plumbing for `CELL` and `INFO` (PD-331).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use easyoffice_formats::{
    normalize_runs, resolve_code, DocumentFormat, EasyOfficeError, HAlign, StrictJsonValue,
    CUSTOM_BASE, DEFAULT_COL_PX, MAX_COLUMN_PX, MAX_DIGIT_PX, MIN_COLUMN_PX, STANDARD_DIGIT_PX,
};
use easyoffice_sheet::{
    CellMap, PackedCellCoordinate, SheetCellMeta, SheetLock, WorkbookEngine, WorkbookMeta,
};

use crate::sheet_layout_json::parse_runs;
use crate::sheet_protect_json::parse_sheet_lock;
use crate::sheet_session::{recalc_answer, registry, session_id, EXPIRED, REFUSED};

const MAX_FILE_NAME_BYTES: usize = 1_024;
const MAX_META_SHEETS: usize = 1_024;
const MAX_CUSTOM_NUM_FMTS: usize = 16_384;
const MAX_META_CELLS: usize = 500_000;
const MAX_META_COLS: usize = 16_384;
const MAX_NUM_FMT_LEN: usize = 512;
const LAST_ROW_0BASED: u32 = 1_048_575;
const LAST_COL_0BASED: u32 = 16_383;

const PLACEHOLDER_TITLES: [&str; 3] = ["제목 없음", "변환_문서", "Untitled"];
const SPREADSHEET_EXTS: [&str; 6] = [".xlsx", ".xlsm", ".xls", ".ods", ".csv", ".tsv"];

thread_local! {
    static SAVE_FILE_NAME: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Restores the previous save-time workbook title on drop.
pub(crate) struct SaveFileNameGuard(String);

impl Drop for SaveFileNameGuard {
    fn drop(&mut self) {
        SAVE_FILE_NAME.with(|slot| *slot.borrow_mut() = std::mem::take(&mut self.0));
    }
}

fn bad(msg: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("셀 메타데이터 JSON이 올바르지 않습니다: {msg}"))
}

fn clean_file_basename(raw: &str) -> Result<String, EasyOfficeError> {
    if raw.contains('\0') {
        return Err(bad("fileName"));
    }
    if raw.len() > MAX_FILE_NAME_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "파일 이름 길이가 한도를 초과했습니다.".to_string(),
        ));
    }
    let trimmed = raw.trim().trim_end_matches('×').trim();
    let base = trimmed
        .rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .unwrap_or("");
    if base.is_empty() || PLACEHOLDER_TITLES.contains(&base) {
        return Ok(String::new());
    }
    Ok(base.to_string())
}

fn clean_opt_meta_str(
    entries: &[(String, StrictJsonValue)],
    key: &str,
) -> Result<Option<String>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, key) else {
        return Ok(None);
    };
    if matches!(val, StrictJsonValue::Null) {
        return Ok(Some(String::new()));
    }
    let s = val.as_str().ok_or_else(|| bad(key))?;
    if s.contains('\0') {
        return Err(bad(key));
    }
    if s.len() > MAX_FILE_NAME_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "메타데이터 '{key}' 길이가 한도를 초과했습니다."
        )));
    }
    Ok(Some(s.trim().to_string()))
}

/// Binds the exported document's safe base file name during a save/export call so `CELL("filename")`
/// reflects it inside formula cache computation without leaking host directory paths.
pub(crate) fn enter_save_file_name(title: &str, format: DocumentFormat) -> SaveFileNameGuard {
    let next = match clean_file_basename(title) {
        Ok(base) if !base.is_empty() => {
            let lower = base.to_ascii_lowercase();
            let has_sheet_ext = SPREADSHEET_EXTS.iter().any(|ext| lower.ends_with(ext));
            if has_sheet_ext {
                base
            } else {
                format!("{base}.{}", format.extension())
            }
        }
        _ => String::new(),
    };
    let prev = SAVE_FILE_NAME.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), next));
    SaveFileNameGuard(prev)
}

/// Applies the active save-time file name (when non-empty) onto `wb`.
pub(crate) fn apply_current_save_meta(wb: &mut WorkbookEngine) {
    let file_name = SAVE_FILE_NAME.with(|slot| slot.borrow().clone());
    if !file_name.is_empty() {
        wb.set_meta(WorkbookMeta {
            file_name,
            ..wb.meta().clone()
        });
    }
}

fn parse_workbook_meta_patch(
    entries: &[(String, StrictJsonValue)],
) -> Result<Option<WorkbookMeta>, EasyOfficeError> {
    let file_name_raw = clean_opt_meta_str(entries, "fileName")?;
    let directory = clean_opt_meta_str(entries, "directory")?;
    let release = clean_opt_meta_str(entries, "release")?;
    let osversion = clean_opt_meta_str(entries, "osversion")?;
    if file_name_raw.is_none() && directory.is_none() && release.is_none() && osversion.is_none() {
        return Ok(None);
    }
    let file_name = match file_name_raw {
        Some(raw) => clean_file_basename(&raw)?,
        None => String::new(),
    };
    Ok(Some(WorkbookMeta {
        file_name,
        directory: directory.filter(|s| !s.is_empty()),
        release: release.filter(|s| !s.is_empty()),
        osversion: osversion.filter(|s| !s.is_empty()),
    }))
}

fn parse_h_align(raw: &str) -> Result<HAlign, EasyOfficeError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "g" | "general" | "" => Ok(HAlign::General),
        "l" | "left" => Ok(HAlign::Left),
        "c" | "center" => Ok(HAlign::Center),
        "r" | "right" => Ok(HAlign::Right),
        _ => Err(bad("ha")),
    }
}

fn parse_custom_num_fmts(
    obj: &[(String, StrictJsonValue)],
) -> Result<Vec<String>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(obj, "numFmts") else {
        return Ok(Vec::new());
    };
    let arr = val.as_array().ok_or_else(|| bad("numFmts"))?;
    if arr.len() > MAX_CUSTOM_NUM_FMTS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "사용자 정의 서식 수가 한도를 초과했습니다.".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let code = item.as_str().ok_or_else(|| bad("numFmts"))?;
        if code.contains('\0') || code.len() > MAX_NUM_FMT_LEN {
            return Err(bad("numFmts"));
        }
        out.push(code.to_string());
    }
    Ok(out)
}

fn parse_sheet_cells_meta(
    obj: &[(String, StrictJsonValue)],
    custom_len: usize,
) -> Result<(CellMap<u16>, CellMap<HAlign>), EasyOfficeError> {
    let mut num_fmts = CellMap::default();
    let mut h_aligns = CellMap::default();
    let Some(val) = StrictJsonValue::get_field(obj, "cells") else {
        return Ok((num_fmts, h_aligns));
    };
    let entries = val.as_object().ok_or_else(|| bad("cells"))?;
    if entries.len() > MAX_META_CELLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "셀 메타데이터 항목 수가 한도를 초과했습니다.".to_string(),
        ));
    }
    for (addr, spec) in entries {
        let coord = PackedCellCoordinate::parse_a1(addr.trim())?;
        let spec_obj = spec.as_object().ok_or_else(|| bad("cells[]"))?;
        if let Some(nf_val) = StrictJsonValue::get_field(spec_obj, "nf") {
            let idx = nf_val
                .as_u64()
                .filter(|&n| n >= 1 && n as usize <= custom_len)
                .ok_or_else(|| bad("nf"))?;
            let id = CUSTOM_BASE
                .checked_add((idx - 1) as u16)
                .ok_or_else(|| bad("nf"))?;
            num_fmts.insert(coord, id);
        } else if let Some(id_val) = StrictJsonValue::get_field(spec_obj, "nfId") {
            let id = id_val
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| bad("nfId"))?;
            if id != 0 {
                // Verify resolvable if >= CUSTOM_BASE so invalid ids fail closed cleanly.
                if id >= CUSTOM_BASE && resolve_code(id, &vec![String::new(); custom_len]).is_none()
                {
                    return Err(bad("nfId"));
                }
                num_fmts.insert(coord, id);
            }
        }
        if let Some(ha_val) = StrictJsonValue::get_field(spec_obj, "ha") {
            let align = parse_h_align(ha_val.as_str().ok_or_else(|| bad("ha"))?)?;
            if align != HAlign::General {
                h_aligns.insert(coord, align);
            }
        }
    }
    Ok((num_fmts, h_aligns))
}

fn parse_col_widths(
    obj: &[(String, StrictJsonValue)],
) -> Result<BTreeMap<u32, u32>, EasyOfficeError> {
    let mut out = BTreeMap::new();
    let Some(val) = StrictJsonValue::get_field(obj, "cols") else {
        return Ok(out);
    };
    let arr = val.as_array().ok_or_else(|| bad("cols"))?;
    if arr.len() > MAX_META_COLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "열 너비 메타데이터 수가 한도를 초과했습니다.".to_string(),
        ));
    }
    for pair in arr {
        let items = pair.as_array().ok_or_else(|| bad("cols[]"))?;
        let [col_val, px_val] = items else {
            return Err(bad("cols[]"));
        };
        let col_1 = col_val
            .as_u64()
            .filter(|&c| (1..=u64::from(LAST_COL_0BASED + 1)).contains(&c))
            .ok_or_else(|| bad("cols[]"))? as u32;
        let px = px_val
            .as_u64()
            .filter(|&p| p <= u64::from(MAX_COLUMN_PX))
            .ok_or_else(|| bad("cols[]"))? as u32;
        out.insert(col_1 - 1, px);
    }
    Ok(out)
}

fn parse_digit_and_default_px(
    obj: &[(String, StrictJsonValue)],
) -> Result<(u32, u32), EasyOfficeError> {
    let default_col_px = match StrictJsonValue::get_field(obj, "defaultColPx") {
        None | Some(StrictJsonValue::Null) => DEFAULT_COL_PX,
        Some(v) => v
            .as_u64()
            .filter(|&p| (u64::from(MIN_COLUMN_PX)..=u64::from(MAX_COLUMN_PX)).contains(&p))
            .ok_or_else(|| bad("defaultColPx"))? as u32,
    };
    let mut digit_px = match StrictJsonValue::get_field(obj, "digitPx") {
        None | Some(StrictJsonValue::Null) => STANDARD_DIGIT_PX,
        Some(v) => v
            .as_u64()
            .filter(|&d| (1..=u64::from(MAX_DIGIT_PX)).contains(&d))
            .ok_or_else(|| bad("digitPx"))? as u32,
    };
    if let Some(df_val) = StrictJsonValue::get_field(obj, "defaultFont") {
        digit_px = crate::sheet_default_font_json::parse_default_font(df_val)?
            .map_or(STANDARD_DIGIT_PX, |f| f.digit_px());
    }
    Ok((default_col_px, digit_px))
}

fn parse_origin(obj: &[(String, StrictJsonValue)]) -> Result<(u32, u32), EasyOfficeError> {
    let Some(val) =
        StrictJsonValue::get_field(obj, "origin").filter(|v| !matches!(v, StrictJsonValue::Null))
    else {
        return Ok((0, 0));
    };
    let items = val.as_array().ok_or_else(|| bad("origin"))?;
    let [r_val, c_val] = items else {
        return Err(bad("origin"));
    };
    let row = r_val
        .as_u64()
        .filter(|&r| (1..=u64::from(LAST_ROW_0BASED + 1)).contains(&r))
        .ok_or_else(|| bad("origin"))? as u32;
    let col = c_val
        .as_u64()
        .filter(|&c| (1..=u64::from(LAST_COL_0BASED + 1)).contains(&c))
        .ok_or_else(|| bad("origin"))? as u32;
    Ok((row - 1, col - 1))
}

fn parse_one_sheet_meta(v: &StrictJsonValue) -> Result<SheetCellMeta, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("sheet"))?;
    let custom_num_fmts = parse_custom_num_fmts(obj)?;
    let (num_fmts, h_aligns) = parse_sheet_cells_meta(obj, custom_num_fmts.len())?;
    let lock = match StrictJsonValue::get_field(obj, "lock") {
        None | Some(StrictJsonValue::Null) => SheetLock::default(),
        Some(lock_val) => parse_sheet_lock(lock_val)?,
    };
    let col_widths_px = parse_col_widths(obj)?;
    let (default_col_px, digit_px) = parse_digit_and_default_px(obj)?;
    let mut hidden_cols = match StrictJsonValue::get_field(obj, "hiddenCols") {
        None | Some(StrictJsonValue::Null) => Vec::new(),
        Some(hc) => parse_runs(hc, LAST_COL_0BASED, "hiddenCols")?,
    };
    normalize_runs(&mut hidden_cols);
    let origin = parse_origin(obj)?;
    Ok(SheetCellMeta {
        num_fmts,
        custom_num_fmts,
        h_aligns,
        lock,
        col_widths_px,
        default_col_px,
        digit_px,
        hidden_cols,
        origin,
    })
}

fn parse_cell_meta_map(
    entries: &[(String, StrictJsonValue)],
) -> Result<Option<HashMap<String, SheetCellMeta>>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "cellMeta") else {
        return Ok(None);
    };
    if matches!(val, StrictJsonValue::Null) {
        return Ok(Some(HashMap::new()));
    }
    let sheets = val.as_object().ok_or_else(|| bad("cellMeta"))?;
    if sheets.len() > MAX_META_SHEETS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "셀 메타데이터 시트 수가 한도를 초과했습니다.".to_string(),
        ));
    }
    let mut map = HashMap::with_capacity(sheets.len());
    for (sheet_name, spec) in sheets {
        map.insert(sheet_name.clone(), parse_one_sheet_meta(spec)?);
    }
    Ok(Some(map))
}

/// Applies optional `fileName`/`directory`/`release`/`osversion` and `cellMeta` from stateless RPC
/// entries onto `wb`.
pub(crate) fn apply_stateless_meta(
    wb: &mut WorkbookEngine,
    entries: &[(String, StrictJsonValue)],
) -> Result<(), EasyOfficeError> {
    if let Some(meta) = parse_workbook_meta_patch(entries)? {
        wb.set_meta(meta);
    }
    if let Some(mut sheet_map) = parse_cell_meta_map(entries)? {
        let names: Vec<String> = wb.sheet_names().into_iter().map(str::to_string).collect();
        for name in names {
            if let Some(meta) = sheet_map.remove(&name) {
                wb.set_sheet_cell_meta(&name, Some(meta))?;
            }
        }
    }
    Ok(())
}

/// Handles `sessionAction: "cellMeta"`: updates workbook file name and per-sheet `CELL`/`INFO`
/// metadata on an active sheet session and recalculates volatile/affected cells.
pub(crate) fn sheet_session_cell_meta(
    entries: &[(String, StrictJsonValue)],
    owner: &str,
) -> Result<String, EasyOfficeError> {
    let id = session_id(entries)?;
    let wb_meta = parse_workbook_meta_patch(entries)?;
    let sheet_map = parse_cell_meta_map(entries)?;
    let now = Instant::now();
    let Some(handle) = registry().checkout(id, owner, now) else {
        return Ok(EXPIRED.to_string());
    };
    let Ok(mut session) = handle.lock() else {
        registry().forget(id);
        return Ok(EXPIRED.to_string());
    };
    if let Some(meta) = wb_meta {
        session.set_workbook_meta(meta);
    }
    if let Some(mut by_sheet) = sheet_map {
        let names: Vec<String> = session
            .workbook()
            .sheet_names()
            .into_iter()
            .map(str::to_string)
            .collect();
        for name in names {
            let next = by_sheet.remove(&name);
            match session.set_sheet_cell_meta(&name, next) {
                Ok(_) => {}
                Err(easyoffice_sheet::RecalcError::Invalid(err)) => return Err(err),
                Err(easyoffice_sheet::RecalcError::OverBudget) => return Ok(REFUSED.to_string()),
            }
        }
    }
    let outcome = session.recalculate();
    let json = recalc_answer(id, &mut session, &[], &outcome);
    let after = session.footprint();
    drop(session);
    if !registry().fits(id, after) {
        registry().forget(id);
        return Ok(REFUSED.to_string());
    }
    registry().record_footprint(id, after);
    Ok(json)
}
