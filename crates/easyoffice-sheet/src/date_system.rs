//! The date system a calculation runs in (PD-263): serial 1 is 1900-01-01 in the 1900 system and 1904-01-02 in the 1904 system
//! (day 0 is 1904-01-01), so the same day has a serial 1,462 lower. The calendar functions, the date number formats and the
//! date filters all go through `civil_from_serial` and `serial_from_civil`, which take the system from here; the weekday and
//! `TODAY` arithmetic do too. A caller enters the system of the workbook it works on before it calculates or formats, and the
//! guard puts the previous one back. The system is per thread, as the format cache is.

use std::cell::Cell;

/// Days from 1899-12-30 (serial 0 of the 1900 system) to 1904-01-01 (serial 0 of the 1904 system).
pub const SERIAL_1904_OFFSET: i64 = 1462;

thread_local! {
    static DATE_1904: Cell<bool> = const { Cell::new(false) };
}

/// Keeps a date system for its lifetime; the previous one comes back when it is dropped.
#[must_use = "the date system is only in effect while the guard lives"]
pub struct DateSystemGuard {
    previous: bool,
}

impl Drop for DateSystemGuard {
    fn drop(&mut self) {
        DATE_1904.with(|flag| flag.set(self.previous));
    }
}

/// Makes the 1904 system (`true`) or the 1900 system (`false`) the one this thread calculates in.
pub fn enter_date_system(date1904: bool) -> DateSystemGuard {
    DateSystemGuard {
        previous: DATE_1904.with(|flag| flag.replace(date1904)),
    }
}

/// Whether this thread calculates in the 1904 system.
pub fn in_date_1904() -> bool {
    DATE_1904.with(Cell::get)
}

/// The serial of the 1900 system that names the same day as `serial` of the current system.
#[inline]
pub(crate) fn to_1900(serial: i64) -> i64 {
    if in_date_1904() {
        serial + SERIAL_1904_OFFSET
    } else {
        serial
    }
}

/// The serial of the current system for `serial` of the 1900 system.
#[inline]
pub(crate) fn from_1900(serial: i64) -> i64 {
    if in_date_1904() {
        serial - SERIAL_1904_OFFSET
    } else {
        serial
    }
}
