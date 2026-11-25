//! ODT reader producing a [`RichDocument`] from `content.xml` automatic styles: span formats,
//! paragraph alignment, `text:h` outline levels, list paragraphs, and tables in position.

mod frame;
mod notes;
mod table;

use std::borrow::Cow;
use std::collections::HashMap;

use self::frame::{parse_odt_list_level_bullet, parse_odt_list_level_number, OdtListStyle};
use self::notes::OdtNotesState;
use self::table::{read_frame, OdtTableState, OdtTableStylesMap};
use crate::decode_budget::DecodeBudget;
use crate::office_codecs::{checked_part, checked_zip_xml_entries_for};
use crate::rich_document::{normalize_hex_color, safe_link_target, toc_bookmark_number, RichToc};
use crate::rich_image::{safe_package_path, ImageBudget};
use crate::rich_table::{join_cell_paragraphs, TableAssembler, MAX_PADDED_COLUMNS};
use crate::rich_xml::{check_paragraph_depth, take_toc_page, toc_role_from_name, CharFmt};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    DocumentFormat, EasyOfficeError, OdfCodec, RichAlignment, RichBlock, RichColumns, RichDocument,
    RichField, RichImage, RichListKind, RichParagraph, RichRun, RichScript, ZipArchiveCodec,
    ZipArchiveEntry, ZipSecurityPolicy,
};

const EMU_PER_INCH: f64 = 914_400.0;
const EMU_PER_CM: f64 = 360_000.0;
const EMU_PER_MM: f64 = 36_000.0;
const EMU_PER_POINT: f64 = 12_700.0;
const EMU_PER_PIXEL: f64 = 9_525.0;
/// EMU per twip (1/20 point).
const EMU_PER_TWIP: f64 = 635.0;
const MAX_LENGTH_EMU: f64 = 182_880_000.0;
const MAX_LINE_PERCENT: f64 = 1_000.0;

/// Left margin the ODT writer gives list paragraphs; the reader treats it as the list marker.
const LIST_MARGIN: &str = "0.5in";

const MAX_STYLE_INHERIT_DEPTH: usize = 16;
const SPACES_64: &str = "                                                                ";

#[derive(Default, Clone)]
struct StyleProps {
    parent: Option<String>,
    align: Option<RichAlignment>,
    fmt: CharFmt,
    list_margin: bool,
    list_kind: Option<RichListKind>,
    left_twips: Option<u32>,
    right_twips: Option<u32>,
    first_line_twips: Option<i32>,
    line_percent: Option<u16>,
    page_break_before: bool,
    /// Columns of a section style (`style:section-properties`); a count of 1 stands for one column.
    columns: Option<RichColumns>,
}

/// A CSS/ODF length (`36pt`, `0.5in`, `1.27cm`, `12.7mm`, `48px`) in EMU.
fn length_emu(value: &str) -> Option<f64> {
    let v = value.trim();
    let (number, unit) = v.split_at(v.find(|c: char| c.is_ascii_alphabetic())?);
    let n: f64 = number.trim().parse().ok()?;
    let per_unit = match unit {
        "in" => EMU_PER_INCH,
        "cm" => EMU_PER_CM,
        "mm" => EMU_PER_MM,
        "pt" => EMU_PER_POINT,
        "px" => EMU_PER_PIXEL,
        _ => return None,
    };
    let emu = n * per_unit;
    (emu.is_finite() && emu.abs() <= MAX_LENGTH_EMU).then_some(emu)
}

fn length_twips(value: &str) -> Option<i32> {
    length_emu(value).map(|e| (e / EMU_PER_TWIP).round() as i32)
}

/// `fo:line-height` as a percentage of single spacing (`100%`, `200%`), or an absolute length
/// scaled against a `12pt` (`240` twip) single line.
fn line_height_percent(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    if let Some(raw) = trimmed.strip_suffix('%') {
        let n: f64 = raw.trim().parse().ok()?;
        let percent = n.round();
        return (n.is_finite() && (1.0..=MAX_LINE_PERCENT).contains(&percent))
            .then_some(percent as u16);
    }
    let twips = length_twips(trimmed).filter(|t| *t > 0)?;
    Some(((i64::from(twips) * 100 + 120) / 240).clamp(50, 1000) as u16)
}

