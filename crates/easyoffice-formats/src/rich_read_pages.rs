//! Pages reader producing a [`RichDocument`]: body text storage -> paragraphs and runs (character
//! styles, paragraph styles, list styles) with tables placed at their attachment positions.
//! Everything that depends on Apple's object schema lives in [`crate::pages_schema`]; table
//! cells are decoded by [`crate::pages_table`].
//!
//! Pictures (PNG/JPEG) are imported as image blocks (other picture data as a placeholder
//! paragraph), shape and text-box text at its anchor, and footnotes, headers and footers as
//! appendix paragraphs. Unsupported elements (comments, page-floating drawables, unreadable cell
//! values) are skipped and reported as warnings with kinds and counts; legacy
//! XML and encrypted packages are refused by [`crate::pages_package::load_package`].

mod notes;

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::pages_package::load_package;
use crate::pages_proto::{ArchiveObject, Message, ObjectTable};
use crate::pages_schema::{attachment, baseline, char_props_layout, compact_para, heading};
use crate::pages_schema::{list_type, para_props, root};
use crate::pages_schema::{storage, style, ty};
use crate::pages_schema::{CharPropFields, COLOR_RGBA};
use crate::pages_table::{read_table, TableBudget, TableOutcome};
use crate::rich_read_pages_image::{PageImage, PageImages, CROPPED, UNSUPPORTED_FORMAT};
use crate::{
    EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph, RichRun,
    RichScript,
};

const MAX_PARAGRAPHS: usize = 200_000;
const MAX_RUNS: usize = 1_000_000;
const MAX_STYLE_CHAIN: usize = 16;
const MAX_FONT_SIZE_PT: f32 = 1000.0;
const MAX_HEADING_LEVEL: u8 = 6;
const COLOR_CHANNEL_MAX: f32 = 255.0;
const MAX_FONT_NAME_BYTES: usize = 128;
const TWIPS_PER_POINT: f32 = 20.0;
const MAX_INDENT_PT: f32 = 1000.0;
const MAX_LINE_SPACING_MULTIPLE: f32 = 10.0;
const SINGLE_SPACING_PERCENT: u16 = 100;

fn corrupt(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("Pages 문서: {detail}"))
}

fn reference(m: &Message, field: u32) -> Option<u64> {
    m.sub(field).ok().flatten().and_then(|r| r.varint(1))
}

/// Attribute table `{ entries { index, value } }` sorted by index.
#[derive(Default)]
struct AttrTable {
    entries: Vec<(u64, Option<u64>)>,
}

impl AttrTable {
    /// Table whose entries carry an object reference (`entry.object`).
    fn parse(storage_msg: &Message, field: u32) -> Result<Self, EasyOfficeError> {
        Self::parse_with(storage_msg, field, |e| reference(e, storage::ENTRY_OBJECT))
    }

    /// Table whose entries carry a plain varint (`entry.level`).
    fn parse_values(storage_msg: &Message, field: u32) -> Result<Self, EasyOfficeError> {
        Self::parse_with(storage_msg, field, |e| e.varint(storage::ENTRY_LEVEL))
    }

    fn parse_with(
        storage_msg: &Message,
        field: u32,
        value_of: impl Fn(&Message) -> Option<u64>,
    ) -> Result<Self, EasyOfficeError> {
        let mut entries = Vec::new();
        if let Some(t) = storage_msg.sub(field)? {
            for e in t.subs(storage::TABLE_ENTRIES)? {
                let idx = e
                    .varint(storage::ENTRY_INDEX)
                    .ok_or_else(|| corrupt("속성 표 항목에 위치가 없습니다"))?;
                entries.push((idx, value_of(&e)));
            }
        }
        entries.sort_by_key(|(i, _)| *i);
        Ok(Self { entries })
    }

    /// Value in effect at `idx` (last entry at or before it; an entry without value resets).
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

