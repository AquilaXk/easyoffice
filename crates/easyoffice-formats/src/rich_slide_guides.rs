//! Drawing guides of a slide deck (`PD-402`, 보기 ▸ 안내선): the `p:guideLst` of `ppt/viewProps.xml`.
//!
//! Reference implementation (3-step check): (1) no maintained crate reads or writes a PresentationML view part, and the
//! reference writers named for this work (PptxGenJS, python-pptx) write no guides, so there is no source to take; (2) the
//! deck settings (`DeckSettings`) and the part helpers of the PPTX package already carry the other deck-wide parts, so the
//! guides join them; (3) the element names and attributes follow ECMA-376 Part 1 19.2.1.15 (`p:guide`, `orient` `horz` or
//! `vert` with `vert` the default, `pos` in 1/8 point, which is what the Korean presentation program writes: the center guides of a 10 in x 7.5 in
//! slide are `pos="2880"` and `pos="2160"`) and 19.2.1.16 (`p:guideLst` inside `p:cSldViewPr`, whose `showGuides` attribute is
//! the 안내선 check box). Nothing is copied. Guides are capped in number and position; a part past a limit is refused.

use std::fmt::Write;

use crate::rich_slide::MAX_SLIDE_COORD_PT;
use crate::rich_slide_ext::DeckSettings;
use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_balanced_elements};
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

/// The `p:guideLst` of the guides, empty when there are none.
fn guide_list_xml(settings: &DeckSettings) -> String {
    let mut list = String::new();
    for g in settings
        .guides
        .iter()
        .take(MAX_SLIDE_GUIDES)
        .filter(|g| g.is_valid())
    {
        let orient = if g.vertical { "" } else { " orient=\"horz\"" };
        let _ = write!(list, "<p:guide{orient} pos=\"{}\"/>", g.pos);
    }
    if list.is_empty() {
        list
    } else {
        format!("<p:guideLst>{list}</p:guideLst>")
    }
}

/// The index just past the `>` of the start tag that begins at `from` (which holds `<name`), and whether it ends in `/>`.
fn tag_end(xml: &str, from: usize) -> Option<(usize, bool)> {
    let close = from + xml[from..].find('>')?;
    Some((close + 1, xml[..close].ends_with('/')))
}