fn off(value: Option<&str>) -> bool {
    value.is_none_or(|v| v == "none")
}

/// `14pt`/`9.5pt` to half-points; other units are ignored.
/// `style:text-position` such as `super 58%`, `sub 58%` or `33% 58%` (positive raise = super).
fn script_of(position: Option<&str>) -> RichScript {
    let first = position
        .and_then(|p| p.split_whitespace().next())
        .unwrap_or("");
    match first {
        "super" => RichScript::Superscript,
        "sub" => RichScript::Subscript,
        other => match other.strip_suffix('%').and_then(|n| n.parse::<i32>().ok()) {
            Some(n) if n > 0 => RichScript::Superscript,
            Some(n) if n < 0 => RichScript::Subscript,
            _ => RichScript::Normal,
        },
    }
}

fn parse_points(value: &str) -> Option<u16> {
    let pt: f32 = value.trim().strip_suffix("pt")?.parse().ok()?;
    let half = (pt * 2.0).round();
    (1.0..=f32::from(u16::MAX))
        .contains(&half)
        .then_some(half as u16)
}

/// Character format described by a `style:text-properties` element.
fn char_fmt_from_text_properties(attrs: &Attrs<'_>) -> CharFmt {
    CharFmt {
        bold: attr(attrs, "fo:font-weight")
            .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)),
        italic: attr(attrs, "fo:font-style").is_some_and(|s| s == "italic" || s == "oblique"),
        underline: !off(attr(attrs, "style:text-underline-style").as_deref()),
        strike: !off(attr(attrs, "style:text-line-through-style").as_deref()),
        size_half_pt: ["style:font-size-asian", "fo:font-size"]
            .into_iter()
            .find_map(|k| attr(attrs, k).as_deref().and_then(parse_points)),
        font: [
            "style:font-name-asian",
            "style:font-family-asian",
            "style:font-name",
            "fo:font-family",
        ]
        .into_iter()
        .find_map(|k| {
            attr(attrs, k)
                .map(|f| f.trim().trim_matches('\'').to_string())
                .filter(|f| !f.is_empty())
        }),
        color: attr(attrs, "fo:color")
            .as_deref()
            .and_then(normalize_hex_color),
        highlight: attr(attrs, "fo:background-color")
            .as_deref()
            .and_then(normalize_hex_color),
        script: script_of(attr(attrs, "style:text-position").as_deref()),
    }
}

/// Alignment, list marker margin, indents and line spacing of `style:paragraph-properties`.
fn apply_paragraph_properties(p: &mut StyleProps, attrs: &Attrs<'_>) {
    p.align = attr(attrs, "fo:text-align")
        .as_deref()
        .and_then(RichAlignment::parse);
    p.list_margin = attr(attrs, "fo:margin-left").as_deref() == Some(LIST_MARGIN);
    if !p.list_margin {
        p.left_twips = attr(attrs, "fo:margin-left")
            .as_deref()
            .and_then(length_twips)
            .and_then(|t| u32::try_from(t).ok())
            .filter(|t| *t > 0);
    }
    p.right_twips = attr(attrs, "fo:margin-right")
        .as_deref()
        .and_then(length_twips)
        .and_then(|t| u32::try_from(t).ok())
        .filter(|t| *t > 0);
    p.first_line_twips = attr(attrs, "fo:text-indent")
        .as_deref()
        .and_then(length_twips)
        .filter(|t| *t != 0);
    p.line_percent = attr(attrs, "fo:line-height")
        .as_deref()
        .and_then(line_height_percent);
    p.page_break_before = attr(attrs, "fo:break-before").as_deref() == Some("page");
}

/// The columns a `style:columns` element sets: count, equal widths and the gap (720 twips when unreadable).
fn section_columns(attrs: &Attrs<'_>) -> RichColumns {
    let count = attr(attrs, "fo:column-count")
        .and_then(|v| v.trim().parse::<u16>().ok())
        .map_or(1, |n| n.clamp(1, 16) as u8);
    let gap_twips = attr(attrs, "fo:column-gap")
        .as_deref()
        .and_then(length_twips)
        .map_or(720, |t| t.clamp(0, i32::from(u16::MAX)) as u16);
    RichColumns {
        count,
        equal: true,
        gap_twips,
    }
}

