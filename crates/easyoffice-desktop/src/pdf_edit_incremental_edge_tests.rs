//! Edge-case, boundary and security regression tests for incremental PDF save (PD-349).

use crate::pdf_edit_build::build;
use crate::pdf_edit_fixtures::deadline;
use crate::pdf_edit_incremental::try_incremental_build;
use crate::pdf_edit_incremental::IncrementalOutcome;
use crate::pdf_edit_incremental_xref::{inspect_xref, XRefFormat, XRefInspectResult};
use crate::pdf_edit_inspect::open_pdf;
use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_wire::{OverlayKind, PagePlan, WireForm, MAX_WARNINGS};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hayro_syntax::object::{Array, Dict, Name, ObjectIdentifier};
use std::sync::Arc;

fn edge_make_overlay(w: f32, h: f32, _kind: OverlayKind) -> Result<Vec<u8>, String> {
    let mut pdf = hayro_write::pdf_writer::Pdf::new();
    let mut next = hayro_write::pdf_writer::Ref::new(1);
    let (catalog, tree, page_ref, content_ref) =
        (next.bump(), next.bump(), next.bump(), next.bump());
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).count(1).kids([page_ref]);
    let mut content = hayro_write::pdf_writer::Content::new();
    content
        .rect(1.0, 1.0, (w - 2.0).max(1.0), (h - 2.0).max(1.0))
        .stroke();
    pdf.stream(content_ref, &content.finish());
    pdf.page(page_ref)
        .media_box(hayro_write::pdf_writer::Rect::new(
            0.0,
            0.0,
            w.max(1.0),
            h.max(1.0),
        ))
        .parent(tree)
        .contents(content_ref);
    Ok(pdf.finish())
}

fn build_classic_with_entries(objs: Vec<(u32, String)>, root_id: u32) -> Vec<u8> {
    let mut buf = Vec::from(&b"%PDF-1.4\n"[..]);
    let mut map = std::collections::BTreeMap::new();
    for (id, body) in &objs {
        map.insert(*id, buf.len());
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    let size = objs.iter().map(|(id, _)| *id).max().unwrap_or(0) + 1;
    let xref_off = buf.len();
    buf.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for id in 1..size {
        if let Some(off) = map.get(&id) {
            buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        } else {
            buf.extend_from_slice(b"0000000000 00000 f \n");
        }
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root {root_id} 0 R >>\nstartxref\n{xref_off}\n%%EOF\n")
            .as_bytes(),
    );
    buf
}

fn append_classic_catalog_update(
    buf: &mut Vec<u8>,
    prev: usize,
    size: u32,
    root_body: &str,
) -> usize {
    let root_off = buf.len();
    buf.extend_from_slice(format!("1 0 obj\n{root_body}\nendobj\n").as_bytes());
    let xref_off = buf.len();
    buf.extend_from_slice(
        format!(
            "xref\n1 1\n{root_off:010} 00000 n \n\
             trailer\n<< /Size {size} /Root 1 0 R /Prev {prev} >>\nstartxref\n{xref_off}\n%%EOF\n"
        )
        .as_bytes(),
    );
    xref_off
}

fn build_xref_stream_pdf(
    objs: Vec<(u32, String)>,
    root_id: u32,
    w: [usize; 3],
    declared_index: Option<&str>,
    compress: bool,
    png_sub_predictor: bool,
) -> Vec<u8> {
    let mut buf = Vec::from(&b"%PDF-1.5\n"[..]);
    let mut map = std::collections::BTreeMap::new();
    for (id, body) in &objs {
        map.insert(*id, buf.len());
        buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    let xref_id = objs.iter().map(|(id, _)| *id).max().unwrap_or(0) + 1;
    let size = xref_id + 1;
    let xref_off = buf.len();
    map.insert(xref_id, xref_off);

    let encode_field = |val: u64, width: usize, out: &mut Vec<u8>| {
        for shift in (0..width).rev() {
            out.push(((val >> (shift * 8)) & 0xff) as u8);
        }
    };
    let stride = w[0] + w[1] + w[2];
    let mut raw_rows = Vec::new();
    for id in 0..size {
        let mut row = Vec::with_capacity(stride);
        if let Some(&off) = map.get(&id) {
            encode_field(1, w[0], &mut row);
            encode_field(off as u64, w[1], &mut row);
            encode_field(0, w[2], &mut row);
        } else {
            encode_field(0, w[0], &mut row);
            encode_field(0, w[1], &mut row);
            encode_field(65535, w[2], &mut row);
        }
        if png_sub_predictor {
            out_row_with_tag(&mut raw_rows, &row);
        } else {
            raw_rows.extend_from_slice(&row);
        }
    }

    let payload = if compress {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut enc, &raw_rows).unwrap();
        enc.finish().unwrap()
    } else {
        raw_rows
    };

    let idx_str = declared_index.map_or_else(String::new, |s| format!("/Index [{s}] "));
    let filter_str = if compress { "/F /FlateDecode " } else { "" };
    let dp_str = if png_sub_predictor {
        format!("/DP << /Predictor 12 /Columns {stride} /Colors 1 /BitsPerComponent 8 >> ")
    } else {
        String::new()
    };

    buf.extend_from_slice(
        format!(
            "{xref_id} 0 obj\n<< /Type /XRef /Size {size} /Root {root_id} 0 R \
             /W [{} {} {}] {idx_str}{filter_str}{dp_str}/Length {} >>\nstream\n",
            w[0],
            w[1],
            w[2],
            payload.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(&payload);
    buf.extend_from_slice(
        format!("\nendstream\nendobj\nstartxref\n{xref_off}\n%%EOF\n").as_bytes(),
    );
    buf
}

fn out_row_with_tag(out: &mut Vec<u8>, row: &[u8]) {
    out.push(0);
    out.extend_from_slice(row);
}

fn minimal_tagged_objs() -> Vec<(u32, String)> {
    let stream = b"BT /F1 12 Tf 50 50 Td (Test) Tj ET";
    vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
        ),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 6 0 R >> >> /Contents 4 0 R >>"
                .to_string(),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                std::str::from_utf8(stream).unwrap()
            ),
        ),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (
            6,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
    ]
}

