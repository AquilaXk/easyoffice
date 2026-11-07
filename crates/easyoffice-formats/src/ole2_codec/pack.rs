//! OLE2 CFBF container writer (directory tree, mini stream, FAT / DIFAT layout).

use super::{
    write_u16_le, write_u32_le, EasyOfficeError, FormatDetector, Ole2ContainerCodec, Ole2DirNode,
    Ole2SecurityGuard,
};

const HEADER_MINOR_VERSION: u16 = 0x003E;
const HEADER_MAJOR_VERSION: u16 = 0x0003;
const HEADER_BYTE_ORDER: u16 = 0xFFFE;
const HEADER_SECTOR_SHIFT: u16 = 0x0009;
const HEADER_MINI_SECTOR_SHIFT: u16 = 0x0006;
const HEADER_DIFAT_OFFSET: usize = 76;

fn sector_overflow() -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer("OLE2 섹터 인덱스 오버플로".to_string())
}

fn dir_entry_budget_error() -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(format!(
        "OLE2 디렉터리 엔트리 개수가 보안 상한({})을 초과했습니다.",
        crate::MAX_OLE2_DIR_ENTRIES
    ))
}

fn empty_stream_name_error() -> EasyOfficeError {
    EasyOfficeError::InvalidArgument("OLE2 스트림 이름은 비어 있을 수 없습니다.".to_string())
}

/// Pads `buf` with zeros up to the next multiple of `unit`.
fn pad_to_multiple(buf: &mut Vec<u8>, unit: usize) {
    let rem = buf.len() % unit;
    if rem != 0 {
        buf.resize(buf.len() + (unit - rem), 0);
    }
}

fn next_chain_entry(offset: usize, count: usize, cur: usize) -> u32 {
    if offset + 1 < count {
        (cur + 1) as u32
    } else {
        Ole2ContainerCodec::END_OF_CHAIN
    }
}

/// Sector assignment of all stream payloads.
struct PackLayout<'a> {
    next_sector_idx: u32,
    regular_chains: Vec<(u32, usize)>,
    regular_payloads: Vec<&'a [u8]>,
    mini_stream_bytes: Vec<u8>,
    minifat: Vec<u32>,
}

/// FAT / DIFAT sector counts and positions.
struct FatPlan {
    content_sector_count: usize,
    fat_sector_count: usize,
    difat_sector_count: usize,
}

impl FatPlan {
    fn first_fat_sector(&self) -> usize {
        self.content_sector_count
    }

    fn first_difat_sector(&self) -> usize {
        self.content_sector_count + self.fat_sector_count
    }
}

impl Ole2ContainerCodec {
    pub fn pack(streams: &[(String, Vec<u8>)]) -> Result<Vec<u8>, EasyOfficeError> {
        if streams.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }
        let mut nodes = Self::build_dir_nodes(streams)?;
        Self::link_dir_siblings(&mut nodes);

        let dir_sector_count = nodes.len().div_ceil(Self::ENTRIES_PER_SECTOR).max(1);
        let mut layout = Self::assign_sectors(&mut nodes, dir_sector_count)?;
        let (first_minifat_sector, minifat_sector_count, minifat_sector_bytes) =
            Self::plan_mini_structures(&mut nodes, &mut layout)?;
        let plan = Self::plan_fat(layout.next_sector_idx as usize);
        let fat = Self::build_fat(&plan, &layout, dir_sector_count);

        let mut container = Self::build_header(&plan, first_minifat_sector, minifat_sector_count);
        let mut dir_bytes = vec![0u8; dir_sector_count * Self::SECTOR_SIZE];
        for (entry_idx, node) in nodes.iter().enumerate() {
            Self::write_dir_entry(&mut dir_bytes, entry_idx, node)?;
        }
        container.extend_from_slice(&dir_bytes);
        for data in &layout.regular_payloads {
            container.extend_from_slice(data);
            pad_to_multiple(&mut container, Self::SECTOR_SIZE);
        }
        if !layout.mini_stream_bytes.is_empty() {
            container.extend_from_slice(&layout.mini_stream_bytes);
            pad_to_multiple(&mut container, Self::SECTOR_SIZE);
            container.extend_from_slice(&minifat_sector_bytes);
        }
        for &entry in &fat {
            container.extend_from_slice(&entry.to_le_bytes());
        }
        Self::append_difat_sectors(&mut container, &plan);

