//! Integration tests for original-object PDF studio preservation, phrase markup and interactive
//! `/AcroForm` updates (PD-245).

use crate::pdf_edit_budget::Deadline;
use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::{hand_written_with_xref, picture_bytes, pictures_by_page, read};
use crate::pdf_edit_preserve::decode_pdf_text;
use crate::pdf_edit_session::EditSession;
use crate::pdf_first_page::first_page_png;
use easyoffice_formats::{PdfDocumentModel, PdfPageModel, PdfSerializer};
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Number, ObjRef, ObjectIdentifier};
use hayro_write::pdf_writer::types::{
    ActionType, AnnotationFlags, AnnotationType, CheckBoxState, FieldType,
};
use hayro_write::pdf_writer::{Content, Finish, Name as WName, Pdf, Rect, Ref, Str, TextStr};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use usvg::fontdb::Database;

fn korean_font_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/rhwp/ttfs/opensource/NotoSansKR-Regular.ttf");
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; run `node scripts/checkout-rhwp.mjs`",
            path.display()
        )
    })
}

fn engine_with_korean_font() -> InProcessEngine {
    let mut db = Database::new();
    db.load_font_data(korean_font_bytes());
    InProcessEngine::new(Arc::new(db))
}

/// Builds a rich 3-page PDF containing pictures, safe & hostile links, explicit + `/Dests` +
/// `/Names /Dests` destinations, existing `/Highlight` & `/Text` annotations, hierarchical Korean
/// `/Outlines` (including a hostile JS item) and split/merged `/AcroForm` fields.
fn rich_contract_pdf() -> Vec<u8> {
    let mut next = Ref::new(1);
    let (catalog_ref, tree_ref, font_ref, outlines_ref) =
        (next.bump(), next.bump(), next.bump(), next.bump());
    let (dests_dict_ref, names_tree_ref) = (next.bump(), next.bump());
    let page_refs = [next.bump(), next.bump(), next.bump()];
    let mut pdf = Pdf::new();
    pdf.type1_font(font_ref).base_font(WName(b"Helvetica"));

    // Page 1 annotations: safe https, safe mailto, hostile javascript:, hostile file:, hostile
    // Launch, explicit GoTo -> page 3, named /sec2 -> page 2, named (sec3) -> page 3, existing
    // Highlight, existing Text note, and 5 AcroForm widgets (split parent+child applicant, merged
    // Korean dept, doc_no text, approved checkbox, grade choice).
    let link_https = next.bump();
    pdf.annotation(link_https)
        .subtype(AnnotationType::Link)
        .rect(Rect::new(20.0, 240.0, 120.0, 258.0))
        .action()
        .action_type(ActionType::Uri)
        .uri(Str(b"https://example.com/clause-1"));

    let link_mailto = next.bump();
    pdf.annotation(link_mailto)
        .subtype(AnnotationType::Link)
        .rect(Rect::new(20.0, 220.0, 120.0, 238.0))
        .action()
        .action_type(ActionType::Uri)
        .uri(Str(b"mailto:legal@example.com"));

    let link_js_uri = next.bump();
    pdf.annotation(link_js_uri)
        .subtype(AnnotationType::Link)
        .rect(Rect::new(20.0, 200.0, 120.0, 218.0))
        .action()
        .action_type(ActionType::Uri)
        .uri(Str(b"javascript:alert(1)"));

    let link_file_uri = next.bump();
    pdf.annotation(link_file_uri)
        .subtype(AnnotationType::Link)
        .rect(Rect::new(20.0, 180.0, 120.0, 198.0))
        .action()
        .action_type(ActionType::Uri)
        .uri(Str(b"file:///etc/passwd"));

    let link_launch = next.bump();
    {
        let mut a = pdf.annotation(link_launch);
        a.subtype(AnnotationType::Link)
            .rect(Rect::new(20.0, 160.0, 120.0, 178.0));
        a.action()
            .action_type(ActionType::Launch)
            .file_spec()
            .path(Str(b"calc.exe"));
        a.finish();
    }

    let link_goto_p3 = next.bump();
    {
        let mut a = pdf.annotation(link_goto_p3);
        a.subtype(AnnotationType::Link)
            .rect(Rect::new(20.0, 140.0, 120.0, 158.0));
        a.action()
            .action_type(ActionType::GoTo)
            .destination()
            .page(page_refs[2])
            .xyz(20.0, 260.0, None);
        a.finish();
    }

    let link_named_sec2 = next.bump();
    {
        let mut a = pdf.annotation(link_named_sec2);
        a.subtype(AnnotationType::Link)
            .rect(Rect::new(125.0, 240.0, 190.0, 258.0));
        a.pair(WName(b"Dest"), WName(b"sec2"));
        a.finish();
    }

    let link_named_sec3 = next.bump();
    {
        let mut a = pdf.annotation(link_named_sec3);
        a.subtype(AnnotationType::Link)
            .rect(Rect::new(125.0, 220.0, 190.0, 238.0));
        a.pair(WName(b"Dest"), Str(b"sec3"));
        a.finish();
    }

    let existing_hl = next.bump();
    pdf.annotation(existing_hl)
        .subtype(AnnotationType::Highlight)
        .rect(Rect::new(20.0, 96.0, 90.0, 114.0))
        .quad_points([20.0, 114.0, 90.0, 114.0, 20.0, 96.0, 90.0, 96.0])
        .color_rgb(0.0, 1.0, 0.0)
        .flags(AnnotationFlags::PRINT)
        .contents(TextStr("검토 완료"));

    let existing_note = next.bump();
    pdf.annotation(existing_note)
        .subtype(AnnotationType::Text)
        .rect(Rect::new(160.0, 260.0, 180.0, 280.0))
        .contents(TextStr("담당자 메모"))
        .author(TextStr("법무팀"));

    // Split parent+child applicant field + 4 merged fields.
    let (parent_form_ref, app_widget_ref, dept_widget_ref, doc_no_ref, chk_ref, choice_ref) = (
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
    );
    pdf.form_field(parent_form_ref)
        .partial_name(TextStr("form"))
        .children([app_widget_ref]);
    {
        let mut f = pdf.form_field(app_widget_ref);
        f.field_type(FieldType::Text)
            .parent(parent_form_ref)
            .partial_name(TextStr("applicant"))
            .text_value(TextStr("김철수"))
            .vartext_default_appearance(Str(b"/Helv 10 Tf 0 g"));
        f.into_annotation()
            .rect(Rect::new(20.0, 60.0, 140.0, 78.0))
            .flags(AnnotationFlags::PRINT)
            .page(page_refs[0]);
    }
    {
        let mut f = pdf.form_field(dept_widget_ref);
        f.field_type(FieldType::Text)
            .partial_name(TextStr("부서"))
            .text_value(TextStr("법무팀"))
            .vartext_default_appearance(Str(b"/Helv 10 Tf 0 g"));
        f.into_annotation()
            .rect(Rect::new(20.0, 38.0, 140.0, 56.0))
            .flags(AnnotationFlags::PRINT)
            .page(page_refs[0]);
    }
    {
        let mut f = pdf.form_field(doc_no_ref);
        f.field_type(FieldType::Text)
            .partial_name(TextStr("doc_no"))
            .text_value(TextStr("2026-042"))
            .vartext_default_appearance(Str(b"/Helv 10 Tf 0 g"));
        f.into_annotation()
            .rect(Rect::new(20.0, 16.0, 110.0, 34.0))
            .flags(AnnotationFlags::PRINT)
            .page(page_refs[0]);
    }
    {
        let mut f = pdf.form_field(chk_ref);
        f.field_type(FieldType::Button)
            .partial_name(TextStr("approved"))
            .checkbox_value(CheckBoxState::Yes);
        f.into_annotation()
            .rect(Rect::new(145.0, 60.0, 160.0, 75.0))
            .flags(AnnotationFlags::PRINT)
            .page(page_refs[0]);
    }
    {
        let mut f = pdf.form_field(choice_ref);
        f.field_type(FieldType::Choice)
            .partial_name(TextStr("grade"))
            .choice_value(Some(TextStr("대외비")))
            .vartext_default_appearance(Str(b"/Helv 10 Tf 0 g"));
        f.choice_options()
            .options([TextStr("일반"), TextStr("대외비")]);
        f.into_annotation()
            .rect(Rect::new(120.0, 16.0, 185.0, 34.0))
            .flags(AnnotationFlags::PRINT)
            .page(page_refs[0]);
    }

    let p1_annots = [
        link_https,
        link_mailto,
        link_js_uri,
        link_file_uri,
        link_launch,
        link_goto_p3,
        link_named_sec2,
        link_named_sec3,
        existing_hl,
        existing_note,
        app_widget_ref,
        dept_widget_ref,
        doc_no_ref,
        chk_ref,
        choice_ref,
    ];

    for (idx, &page_ref) in page_refs.iter().enumerate() {
        let n = idx + 1;
        let (img_ref, content_ref) = (next.bump(), next.bump());
        let pic = picture_bytes(1, n);
        let mut img = pdf.image_xobject(img_ref, &pic);
        img.width(2).height(2).bits_per_component(8);
        img.color_space().device_rgb();
        img.finish();

        let mut content = Content::new();
        content
            .begin_text()
            .set_font(WName(b"F1"), 14.0)
            .next_line(20.0, 100.0)
            .show(Str(format!("Contract Page {n} Clause").as_bytes()))
            .end_text()
            .save_state()
            .transform([60.0, 0.0, 0.0, 60.0, 120.0, 170.0])
            .x_object(WName(b"Im1"))
            .restore_state();
        pdf.stream(content_ref, &content.finish());

        let mut page = pdf.page(page_ref);
        page.media_box(Rect::new(0.0, 0.0, 200.0, 300.0))
            .parent(tree_ref)
            .contents(content_ref);
        if idx == 0 {
            page.annotations(p1_annots);
        }
        let mut res = page.resources();
        res.x_objects().pair(WName(b"Im1"), img_ref);
        res.fonts().pair(WName(b"F1"), font_ref);
        res.finish();
        page.finish();
    }

    // Named destinations: `/Dests << /sec2 [p2 /FitH 250] >>` and `/Names /Dests << /Names [(sec3) [p3 /Fit]] >>`.
    {
        let mut dests = pdf.destinations(dests_dict_ref);
        dests
            .insert(WName(b"sec2"))
            .page(page_refs[1])
            .fit_horizontal(250.0);
        dests.finish();
    }
    {
        let sec3_arr_ref = next.bump();
        let mut arr = pdf.indirect(sec3_arr_ref).array();
        arr.item(page_refs[2]);
        arr.item(WName(b"Fit"));
        arr.finish();
        let mut tree = pdf.indirect(names_tree_ref).dict();
        let mut names_arr = tree.insert(WName(b"Names")).array();
        names_arr.item(Str(b"sec3"));
        names_arr.item(sec3_arr_ref);
        names_arr.finish();
        tree.finish();
    }

    // Outlines: "1장 총칙" (with child "1.1 정의" -> page 2 and hostile "악성 스크립트" -> JS) + "2장 부칙" -> page 3.
    let (ch1_ref, sub11_ref, evil_js_ref, ch2_ref) =
        (next.bump(), next.bump(), next.bump(), next.bump());
    pdf.outline(outlines_ref)
        .first(ch1_ref)
        .last(ch2_ref)
        .count(3);
    {
        let mut o = pdf.outline_item(ch1_ref);
        o.title(TextStr("1장 총칙"))
            .parent(outlines_ref)
            .next(ch2_ref)
            .first(sub11_ref)
            .last(evil_js_ref)
            .count(2);
        o.dest().page(page_refs[0]).xyz(0.0, 300.0, None);
    }
    {
        let mut o = pdf.outline_item(sub11_ref);
        o.title(TextStr("1.1 정의"))
            .parent(ch1_ref)
            .next(evil_js_ref);
        o.dest().page(page_refs[1]).fit_horizontal(250.0);
    }
    {
        let mut o = pdf.outline_item(evil_js_ref);
        o.title(TextStr("악성 스크립트"))
            .parent(ch1_ref)
            .prev(sub11_ref);
        o.insert(WName(b"A"))
            .start::<hayro_write::pdf_writer::writers::Action<'_>>()
            .action_type(ActionType::JavaScript)
            .js_string(TextStr("app.alert(1)"));
    }
    {
        let mut o = pdf.outline_item(ch2_ref);
        o.title(TextStr("2장 부칙"))
            .parent(outlines_ref)
            .prev(ch1_ref);
        o.dest().page(page_refs[2]).fit();
    }

    pdf.pages(tree_ref).count(3).kids(page_refs);
    {
        let mut cat = pdf.catalog(catalog_ref);
        cat.pages(tree_ref)
            .outlines(outlines_ref)
            .destinations(dests_dict_ref);
        cat.names().pair(WName(b"Dests"), names_tree_ref);
        cat.form().fields([
            parent_form_ref,
            dept_widget_ref,
            doc_no_ref,
            chk_ref,
            choice_ref,
        ]);
        cat.finish();
    }
    pdf.finish()
}

