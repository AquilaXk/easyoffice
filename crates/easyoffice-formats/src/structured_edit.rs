//! Two edits of the formulas that name a table (PD-231): the table gets a new name, or it becomes a plain
//! range. Both look at the structured references (`structured_ref::rewrite_refs`) and at the bare table name
//! (`=SUM(Sales)`, the data rows), outside quoted text and outside bracket groups.

use crate::sheet_table::SheetTable;
use crate::structured_ref::{
    a1_text, column_position, group_end, resolve, rewrite_refs, same_name, Columns, RefError,
    StructuredRef, TableLookup, ITEM_THIS_ROW,
};

#[inline]
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'\\') || !b.is_ascii()
}

/// The end of the quoted run (string or sheet name) opening at `i`.
fn quoted_end(bytes: &[u8], i: usize) -> usize {
    let quote = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == quote {
            if bytes.get(j + 1) == Some(&quote) {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    bytes.len()
}

/// `text` with each bare occurrence of the name `name` (a name that is not a function call, a sheet name
/// before `!`, a part of a longer name or a bracket group) replaced by `replace(row)`'s answer; `None` when
/// nothing is replaced.
fn replace_bare(text: &str, name: &str, mut replace: impl FnMut() -> String) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            i = quoted_end(bytes, i);
        } else if b == b'[' {
            i = group_end(text, i).unwrap_or(i + 1);
        } else if is_name_byte(b) {
            let start = i;
            while i < bytes.len() && is_name_byte(bytes[i]) {
                i += 1;
            }
            let after = text[i..]
                .trim_start_matches(' ')
                .as_bytes()
                .first()
                .copied();
            let before = start.checked_sub(1).map(|p| bytes[p]);
            if same_name(&text[start..i], name)
                && !matches!(after, Some(b'(' | b'!' | b'['))
                && before != Some(b'!')
            {
                let buf = out.get_or_insert_with(|| String::with_capacity(text.len() + 16));
                buf.push_str(&text[copied..start]);
                buf.push_str(&replace());
                copied = i;
            }
        } else {
            i += 1;
        }
    }
    out.map(|mut buf| {
        buf.push_str(&text[copied..]);
        buf
    })
}

/// `formula` (text without `=`) with the table `from` named `to`: `Sales[Q1]` and the bare `Sales`.
pub fn rename_table_refs(formula: &str, from: &str, to: &str) -> Option<String> {
    let qualified = rewrite_refs(formula, |found| {
        let table = found.table.filter(|t| same_name(t, from))?;
        Some(format!(
            "{to}{}",
            &formula[found.start + table.len()..found.end]
        ))
    });
    let current = qualified.as_deref().unwrap_or(formula);
    replace_bare(current, from, || to.to_string()).or(qualified)
}

/// `formula` (text without `=`) with every reference to `table` written as the cells it stands for, as
/// `to_range_refs` does for all the tables; the references to other tables stay. `sheet` is the sheet of the
/// formula and `row` its zero-based row; `lookup` knows `table` only.
pub fn convert_table_refs(
    formula: &str,
    sheet: &str,
    row: u32,
    lookup: &TableLookup<'_>,
    table: &SheetTable,
    table_sheet: &str,
) -> Option<String> {
    let same_sheet = same_name(table_sheet, sheet);
    let prefix = (!same_sheet).then_some(table_sheet);
    let converted = rewrite_refs(formula, |found| {
        let named = found.table.or(lookup.own.map(|(_, t)| t.name.as_str()))?;
        if !same_name(named, &table.name) {
            return None;
        }
        let resolved = found
            .sref
            .as_ref()
            .ok_or(RefError::Ref)
            .and_then(|sref: &StructuredRef| {
                let this_row = (same_sheet && sref.items == ITEM_THIS_ROW).then_some(row);
                let cells = resolve(sref, table, |n| column_position(table, n), this_row)?;
                Ok(a1_text(cells, prefix, sref.items == ITEM_THIS_ROW))
            });
        Some(match resolved {
            Ok(text) => text,
            Err(RefError::Ref) => "#REF!".to_string(),
            Err(RefError::Value) => "#VALUE!".to_string(),
        })
    });
    let current = converted.as_deref().unwrap_or(formula);
    let data = StructuredRef {
        items: 0,
        columns: Columns::All,
        at: false,
    };
    let bare = resolve(&data, table, |n| column_position(table, n), None)
        .map(|cells| a1_text(cells, prefix, false));
    match bare {
        Ok(text) => replace_bare(current, &table.name, || text.clone()).or(converted),
        Err(_) => converted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CellBox;

    fn sales() -> SheetTable {
        let mut t = SheetTable::new("Sales", CellBox::parse("B2:D6").unwrap());
        t.columns = vec!["Region".into(), "Q1".into(), "Q2".into()];
        t
    }

    #[test]
    fn a_renamed_table_is_renamed_in_references_and_bare_uses_only() {
        let r = |f: &str| rename_table_refs(f, "Sales", "Orders");
        assert_eq!(r("SUM(Sales[Q1])").as_deref(), Some("SUM(Orders[Q1])"));
        assert_eq!(
            r("SUM(sales[[#Totals],[Q1]])").as_deref(),
            Some("SUM(Orders[[#Totals],[Q1]])")
        );
        assert_eq!(
            r("COUNTA(Sales)+Sales[Q2]").as_deref(),
            Some("COUNTA(Orders)+Orders[Q2]")
        );
        assert_eq!(
            r("[@Q1]*2"),
            None,
            "a reference inside the table names none"
        );
        assert_eq!(r("Sales!A1"), None, "a sheet of that name is not the table");
        assert_eq!(r("'Sales'!A1+\"Sales\""), None);
        assert_eq!(
            r("SalesTotal+Sales2"),
            None,
            "a longer name is another name"
        );
        assert_eq!(
            r("Other[Sales]"),
            None,
            "a column of that name is not the table"
        );
        assert_eq!(r("Sales(1)"), None, "a call is not a table");
    }

    #[test]
    fn a_converted_table_becomes_ranges_and_other_tables_stay() {
        let table = sales();
        let find = |n: &str| same_name(n, "Sales").then_some(("Data", &table));
        let lookup = TableLookup {
            find: &find,
            own: None,
        };
        let c = |f: &str, sheet: &str, row: u32| {
            convert_table_refs(f, sheet, row, &lookup, &table, "Data")
        };
        assert_eq!(
            c("SUM(Sales[Q1])", "Data", 0).as_deref(),
            Some("SUM($C$3:$C$6)")
        );
        assert_eq!(
            c("SUM(Sales[Q1])", "Other", 0).as_deref(),
            Some("SUM(Data!$C$3:$C$6)")
        );
        assert_eq!(
            c("Sales[[#Totals],[Q1]]", "Data", 0).as_deref(),
            Some("#REF!")
        );
        assert_eq!(
            c("SUM(Sales)+Other[Q1]", "Data", 0).as_deref(),
            Some("SUM($B$3:$D$6)+Other[Q1]")
        );
        assert_eq!(c("Other[Q1]", "Data", 0), None);
        let own = TableLookup {
            find: &find,
            own: Some(("Data", &table)),
        };
        assert_eq!(
            convert_table_refs("[@Q1]*2", "Data", 3, &own, &table, "Data").as_deref(),
            Some("$C4*2")
        );
        assert_eq!(
            convert_table_refs("[@Q1]", "Data", 0, &own, &table, "Data").as_deref(),
            Some("#VALUE!")
        );
    }
}
