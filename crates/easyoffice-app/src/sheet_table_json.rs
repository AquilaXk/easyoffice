//! JSON form of a sheet's tables (PD-224), shared by the open answer, the save request and the row/column
//! edit of a sheet session.
//!
//! `tables`: `[{"name":"Table1","range":"A1:C5","header":true,"totals":false,"columns":["a","b","c"],
//! "style":"TableStyleMedium2","firstColumn":false,"lastColumn":false,"rowStripes":true,
//! "columnStripes":false,"filter":[...]}, ...]`. `header` defaults to true and `rowStripes` to true, the
//! other flags to false; `columns` has one name per column of the range; `filter` is absent for a table
//! without filter buttons, else the conditions of its columns as in `sheet_filter_json`, and
//! `filterHidden` lists the rows (1-based runs) those conditions hide (PD-226). The ranges of
//! the table that a session needs to place arrays are `["A1:C5", ...]`. Input over a bound is refused,
//! not trimmed.

use crate::sheet_filter_json::{
    parse_columns, parse_hidden_buttons, write_columns, write_hidden_buttons,
};
use crate::sheet_layout_json::{parse_runs_limited, runs_json};
use crate::table_style_json::{parse_style_def, write_style_def};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    parse_hex_rgb, tables_within_bounds, CellBox, EasyOfficeError, SheetTable, StrictJsonValue,
    TableSlicer, DEFAULT_THEME, MAX_FILTER_ROWS, MAX_SLICERS_PER_TABLE, MAX_SLICER_DIM_PX,
    MAX_SLICER_POS_PX, MAX_TABLES, MAX_TABLE_TEXT_CHARS, MIN_SLICER_DIM_PX,
};
use std::fmt::Write;

const LAST_ROW: u32 = 1_048_575;
const LAST_COL: u32 = 16_383;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("표 JSON이 올바르지 않습니다: {what}"))
}

fn over() -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(format!(
        "표 개수·열 개수·범위가 한도({MAX_TABLES}개)를 넘었습니다."
    ))
}

fn parse_range(v: &StrictJsonValue, what: &str) -> Result<CellBox, EasyOfficeError> {
    v.as_str()
        .filter(|t| t.contains(':') && t.bytes().any(|b| b.is_ascii_digit()))
        .and_then(CellBox::parse)
        .filter(|r| r.row1 <= LAST_ROW && r.col1 <= LAST_COL)
        .ok_or_else(|| bad(what))
}

fn text_of(v: &StrictJsonValue, what: &str) -> Result<String, EasyOfficeError> {
    v.as_str()
        .filter(|t| t.chars().count() <= MAX_TABLE_TEXT_CHARS)
        .map(str::to_string)
        .ok_or_else(|| bad(what))
}

