//! The defaults of an ODT (PD-363): the `style:default-style` of the paragraph family merged with
//! the `Standard` paragraph style, which is what a paragraph without a style name gets.

use std::collections::HashMap;

use super::{length_twips, line_height_percent, parse_points};
use crate::rich_defaults::RichDefaults;
use crate::rich_document::{RichAlignment, MAX_PARA_SPACING_TWIPS};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::EasyOfficeError;

const MAX_TAB_STOP_TWIPS: u32 = 31_680;
/// Largest body size, in half-points, a default may carry (1638 pt), as in the DOCX and RTF readers.
const MAX_HALF_POINTS: u16 = 3_276;
/// The style a paragraph without a style name uses.
const STANDARD_STYLE: &str = "Standard";

#[derive(Default)]
struct Layer {
    align: Option<Option<RichAlignment>>,
    before: Option<u32>,
    after: Option<u32>,
    line_percent: Option<u16>,
    tab_stop: Option<u32>,
    font: Option<String>,
    asian_font: Option<String>,
    size: Option<u16>,
}

fn twips(attrs: &Attrs<'_>, key: &str) -> Option<u32> {
    attr(attrs, key)
        .as_deref()
        .and_then(length_twips)
        .and_then(|t| u32::try_from(t).ok())
}

fn font_name(attrs: &Attrs<'_>, keys: [&str; 2]) -> Option<String> {
    keys.into_iter().find_map(|k| {
        attr(attrs, k)
            .map(|f| f.trim().trim_matches('\'').to_string())
            .filter(|f| !f.is_empty())
    })
}

impl Layer {
    fn paragraph(&mut self, attrs: &Attrs<'_>) {
        if let Some(align) = attr(attrs, "fo:text-align")
            .as_deref()
            .and_then(RichAlignment::parse)
        {
            self.align = Some((align != RichAlignment::Left).then_some(align));
        }
        self.before = twips(attrs, "fo:margin-top")
            .map(|t| t.min(MAX_PARA_SPACING_TWIPS))
            .or(self.before);
        self.after = twips(attrs, "fo:margin-bottom")
            .map(|t| t.min(MAX_PARA_SPACING_TWIPS))
            .or(self.after);
        self.line_percent = attr(attrs, "fo:line-height")
            .as_deref()
            .and_then(line_height_percent)
            .or(self.line_percent);
        self.tab_stop = twips(attrs, "style:tab-stop-distance")
            .filter(|t| (1..=MAX_TAB_STOP_TWIPS).contains(t))
            .or(self.tab_stop);
    }

    fn text(&mut self, attrs: &Attrs<'_>) {
        self.font = font_name(attrs, ["style:font-name", "fo:font-family"]).or(self.font.take());
        self.asian_font = font_name(attrs, ["style:font-name-asian", "style:font-family-asian"])
            .or(self.asian_font.take());
        let size = ["fo:font-size", "style:font-size-asian"]
            .into_iter()
            .find_map(|k| {
                attr(attrs, k)
                    .as_deref()
                    .and_then(parse_points)
                    .filter(|s| *s <= MAX_HALF_POINTS)
            });
        self.size = size.or(self.size);
    }

