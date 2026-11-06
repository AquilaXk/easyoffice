mod pages_fixture;

use std::time::{Duration, Instant};

use easyoffice_formats::pages_iwa::{decompress_iwa, snappy_decompress, split_records};
use easyoffice_formats::pages_proto::{ArchiveObject, Message, ObjectTable};
use easyoffice_formats::{
    EasyOfficeError, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};
use pages_fixture::*;

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

const TIME_BOUND: Duration = Duration::from_secs(60);

fn valid_stream() -> Vec<u8> {
    let mut s = record(1, 10000, &f_ref(4, 2));
    s.extend(record(2, 2001, &f_str(3, "a\nb")));
    s
}

#[test]
fn random_and_mutated_snappy_never_panics() {
    let start = Instant::now();
    let mut rng = Xorshift(0x9E37_79B9_7F4A_7C15);
    let good = snappy_compress(&b"abcabcabc-fuzz-".repeat(200));
    for i in 0..20_000 {
        let n = rng.below(300);
        let input = if i % 3 == 0 {
            rng.bytes(n)
        } else {
            let mut m = good.clone();
            for _ in 0..1 + rng.below(4) {
                let k = rng.below(m.len());
                m[k] ^= 1 << rng.below(8);
            }
            m.truncate(rng.below(m.len() + 1));
            m
        };
        let _ = snappy_decompress(&input, 1 << 20);
    }
    assert!(start.elapsed() < TIME_BOUND);
}

#[test]
fn random_and_mutated_iwa_streams_never_panic() {
    let start = Instant::now();
    let mut rng = Xorshift(0xD1B5_4A32_D192_ED03);
    let good = iwa_frame(&valid_stream(), true);
    for i in 0..10_000 {
        let n = rng.below(200);
        let input = if i % 4 == 0 {
            rng.bytes(n)
        } else {
            let mut m = good.clone();
            for _ in 0..1 + rng.below(3) {
                let k = rng.below(m.len());
                m[k] = rng.next() as u8;
            }
            m.truncate(rng.below(m.len() + 1));
            m
        };
        let mut budget = 1 << 24;
        if let Ok(stream) = decompress_iwa(&input, &mut budget) {
            let _ = split_records(&stream);
        }
        let _ = split_records(&input);
    }
    assert!(start.elapsed() < TIME_BOUND);
}

