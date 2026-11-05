use easyoffice_formats::{
    convert_eqedit_to_latex, decode_hwp5_eqedit_script, decode_hwp5_para_text_bytes,
    hwp5_symbol_to_plane15, plane15_to_hwp5_symbol, EasyOfficeError, Hwp5Codec, HwpxCodec,
    ZipArchiveCodec, HWP5_PUA_BMP_SYMBOLS, HWPTAG_EQEDIT, MAX_EQEDIT_DEPTH,
};

#[test]
fn eqedit_fractions_radicals_and_scripts_convert_to_latex() {
    assert_eq!(convert_eqedit_to_latex("a over b").unwrap(), "\\frac{a}{b}");
    assert_eq!(
        convert_eqedit_to_latex("{x + 1} over {x - 1}").unwrap(),
        "\\frac{x + 1}{x - 1}"
    );
    assert_eq!(convert_eqedit_to_latex("a atop b").unwrap(), "{a \\atop b}");
    assert_eq!(
        convert_eqedit_to_latex("sqrt {x + 1}").unwrap(),
        "\\sqrt{x + 1}"
    );
    assert_eq!(
        convert_eqedit_to_latex("root n of {x}").unwrap(),
        "\\sqrt[n]{x}"
    );
    assert_eq!(
        convert_eqedit_to_latex("x^2 + y_1 + z_i^{n+1}").unwrap(),
        "x^2 + y_1 + z_i^{n + 1}"
    );
    assert_eq!(
        convert_eqedit_to_latex("a sup 2 + b sub k").unwrap(),
        "a^2 + b_k"
    );
    assert_eq!(
        convert_eqedit_to_latex("x lsub 1 lsup 2").unwrap(),
        "{}^2{}_1x"
    );
}

#[test]
fn eqedit_big_operators_matrices_delimiters_and_accents_convert() {
    assert_eq!(
        convert_eqedit_to_latex("sum from {i=1} to {n} x_i").unwrap(),
        "\\sum_{i = 1}^n x_i"
    );
    assert_eq!(
        convert_eqedit_to_latex("sum from i=1 to n x_i").unwrap(),
        "\\sum_{i = 1}^n x_i"
    );
    assert_eq!(
        convert_eqedit_to_latex("int from 0 to infty e^{-x}").unwrap(),
        "\\int_0^{\\infty} e^{- x}"
    );
    assert_eq!(
        convert_eqedit_to_latex("int to infty e^{-x}").unwrap(),
        "\\int^{\\infty} e^{- x}"
    );
    assert_eq!(
        convert_eqedit_to_latex("dint_D f + tint_V g + coprod_i A_i + bigcup_k U_k + bigcap_k V_k")
            .unwrap(),
        "\\iint_D f + \\iiint_V g + \\coprod_i A_i + \\bigcup_k U_k + \\bigcap_k V_k"
    );
    assert_eq!(
        convert_eqedit_to_latex("matrix{a & b # c & d}").unwrap(),
        "\\begin{matrix}a & b \\\\ c & d\\end{matrix}"
    );
    assert_eq!(
        convert_eqedit_to_latex("pmatrix{1 & 0 # 0 & 1}").unwrap(),
        "\\begin{pmatrix}1 & 0 \\\\ 0 & 1\\end{pmatrix}"
    );
    assert_eq!(
        convert_eqedit_to_latex("pmatrix{1 over 2 & 0 # 0 & 1}").unwrap(),
        "\\begin{pmatrix}\\frac{1}{2} & 0 \\\\ 0 & 1\\end{pmatrix}"
    );
    assert_eq!(
        convert_eqedit_to_latex("bmatrix{1 & 2 # 3 & 4}").unwrap(),
        "\\begin{bmatrix}1 & 2 \\\\ 3 & 4\\end{bmatrix}"
    );
    assert_eq!(
        convert_eqedit_to_latex("dmatrix{a & b # c & d}").unwrap(),
        "\\begin{vmatrix}a & b \\\\ c & d\\end{vmatrix}"
    );
    assert_eq!(
        convert_eqedit_to_latex("cases{x & 0 # -x & 1}").unwrap(),
        "\\begin{cases}x & 0 \\\\ - x & 1\\end{cases}"
    );
    assert_eq!(
        convert_eqedit_to_latex("left ( a over b right )").unwrap(),
        "\\left(\\frac{a}{b}\\right)"
    );
    assert_eq!(
        convert_eqedit_to_latex("left { x + 1 right }").unwrap(),
        "\\left\\{x + 1\\right\\}"
    );
    assert_eq!(
        convert_eqedit_to_latex("left langle x right rangle").unwrap(),
        "\\left\\langle x\\right\\rangle"
    );
    assert_eq!(
        convert_eqedit_to_latex("left lfloor x right rfloor + left lceil y right rceil").unwrap(),
        "\\left\\lfloor x\\right\\rfloor + \\left\\lceil y\\right\\rceil"
    );
    assert_eq!(
        convert_eqedit_to_latex("bar x + hat y + vec v + dot t + tilde a").unwrap(),
        "\\bar{x} + \\hat{y} + \\vec{v} + \\dot{t} + \\tilde{a}"
    );
    assert_eq!(
        convert_eqedit_to_latex("binom {n} {k}").unwrap(),
        "\\binom{n}{k}"
    );
}

