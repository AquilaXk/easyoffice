//! Cleanroom XPath 1.0 lexer and recursive-descent parser for `FILTERXML`.
//!
//! 참조 구현: none (implemented cleanly from W3C XML Path Language (XPath) Version 1.0,
//! 1999-11-16, Sections 2, 3, and 4, with fail-closed structural bounds).

use crate::FormulaError;

/// Longest XPath expression accepted (bytes).
pub(crate) const MAX_XPATH_BYTES: usize = 4_096;
/// Maximum location steps across one XPath AST (expanded `//` adds one step).
pub(crate) const MAX_XPATH_STEPS: usize = 64;
/// Maximum parser recursion depth (predicates, parentheses, unary chains).
pub(crate) const MAX_XPATH_DEPTH: usize = 32;
/// Maximum AST expression nodes allocated for one XPath query.
pub(crate) const MAX_XPATH_AST_NODES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Child,
    Descendant,
    Parent,
    Ancestor,
    FollowingSibling,
    PrecedingSibling,
    Following,
    Preceding,
    Attribute,
    SelfNode,
    DescendantOrSelf,
    AncestorOrSelf,
}

impl Axis {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "child" => Self::Child,
            "descendant" => Self::Descendant,
            "parent" => Self::Parent,
            "ancestor" => Self::Ancestor,
            "following-sibling" => Self::FollowingSibling,
            "preceding-sibling" => Self::PrecedingSibling,
            "following" => Self::Following,
            "preceding" => Self::Preceding,
            "attribute" => Self::Attribute,
            "self" => Self::SelfNode,
            "descendant-or-self" => Self::DescendantOrSelf,
            "ancestor-or-self" => Self::AncestorOrSelf,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKindTest {
    AnyNode,
    Text,
    Comment,
    ProcessingInstruction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeTest {
    Star,
    Name(String),
    QName(String, String),
    PrefixStar(String),
    AnyNode,
    Text,
    Comment,
    Pi(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Step {
    pub(crate) axis: Axis,
    pub(crate) test: NodeTest,
    pub(crate) predicates: Vec<XExpr>,
}

impl Step {
    fn descendant_or_self_node() -> Self {
        Self {
            axis: Axis::DescendantOrSelf,
            test: NodeTest::AnyNode,
            predicates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Union,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum XExpr {
    LocPath {
        absolute: bool,
        steps: Vec<Step>,
    },
    Filter {
        base: Box<XExpr>,
        predicates: Vec<XExpr>,
        tail: Vec<Step>,
    },
    Call {
        name: String,
        args: Vec<XExpr>,
    },
    Literal(String),
    Number(f64),
    UnaryNeg(Box<XExpr>),
    Binary {
        op: BinOp,
        left: Box<XExpr>,
        right: Box<XExpr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Axis(Axis),
    Name(String),
    QName(String, String),
    PrefixStar(String),
    Star,
    NodeType(NodeKindTest),
    FuncName(String),
    Literal(String),
    Number(f64),
    At,
    Slash,
    SlashSlash,
    Dot,
    DotDot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Pipe,
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    MulOp,
    Div,
    Mod,
}

impl Tok {
    /// W3C XPath 1.0 Section 3.7 lexical rule: whether this token ends a primary/test/operand
    /// so a following `*` or keyword name is tokenized as an operator rather than a NodeTest.
    #[inline]
    fn ends_value(&self) -> bool {
        matches!(
            self,
            Self::Name(_)
                | Self::QName(_, _)
                | Self::PrefixStar(_)
                | Self::Star
                | Self::Number(_)
                | Self::Literal(_)
                | Self::Dot
                | Self::DotDot
                | Self::RParen
                | Self::RBracket
        )
    }

    #[inline]
    fn can_start_step(&self) -> bool {
        matches!(
            self,
            Self::Axis(_)
                | Self::Name(_)
                | Self::QName(_, _)
                | Self::PrefixStar(_)
                | Self::Star
                | Self::NodeType(_)
                | Self::At
                | Self::Dot
                | Self::DotDot
        )
    }
}

#[inline]
fn is_ncname_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

#[inline]
fn is_ncname_char(c: char) -> bool {
    is_ncname_start(c) || c.is_ascii_digit() || c == '-' || c == '.' || c == '\u{00B7}'
}

fn scan_ncname(input: &str, start: usize) -> Option<(&str, usize)> {
    let mut chars = input[start..].char_indices();
    let (_, first) = chars.next()?;
    if !is_ncname_start(first) {
        return None;
    }
    let mut end = input.len();
    for (rel, ch) in chars {
        if !is_ncname_char(ch) {
            end = start + rel;
            break;
        }
    }
    Some((&input[start..end], end))
}

#[inline]
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

fn scan_number(input: &str, start: usize) -> Result<(f64, usize), FormulaError> {
    let bytes = input.as_bytes();
    let mut i = start;
    let mut saw_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        saw_digit = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            saw_digit = true;
            i += 1;
        }
    }
    if !saw_digit {
        return Err(FormulaError::Value);
    }
    let n = input[start..i]
        .parse::<f64>()
        .map_err(|_| FormulaError::Value)?;
    if !n.is_finite() {
        return Err(FormulaError::Value);
    }
    Ok((n, i))
}

fn scan_ident_token(
    input: &str,
    start: usize,
    prev_is_value: bool,
) -> Result<(Tok, usize), FormulaError> {
    let (ident, mut i) = scan_ncname(input, start).ok_or(FormulaError::Value)?;
    if prev_is_value {
        return Ok((
            match ident {
                "or" => Tok::Or,
                "and" => Tok::And,
                "div" => Tok::Div,
                "mod" => Tok::Mod,
                _ => Tok::Name(ident.to_string()),
            },
            i,
        ));
    }
    let bytes = input.as_bytes();
    // Single `:` directly attached to the NCName introduces `prefix:*` or `prefix:local`.
    if bytes.get(i) == Some(&b':') && bytes.get(i + 1) != Some(&b':') {
        i += 1;
        if bytes.get(i) == Some(&b'*') {
            return Ok((Tok::PrefixStar(ident.to_string()), i + 1));
        }
        let (local, next_i) = scan_ncname(input, i).ok_or(FormulaError::Value)?;
        let ws_next = skip_ws(bytes, next_i);
        if bytes.get(ws_next) == Some(&b'(') {
            return Err(FormulaError::Value);
        }
        return Ok((Tok::QName(ident.to_string(), local.to_string()), next_i));
    }
    let ws = skip_ws(bytes, i);
    if bytes.get(ws) == Some(&b':') && bytes.get(ws + 1) == Some(&b':') {
        let axis = Axis::from_name(ident).ok_or(FormulaError::Value)?;
        return Ok((Tok::Axis(axis), ws + 2));
    }
    if bytes.get(ws) == Some(&b'(') {
        let tok = match ident {
            "node" => Tok::NodeType(NodeKindTest::AnyNode),
            "text" => Tok::NodeType(NodeKindTest::Text),
            "comment" => Tok::NodeType(NodeKindTest::Comment),
            "processing-instruction" => Tok::NodeType(NodeKindTest::ProcessingInstruction),
            _ => Tok::FuncName(ident.to_string()),
        };
        return Ok((tok, i));
    }
    Ok((Tok::Name(ident.to_string()), i))
}

fn tokenize(input: &str) -> Result<Vec<Tok>, FormulaError> {
    if input.len() > MAX_XPATH_BYTES || input.trim().is_empty() {
        return Err(FormulaError::Value);
    }
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut prev_is_value = false;
    while i < bytes.len() {
        i = skip_ws(bytes, i);
        if i >= bytes.len() {
            break;
        }
        let b = bytes[i];
        let (tok, next_i) = match b {
            b'/' => {
                if bytes.get(i + 1) == Some(&b'/') {
                    (Tok::SlashSlash, i + 2)
                } else {
                    (Tok::Slash, i + 1)
                }
            }
            b'.' => {
                if bytes.get(i + 1) == Some(&b'.') {
                    (Tok::DotDot, i + 2)
                } else if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    let (n, end) = scan_number(input, i)?;
                    (Tok::Number(n), end)
                } else {
                    (Tok::Dot, i + 1)
                }
            }
            b'@' => (Tok::At, i + 1),
            b'(' => (Tok::LParen, i + 1),
            b')' => (Tok::RParen, i + 1),
            b'[' => (Tok::LBracket, i + 1),
            b']' => (Tok::RBracket, i + 1),
            b',' => (Tok::Comma, i + 1),
            b'|' => (Tok::Pipe, i + 1),
            b'+' => (Tok::Plus, i + 1),
            b'-' => (Tok::Minus, i + 1),
            b'=' => (Tok::Eq, i + 1),
            b'!' if bytes.get(i + 1) == Some(&b'=') => (Tok::Ne, i + 2),
            b'<' if bytes.get(i + 1) == Some(&b'=') => (Tok::Le, i + 2),
            b'<' => (Tok::Lt, i + 1),
            b'>' if bytes.get(i + 1) == Some(&b'=') => (Tok::Ge, i + 2),
            b'>' => (Tok::Gt, i + 1),
            b'*' => {
                if prev_is_value {
                    (Tok::MulOp, i + 1)
                } else {
                    (Tok::Star, i + 1)
                }
            }
            b'\'' | b'"' => {
                let quote = b;
                let rest = &bytes[i + 1..];
                let rel_end = rest
                    .iter()
                    .position(|&c| c == quote)
                    .ok_or(FormulaError::Value)?;
                let s = input[i + 1..i + 1 + rel_end].to_string();
                (Tok::Literal(s), i + 1 + rel_end + 1)
            }
            c if c.is_ascii_digit() => {
                let (n, end) = scan_number(input, i)?;
                (Tok::Number(n), end)
            }
            _ => scan_ident_token(input, i, prev_is_value)?,
        };
        prev_is_value = tok.ends_value();
        tokens.push(tok);
        i = next_i;
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
    depth: usize,
    steps: usize,
    nodes: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Tok]) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
            steps: 0,
            nodes: 0,
        }
    }

    #[inline]
    fn peek(&self) -> Option<&'a Tok> {
        self.tokens.get(self.pos)
    }

    #[inline]
    fn advance(&mut self) -> Option<&'a Tok> {
        let tok = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(tok)
    }

