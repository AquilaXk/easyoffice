//! ECMA-376 DrawingML theme palette, color map (`clrMap`/`clrMapOvr`), §20.1.2.3
//! color transforms (`lumMod`, `lumOff`, `tint`, `shade`), and `<p:style>` ref resolution.

use crate::rich_slide::{emu_to_pt, MAX_SLIDE_STROKE_PT};
use crate::sheet_styling::xlsx_color::parse_argb;
use crate::sheet_styling::xlsx_view::DEFAULT_THEME;
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::EasyOfficeError;

const MAX_THEME_XML_EVENTS: usize = 65_536;
const MAX_FMT_STYLE_ENTRIES: usize = 16;
const MAX_COLOR_TRANSFORMS: usize = 16;

#[inline]
fn loc(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

#[inline]
pub(crate) fn rgb_to_hex(rgb: u32) -> String {
    format!("{:06X}", rgb & 0xFF_FFFF)
}

/// Twelve base color slots in `<a:clrScheme>` (ECMA-376 §20.1.6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeSlot {
    Dk1 = 0,
    Lt1 = 1,
    Dk2 = 2,
    Lt2 = 3,
    Accent1 = 4,
    Accent2 = 5,
    Accent3 = 6,
    Accent4 = 7,
    Accent5 = 8,
    Accent6 = 9,
    Hlink = 10,
    FolHlink = 11,
}

impl ThemeSlot {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "dk1" => Some(Self::Dk1),
            "lt1" => Some(Self::Lt1),
            "dk2" => Some(Self::Dk2),
            "lt2" => Some(Self::Lt2),
            "accent1" => Some(Self::Accent1),
            "accent2" => Some(Self::Accent2),
            "accent3" => Some(Self::Accent3),
            "accent4" => Some(Self::Accent4),
            "accent5" => Some(Self::Accent5),
            "accent6" => Some(Self::Accent6),
            "hlink" => Some(Self::Hlink),
            "folHlink" => Some(Self::FolHlink),
            _ => None,
        }
    }
}

const CLR_MAP_KEYS: [&str; 12] = [
    "bg1", "tx1", "bg2", "tx2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6",
    "hlink", "folHlink",
];

/// Mapping from logical presentation slots (`bg1`, `tx1`, `bg2`, `tx2`, `accent1..6`,
/// `hlink`, `folHlink`) to [`ThemeSlot`] entries (`<p:clrMap>` / `<a:overrideClrMapping>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PptxClrMap {
    slots: [ThemeSlot; 12],
}

impl Default for PptxClrMap {
    fn default() -> Self {
        Self {
            slots: [
                ThemeSlot::Lt1,
                ThemeSlot::Dk1,
                ThemeSlot::Lt2,
                ThemeSlot::Dk2,
                ThemeSlot::Accent1,
                ThemeSlot::Accent2,
                ThemeSlot::Accent3,
                ThemeSlot::Accent4,
                ThemeSlot::Accent5,
                ThemeSlot::Accent6,
                ThemeSlot::Hlink,
                ThemeSlot::FolHlink,
            ],
        }
    }
}

impl PptxClrMap {
    pub(crate) fn apply_attrs(&mut self, attrs: &Attrs<'_>) {
        for (idx, key) in CLR_MAP_KEYS.iter().enumerate() {
            if let Some(slot) = attr(attrs, key).and_then(|v| ThemeSlot::parse(v.trim())) {
                self.slots[idx] = slot;
            }
        }
    }

    pub(crate) fn resolve_slot(self, name: &str) -> Option<ThemeSlot> {
        if let Some(idx) = CLR_MAP_KEYS.iter().position(|&k| k == name) {
            return Some(self.slots[idx]);
        }
        ThemeSlot::parse(name)
    }
}

