//! Character-run property writers shared by the DOCX, ODT, and RTF rich writers: size (half
//! points), font family, text color, and highlight color. Every value is validated or escaped for
//! the target syntax; none is ever parsed as markup.

use crate::rich_document::hex_to_highlight_name;
use crate::{RichRun, RichScript, RtfCodec, XmlSanitizer};

// ---------------------------------------------------------------- DOCX

/// `<w:rPr>…</w:rPr>` for a run, or an empty string when the run carries no formatting. Child
/// order follows the CT_RPr schema sequence.
pub(crate) fn docx_run_properties(run: &RichRun) -> String {
    let named_highlight = run.highlight.as_deref().and_then(hex_to_highlight_name);
    let mut xml = String::new();
    if let Some(font) = run.font.as_deref() {
        let f = XmlSanitizer::escape(font);
        xml.push_str(&format!(
            "<w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:eastAsia=\"{f}\" w:cs=\"{f}\"/>"
        ));
    }
    if run.bold {
        xml.push_str("<w:b/>");
    }
    if run.italic {
        xml.push_str("<w:i/>");
    }
    if run.strike {
        xml.push_str("<w:strike/>");
    }
    if let Some(color) = run.color.as_deref() {
        xml.push_str(&format!("<w:color w:val=\"{color}\"/>"));
    }
    if let Some(size) = run.size_half_pt {
        xml.push_str(&format!(
            "<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"
        ));
    }
    if let Some(name) = named_highlight {
        xml.push_str(&format!("<w:highlight w:val=\"{name}\"/>"));
    }
    if run.underline {
        xml.push_str("<w:u w:val=\"single\"/>");
    }
    if let (Some(fill), None) = (run.highlight.as_deref(), named_highlight) {
        xml.push_str(&format!(
            "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/>"
        ));
    }
    match run.script {
        RichScript::Normal => {}
        RichScript::Superscript => xml.push_str("<w:vertAlign w:val=\"superscript\"/>"),
        RichScript::Subscript => xml.push_str("<w:vertAlign w:val=\"subscript\"/>"),
    }
    if xml.is_empty() {
        xml
    } else {
        format!("<w:rPr>{xml}</w:rPr>")
    }
}

// ---------------------------------------------------------------- ODT

/// Automatic text style identity: toggles, size, font, color, highlight, script.
pub(crate) type OdtTextKey = (
    (bool, bool, bool, bool),
    Option<u16>,
    Option<String>,
    Option<String>,
    Option<String>,
    RichScript,
);

pub(crate) fn odt_text_key(run: &RichRun) -> OdtTextKey {
    (
        (run.bold, run.italic, run.underline, run.strike),
        run.size_half_pt,
        run.font.clone(),
        run.color.clone(),
        run.highlight.clone(),
        run.script,
    )
}

pub(crate) fn odt_text_key_is_plain(key: &OdtTextKey) -> bool {
    key.0 == (false, false, false, false)
        && key.1.is_none()
        && key.2.is_none()
        && key.3.is_none()
        && key.4.is_none()
        && key.5 == RichScript::Normal
}

fn odt_points(half_pt: u16) -> String {
    if half_pt % 2 == 0 {
        format!("{}pt", half_pt / 2)
    } else {
        format!("{}.5pt", half_pt / 2)
    }
}

/// `<style:style family="text">` for the `index`-th (1-based) automatic text style.
pub(crate) fn odt_text_style_xml(index: usize, key: &OdtTextKey) -> String {
    let ((bold, italic, underline, strike), size, font, color, highlight, script) = key;
    let mut xml = format!(
        "<style:style style:name=\"T{index}\" style:family=\"text\"><style:text-properties"
    );
    if let Some(size) = size {
        xml.push_str(&format!(" fo:font-size=\"{}\"", odt_points(*size)));
    }
    if let Some(font) = font {
        xml.push_str(&format!(
            " style:font-name=\"{}\"",
            XmlSanitizer::escape(font)
        ));
    }
    if let Some(color) = color {
        xml.push_str(&format!(" fo:color=\"#{color}\""));
    }
    if let Some(highlight) = highlight {
        xml.push_str(&format!(" fo:background-color=\"#{highlight}\""));
    }
    if *bold {
        xml.push_str(" fo:font-weight=\"bold\"");
    }
    if *italic {
        xml.push_str(" fo:font-style=\"italic\"");
    }
    if *underline {
        xml.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
    }
    if *strike {
        xml.push_str(" style:text-line-through-style=\"solid\"");
    }
    match script {
        RichScript::Normal => {}
        RichScript::Superscript => xml.push_str(" style:text-position=\"super 58%\""),
        RichScript::Subscript => xml.push_str(" style:text-position=\"sub 58%\""),
    }
    xml.push_str("/></style:style>");
    xml
}

