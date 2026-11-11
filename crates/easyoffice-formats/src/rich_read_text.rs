//! Markdown and plain-text readers producing [`RichBlock`]s.
//!
//! Reference implementation: `pulldown-cmark` (MIT License, Copyright 2015 Google Inc.) and
//! CommonMark / GFM / Pandoc inline & block specifications: ATX and Setext headings,
//! bullet/ordered/task list items, pipe tables, nested block quotes, fenced code, inline
//! `**`/`__`, `*`/`_`, `~~`, `==`, `^sup^`, `~sub~`, inline HTML (`<u>`, `<sup>`, `<sub>`,
//! `<mark>`, `<b>`/`<strong>`, `<i>`/`<em>`, `<s>`/`<del>`, `<br>`), links, inline code, and
//! backslash escapes.

use crate::rich_xml::{push_run, CharFmt};
use crate::{
    CommonMarkCodec, EasyOfficeError, RichBlock, RichListKind, RichParagraph, RichRun, RichScript,
};

const MAX_MARKDOWN_BLOCKS: usize = 1_000_000;

fn is_ws(c: Option<char>) -> bool {
    c.is_none_or(char::is_whitespace)
}

/// Zero-allocation CommonMark thematic break check (`---`, `***`, `___` with optional spaces).
#[inline]
fn is_thematic_break(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }
    let mut marker: u8 = 0;
    let mut count: usize = 0;
    for &b in trimmed.as_bytes() {
        if b == b' ' || b == b'\t' {
            continue;
        }
        if !matches!(b, b'-' | b'*' | b'_') {
            return false;
        }
        if marker == 0 {
            marker = b;
        } else if marker != b {
            return false;
        }
        count += 1;
    }
    count >= 3
}

#[inline]
fn setext_heading_level(trimmed: &str) -> Option<u8> {
    if trimmed.len() < 2 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    if bytes.iter().all(|&b| b == b'=') {
        Some(1)
    } else if bytes.iter().all(|&b| b == b'-') {
        Some(2)
    } else {
        None
    }
}

fn blockquote_depth_and_body(trimmed: &str) -> Option<(usize, &str)> {
    if !trimmed.starts_with('>') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    let mut depth = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'>' {
            depth += 1;
            i += 1;
        } else if bytes[i] == b' ' || bytes[i] == b'\t' {
            i += 1;
        } else {
            break;
        }
    }
    Some((depth, trimmed[i..].trim()))
}

/// Inline Markdown scanner state.
struct MdInline {
    chars: Vec<char>,
    runs: Vec<RichRun>,
    fmt: CharFmt,
    buf: String,
    i: usize,
    no_more_single_tilde: bool,
}

impl MdInline {
    #[inline]
    fn starts(&self, at: usize, pat: &str) -> bool {
        let pb = pat.as_bytes();
        if at + pb.len() > self.chars.len() {
            return false;
        }
        let slice = &self.chars[at..at + pb.len()];
        let mut k = 0;
        while k < pb.len() {
            if slice[k] != (pb[k] as char) {
                return false;
            }
            k += 1;
        }
        true
    }

    fn has_closing_single_tilde(&mut self, from: usize) -> bool {
        if self.no_more_single_tilde {
            return false;
        }
        let mut k = from;
        while k < self.chars.len() {
            if self.chars[k] == '~' {
                if self.at(k + 1) == Some('~') {
                    while k < self.chars.len() && self.chars[k] == '~' {
                        k += 1;
                    }
                    continue;
                }
                if !is_ws(self.at(k - 1)) {
                    return true;
                }
                self.no_more_single_tilde = true;
                return false;
            }
            k += 1;
        }
        self.no_more_single_tilde = true;
        false
    }

    fn flush(&mut self) {
        push_run(&mut self.runs, &self.buf, &self.fmt);
        self.buf.clear();
    }

    fn at(&self, idx: usize) -> Option<char> {
        self.chars.get(idx).copied()
    }

