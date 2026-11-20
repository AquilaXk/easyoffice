//! Optional content (layers) across an edit (PD-266): a layer that was hidden stays hidden, in the
//! catalog, in the references the content makes, and in what is drawn.

use crate::pdf_edit_carry_tests::{one_page_with, rebuilt};
use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::{deadline, hand_written_with_xref, read};
use crate::pdf_edit_session::EditSession;
use crate::pdf_first_page::first_page_png;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, ObjectIdentifier, Stream};
use hayro_syntax::Pdf as ReadPdf;
use std::sync::Arc;
use usvg::fontdb::Database;

/// Content that draws four things, each under another kind of switch: a layer that is on (blue), a
/// layer that is off (red), a membership dictionary that needs both layers on (green) and a
/// picture whose own `/OC` names the off layer. Everything is in the upper half of the page: the
/// picture of the page the check compares (`first_page_png`) shows the top of it.
const LAYERED_CONTENT: &str = "/OC /L1 BDC 0 0 1 rg 10 230 60 60 re f EMC \
    /OC /L2 BDC 1 0 0 rg 80 230 60 60 re f EMC \
    /OC /L3 BDC 0 1 0 rg 150 230 40 60 re f EMC \
    q 60 0 0 60 10 160 cm /Im Do Q";

/// A one-page file with the layers above. `off` is the `/OFF` array of the default configuration.
fn layered(off: &str) -> Vec<u8> {
    let catalog = format!(
        "/OCProperties << /OCGs [6 0 R 7 0 R] /D << /Order [6 0 R 7 0 R] /ON [6 0 R] /OFF [{off}] \
         /Name (Standard) >> >>"
    );
    let resources = "/Resources << /Properties << /L1 6 0 R /L2 7 0 R /L3 5 0 R >> \
                     /XObject << /Im 8 0 R >> >>";
    let more = vec![
        "<< /Type /OCMD /OCGs [6 0 R 7 0 R] /P /AllOn >>".to_string(),
        "<< /Type /OCG /Name (Visible) >>".to_string(),
        "<< /Type /OCG /Name (Hidden) >>".to_string(),
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray \
         /BitsPerComponent 8 /OC 7 0 R /Length 1 >>\nstream\nA\nendstream"
            .to_string(),
    ];
    one_page_with(catalog.as_str(), resources, LAYERED_CONTENT, "", &more)
}

fn catalog_of(pdf: &ReadPdf) -> Dict<'_> {
    pdf.xref()
        .get::<Dict<'_>>(pdf.xref().root_id())
        .expect("a catalog")
}

