//! The package: components (one `Index/<name>.iwa` stream each), the component index
//! (`TSP.PackageMetadata`, `Index/Metadata.iwa`), the identity files and the ZIP.
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig), `src/create.rs` 의 `Blueprint`
//! (`component_index` 366-480행, `finish` 328-352행)와 `FORMAT.md` §14·"Writing documents"
//! 규칙 3·8·22. 구성 요소 번호 체계와 `PackageMetadata` 필드 5·7·8, `ComponentInfo` 필드
//! 1·2·4·5·6·10·12·21, 루트 참조는 `{component}`·그 밖은 `{component, object}` 형태를 따름.
//! 객체별 UUID 표(`ComponentInfo` 11)는 Pages가 필요로 하지 않는다는 측정에 따라 쓰지 않음.

use super::frame::{chunks, stream, Record};
use super::identity::{properties, uuid};
use super::refs::classify;
use super::wire::{bytes, encode, is_ascending, msg, string, varint, Fld};
use crate::pages_iwa::{MAX_DOCUMENT_BYTES, MAX_IWA_FILE_BYTES};
use crate::{EasyOfficeError, ZipArchiveCodec};
use std::collections::{BTreeMap, BTreeSet};

/// The document archive, always object 1 and the root of the `Document` component.
pub(super) const ROOT: u64 = 1;
/// `TSP.PackageMetadata`, object 2 of `Index/Metadata.iwa`.
const PACKAGE_METADATA: u64 = 2;
/// `TSP.DocumentMetadata`, the root of the `DocumentMetadata` component.
pub(super) const DOCUMENT_METADATA: u64 = 71;
/// Identifiers handed out by [`Blueprint::allocate`] start here, above the three fixed ones.
const FIRST_IDENTIFIER: u64 = 1000;

/// `TSP.PackageMetadata` and `TSP.DocumentMetadata`.
const TYPE_PACKAGE_METADATA: u32 = 11006;
const TYPE_DOCUMENT_METADATA: u32 = 11011;

/// `ComponentInfo` fields 4 and 5 (`[2, 0, 0]`, packed) and `PackageMetadata` field 5.
const COMPONENT_VERSION: [u8; 3] = [2, 0, 0];
/// `PackageMetadata` field 7 (`[26, 3, 1]`, packed). Left out, the document opens empty.
const PACKAGE_VERSION: [u8; 3] = [26, 3, 1];
/// `PackageMetadata` field 8 and `ComponentInfo` field 12: the format generation, per app.
const FORMAT_GENERATION: u64 = 835;

/// Index of a component of the [`Blueprint`].
#[derive(Clone, Copy)]
pub(super) struct ComponentId(usize);

struct Entry {
    record: Record,
    /// Every object the payload refers to, strong or weak.
    refers_to: BTreeSet<u64>,
    strong: BTreeSet<u64>,
}

struct Component {
    id: u64,
    name: &'static str,
    always_loaded: bool,
    entries: Vec<Entry>,
}

/// A document being assembled.
pub(super) struct Blueprint {
    components: Vec<Component>,
    next: u64,
}

fn corrupt(detail: String) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("Pages 쓰기: {detail}"))
}

impl Blueprint {
    /// A blueprint with the two components every document has.
    pub(super) fn new() -> Self {
        let component = |id, name, always_loaded| Component {
            id,
            name,
            always_loaded,
            entries: Vec::new(),
        };
        Self {
            components: vec![
                component(ROOT, "Document", false),
                component(DOCUMENT_METADATA, "DocumentMetadata", true),
            ],
            next: FIRST_IDENTIFIER,
        }
    }

    pub(super) fn document(&self) -> ComponentId {
        ComponentId(0)
    }

    pub(super) fn allocate(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }

    /// A new component rooted at a fresh identifier (returned: that identifier).
    pub(super) fn component(&mut self, name: &'static str) -> (ComponentId, u64) {
        let id = self.allocate();
        self.components.push(Component {
            id,
            name,
            always_loaded: false,
            entries: Vec::new(),
        });
        (ComponentId(self.components.len() - 1), id)
    }

    /// Puts an object into a component. A payload whose fields are out of order is refused: the
    /// writer's rule is that no message is written any other way.
    pub(super) fn put(
        &mut self,
        component: ComponentId,
        id: u64,
        type_id: u32,
        fields: &[Fld],
    ) -> Result<(), EasyOfficeError> {
        if !is_ascending(fields) {
            return Err(corrupt(format!(
                "객체 {id}(형식 {type_id})의 필드 순서가 올바르지 않습니다"
            )));
        }
        let refs = classify(type_id, fields);
        self.components[component.0].entries.push(Entry {
            record: Record {
                id,
                type_id,
                payload: encode(fields),
                owned: Vec::new(),
            },
            refers_to: refs.all,
            strong: refs.strong,
        });
        Ok(())
    }

    /// Allocates an identifier and puts an object under it.
    pub(super) fn add(
        &mut self,
        component: ComponentId,
        type_id: u32,
        fields: &[Fld],
    ) -> Result<u64, EasyOfficeError> {
        let id = self.allocate();
        self.put(component, id, type_id, fields)?;
        Ok(id)
    }