    fn step_html_tag(&mut self, i: usize) -> bool {
        if self.starts(i, "<u>") {
            self.toggle_underline(true, 3);
        } else if self.starts(i, "</u>") {
            self.toggle_underline(false, 4);
        } else if self.starts(i, "<sup>") {
            self.set_script(RichScript::Superscript, 5);
        } else if self.starts(i, "</sup>") {
            self.set_script(RichScript::Normal, 6);
        } else if self.starts(i, "<sub>") {
            self.set_script(RichScript::Subscript, 5);
        } else if self.starts(i, "</sub>") {
            self.set_script(RichScript::Normal, 6);
        } else if self.starts(i, "<mark>") {
            self.set_highlight(Some("FFFF00".to_string()), 6);
        } else if self.starts(i, "</mark>") {
            self.set_highlight(None, 7);
        } else if self.starts(i, "<b>") {
            self.toggle_bold(true, 3);
        } else if self.starts(i, "</b>") {
            self.toggle_bold(false, 4);
        } else if self.starts(i, "<strong>") {
            self.toggle_bold(true, 8);
        } else if self.starts(i, "</strong>") {
            self.toggle_bold(false, 9);
        } else if self.starts(i, "<i>") {
            self.toggle_italic(true, 3);
        } else if self.starts(i, "</i>") {
            self.toggle_italic(false, 4);
        } else if self.starts(i, "<em>") {
            self.toggle_italic(true, 4);
        } else if self.starts(i, "</em>") {
            self.toggle_italic(false, 5);
        } else if self.starts(i, "<s>") {
            self.toggle_strike(true, 3);
        } else if self.starts(i, "</s>") {
            self.toggle_strike(false, 4);
        } else if self.starts(i, "<del>") {
            self.toggle_strike(true, 5);
        } else if self.starts(i, "</del>") {
            self.toggle_strike(false, 6);
        } else if self.starts(i, "<br />") {
            self.buf.push('\n');
            self.i += 6;
        } else if self.starts(i, "<br/>") {
            self.buf.push('\n');
            self.i += 5;
        } else if self.starts(i, "<br>") {
            self.buf.push('\n');
            self.i += 4;
        } else {
            return false;
        }
        true
    }

    fn step(&mut self) {
        let i = self.i;
        let c = self.chars[i];
        let next = self.at(i + 1);
        let prev = if i > 0 { self.at(i - 1) } else { None };
        match c {
            '\\' => self.escape(next),
            '\n' => self.soft_break(),
            '<' if self.step_html_tag(i) => {}
            '*' | '_' if next == Some(c) => {
                let can_open = !self.fmt.bold && !is_ws(self.at(i + 2));
                let can_close = self.fmt.bold && !is_ws(prev);
                if can_open || can_close {
                    self.flush();
                    self.fmt.bold = !self.fmt.bold;
                } else {
                    self.buf.push(c);
                    self.buf.push(c);
                }
                self.i += 2;
            }
            '~' if next == Some('~') => {
                let can_open = !self.fmt.strike && !is_ws(self.at(i + 2));
                let can_close = self.fmt.strike && !is_ws(prev);
                if can_open || can_close {
                    self.flush();
                    self.fmt.strike = !self.fmt.strike;
                } else {
                    self.buf.push_str("~~");
                }
                self.i += 2;
            }
            '~' => {
                let active = self.fmt.script == RichScript::Subscript;
                let can_open = !active && !is_ws(next) && self.has_closing_single_tilde(i + 1);
                let can_close = active && !is_ws(prev);
                if can_open || can_close {
                    self.flush();
                    self.fmt.script = if active {
                        RichScript::Normal
                    } else {
                        RichScript::Subscript
                    };
                } else {
                    self.buf.push('~');
                }
                self.i += 1;
            }
            '=' if next == Some('=') => {
                let active = self.fmt.highlight.is_some();
                let can_open = !active && !is_ws(self.at(i + 2));
                let can_close = active && !is_ws(prev);
                if can_open || can_close {
                    self.flush();
                    self.fmt.highlight = if active {
                        None
                    } else {
                        Some("FFFF00".to_string())
                    };
                } else {
                    self.buf.push_str("==");
                }
                self.i += 2;
            }
            '^' => {
                let active = self.fmt.script == RichScript::Superscript;
                let can_open = !active && !is_ws(next) && self.chars[i + 1..].contains(&'^');
                let can_close = active && !is_ws(prev);
                if can_open || can_close {
                    self.flush();
                    self.fmt.script = if active {
                        RichScript::Normal
                    } else {
                        RichScript::Superscript
                    };
                } else {
                    self.buf.push('^');
                }
                self.i += 1;
            }
            '*' | '_' => self.emphasis(c, prev, next),
            '`' => self.code_span(),
            '!' | '[' => self.link_or_image(c, next),
            _ => {
                self.buf.push(c);
                self.i += 1;
            }
        }
    }

