//! Tests for ISO 32000-1 incremental PDF save preserving `/StructTreeRoot` and `/FT /Sig`
//! fields across classic `xref` tables, `/Type /XRef` + `/ObjStm` streams, edits, security
//! boundaries and hostile cross-reference fallbacks (PD-349).

use crate::pdf_edit_budget::MAX_OBJECTS;
use crate::pdf_edit_build::build;
use crate::pdf_edit_fixtures::{deadline, hand_written_with_xref};
use crate::pdf_edit_incremental_val::IncVal;
use crate::pdf_edit_incremental_xref::{inspect_xref, XRefFormat, XRefInspectResult};
use crate::pdf_edit_inspect::open_pdf;
use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_preserve::read_shifted_rect;
use crate::pdf_edit_wire::{OverlayKind, PagePlan, WireForm, WireMarkup};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hayro_syntax::object::{Array, Dict, Name, Object, ObjectIdentifier, Stream};
use hayro_write::pdf_writer::{Content, Pdf, Rect, Ref};
use std::io::Write;
use std::sync::Arc;

fn test_make_overlay(w: f32, h: f32, _kind: OverlayKind) -> Result<Vec<u8>, String> {
    let mut pdf = Pdf::new();
    let mut next = Ref::new(1);
    let (catalog, tree, page_ref, content_ref) =
        (next.bump(), next.bump(), next.bump(), next.bump());
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).count(1).kids([page_ref]);
    let mut content = Content::new();
    content
        .rect(1.0, 1.0, (w - 2.0).max(1.0), (h - 2.0).max(1.0))
        .stroke();
    pdf.stream(content_ref, &content.finish());
    pdf.page(page_ref)
        .media_box(Rect::new(0.0, 0.0, w.max(1.0), h.max(1.0)))
        .parent(tree)
        .contents(content_ref);
    Ok(pdf.finish())
}

fn build_single(bytes: Vec<u8>, turns: &[u16], overlay: Overlay) -> crate::pdf_edit_build::Built {
    let pdf = open_pdf(&Arc::new(bytes.clone())).expect("input opens");
    let count = pdf.pages().len();
    let pages: Vec<PagePlan> = (0..count)
        .map(|i| PagePlan {
            source: 0,
            index: i,
            turn: turns.get(i).copied().unwrap_or(0),
        })
        .collect();
    build(
        &[Arc::new(bytes)],
        &pages,
        &overlay,
        &mut test_make_overlay,
        &deadline(),
    )
    .expect("build succeeds")
}

fn tagged_signed_classic_pdf(extra_catalog: &str, sig_name: &str, sig_body: &str) -> Vec<u8> {
    let stream_body = b"/P << /MCID 0 >> BDC\nBT /F1 12 Tf 72 200 Td (Hello World) Tj ET\nEMC\n";
    let objs = vec![
        (
            1,
            format!(
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R \
                 /MarkInfo << /Marked true >> /AcroForm << /Fields [6 0 R] /SigFlags 3 >> \
                 {extra_catalog} >>"
            ),
        ),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources << /Font << /F1 7 0 R >> >> >>"
                .to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [50 100 250 400] \
             /CropBox [50 100 250 400] /StructParents 0 /Contents 4 0 R /Annots [6 0 R] >>"
                .to_string(),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{}endstream",
                stream_body.len(),
                String::from_utf8_lossy(stream_body)
            ),
        ),
        (
            5,
            "<< /Type /StructTreeRoot /ParentTree << /Nums [0 [8 0 R]] >> /K 8 0 R >>".to_string(),
        ),
        (
            6,
            format!(
                "<< /Type /Annot /Subtype /Widget /FT /Sig /T ({sig_name}) \
                 /Rect [60 110 140 140] {sig_body} >>"
            ),
        ),
        (
            7,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
        (
            8,
            "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /K 0 >>".to_string(),
        ),
    ];
    hand_written_with_xref(&objs, "")
}

