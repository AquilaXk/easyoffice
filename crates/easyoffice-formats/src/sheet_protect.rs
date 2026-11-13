//! Sheet and workbook protection (PD-216): the allowed-action flags of a protected sheet, the password
//! credential of a sheet, of a range and of the workbook structure, and the checks that compare a typed
//! password with it.
//!
//! Protection is an edit lock, not encryption: the cells stay readable in the file. The password is kept
//! only as the digest the file format defines, never as text.
//!
//! 참조 구현: none for the code. The model follows the standards, and the sets of actions were compared
//! with the open sheet engines: ECMA-376 18.3.1.85 `sheetProtection` (the flags below carry the names and
//! the meaning of its attributes: a flag set means the action is locked), 18.3.1.70 `protectedRange`,
//! 18.2.29 `workbookProtection`, the agile digest of 18.2.28 and MS-OFFCRYPTO 2.3.4.11 (`H0 =
//! hash(salt || password)`, `Hn = hash(H(n-1) || n)`), and the legacy 16-bit hash of ECMA-376 Part 4
//! 14.7.1. Univer (`packages/sheets/src/services/permission/permission-point/worksheet/*.ts`, commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache-2.0) names the same set as one permission point per
//! action (cell value, cell, row and column style, insert and delete of rows and columns, sort, filter,
//! select protected and unprotected cells, hyperlink, extra objects, pivot table); Fortune-sheet
//! (`packages/core/src/modules/protection.ts`, commit 94346608877db4747406707a177c4b8f3bacdbf9, MIT) keeps
//! them as a sheet `authority` with `selectLockedCells`, `selectunLockedCells`, `formatCells` and so on and
//! treats a cell as locked unless it carries its own flag; IronCalc has no protection. The LibreOffice
//! behaviour for `table:protected`, `table:protection-key` and `loext:table-protection` (MPL-2.0; no code
//! taken) guided the ODS side in `ods_protect.rs`.
//!
//! Every size is bounded. A digest with a `spinCount` over [`MAX_SPIN_COUNT`], an unknown algorithm or text
//! that is not base64 stays in the model so a save writes it back, but no password opens it.

use std::time::{Duration, Instant};

use crate::sha_digest::{constant_time_eq, Sha};
use crate::{base64_codec, CellBox};

/// Largest `spinCount` a check runs (a hostile file could ask for billions of rounds).
pub const MAX_SPIN_COUNT: u32 = 10_000_000;
/// The `spinCount` a new password gets (what the spreadsheet programs write).
pub const DEFAULT_SPIN_COUNT: u32 = 100_000;
/// Longest password, in UTF-16 units (the spreadsheet programs stop at 255).
pub const MAX_PASSWORD_UNITS: usize = 255;
/// Longest text of a digest, a salt, an algorithm name or a range name in a file.
pub const MAX_PROTECT_TEXT_CHARS: usize = 1_024;
/// Ranges one protected sheet may allow editing of.
pub const MAX_PROTECTED_RANGES: usize = 1_000;
/// Areas one allowed range may list.
pub const MAX_RANGE_AREAS: usize = 256;
/// The longest a password check runs.
pub const VERIFY_TIME: Duration = Duration::from_secs(10);
const SALT_BYTES: usize = 16;
/// The legacy hash looks at no more than this many characters.
const LEGACY_PASSWORD_CHARS: usize = 15;

/// One action a protected sheet can lock. The bit of an action is set in [`SheetProtection::deny`] when the
/// action is locked; the file attribute of the same name is `1`.
pub const OBJECTS: u16 = 1 << 0;
pub const SCENARIOS: u16 = 1 << 1;
pub const FORMAT_CELLS: u16 = 1 << 2;
pub const FORMAT_COLUMNS: u16 = 1 << 3;
pub const FORMAT_ROWS: u16 = 1 << 4;
pub const INSERT_COLUMNS: u16 = 1 << 5;
pub const INSERT_ROWS: u16 = 1 << 6;
pub const INSERT_HYPERLINKS: u16 = 1 << 7;
pub const DELETE_COLUMNS: u16 = 1 << 8;
pub const DELETE_ROWS: u16 = 1 << 9;
pub const SELECT_LOCKED_CELLS: u16 = 1 << 10;
pub const SORT: u16 = 1 << 11;
pub const AUTO_FILTER: u16 = 1 << 12;
pub const PIVOT_TABLES: u16 = 1 << 13;
pub const SELECT_UNLOCKED_CELLS: u16 = 1 << 14;

