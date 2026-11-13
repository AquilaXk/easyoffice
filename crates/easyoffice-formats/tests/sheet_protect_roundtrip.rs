//! Sheet and workbook protection survives XLSX and ODS round trips, a password is only ever a digest,
//! and hostile attributes stay in the model without opening to any password (PD-216).

use easyoffice_formats::{
    CellBox, CellStyle, Credential, OdfCodec, OoxmlCodec, PasswordCheck, ProtectedRange,
    SheetProtection, SheetStyling, Unsupported, WorkbookProtection, ZipArchiveCodec,
    ZipSecurityPolicy, DELETE_ROWS, FORMAT_CELLS, INSERT_ROWS, NEW_SHEET_DENY, SORT,
};

fn cells() -> Vec<(u32, u32, String)> {
    vec![
        (0, 0, "locked".to_string()),
        (0, 1, "open".to_string()),
        (0, 2, "=A1&B1".to_string()),
    ]
}

fn styled() -> SheetStyling {
    let mut st = SheetStyling::default();
    st.cells.insert(
        (0, 1),
        CellStyle {
            unlocked: true,
            ..CellStyle::default()
        },
    );
    st.cells.insert(
        (0, 2),
        CellStyle {
            formula_hidden: true,
            bold: true,
            ..CellStyle::default()
        },
    );
    st
}

fn protected_styling() -> SheetStyling {
    let credential = Credential::new_agile("비밀번호1!").unwrap();
    let mut protect = SheetProtection::locked_with(credential);
    protect.deny &= !(INSERT_ROWS | SORT);
    protect.ranges.push(ProtectedRange {
        name: "입력".to_string(),
        areas: vec![CellBox::parse("B2:C3").unwrap()],
        credential: Credential::Legacy(0x83AF),
        descriptor: None,
    });
    let mut st = styled();
    st.protect = Some(protect);
    st
}

fn pack_xlsx(stylings: &[SheetStyling]) -> Vec<u8> {
    let sheets: Vec<_> = stylings
        .iter()
        .enumerate()
        .map(|(i, _)| (format!("Sheet{}", i + 1), cells()))
        .collect();
    OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], stylings).unwrap()
}

fn read_xlsx(bytes: &[u8]) -> Vec<SheetStyling> {
    OoxmlCodec::unpack_xlsx_with_styling(bytes)
        .unwrap()
        .1
        .into_iter()
        .map(|(_, s)| s)
        .collect()
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

fn replace_part(bytes: &[u8], path: &str, edit: impl Fn(&str) -> String) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let out: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == path {
                let text = edit(std::str::from_utf8(&e.data).unwrap());
                (e.path, text.into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&out).unwrap()
}

#[test]
fn xlsx_sheet_protection_cell_locks_and_ranges_round_trip() {
    let source = protected_styling();
    let bytes = pack_xlsx(&[source.clone(), SheetStyling::default()]);
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<sheetProtection sheet=\"1\" insertRows=\"0\" sort=\"0\" objects=\"1\" scenarios=\"1\" algorithmName=\"SHA-512\""), "{sheet}");
    assert!(
        sheet.contains("<protectedRange sqref=\"B2:C3\" name=\"입력\" password=\"83AF\"/>"),
        "{sheet}"
    );
    // After the data and before the merge cells, in worksheet order.
    assert!(sheet.find("</sheetData>").unwrap() < sheet.find("<sheetProtection").unwrap());
    let styles = part(&bytes, "xl/styles.xml");
    assert!(styles.contains("<protection locked=\"0\"/>"), "{styles}");
    assert!(styles.contains("<protection hidden=\"1\"/>"), "{styles}");
    let read = read_xlsx(&bytes);
    assert_eq!(read[0].protect, source.protect);
    assert_eq!(read[0].cells, styled().cells);
    assert!(read[1].protect.is_none());
    // A second write changes nothing.
    assert_eq!(read_xlsx(&pack_xlsx(&read)), read);
    // The password only checks as a digest.
    let credential = &read[0].protect.as_ref().unwrap().credential;
    assert_eq!(credential.verify("비밀번호1!"), PasswordCheck::Accepted);
    assert_eq!(credential.verify("비밀번호"), PasswordCheck::Rejected);
}

