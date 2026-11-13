//! Protection in SpreadsheetML (PD-216): `sheetProtection` and `protectedRanges` of a worksheet part
//! (ECMA-376 18.3.1.85, 18.3.1.70), `workbookProtection` of `xl/workbook.xml` (18.2.29), and the
//! `protection` element of a cell format (18.8.33; read and written in `xlsx_styles`). The model is
//! `sheet_protect`.
//!
//! A flag attribute that is absent reads as the schema default (`FILE_DEFAULT_DENY`) and only attributes that
//! differ from it are written. A password is a digest: the agile attributes (`algorithmName`, `hashValue`,
//! `saltValue`, `spinCount`) or the legacy hex `password`; a digest this program cannot check is kept as it is
//! and a save writes it back. Text over `MAX_PROTECT_TEXT_CHARS` is not read: the part fails to open, so a
//! sheet is never silently opened without the password that was set on it.

use std::fmt::Write;

use crate::sheet_protect::{
    AgileHash, Credential, ProtectedRange, SheetProtection, WorkbookProtection, FILE_DEFAULT_DENY,
    FLAG_NAMES, MAX_PROTECTED_RANGES, MAX_PROTECT_TEXT_CHARS, MAX_RANGE_AREAS,
};
use crate::xml_stream::{Attrs, XmlEvent};
use crate::{parse_sqref, EasyOfficeError, XmlSanitizer};

fn truthy(value: &str) -> bool {
    matches!(value, "1" | "true")
}

fn too_long() -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(
        "XLSX 보호 정보의 문자열이 허용 길이를 초과했습니다.".to_string(),
    )
}

/// The attribute names that carry one credential: the legacy hex and the four agile ones.
struct CredentialNames {
    legacy: &'static str,
    algorithm: &'static str,
    hash: &'static str,
    salt: &'static str,
    spin: &'static str,
}

const SHEET_NAMES: CredentialNames = CredentialNames {
    legacy: "password",
    algorithm: "algorithmName",
    hash: "hashValue",
    salt: "saltValue",
    spin: "spinCount",
};
const WORKBOOK_NAMES: CredentialNames = CredentialNames {
    legacy: "workbookPassword",
    algorithm: "workbookAlgorithmName",
    hash: "workbookHashValue",
    salt: "workbookSaltValue",
    spin: "workbookSpinCount",
};
const REVISION_NAMES: CredentialNames = CredentialNames {
    legacy: "revisionsPassword",
    algorithm: "revisionsAlgorithmName",
    hash: "revisionsHashValue",
    salt: "revisionsSaltValue",
    spin: "revisionsSpinCount",
};

fn text_of(attrs: &Attrs<'_>, name: &str) -> Result<Option<String>, EasyOfficeError> {
    match attrs.get(name) {
        Some(v) if v.chars().count() > MAX_PROTECT_TEXT_CHARS => Err(too_long()),
        Some(v) => Ok(Some(v.into_owned())),
        None => Ok(None),
    }
}

/// The credential the attributes describe. The agile attributes win over the legacy one (a file that has both
/// keeps the stronger). A `spinCount` that is not a number reads as the largest value, which no check runs;
/// a legacy value that is not hex reads as a digest of an unknown algorithm: neither opens with a password.
fn read_credential(
    attrs: &Attrs<'_>,
    names: &CredentialNames,
) -> Result<Credential, EasyOfficeError> {
    let algorithm = text_of(attrs, names.algorithm)?;
    let hash = text_of(attrs, names.hash)?;
    if algorithm.is_some() || hash.is_some() {
        let spin = attrs
            .get(names.spin)
            .map_or(0, |v| v.trim().parse::<u32>().unwrap_or(u32::MAX));
        return Ok(Credential::Agile(AgileHash {
            algorithm: algorithm.unwrap_or_default(),
            hash: hash.unwrap_or_default(),
            salt: text_of(attrs, names.salt)?.unwrap_or_default(),
            spin,
        }));
    }
    let Some(legacy) = text_of(attrs, names.legacy)? else {
        return Ok(Credential::None);
    };
    Ok(match u16::from_str_radix(legacy.trim(), 16) {
        Ok(hash) => Credential::Legacy(hash),
        Err(_) => Credential::Agile(AgileHash {
            algorithm: "unknown".to_string(),
            hash: String::new(),
            salt: String::new(),
            spin: 0,
        }),
    })
}

