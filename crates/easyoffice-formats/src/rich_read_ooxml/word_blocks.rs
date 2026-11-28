//! The PD-401 parts of the DOCX walk: tab stops, paragraph borders and shading, the drop cap, the column separator and narrow
//! side, column breaks, text boxes (`w:pict` with `v:textbox`, `wps:txbx`), equations (OMML) and the styles of the user.
//! Rules after the Apache-2.0 renderer `docx-preview` (`parseFrame`, `parseBorderProperties` of `src/document-parser.ts`); the
//! bounds are the ones of [`crate::rich_word_blocks`].

use super::*;
use crate::rich_image_layout::RichImageWrap;
use crate::rich_word_blocks::{
    normalize_tab_stops, RichBorderLine, RichBorderStyle, RichColumnLook, RichDropCap,
    RichNarrowColumn, RichParaBorders, RichStyleDef, RichTabAlign, RichTabLeader, RichTabStop,
    RichTextBox, MAX_BORDER_EIGHTHS, MAX_BORDER_SPACE_PT, MAX_COLUMNS, MAX_COLUMN_TWIPS, MAX_MATHS,
    MAX_MATH_BYTES, MAX_MATH_DEPTH, MAX_MATH_NODES, MAX_TAB_STOPS, MAX_TEXT_BOXES,
    MAX_TEXT_BOX_PARAGRAPHS,
};
use crate::{normalize_hex_color, RichAlignment};

/// The elements of OMML an equation keeps (ECMA-376 22.1); the control properties (`m:ctrlPr`) and every other element, with
/// its whole subtree, are left out.
const OMML_NAMES: &[&str] = &[
    "m:oMath",
    "m:oMathPara",
    "m:oMathParaPr",
    "m:jc",
    "m:r",
    "m:rPr",
    "m:t",
    "m:sty",
    "m:scr",
    "m:nor",
    "m:lit",
    "m:brk",
    "m:aln",
    "m:f",
    "m:fPr",
    "m:type",
    "m:num",
    "m:den",
    "m:rad",
    "m:radPr",
    "m:degHide",
    "m:deg",
    "m:e",
    "m:sSup",
    "m:sSupPr",
    "m:sup",
    "m:sSub",
    "m:sSubPr",
    "m:sub",
    "m:sSubSup",
    "m:sSubSupPr",
    "m:sPre",
    "m:sPrePr",
    "m:d",
    "m:dPr",
    "m:begChr",
    "m:sepChr",
    "m:endChr",
    "m:grow",
    "m:nary",
    "m:naryPr",
    "m:chr",
    "m:limLoc",
    "m:subHide",
    "m:supHide",
    "m:func",
    "m:fName",
    "m:limLow",
    "m:limUpp",
    "m:lim",
    "m:eqArr",
    "m:m",
    "m:mr",
    "m:bar",
    "m:barPr",
    "m:pos",
    "m:groupChr",
    "m:groupChrPr",
    "m:limLowPr",
    "m:limUppPr",
    "m:funcPr",
    "m:eqArrPr",
    "m:mPr",
    "m:vertJc",
    "m:maxDist",
    "m:baseJc",
    "m:rSp",
    "m:rSpRule",
    "m:cGp",
    "m:cSp",
    "m:cGpRule",
    "m:mcs",
    "m:mc",
    "m:mcPr",
    "m:count",
    "m:mcJc",
    "m:acc",
    "m:accPr",
    "m:box",
    "m:boxPr",
    "m:borderBox",
    "m:borderBoxPr",
    "m:phant",
    "m:phantPr",
    "m:show",
    "m:zeroWid",
    "m:zeroAsc",
    "m:zeroDesc",
    "m:transp",
];

/// Whether a start tag is an OMML element of the list.
fn omml_name(name: &str) -> bool {
    OMML_NAMES.contains(&name)
}

