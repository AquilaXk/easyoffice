//! RFC 1951 DEFLATE and RFC 1950 zlib streams.
//!
//! A thin adapter over the `miniz_oxide` crate (MIT OR Zlib OR Apache-2.0) and `adler2`
//! (0BSD OR MIT OR Apache-2.0). The crate decodes and encodes; this module adds what the readers
//! rely on: a hard output budget that fails closed (`ResourceBudgetExceeded`), an output buffer
//! that grows with the data that really appears instead of trusting a container's declared size
//! (PD-175), the number of input bytes a stream used, and the zlib trailer handling of
//! [`inflate_auto`].

use crate::EasyOfficeError;
use miniz_oxide::deflate::{compress_to_vec, compress_to_vec_zlib};
use miniz_oxide::inflate::core::{decompress, inflate_flags, DecompressorOxide};
use miniz_oxide::inflate::TINFLStatus;

/// `miniz_oxide` effort level of every compressor entry point (1 = fastest, 9 = smallest). On the
/// 1 MB DOCX conversion benchmark level 3 wrote 17% smaller files than level 2 for 13% more time, and
/// level 1 compressed worse than the former fixed-Huffman baseline of the self-test.
pub(crate) const COMPRESSION_LEVEL: u8 = 3;
const MIN_OUT_CAPACITY: usize = 4_096;
/// Most output allocated before any byte is produced. The declared size is untrusted (a ZIP
/// header can claim 512 MiB for a 100-byte stream), so a bigger buffer is only reached by growing
/// with the output that really appears.
const MAX_INITIAL_OUT_CAPACITY: usize = 4 * 1024 * 1024;
/// Growth factor while the declared size is still ahead of the buffer.
const HINTED_GROWTH: usize = 4;
/// Theoretical upper bound of RFC 1951 DEFLATE expansion (~1032x per compressed input byte).
const MAX_DEFLATE_EXPANSION: usize = 1_032;

/// Decompresses raw DEFLATE `input` and returns the output with the number of input bytes the
/// stream occupied (a zlib trailer or a following record starts there).
///
/// The output is cut off at `max_output_bytes`: one byte past it proves the stream is bigger, so the
/// buffer never grows beyond `max_output_bytes + 1`. `expected_len` is a growth hint only.
pub(crate) fn inflate_raw_with_consumed(
    input: &[u8],
    max_output_bytes: usize,
    expected_len: usize,
) -> Result<(Vec<u8>, usize), EasyOfficeError> {
    if input.is_empty() {
        return Err(EasyOfficeError::CorruptedContainer(
            "비어 있는 DEFLATE 스트림입니다.".to_string(),
        ));
    }
    let hard_cap = max_output_bytes.saturating_add(1);
    let max_possible = input
        .len()
        .saturating_mul(MAX_DEFLATE_EXPANSION)
        .max(MIN_OUT_CAPACITY);
    let hint = expected_len.min(max_possible);
    let guess = if hint > 0 {
        hint
    } else {
        input.len().saturating_mul(8).max(MIN_OUT_CAPACITY)
    };
    let mut out = vec![0u8; guess.min(MAX_INITIAL_OUT_CAPACITY).min(hard_cap).max(1)];
    let mut decompressor = Box::<DecompressorOxide>::default();
    let flags = inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    let (mut in_pos, mut out_pos) = (0usize, 0usize);
    loop {
        let (status, used_in, used_out) = decompress(
            &mut decompressor,
            &input[in_pos..],
            &mut out,
            out_pos,
            flags,
        );
        in_pos += used_in;
        out_pos += used_out;
        match status {
            TINFLStatus::Done => {
                if out_pos > max_output_bytes {
                    return Err(over_budget(max_output_bytes));
                }
                out.truncate(out_pos);
                return Ok((out, in_pos));
            }
            TINFLStatus::HasMoreOutput => {
                if out.len() >= hard_cap {
                    return Err(over_budget(max_output_bytes));
                }
                let grown = if out.len() <= hint {
                    out.len().saturating_mul(HINTED_GROWTH).min(hint + 1)
                } else {
                    out.len().saturating_mul(2)
                };
                out.resize(grown.max(out.len() + 1).min(hard_cap), 0);
            }
            TINFLStatus::NeedsMoreInput | TINFLStatus::FailedCannotMakeProgress => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DEFLATE 스트림이 끝나기 전에 입력이 절단되었습니다.".to_string(),
                ));
            }
            other => {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "유효하지 않은 DEFLATE 스트림입니다({other:?})."
                )));
            }
        }
    }
}

