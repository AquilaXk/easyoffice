//! PD-330: Korean locale built-in numFmtId reading / custom writing, and legacy CSE array formula
//! subordinate cell preservation vs. dynamic array / `cm` spill dropping.
#![allow(clippy::unwrap_used)]

use easyoffice_formats::{
    builtin_code, is_known_builtin, read_sheet_styling, resolve_code, DocumentFormat, NumFmtTable,
    OoxmlCodec, SheetStyling, SpillRef, ZipArchiveCodec, ZipSecurityPolicy, CUSTOM_BASE,
};

fn replace_zip_entry(bytes: &[u8], path: &str, replacement: &str) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let mut replaced = false;
    let out: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == path {
                replaced = true;
                (e.path, replacement.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    assert!(replaced, "missing zip entry {path}");
    ZipArchiveCodec::pack(&out).unwrap()
}

fn minimal_xlsx_with_sheet_xml(sheet_xml: &str) -> Vec<u8> {
    let base = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), vec![])],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    replace_zip_entry(&base, "xl/worksheets/sheet1.xml", sheet_xml)
}

#[test]
fn korean_builtin_numfmt_ids_resolve_and_write_as_custom() {
    let expected: &[(u16, &str)] = &[
        (5, "₩#,##0;-₩#,##0"),
        (6, "₩#,##0;[Red]-₩#,##0"),
        (7, "₩#,##0.00;-₩#,##0.00"),
        (8, "₩#,##0.00;[Red]-₩#,##0.00"),
        (27, "yyyy\"년\" m\"월\""),
        (28, "m\"월\" d\"일\""),
        (29, "m\"월\" d\"일\""),
        (30, "m-d-yy"),
        (31, "yyyy\"년\" m\"월\" d\"일\""),
        (32, "h\"시\" mm\"분\""),
        (33, "h\"시\" mm\"분\" ss\"초\""),
        (34, "[$-412]AM/PM h\"시\" mm\"분\""),
        (35, "[$-412]AM/PM h\"시\" mm\"분\" ss\"초\""),
        (36, "yyyy\"년\" m\"월\""),
        (50, "yyyy\"년\" m\"월\""),
        (51, "m\"월\" d\"일\""),
        (52, "yyyy\"년\" m\"월\""),
        (53, "m\"월\" d\"일\""),
        (54, "m\"월\" d\"일\""),
        (55, "[$-412]AM/PM h\"시\" mm\"분\""),
        (56, "[$-412]AM/PM h\"시\" mm\"분\" ss\"초\""),
        (57, "yyyy\"년\" m\"월\""),
        (58, "m\"월\" d\"일\""),
    ];

    let mut table = NumFmtTable::default();
    for &(id, code) in expected {
        assert!(is_known_builtin(id), "built-in id {id} should be known");
        assert_eq!(builtin_code(id), Some(code), "id {id}");
        assert_eq!(resolve_code(id, &[]), Some(code), "resolve id {id}");
        let interned = table.intern(code);
        assert!(
            interned >= CUSTOM_BASE,
            "locale-dependent id {id} code {code:?} must write as custom id, got {interned}"
        );
    }

    // Round-trip via XLSX styles.xml using built-in id 31 without <numFmts> entry.
    let base = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "46096".to_string())])],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    let sheet_xml = r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" s="1"><v>46096</v></c></row></sheetData></worksheet>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Malgun Gothic"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="31" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs></styleSheet>"#;
    let xlsx = replace_zip_entry(
        &replace_zip_entry(&base, "xl/worksheets/sheet1.xml", sheet_xml),
        "xl/styles.xml",
        styles_xml,
    );
    let back = read_sheet_styling(&xlsx, DocumentFormat::Xlsx).unwrap();
    let st = &back[0].1;
    assert_eq!(st.num_fmts_rejected, 0);
    assert_eq!(
        resolve_code(st.cells[&(0, 0)].num_fmt, &st.num_fmts),
        Some("yyyy\"년\" m\"월\" d\"일\"")
    );
    let resaved = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "46096".to_string())])],
        &[],
        std::slice::from_ref(st),
    )
    .unwrap();
    let entries = ZipArchiveCodec::unpack(&resaved, &ZipSecurityPolicy::default()).unwrap();
    let resaved_styles = String::from_utf8(
        entries
            .iter()
            .find(|e| e.path == "xl/styles.xml")
            .unwrap()
            .data
            .clone(),
    )
    .unwrap();
    assert!(
        resaved_styles.contains("numFmtId=\"164\"")
            && resaved_styles
                .contains("formatCode=\"yyyy&quot;년&quot; m&quot;월&quot; d&quot;일&quot;\""),
        "resaved styles should emit custom formatCode for locale id 31: {resaved_styles}"
    );
}

