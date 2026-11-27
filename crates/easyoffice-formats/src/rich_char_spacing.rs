//! Letter spacing of a run (PD-367): the space added after each character, in twips. A positive
//! value spreads the text (DOCX `w:spacing`, ODT `fo:letter-spacing`, RTF `\expndtw`), a negative
//! one tightens it. HWPX (`hh:spacing`) and Pages (`tracking`) keep it as a fraction of the font
//! size, so those two convert with the run's size.

/// Largest letter spacing a document keeps, in twips either way (1,584 pt, the largest value the
/// DOCX font dialog takes).
pub const MAX_LETTER_SPACING_TWIPS: i16 = 31_680;

/// Range of the HWPX character spacing (`hh:spacing`), in percent of the font size either way.
pub(crate) const MAX_SPACING_PERCENT: i64 = 50;

/// Largest per-mille a file's spacing is read at (1000 times the font size) so the arithmetic cannot overflow.
const MAX_PERMILLE: i64 = 1_000_000;

/// Twips of the 10 pt default size the HWPX writer gives a run that names none.
const DEFAULT_SIZE_HALF_POINTS: u32 = 20;
const TWIPS_PER_HALF_POINT: i64 = 10;

/// A letter spacing from `twips`, clamped to [`MAX_LETTER_SPACING_TWIPS`] either way; zero is no spacing.
pub fn letter_spacing(twips: i64) -> Option<i16> {
    let max = i64::from(MAX_LETTER_SPACING_TWIPS);
    i16::try_from(twips.clamp(-max, max))
        .ok()
        .filter(|t| *t != 0)
}

/// The size a spacing is measured against, in twips: the run's size or the default.
fn size_twips(size_half_pt: Option<u16>) -> i64 {
    i64::from(size_half_pt.map_or(DEFAULT_SIZE_HALF_POINTS, u32::from)) * TWIPS_PER_HALF_POINT
}

/// Letter spacing in per-mille of the font size (HWPX percent is a tenth of this), rounded.
pub(crate) fn to_permille(twips: i16, size_half_pt: Option<u16>) -> i64 {
    let size = size_twips(size_half_pt).max(1);
    let scaled = i64::from(twips) * 1000;
    (scaled + scaled.signum() * size / 2) / size
}

/// Letter spacing from per-mille of the font size.
pub(crate) fn from_permille(permille: i64, size_half_pt: Option<u16>) -> Option<i16> {
    let scaled = permille.clamp(-MAX_PERMILLE, MAX_PERMILLE) * size_twips(size_half_pt);
    letter_spacing((scaled + scaled.signum() * 500) / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_and_drops_zero() {
        assert_eq!(letter_spacing(0), None);
        assert_eq!(letter_spacing(40), Some(40));
        assert_eq!(letter_spacing(-40), Some(-40));
        assert_eq!(letter_spacing(i64::MAX), Some(MAX_LETTER_SPACING_TWIPS));
        assert_eq!(letter_spacing(i64::MIN), Some(-MAX_LETTER_SPACING_TWIPS));
    }

    #[test]
    fn a_zero_size_does_not_divide_by_zero() {
        assert_eq!(to_permille(60, Some(0)), 60_000);
        assert_eq!(from_permille(60_000, Some(0)), None);
    }

    #[test]
    fn size_relative_conversion_roundtrips_at_the_run_size() {
        for size in [None, Some(18u16), Some(22), Some(40)] {
            for twips in [-60i16, -1, 1, 20, 60, 400] {
                let permille = to_permille(twips, size);
                let back = from_permille(permille, size).unwrap_or(0);
                let step = i64::from(size.map_or(20, u32::from)) * 10 / 1000 + 1;
                assert!(
                    (i64::from(back) - i64::from(twips)).abs() <= step,
                    "{twips} at {size:?}: {back}"
                );
            }
        }
        assert_eq!(
            to_permille(60, Some(24)),
            250,
            "3 pt on 12 pt is a quarter of the size"
        );
        assert_eq!(from_permille(250, Some(24)), Some(60));
    }
}
