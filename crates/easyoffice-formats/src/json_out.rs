//! JSON text output in one place: string escaping and serialization go through `serde_json`.
//!
//! The readers of the IPC payloads stay the policy parsers of `strict_json` (duplicate keys, depth,
//! exact number text); every writer that used to assemble JSON by hand appends through these
//! functions. Compared with the escapers they replace, `serde_json` writes `\b` and `\f` for U+0008
//! and U+000C where the old ones wrote `\u0008` and `\u000c`; both parse to the same text.

use serde::Serialize;
use serde_json::ser::{CharEscape, Formatter, Serializer};
use std::io;

/// `String` as an `io::Write` target; `serde_json` only writes whole UTF-8 fragments.
struct StringSink<'a>(&'a mut String);

impl io::Write for StringSink<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = std::str::from_utf8(buf).map_err(|_| io::ErrorKind::InvalidData)?;
        self.0.push_str(text);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Writes the fragments and escapes of one string straight into a `String` (no quotes, no UTF-8
/// revalidation: `serde_json` hands over `&str` fragments cut at character boundaries).
struct Unquoted<'a>(&'a mut String);

impl Formatter for Unquoted<'_> {
    fn begin_string<W: ?Sized + io::Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        Ok(())
    }

    fn end_string<W: ?Sized + io::Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        Ok(())
    }

    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        _writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        self.0.push_str(fragment);
        Ok(())
    }

    fn write_char_escape<W: ?Sized + io::Write>(
        &mut self,
        _writer: &mut W,
        escape: CharEscape,
    ) -> io::Result<()> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        match escape {
            CharEscape::Quote => self.0.push_str("\\\""),
            CharEscape::ReverseSolidus => self.0.push_str("\\\\"),
            CharEscape::Solidus => self.0.push_str("\\/"),
            CharEscape::Backspace => self.0.push_str("\\b"),
            CharEscape::FormFeed => self.0.push_str("\\f"),
            CharEscape::LineFeed => self.0.push_str("\\n"),
            CharEscape::CarriageReturn => self.0.push_str("\\r"),
            CharEscape::Tab => self.0.push_str("\\t"),
            CharEscape::AsciiControl(byte) => {
                self.0.push_str("\\u00");
                self.0.push(char::from(HEX[usize::from(byte >> 4)]));
                self.0.push(char::from(HEX[usize::from(byte & 0xF)]));
            }
        }
        Ok(())
    }
}

/// Appends `text` with the JSON string escapes applied and no quotes around it.
pub fn escape_json_into(out: &mut String, text: &str) {
    let mut serializer = Serializer::with_formatter(io::sink(), Unquoted(out));
    // A `&str` always serializes; the sink never fails.
    let _ = text.serialize(&mut serializer);
}

/// Appends `text` as a quoted JSON string.
pub fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    escape_json_into(out, text);
    out.push('"');
}

/// Appends `value` as compact JSON. Plain data (structs, sequences, maps with string keys) always
/// serializes.
pub fn push_json<T: Serialize + ?Sized>(out: &mut String, value: &T) {
    let _ = serde_json::to_writer(StringSink(out), value);
}

/// A JSON array of the items of a cloneable iterator (nothing is collected first).
pub struct Items<I>(pub I);

impl<I> Serialize for Items<I>
where
    I: Iterator + Clone,
    I::Item: Serialize,
{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.clone())
    }
}

/// An object whose keys keep the order of the pairs (a `serde_json` map would sort them).
pub struct OrderedObject<'a, V>(pub &'a [(&'a str, V)]);

impl<V: Serialize> Serialize for OrderedObject<'_, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The escapes every former hand-written escaper agreed on, plus the short forms for U+0008
    /// and U+000C.
    fn reference(text: &str) -> String {
        let mut out = String::new();
        for ch in text.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }

    #[test]
    fn escapes_match_the_reference_for_every_control_quote_and_multibyte_character() {
        let mut text = String::new();
        for c in (0u32..0x80).chain([
            0xA0, 0x2028, 0x2029, 0xD7FF, 0xE000, 0xFFFD, 0x1F600, 0x10FFFF,
        ]) {
            text.push(char::from_u32(c).unwrap());
        }
        for len in 0..=text.chars().count() {
            let part: String = text.chars().take(len).collect();
            let mut out = String::new();
            escape_json_into(&mut out, &part);
            assert_eq!(out, reference(&part), "len {len}");
        }
        let mut quoted = String::from("x");
        push_json_string(&mut quoted, "a\"b\u{7f}한");
        assert_eq!(quoted, "x\"a\\\"b\u{7f}한\"");
    }

    #[test]
    fn escaped_text_parses_back_through_the_strict_parser() {
        let text = "line1\nline2\t\"q\" \\ \u{8}\u{c}\u{1f} 한글 😀";
        let mut json = String::new();
        push_json_string(&mut json, text);
        let parsed = crate::StrictJsonValue::parse(&json).unwrap();
        assert_eq!(parsed.as_str(), Some(text));
    }

    #[test]
    fn ordered_objects_keep_their_key_order_and_structs_their_field_order() {
        #[derive(Serialize)]
        struct Pair {
            zebra: u32,
            #[serde(skip_serializing_if = "String::is_empty")]
            apple: String,
        }
        let mut out = String::new();
        push_json(&mut out, &OrderedObject(&[("b", 1), ("a", 2)]));
        out.push(' ');
        push_json(
            &mut out,
            &Pair {
                zebra: 1,
                apple: String::new(),
            },
        );
        assert_eq!(out, "{\"b\":1,\"a\":2} {\"zebra\":1}");
    }
}
