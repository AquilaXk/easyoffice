//! MS-CFB 2.6.3 and 2.6.4: a version 3 file (512-byte sectors) defines only the low 32 bits of a
//! directory entry's stream size, older writers left the high half uninitialized. A version 4 file
//! (4096-byte sectors) uses all 64 bits.

use easyoffice_formats::{EasyOfficeError, Ole2ContainerCodec};

const DIR_ENTRY: usize = 128;
const SIZE_AT: usize = 120;
const FREE: u32 = 0xFFFF_FFFF;
const END: u32 = 0xFFFF_FFFE;
const FAT_SECTOR: u32 = 0xFFFF_FFFD;

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u32(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

fn streams() -> Vec<(String, Vec<u8>)> {
    vec![
        ("Big".to_string(), vec![0xAB; 5_000]),
        ("Small".to_string(), b"mini bytes".to_vec()),
    ]
}

/// Sets the stream size of every directory entry of a packed version 3 file (the directory of
/// three entries fits one sector).
fn with_sizes(mut bytes: Vec<u8>, high: u32) -> Vec<u8> {
    assert_eq!(bytes[26], 3, "the packer writes version 3");
    let dir = (1 + get_u32(&bytes, 48)) * 512;
    for entry in 0..3 {
        let at = dir + entry * DIR_ENTRY + SIZE_AT;
        let low = u64::from(u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()));
        put_u64(&mut bytes, at, (u64::from(high) << 32) | low);
    }
    bytes
}

fn sorted(mut entries: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    entries.sort();
    entries
}

fn unpacked(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, EasyOfficeError> {
    Ok(sorted(
        Ole2ContainerCodec::unpack(bytes)?
            .into_iter()
            .map(|entry| (entry.name, entry.data))
            .collect(),
    ))
}

#[test]
fn version_3_ignores_the_high_half_of_every_size() {
    let packed = Ole2ContainerCodec::pack(&streams()).expect("pack");
    let expected = unpacked(&packed).expect("clean file opens");
    assert_eq!(expected.len(), 2);
    for high in [1, 0x8000_0000, 0xDEAD_BEEF, u32::MAX] {
        let dirty = with_sizes(packed.clone(), high);
        assert_eq!(unpacked(&dirty).expect("garbage high bits"), expected);
    }
}

/// One 4096-byte-sector file: header, FAT, directory, and one data sector holding `data`.
fn version_4_file(stream_size: u64, data: &[u8]) -> Vec<u8> {
    const SECTOR: usize = 4096;
    let mut file = vec![0u8; SECTOR * 4];
    file[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    put_u16(&mut file, 24, 0x003E);
    put_u16(&mut file, 26, 4);
    put_u16(&mut file, 28, 0xFFFE);
    put_u16(&mut file, 30, 12);
    put_u16(&mut file, 32, 6);
    put_u32(&mut file, 40, 1); // directory sectors (version 4 only)
    put_u32(&mut file, 44, 1); // FAT sectors
    put_u32(&mut file, 48, 1); // first directory sector
    put_u32(&mut file, 56, 4096); // mini stream cutoff
    put_u32(&mut file, 60, END);
    put_u32(&mut file, 68, END);
    put_u32(&mut file, 76, 0); // DIFAT[0]: the FAT is sector 0
    for slot in 1..109 {
        put_u32(&mut file, 76 + slot * 4, FREE);
    }
    let fat = SECTOR;
    for slot in 0..SECTOR / 4 {
        put_u32(&mut file, fat + slot * 4, FREE);
    }
    put_u32(&mut file, fat, FAT_SECTOR);
    put_u32(&mut file, fat + 4, END); // directory
    put_u32(&mut file, fat + 8, END); // data
    let dir = SECTOR * 2;
    let mut entry = |index: usize, name: &str, kind: u8, child: u32, start: u32, size: u64| {
        let at = dir + index * DIR_ENTRY;
        for (i, unit) in name.encode_utf16().enumerate() {
            put_u16(&mut file, at + i * 2, unit);
        }
        put_u16(
            &mut file,
            at + 64,
            ((name.encode_utf16().count() + 1) * 2) as u16,
        );
        file[at + 66] = kind;
        file[at + 67] = 1;
        put_u32(&mut file, at + 68, FREE);
        put_u32(&mut file, at + 72, FREE);
        put_u32(&mut file, at + 76, child);
        put_u32(&mut file, at + 116, start);
        put_u64(&mut file, at + SIZE_AT, size);
    };
    entry(0, "Root Entry", 5, 1, END, 0);
    entry(1, "Data", 2, FREE, 2, stream_size);
    // Sector index 2 follows the header, so it starts at the third sector slot after it.
    let data_at = SECTOR * 3;
    file[data_at..data_at + data.len()].copy_from_slice(data);
    file
}

#[test]
fn version_4_file_reads_a_whole_sector_stream() {
    let data = vec![0x5A; 4096];
    let file = version_4_file(4096, &data);
    assert_eq!(
        unpacked(&file).expect("v4 opens"),
        vec![("Data".to_string(), data)]
    );
}

#[test]
fn version_4_keeps_the_high_half_so_an_oversized_stream_is_refused() {
    let data = vec![0x5A; 4096];
    for size in [(1u64 << 32) | 4096, 1 << 40, u64::MAX] {
        let file = version_4_file(size, &data);
        assert!(
            matches!(
                Ole2ContainerCodec::unpack(&file),
                Err(EasyOfficeError::CorruptedContainer(_))
            ),
            "size {size:#x} must not be cut to its low half"
        );
    }
}
