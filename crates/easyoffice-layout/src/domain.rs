use crate::{LayoutRect, OfficeModuleKind};
use easyoffice_formats::{EasyOfficeError, PdfDocumentModel, PdfPageModel};
use std::collections::HashMap;

/// Ribbon layout style supporting both global standard office and Korean HWP page/manuscript modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RibbonLayoutStyle {
    StandardOffice,
    HangulHwp,
}

impl RibbonLayoutStyle {
    pub fn category_titles(&self) -> [&'static str; 7] {
        match self {
            Self::StandardOffice => [
                "홈",
                "삽입",
                "레이아웃",
                "수식·데이터",
                "모듈 전용",
                "검토·보안",
                "보기·창",
            ],
            Self::HangulHwp => [
                "편집",
                "입력",
                "쪽·원고지",
                "표·개체",
                "시트·차트",
                "보안·도구",
                "보기",
            ],
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::StandardOffice => "표준 오피스",
            Self::HangulHwp => "한글(HWP) 쪽·편집",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            Self::StandardOffice => Self::HangulHwp,
            Self::HangulHwp => Self::StandardOffice,
        }
    }
}

/// Contextual object tool mode automatically activated by the active editor module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextualObjectToolKind {
    TableAndObject,
    PageApprovalAndManuscript,
    FormulaAndPivot,
    PresenterAndLaser,
    ApprovalAndAnnotation,
}

impl ContextualObjectToolKind {
    pub fn badge_title(&self) -> &'static str {
        match self {
            Self::TableAndObject => "표·개체 도구",
            Self::PageApprovalAndManuscript => "쪽·결재·원고지 도구",
            Self::FormulaAndPivot => "수식·피벗 분석",
            Self::PresenterAndLaser => "발표자·레이저 도구",
            Self::ApprovalAndAnnotation => "전자결재·주석 도구",
        }
    }

    pub fn for_module(module: OfficeModuleKind) -> Self {
        match module {
            OfficeModuleKind::Document => Self::TableAndObject,
            OfficeModuleKind::Hwp => Self::PageApprovalAndManuscript,
            OfficeModuleKind::Sheet => Self::FormulaAndPivot,
            OfficeModuleKind::Slide => Self::PresenterAndLaser,
            OfficeModuleKind::Pdf => Self::ApprovalAndAnnotation,
        }
    }
}

/// Cell span descriptor inside a document or HWP table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableCellSpan {
    pub start_row: usize,
    pub start_col: usize,
    pub row_span: usize,
    pub col_span: usize,
}

/// Spanned table model for Document and HWP/HWPX editors with formula evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentTableModel {
    pub rows: Vec<Vec<String>>,
    pub spans: Vec<TableCellSpan>,
    pub border_style: String,
}

