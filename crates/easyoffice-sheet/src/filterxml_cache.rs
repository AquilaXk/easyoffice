//! Results of `FILTERXML` within one evaluation pass (PD-363). A sheet that repeats one document
//! and one query in many cells parses and walks the document once. Only successful results are
//! kept: a failure can come from the step budget of the cell that ran it, and the next cell has a
//! fresh budget. The cache is bounded and stops taking entries when it is full.

use crate::EvaluatedCellValue as V;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Results one pass keeps.
const MAX_ENTRIES: usize = 256;
/// Bytes of document text, query text and result values one pass keeps.
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
struct Entry {
    xml: Box<str>,
    xpath: Box<str>,
    found: Vec<V>,
}

#[derive(Debug, Default)]
pub(crate) struct FilterXmlCache {
    /// Entries by the hash of their two texts; equal hashes are told apart by the texts.
    entries: HashMap<u64, Vec<Entry>>,
    count: usize,
    bytes: usize,
}

fn key(xml: &str, xpath: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    xml.hash(&mut hasher);
    xpath.hash(&mut hasher);
    hasher.finish()
}

fn value_bytes(found: &[V]) -> usize {
    found
        .iter()
        .map(|v| std::mem::size_of::<V>() + v.heap_bytes())
        .sum()
}

impl FilterXmlCache {
    pub(crate) fn get(&self, xml: &str, xpath: &str) -> Option<Vec<V>> {
        self.entries
            .get(&key(xml, xpath))?
            .iter()
            .find(|e| &*e.xml == xml && &*e.xpath == xpath)
            .map(|e| e.found.clone())
    }

    pub(crate) fn insert(&mut self, xml: &str, xpath: &str, found: &[V]) {
        let bytes = xml.len() + xpath.len() + value_bytes(found);
        if self.count >= MAX_ENTRIES || self.bytes.saturating_add(bytes) > MAX_BYTES {
            return;
        }
        let bucket = self.entries.entry(key(xml, xpath)).or_default();
        if bucket.iter().any(|e| &*e.xml == xml && &*e.xpath == xpath) {
            return;
        }
        bucket.push(Entry {
            xml: xml.into(),
            xpath: xpath.into(),
            found: found.to_vec(),
        });
        self.count += 1;
        self.bytes += bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_result_comes_back_for_the_same_pair_only() {
        let mut cache = FilterXmlCache::default();
        let found = vec![V::Number(7.0), V::Text("a".into())];
        assert!(cache.get("<r/>", "/r").is_none());
        cache.insert("<r/>", "/r", &found);
        assert_eq!(cache.get("<r/>", "/r"), Some(found));
        assert!(cache.get("<r/>", "/q").is_none());
        assert!(cache.get("<q/>", "/r").is_none());
    }

    #[test]
    fn the_cache_stops_taking_entries_when_full() {
        let mut cache = FilterXmlCache::default();
        for i in 0..MAX_ENTRIES + 10 {
            cache.insert(&format!("<r>{i}</r>"), "/r", &[V::Number(i as f64)]);
        }
        assert_eq!(cache.count, MAX_ENTRIES);
        assert!(cache.get("<r>0</r>", "/r").is_some());
        assert!(cache
            .get(&format!("<r>{}</r>", MAX_ENTRIES + 5), "/r")
            .is_none());

        let mut small = FilterXmlCache::default();
        let big = "x".repeat(MAX_BYTES);
        small.insert(&big, "/r", &[]);
        assert!(small.get(&big, "/r").is_none());
    }
}
