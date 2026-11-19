//! Document and slide layout lint diagnostics (`PD-258`): overflow, overlap, and empty placeholders.

use crate::rich_notes::is_deleted_run;
use crate::{
    DecodedSlide, RichBlock, RichDocument, RichPaper, RichParagraph, RichSlide, RichSlideDeck,
    SlideElement, SlideMasterLayout, SlidePlaceholderRole, MAX_ELEMENTS_PER_SLIDE,
};

/// Maximum lint issues collected per report before truncating fail-closed.
pub const MAX_LINT_ISSUES: usize = 1_024;

const DEFAULT_A4_MARGIN_PT: f64 = 56.7;
const TWIPS_PER_PT: f64 = 20.0;
const EMU_PER_PT: f64 = 12_700.0;
const MAX_MERGE_CHECK_LIMIT: usize = 4_096;

const DEFAULT_PLACEHOLDER_PROMPTS: [&str; 6] = [
    "제목을 입력하려면 클릭하십시오",
    "부제목을 입력하려면 클릭하십시오",
    "텍스트를 입력하려면 클릭하십시오",
    "Click to add title",
    "Click to add subtitle",
    "Click to add text",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutIssueKind {
    Overflow,
    Overlap,
    EmptyPlaceholder,
}

impl LayoutIssueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
            Self::Overlap => "overlap",
            Self::EmptyPlaceholder => "emptyPlaceholder",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutIssue {
    pub kind: LayoutIssueKind,
    pub page_or_slide: usize,
    pub block_index: Option<usize>,
    pub target_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutLintReport {
    pub issues: Vec<LayoutIssue>,
    pub overflow_count: usize,
    pub overlap_count: usize,
    pub empty_placeholder_count: usize,
    pub truncated: bool,
}

impl LayoutLintReport {
    #[inline]
    fn push(&mut self, issue: LayoutIssue) -> bool {
        if self.issues.len() >= MAX_LINT_ISSUES {
            self.truncated = true;
            return false;
        }
        match issue.kind {
            LayoutIssueKind::Overflow => self.overflow_count += 1,
            LayoutIssueKind::Overlap => self.overlap_count += 1,
            LayoutIssueKind::EmptyPlaceholder => self.empty_placeholder_count += 1,
        }
        self.issues.push(issue);
        true
    }
}

/// Checks for `{{...}}` template/mail-merge tokens with 1..=64 non-whitespace chars and no newlines.
#[inline]
pub fn contains_template_token(text: &str) -> bool {
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            break;
        };
        let inner = &after_open[..close];
        if let Some(nested) = inner.find("{{") {
            rest = &after_open[nested..];
            continue;
        }
        if !inner.contains(['\n', '\r']) {
            let non_ws = inner.chars().filter(|c| !c.is_whitespace()).count();
            if (1..=64).contains(&non_ws) {
                return true;
            }
        }
        rest = &after_open[close + 2..];
    }
    false
}

#[inline]
fn is_default_placeholder_prompt(trimmed: &str) -> bool {
    DEFAULT_PLACEHOLDER_PROMPTS
        .iter()
        .any(|prompt| trimmed.eq_ignore_ascii_case(prompt))
}

#[inline]
fn estimate_wrapped_lines(text: &str, avail_w: f64, font_pt: f64) -> usize {
    let trimmed = text.trim_end_matches(['\r', '\n']);
    let mut lines = 0usize;
    for raw_seg in trimmed.split('\n') {
        let seg = raw_seg.strip_suffix('\r').unwrap_or(raw_seg);
        lines += 1;
        if seg.is_empty() {
            continue;
        }
        let mut line_w = 0.0f64;
        if seg.is_ascii() {
            let cw = font_pt * 0.55;
            for _ in seg.bytes() {
                if line_w > 0.0 && line_w + cw > avail_w + 1e-6 {
                    lines += 1;
                    line_w = cw;
                } else {
                    line_w += cw;
                }
            }
        } else {
            for ch in seg.chars() {
                let cw = if ch.is_ascii() {
                    font_pt * 0.55
                } else {
                    font_pt
                };
                if line_w > 0.0 && line_w + cw > avail_w + 1e-6 {
                    lines += 1;
                    line_w = cw;
                } else {
                    line_w += cw;
                }
            }
        }
    }
    lines
}

