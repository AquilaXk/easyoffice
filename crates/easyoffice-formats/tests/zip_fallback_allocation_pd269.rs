//! The `zip` crate drops an end record it cannot use and tries an older one, and a ZIP64 end record
//! can announce any entry count; the crate then reserves memory for the announced count before it
//! reads a byte of the directory. The reader must never let that fallback happen: the largest single
//! allocation while it refuses the archive stays small.

use easyoffice_formats::{ZipArchiveCodec, ZipSecurityPolicy};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Tracking;

thread_local! {
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(layout.size())));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(new_size)));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Tracking = Tracking;

/// The announced entry count of the hidden ZIP64 record. The crate only believes a count that the
/// file is long enough to hold (46 bytes per entry), so the archive is padded to that length; it
/// would reserve about 50 MB for the count if it followed the record.
const HUGE_COUNT: u64 = 200_000;
/// Most one refusal may reserve at once (the archive itself is a few hundred bytes).
const SMALL: usize = 8 * 1024 * 1024;

fn eocd(entries: u16, size: u32, offset: u32) -> Vec<u8> {
    let mut out = vec![0x50, 0x4B, 0x05, 0x06, 0, 0, 0, 0];
    out.extend_from_slice(&entries.to_le_bytes());
    out.extend_from_slice(&entries.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// A local entry, then (older to newer) a ZIP64 end record with a huge count, its locator, an end
/// record of 0xFFFF entries that points at them, and last the end record that really ends the file:
/// one entry whose directory offset has no directory entry behind it.
fn hostile_archive() -> Vec<u8> {
    let real = ZipArchiveCodec::pack(&[("a.xml".to_string(), b"hello".to_vec())]).unwrap();
    let cd = real
        .windows(4)
        .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
        .unwrap();
    let mut out = real[..cd].to_vec();
    out.resize(out.len() + HUGE_COUNT as usize * 47, 0);
    let eocd64_at = out.len() as u64;
    out.extend_from_slice(&[0x50, 0x4B, 0x06, 0x06]);
    out.extend_from_slice(&44u64.to_le_bytes());
    out.extend_from_slice(&[45, 0, 45, 0]);
    out.extend_from_slice(&[0; 8]); // disk numbers
    out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
    out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // directory size
    out.extend_from_slice(&HUGE_COUNT.to_le_bytes()); // directory offset
    out.extend_from_slice(&[0x50, 0x4B, 0x06, 0x07]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&eocd64_at.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF));
    out.extend_from_slice(&eocd(1, 0, 0));
    out
}

#[test]
fn an_older_zip64_end_record_is_never_tried() {
    let archive = hostile_archive();
    LARGEST.with(|largest| largest.set(0));
    let refused = ZipArchiveCodec::unpack(&archive, &ZipSecurityPolicy::default());
    let largest = LARGEST.with(Cell::get);
    assert!(refused.is_err());
    assert!(largest < SMALL, "one refusal reserved {largest} bytes");
}