#[test]
fn prev_chain_depth_127_ready_and_128_fallback() {
    let base = build_classic_with_entries(minimal_tagged_objs(), 1);
    let pdf = open_pdf(&Arc::new(base.clone())).unwrap();
    let ready = inspect_xref(&base, &pdf).unwrap();
    let XRefInspectResult::Ready(meta) = ready else {
        panic!("expected Ready");
    };
    let mut prev = meta.startxref as usize;
    let mut chained = base;
    for _ in 1..127 {
        prev = append_classic_catalog_update(
            &mut chained,
            prev,
            7,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>",
        );
    }
    let pdf127 = open_pdf(&Arc::new(chained.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&chained, &pdf127).unwrap(),
        XRefInspectResult::Ready(_)
    ));

    // Appending the 128th existing section crosses MAX_EXISTING_XREF_CHAIN_DEPTH (=127).
    append_classic_catalog_update(
        &mut chained,
        prev,
        7,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>",
    );
    let pdf128 = open_pdf(&Arc::new(chained.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&chained, &pdf128).unwrap(),
        XRefInspectResult::Fallback(_)
    ));
}

#[test]
fn xref_stream_w_variants_dp_alias_and_hostile_index() {
    // Valid /W [1 8 2] with /F /FlateDecode and /DP PNG Predictor 12
    let valid_w182 = build_xref_stream_pdf(minimal_tagged_objs(), 1, [1, 8, 2], None, true, true);
    let pdf = open_pdf(&Arc::new(valid_w182.clone())).unwrap();
    let res = inspect_xref(&valid_w182, &pdf).unwrap();
    assert!(matches!(
        res,
        XRefInspectResult::Ready(m) if m.format == XRefFormat::Stream
    ));

    // Unsupported /W [2 4 2] (w0 > 1), /W [1 5 2] (w1 = 5), /W [1 0 2] (w1 = 0)
    for bad_w in [[2, 4, 2], [1, 5, 2], [1, 0, 2]] {
        let bad = build_xref_stream_pdf(minimal_tagged_objs(), 1, bad_w, None, false, false);
        if let Ok(p) = open_pdf(&Arc::new(bad.clone())) {
            assert!(
                matches!(
                    inspect_xref(&bad, &p).unwrap(),
                    XRefInspectResult::Fallback(_)
                ),
                "bad /W {bad_w:?} must fallback"
            );
        }
    }

    // Wrong /DP Columns must fallback (binary-safe same-length patch)
    let mut bad_dp = valid_w182.clone();
    let needle = b"/Columns 11";
    let pos = bad_dp
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("/Columns 11 present");
    bad_dp[pos..pos + needle.len()].copy_from_slice(b"/Columns 05");
    if let Ok(p) = open_pdf(&Arc::new(bad_dp.clone())) {
        assert!(matches!(
            inspect_xref(&bad_dp, &p).unwrap(),
            XRefInspectResult::Fallback(_)
        ));
    }

    // Hostile /Index exceeding /Size must fallback cleanly without OOM or hang
    let hostile_idx = build_xref_stream_pdf(
        minimal_tagged_objs(),
        1,
        [1, 4, 2],
        Some("0 1000000000"),
        false,
        false,
    );
    if let Ok(p) = open_pdf(&Arc::new(hostile_idx.clone())) {
        assert!(matches!(
            inspect_xref(&hostile_idx, &p).unwrap(),
            XRefInspectResult::Fallback(_)
        ));
    }
}