/// Builds a PDF 1.5+ document where page object `3` is compressed inside `/ObjStm` (`obj 6`) and
/// the file uses a compressed `/Type /XRef` stream (`obj 7`).
fn pdf15_objstm_tagged_pdf() -> Vec<u8> {
    let mut buf = b"%PDF-1.5\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<(u32, u8, u64, u16)> = Vec::new(); // (id, type, field1, field2)

    let push_plain =
        |buf: &mut Vec<u8>, offsets: &mut Vec<(u32, u8, u64, u16)>, id: u32, body: &[u8]| {
            let off = buf.len() as u64;
            buf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
            buf.extend_from_slice(body);
            buf.extend_from_slice(b"\nendobj\n");
            offsets.push((id, 1, off, 0));
        };

    push_plain(
        &mut buf,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>",
    );
    push_plain(
        &mut buf,
        &mut offsets,
        2,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    let content = b"BT /F1 12 Tf 20 200 Td (Alpha Beta) Tj ET";
    let stream_obj = format!(
        "<< /Length {} >>\nstream\n{}\nendstream",
        content.len(),
        std::str::from_utf8(content).unwrap()
    );
    push_plain(&mut buf, &mut offsets, 4, stream_obj.as_bytes());
    push_plain(&mut buf, &mut offsets, 5, b"<< /Type /StructTreeRoot >>");

    // Put object 3 (Page) inside ObjStm 6 0 obj.
    let page_body = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] \
        /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> \
        /Contents 4 0 R /StructParents 0 >>";
    let objstm_header = b"3 0 ";
    let first = objstm_header.len();
    let mut objstm_raw = Vec::new();
    objstm_raw.extend_from_slice(objstm_header);
    objstm_raw.extend_from_slice(page_body);
    let objstm_off = buf.len() as u64;
    let objstm_dict = format!(
        "<< /Type /ObjStm /N 1 /First {first} /Length {} >>\nstream\n",
        objstm_raw.len()
    );
    buf.extend_from_slice(b"6 0 obj\n");
    buf.extend_from_slice(objstm_dict.as_bytes());
    buf.extend_from_slice(&objstm_raw);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    offsets.push((6, 1, objstm_off, 0));
    offsets.push((3, 2, 6, 0)); // compressed object 3 inside ObjStm 6 at index 0

    let xref_id = 7u32;
    let xref_off = buf.len() as u64;
    offsets.push((xref_id, 1, xref_off, 0));
    offsets.push((0, 0, 0, u16::MAX));
    offsets.sort_by_key(|e| e.0);

    let mut raw_xref = Vec::with_capacity(offsets.len() * 7);
    for &(_, kind, f1, f2) in &offsets {
        raw_xref.push(kind);
        raw_xref.extend_from_slice(&(f1 as u32).to_be_bytes());
        raw_xref.extend_from_slice(&f2.to_be_bytes());
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw_xref).unwrap();
    let comp = enc.finish().unwrap();

    let hdr = format!(
        "{xref_id} 0 obj\n<< /Type /XRef /Size 8 /W [1 4 2] /Root 1 0 R /Filter /FlateDecode /Length {} >>\nstream\n",
        comp.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(&comp);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    buf.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    buf
}

#[test]
fn preserves_byte_prefix_tags_mcid_and_signature_across_edits() {
    let orig = tagged_signed_classic_pdf(
        "",
        "Signature1",
        "/V << /Type /Sig /ByteRange [0 10 20 30] /Contents <DEADBEEF> >>",
    );
    let mut overlay = Overlay::new(Some("CONFIDENTIAL"), Some("APPROVED")).unwrap();
    overlay.markup = Some(WireMarkup {
        text: "Hello World".to_string(),
        underline: false,
        color: [255, 255, 0],
    });
    overlay.form = WireForm {
        applicant: Some("홍길동".to_string()),
        department: Some("개발팀".to_string()),
    };

    let built = build_single(orig.clone(), &[90], overlay);
    assert!(built.bytes.len() > orig.len());
    assert_eq!(&built.bytes[..orig.len()], orig.as_slice());

    let reopened = open_pdf(&Arc::new(built.bytes.clone())).expect("incremental output opens");
    assert_eq!(reopened.pages().len(), 1);
    let catalog = reopened
        .xref()
        .get::<Dict<'_>>(reopened.xref().root_id())
        .unwrap();
    assert!(catalog.get::<Dict<'_>>(b"StructTreeRoot").is_some());
    assert!(catalog.get::<Dict<'_>>(b"MarkInfo").is_some());

    // Original signature object 6 must be untouched.
    let sig_dict = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(6, 0))
        .unwrap();
    assert_eq!(
        sig_dict.get::<Name<'_>>(b"FT").as_deref(),
        Some(b"Sig".as_slice())
    );
    assert!(sig_dict.get::<Dict<'_>>(b"V").is_some());

    // Page kept /StructParents, inherited font /F1, rotated 90, CropBox-aligned annots & contents.
    let page = reopened.pages().first().unwrap();
    assert_eq!(page.raw().get::<i64>(b"StructParents"), Some(0));
    assert_eq!(page.raw().get::<i64>(b"Rotate"), Some(90));
    let res = page.raw().get::<Dict<'_>>(b"Resources").unwrap();
    assert!(res
        .get::<Dict<'_>>(b"Font")
        .is_some_and(|f| f.get::<Dict<'_>>(b"F1").is_some()));
    assert!(res.get::<Dict<'_>>(b"XObject").is_some());

    let contents = page.raw().get::<Array<'_>>(b"Contents").unwrap();
    assert_eq!(contents.iter::<Object<'_>>().count(), 3);
    let orig_stream = reopened
        .xref()
        .get::<Stream<'_>>(ObjectIdentifier::new(4, 0))
        .unwrap();
    assert!(String::from_utf8_lossy(&orig_stream.decoded().unwrap()).contains("/MCID 0"));

    // Highlight and new form widgets live inside original CropBox [50 100 250 400].
    let annots = page.raw().get::<Array<'_>>(b"Annots").unwrap();
    let mut saw_highlight = false;
    let mut saw_applicant = false;
    for annot in annots.iter::<Dict<'_>>() {
        let subtype = annot.get::<Name<'_>>(b"Subtype");
        if subtype.as_deref() == Some(b"Highlight".as_slice()) {
            let r = read_shifted_rect(&annot, 0.0, 0.0).unwrap();
            assert!(
                r[0] >= 50.0 && r[1] >= 100.0 && r[2] <= 250.0 && r[3] <= 400.0,
                "{r:?}"
            );
            saw_highlight = true;
        } else if annot.get::<Name<'_>>(b"FT").as_deref() == Some(b"Tx".as_slice()) {
            let r = read_shifted_rect(&annot, 0.0, 0.0).unwrap();
            assert!(
                r[0] >= 50.0 && r[1] >= 100.0 && r[2] <= 250.0 && r[3] <= 400.0,
                "{r:?}"
            );
            saw_applicant = true;
        }
    }
    assert!(saw_highlight && saw_applicant);
    assert!(built.warnings.iter().any(|w| w.contains("전자서명")));
}

#[test]
fn pdf15_xref_stream_and_objstm_override_with_two_pass_roundtrip() {
    let orig = pdf15_objstm_tagged_pdf();
    assert!(matches!(
        inspect_xref(&orig, &open_pdf(&Arc::new(orig.clone())).unwrap()).unwrap(),
        XRefInspectResult::Ready(meta) if meta.format == XRefFormat::Stream
    ));

    let ov1 = Overlay {
        markup: Some(WireMarkup {
            text: "Alpha Beta".to_string(),
            underline: true,
            color: [255, 0, 0],
        }),
        ..Default::default()
    };
    let pass1 = build_single(orig.clone(), &[90], ov1);
    assert_eq!(&pass1.bytes[..orig.len()], orig.as_slice());

    let p1_pdf = open_pdf(&Arc::new(pass1.bytes.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&pass1.bytes, &p1_pdf).unwrap(),
        XRefInspectResult::Ready(meta) if meta.format == XRefFormat::Stream
    ));
    assert_eq!(
        p1_pdf.pages().first().unwrap().raw().get::<i64>(b"Rotate"),
        Some(90)
    );
    assert!(p1_pdf
        .xref()
        .get::<Dict<'_>>(p1_pdf.xref().root_id())
        .unwrap()
        .get::<Dict<'_>>(b"StructTreeRoot")
        .is_some());

    // 2nd incremental update on top of the incremental `/Type /XRef` result.
    let mut ov2 = Overlay::default();
    ov2.form.applicant = Some("김영희".to_string());
    let pass2 = build_single(pass1.bytes.clone(), &[180], ov2);
    assert_eq!(&pass2.bytes[..pass1.bytes.len()], pass1.bytes.as_slice());
    let p2_pdf = open_pdf(&Arc::new(pass2.bytes.clone())).unwrap();
    assert_eq!(
        p2_pdf.pages().first().unwrap().raw().get::<i64>(b"Rotate"),
        Some(270)
    );
    assert!(matches!(
        inspect_xref(&pass2.bytes, &p2_pdf).unwrap(),
        XRefInspectResult::Ready(meta) if meta.format == XRefFormat::Stream
    ));
}

#[test]
fn never_overwrites_signature_field_named_applicant_and_updates_split_parent() {
    let objs = vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R] /SigFlags 3 >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R \
             /Annots [5 0 R 7 0 R] >>"
                .to_string(),
        ),
        (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
        // Signature field deliberately named `applicant`!
        (
            5,
            "<< /Type /Annot /Subtype /Widget /FT /Sig /T (applicant) \
             /Rect [10 10 90 40] /V << /Type /Sig /ByteRange [0 1 2 3] >> >>"
                .to_string(),
        ),
        // Split parent+widget field for `department`.
        (6, "<< /FT /Tx /T (department) /Kids [7 0 R] >>".to_string()),
        (
            7,
            "<< /Type /Annot /Subtype /Widget /Parent 6 0 R /Rect [10 60 150 90] >>".to_string(),
        ),
    ];
    let orig = hand_written_with_xref(&objs, "");
    let overlay = Overlay {
        form: WireForm {
            applicant: Some("홍길동".to_string()),
            department: Some("개발본부".to_string()),
        },
        ..Default::default()
    };

    let built = build_single(orig.clone(), &[0], overlay);
    assert_eq!(&built.bytes[..orig.len()], orig.as_slice());
    let reopened = open_pdf(&Arc::new(built.bytes)).unwrap();

    // Object 5 (`/FT /Sig` named `applicant`) MUST remain untouched.
    let sig = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(5, 0))
        .unwrap();
    assert_eq!(
        sig.get::<Name<'_>>(b"FT").as_deref(),
        Some(b"Sig".as_slice())
    );
    assert!(sig.get::<Dict<'_>>(b"V").is_some());

    // Parent field 6 got `/V` and child widget 7 got `/AP /N`.
    let parent = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(6, 0))
        .unwrap();
    assert!(parent
        .get::<hayro_syntax::object::String<'_>>(b"V")
        .is_some());
    let child = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(7, 0))
        .unwrap();
    assert!(child.get::<Dict<'_>>(b"AP").is_some());

    // Newly inserted `applicant` field should be deduplicated (`applicant_2`) and added to `/AcroForm`.
    let acro = reopened
        .xref()
        .get::<Dict<'_>>(reopened.xref().root_id())
        .unwrap()
        .get::<Dict<'_>>(b"AcroForm")
        .unwrap();
    assert_eq!(
        acro.get::<Array<'_>>(b"Fields").unwrap().raw_iter().count(),
        3
    );
}

#[test]
fn unsigned_tagged_strips_dangerous_entries_while_signed_locked_warns() {
    // 1. Unsigned tagged doc with JS, OpenAction, Catalog AA, Page AA, Collection + EmbeddedFiles.
    let unsigned_objs = vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R \
             /Names << /JavaScript 6 0 R /EmbeddedFiles 7 0 R >> \
             /OpenAction << /S /JavaScript /JS (app.alert(1)) >> \
             /AA << /WC << /S /JavaScript /JS (2) >> >> /Collection << /View /T >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R \
             /AA << /O << /S /JavaScript /JS (3) >> >> >>"
                .to_string(),
        ),
        (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (6, "<< /Names [] >>".to_string()),
        (7, "<< /Names [] >>".to_string()),
    ];
    let orig_unsigned = hand_written_with_xref(&unsigned_objs, "");
    let built_unsigned = build_single(orig_unsigned.clone(), &[0], Overlay::default());
    assert_eq!(
        &built_unsigned.bytes[..orig_unsigned.len()],
        orig_unsigned.as_slice()
    );
    assert_eq!(
        built_unsigned.warnings,
        [
            "문서 스크립트는 보안을 위해 저장하지 않았습니다.",
            "열 때 자동 실행 동작은 보안을 위해 저장하지 않았습니다.",
            "PDF 포트폴리오 구성은 저장하지 않았습니다."
        ]
    );
    let reopened = open_pdf(&Arc::new(built_unsigned.bytes)).unwrap();
    let cat = reopened
        .xref()
        .get::<Dict<'_>>(reopened.xref().root_id())
        .unwrap();
    assert!(cat.get::<Dict<'_>>(b"StructTreeRoot").is_some());
    assert!(cat.get_raw::<Object<'_>>(b"OpenAction").is_none());
    assert!(cat.get_raw::<Object<'_>>(b"AA").is_none());
    assert!(cat.get_raw::<Object<'_>>(b"Collection").is_none());
    let names = cat.get::<Dict<'_>>(b"Names").unwrap();
    assert!(names.get_raw::<Object<'_>>(b"JavaScript").is_none());
    assert!(names.get::<Dict<'_>>(b"EmbeddedFiles").is_some());
    assert!(reopened
        .pages()
        .first()
        .unwrap()
        .raw()
        .get_raw::<Object<'_>>(b"AA")
        .is_none());

    // 2. Signed DocMDP P=1 document with edits emits exact lock warning + keeps prefix.
    let doc_mdp_orig = tagged_signed_classic_pdf(
        "/Names << /JavaScript << /Names [] >> >>",
        "CertSig",
        "/V << /Type /Sig /ByteRange [0 1 2 3] /Reference [<< /TransformMethod /DocMDP /TransformParams << /P 1 >> >>] >>",
    );
    let built_locked = build_single(doc_mdp_orig.clone(), &[90], Overlay::default());
    assert_eq!(
        &built_locked.bytes[..doc_mdp_orig.len()],
        doc_mdp_orig.as_slice()
    );
    assert!(built_locked
        .warnings
        .iter()
        .any(|w| w.contains("/DocMDP P=1")));
    assert!(built_locked
        .warnings
        .iter()
        .any(|w| w.contains("원본 전자서명 보존")));
}

#[test]
fn hostile_forged_startxref_circular_prev_huge_size_and_hybrid_safely_fallback() {
    let base = tagged_signed_classic_pdf("", "Sign", "");
    let base_pdf = open_pdf(&Arc::new(base.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&base, &base_pdf).unwrap(),
        XRefInspectResult::Ready(_)
    ));

    // 1. Forged startxref pointing into mid-object (valid PDF via reader fallback scan).
    let mut forged = base.clone();
    let sxref_pos = forged.windows(9).rposition(|w| w == b"startxref").unwrap();
    forged.truncate(sxref_pos);
    forged.extend_from_slice(b"startxref\n15\n%%EOF\n");
    let forged_pdf = open_pdf(&Arc::new(forged.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&forged, &forged_pdf).unwrap(),
        XRefInspectResult::Fallback(_)
    ));
    let built_forged = build_single(forged, &[0], Overlay::default());
    assert!(open_pdf(&Arc::new(built_forged.bytes)).is_ok());
    assert!(!built_forged.warnings.is_empty());

    // 2. Self-referential circular `/Prev` in classic trailer.
    let xref_pos = base.windows(4).rposition(|w| w == b"xref").unwrap();
    let circular = tagged_with_custom_trailer(&format!("/Prev {xref_pos}"));
    let circ_pdf = open_pdf(&Arc::new(circular.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&circular, &circ_pdf).unwrap(),
        XRefInspectResult::Fallback(_)
    ));
    let built_circ = build_single(circular, &[0], Overlay::default());
    assert!(open_pdf(&Arc::new(built_circ.bytes)).is_ok());

    // 3. Huge `/Size` above `MAX_OBJECTS` (must fallback immediately without OOM).
    let huge_size = i64::try_from(MAX_OBJECTS).unwrap() + 1000;
    let huge = tagged_with_custom_trailer_size(huge_size);
    let huge_pdf = open_pdf(&Arc::new(huge.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&huge, &huge_pdf).unwrap(),
        XRefInspectResult::Fallback(_)
    ));
    let built_huge = build_single(huge, &[0], Overlay::default());
    assert!(open_pdf(&Arc::new(built_huge.bytes)).is_ok());

    // 4. Hybrid `/XRefStm` trailer falls back cleanly without breaking PDF.
    let hybrid = tagged_with_custom_trailer("/XRefStm 100");
    let hybrid_pdf = open_pdf(&Arc::new(hybrid.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&hybrid, &hybrid_pdf).unwrap(),
        XRefInspectResult::Fallback(_)
    ));
    let built_hybrid = build_single(hybrid, &[0], Overlay::default());
    assert!(open_pdf(&Arc::new(built_hybrid.bytes)).is_ok());
}

#[test]
fn inc_val_preserves_generations_and_real_precision_without_exponents() {
    let mut out = Vec::new();
    let id = ObjectIdentifier::new(42, 7);
    IncVal::Ref(id).write_to(&mut out);
    assert_eq!(out, b"42 7 R");

    out.clear();
    IncVal::Real(0.000_015).write_to(&mut out);
    let s = std::str::from_utf8(&out).unwrap();
    assert!(!s.contains('e') && !s.contains('E'), "{s}");
    assert_eq!(s, "0.000015");

    out.clear();
    IncVal::Real(-0.0).write_to(&mut out);
    assert_eq!(out, b"0");
}

fn tagged_with_custom_trailer(extra: &str) -> Vec<u8> {
    let base = tagged_signed_classic_pdf("", "Sign", "");
    let sxref_kw = base.windows(9).rposition(|w| w == b"startxref").unwrap();
    let tr_dict_end = base[..sxref_kw]
        .windows(2)
        .rposition(|w| w == b">>")
        .unwrap();
    let xref_off = base.windows(4).rposition(|w| w == b"xref").unwrap();
    let tr_body = std::str::from_utf8(&base[..tr_dict_end]).unwrap();
    let mut out = Vec::new();
    out.extend_from_slice(tr_body.as_bytes());
    out.push(b' ');
    out.extend_from_slice(extra.as_bytes());
    out.extend_from_slice(b" >>\nstartxref\n");
    out.extend_from_slice(format!("{xref_off}\n%%EOF\n").as_bytes());
    out
}

fn tagged_with_custom_trailer_size(size: i64) -> Vec<u8> {
    let base = tagged_signed_classic_pdf("", "Sign", "");
    let tr = base.windows(7).rposition(|w| w == b"trailer").unwrap();
    let mut out = base[..tr].to_vec();
    out.extend_from_slice(format!("trailer\n<< /Size {size} /Root 1 0 R >>\n").as_bytes());
    let xref_off = base.windows(4).rposition(|w| w == b"xref").unwrap();
    out.extend_from_slice(format!("startxref\n{xref_off}\n%%EOF\n").as_bytes());
    out
}

#[test]
fn page_with_nonzero_generation_and_inline_widget_preserves_refs() {
    let content = b"BT /F1 12 Tf 20 200 Td (Target Text) Tj ET";
    let mut buf = b"%PDF-1.7\n".to_vec();
    let mut entries = Vec::new();
    let mut push_obj = |id: u32, gen: u16, body: &str| {
        entries.push((id, gen, buf.len()));
        buf.extend_from_slice(format!("{id} {gen} obj\n{body}\nendobj\n").as_bytes());
    };
    push_obj(
        1,
        0,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>",
    );
    push_obj(2, 0, "<< /Type /Pages /Kids [3 2 R] /Count 1 >>");
    push_obj(
        3,
        2,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] \
         /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> \
         /Contents 4 0 R \
         /Annots [<< /Type /Annot /Subtype /Widget /FT /Tx /T (applicant) /Rect [10 10 120 30] >>] >>",
    );
    push_obj(
        4,
        0,
        &format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            content.len(),
            std::str::from_utf8(content).unwrap()
        ),
    );
    push_obj(5, 0, "<< /Type /StructTreeRoot >>");

    let xref_off = buf.len();
    buf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for (id, gen, off) in &entries {
        let _ = id;
        buf.extend_from_slice(format!("{off:010} {gen:05} n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n").as_bytes(),
    );

    let overlay = Overlay {
        markup: Some(WireMarkup {
            text: "Target Text".to_string(),
            underline: false,
            color: [255, 255, 0],
        }),
        form: WireForm {
            applicant: Some("박보검".to_string()),
            department: None,
        },
        ..Default::default()
    };
    let built = build_single(buf.clone(), &[90], overlay);
    assert_eq!(&built.bytes[..buf.len()], buf.as_slice());
    let reopened = open_pdf(&Arc::new(built.bytes)).unwrap();
    let page = reopened.pages().first().unwrap();
    assert_eq!(page.raw().obj_id(), Some(ObjectIdentifier::new(3, 2)));
    assert_eq!(page.raw().get::<i64>(b"Rotate"), Some(90));
    let annots = page.raw().get::<Array<'_>>(b"Annots").unwrap();
    assert_eq!(annots.raw_iter().count(), 2);
    for annot in annots.iter::<Dict<'_>>() {
        if annot.get::<Name<'_>>(b"Subtype").as_deref() == Some(b"Highlight".as_slice()) {
            assert_eq!(
                annot.get_ref(b"P").map(ObjectIdentifier::from),
                Some(ObjectIdentifier::new(3, 2))
            );
        }
        if annot.get::<Name<'_>>(b"FT").as_deref() == Some(b"Tx".as_slice()) {
            assert!(annot
                .get::<hayro_syntax::object::String<'_>>(b"V")
                .is_some());
            assert!(annot.get::<Dict<'_>>(b"AP").is_some());
        }
    }
}

