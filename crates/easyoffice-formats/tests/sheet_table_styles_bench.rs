//! Open and save of a workbook whose 1,000 tables each have a custom style, against the same workbook with the style
//! definitions left out (PD-230). Run with
//! `cargo test --release -p easyoffice-formats --test sheet_table_styles_bench -- --ignored --nocapture`.

use easyoffice_formats::{
    CellBox, OoxmlCodec, SheetStyling, SheetTable, StyleElement, StyleFormat, TableStyleDef,
    ZipArchiveCodec, ZipSecurityPolicy,
};
use std::time::Instant;

const TABLES: u32 = 1_000;
const TABLE_ROWS: u32 = 20;
const TABLE_COLS: u32 = 4;
const EXTRA_DXFS: usize = 60_000;

fn def(n: u32) -> TableStyleDef {
    let fmt = |fill: u32| StyleFormat {
        fill: Some(fill),
        color: Some(0xFFFFFF),
        bold: true,
        stripe: 1,
        ..StyleFormat::default()
    };
    TableStyleDef {
        elements: [
            StyleElement::Header,
            StyleElement::RowStripe1,
            StyleElement::RowStripe2,
            StyleElement::FirstColumn,
            StyleElement::LastColumn,
            StyleElement::Total,
            StyleElement::Whole,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, e)| (e, fmt(0x100000 + n * 7 + i as u32)))
        .collect(),
    }
}

fn workbook(styled: bool) -> Vec<u8> {
    let mut cells = Vec::new();
    let mut tables = Vec::new();
    for t in 0..TABLES {
        let top = t * (TABLE_ROWS + 1);
        for col in 0..TABLE_COLS {
            cells.push((top, col, format!("h{col}")));
            for row in 1..TABLE_ROWS {
                cells.push((top + row, col, format!("{}", t + row + col)));
            }
        }
        let range = CellBox::parse(&format!("A{}:D{}", top + 1, top + TABLE_ROWS)).unwrap();
        let mut table = SheetTable::new(format!("Table_{t}"), range);
        table.columns = (0..TABLE_COLS).map(|c| format!("h{c}")).collect();
        table.style = Some(format!("Style{t}"));
        if styled {
            table.style_def = Some(def(t));
        }
        tables.push(table);
    }
    let styling = SheetStyling { tables, ..SheetStyling::default() };
    OoxmlCodec::pack_xlsx_multi_styled(&[("S".to_string(), cells)], &[], &[styling]).unwrap()
}

/// The same file with `EXTRA_DXFS` more empty `dxf` entries in front of the `dxfs` (a hostile stylesheet).
fn with_many_dxfs(bytes: &[u8]) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let parts: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let mut text = String::from_utf8_lossy(&e.data).into_owned();
            if e.path == "xl/styles.xml" {
                let padding = "<dxf><font><b/></font></dxf>".repeat(EXTRA_DXFS);
                text = text.replacen("<dxfs count=\"", &format!("<dxfs count=\"{EXTRA_DXFS}\" data-x=\""), 1).replacen("\">", &format!("\">{padding}"), 1);
            }
            (e.path, text.into_bytes())
        })
        .collect();
    ZipArchiveCodec::pack(&parts).unwrap()
}

fn best_ms(mut f: impl FnMut()) -> f64 {
    (0..5)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::MAX, f64::min)
}

#[test]
#[ignore = "benchmark"]
fn custom_table_styles_cost_little_to_open_and_save() {
    let plain = workbook(false);
    let styled = workbook(true);
    let hostile = with_many_dxfs(&styled);
    let open = |bytes: &[u8]| {
        OoxmlCodec::unpack_xlsx_with_styling(bytes).unwrap();
    };
    println!("bytes: no styles {}, 1,000 styles {}, with {} more dxfs {}", plain.len(), styled.len(), EXTRA_DXFS, hostile.len());
    println!("open ms (min of 5): no styles {:.1}, 1,000 styles {:.1}, hostile {:.1}", best_ms(|| open(&plain)), best_ms(|| open(&styled)), best_ms(|| open(&hostile)));
    let (cells, styling) = OoxmlCodec::unpack_xlsx_with_styling(&styled).unwrap();
    let styling: Vec<SheetStyling> = styling.into_iter().map(|(_, s)| s).collect();
    let cells = vec![("S".to_string(), cells.into_iter().next().map(|(_, c)| c).unwrap())];
    println!("save ms (min of 5): 1,000 styles {:.1}", best_ms(|| {
        OoxmlCodec::pack_xlsx_multi_styled(&cells, &[], &styling).unwrap();
    }));
}
