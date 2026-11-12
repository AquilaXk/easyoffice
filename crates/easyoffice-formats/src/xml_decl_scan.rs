//! Declaration scan of [`XmlSanitizer::validate_safe_xml`]: `<!DOCTYPE` and `<!ENTITY` are refused,
//! the same words inside comments, CDATA sections and processing instructions are text.

use crate::{EasyOfficeError, XmlSanitizer};

/// Finds the `<?...?>` processing instructions of an XML text in the gap before a position, so the
/// `<!` scan can tell a declaration from the same bytes inside an instruction. Each byte is looked
/// at once: `from` only moves forward, and the next `?` and the lack of any later `?>` are kept.
#[derive(Default)]
struct ProcessingInstructions {
    from: usize,
    next_question: Option<Option<usize>>,
    no_more_ends: bool,
}

impl ProcessingInstructions {
    /// Marks everything before `end` as scanned (a comment or CDATA section was skipped).
    fn skip_to(&mut self, end: usize) {
        self.from = self.from.max(end);
    }

    /// End of the instruction that covers the `<` at `lt`, when there is one.
    fn end_covering(&mut self, bytes: &[u8], lt: usize) -> Option<usize> {
        while !self.no_more_ends && self.from < lt {
            let question = match self.next_question {
                Some(Some(at)) if at >= self.from => Some(at),
                Some(None) => None,
                _ => XmlSanitizer::find_byte_swar(bytes, self.from.max(1), b'?'),
            };
            self.next_question = Some(question);
            let at = question.filter(|&at| at < lt)?;
            self.from = at + 1;
            if bytes[at - 1] != b'<' {
                continue;
            }
            match XmlSanitizer::find_after(bytes, at, b"?>") {
                Some(end) if end > lt => {
                    self.from = end;
                    return Some(end);
                }
                Some(end) => self.from = end,
                None => self.no_more_ends = true,
            }
        }
        self.from = self.from.max(lt);
        None
    }
}

impl XmlSanitizer {
    /// Rejects `<!DOCTYPE` and `<!ENTITY` declarations (any case, whitespace after `<!`) and
    /// accepts the same words inside comments, CDATA sections and processing instructions, where
    /// they are text. A comment or CDATA section that never closes is refused (Fail-Closed).
    ///
    /// The three skipped constructs end at the earliest terminator a parser could use (`<!-->`
    /// closes itself, `<?>` is a complete instruction, an unclosed instruction hides nothing), so
    /// no reading of the input can hide a declaration from this scan. Only `!` is searched, as it
    /// is rare in markup; processing instructions are looked up only in the gap before a `<!`.
    pub fn validate_safe_xml(xml: &str) -> Result<(), EasyOfficeError> {
        let bytes = xml.as_bytes();
        let mut instructions = ProcessingInstructions::default();
        let mut cursor = 1usize;
        while let Some(bang) = Self::find_byte_swar(bytes, cursor, b'!') {
            cursor = bang + 1;
            if bytes[bang - 1] != b'<' {
                continue;
            }
            if let Some(end) = instructions.end_covering(bytes, bang - 1) {
                cursor = end;
                continue;
            }
            let rest = &bytes[bang + 1..];
            if rest.starts_with(b"--") {
                cursor = Self::find_after(bytes, bang + 1, b"-->")
                    .ok_or_else(|| Self::unclosed("XML 주석이 닫히지 않았습니다."))?;
                instructions.skip_to(cursor);
            } else if rest.starts_with(b"[CDATA[") {
                cursor = Self::find_after(bytes, bang + 8, b"]]>")
                    .ok_or_else(|| Self::unclosed("CDATA가 닫히지 않았습니다."))?;
                instructions.skip_to(cursor);
            } else {
                let word = &rest[rest.iter().take_while(|b| b.is_ascii_whitespace()).count()..];
                if word.len() >= 6 && word[..6].eq_ignore_ascii_case(b"ENTITY")
                    || word.len() >= 7 && word[..7].eq_ignore_ascii_case(b"DOCTYPE")
                {
                    return Err(EasyOfficeError::XxeOrExternalEntityDetected);
                }
            }
        }
        Ok(())
    }

    /// Byte-at-a-time reading of the rules of [`Self::validate_safe_xml`], kept as the reference
    /// for the equivalence test and the benchmark.
    pub fn validate_safe_xml_scalar_baseline(xml: &str) -> Result<(), EasyOfficeError> {
        let b = xml.as_bytes();
        let find = |from: usize, needle: &[u8]| {
            (from..b.len())
                .find(|&i| b[i..].starts_with(needle))
                .map(|i| i + needle.len())
        };
        let mut i = 0;
        while i < b.len() {
            let rest = &b[i..];
            if rest.starts_with(b"<?") {
                i = find(i + 1, b"?>").unwrap_or(i + 1);
            } else if rest.starts_with(b"<!--") {
                i = find(i + 2, b"-->")
                    .ok_or_else(|| Self::unclosed("XML 주석이 닫히지 않았습니다."))?;
            } else if rest.starts_with(b"<![CDATA[") {
                i = find(i + 9, b"]]>")
                    .ok_or_else(|| Self::unclosed("CDATA가 닫히지 않았습니다."))?;
            } else {
                if let Some(decl) = rest.strip_prefix(b"<!") {
                    let word = decl.trim_ascii_start();
                    if word.len() >= 6 && word[..6].eq_ignore_ascii_case(b"ENTITY")
                        || word.len() >= 7 && word[..7].eq_ignore_ascii_case(b"DOCTYPE")
                    {
                        return Err(EasyOfficeError::XxeOrExternalEntityDetected);
                    }
                }
                i += 1;
            }
        }
        Ok(())
    }

    fn unclosed(message: &str) -> EasyOfficeError {
        EasyOfficeError::CorruptedContainer(message.to_string())
    }

    /// Index just past the first `needle` that starts at `from` or later.
    fn find_after(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
        let last = *needle.last()?;
        let mut at = from + needle.len() - 1;
        while let Some(hit) = Self::find_byte_swar(bytes, at, last) {
            if bytes[hit + 1 - needle.len()..=hit] == *needle {
                return Some(hit + 1);
            }
            at = hit + 1;
        }
        None
    }
}
