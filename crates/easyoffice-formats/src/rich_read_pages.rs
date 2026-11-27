//! Pages reader producing a [`RichDocument`]: body text storage -> paragraphs and runs (character
//! styles, paragraph styles, list styles) with tables placed at their attachment positions.
//! Everything that depends on the format's object schema lives in [`crate::pages_schema`]; table
//! cells are decoded by [`crate::pages_table`].
//!
//! Pictures (PNG/JPEG) are imported as image blocks (other picture data is reported, not
//! mixed into the text), shape and text-box text at its anchor, and footnotes, headers and footers as
//! appendix paragraphs. Unsupported elements (comments, page-floating drawables, unreadable cell
//! values) are skipped and reported as warnings with kinds and counts; legacy
//! XML and encrypted packages are refused by [`crate::pages_package::load_package`].

mod changes;
mod charts;
pub(crate) mod equations;
mod floating;
mod marks;
mod notes;
mod numbering;
mod para_style;
mod sections;

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use self::marks::FootnoteMark;
use self::numbering::DocumentSettings;
use self::para_style::{alignment_of, heading_level, push_clean, styled_run, Fmt};
use crate::decode_budget::DecodeBudget;
use crate::pages_package::load_package;
use crate::pages_proto::{read_reference, ArchiveObject, FastU64Map, Message, ObjectTable};
use crate::pages_schema::{attachment, list_type, root, storage, style, ty};
use crate::pages_table::{read_table, TableBudget, TableOutcome};
use crate::rich_read_pages_image::{PageImage, PageImages, CROPPED};
use crate::{
    EasyOfficeError, RichBlock, RichDocument, RichListKind, RichParagraph, RichRun, RichScript,
    RichTable,
};

const MAX_PARAGRAPHS: usize = 200_000;
const MAX_RUNS: usize = 1_000_000;
/// Text the reader may convert in all: a storage that many shapes or footnotes point at is copied once
/// per reference, so the paragraph and run counts alone do not bound what it copies.
const MAX_CONVERTED_TEXT_BYTES: usize = 64 * 1024 * 1024;
const MAX_STYLE_CHAIN: usize = 16;
const MAX_FMT_CACHE: usize = 65_536;
/// Deepest list level kept (Pages has nine levels).
const MAX_LIST_LEVEL: u64 = 8;
const TWIPS_PER_POINT: f32 = 20.0;

fn corrupt(detail: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("Pages 문서: {detail}"))
}

fn reference(m: &Message, field: u32) -> Option<u64> {
    m.bytes(field).and_then(read_reference)
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
            t.for_each_sub(storage::TABLE_ENTRIES, |e| {
                let idx = e
                    .varint(storage::ENTRY_INDEX)
                    .ok_or_else(|| corrupt("속성 표 항목에 위치가 없습니다"))?;
                entries.push((idx, value_of(&e)));
                Ok(())
            })?;
        }
        entries.sort_by_key(|(i, _)| *i);
        Ok(Self { entries })
    }

    /// Table from `(index, value)` entries in any order.
    fn from_entries(mut entries: Vec<(u64, Option<u64>)>) -> Self {
        entries.sort_by_key(|(i, _)| *i);
        Self { entries }
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

    /// Indexes strictly greater than `start` where the value changes, found via O(log N) binary search.
    fn boundaries_after(&self, start: u64) -> impl Iterator<Item = u64> + '_ {
        let first = self.entries.partition_point(|(i, _)| *i <= start);
        self.entries[first..].iter().map(|(i, _)| *i)
    }
}

/// Attribute tables of one text storage.
struct StorageTables {
    para_styles: AttrTable,
    char_styles: AttrTable,
    list_styles: AttrTable,
    list_levels: AttrTable,
    layout_styles: AttrTable,
    attachments: AttrTable,
    /// Hyperlink runs (the value is the hyperlink object, `None` outside a link).
    links: AttrTable,
    /// Merged UTF-16 `[start, end)` intervals of tracked deletions to omit from body text.
    deleted_ranges: Vec<(u64, u64)>,
    /// Footnote marks in text order; only the main flow reads them, so the number shown at a
    /// mark equals the footnote's number in the appendix.
    footnote_marks: Vec<FootnoteMark>,
}

