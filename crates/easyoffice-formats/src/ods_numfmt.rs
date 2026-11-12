//! OpenDocument number styles (`number:number-style`, `-percentage-`, `-date-`, `-time-`,
//! `-text-style`) as spreadsheet format codes (PD-191).
//!
//! The writer turns the positive section of a code into the ODF element structure other
//! programs show, and keeps the full code in the `eo:format-code` attribute so a reopen here
//! restores it exactly. The reader prefers that attribute and otherwise rebuilds a code from the
//! elements (ODF 1.3 section 16: `number:number`, `number:scientific-number`, `number:text`,
//! `number:year` ... `number:am-pm`). A code the writer cannot express (fractions, `[h]` elapsed
//! time, thousands scaling) is written as a plain number style and read back through the
//! attribute.

use std::collections::HashMap;

use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_odf_blocks_in_order};
use crate::XmlSanitizer;

/// Namespace of the `eo:` extension attributes the writer declares.
pub(crate) const EO_NS: &str = "urn:easyoffice:odf-extension:1";
/// Namespace of the data style elements.
pub(crate) const NUMBER_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0";

const STYLE_TAGS: [&str; 6] = [
    "number:number-style",
    "number:percentage-style",
    "number:currency-style",
    "number:date-style",
    "number:time-style",
    "number:text-style",
];

/// One piece of a data style, in code order.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Number {
        int_digits: usize,
        decimals: usize,
        grouping: bool,
    },
    Scientific {
        int_digits: usize,
        decimals: usize,
        exp_digits: usize,
    },
    Year(bool),
    /// Month number (`long` = two digits) or name (`textual`: 3 = abbreviation, 4 = full).
    Month {
        long: bool,
        textual: u8,
    },
    Day(bool),
    Weekday(bool),
    Hours(bool),
    Minutes(bool),
    Seconds(bool),
    AmPm,
    TextContent,
    /// `%`: multiplies by 100 and prints the sign.
    Percent,
}

/// Why a code has no ODF structure.
struct Unsupported;

fn run_len(chars: &[char], at: usize) -> usize {
    chars[at..].iter().take_while(|&&c| c == chars[at]).count()
}

fn push_text(pieces: &mut Vec<Piece>, text: &str) {
    if let Some(Piece::Text(last)) = pieces.last_mut() {
        last.push_str(text);
    } else {
        pieces.push(Piece::Text(text.to_string()));
    }
}

