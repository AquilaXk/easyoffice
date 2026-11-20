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
fn test_repeating_patterns_multi_window_input_and_adler32() {
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
    assert_eq!(inflate_zlib(&z, 200_000).unwrap(), multi_window);

    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in &multi_window {
        s1 = (s1 + u32::from(b)) % 65_521;
        s2 = (s2 + s1) % 65_521;
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
fn test_roundtrip_on_varied_inputs_and_determinism() {
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
        vec![7u8; 300],
        vec![0u8; 200_000],
        pseudo_random(100_000, 1),
        text,
        mixed,
        (0..=255u8).cycle().take(150_000).collect(),
    ];
    for input in &inputs {
        let packed = deflate_raw(input);
        assert_eq!(packed, deflate_raw(input), "deterministic");
        if input.is_empty() {
            continue;
        }
        assert_eq!(&inflate_raw(&packed, input.len()).unwrap(), input);
        // Incompressible data must not expand beyond stored-block overhead.
        assert!(packed.len() <= input.len() + input.len() / 16 + 16);
    }
}

#[test]
fn test_bit_flips_and_truncations_never_pass_the_budget_or_panic() {
    let text = b"<row r=\"1\"><c r=\"A1\" t=\"s\"><v>12</v></c></row>".repeat(300);
    let packed = deflate_raw(&text);
    let budget = text.len() + 64;
    let mut seed = 0x9E37_79B9u32;
    for _ in 0..2_000 {
        let draw = pseudo_random(3, seed);
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut mutated = packed.clone();
        let pos = (usize::from(draw[0]) << 8 | usize::from(draw[1])) % mutated.len();
        if draw[2].is_multiple_of(4) {
            mutated.truncate(pos);
        } else {
            mutated[pos] ^= 1 << (draw[2] % 8);
        }
        if let Ok(out) = inflate_raw(&mutated, budget) {
            assert!(out.len() <= budget);
        }
    }
    // One byte short of the real size fails closed.
    assert!(matches!(
        inflate_raw(&packed, text.len() - 1),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    assert_eq!(inflate_raw(&packed, text.len()).unwrap(), text);
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
fn test_a_huge_declared_size_does_not_allocate_before_output_appears() {
    // A tiny stream whose container claims 512 MiB, inside a 512 MiB budget: it still returns at
    // once with its real few bytes.
    let packed = deflate_raw(b"hello hello hello hello");
    let out = inflate_raw_sized(&packed, 512 * 1024 * 1024, 512 * 1024 * 1024).unwrap();
    assert_eq!(out, b"hello hello hello hello");
}

#[test]
fn test_output_grows_to_the_declared_size_and_the_consumed_count_ends_the_stream() {
    let input: Vec<u8> = (0..=250u8).cycle().take(20 * 1024 * 1024).collect();
    let packed = deflate_raw(&input);
    assert_eq!(
        inflate_raw_sized(&packed, input.len(), input.len()).unwrap(),
        input
    );
    let mut with_trailer = packed.clone();
    with_trailer.extend_from_slice(b"TRAIL");
    let (out, consumed) = inflate_raw_with_consumed(&with_trailer, input.len(), 0).unwrap();
    assert_eq!(out.len(), input.len());
    assert_eq!(consumed, packed.len());
}