fn check_legacy_bullets(
    report: &mut LayoutLintReport,
    slide_no: usize,
    bullets: &[String],
) -> bool {
    let mut bullet_lines = 0usize;
    let mut body_chars = 0usize;
    for b in bullets {
        body_chars += b.chars().count();
        if !b.trim().is_empty() {
            bullet_lines += b.lines().count().max(1);
        }
    }
    if bullet_lines > 10 || body_chars > 500 {
        return report.push(LayoutIssue {
            kind: LayoutIssueKind::Overflow,
            page_or_slide: slide_no,
            block_index: None,
            target_id: format!("slide-{slide_no}-body"),
            message: format!(
                "슬라이드 {slide_no} 본문 글머리가 허용량({bullet_lines}줄, {body_chars}자)을 초과해 넘칠 수 있습니다."
            ),
        });
    }
    true
}

#[derive(Clone, Copy)]
struct SweepBox {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    orig_idx: usize,
}

fn check_slide_overlaps(
    report: &mut LayoutLintReport,
    slide_no: usize,
    slide: &RichSlide,
    stage_area: f64,
) -> bool {
    let limit = slide.elements.len().min(MAX_ELEMENTS_PER_SLIDE);
    if limit < 2 {
        return true;
    }
    let mut boxes: Vec<SweepBox> = Vec::with_capacity(limit);
    for (orig_idx, el) in slide.elements.iter().take(limit).enumerate() {
        let r = el.rect();
        if !r.x.is_finite() || !r.y.is_finite() || !r.w.is_finite() || !r.h.is_finite() {
            continue;
        }
        if matches!(el, SlideElement::Shape { .. }) && r.w * r.h >= stage_area * 0.90 {
            continue;
        }
        boxes.push(SweepBox {
            x0: r.x,
            y0: r.y,
            x1: r.x + r.w,
            y1: r.y + r.h,
            orig_idx,
        });
    }
    if boxes.len() < 2 {
        return true;
    }
    boxes.sort_by(|a, b| a.x0.total_cmp(&b.x0).then(a.orig_idx.cmp(&b.orig_idx)));

    for (p, &a) in boxes.iter().enumerate() {
        for &b in &boxes[(p + 1)..] {
            if b.x0 >= a.x1 - 1.0 {
                break;
            }
            let ix = a.x1.min(b.x1) - a.x0.max(b.x0);
            if ix <= 1.0 {
                continue;
            }
            let iy = a.y1.min(b.y1) - a.y0.max(b.y0);
            if iy <= 1.0 {
                continue;
            }
            let area = ix * iy;
            if area <= 4.0 {
                continue;
            }
            let (first, second) = if a.orig_idx < b.orig_idx {
                (a.orig_idx, b.orig_idx)
            } else {
                (b.orig_idx, a.orig_idx)
            };
            let id_a = slide.elements[first].id();
            let id_b = slide.elements[second].id();
            if !report.push(LayoutIssue {
                kind: LayoutIssueKind::Overlap,
                page_or_slide: slide_no,
                block_index: None,
                target_id: id_a.to_string(),
                message: format!(
                    "슬라이드 {slide_no} 개체 '{id_a}'와 '{id_b}'가 {area:.1}pt² 영역에서 겹쳤습니다."
                ),
            }) {
                return false;
            }
        }
    }
    true
}

