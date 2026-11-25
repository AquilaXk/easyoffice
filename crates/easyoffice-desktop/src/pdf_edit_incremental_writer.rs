//! Incremental PDF byte writer for classic `xref` tables and PDF 1.5+ `/Type /XRef` streams
//! (PD-349).
//!
//! Appends modified and newly allocated indirect objects after `orig.len()` while keeping
//! `0..orig.len()` byte-identical so existing digital signature `/ByteRange` digests remain
//! intact, and writes either sorted `xref` subsections with `/Prev` or a Flate-compressed
//! `/Type /XRef` stream with `/Prev`.

use crate::pdf_edit_budget::{MAX_OBJECTS, MAX_OUTPUT_BYTES};
use crate::pdf_edit_incremental_val::{scan_pdf_writer_chunk, IncVal};
use crate::pdf_edit_incremental_xref::{XRefFormat, XRefMeta};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hayro_syntax::object::ObjectIdentifier;
use hayro_write::pdf_writer::{Chunk, Ref};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write;

/// Appends incremental PDF objects and emits a matching classic `xref` table or `/Type /XRef`
/// stream section without modifying `0..orig.len()`.
pub(crate) struct IncWriter {
    buf: Vec<u8>,
    orig_len: usize,
    meta: XRefMeta,
    next_id: u32,
    updates: BTreeMap<u32, (u16, usize)>,
}

impl IncWriter {
    pub fn new(orig: &[u8], meta: XRefMeta) -> Result<Self, String> {
        if orig.len() > MAX_OUTPUT_BYTES {
            return Err(crate::pdf_edit_build::MSG_TOO_BIG.to_string());
        }
        let next_id = meta.orig_size.max(1);
        Ok(Self {
            buf: orig.to_vec(),
            orig_len: orig.len(),
            meta,
            next_id,
            updates: BTreeMap::new(),
        })
    }

    pub fn peek_next_ref(&self) -> Result<Ref, String> {
        let id = i32::try_from(self.next_id)
            .map_err(|_| "새 PDF 객체 번호가 범위를 벗어났습니다.".to_string())?;
        Ok(Ref::new(id))
    }

    /// Synchronizes next object number after a helper allocated refs starting from `peek_next_ref`.
    pub fn advance_next_to(&mut self, next_ref: Ref) -> Result<(), String> {
        let next_u32 = u32::try_from(next_ref.get())
            .map_err(|_| "새 PDF 객체 번호가 범위를 벗어났습니다.".to_string())?;
        if next_u32 > MAX_OBJECTS as u32 {
            return Err("PDF 객체 수가 허용 한도를 초과합니다.".to_string());
        }
        self.next_id = self.next_id.max(next_u32);
        Ok(())
    }

    pub fn alloc_id(&mut self) -> Result<ObjectIdentifier, String> {
        if self.next_id as usize >= MAX_OBJECTS {
            return Err("PDF 객체 수가 허용 한도를 초과합니다.".to_string());
        }
        let id = ObjectIdentifier::new(self.next_id, 0);
        self.next_id += 1;
        Ok(id)
    }

    pub fn write_val(&mut self, id: ObjectIdentifier, val: &IncVal) -> Result<(), String> {
        if id.obj_number == 0 || id.obj_number as usize >= MAX_OBJECTS {
            return Err("갱신할 PDF 객체 번호가 범위를 벗어났습니다.".to_string());
        }
        self.ensure_line_boundary()?;
        let gen_u16 = u16::try_from(id.gen_number)
            .map_err(|_| "갱신할 PDF 세대 번호가 범위를 벗어났습니다.".to_string())?;
        let offset = self.buf.len();
        let mut head = String::new();
        let _ = writeln!(&mut head, "{} {} obj", id.obj_number, gen_u16);
        self.push_bytes(head.as_bytes())?;
        let mut body = Vec::new();
        val.write_to(&mut body);
        self.push_bytes(&body)?;
        self.push_bytes(b"\nendobj\n")?;
        self.updates.insert(id.obj_number, (gen_u16, offset));
        Ok(())
    }

