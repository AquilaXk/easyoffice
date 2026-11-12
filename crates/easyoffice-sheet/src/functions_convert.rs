//! `CONVERT` unit conversion (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/functions/engineering/convert/index.ts`
//! (the unit table with symbols, alternate symbols, quantity and ratio to the base unit, the
//! decimal and binary prefix tables, the temperature conversions) of the Univer formula engine,
//! commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum
//! Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: the symbol lookup is a hash map built once (last table row wins,
//! as the reference's scan does) instead of a scan of the whole table per call; rows whose symbol
//! the reference stores garbled (the atomic-unit rows, the arc minute and second, a few derived SI
//! units) are left out; the square metre answers to `m2` and `m^2`.

use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};
use std::collections::HashMap;
use std::sync::OnceLock;

/// `(symbol, alternate symbols, quantity, ratio to the base unit of the quantity)`.
type Unit = (&'static str, &'static [&'static str], &'static str, f64);

const UNITS: &[Unit] = &[
    ("e", &[], "electric_charge", 1.60217653141414e-19),
    ("Eh", &[], "energy", 4.35974417757576e-18),
    ("admkn", &[], "speed", 0.514773333),
    ("A", &[], "electric_current", 1.0),
    ("A/m", &[], "magnetic_field_intensity", 1.0),
    ("Å", &["ang"], "length", 1e-10),
    ("ar", &[], "area", 100.0),
    ("ua", &[], "length", 1.49597870691667e-11),
    ("bar", &[], "pressure", 100000.0),
    ("b", &[], "area", 1e-28),
    ("Bq", &[], "radioactivity", 1.0),
    ("bit", &["b"], "information", 1.0),
    ("BTU", &["btu"], "energy", 1055.05585262),
    ("byte", &[], "information", 8.0),
    ("cd", &[], "luminous_intensity", 1.0),
    ("C", &["cel"], "temperature", 1.0),
    ("ang3", &["ang^3"], "volume", 1e-30),
    ("ft3", &["ft^3"], "volume", 0.028316846592),
    ("in3", &["in^3"], "volume", 0.000016387064),
    ("ly3", &["ly^3"], "volume", 8.46786664623715e-47),
    ("m3", &["m^3"], "volume", 1.0),
    ("mi3", &["mi^3"], "volume", 4168181825.44058),
    ("Nmi3", &["Nmi^3"], "volume", 6352182208.0),
    (
        "Pica3",
        &["Picapt3", "Pica^3", "Picapt^3"],
        "volume",
        7.58660370370369e-8,
    ),
    ("yd3", &["yd^3"], "volume", 0.764554857984),
    ("cup", &[], "volume", 0.0002365882365),
    ("Da", &["u"], "mass", 1.66053886282828e-27),
    ("d", &["day"], "time", 86400.0),
    ("°", &[], "angle", 0.0174532925199433),
    ("dyn", &["dy"], "force", 0.00001),
    ("eV", &["ev"], "energy", 1.60217656514141),
    ("ell", &[], "length", 1.143),
    ("erg", &["e"], "energy", 1e-7),
    ("F", &["fah"], "temperature", 1.0),
    ("oz", &[], "volume", 0.0000295735295625),
    ("ft", &[], "length", 0.3048),
    ("flb", &[], "energy", 1.3558179483314),
    ("Gal", &[], "acceleration", 0.01),
    ("gal", &[], "volume", 0.003785411784),
    ("G", &["ga"], "magnetic_flux_density", 1.0),
    ("grain", &[], "mass", 0.0000647989),
    ("g", &[], "mass", 0.001),
    ("Gy", &[], "absorbed_dose", 1.0),
    ("GRT", &["regton"], "volume", 2.8316846592),
    ("ha", &[], "area", 10000.0),
    ("H", &[], "inductance", 1.0),
    ("Hz", &[], "frequency", 1.0),
    ("HP", &["h"], "power", 745.69987158227),
    ("HPh", &["hh", "hph"], "energy", 2684519.538),
    ("h", &["hr"], "time", 3600.0),
    ("uk_gal", &[], "volume", 0.00454609),
    ("lcwt", &["uk_cwt", "hweight"], "mass", 50.802345),
    ("uk_qt", &[], "volume", 0.0011365225),
    ("brton", &["uk_ton", "LTON"], "mass", 1016.046909),
    ("in", &[], "length", 0.0254),
    ("uk_acre", &[], "area", 4046.8564224),
    ("cal", &[], "energy", 4.1868),
    ("J", &[], "energy", 1.0),
    ("kat", &[], "catalytic_activity", 1.0),
    ("K", &["kel"], "temperature", 1.0),
    ("kg", &[], "mass", 1.0),
    ("kn", &[], "speed", 0.514444444444444),
    ("ly", &[], "length", 9460730472580800.0),
    ("L", &["l", "lt"], "volume", 0.001),
    ("lm", &[], "luminous_flux", 1.0),
    ("lx", &[], "illuminance", 1.0),
    ("Mx", &[], "magnetic_flux", 1e-18),
    ("MTON", &[], "volume", 1.13267386368),
    ("m/h", &["m/hr"], "speed", 0.00027777777777778),
    ("m/s", &["m/sec"], "speed", 1.0),
    ("pc", &["parsec"], "length", 30856775814671900.0),
    ("m", &[], "length", 1.0),
    ("mph", &[], "speed", 0.44704),
    ("mmHg", &[], "pressure", 133.322),
    ("min", &["mn"], "time", 60.0),
    ("tspm", &[], "volume", 0.000005),
    ("mol", &[], "amount_of_substance", 1.0),
    ("Morgen", &[], "area", 2500.0),
    ("M", &["Nmi"], "length", 1852.0),
    ("N", &[], "force", 1.0),
    ("Oe ", &[], "magnetic_field_intensity", 79.5774715459477),
    ("Ω", &[], "electric_resistance", 1.0),
    ("ozm", &[], "mass", 0.028349523125),
    ("Pa", &[], "pressure", 1.0),
    ("PS", &[], "power", 735.49875),
    ("ph", &[], "illuminance", 0.0001),
    ("pica", &[], "length", 0.00035277777777778),
    ("Pica", &["Picapt"], "length", 0.00423333333333333),
    ("P", &[], "dynamic_viscosity", 0.1),
    ("pond", &[], "force", 0.00980665),
    ("lbf", &[], "force", 4.4482216152605),
    ("lbm", &[], "mass", 0.45359237),
    ("qt", &[], "volume", 0.000946352946),
    ("rad", &[], "angle", 1.0),
    ("Rank", &[], "temperature", 1.0),
    ("Reau", &[], "temperature", 1.0),
    ("s", &["sec"], "time", 1.0),
    ("cwt", &["shweight"], "mass", 45.359237),
    ("S", &[], "electrical_conductance", 1.0),
    ("Sv", &[], "equivalent_dose", 1.0),
    ("sg", &[], "mass", 14.59390294),
    ("ang2", &["ang^2"], "area", 1e-20),
    ("ft2", &["ft^2"], "area", 0.09290304),
    ("in2", &["in^2"], "area", 0.00064516),
    ("ly2", &["ly^2"], "area", 8.95054210748189e31),
    ("m2", &["m^2"], "area", 1.0),
    ("mi2", &["mi^2"], "area", 2589988.110336),
    ("Nmi2", &["Nmi^2"], "area", 3429904.0),
    (
        "Pica2",
        &["Picapt2", "Pica^2", "Picapt^2"],
        "area",
        0.00001792111111111,
    ),
    ("yd2", &["yd^2"], "area", 0.83612736),
    ("mi", &[], "length", 1609.344),
    ("sr", &[], "solid_angle", 1.0),
    ("sb", &[], "luminance", 0.0001),
    ("St", &[], "kinematic_viscosity", 0.0001),
    ("stone", &[], "mass", 6.35029318),
    ("tbs", &[], "volume", 0.0000147868),
    ("tsp", &[], "volume", 0.00000492892),
    ("T", &[], "magnetic_flux_density", 1.0),
    ("c", &[], "energy", 4.184),
    ("ton", &[], "mass", 907.18474),
    ("t", &[], "mass", 1000.0),
    ("uk_pt", &[], "volume", 0.00056826125),
    ("bushel", &[], "volume", 0.03523907),
    ("barrel", &[], "volume", 0.158987295),
    ("pt", &["us_pt"], "volume", 0.000473176473),
    ("survey_mi", &[], "length", 1609.347219),
    ("us_acre", &[], "area", 4046.87261),
    ("V", &[], "voltage", 1.0),
    ("W", &[], "power", 1.0),
    ("Wh", &["wh"], "energy", 3600.0),
    ("Wb", &[], "magnetic_flux", 1.0),
    ("yd", &[], "length", 0.9144),
    ("yr", &[], "time", 31557600.0),
];

/// Binary prefixes (`ki` to `Yi`) and their multipliers.
const BINARY_PREFIXES: &[(&str, f64)] = &[
    ("Yi", 1_208_925_819_614_629_174_706_176.0),
    ("Zi", 1_180_591_620_717_411_303_424.0),
    ("Ei", 1_152_921_504_606_846_976.0),
    ("Pi", 1_125_899_906_842_624.0),
    ("Ti", 1_099_511_627_776.0),
    ("Gi", 1_073_741_824.0),
    ("Mi", 1_048_576.0),
    ("ki", 1_024.0),
];

/// One-letter decimal prefixes (`da` is handled in `split_prefix`).
const UNIT_PREFIXES: &[(&str, f64)] = &[
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("e", 1e1),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
];

/// Symbol (or alternate symbol) to its row in [`UNITS`]; a later row replaces an earlier one.
fn unit_index() -> &'static HashMap<&'static str, usize> {
    static INDEX: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map = HashMap::with_capacity(UNITS.len() * 2);
        for (row, (symbol, alternates, _, _)) in UNITS.iter().enumerate() {
            map.insert(*symbol, row);
            for alternate in *alternates {
                map.insert(*alternate, row);
            }
        }
        map
    })
}

