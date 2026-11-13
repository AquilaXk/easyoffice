//! The defined names of a workbook (scope, hidden, comment, constants and formulas) survive XLSX and ODS
//! round trips, and a hostile file cannot make a reader build an unbounded list (PD-214).

use easyoffice_formats::{
    DefinedName, OdfCodec, OoxmlCodec, SheetStyling, ZipArchiveCodec, ZipSecurityPolicy, MAX_NAMES,
};

fn def(name: &str, scope: Option<&str>, refers_to: &str) -> DefinedName {
    DefinedName {
        name: name.to_string(),
        scope: scope.map(str::to_string),
        refers_to: refers_to.to_string(),
        ..DefinedName::default()
    }
}

fn names() -> Vec<DefinedName> {
    vec![
        def("Prices", None, "Sheet1!$A$1:$A$3"),
        def("One", None, "'My Data'!$B$2"),
        DefinedName {
            comment: "tax & \"fees\" <5>".to_string(),
            hidden: true,
            ..def("Margin", None, "0.25")
        },
        def("Greeting", None, "\"hi & bye\""),
        def("Total", None, "SUM(Prices)*(1+Margin)"),
        def("Local", Some("My Data"), "'My Data'!$A$1:$C$9"),
        def("Local", Some("Sheet1"), "Sheet1!$D$1"),
    ]
}

type Sheet = (String, Vec<(u32, u32, String)>);

fn sheets() -> Vec<Sheet> {
    ["Sheet1", "My Data"]
        .iter()
        .map(|n| ((*n).to_string(), vec![(0u32, 0u32, "1".to_string())]))
        .collect()
}

fn stylings(names: Vec<DefinedName>) -> Vec<SheetStyling> {
    vec![
        SheetStyling {
            defined_names: names,
            ..SheetStyling::default()
        },
        SheetStyling::default(),
    ]
}

fn read_xlsx(bytes: &[u8]) -> (Vec<DefinedName>, u32) {
    let first = OoxmlCodec::unpack_xlsx_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1;
    (first.defined_names, first.names_rejected)
}

fn read_ods(bytes: &[u8]) -> (Vec<DefinedName>, u32) {
    let first = OdfCodec::unpack_ods_with_styling(bytes)
        .unwrap()
        .1
        .remove(0)
        .1;
    (first.defined_names, first.names_rejected)
}

fn sorted(mut v: Vec<DefinedName>) -> Vec<DefinedName> {
    v.sort_by(|a, b| (&a.name, &a.scope).cmp(&(&b.name, &b.scope)));
    v
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

fn replace_part(bytes: &[u8], path: &str, xml: &str) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let out: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == path {
                (e.path, xml.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&out).unwrap()
}

#[test]
fn names_round_trip_through_xlsx_and_a_second_write_is_the_same() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &stylings(names())).unwrap();
    let book = part(&bytes, "xl/workbook.xml");
    assert!(
        book.contains("<definedName name=\"Prices\">Sheet1!$A$1:$A$3</definedName>"),
        "{book}"
    );
    assert!(
        book.contains(
            "name=\"Local\" localSheetId=\"1\">&apos;My Data&apos;!$A$1:$C$9</definedName>"
        ),
        "{book}"
    );
    assert!(book.contains("hidden=\"1\">0.25</definedName>"), "{book}");
    let (read, rejected) = read_xlsx(&bytes);
    assert_eq!(rejected, 0);
    assert_eq!(sorted(read.clone()), sorted(names()));
    let again = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &stylings(read)).unwrap();
    assert_eq!(part(&again, "xl/workbook.xml"), book);
}

#[test]
fn names_round_trip_through_ods() {
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets(), &stylings(names())).unwrap();
    let content = part(&bytes, "content.xml");
    assert!(
        content.contains("<table:named-range table:name=\"Prices\""),
        "{content}"
    );
    assert!(
        content.contains("<table:named-expression table:name=\"Total\""),
        "{content}"
    );
    let (read, rejected) = read_ods(&bytes);
    assert_eq!(rejected, 0);
    assert_eq!(sorted(read), sorted(names()));
}

#[test]
fn a_workbook_without_names_writes_none() {
    let x = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &stylings(Vec::new())).unwrap();
    assert!(!part(&x, "xl/workbook.xml").contains("definedName"));
    let o = OdfCodec::pack_ods_multi_styled(&sheets(), &stylings(Vec::new())).unwrap();
    assert!(!part(&o, "content.xml").contains("named-expressions"));
}

#[test]
fn a_scope_that_is_no_sheet_is_not_written() {
    let stray = vec![def("Stray", Some("Nowhere"), "1"), def("Kept", None, "2")];
    let x = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &stylings(stray)).unwrap();
    assert_eq!(read_xlsx(&x).0, vec![def("Kept", None, "2")]);
}

#[test]
fn a_hostile_xlsx_list_is_cut_at_the_limit_and_counted() {
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets(), &[], &stylings(Vec::new())).unwrap();
    let mut book = part(&bytes, "xl/workbook.xml");
    let mut inner = String::from("<definedNames>");
    for i in 0..MAX_NAMES + 50 {
        inner.push_str(&format!("<definedName name=\"n_{i}\">{i}</definedName>"));
    }
    inner.push_str(&format!(
        "<definedName name=\"huge\">{}</definedName></definedNames>",
        "1+".repeat(5_000)
    ));
    book = book.replace("<calcPr", &format!("{inner}<calcPr"));
    let (read, rejected) = read_xlsx(&replace_part(&bytes, "xl/workbook.xml", &book));
    assert_eq!(read.len(), MAX_NAMES);
    assert_eq!(rejected, 51);
}

#[test]
fn a_hostile_ods_list_is_cut_at_the_limit_and_counted() {
    let bytes = OdfCodec::pack_ods_multi_styled(&sheets(), &stylings(Vec::new())).unwrap();
    let content = part(&bytes, "content.xml");
    let mut inner = String::from("<table:named-expressions>");
    for i in 0..MAX_NAMES + 20 {
        inner.push_str(&format!(
            "<table:named-expression table:name=\"n_{i}\" table:expression=\"of:={i}\"/>"
        ));
    }
    inner.push_str("</table:named-expressions></office:spreadsheet>");
    let hostile = content.replace("</office:spreadsheet>", &inner);
    let (read, rejected) = read_ods(&replace_part(&bytes, "content.xml", &hostile));
    assert_eq!(read.len(), MAX_NAMES);
    assert_eq!(rejected, 20);
}
