//! PD-379: the underline shape and colour (`hh:underline`) and the emphasis mark (`hh:charPr@symMark`) of a run survive HWPX
//! in the words of the reference parser's tables (13 line shapes, 7 marks); what the 17 underline names and the 5 marks cannot
//! say is mapped by a fixed rule, and a hostile value reads as not set.

use easyoffice_formats::{
    CharAdvanced, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
    TextFx, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn run(text: &str, underline: bool, color: Option<&str>, fx: TextFx) -> RichRun {
    RichRun {
        text: text.to_string(),
        underline,
        color: color.map(str::to_string),
        advanced: CharAdvanced {
            fx,
            ..CharAdvanced::default()
        },
        ..RichRun::default()
    }
}

fn document(runs: Vec<RichRun>) -> RichDocument {
    RichDocument {
        title: "효과".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs,
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    }
}

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("expected document: {other:?}"),
    }
}

fn the_run(doc: &RichDocument, text: &str) -> RichRun {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter().cloned())
        .find(|r| r.text == text)
        .unwrap_or_else(|| panic!("run {text}"))
}

fn encode(doc: &RichDocument, format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, format, None).unwrap()
}

fn header(bytes: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries
        .iter()
        .find(|e| e.path == "Contents/header.xml")
        .expect("header");
    String::from_utf8(entry.data.clone()).expect("utf8")
}

