//! RTF reader producing a [`RichDocument`]: runs with bold/italic/underline/strike, size
//! (`\fs`), font (`\f` through `\fonttbl`), color (`\cf`) and highlight (`\highlight`) through
//! `\colortbl`, paragraph alignment, outline levels, and `\trowd` table rows in position.
//! Unknown destinations (`\*`, style sheets, pictures, headers) are skipped.

use std::collections::HashMap;

use crate::rich_document::normalize_hex_color;
use crate::rich_rtf_table::{merges_from_marks, CellMarks, HMark, VMark};
use crate::rich_table::RichTable;
use crate::rich_writers::heading_half_points;
use crate::rich_xml::{push_run, CharFmt};
use crate::{
    EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichParagraph, RichRun, RichScript,
};

const MAX_GROUP_DEPTH: usize = 256;
const DEFAULT_TITLE: &str = "RTF 문서";
/// RTF 1.9.1: `\uc1` is in force until a `\ucN` says otherwise.
const DEFAULT_UNICODE_SKIP: usize = 1;
const SKIPPED_DESTINATIONS: [&str; 15] = [
    "stylesheet",
    "pict",
    "header",
    "footer",
    "footnote",
    "fldinst",
    "listtable",
    "listoverridetable",
    "generator",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "rsidtbl",
    "xmlnstbl",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dest {
    Body,
    FontTbl,
    ColorTbl,
    Info,
    Title,
    Skip,
}

/// Character state of one RTF group (resolved against the tables when text is emitted).
#[derive(Clone, Default)]
struct CharState {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    size: Option<u16>,
    font: Option<i32>,
    color: Option<usize>,
    highlight: Option<usize>,
    script: RichScript,
}

#[derive(Clone)]
struct Group {
    dest: Dest,
    chars: CharState,
    first_word: bool,
    /// `\ucN`: how many fallback units follow each `\uN` (scoped to the group, 1 by default).
    unicode_skip: usize,
}

#[derive(Default)]
struct ParaProps {
    align: Option<RichAlignment>,
    heading: u8,
    list: Option<crate::RichListKind>,
    left_indent: Option<u32>,
    first_line: Option<i32>,
    sl: Option<i32>,
    slmult: bool,
}

struct Reader {
    stack: Vec<Group>,
    fonts: HashMap<i32, String>,
    colors: Vec<Option<String>>,
    default_font: i32,
    font_idx: i32,
    font_name: String,
    rgb: [Option<u8>; 3],
    title: String,
    runs: Vec<RichRun>,
    para: ParaProps,
    blocks: Vec<RichBlock>,
    in_row: bool,
    cell: String,
    row: Vec<String>,
    table_idx: usize,
    table: Option<OpenTable>,
    pending_marks: CellMarks,
    row_marks: Vec<CellMarks>,
    merges_dropped: usize,
    high_surrogate: Option<u16>,
}

/// Rows (text and merge marks) of the table being read, numbered by `\tblidx`.
struct OpenTable {
    idx: usize,
    rows: Vec<Vec<String>>,
    marks: Vec<Vec<CellMarks>>,
}

impl Reader {
    fn new() -> Self {
        Self {
            stack: vec![Group {
                dest: Dest::Body,
                chars: CharState::default(),
                first_word: false,
                unicode_skip: DEFAULT_UNICODE_SKIP,
            }],
            fonts: HashMap::new(),
            colors: Vec::new(),
            default_font: 0,
            font_idx: 0,
            font_name: String::new(),
            rgb: [None; 3],
            title: String::new(),
            runs: Vec::new(),
            para: ParaProps::default(),
            blocks: Vec::new(),
            in_row: false,
            cell: String::new(),
            row: Vec::new(),
            table_idx: 0,
            table: None,
            pending_marks: CellMarks::default(),
            row_marks: Vec::new(),
            merges_dropped: 0,
            high_surrogate: None,
        }
    }

    fn top(&mut self) -> &mut Group {
        let last = self.stack.len() - 1;
        &mut self.stack[last]
    }

    fn resolve(&self, st: &CharState) -> CharFmt {
        // Index 0 of the color table is "auto" and stored as `None`.
        let color_at = |i: Option<usize>| i.and_then(|i| self.colors.get(i)).and_then(Clone::clone);
        CharFmt {
            bold: st.bold,
            italic: st.italic,
            underline: st.underline,
            strike: st.strike,
            size_half_pt: st.size,
            font: st
                .font
                .filter(|f| *f != self.default_font)
                .and_then(|f| self.fonts.get(&f).cloned()),
            color: color_at(st.color),
            highlight: color_at(st.highlight),
            script: st.script,
        }
    }

    fn emit(&mut self, text: &str) {
        self.high_surrogate = None;
        match self.top().dest {
            Dest::Body => {
                if self.in_row {
                    // Table writers put a separator space between the row group and the first cell.
                    let first_cell = self.row.is_empty() && self.cell.is_empty();
                    self.cell
                        .push_str(if first_cell { text.trim_start() } else { text });
                } else {
                    let chars = self.top().chars.clone();
                    let fmt = self.resolve(&chars);
                    push_run(&mut self.runs, text, &fmt);
                }
            }
            Dest::FontTbl => {
                for ch in text.chars() {
                    if ch == ';' {
                        let name = self.font_name.trim().to_string();
                        if !name.is_empty() {
                            self.fonts.insert(self.font_idx, name);
                        }
                        self.font_name.clear();
                    } else {
                        self.font_name.push(ch);
                    }
                }
            }
            Dest::ColorTbl => {
                for _ in text.matches(';') {
                    let [r, g, b] = self.rgb;
                    let color = (r.is_some() || g.is_some() || b.is_some()).then(|| {
                        format!(
                            "{:02X}{:02X}{:02X}",
                            r.unwrap_or(0),
                            g.unwrap_or(0),
                            b.unwrap_or(0)
                        )
                    });
                    self.colors
                        .push(color.and_then(|c| normalize_hex_color(&c)));
                    self.rgb = [None; 3];
                }
            }
            Dest::Title => self.title.push_str(text),
            Dest::Info | Dest::Skip => {}
        }
    }

    fn flush_table(&mut self) {
        if let Some(open) = self.table.take() {
            if !open.rows.is_empty() {
                let merges = merges_from_marks(&open.marks);
                let (table, dropped) = RichTable::with_merges(open.rows, merges);
                self.merges_dropped += dropped;
                self.blocks.push(RichBlock::Table(table));
            }
        }
    }

    fn end_paragraph(&mut self) {
        let mut runs = std::mem::take(&mut self.runs);
        let para = std::mem::take(&mut self.para);
        if para.heading > 0 {
            let inherited = u16::try_from(heading_half_points(para.heading)).ok();
            if self.top().chars.size == inherited {
                self.top().chars.size = None;
            }
            let mut merged: Vec<RichRun> = Vec::new();
            for r in runs {
                let fmt = CharFmt {
                    bold: r.bold,
                    italic: r.italic,
                    underline: r.underline,
                    strike: r.strike,
                    size_half_pt: r.size_half_pt.filter(|s| Some(*s) != inherited),
                    font: r.font,
                    color: r.color,
                    highlight: r.highlight,
                    script: r.script,
                };
                push_run(&mut merged, &r.text, &fmt);
            }
            runs = merged;
        }
        if runs.iter().any(|r| !r.text.trim().is_empty()) {
            self.flush_table();
            let list = if para.heading > 0 { None } else { para.list };
            let left_indent_twips = if list.is_some() && para.left_indent == Some(720) {
                None
            } else {
                para.left_indent
            };
            let line_spacing_percent = para
                .sl
                .filter(|s| para.slmult && *s > 0)
                .and_then(|s| u16::try_from((i64::from(s) * 100 + 120) / 240).ok());
            self.blocks.push(RichBlock::Paragraph(RichParagraph {
                runs,
                alignment: para.align,
                heading_level: para.heading,
                list,
                left_indent_twips,
                first_line_indent_twips: para.first_line,
                line_spacing_percent,
            }));
        }
    }

    fn end_row(&mut self) {
        let row = std::mem::take(&mut self.row);
        let marks = std::mem::take(&mut self.row_marks);
        self.in_row = false;
        self.cell.clear();
        if self
            .table
            .as_ref()
            .is_some_and(|open| open.idx != self.table_idx)
        {
            self.flush_table();
        }
        match self.table.as_mut() {
            Some(open) => {
                open.rows.push(row);
                open.marks.push(marks);
            }
            None => {
                self.table = Some(OpenTable {
                    idx: self.table_idx,
                    rows: vec![row],
                    marks: vec![marks],
                })
            }
        }
    }

    fn emit_unicode(&mut self, n: i32) {
        let unit = (n & 0xFFFF) as u16;
        if (0xD800..0xDC00).contains(&unit) {
            self.high_surrogate = Some(unit);
            return;
        }
        let ch = match self.high_surrogate.take() {
            Some(high) if (0xDC00..0xE000).contains(&unit) => {
                char::decode_utf16([high, unit]).next().and_then(Result::ok)
            }
            _ => char::from_u32(u32::from(unit)),
        };
        let mut buf = [0u8; 4];
        self.emit(ch.unwrap_or('\u{FFFD}').encode_utf8(&mut buf));
    }

    fn control_word(&mut self, word: &str, param: Option<i32>) {
        if word == "tblidx" {
            self.table_idx = usize::try_from(param.unwrap_or(0)).unwrap_or(0);
            return;
        }
        if word == "uc" {
            self.top().unicode_skip = usize::try_from(param.unwrap_or(1)).unwrap_or(0);
            return;
        }
        if word == "u" && param.is_some() && self.top().dest != Dest::Skip {
            self.top().first_word = false;
            self.emit_unicode(param.unwrap_or(0));
            return;
        }
        let first = std::mem::replace(&mut self.top().first_word, false);
        if first {
            let dest = match word {
                "fonttbl" => Some(Dest::FontTbl),
                "colortbl" => Some(Dest::ColorTbl),
                "info" => Some(Dest::Info),
                "title" if self.top().dest == Dest::Info => Some(Dest::Title),
                w if SKIPPED_DESTINATIONS.contains(&w) => Some(Dest::Skip),
                _ => None,
            };
            if let Some(dest) = dest {
                self.top().dest = dest;
                return;
            }
        }
        if word == "pnlvlblt" {
            self.para.list = Some(crate::RichListKind::Bullet);
            return;
        }
        if matches!(word, "pnlvlbody" | "pnlvl") {
            self.para.list = Some(crate::RichListKind::Ordered);
            return;
        }
        match self.top().dest {
            Dest::FontTbl => {
                if word == "f" {
                    self.font_idx = param.unwrap_or(0);
                    self.font_name.clear();
                }
                return;
            }
            Dest::ColorTbl => {
                let value = param.and_then(|v| u8::try_from(v.clamp(0, 255)).ok());
                match word {
                    "red" => self.rgb[0] = value,
                    "green" => self.rgb[1] = value,
                    "blue" => self.rgb[2] = value,
                    _ => {}
                }
                return;
            }
            Dest::Skip | Dest::Info | Dest::Title => return,
            Dest::Body => {}
        }
        self.body_word(word, param);
    }

    fn body_word(&mut self, word: &str, param: Option<i32>) {
        let on = param != Some(0);
        match word {
            "par" if !self.in_row => self.end_paragraph(),
            "par" | "line" if self.in_row => {
                self.high_surrogate = None;
                if !self.cell.is_empty() {
                    self.cell.push('\n');
                }
            }
            "pard" => {
                self.para = ParaProps::default();
            }
            "line" => self.emit("\n"),
            "tab" => self.emit("\t"),
            "emdash" => self.emit("\u{2014}"),
            "endash" => self.emit("\u{2013}"),
            "bullet" => self.emit("\u{2022}"),
            "lquote" => self.emit("\u{2018}"),
            "rquote" => self.emit("\u{2019}"),
            "ldblquote" => self.emit("\u{201C}"),
            "rdblquote" => self.emit("\u{201D}"),
            "enspace" => self.emit("\u{2002}"),
            "emspace" => self.emit("\u{2003}"),
            "deff" => self.default_font = param.unwrap_or(0),
            "trowd" => {
                self.in_row = true;
                self.row.clear();
                self.row_marks.clear();
                self.pending_marks = CellMarks::default();
                self.cell.clear();
            }
            "clmgf" if self.in_row => self.pending_marks.h = HMark::First,
            "clmrg" if self.in_row => self.pending_marks.h = HMark::Merged,
            "clvmgf" if self.in_row => self.pending_marks.v = VMark::First,
            "clvmrg" if self.in_row => self.pending_marks.v = VMark::Merged,
            "cellx" if self.in_row => {
                let marks = std::mem::take(&mut self.pending_marks);
                self.row_marks.push(marks);
            }
            "cell" if self.in_row => {
                let cell = std::mem::take(&mut self.cell);
                self.row.push(cell);
            }
            "row" if self.in_row => self.end_row(),
            "ql" => self.para.align = Some(RichAlignment::Left),
            "qc" => self.para.align = Some(RichAlignment::Center),
            "qr" => self.para.align = Some(RichAlignment::Right),
            "qj" => self.para.align = Some(RichAlignment::Justify),
            "ls" => {
                self.para.list = Some(if param == Some(2) {
                    crate::RichListKind::Ordered
                } else {
                    crate::RichListKind::Bullet
                });
            }
            "li" => {
                self.para.left_indent =
                    param.filter(|v| *v > 0).and_then(|v| u32::try_from(v).ok());
            }
            "fi" => {
                self.para.first_line = param.filter(|v| *v != 0);
            }
            "sl" => {
                self.para.sl = param;
            }
            "slmult" => {
                self.para.slmult = on;
            }
            "outlinelevel" => {
                if let Some(n) = param.filter(|n| (0..=5).contains(n)) {
                    self.para.heading = (n + 1) as u8;
                }
            }
            _ => self.char_word(word, param, on),
        }
    }

    fn char_word(&mut self, word: &str, param: Option<i32>, on: bool) {
        let top = self.top();
        let st = &mut top.chars;
        match word {
            "b" => st.bold = on,
            "i" => st.italic = on,
            "ul" | "uld" | "uldb" | "ulw" => st.underline = on,
            "ulnone" => st.underline = false,
            "strike" | "striked" => st.strike = on,
            "super" => {
                st.script = if on {
                    RichScript::Superscript
                } else {
                    RichScript::Normal
                }
            }
            "sub" => {
                st.script = if on {
                    RichScript::Subscript
                } else {
                    RichScript::Normal
                }
            }
            "nosupersub" => st.script = RichScript::Normal,
            "fs" => {
                st.size = param.and_then(|v| u16::try_from(v).ok()).filter(|v| *v > 0);
            }
            "f" => st.font = param,
            "cf" => st.color = param.and_then(|v| usize::try_from(v).ok()),
            "highlight" | "chcbpat" | "cb" => {
                st.highlight = param.and_then(|v| usize::try_from(v).ok());
            }
            "plain" => *st = CharState::default(),
            _ => {}
        }
    }

    fn escape_symbol(&mut self, rest: &str) -> usize {
        let Some(sym) = rest.chars().next() else {
            return 0;
        };
        let mut buf = [0u8; 4];
        match sym {
            '\\' | '{' | '}' => self.emit(sym.encode_utf8(&mut buf)),
            '~' => self.emit("\u{00A0}"),
            '_' => self.emit("\u{2011}"),
            '*' => self.top().dest = Dest::Skip,
            '\n' | '\r' => self.body_word("par", None),
            '\'' => {
                let hex = rest
                    .get(1..3)
                    .filter(|_| escape_len(rest) == 3)
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(b1) = hex {
                    if let Some(b2) = rest
                        .get(3..7)
                        .and_then(|s| s.strip_prefix("\\'"))
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        if let Some(ch) = crate::cp949::decode_cp949_pair(b1, b2) {
                            self.emit(ch.encode_utf8(&mut buf));
                            return 7;
                        }
                    }
                    self.emit(char::from(b1).encode_utf8(&mut buf));
                }
            }
            _ => {}
        }
        escape_len(rest)
    }

    /// Reads `\ucN` fallback units as the text runs: each plain character, `\'hh`, control
    /// word, control symbol or whole `{...}` group is one unit, and nothing is skipped past the
    /// end of the group that holds the `\uN`.
    fn run(&mut self, text: &str) -> Result<(), EasyOfficeError> {
        let mut i = 0usize;
        let mut skip_fallback = 0usize;
        while i < text.len() {
            let rest = &text[i..];
            let Some(ch) = rest.chars().next() else { break };
            match ch {
                '{' if skip_fallback > 0 => {
                    skip_fallback -= 1;
                    i += group_len(rest);
                }
                '{' => {
                    if self.stack.len() >= MAX_GROUP_DEPTH {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "RTF 그룹 중첩이 허용 깊이를 초과했습니다.".to_string(),
                        ));
                    }
                    let mut g = self.top().clone();
                    g.first_word = true;
                    self.stack.push(g);
                    skip_fallback = 0;
                    i += 1;
                }
                '}' => {
                    if self.stack.len() > 1 {
                        self.stack.pop();
                    }
                    skip_fallback = 0;
                    i += 1;
                }
                '\\' => {
                    let after = &rest[1..];
                    let name_len = after.bytes().take_while(u8::is_ascii_alphabetic).count();
                    if name_len == 0 {
                        // `\*` marks an ignorable destination; `\'hh` is a code-page byte.
                        let adv = if skip_fallback > 0 {
                            skip_fallback -= 1;
                            escape_len(after)
                        } else {
                            self.escape_symbol(after)
                        };
                        i += 1 + adv;
                        continue;
                    }
                    let word = &after[..name_len];
                    let mut j = name_len;
                    let digits_from = j;
                    if after[j..].starts_with('-') {
                        j += 1;
                    }
                    j += after[j..].bytes().take_while(u8::is_ascii_digit).count();
                    let param = after[digits_from..j].parse::<i32>().ok();
                    if after[j..].starts_with(' ') {
                        j += 1;
                    }
                    i += 1 + j;
                    if skip_fallback > 0 {
                        skip_fallback -= 1;
                        continue;
                    }
                    self.control_word(word, param);
                    if word == "u" && param.is_some() {
                        skip_fallback = self.top().unicode_skip;
                    }
                }
                '\r' | '\n' => i += 1,
                _ => {
                    if skip_fallback > 0 {
                        skip_fallback -= 1;
                    } else {
                        let end = rest
                            .find(['\\', '{', '}', '\r', '\n'])
                            .unwrap_or(rest.len());
                        let end = end.max(ch.len_utf8());
                        let chunk = &rest[..end];
                        self.emit(chunk);
                        i += end;
                        continue;
                    }
                    i += ch.len_utf8();
                }
            }
        }
        Ok(())
    }
}