/// `p:slideViewPr` of a view part with its guides and `showGuides` replaced and every other element left as it was; `None`
/// when the part has no shape this can patch (the caller then writes a fresh part).
fn patch_view_props(raw: &str, settings: &DeckSettings) -> Option<String> {
    let list = guide_list_xml(settings);
    let show = if settings.show_guides {
        " showGuides=\"1\""
    } else {
        ""
    };
    let fresh_view = |list: &str| {
        format!("<p:slideViewPr><p:cSldViewPr{show}><p:cViewPr varScale=\"1\"><p:scale><a:sx n=\"100\" d=\"100\"/><a:sy n=\"100\" d=\"100\"/></p:scale><p:origin x=\"0\" y=\"0\"/></p:cViewPr>{list}</p:cSldViewPr></p:slideViewPr>")
    };
    let Some(s_open) = raw.find("<p:slideViewPr") else {
        // No slide view yet: it goes before the views that follow it in the schema order.
        let at = [
            "<p:outlineViewPr",
            "<p:notesTextViewPr",
            "<p:sorterViewPr",
            "<p:notesViewPr",
            "<p:gridSpacing",
            "<p:extLst",
        ]
        .iter()
        .filter_map(|t| raw.find(t))
        .min()
        .or_else(|| raw.find("</p:viewPr>"))?;
        return Some(format!("{}{}{}", &raw[..at], fresh_view(&list), &raw[at..]));
    };
    let (s_body, s_self_closed) = tag_end(raw, s_open)?;
    if s_self_closed {
        return Some(format!(
            "{}{}{}",
            &raw[..s_open],
            fresh_view(&list),
            &raw[s_body..]
        ));
    }
    let s_close = s_body + raw[s_body..].find("</p:slideViewPr>")?;
    let body = &raw[s_body..s_close];
    let patched = match body.find("<p:cSldViewPr") {
        None => format!("<p:cSldViewPr{show}><p:cViewPr varScale=\"1\"><p:scale><a:sx n=\"100\" d=\"100\"/><a:sy n=\"100\" d=\"100\"/></p:scale><p:origin x=\"0\" y=\"0\"/></p:cViewPr>{list}</p:cSldViewPr>{body}"),
        Some(c_open) => {
            let (c_body, c_self_closed) = tag_end(body, c_open)?;
            let start_tag = &body[c_open..c_body];
            // The start tag without `showGuides`, then with it when the guides are shown.
            let without = match start_tag.find(" showGuides=\"") {
                Some(at) => {
                    let end = at + " showGuides=\"".len() + start_tag[at + " showGuides=\"".len()..].find('"')? + 1;
                    format!("{}{}", &start_tag[..at], &start_tag[end..])
                }
                None => start_tag.to_string(),
            };
            let name_end = "<p:cSldViewPr".len();
            let tag = format!("{}{show}{}", &without[..name_end], &without[name_end..]);
            if c_self_closed {
                let open = tag.trim_end_matches("/>").to_string() + ">";
                format!("{}{open}{list}</p:cSldViewPr>{}", &body[..c_open], &body[c_body..])
            } else {
                let c_close = c_body + body[c_body..].find("</p:cSldViewPr>")?;
                let mut inner = body[c_body..c_close].to_string();
                if let Some(at) = inner.find("<p:guideLst") {
                    let (g_body, g_self_closed) = tag_end(&inner, at)?;
                    let end = if g_self_closed {
                        g_body
                    } else {
                        g_body + inner[g_body..].find("</p:guideLst>")? + "</p:guideLst>".len()
                    };
                    inner.replace_range(at..end, "");
                }
                // `p:extLst` is the last child of the view, the guides come before it.
                let at = inner.find("<p:extLst").unwrap_or(inner.len());
                inner.insert_str(at, &list);
                format!("{}{tag}{inner}{}", &body[..c_open], &body[c_close..])
            }
        }
    };
    Some(format!("{}{patched}{}", &raw[..s_body], &raw[s_close..]))
}

/// `ppt/viewProps.xml` with the guides (and whether they are shown). A deck read from a file keeps the part it came with, so
/// the other views (normal, outline, notes, sorter, grid) are written back as they were.
pub(crate) fn view_props_xml(settings: &DeckSettings) -> String {
    if let Some(patched) = settings
        .view_props
        .as_deref()
        .filter(|raw| raw.len() <= MAX_VIEW_PROPS_BYTES)
        .and_then(|raw| patch_view_props(raw, settings))
    {
        return patched;
    }
    let show = if settings.show_guides {
        " showGuides=\"1\""
    } else {
        ""
    };
    let list = guide_list_xml(settings);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:viewPr xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:slideViewPr><p:cSldViewPr{show}><p:cViewPr varScale=\"1\"><p:scale><a:sx n=\"100\" d=\"100\"/><a:sy n=\"100\" d=\"100\"/></p:scale><p:origin x=\"0\" y=\"0\"/></p:cViewPr>{list}</p:cSldViewPr></p:slideViewPr></p:viewPr>"
    )
}

