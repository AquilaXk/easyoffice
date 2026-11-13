//! The engine's own PDF (the one written where no page renderer exists) follows the page setup (PD-213): a sheet's paper,
//! orientation, scale, print area and repeated rows, and a document's paper, header, footer and page numbers.

use easyoffice_app::EasyOfficeIpcRouter;

fn pdf(payload: &str) -> Result<String, String> {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload)
        .map_err(|e| e.to_string())?;
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    let bytes = EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap();
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The operands that show `text` as the writer spells them: one operand in one font, or, when the text has Latin-1 signs
/// beyond ASCII, run by run (ASCII and Latin-1 in parentheses, the signs as octal escapes, the rest as UTF-16 hex).
fn operands(text: &str) -> Vec<String> {
    if !text.chars().any(|c| matches!(c, '\u{A0}'..='\u{FF}')) {
        return vec![if text.is_ascii() {
            format!("({text})")
        } else {
            format!(
                "<{}>",
                text.encode_utf16()
                    .map(|u| format!("{u:04X}"))
                    .collect::<String>()
            )
        }];
    }
    let latin = |c: char| matches!(c, ' '..='~' | '\u{A0}'..='\u{FF}');
    let mut out: Vec<String> = Vec::new();
    let mut run = String::new();
    let mut run_latin = false;
    let flush = |run: &mut String, run_latin: bool, out: &mut Vec<String>| {
        if run.is_empty() {
            return;
        }
        out.push(if run_latin {
            let body: String = run
                .chars()
                .map(|c| {
                    if c.is_ascii() {
                        c.to_string()
                    } else {
                        format!("\\{:03o}", c as u32)
                    }
                })
                .collect();
            format!("({body})")
        } else {
            let hex: String = run.encode_utf16().map(|u| format!("{u:04X}")).collect();
            format!("<{hex}>")
        });
        run.clear();
    };
    for c in text.chars() {
        if !run.is_empty() && latin(c) != run_latin {
            flush(&mut run, run_latin, &mut out);
        }
        run_latin = latin(c);
        run.push(c);
    }
    flush(&mut run, run_latin, &mut out);
    out
}

fn media_boxes(pdf: &str) -> Vec<(f64, f64)> {
    pdf.match_indices("/MediaBox [0 0 ")
        .map(|(at, tag)| {
            let rest = &pdf[at + tag.len()..];
            let mut it = rest[..rest.find(']').unwrap()]
                .split_whitespace()
                .map(|n| n.parse::<f64>().unwrap());
            (it.next().unwrap(), it.next().unwrap())
        })
        .collect()
}

/// How many times `text` is shown: its operands one after the other, only text-object and font operators between them.
fn count(pdf: &str, text: &str) -> usize {
    let ops = operands(text);
    pdf.match_indices(&ops[0])
        .filter(|(at, first)| {
            let mut rest = &pdf[at + first.len()..];
            ops[1..].iter().all(|next| match rest.find(next.as_str()) {
                Some(i) if i < 90 => {
                    rest = &rest[i + next.len()..];
                    true
                }
                _ => false,
            })
        })
        .count()
}

fn sheet_payload(page: &str) -> String {
    let cells = (1..=60)
        .flat_map(|r| {
            (0..3).map(move |c| {
                let col = (b'A' + c) as char;
                let raw = if r == 1 {
                    format!("항목{}", c + 1)
                } else {
                    format!("행{r}")
                };
                format!(r#"{{"ref":"{col}{r}","raw":"{raw}"}}"#)
            })
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"pdf","title":"t","sheets":[{{"name":"시트","cells":[{cells}]{page}}}]}}"#
    )
}

