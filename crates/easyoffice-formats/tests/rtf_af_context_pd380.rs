//! PD-380: the RTF reader takes `\af` by the text type switch in force (RTF 1.9.1 `\loch`, `\hich`, `\dbch`, `\ltrch`, `\rtlch`):
//! only `\dbch` makes it the East Asian face, the complex script face of `\rtlch` and the high ANSI face of `\hich` are not
//! it, and a bare `\af` stays the associated face of the run as before. The writer names the context.
//!
//! The documents are written after the control word patterns the word processors use (a Word run is
//! `{\rtlch\fcs1 \af<complex> \ltrch\fcs0 \hich\af<latin>\dbch\af<east>\loch\f<latin> text}`), not copied from a file.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter,
};

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("expected document: {other:?}"),
    }
}

fn fonts_of(doc: &RichDocument, text: &str) -> (Option<String>, Option<String>) {
    let run = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter())
        .find(|r| r.text == text)
        .unwrap_or_else(|| panic!("run {text}"));
    (run.font.clone(), run.font_east_asia.clone())
}

/// The Latin and the East Asian font of a run, comparable with a pair of names.
#[derive(Debug)]
struct Fonts(Option<String>, Option<String>);

impl PartialEq<(Option<&str>, Option<&str>)> for Fonts {
    fn eq(&self, other: &(Option<&str>, Option<&str>)) -> bool {
        self.0.as_deref() == other.0 && self.1.as_deref() == other.1
    }
}

fn fonts(rtf: &str, text: &str) -> Fonts {
    let (font, east) = fonts_of(&decode(rtf.as_bytes(), "a.rtf"), text);
    Fonts(font, east)
}

/// The head of a Word document of an English install: no East Asian text type anywhere, `\af` only in the `\rtlch` part.
const WORD_EN_HEAD: &str = r"{\rtf1\adeflang1025\ansi\ansicpg1252\uc1\adeff31507\deff0\stshfdbch31505\stshfloch31506\stshfhich31506\stshfbi31507\deflang1033\deflangfe1033{\fonttbl{\f0\fbidi \froman\fcharset0\fprq2 Times New Roman;}{\f31505\fbidi \fnil\fcharset129\fprq2 Malgun Gothic;}{\f31506\fbidi \fswiss\fcharset0\fprq2 Calibri;}{\f31507\fbidi \froman\fcharset0\fprq2 Times New Roman;}}{\colortbl;\red0\green0\blue0;}\pard\plain \ltrpar\ql \li0\ri0\widctlpar\wrapdefault\aspalpha\aspnum\faauto\adjustright\rin0\lin0\itap0 \rtlch\fcs1 \af31507\afs22\alang1025 \ltrch\fcs0 \f31506\fs22\lang1033\langfe1033\cgrid\langnp1033\langfenp1033 ";

/// The head of a Word document of a Korean install: the paragraph defaults name the double byte font with `\dbch\af`.
const WORD_KO_HEAD: &str = r"{\rtf1\adeflang1025\ansi\ansicpg949\uc2\adeff31507\deff0\stshfdbch31505\stshfloch31506\stshfhich31506\stshfbi31507\deflang1033\deflangfe1042{\fonttbl{\f0\fbidi \froman\fcharset0\fprq2 Times New Roman;}{\f31505\fbidi \fnil\fcharset129\fprq2 Malgun Gothic;}{\f31506\fbidi \fswiss\fcharset0\fprq2 Calibri;}{\f31507\fbidi \froman\fcharset0\fprq2 Times New Roman;}}\pard\plain \ltrpar\ql \li0\ri0\widctlpar\rin0\lin0\itap0 \rtlch\fcs1 \af31507\afs22\alang1025 \ltrch\fcs0 \f31506\fs22\lang1033\langfe1042\loch\af31506\hich\af31506\dbch\af31505\cgrid ";

#[test]
fn an_english_word_document_has_no_east_asian_font_from_the_complex_script_face() {
    // Before PD-380 every run here read `Times New Roman` (the `\rtlch\af`) as its East Asian font.
    let rtf = format!(
        "{WORD_EN_HEAD}{{\\rtlch\\fcs1 \\af31507 \\ltrch\\fcs0 \\insrsid1 Hello}}{{\\rtlch\\fcs1 \\af31507 \\ltrch\\fcs0 \\insrsid1 \\f0 Plain}}\\par }}"
    );
    assert_eq!(fonts(&rtf, "Hello"), (Some("Calibri"), None));
    // `\f0` is the default face (`\deff0`), which is no font next to no East Asian font.
    assert_eq!(fonts(&rtf, "Plain"), (None, None));
}

