//! Cell number formats survive XLSX/ODS save -> reopen (PD-193).

use std::sync::Arc;

use easyoffice_formats::{
    read_sheet_styling, resolve_code, CellStyle, DocumentFormat, OdfCodec, OdsUnpacked, OoxmlCodec,
    SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, CUSTOM_BASE, MAX_NUM_FMTS,
};

const BOTH: [DocumentFormat; 2] = [DocumentFormat::Xlsx, DocumentFormat::Ods];

fn sheet(cells: &[(u32, u32, &str)]) -> OdsUnpacked {
    (
        "Sheet1".to_string(),
        cells
            .iter()
            .map(|&(r, c, t)| (r, c, t.to_string()))
            .collect(),
    )
}

/// A styling whose cells use `codes[i]` (custom table entry `i`, or a built-in id when equal).
fn styling_with(cells: &[((u32, u32), u16)], custom: &[&str]) -> SheetStyling {
    SheetStyling {
        cells: cells
            .iter()
            .map(|&(at, num_fmt)| {
                (
                    at,
                    CellStyle {
                        num_fmt,
                        ..CellStyle::default()
                    },
                )
            })
            .collect(),
        num_fmts: Arc::new(custom.iter().map(|c| c.to_string()).collect()),
        ..SheetStyling::default()
    }
}