    fn escape(&mut self, next: Option<char>) {
        match next {
            Some('\n') => {
                self.buf.push('\n');
                self.i += 2;
            }
            Some(n) if n.is_ascii_punctuation() => {
                self.buf.push(n);
                self.i += 2;
            }
            _ => {
                self.buf.push('\\');
                self.i += 1;
            }
        }
    }

    fn soft_break(&mut self) {
        if self.buf.ends_with("  ") {
            let keep = self.buf.trim_end_matches(' ').len();
            self.buf.truncate(keep);
            self.buf.push('\n');
        } else {
            self.buf.push(' ');
        }
        self.i += 1;
    }

    fn toggle_bold(&mut self, on: bool, len: usize) {
        self.flush();
        self.fmt.bold = on;
        self.i += len;
    }

    fn toggle_italic(&mut self, on: bool, len: usize) {
        self.flush();
        self.fmt.italic = on;
        self.i += len;
    }

    fn toggle_underline(&mut self, on: bool, len: usize) {
        self.flush();
        self.fmt.underline = on;
        self.i += len;
    }

    fn toggle_strike(&mut self, on: bool, len: usize) {
        self.flush();
        self.fmt.strike = on;
        self.i += len;
    }

    fn set_highlight(&mut self, hl: Option<String>, len: usize) {
        self.flush();
        self.fmt.highlight = hl;
        self.i += len;
    }

    fn set_script(&mut self, script: RichScript, len: usize) {
        self.flush();
        self.fmt.script = script;
        self.i += len;
    }

    fn emphasis(&mut self, c: char, prev: Option<char>, next: Option<char>) {
        let open = !self.fmt.italic
            && !is_ws(next)
            && (c == '*' || prev.is_none_or(|p| !p.is_alphanumeric()));
        let close = self.fmt.italic
            && !is_ws(prev)
            && (c == '*' || next.is_none_or(|n| !n.is_alphanumeric()));
        if open || close {
            self.flush();
            self.fmt.italic = !self.fmt.italic;
        } else {
            self.buf.push(c);
        }
        self.i += 1;
    }

    fn code_span(&mut self) {
        let i = self.i;
        if let Some(end) = self.chars[i + 1..].iter().position(|&x| x == '`') {
            self.buf.extend(&self.chars[i + 1..i + 1 + end]);
            self.i += end + 2;
        } else {
            self.buf.push('`');
            self.i += 1;
        }
    }

    /// Replaces `[text](url)` / `![alt](url)` with its label; otherwise keeps the character.
    fn link_or_image(&mut self, c: char, next: Option<char>) {
        let i = self.i;
        let bracket = match (c, next) {
            ('!', Some('[')) => Some(i + 1),
            ('[', _) => Some(i),
            _ => None,
        };
        if let Some((label_range, after)) = bracket.and_then(|b| self.link_span(b)) {
            self.buf.extend(&self.chars[label_range]);
            self.i = after;
        } else {
            self.buf.push(c);
            self.i += 1;
        }
    }

    /// Label range and the index after `)` for a link whose `[` is at `bracket`.
    fn link_span(&self, bracket: usize) -> Option<(std::ops::Range<usize>, usize)> {
        let close = bracket + 1 + self.chars[bracket + 1..].iter().position(|&x| x == ']')?;
        if self.chars.get(close + 1) != Some(&'(') {
            return None;
        }
        let end = self.chars[close + 2..].iter().position(|&x| x == ')')?;
        Some((bracket + 1..close, close + 2 + end + 1))
    }
}

/// Parses inline markers into formatted runs.
pub(crate) fn parse_md_inline(text: &str) -> Vec<RichRun> {
    if !text.bytes().any(|b| {
        matches!(
            b,
            b'\\' | b'\n' | b'<' | b'*' | b'~' | b'_' | b'`' | b'!' | b'[' | b'^' | b'='
        )
    }) {
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![RichRun::plain(text)]
        };
    }
    let mut scanner = MdInline {
        chars: text.chars().collect(),
        runs: Vec::new(),
        fmt: CharFmt::default(),
        buf: String::new(),
        i: 0,
        no_more_single_tilde: false,
    };
    while scanner.i < scanner.chars.len() {
        scanner.step();
    }
    scanner.flush();
    scanner.runs
}

