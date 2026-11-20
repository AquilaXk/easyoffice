//! The PDF reader stays the engine's own forward scanner (PD-269, step 4): the object parser of the
//! crate the desktop shell uses decodes cross-reference streams, object streams and page contents
//! without an output limit and runs in the shell's worker process for that reason, which the
//! engine's in-process readers (conversion, readback, the command line) do not have. These tests
//! hold the limits a replacement would have to keep: forged cross-reference data, object streams
//! that name themselves, Flate bombs, nesting, and lengths that lie must each end in a refusal or
//! in the text the file really holds, quickly and without a large allocation.

use easyoffice_formats::{deflate_zlib, EasyOfficeError, PdfSerializer};
use std::time::{Duration, Instant};

/// Generous for a shared CI runner on a debug build; a quadratic or unbounded path takes far longer.
const CEILING: Duration = Duration::from_secs(15);

const HEAD: &[u8] = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n";
const TAIL: &[u8] = b"xref\n0 9\ntrailer << /Size 9 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n";
const PAGE_TEXT: &[u8] = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET";

/// A one-page file whose content object is `4 0 obj << {dict} >> stream {data} endstream`, followed
/// by `extra` objects.
fn page_file(dict: &str, data: &[u8], extra: &[u8]) -> Vec<u8> {
    let mut out = HEAD.to_vec();
    out.extend_from_slice(format!("4 0 obj << {dict} >>\nstream\n").as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out.extend_from_slice(extra);
    out.extend_from_slice(TAIL);
    out
}

fn timed<T>(work: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = work();
    assert!(start.elapsed() < CEILING, "took {:?}", start.elapsed());
    result
}

fn text_of(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    PdfSerializer::parse(bytes).map(|doc| {
        doc.pages
            .iter()
            .map(|p| p.body_text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[test]
fn a_cross_reference_table_that_lies_is_not_followed() {
    // The scanner reads the objects in the order they stand; the table and `startxref` are only
    // skipped, so a huge declared count or an offset into the middle of a stream moves nothing.
    let mut file = HEAD.to_vec();
    file.extend_from_slice(
        format!("4 0 obj << /Length {} >>\nstream\n", PAGE_TEXT.len()).as_bytes(),
    );
    file.extend_from_slice(PAGE_TEXT);
    file.extend_from_slice(
        b"\nendstream\nendobj\n\
xref\n0 4294967295\n0000000000 65535 f \n0000000009 00000 n \n9999999999 00000 n \n\
trailer << /Size 4294967295 /Root 99 0 R >>\nstartxref\n77\n%%EOF\n",
    );
    let text = timed(|| text_of(&file)).expect("a forged table does not hide the objects");
    assert_eq!(text, "Hello");
}

#[test]
fn an_object_defined_again_after_the_end_marker_replaces_the_earlier_one() {
    // An incremental update: a second revision after `%%EOF`. The later definition of an object
    // number wins, as in a viewer that follows the last `startxref`; the earlier text is not mixed in.
    let mut file = page_file(&format!("/Length {}", PAGE_TEXT.len()), PAGE_TEXT, b"");
    file.extend_from_slice(
        b"4 0 obj << /Length 22 >>\nstream\nBT (Hidden) Tj ET\n\nendstream\nendobj\n\
xref\n0 1\ntrailer << /Size 9 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    assert_eq!(timed(|| text_of(&file)).unwrap(), "Hidden");
}

/// An object stream (object 5) of `count` declared members whose header lists `entries` as
/// `number offset` pairs, with the body `objects`. `dict_extra` is added to its dictionary.
fn object_stream_file(count: u64, entries: &str, objects: &str, dict_extra: &str) -> Vec<u8> {
    let body = format!("{entries} {objects}");
    let packed = deflate_zlib(body.as_bytes());
    let mut out = HEAD.to_vec();
    out.extend_from_slice(
        format!("4 0 obj << /Length {} >>\nstream\n", PAGE_TEXT.len()).as_bytes(),
    );
    out.extend_from_slice(PAGE_TEXT);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out.extend_from_slice(
        format!(
            "5 0 obj << /Type /ObjStm /N {count} /First {} /Filter /FlateDecode /Length {} {dict_extra} >>\nstream\n",
            entries.len() + 1,
            packed.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&packed);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out.extend_from_slice(TAIL);
    out
}

#[test]
fn an_object_stream_that_names_itself_or_extends_itself_ends() {
    // Object 5 lists object 5 as one of its members, and `/Extends` points back at it.
    let file = object_stream_file(2, "5 0 6 7", "<< /A 1 >> << /B 2 >>", "/Extends 5 0 R");
    assert_eq!(timed(|| text_of(&file)).unwrap(), "Hello");
    // Two object streams that extend each other.
    let mut two = object_stream_file(2, "6 0 7 12", "<< /A 1 >> << /B 2 >>", "/Extends 8 0 R");
    let packed = deflate_zlib(b"8 0 9 12 << /C 3 >> << /D 4 >>");
    two.truncate(two.len() - TAIL.len());
    two.extend_from_slice(
        format!(
            "8 0 obj << /Type /ObjStm /N 2 /First 7 /Filter /FlateDecode /Length {} /Extends 5 0 R >>\nstream\n",
            packed.len()
        )
        .as_bytes(),
    );
    two.extend_from_slice(&packed);
    two.extend_from_slice(b"\nendstream\nendobj\n");
    two.extend_from_slice(TAIL);
    assert_eq!(timed(|| text_of(&two)).unwrap(), "Hello");
}

#[test]
fn an_object_stream_that_declares_an_absurd_member_count_is_refused() {
    // A count a little over what the header lists is read as far as it goes; a count in the
    // billions is a way to ask for a table that size, and the budget on entries refuses it.
    let small = object_stream_file(3, "6 0 7 12", "<< /A 1 >> << /B 2 >>", "");
    assert_eq!(timed(|| text_of(&small)).unwrap(), "Hello");
    let huge = object_stream_file(4_000_000_000, "6 0 7 12", "<< /A 1 >> << /B 2 >>", "");
    let outcome = timed(|| text_of(&huge));
    assert!(
        matches!(
            outcome,
            Err(EasyOfficeError::CorruptedContainer(_) | EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "{outcome:?}"
    );
}

/// A zlib stream of `len` zero bytes, built in 1 MiB steps so the test itself stays small.
fn zeros_zlib(len: usize) -> Vec<u8> {
    deflate_zlib(&vec![0u8; len])
}

#[test]
fn a_flate_bomb_as_page_content_is_refused_by_the_byte_budget() {
    // 48 MiB of zeros packs to about 48 KiB; one stream may inflate to 16 MiB.
    let bomb = zeros_zlib(48 * 1024 * 1024);
    let file = page_file(
        &format!("/Filter /FlateDecode /Length {}", bomb.len()),
        &bomb,
        b"",
    );
    assert!(file.len() < 256 * 1024);
    let outcome = timed(|| text_of(&file));
    assert!(
        matches!(
            outcome,
            Err(EasyOfficeError::SecurityViolation(_) | EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "{outcome:?}"
    );
}

#[test]
fn a_flate_bomb_named_through_a_filter_array_or_a_second_stage_is_refused() {
    let bomb = zeros_zlib(48 * 1024 * 1024);
    // /Filter as a one-element array, and Flate applied to what ASCIIHex decodes to.
    let file = page_file(
        &format!("/Filter [/FlateDecode] /Length {}", bomb.len()),
        &bomb,
        b"",
    );
    let outcome = timed(|| text_of(&file));
    assert!(
        matches!(
            outcome,
            Err(EasyOfficeError::SecurityViolation(_) | EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "{outcome:?}"
    );
    let hex: String = bomb.iter().map(|b| format!("{b:02x}")).collect();
    let chained = page_file(
        &format!(
            "/Filter [/ASCIIHexDecode /FlateDecode] /Length {}",
            hex.len() + 1
        ),
        format!("{hex}>").as_bytes(),
        b"",
    );
    let outcome = timed(|| text_of(&chained));
    assert!(
        matches!(
            outcome,
            Err(EasyOfficeError::SecurityViolation(_) | EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "{outcome:?}"
    );
}

#[test]
fn a_flate_bomb_in_an_unused_object_counts_against_the_file() {
    // Not referenced by any page: it is still decoded when the structure is validated, so it has
    // to be charged to the same budget.
    let bomb = zeros_zlib(48 * 1024 * 1024);
    let mut extra = format!(
        "7 0 obj << /Filter /FlateDecode /Length {} >>\nstream\n",
        bomb.len()
    )
    .into_bytes();
    extra.extend_from_slice(&bomb);
    extra.extend_from_slice(b"\nendstream\nendobj\n");
    let file = page_file(&format!("/Length {}", PAGE_TEXT.len()), PAGE_TEXT, &extra);
    let outcome = timed(|| text_of(&file));
    assert!(
        matches!(
            outcome,
            Err(EasyOfficeError::SecurityViolation(_)
                | EasyOfficeError::ResourceBudgetExceeded(_)
                | EasyOfficeError::CorruptedContainer(_))
        ),
        "{outcome:?}"
    );
}

#[test]
fn arrays_nested_without_end_do_not_overflow_the_stack() {
    let depth = 200_000;
    // In an object value.
    let mut value = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R /X ".to_vec();
    value.extend(std::iter::repeat_n(b'[', depth));
    value.extend(std::iter::repeat_n(b']', depth));
    value.extend_from_slice(b" >> endobj\n");
    value.extend_from_slice(&HEAD[HEAD.iter().position(|&b| b == b'2').unwrap()..]);
    value.extend_from_slice(b"4 0 obj << /Length 0 >>\nstream\n\nendstream\nendobj\n");
    value.extend_from_slice(TAIL);
    let outcome = timed(|| text_of(&value));
    assert!(
        matches!(
            outcome,
            Err(EasyOfficeError::CorruptedContainer(_) | EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "{outcome:?}"
    );
    // In a page content stream.
    let mut content = b"BT [".to_vec();
    content.extend(std::iter::repeat_n(b'[', depth));
    content.extend(std::iter::repeat_n(b']', depth));
    content.extend_from_slice(b"] TJ ET");
    let file = page_file(&format!("/Length {}", content.len()), &content, b"");
    assert_eq!(timed(|| text_of(&file)).unwrap(), "");
}

#[test]
fn dictionaries_nested_without_end_do_not_overflow_the_stack() {
    let depth = 100_000;
    let mut nested = Vec::new();
    for _ in 0..depth {
        nested.extend_from_slice(b"<< /K ");
    }
    nested.extend_from_slice(b"1");
    for _ in 0..depth {
        nested.extend_from_slice(b" >>");
    }
    let mut file = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R /X ".to_vec();
    file.extend_from_slice(&nested);
    file.extend_from_slice(b" >> endobj\n");
    file.extend_from_slice(&HEAD[HEAD.iter().position(|&b| b == b'2').unwrap()..]);
    file.extend_from_slice(b"4 0 obj << /Length 0 >>\nstream\n\nendstream\nendobj\n");
    file.extend_from_slice(TAIL);
    let outcome = timed(|| text_of(&file));
    assert!(
        matches!(
            outcome,
            Err(EasyOfficeError::CorruptedContainer(_) | EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "{outcome:?}"
    );
}

#[test]
fn a_length_that_lies_never_moves_the_stream_end_past_the_real_one() {
    // Too long, absurdly long, too short, negative, fractional, and indirect: itself, a missing
    // object, and a chain that loops. The keyword `endstream` ends the data wherever the number
    // points, so every spelling reads the same text.
    let lengths = [
        ("100000", ""),
        ("4294967295", ""),
        ("99999999999999999999", ""),
        ("3", ""),
        ("-5", ""),
        ("1.5", ""),
        ("4 0 R", ""),
        ("77 0 R", ""),
        ("5 0 R", "5 0 obj 6 0 R endobj\n6 0 obj 5 0 R endobj\n"),
    ];
    for (length, extra) in lengths {
        let file = page_file(&format!("/Length {length}"), PAGE_TEXT, extra.as_bytes());
        assert_eq!(
            timed(|| text_of(&file)).unwrap_or_else(|e| panic!("length {length}: {e:?}")),
            "Hello",
            "length {length}"
        );
    }
}

#[test]
fn the_same_input_reads_to_the_same_model_every_time() {
    let file = page_file(&format!("/Length {}", PAGE_TEXT.len()), PAGE_TEXT, b"");
    let first = PdfSerializer::parse(&file).unwrap();
    for _ in 0..8 {
        assert_eq!(PdfSerializer::parse(&file).unwrap(), first);
    }
}