/// `hwpx` with its `Contents/header.xml` passed through `edit`.
fn with_header(hwpx: &[u8], edit: impl Fn(&str) -> String) -> Vec<u8> {
    let edited = edit(&header(hwpx));
    let entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(hwpx, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                if e.path == "Contents/header.xml" {
                    (e.path, edited.clone().into_bytes())
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

/// The `hh:underline` element of the charPr that holds the run `x` (the only underlined one in these documents).
fn underline_element(header: &str) -> &str {
    let start = header
        .find("<hh:underline type=\"BOTTOM\"")
        .expect("underline");
    &header[start..start + header[start..].find("/>").unwrap() + 2]
}

/// The HWPX line shape each of the 17 underline names is written as (the 13 shapes of the reference parser's table; the
/// heavy and word-only names have none and take their plain shape).
const SHAPE_OF_STYLE: [&str; 17] = [
    "SOLID",
    "SOLID",
    "DOUBLE_SLIM",
    "SOLID",
    "DOT",
    "DOT",
    "DASH",
    "DASH",
    "LONG_DASH",
    "LONG_DASH",
    "DASH_DOT",
    "DASH_DOT",
    "DASH_DOT_DOT",
    "DASH_DOT_DOT",
    "WAVE",
    "WAVE",
    "DOUBLE_WAVE",
];

/// The underline name each style comes back as (the plain one of the pair that shares a shape).
const STYLE_BACK: [u8; 17] = [0, 0, 2, 0, 4, 4, 6, 6, 8, 8, 10, 10, 12, 12, 14, 14, 16];

const SYM_MARKS: [&str; 5] = ["NONE", "DOT_ABOVE", "TILDE", "RING_ABOVE", "SIDE"];

fn underlined(style: u8, color: Option<u32>, emphasis: u8) -> RichDocument {
    document(vec![run(
        "x",
        true,
        None,
        TextFx {
            underline_style: style,
            underline_color: color,
            emphasis,
            ..TextFx::default()
        },
    )])
}

#[test]
fn docx_to_hwpx_to_docx_keeps_the_shape_the_colour_and_the_mark() {
    for style in 0..17u8 {
        let emphasis = style % 5;
        let docx = encode(
            &underlined(style, Some(0x1F_3A5F), emphasis),
            DocumentFormat::Docx,
        );
        let hwpx = encode(&decode(&docx, "a.docx"), DocumentFormat::Hwpx);
        let head = header(&hwpx);
        assert!(
            head.contains(&format!(
                "<hh:underline type=\"BOTTOM\" shape=\"{}\" color=\"#1F3A5F\"/>",
                SHAPE_OF_STYLE[usize::from(style)]
            )),
            "style {style}: {head}"
        );
        assert!(
            head.contains(&format!("symMark=\"{}\"", SYM_MARKS[usize::from(emphasis)])),
            "mark {emphasis}: {head}"
        );
        let back = the_run(&decode(&hwpx, "a.hwpx"), "x");
        assert!(back.underline, "style {style}");
        let fx = back.advanced.fx;
        assert_eq!(
            fx.underline_style,
            STYLE_BACK[usize::from(style)],
            "{style}"
        );
        assert_eq!(fx.underline_color, Some(0x1F_3A5F), "{style}");
        assert_eq!(fx.emphasis, emphasis, "{style}");
        let docx2 = encode(&decode(&hwpx, "a.hwpx"), DocumentFormat::Docx);
        let fx2 = the_run(&decode(&docx2, "a.docx"), "x").advanced.fx;
        assert_eq!(fx2, fx, "{style}");
    }
}

#[test]
fn an_automatic_colour_follows_the_text_colour_and_reads_back_as_automatic() {
    let doc = document(vec![run(
        "x",
        true,
        Some("FF0000"),
        TextFx {
            underline_style: 14,
            ..TextFx::default()
        },
    )]);
    let hwpx = encode(&doc, DocumentFormat::Hwpx);
    assert_eq!(
        underline_element(&header(&hwpx)),
        "<hh:underline type=\"BOTTOM\" shape=\"WAVE\" color=\"#FF0000\"/>"
    );
    let back = the_run(&decode(&hwpx, "a.hwpx"), "x");
    assert_eq!(back.advanced.fx.underline_color, None);
    assert_eq!(back.color.as_deref(), Some("FF0000"));
    // A colour that differs from the text colour stays a colour, black included.
    let black = document(vec![run(
        "x",
        true,
        Some("FF0000"),
        TextFx {
            underline_color: Some(0),
            ..TextFx::default()
        },
    )]);
    let back = the_run(
        &decode(&encode(&black, DocumentFormat::Hwpx), "a.hwpx"),
        "x",
    );
    assert_eq!(back.advanced.fx.underline_color, Some(0));
}

#[test]
fn a_plain_underline_and_a_plain_run_write_what_they_wrote_before() {
    let hwpx = encode(&underlined(0, None, 0), DocumentFormat::Hwpx);
    let head = header(&hwpx);
    assert_eq!(
        underline_element(&head),
        "<hh:underline type=\"BOTTOM\" shape=\"SOLID\" color=\"#000000\"/>"
    );
    assert!(!head.contains("symMark=\"DOT_ABOVE\""));
    let fx = the_run(&decode(&hwpx, "a.hwpx"), "x").advanced.fx;
    assert_eq!(fx, TextFx::default());
    // A style and a mark on a run that is not underlined: the mark stays, the stale underline shape is not written.
    let doc = document(vec![run(
        "x",
        false,
        None,
        TextFx {
            underline_style: 14,
            underline_color: Some(0xFF_0000),
            emphasis: 3,
            ..TextFx::default()
        },
    )]);
    let hwpx = encode(&doc, DocumentFormat::Hwpx);
    let head = header(&hwpx);
    assert!(head.contains("<hh:underline type=\"NONE\" shape=\"SOLID\" color=\"#000000\"/>"));
    let back = the_run(&decode(&hwpx, "a.hwpx"), "x");
    assert!(!back.underline);
    assert_eq!(
        back.advanced.fx,
        TextFx {
            emphasis: 3,
            ..TextFx::default()
        }
    );
}

#[test]
fn hwpx_to_hwpx_maps_all_thirteen_shapes_and_seven_marks_by_the_fixed_rule() {
    // (HWPX shape read, the shape it is written as): the four shapes with no underline name take the nearest one.
    let shapes = [
        ("SOLID", "SOLID"),
        ("DASH", "DASH"),
        ("DOT", "DOT"),
        ("DASH_DOT", "DASH_DOT"),
        ("DASH_DOT_DOT", "DASH_DOT_DOT"),
        ("LONG_DASH", "LONG_DASH"),
        ("CIRCLE", "DOT"),
        ("DOUBLE_SLIM", "DOUBLE_SLIM"),
        ("SLIM_THICK", "SOLID"),
        ("THICK_SLIM", "SOLID"),
        ("SLIM_THICK_SLIM", "DOUBLE_SLIM"),
        ("WAVE", "WAVE"),
        ("DOUBLE_WAVE", "DOUBLE_WAVE"),
    ];
    let marks = [
        ("NONE", "NONE"),
        ("DOT_ABOVE", "DOT_ABOVE"),
        ("RING_ABOVE", "RING_ABOVE"),
        ("TILDE", "TILDE"),
        ("CARON", "TILDE"),
        ("SIDE", "SIDE"),
        ("COLON", "DOT_ABOVE"),
    ];
    let base = encode(&underlined(0, None, 0), DocumentFormat::Hwpx);
    for (i, (shape, written)) in shapes.iter().enumerate() {
        let (mark, mark_written) = marks[i % marks.len()];
        let source = with_header(&base, |h| {
            h.replace(
                "shape=\"SOLID\" color=\"#000000\"/><hh:strikeout",
                &format!("shape=\"{shape}\" color=\"#336699\"/><hh:strikeout"),
            )
            .replace("symMark=\"NONE\"", &format!("symMark=\"{mark}\""))
        });
        let doc = decode(&source, "a.hwpx");
        assert_eq!(
            the_run(&doc, "x").advanced.fx.underline_color,
            Some(0x33_6699),
            "{shape}"
        );
        let head = header(&encode(&doc, DocumentFormat::Hwpx));
        assert_eq!(
            underline_element(&head),
            format!("<hh:underline type=\"BOTTOM\" shape=\"{written}\" color=\"#336699\"/>"),
            "{shape}"
        );
        assert!(
            head.contains(&format!("symMark=\"{mark_written}\"")),
            "{mark}: {head}"
        );
    }
    // Every mark of the table is read, also next to a plain underline.
    for (mark, written) in marks {
        let source = with_header(&base, |h| {
            h.replace("symMark=\"NONE\"", &format!("symMark=\"{mark}\""))
        });
        let head = header(&encode(&decode(&source, "a.hwpx"), DocumentFormat::Hwpx));
        assert!(head.contains(&format!("symMark=\"{written}\"")), "{mark}");
    }
}

#[test]
fn hostile_underline_and_mark_values_read_as_not_set() {
    let base = encode(&underlined(14, Some(0xFF_0000), 1), DocumentFormat::Hwpx);
    let long = "A".repeat(100_000);
    let read = |underline: &str, sym_mark: &str| {
        let source = with_header(&base, |h| {
            let start = h.find("<hh:underline type=\"BOTTOM\"").unwrap();
            let end = start + h[start..].find("/>").unwrap() + 2;
            format!("{}{underline}{}", &h[..start], &h[end..])
                .replace("symMark=\"DOT_ABOVE\"", &format!("symMark=\"{sym_mark}\""))
        });
        let run = the_run(&decode(&source, "h.hwpx"), "x");
        (run.underline, run.advanced.fx)
    };
    let line = |kind: &str, shape: &str, color: &str| {
        format!("<hh:underline type=\"{kind}\" shape=\"{shape}\" color=\"{color}\"/>")
    };
    let off = (false, TextFx::default());
    // An unknown or middle-line type is no underline (the reference parser reads only BOTTOM and TOP).
    for kind in ["NONE", "CENTER", "BOGUS", "bottom", "", long.as_str()] {
        assert_eq!(
            read(&line(kind, "WAVE", "#FF0000"), "NONE"),
            off,
            "{kind:.12}"
        );
    }
    // An unknown shape is a single line, and a colour that is not six hex digits is automatic.
    for shape in ["BOGUS", "solid", "", "3D", long.as_str()] {
        let (on, fx) = read(&line("BOTTOM", shape, "#FF0000"), "NONE");
        assert!(on, "{shape:.12}");
        assert_eq!(fx.underline_style, 0, "{shape:.12}");
    }
    for color in [
        "#FFFFFFFFFF",
        "-1",
        "-#FF0000",
        "#GGGGGG",
        "#12345",
        "#1234567",
        "none",
        "",
        "#80FF0000",
        "0x336699",
        "#FF 000",
        "4294967296",
        "-4294967295",
        long.as_str(),
    ] {
        let (on, fx) = read(&line("BOTTOM", "SOLID", color), "NONE");
        assert!(on, "{color:.12}");
        assert_eq!(fx.underline_color, None, "{color:.12}");
    }
    let (_, fx) = read(&line("BOTTOM", "SOLID", "ff8000"), "NONE");
    assert_eq!(
        fx.underline_color,
        Some(0xFF_8000),
        "no # and lower case are read"
    );
    // The top line is read as an underline (the model has no line above the text); an unknown mark is none.
    assert!(read(&line("TOP", "SOLID", "#000000"), "NONE").0);
    for mark in ["BOGUS", "dot_above", "", "7", "-1", long.as_str()] {
        assert_eq!(
            read(&line("NONE", "SOLID", "#000000"), mark),
            off,
            "{mark:.12}"
        );
    }
}

#[test]
fn many_charprs_with_hostile_values_stay_inside_the_budget() {
    let base = encode(&underlined(0, None, 0), DocumentFormat::Hwpx);
    let source = with_header(&base, |h| {
        let one = "<hh:charPr id=\"1\" height=\"1000\" textColor=\"#000000\" shadeColor=\"none\" useFontSpace=\"0\" useKerning=\"0\" symMark=\"BOGUS\" borderFillIDRef=\"1\"><hh:underline type=\"BOTTOM\" shape=\"BOGUS\" color=\"#-1\"/></hh:charPr>";
        h.replacen(
            "</hh:charProperties>",
            &format!("{}</hh:charProperties>", one.repeat(5_000)),
            1,
        )
    });
    let doc = decode(&source, "a.hwpx");
    assert_eq!(the_run(&doc, "x").text, "x");
}