fn read_styles_into(
    xml: &str,
    styles: &mut HashMap<String, StyleProps>,
    lists: &mut HashMap<String, OdtListStyle>,
    table_styles: &mut OdtTableStylesMap,
) -> Result<(), EasyOfficeError> {
    let mut cur: Option<(String, StyleProps)> = None;
    let mut cur_list: Option<String> = None;
    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start { name, ref attrs } => match name {
                "style:style" => {
                    let list_kind = match attr(attrs, "style:list-style-name").as_deref() {
                        Some("L_Ordered" | "Numbering" | "Ordered") => Some(RichListKind::Ordered),
                        Some("L_Bullet" | "Bullet") => Some(RichListKind::Bullet),
                        _ => None,
                    };
                    let parent = attr(attrs, "style:parent-style-name").map(Cow::into_owned);
                    let family = attr(attrs, "style:family").unwrap_or_default();
                    cur = attr(attrs, "style:name").map(|n| {
                        table_styles.start_style(n.to_string(), &family);
                        (
                            n.to_string(),
                            StyleProps {
                                parent,
                                list_kind,
                                ..StyleProps::default()
                            },
                        )
                    });
                }
                "style:columns" => {
                    if let Some((_, p)) = cur.as_mut() {
                        p.columns = Some(section_columns(attrs));
                    }
                }
                "style:table-column-properties" => table_styles.on_col_props(attrs),
                "style:table-cell-properties" => table_styles.on_cell_props(attrs),
                "style:paragraph-properties" => {
                    if let Some((_, p)) = cur.as_mut() {
                        apply_paragraph_properties(p, attrs);
                    }
                }
                "style:text-properties" => {
                    if let Some((_, p)) = cur.as_mut() {
                        p.fmt = char_fmt_from_text_properties(attrs);
                    }
                }
                "text:list-style" => cur_list = attr(attrs, "style:name").map(Cow::into_owned),
                "text:list-level-style-number" => {
                    if let Some(n) = cur_list.as_ref() {
                        let (lvl, num) = parse_odt_list_level_number(attrs);
                        let entry = lists
                            .entry(n.clone())
                            .or_insert_with(|| OdtListStyle::new(RichListKind::Ordered));
                        entry.kind = RichListKind::Ordered;
                        entry.levels[lvl] = Some(num);
                    }
                }
                "text:list-level-style-bullet" => {
                    if let Some(n) = cur_list.as_ref() {
                        let parsed = parse_odt_list_level_bullet(attrs);
                        lists.entry(n.clone()).or_insert(parsed);
                    }
                }
                "office:body" => break,
                _ => {}
            },
            XmlEvent::End(name) => match name {
                "style:style" => {
                    table_styles.finish_style();
                    if let Some((n, p)) = cur.take() {
                        styles.insert(n, p);
                    }
                }
                "text:list-style" => cur_list = None,
                _ => {}
            },
            XmlEvent::Text(_) => {}
        }
    }
    Ok(())
}

fn resolve_style_parents(styles: &mut HashMap<String, StyleProps>) {
    let snapshot = styles.clone();
    for (name, props) in styles.iter_mut() {
        let mut cur_parent = props.parent.as_deref();
        for _ in 0..MAX_STYLE_INHERIT_DEPTH {
            let Some(p_name) = cur_parent else {
                break;
            };
            if p_name == name {
                break;
            }
            let Some(p) = snapshot.get(p_name) else {
                break;
            };
            if props.align.is_none() {
                props.align = p.align;
            }
            if !props.list_margin {
                props.list_margin = p.list_margin;
            }
            if props.list_kind.is_none() {
                props.list_kind = p.list_kind;
            }
            if props.left_twips.is_none() {
                props.left_twips = p.left_twips;
            }
            if props.right_twips.is_none() {
                props.right_twips = p.right_twips;
            }
            if props.first_line_twips.is_none() {
                props.first_line_twips = p.first_line_twips;
            }
            if props.line_percent.is_none() {
                props.line_percent = p.line_percent;
            }
            props.page_break_before |= p.page_break_before;
            props.fmt = merge_span_fmt(p.fmt.clone(), props.fmt.clone());
            cur_parent = p.parent.as_deref();
        }
    }
}