impl StorageTables {
    fn parse(
        st: &Message,
        footnote_marks: Vec<FootnoteMark>,
        links: AttrTable,
        deleted_ranges: Vec<(u64, u64)>,
    ) -> Result<Self, EasyOfficeError> {
        Ok(Self {
            footnote_marks,
            links,
            deleted_ranges,
            para_styles: AttrTable::parse(st, storage::PARA_STYLE_TABLE)?,
            char_styles: AttrTable::parse(st, storage::CHAR_STYLE_TABLE)?,
            list_styles: AttrTable::parse(st, storage::LIST_STYLE_TABLE)?,
            list_levels: AttrTable::parse_values(st, storage::LIST_LEVEL_TABLE)?,
            layout_styles: AttrTable::parse(st, storage::LAYOUT_STYLE_TABLE)?,
            attachments: AttrTable::parse(st, storage::ATTACHMENT_TABLE)?,
        })
    }
}

/// Characters that end a paragraph in a text storage: line feed, carriage return, page or column
/// break, layout change mid-storage and section break.
const fn ends_paragraph(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{c}' | '\u{4}' | '\u{5}')
}

const PAGE_BREAK: char = '\u{c}';
const SECTION_BREAK: char = '\u{4}';
/// Warning kinds for breaks that the rich model cannot hold.
const PAGE_BREAKS: &str = "쪽·단 나누기";
const SECTION_BREAKS: &str = "구역 나누기";

/// Where a storage sits: the main flow takes every drawable; text inside a shape, footnote,
/// header or footer takes pictures only (tables and shapes cannot nest in the rich model).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Body,
    Nested,
}

/// Note for page-layout documents, whose content is not a flowing body.
const PAGE_LAYOUT_NOTE: &str =
    "쪽 배치 문서입니다. 본문 대신 쪽 위의 글상자·그림·표를 쪽 순서와 위·왼쪽 위치 순서로 가져왔습니다.";

/// Warning kind for shapes and text boxes that hold no text.
const EMPTY_SHAPE: &str = "글이 없는 도형";

/// Warning kind for tables and shapes found inside nested text.
const NESTED_UNSUPPORTED: &str = "도형·각주·머리말·꼬리말 안의 표·도형";

/// Text and attachments found in one line of a storage.
#[derive(Default)]
struct LineContent {
    runs: Vec<RichRun>,
    attachments: Vec<u64>,
}

struct Reader<'a> {
    objects: &'a ObjectTable,
    skipped: BTreeMap<String, usize>,
    paragraphs: usize,
    runs: usize,
    tables: TableBudget,
    fmt_cache: FastU64Map<Rc<Fmt>>,
    /// Nested storages already converted, by storage id and the `quiet` state they were read in, which
    /// decides what is reported as skipped (see [`notes::NestedRead`]).
    nested_reads: HashMap<(u64, bool), notes::NestedRead>,
    images: PageImages<'a>,
    /// Marks such as page-number fields in headers and footers are not reported as skipped.
    quiet: bool,
    settings: DocumentSettings,
    text_budget: DecodeBudget,
    body_width_pt: f32,
    body_height_pt: f32,
}

impl<'a> Reader<'a> {
    /// Counts skipped drawables unless marks are being ignored (see [`Reader::quiet`]).
    fn skip_many(&mut self, kind: impl Into<String>, count: usize) {
        if !self.quiet {
            self.report_many(kind, count);
        }
    }

