//! List definitions of the DOCX, ODT and HWPX writers (PD-178, PD-234).
//!
//! A *run* is a stretch of consecutive list paragraphs of the same kind (bullet or ordered); a
//! non-list block ends it. Each run is one list in the output (`w:num` in DOCX, `text:list` in
//! ODT, `hh:numbering` in HWPX), so a numbered list after a body paragraph starts again at 1. The
//! number format and the punctuation of each level come from the first paragraph of the run at
//! that level ([`RichParagraph::list_number`], PD-157), the nesting from
//! [`RichParagraph::list_level`].
//!
//! 참조 구현: HWPX OWPML `hh:numberings`·`hh:bullets`·`hh:paraHead` 속성 및 7수준 매핑은
//! `rhwp` (`src/serializer/hwpx/header.rs` lines 448-460, 792-958, MIT, `THIRD_PARTY_NOTICES.md`)를
//! 따른다.

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
/// `hh:bullet@id` reserved for checkable task bullets (`☐` / `☑`, `checkable="1"`).
pub(crate) const HWPX_TASK_BULLET_ID: u32 = 4;

/// What a run's list looks like at every level.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ListDef {
    pub kind: RichListKind,
    pub checked: bool,
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
        let (kind, checked) = match &blocks[run_start] {
            RichBlock::Paragraph(p) => match list_kind_of(p) {
                Some(k) => (Some(k), k == RichListKind::Task && p.task_checked),
                None => (None, false),
            },
            _ => (None, false),
        };
        let Some(kind) = kind else {
            plan.block_run.push(None);
            run_start += 1;
            continue;
        };
        let run: Vec<&RichParagraph> = blocks[run_start..]
            .iter()
            .map_while(|block| match block {
                RichBlock::Paragraph(p)
                    if list_kind_of(p) == Some(kind)
                        && (kind != RichListKind::Task || p.task_checked == checked) =>
                {
                    Some(p)
                }
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
        let def = ListDef {
            kind,
            checked,
            levels,
        };
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

/// Whether `plan` needs `word/numbering.xml` (bullet or ordered runs; task lists use `w14:checkbox` SDTs).
pub(crate) fn has_docx_numbering(plan: &ListPlan) -> bool {
    plan.runs
        .iter()
        .any(|&d| plan.defs[d].kind != RichListKind::Task)
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
                RichListKind::Task => (
                    "bullet",
                    if def.checked { "\u{2611}" } else { "\u{2610}" }.to_string(),
                ),
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
                RichListKind::Task => xml.push_str(&format!(
                    "<text:list-level-style-bullet text:level=\"{level_number}\" text:bullet-char=\"{}\">{position}</text:list-level-style-bullet>",
                    if def.checked { "\u{2611}" } else { "\u{2610}" }
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

/// Levels an HWP5 / OWPML numbering head carries (`0..=6`; levels `7..=9` in `hh:paraHead` repeat level 6).
pub(crate) const HWPX_MAX_LIST_LEVEL: u8 = 6;
const HWPX_PARA_HEAD_SLOTS: usize = 10;

fn hwpx_format(format: RichNumberFormat) -> &'static str {
    match format {
        RichNumberFormat::Decimal => "DIGIT",
        RichNumberFormat::LowerLatin => "LATIN_SMALL",
        RichNumberFormat::UpperLatin => "LATIN_CAPITAL",
        RichNumberFormat::LowerRoman => "ROMAN_SMALL",
        RichNumberFormat::UpperRoman => "ROMAN_CAPITAL",
        RichNumberFormat::HangulSyllable => "HANGUL_SYLLABLE",
    }
}

/// 1-based `hh:numbering@id` for each run in `plan` (`0` for bullet runs).
pub(crate) fn hwpx_ordered_run_ids(plan: &ListPlan) -> Vec<u32> {
    let mut next_id = 0u32;
    plan.runs
        .iter()
        .map(|&def_idx| {
            if plan.defs[def_idx].kind == RichListKind::Ordered {
                next_id += 1;
                next_id
            } else {
                0
            }
        })
        .collect()
}

/// `hh:numberings` element for HWPX `header.xml` (one `hh:numbering` per ordered run so each run starts at 1).
pub(crate) fn hwpx_numberings_xml(plan: &ListPlan) -> String {
    use std::fmt::Write;
    let ordered_defs: Vec<&ListDef> = plan
        .runs
        .iter()
        .map(|&d| &plan.defs[d])
        .filter(|def| def.kind == RichListKind::Ordered)
        .collect();
    if ordered_defs.is_empty() {
        return String::new();
    }
    let mut xml = String::with_capacity(64 + ordered_defs.len() * 1_900);
    let _ = write!(xml, "<hh:numberings itemCnt=\"{}\">", ordered_defs.len());
    for (idx, def) in ordered_defs.iter().enumerate() {
        let _ = write!(xml, "<hh:numbering id=\"{}\" start=\"0\">", idx + 1);
        for slot in 0..HWPX_PARA_HEAD_SLOTS {
            let number = def.levels[slot.min(usize::from(HWPX_MAX_LIST_LEVEL))];
            let (before, after) = enclosure_affixes(number.enclosure);
            let lvl_1 = slot + 1;
            let _ = write!(
                xml,
                "<hh:paraHead start=\"1\" level=\"{lvl_1}\" align=\"LEFT\" useInstWidth=\"1\" autoIndent=\"1\" widthAdjust=\"0\" textOffsetType=\"PERCENT\" textOffset=\"50\" numFormat=\"{}\" charPrIDRef=\"4294967295\" checkable=\"0\">{before}^{lvl_1}{after}</hh:paraHead>",
                hwpx_format(number.format),
            );
        }
        xml.push_str("</hh:numbering>");
    }
    xml.push_str("</hh:numberings>");
    xml
}

/// `hh:bullets` element for HWPX `header.xml` covering bullet ids `1..=max_id` (`•`, `◦`, `▪`, plus checkable id `4`).
pub(crate) fn hwpx_bullets_xml(max_id: u32) -> String {
    use std::fmt::Write;
    if max_id == 0 {
        return String::new();
    }
    let has_task = max_id >= HWPX_TASK_BULLET_ID;
    let plain_count = usize::try_from(max_id).unwrap_or(1).min(BULLETS.len());
    let total_count = plain_count + usize::from(has_task);
    let mut xml = String::with_capacity(48 + total_count * 260);
    let _ = write!(xml, "<hh:bullets itemCnt=\"{total_count}\">");
    for (idx, glyph) in BULLETS.iter().enumerate().take(plain_count) {
        let _ = write!(
            xml,
            "<hh:bullet id=\"{}\" char=\"{glyph}\" useImage=\"0\"><hh:paraHead level=\"0\" align=\"LEFT\" useInstWidth=\"0\" autoIndent=\"1\" widthAdjust=\"0\" textOffsetType=\"PERCENT\" textOffset=\"50\" numFormat=\"DIGIT\" charPrIDRef=\"4294967295\" checkable=\"0\"/></hh:bullet>",
            idx + 1,
        );
    }
    if has_task {
        xml.push_str(concat!(
            "<hh:bullet id=\"4\" char=\"\u{2610}\" checkedChar=\"\u{2611}\" useImage=\"0\">",
            "<hh:paraHead level=\"0\" align=\"LEFT\" useInstWidth=\"0\" autoIndent=\"1\" widthAdjust=\"0\" textOffsetType=\"PERCENT\" textOffset=\"50\" numFormat=\"DIGIT\" charPrIDRef=\"4294967295\" checkable=\"1\"/>",
            "</hh:bullet>"
        ));
    }
    xml.push_str("</hh:bullets>");
    xml
}

/// Maximum `<hh:paraHead>` entries accepted in one HWPX `header.xml` before failing closed.
pub(crate) const MAX_HWPX_NUMBERING_HEADS: usize = 8_192;
const MAX_HWPX_BULLETS: usize = 1_024;
const HWPX_HEAD_TEXT_CAP: usize = 32;

fn decode_hwpx_num_format(num_format: Option<&str>) -> RichNumberFormat {
    match num_format.unwrap_or("DIGIT") {
        "LATIN_SMALL" => RichNumberFormat::LowerLatin,
        "LATIN_CAPITAL" => RichNumberFormat::UpperLatin,
        "ROMAN_SMALL" => RichNumberFormat::LowerRoman,
        "ROMAN_CAPITAL" => RichNumberFormat::UpperRoman,
        "HANGUL_SYLLABLE" | "HANGUL_JAMO" => RichNumberFormat::HangulSyllable,
        _ => RichNumberFormat::Decimal,
    }
}

fn decode_hwpx_enclosure(bytes: &[u8]) -> RichNumberEnclosure {
    let trimmed = bytes.trim_ascii();
    if trimmed.first() == Some(&b'(') && trimmed.last() == Some(&b')') {
        RichNumberEnclosure::Parentheses
    } else if trimmed.last() == Some(&b')') {
        RichNumberEnclosure::RightParenthesis
    } else {
        RichNumberEnclosure::Dot
    }
}

/// Collects `((numbering_id: u32) << 8 | level: u8) -> RichListNumber` while streaming HWPX `header.xml`
/// with zero heap allocation per `<hh:paraHead>` or paragraph lookup, failing closed past [`MAX_HWPX_NUMBERING_HEADS`].
#[derive(Default)]
pub(crate) struct HwpxNumberingReader {
    map: HashMap<u64, RichListNumber>,
    /// `hh:bullet@id` -> `1` (checkable `☐`/`☑`) or `2` (static checked `☑`/`☒`).
    bullets: HashMap<u32, u8>,
    cur_id: Option<u32>,
    cur_bullet_id: Option<u32>,
    cur_head: Option<(u8, RichNumberFormat, u8, [u8; HWPX_HEAD_TEXT_CAP])>,
    overflowed: bool,
}

impl HwpxNumberingReader {
    #[inline]
    const fn pack_key(id: u32, level: u8) -> u64 {
        (id as u64) << 8 | (level as u64)
    }

    pub(crate) fn on_start(&mut self, name: &str, attrs: &crate::xml_stream::Attrs<'_>) {
        use crate::xml_stream::attr;
        if self.overflowed {
            return;
        }
        match name {
            "hh:numbering" => {
                self.cur_id = attr(attrs, "id").and_then(|s| s.parse::<u32>().ok());
            }
            "hh:bullet" => {
                if self.bullets.len() >= MAX_HWPX_BULLETS {
                    self.overflowed = true;
                    return;
                }
                let bid = attr(attrs, "id").and_then(|s| s.parse::<u32>().ok());
                self.cur_bullet_id = bid;
                if let Some(id) = bid {
                    let ch = attr(attrs, "char")
                        .and_then(|s| s.chars().next())
                        .unwrap_or('\0');
                    let has_checked_char =
                        attr(attrs, "checkedChar").is_some_and(|s| !s.is_empty());
                    if has_checked_char || ch == '\u{2610}' {
                        self.bullets.insert(id, 1);
                    } else if matches!(ch, '\u{2611}' | '\u{2612}' | '\u{F0FE}') {
                        self.bullets.insert(id, 2);
                    }
                }
            }
            "hh:paraHead" if self.cur_bullet_id.is_some() => {
                if attr(attrs, "checkable").as_deref() == Some("1") {
                    if let Some(id) = self.cur_bullet_id {
                        self.bullets.insert(id, 1);
                    }
                }
            }
            "hh:paraHead" if self.cur_id.is_some() => {
                if self.map.len() >= MAX_HWPX_NUMBERING_HEADS {
                    self.overflowed = true;
                    self.cur_head = None;
                    return;
                }
                let lvl = attr(attrs, "level")
                    .and_then(|s| s.parse::<u8>().ok())
                    .and_then(|l| l.checked_sub(1));
                let fmt = decode_hwpx_num_format(attr(attrs, "numFormat").as_deref());
                if let Some(level) = lvl.filter(|&l| l < 10) {
                    self.cur_head = Some((level, fmt, 0, [0u8; HWPX_HEAD_TEXT_CAP]));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn on_text(&mut self, text: &str) {
        if let Some((_, _, len, buf)) = self.cur_head.as_mut() {
            let cur = usize::from(*len);
            let take = text.len().min(HWPX_HEAD_TEXT_CAP.saturating_sub(cur));
            if take > 0 {
                buf[cur..cur + take].copy_from_slice(&text.as_bytes()[..take]);
                *len = (cur + take) as u8;
            }
        }
    }

    pub(crate) fn on_end(&mut self, name: &str) {
        match name {
            "hh:paraHead" => {
                if let (Some(id), Some((lvl, format, len, buf))) =
                    (self.cur_id, self.cur_head.take())
                {
                    let enclosure = decode_hwpx_enclosure(&buf[..usize::from(len)]);
                    self.map.insert(
                        Self::pack_key(id, lvl),
                        RichListNumber { format, enclosure },
                    );
                }
            }
            "hh:numbering" => {
                self.cur_id = None;
                self.cur_head = None;
            }
            "hh:bullet" => {
                self.cur_bullet_id = None;
            }
            _ => {}
        }
    }

    pub(crate) fn check_budget(&self) -> Result<(), crate::EasyOfficeError> {
        if self.overflowed {
            return Err(crate::EasyOfficeError::ResourceBudgetExceeded(format!(
                "HWPX 번호·글머리표 머리글 개수가 한도({MAX_HWPX_NUMBERING_HEADS}개)를 넘었습니다."
            )));
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn bullet_task(&self, id: u32, para_checked: bool) -> Option<bool> {
        match self.bullets.get(&id).copied() {
            Some(1) => Some(para_checked),
            Some(2) => Some(true),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn get(&self, id: u32, level: u8) -> Option<RichListNumber> {
        if id == 0 {
            return None;
        }
        let num = self.map.get(&Self::pack_key(id, level)).copied()?;
        (num != RichListNumber::default()).then_some(num)
    }
}
