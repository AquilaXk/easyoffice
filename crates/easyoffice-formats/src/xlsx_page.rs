//! The page setup of a worksheet in XLSX (PD-213): `sheetPr/pageSetUpPr@fitToPage`, `printOptions`,
//! `pageMargins` and `pageSetup` in the worksheet part, and the built-in defined names `_xlnm.Print_Area`
//! and `_xlnm.Print_Titles` in `xl/workbook.xml` (ECMA-376 18.3.1.62 to 18.3.1.70, 18.2.5). The model and its
//! bounds are [`crate::sheet_page`]; a value out of its bounds is left out when a file is read.

use std::fmt::Write;

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_balanced_elements, write_col_letters,
};
use crate::sheet_page::{inches_to_mils, mils_text, PageMargins, PageScale, SheetPage};
use crate::sheet_page::{
    MAX_FIT_PAGES, MAX_MARGIN_MILS, MAX_PAPER_CODE, MAX_TITLE_COLS, MAX_TITLE_ROWS,
};
use crate::xml_stream::{Attrs, XmlEvent};
use crate::{CellBox, OoxmlCodec, SheetStyling, XmlSanitizer};

const PRINT_AREA: &str = "_xlnm.Print_Area";
const PRINT_TITLES: &str = "_xlnm.Print_Titles";
const LAST_ROW: u32 = OoxmlCodec::MAX_XLSX_ROWS - 1;
const LAST_COL: u32 = OoxmlCodec::MAX_XLSX_COLS - 1;

/// `<pageSetUpPr fitToPage="1"/>` for the end of `sheetPr` when the sheet is fitted to pages.
pub(crate) fn fit_to_page_xml(page: &SheetPage) -> &'static str {
    match page.scale {
        PageScale::Fit { .. } => "<pageSetUpPr fitToPage=\"1\"/>",
        PageScale::Percent(_) => "",
    }
}

/// `printOptions`, `pageMargins` and `pageSetup` in worksheet order, empty for a default setup.
pub(crate) fn page_tail_xml(page: &SheetPage) -> String {
    let mut xml = String::new();
    let flags = [
        ("horizontalCentered", page.center_horizontal),
        ("verticalCentered", page.center_vertical),
        ("headings", page.headings),
        ("gridLines", page.grid_lines),
    ];
    if flags.iter().any(|(_, on)| *on) {
        xml.push_str("<printOptions");
        for (name, _) in flags.iter().filter(|(_, on)| *on) {
            let _ = write!(xml, " {name}=\"1\"");
        }
        xml.push_str("/>");
    }
    if let Some(m) = page.margins {
        let _ = write!(
            xml,
            "<pageMargins left=\"{}\" right=\"{}\" top=\"{}\" bottom=\"{}\" header=\"{}\" footer=\"{}\"/>",
            mils_text(m.left),
            mils_text(m.right),
            mils_text(m.top),
            mils_text(m.bottom),
            mils_text(m.header),
            mils_text(m.footer)
        );
    }
    let mut setup = String::new();
    if let Some(code) = page.paper {
        let _ = write!(setup, " paperSize=\"{code}\"");
    }
    match page.scale {
        PageScale::Percent(100) => {}
        PageScale::Percent(p) => {
            let _ = write!(setup, " scale=\"{p}\"");
        }
        PageScale::Fit { width, height } => {
            let _ = write!(setup, " fitToWidth=\"{width}\" fitToHeight=\"{height}\"");
        }
    }
    if page.landscape {
        setup.push_str(" orientation=\"landscape\"");
    }
    if !setup.is_empty() {
        let _ = write!(xml, "<pageSetup{setup}/>");
    }
    xml
}

fn flag(attrs: &Attrs<'_>, name: &str) -> bool {
    matches!(attrs.get(name).as_deref(), Some("1" | "true"))
}

fn number(attrs: &Attrs<'_>, name: &str) -> Option<u16> {
    attrs.get(name).as_deref()?.trim().parse().ok()
}

/// The page setup elements of one worksheet as the scan meets them.
#[derive(Default)]
pub(crate) struct PageScan {
    fit_to_page: bool,
    page: SheetPage,
    scale: Option<u16>,
    fit: (Option<u16>, Option<u16>),
}