fn lint_slide_element(
    report: &mut LayoutLintReport,
    slide_no: usize,
    el: &SlideElement,
    stage_w: f64,
    stage_h: f64,
) -> bool {
    let r = el.rect();
    let out_of_stage = !r.x.is_finite()
        || !r.y.is_finite()
        || !r.w.is_finite()
        || !r.h.is_finite()
        || r.x < -0.5
        || r.y < -0.5
        || r.x + r.w > stage_w + 0.5
        || r.y + r.h > stage_h + 0.5;
    if out_of_stage
        && !report.push(LayoutIssue {
            kind: LayoutIssueKind::Overflow,
            page_or_slide: slide_no,
            block_index: None,
            target_id: el.id().to_string(),
            message: format!(
                "슬라이드 {slide_no} 개체 '{}'가 스테이지 경계({stage_w:.0}x{stage_h:.0}pt) 밖으로 벗어났습니다.",
                el.id()
            ),
        })
    {
        return false;
    }

    match el {
        SlideElement::TextBox {
            id,
            rect,
            text,
            font_size_pt,
            ..
        } => {
            let trimmed = text.trim();
            if trimmed.is_empty()
                || is_default_placeholder_prompt(trimmed)
                || contains_template_token(text)
            {
                if !report.push(LayoutIssue {
                    kind: LayoutIssueKind::EmptyPlaceholder,
                    page_or_slide: slide_no,
                    block_index: None,
                    target_id: id.clone(),
                    message: format!(
                        "슬라이드 {slide_no} 글상자 '{id}'에 미입력 또는 치환되지 않은 자리표시자가 있습니다."
                    ),
                }) {
                    return false;
                }
            } else {
                let font_pt = if font_size_pt.is_finite() && *font_size_pt > 0.0 {
                    *font_size_pt
                } else {
                    18.0
                };
                let avail_w = (rect.w - 12.0).max(12.0);
                let lines = estimate_wrapped_lines(text, avail_w, font_pt);
                let est_h = lines as f64 * (font_pt * 1.25);
                if est_h > rect.h + font_pt * 0.35
                    && !report.push(LayoutIssue {
                        kind: LayoutIssueKind::Overflow,
                        page_or_slide: slide_no,
                        block_index: None,
                        target_id: id.clone(),
                        message: format!(
                            "슬라이드 {slide_no} 글상자 '{id}' 텍스트 예상 높이({est_h:.1}pt)가 상자 높이({:.1}pt)를 초과했습니다.",
                            rect.h
                        ),
                    })
                {
                    return false;
                }
            }
        }
        SlideElement::Shape {
            id,
            rect,
            text,
            font_size_pt,
            ..
        } => {
            if !text.trim().is_empty() {
                let font_pt = if font_size_pt.is_finite() && *font_size_pt > 0.0 {
                    *font_size_pt
                } else {
                    16.0
                };
                let avail_w = (rect.w - 12.0).max(12.0);
                let lines = estimate_wrapped_lines(text, avail_w, font_pt);
                let est_h = lines as f64 * (font_pt * 1.25);
                if est_h > rect.h + font_pt * 0.35
                    && !report.push(LayoutIssue {
                        kind: LayoutIssueKind::Overflow,
                        page_or_slide: slide_no,
                        block_index: None,
                        target_id: id.clone(),
                        message: format!(
                            "슬라이드 {slide_no} 도형 '{id}' 내부 텍스트가 높이({:.1}pt)를 초과했습니다.",
                            rect.h
                        ),
                    })
                {
                    return false;
                }
            }
        }
        SlideElement::Table { id, rect, rows, .. } => {
            let need_h = rows.len() as f64 * 16.0;
            if need_h > rect.h + 4.0
                && !report.push(LayoutIssue {
                    kind: LayoutIssueKind::Overflow,
                    page_or_slide: slide_no,
                    block_index: None,
                    target_id: id.clone(),
                    message: format!(
                        "슬라이드 {slide_no} 표 '{id}' 행 높이({need_h:.1}pt)가 상자 높이({:.1}pt)를 초과했습니다.",
                        rect.h
                    ),
                })
            {
                return false;
            }
        }
        SlideElement::Image { .. } | SlideElement::Chart { .. } => {}
    }
    true
}

