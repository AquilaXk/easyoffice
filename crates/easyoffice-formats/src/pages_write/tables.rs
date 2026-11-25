//! Table writer for `.pages` packages: `TST.TableInfoArchive` (6000), `TST.TableModelArchive`
//! (6001), `TST.Tile` (6002), string/rich-text `TST.TableDataList` (6005), `MergeRegionMap`
//! (6007) and `TST.RichTextPayloadArchive` (6218) for cells carrying hyperlinks.
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig; see THIRD_PARTY_NOTICES.md),
//! `src/table.rs`, `src/create.rs`, `FORMAT.md` §5.

use std::collections::{BTreeMap, HashMap};

use super::body::{drawable_archive_fields, valid_pages_link};
use super::package::{Blueprint, ComponentId};
use super::styles::{self, Styles};
use super::wire::{bytes, float, msg, reference, string, varint, Fld};
use crate::pages_schema::{cell, hyperlink, storage, table, ty};
use crate::pages_table::{MAX_TABLE_CELLS, MAX_TABLE_COLS, MAX_TABLE_ROWS};
use crate::rich_table_style::{TableBorderEdge, TableBorderKind, TableCellStyleEntry};
use crate::{EasyOfficeError, RichTable, TableCellLink};

const MAX_TABLES: usize = 2_000;
const MAX_TOTAL_CELLS: usize = 4_000_000;
const MAX_LIST_ENTRIES: usize = 2_000_000;
const MAX_CELL_MARK_ENTRIES: usize = 50_000;
const TYPE_MERGE_REGION_MAP: u32 = 6007;
const TILE_ROWS: usize = 256;

#[derive(Default)]
pub(super) struct TableWriteBudget {
    tables: usize,
    total_cells: usize,
    total_list_entries: usize,
}

impl TableWriteBudget {
    fn charge_table(&mut self, rows: usize, cols: usize) -> Result<(), EasyOfficeError> {
        self.tables = self.tables.saturating_add(1);
        if self.tables > MAX_TABLES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 쓰기: 표 수가 열기 한도를 넘었습니다".to_string(),
            ));
        }
        if rows > MAX_TABLE_ROWS || cols > MAX_TABLE_COLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 쓰기: 표 행·열 수가 열기 한도를 넘었습니다".to_string(),
            ));
        }
        let cells = rows
            .checked_mul(cols)
            .filter(|n| *n <= MAX_TABLE_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 쓰기: 단일 표 셀 수가 열기 한도를 넘었습니다".to_string(),
                )
            })?;
        self.total_cells = self
            .total_cells
            .checked_add(cells)
            .filter(|n| *n <= MAX_TOTAL_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 쓰기: 문서 전체 표 셀 수가 열기 한도를 넘었습니다".to_string(),
                )
            })?;
        Ok(())
    }

    fn charge_list_entry(&mut self) -> Result<u32, EasyOfficeError> {
        self.total_list_entries = self
            .total_list_entries
            .checked_add(1)
            .filter(|n| *n <= MAX_LIST_ENTRIES)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 쓰기: 표 데이터 목록 항목 수가 한도를 넘었습니다".to_string(),
                )
            })?;
        Ok(self.total_list_entries as u32)
    }
}

fn pack_string_cell(buf: &mut Vec<u8>, key: u32, cell_style_key: Option<u32>) {
    let flags = (1u32 << cell::LEGACY_BIT_STRING_KEY)
        | cell_style_key.map_or(0, |_| 1u32 << cell::LEGACY_BIT_CELL_STYLE);
    buf.extend_from_slice(&[cell::LEGACY_VERSION, cell::TYPE_STRING, 0, 0, 0, 0, 0, 0]);
    buf.extend_from_slice(&flags.to_le_bytes());
    buf.extend_from_slice(&key.to_le_bytes());
    if let Some(sk) = cell_style_key {
        buf.extend_from_slice(&sk.to_le_bytes());
    }
}

