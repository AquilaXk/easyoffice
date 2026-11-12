//! Conditional formatting and data validation survive XLSX writing and reading (PD-171).

use easyoffice_formats::{
    CellBox, CfKind, CfRule, CfStyle, CfValue, Cmp, DataValidation, DvAlert, DvKind, OdfCodec,
    OoxmlCodec, SheetRules, SheetStyling, TextOp, TimePeriod, ValueKind, FLAG_BOLD, FLAG_ITALIC,
};

fn boxes(text: &str) -> Vec<CellBox> {
    easyoffice_formats::parse_sqref(text)
}

fn red() -> CfStyle {
    CfStyle {
        bg: Some(0xFFC7CE),
        fg: Some(0x9C0006),
        flags: FLAG_BOLD | FLAG_ITALIC,
    }
}

fn rule(ranges: &str, priority: u32, kind: CfKind) -> CfRule {
    let style = if matches!(
        kind,
        CfKind::ColorScale(_) | CfKind::DataBar { .. } | CfKind::IconSet { .. }
    ) {
        CfStyle::default()
    } else {
        red()
    };
    CfRule {
        ranges: boxes(ranges),
        priority,
        stop_if_true: priority % 2 == 0,
        kind,
        style,
    }
}

fn sample_rules() -> SheetRules {
    let mut dv = DataValidation::new(boxes("B2:B20"), DvKind::Whole);
    dv.a = "1".into();
    dv.b = "10".into();
    dv.alert = DvAlert::Warning;
    dv.error_title = "범위".into();
    dv.error = "1~10 사이 정수".into();
    dv.show_prompt = true;
    dv.prompt = "정수 입력".into();
    let mut list = DataValidation::new(boxes("C2:C9"), DvKind::List);
    list.a = "사과,배,\"감\"".into();
    list.show_dropdown = false;
    list.allow_blank = false;
    let mut by_ref = DataValidation::new(boxes("D2"), DvKind::List);
    by_ref.a = "=Sheet1!$A$1:$A$5".into();
    let mut custom = DataValidation::new(boxes("E1:E5"), DvKind::Custom);
    custom.a = "AND(E1>0,E1<A1)".into();
    let mut len = DataValidation::new(boxes("F1:F5"), DvKind::TextLength);
    len.cmp = Cmp::LessEqual;
    len.a = "8".into();
    SheetRules {
        cf: vec![
            rule(
                "A1:A100",
                1,
                CfKind::CellIs {
                    cmp: Cmp::Greater,
                    a: "100".into(),
                    b: String::new(),
                },
            ),
            rule(
                "A1:A100 C1:C5",
                2,
                CfKind::CellIs {
                    cmp: Cmp::Between,
                    a: "1".into(),
                    b: "$B$1".into(),
                },
            ),
            rule(
                "B1:B9",
                3,
                CfKind::Text {
                    op: TextOp::Contains,
                    text: "a\"b<".into(),
                },
            ),
            rule(
                "B1:B9",
                4,
                CfKind::Text {
                    op: TextOp::Begins,
                    text: "x".into(),
                },
            ),
            rule("B1:B9", 5, CfKind::Blanks { not: true }),
            rule("B1:B9", 6, CfKind::Errors { not: false }),
            rule(
                "C1:C50",
                7,
                CfKind::Top {
                    n: 5,
                    percent: true,
                    bottom: true,
                },
            ),
            rule(
                "C1:C50",
                8,
                CfKind::Average {
                    above: false,
                    equal: true,
                    std_dev: 2,
                },
            ),
            rule("D1:D50", 9, CfKind::Duplicates { unique: true }),
            rule("D1:D50", 10, CfKind::Expression("AND(D1>1,$A1<3)".into())),
            rule(
                "E1:E50",
                11,
                CfKind::ColorScale(vec![
                    (CfValue::new(ValueKind::Min, ""), 0xF8696B),
                    (CfValue::new(ValueKind::Percentile, "50"), 0xFFEB84),
                    (CfValue::new(ValueKind::Max, ""), 0x63BE7B),
                ]),
            ),
            rule(
                "F1:F50",
                12,
                CfKind::DataBar {
                    min: CfValue::new(ValueKind::Num, "0"),
                    max: CfValue::new(ValueKind::Max, ""),
                    positive: 0x638EC6,
                    negative: 0xFF0000,
                    gradient: true,
                    show_value: false,
                },
            ),
            rule(
                "G1:G50",
                13,
                CfKind::IconSet {
                    set: 3,
                    thresholds: vec![
                        CfValue::new(ValueKind::Percent, "0"),
                        CfValue {
                            gte: false,
                            ..CfValue::new(ValueKind::Percent, "33")
                        },
                        CfValue::new(ValueKind::Num, "67"),
                    ],
                    reverse: true,
                    show_value: false,
                },
            ),
        ],
        dv: vec![dv, list, by_ref, custom, len],
        unsupported: Vec::new(),
        today: None,
    }
}