/// A table cell holds text only: the inline syntax is resolved like a paragraph's and the
/// formatting it set (bold, italic, code, link, script) is dropped, the text stays.
fn inline_plain_text(cell: &str) -> String {
    parse_md_inline(cell).into_iter().map(|r| r.text).collect()
}

fn paragraph(text: &str, heading: u8, list: Option<RichListKind>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: parse_md_inline(text),
        alignment: None,
        heading_level: heading,
        list,
        ..RichParagraph::default()
    })
}

fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let core = c.trim().trim_matches(':');
            core.len() >= 3 && core.chars().all(|ch| ch == '-')
        })
}

fn parse_gfm_table_row_line(t: &str) -> Option<Vec<String>> {
    if t.len() >= 2 && t.starts_with('|') && t.ends_with('|') {
        return Some(CommonMarkCodec::parse_markdown_table_row(
            &t[1..t.len() - 1],
        ));
    }
    if t.contains('|') {
        let inner = t.strip_prefix('|').unwrap_or(t);
        let inner = inner.strip_suffix('|').unwrap_or(inner);
        let cells = CommonMarkCodec::parse_markdown_table_row(inner);
        if cells.len() >= 2 {
            return Some(cells);
        }
    }
    None
}

fn list_marker(trimmed: &str) -> Option<(RichListKind, std::borrow::Cow<'_, str>)> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            let rest = rest.trim();
            if let Some(task) = rest.strip_prefix("[ ] ") {
                return Some((
                    RichListKind::Bullet,
                    format!("\u{2610} {}", task.trim()).into(),
                ));
            }
            if let Some(task) = rest
                .strip_prefix("[x] ")
                .or_else(|| rest.strip_prefix("[X] "))
            {
                return Some((
                    RichListKind::Bullet,
                    format!("\u{2611} {}", task.trim()).into(),
                ));
            }
            return Some((RichListKind::Bullet, rest.into()));
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let rest = &trimmed[digits..];
        for marker in [". ", ") "] {
            if let Some(body) = rest.strip_prefix(marker) {
                return Some((RichListKind::Ordered, body.trim().into()));
            }
        }
    }
    None
}