/// Second 1-page PDF with its own picture and outline bookmark `"부록 A"`.
fn appendix_pdf() -> Vec<u8> {
    let mut next = Ref::new(1);
    let (catalog_ref, tree_ref, font_ref, page_ref, img_ref, content_ref, outlines_ref, item_ref) = (
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
    );
    let mut pdf = Pdf::new();
    pdf.type1_font(font_ref).base_font(WName(b"Helvetica"));
    let pic = picture_bytes(2, 1);
    let mut img = pdf.image_xobject(img_ref, &pic);
    img.width(2).height(2).bits_per_component(8);
    img.color_space().device_rgb();
    img.finish();

    let mut content = Content::new();
    content
        .begin_text()
        .set_font(WName(b"F1"), 14.0)
        .next_line(20.0, 120.0)
        .show(Str(b"Appendix Page 2 Summary"))
        .end_text()
        .save_state()
        .transform([50.0, 0.0, 0.0, 50.0, 100.0, 180.0])
        .x_object(WName(b"Im1"))
        .restore_state();
    pdf.stream(content_ref, &content.finish());

    let mut page = pdf.page(page_ref);
    page.media_box(Rect::new(0.0, 0.0, 200.0, 300.0))
        .parent(tree_ref)
        .contents(content_ref);
    let mut res = page.resources();
    res.x_objects().pair(WName(b"Im1"), img_ref);
    res.fonts().pair(WName(b"F1"), font_ref);
    res.finish();
    page.finish();

    pdf.outline(outlines_ref)
        .first(item_ref)
        .last(item_ref)
        .count(1);
    {
        let mut o = pdf.outline_item(item_ref);
        o.title(TextStr("부록 A")).parent(outlines_ref);
        o.dest().page(page_ref).fit();
    }
    pdf.pages(tree_ref).count(1).kids([page_ref]);
    pdf.catalog(catalog_ref)
        .pages(tree_ref)
        .outlines(outlines_ref);
    pdf.finish()
}