/// The attribute name of every flag, in the order of the dialog.
pub const FLAG_NAMES: [(&str, u16); 15] = [
    ("selectLockedCells", SELECT_LOCKED_CELLS),
    ("selectUnlockedCells", SELECT_UNLOCKED_CELLS),
    ("formatCells", FORMAT_CELLS),
    ("formatColumns", FORMAT_COLUMNS),
    ("formatRows", FORMAT_ROWS),
    ("insertColumns", INSERT_COLUMNS),
    ("insertRows", INSERT_ROWS),
    ("insertHyperlinks", INSERT_HYPERLINKS),
    ("deleteColumns", DELETE_COLUMNS),
    ("deleteRows", DELETE_ROWS),
    ("sort", SORT),
    ("autoFilter", AUTO_FILTER),
    ("pivotTables", PIVOT_TABLES),
    ("objects", OBJECTS),
    ("scenarios", SCENARIOS),
];

/// The flags of an attribute that is absent from the file (ECMA-376 18.3.1.85: everything but
/// `objects`, `scenarios` and the two selections is locked).
pub const FILE_DEFAULT_DENY: u16 = FORMAT_CELLS
    | FORMAT_COLUMNS
    | FORMAT_ROWS
    | INSERT_COLUMNS
    | INSERT_ROWS
    | INSERT_HYPERLINKS
    | DELETE_COLUMNS
    | DELETE_ROWS
    | SORT
    | AUTO_FILTER
    | PIVOT_TABLES;
/// The flags of a sheet protected from the dialog without a change: what the spreadsheet programs write
/// (`objects` and `scenarios` locked next to the file defaults).
pub const NEW_SHEET_DENY: u16 = FILE_DEFAULT_DENY | OBJECTS | SCENARIOS;

/// Why a password was not checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    /// The digest names an algorithm this program does not run.
    Algorithm,
    /// The `spinCount` is over [`MAX_SPIN_COUNT`].
    SpinCount,
    /// The salt or the digest is not base64, or the digest has the wrong length.
    Encoding,
    /// The password is longer than [`MAX_PASSWORD_UNITS`].
    TooLong,
    /// The check ran past [`VERIFY_TIME`].
    Time,
}

/// The answer to a password check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordCheck {
    Accepted,
    Rejected,
    Unsupported(Unsupported),
}

/// The agile digest of `sheetProtection`: `algorithmName`, `hashValue`, `saltValue`, `spinCount`. The two
/// texts stay as the file wrote them (base64) so a digest this program cannot check is written back whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgileHash {
    pub algorithm: String,
    pub hash: String,
    pub salt: String,
    pub spin: u32,
}

/// The key of an ODS table or body: `table:protection-key` (base64) and `table:protection-key-digest-algorithm`
/// (an absent attribute is the SHA-1 default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdfKey {
    pub key: String,
    pub algorithm: Option<String>,
}

/// How a protection checks its password.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Credential {
    /// No password: anyone may lift the protection.
    #[default]
    None,
    /// The legacy 16-bit hash (`password="CC1A"`); read, never written for a new password.
    Legacy(u16),
    Agile(AgileHash),
    Odf(OdfKey),
}

