//! Cursor over the chunks of an SST record and its CONTINUE records (BIFF8 string table).

use crate::ole2_codec::decode_utf16le;
use crate::EasyOfficeError;

pub(crate) struct SstContinueCursor<'a> {
    chunks: &'a [&'a [u8]],
    chunk_idx: usize,
    pos: usize,
}

impl<'a> SstContinueCursor<'a> {
    pub(crate) fn new(chunks: &'a [&'a [u8]], initial_pos: usize) -> Self {
        Self {
            chunks,
            chunk_idx: 0,
            pos: initial_pos,
        }
    }

    fn advance_if_exhausted(&mut self) -> bool {
        while self.chunk_idx < self.chunks.len() && self.pos >= self.chunks[self.chunk_idx].len() {
            self.chunk_idx += 1;
            self.pos = 0;
        }
        self.chunk_idx < self.chunks.len()
    }

    fn read_raw_bytes(&mut self, mut n: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(n);
        while n > 0 {
            if !self.advance_if_exhausted() {
                return None;
            }
            let chunk = self.chunks[self.chunk_idx];
            let avail = (chunk.len() - self.pos).min(n);
            out.extend_from_slice(&chunk[self.pos..self.pos + avail]);
            self.pos += avail;
            n -= avail;
        }
        Some(out)
    }

    pub(crate) fn skip_raw_bytes(&mut self, mut n: usize) -> bool {
        while n > 0 {
            if !self.advance_if_exhausted() {
                return false;
            }
            let chunk = self.chunks[self.chunk_idx];
            let avail = (chunk.len() - self.pos).min(n);
            self.pos += avail;
            n -= avail;
        }
        true
    }

    pub(crate) fn read_u8(&mut self) -> Option<u8> {
        if !self.advance_if_exhausted() {
            return None;
        }
        let b = self.chunks[self.chunk_idx][self.pos];
        self.pos += 1;
        Some(b)
    }

    pub(crate) fn read_u16(&mut self) -> Option<u16> {
        let b = self.read_raw_bytes(2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn read_u32(&mut self) -> Option<u32> {
        let b = self.read_raw_bytes(4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn read_biff8_chars(
        &mut self,
        mut rem_chars: usize,
        mut is_utf16: bool,
    ) -> Result<String, EasyOfficeError> {
        let mut out = String::with_capacity(rem_chars);
        let mut first_segment = true;
        while rem_chars > 0 {
            if !self.advance_if_exhausted() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "BIFF8 SST 문자열 데이터가 조기 종료되었습니다.".to_string(),
                ));
            }
            if !first_segment && self.pos == 0 {
                let cont_grbit = self.read_u8().ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer(
                        "BIFF8 SST CONTINUE 플래그 바이트 누락".to_string(),
                    )
                })?;
                is_utf16 = (cont_grbit & 0x01) != 0;
            }
            first_segment = false;
            let chunk = self.chunks[self.chunk_idx];
            let avail_bytes = chunk.len().saturating_sub(self.pos);
            if is_utf16 {
                let take_chars = (avail_bytes / 2).min(rem_chars);
                if take_chars == 0 {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "BIFF8 SST UTF-16LE 코드 유닛이 레코드 경계에서 홀수 바이트로 잘렸습니다."
                            .to_string(),
                    ));
                }
                let take_bytes = take_chars * 2;
                let s = decode_utf16le(&chunk[self.pos..self.pos + take_bytes])?;
                out.push_str(&s);
                self.pos += take_bytes;
                rem_chars -= take_chars;
            } else {
                let take_chars = avail_bytes.min(rem_chars);
                if take_chars == 0 {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "BIFF8 SST 8비트 문자열 세그먼트가 비어 있습니다.".to_string(),
                    ));
                }
                for &b in &chunk[self.pos..self.pos + take_chars] {
                    out.push(b as char);
                }
                self.pos += take_chars;
                rem_chars -= take_chars;
            }
        }
        Ok(out)
    }
}