/// DrawingML color modifier in normalized fraction (`100_000 = 1.0`, ECMA-376 §20.1.2.3).
///
/// - `LumMod(f)` / `LumOff(f)`: convert sRGB to HSL (`H in [0, 6)`, `S in [0, 1]`, `L in [0, 1]`),
///   update `L' = L * f` or `L' = L + f` (clamped `[0, 1]` on HSL exit), and convert back to sRGB.
/// - `Tint(t)`: convert sRGB to linear sRGB (`scRGB`) and blend `t` of input color with `(1 - t)`
///   white: `c_lin' = c_lin * t + (1 - t)` (ECMA-376 §20.1.2.3.34).
/// - `Shade(s)`: convert sRGB to linear sRGB (`scRGB`) and scale toward black:
///   `c_lin' = c_lin * s` (ECMA-376 §20.1.2.3.31).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ColorTransform {
    LumMod(f64),
    LumOff(f64),
    Tint(f64),
    Shade(f64),
}

fn parse_ooxml_pct(raw: &str) -> Option<f64> {
    let t = raw.trim();
    let v = if let Some(p) = t.strip_suffix('%') {
        p.trim().parse::<f64>().ok()? / 100.0
    } else {
        t.parse::<f64>().ok()? / 100_000.0
    };
    v.is_finite().then_some(v)
}

#[inline]
fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
fn linear_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn rgb_to_hsl([r, g, b]: [f64; 3]) -> (f64, f64, f64) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) * 0.5;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let denom = 1.0 - (2.0 * l - 1.0).abs();
    let s = if denom <= 0.0 {
        0.0
    } else {
        (d / denom).clamp(0.0, 1.0)
    };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h.rem_euclid(6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c * 0.5;
    [r1 + m, g1 + m, b1 + m]
}