struct Para {
    runs: Vec<RichRun>,
    style: StyleProps,
    outline: u8,
    toc: Option<RichToc>,
    /// Number of a `_Toc` bookmark that starts in the paragraph.
    bookmark: Option<u32>,
    /// The entry style is not the `Plain` (blank tab fill) variant.
    toc_dots: bool,
    base: CharFmt,
    images: Vec<RichImage>,
}

const MAX_REPEATED_SPACES: usize = 1024;
const MAX_OUTLINE_LEVEL: u8 = 6;
const UNSUPPORTED_OBJECT_NOTE: &str = "그림·도형·OLE 개체는 가져오지 않았습니다.";

/// A positive count attribute; unreadable values are 1 and huge ones saturate.
fn count_attr(attrs: &Attrs<'_>, name: &str) -> u32 {
    attr(attrs, name)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(1, |v| u32::try_from(v).unwrap_or(u32::MAX).max(1))
}

/// Walker state for ODF `content.xml` events.
struct OdtWalker<'a> {
    styles: &'a HashMap<String, StyleProps>,
    list_styles: &'a HashMap<String, OdtListStyle>,
    table_styles: &'a OdtTableStylesMap,
    title: String,
    warnings: Vec<String>,
    blocks: Vec<RichBlock>,
    paras: Vec<Para>,
    span_fmts: Vec<CharFmt>,
    list_stack: Vec<OdtListStyle>,
    tbl_depth: usize,
    table: TableAssembler,
    table_state: OdtTableState,
    cell_paras: Vec<Vec<RichRun>>,
    /// Row span, column span and repeat count of the open top-level cell.
    cell_shape: (u32, u32, u32),
    /// Merged regions dropped as invalid in this document.
    dropped_merges: usize,
    in_automatic_styles: bool,
    /// The columns of the open `text:section`s, innermost last.
    sections: Vec<Option<RichColumns>>,
    budget: ImageBudget,
    text_budget: DecodeBudget,
    /// Address of the open `text:a`, when it is a safe link.
    link: Option<String>,
    /// The last paragraph that held no text asked for a page break; the next one carries it.
    pending_page_break: bool,
    /// Header and footer text: a page-number field is kept as a field run.
    frame_mode: bool,
    in_field: bool,
    notes: OdtNotesState,
}

/// Merges a span's character format into the enclosing format.
fn merge_span_fmt(cur: CharFmt, add: CharFmt) -> CharFmt {
    CharFmt {
        bold: cur.bold || add.bold,
        italic: cur.italic || add.italic,
        underline: cur.underline || add.underline,
        strike: cur.strike || add.strike,
        size_half_pt: add.size_half_pt.or(cur.size_half_pt),
        font: add.font.or(cur.font),
        color: add.color.or(cur.color),
        highlight: add.highlight.or(cur.highlight),
        script: if add.script == RichScript::Normal {
            cur.script
        } else {
            add.script
        },
    }
}

impl<'a> OdtWalker<'a> {
    fn new(
        styles: &'a HashMap<String, StyleProps>,
        list_styles: &'a HashMap<String, OdtListStyle>,
        table_styles: &'a OdtTableStylesMap,
    ) -> Self {
        Self {
            styles,
            list_styles,
            table_styles,
            title: String::new(),
            warnings: Vec::new(),
            blocks: Vec::new(),
            paras: Vec::new(),
            span_fmts: Vec::new(),
            list_stack: Vec::new(),
            tbl_depth: 0,
            table: TableAssembler::default(),
            table_state: OdtTableState::default(),
            cell_paras: Vec::new(),
            cell_shape: (1, 1, 1),
            dropped_merges: 0,
            in_automatic_styles: false,
            sections: Vec::new(),
            budget: ImageBudget::default(),
            text_budget: DecodeBudget::new(),
            link: None,
            pending_page_break: false,
            frame_mode: false,
            in_field: false,
            notes: OdtNotesState::default(),
        }
    }

