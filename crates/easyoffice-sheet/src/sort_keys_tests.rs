use super::*;

fn col(ascending: bool, case_sensitive: bool, cells: &[Option<&str>]) -> SortColumn {
    let mut c = SortColumn::new(ascending, case_sensitive, cells.len());
    for cell in cells {
        match cell {
            None => c.push_empty(),
            Some(t) => match t.parse::<f64>() {
                Ok(n) => c.push_number(n),
                Err(_) => c.push_text(t),
            },
        }
    }
    c
}

#[test]
fn numbers_come_before_text_and_blanks_are_last_in_both_directions() {
    let cells = [Some("b"), None, Some("10"), Some("a"), Some("2"), Some("")];
    let asc = sort_order(&[col(true, false, &cells)]);
    // 2, 10, a, b, then the two blanks in their original order (None at 1, "" at 5).
    assert_eq!(asc, vec![4, 2, 3, 0, 1, 5]);
    let desc = sort_order(&[col(false, false, &cells)]);
    assert_eq!(desc, vec![0, 3, 2, 4, 1, 5]);
}

#[test]
fn equal_keys_keep_their_original_order() {
    let cells = [Some("1"), Some("1"), Some("0"), Some("1")];
    assert_eq!(sort_order(&[col(true, false, &cells)]), vec![2, 0, 1, 3]);
    assert_eq!(sort_order(&[col(false, false, &cells)]), vec![0, 1, 3, 2]);
}

#[test]
fn later_keys_break_ties_of_earlier_ones() {
    let first = col(true, false, &[Some("1"), Some("1"), Some("0"), Some("1")]);
    let second = col(false, false, &[Some("5"), Some("9"), Some("7"), Some("9")]);
    assert_eq!(sort_order(&[first, second]), vec![2, 1, 3, 0]);
}

#[test]
fn case_is_ignored_unless_asked_and_then_lowercase_comes_first() {
    let cells = [Some("b"), Some("B"), Some("a"), Some("A")];
    assert_eq!(sort_order(&[col(true, false, &cells)]), vec![2, 3, 0, 1]);
    assert_eq!(sort_order(&[col(true, true, &cells)]), vec![2, 3, 0, 1]);
    // Case-sensitive: a, A, b, B; descending reverses the whole order.
    let cells = [Some("A"), Some("a"), Some("B"), Some("b")];
    assert_eq!(sort_order(&[col(true, true, &cells)]), vec![1, 0, 3, 2]);
    assert_eq!(sort_order(&[col(false, true, &cells)]), vec![2, 3, 0, 1]);
    // Without the option the equal keys keep their order.
    assert_eq!(sort_order(&[col(true, false, &cells)]), vec![0, 1, 2, 3]);
}

#[test]
fn hyphens_and_apostrophes_do_not_count_in_text() {
    let cells = [Some("co-op"), Some("coop"), Some("cop"), Some("it's")];
    // "coop" and "co-op" are equal keys (original order), "cop" follows, "it's" ~ "its".
    assert_eq!(sort_order(&[col(true, false, &cells)]), vec![0, 1, 2, 3]);
}

#[test]
fn korean_text_sorts_by_syllable_order() {
    let cells = [Some("다"), Some("가"), Some("나"), Some("ABC")];
    assert_eq!(sort_order(&[col(true, false, &cells)]), vec![3, 1, 2, 0]);
}

#[test]
fn an_empty_request_gives_the_identity_and_big_input_is_stable() {
    assert!(sort_order(&[]).is_empty());
    let mut c = SortColumn::new(true, false, 1000);
    for i in 0..1000u32 {
        c.push_number(f64::from(i % 3));
    }
    let order = sort_order(&[c]);
    assert!(order
        .windows(2)
        .all(|w| (w[0] % 3, w[0]) < (w[1] % 3, w[1])));
}