fn pack(format: DocumentFormat, sheets: &[OdsUnpacked], styling: &[SheetStyling]) -> Vec<u8> {
    match format {
        DocumentFormat::Xlsx => OoxmlCodec::pack_xlsx_multi_styled(sheets, &[], styling).unwrap(),
        _ => OdfCodec::pack_ods_multi_styled(sheets, styling).unwrap(),
    }
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

fn code_at(styling: &SheetStyling, at: (u32, u32)) -> Option<String> {
    let style = styling.cells.get(&at)?;
    resolve_code(style.num_fmt, &styling.num_fmts).map(str::to_string)
}

#[test]
fn custom_and_builtin_codes_round_trip_in_xlsx_and_ods() {
    let codes = [
        "\"₩\"#,##0_);[Red](\"₩\"#,##0)",
        "yyyy\"년\" m\"월\" d\"일\"",
        "0.00",
        "0.0%",
        "@",
        "# ?/?",
    ];
    for format in BOTH {
        let sheets = [sheet(&[(0, 0, "1234.5"), (1, 0, "0.25"), (2, 0, "x")])];
        // Cell ids: custom entries 0 and 1 and so on; the built-in ones read as built-in ids.
        let st = styling_with(
            &[
                ((0, 0), CUSTOM_BASE),
                ((0, 1), CUSTOM_BASE + 1),
                ((0, 2), CUSTOM_BASE + 2),
                ((0, 3), CUSTOM_BASE + 3),
                ((0, 4), CUSTOM_BASE + 4),
                ((0, 5), CUSTOM_BASE + 5),
            ],
            &codes,
        );
        let bytes = pack(format, &sheets, &[st]);
        let back = read_sheet_styling(&bytes, format).unwrap();
        let styling = &back[0].1;
        for (col, code) in codes.iter().enumerate() {
            assert_eq!(
                code_at(styling, (0, col as u32)).as_deref(),
                Some(*code),
                "{format:?} column {col}"
            );
        }
    }
}

#[test]
fn xlsx_writes_builtin_ids_without_a_numfmt_entry() {
    // "0.00" is built-in id 2; no `numFmt` element is needed for it.
    let st = styling_with(&[((0, 0), CUSTOM_BASE)], &["0.00"]);
    let bytes = pack(DocumentFormat::Xlsx, &[sheet(&[(0, 0, "3")])], &[st]);
    let styles = part(&bytes, "xl/styles.xml");
    assert!(!styles.contains("<numFmts"), "{styles}");
    assert!(styles.contains("numFmtId=\"2\""), "{styles}");
    assert!(styles.contains("applyNumberFormat=\"1\""));
    let back = read_sheet_styling(&bytes, DocumentFormat::Xlsx).unwrap();
    assert_eq!(back[0].1.cells[&(0, 0)].num_fmt, 2);
}

#[test]
fn xlsx_numfmts_precede_fonts_and_escape_quotes() {
    let st = styling_with(&[((0, 0), CUSTOM_BASE)], &["0.0\"x\""]);
    let bytes = pack(DocumentFormat::Xlsx, &[sheet(&[(0, 0, "3")])], &[st]);
    let styles = part(&bytes, "xl/styles.xml");
    let numfmts = styles.find("<numFmts count=\"1\">").unwrap();
    assert!(numfmts < styles.find("<fonts").unwrap());
    assert!(styles.contains("numFmtId=\"164\" formatCode=\"0.0&quot;x&quot;\""));
}

#[test]
fn a_font_look_and_a_number_format_combine() {
    let mut st = styling_with(&[((0, 0), CUSTOM_BASE)], &["0.00"]);
    st.cells.get_mut(&(0, 0)).unwrap().bold = true;
    st.cells.insert(
        (0, 1),
        CellStyle {
            bold: true,
            ..CellStyle::default()
        },
    );
    for format in BOTH {
        let back = read_sheet_styling(
            &pack(format, &[sheet(&[(0, 0, "1"), (0, 1, "2")])], &[st.clone()]),
            format,
        )
        .unwrap();
        let styling = &back[0].1;
        assert!(styling.cells[&(0, 0)].bold && styling.cells[&(0, 1)].bold);
        assert_eq!(
            code_at(styling, (0, 0)).as_deref(),
            Some("0.00"),
            "{format:?}"
        );
        assert_eq!(styling.cells[&(0, 1)].num_fmt, 0);
    }
}

#[test]
fn sheets_share_codes_through_the_workbook_table() {
    let a = styling_with(&[((0, 0), CUSTOM_BASE)], &["0.0\"a\""]);
    let b = styling_with(&[((0, 0), CUSTOM_BASE)], &["0.0\"b\""]);
    let sheets = [
        ("A".to_string(), vec![(0, 0, "1".to_string())]),
        ("B".to_string(), vec![(0, 0, "2".to_string())]),
    ];
    for format in BOTH {
        let back =
            read_sheet_styling(&pack(format, &sheets, &[a.clone(), b.clone()]), format).unwrap();
        assert_eq!(
            code_at(&back[0].1, (0, 0)).as_deref(),
            Some("0.0\"a\""),
            "{format:?}"
        );
        assert_eq!(
            code_at(&back[1].1, (0, 0)).as_deref(),
            Some("0.0\"b\""),
            "{format:?}"
        );
    }
}

fn xlsx_with_styles(styles: &str) -> Vec<u8> {
    let base = OoxmlCodec::pack_xlsx_multi_styled(
        &[sheet(&[(0, 0, "1")])],
        &[],
        &[styling_with(&[((0, 0), 2)], &[])],
    )
    .unwrap();
    let entries = ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default()).unwrap();
    let swapped: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "xl/styles.xml" {
                (e.path, styles.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&swapped).unwrap()
}

fn style_sheet(num_fmts: &str, xfs: &str) -> String {
    format!(
        "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{num_fmts}<fonts count=\"1\"><font><sz val=\"11\"/></font></fonts><cellXfs count=\"3\">{xfs}</cellXfs></styleSheet>"
    )
}

#[test]
fn hostile_codes_read_as_general_and_are_counted() {
    let long = "0".repeat(300);
    let numfmts = format!(
        "<numFmts count=\"3\"><numFmt numFmtId=\"164\" formatCode=\"{long}\"/><numFmt numFmtId=\"165\" formatCode=\"0;0;0;0;0\"/><numFmt numFmtId=\"166\" formatCode=\"0.0&quot;ok&quot;\"/></numFmts>"
    );
    let xfs = concat!(
        "<xf numFmtId=\"164\" fontId=\"0\"/><xf numFmtId=\"165\" fontId=\"0\"/><xf numFmtId=\"166\" fontId=\"0\"/>",
        "<xf numFmtId=\"27\" fontId=\"0\"/><xf numFmtId=\"999\" fontId=\"0\"/>"
    );
    let bytes = xlsx_with_styles(&style_sheet(&numfmts, xfs));
    let styling = &read_sheet_styling(&bytes, DocumentFormat::Xlsx).unwrap()[0].1;
    // Cell A1 uses xf 0 of the file written above (swapped styles keep the cell's `s` of 1).
    assert!(
        styling.num_fmts_rejected >= 4,
        "{}",
        styling.num_fmts_rejected
    );
    assert_eq!(styling.num_fmts.as_slice(), ["0.0\"ok\""]);
}

#[test]
fn the_custom_table_is_bounded_when_writing() {
    let codes: Vec<String> = (0..MAX_NUM_FMTS + 5)
        .map(|n| format!("0.0\"{n}\""))
        .collect();
    let refs: Vec<&str> = codes.iter().map(String::as_str).collect();
    let cells: Vec<((u32, u32), u16)> = (0..codes.len())
        .map(|n| ((n as u32, 0), CUSTOM_BASE + n as u16))
        .collect();
    // The model table itself is not bounded here; the writer's workbook table is.
    let st = styling_with(&cells, &refs);
    let sheets = [sheet(&[(0, 0, "1")])];
    let bytes = pack(DocumentFormat::Xlsx, &sheets, &[st]);
    let styles = part(&bytes, "xl/styles.xml");
    assert_eq!(styles.matches("<numFmt ").count(), MAX_NUM_FMTS);
}
