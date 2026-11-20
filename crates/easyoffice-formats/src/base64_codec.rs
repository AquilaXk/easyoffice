//! Standard-alphabet, padded Base64 (RFC 4648 §4) encoder and decoder.
//!
//! A thin adapter over the `base64` crate (MIT OR Apache-2.0). The crate's `GeneralPurpose` engine
//! does the block coding; this module only fixes the contract the callers rely on: padding is
//! required and only valid in the final quad, non-zero trailing bits are accepted, ASCII whitespace
//! is ignored, and empty input is `None`.

use base64::alphabet::STANDARD;
use base64::engine::{GeneralPurpose, GeneralPurposeConfig};
use base64::Engine;

const ENGINE: GeneralPurpose = GeneralPurpose::new(
    &STANDARD,
    GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
);

/// Encodes `bytes` as padded standard Base64.
pub fn encode(bytes: &[u8]) -> String {
    ENGINE.encode(bytes)
}

/// Decodes padded standard Base64, ignoring ASCII whitespace. Returns `None` for empty input,
/// a length that is not a multiple of 4, a symbol outside the alphabet, or padding anywhere but
/// the final quad.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let raw = input.as_bytes();
    // Whitespace is outside the alphabet, so the common no-whitespace input decodes in one pass;
    // only a failed pass pays for the strip-and-retry.
    decode_clean(raw).or_else(|| {
        if !raw.iter().any(u8::is_ascii_whitespace) {
            return None;
        }
        let stripped: Vec<u8> = raw
            .iter()
            .copied()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        decode_clean(&stripped)
    })
}

fn decode_clean(clean: &[u8]) -> Option<Vec<u8>> {
    if clean.is_empty() {
        return None;
    }
    ENGINE.decode(clean).ok()
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    /// The byte-at-a-time decoder this module's contract was written against; kept as the oracle for
    /// the malformed-input cases (padding position, whitespace, trailing bits).
    fn reference_decode(input: &str) -> Option<Vec<u8>> {
        let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        if clean.is_empty() || (clean.len() & 3) != 0 {
            return None;
        }
        let val = |b: u8| -> Option<u8> {
            match b {
                b'A'..=b'Z' => Some(b - b'A'),
                b'a'..=b'z' => Some(b - b'a' + 26),
                b'0'..=b'9' => Some(b - b'0' + 52),
                b'+' => Some(62),
                b'/' => Some(63),
                _ => None,
            }
        };
        let mut out = Vec::new();
        let mut idx = 0usize;
        while idx + 4 <= clean.len() {
            if (clean[idx + 2] == b'=' || clean[idx + 3] == b'=') && idx + 4 != clean.len() {
                return None;
            }
            let a = val(clean[idx])?;
            let b = val(clean[idx + 1])?;
            out.push((a << 2) | (b >> 4));
            if clean[idx + 2] != b'=' {
                let c = val(clean[idx + 2])?;
                out.push(((b & 0x0F) << 4) | (c >> 2));
                if clean[idx + 3] != b'=' {
                    let d = val(clean[idx + 3])?;
                    out.push(((c & 0x03) << 6) | d);
                }
            } else if clean[idx + 3] != b'=' {
                return None;
            }
            idx += 4;
        }
        Some(out)
    }

    fn sample(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i.wrapping_mul(131) ^ (i >> 3)) as u8)
            .collect()
    }

    #[test]
    fn rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, coded) in cases {
            assert_eq!(encode(plain.as_bytes()), coded);
            if !coded.is_empty() {
                assert_eq!(decode(coded).as_deref(), Some(plain.as_bytes()));
            }
        }
    }

    #[test]
    fn round_trips_for_every_tail_length() {
        for len in 0..200 {
            let bytes = sample(len);
            let coded = encode(&bytes);
            if len > 0 {
                assert_eq!(decode(&coded), Some(bytes), "len {len}");
            }
        }
    }

    #[test]
    fn matches_reference_decoder_on_malformed_input() {
        let cases = [
            "",
            "   ",
            "Zg=",
            "Zg===",
            "Z===",
            "Zm9v\nYmFy",
            " Zm9v Ym E= ",
            "Zg==Zg==",
            "Zm=v",
            "Zm9v!mFy",
            "=Zm9",
            "Zm9vYmFyZm9vYmFy",
            "Zm9vYmFyZm9vYm=y",
            "Zh==",
            "Zm9\u{e9}",
        ];
        for case in cases {
            assert_eq!(decode(case), reference_decode(case), "{case:?}");
        }
    }

    #[test]
    fn rejects_padding_inside_an_eight_symbol_block() {
        assert_eq!(decode("Zm9vYg==Zm9vYmFy"), None);
        assert_eq!(
            decode("Zm9vYg==Zm9vYmFy"),
            reference_decode("Zm9vYg==Zm9vYmFy")
        );
    }
}
