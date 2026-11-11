//! Hostile PDFs (PD-164): every one must end in a result or a sentence, never in a panic, a hang or
//! an allocation the file did not pay for. The files are written by hand; most have no cross-
//! reference table, so the reader has to repair them, which is the path a crafted file takes.

use crate::pdf_edit_budget::{MAX_PAGES, MAX_PAGE_TREE_DEPTH};
use crate::pdf_edit_fixtures::{blank_page, deadline, hand_written, read, sample_pdf};
use crate::pdf_edit_inspect::{
    inflated_len, inspect, inspect_within, Inflate, Limits, MSG_CONTENT_BROKEN, MSG_CONTENT_FILTER,
    MSG_ENCRYPTED, MSG_UNREADABLE,
};
use crate::pdf_edit_session::EditSession;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use usvg::fontdb::Database;

/// Runs `work` on a thread with the worker's stack size, so an overflow of the stack or a panic of
/// the reader shows up as a failed test instead of ending the test run.
fn on_worker_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(32 * 1_048_576)
        .spawn(work)
        .unwrap()
        .join()
        .expect("the work panicked")
}

/// Opens the bytes and, when they open, writes the whole result back out. Returns the page count
/// or the sentence. The call must not take longer than `within`.
fn open_and_rebuild(bytes: Vec<u8>, within: Duration) -> Result<usize, String> {
    let started = Instant::now();
    let outcome = on_worker_stack(move || {
        let session = EditSession::open(bytes, &deadline())?;
        let out = session.render(None, &Arc::new(Database::new()), &deadline())?;
        Ok(read(&out).pages().len())
    });
    assert!(started.elapsed() < within, "took {:?}", started.elapsed());
    outcome
}

fn inspect_bytes(bytes: Vec<u8>, limits: Limits) -> Result<usize, String> {
    on_worker_stack(move || {
        inspect_within(&Arc::new(bytes), &deadline(), &limits).map(|info| info.pages)
    })
}

fn zeros_zlib(bytes: usize) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    let block = vec![0u8; 1 << 16];
    let mut left = bytes;
    while left > 0 {
        let n = left.min(block.len());
        encoder.write_all(&block[..n]).unwrap();
        left -= n;
    }
    encoder.finish().unwrap()
}

/// A stream object body: the dictionary, the data and the keywords around it.
fn stream_object(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// A one-page file whose page has the given content stream (raw bytes, dictionary entries).
fn one_page_with_content(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let objects: [(u32, Vec<u8>); 3] = [
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>".to_vec(),
        ),
    ];
    for (number, body) in objects {
        out.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(b"\nendobj\n");
    }
    out.extend_from_slice(b"4 0 obj\n");
    out.extend_from_slice(&stream_object(dict, data));
    out.extend_from_slice(b"\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n");
    out
}

fn small() -> Limits {
    Limits {
        stream_inflated: 1 << 20,
        total_inflated: 3 << 20,
        page_content: 1 << 20,
        total_content: 3 << 20,
        ..Limits::DEFAULT
    }
}

// ---- compression bombs --------------------------------------------------------------------------

