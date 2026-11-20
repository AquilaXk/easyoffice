//! SHA-1, SHA-256, SHA-384 and SHA-512 (FIPS 180-4) for the password checks of sheet and workbook
//! protection (PD-216), and the salted, iterated hash of the agile password scheme
//! (ECMA-376 Part 1 18.2.28 `workbookProtection`, 18.3.1.85 `sheetProtection`; the same loop as
//! MS-OFFCRYPTO 2.3.4.11): `H0 = hash(salt || password)`, `Hn = hash(H(n-1) || n as u32 LE)` for
//! `n` in `0..spinCount`.
//!
//! A thin adapter over the RustCrypto `sha1` and `sha2` crates (MIT OR Apache-2.0), which use the
//! CPU's SHA instructions where they exist. This module keeps the algorithm-name lookup, the spin
//! loop with its deadline, and a constant-time comparison.

use std::time::Instant;

use sha1::Sha1 as Sha1Core;
use sha2::digest::FixedOutputReset;
use sha2::{Digest, Sha256, Sha384, Sha512};

/// The hash functions a password digest may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sha {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Sha {
    /// The function an `algorithmName` of the file names (`SHA-512`, ...); `None` for anything else.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "SHA-1" | "SHA1" => Some(Self::Sha1),
            "SHA-256" | "SHA256" => Some(Self::Sha256),
            "SHA-384" | "SHA384" => Some(Self::Sha384),
            "SHA-512" | "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }

    pub(crate) const fn len(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }

    pub(crate) fn digest(self, message: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => Sha1Core::digest(message).to_vec(),
            Self::Sha256 => Sha256::digest(message).to_vec(),
            Self::Sha384 => Sha384::digest(message).to_vec(),
            Self::Sha512 => Sha512::digest(message).to_vec(),
        }
    }

    /// `Hn = hash(H(n-1) || n as u32 LE)` for `n` in `0..rounds`, starting from `seed` (a digest of
    /// this function). Stops early with `None` when `deadline` passes, checked every 4,096 rounds.
    pub(crate) fn iterate(
        self,
        seed: &[u8],
        rounds: u32,
        deadline: Option<Instant>,
    ) -> Option<Vec<u8>> {
        match self {
            Self::Sha1 => iterate_with::<Sha1Core>(seed, rounds, deadline),
            Self::Sha256 => iterate_with::<Sha256>(seed, rounds, deadline),
            Self::Sha384 => iterate_with::<Sha384>(seed, rounds, deadline),
            Self::Sha512 => iterate_with::<Sha512>(seed, rounds, deadline),
        }
    }
}

/// The spin loop for one hash function; the hasher is reset in place so no round allocates.
fn iterate_with<D: Digest + FixedOutputReset>(
    seed: &[u8],
    rounds: u32,
    deadline: Option<Instant>,
) -> Option<Vec<u8>> {
    let mut hasher = D::new();
    let mut current = seed.to_vec();
    for round in 0..rounds {
        if round & 0xFFF == 0 && deadline.is_some_and(|d| Instant::now() >= d) {
            return None;
        }
        Digest::update(&mut hasher, &current);
        Digest::update(&mut hasher, round.to_le_bytes());
        current.copy_from_slice(&Digest::finalize_reset(&mut hasher));
    }
    Some(current)
}

/// Whether `a` and `b` are equal, looking at every byte of the shorter-or-equal length so the time
/// does not tell where they first differ.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn standard_vectors() {
        assert_eq!(
            hex(&Sha::Sha1.digest(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&Sha::Sha256.digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&Sha::Sha384.digest(b"abc")),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
        assert_eq!(
            hex(&Sha::Sha512.digest(b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        assert_eq!(
            hex(&Sha::Sha512.digest(b"")),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert_eq!(
            hex(&Sha::Sha256.digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn block_boundaries_and_million_a() {
        // 111 and 112 bytes are the two sides of the SHA-512 padding boundary; 55 and 56 of SHA-256.
        assert_eq!(
            hex(&Sha::Sha512.digest(&[b'a'; 111])),
            "fa9121c7b32b9e01733d034cfc78cbf67f926c7ed83e82200ef86818196921760b4beff48404df811b953828274461673c68d04e297b0eb7b2b4d60fc6b566a2"
        );
        assert_eq!(
            hex(&Sha::Sha256.digest(&[b'a'; 55])),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            hex(&Sha::Sha256.digest(&vec![b'a'; 1_000_000])),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(
            hex(&Sha::Sha1.digest(&vec![b'a'; 1_000_000])),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    #[test]
    fn iterate_matches_the_plain_loop() {
        for sha in [Sha::Sha1, Sha::Sha256, Sha::Sha384, Sha::Sha512] {
            let seed = sha.digest(b"salt-and-password");
            let mut expected = seed.clone();
            for round in 0u32..37 {
                let mut message = expected.clone();
                message.extend_from_slice(&round.to_le_bytes());
                expected = sha.digest(&message);
            }
            assert_eq!(sha.iterate(&seed, 37, None).unwrap(), expected, "{sha:?}");
        }
    }

    #[test]
    fn iterate_stops_at_a_passed_deadline() {
        let seed = Sha::Sha512.digest(b"x");
        assert!(Sha::Sha512
            .iterate(&seed, 10_000, Some(Instant::now()))
            .is_none());
    }

    #[test]
    fn comparison_looks_at_every_byte() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }
}
