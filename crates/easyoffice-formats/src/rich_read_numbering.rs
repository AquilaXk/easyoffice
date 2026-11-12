//! DOCX `word/numbering.xml` (ECMA-376 17.9) for the reader: each `w:num` resolves through its
//! `w:abstractNum` to the kind and number format of every level (`w:lvl/@w:ilvl` 0..=8), with the
//! levels that the `w:num` overrides in `w:lvlOverride/w:lvl` replaced.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::xml_stream::{attr, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichListKind, RichListNumber, RichNumberEnclosure, RichNumberFormat};

/// Levels of a list definition: `w:ilvl` is 0..=8 (ECMA-376 17.9.3).
pub(crate) const LIST_LEVELS: usize = 9;
/// Largest number of `w:abstractNum` and of `w:num` elements read from one part (a document has
/// a few dozen); more is rejected instead of grown without bound.
const MAX_DEFINITIONS: usize = 8_192;

/// Kind and number format of one list level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LevelDef {
    pub(crate) kind: RichListKind,
    /// `None` for a bullet and for the plain `1.`.
    pub(crate) number: Option<RichListNumber>,
}

impl LevelDef {
    /// What a list paragraph is when no definition says (the reader's earlier default).
    const FALLBACK: Self = Self {
        kind: RichListKind::Bullet,
        number: None,
    };
}

type Levels = [Option<LevelDef>; LIST_LEVELS];

/// `numId` -> the levels of its definition.
#[derive(Default)]
pub(crate) struct Numbering {
    nums: HashMap<String, Levels>,
}

impl Numbering {
    /// The definition of `level` of list `num_id`. A level the definition lacks takes the first
    /// level it has; an unknown list is a bullet.
    pub(crate) fn level(&self, num_id: &str, level: usize) -> LevelDef {
        let Some(levels) = self.nums.get(num_id) else {
            return LevelDef::FALLBACK;
        };
        levels
            .get(level)
            .copied()
            .flatten()
            .or_else(|| levels.iter().copied().flatten().next())
            .unwrap_or(LevelDef::FALLBACK)
    }
}

/// The `w:lvl` being read: its level and the two attributes that decide its definition.
struct OpenLevel {
    level: usize,
    format: Option<String>,
    text: Option<String>,
}

impl OpenLevel {
    fn finish(self) -> Option<(usize, LevelDef)> {
        let format = self.format?;
        Some((self.level, level_def(&format, self.text.as_deref())))
    }
}

/// A `w:num` and the levels it overrides.
struct OpenNum {
    id: String,
    abstract_id: Option<String>,
    overrides: Vec<(usize, LevelDef)>,
}

fn level_index(value: Option<Cow<'_, str>>) -> Option<usize> {
    let level = value?.trim().parse::<usize>().ok()?;
    (level < LIST_LEVELS).then_some(level)
}

/// The kind and format of a `w:numFmt` value and the punctuation of its `w:lvlText`.
fn level_def(format: &str, text: Option<&str>) -> LevelDef {
    let number_format = match format {
        "bullet" | "none" => {
            return LevelDef {
                kind: RichListKind::Bullet,
                number: None,
            }
        }
        "lowerLetter" => RichNumberFormat::LowerLatin,
        "upperLetter" => RichNumberFormat::UpperLatin,
        "lowerRoman" => RichNumberFormat::LowerRoman,
        "upperRoman" => RichNumberFormat::UpperRoman,
        "ganada" => RichNumberFormat::HangulSyllable,
        // Decimal and every format the model has no name for.
        _ => RichNumberFormat::Decimal,
    };
    let text = text.unwrap_or("").trim();
    let enclosure = match (text.starts_with('('), text.ends_with(')')) {
        (true, true) => RichNumberEnclosure::Parentheses,
        (false, true) => RichNumberEnclosure::RightParenthesis,
        _ => RichNumberEnclosure::Dot,
    };
    let number = RichListNumber {
        format: number_format,
        enclosure,
    };
    LevelDef {
        kind: RichListKind::Ordered,
        number: (number != RichListNumber::default()).then_some(number),
    }
}

