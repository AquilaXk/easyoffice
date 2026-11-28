//! Drawing guides of a slide deck (`PD-402`, 보기 ▸ 안내선): the `p:guideLst` of `ppt/viewProps.xml`.
//!
//! Reference implementation (3-step check): (1) no maintained crate reads or writes a PresentationML view part, and the
//! reference writers named for this work (PptxGenJS, python-pptx) write no guides, so there is no source to take; (2) the
//! deck settings (`DeckSettings`) and the part helpers of the PPTX package already carry the other deck-wide parts, so the
//! guides join them, and the part is read and patched with the `quick-xml` tokenizer the other readers use
//! (`pptx_view_props.rs`); (3) the element names and attributes follow ECMA-376 Part 1 19.2.1.15 (`p:guide`, `orient` `horz` or
//! `vert` with `vert` the default, `pos` in 1/8 point, which is what the Korean presentation program writes: the center guides of a 10 in x 7.5 in
//! slide are `pos="2880"` and `pos="2160"`) and 19.2.1.16 (`p:guideLst` inside `p:cSldViewPr`, whose `showGuides` attribute is
//! the 안내선 check box). Nothing is copied. Guides are capped in number and position; a part past a limit is refused.

use std::fmt::Write;

use crate::pptx_view_props::{patch, scan, slide_view_fragment};
use crate::rich_slide::MAX_SLIDE_COORD_PT;
use crate::rich_slide_ext::DeckSettings;
use crate::EasyOfficeError;

pub const MAX_SLIDE_GUIDES: usize = 64;
/// Units of a guide position per point.
pub const GUIDE_UNITS_PER_PT: i32 = 8;
/// Furthest a guide may lie from the page origin, in guide units (the coordinate cap of a slide).
pub const MAX_GUIDE_POS: i32 = (MAX_SLIDE_COORD_PT as i32) * GUIDE_UNITS_PER_PT;

pub(crate) const VIEW_PROPS_PATH: &str = "ppt/viewProps.xml";
pub(crate) const VIEW_PROPS_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml";

/// One guide: a vertical line at `pos` from the left edge, or a horizontal one at `pos` from the top, in 1/8 point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlideGuide {
    pub vertical: bool,
    pub pos: i32,
}

impl SlideGuide {
    pub fn is_valid(self) -> bool {
        (-MAX_GUIDE_POS..=MAX_GUIDE_POS).contains(&self.pos)
    }
}

/// Fail-Closed check of a guide list a save request carries.
pub fn check_guides(guides: &[SlideGuide]) -> Result<(), EasyOfficeError> {
    if guides.len() > MAX_SLIDE_GUIDES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "안내선 수가 상한({MAX_SLIDE_GUIDES})을 초과했습니다."
        )));
    }
    if guides.iter().any(|g| !g.is_valid()) {
        return Err(EasyOfficeError::InvalidArgument(
            "안내선 위치가 슬라이드 좌표 범위를 벗어났습니다.".to_string(),
        ));
    }
    Ok(())
}

/// Longest `ppt/viewProps.xml` a deck keeps to write back (the other views of the file stay in it); a longer one is dropped.
pub const MAX_VIEW_PROPS_BYTES: usize = 65_536;

/// True when the settings need a view part at all.
pub(crate) fn needs_view_props(settings: &DeckSettings) -> bool {
    settings.show_guides || !settings.guides.is_empty() || settings.view_props.is_some()
}

/// The `p:guideLst` of the guides (its elements under the prefix `pp`, `"p:"` or `""`), empty when there are none.
fn guide_list_xml(settings: &DeckSettings, pp: &str) -> String {
    let mut list = String::new();
    for g in settings
        .guides
        .iter()
        .take(MAX_SLIDE_GUIDES)
        .filter(|g| g.is_valid())
    {
        let orient = if g.vertical { "" } else { " orient=\"horz\"" };
        let _ = write!(list, "<{pp}guide{orient} pos=\"{}\"/>", g.pos);
    }
    if list.is_empty() {
        list
    } else {
        format!("<{pp}guideLst>{list}</{pp}guideLst>")
    }
}