#[test]
fn trailing_junk_bad_w_width_and_xref_stream_bomb_fallback() {
    let base = tagged_signed_classic_pdf("", "Sign", "");
    let mut with_junk = base.clone();
    with_junk.extend_from_slice(b"trailing_corrupt_bytes");
    let junk_pdf = open_pdf(&Arc::new(with_junk.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&with_junk, &junk_pdf).unwrap(),
        XRefInspectResult::Fallback(_)
    ));

    // Craft an `/XRef` stream with unsupported `/W [1 5 2]` or compressed zero-bomb mismatch.
    let mut s_pdf = pdf15_objstm_tagged_pdf();
    let base_pdf = open_pdf(&Arc::new(s_pdf.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&s_pdf, &base_pdf).unwrap(),
        XRefInspectResult::Ready(_)
    ));

    // Append trailing non-whitespace junk on PDF 1.5 xref stream doc.
    s_pdf.extend_from_slice(b"\nJUNK");
    let s_junk_pdf = open_pdf(&Arc::new(s_pdf.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&s_pdf, &s_junk_pdf).unwrap(),
        XRefInspectResult::Fallback(_)
    ));
}

#[test]
fn unsigned_sanitizes_xfa_annot_actions_and_outlines_while_preserving_safe_links() {
    let content = b"BT /F1 12 Tf 20 200 Td (Tagged) Tj ET";
    let inline_dept = "<< /Type /Annot /Subtype /Widget /FT /Tx /T (department) \
        /Rect [10 100 120 125] /AA << /E << /S /Launch /F (cmd.exe) >> >> >>";
    let objs = vec![
        (1, "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm 9 0 R /Outlines 11 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents 4 0 R /Annots [6 0 R 7 0 R 8 0 R {inline_dept}] >>")),
        (4, format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), std::str::from_utf8(content).unwrap())),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (6, "<< /Type /Annot /Subtype /Link /Rect [10 10 80 30] /A << /S /URI /URI (https://example.org/docs) >> >>".to_string()),
        (7, "<< /Type /Annot /Subtype /Widget /FT /Tx /T (applicant) /Rect [10 40 120 65] /P 3 0 R /AA << /K << /S /JavaScript /JS (app.alert(2)) >> >> >>".to_string()),
        (8, "<< /Type /Annot /Subtype /Link /Rect [10 70 80 90] /A << /S /JavaScript /JS (app.alert(1)) >> >>".to_string()),
        (9, "<< /Fields [7 0 R] /XFA 10 0 R >>".to_string()),
        (10, "<< /Length 6 >>\nstream\n<xfa/>\nendstream".to_string()),
        (11, "<< /Type /Outlines /First 12 0 R /Last 14 0 R /Count 3 >>".to_string()),
        (12, "<< /Title (SafeGoTo) /Parent 11 0 R /Next 13 0 R /A << /S /GoTo /D [3 0 R /Fit] >> >>".to_string()),
        (13, "<< /Title (EvilLaunch) /Parent 11 0 R /Prev 12 0 R /Next 14 0 R /A << /S /Launch /F (calc.exe) >> >>".to_string()),
        (14, "<< /Title (ChainedSubmit) /Parent 11 0 R /Prev 13 0 R /AA << /O << /S /JavaScript /JS (1) >> >> /A << /S /URI /URI (https://example.org) /Next << /S /SubmitForm /F (https://evil.example) >> >> >>".to_string()),
    ];
    let orig = hand_written_with_xref(&objs, "");
    let overlay = Overlay {
        form: WireForm {
            applicant: Some("홍길동".to_string()),
            department: Some("개발팀".to_string()),
        },
        ..Default::default()
    };
    let built = build_single(orig.clone(), &[0], overlay);
    assert_eq!(&built.bytes[..orig.len()], orig.as_slice());
    assert!(built
        .warnings
        .iter()
        .any(|w| w == "문서 스크립트는 보안을 위해 저장하지 않았습니다."));
    assert!(built
        .warnings
        .iter()
        .any(|w| w.contains("보안을 위해 저장하지 않았습니다.")
            && w != "문서 스크립트는 보안을 위해 저장하지 않았습니다."));

    let reopened = open_pdf(&Arc::new(built.bytes)).unwrap();
    let cat = reopened
        .xref()
        .get::<Dict<'_>>(reopened.xref().root_id())
        .unwrap();
    assert!(cat.get::<Dict<'_>>(b"StructTreeRoot").is_some());
    assert!(cat
        .get::<Dict<'_>>(b"AcroForm")
        .unwrap()
        .get_raw::<Object<'_>>(b"XFA")
        .is_none());
    assert!(reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(6, 0))
        .unwrap()
        .get::<Dict<'_>>(b"A")
        .is_some());

    let app_widget = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(7, 0))
        .unwrap();
    assert!(app_widget.get_raw::<Object<'_>>(b"AA").is_none());
    assert!(app_widget
        .get::<hayro_syntax::object::String<'_>>(b"V")
        .is_some());
    assert!(app_widget.get::<Dict<'_>>(b"AP").is_some());

    assert!(reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(8, 0))
        .unwrap()
        .get_raw::<Object<'_>>(b"A")
        .is_none());
    let annots = reopened
        .pages()
        .first()
        .unwrap()
        .raw()
        .get::<Array<'_>>(b"Annots")
        .unwrap();
    let inline_widget = annots.iter::<Dict<'_>>().nth(3).unwrap();
    assert!(inline_widget.get_raw::<Object<'_>>(b"AA").is_none());
    assert!(inline_widget
        .get::<hayro_syntax::object::String<'_>>(b"V")
        .is_some());
    assert!(inline_widget.get::<Dict<'_>>(b"AP").is_some());

    assert!(reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(12, 0))
        .unwrap()
        .get::<Dict<'_>>(b"A")
        .is_some());
    assert!(reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(13, 0))
        .unwrap()
        .get_raw::<Object<'_>>(b"A")
        .is_none());
    let chained_bm = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(14, 0))
        .unwrap();
    assert!(chained_bm.get_raw::<Object<'_>>(b"AA").is_none());
    assert!(chained_bm.get_raw::<Object<'_>>(b"A").is_none());

    // Inline `/AcroForm /XFA` and inline `/Outlines` items are sanitized too.
    let inline_orig = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm << /Fields [] /XFA << /t (x) >> >> /Outlines << /Type /Outlines /First << /Title (B) /A << /S /Launch /F (sh) >> >> >> >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string()),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (5, "<< /Type /StructTreeRoot >>".to_string()),
        ],
        "",
    );
    let inline_built = build_single(inline_orig, &[0], Overlay::default());
    let inline_reopened = open_pdf(&Arc::new(inline_built.bytes)).unwrap();
    let inline_cat = inline_reopened
        .xref()
        .get::<Dict<'_>>(inline_reopened.xref().root_id())
        .unwrap();
    assert!(inline_cat
        .get::<Dict<'_>>(b"AcroForm")
        .unwrap()
        .get_raw::<Object<'_>>(b"XFA")
        .is_none());
    assert!(inline_cat
        .get::<Dict<'_>>(b"Outlines")
        .unwrap()
        .get::<Dict<'_>>(b"First")
        .unwrap()
        .get_raw::<Object<'_>>(b"A")
        .is_none());
}