/// The part of a code before the first section separator outside quotes.
fn positive_section(code: &str) -> &str {
    let (mut quoted, mut escaped) = (false, false);
    for (at, ch) in code.char_indices() {
        if escaped {
            escaped = false;
        } else if !quoted && (ch == '\\' || ch == '_' || ch == '*') {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch == ';' && !quoted {
            return &code[..at];
        }
    }
    code
}

/// A run of digit placeholders: `#,##0.00` -> number piece.
fn digit_run(run: &str) -> Result<Piece, Unsupported> {
    if run.contains('?') {
        return Err(Unsupported);
    }
    let (int_part, frac_part) = run.split_once('.').unwrap_or((run, ""));
    // A comma after the last placeholder scales by 1000; the structure has no such attribute.
    if int_part.ends_with(',') || frac_part.contains(',') {
        return Err(Unsupported);
    }
    Ok(Piece::Number {
        int_digits: int_part.chars().filter(|&c| c == '0').count(),
        decimals: frac_part
            .chars()
            .filter(|&c| matches!(c, '0' | '#'))
            .count(),
        grouping: int_part.contains(','),
    })
}

/// Splits the positive section of `code` into pieces; `Err` for what ODF has no element for.
fn pieces_of(code: &str) -> Result<Vec<Piece>, Unsupported> {
    let chars: Vec<char> = positive_section(code).chars().collect();
    let mut pieces = Vec::new();
    let mut at = 0usize;
    while at < chars.len() {
        let ch = chars[at];
        match ch {
            '"' => {
                let end = chars[at + 1..].iter().position(|&c| c == '"');
                let end = end.map_or(chars.len(), |n| at + 1 + n);
                push_text(&mut pieces, &chars[at + 1..end].iter().collect::<String>());
                at = end + 1;
            }
            '\\' => {
                if let Some(next) = chars.get(at + 1) {
                    push_text(&mut pieces, &next.to_string());
                }
                at += 2;
            }
            '_' => {
                push_text(&mut pieces, " ");
                at += 2;
            }
            '*' => at += 2,
            '[' => {
                let end = chars[at..].iter().position(|&c| c == ']').map(|n| at + n);
                let end = end.ok_or(Unsupported)?;
                let inner: String = chars[at + 1..end].iter().collect();
                if let Some(currency) = inner.strip_prefix('$') {
                    let symbol = currency.split('-').next().unwrap_or("");
                    push_text(&mut pieces, symbol);
                } else if matches!(
                    inner.to_ascii_lowercase().as_str(),
                    "h" | "m" | "s" | "hh" | "mm" | "ss"
                ) {
                    return Err(Unsupported);
                }
                at = end + 1;
            }
            '0' | '#' | '?' | ',' | '.' if ch != '.' && ch != ',' || digits_follow(&chars, at) => {
                let len = chars[at..]
                    .iter()
                    .take_while(|&&c| matches!(c, '0' | '#' | '?' | ',' | '.'))
                    .count();
                let run: String = chars[at..at + len].iter().collect();
                at += len;
                let exponent = matches!(chars.get(at), Some('E' | 'e'))
                    && matches!(chars.get(at + 1), Some('+' | '-'));
                if exponent {
                    let zeros = chars[at + 2..].iter().take_while(|&&c| c == '0').count();
                    let Piece::Number {
                        int_digits,
                        decimals,
                        ..
                    } = digit_run(&run)?
                    else {
                        return Err(Unsupported);
                    };
                    pieces.push(Piece::Scientific {
                        int_digits,
                        decimals,
                        exp_digits: zeros.max(1),
                    });
                    at += 2 + zeros;
                } else {
                    pieces.push(digit_run(&run)?);
                }
            }
            '%' => {
                pieces.push(Piece::Percent);
                at += 1;
            }
            '@' => {
                pieces.push(Piece::TextContent);
                at += 1;
            }
            'y' | 'Y' => {
                let len = run_len(&chars, at);
                pieces.push(Piece::Year(len >= 3));
                at += len;
            }
            'm' | 'M' => {
                let len = run_len(&chars, at);
                pieces.push(Piece::Month {
                    long: len == 2,
                    textual: if len >= 3 { len.min(4) as u8 } else { 0 },
                });
                at += len;
            }
            'd' | 'D' => {
                let len = run_len(&chars, at);
                pieces.push(match len {
                    1 | 2 => Piece::Day(len == 2),
                    _ => Piece::Weekday(len >= 4),
                });
                at += len;
            }
            'h' | 'H' => {
                let len = run_len(&chars, at);
                pieces.push(Piece::Hours(len >= 2));
                at += len;
            }
            's' | 'S' => {
                let len = run_len(&chars, at);
                pieces.push(Piece::Seconds(len >= 2));
                at += len;
            }
            'A' | 'a' | 'P' | 'p' => {
                let rest: String = chars[at..]
                    .iter()
                    .take(5)
                    .collect::<String>()
                    .to_ascii_uppercase();
                if rest.starts_with("AM/PM") {
                    pieces.push(Piece::AmPm);
                    at += 5;
                } else if rest.starts_with("A/P") {
                    pieces.push(Piece::AmPm);
                    at += 3;
                } else {
                    return Err(Unsupported);
                }
            }
            'G' | 'g'
                if chars[at..]
                    .iter()
                    .take(7)
                    .collect::<String>()
                    .eq_ignore_ascii_case("general") =>
            {
                pieces.push(Piece::Number {
                    int_digits: 1,
                    decimals: 0,
                    grouping: false,
                });
                at += 7;
            }
            _ => {
                push_text(&mut pieces, &ch.to_string());
                at += 1;
            }
        }
    }
    resolve_minutes(&mut pieces);
    Ok(pieces)
}

/// A `.` or `,` starts a digit run only when a placeholder sits next to it.
fn digits_follow(chars: &[char], at: usize) -> bool {
    let is_digit = |c: Option<&char>| c.is_some_and(|c| matches!(c, '0' | '#' | '?'));
    is_digit(chars.get(at + 1)) || (at > 0 && is_digit(chars.get(at - 1)))
}

/// `m` after hours or before seconds is minutes, not a month.
fn resolve_minutes(pieces: &mut [Piece]) {
    let kind = |p: &Piece| !matches!(p, Piece::Text(_));
    for at in 0..pieces.len() {
        let Piece::Month { long, textual: 0 } = pieces[at] else {
            continue;
        };
        let before = pieces[..at].iter().rev().find(|p| kind(p));
        let after = pieces[at + 1..].iter().find(|p| kind(p));
        if matches!(before, Some(Piece::Hours(_))) || matches!(after, Some(Piece::Seconds(_))) {
            pieces[at] = Piece::Minutes(long);
        }
    }
}

fn style_tag(pieces: &[Piece]) -> &'static str {
    let any = |f: fn(&Piece) -> bool| pieces.iter().any(f);
    if any(|p| {
        matches!(
            p,
            Piece::Year(_) | Piece::Month { .. } | Piece::Day(_) | Piece::Weekday(_)
        )
    }) {
        "number:date-style"
    } else if any(|p| {
        matches!(
            p,
            Piece::Hours(_) | Piece::Minutes(_) | Piece::Seconds(_) | Piece::AmPm
        )
    }) {
        "number:time-style"
    } else if any(|p| matches!(p, Piece::TextContent)) {
        "number:text-style"
    } else if any(|p| matches!(p, Piece::Percent)) {
        "number:percentage-style"
    } else {
        "number:number-style"
    }
}