pub(crate) fn apply_color_transforms(rgb: u32, transforms: &[ColorTransform]) -> u32 {
    if transforms.is_empty() {
        return rgb & 0xFF_FFFF;
    }
    let mut c = [
        f64::from((rgb >> 16) & 0xFF) / 255.0,
        f64::from((rgb >> 8) & 0xFF) / 255.0,
        f64::from(rgb & 0xFF) / 255.0,
    ];
    let mut hsl: Option<(f64, f64, f64)> = None;
    for &tr in transforms {
        match tr {
            ColorTransform::LumMod(f) => {
                let (_, _, l) = hsl.get_or_insert_with(|| rgb_to_hsl(c));
                *l *= f;
            }
            ColorTransform::LumOff(f) => {
                let (_, _, l) = hsl.get_or_insert_with(|| rgb_to_hsl(c));
                *l += f;
            }
            ColorTransform::Tint(t) => {
                if let Some((h, s, l)) = hsl.take() {
                    c = hsl_to_rgb(h, s, l.clamp(0.0, 1.0));
                }
                let t = t.clamp(0.0, 1.0);
                c = c.map(|ch| linear_to_srgb(srgb_to_linear(ch) * t + (1.0 - t)));
            }
            ColorTransform::Shade(s) => {
                if let Some((h, st, l)) = hsl.take() {
                    c = hsl_to_rgb(h, st, l.clamp(0.0, 1.0));
                }
                let s = s.clamp(0.0, 1.0);
                c = c.map(|ch| linear_to_srgb(srgb_to_linear(ch) * s));
            }
        }
    }
    if let Some((h, s, l)) = hsl {
        c = hsl_to_rgb(h, s, l.clamp(0.0, 1.0));
    }
    let byte = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u32;
    (byte(c[0]) << 16) | (byte(c[1]) << 8) | byte(c[2])
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ColorBase {
    Srgb(u32),
    Scheme(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColorSpec {
    pub(crate) base: ColorBase,
    pub(crate) transforms: Vec<ColorTransform>,
}

impl ColorSpec {
    pub(crate) fn resolve_rgb(
        &self,
        palette: &PptxThemePalette,
        clr_map: &PptxClrMap,
        ph_clr: Option<u32>,
    ) -> Option<u32> {
        let base = match &self.base {
            ColorBase::Srgb(rgb) => *rgb,
            ColorBase::Scheme(name) => palette.resolve_scheme(clr_map, name, ph_clr)?,
        };
        Some(apply_color_transforms(base, &self.transforms))
    }

    pub(crate) fn resolve_hex(
        &self,
        palette: &PptxThemePalette,
        clr_map: &PptxClrMap,
        ph_clr: Option<u32>,
    ) -> Option<String> {
        self.resolve_rgb(palette, clr_map, ph_clr).map(rgb_to_hex)
    }
}

fn default_sys_clr(val: Option<&str>) -> Option<u32> {
    match val? {
        "windowText" => Some(0x000000),
        "window" => Some(0xFFFFFF),
        _ => None,
    }
}

fn preset_clr(val: &str) -> Option<u32> {
    match val {
        "black" => Some(0x000000),
        "white" => Some(0xFFFFFF),
        "red" => Some(0xFF0000),
        "green" => Some(0x008000),
        "lime" => Some(0x00FF00),
        "blue" => Some(0x0000FF),
        "yellow" => Some(0xFFFF00),
        "cyan" => Some(0x00FFFF),
        "magenta" => Some(0xFF00FF),
        "gray" | "grey" => Some(0x808080),
        "dkGray" | "darkGray" => Some(0xA9A9A9),
        "ltGray" | "lightGray" => Some(0xD3D3D3),
        "silver" => Some(0xC0C0C0),
        "navy" => Some(0x000080),
        "maroon" => Some(0x800000),
        "olive" => Some(0x808000),
        "purple" => Some(0x800080),
        "teal" => Some(0x008080),
        "orange" => Some(0xFFA500),
        _ => None,
    }
}

/// Accumulates `<a:srgbClr>` / `<a:schemeClr>` / `<a:sysClr>` / `<a:prstClr>` and its
/// transform child elements (`lumMod`, `lumOff`, `tint`, `shade`).
#[derive(Debug, Clone)]
pub(crate) struct ColorSpecBuilder {
    spec: Option<ColorSpec>,
}

impl ColorSpecBuilder {
    pub(crate) fn from_start(local_name: &str, attrs: &Attrs<'_>) -> Option<Self> {
        let base = match local_name {
            "srgbClr" => attr(attrs, "val")
                .and_then(|v| parse_argb(&v))
                .map(ColorBase::Srgb),
            "schemeClr" => attr(attrs, "val").map(|v| ColorBase::Scheme(v.trim().to_string())),
            "sysClr" => attr(attrs, "lastClr")
                .and_then(|v| parse_argb(&v))
                .or_else(|| default_sys_clr(attr(attrs, "val").as_deref()))
                .map(ColorBase::Srgb),
            "prstClr" => attr(attrs, "val")
                .and_then(|v| preset_clr(v.trim()))
                .map(ColorBase::Srgb),
            _ => return None,
        };
        Some(Self {
            spec: base.map(|b| ColorSpec {
                base: b,
                transforms: Vec::new(),
            }),
        })
    }

    pub(crate) fn push_transform(&mut self, local_name: &str, attrs: &Attrs<'_>) {
        let Some(spec) = self.spec.as_mut() else {
            return;
        };
        if spec.transforms.len() >= MAX_COLOR_TRANSFORMS {
            return;
        }
        let Some(val) = attr(attrs, "val").and_then(|v| parse_ooxml_pct(&v)) else {
            return;
        };
        let tr = match local_name {
            "lumMod" => ColorTransform::LumMod(val),
            "lumOff" => ColorTransform::LumOff(val),
            "tint" => ColorTransform::Tint(val),
            "shade" => ColorTransform::Shade(val),
            _ => return,
        };
        spec.transforms.push(tr);
    }

    pub(crate) fn finish(self) -> Option<ColorSpec> {
        self.spec
    }
}

#[derive(Debug, Clone)]
enum FmtFillEntry {
    NoFill,
    Color(Option<ColorSpec>),
}

#[derive(Debug, Clone)]
struct FmtLineEntry {
    width_pt: Option<f64>,
    fill: FmtFillEntry,
}

/// Parsed DrawingML theme (`a:clrScheme` + optional `a:fmtScheme` fill/line style lists).
#[derive(Debug, Clone)]
pub(crate) struct PptxThemePalette {
    colors: [u32; 12],
    fill_styles: Vec<FmtFillEntry>,
    bg_fill_styles: Vec<FmtFillEntry>,
    line_styles: Vec<FmtLineEntry>,
}

impl Default for PptxThemePalette {
    fn default() -> Self {
        let mut colors = DEFAULT_THEME;
        colors.swap(0, 1);
        colors.swap(2, 3);
        Self {
            colors,
            fill_styles: Vec::new(),
            bg_fill_styles: Vec::new(),
            line_styles: Vec::new(),
        }
    }
}

impl PptxThemePalette {
    pub(crate) fn from_theme_xml(xml: &str) -> Result<Self, EasyOfficeError> {
        let mut palette = Self::default();
        let mut reader = XmlReader::with_budget(xml, MAX_THEME_XML_EVENTS);
        let (mut d, mut scheme_d, mut cur_slot) = (0usize, None::<usize>, None::<ThemeSlot>);
        let mut scheme_done = false;
        let (mut fmt_d, mut list_d, mut list_kind, mut entry_d) =
            (None::<usize>, None::<usize>, 0u8, None::<usize>);
        let (mut ln_w, mut entry_no_fill, mut entry_spec) = (None, false, None);
        let (mut color_d, mut pending_color) = (None::<usize>, None::<ColorSpecBuilder>);

        while let Some(ev) = reader.next_event()? {
            match ev {
                XmlEvent::Start { name, attrs } => {
                    d += 1;
                    let ln = loc(name);
                    if !scheme_done && scheme_d.is_none() && ln == "clrScheme" {
                        scheme_d = Some(d);
                    } else if scheme_d.is_some_and(|sd| d == sd + 1) {
                        cur_slot = ThemeSlot::parse(ln);
                    } else if cur_slot.is_some() && scheme_d.is_some_and(|sd| d == sd + 2) {
                        let rgb = match ln {
                            "srgbClr" => attr(&attrs, "val").and_then(|v| parse_argb(&v)),
                            "sysClr" => attr(&attrs, "lastClr")
                                .and_then(|v| parse_argb(&v))
                                .or_else(|| default_sys_clr(attr(&attrs, "val").as_deref())),
                            _ => None,
                        };
                        if let (Some(slot), Some(rgb)) = (cur_slot, rgb) {
                            palette.colors[slot as usize] = rgb;
                        }
                    } else if fmt_d.is_none() && ln == "fmtScheme" {
                        fmt_d = Some(d);
                    } else if fmt_d.is_some_and(|fd| d == fd + 1) {
                        list_kind = match ln {
                            "fillStyleLst" => 1,
                            "lnStyleLst" => 2,
                            "bgFillStyleLst" => 3,
                            _ => 0,
                        };
                        if list_kind != 0 {
                            list_d = Some(d);
                        }
                    } else if list_d.is_some_and(|ld| d == ld + 1) {
                        entry_d = Some(d);
                        entry_no_fill = ln == "noFill";
                        entry_spec = None;
                        ln_w = if list_kind == 2 && ln == "ln" {
                            attr(&attrs, "w")
                                .and_then(|s| s.trim().parse::<f64>().ok())
                                .filter(|w| w.is_finite())
                                .map(|w| emu_to_pt(w).clamp(0.0, MAX_SLIDE_STROKE_PT))
                        } else {
                            None
                        };
                    } else if entry_d.is_some() {
                        if list_kind == 2 && entry_d.is_some_and(|ed| d == ed + 1) && ln == "noFill"
                        {
                            entry_no_fill = true;
                        }
                        if color_d.is_none() && entry_spec.is_none() {
                            if let Some(b) = ColorSpecBuilder::from_start(ln, &attrs) {
                                color_d = Some(d);
                                pending_color = Some(b);
                            }
                        } else if color_d.is_some_and(|cd| d == cd + 1) {
                            if let Some(b) = pending_color.as_mut() {
                                b.push_transform(ln, &attrs);
                            }
                        }
                    }
                }
                XmlEvent::End(name) => {
                    let ln = loc(name);
                    if color_d == Some(d) {
                        color_d = None;
                        if let Some(b) = pending_color.take() {
                            entry_spec = b.finish();
                        }
                    }
                    if entry_d == Some(d) {
                        entry_d = None;
                        let fill = if entry_no_fill {
                            FmtFillEntry::NoFill
                        } else {
                            FmtFillEntry::Color(entry_spec.take())
                        };
                        match list_kind {
                            1 if palette.fill_styles.len() < MAX_FMT_STYLE_ENTRIES => {
                                palette.fill_styles.push(fill);
                            }
                            2 if palette.line_styles.len() < MAX_FMT_STYLE_ENTRIES => {
                                palette.line_styles.push(FmtLineEntry {
                                    width_pt: ln_w.take(),
                                    fill,
                                });
                            }
                            3 if palette.bg_fill_styles.len() < MAX_FMT_STYLE_ENTRIES => {
                                palette.bg_fill_styles.push(fill);
                            }
                            _ => {}
                        }
                    }
                    if list_d == Some(d) {
                        list_d = None;
                        list_kind = 0;
                    }
                    if fmt_d == Some(d) {
                        fmt_d = None;
                    }
                    if scheme_d.is_some_and(|sd| d == sd + 1) && ThemeSlot::parse(ln).is_some() {
                        cur_slot = None;
                    }
                    if scheme_d == Some(d) {
                        scheme_d = None;
                        scheme_done = true;
                    }
                    d = d.saturating_sub(1);
                }
                _ => {}
            }
        }
        Ok(palette)
    }

    pub(crate) fn slot_rgb(&self, slot: ThemeSlot) -> u32 {
        self.colors[slot as usize]
    }

    pub(crate) fn resolve_scheme(
        &self,
        clr_map: &PptxClrMap,
        name: &str,
        ph_clr: Option<u32>,
    ) -> Option<u32> {
        if name == "phClr" {
            return ph_clr;
        }
        clr_map.resolve_slot(name).map(|s| self.slot_rgb(s))
    }

    pub(crate) fn resolve_fill_ref(
        &self,
        clr_map: &PptxClrMap,
        idx: u32,
        child: Option<&ColorSpec>,
    ) -> Option<String> {
        if idx == 0 {
            return None;
        }
        let ph_rgb = child.and_then(|c| c.resolve_rgb(self, clr_map, None));
        let entry = if (1..=1000).contains(&idx) {
            self.fill_styles.get((idx - 1) as usize)
        } else if idx >= 1001 {
            self.bg_fill_styles.get((idx - 1001) as usize)
        } else {
            None
        };
        match entry {
            Some(FmtFillEntry::NoFill) => None,
            Some(FmtFillEntry::Color(Some(spec))) => spec.resolve_hex(self, clr_map, ph_rgb),
            Some(FmtFillEntry::Color(None)) | None => ph_rgb.map(rgb_to_hex),
        }
    }

    pub(crate) fn resolve_line_ref(
        &self,
        clr_map: &PptxClrMap,
        idx: u32,
        child: Option<&ColorSpec>,
    ) -> (Option<f64>, Option<String>) {
        if idx == 0 {
            return (None, None);
        }
        let ph_rgb = child.and_then(|c| c.resolve_rgb(self, clr_map, None));
        let Some(entry) = self.line_styles.get((idx - 1) as usize) else {
            return (None, ph_rgb.map(rgb_to_hex));
        };
        let color = match &entry.fill {
            FmtFillEntry::NoFill => None,
            FmtFillEntry::Color(Some(spec)) => spec.resolve_hex(self, clr_map, ph_rgb),
            FmtFillEntry::Color(None) => ph_rgb.map(rgb_to_hex),
        };
        (entry.width_pt, color)
    }
}

/// Captured `<p:style>` reference attributes and child colors for a `<p:sp>` or `<p:cxnSp>`.
#[derive(Default)]
pub(crate) struct StyleRefs {
    pub(crate) fill_idx: Option<u32>,
    pub(crate) fill_color: Option<ColorSpec>,
    pub(crate) ln_idx: Option<u32>,
    pub(crate) ln_color: Option<ColorSpec>,
    pub(crate) font_active: bool,
    pub(crate) font_color: Option<ColorSpec>,
}

impl StyleRefs {
    pub(crate) fn resolve_fill(
        &self,
        palette: &PptxThemePalette,
        clr_map: &PptxClrMap,
    ) -> Option<String> {
        palette.resolve_fill_ref(clr_map, self.fill_idx?, self.fill_color.as_ref())
    }

    pub(crate) fn resolve_line(
        &self,
        palette: &PptxThemePalette,
        clr_map: &PptxClrMap,
    ) -> (Option<f64>, Option<String>) {
        let Some(idx) = self.ln_idx else {
            return (None, None);
        };
        palette.resolve_line_ref(clr_map, idx, self.ln_color.as_ref())
    }

    pub(crate) fn resolve_font_color(
        &self,
        palette: &PptxThemePalette,
        clr_map: &PptxClrMap,
    ) -> Option<String> {
        if !self.font_active {
            return None;
        }
        self.font_color
            .as_ref()
            .and_then(|c| c.resolve_hex(palette, clr_map, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_theme_clr_scheme_ignoring_extra_and_resolves_transforms() {
        let theme_xml = concat!(
            "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
            "<a:themeElements><a:clrScheme name=\"T\">",
            "<a:dk1><a:sysClr val=\"windowText\" lastClr=\"111111\"/></a:dk1>",
            "<a:lt1><a:sysClr val=\"window\" lastClr=\"FEFEFE\"/></a:lt1>",
            "<a:dk2><a:srgbClr val=\"222222\"/></a:dk2>",
            "<a:lt2><a:srgbClr val=\"EEEEEE\"/></a:lt2>",
            "<a:accent1><a:srgbClr val=\"4472C4\"/></a:accent1>",
            "</a:clrScheme></a:themeElements>",
            "<a:extraClrSchemeLst><a:extraClrScheme><a:clrScheme name=\"Alt\">",
            "<a:accent1><a:srgbClr val=\"FF00FF\"/></a:accent1>",
            "</a:clrScheme></a:extraClrScheme></a:extraClrSchemeLst>",
            "</a:theme>"
        );
        let pal = PptxThemePalette::from_theme_xml(theme_xml).unwrap();
        let map = PptxClrMap::default();
        assert_eq!(pal.resolve_scheme(&map, "tx1", None), Some(0x111111));
        assert_eq!(pal.resolve_scheme(&map, "bg1", None), Some(0xFEFEFE));
        assert_eq!(pal.resolve_scheme(&map, "accent1", None), Some(0x4472C4));

        // tint/shade identity at 100_000, pure white at tint=0, pure black at shade=0
        assert_eq!(
            apply_color_transforms(0x4472C4, &[ColorTransform::Tint(1.0)]),
            0x4472C4
        );
        assert_eq!(
            apply_color_transforms(0x4472C4, &[ColorTransform::Tint(0.0)]),
            0xFFFFFF
        );
        assert_eq!(
            apply_color_transforms(0x4472C4, &[ColorTransform::Shade(0.0)]),
            0x000000
        );
        // black + lumMod(0.5) + lumOff(0.5) -> mid-gray 0x808080
        assert_eq!(
            apply_color_transforms(
                0x000000,
                &[ColorTransform::LumMod(0.5), ColorTransform::LumOff(0.5)]
            ),
            0x808080
        );
    }

    #[test]
    fn resolves_style_refs_with_ph_clr_and_zero_idx() {
        let theme_xml = concat!(
            "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
            "<a:themeElements><a:clrScheme name=\"T\">",
            "<a:accent1><a:srgbClr val=\"4472C4\"/></a:accent1>",
            "</a:clrScheme><a:fmtScheme name=\"T\">",
            "<a:fillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:fillStyleLst>",
            "<a:lnStyleLst><a:ln w=\"25400\"><a:solidFill><a:schemeClr val=\"phClr\"><a:shade val=\"50000\"/></a:schemeClr></a:solidFill></a:ln></a:lnStyleLst>",
            "</a:fmtScheme></a:themeElements></a:theme>"
        );
        let pal = PptxThemePalette::from_theme_xml(theme_xml).unwrap();
        let map = PptxClrMap::default();
        let child = ColorSpec {
            base: ColorBase::Scheme("accent1".into()),
            transforms: vec![],
        };
        assert_eq!(
            pal.resolve_fill_ref(&map, 0, Some(&child)),
            None,
            "idx=0 means no fill"
        );
        assert_eq!(
            pal.resolve_fill_ref(&map, 1, Some(&child)).as_deref(),
            Some("4472C4")
        );
        let (lw, lc) = pal.resolve_line_ref(&map, 1, Some(&child));
        assert_eq!(lw, Some(2.0));
        assert!(lc.is_some());
    }
}