fn pack_rich_text_cell(buf: &mut Vec<u8>, key: u32, cell_style_key: Option<u32>) {
    let flags = cell::LEGACY_FLAG_RICH_TEXT_KEY
        | cell_style_key.map_or(0, |_| 1u32 << cell::LEGACY_BIT_CELL_STYLE);
    buf.extend_from_slice(&[cell::LEGACY_VERSION, cell::TYPE_RICH_TEXT, 0, 0, 0, 0, 0, 0]);
    buf.extend_from_slice(&flags.to_le_bytes());
    buf.extend_from_slice(&key.to_le_bytes());
    if let Some(sk) = cell_style_key {
        buf.extend_from_slice(&sk.to_le_bytes());
    }
}

fn pack_empty_styled_cell(buf: &mut Vec<u8>, cell_style_key: u32) {
    buf.extend_from_slice(&[cell::LEGACY_VERSION, cell::TYPE_EMPTY, 0, 0, 0, 0, 0, 0]);
    buf.extend_from_slice(&(1u32 << cell::LEGACY_BIT_CELL_STYLE).to_le_bytes());
    buf.extend_from_slice(&cell_style_key.to_le_bytes());
}

fn encode_stroke(edge: TableBorderEdge) -> Vec<Fld> {
    let [_, r, g, b] = edge.rgb().to_be_bytes();
    let kind = edge.kind();
    let (color_rgb, width, pattern_fields) = match kind {
        TableBorderKind::None => (
            [0u8, 0, 0],
            0.0f32,
            vec![
                varint(table::STROKE_PATTERN_TYPE, table::PATTERN_NONE),
                float(2, 0.0),
                varint(3, 0),
            ],
        ),
        TableBorderKind::Thin => (
            [r, g, b],
            1.0f32,
            vec![
                varint(table::STROKE_PATTERN_TYPE, table::PATTERN_SOLID),
                float(2, 0.0),
                varint(3, 0),
            ],
        ),
        TableBorderKind::Thick | TableBorderKind::Double => (
            [r, g, b],
            2.0f32,
            vec![
                varint(table::STROKE_PATTERN_TYPE, table::PATTERN_SOLID),
                float(2, 0.0),
                varint(3, 0),
            ],
        ),
        TableBorderKind::Dashed => (
            [r, g, b],
            1.0f32,
            vec![
                varint(table::STROKE_PATTERN_TYPE, table::PATTERN_DASHED),
                float(table::STROKE_PATTERN_DASH, 4.0),
            ],
        ),
        TableBorderKind::Dotted => (
            [r, g, b],
            1.0f32,
            vec![
                varint(table::STROKE_PATTERN_TYPE, table::PATTERN_DASHED),
                float(table::STROKE_PATTERN_DASH, 1.5),
            ],
        ),
        TableBorderKind::Unspecified => unreachable!(),
    };
    vec![
        msg(table::STROKE_COLOR, styles::rgb(color_rgb)),
        float(table::STROKE_WIDTH, width),
        msg(table::STROKE_PATTERN, pattern_fields),
    ]
}

fn emit_axis_layers(
    bp: &mut Blueprint,
    doc_comp: ComponentId,
    grouped: BTreeMap<usize, Vec<(usize, TableBorderEdge)>>,
    next_order: &mut u64,
) -> Result<Vec<u64>, EasyOfficeError> {
    let mut layer_ids = Vec::with_capacity(grouped.len());
    for (axis_idx, items) in grouped {
        let mut layer_fields = vec![varint(table::STROKE_LAYER_INDEX, axis_idx as u64)];
        let mut cur_run: Option<(usize, usize, TableBorderEdge)> = None;
        let flush_run =
            |run: (usize, usize, TableBorderEdge), order: &mut u64, out: &mut Vec<Fld>| {
                *order += 1;
                out.push(msg(
                    table::STROKE_LAYER_RUNS,
                    vec![
                        varint(table::STROKE_RUN_ORIGIN, run.0 as u64),
                        varint(table::STROKE_RUN_LENGTH, run.1 as u64),
                        msg(table::STROKE_RUN_STROKE, encode_stroke(run.2)),
                        varint(table::STROKE_RUN_ORDER, *order),
                    ],
                ));
            };
        for (pos, edge) in items {
            match &mut cur_run {
                Some((orig, len, prev_edge)) if *orig + *len == pos && *prev_edge == edge => {
                    *len += 1;
                }
                Some(run) => {
                    flush_run(*run, next_order, &mut layer_fields);
                    cur_run = Some((pos, 1, edge));
                }
                None => cur_run = Some((pos, 1, edge)),
            }
        }
        if let Some(run) = cur_run {
            flush_run(run, next_order, &mut layer_fields);
        }
        layer_ids.push(bp.add(doc_comp, ty::STROKE_LAYER, &layer_fields)?);
    }
    Ok(layer_ids)
}

