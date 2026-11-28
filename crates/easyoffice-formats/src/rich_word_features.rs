//! The Word-style features of a document (PD-395) that DOCX stores and the others carry as far as they can:
//! field instructions (date, time, cross reference, caption number), bookmarks, section breaks, line numbers,
//! the watermark and the document properties. Every value that comes from a file is checked here before it is kept:
//! an instruction that is not on the list, a bookmark name that Word would refuse and a number outside its bounds
//! read as absent.

/// Longest field instruction a document keeps, in bytes (the instruction is text of the file, not a program).
pub const MAX_FIELD_CODE_BYTES: usize = 256;
/// Longest bookmark name (the Word program's limit is 40 characters).
pub const MAX_BOOKMARK_CHARS: usize = 40;
/// Most bookmarks a document keeps.
pub const MAX_BOOKMARKS: usize = 1000;
/// Most section breaks a document keeps (a section of a file past this reads as part of the one before it).
pub const MAX_SECTION_BREAKS: usize = 1000;
/// Longest watermark text, in characters.
pub const MAX_WATERMARK_CHARS: usize = 64;
/// Longest value of a document property, in bytes.
pub const MAX_PROPERTY_BYTES: usize = 1024;
/// Largest `w:lnNumType` number (start, step or distance from the text, in twips).
pub const MAX_LINE_NUMBER_VALUE: u32 = 31_680;

/// The fields a document keeps; any other instruction (`INCLUDETEXT`, `DDE`, `MACROBUTTON`, `HYPERLINK`, ...) is read as its
/// result text only.
const FIELD_VERBS: [&str; 4] = ["DATE", "TIME", "REF", "SEQ"];

/// The instruction text of a field the document keeps: the verb (`DATE`, `TIME`, `REF`, `SEQ`) and its switches with the
/// white space collapsed. `None` for any other instruction, an over-long one and one with a control character.
pub fn safe_field_code(instruction: &str) -> Option<String> {
    let collapsed = instruction.split_whitespace().collect::<Vec<_>>().join(" ");
    let verb = collapsed.split(' ').next()?;
    if collapsed.len() > MAX_FIELD_CODE_BYTES
        || !FIELD_VERBS.iter().any(|v| verb.eq_ignore_ascii_case(v))
        || collapsed.chars().any(char::is_control)
    {
        return None;
    }
    // The verb is kept in capitals, as the Word program writes it.
    let mut parts = collapsed.splitn(2, ' ');
    let verb = parts.next()?.to_ascii_uppercase();
    Some(match parts.next() {
        Some(rest) => format!("{verb} {rest}"),
        None => verb,
    })
}

/// The kind of field an instruction names, for the writers that have a counterpart.
pub fn field_code_verb(code: &str) -> &str {
    code.split(' ').next().unwrap_or("")
}

/// A bookmark name as the Word program accepts it: 1 to 40 letters, digits and underscores, starting with a letter or an
/// underscore. The hidden names the table of contents writes (`_Toc...`) and the cursor mark `_GoBack` are not ours.
pub fn safe_bookmark_name(name: &str) -> Option<String> {
    let mut chars = name.chars();
    let first = chars.next()?;
    let valid = (first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
        && name.chars().count() <= MAX_BOOKMARK_CHARS;
    (valid && !name.starts_with("_Toc") && name != "_GoBack").then(|| name.to_string())
}

/// How a section begins (DOCX `w:type` of the section properties).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RichSectionBreak {
    /// On the next page.
    NextPage,
    /// On the same page, below the text of the section before.
    Continuous,
    /// On the next even-numbered page.
    EvenPage,
    /// On the next odd-numbered page.
    OddPage,
}

impl RichSectionBreak {
    /// The `w:type` value of the DOCX section properties (`nextPage`, `continuous`, `evenPage`, `oddPage`).
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::NextPage => "nextPage",
            Self::Continuous => "continuous",
            Self::EvenPage => "evenPage",
            Self::OddPage => "oddPage",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        [
            Self::NextPage,
            Self::Continuous,
            Self::EvenPage,
            Self::OddPage,
        ]
        .into_iter()
        .find(|kind| kind.wire_name() == name)
    }
}

/// When the line numbers start again (DOCX `w:lnNumType/@w:restart`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichLineNumberRestart {
    /// On every page (the attribute's default).
    #[default]
    EachPage,
    /// In every section.
    EachSection,
    /// Never: one count through the document.
    Continuous,
}

impl RichLineNumberRestart {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::EachPage => "newPage",
            Self::EachSection => "newSection",
            Self::Continuous => "continuous",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        [Self::EachPage, Self::EachSection, Self::Continuous]
            .into_iter()
            .find(|kind| kind.wire_name() == name)
    }
}

/// Line numbers in the margin (DOCX `w:lnNumType`): the first number, how many lines between two numbers, the distance
/// from the text in twips (0 is the program's own distance) and when the count starts again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RichLineNumbers {
    pub start: u32,
    pub count_by: u32,
    pub distance_twips: u32,
    pub restart: RichLineNumberRestart,
}