impl DocumentTableModel {
    pub fn new(rows: Vec<Vec<String>>) -> Self {
        Self {
            rows,
            spans: Vec::new(),
            border_style: "solid".to_string(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn col_count(&self) -> usize {
        self.rows.first().map(|r| r.len()).unwrap_or(0)
    }

    pub fn merge_cells(
        &mut self,
        start_row: usize,
        start_col: usize,
        row_span: usize,
        col_span: usize,
    ) -> Result<(), EasyOfficeError> {
        if row_span == 0 || col_span == 0 {
            return Err(EasyOfficeError::InvalidArgument(
                "병합 범위(row_span, col_span)는 1 이상이어야 합니다.".to_string(),
            ));
        }
        if start_row + row_span > self.row_count() || start_col + col_span > self.col_count() {
            return Err(EasyOfficeError::InvalidArgument(
                "표 셀 병합 범위가 표 크기를 벗어났습니다.".to_string(),
            ));
        }
        self.spans
            .retain(|s| !(s.start_row == start_row && s.start_col == start_col));
        self.spans.push(TableCellSpan {
            start_row,
            start_col,
            row_span,
            col_span,
        });
        Ok(())
    }

    pub fn is_covered_by_span(&self, row: usize, col: usize) -> bool {
        self.spans.iter().any(|s| {
            row >= s.start_row
                && row < s.start_row + s.row_span
                && col >= s.start_col
                && col < s.start_col + s.col_span
                && !(row == s.start_row && col == s.start_col)
        })
    }

    pub fn insert_row(
        &mut self,
        row_idx: usize,
        default_text: &str,
    ) -> Result<(), EasyOfficeError> {
        let cols = self.col_count().max(1);
        let idx = row_idx.min(self.rows.len());
        self.rows.insert(idx, vec![default_text.to_string(); cols]);
        for span in &mut self.spans {
            if span.start_row >= idx {
                span.start_row += 1;
            }
        }
        Ok(())
    }

    pub fn insert_column(
        &mut self,
        col_idx: usize,
        default_text: &str,
    ) -> Result<(), EasyOfficeError> {
        for row in &mut self.rows {
            let idx = col_idx.min(row.len());
            row.insert(idx, default_text.to_string());
        }
        for span in &mut self.spans {
            if span.start_col >= col_idx {
                span.start_col += 1;
            }
        }
        Ok(())
    }

    /// Evaluates table cell formulas (`=SUM(ABOVE)`, `=AVERAGE(ABOVE)`, `=COUNT(ABOVE)`, `=SUM(LEFT)`, `=AVERAGE(LEFT)`, `=COUNT(LEFT)`).
    pub fn evaluate_cell_formula(&self, row: usize, col: usize) -> Result<f64, EasyOfficeError> {
        let raw = self
            .rows
            .get(row)
            .and_then(|r| r.get(col))
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(format!(
                    "유효하지 않은 표 셀 좌표: ({row}, {col})"
                ))
            })?
            .trim();

        let upper = raw.to_ascii_uppercase().replace(' ', "");
        let collect_above = || -> Vec<f64> {
            (0..row)
                .filter_map(|r| self.rows.get(r).and_then(|cols| cols.get(col)))
                .filter_map(|v| v.trim().parse::<f64>().ok())
                .collect()
        };
        let collect_left = || -> Vec<f64> {
            (0..col)
                .filter_map(|c| self.rows.get(row).and_then(|cols| cols.get(c)))
                .filter_map(|v| v.trim().parse::<f64>().ok())
                .collect()
        };

        match upper.as_str() {
            "=SUM(ABOVE)" => Ok(collect_above().into_iter().sum()),
            "=AVERAGE(ABOVE)" => {
                let vals = collect_above();
                if vals.is_empty() {
                    Ok(0.0)
                } else {
                    Ok(vals.iter().sum::<f64>() / (vals.len() as f64))
                }
            }
            "=COUNT(ABOVE)" => Ok(collect_above().len() as f64),
            "=SUM(LEFT)" => Ok(collect_left().into_iter().sum()),
            "=AVERAGE(LEFT)" => {
                let vals = collect_left();
                if vals.is_empty() {
                    Ok(0.0)
                } else {
                    Ok(vals.iter().sum::<f64>() / (vals.len() as f64))
                }
            }
            "=COUNT(LEFT)" => Ok(collect_left().len() as f64),
            _ => raw.parse::<f64>().map_err(|_| {
                EasyOfficeError::FormulaError(format!("지원하지 않는 표 수식입니다: {raw}"))
            }),
        }
    }
}

/// Tracked revision record for Document / HWP review workflows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRecord {
    pub id: String,
    pub author: String,
    pub action: String,
    pub summary: String,
    pub accepted: Option<bool>,
}

/// Footnote or endnote entry attached to a document paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteEntry {
    pub number: usize,
    pub anchor_text: String,
    pub note_text: String,
    pub is_endnote: bool,
}

/// Document & HWP/HWPX domain engine managing spanned tables, revisions, footnotes, TOC, and mail merge.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentDomainEngine {
    pub tables: Vec<DocumentTableModel>,
    pub revisions: Vec<RevisionRecord>,
    pub footnotes: Vec<FootnoteEntry>,
    pub headings: Vec<(u8, String)>,
}