#[test]
fn no_plain_password_reaches_the_file() {
    let password = "plain-secret-PW";
    let st = SheetStyling {
        protect: Some(SheetProtection::locked_with(
            Credential::new_agile(password).unwrap(),
        )),
        workbook_protect: Some(WorkbookProtection::structure_with(
            Credential::new_agile(password).unwrap(),
        )),
        ..SheetStyling::default()
    };
    let bytes = pack_xlsx(&[st]);
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    for entry in entries {
        let text = String::from_utf8_lossy(&entry.data);
        assert!(!text.contains(password), "{}", entry.path);
        let wide: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert!(
            !entry.data.windows(wide.len()).any(|w| w == wide),
            "{}",
            entry.path
        );
    }
}

#[test]
fn xlsx_workbook_protection_round_trips() {
    let mut first = SheetStyling::default();
    let mut workbook = WorkbookProtection::structure_with(Credential::new_agile("구조").unwrap());
    workbook.lock_windows = true;
    first.workbook_protect = Some(workbook.clone());
    let bytes = pack_xlsx(&[first, SheetStyling::default()]);
    let book = part(&bytes, "xl/workbook.xml");
    assert!(
        book.contains("<workbookProtection workbookAlgorithmName=\"SHA-512\""),
        "{book}"
    );
    assert!(book.find("<workbookProtection").unwrap() < book.find("<sheets>").unwrap());
    let read = read_xlsx(&bytes);
    assert_eq!(read[0].workbook_protect, Some(workbook));
    assert_eq!(
        read[0]
            .workbook_protect
            .as_ref()
            .unwrap()
            .credential
            .verify("구조"),
        PasswordCheck::Accepted
    );
    assert!(read[1].workbook_protect.is_none());
}

#[test]
fn an_unlocked_row_and_column_keep_their_lock_state() {
    let mut st = SheetStyling {
        protect: Some(SheetProtection::locked_with(Credential::None)),
        unlocked_cols: vec![(3, 4)],
        unlocked_rows: vec![(9, 10)],
        ..SheetStyling::default()
    };
    // A cell locked on purpose inside the unlocked column.
    st.cells.insert(
        (5, 3),
        CellStyle {
            force_locked: true,
            ..CellStyle::default()
        },
    );
    let sheets = vec![(
        "Sheet1".to_string(),
        vec![(5u32, 3u32, "x".to_string()), (6, 3, "y".to_string())],
    )];
    let bytes = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &[st.clone()]).unwrap();
    let read = read_xlsx(&bytes);
    assert_eq!(read[0].unlocked_cols, vec![(3, 4)]);
    assert_eq!(read[0].unlocked_rows, vec![(9, 10)]);
    assert!(!read[0].cell_unlocked(5, 3));
    assert!(read[0].cell_unlocked(6, 3));
    assert!(read[0].cell_unlocked(9, 0));
    assert!(!read[0].cell_unlocked(0, 0));
    assert_eq!(
        read[0].cells.get(&(5, 3)).map(|c| c.force_locked),
        Some(true)
    );
}

#[test]
fn a_file_written_by_another_program_is_read() {
    let base = pack_xlsx(&[SheetStyling::default()]);
    let with_protection = replace_part(&base, "xl/worksheets/sheet1.xml", |xml| {
        xml.replace(
            "</sheetData>",
            "</sheetData><sheetProtection algorithmName=\"SHA-512\" hashValue=\"x\" saltValue=\"y\" spinCount=\"100000\" sheet=\"1\" objects=\"1\" scenarios=\"1\" formatCells=\"0\" deleteRows=\"0\" selectLockedCells=\"1\"/>",
        )
    });
    let read = read_xlsx(&with_protection);
    let p = read[0].protect.as_ref().unwrap();
    assert!(p.on && p.allows(FORMAT_CELLS) && p.allows(DELETE_ROWS));
    assert!(!p.allows(INSERT_ROWS));
    assert_eq!(
        p.deny,
        NEW_SHEET_DENY & !(FORMAT_CELLS | DELETE_ROWS) | easyoffice_formats::SELECT_LOCKED_CELLS
    );
    // The digest is kept as it is and written back.
    let again = pack_xlsx(&read);
    let sheet = part(&again, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("hashValue=\"x\" saltValue=\"y\" spinCount=\"100000\""),
        "{sheet}"
    );
}

