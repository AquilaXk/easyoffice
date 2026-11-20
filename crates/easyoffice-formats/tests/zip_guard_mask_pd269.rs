//! `open_guarded` reads the archive through a view in which everything before the central directory
//! is zeros while the `zip` crate looks for its end records (PD-269). When the directory cannot be
//! read, the crate goes on to the older end records it can find; these tests put such a record
//! chain where only the masked view hides it, so the refusal is the mask's doing and not a check
//! made earlier.

use easyoffice_formats::{open_guarded, OpenLimits};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::Cursor;

/// Records the largest single allocation of the thread that asks for it.
struct Tracking;

thread_local! {
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: every call is forwarded unchanged to the system allocator.
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

const HUGE_COUNT: u64 = 200_000;
const LOCAL_HEADER_LEN: usize = 30;
const CENTRAL_HEADER_LEN: usize = 46;
const DOS_DATE_1980: u16 = 0x0021;

fn eocd(entries: u16, size: u32, offset: u32) -> Vec<u8> {
    let mut out = vec![0x50, 0x4B, 0x05, 0x06, 0, 0, 0, 0];
    out.extend_from_slice(&entries.to_le_bytes());
    out.extend_from_slice(&entries.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// A ZIP64 end record announcing [`HUGE_COUNT`] entries, its locator, and an end record whose
/// fields all say "look in the ZIP64 record". `at` is where the first byte lands in the file.
fn hidden_zip64_chain(at: usize) -> Vec<u8> {
    let mut out = vec![0x50, 0x4B, 0x06, 0x06];
    out.extend_from_slice(&44u64.to_le_bytes());
    out.extend_from_slice(&[45, 0, 45, 0]);
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
    out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&HUGE_COUNT.to_le_bytes());
    out.extend_from_slice(&[0x50, 0x4B, 0x06, 0x07]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(at as u64).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&eocd(0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF));
    out
}

fn central_header(data_len: u32, name: u8) -> Vec<u8> {
    let mut out = vec![0x50, 0x4B, 0x01, 0x02, 20, 0, 10, 0, 0, 0, 0, 0, 0, 0];
    out.extend_from_slice(&DOS_DATE_1980.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(name);
    out
}

/// One stored entry whose data is zeros followed by [`hidden_zip64_chain`]. The directory has a
/// good first record and, when `break_second` is set, a second record with a wrong signature.
fn archive_hiding_end_records(break_second: bool) -> Vec<u8> {
    let name_at = LOCAL_HEADER_LEN;
    let data_at = name_at + 1;
    let padding = HUGE_COUNT as usize * 47;
    let chain = hidden_zip64_chain(data_at + padding);
    let data_len = (padding + chain.len()) as u32;

    let mut out = vec![0x50, 0x4B, 0x03, 0x04, 10, 0, 0, 0, 0, 0, 0, 0];
    out.extend_from_slice(&DOS_DATE_1980.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.push(b'a');
    out.resize(data_at + padding, 0);
    out.extend_from_slice(&chain);

    let directory_at = out.len();
    out.extend_from_slice(&central_header(data_len, b'a'));
    let mut second = central_header(data_len, b'b');
    if break_second {
        second[2] = 0x01;
        second[3] = 0x03;
    }
    out.extend_from_slice(&second);
    let directory_len = out.len() - directory_at;
    debug_assert_eq!(directory_len, 2 * (CENTRAL_HEADER_LEN + 1));
    out.extend_from_slice(&eocd(2, directory_len as u32, directory_at as u32));
    out
}

const LIMITS: OpenLimits = OpenLimits {
    max_entries: 8_192,
    max_directory_bytes: 4 * 1024 * 1024,
};

#[test]
fn the_archive_with_both_records_good_opens() {
    let archive = archive_hiding_end_records(false);
    let opened = open_guarded(Cursor::new(&archive), LIMITS).expect("a plain two entry archive");
    assert_eq!(opened.total_entries, 2);
    assert_eq!(
        opened.archive.central_directory_start(),
        opened.directory_start
    );
}

#[test]
fn an_unreadable_directory_does_not_lead_the_crate_to_an_end_record_inside_entry_data() {
    let archive = archive_hiding_end_records(true);
    LARGEST.with(|largest| largest.set(0));
    let opened = open_guarded(Cursor::new(&archive), LIMITS);
    let largest = LARGEST.with(Cell::get);
    assert!(opened.is_err(), "the broken second record must be an error");
    assert!(
        largest < 8 * 1024 * 1024,
        "the crate reserved {largest} bytes for a count taken from the hidden record"
    );
}
