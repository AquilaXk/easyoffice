//! Minimal protobuf wire-format decoder (no generated code) for the Pages reader.
//!
//! Supports varint, fixed32/fixed64 and length-delimited fields. Start/end groups and reserved
//! wire types are rejected. Every length and offset computation is checked, nesting depth is
//! capped at [`MAX_PROTO_DEPTH`], and field counts are capped per message. Also parses the IWA
//! `ArchiveInfo`/`MessageInfo` envelope and keeps an object table keyed by identifier with
//! cycle-safe traversal helpers. The envelope layout is written from the publicly documented
//! wire format (varint-prefixed ArchiveInfo, then one payload per MessageInfo.length).

use std::collections::{HashMap, HashSet};

use crate::EasyOfficeError;

/// Maximum protobuf nesting depth.
pub const MAX_PROTO_DEPTH: u8 = 32;
/// Maximum number of fields decoded from one message.
pub const MAX_PROTO_FIELDS: usize = 65_536;
/// Maximum `MessageInfo` entries per `ArchiveInfo`.
pub const MAX_MESSAGE_INFOS: usize = 8;
/// Maximum elements read from one packed varint field.
pub const MAX_PACKED_ELEMENTS: usize = 1_000_000;

fn bad(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("Pages protobuf: {detail}"))
}

/// Decodes one varint at `*pos` (advancing it). At most 10 bytes; overflow bits are rejected.
pub fn read_varint(buf: &[u8], pos: &mut usize) -> Result<u64, EasyOfficeError> {
    let b0 = *buf.get(*pos).ok_or_else(|| bad("varint truncated"))?;
    *pos = pos.checked_add(1).ok_or_else(|| bad("offset overflow"))?;
    if b0 < 0x80 {
        return Ok(u64::from(b0));
    }
    let mut value = u64::from(b0 & 0x7F);
    for i in 1..10u32 {
        let byte = *buf.get(*pos).ok_or_else(|| bad("varint truncated"))?;
        *pos = pos.checked_add(1).ok_or_else(|| bad("offset overflow"))?;
        let low = u64::from(byte & 0x7F);
        if i == 9 && low > 1 {
            return Err(bad("varint overflows 64 bits"));
        }
        value |= low << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(bad("varint longer than 10 bytes"))
}

/// Zigzag decoding of a `sint64` varint.
pub fn zigzag_decode(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

/// One decoded field value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireValue<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

/// One decoded field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field<'a> {
    pub number: u32,
    pub value: WireValue<'a>,
}

/// A decoded message: its fields in wire order plus its nesting depth.
#[derive(Debug, Clone, Default)]
pub struct Message<'a> {
    pub fields: Vec<Field<'a>>,
    depth: u8,
}

impl<'a> Message<'a> {
    /// Decodes a top-level message.
    pub fn parse(buf: &'a [u8]) -> Result<Self, EasyOfficeError> {
        Self::parse_at(buf, 0)
    }

