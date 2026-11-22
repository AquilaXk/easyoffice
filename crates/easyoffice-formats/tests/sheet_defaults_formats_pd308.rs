//! PD-308: Korean spreadsheet defaults (`맑은 고딕` 11 pt, `16.5 pt` / `22 px` rows), Aptos digit
//! widths, and XLSX · XLS · ODS format parity with fail-closed bounds.

use std::io::Cursor;

use easyoffice_formats::{
    default_col_px, default_row_centipt_for_font, max_digit_px, read_sheet_styling,
    DefaultColWidth, DefaultFont, DocumentFormat, EasyOfficeError, LegacyOle2Codec,
    LegacyXlsPayload, OdfCodec, OdsUnpacked, Ole2ContainerCodec, OoxmlCodec, RowHeight, SheetLinks,
    SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, CALIBRI_ROW_HEIGHT_CENTIPT, GRID_ROW_PX,
    STANDARD_ROW_HEIGHT_CENTIPT,
};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn sample_cells() -> Vec<OdsUnpacked> {
    vec![(
        "매출".to_string(),
        vec![(0, 0, "항목".to_string()), (1, 0, "100".to_string())],
    )]
}

fn zip_part(bytes: &[u8], path: &str) -> Option<String> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8(e.data).unwrap())
}

