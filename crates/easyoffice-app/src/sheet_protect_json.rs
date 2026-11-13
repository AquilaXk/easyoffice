//! JSON form of the protection of a sheet and of the workbook, shared by the open answer, the save
//! request and the row/column edit of a sheet session (PD-216).
//!
//! `protect` of a sheet: `{"on": bool, "deny": [flag, ...], "pwd": credential, "ranges": [{"name",
//! "sqref": ["B2:C3", ...], "pwd": credential, "descriptor"}, ...]}`. `deny` lists the locked actions by the
//! attribute names of `sheetProtection` (`formatCells`, `insertRows`, `sort`, ...); `pwd` is absent for a
//! sheet without a password. `unlockedRows`, `unlockedCols`: `[[first, last], ...]`, 1-based and inclusive,
//! the rows and columns whose own format is unlocked. `workbookProtect`: `{"structure": bool, "windows":
//! bool, "pwd": credential, "revisions": [[name, value], ...]}`.
//!
//! A credential is the digest, never the password: `{"kind":"agile","alg","hash","salt","spin"}`,
//! `{"kind":"legacy","hash":"83AF"}`, `{"kind":"odf","key","alg"}` or `{"kind":"both", ...the agile fields,
//! "key", "keyAlg"}` for a password just set in the editor (an XLSX file keeps the first digest and an ODS
//! file the second). Text over a bound, a `spin` that is
//! not a 32-bit number, an unknown flag or kind is refused, not trimmed. A digest of an unknown algorithm
//! or with a `spin` over the check bound is accepted: it is kept and written back, and no password opens it.

use std::fmt::Write;

use easyoffice_formats::{
    credential_within_bounds, normalize_runs, AgileHash, CellBox, Credential, EasyOfficeError,
    OdfKey, ProtectedRange, SheetProtection, SheetStyling, StrictJsonValue, WorkbookProtection,
    FLAG_NAMES, MAX_PROTECTED_RANGES, MAX_PROTECT_TEXT_CHARS, MAX_RANGE_AREAS,
};

use crate::sheet_layout_json::{parse_runs, runs_json};
use crate::EasyOfficeIpcRouter;

const ROWS: u32 = 1_048_576;
const COLS: u32 = 16_384;

type Obj<'a> = &'a [(String, StrictJsonValue)];

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("보호 JSON이 올바르지 않습니다: {what}"))
}

fn field<'a>(obj: Obj<'a>, key: &str) -> Option<&'a StrictJsonValue> {
    StrictJsonValue::get_field(obj, key)
}

fn flag(obj: Obj<'_>, key: &str) -> Result<bool, EasyOfficeError> {
    field(obj, key).map_or(Ok(false), |v| v.as_bool().ok_or_else(|| bad(key)))
}

fn text(obj: Obj<'_>, key: &str) -> Result<Option<String>, EasyOfficeError> {
    match field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .filter(|s| s.chars().count() <= MAX_PROTECT_TEXT_CHARS)
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| bad(key)),
    }
}

fn parse_credential(v: Option<&StrictJsonValue>) -> Result<Credential, EasyOfficeError> {
    let Some(v) = v.filter(|v| !matches!(v, StrictJsonValue::Null)) else {
        return Ok(Credential::None);
    };
    let obj = v.as_object().ok_or_else(|| bad("pwd"))?;
    let agile = |obj: Obj<'_>| -> Result<AgileHash, EasyOfficeError> {
        Ok(AgileHash {
            algorithm: text(obj, "alg")?.ok_or_else(|| bad("alg"))?,
            hash: text(obj, "hash")?.ok_or_else(|| bad("hash"))?,
            salt: text(obj, "salt")?.ok_or_else(|| bad("salt"))?,
            spin: field(obj, "spin")
                .and_then(StrictJsonValue::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| bad("spin"))?,
        })
    };
    let credential = match text(obj, "kind")?.as_deref() {
        Some("agile") => Credential::Agile(agile(obj)?),
        Some("both") => Credential::Both(
            Box::new(agile(obj)?),
            OdfKey {
                key: text(obj, "key")?.ok_or_else(|| bad("key"))?,
                algorithm: text(obj, "keyAlg")?,
            },
        ),
        Some("legacy") => {
            let hash = text(obj, "hash")?.ok_or_else(|| bad("hash"))?;
            Credential::Legacy(u16::from_str_radix(&hash, 16).map_err(|_| bad("hash"))?)
        }
        Some("odf") => Credential::Odf(OdfKey {
            key: text(obj, "key")?.ok_or_else(|| bad("key"))?,
            algorithm: text(obj, "alg")?,
        }),
        _ => return Err(bad("kind")),
    };
    if credential_within_bounds(&credential) {
        Ok(credential)
    } else {
        Err(bad("pwd"))
    }
}

