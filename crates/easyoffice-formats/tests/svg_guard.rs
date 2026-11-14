//! SVG pictures reach a parser only after `checked_svg_text`: no declaration, bounded inflation.

use easyoffice_formats::deflate::deflate_raw;
use easyoffice_formats::{checked_svg_text, EasyOfficeError, IeeeCrc32};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#;
const BOMB: &str = r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY e0 "AAAAAAAAAA"><!ENTITY e1 "&e0;&e0;&e0;&e0;&e0;&e0;&e0;&e0;&e0;&e0;">]><svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text>&e1;</text></svg>"#;

/// A gzip member with the given optional header fields set.
fn gzip(text: &[u8], flags: u8, fields: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 8, flags, 0, 0, 0, 0, 0, 3];
    out.extend_from_slice(fields);
    out.extend_from_slice(&deflate_raw(text));
    out.extend_from_slice(&IeeeCrc32::checksum(text).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out
}

fn is_declaration(r: Result<impl Sized, EasyOfficeError>) -> bool {
    matches!(r, Err(EasyOfficeError::XxeOrExternalEntityDetected))
}

#[test]
fn plain_and_gzip_svgs_pass() {
    assert_eq!(checked_svg_text(SVG.as_bytes(), 1 << 20).unwrap(), SVG);
    assert_eq!(
        checked_svg_text(&gzip(SVG.as_bytes(), 0, &[]), 1 << 20).unwrap(),
        SVG
    );
    // Optional header fields: extra (2-byte length), name, comment, header CRC.
    let mut fields = vec![3, 0, 1, 2, 3];
    fields.extend_from_slice(b"name\0comment\0");
    fields.extend_from_slice(&[0, 0]);
    let flags = 0x04 | 0x08 | 0x10 | 0x02;
    assert_eq!(
        checked_svg_text(&gzip(SVG.as_bytes(), flags, &fields), 1 << 20).unwrap(),
        SVG
    );
}

#[test]
fn a_declaration_is_refused_plain_or_inside_gzip() {
    assert!(is_declaration(checked_svg_text(BOMB.as_bytes(), 1 << 20)));
    assert!(is_declaration(checked_svg_text(
        &gzip(BOMB.as_bytes(), 0, &[]),
        1 << 20
    )));
    for doctype in ["<!doctype svg>", "<!  DOCTYPE svg>", "<!ENTITY a \"b\">"] {
        let svg = format!("{doctype}{SVG}");
        assert!(
            is_declaration(checked_svg_text(svg.as_bytes(), 1 << 20)),
            "{doctype}"
        );
    }
    // The words in a comment or CDATA section are only text.
    let quoted = format!("<!-- <!DOCTYPE x> -->{SVG}");
    assert!(checked_svg_text(quoted.as_bytes(), 1 << 20).is_ok());
}

#[test]
fn inflation_stops_at_the_budget() {
    let big = format!("<svg>{}</svg>", " ".repeat(8 << 20));
    let packed = gzip(big.as_bytes(), 0, &[]);
    assert!(
        packed.len() < 64 * 1024,
        "the bomb is small: {}",
        packed.len()
    );
    assert!(matches!(
        checked_svg_text(&packed, 1 << 20),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    assert!(checked_svg_text(&packed, 16 << 20).is_ok());
    let plain = vec![b' '; (1 << 20) + 1];
    assert!(matches!(
        checked_svg_text(&plain, 1 << 20),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn damaged_gzip_and_non_utf8_are_refused() {
    let good = gzip(SVG.as_bytes(), 0, &[]);
    for bad in [
        vec![0x1F, 0x8B],
        vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0],
        good[..12].to_vec(),
        [&[0x1F, 0x8B, 7][..], &good[3..]].concat(),
        [&[0x1F, 0x8B, 8, 0xE0][..], &good[4..]].concat(),
        [
            &[0x1F, 0x8B, 8, 0x04, 0, 0, 0, 0, 0, 3, 0xFF, 0xFF][..],
            &good[10..],
        ]
        .concat(),
        [&[0x1F, 0x8B, 8, 0x08, 0, 0, 0, 0, 0, 3][..], b"never ends"].concat(),
    ] {
        assert!(checked_svg_text(&bad, 1 << 20).is_err(), "{bad:?}");
    }
    assert!(checked_svg_text(&[b'<', 0xFF, 0xFE], 1 << 20).is_err());
    assert!(checked_svg_text(&gzip(&[b'<', 0xFF, 0xFE], 0, &[]), 1 << 20).is_err());
    // Gzip inside gzip is not unpacked a second time: the inner bytes are not UTF-8 text.
    let nested = gzip(&good, 0, &[]);
    assert!(checked_svg_text(&nested, 1 << 20).is_err());
}

#[test]
fn forged_gzip_crc32_isize_and_trailing_bytes_fail_closed() {
    let good = gzip(SVG.as_bytes(), 0, &[]);
    let len = good.len();
    assert!(len > 18);

    // Tampered CRC32 trailer byte.
    let mut bad_crc = good.clone();
    bad_crc[len - 6] ^= 0x01;
    assert!(checked_svg_text(&bad_crc, 1 << 20).is_err());

    // Tampered ISIZE trailer byte.
    let mut bad_isize = good.clone();
    bad_isize[len - 2] ^= 0x01;
    assert!(checked_svg_text(&bad_isize, 1 << 20).is_err());

    // Truncated trailer (fewer than 8 bytes after deflate stream).
    assert!(checked_svg_text(&good[..len - 4], 1 << 20).is_err());

    // Unexpected trailing garbage bytes after trailer.
    let mut extra = good.clone();
    extra.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
    assert!(checked_svg_text(&extra, 1 << 20).is_err());
}
