use crate::EasyOfficeError;

pub const HWPTAG_EQEDIT: u16 = 88;
pub const MAX_EQEDIT_DEPTH: usize = 64;

const PLANE15_PUA_BASE: u32 = 0x0F_0000;
const UNPAIRED_SURROGATE_REPLACEMENT: char = '\u{25A1}';

pub const HWP5_PUA_BMP_SYMBOLS: &[u16] = &[
    0xA0E1, 0xA12B, 0xA289, 0xA28A, 0xA292, 0xA293, 0xA294, 0xA295, 0xA296, 0xA297, 0xA298, 0xA299,
    0xA29A, 0xA29B, 0xA2B1, 0xA2B2, 0xA2B3, 0xA2B4, 0xA2B5, 0xA2B6, 0xA2B7, 0xA2B8, 0xA2B9, 0xA2BA,
    0xA2BB, 0xA2BC, 0xA2BD, 0xA2BE, 0xA2BF, 0xA2C0, 0xA2C3, 0xA2C4, 0xA2C5, 0xA2C6, 0xA2C7, 0xA2C8,
    0xA2C9, 0xA2CA, 0xA2CB, 0xA2CC, 0xA2FC, 0xA3C5, 0xA807, 0xA80A, 0xA80E, 0xA80F, 0xA810, 0xA81A,
    0xA832, 0xA852, 0xA853, 0xA854, 0xA855,
];

pub fn hwp5_symbol_to_plane15(unit: u16) -> Option<u32> {
    HWP5_PUA_BMP_SYMBOLS
        .contains(&unit)
        .then(|| PLANE15_PUA_BASE | u32::from(unit & 0x0FFF))
}

pub fn plane15_to_hwp5_symbol(cp: u32) -> Option<u16> {
    if cp & 0xFF_F000 != PLANE15_PUA_BASE {
        return None;
    }
    let unit = 0xA000u16 | u16::try_from(cp & 0x0FFF).ok()?;
    (hwp5_symbol_to_plane15(unit) == Some(cp)).then_some(unit)
}

fn decode_surrogate_pair(bytes: &[u8], cursor: usize, high: u16) -> Option<char> {
    if !(0xD800..=0xDBFF).contains(&high) || cursor + 3 >= bytes.len() {
        return None;
    }
    let low = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
    if !(0xDC00..=0xDFFF).contains(&low) {
        return None;
    }
    let cp = 0x10000 + ((high as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
    let mapped = plane15_to_hwp5_symbol(cp).map_or(cp, u32::from);
    Some(char::from_u32(mapped).unwrap_or(UNPAIRED_SURROGATE_REPLACEMENT))
}

pub fn decode_hwp5_para_text_bytes(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    if bytes.len() % 2 != 0 {
        return Err(EasyOfficeError::CorruptedContainer(
            "HWP 5.0 문단 텍스트 페이로드 길이가 2바이트(WCHAR) 배수가 아닙니다.".to_string(),
        ));
    }
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let code = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        if code == 0x0009 {
            out.push('\t');
            let is_ext_tab = cursor + 16 <= bytes.len()
                && u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]) == 0x0009
                && u16::from_le_bytes([bytes[cursor + 12], bytes[cursor + 13]]) == 0;
            cursor += if is_ext_tab { 16 } else { 2 };
        } else if code == 0x000A {
            out.push('\n');
            cursor += 2;
        } else if code == 0x000D {
            if cursor + 2 < bytes.len() {
                out.push('\n');
            }
            cursor += 2;
        } else if matches!(code, 0x0001..=0x0008 | 0x000B..=0x000C | 0x000E..=0x0017)
            && cursor + 16 <= bytes.len()
            && u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]) == code
        {
            if code == 0x0012 {
                out.push(' ');
            }
            cursor += 16;
        } else if code < 0x0020 {
            if let Some(ch) = match code {
                0x0012 | 0x0019 => Some(' '),
                0x0018 => Some('\u{00AD}'),
                0x001E => Some('\u{00A0}'),
                0x001F => Some('\u{2007}'),
                _ => None,
            } {
                out.push(ch);
            }
            cursor += 2;
        } else if let Some(ch) = decode_surrogate_pair(bytes, cursor, code) {
            out.push(ch);
            cursor += 4;
        } else {
            out.push(char::from_u32(code as u32).unwrap_or(UNPAIRED_SURROGATE_REPLACEMENT));
            cursor += 2;
        }
    }
    Ok(out)
}