/// Runs layout lint on a coordinate-based [`RichSlideDeck`].
pub fn lint_slide_deck(deck: &RichSlideDeck) -> LayoutLintReport {
    let mut report = LayoutLintReport::default();
    let (stage_w, stage_h) = deck.aspect.dimensions_pt();
    let stage_area = stage_w * stage_h;

    for (idx, s) in deck.slides.iter().enumerate() {
        let slide_no = idx + 1;

        for el in s.elements.iter().take(MAX_ELEMENTS_PER_SLIDE) {
            if !lint_slide_element(&mut report, slide_no, el, stage_w, stage_h) {
                return report;
            }
        }

        if !check_legacy_bullets(&mut report, slide_no, &s.bullets) {
            return report;
        }
        if !check_slide_overlaps(&mut report, slide_no, s, stage_area) {
            return report;
        }

        let has_title = !s.title.trim().is_empty()
            || s.elements.iter().any(|el| {
                matches!(
                    el,
                    SlideElement::TextBox {
                        role: Some(SlidePlaceholderRole::Title),
                        text,
                        ..
                    } if !text.trim().is_empty()
                )
            });

        match s.layout {
            SlideMasterLayout::TitleSlide | SlideMasterLayout::SectionHeader => {
                if !has_title
                    && !report.push(LayoutIssue {
                        kind: LayoutIssueKind::EmptyPlaceholder,
                        page_or_slide: slide_no,
                        block_index: None,
                        target_id: format!("slide-{slide_no}-title"),
                        message: format!(
                            "슬라이드 {slide_no}({})의 제목 영역이 비어 있습니다.",
                            s.layout.as_str()
                        ),
                    })
                {
                    return report;
                }
            }
            SlideMasterLayout::TitleAndContent | SlideMasterLayout::TwoColumn => {
                if !has_title
                    && !report.push(LayoutIssue {
                        kind: LayoutIssueKind::EmptyPlaceholder,
                        page_or_slide: slide_no,
                        block_index: None,
                        target_id: format!("slide-{slide_no}-title"),
                        message: format!("슬라이드 {slide_no}의 제목이 비어 있습니다."),
                    })
                {
                    return report;
                }
                let has_bullets = s.bullets.iter().any(|b| !b.trim().is_empty());
                if !has_bullets
                    && s.elements.is_empty()
                    && !report.push(LayoutIssue {
                        kind: LayoutIssueKind::EmptyPlaceholder,
                        page_or_slide: slide_no,
                        block_index: None,
                        target_id: format!("slide-{slide_no}-body"),
                        message: format!("슬라이드 {slide_no}의 본문 영역이 비어 있습니다."),
                    })
                {
                    return report;
                }
            }
            SlideMasterLayout::Blank => {}
        }
    }
    report
}

/// Runs layout lint on legacy `(title, bullets, notes)` slides.
pub fn lint_legacy_slides(slides: &[DecodedSlide]) -> LayoutLintReport {
    let mut report = LayoutLintReport::default();
    for (idx, (title, bullets, _notes)) in slides.iter().enumerate() {
        let slide_no = idx + 1;
        if title.trim().is_empty()
            && !report.push(LayoutIssue {
                kind: LayoutIssueKind::EmptyPlaceholder,
                page_or_slide: slide_no,
                block_index: None,
                target_id: format!("slide-{slide_no}-title"),
                message: format!("슬라이드 {slide_no}의 제목이 비어 있습니다."),
            })
        {
            return report;
        }
        if !bullets.iter().any(|b| !b.trim().is_empty())
            && !report.push(LayoutIssue {
                kind: LayoutIssueKind::EmptyPlaceholder,
                page_or_slide: slide_no,
                block_index: None,
                target_id: format!("slide-{slide_no}-body"),
                message: format!("슬라이드 {slide_no}의 본문 글머리가 비어 있습니다."),
            })
        {
            return report;
        }
        if !check_legacy_bullets(&mut report, slide_no, bullets) {
            return report;
        }
    }
    report
}

fn fill_paragraph_text<'a>(p: &'a RichParagraph, scratch: &'a mut String) -> &'a str {
    if p.runs.len() == 1 {
        let r = &p.runs[0];
        if r.note_ref.is_none() && !is_deleted_run(r) {
            return r.text.as_str();
        }
        return "";
    }
    scratch.clear();
    for r in &p.runs {
        if r.note_ref.is_none() && !is_deleted_run(r) {
            scratch.push_str(&r.text);
        }
    }
    scratch.as_str()
}

fn zone_has_content(paras: &[RichParagraph]) -> bool {
    paras.iter().any(|p| {
        p.runs.iter().any(|r| {
            r.field.is_some()
                || (r.note_ref.is_none() && !is_deleted_run(r) && !r.text.trim().is_empty())
        })
    })
}

