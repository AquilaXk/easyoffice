//! Font-derived automatic default row height round-trips in ODS without trailing 1M-row band bloat (PD-345).

use easyoffice_formats::{
    DefaultFont, EasyOfficeError, LegacyOle2Codec, LegacyXlsPayload, OdfCodec, OoxmlCodec,
    RowHeight, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, MAX_ROW_HEIGHTS,
};

const ODS_ROWS: u32 = 1_048_576;

fn height(points: &str, custom: bool) -> RowHeight {
    RowHeight::parse(points, custom).unwrap()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

fn replace_part(bytes: &[u8], path: &str, text: &str) -> Vec<u8> {
    let mut found = false;
    let mut parts: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                let data = if e.path == path {
                    found = true;
                    text.as_bytes().to_vec()
                } else {
                    e.data
                };
                (e.path, data)
            })
            .collect();
    if !found {
        parts.push((path.to_string(), text.as_bytes().to_vec()));
    }
    ZipArchiveCodec::pack_compressed(&parts).unwrap()
}

fn remove_part(bytes: &[u8], path: &str) -> Vec<u8> {
    let parts: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .filter(|e| e.path != path)
            .map(|e| (e.path, e.data))
            .collect();
    ZipArchiveCodec::pack_compressed(&parts).unwrap()
}

fn pack_one_ods(styling: &SheetStyling) -> Vec<u8> {
    let sheets = vec![("Data".to_string(), vec![(0, 0, "a".to_string())])];
    OdfCodec::pack_ods_multi_styled(&sheets, std::slice::from_ref(styling)).unwrap()
}