#[test]
fn eqedit_fonts_greek_case_and_operators_convert() {
    assert_eq!(
        convert_eqedit_to_latex("rmd x + itx + boldv").unwrap(),
        "\\mathrm{d}x + \\mathit{x} + \\mathbf{v}"
    );
    assert_eq!(
        convert_eqedit_to_latex("alpha + GAMMA + Delta + OMEGA").unwrap(),
        "\\alpha + \\Gamma + \\Delta + \\Omega"
    );
    assert_eq!(
        convert_eqedit_to_latex("{A OVER B} + SQRT {X}").unwrap(),
        "\\frac{A}{B} + \\sqrt{X}"
    );
    let ops = convert_eqedit_to_latex(
        "a times b div c cdot d pm e mp f leq g geq h neq i approx j equiv k in S subset T cup U cap V rarrow W",
    )
    .unwrap();
    for expected in [
        "\\times",
        "\\div",
        "\\cdot",
        "\\pm",
        "\\mp",
        "\\leq",
        "\\geq",
        "\\neq",
        "\\approx",
        "\\equiv",
        "\\in",
        "\\subset",
        "\\cup",
        "\\cap",
        "\\rightarrow",
    ] {
        assert!(ops.contains(expected), "missing {expected} in {ops}");
    }
    assert_eq!(convert_eqedit_to_latex("a ` b ~ c").unwrap(), "a\\,b\\ c");
}