fn styling(rules: SheetRules) -> SheetStyling {
    SheetStyling {
        rules,
        ..SheetStyling::default()
    }
}

#[test]
fn xlsx_rules_round_trip_with_styles_and_priorities() {
    let rules = sample_rules();
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    let bytes =
        OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling(rules.clone())]).unwrap();
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    let mut got = back[0].1.rules.clone();
    // The data bar's negative colour and gradient flag live in a schema extension the writer omits.
    for r in &mut got.cf {
        if let CfKind::DataBar {
            negative, gradient, ..
        } = &mut r.kind
        {
            *negative = 0xFF0000;
            *gradient = true;
        }
    }
    assert_eq!(got.cf, rules.cf);
    assert_eq!(got.dv, rules.dv);
    assert!(got.unsupported.is_empty(), "{:?}", got.unsupported);
}

#[test]
fn xlsx_reader_names_rules_it_cannot_hold() {
    let rules = SheetRules {
        cf: vec![rule("A1:A3", 1, CfKind::Blanks { not: false })],
        ..SheetRules::default()
    };
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling(rules)]).unwrap();
    let mut entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    for e in &mut entries {
        if e.path == "xl/worksheets/sheet1.xml" {
            let xml = String::from_utf8(e.data.clone()).unwrap().replace(
                "</worksheet>",
                "<conditionalFormatting sqref=\"B1\"><cfRule type=\"timePeriod\" timePeriod=\"decade\" priority=\"2\"/></conditionalFormatting><dataValidations count=\"1\"><dataValidation sqref=\"C1\"/></dataValidations></worksheet>",
            );
            e.data = xml.into_bytes();
        }
    }
    let repacked = easyoffice_formats::ZipArchiveCodec::pack_compressed(
        &entries
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&repacked).unwrap();
    let rules = &back[0].1.rules;
    assert_eq!(rules.cf.len(), 1);
    assert_eq!(
        rules.unsupported,
        vec![
            "조건부 서식 timePeriod".to_string(),
            "데이터 유효성 none".to_string()
        ]
    );
}

#[test]
fn rules_count_is_bounded_on_read() {
    let mut cf = String::new();
    for i in 0..400 {
        cf.push_str(&format!(
            "<conditionalFormatting sqref=\"A{n}\"><cfRule type=\"duplicateValues\" priority=\"{n}\"/></conditionalFormatting>",
            n = i + 1
        ));
    }
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[]).unwrap();
    let mut entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    for e in &mut entries {
        if e.path == "xl/worksheets/sheet1.xml" {
            e.data = String::from_utf8(e.data.clone())
                .unwrap()
                .replace("</worksheet>", &format!("{cf}</worksheet>"))
                .into_bytes();
        }
    }
    let repacked = easyoffice_formats::ZipArchiveCodec::pack_compressed(
        &entries
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&repacked).unwrap();
    assert_eq!(
        back[0].1.rules.cf.len(),
        easyoffice_formats::MAX_RULES_PER_SHEET
    );
}

fn ods_sample() -> SheetRules {
    let mut rules = sample_rules();
    // ODS has no blank/error/std-dev/reverse/stop-if-true forms: keep the rules it can express.
    rules
        .cf
        .retain(|r| !matches!(r.kind, CfKind::Blanks { .. } | CfKind::Errors { .. }));
    for r in &mut rules.cf {
        r.stop_if_true = false;
        match &mut r.kind {
            CfKind::Average { std_dev, .. } => *std_dev = 0,
            CfKind::IconSet {
                reverse,
                thresholds,
                ..
            } => {
                *reverse = false;
                thresholds.iter_mut().for_each(|t| t.gte = true);
            }
            _ => {}
        }
    }
    for (i, r) in rules.cf.iter_mut().enumerate() {
        r.priority = i as u32 + 1;
    }
    for v in &mut rules.dv {
        v.show_error = true;
    }
    rules.dv[0].show_prompt = true;
    // Only the cells in range carry a validation: single-cell and small ranges are exact.
    rules
}