#[test]
fn legacy_cse_scalar_broadcast_preserves_subordinate_cells() {
    let sheet_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>1</v></c>
      <c r="B1"><v>9</v></c>
      <c r="C1"><f t="array" ref="C1:C3">1+2</f><v>3</v></c>
      <c r="D1"><f t="array" ref="D1:D2">A1+B1</f><v>10</v></c>
      <c r="E1"><f t="array" ref="E1:E2">SUM(A1:A2)</f><v>5</v></c>
      <c r="F1"><f t="array" ref="F1:F2">IF(A1="SEQUENCE",1,2)</f><v>2</v></c>
      <c r="G1"><f t="array" ref="G1:G2">'SORT'!A1+1</f><v>2</v></c>
    </row>
    <row r="2">
      <c r="A2"><v>4</v></c>
      <c r="C2"><v>3</v></c>
      <c r="D2"><v>20</v></c>
      <c r="E2"><v>5</v></c>
      <c r="F2"><v>2</v></c>
      <c r="G2"><v>3</v></c>
    </row>
    <row r="3">
      <c r="C3"><v>3</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    let xlsx = minimal_xlsx_with_sheet_xml(sheet_xml);
    let (sheets, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let cells = &sheets[0].1;
    assert!(
        stylings[0].1.spills.is_empty(),
        "legacy scalar CSE without cm must not register spills: {:?}",
        stylings[0].1.spills
    );
    assert!(cells.contains(&(0, 2, "=1+2".to_string())));
    assert!(
        cells.contains(&(1, 2, "3".to_string())),
        "C2 lost: {cells:?}"
    );
    assert!(
        cells.contains(&(2, 2, "3".to_string())),
        "C3 lost: {cells:?}"
    );
    assert!(
        cells.contains(&(1, 3, "20".to_string())),
        "D2 lost: {cells:?}"
    );
    assert!(
        cells.contains(&(1, 4, "5".to_string())),
        "E2 lost: {cells:?}"
    );
    assert!(
        cells.contains(&(1, 5, "2".to_string())),
        "F2 lost: {cells:?}"
    );
    assert!(
        cells.contains(&(1, 6, "3".to_string())),
        "G2 lost: {cells:?}"
    );
}

#[test]
fn dynamic_array_and_cm_cells_still_spill_and_drop_subordinates() {
    let sheet_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" cm="1"><f t="array" ref="A1:A2">1+2</f><v>3</v></c>
      <c r="B1"><f t="array" ref="B1:B2">SEQUENCE(2)</f><v>1</v></c>
      <c r="C1"><f t="array" ref="C1:C2">TRANSPOSE(E1:F1)</f><v>10</v></c>
      <c r="D1"><f t="array" ref="D1:D2">_xlfn.ANCHORARRAY(B1)</f><v>1</v></c>
      <c r="E1"><v>10</v></c>
      <c r="F1"><v>20</v></c>
    </row>
    <row r="2">
      <c r="A2"><v>3</v></c>
      <c r="B2"><v>2</v></c>
      <c r="C2"><v>20</v></c>
      <c r="D2"><v>2</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    let xlsx = minimal_xlsx_with_sheet_xml(sheet_xml);
    let (sheets, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let cells = &sheets[0].1;
    assert_eq!(
        stylings[0].1.spills,
        vec![
            SpillRef {
                row: 0,
                col: 0,
                rows: 2,
                cols: 1
            },
            SpillRef {
                row: 0,
                col: 1,
                rows: 2,
                cols: 1
            },
            SpillRef {
                row: 0,
                col: 2,
                rows: 2,
                cols: 1
            },
            SpillRef {
                row: 0,
                col: 3,
                rows: 2,
                cols: 1
            },
        ]
    );
    for col in 0..4 {
        assert!(
            !cells.iter().any(|&(r, c, _)| r == 1 && c == col),
            "subordinate cell (1, {col}) should be dropped: {cells:?}"
        );
    }
}