fn write_credential(xml: &mut String, credential: &Credential, names: &CredentialNames) {
    match credential {
        Credential::None | Credential::Odf(_) => {}
        Credential::Legacy(hash) => {
            let _ = write!(xml, " {}=\"{hash:04X}\"", names.legacy);
        }
        Credential::Both(a, _) => {
            write_credential(xml, &Credential::Agile((**a).clone()), names);
        }
        Credential::Agile(a) => {
            let _ = write!(
                xml,
                " {}=\"{}\" {}=\"{}\" {}=\"{}\" {}=\"{}\"",
                names.algorithm,
                XmlSanitizer::escape(&a.algorithm),
                names.hash,
                XmlSanitizer::escape(&a.hash),
                names.salt,
                XmlSanitizer::escape(&a.salt),
                names.spin,
                a.spin
            );
        }
    }
}

/// The protection elements of one worksheet as the scan meets them.
#[derive(Default)]
pub(crate) struct ProtectScan {
    protection: Option<SheetProtection>,
    ranges: Vec<ProtectedRange>,
    failed: Option<EasyOfficeError>,
}

impl ProtectScan {
    pub(crate) fn feed(&mut self, event: &XmlEvent<'_>) {
        let XmlEvent::Start { name, attrs } = event else {
            return;
        };
        let result = match *name {
            "sheetProtection" if self.protection.is_none() => self.sheet(attrs),
            "protectedRange" => self.range(attrs),
            _ => Ok(()),
        };
        if let Err(err) = result {
            self.failed.get_or_insert(err);
        }
    }

    fn sheet(&mut self, attrs: &Attrs<'_>) -> Result<(), EasyOfficeError> {
        let mut deny = FILE_DEFAULT_DENY;
        for (name, bit) in FLAG_NAMES {
            if let Some(value) = attrs.get(name) {
                if truthy(&value) {
                    deny |= bit;
                } else {
                    deny &= !bit;
                }
            }
        }
        self.protection = Some(SheetProtection {
            on: attrs.get("sheet").as_deref().is_some_and(truthy),
            deny,
            credential: read_credential(attrs, &SHEET_NAMES)?,
            ranges: Vec::new(),
        });
        Ok(())
    }

    fn range(&mut self, attrs: &Attrs<'_>) -> Result<(), EasyOfficeError> {
        if self.ranges.len() >= MAX_PROTECTED_RANGES {
            return Err(too_long());
        }
        let mut areas = parse_sqref(&attrs.get("sqref").unwrap_or_default());
        areas.truncate(MAX_RANGE_AREAS);
        self.ranges.push(ProtectedRange {
            name: text_of(attrs, "name")?.unwrap_or_default(),
            areas,
            credential: read_credential(attrs, &SHEET_NAMES)?,
            descriptor: text_of(attrs, "securityDescriptor")?,
        });
        Ok(())
    }

    /// The protection of the sheet; an error for text over the bound. Ranges without a `sheetProtection`
    /// element are kept on a sheet that is not protected.
    pub(crate) fn finish(self) -> Result<Option<SheetProtection>, EasyOfficeError> {
        if let Some(err) = self.failed {
            return Err(err);
        }
        let mut protection = self.protection;
        if !self.ranges.is_empty() {
            protection
                .get_or_insert_with(SheetProtection::default)
                .ranges = self.ranges;
        }
        Ok(protection.filter(|p| !p.is_default()))
    }
}

