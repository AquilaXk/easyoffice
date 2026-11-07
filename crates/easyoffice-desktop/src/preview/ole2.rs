//! The `PrvImage` stream of an OLE2 compound file (HWP 5), read with seeks: the header, the sector
//! tables that lead to it, the directory and the stream itself.

use super::{le_u16, le_u32, read_at, MAX_THUMBNAIL_BYTES};
use std::io::{Read, Seek};

const HEADER_LEN: usize = 512;
const SECTOR_SHIFT_V3: u16 = 9;
const SECTOR_SHIFT_V4: u16 = 12;
const MINI_SECTOR_SHIFT: u16 = 6;
const MINI_SECTOR_LEN: usize = 1 << MINI_SECTOR_SHIFT;
const HEADER_DIFAT_COUNT: usize = 109;
const HEADER_DIFAT_AT: usize = 76;
const FAT_ENTRY_LEN: usize = 4;
const MAX_REGULAR_SECTOR: u32 = 0xFFFF_FFFA;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FREE_SECTOR: u32 = 0xFFFF_FFFF;
const MAX_FAT_SECTORS: usize = 8_192;
const MAX_DIFAT_SECTORS: usize = 256;
const MAX_CHAIN_SECTORS: usize = 65_536;
const MAX_DIR_ENTRIES: usize = 4_096;
const DIR_ENTRY_LEN: usize = 128;
const ENTRY_NAME_LEN_AT: usize = 64;
const ENTRY_TYPE_AT: usize = 66;
const ENTRY_START_AT: usize = 116;
const ENTRY_SIZE_AT: usize = 120;
const ENTRY_TYPE_STREAM: u8 = 2;
const ENTRY_TYPE_ROOT: u8 = 5;
const TARGET_NAME: &str = "PrvImage";

/// Offsets and tables of the container that every lookup needs.
struct Container {
    shift: u32,
    cutoff: u32,
    first_dir: u32,
    first_mini_fat: u32,
    mini_fat_count: usize,
    fat_sectors: Vec<u32>,
}

struct DirEntry {
    start: u32,
    size: u64,
}

impl Container {
    fn sector_len(&self) -> usize {
        1 << self.shift
    }

    fn sector_offset(&self, sector: u32) -> Option<u64> {
        u64::from(sector).checked_add(1)?.checked_shl(self.shift)
    }

    /// The FAT entry (next sector) of `sector`, read from the FAT sector that holds it.
    fn next_sector<R: Read + Seek>(&self, src: &mut R, sector: u32) -> Option<u32> {
        let per_sector = self.sector_len() / FAT_ENTRY_LEN;
        let index = usize::try_from(sector).ok()?;
        let fat_sector = *self.fat_sectors.get(index / per_sector)?;
        let at = self
            .sector_offset(fat_sector)?
            .checked_add(u64::try_from((index % per_sector) * FAT_ENTRY_LEN).ok()?)?;
        let mut raw = [0_u8; FAT_ENTRY_LEN];
        read_at(src, at, &mut raw)?;
        Some(u32::from_le_bytes(raw))
    }

    /// The sectors of a chain, at most `limit` of them; a damaged or cyclic chain is `None`.
    fn chain<R: Read + Seek>(&self, src: &mut R, first: u32, limit: usize) -> Option<Vec<u32>> {
        let mut sectors = Vec::new();
        let mut current = first;
        while current != END_OF_CHAIN {
            if current > MAX_REGULAR_SECTOR || sectors.len() >= limit {
                return None;
            }
            sectors.push(current);
            current = self.next_sector(src, current)?;
        }
        Some(sectors)
    }
}

fn read_fat_sector_list<R: Read + Seek>(
    src: &mut R,
    header: &[u8],
    shift: u32,
) -> Option<Vec<u32>> {
    let count = usize::try_from(le_u32(header, 44)?).ok()?;
    if count == 0 || count > MAX_FAT_SECTORS {
        return None;
    }
    let mut ids = Vec::with_capacity(count);
    for i in 0..HEADER_DIFAT_COUNT.min(count) {
        ids.push(le_u32(header, HEADER_DIFAT_AT + i * FAT_ENTRY_LEN)?);
    }
    let mut next = le_u32(header, 68)?;
    let per_sector = (1_usize << shift) / FAT_ENTRY_LEN - 1;
    let mut visited = 0_usize;
    while ids.len() < count && next != END_OF_CHAIN && next != FREE_SECTOR {
        visited += 1;
        if visited > MAX_DIFAT_SECTORS || next > MAX_REGULAR_SECTOR {
            return None;
        }
        let mut sector = vec![0_u8; 1 << shift];
        read_at(
            src,
            u64::from(next).checked_add(1)?.checked_shl(shift)?,
            &mut sector,
        )?;
        for i in 0..per_sector.min(count - ids.len()) {
            ids.push(le_u32(&sector, i * FAT_ENTRY_LEN)?);
        }
        next = le_u32(&sector, per_sector * FAT_ENTRY_LEN)?;
    }
    (ids.len() == count && ids.iter().all(|&id| id <= MAX_REGULAR_SECTOR)).then_some(ids)
}

fn open_container<R: Read + Seek>(src: &mut R) -> Option<Container> {
    let mut header = [0_u8; HEADER_LEN];
    read_at(src, 0, &mut header)?;
    let shift = le_u16(&header, 30)?;
    if (shift != SECTOR_SHIFT_V3 && shift != SECTOR_SHIFT_V4)
        || le_u16(&header, 32)? != MINI_SECTOR_SHIFT
    {
        return None;
    }
    let shift = u32::from(shift);
    Some(Container {
        shift,
        cutoff: le_u32(&header, 56)?,
        first_dir: le_u32(&header, 48)?,
        first_mini_fat: le_u32(&header, 60)?,
        mini_fat_count: usize::try_from(le_u32(&header, 64)?).ok()?,
        fat_sectors: read_fat_sector_list(src, &header, shift)?,
    })
}

