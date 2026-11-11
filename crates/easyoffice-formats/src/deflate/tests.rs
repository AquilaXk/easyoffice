use super::*;

#[test]
fn test_deflate_and_inflate_fixed_stored_and_zlib_auto() {
    // RFC 1950 standard Adler-32 test vector: "Wikipedia" -> 0x11E60398
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);

    let xml = b"<w:document><w:body><w:p><w:r><w:t>EasyOffice RFC 1951 DEFLATE</w:t></w:p></w:body></w:document>"
        .repeat(8);
    let compressed = deflate_raw(&xml);
    assert!(compressed.len() < xml.len() / 3);
    let decompressed = inflate_raw(&compressed, 1024 * 1024).unwrap();
    assert_eq!(decompressed, xml);

    let zlib_full = deflate_zlib(&xml);
    assert_eq!(inflate_zlib(&zlib_full, 1024 * 1024).unwrap(), xml);
    assert_eq!(inflate_auto(&zlib_full, 1024 * 1024).unwrap(), xml);

    let mut bad_zlib = zlib_full.clone();
    let last_idx = bad_zlib.len() - 1;
    bad_zlib[last_idx] ^= 0xFF;
    assert!(matches!(
        inflate_zlib(&bad_zlib, 1024 * 1024),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(matches!(
        inflate_auto(&bad_zlib, 1024 * 1024),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let stored_stream = [0x01u8, 0x05, 0x00, 0xFA, 0xFF, b'H', b'e', b'l', b'l', b'o'];
    assert_eq!(inflate_raw(&stored_stream, 1024).unwrap(), b"Hello");

    let mut zlib_wrapped = vec![0x78, 0x9C];
    zlib_wrapped.extend_from_slice(&compressed);
    assert!(matches!(
        inflate_zlib(&zlib_wrapped, 1024 * 1024),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert_eq!(inflate_auto(&zlib_wrapped, 1024 * 1024).unwrap(), xml);
    zlib_wrapped.push(0x11);
    assert!(matches!(
        inflate_auto(&zlib_wrapped, 1024 * 1024),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    assert!(matches!(
        inflate_raw(&[0x07], 1024),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    assert!(matches!(
        inflate_raw(&compressed, 16),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let sparse_zeros = vec![0u8; 32_768];
    let compressed_zeros = deflate_raw(&sparse_zeros);
    assert!(compressed_zeros.len() * 100 < sparse_zeros.len());
    let inflated_zeros = inflate_raw(&compressed_zeros, 65_536).unwrap();
    assert_eq!(inflated_zeros, sparse_zeros);
}

#[test]
fn test_deflate_ring_buffer_overlap_doubling_and_adler32_chunks() {
    for period in [2usize, 3, 5, 7, 13] {
        let pattern: Vec<u8> = (0..period).map(|i| b'A' + (i as u8)).collect();
        let repeated = pattern.repeat(300);
        let compressed = deflate_raw(&repeated);
        let decompressed = inflate_raw(&compressed, repeated.len() + 64).unwrap();
        assert_eq!(decompressed, repeated);
    }

    let multi_window: Vec<u8> = (0..100_000usize)
        .map(|i| ((i * 31 + (i >> 7)) & 0x7F) as u8)
        .collect();
    let z = deflate_zlib(&multi_window);
    let roundtrip = inflate_zlib(&z, 200_000).unwrap();
    assert_eq!(roundtrip, multi_window);

    let mut s1 = 1u32;
    let mut s2 = 0u32;
    for &b in &multi_window {
        s1 = (s1 + u32::from(b)) % ADLER_MOD;
        s2 = (s2 + s1) % ADLER_MOD;
    }
    assert_eq!(adler32(&multi_window), (s2 << 16) | s1);
}

/// Deterministic pseudo-random bytes (xorshift) so the test needs no external RNG.
fn pseudo_random(len: usize, mut seed: u32) -> Vec<u8> {
    (0..len)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed >> 8) as u8
        })
        .collect()
}

#[test]
fn test_compressor_roundtrips_every_level_on_varied_inputs() {
    let text =
        b"<w:p><w:r><w:t>EasyOffice dynamic huffman block selection</w:t></w:r></w:p>".repeat(900);
    let mut mixed = pseudo_random(40_000, 7);
    mixed.extend_from_slice(&text);
    mixed.extend(vec![0u8; 70_000]);
    mixed.extend(pseudo_random(5_000, 9));
    let inputs: Vec<Vec<u8>> = vec![
        Vec::new(),
        vec![42],
        b"ab".to_vec(),
        b"abc".to_vec(),
        vec![7u8; 300],
        vec![0u8; 200_000],
        pseudo_random(100_000, 1),
        text,
        mixed,
        (0..=255u8).cycle().take(150_000).collect(),
    ];
    for level in 1..=4u8 {
        for input in &inputs {
            let packed = deflate_raw_level(input, level);
            let unpacked = inflate_raw(&packed, input.len() + 16).unwrap();
            assert_eq!(&unpacked, input, "level {level} len {}", input.len());
            // Incompressible data must not expand beyond stored-block overhead.
            assert!(packed.len() <= input.len() + input.len() / 16 + 16);
        }
    }
}

#[test]
fn test_default_level_is_deterministic_and_dynamic_blocks_beat_fixed_on_text() {
    let text = b"<w:t>alpha beta gamma delta epsilon</w:t>".repeat(2000);
    let a = deflate_raw(&text);
    let b = deflate_raw(&text);
    assert_eq!(a, b);
    assert!(a.len() * 20 < text.len());
    let z = deflate_zlib(&text);
    assert_eq!(inflate_zlib(&z, text.len() + 1).unwrap(), text);
}

const FUZZ_ITERATIONS: u32 = 400;
const FUZZ_BUDGET_SLACK: usize = 64;

fn fuzz_corpus() -> Vec<Vec<u8>> {
    let text = b"<row r=\"1\"><c r=\"A1\" t=\"s\"><v>12</v></c><c r=\"B1\"><v>3.5</v></c></row>"
        .repeat(300);
    let mut mixed = pseudo_random(3_000, 11);
    mixed.extend_from_slice(&text);
    mixed.extend(vec![0u8; 5_000]);
    vec![
        b"a".to_vec(),
        text,
        mixed,
        vec![9u8; 4_000],
        pseudo_random(6_000, 5),
        (0..=255u8).cycle().take(9_000).collect(),
    ]
}

/// Applies one deterministic mutation (bit flip or truncation) selected by `seed`.
fn mutate(packed: &[u8], seed: &mut u32) -> Vec<u8> {
    let draw = pseudo_random(3, *seed);
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let mut out = packed.to_vec();
    let pos = (u32::from(draw[0]) << 8 | u32::from(draw[1])) as usize % out.len();
    if draw[2] % 4 == 0 {
        out.truncate(pos);
    } else {
        out[pos] ^= 1 << (draw[2] % 8);
    }
    out
}

fn assert_matches_reference(stream: &[u8], budget: usize, context: &str) {
    let fast = inflate_raw(stream, budget);
    let reference = reference_inflate::reference_inflate_raw(stream, budget);
    match (fast, reference) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "{context}"),
        (Err(_), Err(_)) => {}
        (a, b) => panic!(
            "{context}: new={:?} reference={:?}",
            a.map(|v| v.len()),
            b.map(|v| v.len())
        ),
    }
}

#[test]
fn test_inflate_matches_reference_under_bit_flips_and_truncations() {
    let mut seed = 0x9E37_79B9u32;
    for input in fuzz_corpus() {
        for level in 1..=4u8 {
            let packed = deflate_raw_level(&input, level);
            assert_matches_reference(&packed, input.len() + FUZZ_BUDGET_SLACK, "pristine");
            // Budget one byte short must fail closed in both implementations.
            assert_matches_reference(&packed, input.len().saturating_sub(1), "tight budget");
            for iteration in 0..FUZZ_ITERATIONS {
                let mutated = mutate(&packed, &mut seed);
                let budget = input.len() + FUZZ_BUDGET_SLACK;
                assert_matches_reference(
                    &mutated,
                    budget,
                    &format!("level {level} iter {iteration}"),
                );
            }
        }
    }
}

#[test]
fn test_inflate_roundtrips_every_level_against_reference() {
    for input in fuzz_corpus() {
        for level in 1..=4u8 {
            let packed = deflate_raw_level(&input, level);
            let fast = inflate_raw(&packed, input.len()).unwrap();
            assert_eq!(fast, input, "level {level}");
            assert_eq!(
                reference_inflate::reference_inflate_raw(&packed, input.len()).unwrap(),
                fast
            );
        }
    }
}

#[test]
fn test_inflate_raw_sized_treats_the_declared_size_only_as_a_hint() {
    let input: Vec<u8> = (0..=250u8).cycle().take(50_000).collect();
    let packed = deflate_raw(&input);
    // Exact, too small and too large hints all give the same output.
    for hint in [input.len(), 10, 10_000_000] {
        assert_eq!(
            inflate_raw_sized(&packed, input.len(), hint).unwrap(),
            input
        );
    }
    // The output budget still cuts off a stream that produces more than declared.
    assert!(inflate_raw_sized(&packed, input.len() - 1, 10).is_err());
    assert!(inflate_raw_sized(&packed, 1_000, 10_000_000).is_err());
}

#[test]
fn test_initial_output_allocation_is_bounded_whatever_the_container_declares() {
    const BOUND: usize = 4 * 1024 * 1024 + OUT_SLACK;
    const GIB_HALF: usize = 512 * 1024 * 1024;
    // A tiny stream whose ZIP header claims 512 MiB, and a 100 MB stream without any claim.
    assert!(OutBuf::new(100, GIB_HALF, GIB_HALF).buf.len() <= BOUND);
    assert!(OutBuf::new(100_000_000, GIB_HALF, 0).buf.len() <= BOUND);
    // An honest small claim is still allocated exactly once.
    assert_eq!(
        OutBuf::new(100, GIB_HALF, 1_000).buf.len(),
        1_000 + OUT_SLACK
    );
}

#[test]
fn test_output_grows_toward_the_declared_size_without_passing_it() {
    let input: Vec<u8> = (0..=250u8).cycle().take(20 * 1024 * 1024).collect();
    let packed = deflate_raw(&input);
    let mut out = OutBuf::new(packed.len(), 64 * 1024 * 1024, input.len());
    out.reserve_extra(input.len());
    assert!(
        out.buf.len() <= input.len() + OUT_SLACK,
        "growth stops at the declared size ({} > {})",
        out.buf.len(),
        input.len() + OUT_SLACK
    );
    assert_eq!(
        inflate_raw_sized(&packed, input.len(), input.len()).unwrap(),
        input
    );
}