#[test]
fn eqedit_fails_closed_on_unbalanced_braces_and_excessive_depth() {
    let unclosed = convert_eqedit_to_latex("sqrt { x + 1");
    assert!(matches!(
        unclosed,
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let unmatched_close = convert_eqedit_to_latex("x + 1 }");
    assert!(matches!(
        unmatched_close,
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    assert_eq!(MAX_EQEDIT_DEPTH, 64);
    let ok_50 = "{ ".repeat(50) + "x" + &" }".repeat(50);
    assert_eq!(convert_eqedit_to_latex(&ok_50).unwrap(), "x");

    let deep_66 = "{ ".repeat(66) + "x" + &" }".repeat(66);
    let depth_err = convert_eqedit_to_latex(&deep_66);
    assert!(matches!(
        depth_err,
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hwp5_eqedit_binary_record_decodes_to_latex_and_rejects_truncation() {
    assert_eq!(HWPTAG_EQEDIT, 88);
    let script = "sum from {k=1} to {n} {1 over k^2}";
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(&(utf16.len() as u16).to_le_bytes());
    for u in &utf16 {
        payload.extend_from_slice(&u.to_le_bytes());
    }
    let latex = decode_hwp5_eqedit_script(&payload).unwrap();
    assert_eq!(latex, "\\sum_{k = 1}^n\\frac{1}{k^2}");

    assert!(matches!(
        decode_hwp5_eqedit_script(&[0, 1, 2]),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    let mut truncated = payload.clone();
    truncated.truncate(8);
    assert!(matches!(
        decode_hwp5_eqedit_script(&truncated),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
}

#[test]
fn hwp5_pua_bmp_symbols_map_bidirectionally_with_plane15() {
    assert_eq!(HWP5_PUA_BMP_SYMBOLS.len(), 53);
    for &unit in HWP5_PUA_BMP_SYMBOLS {
        let plane15 = hwp5_symbol_to_plane15(unit).expect("symbol should map to plane 15");
        assert_eq!(plane15 & 0xFF_F000, 0x0F_0000);
        let roundtrip = plane15_to_hwp5_symbol(plane15).expect("plane 15 should map back to BMP");
        assert_eq!(roundtrip, unit);
    }
    assert_eq!(hwp5_symbol_to_plane15(0xA813), None);
    assert_eq!(hwp5_symbol_to_plane15(0xA2C1), None);
    assert_eq!(hwp5_symbol_to_plane15(0xA2C2), None);
    assert_eq!(plane15_to_hwp5_symbol(0x0F_0813), None);
}

#[test]
fn hwp5_para_text_handles_16byte_tabs_autonumber_spaces_and_unpaired_surrogates() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0xAC00u16.to_le_bytes());
    bytes.extend_from_slice(&0x0009u16.to_le_bytes());
    bytes.extend_from_slice(&[0x40, 0x1F, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    bytes.extend_from_slice(&0x0000u16.to_le_bytes());
    bytes.extend_from_slice(&0x0009u16.to_le_bytes());
    bytes.extend_from_slice(&0x0012u16.to_le_bytes());
    bytes.extend_from_slice(&[0x01; 12]);
    bytes.extend_from_slice(&0x0012u16.to_le_bytes());
    bytes.extend_from_slice(&0x0012u16.to_le_bytes());
    bytes.extend_from_slice(&0x0018u16.to_le_bytes());
    bytes.extend_from_slice(&0x001Eu16.to_le_bytes());
    bytes.extend_from_slice(&0x001Fu16.to_le_bytes());
    bytes.extend_from_slice(&0xDFDAu16.to_le_bytes());
    bytes.extend_from_slice(&0xDB80u16.to_le_bytes());
    bytes.extend_from_slice(&0xD83Du16.to_le_bytes());
    bytes.extend_from_slice(&0xDE00u16.to_le_bytes());
    bytes.extend_from_slice(&0xDB80u16.to_le_bytes());
    bytes.extend_from_slice(&0xDCE1u16.to_le_bytes());

    let decoded = decode_hwp5_para_text_bytes(&bytes).unwrap();
    assert_eq!(decoded, "가\t  \u{00AD}\u{00A0}\u{2007}□□😀\u{A0E1}");
    let via_codec = Hwp5Codec::decode_hwp5_para_text(&bytes).unwrap();
    assert_eq!(via_codec, "가\t  \u{00AD}\u{00A0}\u{2007}□□😀\u{A0E1}");
}

#[test]
fn hwpx_unpack_converts_hp_equation_scripts_and_fails_closed_on_malformed_equations() {
    let build_hwpx_with_section = |sec_body: &str| -> Vec<u8> {
        let container_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ocf:container xmlns:ocf=\"urn:oasis:names:tc:opendocument:xmlns:container\"><ocf:rootfiles><ocf:rootfile full-path=\"Contents/content.hpf\" media-type=\"application/hwpml-package+xml\"/></ocf:rootfiles></ocf:container>";
        let content_hpf = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\" version=\"1.0\"><opf:metadata><opf:title>수식문서</opf:title></opf:metadata></opf:package>";
        let header_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hh:head xmlns:hh=\"http://www.owpml.org/owpml/2011/head\" version=\"1.4\" title=\"수식문서\"/>";
        let sec_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hs:sec xmlns:hs=\"http://www.owpml.org/owpml/2011/section\" xmlns:hp=\"http://www.owpml.org/owpml/2011/paragraph\">{sec_body}</hs:sec>"
        );
        ZipArchiveCodec::pack(&[
            (
                "mimetype".to_string(),
                HwpxCodec::MIMETYPE_HWPX.as_bytes().to_vec(),
            ),
            (
                "META-INF/container.xml".to_string(),
                container_xml.as_bytes().to_vec(),
            ),
            (
                "Contents/content.hpf".to_string(),
                content_hpf.as_bytes().to_vec(),
            ),
            (
                "Contents/header.xml".to_string(),
                header_xml.as_bytes().to_vec(),
            ),
            ("Contents/section0.xml".to_string(), sec_xml.into_bytes()),
        ])
        .unwrap()
    };

    let valid_bytes = build_hwpx_with_section(
        "<hp:p><hp:run><hp:t>문단1</hp:t></hp:run></hp:p><hp:p><hp:run><hp:equation><hp:script>left langle x right rangle = {1 over 2}</hp:script></hp:equation></hp:run></hp:p><hp:p><hp:run><hp:t>문단2</hp:t></hp:run></hp:p>",
    );
    let unpacked = HwpxCodec::unpack(&valid_bytes).unwrap();
    assert_eq!(
        unpacked.paragraphs,
        vec![
            "문단1".to_string(),
            "$$\\left\\langle x\\right\\rangle = \\frac{1}{2}$$".to_string(),
            "문단2".to_string(),
        ]
    );
    assert_eq!(
        convert_eqedit_to_latex("sum from i=-1 to n x_i").unwrap(),
        "\\sum_{i = - 1}^n x_i"
    );
    assert_eq!(
        convert_eqedit_to_latex("lim from x rarrow 0 f(x)").unwrap(),
        "\\lim_{x\\rightarrow 0} f(x)"
    );
    assert_eq!(
        convert_eqedit_to_latex("int from D (x + 1)").unwrap(),
        "\\int_D(x + 1)"
    );
    assert_eq!(
        convert_eqedit_to_latex("int from -1 to 1 x dx").unwrap(),
        "\\int_{- 1}^1 xdx"
    );
    assert!(matches!(
        convert_eqedit_to_latex("{ a + right }"),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let tbl_eq_bytes = build_hwpx_with_section(
        "<hp:tbl><hp:tr><hp:tc><hp:p><hp:run><hp:t>셀</hp:t></hp:run></hp:p><hp:p><hp:run><hp:equation><hp:script>a over b</hp:script></hp:equation></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>",
    );
    let tbl_unpacked = HwpxCodec::unpack(&tbl_eq_bytes).unwrap();
    assert_eq!(
        tbl_unpacked.tables,
        vec![vec![vec!["셀\n$$\\frac{a}{b}$$".to_string()]]]
    );

    let broken_tbl_eq_bytes = build_hwpx_with_section(
        "<hp:tbl><hp:tr><hp:tc><hp:p><hp:run><hp:equation><hp:script>sqrt { x + 1</hp:script></hp:equation></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>",
    );
    assert!(matches!(
        HwpxCodec::unpack(&broken_tbl_eq_bytes),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let broken_bytes = build_hwpx_with_section(
        "<hp:p><hp:run><hp:equation><hp:script>sqrt { x + 1</hp:script></hp:equation></hp:run></hp:p>",
    );
    assert!(matches!(
        HwpxCodec::unpack(&broken_bytes),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let deep_script = "{ ".repeat(66) + "x" + &" }".repeat(66);
    let deep_bytes = build_hwpx_with_section(&format!(
        "<hp:p><hp:run><hp:equation><hp:script>{deep_script}</hp:script></hp:equation></hp:run></hp:p>"
    ));
    assert!(matches!(
        HwpxCodec::unpack(&deep_bytes),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
