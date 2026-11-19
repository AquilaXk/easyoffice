//! Pages package writer (PD-162 · PD-246): encodes a [`RichDocument`] as an `.pages` package.
//!
//! Writes body paragraphs, headings 1-6, alignment, indents, line spacing, multi-column paragraph
//! layouts, character styles (`bold`, `italic`, `underline`, `strike`, `size`, `font`, `color`,
//! `highlight`, `superscript`/`subscript`), bullet and 9-level numbered lists, external hyperlinks,
//! inline PNG/JPEG pictures, tables (merged regions and cell hyperlinks), page setup (paper size,
//! margins, header/footer insets, duplex facing pages) and section headers/footers (3-zone strips,
//! first-page and even-page variants, page-number and page-count fields). Features that `.pages`
//! cannot represent directly (`RichToc` containers, empty `0x0` tables, `#_Toc<N>` internal
//! anchors, `page_break_before`, top binding/gutter, `FileName` fields) are counted by
//! [`pages_write_loss`] so a save can surface honest warnings.
//!
//! Package rules the writer keeps (each one has a test): ZIP entries are stored, never deflated;
//! every Snappy block of an `.iwa` chunk decompresses to at most 64 KiB; protobuf fields are in
//! ascending order; `MessageInfo.object_references` lists the strong references of each object;
//! the component index (`TSP.PackageMetadata`) names every component, lists `Data/` files and
//! declares every reference that leaves one (a component root in the short form); new object
//! identifiers lie above its field 1; every colour has all its fields; variation styles carry no
//! name; the identity files get new UUIDs on every write; no preview picture is written.
//!
//! 구현 검토: (1) 허용 라이선스의 유지되는 크레이트로 Pages를 쓰는 것은 없음(iwork-rs 는
//! MIT 이나 자체 객체 모델·`snap`·`/dev/urandom` 의존이라 Windows·상한·Fail-Closed 를 다시 짜야 함),
//! (2) 이미 쓰는 의존성에는 protobuf·Snappy 인코더가 없음(워크스페이스 의존성 0), (3) 그래서
//! iwork-rs 의 구조 지식(규칙·필드 번호·구성 요소 색인·표·그림·구역·머리글)만 이식하고 Snappy 리터럴
//! 인코더·protobuf 인코더·binary plist 작성기는 최소로 직접 작성함. 이식한 함수의 출처는 각 파일 머리
//! 주석과 THIRD_PARTY_NOTICES.md 에 있음.

mod body;
mod frame;
mod identity;
mod package;
mod refs;
mod sections;
mod styles;
mod tables;
mod wire;

use crate::pages_schema::root;
use crate::rich_read_pages::read_pages;
use crate::{
    CellMerge, EasyOfficeError, RichBindingKind, RichBlock, RichDocument, RichImageFormat,
    RichPageMargins, RichPaper,
};
use package::{Blueprint, ROOT};
use styles::Styles;
use wire::{float, msg, reference, string, varint};

/// `TP.DocumentArchive`, `TP.ThemeArchive`, the floating-drawables list and the z-order list.
const TYPE_DOCUMENT: u32 = 10000;
const TYPE_THEME: u32 = 10001;
const TYPE_FLOATING_DRAWABLES: u32 = 10010;
const TYPE_ZORDER: u32 = 10015;
const PAPER_NAME: &str = "iso-a4";

/// What a write leaves out, for the save response's warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PagesWriteLoss {
    /// Unrepresentable structural elements (`RichToc` containers and degenerate `0x0` tables).
    pub elements: usize,
    /// Runs, paragraphs, cell links or page-setup attributes that cannot be represented in `.pages`.
    pub formats: usize,
}

impl PagesWriteLoss {
    /// The warnings of a save: one line per kind of loss, none when nothing is lost.
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.elements > 0 {
            out.push(format!("Pages 저장에서 빠진 요소 {}개", self.elements));
        }
        if self.formats > 0 {
            out.push(format!("Pages 저장에서 빠진 서식 {}곳", self.formats));
        }
        out
    }
}

