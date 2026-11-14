use super::*;

#[test]
fn test_swar_csv_parser_large_rows_escaped_quotes_crlf_lf_and_korean_utf8() {
    let mut table = Vec::with_capacity(64);
    for r in 0..64 {
        table.push(vec![
            format!("기획조정실_{r:03}"),
            format!("\"예산집행, 1분기 \"\"승인\"\" ({r})\""),
            format!("{}", (r + 1) * 1250),
            format!("\"다중행 메모 1행\n2행 상세내역 {r}\""),
            "디지털행정혁신과".to_string(),
            String::new(),
        ]);
    }

    let crlf_csv = table
        .iter()
        .map(|row| row.join(","))
        .collect::<Vec<_>>()
        .join("\r\n");
    let lf_csv = crlf_csv.replace("\r\n", "\n");

    let parsed_crlf = CsvCodec::parse(&crlf_csv, ',').unwrap();
    let baseline_crlf = CsvCodec::parse_scalar_baseline(&crlf_csv, ',').unwrap();
    let parsed_lf = CsvCodec::parse(&lf_csv, ',').unwrap();

    assert_eq!(parsed_crlf.len(), 64);
    assert_eq!(parsed_crlf, baseline_crlf);
    assert_eq!(parsed_lf, baseline_crlf);
    assert_eq!(parsed_crlf[0][0], "기획조정실_000");
    assert_eq!(parsed_crlf[0][1], "예산집행, 1분기 \"승인\" (0)");
    assert_eq!(parsed_crlf[0][3], "다중행 메모 1행\n2행 상세내역 0");
    assert_eq!(parsed_crlf[0][5], "");

    assert!(CsvCodec::parse("\"unclosed quote", ',').is_err());
    assert!(CsvCodec::parse("unquoted\"quote,val", ',').is_err());

    let mut visited_borrowed = 0usize;
    let mut visited_owned = 0usize;
    let mut visited_rows = vec![Vec::<String>::new()];
    CsvCodec::visit_fields_swar(&crlf_csv, ',', |r_idx, _c_idx, field| {
        while visited_rows.len() <= r_idx {
            visited_rows.push(Vec::new());
        }
        match &field {
            Cow::Borrowed(_) => visited_borrowed += 1,
            Cow::Owned(_) => visited_owned += 1,
        }
        visited_rows[r_idx].push(field.into_owned());
    })
    .unwrap();
    assert_eq!(visited_rows, parsed_crlf);
    assert_eq!(visited_owned, 64);
    assert_eq!(visited_borrowed, 64 * 5);
    assert_eq!(
        crate::rtf_xml_helpers::unescape_xml_single_pass("A&#0;&#x01;&#xFFFE;&#65;B"),
        "A&#0;&#x01;&#xFFFE;AB"
    );
}

#[test]
fn test_f14_csv_scalar_baseline_enforces_max_cols() {
    let too_many_cols = "a,".repeat(CsvCodec::MAX_COLS + 1);
    assert!(matches!(
        CsvCodec::parse_scalar_baseline(&too_many_cols, ','),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hostile_csv_exceeding_max_grid_cells_fails_closed() {
    let row = format!("{}\n", "a,".repeat(2_000));
    let csv = row.repeat(1_001);
    assert!(matches!(
        CsvCodec::visit_fields_swar(&csv, ',', |_, _, _| {}),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    assert!(matches!(
        CsvCodec::parse(&csv, ','),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn serialize_with_bom_and_one_pass_quoting_round_trip() {
    let long_prefix = format!("{}\"quoted\"{}", "x".repeat(47), "y".repeat(29));
    let rows = vec![
        vec![
            "plain".to_string(),
            "has,comma".to_string(),
            "has\"quote\"twice".to_string(),
            long_prefix,
            "\"\"".to_string(),
            "line1\nline2".to_string(),
            "cr\rinside".to_string(),
        ],
        vec!["가나다".to_string(), "탭\t포함".to_string()],
    ];
    let with_bom = CsvCodec::serialize_with_bom(&rows, ',');
    assert!(with_bom.starts_with(CsvCodec::UTF8_BOM));
    let body = CsvCodec::serialize(&rows, ',');
    assert_eq!(&with_bom[3..], body.as_bytes());
    let bom_str = std::str::from_utf8(&with_bom).unwrap();
    let reparsed = CsvCodec::parse(bom_str, ',').unwrap();
    assert_eq!(reparsed, rows);

    let mut visited = Vec::new();
    CsvCodec::visit_fields_swar(bom_str, ',', |r, c, v| visited.push((r, c, v.into_owned())))
        .unwrap();
    assert_eq!(visited.first().map(|x| x.2.as_str()), Some("plain"));

    // Non-ASCII delimiter single-pass quoting parity.
    let mut visited_non_ascii = Vec::new();
    let non_ascii_ser = CsvCodec::serialize(&rows, '§');
    assert_eq!(CsvCodec::parse(&non_ascii_ser, '§').unwrap(), rows);
    CsvCodec::visit_fields_swar(&format!("\u{FEFF}{non_ascii_ser}"), '§', |r, c, v| {
        visited_non_ascii.push((r, c, v.into_owned()))
    })
    .unwrap();
    assert_eq!(
        visited_non_ascii.first().map(|x| x.2.as_str()),
        Some("plain")
    );
}

#[test]
fn hostile_markdown_table_exceeding_max_grid_cells_fails_closed() {
    let cols = 2_000usize;
    let header = format!("{}|\n{}|\n", "|a".repeat(cols), "|---".repeat(cols));
    let row = format!("{}|\n", "|a".repeat(cols));
    let mut md = header;
    for _ in 0..1_001 {
        md.push_str(&row);
    }
    assert!(matches!(
        CommonMarkCodec::parse(&md),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