    /// Indexes strictly inside `(start, end)` where the value changes.
    fn boundaries(&self, start: u64, end: u64) -> impl Iterator<Item = u64> + '_ {
        self.entries
            .iter()
            .map(|(i, _)| *i)
            .filter(move |i| *i > start && *i < end)
    }

    /// Number of entries that point at something.
    fn referenced(&self) -> usize {
        self.entries.iter().filter(|(_, v)| v.is_some()).count()
    }
}

/// Resolved format of a style chain (first definition along the chain wins).
#[derive(Default)]
struct Fmt {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    size_half_pt: Option<u16>,
    color: Option<String>,
    highlight: Option<String>,
    align: Option<u64>,
    identifier: Option<String>,
    font: Option<String>,
    script: Option<RichScript>,
    left_indent_twips: Option<u32>,
    first_line_indent_twips: Option<i32>,
    line_spacing_percent: Option<u16>,
}

fn color_hex(color: &Message) -> Option<String> {
    let mut rgb = [0u8; 3];
    for (slot, field) in rgb.iter_mut().zip(COLOR_RGBA) {
        let v = color.float(field)?;
        if !v.is_finite() {
            return None;
        }
        *slot = (v.clamp(0.0, 1.0) * COLOR_CHANNEL_MAX).round() as u8;
    }
    Some(format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
}

fn size_half_pt(props: &Message, field: u32) -> Option<u16> {
    props
        .float(field)
        .filter(|s| s.is_finite() && *s > 0.0 && *s < MAX_FONT_SIZE_PT)
        .map(|s| (s * 2.0).round() as u16)
}

fn script_of(value: u64) -> Option<RichScript> {
    match value {
        baseline::SUPERSCRIPT => Some(RichScript::Superscript),
        baseline::SUBSCRIPT => Some(RichScript::Subscript),
        _ => None,
    }
}

/// Points as twips (1/20 point); `None` for non-finite or out-of-range values.
fn points_to_twips(points: f32) -> Option<i32> {
    (points.is_finite() && points.abs() <= MAX_INDENT_PT)
        .then(|| (points * TWIPS_PER_POINT).round() as i32)
}

/// Line spacing as a percentage of single spacing, for the "multiple" mode only; 100% is the
/// default and reported as unset.
fn line_spacing_percent(p: &Message) -> Option<u16> {
    let spacing = p.sub(compact_para::LINE_SPACING).ok().flatten()?;
    if spacing
        .varint(compact_para::SPACING_MODE)
        .unwrap_or(compact_para::SPACING_MULTIPLE)
        != compact_para::SPACING_MULTIPLE
    {
        return None;
    }
    let amount = spacing.float(compact_para::SPACING_AMOUNT)?;
    if !amount.is_finite() || amount <= 0.0 || amount > MAX_LINE_SPACING_MULTIPLE {
        return None;
    }
    let percent = (amount * 100.0).round() as u16;
    (percent != SINGLE_SPACING_PERCENT).then_some(percent)
}

fn alignment_of(value: u64) -> Option<RichAlignment> {
    match value {
        para_props::ALIGN_LEFT => Some(RichAlignment::Left),
        para_props::ALIGN_RIGHT => Some(RichAlignment::Right),
        para_props::ALIGN_CENTER => Some(RichAlignment::Center),
        para_props::ALIGN_JUSTIFY => Some(RichAlignment::Justify),
        _ => None,
    }
}

impl Fmt {
    /// Fills the properties that are still unset from a character property message.
    fn merge_props(&mut self, p: &Message, l: &CharPropFields) {
        let flag = |n: u32| p.varint(n).map(|v| v != 0);
        self.bold = self.bold.or_else(|| flag(l.bold));
        self.italic = self.italic.or_else(|| flag(l.italic));
        self.underline = self.underline.or_else(|| flag(l.underline));
        self.strike = self.strike.or_else(|| flag(l.strike));
        if self.size_half_pt.is_none() {
            self.size_half_pt = size_half_pt(p, l.size_pt);
        }
        if self.color.is_none() {
            self.color = p.sub(l.color).ok().flatten().and_then(|c| color_hex(&c));
        }
        if self.highlight.is_none() {
            self.highlight = l
                .highlight
                .and_then(|n| p.sub(n).ok().flatten())
                .and_then(|c| color_hex(&c));
        }
        if self.font.is_none() {
            self.font = l
                .font
                .and_then(|n| p.string(n).ok().flatten())
                .filter(|f| !f.is_empty() && f.len() <= MAX_FONT_NAME_BYTES);
        }
        if self.script.is_none() {
            self.script = l.script.and_then(|n| p.varint(n)).and_then(script_of);
        }
    }

    /// Indents and line spacing of a compact paragraph property message.
    fn merge_para_layout(&mut self, p: &Message) {
        if self.left_indent_twips.is_none() {
            self.left_indent_twips = p
                .float(compact_para::LEFT_INDENT)
                .and_then(points_to_twips)
                .and_then(|t| u32::try_from(t).ok());
        }
        if self.first_line_indent_twips.is_none() {
            self.first_line_indent_twips = p
                .float(compact_para::FIRST_LINE_INDENT)
                .and_then(points_to_twips);
        }
        if self.line_spacing_percent.is_none() {
            self.line_spacing_percent = line_spacing_percent(p);
        }
    }

    /// Merges one style object (full or compact layout) into this format.
    fn merge_object(&mut self, obj: &ArchiveObject) {
        let Ok(m) = Message::parse(&obj.payload) else {
            return;
        };
        if self.identifier.is_none() {
            self.identifier = style_identifier(&m);
        }
        if let Some((props, layout)) = char_props_layout(&m) {
            self.merge_props(&props, layout);
        }
        if obj.type_id == ty::PARA_STYLE && self.align.is_none() {
            self.align = para_props_of(&m).and_then(|p| p.varint(para_props::ALIGNMENT));
        }
        if obj.type_id == ty::PARA_STYLE && matches!(m.sub(style::PARA_PROPS), Ok(None)) {
            if let Ok(Some(p)) = m.sub(style::COMPACT_PARA_PROPS) {
                self.merge_para_layout(&p);
            }
        }
    }
}

fn style_identifier(m: &Message) -> Option<String> {
    m.sub(style::HEADER)
        .ok()
        .flatten()
        .and_then(|h| h.string(style::HEADER_IDENTIFIER).ok().flatten())
        .filter(|s| !s.is_empty())
}

fn para_props_of<'a>(m: &Message<'a>) -> Option<Message<'a>> {
    match m.sub(style::PARA_PROPS) {
        Ok(Some(p)) => Some(p),
        _ => m.sub(style::COMPACT_PARA_PROPS).ok().flatten(),
    }
}

