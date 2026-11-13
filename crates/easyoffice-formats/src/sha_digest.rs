//! SHA-1, SHA-256, SHA-384 and SHA-512 (FIPS 180-4) for the password checks of sheet and workbook
//! protection (PD-216), and the salted, iterated hash of the agile password scheme
//! (ECMA-376 Part 1 18.2.28 `workbookProtection`, 18.3.1.85 `sheetProtection`; the same loop as
//! MS-OFFCRYPTO 2.3.4.11): `H0 = hash(salt || password)`, `Hn = hash(H(n-1) || n as u32 LE)` for
//! `n` in `0..spinCount`.
//!
//! The engine crates have no third-party dependency (PD-053), so the algorithms are written here from
//! the FIPS text; the tests check the standard vectors and a long message. Safe Rust only. The
//! iteration hashes one block per round (a 64-byte digest and a counter fit one 128-byte block), so it
//! rewrites the changing bytes of one buffer and allocates nothing per round.

use std::time::Instant;

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

    const fn block(self) -> usize {
        match self {
            Self::Sha1 | Self::Sha256 => 64,
            Self::Sha384 | Self::Sha512 => 128,
        }
    }

    pub(crate) fn digest(self, message: &[u8]) -> Vec<u8> {
        let mut state = State::new(self);
        let block = self.block();
        let mut chunks = message.chunks_exact(block);
        for chunk in &mut chunks {
            state.compress(chunk);
        }
        // The rest, `0x80`, zeros and the bit length (8 bytes; 16 for the 128-byte block).
        let rest = chunks.remainder();
        let length_bytes = block / 8;
        let mut tail = vec![
            0u8;
            if rest.len() + 1 + length_bytes > block {
                2 * block
            } else {
                block
            }
        ];
        tail[..rest.len()].copy_from_slice(rest);
        tail[rest.len()] = 0x80;
        let bits = (message.len() as u128) * 8;
        let end = tail.len();
        tail[end - length_bytes..].copy_from_slice(&bits.to_be_bytes()[16 - length_bytes..]);
        for chunk in tail.chunks_exact(block) {
            state.compress(chunk);
        }
        state.output()
    }

    /// `Hn = hash(H(n-1) || n as u32 LE)` for `n` in `0..rounds`, starting from `seed` (a digest of
    /// this function). Stops early with `None` when `deadline` passes, checked every 4,096 rounds.
    pub(crate) fn iterate(
        self,
        seed: &[u8],
        rounds: u32,
        deadline: Option<Instant>,
    ) -> Option<Vec<u8>> {
        let n = self.len();
        debug_assert_eq!(seed.len(), n);
        let block = self.block();
        let length_bytes = block / 8;
        // One block: digest, counter, `0x80`, zeros, bit length of `n + 4` bytes.
        let mut buf = vec![0u8; block];
        buf[..n].copy_from_slice(seed);
        buf[n + 4] = 0x80;
        let bits = ((n + 4) as u128) * 8;
        buf[block - length_bytes..].copy_from_slice(&bits.to_be_bytes()[16 - length_bytes..]);
        let mut digest = seed.to_vec();
        for round in 0..rounds {
            if round & 0xFFF == 0 && deadline.is_some_and(|d| Instant::now() >= d) {
                return None;
            }
            buf[..n].copy_from_slice(&digest);
            buf[n..n + 4].copy_from_slice(&round.to_le_bytes());
            let mut state = State::new(self);
            state.compress(&buf);
            digest = state.output();
        }
        Some(digest)
    }
}

enum State {
    Sha1([u32; 5]),
    Sha256([u32; 8]),
    Wide([u64; 8], usize),
}

