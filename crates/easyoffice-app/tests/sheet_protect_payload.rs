//! Sheet and workbook protection in the save and open payloads: it reaches the XLSX and ODS file, comes back
//! to the page as the same keys, a password is never in the payload the page gets back, a bad value refuses
//! the whole save, and a locked row or column edit is refused by the engine (PD-216).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    read_sheet_styling, DocumentFormat, PasswordCheck, INSERT_ROWS, SELECT_LOCKED_CELLS,
};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn call(command: &str, payload: &str) -> Result<String, String> {
    EasyOfficeIpcRouter::dispatch(command, payload).map_err(|e| e.to_string())
}

fn hash(password: &str, kind: &str) -> String {
    let answer = call(
        "evaluate_sheet",
        &format!(r#"{{"sessionAction":"protectHash","password":"{password}","kind":"{kind}"}}"#),
    )
    .unwrap();
    answer["{\"pwd\":".len()..answer.len() - 1].to_string()
}

fn save(dst: &str, sheet_extra: &str, top_extra: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가"}},{{"ref":"B1","raw":"=A1","style":{{"unlocked":true,"hideFormula":true}}}}]{sheet_extra}}}]{top_extra}}}"#
    );
    let json = call("convert_document_format", &payload)?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap())
}

fn open(dst: &str, bytes: &[u8]) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"s.{dst}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    call("convert_document_format", &payload).unwrap()
}

#[test]
fn protection_reaches_the_file_and_comes_back_as_the_same_keys() {
    for (dst, kind, format) in [
        ("xlsx", "agile", DocumentFormat::Xlsx),
        ("ods", "odf", DocumentFormat::Ods),
    ] {
        let secret = "비밀-pw-1";
        let pwd = hash(secret, kind);
        let sheet = format!(
            r#","protect":{{"on":true,"deny":["formatCells","deleteRows","deleteColumns","insertColumns","objects","scenarios"],"pwd":{pwd}}}"#
        );
        let book = format!(
            r#","workbookProtect":{{"structure":true,"pwd":{}}}"#,
            hash("구조", kind)
        );
        let bytes = save(dst, &sheet, &book).unwrap();
        let styling = read_sheet_styling(&bytes, format).unwrap().remove(0).1;
        let protect = styling.protect.as_ref().unwrap();
        assert!(protect.on && protect.allows(INSERT_ROWS), "{dst}");
        assert!(protect.allows(SELECT_LOCKED_CELLS), "{dst}");
        assert_eq!(
            protect.credential.verify(secret),
            PasswordCheck::Accepted,
            "{dst}"
        );
        assert_eq!(
            protect.credential.verify("x"),
            PasswordCheck::Rejected,
            "{dst}"
        );
        let style = styling.cells.get(&(0, 1)).unwrap();
        assert!(style.unlocked && style.formula_hidden, "{dst}");
        assert_eq!(
            styling
                .workbook_protect
                .as_ref()
                .unwrap()
                .credential
                .verify("구조"),
            PasswordCheck::Accepted,
            "{dst}"
        );
        let opened = open(dst, &bytes);
        assert!(
            opened.contains("\"protect\":{\"on\":true,\"deny\":["),
            "{dst}: {opened}"
        );
        assert!(
            opened.contains("\"unlocked\":true") && opened.contains("\"hideFormula\":true"),
            "{dst}"
        );
        assert!(
            opened.contains("\"workbookProtect\":{\"structure\":true"),
            "{dst}: {opened}"
        );
        assert!(
            !opened.contains(secret) && !opened.contains("구조\""),
            "{dst}: the password is not in the answer"
        );
        let plain = open(dst, &save(dst, "", "").unwrap());
        assert!(
            !plain.contains("\"protect\"") && !plain.contains("workbookProtect"),
            "{dst}"
        );
    }
}

#[test]
fn a_bad_protection_value_refuses_the_save() {
    let long = "A".repeat(2_000);
    for extra in [
        r#","protect":{"on":true,"deny":["nope"]}"#.to_string(),
        r#","protect":{"on":true,"deny":[],"pwd":{"kind":"agile","alg":"SHA-512","hash":"A","salt":"B","spin":-5}}"#.to_string(),
        format!(r#","protect":{{"on":true,"deny":[],"pwd":{{"kind":"agile","alg":"SHA-512","hash":"{long}","salt":"B","spin":1}}}}"#),
        r#","protect":"on""#.to_string(),
        r#","unlockedRows":[[0,1]]"#.to_string(),
    ] {
        for dst in ["xlsx", "ods"] {
            assert!(save(dst, &extra, "").is_err(), "{dst}: {extra}");
        }
    }
    assert!(save("xlsx", "", r#","workbookProtect":{"structure":"yes"}"#).is_err());
}

#[test]
fn a_hostile_digest_is_saved_back_and_never_opens() {
    let hostile = r#","protect":{"on":true,"deny":[],"pwd":{"kind":"agile","alg":"WHIRLPOOL","hash":"AAAA","salt":"AAAA","spin":4294967295}}"#;
    let bytes = save("xlsx", hostile, "").unwrap();
    let opened = open("xlsx", &bytes);
    assert!(
        opened.contains("\"alg\":\"WHIRLPOOL\"") && opened.contains("\"spin\":4294967295"),
        "{opened}"
    );
    let verdict = call(
        "evaluate_sheet",
        r#"{"sessionAction":"protectVerify","password":"x","pwd":{"kind":"agile","alg":"WHIRLPOOL","hash":"AAAA","salt":"AAAA","spin":4294967295}}"#,
    )
    .unwrap();
    assert_eq!(verdict, r#"{"unsupported":"algorithm"}"#);
}

fn open_session() -> String {
    let res = call(
        "evaluate_sheet",
        r#"{"sessionAction":"open","sheets":["S"]}"#,
    )
    .unwrap();
    json_str_field(&res, "session").to_string()
}

#[test]
fn the_engine_refuses_a_row_or_column_edit_the_sheet_locks() {
    let id = open_session();
    let protect = r#""protect":{"on":true,"deny":["insertColumns","deleteRows","deleteColumns"],"ranges":[{"name":"r","sqref":["B3:C4"]}]}"#;
    let edit = |axis: &str, op: &str, protect: &str| {
        call(
            "evaluate_sheet",
            &format!(
                r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"{axis}","op":"{op}","at":2,"count":1,{protect}}}"#
            ),
        )
        .unwrap()
    };
    // Inserting rows is allowed: the allowed range moves down with the cells.
    let moved = edit("row", "insert", protect);
    assert!(moved.contains("\"sqref\":[\"B4:C5\"]"), "{moved}");
    assert!(
        moved.contains("\"deny\":[\"insertColumns\",\"deleteColumns\",\"deleteRows\"]"),
        "{moved}"
    );
    for (axis, op) in [("col", "insert"), ("row", "delete"), ("col", "delete")] {
        assert_eq!(
            edit(axis, op, protect),
            r#"{"refused":"protected"}"#,
            "{axis} {op}"
        );
    }
    // The same edit on a sheet that is not protected, or whose protection is off, goes through.
    let off = protect.replace("\"on\":true", "\"on\":false");
    assert!(!edit("col", "delete", &off).contains("refused"));
    assert!(edit(
        "row",
        "delete",
        &format!("\"unlockedRows\":[[5,6]],{protect}").replace("deleteRows", "sort")
    )
    .contains("\"unlockedRows\":[[4,5]]"));
    call(
        "evaluate_sheet",
        &format!(r#"{{"sessionAction":"close","session":"{id}"}}"#),
    )
    .unwrap();
}