#[test]
fn ods_rules_round_trip_through_calc_extension_elements() {
    let rules = ods_sample();
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![(0, 0, "1".to_string()), (3, 1, "5".to_string())],
    )];
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &[styling(rules.clone())]).unwrap();
    let back =
        easyoffice_formats::read_sheet_styling(&bytes, easyoffice_formats::DocumentFormat::Ods)
            .unwrap();
    let mut got = back[0].1.rules.clone();
    for r in &mut got.cf {
        if let CfKind::DataBar {
            negative, gradient, ..
        } = &mut r.kind
        {
            // Preserved by ODS, unlike XLSX.
            assert_eq!((*negative, *gradient), (0xFF0000, true));
        }
    }
    let norm = |mut v: Vec<DataValidation>| {
        v.sort_by_key(|d| d.ranges[0]);
        v
    };
    assert_eq!(got.cf, rules.cf);
    assert_eq!(norm(got.dv.clone()), norm(rules.dv.clone()));
    assert!(got.unsupported.is_empty(), "{:?}", got.unsupported);
    // The cells are still there and unchanged.
    let cells = OdfCodec::unpack_ods_multi(&bytes).unwrap();
    assert_eq!(cells[0].1.iter().filter(|c| !c.2.is_empty()).count(), 2);
}

#[test]
fn ods_names_conditions_it_cannot_read() {
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    let rules = SheetRules {
        cf: vec![rule("A1", 1, CfKind::Duplicates { unique: false })],
        ..SheetRules::default()
    };
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &[styling(rules)]).unwrap();
    let mut entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    for e in &mut entries {
        if e.path == "content.xml" {
            e.data = String::from_utf8(e.data.clone())
                .unwrap()
                .replace(
                    "calcext:value=\"duplicate\"",
                    "calcext:value=\"date(today)\"",
                )
                .into_bytes();
        }
    }
    let repacked = easyoffice_formats::ZipArchiveCodec::pack_compressed(
        &entries
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let back =
        easyoffice_formats::read_sheet_styling(&repacked, easyoffice_formats::DocumentFormat::Ods)
            .unwrap();
    assert!(back[0].1.rules.cf.is_empty());
    assert_eq!(
        back[0].1.rules.unsupported,
        vec!["조건부 서식 date".to_string()]
    );
}

#[test]
fn ods_is_untouched_by_empty_rules() {
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &[SheetStyling::default()]).unwrap();
    let back =
        easyoffice_formats::read_sheet_styling(&bytes, easyoffice_formats::DocumentFormat::Ods)
            .unwrap();
    assert!(back[0].1.rules.is_empty());
}

fn written_sheet_xml(rules: SheetRules) -> (String, Vec<u8>) {
    let sheets = vec![("Sheet1".to_string(), vec![(0, 0, "1".to_string())])];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling(rules)]).unwrap();
    let entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    let xml = entries
        .iter()
        .find(|e| e.path == "xl/worksheets/sheet1.xml")
        .map(|e| String::from_utf8(e.data.clone()).unwrap())
        .unwrap();
    (xml, bytes)
}

#[test]
fn xlsx_period_rules_round_trip_with_the_period_and_its_formula() {
    let cf: Vec<CfRule> = TimePeriod::NAMES
        .iter()
        .enumerate()
        .map(|(i, (period, _))| rule("D2:D30", i as u32 + 1, CfKind::TimePeriod(*period)))
        .collect();
    let rules = SheetRules {
        cf,
        ..SheetRules::default()
    };
    let (xml, bytes) = written_sheet_xml(rules.clone());
    for (_, name) in TimePeriod::NAMES {
        assert!(
            xml.contains(&format!("type=\"timePeriod\" timePeriod=\"{name}\"")),
            "{name}"
        );
    }
    assert!(xml.contains("<formula>FLOOR(D2,1)=TODAY()</formula>"));
    assert!(xml.contains("<formula>AND(MONTH(D2)=MONTH(EDATE(TODAY(),0-1)),YEAR(D2)=YEAR(EDATE(TODAY(),0-1)))</formula>"));
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    assert_eq!(back[0].1.rules.cf, rules.cf);
    assert!(back[0].1.rules.unsupported.is_empty());
}