#[test]
fn font_default_row_height_round_trips_via_styles_xml_without_band() {
    let styling = SheetStyling {
        default_font: DefaultFont::new("Calibri", 11.0),
        default_row_height: Some(height("15", false)),
        row_heights: [(2, height("36", true))].into(),
        ..SheetStyling::default()
    };
    let bytes = pack_one_ods(&styling);
    let content = part(&bytes, "content.xml");
    let styles = part(&bytes, "styles.xml");

    assert!(!content.contains(&format!(r#"table:number-rows-repeated="{}""#, ODS_ROWS - 3)));
    assert!(!content.contains(r#"style:row-height="15pt""#));
    assert!(!content.contains("eo:default-row-height"));
    assert!(styles.contains(
        r#"<style:default-style style:family="table-row"><style:table-row-properties style:row-height="15pt" style:use-optimal-row-height="true"/></style:default-style>"#
    ));

    let via_with = OdfCodec::unpack_ods_with_styling(&bytes)
        .unwrap()
        .1
        .remove(0)
        .1;
    let via_only = OdfCodec::unpack_ods_styling(&bytes).unwrap().remove(0).1;
    for back in [&via_with, &via_only] {
        assert_eq!(back.default_row_height, Some(height("15", false)));
        assert_eq!(back.default_font, styling.default_font);
        assert_eq!(back.row_heights, styling.row_heights);
        assert!(back.user_default_row_height().is_none());
    }
}

#[test]
fn none_default_row_height_omits_table_row_default_style_and_round_trips() {
    for default_font in [None, DefaultFont::new("Aptos", 11.0)] {
        let styling = SheetStyling {
            default_font,
            default_row_height: None,
            ..SheetStyling::default()
        };
        let bytes = pack_one_ods(&styling);
        let styles = part(&bytes, "styles.xml");
        let content = part(&bytes, "content.xml");
        assert!(!styles.contains(r#"style:family="table-row""#));
        assert!(!content.contains("eo:default-row-height"));
        let back = OdfCodec::unpack_ods_styling(&bytes).unwrap().remove(0).1;
        assert_eq!(back.default_row_height, None);
        assert_eq!(back.default_font, styling.default_font);
    }
}

#[test]
fn xlsx_and_xls_through_ods_round_trips_preserve_font_row_height_and_custom_flag() {
    let just_stylings =
        |pairs: Vec<(String, SheetStyling)>| pairs.into_iter().map(|(_, s)| s).collect::<Vec<_>>();
    let xlsx_calibri = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "a".to_string())])],
        &[],
        &[SheetStyling {
            default_font: DefaultFont::new("Calibri", 11.0),
            default_row_height: Some(height("15", false)),
            ..SheetStyling::default()
        }],
    )
    .unwrap();
    let ods_bytes = OdfCodec::pack_ods_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "a".to_string())])],
        &just_stylings(OoxmlCodec::unpack_xlsx_styling(&xlsx_calibri).unwrap()),
    )
    .unwrap();
    let back_xlsx = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "a".to_string())])],
        &[],
        &just_stylings(OdfCodec::unpack_ods_styling(&ods_bytes).unwrap()),
    )
    .unwrap();
    let final_st = OoxmlCodec::unpack_xlsx_styling(&back_xlsx).unwrap();
    assert_eq!(final_st[0].1.default_row_height, Some(height("15", false)));
    assert_eq!(
        final_st[0].1.default_font,
        DefaultFont::new("Calibri", 11.0)
    );

    let xls_korean = LegacyOle2Codec::pack_xls_multi_values_styled(
        &[LegacyXlsPayload {
            sheet_name: "Sheet1".to_string(),
            cells: vec![(0, 0, "a".to_string())],
        }],
        &[],
        &[SheetStyling {
            default_font: DefaultFont::new("맑은 고딕", 10.0),
            default_row_height: Some(height("14.4", false)),
            ..SheetStyling::default()
        }],
    )
    .unwrap();
    let xls_read = just_stylings(
        LegacyOle2Codec::unpack_xls_with_styling(&xls_korean)
            .unwrap()
            .1,
    );
    assert_eq!(xls_read[0].default_row_height, Some(height("14.4", false)));
    let xls_ods = OdfCodec::pack_ods_multi_styled(
        &[("Sheet1".to_string(), vec![(0, 0, "a".to_string())])],
        &xls_read,
    )
    .unwrap();
    let xls_back = LegacyOle2Codec::pack_xls_multi_values_styled(
        &[LegacyXlsPayload {
            sheet_name: "Sheet1".to_string(),
            cells: vec![(0, 0, "a".to_string())],
        }],
        &[],
        &just_stylings(OdfCodec::unpack_ods_styling(&xls_ods).unwrap()),
    )
    .unwrap();
    let xls_final = LegacyOle2Codec::unpack_xls_with_styling(&xls_back)
        .unwrap()
        .1;
    assert_eq!(
        xls_final[0].1.default_row_height,
        Some(height("14.4", false))
    );
}

