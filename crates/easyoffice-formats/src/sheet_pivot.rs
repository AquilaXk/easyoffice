// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/pivot_table.rs, src/pivot_cache.rs; see THIRD_PARTY_NOTICES.md)

//! Spreadsheet pivot table model and fail-closed resource budgets (`PD-255`).

use std::collections::HashMap;
use std::fmt::Write;

use crate::cell_numbers::numeric_cell_value;
use crate::sheet_names::fold_name;
use crate::sheet_styling::sheet_chart::parse_sheet_and_box;
use crate::sheet_styling::xlsx_slicers::{MAX_SLICER_DIM_PX, MAX_SLICER_POS_PX, MIN_SLICER_DIM_PX};
use crate::{CellBox, EasyOfficeError, OdsUnpacked, OoxmlCodec, SheetStyling, XmlSanitizer};

pub(crate) const MAX_SHARED_ITEMS_PER_FIELD: usize = 16_384;

pub const MAX_PIVOTS_PER_SHEET: usize = 32;
pub const MAX_WORKBOOK_PIVOTS: usize = 128;
pub const MAX_PIVOT_AXIS_FIELDS: usize = 16;
pub const MAX_PIVOT_VALUE_FIELDS: usize = 16;
pub const MAX_PIVOT_SLICERS: usize = 32;
pub const MAX_PIVOT_FILTER_VALUES: usize = 2_000;
pub const MAX_PIVOT_TEXT_BYTES: usize = 512;
/// 캐시 정의 하나(또는 피벗 하나)가 가질 수 있는 필드 이름 바이트 합(PD-262).
pub(crate) const MAX_PIVOT_FIELD_NAMES_BYTES: usize = 256 * 1024;
/// 통합 문서 전체 피벗이 복제해 들고 있는 필드 이름 바이트 합(PD-262).
pub(crate) const MAX_WORKBOOK_PIVOT_FIELD_NAMES_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_PIVOT_FILTERS: usize = 64;

/// Aggregation function of a pivot value field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PivotAggKind {
    #[default]
    Sum,
    Average,
    Count,
    Min,
    Max,
}

impl PivotAggKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Average => "average",
            Self::Count => "count",
            Self::Min => "min",
            Self::Max => "max",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("sum") {
            Some(Self::Sum)
        } else if t.eq_ignore_ascii_case("average") || t.eq_ignore_ascii_case("avg") {
            Some(Self::Average)
        } else if t.eq_ignore_ascii_case("count") {
            Some(Self::Count)
        } else if t.eq_ignore_ascii_case("min") {
            Some(Self::Min)
        } else if t.eq_ignore_ascii_case("max") {
            Some(Self::Max)
        } else {
            None
        }
    }

    pub const fn ooxml_subtotal(self) -> &'static str {
        self.as_str()
    }

    pub fn from_ooxml(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("countnums") {
            Some(Self::Count)
        } else {
            Self::parse(t)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PivotValueField {
    pub col: u32,
    pub agg: PivotAggKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PivotFilterCriteria {
    pub col: u32,
    pub values: Vec<String>,
    pub blank: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PivotSlicer {
    pub col: u32,
    pub caption: String,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub multi_select: bool,
}

impl Default for PivotSlicer {
    fn default() -> Self {
        Self {
            col: 0,
            caption: String::new(),
            left: 24,
            top: 24,
            width: 184,
            height: 210,
            multi_select: false,
        }
    }
}

impl PivotSlicer {
    pub fn check_bounds(&self, source_width: u32) -> Result<(), EasyOfficeError> {
        if self.caption.len() > MAX_PIVOT_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 슬라이서 캡션 길이가 허용 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
            )));
        }
        if self.caption.trim().is_empty()
            || self.left > MAX_SLICER_POS_PX
            || self.top > MAX_SLICER_POS_PX
            || !(MIN_SLICER_DIM_PX..=MAX_SLICER_DIM_PX).contains(&self.width)
            || !(MIN_SLICER_DIM_PX..=MAX_SLICER_DIM_PX).contains(&self.height)
            || self.col >= OoxmlCodec::MAX_XLSX_COLS
            || (source_width > 0 && self.col >= source_width)
        {
            return Err(EasyOfficeError::InvalidArgument(
                "피벗 슬라이서 속성 또는 열 오프셋이 올바르지 않습니다.".to_string(),
            ));
        }
        Ok(())
    }
}