pub fn decode_hwp5_eqedit_script(data: &[u8]) -> Result<String, EasyOfficeError> {
    if data.len() < 6 {
        return Err(EasyOfficeError::CorruptedContainer(
            "HWP 5.0 HWPTAG_EQEDIT 레코드 길이가 최소 헤더(6바이트)보다 짧습니다.".to_string(),
        ));
    }
    let char_len = u16::from_le_bytes([data[4], data[5]]) as usize;
    let byte_len = char_len.checked_mul(2).ok_or_else(|| {
        EasyOfficeError::CorruptedContainer("HWP 5.0 HWPTAG_EQEDIT 길이 오버플로".to_string())
    })?;
    if 6 + byte_len > data.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "HWP 5.0 HWPTAG_EQEDIT 수식 문자열 길이가 레코드 경계를 초과합니다.".to_string(),
        ));
    }
    let mut script = String::with_capacity(char_len);
    let slice = &data[6..6 + byte_len];
    let mut idx = 0usize;
    while idx < slice.len() {
        let u = u16::from_le_bytes([slice[idx], slice[idx + 1]]);
        if u == 0 {
            break;
        }
        if (0xD800..=0xDBFF).contains(&u) && idx + 3 < slice.len() {
            let low = u16::from_le_bytes([slice[idx + 2], slice[idx + 3]]);
            if (0xDC00..=0xDFFF).contains(&low) {
                let cp = 0x10000 + ((u as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
                let mapped = plane15_to_hwp5_symbol(cp).map_or(cp, u32::from);
                script.push(char::from_u32(mapped).unwrap_or(UNPAIRED_SURROGATE_REPLACEMENT));
                idx += 4;
                continue;
            }
        }
        script.push(char::from_u32(u as u32).unwrap_or(UNPAIRED_SURROGATE_REPLACEMENT));
        idx += 2;
    }
    let trimmed = script.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    convert_eqedit_to_latex(trimmed)
}

#[derive(Debug, Clone, PartialEq)]
enum EqToken {
    LBrace,
    RBrace,
    Superscript,
    Subscript,
    ThinSpace,
    NormalSpace,
    Newline,
    Ampersand,
    Number(String),
    Command(String),
    Text(String),
    QuotedText(String),
    Symbol(char),
}

const FONT_PREFIXES: &[&str] = &["bold", "rm", "it"];

const KNOWN_COMMANDS_LIST: &str = "over atop sqrt root of sup sub lsub lsup from to left right \
sum prod coprod int oint dint tint bigcup bigcap lim matrix pmatrix bmatrix dmatrix cases pile \
lpile rpile cpile hat bar vec dot ddot tilde acute grave check breve overline underline widehat \
widetilde overbrace underbrace overrightarrow mathring rm it bold binom choose sin cos tan csc \
sec cot arcsin arccos arctan sinh cosh tanh log ln lg exp max min det dim gcd lcm mod deg ker \
hom arg inf alpha beta gamma delta epsilon varepsilon zeta eta theta vartheta iota kappa lambda \
mu nu xi omicron pi varpi rho varrho sigma varsigma tau upsilon phi varphi chi psi omega times \
div cdot pm mp circ bullet star ast oplus otimes odot leq le geq ge neq ne approx equiv sim \
simeq cong propto ll gg subset supset subseteq supseteq in notin ni cup cap emptyset nothing \
rarrow rightarrow larrow leftarrow lrarrow leftrightarrow uparrow downarrow uarrow darrow \
mapsto implies iff infty infin partial nabla forall exists exist neg lnot land lor therefore \
because dots cdots ldots vdots ddots prime aleph hbar ell wp re im angle triangle perp parallel \
mid scale color langle rangle lfloor rfloor lceil rceil vert dvert line dline";

fn is_known_command(word: &str) -> bool {
    KNOWN_COMMANDS_LIST
        .split_ascii_whitespace()
        .any(|w| w.eq_ignore_ascii_case(word))
}

fn push_eqedit_word_tokens(word: String, tokens: &mut Vec<EqToken>) {
    if is_known_command(&word) {
        tokens.push(EqToken::Command(word));
    } else if let Some(prefix) = FONT_PREFIXES
        .iter()
        .find(|&&p| word.len() > p.len() && word[..p.len()].eq_ignore_ascii_case(p))
    {
        tokens.push(EqToken::Command(word[..prefix.len()].to_string()));
        let rest = &word[prefix.len()..];
        if is_known_command(rest) {
            tokens.push(EqToken::Command(rest.to_string()));
        } else {
            tokens.extend(rest.chars().map(|ch| EqToken::Text(ch.to_string())));
        }
    } else {
        tokens.extend(word.chars().map(|ch| EqToken::Text(ch.to_string())));
    }
}

fn tokenize_eqedit(input: &str) -> Result<Vec<EqToken>, EasyOfficeError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0usize;

    while i < len {
        match chars[i] {
            ' ' | '\t' | '\r' | '\n' => i += 1,
            '{' => {
                tokens.push(EqToken::LBrace);
                i += 1;
            }
            '}' => {
                tokens.push(EqToken::RBrace);
                i += 1;
            }
            '^' => {
                tokens.push(EqToken::Superscript);
                i += 1;
            }
            '_' => {
                tokens.push(EqToken::Subscript);
                i += 1;
            }
            '`' => {
                tokens.push(EqToken::ThinSpace);
                i += 1;
            }
            '~' => {
                tokens.push(EqToken::NormalSpace);
                i += 1;
            }
            '#' => {
                tokens.push(EqToken::Newline);
                i += 1;
            }
            '&' => {
                tokens.push(EqToken::Ampersand);
                i += 1;
            }
            '"' => {
                i += 1;
                let start = i;
                while i < len && chars[i] != '"' {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                if i < len {
                    i += 1;
                }
                tokens.push(EqToken::QuotedText(text));
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                tokens.push(EqToken::Number(chars[start..i].iter().collect()));
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < len && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                push_eqedit_word_tokens(chars[start..i].iter().collect(), &mut tokens);
            }
            c => {
                tokens.push(EqToken::Symbol(c));
                i += 1;
            }
        }
    }
    Ok(tokens)
}

#[derive(Debug, Clone)]
enum EqExpr {
    Raw(String),
    Group(Vec<EqExpr>),
    Frac(Vec<EqExpr>, Vec<EqExpr>),
    Atop(Vec<EqExpr>, Vec<EqExpr>),
    Sqrt(Option<Vec<EqExpr>>, Vec<EqExpr>),
    Sup(Box<EqExpr>, Vec<EqExpr>),
    Sub(Box<EqExpr>, Vec<EqExpr>),
    SubSup(Box<EqExpr>, Vec<EqExpr>, Vec<EqExpr>),
    LeftSub(Box<EqExpr>, Vec<EqExpr>),
    LeftSup(Box<EqExpr>, Vec<EqExpr>),
    BigOp(String, Option<Vec<EqExpr>>, Option<Vec<EqExpr>>),
    LeftRight(String, Vec<EqExpr>, String),
    Matrix(&'static str, Vec<Vec<Vec<EqExpr>>>),
    Accent(&'static str, Vec<EqExpr>),
    Font(&'static str, Vec<EqExpr>),
    Binom(Vec<EqExpr>, Vec<EqExpr>),
}

struct EqParser {
    tokens: Vec<EqToken>,
    pos: usize,
}

impl EqParser {
    fn new(tokens: Vec<EqToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&EqToken> {
        self.tokens.get(self.pos)
    }

    fn check_depth(depth: usize) -> Result<(), EasyOfficeError> {
        if depth > MAX_EQEDIT_DEPTH {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "EqEdit 수식 중첩 깊이가 보안 상한({})을 초과했습니다.",
                MAX_EQEDIT_DEPTH
            )));
        }
        Ok(())
    }

    fn peek_infix_frac_kind(&self) -> Option<&'static str> {
        let Some(EqToken::Command(cmd)) = self.peek() else {
            return None;
        };
        if cmd.eq_ignore_ascii_case("over") {
            Some("over")
        } else if cmd.eq_ignore_ascii_case("choose") {
            Some("choose")
        } else if cmd.eq_ignore_ascii_case("atop") {
            Some("atop")
        } else {
            None
        }
    }

    fn parse_expr_until(
        &mut self,
        stop_rbrace: bool,
        stop_right: bool,
        stop_matrix_sep: bool,
        depth: usize,
    ) -> Result<Vec<EqExpr>, EasyOfficeError> {
        Self::check_depth(depth)?;
        let mut items = Vec::new();

        while self.pos < self.tokens.len() {
            if matches!(self.peek(), Some(EqToken::RBrace)) {
                if stop_rbrace {
                    break;
                }
                return Err(EasyOfficeError::CorruptedContainer(
                    "EqEdit 수식에 짝이 맞지 않는 닫는 중괄호 '}'가 포함되어 있습니다.".to_string(),
                ));
            }
            if (stop_matrix_sep
                && matches!(self.peek(), Some(EqToken::Ampersand | EqToken::Newline)))
                || (stop_right
                    && matches!(self.peek(), Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("right")))
            {
                break;
            }
            if let Some(kind) = self.peek_infix_frac_kind() {
                self.pos += 1;
                let num = std::mem::take(&mut items);
                let den =
                    self.parse_expr_until(stop_rbrace, stop_right, stop_matrix_sep, depth + 1)?;
                let node = match kind {
                    "choose" => EqExpr::Binom(num, den),
                    "atop" => EqExpr::Atop(num, den),
                    _ => EqExpr::Frac(num, den),
                };
                items.push(node);
                continue;
            }
            if let Some(atom) = self.parse_atom(depth)? {
                items.push(self.parse_postfix(atom, depth)?);
            }
        }
        Ok(items)
    }

    fn parse_group(&mut self, depth: usize) -> Result<Vec<EqExpr>, EasyOfficeError> {
        Self::check_depth(depth)?;
        if matches!(self.peek(), Some(EqToken::LBrace)) {
            self.pos += 1;
            let inner = self.parse_expr_until(true, false, false, depth + 1)?;
            if !matches!(self.peek(), Some(EqToken::RBrace)) {
                return Err(EasyOfficeError::CorruptedContainer(
                    "EqEdit 수식의 여는 중괄호 '{'가 닫히지 않았습니다.".to_string(),
                ));
            }
            self.pos += 1;
            Ok(inner)
        } else if let Some(atom) = self.parse_atom(depth)? {
            Ok(vec![atom])
        } else {
            Ok(Vec::new())
        }
    }

    fn parse_bound_group(&mut self, depth: usize) -> Result<Vec<EqExpr>, EasyOfficeError> {
        if matches!(self.peek(), Some(EqToken::LBrace)) {
            return self.parse_group(depth);
        }
        let mut grp = Vec::new();
        if let Some(&EqToken::Symbol(sign @ ('-' | '+'))) = self.peek() {
            self.pos += 1;
            grp.push(EqExpr::Raw(sign.to_string()));
        }
        grp.extend(self.parse_group(depth)?);
        let rel_op = match self.peek() {
            Some(EqToken::Symbol(op @ '<'..='>')) => Some(op.to_string()),
            Some(EqToken::Command(c))
                if c.eq_ignore_ascii_case("rarrow") || c.eq_ignore_ascii_case("rightarrow") =>
            {
                Some("\\rightarrow".to_string())
            }
            Some(EqToken::Command(c))
                if c.eq_ignore_ascii_case("larrow") || c.eq_ignore_ascii_case("leftarrow") =>
            {
                Some("\\leftarrow".to_string())
            }
            _ => None,
        };
        if let Some(op_str) = rel_op {
            self.pos += 1;
            grp.push(EqExpr::Raw(op_str));
            if let Some(&EqToken::Symbol(sign @ ('-' | '+'))) = self.peek() {
                self.pos += 1;
                grp.push(EqExpr::Raw(sign.to_string()));
            }
            grp.extend(self.parse_group(depth)?);
        }
        Ok(grp)
    }

    fn parse_postfix(&mut self, mut base: EqExpr, depth: usize) -> Result<EqExpr, EasyOfficeError> {
        Self::check_depth(depth)?;
        let mut chain_depth = depth;
        loop {
            let is_sup = matches!(self.peek(), Some(EqToken::Superscript))
                || matches!(self.peek(), Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("sup"));
            let is_sub = matches!(self.peek(), Some(EqToken::Subscript))
                || matches!(self.peek(), Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("sub"));
            let is_lsup =
                matches!(self.peek(), Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("lsup"));
            let is_lsub =
                matches!(self.peek(), Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("lsub"));

            if is_sup {
                chain_depth += 1;
                Self::check_depth(chain_depth)?;
                self.pos += 1;
                let sup_arg = self.parse_group(chain_depth)?;
                let next_sub = matches!(self.peek(), Some(EqToken::Subscript))
                    || matches!(self.peek(), Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("sub"));
                if next_sub {
                    chain_depth += 1;
                    Self::check_depth(chain_depth)?;
                    self.pos += 1;
                    let sub_arg = self.parse_group(chain_depth)?;
                    base = EqExpr::SubSup(Box::new(base), sub_arg, sup_arg);
                } else {
                    base = EqExpr::Sup(Box::new(base), sup_arg);
                }
            } else if is_sub {
                chain_depth += 1;
                Self::check_depth(chain_depth)?;
                self.pos += 1;
                let sub_arg = self.parse_group(chain_depth)?;
                let next_sup = matches!(self.peek(), Some(EqToken::Superscript))
                    || matches!(self.peek(), Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("sup"));
                if next_sup {
                    chain_depth += 1;
                    Self::check_depth(chain_depth)?;
                    self.pos += 1;
                    let sup_arg = self.parse_group(chain_depth)?;
                    base = EqExpr::SubSup(Box::new(base), sub_arg, sup_arg);
                } else {
                    base = EqExpr::Sub(Box::new(base), sub_arg);
                }
            } else if is_lsup {
                chain_depth += 1;
                Self::check_depth(chain_depth)?;
                self.pos += 1;
                let sup_arg = self.parse_group(chain_depth)?;
                base = EqExpr::LeftSup(Box::new(base), sup_arg);
            } else if is_lsub {
                chain_depth += 1;
                Self::check_depth(chain_depth)?;
                self.pos += 1;
                let sub_arg = self.parse_group(chain_depth)?;
                base = EqExpr::LeftSub(Box::new(base), sub_arg);
            } else {
                break;
            }
        }
        Ok(base)
    }

    fn parse_delimiter(&mut self) -> String {
        match self.peek().cloned() {
            Some(EqToken::Symbol(c)) => {
                self.pos += 1;
                match c {
                    '(' | ')' | '[' | ']' | '|' => c.to_string(),
                    '.' => ".".to_string(),
                    _ => c.to_string(),
                }
            }
            Some(EqToken::LBrace) => {
                self.pos += 1;
                "\\{".to_string()
            }
            Some(EqToken::RBrace) => {
                self.pos += 1;
                "\\}".to_string()
            }
            Some(EqToken::Command(cmd)) => {
                let lower = cmd.to_ascii_lowercase();
                match lower.as_str() {
                    "langle" => {
                        self.pos += 1;
                        "\\langle ".to_string()
                    }
                    "rangle" => {
                        self.pos += 1;
                        "\\rangle".to_string()
                    }
                    "lfloor" => {
                        self.pos += 1;
                        "\\lfloor ".to_string()
                    }
                    "rfloor" => {
                        self.pos += 1;
                        "\\rfloor".to_string()
                    }
                    "lceil" => {
                        self.pos += 1;
                        "\\lceil ".to_string()
                    }
                    "rceil" => {
                        self.pos += 1;
                        "\\rceil".to_string()
                    }
                    "vert" | "line" => {
                        self.pos += 1;
                        "|".to_string()
                    }
                    "dvert" | "dline" => {
                        self.pos += 1;
                        "\\|".to_string()
                    }
                    _ => ".".to_string(),
                }
            }
            _ => ".".to_string(),
        }
    }

    fn parse_matrix_rows(
        &mut self,
        env: &'static str,
        depth: usize,
    ) -> Result<EqExpr, EasyOfficeError> {
        Self::check_depth(depth)?;
        if !matches!(self.peek(), Some(EqToken::LBrace)) {
            return Ok(EqExpr::Matrix(env, Vec::new()));
        }
        self.pos += 1;
        let mut rows = Vec::new();
        let mut current_row = Vec::new();

        while self.pos < self.tokens.len() {
            let cell = self.parse_expr_until(true, false, true, depth)?;
            current_row.push(cell);
            match self.peek() {
                Some(EqToken::RBrace) => {
                    self.pos += 1;
                    rows.push(current_row);
                    return Ok(EqExpr::Matrix(env, rows));
                }
                Some(EqToken::Ampersand) => {
                    self.pos += 1;
                }
                Some(EqToken::Newline) => {
                    self.pos += 1;
                    rows.push(std::mem::take(&mut current_row));
                }
                None => break,
                _ => {}
            }
        }
        Err(EasyOfficeError::CorruptedContainer(
            "EqEdit 행렬(matrix) 중괄호 '{'가 닫히지 않았습니다.".to_string(),
        ))
    }

    fn parse_atom(&mut self, depth: usize) -> Result<Option<EqExpr>, EasyOfficeError> {
        Self::check_depth(depth)?;
        match self.peek().cloned() {
            None | Some(EqToken::RBrace) => Ok(None),
            Some(EqToken::LBrace) => {
                let grp = self.parse_group(depth)?;
                Ok(Some(EqExpr::Group(grp)))
            }
            Some(EqToken::Number(n)) | Some(EqToken::Text(n)) => {
                self.pos += 1;
                Ok(Some(EqExpr::Raw(n)))
            }
            Some(EqToken::QuotedText(t)) => {
                self.pos += 1;
                Ok(Some(EqExpr::Raw(format!("\\text{{{t}}}"))))
            }
            Some(EqToken::Symbol(c)) => {
                self.pos += 1;
                Ok(Some(EqExpr::Raw(c.to_string())))
            }
            Some(EqToken::ThinSpace) => {
                self.pos += 1;
                Ok(Some(EqExpr::Raw("\\,".to_string())))
            }
            Some(EqToken::NormalSpace) => {
                self.pos += 1;
                Ok(Some(EqExpr::Raw("\\ ".to_string())))
            }
            Some(EqToken::Newline) => {
                self.pos += 1;
                Ok(Some(EqExpr::Raw("\\\\".to_string())))
            }
            Some(EqToken::Ampersand) => {
                self.pos += 1;
                Ok(Some(EqExpr::Raw("&".to_string())))
            }
            Some(EqToken::Superscript) | Some(EqToken::Subscript) => {
                let is_sup = matches!(self.peek(), Some(EqToken::Superscript));
                self.pos += 1;
                let arg = self.parse_group(depth + 1)?;
                let empty = Box::new(EqExpr::Raw(String::new()));
                Ok(Some(if is_sup {
                    EqExpr::Sup(empty, arg)
                } else {
                    EqExpr::Sub(empty, arg)
                }))
            }
            Some(EqToken::Command(cmd)) => {
                self.pos += 1;
                self.parse_command(&cmd, depth)
            }
        }
    }

    fn parse_command(
        &mut self,
        cmd: &str,
        depth: usize,
    ) -> Result<Option<EqExpr>, EasyOfficeError> {
        Self::check_depth(depth)?;
        let lower = cmd.to_ascii_lowercase();
        let next_depth = depth + 1;
        match lower.as_str() {
            "sqrt" => {
                let rad = self.parse_group(next_depth)?;
                Ok(Some(EqExpr::Sqrt(None, rad)))
            }
            "root" => {
                let idx = self.parse_group(next_depth)?;
                if let Some(EqToken::Command(c)) = self.peek() {
                    if c.eq_ignore_ascii_case("of") {
                        self.pos += 1;
                    }
                }
                let rad = self.parse_group(next_depth)?;
                Ok(Some(EqExpr::Sqrt(Some(idx), rad)))
            }
            "sum" | "prod" | "coprod" | "int" | "oint" | "dint" | "tint" | "bigcup" | "bigcap"
            | "lim" => {
                let latex_op = match lower.as_str() {
                    "dint" => "\\iint".to_string(),
                    "tint" => "\\iiint".to_string(),
                    _ => format!("\\{lower}"),
                };
                let mut lower_bd = None;
                let mut upper_bd = None;
                loop {
                    match self.peek() {
                        Some(EqToken::Command(c)) if c.eq_ignore_ascii_case("from") => {
                            self.pos += 1;
                            lower_bd = Some(self.parse_bound_group(next_depth)?);
                        }
                        Some(EqToken::Command(c))
                            if c.eq_ignore_ascii_case("to") && upper_bd.is_none() =>
                        {
                            self.pos += 1;
                            upper_bd = Some(self.parse_bound_group(next_depth)?);
                        }
                        Some(EqToken::Subscript) if lower_bd.is_none() => {
                            self.pos += 1;
                            lower_bd = Some(self.parse_bound_group(next_depth)?);
                        }
                        Some(EqToken::Command(c))
                            if (c.eq_ignore_ascii_case("sub")
                                || c.eq_ignore_ascii_case("lsub"))
                                && lower_bd.is_none() =>
                        {
                            self.pos += 1;
                            lower_bd = Some(self.parse_bound_group(next_depth)?);
                        }
                        Some(EqToken::Superscript) if upper_bd.is_none() => {
                            self.pos += 1;
                            upper_bd = Some(self.parse_bound_group(next_depth)?);
                        }
                        Some(EqToken::Command(c))
                            if (c.eq_ignore_ascii_case("sup")
                                || c.eq_ignore_ascii_case("lsup"))
                                && upper_bd.is_none() =>
                        {
                            self.pos += 1;
                            upper_bd = Some(self.parse_bound_group(next_depth)?);
                        }
                        _ => break,
                    }
                }
                Ok(Some(EqExpr::BigOp(latex_op, lower_bd, upper_bd)))
            }
            "left" => {
                let open = self.parse_delimiter();
                let body = self.parse_expr_until(false, true, false, next_depth)?;
                let close = if let Some(EqToken::Command(c)) = self.peek() {
                    if c.eq_ignore_ascii_case("right") {
                        self.pos += 1;
                        self.parse_delimiter()
                    } else {
                        ".".to_string()
                    }
                } else {
                    ".".to_string()
                };
                Ok(Some(EqExpr::LeftRight(open, body, close)))
            }
            "right" => Err(EasyOfficeError::CorruptedContainer(
                "짝이 맞지 않는 right 구분자입니다.".to_string(),
            )),
            "matrix" | "pile" | "cpile" | "lpile" | "rpile" => {
                self.parse_matrix_rows("matrix", next_depth).map(Some)
            }
            "pmatrix" => self.parse_matrix_rows("pmatrix", next_depth).map(Some),
            "bmatrix" => self.parse_matrix_rows("bmatrix", next_depth).map(Some),
            "dmatrix" => self.parse_matrix_rows("vmatrix", next_depth).map(Some),
            "cases" => self.parse_matrix_rows("cases", next_depth).map(Some),
            "hat" | "widehat" | "bar" | "overline" | "underline" | "vec" | "overrightarrow"
            | "dot" | "ddot" | "tilde" | "widetilde" | "acute" | "grave" | "check" | "breve"
            | "mathring" | "overbrace" | "underbrace" => {
                let tag = match lower.as_str() {
                    "hat" => "\\hat",
                    "widehat" => "\\widehat",
                    "bar" => "\\bar",
                    "overline" => "\\overline",
                    "underline" => "\\underline",
                    "vec" => "\\vec",
                    "overrightarrow" => "\\overrightarrow",
                    "dot" => "\\dot",
                    "ddot" => "\\ddot",
                    "tilde" => "\\tilde",
                    "widetilde" => "\\widetilde",
                    "acute" => "\\acute",
                    "grave" => "\\grave",
                    "check" => "\\check",
                    "breve" => "\\breve",
                    "mathring" => "\\mathring",
                    "overbrace" => "\\overbrace",
                    _ => "\\underbrace",
                };
                Ok(Some(EqExpr::Accent(tag, self.parse_group(next_depth)?)))
            }
            "rm" | "it" | "bold" => {
                let tag = match lower.as_str() {
                    "rm" => "\\mathrm",
                    "it" => "\\mathit",
                    _ => "\\mathbf",
                };
                Ok(Some(EqExpr::Font(tag, self.parse_group(next_depth)?)))
            }
            "binom" => {
                let n = self.parse_group(next_depth)?;
                let k = self.parse_group(next_depth)?;
                Ok(Some(EqExpr::Binom(n, k)))
            }
            "scale" | "color" => {
                let _ = self.parse_group(next_depth)?;
                let body = self.parse_group(next_depth)?;
                Ok(Some(EqExpr::Group(body)))
            }
            _ => Ok(Some(EqExpr::Raw(command_to_latex_symbol(cmd)))),
        }
    }
}

fn command_to_latex_symbol(cmd: &str) -> String {
    match cmd {
        "GAMMA" | "Gamma" => return "\\Gamma".to_string(),
        "DELTA" | "Delta" => return "\\Delta".to_string(),
        "THETA" | "Theta" => return "\\Theta".to_string(),
        "LAMBDA" | "Lambda" => return "\\Lambda".to_string(),
        "XI" | "Xi" => return "\\Xi".to_string(),
        "PI" | "Pi" => return "\\Pi".to_string(),
        "SIGMA" | "Sigma" => return "\\Sigma".to_string(),
        "UPSILON" | "Upsilon" => return "\\Upsilon".to_string(),
        "PHI" | "Phi" => return "\\Phi".to_string(),
        "PSI" | "Psi" => return "\\Psi".to_string(),
        "OMEGA" | "Omega" => return "\\Omega".to_string(),
        "Rarrow" => return "\\Rightarrow".to_string(),
        "Larrow" => return "\\Leftarrow".to_string(),
        "LRarrow" | "Lrarrow" => return "\\Leftrightarrow".to_string(),
        _ => {}
    }
    let lower = cmd.to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "alpha" | "beta" | "gamma" | "delta" | "epsilon" | "varepsilon" | "zeta" | "eta"
        | "theta" | "vartheta" | "iota" | "kappa" | "lambda" | "mu" | "nu" | "xi" | "pi"
        | "varpi" | "rho" | "varrho" | "sigma" | "varsigma" | "tau" | "upsilon" | "phi"
        | "varphi" | "chi" | "psi" | "omega" | "sin" | "cos" | "tan" | "csc" | "sec" | "cot"
        | "arcsin" | "arccos" | "arctan" | "sinh" | "cosh" | "tanh" | "log" | "ln" | "lg"
        | "exp" | "max" | "min" | "det" | "dim" | "gcd" | "deg" | "ker" | "hom" | "arg"
        | "times" | "div" | "cdot" | "pm" | "mp" | "circ" | "bullet" | "star" | "ast" | "oplus"
        | "otimes" | "odot" | "approx" | "equiv" | "sim" | "simeq" | "cong" | "propto" | "ll"
        | "gg" | "subset" | "supset" | "subseteq" | "supseteq" | "in" | "notin" | "ni" | "cup"
        | "cap" | "emptyset" | "uparrow" | "downarrow" | "mapsto" | "partial" | "nabla"
        | "forall" | "exists" | "neg" | "land" | "lor" | "therefore" | "because" | "cdots"
        | "ldots" | "vdots" | "ddots" | "aleph" | "hbar" | "ell" | "wp" | "angle" | "triangle"
        | "perp" | "parallel" | "mid" | "langle" | "rangle" | "lfloor" | "rfloor" | "lceil"
        | "rceil" | "coprod" | "bigcup" | "bigcap" => return format!("\\{lower}"),
        "dint" => "\\iint",
        "tint" => "\\iiint",
        "vert" | "line" => "|",
        "dvert" | "dline" => "\\|",
        "omicron" => "o",
        "lcm" => "\\operatorname{lcm}",
        "mod" => "\\bmod",
        "inf" | "infty" | "infin" => "\\infty",
        "leq" | "le" => "\\leq",
        "geq" | "ge" => "\\geq",
        "neq" | "ne" => "\\neq",
        "nothing" => "\\emptyset",
        "rarrow" | "rightarrow" | "to" => "\\rightarrow",
        "larrow" | "leftarrow" => "\\leftarrow",
        "lrarrow" | "leftrightarrow" => "\\leftrightarrow",
        "uarrow" => "\\uparrow",
        "darrow" => "\\downarrow",
        "implies" => "\\Rightarrow",
        "iff" => "\\Leftrightarrow",
        "exist" => "\\exists",
        "lnot" => "\\lnot",
        "dots" => "\\dots",
        "prime" => "'",
        "re" => "\\Re",
        "im" => "\\Im",
        _ => return cmd.to_string(),
    };
    mapped.to_string()
}

