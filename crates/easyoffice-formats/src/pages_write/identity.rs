//! Document identity: `Metadata/Properties.plist` (a binary property list) and
//! `Metadata/DocumentIdentifier`.
//!
//! Every package the writer makes is a new document: new UUIDs, a new version, and a lineage of
//! its own (`stableDocumentUUID` equals `documentUUID` for a document made from nothing).
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig), `FORMAT.md` §11 의 키 목록과
//! `src/metadata.rs`(`write_identity`, 509-545행)의 값 구성. plist 는 공개된 binary plist
//! 형식(`bplist00`)에 맞춰 키·문자열·불리언만 쓰는 최소 작성기를 직접 구현함.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// `fileFormatVersion` of the documents the app writes (also the package's format claim).
const FILE_FORMAT_VERSION: &str = "26.3.1";

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 64 bits that differ between calls, processes and runs: the standard library's randomly keyed
/// hasher over the clock and a counter. Identity values only need to be unique, not secret.
fn entropy() -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    hasher.write_u128(nanos);
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    hasher.finish()
}

/// A version-4 UUID in the apps' form: upper-case and hyphenated.
pub(super) fn uuid() -> String {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&entropy().to_le_bytes());
    bytes[8..].copy_from_slice(&entropy().to_le_bytes());
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A value of the flat dictionary of [`Properties.plist`](properties).
enum Item<'a> {
    Text(&'a str),
    Flag(bool),
}

/// Appends an integer object (`0001nnnn` marker) that holds `value`.
fn push_int(out: &mut Vec<u8>, value: usize) {
    if let Ok(byte) = u8::try_from(value) {
        out.extend_from_slice(&[0x10, byte]);
    } else {
        out.push(0x11);
        out.extend_from_slice(&(value as u16).to_be_bytes());
    }
}

/// Appends a marker with the object count (`0xN` below 15, else `0xF` plus an integer object).
fn push_marker(out: &mut Vec<u8>, kind: u8, count: usize) {
    if count < 15 {
        out.push(kind << 4 | count as u8);
    } else {
        out.push(kind << 4 | 0x0F);
        push_int(out, count);
    }
}

/// A binary property list (`bplist00`): one dictionary of string keys to strings and booleans.
/// At most 127 entries and 65,535 characters per string, which is far above what the fixed
/// key set needs.
fn binary_plist(entries: &[(&str, Item)]) -> Vec<u8> {
    debug_assert!(entries.len() < 128);
    // Objects: the dictionary (0), then every key, then every value.
    let count = 1 + entries.len() * 2;
    let mut body = Vec::new();
    let mut offsets = vec![8usize];
    let mut dict = Vec::new();
    push_marker(&mut dict, 0xD, entries.len());
    for index in 0..entries.len() {
        dict.push((1 + index) as u8);
    }
    for index in 0..entries.len() {
        dict.push((1 + entries.len() + index) as u8);
    }
    body.extend_from_slice(&dict);
    let mut push_object = |bytes: &[u8], body: &mut Vec<u8>| {
        offsets.push(8 + body.len());
        body.extend_from_slice(bytes);
    };
    for (key, _) in entries {
        let mut object = Vec::new();
        push_marker(&mut object, 0x5, key.len());
        object.extend_from_slice(key.as_bytes());
        push_object(&object, &mut body);
    }
    for (_, item) in entries {
        let mut object = Vec::new();
        match item {
            Item::Text(text) => {
                push_marker(&mut object, 0x5, text.len());
                object.extend_from_slice(text.as_bytes());
            }
            Item::Flag(flag) => object.push(if *flag { 0x09 } else { 0x08 }),
        }
        push_object(&object, &mut body);
    }
    let table_at = 8 + body.len();
    let offset_size = if table_at <= 0xFF { 1 } else { 2 };
    let mut out = Vec::with_capacity(table_at + count * offset_size + 32);
    out.extend_from_slice(b"bplist00");
    out.extend_from_slice(&body);
    for offset in &offsets {
        if offset_size == 1 {
            out.push(*offset as u8);
        } else {
            out.extend_from_slice(&(*offset as u16).to_be_bytes());
        }
    }
    out.extend_from_slice(&[0; 6]);
    out.push(offset_size as u8);
    out.push(1); // object reference size
    out.extend_from_slice(&(count as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes()); // top object
    out.extend_from_slice(&(table_at as u64).to_be_bytes());
    out
}

/// `Metadata/Properties.plist` and `Metadata/DocumentIdentifier` of a new document.
pub(super) fn properties() -> (Vec<u8>, Vec<u8>) {
    let document = uuid();
    let version = uuid();
    let private = uuid();
    let revision = format!("0::{version}");
    let plist = binary_plist(&[
        ("documentUUID", Item::Text(&document)),
        ("shareUUID", Item::Text(&document)),
        ("stableDocumentUUID", Item::Text(&document)),
        ("privateUUID", Item::Text(&private)),
        ("versionUUID", Item::Text(&version)),
        ("revision", Item::Text(&revision)),
        ("fileFormatVersion", Item::Text(FILE_FORMAT_VERSION)),
        ("isMultiPage", Item::Flag(false)),
        ("hasExternalReferenceOrMissingData", Item::Flag(false)),
        ("hasUnmaterializedRemoteData", Item::Flag(false)),
    ]);
    (plist, document.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_are_version_4_and_differ() {
        let (a, b) = (uuid(), uuid());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
        assert!(matches!(a.as_bytes()[19], b'8' | b'9' | b'A' | b'B'));
    }

    #[test]
    fn the_plist_trailer_describes_its_objects() {
        let (plist, identifier) = properties();
        assert_eq!(identifier.len(), 36);
        let trailer = &plist[plist.len() - 32..];
        let objects = u64::from_be_bytes(trailer[8..16].try_into().unwrap()) as usize;
        let table_at = u64::from_be_bytes(trailer[24..32].try_into().unwrap()) as usize;
        let offset_size = usize::from(trailer[6]);
        assert_eq!(objects, 21);
        assert_eq!(plist.len(), table_at + objects * offset_size + 32);
        assert_eq!(&plist[..8], b"bplist00");
        // The dictionary starts right after the header and holds ten entries.
        assert_eq!(plist[8], 0xDA);
    }
}
