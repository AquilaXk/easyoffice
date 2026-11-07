//! Standard-alphabet, padded Base64 (RFC 4648 §4) encoder and decoder.
//!
//! The block layout follows the `GeneralPurpose` engine of the `base64` crate
//! (https://github.com/marshallpierce/rust-base64, v0.22, MIT OR Apache-2.0, see
//! THIRD_PARTY_NOTICES.md): 6 input bytes are packed big-endian into one `u64` and emitted as
//! 8 symbols, and 8 symbols are decoded through a 256-entry table into 6 bytes, so the hot loops
//! work on whole words with no per-symbol branching and write straight into a pre-sized buffer.

const ENCODE_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const PAD: u8 = b'=';
/// Decode-table marker for bytes outside the alphabet (high bit set, never a 6-bit value).
const INVALID: u8 = 0x80;
const SIX_BIT_MASK: u64 = 0x3F;
const ENCODE_BLOCK_IN: usize = 6;
const DECODE_BLOCK_IN: usize = 8;
const QUAD: usize = 4;
const LOAD_BYTES: usize = 8;

const DECODE_TABLE: [u8; 256] = build_decode_table();

const fn build_decode_table() -> [u8; 256] {
    let mut table = [INVALID; 256];
    let mut i = 0;
    while i < ENCODE_TABLE.len() {
        table[ENCODE_TABLE[i] as usize] = i as u8;
        i += 1;
    }
    table
}

#[inline]
fn symbol(bits: u64, shift: u32) -> u8 {
    ENCODE_TABLE[((bits >> shift) & SIX_BIT_MASK) as usize]
}

/// Encodes `bytes` as padded standard Base64.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = vec![0u8; bytes.len().div_ceil(3) * QUAD];
    let mut src = 0usize;
    let mut dst = 0usize;
    // Load 8 bytes and use the top 6, so each step is one unaligned word read (the trailing two
    // bytes are re-read by the next step); stop while 8 bytes are still in range.
    while src + LOAD_BYTES <= bytes.len() {
        let mut word = [0u8; LOAD_BYTES];
        word.copy_from_slice(&bytes[src..src + LOAD_BYTES]);
        let bits = u64::from_be_bytes(word);
        out[dst..dst + DECODE_BLOCK_IN].copy_from_slice(&[
            symbol(bits, 58),
            symbol(bits, 52),
            symbol(bits, 46),
            symbol(bits, 40),
            symbol(bits, 34),
            symbol(bits, 28),
            symbol(bits, 22),
            symbol(bits, 16),
        ]);
        src += ENCODE_BLOCK_IN;
        dst += DECODE_BLOCK_IN;
    }
    encode_tail(&bytes[src..], &mut out[dst..]);
    // Every byte written above comes from ENCODE_TABLE or is '=', so the buffer is ASCII.
    String::from_utf8(out).unwrap_or_default()
}

/// Encodes the final (fewer than 8) input bytes into `dst` (exactly their padded quads).
fn encode_tail(rest: &[u8], dst: &mut [u8]) {
    for (src, quad) in rest.chunks(3).zip(dst.chunks_exact_mut(QUAD)) {
        let mut word = [0u8; 3];
        word[..src.len()].copy_from_slice(src);
        let bits = (u64::from(word[0]) << 16) | (u64::from(word[1]) << 8) | u64::from(word[2]);
        quad.copy_from_slice(&[
            symbol(bits, 18),
            symbol(bits, 12),
            if src.len() > 1 { symbol(bits, 6) } else { PAD },
            if src.len() > 2 { symbol(bits, 0) } else { PAD },
        ]);
    }
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
    if clean.is_empty() || clean.len() % QUAD != 0 {
        return None;
    }
    let (body, last) = clean.split_at(clean.len() - QUAD);
    let mut out = vec![0u8; body.len() / QUAD * 3];
    if !decode_body(body, &mut out) {
        return None;
    }
    decode_final_quad(last, &mut out)?;
    Some(out)
}

/// Decodes unpadded full quads into the pre-sized `out`; `false` if any symbol is outside the
/// alphabet (including `=`).
fn decode_body(body: &[u8], out: &mut [u8]) -> bool {
    let mut invalid = 0u8;
    let full_blocks = body.len() / DECODE_BLOCK_IN;
    let (block_in, rest) = body.split_at(full_blocks * DECODE_BLOCK_IN);
    let (block_out, rest_out) = out.split_at_mut(full_blocks * ENCODE_BLOCK_IN);
    for (src, dst) in block_in
        .chunks_exact(DECODE_BLOCK_IN)
        .zip(block_out.chunks_exact_mut(ENCODE_BLOCK_IN))
    {
        let mut bits = 0u64;
        for &sym in src {
            let v = DECODE_TABLE[sym as usize];
            invalid |= v;
            bits = (bits << 6) | u64::from(v);
        }
        dst.copy_from_slice(&bits.to_be_bytes()[2..]);
    }
    for (quad, dst) in rest.chunks_exact(QUAD).zip(rest_out.chunks_exact_mut(3)) {
        let (bits, bad) = quad_bits(quad);
        invalid |= bad;
        dst.copy_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
    }
    invalid & INVALID == 0
}

#[inline]
fn quad_bits(quad: &[u8]) -> (u32, u8) {
    let mut bits = 0u32;
    let mut bad = 0u8;
    for &sym in quad {
        let v = DECODE_TABLE[sym as usize];
        bad |= v;
        bits = (bits << 6) | u32::from(v & 0x3F);
    }
    (bits, bad)
}

/// Decodes the last quad, which may end in `=` or `==`.
fn decode_final_quad(quad: &[u8], out: &mut Vec<u8>) -> Option<()> {
    let pad = match (quad[2], quad[3]) {
        (PAD, PAD) => 2,
        (PAD, _) => return None,
        (_, PAD) => 1,
        _ => 0,
    };
    let mut padded = [b'A'; QUAD];
    padded[..QUAD - pad].copy_from_slice(&quad[..QUAD - pad]);
    let (bits, bad) = quad_bits(&padded);
    if bad & INVALID != 0 {
        return None;
    }
    let bytes = [(bits >> 16) as u8, (bits >> 8) as u8, bits as u8];
    out.extend_from_slice(&bytes[..3 - pad]);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    /// The byte-at-a-time decoder this module replaced; kept as the behavioural reference.
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

    /// The char-at-a-time encoder this module replaced.
    fn reference_encode(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(if chunk.len() > 1 {
                TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                TABLE[(b2 & 0x3F) as usize] as char
            } else {
                '='
            });
        }
        out
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
    fn matches_reference_encoder_and_round_trips_for_every_tail_length() {
        for len in 0..200 {
            let bytes = sample(len);
            let coded = encode(&bytes);
            assert_eq!(coded, reference_encode(&bytes), "len {len}");
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