impl PageScan {
    pub(crate) fn feed(&mut self, event: &XmlEvent<'_>) {
        let XmlEvent::Start { name, attrs } = event else {
            return;
        };
        match *name {
            "pageSetUpPr" => self.fit_to_page = flag(attrs, "fitToPage"),
            "printOptions" => {
                self.page.center_horizontal = flag(attrs, "horizontalCentered");
                self.page.center_vertical = flag(attrs, "verticalCentered");
                self.page.headings = flag(attrs, "headings");
                self.page.grid_lines = flag(attrs, "gridLines");
            }
            "pageMargins" => self.page.margins = margins(attrs),
            "pageSetup" => {
                self.page.paper =
                    number(attrs, "paperSize").filter(|c| (1..=MAX_PAPER_CODE).contains(c));
                self.page.landscape = attrs.get("orientation").as_deref() == Some("landscape");
                self.scale = number(attrs, "scale");
                self.fit = (number(attrs, "fitToWidth"), number(attrs, "fitToHeight"));
            }
            _ => {}
        }
    }

    /// The setup read, each part within its bounds (a scale or a page count out of range is the default).
    pub(crate) fn finish(mut self) -> SheetPage {
        let (width, height) = (self.fit.0.unwrap_or(1), self.fit.1.unwrap_or(1));
        self.page.scale = if self.fit_to_page && width <= MAX_FIT_PAGES && height <= MAX_FIT_PAGES {
            PageScale::Fit { width, height }
        } else {
            match self.scale.unwrap_or(100) {
                p @ crate::sheet_page::MIN_PAGE_SCALE..=crate::sheet_page::MAX_PAGE_SCALE => {
                    PageScale::Percent(p)
                }
                _ => PageScale::Percent(100),
            }
        };
        self.page
    }
}

fn margins(attrs: &Attrs<'_>) -> Option<PageMargins> {
    let side = |name: &str| attrs.get(name).as_deref().and_then(inches_to_mils);
    Some(PageMargins {
        left: side("left")?,
        right: side("right")?,
        top: side("top")?,
        bottom: side("bottom")?,
        header: side("header")?,
        footer: side("footer")?,
    })
    .filter(|m| {
        m.left
            .max(m.right)
            .max(m.top)
            .max(m.bottom)
            .max(m.header)
            .max(m.footer)
            <= MAX_MARGIN_MILS
    })
}

// ---- the defined names of the workbook ------------------------------------------------------