    pub fn write_stream(
        &mut self,
        id: ObjectIdentifier,
        dict: &BTreeMap<Vec<u8>, IncVal>,
        raw_data: &[u8],
    ) -> Result<(), String> {
        if id.obj_number == 0 || id.obj_number as usize >= MAX_OBJECTS {
            return Err("갱신할 PDF 스트림 번호가 범위를 벗어났습니다.".to_string());
        }
        let mut dict_val = IncVal::Dict(dict.clone());
        let len_i64 = i64::try_from(raw_data.len())
            .map_err(|_| crate::pdf_edit_build::MSG_TOO_BIG.to_string())?;
        dict_val.set(b"Length", IncVal::Int(len_i64));

        self.ensure_line_boundary()?;
        let gen_u16 = u16::try_from(id.gen_number)
            .map_err(|_| "갱신할 PDF 스트림 세대 번호가 범위를 벗어났습니다.".to_string())?;
        let offset = self.buf.len();
        let mut head = String::new();
        let _ = writeln!(&mut head, "{} {} obj", id.obj_number, gen_u16);
        self.push_bytes(head.as_bytes())?;
        let mut body = Vec::new();
        dict_val.write_to(&mut body);
        self.push_bytes(&body)?;
        self.push_bytes(b"\nstream\n")?;
        self.push_bytes(raw_data)?;
        self.push_bytes(b"\nendstream\nendobj\n")?;
        self.updates.insert(id.obj_number, (gen_u16, offset));
        Ok(())
    }

    pub fn append_chunk(&mut self, chunk: &Chunk) -> Result<(), String> {
        let entries = scan_pdf_writer_chunk(chunk)?;
        if entries.is_empty() {
            return Ok(());
        }
        self.ensure_line_boundary()?;
        let base = self.buf.len();
        self.push_bytes(chunk.as_bytes())?;
        for (obj_num, gen_num, local_offset) in entries {
            if obj_num == 0 || obj_num as usize >= MAX_OBJECTS {
                return Err("추가된 PDF 객체 번호가 범위를 벗어났습니다.".to_string());
            }
            self.updates.insert(obj_num, (gen_num, base + local_offset));
        }
        Ok(())
    }

    pub fn orig_len(&self) -> usize {
        self.orig_len
    }

    pub fn finish_update(self) -> Result<Vec<u8>, String> {
        if self.updates.is_empty() {
            return Ok(self.buf);
        }
        match self.meta.format {
            XRefFormat::Table => self.finish_table(),
            XRefFormat::Stream => self.finish_stream(),
        }
    }

    fn finish_table(mut self) -> Result<Vec<u8>, String> {
        self.ensure_line_boundary()?;
        let xref_offset = self.buf.len();
        self.push_bytes(b"xref\n")?;

        let runs = group_contiguous_runs(&self.updates);
        for (start, entries) in runs {
            let mut hdr = String::new();
            let _ = writeln!(&mut hdr, "{start} {}", entries.len());
            self.push_bytes(hdr.as_bytes())?;
            for &(gen, offset) in &entries {
                if offset > 9_999_999_999 {
                    return Err(crate::pdf_edit_build::MSG_TOO_BIG.to_string());
                }
                let line = format!("{offset:010} {gen:05} n \n");
                self.push_bytes(line.as_bytes())?;
            }
        }

        let trailer = self.build_trailer_dict(None);
        self.push_bytes(b"trailer\n")?;
        let mut trailer_bytes = Vec::new();
        trailer.write_to(&mut trailer_bytes);
        self.push_bytes(&trailer_bytes)?;
        let tail = format!("\nstartxref\n{xref_offset}\n%%EOF\n");
        self.push_bytes(tail.as_bytes())?;
        Ok(self.buf)
    }

    fn finish_stream(mut self) -> Result<Vec<u8>, String> {
        self.ensure_line_boundary()?;
        let xref_id = self.alloc_id()?;
        let xref_offset = self.buf.len();
        let xref_gen = u16::try_from(xref_id.gen_number).unwrap_or(0);
        self.updates
            .insert(xref_id.obj_number, (xref_gen, xref_offset));

        let max_offset = self
            .updates
            .values()
            .map(|&(_, off)| off)
            .max()
            .unwrap_or(xref_offset);
        let w1: usize = if u32::try_from(max_offset).is_ok() {
            4
        } else {
            8
        };

        let runs = group_contiguous_runs(&self.updates);
        let mut index_vals = Vec::with_capacity(runs.len() * 2);
        let total_entries: usize = runs.iter().map(|(_, e)| e.len()).sum();
        let mut raw = Vec::with_capacity(total_entries * (1 + w1 + 2));
        for (start, entries) in runs {
            index_vals.push(IncVal::Int(i64::from(start)));
            index_vals.push(IncVal::Int(
                i64::try_from(entries.len()).unwrap_or(i64::MAX),
            ));
            for &(gen, offset) in &entries {
                raw.push(1u8);
                if w1 == 4 {
                    raw.extend_from_slice(&(offset as u32).to_be_bytes());
                } else {
                    raw.extend_from_slice(&(offset as u64).to_be_bytes());
                }
                raw.extend_from_slice(&gen.to_be_bytes());
            }
        }

        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&raw)
            .map_err(|_| "교차 참조 스트림을 압축하지 못했습니다.".to_string())?;
        let compressed = enc
            .finish()
            .map_err(|_| "교차 참조 스트림을 마무리하지 못했습니다.".to_string())?;