fn collect_outline_titles_and_pages(bytes: &[u8]) -> Vec<(String, Option<usize>)> {
    let pdf = read(bytes);
    let page_ids: HashMap<ObjectIdentifier, usize> = pdf
        .pages()
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.raw().obj_id().map(|id| (id, i)))
        .collect();
    let xref = pdf.xref();
    let Some(cat) = xref.get::<Dict<'_>>(xref.root_id()) else {
        return Vec::new();
    };
    let Some(outlines) = cat.get::<Dict<'_>>(b"Outlines") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk_outline_items(outlines.get::<Dict<'_>>(b"First"), &page_ids, &mut out);
    out
}

fn walk_outline_items(
    mut cur: Option<Dict<'_>>,
    page_ids: &HashMap<ObjectIdentifier, usize>,
    out: &mut Vec<(String, Option<usize>)>,
) {
    while let Some(item) = cur {
        let title = item
            .get::<hayro_syntax::object::String<'_>>(b"Title")
            .map(|s| decode_pdf_text(s.as_bytes(), false, 256))
            .unwrap_or_default();
        let target_page =
            item.get::<Array<'_>>(b"Dest")
                .and_then(|arr| match arr.raw_iter().next()? {
                    MaybeRef::Ref(ObjRef {
                        obj_number,
                        gen_number,
                    }) => page_ids
                        .get(&ObjectIdentifier::new(obj_number, gen_number))
                        .copied(),
                    _ => None,
                });
        out.push((title, target_page));
        walk_outline_items(item.get::<Dict<'_>>(b"First"), page_ids, out);
        cur = item.get::<Dict<'_>>(b"Next");
    }
}

fn collect_acroform_fields(bytes: &[u8]) -> HashMap<String, String> {
    let pdf = read(bytes);
    let xref = pdf.xref();
    let Some(cat) = xref.get::<Dict<'_>>(xref.root_id()) else {
        return HashMap::new();
    };
    let Some(acro) = cat.get::<Dict<'_>>(b"AcroForm") else {
        return HashMap::new();
    };
    let Some(fields) = acro.get::<Array<'_>>(b"Fields") else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for field in fields.iter::<Dict<'_>>() {
        let name = field
            .get::<hayro_syntax::object::String<'_>>(b"T")
            .map(|s| decode_pdf_text(s.as_bytes(), false, 256))
            .unwrap_or_default();
        let val = if let Some(s) = field.get::<hayro_syntax::object::String<'_>>(b"V") {
            decode_pdf_text(s.as_bytes(), true, 512)
        } else if let Some(n) = field.get::<Name<'_>>(b"V") {
            format!("/{}", n.as_str())
        } else {
            String::new()
        };
        map.insert(name, val);
    }
    map
}

