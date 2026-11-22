//! Workbook calculation options (`CalcMode`, `CalcOptions`) shared across XLSX, BIFF8 XLS, and ODS
//! (PD-309, ECMA-376 §18.2.2 `calcPr`, OASIS ODF 1.2 §9.4.6-9.4.9, and MS-XLS calculation records).
//!
//! 참조 구현: Apache POI (Apache-2.0, Copyright The Apache Software Foundation, commit
//! `47dcff769bb395034dddfd59c4609659b6dbc6a4`):
//! `poi/src/main/java/org/apache/poi/hssf/record/CalcCountRecord.java` (`0x000C` `cIter`),
//! `CalcModeRecord.java` (`0x000D` `fAutoRecalc`: `0` manual, `1` auto, `-1` autoNoTable),
//! `DeltaRecord.java` (`0x0010` `numDelta` IEEE-754 `f64`),
//! `IterationRecord.java` (`0x0011` `fIter`), and
//! `SaveRecalcRecord.java` (`0x005F` `fSaveRecalc`).
//! See `THIRD_PARTY_NOTICES.md`.
//!
//! Modifications: rewritten in safe Rust with fail-closed validation on untrusted inputs
//! (`iterate_count` in `1..=32_767`, finite `iterate_delta` in `0.0..=1e100`), preserving existing
//! `fullCalcOnLoad="1"` on default/automatic XLSX workbooks so formula error/timeout cells still
//! recompute on open.

use std::fmt::Write;

use crate::ole2_codec::{read_u16_le, read_u64_le};
use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_tags_or_self_closing_with_attrs};

/// Upper bound on MS-XLS `CALCCOUNT` / OOXML `iterateCount` (MS-XLS §2.4.32, UI max 32,767).
pub const MAX_ITERATE_COUNT: u16 = 32_767;
/// Default maximum iterations matching the Korean reference spreadsheet edition.
pub const DEFAULT_ITERATE_COUNT: u16 = 100;
/// Upper bound on `iterateDelta` (`<= 1e100`).
pub const MAX_ITERATE_DELTA: f64 = 1e100;
/// Default maximum change matching the Korean reference spreadsheet edition.
pub const DEFAULT_ITERATE_DELTA: f64 = 0.001;

pub(crate) const BIFF8_CALCCOUNT: u16 = 0x000C;
pub(crate) const BIFF8_CALCMODE: u16 = 0x000D;
pub(crate) const BIFF8_DELTA: u16 = 0x0010;
pub(crate) const BIFF8_ITERATION: u16 = 0x0011;
pub(crate) const BIFF8_SAVERECALC: u16 = 0x005F;

const OOO_CONFIG_NAME: &str = "ooo:configuration-settings";

/// Workbook formula calculation mode (`수식 > 계산 옵션`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CalcMode {
    /// `자동` (`calcMode="auto"` / `CALCMODE=1`).
    #[default]
    Auto,
    /// `데이터 테이블을 제외한 자동` (`calcMode="autoNoTable"` / `CALCMODE=-1`).
    AutoNoTable,
    /// `수동` (`calcMode="manual"` / `CALCMODE=0`).
    Manual,
}

impl CalcMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::AutoNoTable => "autoNoTable",
            Self::Manual => "manual",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "auto" => Some(Self::Auto),
            "autoNoTable" => Some(Self::AutoNoTable),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// Workbook calculation properties carried on the first [`crate::SheetStyling`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalcOptions {
    pub mode: CalcMode,
    pub calc_on_save: bool,
    pub full_calc_on_load: bool,
    pub iterate: bool,
    pub iterate_count: u16,
    pub iterate_delta: f64,
}

impl Default for CalcOptions {
    fn default() -> Self {
        Self {
            mode: CalcMode::Auto,
            calc_on_save: true,
            full_calc_on_load: false,
            iterate: false,
            iterate_count: DEFAULT_ITERATE_COUNT,
            iterate_delta: DEFAULT_ITERATE_DELTA,
        }
    }
}

impl CalcOptions {
    #[inline]
    pub fn valid_iterate_count(count: u16) -> bool {
        (1..=MAX_ITERATE_COUNT).contains(&count)
    }

    #[inline]
    pub fn valid_iterate_delta(delta: f64) -> bool {
        delta.is_finite() && (0.0..=MAX_ITERATE_DELTA).contains(&delta)
    }

    pub fn within_bounds(&self) -> bool {
        Self::valid_iterate_count(self.iterate_count)
            && Self::valid_iterate_delta(self.iterate_delta)
    }