fn heading_level(identifier: &str) -> u8 {
    if let Some(pos) = identifier.find(heading::HEADING_PREFIX) {
        let rest = &identifier[pos + heading::HEADING_PREFIX.len()..];
        if let Some(d) = rest.chars().next().and_then(|c| c.to_digit(10)) {
            return (d as u8).clamp(1, MAX_HEADING_LEVEL);
        }
    }
    u8::from(identifier.contains(heading::TITLE_MARKER))
}

/// Attribute tables of one text storage.
struct StorageTables {
    para_styles: AttrTable,
    char_styles: AttrTable,
    list_styles: AttrTable,
    list_levels: AttrTable,
    attachments: AttrTable,
}

impl StorageTables {
    fn parse(st: &Message) -> Result<Self, EasyOfficeError> {
        Ok(Self {
            para_styles: AttrTable::parse(st, storage::PARA_STYLE_TABLE)?,
            char_styles: AttrTable::parse(st, storage::CHAR_STYLE_TABLE)?,
            list_styles: AttrTable::parse(st, storage::LIST_STYLE_TABLE)?,
            list_levels: AttrTable::parse_values(st, storage::LIST_LEVEL_TABLE)?,
            attachments: AttrTable::parse(st, storage::ATTACHMENT_TABLE)?,
        })
    }
}