    /// The ZIP entries of the package: object streams, the component index and the identity.
    pub(super) fn finish(mut self) -> Result<Vec<u8>, EasyOfficeError> {
        let metadata = self
            .components
            .iter()
            .position(|c| c.id == DOCUMENT_METADATA);
        let metadata = ComponentId(metadata.unwrap_or(1));
        self.put(
            metadata,
            DOCUMENT_METADATA,
            TYPE_DOCUMENT_METADATA,
            &[varint(1, 0)],
        )?;

        let home: BTreeMap<u64, u64> = self
            .components
            .iter()
            .flat_map(|c| c.entries.iter().map(move |e| (e.record.id, c.id)))
            .collect();
        for component in &mut self.components {
            for entry in &mut component.entries {
                if let Some(target) = entry.refers_to.iter().find(|t| !home.contains_key(t)) {
                    return Err(corrupt(format!(
                        "객체 {}가 없는 객체 {target}를 참조합니다",
                        entry.record.id
                    )));
                }
                entry.record.owned = entry.strong.iter().copied().collect();
            }
        }

        let index = self.component_index(&home)?;
        let mut zip_entries: Vec<(String, Vec<u8>)> = Vec::new();
        let mut total = 0usize;
        for component in &mut self.components {
            // The root of a component comes first in its stream.
            let id = component.id;
            component.entries.sort_by_key(|e| e.record.id != id);
            let records: Vec<&Record> = component.entries.iter().map(|e| &e.record).collect();
            total += add_stream(&mut zip_entries, component.name, &records)?;
        }
        total += add_stream(&mut zip_entries, "Metadata", &[&index])?;
        if total > MAX_DOCUMENT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 쓰기: 문서 전체 크기 한도 초과".to_string(),
            ));
        }
        let (plist, identifier) = properties();
        zip_entries.push(("Metadata/DocumentIdentifier".to_string(), identifier));
        zip_entries.push(("Metadata/Properties.plist".to_string(), plist));
        // `ZipArchiveCodec::pack` stores every entry (no deflate): the apps read stored entries.
        ZipArchiveCodec::pack(&zip_entries)
    }

    /// `TSP.PackageMetadata`: every component and every reference that leaves its component.
    fn component_index(&self, home: &BTreeMap<u64, u64>) -> Result<Record, EasyOfficeError> {
        let highest = home
            .keys()
            .copied()
            .max()
            .unwrap_or(DOCUMENT_METADATA)
            .max(self.next);
        let mut fields = vec![
            varint(1, highest),
            msg(2, vec![string(2, &uuid()), varint(3, 0)]),
        ];
        for component in &self.components {
            let mut info = vec![
                varint(1, component.id),
                string(2, component.name),
                bytes(4, COMPONENT_VERSION.to_vec()),
                bytes(5, COMPONENT_VERSION.to_vec()),
            ];
            // The document and its metadata are open before anything else is read, so a
            // reference into either needs no declaring; every other crossing does, roots too.
            let mut declared = BTreeSet::new();
            for entry in &component.entries {
                for target in &entry.refers_to {
                    let owner = home[target];
                    if owner != component.id && *target != ROOT && *target != DOCUMENT_METADATA {
                        declared.insert(*target);
                    }
                }
            }
            for target in declared {
                let owner = home[&target];
                // `{component}` declares that component's root (its identifier is the
                // component's own); the long form `{component, object}` declares an object
                // inside it. They are not interchangeable.
                info.push(msg(
                    6,
                    if target == owner {
                        vec![varint(1, owner)]
                    } else {
                        vec![varint(1, owner), varint(2, target)]
                    },
                ));
            }
            info.push(varint(10, 0));
            info.push(varint(12, FORMAT_GENERATION));
            if component.always_loaded {
                info.push(varint(21, 1));
            }
            fields.push(msg(3, info));
        }
        fields.push(bytes(5, COMPONENT_VERSION.to_vec()));
        fields.push(bytes(7, PACKAGE_VERSION.to_vec()));
        fields.push(varint(8, FORMAT_GENERATION));
        if !is_ascending(&fields) {
            return Err(corrupt(
                "구성 요소 목록의 필드 순서가 올바르지 않습니다".to_string(),
            ));
        }
        Ok(Record {
            id: PACKAGE_METADATA,
            type_id: TYPE_PACKAGE_METADATA,
            payload: encode(&fields),
            owned: Vec::new(),
        })
    }
}

/// Frames `records` into `Index/<name>.iwa`; returns the size of the record stream.
fn add_stream(
    out: &mut Vec<(String, Vec<u8>)>,
    name: &str,
    records: &[&Record],
) -> Result<usize, EasyOfficeError> {
    let bytes = stream(records.iter().copied());
    if bytes.len() > MAX_IWA_FILE_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "Pages 쓰기: .iwa 파일 크기 한도 초과".to_string(),
        ));
    }
    let len = bytes.len();
    out.push((format!("Index/{name}.iwa"), chunks(&bytes)?));
    Ok(len)
}