/// `<office:font-face-decls>` declaring every distinct font used by the text styles.
pub(crate) fn odt_font_face_decls(keys: &[OdtTextKey]) -> String {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut fonts: Vec<&str> = Vec::new();
    for font in keys.iter().filter_map(|k| k.2.as_deref()) {
        if seen.insert(font) {
            fonts.push(font);
        }
    }
    if fonts.is_empty() {
        return String::new();
    }
    let mut xml = String::from("<office:font-face-decls>");
    for font in fonts {
        let f = XmlSanitizer::escape(font);
        xml.push_str(&format!(
            "<style:font-face style:name=\"{f}\" svg:font-family=\"{f}\"/>"
        ));
    }
    xml.push_str("</office:font-face-decls>");
    xml
}

// ---------------------------------------------------------------- RTF

/// Font and color tables of an RTF document plus per-run control-word prefixes.
#[derive(Default)]
pub(crate) struct RtfRunTables {
    fonts: Vec<String>,
    font_map: std::collections::HashMap<String, usize>,
    colors: Vec<String>,
    color_map: std::collections::HashMap<String, usize>,
}

impl RtfRunTables {
    pub(crate) fn collect<'a>(runs: impl Iterator<Item = &'a RichRun>) -> Self {
        let mut t = Self::default();
        for run in runs.filter(|r| !r.text.is_empty()) {
            if let Some(f) = run.font.as_ref() {
                if !t.font_map.contains_key(f) {
                    let idx = t.fonts.len() + 1;
                    t.font_map.insert(f.clone(), idx);
                    t.fonts.push(f.clone());
                }
            }
            for c in [run.color.as_ref(), run.highlight.as_ref()]
                .into_iter()
                .flatten()
            {
                if !t.color_map.contains_key(c) {
                    let idx = t.colors.len() + 1;
                    t.color_map.insert(c.clone(), idx);
                    t.colors.push(c.clone());
                }
            }
        }
        t
    }

    /// `{\fonttbl…}` and `{\colortbl;…}` header lines (none when no run needs them).
    pub(crate) fn header_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.fonts.is_empty() {
            let mut line = String::from("{\\fonttbl{\\f0\\fnil Calibri;}");
            for (i, font) in self.fonts.iter().enumerate() {
                line.push_str(&format!(
                    "{{\\f{}\\fnil {};}}",
                    i + 1,
                    RtfCodec::escape_rtf_text(font)
                ));
            }
            line.push('}');
            lines.push(line);
        }
        if !self.colors.is_empty() {
            let mut line = String::from("{\\colortbl;");
            for hex in &self.colors {
                let n = u32::from_str_radix(hex, 16).unwrap_or(0);
                line.push_str(&format!(
                    "\\red{}\\green{}\\blue{};",
                    (n >> 16) & 0xFF,
                    (n >> 8) & 0xFF,
                    n & 0xFF
                ));
            }
            line.push('}');
            lines.push(line);
        }
        lines
    }

    /// Control words (each followed by a delimiter space) for size, font, color, and highlight.
    pub(crate) fn run_words(&self, run: &RichRun) -> String {
        let mut out = String::new();
        if let Some(font) = &run.font {
            if let Some(&idx) = self.font_map.get(font) {
                out.push_str(&format!("\\f{idx} "));
            }
        }
        if let Some(size) = run.size_half_pt {
            out.push_str(&format!("\\fs{size} "));
        }
        if let Some(color) = &run.color {
            if let Some(&idx) = self.color_map.get(color) {
                out.push_str(&format!("\\cf{idx} "));
            }
        }
        if let Some(hl) = &run.highlight {
            if let Some(&idx) = self.color_map.get(hl) {
                out.push_str(&format!("\\highlight{idx} "));
            }
        }
        out
    }
}

pub(crate) fn has_extra_run_format(run: &RichRun) -> bool {
    run.size_half_pt.is_some()
        || run.font.is_some()
        || run.color.is_some()
        || run.highlight.is_some()
}