/// Bytes of the control symbol after a backslash: `\'hh` is three, any other symbol its character.
fn escape_len(after: &str) -> usize {
    match after.as_bytes() {
        [b'\'', hi, lo, ..] if hi.is_ascii_hexdigit() && lo.is_ascii_hexdigit() => 3,
        _ => after.chars().next().map_or(0, char::len_utf8),
    }
}

/// Bytes of the `{...}` group at the start of `rest` (to the end of the text when it is never closed).
fn group_len(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let (mut depth, mut i) = (0usize, 0usize);
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    rest.len()
}

/// Decodes an RTF document into runs, paragraph properties, and tables.
pub(crate) fn read_rtf(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let text_cow = match std::str::from_utf8(bytes) {
        Ok(s) => std::borrow::Cow::Borrowed(s),
        Err(_) => std::borrow::Cow::Owned(crate::cp949::decode_cp949_or_latin1_bytes(bytes)),
    };
    let mut reader = Reader::new();
    reader.run(text_cow.as_ref())?;
    reader.end_paragraph();
    reader.flush_table();
    let title = if reader.title.trim().is_empty() {
        DEFAULT_TITLE.to_string()
    } else {
        reader.title.trim().to_string()
    };
    let mut warnings = vec!["표 셀 서식·그림·머리말은 가져오지 않았습니다.".to_string()];
    if reader.merges_dropped > 0 {
        warnings.push(format!(
            "표의 병합 {}곳은 범위를 벗어나거나 겹쳐 가져오지 않았습니다.",
            reader.merges_dropped
        ));
    }
    Ok((
        RichDocument {
            title,
            blocks: reader.blocks,
            official_agency: None,
        },
        warnings,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_reads_super_sub_para_layout_and_special_characters() {
        let rtf = concat!(
            "{\\rtf1\\ansi\\pard\\li720\\fi-360\\sl360\\slmult1 ",
            "H{\\sub 2}O X\\super 3\\nosupersub !\\par",
            "\\pard \\bullet \\lquote A\\rquote \\ldblquote B\\rdblquote \\enspace\\emspace\\~\\_\\par}"
        );
        let (doc, _) = read_rtf(rtf.as_bytes()).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        let RichBlock::Paragraph(p0) = &doc.blocks[0] else {
            panic!("paragraph 0");
        };
        assert_eq!(p0.left_indent_twips, Some(720));
        assert_eq!(p0.first_line_indent_twips, Some(-360));
        assert_eq!(p0.line_spacing_percent, Some(150));
        assert!(p0
            .runs
            .iter()
            .any(|r| r.text == "2" && r.script == RichScript::Subscript));
        assert!(p0
            .runs
            .iter()
            .any(|r| r.text == "3" && r.script == RichScript::Superscript));
        assert!(p0
            .runs
            .iter()
            .any(|r| r.text == "!" && r.script == RichScript::Normal));

        let RichBlock::Paragraph(p1) = &doc.blocks[1] else {
            panic!("paragraph 1");
        };
        assert_eq!(
            p1.plain_text(),
            "\u{2022}\u{2018}A\u{2019}\u{201C}B\u{201D}\u{2002}\u{2003}\u{00A0}\u{2011}"
        );
    }

    #[test]
    fn rtf_reads_chcbpat_cb_outlinelevel_and_pn_list_levels() {
        let rtf = concat!(
            "{\\rtf1\\ansi{\\colortbl ;\\red255\\green255\\blue0;\\red0\\green255\\blue255;}",
            "\\pard\\outlinelevel1 HeadingTwo\\par",
            "\\pard{\\*\\pn\\pnlvlblt}\\chcbpat1 BulletItem\\par",
            "\\pard\\pnlvlbody\\cb2 OrderedItem\\par}"
        );
        let (doc, _) = read_rtf(rtf.as_bytes()).unwrap();
        assert_eq!(doc.blocks.len(), 3);
        let RichBlock::Paragraph(h2) = &doc.blocks[0] else {
            panic!("paragraph 0");
        };
        assert_eq!(h2.heading_level, 2);
        let RichBlock::Paragraph(b_item) = &doc.blocks[1] else {
            panic!("paragraph 1");
        };
        assert_eq!(b_item.list, Some(crate::RichListKind::Bullet));
        assert_eq!(b_item.runs[0].highlight.as_deref(), Some("FFFF00"));
        let RichBlock::Paragraph(o_item) = &doc.blocks[2] else {
            panic!("paragraph 2");
        };
        assert_eq!(o_item.list, Some(crate::RichListKind::Ordered));
        assert_eq!(o_item.runs[0].highlight.as_deref(), Some("00FFFF"));
    }
}