    fn expect(&mut self, expected: &Tok) -> Result<(), FormulaError> {
        if self.peek() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(FormulaError::Value)
        }
    }

    #[inline]
    fn bump_node(&mut self) -> Result<(), FormulaError> {
        self.nodes += 1;
        if self.nodes > MAX_XPATH_AST_NODES {
            Err(FormulaError::Value)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn bump_step(&mut self) -> Result<(), FormulaError> {
        self.steps += 1;
        if self.steps > MAX_XPATH_STEPS {
            Err(FormulaError::Value)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn enter(&mut self) -> Result<(), FormulaError> {
        self.depth += 1;
        if self.depth > MAX_XPATH_DEPTH {
            Err(FormulaError::Value)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn parse_expr(&mut self) -> Result<XExpr, FormulaError> {
        self.enter()?;
        let res = self.parse_or();
        self.leave();
        res
    }

    fn parse_or(&mut self) -> Result<XExpr, FormulaError> {
        let mut lhs = self.parse_and()?;
        while self.peek() == Some(&Tok::Or) {
            self.advance();
            self.bump_node()?;
            let rhs = self.parse_and()?;
            lhs = XExpr::Binary {
                op: BinOp::Or,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<XExpr, FormulaError> {
        let mut lhs = self.parse_eq()?;
        while self.peek() == Some(&Tok::And) {
            self.advance();
            self.bump_node()?;
            let rhs = self.parse_eq()?;
            lhs = XExpr::Binary {
                op: BinOp::And,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_eq(&mut self) -> Result<XExpr, FormulaError> {
        let mut lhs = self.parse_rel()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Eq) => BinOp::Eq,
                Some(Tok::Ne) => BinOp::Ne,
                _ => break,
            };
            self.advance();
            self.bump_node()?;
            let rhs = self.parse_rel()?;
            lhs = XExpr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_rel(&mut self) -> Result<XExpr, FormulaError> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Lt) => BinOp::Lt,
                Some(Tok::Le) => BinOp::Le,
                Some(Tok::Gt) => BinOp::Gt,
                Some(Tok::Ge) => BinOp::Ge,
                _ => break,
            };
            self.advance();
            self.bump_node()?;
            let rhs = self.parse_add()?;
            lhs = XExpr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<XExpr, FormulaError> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.advance();
            self.bump_node()?;
            let rhs = self.parse_mul()?;
            lhs = XExpr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<XExpr, FormulaError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::MulOp) => BinOp::Mul,
                Some(Tok::Div) => BinOp::Div,
                Some(Tok::Mod) => BinOp::Mod,
                _ => break,
            };
            self.advance();
            self.bump_node()?;
            let rhs = self.parse_unary()?;
            lhs = XExpr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<XExpr, FormulaError> {
        if self.peek() == Some(&Tok::Minus) {
            self.advance();
            self.enter()?;
            self.bump_node()?;
            let inner = self.parse_unary();
            self.leave();
            return Ok(XExpr::UnaryNeg(Box::new(inner?)));
        }
        self.parse_union()
    }

    fn parse_union(&mut self) -> Result<XExpr, FormulaError> {
        let mut lhs = self.parse_path()?;
        while self.peek() == Some(&Tok::Pipe) {
            self.advance();
            self.bump_node()?;
            let rhs = self.parse_path()?;
            lhs = XExpr::Binary {
                op: BinOp::Union,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_path(&mut self) -> Result<XExpr, FormulaError> {
        self.bump_node()?;
        match self.peek() {
            Some(Tok::Slash) => {
                self.advance();
                let mut steps = Vec::new();
                if self.peek().is_some_and(Tok::can_start_step) {
                    self.parse_relative_steps(&mut steps)?;
                }
                Ok(XExpr::LocPath {
                    absolute: true,
                    steps,
                })
            }
            Some(Tok::SlashSlash) => {
                self.advance();
                let mut steps = Vec::new();
                self.bump_step()?;
                steps.push(Step::descendant_or_self_node());
                self.parse_relative_steps(&mut steps)?;
                Ok(XExpr::LocPath {
                    absolute: true,
                    steps,
                })
            }
            Some(Tok::LParen | Tok::Literal(_) | Tok::Number(_) | Tok::FuncName(_)) => {
                let base = self.parse_primary()?;
                let predicates = self.parse_predicates()?;
                let mut tail = Vec::new();
                match self.peek() {
                    Some(Tok::Slash) => {
                        self.advance();
                        self.parse_relative_steps(&mut tail)?;
                    }
                    Some(Tok::SlashSlash) => {
                        self.advance();
                        self.bump_step()?;
                        tail.push(Step::descendant_or_self_node());
                        self.parse_relative_steps(&mut tail)?;
                    }
                    _ => {}
                }
                if predicates.is_empty() && tail.is_empty() {
                    Ok(base)
                } else {
                    Ok(XExpr::Filter {
                        base: Box::new(base),
                        predicates,
                        tail,
                    })
                }
            }
            _ => {
                let mut steps = Vec::new();
                self.parse_relative_steps(&mut steps)?;
                Ok(XExpr::LocPath {
                    absolute: false,
                    steps,
                })
            }
        }
    }

    fn parse_primary(&mut self) -> Result<XExpr, FormulaError> {
        match self.advance().ok_or(FormulaError::Value)? {
            Tok::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&Tok::RParen)?;
                Ok(inner)
            }
            Tok::Literal(s) => Ok(XExpr::Literal(s.clone())),
            Tok::Number(n) => Ok(XExpr::Number(*n)),
            Tok::FuncName(name) => {
                self.expect(&Tok::LParen)?;
                let mut args = Vec::new();
                if self.peek() != Some(&Tok::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if self.peek() == Some(&Tok::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&Tok::RParen)?;
                Ok(XExpr::Call {
                    name: name.clone(),
                    args,
                })
            }
            _ => Err(FormulaError::Value),
        }
    }

    fn parse_relative_steps(&mut self, out: &mut Vec<Step>) -> Result<(), FormulaError> {
        out.push(self.parse_step()?);
        loop {
            match self.peek() {
                Some(Tok::Slash) => {
                    self.advance();
                    out.push(self.parse_step()?);
                }
                Some(Tok::SlashSlash) => {
                    self.advance();
                    self.bump_step()?;
                    out.push(Step::descendant_or_self_node());
                    out.push(self.parse_step()?);
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn parse_step(&mut self) -> Result<Step, FormulaError> {
        self.bump_step()?;
        match self.peek() {
            Some(Tok::Dot) => {
                self.advance();
                Ok(Step {
                    axis: Axis::SelfNode,
                    test: NodeTest::AnyNode,
                    predicates: Vec::new(),
                })
            }
            Some(Tok::DotDot) => {
                self.advance();
                Ok(Step {
                    axis: Axis::Parent,
                    test: NodeTest::AnyNode,
                    predicates: Vec::new(),
                })
            }
            _ => {
                let axis = match self.peek() {
                    Some(Tok::Axis(ax)) => {
                        let ax = *ax;
                        self.advance();
                        ax
                    }
                    Some(Tok::At) => {
                        self.advance();
                        Axis::Attribute
                    }
                    _ => Axis::Child,
                };
                let test = self.parse_node_test()?;
                let predicates = self.parse_predicates()?;
                Ok(Step {
                    axis,
                    test,
                    predicates,
                })
            }
        }
    }

    fn parse_node_test(&mut self) -> Result<NodeTest, FormulaError> {
        match self.advance().ok_or(FormulaError::Value)? {
            Tok::Star => Ok(NodeTest::Star),
            Tok::Name(n) => Ok(NodeTest::Name(n.clone())),
            Tok::QName(p, l) => Ok(NodeTest::QName(p.clone(), l.clone())),
            Tok::PrefixStar(p) => Ok(NodeTest::PrefixStar(p.clone())),
            Tok::NodeType(kind) => {
                self.expect(&Tok::LParen)?;
                let test = match kind {
                    NodeKindTest::AnyNode => NodeTest::AnyNode,
                    NodeKindTest::Text => NodeTest::Text,
                    NodeKindTest::Comment => NodeTest::Comment,
                    NodeKindTest::ProcessingInstruction => {
                        let target = if let Some(Tok::Literal(s)) = self.peek() {
                            let s = s.clone();
                            self.advance();
                            Some(s)
                        } else {
                            None
                        };
                        NodeTest::Pi(target)
                    }
                };
                self.expect(&Tok::RParen)?;
                Ok(test)
            }
            _ => Err(FormulaError::Value),
        }
    }

    fn parse_predicates(&mut self) -> Result<Vec<XExpr>, FormulaError> {
        let mut preds = Vec::new();
        while self.peek() == Some(&Tok::LBracket) {
            self.advance();
            preds.push(self.parse_expr()?);
            self.expect(&Tok::RBracket)?;
        }
        Ok(preds)
    }
}

/// Parses an XPath 1.0 expression with strict size, step, AST-node, and nesting bounds.
pub(crate) fn parse_xpath(input: &str) -> Result<XExpr, FormulaError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(&tokens);
    let expr = parser.parse_expr()?;
    if parser.peek().is_some() {
        return Err(FormulaError::Value);
    }
    Ok(expr)
}