    /// Counts skipped content that must always be reported (pictures).
    fn report_many(&mut self, kind: impl Into<String>, count: usize) {
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
        if self.fmt_cache.len() < MAX_FMT_CACHE {
            self.fmt_cache.insert(id, Rc::clone(&out));
        }
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
        flow: Flow,
    ) -> (RichParagraph, Option<Rc<Fmt>>) {
        let mut para = RichParagraph::default();
        if flow == Flow::Body {
            para.columns = t
                .layout_styles
                .at(start)
                .and_then(|id| self.resolve_columns(id));
        }
        let pfmt = t.para_styles.at(start).map(|id| self.style_fmt(id));
        if let Some(f) = &pfmt {
            para.alignment = f.align.and_then(alignment_of);
            para.heading_level = f.identifier.as_deref().map_or(0, heading_level);
            para.left_indent_twips = f.left_indent_twips;
            para.right_indent_twips = f.right_indent_twips;
            para.first_line_indent_twips = f.first_line_indent_twips;
            para.line_spacing = f.line_spacing;
            para.space_before_twips = f.space_before_twips;
            para.space_after_twips = f.space_after_twips;
        }
        let level = t.list_levels.at(start).unwrap_or(0);
        if let Some(style_id) = t.list_styles.at(start) {
            para.list = self.list_kind(style_id, level);
            if let Some(kind) = para.list {
                para.list_level = u8::try_from(level.min(MAX_LIST_LEVEL)).unwrap_or(0);
                if kind == RichListKind::Ordered {
                    para.list_number = self.list_number_of(style_id, level);
                }
            }
        }
        (para, pfmt)
    }

