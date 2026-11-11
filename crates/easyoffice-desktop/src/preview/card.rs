//! The first page of a document the shell has no page renderer for, drawn as a card picture
//! (PD-179): a text page, a spreadsheet grid or a slide, laid out from the content the engine
//! already reads (`card_source.rs`) and written as one SVG that `usvg`/`resvg` rasterize like the
//! Hangul pages (`save_thumbnail::render_png_sized`). It is a picture of what the file holds, not
//! of how the originating program would lay it out.
//!
//! The canvas has the card's 4:3 (`object-fit: cover` from the top shows all of it). Everything
//! drawn is cut to a fixed number of lines, cells and characters before it is written, so the SVG
//! stays a few tens of KB whatever the document holds, and every string is escaped for XML.

use std::fmt::Write;

pub const CANVAS_WIDTH: f32 = 600.0;
pub const CANVAS_HEIGHT: f32 = 450.0;

const INK: &str = "#1f2328";
const MUTED: &str = "#6b7177";
const RULE: &str = "#c9ced3";
const HEADER_FILL: &str = "#eef0f2";
const ACCENT: &str = "#3b7ddd";
const FONT: &str = "sans-serif";

/// Most characters of one paragraph, cell or line that are looked at.
const MAX_TEXT_CHARS: usize = 400;
/// Most blocks of a document that are laid out (the page holds about 20 lines).
const MAX_BLOCKS: usize = 64;