impl State {
    fn new(sha: Sha) -> Self {
        match sha {
            Sha::Sha1 => Self::Sha1([
                0x6745_2301,
                0xEFCD_AB89,
                0x98BA_DCFE,
                0x1032_5476,
                0xC3D2_E1F0,
            ]),
            Sha::Sha256 => Self::Sha256([
                0x6A09_E667,
                0xBB67_AE85,
                0x3C6E_F372,
                0xA54F_F53A,
                0x510E_527F,
                0x9B05_688C,
                0x1F83_D9AB,
                0x5BE0_CD19,
            ]),
            Sha::Sha384 => Self::Wide(
                [
                    0xCBBB_9D5D_C105_9ED8,
                    0x629A_292A_367C_D507,
                    0x9159_015A_3070_DD17,
                    0x152F_ECD8_F70E_5939,
                    0x6733_2667_FFC0_0B31,
                    0x8EB4_4A87_6858_1511,
                    0xDB0C_2E0D_64F9_8FA7,
                    0x47B5_481D_BEFA_4FA4,
                ],
                48,
            ),
            Sha::Sha512 => Self::Wide(
                [
                    0x6A09_E667_F3BC_C908,
                    0xBB67_AE85_84CA_A73B,
                    0x3C6E_F372_FE94_F82B,
                    0xA54F_F53A_5F1D_36F1,
                    0x510E_527F_ADE6_82D1,
                    0x9B05_688C_2B3E_6C1F,
                    0x1F83_D9AB_FB41_BD6B,
                    0x5BE0_CD19_137E_2179,
                ],
                64,
            ),
        }
    }

    fn compress(&mut self, block: &[u8]) {
        match self {
            Self::Sha1(s) => compress_sha1(s, block),
            Self::Sha256(s) => compress_sha256(s, block),
            Self::Wide(s, _) => compress_sha512(s, block),
        }
    }

    fn output(&self) -> Vec<u8> {
        match self {
            Self::Sha1(s) => s.iter().flat_map(|w| w.to_be_bytes()).collect(),
            Self::Sha256(s) => s.iter().flat_map(|w| w.to_be_bytes()).collect(),
            Self::Wide(s, len) => {
                let mut out: Vec<u8> = s.iter().flat_map(|w| w.to_be_bytes()).collect();
                out.truncate(*len);
                out
            }
        }
    }
}

fn compress_sha1(state: &mut [u32; 5], block: &[u8]) {
    let mut w = [0u32; 80];
    for (i, word) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }
    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (i, &word) in w.iter().enumerate() {
        let (f, k) = match i {
            0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
            20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
            _ => (b ^ c ^ d, 0xCA62_C1D6),
        };
        let t = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = t;
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e]) {
        *s = s.wrapping_add(v);
    }
}

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn compress_sha256(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (i, word) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let mut v = *state;
    for i in 0..64 {
        let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
        let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
        let t1 = v[7]
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K256[i])
            .wrapping_add(w[i]);
        let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
        let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
        let t2 = s0.wrapping_add(maj);
        v = [
            t1.wrapping_add(t2),
            v[0],
            v[1],
            v[2],
            v[3].wrapping_add(t1),
            v[4],
            v[5],
            v[6],
        ];
    }
    for (s, x) in state.iter_mut().zip(v) {
        *s = s.wrapping_add(x);
    }
}

const K512: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

fn compress_sha512(state: &mut [u64; 8], block: &[u8]) {
    let mut w = [0u64; 80];
    for (i, word) in block.chunks_exact(8).enumerate() {
        w[i] = u64::from_be_bytes([
            word[0], word[1], word[2], word[3], word[4], word[5], word[6], word[7],
        ]);
    }
    for i in 16..80 {
        let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
        let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let mut v = *state;
    for i in 0..80 {
        let s1 = v[4].rotate_right(14) ^ v[4].rotate_right(18) ^ v[4].rotate_right(41);
        let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
        let t1 = v[7]
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K512[i])
            .wrapping_add(w[i]);
        let s0 = v[0].rotate_right(28) ^ v[0].rotate_right(34) ^ v[0].rotate_right(39);
        let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
        let t2 = s0.wrapping_add(maj);
        v = [
            t1.wrapping_add(t2),
            v[0],
            v[1],
            v[2],
            v[3].wrapping_add(t1),
            v[4],
            v[5],
            v[6],
        ];
    }
    for (s, x) in state.iter_mut().zip(v) {
        *s = s.wrapping_add(x);
    }
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
