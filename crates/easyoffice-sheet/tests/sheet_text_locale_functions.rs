//! `BAHTTEXT`, `EUROCONVERT`, `JIS` and `PHONETIC` (PD-271).
//!
//! 참조 구현: the unit tests of the reference engine (Univer `functions/text/bahttext/__tests__/index.spec.ts` and
//! `euroconvert/__tests__/index.spec.ts`, commit 4767b3b309467c283bbd52b725536ca73861e0fc,
//! Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.). The other values are the documented
//! examples of the functions. `DIFFERS` marks a case where the
//! documented behaviour is followed instead of the reference.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

fn eval(formula: &str) -> String {
    let mut model = new_empty_model();
    model._set("A1", formula);
    model.evaluate();
    model._get_text("A1")
}

#[test]
fn bahttext_reads_baht_and_satang() {
    assert_eq!(eval("=BAHTTEXT(123.45)"), "หนึ่งร้อยยี่สิบสามบาทสี่สิบห้าสตางค์");
    assert_eq!(eval("=BAHTTEXT(-2.34)"), "ลบสองบาทสามสิบสี่สตางค์");
    assert_eq!(eval("=BAHTTEXT(1)"), "หนึ่งบาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(1.23)"), "หนึ่งบาทยี่สิบสามสตางค์");
    assert_eq!(eval("=BAHTTEXT(0)"), "ศูนย์บาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(0.123)"), "สิบสองสตางค์");
    assert_eq!(eval("=BAHTTEXT(-3)"), "ลบสามบาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(\"100\")"), "หนึ่งร้อยบาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(TRUE)"), "หนึ่งบาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(B1)"), "ศูนย์บาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(\"abc\")"), "#VALUE!");
    assert_eq!(eval("=BAHTTEXT(#NAME?)"), "#NAME?");
}

#[test]
fn bahttext_reads_the_documented_example_and_large_numbers() {
    // The function's documentation: BAHTTEXT(1234).
    assert_eq!(eval("=BAHTTEXT(1234)"), "หนึ่งพันสองร้อยสามสิบสี่บาทถ้วน");
    assert_eq!(
        eval("=BAHTTEXT(1234567890)"),
        "หนึ่งพันสองร้อยสามสิบสี่ล้านห้าแสนหกหมื่นเจ็ดพันแปดร้อยเก้าสิบบาทถ้วน"
    );
    assert_eq!(
        eval("=BAHTTEXT(1111111)"),
        "หนึ่งล้านหนึ่งแสนหนึ่งหมื่นหนึ่งพันหนึ่งร้อยสิบเอ็ดบาทถ้วน"
    );
    assert_eq!(eval("=BAHTTEXT(10000000)"), "สิบล้านบาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(11)"), "สิบเอ็ดบาทถ้วน");
    assert_eq!(eval("=BAHTTEXT(21)"), "ยี่สิบเอ็ดบาทถ้วน");
}

#[test]
fn bahttext_differs_from_the_reference_on_binary_fractions_and_the_satang_carry() {
    // DIFFERS: the reference multiplies 0.57 by 100 in binary (56.99999999999999) and reads the dot.
    assert_eq!(eval("=BAHTTEXT(0.57)"), "ห้าสิบเจ็ดสตางค์");
    assert_eq!(eval("=BAHTTEXT(2.29)"), "สองบาทยี่สิบเก้าสตางค์");
    // DIFFERS: the reference reads 1.995 as 100 satang. Two decimals round to 2.00.
    assert_eq!(eval("=BAHTTEXT(1.999)"), "สองบาทถ้วน");
    // A number with no exact whole digits left is refused (the reference prints exponent text).
    assert_eq!(eval("=BAHTTEXT(1E+15)"), "#VALUE!");
    assert_eq!(eval("=BAHTTEXT(-1E+300)"), "#VALUE!");
}

#[test]
fn bahttext_works_per_element_of_an_array() {
    let mut model = new_empty_model();
    model._set("A1", "=BAHTTEXT({1,0;11,\"x\"})");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "หนึ่งบาทถ้วน");
    assert_eq!(model._get_text("B1"), "ศูนย์บาทถ้วน");
    assert_eq!(model._get_text("A2"), "สิบเอ็ดบาทถ้วน");
    assert_eq!(model._get_text("B2"), "#VALUE!");
}

