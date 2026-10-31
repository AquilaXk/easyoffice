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

/// 512-byte sector-aligned OLE2 CFBF (`D0 CF 11 E0 A1 B1 1A E1`) container packer and unpacker.
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
    pub const FAT_SECTOR_MARKER: u32 = 0xFFFF_FFFD;
    pub const END_OF_CHAIN: u32 = Ole2SecurityGuard::END_OF_CHAIN;
    pub const FREE_SECTOR: u32 = Ole2SecurityGuard::FREE_SECTOR;

    const OBJ_TYPE_STORAGE: u8 = 1;
    const OBJ_TYPE_STREAM: u8 = 2;
    const OBJ_TYPE_ROOT: u8 = 5;

    #[inline]
    fn fold_cfb_utf16_unit(u: u16) -> u16 {
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

    fn cmp_cfb_dir_names(a: &str, b: &str) -> std::cmp::Ordering {
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
                let existing = nodes[parent_idx].children.iter().copied().find(|&c_idx| {
                    nodes[c_idx].obj_type == Self::OBJ_TYPE_STORAGE
                        && nodes[c_idx].name.eq_ignore_ascii_case(part_trimmed)
                });
                if let Some(c_idx) = existing {
                    parent_idx = c_idx;
                } else {
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
        let fat_sector_count = content_sector_count
            .div_ceil(Self::FAT_ENTRIES_PER_SECTOR)
            .max(1);
        if fat_sector_count > Ole2SecurityGuard::MAX_FAT_SECTORS {
            return Err(EasyOfficeError::SecurityViolation(
                "OLE2 DIFAT 109 섹터 한도를 초과했습니다.".to_string(),
            ));
        }

        let first_fat_sector_idx = content_sector_count;
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

        let total_sectors = content_sector_count + fat_sector_count;
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
        write_u32_le(&mut container, 68, Self::END_OF_CHAIN);
        write_u32_le(&mut container, 72, 0);

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

        debug_assert_eq!(container.len(), (1 + total_sectors) * Self::SECTOR_SIZE);
        Ok(container)
    }

    pub fn unpack(bytes: &[u8]) -> Result<Vec<Ole2StreamEntry>, EasyOfficeError> {
        const MINI_SECTOR_SIZE: usize = 64;
        let fat = Ole2SecurityGuard::validate_header_and_fat(bytes)?;
        let first_dir_sector = read_u32_le(bytes, 48)?;
        let mini_cutoff = read_u32_le(bytes, 56)? as usize;
        let first_minifat_sector = read_u32_le(bytes, 60)?;
        let dir_chain = Ole2SecurityGuard::validate_sector_chain(bytes, &fat, first_dir_sector)?;

        let mut dir_bytes = Vec::with_capacity(dir_chain.len() * Self::SECTOR_SIZE);
        for &sec in &dir_chain {
            let start = (1 + sec as usize) * Self::SECTOR_SIZE;
            dir_bytes.extend_from_slice(&bytes[start..start + Self::SECTOR_SIZE]);
        }

        let entry_count = dir_bytes.len() / Self::DIR_ENTRY_SIZE;
        let mut streams: Vec<Ole2StreamEntry> = Vec::new();
        let total_sectors = (bytes.len() / Self::SECTOR_SIZE).saturating_sub(1);
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
                mini_stream.reserve(root_chain.len() * Self::SECTOR_SIZE);
                for sec in root_chain {
                    if !claimed_sectors.insert(sec) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 Root MiniStream 섹터 중복 참조가 감지되었습니다.".to_string(),
                        ));
                    }
                    let offset = (1 + sec as usize) * Self::SECTOR_SIZE;
                    mini_stream.extend_from_slice(&bytes[offset..offset + Self::SECTOR_SIZE]);
                }
                if mini_stream.len() < root_size {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "OLE2 Root Entry MiniStream 섹터 체인 길이가 부족합니다.".to_string(),
                    ));
                }
                mini_stream.truncate(root_size);

                let mf_chain =
                    Ole2SecurityGuard::validate_sector_chain(bytes, &fat, first_minifat_sector)?;
                minifat.reserve(mf_chain.len() * Self::FAT_ENTRIES_PER_SECTOR);
                for sec in mf_chain {
                    let offset = (1 + sec as usize) * Self::SECTOR_SIZE;
                    for entry_idx in 0..Self::FAT_ENTRIES_PER_SECTOR {
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
                let mut payload = Vec::with_capacity(chain.len() * Self::SECTOR_SIZE);
                for sec in chain {
                    if !claimed_sectors.insert(sec) {
                        return Err(EasyOfficeError::SecurityViolation(
                            "OLE2 스트림 간 섹터 공유 또는 중복 참조가 감지되었습니다.".to_string(),
                        ));
                    }
                    let offset = (1 + sec as usize) * Self::SECTOR_SIZE;
                    payload.extend_from_slice(&bytes[offset..offset + Self::SECTOR_SIZE]);
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

                    if obj_type == Self::OBJ_TYPE_STORAGE {
                        let name = read_entry_name(idx)?;
                        if child_id != Self::FREE_SECTOR && child_id != Self::END_OF_CHAIN {
                            stack.push((child_id, format!("{prefix}{name}/")));
                        }
                    } else if obj_type == Self::OBJ_TYPE_STREAM {
                        let name = read_entry_name(idx)?;
                        let full_name = if prefix.is_empty() {
                            name
                        } else {
                            format!("{prefix}{name}")
                        };
                        streams.push(extract_stream(idx, full_name)?);
                    }

                    if left_sib != Self::FREE_SECTOR && left_sib != Self::END_OF_CHAIN {
                        stack.push((left_sib, prefix));
                    }
                }
            }
        }

        for idx in 0..entry_count {
            if !visited_dir.insert(idx as u32) {
                continue;
            }
            let base = idx * Self::DIR_ENTRY_SIZE;
            let obj_type = dir_bytes[base + 66];
            if obj_type != Self::OBJ_TYPE_STREAM {
                continue;
            }
            let name = read_entry_name(idx)?;
            streams.push(extract_stream(idx, name)?);
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
        let name_byte_len = ((units.len() + 1) * 2) as u16;
        write_u16_le(dir_bytes, base + 64, name_byte_len);
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

/// Paragraph record inside an HWP 5.0 `BodyText/Section0` stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hwp5ParagraphRecord {
    pub text: String,
    pub style_name: String,
    pub is_official_header: bool,
}

