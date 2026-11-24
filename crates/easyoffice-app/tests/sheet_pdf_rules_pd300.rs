//! The engine's PDF of a sheet draws what the conditional formats give its cells (PD-300). The engine evaluates the rules
//! (the code the grid and the print read) and the PDF only draws the answer.

use easyoffice_app::EasyOfficeIpcRouter;

fn convert_bytes(sheet: &str) -> (Vec<u8>, String) {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"pdf","title":"t","sheets":[{{"name":"s",{sheet}}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    (
        EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap(),
        json,
    )
}

fn convert(sheet: &str) -> (String, String) {
    let (bytes, json) = convert_bytes(sheet);
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
        .filter_map(|line| line.strip_prefix(&head)?.strip_suffix(" f Q"))
        .flat_map(|path| {
            let words: Vec<&str> = path.split(' ').collect();
            words
                .chunks(5)
                .map(|c| {
                    let n = |i: usize| c[i].parse::<f64>().unwrap();
                    (n(0), n(1), n(2), n(3))
                })
                .collect::<Vec<_>>()
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
        rows.windows(2).all(|w| (w[0] - w[1] - 16.5).abs() < 0.011),
        "{rows:?}"
    );
    // The three cells draw their text in the rule's color, bold, under one graphics state.
    let state = format!("q {} rg {} RG 2 Tr", rgb(DARK_RED), rgb(DARK_RED));
    assert_eq!(pdf.matches(&state).count(), 1, "{pdf}");
    let at = pdf.find(&state).unwrap();
    let run = &pdf[at..at + pdf[at..].find(" Q").unwrap()];
    assert_eq!(run.matches(" Tj").count(), 3, "{run}");
    // The cell A1 does not match: its text is plain and comes before the run.
    assert!(pdf.find("(5) Tj").unwrap() < at);
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
fn a_bar_or_icon_set_that_shows_no_value_hides_it_and_icon_set_shows_it_by_default() {
    let bar = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:A3","p":1,"t":"bar","min":{{"k":"num","v":"0"}},"max":{{"k":"max","v":""}},"show":false}}]}}"##,
        cells(&[("A1", "10"), ("A2", "20"), ("A3", "40")])
    );
    let pdf = convert(&bar).0;
    assert!(!pdf.contains("(10) Tj") && !pdf.contains("(20) Tj") && !pdf.contains("(40) Tj"));
    let icons_shown = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:A3","p":1,"t":"icons","set":"3Arrows","th":[{{"k":"num","v":"0"}},{{"k":"num","v":"15"}},{{"k":"num","v":"30"}}],"show":true}}]}}"##,
        cells(&[("A1", "10"), ("A2", "20"), ("A3", "40")])
    );
    let pdf_shown = convert(&icons_shown).0;
    assert!(
        pdf_shown.contains("(10) Tj")
            && pdf_shown.contains("(20) Tj")
            && pdf_shown.contains("(40) Tj")
    );
    assert!(
        pdf_shown.contains("/Ic1 Do")
            && pdf_shown.contains("/Ic2 Do")
            && pdf_shown.contains("/Ic3 Do")
    );
    let icons_hidden = format!(
        r##"{},"rules":{{"cf":[{{"r":"A1:A3","p":1,"t":"icons","set":"3Arrows","th":[{{"k":"num","v":"0"}},{{"k":"num","v":"15"}},{{"k":"num","v":"30"}}],"show":false}}]}}"##,
        cells(&[("A1", "10"), ("A2", "20"), ("A3", "40")])
    );
    let pdf_hidden = convert(&icons_hidden).0;
    assert!(
        !pdf_hidden.contains("(10) Tj")
            && !pdf_hidden.contains("(20) Tj")
            && !pdf_hidden.contains("(40) Tj")
    );
    assert!(
        pdf_hidden.contains("/Ic1 Do")
            && pdf_hidden.contains("/Ic2 Do")
            && pdf_hidden.contains("/Ic3 Do")
    );
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

fn column_name(col: u32) -> String {
    let letter = |n: u32| char::from(b'A' + u8::try_from(n).unwrap());
    if col < 26 {
        letter(col).to_string()
    } else {
        format!("{}{}", letter(col / 26 - 1), letter(col % 26))
    }
}

/// The bytes of the PDF of a 200 x 100 sheet (20,000 cells) whose values come from `value(row, col)`, without and with
/// rules: a fill, bold colored text and an edge on every cell over 50, and a gradient bar in the first ten columns.
fn big_sheet_sizes(value: impl Fn(u32, u32) -> u32) -> (usize, usize) {
    let raws: Vec<String> = (0..200u32)
        .flat_map(|r| {
            let value = &value;
            (0..100u32).map(move |c| {
                format!(
                    r#"{{"ref":"{}{}","raw":"{}"}}"#,
                    column_name(c),
                    r + 1,
                    value(r, c)
                )
            })
        })
        .collect();
    let cells = format!(r#""cells":[{}]"#, raws.join(","));
    let rules = r##","rules":{"cf":[
      {"r":"A1:CV200","p":1,"t":"cellIs","op":"greaterThan","a":"50","bg":"#FFC7CE","fg":"#9C0006","fl":1,"bd":{"b":"thin:9c0006"}},
      {"r":"A1:J200","p":2,"t":"bar","min":{"k":"num","v":"0"},"max":{"k":"num","v":"100"},"pos":"#638EC6","grad":true}]}"##;
    let (plain, _) = convert_bytes(&cells);
    let (ruled, _) = convert_bytes(&format!("{cells}{rules}"));
    assert!(
        String::from_utf8_lossy(&ruled).contains("/FlateDecode"),
        "a page this big is deflated"
    );
    (plain.len(), ruled.len())
}

#[test]
fn rules_on_blocks_of_a_big_sheet_make_the_file_no_more_than_half_as_big_again() {
    // The cells over 50 are bands of ten columns, as a rule over a region of a sheet makes them.
    let (plain, ruled) = big_sheet_sizes(|_, c| if (c / 10) % 2 == 0 { 20 } else { 80 });
    assert!(
        ruled * 2 <= plain * 3,
        "{ruled} bytes with the rules, {plain} without"
    );
}

#[test]
fn rules_on_scattered_cells_of_a_big_sheet_make_the_file_less_than_twice_as_big() {
    // The worst case for the paths and runs the PDF collects: the cells over 50 are scattered, so no two neighbours
    // share a box, an edge or a text state.
    let (plain, ruled) = big_sheet_sizes(|r, c| ((r + 1) * 7 + c * 13) % 100);
    assert!(
        ruled < plain * 2,
        "{ruled} bytes with the rules, {plain} without"
    );
}
