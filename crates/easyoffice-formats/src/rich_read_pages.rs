//! Pages reader producing a [`RichDocument`]: body text storage -> paragraphs and runs (character
//! styles, paragraph styles, list styles) with tables placed at their attachment positions.
//! Everything that depends on Apple's object schema lives in [`crate::pages_schema`].
//!
//! Unsupported elements (images, shapes, non-rich-text cells, ...) are skipped and reported as
//! warnings with counts; legacy XML and encrypted packages are refused by
//! [`crate::pages_package::load_package`].

use std::collections::{BTreeMap, HashMap};

use crate::pages_package::load_package;
use crate::pages_proto::{ArchiveObject, Message, ObjectTable};
use crate::pages_schema::{attachment, cell, char_props, heading, list_type, para_props, root};
use crate::pages_schema::{storage, style, table, ty};
use crate::{
    EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph, RichRun,
};

const MAX_PARAGRAPHS: usize = 200_000;
const MAX_RUNS: usize = 1_000_000;
const MAX_TABLES: usize = 2_000;
const MAX_TABLE_ROWS: usize = 10_000;
const MAX_TABLE_COLS: usize = 256;
const MAX_TABLE_CELLS: usize = 2_000_000;
const MAX_TOTAL_CELLS: usize = 4_000_000;
const MAX_STYLE_CHAIN: usize = 16;
const MAX_LIST_ENTRIES: usize = 2_000_000;

fn corrupt(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("Pages 문서: {detail}"))
}

fn reference(m: &Message, field: u32) -> Option<u64> {
    m.sub(field).ok().flatten().and_then(|r| r.varint(1))
}

/// Attribute table `{ entries { index, object? } }` sorted by index.
#[derive(Default)]
struct AttrTable {
    entries: Vec<(u64, Option<u64>)>,
}

impl AttrTable {
    fn parse(storage_msg: &Message, field: u32) -> Result<Self, EasyOfficeError> {
        let mut entries = Vec::new();
        if let Some(t) = storage_msg.sub(field)? {
            for e in t.subs(storage::TABLE_ENTRIES)? {
                let idx = e
                    .varint(storage::ENTRY_INDEX)
                    .ok_or_else(|| corrupt("속성 표 항목에 위치가 없습니다"))?;
                entries.push((idx, reference(&e, storage::ENTRY_OBJECT)));
            }
        }
        entries.sort_by_key(|(i, _)| *i);
        Ok(Self { entries })
    }

    /// Object in effect at `idx` (last entry at or before it; an entry without object resets).
    fn at(&self, idx: u64) -> Option<u64> {
        let n = self.entries.partition_point(|(i, _)| *i <= idx);
        n.checked_sub(1).and_then(|k| self.entries[k].1)
    }

    fn exact(&self, idx: u64) -> Option<u64> {
        self.entries
            .binary_search_by_key(&idx, |(i, _)| *i)
            .ok()
            .and_then(|k| self.entries[k].1)
    }

    /// Indexes strictly inside `(start, end)` where the object changes.
    fn boundaries(&self, start: u64, end: u64) -> impl Iterator<Item = u64> + '_ {
        self.entries
            .iter()
            .map(|(i, _)| *i)
            .filter(move |i| *i > start && *i < end)
    }
}

/// Resolved format of a style chain (first definition along the chain wins).
#[derive(Default, Clone)]
struct Fmt {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    size_half_pt: Option<u16>,
    color: Option<String>,
    align: Option<u64>,
    identifier: Option<String>,
}

