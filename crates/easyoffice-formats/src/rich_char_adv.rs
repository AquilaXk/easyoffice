//! Advanced character settings of the word processor's Font dialog (PD-372): the width scale of the
//! glyphs, the raised or lowered position and the kerning threshold. Each is `None` when the run
//! inherits (scale 100 %, position on the baseline, kerning off).
//!
//! Units follow DOCX and RTF (percent, half-points, half-points). ODT and HWPX keep the position as
//! a percent of the font size and keep kerning as a switch only, so those two convert with the run's
//! size and read the switch back as [`DEFAULT_KERNING_HALF_PT`]. HWPX also limits the scale to 10-200 %.

use crate::xml_stream::{attr, Attrs};

/// Largest width scale in percent (`w:w` is 1 to 600).
pub const MAX_SCALE_PERCENT: u16 = 600;

/// Largest raise or lowering in half-points either way (1,584 pt, the dialog's limit).
pub const MAX_POSITION_HALF_PT: i16 = 3_168;

/// Largest kerning threshold in half-points (1,638 pt, the dialog's limit).
pub const MAX_KERNING_HALF_PT: u16 = 3_276;

/// Threshold a switch-only format (ODT `style:letter-kerning`, HWPX `useKerning`) reads back as: 1 pt.
pub const DEFAULT_KERNING_HALF_PT: u16 = 2;

/// Range of the HWPX character width ratio (`hh:ratio`) in percent.
const HWPX_RATIO_RANGE: (u16, u16) = (10, 200);
/// Range of the HWPX relative position (`hh:offset`) in percent of the size either way.
const HWPX_MAX_OFFSET_PERCENT: i64 = 100;
/// Size a position is measured against when a run names none, in half-points (10 pt, as HWPX spacing does).
const DEFAULT_SIZE_HALF_PT: u16 = 20;
const TWIPS_PER_HALF_POINT: i64 = 10;
/// Largest per-mille a file's position is read at (1000 times the size) so the arithmetic cannot overflow.
const MAX_PERMILLE: i64 = 1_000_000;

/// Scale, position and kerning of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CharAdvanced {
    /// Width of the glyphs in percent of normal (1 to 600, never 100).
    pub scale_percent: Option<u16>,
    /// Raise (positive) or lowering (negative) from the baseline, in half-points (never 0).
    pub position_half_pt: Option<i16>,
    /// Smallest font size, in half-points, that is kerned (1 to 3,276); `None` is no kerning.
    pub kerning_half_pt: Option<u16>,
}

/// A scale from `percent`: zero and below is none, above [`MAX_SCALE_PERCENT`] is clamped, 100 is none.
pub fn char_scale(percent: i64) -> Option<u16> {
    u16::try_from(percent.min(i64::from(MAX_SCALE_PERCENT)))
        .ok()
        .filter(|p| *p > 0 && *p != 100)
}

/// A position from `half_points`, clamped to [`MAX_POSITION_HALF_PT`] either way; zero is none.
pub fn char_position(half_points: i64) -> Option<i16> {
    let max = i64::from(MAX_POSITION_HALF_PT);
    i16::try_from(half_points.clamp(-max, max))
        .ok()
        .filter(|p| *p != 0)
}

/// A kerning threshold from `half_points`: zero and below is off, above [`MAX_KERNING_HALF_PT`] is clamped.
pub fn char_kerning(half_points: i64) -> Option<u16> {
    u16::try_from(half_points.min(i64::from(MAX_KERNING_HALF_PT)))
        .ok()
        .filter(|k| *k > 0)
}

/// The size a position is measured against, in twips: the run's size or the default.
fn size_twips(size_half_pt: Option<u16>) -> i64 {
    i64::from(size_half_pt.unwrap_or(DEFAULT_SIZE_HALF_PT)).max(1) * TWIPS_PER_HALF_POINT
}

/// A position in per-mille of the font size, rounded to the nearest (ODT percent in tenths, HWPX percent in tens).
fn position_to_permille(half_pt: i16, size_half_pt: Option<u16>) -> i64 {
    let size = size_twips(size_half_pt);
    let scaled = i64::from(half_pt) * TWIPS_PER_HALF_POINT * 1000;
    (scaled + scaled.signum() * size / 2) / size
}