/// Decoded HWP 5.0 (`.hwp`) or HWT (`.hwt`) document payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hwp5DocumentPayload {
    pub version: (u8, u8, u8, u8),
    pub is_template: bool,
    pub agency_name: String,
    pub document_number: String,
    pub paragraphs: Vec<Hwp5ParagraphRecord>,
    pub table_cells: Vec<Vec<String>>,
}

pub type DocumentCodecPayload = Hwp5DocumentPayload;
pub type Hwp5TableGrid = Vec<Vec<String>>;

/// HWP 5.0 (`.hwp`) and HWT (`.hwt`) OLE2 container codec using 32-bit HWP record headers.
pub struct Hwp5Codec;

impl Hwp5Codec {
    pub const HWPTAG_DOCUMENT_PROPERTIES: u16 = 16;
    pub const HWPTAG_PARA_HEADER: u16 = 66;
    pub const HWPTAG_PARA_TEXT: u16 = 67;
    pub const HWPTAG_TABLE: u16 = 77;
    pub const HWPTAG_TABLE_RECORD: u16 = 77;
    pub const HWT_TEMPLATE_MARKER: &'static [u8; 12] = b"HWT_TEMPLATE";
    pub const MAX_HWP5_RECORDS: usize = 524_288;
    pub const MAX_TOTAL_HWP5_TABLE_CELLS: usize = 65_536;

    pub fn encode_record_header(tag_id: u16, level: u16, size: usize) -> Vec<u8> {
        let tag_bits = (tag_id as u32) & 0x03FF;
        let level_bits = ((level as u32) & 0x03FF) << 10;
        let mut out = Vec::with_capacity(8);
        if size < 0x0FFF {
            let word = tag_bits | level_bits | (((size as u32) & 0x0FFF) << 20);
            out.extend_from_slice(&word.to_le_bytes());
        } else {
            let word = tag_bits | level_bits | (0x0FFFu32 << 20);
            out.extend_from_slice(&word.to_le_bytes());
            out.extend_from_slice(&(size as u32).to_le_bytes());
        }
        out
    }

    pub fn decode_record_header(
        bytes: &[u8],
        offset: usize,
    ) -> Result<(u16, u16, usize, usize), EasyOfficeError> {
        let word = read_u32_le(bytes, offset)?;
        let tag_id = (word & 0x03FF) as u16;
        let level = ((word >> 10) & 0x03FF) as u16;
        let inline_size = ((word >> 20) & 0x0FFF) as usize;
        if inline_size == 0x0FFF {
            let ext_size = read_u32_le(bytes, offset + 4)? as usize;
            Ok((tag_id, level, ext_size, 8))
        } else {
            Ok((tag_id, level, inline_size, 4))
        }
    }

    pub fn pack(payload: &Hwp5DocumentPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_with_compression_and_tables(payload, &[], false)
    }

    pub fn pack_multi_tables(
        payload: &Hwp5DocumentPayload,
        extra_tables: &[Hwp5TableGrid],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_with_compression_and_tables(payload, extra_tables, false)
    }

    pub fn pack_compressed(payload: &Hwp5DocumentPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_with_compression_and_tables(payload, &[], true)
    }

