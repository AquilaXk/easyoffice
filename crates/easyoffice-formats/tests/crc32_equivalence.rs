//! CRC-32 equivalence fuzzing: the one-shot and the incremental `crc32fast` adapter must equal the
//! bitwise reference for random lengths and alignments, driven by a deterministic PRNG.

use easyoffice_formats::IeeeCrc32;

fn bitwise_reference(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn known_vectors() {
    assert_eq!(IeeeCrc32::checksum(b""), 0);
    assert_eq!(IeeeCrc32::checksum(b"123456789"), 0xCBF4_3926);
    assert_eq!(
        IeeeCrc32::checksum(b"The quick brown fox jumps over the lazy dog"),
        0x414F_A339
    );
    assert_eq!(IeeeCrc32::checksum(&[0u8; 32]), 0x190A_55AD);
    assert_eq!(IeeeCrc32::checksum(&[0xFFu8; 32]), 0xFF6C_AB0B);
}

#[test]
fn dispatch_matches_the_bitwise_reference_on_random_lengths_and_alignments() {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut pool = vec![0u8; 9_000];
    for chunk in pool.chunks_mut(8) {
        let word = rng.next().to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
    for _ in 0..1_500 {
        let len = (rng.next() % 4_200) as usize;
        let offset = (rng.next() % 64) as usize;
        let data = &pool[offset..offset + len];
        let expected = bitwise_reference(data);
        assert_eq!(IeeeCrc32::checksum(data), expected, "dispatch len={len}");
    }
}

#[test]
fn boundary_lengths_cover_every_fold_and_slice_edge() {
    let mut rng = XorShift(0xD1B5_4A32_D192_ED03);
    let data: Vec<u8> = (0..700).map(|_| rng.next() as u8).collect();
    for len in 0..=data.len() {
        let expected = bitwise_reference(&data[..len]);
        assert_eq!(IeeeCrc32::checksum(&data[..len]), expected, "len={len}");
    }
}

#[test]
fn incremental_update_matches_one_shot_for_arbitrary_splits() {
    let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
    let data: Vec<u8> = (0..5_000).map(|_| rng.next() as u8).collect();
    let expected = bitwise_reference(&data);
    for _ in 0..200 {
        let a = (rng.next() % 5_000) as usize;
        let b = a + (rng.next() % (5_000 - a as u64 + 1)) as usize;
        let mut crc = IeeeCrc32::update(0, &data[..a]);
        crc = IeeeCrc32::update(crc, &data[a..b]);
        crc = IeeeCrc32::update(crc, &data[b..]);
        assert_eq!(crc, expected, "split a={a} b={b}");
    }
}

#[test]
fn megabyte_inputs_match_the_bitwise_reference() {
    let mut rng = XorShift(0x6C07_8965_D1A4_F87B);
    let data: Vec<u8> = (0..(1 << 20) + 13).map(|_| rng.next() as u8).collect();
    assert_eq!(IeeeCrc32::checksum(&data), bitwise_reference(&data));
}
