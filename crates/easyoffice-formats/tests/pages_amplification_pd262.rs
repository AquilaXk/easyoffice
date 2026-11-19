//! PD-262: Pages notes and shapes named by many marks must be read once, not once per mark.

#[allow(dead_code)]
mod image_fixture;
mod pages_fixture;
mod pages_picture_fixture;

use std::time::Instant;

use pages_fixture::*;
use pages_picture_fixture::body_with;

const FFFC: &str = "\u{FFFC}";

/// 시간 상한 시험의 여유 폭: 수정 후 코드는 이 값의 1/10 안에 끝난다.
const LINEAR_TIME_LIMIT_SECS: u64 = 5;

fn body_with_footnotes(b: &mut DocBuilder, text: &str, marks: &[(u64, u64)]) -> u64 {
    let table: Vec<(u64, Option<u64>)> = marks.iter().map(|(i, f)| (*i, Some(*f))).collect();
    let payload = cat(&[
        DocBuilder::storage_payload(text, &[], &[], &[], &[]),
        f_bytes(11, &DocBuilder::attr_table(&table)),
    ]);
    b.add(2001, &payload)
}

/// 속성 테이블 항목이 많은 각주 하나를 표식이 줄줄이 가리킨다. 표식마다 저장소를 다시 읽으면
/// 표식 수 x 항목 수만큼 일한다.
fn heavy_footnote(b: &mut DocBuilder, entries: u64) -> u64 {
    let chars: Vec<(u64, Option<u64>)> = (0..entries).map(|i| (i, None)).collect();
    let st = b.add(
        2001,
        &DocBuilder::storage_payload("주석", &[], &chars, &[], &[]),
    );
    b.add(2012, &f_ref(2, st))
}

fn body_of_marks(b: &mut DocBuilder, note: u64, marks_len: u64) -> u64 {
    let marks: Vec<(u64, u64)> = (0..marks_len).map(|i| (i, note)).collect();
    let text = FFFC.repeat(marks_len as usize);
    body_with_footnotes(b, &text, &marks)
}

fn decode_secs(bytes: &[u8]) -> f64 {
    let start = Instant::now();
    let result = easyoffice_formats::UniversalFormatConverter::decode_rich(bytes, Some("a.pages"));
    let took = start.elapsed();
    eprintln!(
        "input {} bytes, took {took:?}, ok={}",
        bytes.len(),
        result.is_ok()
    );
    took.as_secs_f64()
}

#[test]
fn one_footnote_named_by_many_marks_is_read_once() {
    let mut b = DocBuilder::default();
    let note = heavy_footnote(&mut b, 20_000);
    let body = body_of_marks(&mut b, note, 20_000);
    let secs = decode_secs(&pages_zip(b.finish(body)));
    assert!(secs < LINEAR_TIME_LIMIT_SECS as f64, "decode took {secs}s");
}

#[test]
fn one_shape_anchored_many_times_is_read_once() {
    let mut b = DocBuilder::default();
    let chars: Vec<(u64, Option<u64>)> = (0..20_000u64).map(|i| (i, None)).collect();
    let st = b.add(
        2001,
        &DocBuilder::storage_payload("도형 글", &[], &chars, &[], &[]),
    );
    let shape = b.add(2011, &f_ref(2, st));
    let att = b.attachment(shape);
    let body = body_with(&mut b, "앞", &vec![att; 20_000], "뒤");
    let secs = decode_secs(&pages_zip(b.finish(body)));
    assert!(secs < LINEAR_TIME_LIMIT_SECS as f64, "decode took {secs}s");
}

fn table_field(field: u32, entries: &[(u64, Option<u64>)]) -> Vec<u8> {
    f_bytes(field, &DocBuilder::attr_table(entries))
}

#[test]
fn one_comment_text_named_by_many_anchors_is_read_once() {
    // 공백뿐인 90만 바이트 댓글 하나를 앵커 5만 개가 가리킨다. 읽을 수 없는 댓글은 예산에 안 잡혀
    // 앵커마다 90만 바이트를 다시 복사하면 45 GB를 훑는다.
    let mut b = DocBuilder::default();
    let text = b.add(3056, &f_str(1, &" ".repeat(900_000)));
    let anchor = b.add(2013, &f_ref(1, text));
    let marks: Vec<(u64, Option<u64>)> = (0..50_000u64).map(|i| (i, Some(anchor))).collect();
    let st = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload("본문 글", &[], &[], &[], &[]),
            table_field(19, &marks),
        ]),
    );
    let secs = decode_secs(&pages_zip(b.finish(st)));
    assert!(secs < LINEAR_TIME_LIMIT_SECS as f64, "decode took {secs}s");
}
