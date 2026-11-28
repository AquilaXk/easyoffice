//! Deck-wide PresentationML parts of [`crate::DeckSettings`] (PD-398): `ppt/presProps.xml` (`p:showPr loop`, the
//! 쇼 설정 repeat flag) and the theme's `a:clrScheme` (the 색 구성표).
//!
//! Reuse (구현 원칙 3단계): (1) no Rust PresentationML crate exists; (2) the theme reader `PptxThemePalette` (the slide readers' own
//! color resolution) reads the twelve slots, and `drawingml_theme_xml` is the theme the writer already emits, so only its
//! `a:clrScheme` block is swapped; (3) the element names and the slot order (dk1, lt1, dk2, lt2, accent1 to accent6, hlink,
//! folHlink) follow ECMA-376 Part 1 §20.1.6.2 as PptxGenJS (MIT, Copyright (c) 2015-present Brent Ely) writes its theme part
//! (`src/gen-xml.ts` `makeXmlTheme`). Nothing was copied. A scheme has twelve colors, so a hostile theme cannot grow it.

use std::fmt::Write;

use crate::ooxml_shared_parts::drawingml_theme_xml;
use crate::pptx_theme_color::{rgb_to_hex, PptxThemePalette, ThemeSlot};
use crate::rich_slide_ext::{ColorScheme, DeckSettings, COLOR_SCHEME_SLOTS};
use crate::rtf_xml_helpers::extract_attr;
use crate::{EasyOfficeError, XmlSanitizer};

/// Slot order of a scheme, as the theme part lists them.
const SLOT_TAGS: [(&str, ThemeSlot); COLOR_SCHEME_SLOTS] = [
    ("dk1", ThemeSlot::Dk1),
    ("lt1", ThemeSlot::Lt1),
    ("dk2", ThemeSlot::Dk2),
    ("lt2", ThemeSlot::Lt2),
    ("accent1", ThemeSlot::Accent1),
    ("accent2", ThemeSlot::Accent2),
    ("accent3", ThemeSlot::Accent3),
    ("accent4", ThemeSlot::Accent4),
    ("accent5", ThemeSlot::Accent5),
    ("accent6", ThemeSlot::Accent6),
    ("hlink", ThemeSlot::Hlink),
    ("folHlink", ThemeSlot::FolHlink),
];

pub(crate) const PRES_PROPS_PATH: &str = "ppt/presProps.xml";
pub(crate) const PRES_PROPS_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";

/// `ppt/presProps.xml`: only the show's repeat flag.
pub(crate) fn pres_props_xml(settings: &DeckSettings) -> String {
    let loop_attr = if settings.show_loop {
        " loop=\"1\""
    } else {
        ""
    };
    let timings = if settings.manual_advance {
        " useTimings=\"0\""
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentationPr xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:showPr{loop_attr}{timings}><p:present/></p:showPr></p:presentationPr>"
    )
}

/// The theme part: the writer's own theme with the deck's color scheme in place of the default one.
pub(crate) fn theme_xml(name: &str, settings: &DeckSettings) -> String {
    let xml = drawingml_theme_xml(name);
    let Some(scheme) = &settings.color_scheme else {
        return xml;
    };
    let (Some(start), Some(end)) = (xml.find("<a:clrScheme"), xml.find("</a:clrScheme>")) else {
        return xml;
    };
    let mut block = format!(
        "<a:clrScheme name=\"{}\">",
        XmlSanitizer::escape(&scheme.name)
    );
    for ((tag, _), hex) in SLOT_TAGS.iter().zip(&scheme.colors) {
        let _ = write!(block, "<a:{tag}><a:srgbClr val=\"{hex}\"/></a:{tag}>");
    }
    format!("{}{}{}", &xml[..start], block, &xml[end..])
}

/// The deck settings of a package: the repeat flag of `ppt/presProps.xml` and the theme's color scheme when it differs
/// from the one the writer emits by default.
pub(crate) fn read_deck_settings(
    pres_props: Option<&str>,
    theme: Option<&str>,
) -> Result<DeckSettings, EasyOfficeError> {
    let flag = |name: &str, on: [&str; 2]| {
        pres_props
            .and_then(|x| extract_attr(x, "<p:showPr", name))
            .is_some_and(|v| on.contains(&v))
    };
    let show_loop = flag("loop", ["1", "true"]);
    let manual_advance = flag("useTimings", ["0", "false"]);
    let color_scheme = match theme {
        Some(xml) => scheme_of(xml)?,
        None => None,
    };
    Ok(DeckSettings {
        show_loop,
        manual_advance,
        color_scheme,
        ..DeckSettings::default()
    })
}

fn scheme_of(theme_xml: &str) -> Result<Option<ColorScheme>, EasyOfficeError> {
    let palette = PptxThemePalette::from_theme_xml(theme_xml)?;
    let colors: Vec<String> = SLOT_TAGS
        .iter()
        .map(|(_, slot)| rgb_to_hex(palette.slot_rgb(*slot)))
        .collect();
    let name = extract_attr(theme_xml, "<a:clrScheme", "name").unwrap_or("");
    let scheme = ColorScheme::checked(&XmlSanitizer::unescape(name), &colors);
    let default = scheme_of_default();
    Ok(scheme.filter(|s| s.colors != default))
}

/// The twelve colors of the scheme the writer emits when the deck names none.
fn scheme_of_default() -> [String; COLOR_SCHEME_SLOTS] {
    let palette =
        PptxThemePalette::from_theme_xml(&drawingml_theme_xml("EasyOffice")).unwrap_or_default();
    SLOT_TAGS.map(|(_, slot)| rgb_to_hex(palette.slot_rgb(slot)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheme(first: &str) -> ColorScheme {
        let mut colors: Vec<String> = scheme_of_default().to_vec();
        colors[4] = first.to_string();
        ColorScheme::checked("테스트", &colors).expect("twelve colors")
    }

    #[test]
    fn a_color_scheme_goes_through_the_theme_and_back() {
        let settings = DeckSettings {
            show_loop: true,
            manual_advance: true,
            color_scheme: Some(scheme("AA0011")),
            ..DeckSettings::default()
        };
        let theme = theme_xml("EasyOffice", &settings);
        let props = pres_props_xml(&settings);
        let back = read_deck_settings(Some(&props), Some(&theme)).expect("read");
        assert_eq!(back, settings);
        assert!(theme.contains("<a:accent1><a:srgbClr val=\"AA0011\"/></a:accent1>"));
    }

    #[test]
    fn the_default_scheme_is_no_scheme() {
        let none = DeckSettings::default();
        let theme = theme_xml("EasyOffice", &none);
        let back = read_deck_settings(Some(&pres_props_xml(&none)), Some(&theme)).expect("read");
        assert_eq!(back, none);
    }
}