fn ids(array: Option<Array<'_>>) -> Vec<ObjectIdentifier> {
    array
        .map(|array| {
            array
                .raw_iter()
                .filter_map(|item| match item {
                    MaybeRef::Ref(r) => Some(ObjectIdentifier::from(r)),
                    MaybeRef::NotRef(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn name_of(pdf: &ReadPdf, id: ObjectIdentifier) -> String {
    let dict = pdf
        .xref()
        .get::<Dict<'_>>(id)
        .expect("an optional content group");
    assert_eq!(
        dict.get::<Name<'_>>(b"Type")
            .map(|n| n.as_str().to_string()),
        Some("OCG".to_string())
    );
    let text = dict
        .get::<hayro_syntax::object::String<'_>>(b"Name")
        .expect("a name");
    String::from_utf8_lossy(text.as_bytes()).into_owned()
}

/// The form that holds the original page, and the resources it draws with.
fn page_form(pdf: &ReadPdf, page: usize) -> Stream<'_> {
    pdf.pages()[page]
        .raw()
        .get::<Dict<'_>>(b"Resources")
        .and_then(|resources| resources.get::<Dict<'_>>(b"XObject"))
        .and_then(|xobjects| xobjects.get::<Stream<'_>>(b"P"))
        .expect("the page draws the copy of the original")
}

fn card(bytes: &[u8]) -> Vec<u8> {
    first_page_png(&Arc::new(bytes.to_vec()), None, &deadline()).expect("the page is drawn")
}

#[test]
fn a_hidden_layer_stays_listed_hidden_and_referenced_by_the_copied_content() {
    let built = rebuilt(layered("7 0 R"));
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    let out = read(&built.bytes);
    let props = catalog_of(&out)
        .get::<Dict<'_>>(b"OCProperties")
        .expect("the layer list is kept");
    let all = ids(props.get::<Array<'_>>(b"OCGs"));
    let config = props
        .get::<Dict<'_>>(b"D")
        .expect("the default configuration");
    let (on, off) = (
        ids(config.get::<Array<'_>>(b"ON")),
        ids(config.get::<Array<'_>>(b"OFF")),
    );
    assert_eq!(all.len(), 2);
    assert_eq!(on.len(), 1);
    assert_eq!(off.len(), 1);
    assert_eq!(name_of(&out, on[0]), "Visible");
    assert_eq!(name_of(&out, off[0]), "Hidden");
    assert_eq!(ids(config.get::<Array<'_>>(b"Order")), all);

    // The content names the very same objects: a copy of a layer would be a layer nobody switches.
    let form = page_form(&out, 0);
    let properties = form
        .dict()
        .get::<Dict<'_>>(b"Resources")
        .and_then(|r| r.get::<Dict<'_>>(b"Properties"))
        .expect("the layers the content uses");
    assert_eq!(properties.get_ref(b"L1").map(Into::into), Some(on[0]));
    assert_eq!(properties.get_ref(b"L2").map(Into::into), Some(off[0]));
    let membership = properties
        .get::<Dict<'_>>(b"L3")
        .expect("the membership dictionary");
    assert_eq!(ids(membership.get::<Array<'_>>(b"OCGs")), all);
    // A picture keeps its own switch.
    let image = form
        .dict()
        .get::<Dict<'_>>(b"Resources")
        .and_then(|r| r.get::<Dict<'_>>(b"XObject"))
        .and_then(|x| x.get::<Stream<'_>>(b"Im"))
        .expect("the picture");
    assert_eq!(image.dict().get_ref(b"OC").map(Into::into), Some(off[0]));
}

#[test]
fn what_is_drawn_is_the_same_before_and_after_the_edit() {
    let hidden = layered("7 0 R");
    let all_on = layered("");
    let (before_hidden, before_on) = (card(&hidden), card(&all_on));
    assert!(
        before_hidden != before_on,
        "the file shows something the layer hides, so the comparison below means something"
    );
    // `assert!` on the pictures: a failed `assert_eq!` would print both of them.
    assert!(
        card(&rebuilt(hidden).bytes) == before_hidden,
        "hidden stays hidden"
    );
    assert!(
        card(&rebuilt(all_on).bytes) == before_on,
        "visible stays visible"
    );
}

/// Two layers, the second one named in `/ON`, and a drawing under each; `base` is `/BaseState`.
fn base_state_file(base: &str) -> Vec<u8> {
    let catalog =
        format!("/OCProperties << /OCGs [5 0 R 6 0 R] /D << /BaseState /{base} /ON [6 0 R] >> >>");
    let more = vec![
        "<< /Type /OCG /Name (Listed off) >>".to_string(),
        "<< /Type /OCG /Name (Listed on) >>".to_string(),
    ];
    one_page_with(
        &catalog,
        "/Resources << /Properties << /A 5 0 R /B 6 0 R >> >>",
        "/OC /A BDC 1 0 0 rg 10 200 80 90 re f EMC /OC /B BDC 0 0 1 rg 100 200 90 90 re f EMC",
        "",
        &more,
    )
}

#[test]
fn a_base_state_of_off_is_kept_for_a_single_file() {
    let (off, on) = (base_state_file("OFF"), base_state_file("ON"));
    assert!(card(&on) != card(&off), "the base state hides something");
    assert!(card(&rebuilt(off.clone()).bytes) == card(&off));
    assert!(card(&rebuilt(on.clone()).bytes) == card(&on));
}

/// A file with its own layers and one drawing under each; `hidden` says which of them is off.
fn two_layer_file(red_hidden: bool) -> Vec<u8> {
    let off = if red_hidden { "6 0 R" } else { "" };
    let catalog = format!("/OCProperties << /OCGs [5 0 R 6 0 R] /D << /OFF [{off}] >> >>");
    let more = vec![
        "<< /Type /OCG /Name (Blue) >>".to_string(),
        "<< /Type /OCG /Name (Red) >>".to_string(),
    ];
    one_page_with(
        &catalog,
        "/Resources << /Properties << /A 5 0 R /B 6 0 R >> >>",
        "/OC /A BDC 0 0 1 rg 10 200 80 90 re f EMC /OC /B BDC 1 0 0 rg 100 200 90 90 re f EMC",
        "",
        &more,
    )
}

#[test]
fn merged_files_keep_the_layers_each_file_hid() {
    let (first, second) = (two_layer_file(true), two_layer_file(false));
    let (shot_first, shot_second) = (card(&first), card(&second));
    assert!(
        shot_first != shot_second,
        "the files differ in what they show"
    );
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let mut session = EditSession::open(first, &engine).unwrap();
    session.merge(vec![second], &engine).unwrap();
    let built = session.render(None, &engine).unwrap();
    let out = read(&built.bytes);
    let props = catalog_of(&out).get::<Dict<'_>>(b"OCProperties").unwrap();
    assert_eq!(ids(props.get::<Array<'_>>(b"OCGs")).len(), 4);
    let off = ids(props
        .get::<Dict<'_>>(b"D")
        .unwrap()
        .get::<Array<'_>>(b"OFF"));
    assert_eq!(off.len(), 1, "only the layer the first file hid");
    assert_eq!(name_of(&out, off[0]), "Red");

    // The same two files the other way round: the page that is first now is the other file's.
    let (first, second) = (two_layer_file(true), two_layer_file(false));
    let mut session = EditSession::open(second.clone(), &engine).unwrap();
    session.merge(vec![first], &engine).unwrap();
    assert!(card(&session.render(None, &engine).unwrap().bytes) == shot_second);
}

#[test]
fn hostile_layer_structures_end_in_a_sentence_or_a_file_that_opens() {
    // A layer that refers to itself and an order list nested far past the limit.
    let cyclic = vec!["<< /Type /OCG /Name (Loop) /Usage << /Self 5 0 R >> >>".to_string()];
    let catalog = "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>";
    let looped = one_page_with(catalog, "", "", "", &cyclic);
    let out = rebuilt(looped);
    assert_eq!(read(&out.bytes).pages().len(), 1);

    let depth = 300;
    let nested = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let deep = one_page_with(
        &format!("/OCProperties << /OCGs [] /D << /Order {nested} >> >>"),
        "",
        "",
        "",
        &[],
    );
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let session = EditSession::open(deep, &engine).expect("the file opens");
    let err = session.render(None, &engine).unwrap_err();
    assert!(err.contains("선택적 내용"), "{err}");
}

#[test]
fn a_layer_list_past_the_limit_is_refused() {
    let count = crate::pdf_edit_budget::MAX_VALUE_NODES + 10;
    let many = "[] ".repeat(count);
    let file = one_page_with(
        &format!("/OCProperties << /OCGs [] /D << /Order [{many}] >> >>"),
        "",
        "",
        "",
        &[],
    );
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let session = EditSession::open(file, &engine).unwrap();
    let err = session.render(None, &engine).unwrap_err();
    assert!(err.contains("선택적 내용"), "{err}");
}

#[test]
fn a_file_without_layers_gets_no_layer_list() {
    let file = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 50 50] >>".to_string(),
            ),
        ],
        "",
    );
    let out = read(&rebuilt(file).bytes);
    assert!(!catalog_of(&out).contains_key(b"OCProperties"));
}
