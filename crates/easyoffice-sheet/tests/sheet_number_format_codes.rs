//! Number format codes beyond the basic table: colors, conditions, fractions, locales,
//! Korean date names, `DBNum`, width spacing and fill, plus section selection edge cases.

use easyoffice_sheet::{
    compiled_format, format_number_styled, format_number_with_code, format_text_value_with_code,
    FormatColor, FormulaError,
};

fn fmt(num: f64, code: &str) -> String {
    format_number_with_code(num, code).unwrap_or_else(|e| format!("ERR {e:?}"))
}

#[test]
fn color_sections_report_color_without_changing_text() {
    let pos = format_number_styled(1234.5, "#,##0.0;[Red]-#,##0.0").unwrap();
    assert_eq!((pos.text.as_str(), pos.color), ("1,234.5", None));
    let neg = format_number_styled(-1234.5, "#,##0.0;[Red]-#,##0.0").unwrap();
    assert_eq!(neg.text, "-1,234.5");
    assert_eq!(neg.color, Some(FormatColor(0xFF0000)));
    assert_eq!(neg.color.unwrap().hex(), "#ff0000");
    let blue = format_number_styled(3.0, "[Blue]0").unwrap();
    assert_eq!(
        (blue.text.as_str(), blue.color),
        ("3", Some(FormatColor(0x0000FF)))
    );
    let idx = format_number_styled(3.0, "[Color3]0").unwrap();
    assert_eq!(idx.color, Some(FormatColor(0xFF0000)));
    assert_eq!(format_number_styled(3.0, "[Color57]0").unwrap().color, None);
    let text = compiled_format("0;0;0;[Green]@").render_text("hi");
    assert_eq!(
        (text.text.as_str(), text.color),
        ("hi", Some(FormatColor(0x00FF00)))
    );
}

#[test]
fn conditions_choose_sections() {
    let code = "[>=100]\"big\";[<0]\"neg\";\"small\"";
    assert_eq!(fmt(150.0, code), "big");
    assert_eq!(fmt(-1.0, code), "neg");
    assert_eq!(fmt(5.0, code), "small");
    assert_eq!(fmt(100.0, code), "big");
    let one = "[>100]0.0;0";
    assert_eq!(fmt(150.0, one), "150.0");
    assert_eq!(fmt(50.0, one), "50");
    assert_eq!(fmt(-50.0, one), "50");
    let eq = "[=0]\"zero\";General";
    assert_eq!(fmt(0.0, eq), "zero");
    assert_eq!(fmt(2.5, eq), "2.5");
}

#[test]
fn fractions() {
    for (v, code, want) in [
        (1.5, "# ?/?", "1 1/2"),
        (0.5, "# ?/?", " 1/2"),
        (3.0, "# ?/?", "3    "),
        (1.5, "?/?", "3/2"),
        (3.14158, "# ??/??", "3 14/99"),
        (0.125, "# ?/8", " 1/8"),
        (2.3, "# ?/16", "2 5/16"),
        (0.3333333, "# ?/?", " 1/3"),
        (-1.25, "# ?/?", "-1 1/4"),
        (0.0, "# ?/?", "0    "),
        (12.5, "# ?/?", "12 1/2"),
        (0.999999999, "# ?/?", "1    "),
    ] {
        assert_eq!(fmt(v, code), want, "{code} <- {v}");
    }
}

#[test]
fn locale_and_korean_names() {
    // 45000 = 2023-03-15, a Wednesday; 45000.75 = 18:00.
    assert_eq!(fmt(45000.0, "[$-412]yyyy-mm-dd"), "2023-03-15");
    assert_eq!(fmt(45000.0, "[$-412]mmm"), "3월");
    assert_eq!(fmt(45000.0, "[$-412]dddd"), "수요일");
    assert_eq!(fmt(45000.0, "aaa"), "수");
    assert_eq!(fmt(45000.0, "aaaa"), "수요일");
    assert_eq!(
        fmt(45000.0, "yyyy\"년\" m\"월\" d\"일\" (aaa)"),
        "2023년 3월 15일 (수)"
    );
    assert_eq!(fmt(45000.75, "[$-412]AM/PM h:mm"), "오후 6:00");
    assert_eq!(fmt(45000.25, "[$-412]AM/PM h:mm"), "오전 6:00");
    assert_eq!(fmt(45000.75, "AM/PM h:mm"), "PM 6:00");
    assert_eq!(fmt(45000.0, "[$-409]ddd"), "Wed");
    assert_eq!(fmt(1234.5, "[$₩-412]#,##0"), "₩1,235");
    assert_eq!(fmt(1234.5, "[$-412]#,##0"), "1,235");
    assert_eq!(fmt(1234.5, "[$$-409]#,##0.00"), "$1,234.50");
}