fn expr_to_latex(expr: &EqExpr) -> String {
    match expr {
        EqExpr::Raw(s) => s.clone(),
        EqExpr::Group(inner) => exprs_to_latex(inner),
        EqExpr::Frac(n, d) => format!("\\frac{{{}}}{{{}}}", exprs_to_latex(n), exprs_to_latex(d)),
        EqExpr::Atop(t, b) => format!("{{{} \\atop {}}}", exprs_to_latex(t), exprs_to_latex(b)),
        EqExpr::Sqrt(None, r) => format!("\\sqrt{{{}}}", exprs_to_latex(r)),
        EqExpr::Sqrt(Some(i), r) => {
            format!("\\sqrt[{}]{{{}}}", exprs_to_latex(i), exprs_to_latex(r))
        }
        EqExpr::Sup(b, s) => format!("{}^{}", expr_to_latex(b), wrap_script(s)),
        EqExpr::Sub(b, s) => format!("{}_{}", expr_to_latex(b), wrap_script(s)),
        EqExpr::SubSup(b, sub, sup) => {
            format!(
                "{}_{}^{}",
                expr_to_latex(b),
                wrap_script(sub),
                wrap_script(sup)
            )
        }
        EqExpr::LeftSub(b, s) => format!("{{}}_{}{}", wrap_script(s), expr_to_latex(b)),
        EqExpr::LeftSup(b, s) => format!("{{}}^{}{}", wrap_script(s), expr_to_latex(b)),
        EqExpr::BigOp(op, l, u) => {
            let mut out = op.clone();
            if let Some(lo) = l {
                out.push('_');
                out.push_str(&wrap_script(lo));
            }
            if let Some(up) = u {
                out.push('^');
                out.push_str(&wrap_script(up));
            }
            out
        }
        EqExpr::LeftRight(o, body, c) => format!("\\left{}{}\\right{}", o, exprs_to_latex(body), c),
        EqExpr::Matrix(env, rows) => {
            let body: Vec<String> = rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|c| exprs_to_latex(c))
                        .collect::<Vec<_>>()
                        .join(" & ")
                })
                .collect();
            format!("\\begin{{{env}}}{}\\end{{{env}}}", body.join(" \\\\ "))
        }
        EqExpr::Accent(cmd, a) | EqExpr::Font(cmd, a) => {
            format!("{cmd}{{{}}}", exprs_to_latex(a))
        }
        EqExpr::Binom(n, k) => format!("\\binom{{{}}}{{{}}}", exprs_to_latex(n), exprs_to_latex(k)),
    }
}

