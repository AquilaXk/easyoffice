//! The totals row of a table is written as the function list's attributes of the table columns, derived from the
//! cells, and read back without loss; hostile attributes and a huge formula are read to a bound (PD-232).

use easyoffice_formats::{
    CellBox, OoxmlCodec, SheetStyling, SheetTable, ZipArchiveCodec, ZipSecurityPolicy,
};

type Cells = Vec<(u32, u32, String)>;

fn totals_table() -> (Cells, SheetTable) {
    let mut t = SheetTable::new("Sales", CellBox::parse("A1:F5").unwrap());
    t.totals = true;
    let names = ["Region", "Q1", "Q2", "Q3", "Q4", "Note"];
    let mut cells: Cells = names
        .iter()
        .enumerate()
        .map(|(c, n)| (0, c as u32, (*n).to_string()))
        .collect();
    for r in 1..4u32 {
        cells.push((r, 0, format!("r{r}")));
        for c in 1..5u32 {
            cells.push((r, c, (r * c).to_string()));
        }
        cells.push((r, 5, "x".to_string()));
    }
    cells.extend([
        (4, 0, "요약".to_string()),
        (4, 1, "=SUBTOTAL(109,Sales[Q1])".to_string()),
        (4, 2, "=SUBTOTAL(101,Sales[Q2])".to_string()),
        (4, 3, "=SUM(Sales[Q3])*2".to_string()),
        (4, 4, "=SUBTOTAL(9,Sales[Q4])".to_string()),
        (4, 5, "=SUBTOTAL(103,Sales[Note])".to_string()),
    ]);
    t.columns = names.iter().map(|n| (*n).to_string()).collect();
    (cells, t)
}