    fn count_run(&mut self) -> Result<(), EasyOfficeError> {
        self.runs += 1;
        if self.runs > MAX_RUNS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 글자 구간 수 한도 초과".to_string(),
            ));
        }
        Ok(())
    }

    /// Superscript footnote number run (shown at the mark and in front of the appendix text).
    fn footnote_number_run(&mut self, number: usize) -> Result<RichRun, EasyOfficeError> {
        self.count_run()?;
        Ok(RichRun {
            script: RichScript::Superscript,
            ..RichRun::plain(self.note_label(number))
        })
    }

    /// Closes the run collected in `cur`: formatted by the character style and carrying the
    /// hyperlink in effect at `seg_start`.
    fn flush_run(
        &mut self,
        content: &mut LineContent,
        cur: &mut String,
        t: &StorageTables,
        seg_start: u64,
        pfmt: Option<&Fmt>,
    ) -> Result<(), EasyOfficeError> {
        if cur.is_empty() {
            return Ok(());
        }
        self.count_run()?;
        let cfmt = t.char_styles.at(seg_start).map(|id| self.style_fmt(id));
        let mut run = styled_run(std::mem::take(cur), cfmt.as_deref(), pfmt);
        run.link = t.links.at(seg_start).and_then(|id| self.link_url(id));
        if let Some(link) = &run.link {
            self.text_budget.charge(link.len(), "Pages 하이퍼링크")?;
        }
        content.runs.push(run);
        Ok(())
    }

    /// Splits one line at character-style, hyperlink and deletion boundaries into runs and
    /// collects its non-inline attachments in a single UTF-16 pass.
    fn convert_line(
        &mut self,
        t: &StorageTables,
        line: &str,
        start: u64,
        pfmt: Option<&Fmt>,
    ) -> Result<(LineContent, u64, bool), EasyOfficeError> {
        let mut style_cuts = t.char_styles.boundaries_after(start).peekable();
        let mut link_cuts = t.links.boundaries_after(start).peekable();
        let mut fn_cursor = t.footnote_marks.partition_point(|m| m.index < start);
        let mut del_cursor = t.deleted_ranges.partition_point(|&(_, end)| end <= start);
        let mut content = LineContent::default();
        let mut cur = String::new();
        let mut seg_start = start;
        let mut utf16 = start;
        let mut all_deleted = !line.is_empty();
        for ch in line.chars() {
            while del_cursor < t.deleted_ranges.len() && t.deleted_ranges[del_cursor].1 <= utf16 {
                del_cursor += 1;
            }
            let deleted = del_cursor < t.deleted_ranges.len()
                && t.deleted_ranges[del_cursor].0 <= utf16
                && utf16 < t.deleted_ranges[del_cursor].1;
            if deleted {
                self.flush_run(&mut content, &mut cur, t, seg_start, pfmt)?;
                while style_cuts.next_if(|c| *c <= utf16).is_some() {}
                while link_cuts.next_if(|c| *c <= utf16).is_some() {}
                utf16 += ch.len_utf16() as u64;
                seg_start = utf16;
                continue;
            }
            all_deleted = false;
            let mut cut = false;
            while style_cuts.next_if(|c| *c <= utf16).is_some() {
                cut = true;
            }
            while link_cuts.next_if(|c| *c <= utf16).is_some() {
                cut = true;
            }
            if cut {
                self.flush_run(&mut content, &mut cur, t, seg_start, pfmt)?;
                seg_start = utf16;
            }
            if ch == '\u{FFFC}' || ch == storage::FOOTNOTE_MARK_CHAR {
                if ch == '\u{FFFC}' {
                    if let Some(att_id) = t.attachments.exact(utf16) {
                        if let Some(eq_text) = self.inline_equation_at_attachment(att_id)? {
                            cur.push_str(&eq_text);
                        } else {
                            content.attachments.push(att_id);
                        }
                    }
                }
                while fn_cursor < t.footnote_marks.len()
                    && t.footnote_marks[fn_cursor].index < utf16
                {
                    fn_cursor += 1;
                }
                if fn_cursor < t.footnote_marks.len() && t.footnote_marks[fn_cursor].index == utf16
                {
                    let number = t.footnote_marks[fn_cursor].number;
                    fn_cursor += 1;
                    self.flush_run(&mut content, &mut cur, t, seg_start, pfmt)?;
                    seg_start = utf16 + 1;
                    content.runs.push(self.footnote_number_run(number)?);
                }
            } else {
                push_clean(&mut cur, ch);
            }
            utf16 += ch.len_utf16() as u64;
        }
        self.flush_run(&mut content, &mut cur, t, seg_start, pfmt)?;
        Ok((content, utf16 - start, all_deleted))
    }

    fn convert_storage(
        &mut self,
        st: &Message,
        flow: Flow,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let text = st.text(storage::TEXT)?;
        self.text_budget.charge(text.len(), "Pages 본문 텍스트")?;
        let deleted_ranges = if flow == Flow::Body {
            self.deleted_ranges(st, &text)?
        } else {
            Vec::new()
        };
        let marks = if flow == Flow::Body {
            self.footnote_marks(st, &deleted_ranges)?
        } else {
            Vec::new()
        };
        let links = self.link_table(st)?;
        let tables = StorageTables::parse(st, marks, links, deleted_ranges)?;
        self.note_marks(st, flow)?;
        let mut blocks = Vec::new();
        let mut pos: u64 = 0;
        let (mut page_breaks, mut section_breaks) = (0usize, 0usize);
        let mut offset = 0usize;
        for line in text.split(ends_paragraph) {
            let term = text[offset + line.len()..].chars().next();
            match term {
                Some(PAGE_BREAK) => page_breaks += 1,
                Some(SECTION_BREAK) => section_breaks += 1,
                _ => {}
            }
            offset += line.len() + term.map_or(0, char::len_utf8);
            self.paragraphs += 1;
            if self.paragraphs > MAX_PARAGRAPHS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 문단 수 한도 초과".to_string(),
                ));
            }
            let start = pos;
            let (mut para, pfmt) = self.paragraph_header(&tables, start, flow);
            let (content, len_u16, all_deleted) =
                self.convert_line(&tables, line, start, pfmt.as_deref())?;
            pos = start.saturating_add(len_u16).saturating_add(1);
            para.runs = content.runs;
            if !all_deleted && (!para.runs.is_empty() || content.attachments.is_empty()) {
                blocks.push(RichBlock::Paragraph(para));
            }
            for att in content.attachments {
                blocks.extend(self.attachment_blocks(att, flow)?);
            }
        }
        self.skip_many(PAGE_BREAKS, page_breaks);
        self.skip_many(SECTION_BREAKS, section_breaks);
        Ok(blocks)
    }

    /// Resolves an attachment to table, chart, equation, picture or shape-text blocks, counting
    /// anything else as skipped. Nested text takes pictures and equations only.
    fn attachment_blocks(
        &mut self,
        att_id: u64,
        flow: Flow,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        if self
            .objects
            .get(att_id)
            .is_some_and(|o| o.type_id == ty::NUMBER_ATTACHMENT)
        {
            self.skip("쪽 번호·쪽 수 필드");
            return Ok(Vec::new());
        }
        if let Some(eq_obj) = self.object(att_id, ty::EQUATION) {
            return self.read_standalone_equation(eq_obj, false);
        }
        let drawable = self
            .object(att_id, ty::ATTACHMENT)
            .and_then(|o| Message::parse(&o.payload).ok())
            .and_then(|m| reference(&m, attachment::DRAWABLE));
        let Some(did) = drawable else {
            self.skip("알 수 없는 첨부 개체");
            return Ok(Vec::new());
        };
        match self.objects.get(did) {
            Some(o)
                if flow == Flow::Nested
                    && matches!(
                        o.type_id,
                        ty::TABLE_INFO | ty::SHAPE_WITH_TEXT | ty::CHART | ty::CHART_LEGACY
                    ) =>
            {
                self.skip(NESTED_UNSUPPORTED);
                Ok(Vec::new())
            }
            Some(o) if o.type_id == ty::TABLE_INFO => self.read_table_blocks(o),
            Some(o) if o.type_id == ty::IMAGE => self.read_picture_or_equation(o, false),
            Some(o) if o.type_id == ty::EQUATION => self.read_standalone_equation(o, false),
            Some(o) if matches!(o.type_id, ty::CHART | ty::CHART_LEGACY) => {
                self.read_chart(o, false)
            }
            Some(o) if o.type_id == ty::SHAPE_WITH_TEXT => {
                let blocks = self.shape_blocks(o)?;
                if blocks.is_empty() {
                    self.skip(EMPTY_SHAPE);
                }
                Ok(blocks)
            }
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
                    self.report_many(CROPPED, 1);
                }
                Ok(Some(RichBlock::Image(image)))
            }
            PageImage::Skipped(why) => {
                self.report_many(why, 1);
                Ok(None)
            }
        }
    }

    fn read_table_blocks(
        &mut self,
        info: &ArchiveObject,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        match read_table(
            self.objects,
            info,
            &mut self.tables,
            &mut self.images,
            &mut self.text_budget,
        )? {
            TableOutcome::Read(t) => {
                self.skip_many("표 셀의 읽을 수 없는 값(문자열·기간 등)", t.unsupported);
                self.skip_many("표 셀 안의 개체(그림·필드 등)", t.cell_objects);
                self.skip_many(marks::UNSAFE_LINK, t.unsafe_links);
                let (mut table, dropped_merges, dropped_links) =
                    RichTable::with_merges_and_links(t.grid, t.merges, t.links);
                table.col_widths_twips = t.col_widths_twips;
                table.cell_styles = t.cell_styles;
                self.skip_many("범위를 벗어나거나 겹치는 표 병합 영역", dropped_merges);
                self.skip_many(marks::UNSAFE_LINK, dropped_links);
                let mut out = Vec::with_capacity(1 + t.cell_images.len());
                out.push(RichBlock::Table(table));
                for (did, eq_alt) in t.cell_images {
                    let Some(img_obj) = self.object(did, ty::IMAGE) else {
                        self.skip("표 셀 안의 개체(그림·필드 등)");
                        continue;
                    };
                    match self
                        .images
                        .read_with_fallback_alt(img_obj, eq_alt.as_deref())?
                    {
                        PageImage::Picture { image, cropped } => {
                            if cropped {
                                self.report_many(CROPPED, 1);
                            }
                            out.push(RichBlock::Image(image));
                        }
                        PageImage::Skipped(why) => self.report_many(why, 1),
                    }
                }
                Ok(out)
            }
            TableOutcome::Absent => Ok(Vec::new()),
            TableOutcome::Skipped(why) => {
                self.skip(why);
                Ok(Vec::new())
            }
        }
    }
}