        let extra = (compressed.len(), w1, index_vals);
        let dict_val = self.build_trailer_dict(Some(extra));
        let IncVal::Dict(dict_map) = dict_val else {
            unreachable!();
        };
        self.write_stream(xref_id, &dict_map, &compressed)?;
        let tail = format!("startxref\n{xref_offset}\n%%EOF\n");
        self.push_bytes(tail.as_bytes())?;
        Ok(self.buf)
    }

    fn build_trailer_dict(&self, stream_meta: Option<(usize, usize, Vec<IncVal>)>) -> IncVal {
        let mut map = BTreeMap::new();
        map.insert(b"Size".to_vec(), IncVal::Int(i64::from(self.next_id)));
        map.insert(b"Root".to_vec(), IncVal::Ref(self.meta.root_ref));
        map.insert(
            b"Prev".to_vec(),
            IncVal::Int(i64::try_from(self.meta.startxref).unwrap_or(i64::MAX)),
        );
        if let Some(info_ref) = self.meta.info_ref {
            map.insert(b"Info".to_vec(), IncVal::Ref(info_ref));
        }
        if let Some(IncVal::Array(parts)) = &self.meta.id_entry {
            if let Some(IncVal::Str(id0)) = parts.first() {
                let id1 = derive_updated_id(id0, self.buf.len(), self.next_id);
                map.insert(
                    b"ID".to_vec(),
                    IncVal::Array(vec![IncVal::Str(id0.clone()), IncVal::Str(id1)]),
                );
            }
        }
        if let Some((comp_len, w1, index_vals)) = stream_meta {
            map.insert(b"Type".to_vec(), IncVal::Name(b"XRef".to_vec()));
            map.insert(
                b"W".to_vec(),
                IncVal::Array(vec![
                    IncVal::Int(1),
                    IncVal::Int(i64::try_from(w1).unwrap_or(4)),
                    IncVal::Int(2),
                ]),
            );
            map.insert(b"Index".to_vec(), IncVal::Array(index_vals));
            map.insert(b"Filter".to_vec(), IncVal::Name(b"FlateDecode".to_vec()));
            map.insert(
                b"Length".to_vec(),
                IncVal::Int(i64::try_from(comp_len).unwrap_or(i64::MAX)),
            );
        }
        IncVal::Dict(map)
    }

    fn ensure_line_boundary(&mut self) -> Result<(), String> {
        if self.buf.last().is_some_and(|&b| b != b'\n' && b != b'\r') {
            self.push_bytes(b"\n")?;
        }
        Ok(())
    }

    fn push_bytes(&mut self, slice: &[u8]) -> Result<(), String> {
        let next = self
            .buf
            .len()
            .checked_add(slice.len())
            .filter(|&len| len <= MAX_OUTPUT_BYTES)
            .ok_or_else(|| crate::pdf_edit_build::MSG_TOO_BIG.to_string())?;
        self.buf.reserve(next - self.buf.len());
        self.buf.extend_from_slice(slice);
        Ok(())
    }
}

fn group_contiguous_runs(updates: &BTreeMap<u32, (u16, usize)>) -> Vec<(u32, Vec<(u16, usize)>)> {
    let mut runs: Vec<(u32, Vec<(u16, usize)>)> = Vec::new();
    for (&obj_num, &entry) in updates {
        if let Some((start, list)) = runs.last_mut() {
            if let Ok(offset) = u32::try_from(list.len()) {
                if *start + offset == obj_num {
                    list.push(entry);
                    continue;
                }
            }
        }
        runs.push((obj_num, vec![entry]));
    }
    runs
}

fn derive_updated_id(id0: &[u8], len: usize, next_id: u32) -> Vec<u8> {
    let mut h1 = 0xcbf2_9ce4_8422_2325_u64;
    let mut h2 = 0x9e37_79b9_7f4a_7c15_u64;
    for &b in id0 {
        h1 ^= u64::from(b);
        h1 = h1.wrapping_mul(0x100_0000_01b3);
        h2 = h2.wrapping_add(u64::from(b)).rotate_left(13);
    }
    for byte in len.to_le_bytes() {
        h1 ^= u64::from(byte);
        h1 = h1.wrapping_mul(0x100_0000_01b3);
    }
    for byte in next_id.to_le_bytes() {
        h2 ^= u64::from(byte);
        h2 = h2.wrapping_mul(0xff51_afd7_ed55_8ccd);
    }
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&h1.to_be_bytes());
    out.extend_from_slice(&h2.to_be_bytes());
    out
}