fn check_table_merge_overlaps(
    report: &mut LayoutLintReport,
    page_no: usize,
    block_idx: usize,
    table: &crate::RichTable,
) -> bool {
    if table.merges.len() < 2 {
        return true;
    }
    let mut regions: Vec<(u32, u32, u32, u32)> = table
        .merges
        .iter()
        .take(MAX_MERGE_CHECK_LIMIT)
        .filter(|m| m.row_span >= 1 && m.col_span >= 1)
        .map(|m| {
            (
                m.row,
                m.row.saturating_add(m.row_span),
                m.col,
                m.col.saturating_add(m.col_span),
            )
        })
        .collect();
    if regions.len() < 2 {
        return true;
    }
    regions.sort_unstable();

    for i in 0..regions.len() {
        let (r0, r1, c0, c1) = regions[i];
        for &(or0, or1, oc0, oc1) in &regions[(i + 1)..] {
            if or0 >= r1 {
                break;
            }
            if or1 > r0
                && oc0 < c1
                && oc1 > c0
                && !report.push(LayoutIssue {
                    kind: LayoutIssueKind::Overlap,
                    page_or_slide: page_no,
                    block_index: Some(block_idx),
                    target_id: format!("table-{block_idx}"),
                    message: format!(
                        "표(블록 {block_idx})의 병합 영역 ({r0},{c0})과 ({or0},{oc0})이 서로 교차합니다."
                    ),
                })
            {
                return false;
            }
        }
    }
    true
}

