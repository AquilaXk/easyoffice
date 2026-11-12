//! Wildcard text match in time linear in the text: `*` any run, `?` any one character, `~`
//! escapes `*`, `?` and `~`; letter case is ignored.
//!
//! 참조 구현: the match rule is the reference filter's text match (Univer
//! `packages/sheets-filter/src/models/custom-filters.ts` lines 141-152 `textMatch`, which builds a
//! regular expression from the pattern through `regexp.createFromWildcard`; Apache-2.0, Copyright
//! 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9). A regular
//! expression engine can backtrack without bound, so this port compiles the pattern into
//! segments between the stars and finds each segment with the bit-parallel shift-and search: one
//! pass over the text per segment, a state of at most four machine words, no backtracking.
//! `coercion::wildcard_match` (the COUNTIF rule) is the same language and the oracle of the tests.

/// Longest pattern in characters (one segment is a bit vector of `WORDS` words).
pub const MAX_PATTERN_CHARS: usize = 255;
const WORDS: usize = 4;

type Bits = [u64; WORDS];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Token {
    Lit(char),
    Any,
}

/// The one character that stands for `c` under case-insensitive comparison.
#[inline]
pub(super) fn fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

/// A run of non-star tokens, searched with shift-and.
struct Segment {
    tokens: Vec<Token>,
    /// Sorted folded characters of the literals and, per character, the positions it fills.
    chars: Vec<char>,
    masks: Vec<Bits>,
    /// Positions that accept any character.
    any: Bits,
}

impl Segment {
    fn new(tokens: Vec<Token>) -> Self {
        let mut any = [0u64; WORDS];
        let mut by_char: Vec<(char, usize)> = Vec::new();
        for (at, token) in tokens.iter().enumerate() {
            match token {
                Token::Any => any[at / 64] |= 1 << (at % 64),
                Token::Lit(c) => by_char.push((*c, at)),
            }
        }
        by_char.sort_unstable();
        let mut chars: Vec<char> = Vec::new();
        let mut masks: Vec<Bits> = Vec::new();
        for (c, at) in by_char {
            if chars.last() != Some(&c) {
                chars.push(c);
                masks.push(any);
            }
            if let Some(mask) = masks.last_mut() {
                mask[at / 64] |= 1 << (at % 64);
            }
        }
        Self {
            tokens,
            chars,
            masks,
            any,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.tokens.len()
    }

    #[inline]
    fn mask(&self, c: char) -> &Bits {
        match self.chars.binary_search(&c) {
            Ok(i) => &self.masks[i],
            Err(_) => &self.any,
        }
    }

    /// Whether the segment matches `text[at..at + len]`.
    fn matches_at(&self, text: &[char], at: usize) -> bool {
        text.get(at..at + self.len()).is_some_and(|window| {
            self.tokens
                .iter()
                .zip(window)
                .all(|(token, c)| matches!(token, Token::Any) || *token == Token::Lit(*c))
        })
    }

    /// End of the leftmost match that starts at or after the start of `text` and ends within it.
    fn find(&self, text: &[char]) -> Option<usize> {
        let last = self.len() - 1;
        let (word, bit) = (last / 64, last % 64);
        let words = word + 1;
        let mut state = [0u64; WORDS];
        for (i, &c) in text.iter().enumerate() {
            let mask = self.mask(c);
            let mut carry = 1u64;
            for w in 0..words {
                let shifted = (state[w] << 1) | carry;
                carry = state[w] >> 63;
                state[w] = shifted & mask[w];
            }
            if (state[word] >> bit) & 1 == 1 {
                return Some(i + 1);
            }
        }
        None
    }
}

/// A compiled pattern.
pub struct Wildcard {
    segments: Vec<Segment>,
    /// The pattern does not start with a star: the first segment must match at the start.
    anchored_start: bool,
    /// The pattern does not end with a star: the last segment must match at the end.
    anchored_end: bool,
    has_star: bool,
}

impl Wildcard {
    /// `None` when the pattern is longer than [`MAX_PATTERN_CHARS`] characters.
    pub fn new(pattern: &str) -> Option<Self> {
        let mut segments: Vec<Segment> = Vec::new();
        let mut tokens: Vec<Token> = Vec::new();
        let (mut starts_star, mut ends_star, mut has_star) = (false, false, false);
        let mut chars = pattern.chars().peekable();
        let mut count = 0usize;
        while let Some(c) = chars.next() {
            count += 1;
            if count > MAX_PATTERN_CHARS {
                return None;
            }
            ends_star = false;
            match c {
                '*' => {
                    has_star = true;
                    ends_star = true;
                    starts_star |= segments.is_empty() && tokens.is_empty();
                    if !tokens.is_empty() {
                        segments.push(Segment::new(std::mem::take(&mut tokens)));
                    }
                }
                '?' => tokens.push(Token::Any),
                '~' if matches!(chars.peek(), Some('*' | '?' | '~')) => {
                    tokens.push(Token::Lit(fold(chars.next().unwrap_or('~'))));
                }
                _ => tokens.push(Token::Lit(fold(c))),
            }
        }
        if !tokens.is_empty() {
            segments.push(Segment::new(tokens));
        }
        Some(Self {
            segments,
            anchored_start: !starts_star,
            anchored_end: !ends_star,
            has_star,
        })
    }

