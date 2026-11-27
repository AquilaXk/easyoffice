//! PD-375: a run in a table cell that carries only a character format the cell gate did not know (East Asian font, letter
//! spacing, scale, position, kerning, text effects, OpenType features, grid switch) is kept on save. Before, the whole cell
//! paragraph entry was dropped when nothing else in the cell was formatted.

use easyoffice_formats::{
    CharAdvanced, DecodedContent, DocumentFormat, OpenType, RichBlock, RichCellParagraph,
    RichDocument, RichRun, RichTable, TableCellParagraphs, TextFx, UniversalFormatConverter,
};

type Probe = (&'static str, fn(&mut RichRun), fn(&RichRun) -> bool, bool);

fn advanced(run: &mut RichRun, f: impl FnOnce(&mut CharAdvanced)) {
    f(&mut run.advanced);
}

fn probes() -> Vec<Probe> {
    vec![
        (
            "fontEastAsia",
            |r| r.font_east_asia = Some("바탕".into()),
            // A face for the East Asian text alone reads back as the run's one font.
            |r| r.font.as_deref() == Some("바탕") || r.font_east_asia.as_deref() == Some("바탕"),
            true,
        ),
        (
            "letterSpacing",
            |r| r.letter_spacing_twips = Some(40),
            |r| r.letter_spacing_twips == Some(40),
            true,
        ),
        (
            "scale",
            |r| advanced(r, |a| a.scale_percent = Some(150)),
            |r| r.advanced.scale_percent == Some(150),
            true,
        ),
        (
            "position",
            |r| advanced(r, |a| a.position_half_pt = Some(6)),
            |r| r.advanced.position_half_pt == Some(6),
            true,
        ),
        (
            "kerning",
            |r| advanced(r, |a| a.kerning_half_pt = Some(24)),
            |r| r.advanced.kerning_half_pt.is_some(),
            true,
        ),
        (
            "smallCaps",
            |r| {
                advanced(r, |a| {
                    a.fx = TextFx {
                        small_caps: true,
                        ..TextFx::default()
                    }
                })
            },
            |r| r.advanced.fx.small_caps,
            true,
        ),
        (
            "hidden",
            |r| {
                advanced(r, |a| {
                    a.fx = TextFx {
                        hidden: true,
                        ..TextFx::default()
                    }
                })
            },
            |r| r.advanced.fx.hidden,
            true,
        ),
        (
            "doubleStrike",
            |r| {
                advanced(r, |a| {
                    a.fx = TextFx {
                        double_strike: true,
                        ..TextFx::default()
                    }
                })
            },
            |r| r.advanced.fx.double_strike,
            true,
        ),
        (
            "openType",
            |r| {
                advanced(r, |a| {
                    a.open_type = OpenType {
                        ligatures: 1,
                        ..OpenType::default()
                    }
                })
            },
            |r| r.advanced.open_type.ligatures == 1,
            false,
        ),
        (
            "gridSnapOff",
            |r| advanced(r, |a| a.grid_snap_off = true),
            |r| r.advanced.grid_snap_off,
            false,
        ),
    ]
}

fn document(run: RichRun) -> RichDocument {
    let mut table = RichTable::new(vec![vec!["ab".to_string()]]);
    table.cell_paragraphs = vec![TableCellParagraphs {
        row: 0,
        col: 0,
        paragraphs: vec![RichCellParagraph {
            runs: vec![run],
            ..RichCellParagraph::default()
        }],
    }];
    RichDocument {
        title: "표".to_string(),
        blocks: vec![RichBlock::Table(table)],
        ..RichDocument::default()
    }
}

fn reopened_cell_runs(bytes: &[u8], name: &str) -> Vec<RichRun> {
    let doc = match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("expected document: {other:?}"),
    };
    doc.blocks
        .iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .expect("table")
        .cell_paragraphs
        .iter()
        .flat_map(|c| c.paragraphs.iter())
        .flat_map(|p| p.runs.iter().cloned())
        .collect()
}

#[test]
fn a_cell_run_with_only_one_of_these_formats_keeps_it_in_docx_and_odt() {
    for (what, set, kept, odt) in probes() {
        let mut run = RichRun {
            text: "ab".to_string(),
            ..RichRun::default()
        };
        set(&mut run);
        let doc = document(run);
        let mut formats = vec![(DocumentFormat::Docx, "c.docx")];
        if odt {
            formats.push((DocumentFormat::Odt, "c.odt"));
        }
        for (fmt, name) in formats {
            let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).unwrap();
            let runs = reopened_cell_runs(&bytes, name);
            assert!(runs.iter().any(kept), "{what} lost in {name}: {runs:?}");
        }
    }
}
