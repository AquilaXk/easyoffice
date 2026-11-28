//! `is_number` answers what the parse it replaced answered (PD-397): trim, drop the commas, and `f64::from_str`.

use super::is_number;

fn by_parsing(text: &str) -> bool {
    let t = text.trim().replace(',', "");
    !t.is_empty() && t.parse::<f64>().is_ok()
}

#[test]
fn the_words_the_cells_hold_are_told_apart_as_the_parser_told_them() {
    for text in [
        "",
        " ",
        ",",
        ",,",
        "0",
        "-0",
        "+1",
        "1,234",
        "1,234.50",
        "12,34,567",
        " 42 ",
        "\t7\n",
        ".5",
        "5.",
        ".",
        "-.",
        "+.5e3",
        "1e5",
        "1E-5",
        "1e",
        "1e+",
        "e5",
        "1.2.3",
        "1,2,e,3",
        "inf",
        "-inf",
        "+Inf",
        "INFINITY",
        "infinit",
        "infinityx",
        "nan",
        "-NaN",
        "nanx",
        "n",
        "i",
        "abc",
        "12abc",
        "１２",
        "٣",
        "1 2",
        "0x10",
        "1_000",
        "--1",
        "+-1",
        "1e5.5",
        "1e5e5",
        "9".repeat(400).as_str(),
        "1e99999999999999999999",
        "한글",
        "$5",
        "5%",
        "(5)",
        "1,000,000.00",
        "−5",
    ] {
        assert_eq!(is_number(text), by_parsing(text), "{text:?}");
    }
}

#[test]
fn random_texts_of_the_number_alphabet_agree_with_the_parser() {
    const ALPHABET: &[u8] = b"0123456789+-.,eEiInNfFaAtTyY x";
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut numbers = 0;
    for _ in 0..600_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let len = 1 + (state >> 60) as usize % 9;
        let text: String = (0..len)
            .map(|at| {
                char::from(ALPHABET[((state >> (at * 6)) as usize ^ (at * 7)) % ALPHABET.len()])
            })
            .collect();
        let expected = by_parsing(&text);
        numbers += usize::from(expected);
        assert_eq!(is_number(&text), expected, "{text:?}");
    }
    assert!(numbers > 1_000, "the sample holds numbers: {numbers}");
}
