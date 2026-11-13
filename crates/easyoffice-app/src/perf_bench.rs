//! Measurement harness of the engine paths for `docs/performance.md`, not a check. It builds
//! synthetic documents of every family once (`bench_generate_inputs`) and times the two paths the
//! editor calls for a file, through the same IPC entry the shell uses: opening (`convert_document_format`
//! with `action: decode`) and saving (the decoded structured content sent back with a target
//! format). Each case prints one `BENCH ...` line, so `scripts/bench-run.mjs` can run it in a fresh
//! process N times under `/usr/bin/time -l`. `EASYOFFICE_BENCH_REPEAT` repeats the timed part in one
//! process (for a sampling profiler; the line shows the last run).
//!
//! `EASYOFFICE_BENCH_SCALE` (default 1) multiplies the paragraph and sheet row counts of the
//! generated inputs, to reach megabyte-sized files. `EASYOFFICE_BENCH_DUMP` (a file path) keeps
//! the answer of an `open`, to compare two builds byte for byte.
//!
//! `EASYOFFICE_BENCH_CASE`: `open`, `save` (input: a file made by `bench_generate_inputs`),
//! `encode` (input: an extension; one `encode_structured` of the generated content),
//! `recalc` (input: the row count), `sheet` (input: the row count, see `perf_bench_sheet`),
//! `layout` (input: the call count).

use crate::benchmark_large::build_recalc_sheet;
use crate::convert_structured::{encode_structured, StructuredContent};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    CellMerge, DocumentFormat, PdfWriteOptions, RichBlock, RichDocument, RichParagraph, RichRun,
    RichTable,
};
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

const WORDS: [&str; 16] = [
    "기획조정실",
    "예산집행",
    "디지털",
    "행정혁신",
    "report",
    "budget",
    "분기별",
    "성과",
    "표준",
    "문서",
    "Quarterly",
    "alignment",
    "검토",
    "승인",
    "2026",
    "plan",
];
const PARAGRAPHS: usize = 9_700;
const TABLE_EVERY: usize = 97;
const TABLE_ROWS: usize = 6;
const TABLE_COLS: usize = 4;
const SHEET_ROWS: u32 = 1_000;
const SHEET_COLS: u32 = 104;
const SLIDES: usize = 300;

fn scale() -> usize {
    std::env::var("EASYOFFICE_BENCH_SCALE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1)
}

struct XorShift(u32);

impl XorShift {
    fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as usize
    }
}

fn sentence(rng: &mut XorShift, index: usize) -> String {
    let mut text = format!("{}. ", index + 1);
    for _ in 0..8 + rng.next() % 6 {
        text.push_str(WORDS[rng.next() % WORDS.len()]);
        text.push(' ');
    }
    text
}

/// A report of [`PARAGRAPHS`] paragraphs: headings, a bold/italic run in every third one and a
/// small table with a merged cell after every [`TABLE_EVERY`]th.
fn large_document() -> RichDocument {
    let mut rng = XorShift(0x2545_F491);
    let paragraphs = PARAGRAPHS * scale();
    let mut blocks = Vec::with_capacity(paragraphs + paragraphs / TABLE_EVERY);
    for index in 0..paragraphs {
        let mut runs = vec![RichRun::plain(sentence(&mut rng, index))];
        if index % 3 == 0 {
            runs.push(RichRun {
                text: WORDS[rng.next() % WORDS.len()].to_string(),
                bold: true,
                italic: index % 6 == 0,
                ..RichRun::default()
            });
        }
        blocks.push(RichBlock::Paragraph(RichParagraph {
            runs,
            heading_level: if index % 50 == 0 { 2 } else { 0 },
            ..RichParagraph::default()
        }));
        if index % TABLE_EVERY == TABLE_EVERY - 1 {
            let rows = (0..TABLE_ROWS)
                .map(|row| {
                    (0..TABLE_COLS)
                        .map(|col| format!("{}-{row}-{col}", WORDS[rng.next() % WORDS.len()]))
                        .collect()
                })
                .collect();
            let merge = CellMerge {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 2,
            };
            blocks.push(RichBlock::Table(
                RichTable::with_merges(rows, vec![merge]).0,
            ));
        }
    }
    RichDocument {
        page_frame: Default::default(),
        title: "대용량 보고서".to_string(),
        blocks,
        official_agency: None,
    }
}