    #[inline]
    fn delta_is_default(delta: f64) -> bool {
        (delta - DEFAULT_ITERATE_DELTA).abs() <= 1e-15
    }

    pub fn is_default(&self) -> bool {
        self.mode == CalcMode::Auto
            && self.calc_on_save
            && !self.full_calc_on_load
            && !self.iterate
            && self.iterate_count == DEFAULT_ITERATE_COUNT
            && Self::delta_is_default(self.iterate_delta)
    }

    #[inline]
    pub fn has_non_default_iteration(&self) -> bool {
        self.iterate
            || self.iterate_count != DEFAULT_ITERATE_COUNT
            || !Self::delta_is_default(self.iterate_delta)
    }

    #[inline]
    pub fn has_non_default_settings(&self) -> bool {
        self.mode != CalcMode::Auto || !self.calc_on_save || self.full_calc_on_load
    }

    /// Normalizes a delta so `-0.0` becomes `0.0` for clean comparisons and serialization.
    #[inline]
    pub fn sanitized_delta(delta: f64) -> Option<f64> {
        Self::valid_iterate_delta(delta).then_some(if delta == 0.0 { 0.0 } else { delta })
    }
}

fn xml_bool_is_true(raw: &str) -> bool {
    matches!(raw.trim(), "1" | "true")
}

fn xml_bool_is_false(raw: &str) -> bool {
    matches!(raw.trim(), "0" | "false")
}