    fn parse_at(buf: &'a [u8], depth: u8) -> Result<Self, EasyOfficeError> {
        if depth > MAX_PROTO_DEPTH {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages protobuf 중첩 깊이 한도 초과".to_string(),
            ));
        }
        let mut fields = Vec::new();
        let mut pos = 0usize;
        while pos < buf.len() {
            if fields.len() >= MAX_PROTO_FIELDS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages protobuf 필드 수 한도 초과".to_string(),
                ));
            }
            let key = read_varint(buf, &mut pos)?;
            let number = u32::try_from(key >> 3).map_err(|_| bad("field number too large"))?;
            if number == 0 {
                return Err(bad("field number 0"));
            }
            let value = match key & 7 {
                0 => WireValue::Varint(read_varint(buf, &mut pos)?),
                1 => {
                    let end = pos.checked_add(8).ok_or_else(|| bad("offset overflow"))?;
                    let raw = buf.get(pos..end).ok_or_else(|| bad("fixed64 truncated"))?;
                    pos = end;
                    let mut a = [0u8; 8];
                    a.copy_from_slice(raw);
                    WireValue::Fixed64(u64::from_le_bytes(a))
                }
                2 => {
                    let len = usize::try_from(read_varint(buf, &mut pos)?)
                        .map_err(|_| bad("length too large"))?;
                    let end = pos.checked_add(len).ok_or_else(|| bad("offset overflow"))?;
                    let raw = buf
                        .get(pos..end)
                        .ok_or_else(|| bad("length-delimited truncated"))?;
                    pos = end;
                    WireValue::Bytes(raw)
                }
                5 => {
                    let end = pos.checked_add(4).ok_or_else(|| bad("offset overflow"))?;
                    let raw = buf.get(pos..end).ok_or_else(|| bad("fixed32 truncated"))?;
                    pos = end;
                    let mut a = [0u8; 4];
                    a.copy_from_slice(raw);
                    WireValue::Fixed32(u32::from_le_bytes(a))
                }
                3 | 4 => return Err(bad("group wire types are not supported")),
                _ => return Err(bad("reserved wire type")),
            };
            fields.push(Field { number, value });
        }
        Ok(Self { fields, depth })
    }

    /// First varint value of field `number`.
    pub fn varint(&self, number: u32) -> Option<u64> {
        self.fields.iter().find_map(|f| match f.value {
            WireValue::Varint(v) if f.number == number => Some(v),
            _ => None,
        })
    }

    /// First fixed32 value of field `number`.
    pub fn fixed32(&self, number: u32) -> Option<u32> {
        self.fields.iter().find_map(|f| match f.value {
            WireValue::Fixed32(v) if f.number == number => Some(v),
            _ => None,
        })
    }

    /// First fixed32 value of field `number`, reinterpreted as `f32`.
    pub fn float(&self, number: u32) -> Option<f32> {
        self.fields.iter().find_map(|f| match f.value {
            WireValue::Fixed32(v) if f.number == number => Some(f32::from_bits(v)),
            _ => None,
        })
    }

    /// First length-delimited value of field `number`.
    pub fn bytes(&self, number: u32) -> Option<&'a [u8]> {
        self.fields.iter().find_map(|f| match f.value {
            WireValue::Bytes(b) if f.number == number => Some(b),
            _ => None,
        })
    }

    /// Every length-delimited value of field `number`, in order.
    pub fn all_bytes(&self, number: u32) -> Vec<&'a [u8]> {
        self.fields
            .iter()
            .filter_map(|f| match f.value {
                WireValue::Bytes(b) if f.number == number => Some(b),
                _ => None,
            })
            .collect()
    }

    /// Every varint of repeated field `number`, accepting both unpacked and packed encodings.
    pub fn repeated_varints(&self, number: u32) -> Result<Vec<u64>, EasyOfficeError> {
        let mut out = Vec::new();
        for f in self.fields.iter().filter(|f| f.number == number) {
            match f.value {
                WireValue::Varint(v) => out.push(v),
                WireValue::Bytes(b) => {
                    let mut pos = 0usize;
                    while pos < b.len() {
                        if out.len() >= MAX_PACKED_ELEMENTS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "Pages protobuf packed 요소 수 한도 초과".to_string(),
                            ));
                        }
                        out.push(read_varint(b, &mut pos)?);
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// UTF-8 string of field `number`.
    pub fn string(&self, number: u32) -> Result<Option<String>, EasyOfficeError> {
        self.bytes(number)
            .map(|b| {
                std::str::from_utf8(b)
                    .map(str::to_string)
                    .map_err(|_| EasyOfficeError::InvalidUtf8("Pages protobuf 문자열".to_string()))
            })
            .transpose()
    }

    /// Concatenation of every UTF-8 string value of field `number`.
    pub fn text(&self, number: u32) -> Result<String, EasyOfficeError> {
        let mut out = String::new();
        for part in self.fields.iter().filter(|f| f.number == number) {
            if let WireValue::Bytes(b) = part.value {
                out.push_str(
                    std::str::from_utf8(b)
                        .map_err(|_| EasyOfficeError::InvalidUtf8("Pages 텍스트".to_string()))?,
                );
            }
        }
        Ok(out)
    }

    /// Decodes the first length-delimited value of `number` as a nested message.
    pub fn sub(&self, number: u32) -> Result<Option<Message<'a>>, EasyOfficeError> {
        self.bytes(number)
            .map(|b| Message::parse_at(b, self.depth.saturating_add(1)))
            .transpose()
    }

    /// Decodes every length-delimited value of `number` as nested messages.
    pub fn subs(&self, number: u32) -> Result<Vec<Message<'a>>, EasyOfficeError> {
        self.all_bytes(number)
            .into_iter()
            .map(|b| Message::parse_at(b, self.depth.saturating_add(1)))
            .collect()
    }
}