#[test]
fn preserves_links_outlines_markup_forms_and_pictures_across_rotate_merge_and_split() {
    let eng = engine_with_korean_font();
    let mut session = EditSession::open(rich_contract_pdf(), &eng).expect("open contract");
    assert_eq!(session.merge(vec![appendix_pdf()], &eng), Ok(1));
    session.set_turn(90).unwrap();
    session
        .set_overlay_marks(Some("대외비"), Some("승인필"))
        .unwrap();
    session
        .set_markup("Page 2", Some("underline"), Some("#FF0000"))
        .unwrap();
    session.set_form(Some("홍길동"), Some("기획실")).unwrap();

    let merged_bytes = session.render(None, &eng).expect("render merged").bytes;
    assert_eq!(
        pictures_by_page(&merged_bytes),
        vec![
            picture_bytes(1, 1).to_vec(),
            picture_bytes(1, 2).to_vec(),
            picture_bytes(1, 3).to_vec(),
            picture_bytes(2, 1).to_vec(),
        ]
    );

    let pdf = read(&merged_bytes);
    assert_eq!(pdf.pages().len(), 4);
    let page_ids: HashMap<ObjectIdentifier, usize> = pdf
        .pages()
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.raw().obj_id().map(|id| (id, i)))
        .collect();

    // Verify page 1 links: safe URIs and resolved destinations survive; javascript:, file: and Launch are gone.
    let p0_annots = pdf.pages()[0]
        .raw()
        .get::<Array<'_>>(b"Annots")
        .expect("page 1 annots");
    let mut uris = Vec::new();
    let mut dest_pages = Vec::new();
    let mut subtypes = Vec::new();
    for annot in p0_annots.iter::<Dict<'_>>() {
        let sub = annot
            .get::<Name<'_>>(b"Subtype")
            .map(|n| n.as_str().to_string())
            .unwrap_or_default();
        subtypes.push(sub.clone());
        if let Some(act) = annot.get::<Dict<'_>>(b"A") {
            let s_kind = act
                .get::<Name<'_>>(b"S")
                .map(|n| n.as_str().to_string())
                .unwrap_or_default();
            assert!(
                s_kind != "JavaScript" && s_kind != "Launch",
                "hostile action leaked: {s_kind}"
            );
            if let Some(u) = act.get::<hayro_syntax::object::String<'_>>(b"URI") {
                uris.push(decode_pdf_text(u.as_bytes(), false, 512));
            }
        }
        if let Some(dest) = annot.get::<Array<'_>>(b"Dest") {
            if let Some(MaybeRef::Ref(ObjRef {
                obj_number,
                gen_number,
            })) = dest.raw_iter().next()
            {
                dest_pages.push(page_ids[&ObjectIdentifier::new(obj_number, gen_number)]);
            }
        }
    }
    assert_eq!(
        uris,
        vec![
            "https://example.com/clause-1".to_string(),
            "mailto:legal@example.com".to_string(),
        ]
    );
    // Explicit GoTo -> page 3 (idx 2), named /sec2 -> page 2 (idx 1), named (sec3) -> page 3 (idx 2).
    assert_eq!(dest_pages, vec![2, 1, 2]);
    assert!(subtypes.contains(&"Highlight".to_string()));
    assert!(subtypes.contains(&"Text".to_string()));

    // Verify "Page 2" underline annotation was added on page 2 (idx 1) AND page 4 ("Appendix Page 2 Summary").
    for page_idx in [1usize, 3usize] {
        let annots = pdf.pages()[page_idx]
            .raw()
            .get::<Array<'_>>(b"Annots")
            .expect("underline annots");
        let ul = annots
            .iter::<Dict<'_>>()
            .find(|d| d.get::<Name<'_>>(b"Subtype").as_ref().map(Name::as_str) == Some("Underline"))
            .expect("Underline present");
        let quads: Vec<f32> = ul
            .get::<Array<'_>>(b"QuadPoints")
            .unwrap()
            .iter::<Number>()
            .map(|n| n.as_f32())
            .collect();
        assert_eq!(quads.len(), 8);
        assert!(quads[0] > 20.0 && quads[2] > quads[0], "{quads:?}");
        assert!(ul.get::<Dict<'_>>(b"AP").is_some());
    }

    // Verify outlines: hostile JS child dropped, valid items mapped to exact output page indexes.
    assert_eq!(
        collect_outline_titles_and_pages(&merged_bytes),
        vec![
            ("1장 총칙".to_string(), Some(0)),
            ("1.1 정의".to_string(), Some(1)),
            ("2장 부칙".to_string(), Some(2)),
            ("부록 A".to_string(), Some(3)),
        ]
    );

    // Verify AcroForm fields: applicant & 부서 updated, doc_no / approved / grade preserved.
    let fields = collect_acroform_fields(&merged_bytes);
    assert_eq!(
        fields.get("form_applicant").map(String::as_str),
        Some("홍길동")
    );
    assert_eq!(fields.get("부서").map(String::as_str), Some("기획실"));
    assert_eq!(fields.get("doc_no").map(String::as_str), Some("2026-042"));
    assert_eq!(fields.get("approved").map(String::as_str), Some("/Yes"));
    assert_eq!(fields.get("grade").map(String::as_str), Some("대외비"));

    // Split pages 2-3 (0-based 1..3): "1장 총칙" survives as a container (no Dest) holding "1.1 정의" -> 0,
    // "2장 부칙" -> 1, and "부록 A" (page 4) is pruned.
    let split_bytes = session.render(Some(1..3), &eng).expect("split 2-3").bytes;
    assert_eq!(
        collect_outline_titles_and_pages(&split_bytes),
        vec![
            ("1장 총칙".to_string(), None),
            ("1.1 정의".to_string(), Some(0)),
            ("2장 부칙".to_string(), Some(1)),
        ]
    );
}

#[test]
fn korean_pdf_phrase_highlight_and_new_form_widgets_round_trip_twice() {
    let eng = engine_with_korean_font();
    let model = PdfDocumentModel {
        title: "표준 용역 계약서".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "제1조 목적 본 계약은 용역 수행 조건을 정한다.\n제2조 효력 서명일로부터 효력이 발생한다."
                .to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    };
    let source_bytes = PdfSerializer::serialize(&model);

    // Pass 1: highlight "제1조 목적" and add new studio form fields on a PDF that had no AcroForm.
    let mut s1 = EditSession::open(source_bytes, &eng).expect("open Korean PDF");
    s1.set_markup("제1조 목적", Some("highlight"), Some("#FFEB3B"))
        .unwrap();
    s1.set_form(Some("이순신"), Some("전략기획본부")).unwrap();
    let saved1 = s1.render(None, &eng).expect("save pass 1").bytes;

    let pdf1 = read(&saved1);
    let annots1: Vec<Dict<'_>> = pdf1.pages()[0]
        .raw()
        .get::<Array<'_>>(b"Annots")
        .expect("annots pass 1")
        .iter::<Dict<'_>>()
        .collect();
    let highlights1: Vec<&Dict<'_>> = annots1
        .iter()
        .filter(|d| d.get::<Name<'_>>(b"Subtype").as_ref().map(Name::as_str) == Some("Highlight"))
        .collect();
    assert_eq!(
        highlights1.len(),
        1,
        "hidden 1pt canonical text slot at (0,0) must not produce a second highlight"
    );
    let q1: Vec<f32> = highlights1[0]
        .get::<Array<'_>>(b"QuadPoints")
        .unwrap()
        .iter::<Number>()
        .map(|n| n.as_f32())
        .collect();
    assert!(q1[0] >= 50.0 && q1[1] > 740.0, "body baseline quad: {q1:?}");

    let fields1 = collect_acroform_fields(&saved1);
    assert_eq!(fields1.len(), 2);
    assert_eq!(fields1.get("applicant").map(String::as_str), Some("이순신"));
    assert_eq!(
        fields1.get("department").map(String::as_str),
        Some("전략기획본부")
    );

    // Card preview renders the saved PDF (including `/AP /N` widgets and `/Highlight`) cleanly.
    let png = first_page_png(
        &Arc::new(saved1.clone()),
        Some(korean_font_bytes()),
        &Deadline::after(Duration::from_secs(20)),
    )
    .expect("preview PNG");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

    // Pass 2: re-open the saved PDF, add a second highlight "제2조 효력", and update applicant in place.
    let mut s2 = EditSession::open(saved1, &eng).expect("re-open pass 1");
    s2.set_markup("제2조 효력", Some("highlight"), Some("#90CAF9"))
        .unwrap();
    s2.set_form(Some("강감찬"), None).unwrap();
    let saved2 = s2.render(None, &eng).expect("save pass 2").bytes;

    let pdf2 = read(&saved2);
    let hl_count2 = pdf2.pages()[0]
        .raw()
        .get::<Array<'_>>(b"Annots")
        .unwrap()
        .iter::<Dict<'_>>()
        .filter(|d| d.get::<Name<'_>>(b"Subtype").as_ref().map(Name::as_str) == Some("Highlight"))
        .count();
    assert_eq!(hl_count2, 2, "both pass-1 and pass-2 highlights exist");

    let fields2 = collect_acroform_fields(&saved2);
    assert_eq!(fields2.len(), 2, "widgets updated in place, not duplicated");
    assert_eq!(fields2.get("applicant").map(String::as_str), Some("강감찬"));
    assert_eq!(
        fields2.get("department").map(String::as_str),
        Some("전략기획본부")
    );
}

#[test]
fn cyclic_outlines_name_trees_and_field_parents_fail_closed() {
    let eng = engine_with_korean_font();

    // 1. Cyclic `/Outlines` sibling `/Next` loop.
    let cyclic_outlines = hand_written_with_xref(
        &[
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
            ),
            (
                4,
                "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 1 >>".to_string(),
            ),
            (
                5,
                "<< /Title (Loop) /Parent 4 0 R /Next 5 0 R /Dest [3 0 R /Fit] >>".to_string(),
            ),
        ],
        "",
    );
    let s_out = EditSession::open(cyclic_outlines, &eng).unwrap();
    let err_out = s_out.render(None, &eng).unwrap_err();
    assert!(err_out.contains("순환 참조"), "{err_out}");

    // 1b. Two-node `/Outlines` sibling cycle (5 -> 6 -> 5).
    let two_node_outlines = hand_written_with_xref(
        &[
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
            ),
            (
                4,
                "<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>".to_string(),
            ),
            (
                5,
                "<< /Title (A) /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>".to_string(),
            ),
            (
                6,
                "<< /Title (B) /Parent 4 0 R /Next 5 0 R /Dest [3 0 R /Fit] >>".to_string(),
            ),
        ],
        "",
    );
    let err_two = EditSession::open(two_node_outlines, &eng)
        .unwrap()
        .render(None, &eng)
        .unwrap_err();
    assert!(err_two.contains("순환 참조"), "{err_two}");

    // 2. Cyclic `/Names /Dests` tree `/Kids` loop.
    let cyclic_names = hand_written_with_xref(
        &[
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 4 0 R >> >>".to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
            ),
            (4, "<< /Kids [4 0 R] >>".to_string()),
        ],
        "",
    );
    let s_names = EditSession::open(cyclic_names, &eng).unwrap();
    let err_names = s_names.render(None, &eng).unwrap_err();
    assert!(err_names.contains("순환 참조"), "{err_names}");

    // 3. Cyclic `/Widget /Parent` loop.
    let cyclic_field = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots [4 0 R] >>"
                    .to_string(),
            ),
            (
                4,
                "<< /Type /Annot /Subtype /Widget /FT /Tx /T (x) /Rect [0 0 50 20] /Parent 4 0 R >>"
                    .to_string(),
            ),
        ],
        "",
    );
    let s_field = EditSession::open(cyclic_field, &eng).unwrap();
    let err_field = s_field.render(None, &eng).unwrap_err();
    assert!(err_field.contains("순환 참조"), "{err_field}");

    // 4. Input validation budgets on markup & form ops.
    let mut valid = EditSession::open(appendix_pdf(), &eng).unwrap();
    assert!(valid.set_markup(&"가".repeat(201), None, None).is_err());
    assert!(valid.set_markup("제1조\n제2조", None, None).is_err());
    assert!(valid.set_markup("제1조", Some("wavy"), None).is_err());
    assert!(valid.set_markup("제1조", None, Some("not-hex")).is_err());
    assert!(valid.set_form(Some(&"홍".repeat(201)), None).is_err());
    assert!(valid.set_form(None, Some("기획\0팀")).is_err());
}

#[test]
fn bench_pd245_pdf_preserve_incremental_save() {
    let eng = engine_with_korean_font();
    let source = Arc::new(rich_contract_pdf());
    let mut session = EditSession::open((*source).clone(), &eng).unwrap();
    session
        .set_markup("Contract Page", Some("highlight"), Some("#FFFF00"))
        .unwrap();
    session.set_form(Some("홍길동"), Some("기획실")).unwrap();

    let warm = session.render(None, &eng).expect("warmup").bytes;
    assert!(!warm.is_empty());

    let iterations = 10u32;
    let start = Instant::now();
    let mut out_len = 0usize;
    for _ in 0..iterations {
        out_len = session.render(None, &eng).unwrap().bytes.len();
    }
    let avg = start.elapsed() / iterations;
    eprintln!(
        "[PD-245 bench] 3-page rich PDF preserve+highlight+form incremental build: avg={avg:?} ({} B)",
        out_len
    );
    assert!(avg < Duration::from_secs(2), "too slow: {avg:?}");
}