/// Reads Markdown text into ordered blocks.
pub(crate) fn read_markdown(text: &str) -> Result<Vec<RichBlock>, EasyOfficeError> {
    if text.as_bytes().contains(&0) {
        return Err(EasyOfficeError::InvalidUtf8(
            "Markdown 스트림에 NUL 바이트가 포함되어 있습니다.".to_string(),
        ));
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks: Vec<RichBlock> = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    let flush = |blocks: &mut Vec<RichBlock>, pending: &mut Vec<&str>| {
        if !pending.is_empty() {
            blocks.push(paragraph(&pending.join("\n"), 0, None));
            pending.clear();
        }
    };
    let mut i = 0usize;
    while i < lines.len() {
        if blocks.len() > MAX_MARKDOWN_BLOCKS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Markdown 블록 수가 허용 예산을 초과했습니다.".to_string(),
            ));
        }
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut blocks, &mut pending);
            i += 1;
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush(&mut blocks, &mut pending);
            i += 1;
            while i < lines.len()
                && !lines[i].trim().starts_with("```")
                && !lines[i].trim().starts_with("~~~")
            {
                if !lines[i].trim().is_empty() {
                    blocks.push(RichBlock::Paragraph(RichParagraph {
                        runs: vec![RichRun::plain(lines[i].trim_end())],
                        ..RichParagraph::default()
                    }));
                    if blocks.len() > MAX_MARKDOWN_BLOCKS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "Markdown 블록 수가 허용 예산을 초과했습니다.".to_string(),
                        ));
                    }
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut blocks, &mut pending);
            let body = trimmed[hashes..].trim().trim_end_matches('#').trim_end();
            blocks.push(paragraph(body, hashes as u8, None));
            i += 1;
            continue;
        }
        if !pending.is_empty() {
            if let Some(level) = setext_heading_level(trimmed) {
                blocks.push(paragraph(&pending.join("\n"), level, None));
                pending.clear();
                i += 1;
                continue;
            }
        }
        if is_thematic_break(trimmed) {
            flush(&mut blocks, &mut pending);
            i += 1;
            continue;
        }
        let is_outer_pipe =
            trimmed.len() >= 2 && trimmed.starts_with('|') && trimmed.ends_with('|');
        let is_pipeless_table = !is_outer_pipe
            && trimmed.contains('|')
            && lines
                .get(i + 1)
                .and_then(|next| parse_gfm_table_row_line(next.trim()))
                .is_some_and(|sep| sep.len() >= 2 && is_separator_row(&sep));
        if is_outer_pipe || is_pipeless_table {
            flush(&mut blocks, &mut pending);
            let mut rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() {
                let t = lines[i].trim();
                let Some(cells) = (if is_outer_pipe {
                    (t.len() >= 2 && t.starts_with('|') && t.ends_with('|'))
                        .then(|| CommonMarkCodec::parse_markdown_table_row(&t[1..t.len() - 1]))
                } else {
                    parse_gfm_table_row_line(t)
                }) else {
                    break;
                };
                if !(rows.len() == 1 && is_separator_row(&cells)) {
                    rows.push(cells.iter().map(|c| inline_plain_text(c)).collect());
                    if rows.len() > MAX_MARKDOWN_BLOCKS {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "Markdown 블록 수가 허용 예산을 초과했습니다.".to_string(),
                        ));
                    }
                }
                i += 1;
            }
            blocks.push(RichBlock::Table(rows.into()));
            continue;
        }
        if let Some((kind, body)) = list_marker(trimmed) {
            flush(&mut blocks, &mut pending);
            blocks.push(paragraph(&body, 0, Some(kind)));
            i += 1;
            continue;
        }
        if let Some((depth, quote_body)) = blockquote_depth_and_body(trimmed) {
            flush(&mut blocks, &mut pending);
            blocks.push(RichBlock::Paragraph(RichParagraph {
                runs: parse_md_inline(quote_body),
                left_indent_twips: Some((depth.min(6) as u32) * 720),
                ..RichParagraph::default()
            }));
            i += 1;
            continue;
        }
        pending.push(line.trim_end_matches('\r'));
        i += 1;
    }
    flush(&mut blocks, &mut pending);
    Ok(blocks)
}