/// Display name of a drawable object type that is not imported.
fn object_kind(type_id: u32) -> &'static str {
    match type_id {
        ty::IMAGE => "그림",
        ty::SHAPE | ty::SHAPE_WITH_TEXT => "도형",
        ty::MOVIE => "동영상·오디오",
        ty::GROUP => "개체 그룹",
        ty::CONNECTION_LINE => "선·연결선",
        ty::CHART | ty::CHART_LEGACY => "차트",
        ty::EQUATION => "방정식",
        ty::TOC => "목차",
        ty::TABLE_INFO => "표",
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
    let settings = reference(&root_msg, root::SETTINGS)
        .and_then(|id| pkg.objects.get(id))
        .filter(|o| o.type_id == ty::SETTINGS)
        .or_else(|| {
            pkg.objects
                .objects
                .values()
                .find(|o| o.type_id == ty::SETTINGS)
        })
        .and_then(|o| Message::parse(&o.payload).ok())
        .map(|m| DocumentSettings::read(&m))
        .unwrap_or_default();
    let setup = sections::read_page_setup(&root_msg, settings.facing_pages);
    let mut reader = Reader {
        objects: &pkg.objects,
        skipped: BTreeMap::new(),
        paragraphs: 0,
        runs: 0,
        tables: TableBudget::default(),
        fmt_cache: FastU64Map::default(),
        nested_reads: HashMap::new(),
        images: PageImages::new(&pkg.objects, &pkg.data_files),
        quiet: false,
        text_budget: DecodeBudget::with_limit(MAX_CONVERTED_TEXT_BYTES),
        settings,
        body_width_pt: setup.body_width_pt,
        body_height_pt: setup.body_height_pt,
    };
    let body = reader
        .storage_message(body_id)
        .ok_or_else(|| corrupt("본문 저장소를 읽을 수 없습니다"))?;
    let page_frame = reader.build_page_frame(setup, &body)?;
    let mut blocks = reader.convert_storage(&body, Flow::Body)?;
    while matches!(blocks.last(), Some(RichBlock::Paragraph(p)) if p.runs.is_empty()) {
        blocks.pop();
    }
    blocks.extend(reader.floating_blocks()?);
    blocks.extend(reader.appendix_blocks(&body)?);
    let mut warnings: Vec<String> = reader
        .skipped
        .iter()
        .map(|(kind, n)| format!("가져오지 않은 요소: {kind} {n}개"))
        .collect();
    if reader.settings.page_layout {
        warnings.push(PAGE_LAYOUT_NOTE.to_string());
    }
    warnings.push(
        "Pages 문서의 머리말·꼬리말·각주·댓글은 본문 뒤 부록 문단으로도 옮기며, 동영상·오디오 등 일부 요소는 가져오지 않고 서식이 단순화될 수 있습니다. Pages로 저장할 때 도형·각주·댓글 등은 쓰지 않습니다."
            .to_string(),
    );
    let title = blocks
        .iter()
        .find_map(|b| match b {
            RichBlock::Paragraph(p) if p.heading_level == 1 => {
                let text = p.plain_text();
                (!text.trim().is_empty()).then_some(text)
            }
            _ => None,
        })
        .unwrap_or_default();
    Ok((
        RichDocument {
            page_frame,
            title,
            blocks,
            official_agency: None,
            ..RichDocument::default()
        },
        warnings,
    ))
}