    /// Whether the pattern has no wildcard: it matches exactly the texts equal to it.
    pub fn is_literal(&self) -> bool {
        !self.has_star && self.segments.iter().all(|s| s.any.iter().all(|w| *w == 0))
    }

    /// Whether the whole of `text` matches. `scratch` holds the folded text and is reused.
    pub fn is_match(&self, text: &str, scratch: &mut Vec<char>) -> bool {
        scratch.clear();
        scratch.extend(text.chars().map(fold));
        self.matches_folded(scratch)
    }

    fn matches_folded(&self, text: &[char]) -> bool {
        let Some((first, rest)) = self.segments.split_first() else {
            // "" matches only the empty text, "*" (and "**") any text.
            return self.has_star || text.is_empty();
        };
        if !self.has_star {
            return text.len() == first.len() && first.matches_at(text, 0);
        }
        let mut from = 0usize;
        let mut to = text.len();
        let mut middle: &[Segment] = &self.segments;
        if self.anchored_start {
            if !first.matches_at(text, 0) {
                return false;
            }
            from = first.len();
            middle = rest;
        }
        if self.anchored_end {
            let Some((last, before)) = middle.split_last() else {
                return from == text.len();
            };
            let Some(at) = text.len().checked_sub(last.len()).filter(|&at| at >= from) else {
                return false;
            };
            if !last.matches_at(text, at) {
                return false;
            }
            to = at;
            middle = before;
        }
        for segment in middle {
            match segment.find(&text[from..to]) {
                Some(end) => from += end,
                None => return false,
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coercion::wildcard_match;

    fn is(pattern: &str, text: &str) -> bool {
        Wildcard::new(pattern)
            .unwrap()
            .is_match(text, &mut Vec::new())
    }

    #[test]
    fn stars_question_marks_and_escapes() {
        assert!(is("abc", "ABC") && !is("abc", "abcd") && !is("abc", "ab"));
        assert!(is("a*", "a") && is("a*", "abc") && !is("a*", "ba"));
        assert!(is("*c", "abc") && !is("*c", "cb") && is("*", "") && is("**", "xyz"));
        assert!(is("a*c", "ac") && is("a*c", "abbbc") && !is("a*c", "a"));
        assert!(is("*b*", "abc") && is("*b*", "b") && !is("*b*", "ac"));
        assert!(is("a?c", "abc") && !is("a?c", "ac") && !is("a?c", "abbc"));
        assert!(is("?*", "x") && !is("?*", ""));
        assert!(is("a~*b", "a*b") && !is("a~*b", "axb") && is("~?", "?") && is("~~", "~"));
        assert!(is("a~b", "a~b") && is("한*글", "한 국 글") && is("?글", "한글"));
        assert!(is("", "") && !is("", "a"));
        assert!(is("*ab*ab*", "xabyab") && !is("*ab*ab*", "xaby"));
        assert!(!is("a*a", "a") && is("a*a", "aa"));
        // An end segment may not reuse what the start segment took.
        assert!(!is("ab*ba", "aba") && is("ab*ba", "abba"));
    }

    #[test]
    fn a_pattern_over_the_bound_is_refused_and_a_long_one_still_matches() {
        assert!(Wildcard::new(&"a".repeat(MAX_PATTERN_CHARS + 1)).is_none());
        let pattern = format!("{}*{}", "a?".repeat(100), "b".repeat(50));
        let text = format!("{}{}", "ax".repeat(120), "b".repeat(60));
        assert!(is(&pattern, &text));
        assert!(!is(&pattern, &"ax".repeat(120)));
    }

    /// The same language as the COUNTIF matcher, on every short text over a small alphabet.
    #[test]
    fn agrees_with_the_criteria_matcher_on_all_short_inputs() {
        let alphabet = ['a', 'b', '*', '?', '~'];
        let mut all: Vec<String> = vec![String::new()];
        let mut frontier = all.clone();
        for _ in 0..5 {
            let mut next = Vec::new();
            for s in &frontier {
                for c in alphabet {
                    next.push(format!("{s}{c}"));
                }
            }
            all.extend(next.iter().cloned());
            frontier = next;
        }
        let texts: Vec<&String> = all
            .iter()
            .filter(|t| t.chars().all(|c| c == 'a' || c == 'b' || c == '?'))
            .collect();
        for pattern in &all {
            let compiled = Wildcard::new(pattern).unwrap();
            let mut scratch = Vec::new();
            for text in &texts {
                assert_eq!(
                    compiled.is_match(text, &mut scratch),
                    wildcard_match(text, pattern),
                    "pattern {pattern:?} text {text:?}"
                );
            }
        }
    }

    /// Text that makes a backtracking matcher exponential stays linear.
    #[test]
    fn hostile_input_is_linear() {
        let pattern = format!("{}b", "a*".repeat(60));
        let text = "a".repeat(30_000);
        let start = std::time::Instant::now();
        assert!(!is(&pattern, &text));
        let tricky = format!("{}c", "ab".repeat(15_000));
        assert!(!is("*a*a*a*a*a*a*a*a*b", &tricky.replace('b', "c")));
        assert!(start.elapsed().as_millis() < 2_000, "{:?}", start.elapsed());
    }
}