fn utf16le(password: &str) -> Vec<u8> {
    password.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The legacy 16-bit hash of a password (ECMA-376 Part 4 14.7.1).
fn legacy_hash(password: &str) -> u16 {
    let units: Vec<u16> = password
        .encode_utf16()
        .take(LEGACY_PASSWORD_CHARS)
        .collect();
    let rotate = |h: u16| ((h >> 14) & 1) | ((h << 1) & 0x7FFF);
    let mut hash = 0u16;
    for &unit in units.iter().rev() {
        hash = rotate(hash) ^ unit;
    }
    rotate(hash) ^ (units.len() as u16) ^ 0xCE4B
}

/// Key bytes of the ODS digest `uri` for `password`: SHA-1 (the default) tries UTF-8 and the older UTF-16LE
/// form; SHA-256 hashes UTF-8. Both URIs of SHA-256 in use are accepted.
fn odf_candidates(uri: Option<&str>, password: &str) -> Option<Vec<Vec<u8>>> {
    match uri {
        None | Some("http://www.w3.org/2000/09/xmldsig#sha1") => Some(vec![
            Sha::Sha1.digest(password.as_bytes()),
            Sha::Sha1.digest(&utf16le(password)),
        ]),
        Some(ODF_SHA256 | "http://www.w3.org/2001/04/xmlenc#sha256") => {
            Some(vec![Sha::Sha256.digest(password.as_bytes())])
        }
        Some(_) => None,
    }
}

/// The digest URI a new ODS key is written with.
pub const ODF_SHA256: &str = "http://www.w3.org/2000/09/xmldsig#sha256";

fn random_salt() -> [u8; SALT_BYTES] {
    use std::hash::BuildHasher;
    let mut salt = [0u8; SALT_BYTES];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    for (i, chunk) in salt.chunks_exact_mut(8).enumerate() {
        // `RandomState` keys come from the operating system's random source.
        let word = std::collections::hash_map::RandomState::new().hash_one((nanos, i));
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    salt
}

impl Credential {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// A new agile digest (SHA-512, a random salt, [`DEFAULT_SPIN_COUNT`] rounds) of `password`; `None` when
    /// the password is empty or longer than [`MAX_PASSWORD_UNITS`].
    pub fn new_agile(password: &str) -> Option<Self> {
        Self::agile_with(password, &random_salt(), DEFAULT_SPIN_COUNT)
    }

    /// [`Self::new_agile`] with a given salt and round count.
    pub(crate) fn agile_with(password: &str, salt: &[u8], spin: u32) -> Option<Self> {
        if password.is_empty() || password.encode_utf16().count() > MAX_PASSWORD_UNITS {
            return None;
        }
        let sha = Sha::Sha512;
        let mut message = salt.to_vec();
        message.extend(utf16le(password));
        let hash = sha.iterate(&sha.digest(&message), spin, None)?;
        Some(Self::Agile(AgileHash {
            algorithm: "SHA-512".to_string(),
            hash: base64_codec::encode(&hash),
            salt: base64_codec::encode(salt),
            spin,
        }))
    }

    /// A new ODS key of `password` (SHA-256 of its UTF-8 bytes); `None` for an empty or too long password.
    pub fn new_odf(password: &str) -> Option<Self> {
        if password.is_empty() || password.encode_utf16().count() > MAX_PASSWORD_UNITS {
            return None;
        }
        Some(Self::Odf(OdfKey {
            key: base64_codec::encode(&Sha::Sha256.digest(password.as_bytes())),
            algorithm: Some(ODF_SHA256.to_string()),
        }))
    }

    /// Whether `password` opens this credential. No digest opens a [`Credential::None`] only by
    /// being accepted: everything is. The time of a digest check does not depend on where the bytes
    /// differ.
    pub fn verify(&self, password: &str) -> PasswordCheck {
        if password.encode_utf16().count() > MAX_PASSWORD_UNITS {
            return PasswordCheck::Unsupported(Unsupported::TooLong);
        }
        match self {
            Self::None => PasswordCheck::Accepted,
            Self::Legacy(hash) => accepted_if(legacy_hash(password) == *hash),
            Self::Agile(agile) => agile.verify(password),
            Self::Odf(odf) => {
                let Some(key) = base64_codec::decode(&odf.key) else {
                    return PasswordCheck::Unsupported(Unsupported::Encoding);
                };
                let Some(candidates) = odf_candidates(odf.algorithm.as_deref(), password) else {
                    return PasswordCheck::Unsupported(Unsupported::Algorithm);
                };
                // Every candidate is compared, so the answer does not depend on which one matches.
                let hits = candidates
                    .iter()
                    .fold(false, |hit, c| constant_time_eq(c, &key) | hit);
                accepted_if(hits)
            }
        }
    }
}

fn accepted_if(matches: bool) -> PasswordCheck {
    if matches {
        PasswordCheck::Accepted
    } else {
        PasswordCheck::Rejected
    }
}

impl AgileHash {
    fn verify(&self, password: &str) -> PasswordCheck {
        let Some(sha) = Sha::from_name(&self.algorithm) else {
            return PasswordCheck::Unsupported(Unsupported::Algorithm);
        };
        if self.spin > MAX_SPIN_COUNT {
            return PasswordCheck::Unsupported(Unsupported::SpinCount);
        }
        let (Some(salt), Some(hash)) = (
            base64_codec::decode(&self.salt),
            base64_codec::decode(&self.hash),
        ) else {
            return PasswordCheck::Unsupported(Unsupported::Encoding);
        };
        if hash.len() != sha.len() {
            return PasswordCheck::Unsupported(Unsupported::Encoding);
        }
        let mut message = salt;
        message.extend(utf16le(password));
        let Some(derived) = sha.iterate(
            &sha.digest(&message),
            self.spin,
            Some(Instant::now() + VERIFY_TIME),
        ) else {
            return PasswordCheck::Unsupported(Unsupported::Time);
        };
        accepted_if(constant_time_eq(&derived, &hash))
    }
}

/// A range of a protected sheet that may be edited: with no password by anyone, with one by whoever
/// types it (ECMA-376 18.3.1.70 `protectedRange`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedRange {
    pub name: String,
    pub areas: Vec<CellBox>,
    pub credential: Credential,
    /// `securityDescriptor` of the file (an access list this program does not use), kept to write back.
    pub descriptor: Option<String>,
}

impl ProtectedRange {
    pub fn contains(&self, row: u32, col: u32) -> bool {
        self.areas
            .iter()
            .any(|a| row >= a.row0 && row <= a.row1 && col >= a.col0 && col <= a.col1)
    }
}

/// The protection of one sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetProtection {
    /// `sheet="1"`: the sheet is protected. A file may hold the other attributes of a sheet that is not.
    pub on: bool,
    /// The locked actions, a set of the flag bits.
    pub deny: u16,
    pub credential: Credential,
    pub ranges: Vec<ProtectedRange>,
}

