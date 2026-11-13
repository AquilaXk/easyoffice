//! A small model over a [`RecalcSession`] for the dynamic array tests: set cells by A1 address, calculate,
//! read the text of a cell. Two sheets (`Sheet1`, `Sheet2`) exist from the start.
#![allow(dead_code)]

use easyoffice_formats::DefinedName;
use easyoffice_sheet::{
    EvaluatedCellValue, PackedCellCoordinate, RecalcOutcome, RecalcSession, WorkbookEngine,
};

pub struct Model {
    session: RecalcSession,
    names: Vec<DefinedName>,
    pub last: Option<RecalcOutcome>,
}

/// A defined name for [`Model::new_defined_name`]; `scope` is a sheet index.
pub struct DefinedNameEntry {
    name: String,
    refers_to: String,
    scope: Option<usize>,
}

impl DefinedNameEntry {
    pub fn new(name: &str, refers_to: &str) -> Self {
        Self {
            name: name.to_string(),
            refers_to: refers_to.to_string(),
            scope: None,
        }
    }

    pub fn scope(mut self, sheet: usize) -> Self {
        self.scope = Some(sheet);
        self
    }
}

pub fn new_empty_model() -> Model {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Sheet2");
    Model {
        session: RecalcSession::new(wb).expect("session"),
        names: Vec::new(),
        last: None,
    }
}

fn split(address: &str) -> (String, PackedCellCoordinate) {
    let (sheet, cell) = address
        .rsplit_once('!')
        .map_or(("Sheet1", address), |(s, c)| (s, c));
    (
        sheet.to_string(),
        PackedCellCoordinate::parse_a1(cell).expect("address"),
    )
}

impl Model {
    pub fn _set(&mut self, address: &str, value: &str) {
        let (sheet, coord) = split(address);
        self.session
            .set_cell(&sheet, coord, value)
            .expect("set_cell");
    }

    pub fn evaluate(&mut self) {
        self.last = Some(self.session.recalculate());
    }

    pub fn _get_text(&self, address: &str) -> String {
        let (sheet, coord) = split(address);
        self.session.cell_value(&sheet, coord).to_display_string()
    }

    pub fn value(&self, address: &str) -> EvaluatedCellValue {
        let (sheet, coord) = split(address);
        self.session.cell_value(&sheet, coord)
    }

    pub fn new_sheet(&mut self) {}

    pub fn new_defined_name(&mut self, entry: DefinedNameEntry) -> Result<(), String> {
        let scope = entry.scope.map(|i| format!("Sheet{}", i + 1));
        self.names.push(DefinedName {
            name: entry.name,
            scope,
            refers_to: entry.refers_to,
            comment: String::new(),
            hidden: false,
        });
        self.session
            .set_names(&self.names)
            .map_err(|e| e.to_string())
    }

    pub fn session(&mut self) -> &mut RecalcSession {
        &mut self.session
    }
}
