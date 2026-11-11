//! Which references of an object it owns (`MessageInfo.object_references`).
//!
//! A reference in a payload is `{1: identifier}` and nothing in the bytes says whether its owner
//! keeps the target alive. The apps write the strong ones next to the payload, in
//! `object_references`, and newer versions resolve a strong reference only if the list names it.
//! The measurement of which references are weak comes from the independent implementation named
//! below; a path that is not in the table is strong.
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig), `src/references.rs` 의 `WEAK` 표
//! (17-117행)에서 이 작성기가 쓰는 객체 형식(401, 2001, 2021-2025, 3016, 10001, 11006)의 항목만
//! 옮김. 출처는 THIRD_PARTY_NOTICES.md 에 남김.

use super::wire::{references, Fld};
use std::collections::BTreeSet;

/// `(message type, field path to the reference)` of every reference seen left out of the list.
/// Sorted, for the binary search.
const WEAK: &[(u32, &[u32])] = &[
    (401, &[1]),
    (401, &[5, 1]),
    (401, &[5, 2]),
    (401, &[8, 1]),
    (401, &[8, 3, 2]),
    (401, &[14, 1]),
    (401, &[14, 3, 2]),
    (2001, &[2]),
    (2001, &[5, 1]),
    (2001, &[8, 1]),
    (2001, &[11, 1]),
    (2001, &[19, 1]),
    (2001, &[20, 1]),
    (2021, &[1, 5]),
    (2021, &[11]),
    (2022, &[1, 5]),
    (2022, &[11]),
    (2022, &[12]),
    (2023, &[1, 5]),
    (2024, &[1, 5]),
    (2024, &[11, 7, 1]),
    (2025, &[1, 1, 5]),
    (2025, &[11]),
    (2025, &[11, 4, 1]),
    (3016, &[1, 5]),
    (10001, &[1, 4]),
    (11006, &[3, 6]),
];

/// Every referenced object of a payload, and the subset the object owns.
pub(super) struct Refs {
    pub all: BTreeSet<u64>,
    pub strong: BTreeSet<u64>,
}

pub(super) fn classify(type_id: u32, fields: &[Fld]) -> Refs {
    let mut refs = Refs {
        all: BTreeSet::new(),
        strong: BTreeSet::new(),
    };
    for (path, target) in references(fields) {
        refs.all.insert(target);
        if WEAK.binary_search(&(type_id, path.as_slice())).is_err() {
            refs.strong.insert(target);
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::super::wire::{msg, reference};
    use super::*;

    #[test]
    fn the_weak_table_is_sorted_for_the_binary_search() {
        assert!(WEAK.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_style_does_not_own_its_stylesheet_but_owns_its_parent() {
        let fields = [msg(1, vec![reference(3, 7), reference(5, 9)])];
        let refs = classify(2022, &fields);
        assert_eq!(refs.all, BTreeSet::from([7, 9]));
        assert_eq!(refs.strong, BTreeSet::from([7]));
    }
}