fn entry_name_is(raw: &[u8], target: &str) -> Option<bool> {
    let bytes = usize::from(le_u16(raw, ENTRY_NAME_LEN_AT)?);
    let expected = (target.len() + 1) * 2;
    if bytes != expected {
        return Some(false);
    }
    for (i, unit) in target.encode_utf16().enumerate() {
        if le_u16(raw, i * 2)? != unit {
            return Some(false);
        }
    }
    Some(true)
}

/// The root entry (the mini stream) and the `PrvImage` stream entry of the directory.
fn find_entries<R: Read + Seek>(src: &mut R, c: &Container) -> Option<(DirEntry, DirEntry)> {
    let per_sector = c.sector_len() / DIR_ENTRY_LEN;
    let max_sectors = MAX_DIR_ENTRIES.div_ceil(per_sector);
    let mut root = None;
    let mut target = None;
    let mut seen = 0_usize;
    for sector in c.chain(src, c.first_dir, max_sectors)? {
        let mut data = vec![0_u8; c.sector_len()];
        read_at(src, c.sector_offset(sector)?, &mut data)?;
        let (records, _) = data.as_chunks::<DIR_ENTRY_LEN>();
        for raw in records {
            seen += 1;
            if seen > MAX_DIR_ENTRIES {
                return None;
            }
            let entry = DirEntry {
                start: le_u32(raw, ENTRY_START_AT)?,
                size: size_of_entry(raw, c.shift)?,
            };
            match raw.get(ENTRY_TYPE_AT).copied()? {
                ENTRY_TYPE_ROOT if root.is_none() => root = Some(entry),
                ENTRY_TYPE_STREAM if target.is_none() && entry_name_is(raw, TARGET_NAME)? => {
                    target = Some(entry);
                }
                _ => {}
            }
        }
    }
    Some((root?, target?))
}

/// Stream size; version 3 files only define the low 32 bits.
fn size_of_entry(raw: &[u8], shift: u32) -> Option<u64> {
    let low = u64::from(le_u32(raw, ENTRY_SIZE_AT)?);
    if shift == u32::from(SECTOR_SHIFT_V3) {
        return Some(low);
    }
    Some(low | (u64::from(le_u32(raw, ENTRY_SIZE_AT + 4)?) << 32))
}

fn read_regular<R: Read + Seek>(
    src: &mut R,
    c: &Container,
    entry: &DirEntry,
    size: usize,
) -> Option<Vec<u8>> {
    let sectors = c.chain(src, entry.start, size.div_ceil(c.sector_len()))?;
    let mut out = Vec::with_capacity(size);
    for sector in sectors {
        let take = (size - out.len()).min(c.sector_len());
        let mut part = vec![0_u8; take];
        read_at(src, c.sector_offset(sector)?, &mut part)?;
        out.extend_from_slice(&part);
    }
    (out.len() == size).then_some(out)
}

fn read_mini<R: Read + Seek>(
    src: &mut R,
    c: &Container,
    root: &DirEntry,
    entry: &DirEntry,
    size: usize,
) -> Option<Vec<u8>> {
    let mini_fat = c.chain(
        src,
        c.first_mini_fat,
        c.mini_fat_count.min(MAX_CHAIN_SECTORS),
    )?;
    let mini_stream = c.chain(src, root.start, MAX_CHAIN_SECTORS)?;
    let per_fat = c.sector_len() / FAT_ENTRY_LEN;
    let mut out = Vec::with_capacity(size);
    let mut current = entry.start;
    for _ in 0..size.div_ceil(MINI_SECTOR_LEN) {
        if current > MAX_REGULAR_SECTOR {
            return None;
        }
        let index = usize::try_from(current).ok()?;
        let offset = index.checked_mul(MINI_SECTOR_LEN)?;
        let holder = *mini_stream.get(offset / c.sector_len())?;
        let take = (size - out.len()).min(MINI_SECTOR_LEN);
        let mut part = vec![0_u8; take];
        let at = c
            .sector_offset(holder)?
            .checked_add(u64::try_from(offset % c.sector_len()).ok()?)?;
        read_at(src, at, &mut part)?;
        out.extend_from_slice(&part);
        let fat_sector = *mini_fat.get(index / per_fat)?;
        let entry_at = c
            .sector_offset(fat_sector)?
            .checked_add(u64::try_from((index % per_fat) * FAT_ENTRY_LEN).ok()?)?;
        let mut raw = [0_u8; FAT_ENTRY_LEN];
        read_at(src, entry_at, &mut raw)?;
        current = u32::from_le_bytes(raw);
    }
    (out.len() == size && current == END_OF_CHAIN).then_some(out)
}

pub fn read_prv_image<R: Read + Seek>(src: &mut R) -> Option<Vec<u8>> {
    let container = open_container(src)?;
    let (root, entry) = find_entries(src, &container)?;
    let size = usize::try_from(entry.size).ok()?;
    if size == 0 || size > MAX_THUMBNAIL_BYTES {
        return None;
    }
    if entry.size >= u64::from(container.cutoff) {
        read_regular(src, &container, &entry, size)
    } else {
        read_mini(src, &container, &root, &entry, size)
    }
}