/// A position from per-mille of the font size, rounded to the nearest half-point and clamped.
fn position_from_permille(permille: i64, size_half_pt: Option<u16>) -> Option<i16> {
    let scaled = permille.clamp(-MAX_PERMILLE, MAX_PERMILLE) * size_twips(size_half_pt);
    let unit = 1000 * TWIPS_PER_HALF_POINT;
    char_position((scaled + scaled.signum() * unit / 2) / unit)
}

/// An ODT percent (`150%`, `-33.5%`) as a number; `None` for another unit or a number that is not finite.
fn odt_percent(token: &str) -> Option<f64> {
    let n: f64 = token.trim().strip_suffix('%')?.trim().parse().ok()?;
    n.is_finite().then_some(n)
}

/// A `style:text-position` that moves the text without shrinking it (`20%`, `-20% 100%`), in percent.
/// A value that also sets another size (`33% 58%`) or a keyword (`super 58%`) is a script instead.
pub(crate) fn odt_plain_position(value: &str) -> Option<f64> {
    let mut parts = value.split_whitespace();
    let position = odt_percent(parts.next()?)?;
    let keeps_size = parts.next().is_none_or(|s| odt_percent(s) == Some(100.0));
    (keeps_size && parts.next().is_none()).then_some(position)
}

impl CharAdvanced {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// Appends the settings as `,"key":value` pairs of a run's JSON object (`scalePercent`, `positionHalfPt`,
    /// `kerningHalfPt`), none for a setting that is not set.
    pub fn write_json(&self, out: &mut String) {
        let pairs = [
            ("scalePercent", self.scale_percent.map(i64::from)),
            ("positionHalfPt", self.position_half_pt.map(i64::from)),
            ("kerningHalfPt", self.kerning_half_pt.map(i64::from)),
        ];
        for (key, value) in pairs.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))) {
            out.push_str(&format!(",\"{key}\":{value}"));
        }
    }

    /// The settings of `self`, each filled from `inherited` where `self` names none.
    pub(crate) fn or(self, inherited: Self) -> Self {
        Self {
            scale_percent: self.scale_percent.or(inherited.scale_percent),
            position_half_pt: self.position_half_pt.or(inherited.position_half_pt),
            kerning_half_pt: self.kerning_half_pt.or(inherited.kerning_half_pt),
        }
    }

    /// Applies the RTF control word `word` (`\charscalex`, `\up`, `\dn`, `\kerning`); any other is ignored.
    /// `\up` and `\dn` without a value mean 6 half-points, and the value 0 puts the text back on the baseline.
    pub(crate) fn apply_rtf_word(&mut self, word: &str, param: Option<i32>) {
        let lift = |sign: i64| char_position(sign * i64::from(param.unwrap_or(6)));
        match word {
            "charscalex" => self.scale_percent = param.and_then(|v| char_scale(i64::from(v))),
            "kerning" => self.kerning_half_pt = param.and_then(|v| char_kerning(i64::from(v))),
            "up" => self.position_half_pt = lift(1),
            "dn" => self.position_half_pt = lift(-1),
            _ => {}
        }
    }

    /// The RTF control words of the settings, each followed by a space.
    pub(crate) fn rtf_words(&self) -> String {
        let mut out = String::new();
        if let Some(scale) = self.scale_percent {
            out.push_str(&format!("\\charscalex{scale} "));
        }
        match self.position_half_pt {
            Some(up) if up > 0 => out.push_str(&format!("\\up{up} ")),
            Some(down) => out.push_str(&format!("\\dn{} ", -i32::from(down))),
            None => {}
        }
        if let Some(kerning) = self.kerning_half_pt {
            out.push_str(&format!("\\kerning{kerning} "));
        }
        out
    }

    /// The DOCX run properties `w:w`, `w:kern` and `w:position`, in the order the schema lists them.
    pub(crate) fn docx_xml(&self) -> String {
        let mut xml = String::new();
        if let Some(scale) = self.scale_percent {
            xml.push_str(&format!("<w:w w:val=\"{scale}\"/>"));
        }
        if let Some(kerning) = self.kerning_half_pt {
            xml.push_str(&format!("<w:kern w:val=\"{kerning}\"/>"));
        }
        if let Some(position) = self.position_half_pt {
            xml.push_str(&format!("<w:position w:val=\"{position}\"/>"));
        }
        xml
    }

    /// Applies a DOCX run property (`w:w`, `w:kern`, `w:position`) with its `w:val`; any other is ignored.
    pub(crate) fn apply_docx_property(&mut self, name: &str, val: Option<&str>) {
        let number = val.and_then(|v| v.trim().parse::<i64>().ok());
        match name {
            "w:w" => self.scale_percent = number.and_then(char_scale),
            "w:kern" => self.kerning_half_pt = number.and_then(char_kerning),
            "w:position" => self.position_half_pt = number.and_then(char_position),
            _ => {}
        }
    }

    /// The settings of an ODT `style:text-properties` element; `size_half_pt` is the size the position
    /// is measured against.
    pub(crate) fn from_odt(attrs: &Attrs<'_>, size_half_pt: Option<u16>) -> Self {
        let value = |key: &str| attr(attrs, key);
        Self {
            scale_percent: value("style:text-scale")
                .as_deref()
                .and_then(odt_percent)
                .and_then(|p| char_scale(p.round() as i64)),
            position_half_pt: value("style:text-position")
                .as_deref()
                .and_then(odt_plain_position)
                .and_then(|p| position_from_permille((p * 10.0).round() as i64, size_half_pt)),
            kerning_half_pt: matches!(value("style:letter-kerning").as_deref(), Some("true" | "1"))
                .then_some(DEFAULT_KERNING_HALF_PT),
        }
    }

    /// ODT `style:text-properties` attributes (each with a leading space). The position is left out when
    /// `keep_position` is false, because the run's script already writes `style:text-position`.
    pub(crate) fn odt_attrs(&self, size_half_pt: Option<u16>, keep_position: bool) -> String {
        let mut xml = String::new();
        if let Some(scale) = self.scale_percent {
            xml.push_str(&format!(" style:text-scale=\"{scale}%\""));
        }
        if let Some(position) = self.position_half_pt.filter(|_| keep_position) {
            let percent = position_to_permille(position, size_half_pt) as f64 / 10.0;
            xml.push_str(&format!(" style:text-position=\"{percent}% 100%\""));
        }
        if self.kerning_half_pt.is_some() {
            xml.push_str(" style:letter-kerning=\"true\"");
        }
        xml
    }

    /// HWPX `hh:ratio` (10 to 200 %), `hh:offset` (-100 to 100 % of the size) and `useKerning`.
    pub(crate) fn hwpx_values(&self, size_half_pt: Option<u16>) -> (u16, i8, bool) {
        let (low, high) = HWPX_RATIO_RANGE;
        let ratio = self.scale_percent.map_or(100, |s| s.clamp(low, high));
        let offset = self.position_half_pt.map_or(0, |p| {
            let permille = position_to_permille(p, size_half_pt);
            ((permille + permille.signum() * 5) / 10)
                .clamp(-HWPX_MAX_OFFSET_PERCENT, HWPX_MAX_OFFSET_PERCENT) as i8
        });
        (ratio, offset, self.kerning_half_pt.is_some())
    }

    /// The scale of an HWPX `hh:ratio` percent.
    pub(crate) fn hwpx_scale(percent: Option<i64>) -> Option<u16> {
        percent.and_then(char_scale)
    }

    /// The position of an HWPX `hh:offset` percent of `size_half_pt`.
    pub(crate) fn hwpx_position(percent: Option<i64>, size_half_pt: Option<u16>) -> Option<i16> {
        let percent = percent?.clamp(-HWPX_MAX_OFFSET_PERCENT, HWPX_MAX_OFFSET_PERCENT);
        position_from_permille(percent * 10, size_half_pt)
    }

    /// The kerning of an HWPX `useKerning` value.
    pub(crate) fn hwpx_kerning(value: Option<&str>) -> Option<u16> {
        matches!(value, Some("1" | "true")).then_some(DEFAULT_KERNING_HALF_PT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_clamp_and_drop_the_neutral_value() {
        assert_eq!(char_scale(150), Some(150));
        assert_eq!(char_scale(100), None);
        assert_eq!(char_scale(0), None);
        assert_eq!(char_scale(-5), None);
        assert_eq!(char_scale(i64::MAX), Some(MAX_SCALE_PERCENT));
        assert_eq!(char_scale(i64::MIN), None);
        assert_eq!(char_position(6), Some(6));
        assert_eq!(char_position(0), None);
        assert_eq!(char_position(i64::MAX), Some(MAX_POSITION_HALF_PT));
        assert_eq!(char_position(i64::MIN), Some(-MAX_POSITION_HALF_PT));
        assert_eq!(char_kerning(2), Some(2));
        assert_eq!(char_kerning(0), None);
        assert_eq!(char_kerning(-1), None);
        assert_eq!(char_kerning(i64::MAX), Some(MAX_KERNING_HALF_PT));
    }

    #[test]
    fn rtf_words_read_back_and_default_the_lift_to_six_half_points() {
        let mut adv = CharAdvanced::default();
        adv.apply_rtf_word("charscalex", Some(150));
        adv.apply_rtf_word("dn", None);
        adv.apply_rtf_word("kerning", Some(24));
        adv.apply_rtf_word("fs", Some(24));
        assert_eq!(adv.rtf_words(), "\\charscalex150 \\dn6 \\kerning24 ");
        adv.apply_rtf_word("dn", Some(0));
        assert_eq!(adv.position_half_pt, None);
    }

    #[test]
    fn position_converts_to_and_from_the_size_percent() {
        assert_eq!(position_to_permille(6, Some(24)), 250, "3 pt on 12 pt");
        assert_eq!(position_from_permille(250, Some(24)), Some(6));
        assert_eq!(position_from_permille(-250, Some(24)), Some(-6));
        assert_eq!(
            position_to_permille(6, None),
            300,
            "3 pt on the 10 pt default"
        );
        assert_eq!(
            position_from_permille(i64::MAX, Some(24)),
            Some(MAX_POSITION_HALF_PT)
        );
        assert_eq!(
            position_from_permille(1, Some(0)),
            None,
            "a zero size does not divide"
        );
    }

    #[test]
    fn odt_positions_that_resize_the_text_are_scripts() {
        assert_eq!(odt_plain_position("20%"), Some(20.0));
        assert_eq!(odt_plain_position("-20% 100%"), Some(-20.0));
        for script in [
            "33% 58%",
            "super 58%",
            "sub",
            "20% 100% 5%",
            "",
            "NaN%",
            "1e999%",
        ] {
            assert_eq!(odt_plain_position(script), None, "{script}");
        }
    }

    #[test]
    fn hwpx_values_clamp_to_the_hwpx_ranges() {
        let adv = |scale, position, kerning| CharAdvanced {
            scale_percent: scale,
            position_half_pt: position,
            kerning_half_pt: kerning,
        };
        assert_eq!(adv(None, None, None).hwpx_values(Some(24)), (100, 0, false));
        assert_eq!(adv(Some(5), None, None).hwpx_values(Some(24)).0, 10);
        assert_eq!(adv(Some(600), None, None).hwpx_values(Some(24)).0, 200);
        assert_eq!(
            adv(None, Some(MAX_POSITION_HALF_PT), Some(2))
                .hwpx_values(Some(24))
                .1,
            100
        );
        assert_eq!(adv(None, Some(-6), None).hwpx_values(Some(24)).1, -25);
        assert_eq!(
            CharAdvanced::hwpx_position(Some(i64::MIN), Some(24)),
            Some(-24)
        );
        assert_eq!(
            CharAdvanced::hwpx_kerning(Some("true")),
            Some(DEFAULT_KERNING_HALF_PT)
        );
        assert_eq!(CharAdvanced::hwpx_kerning(Some("0")), None);
    }
}
