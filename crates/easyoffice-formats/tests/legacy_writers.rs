//! Structure and round-trip checks for the `.doc` / `.ppt` writers (#112).

use easyoffice_formats::{
    LegacyDocPayload, LegacyOle2Codec, LegacyPptPayload, LegacyPptSlide, Ole2ContainerCodec,
};

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn stream(bytes: &[u8], name: &str) -> Vec<u8> {
    Ole2ContainerCodec::unpack(bytes)
        .unwrap()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("스트림 {name} 누락"))
        .data
}

fn utf16(d: &[u8]) -> String {
    let units: Vec<u16> = d
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).unwrap()
}

fn doc_payload() -> LegacyDocPayload {
    LegacyDocPayload {
        title: "제목 문서".to_string(),
        paragraphs: vec![
            "첫째 문단 Hello".to_string(),
            "둘째 문단입니다 😀".to_string(),
            String::new(),
            "Third paragraph".to_string(),
        ],
        tables: vec![vec![
            vec!["이름".to_string(), "값".to_string()],
            vec!["가".to_string(), "1".to_string()],
        ]],
    }
}

/// FibRgFcLcb97 pair `index` of the template layout (csw 14, cslw 22).
fn fc_lcb(word: &[u8], index: usize) -> (usize, usize) {
    let at = 154 + index * 8;
    (u32_at(word, at) as usize, u32_at(word, at + 4) as usize)
}

#[test]
fn doc_has_spec_streams_fib_clx_and_fkp_pages() {
    let payload = doc_payload();
    let bytes = LegacyOle2Codec::pack_doc(&payload).unwrap();
    let word = stream(&bytes, "WordDocument");
    let table = stream(&bytes, "1Table");

    assert_eq!(u16_at(&word, 0), 0xA5EC, "wIdent");
    assert_eq!(u16_at(&word, 2), 0x00C1, "nFib");
    assert_ne!(u16_at(&word, 10) & 0x0200, 0, "fWhichTblStm");
    assert_eq!(u16_at(&word, 10) & 0x0100, 0, "fEncrypted");
    assert_eq!(u16_at(&word, 32), 14, "csw");
    assert_eq!(u16_at(&word, 62), 22, "cslw");
    assert_eq!(u16_at(&word, 152), 93, "cbRgFcLcb");
    assert_eq!(u32_at(&word, 64) as usize, word.len(), "cbMac");

    let expected = "첫째 문단 Hello\r둘째 문단입니다 😀\r\rThird paragraph\r이름\t값\r가\t1\r";
    let ccp = expected.encode_utf16().count();
    assert_eq!(u32_at(&word, 76) as usize, ccp, "ccpText");

    // Clx: one Pcdt holding a PlcPcd with a single UTF-16LE piece.
    let (fc_clx, lcb_clx) = fc_lcb(&word, 33);
    let clx = &table[fc_clx..fc_clx + lcb_clx];
    assert_eq!(clx[0], 0x02, "Pcdt clxt");
    assert_eq!(u32_at(clx, 1) as usize, lcb_clx - 5);
    assert_eq!(lcb_clx - 5, 4 * 2 + 8, "one piece");
    assert_eq!(u32_at(clx, 5), 0);
    assert_eq!(u32_at(clx, 9) as usize, ccp);
    let fc_raw = u32_at(clx, 5 + 8 + 2);
    assert_eq!(
        fc_raw & 0x4000_0000,
        0,
        "UTF-16 piece must not be compressed"
    );
    let text_fc = fc_raw as usize;
    assert_eq!(text_fc % 512, 0, "text starts on a 512-byte boundary");
    assert_eq!(
        utf16(&word[text_fc..text_fc + ccp_bytes(expected)]),
        expected
    );

    // PlcfSed ends at ccpText and its section properties live inside the stream.
    let (fc_sed, lcb_sed) = fc_lcb(&word, 6);
    assert_eq!(lcb_sed, 20);
    assert_eq!(u32_at(&table, fc_sed + 4) as usize, ccp);
    let fc_sepx = u32_at(&table, fc_sed + 10) as usize;
    let cb_sepx = u16_at(&word, fc_sepx) as usize;
    assert!(fc_sepx + 2 + cb_sepx <= word.len());

    // PlcBteChpx / PlcBtePapx / PlcBteLvc: FKP pages on 512-byte boundaries.
    for (index, expect_pages) in [(12, 1usize), (13, 1), (86, 1)] {
        let (fc, lcb) = fc_lcb(&word, index);
        let pages = (lcb - 4) / 8;
        assert_eq!(pages, expect_pages, "PlcBte index {index}");
        assert_eq!(lcb, (pages + 1) * 4 + pages * 4);
        assert_eq!(u32_at(&table, fc) as usize, text_fc);
        assert_eq!(u32_at(&table, fc + pages * 4) as usize, text_fc + ccp * 2);
        for p in 0..pages {
            let pn = u32_at(&table, fc + (pages + 1) * 4 + p * 4) as usize;
            let page = &word[pn * 512..pn * 512 + 512];
            let crun = usize::from(page[511]);
            assert!(crun >= 1);
            assert_eq!(
                u32_at(page, 0),
                u32_at(&table, fc + p * 4),
                "first fc of page"
            );
        }
    }
    // Paragraph FKP: one run per paragraph, bounds ascend to the end of the text.
    let (fc, _) = fc_lcb(&word, 13);
    let pn = u32_at(&table, fc + 8) as usize;
    let page = &word[pn * 512..pn * 512 + 512];
    let runs = usize::from(page[511]);
    assert_eq!(runs, 6, "paragraph count");
    assert_eq!(u32_at(page, runs * 4) as usize, text_fc + ccp * 2);
    for i in 0..runs {
        assert!(u32_at(page, i * 4) < u32_at(page, i * 4 + 4));
    }

    assert_eq!(LegacyOle2Codec::unpack_doc(&bytes).unwrap(), payload);
}