/// Counts what [`pack_pages`] does not write.
pub fn pages_write_loss(doc: &RichDocument) -> PagesWriteLoss {
    let mut loss = PagesWriteLoss::default();
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => {
                loss.elements += usize::from(p.toc.is_some());
                loss.formats += usize::from(body::paragraph_loses_format(p))
                    + p.runs.iter().filter(|r| body::run_loses_format(r)).count();
            }
            RichBlock::Table(t) => {
                if t.rows.is_empty() || t.column_count() == 0 {
                    loss.elements += 1;
                } else {
                    loss.formats += t
                        .validated_links()
                        .iter()
                        .filter(|l| body::valid_pages_link(&l.url).is_none())
                        .count();
                    if !t.col_widths_twips.is_empty() {
                        loss.formats += 1;
                    }
                    if !t.cell_styles.is_empty() {
                        loss.formats += t.validated_cell_styles().len().max(1);
                    }
                }
            }
            RichBlock::Image(_) => {}
        }
    }
    let frame = &doc.page_frame;
    if frame.binding.kind == RichBindingKind::Top || frame.binding.gutter > 0 {
        loss.formats += 1;
    }
    if frame.paper.is_some() || frame.margins.is_some() {
        let paper = frame.paper.unwrap_or(RichPaper::A4);
        let margins = frame.margins.unwrap_or(RichPageMargins {
            top: 1440,
            bottom: 1440,
            header: 720,
            footer: 720,
            left: 1440,
            right: 1440,
        });
        if !paper.holds_bound(&margins, frame.binding) {
            loss.formats += 1;
        }
    }
    for strip in [
        &frame.header,
        &frame.footer,
        &frame.first_header,
        &frame.first_footer,
        &frame.even_header,
        &frame.even_footer,
    ] {
        for p in strip {
            loss.formats += usize::from(body::paragraph_loses_format(p))
                + p.runs.iter().filter(|r| body::run_loses_format(r)).count();
        }
    }
    let validated_notes = doc.validated_notes();
    let validated_comments = doc.validated_comments();
    loss.elements += validated_comments.len()
        + doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                RichBlock::Paragraph(p) => {
                    Some(p.runs.iter().filter(|r| r.revision.is_some()).count())
                }
                _ => None,
            })
            .sum::<usize>();
    loss.formats += validated_notes.len() + usize::from(doc.track_changes);
    loss
}

#[derive(Debug, PartialEq, Eq)]
enum ExpectedBlock {
    Paragraph(String),
    Table(Vec<Vec<String>>, Vec<CellMerge>),
    Image(RichImageFormat, u32, u32),
}

fn expected_blocks(doc: &RichDocument) -> Vec<ExpectedBlock> {
    let (notes, ordinals) = crate::rich_notes::collect_note_ordinals(doc);
    let mut out = Vec::with_capacity(doc.blocks.len() + notes.len());
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => {
                let text: String = p
                    .runs
                    .iter()
                    .filter(|r| !crate::rich_notes::is_deleted_run(r))
                    .map(|r| match r.note_ref {
                        Some(key) => format!("[{}]", ordinals.get(&key).copied().unwrap_or(1)),
                        None => body::clean(&r.text).replace('\u{2028}', " "),
                    })
                    .collect();
                out.push(ExpectedBlock::Paragraph(text));
            }
            RichBlock::Table(t) => {
                let cols = t.column_count();
                if t.rows.is_empty() || cols == 0 {
                    continue;
                }
                let mut has_links = vec![vec![false; cols]; t.rows.len()];
                for l in t.validated_links() {
                    if body::valid_pages_link(&l.url).is_some() {
                        if let Some(cell) = has_links
                            .get_mut(l.row as usize)
                            .and_then(|r| r.get_mut(l.col as usize))
                        {
                            *cell = true;
                        }
                    }
                }
                let grid: Vec<Vec<String>> = t
                    .rows
                    .iter()
                    .enumerate()
                    .map(|(r_idx, r)| {
                        let mut row: Vec<String> = (0..cols)
                            .map(|c_idx| {
                                let raw = r.get(c_idx).map_or("", String::as_str);
                                if has_links[r_idx][c_idx] {
                                    raw.trim_end_matches('\n')
                                        .chars()
                                        .filter_map(|c| match c {
                                            '\n' | '\t' => Some(c),
                                            '\r' | '\u{2028}' | '\u{2029}' => Some(' '),
                                            '\u{FFFC}' => None,
                                            c if c.is_control() => None,
                                            c => Some(c),
                                        })
                                        .collect()
                                } else {
                                    raw.to_string()
                                }
                            })
                            .collect();
                        let _ = &mut row;
                        row
                    })
                    .collect();
                let merges: Vec<CellMerge> = t
                    .validated_merges()
                    .into_iter()
                    .filter(|m| {
                        m.row <= 0xFFFF
                            && m.col <= 0xFFFF
                            && m.row_span <= 0xFFFF
                            && m.col_span <= 0xFFFF
                    })
                    .collect();
                out.push(ExpectedBlock::Table(grid, merges));
                for ci in t.validated_cell_images() {
                    out.push(ExpectedBlock::Image(
                        ci.image.format(),
                        ci.image.width_px(),
                        ci.image.height_px(),
                    ));
                }
            }
            RichBlock::Image(img) => {
                out.push(ExpectedBlock::Image(
                    img.format(),
                    img.width_px(),
                    img.height_px(),
                ));
            }
        }
    }
    for (note, ord) in notes {
        let note_text = body::clean(&format!("[{ord}] {}", note.text.replace('\n', " ")))
            .replace('\u{2028}', " ");
        out.push(ExpectedBlock::Paragraph(note_text));
    }
    while matches!(out.last(), Some(ExpectedBlock::Paragraph(s)) if s.is_empty()) {
        out.pop();
    }
    out
}