#[test]
fn multi_sheet_mixed_automatic_none_and_custom_defaults_round_trip() {
    let sheets = vec![
        ("S1".to_string(), vec![(0, 0, "a".to_string())]),
        ("S2".to_string(), vec![(0, 0, "b".to_string())]),
        ("S3".to_string(), vec![(0, 0, "c".to_string())]),
    ];
    let mixed = vec![
        SheetStyling {
            default_font: DefaultFont::new("Calibri", 11.0),
            default_row_height: Some(height("15", false)),
            ..SheetStyling::default()
        },
        SheetStyling {
            default_font: DefaultFont::new("Calibri", 11.0),
            default_row_height: Some(height("14.4", false)),
            ..SheetStyling::default()
        },
        SheetStyling {
            default_font: DefaultFont::new("Calibri", 11.0),
            default_row_height: Some(height("24", true)),
            ..SheetStyling::default()
        },
    ];
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &mixed).unwrap();
    let styles = part(&bytes, "styles.xml");
    let content = part(&bytes, "content.xml");
    assert!(styles.contains(r#"style:row-height="15pt" style:use-optimal-row-height="true""#));
    assert!(content.contains(r#"<table:table table:name="S2" eo:default-row-height="14.4pt">"#));
    assert!(content.contains(r#"style:row-height="24pt" style:use-optimal-row-height="false""#));

    let back = OdfCodec::unpack_ods_with_styling(&bytes).unwrap().1;
    assert_eq!(back[0].1.default_row_height, Some(height("15", false)));
    assert_eq!(back[1].1.default_row_height, Some(height("14.4", false)));
    assert_eq!(back[2].1.default_row_height, Some(height("24", true)));

    let with_none = vec![
        SheetStyling {
            default_font: DefaultFont::new("Calibri", 11.0),
            default_row_height: None,
            ..SheetStyling::default()
        },
        SheetStyling {
            default_font: DefaultFont::new("Calibri", 11.0),
            default_row_height: Some(height("14.4", false)),
            ..SheetStyling::default()
        },
        SheetStyling {
            default_font: DefaultFont::new("Calibri", 11.0),
            default_row_height: Some(height("24", true)),
            ..SheetStyling::default()
        },
    ];
    let bytes2 = OdfCodec::pack_ods_multi_styled(&sheets, &with_none).unwrap();
    assert!(!part(&bytes2, "styles.xml").contains(r#"style:family="table-row""#));
    let back2 = OdfCodec::unpack_ods_styling(&bytes2).unwrap();
    assert_eq!(back2[0].1.default_row_height, None);
    assert_eq!(back2[1].1.default_row_height, Some(height("14.4", false)));
    assert_eq!(back2[2].1.default_row_height, Some(height("24", true)));

    let majority = vec![
        SheetStyling {
            default_row_height: Some(height("14.4", false)),
            ..SheetStyling::default()
        },
        SheetStyling {
            default_row_height: Some(height("15", false)),
            ..SheetStyling::default()
        },
        SheetStyling {
            default_row_height: Some(height("15", false)),
            ..SheetStyling::default()
        },
    ];
    let bytes3 = OdfCodec::pack_ods_multi_styled(&sheets, &majority).unwrap();
    let content3 = part(&bytes3, "content.xml");
    assert!(part(&bytes3, "styles.xml").contains(r#"style:row-height="15pt""#));
    assert!(content3.contains(r#"<table:table table:name="S1" eo:default-row-height="14.4pt">"#));
    assert!(content3.contains(r#"<table:table table:name="S2">"#));
    let back3 = OdfCodec::unpack_ods_styling(&bytes3).unwrap();
    assert_eq!(back3[0].1.default_row_height, Some(height("14.4", false)));
    assert_eq!(back3[1].1.default_row_height, Some(height("15", false)));
    assert_eq!(back3[2].1.default_row_height, Some(height("15", false)));
}

#[test]
fn precedence_trailing_band_beats_eo_attr_beats_styles_xml_and_content_fallback() {
    let base = OdfCodec::pack_ods_multi_styled(
        &[
            ("BandAndEo".to_string(), vec![(0, 0, "a".to_string())]),
            ("WbOnly".to_string(), vec![(0, 0, "b".to_string())]),
            ("EoOnly".to_string(), vec![(0, 0, "c".to_string())]),
        ],
        &[
            SheetStyling {
                default_row_height: Some(height("24", true)),
                ..SheetStyling::default()
            },
            SheetStyling {
                default_row_height: Some(height("15", false)),
                ..SheetStyling::default()
            },
            SheetStyling {
                default_row_height: Some(height("18", false)),
                ..SheetStyling::default()
            },
        ],
    )
    .unwrap();
    let patched_content = part(&base, "content.xml").replace(
        r#"<table:table table:name="BandAndEo">"#,
        r#"<table:table table:name="BandAndEo" eo:default-row-height="12pt">"#,
    );
    let patched = replace_part(&base, "content.xml", &patched_content);
    let back = OdfCodec::unpack_ods_styling(&patched).unwrap();
    assert_eq!(back[0].1.default_row_height, Some(height("24", true)));
    assert_eq!(back[1].1.default_row_height, Some(height("15", false)));
    assert_eq!(back[2].1.default_row_height, Some(height("18", false)));

    let content_fallback = remove_part(&base, "styles.xml");
    let content_with_default = part(&content_fallback, "content.xml").replace(
        "<office:body>",
        "<office:styles><style:default-style style:family=\"table-row\"><style:table-row-properties style:row-height=\"14.4pt\" style:use-optimal-row-height=\"true\"/></style:default-style></office:styles><office:body>",
    );
    let fallback_ods = replace_part(&content_fallback, "content.xml", &content_with_default);
    let fallback_back = OdfCodec::unpack_ods_styling(&fallback_ods).unwrap();
    assert_eq!(
        fallback_back[1].1.default_row_height,
        Some(height("14.4", false))
    );

    let broken_styles = part(&base, "styles.xml")
        .replace(r#"style:row-height="15pt""#, r#"style:row-height="999pt""#);
    let no_fallback_on_seen_styles = replace_part(&fallback_ods, "styles.xml", &broken_styles);
    let no_fallback_back = OdfCodec::unpack_ods_styling(&no_fallback_on_seen_styles).unwrap();
    assert_eq!(no_fallback_back[1].1.default_row_height, None);
}

#[test]
fn external_ods_units_optimal_flags_and_standard_normalization() {
    let base = pack_one_ods(&SheetStyling {
        default_row_height: Some(height("15", false)),
        ..SheetStyling::default()
    });
    let styles = part(&base, "styles.xml");

    let custom_in = styles
        .replace(r#"style:row-height="15pt""#, r#"style:row-height="0.25in""#)
        .replace(
            r#"style:use-optimal-row-height="true""#,
            r#"style:use-optimal-row-height="false""#,
        );
    let back_in = OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &custom_in))
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back_in.default_row_height, Some(height("18", true)));
    assert_eq!(back_in.user_default_row_height(), Some(height("18", true)));

    let auto_mm = styles
        .replace(r#"style:row-height="15pt""#, r#"style:row-height="5mm""#)
        .replace(r#" style:use-optimal-row-height="true""#, "");
    let back_mm = OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &auto_mm))
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back_mm.default_row_height, Some(height("14.17", false)));
    assert!(back_mm.user_default_row_height().is_none());

    let standard_none =
        styles.replace(r#"style:row-height="15pt""#, r#"style:row-height="16.5pt""#);
    let back_std = OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &standard_none))
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back_std.default_row_height, None);
}

#[test]
fn max_row_heights_budget_stays_safe_over_110k_rows_with_font_default() {
    let styling = SheetStyling {
        default_font: DefaultFont::new("Calibri", 11.0),
        default_row_height: Some(height("15", false)),
        row_heights: [(110_000, height("40", true))].into(),
        ..SheetStyling::default()
    };
    let bytes = OdfCodec::pack_ods_multi_styled(
        &[("Sheet1".to_string(), vec![(110_000, 0, "x".to_string())])],
        std::slice::from_ref(&styling),
    )
    .unwrap();
    let content = part(&bytes, "content.xml");
    assert!(!content.contains(&format!(
        r#"table:number-rows-repeated="{}""#,
        ODS_ROWS - 110_001
    )));

    let mut row_runs = String::new();
    for _ in 0..110_000 {
        row_runs.push_str(
            r#"<table:table-row table:style-name="roauto"><table:table-cell/></table:table-row>"#,
        );
    }
    let patched_content = content
        .replace(
            "<office:automatic-styles>",
            r#"<office:automatic-styles><style:style style:name="roauto" style:family="table-row"><style:table-row-properties style:row-height="15pt" style:use-optimal-row-height="true"/></style:style>"#,
        )
        .replace(
            r#"<table:table-row table:number-rows-repeated="110000"/>"#,
            &row_runs,
        );
    let patched = replace_part(&bytes, "content.xml", &patched_content);
    let back = OdfCodec::unpack_ods_styling(&patched).unwrap().remove(0).1;
    assert_eq!(back.default_row_height, Some(height("15", false)));
    assert_eq!(back.row_heights, styling.row_heights);
    assert!(back.row_heights.len() < MAX_ROW_HEIGHTS);
}

#[test]
fn out_of_bounds_font_default_row_height_refused_on_write() {
    for bad_centipt in [0, 40_901] {
        let styling = SheetStyling {
            default_row_height: Some(RowHeight {
                centipt: bad_centipt,
                custom: false,
            }),
            ..SheetStyling::default()
        };
        let err = OdfCodec::pack_ods_multi_styled(
            &[("Sheet1".to_string(), vec![(0, 0, "a".to_string())])],
            &[styling],
        );
        assert!(err.is_err(), "centipt {bad_centipt} must fail");
    }
}

#[test]
fn hostile_untrusted_ods_xml_row_height_defaults_fail_closed() {
    let base = pack_one_ods(&SheetStyling {
        default_row_height: Some(height("15", false)),
        ..SheetStyling::default()
    });
    let styles = part(&base, "styles.xml");
    let content = part(&base, "content.xml");

    let exact_32 = "15.000000000000000000000000000pt";
    assert_eq!(exact_32.len(), 32);
    let ok_32 = OdfCodec::unpack_ods_styling(&replace_part(
        &base,
        "styles.xml",
        &styles.replace("15pt", exact_32),
    ))
    .unwrap()
    .remove(0)
    .1;
    assert_eq!(ok_32.default_row_height, Some(height("15", false)));

    let over_32 = "15.0000000000000000000000000000pt";
    assert_eq!(over_32.len(), 33);
    for bad_len in [
        "0pt",
        "-3mm",
        "409.01pt",
        "NaNpt",
        "infpt",
        "1e400pt",
        "15em",
        over_32,
        "15.&#x30;0000000000000000000000000000pt",
        &format!("{}pt", "&#x31;".repeat(33)),
        "15pt&bogus;",
        "15pt&#0;",
        "&#xD800;15pt",
    ] {
        let bad_styles = styles.replace("15pt", bad_len);
        let back = OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &bad_styles))
            .unwrap()
            .remove(0)
            .1;
        assert_eq!(back.default_row_height, None, "styles.xml bad={bad_len}");

        let base_none = remove_part(&base, "styles.xml");
        let bad_eo = content.replace(
            r#"<table:table table:name="Data">"#,
            &format!(r#"<table:table table:name="Data" eo:default-row-height="{bad_len}">"#),
        );
        let back_eo =
            OdfCodec::unpack_ods_styling(&replace_part(&base_none, "content.xml", &bad_eo))
                .unwrap()
                .remove(0)
                .1;
        assert_eq!(back_eo.default_row_height, None, "eo attr bad={bad_len}");
    }

    for bad_opt in ["yes", "1", "false&bogus;", "false&#0;"] {
        let bad_styles = styles.replace(
            r#"style:use-optimal-row-height="true""#,
            &format!(r#"style:use-optimal-row-height="{bad_opt}""#),
        );
        let back = OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &bad_styles))
            .unwrap()
            .remove(0)
            .1;
        assert_eq!(back.default_row_height, None, "bad optimal={bad_opt}");
    }

    let doctype_styles = styles.replace(
        "<office:document-styles",
        "<!DOCTYPE x [<!ENTITY e \"15pt\">]><office:document-styles",
    );
    assert!(matches!(
        OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &doctype_styles)),
        Err(EasyOfficeError::XxeOrExternalEntityDetected)
    ));

    let malformed_styles = styles.replace(r#"style:family="table-row""#, "style:family=table-row");
    assert!(
        OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &malformed_styles))
            .is_err()
    );

    let deep_open = "<a>".repeat(130);
    let deep_close = "</a>".repeat(130);
    let deep_styles = styles.replace("<office:styles>", &format!("<office:styles>{deep_open}"));
    let deep_styles =
        deep_styles.replace("</office:styles>", &format!("{deep_close}</office:styles>"));
    assert!(matches!(
        OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &deep_styles)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let base_none = remove_part(&base, "styles.xml");
    let deep_prebody = content.replace(
        "<office:body>",
        &format!("<office:styles>{deep_open}{deep_close}</office:styles><office:body>"),
    );
    assert!(matches!(
        OdfCodec::unpack_ods_styling(&replace_part(&base_none, "content.xml", &deep_prebody)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let deep_body_cell = content.replace(
        "<text:p>a</text:p>",
        &format!("<text:p>{deep_open}a{deep_close}</text:p>"),
    );
    assert!(OdfCodec::unpack_ods_styling(&replace_part(
        &base_none,
        "content.xml",
        &deep_body_cell
    ))
    .is_ok());

    let misplaced = styles.replace(
        r#"<style:default-style style:family="table-row"><style:table-row-properties style:row-height="15pt" style:use-optimal-row-height="true"/></style:default-style>"#,
        "",
    ).replace(
        "<office:automatic-styles>",
        r#"<office:automatic-styles><style:default-style style:family="table-row"><style:table-row-properties style:row-height="22pt" style:use-optimal-row-height="true"/></style:default-style>"#,
    );
    let back_misplaced =
        OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &misplaced))
            .unwrap()
            .remove(0)
            .1;
    assert_eq!(back_misplaced.default_row_height, None);

    let many_defaults = "<style:default-style style:family=\"paragraph\"/>".repeat(70);
    let capped = styles.replace(
        r#"<style:default-style style:family="table-row">"#,
        &format!(r#"{many_defaults}<style:default-style style:family="table-row">"#),
    );
    let back_capped = OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &capped))
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back_capped.default_row_height, None);

    // Nested default-style inside an untracked family (e.g. paragraph) and default-style outside <office:styles> must fail closed.
    let nested_in_other = styles
        .replace(
            r#"<style:default-style style:family="table-row"><style:table-row-properties style:row-height="15pt" style:use-optimal-row-height="true"/></style:default-style>"#,
            r#"<style:default-style style:family="paragraph"><style:default-style style:family="table-row"><style:table-row-properties style:row-height="22pt" style:use-optimal-row-height="true"/></style:default-style></style:default-style>"#,
        );
    let back_nested =
        OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &nested_in_other))
            .unwrap()
            .remove(0)
            .1;
    assert_eq!(back_nested.default_row_height, None);

    let outside_office_styles = styles
        .replace(
            r#"<style:default-style style:family="table-row"><style:table-row-properties style:row-height="15pt" style:use-optimal-row-height="true"/></style:default-style>"#,
            "",
        )
        .replace(
            "<office:styles>",
            r#"<office:font-face-decls><style:default-style style:family="table-row"><style:table-row-properties style:row-height="22pt" style:use-optimal-row-height="true"/></style:default-style></office:font-face-decls><office:styles>"#,
        );
    let back_outside =
        OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &outside_office_styles))
            .unwrap()
            .remove(0)
            .1;
    assert_eq!(back_outside.default_row_height, None);

    let wrapped_in_office_styles = styles.replace(
        r#"<style:default-style style:family="table-row"><style:table-row-properties style:row-height="15pt" style:use-optimal-row-height="true"/></style:default-style>"#,
        r#"<style:style style:name="wrap" style:family="table-row"><style:default-style style:family="table-row"><style:table-row-properties style:row-height="22pt" style:use-optimal-row-height="true"/></style:default-style></style:style>"#,
    );
    let back_wrapped = OdfCodec::unpack_ods_styling(&replace_part(
        &base,
        "styles.xml",
        &wrapped_in_office_styles,
    ))
    .unwrap()
    .remove(0)
    .1;
    assert_eq!(back_wrapped.default_row_height, None);

    // A stray closing tag inside <office:automatic-styles> must not break out of the ignored subtree.
    let stray_end_in_auto = styles
        .replace(
            r#"<style:default-style style:family="table-row"><style:table-row-properties style:row-height="15pt" style:use-optimal-row-height="true"/></style:default-style>"#,
            "",
        )
        .replace(
            "<office:automatic-styles>",
            r#"<office:automatic-styles></bogus><style:default-style style:family="table-row"><style:table-row-properties style:row-height="22pt" style:use-optimal-row-height="true"/></style:default-style>"#,
        );
    let back_stray =
        OdfCodec::unpack_ods_styling(&replace_part(&base, "styles.xml", &stray_end_in_auto))
            .unwrap()
            .remove(0)
            .1;
    assert_eq!(back_stray.default_row_height, None);
}