#[test]
fn dbnum_numerals() {
    for (v, code, want) in [
        (1234.0, "[DBNum1]0", "一千二百三十四"),
        (10001.0, "[DBNum1]0", "一萬零一"),
        (100000001.0, "[DBNum1]General", "一億零一"),
        (0.0, "[DBNum1]0", "零"),
        (1234.0, "[DBNum2]0", "壹阡貳百參拾四"),
        (1234.0, "[DBNum3]0", "１２３４"),
        (1234.0, "[DBNum4]0", "일천이백삼십사"),
        (12.5, "[DBNum4]0.0", "일십이.오"),
        (1234.0, "[DBNum1]0\"원\"", "一千二百三十四원"),
    ] {
        assert_eq!(fmt(v, code), want, "{code} <- {v}");
    }
}

#[test]
fn width_spacing_and_fill() {
    assert_eq!(fmt(5.0, "0_)"), "5 ");
    assert_eq!(fmt(-5.0, "0_);(0)"), "(5)");
    let f = format_number_styled(5.0, "*-0").unwrap();
    assert_eq!((f.text.as_str(), f.fill), ("5", Some((0, '-'))));
    let f = format_number_styled(5.0, "0*.").unwrap();
    assert_eq!((f.text.as_str(), f.fill), ("5", Some((1, '.'))));
    assert_eq!(format_number_styled(5.0, "0").unwrap().fill, None);
}

#[test]
fn section_edges() {
    assert_eq!(fmt(-5.0, "0"), "-5");
    assert_eq!(fmt(-5.0, "0;0"), "5");
    assert_eq!(fmt(0.0, "0;-0;\"zero\""), "zero");
    assert_eq!(fmt(5.0, "0;@"), "5");
    assert_eq!(fmt(-5.0, "0;@"), "-5");
    assert_eq!(
        format_text_value_with_code("abc", "0;0;0;\"[\"@\"]\""),
        "[abc]"
    );
    assert_eq!(format_text_value_with_code("abc", "0.00"), "abc");
    assert_eq!(fmt(-0.0, "0.0"), "0.0");
    assert_eq!(fmt(1.0 / 3.0, "0.000"), "0.333");
    assert_eq!(fmt(1234567.0, "0.00E+00"), "1.23E+06");
    assert_eq!(
        format_number_with_code(f64::NAN, "0"),
        Err(FormulaError::Num)
    );
    assert_eq!(
        format_number_with_code(-1.0, "yyyy"),
        Err(FormulaError::Value)
    );
    assert_eq!(
        format_number_with_code(3_000_000.0, "yyyy"),
        Err(FormulaError::Value)
    );
}

mod cell_display {
    use easyoffice_sheet::format_cell_value;

    fn shown(code: &str, raw: &str) -> Option<String> {
        format_cell_value(code, raw).map(|f| f.text)
    }

    #[test]
    fn numbers_use_the_number_sections() {
        assert_eq!(shown("#,##0.00", "1234.5").as_deref(), Some("1,234.50"));
        assert_eq!(shown("0.0%", "0.256").as_deref(), Some("25.6%"));
        assert_eq!(shown("yyyy-mm-dd", "45000").as_deref(), Some("2023-03-15"));
        assert_eq!(shown("\"₩\"#,##0", " 1500 ").as_deref(), Some("₩1,500"));
        assert_eq!(shown("0.00E+00", "12345").as_deref(), Some("1.23E+04"));
        assert_eq!(shown("# ?/?", "0.75").as_deref(), Some(" 3/4"));
    }

    #[test]
    fn negative_sections_carry_their_color() {
        let f = format_cell_value("#,##0;[Red]-#,##0", "-5").unwrap();
        assert_eq!(f.text, "-5");
        assert_eq!(f.color.map(|c| c.hex()).as_deref(), Some("#ff0000"));
    }

    #[test]
    fn text_keeps_its_display_unless_the_code_has_a_text_section() {
        assert_eq!(shown("0.00", "abc"), None);
        assert_eq!(shown("0.00;;;\"[\"@\"]\"", "abc").as_deref(), Some("[abc]"));
        // The text format shows a number-looking value exactly as typed.
        assert_eq!(shown("@", "00123").as_deref(), Some("00123"));
    }

    #[test]
    fn values_the_code_cannot_show_fall_back_to_general() {
        assert_eq!(shown("yyyy-mm-dd", "-3"), None);
        assert_eq!(shown("0.00", "inf"), None);
        assert_eq!(shown("0.00", ""), None);
    }
}