#[test]
fn a_flate_bomb_as_page_content_is_refused_before_it_is_decoded() {
    let bomb = zeros_zlib(8 << 20);
    assert!(
        bomb.len() < 128 * 1024,
        "the bomb is small: {} bytes",
        bomb.len()
    );
    let file = one_page_with_content("/Filter /FlateDecode", &bomb);
    let started = Instant::now();
    // The stream scan sees it first (its own limit), with a limit high enough for the scan the page
    // check is the one that fires.
    let limits = Limits {
        stream_inflated: 64 << 20,
        total_inflated: 64 << 20,
        ..small()
    };
    let err = inspect_bytes(file.clone(), limits).unwrap_err();
    assert!(err.contains("쪽 내용"), "{err}");
    let err = inspect_bytes(file, small()).unwrap_err();
    assert!(err.contains("압축 해제"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn a_flate_bomb_hidden_in_a_picture_is_stopped_by_the_stream_scan() {
    let bomb = zeros_zlib(8 << 20);
    let mut file = one_page_with_content("", b"BT ET");
    let tail = file.len() - b"\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n".len();
    let _ = tail;
    file.truncate(file.len() - b"trailer\n<< /Root 1 0 R >>\n%%EOF\n".len());
    file.extend_from_slice(b"5 0 obj\n");
    file.extend_from_slice(&stream_object(
        "/Type /XObject /Subtype /Image /Width 1 /Height 1 /Filter /FlateDecode",
        &bomb,
    ));
    file.extend_from_slice(b"\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n");
    let err = inspect_bytes(file, small()).unwrap_err();
    assert_eq!(err, "PDF의 압축 해제 크기가 한도를 초과합니다.");
}

#[test]
fn many_streams_that_are_each_small_but_together_too_large_are_refused() {
    let part = zeros_zlib(1 << 20);
    let mut file = one_page_with_content("", b"BT ET");
    file.truncate(file.len() - b"trailer\n<< /Root 1 0 R >>\n%%EOF\n".len());
    for n in 5..10 {
        file.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        file.extend_from_slice(&stream_object("/Filter /FlateDecode", &part));
        file.extend_from_slice(b"\nendobj\n");
    }
    file.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    assert!(inspect_bytes(file.clone(), small()).is_err());
    // The same file passes with room for the total.
    let roomy = Limits {
        total_inflated: 64 << 20,
        total_content: 64 << 20,
        ..small()
    };
    assert_eq!(inspect_bytes(file, roomy), Ok(1));
}

#[test]
fn a_file_of_nothing_but_tiny_streams_is_scanned_in_linear_time() {
    let mut file = one_page_with_content("", b"BT ET");
    file.truncate(file.len() - b"trailer\n<< /Root 1 0 R >>\n%%EOF\n".len());
    file.extend_from_slice(&b"<< /Filter /FlateDecode >>\nstream\nx\nendstream\n".repeat(60_000));
    file.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    let started = Instant::now();
    // The reader may or may not accept the damaged objects; the scan itself must not be the slow part.
    let _ = inspect_bytes(file, Limits::DEFAULT);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn the_counting_inflater_stops_at_the_cap_and_rejects_what_is_not_flate() {
    let data = zeros_zlib(4 << 20);
    assert_eq!(inflated_len(&data, 8 << 20), Ok(4 << 20));
    assert_eq!(inflated_len(&data, (4 << 20) - 1), Err(Inflate::TooLarge));
    assert_eq!(
        inflated_len(b"not flate at all", 1 << 20),
        Err(Inflate::Invalid)
    );
    assert_eq!(
        inflated_len(&data[..data.len() / 2], 8 << 20),
        Err(Inflate::Invalid)
    );
    assert_eq!(inflated_len(b"", 10), Err(Inflate::Invalid));
}

#[test]
fn page_content_the_check_cannot_count_is_refused_not_guessed() {
    for (dict, message) in [
        ("/Filter /ASCIIHexDecode", MSG_CONTENT_FILTER),
        ("/Filter /LZWDecode", MSG_CONTENT_FILTER),
        ("/Filter [/ASCII85Decode /FlateDecode]", MSG_CONTENT_FILTER),
        (
            "/Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 4 >>",
            MSG_CONTENT_FILTER,
        ),
    ] {
        let file = one_page_with_content(dict, &zeros_zlib(10));
        assert_eq!(
            inspect_bytes(file, Limits::DEFAULT).unwrap_err(),
            message,
            "{dict}"
        );
    }
    let broken = one_page_with_content("/Filter /FlateDecode", b"x\x9cgarbage-not-deflate");
    assert_eq!(
        inspect_bytes(broken, Limits::DEFAULT).unwrap_err(),
        MSG_CONTENT_BROKEN
    );
}

// ---- declared sizes -----------------------------------------------------------------------------

#[test]
fn a_stream_that_declares_a_huge_length_costs_nothing() {
    let file = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Contents 4 0 R >>".to_string(),
            ),
            (
                4,
                "<< /Length 999999999999999 >>\nstream\nBT ET\nendstream".to_string(),
            ),
        ],
        "",
    );
    // Whatever the reader makes of it, it ends quickly and without a panic.
    let _ = open_and_rebuild(file, Duration::from_secs(10));
}

#[test]
fn an_absurd_cross_reference_declaration_does_not_allocate_for_it() {
    let mut file = sample_pdf(1, 2, &[]);
    let at = file.windows(4).rposition(|w| w == b"xref").expect("xref");
    file.truncate(at);
    file.extend_from_slice(
        b"xref\n0 4294967295\n0000000000 65535 f \ntrailer\n<< /Size 4294967295 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    let _ = open_and_rebuild(file, Duration::from_secs(10));
}

#[test]
fn a_file_over_the_size_limit_is_refused_by_its_length() {
    let file = sample_pdf(1, 1, &[]);
    let limits = Limits {
        source_bytes: file.len() - 1,
        ..Limits::DEFAULT
    };
    let err = inspect_bytes(file.clone(), limits).unwrap_err();
    assert!(err.contains("허용 크기"), "{err}");
    assert_eq!(inspect_bytes(file, Limits::DEFAULT), Ok(1));
}

#[test]
fn objects_pages_and_nodes_are_counted_against_their_limits() {
    let file = sample_pdf(1, 5, &[]);
    let few_objects = Limits {
        objects: 5,
        ..Limits::DEFAULT
    };
    assert!(inspect_bytes(file.clone(), few_objects)
        .unwrap_err()
        .contains("개체 수"));
    let few_pages = Limits {
        pages: 4,
        ..Limits::DEFAULT
    };
    assert!(inspect_bytes(file.clone(), few_pages)
        .unwrap_err()
        .contains("쪽 수"));
    let few_nodes = Limits {
        tree_nodes: 3,
        ..Limits::DEFAULT
    };
    assert!(inspect_bytes(file, few_nodes).unwrap_err().contains("노드"));
}

#[test]
fn more_pages_than_the_limit_are_refused() {
    let mut objects = vec![
        (1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            format!(
                "<< /Type /Pages /Count {} /Kids [{}] >>",
                MAX_PAGES + 1,
                (3..)
                    .take(MAX_PAGES + 1)
                    .map(|n| format!("{n} 0 R"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ),
    ];
    for n in 3..3 + MAX_PAGES as u32 + 1 {
        objects.push((n, blank_page(2)));
    }
    let err = inspect_bytes(hand_written(&objects, ""), Limits::DEFAULT).unwrap_err();
    assert!(err.contains("쪽 수"), "{err}");
}

// ---- structure ----------------------------------------------------------------------------------

#[test]
fn a_page_tree_that_contains_itself_ends() {
    let file = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (
                2,
                "<< /Type /Pages /Kids [2 0 R 3 0 R 2 0 R] /Count 1 >>".to_string(),
            ),
            (3, blank_page(2)),
        ],
        "",
    );
    assert_eq!(open_and_rebuild(file, Duration::from_secs(10)), Ok(1));
}

#[test]
fn two_page_nodes_that_name_each_other_end() {
    let file = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Pages /Parent 2 0 R /Kids [2 0 R 4 0 R] /Count 1 >>".to_string(),
            ),
            (4, blank_page(3)),
        ],
        "",
    );
    assert_eq!(open_and_rebuild(file, Duration::from_secs(10)), Ok(1));
}

