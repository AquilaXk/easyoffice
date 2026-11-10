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

mod pack;
mod unpack;

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

impl<'a> Ole2DirNode<'a> {
    fn new(
        name: String,
        obj_type: u8,
        data: Option<&'a [u8]>,
        start_sector: u32,
        stream_size: u64,
    ) -> Self {
        Self {
            name,
            obj_type,
            children: Vec::new(),
            data,
            left_sibling: Ole2ContainerCodec::FREE_SECTOR,
            right_sibling: Ole2ContainerCodec::FREE_SECTOR,
            child_id: Ole2ContainerCodec::FREE_SECTOR,
            color: 1,
            start_sector,
            stream_size,
        }
    }
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
        if (u16::from(b'a')..=u16::from(b'z')).contains(&u) {
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

    fn write_dir_entry(
        dir_bytes: &mut [u8],
        entry_idx: usize,
        node: &Ole2DirNode<'_>,
    ) -> Result<(), EasyOfficeError> {
        let base = entry_idx * Self::DIR_ENTRY_SIZE;
        let units: Vec<u16> = node.name.encode_utf16().collect();
        if units.len() > 31 {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "OLE2 디렉터리 이름 길이(31 UTF-16 코드 유닛) 초과: {}",
                node.name
            )));
        }
        for (i, &u) in units.iter().enumerate() {
            write_u16_le(dir_bytes, base + i * 2, u);
        }
        write_u16_le(dir_bytes, base + 64, ((units.len() + 1) * 2) as u16);
        dir_bytes[base + 66] = node.obj_type;
        dir_bytes[base + 67] = node.color;
        write_u32_le(dir_bytes, base + 68, node.left_sibling);
        write_u32_le(dir_bytes, base + 72, node.right_sibling);
        write_u32_le(dir_bytes, base + 76, node.child_id);
        write_u32_le(dir_bytes, base + 116, node.start_sector);
        write_u64_le(dir_bytes, base + 120, node.stream_size);
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
