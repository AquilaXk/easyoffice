//! Direct PDF value representation with generation-preserving object references for
//! ISO 32000-1 incremental updates (PD-349).
//!
//! Unlike `pdf_edit_value::Value` (which stores 32-bit numbers and drops generation numbers on
//! write-out), [`IncVal`] preserves `(obj_number, gen_number)` in indirect references as well as
//! 64-bit integers and 64-bit reals (serialized without exponential notation per ISO 32000-1
//! §7.3.3). Budgets from [`crate::pdf_edit_value::Budget`] bound recursion depth, total nodes and
//! inlined byte lengths (fail-closed).

use crate::pdf_edit_budget::{MAX_VALUE_DEPTH, MAX_VALUE_INLINED_BYTES, MAX_VALUE_NODES};
use crate::pdf_edit_value::{spend_bytes, spend_node, Budget, Refused};
use hayro_syntax::object::{Dict, MaybeRef, Number, Object, ObjectIdentifier};
use hayro_write::pdf_writer::{Chunk, Ref};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const MAX_CHUNK_DEPTH: usize = 256;
const MAX_SCAN_ABS_REAL: f64 = 1e15;

/// An indirect-reference-preserving PDF value used when building updated dictionaries/arrays.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum IncVal {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(Vec<u8>),
    Str(Vec<u8>),
    Ref(ObjectIdentifier),
    Array(Vec<IncVal>),
    Dict(BTreeMap<Vec<u8>, IncVal>),
}

impl IncVal {
    /// Reads a PDF dictionary without resolving indirect entries, enforcing size/depth budgets.
    pub fn read_dict(dict: &Dict<'_>, total: &mut Budget) -> Result<Self, Refused> {
        let mut own = MAX_VALUE_NODES;
        let mut own_bytes = MAX_VALUE_INLINED_BYTES;
        read_dict_inner(dict, 1, &mut own, &mut own_bytes, total)
    }

    /// Reads a raw PDF entry (`MaybeRef<Object>`) preserving indirect object IDs and generations.
    pub fn read_raw(raw: &MaybeRef<Object<'_>>, total: &mut Budget) -> Result<Self, Refused> {
        let mut own = MAX_VALUE_NODES;
        let mut own_bytes = MAX_VALUE_INLINED_BYTES;
        read_raw_inner(raw, 1, &mut own, &mut own_bytes, total)
    }

