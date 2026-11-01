pub use crate::hwp5_codec::*;
pub use crate::legacy_ole2_codec::*;
pub(crate) use crate::rtf_xml_helpers::*;
use crate::{ContainerFamily, EasyOfficeError, FormatDetector, Ole2SecurityGuard};

/// Named stream inside an OLE2 Compound File Binary Format (CFBF) container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ole2StreamEntry {
    pub name: String,
    pub data: Vec<u8>,
}

impl Ole2StreamEntry {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            data,
        }
    }
}

/// 512-byte/4096-byte sector-aligned OLE2 CFBF (`D0 CF 11 E0 A1 B1 1A E1`) container packer and unpacker.
pub struct Ole2ContainerCodec;

#[derive(Debug, Clone)]
struct Ole2DirNode<'a> {
    name: String,
    obj_type: u8,
    children: Vec<usize>,
    data: Option<&'a [u8]>,
    left_sibling: u32,
    right_sibling: u32,
    child_id: u32,
    color: u8,
    start_sector: u32,
    stream_size: u64,
}

impl Ole2ContainerCodec {
    pub const SECTOR_SIZE: usize = 512;
    pub const MINI_SECTOR_SIZE: usize = 64;
    pub const MINI_STREAM_CUTOFF: usize = 4096;
    pub const DIR_ENTRY_SIZE: usize = 128;
    pub const ENTRIES_PER_SECTOR: usize = Self::SECTOR_SIZE / Self::DIR_ENTRY_SIZE;
    pub const FAT_ENTRIES_PER_SECTOR: usize = Self::SECTOR_SIZE / 4;
    pub const DIFAT_ENTRIES_PER_SECTOR: usize = Self::FAT_ENTRIES_PER_SECTOR - 1;
    pub const FAT_SECTOR_MARKER: u32 = 0xFFFF_FFFD;
    pub const DIFAT_SECTOR_MARKER: u32 = 0xFFFF_FFFC;
    pub const END_OF_CHAIN: u32 = Ole2SecurityGuard::END_OF_CHAIN;
    pub const FREE_SECTOR: u32 = Ole2SecurityGuard::FREE_SECTOR;

    const OBJ_TYPE_STORAGE: u8 = 1;
    const OBJ_TYPE_STREAM: u8 = 2;
    const OBJ_TYPE_ROOT: u8 = 5;

    #[inline]
    pub(crate) fn fold_cfb_utf16_unit(u: u16) -> u16 {
        if u >= u16::from(b'a') && u <= u16::from(b'z') {
            u - 32
        } else if u < 0x80 {
            u
        } else if let Some(ch) = char::from_u32(u32::from(u)) {
            let mut upper = ch.to_uppercase();
            if let (Some(first), None) = (upper.next(), upper.next()) {
                if (first as u32) <= 0xFFFF {
                    return first as u16;
                }
            }
            u
        } else {
            u
        }
    }

    pub(crate) fn cmp_cfb_dir_names(a: &str, b: &str) -> std::cmp::Ordering {
        let a_len = a.encode_utf16().count();
        let b_len = b.encode_utf16().count();
        a_len.cmp(&b_len).then_with(|| {
            a.encode_utf16()
                .map(Self::fold_cfb_utf16_unit)
                .cmp(b.encode_utf16().map(Self::fold_cfb_utf16_unit))
        })
    }

