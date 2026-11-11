//! Memo of font requests for one PDF export (PD-163).
//!
//! usvg asks its font selector once per `<text>` element, and the stock selector
//! ([`usvg::FontResolver::default_font_selector`]) answers each ask by filtering every face of the
//! database (`fontdb::Database::query`, several hundred faces on a desktop). A report repeats the
//! same few requests (family list, weight, stretch, style) thousands of times, so the answer for a
//! request is kept after the first lookup. The memo is made for one export and one database: the
//! database is never changed during an export, so an answer does not go stale, and the memo is
//! dropped with the export.
//!
//! The memo is bounded for an untrusted page SVG: it keeps at most [`MAX_MEMO_ENTRIES`] requests,
//! and a request whose family names are longer than [`MAX_KEY_NAME_BYTES`] in sum is looked up
//! without being kept. A full memo still answers correctly, it only stops growing.

use std::collections::HashMap;
use std::sync::RwLock;
use usvg::fontdb::ID;

/// Most distinct font requests one export remembers.
pub const MAX_MEMO_ENTRIES: usize = 4_096;
/// Longest family-name text (bytes, summed over the list) of a request that is remembered.
pub const MAX_KEY_NAME_BYTES: usize = 512;

fn name_bytes(font: &usvg::Font) -> usize {
    font.families()
        .iter()
        .map(|family| match family {
            usvg::FontFamily::Named(name) => name.len(),
            _ => 0,
        })
        .sum()
}

/// `select` with its answers remembered, at most `max_entries` requests.
pub fn memoized<'a>(
    select: usvg::FontSelectionFn<'a>,
    max_entries: usize,
) -> usvg::FontSelectionFn<'a> {
    let memo: RwLock<HashMap<usvg::Font, Option<ID>>> = RwLock::new(HashMap::new());
    Box::new(move |font, db| {
        if let Some(id) = memo.read().ok().and_then(|memo| memo.get(font).copied()) {
            return id;
        }
        let id = select(font, db);
        if name_bytes(font) <= MAX_KEY_NAME_BYTES {
            if let Ok(mut memo) = memo.write() {
                if memo.len() < max_entries {
                    memo.insert(font.clone(), id);
                }
            }
        }
        id
    })
}

/// The stock selector with its answers remembered: the selector of one export.
pub fn memoized_stock_selector() -> usvg::FontSelectionFn<'static> {
    memoized(
        usvg::FontResolver::default_font_selector(),
        MAX_MEMO_ENTRIES,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use usvg::fontdb::Database;

    fn font(families: &str, weight: u16) -> usvg::Font {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text font-family="{families}" font-weight="{weight}">a</text></svg>"#
        );
        let asked = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&asked);
        let options = usvg::Options {
            font_resolver: usvg::FontResolver {
                select_font: Box::new(move |font, _| {
                    *sink.lock().expect("lock") = Some(font.clone());
                    None
                }),
                ..usvg::FontResolver::default()
            },
            ..usvg::Options::default()
        };
        usvg::Tree::from_str(&svg, &options).expect("tree");
        let found = asked.lock().expect("lock").take();
        found.expect("a font request")
    }

    fn counting(counter: &Arc<AtomicUsize>) -> usvg::FontSelectionFn<'static> {
        let counter = Arc::clone(counter);
        Box::new(move |_, _| {
            counter.fetch_add(1, Ordering::Relaxed);
            None
        })
    }

    #[test]
    fn a_repeated_request_asks_the_database_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let select = memoized(counting(&calls), MAX_MEMO_ENTRIES);
        let mut db = Arc::new(Database::new());
        let (a, b) = (font("Foo", 400), font("Foo", 700));
        for _ in 0..5 {
            assert_eq!(select(&a, &mut db), None);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        select(&b, &mut db);
        select(&a, &mut db);
        assert_eq!(
            calls.load(Ordering::Relaxed),
            2,
            "weight is part of the key"
        );
    }

    #[test]
    fn the_memo_stops_growing_at_its_bound_and_still_answers() {
        let calls = Arc::new(AtomicUsize::new(0));
        let select = memoized(counting(&calls), 2);
        let mut db = Arc::new(Database::new());
        let fonts: Vec<_> = (0..4).map(|n| font(&format!("F{n}"), 400)).collect();
        for font in &fonts {
            select(font, &mut db);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 4);
        for font in &fonts {
            select(font, &mut db);
        }
        assert_eq!(
            calls.load(Ordering::Relaxed),
            6,
            "only the two kept ones are answered from memory"
        );
    }

    #[test]
    fn a_request_with_long_names_is_not_kept() {
        let calls = Arc::new(AtomicUsize::new(0));
        let select = memoized(counting(&calls), MAX_MEMO_ENTRIES);
        let mut db = Arc::new(Database::new());
        let long = font(&"x".repeat(MAX_KEY_NAME_BYTES + 1), 400);
        select(&long, &mut db);
        select(&long, &mut db);
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn the_memo_answers_what_the_stock_selector_answers() {
        let Some(db) = crate::pdf_fonts::tests::bundled_only_db() else {
            return;
        };
        let mut db = Arc::new(db);
        let stock = usvg::FontResolver::default_font_selector();
        let select = memoized_stock_selector();
        for (families, weight) in [
            ("Noto Sans KR", 400),
            ("Missing, serif", 700),
            ("Missing", 300),
        ] {
            let request = font(families, weight);
            let want = stock(&request, &mut db);
            assert!(want.is_some(), "{families}");
            assert_eq!(select(&request, &mut db), want, "{families} first");
            assert_eq!(select(&request, &mut db), want, "{families} remembered");
        }
    }
}