/// `sheetProtection` and `protectedRanges` for the worksheet, after `sheetData` (empty without protection).
pub(crate) fn sheet_protection_xml(protection: &SheetProtection) -> String {
    if protection.is_default() {
        return String::new();
    }
    let mut xml = String::from("<sheetProtection");
    if protection.on {
        xml.push_str(" sheet=\"1\"");
    }
    for (name, bit) in FLAG_NAMES {
        let denied = protection.deny & bit != 0;
        if denied != (FILE_DEFAULT_DENY & bit != 0) {
            let _ = write!(xml, " {name}=\"{}\"", u8::from(denied));
        }
    }
    write_credential(&mut xml, &protection.credential, &SHEET_NAMES);
    xml.push_str("/>");
    if !protection.ranges.is_empty() {
        xml.push_str("<protectedRanges>");
        for range in &protection.ranges {
            let sqref: Vec<String> = range.areas.iter().map(|a| a.to_a1()).collect();
            let _ = write!(
                xml,
                "<protectedRange sqref=\"{}\" name=\"{}\"",
                sqref.join(" "),
                XmlSanitizer::escape(&range.name)
            );
            write_credential(&mut xml, &range.credential, &SHEET_NAMES);
            if let Some(descriptor) = &range.descriptor {
                let _ = write!(
                    xml,
                    " securityDescriptor=\"{}\"",
                    XmlSanitizer::escape(descriptor)
                );
            }
            xml.push_str("/>");
        }
        xml.push_str("</protectedRanges>");
    }
    xml
}

/// The `workbookProtection` element of the workbook part, if there is one.
pub(crate) fn read_workbook_protection(
    workbook_xml: &str,
) -> Result<Option<WorkbookProtection>, EasyOfficeError> {
    let mut reader = crate::xml_stream::XmlReader::unbounded(workbook_xml);
    while let Some(event) = reader.next_event()? {
        let XmlEvent::Start {
            name: "workbookProtection",
            attrs,
        } = event
        else {
            continue;
        };
        let flag = |name: &str| attrs.get(name).as_deref().is_some_and(truthy);
        let mut revisions = Vec::new();
        for name in [
            "lockRevision",
            REVISION_NAMES.legacy,
            REVISION_NAMES.algorithm,
            REVISION_NAMES.hash,
            REVISION_NAMES.salt,
            REVISION_NAMES.spin,
        ] {
            if let Some(value) = text_of(&attrs, name)? {
                revisions.push((name.to_string(), value));
            }
        }
        let protection = WorkbookProtection {
            lock_structure: flag("lockStructure"),
            lock_windows: flag("lockWindows"),
            credential: read_credential(&attrs, &WORKBOOK_NAMES)?,
            revisions,
        };
        return Ok(Some(protection).filter(|p| !p.is_default()));
    }
    Ok(None)
}