    fn build_dir_bst(
        sorted: &[usize],
        nodes: &mut [Ole2DirNode<'_>],
        depth: usize,
        max_depth: usize,
    ) -> u32 {
        if sorted.is_empty() {
            return Self::FREE_SECTOR;
        }
        let mid = sorted.len() / 2;
        let idx = sorted[mid];
        let left = Self::build_dir_bst(&sorted[..mid], nodes, depth + 1, max_depth);
        let right = Self::build_dir_bst(&sorted[mid + 1..], nodes, depth + 1, max_depth);
        nodes[idx].left_sibling = left;
        nodes[idx].right_sibling = right;
        nodes[idx].color = if depth == max_depth && max_depth > 0 {
            0
        } else {
            1
        };
        idx as u32
    }

    pub fn pack(streams: &[(String, Vec<u8>)]) -> Result<Vec<u8>, EasyOfficeError> {
        if streams.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }

        let mut nodes: Vec<Ole2DirNode<'_>> = Vec::with_capacity(1 + streams.len() * 2);
        nodes.push(Ole2DirNode {
            name: "Root Entry".to_string(),
            obj_type: Self::OBJ_TYPE_ROOT,
            children: Vec::new(),
            data: None,
            left_sibling: Self::FREE_SECTOR,
            right_sibling: Self::FREE_SECTOR,
            child_id: Self::FREE_SECTOR,
            color: 1,
            start_sector: Self::END_OF_CHAIN,
            stream_size: 0,
        });

        for (name, data) in streams {
            let trimmed = name.trim().trim_matches('/');
            if trimmed.is_empty() {
                return Err(EasyOfficeError::InvalidArgument(
                    "OLE2 스트림 이름은 비어 있을 수 없습니다.".to_string(),
                ));
            }
            let parts: Vec<&str> = trimmed.split('/').collect();
            let mut parent_idx = 0usize;
            for &part in &parts[..parts.len() - 1] {
                let part_trimmed = part.trim();
                if part_trimmed.is_empty() {
                    return Err(EasyOfficeError::InvalidArgument(
                        "OLE2 스토리지 이름은 비어 있을 수 없습니다.".to_string(),
                    ));
                }
                if nodes[parent_idx].children.iter().any(|&c_idx| {
                    nodes[c_idx].obj_type != Self::OBJ_TYPE_STORAGE
                        && Self::cmp_cfb_dir_names(&nodes[c_idx].name, part_trimmed)
                            == std::cmp::Ordering::Equal
                }) {
                    return Err(EasyOfficeError::SecurityViolation(format!(
                        "OLE2 디렉터리 내 대소문자 구분 없는 이름 충돌이 감지되었습니다: {part_trimmed}"
                    )));
                }
                let existing = nodes[parent_idx].children.iter().copied().find(|&c_idx| {
                    nodes[c_idx].obj_type == Self::OBJ_TYPE_STORAGE
                        && Self::cmp_cfb_dir_names(&nodes[c_idx].name, part_trimmed)
                            == std::cmp::Ordering::Equal
                });
                if let Some(c_idx) = existing {
                    parent_idx = c_idx;
                } else {
                    if nodes.len() >= crate::MAX_OLE2_DIR_ENTRIES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "OLE2 디렉터리 엔트리 개수가 보안 상한({})을 초과했습니다.",
                            crate::MAX_OLE2_DIR_ENTRIES
                        )));
                    }
                    let new_idx = nodes.len();
                    nodes.push(Ole2DirNode {
                        name: part_trimmed.to_string(),
                        obj_type: Self::OBJ_TYPE_STORAGE,
                        children: Vec::new(),
                        data: None,
                        left_sibling: Self::FREE_SECTOR,
                        right_sibling: Self::FREE_SECTOR,
                        child_id: Self::FREE_SECTOR,
                        color: 1,
                        start_sector: 0,
                        stream_size: 0,
                    });
                    nodes[parent_idx].children.push(new_idx);
                    parent_idx = new_idx;
                }
            }

            let leaf_name = parts[parts.len() - 1].trim();
            if leaf_name.is_empty() {
                return Err(EasyOfficeError::InvalidArgument(
                    "OLE2 스트림 이름은 비어 있을 수 없습니다.".to_string(),
                ));
            }
            if nodes[parent_idx].children.iter().any(|&c_idx| {
                Self::cmp_cfb_dir_names(&nodes[c_idx].name, leaf_name) == std::cmp::Ordering::Equal
            }) {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "OLE2 동일 스토리지 내 중복 스트림 이름(대소문자 무시)이 감지되었습니다: {leaf_name}"
                )));
            }
            if nodes.len() >= crate::MAX_OLE2_DIR_ENTRIES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "OLE2 디렉터리 엔트리 개수가 보안 상한({})을 초과했습니다.",
                    crate::MAX_OLE2_DIR_ENTRIES
                )));
            }
            let new_idx = nodes.len();
            nodes.push(Ole2DirNode {
                name: leaf_name.to_string(),
                obj_type: Self::OBJ_TYPE_STREAM,
                children: Vec::new(),
                data: Some(data.as_slice()),
                left_sibling: Self::FREE_SECTOR,
                right_sibling: Self::FREE_SECTOR,
                child_id: Self::FREE_SECTOR,
                color: 1,
                start_sector: Self::END_OF_CHAIN,
                stream_size: data.len() as u64,
            });
            nodes[parent_idx].children.push(new_idx);
        }

        for i in 0..nodes.len() {
            if !nodes[i].children.is_empty() {
                let mut ch = std::mem::take(&mut nodes[i].children);
                ch.sort_by(|&a, &b| Self::cmp_cfb_dir_names(&nodes[a].name, &nodes[b].name));
                let max_depth = (usize::BITS - ch.len().leading_zeros()) as usize - 1;
                let root_child = Self::build_dir_bst(&ch, &mut nodes, 0, max_depth);
                nodes[i].child_id = root_child;
                nodes[i].children = ch;
            }
        }

        let dir_sector_count = nodes.len().div_ceil(Self::ENTRIES_PER_SECTOR).max(1);
        let mut next_sector_idx = dir_sector_count as u32;
        let mut regular_chains: Vec<(u32, usize)> = Vec::new();
        let mut regular_payloads: Vec<&[u8]> = Vec::new();
        let mut mini_stream_bytes: Vec<u8> = Vec::new();
        let mut minifat: Vec<u32> = Vec::new();

        for node in nodes.iter_mut().skip(1) {
            if node.obj_type != Self::OBJ_TYPE_STREAM {
                node.start_sector = 0;
                node.stream_size = 0;
                continue;
            }
            let data = node.data.unwrap_or(&[]);
            node.stream_size = data.len() as u64;
            if data.is_empty() {
                node.start_sector = Self::END_OF_CHAIN;
            } else if data.len() < Self::MINI_STREAM_CUTOFF {
                let m_start = (mini_stream_bytes.len() / Self::MINI_SECTOR_SIZE) as u32;
                let m_count = data.len().div_ceil(Self::MINI_SECTOR_SIZE);
                node.start_sector = m_start;
                for m_off in 0..m_count {
                    let next_m = if m_off + 1 < m_count {
                        m_start + (m_off as u32) + 1
                    } else {
                        Self::END_OF_CHAIN
                    };
                    minifat.push(next_m);
                }
                mini_stream_bytes.extend_from_slice(data);
                let rem = data.len() % Self::MINI_SECTOR_SIZE;
                if rem != 0 {
                    mini_stream_bytes
                        .resize(mini_stream_bytes.len() + (Self::MINI_SECTOR_SIZE - rem), 0);
                }
            } else {
                let count = data.len().div_ceil(Self::SECTOR_SIZE);
                node.start_sector = next_sector_idx;
                regular_chains.push((next_sector_idx, count));
                regular_payloads.push(data);
                next_sector_idx = next_sector_idx.checked_add(count as u32).ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer("OLE2 섹터 인덱스 오버플로".to_string())
                })?;
            }
        }

        let mut minifat_sector_bytes: Vec<u8> = Vec::new();
        let (first_minifat_sector, minifat_sector_count) = if !mini_stream_bytes.is_empty() {
            let root_sec_count = mini_stream_bytes.len().div_ceil(Self::SECTOR_SIZE);
            nodes[0].start_sector = next_sector_idx;
            nodes[0].stream_size = mini_stream_bytes.len() as u64;
            regular_chains.push((next_sector_idx, root_sec_count));
            next_sector_idx = next_sector_idx
                .checked_add(root_sec_count as u32)
                .ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer("OLE2 섹터 인덱스 오버플로".to_string())
                })?;

            let mf_sec_count = minifat.len().div_ceil(Self::FAT_ENTRIES_PER_SECTOR).max(1);
            let mf_start = next_sector_idx;
            regular_chains.push((mf_start, mf_sec_count));
            next_sector_idx = next_sector_idx
                .checked_add(mf_sec_count as u32)
                .ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer("OLE2 섹터 인덱스 오버플로".to_string())
                })?;

            minifat.resize(
                mf_sec_count * Self::FAT_ENTRIES_PER_SECTOR,
                Self::FREE_SECTOR,
            );
            minifat_sector_bytes.reserve(mf_sec_count * Self::SECTOR_SIZE);
            for &entry in &minifat {
                minifat_sector_bytes.extend_from_slice(&entry.to_le_bytes());
            }
            (mf_start, mf_sec_count as u32)
        } else {
            nodes[0].start_sector = Self::END_OF_CHAIN;
            nodes[0].stream_size = 0;
            (Self::END_OF_CHAIN, 0u32)
        };

        let content_sector_count = next_sector_idx as usize;
        let mut fat_sector_count = content_sector_count
            .div_ceil(Self::FAT_ENTRIES_PER_SECTOR)
            .max(1);
        let mut difat_sector_count = 0usize;
        loop {
            let needed_difat = if fat_sector_count > Ole2SecurityGuard::MAX_FAT_SECTORS {
                (fat_sector_count - Ole2SecurityGuard::MAX_FAT_SECTORS)
                    .div_ceil(Self::DIFAT_ENTRIES_PER_SECTOR)
            } else {
                0
            };
            let total_secs = content_sector_count + fat_sector_count + needed_difat;
            let needed_fat = total_secs.div_ceil(Self::FAT_ENTRIES_PER_SECTOR).max(1);
            if needed_fat == fat_sector_count && needed_difat == difat_sector_count {
                break;
            }
            fat_sector_count = needed_fat;
            difat_sector_count = needed_difat;
        }

        let first_fat_sector_idx = content_sector_count;
        let first_difat_sector_idx = content_sector_count + fat_sector_count;
        let mut fat = vec![Self::FREE_SECTOR; fat_sector_count * Self::FAT_ENTRIES_PER_SECTOR];

        for (sec, slot) in fat.iter_mut().enumerate().take(dir_sector_count) {
            *slot = if sec + 1 < dir_sector_count {
                (sec + 1) as u32
            } else {
                Self::END_OF_CHAIN
            };
        }

        for &(start_sec, count) in &regular_chains {
            let start = start_sec as usize;
            for offset in 0..count {
                let cur = start + offset;
                fat[cur] = if offset + 1 < count {
                    (cur + 1) as u32
                } else {
                    Self::END_OF_CHAIN
                };
            }
        }

        for f_idx in 0..fat_sector_count {
            let slot = first_fat_sector_idx + f_idx;
            if slot < fat.len() {
                fat[slot] = Self::FAT_SECTOR_MARKER;
            }
        }
        for d_idx in 0..difat_sector_count {
            let slot = first_difat_sector_idx + d_idx;
            if slot < fat.len() {
                fat[slot] = Self::DIFAT_SECTOR_MARKER;
            }
        }

        let total_sectors = content_sector_count + fat_sector_count + difat_sector_count;
        let mut container = vec![0u8; Self::SECTOR_SIZE];
        container[..8].copy_from_slice(&FormatDetector::OLE2_MAGIC);
        write_u16_le(&mut container, 24, 0x003E); // minor version
        write_u16_le(&mut container, 26, 0x0003); // major version 3
        write_u16_le(&mut container, 28, 0xFFFE); // little-endian byte order
        write_u16_le(&mut container, 30, 0x0009); // sector_shift = 9 (512 bytes)
        write_u16_le(&mut container, 32, 0x0006); // mini_sector_shift = 6 (64 bytes)
        write_u32_le(&mut container, 44, fat_sector_count as u32);
        write_u32_le(&mut container, 48, 0); // first directory sector = 0
        write_u32_le(&mut container, 56, Self::MINI_STREAM_CUTOFF as u32);
        write_u32_le(&mut container, 60, first_minifat_sector);
        write_u32_le(&mut container, 64, minifat_sector_count);
        let first_difat_header = if difat_sector_count > 0 {
            first_difat_sector_idx as u32
        } else {
            Self::END_OF_CHAIN
        };
        write_u32_le(&mut container, 68, first_difat_header);
        write_u32_le(&mut container, 72, difat_sector_count as u32);

        for slot in 0..Ole2SecurityGuard::MAX_FAT_SECTORS {
            let val = if slot < fat_sector_count {
                (first_fat_sector_idx + slot) as u32
            } else {
                Self::FREE_SECTOR
            };
            write_u32_le(&mut container, 76 + slot * 4, val);
        }

        let mut dir_bytes = vec![0u8; dir_sector_count * Self::SECTOR_SIZE];
        for (entry_idx, node) in nodes.iter().enumerate() {
            Self::write_dir_entry(
                &mut dir_bytes,
                entry_idx,
                &node.name,
                node.obj_type,
                node.color,
                node.left_sibling,
                node.right_sibling,
                node.child_id,
                node.start_sector,
                node.stream_size,
            )?;
        }
        container.extend_from_slice(&dir_bytes);

        for data in regular_payloads {
            container.extend_from_slice(data);
            let rem = data.len() % Self::SECTOR_SIZE;
            if rem != 0 {
                container.resize(container.len() + (Self::SECTOR_SIZE - rem), 0);
            }
        }

        if !mini_stream_bytes.is_empty() {
            container.extend_from_slice(&mini_stream_bytes);
            let rem = mini_stream_bytes.len() % Self::SECTOR_SIZE;
            if rem != 0 {
                container.resize(container.len() + (Self::SECTOR_SIZE - rem), 0);
            }
            container.extend_from_slice(&minifat_sector_bytes);
        }

        for &entry in &fat {
            container.extend_from_slice(&entry.to_le_bytes());
        }

        for d_idx in 0..difat_sector_count {
            let mut difat_sec = [0u8; Self::SECTOR_SIZE];
            let base_fat_slot =
                Ole2SecurityGuard::MAX_FAT_SECTORS + d_idx * Self::DIFAT_ENTRIES_PER_SECTOR;
            for slot in 0..Self::DIFAT_ENTRIES_PER_SECTOR {
                let fat_slot = base_fat_slot + slot;
                let val = if fat_slot < fat_sector_count {
                    (first_fat_sector_idx + fat_slot) as u32
                } else {
                    Self::FREE_SECTOR
                };
                write_u32_le(&mut difat_sec, slot * 4, val);
            }
            let next_difat = if d_idx + 1 < difat_sector_count {
                (first_difat_sector_idx + d_idx + 1) as u32
            } else {
                Self::END_OF_CHAIN
            };
            write_u32_le(
                &mut difat_sec,
                Self::DIFAT_ENTRIES_PER_SECTOR * 4,
                next_difat,
            );
            container.extend_from_slice(&difat_sec);
        }

        debug_assert_eq!(container.len(), (1 + total_sectors) * Self::SECTOR_SIZE);
        Ok(container)
    }

    pub fn unpack(bytes: &[u8]) -> Result<Vec<Ole2StreamEntry>, EasyOfficeError> {
        const MINI_SECTOR_SIZE: usize = 64;
        let fat = Ole2SecurityGuard::validate_header_and_fat(bytes)?;
        let sector_shift = read_u16_le(bytes, 30)? as usize;
        let sector_size = 1usize << sector_shift;
        let fat_entries_per_sector = sector_size / 4;
        let first_dir_sector = read_u32_le(bytes, 48)?;
        let mini_cutoff = read_u32_le(bytes, 56)? as usize;
        let first_minifat_sector = read_u32_le(bytes, 60)?;
        let dir_chain = Ole2SecurityGuard::validate_sector_chain(bytes, &fat, first_dir_sector)?;

        let mut dir_bytes = Vec::with_capacity(dir_chain.len() * sector_size);
        for &sec in &dir_chain {
            let start = (1 + sec as usize) * sector_size;
            dir_bytes.extend_from_slice(&bytes[start..start + sector_size]);
        }

        let entry_count = dir_bytes.len() / Self::DIR_ENTRY_SIZE;
        let active_entries = (0..entry_count)
            .filter(|&i| dir_bytes[i * Self::DIR_ENTRY_SIZE + 66] != 0)
            .count();
        if active_entries > crate::MAX_OLE2_DIR_ENTRIES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "OLE2 디렉터리 엔트리 개수({active_entries})가 보안 상한({})을 초과했습니다.",
                crate::MAX_OLE2_DIR_ENTRIES
            )));
        }

        let mut streams: Vec<Ole2StreamEntry> = Vec::new();
        let total_sectors = (bytes.len() / sector_size).saturating_sub(1);
        let mut claimed_sectors = SectorBitset::with_capacity(total_sectors);
        for &sec in &dir_chain {
            claimed_sectors.insert(sec);
        }

        let mut mini_stream: Vec<u8> = Vec::new();
        let mut minifat: Vec<u32> = Vec::new();
        if entry_count > 0 && dir_bytes[66] == Self::OBJ_TYPE_ROOT {
            let root_start = read_u32_le(&dir_bytes, 116)?;
            let root_size = read_u64_le(&dir_bytes, 120)? as usize;
            if root_size > 0
                && root_start != Self::END_OF_CHAIN
                && root_start != Self::FREE_SECTOR
                && first_minifat_sector != Self::END_OF_CHAIN
                && first_minifat_sector != Self::FREE_SECTOR
            {
                if root_size > bytes.len() {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "OLE2 Root Entry MiniStream 크기가 컨테이너를 초과합니다.".to_string(),
                    ));
                }
                let root_chain = Ole2SecurityGuard::validate_sector_chain(bytes, &fat, root_start)?;
                mini_stream.reserve(root_chain.len() * sector_size);
                for sec in root_chain {
                    if !claimed_sectors.insert(sec) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 Root MiniStream 섹터 중복 참조가 감지되었습니다.".to_string(),
                        ));
                    }
                    let offset = (1 + sec as usize) * sector_size;
                    mini_stream.extend_from_slice(&bytes[offset..offset + sector_size]);
                }
                if mini_stream.len() < root_size {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "OLE2 Root Entry MiniStream 섹터 체인 길이가 부족합니다.".to_string(),
                    ));
                }
                mini_stream.truncate(root_size);

                let mf_chain =
                    Ole2SecurityGuard::validate_sector_chain(bytes, &fat, first_minifat_sector)?;
                minifat.reserve(mf_chain.len() * fat_entries_per_sector);
                for sec in mf_chain {
                    if !claimed_sectors.insert(sec) {
                        return Err(EasyOfficeError::SecurityViolation(format!(
                            "OLE2 MiniFAT 섹터({sec})가 다른 스트림과 중복 점유되었습니다."
                        )));
                    }
                    let offset = (1 + sec as usize) * sector_size;
                    for entry_idx in 0..fat_entries_per_sector {
                        minifat.push(read_u32_le(bytes, offset + entry_idx * 4)?);
                    }
                }
            }
        }
        let total_mini_sectors = mini_stream.len().div_ceil(MINI_SECTOR_SIZE);
        let mut claimed_mini_sectors = SectorBitset::with_capacity(total_mini_sectors);
        let mut visited_dir = SectorBitset::with_capacity(entry_count);
        let mut extract_stream = |idx: usize,
                                  full_name: String|
         -> Result<Ole2StreamEntry, EasyOfficeError> {
            let base = idx * Self::DIR_ENTRY_SIZE;
            let start_sector = read_u32_le(&dir_bytes, base + 116)?;
            let stream_size = read_u64_le(&dir_bytes, base + 120)? as usize;

            if stream_size > bytes.len() {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "OLE2 스트림('{full_name}') 크기가 컨테이너 크기를 초과합니다."
                )));
            }

            if stream_size == 0 {
                Ok(Ole2StreamEntry::new(full_name, Vec::new()))
            } else if stream_size < mini_cutoff && !mini_stream.is_empty() && !minifat.is_empty() {
                let mut payload =
                    Vec::with_capacity(stream_size.div_ceil(MINI_SECTOR_SIZE) * MINI_SECTOR_SIZE);
                let mut cur = start_sector;
                let mut visited = SectorBitset::with_capacity(total_mini_sectors);
                while cur != Self::END_OF_CHAIN {
                    let m_idx = cur as usize;
                    if m_idx >= total_mini_sectors || m_idx >= minifat.len() {
                        return Err(EasyOfficeError::CorruptedContainer(format!(
                            "OLE2 MiniFAT 체인이 유효하지 않은 미니섹터({cur})를 참조합니다."
                        )));
                    }
                    if !visited.insert(cur) || !claimed_mini_sectors.insert(cur) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 MiniFAT 섹터 순환 또는 공유가 감지되었습니다.".to_string(),
                        ));
                    }
                    let offset = m_idx * MINI_SECTOR_SIZE;
                    let end = (offset + MINI_SECTOR_SIZE).min(mini_stream.len());
                    payload.extend_from_slice(&mini_stream[offset..end]);
                    cur = minifat[m_idx];
                }
                if payload.len() < stream_size {
                    return Err(EasyOfficeError::CorruptedContainer(format!(
                        "OLE2 미니스트림('{full_name}') 체인 길이가 부족합니다."
                    )));
                }
                payload.truncate(stream_size);
                Ok(Ole2StreamEntry::new(full_name, payload))
            } else {
                let chain = Ole2SecurityGuard::validate_sector_chain(bytes, &fat, start_sector)?;
                let mut payload = Vec::with_capacity(chain.len() * sector_size);
                for sec in chain {
                    if !claimed_sectors.insert(sec) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 스트림 간 섹터 공유 또는 중복 참조가 감지되었습니다.".to_string(),
                        ));
                    }
                    let offset = (1 + sec as usize) * sector_size;
                    payload.extend_from_slice(&bytes[offset..offset + sector_size]);
                }
                if payload.len() < stream_size {
                    return Err(EasyOfficeError::CorruptedContainer(format!(
                        "OLE2 스트림('{full_name}') 섹터 체인 길이가 부족합니다."
                    )));
                }
                payload.truncate(stream_size);
                Ok(Ole2StreamEntry::new(full_name, payload))
            }
        };

        let read_entry_name = |idx: usize| -> Result<String, EasyOfficeError> {
            let base = idx * Self::DIR_ENTRY_SIZE;
            let name_byte_len = read_u16_le(&dir_bytes, base + 64)? as usize;
            if !(2..=64).contains(&name_byte_len) || name_byte_len % 2 != 0 {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "OLE2 디렉터리 엔트리 이름 길이 오류: {name_byte_len}"
                )));
            }
            decode_utf16le(&dir_bytes[base..base + name_byte_len - 2])
        };

        let mut seen_names_per_prefix: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        if entry_count > 0 && dir_bytes[66] == Self::OBJ_TYPE_ROOT {
            visited_dir.insert(0);
            let root_child = read_u32_le(&dir_bytes, 76)?;
            if root_child != Self::FREE_SECTOR && root_child != Self::END_OF_CHAIN {
                let mut stack: Vec<(u32, String)> = vec![(root_child, String::new())];
                while let Some((dir_id, prefix)) = stack.pop() {
                    if dir_id == Self::FREE_SECTOR || dir_id == Self::END_OF_CHAIN {
                        continue;
                    }
                    let idx = dir_id as usize;
                    if idx >= entry_count {
                        return Err(EasyOfficeError::CorruptedContainer(format!(
                            "OLE2 디렉터리 트리 인덱스({dir_id})가 범위를 벗어났습니다."
                        )));
                    }
                    if !visited_dir.insert(dir_id) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 디렉터리 트리 순환 참조가 감지되었습니다.".to_string(),
                        ));
                    }
                    let base = idx * Self::DIR_ENTRY_SIZE;
                    let obj_type = dir_bytes[base + 66];
                    let left_sib = read_u32_le(&dir_bytes, base + 68)?;
                    let right_sib = read_u32_le(&dir_bytes, base + 72)?;
                    let child_id = read_u32_le(&dir_bytes, base + 76)?;

                    if right_sib != Self::FREE_SECTOR && right_sib != Self::END_OF_CHAIN {
                        stack.push((right_sib, prefix.clone()));
                    }

                    if obj_type == Self::OBJ_TYPE_STORAGE || obj_type == Self::OBJ_TYPE_STREAM {
                        let name = read_entry_name(idx)?;
                        let siblings = seen_names_per_prefix.entry(prefix.clone()).or_default();
                        if siblings.iter().any(|existing| {
                            Self::cmp_cfb_dir_names(existing, &name) == std::cmp::Ordering::Equal
                        }) {
                            return Err(EasyOfficeError::SecurityViolation(format!(
                                "OLE2 디렉터리 내 대소문자 구분 없는 중복 이름이 감지되었습니다: {prefix}{name}"
                            )));
                        }
                        siblings.push(name.clone());

                        if obj_type == Self::OBJ_TYPE_STORAGE {
                            if child_id != Self::FREE_SECTOR && child_id != Self::END_OF_CHAIN {
                                stack.push((child_id, format!("{prefix}{name}/")));
                            }
                        } else {
                            let full_name = if prefix.is_empty() {
                                name
                            } else {
                                format!("{prefix}{name}")
                            };
                            streams.push(extract_stream(idx, full_name)?);
                        }
                    }

                    if left_sib != Self::FREE_SECTOR && left_sib != Self::END_OF_CHAIN {
                        stack.push((left_sib, prefix));
                    }
                }
            }
        }
        if streams.is_empty() {
            for idx in 0..entry_count {
                if !visited_dir.insert(idx as u32) {
                    continue;
                }
                let base = idx * Self::DIR_ENTRY_SIZE;
                if dir_bytes[base + 66] != Self::OBJ_TYPE_STREAM {
                    continue;
                }
                let name = read_entry_name(idx)?;
                let siblings = seen_names_per_prefix.entry(String::new()).or_default();
                if siblings
                    .iter()
                    .any(|e| Self::cmp_cfb_dir_names(e, &name) == std::cmp::Ordering::Equal)
                {
                    return Err(EasyOfficeError::SecurityViolation(format!(
                        "OLE2 디렉터리 내 대소문자 구분 없는 중복 이름이 감지되었습니다: {name}"
                    )));
                }
                siblings.push(name.clone());
                streams.push(extract_stream(idx, name)?);
            }
        }
        Ok(streams)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_dir_entry(
        dir_bytes: &mut [u8],
        entry_idx: usize,
        name: &str,
        obj_type: u8,
        color: u8,
        left_sibling_id: u32,
        right_sibling_id: u32,
        child_id: u32,
        start_sector: u32,
        stream_size: u64,
    ) -> Result<(), EasyOfficeError> {
        let base = entry_idx * Self::DIR_ENTRY_SIZE;
        let units: Vec<u16> = name.encode_utf16().collect();
        if units.len() > 31 {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "OLE2 디렉터리 이름 길이(31 UTF-16 코드 유닛) 초과: {name}"
            )));
        }
        for (i, &u) in units.iter().enumerate() {
            write_u16_le(dir_bytes, base + i * 2, u);
        }
        write_u16_le(dir_bytes, base + 64, ((units.len() + 1) * 2) as u16);
        dir_bytes[base + 66] = obj_type;
        dir_bytes[base + 67] = color;
        write_u32_le(dir_bytes, base + 68, left_sibling_id);
        write_u32_le(dir_bytes, base + 72, right_sibling_id);
        write_u32_le(dir_bytes, base + 76, child_id);
        write_u32_le(dir_bytes, base + 116, start_sector);
        write_u64_le(dir_bytes, base + 120, stream_size);
        Ok(())
    }
}

