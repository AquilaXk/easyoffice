//! 참조 구현: IronCalc `base/src/test/dynamic_evaluation/test_order_independence.rs` (기준 커밋
//! 6dc9e8bae4b8d066276f399de7b3e1cf991c220b)의 무작위 시트 생성(상수, 스칼라 수식, 범위 펼치기,
//! `SEQUENCE`, `#`; `OFFSET`은 휘발성이라 뺐습니다)과 시험 구조를 옮겼습니다(MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH). 이 엔진은 평가 순서를 기억하지 않고
//! 펼친 영역을 반영한 뒤 건드린 칸을 다시 계산하므로, 다음을 무작위 시트 1,000개로 확인합니다.
//! 1. 고정점: 한 번 더 계산해도 바뀌는 칸이 없다.
//! 2. 입력 순서 무관: 같은 칸을 섞은 순서로 넣어도 같은 시트가 된다.
//! 3. 증분 일치: 칸을 하나씩 넣으며 계산한 결과가 한꺼번에 넣은 결과와 같다.
//!
//! 영역이 먼저 차지한 쪽이 이기는 `#SPILL!`·순환이 끼면 이력에 따라 달라지므로 1~3을 완화합니다.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::{new_empty_model, Model};

const ROWS: u64 = 5;
const COLS: u64 = 5;
const CELLS_PER_SHEET: usize = 12;
const SEEDS: u64 = 1000;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn cell_name(row: u64, column: u64) -> String {
    format!("{}{}", (b'A' + (column - 1) as u8) as char, row)
}

fn random_ref(rng: &mut Rng) -> String {
    cell_name(1 + rng.below(ROWS), 1 + rng.below(COLS))
}

fn random_content(rng: &mut Rng) -> String {
    match rng.below(15) {
        0 | 1 => format!("{}", 1 + rng.below(4)),
        2 => format!("={}+1", random_ref(rng)),
        3 => format!("={}*2", random_ref(rng)),
        4 => {
            let r = random_ref(rng);
            format!("=SUM({r}:{})", random_ref(rng))
        }
        5 => {
            let (row, column) = (1 + rng.below(ROWS), 1 + rng.below(COLS));
            format!("={}:{}", cell_name(row, column), cell_name(row + 2, column))
        }
        6 => format!("=SEQUENCE({})", 1 + rng.below(3)),
        7 => format!("=SEQUENCE({})", random_ref(rng)),
        8 => format!("={}#", random_ref(rng)),
        9 => format!("=SUM({}#)", random_ref(rng)),
        10 => format!(
            "=SUM({}:{})",
            random_ref(rng),
            cell_name(1 + rng.below(ROWS), 1 + rng.below(COLS))
        ),
        11 => format!(
            "=SORT({}:{})",
            random_ref(rng),
            cell_name(1 + rng.below(ROWS), 1 + rng.below(COLS))
        ),
        12 => format!("={}+{}", random_ref(rng), random_ref(rng)),
        13 => format!("=SEQUENCE({}+{})", random_ref(rng), random_ref(rng)),
        _ => {
            let r = random_ref(rng);
            let r2 = random_ref(rng);
            format!("=SUM({r}:{r2})+{}", random_ref(rng))
        }
    }
}

fn random_sheet(seed: u64) -> Vec<(String, String)> {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut cells: Vec<(String, String)> = Vec::new();
    while cells.len() < CELLS_PER_SHEET {
        let reference = random_ref(&mut rng);
        if cells.iter().any(|(r, _)| *r == reference) {
            continue;
        }
        cells.push((reference, random_content(&mut rng)));
    }
    cells
}

fn shuffled(cells: &[(String, String)], seed: u64) -> Vec<(String, String)> {
    let mut rng = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
    let mut result = cells.to_vec();
    for i in (1..result.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        result.swap(i, j);
    }
    result
}

/// Text of every cell in a generous area (arrays extend past the input grid).
fn snapshot(model: &Model) -> Vec<String> {
    let mut lines = Vec::new();
    for row in 1..=ROWS + 6 {
        for column in 1..=COLS + 6 {
            let name = cell_name(row, column);
            let text = model._get_text(&name);
            if !text.is_empty() {
                lines.push(format!("{name}: {text}"));
            }
        }
    }
    lines
}

fn describe(cells: &[(String, String)]) -> String {
    cells
        .iter()
        .map(|(r, c)| format!("  {r} = {c}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn random_sheets_are_order_independent_fixed_points() {
    let mut failures = Vec::new();
    for seed in 1..=SEEDS {
        let cells = random_sheet(seed);

        let mut all_at_once = new_empty_model();
        for (reference, content) in &cells {
            all_at_once._set(reference, content);
        }
        all_at_once.evaluate();
        let first = snapshot(&all_at_once);
        all_at_once.evaluate();
        let second = snapshot(&all_at_once);
        let settled = all_at_once.last.as_ref().unwrap().changed.is_empty();

        let mut other = new_empty_model();
        for (reference, content) in &shuffled(&cells, seed) {
            other._set(reference, content);
        }
        other.evaluate();
        let other_first = snapshot(&other);

        let mut stepwise = new_empty_model();
        for (reference, content) in &cells {
            stepwise._set(reference, content);
            stepwise.evaluate();
        }
        let stepwise_first = snapshot(&stepwise);

        let history_dependent = first
            .iter()
            .chain(&second)
            .chain(&other_first)
            .chain(&stepwise_first)
            .any(|l| l.ends_with("#SPILL!") || l.ends_with("#NUM!"));
        let same_state = history_dependent
            || (first == second && first == other_first && first == stepwise_first && settled);
        if !same_state {
            failures.push(format!(
                "seed {seed}\n{}\nall at once:\n  {}\nsecond evaluation:\n  {}\nshuffled insertion:\n  {}\nstep by step:\n  {}",
                describe(&cells),
                first.join("\n  "),
                second.join("\n  "),
                other_first.join("\n  "),
                stepwise_first.join("\n  "),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {SEEDS} random sheets failed:\n\n{}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}