/// Persistent pivot table definition stored in `SheetStyling::pivots`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetPivotTable {
    pub name: String,
    pub cache_id: u32,
    pub source_sheet: String,
    pub source_range: String,
    pub source_table: Option<String>,
    pub has_header: bool,
    pub location: CellBox,
    pub field_names: Vec<String>,
    pub row_fields: Vec<u32>,
    pub col_fields: Vec<u32>,
    pub filter_fields: Vec<u32>,
    pub value_fields: Vec<PivotValueField>,
    pub filters: Vec<PivotFilterCriteria>,
    pub slicers: Vec<PivotSlicer>,
}

impl Default for SheetPivotTable {
    fn default() -> Self {
        Self {
            name: "피벗 테이블1".to_string(),
            cache_id: 1,
            source_sheet: String::new(),
            source_range: String::new(),
            source_table: None,
            has_header: true,
            location: CellBox::cell(0, 0),
            field_names: Vec::new(),
            row_fields: Vec::new(),
            col_fields: Vec::new(),
            filter_fields: Vec::new(),
            value_fields: Vec::new(),
            filters: Vec::new(),
            slicers: Vec::new(),
        }
    }
}

fn check_text_len(label: &str, text: &str) -> Result<(), EasyOfficeError> {
    if text.len() > MAX_PIVOT_TEXT_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "피벗 '{label}' 길이가 허용 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
        )));
    }
    Ok(())
}

impl SheetPivotTable {
    /// Width of the pivot source in columns when statically known (`0` if deferred to a table).
    pub fn known_source_width(&self) -> u32 {
        if !self.source_range.trim().is_empty() {
            if let Some((_, b)) = parse_sheet_and_box(&self.source_sheet, &self.source_range) {
                return b.col1 - b.col0 + 1;
            }
        }
        self.field_names.len() as u32
    }

    /// Sum of the field-name bytes this pivot owns (each pivot holds its own copy).
    pub(crate) fn field_name_bytes(&self) -> usize {
        self.field_names.iter().map(String::len).sum()
    }

    pub fn check_budgets(&self) -> Result<(), EasyOfficeError> {
        check_text_len("name", &self.name)?;
        check_text_len("sourceSheet", &self.source_sheet)?;
        check_text_len("sourceRange", &self.source_range)?;
        if let Some(tbl) = self.source_table.as_deref() {
            check_text_len("sourceTable", tbl)?;
            if tbl.trim().is_empty() {
                return Err(EasyOfficeError::InvalidArgument(
                    "피벗 원본 표 이름이 비어 있습니다.".to_string(),
                ));
            }
        }
        if self.name.trim().is_empty() || self.cache_id == 0 {
            return Err(EasyOfficeError::InvalidArgument(
                "피벗 이름 또는 cacheId가 올바르지 않습니다.".to_string(),
            ));
        }
        if self.location.row1 >= OoxmlCodec::MAX_XLSX_ROWS
            || self.location.col1 >= OoxmlCodec::MAX_XLSX_COLS
        {
            return Err(EasyOfficeError::InvalidArgument(
                "피벗 위치 범위가 시트 경계를 벗어났습니다.".to_string(),
            ));
        }
        if self.row_fields.len() > MAX_PIVOT_AXIS_FIELDS
            || self.col_fields.len() > MAX_PIVOT_AXIS_FIELDS
            || self.filter_fields.len() > MAX_PIVOT_AXIS_FIELDS
        {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 축 필드 수가 허용 상한({MAX_PIVOT_AXIS_FIELDS})을 초과했습니다."
            )));
        }
        if self.value_fields.len() > MAX_PIVOT_VALUE_FIELDS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 값 필드 수가 허용 상한({MAX_PIVOT_VALUE_FIELDS})을 초과했습니다."
            )));
        }
        if self.row_fields.is_empty() && self.value_fields.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "피벗 테이블은 행 필드 또는 값 필드를 하나 이상 가져야 합니다.".to_string(),
            ));
        }
        if self.slicers.len() > MAX_PIVOT_SLICERS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 슬라이서 수가 허용 상한({MAX_PIVOT_SLICERS})을 초과했습니다."
            )));
        }
        if self.filters.len() > MAX_PIVOT_FILTERS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 필터 수가 허용 상한({MAX_PIVOT_FILTERS})을 초과했습니다."
            )));
        }
        if self.field_names.len() > OoxmlCodec::MAX_XLSX_COLS as usize {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "피벗 필드 이름이 최대 열 수를 초과했습니다.".to_string(),
            ));
        }
        for fname in &self.field_names {
            check_text_len("fieldName", fname)?;
        }
        if self.field_name_bytes() > MAX_PIVOT_FIELD_NAMES_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "피벗 필드 이름 총 길이가 허용 상한({MAX_PIVOT_FIELD_NAMES_BYTES}B)을 초과했습니다."
            )));
        }

        let has_range = !self.source_range.trim().is_empty();
        if !has_range && self.source_table.is_none() {
            return Err(EasyOfficeError::InvalidArgument(
                "피벗 원본 범위 또는 원본 표가 필요합니다.".to_string(),
            ));
        }
        let mut range_width = 0u32;
        if has_range {
            let Some((_, box_ref)) = parse_sheet_and_box(&self.source_sheet, &self.source_range)
            else {
                return Err(EasyOfficeError::InvalidArgument(
                    "피벗 원본 범위 형식이 올바르지 않습니다.".to_string(),
                ));
            };
            if box_ref.row1 >= OoxmlCodec::MAX_XLSX_ROWS
                || box_ref.col1 >= OoxmlCodec::MAX_XLSX_COLS
            {
                return Err(EasyOfficeError::InvalidArgument(
                    "피벗 원본 범위가 시트 경계를 벗어났습니다.".to_string(),
                ));
            }
            range_width = box_ref.col1 - box_ref.col0 + 1;
        }
        let known_width = if range_width > 0 {
            range_width
        } else {
            self.field_names.len() as u32
        };
        let check_col = |c: u32| -> Result<(), EasyOfficeError> {
            if c >= OoxmlCodec::MAX_XLSX_COLS || (known_width > 0 && c >= known_width) {
                return Err(EasyOfficeError::InvalidArgument(
                    "피벗 열 오프셋이 원본 범위를 벗어났습니다.".to_string(),
                ));
            }
            Ok(())
        };
        for &c in self
            .row_fields
            .iter()
            .chain(self.col_fields.iter())
            .chain(self.filter_fields.iter())
        {
            check_col(c)?;
        }
        for vf in &self.value_fields {
            check_text_len("valueField.name", &vf.name)?;
            check_col(vf.col)?;
        }
        for f in &self.filters {
            check_col(f.col)?;
            if f.values.len() > MAX_PIVOT_FILTER_VALUES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "피벗 필터 항목 수({})가 허용 상한({MAX_PIVOT_FILTER_VALUES})을 초과했습니다.",
                    f.values.len()
                )));
            }
            for v in &f.values {
                check_text_len("filter.value", v)?;
            }
        }
        for s in &self.slicers {
            s.check_bounds(known_width)?;
        }
        Ok(())
    }

    pub fn within_bounds(&self) -> bool {
        self.check_budgets().is_ok()
    }
}

