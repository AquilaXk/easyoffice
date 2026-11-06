//! IEEE 802.3 CRC-32 (`0xEDB88320` reflected polynomial) used by ZIP, PNG and gzip containers.
//!
//! Three implementations share one result: a scalar slicing-by-16 core (always available, backed
//! by a `static` table so no per-lookup copies occur), a PCLMULQDQ carry-less-multiply folding
//! path on x86_64, and the CRC32 instruction path on aarch64. The hardware paths are selected at
//! run time and only run when the CPU reports the feature; everything else uses the scalar core.
//! All `unsafe` is confined to the two small `#[target_feature]` functions below.

const POLYNOMIAL: u32 = 0xEDB8_8320;
const SLICES: usize = 16;

static CRC32_TABLES: [[u32; 256]; SLICES] = build_tables();

const fn build_tables() -> [[u32; 256]; SLICES] {
    let mut tables = [[0u32; 256]; SLICES];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLYNOMIAL
            } else {
                crc >> 1
            };
            bit += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }
    let mut t = 1usize;
    while t < SLICES {
        let mut j = 0usize;
        while j < 256 {
            let prev = tables[t - 1][j];
            tables[t][j] = (prev >> 8) ^ tables[0][(prev & 0xFF) as usize];
            j += 1;
        }
        t += 1;
    }
    tables
}

/// Inputs shorter than this stay on the scalar core: the SIMD setup cost is not worth it.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const HARDWARE_MIN_LEN: usize = 64;

/// IEEE 802.3 CRC-32 checksum calculator (slicing-by-16 scalar core + runtime-detected hardware).
pub struct IeeeCrc32;

impl IeeeCrc32 {
    pub const POLYNOMIAL: u32 = POLYNOMIAL;

    /// CRC-32 of `bytes`, using the fastest implementation the running CPU supports.
    #[inline]
    pub fn checksum(bytes: &[u8]) -> u32 {
        Self::update(0, bytes)
    }

    /// Continues a CRC-32 (the value returned by [`Self::checksum`] / a previous `update`)
    /// over more bytes; `update(0, data)` equals `checksum(data)`.
    #[inline]
    pub fn update(crc: u32, bytes: &[u8]) -> u32 {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        if bytes.len() >= HARDWARE_MIN_LEN {
            if let Some(v) = hardware_update(crc, bytes) {
                return v;
            }
        }
        scalar_update(crc, bytes)
    }

    /// Scalar slicing-by-16 implementation (no hardware instructions).
    #[inline]
    pub fn checksum_scalar(bytes: &[u8]) -> u32 {
        scalar_update(0, bytes)
    }

    /// Hardware-accelerated CRC-32, or `None` when the CPU lacks PCLMULQDQ (x86_64) / CRC32
    /// (aarch64) support or the target architecture has no such path.
    #[inline]
    pub fn checksum_hardware(bytes: &[u8]) -> Option<u32> {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        {
            hardware_update(0, bytes)
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            let _ = bytes;
            None
        }
    }

    /// Name of the accelerated path the running CPU would use (`"pclmulqdq"`, `"crc32"` or
    /// `"scalar"`), for benchmark reports.
    pub fn hardware_path_name() -> &'static str {
        #[cfg(target_arch = "x86_64")]
        if std::arch::is_x86_feature_detected!("pclmulqdq")
            && std::arch::is_x86_feature_detected!("sse4.1")
        {
            return "pclmulqdq";
        }
        #[cfg(target_arch = "aarch64")]
        if std::arch::is_aarch64_feature_detected!("crc") {
            return "crc32";
        }
        "scalar"
    }

    /// One table lookup per byte; kept as the benchmark baseline.
    #[inline]
    pub fn checksum_byte_by_byte_baseline(bytes: &[u8]) -> u32 {
        let table = &CRC32_TABLES[0];
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in bytes {
            let idx = ((crc ^ u32::from(b)) & 0xFF) as usize;
            crc = (crc >> 8) ^ table[idx];
        }
        !crc
    }
}