fn pack(cells: &Cells, t: &SheetTable) -> Vec<u8> {
    let styling = SheetStyling {
        tables: vec![t.clone()],
        ..SheetStyling::default()
    };
    OoxmlCodec::pack_xlsx_multi_styled(&[("S".to_string(), cells.clone())], &[], &[styling])
        .unwrap()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
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

#[test]
fn the_columns_carry_the_function_the_cells_hold() {
    let (cells, t) = totals_table();
    let xml = part(&pack(&cells, &t), "xl/tables/table1.xml");
    assert!(xml.contains("totalsRowCount=\"1\""), "{xml}");
    for expected in [
        "<tableColumn id=\"1\" name=\"Region\" totalsRowLabel=\"요약\"/>",
        "<tableColumn id=\"2\" name=\"Q1\" totalsRowFunction=\"sum\"/>",
        "<tableColumn id=\"3\" name=\"Q2\" totalsRowFunction=\"average\"/>",
        "<tableColumn id=\"4\" name=\"Q3\" totalsRowFunction=\"custom\"><totalsRowFormula>SUM(Sales[Q3])*2</totalsRowFormula></tableColumn>",
        "<tableColumn id=\"5\" name=\"Q4\" totalsRowFunction=\"custom\"><totalsRowFormula>SUBTOTAL(9,Sales[Q4])</totalsRowFormula></tableColumn>",
        "<tableColumn id=\"6\" name=\"Note\" totalsRowFunction=\"count\"/>",
    ] {
        assert!(xml.contains(expected), "{expected}\n{xml}");
    }
}

#[test]
fn a_table_without_a_totals_row_writes_none_of_them() {
    let (cells, mut t) = totals_table();
    t.totals = false;
    t.range = CellBox::parse("A1:F4").unwrap();
    let xml = part(&pack(&cells, &t), "xl/tables/table1.xml");
    assert!(
        !xml.contains("totalsRow") || xml.contains("totalsRowShown"),
        "{xml}"
    );
    assert!(
        !xml.contains("totalsRowFunction") && !xml.contains("totalsRowLabel"),
        "{xml}"
    );
}

#[test]
fn the_attributes_survive_a_read_and_a_write() {
    let (cells, t) = totals_table();
    let first = pack(&cells, &t);
    let (sheets, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&first).unwrap();
    let cells_back: Cells = sheets
        .into_iter()
        .next()
        .map(|(_, c)| c)
        .expect("one sheet");
    let again = pack(&cells_back, &stylings[0].1.tables[0]);
    assert_eq!(
        part(&first, "xl/tables/table1.xml"),
        part(&again, "xl/tables/table1.xml")
    );
}

#[test]
fn a_formula_over_the_limit_and_a_long_label_are_not_written() {
    let (mut cells, t) = totals_table();
    cells.retain(|(r, c, _)| !(*r == 4 && (*c == 3 || *c == 0)));
    cells.push((4, 3, format!("=SUM({})", "A1,".repeat(9_000))));
    cells.push((4, 0, "x".repeat(300)));
    let xml = part(&pack(&cells, &t), "xl/tables/table1.xml");
    assert!(
        xml.contains("<tableColumn id=\"4\" name=\"Q3\"/>"),
        "{}",
        &xml[..xml.len().min(800)]
    );
    assert!(xml.contains("<tableColumn id=\"1\" name=\"Region\"/>"));
    assert!(
        xml.len() < 4_000,
        "the hostile text stays out: {}",
        xml.len()
    );
}

#[test]
fn hostile_attributes_and_a_huge_totals_formula_in_a_file_are_read_to_a_bound() {
    let (cells, t) = totals_table();
    let bytes = pack(&cells, &t);
    let huge = "A1+".repeat(700_000);
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let parts: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let text = String::from_utf8_lossy(&e.data).into_owned();
            let text = if e.path == "xl/tables/table1.xml" {
                text.replace(
                    "totalsRowFunction=\"sum\"",
                    "totalsRowFunction=\"&lt;&amp;&gt;999\" totalsRowLabel=\"&#xFFFF;\"",
                )
                .replace(
                    "</tableColumn>",
                    &format!("<totalsRowFormula>{huge}</totalsRowFormula></tableColumn>"),
                )
            } else {
                text
            };
            (e.path.clone(), text.into_bytes())
        })
        .collect();
    let hostile = ZipArchiveCodec::pack(&parts).unwrap();
    // The 2 MB formula text is skipped and the table is read as it is, the cells holding the totals.
    let (_, stylings) = OoxmlCodec::unpack_xlsx_with_styling(&hostile).unwrap();
    let read = &stylings[0].1;
    assert_eq!(read.tables.len(), 1);
    assert!(read.tables[0].totals);
    assert_eq!(read.tables[0].columns[1], "Q1");
}

#[test]
fn a_custom_formula_is_spelled_as_the_cell_spells_it_and_a_cell_of_only_an_equal_sign_names_no_function(
) {
    let (mut cells, t) = totals_table();
    cells.retain(|(r, c, _)| !(*r == 4 && matches!(*c, 3 | 4 | 5)));
    cells.push((4, 3, "=LET(x,1,x+1)".to_string()));
    cells.push((4, 4, "=XLOOKUP(1,B2:B4,C2:C4)".to_string()));
    cells.push((4, 5, "=".to_string()));
    let bytes = pack(&cells, &t);
    let xml = part(&bytes, "xl/tables/table1.xml");
    for expected in [
        "<tableColumn id=\"4\" name=\"Q3\" totalsRowFunction=\"custom\"><totalsRowFormula>_xlfn.LET(_xlpm.x,1,_xlpm.x+1)</totalsRowFormula></tableColumn>",
        "<tableColumn id=\"5\" name=\"Q4\" totalsRowFunction=\"custom\"><totalsRowFormula>_xlfn.XLOOKUP(1,B2:B4,C2:C4)</totalsRowFormula></tableColumn>",
        "<tableColumn id=\"6\" name=\"Note\"/>",
    ] {
        assert!(xml.contains(expected), "{expected}\n{xml}");
    }
    // The cell of the sheet holds the same text.
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("_xlfn.LET(_xlpm.x,1,_xlpm.x+1)"), "{sheet}");
    assert!(sheet.contains("_xlfn.XLOOKUP(1,B2:B4,C2:C4)"), "{sheet}");
}