fn write_stroke_sidecar(
    bp: &mut Blueprint,
    doc_comp: ComponentId,
    rows: usize,
    cols: usize,
    styled: &[TableCellStyleEntry],
) -> Result<Option<u64>, EasyOfficeError> {
    let mut left_cols: BTreeMap<usize, Vec<(usize, TableBorderEdge)>> = BTreeMap::new();
    let mut right_cols: BTreeMap<usize, Vec<(usize, TableBorderEdge)>> = BTreeMap::new();
    let mut top_rows: BTreeMap<usize, Vec<(usize, TableBorderEdge)>> = BTreeMap::new();
    let mut bottom_rows: BTreeMap<usize, Vec<(usize, TableBorderEdge)>> = BTreeMap::new();
    for e in styled {
        let (r, c) = (e.row as usize, e.col as usize);
        if !e.borders.left.is_unspecified() {
            left_cols.entry(c).or_default().push((r, e.borders.left));
        }
        if !e.borders.right.is_unspecified() {
            right_cols.entry(c).or_default().push((r, e.borders.right));
        }
        if !e.borders.top.is_unspecified() {
            top_rows.entry(r).or_default().push((c, e.borders.top));
        }
        if !e.borders.bottom.is_unspecified() {
            bottom_rows
                .entry(r)
                .or_default()
                .push((c, e.borders.bottom));
        }
    }
    for list in left_cols
        .values_mut()
        .chain(right_cols.values_mut())
        .chain(top_rows.values_mut())
        .chain(bottom_rows.values_mut())
    {
        list.sort_by_key(|(pos, _)| *pos);
    }
    if left_cols.is_empty()
        && right_cols.is_empty()
        && top_rows.is_empty()
        && bottom_rows.is_empty()
    {
        return Ok(None);
    }
    let mut next_order = 0u64;
    let left_ids = emit_axis_layers(bp, doc_comp, left_cols, &mut next_order)?;
    let right_ids = emit_axis_layers(bp, doc_comp, right_cols, &mut next_order)?;
    let top_ids = emit_axis_layers(bp, doc_comp, top_rows, &mut next_order)?;
    let bottom_ids = emit_axis_layers(bp, doc_comp, bottom_rows, &mut next_order)?;

    let mut sidecar = vec![
        varint(table::STROKE_SIDECAR_MAX_ORDER, next_order),
        varint(table::STROKE_SIDECAR_COLS, cols as u64),
        varint(table::STROKE_SIDECAR_ROWS, rows as u64),
    ];
    sidecar.extend(
        left_ids
            .into_iter()
            .map(|id| reference(table::STROKE_SIDECAR_LEFT_COLS, id)),
    );
    sidecar.extend(
        right_ids
            .into_iter()
            .map(|id| reference(table::STROKE_SIDECAR_RIGHT_COLS, id)),
    );
    sidecar.extend(
        top_ids
            .into_iter()
            .map(|id| reference(table::STROKE_SIDECAR_TOP_ROWS, id)),
    );
    sidecar.extend(
        bottom_ids
            .into_iter()
            .map(|id| reference(table::STROKE_SIDECAR_BOTTOM_ROWS, id)),
    );
    Ok(Some(bp.add(doc_comp, ty::STROKE_SIDECAR, &sidecar)?))
}

