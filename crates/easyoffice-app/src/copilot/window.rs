//! 참조 구현: continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., 커밋 5522c6f4, see THIRD_PARTY_NOTICES.md)
//! `core/llm/countTokens.ts:282-336`(`pruneLinesFromTop`, `pruneLinesFromBottom`)와
//! `core/autocomplete/templating/index.ts:251-285`(넘친 몫을 앞·뒤 크기 비례로 나눠 빼는 `renderPromptWithTokenLimit`)을 옮겼다.
//!
//! 선택 영역을 먼저 싣고 남은 문맥 예산으로 앞뒤 문단(슬라이드)을 채운다. 앞쪽은 선택에서 먼 위쪽 줄부터,
//! 뒤쪽은 먼 아래쪽 줄부터 뺀다. 원본은 토큰 수로 재고, 여기서는 줄마다 UTF-8 바이트(줄바꿈 포함)로 잰다.
//! 원본은 줄 수를 줄이며 합계를 다시 더하는 대신 인덱스만 옮기고, 이 구현도 슬라이스 경계만 옮겨 복사하지 않는다.

/// 문맥의 한 줄: 문서 안 번호와 이미 이스케이프를 마친 줄 글.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub index: usize,
    pub text: String,
}

impl Line {
    /// 줄글과 줄바꿈 한 개의 바이트 수.
    pub fn cost(&self) -> usize {
        self.text.len() + 1
    }
}

pub fn total_cost(lines: &[Line]) -> usize {
    lines.iter().map(Line::cost).sum()
}

// 옮김[continue-prune-top] 시작
/// 합계가 `max_bytes` 이하가 될 때까지 위쪽(앞쪽) 줄부터 뺀 나머지.
pub fn prune_lines_from_top(lines: &[Line], max_bytes: usize) -> &[Line] {
    let mut total = total_cost(lines);
    let mut start = 0;
    while total > max_bytes && start < lines.len() {
        total -= lines[start].cost();
        start += 1;
    }
    &lines[start..]
}
// 옮김[continue-prune-top] 끝

// 옮김[continue-prune-bottom] 시작
/// 합계가 `max_bytes` 이하가 될 때까지 아래쪽(뒤쪽) 줄부터 뺀 나머지.
pub fn prune_lines_from_bottom(lines: &[Line], max_bytes: usize) -> &[Line] {
    let mut total = total_cost(lines);
    let mut end = lines.len();
    while total > max_bytes && end > 0 {
        end -= 1;
        total -= lines[end].cost();
    }
    &lines[..end]
}
// 옮김[continue-prune-bottom] 끝

// 옮김[continue-split] 시작
/// 앞뒤 문맥이 `room` 바이트를 넘으면 넘친 몫을 두 쪽 크기에 비례해 나눠, 앞쪽은 위에서부터 뒤쪽은 아래에서부터 뺀다.
/// 줄 단위로 빼면 몫이 한 줄에 못 미치는 쪽은 통째로 사라지므로, 남은 몫은 아래 `refill`이 선택 가까운 줄부터 다시 채운다.
pub fn fit_around<'a>(
    prefix: &'a [Line],
    suffix: &'a [Line],
    room: usize,
) -> (&'a [Line], &'a [Line]) {
    let prefix_cost = total_cost(prefix);
    let suffix_cost = total_cost(suffix);
    let total = prefix_cost + suffix_cost;
    if total <= room {
        return (prefix, suffix);
    }
    let to_drop = total - room;
    let drop_prefix = (to_drop * prefix_cost).div_ceil(total);
    let drop_suffix = to_drop - drop_prefix;
    let kept_prefix = prune_lines_from_top(prefix, prefix_cost.saturating_sub(drop_prefix));
    let kept_suffix = prune_lines_from_bottom(suffix, suffix_cost.saturating_sub(drop_suffix));
    refill(prefix, suffix, kept_prefix.len(), kept_suffix.len(), room)
}
// 옮김[continue-split] 끝