fn wrap_script(exprs: &[EqExpr]) -> String {
    let s = exprs_to_latex(exprs);
    if s.len() == 1 {
        s
    } else {
        format!("{{{s}}}")
    }
}

fn exprs_to_latex(exprs: &[EqExpr]) -> String {
    let mut out = String::new();
    for (i, expr) in exprs.iter().enumerate() {
        let piece = expr_to_latex(expr);
        if i > 0 && !out.is_empty() && !piece.is_empty() {
            let prev = out.as_bytes()[out.len() - 1];
            let first = piece.as_bytes()[0];
            let prev_cmd = out.rfind('\\').is_some_and(|p| {
                out[p + 1..].bytes().all(|b| b.is_ascii_alphabetic()) && out.len() > p + 1
            });
            let prev_bigop = matches!(exprs[i - 1], EqExpr::BigOp(..));
            if ((prev_cmd || prev_bigop) && first.is_ascii_alphanumeric())
                || (matches!(prev, b'+' | b'-' | b'=') && first != b' ')
                || (matches!(first, b'+' | b'-' | b'=') && prev != b'{' && prev != b' ')
            {
                out.push(' ');
            }
        }
        out.push_str(&piece);
    }
    out
}

pub fn convert_eqedit_to_latex(script: &str) -> Result<String, EasyOfficeError> {
    let tokens = tokenize_eqedit(script)?;
    let mut parser = EqParser::new(tokens);
    let exprs = parser.parse_expr_until(false, false, false, 0)?;
    Ok(exprs_to_latex(&exprs))
}