fn custom_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    let buf = Vec::new();
    let mut writer = ZipWriter::new(Cursor::new(buf));
    let opts = SimpleFileOptions::default();
    for (name, body) in entries {
        writer.start_file(*name, opts).unwrap();
        std::io::Write::write_all(&mut writer, body.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn biff8_record(opcode: u16, body: &[u8]) -> Vec<u8> {
    let mut rec = Vec::with_capacity(4 + body.len());
    rec.extend_from_slice(&opcode.to_le_bytes());
    rec.extend_from_slice(&(body.len() as u16).to_le_bytes());
    rec.extend_from_slice(body);
    rec
}

fn biff8_records(wb_bytes: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let streams = Ole2ContainerCodec::unpack(wb_bytes).unwrap();
    let wb = streams
        .into_iter()
        .find(|s| s.name == "Workbook" || s.name == "Book")
        .unwrap();
    let data = &wb.data;
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 4 <= data.len() {
        let op = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let len = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        out.push((op, data[pos..pos + len].to_vec()));
        pos += len;
    }
    out
}

#[test]
fn aptos_family_digit_widths_match_v440_hmtx_table() {
    let cases = [
        ("Aptos", 10.0, 7),
        ("Aptos", 11.0, 8),
        ("Aptos", 12.0, 9),
        ("Aptos Narrow", 10.0, 7),
        ("Aptos Narrow", 11.0, 7),
        ("Aptos Narrow", 12.0, 8),
        ("Aptos Display", 10.0, 7),
        ("Aptos Display", 11.0, 8),
        ("Aptos Display", 12.0, 8),
        ("Aptos Mono", 10.0, 8),
        ("Aptos Mono", 11.0, 9),
        ("Aptos Mono", 12.0, 10),
        ("Aptos Serif", 10.0, 7),
        ("Aptos Serif", 11.0, 8),
        ("Aptos Serif", 12.0, 9),
    ];
    for (face, size, expected_px) in cases {
        assert_eq!(
            max_digit_px(Some(face), Some(size)),
            expected_px,
            "{face} {size}pt"
        );
    }
    let aptos_11 = DefaultFont::new("Aptos", 11.0).unwrap();
    assert_eq!(aptos_11.digit_px(), 8);
    assert_eq!(default_col_px(None, aptos_11.digit_px()), 72);
    let narrow_11 = DefaultFont::new("Aptos Narrow", 11.0).unwrap();
    assert_eq!(narrow_11.digit_px(), 7);
    assert_eq!(default_col_px(None, narrow_11.digit_px()), 64);
}

#[test]
fn xlsx_standard_16_5pt_22px_calibri_15pt_and_custom_height_round_trip() {
    assert_eq!(GRID_ROW_PX, 22.0);
    assert_eq!(STANDARD_ROW_HEIGHT_CENTIPT, 1650);
    assert_eq!(CALIBRI_ROW_HEIGHT_CENTIPT, 1500);
    assert_eq!(default_row_centipt_for_font(None), 1650);

    // Unstyled multi-sheet XLSX always emits <sheetFormatPr defaultRowHeight="16.5"/> without customHeight.
    let unstyled = OoxmlCodec::pack_xlsx_multi(&sample_cells()).unwrap();
    let sheet1 = zip_part(&unstyled, "xl/worksheets/sheet1.xml").unwrap();
    assert!(
        sheet1.contains(r#"<sheetFormatPr defaultRowHeight="16.5"/>"#)
            && !sheet1.contains("customHeight"),
        "{sheet1}"
    );
    let read_back = read_sheet_styling(&unstyled, DocumentFormat::Xlsx).unwrap();
    assert_eq!(read_back[0].1.default_row_height, None);
    assert_eq!(read_back[0].1.shown_row_px(0, GRID_ROW_PX), 22.0);

    // Calibri 11 pt emits defaultRowHeight="15" and preserves it on read back.
    let calibri = SheetStyling {
        default_font: DefaultFont::new("Calibri", 11.0),
        ..SheetStyling::default()
    };
    let calibri_xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&sample_cells(), &[], &[calibri]).unwrap();
    let cal_sheet = zip_part(&calibri_xlsx, "xl/worksheets/sheet1.xml").unwrap();
    assert!(
        cal_sheet.contains(r#"<sheetFormatPr defaultRowHeight="15"/>"#)
            && !cal_sheet.contains("customHeight"),
        "{cal_sheet}"
    );
    let cal_back = read_sheet_styling(&calibri_xlsx, DocumentFormat::Xlsx).unwrap();
    assert_eq!(
        cal_back[0].1.default_row_height,
        RowHeight::from_points(15.0, false)
    );

    // Explicit customHeight="1" at 16.5 pt keeps Some(RowHeight { centipt: 1650, custom: true }).
    let custom_165 = SheetStyling {
        default_row_height: RowHeight::from_points(16.5, true),
        ..SheetStyling::default()
    };
    let custom_xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&sample_cells(), &[], &[custom_165]).unwrap();
    let custom_sheet = zip_part(&custom_xlsx, "xl/worksheets/sheet1.xml").unwrap();
    assert!(
        custom_sheet.contains(r#"<sheetFormatPr defaultRowHeight="16.5" customHeight="1"/>"#),
        "{custom_sheet}"
    );
    let custom_back = read_sheet_styling(&custom_xlsx, DocumentFormat::Xlsx).unwrap();
    assert_eq!(
        custom_back[0].1.default_row_height,
        RowHeight::from_points(16.5, true)
    );
}

#[test]
fn ods_default_style_16384_band_settling_and_xlsx_ods_roundtrip() {
    let plain_ods =
        OdfCodec::pack_ods_multi_styled(&sample_cells(), &[SheetStyling::default()]).unwrap();
    let styles_xml = zip_part(&plain_ods, "styles.xml").unwrap();
    let manifest_xml = zip_part(&plain_ods, "META-INF/manifest.xml").unwrap();
    assert!(manifest_xml.contains("styles.xml"), "{manifest_xml}");
    assert!(
        styles_xml.contains(
            r#"<style:default-style style:family="table-cell"><style:text-properties fo:font-family="'맑은 고딕'" style:font-family-asian="'맑은 고딕'" fo:font-size="11pt" style:font-size-asian="11pt"/></style:default-style>"#
        ),
        "{styles_xml}"
    );
    let read_plain = read_sheet_styling(&plain_ods, DocumentFormat::Ods).unwrap();
    assert_eq!(read_plain[0].1.default_font, None);

    // Synthetic external ODS with Liberation Sans 10 pt and 16,384 repeated column band (85 px).
    let ext_styles = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.2">
  <office:styles>
    <style:default-style style:family="table-cell">
      <style:text-properties fo:font-family="'Liberation Sans', sans-serif" fo:font-size="10pt"/>
    </style:default-style>
  </office:styles>
</office:document-styles>"#;
    let ext_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2">
  <office:automatic-styles>
    <style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="3.2cm"/></style:style>
    <style:style style:name="co2" style:family="table-column"><style:table-column-properties style:column-width="2.258cm"/></style:style>
  </office:automatic-styles>
  <office:body><office:spreadsheet>
    <table:table table:name="Ext">
      <table:table-column table:style-name="co1"/>
      <table:table-column table:style-name="co2" table:number-columns-repeated="16383"/>
      <table:table-row><table:table-cell><text:p>ok</text:p></table:table-cell></table:table-row>
    </table:table>
  </office:spreadsheet></office:body>
</office:document-content>"#;
    let ext_ods = custom_zip(&[
        ("mimetype", "application/vnd.oasis.opendocument.spreadsheet"),
        ("styles.xml", ext_styles),
        ("content.xml", ext_content),
    ]);
    let (_, ext_stylings) = OdfCodec::unpack_ods_with_styling(&ext_ods).unwrap();
    let ext = &ext_stylings[0].1;
    assert_eq!(ext.default_font, DefaultFont::new("Liberation Sans", 10.0));
    assert_eq!(ext.columns.len(), 1);
    assert_eq!(ext.columns.get(&0), Some(&121));
    assert_eq!(ext.default_col_px(), 85);

    // Single-XML content fallback + XLSX -> ODS -> XLSX parity.
    let mut xlsx_st = SheetStyling {
        default_font: DefaultFont::new("Aptos", 11.0),
        default_col_width: Some(DefaultColWidth {
            width: Some(12.5),
            base: Some(10),
        }),
        ..SheetStyling::default()
    };
    xlsx_st.columns.insert(2, 140);
    let xlsx =
        OoxmlCodec::pack_xlsx_multi_styled(&sample_cells(), &[], &[xlsx_st.clone()]).unwrap();
    let (sheets, mid_st) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let ods = OdfCodec::pack_ods_multi_styled(
        &sheets,
        &mid_st.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    let (ods_sheets, ods_st) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
    let xlsx2 = OoxmlCodec::pack_xlsx_multi_styled(
        &ods_sheets,
        &[],
        &ods_st.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    let (_, back_st) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx2).unwrap();
    assert_eq!(back_st[0].1.default_font, xlsx_st.default_font);
    assert_eq!(back_st[0].1.default_col_width, xlsx_st.default_col_width);
    assert_eq!(back_st[0].1.columns, xlsx_st.columns);
}

#[test]
fn ods_rejects_out_of_bounds_widths_and_handles_hostile_default_style() {
    let bad_col = SheetStyling {
        default_col_width: Some(DefaultColWidth {
            width: Some(300.0),
            base: None,
        }),
        ..SheetStyling::default()
    };
    assert!(matches!(
        OdfCodec::pack_ods_multi_styled(&sample_cells(), &[bad_col]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let bad_row = SheetStyling {
        row_heights: [(2_000_000, RowHeight::from_points(20.0, true).unwrap())].into(),
        ..SheetStyling::default()
    };
    assert!(matches!(
        OdfCodec::pack_ods_multi_styled(&sample_cells(), &[bad_row]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // Hostile attributes inside style:default-style fail closed cleanly + giant col repeat does not OOM.
    let hostile_styles = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.2">
  <office:styles>
    <style:default-style style:family="table-cell">
      <style:text-properties fo:font-family="Bad&lt;script&gt;" fo:font-size="99999pt"/>
    </style:default-style>
  </office:styles>
</office:document-styles>"#;
    let hostile_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2">
  <office:automatic-styles>
    <style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="2cm"/></style:style>
  </office:automatic-styles>
  <office:body><office:spreadsheet>
    <table:table table:name="Hostile">
      <table:table-column table:style-name="co1" table:number-columns-repeated="999999999"/>
      <table:table-row><table:table-cell><text:p>ok</text:p></table:table-cell></table:table-row>
    </table:table>
  </office:spreadsheet></office:body>
</office:document-content>"#;
    let hostile_ods = custom_zip(&[
        ("mimetype", "application/vnd.oasis.opendocument.spreadsheet"),
        ("styles.xml", hostile_styles),
        ("content.xml", hostile_content),
    ]);
    let (_, stylings) = OdfCodec::unpack_ods_with_styling(&hostile_ods).unwrap();
    assert_eq!(
        stylings[0].1.default_font,
        DefaultFont::new("Calibri", 11.0)
    );
    assert!(stylings[0].1.columns.is_empty());
}

#[test]
fn xls_biff8_korean_font_layout_records_and_hostile_bounds() {
    let payload = vec![LegacyXlsPayload {
        sheet_name: "시트1".to_string(),
        cells: vec![(0, 0, "100".to_string())],
    }];
    let plain_xls =
        LegacyOle2Codec::pack_xls_multi_values_styled(&payload, &[SheetLinks::default()], &[])
            .unwrap();
    let recs = biff8_records(&plain_xls);
    let font0 = recs.iter().find(|(op, _)| *op == 0x0031).unwrap();
    assert_eq!(u16::from_le_bytes([font0.1[0], font0.1[1]]), 220); // 11pt
    assert_eq!(font0.1[11], 2); // bFamily = Swiss
    assert_eq!(font0.1[12], 129); // bCharSet = HANGUL_CHARSET
    assert_eq!(font0.1[15], 1); // UTF-16LE flag
    let def_rh = recs.iter().find(|(op, _)| *op == 0x0225).unwrap();
    assert_eq!(u16::from_le_bytes([def_rh.1[0], def_rh.1[1]]), 0);
    assert_eq!(u16::from_le_bytes([def_rh.1[2], def_rh.1[3]]), 330); // 16.5pt
    let def_cw = recs.iter().find(|(op, _)| *op == 0x0055).unwrap();
    assert_eq!(u16::from_le_bytes([def_cw.1[0], def_cw.1[1]]), 8);

    let (_, plain_back) = LegacyOle2Codec::unpack_xls_with_styling(&plain_xls).unwrap();
    assert_eq!(plain_back[0].1.default_font, None);
    assert_eq!(plain_back[0].1.default_row_height, None);
    assert_eq!(plain_back[0].1.default_col_width, None);

    // Rich layout + non-standard font round-trip across XLS.
    let mut rich = SheetStyling {
        default_font: DefaultFont::new("Calibri", 11.0),
        default_row_height: RowHeight::from_points(22.5, true),
        default_col_width: Some(DefaultColWidth {
            width: Some(14.5),
            base: Some(10),
        }),
        hidden_rows: vec![(2, 3), (7, 7)],
        hidden_cols: vec![(0, 0), (2, 2)],
        ..SheetStyling::default()
    };
    rich.columns.insert(1, 120);
    rich.columns.insert(2, 120);
    rich.columns.insert(5, 160);
    // Row/col past BIFF8 grid bounds should skip cleanly on export without error.
    rich.columns.insert(300, 120);
    rich.row_heights
        .insert(0, RowHeight::from_points(30.0, true).unwrap());
    rich.row_heights
        .insert(7, RowHeight::from_points(40.0, true).unwrap());
    rich.row_heights
        .insert(70_000, RowHeight::from_points(25.0, true).unwrap());

    let rich_xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &payload,
        &[SheetLinks::default()],
        &[rich.clone()],
    )
    .unwrap();
    let (_, rich_back) = LegacyOle2Codec::unpack_xls_with_styling(&rich_xls).unwrap();
    let back = &rich_back[0].1;
    assert_eq!(back.default_font, rich.default_font);
    assert_eq!(back.default_row_height, rich.default_row_height);
    assert_eq!(back.default_col_width, rich.default_col_width);
    assert_eq!(back.columns.get(&0), None);
    assert_eq!(back.columns.get(&1), Some(&120));
    assert_eq!(back.columns.get(&2), Some(&120));
    assert_eq!(back.columns.get(&5), Some(&160));
    assert!(!back.columns.contains_key(&300));
    assert_eq!(back.hidden_cols, vec![(0, 0), (2, 2)]);
    assert_eq!(back.hidden_rows, vec![(2, 3), (7, 7)]);
    assert_eq!(
        back.row_heights.get(&0),
        Some(&RowHeight::from_points(30.0, true).unwrap())
    );
    assert_eq!(
        back.row_heights.get(&7),
        Some(&RowHeight::from_points(40.0, true).unwrap())
    );
    assert!(!back.row_heights.contains_key(&2));
    assert!(!back.row_heights.contains_key(&70_000));

    // Out-of-bounds styling fails closed on XLS write.
    let bad_col = SheetStyling {
        default_col_width: Some(DefaultColWidth {
            width: Some(300.0),
            base: None,
        }),
        ..SheetStyling::default()
    };
    assert!(matches!(
        LegacyOle2Codec::pack_xls_multi_values_styled(
            &payload,
            &[SheetLinks::default()],
            &[bad_col]
        ),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // Malformed / truncated BIFF8 layout records fail closed without panic.
    let mut wb = Vec::new();
    // Globals BOF + default FONT + XF + BOUNDSHEET + EOF
    let bof_g = [
        0x00, 0x06, 0x05, 0x00, 0xBB, 0x0D, 0xCC, 0x07, 0, 0, 0, 0, 6, 0, 0, 0,
    ];
    wb.extend(biff8_record(0x0809, &bof_g));
    wb.extend(biff8_record(0x0031, &[0, 0, 0, 0])); // truncated FONT
    wb.extend(biff8_record(0x00E0, &[0u8; 20]));
    let mut bs = vec![0u8; 8];
    let bs_off = wb.len() + 4;
    bs[6] = 1;
    bs.push(0);
    bs.push(b'S');
    wb.extend(biff8_record(0x0085, &bs));
    wb.extend(biff8_record(0x000A, &[]));
    let ws_pos = (wb.len() as u32).to_le_bytes();
    wb[bs_off..bs_off + 4].copy_from_slice(&ws_pos);

    let bof_ws = [
        0x00, 0x06, 0x10, 0x00, 0xBB, 0x0D, 0xCC, 0x07, 0, 0, 0, 0, 6, 0, 0, 0,
    ];
    wb.extend(biff8_record(0x0809, &bof_ws));
    wb.extend(biff8_record(0x0225, &[0x01, 0x00])); // truncated DEFAULTROWHEIGHT
    wb.extend(biff8_record(0x0055, &[44, 1])); // DEFCOLWIDTH = 300 > 255
    wb.extend(biff8_record(0x0099, &[0, 0])); // STANDARDWIDTH = 0
    wb.extend(biff8_record(0x007D, &[5, 0, 1, 0, 10, 0, 0, 0, 0, 0, 0, 0])); // first > last COLINFO
    wb.extend(biff8_record(
        0x007D,
        &[0, 0, 44, 1, 10, 0, 0, 0, 0, 0, 0, 0],
    )); // last = 300 > 255
    wb.extend(biff8_record(0x0208, &[0u8; 10])); // truncated ROW
    wb.extend(biff8_record(0x000A, &[]));

    let hostile_xls = Ole2ContainerCodec::pack(&[("Workbook".to_string(), wb)]).unwrap();
    let (_, h_back) = LegacyOle2Codec::unpack_xls_with_styling(&hostile_xls).unwrap();
    let hb = &h_back[0].1;
    assert_eq!(hb.default_row_height, None);
    assert_eq!(hb.default_col_width, None);
    assert!(hb.columns.is_empty());
    assert!(hb.row_heights.is_empty());
    assert!(hb.hidden_rows.is_empty());
}