#[test]
fn hostile_digests_are_kept_and_never_open() {
    let base = pack_xlsx(&[SheetStyling::default()]);
    for (attrs, why) in [
        (
            "algorithmName=\"SHA-512\" hashValue=\"AAAA\" saltValue=\"AAAA\" spinCount=\"4294967295\"",
            Unsupported::SpinCount,
        ),
        (
            "algorithmName=\"SHA-512\" hashValue=\"AAAA\" saltValue=\"AAAA\" spinCount=\"10000001\"",
            Unsupported::SpinCount,
        ),
        (
            "algorithmName=\"SHA-512\" hashValue=\"AAAA\" saltValue=\"AAAA\" spinCount=\"-1\"",
            Unsupported::SpinCount,
        ),
        (
            "algorithmName=\"MD5\" hashValue=\"AAAA\" saltValue=\"AAAA\" spinCount=\"1\"",
            Unsupported::Algorithm,
        ),
        (
            "algorithmName=\"SHA-512\" hashValue=\"!!bad!!\" saltValue=\"AAAA\" spinCount=\"1\"",
            Unsupported::Encoding,
        ),
    ] {
        let bytes = replace_part(&base, "xl/worksheets/sheet1.xml", |xml| {
            xml.replace("</sheetData>", &format!("</sheetData><sheetProtection sheet=\"1\" {attrs}/>"))
        });
        let read = read_xlsx(&bytes);
        let credential = &read[0].protect.as_ref().unwrap().credential;
        assert_eq!(credential.verify("password"), PasswordCheck::Unsupported(why), "{attrs}");
        // Written back unchanged (a spinCount that is not a number is the largest value).
        let again = read_xlsx(&pack_xlsx(&read));
        assert_eq!(again[0].protect, read[0].protect, "{attrs}");
    }
}

#[test]
fn text_over_the_bound_fails_the_open() {
    let base = pack_xlsx(&[SheetStyling::default()]);
    let long = "A".repeat(5_000);
    let bytes = replace_part(&base, "xl/worksheets/sheet1.xml", |xml| {
        xml.replace(
            "</sheetData>",
            &format!("</sheetData><sheetProtection sheet=\"1\" hashValue=\"{long}\"/>"),
        )
    });
    assert!(OoxmlCodec::unpack_xlsx_with_styling(&bytes).is_err());
}

#[test]
fn a_ranges_only_element_keeps_the_ranges_on_an_unprotected_sheet() {
    let base = pack_xlsx(&[SheetStyling::default()]);
    let bytes = replace_part(&base, "xl/worksheets/sheet1.xml", |xml| {
        xml.replace(
            "</sheetData>",
            "</sheetData><protectedRanges><protectedRange sqref=\"A1:B2 D4\" name=\"R\"/></protectedRanges>",
        )
    });
    let read = read_xlsx(&bytes);
    let p = read[0].protect.as_ref().unwrap();
    assert!(!p.on);
    assert_eq!(p.ranges[0].areas.len(), 2);
    let again = read_xlsx(&pack_xlsx(&read));
    assert_eq!(again[0].protect, read[0].protect);
}

fn pack_ods(stylings: &[SheetStyling]) -> Vec<u8> {
    let sheets: Vec<_> = stylings
        .iter()
        .enumerate()
        .map(|(i, _)| (format!("Sheet{}", i + 1), cells()))
        .collect();
    OdfCodec::pack_ods_multi_styled(&sheets, stylings).unwrap()
}

fn read_ods(bytes: &[u8]) -> Vec<SheetStyling> {
    OdfCodec::unpack_ods_with_styling(bytes)
        .unwrap()
        .1
        .into_iter()
        .map(|(_, s)| s)
        .collect()
}