impl Default for SheetProtection {
    fn default() -> Self {
        Self {
            on: false,
            deny: FILE_DEFAULT_DENY,
            credential: Credential::None,
            ranges: Vec::new(),
        }
    }
}

impl SheetProtection {
    /// A sheet protected from the dialog: the usual locked actions and the given credential.
    pub fn locked_with(credential: Credential) -> Self {
        Self {
            on: true,
            deny: NEW_SHEET_DENY,
            credential,
            ranges: Vec::new(),
        }
    }

    /// Nothing to write or keep: not protected, the file's flags, no password and no ranges.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the action `flag` is allowed (always, on a sheet that is not protected).
    pub fn allows(&self, flag: u16) -> bool {
        !self.on || self.deny & flag == 0
    }

    /// Whether the cell may be changed: its sheet is not protected, the cell is unlocked, or an allowed range
    /// without a password holds it. `unlocked` is the cell's own flag.
    pub fn cell_editable(&self, row: u32, col: u32, unlocked: bool) -> bool {
        !self.on
            || unlocked
            || self
                .ranges
                .iter()
                .any(|r| r.credential.is_none() && r.contains(row, col))
    }

    /// Whether every bound holds.
    pub fn within_bounds(&self) -> bool {
        self.ranges.len() <= MAX_PROTECTED_RANGES
            && self.ranges.iter().all(|r| {
                r.areas.len() <= MAX_RANGE_AREAS
                    && r.name.chars().count() <= MAX_PROTECT_TEXT_CHARS
                    && credential_within_bounds(&r.credential)
            })
            && credential_within_bounds(&self.credential)
    }
}