#[test]
fn random_protobuf_never_panics_and_respects_caps() {
    let start = Instant::now();
    let mut rng = Xorshift(0x2545_F491_4F6C_DD1D);
    for _ in 0..30_000 {
        let n = rng.below(120);
        let data = rng.bytes(n);
        if let Ok(m) = Message::parse(&data) {
            for f in &m.fields {
                let _ = m.sub(f.number);
                let _ = m.repeated_varints(f.number);
            }
        }
    }
    // a flood of tiny fields exceeds the per-message cap with an error, not memory growth
    let flood = [0x08u8, 0x01].repeat(70_000);
    assert!(matches!(
        Message::parse(&flood),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    assert!(start.elapsed() < TIME_BOUND);
}

#[test]
fn oversized_declarations_are_rejected_without_allocating() {
    // snappy chunk declaring 4 GiB, IWA chunk length beyond the data, record length huge
    let mut huge = Vec::new();
    varint(1 << 32, &mut huge);
    huge.extend_from_slice(&[0, b'x']);
    let mut framed = vec![0u8];
    framed.extend_from_slice(&(huge.len() as u32).to_le_bytes()[..3]);
    framed.extend_from_slice(&huge);
    assert!(decompress_iwa(&framed, &mut (1 << 30)).is_err());
    assert!(decompress_iwa(&[0, 0xFF, 0xFF, 0xFF, 1, 2], &mut (1 << 30)).is_err());
    let mut rec = Vec::new();
    varint(u64::MAX, &mut rec);
    assert!(split_records(&rec).is_err());
    let info = cat(&[
        f_varint(1, 1),
        f_bytes(2, &cat(&[f_varint(1, 1), f_varint(3, 0xFFFF_FFFF)])),
    ]);
    let mut rec = Vec::new();
    varint(info.len() as u64, &mut rec);
    rec.extend(info);
    assert!(split_records(&rec).is_err());
}

#[test]
fn many_chunks_cannot_exceed_the_document_budget() {
    let stream = vec![0u8; 60_000];
    let one = iwa_frame(&stream, true);
    let many: Vec<u8> = one.iter().copied().cycle().take(one.len() * 200).collect();
    // each repeated chunk is valid; the total exceeds the budget given here
    let mut budget = 1_000_000usize;
    assert!(matches!(
        decompress_iwa(&many, &mut budget),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn reference_cycles_terminate() {
    // Table traversal helpers
    let mut t = ObjectTable::default();
    for id in 1..=3u64 {
        t.objects.insert(
            id,
            ArchiveObject {
                type_id: 1,
                payload: f_varint(1, id % 3 + 1),
                extra: vec![],
            },
        );
    }
    let next = |o: &ArchiveObject| Message::parse(&o.payload).ok().and_then(|m| m.varint(1));
    assert_eq!(t.chain(1, 64, next).len(), 3);
    assert_eq!(
        t.reachable(1, 64, 100, |o| next(o).into_iter().collect())
            .len(),
        3
    );

    // Style parent cycle and a table whose cell storage references itself must still decode.
    let mut b = DocBuilder::default();
    let a = b.reserve_id();
    let c = b.reserve_id();
    let header_a = f_bytes(1, &cat(&[f_str(2, "a"), f_ref(3, c)]));
    let header_c = f_bytes(1, &cat(&[f_str(2, "c"), f_ref(3, a)]));
    b.add_with_id(a, 2022, &cat(&[header_a, f_bytes(11, &f_varint(1, 1))]));
    b.add_with_id(c, 2022, &cat(&[header_c, f_bytes(11, &f_varint(2, 1))]));
    let tbl = b.reserve_id();
    let att = b.attachment(tbl);
    b.add_with_id(tbl, 6000, &f_ref(2, tbl)); // model reference points back at itself
    let body = b.add(
        2001,
        &DocBuilder::storage_payload(
            "x\u{FFFC}",
            &[(0, Some(a))],
            &[(0, Some(c))],
            &[],
            &[(1, Some(att))],
        ),
    );
    let doc = UniversalFormatConverter::decode_rich(&pages_zip(b.finish(body)), Some("c.pages"))
        .expect("cyclic styles must not break decoding");
    assert!(!doc.warnings.is_empty());
}

fn flip_in_iwa(zip: &[u8], rng: &mut Xorshift) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(zip, &ZipSecurityPolicy::default()).unwrap();
    let packed: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "Index/Document.iwa" {
                // mutate the decompressed record stream, then re-frame it validly
                let mut budget = 1 << 26;
                let mut stream = decompress_iwa(&e.data, &mut budget).unwrap();
                for _ in 0..1 + rng.below(6) {
                    let k = rng.below(stream.len());
                    stream[k] = rng.next() as u8;
                }
                if rng.below(4) == 0 {
                    stream.truncate(rng.below(stream.len() + 1));
                }
                (e.path, iwa_frame(&stream, true))
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack(&packed).unwrap()
}

#[test]
fn mutated_documents_decode_or_fail_cleanly() {
    let start = Instant::now();
    let mut rng = Xorshift(0xA076_1D64_78BD_642F);
    let base = sample_document();
    let (mut ok, mut err) = (0, 0);
    for _ in 0..600 {
        let doc = flip_in_iwa(&base, &mut rng);
        match UniversalFormatConverter::decode_rich(&doc, Some("f.pages")) {
            Ok(_) => ok += 1,
            Err(_) => err += 1,
        }
    }
    assert!(ok + err == 600);
    assert!(
        start.elapsed() < TIME_BOUND,
        "fuzz took {:?}",
        start.elapsed()
    );
}

#[test]
fn raw_zip_mutations_and_garbage_never_panic() {
    let mut rng = Xorshift(0xE703_7ED1_A0B4_28DB);
    let base = sample_document();
    for _ in 0..400 {
        let mut m = base.clone();
        for _ in 0..1 + rng.below(8) {
            let k = rng.below(m.len());
            m[k] ^= 1 << rng.below(8);
        }
        m.truncate(rng.below(m.len() + 1).max(4));
        let _ = UniversalFormatConverter::decode_rich(&m, Some("z.pages"));
    }
    for _ in 0..200 {
        let n = rng.below(2000);
        let mut g = vec![0x50, 0x4B, 0x03, 0x04];
        g.extend(rng.bytes(n));
        let _ = UniversalFormatConverter::decode_rich(&g, Some("g.pages"));
    }
}

#[test]
fn huge_declared_table_is_skipped_not_allocated() {
    let mut b = DocBuilder::default();
    let model = b.add(
        6001,
        &cat(&[
            f_bytes(4, &f_varint(99, 1)),
            f_varint(6, 4_000_000_000),
            f_varint(7, 4_000_000_000),
        ]),
    );
    let info = b.add(6000, &f_ref(2, model));
    let att = b.attachment(info);
    let body = b.add(
        2001,
        &DocBuilder::storage_payload("\u{FFFC}", &[], &[], &[], &[(0, Some(att))]),
    );
    let doc =
        UniversalFormatConverter::decode_rich(&pages_zip(b.finish(body)), Some("h.pages")).unwrap();
    assert!(
        doc.warnings.iter().any(|w| w.contains("너무 큰 표")),
        "{:?}",
        doc.warnings
    );
}