#[test]
fn a_korean_word_document_reads_the_double_byte_face_as_the_east_asian_font() {
    let rtf = format!(
        "{WORD_KO_HEAD}{{\\rtlch\\fcs1 \\af31507 \\ltrch\\fcs0 \\insrsid1 \\hich\\af31506\\dbch\\af31505\\loch\\f31506 Mixed}}\\par }}"
    );
    assert_eq!(
        fonts(&rtf, "Mixed"),
        (Some("Calibri"), Some("Malgun Gothic"))
    );
    // A run that switches the text type back and forth keeps the face of each type: `\hich\af` is not the East Asian one.
    let rtf = format!(
        "{WORD_KO_HEAD}{{\\rtlch\\fcs1 \\af31507 \\ltrch\\fcs0 \\hich\\af31507\\dbch\\af0\\loch\\f31506 Swap}}\\par }}"
    );
    assert_eq!(
        fonts(&rtf, "Swap"),
        (Some("Calibri"), Some("Times New Roman"))
    );
}

#[test]
fn an_open_source_writer_style_document_reads_the_same_way() {
    let rtf = r"{\rtf1\ansi\deff4\adeflang1025{\fonttbl{\f0\froman\fprq2\fcharset0 Times New Roman;}{\f4\froman\fprq2\fcharset0 Liberation Serif{\*\falt Times New Roman};}{\f8\fnil\fprq2\fcharset129 Noto Serif CJK KR;}{\f9\fnil\fprq2\fcharset0 Noto Sans Devanagari;}}\pard\plain\ltrpar\ql\li0\ri0\sa0\sb0\ltrch\dbch\af8\langfe1042\dbch\af8\afs24\alang1081\loch\f4\hich\af4\fs24\lang1033 {\rtlch \ltrch\loch\f4\hich\af4\dbch\af8 Hello}{\rtlch\af9 \ltrch\loch\f4\fs24\b Latin}\par}";
    assert_eq!(
        fonts(rtf, "Hello"),
        (Some("Liberation Serif"), Some("Noto Serif CJK KR"))
    );
    // The second run inherits the paragraph's `\dbch\af8`; its own `\rtlch\af9` is the complex script face and changes nothing.
    assert_eq!(
        fonts(rtf, "Latin"),
        (Some("Liberation Serif"), Some("Noto Serif CJK KR"))
    );
}

fn plain(body: &str) -> String {
    format!(
        "{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Arial;}}{{\\f1 Batang;}}{{\\f2 Gulim;}}}}\\pard {body}\\par}}"
    )
}

#[test]
fn only_the_double_byte_context_sets_the_east_asian_font() {
    for (body, expected) in [
        ("\\f1\\dbch\\af2 x", (Some("Batang"), Some("Gulim"))),
        ("\\f1\\dbch\\af2\\loch x", (Some("Batang"), Some("Gulim"))),
        // The complex script face of `\rtlch` and the high ANSI face of `\hich` are no East Asian font.
        ("\\f1\\rtlch\\af2 x", (Some("Batang"), None)),
        (
            "\\f1\\rtlch\\fcs1 \\af2 \\ltrch\\fcs0 x",
            (Some("Batang"), None),
        ),
        ("\\f1\\hich\\af2 x", (Some("Batang"), None)),
        ("\\f1\\rtlch\\dbch\\af2 x", (Some("Batang"), None)),
        // `\ltrch` ends the right-to-left part, `\loch` the double byte part.
        (
            "\\f1\\rtlch\\ltrch\\dbch\\af2 x",
            (Some("Batang"), Some("Gulim")),
        ),
        (
            "\\f1\\dbch\\af2\\hich\\af0\\loch x",
            (Some("Batang"), Some("Gulim")),
        ),
        // The first `\af` is the associated face of a bare run (below), the one under `\dbch` is the East Asian font.
        ("\\f1\\af0\\dbch\\af2 x", (Some("Batang"), Some("Gulim"))),
        (
            "\\f1\\dbch\\af2\\loch\\af0 x",
            (Some("Batang"), Some("Gulim")),
        ),
    ] {
        assert_eq!(fonts(&plain(body), "x"), expected, "{body}");
    }
}

#[test]
fn a_bare_af_stays_the_associated_east_asian_face_of_the_run() {
    // What PD-376 wrote and read: `\f<latin> \af<east>` with no text type switch. The default face next to another is a name.
    for (body, expected) in [
        ("\\f1\\af0 x", (Some("Batang"), Some("Arial"))),
        ("\\f0\\af1 x", (Some("Arial"), Some("Batang"))),
        ("\\f0\\af0 x", (None, None)),
        ("\\f1\\af1 x", (Some("Batang"), None)),
        // Only one face named: it is the font of the run, as for every format (PD-376).
        ("\\af2 x", (Some("Gulim"), None)),
    ] {
        assert_eq!(fonts(&plain(body), "x"), expected, "{body}");
    }
}

