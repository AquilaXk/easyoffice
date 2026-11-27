//! The defaults of a DOCX (PD-363): `w:docDefaults` merged with the Normal style, theme typefaces
//! resolved against `word/theme/theme1.xml`. A word processor applies them to every paragraph and run that
//! names no value, so a writer has to give them back unchanged.

use crate::rich_defaults::RichDefaults;
use crate::rich_document::{RichAlignment, MAX_PARA_SPACING_TWIPS};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::EasyOfficeError;

/// Largest line height, in 240ths of a line, a default may carry (ten lines).
const MAX_LINE_240THS: u32 = 2_400;
/// Largest body size, in half-points, a default may carry (1638 pt, the format's limit).
const MAX_HALF_POINTS: u16 = 3_276;
const MAX_TAB_STOP_TWIPS: u32 = 31_680;

/// The Latin and East Asian typefaces of the theme, and the script typefaces it lists.
#[derive(Debug, Default)]
pub(super) struct ThemeFonts {
    major: Typefaces,
    minor: Typefaces,
}

#[derive(Debug, Default)]
struct Typefaces {
    latin: Option<String>,
    east_asian: Option<String>,
    /// `a:font script="Hang"` and the like.
    scripts: Vec<(String, String)>,
}

impl ThemeFonts {
    pub(super) fn read(xml: &str) -> Result<Self, EasyOfficeError> {
        let mut theme = Self::default();
        let mut in_major = None;
        for event in XmlReader::new(xml) {
            match event? {
                XmlEvent::Start { name, attrs } => {
                    let face = || attr(&attrs, "typeface").filter(|f| !f.trim().is_empty());
                    match name {
                        "a:majorFont" => in_major = Some(true),
                        "a:minorFont" => in_major = Some(false),
                        "a:latin" | "a:ea" | "a:font" => {
                            let Some(major) = in_major else { continue };
                            let set = if major {
                                &mut theme.major
                            } else {
                                &mut theme.minor
                            };
                            match (name, face()) {
                                ("a:latin", Some(f)) => set.latin = Some(f.into_owned()),
                                ("a:ea", Some(f)) => set.east_asian = Some(f.into_owned()),
                                ("a:font", Some(f)) => {
                                    if let Some(script) = attr(&attrs, "script") {
                                        set.scripts.push((script.into_owned(), f.into_owned()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                XmlEvent::End("a:majorFont" | "a:minorFont") => in_major = None,
                _ => {}
            }
        }
        Ok(theme)
    }

    fn typefaces(&self, theme_name: &str) -> &Typefaces {
        if theme_name.starts_with("major") {
            &self.major
        } else {
            &self.minor
        }
    }

    /// The typeface a `w:asciiTheme`/`w:hAnsiTheme` names.
    fn latin(&self, theme_name: &str) -> Option<&str> {
        self.typefaces(theme_name).latin.as_deref()
    }

    /// The typeface a `w:eastAsiaTheme` names; an empty `a:ea` falls back to the script typeface of
    /// the language (`Hang` for Korean).
    fn east_asian(&self, theme_name: &str, lang: Option<&str>) -> Option<&str> {
        let faces = self.typefaces(theme_name);
        if let Some(face) = faces.east_asian.as_deref() {
            return Some(face);
        }
        let script = match lang?.get(..2)? {
            "ko" => "Hang",
            "ja" => "Jpan",
            "zh" if lang?.contains("TW") || lang?.contains("HK") => "Hant",
            "zh" => "Hans",
            _ => return None,
        };
        faces
            .scripts
            .iter()
            .find(|(s, _)| s == script)
            .map(|(_, f)| f.as_str())
    }
}

/// One layer of properties (`w:docDefaults`, or the Normal style) before merging.
#[derive(Default)]
struct Layer {
    ascii: Option<String>,
    east_asia: Option<String>,
    ascii_theme: Option<String>,
    east_asia_theme: Option<String>,
    lang_east_asia: Option<String>,
    size: Option<u16>,
    jc: Option<Option<RichAlignment>>,
    before: Option<u32>,
    after: Option<u32>,
    line: Option<u32>,
}

impl Layer {
    fn apply(&mut self, name: &str, attrs: &Attrs<'_>) {
        let text = |key: &str| {
            attr(attrs, key)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let number = |key: &str| attr(attrs, key).and_then(|v| v.trim().parse::<i64>().ok());
        match name {
            "w:rFonts" => {
                self.ascii = text("w:ascii")
                    .or_else(|| text("w:hAnsi"))
                    .or(self.ascii.take());
                self.east_asia = text("w:eastAsia").or(self.east_asia.take());
                self.ascii_theme = text("w:asciiTheme")
                    .or_else(|| text("w:hAnsiTheme"))
                    .or(self.ascii_theme.take());
                self.east_asia_theme = text("w:eastAsiaTheme").or(self.east_asia_theme.take());
            }
            "w:lang" => {
                self.lang_east_asia = text("w:eastAsia").or(self.lang_east_asia.take());
            }
            "w:sz" => {
                let size = number("w:val")
                    .and_then(|v| u16::try_from(v).ok())
                    .filter(|v| (1..=MAX_HALF_POINTS).contains(v));
                self.size = size.or(self.size);
            }
            "w:jc" => {
                // `left`, `start`, `right` and the rest parse; an unknown token keeps the layer below
                if let Some(token) = text("w:val") {
                    self.jc = RichAlignment::parse(&token)
                        .map(|a| (a != RichAlignment::Left).then_some(a))
                        .or(self.jc);
                }
            }
            "w:spacing" => {
                let twips = |key: &str| {
                    number(key)
                        .filter(|v| *v >= 0)
                        .and_then(|v| u32::try_from(v).ok())
                        .map(|v| v.min(MAX_PARA_SPACING_TWIPS))
                };
                self.before = twips("w:before").or(self.before);
                self.after = twips("w:after").or(self.after);
                // `exact` and `atLeast` count twips, not 240ths of a line: they are no multiplier
                let multiple = text("w:lineRule").is_none_or(|rule| rule == "auto");
                let line = number("w:line")
                    .filter(|l| *l > 0 && multiple)
                    .and_then(|l| u32::try_from(l).ok())
                    .map(|l| l.min(MAX_LINE_240THS));
                self.line = line.or(self.line);
            }
            _ => {}
        }
    }

    /// `self` over `base`: what `self` names wins.
    fn over(self, base: Self) -> Self {
        Self {
            ascii: self.ascii.or(base.ascii),
            east_asia: self.east_asia.or(base.east_asia),
            ascii_theme: self.ascii_theme.or(base.ascii_theme),
            east_asia_theme: self.east_asia_theme.or(base.east_asia_theme),
            lang_east_asia: self.lang_east_asia.or(base.lang_east_asia),
            size: self.size.or(base.size),
            jc: self.jc.or(base.jc),
            before: self.before.or(base.before),
            after: self.after.or(base.after),
            line: self.line.or(base.line),
        }
    }
}

/// Which element of `word/styles.xml` the reader is inside.
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Outside,
    DocDefaults,
    NormalStyle,
    OtherStyle,
}

/// The defaults of `word/styles.xml` (`None` when the part is missing) with the tab stop of
/// `word/settings.xml`. A property no layer names takes the format's own default.
pub(super) fn read_doc_defaults(
    styles_xml: Option<&str>,
    theme: &ThemeFonts,
    tab_stop_twips: Option<u32>,
) -> Result<RichDefaults, EasyOfficeError> {
    let mut doc_defaults = Layer::default();
    let mut normal = Layer::default();
    let mut scope = Scope::Outside;
    for event in XmlReader::new(styles_xml.unwrap_or_default()) {
        match event? {
            XmlEvent::Start { name, attrs } => match name {
                "w:docDefaults" => scope = Scope::DocDefaults,
                "w:style" => {
                    let is_default = attr(&attrs, "w:default")
                        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "on"));
                    let is_paragraph = attr(&attrs, "w:type").as_deref() == Some("paragraph");
                    scope = if is_default && is_paragraph {
                        Scope::NormalStyle
                    } else {
                        Scope::OtherStyle
                    };
                }
                _ => match scope {
                    Scope::DocDefaults => doc_defaults.apply(name, &attrs),
                    Scope::NormalStyle => normal.apply(name, &attrs),
                    Scope::Outside | Scope::OtherStyle => {}
                },
            },
            XmlEvent::End("w:docDefaults" | "w:style") => scope = Scope::Outside,
            _ => {}
        }
    }
    let layer = normal.over(doc_defaults);
    let fallback = RichDefaults::default();
    Ok(RichDefaults {
        font: layer
            .ascii
            .or_else(|| theme_font(&layer.ascii_theme, |n| theme.latin(n))),
        east_asian_font: layer.east_asia.or_else(|| {
            let lang = layer.lang_east_asia.as_deref();
            theme_font(&layer.east_asia_theme, |n| theme.east_asian(n, lang))
        }),
        size_half_points: layer.size.unwrap_or(fallback.size_half_points),
        alignment: layer.jc.unwrap_or(fallback.alignment),
        space_before_twips: layer.before.unwrap_or(fallback.space_before_twips),
        space_after_twips: layer.after.unwrap_or(fallback.space_after_twips),
        line_240ths: layer.line.unwrap_or(fallback.line_240ths),
        tab_stop_twips: tab_stop_twips
            .filter(|t| (1..=MAX_TAB_STOP_TWIPS).contains(t))
            .unwrap_or(fallback.tab_stop_twips),
    })
}

fn theme_font<'t>(
    theme_name: &Option<String>,
    resolve: impl FnOnce(&str) -> Option<&'t str>,
) -> Option<String> {
    resolve(theme_name.as_deref()?).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: &str = concat!(
        "<w:styles xmlns:w=\"w\"><w:docDefaults><w:rPrDefault><w:rPr>",
        "<w:rFonts w:asciiTheme=\"minorHAnsi\" w:eastAsiaTheme=\"minorEastAsia\"/>",
        "<w:sz w:val=\"22\"/><w:lang w:val=\"en-US\" w:eastAsia=\"ko-KR\"/></w:rPr></w:rPrDefault>",
        "<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/>",
        "</w:pPr></w:pPrDefault></w:docDefaults>",
        "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/>",
        "<w:pPr><w:jc w:val=\"center\"/></w:pPr></w:style></w:styles>"
    );
    const THEME: &str = concat!(
        "<a:theme><a:themeElements><a:fontScheme><a:majorFont><a:latin typeface=\"Calibri Light\"/>",
        "<a:ea typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"Calibri\"/>",
        "<a:ea typeface=\"\"/><a:font script=\"Hang\" typeface=\"맑은 고딕\"/>",
        "<a:font script=\"Jpan\" typeface=\"游明朝\"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"
    );

    #[test]
    fn doc_defaults_and_the_normal_style_merge_with_theme_typefaces() {
        let theme = ThemeFonts::read(THEME).unwrap();
        let d = read_doc_defaults(Some(STYLES), &theme, Some(720)).unwrap();
        assert_eq!(d.font.as_deref(), Some("Calibri"));
        assert_eq!(d.east_asian_font.as_deref(), Some("맑은 고딕"));
        assert_eq!(
            (d.size_half_points, d.space_after_twips, d.line_240ths),
            (22, 160, 259)
        );
        assert_eq!(
            (d.alignment, d.tab_stop_twips),
            (Some(RichAlignment::Center), 720)
        );
    }

    #[test]
    fn a_fixed_line_height_is_no_line_multiplier() {
        let styles = concat!(
            "<w:styles xmlns:w=\"w\"><w:docDefaults><w:pPrDefault><w:pPr>",
            "<w:spacing w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>",
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:pPr>",
            "<w:spacing w:line=\"400\" w:lineRule=\"exact\"/></w:pPr></w:style></w:styles>"
        );
        let d = read_doc_defaults(Some(styles), &ThemeFonts::default(), None).unwrap();
        assert_eq!(d.line_240ths, 276);
    }

    #[test]
    fn a_file_without_styles_gets_the_format_defaults() {
        let d = read_doc_defaults(None, &ThemeFonts::default(), None).unwrap();
        assert_eq!(d, RichDefaults::default());
    }

    #[test]
    fn left_alignment_in_the_normal_style_overrides_justify_below_it() {
        let styles = concat!(
            "<w:styles xmlns:w=\"w\"><w:docDefaults><w:pPrDefault><w:pPr><w:jc w:val=\"both\"/>",
            "</w:pPr></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" ",
            "w:styleId=\"Normal\"><w:pPr><w:jc w:val=\"left\"/></w:pPr></w:style></w:styles>"
        );
        let d = read_doc_defaults(Some(styles), &ThemeFonts::default(), None).unwrap();
        assert_eq!(d.alignment, None);
    }
}