    fn pack_with_compression_and_tables(
        payload: &Hwp5DocumentPayload,
        extra_tables: &[Hwp5TableGrid],
        compressed: bool,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let mut file_header = vec![0u8; Ole2SecurityGuard::HWP5_FILE_HEADER_LEN];
        file_header[..17].copy_from_slice(b"HWP Document File");
        file_header[32] = payload.version.3;
        file_header[33] = payload.version.2;
        file_header[34] = payload.version.1;
        file_header[35] = payload.version.0;
        let flags = if compressed {
            Ole2SecurityGuard::HWP5_FLAG_COMPRESSED
        } else {
            0x0000_0000
        };
        write_u32_le(&mut file_header, 36, flags);
        if payload.is_template {
            file_header[40..52].copy_from_slice(Self::HWT_TEMPLATE_MARKER);
        }

        let mut doc_props = Vec::new();
        append_len_prefixed_utf16le(&mut doc_props, &payload.agency_name);
        append_len_prefixed_utf16le(&mut doc_props, &payload.document_number);
        let mut doc_info_stream =
            Self::encode_record_header(Self::HWPTAG_DOCUMENT_PROPERTIES, 0, doc_props.len());
        doc_info_stream.extend_from_slice(&doc_props);

        let mut section0 = Vec::new();
        for para in &payload.paragraphs {
            let mut p_header = Vec::new();
            append_len_prefixed_utf16le(&mut p_header, &para.style_name);
            p_header.push(if para.is_official_header { 1 } else { 0 });
            section0.extend_from_slice(&Self::encode_record_header(
                Self::HWPTAG_PARA_HEADER,
                0,
                p_header.len(),
            ));
            section0.extend_from_slice(&p_header);

            let text_utf16 = encode_utf16le(&para.text);
            section0.extend_from_slice(&Self::encode_record_header(
                Self::HWPTAG_PARA_TEXT,
                1,
                text_utf16.len(),
            ));
            section0.extend_from_slice(&text_utf16);
        }

        for tbl in std::iter::once(&payload.table_cells).chain(extra_tables.iter()) {
            if !tbl.is_empty() {
                let mut tbl_buf = Vec::new();
                tbl_buf.extend_from_slice(&(tbl.len() as u16).to_le_bytes());
                for row in tbl {
                    tbl_buf.extend_from_slice(&(row.len() as u16).to_le_bytes());
                    for cell in row {
                        append_len_prefixed_utf16le(&mut tbl_buf, cell);
                    }
                }
                section0.extend_from_slice(&Self::encode_record_header(
                    Self::HWPTAG_TABLE_RECORD,
                    0,
                    tbl_buf.len(),
                ));
                section0.extend_from_slice(&tbl_buf);
            }
        }

        let (final_doc_info, final_section0) = if compressed {
            (
                crate::deflate::deflate_raw(&doc_info_stream),
                crate::deflate::deflate_raw(&section0),
            )
        } else {
            (doc_info_stream, section0)
        };

        Ole2ContainerCodec::pack(&[
            ("FileHeader".to_string(), file_header),
            ("DocInfo".to_string(), final_doc_info),
            ("BodyText/Section0".to_string(), final_section0),
        ])
    }

    pub fn unpack(bytes: &[u8]) -> Result<Hwp5DocumentPayload, EasyOfficeError> {
        Ok(Self::unpack_multi_tables(bytes)?.0)
    }

