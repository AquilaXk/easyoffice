//! Line breaking for the text pages of the PDF writer, by glyph advance width.
//!
//! A line is shown with one font: Helvetica when it is ASCII, otherwise the predefined Korean CID
//! font. Helvetica advances are the published metrics of that standard font (the PDF base-14
//! Helvetica AFM, WinAnsi codes 32..=126, thousandths of the font size). The Korean font is
//! measured with the widths the writer declares for it (`/DW 1000` and the Helvetica widths for the
//! ASCII CIDs, `/W [1 [...]]`): ASCII keeps its Helvetica width, every other character is full width. Breaks fall after spaces; a word wider than a whole
//! line is split between characters, and breaking never drops text except the spaces at a break.

use std::ops::Range;

/// Thousandths of the font size by ASCII code `0x20..=0x7E` (Helvetica).
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278,
    278, // space .. /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 0 .. ?
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // @ .. O
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // P .. _
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // ` .. o
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // p .. ~
];
const SPACE_UNITS: u32 = 278;
/// The advance of every character of the Korean font that is not ASCII.
const CID_WIDE_UNITS: u32 = 1000;

/// The Latin font's width of an ASCII character.
pub(crate) fn ascii_units(c: char) -> u32 {
    latin_units(c)
}

fn latin_units(c: char) -> u32 {
    match c {
        ' '..='~' => u32::from(HELVETICA_WIDTHS[c as usize - 0x20]),
        _ => SPACE_UNITS,
    }
}

fn cid_units(c: char) -> u32 {
    if c.is_ascii() {
        latin_units(c)
    } else {
        CID_WIDE_UNITS
    }
}

/// The `/W` entry of the Korean font: the ASCII CIDs (1..=95, in code order) take the Helvetica widths, so a space between
/// Korean words is a Latin space and not half an em.
pub(crate) fn cid_ascii_widths() -> String {
    let widths: Vec<String> = HELVETICA_WIDTHS.iter().map(u16::to_string).collect();
    format!("[1 [{}]]", widths.join(" "))
}

/// Width of `text` as the writer will show it (one font for the whole string), in thousandths of
/// the font size. Trailing spaces are not counted: they hang past the right edge.
pub(crate) fn text_units(text: &str) -> u32 {
    let text = text.trim_end_matches(' ');
    if text.is_ascii() {
        text.chars().map(latin_units).sum()
    } else {
        text.chars().map(cid_units).sum()
    }
}

/// Splits `line` into lines no wider than `max_units` (thousandths of the font size).
#[cfg(test)]
fn wrap_line(line: &str, max_units: u32) -> Vec<String> {
    wrap_spans(line, max_units)
        .into_iter()
        .map(|span| line[span].to_string())
        .collect()
}

/// [`wrap_line`] as byte ranges of `line`: each range is the text of one output line (spaces at
/// a break are left out, a line that fits whole keeps its trailing spaces). Ranges are in order
/// and do not overlap, so a caller can map a position of `line` to the output line holding it.
pub(crate) fn wrap_spans(line: &str, max_units: u32) -> Vec<Range<usize>> {
    if text_units(line) <= max_units {
        return std::iter::once(0..line.len()).collect();
    }
    let trimmed = |start: usize, end: usize| start..start + line[start..end].trim_end().len();
    let mut out: Vec<Range<usize>> = Vec::new();
    // The line being built is `line[start..end]`; `off` is where the next word begins.
    let (mut start, mut end, mut off) = (0usize, 0usize, 0usize);
    for word in words_with_trailing_spaces(line) {
        let word_end = off + word.len();
        let word_off = off;
        off = word_end;
        if text_units(&line[start..word_end]) <= max_units {
            end = word_end;
            continue;
        }
        if !line[start..end].trim_end().is_empty() {
            out.push(trimmed(start, end));
            (start, end) = (word_off, word_off);
            if text_units(word) <= max_units {
                end = word_end;
                continue;
            }
        }
        for (i, ch) in word.char_indices() {
            let at = word_off + i;
            let ch_end = at + ch.len_utf8();
            if !line[start..end].trim_end().is_empty()
                && text_units(&line[start..ch_end]) > max_units
            {
                out.push(trimmed(start, end));
                start = at;
            }
            end = ch_end;
        }
    }
    if !line[start..end].trim_end().is_empty() || out.is_empty() {
        out.push(trimmed(start, end));
    }
    out
}

