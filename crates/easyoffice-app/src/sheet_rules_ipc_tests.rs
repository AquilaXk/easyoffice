//! Rules over the IPC commands: session `rules`/`validate`, the `cf` answers of `apply`, and the
//! save/open round trip of the page's rule JSON (PD-165).

use super::*;
use easyoffice_formats::StrictJsonValue;
use easyoffice_sheet::{decode_visuals, CellVisual, PackedVisuals};

fn dispatch(command: &str, payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch(command, payload).unwrap()
}

fn session_call(body: &str) -> String {
    dispatch("evaluate_sheet", body)
}

fn open() -> String {
    let res = session_call("{\"sessionAction\":\"open\",\"sheets\":[\"S\"]}");
    let start = res.find("\"session\":\"").unwrap() + 11;
    res[start..start + res[start..].find('"').unwrap()].to_string()
}

fn apply(id: &str, changes: &str) -> String {
    session_call(&format!(
        "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{changes}]}}"
    ))
}

fn cell(a1: &str, value: &str) -> String {
    format!("{{\"sheet\":\"S\",\"cell\":\"{a1}\",\"value\":\"{value}\"}}")
}

const RULES: &str = r##"{"cf":[
  {"r":"A1:A9","p":1,"t":"cellIs","op":"greaterThan","a":"10","bg":"#FFC7CE","fg":"#9C0006"},
  {"r":"B1:B9","p":2,"t":"bar","min":{"k":"num","v":"0"},"max":{"k":"max","v":""}}
 ],
 "dv":[{"r":"C1:C9","t":"whole","op":"between","a":"1","b":"5","alert":"warning","err":"1~5"},
       {"r":"D1:D9","t":"list","a":"가,나"}]}"##;

fn unpack(json: &str) -> Vec<CellVisual> {
    // `"pal":[..],"w":"<base64>"` of an answer or of one `cf` entry.
    let pal_start = json.find("\"pal\":[").unwrap() + 7;
    let pal_end = pal_start + json[pal_start..].find(']').unwrap();
    let palette: Vec<u32> = json[pal_start..pal_end]
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    let w_start = json.find("\"w\":\"").unwrap() + 5;
    let w = &json[w_start..w_start + json[w_start..].find('"').unwrap()];
    let bytes = EasyOfficeIpcRouter::decode_base64(w).unwrap();
    let words = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    decode_visuals(&PackedVisuals { palette, words })
}

fn a1(v: &CellVisual) -> String {
    v.coord.to_a1()
}

#[test]
fn a_session_formats_cells_and_reports_only_what_an_edit_changed() {
    let id = open();
    apply(
        &id,
        &[
            cell("A1", "5"),
            cell("A2", "50"),
            cell("B1", "0"),
            cell("B2", "10"),
        ]
        .join(","),
    );
    let first = session_call(&format!(
        "{{\"sessionAction\":\"rules\",\"session\":\"{id}\",\"sheet\":\"S\",\"rules\":{RULES}}}"
    ));
    let visuals = unpack(&first);
    let names: Vec<String> = visuals.iter().map(a1).collect();
    assert_eq!(names, ["A2", "B2"], "{first}");
    assert_eq!(visuals[0].bg, Some(0xFFC7CE));
    assert_eq!(visuals[1].bar.unwrap().pct, 100);

    let second = apply(&id, &cell("A1", "99"));
    assert!(
        second.contains("\"cf\":[{\"sheet\":\"S\",\"pal\":["),
        "{second}"
    );
    let delta = unpack(&second);
    assert_eq!(delta.iter().map(a1).collect::<Vec<_>>(), ["A1"]);

    let third = apply(&id, &cell("A1", "1"));
    let cleared = unpack(&third);
    assert_eq!(cleared.len(), 1);
    assert!(
        cleared[0].is_none(),
        "a cell that lost its rule comes back cleared"
    );

    let untouched = apply(&id, &cell("E5", "7"));
    assert!(!untouched.contains("\"cf\""), "{untouched}");
}

#[test]
fn validate_answers_with_the_rule_alert() {
    let id = open();
    session_call(&format!(
        "{{\"sessionAction\":\"rules\",\"session\":\"{id}\",\"sheet\":\"S\",\"rules\":{RULES}}}"
    ));
    let check = |cell: &str, value: &str| {
        session_call(&format!(
            "{{\"sessionAction\":\"validate\",\"session\":\"{id}\",\"sheet\":\"S\",\"cell\":\"{cell}\",\"value\":\"{value}\"}}"
        ))
    };
    assert_eq!(check("C1", "3"), "{\"ok\":true}");
    let bad = check("C1", "9");
    assert!(
        bad.contains("\"ok\":false") && bad.contains("\"alert\":\"warning\""),
        "{bad}"
    );
    assert!(
        bad.contains("\"message\":\"1~5\"") && bad.contains("\"show\":true"),
        "{bad}"
    );
    assert_eq!(check("D2", "가"), "{\"ok\":true}");
    assert!(check("D2", "다").contains("\"ok\":false"));
    assert_eq!(check("Z9", "anything"), "{\"ok\":true}");
}