#[test]
fn partial_stylings_empty_sheets_boundary_heights_and_deep_auto_styles_behave_safely() {
    // When stylings.len() < sheets.len(), omitted trailing sheets default to None and must not inherit styles.xml height.
    let two_sheets = vec![
        ("S1".to_string(), vec![(0, 0, "a".to_string())]),
        ("S2".to_string(), vec![(0, 0, "b".to_string())]),
    ];
    let one_styling = vec![SheetStyling {
        default_font: DefaultFont::new("Calibri", 11.0),
        default_row_height: Some(height("15", false)),
        ..SheetStyling::default()
    }];
    let partial_ods = OdfCodec::pack_ods_multi_styled(&two_sheets, &one_styling).unwrap();
    assert!(!part(&partial_ods, "styles.xml").contains(r#"style:family="table-row""#));
    let partial_back = OdfCodec::unpack_ods_with_styling(&partial_ods).unwrap().1;
    assert_eq!(
        partial_back[0].1.default_row_height,
        Some(height("15", false))
    );
    assert_eq!(partial_back[1].1.default_row_height, None);

    // Empty-cell sheets and min/max point boundaries (0.01pt, 409pt) round-trip cleanly with custom == false.
    for edge in [height("0.01", false), height("409", false)] {
        let empty_sheets = vec![("Empty".to_string(), Vec::new())];
        let st = vec![SheetStyling {
            default_row_height: Some(edge),
            ..SheetStyling::default()
        }];
        let bytes = OdfCodec::pack_ods_multi_styled(&empty_sheets, &st).unwrap();
        let back = OdfCodec::unpack_ods_with_styling(&bytes).unwrap().1;
        assert_eq!(back[0].1.default_row_height, Some(edge));
    }

    // Empty <style:default-style style:family="table-cell"/> keeps Latin fallback Calibri 11pt.
    let empty_cell_default = r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.2"><office:styles><style:default-style style:family="table-cell"/></office:styles></office:document-styles>"#;
    let base_none = pack_one_ods(&SheetStyling::default());
    let empty_cell_ods = replace_part(&base_none, "styles.xml", empty_cell_default);
    let back_font = OdfCodec::unpack_ods_styling(&empty_cell_ods)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(back_font.default_font, DefaultFont::new("Calibri", 11.0));
    assert_eq!(back_font.default_row_height, None);

    // Deeply nested elements inside <office:automatic-styles> in content.xml do not fail MAX_STYLE_XML_DEPTH when styles.xml omits table-row.
    let deep_open = "<a>".repeat(140);
    let deep_close = "</a>".repeat(140);
    let content = part(&base_none, "content.xml").replace(
        "<office:body>",
        &format!("<office:automatic-styles>{deep_open}{deep_close}</office:automatic-styles><office:body>"),
    );
    let deep_auto_ods = replace_part(&base_none, "content.xml", &content);
    let back_deep_auto = OdfCodec::unpack_ods_styling(&deep_auto_ods).unwrap();
    assert_eq!(back_deep_auto[0].1.default_row_height, None);
}