    pub fn as_dict(&self) -> Option<&BTreeMap<Vec<u8>, IncVal>> {
        match self {
            Self::Dict(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_dict_mut(&mut self) -> Option<&mut BTreeMap<Vec<u8>, IncVal>> {
        match self {
            Self::Dict(map) => Some(map),
            _ => None,
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<&Self> {
        self.as_dict().and_then(|map| map.get(key))
    }

    pub fn get_mut(&mut self, key: &[u8]) -> Option<&mut Self> {
        self.as_dict_mut().and_then(|map| map.get_mut(key))
    }

    pub fn set(&mut self, key: &[u8], val: Self) {
        if let Self::Dict(map) = self {
            map.insert(key.to_vec(), val);
        }
    }

    pub fn remove(&mut self, key: &[u8]) -> Option<Self> {
        self.as_dict_mut().and_then(|map| map.remove(key))
    }

    /// Serializes this value in valid ISO 32000-1 PDF syntax.
    pub fn write_to(&self, out: &mut Vec<u8>) {
        match self {
            Self::Null => out.extend_from_slice(b"null"),
            Self::Bool(true) => out.extend_from_slice(b"true"),
            Self::Bool(false) => out.extend_from_slice(b"false"),
            Self::Int(v) => {
                let mut tmp = String::new();
                let _ = write!(&mut tmp, "{v}");
                out.extend_from_slice(tmp.as_bytes());
            }
            Self::Real(v) => write_pdf_real(*v, out),
            Self::Name(bytes) => write_pdf_name(bytes, out),
            Self::Str(bytes) => write_pdf_str(bytes, out),
            Self::Ref(id) => {
                if id.obj_number == 0 || id.gen_number < 0 || id.gen_number > i32::from(u16::MAX) {
                    out.extend_from_slice(b"null");
                } else {
                    let mut tmp = String::new();
                    let _ = write!(&mut tmp, "{} {} R", id.obj_number, id.gen_number);
                    out.extend_from_slice(tmp.as_bytes());
                }
            }
            Self::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b' ');
                    }
                    item.write_to(out);
                }
                out.push(b']');
            }
            Self::Dict(map) => {
                out.extend_from_slice(b"<<");
                for (key, val) in map {
                    out.push(b' ');
                    write_pdf_name(key, out);
                    out.push(b' ');
                    val.write_to(out);
                }
                out.extend_from_slice(b" >>");
            }
        }
    }
}

/// Encodes a Rust string into ISO 32000-1 PDF text string bytes (ASCII/PDFDoc or UTF-16BE + BOM).
pub(crate) fn encode_pdf_text_bytes(text: &str) -> Vec<u8> {
    if text
        .bytes()
        .all(|b| (0x20..=0x7e).contains(&b) || matches!(b, b'\n' | b'\r' | b'\t'))
    {
        return text.as_bytes().to_vec();
    }
    let mut out = Vec::with_capacity(2 + text.len() * 2);
    out.extend_from_slice(&[0xfe, 0xff]);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

pub(crate) fn write_pdf_name(bytes: &[u8], out: &mut Vec<u8>) {
    out.push(b'/');
    for &b in bytes {
        if matches!(b, b'!'..=b'~')
            && !matches!(
                b,
                b'#' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
            )
        {
            out.push(b);
        } else {
            let mut tmp = String::new();
            let _ = write!(&mut tmp, "#{b:02X}");
            out.extend_from_slice(tmp.as_bytes());
        }
    }
}

fn write_pdf_str(bytes: &[u8], out: &mut Vec<u8>) {
    let ascii_printable = bytes
        .iter()
        .all(|&b| (0x20..=0x7e).contains(&b) || matches!(b, b'\n' | b'\r' | b'\t'));
    if ascii_printable {
        out.push(b'(');
        for &b in bytes {
            match b {
                b'(' | b')' | b'\\' => {
                    out.push(b'\\');
                    out.push(b);
                }
                b'\n' => out.extend_from_slice(b"\\n"),
                b'\r' => out.extend_from_slice(b"\\r"),
                b'\t' => out.extend_from_slice(b"\\t"),
                _ => out.push(b),
            }
        }
        out.push(b')');
    } else {
        out.push(b'<');
        for &b in bytes {
            let mut tmp = String::new();
            let _ = write!(&mut tmp, "{b:02X}");
            out.extend_from_slice(tmp.as_bytes());
        }
        out.push(b'>');
    }
}

fn write_pdf_real(v: f64, out: &mut Vec<u8>) {
    if !v.is_finite() || v == 0.0 {
        out.push(b'0');
        return;
    }
    let mut s = format!("{v:.8}");
    if let Some(dot) = s.find('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.len() == dot + 1 {
            s.pop();
        }
    }
    if s == "-0" {
        out.push(b'0');
    } else {
        out.extend_from_slice(s.as_bytes());
    }
}

fn read_dict_inner(
    dict: &Dict<'_>,
    depth: usize,
    own: &mut usize,
    own_bytes: &mut usize,
    total: &mut Budget,
) -> Result<IncVal, Refused> {
    if depth >= MAX_VALUE_DEPTH {
        return Err(Refused::Deep);
    }
    let mut map = BTreeMap::new();
    for (key, item) in dict.entries() {
        spend_node(own, total)?;
        let key_bytes = key.as_ref();
        spend_bytes(key_bytes.len(), own_bytes, total)?;
        let val = read_raw_inner(&item, depth + 1, own, own_bytes, total)?;
        map.insert(key_bytes.to_vec(), val);
    }
    Ok(IncVal::Dict(map))
}

fn read_raw_inner(
    raw: &MaybeRef<Object<'_>>,
    depth: usize,
    own: &mut usize,
    own_bytes: &mut usize,
    total: &mut Budget,
) -> Result<IncVal, Refused> {
    spend_node(own, total)?;
    let object = match raw {
        MaybeRef::Ref(r) => return Ok(IncVal::Ref((*r).into())),
        MaybeRef::NotRef(obj) => obj,
    };
    read_object_inner(object, depth, own, own_bytes, total)
}

fn read_object_inner(
    object: &Object<'_>,
    depth: usize,
    own: &mut usize,
    own_bytes: &mut usize,
    total: &mut Budget,
) -> Result<IncVal, Refused> {
    match object {
        Object::Null(_) | Object::Stream(_) => Ok(IncVal::Null),
        Object::Boolean(b) => Ok(IncVal::Bool(*b)),
        Object::Number(n) => read_number(n),
        Object::String(s) => {
            let bytes = s.as_bytes();
            spend_bytes(bytes.len(), own_bytes, total)?;
            Ok(IncVal::Str(bytes.to_vec()))
        }
        Object::Name(n) => {
            let bytes = n.as_ref();
            spend_bytes(bytes.len(), own_bytes, total)?;
            Ok(IncVal::Name(bytes.to_vec()))
        }
        Object::Array(array) => {
            if depth >= MAX_VALUE_DEPTH {
                return Err(Refused::Deep);
            }
            let mut items = Vec::new();
            for item in array.raw_iter() {
                items.push(read_raw_inner(&item, depth + 1, own, own_bytes, total)?);
            }
            Ok(IncVal::Array(items))
        }
        Object::Dict(dict) => read_dict_inner(dict, depth, own, own_bytes, total),
    }
}

fn read_number(n: &Number) -> Result<IncVal, Refused> {
    let f = n.as_f64();
    if !f.is_finite() {
        return Err(Refused::Number);
    }
    let i = n.as_i64();
    const EXACT_F64_INT: f64 = 9_007_199_254_740_991.0;
    if f.fract() == 0.0 && f.abs() <= EXACT_F64_INT && (i as f64 - f).abs() < 1.0 {
        return Ok(IncVal::Int(i));
    }
    let formatted = n.to_string();
    if !formatted.contains('.') && !formatted.contains('e') && !formatted.contains('E') {
        return Ok(IncVal::Int(i));
    }
    if f.abs() > MAX_SCAN_ABS_REAL {
        return Err(Refused::Number);
    }
    Ok(IncVal::Real(f))
}

/// Deterministically scans a serialized PDF writer [`Chunk`] and returns `(obj_num, gen_num,
/// byte_offset)` for every indirect object, verifying alignment against [`Chunk::refs`].
pub(crate) fn scan_pdf_writer_chunk(chunk: &Chunk) -> Result<Vec<(u32, u16, usize)>, String> {
    let expected: Vec<Ref> = chunk.refs().collect();
    let bytes = chunk.as_bytes();
    if expected.is_empty() {
        return Ok(Vec::new());
    }
    let mut cur = ChunkCursor { bytes, pos: 0 };
    let mut out = Vec::with_capacity(expected.len());
    for expected_ref in expected {
        cur.skip_ws_and_comments();
        let offset = cur.pos;
        let obj_num = cur.read_u32()?;
        cur.skip_ws_and_comments();
        let gen_num = cur.read_u16()?;
        cur.skip_ws_and_comments();
        cur.expect_word(b"obj")?;
        let expected_id = u32::try_from(expected_ref.get())
            .map_err(|_| "생성된 PDF 객체 번호가 범위를 벗어났습니다.".to_string())?;
        if obj_num != expected_id {
            return Err("생성된 PDF 객체 번호가 일치하지 않습니다.".to_string());
        }
        cur.skip_indirect_body()?;
        out.push((obj_num, gen_num, offset));
    }
    cur.skip_ws_and_comments();
    if cur.pos != bytes.len() {
        return Err("생성된 PDF 청크 끝에 예상하지 못한 바이트가 남았습니다.".to_string());
    }
    Ok(out)
}

struct ChunkCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ChunkCursor<'a> {
    fn is_ws(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x00 | 0x0c)
    }

    fn is_delim(b: u8) -> bool {
        matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    fn skip_ws_and_comments(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if Self::is_ws(b) {
                self.pos += 1;
            } else if b == b'%' {
                while self.pos < self.bytes.len() && !matches!(self.bytes[self.pos], b'\r' | b'\n')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn starts_with_word(&self, word: &[u8]) -> bool {
        let Some(rest) = self.bytes.get(self.pos..) else {
            return false;
        };
        if !rest.starts_with(word) {
            return false;
        }
        rest.get(word.len())
            .is_none_or(|&b| Self::is_ws(b) || Self::is_delim(b))
    }

    fn expect_word(&mut self, word: &[u8]) -> Result<(), String> {
        if !self.starts_with_word(word) {
            return Err("생성된 PDF 객체 구문을 확인할 수 없습니다.".to_string());
        }
        self.pos += word.len();
        Ok(())
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let start = self.pos;
        let mut val = 0u64;
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            val = val
                .checked_mul(10)
                .and_then(|v| v.checked_add(u64::from(self.bytes[self.pos] - b'0')))
                .ok_or_else(|| "생성된 PDF 객체 정수가 너무 큽니다.".to_string())?;
            self.pos += 1;
        }
        if self.pos == start {
            return Err("생성된 PDF 객체에서 숫자를 찾지 못했습니다.".to_string());
        }
        u32::try_from(val).map_err(|_| "생성된 PDF 객체 번호가 범위를 벗어났습니다.".to_string())
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        u16::try_from(self.read_u32()?)
            .map_err(|_| "생성된 PDF 세대 번호가 범위를 벗어났습니다.".to_string())
    }

    fn skip_indirect_body(&mut self) -> Result<(), String> {
        let mut seen_top_dict_len: Option<usize> = None;
        loop {
            self.skip_ws_and_comments();
            if self.pos >= self.bytes.len() {
                return Err("생성된 PDF 객체에 endobj가 없습니다.".to_string());
            }
            if self.starts_with_word(b"endobj") {
                self.pos += b"endobj".len();
                return Ok(());
            }
            if self.starts_with_word(b"stream") {
                let stream_len = seen_top_dict_len.ok_or_else(|| {
                    "생성된 PDF 스트림 객체에 직접 /Length 정수가 없습니다.".to_string()
                })?;
                self.pos += b"stream".len();
                if self.bytes.get(self.pos..self.pos + 2) == Some(b"\r\n") {
                    self.pos += 2;
                } else if matches!(self.bytes.get(self.pos), Some(&b'\n' | &b'\r')) {
                    self.pos += 1;
                } else {
                    return Err("생성된 PDF 스트림 시작 개행이 올바르지 않습니다.".to_string());
                }
                self.pos = self
                    .pos
                    .checked_add(stream_len)
                    .filter(|&end| end <= self.bytes.len())
                    .ok_or_else(|| "생성된 PDF 스트림 길이가 버퍼를 벗어납니다.".to_string())?;
                self.skip_ws_and_comments();
                self.expect_word(b"endstream")?;
                seen_top_dict_len = None;
                continue;
            }
            seen_top_dict_len = self.skip_value(0)?;
        }
    }

    fn skip_value(&mut self, depth: usize) -> Result<Option<usize>, String> {
        if depth > MAX_CHUNK_DEPTH {
            return Err("생성된 PDF 중첩 깊이가 한도를 초과합니다.".to_string());
        }
        self.skip_ws_and_comments();
        let b = *self
            .bytes
            .get(self.pos)
            .ok_or_else(|| "생성된 PDF 청크가 예기치 않게 끝났습니다.".to_string())?;
        if self.bytes[self.pos..].starts_with(b"<<") {
            let len = self.skip_dict(depth + 1)?;
            return Ok(len);
        }
        match b {
            b'(' => {
                self.skip_literal_string()?;
                Ok(None)
            }
            b'<' => {
                self.skip_hex_string()?;
                Ok(None)
            }
            b'[' => {
                self.skip_array(depth + 1)?;
                Ok(None)
            }
            b'/' => {
                self.pos += 1;
                while self.pos < self.bytes.len()
                    && !Self::is_ws(self.bytes[self.pos])
                    && !Self::is_delim(self.bytes[self.pos])
                {
                    self.pos += 1;
                }
                Ok(None)
            }
            _ => {
                let tok_start = self.pos;
                while self.pos < self.bytes.len()
                    && !Self::is_ws(self.bytes[self.pos])
                    && !Self::is_delim(self.bytes[self.pos])
                {
                    self.pos += 1;
                }
                if self.bytes[tok_start..self.pos]
                    .iter()
                    .all(u8::is_ascii_digit)
                {
                    let after_first = self.pos;
                    self.skip_ws_and_comments();
                    let gen_start = self.pos;
                    while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                        self.pos += 1;
                    }
                    if self.pos > gen_start {
                        self.skip_ws_and_comments();
                        if self.starts_with_word(b"R") {
                            self.pos += 1;
                            return Ok(None);
                        }
                    }
                    self.pos = after_first;
                }
                Ok(None)
            }
        }
    }

    fn skip_literal_string(&mut self) -> Result<(), String> {
        self.pos += 1;
        let mut depth = 1usize;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            if b == b'\\' {
                if self.pos < self.bytes.len() {
                    self.pos += 1;
                }
            } else if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
        }
        Err("생성된 PDF 문자열이 닫히지 않았습니다.".to_string())
    }

    fn skip_hex_string(&mut self) -> Result<(), String> {
        self.pos += 1;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            if b == b'>' {
                return Ok(());
            }
        }
        Err("생성된 PDF 16진수 문자열이 닫히지 않았습니다.".to_string())
    }

    fn skip_array(&mut self, depth: usize) -> Result<(), String> {
        if depth > MAX_CHUNK_DEPTH {
            return Err("생성된 PDF 배열 깊이가 한도를 초과합니다.".to_string());
        }
        self.pos += 1;
        loop {
            self.skip_ws_and_comments();
            let b = *self
                .bytes
                .get(self.pos)
                .ok_or_else(|| "생성된 PDF 배열이 닫히지 않았습니다.".to_string())?;
            if b == b']' {
                self.pos += 1;
                return Ok(());
            }
            self.skip_value(depth)?;
        }
    }

    fn skip_dict(&mut self, depth: usize) -> Result<Option<usize>, String> {
        if depth > MAX_CHUNK_DEPTH {
            return Err("생성된 PDF 사전 깊이가 한도를 초과합니다.".to_string());
        }
        self.pos += 2;
        let mut length_val = None;
        loop {
            self.skip_ws_and_comments();
            if self.pos >= self.bytes.len() {
                return Err("생성된 PDF 사전이 닫히지 않았습니다.".to_string());
            }
            if self.bytes[self.pos..].starts_with(b">>") {
                self.pos += 2;
                return Ok(length_val);
            }
            let key = self.read_name_slice()?;
            self.skip_ws_and_comments();
            if key == b"Length" {
                let saved = self.pos;
                if let Some(len) = self.try_read_direct_usize() {
                    length_val = Some(len);
                    continue;
                }
                self.pos = saved;
            }
            self.skip_value(depth)?;
        }
    }

    fn read_name_slice(&mut self) -> Result<&'a [u8], String> {
        if self.bytes.get(self.pos) != Some(&b'/') {
            return Err("생성된 PDF 사전 키가 이름(/)이 아닙니다.".to_string());
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.bytes.len()
            && !Self::is_ws(self.bytes[self.pos])
            && !Self::is_delim(self.bytes[self.pos])
        {
            self.pos += 1;
        }
        Ok(&self.bytes[start..self.pos])
    }

    fn try_read_direct_usize(&mut self) -> Option<usize> {
        let start = self.pos;
        let mut val = 0usize;
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            val = val
                .checked_mul(10)?
                .checked_add(usize::from(self.bytes[self.pos] - b'0'))?;
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        if let Some(&b) = self.bytes.get(self.pos) {
            if !Self::is_ws(b) && !Self::is_delim(b) {
                return None;
            }
        }
        // Reject indirect ref `N G R` by peeking past following token.
        let after_num = self.pos;
        self.skip_ws_and_comments();
        if self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            self.skip_ws_and_comments();
            if self.starts_with_word(b"R") {
                return None;
            }
        }
        self.pos = after_num;
        Some(val)
    }
}