impl RichLineNumbers {
    /// Line numbers from 1, on every line, the program's own distance, restarting on each page.
    pub const DEFAULT: Self = Self {
        start: 1,
        count_by: 1,
        distance_twips: 0,
        restart: RichLineNumberRestart::EachPage,
    };

    /// The numbers of a file within the bounds (`count_by` 1 and up), or `None`.
    pub fn checked(self) -> Option<Self> {
        (self.count_by >= 1
            && self.count_by <= MAX_LINE_NUMBER_VALUE
            && self.start <= MAX_LINE_NUMBER_VALUE
            && self.distance_twips <= MAX_LINE_NUMBER_VALUE)
            .then_some(self)
    }
}

/// The document properties of the Word program's 정보 pane (DOCX `docProps/core.xml`, ODT `meta.xml`). A document is
/// titled by its name unless `title` says otherwise: the readers leave `title` empty (the title they read is
/// [`crate::RichDocument::title`]); the editor sends it when the user wrote one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichProperties {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub comments: String,
}

impl RichProperties {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
            && self.author.is_empty()
            && self.subject.is_empty()
            && self.keywords.is_empty()
            && self.comments.is_empty()
    }

    /// The properties cut to [`MAX_PROPERTY_BYTES`] each (at a character boundary) and without control characters
    /// other than the line break of the comments.
    pub fn bounded(&self) -> Self {
        let clean = |text: &str, keep_newline: bool| {
            let mut kept: String = text
                .chars()
                .filter(|c| !c.is_control() || (keep_newline && *c == '\n'))
                .collect();
            if kept.len() > MAX_PROPERTY_BYTES {
                let mut end = MAX_PROPERTY_BYTES;
                while !kept.is_char_boundary(end) {
                    end -= 1;
                }
                kept.truncate(end);
            }
            kept
        };
        Self {
            title: clean(&self.title, false),
            author: clean(&self.author, false),
            subject: clean(&self.subject, false),
            keywords: clean(&self.keywords, false),
            comments: clean(&self.comments, true),
        }
    }
}

/// The text of a watermark within [`MAX_WATERMARK_CHARS`] characters and without control characters, or `None` when nothing
/// is left.
pub fn safe_watermark_text(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_WATERMARK_CHARS)
        .collect();
    let cleaned = cleaned.trim().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_listed_field_verbs_are_kept() {
        assert_eq!(
            safe_field_code("date  \\@ \"yyyy-MM-dd\"").as_deref(),
            Some("DATE \\@ \"yyyy-MM-dd\"")
        );
        assert_eq!(
            safe_field_code(" ref _Ref12 \\h ").as_deref(),
            Some("REF _Ref12 \\h")
        );
        assert_eq!(
            safe_field_code("SEQ 그림 \\* ARABIC").as_deref(),
            Some("SEQ 그림 \\* ARABIC")
        );
        for hostile in [
            "INCLUDETEXT \"C:\\\\x\"",
            "DDEAUTO cmd calc",
            "HYPERLINK \"http://x\"",
            "MACROBUTTON Run",
            "",
            "DATE\u{7}",
        ] {
            assert_eq!(safe_field_code(hostile), None, "{hostile:?}");
        }
        assert_eq!(safe_field_code(&format!("REF {}", "a".repeat(300))), None);
    }

    #[test]
    fn a_bookmark_name_is_one_the_word_program_accepts() {
        assert_eq!(safe_bookmark_name("_Ref123").as_deref(), Some("_Ref123"));
        assert_eq!(safe_bookmark_name("결론_1").as_deref(), Some("결론_1"));
        for bad in [
            "",
            "1abc",
            "a b",
            "a-b",
            "_Toc12",
            "_GoBack",
            &"a".repeat(41),
        ] {
            assert_eq!(safe_bookmark_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn line_numbers_and_properties_stay_inside_their_bounds() {
        assert!(RichLineNumbers::DEFAULT.checked().is_some());
        assert!(RichLineNumbers {
            count_by: 0,
            ..RichLineNumbers::DEFAULT
        }
        .checked()
        .is_none());
        assert!(RichLineNumbers {
            start: MAX_LINE_NUMBER_VALUE + 1,
            ..RichLineNumbers::DEFAULT
        }
        .checked()
        .is_none());
        let long = RichProperties {
            author: "가".repeat(2000),
            comments: "a\u{7}b\nc".to_string(),
            ..RichProperties::default()
        }
        .bounded();
        assert!(long.author.len() <= MAX_PROPERTY_BYTES);
        assert_eq!(long.comments, "ab\nc");
        assert_eq!(safe_watermark_text("  \u{7}  "), None);
        assert_eq!(
            safe_watermark_text(&"가".repeat(100)).map(|t| t.chars().count()),
            Some(MAX_WATERMARK_CHARS)
        );
    }
}