/// The guides and the show flag of a `ppt/viewProps.xml`. A guide past the position limit is dropped and more than
/// [`MAX_SLIDE_GUIDES`] guides refuse the file.
pub(crate) fn read_guides(xml: &str) -> Result<(Vec<SlideGuide>, bool), EasyOfficeError> {
    // Only the guides of the slide view count: the notes view and the others carry guides of their own.
    let slide_view = extract_balanced_elements(xml, "p:slideViewPr")
        .into_iter()
        .next()
        .map_or("", |(_, body)| body);
    let show = extract_balanced_elements(slide_view, "p:cSldViewPr")
        .first()
        .and_then(|(attrs, _)| extract_attr_from_fragment(attrs, "showGuides"))
        .is_some_and(|v| v == "1" || v == "true");
    let found = extract_balanced_elements(slide_view, "p:guide");
    if found.len() > MAX_SLIDE_GUIDES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "PPTX 안내선 수가 상한({MAX_SLIDE_GUIDES})을 초과했습니다."
        )));
    }
    let guides = found
        .iter()
        .filter_map(|(attrs, _)| {
            let pos = extract_attr_from_fragment(attrs, "pos")?
                .trim()
                .parse::<i32>()
                .ok()?;
            let vertical = extract_attr_from_fragment(attrs, "orient") != Some("horz");
            Some(SlideGuide { vertical, pos }).filter(|g| g.is_valid())
        })
        .collect();
    Ok((guides, show))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(vertical: bool, pos: i32) -> SlideGuide {
        SlideGuide { vertical, pos }
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
        assert_eq!(read_guides(&xml).unwrap(), (settings.guides, true));
    }

    const FILE_VIEW: &str = "<?xml version=\"1.0\"?><p:viewPr xmlns:p=\"p\"><p:normalViewPr><p:restoredLeft sz=\"15620\"/></p:normalViewPr><p:slideViewPr><p:cSldViewPr snapToGrid=\"0\" showGuides=\"1\"><p:cViewPr varScale=\"1\"><p:scale><a:sx n=\"1\" d=\"1\"/></p:scale></p:cViewPr><p:guideLst><p:guide pos=\"10\"/></p:guideLst><p:extLst/></p:cSldViewPr></p:slideViewPr><p:notesViewPr><p:cSldViewPr><p:cViewPr/><p:guideLst><p:guide orient=\"horz\" pos=\"99\"/></p:guideLst></p:cSldViewPr></p:notesViewPr><p:gridSpacing cx=\"72008\" cy=\"72008\"/></p:viewPr>";

    #[test]
    fn only_the_guides_of_the_slide_view_are_read() {
        let (guides, show) = read_guides(FILE_VIEW).unwrap();
        assert_eq!(
            guides,
            vec![guide(true, 10)],
            "the notes view's guide is not a slide guide"
        );
        assert!(show);
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
        assert_eq!(read_guides(&xml).unwrap(), (settings.guides.clone(), false));
        // Without guides the list goes and the rest stays; a part with no slide view gets one.
        let none = DeckSettings {
            view_props: Some(FILE_VIEW.to_string()),
            ..DeckSettings::default()
        };
        assert!(
            !view_props_xml(&none).contains("pos=\"10\"")
                && view_props_xml(&none).contains("pos=\"99\"")
        );
        let no_slide = DeckSettings {
            guides: vec![guide(true, 8)],
            view_props: Some("<p:viewPr><p:notesViewPr/></p:viewPr>".to_string()),
            ..DeckSettings::default()
        };
        let xml = view_props_xml(&no_slide);
        assert!(xml.contains("<p:guide pos=\"8\"/>") && xml.contains("<p:notesViewPr/>"));
        assert!(xml.find("<p:slideViewPr").unwrap() < xml.find("<p:notesViewPr").unwrap());
        let broken = DeckSettings {
            view_props: Some("<p:viewPr><p:slideViewPr>".to_string()),
            ..DeckSettings::default()
        };
        assert!(
            view_props_xml(&broken).contains("<p:viewPr "),
            "a torn part is replaced by a fresh one"
        );
    }

    #[test]
    fn hostile_guide_lists_are_cut_or_refused() {
        let far = "<p:viewPr><p:slideViewPr><p:cSldViewPr><p:guideLst><p:guide pos=\"99999999\"/><p:guide pos=\"x\"/><p:guide pos=\"80\"/></p:guideLst></p:cSldViewPr></p:slideViewPr></p:viewPr>";
        assert_eq!(read_guides(far).unwrap().0, vec![guide(true, 80)]);
        let many = format!(
            "<p:viewPr><p:slideViewPr><p:cSldViewPr><p:guideLst>{}</p:guideLst></p:cSldViewPr></p:slideViewPr></p:viewPr>",
            "<p:guide pos=\"8\"/>".repeat(MAX_SLIDE_GUIDES + 1)
        );
        assert!(read_guides(&many).is_err());
        assert!(check_guides(&vec![guide(true, 0); MAX_SLIDE_GUIDES + 1]).is_err());
        assert!(check_guides(&[guide(true, MAX_GUIDE_POS + 1)]).is_err());
    }
}
