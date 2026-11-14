//! Tables survive XLSX and ODS round trips and a hostile table part or database range is read to a bound or
//! turned away (PD-224).

use easyoffice_formats::{
    CellBox, DefinedName, EasyOfficeError, OdfCodec, OoxmlCodec, SheetStyling, SheetTable,
    ZipArchiveCodec, ZipSecurityPolicy, MAX_TABLES,
};

fn table(name: &str, range: &str) -> SheetTable {
    SheetTable::new(name, CellBox::parse(range).unwrap())
}

fn headers(row: u32, names: &[&str]) -> Vec<(u32, u32, String)> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| (row, i as u32, (*n).to_string()))
        .collect()
}

type Cells = Vec<(u32, u32, String)>;

fn xlsx(sheets: &[(&str, Cells, Vec<SheetTable>)]) -> Vec<u8> {
    let cells: Vec<_> = sheets
        .iter()
        .map(|(n, c, _)| ((*n).to_string(), c.clone()))
        .collect();
    let stylings: Vec<_> = sheets
        .iter()
        .map(|(_, _, t)| SheetStyling {
            tables: t.clone(),
            ..SheetStyling::default()
        })
        .collect();
    OoxmlCodec::pack_xlsx_multi_styled(&cells, &[], &stylings).unwrap()
}

fn read_xlsx(bytes: &[u8]) -> Vec<SheetStyling> {
    OoxmlCodec::unpack_xlsx_with_styling(bytes)
        .unwrap()
        .1
        .into_iter()
        .map(|(_, s)| s)
        .collect()
}

fn entry(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let e = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(e.data.clone()).unwrap()
}

fn rebuild(bytes: &[u8], edit: impl Fn(&str, String) -> String) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let parts: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let text = String::from_utf8_lossy(&e.data).into_owned();
            (e.path.clone(), edit(&e.path, text).into_bytes())
        })
        .collect();
    ZipArchiveCodec::pack(&parts).unwrap()
}

#[test]
fn tables_of_two_sheets_round_trip_through_xlsx() {
    let mut sales = table("Sales", "A1:C6");
    sales.totals = true;
    sales.style = Some("TableStyleMedium2".into());
    sales.first_column = true;
    sales.filter = Some(Vec::new());
    sales.columns = vec!["Region".into(), "Q1".into(), "Q2".into()];
    let mut plain = table("Plain", "E2:F4");
    plain.header = false;
    plain.row_stripes = false;
    plain.columns = vec!["p".into(), "q".into()];
    let bytes = xlsx(&[
        (
            "Data",
            headers(0, &["Region", "Q1", "Q2"]),
            vec![sales.clone(), plain.clone()],
        ),
        ("Other", Vec::new(), vec![table("Second", "B2:B3")]),
    ]);
    let back = read_xlsx(&bytes);
    assert_eq!(back[0].tables, vec![sales, plain]);
    assert_eq!(back[0].tables_rejected, 0);
    // The second sheet's header cell did not exist: it is written as `Column1`, so the table reads as that.
    assert_eq!(back[1].tables.len(), 1);
    assert_eq!(back[1].tables[0].columns, ["Column1"]);

    let part = entry(&bytes, "xl/tables/table1.xml");
    assert!(part.contains("name=\"Sales\"") && part.contains("totalsRowCount=\"1\""));
    assert!(entry(&bytes, "xl/tables/table3.xml").contains("id=\"3\""));
    assert!(entry(&bytes, "[Content_Types].xml").contains("/xl/tables/table3.xml"));
    assert!(entry(&bytes, "xl/worksheets/_rels/sheet1.xml.rels").contains("../tables/table1.xml"));
    let sheet = entry(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<tableParts") && sheet.ends_with("</tableParts></worksheet>"));
}

#[test]
fn header_cells_become_strings_and_match_the_column_names() {
    let t = table("T", "A1:D3");
    let cells = vec![
        (0, 0, "2024".to_string()),
        (0, 1, "=A1".to_string()),
        (0, 2, "dup".to_string()),
        (0, 3, "DUP".to_string()),
        (1, 0, "10".to_string()),
    ];
    let bytes = xlsx(&[("S", cells, vec![t])]);
    let sheet = entry(&bytes, "xl/worksheets/sheet1.xml");
    // The data row keeps its number; the header cells are inline strings.
    assert!(
        sheet.contains("<c r=\"A1\" t=\"inlineStr\"><is><t xml:space=\"preserve\">2024</t>"),
        "{sheet}"
    );
    assert!(sheet.contains("<c r=\"A2\" t=\"n\"><v>10</v>"));
    assert!(sheet.contains(">Column2</t>") && sheet.contains(">DUP2</t>"));
    let part = entry(&bytes, "xl/tables/table1.xml");
    assert!(part.contains("name=\"2024\"") && part.contains("name=\"Column2\""));
    assert!(part.contains("name=\"dup\"") && part.contains("name=\"DUP2\""));
}

#[test]
fn table_names_stay_unique_and_apart_from_defined_names() {
    let mut first = SheetStyling {
        tables: vec![table("Rate", "A1:A3"), table("Other", "C1:C3")],
        ..SheetStyling::default()
    };
    first.defined_names.push(DefinedName {
        name: "OTHER".into(),
        refers_to: "S!$A$1".into(),
        ..DefinedName::default()
    });
    let second = SheetStyling {
        tables: vec![table("rate", "A1:A3")],
        ..SheetStyling::default()
    };
    let cells = vec![("S".to_string(), Vec::new()), ("T".to_string(), Vec::new())];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&cells, &[], &[first, second]).unwrap();
    let back = read_xlsx(&bytes);
    let names: Vec<String> = back
        .iter()
        .flat_map(|s| s.tables.iter().map(|t| t.name.clone()))
        .collect();
    assert_eq!(names, ["Rate", "Other_2", "rate_2"]);
}