/// Text and attachments found in one line of a storage.
#[derive(Default)]
struct LineContent {
    runs: Vec<RichRun>,
    attachments: Vec<u64>,
}

fn push_clean(cur: &mut String, ch: char) {
    match ch {
        '\u{2028}' => cur.push(' '),
        c if c.is_control() && c != '\t' => {}
        c => cur.push(c),
    }
}

fn pick<T>(c: Option<&Fmt>, p: Option<&Fmt>, field: impl Fn(&Fmt) -> Option<T>) -> Option<T> {
    c.and_then(&field).or_else(|| p.and_then(&field))
}

/// Run formatted by a character style layered over the paragraph style.
fn styled_run(text: String, c: Option<&Fmt>, p: Option<&Fmt>) -> RichRun {
    RichRun {
        text,
        bold: pick(c, p, |f| f.bold).unwrap_or(false),
        italic: pick(c, p, |f| f.italic).unwrap_or(false),
        underline: pick(c, p, |f| f.underline).unwrap_or(false),
        strike: pick(c, p, |f| f.strike).unwrap_or(false),
        size_half_pt: pick(c, p, |f| f.size_half_pt),
        color: pick(c, p, |f| f.color.clone()),
        highlight: pick(c, p, |f| f.highlight.clone()),
        // The paragraph style's font is the template default, so only a character style counts.
        font: c.and_then(|f| f.font.clone()),
        script: pick(c, p, |f| f.script).unwrap_or_default(),
    }
}

struct Reader<'a> {
    objects: &'a ObjectTable,
    skipped: BTreeMap<String, usize>,
    paragraphs: usize,
    runs: usize,
    tables: TableBudget,
    fmt_cache: HashMap<u64, Rc<Fmt>>,
    images: PageImages<'a>,
}

impl<'a> Reader<'a> {
    fn skip_many(&mut self, kind: impl Into<String>, count: usize) {
        if count > 0 {
            *self.skipped.entry(kind.into()).or_default() += count;
        }
    }

    fn skip(&mut self, kind: impl Into<String>) {
        self.skip_many(kind, 1);
    }