#[test]
fn rules_for_an_unknown_session_or_with_bad_json_are_answered_safely() {
    let expired = session_call(
        "{\"sessionAction\":\"rules\",\"session\":\"nope\",\"sheet\":\"S\",\"rules\":{\"cf\":[],\"dv\":[]}}",
    );
    assert_eq!(expired, "{\"expired\":true}");
    let id = open();
    let bad = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        &format!(
            "{{\"sessionAction\":\"rules\",\"session\":\"{id}\",\"sheet\":\"S\",\"rules\":{{\"cf\":[{{\"r\":\"A1\",\"t\":\"nope\"}}]}}}}"
        ),
    );
    assert!(bad.is_err());
}

fn json_field<'a>(json: &'a str, key: &str) -> &'a str {
    let start = json.find(&format!("\"{key}\":\"")).unwrap() + key.len() + 4;
    &json[start..start + json[start..].find('"').unwrap()]
}

#[test]
fn rules_survive_save_and_reopen_in_xlsx_and_ods() {
    let sheets = format!(
        "\"sheets\":[{{\"name\":\"S\",\"cells\":[{{\"ref\":\"A1\",\"raw\":\"5\"}}],\"rules\":{RULES}}}]"
    );
    let want = StrictJsonValue::parse(&format!("{{{sheets}}}")).unwrap();
    let want_rules = crate::sheet_rules_json::parse_rules(
        StrictJsonValue::get_field(
            want.as_object().unwrap()[0].1.as_array().unwrap()[0]
                .as_object()
                .unwrap(),
            "rules",
        )
        .unwrap(),
    )
    .unwrap();
    for (target, file) in [("xlsx", "r.xlsx"), ("ods", "r.ods")] {
        let saved = dispatch(
            "convert_document_format",
            &format!("{{\"targetFormat\":\"{target}\",\"title\":\"r\",{sheets}}}"),
        );
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
        let opened = dispatch(
            "convert_document_format",
            &format!(
                "{{\"action\":\"decode\",\"fileName\":\"{file}\",\"sourceBase64\":\"{}\"}}",
                EasyOfficeIpcRouter::encode_base64(&bytes)
            ),
        );
        let start = opened
            .find("\"rules\":")
            .unwrap_or_else(|| panic!("{target}: {opened}"))
            + 8;
        let rules_json = &opened[start..];
        let parsed = StrictJsonValue::parse(&rules_json[..balanced_end(rules_json)]).unwrap();
        let got = crate::sheet_rules_json::parse_rules(&parsed).unwrap();
        assert_eq!(got.cf.len(), want_rules.cf.len(), "{target}");
        assert_eq!(got.dv.len(), want_rules.dv.len(), "{target}");
        assert_eq!(got.cf[0].kind, want_rules.cf[0].kind, "{target}");
        assert_eq!(got.dv[0].alert, want_rules.dv[0].alert, "{target}");
    }
}

/// Index just past the JSON object that starts at the beginning of `s`.
fn balanced_end(s: &str) -> usize {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for (i, ch) in s.char_indices() {
        if quoted {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => quoted = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    s.len()
}

#[test]
fn unsupported_rules_in_a_file_become_open_warnings() {
    let sheets = "\"sheets\":[{\"name\":\"S\",\"cells\":[{\"ref\":\"A1\",\"raw\":\"5\"}]}]";
    let saved = dispatch(
        "convert_document_format",
        &format!("{{\"targetFormat\":\"xlsx\",\"title\":\"r\",{sheets}}}"),
    );
    let bytes = EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
    let mut entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    for e in &mut entries {
        if e.path == "xl/worksheets/sheet1.xml" {
            e.data = String::from_utf8(e.data.clone())
                .unwrap()
                .replace(
                    "</worksheet>",
                    "<conditionalFormatting sqref=\"A1\"><cfRule type=\"timePeriod\" timePeriod=\"today\" priority=\"1\"/></conditionalFormatting></worksheet>",
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
    let opened = dispatch(
        "convert_document_format",
        &format!(
            "{{\"action\":\"decode\",\"fileName\":\"w.xlsx\",\"sourceBase64\":\"{}\"}}",
            EasyOfficeIpcRouter::encode_base64(&repacked)
        ),
    );
    assert!(
        opened
            .contains("S 시트의 조건부 서식 timePeriod 규칙은 지원하지 않아 가져오지 않았습니다."),
        "{opened}"
    );
    assert!(!opened.contains("\"rules\":"), "{opened}");
}