fn long_attr(long: bool) -> &'static str {
    if long {
        " number:style=\"long\""
    } else {
        ""
    }
}

fn piece_xml(piece: &Piece, out: &mut String) {
    match piece {
        Piece::Text(text) => {
            out.push_str(&format!("<number:text>{}</number:text>", XmlSanitizer::escape(text)));
        }
        Piece::Number { int_digits, decimals, grouping } => {
            let group = if *grouping { " number:grouping=\"true\"" } else { "" };
            out.push_str(&format!(
                "<number:number number:decimal-places=\"{decimals}\" number:min-integer-digits=\"{int_digits}\"{group}/>"
            ));
        }
        Piece::Scientific { int_digits, decimals, exp_digits } => out.push_str(&format!(
            "<number:scientific-number number:decimal-places=\"{decimals}\" number:min-integer-digits=\"{int_digits}\" number:min-exponent-digits=\"{exp_digits}\"/>"
        )),
        Piece::Year(long) => out.push_str(&format!("<number:year{}/>", long_attr(*long))),
        Piece::Month { long, textual } if *textual > 0 => out.push_str(&format!(
            "<number:month number:textual=\"true\"{}/>",
            long_attr(*textual >= 4)
        )),
        Piece::Month { long, .. } => out.push_str(&format!("<number:month{}/>", long_attr(*long))),
        Piece::Day(long) => out.push_str(&format!("<number:day{}/>", long_attr(*long))),
        Piece::Weekday(long) => out.push_str(&format!("<number:day-of-week{}/>", long_attr(*long))),
        Piece::Hours(long) => out.push_str(&format!("<number:hours{}/>", long_attr(*long))),
        Piece::Minutes(long) => out.push_str(&format!("<number:minutes{}/>", long_attr(*long))),
        Piece::Seconds(long) => out.push_str(&format!("<number:seconds{}/>", long_attr(*long))),
        Piece::AmPm => out.push_str("<number:am-pm/>"),
        Piece::Percent => out.push_str("<number:text>%</number:text>"),
        Piece::TextContent => out.push_str("<number:text-content/>"),
    }
}