fn over_budget(max_output_bytes: usize) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(format!(
        "DEFLATE 출력 크기가 허용 상한({max_output_bytes} bytes)을 초과했습니다."
    ))
}

/// Decompresses an RFC 1951 raw DEFLATE bitstream with Fail-Closed output budget enforcement.
pub fn inflate_raw(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    inflate_raw_with_consumed(input, max_output_bytes, 0).map(|(out, _)| out)
}

/// [`inflate_raw`] for a stream whose output size the container declares (`expected_len`): the
/// output is allocated once at that size. The declaration is only a hint, so a stream that
/// produces more is still cut off at `max_output_bytes`.
pub fn inflate_raw_sized(
    input: &[u8],
    max_output_bytes: usize,
    expected_len: usize,
) -> Result<Vec<u8>, EasyOfficeError> {
    inflate_raw_with_consumed(input, max_output_bytes, expected_len).map(|(out, _)| out)
}

/// Computes the RFC 1950 Adler-32 checksum.
#[inline]
pub fn adler32(bytes: &[u8]) -> u32 {
    adler2::adler32_slice(bytes)
}

/// Compresses `input` into an RFC 1950 zlib stream.
pub fn deflate_zlib(input: &[u8]) -> Vec<u8> {
    compress_to_vec_zlib(input, COMPRESSION_LEVEL)
}

#[inline]
fn is_valid_zlib_header(input: &[u8]) -> bool {
    input.len() >= 2
        && (input[0] & 0x0F) == 8
        && (input[0] & 0x80) == 0
        && (input[1] & 0x20) == 0
        && (u16::from(input[0]) * 256 + u16::from(input[1])) % 31 == 0
}

fn adler_mismatch(expected: u32, actual: u32) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!(
        "RFC 1950 Adler-32 체크섬 불일치 (expected: {expected:#010x}, actual: {actual:#010x})"
    ))
}

/// Decompresses an RFC 1950 zlib stream and validates the 2-byte header and 4-byte Adler-32 trailer.
pub fn inflate_zlib(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    if input.len() < 6 || !is_valid_zlib_header(input) {
        return Err(EasyOfficeError::CorruptedContainer(
            "유효하지 않은 RFC 1950 zlib 스트림(헤더 2 + 데이터 + Adler-32 4)입니다.".to_string(),
        ));
    }
    let (decoded, consumed) = inflate_raw_with_consumed(&input[2..], max_output_bytes, 0)?;
    let rem = &input[2 + consumed..];
    if rem.len() < 4 {
        return Err(EasyOfficeError::CorruptedContainer(
            "RFC 1950 zlib 스트림에 4바이트 Adler-32 트레일러가 누락되었습니다.".to_string(),
        ));
    }
    let expected = u32::from_be_bytes([rem[0], rem[1], rem[2], rem[3]]);
    let actual = adler32(&decoded);
    if actual != expected {
        return Err(adler_mismatch(expected, actual));
    }
    Ok(decoded)
}

/// Decompresses either a raw RFC 1951 DEFLATE stream or an RFC 1950 zlib-wrapped DEFLATE stream.
pub fn inflate_auto(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    if is_valid_zlib_header(input) {
        match inflate_raw_with_consumed(&input[2..], max_output_bytes, 0) {
            Ok((decoded, consumed)) => {
                let rem = &input[2 + consumed..];
                if rem.len() >= 4 {
                    let expected = u32::from_be_bytes([rem[0], rem[1], rem[2], rem[3]]);
                    let actual = adler32(&decoded);
                    if expected != actual {
                        return Err(adler_mismatch(expected, actual));
                    }
                    return Ok(decoded);
                }
                if rem.is_empty() {
                    return Ok(decoded);
                }
                return Err(EasyOfficeError::CorruptedContainer(
                    "RFC 1950 zlib 스트림의 4바이트 Adler-32 트레일러가 잘렸습니다.".to_string(),
                ));
            }
            Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(e),
            Err(_) => {}
        }
    }
    inflate_raw(input, max_output_bytes)
}

/// Compresses `input` into an RFC 1951 raw DEFLATE bitstream. The output is deterministic for a
/// given input and crate version.
pub fn deflate_raw(input: &[u8]) -> Vec<u8> {
    compress_to_vec(input, COMPRESSION_LEVEL)
}

#[cfg(test)]
mod tests;