/// The unit a symbol names, with the multiplier of its prefix (1 without one).
fn lookup(symbol: &str) -> Option<(&'static Unit, f64)> {
    let index = unit_index();
    if let Some(&row) = index.get(symbol) {
        return Some((&UNITS[row], 1.0));
    }
    let (prefix_len, multiplier) = split_prefix(symbol)?;
    index
        .get(&symbol[prefix_len..])
        .map(|&row| (&UNITS[row], multiplier))
}

/// Length and multiplier of the prefix a symbol starts with: binary prefixes first (so `Yi` is not
/// read as `Y`), then `da`, then the one-letter decimal prefixes.
fn split_prefix(symbol: &str) -> Option<(usize, f64)> {
    if let Some(&(_, multiplier)) = BINARY_PREFIXES.iter().find(|(p, _)| symbol.starts_with(p)) {
        return Some((2, multiplier));
    }
    if symbol.starts_with("da") {
        return Some((2, 1e1));
    }
    let first = symbol.chars().next()?;
    UNIT_PREFIXES
        .iter()
        .find(|(p, _)| p.starts_with(first))
        .map(|&(p, multiplier)| (p.len(), multiplier))
}

fn temperature(value: f64, from: &str, to: &str) -> f64 {
    let kelvin = match from {
        "C" => value + 273.15,
        "F" => (value - 32.0) * 5.0 / 9.0 + 273.15,
        "K" => value,
        "Rank" => value * 5.0 / 9.0,
        "Reau" => value * 5.0 / 4.0 + 273.15,
        _ => return value,
    };
    match to {
        "C" => kelvin - 273.15,
        "F" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
        "K" => kelvin,
        "Rank" => kelvin * 9.0 / 5.0,
        "Reau" => (kelvin - 273.15) * 4.0 / 5.0,
        _ => value,
    }
}

impl SheetEngine {
    pub(crate) fn eval_convert(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let a = self.fin_args(args, (3, 3), ArgRule::Strict, ctx, depth)?;
        let value = a.num(0)?;
        let (from, to) = (a.text(1), a.text(2));
        let (Some((from_unit, from_multiplier)), Some((to_unit, to_multiplier))) =
            (lookup(&from), lookup(&to))
        else {
            return Err(FormulaError::Na);
        };
        if from_unit.2 != to_unit.2 {
            return Err(FormulaError::Na);
        }
        let result = if from_unit.2 == "temperature" {
            temperature(value, from_unit.0, to_unit.0)
        } else {
            value * from_unit.3 * from_multiplier / (to_unit.3 * to_multiplier)
        };
        Ok(EvaluatedCellValue::Number(result))
    }
}
