//! A direct PDF object held in memory with its indirect references left as references (PD-266).
//!
//! What the edit carries over without rewriting it (layer lists, annotation dictionaries, the name
//! tree of attachments) is read into a [`Value`] first. The references in it are the source file's;
//! the page copier (`pdf_edit_build.rs`, `hayro-write`) copies the objects they name and says which
//! new reference each one got, [`Value::translate`] swaps the references, and only then is the value
//! written out. A reference that has no copy becomes `null`.
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-write` copies objects
//! but offers no way to change a dictionary on the way (its writer is private), so a dictionary
//! whose `/Rect` or `/InkList` must move cannot go through it; (2) the JavaScript
//! PDF viewers read these entries into script values for display, the same idea, but nothing of
//! it fits a Rust writer; (3) written here, about 150 lines, on the `hayro-syntax` reader and the
//! PDF writer crate that `hayro-write` writes with. Nothing was copied.
//!
//! Reading never follows a reference, so a cycle of references cannot loop here; nesting depth and
//! the number of nodes are limited (fail closed, [`Refused`]).

use crate::pdf_edit_budget::{MAX_CARRIED_NODES, MAX_VALUE_DEPTH, MAX_VALUE_NODES};
use hayro_syntax::object::{Dict, MaybeRef, ObjRef, Object};
use hayro_write::pdf_writer::{Name, Null, Obj, Ref, Str};

/// An object as it is written, minus streams (a stream is always an indirect object, carried as a
/// [`Value::Ref`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i32),
    Real(f32),
    Str(Vec<u8>),
    Name(Vec<u8>),
    Array(Vec<Value>),
    Dict(Vec<(Vec<u8>, Value)>),
    Ref(ObjRef),
}

/// Why an object was not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Nested deeper than [`MAX_VALUE_DEPTH`].
    Deep,
    /// More than [`MAX_VALUE_NODES`] nodes in one object.
    Nodes,
    /// The objects carried over in one build together hold more than [`MAX_CARRIED_NODES`] nodes.
    Total,
    /// A number that is not finite once it is a single-precision real (the writer cannot write it).
    Number,
}

impl Refused {
    /// The sentence for the person; `what` says what was being read ("선택적 내용(레이어) 정보").
    pub fn sentence(self, what: &str) -> String {
        match self {
            Self::Deep => format!("{what}의 중첩이 허용 깊이({MAX_VALUE_DEPTH}단계)를 초과합니다."),
            Self::Nodes => {
                format!("{what}의 크기가 허용 한도(항목 {MAX_VALUE_NODES}개)를 초과합니다.")
            }
            Self::Number => {
                format!("{what}에 쓸 수 없는 숫자(너무 크거나 유한하지 않음)가 있습니다.")
            }
            Self::Total => format!(
                "원본에서 그대로 옮길 정보의 합계가 한도(항목 {MAX_CARRIED_NODES}개)를 초과합니다."
            ),
        }
    }
}

/// The nodes that all the values of one build may still hold.
#[derive(Debug)]
pub struct Budget(usize);

impl Default for Budget {
    fn default() -> Self {
        Self(MAX_CARRIED_NODES)
    }
}

impl Value {
    /// Reads `raw` and everything direct inside it; references stay references.
    pub fn read(raw: &MaybeRef<Object<'_>>, total: &mut Budget) -> Result<Self, Refused> {
        let mut own = MAX_VALUE_NODES;
        read_raw(raw, 1, &mut own, &mut total.0)
    }

    /// The entries of `dict` whose key `keep` accepts, as a dictionary value; the other entries
    /// are not read (they use none of the limits).
    pub fn read_dict(
        dict: &Dict<'_>,
        keep: impl Fn(&[u8]) -> bool,
        total: &mut Budget,
    ) -> Result<Self, Refused> {
        let mut own = MAX_VALUE_NODES;
        spend(&mut own, &mut total.0)?;
        let mut entries = Vec::new();
        // Only the keys that are kept are parsed: the others (a pop-up, the page) may be large.
        for key in dict.keys().filter(|key| keep(key.as_ref())) {
            let Some(item) = dict.get_raw::<Object<'_>>(key.as_ref()) else {
                continue;
            };
            spend(&mut own, &mut total.0)?;
            entries.push((
                key.as_ref().to_vec(),
                read_raw(&item, 2, &mut own, &mut total.0)?,
            ));
        }
        Ok(Self::Dict(entries))
    }

    /// The entry `key` of a dictionary.
    pub fn get(&self, key: &[u8]) -> Option<&Self> {
        match self {
            Self::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &[u8]) -> Option<&mut Self> {
        match self {
            Self::Dict(entries) => entries.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Removes the entry `key` of a dictionary.
    pub fn remove(&mut self, key: &[u8]) {
        if let Self::Dict(entries) = self {
            entries.retain(|(k, _)| k != key);
        }
    }

    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Self::Int(n) => Some(n as f32),
            Self::Real(n) => Some(n),
            _ => None,
        }
    }

    /// Keeps the entries of a dictionary whose key `keep` accepts (anything else stays as it is).
    pub fn retain_keys(&mut self, keep: impl Fn(&[u8]) -> bool) {
        if let Self::Dict(entries) = self {
            entries.retain(|(key, _)| keep(key));
        }
    }

    /// Sets the entry `key` of a dictionary, replacing an earlier one.
    pub fn set(&mut self, key: &[u8], value: Self) {
        if let Self::Dict(entries) = self {
            match entries.iter_mut().find(|(k, _)| k == key) {
                Some(entry) => entry.1 = value,
                None => entries.push((key.to_vec(), value)),
            }
        }
    }

    /// The number of nodes the value holds (what a read of it costs against the limits).
    pub fn node_count(&self) -> usize {
        match self {
            Self::Array(items) => 1 + items.iter().map(Self::node_count).sum::<usize>(),
            Self::Dict(entries) => {
                1 + entries
                    .iter()
                    .map(|(_, value)| 1 + value.node_count())
                    .sum::<usize>()
            }
            _ => 1,
        }
    }

    /// Every reference inside, in order of appearance.
    pub fn refs(&self, out: &mut Vec<ObjRef>) {
        match self {
            Self::Ref(r) => out.push(*r),
            Self::Array(items) => items.iter().for_each(|item| item.refs(out)),
            Self::Dict(entries) => entries.iter().for_each(|(_, value)| value.refs(out)),
            _ => {}
        }
    }

    /// The value with each reference swapped for the new reference `map` gives (`None`: the object
    /// was not copied, so the reference becomes `null`). The new references are written as
    /// references to the objects of the result, which [`Value::write`] writes as they are.
    pub fn translate(&self, map: &dyn Fn(ObjRef) -> Option<Ref>) -> Self {
        match self {
            Self::Ref(r) => match map(*r) {
                Some(new) => Self::Ref(ObjRef::new(new.get().cast_unsigned(), 0)),
                None => Self::Null,
            },
            Self::Array(items) => Self::Array(items.iter().map(|v| v.translate(map)).collect()),
            Self::Dict(entries) => Self::Dict(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), v.translate(map)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Writes the value; its references are taken to be references of the result.
    pub fn write(&self, obj: Obj<'_>) {
        match self {
            Self::Null => obj.primitive(Null),
            Self::Bool(b) => obj.primitive(*b),
            Self::Int(n) => obj.primitive(*n),
            Self::Real(n) => obj.primitive(*n),
            Self::Str(bytes) => obj.primitive(Str(bytes)),
            Self::Name(bytes) => obj.primitive(Name(bytes)),
            Self::Ref(r) => obj.primitive(Ref::new(r.obj_number.cast_signed())),
            Self::Array(items) => {
                let mut array = obj.array();
                for item in items {
                    item.write(array.push());
                }
            }
            Self::Dict(entries) => {
                let mut dict = obj.dict();
                for (key, value) in entries {
                    value.write(dict.insert(Name(key)));
                }
            }
        }
    }
}

fn spend(own: &mut usize, total: &mut usize) -> Result<(), Refused> {
    *own = own.checked_sub(1).ok_or(Refused::Nodes)?;
    *total = total.checked_sub(1).ok_or(Refused::Total)?;
    Ok(())
}

fn read_raw(
    raw: &MaybeRef<Object<'_>>,
    depth: usize,
    own: &mut usize,
    total: &mut usize,
) -> Result<Value, Refused> {
    spend(own, total)?;
    let object = match raw {
        MaybeRef::Ref(r) => return Ok(Value::Ref(*r)),
        MaybeRef::NotRef(object) => object,
    };
    Ok(match object {
        Object::Null(_) | Object::Stream(_) => Value::Null,
        Object::Boolean(b) => Value::Bool(*b),
        Object::Number(n) => {
            let float = n.as_f64();
            if float.fract() == 0.0 && float.abs() <= f64::from(i32::MAX) {
                Value::Int(float as i32)
            } else if (float as f32).is_finite() {
                Value::Real(float as f32)
            } else {
                return Err(Refused::Number);
            }
        }
        Object::String(s) => Value::Str(s.as_bytes().to_vec()),
        Object::Name(n) => Value::Name(n.as_ref().to_vec()),
        Object::Array(array) => {
            if depth >= MAX_VALUE_DEPTH {
                return Err(Refused::Deep);
            }
            let mut items = Vec::new();
            for item in array.raw_iter() {
                items.push(read_raw(&item, depth + 1, own, total)?);
            }
            Value::Array(items)
        }
        Object::Dict(dict) => {
            if depth >= MAX_VALUE_DEPTH {
                return Err(Refused::Deep);
            }
            let mut entries = Vec::new();
            for (key, item) in dict.entries() {
                spend(own, total)?;
                entries.push((
                    key.as_ref().to_vec(),
                    read_raw(&item, depth + 1, own, total)?,
                ));
            }
            Value::Dict(entries)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hayro_syntax::Pdf;
    use std::sync::Arc;

    /// A file whose object 5 is `body`, read back as a `Value`.
    fn value_of(body: &str) -> Result<Value, Refused> {
        let file = crate::pdf_edit_fixtures::hand_written_with_xref(
            &[
                (1, "<< /Type /Catalog /Pages 2 0 R /X 5 0 R >>".to_string()),
                (2, "<< /Type /Pages /Kids [] /Count 0 >>".to_string()),
                (3, "null".to_string()),
                (4, "null".to_string()),
                (5, body.to_string()),
            ],
            "",
        );
        let pdf = Pdf::new(Arc::new(file)).expect("a file");
        let catalog = pdf
            .xref()
            .get::<hayro_syntax::object::Dict<'_>>(pdf.xref().root_id())
            .unwrap();
        let raw = catalog.get_raw::<Object<'_>>(b"X").unwrap();
        // The catalog entry is a reference; read the object it names.
        let MaybeRef::Ref(r) = raw else {
            panic!("a reference")
        };
        let object = pdf.xref().get::<Object<'_>>(r.into()).unwrap();
        Value::read(&MaybeRef::NotRef(object), &mut Budget::default())
    }

    #[test]
    fn a_dictionary_is_read_with_its_references_and_written_with_new_ones() {
        let value =
            value_of("<< /A [1 2.5 (s) /N true null] /B 3 0 R /C << /D 4 0 R >> >>").unwrap();
        let mut refs = Vec::new();
        value.refs(&mut refs);
        assert_eq!(refs, [ObjRef::new(3, 0), ObjRef::new(4, 0)]);
        assert_eq!(
            value.get(b"A").and_then(Value::as_array).map(<[_]>::len),
            Some(6)
        );

        let mut pdf = hayro_write::pdf_writer::Pdf::new();
        let map = |r: ObjRef| (r.obj_number == 3).then(|| Ref::new(30));
        value.translate(&map).write(pdf.indirect(Ref::new(9)));
        let text = String::from_utf8_lossy(&pdf.finish()).into_owned();
        assert!(text.contains("/B 30 0 R"), "{text}");
        assert!(
            text.contains("/D null"),
            "a reference without a copy is null: {text}"
        );
        assert!(
            text.contains("2.5") && text.contains("(s)") && text.contains("/N"),
            "{text}"
        );
    }

    #[test]
    fn nesting_and_size_are_limited() {
        let deep = format!(
            "{}{}",
            "[".repeat(MAX_VALUE_DEPTH + 1),
            "]".repeat(MAX_VALUE_DEPTH + 1)
        );
        assert_eq!(value_of(&deep).unwrap_err(), Refused::Deep);
        let shallow = format!(
            "{}{}",
            "[".repeat(MAX_VALUE_DEPTH - 2),
            "]".repeat(MAX_VALUE_DEPTH - 2)
        );
        assert!(value_of(&shallow).is_ok());
        let wide = format!("[{}]", "0 ".repeat(MAX_VALUE_NODES));
        assert_eq!(value_of(&wide).unwrap_err(), Refused::Nodes);
        assert!(Refused::Deep
            .sentence("선택적 내용")
            .contains("선택적 내용"));
    }

    #[test]
    fn a_number_that_is_no_finite_real_is_refused() {
        // Sixty digits are no integer here and overflow a single-precision real: the writer would
        // have to write `inf`, which is not a number of the format.
        let huge = format!("[1 {}]", "9".repeat(60));
        assert_eq!(value_of(&huge).unwrap_err(), Refused::Number);
        assert!(value_of("[16777217 2147483647 -2147483648 3.5]").is_ok());
        assert!(Refused::Number.sentence("주석").contains("숫자"));
    }

    #[test]
    fn the_total_budget_is_shared_by_all_values() {
        let mut budget = Budget(10);
        let raw = MaybeRef::NotRef(Object::Boolean(true));
        for _ in 0..10 {
            assert!(Value::read(&raw, &mut budget).is_ok());
        }
        assert_eq!(Value::read(&raw, &mut budget).unwrap_err(), Refused::Total);
    }

    #[test]
    fn dictionary_entries_can_be_replaced_and_filtered() {
        let mut value = Value::Dict(vec![
            (b"Keep".to_vec(), Value::Int(1)),
            (b"Drop".to_vec(), Value::Int(2)),
        ]);
        value.retain_keys(|key| key == b"Keep");
        value.set(b"Keep", Value::Int(3));
        value.set(b"New", Value::Bool(true));
        assert_eq!(value.get(b"Keep"), Some(&Value::Int(3)));
        assert_eq!(value.get(b"Drop"), None);
        assert_eq!(value.get(b"New"), Some(&Value::Bool(true)));
    }
}