fn parse_table(v: &StrictJsonValue) -> Result<SheetTable, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("tables"))?;
    let field = |key: &str| StrictJsonValue::get_field(obj, key);
    let flag = |key: &str, default: bool| match field(key) {
        None | Some(StrictJsonValue::Null) => Ok(default),
        Some(StrictJsonValue::Bool(on)) => Ok(*on),
        Some(_) => Err(bad(key)),
    };
    let range = parse_range(field("range").ok_or_else(|| bad("range"))?, "range")?;
    let name = text_of(field("name").ok_or_else(|| bad("name"))?, "name")?;
    let mut table = SheetTable::new(name, range);
    table.header = flag("header", true)?;
    table.totals = flag("totals", false)?;
    table.first_column = flag("firstColumn", false)?;
    table.last_column = flag("lastColumn", false)?;
    table.row_stripes = flag("rowStripes", true)?;
    table.column_stripes = flag("columnStripes", false)?;
    let columns = field("columns")
        .and_then(StrictJsonValue::as_array)
        .filter(|c| c.len() == table.width() as usize)
        .ok_or_else(|| bad("columns"))?;
    table.columns = columns
        .iter()
        .map(|c| text_of(c, "columns"))
        .collect::<Result<_, _>>()?;
    table.style = match field("style") {
        None | Some(StrictJsonValue::Null) => None,
        Some(style) => Some(text_of(style, "style")?),
    };
    table.style_def = match field("styleDef") {
        None | Some(StrictJsonValue::Null) => None,
        Some(def) => Some(parse_style_def(def)?),
    };
    table.filter = match field("filter") {
        None | Some(StrictJsonValue::Null) => None,
        Some(conditions) => Some(parse_columns(conditions, &range)?),
    };
    if let Some(hidden) = field("filterHidden") {
        table.filter_hidden = parse_runs_limited(
            hidden,
            LAST_ROW,
            "table.filterHidden",
            u64::from(MAX_FILTER_ROWS),
        )?;
    }
    if let Some(hb) = field("hiddenButtons").filter(|v| !matches!(v, StrictJsonValue::Null)) {
        if table.filter.is_none() {
            return Err(bad("hiddenButtons"));
        }
        table.hidden_buttons = parse_hidden_buttons(hb, &range)?;
    }
    if let Some(calc) = field("calculatedColumns").filter(|v| !matches!(v, StrictJsonValue::Null)) {
        let items = calc
            .as_array()
            .filter(|a| a.len() == table.width() as usize)
            .ok_or_else(|| bad("calculatedColumns"))?;
        let mut parsed = Vec::with_capacity(items.len());
        for item in items {
            if matches!(item, StrictJsonValue::Null) {
                parsed.push(None);
            } else if let Some(f) = item
                .as_str()
                .filter(|f| f.starts_with('=') && f.len() > 1 && f.chars().count() <= 8_192)
            {
                parsed.push(Some(f.to_string()));
            } else {
                return Err(bad("calculatedColumns"));
            }
        }
        if parsed.iter().any(Option::is_some) {
            table.calculated_columns = parsed;
        }
    }
    if let Some(sl_val) = field("slicers").filter(|v| !matches!(v, StrictJsonValue::Null)) {
        if !table.header {
            return Err(bad("slicers"));
        }
        let items = sl_val.as_array().ok_or_else(|| bad("slicers"))?;
        if items.len() > MAX_SLICERS_PER_TABLE {
            return Err(over());
        }
        let width = table.width();
        let mut parsed = Vec::with_capacity(items.len());
        for item in items {
            parsed.push(parse_slicer(item, width)?);
        }
        table.slicers = parsed;
    }
    if !table.within_bounds() {
        return Err(over());
    }
    Ok(table)
}

fn parse_slicer(v: &StrictJsonValue, width: u32) -> Result<TableSlicer, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("slicers"))?;
    let field = |key: &str| StrictJsonValue::get_field(obj, key);
    let u32_of = |key: &str, lo: u32, hi: u32| -> Result<u32, EasyOfficeError> {
        field(key)
            .and_then(StrictJsonValue::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| (lo..=hi).contains(n))
            .ok_or_else(|| bad("slicers"))
    };
    let col = field("col")
        .and_then(StrictJsonValue::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&c| c < width)
        .ok_or_else(|| bad("slicers"))?;
    let caption = field("caption")
        .map(|c| text_of(c, "slicers"))
        .transpose()?
        .filter(|c| !c.is_empty())
        .ok_or_else(|| bad("slicers"))?;
    let multi_select = match field("multiSelect") {
        None | Some(StrictJsonValue::Null) => false,
        Some(StrictJsonValue::Bool(on)) => *on,
        Some(_) => return Err(bad("slicers")),
    };
    Ok(TableSlicer {
        col,
        caption,
        left: u32_of("left", 0, MAX_SLICER_POS_PX)?,
        top: u32_of("top", 0, MAX_SLICER_POS_PX)?,
        width: u32_of("width", MIN_SLICER_DIM_PX, MAX_SLICER_DIM_PX)?,
        height: u32_of("height", MIN_SLICER_DIM_PX, MAX_SLICER_DIM_PX)?,
        multi_select,
    })
}

/// Parses `"workbookTheme"`: `null` (default Office theme) or an array of 12 6-hex RGB strings.
pub(crate) fn parse_workbook_theme(
    v: &StrictJsonValue,
) -> Result<Option<[u32; 12]>, EasyOfficeError> {
    if matches!(v, StrictJsonValue::Null) {
        return Ok(None);
    }
    let items = v.as_array().filter(|a| a.len() == 12).ok_or_else(|| {
        EasyOfficeError::InvalidArgument("workbookTheme은 12색 배열이어야 합니다.".into())
    })?;
    let mut out = [0u32; 12];
    for (slot, item) in out.iter_mut().zip(items) {
        let raw = item.as_str().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("workbookTheme 색상은 6자리 16진수여야 합니다.".into())
        })?;
        let trimmed = raw.strip_prefix('#').unwrap_or(raw);
        *slot = parse_hex_rgb(trimmed).ok_or_else(|| {
            EasyOfficeError::InvalidArgument("workbookTheme 색상은 6자리 16진수여야 합니다.".into())
        })?;
    }
    Ok((out != DEFAULT_THEME).then_some(out))
}