/// Runs layout lint on a [`RichDocument`] (page margins, image/table width overflow, merge/frame overlap, placeholders).
pub fn lint_document(doc: &RichDocument) -> LayoutLintReport {
    let mut report = LayoutLintReport::default();

    let frame = &doc.page_frame;
    let paper = frame.paper.unwrap_or(RichPaper::A4);
    let paper_w_pt = f64::from(paper.width) / TWIPS_PER_PT;
    let paper_h_pt = f64::from(paper.height) / TWIPS_PER_PT;
    let (left_pt, right_pt, top_pt, bottom_pt) = match frame.margins {
        Some(m) => (
            f64::from(m.left) / TWIPS_PER_PT,
            f64::from(m.right) / TWIPS_PER_PT,
            f64::from(m.top) / TWIPS_PER_PT,
            f64::from(m.bottom) / TWIPS_PER_PT,
        ),
        None => (
            DEFAULT_A4_MARGIN_PT,
            DEFAULT_A4_MARGIN_PT,
            DEFAULT_A4_MARGIN_PT,
            DEFAULT_A4_MARGIN_PT,
        ),
    };
    let content_w_pt = (paper_w_pt - left_pt - right_pt).max(1.0);
    let content_h_pt = (paper_h_pt - top_pt - bottom_pt).max(1.0);

    if let Some(m) = frame.margins {
        let has_header = zone_has_content(&frame.header)
            || zone_has_content(&frame.first_header)
            || zone_has_content(&frame.even_header);
        if has_header
            && m.header > m.top
            && !report.push(LayoutIssue {
                kind: LayoutIssueKind::Overlap,
                page_or_slide: 1,
                block_index: None,
                target_id: "header".to_string(),
                message: format!(
                    "머리말 여백({:.1}pt)이 본문 상단 여백({:.1}pt)보다 커서 본문과 겹칩니다.",
                    f64::from(m.header) / TWIPS_PER_PT,
                    top_pt
                ),
            })
        {
            return report;
        }
        let has_footer = zone_has_content(&frame.footer)
            || zone_has_content(&frame.first_footer)
            || zone_has_content(&frame.even_footer);
        if has_footer
            && m.footer > m.bottom
            && !report.push(LayoutIssue {
                kind: LayoutIssueKind::Overlap,
                page_or_slide: 1,
                block_index: None,
                target_id: "footer".to_string(),
                message: format!(
                    "꼬리말 여백({:.1}pt)이 본문 하단 여백({:.1}pt)보다 커서 본문과 겹칩니다.",
                    f64::from(m.footer) / TWIPS_PER_PT,
                    bottom_pt
                ),
            })
        {
            return report;
        }
    }

    let mut page_no = 1usize;
    let mut scratch = String::new();

    for (idx, block) in doc.blocks.iter().enumerate() {
        match block {
            RichBlock::Paragraph(p) => {
                if idx > 0 && p.page_break_before {
                    page_no += 1;
                }
                let text = fill_paragraph_text(p, &mut scratch);
                let trimmed = text.trim();
                if p.heading_level > 0 && trimmed.is_empty() {
                    if !report.push(LayoutIssue {
                        kind: LayoutIssueKind::EmptyPlaceholder,
                        page_or_slide: page_no,
                        block_index: Some(idx),
                        target_id: format!("block-{idx}"),
                        message: format!("제목 문단(블록 {idx})이 비어 있습니다."),
                    }) {
                        return report;
                    }
                } else if !trimmed.is_empty()
                    && contains_template_token(text)
                    && !report.push(LayoutIssue {
                        kind: LayoutIssueKind::EmptyPlaceholder,
                        page_or_slide: page_no,
                        block_index: Some(idx),
                        target_id: format!("block-{idx}"),
                        message: format!(
                            "문단(블록 {idx})에 치환되지 않은 템플릿 자리표시자가 있습니다."
                        ),
                    })
                {
                    return report;
                }
            }
            RichBlock::Image(img) => {
                let (w_emu, h_emu) = img.display_emu();
                let w_pt = w_emu as f64 / EMU_PER_PT;
                let h_pt = h_emu as f64 / EMU_PER_PT;
                if (w_pt > content_w_pt + 1.0 || h_pt > content_h_pt + 1.0)
                    && !report.push(LayoutIssue {
                        kind: LayoutIssueKind::Overflow,
                        page_or_slide: page_no,
                        block_index: Some(idx),
                        target_id: format!("block-{idx}"),
                        message: format!(
                            "그림(블록 {idx}, {w_pt:.1}x{h_pt:.1}pt)이 본문 가용 영역({content_w_pt:.1}x{content_h_pt:.1}pt)을 초과합니다."
                        ),
                    })
                {
                    return report;
                }
            }
            RichBlock::Table(t) => {
                let col_count = t.column_count().max(1);
                let explicit_widths = t.validated_col_widths();
                if let Some(ref widths) = explicit_widths {
                    let sum_pt: f64 = widths.iter().map(|&w| f64::from(w) / TWIPS_PER_PT).sum();
                    if sum_pt > content_w_pt + 1.0
                        && !report.push(LayoutIssue {
                            kind: LayoutIssueKind::Overflow,
                            page_or_slide: page_no,
                            block_index: Some(idx),
                            target_id: format!("table-{idx}"),
                            message: format!(
                                "표(블록 {idx}) 전체 열 너비({sum_pt:.1}pt)가 본문 폭({content_w_pt:.1}pt)을 초과합니다."
                            ),
                        })
                    {
                        return report;
                    }
                }

                for ci in t.validated_cell_images() {
                    let (w_emu, _) = ci.image.display_emu();
                    let w_pt = w_emu as f64 / EMU_PER_PT;
                    let col_w_pt = explicit_widths
                        .as_ref()
                        .and_then(|ws| ws.get(ci.col as usize))
                        .map_or_else(
                            || content_w_pt / col_count as f64,
                            |&w| f64::from(w) / TWIPS_PER_PT,
                        );
                    if w_pt > col_w_pt + 1.0
                        && !report.push(LayoutIssue {
                            kind: LayoutIssueKind::Overflow,
                            page_or_slide: page_no,
                            block_index: Some(idx),
                            target_id: format!("table-{idx}"),
                            message: format!(
                                "표(블록 {idx}) 셀({}, {}) 그림 너비({w_pt:.1}pt)가 열 너비({col_w_pt:.1}pt)를 초과합니다.",
                                ci.row, ci.col
                            ),
                        })
                    {
                        return report;
                    }
                }

                if !check_table_merge_overlaps(&mut report, page_no, idx, t) {
                    return report;
                }

                if !t.rows.is_empty() {
                    let mut all_empty = t.cell_images.is_empty();
                    let mut found_template = false;
                    for row in &t.rows {
                        for cell in row {
                            if !cell.trim().is_empty() {
                                all_empty = false;
                                if !found_template && contains_template_token(cell) {
                                    found_template = true;
                                }
                            }
                        }
                    }
                    if all_empty {
                        if !report.push(LayoutIssue {
                            kind: LayoutIssueKind::EmptyPlaceholder,
                            page_or_slide: page_no,
                            block_index: Some(idx),
                            target_id: format!("table-{idx}"),
                            message: format!("표(블록 {idx})의 모든 셀이 비어 있습니다."),
                        }) {
                            return report;
                        }
                    } else if found_template
                        && !report.push(LayoutIssue {
                            kind: LayoutIssueKind::EmptyPlaceholder,
                            page_or_slide: page_no,
                            block_index: Some(idx),
                            target_id: format!("table-{idx}"),
                            message: format!(
                                "표(블록 {idx}) 셀에 치환되지 않은 템플릿 자리표시자가 있습니다."
                            ),
                        })
                    {
                        return report;
                    }
                }
            }
        }
    }

    report
}