#[test]
fn classic_non20_entry_and_repaired_offset_fallback() {
    let base = build_classic_with_entries(minimal_tagged_objs(), 1);
    let base_str = String::from_utf8_lossy(&base);

    // 19-byte entry (`n\n` instead of `n \n`) must fallback instead of silently repairing.
    let short_entry = base_str.replace("00000 n \n", "00000 n\n");
    let short_bytes = short_entry.into_bytes();
    if let Ok(p) = open_pdf(&Arc::new(short_bytes.clone())) {
        assert!(matches!(
            inspect_xref(&short_bytes, &p).unwrap(),
            XRefInspectResult::Fallback(_)
        ));
    }

    // 21-byte entry (`n \r\n`) must fallback.
    let long_entry = base_str.replace("00000 n \n", "00000 n \r\n");
    let long_bytes = long_entry.into_bytes();
    if let Ok(p) = open_pdf(&Arc::new(long_bytes.clone())) {
        assert!(matches!(
            inspect_xref(&long_bytes, &p).unwrap(),
            XRefInspectResult::Fallback(_)
        ));
    }

    // Corrupted offset for object 1 pointing into `%PDF-1.4` header (repaired by reader) must fallback.
    let marker = "0000000009 00000 n \n";
    let patched = base_str.replacen(marker, "0000000000 00000 n \n", 1);
    let patched_bytes = patched.into_bytes();
    let repaired_pdf = open_pdf(&Arc::new(patched_bytes.clone())).unwrap();
    assert!(matches!(
        inspect_xref(&patched_bytes, &repaired_pdf).unwrap(),
        XRefInspectResult::Fallback(_)
    ));
}