#[test]
fn euroconvert_returns_the_documented_examples() {
    // The function's documentation: 1.2 DEM to EUR, 1 FRF to EUR with 3 decimals, to DEM at full precision.
    assert_eq!(eval("=EUROCONVERT(1.2,\"DEM\",\"EUR\")"), "0.61");
    assert_eq!(eval("=EUROCONVERT(1,\"FRF\",\"EUR\",TRUE,3)"), "0.152");
    assert_eq!(eval("=EUROCONVERT(1,\"FRF\",\"EUR\",FALSE,3)"), "0.15");
    assert_eq!(eval("=EUROCONVERT(1,\"FRF\",\"DEM\",TRUE,3)"), "0.29728616");
}

#[test]
fn euroconvert_converts_with_the_fixed_rates_and_rounds_to_the_calculation_precision() {
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"DEM\",TRUE)"), "1.95583");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"DEM\")"), "1.96");
    assert_eq!(eval("=EUROCONVERT(100,\"EUR\",\"ITL\")"), "193627");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"IEP\",TRUE)"), "0.787564");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"BEF\")"), "40");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"LUF\")"), "40");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"ESP\")"), "166");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"PTE\")"), "200");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"GRD\")"), "341");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"NLG\")"), "2.2");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"ATS\")"), "13.76");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"FIM\")"), "5.95");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"SIT\")"), "239.64");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"GRD\",TRUE)"), "340.75");
    assert_eq!(eval("=EUROCONVERT(-1.2,\"DEM\",\"EUR\")"), "-0.61");
    assert_eq!(eval("=EUROCONVERT(-100,\"EUR\",\"ESP\")"), "-16639");
}

#[test]
fn euroconvert_triangulates_through_the_euro() {
    assert_eq!(eval("=EUROCONVERT(100,\"DEM\",\"FRF\")"), "335.39");
    assert_eq!(eval("=EUROCONVERT(100,\"DEM\",\"FRF\",FALSE,3)"), "335.38");
    assert_eq!(eval("=EUROCONVERT(1,\"FRF\",\"DEM\",FALSE,3)"), "0.3");
    assert_eq!(eval("=EUROCONVERT(100,\"ATS\",\"PTE\")"), "1457");
    // the intermediate 51.12918812 euros become 51.129 before the second conversion
    assert_eq!(
        eval("=EUROCONVERT(100,\"DEM\",\"FRF\",TRUE,3)"),
        "335.38425453"
    );
}

#[test]
fn euroconvert_checks_its_arguments_in_the_references_order() {
    // the same currency on both sides returns the number as it is
    assert_eq!(eval("=EUROCONVERT(1.23456,\"DEM\",\"DEM\")"), "1.23456");
    assert_eq!(
        eval("=EUROCONVERT(1.23456,\"EUR\",\"EUR\",FALSE,3)"),
        "1.23456"
    );
    // an invalid precision is still an error when no triangulation takes place
    assert_eq!(eval("=EUROCONVERT(1,\"DEM\",\"DEM\",TRUE,2)"), "#VALUE!");
    assert_eq!(eval("=EUROCONVERT(1.2,\"dem\",\"Eur\")"), "0.61");
    assert_eq!(eval("=EUROCONVERT(\"1.2\",\"DEM\",\"EUR\")"), "0.61");
    assert_eq!(eval("=EUROCONVERT(\"test\",\"DEM\",\"EUR\")"), "#VALUE!");
    assert_eq!(eval("=EUROCONVERT(#NAME?,\"DEM\",\"EUR\")"), "#NAME?");
    assert_eq!(eval("=EUROCONVERT(1,#NAME?,\"EUR\")"), "#NAME?");
    for formula in [
        "=EUROCONVERT(1,\"USD\",\"EUR\")",
        "=EUROCONVERT(1,\"EUR\",\"GBP\")",
        "=EUROCONVERT(1,\"EURO\",\"DEM\")",
        "=EUROCONVERT(1,\"\",\"DEM\")",
        "=EUROCONVERT(1,1,\"DEM\")",
        "=EUROCONVERT(1,\"DEM\",TRUE)",
        "=EUROCONVERT(1,B1,\"DEM\")",
        "=EUROCONVERT(1,\"constructor\",\"toString\")",
        "=EUROCONVERT(1,\"EUR\",\"DEM\",\"test\")",
        "=EUROCONVERT(1,\"DEM\")",
        "=EUROCONVERT(1,\"DEM\",\"EUR\",TRUE,3,1)",
    ] {
        assert_eq!(eval(formula), "#VALUE!", "{formula}");
    }
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"DEM\",1)"), "1.95583");
    assert_eq!(eval("=EUROCONVERT(1,\"EUR\",\"DEM\",0)"), "1.96");
}

