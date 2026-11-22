//! The engine's PDF of a sheet draws what the conditional formats give its cells (PD-300). The engine evaluates the rules
//! (the code the grid and the print read) and the PDF only draws the answer.

use easyoffice_app::EasyOfficeIpcRouter;

fn convert(sheet: &str) -> (String, String) {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"pdf","title":"t","sheets":[{{"name":"s",{sheet}}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    let bytes = EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap();
    (String::from_utf8_lossy(&bytes).into_owned(), json)
}

fn rgb(value: u32) -> String {
    let c = |shift: u32| f64::from((value >> shift) & 0xFF) / 255.0;
    format!("{:.3} {:.3} {:.3}", c(16), c(8), c(0))
}

/// `(x, y, w, h)` of every rectangle filled in `color`, top row first.
fn fills(pdf: &str, color: u32) -> Vec<(f64, f64, f64, f64)> {
    let head = format!("q {} rg ", rgb(color));
    let mut found: Vec<(f64, f64, f64, f64)> = pdf
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix(&head)?.strip_suffix(" re f Q")?;
            let n: Vec<f64> = rest.split(' ').map(|v| v.parse().unwrap()).collect();
            Some((n[0], n[1], n[2], n[3]))
        })
        .collect();
    found.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.total_cmp(&b.0)));
    found
}

const PINK: u32 = 0xFFC7CE;
const DARK_RED: u32 = 0x9C0006;

fn cells(raws: &[(&str, &str)]) -> String {
    let list: Vec<String> = raws
        .iter()
        .map(|(cell, raw)| format!(r#"{{"ref":"{cell}","raw":"{raw}"}}"#))
        .collect();
    format!(r#""cells":[{}]"#, list.join(","))
}

#[test]
fn a_rule_paints_the_cells_it_matches_with_the_values_the_engine_computes() {
    let sheet = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:A4","p":1,"t":"cellIs","op":"greaterThan","a":"10","bg":"#FFC7CE","fg":"#9C0006","fl":1}}]}}"##,
        cells(&[("A1", "5"), ("A2", "15"), ("A3", "25"), ("A4", "=A3*2")])
    );
    let (pdf, _) = convert(&sheet);
    let filled = fills(&pdf, PINK);
    assert_eq!(
        filled.len(),
        3,
        "A2, A3 and A4 (a formula that gives 50), not A1"
    );
    let rows: Vec<f64> = filled.iter().map(|f| f.1).collect();
    assert!(
        rows.windows(2).all(|w| (w[0] - w[1] - 18.0).abs() < 0.011),
        "{rows:?}"
    );
    let red = format!("q {} rg", rgb(DARK_RED));
    assert_eq!(
        pdf.lines()
            .filter(|l| l.starts_with(&red) && l.contains("2 Tr"))
            .count(),
        3
    );
    // The first row is above the first filled one: the row of A1 is not painted.
    let one_up = pdf.lines().any(|l| l.contains("(5) Tj"));
    assert!(one_up);
}

#[test]
fn a_rule_with_a_formula_paints_empty_cells_inside_the_printed_range() {
    let sheet = format!(
        r##"{},"rules":{{"cf":[{{"r":"B1:B4","p":1,"t":"expr","formula":"$A1>20","bg":"#FFC7CE"}}]}}"##,
        cells(&[
            ("A1", "5"),
            ("A2", "15"),
            ("A3", "25"),
            ("A4", "35"),
            ("B1", "x")
        ])
    );
    let filled = fills(&convert(&sheet).0, PINK);
    assert_eq!(filled.len(), 2, "B3 and B4: empty cells with a rule fill");
}

#[test]
fn a_data_bar_is_as_wide_as_the_value_says() {
    let sheet = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:A3","p":1,"t":"bar","min":{{"k":"num","v":"0"}},"max":{{"k":"max","v":""}},"pos":"#638EC6","grad":false}}]}}"##,
        cells(&[("A1", "10"), ("A2", "20"), ("A3", "40")])
    );
    let bars = fills(&convert(&sheet).0, 0x638EC6);
    assert_eq!(bars.len(), 3);
    let widths: Vec<f64> = bars.iter().map(|b| b.2).collect();
    assert!(widths[0] < widths[1] && widths[1] < widths[2], "{widths:?}");
    assert!(
        widths[2] <= 72.011,
        "the longest bar is the cell: {widths:?}"
    );
    assert!(
        (widths[1] / widths[0] - 2.0).abs() < 0.3,
        "20 is twice 10: {widths:?}"
    );
}

#[test]
fn a_bar_that_shows_no_value_hides_it_and_an_icon_set_keeps_the_value() {
    let bar = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:A3","p":1,"t":"bar","min":{{"k":"num","v":"0"}},"max":{{"k":"max","v":""}},"show":false}}]}}"##,
        cells(&[("A1", "10"), ("A2", "20"), ("A3", "40")])
    );
    let pdf = convert(&bar).0;
    assert!(!pdf.contains("(10) Tj") && !pdf.contains("(20) Tj") && !pdf.contains("(40) Tj"));
    let icons = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:A3","p":1,"t":"icons","set":"3Arrows","th":[{{"k":"num","v":"0"}},{{"k":"num","v":"15"}},{{"k":"num","v":"30"}}],"show":false}}]}}"##,
        cells(&[("A1", "10"), ("A2", "20"), ("A3", "40")])
    );
    let pdf = convert(&icons).0;
    assert!(pdf.contains("(10) Tj") && pdf.contains("(20) Tj") && pdf.contains("(40) Tj"));
}

#[test]
fn the_edges_of_a_rule_are_drawn() {
    let sheet = format!(
        r##"{},"rules":{{"cf":[{{"r":"A2","p":1,"t":"cellIs","op":"greaterThan","a":"10","bd":{{"b":"thin:ff0000","r":"dashed:0000ff"}}}}]}}"##,
        cells(&[("A1", "5"), ("A2", "15")])
    );
    let pdf = convert(&sheet).0;
    assert_eq!(
        pdf.lines()
            .filter(|l| l.contains("1.000 0.000 0.000 RG"))
            .count(),
        1
    );
    assert_eq!(
        pdf.lines()
            .filter(|l| l.contains("0.000 0.000 1.000 RG") && l.contains("[3.00 2.00] 0 d"))
            .count(),
        1
    );
}

#[test]
fn a_rule_over_a_whole_sheet_draws_only_what_the_page_prints() {
    let sheet = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:XFD1048576","p":1,"t":"cellIs","op":"greaterThan","a":"0","bg":"#FFC7CE"}},{{"r":"A1:XFD1048576","p":2,"t":"blanks","bg":"#00B050"}}]}}"##,
        cells(&[("A1", "5"), ("A2", "7"), ("C9", "-3")])
    );
    let (pdf, json) = convert(&sheet);
    assert_eq!(fills(&pdf, PINK).len(), 2, "A1 and A2");
    // A rule over the whole sheet that matches the empty cells does not scan them (the grid does not either).
    assert!(fills(&pdf, 0x00B050).is_empty());
    assert!(!json.contains("조건부 서식"), "{json}");
}

#[test]
fn a_sheet_without_rules_is_drawn_as_before() {
    let plain = convert(&cells(&[("A1", "5"), ("A2", "15")])).0;
    let empty_rules = convert(&format!(
        r#"{},"rules":{{"cf":[]}}"#,
        cells(&[("A1", "5"), ("A2", "15")])
    ))
    .0;
    assert_eq!(plain, empty_rules);
    assert!(fills(&plain, PINK).is_empty());
}