#[test]
fn a_sheet_prints_on_its_paper_with_its_area_and_repeated_rows() {
    let page = r#","page":{"landscape":true,"paper":9,"printArea":["A1:C50"],"titleRows":[1,1]}"#;
    let out = pdf(&sheet_payload(page)).unwrap();
    let boxes = media_boxes(&out);
    assert!(boxes.len() >= 2, "{boxes:?}");
    assert!(
        boxes
            .iter()
            .all(|&(w, h)| (w - 841.89).abs() < 0.1 && (h - 595.28).abs() < 0.1),
        "A4 turned: {boxes:?}"
    );
    assert_eq!(
        count(&out, "항목1"),
        boxes.len(),
        "the title row is on every page"
    );
    assert_eq!(count(&out, "행50"), 3, "one per column");
    assert_eq!(count(&out, "행51"), 0, "outside the print area");
}

#[test]
fn fit_to_pages_and_a_percentage_change_the_page_count() {
    let tall = r#","page":{"printArea":["A1:C60"]}"#;
    let plain = media_boxes(&pdf(&sheet_payload(tall)).unwrap()).len();
    assert!(plain >= 2, "{plain}");
    let fit = r#","page":{"printArea":["A1:C60"],"scale":{"fitWidth":1,"fitHeight":1}}"#;
    assert_eq!(
        media_boxes(&pdf(&sheet_payload(fit)).unwrap()).len(),
        1,
        "everything on one page"
    );
    let small = r#","page":{"printArea":["A1:C60"],"scale":{"percent":50}}"#;
    assert!(media_boxes(&pdf(&sheet_payload(small)).unwrap()).len() < plain);
}

#[test]
fn a_sheet_without_a_setup_prints_its_used_range_on_a_portrait_a4() {
    let out = pdf(&sheet_payload("")).unwrap();
    let boxes = media_boxes(&out);
    assert!(
        boxes
            .iter()
            .all(|&(w, h)| (w - 595.28).abs() < 0.1 && (h - 841.89).abs() < 0.1),
        "{boxes:?}"
    );
    assert_eq!(count(&out, "행60"), 3);
}