#[test]
fn euroconvert_reads_the_triangulation_precision_as_a_vba_integer() {
    for formula in [
        "=EUROCONVERT(1,\"FRF\",\"DEM\",TRUE,2)",
        "=EUROCONVERT(1,\"FRF\",\"DEM\",TRUE,0)",
        "=EUROCONVERT(1,\"FRF\",\"DEM\",TRUE,-3)",
        "=EUROCONVERT(1,\"FRF\",\"DEM\",TRUE,\"test\")",
        "=EUROCONVERT(1,\"EUR\",\"DEM\",TRUE,2)",
        "=EUROCONVERT(1,\"FRF\",\"EUR\",TRUE,2.5)",
    ] {
        assert_eq!(eval(formula), "#VALUE!", "{formula}");
    }
    let cases = [
        ("3.1", "0.152"),
        ("3.4", "0.152"),
        ("3.5", "0.1524"),
        ("3.9", "0.1524"),
        ("2.9", "0.152"),
        ("4.5", "0.1524"),
        ("5.5", "0.152449"),
        ("\"3.9\"", "0.1524"),
        ("\"4\"", "0.1524"),
        ("20", "0.15244901723741"),
        ("32767", "0.15244901723741"),
    ];
    for (precision, expected) in cases {
        assert_eq!(
            eval(&format!("=EUROCONVERT(1,\"FRF\",\"EUR\",TRUE,{precision})")),
            expected,
            "{precision}"
        );
    }
    // values above the VBA Integer limit overflow
    assert_eq!(eval("=EUROCONVERT(1,\"FRF\",\"EUR\",TRUE,32768)"), "#NUM!");
}

#[test]
fn euroconvert_overflow_is_num_error() {
    assert_eq!(eval("=EUROCONVERT(1E+308,\"EUR\",\"ITL\",TRUE)"), "#NUM!");
    assert_eq!(eval("=EUROCONVERT(1E+308,\"EUR\",\"ITL\")"), "#NUM!");
    assert_eq!(eval("=EUROCONVERT(1E+308,\"EUR\",\"EUR\")"), "1E+308");
}

#[test]
fn euroconvert_takes_no_multi_cell_argument() {
    let mut model = new_empty_model();
    model._set("B1", "1.2");
    model._set("C1", "2.4");
    model._set("A1", "=EUROCONVERT(B1:C1,\"DEM\",\"EUR\")");
    model._set("A2", "=EUROCONVERT(B1:B1,\"DEM\",\"EUR\")");
    model._set("A3", "=EUROCONVERT(1,\"DEM\",\"EUR\",TRUE,B1:C1)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#VALUE!");
    assert_eq!(model._get_text("A2"), "0.61");
    assert_eq!(model._get_text("A3"), "#VALUE!");
}