fn ccp_bytes(s: &str) -> usize {
    s.encode_utf16().count() * 2
}

#[test]
fn doc_text_is_recoverable_through_the_piece_table_alone() {
    let payload = doc_payload();
    let bytes = LegacyOle2Codec::pack_doc(&payload).unwrap();
    let mut streams = Ole2ContainerCodec::unpack(&bytes).unwrap();
    for s in &mut streams {
        if s.name == "WordDocument" {
            // Break the private footer magic: readers must fall back to the CLX path.
            let n = s.data.len();
            s.data[n - 4..].copy_from_slice(&[0, 0, 0, 0]);
        }
    }
    let rebuilt = Ole2ContainerCodec::pack(
        &streams
            .into_iter()
            .map(|s| (s.name, s.data))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let decoded = LegacyOle2Codec::unpack_doc(&rebuilt).unwrap();
    assert_eq!(
        decoded.paragraphs,
        vec![
            "첫째 문단 Hello",
            "둘째 문단입니다 😀",
            "Third paragraph",
            "이름\t값",
            "가\t1"
        ]
    );
}

#[test]
fn doc_roundtrip_covers_empty_and_many_paragraphs() {
    let empty = LegacyDocPayload {
        title: "문서".to_string(),
        paragraphs: vec![],
        tables: vec![],
    };
    let bytes = LegacyOle2Codec::pack_doc(&empty).unwrap();
    assert_eq!(LegacyOle2Codec::unpack_doc(&bytes).unwrap(), empty);
    let word = stream(&bytes, "WordDocument");
    assert_eq!(
        u32_at(&word, 76),
        1,
        "an empty document still has its paragraph mark"
    );

    let many = LegacyDocPayload {
        title: "긴 문서".to_string(),
        paragraphs: (0..100)
            .map(|i| format!("문단 {i} 한글 text\nline"))
            .collect(),
        tables: vec![],
    };
    let bytes = LegacyOle2Codec::pack_doc(&many).unwrap();
    assert_eq!(LegacyOle2Codec::unpack_doc(&bytes).unwrap(), many);
    let word = stream(&bytes, "WordDocument");
    let table = stream(&bytes, "1Table");
    let (fc, lcb) = fc_lcb(&word, 13);
    let pages = (lcb - 4) / 8;
    assert_eq!(pages, 4, "100 paragraphs need four 29-run PAPX pages");
    let mut seen = 0usize;
    for p in 0..pages {
        let pn = u32_at(&table, fc + (pages + 1) * 4 + p * 4) as usize;
        assert!(pn * 512 + 512 <= word.len());
        seen += usize::from(word[pn * 512 + 511]);
    }
    assert_eq!(seen, 100);
}

fn slide(title: &str, bullets: &[&str], notes: &str) -> LegacyPptSlide {
    LegacyPptSlide {
        title: title.to_string(),
        bullets: bullets.iter().map(ToString::to_string).collect(),
        notes: notes.to_string(),
        theme: "SignatureLavender".to_string(),
        transition: "Fade".to_string(),
    }
}

struct Rec {
    ver: u16,
    ty: u16,
    start: usize,
    end: usize,
}

fn rec_at(d: &[u8], off: usize) -> Rec {
    let vi = u16_at(d, off);
    let len = u32_at(d, off + 4) as usize;
    assert!(
        off + 8 + len <= d.len(),
        "record at {off} overruns the stream"
    );
    Rec {
        ver: vi & 0xF,
        ty: u16_at(d, off + 2),
        start: off + 8,
        end: off + 8 + len,
    }
}

fn children(d: &[u8], r: &Rec) -> Vec<Rec> {
    assert_eq!(r.ver, 0xF, "not a container");
    let mut out = Vec::new();
    let mut off = r.start;
    while off < r.end {
        let c = rec_at(d, off);
        off = c.end;
        out.push(c);
    }
    assert_eq!(off, r.end);
    out
}

fn collect_text(d: &[u8], r: &Rec, out: &mut Vec<String>) {
    if r.ver == 0xF {
        for c in children(d, r) {
            collect_text(d, &c, out);
        }
    } else if r.ty == 0x0FA0 {
        out.push(utf16(&d[r.start..r.end]));
    }
}

#[test]
fn ppt_current_user_edit_chain_resolves_to_slide_containers() {
    let payload = LegacyPptPayload {
        title: "발표자료".to_string(),
        slides: vec![
            slide("첫 슬라이드", &["항목 하나", "항목 둘"], "메모"),
            slide("Second", &["Alpha", "Beta", "Gamma"], ""),
            slide("제목만", &[], ""),
        ],
    };
    let bytes = LegacyOle2Codec::pack_ppt(&payload).unwrap();
    let cu = stream(&bytes, "Current User");
    let d = stream(&bytes, "PowerPoint Document");

    // Current User -> UserEditAtom
    assert_eq!(u16_at(&cu, 2), 0x0FF6);
    assert_eq!(u32_at(&cu, 8), 0x14);
    assert_eq!(u32_at(&cu, 12), 0xE391_C05F, "header token (not encrypted)");
    let edit_off = u32_at(&cu, 16) as usize;
    let ue = rec_at(&d, edit_off);
    assert_eq!(ue.ty, 0x0FF5);
    assert_eq!(ue.end, d.len(), "UserEditAtom is the last record");
    assert_eq!(d[ue.start + 7], 3, "majorVersion");
    assert_eq!(u32_at(&d, ue.start + 8), 0, "offsetLastEdit");
    let dir_off = u32_at(&d, ue.start + 12) as usize;
    let doc_ref = u32_at(&d, ue.start + 16);
    let seed = u32_at(&d, ue.start + 20);
    assert_eq!(doc_ref, 1);
    assert_eq!(seed, 2 + 3, "max persist id");
    assert_eq!(u32_at(&d, ue.start), 256 + 2, "lastSlideIdRef");

    // PersistDirectoryAtom -> offsets
    let dir = rec_at(&d, dir_off);
    assert_eq!(dir.ty, 0x1772);
    assert_eq!(dir.end, edit_off);
    let mut offsets = std::collections::BTreeMap::new();
    let mut p = dir.start;
    while p < dir.end {
        let word = u32_at(&d, p);
        let first = word & 0xF_FFFF;
        let count = (word >> 20) as usize;
        p += 4;
        for i in 0..count {
            offsets.insert(first + i as u32, u32_at(&d, p) as usize);
            p += 4;
        }
    }
    assert_eq!(p, dir.end);
    assert_eq!(offsets.len(), 5);
    assert_eq!(rec_at(&d, offsets[&1]).ty, 0x03E8, "Document");
    assert_eq!(rec_at(&d, offsets[&2]).ty, 0x03F8, "MainMaster");

    // Document -> slide SlideListWithText -> SlidePersistAtoms agree with the directory.
    let doc = rec_at(&d, offsets[&1]);
    let lists: Vec<Rec> = children(&d, &doc)
        .into_iter()
        .filter(|c| c.ty == 0x0FF0)
        .collect();
    assert_eq!(lists.len(), 2, "master list and slide list");
    let slide_list = &lists[1];
    assert_eq!(
        u16_at(&d, slide_list.start - 8) >> 4,
        0,
        "slide list instance"
    );
    let atoms = children(&d, slide_list);
    assert_eq!(atoms.len(), 3);
    let expected_text = [
        vec!["첫 슬라이드", "항목 하나\r항목 둘"],
        vec!["Second", "Alpha\rBeta\rGamma"],
        vec!["제목만"],
    ];
    for (i, a) in atoms.iter().enumerate() {
        assert_eq!(a.ty, 0x03F3);
        let persist_ref = u32_at(&d, a.start);
        assert_eq!(persist_ref, 3 + i as u32);
        assert_eq!(u32_at(&d, a.start + 12), 256 + i as u32, "slideId");
        let sl = rec_at(&d, offsets[&persist_ref]);
        assert_eq!(sl.ty, 0x03EE, "persisted object is a SlideContainer");
        let kids = children(&d, &sl);
        assert_eq!(kids[0].ty, 0x03EF, "SlideAtom first");
        assert_eq!(u32_at(&d, kids[0].start + 12), 0x8000_0000);
        let mut texts = Vec::new();
        collect_text(&d, &sl, &mut texts);
        assert_eq!(texts, expected_text[i]);
    }

    assert_eq!(LegacyOle2Codec::unpack_ppt(&bytes).unwrap(), payload);
}

#[test]
fn ppt_roundtrip_covers_zero_and_many_slides() {
    let none = LegacyPptPayload {
        title: "빈 발표".to_string(),
        slides: vec![],
    };
    let bytes = LegacyOle2Codec::pack_ppt(&none).unwrap();
    assert_eq!(LegacyOle2Codec::unpack_ppt(&bytes).unwrap(), none);
    let d = stream(&bytes, "PowerPoint Document");
    let cu = stream(&bytes, "Current User");
    let edit = rec_at(&d, u32_at(&cu, 16) as usize);
    assert_eq!(edit.ty, 0x0FF5);

    let many = LegacyPptPayload {
        title: "긴 발표".to_string(),
        slides: (0..40)
            .map(|i| {
                slide(
                    &format!("슬라이드 {i}"),
                    &["하나", "둘 two"],
                    &format!("노트 {i}"),
                )
            })
            .collect(),
    };
    let bytes = LegacyOle2Codec::pack_ppt(&many).unwrap();
    assert_eq!(LegacyOle2Codec::unpack_ppt(&bytes).unwrap(), many);
}

#[test]
fn ppt_reader_still_accepts_the_previous_nested_layout() {
    // Previous writer output: slides nested directly inside RT_Document, no Current User.
    fn atom(buf: &mut Vec<u8>, word: u16, ty: u16, body: &[u8]) {
        buf.extend_from_slice(&word.to_le_bytes());
        buf.extend_from_slice(&ty.to_le_bytes());
        buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
        buf.extend_from_slice(body);
    }
    fn u16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }
    let mut slide_kids = Vec::new();
    atom(&mut slide_kids, 0x0002, 0x03EF, &u16le("Theme|Cut"));
    atom(&mut slide_kids, 0x0000, 0x0FA0, &u16le("옛 제목"));
    atom(&mut slide_kids, 0x0020, 0x0FA0, &u16le("옛 노트"));
    atom(&mut slide_kids, 0x0030, 0x0FA0, &u16le("옛 항목"));
    let mut doc_kids = Vec::new();
    atom(&mut doc_kids, 0x0000, 0x0FA0, &u16le("옛 발표"));
    atom(&mut doc_kids, 0x000F, 0x03EE, &slide_kids);
    let mut stream_bytes = Vec::new();
    atom(&mut stream_bytes, 0x000F, 0x03E8, &doc_kids);
    let bytes =
        Ole2ContainerCodec::pack(&[("PowerPoint Document".to_string(), stream_bytes)]).unwrap();
    let decoded = LegacyOle2Codec::unpack_ppt(&bytes).unwrap();
    assert_eq!(decoded.title, "옛 발표");
    assert_eq!(decoded.slides.len(), 1);
    assert_eq!(decoded.slides[0].title, "옛 제목");
    assert_eq!(decoded.slides[0].notes, "옛 노트");
    assert_eq!(decoded.slides[0].bullets, vec!["옛 항목"]);
    assert_eq!(decoded.slides[0].theme, "Theme");
    assert_eq!(decoded.slides[0].transition, "Cut");
}
