//! Markdown and plain-text readers producing [`RichBlock`]s. Markdown is read as the subset the
//! writer emits and common CommonMark documents use: ATX headings, bullet/ordered list items,
//! pipe tables, block quotes, fenced code (as plain paragraphs), and inline `**`, `*`/`_`, `~~`,
//! `<u>`, links, inline code, and backslash escapes. Nothing is ever interpreted as markup
//! beyond those markers.

use crate::rich_xml::{push_run, CharFmt};
use crate::{CommonMarkCodec, EasyOfficeError, RichBlock, RichListKind, RichParagraph, RichRun};

const MAX_MARKDOWN_BLOCKS: usize = 1_000_000;

fn is_ws(c: Option<char>) -> bool {
    c.is_none_or(char::is_whitespace)
}

/// Inline Markdown scanner state.
struct MdInline {
    chars: Vec<char>,
    runs: Vec<RichRun>,
    fmt: CharFmt,
    buf: String,
    i: usize,
}

impl MdInline {
    fn starts(&self, at: usize, pat: &str) -> bool {
        pat.chars()
            .enumerate()
            .all(|(k, p)| self.chars.get(at + k) == Some(&p))
    }

    fn flush(&mut self) {
        push_run(&mut self.runs, &self.buf, &self.fmt);
        self.buf.clear();
    }

    fn at(&self, idx: usize) -> Option<char> {
        self.chars.get(idx).copied()
    }

    fn step(&mut self) {
        let i = self.i;
        let c = self.chars[i];
        let next = self.at(i + 1);
        let prev = if i > 0 { self.at(i - 1) } else { None };
        match c {
            '\\' => self.escape(next),
            '\n' => self.soft_break(),
            '<' if self.starts(i, "<u>") => self.toggle_underline(true, 3),
            '<' if self.starts(i, "</u>") => self.toggle_underline(false, 4),
            '*' if next == Some('*') => {
                let can_open = !self.fmt.bold && !is_ws(self.at(i + 2));
                let can_close = self.fmt.bold && !is_ws(prev);
                if can_open || can_close {
                    self.flush();
                    self.fmt.bold = !self.fmt.bold;
                } else {
                    self.buf.push_str("**");
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

    fn toggle_underline(&mut self, on: bool, len: usize) {
        self.flush();
        self.fmt.underline = on;
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
    let mut scanner = MdInline {
        chars: text.chars().collect(),
        runs: Vec::new(),
        fmt: CharFmt::default(),
        buf: String::new(),
        i: 0,
    };
    while scanner.i < scanner.chars.len() {
        scanner.step();
    }
    scanner.flush();
    scanner.runs
}

fn paragraph(text: &str, heading: u8, list: Option<RichListKind>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: parse_md_inline(text),
        alignment: None,
        heading_level: heading,
        list,
    })
}

fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let core = c.trim().trim_matches(':');
            core.len() >= 3 && core.chars().all(|ch| ch == '-')
        })
}

fn list_marker(trimmed: &str) -> Option<(RichListKind, &str)> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some((RichListKind::Bullet, rest.trim()));
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits <= 9 {
        let rest = &trimmed[digits..];
        for marker in [". ", ") "] {
            if let Some(body) = rest.strip_prefix(marker) {
                return Some((RichListKind::Ordered, body.trim()));
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
        if trimmed.len() >= 3
            && trimmed.chars().all(|c| matches!(c, '-' | '*' | '_' | ' '))
            && trimmed.chars().filter(|c| *c != ' ').count() >= 3
            && trimmed
                .chars()
                .filter(|c| *c != ' ')
                .collect::<std::collections::HashSet<_>>()
                .len()
                == 1
        {
            flush(&mut blocks, &mut pending);
            i += 1;
            continue;
        }
        if trimmed.len() >= 2 && trimmed.starts_with('|') && trimmed.ends_with('|') {
            flush(&mut blocks, &mut pending);
            let mut rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() {
                let t = lines[i].trim();
                if !(t.len() >= 2 && t.starts_with('|') && t.ends_with('|')) {
                    break;
                }
                let inner = &t[1..t.len() - 1];
                let cells = CommonMarkCodec::parse_markdown_table_row(inner);
                if !(rows.len() == 1 && is_separator_row(&cells)) {
                    rows.push(cells);
                }
                i += 1;
            }
            blocks.push(RichBlock::Table(rows));
            continue;
        }
        if let Some((kind, body)) = list_marker(trimmed) {
            flush(&mut blocks, &mut pending);
            blocks.push(paragraph(body, 0, Some(kind)));
            i += 1;
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix('>') {
            flush(&mut blocks, &mut pending);
            blocks.push(paragraph(quote.trim(), 0, None));
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
pub(crate) fn read_plain_text(text: &str) -> Vec<RichBlock> {
    text.strip_prefix('\u{FEFF}')
        .unwrap_or(text)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(l.trim_end())],
                ..RichParagraph::default()
            })
        })
        .collect()
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
}