pub(crate) fn write_workbook_theme(out: &mut String, theme: &[u32; 12]) {
    out.push('[');
    for (i, &rgb) in theme.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{:06x}\"", rgb & 0xFF_FFFF);
    }
    out.push(']');
}

/// What a request says of the tables of one sheet: their ranges and, when it sends the tables themselves (names,
/// header and totals rows, columns), the tables, which structured references are resolved against (PD-225).
pub(crate) struct TablesField {
    pub(crate) boxes: Vec<CellBox>,
    pub(crate) tables: Option<Vec<SheetTable>>,
}

/// A sheet's `tables` field: `["A1:C5", ...]` (ranges only) or the table objects of [`parse_tables`]. An empty
/// list is "no tables" in either form and so takes the table form, which clears what the engine holds.
pub(crate) fn parse_tables_field(v: &StrictJsonValue) -> Result<TablesField, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("tables"))?;
    if items.is_empty() || items.iter().any(|item| item.as_object().is_some()) {
        let tables = parse_tables(v)?;
        let boxes = tables.iter().map(|t| t.range).collect();
        return Ok(TablesField {
            boxes,
            tables: Some(tables),
        });
    }
    Ok(TablesField {
        boxes: parse_table_ranges(v)?,
        tables: None,
    })
}

/// The tables of one sheet from their JSON; they must be within the bounds and share no cell.
pub(crate) fn parse_tables(v: &StrictJsonValue) -> Result<Vec<SheetTable>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("tables"))?;
    if items.len() > MAX_TABLES {
        return Err(over());
    }
    let tables: Vec<SheetTable> = items.iter().map(parse_table).collect::<Result<_, _>>()?;
    if !tables_within_bounds(&tables) {
        return Err(over());
    }
    Ok(tables)
}

/// The ranges a session is told for a sheet: `["A1:C5", ...]`.
pub(crate) fn parse_table_ranges(v: &StrictJsonValue) -> Result<Vec<CellBox>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("tables"))?;
    if items.len() > MAX_TABLES {
        return Err(over());
    }
    items
        .iter()
        .map(|item| parse_range(item, "tables"))
        .collect()
}

/// The `tables` field of a request without a session: `{"Sheet1": ["A1:C5", ...], ...}` (or the table objects
/// in place of the ranges); none when absent.
pub(crate) fn tables_by_sheet(
    entries: &[(String, StrictJsonValue)],
) -> Result<Vec<(String, TablesField)>, EasyOfficeError> {
    let Some(field) = StrictJsonValue::get_field(entries, "tables") else {
        return Ok(Vec::new());
    };
    let sheets = field.as_object().ok_or_else(|| bad("tables"))?;
    sheets
        .iter()
        .map(|(name, list)| Ok((name.clone(), parse_tables_field(list)?)))
        .collect()
}