#[test]
fn the_text_type_follows_the_group_and_plain_resets_it() {
    let rtf = plain("{\\f1\\dbch {\\af2 a}\\af0 b}");
    // `a` is inside the group that switched to `\dbch`; `\af0` after it is the East Asian face of `b`.
    assert_eq!(fonts(&rtf, "a"), (Some("Batang"), Some("Gulim")));
    assert_eq!(fonts(&rtf, "b"), (Some("Batang"), Some("Arial")));
    // After the group closes the switch and the faces of the group are gone.
    assert_eq!(fonts(&plain("{\\dbch\\af1 a} c"), " c"), (None, None));
    // A `\af` after the group is a bare one, not the double byte one of the group that closed.
    assert_eq!(
        fonts(&plain("{\\dbch\\af1 a}\\af2 c"), "c"),
        (Some("Gulim"), None)
    );
    let reset = plain("\\f1\\dbch\\af2 \\plain\\f0\\af1 x");
    assert_eq!(fonts(&reset, "x"), (Some("Arial"), Some("Batang")));
}

#[test]
fn hostile_af_words_read_as_not_set() {
    let long_ids = "\\dbch\\af99999999999999999999 x";
    for (body, expected) in [
        (long_ids, (None, None)),
        ("\\f1\\dbch\\af99 x", (Some("Batang"), None)),
        ("\\f1\\dbch\\af-5 x", (Some("Batang"), None)),
        ("\\f1\\dbch\\af x", (Some("Batang"), None)),
        ("\\f1\\dbch2\\af2 x", (Some("Batang"), Some("Gulim"))),
        ("\\f1\\rtlch999\\dbch\\af2 x", (Some("Batang"), None)),
    ] {
        assert_eq!(fonts(&plain(body), "x"), expected, "{body}");
    }
    // A hundred thousand switches and faces in one document neither fail nor change the result.
    let storm = format!(
        "\\f1{}\\dbch\\af2 x",
        "\\rtlch\\af0\\ltrch\\hich\\af0\\loch".repeat(100_000)
    );
    assert_eq!(fonts(&plain(&storm), "x"), (Some("Batang"), Some("Gulim")));
    // Nesting beyond the group limit is refused as before and not read as a font.
    let deep = format!(
        "{}\\dbch\\af2 x{}",
        "{".repeat(100_000),
        "}".repeat(100_000)
    );
    assert!(UniversalFormatConverter::decode_rich(plain(&deep).as_bytes(), Some("a.rtf")).is_err());
}

fn east_only(text: &str, font: Option<&str>, east: &str) -> RichDocument {
    RichDocument {
        title: "글꼴".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun {
                text: text.to_string(),
                font: font.map(str::to_string),
                font_east_asia: Some(east.to_string()),
                ..RichRun::default()
            }],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    }
}

#[test]
fn the_writer_names_the_double_byte_context_and_the_pair_survives_two_saves() {
    for (font, east) in [
        (Some("Calibri"), "맑은 고딕"),
        (None, "바탕"),
        (Some("Arial"), "함초롬바탕"),
    ] {
        let doc = east_only("둘", font, east);
        let first = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).unwrap();
        let rtf = String::from_utf8(first.clone()).unwrap();
        assert!(rtf.contains("\\dbch\\af"), "{rtf}");
        // The run goes back to the low ANSI type, so text after it in the group is not read as double byte.
        assert!(rtf.contains("\\loch"), "{rtf}");
        let once = decode(&first, "a.rtf");
        // A face for the East Asian text alone reads back as the one font of the run (PD-376).
        let want = match font {
            Some(font) => (Some(font.to_string()), Some(east.to_string())),
            None => (Some(east.to_string()), None),
        };
        assert_eq!(fonts_of(&once, "둘"), want);
        let second =
            UniversalFormatConverter::encode_rich(&once, DocumentFormat::Rtf, None).unwrap();
        if font.is_some() {
            assert_eq!(first, second, "the second save writes the same bytes");
        }
        let third = UniversalFormatConverter::encode_rich(
            &decode(&second, "a.rtf"),
            DocumentFormat::Rtf,
            None,
        )
        .unwrap();
        assert_eq!(
            second, third,
            "from the second save on the bytes are stable"
        );
        assert_eq!(fonts_of(&decode(&third, "a.rtf"), "둘"), want);
    }
}

#[test]
fn a_document_with_one_font_writes_and_reads_the_same_font_words() {
    let doc = RichDocument {
        title: "글꼴".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun {
                text: "하나".to_string(),
                font: Some("Arial".to_string()),
                ..RichRun::default()
            }],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    let first = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).unwrap();
    let rtf = String::from_utf8(first.clone()).unwrap();
    assert!(!rtf.contains("\\af") && !rtf.contains("\\dbch"), "{rtf}");
    let once = decode(&first, "a.rtf");
    assert_eq!(fonts_of(&once, "하나"), (Some("Arial".into()), None));
    let second = UniversalFormatConverter::encode_rich(&once, DocumentFormat::Rtf, None).unwrap();
    assert_eq!(first, second);
}