#[test]
fn indirect_acroform_dr_preserves_existing_fonts_and_button_applicant_not_swallowed() {
    let objs = vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm 9 0 R >>".to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Resources << /Font << /F1 6 0 R >> >> /Contents 4 0 R /Annots [7 0 R 8 0 R] >>"
                .to_string(),
        ),
        (4, "<< /Length 25 >>\nstream\nBT /F1 12 Tf (Hi) Tj ET\nendstream".to_string()),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (6, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
        // Pushbutton named `applicant` (non-text field!)
        (
            7,
            "<< /Type /Annot /Subtype /Widget /FT /Btn /T (applicant) /Ff 65536 /Rect [10 10 80 30] /P 3 0 R >>"
                .to_string(),
        ),
        // Signed field with indirect /V using FieldMDP /Action /All
        (
            8,
            "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sig1) /Rect [10 40 80 60] /P 3 0 R /V 13 0 R >>"
                .to_string(),
        ),
        // /AcroForm with indirect /DR and orphan parent not referenced here
        (
            9,
            "<< /Fields [7 0 R 8 0 R] /SigFlags 3 /DR 10 0 R /DA (/Helv 12 Tf 0 g) >>"
                .to_string(),
        ),
        (10, "<< /Font 11 0 R >>".to_string()),
        (11, "<< /ZaDb 12 0 R >>".to_string()),
        (12, "<< /Type /Font /Subtype /Type1 /BaseFont /ZapfDingbats >>".to_string()),
        (
            13,
            "<< /Type /Sig /Filter /TestSigFilter /SubFilter /ETSI.CAdES.detached /ByteRange [0 10 20 10] /Contents <00> /Reference [ << /Type /SigRef /TransformMethod /FieldMDP /TransformParams << /Type /TransformParams /Action /All /V /1.2 >> >> ] >>"
                .to_string(),
        ),
    ];
    let bytes = build_classic_with_entries(objs, 1);
    let overlay = Overlay {
        form: WireForm {
            applicant: Some("강감찬".to_string()),
            department: None,
        },
        ..Default::default()
    };
    let pages = vec![PagePlan {
        source: 0,
        index: 0,
        turn: 0,
    }];
    let built = build(
        &[Arc::new(bytes.clone())],
        &pages,
        &overlay,
        &mut edge_make_overlay,
        &deadline(),
    )
    .unwrap();
    assert_eq!(&built.bytes[..bytes.len()], bytes.as_slice());
    assert!(
        built.warnings.iter().any(|w| w.contains("수정 잠금")),
        "expected FieldMDP Action All lock warning, got: {:?}",
        built.warnings
    );

    let reopened = open_pdf(&Arc::new(built.bytes)).unwrap();
    let cat = reopened
        .xref()
        .get::<Dict<'_>>(reopened.xref().root_id())
        .unwrap();
    let acro = cat.get::<Dict<'_>>(b"AcroForm").unwrap();
    let fonts = acro
        .get::<Dict<'_>>(b"DR")
        .and_then(|dr| dr.get::<Dict<'_>>(b"Font"))
        .unwrap();
    assert!(
        fonts.get::<Dict<'_>>(b"ZaDb").is_some(),
        "ZaDb must be preserved"
    );
    assert!(
        fonts.get::<Dict<'_>>(b"Helv").is_some(),
        "Helv must be added"
    );

    let annots = reopened
        .pages()
        .first()
        .unwrap()
        .raw()
        .get::<Array<'_>>(b"Annots")
        .unwrap();
    assert_eq!(
        annots.raw_iter().count(),
        3,
        "new applicant_2 /Tx widget must be added"
    );
    let has_tx_app2 = annots.iter::<Dict<'_>>().any(|d| {
        d.get::<Name<'_>>(b"FT").as_deref() == Some(b"Tx")
            && d.get::<hayro_syntax::object::String<'_>>(b"T")
                .is_some_and(|s| s.as_bytes() == b"applicant_2")
    });
    assert!(has_tx_app2);
}

#[test]
fn split_parent_child_removes_stale_v_and_added_form_warns_sig() {
    let objs = vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm << /Fields [7 0 R 9 0 R] /SigFlags 3 >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Resources << /Font << /F1 6 0 R >> >> /Contents 4 0 R /Annots [8 0 R 9 0 R] >>"
                .to_string(),
        ),
        (4, "<< /Length 22 >>\nstream\nBT /F1 12 Tf () Tj ET\nendstream".to_string()),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (6, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()),
        (
            7,
            "<< /FT /Tx /T (applicant) /V (OldParent) /Kids [8 0 R] >>".to_string(),
        ),
        (
            8,
            "<< /Type /Annot /Subtype /Widget /Parent 7 0 R /V (StaleChild) /Rect [20 20 120 45] /P 3 0 R >>"
                .to_string(),
        ),
        (
            9,
            "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Approval) /Rect [20 80 120 110] /P 3 0 R /V << /Type /Sig /Filter /TestSigFilter /SubFilter /ETSI.CAdES.detached /ByteRange [0 1 2 3] /Contents <00> >> >>"
                .to_string(),
        ),
    ];
    let bytes = build_classic_with_entries(objs, 1);
    let overlay = Overlay {
        form: WireForm {
            applicant: Some("이순신".to_string()),
            department: Some("개발팀".to_string()),
        },
        ..Default::default()
    };
    let built = build(
        &[Arc::new(bytes.clone())],
        &[PagePlan {
            source: 0,
            index: 0,
            turn: 0,
        }],
        &overlay,
        &mut edge_make_overlay,
        &deadline(),
    )
    .unwrap();
    assert_eq!(&built.bytes[..bytes.len()], bytes.as_slice());
    assert!(
        built.warnings.iter().any(|w| w.contains("달라질 수")),
        "adding department field to signed doc should warn on /AcroForm change: {:?}",
        built.warnings
    );

    let reopened = open_pdf(&Arc::new(built.bytes)).unwrap();
    let parent = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(7, 0))
        .unwrap();
    assert!(parent
        .get::<hayro_syntax::object::String<'_>>(b"V")
        .is_some());
    let child = reopened
        .xref()
        .get::<Dict<'_>>(ObjectIdentifier::new(8, 0))
        .unwrap();
    assert!(
        child
            .get::<hayro_syntax::object::String<'_>>(b"V")
            .is_none(),
        "stale /V on split widget child must be removed"
    );
    assert!(child.get::<Dict<'_>>(b"AP").is_some());
}