#[test]
fn a_page_tree_that_is_too_deep_is_refused() {
    let depth = MAX_PAGE_TREE_DEPTH + 40;
    let mut objects = vec![(1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string())];
    for level in 0..depth {
        let n = 2 + level as u32;
        objects.push((
            n,
            format!("<< /Type /Pages /Kids [{} 0 R] /Count 1 >>", n + 1),
        ));
    }
    objects.push((2 + depth as u32, blank_page(2)));
    let err = inspect_bytes(hand_written(&objects, ""), Limits::DEFAULT).unwrap_err();
    assert!(err.contains("너무 깊"), "{err}");
    // A tree just inside the limit is fine.
    let mut ok = vec![(1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string())];
    for level in 0..MAX_PAGE_TREE_DEPTH - 1 {
        let n = 2 + level as u32;
        ok.push((
            n,
            format!("<< /Type /Pages /Kids [{} 0 R] /Count 1 >>", n + 1),
        ));
    }
    ok.push((1 + MAX_PAGE_TREE_DEPTH as u32, blank_page(2)));
    assert_eq!(inspect_bytes(hand_written(&ok, ""), Limits::DEFAULT), Ok(1));
}

#[test]
fn resources_that_refer_to_themselves_are_copied_once_and_end() {
    let file = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Font << /F1 4 0 R >> /XObject << /X 5 0 R >> >> >>"
                    .to_string(),
            ),
            (4, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Next 4 0 R /Other 5 0 R >>".to_string()),
            (5, "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] /Self 5 0 R /Font 4 0 R >>".to_string()),
        ],
        "",
    );
    assert_eq!(open_and_rebuild(file, Duration::from_secs(10)), Ok(1));
}