/// Validates workbook and per-sheet pivot budgets fail-closed.
pub fn validate_workbook_pivots(stylings: &[SheetStyling]) -> Result<(), EasyOfficeError> {
    let mut total = 0usize;
    for st in stylings {
        if st.pivots.len() > MAX_PIVOTS_PER_SHEET {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "시트 피벗 테이블 수({})가 허용 상한({MAX_PIVOTS_PER_SHEET})을 초과했습니다.",
                st.pivots.len()
            )));
        }
        total += st.pivots.len();
        if total > MAX_WORKBOOK_PIVOTS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "통합 문서 전체 피벗 테이블 수({total})가 허용 상한({MAX_WORKBOOK_PIVOTS})을 초과했습니다."
            )));
        }
        for pivot in &st.pivots {
            pivot.check_budgets()?;
        }
    }
    Ok(())
}

pub(crate) enum CacheCell {
    Blank,
    Number(f64),
    Bool(bool),
    Text(String),
}

impl CacheCell {
    pub(crate) fn from_raw(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Self::Blank;
        }
        if trimmed.eq_ignore_ascii_case("TRUE") {
            return Self::Bool(true);
        }
        if trimmed.eq_ignore_ascii_case("FALSE") {
            return Self::Bool(false);
        }
        if let Some(n) = numeric_cell_value(trimmed) {
            return Self::Number(n);
        }
        if let Ok(n) = trimmed.parse::<f64>() {
            if n.is_finite() {
                return Self::Number(n);
            }
        }
        Self::Text(raw.to_string())
    }
}

#[derive(Default)]
pub(crate) struct SharedItemsCol {
    pub(crate) items: Vec<CacheCell>,
    index: HashMap<String, usize>,
    pub(crate) has_str: bool,
    pub(crate) has_num: bool,
    pub(crate) has_bool: bool,
    pub(crate) has_blank: bool,
    pub(crate) all_int: bool,
    pub(crate) min_num: Option<f64>,
    pub(crate) max_num: Option<f64>,
    pub(crate) overflowed: bool,
}

impl SharedItemsCol {
    pub(crate) fn new() -> Self {
        Self {
            all_int: true,
            ..Default::default()
        }
    }