/// Reads an object reference (`{ identifier = 1 }`) from message bytes in a single zero-allocation pass.
pub fn read_reference(bytes: &[u8]) -> Option<u64> {
    if bytes.len() >= 2 && bytes[0] == 0x08 && bytes[1] < 0x80 {
        return Some(u64::from(bytes[1]));
    }
    let mut pos = 0usize;
    let mut count = 0usize;
    while pos < bytes.len() {
        if count >= MAX_PROTO_FIELDS {
            return None;
        }
        count += 1;
        let key = read_varint(bytes, &mut pos).ok()?;
        let number = u32::try_from(key >> 3).ok()?;
        if number == 0 {
            return None;
        }
        match key & 7 {
            0 => {
                let v = read_varint(bytes, &mut pos).ok()?;
                if number == 1 {
                    return Some(v);
                }
            }
            1 => {
                pos = pos.checked_add(8)?;
                bytes.get(..pos)?;
            }
            2 => {
                let len = usize::try_from(read_varint(bytes, &mut pos).ok()?).ok()?;
                pos = pos.checked_add(len)?;
                bytes.get(..pos)?;
            }
            5 => {
                pos = pos.checked_add(4)?;
                bytes.get(..pos)?;
            }
            _ => return None,
        }
    }
    None
}

/// `MessageInfo` entry of an `ArchiveInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageInfo {
    pub type_id: u32,
    pub length: u32,
    pub object_references: Vec<u64>,
    pub data_references: Vec<u64>,
}

/// IWA record header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchiveInfo {
    pub identifier: u64,
    pub messages: Vec<MessageInfo>,
}

/// Parses an `ArchiveInfo` (`identifier = 1`, repeated `MessageInfo = 2`).
pub fn parse_archive_info(buf: &[u8]) -> Result<ArchiveInfo, EasyOfficeError> {
    let m = Message::parse(buf)?;
    let identifier = m
        .varint(1)
        .ok_or_else(|| bad("ArchiveInfo without identifier"))?;
    let infos = m.subs(2)?;
    if infos.len() > MAX_MESSAGE_INFOS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "ArchiveInfo의 MessageInfo 수 한도 초과".to_string(),
        ));
    }
    let mut messages = Vec::with_capacity(infos.len());
    for mi in infos {
        messages.push(MessageInfo {
            type_id: u32::try_from(
                mi.varint(1)
                    .ok_or_else(|| bad("MessageInfo without type"))?,
            )
            .map_err(|_| bad("type id too large"))?,
            length: u32::try_from(
                mi.varint(3)
                    .ok_or_else(|| bad("MessageInfo without length"))?,
            )
            .map_err(|_| bad("payload length too large"))?,
            object_references: mi.repeated_varints(5)?,
            data_references: mi.repeated_varints(6)?,
        });
    }
    Ok(ArchiveInfo {
        identifier,
        messages,
    })
}

/// One archived object: the first message type and payload, plus any extra messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveObject {
    pub type_id: u32,
    pub payload: Vec<u8>,
    pub extra: Vec<(u32, Vec<u8>)>,
}

/// Objects keyed by identifier.
#[derive(Debug, Default)]
pub struct ObjectTable {
    pub objects: HashMap<u64, ArchiveObject>,
}

impl ObjectTable {
    pub fn get(&self, id: u64) -> Option<&ArchiveObject> {
        self.objects.get(&id)
    }

    /// Identifiers reachable from `root` through `refs_of`, breadth-first, visiting each object
    /// once and stopping at `max_depth` levels or `max_nodes` objects (cycle protection).
    pub fn reachable(
        &self,
        root: u64,
        max_depth: usize,
        max_nodes: usize,
        refs_of: impl Fn(&ArchiveObject) -> Vec<u64>,
    ) -> Vec<u64> {
        let mut seen: HashSet<u64> = HashSet::new();
        let mut order = Vec::new();
        let mut frontier = vec![root];
        for _ in 0..=max_depth {
            let mut next = Vec::new();
            for id in frontier {
                if order.len() >= max_nodes || !seen.insert(id) {
                    continue;
                }
                let Some(obj) = self.objects.get(&id) else {
                    continue;
                };
                order.push(id);
                next.extend(refs_of(obj));
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        order
    }

    /// Follows a parent chain from `start` (`parent_of` returns the next id), ending at the
    /// first repeated id, missing object or after `max_depth` hops.
    pub fn chain(
        &self,
        start: u64,
        max_depth: usize,
        parent_of: impl Fn(&ArchiveObject) -> Option<u64>,
    ) -> Vec<u64> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut cur = Some(start);
        while let Some(id) = cur {
            if out.len() > max_depth || !seen.insert(id) {
                break;
            }
            let Some(obj) = self.objects.get(&id) else {
                break;
            };
            out.push(id);
            cur = parent_of(obj);
        }
        out
    }
}
