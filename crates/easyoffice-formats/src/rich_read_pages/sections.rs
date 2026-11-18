//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig; see THIRD_PARTY_NOTICES.md), src/pages.rs
//!
//! Pages document geometry, section headers/footers (zone alignment, first/even/odd templates,
//! page-number and page-count fields) and column styles (`TSWP.ColumnStyleArchive`, type 2024).

use super::{alignment_of, push_clean, reference, styled_run, AttrTable, Flow, Fmt, Reader};
use crate::pages_proto::{read_reference, Message};
use crate::pages_schema::{
    attachment, column, header_footer, number_attachment, root, section, storage, style, ty,
};
use crate::{
    frame_paragraph, EasyOfficeError, RichAlignment, RichBinding, RichBindingKind, RichBlock,
    RichColumns, RichField, RichPageFrame, RichPageMargins, RichPaper, RichParagraph, RichRun,
    MAX_PAGE_FRAME_PARAGRAPHS, MAX_PAGE_MARGIN_TWIPS,
};

/// Default A4 text body width and height in PostScript points (`595.3 - 144`, `841.9 - 144`).
pub(super) const DEFAULT_BODY_WIDTH_PT: f32 = 451.3;
pub(super) const DEFAULT_BODY_HEIGHT_PT: f32 = 697.9;
const MAX_COLUMNS: u64 = 16;

#[derive(Debug, Clone)]
pub(super) struct SectionInfo {
    pub index: usize,
    pub inherits_header_footer: bool,
    pub first_page_different: bool,
    pub even_odd_different: bool,
    pub hides_first_page: bool,
    pub first_tpl: Option<u64>,
    pub even_tpl: Option<u64>,
    pub odd_tpl: Option<u64>,
}

