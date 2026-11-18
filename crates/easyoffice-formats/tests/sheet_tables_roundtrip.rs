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

#[test]
fn calculated_columns_and_hidden_buttons_round_trip_through_xlsx_and_refuse_overlong_formulas() {
    let mut t = table("Sales", "B2:E5");
    t.totals = true;
    t.columns = vec!["Item".into(), "Price".into(), "Qty".into(), "Total".into()];
    t.filter = Some(Vec::new());
    t.hidden_buttons = vec![2, 4]; // C and E (0-based 2 and 4)
    t.calculated_columns = vec![
        None,
        None,
        None,
        Some("=IFS([@Qty]>0,[@Price]*[@Qty])".into()),
    ];
    let cells = vec![
        (1, 1, "Item".to_string()),
        (1, 2, "Price".to_string()),
        (1, 3, "Qty".to_string()),
        (1, 4, "Total".to_string()),
        (2, 1, "Pen".to_string()),
        (2, 2, "100".to_string()),
        (2, 3, "3".to_string()),
        (2, 4, "=IFS([@Qty]>0,[@Price]*[@Qty])".to_string()),
        (3, 1, "Book".to_string()),
        (3, 2, "250".to_string()),
        (3, 3, "2".to_string()),
        (3, 4, "=IFS([@Qty]>0,[@Price]*[@Qty])".to_string()),
        (4, 1, "합계".to_string()),
        (4, 4, "=SUBTOTAL(109,Sales[Total])".to_string()),
    ];
    let bytes = xlsx(&[("S", cells, vec![t])]);
    let part = entry(&bytes, "xl/tables/table1.xml");
    assert!(
        part.contains(r#"<filterColumn colId="1" hiddenButton="1"/>"#)
            && part.contains(r#"<filterColumn colId="3" hiddenButton="1"/>"#),
        "{part}"
    );
    // ECMA-376 CT_TableColumn: calculatedColumnFormula precedes totalsRowFunction/totalsRowFormula, with _xlfn. prefix.
    assert!(
        part.contains("<calculatedColumnFormula>_xlfn.IFS(Sales[[#This Row],[Qty]]&gt;0,Sales[[#This Row],[Price]]*Sales[[#This Row],[Qty]])</calculatedColumnFormula>")
            || part.contains("<calculatedColumnFormula>_xlfn.IFS([@Qty]&gt;0,[@Price]*[@Qty])</calculatedColumnFormula>"),
        "{part}"
    );
    let back = read_xlsx(&bytes);
    assert_eq!(back[0].tables[0].hidden_buttons, vec![2, 4]);
    assert_eq!(
        back[0].tables[0].calculated_columns,
        vec![
            None,
            None,
            None,
            Some(
                "=IFS(Sales[[#This Row],[Qty]]>0,Sales[[#This Row],[Price]]*Sales[[#This Row],[Qty]])"
                    .into()
            )
        ]
    );

    // Fail-Closed: calculatedColumnFormula over 8,192 characters rejects the hostile table part.
    let huge = "1+".repeat(4_100) + "1";
    let hostile = rebuild(&bytes, |path, xml| {
        if path == "xl/tables/table1.xml" {
            let start = xml.find("<calculatedColumnFormula>").unwrap();
            let end = xml.find("</calculatedColumnFormula>").unwrap()
                + "</calculatedColumnFormula>".len();
            format!(
                "{}<calculatedColumnFormula>{huge}</calculatedColumnFormula>{}",
                &xml[..start],
                &xml[end..]
            )
        } else {
            xml
        }
    });
    let checked = read_xlsx(&hostile);
    assert!(checked[0].tables.is_empty());
    assert_eq!(checked[0].tables_rejected, 1);
}

#[test]
fn workbook_theme_and_table_slicers_round_trip_and_import_excel_slicer_caches() {
    use easyoffice_formats::{TableSlicer, ZipArchiveCodec, ZipSecurityPolicy};

    let custom_theme: [u32; 12] = [
        0xFFFFFF, 0x111827, 0xF3F4F6, 0x1F2937, 0x0F766E, 0xD97706, 0x4F46E5, 0xDC2626, 0x0284C7,
        0x65A30D, 0x2563EB, 0x7C3AED,
    ];
    let mut t = table("Sales", "A1:C4");
    t.columns = vec!["지역".into(), "품목".into(), "매출".into()];
    t.slicers = vec![TableSlicer {
        col: 0,
        caption: "지역".into(),
        left: 36,
        top: 28,
        width: 190,
        height: 220,
        multi_select: true,
    }];
    let styling = SheetStyling {
        workbook_theme: Some(custom_theme),
        tables: vec![t],
        ..SheetStyling::default()
    };
    let sheets = vec![("S".to_string(), headers(0, &["지역", "품목", "매출"]))];
    let packed = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[styling]).unwrap();
    let theme_xml = entry(&packed, "xl/theme/theme1.xml");
    assert!(
        theme_xml.contains("<a:accent1><a:srgbClr val=\"0F766E\"/></a:accent1>"),
        "{theme_xml}"
    );

    // Extra colour schemes inside `<a:extraClrSchemeLst>` must not clobber `<a:themeElements>`.
    let with_extra_scheme = rebuild(&packed, |path, xml| {
        if path == "xl/theme/theme1.xml" {
            xml.replace(
                "<a:extraClrSchemeLst/>",
                "<a:extraClrSchemeLst><a:extraClrScheme><a:clrScheme name=\"Alt\"><a:accent1><a:srgbClr val=\"FF00FF\"/></a:accent1></a:clrScheme></a:extraClrScheme></a:extraClrSchemeLst>",
            )
        } else {
            xml
        }
    });
    let back = read_xlsx(&with_extra_scheme);
    assert_eq!(back[0].workbook_theme, Some(custom_theme));
    assert_eq!(back[0].tables[0].slicers.len(), 1);
    assert_eq!(back[0].tables[0].slicers[0].caption, "지역");
    assert!(back[0].tables[0].slicers[0].multi_select);

    // Native `[MS-XLSX]` `xl/slicerCaches/slicerCache1.xml` + `xl/slicers/slicer1.xml` import.
    let plain = xlsx(&[(
        "S",
        headers(0, &["지역", "품목", "매출"]),
        vec![table("Sales", "A1:C4")],
    )]);
    let mut raw_entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&plain, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.data))
            .collect();
    raw_entries.push((
        "xl/slicerCaches/slicerCache1.xml".into(),
        r#"<?xml version="1.0" encoding="UTF-8"?><slicerCacheDefinition xmlns="urn:spreadsheetml:2009:main" name="Slicer_Item" sourceName="품목"><extLst><ext uri="{470722E0-AACD-4C17-9CDC-17EF765DBC7E}" xmlns:x15="urn:spreadsheetml:2010:main"><x15:tableSlicerCache tableId="1" column="2"/></ext></extLst></slicerCacheDefinition>"#.as_bytes().to_vec(),
    ));
    raw_entries.push((
        "xl/slicers/slicer1.xml".into(),
        r#"<?xml version="1.0" encoding="UTF-8"?><slicers xmlns="urn:spreadsheetml:2009:main"><slicer name="품목" cache="Slicer_Item" caption="품목 필터" width="1905000" height="2095500"/></slicers>"#.as_bytes().to_vec(),
    ));
    let with_excel_slicer = ZipArchiveCodec::pack(&raw_entries).unwrap();
    let imported = read_xlsx(&with_excel_slicer);
    assert_eq!(imported[0].tables[0].slicers.len(), 1);
    assert_eq!(imported[0].tables[0].slicers[0].col, 1);
    assert_eq!(imported[0].tables[0].slicers[0].caption, "품목 필터");
    assert_eq!(imported[0].tables[0].slicers[0].width, 200);
    assert_eq!(imported[0].tables[0].slicers[0].height, 220);

    // Hostile flood of >64 `<eo:slicer>` tags inside `<extLst>` rejects the table part fail-closed.
    let flood_slicers: String = (0..65)
        .map(|_| r#"<eo:slicer col="0" caption="a" left="10" top="10" width="180" height="200" multi="0"/>"#)
        .collect();
    let flooded = rebuild(&packed, |path, xml| {
        if path == "xl/tables/table1.xml" {
            let s = xml.find("<eo:slicers").unwrap();
            let e = xml.find("</eo:slicers>").unwrap() + "</eo:slicers>".len();
            format!(
                "{}<eo:slicers xmlns:eo=\"urn:easyoffice:spreadsheet:slicers:1.0\">{flood_slicers}</eo:slicers>{}",
                &xml[..s],
                &xml[e..]
            )
        } else {
            xml
        }
    });
    let rejected = read_xlsx(&flooded);
    assert!(rejected[0].tables.is_empty());
    assert_eq!(rejected[0].tables_rejected, 1);
}