impl DocumentDomainEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_revision(&mut self, id: &str, author: &str, action: &str, summary: &str) {
        self.revisions.push(RevisionRecord {
            id: id.to_string(),
            author: author.to_string(),
            action: action.to_string(),
            summary: summary.to_string(),
            accepted: None,
        });
    }

    pub fn accept_revision(&mut self, id: &str) -> bool {
        if let Some(rev) = self.revisions.iter_mut().find(|r| r.id == id) {
            rev.accepted = Some(true);
            true
        } else {
            false
        }
    }

    pub fn reject_revision(&mut self, id: &str) -> bool {
        if let Some(rev) = self.revisions.iter_mut().find(|r| r.id == id) {
            rev.accepted = Some(false);
            true
        } else {
            false
        }
    }

    pub fn add_footnote(
        &mut self,
        anchor_text: &str,
        note_text: &str,
        is_endnote: bool,
    ) -> FootnoteEntry {
        let entry = FootnoteEntry {
            number: self.footnotes.len() + 1,
            anchor_text: anchor_text.to_string(),
            note_text: note_text.to_string(),
            is_endnote,
        };
        self.footnotes.push(entry.clone());
        entry
    }

    pub fn generate_toc(&self) -> Vec<String> {
        Self::generate_toc_from_headings(&self.headings)
    }

    pub fn generate_toc_from_headings(headings: &[(u8, String)]) -> Vec<String> {
        let mut h1 = 0usize;
        let mut h2 = 0usize;
        let mut h3 = 0usize;
        let mut lines = Vec::new();

        for (level, title) in headings {
            match level {
                1 => {
                    h1 += 1;
                    h2 = 0;
                    h3 = 0;
                    lines.push(format!("{h1}. {}", title.trim()));
                }
                2 => {
                    if h1 == 0 {
                        h1 = 1;
                    }
                    h2 += 1;
                    h3 = 0;
                    lines.push(format!("  {h1}.{h2}. {}", title.trim()));
                }
                3 => {
                    if h1 == 0 {
                        h1 = 1;
                    }
                    if h2 == 0 {
                        h2 = 1;
                    }
                    h3 += 1;
                    lines.push(format!("    {h1}.{h2}.{h3}. {}", title.trim()));
                }
                _ => {}
            }
        }
        lines
    }

    pub fn render_mail_merge(template: &str, records: &[HashMap<String, String>]) -> Vec<String> {
        records
            .iter()
            .map(|record| {
                let mut rendered = template.to_string();
                let mut keys: Vec<&String> = record.keys().collect();
                keys.sort();
                for key in keys {
                    if let Some(val) = record.get(key) {
                        let token = format!("{{{{{key}}}}}");
                        rendered = rendered.replace(&token, val);
                    }
                }
                rendered
            })
            .collect()
    }
}

/// 5 master slide layouts for the Slide presentation domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlideMasterLayout {
    TitleSlide,
    TitleAndContent,
    TwoColumnComparison,
    SectionHeader,
    BlankCanvas,
}

impl SlideMasterLayout {
    pub fn title(&self) -> &'static str {
        match self {
            Self::TitleSlide => "제목 슬라이드",
            Self::TitleAndContent => "제목 및 본문",
            Self::TwoColumnComparison => "2단 비교 레이아웃",
            Self::SectionHeader => "섹션 구획 머리글",
            Self::BlankCanvas => "빈 캔버스",
        }
    }
}

/// Slide transition timing & auto-advance configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideTransitionConfig {
    pub kind: String,
    pub duration_seconds: f64,
    pub auto_advance_seconds: Option<u32>,
}

impl SlideTransitionConfig {
    pub fn new(kind: &str, duration_seconds: f64, auto_advance_seconds: Option<u32>) -> Self {
        Self {
            kind: kind.to_string(),
            duration_seconds: duration_seconds.clamp(0.2, 5.0),
            auto_advance_seconds,
        }
    }
}

/// Live presenter session state tracking current slide, timer, laser pointer, and ink annotations.
#[derive(Debug, Clone, PartialEq)]
pub struct PresenterSessionState {
    pub current_slide: usize,
    pub elapsed_seconds: u32,
    pub laser_pointer: Option<(f64, f64)>,
    pub ink_strokes: Vec<Vec<(f64, f64)>>,
}

impl Default for PresenterSessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl PresenterSessionState {
    pub fn new() -> Self {
        Self {
            current_slide: 0,
            elapsed_seconds: 0,
            laser_pointer: None,
            ink_strokes: Vec::new(),
        }
    }

    pub fn move_laser_pointer(&mut self, x: f64, y: f64) {
        let clamped_x = x.clamp(0.0, 840.0);
        let clamped_y = y.clamp(0.0, 472.5);
        self.laser_pointer = Some((clamped_x, clamped_y));
    }

    pub fn append_ink_point(&mut self, x: f64, y: f64) {
        if self.ink_strokes.is_empty() {
            self.ink_strokes.push(Vec::new());
        }
        if let Some(last_stroke) = self.ink_strokes.last_mut() {
            last_stroke.push((x.clamp(0.0, 840.0), y.clamp(0.0, 472.5)));
        }
    }

    pub fn start_new_ink_stroke(&mut self) {
        self.ink_strokes.push(Vec::new());
    }

    pub fn clear_ink(&mut self) {
        self.ink_strokes.clear();
    }
}

/// Slide presentation domain engine combining master layout, transitions, and presenter tools.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideDomainEngine {
    pub master_layout: SlideMasterLayout,
    pub transition: SlideTransitionConfig,
    pub presenter: PresenterSessionState,
}

impl Default for SlideDomainEngine {
    fn default() -> Self {
        Self {
            master_layout: SlideMasterLayout::TitleAndContent,
            transition: SlideTransitionConfig::new("morph", 0.8, None),
            presenter: PresenterSessionState::new(),
        }
    }
}

