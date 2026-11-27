//! The implicit intersection operator in a formula (PD-217, PD-363): the page writes `@A1:A5`, a
//! file writes `_xlfn.SINGLE(A1:A5)`.
//!
//! The text of a file is untrusted (PD-262). Both directions run on an explicit stack, never on the
//! call stack, and take time linear in the token count: matching brackets come from one pass, and
//! the end of an operand is looked up once per start. A formula nested deeper than `MAX_NESTING`
//! levels (the limit of the spreadsheet format) is not converted; the callers keep its text.

use super::xlsx_formula_text::Tok;

/// Levels of nested functions and brackets the spreadsheet format allows.
pub(crate) const MAX_NESTING: usize = 64;

/// Marks "no matching bracket" and "operand has no end" in the index tables.
const NONE: u32 = u32::MAX;
/// Marks an operand start not looked up yet.
const UNSET: u32 = u32::MAX - 1;

fn is_space(tok: &Tok) -> bool {
    matches!(tok, Tok::Punct(c) if c.is_whitespace())
}

fn skip_space(tokens: &[Tok], mut i: usize) -> usize {
    while tokens.get(i).is_some_and(is_space) {
        i += 1;
    }
    i
}

/// For every `(` and `{`, the index after its closing bracket; `NONE` when it has none.
struct Brackets {
    after: Vec<u32>,
}

impl Brackets {
    /// `None` when brackets nest deeper than `MAX_NESTING`.
    fn of(tokens: &[Tok]) -> Option<Self> {
        let mut after = vec![NONE; tokens.len()];
        let mut parens: Vec<usize> = Vec::new();
        let mut braces: Vec<usize> = Vec::new();
        for (i, tok) in tokens.iter().enumerate() {
            match tok {
                Tok::Punct('(') => parens.push(i),
                Tok::Punct('{') => braces.push(i),
                Tok::Punct(')') => {
                    if let Some(open) = parens.pop() {
                        after[open] = (i + 1) as u32;
                    }
                }
                Tok::Punct('}') => {
                    if let Some(open) = braces.pop() {
                        after[open] = (i + 1) as u32;
                    }
                }
                _ => continue,
            }
            if parens.len() + braces.len() > MAX_NESTING {
                return None;
            }
        }
        Some(Self { after })
    }

    fn after(&self, open: usize) -> Option<usize> {
        match self.after[open] {
            NONE => None,
            index => Some(index as usize),
        }
    }
}

/// The end of the operand of an `@`, looked up once per start index.
struct Operands<'a> {
    tokens: &'a [Tok],
    brackets: &'a Brackets,
    memo: Vec<u32>,
    /// Starts whose answer is the answer of the start after them (`@@A1`, `A1:B1:C1`).
    chain: Vec<usize>,
}

impl<'a> Operands<'a> {
    fn new(tokens: &'a [Tok], brackets: &'a Brackets) -> Self {
        Self {
            tokens,
            brackets,
            memo: vec![UNSET; tokens.len() + 1],
            chain: Vec::new(),
        }
    }

    /// The index after the operand that begins at `start` (spaces first), or `None`.
    fn end(&mut self, start: usize) -> Option<usize> {
        let tokens = self.tokens;
        let mut from = start;
        let result = 'walk: loop {
            match self.memo[from] {
                UNSET => {}
                NONE => break None,
                known => break Some(known as usize),
            }
            self.chain.push(from);
            let i = skip_space(tokens, from);
            let mut end = match tokens.get(i) {
                Some(Tok::Punct('@' | '+' | '-')) => {
                    from = i + 1;
                    continue;
                }
                Some(Tok::Punct('(' | '{')) => match self.brackets.after(i) {
                    Some(after) => after,
                    None => break None,
                },
                Some(Tok::Quoted(_)) => i + 1,
                Some(Tok::Word(_)) => {
                    let next = if tokens.get(i + 1) == Some(&Tok::Punct('(')) {
                        match self.brackets.after(i + 1) {
                            Some(after) => after,
                            None => break None,
                        }
                    } else {
                        i + 1
                    };
                    if matches!(tokens.get(next), Some(Tok::Quoted(q)) if q.starts_with('[')) {
                        next + 1
                    } else {
                        next
                    }
                }
                Some(Tok::Punct(_)) | None => break None,
            };
            while let Some(Tok::Punct(op)) = tokens.get(end) {
                match op {
                    '!' | ':' => {
                        from = end + 1;
                        continue 'walk;
                    }
                    '%' => end += 1,
                    _ => break,
                }
            }
            break Some(end);
        };
        let code = result.map_or(NONE, |end| end as u32);
        for start in self.chain.drain(..) {
            self.memo[start] = code;
        }
        result
    }
}