/// Writes `doc` as a Pages package.
pub fn pack_pages(doc: &RichDocument) -> Result<Vec<u8>, EasyOfficeError> {
    let mut bp = Blueprint::new();
    let mut styles = Styles::define(&mut bp)?;
    let body_id = bp.allocate();
    let setup = sections::resolve_page_setup(&mut bp, &doc.page_frame)?;
    let section_id = sections::write_section(&mut bp, &mut styles, doc)?;
    let body = body::write_body(
        &mut bp,
        &mut styles,
        doc,
        body_id,
        setup.body_width_pt,
        section_id,
    )?;
    let document = bp.document();
    // Nothing floats on a page, but the list that would name it exists; the z-order holds the
    // body flow alone.
    let floating = bp.add(document, TYPE_FLOATING_DRAWABLES, &[])?;
    let zorder = bp.add(document, TYPE_ZORDER, &[reference(1, body)])?;
    let theme = bp.add(document, TYPE_THEME, &styles.theme())?;
    let stylesheet = styles.stylesheet;
    styles.finish(&mut bp)?;

    let mut root_fields = vec![
        reference(2, stylesheet),
        reference(3, floating),
        reference(root::BODY_STORAGE, body),
        reference(6, theme),
    ];
    if let Some(settings_id) = setup.settings_id {
        root_fields.push(reference(root::SETTINGS, settings_id));
    }
    root_fields.extend([
        // `TSA.DocumentArchive`: locales unset, never chosen by a user, no template.
        msg(
            15,
            vec![msg(
                1,
                vec![varint(10, 1), varint(12, 0), varint(15, 1), varint(16, 1)],
            )],
        ),
        reference(20, zorder),
        float(root::PAGE_WIDTH, setup.page_width_pt),
        float(root::PAGE_HEIGHT, setup.page_height_pt),
        float(root::LEFT_MARGIN, setup.left_pt),
        float(root::RIGHT_MARGIN, setup.right_pt),
        float(root::TOP_MARGIN, setup.top_pt),
        float(root::BOTTOM_MARGIN, setup.bottom_pt),
        float(root::HEADER_MARGIN, setup.header_pt),
        float(root::FOOTER_MARGIN, setup.footer_pt),
        float(38, 1.0),
        varint(39, 0),
        varint(40, 0),
        varint(42, 0),
        string(43, " "),
        string(44, PAPER_NAME),
    ]);
    bp.put(document, ROOT, TYPE_DOCUMENT, &root_fields)?;
    let bytes = bp.finish()?;
    verify_written(&bytes, &expected_blocks(doc), section_id.is_some())?;
    Ok(bytes)
}

/// Reads `bytes` with this crate's own reader and compares the body blocks and section presence.
fn verify_written(
    bytes: &[u8],
    expected: &[ExpectedBlock],
    wrote_section: bool,
) -> Result<(), EasyOfficeError> {
    let (read, _) = read_pages(bytes)?;
    let got: Vec<ExpectedBlock> = read
        .blocks
        .iter()
        .take(if wrote_section {
            expected.len()
        } else {
            read.blocks.len()
        })
        .map(|b| match b {
            RichBlock::Paragraph(p) => ExpectedBlock::Paragraph(p.plain_text()),
            RichBlock::Table(t) => ExpectedBlock::Table(t.rows.clone(), t.merges.clone()),
            RichBlock::Image(img) => {
                ExpectedBlock::Image(img.format(), img.width_px(), img.height_px())
            }
        })
        .collect();
    let frame_ok = !wrote_section || !read.page_frame.is_empty();
    if got == expected && frame_ok {
        Ok(())
    } else {
        Err(EasyOfficeError::CorruptedContainer(
            "Pages 쓰기: 다시 읽은 본문이 쓰려던 본문과 다릅니다".to_string(),
        ))
    }
}