/// The data style element named `name` for `code`; a code with no ODF structure becomes a plain
/// number style that still carries the code.
pub(crate) fn data_style_xml(name: &str, code: &str) -> String {
    let (tag, body) = match pieces_of(code) {
        Ok(pieces) if !pieces.is_empty() => {
            let mut body = String::new();
            pieces.iter().for_each(|p| piece_xml(p, &mut body));
            (style_tag(&pieces), body)
        }
        _ => (
            "number:number-style",
            "<number:number number:min-integer-digits=\"1\"/>".to_string(),
        ),
    };
    format!(
        "<{tag} style:name=\"{}\" eo:format-code=\"{}\">{body}</{tag}>",
        XmlSanitizer::escape(name),
        XmlSanitizer::escape(code)
    )
}

/// Characters a format code shows as they are, without quotes.
const BARE_LITERALS: &str = "-+():$/ ";

fn literal(text: &str) -> String {
    if text.chars().all(|c| BARE_LITERALS.contains(c)) {
        text.to_string()
    } else {
        format!("\"{}\"", text.replace('"', ""))
    }
}

fn code_piece(tag: &str, attrs: &str, body: &str, out: &mut String) {
    let attr = |name: &str| extract_attr_from_fragment(attrs, name);
    let long = attr("number:style") == Some("long");
    let count = |name: &str| {
        attr(name)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
            .min(30)
    };
    match tag {
        "number:text" | "number:currency-symbol" => {
            let text = XmlSanitizer::unescape(body);
            if !text.is_empty() {
                out.push_str(&literal(&text));
            }
        }
        "number:number" | "number:scientific-number" => {
            let int_digits = count("number:min-integer-digits");
            let grouping = attr("number:grouping") == Some("true");
            let int = if grouping {
                format!("#,##{}", "0".repeat(int_digits.max(1)))
            } else if int_digits == 0 {
                "#".to_string()
            } else {
                "0".repeat(int_digits)
            };
            let decimals = count("number:decimal-places");
            out.push_str(&int);
            if decimals > 0 {
                out.push('.');
                out.push_str(&"0".repeat(decimals));
            }
            if tag == "number:scientific-number" {
                out.push_str("E+");
                out.push_str(&"0".repeat(count("number:min-exponent-digits").max(1)));
            }
        }
        "number:year" => out.push_str(if long { "yyyy" } else { "yy" }),
        "number:month" if attr("number:textual") == Some("true") => {
            out.push_str(if long { "mmmm" } else { "mmm" });
        }
        "number:month" => out.push_str(if long { "mm" } else { "m" }),
        "number:day" => out.push_str(if long { "dd" } else { "d" }),
        "number:day-of-week" => out.push_str(if long { "dddd" } else { "ddd" }),
        "number:hours" => out.push_str(if long { "hh" } else { "h" }),
        "number:minutes" => out.push_str(if long { "mm" } else { "m" }),
        "number:seconds" => out.push_str(if long { "ss" } else { "s" }),
        "number:am-pm" => out.push_str("AM/PM"),
        "number:text-content" => out.push('@'),
        _ => {}
    }
}

const PIECE_TAGS: [&str; 14] = [
    "number:text",
    "number:currency-symbol",
    "number:number",
    "number:scientific-number",
    "number:year",
    "number:month",
    "number:day",
    "number:day-of-week",
    "number:hours",
    "number:minutes",
    "number:seconds",
    "number:am-pm",
    "number:text-content",
    "number:fraction",
];

/// The code a data style element stands for; `None` for an empty or fraction style.
fn code_of_style(tag: &str, body: &str) -> Option<String> {
    let mut code = String::new();
    let percentage = tag == "number:percentage-style";
    let mut percent_seen = false;
    for (piece, attrs, inner) in extract_odf_blocks_in_order(body, &PIECE_TAGS) {
        if piece == "number:fraction" {
            return None;
        }
        if percentage && piece == "number:text" && inner == "%" {
            code.push('%');
            percent_seen = true;
        } else {
            code_piece(piece, attrs, inner, &mut code);
        }
    }
    if percentage && !percent_seen {
        code.push('%');
    }
    (!code.is_empty()).then_some(code)
}

