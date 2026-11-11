//! Minimal Pages writer (experimental, PD-162): a [`RichDocument`] as an `.pages` package.
//!
//! Scope of this first step: body paragraphs, headings 1-6, alignment and the character formats
//! bold, italic, underline, size and colour. Tables, pictures and list markers are not written;
//! [`pages_write_loss`] counts them so a save can say so.
//!
//! Package rules the writer keeps (each one has a test): ZIP entries are stored, never deflated;
//! every Snappy block of an `.iwa` chunk decompresses to at most 64 KiB; protobuf fields are in
//! ascending order; `MessageInfo.object_references` lists the strong references of each object;
//! the component index (`TSP.PackageMetadata`) names every component and declares every reference
//! that leaves one (a component root in the short form); new object identifiers lie above its
//! field 1; every colour has all its fields; variation styles carry no name; the identity files get
//! new UUIDs on every write; no preview picture is written.
//!
//! The written package is read back with this crate's own reader and its text compared with what
//! was meant to be written (the rule of the reference app: the next engine that opens the file
//! must accept it). That check is circular: it shows that writer and reader agree, not that
//! Pages opens the file.
//!
//! 구현 검토: (1) 허용 라이선스의 유지되는 크레이트로 Pages를 쓰는 것은 없음(iwork-rs 는
//! MIT 이나 자체 객체 모델·`snap`·`/dev/urandom` 의존이라 Windows·상한·Fail-Closed 를 다시 짜야 함),
//! (2) 이미 쓰는 의존성에는 protobuf·Snappy 인코더가 없음(워크스페이스 의존성 0), (3) 그래서
//! iwork-rs 의 구조 지식(규칙·필드 번호·구성 요소 색인)만 이식하고 Snappy 리터럴 인코더·protobuf
//! 인코더·binary plist 작성기는 최소로 직접 작성함. 이식한 함수의 출처는 각 파일 머리 주석과
//! THIRD_PARTY_NOTICES.md 에 있음.

mod body;
mod frame;
mod identity;
mod package;
mod refs;
mod styles;
mod wire;

use crate::rich_read_pages::read_pages;
use crate::{EasyOfficeError, RichBlock, RichDocument};
use package::{Blueprint, ROOT};
use styles::Styles;
use wire::{float, msg, reference, string, varint};

/// `TP.DocumentArchive`, `TP.ThemeArchive`, the floating-drawables list and the z-order list.
const TYPE_DOCUMENT: u32 = 10000;
const TYPE_THEME: u32 = 10001;
const TYPE_FLOATING_DRAWABLES: u32 = 10010;
const TYPE_ZORDER: u32 = 10015;

/// A4 in points (210 x 297 mm), margins of one inch and half-inch header and footer insets.
const PAGE_WIDTH: f32 = 595.28;
const PAGE_HEIGHT: f32 = 841.89;
const MARGIN: f32 = 72.0;
const HEADER_FOOTER_INSET: f32 = 36.0;
const PAPER_NAME: &str = "iso-a4";

/// What a write leaves out, for the save response's warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PagesWriteLoss {
    /// Tables, pictures and list paragraphs (the text of a list paragraph stays, its marker goes).
    pub elements: usize,
    /// Runs and paragraphs whose strikethrough, link, font, highlight, script, indent or line
    /// spacing is not written.
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
                loss.elements += usize::from(p.list.is_some());
                loss.formats += usize::from(body::paragraph_loses_format(p))
                    + p.runs.iter().filter(|r| body::run_loses_format(r)).count();
            }
            RichBlock::Table(_) | RichBlock::Image(_) => loss.elements += 1,
        }
    }
    loss
}

/// The text each paragraph is meant to have after reading, trailing empty ones dropped (the
/// reader drops them too).
fn expected_texts(doc: &RichDocument) -> Vec<String> {
    let mut texts: Vec<String> = body::paragraphs(doc)
        .map(|p| {
            p.runs
                .iter()
                .map(|r| body::clean(&r.text).replace('\u{2028}', " "))
                .collect()
        })
        .collect();
    while texts.last().is_some_and(String::is_empty) {
        texts.pop();
    }
    texts
}

/// Writes `doc` as a Pages package.
pub fn pack_pages(doc: &RichDocument) -> Result<Vec<u8>, EasyOfficeError> {
    let mut bp = Blueprint::new();
    let mut styles = Styles::define(&mut bp)?;
    let body = body::write_body(&mut bp, &mut styles, doc)?;
    let document = bp.document();
    // Nothing floats on a page, but the list that would name it exists; the z-order holds the
    // body flow alone.
    let floating = bp.add(document, TYPE_FLOATING_DRAWABLES, &[])?;
    let zorder = bp.add(document, TYPE_ZORDER, &[reference(1, body)])?;
    let theme = bp.add(document, TYPE_THEME, &styles.theme())?;
    let stylesheet = styles.stylesheet;
    styles.finish(&mut bp)?;
    bp.put(
        document,
        ROOT,
        TYPE_DOCUMENT,
        &[
            reference(2, stylesheet),
            reference(3, floating),
            reference(4, body),
            reference(6, theme),
            // `TSA.DocumentArchive`: locales unset, never chosen by a user, no template.
            msg(
                15,
                vec![msg(
                    1,
                    vec![varint(10, 1), varint(12, 0), varint(15, 1), varint(16, 1)],
                )],
            ),
            reference(20, zorder),
            float(30, PAGE_WIDTH),
            float(31, PAGE_HEIGHT),
            float(32, MARGIN),
            float(33, MARGIN),
            float(34, MARGIN),
            float(35, MARGIN),
            float(36, HEADER_FOOTER_INSET),
            float(37, HEADER_FOOTER_INSET),
            float(38, 1.0),
            varint(39, 0),
            varint(40, 0),
            varint(42, 0),
            string(43, " "),
            string(44, PAPER_NAME),
        ],
    )?;
    let bytes = bp.finish()?;
    verify_written(&bytes, &expected_texts(doc))?;
    Ok(bytes)
}

/// Reads `bytes` with this crate's own reader and compares the paragraph text.
fn verify_written(bytes: &[u8], expected: &[String]) -> Result<(), EasyOfficeError> {
    let (read, _) = read_pages(bytes)?;
    let got: Vec<String> = body::paragraphs(&read).map(|p| p.plain_text()).collect();
    if got == expected {
        Ok(())
    } else {
        Err(EasyOfficeError::CorruptedContainer(
            "Pages 쓰기: 다시 읽은 본문이 쓰려던 본문과 다릅니다".to_string(),
        ))
    }
}