/// Distance in thousandths of the font size from the start of `rendered` (one output line, shown
/// with one font) to the character at `byte_offset`.
pub(crate) fn units_before(rendered: &str, byte_offset: usize) -> u32 {
    let head = &rendered[..byte_offset];
    if rendered.is_ascii() {
        head.chars().map(latin_units).sum()
    } else {
        head.chars().map(cid_units).sum()
    }
}

/// Each run of non-space characters with the spaces after it; leading spaces form the first item.
fn words_with_trailing_spaces(line: &str) -> impl Iterator<Item = &str> {
    let mut rest = line;
    let spaces_len = |s: &str| s.len() - s.trim_start_matches(' ').len();
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = if rest.starts_with(' ') {
            spaces_len(rest)
        } else {
            let word = rest.find(' ').unwrap_or(rest.len());
            word + spaces_len(&rest[word..])
        };
        let (item, tail) = rest.split_at(end);
        rest = tail;
        Some(item)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helvetica_widths_match_the_published_metrics() {
        assert_eq!(latin_units(' '), 278);
        assert_eq!(latin_units('W'), 944);
        assert_eq!(latin_units('i'), 222);
        assert_eq!(latin_units('~'), 584);
        assert_eq!(text_units("Hello"), 722 + 556 + 222 + 222 + 556);
    }

    #[test]
    fn korean_lines_use_the_declared_cid_widths() {
        assert_eq!(text_units("한글 ab"), 1000 + 1000 + 278 + 556 + 556);
        assert_eq!(text_units("한글   "), 2000, "trailing spaces hang");
    }

    #[test]
    fn words_keep_their_trailing_spaces_and_leading_spaces_are_an_item() {
        let items: Vec<&str> = words_with_trailing_spaces("  가나  다 라").collect();
        assert_eq!(items, ["  ", "가나  ", "다 ", "라"]);
        assert_eq!(words_with_trailing_spaces("").count(), 0);
    }

    #[test]
    fn a_fitting_line_is_returned_as_is_and_a_long_one_breaks_after_spaces() {
        assert_eq!(wrap_line("짧은 줄", 10_000), ["짧은 줄"]);
        assert_eq!(wrap_line("", 10_000), [""]);
        assert_eq!(wrap_line("가나 다라 마바", 4_500), ["가나 다라", "마바"]);
    }

    #[test]
    fn an_over_wide_word_is_split_between_characters() {
        assert_eq!(wrap_line("가나다라마", 2_000), ["가나", "다라", "마"]);
        assert_eq!(
            wrap_line("앞 가나다라마 뒤", 2_000),
            ["앞", "가나", "다라", "마", "뒤"]
        );
    }

    #[test]
    fn indentation_survives_on_the_first_line_only() {
        assert_eq!(wrap_line("  가나 다라", 3_000), ["  가나", "다라"]);
    }

    #[test]
    fn a_hostile_single_line_is_broken_in_linear_time() {
        let started = std::time::Instant::now();
        let max = 40_583;
        let ascii = "x".repeat(1_000_000);
        let lines = wrap_line(&ascii, max);
        assert_eq!(lines.len(), 1_000_000usize.div_ceil(81));
        assert_eq!(lines.concat(), ascii);
        let mixed = "한a".repeat(250_000);
        let lines = wrap_line(&mixed, max);
        assert!(lines.iter().all(|l| text_units(l) <= max));
        assert_eq!(lines.concat(), mixed);
        let spaced = "가 ".repeat(250_000);
        assert!(wrap_line(&spaced, max).iter().all(|l| text_units(l) <= max));
        assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
    }

    #[test]
    fn spans_are_the_text_of_each_output_line_in_order() {
        let line = "앞 가나다라마 뒤  끝";
        for max in [2_000, 3_000, 4_500] {
            let spans = wrap_spans(line, max);
            let texts: Vec<&str> = spans.iter().map(|s| &line[s.clone()]).collect();
            assert_eq!(texts, wrap_line(line, max));
            assert!(
                spans.windows(2).all(|w| w[0].end <= w[1].start),
                "{spans:?}"
            );
        }
        assert_eq!(wrap_spans("", 1_000).len(), 1);
        assert_eq!(wrap_spans("", 1_000)[0], 0..0);
    }

    #[test]
    fn units_before_uses_the_font_of_the_whole_line() {
        assert_eq!(units_before("Hello", 2), 722 + 556);
        assert_eq!(units_before("Hello 한", 2), 500 + 500);
        assert_eq!(units_before("한글", "한".len()), 1000);
    }
}
