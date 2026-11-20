//! What the result of an edit keeps of the original besides the pages, and what it tells the person
//! it could not keep (PD-266).
//!
//! The pages are copied by `pdf_edit_build.rs`; everything the page tree does not hold is looked
//! at here before any page is written, once for each source that contributes a page. Anything
//! the result does not keep becomes a sentence in [`Warnings`], which travels with the PDF to the
//! page (`pdf_edit_wire.rs`, `WARNING`), instead of being left out without a word.
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-syntax` reads the
//! objects, `hayro-write` copies them, and nothing in the dependency graph lists what a PDF holds
//! beyond its pages; (2) the JavaScript PDF viewers read the same catalog entries for display
//! only, so nothing was ported; (3) the checks below are written against ISO 32000-1 §12.5.6.1
//! (annotation types), §14.7 (structure tree), §12.6.4.17 (JavaScript) and §14.3.2 (metadata
//! streams); the entries each part writes are in `pdf_edit_oc.rs`, `pdf_edit_annots.rs`,
//! `pdf_edit_labels.rs` and `pdf_edit_attach.rs`.

use crate::pdf_edit_annots::{lost_sentence, scan_page, Lost};
use crate::pdf_edit_attach::{self, Attachment};
use crate::pdf_edit_budget::{
    Deadline, MAX_ATTACHMENTS, MAX_CARRIED_NODES, MAX_TOTAL_ANNOTS, MAX_XMP_BYTES,
    PAGES_PER_CHECK_MASK,
};
use crate::pdf_edit_labels;
use crate::pdf_edit_oc;
use crate::pdf_edit_preserve::too_many_annots;
use crate::pdf_edit_value::Refused;
use crate::pdf_edit_value::{Budget, Value};
use crate::pdf_edit_wire::{PagePlan, MAX_WARNINGS, MAX_WARNING_BYTES};
use hayro_syntax::object::{Dict, MaybeRef, ObjRef, Object, Stream};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::writers::Catalog;
use hayro_write::pdf_writer::{Name as WName, Pdf, Ref};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The reference in the result of each object the copier was asked for, by source.
pub type Copies = HashMap<(usize, ObjRef), Ref>;

/// The sentences about what the result does not keep, without repeats and within the limits the
/// wire allows.
#[derive(Debug, Default)]
pub struct Warnings(Vec<String>);

impl Warnings {
    pub fn push(&mut self, mut text: String) {
        if text.len() > MAX_WARNING_BYTES {
            let mut end = MAX_WARNING_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        if self.0.len() < MAX_WARNINGS && !self.0.contains(&text) {
            self.0.push(text);
        }
    }

    pub fn into_vec(self) -> Vec<String> {
        self.0
    }
}

/// What was found in the sources, ready to be written into the result.
#[derive(Debug, Default)]
pub struct Carry {
    pub warnings: Warnings,
    budget: Budget,
    /// The `/OCProperties` of each source that has them.
    layers: Vec<(usize, Value)>,
    /// The free text, ink and stamp annotations of each (source, page) that is used.
    annots: HashMap<(usize, usize), Vec<Value>>,
    /// The attachments of each source.
    attachments: Vec<(usize, Vec<Attachment>)>,
    /// The `/PageLabels` of the result.
    page_labels: Option<Value>,
    /// The metadata stream of the file of the first page.
    metadata: Option<(usize, ObjRef)>,
    /// The nodes of the annotations written so far: a page placed again writes them again.
    written_nodes: usize,
    /// The objects of each source that are copied together with its pages.
    roots: BTreeMap<usize, Vec<ObjRef>>,
}

impl Carry {
    /// Looks at the pages in `pages` and at the catalog of every source they come from.
    pub fn plan(
        parsed: &BTreeMap<usize, ReadPdf>,
        pages: &[PagePlan],
        deadline: &Deadline,
    ) -> Result<Self, String> {
        let mut carry = Self::default();
        carry.scan_pages(parsed, pages, deadline)?;
        carry.scan_catalogs(parsed, pages)?;
        for roots in carry.roots.values_mut() {
            let mut seen = HashSet::new();
            roots.retain(|object| seen.insert(*object));
        }
        Ok(carry)
    }

    /// The annotations of every page that is used (once, however often the page is placed).
    fn scan_pages(
        &mut self,
        parsed: &BTreeMap<usize, ReadPdf>,
        pages: &[PagePlan],
        deadline: &Deadline,
    ) -> Result<(), String> {
        let mut lost = Lost::new();
        for (n, plan) in pages.iter().enumerate() {
            if n & PAGES_PER_CHECK_MASK == 0 {
                deadline.check()?;
            }
            let key = (plan.source, plan.index);
            if self.annots.contains_key(&key) {
                continue;
            }
            let pdf = parsed
                .get(&plan.source)
                .ok_or("쪽의 원본 PDF가 없습니다.")?;
            let page = pdf.pages().get(plan.index).ok_or("없는 쪽을 가리킵니다.")?;
            let found = scan_page(page, &mut self.budget, &mut lost)?;
            for value in &found {
                self.add_roots(plan.source, value);
            }
            self.annots.insert(key, found);
        }
        if !lost.is_empty() {
            self.warnings.push(lost_sentence(&lost));
        }
        Ok(())
    }

    /// What each source's catalog holds: layers, attachments, page labels, metadata.
    fn scan_catalogs(
        &mut self,
        parsed: &BTreeMap<usize, ReadPdf>,
        pages: &[PagePlan],
    ) -> Result<(), String> {
        let mut labels = BTreeMap::new();
        let mut attached = 0usize;
        for (&source, pdf) in parsed {
            note_catalog(pdf, &mut self.warnings);
            if let Some(list) = pdf_edit_oc::read(pdf, &mut self.budget, &mut self.warnings)? {
                self.add_roots(source, &list);
                self.layers.push((source, list));
            }
            let files = pdf_edit_attach::read(pdf, &mut self.budget)?;
            attached += files.len();
            if attached > MAX_ATTACHMENTS {
                return Err(pdf_edit_attach::too_many_in_all());
            }
            for file in &files {
                self.add_roots(source, file.spec());
            }
            if !files.is_empty() {
                self.attachments.push((source, files));
            }
            if let Some(found) = pdf_edit_labels::read(pdf)? {
                labels.insert(source, found);
            }
        }
        self.page_labels = pdf_edit_labels::combine(&labels, pages)?;
        // The information dictionary comes from the file of the first page; so does the metadata.
        if let Some((source, pdf)) = pages
            .first()
            .and_then(|p| Some((p.source, parsed.get(&p.source)?)))
        {
            if let Some(stream) = read_metadata(pdf)? {
                self.add_roots(source, &Value::Ref(stream));
                self.metadata = Some((source, stream));
            }
        }
        Ok(())
    }

    /// Asks for the objects `value` names to be copied with the pages of `source`.
    fn add_roots(&mut self, source: usize, value: &Value) {
        let mut found = Vec::new();
        value.refs(&mut found);
        self.roots.entry(source).or_default().extend(found);
    }

    /// The objects of `source` that are copied in the same call as its pages, so that they get the
    /// same new references the pages' own objects get.
    pub fn roots(&self, source: usize) -> &[ObjRef] {
        self.roots.get(&source).map_or(&[], Vec::as_slice)
    }

    /// Writes the carried annotations of page `index` of `source` and returns their references.
    pub fn write_page_annots(
        &mut self,
        out: &mut Pdf,
        next: &mut Ref,
        (source, index): (usize, usize),
        copies: &Copies,
        total_annots: &mut usize,
    ) -> Result<Vec<Ref>, String> {
        let Some(found) = self.annots.get(&(source, index)) else {
            return Ok(Vec::new());
        };
        let mut written = Vec::with_capacity(found.len());
        for annot in found {
            self.written_nodes = self.written_nodes.saturating_add(annot.node_count());
            if self.written_nodes > MAX_CARRIED_NODES {
                return Err(Refused::Total.sentence("주석"));
            }
            *total_annots += 1;
            if *total_annots > MAX_TOTAL_ANNOTS {
                return Err(too_many_annots());
            }
            let id = next.bump();
            annot
                .translate(&|r| copies.get(&(source, r)).copied())
                .write(out.indirect(id));
            written.push(id);
        }
        Ok(written)
    }

    /// Writes what the catalog of the result holds besides the pages.
    pub fn write_catalog(&mut self, catalog: &mut Catalog<'_>, copies: &Copies) {
        let lists: Vec<Value> = self
            .layers
            .iter()
            .map(|(source, list)| list.translate(&|r| copies.get(&(*source, r)).copied()))
            .collect();
        if let Some(layers) = pdf_edit_oc::combine(lists, &mut self.warnings) {
            layers.write(catalog.insert(WName(b"OCProperties")));
        }
        if let Some(labels) = &self.page_labels {
            labels.write(catalog.insert(WName(b"PageLabels")));
        }
        let files: Vec<Vec<Attachment>> = self
            .attachments
            .iter()
            .map(|(source, list)| {
                list.iter()
                    .map(|file| file.translate(&|r| copies.get(&(*source, r)).copied()))
                    .collect()
            })
            .collect();
        if let Some(names) = pdf_edit_attach::combine(files) {
            names.write(catalog.insert(WName(b"Names")));
        }
        if let Some(stream) = self
            .metadata
            .and_then(|(source, id)| copies.get(&(source, id)))
        {
            catalog.pair(WName(b"Metadata"), *stream);
        }
    }
}

/// The metadata stream of `pdf`, when it has one that is not larger than the limit.
fn read_metadata(pdf: &ReadPdf) -> Result<Option<ObjRef>, String> {
    let xref = pdf.xref();
    let Some(MaybeRef::Ref(id)) = xref
        .get::<Dict<'_>>(xref.root_id())
        .and_then(|catalog| catalog.get_raw::<Object<'_>>(b"Metadata"))
    else {
        return Ok(None);
    };
    let Some(stream) = xref.get::<Stream<'_>>(id.into()) else {
        return Ok(None);
    };
    if stream.raw_data().len() > MAX_XMP_BYTES {
        return Err(format!(
            "XMP 메타데이터가 허용 크기({}MiB)를 초과합니다.",
            MAX_XMP_BYTES / 1_048_576
        ));
    }
    Ok(Some(id))
}

/// The catalog entries that the result does not write, as a sentence each.
fn note_catalog(pdf: &ReadPdf, warnings: &mut Warnings) {
    let xref = pdf.xref();
    let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else {
        return;
    };
    if catalog.contains_key(b"StructTreeRoot") {
        warnings.push("태그 구조 트리(접근성 정보)는 저장하지 않았습니다.".to_string());
    }
    let scripts = catalog
        .get::<Dict<'_>>(b"Names")
        .is_some_and(|names| names.contains_key(b"JavaScript"))
        || catalog.contains_key(b"AA");
    if scripts {
        warnings.push("문서 스크립트는 보안을 위해 저장하지 않았습니다.".to_string());
    }
    if catalog.contains_key(b"Collection") {
        warnings.push("PDF 포트폴리오 구성은 저장하지 않았습니다.".to_string());
    }
}

/// The sentence for the form fields the result does not write.
pub fn skipped_fields_sentence(count: usize) -> String {
    format!("전자서명 등 지원하지 않는 양식 필드 {count}개는 저장하지 못했습니다.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_are_cut_deduplicated_and_limited() {
        let mut warnings = Warnings::default();
        warnings.push("a".to_string());
        warnings.push("a".to_string());
        warnings.push("한".repeat(MAX_WARNING_BYTES));
        for n in 0..(MAX_WARNINGS * 2) {
            warnings.push(format!("w{n}"));
        }
        let all = warnings.into_vec();
        assert_eq!(all.len(), MAX_WARNINGS);
        assert_eq!(all[0], "a");
        assert!(all[1].len() <= MAX_WARNING_BYTES && all[1].chars().all(|c| c == '한'));
    }
}
