use std::collections::HashSet;

use crate::EasyOfficeError;

const MAX_JSON_NESTING_DEPTH: usize = 32;
/// Keys an object may hold before duplicate detection switches from a scan to a hash set.
const LINEAR_KEY_LIMIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictJsonValue {
    Null,
    Bool(bool),
    Number(String),
    Str(String),
    Array(Vec<StrictJsonValue>),
    Object(Vec<(String, StrictJsonValue)>),
}

/// Extra digits of headroom added to the exponent clamp.
const EXPONENT_CLAMP_MARGIN: usize = 25;
/// A `u64` has at most 20 decimal digits.
const MAX_U64_DIGITS: usize = 20;

/// Splits a JSON number mantissa into validated integer and fraction digit strings.
fn split_mantissa(mantissa: &str) -> Option<(&str, &str)> {
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((ip, fp)) => {
            if fp.is_empty() || !fp.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            (ip, fp)
        }
        None => (mantissa, ""),
    };
    let valid_int = !int_part.is_empty()
        && int_part.bytes().all(|b| b.is_ascii_digit())
        && !(int_part.len() > 1 && int_part.starts_with('0'));
    valid_int.then_some((int_part, frac_part))
}

/// Parses an exponent (`+5`, `-3`, `12`), clamping its magnitude to `exp_clamp`.
fn parse_clamped_exponent(exp_raw: &str, exp_clamp: i64) -> Option<i64> {
    let (exp_neg, exp_digits) = if let Some(r) = exp_raw.strip_prefix('-') {
        (true, r)
    } else {
        (false, exp_raw.strip_prefix('+').unwrap_or(exp_raw))
    };
    if exp_digits.is_empty() || !exp_digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut acc = 0i64;
    for b in exp_digits.bytes() {
        acc = (acc.saturating_mul(10)).saturating_add(i64::from(b - b'0'));
        if acc > exp_clamp {
            acc = exp_clamp;
            break;
        }
    }
    Some(if exp_neg { -acc } else { acc })
}

/// Converts `int.frac * 10^exp` to `u64` when it is a non-negative integer that fits.
fn decimal_to_u64(is_neg: bool, int_bytes: &[u8], frac_bytes: &[u8], exp: i64) -> Option<u64> {
    let total_digits = int_bytes.len().checked_add(frac_bytes.len())?;
    let digit_at = |idx: usize| -> u8 {
        if idx < int_bytes.len() {
            int_bytes[idx]
        } else {
            frac_bytes[idx - int_bytes.len()]
        }
    };
    let Some(first_nz) = (0..total_digits).find(|&idx| digit_at(idx) != b'0') else {
        return Some(0);
    };
    if is_neg {
        return None;
    }
    let last_nz = (first_nz..total_digits)
        .rev()
        .find(|&idx| digit_at(idx) != b'0')?;
    let sig_len = last_nz - first_nz + 1;
    let trailing_zeros = i64::try_from(total_digits - 1 - last_nz).ok()?;
    let frac_len = i64::try_from(frac_bytes.len()).ok()?;
    let scale = exp.checked_sub(frac_len)?.checked_add(trailing_zeros)?;
    if scale < 0 || sig_len > MAX_U64_DIGITS || (sig_len as i64) + scale > MAX_U64_DIGITS as i64 {
        return None;
    }
    let mut val = 0u64;
    for idx in first_nz..=last_nz {
        val = val
            .checked_mul(10)?
            .checked_add(u64::from(digit_at(idx) - b'0'))?;
    }
    for _ in 0..scale {
        val = val.checked_mul(10)?;
    }
    Some(val)
}

impl StrictJsonValue {
    pub fn parse(input: &str) -> Result<Self, EasyOfficeError> {
        let trimmed = input.strip_prefix('\u{FEFF}').unwrap_or(input);
        let mut cursor = JsonCursor {
            src: trimmed,
            bytes: trimmed.as_bytes(),
            pos: 0,
        };
        let root = cursor.parse_value(0).ok_or_else(|| {
            EasyOfficeError::InvalidArgument("유효하지 않은 RFC 8259 JSON 구문입니다.".to_string())
        })?;
        cursor.skip_ws();
        if cursor.pos < cursor.bytes.len() {
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
        let (is_neg, unsigned_str) = match n.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, n.as_str()),
        };
        let (mantissa, exp_opt) = match unsigned_str.find(['e', 'E']) {
            Some(pos) => (&unsigned_str[..pos], Some(&unsigned_str[pos + 1..])),
            None => (unsigned_str, None),
        };
        let (int_part, frac_part) = split_mantissa(mantissa)?;
        if !is_neg && frac_part.is_empty() && exp_opt.is_none() {
            return int_part.parse::<u64>().ok();
        }
        let total_digits = int_part.len().checked_add(frac_part.len())?;
        let exp_clamp = i64::try_from(total_digits.saturating_add(EXPONENT_CLAMP_MARGIN))
            .unwrap_or(i64::MAX / 4);
        let exp = match exp_opt {
            Some(exp_raw) => parse_clamped_exponent(exp_raw, exp_clamp)?,
            None => 0,
        };
        decimal_to_u64(is_neg, int_part.as_bytes(), frac_part.as_bytes(), exp)
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
}

