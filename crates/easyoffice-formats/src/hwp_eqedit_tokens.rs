//! EqEdit formula tokenizer and keyword table (`hwp_eqedit` submodule).

use crate::EasyOfficeError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EqToken {
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

#[rustfmt::skip]
const KNOWN_COMMANDS: &[&str] = &[
    "acute", "aleph", "alpha", "angle", "approx", "arccos", "arcsin", "arctan", "arg", "ast", "atop", "bar",
    "because", "beta", "bigcap", "bigcup", "binom", "bmatrix", "bold", "breve", "bullet", "cap", "cases", "cdot",
    "cdots", "check", "chi", "choose", "circ", "color", "cong", "coprod", "cos", "cosh", "cot", "cpile",
    "csc", "cup", "darrow", "ddot", "ddots", "deg", "delta", "det", "dim", "dint", "div", "dline",
    "dmatrix", "dot", "dots", "downarrow", "dvert", "ell", "emptyset", "epsilon", "equiv", "eta", "exist", "exists",
    "exp", "forall", "from", "gamma", "gcd", "ge", "geq", "gg", "grave", "hat", "hbar", "hom",
    "iff", "im", "implies", "in", "inf", "infin", "infty", "int", "iota", "it", "kappa", "ker",
    "lambda", "land", "langle", "larrow", "lceil", "lcm", "ldots", "le", "left", "leftarrow", "leftrightarrow", "leq",
    "lfloor", "lg", "lim", "line", "ll", "ln", "lnot", "log", "lor", "lpile", "lrarrow", "lsub",
    "lsup", "mapsto", "mathring", "matrix", "max", "mid", "min", "mod", "mp", "mu", "nabla", "ne",
    "neg", "neq", "ni", "nothing", "notin", "nu", "odot", "of", "oint", "omega", "omicron", "oplus",
    "otimes", "over", "overbrace", "overline", "overrightarrow", "parallel", "partial", "perp", "phi", "pi", "pile", "pm",
    "pmatrix", "prime", "prod", "propto", "psi", "rangle", "rarrow", "rceil", "re", "rfloor", "rho", "right",
    "rightarrow", "rm", "root", "rpile", "scale", "sec", "sigma", "sim", "simeq", "sin", "sinh", "sqrt",
    "star", "sub", "subset", "subseteq", "sum", "sup", "supset", "supseteq", "tan", "tanh", "tau", "therefore",
    "theta", "tilde", "times", "tint", "to", "triangle", "uarrow", "underbrace", "underline", "uparrow", "upsilon", "varepsilon",
    "varphi", "varpi", "varrho", "varsigma", "vartheta", "vdots", "vec", "vert", "widehat", "widetilde", "wp", "xi",
    "zeta",
];

pub(crate) fn is_known_command(word: &str) -> bool {
    let mut lower_buf = [0u8; 32];
    if word.len() > lower_buf.len() {
        return false;
    }
    let lower = &mut lower_buf[..word.len()];
    for (dst, &src) in lower.iter_mut().zip(word.as_bytes()) {
        *dst = src.to_ascii_lowercase();
    }
    let Ok(needle) = std::str::from_utf8(lower) else {
        return false;
    };
    KNOWN_COMMANDS.binary_search(&needle).is_ok()
}

pub(crate) fn push_eqedit_word_tokens(word: String, tokens: &mut Vec<EqToken>) {
    if is_known_command(&word) {
        tokens.push(EqToken::Command(word));
    } else if let Some((prefix, rest)) = FONT_PREFIXES
        .iter()
        .find_map(|&p| {
            (word.len() > p.len() && word[..p.len()].eq_ignore_ascii_case(p))
                .then(|| (&word[..p.len()], &word[p.len()..]))
        })
        .filter(|&(_, rest)| is_known_command(rest) || rest.chars().count() == 1)
    {
        tokens.push(EqToken::Command(prefix.to_string()));
        if is_known_command(rest) {
            tokens.push(EqToken::Command(rest.to_string()));
        } else {
            tokens.push(EqToken::Text(rest.to_string()));
        }
    } else {
        tokens.extend(word.chars().map(|ch| EqToken::Text(ch.to_string())));
    }
}

pub(crate) fn tokenize_eqedit(input: &str) -> Result<Vec<EqToken>, EasyOfficeError> {
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
