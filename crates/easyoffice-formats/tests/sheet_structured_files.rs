//! Structured references in files (PD-225): an XLSX keeps the formula text with the table's name and
//! `#This Row` (what the page wrote as `[@Q1]` too), an ODS holds the cells the reference stands for, the
//! shared formulas and defined names of a file carry them, and hostile text stays bounded.

use easyoffice_formats::{
    CellBox, DefinedName, OdfCodec, OoxmlCodec, SheetStyling, SheetTable, ZipArchiveCodec,
    ZipSecurityPolicy,
};

type Cells = Vec<(u32, u32, String)>;

/// `Sales` is A1:D5 on `Data`: columns Region, Q1, Q2, Calc; data rows 2-4; totals row 5.
fn sales() -> SheetTable {
    let mut t = SheetTable::new("Sales", CellBox::parse("A1:D5").unwrap());
    t.totals = true;
    t.columns = ["Region", "Q1", "Q2", "Calc"].map(String::from).to_vec();
    t
}

fn data(formulas: &[(u32, u32, &str)]) -> Cells {
    let mut cells: Cells = ["Region", "Q1", "Q2", "Calc"]
        .iter()
        .enumerate()
        .map(|(i, h)| (0, i as u32, (*h).to_string()))
        .collect();
    for (row, region) in [(1, "N"), (2, "S"), (3, "E")] {
        cells.push((row, 0, region.to_string()));
        cells.push((row, 1, (row * 10).to_string()));
        cells.push((row, 2, (row * 100).to_string()));
    }
    cells.extend(formulas.iter().map(|&(r, c, f)| (r, c, f.to_string())));
    cells
}

fn styling(tables: Vec<SheetTable>) -> SheetStyling {
    SheetStyling {
        tables,
        ..SheetStyling::default()
    }
}

fn sheet_xml(bytes: &[u8], n: usize) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let path = format!("xl/worksheets/sheet{n}.xml");
    String::from_utf8(
        entries
            .iter()
            .find(|e| e.path == path)
            .unwrap()
            .data
            .clone(),
    )
    .unwrap()
}

fn read(bytes: &[u8]) -> Vec<(String, Cells)> {
    OoxmlCodec::unpack_xlsx_multi(bytes).unwrap()
}

fn formula_at(cells: &Cells, row: u32, col: u32) -> &str {
    cells
        .iter()
        .find(|c| (c.0, c.1) == (row, col))
        .map_or("", |c| c.2.as_str())
}

#[test]
fn xlsx_writes_the_files_spelling_and_reads_it_back_unchanged() {
    let cells = data(&[
        (5, 1, "=SUM(Sales[Q1])"),
        (1, 3, "=[@Q1]*2"),
        (2, 3, "=Sales[@[Q1]:[Q2]]"),
        (3, 3, "=SUM(Sales[[#Headers],[#Data],[Q1]])"),
        (6, 0, "=SUM(Sales[#This Row])"),
        (7, 0, "=\"Sales[@Q1]\"&Sales[[#Totals],[Q2]]"),
        (8, 0, "=SUM([@Q1])"),
    ]);
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Data".to_string(), cells)],
        &[],
        &[styling(vec![sales()])],
    )
    .unwrap();
    let xml = sheet_xml(&bytes, 1);
    assert!(xml.contains("<f>SUM(Sales[Q1])</f>"), "{xml}");
    assert!(xml.contains("<f>Sales[[#This Row],[Q1]]*2</f>"), "{xml}");
    assert!(xml.contains("<f>Sales[[#This Row],[Q1]:[Q2]]</f>"), "{xml}");
    assert!(xml.contains("[[#Headers],[#Data],[Q1]]"), "{xml}");
    // A reference that names its table stays as written; a string literal is not a reference.
    assert!(xml.contains("<f>SUM(Sales[#This Row])</f>"), "{xml}");
    assert!(
        xml.contains("&quot;Sales[@Q1]&quot;&amp;Sales[[#Totals],[Q2]]"),
        "{xml}"
    );
    // Outside the table an unqualified reference has no table to name.
    assert!(xml.contains("<f>SUM(#REF!)</f>"), "{xml}");
    let back = read(&bytes);
    assert_eq!(formula_at(&back[0].1, 5, 1), "=SUM(Sales[Q1])");
    assert_eq!(formula_at(&back[0].1, 1, 3), "=Sales[[#This Row],[Q1]]*2");
    // A second round does not change the text.
    let again = OoxmlCodec::pack_xlsx_multi_styled(&back, &[], &[styling(vec![sales()])]).unwrap();
    assert_eq!(read(&again)[0].1, back[0].1);
}