    pub(crate) fn push_cell(&mut self, cell: &CacheCell) -> Option<usize> {
        let key = match cell {
            CacheCell::Blank => {
                self.has_blank = true;
                "B:".to_string()
            }
            CacheCell::Number(n) => {
                self.has_num = true;
                if n.fract() != 0.0 {
                    self.all_int = false;
                }
                self.min_num = Some(self.min_num.map_or(*n, |m| m.min(*n)));
                self.max_num = Some(self.max_num.map_or(*n, |m| m.max(*n)));
                format!("N:{n}")
            }
            CacheCell::Bool(b) => {
                self.has_bool = true;
                format!("L:{}", u8::from(*b))
            }
            CacheCell::Text(s) => {
                self.has_str = true;
                format!("S:{s}")
            }
        };
        if let Some(&idx) = self.index.get(&key) {
            return Some(idx);
        }
        if self.overflowed || self.items.len() >= MAX_SHARED_ITEMS_PER_FIELD {
            self.overflowed = true;
            return None;
        }
        let idx = self.items.len();
        let owned = match cell {
            CacheCell::Blank => CacheCell::Blank,
            CacheCell::Number(n) => CacheCell::Number(*n),
            CacheCell::Bool(b) => CacheCell::Bool(*b),
            CacheCell::Text(s) => CacheCell::Text(s.clone()),
        };
        self.items.push(owned);
        self.index.insert(key, idx);
        Some(idx)
    }

    pub(crate) fn write_cache_field_xml(&self, fname: &str, out: &mut String) {
        let _ = write!(
            out,
            "<cacheField name=\"{}\" numFmtId=\"0\"><sharedItems",
            XmlSanitizer::escape(fname)
        );
        if self.has_blank {
            out.push_str(" containsBlank=\"1\"");
        }
        if self.has_num && !self.has_str && !self.has_bool {
            out.push_str(" containsSemiMixedTypes=\"0\" containsString=\"0\" containsNumber=\"1\"");
            if self.all_int {
                out.push_str(" containsInteger=\"1\"");
            }
            if let (Some(mn), Some(mx)) = (self.min_num, self.max_num) {
                let _ = write!(out, " minValue=\"{mn}\" maxValue=\"{mx}\"");
            }
        } else if self.has_num && self.has_str {
            out.push_str(" containsMixedTypes=\"1\" containsNumber=\"1\"");
        } else if !self.has_str {
            out.push_str(" containsString=\"0\"");
        }
        if self.overflowed || self.items.is_empty() {
            out.push_str("/></cacheField>");
        } else {
            let _ = write!(out, " count=\"{}\">", self.items.len());
            for item in &self.items {
                item.write_xml(out);
            }
            out.push_str("</sharedItems></cacheField>");
        }
    }
}

impl CacheCell {
    pub(crate) fn write_xml(&self, out: &mut String) {
        match self {
            CacheCell::Blank => out.push_str("<m/>"),
            CacheCell::Number(n) => {
                let _ = write!(out, "<n v=\"{n}\"/>");
            }
            CacheCell::Bool(b) => {
                let _ = write!(out, "<b v=\"{}\"/>", u8::from(*b));
            }
            CacheCell::Text(s) => {
                let _ = write!(out, "<s v=\"{}\"/>", XmlSanitizer::escape(s));
            }
        }
    }
}

fn find_table_source(
    table_name: &str,
    preferred_sheet: &str,
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> Option<(String, CellBox)> {
    let folded = fold_name(table_name);
    for prefer in [true, false] {
        for (idx, st) in stylings.iter().enumerate() {
            let sh_name = sheets.get(idx).map(|(n, _)| n.as_str()).unwrap_or("");
            if prefer
                && !preferred_sheet.is_empty()
                && !sh_name.eq_ignore_ascii_case(preferred_sheet)
            {
                continue;
            }
            if let Some(t) = st.tables.iter().find(|t| fold_name(&t.name) == folded) {
                return Some((sh_name.to_string(), t.range));
            }
        }
    }
    None
}

pub(crate) fn resolve_pivot_source(
    pivot: &SheetPivotTable,
    sheet_name: &str,
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> (String, CellBox) {
    if !pivot.source_range.trim().is_empty() {
        let def_sh = if pivot.source_sheet.trim().is_empty() {
            sheet_name
        } else {
            &pivot.source_sheet
        };
        if let Some((sh, b)) = parse_sheet_and_box(def_sh, &pivot.source_range) {
            return (sh, b);
        }
    }
    if let Some(tbl) = pivot.source_table.as_deref() {
        if let Some((sh, b)) = find_table_source(tbl, &pivot.source_sheet, sheets, stylings) {
            return (sh, b);
        }
    }
    let sh = if pivot.source_sheet.trim().is_empty() {
        sheet_name.to_string()
    } else {
        pivot.source_sheet.clone()
    };
    (sh, pivot.location)
}
