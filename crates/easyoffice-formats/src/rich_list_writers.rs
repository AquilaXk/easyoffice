//! List definitions of the DOCX and ODT writers (PD-172).
//!
//! A *run* is a stretch of consecutive list paragraphs of the same kind (bullet or ordered); a
//! non-list block ends it. Each run is one list in the output (`w:num` in DOCX, `text:list` in
//! ODT), so a numbered list after a body paragraph starts again at 1. The number format and the
//! punctuation of each level come from the first paragraph of the run at that level
//! ([`RichParagraph::list_number`], PD-157), the nesting from [`RichParagraph::list_level`].

use std::collections::HashMap;

use crate::rich_document::{
    RichBlock, RichListKind, RichListNumber, RichNumberEnclosure, RichNumberFormat, RichParagraph,
};

/// Levels a list defines (Word's nine; ODF allows ten).
pub(crate) const LIST_LEVELS: usize = 9;
/// Hanging indent of a list level, in twips (0.25 in), and the indent step per level.
const LEVEL_STEP_TWIPS: u32 = 720;
const HANGING_TWIPS: u32 = 360;
/// Bullet characters by level, repeating.
const BULLETS: [&str; 3] = ["\u{2022}", "\u{25E6}", "\u{25AA}"];

/// What a run's list looks like at every level.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ListDef {
    pub kind: RichListKind,
    pub levels: [RichListNumber; LIST_LEVELS],
}

/// The lists of a document: one definition per distinct look, one entry per run.
#[derive(Debug, Default)]
pub(crate) struct ListPlan {
    pub defs: Vec<ListDef>,
    /// Definition index of each run, in document order.
    pub runs: Vec<usize>,
    /// For each block, the run its paragraph belongs to.
    pub block_run: Vec<Option<usize>>,
}

/// A paragraph that is a list item: headings are never list items.
pub(crate) fn list_kind_of(p: &RichParagraph) -> Option<RichListKind> {
    if (1..=6).contains(&p.heading_level) {
        None
    } else {
        p.list
    }
}

pub(crate) fn level_of(p: &RichParagraph) -> usize {
    usize::from(p.list_level).min(LIST_LEVELS - 1)
}

pub(crate) fn plan_lists(blocks: &[RichBlock]) -> ListPlan {
    let mut plan = ListPlan::default();
    let mut index: HashMap<ListDef, usize> = HashMap::new();
    let mut run_start = 0;
    while run_start < blocks.len() {
        let kind = match &blocks[run_start] {
            RichBlock::Paragraph(p) => list_kind_of(p),
            _ => None,
        };
        let Some(kind) = kind else {
            plan.block_run.push(None);
            run_start += 1;
            continue;
        };
        let run: Vec<&RichParagraph> = blocks[run_start..]
            .iter()
            .map_while(|block| match block {
                RichBlock::Paragraph(p) if list_kind_of(p) == Some(kind) => Some(p),
                _ => None,
            })
            .collect();
        let mut levels = [RichListNumber::default(); LIST_LEVELS];
        let mut seen = [false; LIST_LEVELS];
        for p in &run {
            let level = level_of(p);
            if !seen[level] {
                seen[level] = true;
                levels[level] = p.list_number.unwrap_or_default();
            }
        }
        let def = ListDef { kind, levels };
        let def_index = *index.entry(def.clone()).or_insert_with(|| {
            plan.defs.push(def);
            plan.defs.len() - 1
        });
        let run_index = plan.runs.len();
        plan.runs.push(def_index);
        plan.block_run
            .extend(std::iter::repeat_n(Some(run_index), run.len()));
        run_start += run.len();
    }
    plan
}

fn word_format(format: RichNumberFormat) -> &'static str {
    match format {
        RichNumberFormat::Decimal => "decimal",
        RichNumberFormat::LowerLatin => "lowerLetter",
        RichNumberFormat::UpperLatin => "upperLetter",
        RichNumberFormat::LowerRoman => "lowerRoman",
        RichNumberFormat::UpperRoman => "upperRoman",
        RichNumberFormat::HangulSyllable => "ganada",
    }
}

fn odf_format(format: RichNumberFormat) -> &'static str {
    match format {
        RichNumberFormat::Decimal => "1",
        RichNumberFormat::LowerLatin => "a",
        RichNumberFormat::UpperLatin => "A",
        RichNumberFormat::LowerRoman => "i",
        RichNumberFormat::UpperRoman => "I",
        RichNumberFormat::HangulSyllable => "가, 나, 다, ...",
    }
}

/// Text before and after the number: `1.`, `(1)`, `1)`.
fn enclosure_affixes(enclosure: RichNumberEnclosure) -> (&'static str, &'static str) {
    match enclosure {
        RichNumberEnclosure::Dot => ("", "."),
        RichNumberEnclosure::Parentheses => ("(", ")"),
        RichNumberEnclosure::RightParenthesis => ("", ")"),
    }
}