fn storage_entry(index: u64, object_id: Option<u64>) -> Fld {
    let mut fields = vec![varint(storage::ENTRY_INDEX, index)];
    if let Some(id) = object_id {
        fields.push(reference(storage::ENTRY_OBJECT, id));
    }
    msg(storage::TABLE_ENTRIES, fields)
}

fn write_rich_cell_payload(
    bp: &mut Blueprint,
    styles: &Styles,
    raw: &str,
    links: &[&TableCellLink],
) -> Result<u64, EasyOfficeError> {
    let doc_comp = bp.document();
    let trimmed = raw.trim_end_matches('\n');
    let mut cleaned = String::with_capacity(trimmed.len());
    let mut para_entries = vec![storage_entry(0, Some(styles.body))];
    let mut smart_entries: Vec<Fld> = Vec::new();
    let mut u16_pos = 0u64;
    let mut link_idx = 0usize;
    let mut active_end: Option<usize> = None;

    for (byte_pos, ch) in trimmed.char_indices() {
        if let Some(end_b) = active_end {
            if byte_pos >= end_b {
                smart_entries.push(storage_entry(u16_pos, None));
                active_end = None;
            }
        }
        while link_idx < links.len() && (links[link_idx].end as usize) <= byte_pos {
            link_idx += 1;
        }
        if active_end.is_none() && link_idx < links.len() {
            let candidate = links[link_idx];
            if (candidate.start as usize) == byte_pos && (candidate.end as usize) <= trimmed.len() {
                if let Some(url) = valid_pages_link(&candidate.url) {
                    if smart_entries.len() + 2 > MAX_CELL_MARK_ENTRIES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "Pages 쓰기: 표 셀 하이퍼링크 항목 수가 한도를 넘었습니다".to_string(),
                        ));
                    }
                    let hid = bp.add(doc_comp, ty::HYPERLINK, &[string(hyperlink::URL, url)])?;
                    smart_entries.push(storage_entry(u16_pos, Some(hid)));
                    active_end = Some(candidate.end as usize);
                }
                link_idx += 1;
            }
        }
        match ch {
            '\n' => {
                if active_end.is_some() {
                    smart_entries.push(storage_entry(u16_pos, None));
                    active_end = None;
                }
                cleaned.push('\n');
                u16_pos += 1;
                para_entries.push(storage_entry(u16_pos, Some(styles.body)));
            }
            '\t' => {
                cleaned.push('\t');
                u16_pos += 1;
            }
            '\r' | '\u{2028}' | '\u{2029}' => {
                cleaned.push(' ');
                u16_pos += 1;
            }
            '\u{FFFC}' => {}
            c if c.is_control() => {}
            c => {
                cleaned.push(c);
                u16_pos += c.len_utf16() as u64;
            }
        }
    }
    if active_end.is_some() {
        smart_entries.push(storage_entry(u16_pos, None));
    }

    let mut st_fields = vec![varint(1, 5), reference(2, styles.stylesheet)];
    if !cleaned.is_empty() {
        st_fields.push(string(storage::TEXT, &cleaned));
    }
    st_fields.push(msg(storage::PARA_STYLE_TABLE, para_entries));
    if !smart_entries.is_empty() {
        st_fields.push(msg(storage::SMARTFIELD_TABLE, smart_entries));
    }
    let st_id = bp.add(doc_comp, ty::STORAGE, &st_fields)?;
    bp.add(
        doc_comp,
        ty::RICH_TEXT_PAYLOAD,
        &[reference(table::PAYLOAD_STORAGE, st_id)],
    )
}

const DEFAULT_ROW_HEIGHT_PT: f32 = 22.0;
const MIN_COL_WIDTH_PT: f32 = 36.0;

