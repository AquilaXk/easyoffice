//! Open and save of a workbook with many styled cells, with and without colours (PD-227):
//! `cargo test --release -p easyoffice-formats --test sheet_color_bench -- --ignored --nocapture`.

use easyoffice_formats::{
    read_sheet_styling, CellStyle, DocumentFormat, OdfCodec, OdsUnpacked, OoxmlCodec, SheetStyling,
};
use std::time::Instant;

const ROWS: u32 = 5_000;
const COLS: u32 = 20;

fn workbook(colors: bool) -> (Vec<OdsUnpacked>, Vec<SheetStyling>) {
    let palette = [
        0xFFEB9C, 0xFFC7CE, 0xC6EFCE, 0xDDEBF7, 0xFCE4D6, 0xE2EFDA, 0xD9D9D9, 0xFFF2CC,
    ];
    let mut cells = Vec::new();
    let mut styling = SheetStyling::default();
    for r in 0..ROWS {
        for c in 0..COLS {
            cells.push((r, c, (r * c).to_string()));
            let mut style = CellStyle {
                bold: (r + c) % 5 == 0,
                size_pt: Some(9.0 + f64::from((r + c) % 4)),
                ..Default::default()
            };
            if colors {
                style.fill =
                    Some(palette[((r + c) % 8) as usize] ^ (((r * 7 + c) % 200) << 16 & 0x0F0000));
                style.font_color = (c % 3 == 0).then_some(0x9C0006);
            }
            styling.cells.insert((r, c), style);
        }
    }
    (vec![("Sheet1".to_string(), cells)], vec![styling])
}

fn best<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let mut out = None;
    let mut best = f64::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        let v = f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        out = Some(v);
    }
    (best, out.unwrap())
}

#[test]
#[ignore = "measurement"]
fn open_and_save_with_and_without_colours() {
    for colors in [false, true] {
        let (sheets, stylings) = workbook(colors);
        let (save, bytes) =
            best(|| OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &stylings).unwrap());
        let (open, back) = best(|| read_sheet_styling(&bytes, DocumentFormat::Xlsx).unwrap());
        let (save_ods, ods) = best(|| OdfCodec::pack_ods_multi_styled(&sheets, &stylings).unwrap());
        let (open_ods, _) = best(|| read_sheet_styling(&ods, DocumentFormat::Ods).unwrap());
        println!(
            "colours={colors}: xlsx save {save:.1} ms, open {open:.1} ms ({} bytes, {} styled cells); ods save {save_ods:.1} ms, open {open_ods:.1} ms ({} bytes)",
            bytes.len(),
            back[0].1.cells.len(),
            ods.len()
        );
    }
}