/// `ppt/viewProps.xml` with the guides (and whether they are shown). A deck read from a file keeps the part it came with, so
/// the other views (normal, outline, notes, sorter, grid) are written back as they were, but only a part, and a patch of it,
/// that passes the same check as every other door of the part (`pptx_view_props.rs`); anything else is written fresh.
pub(crate) fn view_props_xml(settings: &DeckSettings) -> String {
    let patched = settings.view_props.as_deref().and_then(|raw| {
        let found = scan(raw).ok()?;
        let list = guide_list_xml(settings, &found.prefix);
        let out = patch(raw, &found, settings.show_guides, &list)?;
        scan(&out).ok().map(|_| out)
    });
    patched.unwrap_or_else(|| {
        let view = slide_view_fragment("p:", settings.show_guides, &guide_list_xml(settings, "p:"));
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:viewPr xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">{view}</p:viewPr>"
        )
    })
}

/// What a view part gives a deck: the guides and the show flag of its slide view, and the part itself when it passes the check
/// (so a save can write it back patched). A part that fails the check gives neither guides nor a part (the deck then gets a
/// fresh one); more than [`MAX_SLIDE_GUIDES`] guides refuse the file.
pub(crate) type ReadView = (Vec<SlideGuide>, bool, Option<String>);

pub(crate) fn read_view_part(xml: &str) -> Result<ReadView, EasyOfficeError> {
    let Ok(found) = scan(xml) else {
        return Ok((Vec::new(), false, None));
    };
    if found.guides.len() > MAX_SLIDE_GUIDES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "PPTX 안내선 수가 상한({MAX_SLIDE_GUIDES})을 초과했습니다."
        )));
    }
    let show = found
        .slide
        .as_ref()
        .and_then(|s| s.csld.as_ref())
        .is_some_and(|c| c.show);
    let guides = found.guides.into_iter().filter(|g| g.is_valid()).collect();
    Ok((guides, show, Some(xml.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pptx_view_props::validate_view_props;

    const NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";

    fn guide(vertical: bool, pos: i32) -> SlideGuide {
        SlideGuide { vertical, pos }
    }

    fn read(xml: &str) -> (Vec<SlideGuide>, bool) {
        let (guides, show, _) = read_view_part(xml).unwrap();
        (guides, show)
    }

    #[test]
    fn guides_go_through_the_view_part_and_back() {
        let settings = DeckSettings {
            guides: vec![guide(true, 2880), guide(false, 2160), guide(true, -8)],
            show_guides: true,
            ..DeckSettings::default()
        };
        let xml = view_props_xml(&settings);
        assert!(xml.contains("<p:guide pos=\"2880\"/><p:guide orient=\"horz\" pos=\"2160\"/>"));
        validate_view_props(&xml).unwrap();
        assert_eq!(read(&xml), (settings.guides, true));
    }

    const FILE_VIEW: &str = "<?xml version=\"1.0\"?><p:viewPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:normalViewPr><p:restoredLeft sz=\"15620\"/></p:normalViewPr><p:slideViewPr><p:cSldViewPr snapToGrid=\"0\" showGuides=\"1\"><p:cViewPr varScale=\"1\"><p:scale><a:sx n=\"1\" d=\"1\"/></p:scale></p:cViewPr><p:guideLst><p:guide pos=\"10\"/></p:guideLst><p:extLst/></p:cSldViewPr></p:slideViewPr><p:notesViewPr><p:cSldViewPr><p:cViewPr/><p:guideLst><p:guide orient=\"horz\" pos=\"99\"/></p:guideLst></p:cSldViewPr></p:notesViewPr><p:gridSpacing cx=\"72008\" cy=\"72008\"/></p:viewPr>";

    #[test]
    fn only_the_guides_of_the_slide_view_are_read() {
        let (guides, show, kept) = read_view_part(FILE_VIEW).unwrap();
        assert_eq!(
            guides,
            vec![guide(true, 10)],
            "the notes view's guide is not a slide guide"
        );
        assert!(show);
        assert_eq!(kept.as_deref(), Some(FILE_VIEW));
    }

    #[test]
    fn a_file_view_part_is_patched_not_rewritten() {
        let settings = DeckSettings {
            guides: vec![guide(false, 160), guide(true, 8)],
            show_guides: false,
            view_props: Some(FILE_VIEW.to_string()),
            ..DeckSettings::default()
        };
        let xml = view_props_xml(&settings);
        for kept in [
            "<p:normalViewPr><p:restoredLeft sz=\"15620\"/></p:normalViewPr>",
            "snapToGrid=\"0\"",
            "<p:notesViewPr><p:cSldViewPr><p:cViewPr/><p:guideLst><p:guide orient=\"horz\" pos=\"99\"/></p:guideLst></p:cSldViewPr></p:notesViewPr>",
            "<p:gridSpacing cx=\"72008\" cy=\"72008\"/>",
            "<p:extLst/></p:cSldViewPr>",
        ] {
            assert!(xml.contains(kept), "{kept} in {xml}");
        }
        assert!(xml.contains("<p:guideLst><p:guide orient=\"horz\" pos=\"160\"/><p:guide pos=\"8\"/></p:guideLst><p:extLst/>"));
        assert!(!xml.contains("pos=\"10\""), "the old slide guide is gone");
        assert!(!xml.contains("showGuides"), "hidden guides drop the flag");
        assert_eq!(read(&xml), (settings.guides.clone(), false));
        // Without guides the list goes and the rest stays.
        let none = DeckSettings {
            view_props: Some(FILE_VIEW.to_string()),
            ..DeckSettings::default()
        };
        let xml = view_props_xml(&none);
        assert!(!xml.contains("pos=\"10\"") && xml.contains("pos=\"99\""));
    }

    #[test]
    fn a_part_without_a_slide_view_or_with_empty_pieces_is_patched_in_place() {
        let list = |g: Vec<SlideGuide>| DeckSettings {
            guides: g,
            show_guides: true,
            ..DeckSettings::default()
        };
        let with = |raw: &str, g: Vec<SlideGuide>| {
            let mut s = list(g);
            s.view_props = Some(raw.to_string());
            view_props_xml(&s)
        };
        let no_slide = format!("<p:viewPr xmlns:p=\"{NS}\"><p:notesViewPr/><p:gridSpacing cx=\"1\" cy=\"1\"/></p:viewPr>");
        let xml = with(&no_slide, vec![guide(true, 8)]);
        assert!(xml.contains("<p:guide pos=\"8\"/>") && xml.contains("<p:notesViewPr/>"));
        assert!(xml.find("<p:slideViewPr").unwrap() < xml.find("<p:notesViewPr").unwrap());
        assert_eq!(read(&xml), (vec![guide(true, 8)], true));
        let root_empty = format!("<p:viewPr xmlns:p=\"{NS}\"/>");
        assert_eq!(
            read(&with(&root_empty, vec![guide(true, 8)])),
            (vec![guide(true, 8)], true)
        );
        let slide_empty =
            format!("<p:viewPr xmlns:p=\"{NS}\"><p:slideViewPr/><p:notesViewPr/></p:viewPr>");
        let xml = with(&slide_empty, vec![guide(false, 16)]);
        assert!(xml.contains("<p:notesViewPr/>"));
        assert_eq!(read(&xml), (vec![guide(false, 16)], true));
        let csld_empty = format!("<p:viewPr xmlns:p=\"{NS}\"><p:slideViewPr><p:cSldViewPr snapToGrid=\"0\"/></p:slideViewPr></p:viewPr>");
        let xml = with(&csld_empty, vec![guide(true, 24)]);
        assert!(xml.contains("snapToGrid=\"0\" showGuides=\"1\">"), "{xml}");
        assert_eq!(read(&xml), (vec![guide(true, 24)], true));
        let no_csld =
            format!("<p:viewPr xmlns:p=\"{NS}\"><p:slideViewPr></p:slideViewPr></p:viewPr>");
        assert_eq!(
            read(&with(&no_csld, vec![guide(true, 32)])),
            (vec![guide(true, 32)], true)
        );
    }

    #[test]
    fn the_patch_follows_the_markup_not_the_text() {
        // A comment, a single-quoted attribute, a `>` inside an attribute value, a default namespace and a prefix of its own.
        let tricky = format!(
            "<?xml version='1.0' encoding='utf-8'?>\n<!-- <p:slideViewPr><p:guideLst><p:guide pos=\"1\"/></p:guideLst> -->\n<v:viewPr xmlns:v='{NS}' note='a > b'><v:slideViewPr><!-- <v:guideLst/> --><v:cSldViewPr note=\"x>y\" showGuides='1' snap='a&amp;b'><v:cViewPr/><v:guideLst><v:guide pos='7'/></v:guideLst></v:cSldViewPr></v:slideViewPr><v:notesViewPr><v:cSldViewPr><v:guideLst><v:guide pos='55'/></v:guideLst></v:cSldViewPr></v:notesViewPr></v:viewPr>\n"
        );
        let (guides, show, kept) = read_view_part(&tricky).unwrap();
        assert_eq!((guides, show), (vec![guide(true, 7)], true));
        assert!(kept.is_some());
        let settings = DeckSettings {
            guides: vec![guide(false, 40)],
            show_guides: false,
            view_props: Some(tricky.clone()),
            ..DeckSettings::default()
        };
        let xml = view_props_xml(&settings);
        assert!(xml.starts_with("<?xml version='1.0' encoding='utf-8'?>\n<!-- <p:slideViewPr>"));
        assert!(
            xml.contains(
                "<v:guideLst><v:guide orient=\"horz\" pos=\"40\"/></v:guideLst></v:cSldViewPr>"
            ),
            "{xml}"
        );
        assert!(
            xml.contains("note=\"x&gt;y\"") && xml.contains("snap=\"a&amp;b\""),
            "{xml}"
        );
        assert!(
            xml.contains("<v:guide pos='55'/>")
                && xml.contains("note='a > b'")
                && xml.contains("<!-- <v:guideLst/> -->")
        );
        assert!(!xml.contains("showGuides"));
        assert_eq!(read(&xml), (vec![guide(false, 40)], false));
    }

    #[test]
    fn hostile_view_parts_fail_the_check_everywhere() {
        let ok = format!("<p:viewPr xmlns:p=\"{NS}\"><p:slideViewPr/></p:viewPr>");
        validate_view_props(&ok).unwrap();
        validate_view_props(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<viewPr xmlns=\"{NS}\"/>\n"
        ))
        .unwrap();
        validate_view_props(&format!("<a:viewPr xmlns:a='{NS}'><a:x/></a:viewPr> \n\t")).unwrap();
        let big = format!(
            "<p:viewPr xmlns:p=\"{NS}\">{}</p:viewPr>",
            " ".repeat(MAX_VIEW_PROPS_BYTES)
        );
        let hostile = [
            format!("<!DOCTYPE x><viewPr xmlns=\"{NS}\"/>"),
            format!("<!DOCTYPE x [<!ENTITY e \"v\">]><p:viewPr xmlns:p=\"{NS}\">&e;</p:viewPr>"),
            format!("<p:viewPr xmlns:p=\"{NS}\"><p:slideViewPr>"),
            format!("<p:viewPr xmlns:p=\"{NS}\"><p:a></p:b></p:viewPr>"),
            format!("<p:viewPr xmlns:p=\"{NS}\"></p:viewPr>tail"),
            format!("<p:viewPr xmlns:p=\"{NS}\"/><p:viewPr xmlns:p=\"{NS}\"/>"),
            format!("<p:viewPr xmlns:p=\"{NS}\"/><!-- later -->"),
            format!("<p:viewPr xmlns:p=\"{NS}\"/><?pi x?>"),
            format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?><p:viewPr xmlns:p=\"{NS}\"/>"),
            format!("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><p:viewPr xmlns:p=\"{NS}\"/>"),
            format!(" <?xml version=\"1.0\"?><p:viewPr xmlns:p=\"{NS}\"/>"),
            format!("<?xml-stylesheet href=\"x\"?><p:viewPr xmlns:p=\"{NS}\"/>"),
            format!("<p:viewPr xmlns:p=\"{NS}\"><?pi x?></p:viewPr>"),
            format!("<!-- <p:viewPr xmlns:p=\"{NS}\"> -->"),
            format!("x<p:viewPr xmlns:p=\"{NS}\"/>"),
            "<p:viewPr xmlns:p=\"urn:other\"/>".to_string(),
            "<p:viewPr/>".to_string(),
            format!("<p:other xmlns:p=\"{NS}\"/>"),
            format!("<q:viewPr xmlns:p=\"{NS}\"/>"),
            format!("<p:viewPr xmlns:p=\"{NS}\"><p:x xmlns:p=\"urn:other\"/></p:viewPr>"),
            format!("<viewPr xmlns=\"{NS}\"><x xmlns=\"urn:other\"/></viewPr>"),
            format!("<p:viewPr xmlns:p=\"{NS}\">&bogus;</p:viewPr>"),
            format!("<p:viewPr xmlns:p=\"{NS}\" a=\"1\" a=\"2\"/>"),
            String::new(),
            big,
        ];
        for xml in &hostile {
            assert!(
                validate_view_props(xml).is_err(),
                "{}",
                &xml[..xml.len().min(70)]
            );
            // A part the reader cannot keep gives no guides and no part, and the writer sets a fresh one in its place.
            let (guides, _, kept) = read_view_part(xml).unwrap();
            assert!(guides.is_empty() && kept.is_none());
            let settings = DeckSettings {
                guides: vec![guide(true, 8)],
                view_props: Some(xml.clone()),
                ..DeckSettings::default()
            };
            let written = view_props_xml(&settings);
            validate_view_props(&written).unwrap();
            assert_eq!(
                read(&written),
                (vec![guide(true, 8)], false),
                "{}",
                &xml[..xml.len().min(70)]
            );
        }
    }

    #[test]
    fn hostile_guide_lists_are_cut_or_refused() {
        let far = format!("<p:viewPr xmlns:p=\"{NS}\"><p:slideViewPr><p:cSldViewPr><p:guideLst><p:guide pos=\"99999999\"/><p:guide pos=\"x\"/><p:guide pos=\"80\"/></p:guideLst></p:cSldViewPr></p:slideViewPr></p:viewPr>");
        assert_eq!(read(&far).0, vec![guide(true, 80)]);
        let many = format!(
            "<p:viewPr xmlns:p=\"{NS}\"><p:slideViewPr><p:cSldViewPr><p:guideLst>{}</p:guideLst></p:cSldViewPr></p:slideViewPr></p:viewPr>",
            "<p:guide pos=\"8\"/>".repeat(MAX_SLIDE_GUIDES + 1)
        );
        assert!(read_view_part(&many).is_err());
        assert!(check_guides(&vec![guide(true, 0); MAX_SLIDE_GUIDES + 1]).is_err());
        assert!(check_guides(&[guide(true, MAX_GUIDE_POS + 1)]).is_err());
    }
}