        debug_assert_eq!(
            container.len(),
            (1 + plan.content_sector_count + plan.fat_sector_count + plan.difat_sector_count)
                * Self::SECTOR_SIZE
        );
        Ok(container)
    }

    /// Creates the directory node list (root, storages and streams) from slash-separated names.
    fn build_dir_nodes(
        streams: &[(String, Vec<u8>)],
    ) -> Result<Vec<Ole2DirNode<'_>>, EasyOfficeError> {
        let mut nodes: Vec<Ole2DirNode<'_>> = Vec::with_capacity(1 + streams.len() * 2);
        nodes.push(Ole2DirNode::new(
            "Root Entry".to_string(),
            Self::OBJ_TYPE_ROOT,
            None,
            Self::END_OF_CHAIN,
            0,
        ));
        for (name, data) in streams {
            let trimmed = name.trim().trim_matches('/');
            if trimmed.is_empty() {
                return Err(empty_stream_name_error());
            }
            let parts: Vec<&str> = trimmed.split('/').collect();
            let mut parent_idx = 0usize;
            for &part in &parts[..parts.len() - 1] {
                parent_idx = Self::find_or_add_storage(&mut nodes, parent_idx, part.trim())?;
            }
            Self::add_stream_node(&mut nodes, parent_idx, parts[parts.len() - 1].trim(), data)?;
        }
        Ok(nodes)
    }

    fn find_or_add_storage(
        nodes: &mut Vec<Ole2DirNode<'_>>,
        parent_idx: usize,
        part: &str,
    ) -> Result<usize, EasyOfficeError> {
        if part.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "OLE2 스토리지 이름은 비어 있을 수 없습니다.".to_string(),
            ));
        }
        let mut existing = None;
        for &c_idx in &nodes[parent_idx].children {
            let same_name =
                Self::cmp_cfb_dir_names(&nodes[c_idx].name, part) == std::cmp::Ordering::Equal;
            if !same_name {
                continue;
            }
            if nodes[c_idx].obj_type != Self::OBJ_TYPE_STORAGE {
                return Err(EasyOfficeError::SecurityViolation(format!(
                    "OLE2 디렉터리 내 대소문자 구분 없는 이름 충돌이 감지되었습니다: {part}"
                )));
            }
            existing.get_or_insert(c_idx);
        }
        if let Some(c_idx) = existing {
            return Ok(c_idx);
        }
        Self::push_child(
            nodes,
            parent_idx,
            Ole2DirNode::new(part.to_string(), Self::OBJ_TYPE_STORAGE, None, 0, 0),
        )
    }

    fn add_stream_node<'a>(
        nodes: &mut Vec<Ole2DirNode<'a>>,
        parent_idx: usize,
        leaf_name: &str,
        data: &'a [u8],
    ) -> Result<(), EasyOfficeError> {
        if leaf_name.is_empty() {
            return Err(empty_stream_name_error());
        }
        let duplicate = nodes[parent_idx].children.iter().any(|&c_idx| {
            Self::cmp_cfb_dir_names(&nodes[c_idx].name, leaf_name) == std::cmp::Ordering::Equal
        });
        if duplicate {
            return Err(EasyOfficeError::SecurityViolation(format!(
                "OLE2 동일 스토리지 내 중복 스트림 이름(대소문자 무시)이 감지되었습니다: {leaf_name}"
            )));
        }
        let node = Ole2DirNode::new(
            leaf_name.to_string(),
            Self::OBJ_TYPE_STREAM,
            Some(data),
            Self::END_OF_CHAIN,
            data.len() as u64,
        );
        Self::push_child(nodes, parent_idx, node)?;
        Ok(())
    }

    /// Appends `node` under `parent_idx`, enforcing the directory entry budget.
    fn push_child<'a>(
        nodes: &mut Vec<Ole2DirNode<'a>>,
        parent_idx: usize,
        node: Ole2DirNode<'a>,
    ) -> Result<usize, EasyOfficeError> {
        if nodes.len() >= crate::MAX_OLE2_DIR_ENTRIES {
            return Err(dir_entry_budget_error());
        }
        let new_idx = nodes.len();
        nodes.push(node);
        nodes[parent_idx].children.push(new_idx);
        Ok(new_idx)
    }

    /// Builds the balanced sibling tree (left / right / child ids) for each directory.
    fn link_dir_siblings(nodes: &mut [Ole2DirNode<'_>]) {
        for i in 0..nodes.len() {
            if nodes[i].children.is_empty() {
                continue;
            }
            let mut ch = std::mem::take(&mut nodes[i].children);
            ch.sort_by(|&a, &b| Self::cmp_cfb_dir_names(&nodes[a].name, &nodes[b].name));
            let max_depth = (usize::BITS - ch.len().leading_zeros()) as usize - 1;
            nodes[i].child_id = Self::build_dir_bst(&ch, nodes, 0, max_depth);
            nodes[i].children = ch;
        }
    }

    /// Assigns mini-stream or regular sectors to every stream node.
    fn assign_sectors<'a>(
        nodes: &mut [Ole2DirNode<'a>],
        dir_sector_count: usize,
    ) -> Result<PackLayout<'a>, EasyOfficeError> {
        let mut layout = PackLayout {
            next_sector_idx: dir_sector_count as u32,
            regular_chains: Vec::new(),
            regular_payloads: Vec::new(),
            mini_stream_bytes: Vec::new(),
            minifat: Vec::new(),
        };
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
                node.start_sector = Self::append_mini_stream(&mut layout, data);
            } else {
                node.start_sector = Self::append_regular_stream(&mut layout, data)?;
            }
        }
        Ok(layout)
    }

    /// Appends a small stream to the mini stream; returns its first mini sector.
    fn append_mini_stream(layout: &mut PackLayout<'_>, data: &[u8]) -> u32 {
        let m_start = (layout.mini_stream_bytes.len() / Self::MINI_SECTOR_SIZE) as u32;
        let m_count = data.len().div_ceil(Self::MINI_SECTOR_SIZE);
        for m_off in 0..m_count {
            let next_m = if m_off + 1 < m_count {
                m_start + (m_off as u32) + 1
            } else {
                Self::END_OF_CHAIN
            };
            layout.minifat.push(next_m);
        }
        layout.mini_stream_bytes.extend_from_slice(data);
        pad_to_multiple(&mut layout.mini_stream_bytes, Self::MINI_SECTOR_SIZE);
        m_start
    }

    /// Reserves regular sectors for a large stream; returns its first sector.
    fn append_regular_stream<'a>(
        layout: &mut PackLayout<'a>,
        data: &'a [u8],
    ) -> Result<u32, EasyOfficeError> {
        let count = data.len().div_ceil(Self::SECTOR_SIZE);
        let start = layout.next_sector_idx;
        layout.regular_chains.push((start, count));
        layout.regular_payloads.push(data);
        layout.next_sector_idx = start
            .checked_add(count as u32)
            .ok_or_else(sector_overflow)?;
        Ok(start)
    }

    /// Reserves root mini-stream and MiniFAT sectors; returns `(first minifat sector, count, bytes)`.
    fn plan_mini_structures(
        nodes: &mut [Ole2DirNode<'_>],
        layout: &mut PackLayout<'_>,
    ) -> Result<(u32, u32, Vec<u8>), EasyOfficeError> {
        if layout.mini_stream_bytes.is_empty() {
            nodes[0].start_sector = Self::END_OF_CHAIN;
            nodes[0].stream_size = 0;
            return Ok((Self::END_OF_CHAIN, 0, Vec::new()));
        }
        let root_sec_count = layout.mini_stream_bytes.len().div_ceil(Self::SECTOR_SIZE);
        nodes[0].start_sector = layout.next_sector_idx;
        nodes[0].stream_size = layout.mini_stream_bytes.len() as u64;
        layout
            .regular_chains
            .push((layout.next_sector_idx, root_sec_count));
        layout.next_sector_idx = layout
            .next_sector_idx
            .checked_add(root_sec_count as u32)
            .ok_or_else(sector_overflow)?;

        let mf_sec_count = layout
            .minifat
            .len()
            .div_ceil(Self::FAT_ENTRIES_PER_SECTOR)
            .max(1);
        let mf_start = layout.next_sector_idx;
        layout.regular_chains.push((mf_start, mf_sec_count));
        layout.next_sector_idx = mf_start
            .checked_add(mf_sec_count as u32)
            .ok_or_else(sector_overflow)?;

        layout.minifat.resize(
            mf_sec_count * Self::FAT_ENTRIES_PER_SECTOR,
            Self::FREE_SECTOR,
        );
        let mut bytes = Vec::with_capacity(mf_sec_count * Self::SECTOR_SIZE);
        for &entry in &layout.minifat {
            bytes.extend_from_slice(&entry.to_le_bytes());
        }
        Ok((mf_start, mf_sec_count as u32, bytes))
    }

    /// Iterates until the FAT / DIFAT sector counts cover the content plus themselves.
    fn plan_fat(content_sector_count: usize) -> FatPlan {
        let mut fat_sector_count = content_sector_count
            .div_ceil(Self::FAT_ENTRIES_PER_SECTOR)
            .max(1);
        let mut difat_sector_count = 0usize;
        loop {
            let needed_difat = fat_sector_count
                .saturating_sub(Ole2SecurityGuard::MAX_FAT_SECTORS)
                .div_ceil(Self::DIFAT_ENTRIES_PER_SECTOR);
            let total_secs = content_sector_count + fat_sector_count + needed_difat;
            let needed_fat = total_secs.div_ceil(Self::FAT_ENTRIES_PER_SECTOR).max(1);
            if needed_fat == fat_sector_count && needed_difat == difat_sector_count {
                break;
            }
            fat_sector_count = needed_fat;
            difat_sector_count = needed_difat;
        }
        FatPlan {
            content_sector_count,
            fat_sector_count,
            difat_sector_count,
        }
    }

    fn build_fat(plan: &FatPlan, layout: &PackLayout<'_>, dir_sector_count: usize) -> Vec<u32> {
        let mut fat = vec![Self::FREE_SECTOR; plan.fat_sector_count * Self::FAT_ENTRIES_PER_SECTOR];
        for (sec, slot) in fat.iter_mut().enumerate().take(dir_sector_count) {
            *slot = next_chain_entry(sec, dir_sector_count, sec);
        }
        for &(start_sec, count) in &layout.regular_chains {
            for offset in 0..count {
                let cur = start_sec as usize + offset;
                fat[cur] = next_chain_entry(offset, count, cur);
            }
        }
        Self::mark_fat_slots(
            &mut fat,
            plan.first_fat_sector(),
            plan.fat_sector_count,
            Self::FAT_SECTOR_MARKER,
        );
        Self::mark_fat_slots(
            &mut fat,
            plan.first_difat_sector(),
            plan.difat_sector_count,
            Self::DIFAT_SECTOR_MARKER,
        );
        fat
    }

    fn mark_fat_slots(fat: &mut [u32], first: usize, count: usize, marker: u32) {
        for idx in 0..count {
            if let Some(slot) = fat.get_mut(first + idx) {
                *slot = marker;
            }
        }
    }

    /// Writes the 512-byte header sector.
    fn build_header(
        plan: &FatPlan,
        first_minifat_sector: u32,
        minifat_sector_count: u32,
    ) -> Vec<u8> {
        let mut container = vec![0u8; Self::SECTOR_SIZE];
        container[..8].copy_from_slice(&FormatDetector::OLE2_MAGIC);
        write_u16_le(&mut container, 24, HEADER_MINOR_VERSION);
        write_u16_le(&mut container, 26, HEADER_MAJOR_VERSION);
        write_u16_le(&mut container, 28, HEADER_BYTE_ORDER);
        write_u16_le(&mut container, 30, HEADER_SECTOR_SHIFT);
        write_u16_le(&mut container, 32, HEADER_MINI_SECTOR_SHIFT);
        write_u32_le(&mut container, 44, plan.fat_sector_count as u32);
        write_u32_le(&mut container, 48, 0); // first directory sector = 0
        write_u32_le(&mut container, 56, Self::MINI_STREAM_CUTOFF as u32);
        write_u32_le(&mut container, 60, first_minifat_sector);
        write_u32_le(&mut container, 64, minifat_sector_count);
        let first_difat_header = if plan.difat_sector_count > 0 {
            plan.first_difat_sector() as u32
        } else {
            Self::END_OF_CHAIN
        };
        write_u32_le(&mut container, 68, first_difat_header);
        write_u32_le(&mut container, 72, plan.difat_sector_count as u32);
        for slot in 0..Ole2SecurityGuard::MAX_FAT_SECTORS {
            let val = if slot < plan.fat_sector_count {
                (plan.first_fat_sector() + slot) as u32
            } else {
                Self::FREE_SECTOR
            };
            write_u32_le(&mut container, HEADER_DIFAT_OFFSET + slot * 4, val);
        }
        container
    }

    fn append_difat_sectors(container: &mut Vec<u8>, plan: &FatPlan) {
        for d_idx in 0..plan.difat_sector_count {
            let mut difat_sec = [0u8; Self::SECTOR_SIZE];
            let base_fat_slot =
                Ole2SecurityGuard::MAX_FAT_SECTORS + d_idx * Self::DIFAT_ENTRIES_PER_SECTOR;
            for slot in 0..Self::DIFAT_ENTRIES_PER_SECTOR {
                let fat_slot = base_fat_slot + slot;
                let val = if fat_slot < plan.fat_sector_count {
                    (plan.first_fat_sector() + fat_slot) as u32
                } else {
                    Self::FREE_SECTOR
                };
                write_u32_le(&mut difat_sec, slot * 4, val);
            }
            let next_difat = if d_idx + 1 < plan.difat_sector_count {
                (plan.first_difat_sector() + d_idx + 1) as u32
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
    }
}