fn count_definition(count: &mut usize) -> Result<(), EasyOfficeError> {
    *count += 1;
    if *count > MAX_DEFINITIONS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "DOCX 번호 정의가 허용 개수({MAX_DEFINITIONS})를 넘었습니다."
        )));
    }
    Ok(())
}

/// Reads `word/numbering.xml`.
pub(crate) fn read_numbering(xml: &str) -> Result<Numbering, EasyOfficeError> {
    let mut abstracts: HashMap<String, Levels> = HashMap::new();
    let mut nums: Vec<OpenNum> = Vec::new();
    let mut open_abstract: Option<String> = None;
    let mut open_num: Option<OpenNum> = None;
    let mut open_level: Option<OpenLevel> = None;
    let (mut abstract_count, mut num_count) = (0usize, 0usize);
    for event in XmlReader::new(xml) {
        match event? {
            XmlEvent::Start { name, attrs } => match name {
                "w:abstractNum" => {
                    count_definition(&mut abstract_count)?;
                    let id = attr(&attrs, "w:abstractNumId").map(Cow::into_owned);
                    open_abstract = id;
                }
                "w:num" => {
                    count_definition(&mut num_count)?;
                    open_num = attr(&attrs, "w:numId").map(|id| OpenNum {
                        id: id.into_owned(),
                        abstract_id: None,
                        overrides: Vec::new(),
                    });
                }
                "w:abstractNumId" => {
                    if let Some(num) = open_num.as_mut() {
                        num.abstract_id = attr(&attrs, "w:val").map(Cow::into_owned);
                    }
                }
                "w:lvl" => {
                    open_level = level_index(attr(&attrs, "w:ilvl")).map(|level| OpenLevel {
                        level,
                        format: None,
                        text: None,
                    });
                }
                "w:numFmt" => {
                    if let Some(level) = open_level.as_mut() {
                        level.format = attr(&attrs, "w:val").map(Cow::into_owned);
                    }
                }
                "w:lvlText" => {
                    if let Some(level) = open_level.as_mut() {
                        level.text = attr(&attrs, "w:val").map(Cow::into_owned);
                    }
                }
                _ => {}
            },
            XmlEvent::End("w:lvl") => {
                if let Some((level, def)) = open_level.take().and_then(OpenLevel::finish) {
                    if let Some(num) = open_num.as_mut() {
                        num.overrides.push((level, def));
                    } else if let Some(levels) = open_abstract
                        .as_ref()
                        .map(|id| abstracts.entry(id.clone()).or_default())
                    {
                        levels[level] = Some(def);
                    }
                }
            }
            XmlEvent::End("w:num") => nums.extend(open_num.take()),
            XmlEvent::End("w:abstractNum") => open_abstract = None,
            _ => {}
        }
    }
    let nums = nums
        .into_iter()
        .map(|num| {
            let mut levels = num
                .abstract_id
                .as_ref()
                .and_then(|id| abstracts.get(id))
                .copied()
                .unwrap_or_default();
            for (level, def) in num.overrides {
                levels[level] = Some(def);
            }
            (num.id, levels)
        })
        .collect();
    Ok(Numbering { nums })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

    fn numbering(body: &str) -> Numbering {
        read_numbering(&format!("<w:numbering {W}>{body}</w:numbering>")).unwrap()
    }

    fn ordered(format: RichNumberFormat, enclosure: RichNumberEnclosure) -> LevelDef {
        LevelDef {
            kind: RichListKind::Ordered,
            number: Some(RichListNumber { format, enclosure }),
        }
    }

    #[test]
    fn levels_resolve_per_level_through_the_abstract_definition() {
        let n = numbering(concat!(
            "<w:abstractNum w:abstractNumId=\"7\">",
            "<w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl>",
            "<w:lvl w:ilvl=\"1\"><w:numFmt w:val=\"lowerLetter\"/><w:lvlText w:val=\"%2)\"/></w:lvl>",
            "<w:lvl w:ilvl=\"2\"><w:numFmt w:val=\"upperRoman\"/><w:lvlText w:val=\"(%3)\"/></w:lvl>",
            "<w:lvl w:ilvl=\"3\"><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"o\"/></w:lvl>",
            "<w:lvl w:ilvl=\"4\"><w:numFmt w:val=\"ganada\"/><w:lvlText w:val=\"%5.\"/></w:lvl>",
            "</w:abstractNum>",
            "<w:num w:numId=\"2\"><w:abstractNumId w:val=\"7\"/></w:num>"
        ));
        let plain = LevelDef {
            kind: RichListKind::Ordered,
            number: None,
        };
        assert_eq!(n.level("2", 0), plain);
        assert_eq!(
            n.level("2", 1),
            ordered(
                RichNumberFormat::LowerLatin,
                RichNumberEnclosure::RightParenthesis
            )
        );
        assert_eq!(
            n.level("2", 2),
            ordered(
                RichNumberFormat::UpperRoman,
                RichNumberEnclosure::Parentheses
            )
        );
        assert_eq!(n.level("2", 3).kind, RichListKind::Bullet);
        assert_eq!(
            n.level("2", 4),
            ordered(RichNumberFormat::HangulSyllable, RichNumberEnclosure::Dot)
        );
        // A level the definition lacks takes its first level; an unknown list is a bullet.
        assert_eq!(n.level("2", 8), plain);
        assert_eq!(n.level("9", 0), LevelDef::FALLBACK);
    }

    #[test]
    fn a_num_overrides_levels_of_its_abstract_definition() {
        let n = numbering(concat!(
            "<w:abstractNum w:abstractNumId=\"0\">",
            "<w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/></w:lvl>",
            "<w:lvl w:ilvl=\"1\"><w:numFmt w:val=\"decimal\"/></w:lvl>",
            "</w:abstractNum>",
            "<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/>",
            "<w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride>",
            "<w:lvlOverride w:ilvl=\"1\"><w:lvl w:ilvl=\"1\"><w:numFmt w:val=\"upperLetter\"/></w:lvl></w:lvlOverride>",
            "</w:num>"
        ));
        assert_eq!(n.level("1", 0).number, None);
        assert_eq!(
            n.level("1", 1).number.unwrap().format,
            RichNumberFormat::UpperLatin
        );
    }

    #[test]
    fn hostile_levels_and_missing_parts_stay_inert() {
        let n = numbering(concat!(
            "<w:abstractNum w:abstractNumId=\"0\">",
            "<w:lvl w:ilvl=\"9\"><w:numFmt w:val=\"bullet\"/></w:lvl>",
            "<w:lvl w:ilvl=\"-1\"><w:numFmt w:val=\"bullet\"/></w:lvl>",
            "<w:lvl w:ilvl=\"99999999999999999999\"><w:numFmt w:val=\"bullet\"/></w:lvl>",
            "<w:lvl w:ilvl=\"x\"><w:numFmt w:val=\"bullet\"/></w:lvl>",
            "<w:lvl><w:numFmt w:val=\"bullet\"/></w:lvl>",
            "<w:lvl w:ilvl=\"0\"/>",
            "</w:abstractNum>",
            "<w:num w:numId=\"1\"><w:abstractNumId w:val=\"missing\"/></w:num>",
            "<w:num><w:abstractNumId w:val=\"0\"/></w:num>",
            "<w:num w:numId=\"3\"/>"
        ));
        for id in ["1", "3", "4"] {
            assert_eq!(n.level(id, 0), LevelDef::FALLBACK, "{id}");
        }
        assert_eq!(n.level("1", usize::MAX), LevelDef::FALLBACK);
    }

    #[test]
    fn the_definition_count_is_bounded() {
        let mut body = String::new();
        for id in 0..=MAX_DEFINITIONS {
            body.push_str(&format!("<w:num w:numId=\"{id}\"/>"));
        }
        let xml = format!("<w:numbering {W}>{body}</w:numbering>");
        assert!(matches!(
            read_numbering(&xml),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
        let within = format!(
            "<w:numbering {W}>{}</w:numbering>",
            "<w:num w:numId=\"1\"/>".repeat(MAX_DEFINITIONS)
        );
        assert!(read_numbering(&within).is_ok());
    }
}