/// `[{"name":..}, ...]` for `tables`.
pub(crate) fn tables_json(tables: &[SheetTable]) -> String {
    let mut out = String::with_capacity(64 + tables.len() * 96);
    out.push('[');
    for (i, t) in tables.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        EasyOfficeIpcRouter::escape_json_into(&mut out, &t.name);
        let _ = write!(
            out,
            "\",\"range\":\"{}\",\"header\":{},\"totals\":{},\"columns\":[",
            t.range.to_a1(),
            t.header,
            t.totals
        );
        for (j, column) in t.columns.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push('"');
            EasyOfficeIpcRouter::escape_json_into(&mut out, column);
            out.push('"');
        }
        out.push(']');
        if let Some(style) = &t.style {
            out.push_str(",\"style\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, style);
            out.push('"');
        }
        if let Some(def) = &t.style_def {
            out.push_str(",\"styleDef\":");
            write_style_def(&mut out, def);
        }
        let _ = write!(
            out,
            ",\"firstColumn\":{},\"lastColumn\":{},\"rowStripes\":{},\"columnStripes\":{}",
            t.first_column, t.last_column, t.row_stripes, t.column_stripes
        );
        if let Some(conditions) = &t.filter {
            out.push_str(",\"filter\":");
            write_columns(&mut out, conditions);
            if !t.filter_hidden.is_empty() {
                let _ = write!(out, ",\"filterHidden\":{}", runs_json(&t.filter_hidden));
            }
            write_hidden_buttons(&mut out, &t.hidden_buttons);
        }
        if !t.calculated_columns.is_empty() && t.calculated_columns.iter().any(Option::is_some) {
            out.push_str(",\"calculatedColumns\":[");
            for (j, col_f) in t.calculated_columns.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                match col_f {
                    Some(f) => {
                        out.push('"');
                        EasyOfficeIpcRouter::escape_json_into(&mut out, f);
                        out.push('"');
                    }
                    None => out.push_str("null"),
                }
            }
            out.push(']');
        }
        if !t.slicers.is_empty() {
            out.push_str(",\"slicers\":[");
            for (j, s) in t.slicers.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                let _ = write!(out, "{{\"col\":{},\"caption\":\"", s.col);
                EasyOfficeIpcRouter::escape_json_into(&mut out, &s.caption);
                let _ = write!(
                    out,
                    "\",\"left\":{},\"top\":{},\"width\":{},\"height\":{},\"multiSelect\":{}}}",
                    s.left, s.top, s.width, s.height, s.multi_select
                );
            }
            out.push(']');
        }
        out.push('}');
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Vec<SheetTable>, EasyOfficeError> {
        parse_tables(&StrictJsonValue::parse(text).unwrap())
    }

    #[test]
    fn tables_round_trip_through_json() {
        let text = concat!(
            r#"[{"name":"Sales","range":"B2:D9","header":true,"totals":true,"columns":["a","b\"<>&","c"],"#,
            r#""style":"TableStyleMedium2","firstColumn":true,"lastColumn":false,"rowStripes":false,"columnStripes":true,"#,
            r#""filter":[{"col":3,"blank":true,"values":["x"]}]},"#,
            r#"{"name":"Plain","range":"F1:F3","header":false,"totals":false,"columns":["p"],"#,
            r#""firstColumn":false,"lastColumn":false,"rowStripes":true,"columnStripes":false}]"#
        );
        let tables = parse(text).unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].columns[1], "b\"<>&");
        assert!(tables[0].totals && tables[0].filter.is_some() && !tables[1].header);
        assert_eq!(tables_json(&tables), text);
        assert_eq!(tables_json(&[]), "[]");
    }

    #[test]
    fn a_custom_style_definition_goes_with_its_table() {
        let text = concat!(
            r#"[{"name":"T","range":"A1:B3","header":true,"totals":false,"columns":["a","b"],"style":"Mine","#,
            r#""styleDef":{"h":{"f":"1f4e78","c":"ffffff","b":1}},"firstColumn":false,"lastColumn":false,"rowStripes":true,"columnStripes":false}]"#
        );
        let tables = parse(text).unwrap();
        assert!(tables[0].style_def.is_some());
        assert_eq!(tables_json(&tables), text);
        // A definition belongs to a style that is not a built-in one.
        assert!(parse(&text.replace("\"Mine\"", "\"TableStyleMedium2\"")).is_err());
        assert!(parse(&text.replace(",\"style\":\"Mine\"", "")).is_err());
    }

    #[test]
    fn missing_flags_take_the_defaults() {
        let t = &parse(r#"[{"name":"T","range":"A1:B3","columns":["a","b"]}]"#).unwrap()[0];
        assert!(t.header && !t.totals && t.row_stripes && t.style.is_none() && t.filter.is_none());
    }

    #[test]
    fn hostile_tables_are_refused() {
        let long = "n".repeat(MAX_TABLE_TEXT_CHARS + 1);
        let many: Vec<String> = (0..=MAX_TABLES)
            .map(|i| {
                format!(
                    r#"{{"name":"T{i}x","range":"A{}:A{}","columns":["a"]}}"#,
                    2 * i + 1,
                    2 * i + 2
                )
            })
            .collect();
        let flood = format!("[{}]", many.join(","));
        let cases = [
            r#"[{"name":"T","range":"A1","columns":["a"]}]"#.to_string(),
            r#"[{"name":"T","range":"A1:A1","columns":["a"]}]"#.to_string(),
            r#"[{"name":"T","range":"A1:B2","columns":["a"]}]"#.to_string(),
            r#"[{"name":"bad name","range":"A1:A2","columns":["a"]}]"#.to_string(),
            r#"[{"name":"A1","range":"A1:A2","columns":["a"]}]"#.to_string(),
            r#"[{"name":"a\"<>&","range":"A1:A2","columns":["a"]}]"#.to_string(),
            format!(r#"[{{"name":"{long}","range":"A1:A2","columns":["a"]}}]"#),
            format!(r#"[{{"name":"T","range":"A1:A2","columns":["{long}"]}}]"#),
            r#"[{"name":"T","range":"A1:A2","columns":["a"],"header":"yes"}]"#.to_string(),
            r#"[{"name":"T","range":"A1:A2","columns":["a"],"header":false,"filter":[]}]"#.to_string(),
            r#"[{"name":"T","range":"A1:A2","columns":["a"],"filter":[{"col":9,"values":[]}]}]"#.to_string(),
            r#"[{"name":"T","range":"A1:B3","columns":["a","b"]},{"name":"U","range":"B3:C5","columns":["a","b"]}]"#.to_string(),
            r#"[{"name":"T","range":"A1:A1048577","columns":["a"]}]"#.to_string(),
            r#"{"name":"T"}"#.to_string(),
            r#"[1]"#.to_string(),
            flood,
        ];
        for text in cases {
            assert!(parse(&text).is_err(), "{}", &text[..text.len().min(120)]);
        }
    }

    #[test]
    fn a_tables_field_is_ranges_or_tables() {
        let field = |t: &str| parse_tables_field(&StrictJsonValue::parse(t).unwrap());
        let ranges = field(r#"["A1:C5"]"#).unwrap();
        assert!(ranges.tables.is_none() && ranges.boxes.len() == 1);
        let full = field(r#"[{"name":"T","range":"A1:B3","columns":["a","b"]}]"#).unwrap();
        assert_eq!(full.tables.unwrap()[0].columns, ["a", "b"]);
        // An empty list clears the tables the engine holds.
        assert!(field("[]").unwrap().tables.is_some_and(|t| t.is_empty()));
        // The two forms do not mix.
        assert!(field(r#"["A1:C5",{"name":"T","range":"A7:B9","columns":["a","b"]}]"#).is_err());
    }

    #[test]
    fn session_ranges_are_rectangles() {
        let ranges = |t: &str| parse_table_ranges(&StrictJsonValue::parse(t).unwrap());
        assert_eq!(ranges(r#"["A1:C5","E2:E3"]"#).unwrap().len(), 2);
        assert!(
            ranges(r#"["A1"]"#).is_err()
                && ranges(r#"[3]"#).is_err()
                && ranges(r#"["A:C"]"#).is_err()
        );
    }

    #[test]
    fn slicers_and_workbook_theme_round_trip_and_reject_hostile_inputs() {
        let valid = r#"[{"name":"T","range":"A1:C4","header":true,"totals":false,"columns":["지역","품목","매출"],"firstColumn":false,"lastColumn":false,"rowStripes":true,"columnStripes":false,"slicers":[{"col":1,"caption":"품목","left":24,"top":32,"width":184,"height":210,"multiSelect":true}]}]"#;
        let tables = parse(valid).unwrap();
        assert_eq!(tables[0].slicers.len(), 1);
        assert_eq!(tables_json(&tables), valid);

        for bad_slicer in [
            r#"[{"name":"T","range":"A1:B3","header":false,"columns":["a","b"],"slicers":[{"col":0,"caption":"a","left":0,"top":0,"width":180,"height":200}]}]"#,
            r#"[{"name":"T","range":"A1:B3","columns":["a","b"],"slicers":[{"col":2,"caption":"a","left":0,"top":0,"width":180,"height":200}]}]"#,
            r#"[{"name":"T","range":"A1:B3","columns":["a","b"],"slicers":[{"col":0,"caption":"","left":0,"top":0,"width":180,"height":200}]}]"#,
            r#"[{"name":"T","range":"A1:B3","columns":["a","b"],"slicers":[{"col":0,"caption":"a","left":0,"top":0,"width":20,"height":200}]}]"#,
            r#"[{"name":"T","range":"A1:B3","columns":["a","b"],"slicers":[{"col":0,"caption":"a","left":0,"top":0,"width":180,"height":5000}]}]"#,
        ] {
            assert!(parse(bad_slicer).is_err(), "{bad_slicer}");
        }

        let theme_json = r##"["#ffffff","111827","f3f4f6","1f2937","0f766e","d97706","4f46e5","dc2626","0284c7","65a30d","2563eb","7c3aed"]"##;
        let parsed_theme = parse_workbook_theme(&StrictJsonValue::parse(theme_json).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(parsed_theme[4], 0x0F766E);
        let mut encoded = String::new();
        write_workbook_theme(&mut encoded, &parsed_theme);
        assert_eq!(
            encoded,
            r#"["ffffff","111827","f3f4f6","1f2937","0f766e","d97706","4f46e5","dc2626","0284c7","65a30d","2563eb","7c3aed"]"#
        );
        assert!(parse_workbook_theme(&StrictJsonValue::Null)
            .unwrap()
            .is_none());
        for bad_theme in [
            r#"["ffffff"]"#,
            r#"123"#,
            r#"["zzzzzz","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000","000000"]"#,
        ] {
            assert!(parse_workbook_theme(&StrictJsonValue::parse(bad_theme).unwrap()).is_err());
        }
    }
}