/// 비례해 뺀 뒤 남은 예산에 선택에서 가까운 줄을 하나씩 다시 넣는다(줄이 적은 쪽부터). 직접 작성했다.
fn refill<'a>(
    prefix: &'a [Line],
    suffix: &'a [Line],
    kept_before: usize,
    kept_after: usize,
    room: usize,
) -> (&'a [Line], &'a [Line]) {
    let mut start = prefix.len() - kept_before;
    let mut end = kept_after;
    let mut left = room - total_cost(&prefix[start..]) - total_cost(&suffix[..end]);
    loop {
        let before_next = start
            .checked_sub(1)
            .map(|i| prefix[i].cost())
            .filter(|c| *c <= left);
        let after_next = suffix.get(end).map(Line::cost).filter(|c| *c <= left);
        let take_before = match (before_next, after_next) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(_), Some(_)) => prefix.len() - start <= end,
        };
        if take_before {
            start -= 1;
            left -= prefix[start].cost();
        } else {
            left -= suffix[end].cost();
            end += 1;
        }
    }
    (&prefix[start..], &suffix[..end])
}

/// `lines`(번호 오름차순) 가운데 선택 범위 `span` 안의 줄과 앞·뒤 줄.
pub struct Windowed<'a> {
    pub before: &'a [Line],
    pub selected: &'a [Line],
    pub after: &'a [Line],
}

pub fn partition<'a>(lines: &'a [Line], span: &std::ops::RangeInclusive<usize>) -> Windowed<'a> {
    let first = lines.partition_point(|l| l.index < *span.start());
    let last = lines.partition_point(|l| l.index <= *span.end());
    Windowed {
        before: &lines[..first],
        selected: &lines[first..last],
        after: &lines[last..],
    }
}

/// 선택을 먼저 싣고 남은 `budget`으로 앞뒤를 채운다. 선택만으로 예산을 넘으면 `None`이다.
pub fn select_window<'a>(
    lines: &'a [Line],
    span: &std::ops::RangeInclusive<usize>,
    budget: usize,
) -> Option<(&'a [Line], &'a [Line], &'a [Line])> {
    let parts = partition(lines, span);
    let room = budget.checked_sub(total_cost(parts.selected))?;
    let (before, after) = fit_around(parts.before, parts.after, room);
    Some((before, parts.selected, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(sizes: &[usize]) -> Vec<Line> {
        sizes
            .iter()
            .enumerate()
            .map(|(index, size)| Line {
                index,
                text: "x".repeat(*size - 1),
            })
            .collect()
    }

    #[test]
    fn prune_from_top_drops_the_earliest_lines_first() {
        let l = lines(&[10, 10, 10, 10]);
        assert_eq!(prune_lines_from_top(&l, 25).len(), 2);
        assert_eq!(prune_lines_from_top(&l, 25)[0].index, 2);
        assert_eq!(prune_lines_from_top(&l, 40).len(), 4);
        assert!(prune_lines_from_top(&l, 5).is_empty());
    }

    #[test]
    fn prune_from_bottom_drops_the_latest_lines_first() {
        let l = lines(&[10, 10, 10, 10]);
        let kept = prune_lines_from_bottom(&l, 25);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].index, 1);
        assert!(prune_lines_from_bottom(&l, 0).is_empty());
    }

    #[test]
    fn the_excess_is_shared_in_proportion_to_each_side() {
        let before = lines(&[100; 9]);
        let after = lines(&[100; 3]);
        // 합계 1200에서 400을 빼야 한다: 앞쪽 몫 300, 뒤쪽 몫 100.
        let (b, a) = fit_around(&before, &after, 800);
        assert_eq!((b.len(), a.len()), (6, 2));
        assert_eq!(b[0].index, 3, "the earliest lines go first");
        assert_eq!(a[1].index, 1, "the latest lines go first");
        // 몫이 한 줄에 못 미쳐 통째로 사라진 쪽은 남은 예산으로 가장 가까운 줄부터 되돌린다.
        let (head, tail) = (lines(&[100; 3]), lines(&[100; 3]));
        let (b, a) = fit_around(&head, &tail, 250);
        assert_eq!((b.len(), a.len()), (1, 1));
        assert_eq!(
            (b[0].index, a[0].index),
            (2, 0),
            "the lines nearest to the selection stay"
        );
        let (b, a) = fit_around(&before, &after, 10_000);
        assert_eq!((b.len(), a.len()), (9, 3));
    }

    #[test]
    fn a_selection_over_the_budget_has_no_window() {
        let l = lines(&[100, 100, 100]);
        assert!(select_window(&l, &(1..=1), 99).is_none());
        let (before, selected, after) = select_window(&l, &(1..=1), 250).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(
            before.len() + after.len(),
            1,
            "the left-over room takes back one line"
        );
    }
}
