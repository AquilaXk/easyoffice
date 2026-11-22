//! PD-309: Workbook calculation options (`CalcMode`, `CalcOptions`) round-trip across XLSX, ODS,
//! and BIFF8 XLS, cross-format conversion parity, and fail-closed hostile input bounds.

use std::io::Cursor;

use easyoffice_formats::{
    read_sheet_styling, CalcMode, CalcOptions, DocumentFormat, EasyOfficeError, LegacyOle2Codec,
    LegacyXlsPayload, OdfCodec, OdsUnpacked, Ole2ContainerCodec, OoxmlCodec, SheetLinks,
    SheetStyling, ZipArchiveCodec, ZipSecurityPolicy,
};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn sample_cells() -> Vec<OdsUnpacked> {
    vec![
        (
            "Sheet1".to_string(),
            vec![(0, 0, "10".to_string()), (1, 0, "=A1*2".to_string())],
        ),
        (
            "Sheet2".to_string(),
            vec![(0, 0, "=Sheet1!A1+1".to_string())],
        ),
    ]
}

fn zip_part(bytes: &[u8], path: &str) -> Option<String> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8(e.data).unwrap())
}

fn custom_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
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
fn default_calc_options_emit_standard_xlsx_ods_and_biff8_records() {
    let xlsx = OoxmlCodec::pack_xlsx_multi(&sample_cells()).unwrap();
    let wb_xml = zip_part(&xlsx, "xl/workbook.xml").unwrap();
    assert!(
        wb_xml.contains(r#"<calcPr calcId="191029" fullCalcOnLoad="1"/>"#),
        "{wb_xml}"
    );
    let read_xlsx = read_sheet_styling(&xlsx, DocumentFormat::Xlsx).unwrap();
    assert_eq!(read_xlsx[0].1.calc_options, CalcOptions::default());

    let ods = OdfCodec::pack_ods_multi(&sample_cells()).unwrap();
    assert!(zip_part(&ods, "settings.xml").is_none());
    let content_xml = zip_part(&ods, "content.xml").unwrap();
    assert!(!content_xml.contains("<table:iteration"), "{content_xml}");
    let read_ods = read_sheet_styling(&ods, DocumentFormat::Ods).unwrap();
    assert_eq!(read_ods[0].1.calc_options, CalcOptions::default());

    let payloads: Vec<LegacyXlsPayload> = sample_cells()
        .into_iter()
        .map(|(sheet_name, cells)| LegacyXlsPayload { sheet_name, cells })
        .collect();
    let xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &payloads,
        &[SheetLinks::default(), SheetLinks::default()],
        &[],
    )
    .unwrap();
    let recs = biff8_records(&xls);
    assert_eq!(
        recs.iter().find(|(op, _)| *op == 0x000D).unwrap().1,
        1i16.to_le_bytes()
    );
    assert_eq!(
        recs.iter().find(|(op, _)| *op == 0x000C).unwrap().1,
        100u16.to_le_bytes()
    );
    assert_eq!(
        recs.iter().find(|(op, _)| *op == 0x0010).unwrap().1,
        0.001f64.to_le_bytes()
    );
    assert_eq!(
        recs.iter().find(|(op, _)| *op == 0x0011).unwrap().1,
        0u16.to_le_bytes()
    );
    assert_eq!(
        recs.iter().find(|(op, _)| *op == 0x005F).unwrap().1,
        1u16.to_le_bytes()
    );
    let read_xls = read_sheet_styling(&xls, DocumentFormat::Xls).unwrap();
    assert_eq!(read_xls[0].1.calc_options, CalcOptions::default());
}

