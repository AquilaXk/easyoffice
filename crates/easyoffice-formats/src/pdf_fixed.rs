//! `{:.2}` of a PDF coordinate without the float printer (PD-397).
//!
//! `format!("{x:.2}")` runs the exact float-to-decimal algorithm (a Grisu attempt, then Dragon): a sheet page
//! with a few thousand cells spent about a tenth of its time there. The text of a PDF number has to stay the
//! same byte for byte, so this is not an approximation: the value's own bits give the exact hundredths (one
//! integer multiply and a shift), rounded half to even like the standard printer, and the digits are written
//! by hand. Values the integer path does not cover (not finite, 2^40 and up) go through `write!`.

use std::fmt::Write;

/// Values from this exponent (unbiased) up are left to the standard printer: no coordinate is that large.
const MAX_FAST_EXPONENT: i32 = 40;
const EXPONENT_BIAS: i32 = 1023;
const MANTISSA_BITS: u32 = 52;
const HUNDRED: u128 = 100;

/// Appends `value` as `format!("{value:.2}")` would write it.
pub(crate) fn push_fixed2(out: &mut String, value: f64) {
    let bits = value.to_bits();
    let biased = ((bits >> MANTISSA_BITS) & 0x7FF) as i32;
    if biased - EXPONENT_BIAS >= MAX_FAST_EXPONENT {
        let _ = write!(out, "{value:.2}");
        return;
    }
    let fraction = bits & ((1u64 << MANTISSA_BITS) - 1);
    // value = mantissa * 2^exponent; a subnormal has no implicit leading bit.
    let (mantissa, exponent) = if biased == 0 {
        (fraction, 1 - EXPONENT_BIAS - MANTISSA_BITS as i32)
    } else {
        (
            fraction | (1u64 << MANTISSA_BITS),
            biased - EXPONENT_BIAS - MANTISSA_BITS as i32,
        )
    };
    // Below 2^40 and with 52 mantissa bits the exponent is negative, so the hundredths are a right shift.
    let scaled = u128::from(mantissa) * HUNDRED;
    let shift = exponent.unsigned_abs();
    let hundredths = if shift >= 127 {
        0
    } else {
        let whole = scaled >> shift;
        let rest = scaled & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        whole + u128::from(rest > half || (rest == half && whole & 1 == 1))
    };
    if bits >> 63 == 1 {
        out.push('-');
    }
    push_digits(out, (hundredths / HUNDRED) as u64);
    out.push('.');
    let cents = (hundredths % HUNDRED) as u8;
    out.push(char::from(b'0' + cents / 10));
    out.push(char::from(b'0' + cents % 10));
}

/// Appends `values` as numbers with two decimals, one space apart.
pub(crate) fn push_numbers(out: &mut String, values: &[f64]) {
    for (at, &value) in values.iter().enumerate() {
        if at > 0 {
            out.push(' ');
        }
        push_fixed2(out, value);
    }
}

/// A path through `pts`: `x y m`, then `x y l` for each further point.
pub(crate) fn push_path(out: &mut String, pts: &[(f64, f64)]) {
    for (at, &(x, y)) in pts.iter().enumerate() {
        if at > 0 {
            out.push(' ');
        }
        push_numbers(out, &[x, y]);
        out.push_str(if at == 0 { " m" } else { " l" });
    }
}

fn push_digits(out: &mut String, mut number: u64) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    loop {
        at -= 1;
        digits[at] = b'0' + (number % 10) as u8;
        number /= 10;
        if number == 0 {
            break;
        }
    }
    // The bytes are ASCII digits.
    out.extend(digits[at..].iter().map(|&digit| char::from(digit)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same_as_the_printer(value: f64) {
        let mut fast = String::new();
        push_fixed2(&mut fast, value);
        assert_eq!(
            fast,
            format!("{value:.2}"),
            "{value:?} ({:#x})",
            value.to_bits()
        );
    }

    #[test]
    fn ties_zero_signs_and_the_ends_of_the_range_read_like_the_printer() {
        for value in [
            0.0,
            -0.0,
            0.001,
            -0.001,
            0.005,
            0.015,
            0.025,
            0.125,
            0.375,
            0.625,
            1.005,
            2.675,
            0.994999,
            0.995,
            9.995,
            99.995,
            1.0,
            12.5,
            595.28,
            841.89,
            0.75,
            1.5,
            0.25,
            1e-300,
            5e-324,
            -5e-324,
            123_456_789.125,
            1.0e11 + 0.125,
            (1u64 << 39) as f64 + 0.5,
            (1u64 << 40) as f64,
            (1u64 << 52) as f64,
            1e300,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            f64::MIN_POSITIVE,
            0.004_999_999_999_999_999,
        ] {
            same_as_the_printer(value);
        }
    }

    /// Hundredths and their ties, around every power of two a page coordinate can have, and a run of random bit
    /// patterns of the sizes a page holds.
    #[test]
    fn every_coordinate_shape_reads_like_the_printer() {
        for whole in (0..2000u32).chain([4095, 65_535, 1_000_000]) {
            for cents in 0..200u32 {
                let value = f64::from(whole) + f64::from(cents) / 200.0;
                same_as_the_printer(value);
                same_as_the_printer(-value);
                same_as_the_printer(f64::from_bits(value.to_bits() + 1));
                same_as_the_printer(f64::from_bits(value.to_bits().saturating_sub(1)));
            }
        }
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for _ in 0..300_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            same_as_the_printer(((state >> 11) as f64) / 4096.0 - 5000.0);
            same_as_the_printer(f64::from_bits(state));
        }
    }
}
