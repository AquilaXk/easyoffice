//! `[MS-XLSX]` `_xleta.` eta-reduced built-in round-trip tests.

use easyoffice_formats::{
    from_file_formula, to_file_formula, OoxmlCodec, SheetStyling, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn part(bytes: &[u8], path: &str) -> Option<String> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    entries
        .iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8(e.data.clone()).unwrap())
}

#[test]
fn to_file_formula_prefixes_eta_builtins_only_in_eta_slots() {
    let cases = [
        (
            "GROUPBY(A1:A5, B1:B5, SUM)",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xleta.SUM)",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, PERCENTOF)",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xleta.PERCENTOF)",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, HSTACK(COUNT, SUM, PERCENTOF))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.HSTACK(_xleta.COUNT, _xleta.SUM, _xleta.PERCENTOF))",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, VSTACK(HSTACK(COUNT, SUM, PERCENTOF), {\"Count\", \"Cost\", \"%\"}))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.VSTACK(_xlfn.HSTACK(_xleta.COUNT, _xleta.SUM, _xleta.PERCENTOF), {\"Count\", \"Cost\", \"%\"}))",
        ),
        (
            "PIVOTBY(A1:A5, B1:B5, C1:C5, HSTACK(SUM, AVERAGE))",
            "_xlfn.PIVOTBY(A1:A5, B1:B5, C1:C5, _xlfn.HSTACK(_xleta.SUM, _xleta.AVERAGE))",
        ),
        (
            "PIVOTBY(, B1:B5, C1:C5, SUM)",
            "_xlfn.PIVOTBY(, B1:B5, C1:C5, _xleta.SUM)",
        ),
        (
            "PIVOTBY(A1:A5, , C1:C5, VSTACK(SUM, COUNT))",
            "_xlfn.PIVOTBY(A1:A5, , C1:C5, _xlfn.VSTACK(_xleta.SUM, _xleta.COUNT))",
        ),
        ("BYROW(A1:C5, SUM)", "_xlfn.BYROW(A1:C5, _xleta.SUM)"),
        (
            "BYCOL(A1:C5, AVERAGE)",
            "_xlfn.BYCOL(A1:C5, _xleta.AVERAGE)",
        ),
        (
            "MAP(A1:A5, B1:B5, SUM)",
            "_xlfn.MAP(A1:A5, B1:B5, _xleta.SUM)",
        ),
        ("REDUCE(0, A1:A5, MAX)", "_xlfn.REDUCE(0, A1:A5, _xleta.MAX)"),
        ("REDUCE(A1:A5, SUM)", "_xlfn.REDUCE(A1:A5, _xleta.SUM)"),
        ("SCAN(0, A1:A5, SUM)", "_xlfn.SCAN(0, A1:A5, _xleta.SUM)"),
        ("SCAN(A1:A5, MAX)", "_xlfn.SCAN(A1:A5, _xleta.MAX)"),
        (
            "MAKEARRAY(2, 3, MIN)",
            "_xlfn.MAKEARRAY(2, 3, _xleta.MIN)",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, (SUM))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, (_xleta.SUM))",
        ),
    ];
    for (page, file) in cases {
        assert_eq!(to_file_formula(page), file, "to_file {page}");
        assert_eq!(from_file_formula(file), page, "from_file {file}");
    }
}