/// Official approval stamp badge placed on a PDF page.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfStampBadge {
    pub label: String,
    pub approver: String,
    pub date: String,
    pub rect: LayoutRect,
}

/// PDF diagonal watermark configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfWatermarkConfig {
    pub text: String,
    pub opacity: f64,
    pub angle_deg: f64,
}

/// Freehand vector ink stroke on a PDF page.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfFreehandInkStroke {
    pub points: Vec<(f64, f64)>,
    pub stroke_hex: String,
    pub width_pt: f64,
}

/// Interactive AcroForm field widget inside PDF Studio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfAcroFormField {
    pub name: String,
    pub value: String,
    pub is_checked: bool,
}

/// PDF Studio domain engine supporting page rotation, range splitting, merging, stamps, watermarks, ink, and AcroForm fields.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfDomainEngine {
    pub document: PdfDocumentModel,
    pub stamps: Vec<PdfStampBadge>,
    pub watermark: Option<PdfWatermarkConfig>,
    pub ink_strokes: Vec<PdfFreehandInkStroke>,
    pub form_fields: Vec<PdfAcroFormField>,
}

impl PdfDomainEngine {
    pub fn new(document: PdfDocumentModel) -> Self {
        Self {
            document,
            stamps: Vec::new(),
            watermark: None,
            ink_strokes: Vec::new(),
            form_fields: Vec::new(),
        }
    }

    pub fn rotate_page(
        &mut self,
        page_idx: usize,
        degrees_90_multiple: i32,
    ) -> Result<i32, EasyOfficeError> {
        if degrees_90_multiple % 90 != 0 {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "PDF 페이지 회전 각도는 90도 단위여야 합니다: {degrees_90_multiple}"
            )));
        }
        let page = self.document.pages.get_mut(page_idx).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("유효하지 않은 PDF 페이지 인덱스: {page_idx}"))
        })?;
        let current = page.rotation_degrees;
        let next = (current + degrees_90_multiple).rem_euclid(360);
        page.rotation_degrees = next;
        Ok(next)
    }

    pub fn split_range(
        &self,
        start_page: usize,
        end_page: usize,
    ) -> Result<PdfDocumentModel, EasyOfficeError> {
        let len = self.document.pages.len();
        if len == 0 || start_page > end_page || end_page >= len {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "PDF 페이지 분할 범위({start_page}..={end_page})가 총 페이지 수({len})를 초과했거나 유효하지 않습니다."
            )));
        }

        let pages: Vec<PdfPageModel> = self.document.pages[start_page..=end_page]
            .iter()
            .enumerate()
            .map(|(i, p)| PdfPageModel {
                page_number: i + 1,
                rotation_degrees: p.rotation_degrees,
                body_text: p.body_text.clone(),
                approval_stamp: p.approval_stamp.clone(),
            })
            .collect();

        Ok(PdfDocumentModel {
            title: format!("{} (Split)", self.document.title),
            pages,
            watermark_text: self.document.watermark_text.clone(),
        })
    }

    pub fn merge_documents(doc_a: &PdfDocumentModel, doc_b: &PdfDocumentModel) -> PdfDocumentModel {
        let mut pages = Vec::with_capacity(doc_a.pages.len() + doc_b.pages.len());
        for p in doc_a.pages.iter().chain(doc_b.pages.iter()) {
            pages.push(PdfPageModel {
                page_number: pages.len() + 1,
                rotation_degrees: p.rotation_degrees,
                body_text: p.body_text.clone(),
                approval_stamp: p.approval_stamp.clone(),
            });
        }
        PdfDocumentModel {
            title: format!("{} + {}", doc_a.title, doc_b.title),
            pages,
            watermark_text: doc_a
                .watermark_text
                .clone()
                .or_else(|| doc_b.watermark_text.clone()),
        }
    }

    pub fn add_stamp(&mut self, label: &str, approver: &str, date: &str) -> PdfStampBadge {
        let badge = PdfStampBadge {
            label: label.to_string(),
            approver: approver.to_string(),
            date: date.to_string(),
            rect: LayoutRect::new(386.0, 576.0, 160.0, 46.0),
        };
        if let Some(first_page) = self.document.pages.first_mut() {
            first_page.approval_stamp = Some(label.to_string());
        }
        self.stamps.push(badge.clone());
        badge
    }

    pub fn set_watermark(&mut self, text: &str, opacity: f64, angle_deg: f64) {
        let cfg = PdfWatermarkConfig {
            text: text.to_string(),
            opacity: opacity.clamp(0.05, 1.0),
            angle_deg,
        };
        self.document.watermark_text = Some(text.to_string());
        self.watermark = Some(cfg);
    }
}
