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
use crate::{EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichParagraph, RichRun};

const MAX_GROUP_DEPTH: usize = 256;
const DEFAULT_TITLE: &str = "RTF 문서";
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
}

#[derive(Clone)]
struct Group {
    dest: Dest,
    chars: CharState,
    first_word: bool,
}

#[derive(Default)]
struct ParaProps {
    align: Option<RichAlignment>,
    heading: u8,
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
    table: Option<(usize, Vec<Vec<String>>, Vec<Vec<CellMarks>>)>,
    pending_marks: CellMarks,
    row_marks: Vec<CellMarks>,
    merges_dropped: usize,
    high_surrogate: Option<u16>,
}

impl Reader {
    fn new() -> Self {
        Self {
            stack: vec![Group {
                dest: Dest::Body,
                chars: CharState::default(),
                first_word: false,
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
            ..CharFmt::default()
        }
    }

    fn emit(&mut self, text: &str) {
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
        if let Some((_, rows, marks)) = self.table.take() {
            if !rows.is_empty() {
                let (table, dropped) = RichTable::with_merges(rows, merges_from_marks(&marks));
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
                    ..CharFmt::default()
                };
                push_run(&mut merged, &r.text, &fmt);
            }
            runs = merged;
        }
        if runs.iter().any(|r| !r.text.trim().is_empty()) {
            self.flush_table();
            self.blocks.push(RichBlock::Paragraph(RichParagraph {
                runs,
                alignment: para.align,
                heading_level: para.heading,
                list: None,
                ..RichParagraph::default()
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
            .is_some_and(|(idx, _, _)| *idx != self.table_idx)
        {
            self.flush_table();
        }
        match self.table.as_mut() {
            Some((_, rows, all_marks)) => {
                rows.push(row);
                all_marks.push(marks);
            }
            None => self.table = Some((self.table_idx, vec![row], vec![marks])),
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
        self.emit(&ch.unwrap_or('\u{FFFD}').to_string());
    }

    fn control_word(&mut self, word: &str, param: Option<i32>) {
        if word == "tblidx" {
            self.table_idx = usize::try_from(param.unwrap_or(0)).unwrap_or(0);
            return;
        }
        if word == "u" && self.top().dest != Dest::Skip {
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
            "pard" => {
                self.para.align = None;
            }
            "line" => self.emit("\n"),
            "tab" => self.emit("\t"),
            "emdash" => self.emit("\u{2014}"),
            "endash" => self.emit("\u{2013}"),
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
            "outlinelevel" => {
                let level = param.unwrap_or(0).clamp(0, 5) as u8 + 1;
                self.para.heading = level;
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
            "fs" => {
                st.size = param.and_then(|v| u16::try_from(v).ok()).filter(|v| *v > 0);
            }
            "f" => st.font = param,
            "cf" => st.color = param.and_then(|v| usize::try_from(v).ok()),
            "highlight" => st.highlight = param.and_then(|v| usize::try_from(v).ok()),
            "plain" => *st = CharState::default(),
            _ => {}
        }
    }

    fn escape_symbol(&mut self, rest: &str) -> usize {
        let Some(sym) = rest.chars().next() else {
            return 0;
        };
        match sym {
            '\\' | '{' | '}' => self.emit(&sym.to_string()),
            '~' => self.emit("\u{00A0}"),
            '_' => self.emit("-"),
            '*' => self.top().dest = Dest::Skip,
            '\n' | '\r' => self.body_word("par", None),
            '\'' => {
                let hex = rest.get(1..3).and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(byte) = hex {
                    self.emit(&char::from(byte).to_string());
                    return 3;
                }
            }
            _ => {}
        }
        sym.len_utf8()
    }

    fn run(&mut self, text: &str) -> Result<(), EasyOfficeError> {
        let mut i = 0usize;
        let mut skip_fallback = 0usize;
        while i < text.len() {
            let rest = &text[i..];
            let Some(ch) = rest.chars().next() else { break };
            match ch {
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
                        let used = self.escape_symbol(after);
                        skip_fallback = skip_fallback.saturating_sub(1);
                        i += 1 + used;
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
                    skip_fallback = usize::from(word == "u");
                    self.control_word(word, param);
                    i += 1 + j;
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

/// Decodes an RTF document into runs, paragraph properties, and tables.
pub(crate) fn read_rtf(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| EasyOfficeError::InvalidUtf8("RTF UTF-8 디코딩 실패".to_string()))?;
    let mut reader = Reader::new();
    reader.run(text)?;
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