    /// `self` over `base`: what `self` names wins.
    fn over(self, base: Self) -> Self {
        Self {
            align: self.align.or(base.align),
            before: self.before.or(base.before),
            after: self.after.or(base.after),
            line_percent: self.line_percent.or(base.line_percent),
            tab_stop: self.tab_stop.or(base.tab_stop),
            font: self.font.or(base.font),
            asian_font: self.asian_font.or(base.asian_font),
            size: self.size.or(base.size),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Outside,
    Default,
    Standard,
    Other,
}

/// The defaults of `styles.xml` (`None` when the part is missing).
pub(super) fn read_odt_defaults(styles_xml: Option<&str>) -> Result<RichDefaults, EasyOfficeError> {
    let mut faces: HashMap<String, String> = HashMap::new();
    let mut default_style = Layer::default();
    let mut standard = Layer::default();
    let mut scope = Scope::Outside;
    for event in XmlReader::new(styles_xml.unwrap_or_default()) {
        match event? {
            XmlEvent::Start { name, attrs } => match name {
                "style:font-face" => {
                    if let (Some(n), Some(f)) =
                        (attr(&attrs, "style:name"), attr(&attrs, "svg:font-family"))
                    {
                        faces.insert(n.into_owned(), f.trim().trim_matches('\'').to_string());
                    }
                }
                "style:default-style" => {
                    let paragraph = attr(&attrs, "style:family").as_deref() == Some("paragraph");
                    scope = if paragraph {
                        Scope::Default
                    } else {
                        Scope::Other
                    };
                }
                "style:style" => {
                    let standard_paragraph = attr(&attrs, "style:family").as_deref()
                        == Some("paragraph")
                        && attr(&attrs, "style:name").as_deref() == Some(STANDARD_STYLE);
                    scope = if standard_paragraph {
                        Scope::Standard
                    } else {
                        Scope::Other
                    };
                }
                "style:paragraph-properties" | "style:text-properties" => {
                    let layer = match scope {
                        Scope::Default => &mut default_style,
                        Scope::Standard => &mut standard,
                        Scope::Outside | Scope::Other => continue,
                    };
                    if name == "style:paragraph-properties" {
                        layer.paragraph(&attrs);
                    } else {
                        layer.text(&attrs);
                    }
                }
                "office:body" => break,
                _ => {}
            },
            XmlEvent::End("style:default-style" | "style:style") => scope = Scope::Outside,
            _ => {}
        }
    }
    let layer = standard.over(default_style);
    let fallback = RichDefaults::default();
    // A font name in a style refers to a declaration; a name without one is the family itself.
    let family = |name: String| faces.get(&name).cloned().unwrap_or(name);
    Ok(RichDefaults {
        font: layer.font.map(family),
        east_asian_font: layer.asian_font.map(family),
        size_half_points: layer.size.unwrap_or(fallback.size_half_points),
        alignment: layer.align.unwrap_or(fallback.alignment),
        space_before_twips: layer.before.unwrap_or(fallback.space_before_twips),
        space_after_twips: layer.after.unwrap_or(fallback.space_after_twips),
        line_240ths: layer
            .line_percent
            .map_or(fallback.line_240ths, |p| (u32::from(p) * 240 + 50) / 100),
        tab_stop_twips: layer.tab_stop.unwrap_or(fallback.tab_stop_twips),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: &str = concat!(
        "<office:document-styles xmlns:office=\"o\"><office:font-face-decls>",
        "<style:font-face style:name=\"Liberation Serif\" svg:font-family=\"'Liberation Serif'\"/>",
        "<style:font-face style:name=\"NanumGothic\" svg:font-family=\"NanumGothic\"/>",
        "</office:font-face-decls><office:styles><style:default-style style:family=\"paragraph\">",
        "<style:paragraph-properties fo:text-align=\"start\" style:tab-stop-distance=\"0.5in\"/>",
        "<style:text-properties style:font-name=\"Liberation Serif\" fo:font-size=\"12pt\" ",
        "style:font-name-asian=\"NanumGothic\"/></style:default-style>",
        "<style:style style:name=\"Standard\" style:family=\"paragraph\">",
        "<style:paragraph-properties fo:margin-bottom=\"0.1in\" fo:line-height=\"115%\"/></style:style>",
        "<style:style style:name=\"Heading\" style:family=\"paragraph\">",
        "<style:paragraph-properties fo:text-align=\"center\"/></style:style>",
        "</office:styles></office:document-styles>"
    );

    #[test]
    fn the_default_style_and_standard_merge_and_font_names_resolve() {
        let d = read_odt_defaults(Some(STYLES)).unwrap();
        assert_eq!(d.font.as_deref(), Some("Liberation Serif"));
        assert_eq!(d.east_asian_font.as_deref(), Some("NanumGothic"));
        assert_eq!(
            (d.size_half_points, d.space_after_twips, d.line_240ths),
            (24, 144, 276)
        );
        assert_eq!((d.alignment, d.tab_stop_twips), (None, 720));
    }

    #[test]
    fn a_default_size_over_the_cap_is_ignored() {
        let styles = concat!(
            "<office:document-styles xmlns:office=\"o\"><office:styles>",
            "<style:default-style style:family=\"paragraph\">",
            "<style:text-properties fo:font-size=\"2000pt\"/></style:default-style>",
            "</office:styles></office:document-styles>"
        );
        let d = read_odt_defaults(Some(styles)).unwrap();
        assert_eq!(d.size_half_points, RichDefaults::default().size_half_points);
    }

    #[test]
    fn a_file_without_styles_gets_the_format_defaults() {
        assert_eq!(read_odt_defaults(None).unwrap(), RichDefaults::default());
    }
}
