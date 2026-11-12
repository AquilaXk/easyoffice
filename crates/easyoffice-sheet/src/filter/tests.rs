use super::*;
use easyoffice_formats::{CellBox, CustomCondition, CustomOp, FilterColumn, FilterCriteria};

fn cond(op: Option<CustomOp>, text: &str) -> CustomCondition {
    CustomCondition {
        op,
        text: text.to_string(),
    }
}

fn one(op: Option<CustomOp>, text: &str) -> FilterCriteria {
    FilterCriteria::Custom {
        and: false,
        first: cond(op, text),
        second: None,
    }
}

fn two(and: bool, a: (Option<CustomOp>, &str), b: (Option<CustomOp>, &str)) -> FilterCriteria {
    FilterCriteria::Custom {
        and,
        first: cond(a.0, a.1),
        second: Some(cond(b.0, b.1)),
    }
}

fn values(blank: bool, list: &[&str]) -> FilterCriteria {
    FilterCriteria::Values {
        blank,
        values: list.iter().map(|s| s.to_string()).collect(),
    }
}

fn accepts(criteria: &FilterCriteria, text: &str) -> bool {
    ColumnMatcher::compile(criteria)
        .unwrap()
        .accepts(text, &mut Vec::new())
}

use CustomOp::{Equal, GreaterThan, GreaterThanOrEqual, LessThan, LessThanOrEqual, NotEqual};

/// One row per (condition, cell text, shown?).
#[test]
fn the_judgement_table() {
    let cases: Vec<(FilterCriteria, &str, bool)> = vec![
        // Numeric operators accept number cells only.
        (one(Some(GreaterThan), "5"), "6", true),
        (one(Some(GreaterThan), "5"), "5", false),
        (one(Some(GreaterThan), "5"), "abc", false),
        (one(Some(GreaterThan), "5"), "", false),
        (one(Some(GreaterThanOrEqual), "5"), "5", true),
        (one(Some(LessThan), "5"), "4.5", true),
        (one(Some(LessThan), "5"), "5", false),
        (one(Some(LessThanOrEqual), "5"), "5", true),
        (one(Some(LessThanOrEqual), "5"), "-1e2", true),
        (one(Some(Equal), "5"), "5.0", true),
        (one(Some(Equal), "5"), "6", false),
        (one(Some(Equal), "5"), "five", false),
        // A numeric operator whose text is not a number accepts nothing.
        (one(Some(GreaterThan), "abc"), "6", false),
        (one(Some(Equal), ""), "", false),
        // "Not equal": a number against a number, a pattern against a text.
        (one(Some(NotEqual), "5"), "6", true),
        (one(Some(NotEqual), "5"), "5", false),
        (one(Some(NotEqual), "5"), "abc", true),
        (one(Some(NotEqual), "5"), "", true),
        (one(Some(NotEqual), "abc"), "ABC", false),
        (one(Some(NotEqual), "abc"), "abd", true),
        (one(Some(NotEqual), "abc"), "", true),
        (one(Some(NotEqual), "*b*"), "abc", false),
        (one(Some(NotEqual), "*b*"), "xyz", true),
        (one(Some(NotEqual), "*b*"), "", true),
        // A pattern of one space is "not empty".
        (one(Some(NotEqual), " "), "x", true),
        (one(Some(NotEqual), " "), "", false),
        // Text match: the whole text, letter case ignored, empty only against "".
        (one(None, "abc"), "ABC", true),
        (one(None, "abc"), "abcd", false),
        (one(None, ""), "", true),
        (one(None, ""), "x", false),
        (one(None, "*"), "", false),
        (one(None, "*"), "anything", true),
        (one(None, "a*"), "apple", true),
        (one(None, "a*"), "banana", false),
        (one(None, "*e"), "apple", true),
        (one(None, "*pp*"), "apple", true),
        (one(None, "a?ple"), "apple", true),
        (one(None, "a?ple"), "aple", false),
        (one(None, "a~*"), "a*", true),
        (one(None, "a~*"), "ab", false),
        (one(None, "한*"), "한글", true),
        // A plain number typed as text equals the number cell with that value.
        (one(None, "1.0"), "1", true),
        (one(None, "1.0"), "1.5", false),
        (one(None, "5"), "5", true),
        // Two conditions.
        (
            two(
                true,
                (Some(GreaterThanOrEqual), "10"),
                (Some(LessThanOrEqual), "20"),
            ),
            "15",
            true,
        ),
        (
            two(
                true,
                (Some(GreaterThanOrEqual), "10"),
                (Some(LessThanOrEqual), "20"),
            ),
            "25",
            false,
        ),
        (
            two(false, (Some(LessThan), "10"), (Some(GreaterThan), "20")),
            "15",
            false,
        ),
        (
            two(false, (Some(LessThan), "10"), (Some(GreaterThan), "20")),
            "25",
            true,
        ),
        (two(false, (None, "a*"), (None, "*z")), "xyz", true),
        (two(true, (None, "a*"), (None, "*z")), "xyz", false),
        // A list of values: exact text, with the blank switch.
        (values(false, &["a", "b"]), "a", true),
        (values(false, &["a", "b"]), "A", false),
        (values(false, &["a", "b"]), "", false),
        (values(true, &["a"]), "", true),
        (values(true, &[]), "a", false),
        (values(false, &["1,000"]), "1,000", true),
    ];
    for (criteria, text, shown) in cases {
        assert_eq!(accepts(&criteria, text), shown, "{criteria:?} on {text:?}");
    }
}