#[test]
fn jis_widens_the_same_way_dbcs_does() {
    // DBCS and JIS are the same function under two names.
    assert_eq!(eval("=JIS(\"ABC 123\")"), "ＡＢＣ　１２３");
    assert_eq!(eval("=JIS(\"¢£¬¯¦¥₩\")"), "￠￡￢￣￤￥￦");
    assert_eq!(eval("=JIS(\"ｶﾀｶﾅ･ｰ｢｣｡､\")"), "カタカナ・ー「」。、");
    assert_eq!(eval("=JIS(\"ｶﾞｷﾞｸﾞｹﾞｺﾞ\")"), "ガギグゲゴ");
    assert_eq!(eval("=JIS(\"ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟｳﾞ\")"), "パピプペポヴ");
    assert_eq!(eval("=JIS(\"한글\")"), "한글");
    assert_eq!(eval("=JIS(\"abc\")"), eval("=DBCS(\"abc\")"));
    assert_eq!(eval("=JIS()"), "#VALUE!");
    assert_eq!(eval("=JIS(\"a\",\"b\")"), "#VALUE!");
    let mut model = new_empty_model();
    model._set("A1", "=JIS({\"a\",\"1\"})");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "ａ");
    assert_eq!(model._get_text("B1"), "１");
}

#[test]
fn phonetic_gives_the_text_of_the_first_cell() {
    // The engine holds no furigana (the XLSX reader drops `rPh` runs), so every cell is one
    // without: the reference app gives the cell's own text.
    let mut model = new_empty_model();
    model._set("B1", "東京都");
    model._set("B2", "大阪府");
    model._set("B3", "12.5");
    model._set("A1", "=PHONETIC(B1)");
    model._set("A2", "=PHONETIC(B1:B2)");
    model._set("A3", "=PHONETIC(B3)");
    model._set("A4", "=PHONETIC(\"かな\")");
    model._set("A5", "=PHONETIC(C9)");
    model._set("A6", "=PHONETIC(B1,B2)");
    model._set("A7", "=PHONETIC()");
    model._set("A8", "=PHONETIC(#N/A)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "東京都");
    // a range gives its upper left cell
    assert_eq!(model._get_text("A2"), "東京都");
    assert_eq!(model._get_text("A3"), "12.5");
    assert_eq!(model._get_text("A4"), "かな");
    assert_eq!(model._get_text("A5"), "");
    assert_eq!(model._get_text("A6"), "#VALUE!");
    assert_eq!(model._get_text("A7"), "#VALUE!");
    assert_eq!(model._get_text("A8"), "#N/A");
}

#[test]
fn phonetic_uses_stored_reading_across_sheets_sort_axis_and_recalc_session() {
    use easyoffice_sheet::{
        Axis, AxisBudget, AxisEdit, CellValueKind, EditKind, EvaluatedCellValue,
        PackedCellCoordinate, RecalcSession, SortKey, TypedCellChange, WorkbookEngine,
    };

    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S1");
    wb.add_sheet("S2");
    let b1 = PackedCellCoordinate::parse_a1("B1").unwrap();
    let b2 = PackedCellCoordinate::parse_a1("B2").unwrap();
    let c2 = PackedCellCoordinate::parse_a1("C2").unwrap();

    wb.sheet_mut("S1")
        .unwrap()
        .set_cell_typed_with_phonetic_coord(b1, "東京都", None, Some("トウキョウト"));
    wb.sheet_mut("S1")
        .unwrap()
        .set_cell_typed_with_phonetic_coord(b2, "大阪府", None, Some("おおさかふ"));
    wb.sheet_mut("S1")
        .unwrap()
        .set_cell_typed_with_phonetic_coord(c2, "001", Some(CellValueKind::Text), Some("ゼロ"));
    wb.sheet_mut("S2")
        .unwrap()
        .set_cell_typed_with_phonetic_coord(b1, "北海道", None, Some("ホッカイドウ"));

    // Direct reference, multi-cell upper-left, cross-sheet, INDIRECT, OFFSET, formula cell fallback
    assert_eq!(
        wb.evaluate_formula_on_sheet("S1", "=PHONETIC(B1)"),
        EvaluatedCellValue::Text("トウキョウト".into())
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("S1", "=PHONETIC(B1:C2)"),
        EvaluatedCellValue::Text("トウキョウト".into())
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("S1", "=PHONETIC(S2!B1)"),
        EvaluatedCellValue::Text("ホッカイドウ".into())
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("S1", "=PHONETIC(INDIRECT(\"C2\"))"),
        EvaluatedCellValue::Text("ゼロ".into())
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("S1", "=PHONETIC(OFFSET(A1,1,1))"),
        EvaluatedCellValue::Text("おおさかふ".into())
    );
    wb.set_cell("S1", "D1", "=\"東京都\"").unwrap();
    assert_eq!(
        wb.evaluate_formula_on_sheet("S1", "=PHONETIC(D1)"),
        EvaluatedCellValue::Text("東京都".into())
    );

    // Axis row insertion shifts phonetic coordinates and triggers dependency recalculation.
    let mut sess = RecalcSession::new(wb).unwrap();
    let a1 = PackedCellCoordinate::parse_a1("A1").unwrap();
    let a2 = PackedCellCoordinate::parse_a1("A2").unwrap();
    let base_bytes = sess.footprint().raw_bytes;
    assert!(base_bytes >= "トウキョウト".len());
    sess.set_cell("S1", a1, "=PHONETIC(B1)").unwrap();
    let r1 = sess.recalculate();
    assert!(r1
        .changed
        .iter()
        .any(|c| c.coord == a1 && c.value == EvaluatedCellValue::Text("トウキョウト".into())));

    // Updating only the phonetic string (same base text!) invalidates A1 and updates raw_bytes.
    sess.apply_typed(&[TypedCellChange {
        sheet: "S1".into(),
        coord: b1,
        raw: "東京都".into(),
        kind: None,
        phonetic: Some("とうきょうと".into()),
    }])
    .unwrap();
    let r2 = sess.recalculate();
    assert!(r2
        .changed
        .iter()
        .any(|c| c.coord == a1 && c.value == EvaluatedCellValue::Text("とうきょうと".into())));

    // Clearing phonetic falls back to cell text cleanly.
    sess.apply_typed(&[TypedCellChange {
        sheet: "S1".into(),
        coord: b1,
        raw: "東京都".into(),
        kind: None,
        phonetic: None,
    }])
    .unwrap();
    let r3 = sess.recalculate();
    assert!(r3
        .changed
        .iter()
        .any(|c| c.coord == a1 && c.value == EvaluatedCellValue::Text("東京都".into())));

    // Restoring furigana then inserting row 1 shifts B1 -> B2 while A1 (=PHONETIC(B1) -> =PHONETIC(B2)).
    sess.apply_typed(&[TypedCellChange {
        sheet: "S1".into(),
        coord: b1,
        raw: "東京都".into(),
        kind: None,
        phonetic: Some("トウキョウト".into()),
    }])
    .unwrap();
    sess.recalculate();
    let (_, after_insert) = sess
        .apply_axis_edit(
            "S1",
            AxisEdit {
                axis: Axis::Row,
                kind: EditKind::Insert,
                at: 0,
                count: 1,
                cols: None,
            },
            AxisBudget::default(),
        )
        .unwrap();
    assert!(after_insert.changed.iter().all(|c| c.coord != a2));
    assert_eq!(
        sess.workbook()
            .evaluate_formula_on_sheet("S1", "=PHONETIC(B2)"),
        EvaluatedCellValue::Text("トウキョウト".into())
    );

    // Sorting rows moves each cell's furigana alongside its row!
    let mut sort_sheet = easyoffice_sheet::SheetEngine::new("Sort");
    let r0 = PackedCellCoordinate::new(0, 0).unwrap();
    let r1 = PackedCellCoordinate::new(1, 0).unwrap();
    sort_sheet.set_cell_typed_with_phonetic_coord(r0, "山", None, Some("ヤマ"));
    sort_sheet.set_cell_typed_with_phonetic_coord(r1, "川", None, Some("カワ"));
    sort_sheet
        .set_cell("B1", "20")
        .and_then(|_| sort_sheet.set_cell("B2", "10"))
        .unwrap();
    sort_sheet
        .sort_rows(0, 1, &[SortKey::asc(1)], false)
        .unwrap();
    assert_eq!(sort_sheet.cell_phonetic_coord(r0), Some("カワ"));
    assert_eq!(sort_sheet.cell_phonetic_coord(r1), Some("ヤマ"));
}
