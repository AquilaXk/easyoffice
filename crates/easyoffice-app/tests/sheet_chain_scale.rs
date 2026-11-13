//! Saving a workbook whose sheets each read the sheet before it (PD-212): the formula values are
//! the same in every format and the time grows in step with the number of formulas, so a few
//! hundred sheets of a few hundred rows do not take minutes.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{LegacyOle2Codec, ZipArchiveCodec, ZipSecurityPolicy};
use std::time::Instant;

const ROWS: usize = 40;
/// Twice the sheets: linear work is about 2x, the old per-sheet re-evaluation 6x or more.
const MAX_RATIO: f64 = 3.5;

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

/// `S0!A{r} = r`, `S{k}!A{r} = S{k-1}!A{r} + 1`.
fn chain_payload(dst: &str, sheets: usize, rows: usize) -> String {
    let sheets_json: Vec<String> = (0..sheets)
        .map(|k| {
            let cells: Vec<String> = (1..=rows)
                .map(|r| {
                    let raw = if k == 0 {
                        r.to_string()
                    } else {
                        format!("=S{}!A{r}+1", k - 1)
                    };
                    format!(r#"{{"ref":"A{r}","raw":"{raw}"}}"#)
                })
                .collect();
            format!(r#"{{"name":"S{k}","cells":[{}]}}"#, cells.join(","))
        })
        .collect();
    format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{}]}}"#,
        sheets_json.join(",")
    )
}

fn save(dst: &str, sheets: usize, rows: usize) -> Vec<u8> {
    let json =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &chain_payload(dst, sheets, rows))
            .unwrap();
    EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap()
}

/// The cached `<v>` values of one sheet part, in cell order.
fn cached_values(xlsx: &[u8], sheet: usize) -> Vec<String> {
    let entries = ZipArchiveCodec::unpack(xlsx, &ZipSecurityPolicy::default()).unwrap();
    let path = format!("xl/worksheets/sheet{}.xml", sheet + 1);
    let entry = entries.iter().find(|e| e.path == path).expect(&path);
    let xml = String::from_utf8(entry.data.clone()).unwrap();
    xml.split("<v>")
        .skip(1)
        .map(|rest| rest[..rest.find("</v>").unwrap()].to_string())
        .collect()
}

#[test]
fn every_sheet_of_a_chain_keeps_the_value_its_depth_adds_in_xlsx_and_xls() {
    let sheets = 24;
    let xlsx = save("xlsx", sheets, ROWS);
    let xls = LegacyOle2Codec::unpack_xls_multi(&save("xls", sheets, ROWS)).unwrap();
    for (k, xls_sheet) in xls.iter().enumerate() {
        let want: Vec<String> = (1..=ROWS).map(|r| (r + k).to_string()).collect();
        assert_eq!(cached_values(&xlsx, k), want, "xlsx S{k}");
        let mut cells: Vec<_> = xls_sheet.cells.iter().collect();
        cells.sort();
        let got: Vec<&str> = cells.iter().map(|(_, _, v)| v.as_str()).collect();
        assert_eq!(got, want, "xls S{k}");
    }
}

fn best_of(runs: usize, mut work: impl FnMut()) -> f64 {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            work();
            started.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn saving_a_chain_of_sheets_scales_with_the_formula_count() {
    let payload = |sheets| chain_payload("xlsx", sheets, ROWS);
    let (small, large) = (payload(50), payload(100));
    let run = |body: &str| {
        EasyOfficeIpcRouter::dispatch("convert_document_format", body).unwrap();
    };
    let t_small = best_of(3, || run(&small));
    let t_large = best_of(3, || run(&large));
    assert!(
        t_large < t_small * MAX_RATIO,
        "2x the sheets took {:.1}x the time ({:.0} ms -> {:.0} ms)",
        t_large / t_small,
        t_small * 1e3,
        t_large * 1e3
    );
}