impl Ole2SecurityGuard {
    fn parse_and_validate_sector_size(container_bytes: &[u8]) -> Result<usize, EasyOfficeError> {
        if container_bytes.len() < Self::SECTOR_SIZE * 2 {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 최소 섹터 크기보다 작습니다.".to_string(),
            ));
        }
        if !container_bytes.starts_with(&FormatDetector::OLE2_MAGIC) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "OLE2 CFBF 시그니처 불일치".to_string(),
            });
        }
        let major_ver = read_u16_le(container_bytes, 26)?;
        let sector_shift = read_u16_le(container_bytes, 30)?;
        let mini_shift = read_u16_le(container_bytes, 32)?;
        let valid_v3 = (major_ver == 3 || major_ver == 4) && sector_shift == 9 && mini_shift == 6;
        let valid_v4 = major_ver == 4 && sector_shift == 12 && mini_shift == 6;
        if !valid_v3 && !valid_v4 {
            return Err(EasyOfficeError::CorruptedContainer(format!("유효하지 않은 OLE2 버전/섹터 시프트(v{major_ver}, shift={sector_shift}/{mini_shift})")));
        }
        let sector_size = 1usize << (sector_shift as usize);
        if container_bytes.len() < sector_size * 2 || container_bytes.len() % sector_size != 0 {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 섹터 크기 배수가 아닙니다.".to_string(),
            ));
        }
        Ok(sector_size)
    }

    pub fn validate_header_and_fat(container_bytes: &[u8]) -> Result<Vec<u32>, EasyOfficeError> {
        let sector_size = Self::parse_and_validate_sector_size(container_bytes)?;
        let entries_per_fat_sector = sector_size / 4;
        let difat_entries_per_sector = entries_per_fat_sector - 1;
        let fat_sector_count = read_u32_le(container_bytes, 44)? as usize;
        let first_difat_sector = read_u32_le(container_bytes, 68)?;
        let num_difat_sectors = read_u32_le(container_bytes, 72)? as usize;
        let total_sectors = (container_bytes.len() / sector_size) - 1;

        if fat_sector_count == 0 || fat_sector_count > total_sectors {
            return Err(EasyOfficeError::SecurityViolation(format!("OLE2 FAT 섹터 개수({fat_sector_count})가 허용 범위(1..={total_sectors})를 벗어났습니다.")));
        }
        if fat_sector_count <= Self::MAX_FAT_SECTORS {
            if num_difat_sectors != 0
                || (first_difat_sector != Self::END_OF_CHAIN
                    && first_difat_sector != Self::FREE_SECTOR)
            {
                return Err(EasyOfficeError::SecurityViolation(
                    "OLE2 헤더 DIFAT 섹터 수와 FAT 섹터 수가 불일치합니다.".to_string(),
                ));
            }
        } else {
            let max_difat_capacity = Self::MAX_FAT_SECTORS
                .saturating_add(num_difat_sectors.saturating_mul(difat_entries_per_sector));
            if num_difat_sectors == 0
                || num_difat_sectors > total_sectors
                || first_difat_sector == Self::END_OF_CHAIN
                || first_difat_sector == Self::FREE_SECTOR
                || fat_sector_count > max_difat_capacity
            {
                return Err(EasyOfficeError::SecurityViolation(
                    "OLE2 DIFAT 체인 헤더 정보가 유효하지 않습니다.".to_string(),
                ));
            }
        }

        let mut fat_sector_indices = Vec::with_capacity(fat_sector_count);
        let mut fat_sector_visited = SectorBitset::with_capacity(total_sectors);
        for slot in 0..fat_sector_count.min(Self::MAX_FAT_SECTORS) {
            let fat_sec_u32 = read_u32_le(container_bytes, 76 + slot * 4)?;
            let fat_sec_idx = fat_sec_u32 as usize;
            if fat_sec_idx >= total_sectors {
                return Err(EasyOfficeError::CorruptedContainer(
                    "OLE2 FAT 섹터 인덱스가 파일 범위를 벗어났습니다.".to_string(),
                ));
            }
            if !fat_sector_visited.insert(fat_sec_u32) {
                return Err(EasyOfficeError::SecurityViolation(
                    "OLE2 헤더 DIFAT에 중복된 FAT 섹터 인덱스가 있습니다.".to_string(),
                ));
            }
            fat_sector_indices.push(fat_sec_idx);
        }

        let mut difat_sec = first_difat_sector;
        let mut difat_visited = SectorBitset::with_capacity(total_sectors);
        let mut difat_steps = 0usize;
        while difat_steps < num_difat_sectors {
            if difat_sec == Self::END_OF_CHAIN || difat_sec == Self::FREE_SECTOR {
                return Err(EasyOfficeError::CorruptedContainer(
                    "OLE2 DIFAT 체인이 조기 종료되었습니다.".to_string(),
                ));
            }
            let d_idx = difat_sec as usize;
            if d_idx >= total_sectors
                || !difat_visited.insert(difat_sec)
                || !fat_sector_visited.insert(difat_sec)
            {
                return Err(EasyOfficeError::SecurityViolation(
                    "OLE2 DIFAT 체인이 유효하지 않거나 순환/중복됩니다.".to_string(),
                ));
            }
            let d_offset = (1 + d_idx) * sector_size;
            for slot in 0..difat_entries_per_sector {
                let fat_sec = read_u32_le(container_bytes, d_offset + slot * 4)?;
                if fat_sector_indices.len() < fat_sector_count {
                    if fat_sec == Self::FREE_SECTOR || fat_sec == Self::END_OF_CHAIN {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "OLE2 DIFAT 섹터 내 필수 FAT 인덱스가 비어 있습니다.".to_string(),
                        ));
                    }
                    if (fat_sec as usize) >= total_sectors || !fat_sector_visited.insert(fat_sec) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 DIFAT 내 FAT 섹터 인덱스가 범위를 벗어났거나 중복됩니다."
                                .to_string(),
                        ));
                    }
                    fat_sector_indices.push(fat_sec as usize);
                }
            }
            difat_sec = read_u32_le(container_bytes, d_offset + difat_entries_per_sector * 4)?;
            difat_steps += 1;
        }

        if difat_sec != Self::END_OF_CHAIN && difat_sec != Self::FREE_SECTOR {
            return Err(EasyOfficeError::SecurityViolation(
                "OLE2 DIFAT 체인이 END_OF_CHAIN으로 끝나지 않았습니다.".to_string(),
            ));
        }
        if fat_sector_indices.len() != fat_sector_count {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 FAT 섹터 인덱스 수집 개수가 헤더와 일치하지 않습니다.".to_string(),
            ));
        }

        let mut fat = Vec::with_capacity(fat_sector_indices.len() * entries_per_fat_sector);
        for fat_sec_idx in fat_sector_indices {
            let sec_offset = (1 + fat_sec_idx) * sector_size;
            for entry_idx in 0..entries_per_fat_sector {
                fat.push(read_u32_le(container_bytes, sec_offset + entry_idx * 4)?);
            }
        }

        let first_dir_sector = read_u32_le(container_bytes, 48)?;
        Self::validate_sector_chain(container_bytes, &fat, first_dir_sector)?;
        Ok(fat)
    }

    pub fn validate_sector_chain(
        container_bytes: &[u8],
        fat: &[u32],
        start_sector: u32,
    ) -> Result<Vec<u32>, EasyOfficeError> {
        if container_bytes.len() < Self::SECTOR_SIZE * 2 {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 최소 섹터 크기보다 작습니다.".to_string(),
            ));
        }
        if !container_bytes.starts_with(&FormatDetector::OLE2_MAGIC) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "OLE2 CFBF 헤더 시그니처 불일치".to_string(),
            });
        }
        let sector_shift = read_u16_le(container_bytes, 30)?;
        let sector_size = match sector_shift {
            0 | 9 => Self::SECTOR_SIZE,
            12 => 4096,
            _ => {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "유효하지 않은 OLE2 섹터 시프트({sector_shift})"
                )))
            }
        };
        if container_bytes.len() < sector_size * 2 || container_bytes.len() % sector_size != 0 {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 섹터 크기 배수가 아닙니다.".to_string(),
            ));
        }

        let total_sectors = (container_bytes.len() / sector_size) - 1;
        let fat_sector_count = read_u32_le(container_bytes, 44)? as usize;
        let num_difat_sectors = read_u32_le(container_bytes, 72)? as usize;
        let difat_entries_per_sector = (sector_size / 4) - 1;
        let max_allowed = Self::MAX_FAT_SECTORS
            .saturating_add(num_difat_sectors.saturating_mul(difat_entries_per_sector));
        if fat_sector_count == 0
            || fat_sector_count > total_sectors
            || fat_sector_count > max_allowed
        {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "OLE2 FAT 섹터 개수({fat_sector_count})가 허용 범위를 벗어났습니다."
            )));
        }

        let mut visited = SectorBitset::with_capacity(total_sectors);
        let mut chain: Vec<u32> = Vec::new();
        let mut current = start_sector;

        while current != Self::END_OF_CHAIN {
            let idx = current as usize;
            if idx >= total_sectors || idx >= fat.len() {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "OLE2 FAT 체인이 유효하지 않은 섹터({current})를 참조합니다."
                )));
            }
            if !visited.insert(current) {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "OLE2 FAT 섹터 체인 순환 루프가 감지되었습니다 (sector: {current})."
                )));
            }
            chain.push(current);
            current = fat[idx];
        }

        Ok(chain)
    }

    pub fn validate_hwp5_file_header(header_bytes: &[u8]) -> Result<u32, EasyOfficeError> {
        if header_bytes.len() < Self::HWP5_FILE_HEADER_LEN {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "HWP 5.0 FileHeader 길이({})가 256바이트보다 작습니다.",
                header_bytes.len()
            )));
        }
        if !header_bytes.starts_with(Self::HWP5_SIGNATURE) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "HWP 5.0 'HWP Document File' 시그니처가 없습니다.".to_string(),
            });
        }
        let version = read_u32_le(header_bytes, 32)?;
        let flags = read_u32_le(header_bytes, 36)?;
        if (flags & Self::HWP5_FLAG_ENCRYPTED) != 0 || (flags & Self::HWP5_FLAG_DISTRIBUTION) != 0 {
            return Err(EasyOfficeError::EncryptedDocumentUnsupported(
                crate::DocumentFormat::Hwp,
            ));
        }
        Ok(version)
    }
}