/// The `pwd` field of a request as a credential (absent or `null` is none).
pub(crate) fn credential_field(obj: Obj<'_>) -> Result<Credential, EasyOfficeError> {
    parse_credential(field(obj, "pwd"))
}

/// `{"kind":..,"alg":..,"hash":..,"salt":..,"spin":..` without the closing brace.
fn push_agile(out: &mut String, kind: &str, a: &AgileHash) {
    let _ = write!(out, "{{\"kind\":\"{kind}\",\"alg\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, &a.algorithm);
    out.push_str("\",\"hash\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, &a.hash);
    out.push_str("\",\"salt\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, &a.salt);
    let _ = write!(out, "\",\"spin\":{}", a.spin);
}

pub(crate) fn credential_json(credential: &Credential) -> Option<String> {
    let mut out = String::new();
    match credential {
        Credential::None => return None,
        Credential::Legacy(hash) => {
            let _ = write!(out, "{{\"kind\":\"legacy\",\"hash\":\"{hash:04X}\"}}");
        }
        Credential::Agile(a) => {
            push_agile(&mut out, "agile", a);
            out.push('}');
        }
        Credential::Both(a, o) => {
            push_agile(&mut out, "both", a);
            out.push_str(",\"key\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &o.key);
            out.push('"');
            if let Some(alg) = &o.algorithm {
                out.push_str(",\"keyAlg\":\"");
                EasyOfficeIpcRouter::escape_json_into(&mut out, alg);
                out.push('"');
            }
            out.push('}');
        }
        Credential::Odf(o) => {
            out.push_str("{\"kind\":\"odf\",\"key\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &o.key);
            out.push('"');
            if let Some(alg) = &o.algorithm {
                out.push_str(",\"alg\":\"");
                EasyOfficeIpcRouter::escape_json_into(&mut out, alg);
                out.push('"');
            }
            out.push('}');
        }
    }
    Some(out)
}

fn parse_range(v: &StrictJsonValue) -> Result<ProtectedRange, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("ranges"))?;
    let items = field(obj, "sqref")
        .and_then(StrictJsonValue::as_array)
        .filter(|a| a.len() <= MAX_RANGE_AREAS)
        .ok_or_else(|| bad("sqref"))?;
    let areas = items
        .iter()
        .map(|item| {
            item.as_str()
                .and_then(CellBox::parse)
                .ok_or_else(|| bad("sqref"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProtectedRange {
        name: text(obj, "name")?.unwrap_or_default(),
        areas,
        credential: parse_credential(field(obj, "pwd"))?,
        descriptor: text(obj, "descriptor")?,
    })
}

pub(crate) fn parse_protection(v: &StrictJsonValue) -> Result<SheetProtection, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("protect"))?;
    let mut deny = 0u16;
    for item in field(obj, "deny")
        .and_then(StrictJsonValue::as_array)
        .ok_or_else(|| bad("deny"))?
    {
        let name = item.as_str().ok_or_else(|| bad("deny"))?;
        let (_, bit) = FLAG_NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .ok_or_else(|| bad("deny"))?;
        deny |= bit;
    }
    let mut ranges = Vec::new();
    if let Some(list) = field(obj, "ranges") {
        let items = list
            .as_array()
            .filter(|a| a.len() <= MAX_PROTECTED_RANGES)
            .ok_or_else(|| bad("ranges"))?;
        for item in items {
            ranges.push(parse_range(item)?);
        }
    }
    let protection = SheetProtection {
        on: flag(obj, "on")?,
        deny,
        credential: parse_credential(field(obj, "pwd"))?,
        ranges,
    };
    if protection.within_bounds() {
        Ok(protection)
    } else {
        Err(bad("protect"))
    }
}

pub(crate) fn protect_json(p: &SheetProtection) -> String {
    let mut out = String::from("{");
    let _ = write!(out, "\"on\":{},\"deny\":[", p.on);
    let mut first = true;
    for (name, bit) in FLAG_NAMES {
        if p.deny & bit != 0 {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "\"{name}\"");
        }
    }
    out.push(']');
    if let Some(pwd) = credential_json(&p.credential) {
        let _ = write!(out, ",\"pwd\":{pwd}");
    }
    if !p.ranges.is_empty() {
        out.push_str(",\"ranges\":[");
        for (i, r) in p.ranges.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, &r.name);
            out.push_str("\",\"sqref\":[");
            let areas: Vec<String> = r
                .areas
                .iter()
                .map(|a| format!("\"{}\"", a.to_a1()))
                .collect();
            out.push_str(&areas.join(","));
            out.push(']');
            if let Some(pwd) = credential_json(&r.credential) {
                let _ = write!(out, ",\"pwd\":{pwd}");
            }
            if let Some(descriptor) = &r.descriptor {
                out.push_str(",\"descriptor\":\"");
                EasyOfficeIpcRouter::escape_json_into(&mut out, descriptor);
                out.push('"');
            }
            out.push('}');
        }
        out.push(']');
    }
    out.push('}');
    out
}

/// Reads `protect`, `unlockedRows` and `unlockedCols` of a sheet object into `styling`.
pub(crate) fn parse_sheet_protect(
    obj: Obj<'_>,
    styling: &mut SheetStyling,
) -> Result<(), EasyOfficeError> {
    if let Some(v) = field(obj, "protect").filter(|v| !matches!(v, StrictJsonValue::Null)) {
        styling.protect = Some(parse_protection(v)).transpose()?;
    }
    if let Some(v) = field(obj, "unlockedRows") {
        styling.unlocked_rows = parse_runs(v, ROWS - 1, "unlockedRows")?;
    }
    if let Some(v) = field(obj, "unlockedCols") {
        styling.unlocked_cols = parse_runs(v, COLS - 1, "unlockedCols")?;
    }
    Ok(())
}

/// `,"protect":..,"unlockedRows":..,"unlockedCols":..` for the parts a sheet has.
pub(crate) fn write_sheet_protect(out: &mut String, styling: &SheetStyling) {
    if let Some(p) = &styling.protect {
        let _ = write!(out, ",\"protect\":{}", protect_json(p));
    }
    if !styling.unlocked_rows.is_empty() {
        let _ = write!(
            out,
            ",\"unlockedRows\":{}",
            runs_json(&styling.unlocked_rows)
        );
    }
    if !styling.unlocked_cols.is_empty() {
        let _ = write!(
            out,
            ",\"unlockedCols\":{}",
            runs_json(&styling.unlocked_cols)
        );
    }
}

pub(crate) fn parse_workbook_protect(
    v: &StrictJsonValue,
) -> Result<Option<WorkbookProtection>, EasyOfficeError> {
    if matches!(v, StrictJsonValue::Null) {
        return Ok(None);
    }
    let obj = v.as_object().ok_or_else(|| bad("workbookProtect"))?;
    let mut revisions = Vec::new();
    if let Some(list) = field(obj, "revisions") {
        for item in list.as_array().ok_or_else(|| bad("revisions"))? {
            let pair = item
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or_else(|| bad("revisions"))?;
            let (Some(k), Some(v)) = (pair[0].as_str(), pair[1].as_str()) else {
                return Err(bad("revisions"));
            };
            revisions.push((k.to_string(), v.to_string()));
        }
    }
    let protection = WorkbookProtection {
        lock_structure: flag(obj, "structure")?,
        lock_windows: flag(obj, "windows")?,
        credential: parse_credential(field(obj, "pwd"))?,
        revisions,
    };
    if !protection.within_bounds() {
        return Err(bad("workbookProtect"));
    }
    Ok(Some(protection).filter(|p| !p.is_default()))
}

pub(crate) fn workbook_protect_json(p: &WorkbookProtection) -> String {
    let mut out = format!(
        "{{\"structure\":{},\"windows\":{}",
        p.lock_structure, p.lock_windows
    );
    if let Some(pwd) = credential_json(&p.credential) {
        let _ = write!(out, ",\"pwd\":{pwd}");
    }
    if !p.revisions.is_empty() {
        out.push_str(",\"revisions\":[");
        for (i, (k, v)) in p.revisions.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("[\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, k);
            out.push_str("\",\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, v);
            out.push_str("\"]");
        }
        out.push(']');
    }
    out.push('}');
    out
}

/// The protection of a sheet and its unlocked runs after an edit of rows or columns: allowed ranges and
/// runs move with the cells.
pub(crate) fn moved_protect_json(
    value: &StrictJsonValue,
    edit: &easyoffice_sheet::AxisEdit,
) -> Result<String, EasyOfficeError> {
    let mut protection = parse_protection(value)?;
    for range in &mut protection.ranges {
        range.areas = easyoffice_sheet::shift_boxes(&range.areas, edit, false);
    }
    protection.ranges.retain(|r| !r.areas.is_empty());
    Ok(protect_json(&protection))
}

/// The runs of `v` (1-based) moved for `edit` when it is on the runs' axis.
pub(crate) fn moved_runs_json(
    v: &StrictJsonValue,
    edit: &easyoffice_sheet::AxisEdit,
    axis: easyoffice_sheet::Axis,
    what: &str,
) -> Result<String, EasyOfficeError> {
    let last = if axis == easyoffice_sheet::Axis::Row {
        ROWS - 1
    } else {
        COLS - 1
    };
    let mut runs = parse_runs(v, last, what)?;
    if edit.axis == axis {
        runs = easyoffice_sheet::shift_runs(&runs, edit);
    }
    normalize_runs(&mut runs);
    Ok(runs_json(&runs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<SheetStyling, EasyOfficeError> {
        let json = StrictJsonValue::parse(text).unwrap();
        let mut styling = SheetStyling::default();
        parse_sheet_protect(json.as_object().unwrap(), &mut styling).map(|()| styling)
    }

    const FULL: &str = r#"{"protect":{"on":true,"deny":["selectLockedCells","formatCells","insertRows","objects"],"pwd":{"kind":"agile","alg":"SHA-512","hash":"AAAA","salt":"BBBB","spin":100000},"ranges":[{"name":"입력","sqref":["B2:C3","E5"],"pwd":{"kind":"legacy","hash":"83AF"},"descriptor":"O:S-1"}]},"unlockedRows":[[3,4]],"unlockedCols":[[2,2]]}"#;

    #[test]
    fn protection_round_trips_through_json() {
        let styling = read(FULL).unwrap();
        let p = styling.protect.as_ref().unwrap();
        assert!(
            p.on && !p.allows(easyoffice_formats::INSERT_ROWS)
                && p.allows(easyoffice_formats::SORT)
        );
        assert_eq!(p.ranges[0].areas.len(), 2);
        let mut out = String::new();
        write_sheet_protect(&mut out, &styling);
        assert_eq!(read(&format!("{{{}}}", &out[1..])).unwrap(), styling);
        assert_eq!(styling.unlocked_rows, vec![(2, 3)]);
        let mut none = String::new();
        write_sheet_protect(&mut none, &SheetStyling::default());
        assert!(none.is_empty());
    }

    #[test]
    fn hostile_protection_is_refused_or_kept_closed() {
        let long = "A".repeat(MAX_PROTECT_TEXT_CHARS + 1);
        for bad_text in [
            r#"{"protect":{"on":true,"deny":["nope"]}}"#.to_string(),
            r#"{"protect":{"on":"yes","deny":[]}}"#.to_string(),
            r#"{"protect":{"on":true}}"#.to_string(),
            r#"{"protect":{"on":true,"deny":[],"pwd":{"kind":"agile","alg":"SHA-512","hash":"A","salt":"B","spin":-1}}}"#.to_string(),
            r#"{"protect":{"on":true,"deny":[],"pwd":{"kind":"agile","alg":"SHA-512","hash":"A","salt":"B","spin":4294967296}}}"#.to_string(),
            r#"{"protect":{"on":true,"deny":[],"pwd":{"kind":"other"}}}"#.to_string(),
            r#"{"protect":{"on":true,"deny":[],"pwd":{"kind":"legacy","hash":"zz"}}}"#.to_string(),
            format!(r#"{{"protect":{{"on":true,"deny":[],"pwd":{{"kind":"odf","key":"{long}"}}}}}}"#),
            r#"{"protect":{"on":true,"deny":[],"ranges":[{"name":"x","sqref":["??"]}]}}"#.to_string(),
            r#"{"unlockedRows":[[0,3]]}"#.to_string(),
        ] {
            assert!(read(&bad_text).is_err(), "{bad_text}");
        }
        // A huge spin or an unknown algorithm is kept (and never checked).
        let kept = read(r#"{"protect":{"on":true,"deny":[],"pwd":{"kind":"agile","alg":"WHIRLPOOL","hash":"A","salt":"B","spin":4294967295}}}"#).unwrap();
        assert!(!kept.protect.unwrap().credential.is_none());
    }

    #[test]
    fn workbook_protection_round_trips() {
        let json = StrictJsonValue::parse(
            r#"{"structure":true,"windows":false,"pwd":{"kind":"odf","key":"AAAA","alg":"urn:x"},"revisions":[["lockRevision","1"]]}"#,
        )
        .unwrap();
        let p = parse_workbook_protect(&json).unwrap().unwrap();
        assert!(p.lock_structure);
        let again = StrictJsonValue::parse(&workbook_protect_json(&p)).unwrap();
        assert_eq!(parse_workbook_protect(&again).unwrap().unwrap(), p);
        assert!(parse_workbook_protect(&StrictJsonValue::Null)
            .unwrap()
            .is_none());
    }
}
