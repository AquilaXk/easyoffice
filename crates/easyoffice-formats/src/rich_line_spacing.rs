//! Line spacing of a paragraph: the multiple the formats always shared plus the at-least, exact
//! and between-lines rules the word processors offer next to it (PD-366).

/// Largest line spacing multiple a document keeps, in percent of single spacing (1000 = 10 lines).
pub const MAX_LINE_PERCENT: u16 = 1000;
/// Largest at-least, exact or between-lines spacing a document keeps, in twips (1,584 pt, the
/// largest value the DOCX paragraph dialog takes).
pub const MAX_LINE_SPACING_TWIPS: u16 = 31_680;
/// Twips of one single line, the scale that turns a length into a multiple for the formats that
/// know no between-lines spacing (12 pt, as the readers have always scaled absolute lengths).
const SINGLE_LINE_TWIPS: u32 = 240;

/// How a paragraph's lines are spaced (Pages 줄/최소/고정/줄 사이, Word 배수/최소/고정, Hangul
/// 글자에 따라/최소/고정/여백만 지정).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RichLineSpacing {
    /// A multiple of the single line, in percent (`200` = double). DOCX `auto`, ODT `fo:line-height="N%"`,
    /// RTF `\slmult1`, HWPX `PERCENT`, Pages mode 0.
    Multiple(u16),
    /// At least this many twips per line. DOCX `atLeast`, ODT `style:line-height-at-least`, RTF
    /// `\sl` with `\slmult0`, HWPX `AT_LEAST`, Pages mode 1.
    AtLeast(u16),
    /// Exactly this many twips per line. DOCX `exact`, ODT `fo:line-height` as a length, RTF
    /// negative `\sl`, HWPX `FIXED`, Pages mode 2.
    Exact(u16),
    /// This many twips of space between lines on top of the single line. ODT `style:line-spacing`,
    /// HWPX `BETWEEN_LINES`, Pages mode 4. DOCX and RTF have no such rule: they write the equal
    /// multiple ([`Self::multiple_percent`]).
    Between(u16),
}

impl RichLineSpacing {
    /// A multiple in `1..=`[`MAX_LINE_PERCENT`], else `None`.
    pub fn multiple(percent: i64) -> Option<Self> {
        u16::try_from(percent)
            .ok()
            .filter(|p| (1..=MAX_LINE_PERCENT).contains(p))
            .map(Self::Multiple)
    }

    /// A length-based spacing from `twips` (`> 0`, clamped to [`MAX_LINE_SPACING_TWIPS`]); `build`
    /// is `AtLeast` or `Exact`.
    pub fn from_twips(build: fn(u16) -> Self, twips: i64) -> Option<Self> {
        (twips > 0).then(|| Self::clamped(build, twips))
    }

    /// Space between lines from `twips` (`>= 0`: no extra space is the single line), clamped.
    pub fn between(twips: i64) -> Option<Self> {
        (twips >= 0).then(|| Self::clamped(Self::Between, twips))
    }

    fn clamped(build: fn(u16) -> Self, twips: i64) -> Self {
        build(u16::try_from(twips.min(i64::from(MAX_LINE_SPACING_TWIPS))).unwrap_or(0))
    }

    /// RTF `\sl` with `\slmult`: a positive value is a multiple in 240ths of a line with `\slmult1`,
    /// else at least that many twips; a negative value (with `\slmult0`) is exactly that many twips.
    pub(crate) fn from_rtf(sl: i64, multiple: bool) -> Option<Self> {
        match (multiple, sl.signum()) {
            (true, 1) => Self::multiple((sl * 100 + 120) / 240),
            (false, 1) => Self::from_twips(Self::AtLeast, sl),
            (false, -1) => Self::from_twips(Self::Exact, -sl),
            _ => None,
        }
    }

    /// The multiple a format without the between-lines rule writes instead: the space is added to
    /// one single line (12 pt). Other spacings return their own multiple or the length against
    /// that line.
    pub fn multiple_percent(self) -> u16 {
        let scaled = |twips: u16, base: u32| {
            ((base + u32::from(twips)) * 100 + SINGLE_LINE_TWIPS / 2) / SINGLE_LINE_TWIPS
        };
        let percent = match self {
            Self::Multiple(p) => return p,
            Self::AtLeast(t) | Self::Exact(t) => scaled(t, 0),
            Self::Between(t) => scaled(t, SINGLE_LINE_TWIPS),
        };
        percent.clamp(1, u32::from(MAX_LINE_PERCENT)) as u16
    }

    /// Kind token used in the JSON between the editor and the engine.
    pub fn kind_token(self) -> &'static str {
        match self {
            Self::Multiple(_) => "multiple",
            Self::AtLeast(_) => "atLeast",
            Self::Exact(_) => "exact",
            Self::Between(_) => "between",
        }
    }

    /// The percent of a multiple or the twips of a length rule.
    pub fn amount(self) -> u16 {
        match self {
            Self::Multiple(v) | Self::AtLeast(v) | Self::Exact(v) | Self::Between(v) => v,
        }
    }

    /// Builds a spacing from a [`Self::kind_token`] and an amount within that kind's bounds.
    pub fn from_token(kind: &str, amount: i64) -> Option<Self> {
        match kind {
            "multiple" => Self::multiple(amount),
            "atLeast" => Self::from_twips(Self::AtLeast, amount),
            "exact" => Self::from_twips(Self::Exact, amount),
            "between" => Self::between(amount),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_are_enforced() {
        assert_eq!(RichLineSpacing::multiple(0), None);
        assert_eq!(RichLineSpacing::multiple(1001), None);
        assert_eq!(RichLineSpacing::multiple(-5), None);
        assert_eq!(
            RichLineSpacing::multiple(150),
            Some(RichLineSpacing::Multiple(150))
        );
        assert_eq!(RichLineSpacing::from_twips(RichLineSpacing::Exact, 0), None);
        assert_eq!(
            RichLineSpacing::from_twips(RichLineSpacing::Exact, -1),
            None
        );
        assert_eq!(
            RichLineSpacing::from_twips(RichLineSpacing::AtLeast, i64::MAX),
            Some(RichLineSpacing::AtLeast(MAX_LINE_SPACING_TWIPS))
        );
        assert_eq!(RichLineSpacing::between(-1), None);
        assert_eq!(
            RichLineSpacing::between(0),
            Some(RichLineSpacing::Between(0))
        );
        assert_eq!(
            RichLineSpacing::from_token("exact", 360).map(RichLineSpacing::amount),
            Some(360)
        );
        assert_eq!(RichLineSpacing::from_token("nonsense", 360), None);
    }

    #[test]
    fn between_lines_falls_back_to_the_equal_multiple() {
        assert_eq!(RichLineSpacing::Between(0).multiple_percent(), 100);
        assert_eq!(RichLineSpacing::Between(120).multiple_percent(), 150);
        assert_eq!(RichLineSpacing::Between(240).multiple_percent(), 200);
        assert_eq!(
            RichLineSpacing::Between(MAX_LINE_SPACING_TWIPS).multiple_percent(),
            MAX_LINE_PERCENT
        );
        assert_eq!(RichLineSpacing::Exact(240).multiple_percent(), 100);
    }
}