#[test]
fn manual_auto_no_table_and_iteration_round_trip_and_cross_convert() {
    let manual_iter = CalcOptions {
        mode: CalcMode::Manual,
        calc_on_save: false,
        full_calc_on_load: true,
        iterate: true,
        iterate_count: 50,
        iterate_delta: 0.0001,
    };
    let stylings = vec![
        SheetStyling {
            calc_options: manual_iter,
            ..SheetStyling::default()
        },
        SheetStyling::default(),
    ];
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sample_cells(), &[], &stylings).unwrap();
    let wb_xml = zip_part(&xlsx, "xl/workbook.xml").unwrap();
    assert!(wb_xml.contains(r#"calcMode="manual""#), "{wb_xml}");
    assert!(wb_xml.contains(r#"fullCalcOnLoad="1""#), "{wb_xml}");
    assert!(wb_xml.contains(r#"calcOnSave="0""#), "{wb_xml}");
    assert!(wb_xml.contains(r#"iterate="1""#), "{wb_xml}");
    assert!(wb_xml.contains(r#"iterateCount="50""#), "{wb_xml}");
    assert!(wb_xml.contains(r#"iterateDelta="0.0001""#), "{wb_xml}");

    // Manual with full_calc_on_load=false omits fullCalcOnLoad on XLSX.
    let manual_no_load = CalcOptions {
        mode: CalcMode::Manual,
        calc_on_save: false,
        full_calc_on_load: false,
        iterate: false,
        iterate_count: 100,
        iterate_delta: 0.001,
    };
    let xlsx_no_load = OoxmlCodec::pack_xlsx_multi_styled(
        &sample_cells(),
        &[],
        &[SheetStyling {
            calc_options: manual_no_load,
            ..SheetStyling::default()
        }],
    )
    .unwrap();
    let wb_no_load = zip_part(&xlsx_no_load, "xl/workbook.xml").unwrap();
    assert!(!wb_no_load.contains("fullCalcOnLoad"), "{wb_no_load}");
    assert_eq!(
        read_sheet_styling(&xlsx_no_load, DocumentFormat::Xlsx).unwrap()[0]
            .1
            .calc_options,
        manual_no_load
    );

    // Cross-format round-trip: XLSX -> ODS -> XLS -> XLSX.
    let (sheets_x, st_x) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    assert_eq!(st_x[0].1.calc_options, manual_iter);

    let ods = OdfCodec::pack_ods_multi_styled(
        &sheets_x,
        &st_x.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    let ods_content = zip_part(&ods, "content.xml").unwrap();
    let ods_settings = zip_part(&ods, "settings.xml").unwrap();
    let ods_manifest = zip_part(&ods, "META-INF/manifest.xml").unwrap();
    assert!(ods_manifest.contains("settings.xml"), "{ods_manifest}");
    assert!(ods_content.contains("<table:iteration"), "{ods_content}");
    assert!(ods_content.contains(r#"table:steps="50""#), "{ods_content}");
    assert!(
        ods_settings.contains(r#"config:name="AutoCalculate""#)
            && ods_settings.contains(r#"config:name="EoCalcMode""#),
        "{ods_settings}"
    );
    let (sheets_o, st_o) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
    assert_eq!(st_o[0].1.calc_options, manual_iter);

    let xls_payloads: Vec<LegacyXlsPayload> = sheets_o
        .iter()
        .map(|(n, c)| LegacyXlsPayload {
            sheet_name: n.clone(),
            cells: c.clone(),
        })
        .collect();
    let xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &xls_payloads,
        &[SheetLinks::default(), SheetLinks::default()],
        &st_o.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    let (sheets_l, st_l) = LegacyOle2Codec::unpack_xls_with_styling(&xls).unwrap();
    // BIFF8 has no `fullCalcOnLoad` record, so `full_calc_on_load` defaults to `false` across XLS.
    let expected_xls = CalcOptions {
        full_calc_on_load: false,
        ..manual_iter
    };
    assert_eq!(st_l[0].1.calc_options, expected_xls);

    // Also verify AutoNoTable round-trip across XLSX, ODS, and XLS.
    let auto_no_table = CalcOptions {
        mode: CalcMode::AutoNoTable,
        calc_on_save: true,
        full_calc_on_load: false,
        iterate: false,
        iterate_count: 100,
        iterate_delta: 0.001,
    };
    let ant_st = [SheetStyling {
        calc_options: auto_no_table,
        ..SheetStyling::default()
    }];
    let ant_xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets_l[..1], &[], &ant_st).unwrap();
    let ant_ods = OdfCodec::pack_ods_multi_styled(&sheets_l[..1], &ant_st).unwrap();
    let ant_xls = LegacyOle2Codec::pack_xls_multi_values_styled(
        &xls_payloads[..1],
        &[SheetLinks::default()],
        &ant_st,
    )
    .unwrap();
    for (bytes, fmt) in [
        (&ant_xlsx, DocumentFormat::Xlsx),
        (&ant_ods, DocumentFormat::Ods),
        (&ant_xls, DocumentFormat::Xls),
    ] {
        assert_eq!(
            read_sheet_styling(bytes, fmt).unwrap()[0].1.calc_options,
            auto_no_table,
            "{fmt:?}"
        );
    }
}

#[test]
fn out_of_bounds_calc_options_fail_closed_and_malformed_inputs_are_safely_ignored() {
    let invalid_cases = [
        CalcOptions {
            iterate_count: 0,
            ..CalcOptions::default()
        },
        CalcOptions {
            iterate_count: 32_768,
            ..CalcOptions::default()
        },
        CalcOptions {
            iterate_delta: -0.1,
            ..CalcOptions::default()
        },
        CalcOptions {
            iterate_delta: f64::NAN,
            ..CalcOptions::default()
        },
        CalcOptions {
            iterate_delta: 1e101,
            ..CalcOptions::default()
        },
    ];
    let payloads = vec![LegacyXlsPayload {
        sheet_name: "Sheet1".to_string(),
        cells: vec![(0, 0, "1".to_string())],
    }];
    for opts in invalid_cases {
        let st = SheetStyling {
            calc_options: opts,
            ..SheetStyling::default()
        };
        let slice = std::slice::from_ref(&st);
        assert!(OoxmlCodec::pack_xlsx_multi_styled(&sample_cells()[..1], &[], slice).is_err());
        assert!(OdfCodec::pack_ods_multi_styled(&sample_cells()[..1], slice).is_err());
        assert!(LegacyOle2Codec::pack_xls_multi_values_styled(
            &payloads,
            &[SheetLinks::default()],
            slice,
        )
        .is_err());
    }

    // Hostile ODS settings/iteration attributes clamp or fall back without panicking.
    let hostile_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2">
  <office:body><office:spreadsheet>
    <table:calculation-settings>
      <table:iteration table:status="enable" table:steps="9999999" table:maximum-difference="NaN"/>
    </table:calculation-settings>
    <table:table table:name="S1"><table:table-row><table:table-cell><text:p>1</text:p></table:table-cell></table:table-row></table:table>
  </office:spreadsheet></office:body>
</office:document-content>"#;
    let hostile_settings = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-settings xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0" office:version="1.2">
  <office:settings>
    <config:config-item-set config:name="ooo:configuration-settings">
      <config:config-item config:name="AutoCalculate" config:type="boolean">bogus</config:config-item>
      <config:config-item config:name="EoCalcMode" config:type="string">unknown</config:config-item>
    </config:config-item-set>
  </office:settings>
</office:document-settings>"#;
    let hostile_ods = custom_zip(&[
        ("mimetype", "application/vnd.oasis.opendocument.spreadsheet"),
        ("content.xml", hostile_content),
        ("settings.xml", hostile_settings),
    ]);
    let (_, ods_st) = OdfCodec::unpack_ods_with_styling(&hostile_ods).unwrap();
    assert_eq!(ods_st[0].1.calc_options.mode, CalcMode::Auto);
    assert!(ods_st[0].1.calc_options.within_bounds());

    // Malformed BIFF8 calculation records in Worksheet substream are ignored cleanly.
    let mut wb = Vec::new();
    let bof_g = [
        0x00, 0x06, 0x05, 0x00, 0xBB, 0x0D, 0xCC, 0x07, 0, 0, 0, 0, 6, 0, 0, 0,
    ];
    wb.extend(biff8_record(0x0809, &bof_g));
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
    wb.extend(biff8_record(0x000D, &[0])); // truncated CALCMODE
    wb.extend(biff8_record(0x000C, &60_000u16.to_le_bytes())); // out of range CALCCOUNT > 32767
    wb.extend(biff8_record(0x0010, &f64::NAN.to_le_bytes())); // NaN DELTA
    wb.extend(biff8_record(0x000A, &[]));
    let hostile_xls = Ole2ContainerCodec::pack(&[("Workbook".to_string(), wb)]).unwrap();
    let (_, xls_st) = LegacyOle2Codec::unpack_xls_with_styling(&hostile_xls).unwrap();
    assert_eq!(xls_st[0].1.calc_options, CalcOptions::default());
    let _ = EasyOfficeError::InvalidArgument;
}