#[test]
#[ignore]
fn bench_pd242_theme_and_slicers_xlsx_roundtrip_release() {
    use easyoffice_formats::TableSlicer;
    use std::time::Instant;

    let mut table = SheetTable::new("영업표", CellBox::parse("A1:D201").unwrap());
    table.columns = vec!["지역".into(), "품목".into(), "분기".into(), "매출".into()];
    table.slicers = vec![
        TableSlicer {
            col: 0,
            caption: "지역".into(),
            left: 24,
            top: 24,
            width: 184,
            height: 208,
            multi_select: true,
        },
        TableSlicer {
            col: 1,
            caption: "품목".into(),
            left: 220,
            top: 24,
            width: 184,
            height: 208,
            multi_select: false,
        },
    ];
    let custom_theme = [
        0xFFFFFF, 0x111827, 0xF3F4F6, 0x1F2937, 0x0F766E, 0xB45309, 0x6B7280, 0xEAB308, 0x0284C7,
        0x15803D, 0x0369A1, 0x7E22CE,
    ];
    let mut cells = vec![
        (0, 0, "지역".into()),
        (0, 1, "품목".into()),
        (0, 2, "분기".into()),
        (0, 3, "매출".into()),
    ];
    let regions = ["서울", "부산", "대전", "광주", "인천"];
    let items = ["노트북", "모니터", "키보드", "마우스"];
    for r in 1..=200u32 {
        cells.push((r, 0, regions[(r as usize) % regions.len()].into()));
        cells.push((r, 1, items[(r as usize) % items.len()].into()));
        cells.push((r, 2, format!("Q{}", (r % 4) + 1)));
        cells.push((r, 3, format!("{}", r * 125)));
    }
    let styling = SheetStyling {
        tables: vec![table],
        workbook_theme: Some(custom_theme),
        ..SheetStyling::default()
    };
    let sheets = [("Sheet1".to_string(), cells)];
    let stylings = [styling];

    let iters = 100u32;
    let t0 = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iters {
        let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).unwrap();
        total_bytes += bytes.len();
        let (_, unpacked) = OoxmlCodec::unpack_xlsx_with_styling(&bytes).unwrap();
        assert_eq!(unpacked[0].1.workbook_theme, Some(custom_theme));
        assert_eq!(unpacked[0].1.tables[0].slicers.len(), 2);
    }
    let elapsed_us = t0.elapsed().as_micros();
    println!(
        "pd242_xlsx_theme_slicers_roundtrip: iters={iters} total_us={elapsed_us} avg_us={:.1} avg_bytes={}",
        elapsed_us as f64 / f64::from(iters),
        total_bytes / (iters as usize)
    );
}
