use std::borrow::Cow;
use std::collections::HashSet;

use crate::EasyOfficeError;

pub const MAX_STRICT_JSON_DEPTH: usize = 32;
const LINEAR_KEY_LIMIT: usize = 16;
const EXPONENT_CLAMP_MARGIN: usize = 25;
const MAX_U64_DIGITS: usize = 20;

/// Zero-copy borrowing variant of `StrictJsonValue`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictJsonBorrow<'a> {
    Null,
    Bool(bool),
    Number(&'a str),
    Str(Cow<'a, str>),
    Array(Vec<StrictJsonBorrow<'a>>),
    Object(Vec<(Cow<'a, str>, StrictJsonBorrow<'a>)>),
}

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

impl<'a> StrictJsonBorrow<'a> {
    pub fn parse(input: &'a str) -> Result<Self, EasyOfficeError> {
        parse_strict_json_borrowed(input)
    }

    pub fn as_object(&self) -> Option<&[(Cow<'a, str>, StrictJsonBorrow<'a>)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[StrictJsonBorrow<'a>]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_ref()),
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
            None => (false, *n),
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

    pub fn get_field<'b>(
        entries: &'b [(Cow<'a, str>, StrictJsonBorrow<'a>)],
        key: &str,
    ) -> Option<&'b StrictJsonBorrow<'a>> {
        entries
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .map(|(_, v)| v)
    }

    pub fn get_str<'b>(
        entries: &'b [(Cow<'a, str>, StrictJsonBorrow<'a>)],
        key: &str,
    ) -> Option<&'b str> {
        Self::get_field(entries, key).and_then(Self::as_str)
    }

    pub fn to_scalar_string(&self) -> Option<String> {
        match self {
            Self::Null => Some(String::new()),
            Self::Bool(b) => Some(if *b { "true" } else { "false" }.to_string()),
            Self::Number(n) => Some((*n).to_string()),
            Self::Str(s) => Some(s.as_ref().to_string()),
            Self::Array(_) | Self::Object(_) => None,
        }
    }

    pub fn into_scalar_string(self) -> Option<String> {
        match self {
            Self::Null => Some(String::new()),
            Self::Bool(b) => Some(if b { "true" } else { "false" }.to_string()),
            Self::Number(n) => Some(n.to_string()),
            Self::Str(s) => Some(if s.is_empty() {
                String::new()
            } else {
                s.into_owned()
            }),
            Self::Array(_) | Self::Object(_) => None,
        }
    }
}

pub fn parse_strict_json_borrowed<'a>(
    input: &'a str,
) -> Result<StrictJsonBorrow<'a>, EasyOfficeError> {
    let trimmed = input.strip_prefix('\u{FEFF}').unwrap_or(input).trim();
    let mut cursor = JsonBorrowCursor {
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

struct JsonBorrowCursor<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonBorrowCursor<'a> {
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

    fn parse_str(&mut self) -> Option<Cow<'a, str>> {
        const LO: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = 0x8080_8080_8080_8080;
        let mut out: Option<String> = None;
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
                return match out {
                    Some(mut s) => {
                        s.push_str(&self.src[run_start..end]);
                        Some(Cow::Owned(s))
                    }
                    None => Some(Cow::Borrowed(&self.src[run_start..end])),
                };
            }
            if b < 0x20 {
                return None;
            }
            let str_buf = out.get_or_insert_with(String::new);
            if self.pos > run_start {
                str_buf.push_str(&self.src[run_start..self.pos]);
            }
            self.pos += 1;
            let esc = *self.bytes.get(self.pos)?;
            self.pos += 1;
            match esc {
                b'"' => str_buf.push('"'),
                b'\\' => str_buf.push('\\'),
                b'/' => str_buf.push('/'),
                b'b' => str_buf.push('\u{0008}'),
                b'f' => str_buf.push('\u{000C}'),
                b'n' => str_buf.push('\n'),
                b'r' => str_buf.push('\r'),
                b't' => str_buf.push('\t'),
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
                        str_buf.push(char::from_u32(cp)?);
                    } else if (0xDC00..=0xDFFF).contains(&hi) {
                        return None;
                    } else {
                        str_buf.push(char::from_u32(u32::from(hi))?);
                    }
                }
                _ => return None,
            }
        }
    }

    fn parse_number(&mut self) -> Option<&'a str> {
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
        Some(&self.src[start..self.pos])
    }

    fn consume_keyword(&mut self, kw: &[u8]) -> Option<()> {
        if self.bytes.get(self.pos..self.pos + kw.len()) == Some(kw) {
            self.pos += kw.len();
            Some(())
        } else {
            None
        }
    }

    fn parse_value(&mut self, depth: usize) -> Option<StrictJsonBorrow<'a>> {
        if depth > MAX_STRICT_JSON_DEPTH {
            return None;
        }
        self.skip_ws();
        match self.peek_byte()? {
            b'"' => {
                self.pos += 1;
                Some(StrictJsonBorrow::Str(self.parse_str()?))
            }
            b'{' => {
                self.pos += 1;
                self.skip_ws();
                let mut entries: Vec<(Cow<'a, str>, StrictJsonBorrow<'a>)> = Vec::new();
                if self.peek_byte() == Some(b'}') {
                    self.pos += 1;
                    return Some(StrictJsonBorrow::Object(entries));
                }
                let mut seen_keys: Option<HashSet<Cow<'a, str>>> = None;
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
                            let duplicate = entries.iter().any(|(k, _)| k == &key);
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
                    let value = self.parse_value(depth + 1)?;
                    entries.push((key, value));
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
                Some(StrictJsonBorrow::Object(entries))
            }
            b'[' => {
                self.pos += 1;
                self.skip_ws();
                let mut items = Vec::new();
                if self.peek_byte() == Some(b']') {
                    self.pos += 1;
                    return Some(StrictJsonBorrow::Array(items));
                }
                loop {
                    let value = self.parse_value(depth + 1)?;
                    items.push(value);
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
                Some(StrictJsonBorrow::Array(items))
            }
            b't' => {
                self.consume_keyword(b"true")?;
                Some(StrictJsonBorrow::Bool(true))
            }
            b'f' => {
                self.consume_keyword(b"false")?;
                Some(StrictJsonBorrow::Bool(false))
            }
            b'n' => {
                self.consume_keyword(b"null")?;
                Some(StrictJsonBorrow::Null)
            }
            b'-' | b'0'..=b'9' => Some(StrictJsonBorrow::Number(self.parse_number()?)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_json_borrow_zero_copy() {
        let json = r#"{"name": "Alice", "age": 30, "active": true, "note": "hello\nworld"}"#;
        let parsed = parse_strict_json_borrowed(json).unwrap();
        let obj = parsed.as_object().unwrap();

        let name = StrictJsonBorrow::get_field(obj, "name").unwrap();
        assert_eq!(name.as_str(), Some("Alice"));
        if let StrictJsonBorrow::Str(Cow::Borrowed(b)) = name {
            assert_eq!(*b, "Alice");
        } else {
            panic!("Expected borrowed string for unescaped key");
        }

        let age = StrictJsonBorrow::get_field(obj, "age").unwrap();
        assert_eq!(age.as_u64(), Some(30));
        assert_eq!(age, &StrictJsonBorrow::Number("30"));

        let note = StrictJsonBorrow::get_field(obj, "note").unwrap();
        assert_eq!(note.as_str(), Some("hello\nworld"));
        assert!(matches!(note, StrictJsonBorrow::Str(Cow::Owned(_))));
    }

    #[test]
    fn test_strict_json_borrow_rejects_duplicates_and_control_chars() {
        let dup = r#"{"a": 1, "a": 2}"#;
        assert!(parse_strict_json_borrowed(dup).is_err());

        let ctrl = "{\"a\": \"bad\x00str\"}";
        assert!(parse_strict_json_borrowed(ctrl).is_err());
    }
}