#[test]
fn overlay_on_stream_ending_with_comment_and_partial_page_skip() {
    // Stream 4 ends with `% comment` without trailing newline.
    let stream_body = b"BT /F1 12 Tf 40 40 Td (Hi) Tj ET % no newline";
    let mut objs = minimal_tagged_objs();
    objs[3] = (
        4,
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            stream_body.len(),
            String::from_utf8_lossy(stream_body)
        ),
    );
    let bytes = build_classic_with_entries(objs, 1);
    let overlay = Overlay {
        watermark: Some("TEST".to_string()),
        ..Default::default()
    };
    let built = build(
        &[Arc::new(bytes.clone())],
        &[PagePlan {
            source: 0,
            index: 0,
            turn: 0,
        }],
        &overlay,
        &mut edge_make_overlay,
        &deadline(),
    )
    .unwrap();
    assert_eq!(&built.bytes[..bytes.len()], bytes.as_slice());
    let reopened = open_pdf(&Arc::new(built.bytes)).unwrap();
    let page = reopened.pages().first().unwrap();
    let contents = page.page_stream().unwrap();
    let text = String::from_utf8_lossy(contents);
    assert!(
        text.contains("% no newline \nQ\n/Artifact BMC"),
        "post-stream must start with newline before Q: {text:?}"
    );
    assert!(!text.contains("% no newline Q"));

    // 2-page tagged PDF where page 1 has >4096 resource entries; saving page 0 only must bypass incremental and succeed!
    let mut big_fonts = String::from("<< /Font << ");
    for i in 0..4105 {
        big_fonts.push_str(&format!("/F{i} 6 0 R "));
    }
    big_fonts.push_str(">> >>");
    let two_page = build_classic_with_entries(
        vec![
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
            ),
            (
                2,
                "<< /Type /Pages /Kids [3 0 R 7 0 R] /Count 2 >>".to_string(),
            ),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 6 0 R >> >> /Contents 4 0 R >>"
                    .to_string(),
            ),
            (
                4,
                "<< /Length 22 >>\nstream\nBT /F1 12 Tf () Tj ET\nendstream".to_string(),
            ),
            (5, "<< /Type /StructTreeRoot >>".to_string()),
            (
                6,
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            ),
            (
                7,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources {big_fonts} /Contents 4 0 R >>"
                ),
            ),
        ],
        1,
    );
    let partial_out = build(
        &[Arc::new(two_page)],
        &[PagePlan {
            source: 0,
            index: 0,
            turn: 0,
        }],
        &Overlay::default(),
        &mut edge_make_overlay,
        &deadline(),
    )
    .unwrap();
    let partial_reopened = open_pdf(&Arc::new(partial_out.bytes)).unwrap();
    assert_eq!(partial_reopened.pages().len(), 1);
    assert!(partial_out.warnings.len() <= MAX_WARNINGS);

    // Unused helper sanity check: verify NotApplicable on partial selection directly.
    assert!(matches!(
        try_incremental_build(
            &[Arc::new(bytes)],
            &[],
            &Overlay::default(),
            &mut edge_make_overlay,
            &deadline(),
        )
        .unwrap(),
        IncrementalOutcome::NotApplicable
    ));
}