    fn object(&self, id: u64, type_id: u32) -> Option<&'a ArchiveObject> {
        self.objects.get(id).filter(|o| o.type_id == type_id)
    }

    fn style_fmt(&mut self, id: u64) -> Rc<Fmt> {
        if let Some(f) = self.fmt_cache.get(&id) {
            return Rc::clone(f);
        }
        let chain = self.objects.chain(id, MAX_STYLE_CHAIN, |o| {
            let m = Message::parse(&o.payload).ok()?;
            let header = m.sub(style::HEADER).ok().flatten()?;
            reference(&header, style::HEADER_PARENT)
        });
        let mut out = Fmt::default();
        for sid in chain {
            if let Some(obj) = self.objects.get(sid) {
                out.merge_object(obj);
            }
        }
        let out = Rc::new(out);
        self.fmt_cache.insert(id, Rc::clone(&out));
        out
    }

    /// List kind of a list style at a nesting level (levels beyond the style use its last one).
    fn list_kind(&self, list_style: u64, level: u64) -> Option<RichListKind> {
        let obj = self.object(list_style, ty::LIST_STYLE)?;
        let m = Message::parse(&obj.payload).ok()?;
        let types = m.repeated_varints(style::LIST_LEVEL_TYPES).ok()?;
        let at = usize::try_from(level)
            .ok()?
            .min(types.len().checked_sub(1)?);
        match types.get(at)? {
            &list_type::BULLET | &list_type::IMAGE => Some(RichListKind::Bullet),
            &list_type::NUMBER => Some(RichListKind::Ordered),
            _ => None,
        }
    }

    fn storage_message(&self, id: u64) -> Option<Message<'a>> {
        let obj = self.object(id, ty::STORAGE)?;
        Message::parse(&obj.payload).ok()
    }

    fn paragraph_header(
        &mut self,
        t: &StorageTables,
        start: u64,
    ) -> (RichParagraph, Option<Rc<Fmt>>) {
        let mut para = RichParagraph::default();
        let pfmt = t.para_styles.at(start).map(|id| self.style_fmt(id));
        if let Some(f) = &pfmt {
            para.alignment = f.align.and_then(alignment_of);
            para.heading_level = f.identifier.as_deref().map_or(0, heading_level);
            para.left_indent_twips = f.left_indent_twips;
            para.first_line_indent_twips = f.first_line_indent_twips;
            para.line_spacing_percent = f.line_spacing_percent;
        }
        let level = t.list_levels.at(start).unwrap_or(0);
        para.list = t
            .list_styles
            .at(start)
            .and_then(|id| self.list_kind(id, level));
        (para, pfmt)
    }

    fn flush_run(
        &mut self,
        content: &mut LineContent,
        cur: &mut String,
        char_style: Option<u64>,
        pfmt: Option<&Fmt>,
    ) -> Result<(), EasyOfficeError> {
        if cur.is_empty() {
            return Ok(());
        }
        self.runs += 1;
        if self.runs > MAX_RUNS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 글자 구간 수 한도 초과".to_string(),
            ));
        }
        let cfmt = char_style.map(|id| self.style_fmt(id));
        content
            .runs
            .push(styled_run(std::mem::take(cur), cfmt.as_deref(), pfmt));
        Ok(())
    }

    /// Splits one line at character-style boundaries into runs and collects its attachments.
    fn convert_line(
        &mut self,
        t: &StorageTables,
        line: &str,
        start: u64,
        pfmt: Option<&Fmt>,
    ) -> Result<LineContent, EasyOfficeError> {
        let end = start.saturating_add(line.encode_utf16().count() as u64);
        let mut cuts = t.char_styles.boundaries(start, end).peekable();
        let mut content = LineContent::default();
        let mut cur = String::new();
        let mut seg_start = start;
        let mut utf16 = start;
        for ch in line.chars() {
            while cuts.next_if(|c| *c <= utf16).is_some() {
                self.flush_run(&mut content, &mut cur, t.char_styles.at(seg_start), pfmt)?;
                seg_start = utf16;
            }
            if ch == '\u{FFFC}' {
                content.attachments.extend(t.attachments.exact(utf16));
            } else {
                push_clean(&mut cur, ch);
            }
            utf16 += ch.len_utf16() as u64;
        }
        self.flush_run(&mut content, &mut cur, t.char_styles.at(seg_start), pfmt)?;
        Ok(content)
    }

    fn convert_storage(
        &mut self,
        st: &Message,
        allow_tables: bool,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let text = st.text(storage::TEXT)?;
        let tables = StorageTables::parse(st)?;
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
            pos = start
                .saturating_add(line.encode_utf16().count() as u64)
                .saturating_add(1);
            let (mut para, pfmt) = self.paragraph_header(&tables, start);
            let content = self.convert_line(&tables, line, start, pfmt.as_deref())?;
            para.runs = content.runs;
            if !para.runs.is_empty() || content.attachments.is_empty() {
                blocks.push(RichBlock::Paragraph(para));
            }
            for att in content.attachments {
                self.place_attachment(att, allow_tables, &mut blocks)?;
            }
        }
        Ok(blocks)
    }

    fn place_attachment(
        &mut self,
        att: u64,
        allow_tables: bool,
        blocks: &mut Vec<RichBlock>,
    ) -> Result<(), EasyOfficeError> {
        if !allow_tables {
            self.skip("표 안의 개체");
            return Ok(());
        }
        blocks.extend(self.attachment_blocks(att)?);
        Ok(())
    }

    /// Resolves an attachment to table, picture or shape-text blocks, counting anything else as
    /// skipped.
    fn attachment_blocks(&mut self, att_id: u64) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let drawable = self
            .object(att_id, ty::ATTACHMENT)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| reference(&m, attachment::DRAWABLE));
        let Some(did) = drawable else {
            self.skip("알 수 없는 첨부 개체");
            return Ok(Vec::new());
        };
        match self.objects.get(did) {
            Some(o) if o.type_id == ty::TABLE_INFO => Ok(self
                .read_table(o)?
                .map(RichBlock::Table)
                .into_iter()
                .collect()),
            Some(o) if o.type_id == ty::IMAGE => Ok(self.read_picture(o)?.into_iter().collect()),
            Some(o) if o.type_id == ty::SHAPE_WITH_TEXT => self.shape_blocks(o),
            Some(o) => {
                self.skip(format!("{}(형식 {})", object_kind(o.type_id), o.type_id));
                Ok(Vec::new())
            }
            None => {
                self.skip("끊어진 개체 참조");
                Ok(Vec::new())
            }
        }
    }

    fn read_picture(&mut self, obj: &ArchiveObject) -> Result<Option<RichBlock>, EasyOfficeError> {
        match self.images.read(obj)? {
            PageImage::Picture { image, cropped } => {
                if cropped {
                    self.skip(CROPPED);
                }
                Ok(Some(RichBlock::Image(image)))
            }
            PageImage::Unsupported { text } => {
                self.skip(UNSUPPORTED_FORMAT);
                Ok(Some(RichBlock::Paragraph(RichParagraph {
                    runs: vec![RichRun::plain(text)],
                    ..RichParagraph::default()
                })))
            }
            PageImage::Skipped(why) => {
                self.skip(why);
                Ok(None)
            }
        }
    }

    fn read_table(
        &mut self,
        info: &ArchiveObject,
    ) -> Result<Option<Vec<Vec<String>>>, EasyOfficeError> {
        match read_table(self.objects, info, &mut self.tables)? {
            TableOutcome::Read(t) => {
                self.skip_many("표 셀의 읽을 수 없는 값(문자열·기간 등)", t.unsupported);
                Ok(Some(t.grid))
            }
            TableOutcome::Absent => Ok(None),
            TableOutcome::Skipped(why) => {
                self.skip(why);
                Ok(None)
            }
        }
    }

    /// Counts comments, which are not imported.
    fn note_unimported_text(&mut self, body: &Message) -> Result<(), EasyOfficeError> {
        let comments = AttrTable::parse(body, storage::COMMENT_TABLE)?.referenced();
        self.skip_many("댓글", comments);
        Ok(())
    }
}

