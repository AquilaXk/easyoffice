//! `/NeedAppearances` handling of the incremental PDF save: the flag may only be dropped when no
//! untouched text-like widget with a value still depends on the viewer to draw it.

use crate::pdf_edit_build::build;
use crate::pdf_edit_fixtures::{deadline, hand_written_with_xref};
use crate::pdf_edit_inspect::open_pdf;
use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_wire::{OverlayKind, PagePlan, WireForm};
use hayro_syntax::object::Dict;
use hayro_write::pdf_writer::{Content, Pdf, Rect, Ref};
use std::sync::Arc;

fn blank_overlay(w: f32, h: f32, _kind: OverlayKind) -> Result<Vec<u8>, String> {
    let mut pdf = Pdf::new();
    let mut next = Ref::new(1);
    let (catalog, tree, page_ref, content_ref) =
        (next.bump(), next.bump(), next.bump(), next.bump());
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).count(1).kids([page_ref]);
    pdf.stream(content_ref, &Content::new().finish());
    pdf.page(page_ref)
        .media_box(Rect::new(0.0, 0.0, w.max(1.0), h.max(1.0)))
        .parent(tree)
        .contents(content_ref);
    Ok(pdf.finish())
}

/// A tagged form that relies on `/NeedAppearances`: the `applicant` field is the one the studio
/// saves, `other` is a second widget whose body the test chooses.
fn need_appearances_pdf(other_widget: &str) -> Vec<u8> {
    let objs = vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /MarkInfo << /Marked true >> \
             /AcroForm << /Fields [6 0 R 7 0 R] /NeedAppearances true >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R \
             /Annots [6 0 R 7 0 R] >>"
                .to_string(),
        ),
        (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (
            6,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (applicant) /Rect [10 10 90 40] >>"
                .to_string(),
        ),
        (7, other_widget.to_string()),
    ];
    hand_written_with_xref(&objs, "")
}

/// Saves `applicant` through the incremental path and returns `/NeedAppearances` afterwards.
fn need_appearances_after_save(other_widget: &str) -> Option<bool> {
    let orig = need_appearances_pdf(other_widget);
    let overlay = Overlay {
        form: WireForm {
            applicant: Some("홍길동".to_string()),
            department: None,
        },
        ..Default::default()
    };
    let pages = [PagePlan {
        source: 0,
        index: 0,
        turn: 0,
    }];
    let built = build(
        &[Arc::new(orig.clone())],
        &pages,
        &overlay,
        &mut blank_overlay,
        &deadline(),
    )
    .expect("build succeeds");
    assert_eq!(&built.bytes[..orig.len()], orig.as_slice(), "incremental");
    let reopened = open_pdf(&Arc::new(built.bytes)).expect("output opens");
    let catalog = reopened
        .xref()
        .get::<Dict<'_>>(reopened.xref().root_id())
        .expect("catalog");
    let acro = catalog.get::<Dict<'_>>(b"AcroForm").expect("acroform");
    acro.get::<bool>(b"NeedAppearances")
}

#[test]
fn keeps_need_appearances_while_another_field_has_a_value_without_ap() {
    let flag = need_appearances_after_save(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (memo) /V (기존 값) /Rect [10 60 90 90] >>",
    );
    assert_eq!(flag, Some(true));
}

#[test]
fn keeps_need_appearances_for_inherited_value_without_ap() {
    let objs = [
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R \
             /AcroForm << /Fields [6 0 R 8 0 R] /NeedAppearances true >> >>"
                .to_string(),
        ),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R \
             /Annots [6 0 R 7 0 R] >>"
                .to_string(),
        ),
        (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
        (5, "<< /Type /StructTreeRoot >>".to_string()),
        (
            6,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (applicant) /Rect [10 10 90 40] >>"
                .to_string(),
        ),
        (
            7,
            "<< /Type /Annot /Subtype /Widget /Parent 8 0 R /Rect [10 60 90 90] >>".to_string(),
        ),
        (
            8,
            "<< /FT /Tx /T (memo) /V (상위 값) /Kids [7 0 R] >>".to_string(),
        ),
    ];
    let orig = hand_written_with_xref(&objs, "");
    let overlay = Overlay {
        form: WireForm {
            applicant: Some("홍길동".to_string()),
            department: None,
        },
        ..Default::default()
    };
    let pages = [PagePlan {
        source: 0,
        index: 0,
        turn: 0,
    }];
    let built = build(
        &[Arc::new(orig.clone())],
        &pages,
        &overlay,
        &mut blank_overlay,
        &deadline(),
    )
    .expect("build succeeds");
    let reopened = open_pdf(&Arc::new(built.bytes)).expect("output opens");
    let catalog = reopened
        .xref()
        .get::<Dict<'_>>(reopened.xref().root_id())
        .expect("catalog");
    let acro = catalog.get::<Dict<'_>>(b"AcroForm").expect("acroform");
    assert_eq!(acro.get::<bool>(b"NeedAppearances"), Some(true));
}

#[test]
fn drops_need_appearances_when_every_other_value_has_an_appearance() {
    let flag = need_appearances_after_save(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (memo) /V (기존 값) /Rect [10 60 90 90] \
         /AP << /N 4 0 R >> >>",
    );
    assert_eq!(flag, None);
}

#[test]
fn drops_need_appearances_when_other_fields_are_empty() {
    let flag = need_appearances_after_save(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (memo) /V () /Rect [10 60 90 90] >>",
    );
    assert_eq!(flag, None);
}