#[test]
fn partial_field_locks_and_doc_mdp_p2_warn_only_when_violated() {
    let build_form_doc = |sig_extra: &str| {
        let objs = vec![
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R \
                 /AcroForm << /Fields [6 0 R 8 0 R 9 0 R] /SigFlags 3 >> >>"
                    .to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] \
                 /Resources << /Font << /F1 10 0 R >> >> /Contents 4 0 R \
                 /Annots [7 0 R 8 0 R 9 0 R] >>"
                    .to_string(),
            ),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (5, "<< /Type /StructTreeRoot >>".to_string()),
            (6, "<< /T (form) /Kids [7 0 R] >>".to_string()),
            (
                7,
                "<< /Type /Annot /Subtype /Widget /FT /Tx /Parent 6 0 R /T (applicant) \
                 /Rect [10 10 120 35] /P 3 0 R >>"
                    .to_string(),
            ),
            (
                8,
                "<< /Type /Annot /Subtype /Widget /FT /Tx /T (department) \
                 /Rect [10 45 120 70] /P 3 0 R >>"
                    .to_string(),
            ),
            (
                9,
                format!(
                    "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sig1) \
                     /Rect [10 90 120 120] /P 3 0 R {sig_extra} >>"
                ),
            ),
            (
                10,
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            ),
        ];
        build_classic_with_entries(objs, 1)
    };
    let run_form = |bytes: Vec<u8>, form: WireForm| {
        build(
            &[Arc::new(bytes)],
            &[PagePlan {
                source: 0,
                index: 0,
                turn: 0,
            }],
            &Overlay {
                form,
                ..Default::default()
            },
            &mut edge_make_overlay,
            &deadline(),
        )
        .unwrap()
    };

    // 1. `/Lock /Action /Include /Fields [(form.applicant)]`
    let inc_lock = build_form_doc(
        "/Lock << /Action /Include /Fields [(form.applicant)] >> \
         /V << /Type /Sig /ByteRange [0 1 2 3] >>",
    );
    let hit_inc = run_form(
        inc_lock.clone(),
        WireForm {
            applicant: Some("홍길동".to_string()),
            department: None,
        },
    );
    assert!(
        hit_inc.warnings.iter().any(|w| w.contains("수정 잠금")),
        "Include lock hit should emit lock warning: {:?}",
        hit_inc.warnings
    );
    let miss_inc = run_form(
        inc_lock,
        WireForm {
            applicant: None,
            department: Some("개발팀".to_string()),
        },
    );
    assert!(
        !miss_inc.warnings.iter().any(|w| w.contains("수정 잠금")),
        "Unlocked field under Include lock must not warn lock: {:?}",
        miss_inc.warnings
    );

    // 2. `/Lock /Action /Exclude /Fields [(department)]`
    let exc_lock = build_form_doc(
        "/Lock << /Action /Exclude /Fields [(department)] >> \
         /V << /Type /Sig /ByteRange [0 1 2 3] >>",
    );
    let hit_exc = run_form(
        exc_lock.clone(),
        WireForm {
            applicant: Some("홍길동".to_string()),
            department: None,
        },
    );
    assert!(
        hit_exc.warnings.iter().any(|w| w.contains("수정 잠금")),
        "Non-excluded field under Exclude lock should warn lock: {:?}",
        hit_exc.warnings
    );
    let miss_exc = run_form(
        exc_lock,
        WireForm {
            applicant: None,
            department: Some("개발팀".to_string()),
        },
    );
    assert!(
        !miss_exc.warnings.iter().any(|w| w.contains("수정 잠금")),
        "Excluded field under Exclude lock must not warn lock: {:?}",
        miss_exc.warnings
    );

    // 3. `/FieldMDP /Action /Include [/applicant]` and `/Exclude [(department)]`
    let fmdp_inc = build_form_doc(
        "/V << /Type /Sig /ByteRange [0 1 2 3] /Reference [<< /TransformMethod /FieldMDP \
         /TransformParams << /Action /Include /Fields [/applicant] >> >>] >>",
    );
    let hit_fmdp_inc = run_form(
        fmdp_inc,
        WireForm {
            applicant: Some("김철수".to_string()),
            department: None,
        },
    );
    assert!(hit_fmdp_inc
        .warnings
        .iter()
        .any(|w| w.contains("수정 잠금")));

    let fmdp_exc = build_form_doc(
        "/V << /Type /Sig /ByteRange [0 1 2 3] /Reference [<< /TransformMethod /FieldMDP \
         /TransformParams << /Action /Exclude /Fields [(department)] >> >>] >>",
    );
    let miss_fmdp_exc = run_form(
        fmdp_exc.clone(),
        WireForm {
            applicant: None,
            department: Some("기획팀".to_string()),
        },
    );
    assert!(!miss_fmdp_exc
        .warnings
        .iter()
        .any(|w| w.contains("수정 잠금")));
    let hit_fmdp_exc = run_form(
        fmdp_exc,
        WireForm {
            applicant: Some("김철수".to_string()),
            department: None,
        },
    );
    assert!(hit_fmdp_exc
        .warnings
        .iter()
        .any(|w| w.contains("수정 잠금")));

    // 4. `/DocMDP /P 2`: editing existing `department` is allowed, adding new `applicant` warns lock.
    let p2_only_dept = build_classic_with_entries(
        vec![
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R \
                 /AcroForm << /Fields [6 0 R 7 0 R] /SigFlags 3 >> >>"
                    .to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] \
                 /Contents 4 0 R /Annots [6 0 R 7 0 R] >>"
                    .to_string(),
            ),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (5, "<< /Type /StructTreeRoot >>".to_string()),
            (
                6,
                "<< /Type /Annot /Subtype /Widget /FT /Tx /T (department) \
                 /Rect [10 45 120 70] /P 3 0 R >>"
                    .to_string(),
            ),
            (
                7,
                "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Cert) \
                 /Rect [10 90 120 120] /P 3 0 R \
                 /V << /Type /Sig /ByteRange [0 1 2 3] /Reference \
                 [<< /TransformMethod /DocMDP /TransformParams << /P 2 >> >>] >> >>"
                    .to_string(),
            ),
        ],
        1,
    );
    let p2_fill_existing = run_form(
        p2_only_dept.clone(),
        WireForm {
            applicant: None,
            department: Some("총무팀".to_string()),
        },
    );
    assert!(
        !p2_fill_existing
            .warnings
            .iter()
            .any(|w| w.contains("수정 잠금")),
        "DocMDP P=2 updating existing field should not emit lock warning: {:?}",
        p2_fill_existing.warnings
    );
    let p2_add_new = run_form(
        p2_only_dept,
        WireForm {
            applicant: Some("이영희".to_string()),
            department: None,
        },
    );
    assert!(
        p2_add_new.warnings.iter().any(|w| w.contains("수정 잠금")),
        "DocMDP P=2 adding new form field must emit lock warning: {:?}",
        p2_add_new.warnings
    );
}