    /// Handles an element start; returns true when the subtree must be skipped.
    fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) -> Result<bool, EasyOfficeError> {
        match name {
            "office:document-content" => {
                self.title = attr(attrs, "title").unwrap_or_default().into_owned()
            }
            "office:automatic-styles" | "office:font-face-decls" => self.in_automatic_styles = true,
            "text:section" => {
                let columns = attr(attrs, "text:style-name")
                    .and_then(|n| self.styles.get(n.as_ref()))
                    .and_then(|s| s.columns);
                self.sections.push(columns);
            }
            "office:annotation-end" | "text:change-start" | "text:change-end" | "text:change" => {
                self.notes.on_marker_start(
                    name,
                    attrs,
                    &mut self.paras,
                    &self.span_fmts,
                    self.link.as_deref(),
                    &mut self.text_budget,
                )?;
            }
            "draw:custom-shape" => {
                if !self.warnings.iter().any(|w| w == UNSUPPORTED_OBJECT_NOTE) {
                    self.warnings.push(UNSUPPORTED_OBJECT_NOTE.to_string());
                }
                return Ok(true);
            }
            "text:p" | "text:h" if !self.in_automatic_styles => {
                check_paragraph_depth(self.paras.len())?;
                self.start_paragraph(name, attrs);
            }
            "text:span" if !self.paras.is_empty() => self.start_span(attrs),
            "text:a" => {
                self.link = attr(attrs, "xlink:href")
                    .and_then(|href| safe_link_target(&href).map(str::to_string));
            }
            "text:bookmark-start" | "text:bookmark" => {
                let number = attr(attrs, "text:name").and_then(|n| toc_bookmark_number(&n));
                if let Some(top) = self.paras.last_mut() {
                    top.bookmark = top.bookmark.or(number);
                }
            }
            "text:page-number" | "text:page-count" | "text:file-name" if self.frame_mode => {
                self.in_field = true;
                self.push_field_run(match name {
                    "text:page-number" => RichField::PageNumber,
                    "text:page-count" => RichField::TotalPages,
                    _ => RichField::FileName,
                })?;
            }
            "text:tab" => self.push_inline("\t")?,
            "text:line-break" => self.push_inline("\n")?,
            "text:s" => {
                let mut rem = attr(attrs, "text:c")
                    .as_deref()
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(1)
                    .min(MAX_REPEATED_SPACES);
                while rem > 0 {
                    let chunk = rem.min(SPACES_64.len());
                    self.push_inline(&SPACES_64[..chunk])?;
                    rem -= chunk;
                }
            }
            "text:list" => {
                let entry = attr(attrs, "text:style-name")
                    .as_deref()
                    .and_then(|n| self.list_styles.get(n))
                    .copied()
                    .or_else(|| self.list_stack.last().copied())
                    .unwrap_or_else(|| OdtListStyle::new(RichListKind::Bullet));
                self.list_stack.push(entry);
            }
            "table:table" => {
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.table.reset();
                    self.table_state.reset();
                }
            }
            "table:table-column" if self.tbl_depth == 1 => {
                self.table_state.on_table_column(attrs, self.table_styles);
            }
            "table:table-row" if self.tbl_depth == 1 => self.table.start_row(),
            "table:table-cell" if self.tbl_depth == 1 => {
                self.cell_paras.clear();
                self.cell_shape = (
                    count_attr(attrs, "table:number-rows-spanned"),
                    count_attr(attrs, "table:number-columns-spanned"),
                    count_attr(attrs, "table:number-columns-repeated"),
                );
                self.table_state.start_cell(attrs, self.table_styles);
            }
            "table:covered-table-cell" if self.tbl_depth == 1 => {
                let repeated = count_attr(attrs, "table:number-columns-repeated");
                for _ in 0..repeated.min(self.padding_room()) {
                    self.text_budget.charge_cell(0, "ODT 병합 셀")?;
                    self.table.covered(&mut self.text_budget)?;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn start_paragraph(&mut self, name: &str, attrs: &Attrs<'_>) {
        let style_name = attr(attrs, "text:style-name");
        let style = style_name
            .as_deref()
            .and_then(|n| self.styles.get(n))
            .cloned()
            .unwrap_or_default();
        let toc = style_name.as_deref().and_then(toc_role_from_name);
        let outline = if name == "text:h" {
            attr(attrs, "text:outline-level")
                .as_deref()
                .and_then(|l| l.parse::<u8>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_OUTLINE_LEVEL)
        } else {
            0
        };
        let base = if outline > 0 {
            CharFmt::default()
        } else {
            style.fmt.clone()
        };
        self.span_fmts.clear();
        self.span_fmts.push(base.clone());
        self.paras.push(Para {
            runs: Vec::new(),
            style,
            outline,
            toc,
            bookmark: None,
            toc_dots: !style_name
                .as_deref()
                .is_some_and(|n| n.to_ascii_lowercase().contains("plain")),
            base,
            images: Vec::new(),
        });
    }

    fn start_span(&mut self, attrs: &Attrs<'_>) {
        let cur = self.span_fmts.last().cloned().unwrap_or_default();
        let add = attr(attrs, "text:style-name")
            .as_deref()
            .and_then(|n| self.styles.get(n))
            .map(|s| s.fmt.clone())
            .unwrap_or_default();
        self.span_fmts.push(merge_span_fmt(cur, add));
    }

    fn push_field_run(&mut self, field: RichField) -> Result<(), EasyOfficeError> {
        let fmt = self.span_fmts.last().cloned().unwrap_or_default();
        if let Some(top) = self.paras.last_mut() {
            self.text_budget.charge_run(0, "ODT 페이지 필드")?;
            top.runs.push(RichRun {
                bold: fmt.bold,
                italic: fmt.italic,
                underline: fmt.underline,
                strike: fmt.strike,
                size_half_pt: fmt.size_half_pt,
                font: fmt.font,
                color: fmt.color,
                highlight: fmt.highlight,
                script: fmt.script,
                field: Some(field),
                ..RichRun::default()
            });
        }
        Ok(())
    }

    fn push_inline(&mut self, text: &str) -> Result<(), EasyOfficeError> {
        if self.in_field || self.paras.is_empty() {
            return Ok(());
        }
        let link_len = self.link.as_deref().map_or(0, str::len);
        self.text_budget
            .charge_run(text.len().saturating_add(link_len), "ODT 런")?;
        self.notes
            .push_text(&mut self.paras, &self.span_fmts, self.link.as_deref(), text);
        Ok(())
    }

    fn warn_once(&mut self, note: &str) {
        if !self.warnings.iter().any(|w| w == note) {
            self.warnings.push(note.to_string());
        }
    }

    /// Reads a `draw:frame` (events span its start to its end): an embedded PNG/JPEG becomes a
    /// picture of the open paragraph or table cell; text boxes, charts and anything else are skipped.
    fn on_frame(
        &mut self,
        events: &[XmlEvent<'_>],
        entry_by_path: &HashMap<&str, &ZipArchiveEntry>,
    ) -> Result<(), EasyOfficeError> {
        let Some(frame) = read_frame(events) else {
            self.warn_once(UNSUPPORTED_OBJECT_NOTE);
            return Ok(());
        };
        if self.tbl_depth > 1
            || (self.tbl_depth == 1 && !self.table_state.in_cell)
            || (self.tbl_depth == 0 && self.paras.is_empty())
        {
            self.warn_once("표 안의 그림은 가져오지 않았습니다.");
            return Ok(());
        }
        let entry = safe_package_path(&frame.href)
            .and_then(|path| entry_by_path.get(path.as_str()).copied());
        let Some(entry) = entry else {
            self.warn_once("외부 링크이거나 찾을 수 없는 그림은 가져오지 않았습니다.");
            return Ok(());
        };
        self.budget.charge(entry.data.len())?;
        let mut image = match RichImage::new(entry.data.clone(), frame.alt.as_str()) {
            Ok(image) => image,
            Err(err) => {
                self.warnings.push(format!(
                    "그림 '{}'을 가져오지 않았습니다: {err}",
                    entry.path
                ));
                return Ok(());
            }
        };
        if let Some((w, h)) = frame.size_emu {
            // An out-of-range size keeps the default derived from the pixels.
            let _ = image.set_display_emu(w, h);
        }
        if self.tbl_depth == 1 {
            self.table_state.cell_images.push(image);
        } else if let Some(top) = self.paras.last_mut() {
            top.images.push(image);
        }
        Ok(())
    }

    fn on_end(&mut self, name: &str) -> Result<(), EasyOfficeError> {
        match name {
            "office:automatic-styles" | "office:font-face-decls" => {
                self.in_automatic_styles = false
            }
            "text:a" => self.link = None,
            "text:page-number" | "text:page-count" | "text:file-name" => self.in_field = false,
            "text:span" if !self.paras.is_empty() => {
                if self.span_fmts.len() > 1 {
                    self.span_fmts.pop();
                }
            }
            "text:list" => {
                self.list_stack.pop();
            }
            "text:section" => {
                self.sections.pop();
            }
            "text:p" | "text:h" if !self.in_automatic_styles => self.finish_paragraph()?,
            "table:table-cell" if self.tbl_depth == 1 => {
                let (text, spans) = join_cell_paragraphs(&self.cell_paras);
                self.cell_paras.clear();
                self.push_cell_with_links(text, &spans)?;
            }
            "table:table-row" if self.tbl_depth == 1 => self.table.end_row(),
            "table:table" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 && !self.table.is_empty() {
                    self.text_budget.charge_block(0, "ODT 표")?;
                    let (mut table, dropped) =
                        std::mem::take(&mut self.table).finish(&mut self.text_budget)?;
                    if let Some(widths) = self.table_state.resolve_col_widths(table.column_count())
                    {
                        table.col_widths_twips = widths;
                    }
                    self.dropped_merges += dropped;
                    self.blocks.push(RichBlock::Table(table));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Cells that a repeat count may still add to the open row.
    fn padding_room(&self) -> u32 {
        u32::try_from(MAX_PADDED_COLUMNS.saturating_sub(self.table.row_len())).unwrap_or(0)
    }

    /// Adds the closed cell (with any hyperlink spans, style, and images), repeated as declared.
    fn push_cell_with_links(
        &mut self,
        text: String,
        spans: &[(u32, u32, String)],
    ) -> Result<(), EasyOfficeError> {
        let (row_span, col_span, repeated) = self.cell_shape;
        let repeated = repeated.min(self.padding_room()).max(1);
        let (cell_style, cell_images) = self.table_state.finish_cell();
        self.text_budget.charge_cell(text.len(), "ODT 표 셀")?;
        let (r0, c0) = self.table.next_coord();
        if !spans.is_empty() {
            self.table.push_cell_links(r0, c0, spans);
        }
        if let Some((fill, borders)) = cell_style {
            self.table.push_cell_style(r0, c0, fill, borders);
        }
        if !cell_images.is_empty() {
            self.table.push_cell_images(r0, c0, cell_images.clone());
        }
        self.table
            .anchor(text.clone(), row_span, col_span, &mut self.text_budget)?;
        for _ in 1..repeated {
            self.text_budget.charge_cell(text.len(), "ODT 표 셀")?;
            let (rk, ck) = self.table.next_coord();
            if !spans.is_empty() {
                self.table.push_cell_links(rk, ck, spans);
            }
            if let Some((fill, borders)) = cell_style {
                self.table.push_cell_style(rk, ck, fill, borders);
            }
            if !cell_images.is_empty() {
                for img in &cell_images {
                    self.budget.charge(img.bytes().len())?;
                }
                self.table.push_cell_images(rk, ck, cell_images.clone());
            }
            self.table.cell(text.clone(), &mut self.text_budget)?;
        }
        Ok(())
    }

    fn finish_paragraph(&mut self) -> Result<(), EasyOfficeError> {
        let Some(mut p) = self.paras.pop() else {
            return Ok(());
        };
        let toc_page = match p.toc {
            Some(RichToc::Entry(_)) => take_toc_page(&mut p.runs),
            _ => None,
        };
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        let has_special_run = p.runs.iter().any(|r| {
            r.field.is_some()
                || r.note_ref.is_some()
                || r.comment_id.is_some()
                || r.revision.is_some()
        });
        let images = p.images;
        if self.tbl_depth > 0 {
            self.cell_paras.push(p.runs);
        } else if text.trim().is_empty() && !has_special_run {
            self.pending_page_break |= p.style.page_break_before;
        } else {
            self.text_budget.charge_block(0, "ODT 문단")?;
            let (list, task_checked, list_level, list_number) = if p.outline > 0 || p.toc.is_some()
            {
                (None, false, 0, None)
            } else if let Some(entry) = self.list_stack.last() {
                let lvl = (self.list_stack.len() - 1).min(8) as u8;
                let num = (entry.kind == RichListKind::Ordered)
                    .then(|| entry.levels[usize::from(lvl)])
                    .flatten()
                    .filter(|&n| n != crate::RichListNumber::default());
                (
                    Some(entry.kind),
                    entry.kind == RichListKind::Task && entry.checked,
                    lvl,
                    num,
                )
            } else if let Some(kind) = p.style.list_kind {
                (Some(kind), false, 0, None)
            } else if p.style.list_margin {
                (Some(RichListKind::Bullet), false, 0, None)
            } else {
                (None, false, 0, None)
            };
            self.blocks.push(RichBlock::Paragraph(RichParagraph {
                list_level,
                list_number,
                task_checked,
                runs: p.runs,
                alignment: p.style.align,
                heading_level: p.outline,
                list,
                left_indent_twips: p.style.left_twips,
                right_indent_twips: p.style.right_twips,
                first_line_indent_twips: p.style.first_line_twips,
                line_spacing_percent: p.style.line_percent,
                toc: p.toc,
                bookmark: p.bookmark,
                toc_page,
                toc_dots: toc_page.is_some() && p.toc_dots,
                page_break_before: p.style.page_break_before
                    | std::mem::take(&mut self.pending_page_break),
                columns: self.sections.last().copied().flatten(),
                ..RichParagraph::default()
            }));
        }
        if self.tbl_depth == 0 {
            self.blocks.extend(images.into_iter().map(RichBlock::Image));
        }
        self.span_fmts.clear();
        let base = self
            .paras
            .last()
            .map_or_else(CharFmt::default, |q| q.base.clone());
        self.span_fmts.push(base);
        Ok(())
    }
}

/// Decodes `content.xml` (and `styles.xml`) of an ODT package.
pub(crate) fn read_odt(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    // Every XML entry (styles.xml and content.xml included) is checked once, here.
    let parts = checked_zip_xml_entries_for(&entries, Some(DocumentFormat::Odt))?;
    let entry_by_path: HashMap<&str, &ZipArchiveEntry> =
        entries.iter().map(|e| (e.path.as_str(), e)).collect();
    if let Some(&m) = entry_by_path.get("mimetype") {
        if std::str::from_utf8(&m.data).unwrap_or_default().trim() != OdfCodec::MIMETYPE_ODT {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: crate::ContainerFamily::ZipOdf,
                detail: "ODF mimetype이 텍스트 문서(ODT)가 아닙니다.".to_string(),
            });
        }
    }
    let xml = checked_part(&parts, "content.xml").ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(
            "ODF 필수 엔트리 'content.xml'이 누락되었습니다.".to_string(),
        )
    })?;
    let mut styles: HashMap<String, StyleProps> = HashMap::new();
    let mut list_styles: HashMap<String, OdtListStyle> = HashMap::new();
    let mut table_styles = OdtTableStylesMap::default();
    if let Some(s_xml) = checked_part(&parts, "styles.xml") {
        read_styles_into(s_xml, &mut styles, &mut list_styles, &mut table_styles)?;
    }
    read_styles_into(xml, &mut styles, &mut list_styles, &mut table_styles)?;
    resolve_style_parents(&mut styles);

    let mut walker = OdtWalker::new(&styles, &list_styles, &table_styles);
    walker.notes.init_annotation_ends(xml)?;
    let mut reader = XmlReader::new(xml);
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start {
                name: "draw:frame", ..
            } => {
                let frame = reader.collect_subtree(event)?;
                walker.on_frame(&frame, &entry_by_path)?;
            }
            XmlEvent::Start {
                name: name @ ("text:tracked-changes" | "text:note" | "office:annotation"),
                ..
            } => {
                let sub = reader.collect_subtree(event)?;
                walker.notes.on_subtree(
                    name,
                    &sub,
                    &mut walker.paras,
                    walker.link.as_deref(),
                    &mut walker.text_budget,
                )?;
            }
            XmlEvent::Start { name, ref attrs } => {
                if walker.on_start(name, attrs)? {
                    reader.skip_subtree()?;
                }
            }
            XmlEvent::Text(text) => walker.push_inline(&text)?,
            XmlEvent::End(name) => walker.on_end(name)?,
        }
    }
    frame::finalize_odt_document(
        walker,
        checked_part(&parts, "styles.xml"),
        entry_by_path.get("meta.xml").map(|e| e.data.as_slice()),
    )
}

#[cfg(test)]
#[path = "rich_read_odt/tests.rs"]
mod tests;