#[test]
fn to_file_formula_leaves_outside_identifiers_variables_and_lambda_bodies_alone() {
    let preserved = [
        ("N+1", "N+1"),
        ("T*2", "T*2"),
        ("SUM+1", "SUM+1"),
        ("PI+1", "PI+1"),
        ("HSTACK(N, T)", "_xlfn.HSTACK(N, T)"),
        ("HSTACK(SUM, AVERAGE)", "_xlfn.HSTACK(SUM, AVERAGE)"),
        (
            "GROUPBY(A1:A5, B1:B5, LAMBDA(x, COUNT(x) + SUM(x) + N))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.LAMBDA(_xlpm.x, COUNT(_xlpm.x) + SUM(_xlpm.x) + N))",
        ),
        (
            "LET(SUM, 10, GROUPBY(A1:A5, B1:B5, SUM))",
            "_xlfn.LET(_xlpm.SUM, 10, _xlfn.GROUPBY(A1:A5, B1:B5, _xlpm.SUM))",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, Sheet1!SUM)",
            "_xlfn.GROUPBY(A1:A5, B1:B5, Sheet1!SUM)",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, CUSTOM_NAME)",
            "_xlfn.GROUPBY(A1:A5, B1:B5, CUSTOM_NAME)",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, HSTACK(SUM, TRUE))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.HSTACK(_xleta.SUM, TRUE))",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, SUM(B1:B5))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, SUM(B1:B5))",
        ),
        (
            "BYROW(A1:C5, LET(x, 1, LAMBDA(r, SUM(r) + x)))",
            "_xlfn.BYROW(A1:C5, _xlfn.LET(_xlpm.x, 1, _xlfn.LAMBDA(_xlpm.r, SUM(_xlpm.r) + _xlpm.x)))",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, HSTACK(SUM, DATE[Col]))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.HSTACK(_xleta.SUM, DATE[Col]))",
        ),
        (
            "GROUPBY(A1:A5, B1:B5, HSTACK(SUM, LOG10:LOG20))",
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.HSTACK(_xleta.SUM, LOG10:LOG20))",
        ),
        (
            "LET (SUM, 10, GROUPBY(A1:A5, B1:B5, SUM))",
            "_xlfn.LET (_xlpm.SUM, 10, _xlfn.GROUPBY(A1:A5, B1:B5, _xlpm.SUM))",
        ),
    ];
    for (page, expected) in preserved {
        assert_eq!(to_file_formula(page), expected, "page={page}");
    }
}

#[test]
fn from_file_formula_strips_all_xleta_prefix_combinations_and_preserves_quotes() {
    assert_eq!(
        from_file_formula(
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.HSTACK(_xleta.SUM, _xleta.PERCENTOF))"
        ),
        "GROUPBY(A1:A5, B1:B5, HSTACK(SUM, PERCENTOF))"
    );
    assert_eq!(
        from_file_formula(
            "_xlfn.GROUPBY(A1:A5, B1:B5, _xlfn.HSTACK(_xleta._xlfn.PERCENTOF, _xlfn._xleta.SUM))"
        ),
        "GROUPBY(A1:A5, B1:B5, HSTACK(PERCENTOF, SUM))"
    );
    assert_eq!(
        from_file_formula("_XLETA._XLFN._XLWS.SORT + _xlfn._xleta._xlws.FILTER + _XLETA.MAX"),
        "SORT + FILTER + MAX"
    );
    assert_eq!(
        from_file_formula("_xleta._xlws.SORT(A1:A5) + _xlws._xleta.FILTER(A1:A5, B1:B5>0)"),
        "SORT(A1:A5) + FILTER(A1:A5, B1:B5>0)"
    );
    assert_eq!(from_file_formula("_xleta."), "_xleta.");
    assert_eq!(
        from_file_formula("_xlfn.BYROW(A1:C5, _xleta.SUM)"),
        "BYROW(A1:C5, SUM)"
    );
    assert_eq!(
        from_file_formula("=CONCAT(\"_xleta.SUM\", \" \", _xleta.SUM)"),
        "=CONCAT(\"_xleta.SUM\", \" \", SUM)"
    );
}

#[test]
fn xlsx_pack_and_unpack_roundtrips_xleta_groupby_pivotby_and_byrow() {
    let formulas = vec![
        (
            0,
            0,
            "=GROUPBY(A2:A6, B2:B6, HSTACK(COUNT, SUM, PERCENTOF))".to_string(),
        ),
        (
            1,
            0,
            "=PIVOTBY(, B2:B6, C2:C6, VSTACK(SUM, AVERAGE))".to_string(),
        ),
        (2, 0, "=BYROW(A2:B6, SUM)".to_string()),
        (3, 0, "=N+1".to_string()),
    ];
    let sheets = vec![("Sheet1".to_string(), formulas.clone())];
    let bytes =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[SheetStyling::default()]).unwrap();
    let sheet_xml = part(&bytes, "xl/worksheets/sheet1.xml").unwrap();
    assert!(
        sheet_xml.contains(
            "_xlfn.GROUPBY(A2:A6, B2:B6, _xlfn.HSTACK(_xleta.COUNT, _xleta.SUM, _xleta.PERCENTOF))"
        ),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml
            .contains("_xlfn.PIVOTBY(, B2:B6, C2:C6, _xlfn.VSTACK(_xleta.SUM, _xleta.AVERAGE))"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("_xlfn.BYROW(A2:B6, _xleta.SUM)"),
        "{sheet_xml}"
    );
    assert!(sheet_xml.contains(">N+1<"), "{sheet_xml}");

    let (unpacked, _) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    assert_eq!(unpacked[0].1, formulas);
}
