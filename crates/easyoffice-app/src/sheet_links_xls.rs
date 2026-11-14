//! The hyperlinks of a workbook saved as XLS (PD-223): the page's links go in as the file's `HLink` records,
//! with the two changes the format makes. A link outside the 65,536 x 256 grid or too long for a record is left out and counted, and a
//! jump to a sheet whose name the format cannot keep follows the sheet to its new name.

use easyoffice_formats::{SheetLinks, SheetStyling, MAX_BIFF8_COL, MAX_BIFF8_ROW};
use easyoffice_sheet::{rewrite_sheet_names, SheetChange};

/// The links of each sheet for the XLS writer; `old` and `new` are the sheet names before and after the
/// format's name rules. `skipped` counts the links left out.
pub(crate) fn xls_links(
    stylings: &[SheetStyling],
    old: &[&str],
    new: &[String],
    skipped: &mut usize,
) -> Vec<SheetLinks> {
    let renames: Vec<(&str, &str)> = old
        .iter()
        .zip(new)
        .filter(|(old, new)| **old != new.as_str())
        .map(|(old, new)| (*old, new.as_str()))
        .collect();
    stylings
        .iter()
        .take(old.len())
        .map(|styling| {
            let mut kept = SheetLinks::new();
            for (&(row, col), link) in &styling.links {
                if row > MAX_BIFF8_ROW || col > MAX_BIFF8_COL || !link.fits_xls() {
                    *skipped += 1;
                    continue;
                }
                let mut link = link.clone();
                if let Some(location) = link.target.strip_prefix('#') {
                    // A location names one sheet: the first rename that applies is the only one.
                    let moved = renames.iter().find_map(|&(from, to)| {
                        rewrite_sheet_names(location, SheetChange::Rename { from, to })
                    });
                    if let Some(moved) = moved {
                        link.target = format!("#{moved}");
                    }
                }
                kept.insert((row, col), link);
            }
            kept
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::CellLink;

    #[test]
    fn a_jump_follows_a_renamed_sheet_and_cells_off_the_grid_are_counted() {
        let mut styling = SheetStyling::default();
        for (at, target) in [
            ((0, 0), "#'긴:시트'!A1"),
            ((1, 0), "#Other!B2"),
            ((2, 0), "https://a.b/#긴:시트"),
            ((65_536, 0), "https://a.b/"),
            ((0, 256), "https://a.b/"),
        ] {
            styling.links.insert(at, CellLink::new(target, "").unwrap());
        }
        let mut skipped = 0;
        let out = xls_links(
            &[styling],
            &["긴:시트", "Other"],
            &["긴_시트".to_string(), "Other".to_string()],
            &mut skipped,
        );
        assert_eq!(skipped, 2);
        assert_eq!(out[0].len(), 3);
        assert_eq!(out[0][&(0, 0)].target, "#'긴_시트'!A1");
        assert_eq!(out[0][&(1, 0)].target, "#Other!B2");
        assert_eq!(out[0][&(2, 0)].target, "https://a.b/#긴:시트");
    }
}
