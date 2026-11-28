//! Drawing guides of a slide deck (`PD-402`, 보기 ▸ 안내선): the `p:guideLst` of `ppt/viewProps.xml`.
//!
//! Reference implementation (3-step check): (1) no maintained crate reads or writes a PresentationML view part, and the
//! reference writers named for this work (PptxGenJS, python-pptx) write no guides, so there is no source to take; (2) the
//! deck settings (`DeckSettings`) and the part helpers of the PPTX package already carry the other deck-wide parts, so the
//! guides join them; (3) the element names and attributes follow ECMA-376 Part 1 19.2.1.15 (`p:guide`, `orient` `horz` or
//! `vert` with `vert` the default, `pos` in 1/8 point, which is what PowerPoint writes: the center guides of a 10 in x 7.5 in
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

/// True when the settings need a view part at all.
pub(crate) fn needs_view_props(settings: &DeckSettings) -> bool {
    settings.show_guides || !settings.guides.is_empty()
}

/// `ppt/viewProps.xml` with the guides (and whether they are shown).
pub(crate) fn view_props_xml(settings: &DeckSettings) -> String {
    let show = if settings.show_guides {
        " showGuides=\"1\""
    } else {
        ""
    };
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
    let list = if list.is_empty() {
        String::new()
    } else {
        format!("<p:guideLst>{list}</p:guideLst>")
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:viewPr xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:slideViewPr><p:cSldViewPr{show}><p:cViewPr varScale=\"1\"><p:scale><a:sx n=\"100\" d=\"100\"/><a:sy n=\"100\" d=\"100\"/></p:scale><p:origin x=\"0\" y=\"0\"/></p:cViewPr>{list}</p:cSldViewPr></p:slideViewPr></p:viewPr>"
    )
}

/// The guides and the show flag of a `ppt/viewProps.xml`. A guide past the position limit is dropped and more than
/// [`MAX_SLIDE_GUIDES`] guides refuse the file.
pub(crate) fn read_guides(xml: &str) -> Result<(Vec<SlideGuide>, bool), EasyOfficeError> {
    let show = extract_balanced_elements(xml, "p:cSldViewPr")
        .first()
        .and_then(|(attrs, _)| extract_attr_from_fragment(attrs, "showGuides"))
        .is_some_and(|v| v == "1" || v == "true");
    let found = extract_balanced_elements(xml, "p:guide");
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

    #[test]
    fn hostile_guide_lists_are_cut_or_refused() {
        let far = "<p:viewPr><p:cSldViewPr><p:guideLst><p:guide pos=\"99999999\"/><p:guide pos=\"x\"/><p:guide pos=\"80\"/></p:guideLst></p:cSldViewPr></p:viewPr>";
        assert_eq!(read_guides(far).unwrap().0, vec![guide(true, 80)]);
        let many = format!(
            "<p:viewPr><p:cSldViewPr><p:guideLst>{}</p:guideLst></p:cSldViewPr></p:viewPr>",
            "<p:guide pos=\"8\"/>".repeat(MAX_SLIDE_GUIDES + 1)
        );
        assert!(read_guides(&many).is_err());
        assert!(check_guides(&vec![guide(true, 0); MAX_SLIDE_GUIDES + 1]).is_err());
        assert!(check_guides(&[guide(true, MAX_GUIDE_POS + 1)]).is_err());
    }
}