struct JsonCursor<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonCursor<'a> {
    #[inline]
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n')
        {
            self.pos += 1;
        }
    }

    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_hex4(&mut self) -> Option<u16> {
        if self.pos + 4 > self.bytes.len() {
            return None;
        }
        let s = self.src.get(self.pos..self.pos + 4)?;
        let val = u16::from_str_radix(s, 16).ok()?;
        self.pos += 4;
        Some(val)
    }

    fn parse_str(&mut self) -> Option<String> {
        const LO: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = 0x8080_8080_8080_8080;
        let mut out = String::new();
        loop {
            let run_start = self.pos;
            while self.pos + 8 <= self.bytes.len() {
                let word =
                    u64::from_le_bytes(self.bytes[self.pos..self.pos + 8].try_into().unwrap());
                let d_quot = word ^ (0x22u64.wrapping_mul(LO));
                let d_bs = word ^ (0x5Cu64.wrapping_mul(LO));
                let m_quot = d_quot.wrapping_sub(LO) & !d_quot;
                let m_bs = d_bs.wrapping_sub(LO) & !d_bs;
                let m_ctrl = !(word | (word << 1) | (word << 2));
                let mask = (m_quot | m_bs | m_ctrl) & HI;
                if mask == 0 {
                    self.pos += 8;
                    continue;
                }
                self.pos += (mask.trailing_zeros() >> 3) as usize;
                break;
            }
            while self.pos < self.bytes.len() {
                let b = self.bytes[self.pos];
                if b < 0x20 || b == b'"' || b == b'\\' {
                    break;
                }
                self.pos += 1;
            }
            let b = *self.bytes.get(self.pos)?;
            if b == b'"' {
                let end = self.pos;
                self.pos += 1;
                if out.is_empty() {
                    return Some(self.src[run_start..end].to_string());
                }
                out.push_str(&self.src[run_start..end]);
                return Some(out);
            }
            if b < 0x20 {
                return None;
            }
            if self.pos > run_start {
                out.push_str(&self.src[run_start..self.pos]);
            }
            self.pos += 1;
            let esc = *self.bytes.get(self.pos)?;
            self.pos += 1;
            match esc {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{0008}'),
                b'f' => out.push('\u{000C}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    let hi = self.parse_hex4()?;
                    if (0xD800..=0xDBFF).contains(&hi) {
                        if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                            return None;
                        }
                        self.pos += 2;
                        let lo = self.parse_hex4()?;
                        if !(0xDC00..=0xDFFF).contains(&lo) {
                            return None;
                        }
                        let cp =
                            0x10000 + (((u32::from(hi) - 0xD800) << 10) | (u32::from(lo) - 0xDC00));
                        out.push(char::from_u32(cp)?);
                    } else if (0xDC00..=0xDFFF).contains(&hi) {
                        return None;
                    } else {
                        out.push(char::from_u32(u32::from(hi))?);
                    }
                }
                _ => return None,
            }
        }
    }

    fn parse_number(&mut self) -> Option<String> {
        let start = self.pos;
        if self.peek_byte() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek_byte()? {
            b'0' => self.pos += 1,
            b'1'..=b'9' => {
                self.pos += 1;
                while self.peek_byte().is_some_and(|d| d.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => return None,
        }
        if self.peek_byte() == Some(b'.') {
            self.pos += 1;
            if !self.peek_byte().is_some_and(|d| d.is_ascii_digit()) {
                return None;
            }
            while self.peek_byte().is_some_and(|d| d.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if self.peek_byte().is_some_and(|b| b == b'e' || b == b'E') {
            self.pos += 1;
            if self.peek_byte().is_some_and(|b| b == b'+' || b == b'-') {
                self.pos += 1;
            }
            if !self.peek_byte().is_some_and(|d| d.is_ascii_digit()) {
                return None;
            }
            while self.peek_byte().is_some_and(|d| d.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        Some(self.src[start..self.pos].to_string())
    }

    fn consume_keyword(&mut self, kw: &[u8]) -> Option<()> {
        if self.bytes.get(self.pos..self.pos + kw.len()) == Some(kw) {
            self.pos += kw.len();
            Some(())
        } else {
            None
        }
    }

    fn parse_value(&mut self, depth: usize) -> Option<StrictJsonValue> {
        if depth > MAX_JSON_NESTING_DEPTH {
            return None;
        }
        self.skip_ws();
        match self.peek_byte()? {
            b'"' => {
                self.pos += 1;
                Some(StrictJsonValue::Str(self.parse_str()?))
            }
            b'{' => {
                self.pos += 1;
                self.skip_ws();
                let mut entries: Vec<(String, StrictJsonValue)> = Vec::new();
                if self.peek_byte() == Some(b'}') {
                    self.pos += 1;
                    return Some(StrictJsonValue::Object(entries));
                }
                // Small objects (nearly all of them) compare keys directly; a hash set is built
                // only once an object has `LINEAR_KEY_LIMIT` keys, so wide objects stay linear.
                let mut seen_keys: Option<HashSet<String>> = None;
                loop {
                    self.skip_ws();
                    if self.peek_byte()? != b'"' {
                        return None;
                    }
                    self.pos += 1;
                    let key = self.parse_str()?;
                    let duplicate = match seen_keys.as_mut() {
                        Some(set) => !set.insert(key.clone()),
                        None => {
                            let duplicate = entries.iter().any(|(k, _)| *k == key);
                            if !duplicate && entries.len() + 1 == LINEAR_KEY_LIMIT {
                                let set = entries.iter().map(|(k, _)| k.clone());
                                seen_keys = Some(set.chain([key.clone()]).collect());
                            }
                            duplicate
                        }
                    };
                    if duplicate {
                        return None;
                    }
                    self.skip_ws();
                    if self.peek_byte()? != b':' {
                        return None;
                    }
                    self.pos += 1;
                    let val = self.parse_value(depth + 1)?;
                    entries.push((key, val));
                    self.skip_ws();
                    match self.peek_byte()? {
                        b',' => {
                            self.pos += 1;
                        }
                        b'}' => {
                            self.pos += 1;
                            break;
                        }
                        _ => return None,
                    }
                }
                Some(StrictJsonValue::Object(entries))
            }
            b'[' => {
                self.pos += 1;
                self.skip_ws();
                let mut items = Vec::new();
                if self.peek_byte() == Some(b']') {
                    self.pos += 1;
                    return Some(StrictJsonValue::Array(items));
                }
                loop {
                    items.push(self.parse_value(depth + 1)?);
                    self.skip_ws();
                    match self.peek_byte()? {
                        b',' => {
                            self.pos += 1;
                        }
                        b']' => {
                            self.pos += 1;
                            break;
                        }
                        _ => return None,
                    }
                }
                Some(StrictJsonValue::Array(items))
            }
            b't' => {
                self.consume_keyword(b"true")?;
                Some(StrictJsonValue::Bool(true))
            }
            b'f' => {
                self.consume_keyword(b"false")?;
                Some(StrictJsonValue::Bool(false))
            }
            b'n' => {
                self.consume_keyword(b"null")?;
                Some(StrictJsonValue::Null)
            }
            b'-' | b'0'..=b'9' => Some(StrictJsonValue::Number(self.parse_number()?)),
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
        assert_eq!(StrictJsonValue::Number("+42".to_string()).as_u64(), None);
        assert_eq!(StrictJsonValue::Number("007".to_string()).as_u64(), None);
        let huge_overflow = format!("0.{}1e999999999", "0".repeat(999_999));
        assert_eq!(StrictJsonValue::Number(huge_overflow).as_u64(), None);
        let huge_fraction = format!("1{}e-999999999", "0".repeat(1_000_000));
        assert_eq!(StrictJsonValue::Number(huge_fraction).as_u64(), None);
        let huge_exact_five = format!("0.{}5e1000001", "0".repeat(1_000_000));
        assert_eq!(StrictJsonValue::Number(huge_exact_five).as_u64(), Some(5));

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

    #[test]
    fn duplicate_keys_are_refused_before_and_after_the_hash_set_switch() {
        for width in [1usize, 2, 15, 16, 17, 40] {
            let pairs: Vec<String> = (0..width).map(|i| format!("\"k{i}\":{i}")).collect();
            let plain = format!("{{{}}}", pairs.join(","));
            assert_eq!(
                StrictJsonValue::parse(&plain)
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .len(),
                width
            );
            for repeated in [0, width / 2, width - 1] {
                let mut with_duplicate = pairs.clone();
                with_duplicate.push(format!("\"k{repeated}\":0"));
                let text = format!("{{{}}}", with_duplicate.join(","));
                assert!(StrictJsonValue::parse(&text).is_err(), "{text}");
            }
        }
    }
}