#[test]
fn an_object_that_nests_arrays_without_end_does_not_overflow_the_stack() {
    let nest = "[".repeat(200_000);
    let file = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Deep {nest} >>"),
            ),
        ],
        "",
    );
    let _ = open_and_rebuild(file, Duration::from_secs(20));
    let dicts = "<<".repeat(200_000);
    let file = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Deep {dicts} >>"),
            ),
        ],
        "",
    );
    let _ = open_and_rebuild(file, Duration::from_secs(20));
}

// ---- damaged and foreign files ------------------------------------------------------------------

#[test]
fn a_cross_reference_table_cut_anywhere_ends_in_a_result_or_a_sentence() {
    let file = sample_pdf(1, 3, &[]);
    let xref = file.windows(4).rposition(|w| w == b"xref").expect("xref");
    let start = xref.saturating_sub(40);
    for cut in (start..file.len()).step_by(7) {
        let truncated = file[..cut].to_vec();
        match open_and_rebuild(truncated, Duration::from_secs(10)) {
            Ok(pages) => assert!(pages <= 3, "cut {cut}: {pages} pages"),
            Err(message) => assert!(!message.is_empty(), "cut {cut}"),
        }
    }
}

#[test]
fn a_file_cut_in_the_middle_of_its_objects_ends_the_same_way() {
    let file = sample_pdf(1, 6, &[]);
    for cut in (20..file.len()).step_by(file.len() / 40) {
        let _ = open_and_rebuild(file[..cut].to_vec(), Duration::from_secs(10));
    }
}

#[test]
fn flipped_bytes_do_not_panic_the_reader() {
    let file = sample_pdf(1, 3, &[]);
    for seed in 0..64usize {
        let mut damaged = file.clone();
        for k in 0..8 {
            let at = (seed * 131 + k * 977) % damaged.len();
            damaged[at] ^= 0x5a + u8::try_from(k).unwrap();
        }
        let _ = open_and_rebuild(damaged, Duration::from_secs(10));
    }
}

#[test]
fn an_encrypted_file_is_refused_with_the_instruction() {
    let marked = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (3, blank_page(2)),
            (
                4,
                "<< /Filter /Standard /V 1 /R 2 /O (aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa) /U (bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb) /P -4 >>"
                    .to_string(),
            ),
        ],
        "/Encrypt 4 0 R /ID [<00112233445566778899aabbccddeeff> <00112233445566778899aabbccddeeff>]",
    );
    assert_eq!(
        inspect_bytes(marked.clone(), Limits::DEFAULT).unwrap_err(),
        MSG_ENCRYPTED
    );
    assert!(EditSession::open(marked, &deadline())
        .err()
        .is_some_and(|e| e.contains("암호")));
}

#[test]
fn files_that_are_not_pdfs_are_refused_with_one_sentence() {
    for bytes in [
        Vec::new(),
        b"hello".to_vec(),
        b"%PDF-1.7\n".to_vec(),
        b"PK\x03\x04 not a pdf".to_vec(),
        vec![0u8; 4096],
        [b"%PDF-1.4\n".as_slice(), &[0xff; 2048]].concat(),
    ] {
        let message = inspect_bytes(bytes.clone(), Limits::DEFAULT).unwrap_err();
        assert!(
            message == MSG_UNREADABLE || message.contains("쪽이 없는"),
            "{message}"
        );
    }
}

#[test]
fn a_file_with_a_valid_page_tree_but_no_pages_is_refused() {
    let file = hand_written(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [] /Count 0 >>".to_string()),
        ],
        "",
    );
    assert!(inspect_bytes(file, Limits::DEFAULT).is_err());
}

#[test]
fn the_shell_limits_accept_an_ordinary_file() {
    let file = Arc::new(sample_pdf(1, 20, &[0, 90]));
    assert_eq!(inspect(&file, &deadline()).map(|i| i.pages), Ok(20));
}
