//! Expansion of PDF `/ObjStm` object streams (PDF ISO 32000-1 §7.5.7).
//!
//! Embedded indirect objects are extracted from deflated `/ObjStm` streams.
//! Offsets are strictly validated, duplicate offsets rejected (Fail-Closed),
//! and parsing limits per object bound the expansion to prevent OOM bombs.

use std::collections::HashMap;

use super::pdf_value_parser::parse_pdf_value_with_table;
use super::{
    dict_get, dict_name_eq, resolve_int, PdfIndirectObject, PdfObjValue, MAX_PDF_INDIRECT_OBJECTS,
    MAX_PDF_OBJSTM_ENTRIES,
};
use crate::decode_budget::DecodeBudget;
use crate::pdf_stream_parser::ParenJumpTable;
use crate::EasyOfficeError;

/// Estimates memory bytes occupied by a parsed PDF object value for budget charging.
fn pdf_value_charge_bytes(val: &PdfObjValue) -> usize {
    match val {
        PdfObjValue::ParenStr(s) | PdfObjValue::HexStr(s) => {
            s.len().saturating_add(std::mem::size_of::<Vec<u8>>())
        }
        PdfObjValue::Name(s) => s.len().saturating_add(std::mem::size_of::<String>()),
        PdfObjValue::Array(arr) => {
            let mut sum = std::mem::size_of::<Vec<PdfObjValue>>()
                .saturating_add(arr.len().saturating_mul(std::mem::size_of::<PdfObjValue>()));
            for item in arr {
                sum = sum.saturating_add(pdf_value_charge_bytes(item));
            }
            sum
        }
        PdfObjValue::Dict(dict) => {
            let mut sum = std::mem::size_of::<Vec<(String, PdfObjValue)>>().saturating_add(
                dict.len()
                    .saturating_mul(std::mem::size_of::<(String, PdfObjValue)>()),
            );
            for (k, v) in dict {
                sum = sum.saturating_add(k.len().saturating_add(std::mem::size_of::<String>()));
                sum = sum.saturating_add(pdf_value_charge_bytes(v));
            }
            sum
        }
        _ => std::mem::size_of::<PdfObjValue>(),
    }
}

pub(crate) fn expand_pdf_object_streams(
    bytes: &[u8],
    objects: &mut HashMap<(u32, u16), PdfIndirectObject>,
    flate_cache: &HashMap<(u32, u16), Vec<u8>>,
    next_order_seq: &mut usize,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    type ObjStmHeader = ((u32, u16), usize, usize, Option<(usize, usize)>, usize);
    let mut obj_stms: Vec<ObjStmHeader> = objects
        .iter()
        .filter_map(|(&id, obj)| {
            let PdfObjValue::Dict(ref d) = obj.val else {
                return None;
            };
            if !dict_name_eq(d, "/Type", "/ObjStm") {
                return None;
            }
            let n = usize::try_from(resolve_int(dict_get(d, "/N")?, objects)?).ok()?;
            let first = usize::try_from(resolve_int(dict_get(d, "/First")?, objects)?).ok()?;
            Some((id, n, first, obj.stream_range, obj.order_seq))
        })
        .collect();
    obj_stms.sort_by_key(|x| x.4);

    for (id, n, first, stream_range, _) in obj_stms {
        let stream_slice: &[u8] = if let Some(decoded) = flate_cache.get(&id) {
            decoded.as_slice()
        } else if let Some((s, e)) = stream_range {
            &bytes[s..e]
        } else {
            continue;
        };
        if n > MAX_PDF_OBJSTM_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "PDF ObjStm 엔트리 수({n})가 상한({MAX_PDF_OBJSTM_ENTRIES})을 초과했습니다."
            )));
        }
        if first > stream_slice.len() {
            continue;
        }
        let header_str = String::from_utf8_lossy(&stream_slice[..first]);
        let need = n.saturating_mul(2);
        let nums: Vec<usize> = header_str
            .split_ascii_whitespace()
            .filter_map(|tok| tok.parse::<usize>().ok())
            .take(need)
            .collect();
        if nums.len() < need {
            continue;
        }

        // Collect (emb_obj_num, rel_off) pairs and sort by rel_off ascending
        let mut entries: Vec<(u32, usize)> = Vec::with_capacity(n);
        for i in 0..n {
            let emb_obj_num = nums[i * 2] as u32;
            let rel_off = nums[i * 2 + 1];
            entries.push((emb_obj_num, rel_off));
        }
        entries.sort_by_key(|&(_, rel_off)| rel_off);

        // Fail-Closed on adjacent duplicate offsets to prevent overlapping multi-gigabyte alias bombs
        for window in entries.windows(2) {
            if window[0].1 == window[1].1 {
                return Err(EasyOfficeError::CorruptedContainer(
                    "PDF ObjStm에 중복된 엔트리 오프셋이 있습니다.".to_string(),
                ));
            }
        }

        // Build ParenJumpTable once for this stream slice
        let paren_table = ParenJumpTable::build(stream_slice);

        for (idx, &(emb_obj_num, rel_off)) in entries.iter().enumerate() {
            let obj_start = first.saturating_add(rel_off);
            if obj_start >= stream_slice.len() {
                continue;
            }
            // Strict limit: start of next entry in sorted order, or stream_slice.len() for last entry
            let obj_limit = if idx + 1 < entries.len() {
                first
                    .saturating_add(entries[idx + 1].1)
                    .min(stream_slice.len())
            } else {
                stream_slice.len()
            };
            if obj_start >= obj_limit {
                continue;
            }

            if let Ok(Some((val, _))) = parse_pdf_value_with_table(
                stream_slice,
                Some(&paren_table),
                obj_start,
                obj_limit,
                0,
            ) {
                let cost = pdf_value_charge_bytes(&val);
                budget.charge(cost, "PDF ObjStm 객체")?;

                let key = (emb_obj_num, 0);
                if !objects.contains_key(&key) {
                    if objects.len() >= MAX_PDF_INDIRECT_OBJECTS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "PDF 간접 객체 수가 상한({MAX_PDF_INDIRECT_OBJECTS})을 초과했습니다."
                        )));
                    }
                    *next_order_seq += 1;
                    objects.insert(
                        key,
                        PdfIndirectObject {
                            val,
                            stream_range: None,
                            order_seq: *next_order_seq,
                        },
                    );
                }
            }
        }
    }
    Ok(())
}