/// Whether the texts of a credential are within [`MAX_PROTECT_TEXT_CHARS`].
pub fn credential_within_bounds(credential: &Credential) -> bool {
    let fits = |s: &str| s.chars().count() <= MAX_PROTECT_TEXT_CHARS;
    match credential {
        Credential::None | Credential::Legacy(_) => true,
        Credential::Agile(a) => fits(&a.algorithm) && fits(&a.hash) && fits(&a.salt),
        Credential::Odf(o) => fits(&o.key) && o.algorithm.as_deref().is_none_or(fits),
    }
}

/// The protection of the workbook structure (`workbookProtection`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkbookProtection {
    /// Sheets cannot be added, moved, renamed, copied, hidden or deleted.
    pub lock_structure: bool,
    /// The windows of the workbook keep their position and size.
    pub lock_windows: bool,
    pub credential: Credential,
    /// The raw attributes of `lockRevision` and the revisions password, kept to write back.
    pub revisions: Vec<(String, String)>,
}

impl WorkbookProtection {
    /// A workbook protected from the dialog: the structure locked with the given credential.
    pub fn structure_with(credential: Credential) -> Self {
        Self {
            lock_structure: true,
            credential,
            ..Self::default()
        }
    }

    /// Nothing locked and no password.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn within_bounds(&self) -> bool {
        credential_within_bounds(&self.credential)
            && self.revisions.len() <= 16
            && self
                .revisions
                .iter()
                .all(|(k, v)| k.len() <= 64 && v.chars().count() <= MAX_PROTECT_TEXT_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_hash_of_known_passwords() {
        assert_eq!(legacy_hash("password"), 0x83AF);
        assert_eq!(
            Credential::Legacy(0x83AF).verify("password"),
            PasswordCheck::Accepted
        );
        assert_eq!(
            Credential::Legacy(0x83AF).verify("Password"),
            PasswordCheck::Rejected
        );
    }

    #[test]
    fn agile_digest_round_trips_and_rejects() {
        let credential = Credential::agile_with("비밀번호1!", &[7u8; 16], 50).unwrap();
        assert_eq!(credential.verify("비밀번호1!"), PasswordCheck::Accepted);
        assert_eq!(credential.verify("비밀번호1"), PasswordCheck::Rejected);
        assert_eq!(credential.verify(""), PasswordCheck::Rejected);
    }

    #[test]
    fn agile_digest_matches_the_scheme() {
        // Same value from an independent computation of the loop (checked against a reference run).
        let sha = Sha::Sha512;
        let mut m = vec![1u8, 2, 3, 4];
        m.extend(utf16le("abc"));
        let mut h = sha.digest(&m);
        for i in 0u32..3 {
            let mut next = h.clone();
            next.extend_from_slice(&i.to_le_bytes());
            h = sha.digest(&next);
        }
        let Some(Credential::Agile(a)) = Credential::agile_with("abc", &[1, 2, 3, 4], 3) else {
            panic!("no digest");
        };
        assert_eq!(a.hash, base64_codec::encode(&h));
        assert_eq!(a.spin, 3);
        assert_eq!(a.algorithm, "SHA-512");
    }

    #[test]
    fn hostile_digests_are_kept_and_refuse_every_password() {
        let digest = |algorithm: &str, hash: &str, salt: &str, spin: u32| {
            Credential::Agile(AgileHash {
                algorithm: algorithm.to_string(),
                hash: hash.to_string(),
                salt: salt.to_string(),
                spin,
            })
        };
        assert_eq!(
            digest("SHA-512", "AAAA", "AAAA", u32::MAX).verify("x"),
            PasswordCheck::Unsupported(Unsupported::SpinCount)
        );
        assert_eq!(
            digest("SHA-512", "AAAA", "AAAA", MAX_SPIN_COUNT + 1).verify("x"),
            PasswordCheck::Unsupported(Unsupported::SpinCount)
        );
        assert_eq!(
            digest("WHIRLPOOL", "AAAA", "AAAA", 1).verify("x"),
            PasswordCheck::Unsupported(Unsupported::Algorithm)
        );
        assert_eq!(
            digest("SHA-512", "not base64!", "AAAA", 1).verify("x"),
            PasswordCheck::Unsupported(Unsupported::Encoding)
        );
        assert_eq!(
            digest("SHA-512", "AAAA", "@@@@", 1).verify("x"),
            PasswordCheck::Unsupported(Unsupported::Encoding)
        );
        // A digest of the wrong length can never match.
        assert_eq!(
            digest("SHA-512", "AAAA", "AAAA", 1).verify("x"),
            PasswordCheck::Unsupported(Unsupported::Encoding)
        );
    }

    #[test]
    fn password_length_and_emptiness() {
        assert!(Credential::new_agile("").is_none());
        assert!(Credential::new_odf("").is_none());
        let long = "a".repeat(MAX_PASSWORD_UNITS + 1);
        assert!(Credential::new_agile(&long).is_none());
        let edge = "a".repeat(MAX_PASSWORD_UNITS);
        assert!(Credential::agile_with(&edge, &[0; 16], 1).is_some());
        assert_eq!(
            Credential::Legacy(1).verify(&long),
            PasswordCheck::Unsupported(Unsupported::TooLong)
        );
        // A very long password costs one scan, not a hash of it.
        let huge = "x".repeat(10_000_000);
        assert_eq!(
            Credential::None.verify(&huge),
            PasswordCheck::Unsupported(Unsupported::TooLong)
        );
    }

    #[test]
    fn odf_keys_of_both_digests() {
        let key = Credential::new_odf("암호").unwrap();
        assert_eq!(key.verify("암호"), PasswordCheck::Accepted);
        assert_eq!(key.verify("암"), PasswordCheck::Rejected);
        let sha1_utf8 = Credential::Odf(OdfKey {
            key: base64_codec::encode(&Sha::Sha1.digest(b"abc")),
            algorithm: None,
        });
        assert_eq!(sha1_utf8.verify("abc"), PasswordCheck::Accepted);
        let sha1_utf16 = Credential::Odf(OdfKey {
            key: base64_codec::encode(&Sha::Sha1.digest(&utf16le("abc"))),
            algorithm: Some("http://www.w3.org/2000/09/xmldsig#sha1".to_string()),
        });
        assert_eq!(sha1_utf16.verify("abc"), PasswordCheck::Accepted);
        let other = Credential::Odf(OdfKey {
            key: "AAAA".to_string(),
            algorithm: Some(
                "http://docs.oasis-open.org/office/ns/table/legacy-hash-excel".to_string(),
            ),
        });
        assert_eq!(
            other.verify("abc"),
            PasswordCheck::Unsupported(Unsupported::Algorithm)
        );
    }

    #[test]
    fn allowed_actions_and_cells() {
        let mut p = SheetProtection::locked_with(Credential::None);
        assert!(!p.allows(INSERT_ROWS));
        assert!(p.allows(SELECT_LOCKED_CELLS));
        p.deny &= !INSERT_ROWS;
        assert!(p.allows(INSERT_ROWS));
        assert!(!p.cell_editable(0, 0, false));
        assert!(p.cell_editable(0, 0, true));
        p.ranges.push(ProtectedRange {
            name: "입력".to_string(),
            areas: vec![CellBox::parse("B2:C3").unwrap()],
            credential: Credential::None,
            descriptor: None,
        });
        assert!(p.cell_editable(1, 1, false));
        assert!(!p.cell_editable(3, 3, false));
        // A range with a password is not open without it.
        p.ranges[0].credential = Credential::Legacy(1);
        assert!(!p.cell_editable(1, 1, false));
        p.on = false;
        assert!(p.cell_editable(1, 1, false));
        assert!(SheetProtection::default().is_default());
    }
}