/// Formats `<calcPr .../>` for `xl/workbook.xml`.
///
/// Default/automatic modes retain `fullCalcOnLoad="1"` so consumers recalculate formula cells that
/// omit cached `<v>` values; manual mode omits `fullCalcOnLoad="1"` unless explicitly requested.
pub(crate) fn xlsx_calc_pr_xml(opts: &CalcOptions) -> String {
    let mut xml = String::from(r#"<calcPr calcId="191029""#);
    if opts.mode != CalcMode::Auto {
        let _ = write!(xml, r#" calcMode="{}""#, opts.mode.as_str());
    }
    if opts.mode != CalcMode::Manual || opts.full_calc_on_load {
        xml.push_str(r#" fullCalcOnLoad="1""#);
    }
    if !opts.calc_on_save {
        xml.push_str(r#" calcOnSave="0""#);
    }
    if opts.iterate {
        xml.push_str(r#" iterate="1""#);
    }
    if opts.iterate_count != DEFAULT_ITERATE_COUNT
        && CalcOptions::valid_iterate_count(opts.iterate_count)
    {
        let _ = write!(xml, r#" iterateCount="{}""#, opts.iterate_count);
    }
    if !CalcOptions::delta_is_default(opts.iterate_delta) {
        if let Some(d) = CalcOptions::sanitized_delta(opts.iterate_delta) {
            let _ = write!(xml, r#" iterateDelta="{d}""#);
        }
    }
    xml.push_str("/>");
    xml
}

/// Parses `<calcPr>` from `xl/workbook.xml` with fail-closed validation on out-of-range attributes.
pub(crate) fn read_xlsx_calc_options(wb_xml: &str) -> CalcOptions {
    let mut opts = CalcOptions::default();
    let Some((attrs, _)) = extract_tags_or_self_closing_with_attrs(wb_xml, "calcPr")
        .into_iter()
        .next()
    else {
        return opts;
    };
    if let Some(mode) = extract_attr_from_fragment(attrs, "calcMode").and_then(CalcMode::parse) {
        opts.mode = mode;
    }
    if let Some(save) = extract_attr_from_fragment(attrs, "calcOnSave") {
        if xml_bool_is_false(save) {
            opts.calc_on_save = false;
        }
    }
    if opts.mode == CalcMode::Manual {
        if let Some(load) = extract_attr_from_fragment(attrs, "fullCalcOnLoad") {
            opts.full_calc_on_load = xml_bool_is_true(load);
        }
    }
    if let Some(iter) = extract_attr_from_fragment(attrs, "iterate") {
        opts.iterate = xml_bool_is_true(iter);
    }
    if let Some(count) = extract_attr_from_fragment(attrs, "iterateCount")
        .and_then(|s| s.trim().parse::<u16>().ok())
        .filter(|&c| CalcOptions::valid_iterate_count(c))
    {
        opts.iterate_count = count;
    }
    if let Some(delta) = extract_attr_from_fragment(attrs, "iterateDelta")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .and_then(CalcOptions::sanitized_delta)
    {
        opts.iterate_delta = delta;
    }
    opts
}

/// Emits the 5 BIFF8 calculation records in MS-XLS worksheet order right after Worksheet `BOF`.
pub(crate) fn write_biff8_calc_records(out: &mut Vec<u8>, opts: &CalcOptions) {
    let count = if CalcOptions::valid_iterate_count(opts.iterate_count) {
        opts.iterate_count
    } else {
        DEFAULT_ITERATE_COUNT
    };
    let delta = CalcOptions::sanitized_delta(opts.iterate_delta).unwrap_or(DEFAULT_ITERATE_DELTA);
    let mode_u16: u16 = match opts.mode {
        CalcMode::Auto => 1,
        CalcMode::Manual => 0,
        CalcMode::AutoNoTable => 0xFFFF,
    };
    push_biff_u16(out, BIFF8_CALCCOUNT, count);
    push_biff_u16(out, BIFF8_CALCMODE, mode_u16);
    push_biff_u16(out, BIFF8_ITERATION, u16::from(opts.iterate));
    out.extend_from_slice(&BIFF8_DELTA.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&delta.to_bits().to_le_bytes());
    push_biff_u16(out, BIFF8_SAVERECALC, u16::from(opts.calc_on_save));
}

#[inline]
fn push_biff_u16(out: &mut Vec<u8>, opcode: u16, value: u16) {
    out.extend_from_slice(&opcode.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&value.to_le_bytes());
}

/// Collects workbook calculation options from Workbook Globals and/or Worksheet 0 substream.
#[derive(Debug, Default)]
pub(crate) struct Biff8CalcReader {
    globals: CalcOptions,
    sheet0: CalcOptions,
    saw_sheet0: [bool; 5],
}

impl Biff8CalcReader {
    fn apply_record(target: &mut CalcOptions, opcode: u16, body: &[u8]) -> Option<usize> {
        match opcode {
            BIFF8_CALCCOUNT => {
                if let Ok(count) = read_u16_le(body, 0) {
                    if CalcOptions::valid_iterate_count(count) {
                        target.iterate_count = count;
                    }
                }
                Some(0)
            }
            BIFF8_CALCMODE => {
                if let Ok(raw) = read_u16_le(body, 0) {
                    target.mode = match raw {
                        0 => CalcMode::Manual,
                        0xFFFF => CalcMode::AutoNoTable,
                        _ => CalcMode::Auto,
                    };
                }
                Some(1)
            }
            BIFF8_ITERATION => {
                if let Ok(flag) = read_u16_le(body, 0) {
                    target.iterate = flag != 0;
                }
                Some(2)
            }
            BIFF8_DELTA => {
                if let Ok(bits) = read_u64_le(body, 0) {
                    if let Some(delta) = CalcOptions::sanitized_delta(f64::from_bits(bits)) {
                        target.iterate_delta = delta;
                    }
                }
                Some(3)
            }
            BIFF8_SAVERECALC => {
                if let Ok(flag) = read_u16_le(body, 0) {
                    target.calc_on_save = flag != 0;
                }
                Some(4)
            }
            _ => None,
        }
    }

    pub(crate) fn global_record(&mut self, opcode: u16, body: &[u8]) {
        Self::apply_record(&mut self.globals, opcode, body);
    }

    pub(crate) fn sheet0_record(&mut self, opcode: u16, body: &[u8]) {
        if let Some(idx) = Self::apply_record(&mut self.sheet0, opcode, body) {
            self.saw_sheet0[idx] = true;
        }
    }

    pub(crate) fn finish(self) -> CalcOptions {
        let mut out = self.globals;
        if self.saw_sheet0[0] {
            out.iterate_count = self.sheet0.iterate_count;
        }
        if self.saw_sheet0[1] {
            out.mode = self.sheet0.mode;
        }
        if self.saw_sheet0[2] {
            out.iterate = self.sheet0.iterate;
        }
        if self.saw_sheet0[3] {
            out.iterate_delta = self.sheet0.iterate_delta;
        }
        if self.saw_sheet0[4] {
            out.calc_on_save = self.sheet0.calc_on_save;
        }
        out
    }
}

/// Emits `<table:iteration .../>` inside ODS `<table:calculation-settings>` when non-default.
pub(crate) fn ods_iteration_xml(opts: &CalcOptions) -> String {
    if !opts.has_non_default_iteration() {
        return String::new();
    }
    let status = if opts.iterate { "enable" } else { "disable" };
    let count = if CalcOptions::valid_iterate_count(opts.iterate_count) {
        opts.iterate_count
    } else {
        DEFAULT_ITERATE_COUNT
    };
    let delta = CalcOptions::sanitized_delta(opts.iterate_delta).unwrap_or(DEFAULT_ITERATE_DELTA);
    format!(
        r#"<table:iteration table:status="{status}" table:steps="{count}" table:maximum-difference="{delta}"/>"#
    )
}

/// Reads `<table:iteration>` attributes from ODS `content.xml` into `opts`.
pub(crate) fn read_ods_iteration(content_xml: &str, opts: &mut CalcOptions) {
    let Some((attrs, _)) = extract_tags_or_self_closing_with_attrs(content_xml, "table:iteration")
        .into_iter()
        .next()
    else {
        return;
    };
    if let Some(status) = extract_attr_from_fragment(attrs, "table:status") {
        opts.iterate = status.trim() == "enable";
    }
    if let Some(steps) = extract_attr_from_fragment(attrs, "table:steps")
        .and_then(|s| s.trim().parse::<u16>().ok())
        .filter(|&c| CalcOptions::valid_iterate_count(c))
    {
        opts.iterate_count = steps;
    }
    if let Some(diff) = extract_attr_from_fragment(attrs, "table:maximum-difference")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .and_then(CalcOptions::sanitized_delta)
    {
        opts.iterate_delta = diff;
    }
}

/// Emits `<config:config-item-set config:name="ooo:configuration-settings">` when calculation
/// mode, `calc_on_save`, or `full_calc_on_load` differs from defaults.
pub(crate) fn ods_configuration_settings_xml(opts: &CalcOptions) -> String {
    if !opts.has_non_default_settings() {
        return String::new();
    }
    let auto_calc = opts.mode != CalcMode::Manual;
    let mut xml = format!(
        r#"<config:config-item-set config:name="{OOO_CONFIG_NAME}"><config:config-item config:name="AutoCalculate" config:type="boolean">{auto_calc}</config:config-item>"#
    );
    if opts.mode != CalcMode::Auto {
        let _ = write!(
            xml,
            r#"<config:config-item config:name="EoCalcMode" config:type="string">{}</config:config-item>"#,
            opts.mode.as_str()
        );
    }
    if !opts.calc_on_save {
        xml.push_str(
            r#"<config:config-item config:name="EoCalcOnSave" config:type="boolean">false</config:config-item>"#,
        );
    }
    if opts.full_calc_on_load && opts.mode == CalcMode::Manual {
        xml.push_str(
            r#"<config:config-item config:name="EoFullCalcOnLoad" config:type="boolean">true</config:config-item>"#,
        );
    }
    xml.push_str("</config:config-item-set>");
    xml
}

/// Reads `AutoCalculate` / `EoCalcMode` / `EoCalcOnSave` / `EoFullCalcOnLoad` from `settings.xml`.
pub(crate) fn read_ods_config_settings(settings_xml: &str, opts: &mut CalcOptions) {
    let mut reader = crate::xml_stream::XmlReader::unbounded(settings_xml);
    let mut in_config = false;
    let mut item: Option<String> = None;
    let mut auto_calc: Option<bool> = None;
    let mut eo_mode: Option<CalcMode> = None;
    let mut text_buf = String::new();
    while let Ok(Some(event)) = reader.next_event() {
        match event {
            crate::xml_stream::XmlEvent::Start { name, attrs } => match name {
                "config:config-item-set" => {
                    in_config = attrs.get("config:name").as_deref() == Some(OOO_CONFIG_NAME);
                }
                "config:config-item" if in_config => {
                    item = attrs.get("config:name").map(|s| s.into_owned());
                    text_buf.clear();
                }
                _ => {}
            },
            crate::xml_stream::XmlEvent::Text(text) if in_config && item.is_some() => {
                text_buf.push_str(&text);
            }
            crate::xml_stream::XmlEvent::End("config:config-item") if in_config => {
                if let Some(key) = item.take() {
                    let val = text_buf.trim();
                    match key.as_str() {
                        "AutoCalculate" => {
                            if val == "false" {
                                auto_calc = Some(false);
                            } else if val == "true" {
                                auto_calc = Some(true);
                            }
                        }
                        "EoCalcMode" => eo_mode = CalcMode::parse(val),
                        "EoCalcOnSave" if val == "false" => opts.calc_on_save = false,
                        "EoFullCalcOnLoad" if val == "true" => opts.full_calc_on_load = true,
                        _ => {}
                    }
                }
                text_buf.clear();
            }
            crate::xml_stream::XmlEvent::End("config:config-item-set") => in_config = false,
            _ => {}
        }
    }
    if let Some(mode) = eo_mode {
        opts.mode = mode;
    } else if auto_calc == Some(false) {
        opts.mode = CalcMode::Manual;
    }
    if opts.mode != CalcMode::Manual {
        opts.full_calc_on_load = false;
    }
}