/// Reads plain text: every non-blank line is one paragraph.
pub(crate) fn read_plain_text(text: &str) -> Result<Vec<RichBlock>, EasyOfficeError> {
    let mut blocks = Vec::new();
    for l in text.strip_prefix('\u{FEFF}').unwrap_or(text).lines() {
        if !l.trim().is_empty() {
            blocks.push(RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(l.trim_end())],
                ..RichParagraph::default()
            }));
            if blocks.len() > MAX_MARKDOWN_BLOCKS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "텍스트 블록 수가 허용 예산을 초과했습니다.".to_string(),
                ));
            }
        }
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_markers_toggle_formats_and_escapes_stay_literal() {
        let runs = parse_md_inline("a **b** \\*c\\* `d` [e](http://x) ![f](y)");
        let text: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text, "a b *c* d e f");
        assert!(runs.iter().any(|r| r.bold && r.text == "b"));
    }

    #[test]
    fn code_fence_lines_become_plain_paragraphs() {
        let blocks = read_markdown("```\nlet x = 1;\n```\n").unwrap();
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn inline_fast_path_sup_sub_highlight_and_gfm_task_lists() {
        assert_eq!(
            parse_md_inline("평문 텍스트 Fast Path 123"),
            vec![RichRun::plain("평문 텍스트 Fast Path 123")]
        );
        let runs = parse_md_inline("H<sub>2</sub>O X<sup>3</sup> Y^4^ ==강조==");
        assert!(runs
            .iter()
            .any(|r| r.text == "2" && r.script == RichScript::Subscript));
        assert!(runs
            .iter()
            .any(|r| r.text == "3" && r.script == RichScript::Superscript));
        assert!(runs
            .iter()
            .any(|r| r.text == "4" && r.script == RichScript::Superscript));
        assert!(runs
            .iter()
            .any(|r| r.text == "강조" && r.highlight.as_deref() == Some("FFFF00")));

        let blocks = read_markdown("- [ ] 미완료\n- [x] 완료1\n- [X] 완료2\n").unwrap();
        assert_eq!(blocks.len(), 3);
        let texts: Vec<String> = blocks
            .iter()
            .map(|b| match b {
                RichBlock::Paragraph(p) => {
                    assert_eq!(p.list, Some(RichListKind::Bullet));
                    p.plain_text()
                }
                _ => panic!("expected paragraph"),
            })
            .collect();
        assert_eq!(texts, vec!["☐ 미완료", "☑ 완료1", "☑ 완료2"]);
    }

    #[test]
    fn gfm_double_underscore_bold_code_span_blockquote_and_pipeless_table() {
        let runs = parse_md_inline("__굵게__ `*안바뀜*` _기울임_");
        assert!(runs.iter().any(|r| r.bold && r.text == "굵게"));
        assert!(runs
            .iter()
            .any(|r| !r.italic && r.text.contains("*안바뀜*")));
        assert!(runs.iter().any(|r| r.italic && r.text == "기울임"));

        let md = "> 인용 문단\n\n항목 | 값\n--- | ---\nA | 10\nB | 20\n";
        let blocks = read_markdown(md).unwrap();
        assert_eq!(blocks.len(), 2);
        let RichBlock::Paragraph(quote_p) = &blocks[0] else {
            panic!("expected blockquote paragraph");
        };
        assert_eq!(quote_p.left_indent_twips, Some(720));
        assert_eq!(quote_p.plain_text(), "인용 문단");

        let RichBlock::Table(tbl) = &blocks[1] else {
            panic!("expected pipeless GFM table");
        };
        assert_eq!(tbl.rows.len(), 3);
        assert_eq!(tbl.rows[0][0], "항목");
        assert_eq!(tbl.rows[2][1], "20");
    }

    #[test]
    fn setext_headings_pandoc_subscript_inline_html_and_nested_blockquotes() {
        let runs = parse_md_inline(
            "H~2~O <mark>형광</mark> <b>굵음1</b> <strong>굵음2</strong> <i>기울임1</i> <em>기울임2</em> <s>취소1</s> <del>취소2</del> 줄1<br/>줄2",
        );
        assert!(runs
            .iter()
            .any(|r| r.text == "2" && r.script == RichScript::Subscript));
        assert!(runs
            .iter()
            .any(|r| r.text == "형광" && r.highlight.as_deref() == Some("FFFF00")));
        assert!(runs.iter().any(|r| r.text == "굵음1" && r.bold));
        assert!(runs.iter().any(|r| r.text == "굵음2" && r.bold));
        assert!(runs.iter().any(|r| r.text == "기울임1" && r.italic));
        assert!(runs.iter().any(|r| r.text == "기울임2" && r.italic));
        assert!(runs.iter().any(|r| r.text == "취소1" && r.strike));
        assert!(runs.iter().any(|r| r.text == "취소2" && r.strike));
        assert!(runs.iter().any(|r| r.text.contains("줄1\n줄2")));

        let md = "대제목\n===\n\n중제목\n---\n\n---\n\n>> 2단 인용\n> > > 3단 인용\n";
        let blocks = read_markdown(md).unwrap();
        assert_eq!(blocks.len(), 4);
        let RichBlock::Paragraph(h1) = &blocks[0] else {
            panic!("expected h1");
        };
        assert_eq!(h1.heading_level, 1);
        assert_eq!(h1.plain_text(), "대제목");
        let RichBlock::Paragraph(h2) = &blocks[1] else {
            panic!("expected h2");
        };
        assert_eq!(h2.heading_level, 2);
        assert_eq!(h2.plain_text(), "중제목");
        let RichBlock::Paragraph(q2) = &blocks[2] else {
            panic!("expected q2");
        };
        assert_eq!(q2.left_indent_twips, Some(1440));
        assert_eq!(q2.plain_text(), "2단 인용");
        let RichBlock::Paragraph(q3) = &blocks[3] else {
            panic!("expected q3");
        };
        assert_eq!(q3.left_indent_twips, Some(2160));
        assert_eq!(q3.plain_text(), "3단 인용");
    }
}