#[test]
fn ods_holds_the_cells_a_reference_stands_for() {
    let cells = data(&[
        (5, 1, "=SUM(Sales[Q1])"),
        (1, 3, "=[@Q1]*2"),
        (2, 3, "=Sales[@[Q1]:[Q2]]"),
        (6, 0, "=SUM(Sales[Nope])"),
        (6, 1, "=SUM(Sales[[#This Row],[Q1]])"),
    ]);
    let other = vec![
        (0, 0, "=SUM(Sales[Q2])".to_string()),
        (1, 0, "=ROWS(Sales[#All])".to_string()),
    ];
    let sheets = vec![("Data".to_string(), cells), ("My Sheet".to_string(), other)];
    let stylings = [styling(vec![sales()]), SheetStyling::default()];
    let ods = OdfCodec::pack_ods_multi_styled(&sheets, &stylings).unwrap();
    let back = OdfCodec::unpack_ods_multi(&ods).unwrap();
    let data_back = &back[0].1;
    assert_eq!(formula_at(data_back, 5, 1), "=SUM($B$2:$B$4)");
    assert_eq!(formula_at(data_back, 1, 3), "=$B2*2");
    assert_eq!(formula_at(data_back, 2, 3), "=$B3:$C3");
    assert_eq!(formula_at(data_back, 6, 0), "=SUM(#REF!)");
    // Row 7 is outside the table's data rows: the reference is an error, not another row's cell.
    assert_eq!(formula_at(data_back, 6, 1), "=SUM(#VALUE!)");
    assert_eq!(formula_at(&back[1].1, 0, 0), "=SUM(Data!$C$2:$C$4)");
    assert_eq!(formula_at(&back[1].1, 1, 0), "=ROWS(Data!$A$1:$D$5)");
}

#[test]
fn defined_names_with_a_reference_become_ranges_in_ods_and_stay_in_xlsx() {
    let names = vec![DefinedName {
        name: "Qs".into(),
        scope: None,
        refers_to: "Sales[Q1]".into(),
        comment: String::new(),
        hidden: false,
    }];
    let st = SheetStyling {
        tables: vec![sales()],
        defined_names: names,
        ..SheetStyling::default()
    };
    let sheets = vec![("Data".to_string(), data(&[]))];
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], std::slice::from_ref(&st)).unwrap();
    let (_, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    assert_eq!(stylings[0].1.defined_names[0].refers_to, "Sales[Q1]");
    let ods = OdfCodec::pack_ods_multi_styled(&sheets, &[st]).unwrap();
    let (_, back) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
    let text = &back[0].1.defined_names[0].refers_to;
    assert!(
        text.contains("$B$2:$B$4") || text.contains("B2:B4"),
        "{text}"
    );
}

#[test]
fn hostile_reference_text_is_written_as_it_is_or_as_an_error() {
    let deep = format!("=SUM(Sales{}Q1{})", "[".repeat(5000), "]".repeat(5000));
    let escapes = format!("=SUM(Sales[{}])", "'['".repeat(3000));
    let many = format!("=SUM({})", "Sales[Q1],".repeat(700));
    let cells = data(&[
        (5, 1, deep.as_str()),
        (6, 1, escapes.as_str()),
        (7, 1, many.as_str()),
    ]);
    let sheets = vec![("Data".to_string(), cells)];
    let stylings = [styling(vec![sales()])];
    OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).unwrap();
    let ods = OdfCodec::pack_ods_multi_styled(&sheets, &stylings).unwrap();
    let back = OdfCodec::unpack_ods_multi(&ods).unwrap();
    assert!(formula_at(&back[0].1, 7, 1).starts_with("=SUM($B$2:$B$4,"));
}

#[test]
fn shared_formulas_keep_structured_references_unshifted() {
    // A master with a relative cell and a structured reference; the follower moves the cell only.
    let sheet = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f t="shared" ref="A1:A3" si="0">B1+SUM(Sales[Q1])</f><v>1</v></c></row><row r="2"><c r="A2"><f t="shared" si="0"/><v>2</v></c></row></sheetData></worksheet>"#;
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(
        &[("Data".to_string(), vec![(0, 0, "x".to_string())])],
        &[],
        &[SheetStyling::default()],
    )
    .unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let parts: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "xl/worksheets/sheet1.xml" {
                (e.path, sheet.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let patched = ZipArchiveCodec::pack(&parts).unwrap();
    let back = read(&patched);
    assert_eq!(formula_at(&back[0].1, 0, 0), "=B1+SUM(Sales[Q1])");
    assert_eq!(formula_at(&back[0].1, 1, 0), "=B2+SUM(Sales[Q1])");
}