/// A sheet name as a formula writes it before `!`: quoted, with `'` doubled, unless it is plain.
fn sheet_ref(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if plain {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

fn col_letters(col: u32) -> String {
    let mut buf = [0u8; 4];
    write_col_letters(&mut buf, col as usize).to_string()
}

fn absolute(b: &CellBox) -> String {
    let corner = |r: u32, c: u32| format!("${}${}", col_letters(c), r + 1);
    if (b.row0, b.col0) == (b.row1, b.col1) {
        corner(b.row0, b.col0)
    } else {
        format!("{}:{}", corner(b.row0, b.col0), corner(b.row1, b.col1))
    }
}

/// `<definedNames>` for the print areas and repeated rows and columns of the sheets, empty without any.
pub(crate) fn print_names_xml(sheet_names: &[String], stylings: &[SheetStyling]) -> String {
    let mut names = String::new();
    for (index, styling) in stylings.iter().enumerate().take(sheet_names.len()) {
        let page = &styling.page;
        let sheet = sheet_ref(&sheet_names[index]);
        if !page.print_area.is_empty() {
            let refs: Vec<String> = page
                .print_area
                .iter()
                .map(|b| format!("{sheet}!{}", absolute(b)))
                .collect();
            push_name(&mut names, PRINT_AREA, index, &refs.join(","));
        }
        let mut titles = Vec::new();
        if let Some((a, b)) = page.title_cols {
            titles.push(format!("{sheet}!${}:${}", col_letters(a), col_letters(b)));
        }
        if let Some((a, b)) = page.title_rows {
            titles.push(format!("{sheet}!${}:${}", a + 1, b + 1));
        }
        if !titles.is_empty() {
            push_name(&mut names, PRINT_TITLES, index, &titles.join(","));
        }
    }
    if names.is_empty() {
        names
    } else {
        format!("<definedNames>{names}</definedNames>")
    }
}

fn push_name(out: &mut String, name: &str, local_sheet: usize, formula: &str) {
    let _ = write!(
        out,
        "<definedName name=\"{name}\" localSheetId=\"{local_sheet}\">{}</definedName>",
        XmlSanitizer::escape(formula)
    );
}

/// The segments of a defined name's formula, split at commas outside quotes.
fn segments(formula: &str) -> Vec<&str> {
    let (mut out, mut start, mut quoted) = (Vec::new(), 0, false);
    for (i, c) in formula.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ',' if !quoted => {
                out.push(&formula[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&formula[start..]);
    out
}

/// A segment `Sheet!$A$1:$B$2` as its sheet (unquoted) and the range text.
fn split_segment(segment: &str) -> Option<(String, &str)> {
    let segment = segment.trim();
    let at = if let Some(rest) = segment.strip_prefix('\'') {
        let mut i = 0;
        let bytes = rest.as_bytes();
        loop {
            let q = rest[i..].find('\'')? + i;
            if bytes.get(q + 1) == Some(&b'\'') {
                i = q + 2;
            } else {
                break q + 2;
            }
        }
    } else {
        segment.find('!')?
    };
    let (sheet, range) = segment.split_at(at);
    let range = range.strip_prefix('!')?;
    let sheet = sheet
        .strip_prefix('\'')
        .map_or(sheet, |s| s.strip_suffix('\'').unwrap_or(s));
    Some((sheet.replace("''", "'"), range))
}

/// Reads the print areas and repeated rows and columns from `<definedNames>` of the workbook part into the
/// stylings of their sheets (`localSheetId` is the sheet's position). Anything out of the bounds of the model
/// (too many ranges or repeated lines, a range past the sheet) is left out.
pub(crate) fn apply_print_names(
    workbook_xml: &str,
    sheet_names: &[String],
    stylings: &mut [(String, SheetStyling)],
) {
    for (attrs, inner) in extract_balanced_elements(workbook_xml, "definedName") {
        let Some(name) = extract_attr_from_fragment(attrs, "name") else {
            continue;
        };
        if name != PRINT_AREA && name != PRINT_TITLES {
            continue;
        }
        let Some(local) =
            extract_attr_from_fragment(attrs, "localSheetId").and_then(|v| v.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(owner) = sheet_names.get(local) else {
            continue;
        };
        let Some((_, styling)) = stylings.iter_mut().find(|(n, _)| n == owner) else {
            continue;
        };
        let formula = XmlSanitizer::unescape(inner);
        let boxes: Vec<CellBox> = segments(&formula)
            .into_iter()
            .filter_map(|s| split_segment(s))
            .filter(|(sheet, _)| sheet.eq_ignore_ascii_case(owner))
            .filter_map(|(_, range)| CellBox::parse(range))
            .collect();
        let page = &mut styling.page;
        if name == PRINT_AREA {
            if boxes.len() <= crate::sheet_page::MAX_PRINT_RANGES {
                page.print_area = boxes;
            }
            continue;
        }
        for b in boxes {
            let all_cols = (b.col0, b.col1) == (0, LAST_COL);
            let all_rows = (b.row0, b.row1) == (0, LAST_ROW);
            if all_cols && !all_rows && b.row1 - b.row0 < MAX_TITLE_ROWS {
                page.title_rows = Some((b.row0, b.row1));
            } else if all_rows && !all_cols && b.col1 - b.col0 < MAX_TITLE_COLS {
                page.title_cols = Some((b.col0, b.col1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_names_are_quoted_and_split_back() {
        assert_eq!(sheet_ref("Sheet1"), "Sheet1");
        assert_eq!(sheet_ref("내 시트"), "'내 시트'");
        assert_eq!(sheet_ref("It's"), "'It''s'");
        assert_eq!(sheet_ref("2024"), "'2024'");
        assert_eq!(
            split_segment("Sheet1!$A$1:$B$2"),
            Some(("Sheet1".to_string(), "$A$1:$B$2"))
        );
        assert_eq!(
            split_segment("'It''s a, sheet'!$1:$2"),
            Some(("It's a, sheet".to_string(), "$1:$2"))
        );
        assert_eq!(
            segments("'a,b'!$A$1,Sheet1!$C$1"),
            vec!["'a,b'!$A$1", "Sheet1!$C$1"]
        );
        assert_eq!(split_segment("no sheet"), None);
    }

    #[test]
    fn absolute_ranges_and_cells() {
        assert_eq!(
            absolute(&CellBox {
                row0: 0,
                col0: 0,
                row1: 9,
                col1: 2
            }),
            "$A$1:$C$10"
        );
        assert_eq!(absolute(&CellBox::cell(4, 27)), "$AB$5");
    }
}
