//! JSON serialization and bounded validation for per-cell XLSX phonetic guides (`ph`, PD-319).

use std::fmt::Write as _;

use easyoffice_formats::{
    CellPhonetic, EasyOfficeError, PhoneticAlign, PhoneticKind, PhoneticRun, StrictJsonValue,
    MAX_PHONETIC_RUNS_PER_CELL, MAX_PHONETIC_TEXT_BYTES,
};

use crate::EasyOfficeIpcRouter;

/// Writes `{"r":"...","p":[[sb,eb,"..."],...]}` (plus non-default `"k"` / `"a"`) for a text cell.
pub(crate) fn write_cell_phonetic_json(out: &mut String, ph: &CellPhonetic) {
    out.push_str("{\"r\":\"");
    out.push_str(&EasyOfficeIpcRouter::escape_json_str(&ph.reading()));
    out.push_str("\",\"p\":[");
    for (idx, run) in ph.runs.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "[{},{},\"{}\"]",
            run.sb,
            run.eb,
            EasyOfficeIpcRouter::escape_json_str(&run.text)
        );
    }
    out.push(']');
    if ph.kind != PhoneticKind::NoConversion {
        let _ = write!(out, ",\"k\":\"{}\"", ph.kind.tag());
    }
    if ph.align != PhoneticAlign::NoControl {
        let _ = write!(out, ",\"a\":\"{}\"", ph.align.tag());
    }
    out.push('}');
}

/// Parses and validates `"ph"` JSON against `raw` under run-count and byte budgets.
pub(crate) fn parse_cell_phonetic_json(
    v: &StrictJsonValue,
    raw: &str,
) -> Result<Option<CellPhonetic>, EasyOfficeError> {
    if matches!(v, StrictJsonValue::Null) {
        return Ok(None);
    }
    let obj = v.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("구조화 저장: 셀 'ph'는 객체여야 합니다.".to_string())
    })?;
    let p_val = StrictJsonValue::get_field(obj, "p").ok_or_else(|| {
        EasyOfficeError::InvalidArgument("구조화 저장: 셀 'ph.p' 배열이 필요합니다.".to_string())
    })?;
    let run_items = p_val.as_array().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("구조화 저장: 셀 'ph.p'는 배열이어야 합니다.".to_string())
    })?;
    if run_items.len() > MAX_PHONETIC_RUNS_PER_CELL {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "XLSX 셀당 후리가나(rPh) 실행 구간 수가 허용 상한을 초과했습니다.".to_string(),
        ));
    }
    let mut total_bytes = 0usize;
    let mut runs = Vec::with_capacity(run_items.len());
    for item in run_items {
        let tuple = item.as_array().ok_or_else(|| {
            EasyOfficeError::InvalidArgument(
                "구조화 저장: 'ph.p' 항목은 [sb, eb, text] 배열이어야 합니다.".to_string(),
            )
        })?;
        if tuple.len() != 3 {
            return Err(EasyOfficeError::InvalidArgument(
                "구조화 저장: 'ph.p' 항목 길이는 3이어야 합니다.".to_string(),
            ));
        }
        let sb = tuple[0]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "구조화 저장: 'ph.p' 시작 위치(sb)가 올바르지 않습니다.".to_string(),
                )
            })?;
        let eb = tuple[1]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "구조화 저장: 'ph.p' 끝 위치(eb)가 올바르지 않습니다.".to_string(),
                )
            })?;
        let text = tuple[2].as_str().ok_or_else(|| {
            EasyOfficeError::InvalidArgument(
                "구조화 저장: 'ph.p' 텍스트가 문자열이어야 합니다.".to_string(),
            )
        })?;
        total_bytes = total_bytes.saturating_add(text.len());
        if total_bytes > MAX_PHONETIC_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XLSX 셀 후리가나(rPh) 누적 바이트가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
        runs.push(PhoneticRun {
            sb,
            eb,
            text: text.to_string(),
        });
    }
    let kind = match StrictJsonValue::get_field(obj, "k") {
        None | Some(StrictJsonValue::Null) => PhoneticKind::NoConversion,
        Some(StrictJsonValue::Str(s)) => PhoneticKind::from_tag(s).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("구조화 저장: 알 수 없는 후리가나 종류 '{s}'"))
        })?,
        _ => {
            return Err(EasyOfficeError::InvalidArgument(
                "구조화 저장: 'ph.k'는 문자열이어야 합니다.".to_string(),
            ))
        }
    };
    let align = match StrictJsonValue::get_field(obj, "a") {
        None | Some(StrictJsonValue::Null) => PhoneticAlign::NoControl,
        Some(StrictJsonValue::Str(s)) => PhoneticAlign::from_tag(s).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("구조화 저장: 알 수 없는 후리가나 정렬 '{s}'"))
        })?,
        _ => {
            return Err(EasyOfficeError::InvalidArgument(
                "구조화 저장: 'ph.a'는 문자열이어야 합니다.".to_string(),
            ))
        }
    };
    Ok(CellPhonetic::new(raw.to_string(), runs, kind, align))
}

/// Validates and clones optional inline phonetic text (`"ph"`) for formula evaluation payloads.
pub(crate) fn checked_eval_phonetic(raw: Option<&str>) -> Result<Option<String>, EasyOfficeError> {
    let Some(s) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if s.len() > MAX_PHONETIC_TEXT_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "XLSX 셀 후리가나(rPh) 누적 바이트가 허용 상한을 초과했습니다.".to_string(),
        ));
    }
    Ok(Some(s.to_string()))
}
