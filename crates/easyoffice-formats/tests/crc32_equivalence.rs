//! CRC-32 equivalence fuzzing: every implementation (slicing-by-16, byte-wise baseline,
//! runtime-detected hardware path, incremental update) must equal the bitwise reference for
//! random lengths and alignments, driven by a deterministic PRNG.

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
fn every_path_matches_the_bitwise_reference_on_random_lengths_and_alignments() {
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
        assert_eq!(
            IeeeCrc32::checksum_scalar(data),
            expected,
            "scalar len={len} offset={offset}"
        );
        assert_eq!(
            IeeeCrc32::checksum_byte_by_byte_baseline(data),
            expected,
            "baseline len={len}"
        );
        if let Some(hw) = IeeeCrc32::checksum_hardware(data) {
            assert_eq!(hw, expected, "hardware len={len} offset={offset}");
        }
    }
}

#[test]
fn boundary_lengths_cover_every_fold_and_slice_edge() {
    let mut rng = XorShift(0xD1B5_4A32_D192_ED03);
    let data: Vec<u8> = (0..700).map(|_| rng.next() as u8).collect();
    for len in 0..=data.len() {
        let expected = bitwise_reference(&data[..len]);
        assert_eq!(IeeeCrc32::checksum(&data[..len]), expected, "len={len}");
        assert_eq!(IeeeCrc32::checksum_scalar(&data[..len]), expected);
        if let Some(hw) = IeeeCrc32::checksum_hardware(&data[..len]) {
            assert_eq!(hw, expected, "hardware len={len}");
        }
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
fn megabyte_inputs_match_across_paths() {
    let mut rng = XorShift(0x6C07_8965_D1A4_F87B);
    let data: Vec<u8> = (0..(1 << 20) + 13).map(|_| rng.next() as u8).collect();
    let expected = IeeeCrc32::checksum_byte_by_byte_baseline(&data);
    assert_eq!(IeeeCrc32::checksum(&data), expected);
    assert_eq!(IeeeCrc32::checksum_scalar(&data), expected);
    if let Some(hw) = IeeeCrc32::checksum_hardware(&data) {
        assert_eq!(hw, expected);
    }
}

#[test]
#[ignore]
fn bench_crc32_release() {
    let mut rng = XorShift(0x5A17_9C3E_8B14_D207);
    let data: Vec<u8> = (0..(1 << 20)).map(|_| rng.next() as u8).collect();
    let rounds = 200u32;

    let t0 = std::time::Instant::now();
    let mut acc_base = 0u32;
    for _ in 0..rounds {
        acc_base ^= IeeeCrc32::checksum_byte_by_byte_baseline(std::hint::black_box(&data));
    }
    let dt_base = t0.elapsed();

    let t1 = std::time::Instant::now();
    let mut acc_scalar = 0u32;
    for _ in 0..rounds {
        acc_scalar ^= IeeeCrc32::checksum_scalar(std::hint::black_box(&data));
    }
    let dt_scalar = t1.elapsed();

    let t2 = std::time::Instant::now();
    let mut acc_fast = 0u32;
    for _ in 0..rounds {
        acc_fast ^= IeeeCrc32::checksum(std::hint::black_box(&data));
    }
    let dt_fast = t2.elapsed();

    assert_eq!(acc_base, acc_scalar);
    assert_eq!(acc_base, acc_fast);
    let mb = f64::from(rounds);
    println!(
        "bench_crc32_release ({}): baseline {:.2} ms ({:.0} MB/s) | scalar-slice16 {:.2} ms ({:.0} MB/s) | dispatch {:.2} ms ({:.0} MB/s)",
        IeeeCrc32::hardware_path_name(),
        dt_base.as_secs_f64() * 1000.0,
        mb / dt_base.as_secs_f64(),
        dt_scalar.as_secs_f64() * 1000.0,
        mb / dt_scalar.as_secs_f64(),
        dt_fast.as_secs_f64() * 1000.0,
        mb / dt_fast.as_secs_f64(),
    );
}