/// Unary `@<operand>` as `_xlfn.SINGLE(<operand>)`. `None` when the text nests deeper than
/// `MAX_NESTING`.
pub(crate) fn single_calls(tokens: Vec<Tok>) -> Option<Vec<Tok>> {
    if !tokens.contains(&Tok::Punct('@')) {
        return Some(tokens);
    }
    let brackets = Brackets::of(&tokens)?;
    let mut operands = Operands::new(&tokens, &brackets);
    let mut out = Vec::with_capacity(tokens.len() + 16);
    // The end of each open `_xlfn.SINGLE(`, innermost last.
    let mut closes: Vec<usize> = Vec::new();
    let mut i = 0;
    loop {
        while closes.last() == Some(&i) {
            closes.pop();
            out.push(Tok::Punct(')'));
        }
        if i >= tokens.len() {
            return Some(out);
        }
        if tokens[i] == Tok::Punct('@') {
            let op_start = skip_space(&tokens, i + 1);
            let end = operands.end(op_start);
            // An operand that runs past the call around it is no operand there.
            if let Some(end) = end.filter(|&end| closes.last().is_none_or(|&top| end <= top)) {
                if closes.len() == MAX_NESTING {
                    return None;
                }
                out.push(Tok::Word("_xlfn.SINGLE".to_string()));
                out.push(Tok::Punct('('));
                closes.push(end);
                i = op_start;
                continue;
            }
        }
        out.push(tokens[i].clone());
        i += 1;
    }
}

fn has_top_level_comma(tokens: &[Tok]) -> bool {
    let mut depth = 0usize;
    for tok in tokens {
        match tok {
            Tok::Punct('(' | '{') => depth += 1,
            Tok::Punct(')' | '}') => depth = depth.saturating_sub(1),
            Tok::Punct(',') if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// The index after the bracket that closes the `open` bracket at `start`.
fn matching_delim(tokens: &[Tok], start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, tok) in tokens[start..].iter().enumerate() {
        match tok {
            Tok::Punct(c) if *c == open => depth += 1,
            Tok::Punct(c) if *c == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(start + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_outer_parenthesized(tokens: &[Tok]) -> bool {
    let start = skip_space(tokens, 0);
    let mut end = tokens.len();
    while end > start && is_space(&tokens[end - 1]) {
        end -= 1;
    }
    tokens.get(start) == Some(&Tok::Punct('('))
        && matching_delim(tokens, start, '(', ')') == Some(end)
}

fn has_single_top_level_op(tokens: &[Tok]) -> bool {
    let mut depth = 0usize;
    for tok in tokens {
        match tok {
            Tok::Punct('(' | '{') => depth += 1,
            Tok::Punct(')' | '}') => depth = depth.saturating_sub(1),
            Tok::Punct(c)
                if depth == 0
                    && !c.is_whitespace()
                    && !matches!(c, '!' | ':' | '#' | '%' | '@') =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// An open `SINGLE(` whose operand is being copied to the output.
struct Call {
    /// Index of the `)` that closes it.
    close: usize,
    /// Where its operand starts in the output.
    operand_at: usize,
    /// `@` before a `:` needs parentheses around it.
    before_colon: bool,
}

/// `SINGLE(<operand>)` as `@<operand>` (or `@(<operand>)` when top-level operators appear inside).
/// `None` when the text nests deeper than `MAX_NESTING`.
pub(crate) fn single_operators(tokens: Vec<Tok>) -> Option<Vec<Tok>> {
    let is_single = |t: &Tok| matches!(t, Tok::Word(w) if w.eq_ignore_ascii_case("SINGLE"));
    if !tokens.iter().any(is_single) {
        return Some(tokens);
    }
    let brackets = Brackets::of(&tokens)?;
    let mut out = Vec::with_capacity(tokens.len());
    let mut calls: Vec<Call> = Vec::new();
    let mut i = 0;
    loop {
        while let Some(call) = calls.last().filter(|call| call.close == i) {
            let (operand_at, before_colon) = (call.operand_at, call.before_colon);
            calls.pop();
            let operand = &out[operand_at..];
            if !is_outer_parenthesized(operand) && has_single_top_level_op(operand) {
                out.insert(operand_at, Tok::Punct('('));
                out.push(Tok::Punct(')'));
            }
            if before_colon {
                out.push(Tok::Punct(')'));
            }
            i += 1;
        }
        if i >= tokens.len() {
            return Some(out);
        }
        if is_single(&tokens[i]) && tokens.get(i + 1) == Some(&Tok::Punct('(')) {
            if let Some(end) = brackets.after(i + 1) {
                let raw_inner = &tokens[i + 2..end - 1];
                let has_content = raw_inner.iter().any(|t| !is_space(t));
                if has_content && !has_top_level_comma(raw_inner) {
                    let before_colon = tokens.get(end) == Some(&Tok::Punct(':'));
                    if before_colon {
                        out.push(Tok::Punct('('));
                    }
                    out.push(Tok::Punct('@'));
                    calls.push(Call {
                        close: end - 1,
                        operand_at: out.len(),
                        before_colon,
                    });
                    i += 2;
                    continue;
                }
            }
        }
        out.push(tokens[i].clone());
        i += 1;
    }
}
