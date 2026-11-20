//! What a sheet save leaves out of the sheet's objects (PD-263): charts, pictures, notes, pivot
//! tables and slicers have a place in XLSX only, and XLSX rebuilds a pivot cache from the source
//! columns, so the cache fields with no column (calculated, date-grouped) are not written.

use easyoffice_formats::SheetStyling;

/// Adds one line per kind of sheet object a save to `label` does not write, with the count over all sheets.
pub(crate) fn warn_unsaved_sheet_objects(
    stylings: &[SheetStyling],
    label: &str,
    warnings: &mut Vec<String>,
) {
    let count = |of: fn(&SheetStyling) -> usize| stylings.iter().map(of).sum::<usize>();
    for (name, n) in [
        ("차트", count(|st| st.charts.len())),
        ("그림", count(|st| st.images.len())),
        ("메모", count(|st| st.notes.len())),
        ("피벗 테이블", count(|st| st.pivots.len())),
        (
            "피벗 슬라이서",
            count(|st| st.pivots.iter().map(|p| p.slicers.len()).sum()),
        ),
    ] {
        if n > 0 {
            warnings.push(format!("{name} {n}개는 {label}에 저장하지 않았습니다."));
        }
    }
}

/// Adds the pivot cache fields an XLSX save drops: the cache is rebuilt from the source columns, so the
/// fields past them (calculated fields, date groups) are not written.
pub(crate) fn warn_unsaved_pivot_fields(stylings: &[SheetStyling], warnings: &mut Vec<String>) {
    let dropped: usize = stylings
        .iter()
        .flat_map(|st| &st.pivots)
        .map(|p| {
            p.field_names
                .len()
                .saturating_sub(p.known_source_width() as usize)
        })
        .sum();
    if dropped > 0 {
        warnings.push(format!(
            "피벗 계산 필드·날짜 묶음 필드 {dropped}개는 XLSX에 저장하지 않았습니다."
        ));
    }
}