/// The checked OMML and the linear text of the `m:oMath` or `m:oMathPara` that `events` hold, or `None` when the equation is
/// past a bound (size, depth, element count).
pub(super) fn read_math(events: &[XmlEvent<'_>]) -> Option<(String, String)> {
    let (mut omml, mut text) = (String::new(), String::new());
    // The open elements that were kept, so every end tag written closes the start tag written for it; `roots` counts the
    // top-level elements (one `m:oMath` or `m:oMathPara`, nothing else).
    let (mut open, mut roots): (Vec<&str>, usize) = (Vec::new(), 0);
    let (mut nodes, mut skipping) = (0usize, 0usize);
    let mut in_text = false;
    for event in events {
        match event {
            XmlEvent::Start { name, attrs } => {
                if skipping > 0 {
                    skipping += 1;
                    continue;
                }
                if !omml_name(name) {
                    // Outside the equation nothing is skipped: a foreign element there is not one equation.
                    if open.is_empty() {
                        return None;
                    }
                    skipping += 1;
                    continue;
                }
                if open.is_empty() {
                    roots += 1;
                    if roots > 1 || !matches!(*name, "m:oMath" | "m:oMathPara") {
                        return None;
                    }
                }
                open.push(name);
                nodes += 1;
                if open.len() > MAX_MATH_DEPTH || nodes > MAX_MATH_NODES {
                    return None;
                }
                omml.push('<');
                omml.push_str(name);
                if let Some(val) = attr(attrs, "m:val")
                    .filter(|v| v.len() <= 64 && !v.chars().any(char::is_control))
                {
                    omml.push_str(&format!(" m:val=\"{}\"", XmlSanitizer::escape(&val)));
                }
                omml.push('>');
                in_text = *name == "m:t";
            }
            XmlEvent::End(name) => {
                if skipping > 0 {
                    skipping -= 1;
                    continue;
                }
                // The name written is the one that was opened; an end tag of anything else is not this equation's.
                let opened = open.pop()?;
                if opened != *name {
                    return None;
                }
                omml.push_str(&format!("</{opened}>"));
                in_text = false;
            }
            XmlEvent::Text(t) if skipping == 0 && in_text => {
                omml.push_str(&XmlSanitizer::escape(t));
                text.push_str(t);
            }
            XmlEvent::Text(_) => {}
        }
        if omml.len() > MAX_MATH_BYTES {
            return None;
        }
    }
    (open.is_empty() && skipping == 0 && roots == 1 && !omml.is_empty()).then_some((omml, text))
}

/// The checked OMML and linear text of one equation given as text (a save request carries it): it has to be one `m:oMath` or
/// `m:oMathPara`, and every bound of [`read_math`] holds.
pub(crate) fn checked_omml(omml: &str) -> Option<(String, String)> {
    let events = XmlReader::new(omml).collect::<Result<Vec<_>, _>>().ok()?;
    read_math(&events).filter(|(checked, _)| checked.starts_with("<m:oMath"))
}

/// The look of the `w:cols` of the section being read: the separator line and the widths of the columns.
#[derive(Default)]
pub(super) struct ColumnLookState {
    separator: bool,
    widths: Vec<u32>,
}

impl ColumnLookState {
    pub(super) fn begin(&mut self, attrs: &Attrs<'_>) {
        self.separator = attr(attrs, "w:sep").is_some_and(|v| toggle_on(Some(v.as_ref())));
        self.widths.clear();
    }

    pub(super) fn push_width(&mut self, attrs: &Attrs<'_>) {
        if let (true, Some(w)) = (
            self.widths.len() < usize::from(MAX_COLUMNS),
            attr(attrs, "w:w").and_then(|v| v.trim().parse::<u64>().ok()),
        ) {
            // A width past the widest page reads as the widest, so the comparison below cannot overflow.
            self.widths.push(w.min(u64::from(MAX_COLUMN_TWIPS)) as u32);
        }
    }

    /// The look of the section: with two columns, the narrow one is the one that is two thirds or less of the other.
    pub(super) fn finish(&mut self) -> Option<RichColumnLook> {
        let narrow = match self.widths[..] {
            [a, b] if a != b && u64::from(a) * 3 <= u64::from(b) * 2 => RichNarrowColumn::First,
            [a, b] if a != b && u64::from(b) * 3 <= u64::from(a) * 2 => RichNarrowColumn::Last,
            _ => RichNarrowColumn::None,
        };
        let look = RichColumnLook {
            separator: std::mem::take(&mut self.separator),
            narrow,
        };
        self.widths.clear();
        (!look.is_default()).then_some(look)
    }
}

/// A border side of `w:pBdr`: `w:val`, `w:sz` (eighths of a point), `w:space` (points) and `w:color`.
fn border_line(attrs: &Attrs<'_>) -> Option<RichBorderLine> {
    let style = RichBorderStyle::from_docx(attr(attrs, "w:val").as_deref().unwrap_or("single"))?;
    let number = |key: &str, default: u32| {
        attr(attrs, key)
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(default)
    };
    Some(RichBorderLine {
        style,
        eighths: number("w:sz", 4).clamp(2, u32::from(MAX_BORDER_EIGHTHS)) as u8,
        color: attr(attrs, "w:color")
            .as_deref()
            .and_then(normalize_hex_color),
        space_pt: number("w:space", 0).min(u32::from(MAX_BORDER_SPACE_PT)) as u8,
    })
}

/// One child of `w:pPr` that belongs to PD-401: a tab stop, a border side, the shading, the frame of a drop cap. The run
/// properties of the paragraph mark (`w:rPr`) are not the paragraph's.
pub(super) fn apply_ppr(top: &mut ParaState, name: &str, attrs: &Attrs<'_>) {
    if name == "w:rPr" {
        top.in_mark_rpr = true;
    }
    if name == "w:pBdr" {
        top.in_pbdr = true;
    }
    if top.in_mark_rpr {
        return;
    }
    match name {
        "w:tab" => {
            let stop = attr(attrs, "w:val")
                .as_deref()
                .and_then(RichTabAlign::from_docx)
                .and_then(|align| {
                    Some(RichTabStop {
                        pos_twips: attr(attrs, "w:pos")?.trim().parse().ok()?,
                        align,
                        leader: RichTabLeader::from_docx(
                            attr(attrs, "w:leader").as_deref().unwrap_or("none"),
                        ),
                    })
                });
            if let (Some(stop), true) = (stop, top.word.tab_stops.len() < MAX_TAB_STOPS) {
                top.word.tab_stops.push(stop);
            }
        }
        // The sides are children of `w:pBdr` only: the `w:top` of a section's `w:pgBorders` is a page border.
        "w:top" | "w:left" | "w:bottom" | "w:right" if top.in_pbdr => {
            let borders = top
                .word
                .borders
                .get_or_insert_with(RichParaBorders::default);
            let line = border_line(attrs).and_then(RichBorderLine::checked);
            match name {
                "w:top" => borders.top = line,
                "w:left" => borders.left = line,
                "w:bottom" => borders.bottom = line,
                _ => borders.right = line,
            }
        }
        "w:shd" => {
            top.word.shading = attr(attrs, "w:fill")
                .as_deref()
                .and_then(normalize_hex_color)
        }
        "w:framePr" => {
            // docx-preview `parseFrame`: the paragraph of a drop cap is the one that floats left.
            let in_margin = match attr(attrs, "w:dropCap").as_deref() {
                Some("drop") => false,
                Some("margin") => true,
                _ => return,
            };
            // No `w:lines` is the word processor's three; a count that is not a small number is no drop cap.
            let lines = attr(attrs, "w:lines").map_or(Some(3), |v| v.trim().parse::<u8>().ok());
            top.word.drop_cap = lines.and_then(|lines| RichDropCap { lines, in_margin }.checked());
        }
        _ => {}
    }
}

/// The end of a paragraph's `w:rPr`: what follows is the paragraph's again.
pub(super) fn end_ppr_child(top: &mut ParaState, name: &str) {
    match name {
        "w:rPr" => top.in_mark_rpr = false,
        "w:pBdr" => top.in_pbdr = false,
        _ => {}
    }
}

/// What the finished paragraph of the walk keeps of PD-401: the tab stops in order (a table-of-contents entry writes its own),
/// the border and the shading when they hold something.
pub(super) fn finish_word(
    mut word: crate::rich_word_blocks::RichParagraphWord,
    is_toc: bool,
) -> crate::rich_word_blocks::RichParagraphWord {
    word.tab_stops = if is_toc {
        Vec::new()
    } else {
        normalize_tab_stops(word.tab_stops)
    };
    word.borders = word.borders.filter(|b| !b.is_empty());
    word
}

// ---------------------------------------------------------------- text boxes

/// Whether a `w:drawing` or `w:pict` holds a text box (it has a `w:txbxContent`).
pub(super) fn has_text_box(events: &[XmlEvent<'_>]) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            XmlEvent::Start {
                name: "w:txbxContent",
                ..
            }
        )
    })
}