/// `<workbookProtection .../>` (empty without protection).
pub(crate) fn workbook_protection_xml(protection: &WorkbookProtection) -> String {
    if protection.is_default() {
        return String::new();
    }
    let mut xml = String::from("<workbookProtection");
    write_credential(&mut xml, &protection.credential, &WORKBOOK_NAMES);
    for (name, value) in &protection.revisions {
        let _ = write!(xml, " {name}=\"{}\"", XmlSanitizer::escape(value));
    }
    if protection.lock_structure {
        xml.push_str(" lockStructure=\"1\"");
    }
    if protection.lock_windows {
        xml.push_str(" lockWindows=\"1\"");
    }
    xml.push_str("/>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_protect::*;
    use crate::xml_stream::XmlReader;
    use crate::CellBox;

    fn scan(xml: &str) -> Result<Option<SheetProtection>, EasyOfficeError> {
        let mut scan = ProtectScan::default();
        let mut reader = XmlReader::unbounded(xml);
        while let Some(event) = reader.next_event().unwrap() {
            scan.feed(&event);
        }
        scan.finish()
    }

    #[test]
    fn file_defaults_and_written_attributes() {
        let p = scan(r#"<sheetProtection sheet="1" objects="1" scenarios="1"/>"#)
            .unwrap()
            .unwrap();
        assert!(p.on);
        assert_eq!(p.deny, NEW_SHEET_DENY);
        assert_eq!(
            sheet_protection_xml(&p),
            r#"<sheetProtection sheet="1" objects="1" scenarios="1"/>"#
        );
        let open = scan(r#"<sheetProtection sheet="1" formatCells="0" insertRows="false" selectLockedCells="1"/>"#)
            .unwrap()
            .unwrap();
        assert!(open.allows(FORMAT_CELLS) && open.allows(INSERT_ROWS));
        assert!(!open.allows(SELECT_LOCKED_CELLS) && !open.allows(DELETE_ROWS));
        assert_eq!(scan("<worksheet/>").unwrap(), None);
    }

    #[test]
    fn agile_legacy_and_ranges_round_trip() {
        let credential = Credential::agile_with("abc", &[9u8; 16], 20).unwrap();
        let mut p = SheetProtection::locked_with(credential);
        p.ranges.push(ProtectedRange {
            name: "입력 & 칸".to_string(),
            areas: vec![
                CellBox::parse("B2:C3").unwrap(),
                CellBox::parse("E5").unwrap(),
            ],
            credential: Credential::Legacy(0x83AF),
            descriptor: Some("O:S-1".to_string()),
        });
        let xml = sheet_protection_xml(&p);
        assert!(xml.contains("algorithmName=\"SHA-512\"") && xml.contains("spinCount=\"20\""));
        assert!(xml.contains("password=\"83AF\""));
        assert!(!xml.contains("abc"));
        assert_eq!(scan(&xml).unwrap().unwrap(), p);
        let legacy = scan(r#"<sheetProtection sheet="1" password="CC1A"/>"#)
            .unwrap()
            .unwrap();
        assert_eq!(legacy.credential, Credential::Legacy(0xCC1A));
        assert_eq!(
            sheet_protection_xml(&legacy),
            r#"<sheetProtection sheet="1" password="CC1A"/>"#
        );
    }

    #[test]
    fn hostile_attributes_are_kept_or_refused() {
        let p = scan(
            r#"<sheetProtection sheet="1" algorithmName="WHIRLPOOL" hashValue="@@" saltValue="!" spinCount="99999999999"/>"#,
        )
        .unwrap()
        .unwrap();
        let Credential::Agile(a) = &p.credential else {
            panic!("not agile");
        };
        assert_eq!((a.algorithm.as_str(), a.spin), ("WHIRLPOOL", u32::MAX));
        assert_eq!(
            p.credential.verify("x"),
            PasswordCheck::Unsupported(Unsupported::Algorithm)
        );
        let bad = scan(r#"<sheetProtection sheet="1" password="zz"/>"#)
            .unwrap()
            .unwrap();
        assert_eq!(
            bad.credential.verify("x"),
            PasswordCheck::Unsupported(Unsupported::Algorithm)
        );
        let long = "A".repeat(MAX_PROTECT_TEXT_CHARS + 1);
        assert!(scan(&format!(
            r#"<sheetProtection sheet="1" hashValue="{long}"/>"#
        ))
        .is_err());
        assert!(scan(&format!(r#"<protectedRange sqref="A1" name="{long}"/>"#)).is_err());
    }

    #[test]
    fn workbook_protection_round_trips() {
        let credential = Credential::agile_with("pw", &[1u8; 16], 5).unwrap();
        let mut w = WorkbookProtection::structure_with(credential);
        w.lock_windows = true;
        w.revisions = vec![("lockRevision".to_string(), "1".to_string())];
        let xml = workbook_protection_xml(&w);
        assert!(
            xml.contains("workbookAlgorithmName=\"SHA-512\"")
                && xml.contains("lockStructure=\"1\"")
        );
        assert_eq!(
            read_workbook_protection(&format!("<workbook>{xml}</workbook>"))
                .unwrap()
                .unwrap(),
            w
        );
        assert_eq!(read_workbook_protection("<workbook/>").unwrap(), None);
        let legacy = read_workbook_protection(r#"<workbook><workbookProtection workbookPassword="83AF" lockStructure="1"/></workbook>"#)
            .unwrap()
            .unwrap();
        assert_eq!(legacy.credential, Credential::Legacy(0x83AF));
    }
}