    pub fn unpack_multi_tables(
        bytes: &[u8],
    ) -> Result<(Hwp5DocumentPayload, Vec<Hwp5TableGrid>), EasyOfficeError> {
        let streams = Ole2ContainerCodec::unpack(bytes)?;
        let file_header = streams
            .iter()
            .find(|s| s.name == "FileHeader")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "HWP 5.0 필수 스트림 'FileHeader'가 누락되었습니다.".to_string(),
                )
            })?;

        let raw_ver = Ole2SecurityGuard::validate_hwp5_file_header(&file_header.data)?;
        let version = (
            ((raw_ver >> 24) & 0xFF) as u8,
            ((raw_ver >> 16) & 0xFF) as u8,
            ((raw_ver >> 8) & 0xFF) as u8,
            (raw_ver & 0xFF) as u8,
        );
        let flags = read_u32_le(&file_header.data, 36)?;
        let is_compressed = (flags & Ole2SecurityGuard::HWP5_FLAG_COMPRESSED) != 0;
        let is_template =
            file_header.data.len() >= 52 && &file_header.data[40..52] == Self::HWT_TEMPLATE_MARKER;

        let mut total_records = 0usize;
        let mut total_table_cells = 0usize;
        let mut agency_name = String::new();
        let mut document_number = String::new();
        if let Some(doc_info) = streams.iter().find(|s| s.name == "DocInfo") {
            if !doc_info.data.is_empty() {
                let doc_info_bytes: std::borrow::Cow<'_, [u8]> = if is_compressed {
                    std::borrow::Cow::Owned(crate::deflate::inflate_auto(
                        &doc_info.data,
                        crate::ZipSecurityGuard::MAX_SINGLE_ENTRY_UNCOMPRESSED_BYTES,
                    )?)
                } else {
                    std::borrow::Cow::Borrowed(&doc_info.data)
                };

                let mut offset = 0usize;
                while offset < doc_info_bytes.len() {
                    total_records += 1;
                    if total_records > Self::MAX_HWP5_RECORDS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "HWP 5.0 레코드 개수가 보안 상한({})을 초과했습니다.",
                            Self::MAX_HWP5_RECORDS
                        )));
                    }
                    let (tag_id, _level, size, hdr_len) =
                        Self::decode_record_header(&doc_info_bytes, offset)?;
                    let start = offset + hdr_len;
                    let end = start.checked_add(size).ok_or_else(|| {
                        EasyOfficeError::CorruptedContainer(
                            "HWP DocInfo 레코드 오버플로".to_string(),
                        )
                    })?;
                    if end > doc_info_bytes.len() {
                        return Err(EasyOfficeError::CorruptedContainer(
                            "HWP DocInfo 레코드 경계 초과".to_string(),
                        ));
                    }
                    if tag_id == Self::HWPTAG_DOCUMENT_PROPERTIES {
                        Self::parse_doc_properties(
                            &doc_info_bytes,
                            start,
                            end,
                            &mut agency_name,
                            &mut document_number,
                        )?;
                    }
                    offset = end;
                }
            }
        }

        let section0 = streams
            .iter()
            .find(|s| s.name == "BodyText/Section0" || s.name == "Section0")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "HWP 5.0 필수 스트림 'BodyText/Section0'가 누락되었습니다.".to_string(),
                )
            })?;

        let section0_bytes: std::borrow::Cow<'_, [u8]> = if is_compressed {
            std::borrow::Cow::Owned(crate::deflate::inflate_auto(
                &section0.data,
                crate::ZipSecurityGuard::MAX_SINGLE_ENTRY_UNCOMPRESSED_BYTES,
            )?)
        } else {
            std::borrow::Cow::Borrowed(&section0.data)
        };

        let mut paragraphs: Vec<Hwp5ParagraphRecord> = Vec::new();
        let mut table_cells: Vec<Vec<String>> = Vec::new();
        let mut all_tables: Vec<Vec<Vec<String>>> = Vec::new();
        let mut pending_meta: Option<(String, bool)> = None;
        let mut offset = 0usize;

        while offset < section0_bytes.len() {
            total_records += 1;
            if total_records > Self::MAX_HWP5_RECORDS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "HWP 5.0 레코드 개수가 보안 상한({})을 초과했습니다.",
                    Self::MAX_HWP5_RECORDS
                )));
            }
            let (tag_id, _level, size, hdr_len) =
                Self::decode_record_header(&section0_bytes, offset)?;
            let start = offset + hdr_len;
            let end = start.checked_add(size).ok_or_else(|| {
                EasyOfficeError::CorruptedContainer("HWP Section0 레코드 오버플로".to_string())
            })?;
            if end > section0_bytes.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "HWP Section0 레코드 경계 초과".to_string(),
                ));
            }

            match tag_id {
                Self::HWPTAG_PARA_HEADER => {
                    pending_meta = Some(Self::parse_para_header(&section0_bytes, start, end)?);
                }
                Self::HWPTAG_PARA_TEXT => {
                    let text = Self::decode_hwp5_para_text(&section0_bytes[start..end])?;
                    let (style_name, is_official_header) = pending_meta
                        .take()
                        .unwrap_or_else(|| ("본문".to_string(), false));
                    paragraphs.push(Hwp5ParagraphRecord {
                        text,
                        style_name,
                        is_official_header,
                    });
                }
                Self::HWPTAG_TABLE_RECORD => {
                    let parsed_rows = Self::parse_table_record(
                        &section0_bytes,
                        start,
                        end,
                        &mut total_table_cells,
                    )?;
                    if !parsed_rows.is_empty() {
                        table_cells.extend(parsed_rows.iter().cloned());
                        all_tables.push(parsed_rows);
                    }
                }
                _ => {}
            }
            offset = end;
        }

        Ok((
            Hwp5DocumentPayload {
                version,
                is_template,
                agency_name,
                document_number,
                paragraphs,
                table_cells,
            },
            all_tables,
        ))
    }

    fn decode_hwp5_para_text(bytes: &[u8]) -> Result<String, EasyOfficeError> {
        if bytes.len() % 2 != 0 {
            return Err(EasyOfficeError::CorruptedContainer(
                "HWP 5.0 문단 텍스트 페이로드 길이가 2바이트(WCHAR) 배수가 아닙니다.".to_string(),
            ));
        }
        let has_control = bytes
            .chunks_exact(2)
            .any(|c| u16::from_le_bytes([c[0], c[1]]) < 0x0020);
        if !has_control {
            return decode_utf16le(bytes);
        }

        let mut filtered = Vec::with_capacity(bytes.len());
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            let code = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
            if code < 0x0020 {
                match code {
                    0x0009 => {
                        filtered.extend_from_slice(&0x0009u16.to_le_bytes());
                        if cursor + 16 <= bytes.len()
                            && u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]])
                                == 0x0009
                            && bytes[cursor + 2..cursor + 14].iter().all(|&b| b == 0)
                        {
                            cursor += 16;
                        } else {
                            cursor += 2;
                        }
                    }
                    0x000A => {
                        filtered.extend_from_slice(&0x000Au16.to_le_bytes());
                        cursor += 2;
                    }
                    0x000D => {
                        if cursor + 2 < bytes.len() {
                            filtered.extend_from_slice(&0x000Au16.to_le_bytes());
                        }
                        cursor += 2;
                    }
                    0x0001..=0x0008 | 0x000B..=0x000C | 0x000E..=0x0017
                        if cursor + 16 <= bytes.len()
                            && u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]])
                                == code =>
                    {
                        cursor += 16;
                    }
                    _ => {
                        cursor += 2;
                    }
                }
            } else {
                filtered.push(bytes[cursor]);
                filtered.push(bytes[cursor + 1]);
                cursor += 2;
            }
        }
        decode_utf16le(&filtered)
    }

    fn parse_doc_properties(
        data: &[u8],
        start: usize,
        end: usize,
        agency_name: &mut String,
        document_number: &mut String,
    ) -> Result<(), EasyOfficeError> {
        let mut cursor = start;
        if let Ok(s1) = read_len_prefixed_utf16le(data, &mut cursor, end) {
            if let Ok(s2) = read_len_prefixed_utf16le(data, &mut cursor, end) {
                if cursor == end {
                    *agency_name = s1;
                    *document_number = s2;
                    return Ok(());
                }
                if let Ok(s3) = read_len_prefixed_utf16le(data, &mut cursor, end) {
                    if read_len_prefixed_utf16le(data, &mut cursor, end).is_ok() && cursor < end {
                        *agency_name = s1;
                        *document_number = if !s3.is_empty() { s3 } else { s2 };
                        return Ok(());
                    }
                }
            }
        }
        let slice = &data[start..end];
        if slice.len() >= 4 {
            let n = read_u32_le(slice, 0)? as usize;
            if 4 + n == slice.len() && n % 2 == 0 {
                *agency_name = decode_utf16le(&slice[4..])?;
                return Ok(());
            }
            if 4 + n * 2 <= slice.len() {
                *agency_name = decode_utf16le(&slice[4..4 + n * 2])?;
                return Ok(());
            }
        }
        if slice.len() % 2 == 0 {
            *agency_name = decode_utf16le(slice)?;
        }
        Ok(())
    }

    fn parse_para_header(
        data: &[u8],
        start: usize,
        end: usize,
    ) -> Result<(String, bool), EasyOfficeError> {
        if end - start == 4 {
            return Ok(("본문".to_string(), data[start + 1] != 0));
        }
        let mut cursor = start;
        if let Ok(style) = read_len_prefixed_utf16le(data, &mut cursor, end) {
            if cursor + 1 == end {
                return Ok((style, data[cursor] != 0));
            }
            if read_len_prefixed_utf16le(data, &mut cursor, end).is_ok()
                && read_len_prefixed_utf16le(data, &mut cursor, end).is_ok()
                && cursor + 9 <= end
            {
                let flags_or_bold = data[cursor + 8];
                let is_official =
                    (flags_or_bold & 0x01) != 0 || style.contains("표제") || style.contains("제목");
                return Ok((style, is_official));
            }
        }
        Ok(("본문".to_string(), false))
    }

    fn try_parse_u16_rows(
        data: &[u8],
        start: usize,
        end: usize,
        total_table_cells: &mut usize,
    ) -> Result<Option<Vec<Vec<String>>>, EasyOfficeError> {
        if start + 4 > end {
            return Ok(None);
        }
        let mut cur = start;
        let Ok(row_count_u16) = read_u16_le(data, cur) else {
            return Ok(None);
        };
        let Ok(first_col_u16) = read_u16_le(data, cur + 2) else {
            return Ok(None);
        };
        let row_count = row_count_u16 as usize;
        let first_col = first_col_u16 as usize;
        cur += 2;
        if row_count == 0 || first_col == 0 {
            return Ok(None);
        }
        if row_count > 4096
            || first_col > 256
            || row_count.saturating_mul(first_col) > Self::MAX_TOTAL_HWP5_TABLE_CELLS
        {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "HWP 5.0 표 행·열·셀 개수가 보안 상한을 초과했습니다.".to_string(),
            ));
        }
        if row_count.saturating_mul(6) > end.saturating_sub(cur) {
            return Ok(None);
        }
        let mut rows = Vec::with_capacity(row_count.min(256));
        let mut local_cells = 0usize;
        for _ in 0..row_count {
            let Ok(col_count_u16) = read_u16_le(data, cur) else {
                return Ok(None);
            };
            let col_count = col_count_u16 as usize;
            cur += 2;
            if col_count == 0 {
                return Ok(None);
            }
            if col_count > 256 {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWP 5.0 표 열 개수가 보안 상한(256)을 초과했습니다.".to_string(),
                ));
            }
            if col_count.saturating_mul(4) > end.saturating_sub(cur) {
                return Ok(None);
            }
            local_cells = local_cells.saturating_add(col_count);
            if total_table_cells.saturating_add(local_cells) > Self::MAX_TOTAL_HWP5_TABLE_CELLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "HWP 5.0 누적 표 셀 개수가 보안 상한({})을 초과했습니다.",
                    Self::MAX_TOTAL_HWP5_TABLE_CELLS
                )));
            }
            let mut row = Vec::with_capacity(col_count.min(256));
            for _ in 0..col_count {
                let Ok(cell) = read_len_prefixed_utf16le(data, &mut cur, end) else {
                    return Ok(None);
                };
                row.push(cell);
            }
            rows.push(row);
        }
        if cur == end {
            *total_table_cells += local_cells;
            Ok(Some(rows))
        } else {
            Ok(None)
        }
    }

    fn parse_table_record(
        data: &[u8],
        start: usize,
        end: usize,
        total_table_cells: &mut usize,
    ) -> Result<Vec<Vec<String>>, EasyOfficeError> {
        if let Some(rows) = Self::try_parse_u16_rows(data, start, end, total_table_cells)? {
            return Ok(rows);
        }
        let mut cur = start;
        if read_len_prefixed_utf16le(data, &mut cur, end).is_ok() && cur < end {
            let _has_header = data[cur] != 0;
            cur += 1;
            if let Some(rows) = Self::try_parse_u16_rows(data, cur, end, total_table_cells)? {
                return Ok(rows);
            }
            if cur + 8 <= end {
                let row_count = read_u32_le(data, cur)? as usize;
                let col_count = read_u32_le(data, cur + 4)? as usize;
                cur += 8;
                let table_cells = row_count.saturating_mul(col_count);
                if row_count == 0
                    || col_count == 0
                    || row_count > 4096
                    || col_count > 256
                    || table_cells.saturating_mul(4) > end.saturating_sub(cur)
                    || total_table_cells.saturating_add(table_cells)
                        > Self::MAX_TOTAL_HWP5_TABLE_CELLS
                {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "HWP 5.0 표 행·열 개수가 보안 상한을 초과했습니다.".to_string(),
                    ));
                }
                let mut rows = Vec::with_capacity(row_count.min(256));
                let mut ok = true;
                for _ in 0..row_count {
                    let mut row = Vec::with_capacity(col_count.min(256));
                    for _ in 0..col_count {
                        if let Ok(cell) = read_len_prefixed_utf16le(data, &mut cur, end) {
                            row.push(cell);
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        break;
                    }
                    rows.push(row);
                }
                if ok && cur == end {
                    *total_table_cells += table_cells;
                    return Ok(rows);
                }
            }
        }
        if start + 4 <= end && (end - (start + 4)) % 2 == 0 {
            let payload_slice = &data[start + 4..end];
            if payload_slice.len() > 65_536 {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWP 5.0 표 텍스트 페이로드가 보안 상한을 초과했습니다.".to_string(),
                ));
            }
            if payload_slice
                .chunks_exact(2)
                .any(|c| c[0] == 0 && c[1] == 0)
            {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "HWP 5.0 표 레코드 셀 한도 초과 또는 비정상 바이너리 구조입니다.".to_string(),
                ));
            }
            let txt = decode_utf16le(payload_slice)?;
            let mut rows = Vec::new();
            let mut local_cells = 0usize;
            for r in txt.split('\n') {
                if rows.len() >= 4096 {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "HWP 5.0 표 행 개수가 보안 상한(4096)을 초과했습니다.".to_string(),
                    ));
                }
                let mut row = Vec::new();
                for c in r.split('\t') {
                    if row.len() >= 256 {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "HWP 5.0 표 열 개수가 보안 상한(256)을 초과했습니다.".to_string(),
                        ));
                    }
                    local_cells += 1;
                    if total_table_cells.saturating_add(local_cells)
                        > Self::MAX_TOTAL_HWP5_TABLE_CELLS
                    {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "HWP 5.0 누적 표 셀 개수가 보안 상한({})을 초과했습니다.",
                            Self::MAX_TOTAL_HWP5_TABLE_CELLS
                        )));
                    }
                    row.push(c.to_string());
                }
                rows.push(row);
            }
            *total_table_cells += local_cells;
            return Ok(rows);
        }
        Err(EasyOfficeError::CorruptedContainer(
            "HWP 5.0 표 레코드(HWPTAG_TABLE) 구조가 손상되었습니다.".to_string(),
        ))
    }
}