/// A length of a VML `style` (`width:150pt`) in EMU.
fn vml_length_emu(style: &str, key: &str) -> Option<u32> {
    let value = style.split(';').find_map(|item| {
        let (k, v) = item.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })?;
    let split = value.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let number: f64 = value[..split].parse().ok()?;
    let per_unit = match &value[split..] {
        "pt" => 12_700.0,
        "in" => 914_400.0,
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "px" => 9_525.0,
        _ => return None,
    };
    let emu = (number * per_unit).round();
    (emu.is_finite() && (0.0..=f64::from(u32::MAX)).contains(&emu)).then_some(emu as u32)
}

/// The geometry of a text box found in the events around its content: size, wrap, fill and outline.
struct BoxLook {
    width: u32,
    height: u32,
    wrap: RichImageWrap,
    fill: Option<String>,
    border: bool,
}

fn box_look(events: &[XmlEvent<'_>]) -> BoxLook {
    let mut look = BoxLook {
        width: 0,
        height: 0,
        wrap: RichImageWrap::Square,
        fill: None,
        border: true,
    };
    let (mut behind, mut none_wrap, mut in_ln, mut seen_shape) = (false, false, 0usize, false);
    for event in events {
        match event {
            XmlEvent::Start {
                name: "w:txbxContent",
                ..
            } => break,
            XmlEvent::Start { name, attrs } => match *name {
                "wp:anchor" => {
                    behind = attr(attrs, "behindDoc").is_some_and(|v| toggle_on(Some(v.as_ref())))
                }
                "wp:extent" => {
                    let emu = |k: &str| {
                        attr(attrs, k)
                            .and_then(|v| v.trim().parse::<u32>().ok())
                            .unwrap_or(0)
                    };
                    (look.width, look.height) = (emu("cx"), emu("cy"));
                }
                "wp:wrapNone" => none_wrap = true,
                "wp:wrapTopAndBottom" => look.wrap = RichImageWrap::TopBottom,
                "a:ln" => in_ln += 1,
                "a:noFill" if in_ln > 0 => look.border = false,
                "a:srgbClr" if in_ln == 0 && look.fill.is_none() => {
                    look.fill = attr(attrs, "val").as_deref().and_then(normalize_hex_color)
                }
                "w10:wrap" => match attr(attrs, "type").as_deref() {
                    Some("topAndBottom") => look.wrap = RichImageWrap::TopBottom,
                    Some("none") => none_wrap = true,
                    _ => {}
                },
                "v:shape" | "v:rect" | "v:roundrect" if !seen_shape => {
                    if let Some(style) = attr(attrs, "style") {
                        seen_shape = true;
                        look.width = vml_length_emu(&style, "width").unwrap_or(0);
                        look.height = vml_length_emu(&style, "height").unwrap_or(0);
                        behind = style.contains("z-index:-");
                    }
                    look.fill = attr(attrs, "fillcolor")
                        .and_then(|c| normalize_hex_color(c.trim_start_matches('#')));
                    look.border = attr(attrs, "stroked").as_deref() != Some("f");
                }
                _ => {}
            },
            XmlEvent::End("a:ln") => in_ln = in_ln.saturating_sub(1),
            _ => {}
        }
    }
    if none_wrap {
        look.wrap = if behind {
            RichImageWrap::Behind
        } else {
            RichImageWrap::InFront
        };
    }
    look
}

impl DocxWalker {
    /// A text box of a body paragraph: the paragraphs of its content are read by a walker of their own (a picture, a box or an
    /// object inside is skipped, so a box never holds a box), and the box is kept on the paragraph that holds the anchor. A box
    /// past a bound (size, paragraph count, number of boxes) is dropped with a note.
    pub(super) fn on_text_box(
        &mut self,
        events: &[XmlEvent<'_>],
        resolver: &Resolver<'_>,
    ) -> Result<(), EasyOfficeError> {
        if self.tbl_depth > 0 || self.paras.is_empty() || self.text_box_total >= MAX_TEXT_BOXES {
            self.warn_once("가져오지 못한 텍스트 상자가 있습니다.".to_string());
            return Ok(());
        }
        self.text_box_total += 1;
        let Some(start) = events.iter().position(|e| {
            matches!(
                e,
                XmlEvent::Start {
                    name: "w:txbxContent",
                    ..
                }
            )
        }) else {
            return Ok(());
        };
        let look = box_look(events);
        let mut inner = DocxWalker::default();
        let mut skipping = 0usize;
        // A picture, a box, an equation or a table inside the box is not read; the document says so once.
        let mut dropped = false;
        for event in &events[start + 1..] {
            match event {
                XmlEvent::Start { name, attrs } => {
                    let foreign = matches!(
                        *name,
                        "w:drawing" | "w:pict" | "w:txbxContent" | "m:oMath" | "m:oMathPara"
                    );
                    dropped |= foreign && skipping == 0;
                    if skipping > 0 || foreign {
                        skipping += 1;
                    } else if inner.on_start(name, attrs)? {
                        skipping = 1;
                    }
                }
                XmlEvent::End(_) if skipping > 0 => skipping -= 1,
                XmlEvent::End("w:txbxContent") => break,
                XmlEvent::End(name) => inner.on_end(name, resolver)?,
                XmlEvent::Text(text) if skipping == 0 => inner.on_text(text)?,
                XmlEvent::Text(_) => {}
            }
        }
        dropped |= inner
            .blocks
            .iter()
            .any(|b| !matches!(b, RichBlock::Paragraph(_)));
        if dropped {
            self.warn_once(
                "텍스트 상자 안의 그림·수식·표·텍스트 상자는 가져오지 않았습니다.".to_string(),
            );
        }
        let paragraphs: Vec<RichParagraph> = inner
            .blocks
            .into_iter()
            .filter_map(|b| match b {
                RichBlock::Paragraph(p) => Some(p),
                _ => None,
            })
            .take(MAX_TEXT_BOX_PARAGRAPHS + 1)
            .collect();
        let boxed = RichTextBox {
            paragraphs,
            width_emu: look.width,
            height_emu: look.height,
            wrap: look.wrap,
            fill: look.fill,
            border: look.border,
        }
        .checked();
        match (boxed, self.paras.last_mut()) {
            (Some(b), Some(top)) => top.word.text_boxes.push(b),
            _ => self.warn_once("가져오지 못한 텍스트 상자가 있습니다.".to_string()),
        }
        Ok(())
    }

    /// An equation of a body paragraph: a run that holds its OMML and its linear text. An equation past a bound, or one past
    /// the number a document keeps, is read as its linear text alone.
    pub(super) fn on_math(&mut self, events: &[XmlEvent<'_>]) -> Result<(), EasyOfficeError> {
        if self.paras.is_empty() {
            return Ok(());
        }
        let in_cell = self.tbl_depth > 0;
        let linear = || -> String {
            events
                .iter()
                .scan(false, |in_t, e| {
                    Some(match e {
                        XmlEvent::Start { name, .. } => {
                            *in_t = *name == "m:t";
                            None
                        }
                        XmlEvent::End(_) => {
                            *in_t = false;
                            None
                        }
                        XmlEvent::Text(t) if *in_t => Some(t.to_string()),
                        XmlEvent::Text(_) => None,
                    })
                })
                .flatten()
                .collect()
        };
        // A table cell keeps no equation (its paragraphs carry text only): the text of the equation stays.
        let math = (!in_cell && self.math_total < MAX_MATHS)
            .then(|| read_math(events))
            .flatten();
        let (omml, text) = match math {
            Some((omml, text)) => (Some(omml), text),
            None => {
                self.warn_once("가져오지 못한 수식이 있습니다.".to_string());
                (None, linear())
            }
        };
        self.text_budget.charge(
            text.len() + omml.as_ref().map_or(0, String::len),
            "DOCX 수식",
        )?;
        self.math_total += usize::from(omml.is_some());
        if let Some(top) = self.paras.last_mut() {
            top.runs.push(RichRun {
                text,
                math: omml,
                ..RichRun::default()
            });
        }
        Ok(())
    }

    /// A paragraph of a table cell: the cell keeps its text and the look of its runs, not the tab stops, border, shading, drop
    /// cap or text box of PD-401 (the document says so once).
    pub(super) fn note_cell_paragraph(&mut self, p: &RichParagraph) {
        let w = &p.word;
        if !w.tab_stops.is_empty()
            || w.borders.is_some()
            || w.shading.is_some()
            || w.drop_cap.is_some()
            || !w.text_boxes.is_empty()
        {
            self.warn_once(
                "표 칸 안 문단의 탭 정지·테두리·음영·텍스트 상자는 가져오지 않았습니다."
                    .to_string(),
            );
        }
    }

    /// A `w:br w:type="column"`: text after it starts the column, so a break before any text belongs to this paragraph and one
    /// after text to the next.
    pub(super) fn note_column_break(&mut self) {
        let before_text = self.cur_text.trim().is_empty();
        if let Some(top) = self.paras.last_mut() {
            if before_text && top.runs_are_blank() {
                top.word.column_break_before = true;
            } else {
                top.column_break_after = true;
            }
        }
    }
}

// ---------------------------------------------------------------- styles of the user

/// The look of one `w:style` of a paragraph the user made (`w:customStyle`), read from its `w:pPr` and `w:rPr`.
pub(super) struct StyleBuilder {
    def: RichStyleDef,
    para: ParaState,
    fmt: CharFmt,
    in_ppr: bool,
    in_rpr: bool,
    /// The style holds something the editor does not keep: a next or linked style, a base other than the normal style, a
    /// paragraph or run property outside the look of [`RichStyleDef`].
    pub(super) lossy: bool,
}

/// The paragraph and run properties of a style of the user that [`RichStyleDef`] keeps.
const STYLE_PPR_KEPT: [&str; 3] = ["w:jc", "w:ind", "w:spacing"];
const STYLE_RPR_KEPT: [&str; 11] = [
    "w:rFonts", "w:sz", "w:szCs", "w:b", "w:bCs", "w:i", "w:iCs", "w:u", "w:color", "w:lang",
    "w:kern",
];

impl StyleBuilder {
    /// A builder for a paragraph style the user made; `None` for any other style.
    pub(super) fn start(attrs: &Attrs<'_>) -> Option<Self> {
        let custom = attr(attrs, "w:customStyle").is_some_and(|v| toggle_on(Some(v.as_ref())));
        let paragraph = attr(attrs, "w:type").as_deref() == Some("paragraph");
        (custom && paragraph).then(|| Self {
            def: RichStyleDef::default(),
            para: ParaState::default(),
            fmt: CharFmt::default(),
            in_ppr: false,
            in_rpr: false,
            lossy: false,
        })
    }

    pub(super) fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "w:name" => {
                self.def.name = attr(attrs, "w:val")
                    .map(Cow::into_owned)
                    .unwrap_or_default()
            }
            "w:next" | "w:link" => self.lossy = true,
            "w:basedOn" => self.lossy |= attr(attrs, "w:val").as_deref() != Some("Normal"),
            "w:pPr" => self.in_ppr = true,
            "w:rPr" => self.in_rpr = true,
            _ if self.in_ppr => {
                self.lossy |= !STYLE_PPR_KEPT.contains(&name);
                if name == "w:jc" {
                    self.para.jc = attr(attrs, "w:val").map(Cow::into_owned);
                }
                apply_para_layout(&mut self.para, name, attrs);
            }
            _ if self.in_rpr => {
                self.lossy |= !STYLE_RPR_KEPT.contains(&name);
                apply_run_property(&mut self.fmt, name, attrs);
            }
            _ => {}
        }
    }

    pub(super) fn on_end(&mut self, name: &str) {
        match name {
            "w:pPr" => self.in_ppr = false,
            "w:rPr" => self.in_rpr = false,
            _ => {}
        }
    }

    /// The style, when its name and values are within bounds.
    pub(super) fn finish(self) -> Option<RichStyleDef> {
        let (p, f) = (self.para, self.fmt);
        let mut def = self.def;
        // White space around the name is not part of it.
        def.name = def.name.trim().to_string();
        RichStyleDef {
            font: f.font,
            size_half_pt: f.size_half_pt,
            bold: f.bold,
            italic: f.italic,
            underline: f.underline,
            color: f.color,
            alignment: p.jc.as_deref().and_then(RichAlignment::parse),
            left_indent_twips: p.left_indent,
            first_line_indent_twips: p.first_line,
            space_before_twips: p.space_before,
            space_after_twips: p.space_after,
            line_spacing: p.line_spacing,
            ..def
        }
        .checked()
    }
}

/// The notes of reading the styles: what the editor does not keep of a style of the user, and the styles past a bound.
pub(super) fn style_warnings(
    notes: StyleNotes,
    styles: &HashMap<String, DocxStyle>,
) -> Vec<String> {
    let mut out = Vec::new();
    if notes.lossy {
        out.push("사용자 스타일의 다음 스타일·연결 스타일·기준 스타일과 글꼴·단락 모양 밖의 속성은 가져오지 않았습니다.".to_string());
    }
    let found = styles.values().filter(|s| s.custom.is_some()).count();
    if notes.overflow || found > custom_styles(styles).len() {
        out.push(format!(
            "사용자 스타일은 이름이 겹치거나 기본 스타일 이름이거나 한도({})를 넘으면 가져오지 않았습니다.",
            crate::rich_word_blocks::MAX_CUSTOM_STYLES
        ));
    }
    out
}

/// The styles of the user in the order of the file: names that are not the word processor's own, one to a name, at most
/// [`MAX_CUSTOM_STYLES`](crate::rich_word_blocks::MAX_CUSTOM_STYLES).
pub(super) fn custom_styles(styles: &HashMap<String, DocxStyle>) -> Vec<RichStyleDef> {
    let mut found: Vec<&(usize, RichStyleDef)> =
        styles.values().filter_map(|s| s.custom.as_ref()).collect();
    found.sort_by_key(|(order, _)| *order);
    let mut out: Vec<RichStyleDef> = Vec::new();
    for (_, def) in found {
        let fresh = !crate::rich_word_blocks::is_reserved_style_name(&def.name)
            && !out.iter().any(|d| d.name == def.name);
        if fresh && out.len() < crate::rich_word_blocks::MAX_CUSTOM_STYLES {
            out.push(def.clone());
        }
    }
    out
}

/// Gives a paragraph the look of its style where the paragraph and its runs name none of their own (a file written by another
/// program keeps the look in the style).
pub(super) fn apply_style_def(p: &mut RichParagraph, def: &RichStyleDef) {
    p.alignment = p.alignment.or(def.alignment);
    p.left_indent_twips = p.left_indent_twips.or(def.left_indent_twips);
    p.first_line_indent_twips = p.first_line_indent_twips.or(def.first_line_indent_twips);
    p.space_before_twips = p.space_before_twips.or(def.space_before_twips);
    p.space_after_twips = p.space_after_twips.or(def.space_after_twips);
    p.line_spacing = p.line_spacing.or(def.line_spacing);
    for run in &mut p.runs {
        run.font = run.font.take().or_else(|| def.font.clone());
        run.size_half_pt = run.size_half_pt.or(def.size_half_pt);
        run.color = run.color.take().or_else(|| def.color.clone());
        run.bold |= def.bold;
        run.italic |= def.italic;
        run.underline |= def.underline;
    }
    p.word.style_name = Some(def.name.clone());
}