#[test]
fn xlsx_writes_a_check_box_and_a_multiple_list_as_lists_and_reads_them_back() {
    let mut check = DataValidation::new(boxes("B2:B9"), DvKind::Checkbox);
    check.a = "완료".into();
    check.b = "미완료".into();
    let plain = DataValidation::new(boxes("C2:C9"), DvKind::Checkbox);
    let mut multi = DataValidation::new(boxes("D2:D9"), DvKind::ListMultiple);
    multi.a = "사과,배,감".into();
    let mut single = DataValidation::new(boxes("E2:E9"), DvKind::List);
    single.a = "가,나".into();
    let rules = SheetRules {
        dv: vec![check, plain, multi, single],
        ..SheetRules::default()
    };
    let (xml, bytes) = written_sheet_xml(rules.clone());
    // Other programs see lists of the values, and the kind extension sits last in the sheet.
    assert!(xml.contains("<formula1>&quot;완료,미완료&quot;</formula1>"));
    assert!(xml.contains("<formula1>&quot;TRUE,FALSE&quot;</formula1>"));
    assert!(xml.contains("<formula1>&quot;사과,배,감&quot;</formula1>"));
    assert_eq!(xml.matches("type=\"list\"").count(), 4);
    assert!(xml.ends_with("</ext></extLst></worksheet>"));
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
    let got = &back[0].1.rules;
    assert_eq!(got.dv[0].kind, DvKind::Checkbox);
    assert_eq!(
        (got.dv[0].a.as_str(), got.dv[0].b.as_str()),
        ("완료", "미완료")
    );
    assert_eq!(got.dv[1].kind, DvKind::Checkbox);
    // The default values come back written out.
    assert_eq!(
        (got.dv[1].a.as_str(), got.dv[1].b.as_str()),
        ("TRUE", "FALSE")
    );
    assert_eq!(got.dv[2].kind, DvKind::ListMultiple);
    assert_eq!(got.dv[2].a, "사과,배,감");
    assert_eq!(got.dv[3].kind, DvKind::List);
    assert!(got.unsupported.is_empty());
}

#[test]
fn xlsx_does_not_read_a_kind_the_format_does_not_have() {
    let rules = SheetRules {
        dv: vec![DataValidation::new(boxes("A1"), DvKind::List)],
        ..SheetRules::default()
    };
    let (_, bytes) = written_sheet_xml(rules);
    let mut entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    for e in &mut entries {
        if e.path == "xl/worksheets/sheet1.xml" {
            let xml = String::from_utf8(e.data.clone()).unwrap();
            e.data = xml
                .replace("type=\"list\"", "type=\"checkbox\"")
                .into_bytes();
        }
    }
    let repacked = easyoffice_formats::ZipArchiveCodec::pack_compressed(
        &entries
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&repacked).unwrap();
    assert!(back[0].1.rules.dv.is_empty());
    assert_eq!(
        back[0].1.rules.unsupported,
        vec!["데이터 유효성 checkbox".to_string()]
    );
}

#[test]
fn ods_writes_period_rules_as_formula_rules_and_check_boxes_as_lists() {
    let rules = SheetRules {
        cf: vec![rule("D2:D30", 1, CfKind::TimePeriod(TimePeriod::Last7Days))],
        dv: vec![{
            let mut c = DataValidation::new(boxes("B2:B3"), DvKind::Checkbox);
            c.a = "예".into();
            c.b = "아니오".into();
            c
        }],
        ..SheetRules::default()
    };
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![(1, 1, "예".to_string()), (2, 1, "".to_string())],
    )];
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets, &[styling(rules)]).unwrap();
    let back =
        easyoffice_formats::read_sheet_styling(&bytes, easyoffice_formats::DocumentFormat::Ods)
            .unwrap();
    let got = &back[0].1.rules;
    assert!(
        matches!(&got.cf[0].kind, CfKind::Expression(f) if f.contains("TODAY()") && f.contains("D2")),
        "{:?}",
        got.cf
    );
    assert_eq!(got.dv[0].kind, DvKind::List);
    assert_eq!(got.dv[0].a, "예,아니오");
}
