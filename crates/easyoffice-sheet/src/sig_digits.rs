//! 참조 구현: IronCalc `base/src/number_format.rs:117-127` (`to_precision`) and `:220-226`
//! (`to_excel_precision`), commit f93662006b8fb1c401dc870bbda26c5d60ac6037, `MIT OR Apache-2.0`,
//! Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present
//! IronCalc GmbH. The rounding functions take their number, and CEILING and FLOOR their ratio, at 15
//! significant digits before cutting, as the reference app does. The reference's `to_precision`
//! also prints the value with `ryu` and parses it back, which returns the same `f64`, so both
//! functions share this one formatting round trip.

/// Significant digits of a spreadsheet number (`base/src/constants.rs:28`).
pub(crate) const EXCEL_PRECISION: usize = 15;

/// `value` rounded to `precision` significant digits; infinities and NaN pass through.
pub(crate) fn to_precision(value: f64, precision: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{:.*e}", precision.saturating_sub(1), value)
        .parse::<f64>()
        .unwrap_or(value)
}