#[test]
fn page_at_max_annots_rejects_adding_markup_or_form_widget() {
    let max_annots = crate::pdf_edit_budget::MAX_PAGE_ANNOTS;
    let stream = b"BT /F1 12 Tf 50 200 Td (Hello) Tj ET";
    let mut annots_refs = String::new();
    let mut objs = vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, String::new()),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                std::str::from_utf8(stream).unwrap()
            ),
        ),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (
            6,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ),
    ];
    for i in 0..max_annots {
        let id = 7 + u32::try_from(i).unwrap();
        annots_refs.push_str(&format!("{id} 0 R "));
        objs.push((
            id,
            "<< /Type /Annot /Subtype /Text /Rect [1 1 5 5] >>".to_string(),
        ));
    }
    objs[2] = (
        3,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] \
             /Resources << /Font << /F1 6 0 R >> >> /Contents 4 0 R /Annots [{annots_refs}] >>"
        ),
    );
    let orig = build_classic_with_entries(objs, 1);

    let markup_err = build(
        &[Arc::new(orig.clone())],
        &[PagePlan {
            source: 0,
            index: 0,
            turn: 0,
        }],
        &Overlay {
            markup: Some(crate::pdf_edit_wire::WireMarkup {
                text: "Hello".to_string(),
                underline: false,
                color: [255, 255, 0],
            }),
            ..Default::default()
        },
        &mut edge_make_overlay,
        &deadline(),
    )
    .unwrap_err();
    assert_eq!(markup_err, crate::pdf_edit_preserve::too_many_page_annots());

    let form_err = build(
        &[Arc::new(orig)],
        &[PagePlan {
            source: 0,
            index: 0,
            turn: 0,
        }],
        &Overlay {
            form: WireForm {
                applicant: Some("홍길동".to_string()),
                department: None,
            },
            ..Default::default()
        },
        &mut edge_make_overlay,
        &deadline(),
    )
    .unwrap_err();
    assert_eq!(form_err, crate::pdf_edit_preserve::too_many_page_annots());
}