pub use crate::legacy_ole2_codec::*;

impl Ole2SecurityGuard {
    pub fn validate_header_and_fat(container_bytes: &[u8]) -> Result<Vec<u32>, EasyOfficeError> {
        if container_bytes.len() < Self::SECTOR_SIZE * 2
            || container_bytes.len() % Self::SECTOR_SIZE != 0
        {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 512바이트 섹터 배수가 아닙니다.".to_string(),
            ));
        }
        if !container_bytes.starts_with(&FormatDetector::OLE2_MAGIC) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "OLE2 CFBF 시그니처 불일치".to_string(),
            });
        }
        let sector_shift = read_u16_le(container_bytes, 30)?;
        let mini_shift = read_u16_le(container_bytes, 32)?;
        if sector_shift != 9 || mini_shift != 6 {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "유효하지 않은 OLE2 섹터 시프트({sector_shift}/{mini_shift})"
            )));
        }
        let fat_sector_count = read_u32_le(container_bytes, 44)? as usize;
        let first_difat_sector = read_u32_le(container_bytes, 68)?;
        let num_difat_sectors = read_u32_le(container_bytes, 72)? as usize;
        let max_allowed_fat_sectors = Self::MAX_FAT_SECTORS + num_difat_sectors.min(64) * 127;
        if fat_sector_count == 0
            || fat_sector_count > max_allowed_fat_sectors
            || (fat_sector_count > Self::MAX_FAT_SECTORS && num_difat_sectors == 0)
        {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "OLE2 DIFAT 섹터 개수({fat_sector_count})가 허용 범위(1..={})를 벗어났습니다.",
                Self::MAX_FAT_SECTORS
            )));
        }

        let total_sectors = (container_bytes.len() / Self::SECTOR_SIZE) - 1;
        let mut fat_sector_indices = Vec::with_capacity(fat_sector_count);
        for slot in 0..fat_sector_count.min(Self::MAX_FAT_SECTORS) {
            let fat_sec_idx = read_u32_le(container_bytes, 76 + slot * 4)? as usize;
            if fat_sec_idx >= total_sectors {
                return Err(EasyOfficeError::CorruptedContainer(
                    "OLE2 FAT 섹터 인덱스가 파일 범위를 벗어났습니다.".to_string(),
                ));
            }
            fat_sector_indices.push(fat_sec_idx);
        }

        let mut difat_sec = first_difat_sector;
        let mut difat_visited = SectorBitset::with_capacity(total_sectors);
        let mut difat_steps = 0usize;
        while fat_sector_indices.len() < fat_sector_count
            && difat_sec != Self::END_OF_CHAIN
            && difat_sec != Self::FREE_SECTOR
        {
            if difat_steps >= num_difat_sectors.min(64) {
                break;
            }
            let d_idx = difat_sec as usize;
            if d_idx >= total_sectors || !difat_visited.insert(difat_sec) {
                return Err(EasyOfficeError::SecurityViolation(
                    "OLE2 DIFAT 체인이 유효하지 않거나 순환합니다.".to_string(),
                ));
            }
            let d_offset = (1 + d_idx) * Self::SECTOR_SIZE;
            for slot in 0..127 {
                if fat_sector_indices.len() >= fat_sector_count {
                    break;
                }
                let fat_sec = read_u32_le(container_bytes, d_offset + slot * 4)?;
                if fat_sec == Self::FREE_SECTOR || fat_sec == Self::END_OF_CHAIN {
                    continue;
                }
                if (fat_sec as usize) >= total_sectors {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "OLE2 DIFAT 내 FAT 섹터 인덱스가 범위를 벗어났습니다.".to_string(),
                    ));
                }
                fat_sector_indices.push(fat_sec as usize);
            }
            difat_sec = read_u32_le(container_bytes, d_offset + 127 * 4)?;
            difat_steps += 1;
        }

        let mut fat = Vec::with_capacity(fat_sector_indices.len() * 128);
        for fat_sec_idx in fat_sector_indices {
            let sec_offset = (1 + fat_sec_idx) * Self::SECTOR_SIZE;
            for entry_idx in 0..128 {
                let val = read_u32_le(container_bytes, sec_offset + entry_idx * 4)?;
                fat.push(val);
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
        if container_bytes.len() < Self::SECTOR_SIZE * 2
            || container_bytes.len() % Self::SECTOR_SIZE != 0
        {
            return Err(EasyOfficeError::CorruptedContainer(
                "OLE2 컨테이너 크기가 512바이트 섹터 배수가 아닙니다.".to_string(),
            ));
        }
        if !container_bytes.starts_with(&FormatDetector::OLE2_MAGIC) {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::Ole2Cfbf,
                detail: "OLE2 CFBF 헤더 시그니처 불일치".to_string(),
            });
        }

        let fat_sector_count = read_u32_le(container_bytes, 44)? as usize;
        let num_difat_sectors = read_u32_le(container_bytes, 72)? as usize;
        let max_allowed = Self::MAX_FAT_SECTORS + num_difat_sectors.min(64) * 127;
        if fat_sector_count == 0 || fat_sector_count > max_allowed {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "OLE2 DIFAT 섹터 개수({fat_sector_count})가 허용 범위(1..={})를 벗어났습니다.",
                Self::MAX_FAT_SECTORS
            )));
        }

        let total_sectors = (container_bytes.len() / Self::SECTOR_SIZE) - 1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentFormat;

    #[test]
    fn test_ole2_hwp5_and_legacy_codecs_roundtrip() {
        let hwp_payload = Hwp5DocumentPayload {
            version: (5, 0, 3, 0),
            is_template: false,
            agency_name: "행정안전부 디지털정부실".to_string(),
            document_number: "고시 제2026-42호".to_string(),
            paragraphs: vec![
                Hwp5ParagraphRecord {
                    text: "1. 표준 전자문서 호환성 검증".to_string(),
                    style_name: "기관표제".to_string(),
                    is_official_header: true,
                },
                Hwp5ParagraphRecord {
                    text: "가. 32비트 레코드 헤더 및 UTF-16LE 본문 보존".to_string(),
                    style_name: "본문".to_string(),
                    is_official_header: false,
                },
            ],
            table_cells: vec![vec!["항목".to_string(), "상태".to_string()]],
        };
        let hwp_bytes = Hwp5Codec::pack(&hwp_payload).unwrap();
        assert_eq!(
            FormatDetector::detect(&hwp_bytes, Some("test.hwp")).unwrap(),
            DocumentFormat::Hwp
        );
        let decoded_hwp = Hwp5Codec::unpack(&hwp_bytes).unwrap();
        assert_eq!(decoded_hwp, hwp_payload);

        let xls_payload = LegacyXlsPayload {
            sheet_name: "예산시트".to_string(),
            cells: vec![
                (0, 0, "항목".to_string()),
                (0, 1, "1500".to_string()),
                (0, 2, "=B1*2".to_string()),
            ],
        };
        let xls_bytes = LegacyOle2Codec::pack_xls(&xls_payload).unwrap();
        let decoded_xls = LegacyOle2Codec::unpack_xls(&xls_bytes).unwrap();
        assert_eq!(decoded_xls, xls_payload);
    }

    #[test]
    fn test_hwp5_para_text_control_chars_and_cfb_dir_cmp() {
        assert_eq!(
            Ole2ContainerCodec::cmp_cfb_dir_names("DocInfo", "docinfo"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            Ole2ContainerCodec::cmp_cfb_dir_names("a", "BB"),
            std::cmp::Ordering::Less
        );

        const HWP5_CTRL_SECTION_COL: u16 = 0x0002;
        const HWP5_CTRL_DRAWING_TABLE: u16 = 0x000B;
        const HWP5_CTRL_INLINE_TAB: u16 = 0x0009;
        const HWP5_CTRL_HYPHEN: u16 = 0x0018;
        let mut payload = Vec::new();
        payload.extend_from_slice(&encode_utf16le("문단앞"));
        payload.extend_from_slice(&HWP5_CTRL_SECTION_COL.to_le_bytes());
        for i in 0..6u16 {
            payload.extend_from_slice(&(0xAC00 + i).to_le_bytes());
        }
        payload.extend_from_slice(&HWP5_CTRL_SECTION_COL.to_le_bytes());
        payload.extend_from_slice(&HWP5_CTRL_DRAWING_TABLE.to_le_bytes());
        for i in 0..6u16 {
            payload.extend_from_slice(&(0xAC00 + i).to_le_bytes());
        }
        payload.extend_from_slice(&HWP5_CTRL_DRAWING_TABLE.to_le_bytes());
        payload.extend_from_slice(&HWP5_CTRL_INLINE_TAB.to_le_bytes());
        payload.extend_from_slice(&[0u8; 12]);
        payload.extend_from_slice(&HWP5_CTRL_INLINE_TAB.to_le_bytes());
        payload.extend_from_slice(&HWP5_CTRL_HYPHEN.to_le_bytes());
        payload.extend_from_slice(&encode_utf16le("문단뒤"));
        payload.extend_from_slice(&0x000Au16.to_le_bytes());
        payload.extend_from_slice(&encode_utf16le("둘째줄"));
        payload.extend_from_slice(&0x000Du16.to_le_bytes());

        let decoded = Hwp5Codec::decode_hwp5_para_text(&payload).unwrap();
        assert_eq!(decoded, "문단앞\t문단뒤\n둘째줄");
    }
}
