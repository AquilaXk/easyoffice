//! OLE2 CFBF container reader (directory walk, FAT / MiniFAT stream extraction).

use super::*;
use std::collections::HashMap;

const MINI_SECTOR_SIZE: usize = 64;
const MAX_ENTRY_NAME_BYTES: usize = 64;
const MIN_ENTRY_NAME_BYTES: usize = 2;
const HEADER_SECTOR_SHIFT_OFFSET: usize = 30;
const HEADER_FIRST_DIR_SECTOR_OFFSET: usize = 48;
const HEADER_MINI_CUTOFF_OFFSET: usize = 56;
const HEADER_FIRST_MINIFAT_OFFSET: usize = 60;
const ENTRY_NAME_LEN_OFFSET: usize = 64;
const ENTRY_TYPE_OFFSET: usize = 66;
const ENTRY_LEFT_OFFSET: usize = 68;
const ENTRY_RIGHT_OFFSET: usize = 72;
const ENTRY_CHILD_OFFSET: usize = 76;
const ENTRY_START_OFFSET: usize = 116;
const ENTRY_SIZE_OFFSET: usize = 120;

type SiblingNames = HashMap<String, Vec<String>>;

#[inline]
fn is_live_id(id: u32) -> bool {
    id != Ole2ContainerCodec::FREE_SECTOR && id != Ole2ContainerCodec::END_OF_CHAIN
}

/// Rejects a case-insensitive duplicate sibling name, then records it.
fn register_sibling(
    names: &mut SiblingNames,
    prefix: &str,
    name: &str,
) -> Result<(), EasyOfficeError> {
    let siblings = names.entry(prefix.to_string()).or_default();
    if siblings.iter().any(|existing| {
        Ole2ContainerCodec::cmp_cfb_dir_names(existing, name) == std::cmp::Ordering::Equal
    }) {
        return Err(EasyOfficeError::SecurityViolation(format!(
            "OLE2 디렉터리 내 대소문자 구분 없는 중복 이름이 감지되었습니다: {prefix}{name}"
        )));
    }
    siblings.push(name.to_string());
    Ok(())
}

/// Parsed container state shared by the directory walk and stream extraction.
struct Ole2Reader<'a> {
    bytes: &'a [u8],
    fat: Vec<u32>,
    sector_size: usize,
    mini_cutoff: usize,
    dir_bytes: Vec<u8>,
    entry_count: usize,
    claimed_sectors: SectorBitset,
    mini_stream: Vec<u8>,
    minifat: Vec<u32>,
    claimed_mini_sectors: SectorBitset,
    total_mini_sectors: usize,
}

impl<'a> Ole2Reader<'a> {
    /// Validates the header, loads the directory sectors and (when present) the MiniStream.
    fn open(bytes: &'a [u8]) -> Result<Self, EasyOfficeError> {
        let fat = Ole2SecurityGuard::validate_header_and_fat(bytes)?;
        let sector_size = 1usize << (read_u16_le(bytes, HEADER_SECTOR_SHIFT_OFFSET)? as usize);
        let first_dir_sector = read_u32_le(bytes, HEADER_FIRST_DIR_SECTOR_OFFSET)?;
        let mini_cutoff = read_u32_le(bytes, HEADER_MINI_CUTOFF_OFFSET)? as usize;
        let first_minifat_sector = read_u32_le(bytes, HEADER_FIRST_MINIFAT_OFFSET)?;
        let dir_chain = Ole2SecurityGuard::validate_sector_chain(bytes, &fat, first_dir_sector)?;

        let mut dir_bytes = Vec::with_capacity(dir_chain.len() * sector_size);
        for &sec in &dir_chain {
            let start = (1 + sec as usize) * sector_size;
            dir_bytes.extend_from_slice(&bytes[start..start + sector_size]);
        }
        let entry_count = dir_bytes.len() / Ole2ContainerCodec::DIR_ENTRY_SIZE;
        Self::check_entry_budget(&dir_bytes, entry_count)?;

        let total_sectors = (bytes.len() / sector_size).saturating_sub(1);
        let mut claimed_sectors = SectorBitset::with_capacity(total_sectors);
        for &sec in &dir_chain {
            claimed_sectors.insert(sec);
        }
        let mut reader = Self {
            bytes,
            fat,
            sector_size,
            mini_cutoff,
            dir_bytes,
            entry_count,
            claimed_sectors,
            mini_stream: Vec::new(),
            minifat: Vec::new(),
            claimed_mini_sectors: SectorBitset::with_capacity(0),
            total_mini_sectors: 0,
        };
        if reader.has_root() {
            reader.load_mini_structures(first_minifat_sector)?;
        }
        reader.total_mini_sectors = reader.mini_stream.len().div_ceil(MINI_SECTOR_SIZE);
        reader.claimed_mini_sectors = SectorBitset::with_capacity(reader.total_mini_sectors);
        Ok(reader)
    }

