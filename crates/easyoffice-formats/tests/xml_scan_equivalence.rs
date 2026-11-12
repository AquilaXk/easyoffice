//! The wide byte search and the `!`/`?`-first XXE scan give the same answers as byte-at-a-time
//! references.

use easyoffice_formats::XmlSanitizer;

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
fn find_byte_swar_matches_a_plain_search_on_every_start_and_length() {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    for len in 0..140usize {
        let mut bytes: Vec<u8> = (0..len).map(|_| b'a' + (rng.next() % 3) as u8).collect();
        // Sparse targets exercise the wide path: place at most two at random offsets.
        for _ in 0..(rng.next() % 3) {
            if len > 0 {
                let at = (rng.next() as usize) % len;
                bytes[at] = b'!';
            }
        }
        for start in 0..=len + 1 {
            let expected = bytes
                .get(start..)
                .and_then(|tail| tail.iter().position(|&b| b == b'!'))
                .map(|p| p + start);
            assert_eq!(
                XmlSanitizer::find_byte_swar(&bytes, start, b'!'),
                expected,
                "len {len} start {start}"
            );
        }
    }
}

#[test]
fn xxe_scan_matches_the_byte_reference() {
    const PIECES: [&str; 20] = [
        "<",
        "!",
        "<!",
        "ENTITY",
        "DOCTYPE",
        "entity",
        " ",
        "\n",
        "a",
        "<a>",
        "!--",
        "]]>",
        "<?",
        "?>",
        "?",
        "-->",
        "--",
        "<!--",
        "<![CDATA[",
        "[",
    ];
    let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
    for _ in 0..200_000 {
        let mut xml = String::new();
        for _ in 0..(rng.next() % 16) {
            xml.push_str(PIECES[(rng.next() as usize) % PIECES.len()]);
        }
        let fast = XmlSanitizer::validate_safe_xml(&xml);
        assert_eq!(
            fast,
            XmlSanitizer::validate_safe_xml_scalar_baseline(&xml),
            "{xml:?}"
        );
    }
    let padded = format!("{}<!DOCTYPE x>{}", "x".repeat(300), "y".repeat(100));
    assert!(XmlSanitizer::validate_safe_xml(&padded).is_err());
    let tail = format!("{}<!ENTITY", "x".repeat(300));
    assert!(XmlSanitizer::validate_safe_xml(&tail).is_err());
    assert!(XmlSanitizer::validate_safe_xml(&"<a>x</a>".repeat(100)).is_ok());
}