/// [`SHEET_ROWS`] x [`SHEET_COLS`] cells; every fourth column is a formula over the two before it.
fn large_workbook() -> StructuredContent {
    let rows = SHEET_ROWS * scale() as u32;
    let mut cells = Vec::with_capacity((rows * SHEET_COLS) as usize);
    for row in 0..rows {
        for col in 0..SHEET_COLS {
            let text = if col % 4 == 3 {
                let (a, b) = (col_label(col - 2), col_label(col - 1));
                format!("={a}{r}+{b}{r}", r = row + 1)
            } else {
                format!("{}", row * 131 + col * 7)
            };
            cells.push((row, col, text));
        }
    }
    StructuredContent::Workbook(
        vec![("Sheet1".to_string(), cells)],
        vec![Default::default()],
    )
}

fn col_label(mut col: u32) -> String {
    let mut label = String::new();
    loop {
        label.insert(0, (b'A' + (col % 26) as u8) as char);
        if col < 26 {
            return label;
        }
        col = col / 26 - 1;
    }
}

fn large_slides() -> StructuredContent {
    let mut rng = XorShift(0x9E37_79B9);
    StructuredContent::Slides(
        (0..SLIDES)
            .map(|index| {
                let bullets = (0..5).map(|n| sentence(&mut rng, n)).collect();
                (
                    format!("슬라이드 {}", index + 1),
                    bullets,
                    sentence(&mut rng, index),
                )
            })
            .collect(),
    )
}

fn out_dir() -> PathBuf {
    PathBuf::from(std::env::var("EASYOFFICE_BENCH_OUT").expect("EASYOFFICE_BENCH_OUT"))
}

