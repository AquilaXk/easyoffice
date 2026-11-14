//! Open and save of a workbook with many tables, against the same cells without tables (PD-224). Run with
//! `cargo test --release -p easyoffice-formats --test sheet_tables_bench -- --ignored --nocapture`.

use easyoffice_formats::{CellBox, OoxmlCodec, SheetStyling, SheetTable};
use std::time::Instant;

const DATA_ROWS: u32 = 20_000;
const DATA_COLS: u32 = 10;
const TABLES: u32 = 1_000;
const TABLE_ROWS: u32 = 20;
const TABLE_COLS: u32 = 4;

fn data() -> Vec<(u32, u32, String)> {
    let mut cells = Vec::new();
    for row in 0..DATA_ROWS {
        for col in 0..DATA_COLS {
            cells.push((row, col, format!("{}", row * 7 + col)));
        }
    }
    cells
}

fn table_cells() -> Vec<(u32, u32, String)> {
    let mut cells = Vec::new();
    for t in 0..TABLES {
        let top = t * (TABLE_ROWS + 1);
        for col in 0..TABLE_COLS {
            cells.push((top, col, format!("h{col}")));
            for row in 1..TABLE_ROWS {
                cells.push((top + row, col, format!("{}", t + row + col)));
            }
        }
    }
    cells
}

fn tables() -> Vec<SheetTable> {
    (0..TABLES)
        .map(|t| {
            let top = t * (TABLE_ROWS + 1);
            let mut table = SheetTable::new(
                format!("Table{t}x"),
                CellBox {
                    row0: top,
                    col0: 0,
                    row1: top + TABLE_ROWS - 1,
                    col1: TABLE_COLS - 1,
                },
            );
            table.columns = (0..TABLE_COLS).map(|c| format!("h{c}")).collect();
            table.style = Some("TableStyleMedium2".into());
            table
        })
        .collect()
}

fn best<T>(mut run: impl FnMut() -> T) -> f64 {
    (0..5)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(run());
            start.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::MAX, f64::min)
}

#[test]
#[ignore = "benchmark: run in release"]
fn open_and_save_with_and_without_a_thousand_tables() {
    let sheets = vec![
        ("Data".to_string(), data()),
        ("Tables".to_string(), table_cells()),
    ];
    let plain = [SheetStyling::default(), SheetStyling::default()];
    let tabled = [
        SheetStyling::default(),
        SheetStyling {
            tables: tables(),
            ..SheetStyling::default()
        },
    ];
    let bytes_plain = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &plain).unwrap();
    let bytes_tabled = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &tabled).unwrap();
    println!(
        "package bytes: plain {} tabled {}",
        bytes_plain.len(),
        bytes_tabled.len()
    );
    let save_plain = best(|| OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &plain).unwrap());
    let save_tabled = best(|| OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &tabled).unwrap());
    let open_plain = best(|| OoxmlCodec::unpack_xlsx_with_styling(&bytes_plain).unwrap());
    let open_tabled = best(|| OoxmlCodec::unpack_xlsx_with_styling(&bytes_tabled).unwrap());
    println!("save ms: no tables {save_plain:.1}  1000 tables {save_tabled:.1}");
    println!("open ms: no tables {open_plain:.1}  1000 tables {open_tabled:.1}");
    let back = OoxmlCodec::unpack_xlsx_with_styling(&bytes_tabled)
        .unwrap()
        .1;
    assert_eq!(back[1].1.tables.len(), TABLES as usize);
}
