//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
//!
//! 코파일럿 응답 연산·읽기 도구·위치 데이터 구조와 단일 패스 JSON 직렬화기.

use std::fmt::Write as _;

use super::types::{push_json_str, ChartLegend, ChartType, CopilotMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetWriteCell {
    pub coord: String,
    pub raw: String,
    pub num_fmt: Option<String>,
    pub bold: Option<bool>,
    pub fill: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocBlock {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    List {
        kind: String,
        items: Vec<String>,
    },
    Table {
        rows: Vec<Vec<String>>,
        header_row: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocReplacement {
    pub index: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideChartSeries {
    pub name: String,
    pub values: Vec<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideElementSpec {
    pub id: Option<String>,
    pub kind: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: Option<u32>,
    pub h: Option<u32>,
    pub text: Option<String>,
    pub role: Option<String>,
    pub font_size_pt: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<String>,
    pub fill: Option<String>,
    pub align: Option<String>,
    pub shape_type: Option<String>,
    pub stroke: Option<String>,
    pub rows: Option<Vec<Vec<String>>>,
    pub header_row: Option<bool>,
    pub chart_type: Option<String>,
    pub title: Option<String>,
    pub categories: Option<Vec<String>>,
    pub series: Option<Vec<SlideChartSeries>>,
    pub mime: Option<String>,
    pub data: Option<String>,
    pub alt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardGraphNode {
    pub id: String,
    pub kind: String,
    pub shape: Option<String>,
    pub text: String,
    pub fill: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardGraphEdge {
    pub from_id: String,
    pub to_id: String,
    pub label: Option<String>,
    pub style: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopilotOperation {
    SheetWrite {
        sheet: String,
        range: String,
        cells: Vec<SheetWriteCell>,
    },
    SheetFormat {
        sheet: String,
        range: String,
        fill: Option<String>,
        color: Option<String>,
        bold: Option<bool>,
        num_fmt: Option<String>,
    },
    SheetChart {
        sheet: String,
        chart_type: ChartType,
        data_range: String,
        title: String,
        legend: ChartLegend,
        switch_row_col: bool,
        width: Option<u32>,
        height: Option<u32>,
    },
    SheetTable {
        sheet: String,
        action: String,
        range: Option<String>,
        table_name: Option<String>,
        style: Option<String>,
        total_row: Option<bool>,
    },
    SheetPivot {
        sheet: String,
        source_range: String,
        dest_sheet: Option<String>,
        dest_cell: String,
        row_field: String,
        col_field: Option<String>,
        value_field: String,
        agg: String,
    },
    SheetCondFormat {
        sheet: String,
        range: String,
        preset: String,
        value1: Option<String>,
        value2: Option<String>,
        fill: Option<String>,
        color: Option<String>,
    },
    SheetSortFilter {
        sheet: String,
        action: String,
        range: String,
        col_offset: Option<usize>,
        ascending: Option<bool>,
        query: Option<String>,
    },
    SheetStructure {
        sheet: String,
        action: String,
        index: usize,
        count: usize,
    },
    SheetNote {
        sheet: String,
        coord: String,
        text: String,
    },
    DocAppend {
        blocks: Vec<DocBlock>,
    },
    DocReplaceParagraphs {
        replacements: Vec<DocReplacement>,
    },
    DocReplaceSpan {
        index: usize,
        search: String,
        replace: String,
    },
    DocReplaceRange {
        start_index: Option<usize>,
        end_index: Option<usize>,
        blocks: Vec<DocBlock>,
    },
    DocScrubPii,
    SlideAdd {
        title: String,
        bullets: Vec<String>,
        notes: String,
        layout: Option<String>,
        transition: Option<String>,
        elements: Option<Vec<SlideElementSpec>>,
    },
    SlideUpdate {
        index: Option<usize>,
        title: Option<String>,
        bullets: Option<Vec<String>>,
        notes: Option<String>,
        layout: Option<String>,
        transition: Option<String>,
        elements: Option<Vec<SlideElementSpec>>,
    },
    SlideAddElement {
        slide_index: Option<usize>,
        element: Box<SlideElementSpec>,
    },
    BaseAddField {
        name: String,
        field_type: String,
        options: Vec<String>,
    },
    BaseAddRecords {
        records: Vec<Vec<(String, String)>>,
    },
    BaseUpdateRecords {
        updates: Vec<(usize, Vec<(String, String)>)>,
    },
    BaseSetView {
        view: Option<String>,
        filter_field: Option<String>,
        filter_op: Option<String>,
        filter_value: Option<String>,
        sort_field: Option<String>,
        sort_dir: Option<String>,
        group_field: Option<String>,
    },
    BoardAddGraph {
        direction: String,
        nodes: Vec<BoardGraphNode>,
        edges: Vec<BoardGraphEdge>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopilotToolCall {
    ReadDocBlocks {
        start_index: usize,
        end_index: usize,
    },
    ReadSheetRange {
        sheet: Option<String>,
        range: String,
    },
    FindSheetCells {
        sheet: Option<String>,
        query: String,
    },
    SearchPdfPages {
        query: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotLocation {
    pub module: String,
    pub label: String,
    pub sheet: Option<String>,
    pub range: Option<String>,
    pub paragraph_index: Option<usize>,
    pub slide_index: Option<usize>,
    pub page: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotRunResponse {
    pub mode: CopilotMode,
    pub reply: String,
    pub reasoning: String,
    pub updated_in_place: bool,
    pub operations: Vec<CopilotOperation>,
    /// 모델이 보냈지만 형식·범위·개수 검사에서 버린 작업 수.
    pub dropped_operations: usize,
    pub locations: Vec<CopilotLocation>,
    pub tool_calls: Vec<CopilotToolCall>,
    pub usage: Option<super::usage::TokenUsage>,
}

pub fn write_str_array(out: &mut String, items: &[String]) {
    out.push('[');
    for (idx, s) in items.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_json_str(out, s);
    }
    out.push(']');
}

pub fn write_str_matrix(out: &mut String, rows: &[Vec<String>]) {
    out.push('[');
    for (idx, row) in rows.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        write_str_array(out, row);
    }
    out.push(']');
}

pub fn write_doc_blocks(out: &mut String, blocks: &[DocBlock]) {
    out.push('[');
    for (idx, b) in blocks.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        match b {
            DocBlock::Heading { level, text } => {
                let _ = write!(out, "{{\"heading\":{level},\"text\":");
                push_json_str(out, text);
                out.push('}');
            }
            DocBlock::Paragraph { text } => {
                out.push_str("{\"text\":");
                push_json_str(out, text);
                out.push('}');
            }
            DocBlock::List { kind, items } => {
                out.push_str("{\"list\":");
                push_json_str(out, kind);
                out.push_str(",\"items\":");
                write_str_array(out, items);
                out.push('}');
            }
            DocBlock::Table { rows, header_row } => {
                let _ = write!(out, "{{\"table\":{{\"headerRow\":{header_row},\"rows\":");
                write_str_matrix(out, rows);
                out.push_str("}}");
            }
        }
    }
    out.push(']');
}

pub fn write_slide_element(out: &mut String, el: &SlideElementSpec) {
    out.push_str("{\"kind\":");
    push_json_str(out, &el.kind);
    if let Some(id) = &el.id {
        out.push_str(",\"id\":");
        push_json_str(out, id);
    }
    if let Some(x) = el.x {
        let _ = write!(out, ",\"x\":{x}");
    }
    if let Some(y) = el.y {
        let _ = write!(out, ",\"y\":{y}");
    }
    if let Some(w) = el.w {
        let _ = write!(out, ",\"w\":{w}");
    }
    if let Some(h) = el.h {
        let _ = write!(out, ",\"h\":{h}");
    }
    if let Some(t) = &el.text {
        out.push_str(",\"text\":");
        push_json_str(out, t);
    }
    if let Some(r) = &el.role {
        out.push_str(",\"role\":");
        push_json_str(out, r);
    }
    if let Some(fs) = el.font_size_pt {
        let _ = write!(out, ",\"fontSizePt\":{fs}");
    }
    if let Some(b) = el.bold {
        let _ = write!(out, ",\"bold\":{b}");
    }
    if let Some(it) = el.italic {
        let _ = write!(out, ",\"italic\":{it}");
    }
    if let Some(c) = &el.color {
        out.push_str(",\"color\":");
        push_json_str(out, c);
    }
    if let Some(f) = &el.fill {
        out.push_str(",\"fill\":");
        push_json_str(out, f);
    }
    if let Some(a) = &el.align {
        out.push_str(",\"align\":");
        push_json_str(out, a);
    }
    if let Some(st) = &el.shape_type {
        out.push_str(",\"shapeType\":");
        push_json_str(out, st);
    }
    if let Some(sk) = &el.stroke {
        out.push_str(",\"stroke\":");
        push_json_str(out, sk);
    }
    if let Some(rows) = &el.rows {
        out.push_str(",\"rows\":");
        write_str_matrix(out, rows);
    }
    if let Some(hr) = el.header_row {
        let _ = write!(out, ",\"headerRow\":{hr}");
    }
    if let Some(ct) = &el.chart_type {
        out.push_str(",\"chartType\":");
        push_json_str(out, ct);
    }
    if let Some(title) = &el.title {
        out.push_str(",\"title\":");
        push_json_str(out, title);
    }
    if let Some(cats) = &el.categories {
        out.push_str(",\"categories\":");
        write_str_array(out, cats);
    }
    if let Some(series) = &el.series {
        out.push_str(",\"series\":[");
        for (i, s) in series.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            push_json_str(out, &s.name);
            out.push_str(",\"values\":[");
            for (vi, val) in s.values.iter().enumerate() {
                if vi > 0 {
                    out.push(',');
                }
                out.push_str(val);
            }
            out.push(']');
            if let Some(c) = &s.color {
                out.push_str(",\"color\":");
                push_json_str(out, c);
            }
            out.push('}');
        }
        out.push(']');
    }
    if let Some(mime) = &el.mime {
        out.push_str(",\"mime\":");
        push_json_str(out, mime);
    }
    if let Some(data) = &el.data {
        out.push_str(",\"data\":");
        push_json_str(out, data);
    }
    if let Some(alt) = &el.alt {
        out.push_str(",\"alt\":");
        push_json_str(out, alt);
    }
    out.push('}');
}

pub fn write_tool_call_json(out: &mut String, tc: &CopilotToolCall) {
    match tc {
        CopilotToolCall::ReadDocBlocks {
            start_index,
            end_index,
        } => {
            let _ = write!(
                out,
                "{{\"name\":\"read_doc_blocks\",\"startIndex\":{start_index},\"endIndex\":{end_index}}}"
            );
        }
        CopilotToolCall::ReadSheetRange { sheet, range } => {
            out.push_str("{\"name\":\"read_sheet_range\",\"range\":");
            push_json_str(out, range);
            if let Some(s) = sheet {
                out.push_str(",\"sheet\":");
                push_json_str(out, s);
            }
            out.push('}');
        }
        CopilotToolCall::FindSheetCells { sheet, query } => {
            out.push_str("{\"name\":\"find_sheet_cells\",\"query\":");
            push_json_str(out, query);
            if let Some(s) = sheet {
                out.push_str(",\"sheet\":");
                push_json_str(out, s);
            }
            out.push('}');
        }
        CopilotToolCall::SearchPdfPages { query } => {
            out.push_str("{\"name\":\"search_pdf_pages\",\"query\":");
            push_json_str(out, query);
            out.push('}');
        }
    }
}