#[test]
fn hostile_names_are_escaped_in_the_package() {
    let mut t = table("Tab", "A1:B2");
    t.columns = vec!["\"<>&".into(), "'x'".into()];
    t.style = Some("\"<>&".into());
    let bytes = xlsx(&[("S", headers(0, &["\"<>&", "'x'"]), vec![t.clone()])]);
    assert_eq!(read_xlsx(&bytes)[0].tables, vec![t]);
    // A name that is not a name of the syntax is refused when writing, never injected.
    let mut bad = table("a\"<>&", "A1:B2");
    bad.name = "a\"<>&".into();
    let cells = vec![("S".to_string(), Vec::new())];
    let styling = SheetStyling {
        tables: vec![bad],
        ..SheetStyling::default()
    };
    assert!(matches!(
        OoxmlCodec::pack_xlsx_multi_styled(&cells, &[], &[styling]),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn overlapping_and_oversized_tables_are_refused_when_writing() {
    let cells = vec![("S".to_string(), Vec::new())];
    let pack = |tables: Vec<SheetTable>| {
        OoxmlCodec::pack_xlsx_multi_styled(
            &cells,
            &[],
            &[SheetStyling {
                tables,
                ..SheetStyling::default()
            }],
        )
    };
    assert!(pack(vec![table("A", "A1:B3"), table("B", "B3:C5")]).is_err());
    let many: Vec<_> = (0..=MAX_TABLES)
        .map(|n| table(&format!("T{n}x"), &format!("A{}:A{}", 2 * n + 1, 2 * n + 2)))
        .collect();
    assert!(pack(many).is_err());
    let mut wide = table("W", "A1:XFD2");
    wide.columns.truncate(10);
    assert!(pack(vec![wide]).is_err());
}

fn with_table_parts(bytes: &[u8], extra: usize) -> Vec<u8> {
    let tags: String = (0..extra)
        .map(|n| format!("<tablePart r:id=\"rIdX{n}\"/>"))
        .collect();
    rebuild(bytes, |path, text| {
        if path == "xl/worksheets/sheet1.xml" {
            text.replace("</tableParts>", &format!("{tags}</tableParts>"))
        } else {
            text
        }
    })
}

#[test]
fn a_sheet_that_lists_a_hundred_thousand_table_parts_is_refused() {
    let bytes = xlsx(&[("S", headers(0, &["a"]), vec![table("T", "A1:A2")])]);
    let flooded = with_table_parts(&bytes, 100_000);
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&flooded),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn dangling_duplicate_and_traversing_table_parts_are_turned_away_and_counted() {
    let bytes = xlsx(&[("S", headers(0, &["a"]), vec![table("T", "A1:A2")])]);
    let edited = rebuild(&bytes, |path, text| {
        match path {
        "xl/worksheets/sheet1.xml" => text.replace(
            "</tableParts>",
            "<tablePart r:id=\"rIdT1\"/><tablePart r:id=\"rIdGone\"/><tablePart r:id=\"rIdUp\"/><tablePart/></tableParts>",
        ),
        "xl/worksheets/_rels/sheet1.xml.rels" => text.replace(
            "</Relationships>",
            "<Relationship Id=\"rIdUp\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/table\" Target=\"../../../../etc/passwd\"/></Relationships>",
        ),
        _ => text,
    }
    });
    let back = read_xlsx(&edited);
    // The same part listed twice is an overlap with itself; the others have no part.
    assert_eq!(back[0].tables.len(), 1);
    assert_eq!(back[0].tables_rejected, 4);
}

#[test]
fn a_part_with_malformed_content_is_turned_away() {
    let bytes = xlsx(&[("S", headers(0, &["a", "b"]), vec![table("T", "A1:B3")])]);
    let edits: [(&str, &str); 4] = [
        ("ref=\"A1:B3\"", "ref=\"A1:XFE3\""),
        ("<tableColumn id=\"2\" name=\"b\"/>", ""),
        (
            "name=\"T\" displayName=\"T\"",
            "name=\"1 bad\" displayName=\"1 bad\"",
        ),
        ("ref=\"A1:B3\"", "ref=\"A1:B3\" headerRowCount=\"7\""),
    ];
    for (from, to) in edits {
        let edited = rebuild(&bytes, |path, text| {
            if path == "xl/tables/table1.xml" {
                text.replace(from, to)
            } else {
                text
            }
        });
        let back = read_xlsx(&edited);
        assert!(
            back[0].tables.is_empty() && back[0].tables_rejected == 1,
            "{from} -> {to}"
        );
    }
}

#[test]
fn five_tables_of_sixteen_thousand_columns_exceed_the_workbook_budget() {
    let bytes = xlsx(&[("S", headers(0, &["a"]), vec![table("T", "A1:A2")])]);
    let columns: String = (1..=16_384)
        .map(|n| format!("<tableColumn id=\"{n}\" name=\"c{n}\"/>"))
        .collect();
    let part = |n: usize| {
        format!(
            "<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"{n}\" name=\"Wide{n}\" displayName=\"Wide{n}\" ref=\"A1:XFD2\"><tableColumns count=\"16384\">{columns}</tableColumns></table>"
        )
    };
    let extra = |count: usize| {
        let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                let mut text = String::from_utf8_lossy(&e.data).into_owned();
                match e.path.as_str() {
                    "xl/worksheets/sheet1.xml" => {
                        let more: String = (1..=count)
                            .map(|n| format!("<tablePart r:id=\"rIdW{n}\"/>"))
                            .collect();
                        text = text.replace("</tableParts>", &format!("{more}</tableParts>"));
                    }
                    "xl/worksheets/_rels/sheet1.xml.rels" => {
                        let more: String = (1..=count)
                            .map(|n| format!("<Relationship Id=\"rIdW{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/table\" Target=\"../tables/wide{n}.xml\"/>"))
                            .collect();
                        text = text.replace("</Relationships>", &format!("{more}</Relationships>"));
                    }
                    _ => {}
                }
                (e.path.clone(), text.into_bytes())
            })
            .collect::<Vec<_>>();
        entries
            .extend((1..=count).map(|n| (format!("xl/tables/wide{n}.xml"), part(n).into_bytes())));
        ZipArchiveCodec::pack(&entries).unwrap()
    };
    // Three wide tables are within the budget; five are over 65,536 columns.
    let three = read_xlsx(&extra(3));
    assert_eq!(
        three[0].tables.len(),
        1,
        "the wide tables overlap the first one and are turned away"
    );
    assert!(matches!(
        OoxmlCodec::unpack_xlsx_with_styling(&extra(5)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn tables_round_trip_through_ods_and_cross_to_xlsx() {
    let mut sales = table("Sales", "A1:C6");
    sales.style = Some("TableStyleMedium9".into());
    sales.totals = true;
    sales.filter = Some(Vec::new());
    let cells = vec![
        ("Data".to_string(), headers(0, &["Region", "Q1", "Q2"])),
        ("My Sheet".to_string(), Vec::new()),
    ];
    let stylings = [
        SheetStyling {
            tables: vec![sales.clone()],
            ..SheetStyling::default()
        },
        SheetStyling {
            tables: vec![table("Other", "B2:C3")],
            ..SheetStyling::default()
        },
    ];
    let ods = OdfCodec::pack_ods_multi_styled(&cells, &stylings).unwrap();
    let (sheets, back) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
    assert_eq!(back[0].1.tables.len(), 1);
    let got = &back[0].1.tables[0];
    assert_eq!(
        (got.name.as_str(), got.range, got.header),
        ("Sales", sales.range, true)
    );
    assert!(got.totals && got.filter.is_some() && got.style == sales.style && got.row_stripes);
    assert_eq!(back[1].1.tables[0].range.to_a1(), "B2:C3");
    // ODS to XLSX: the column names are the header cells' text.
    let styled: Vec<SheetStyling> = back.into_iter().map(|(_, s)| s).collect();
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &styled).unwrap();
    let again = read_xlsx(&xlsx);
    assert_eq!(again[0].tables[0].columns, ["Region", "Q1", "Q2"]);
    assert_eq!(again[0].tables[0].name, "Sales");
}

#[test]
fn a_header_formula_is_written_as_the_text_it_computed() {
    use easyoffice_formats::XlsxCachedValue;
    let cells = vec![
        (0, 0, "=\"Reg\"&\"ion\"".to_string()),
        (0, 1, "=1+1".to_string()),
        (1, 0, "x".to_string()),
    ];
    let mut cache = easyoffice_formats::XlsxFormulaCache::new();
    cache.insert((0, 0), XlsxCachedValue::Text("Region".into()));
    cache.insert((0, 1), XlsxCachedValue::Number(2.0));
    let styling = SheetStyling {
        tables: vec![table("T", "A1:B3")],
        ..SheetStyling::default()
    };
    let sheets = vec![("S".to_string(), cells)];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[cache], &[styling]).unwrap();
    let sheet = entry(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(">Region</t>") && sheet.contains(">2</t>"),
        "{sheet}"
    );
    assert!(
        !sheet.contains("<f>"),
        "no formula stays in a header cell: {sheet}"
    );
    assert_eq!(read_xlsx(&bytes)[0].tables[0].columns, ["Region", "2"]);
}

#[test]
fn sixteen_thousand_equal_headers_get_unique_names() {
    let width = 16_384u32;
    let cells: Cells = (0..width).map(|c| (0, c, "h".to_string())).collect();
    let mut t = table("T", "A1:XFD2");
    t.columns = (0..width).map(|_| "h".to_string()).collect();
    let started = std::time::Instant::now();
    let bytes = xlsx(&[("S", cells, vec![t])]);
    assert!(started.elapsed().as_secs() < 5, "linear, not quadratic");
    let names = &read_xlsx(&bytes)[0].tables[0].columns;
    let unique: std::collections::HashSet<String> =
        names.iter().map(|n| n.to_lowercase()).collect();
    assert_eq!(unique.len(), width as usize);
    assert_eq!(
        (names[0].as_str(), names[1].as_str(), names[16_383].as_str()),
        ("h", "h2", "h16384")
    );
}

#[test]
fn a_table_part_listed_twice_or_by_two_sheets_is_one_table() {
    let bytes = xlsx(&[
        ("S", headers(0, &["a"]), vec![table("T", "A1:A2")]),
        ("U", headers(0, &["a"]), vec![table("V", "A1:A2")]),
    ]);
    let edited = rebuild(&bytes, |path, text| match path {
        // Sheet 2 lists the part of sheet 1's table, and sheet 1 lists its own twice more.
        "xl/worksheets/_rels/sheet2.xml.rels" => {
            text.replace("../tables/table2.xml", "../tables/table1.xml")
        }
        "xl/worksheets/sheet1.xml" => text.replace(
            "</tableParts>",
            "<tablePart r:id=\"rIdT1\"/><tablePart r:id=\"rIdT1\"/></tableParts>",
        ),
        _ => text,
    });
    let back = read_xlsx(&edited);
    assert_eq!(back[0].tables.len(), 1);
    assert!(back[1].tables.is_empty());
    assert_eq!((back[0].tables_rejected, back[1].tables_rejected), (2, 1));
}

#[test]
fn merged_ranges_over_a_table_are_left_out_of_a_file_and_of_a_save() {
    let bytes = xlsx(&[("S", headers(0, &["a", "b"]), vec![table("T", "A1:B3")])]);
    let with_merges = rebuild(&bytes, |path, text| {
        if path == "xl/worksheets/sheet1.xml" {
            text.replace(
                "</sheetData>",
                "</sheetData><mergeCells count=\"2\"><mergeCell ref=\"A2:A3\"/><mergeCell ref=\"D5:E6\"/></mergeCells>",
            )
        } else {
            text
        }
    });
    let back = read_xlsx(&with_merges);
    assert_eq!(back[0].merges, vec![CellBox::parse("D5:E6").unwrap()]);
    assert_eq!(back[0].merges_dropped, 1);
    assert_eq!(back[0].tables.len(), 1, "the table stays whole");
    // A save of both writes only the merge that is clear of the table.
    let mut styling = back[0].clone();
    styling.merges = vec![
        CellBox::parse("A2:A3").unwrap(),
        CellBox::parse("D5:E6").unwrap(),
    ];
    let cells = vec![("S".to_string(), headers(0, &["a", "b"]))];
    let saved = OoxmlCodec::pack_xlsx_multi_styled(&cells, &[], &[styling]).unwrap();
    let sheet = entry(&saved, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("ref=\"D5:E6\"") && !sheet.contains("ref=\"A2:A3\""),
        "{sheet}"
    );
}
