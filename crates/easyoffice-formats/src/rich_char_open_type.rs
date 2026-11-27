//! OpenType features of a run (ligatures, number spacing, number forms, stylistic sets, contextual alternates), the
//! 고급 tab's "OpenType 기능" group of the word processor's Font dialog (PD-373).
//!
//! Reference implementation: the Open XML SDK (https://github.com/dotnet/Open-XML-SDK, MIT, Copyright (c) .NET Foundation and
//! Contributors), `data/schemas/schemas_microsoft_com_office_word_2010_wordml.json`: the values of `w14:ST_Ligatures` (the 16
//! names, composed of `standard`, `contextual`, `historical` and `discretional` in that order, plus `none` and `all`),
//! `w14:ST_NumSpacing` (`default`, `proportional`, `tabular`) and `w14:ST_NumForm` (`default`, `lining`, `oldStyle`), and the
//! elements `w14:ligatures`, `w14:numSpacing`, `w14:numForm`, `w14:stylisticSets` > `w14:styleSet` (`w14:id`, `w14:val`) and
//! `w14:cntxtAlts`. The names are read as a bit set (the four ligature words are four bits) instead of a table of 16 strings,
//! and every value is clamped: an unknown name, a style set outside 1-20 or an enumeration outside its range reads as not set.
//!
//! Only DOCX carries these. ODT, RTF, HWPX and Pages have no counterpart, so a save to them drops the features (the Pages
//! write warns, see `pages_write_loss`).

use crate::xml_stream::{attr, Attrs};

/// Largest stylistic set number (the Font dialog lists 1 to 20).
pub const MAX_STYLE_SET: u32 = 20;
/// Largest packed value: 4 ligature bits, 2 + 2 bits of number spacing and form, 1 bit of contextual alternates and 20 set bits.
pub const MAX_PACKED_OPEN_TYPE: u32 = (1 << 29) - 1;

const LIGATURE_ALL: u8 = 0b1111;
/// `(lowercase, capitalized, bit)` of the ligature words, in the order a `w14:ST_Ligatures` name lists them.
const LIGATURE_WORDS: [(&str, &str, u8); 4] = [
    ("standard", "Standard", 1),
    ("contextual", "Contextual", 2),
    ("historical", "Historical", 4),
    ("discretional", "Discretional", 8),
];
const NUM_SPACING_NAMES: [&str; 3] = ["default", "proportional", "tabular"];
const NUM_FORM_NAMES: [&str; 3] = ["default", "lining", "oldStyle"];
const NUM_SPACING_SHIFT: u32 = 4;
const NUM_FORM_SHIFT: u32 = 6;
const CONTEXTUAL_SHIFT: u32 = 8;
const STYLE_SETS_SHIFT: u32 = 9;

/// The features of one run; every field is zero (or false) when the run inherits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OpenType {
    /// Bit set of the ligature kinds: standard 1, contextual 2, historical 4, discretional 8 (15 is `all`).
    pub ligatures: u8,
    /// 0 default, 1 proportional, 2 tabular.
    pub num_spacing: u8,
    /// 0 default, 1 lining, 2 old style.
    pub num_form: u8,
    /// Bit `n - 1` is stylistic set `n` (1 to 20).
    pub style_sets: u32,
    pub contextual_alts: bool,
}

/// The `w14:ST_Ligatures` name of a ligature bit set.
fn ligature_name(mask: u8) -> String {
    match mask {
        0 => "none".to_string(),
        LIGATURE_ALL => "all".to_string(),
        _ => {
            let mut name = String::new();
            for (lower, upper, bit) in LIGATURE_WORDS {
                if mask & bit != 0 {
                    name.push_str(if name.is_empty() { lower } else { upper });
                }
            }
            name
        }
    }
}

/// The bit set of a `w14:ST_Ligatures` name; `None` for a name that is not one of the 16.
fn ligatures_from_name(name: &str) -> Option<u8> {
    match name.trim() {
        "none" => return Some(0),
        "all" => return Some(LIGATURE_ALL),
        _ => {}
    }
    let mut rest = name.trim();
    let mut mask = 0u8;
    for (lower, upper, bit) in LIGATURE_WORDS {
        let word = if mask == 0 { lower } else { upper };
        if let Some(after) = rest.strip_prefix(word) {
            mask |= bit;
            rest = after;
        }
    }
    (mask != 0 && rest.is_empty()).then_some(mask)
}

fn index_of(names: &[&str], name: Option<&str>) -> u8 {
    name.and_then(|n| names.iter().position(|k| *k == n.trim()))
        .map_or(0, |i| i as u8)
}

impl OpenType {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// The run's features as one integer (`MAX_PACKED_OPEN_TYPE` at most), the form the editor and the save JSON carry.
    pub fn pack(&self) -> u32 {
        u32::from(self.ligatures)
            | u32::from(self.num_spacing) << NUM_SPACING_SHIFT
            | u32::from(self.num_form) << NUM_FORM_SHIFT
            | u32::from(self.contextual_alts) << CONTEXTUAL_SHIFT
            | (self.style_sets & ((1 << MAX_STYLE_SET) - 1)) << STYLE_SETS_SHIFT
    }

    /// The features of a packed integer; `None` when a field is outside its range (number spacing and form are 0 to 2).
    pub fn unpack(packed: u32) -> Option<Self> {
        let num_spacing = (packed >> NUM_SPACING_SHIFT & 0b11) as u8;
        let num_form = (packed >> NUM_FORM_SHIFT & 0b11) as u8;
        (packed <= MAX_PACKED_OPEN_TYPE && num_spacing <= 2 && num_form <= 2).then_some(Self {
            ligatures: (packed & u32::from(LIGATURE_ALL)) as u8,
            num_spacing,
            num_form,
            contextual_alts: packed >> CONTEXTUAL_SHIFT & 1 == 1,
            style_sets: packed >> STYLE_SETS_SHIFT,
        })
    }

