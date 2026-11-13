//! Arrays that spill survive XLSX and ODS save -> reopen (PD-217): the anchor keeps its formula and
//! its area, the cells it filled are values in the file and are not cells after reading, and the
//! formula text has the file's spelling (`_xlfn.`, `_xlws.`, `_xlpm.`, `ANCHORARRAY`).

use easyoffice_formats::{
    DocumentFormat, OdfCodec, OdsUnpacked, OoxmlCodec, SheetStyling, SpillRef, XlsxCachedValue,
    XlsxFormulaCache, ZipArchiveCodec, ZipSecurityPolicy,
};

fn part(bytes: &[u8], path: &str) -> Option<String> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    entries
        .iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8(e.data.clone()).unwrap())
}

/// `=SORT(UNIQUE(A1:A5))` in C1 spilling over C1:C3, with its two results as values below it, and a reader of the area.
fn sheet() -> (Vec<OdsUnpacked>, SheetStyling) {
    let cells = vec![
        (0, 0, "b".to_string()),
        (1, 0, "a".to_string()),
        (2, 0, "b".to_string()),
        (3, 0, "c".to_string()),
        (0, 2, "=SORT(UNIQUE(A1:A4))".to_string()),
        (1, 2, "b".to_string()),
        (2, 2, "c".to_string()),
        (0, 4, "=SUM(C1#)".to_string()),
    ];
    let styling = SheetStyling {
        spills: vec![SpillRef {
            row: 0,
            col: 2,
            rows: 3,
            cols: 1,
        }],
        ..Default::default()
    };
    (vec![("Sheet1".to_string(), cells)], styling)
}

#[test]
fn xlsx_writes_a_dynamic_array_formula_and_reads_it_back_as_a_spill() {
    let (sheets, styling) = sheet();
    let mut cache = XlsxFormulaCache::new();
    cache.insert((0, 2), XlsxCachedValue::Text("a".to_string()));
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[cache], &[styling]).unwrap();
    let sheet_xml = part(&bytes, "xl/worksheets/sheet1.xml").unwrap();
    assert!(
        sheet_xml.contains(
            "cm=\"1\"><f t=\"array\" ref=\"C1:C3\">_xlfn._xlws.SORT(_xlfn.UNIQUE(A1:A4))</f>"
        ) || sheet_xml
            .contains("<f t=\"array\" ref=\"C1:C3\">_xlfn._xlws.SORT(_xlfn.UNIQUE(A1:A4))</f>"),
        "{sheet_xml}"
    );
    assert!(sheet_xml.contains("_xlfn.ANCHORARRAY(C1)"), "{sheet_xml}");
    let metadata = part(&bytes, "xl/metadata.xml").unwrap();
    assert!(metadata.contains("XLDAPR") && metadata.contains("fDynamic=\"1\""));
    assert!(part(&bytes, "[Content_Types].xml")
        .unwrap()
        .contains("sheetMetadata"));
    assert!(part(&bytes, "xl/_rels/workbook.xml.rels")
        .unwrap()
        .contains("sheetMetadata"));

    let (read, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    let cells = &read[0].1;
    // the two values under the anchor are results, not cells
    assert!(!cells
        .iter()
        .any(|(r, c, _)| (*r, *c) == (1, 2) || (*r, *c) == (2, 2)));
    assert!(cells.contains(&(0, 2, "=SORT(UNIQUE(A1:A4))".to_string())));
    assert!(cells.contains(&(0, 4, "=SUM(C1#)".to_string())));
    assert_eq!(
        stylings[0].1.spills,
        vec![SpillRef {
            row: 0,
            col: 2,
            rows: 3,
            cols: 1
        }]
    );
    // without styling the file reads as plain cells (what a conversion to another format sees)
    let plain = OoxmlCodec::unpack_xlsx_multi(&bytes).unwrap();
    assert!(plain[0].1.contains(&(1, 2, "b".to_string())));
}