/// Format code by style name of every data style in `automatic_styles_xml`.
pub(crate) fn read_data_styles(automatic_styles_xml: &str) -> HashMap<String, String> {
    let mut codes = HashMap::new();
    for (tag, attrs, body) in extract_odf_blocks_in_order(automatic_styles_xml, &STYLE_TAGS) {
        let Some(name) = extract_attr_from_fragment(attrs, "style:name") else {
            continue;
        };
        let code = extract_attr_from_fragment(attrs, "eo:format-code")
            .map(XmlSanitizer::unescape)
            .or_else(|| code_of_style(tag, body));
        if let Some(code) = code {
            codes.insert(XmlSanitizer::unescape(name), code);
        }
    }
    codes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure(code: &str) -> String {
        data_style_xml("n1", code)
    }

    #[test]
    fn number_codes_become_number_elements() {
        let xml = structure("#,##0.00");
        assert!(xml.starts_with("<number:number-style style:name=\"n1\""));
        assert!(xml.contains(
            "number:decimal-places=\"2\" number:min-integer-digits=\"1\" number:grouping=\"true\""
        ));
        let xml = structure("\"₩\"#,##0");
        assert!(xml.contains("<number:text>₩</number:text>"));
        assert!(xml.contains("number:decimal-places=\"0\""));
    }

    #[test]
    fn percent_date_time_and_text_pick_their_style_kind() {
        assert!(structure("0.0%").starts_with("<number:percentage-style"));
        let date = structure("yyyy-mm-dd");
        assert!(date.starts_with("<number:date-style"));
        assert!(date.contains("<number:year number:style=\"long\"/>"));
        assert!(date.contains("<number:month number:style=\"long\"/>"));
        let time = structure("h:mm:ss AM/PM");
        assert!(time.starts_with("<number:time-style"));
        assert!(time.contains("<number:minutes number:style=\"long\"/>"));
        assert!(time.contains("<number:am-pm/>"));
        assert!(structure("@").starts_with("<number:text-style"));
        assert!(structure("0.00E+00").contains("<number:scientific-number"));
    }

    #[test]
    fn unsupported_codes_stay_readable_through_the_attribute() {
        for code in ["# ?/?", "[h]:mm:ss", "#,##0,"] {
            let xml = structure(code);
            assert!(xml.contains("number:min-integer-digits=\"1\""), "{code}");
            let codes = read_data_styles(&xml);
            assert_eq!(codes.get("n1").map(String::as_str), Some(code));
        }
    }

    #[test]
    fn written_styles_read_back_the_same_code() {
        for code in [
            "0.00;[Red]-0.00",
            "\"₩\"#,##0_);[Red](\"₩\"#,##0)",
            "yyyy\"년\" m\"월\"",
            "@",
        ] {
            let codes = read_data_styles(&structure(code));
            assert_eq!(codes.get("n1").map(String::as_str), Some(code));
        }
    }

    #[test]
    fn foreign_styles_read_from_their_elements() {
        let xml = concat!(
            "<number:number-style style:name=\"N2\"><number:number number:decimal-places=\"2\" number:min-integer-digits=\"1\"/></number:number-style>",
            "<number:percentage-style style:name=\"N11\"><number:number number:decimal-places=\"0\" number:min-integer-digits=\"1\"/><number:text>%</number:text></number:percentage-style>",
            "<number:currency-style style:name=\"N104\"><number:currency-symbol>₩</number:currency-symbol><number:number number:decimal-places=\"0\" number:min-integer-digits=\"1\" number:grouping=\"true\"/></number:currency-style>",
            "<number:date-style style:name=\"N30\"><number:year number:style=\"long\"/><number:text>-</number:text><number:month number:style=\"long\"/><number:text>-</number:text><number:day number:style=\"long\"/></number:date-style>",
            "<number:time-style style:name=\"N40\"><number:hours/><number:text>:</number:text><number:minutes number:style=\"long\"/></number:time-style>",
        );
        let codes = read_data_styles(xml);
        assert_eq!(codes["N2"], "0.00");
        assert_eq!(codes["N11"], "0%");
        assert_eq!(codes["N104"], "\"₩\"#,##0");
        assert_eq!(codes["N30"], "yyyy-mm-dd");
        assert_eq!(codes["N40"], "h:mm");
    }
}