    /// The DOCX run properties, in the order Word writes them (`w14:ligatures`, `w14:numForm`, `w14:numSpacing`,
    /// `w14:stylisticSets`, `w14:cntxtAlts`); these go last in `w:rPr`.
    pub(crate) fn docx_xml(&self) -> String {
        let mut xml = String::new();
        if self.ligatures != 0 {
            xml.push_str(&format!(
                "<w14:ligatures w14:val=\"{}\"/>",
                ligature_name(self.ligatures)
            ));
        }
        if self.num_form != 0 {
            xml.push_str(&format!(
                "<w14:numForm w14:val=\"{}\"/>",
                NUM_FORM_NAMES[usize::from(self.num_form)]
            ));
        }
        if self.num_spacing != 0 {
            xml.push_str(&format!(
                "<w14:numSpacing w14:val=\"{}\"/>",
                NUM_SPACING_NAMES[usize::from(self.num_spacing)]
            ));
        }
        if self.style_sets != 0 {
            xml.push_str("<w14:stylisticSets>");
            for id in (1..=MAX_STYLE_SET).filter(|id| self.style_sets >> (id - 1) & 1 == 1) {
                xml.push_str(&format!("<w14:styleSet w14:id=\"{id}\"/>"));
            }
            xml.push_str("</w14:stylisticSets>");
        }
        if self.contextual_alts {
            xml.push_str("<w14:cntxtAlts/>");
        }
        xml
    }

    /// Applies a DOCX run property element (`w14:ligatures`, `w14:numSpacing`, `w14:numForm`, `w14:styleSet`,
    /// `w14:cntxtAlts`); any other is ignored.
    pub(crate) fn apply_docx_property(&mut self, name: &str, attrs: &Attrs<'_>) {
        let val = attr(attrs, "w14:val");
        let val = val.as_deref();
        match name {
            "w14:ligatures" => self.ligatures = val.and_then(ligatures_from_name).unwrap_or(0),
            "w14:numSpacing" => self.num_spacing = index_of(&NUM_SPACING_NAMES, val),
            "w14:numForm" => self.num_form = index_of(&NUM_FORM_NAMES, val),
            "w14:cntxtAlts" => self.contextual_alts = crate::rich_xml::toggle_on(val),
            "w14:styleSet" => {
                let id = attr(attrs, "w14:id").and_then(|v| v.trim().parse::<u32>().ok());
                if let Some(id) = id.filter(|id| (1..=MAX_STYLE_SET).contains(id)) {
                    let bit = 1 << (id - 1);
                    if crate::rich_xml::toggle_on(val) {
                        self.style_sets |= bit;
                    } else {
                        self.style_sets &= !bit;
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_NAMES: [&str; 16] = [
        "none",
        "standard",
        "contextual",
        "historical",
        "discretional",
        "standardContextual",
        "standardHistorical",
        "contextualHistorical",
        "standardDiscretional",
        "contextualDiscretional",
        "historicalDiscretional",
        "standardContextualHistorical",
        "standardContextualDiscretional",
        "standardHistoricalDiscretional",
        "contextualHistoricalDiscretional",
        "all",
    ];

    #[test]
    fn the_sixteen_ligature_names_round_trip_through_the_bit_set() {
        let masks: Vec<u8> = ALL_NAMES
            .iter()
            .map(|n| ligatures_from_name(n).unwrap_or_else(|| panic!("{n}")))
            .collect();
        let mut sorted = masks.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, (0..16).collect::<Vec<u8>>(), "every bit set once");
        for (name, mask) in ALL_NAMES.iter().zip(masks) {
            assert_eq!(ligature_name(mask), *name);
        }
    }

    #[test]
    fn a_name_that_is_not_in_the_schema_reads_as_none() {
        for bad in [
            "",
            "Standard",
            "standardstandard",
            "contextualStandard",
            "standardX",
            "ALL",
            "standard contextual",
        ] {
            assert_eq!(ligatures_from_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn pack_and_unpack_round_trip_and_refuse_out_of_range_fields() {
        let ot = OpenType {
            ligatures: 0b0101,
            num_spacing: 2,
            num_form: 1,
            style_sets: 1 | 1 << 19,
            contextual_alts: true,
        };
        assert_eq!(OpenType::unpack(ot.pack()), Some(ot));
        assert_eq!(OpenType::unpack(0), Some(OpenType::default()));
        assert_eq!(OpenType::unpack(MAX_PACKED_OPEN_TYPE + 1), None);
        assert_eq!(OpenType::unpack(u32::MAX), None);
        assert_eq!(OpenType::unpack(3 << NUM_SPACING_SHIFT), None);
        assert_eq!(OpenType::unpack(3 << NUM_FORM_SHIFT), None);
    }

    #[test]
    fn docx_xml_lists_the_features_in_word_order() {
        let ot = OpenType {
            ligatures: 0b0011,
            num_spacing: 2,
            num_form: 2,
            style_sets: 0b101,
            contextual_alts: true,
        };
        assert_eq!(
            ot.docx_xml(),
            "<w14:ligatures w14:val=\"standardContextual\"/><w14:numForm w14:val=\"oldStyle\"/>\
             <w14:numSpacing w14:val=\"tabular\"/><w14:stylisticSets><w14:styleSet w14:id=\"1\"/>\
             <w14:styleSet w14:id=\"3\"/></w14:stylisticSets><w14:cntxtAlts/>"
        );
        assert_eq!(OpenType::default().docx_xml(), "");
    }
}