#[test]
fn a_file_of_another_program_with_prefixed_names_reads_as_page_formulas() {
    let (sheets, styling) = sheet();
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling]).unwrap();
    // `LET` variables and `LAMBDA` parameters are written with `_xlpm.` and read without it
    let lambda = vec![(
        "Sheet1".to_string(),
        vec![(
            0,
            0,
            "=LET(x, 2, MAP(SEQUENCE(3), LAMBDA(n, n*x)))".to_string(),
        )],
    )];
    let packed =
        OoxmlCodec::pack_xlsx_multi_styled(&lambda, &[], &[SheetStyling::default()]).unwrap();
    let xml = part(&packed, "xl/worksheets/sheet1.xml").unwrap();
    assert!(
        xml.contains("_xlfn.LET(_xlpm.x, 2, _xlfn.MAP(_xlfn.SEQUENCE(3), _xlfn.LAMBDA(_xlpm.n, _xlpm.n*_xlpm.x)))"),
        "{xml}"
    );
    let (read, _) = OoxmlCodec::unpack_xlsx_with_styling(&packed).unwrap();
    assert_eq!(
        read[0].1[0].2,
        "=LET(x, 2, MAP(SEQUENCE(3), LAMBDA(n, n*x)))"
    );
    drop(bytes);
}

#[test]
fn ods_writes_a_matrix_formula_and_reads_it_back_as_a_spill() {
    let (sheets, styling) = sheet();
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &[styling]).unwrap();
    let content = part(&bytes, "content.xml").unwrap();
    assert!(
        content.contains(
            "table:number-matrix-columns-spanned=\"1\" table:number-matrix-rows-spanned=\"3\""
        ),
        "{content}"
    );
    let (read, stylings) = OdfCodec::unpack_ods_with_styling(&bytes).unwrap();
    let cells = &read[0].1;
    assert!(!cells
        .iter()
        .any(|(r, c, _)| (*r, *c) == (1, 2) || (*r, *c) == (2, 2)));
    assert_eq!(
        stylings[0].1.spills,
        vec![SpillRef {
            row: 0,
            col: 2,
            rows: 3,
            cols: 1
        }]
    );
    let _ = DocumentFormat::Ods;
}

#[test]
fn functions_that_came_after_the_first_file_format_are_written_with_the_future_prefix() {
    let formulas = [
        "=IFS(A1>1, \"a\", TRUE, \"b\")",
        "=STDEV.S(A1:A3)+IFNA(B1, 0)",
        "=TEXTJOIN(\",\", TRUE, A1:A3)&CONCAT(A1, B1)",
        "=SWITCH(A1, 1, \"x\", \"y\")+MAXIFS(A1:A3, B1:B3, 1)+XOR(TRUE, FALSE)",
        "=NORM.S.DIST(1, TRUE)+PERCENTILE.INC(A1:A3, 0.5)+DAYS(B1, A1)",
        "=STDEV(A1:A3)+SUM(A1:A3)+FDIST(1, 2, 3)",
    ];
    let cells: Vec<(u32, u32, String)> = formulas
        .iter()
        .enumerate()
        .map(|(i, f)| (i as u32, 3, (*f).to_string()))
        .collect();
    let sheets = vec![("Sheet1".to_string(), cells.clone())];
    let bytes =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[SheetStyling::default()]).unwrap();
    let xml = part(&bytes, "xl/worksheets/sheet1.xml").unwrap();
    for needle in [
        "_xlfn.IFS(",
        "_xlfn.STDEV.S(",
        "_xlfn.IFNA(",
        "_xlfn.TEXTJOIN(",
        "_xlfn.CONCAT(",
        "_xlfn.SWITCH(",
        "_xlfn.MAXIFS(",
        "_xlfn.XOR(",
        "_xlfn.NORM.S.DIST(",
        "_xlfn.PERCENTILE.INC(",
        "_xlfn.DAYS(",
        ">STDEV(A1:A3)+SUM(A1:A3)+FDIST(1, 2, 3)<",
    ] {
        assert!(xml.contains(needle), "{needle} in {xml}");
    }
    let (read, _) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    assert_eq!(read[0].1, cells);
}
