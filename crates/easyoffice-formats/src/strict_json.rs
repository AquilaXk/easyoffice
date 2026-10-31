use std::collections::HashSet;

use crate::EasyOfficeError;

const MAX_JSON_NESTING_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictJsonValue {
    Null,
    Bool(bool),
    Number(String),
    Str(String),
    Array(Vec<StrictJsonValue>),
    Object(Vec<(String, StrictJsonValue)>),
}

impl StrictJsonValue {
    pub fn parse(input: &str) -> Result<Self, EasyOfficeError> {
        let trimmed = input.strip_prefix('\u{FEFF}').unwrap_or(input);
        let mut chars = trimmed.chars().peekable();
        let root = Self::parse_value(&mut chars, 0).ok_or_else(|| {
            EasyOfficeError::InvalidArgument("유효하지 않은 RFC 8259 JSON 구문입니다.".to_string())
        })?;
        Self::skip_ws(&mut chars);
        if chars.peek().is_some() {
            return Err(EasyOfficeError::InvalidArgument(
                "JSON 루트 값 뒤에 허용되지 않는 후행 토큰이 존재합니다.".to_string(),
            ));
        }
        Ok(root)
    }

    pub fn as_object(&self) -> Option<&[(String, StrictJsonValue)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[StrictJsonValue]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        let Self::Number(n) = self else {
            return None;
        };
        if let Ok(v) = n.parse::<u64>() {
            return Some(v);
        }
        let (is_neg, unsigned_str) = match n.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, n.as_str()),
        };
        let (mantissa, exp_opt) = match unsigned_str.find(['e', 'E']) {
            Some(pos) => (&unsigned_str[..pos], Some(&unsigned_str[pos + 1..])),
            None => (unsigned_str, None),
        };
        let (int_part, frac_part) = match mantissa.split_once('.') {
            Some((ip, fp)) => {
                if fp.is_empty() || !fp.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                (ip, fp)
            }
            None => (mantissa, ""),
        };
        if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let exp: i64 = match exp_opt {
            Some(exp_raw) => {
                let (exp_neg, exp_digits) = if let Some(r) = exp_raw.strip_prefix('-') {
                    (true, r)
                } else if let Some(r) = exp_raw.strip_prefix('+') {
                    (false, r)
                } else {
                    (false, exp_raw)
                };
                if exp_digits.is_empty() || !exp_digits.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let mut acc = 0i64;
                for b in exp_digits.bytes() {
                    acc = (acc.saturating_mul(10)).saturating_add(i64::from(b - b'0'));
                    if acc > 1_000_000 {
                        acc = 1_000_000;
                        break;
                    }
                }
                if exp_neg {
                    -acc
                } else {
                    acc
                }
            }
            None => 0,
        };
        let mut raw_digits: Vec<u8> = Vec::with_capacity(int_part.len() + frac_part.len());
        raw_digits.extend_from_slice(int_part.as_bytes());
        raw_digits.extend_from_slice(frac_part.as_bytes());
        let Some(first_nz) = raw_digits.iter().position(|&b| b != b'0') else {
            return Some(0);
        };
        if is_neg {
            return None;
        }
        let last_nz = raw_digits.iter().rposition(|&b| b != b'0')?;
        let trailing_zeros = (raw_digits.len() - 1 - last_nz) as i64;
        let sig = &raw_digits[first_nz..=last_nz];
        let scale = exp - (frac_part.len() as i64) + trailing_zeros;
        if scale < 0 || (sig.len() as i64) + scale > 20 {
            return None;
        }
        let mut val = 0u64;
        for &b in sig {
            val = val.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
        }
        for _ in 0..scale {
            val = val.checked_mul(10)?;
        }
        Some(val)
    }

    pub fn to_scalar_string(&self) -> Option<String> {
        match self {
            Self::Null => Some(String::new()),
            Self::Bool(b) => Some(if *b { "true" } else { "false" }.to_string()),
            Self::Number(n) => Some(n.clone()),
            Self::Str(s) => Some(s.clone()),
            Self::Array(_) | Self::Object(_) => None,
        }
    }

    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_escaped_str(out: &mut String, s: &str) {
        out.push('"');
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{0008}' => out.push_str("\\b"),
                '\u{000C}' => out.push_str("\\f"),
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Number(n) => out.push_str(n),
            Self::Str(s) => Self::write_escaped_str(out, s),
            Self::Array(items) => {
                out.push('[');
                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            Self::Object(entries) => {
                out.push('{');
                for (idx, (k, v)) in entries.iter().enumerate() {
                    if idx > 0 {
                        out.push(',');
                    }
                    Self::write_escaped_str(out, k);
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }

    pub fn get_field<'a>(
        entries: &'a [(String, StrictJsonValue)],
        key: &str,
    ) -> Option<&'a StrictJsonValue> {
        entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_str<'a>(entries: &'a [(String, StrictJsonValue)], key: &str) -> Option<&'a str> {
        Self::get_field(entries, key).and_then(Self::as_str)
    }

    fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
        while chars
            .peek()
            .is_some_and(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
        {
            let _ = chars.next();
        }
    }

    fn parse_string_literal(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ) -> Option<String> {
        let mut out = String::new();
        while let Some(c) = chars.next() {
            if c == '"' {
                return Some(out);
            }
            if (c as u32) < 0x20 {
                return None;
            }
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let mut hex4 = String::with_capacity(4);
                    for _ in 0..4 {
                        hex4.push(chars.next()?);
                    }
                    let hi = u16::from_str_radix(&hex4, 16).ok()?;
                    if (0xD800..=0xDBFF).contains(&hi) {
                        if chars.next()? != '\\' || chars.next()? != 'u' {
                            return None;
                        }
                        let mut lo_hex = String::with_capacity(4);
                        for _ in 0..4 {
                            lo_hex.push(chars.next()?);
                        }
                        let lo = u16::from_str_radix(&lo_hex, 16).ok()?;
                        if !(0xDC00..=0xDFFF).contains(&lo) {
                            return None;
                        }
                        let cp = 0x10000 + (((hi as u32 - 0xD800) << 10) | (lo as u32 - 0xDC00));
                        out.push(char::from_u32(cp)?);
                    } else if (0xDC00..=0xDFFF).contains(&hi) {
                        return None;
                    } else {
                        out.push(char::from_u32(hi as u32)?);
                    }
                }
                _ => return None,
            }
        }
        None
    }

    fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
        let mut raw = String::new();
        if chars.peek() == Some(&'-') {
            raw.push(chars.next()?);
        }
        match chars.next()? {
            '0' => raw.push('0'),
            first @ '1'..='9' => {
                raw.push(first);
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        raw.push(d);
                        let _ = chars.next();
                    } else {
                        break;
                    }
                }
            }
            _ => return None,
        }
        if chars.peek() == Some(&'.') {
            raw.push(chars.next()?);
            if !chars.peek().is_some_and(char::is_ascii_digit) {
                return None;
            }
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    raw.push(d);
                    let _ = chars.next();
                } else {
                    break;
                }
            }
        }
        if chars.peek().is_some_and(|c| *c == 'e' || *c == 'E') {
            raw.push(chars.next()?);
            if chars.peek().is_some_and(|c| *c == '+' || *c == '-') {
                raw.push(chars.next()?);
            }
            if !chars.peek().is_some_and(char::is_ascii_digit) {
                return None;
            }
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    raw.push(d);
                    let _ = chars.next();
                } else {
                    break;
                }
            }
        }
        Some(raw)
    }

    fn consume_keyword(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
        kw: &str,
    ) -> Option<()> {
        for expected in kw.chars() {
            if chars.next()? != expected {
                return None;
            }
        }
        Some(())
    }

    fn parse_value(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
        depth: usize,
    ) -> Option<Self> {
        if depth > MAX_JSON_NESTING_DEPTH {
            return None;
        }
        Self::skip_ws(chars);
        match *chars.peek()? {
            '"' => {
                let _ = chars.next();
                Some(Self::Str(Self::parse_string_literal(chars)?))
            }
            '{' => {
                let _ = chars.next();
                Self::skip_ws(chars);
                let mut entries: Vec<(String, Self)> = Vec::new();
                if chars.peek() == Some(&'}') {
                    let _ = chars.next();
                    return Some(Self::Object(entries));
                }
                let mut seen_keys: HashSet<String> = HashSet::new();
                loop {
                    Self::skip_ws(chars);
                    if chars.next()? != '"' {
                        return None;
                    }
                    let key = Self::parse_string_literal(chars)?;
                    if !seen_keys.insert(key.clone()) {
                        return None;
                    }
                    Self::skip_ws(chars);
                    if chars.next()? != ':' {
                        return None;
                    }
                    let val = Self::parse_value(chars, depth + 1)?;
                    entries.push((key, val));
                    Self::skip_ws(chars);
                    match chars.next()? {
                        ',' => continue,
                        '}' => break,
                        _ => return None,
                    }
                }
                Some(Self::Object(entries))
            }
            '[' => {
                let _ = chars.next();
                Self::skip_ws(chars);
                let mut items = Vec::new();
                if chars.peek() == Some(&']') {
                    let _ = chars.next();
                    return Some(Self::Array(items));
                }
                loop {
                    items.push(Self::parse_value(chars, depth + 1)?);
                    Self::skip_ws(chars);
                    match chars.next()? {
                        ',' => continue,
                        ']' => break,
                        _ => return None,
                    }
                }
                Some(Self::Array(items))
            }
            't' => {
                Self::consume_keyword(chars, "true")?;
                Some(Self::Bool(true))
            }
            'f' => {
                Self::consume_keyword(chars, "false")?;
                Some(Self::Bool(false))
            }
            'n' => {
                Self::consume_keyword(chars, "null")?;
                Some(Self::Null)
            }
            '-' | '0'..='9' => Some(Self::Number(Self::parse_number(chars)?)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc8259_strict_validation_boundaries_and_roundtrip() {
        let deep_ok = format!("{}0{}", "[".repeat(32), "]".repeat(32));
        assert!(StrictJsonValue::parse(&deep_ok).is_ok());
        let deep_err = format!("{}0{}", "[".repeat(33), "]".repeat(33));
        assert!(StrictJsonValue::parse(&deep_err).is_err());

        assert!(StrictJsonValue::parse("\"raw\nnewline\"").is_err());
        assert!(StrictJsonValue::parse("\"\\uD800\"").is_err());
        assert!(StrictJsonValue::parse("\"\\uDC00\"").is_err());
        assert!(StrictJsonValue::parse("\"\\uD83D\\u0041\"").is_err());

        let emoji = StrictJsonValue::parse("\"\\uD83D\\uDE00\"").unwrap();
        assert_eq!(emoji.as_str(), Some("😀"));

        assert_eq!(
            StrictJsonValue::parse("1000.0").unwrap().as_u64(),
            Some(1000)
        );
        assert_eq!(
            StrictJsonValue::parse("1e4").unwrap().as_u64(),
            Some(10_000)
        );
        assert_eq!(StrictJsonValue::parse("-0").unwrap().as_u64(), Some(0));
        assert_eq!(StrictJsonValue::parse("-0.0").unwrap().as_u64(), Some(0));
        assert_eq!(
            StrictJsonValue::parse("1500e-2").unwrap().as_u64(),
            Some(15)
        );
        assert_eq!(
            StrictJsonValue::parse("1e18").unwrap().as_u64(),
            Some(1_000_000_000_000_000_000)
        );
        assert_eq!(
            StrictJsonValue::parse("18446744073709551615")
                .unwrap()
                .as_u64(),
            Some(u64::MAX)
        );
        assert_eq!(
            StrictJsonValue::parse("18446744073709551616")
                .unwrap()
                .as_u64(),
            None
        );
        assert_eq!(
            StrictJsonValue::parse("4503599627370496.5")
                .unwrap()
                .as_u64(),
            None
        );
        assert_eq!(
            StrictJsonValue::parse("1.0000000000000001")
                .unwrap()
                .as_u64(),
            None
        );
        assert_eq!(StrictJsonValue::parse("1e-400").unwrap().as_u64(), None);
        assert_eq!(StrictJsonValue::parse("-1e-400").unwrap().as_u64(), None);
        assert_eq!(StrictJsonValue::parse("1.5").unwrap().as_u64(), None);
        assert_eq!(StrictJsonValue::parse("-1").unwrap().as_u64(), None);
        assert_eq!(StrictJsonValue::parse("1e25").unwrap().as_u64(), None);

        let many_pairs: String = (0..5_000)
            .map(|i| format!("\"k{i}\":{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let big_obj = StrictJsonValue::parse(&format!("{{{many_pairs}}}")).unwrap();
        assert_eq!(big_obj.as_object().unwrap().len(), 5_000);
        assert!(StrictJsonValue::parse("{\"a\":1,\"a\":2}").is_err());

        let complex = StrictJsonValue::parse(
            r#"{"sheet":"'특수시트'","cells":[{"cell":"A1","value":"line1\nline2\u0007"}],"ok":true,"n":null}"#,
        )
        .unwrap();
        let encoded = complex.to_json_string();
        assert_eq!(StrictJsonValue::parse(&encoded).unwrap(), complex);
    }
}