#[test]
fn a_pattern_over_the_bound_is_refused() {
    let long = "a".repeat(MAX_PATTERN_CHARS + 1);
    assert!(ColumnMatcher::compile(&one(None, &long)).is_none());
    assert!(ColumnMatcher::compile(&one(Some(NotEqual), &long)).is_none());
}

fn texts(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn filter_over(rows: u32, columns: Vec<FilterColumn>) -> SheetFilter {
    let mut filter = SheetFilter::new(CellBox {
        row0: 0,
        col0: 0,
        row1: rows,
        col1: 2,
    });
    filter.columns = columns;
    filter
}

#[test]
fn rows_hidden_by_any_column_and_runs_from_the_row_below_the_header() {
    let dept = texts(&["영업", "개발", "영업", "", "개발", "영업"]);
    let sales = texts(&["10", "20", "30", "40", "abc", "50"]);
    let filter = filter_over(
        6,
        vec![
            FilterColumn {
                col: 0,
                criteria: values(false, &["영업"]),
            },
            FilterColumn {
                col: 1,
                criteria: one(Some(GreaterThan), "15"),
            },
        ],
    );
    let data = [
        ColumnText {
            col: 0,
            texts: &dept,
        },
        ColumnText {
            col: 1,
            texts: &sales,
        },
    ];
    // Rows 2, 4, 5 (the data row 0 is sheet row 1): hidden are 개발, the blank, 개발 ("abc"), and
    // the first 영업 whose 10 is not above 15.
    let bits = hidden_bits(&filter, &data, None).unwrap();
    assert_eq!(
        (0..6).filter(|&i| bits.get(i)).collect::<Vec<_>>(),
        vec![0, 1, 3, 4]
    );
    assert_eq!(hidden_runs(&filter, &data).unwrap(), vec![(1, 2), (4, 5)]);
    // Without the second column only the first one hides.
    let only_dept = hidden_bits(&filter, &data, Some(1)).unwrap();
    assert_eq!(
        (0..6).filter(|&i| only_dept.get(i)).collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
    // A missing or short column is refused.
    assert!(hidden_runs(&filter, &data[..1]).is_err());
    let short = texts(&["1"]);
    let bad = [
        data[0],
        ColumnText {
            col: 1,
            texts: &short,
        },
    ];
    assert!(hidden_runs(&filter, &bad).is_err());
}

#[test]
fn the_value_list_counts_sorts_searches_and_is_bounded() {
    let col = texts(&["b", "10", "a", "2", "B", "", "a", "x", ""]);
    let mut excluded = RowBits::new(col.len());
    let all = value_list(&col, &excluded, "", 100);
    let names: Vec<(&str, u32)> = all
        .items
        .iter()
        .map(|i| (i.text.as_str(), i.count))
        .collect();
    // Numbers first by value, then texts without regard to case ("B" after "b" by raw order).
    assert_eq!(
        names,
        vec![("2", 1), ("10", 1), ("a", 2), ("B", 1), ("b", 1), ("x", 1)]
    );
    assert_eq!((all.blank, all.more), (2, false));
    // A row another column hides is not counted.
    excluded.set(2);
    excluded.set(5);
    let part = value_list(&col, &excluded, "", 100);
    assert_eq!(part.blank, 1);
    assert_eq!(part.items.iter().find(|i| i.text == "a").unwrap().count, 1);
    // The search keeps the texts that contain it, case aside, and drops the blank count.
    let found = value_list(&col, &RowBits::new(col.len()), "B", 100);
    assert_eq!(
        found
            .items
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>(),
        vec!["B", "b"]
    );
    assert_eq!(found.blank, 0);
    // Over the limit the first texts met are kept and `more` says so; counts stay right.
    let many: Vec<String> = (0..50).map(|i| format!("v{}", i % 25)).collect();
    let capped = value_list(&many, &RowBits::new(many.len()), "", 10);
    assert!(capped.more && capped.items.len() == 10);
    assert!(capped.items.iter().all(|i| i.count == 2));
    // A text longer than a condition may hold is not listed.
    let long = vec!["x".repeat(300), "y".to_string()];
    assert_eq!(value_list(&long, &RowBits::new(2), "", 10).items.len(), 1);
}

#[test]
fn a_file_s_hidden_rows_split_into_the_filter_s_and_the_user_s() {
    let dept = texts(&["영업", "개발", "영업", "개발"]);
    let mut filter = filter_over(
        4,
        vec![FilterColumn {
            col: 0,
            criteria: values(false, &["영업"]),
        }],
    );
    // The file hides rows 2, 4 (개발), row 3 (a user's, 영업 stays by the filter) and row 9.
    let mut hidden = vec![(2, 4), (9, 9)];
    split_hidden_rows(
        &mut filter,
        &mut hidden,
        &[ColumnText {
            col: 0,
            texts: &dept,
        }],
    )
    .unwrap();
    assert_eq!(filter.hidden, vec![(2, 2), (4, 4)]);
    assert_eq!(hidden, vec![(3, 3), (9, 9)]);
    // Rows the conditions would hide but the file shows stay shown.
    let mut shown = filter_over(4, filter.columns.clone());
    let mut hidden = vec![(2, 2)];
    split_hidden_rows(
        &mut shown,
        &mut hidden,
        &[ColumnText {
            col: 0,
            texts: &dept,
        }],
    )
    .unwrap();
    assert_eq!(shown.hidden, vec![(2, 2)]);
    assert!(hidden.is_empty());
    // A filter without conditions leaves everything to the user.
    let mut plain = filter_over(4, Vec::new());
    let mut hidden = vec![(2, 3)];
    split_hidden_rows(&mut plain, &mut hidden, &[]).unwrap();
    assert!(plain.hidden.is_empty() && hidden == vec![(2, 3)]);
}