/// Writes `tbl` into the `Document` component and returns the `TSWP.DrawableAttachment` (2003)
/// identifier, or `None` when `tbl` has zero rows or zero columns.
pub(super) fn write_table_attachment(
    bp: &mut Blueprint,
    styles: &mut Styles,
    tbl: &RichTable,
    body_width_pt: f32,
    budget: &mut TableWriteBudget,
) -> Result<Option<u64>, EasyOfficeError> {
    let rows = tbl.rows.len();
    let cols = tbl.column_count();
    if rows == 0 || cols == 0 {
        return Ok(None);
    }
    budget.charge_table(rows, cols)?;

    let validated_links = tbl.validated_links();
    let mut cell_links: HashMap<(usize, usize), Vec<&TableCellLink>> = HashMap::new();
    for link in &validated_links {
        if valid_pages_link(&link.url).is_some() {
            cell_links
                .entry((link.row as usize, link.col as usize))
                .or_default()
                .push(link);
        }
    }

    let validated_styles = tbl.validated_cell_styles();
    let mut fill_by_cell: HashMap<(usize, usize), [u8; 3]> = HashMap::new();
    for entry in &validated_styles {
        if let Some(rgb) = entry.fill.rgb() {
            let [_, r, g, b] = rgb.to_be_bytes();
            fill_by_cell.insert((entry.row as usize, entry.col as usize), [r, g, b]);
        }
    }

    let doc_comp = bp.document();
    let mut string_keys: HashMap<&str, u32> = HashMap::new();
    let mut style_keys: HashMap<[u8; 3], u32> = HashMap::new();
    let mut string_entries: Vec<Fld> = Vec::new();
    let mut style_entries: Vec<Fld> = Vec::new();
    let mut rich_entries: Vec<Fld> = Vec::new();
    let mut next_string_key = 0u32;
    let mut next_style_key = 0u32;
    let mut next_rich_key = 0u32;
    let mut tile_entries: Vec<Fld> = Vec::new();

    for (tile_idx, row_chunk) in tbl.rows.chunks(TILE_ROWS).enumerate() {
        let mut tile_row_msgs: Vec<Fld> = Vec::new();
        for (r_in_tile, row_cells) in row_chunk.iter().enumerate() {
            let abs_row = tile_idx * TILE_ROWS + r_in_tile;
            let mut row_buf: Vec<u8> = Vec::new();
            let mut row_offsets: Vec<u8> = Vec::with_capacity(cols * 2);
            let mut has_any_cell = false;

            for col_idx in 0..cols {
                let text = row_cells.get(col_idx).map_or("", String::as_str);
                let style_key = if let Some(&rgb) = fill_by_cell.get(&(abs_row, col_idx)) {
                    Some(if let Some(&existing) = style_keys.get(&rgb) {
                        existing
                    } else {
                        budget.charge_list_entry()?;
                        next_style_key += 1;
                        let style_id = styles.table_cell_fill(bp, rgb)?;
                        style_keys.insert(rgb, next_style_key);
                        style_entries.push(msg(
                            table::LIST_ENTRIES,
                            vec![
                                varint(table::ENTRY_KEY, u64::from(next_style_key)),
                                reference(table::ENTRY_CELL_STYLE, style_id),
                            ],
                        ));
                        next_style_key
                    })
                } else {
                    None
                };

                if let Some(links) = cell_links.get(&(abs_row, col_idx)) {
                    let offset = u16::try_from(row_buf.len())
                        .ok()
                        .filter(|o| *o < table::NO_CELL_OFFSET)
                        .ok_or_else(|| {
                            EasyOfficeError::ResourceBudgetExceeded(
                                "Pages 쓰기: 표 행 셀 버퍼 크기 한도를 넘었습니다".to_string(),
                            )
                        })?;
                    budget.charge_list_entry()?;
                    next_rich_key += 1;
                    let payload_id = write_rich_cell_payload(bp, styles, text, links)?;
                    rich_entries.push(msg(
                        table::LIST_ENTRIES,
                        vec![
                            varint(table::ENTRY_KEY, u64::from(next_rich_key)),
                            reference(table::ENTRY_RICH_TEXT, payload_id),
                        ],
                    ));
                    pack_rich_text_cell(&mut row_buf, next_rich_key, style_key);
                    row_offsets.extend_from_slice(&offset.to_le_bytes());
                    has_any_cell = true;
                } else if !text.is_empty() {
                    let offset = u16::try_from(row_buf.len())
                        .ok()
                        .filter(|o| *o < table::NO_CELL_OFFSET)
                        .ok_or_else(|| {
                            EasyOfficeError::ResourceBudgetExceeded(
                                "Pages 쓰기: 표 행 셀 버퍼 크기 한도를 넘었습니다".to_string(),
                            )
                        })?;
                    let key = if let Some(&existing) = string_keys.get(text) {
                        existing
                    } else {
                        budget.charge_list_entry()?;
                        next_string_key += 1;
                        string_keys.insert(text, next_string_key);
                        string_entries.push(msg(
                            table::LIST_ENTRIES,
                            vec![
                                varint(table::ENTRY_KEY, u64::from(next_string_key)),
                                string(table::ENTRY_STRING, text),
                            ],
                        ));
                        next_string_key
                    };
                    pack_string_cell(&mut row_buf, key, style_key);
                    row_offsets.extend_from_slice(&offset.to_le_bytes());
                    has_any_cell = true;
                } else if let Some(sk) = style_key {
                    let offset = u16::try_from(row_buf.len())
                        .ok()
                        .filter(|o| *o < table::NO_CELL_OFFSET)
                        .ok_or_else(|| {
                            EasyOfficeError::ResourceBudgetExceeded(
                                "Pages 쓰기: 표 행 셀 버퍼 크기 한도를 넘었습니다".to_string(),
                            )
                        })?;
                    pack_empty_styled_cell(&mut row_buf, sk);
                    row_offsets.extend_from_slice(&offset.to_le_bytes());
                    has_any_cell = true;
                } else {
                    row_offsets.extend_from_slice(&table::NO_CELL_OFFSET.to_le_bytes());
                }
            }

            if has_any_cell {
                tile_row_msgs.push(msg(
                    table::TILE_ROWS,
                    vec![
                        varint(table::ROW_INDEX, r_in_tile as u64),
                        bytes(table::ROW_CELL_BUFFER, row_buf),
                        bytes(table::ROW_CELL_OFFSETS, row_offsets),
                    ],
                ));
            }
        }
        let tile_id = bp.add(doc_comp, ty::TABLE_TILE, &tile_row_msgs)?;
        tile_entries.push(msg(
            table::TILES_ENTRIES,
            vec![
                varint(table::TILE_ID, tile_idx as u64),
                reference(table::TILE_REF, tile_id),
            ],
        ));
    }

    tile_entries.push(varint(table::TILES_SIZE, table::DEFAULT_TILE_ROWS));
    let validated_widths = tbl.validated_col_widths();
    let mut store_fields = Vec::new();
    if let Some(widths) = &validated_widths {
        let entries: Vec<Fld> = widths
            .iter()
            .enumerate()
            .map(|(col_idx, &w_tw)| {
                msg(
                    table::HEADER_BUCKET_ENTRIES,
                    vec![
                        varint(table::HEADER_ENTRY_INDEX, col_idx as u64),
                        float(table::HEADER_ENTRY_SIZE, w_tw as f32 / 20.0),
                    ],
                )
            })
            .collect();
        let bucket_id = bp.add(doc_comp, ty::TABLE_HEADER_BUCKET, &entries)?;
        store_fields.push(reference(table::STORE_COL_HEADERS, bucket_id));
    }
    store_fields.push(msg(table::STORE_TILES, tile_entries));

    if !string_entries.is_empty() {
        let mut list_fields = vec![
            varint(1, 1),
            varint(2, u64::from(next_string_key).saturating_add(1)),
        ];
        list_fields.extend(string_entries);
        let str_list_id = bp.add(doc_comp, ty::TABLE_DATA_LIST, &list_fields)?;
        store_fields.push(reference(table::STORE_STRING_LIST, str_list_id));
    }

    if !style_entries.is_empty() {
        let mut list_fields = vec![
            varint(1, table::LIST_KIND_STYLE),
            varint(2, u64::from(next_style_key).saturating_add(1)),
        ];
        list_fields.extend(style_entries);
        let style_list_id = bp.add(doc_comp, ty::TABLE_DATA_LIST, &list_fields)?;
        store_fields.push(reference(table::STORE_STYLE_LIST, style_list_id));
    }

    let merges = tbl.validated_merges();
    if !merges.is_empty() {
        let mut merge_fields = Vec::with_capacity(merges.len());
        for m in merges {
            if m.row <= 0xFFFF && m.col <= 0xFFFF && m.row_span <= 0xFFFF && m.col_span <= 0xFFFF {
                let origin = (u64::from(m.row) << 16) | u64::from(m.col);
                let size = (u64::from(m.row_span) << 16) | u64::from(m.col_span);
                merge_fields.push(msg(
                    table::MERGE_RANGES,
                    vec![
                        varint(table::RANGE_ORIGIN, origin),
                        varint(table::RANGE_SIZE, size),
                    ],
                ));
            }
        }
        if !merge_fields.is_empty() {
            let merge_map_id = bp.add(doc_comp, TYPE_MERGE_REGION_MAP, &merge_fields)?;
            store_fields.push(reference(table::STORE_MERGE_REGION_MAP, merge_map_id));
        }
    }

    if !rich_entries.is_empty() {
        let mut list_fields = vec![
            varint(1, 9),
            varint(2, u64::from(next_rich_key).saturating_add(1)),
        ];
        list_fields.extend(rich_entries);
        let rt_list_id = bp.add(doc_comp, ty::TABLE_DATA_LIST, &list_fields)?;
        store_fields.push(reference(table::STORE_RICH_TEXT_LIST, rt_list_id));
    }

    let stroke_sidecar_id = write_stroke_sidecar(bp, doc_comp, rows, cols, &validated_styles)?;

    let model_id = bp.allocate();
    let table_uuid = format!("00000000-0000-4000-8000-{model_id:012x}");
    let default_col_width_pt = (body_width_pt / cols as f32).max(MIN_COL_WIDTH_PT);
    let table_w_pt = validated_widths.as_ref().map_or(
        (default_col_width_pt * cols as f32).max(MIN_COL_WIDTH_PT),
        |ws| {
            ws.iter()
                .map(|&tw| tw as f32 / 20.0)
                .sum::<f32>()
                .max(MIN_COL_WIDTH_PT)
        },
    );
    let table_h_pt = (DEFAULT_ROW_HEIGHT_PT * rows as f32).max(DEFAULT_ROW_HEIGHT_PT);

    let mut model_fields = vec![
        string(table::MODEL_ID, &table_uuid),
        msg(table::MODEL_DATA_STORE, store_fields),
        varint(table::MODEL_ROWS, rows as u64),
        varint(table::MODEL_COLS, cols as u64),
        float(table::MODEL_DEFAULT_ROW_HEIGHT, DEFAULT_ROW_HEIGHT_PT),
        float(table::MODEL_DEFAULT_COL_WIDTH, default_col_width_pt),
    ];
    if let Some(sidecar_id) = stroke_sidecar_id {
        model_fields.push(reference(table::MODEL_STROKE_SIDECAR, sidecar_id));
    }

    bp.put(doc_comp, model_id, ty::TABLE_MODEL, &model_fields)?;
    let info_id = bp.add(
        doc_comp,
        ty::TABLE_INFO,
        &[
            msg(
                table::INFO_DRAWABLE,
                drawable_archive_fields(table_w_pt, table_h_pt, None),
            ),
            reference(table::INFO_MODEL, model_id),
        ],
    )?;
    let att_id = bp.add(doc_comp, ty::ATTACHMENT, &[reference(1, info_id)])?;
    Ok(Some(att_id))
}