    fn check_entry_budget(dir_bytes: &[u8], entry_count: usize) -> Result<(), EasyOfficeError> {
        let active_entries = (0..entry_count)
            .filter(|&i| dir_bytes[i * Ole2ContainerCodec::DIR_ENTRY_SIZE + ENTRY_TYPE_OFFSET] != 0)
            .count();
        if active_entries > crate::MAX_OLE2_DIR_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "OLE2 디렉터리 엔트리 개수({active_entries})가 보안 상한({})을 초과했습니다.",
                crate::MAX_OLE2_DIR_ENTRIES
            )));
        }
        Ok(())
    }

    fn has_root(&self) -> bool {
        self.entry_count > 0
            && self.dir_bytes[ENTRY_TYPE_OFFSET] == Ole2ContainerCodec::OBJ_TYPE_ROOT
    }

    #[inline]
    fn sector_bytes(&self, sec: u32) -> &'a [u8] {
        let offset = (1 + sec as usize) * self.sector_size;
        &self.bytes[offset..offset + self.sector_size]
    }

    /// Validates the chain starting at `start` and claims every sector exactly once.
    fn claim_chain(
        &mut self,
        start: u32,
        duplicate_message: impl Fn(u32) -> String,
    ) -> Result<Vec<u32>, EasyOfficeError> {
        let chain = Ole2SecurityGuard::validate_sector_chain(self.bytes, &self.fat, start)?;
        for &sec in &chain {
            if !self.claimed_sectors.insert(sec) {
                return Err(EasyOfficeError::SecurityViolation(duplicate_message(sec)));
            }
        }
        Ok(chain)
    }

    fn load_mini_structures(&mut self, first_minifat_sector: u32) -> Result<(), EasyOfficeError> {
        let root_start = read_u32_le(&self.dir_bytes, ENTRY_START_OFFSET)?;
        let root_size = read_u64_le(&self.dir_bytes, ENTRY_SIZE_OFFSET)? as usize;
        if root_size == 0 || !is_live_id(root_start) || !is_live_id(first_minifat_sector) {
            return Ok(());
        }
        if root_size > self.bytes.len() {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 Root Entry MiniStream 크기가 컨테이너를 초과합니다.".to_string(),
            ));
        }
        let root_chain = self.claim_chain(root_start, |_| {
            "OLE2 Root MiniStream 섹터 중복 참조가 감지되었습니다.".to_string()
        })?;
        self.mini_stream
            .reserve(root_chain.len() * self.sector_size);
        for sec in root_chain {
            let slice = self.sector_bytes(sec);
            self.mini_stream.extend_from_slice(slice);
        }
        if self.mini_stream.len() < root_size {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 Root Entry MiniStream 섹터 체인 길이가 부족합니다.".to_string(),
            ));
        }
        self.mini_stream.truncate(root_size);

        let entries_per_sector = self.sector_size / 4;
        let mf_chain = self.claim_chain(first_minifat_sector, |sec| {
            format!("OLE2 MiniFAT 섹터({sec})가 다른 스트림과 중복 점유되었습니다.")
        })?;
        self.minifat.reserve(mf_chain.len() * entries_per_sector);
        for sec in mf_chain {
            let sector = self.sector_bytes(sec);
            for entry_idx in 0..entries_per_sector {
                self.minifat.push(read_u32_le(sector, entry_idx * 4)?);
            }
        }
        Ok(())
    }

    fn read_entry_name(&self, idx: usize) -> Result<String, EasyOfficeError> {
        let base = idx * Ole2ContainerCodec::DIR_ENTRY_SIZE;
        let name_byte_len = read_u16_le(&self.dir_bytes, base + ENTRY_NAME_LEN_OFFSET)? as usize;
        if !(MIN_ENTRY_NAME_BYTES..=MAX_ENTRY_NAME_BYTES).contains(&name_byte_len)
            || name_byte_len % 2 != 0
        {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "OLE2 디렉터리 엔트리 이름 길이 오류: {name_byte_len}"
            )));
        }
        decode_utf16le(&self.dir_bytes[base..base + name_byte_len - 2])
    }

    fn read_stream(
        &mut self,
        idx: usize,
        full_name: String,
    ) -> Result<Ole2StreamEntry, EasyOfficeError> {
        let base = idx * Ole2ContainerCodec::DIR_ENTRY_SIZE;
        let start_sector = read_u32_le(&self.dir_bytes, base + ENTRY_START_OFFSET)?;
        let stream_size = read_u64_le(&self.dir_bytes, base + ENTRY_SIZE_OFFSET)? as usize;
        if stream_size > self.bytes.len() {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "OLE2 스트림('{full_name}') 크기가 컨테이너 크기를 초과합니다."
            )));
        }
        let use_mini = stream_size < self.mini_cutoff
            && !self.mini_stream.is_empty()
            && !self.minifat.is_empty();
        let payload = if stream_size == 0 {
            Vec::new()
        } else if use_mini {
            self.read_mini_payload(start_sector, stream_size, &full_name)?
        } else {
            self.read_regular_payload(start_sector, stream_size, &full_name)?
        };
        Ok(Ole2StreamEntry::new(full_name, payload))
    }

    fn read_mini_payload(
        &mut self,
        start_sector: u32,
        stream_size: usize,
        full_name: &str,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let mut payload =
            Vec::with_capacity(stream_size.div_ceil(MINI_SECTOR_SIZE) * MINI_SECTOR_SIZE);
        let mut cur = start_sector;
        let mut visited = SectorBitset::with_capacity(self.total_mini_sectors);
        while cur != Ole2ContainerCodec::END_OF_CHAIN {
            let m_idx = cur as usize;
            if m_idx >= self.total_mini_sectors || m_idx >= self.minifat.len() {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "OLE2 MiniFAT 체인이 유효하지 않은 미니섹터({cur})를 참조합니다."
                )));
            }
            if !visited.insert(cur) || !self.claimed_mini_sectors.insert(cur) {
                return Err(EasyOfficeError::SecurityViolation(
                    "OLE2 MiniFAT 섹터 순환 또는 공유가 감지되었습니다.".to_string(),
                ));
            }
            let offset = m_idx * MINI_SECTOR_SIZE;
            let end = (offset + MINI_SECTOR_SIZE).min(self.mini_stream.len());
            payload.extend_from_slice(&self.mini_stream[offset..end]);
            cur = self.minifat[m_idx];
        }
        if payload.len() < stream_size {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "OLE2 미니스트림('{full_name}') 체인 길이가 부족합니다."
            )));
        }
        payload.truncate(stream_size);
        Ok(payload)
    }

    fn read_regular_payload(
        &mut self,
        start_sector: u32,
        stream_size: usize,
        full_name: &str,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let chain = self.claim_chain(start_sector, |_| {
            "OLE2 스트림 간 섹터 공유 또는 중복 참조가 감지되었습니다.".to_string()
        })?;
        let mut payload = Vec::with_capacity(chain.len() * self.sector_size);
        for sec in chain {
            payload.extend_from_slice(self.sector_bytes(sec));
        }
        if payload.len() < stream_size {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "OLE2 스트림('{full_name}') 섹터 체인 길이가 부족합니다."
            )));
        }
        payload.truncate(stream_size);
        Ok(payload)
    }

    /// Walks the red-black directory tree from the root's child, collecting streams.
    fn walk_tree(
        &mut self,
        visited_dir: &mut SectorBitset,
        names: &mut SiblingNames,
        streams: &mut Vec<Ole2StreamEntry>,
    ) -> Result<(), EasyOfficeError> {
        visited_dir.insert(0);
        let root_child = read_u32_le(&self.dir_bytes, ENTRY_CHILD_OFFSET)?;
        if !is_live_id(root_child) {
            return Ok(());
        }
        let mut stack: Vec<(u32, String)> = vec![(root_child, String::new())];
        while let Some((dir_id, prefix)) = stack.pop() {
            if is_live_id(dir_id) {
                self.visit_node(dir_id, prefix, &mut stack, visited_dir, names, streams)?;
            }
        }
        Ok(())
    }

    fn visit_node(
        &mut self,
        dir_id: u32,
        prefix: String,
        stack: &mut Vec<(u32, String)>,
        visited_dir: &mut SectorBitset,
        names: &mut SiblingNames,
        streams: &mut Vec<Ole2StreamEntry>,
    ) -> Result<(), EasyOfficeError> {
        let idx = dir_id as usize;
        if idx >= self.entry_count {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "OLE2 디렉터리 트리 인덱스({dir_id})가 범위를 벗어났습니다."
            )));
        }
        if !visited_dir.insert(dir_id) {
            return Err(EasyOfficeError::SecurityViolation(
                "OLE2 디렉터리 트리 순환 참조가 감지되었습니다.".to_string(),
            ));
        }
        let base = idx * Ole2ContainerCodec::DIR_ENTRY_SIZE;
        let obj_type = self.dir_bytes[base + ENTRY_TYPE_OFFSET];
        let left_sib = read_u32_le(&self.dir_bytes, base + ENTRY_LEFT_OFFSET)?;
        let right_sib = read_u32_le(&self.dir_bytes, base + ENTRY_RIGHT_OFFSET)?;
        let child_id = read_u32_le(&self.dir_bytes, base + ENTRY_CHILD_OFFSET)?;
        if is_live_id(right_sib) {
            stack.push((right_sib, prefix.clone()));
        }
        if obj_type == Ole2ContainerCodec::OBJ_TYPE_STORAGE
            || obj_type == Ole2ContainerCodec::OBJ_TYPE_STREAM
        {
            let name = self.read_entry_name(idx)?;
            register_sibling(names, &prefix, &name)?;
            if obj_type == Ole2ContainerCodec::OBJ_TYPE_STORAGE {
                if is_live_id(child_id) {
                    stack.push((child_id, format!("{prefix}{name}/")));
                }
            } else {
                let full_name = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}{name}")
                };
                streams.push(self.read_stream(idx, full_name)?);
            }
        }
        if is_live_id(left_sib) {
            stack.push((left_sib, prefix));
        }
        Ok(())
    }

    /// Fallback for containers without a usable tree: scan every stream entry flatly.
    fn scan_flat(
        &mut self,
        visited_dir: &mut SectorBitset,
        names: &mut SiblingNames,
        streams: &mut Vec<Ole2StreamEntry>,
    ) -> Result<(), EasyOfficeError> {
        for idx in 0..self.entry_count {
            if !visited_dir.insert(idx as u32) {
                continue;
            }
            let base = idx * Ole2ContainerCodec::DIR_ENTRY_SIZE;
            if self.dir_bytes[base + ENTRY_TYPE_OFFSET] != Ole2ContainerCodec::OBJ_TYPE_STREAM {
                continue;
            }
            let name = self.read_entry_name(idx)?;
            register_sibling(names, "", &name)?;
            streams.push(self.read_stream(idx, name)?);
        }
        Ok(())
    }
}

impl Ole2ContainerCodec {
    pub fn unpack(bytes: &[u8]) -> Result<Vec<Ole2StreamEntry>, EasyOfficeError> {
        let mut reader = Ole2Reader::open(bytes)?;
        let mut streams: Vec<Ole2StreamEntry> = Vec::new();
        let mut visited_dir = SectorBitset::with_capacity(reader.entry_count);
        let mut names = SiblingNames::new();
        if reader.has_root() {
            reader.walk_tree(&mut visited_dir, &mut names, &mut streams)?;
        }
        if streams.is_empty() {
            reader.scan_flat(&mut visited_dir, &mut names, &mut streams)?;
        }
        Ok(streams)
    }
}