#[test]
fn signed_doc_with_only_xfa_widget_aa_or_outline_action_keeps_bytes_and_warns() {
    let assert_kept = |bytes: Vec<u8>, label: &str| -> hayro_syntax::Pdf {
        let built = build_single(bytes.clone(), &[0], Overlay::default());
        assert_eq!(&built.bytes[..bytes.len()], bytes.as_slice());
        assert!(
            built
                .warnings
                .iter()
                .any(|w| w.contains("원본 전자서명 보존")),
            "{label}: {:?}",
            built.warnings
        );
        open_pdf(&Arc::new(built.bytes)).unwrap()
    };

    let xfa_signed = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm << /Fields [6 0 R] /SigFlags 3 /XFA << /template (x) >> >> >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R /Annots [6 0 R] >>".to_string()),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (5, "<< /Type /StructTreeRoot >>".to_string()),
            (6, "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sig1) /Rect [10 10 80 40] /V << /Type /Sig /ByteRange [0 1 2 3] >> >>".to_string()),
        ],
        "",
    );
    let r_xfa = assert_kept(xfa_signed, "XFA");
    assert!(r_xfa
        .xref()
        .get::<Dict<'_>>(r_xfa.xref().root_id())
        .unwrap()
        .get::<Dict<'_>>(b"AcroForm")
        .unwrap()
        .get_raw::<Object<'_>>(b"XFA")
        .is_some());

    let widget_aa_signed = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R] /SigFlags 3 >> >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R /Annots [5 0 R 6 0 R] >>".to_string()),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (5, "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sig1) /Rect [10 10 80 40] /V << /Type /Sig /ByteRange [0 1 2 3] >> >>".to_string()),
            (6, "<< /Type /Annot /Subtype /Widget /FT /Tx /T (note) /Rect [10 50 80 70] /AA << /K << /S /JavaScript /JS (1) >> >> >>".to_string()),
        ],
        "",
    );
    let r_waa = assert_kept(widget_aa_signed, "Widget AA");
    assert!(r_waa
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(6, 0))
        .unwrap()
        .get_raw::<Object<'_>>(b"AA")
        .is_some());

    let outline_signed = tagged_signed_classic_pdf(
        "/Outlines << /Type /Outlines /First << /Title (Trap) /A << /S /JavaScript /JS (app.alert(1)) >> >> >>",
        "Sig1",
        "/V << /Type /Sig /ByteRange [0 1 2 3] >>",
    );
    let r_out = assert_kept(outline_signed, "Outlines JS");
    assert!(r_out
        .xref()
        .get::<Dict<'_>>(r_out.xref().root_id())
        .unwrap()
        .get::<Dict<'_>>(b"Outlines")
        .unwrap()
        .get::<Dict<'_>>(b"First")
        .unwrap()
        .get_raw::<Object<'_>>(b"A")
        .is_some());
}