fn ods_styling() -> SheetStyling {
    let mut protect = SheetProtection::locked_with(Credential::new_odf("암호 pw").unwrap());
    protect.deny &= !(INSERT_ROWS
        | easyoffice_formats::SELECT_LOCKED_CELLS
        | easyoffice_formats::SELECT_UNLOCKED_CELLS);
    let mut st = styled();
    st.protect = Some(protect);
    st.unlocked_cols = vec![(4, 5)];
    st.cells.insert(
        (2, 4),
        CellStyle {
            force_locked: true,
            ..CellStyle::default()
        },
    );
    st
}

#[test]
fn ods_protection_cell_locks_and_workbook_structure_round_trip() {
    let mut first = ods_styling();
    first.workbook_protect = Some(WorkbookProtection::structure_with(
        Credential::new_odf("구조").unwrap(),
    ));
    let bytes = pack_ods(&[first.clone(), SheetStyling::default()]);
    let content = {
        let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
        String::from_utf8(
            entries
                .iter()
                .find(|e| e.path == "content.xml")
                .unwrap()
                .data
                .clone(),
        )
        .unwrap()
    };
    assert!(
        content.contains("table:protected=\"true\" table:protection-key=\""),
        "{content}"
    );
    assert!(content.contains(
        "table:protection-key-digest-algorithm=\"http://www.w3.org/2000/09/xmldsig#sha256\""
    ));
    assert!(content.contains("<loext:table-protection loext:select-protected-cells=\"true\""));
    assert!(content.contains("loext:insert-rows=\"true\" loext:delete-columns=\"false\""));
    assert!(content.contains("<office:spreadsheet table:structure-protected=\"true\""));
    assert!(
        content.contains("style:cell-protect=\"none\"")
            && content.contains("style:cell-protect=\"protected formula-hidden\"")
    );
    assert!(content.contains("table:default-cell-style-name="));
    let read = read_ods(&bytes);
    let got = read[0].protect.as_ref().unwrap();
    assert_eq!(got.credential.verify("암호 pw"), PasswordCheck::Accepted);
    assert_eq!(got.credential.verify("암호"), PasswordCheck::Rejected);
    assert!(got.allows(INSERT_ROWS) && !got.allows(DELETE_ROWS));
    assert_eq!(read[0].cells, first.cells);
    assert_eq!(read[0].unlocked_cols, vec![(4, 5)]);
    assert!(!read[0].cell_unlocked(2, 4));
    assert!(read[0].cell_unlocked(3, 4));
    let book = read[0].workbook_protect.as_ref().unwrap();
    assert!(book.lock_structure);
    assert_eq!(book.credential.verify("구조"), PasswordCheck::Accepted);
    assert!(read[1].protect.is_none());
    // A second write changes nothing.
    assert_eq!(read_ods(&pack_ods(&read)), read);
}

#[test]
fn a_digest_ods_cannot_hold_writes_protection_without_a_password() {
    let st = SheetStyling {
        protect: Some(SheetProtection::locked_with(
            Credential::new_agile("pw").unwrap(),
        )),
        ..SheetStyling::default()
    };
    let read = read_ods(&pack_ods(&[st]));
    let got = read[0].protect.as_ref().unwrap();
    assert!(got.on && got.credential.is_none());
}

