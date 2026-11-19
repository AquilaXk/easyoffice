//! Markdown writer for [`RichDocument`], preserving headings, nested bullet/ordered lists with
//! delimiter punctuation, inline character runs (bold, italic, underline, strike, highlight,
//! superscript/subscript, hyperlinks), and pipe tables (including cell hyperlinks).

use crate::rich_document::{
    safe_link_target, RichBlock, RichDocument, RichListKind, RichNumberEnclosure, RichParagraph,
    RichScript,
};
use crate::rich_notes::{collect_note_ordinals, is_deleted_run, RichNoteKind};
use crate::rich_table_writers::markdown_table_rows;
use crate::{CommonMarkCodec, MarkdownBlock};

fn escape_markdown_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut prev = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let doubled_equals = ch == '=' && (prev == Some('=') || chars.peek() == Some(&'='));
        if doubled_equals
            || matches!(
                ch,
                '\\' | '`' | '*' | '_' | '~' | '^' | '[' | ']' | '<' | '>' | '|' | '#' | '!'
            )
        {
            out.push('\\');
        }
        out.push(ch);
        prev = Some(ch);
    }
    out
}

/// Table cell text as Markdown: inline marks escaped, except `|`, which the table renderer
/// escapes itself (single pass without `.replace` allocations).
fn escape_markdown_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut prev = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let doubled_equals = ch == '=' && (prev == Some('=') || chars.peek() == Some(&'='));
        if (doubled_equals
            || matches!(
                ch,
                '\\' | '`' | '*' | '_' | '~' | '^' | '[' | ']' | '<' | '>' | '#' | '!'
            ))
            && ch != '|'
        {
            out.push('\\');
        }
        out.push(ch);
        prev = Some(ch);
    }
    out
}

/// Escapes characters that would start a block construct at the beginning of a line.
fn escape_markdown_line_start(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    if let Some(first) = trimmed.chars().next() {
        if matches!(first, '-' | '+' | '=') {
            return format!("{indent}\\{trimmed}");
        }
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 {
            if let Some(marker @ ('.' | ')')) = trimmed[digits..].chars().next() {
                return format!(
                    "{indent}{}\\{marker}{}",
                    &trimmed[..digits],
                    &trimmed[digits + 1..]
                );
            }
        }
    }
    line.to_string()
}

fn escape_link_destination(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for ch in url.chars() {
        match ch {
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            ' ' => out.push_str("%20"),
            _ => out.push(ch),
        }
    }
    out
}