/// `word/numbering.xml` (ECMA-376 17.9): one `w:abstractNum` per definition, then one `w:num` per
/// run (`numId` = run index + 1). Ordered runs restart at 1 on every level.
pub(crate) fn docx_numbering_xml(plan: &ListPlan) -> String {
    let mut xml = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">"
    ));
    for (id, def) in plan.defs.iter().enumerate() {
        xml.push_str(&format!(
            "<w:abstractNum w:abstractNumId=\"{id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>"
        ));
        for (level, number) in def.levels.iter().enumerate() {
            let (format, text) = match def.kind {
                RichListKind::Bullet => ("bullet", BULLETS[level % BULLETS.len()].to_string()),
                RichListKind::Ordered => {
                    let (before, after) = enclosure_affixes(number.enclosure);
                    (
                        word_format(number.format),
                        format!("{before}%{}{after}", level + 1),
                    )
                }
            };
            let left = LEVEL_STEP_TWIPS * (level as u32 + 1);
            xml.push_str(&format!(
                concat!(
                    "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/>",
                    "<w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/>",
                    "<w:pPr><w:ind w:left=\"{left}\" w:hanging=\"{hanging}\"/></w:pPr></w:lvl>"
                ),
                level = level,
                format = format,
                text = text,
                left = left,
                hanging = HANGING_TWIPS
            ));
        }
        xml.push_str("</w:abstractNum>");
    }
    for (run, def) in plan.runs.iter().enumerate() {
        xml.push_str(&format!(
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"{def}\"/>",
            run + 1
        ));
        if plan.defs[*def].kind == RichListKind::Ordered {
            for level in 0..LIST_LEVELS {
                xml.push_str(&format!(
                    "<w:lvlOverride w:ilvl=\"{level}\"><w:startOverride w:val=\"1\"/></w:lvlOverride>"
                ));
            }
        }
        xml.push_str("</w:num>");
    }
    xml.push_str("</w:numbering>");
    xml
}

/// The `w:numPr` of a list paragraph of run `run` (zero-based).
pub(crate) fn docx_num_pr(run: usize, p: &RichParagraph) -> String {
    format!(
        "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{}\"/></w:numPr>",
        level_of(p),
        run + 1
    )
}

/// Name of the ODF list style of definition `def` (zero-based).
pub(crate) fn odt_list_style_name(def: usize) -> String {
    format!("L{}", def + 1)
}

/// `text:list-style` elements (ODF 1.2 16.30), one per definition, for the automatic styles.
pub(crate) fn odt_list_styles_xml(plan: &ListPlan) -> String {
    let mut xml = String::new();
    for (id, def) in plan.defs.iter().enumerate() {
        xml.push_str(&format!(
            "<text:list-style style:name=\"{}\">",
            odt_list_style_name(id)
        ));
        for (level, number) in def.levels.iter().enumerate() {
            let left = LEVEL_STEP_TWIPS * (level as u32 + 1);
            let position = format!(
                concat!(
                    "<style:list-level-properties text:list-level-position-and-space-mode=\"label-alignment\">",
                    "<style:list-level-label-alignment text:label-followed-by=\"listtab\" ",
                    "text:list-tab-stop-position=\"{left}twip\" fo:text-indent=\"-{hanging}twip\" ",
                    "fo:margin-left=\"{left}twip\"/></style:list-level-properties>"
                ),
                left = left,
                hanging = HANGING_TWIPS
            );
            let level_number = level + 1;
            match def.kind {
                RichListKind::Bullet => xml.push_str(&format!(
                    "<text:list-level-style-bullet text:level=\"{level_number}\" text:bullet-char=\"{}\">{position}</text:list-level-style-bullet>",
                    BULLETS[level % BULLETS.len()]
                )),
                RichListKind::Ordered => {
                    let (before, after) = enclosure_affixes(number.enclosure);
                    let prefix = if before.is_empty() {
                        String::new()
                    } else {
                        format!(" style:num-prefix=\"{before}\"")
                    };
                    xml.push_str(&format!(
                        "<text:list-level-style-number text:level=\"{level_number}\"{prefix} style:num-suffix=\"{after}\" style:num-format=\"{}\">{position}</text:list-level-style-number>",
                        odf_format(number.format)
                    ));
                }
            }
        }
        xml.push_str("</text:list-style>");
    }
    xml
}

/// Where the next list paragraph sits among the open `text:list` elements of an ODT body.
#[derive(Debug, Default)]
pub(crate) struct OdtListWriter {
    run: Option<usize>,
    depth: usize,
}

impl OdtListWriter {
    /// Opens and closes lists so that a paragraph of `level` in `run` can be written next. The
    /// innermost item stays open for the paragraph and for lists nested inside it.
    pub(crate) fn enter(&mut self, body: &mut String, plan: &ListPlan, run: usize, level: usize) {
        if self.run != Some(run) {
            self.leave(body);
            self.run = Some(run);
        }
        let target = level + 1;
        while self.depth > target {
            body.push_str("</text:list-item></text:list>");
            self.depth -= 1;
        }
        if self.depth == target {
            body.push_str("</text:list-item><text:list-item>");
            return;
        }
        while self.depth < target {
            if self.depth == 0 {
                let name = odt_list_style_name(plan.runs[run]);
                body.push_str(&format!("<text:list text:style-name=\"{name}\">"));
            } else {
                body.push_str("<text:list>");
            }
            body.push_str("<text:list-item>");
            self.depth += 1;
        }
    }

    /// Closes every open list (a non-list block follows, or the body ends).
    pub(crate) fn leave(&mut self, body: &mut String) {
        for _ in 0..self.depth {
            body.push_str("</text:list-item></text:list>");
        }
        self.depth = 0;
        self.run = None;
    }
}