/// Writes one input per format the engine can write, named `<family>-large.<ext>`.
#[test]
#[ignore = "measurement input, run by scripts/bench-run.mjs"]
fn bench_generate_inputs() {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("dir");
    let document = StructuredContent::Document(large_document(), PdfWriteOptions::default());
    let families: [(&str, StructuredContent, &[DocumentFormat]); 3] = [
        (
            "doc",
            document,
            &[
                DocumentFormat::Docx,
                DocumentFormat::Odt,
                DocumentFormat::Rtf,
                DocumentFormat::Hwpx,
                DocumentFormat::Hwp,
                DocumentFormat::Pdf,
            ],
        ),
        (
            "sheet",
            large_workbook(),
            &[
                DocumentFormat::Xlsx,
                DocumentFormat::Ods,
                DocumentFormat::Xls,
            ],
        ),
        (
            "slides",
            large_slides(),
            &[
                DocumentFormat::Pptx,
                DocumentFormat::Odp,
                DocumentFormat::Ppt,
            ],
        ),
    ];
    for (family, content, formats) in families {
        for format in formats {
            let started = Instant::now();
            match encode_structured(&content, *format, "bench", &mut Vec::new()) {
                Ok(bytes) => {
                    let path = dir.join(format!("{family}-large.{}", format.extension()));
                    std::fs::write(&path, &bytes).expect("write");
                    println!(
                        "GEN {} {} bytes in {:?}",
                        path.display(),
                        bytes.len(),
                        started.elapsed()
                    );
                }
                Err(err) => println!("GEN {family}-large.{} skipped: {err}", format.extension()),
            }
        }
    }
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn open_request(bytes: &[u8], file_name: &str) -> String {
    format!(
        "{{\"action\":\"decode\",\"fileName\":\"{file_name}\",\"sourceBase64\":\"{}\"}}",
        EasyOfficeIpcRouter::encode_base64(bytes)
    )
}

/// The save request the page builds from what an open returned: the structured content (`blocks`,
/// `sheets` or `slides`, the last member of the open's answer) with the formats and the title.
fn save_request(decoded: &str, extension: &str) -> String {
    let content_at = ["],\"blocks\":[", "],\"sheets\":[", "],\"slides\":["]
        .iter()
        .filter_map(|marker| decoded.find(marker))
        .min()
        .expect("structured content in the open's answer")
        + 2;
    let content = &decoded[content_at..decoded.len() - 1];
    format!(
        "{{\"sourceFormat\":\"{extension}\",\"targetFormat\":\"{extension}\",\"title\":\"bench\",{content}}}"
    )
}

fn run(case: &str, input: &str) -> String {
    let started = Instant::now();
    let mut phases: Vec<(&str, f64)> = Vec::new();
    let (name, out_bytes) = match case {
        "open" | "save" => {
            let path = PathBuf::from(input);
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let extension = path.extension().unwrap().to_string_lossy().to_string();
            let bytes = std::fs::read(&path).expect("read");
            let t = Instant::now();
            let request = open_request(&bytes, &name);
            phases.push(("request", ms(t)));
            let t = Instant::now();
            let decoded =
                EasyOfficeIpcRouter::dispatch("convert_document_format", &request).expect("open");
            phases.push(("open", ms(t)));
            if let Ok(dump) = std::env::var("EASYOFFICE_BENCH_DUMP") {
                std::fs::write(dump, &decoded).expect("dump");
            }
            if case == "open" {
                (name, decoded.len())
            } else {
                let t = Instant::now();
                let request = save_request(&decoded, &extension);
                phases.push(("save_request", ms(t)));
                let t = Instant::now();
                let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &request)
                    .expect("save");
                phases.push(("save", ms(t)));
                (name, saved.len())
            }
        }
        "encode" => {
            // One `encode_structured` of the generated content: the save path may call it for the
            // source format and again for the target format.
            let format = DocumentFormat::from_extension(input).expect("extension");
            let content = match format {
                DocumentFormat::Xlsx | DocumentFormat::Ods => large_workbook(),
                DocumentFormat::Pptx => large_slides(),
                _ => StructuredContent::Document(large_document(), PdfWriteOptions::default()),
            };
            let t = Instant::now();
            let bytes =
                encode_structured(&content, format, "bench", &mut Vec::new()).expect("encode");
            phases.push(("encode", ms(t)));
            (format!("encode-{input}"), bytes.len())
        }
        "recalc" => {
            let rows: u32 = input.parse().expect("rows");
            let t = Instant::now();
            let sheet = build_recalc_sheet(rows);
            phases.push(("build", ms(t)));
            let t = Instant::now();
            let results = black_box(&sheet).recalculate_all();
            phases.push(("recalc", ms(t)));
            (format!("recalc-{rows}"), results.len())
        }
        "sheet" => {
            let rows: u32 = input.parse().expect("rows");
            let out_bytes = crate::perf_bench_sheet::run(rows, &mut phases);
            (format!("sheet-{rows}"), out_bytes)
        }
        "layout" => {
            let calls: usize = input.parse().expect("calls");
            let mut bytes = 0;
            for _ in 0..calls {
                bytes += EasyOfficeIpcRouter::dispatch("compute_layout", "portrait")
                    .expect("layout")
                    .len();
            }
            (format!("layout-x{calls}"), bytes)
        }
        other => panic!("unknown case {other}"),
    };
    let phases: Vec<String> = phases.iter().map(|(n, v)| format!("{n}={v:.1}")).collect();
    format!(
        "BENCH tool=eo case={case} input={name} total_ms={:.1} out_bytes={out_bytes} pages=0 phases={}",
        ms(started),
        phases.join(",")
    )
}

#[test]
#[ignore = "measurement, run by scripts/bench-run.mjs"]
fn bench_case() {
    let case = std::env::var("EASYOFFICE_BENCH_CASE").expect("EASYOFFICE_BENCH_CASE");
    let input = std::env::var("EASYOFFICE_BENCH_INPUT").expect("EASYOFFICE_BENCH_INPUT");
    let repeat: usize = std::env::var("EASYOFFICE_BENCH_REPEAT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    for _ in 0..repeat {
        println!("{}", run(&case, &input));
    }
}