impl SectionInfo {
    fn has_any_template(&self) -> bool {
        self.first_tpl.is_some() || self.even_tpl.is_some() || self.odd_tpl.is_some()
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PageSetupResult {
    pub paper: Option<RichPaper>,
    pub margins: Option<RichPageMargins>,
    pub binding: RichBinding,
    pub body_width_pt: f32,
    pub body_height_pt: f32,
}

fn pt_to_u32_twips(pt: f32) -> Option<u32> {
    if !pt.is_finite() || pt < 0.0 {
        return None;
    }
    let twips = (pt * super::TWIPS_PER_POINT).round();
    if twips > MAX_PAGE_MARGIN_TWIPS as f32 {
        return None;
    }
    Some(twips as u32)
}

/// Reads paper size, margins and binding from `TP.DocumentArchive` (10000) and `facing_pages`.
pub(super) fn read_page_setup(root_msg: &Message<'_>, facing_pages: bool) -> PageSetupResult {
    let binding = RichBinding {
        kind: if facing_pages {
            RichBindingKind::Duplex
        } else {
            RichBindingKind::Single
        },
        gutter: 0,
    };
    let width_tw = root_msg
        .float_any(root::PAGE_WIDTH)
        .and_then(pt_to_u32_twips);
    let height_tw = root_msg
        .float_any(root::PAGE_HEIGHT)
        .and_then(pt_to_u32_twips);
    if let (Some(width), Some(height)) = (width_tw, height_tw) {
        if width > 0 && height > 0 {
            let paper = RichPaper { width, height };
            let m_opt = (|| {
                Some(RichPageMargins {
                    left: pt_to_u32_twips(root_msg.float_any(root::LEFT_MARGIN).unwrap_or(0.0))?,
                    right: pt_to_u32_twips(root_msg.float_any(root::RIGHT_MARGIN).unwrap_or(0.0))?,
                    top: pt_to_u32_twips(root_msg.float_any(root::TOP_MARGIN).unwrap_or(0.0))?,
                    bottom: pt_to_u32_twips(
                        root_msg.float_any(root::BOTTOM_MARGIN).unwrap_or(0.0),
                    )?,
                    header: pt_to_u32_twips(
                        root_msg.float_any(root::HEADER_MARGIN).unwrap_or(0.0),
                    )?,
                    footer: pt_to_u32_twips(
                        root_msg.float_any(root::FOOTER_MARGIN).unwrap_or(0.0),
                    )?,
                })
            })();
            if let Some(margins) = m_opt {
                if paper.holds_bound(&margins, binding) {
                    let body_w = (paper.width - margins.left - margins.right) as f32
                        / super::TWIPS_PER_POINT;
                    let body_h = (paper.height - margins.top - margins.bottom) as f32
                        / super::TWIPS_PER_POINT;
                    return PageSetupResult {
                        paper: Some(paper),
                        margins: Some(margins),
                        binding,
                        body_width_pt: body_w,
                        body_height_pt: body_h,
                    };
                }
            }
        }
    }
    PageSetupResult {
        paper: None,
        margins: None,
        binding,
        body_width_pt: DEFAULT_BODY_WIDTH_PT,
        body_height_pt: DEFAULT_BODY_HEIGHT_PT,
    }
}

/// Preserves the original repeated-field slot index (`0 = Left`, `1 = Center`, `2 = Right`)
/// BEFORE filtering out `{1: 0}` null references, when the template strip has at most 3 slots.
fn indexed_template_storages(list: &Message<'_>, field: u32) -> Vec<(Option<RichAlignment>, u64)> {
    let raw_entries = list.all_bytes(field);
    let has_three_zones = raw_entries.len() <= 3;
    raw_entries
        .into_iter()
        .enumerate()
        .filter_map(|(idx, raw)| {
            let target = read_reference(raw).filter(|id| *id != 0)?;
            let zone = if has_three_zones {
                match idx {
                    0 => Some(RichAlignment::Left),
                    1 => Some(RichAlignment::Center),
                    2 => Some(RichAlignment::Right),
                    _ => None,
                }
            } else {
                None
            };
            Some((zone, target))
        })
        .collect()
}

fn non_zero_ref(m: &Message<'_>, field: u32) -> Option<u64> {
    reference(m, field).filter(|id| *id != 0)
}

impl Reader<'_> {
    /// Parses `TP.SectionArchive` (10011) entries from the body storage's `SECTION_TABLE` (17),
    /// falling back to any `10011` objects in ascending ID order.
    pub(super) fn read_sections(&self, body: &Message<'_>) -> Vec<SectionInfo> {
        let mut pairs: Vec<(u64, u64)> = Vec::new();
        if let Ok(Some(tbl)) = body.sub(storage::SECTION_TABLE) {
            let _ = tbl.for_each_sub(storage::TABLE_ENTRIES, |e| {
                if let (Some(idx), Some(id)) = (
                    e.varint(storage::ENTRY_INDEX),
                    non_zero_ref(&e, storage::ENTRY_OBJECT),
                ) {
                    if self.object(id, ty::SECTION).is_some() {
                        pairs.push((idx, id));
                    }
                }
                Ok(())
            });
        }
        pairs.sort_by_key(|(idx, _)| *idx);
        if pairs.is_empty() {
            let mut loose: Vec<u64> = self
                .objects
                .objects
                .iter()
                .filter(|(_, o)| o.type_id == ty::SECTION)
                .map(|(id, _)| *id)
                .collect();
            loose.sort_unstable();
            pairs = loose.into_iter().map(|id| (0, id)).collect();
        }
        let mut out = Vec::with_capacity(pairs.len());
        for (index, (_, id)) in pairs.into_iter().enumerate() {
            let Some(obj) = self.object(id, ty::SECTION) else {
                continue;
            };
            let Ok(m) = Message::parse(&obj.payload) else {
                continue;
            };
            let flag = |f: u32| m.varint(f).unwrap_or(0) != 0;
            out.push(SectionInfo {
                index,
                inherits_header_footer: flag(section::INHERIT_PREVIOUS_HEADER_FOOTER),
                first_page_different: flag(section::FIRST_PAGE_DIFFERENT),
                even_odd_different: flag(section::EVEN_ODD_PAGES_DIFFERENT),
                hides_first_page: flag(section::FIRST_PAGE_HIDES_HEADER_FOOTER),
                first_tpl: non_zero_ref(&m, section::FIRST_TEMPLATE_PAGE),
                even_tpl: non_zero_ref(&m, section::EVEN_TEMPLATE_PAGE),
                odd_tpl: non_zero_ref(&m, section::ODD_TEMPLATE_PAGE),
            });
        }
        out
    }

    fn number_attachment_field(&self, att_id: u64) -> Option<RichField> {
        let obj = match self.objects.get(att_id) {
            Some(o) if o.type_id == ty::NUMBER_ATTACHMENT => o,
            Some(o) if o.type_id == ty::ATTACHMENT => {
                let m = Message::parse(&o.payload).ok()?;
                let did = reference(&m, attachment::DRAWABLE)?;
                self.object(did, ty::NUMBER_ATTACHMENT)?
            }
            _ => return None,
        };
        let msg = Message::parse(&obj.payload).ok()?;
        let kind = if let Some(sup) = msg.sub(number_attachment::SUPER).ok().flatten() {
            sup.varint(number_attachment::KIND)
                .unwrap_or(number_attachment::KIND_PAGE_NUMBER)
        } else {
            msg.varint(number_attachment::KIND)?
        };
        match kind {
            number_attachment::KIND_PAGE_NUMBER => Some(RichField::PageNumber),
            number_attachment::KIND_PAGE_COUNT => Some(RichField::TotalPages),
            _ => None,
        }
    }

    /// Reads one header/footer zone storage into `RichPageFrame` paragraphs, converting `2043`
    /// attachments into `RichField` runs without charging picture or warning counters.
    fn frame_zone_paragraphs(
        &mut self,
        storage_id: u64,
        zone: Option<RichAlignment>,
        out: &mut Vec<RichParagraph>,
    ) -> Result<(), EasyOfficeError> {
        let Some(st) = self.storage_message(storage_id) else {
            return Ok(());
        };
        let text = st.text(storage::TEXT)?;
        if text.is_empty() {
            return Ok(());
        }
        self.text_budget.charge(text.len(), "Pages 머리글·바닥글")?;
        let para_styles = AttrTable::parse(&st, storage::PARA_STYLE_TABLE)?;
        let char_styles = AttrTable::parse(&st, storage::CHAR_STYLE_TABLE)?;
        let attachments = AttrTable::parse(&st, storage::ATTACHMENT_TABLE)?;

        let mut pos: u64 = 0;
        for line in text.split(super::ends_paragraph) {
            if out.len() >= MAX_PAGE_FRAME_PARAGRAPHS {
                break;
            }
            let start = pos;
            let pfmt = para_styles.at(start).map(|id| self.style_fmt(id));
            let mut style_cuts = char_styles.boundaries_after(start).peekable();
            let mut runs: Vec<RichRun> = Vec::new();
            let mut cur = String::new();
            let mut seg_start = start;
            let mut utf16 = start;

            let flush = |runs: &mut Vec<RichRun>,
                         cur: &mut String,
                         seg_start: u64,
                         pfmt: Option<&Fmt>,
                         this: &mut Self|
             -> Result<(), EasyOfficeError> {
                if cur.is_empty() {
                    return Ok(());
                }
                this.count_run()?;
                let cfmt = char_styles.at(seg_start).map(|id| this.style_fmt(id));
                runs.push(styled_run(std::mem::take(cur), cfmt.as_deref(), pfmt));
                Ok(())
            };

            for ch in line.chars() {
                let mut cut = false;
                while style_cuts.next_if(|c| *c <= utf16).is_some() {
                    cut = true;
                }
                if cut {
                    flush(&mut runs, &mut cur, seg_start, pfmt.as_deref(), self)?;
                    seg_start = utf16;
                }
                if ch == '\u{FFFC}' {
                    if let Some(field_kind) = attachments
                        .exact(utf16)
                        .and_then(|aid| self.number_attachment_field(aid))
                    {
                        flush(&mut runs, &mut cur, seg_start, pfmt.as_deref(), self)?;
                        self.count_run()?;
                        let cfmt = char_styles.at(utf16).map(|id| self.style_fmt(id));
                        let mut f_run = styled_run(String::new(), cfmt.as_deref(), pfmt.as_deref());
                        f_run.field = Some(field_kind);
                        runs.push(f_run);
                        seg_start = utf16 + 1;
                    }
                } else {
                    push_clean(&mut cur, ch);
                }
                utf16 += ch.len_utf16() as u64;
            }
            flush(&mut runs, &mut cur, seg_start, pfmt.as_deref(), self)?;
            pos = utf16.saturating_add(1);

            let has_content = runs
                .iter()
                .any(|r| r.field.is_some() || !r.text.trim().is_empty());
            if !has_content {
                continue;
            }
            let mut para = RichParagraph {
                runs,
                alignment: zone
                    .or_else(|| pfmt.as_ref().and_then(|f| f.align).and_then(alignment_of)),
                left_indent_twips: pfmt.as_ref().and_then(|f| f.left_indent_twips),
                first_line_indent_twips: pfmt.as_ref().and_then(|f| f.first_line_indent_twips),
                line_spacing_percent: pfmt.as_ref().and_then(|f| f.line_spacing_percent),
                ..RichParagraph::default()
            };
            para = frame_paragraph(&para);
            out.push(para);
        }
        Ok(())
    }

    fn template_frame_paragraphs(
        &mut self,
        tpl_id: u64,
        field: u32,
    ) -> Result<Vec<RichParagraph>, EasyOfficeError> {
        let Some(obj) = self.object(tpl_id, ty::HEADER_FOOTER_LIST) else {
            return Ok(Vec::new());
        };
        let list = Message::parse(&obj.payload)?;
        let mut out = Vec::new();
        for (zone, storage_id) in indexed_template_storages(&list, field) {
            let mut zone_paras = Vec::new();
            self.frame_zone_paragraphs(storage_id, zone, &mut zone_paras)?;
            for p in zone_paras {
                if !out
                    .iter()
                    .any(|existing: &RichParagraph| existing.runs == p.runs)
                {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    /// Builds `RichPageFrame` from `PageSetupResult` and the first section's templates (or the
    /// first `10143` object when no section names a template).
    pub(super) fn build_page_frame(
        &mut self,
        setup: PageSetupResult,
        body: &Message<'_>,
    ) -> Result<RichPageFrame, EasyOfficeError> {
        let mut frame = RichPageFrame {
            paper: setup.paper,
            margins: setup.margins,
            binding: setup.binding,
            ..RichPageFrame::default()
        };
        let sections = self.read_sections(body);
        let mut referenced = std::collections::HashSet::new();
        for sec in &sections {
            referenced.extend(sec.first_tpl);
            referenced.extend(sec.even_tpl);
            referenced.extend(sec.odd_tpl);
        }
        if let Some(sec0) = sections.first().filter(|s| s.has_any_template()) {
            frame.first_page_different = sec0.first_page_different;
            frame.even_odd_different = sec0.even_odd_different || self.settings.facing_pages;
            if let Some(odd_id) = sec0.odd_tpl {
                frame.header = self.template_frame_paragraphs(odd_id, header_footer::HEADERS)?;
                frame.footer = self.template_frame_paragraphs(odd_id, header_footer::FOOTERS)?;
            }
            if sec0.first_page_different && !sec0.hides_first_page {
                if let Some(first_id) = sec0.first_tpl {
                    frame.first_header =
                        self.template_frame_paragraphs(first_id, header_footer::HEADERS)?;
                    frame.first_footer =
                        self.template_frame_paragraphs(first_id, header_footer::FOOTERS)?;
                }
            } else if !sec0.hides_first_page {
                // Synthetic fixture compatibility: when `odd_tpl` is blank and `first_page_different`
                // is unset, check `first_tpl` as a fallback default strip.
                if let Some(first_id) = sec0.first_tpl {
                    if frame.header.is_empty() {
                        frame.header =
                            self.template_frame_paragraphs(first_id, header_footer::HEADERS)?;
                    }
                    if frame.footer.is_empty() {
                        frame.footer =
                            self.template_frame_paragraphs(first_id, header_footer::FOOTERS)?;
                    }
                }
            }
            if frame.even_odd_different {
                if let Some(even_id) = sec0.even_tpl {
                    frame.even_header =
                        self.template_frame_paragraphs(even_id, header_footer::HEADERS)?;
                    frame.even_footer =
                        self.template_frame_paragraphs(even_id, header_footer::FOOTERS)?;
                }
            }
            if !frame.is_empty() {
                return Ok(frame);
            }
        }
        let unreferenced_tpl = self
            .objects
            .objects
            .iter()
            .filter(|(id, o)| o.type_id == ty::HEADER_FOOTER_LIST && !referenced.contains(id))
            .map(|(id, _)| *id)
            .min();
        if let Some(tpl_id) = unreferenced_tpl {
            frame.header = self.template_frame_paragraphs(tpl_id, header_footer::HEADERS)?;
            frame.footer = self.template_frame_paragraphs(tpl_id, header_footer::FOOTERS)?;
        }
        Ok(frame)
    }

    /// Reads active header or footer blocks for the document appendix, preserving zone alignment,
    /// `[첫 쪽] ` / `[짝수 쪽] ` prefixes on distinct variant paragraphs, and logo deduplication.
    pub(super) fn appendix_header_footer_blocks(
        &mut self,
        body: &Message<'_>,
        field: u32,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let sections = self.read_sections(body);
        let mut referenced = std::collections::HashSet::new();
        for sec in &sections {
            referenced.extend(sec.first_tpl);
            referenced.extend(sec.even_tpl);
            referenced.extend(sec.odd_tpl);
        }
        let mut templates: Vec<(u64, Option<&'static str>)> = Vec::new();
        let mut fallback_first: Vec<u64> = Vec::new();
        if sections.iter().any(SectionInfo::has_any_template) {
            for sec in &sections {
                if sec.index > 0 && sec.inherits_header_footer {
                    continue;
                }
                if let Some(odd) = sec.odd_tpl {
                    templates.push((odd, None));
                }
                if sec.first_page_different && !sec.hides_first_page {
                    if let Some(first) = sec.first_tpl {
                        templates.push((first, Some("[첫 쪽] ")));
                    }
                } else if !sec.hides_first_page {
                    if let Some(first) = sec.first_tpl {
                        fallback_first.push(first);
                    }
                }
                if sec.even_odd_different || self.settings.facing_pages {
                    if let Some(even) = sec.even_tpl {
                        templates.push((even, Some("[짝수 쪽] ")));
                    }
                }
            }
        }
        let mut loose_ids: Vec<u64> = self
            .objects
            .objects
            .iter()
            .filter(|(id, o)| o.type_id == ty::HEADER_FOOTER_LIST && !referenced.contains(id))
            .map(|(id, _)| *id)
            .collect();
        loose_ids.sort_unstable();
        templates.extend(loose_ids.into_iter().map(|id| (id, None)));

        let same_appendix_block = |a: &RichBlock, b: &RichBlock| -> bool {
            match (a, b) {
                (RichBlock::Paragraph(pa), RichBlock::Paragraph(pb)) => pa.runs == pb.runs,
                _ => a == b,
            }
        };
        let mut out: Vec<RichBlock> = Vec::new();
        let collect_pass = |this: &mut Self,
                            list_tpls: &[(u64, Option<&'static str>)],
                            out: &mut Vec<RichBlock>|
         -> Result<(), EasyOfficeError> {
            for &(tpl_id, prefix) in list_tpls {
                let Some(obj) = this.object(tpl_id, ty::HEADER_FOOTER_LIST) else {
                    continue;
                };
                let list = Message::parse(&obj.payload)?;
                for (zone, storage_id) in indexed_template_storages(&list, field) {
                    let Some(st) = this.storage_message(storage_id) else {
                        continue;
                    };
                    this.quiet = true;
                    let blocks = this.convert_storage(&st, Flow::Nested);
                    this.quiet = false;
                    for mut block in blocks? {
                        if let RichBlock::Paragraph(p) = &mut block {
                            if p.plain_text().trim().is_empty() {
                                continue;
                            }
                            if let Some(z) = zone {
                                p.alignment = Some(z);
                            }
                        }
                        if out
                            .iter()
                            .any(|existing| same_appendix_block(existing, &block))
                        {
                            if let RichBlock::Image(image) = &block {
                                this.images.release(image);
                            }
                            continue;
                        }
                        if let (Some(pfx), RichBlock::Paragraph(p)) = (prefix, &mut block) {
                            p.runs.insert(0, RichRun::plain(pfx));
                        }
                        if !out
                            .iter()
                            .any(|existing| same_appendix_block(existing, &block))
                        {
                            out.push(block);
                        }
                    }
                }
            }
            Ok(())
        };

        collect_pass(self, &templates, &mut out)?;
        if out.is_empty() && !fallback_first.is_empty() {
            let fb: Vec<(u64, Option<&'static str>)> =
                fallback_first.into_iter().map(|id| (id, None)).collect();
            collect_pass(self, &fb, &mut out)?;
        }
        Ok(out)
    }

    /// Resolves multi-column layout (`count > 1`) along a `TSWP.ColumnStyleArchive` (2024) chain.
    pub(super) fn resolve_columns(&self, style_id: u64) -> Option<RichColumns> {
        let chain = self.objects.chain(style_id, super::MAX_STYLE_CHAIN, |o| {
            let m = Message::parse(&o.payload).ok()?;
            let header = m.sub(style::HEADER).ok().flatten()?;
            reference(&header, style::HEADER_PARENT)
        });
        let body_w_twips = (self.body_width_pt * super::TWIPS_PER_POINT).max(0.0);
        let gap_of = |frac: Option<f32>| -> u16 {
            match frac {
                Some(f) if f.is_finite() && f > 0.0 && f < 1.0 => {
                    (f * body_w_twips).round().clamp(0.0, f32::from(u16::MAX)) as u16
                }
                _ => 0,
            }
        };
        for id in chain {
            let Some(props) = self
                .object(id, ty::COLUMN_STYLE)
                .and_then(|o| Message::parse(&o.payload).ok())
                .and_then(|m| m.sub(column::PROPS).ok().flatten())
            else {
                continue;
            };
            if props.varint(column::COLUMNS_NULL).unwrap_or(0) != 0 {
                return None;
            }
            let Some(columns) = props.sub(column::COLUMNS).ok().flatten() else {
                continue;
            };
            if let Ok(Some(equal)) = columns.sub(column::EQUAL) {
                let count = equal.varint(column::EQUAL_COUNT).unwrap_or(1);
                if count <= 1 {
                    return None;
                }
                return Some(RichColumns {
                    count: count.min(MAX_COLUMNS) as u8,
                    equal: true,
                    gap_twips: gap_of(equal.float_any(column::EQUAL_GAP)),
                });
            }
            if let Ok(Some(unequal)) = columns.sub(column::UNEQUAL) {
                let following = unequal.subs(column::UNEQUAL_FOLLOWING).unwrap_or_default();
                let count = (following.len() as u64).saturating_add(1);
                if count <= 1 {
                    return None;
                }
                let first_gap = following
                    .first()
                    .and_then(|f| f.float_any(column::UNEQUAL_GAP));
                return Some(RichColumns {
                    count: count.min(MAX_COLUMNS) as u8,
                    equal: false,
                    gap_twips: gap_of(first_gap),
                });
            }
        }
        None
    }
}