fn render_markdown_runs(p: &RichParagraph) -> String {
    let mut out = String::new();
    for run in &p.runs {
        if is_deleted_run(run) {
            continue;
        }
        if let Some((kind, id)) = run.note_ref {
            let prefix = if kind == RichNoteKind::Footnote {
                "fn"
            } else {
                "en"
            };
            out.push_str(&format!("[^{prefix}{id}]"));
            continue;
        }
        let safe_url = run
            .link
            .as_deref()
            .and_then(safe_link_target)
            .map(escape_link_destination);
        for (l_idx, line) in run.text.split('\n').enumerate() {
            if l_idx > 0 {
                out.push_str("\\\n");
            }
            let core = line.trim();
            if core.is_empty() {
                out.push_str(line);
                continue;
            }
            let lead = &line[..line.len() - line.trim_start().len()];
            let trail = &line[line.trim_end().len()..];
            let mut piece = escape_markdown_inline(core);
            match run.script {
                RichScript::Normal => {}
                RichScript::Superscript => piece = format!("<sup>{piece}</sup>"),
                RichScript::Subscript => piece = format!("<sub>{piece}</sub>"),
            }
            if run.highlight.is_some() {
                piece = format!("=={piece}==");
            }
            if run.italic {
                piece = format!("*{piece}*");
            }
            if run.bold {
                piece = format!("**{piece}**");
            }
            if run.strike {
                piece = format!("~~{piece}~~");
            }
            if run.underline {
                piece = format!("<u>{piece}</u>");
            }
            if let Some(url) = &safe_url {
                piece = format!("[{piece}]({url})");
            }
            out.push_str(lead);
            out.push_str(&piece);
            out.push_str(trail);
        }
    }
    out.split('\n')
        .map(escape_markdown_line_start)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render_markdown_rich(doc: &RichDocument) -> String {
    let opens_with_h1 = matches!(
        doc.blocks.first(),
        Some(RichBlock::Paragraph(p)) if p.heading_level == 1
    );
    let mut parts: Vec<String> = if opens_with_h1 {
        Vec::new()
    } else {
        vec![format!(
            "# {}",
            escape_markdown_inline(&doc.display_title())
        )]
    };
    let mut list_buf: Vec<String> = Vec::new();
    let mut ordinals = [0usize; 9];
    let flush = |parts: &mut Vec<String>, list_buf: &mut Vec<String>| {
        if !list_buf.is_empty() {
            parts.push(list_buf.join("\n"));
            list_buf.clear();
        }
    };
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) if !p.runs.is_empty() && p.runs.iter().all(is_deleted_run) => {
                continue;
            }
            RichBlock::Paragraph(p) if (1..=6).contains(&p.heading_level) => {
                flush(&mut parts, &mut list_buf);
                ordinals = [0; 9];
                let hashes = "#".repeat(usize::from(p.heading_level));
                parts.push(format!("{hashes} {}", render_markdown_runs(p)));
            }
            RichBlock::Paragraph(p) if p.list.is_some() => {
                let lvl = usize::from(p.list_level).min(8);
                for deeper in &mut ordinals[lvl + 1..] {
                    *deeper = 0;
                }
                let indent = "  ".repeat(lvl);
                let marker = match p.list {
                    Some(RichListKind::Ordered) => {
                        ordinals[lvl] += 1;
                        let suffix = match p.list_number.map(|n| n.enclosure) {
                            Some(
                                RichNumberEnclosure::Parentheses
                                | RichNumberEnclosure::RightParenthesis,
                            ) => ")",
                            _ => ".",
                        };
                        format!("{}{suffix}", ordinals[lvl])
                    }
                    Some(RichListKind::Task) => {
                        ordinals[lvl] = 0;
                        if p.task_checked {
                            "- [x]".to_string()
                        } else {
                            "- [ ]".to_string()
                        }
                    }
                    _ => {
                        ordinals[lvl] = 0;
                        "-".to_string()
                    }
                };
                list_buf.push(format!("{indent}{marker} {}", render_markdown_runs(p)));
            }
            RichBlock::Paragraph(p) => {
                flush(&mut parts, &mut list_buf);
                ordinals = [0; 9];
                let text = render_markdown_runs(p);
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
            RichBlock::Image(img) => {
                flush(&mut parts, &mut list_buf);
                ordinals = [0; 9];
                parts.push(escape_markdown_line_start(&escape_markdown_inline(
                    img.fallback_text(),
                )));
            }
            RichBlock::Table(table) => {
                flush(&mut parts, &mut list_buf);
                ordinals = [0; 9];
                if table.rows.iter().any(|r| !r.is_empty()) {
                    let rows = markdown_table_rows(table, escape_markdown_cell);
                    parts.push(CommonMarkCodec::render(&[MarkdownBlock::Table(rows)]));
                }
            }
        }
    }
    flush(&mut parts, &mut list_buf);
    let (notes, _) = collect_note_ordinals(doc);
    if !notes.is_empty() {
        let mut defs = Vec::with_capacity(notes.len());
        for (note, _) in notes {
            let prefix = if note.kind == RichNoteKind::Footnote {
                "fn"
            } else {
                "en"
            };
            let mut lines = note.text.split('\n');
            let first = lines.next().unwrap_or("");
            let mut def = format!("[^{prefix}{}]: {first}", note.id);
            for cont in lines {
                def.push_str("\n    ");
                def.push_str(cont);
            }
            defs.push(def);
        }
        parts.push(defs.join("\n"));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_escapes_block_starters_and_inline_markers() {
        assert_eq!(escape_markdown_line_start("1. 항목"), "1\\. 항목");
        assert_eq!(escape_markdown_line_start("- 항목"), "\\- 항목");
        assert_eq!(escape_markdown_inline("*a*#"), "\\*a\\*\\#");
    }
}
