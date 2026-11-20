//! IEEE 802.3 CRC-32 (`0xEDB88320` reflected polynomial) used by ZIP, PNG and gzip containers.
//!
//! A thin adapter over the `crc32fast` crate (MIT OR Apache-2.0), which picks PCLMULQDQ on x86_64
//! and the CRC32 instruction on aarch64 at run time and falls back to a scalar core elsewhere.

/// IEEE 802.3 CRC-32 checksum calculator.
pub struct IeeeCrc32;

impl IeeeCrc32 {
    /// CRC-32 of `bytes`.
    #[inline]
    pub fn checksum(bytes: &[u8]) -> u32 {
        crc32fast::hash(bytes)
    }

    /// Continues a CRC-32 (the value returned by [`Self::checksum`] / a previous `update`)
    /// over more bytes; `update(0, data)` equals `checksum(data)`.
    #[inline]
    pub fn update(crc: u32, bytes: &[u8]) -> u32 {
        let mut hasher = crc32fast::Hasher::new_with_initial(crc);
        hasher.update(bytes);
        hasher.finalize()
    }
}