fn color_hex(color: &Message) -> Option<String> {
    let mut rgb = [0u8; 3];
    for (slot, field) in rgb.iter_mut().zip(char_props::COLOR_RGBA) {
        let v = color.float(field)?;
        if !v.is_finite() {
            return None;
        }
        *slot = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    Some(format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
}

struct Reader<'a> {
    objects: &'a ObjectTable,
    skipped: BTreeMap<String, usize>,
    paragraphs: usize,
    runs: usize,
    tables: usize,
    total_cells: usize,
    fmt_cache: HashMap<u64, Fmt>,
}

impl<'a> Reader<'a> {
    fn skip(&mut self, kind: impl Into<String>) {
        *self.skipped.entry(kind.into()).or_default() += 1;
    }

    fn object(&self, id: u64, type_id: u32) -> Option<&'a ArchiveObject> {
        self.objects.get(id).filter(|o| o.type_id == type_id)
    }

    fn style_fmt(&mut self, id: u64) -> Fmt {
        if let Some(f) = self.fmt_cache.get(&id) {
            return f.clone();
        }
        let chain = self.objects.chain(id, MAX_STYLE_CHAIN, |o| {
            let m = Message::parse(&o.payload).ok()?;
            let header = m.sub(style::HEADER).ok().flatten()?;
            reference(&header, style::HEADER_PARENT)
        });
        let mut out = Fmt::default();
        for sid in chain {
            let Some(obj) = self.objects.get(sid) else {
                continue;
            };
            let Ok(m) = Message::parse(&obj.payload) else {
                continue;
            };
            if out.identifier.is_none() {
                out.identifier = m
                    .sub(style::HEADER)
                    .ok()
                    .flatten()
                    .and_then(|h| h.string(style::HEADER_IDENTIFIER).ok().flatten())
                    .filter(|s| !s.is_empty());
            }
            if let Ok(Some(p)) = m.sub(style::CHAR_PROPS) {
                let flag = |n: u32| p.varint(n).map(|v| v != 0);
                out.bold = out.bold.or_else(|| flag(char_props::BOLD));
                out.italic = out.italic.or_else(|| flag(char_props::ITALIC));
                out.underline = out.underline.or_else(|| flag(char_props::UNDERLINE));
                out.strike = out.strike.or_else(|| flag(char_props::STRIKE));
                if out.size_half_pt.is_none() {
                    out.size_half_pt = p
                        .float(char_props::SIZE_PT)
                        .filter(|s| s.is_finite() && *s > 0.0 && *s < 1000.0)
                        .map(|s| (s * 2.0).round() as u16);
                }
                if out.color.is_none() {
                    out.color = p
                        .sub(char_props::COLOR)
                        .ok()
                        .flatten()
                        .and_then(|c| color_hex(&c));
                }
            }
            if obj.type_id == ty::PARA_STYLE && out.align.is_none() {
                out.align = m
                    .sub(style::PARA_PROPS)
                    .ok()
                    .flatten()
                    .and_then(|p| p.varint(para_props::ALIGNMENT));
            }
        }
        self.fmt_cache.insert(id, out.clone());
        out
    }

    fn list_kind(&self, list_style: u64) -> Option<RichListKind> {
        let obj = self.object(list_style, ty::LIST_STYLE)?;
        let m = Message::parse(&obj.payload).ok()?;
        match m.repeated_varints(style::LIST_LEVEL_TYPES).ok()?.first() {
            Some(&list_type::BULLET) | Some(&list_type::IMAGE) => Some(RichListKind::Bullet),
            Some(&list_type::NUMBER) => Some(RichListKind::Ordered),
            _ => None,
        }
    }

    fn storage_message(&self, id: u64) -> Option<Message<'a>> {
        let obj = self.object(id, ty::STORAGE)?;
        Message::parse(&obj.payload).ok()
    }

    fn storage_text(m: &Message) -> Result<String, EasyOfficeError> {
        let mut text = String::new();
        for part in m.all_bytes(storage::TEXT) {
            text.push_str(
                std::str::from_utf8(part)
                    .map_err(|_| EasyOfficeError::InvalidUtf8("Pages 본문 텍스트".to_string()))?,
            );
        }
        Ok(text)
    }

    fn heading_level(identifier: &str) -> u8 {
        if let Some(pos) = identifier.find(heading::HEADING_PREFIX) {
            let rest = &identifier[pos + heading::HEADING_PREFIX.len()..];
            if let Some(d) = rest.chars().next().and_then(|c| c.to_digit(10)) {
                return (d as u8).clamp(1, 6);
            }
        }
        u8::from(identifier.contains(heading::TITLE_MARKER))
    }

    fn convert_storage(
        &mut self,
        st: &Message,
        allow_tables: bool,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let text = Self::storage_text(st)?;
        let para_styles = AttrTable::parse(st, storage::PARA_STYLE_TABLE)?;
        let char_styles = AttrTable::parse(st, storage::CHAR_STYLE_TABLE)?;
        let list_styles = AttrTable::parse(st, storage::LIST_STYLE_TABLE)?;
        let attachments = AttrTable::parse(st, storage::ATTACHMENT_TABLE)?;
        let mut blocks = Vec::new();
        let mut pos: u64 = 0;
        for line in text.split('\n') {
            self.paragraphs += 1;
            if self.paragraphs > MAX_PARAGRAPHS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 문단 수 한도 초과".to_string(),
                ));
            }
            let start = pos;
            let end = start + line.encode_utf16().count() as u64;
            pos = end + 1;
            let pfmt = para_styles.at(start).map(|id| self.style_fmt(id));
            let mut para = RichParagraph::default();
            if let Some(f) = &pfmt {
                para.alignment = match f.align {
                    Some(para_props::ALIGN_LEFT) => Some(RichAlignment::Left),
                    Some(para_props::ALIGN_RIGHT) => Some(RichAlignment::Right),
                    Some(para_props::ALIGN_CENTER) => Some(RichAlignment::Center),
                    Some(para_props::ALIGN_JUSTIFY) => Some(RichAlignment::Justify),
                    _ => None,
                };
                para.heading_level = f.identifier.as_deref().map_or(0, Self::heading_level);
            }
            para.list = list_styles.at(start).and_then(|id| self.list_kind(id));

            let mut cuts: Vec<u64> = char_styles.boundaries(start, end).collect();
            cuts.dedup();
            let mut seg_start = start;
            let mut cur = String::new();
            let mut pending_tables: Vec<u64> = Vec::new();
            let mut utf16 = start;
            let mut next_cut = cuts.iter().copied().peekable();
            let flush = |this: &mut Self, cur: &mut String, from: u64, para: &mut RichParagraph| {
                if cur.is_empty() {
                    return Ok(());
                }
                this.runs += 1;
                if this.runs > MAX_RUNS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 글자 구간 수 한도 초과".to_string(),
                    ));
                }
                let mut eff = pfmt.clone().unwrap_or_default();
                if let Some(cid) = char_styles.at(from) {
                    let c = this.style_fmt(cid);
                    eff.bold = c.bold.or(eff.bold);
                    eff.italic = c.italic.or(eff.italic);
                    eff.underline = c.underline.or(eff.underline);
                    eff.strike = c.strike.or(eff.strike);
                    eff.size_half_pt = c.size_half_pt.or(eff.size_half_pt);
                    eff.color = c.color.or(eff.color);
                }
                para.runs.push(RichRun {
                    text: std::mem::take(cur),
                    bold: eff.bold.unwrap_or(false),
                    italic: eff.italic.unwrap_or(false),
                    underline: eff.underline.unwrap_or(false),
                    strike: eff.strike.unwrap_or(false),
                    size_half_pt: eff.size_half_pt,
                    color: eff.color,
                    ..RichRun::default()
                });
                Ok(())
            };
            for ch in line.chars() {
                while next_cut.peek().is_some_and(|c| *c <= utf16) {
                    next_cut.next();
                    flush(self, &mut cur, seg_start, &mut para)?;
                    seg_start = utf16;
                }
                match ch {
                    '\u{FFFC}' => {
                        if let Some(att) = attachments.exact(utf16) {
                            pending_tables.push(att);
                        }
                    }
                    '\u{2028}' => cur.push(' '),
                    c if c.is_control() && c != '\t' => {}
                    c => cur.push(c),
                }
                utf16 += ch.len_utf16() as u64;
            }
            flush(self, &mut cur, seg_start, &mut para)?;
            if !para.runs.is_empty() || pending_tables.is_empty() {
                blocks.push(RichBlock::Paragraph(para));
            }
            for att in pending_tables {
                if allow_tables {
                    if let Some(rows) = self.attachment_table(att)? {
                        blocks.push(RichBlock::Table(rows));
                    }
                } else {
                    self.skip("표 안의 개체");
                }
            }
        }
        Ok(blocks)
    }

    /// Resolves an attachment to a table, counting anything else as skipped.
    fn attachment_table(
        &mut self,
        att_id: u64,
    ) -> Result<Option<Vec<Vec<String>>>, EasyOfficeError> {
        let drawable = self
            .object(att_id, ty::ATTACHMENT)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| reference(&m, attachment::DRAWABLE));
        let Some(did) = drawable else {
            self.skip("알 수 없는 첨부 개체");
            return Ok(None);
        };
        match self.objects.get(did) {
            Some(o) if o.type_id == ty::TABLE_INFO => self.read_table(o),
            Some(o) => {
                self.skip(format!("그림·도형 등 개체(형식 {})", o.type_id));
                Ok(None)
            }
            None => {
                self.skip("끊어진 개체 참조");
                Ok(None)
            }
        }
    }

    fn list_entries(&self, list_id: u64, want_type: u64) -> HashMap<u64, u64> {
        let mut out = HashMap::new();
        let Some(obj) = self.object(list_id, ty::TABLE_DATA_LIST) else {
            return out;
        };
        let Ok(m) = Message::parse(&obj.payload) else {
            return out;
        };
        if m.varint(table::LIST_TYPE) != Some(want_type) {
            return out;
        }
        let Ok(entries) = m.subs(table::LIST_ENTRIES) else {
            return out;
        };
        for e in entries.into_iter().take(MAX_LIST_ENTRIES) {
            if let (Some(key), Some(payload)) = (
                e.varint(table::ENTRY_KEY),
                e.sub(table::ENTRY_RICH_TEXT)
                    .ok()
                    .flatten()
                    .and_then(|r| r.varint(1)),
            ) {
                out.insert(key, payload);
            }
        }
        out
    }

    fn cell_text(&mut self, payload_id: u64) -> String {
        let storage = self
            .object(payload_id, ty::RICH_TEXT_PAYLOAD)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| reference(&m, table::PAYLOAD_STORAGE))
            .and_then(|sid| self.storage_message(sid));
        let Some(st) = storage else {
            return String::new();
        };
        let Ok(text) = Self::storage_text(&st) else {
            return String::new();
        };
        let cleaned: String = text
            .chars()
            .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
            .filter(|c| *c != '\u{FFFC}')
            .map(|c| if c == '\u{2028}' { ' ' } else { c })
            .collect();
        cleaned.trim_end_matches('\n').to_string()
    }

    /// Key of a rich-text cell in the version-5 packed layout.
    fn rich_text_key(buf: &[u8], offset: usize) -> Option<u64> {
        let head = buf.get(offset..offset.checked_add(cell::FIELDS_OFFSET)?)?;
        if head[0] != cell::VERSION || head[1] != cell::TYPE_RICH_TEXT {
            return None;
        }
        let mut f = [0u8; 4];
        f.copy_from_slice(&head[cell::FLAGS_OFFSET..cell::FLAGS_OFFSET + 4]);
        let flags = u32::from_le_bytes(f);
        if flags & cell::FLAG_RICH_TEXT_KEY == 0 {
            return None;
        }
        let below = (flags & (cell::FLAG_RICH_TEXT_KEY - 1)).count_ones() as usize;
        let at = offset
            .checked_add(cell::FIELDS_OFFSET)?
            .checked_add(below.checked_mul(4)?)?;
        let raw = buf.get(at..at.checked_add(4)?)?;
        Some(u64::from(u32::from_le_bytes([
            raw[0], raw[1], raw[2], raw[3],
        ])))
    }

    fn read_table(
        &mut self,
        info: &ArchiveObject,
    ) -> Result<Option<Vec<Vec<String>>>, EasyOfficeError> {
        self.tables += 1;
        if self.tables > MAX_TABLES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 수 한도 초과".to_string(),
            ));
        }
        let info_m = Message::parse(&info.payload)?;
        let Some(model) =
            reference(&info_m, table::INFO_MODEL).and_then(|id| self.object(id, ty::TABLE_MODEL))
        else {
            self.skip("읽을 수 없는 표");
            return Ok(None);
        };
        let m = Message::parse(&model.payload)?;
        let rows = m.varint(table::MODEL_ROWS).unwrap_or(0) as usize;
        let cols = m.varint(table::MODEL_COLS).unwrap_or(0) as usize;
        if rows == 0 || cols == 0 {
            return Ok(None);
        }
        if rows > MAX_TABLE_ROWS
            || cols > MAX_TABLE_COLS
            || rows.saturating_mul(cols) > MAX_TABLE_CELLS
        {
            self.skip("너무 큰 표");
            return Ok(None);
        }
        self.total_cells = self.total_cells.saturating_add(rows * cols);
        if self.total_cells > MAX_TOTAL_CELLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 표 셀 총수 한도 초과".to_string(),
            ));
        }
        let Some(store) = m.sub(table::MODEL_DATA_STORE)? else {
            self.skip("읽을 수 없는 표");
            return Ok(None);
        };
        let rich = reference(&store, table::STORE_RICH_TEXT_LIST)
            .map(|id| self.list_entries(id, table::LIST_TYPE_RICH_TEXT))
            .unwrap_or_default();
        let mut grid = vec![vec![String::new(); cols]; rows];
        let mut unsupported = 0usize;
        if let Some(tiles) = store.sub(table::STORE_TILES)? {
            let tile_rows = tiles
                .varint(table::TILES_SIZE)
                .filter(|v| *v > 0 && *v <= 65_536)
                .unwrap_or(table::DEFAULT_TILE_ROWS);
            for t in tiles.subs(table::TILES_ENTRIES)? {
                let tile_id = t.varint(table::TILE_ID).unwrap_or(0);
                let Some(tile_obj) =
                    reference(&t, table::TILE_REF).and_then(|id| self.object(id, ty::TABLE_TILE))
                else {
                    continue;
                };
                let tile = Message::parse(&tile_obj.payload)?;
                for row in tile.subs(table::TILE_ROWS)? {
                    let Some(ri) = row.varint(table::ROW_INDEX) else {
                        continue;
                    };
                    let abs = tile_id.saturating_mul(tile_rows).saturating_add(ri);
                    let Ok(r) = usize::try_from(abs) else {
                        continue;
                    };
                    if r >= rows {
                        continue;
                    }
                    let buf = row.bytes(table::ROW_CELL_BUFFER).unwrap_or(&[]);
                    let offs = row.bytes(table::ROW_CELL_OFFSETS).unwrap_or(&[]);
                    for (c, pair) in offs.chunks_exact(2).take(cols).enumerate() {
                        let off = u16::from_le_bytes([pair[0], pair[1]]);
                        if off == table::NO_CELL_OFFSET {
                            continue;
                        }
                        match Self::rich_text_key(buf, usize::from(off))
                            .and_then(|k| rich.get(&k).copied())
                        {
                            Some(payload) => grid[r][c] = self.cell_text(payload),
                            None => {
                                if buf
                                    .get(usize::from(off) + 1)
                                    .is_some_and(|t| *t != 0 && *t != cell::TYPE_RICH_TEXT)
                                {
                                    unsupported += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        for _ in 0..unsupported {
            self.skip("텍스트가 아닌 표 셀(숫자·날짜 등) 값");
        }
        Ok(Some(grid))
    }
}

/// Reads a Pages package into a rich document plus warnings.
pub(crate) fn read_pages(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let pkg = load_package(bytes)?;
    let root_obj = pkg
        .objects
        .objects
        .values()
        .find(|o| o.type_id == ty::DOCUMENT_ROOT)
        .ok_or_else(|| corrupt("문서 루트 객체가 없습니다"))?;
    let root_msg = Message::parse(&root_obj.payload)?;
    let body_id = reference(&root_msg, root::BODY_STORAGE)
        .ok_or_else(|| corrupt("본문 저장소 참조가 없습니다"))?;
    let mut reader = Reader {
        objects: &pkg.objects,
        skipped: BTreeMap::new(),
        paragraphs: 0,
        runs: 0,
        tables: 0,
        total_cells: 0,
        fmt_cache: HashMap::new(),
    };
    let body = reader
        .storage_message(body_id)
        .ok_or_else(|| corrupt("본문 저장소를 읽을 수 없습니다"))?;
    let mut blocks = reader.convert_storage(&body, true)?;
    while matches!(blocks.last(), Some(RichBlock::Paragraph(p)) if p.runs.is_empty()) {
        blocks.pop();
    }
    let mut warnings = Vec::new();
    for (kind, n) in &reader.skipped {
        warnings.push(format!("가져오지 않은 요소: {kind} {n}개"));
    }
    warnings.push(
        "Pages 문서는 읽기 전용으로 열며, 머리글·바닥글·각주·변경 추적·차트 등은 가져오지 않고 서식이 일부 단순화될 수 있습니다."
            .to_string(),
    );
    Ok((
        RichDocument {
            title: String::new(),
            blocks,
            official_agency: None,
        },
        warnings,
    ))
}