/// Width of a character in em: the wide scripts (Hangul, kana, CJK, full-width forms) a full em,
/// narrow Latin letters and punctuation less.
fn advance_em(c: char) -> f32 {
    match c as u32 {
        0x20 => 0.3,
        0x21..=0x7E => match c {
            'i' | 'j' | 'l' | 'I' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' | '`' => 0.3,
            'm' | 'w' | 'M' | 'W' | '@' => 0.85,
            _ if c.is_ascii_uppercase() => 0.64,
            _ => 0.55,
        },
        0x1100..=0x11FF
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFF => 1.0,
        _ => 0.6,
    }
}

fn text_width(text: &str, size: f32) -> f32 {
    text.chars().map(advance_em).sum::<f32>() * size
}

/// Whether XML 1.0 can carry the character: its `Char` production (tab, line feeds, U+0020 up to
/// U+D7FF, U+E000 up to U+FFFD, and the supplementary planes) without the Unicode noncharacters
/// (U+FDD0 to U+FDEF, U+FFFE and U+FFFF of every plane), which parsers refuse in practice. A Rust
/// string holds no lone surrogate, so none is left to test for.
fn is_xml_char(c: char) -> bool {
    let code = c as u32;
    matches!(code, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x1_0000..=0x10_FFFF)
        && !(0xFDD0..=0xFDEF).contains(&code)
        && code & 0xFFFE != 0xFFFE
}

/// The text with control characters and characters XML cannot carry turned into spaces (one stray
/// character must not make the parser refuse the whole card) and at most [`MAX_TEXT_CHARS`]
/// characters.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || !is_xml_char(c) {
                ' '
            } else {
                c
            }
        })
        .take(MAX_TEXT_CHARS)
        .collect()
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// `text` cut to `width` with an ellipsis when it does not fit.
fn clip(text: &str, size: f32, width: f32) -> String {
    let text = clean(text);
    if text_width(&text, size) <= width {
        return text;
    }
    let room = width - text_width("…", size);
    let mut used = 0.0;
    let mut out: String = text
        .chars()
        .take_while(|c| {
            used += advance_em(*c) * size;
            used <= room
        })
        .collect();
    out.push('…');
    out
}

/// `text` broken into at most `max_lines` lines of `width`: at spaces where there are any, inside
/// a word where a word alone is wider than a line.
pub(super) fn wrap(text: &str, size: f32, width: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut line_width = 0.0;
    let mut last_space: Option<usize> = None;
    for c in clean(text).chars() {
        let advance = advance_em(c) * size;
        if line_width + advance > width && !line.is_empty() {
            let rest = match last_space.filter(|at| *at > 0) {
                Some(at) => {
                    let rest = line[at..].trim_start().to_string();
                    line.truncate(at);
                    rest
                }
                None => String::new(),
            };
            lines.push(line.trim_end().to_string());
            if lines.len() == max_lines {
                return lines;
            }
            line = rest;
            line_width = text_width(&line, size);
            last_space = None;
        }
        if c == ' ' {
            if line.is_empty() {
                continue;
            }
            last_space = Some(line.len());
        }
        line.push(c);
        line_width += advance;
    }
    if !line.trim().is_empty() {
        lines.push(line.trim_end().to_string());
    }
    lines
}

/// One thing on a text page.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph {
        text: String,
        /// 1 to 6 for headings, 0 for body text.
        heading: u8,
        bullet: bool,
        align: Align,
    },
    Table(Vec<Vec<String>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// What the card shows of a document.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Page(Vec<Block>),
    /// Rows of displayed cell values from the sheet's top left corner.
    Grid(Vec<Vec<String>>),
    Slide {
        title: String,
        lines: Vec<String>,
    },
}

/// How one piece of text is drawn.
#[derive(Clone, Copy)]
struct Style {
    size: f32,
    fill: &'static str,
    bold: bool,
    align: Align,
}

impl Style {
    const fn new(size: f32, fill: &'static str) -> Self {
        Self {
            size,
            fill,
            bold: false,
            align: Align::Left,
        }
    }

    const fn bold(self, bold: bool) -> Self {
        Self { bold, ..self }
    }

    const fn align(self, align: Align) -> Self {
        Self { align, ..self }
    }
}

struct Svg(String);

impl Svg {
    fn new() -> Self {
        let mut svg = String::with_capacity(8 * 1024);
        let _ = write!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{CANVAS_WIDTH}\" height=\"{CANVAS_HEIGHT}\" viewBox=\"0 0 {CANVAS_WIDTH} {CANVAS_HEIGHT}\"><rect width=\"{CANVAS_WIDTH}\" height=\"{CANVAS_HEIGHT}\" fill=\"#ffffff\"/>"
        );
        Self(svg)
    }

    fn rect(&mut self, (x, y, w, h): (f32, f32, f32, f32), fill: &str, stroke: Option<&str>) {
        let _ = write!(
            self.0,
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{fill}\""
        );
        if let Some(stroke) = stroke {
            let _ = write!(self.0, " stroke=\"{stroke}\" stroke-width=\"1\"");
        }
        self.0.push_str("/>");
    }

    fn text(&mut self, (x, y): (f32, f32), style: Style, text: &str) {
        let Style {
            size,
            fill,
            bold,
            align,
        } = style;
        let anchor = match align {
            Align::Left => "start",
            Align::Center => "middle",
            Align::Right => "end",
        };
        let weight = if bold { " font-weight=\"700\"" } else { "" };
        let _ = write!(
            self.0,
            "<text x=\"{x}\" y=\"{y}\" font-family=\"{FONT}\" font-size=\"{size}\" fill=\"{fill}\" text-anchor=\"{anchor}\"{weight}>"
        );
        escape(text, &mut self.0);
        self.0.push_str("</text>");
    }

    fn finish(mut self) -> String {
        self.0.push_str("</svg>");
        self.0
    }
}

const PAGE_MARGIN_X: f32 = 44.0;
const PAGE_MARGIN_TOP: f32 = 40.0;
const PAGE_BOTTOM: f32 = CANVAS_HEIGHT - 16.0;
const BODY_SIZE: f32 = 16.0;
const LINE_GAP: f32 = 1.5;
const PARAGRAPH_GAP: f32 = 6.0;
const BULLET_INDENT: f32 = 16.0;
const TABLE_SIZE: f32 = 13.0;
const TABLE_ROW_HEIGHT: f32 = 26.0;
pub const TABLE_MAX_COLUMNS: usize = 5;
pub const TABLE_MAX_ROWS: usize = 8;

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 30.0,
        2 => 24.0,
        3 => 20.0,
        _ => 17.0,
    }
}

fn page_svg(blocks: &[Block]) -> String {
    let mut svg = Svg::new();
    let width = CANVAS_WIDTH - 2.0 * PAGE_MARGIN_X;
    let mut y = PAGE_MARGIN_TOP;
    for block in blocks.iter().take(MAX_BLOCKS) {
        match block {
            Block::Paragraph {
                text,
                heading,
                bullet,
                align,
            } => {
                let size = if *heading > 0 {
                    heading_size(*heading)
                } else {
                    BODY_SIZE
                };
                let indent = if *bullet { BULLET_INDENT } else { 0.0 };
                let step = size * LINE_GAP;
                let room = ((PAGE_BOTTOM - y) / step).floor().max(0.0) as usize;
                if room == 0 {
                    break;
                }
                let lines = wrap(text, size, width - indent, room);
                if lines.is_empty() {
                    continue;
                }
                for (n, line) in lines.iter().enumerate() {
                    y += step;
                    let x = match align {
                        Align::Left => PAGE_MARGIN_X + indent,
                        Align::Center => CANVAS_WIDTH / 2.0,
                        Align::Right => CANVAS_WIDTH - PAGE_MARGIN_X,
                    };
                    let baseline = y - size * 0.35;
                    if *bullet && n == 0 {
                        svg.text((PAGE_MARGIN_X, baseline), Style::new(size, MUTED), "•");
                    }
                    let style = Style::new(size, INK).bold(*heading > 0).align(*align);
                    svg.text((x, baseline), style, line);
                }
                y += PARAGRAPH_GAP;
            }
            Block::Table(rows) => {
                let columns = rows
                    .iter()
                    .map(Vec::len)
                    .max()
                    .unwrap_or(0)
                    .min(TABLE_MAX_COLUMNS);
                if columns == 0 || y + TABLE_ROW_HEIGHT > PAGE_BOTTOM {
                    continue;
                }
                let cell_width = width / columns as f32;
                for row in rows.iter().take(TABLE_MAX_ROWS) {
                    if y + TABLE_ROW_HEIGHT > PAGE_BOTTOM {
                        break;
                    }
                    for column in 0..columns {
                        let x = PAGE_MARGIN_X + cell_width * column as f32;
                        svg.rect((x, y, cell_width, TABLE_ROW_HEIGHT), "#ffffff", Some(RULE));
                        if let Some(cell) = row.get(column) {
                            let text = clip(cell, TABLE_SIZE, cell_width - 10.0);
                            svg.text((x + 5.0, y + 18.0), Style::new(TABLE_SIZE, INK), &text);
                        }
                    }
                    y += TABLE_ROW_HEIGHT;
                }
                y += PARAGRAPH_GAP;
            }
        }
    }
    svg.finish()
}

pub const GRID_ROWS: usize = 14;
pub const GRID_COLUMNS: usize = 5;
const GRID_LEFT: f32 = 22.0;
const GRID_TOP: f32 = 18.0;
const GRID_HEADER: f32 = 24.0;
const GRID_ROW_NUMBER_WIDTH: f32 = 36.0;
const GRID_CELL_WIDTH: f32 = 104.0;
const GRID_ROW_HEIGHT: f32 = 26.0;
const GRID_SIZE: f32 = 14.0;

fn column_letter(index: usize) -> char {
    char::from(b'A' + u8::try_from(index % 26).unwrap_or(0))
}

fn grid_svg(rows: &[Vec<String>]) -> String {
    let mut svg = Svg::new();
    let origin_x = GRID_LEFT + GRID_ROW_NUMBER_WIDTH;
    let grid_width = GRID_CELL_WIDTH * GRID_COLUMNS as f32;
    svg.rect(
        (
            GRID_LEFT,
            GRID_TOP,
            GRID_ROW_NUMBER_WIDTH + grid_width,
            GRID_HEADER,
        ),
        HEADER_FILL,
        Some(RULE),
    );
    for column in 0..GRID_COLUMNS {
        let x = origin_x + GRID_CELL_WIDTH * column as f32;
        svg.rect(
            (x, GRID_TOP, GRID_CELL_WIDTH, GRID_HEADER),
            HEADER_FILL,
            Some(RULE),
        );
        svg.text(
            (x + GRID_CELL_WIDTH / 2.0, GRID_TOP + 17.0),
            Style::new(GRID_SIZE, MUTED).align(Align::Center),
            &column_letter(column).to_string(),
        );
    }
    for row in 0..GRID_ROWS {
        let y = GRID_TOP + GRID_HEADER + GRID_ROW_HEIGHT * row as f32;
        svg.rect(
            (GRID_LEFT, y, GRID_ROW_NUMBER_WIDTH, GRID_ROW_HEIGHT),
            HEADER_FILL,
            Some(RULE),
        );
        svg.text(
            (GRID_LEFT + GRID_ROW_NUMBER_WIDTH / 2.0, y + 18.0),
            Style::new(GRID_SIZE, MUTED).align(Align::Center),
            &(row + 1).to_string(),
        );
        for column in 0..GRID_COLUMNS {
            let x = origin_x + GRID_CELL_WIDTH * column as f32;
            svg.rect(
                (x, y, GRID_CELL_WIDTH, GRID_ROW_HEIGHT),
                "#ffffff",
                Some(RULE),
            );
            let Some(value) = rows
                .get(row)
                .and_then(|r| r.get(column))
                .filter(|v| !v.is_empty())
            else {
                continue;
            };
            let text = clip(value, GRID_SIZE, GRID_CELL_WIDTH - 10.0);
            if value.trim().parse::<f64>().is_ok() {
                let style = Style::new(GRID_SIZE, INK).align(Align::Right);
                svg.text((x + GRID_CELL_WIDTH - 5.0, y + 18.0), style, &text);
            } else {
                svg.text((x + 5.0, y + 18.0), Style::new(GRID_SIZE, INK), &text);
            }
        }
    }
    svg.finish()
}

const SLIDE_MARGIN_X: f32 = 48.0;
const SLIDE_TITLE_SIZE: f32 = 34.0;
const SLIDE_TITLE_LINES: usize = 2;
const SLIDE_BODY_SIZE: f32 = 22.0;
const SLIDE_BODY_LINES: usize = 8;

fn slide_svg(title: &str, lines: &[String]) -> String {
    let mut svg = Svg::new();
    let width = CANVAS_WIDTH - 2.0 * SLIDE_MARGIN_X;
    svg.rect((0.0, 0.0, 10.0, CANVAS_HEIGHT), ACCENT, None);
    let mut y = 36.0;
    for line in wrap(title, SLIDE_TITLE_SIZE, width, SLIDE_TITLE_LINES) {
        y += SLIDE_TITLE_SIZE * 1.3;
        let style = Style::new(SLIDE_TITLE_SIZE, INK).bold(true);
        svg.text((SLIDE_MARGIN_X, y), style, &line);
    }
    y += 24.0;
    let mut left = SLIDE_BODY_LINES;
    for bullet in lines {
        let wrapped = wrap(bullet, SLIDE_BODY_SIZE, width - 20.0, left);
        for (n, line) in wrapped.iter().enumerate() {
            y += SLIDE_BODY_SIZE * 1.45;
            if n == 0 {
                svg.text(
                    (SLIDE_MARGIN_X, y),
                    Style::new(SLIDE_BODY_SIZE, ACCENT),
                    "•",
                );
            }
            svg.text(
                (SLIDE_MARGIN_X + 20.0, y),
                Style::new(SLIDE_BODY_SIZE, INK),
                line,
            );
        }
        left = left.saturating_sub(wrapped.len());
        if left == 0 {
            break;
        }
        y += 6.0;
    }
    svg.finish()
}

/// The SVG of the card for `content`.
pub fn svg(content: &Content) -> String {
    match content {
        Content::Page(blocks) => page_svg(blocks),
        Content::Grid(rows) => grid_svg(rows),
        Content::Slide { title, lines } => slide_svg(title, lines),
    }
}
