//! Set arithmetic on sorted, disjoint inclusive runs of rows.

/// The rows in both `a` and `b`.
pub fn intersect_runs(a: &[(u32, u32)], b: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let start = a[i].0.max(b[j].0);
        let end = a[i].1.min(b[j].1);
        if start <= end {
            out.push((start, end));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// The rows of `a` that are not in `b`.
pub fn subtract_runs(a: &[(u32, u32)], b: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut j = 0;
    for &(start, end) in a {
        let mut from = start;
        while j < b.len() && b[j].1 < from {
            j += 1;
        }
        let mut k = j;
        while k < b.len() && b[k].0 <= end {
            if b[k].0 > from {
                out.push((from, b[k].0 - 1));
            }
            from = from.max(b[k].1.saturating_add(1));
            if b[k].1 >= end {
                from = end.saturating_add(1);
                break;
            }
            k += 1;
        }
        if from <= end {
            out.push((from, end));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(runs: &[(u32, u32)]) -> Vec<u32> {
        runs.iter().flat_map(|&(s, e)| s..=e).collect()
    }

    #[test]
    fn intersection_and_difference_agree_with_row_sets() {
        let cases: [(&[(u32, u32)], &[(u32, u32)]); 5] = [
            (&[(1, 5), (8, 12), (20, 20)], &[(3, 9), (12, 25)]),
            (&[(0, 100)], &[(10, 20), (30, 40), (100, 100)]),
            (&[(5, 6)], &[]),
            (&[], &[(1, 2)]),
            (&[(2, 3), (7, 9)], &[(0, 1), (4, 6), (10, 11)]),
        ];
        for (a, b) in cases {
            let (sa, sb) = (rows(a), rows(b));
            let both: Vec<u32> = sa.iter().copied().filter(|r| sb.contains(r)).collect();
            let only: Vec<u32> = sa.iter().copied().filter(|r| !sb.contains(r)).collect();
            assert_eq!(rows(&intersect_runs(a, b)), both, "{a:?} {b:?}");
            assert_eq!(rows(&subtract_runs(a, b)), only, "{a:?} {b:?}");
        }
    }
}