#[inline]
fn scalar_update(crc: u32, bytes: &[u8]) -> u32 {
    let t = &CRC32_TABLES;
    let mut state = !crc;
    let mut chunks = bytes.chunks_exact(SLICES);
    for chunk in &mut chunks {
        let a = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ state;
        let b = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        let c = u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
        let d = u32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]);
        state = t[15][(a & 0xFF) as usize]
            ^ t[14][((a >> 8) & 0xFF) as usize]
            ^ t[13][((a >> 16) & 0xFF) as usize]
            ^ t[12][(a >> 24) as usize]
            ^ t[11][(b & 0xFF) as usize]
            ^ t[10][((b >> 8) & 0xFF) as usize]
            ^ t[9][((b >> 16) & 0xFF) as usize]
            ^ t[8][(b >> 24) as usize]
            ^ t[7][(c & 0xFF) as usize]
            ^ t[6][((c >> 8) & 0xFF) as usize]
            ^ t[5][((c >> 16) & 0xFF) as usize]
            ^ t[4][(c >> 24) as usize]
            ^ t[3][(d & 0xFF) as usize]
            ^ t[2][((d >> 8) & 0xFF) as usize]
            ^ t[1][((d >> 16) & 0xFF) as usize]
            ^ t[0][(d >> 24) as usize];
    }
    for &byte in chunks.remainder() {
        state = (state >> 8) ^ t[0][((state ^ u32::from(byte)) & 0xFF) as usize];
    }
    !state
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn hardware_update(crc: u32, bytes: &[u8]) -> Option<u32> {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("pclmulqdq")
            && std::arch::is_x86_feature_detected!("sse4.1")
        {
            // SAFETY: the required CPU features were just verified at run time.
            return Some(unsafe { x86::update(crc, bytes) });
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("crc") {
            // SAFETY: the `crc` CPU feature was just verified at run time.
            return Some(unsafe { arm::update(crc, bytes) });
        }
    }
    let _ = (crc, bytes);
    None
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::scalar_update;
    use std::arch::x86_64::{
        __m128i, _mm_and_si128, _mm_clmulepi64_si128, _mm_cvtsi32_si128, _mm_extract_epi32,
        _mm_loadu_si128, _mm_set_epi32, _mm_set_epi64x, _mm_srli_si128, _mm_xor_si128,
    };

    // Folding constants for the reflected 0x04C11DB7 polynomial (x^(4*128+32), x^(4*128-32),
    // x^(128+32), x^(128-32), x^64 mod P, P(x) and the Barrett constant mu).
    const K1: i64 = 0x1_5444_2bd4;
    const K2: i64 = 0x1_c6e4_1596;
    const K3: i64 = 0x1_7519_97d0;
    const K4: i64 = 0x0_ccaa_009e;
    const K5: i64 = 0x1_63cd_6124;
    const P_X: i64 = 0x1_DB71_0641;
    const U_PRIME: i64 = 0x1_F701_1641;

    /// Loads the next 16 bytes and advances the slice.
    ///
    /// # Safety
    /// `data` must hold at least 16 bytes.
    #[inline]
    #[target_feature(enable = "sse2")]
    unsafe fn load16(data: &mut &[u8]) -> __m128i {
        // SAFETY: the caller guarantees 16 readable bytes; `loadu` has no alignment requirement.
        let value = unsafe { _mm_loadu_si128(data.as_ptr().cast::<__m128i>()) };
        *data = &data[16..];
        value
    }

    #[inline]
    #[target_feature(enable = "sse2,pclmulqdq")]
    unsafe fn fold(acc: __m128i, next: __m128i, keys: __m128i) -> __m128i {
        let lo = _mm_clmulepi64_si128(acc, keys, 0x00);
        let hi = _mm_clmulepi64_si128(acc, keys, 0x11);
        _mm_xor_si128(_mm_xor_si128(next, lo), hi)
    }

    /// PCLMULQDQ folding CRC-32 (`crc` is the finalized running value, as in `update(0, ..)`).
    ///
    /// # Safety
    /// The CPU must support `pclmulqdq` and `sse4.1`.
    #[target_feature(enable = "sse2,sse4.1,pclmulqdq")]
    pub(super) unsafe fn update(crc: u32, mut data: &[u8]) -> u32 {
        if data.len() < 64 {
            return scalar_update(crc, data);
        }
        // SAFETY (all `load16` calls): each is guarded by a length check of at least 16 bytes.
        let mut x3 = unsafe { load16(&mut data) };
        let mut x2 = unsafe { load16(&mut data) };
        let mut x1 = unsafe { load16(&mut data) };
        let mut x0 = unsafe { load16(&mut data) };
        x3 = _mm_xor_si128(x3, _mm_cvtsi32_si128(!crc as i32));

        let k1k2 = _mm_set_epi64x(K2, K1);
        while data.len() >= 64 {
            // SAFETY: at least 64 bytes remain, so four 16-byte loads are in bounds.
            unsafe {
                x3 = fold(x3, load16(&mut data), k1k2);
                x2 = fold(x2, load16(&mut data), k1k2);
                x1 = fold(x1, load16(&mut data), k1k2);
                x0 = fold(x0, load16(&mut data), k1k2);
            }
        }

        let k3k4 = _mm_set_epi64x(K4, K3);
        let mut x = unsafe { fold(x3, x2, k3k4) };
        x = unsafe { fold(x, x1, k3k4) };
        x = unsafe { fold(x, x0, k3k4) };
        while data.len() >= 16 {
            // SAFETY: at least 16 bytes remain.
            x = unsafe { fold(x, load16(&mut data), k3k4) };
        }

        // 128 -> 64 bits.
        let mask32 = _mm_set_epi32(0, 0, 0, !0);
        x = _mm_xor_si128(_mm_clmulepi64_si128(x, k3k4, 0x10), _mm_srli_si128(x, 8));
        x = _mm_xor_si128(
            _mm_clmulepi64_si128(_mm_and_si128(x, mask32), _mm_set_epi64x(0, K5), 0x00),
            _mm_srli_si128(x, 4),
        );

        // Barrett reduction 64 -> 32 bits (bit-reflected variant: result is the upper dword).
        let pu = _mm_set_epi64x(U_PRIME, P_X);
        let t1 = _mm_clmulepi64_si128(_mm_and_si128(x, mask32), pu, 0x10);
        let t2 = _mm_clmulepi64_si128(_mm_and_si128(t1, mask32), pu, 0x00);
        let folded = !(_mm_extract_epi32(_mm_xor_si128(x, t2), 1) as u32);

        if data.is_empty() {
            folded
        } else {
            scalar_update(folded, data)
        }
    }
}

#[cfg(target_arch = "aarch64")]
mod arm {
    use std::arch::aarch64::{__crc32b, __crc32d};

    /// CRC32 instruction path (`crc` is the finalized running value).
    ///
    /// # Safety
    /// The CPU must support the aarch64 `crc` extension.
    #[target_feature(enable = "crc")]
    pub(super) unsafe fn update(crc: u32, data: &[u8]) -> u32 {
        let mut state = !crc;
        let mut chunks = data.chunks_exact(8);
        for chunk in &mut chunks {
            let word = u64::from_le_bytes([
                chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
            ]);
            state = __crc32d(state, word);
        }
        for &byte in chunks.remainder() {
            state = __crc32b(state, byte);
        }
        !state
    }
}