#[test]
fn headings_and_grid_lines_are_drawn_when_asked_and_not_otherwise() {
    let with = pdf(&sheet_payload(r#","page":{"headings":true,"grid":true}"#)).unwrap();
    assert!(
        count(&with, "A") >= 1 && with.contains(" l S"),
        "column letters and lines"
    );
    let without = pdf(&sheet_payload("")).unwrap();
    assert!(!without.contains(" l S"));
}

#[test]
fn a_print_area_or_page_count_past_the_caps_refuses_the_save() {
    let huge = r#","page":{"printArea":["A1:XFD1048576"]}"#;
    assert!(pdf(&sheet_payload(huge)).is_err());
    let over = r#","page":{"printArea":["A1:A100000","A1:A100000","A1:A2"]}"#;
    assert!(
        pdf(&sheet_payload(over)).is_err(),
        "more than 200,000 printed cells"
    );
}

fn document_payload(setup: &str) -> String {
    let body = (0..150)
        .map(|i| format!(r#"{{"type":"paragraph","runs":[{{"text":"{i}번째 줄 filler"}}]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"sourceFormat":"docx","targetFormat":"pdf","title":"보고서","blocks":[{body}],"header":[{{"type":"paragraph","align":"center","runs":[{{"text":"머리말 글"}}]}}],"footer":[{{"type":"paragraph","align":"center","runs":[{{"text":"- "}},{{"text":"","field":"page"}},{{"text":" / "}},{{"text":"","field":"pages"}},{{"text":" -"}}]}}],{setup}"x":1}}"#
    )
}

#[test]
fn a_document_pdf_has_its_paper_header_footer_and_page_numbers() {
    let setup = r#""pageSetup":{"paper":{"width":16838,"height":11906},"margins":{"top":1984,"bottom":1700,"header":1134,"footer":850,"left":1701,"right":1701}},"#;
    let out = pdf(&document_payload(setup)).unwrap();
    let boxes = media_boxes(&out);
    let pages = boxes.len();
    assert!(pages >= 3, "{pages}");
    assert!(
        boxes
            .iter()
            .all(|&(w, h)| (w - 841.9).abs() < 0.2 && (h - 595.3).abs() < 0.2),
        "landscape A4: {boxes:?}"
    );
    assert_eq!(
        count(&out, "머리말 글"),
        pages,
        "the header is on every page"
    );
    for n in 1..=pages {
        assert_eq!(
            count(&out, &format!("- {n} / {pages} -")),
            1,
            "the footer of page {n}"
        );
    }
    assert_eq!(count(&out, "149번째 줄 filler"), 1);
}

#[test]
fn a_document_with_a_header_but_no_setup_is_a_portrait_a4_with_the_header_and_numbers() {
    let out = pdf(&document_payload("")).unwrap();
    let boxes = media_boxes(&out);
    assert!(
        boxes
            .iter()
            .all(|&(w, h)| (w - 595.28).abs() < 0.2 && (h - 841.89).abs() < 0.2),
        "{boxes:?}"
    );
    assert_eq!(count(&out, "머리말 글"), boxes.len());
    assert_eq!(count(&out, &format!("- 1 / {} -", boxes.len())), 1);
}

#[test]
fn grid_lines_are_drawn_once_per_shared_edge_at_one_width() {
    let out = pdf(&sheet_payload(
        r#","page":{"printArea":["A1:C3"],"grid":true}"#,
    ))
    .unwrap();
    assert_eq!(
        out.matches(" l S").count(),
        (3 + 1) + (3 + 1),
        "four row edges and four column edges"
    );
    assert_eq!(
        out.matches(" w\n").count() + out.matches(" w ").count(),
        1,
        "one stroke width"
    );
    assert_eq!(out.matches("0.5 w").count(), 1);
    let with_headings = pdf(&sheet_payload(
        r#","page":{"printArea":["A1:C3"],"grid":true,"headings":true}"#,
    ))
    .unwrap();
    assert_eq!(
        with_headings.matches(" l S").count(),
        (3 + 2) + (3 + 2),
        "the heading row and column add an edge each"
    );
    let headings_only = pdf(&sheet_payload(
        r#","page":{"printArea":["A1:C3"],"headings":true}"#,
    ))
    .unwrap();
    assert_eq!(
        headings_only.matches(" l S").count(),
        4,
        "the box of the headings"
    );
}

/// Every character of the header and the body is in the page's text, in the font that shows it: the middle dot by the Latin
/// font, the rest by the Korean one (a character outside the Basic Multilingual Plane as its two code units).
#[test]
fn no_character_of_a_header_or_body_is_dropped() {
    let samples = [
        "총무팀 · 대외비",
        "※ 참고 사항",
        "전각 ＊ ① ★ ㈜",
        "é ± ½ ©",
        "웃음 😀 끝",
    ];
    let body = samples
        .iter()
        .map(|t| format!(r#"{{"type":"paragraph","runs":[{{"text":"{t}"}}]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let payload = format!(
        r#"{{"sourceFormat":"docx","targetFormat":"pdf","title":"t","blocks":[{body}],"header":[{{"type":"paragraph","runs":[{{"text":"{}"}}]}}]}}"#,
        samples[0]
    );
    let out = pdf(&payload).unwrap();
    for text in samples {
        assert!(count(&out, text) >= 1, "{text:?} is shown whole");
        for c in text.chars().filter(|c| !c.is_whitespace()) {
            let needle = if c.is_ascii() {
                c.to_string()
            } else if ('\u{A0}'..='\u{FF}').contains(&c) {
                format!("\\{:03o}", c as u32)
            } else {
                c.to_string()
                    .encode_utf16()
                    .map(|u| format!("{u:04X}"))
                    .collect()
            };
            assert!(out.contains(&needle), "{c:?} of {text:?} is missing");
        }
    }
    assert!(out.contains("\\267"), "the middle dot is the Latin font's");
    assert!(
        out.contains("/W [1 [278 278 355 "),
        "a space between Korean words is a Latin space, not half an em"
    );
    assert_eq!(count(&out, samples[0]), 2, "the header and the body line");
}