#[test]
fn hostile_ods_keys_never_open() {
    let base = pack_ods(&[SheetStyling::default()]);
    let entries = ZipArchiveCodec::unpack(&base, &ZipSecurityPolicy::default()).unwrap();
    let hostile = |attrs: &str| {
        let out: Vec<(String, Vec<u8>)> = entries
            .iter()
            .map(|e| {
                if e.path == "content.xml" {
                    let text = std::str::from_utf8(&e.data).unwrap().replace(
                        "<table:table table:name=\"Sheet1\"",
                        &format!(
                            "<table:table table:name=\"Sheet1\" table:protected=\"true\" {attrs}"
                        ),
                    );
                    (e.path.clone(), text.into_bytes())
                } else {
                    (e.path.clone(), e.data.clone())
                }
            })
            .collect();
        ZipArchiveCodec::pack_compressed(&out).unwrap()
    };
    for attrs in [
        "table:protection-key=\"!!!\" table:protection-key-digest-algorithm=\"http://www.w3.org/2000/09/xmldsig#sha256\"",
        "table:protection-key=\"AAAA\" table:protection-key-digest-algorithm=\"urn:unknown\"",
        "table:protection-key=\"AAAA\"",
    ] {
        let read = read_ods(&hostile(attrs));
        let credential = &read[0].protect.as_ref().unwrap().credential;
        assert_ne!(credential.verify("password"), PasswordCheck::Accepted, "{attrs}");
        assert_eq!(read_ods(&pack_ods(&read))[0].protect, read[0].protect, "{attrs}");
    }
    let long = "A".repeat(5_000);
    assert!(OdfCodec::unpack_ods_with_styling(&hostile(&format!(
        "table:protection-key=\"{long}\""
    )))
    .is_err());
}

/// The attributes of three files that a spreadsheet program wrote (the test data of an open-source library, Apache-2.0, see THIRD_PARTY_NOTICES.md:
/// `workbookProtection-sheet_password-2013.xlsx` with the password `pwd`, `workbookProtection-workbook_password-2013.xlsx`
/// and `workbookProtection-workbook_password_user_range-2010.xlsx` with `test`): the agile digest and the legacy hash
/// computed by this program match what the program wrote.
#[test]
fn digests_written_by_a_spreadsheet_program_verify() {
    let base = pack_xlsx(&[SheetStyling::default()]);
    let sheet_attrs = "algorithmName=\"SHA-512\" hashValue=\"5MANCkOK6IY02H1LhiJ+ucR5ZHvoV7BwbINSx52iIhe4Xfg986k2l32ONsYpt8JPiy8U8kqPRKXIr7G8hfMWOw==\" saltValue=\"R040EdN/Ec7il6MJ8JrRLQ==\" spinCount=\"100000\" sheet=\"1\" objects=\"1\" scenarios=\"1\"";
    let book_attrs = "workbookAlgorithmName=\"SHA-512\" workbookHashValue=\"hBZdAINPpoA+8nBASfoa7mLOowkmljnvmY5sAOt6nY7wp+OXyq6jhmkmos6b6EcAd60kZXMvRbeTfI+rfSsTDg==\" workbookSaltValue=\"Wq5e2oy8ZLa/369T8z/Jaw==\" workbookSpinCount=\"100000\" lockStructure=\"1\"";
    let with = |part: &str, from: &str, to: String| {
        replace_part(&base, part, move |xml| xml.replace(from, &to))
    };
    let sheet = with(
        "xl/worksheets/sheet1.xml",
        "</sheetData>",
        format!("</sheetData><sheetProtection {sheet_attrs}/>"),
    );
    let read = read_xlsx(&sheet);
    let credential = &read[0].protect.as_ref().unwrap().credential;
    assert_eq!(credential.verify("pwd"), PasswordCheck::Accepted);
    assert_eq!(credential.verify("Pwd"), PasswordCheck::Rejected);
    let book = with(
        "xl/workbook.xml",
        "<sheets>",
        format!("<workbookProtection {book_attrs}/><sheets>"),
    );
    let read = read_xlsx(&book);
    let credential = &read[0].workbook_protect.as_ref().unwrap().credential;
    assert_eq!(credential.verify("test"), PasswordCheck::Accepted);
    let legacy = with("xl/workbook.xml", "<sheets>", "<workbookProtection workbookPassword=\"CBEB\" lockStructure=\"1\" lockWindows=\"1\"/><sheets>".to_string());
    let read = read_xlsx(&legacy);
    let w = read[0].workbook_protect.as_ref().unwrap();
    assert!(w.lock_structure && w.lock_windows);
    assert_eq!(w.credential.verify("test"), PasswordCheck::Accepted);
    assert_eq!(w.credential.verify("tesu"), PasswordCheck::Rejected);
}
