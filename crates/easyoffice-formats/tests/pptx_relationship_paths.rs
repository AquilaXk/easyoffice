//! Relationship targets in a PPTX are paths relative to the part that holds them, with `.` and
//! `..` segments (ECMA-376 Part 2, 9.3.2). The reader resolved the notes slide target that way but
//! took the targets of `presentation.xml.rels` literally after `ppt/`, so `./slides/slide2.xml`
//! matched no part and the slide order fell back to file names (PD-172).

use easyoffice_formats::{OoxmlCodec, OoxmlPptxPayload, ZipArchiveCodec, ZipSecurityPolicy};

fn two_slides() -> Vec<(String, Vec<u8>)> {
    let payload = OoxmlPptxPayload {
        title: "t".into(),
        is_slideshow: false,
        slides: vec![
            ("첫째".into(), vec!["a".into()], "첫 노트".into()),
            ("둘째".into(), vec!["b".into()], "둘 노트".into()),
        ],
    };
    let bytes = OoxmlCodec::pack_pptx(&payload).unwrap();
    ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn edit(entries: &mut [(String, Vec<u8>)], path: &str, f: impl Fn(&str) -> String) {
    let entry = entries.iter_mut().find(|(p, _)| p == path).unwrap();
    entry.1 = f(std::str::from_utf8(&entry.1).unwrap()).into_bytes();
}

fn titles(entries: &[(String, Vec<u8>)]) -> Vec<String> {
    let bytes = ZipArchiveCodec::pack(entries).unwrap();
    OoxmlCodec::unpack_pptx(&bytes)
        .unwrap()
        .slides
        .into_iter()
        .map(|s| format!("{}/{}", s.0, s.2))
        .collect()
}

#[test]
fn presentation_targets_with_dot_segments_keep_the_slide_order() {
    let mut entries = two_slides();
    // rId1 -> slide2 and rId2 -> slide1: the list order differs from the file-name order.
    edit(&mut entries, "ppt/_rels/presentation.xml.rels", |xml| {
        xml.replace(
            "Target=\"slides/slide1.xml\"",
            "Target=\"./slides/TMP.xml\"",
        )
        .replace(
            "Target=\"slides/slide2.xml\"",
            "Target=\"../ppt/slides/slide1.xml\"",
        )
        .replace("./slides/TMP.xml", "./slides/slide2.xml")
    });
    assert_eq!(titles(&entries), ["둘째/둘 노트", "첫째/첫 노트"]);
}

#[test]
fn notes_targets_with_dot_segments_resolve() {
    let mut entries = two_slides();
    edit(&mut entries, "ppt/slides/_rels/slide1.xml.rels", |xml| {
        xml.replace("../notesSlides/", "../slides/../notesSlides/")
    });
    edit(&mut entries, "ppt/slides/_rels/slide2.xml.rels", |xml| {
        xml.replace("../notesSlides/", "./../notesSlides/")
    });
    assert_eq!(titles(&entries), ["첫째/첫 노트", "둘째/둘 노트"]);
}
