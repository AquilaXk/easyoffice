//! 64-bit keyed hash of a byte slice for deduplication keys (never a security boundary: callers
//! compare the bytes of every hash match). Four independent lanes take 32 bytes per step so the
//! multiplies overlap, and the per-process random `seed` keeps a crafted input from colliding on
//! purpose and forcing byte comparisons.

use std::hash::BuildHasher;

const LANE_MUL: u64 = 0x9E37_79B1_85EB_CA87;
const LANE_ADD: [u64; 4] = [
    0x243F_6A88_85A3_08D3,
    0x1319_8A2E_0370_7344,
    0xA409_3822_299F_31D0,
    0x082E_FA98_EC4E_6C89,
];
const FINAL_MUL: [u64; 2] = [0xFF51_AFD7_ED55_8CCD, 0xC4CE_B9FE_1A85_EC53];
const STEP_BYTES: usize = 32;
const LANE_ROTATE: u32 = 27;

/// A random seed, fixed for the life of the value that keeps it.
pub(crate) fn random_seed() -> u64 {
    std::collections::hash_map::RandomState::new().hash_one(0u8)
}

#[inline]
fn mix(lane: u64, word: u64) -> u64 {
    (lane ^ word)
        .wrapping_mul(LANE_MUL)
        .rotate_left(LANE_ROTATE)
}

/// The 64-bit hash of `bytes` under `seed`; the length is part of the hash.
pub(crate) fn hash_bytes(bytes: &[u8], seed: u64) -> u64 {
    let mut lanes = LANE_ADD.map(|add| add ^ seed);
    let mut steps = bytes.chunks_exact(STEP_BYTES);
    for step in &mut steps {
        for (lane, word) in lanes.iter_mut().zip(step.chunks_exact(8)) {
            *lane = mix(*lane, u64::from_le_bytes(word.try_into().unwrap_or([0; 8])));
        }
    }
    let mut tail = steps.remainder().chunks_exact(8);
    let mut acc =
        lanes[0] ^ lanes[1].rotate_left(13) ^ lanes[2].rotate_left(29) ^ lanes[3].rotate_left(47);
    for word in &mut tail {
        acc = mix(acc, u64::from_le_bytes(word.try_into().unwrap_or([0; 8])));
    }
    let mut last = [0u8; 8];
    let rest = tail.remainder();
    last[..rest.len()].copy_from_slice(rest);
    acc = mix(
        acc,
        u64::from_le_bytes(last) ^ (bytes.len() as u64).rotate_left(56),
    );
    acc ^= acc >> 33;
    acc = acc.wrapping_mul(FINAL_MUL[0]);
    acc ^= acc >> 33;
    acc = acc.wrapping_mul(FINAL_MUL[1]);
    acc ^ (acc >> 33)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 + 7) as u8).collect()
    }

    #[test]
    fn equal_bytes_hash_equal_and_a_seed_changes_the_hash() {
        let bytes = sample(1000);
        assert_eq!(hash_bytes(&bytes, 5), hash_bytes(&bytes.clone(), 5));
        assert_ne!(hash_bytes(&bytes, 5), hash_bytes(&bytes, 6));
    }

    #[test]
    fn every_single_bit_flip_and_every_length_changes_the_hash() {
        for len in [0usize, 1, 7, 8, 9, 31, 32, 33, 40, 64, 65, 100] {
            let bytes = sample(len);
            let base = hash_bytes(&bytes, 1);
            for at in 0..len {
                for bit in 0..8 {
                    let mut flipped = bytes.clone();
                    flipped[at] ^= 1 << bit;
                    assert_ne!(
                        hash_bytes(&flipped, 1),
                        base,
                        "len {len} byte {at} bit {bit}"
                    );
                }
            }
        }
        let lengths: std::collections::HashSet<u64> =
            (0..200).map(|len| hash_bytes(&vec![0u8; len], 1)).collect();
        assert_eq!(lengths.len(), 200, "zero runs of different lengths");
    }

    #[test]
    fn the_random_seed_is_stable_within_a_value() {
        let seed = random_seed();
        let bytes = sample(50);
        assert_eq!(hash_bytes(&bytes, seed), hash_bytes(&bytes, seed));
    }
}