/// Display name of a drawable object type that is not imported.
fn object_kind(type_id: u32) -> &'static str {
    match type_id {
        ty::IMAGE => "그림",
        _ => "그림·도형 등 개체",
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
        tables: TableBudget::default(),
        fmt_cache: HashMap::new(),
        images: PageImages::new(&pkg.objects, &pkg.data_files),
    };
    let body = reader
        .storage_message(body_id)
        .ok_or_else(|| corrupt("본문 저장소를 읽을 수 없습니다"))?;
    let mut blocks = reader.convert_storage(&body, true)?;
    reader.note_unimported_text(&body)?;
    reader.note_floating_drawables();
    while matches!(blocks.last(), Some(RichBlock::Paragraph(p)) if p.runs.is_empty()) {
        blocks.pop();
    }
    blocks.extend(reader.appendix_blocks(&body)?);
    let mut warnings: Vec<String> = reader
        .skipped
        .iter()
        .map(|(kind, n)| format!("가져오지 않은 요소: {kind} {n}개"))
        .collect();
    warnings.push(
        "Pages 문서는 읽기 전용으로 열고, 머리글·바닥글·각주는 본문 뒤 부록 문단으로 옮기며, 변경 추적·차트 등은 가져오지 않고 서식이 일부 단순화될 수 있습니다."
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
