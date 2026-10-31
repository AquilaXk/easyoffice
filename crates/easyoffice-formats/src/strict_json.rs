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
        match self {
            Self::Number(n) => n.parse::<u64>().ok(),
            _ => None,
        }
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
            if !chars.peek().is_some_and(|c| c.is_ascii_digit()) {
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
            if !chars.peek().is_some_and(|c| c.is_ascii_digit()) {
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
                loop {
                    Self::skip_ws(chars);
                    if chars.next()? != '"' {
                        return None;
                    }
                    let key = Self::parse_string_literal(chars)?;
                    if entries.iter().any(|(k, _)| k == &key) {
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
