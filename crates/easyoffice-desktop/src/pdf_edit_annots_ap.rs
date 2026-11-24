//! Deep recursive validation for carried `/AP` appearance Form XObject streams (PD-337).
//!
//! `hayro-write` copies a validated `/AP` stream and every object transitively reachable from it
//! via `ExtractionQuery::new_object`. Every key on Form XObjects, Image XObjects, `/Resources`,
//! `/Group`, `/SMask`, `/Pattern`, `/Shading`, `/Font`, `/ColorSpace`, `/Properties`, `/Metadata`,
//! and `/OC` is allowlisted or checked recursively with cycle, depth, and node bounds.

use crate::pdf_edit_annots_safe::{
    has_non_oc_keys, has_structural_type, is_forbidden_action_or_file_key, is_structural_dict,
    is_valid_ocg_dict, MAX_SAFE_NAME_BYTES, MAX_SUBDICT_DEPTH,
};
use hayro_syntax::object::{Dict, MaybeRef, Name, ObjRef, Object, Stream};
use hayro_syntax::Pdf as ReadPdf;

/// Deepest nesting and node visits allowed across an `/AP` Form XObject resource graph.
const MAX_AP_DEPTH: usize = 24;
const MAX_AP_NODES: usize = 65_536;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApRole {
    Any,
    TopForm,
    FormXObject,
    ImageXObject,
    XObjectEntry,
    Pattern,
    Group,
    SMaskAny,
    SMaskForm,
    Metadata,
    ProcSet,
    ColorSpace,
    SubDictEntry,
    Shading,
}

struct ApWalk<'a> {
    pdf: &'a ReadPdf,
    nodes: usize,
    active: Vec<ObjRef>,
}

pub(crate) fn is_safe_ap_stream(pdf: &ReadPdf, stream: &Stream<'_>, stream_ref: ObjRef) -> bool {
    let mut walk = ApWalk {
        pdf,
        nodes: 0,
        active: vec![stream_ref],
    };
    walk.stream_safe(stream, ApRole::TopForm, 0)
}

impl<'a> ApWalk<'a> {
    fn step(&mut self, depth: usize) -> bool {
        self.nodes += 1;
        self.nodes <= MAX_AP_NODES && depth <= MAX_AP_DEPTH
    }

    fn resolve_checked(
        &mut self,
        raw: &MaybeRef<Object<'a>>,
    ) -> Option<(Object<'a>, Option<ObjRef>)> {
        match raw {
            MaybeRef::NotRef(obj) => Some((obj.clone(), None)),
            MaybeRef::Ref(r) => {
                if self.active.contains(r) {
                    return None;
                }
                self.pdf
                    .xref()
                    .get::<Object<'a>>((*r).into())
                    .map(|obj| (obj, Some(*r)))
            }
        }
    }

    /// Resolves an optional `/Type`, `/Subtype`, or `/S` name (`Ok(None)` = absent or `Null`).
    fn opt_name(&mut self, dict: &Dict<'a>, key: &[u8]) -> Result<Option<Name<'a>>, ()> {
        let Some(raw) = dict.get_raw::<Object<'a>>(key) else {
            return Ok(None);
        };
        let Some((obj, _)) = self.resolve_checked(&raw) else {
            return Err(());
        };
        match obj {
            Object::Null(_) => Ok(None),
            Object::Name(name) => Ok(Some(name)),
            _ => Err(()),
        }
    }

    fn raw_safe(&mut self, raw: &MaybeRef<Object<'a>>, role: ApRole, depth: usize) -> bool {
        if !self.step(depth) {
            return false;
        }
        match raw {
            MaybeRef::Ref(r) => {
                if self.active.contains(r) {
                    return false;
                }
                let Some(obj) = self.pdf.xref().get::<Object<'a>>((*r).into()) else {
                    return false;
                };
                self.active.push(*r);
                let ok = self.object_safe(&obj, role, depth, true);
                self.active.pop();
                ok
            }
            MaybeRef::NotRef(obj) => self.object_safe(obj, role, depth, false),
        }
    }

    fn object_safe(
        &mut self,
        obj: &Object<'a>,
        role: ApRole,
        depth: usize,
        is_indirect: bool,
    ) -> bool {
        match obj {
            Object::Null(_) => true,
            Object::Boolean(_) | Object::Number(_) | Object::String(_) => {
                matches!(role, ApRole::Any)
            }
            Object::Name(name) => match role {
                ApRole::Any | ApRole::ColorSpace => true,
                ApRole::SMaskAny => name.as_ref() == b"None",
                _ => false,
            },
            Object::Array(arr) => match role {
                ApRole::Any | ApRole::ColorSpace => arr
                    .raw_iter()
                    .all(|item| self.raw_safe(&item, ApRole::Any, depth + 1)),
                ApRole::ProcSet => arr.raw_iter().all(|item| {
                    self.step(depth + 1)
                        && matches!(self.resolve_checked(&item), Some((Object::Name(_), _)))
                }),
                _ => false,
            },
            Object::Stream(stream) => {
                if !is_indirect {
                    return false;
                }
                match role {
                    ApRole::TopForm
                    | ApRole::FormXObject
                    | ApRole::ImageXObject
                    | ApRole::XObjectEntry
                    | ApRole::Pattern
                    | ApRole::SMaskForm
                    | ApRole::Metadata
                    | ApRole::Shading
                    | ApRole::Any => self.stream_safe(stream, role, depth),
                    _ => false,
                }
            }
            Object::Dict(dict) => match role {
                ApRole::Group
                | ApRole::SMaskAny
                | ApRole::Pattern
                | ApRole::SubDictEntry
                | ApRole::Shading
                | ApRole::Any => self.dict_safe(dict, role, depth, is_indirect),
                _ => false,
            },
        }
    }

    fn stream_safe(&mut self, stream: &Stream<'a>, role: ApRole, depth: usize) -> bool {
        let dict = stream.dict();
        if is_structural_dict(dict) {
            return false;
        }
        let Ok(ty) = self.opt_name(dict, b"Type") else {
            return false;
        };
        let Ok(sub) = self.opt_name(dict, b"Subtype") else {
            return false;
        };
        let ty_b = ty.as_ref().map(AsRef::as_ref);
        let sub_b = sub.as_ref().map(AsRef::as_ref);
        if matches!(
            sub_b,
            Some(b"PS" | b"3D" | b"Flash" | b"RichMedia" | b"Movie" | b"Sound" | b"Link")
        ) {
            return false;
        }

        let effective = match role {
            ApRole::XObjectEntry => match sub_b {
                Some(b"Form") => ApRole::FormXObject,
                Some(b"Image") => ApRole::ImageXObject,
                _ => return false,
            },
            ApRole::Any => match sub_b {
                Some(b"Form") => ApRole::FormXObject,
                Some(b"Image") => ApRole::ImageXObject,
                _ => ApRole::Any,
            },
            other => other,
        };

        match effective {
            ApRole::TopForm | ApRole::FormXObject | ApRole::SMaskForm => {
                let sub_ok = if effective == ApRole::TopForm {
                    matches!(sub_b, None | Some(b"Form"))
                } else {
                    sub_b == Some(b"Form")
                };
                if !matches!(ty_b, None | Some(b"XObject")) || !sub_ok {
                    return false;
                }
                dict.entries().all(|(key, item)| {
                    let kb = key.as_ref();
                    matches!(
                        kb,
                        b"Type"
                            | b"Subtype"
                            | b"BBox"
                            | b"Matrix"
                            | b"FormType"
                            | b"Resources"
                            | b"Group"
                            | b"OC"
                            | b"Filter"
                            | b"DecodeParms"
                            | b"DP"
                            | b"Length"
                            | b"Interpolate"
                            | b"Metadata"
                    ) && self.check_ap_field(kb, &item, depth + 1, false)
                })
            }
            ApRole::ImageXObject => {
                if !matches!(ty_b, None | Some(b"XObject")) || sub_b != Some(b"Image") {
                    return false;
                }
                dict.entries().all(|(key, item)| {
                    let kb = key.as_ref();
                    matches!(
                        kb,
                        b"Type"
                            | b"Subtype"
                            | b"Width"
                            | b"Height"
                            | b"ColorSpace"
                            | b"BitsPerComponent"
                            | b"Intent"
                            | b"ImageMask"
                            | b"Mask"
                            | b"Decode"
                            | b"Interpolate"
                            | b"Alternates"
                            | b"SMask"
                            | b"SMaskInData"
                            | b"Matte"
                            | b"Name"
                            | b"OC"
                            | b"Filter"
                            | b"DecodeParms"
                            | b"DP"
                            | b"Length"
                            | b"Metadata"
                    ) && self.check_ap_field(kb, &item, depth + 1, true)
                })
            }
            ApRole::Metadata => {
                matches!(ty_b, None | Some(b"Metadata"))
                    && matches!(sub_b, None | Some(b"XML"))
                    && dict.entries().all(|(key, item)| {
                        matches!(
                            key.as_ref(),
                            b"Type" | b"Subtype" | b"Filter" | b"DecodeParms" | b"DP" | b"Length"
                        ) && self.raw_safe(&item, ApRole::Any, depth + 1)
                    })
            }
            ApRole::Pattern => {
                matches!(ty_b, None | Some(b"Pattern"))
                    && matches!(dict.get::<i32>(b"PatternType"), Some(1))
                    && dict.entries().all(|(key, item)| {
                        let kb = key.as_ref();
                        matches!(
                            kb,
                            b"Type"
                                | b"PatternType"
                                | b"PaintType"
                                | b"TilingType"
                                | b"BBox"
                                | b"XStep"
                                | b"YStep"
                                | b"Resources"
                                | b"Matrix"
                                | b"Filter"
                                | b"DecodeParms"
                                | b"DP"
                                | b"Length"
                                | b"Metadata"
                        ) && self.check_ap_field(kb, &item, depth + 1, false)
                    })
            }
            ApRole::Shading => {
                matches!(ty_b, None | Some(b"Shading"))
                    && sub_b.is_none()
                    && dict
                        .get::<i32>(b"ShadingType")
                        .is_some_and(|t| (1..=7).contains(&t))
                    && dict.entries().all(|(key, item)| {
                        let kb = key.as_ref();
                        !is_forbidden_action_or_file_key(kb)
                            && kb != b"S"
                            && self.check_ap_field(kb, &item, depth + 1, false)
                    })
            }
            ApRole::Any => {
                if matches!(
                    ty_b,
                    Some(
                        b"EmbeddedFile"
                            | b"ObjStm"
                            | b"XRef"
                            | b"Sig"
                            | b"Metadata"
                            | b"Group"
                            | b"Mask"
                            | b"OCG"
                            | b"OCMD"
                            | b"Pattern"
                            | b"XObject"
                            | b"3D"
                            | b"Movie"
                            | b"Sound"
                            | b"Rendition"
                            | b"Collection"
                            | b"RichMedia"
                            | b"Font"
                            | b"FontDescriptor"
                            | b"ExtGState"
                    )
                ) {
                    return false;
                }
                dict.entries().all(|(key, item)| {
                    let kb = key.as_ref();
                    !is_forbidden_action_or_file_key(kb)
                        && kb != b"S"
                        && self.check_ap_field(kb, &item, depth + 1, false)
                })
            }
            _ => false,
        }
    }

    fn check_ap_field(
        &mut self,
        kb: &[u8],
        item: &MaybeRef<Object<'a>>,
        depth: usize,
        image_smask: bool,
    ) -> bool {
        match kb {
            b"Resources" => self.resources_safe(item, depth),
            b"Group" => self.raw_safe(item, ApRole::Group, depth),
            b"OC" => self.oc_safe(item, depth),
            b"Metadata" => self.raw_safe(item, ApRole::Metadata, depth),
            b"ColorSpace" if image_smask => self.raw_safe(item, ApRole::ColorSpace, depth),
            b"SMask" => {
                let role = if image_smask {
                    ApRole::ImageXObject
                } else {
                    ApRole::SMaskAny
                };
                self.raw_safe(item, role, depth)
            }
            b"Mask" if image_smask => match self.resolve_checked(item) {
                Some((Object::Stream(_), _)) => self.raw_safe(item, ApRole::ImageXObject, depth),
                Some((Object::Null(_) | Object::Array(_), _)) => {
                    self.raw_safe(item, ApRole::Any, depth)
                }
                _ => false,
            },
            b"CharProcs" => self.char_procs_safe(item, depth),
            _ => self.raw_safe(item, ApRole::Any, depth),
        }
    }

    fn char_procs_safe(&mut self, raw: &MaybeRef<Object<'a>>, depth: usize) -> bool {
        if !self.step(depth) {
            return false;
        }
        let Some((obj, opt_ref)) = self.resolve_checked(raw) else {
            return false;
        };
        if let Some(r) = opt_ref {
            self.active.push(r);
        }
        let ok = match obj {
            Object::Null(_) => true,
            Object::Dict(procs) => {
                !has_structural_type(&procs)
                    && procs.entries().all(|(_, item)| {
                        self.step(depth + 1)
                            && match self.resolve_checked(&item) {
                                Some((Object::Null(_), _)) => true,
                                Some((Object::Stream(stream), Some(sr))) => {
                                    self.active.push(sr);
                                    let s_ok = self.stream_safe(&stream, ApRole::Any, depth + 1);
                                    self.active.pop();
                                    s_ok
                                }
                                _ => false,
                            }
                    })
            }
            _ => false,
        };
        if opt_ref.is_some() {
            self.active.pop();
        }
        ok
    }

    fn resources_safe(&mut self, raw: &MaybeRef<Object<'a>>, depth: usize) -> bool {
        if !self.step(depth) {
            return false;
        }
        let Some((obj, opt_ref)) = self.resolve_checked(raw) else {
            return false;
        };
        if let Some(r) = opt_ref {
            self.active.push(r);
        }
        let ok = match obj {
            Object::Null(_) => true,
            Object::Dict(res) => self.resources_dict_safe(&res, depth),
            _ => false,
        };
        if opt_ref.is_some() {
            self.active.pop();
        }
        ok
    }

    fn resources_dict_safe(&mut self, res: &Dict<'a>, depth: usize) -> bool {
        if is_structural_dict(res) {
            return false;
        }
        for (rk, rval) in res.entries() {
            let kb = rk.as_ref();
            if kb == b"ProcSet" {
                if !self.raw_safe(&rval, ApRole::ProcSet, depth + 1) {
                    return false;
                }
                continue;
            }
            let entry_role = match kb {
                b"XObject" => ApRole::XObjectEntry,
                b"Pattern" => ApRole::Pattern,
                b"ColorSpace" => ApRole::ColorSpace,
                b"Shading" => ApRole::Shading,
                b"ExtGState" | b"Font" | b"Properties" => ApRole::SubDictEntry,
                _ => return false,
            };
            if !self.step(depth + 1) {
                return false;
            }
            let Some((sub_obj, opt_ref)) = self.resolve_checked(&rval) else {
                return false;
            };
            if let Some(r) = opt_ref {
                self.active.push(r);
            }
            let sub_ok = match sub_obj {
                Object::Null(_) => true,
                Object::Dict(sub) => {
                    !has_structural_type(&sub)
                        && sub
                            .entries()
                            .all(|(_, item)| self.raw_safe(&item, entry_role, depth + 2))
                }
                _ => false,
            };
            if opt_ref.is_some() {
                self.active.pop();
            }
            if !sub_ok {
                return false;
            }
        }
        true
    }

    fn dict_safe(
        &mut self,
        dict: &Dict<'a>,
        role: ApRole,
        depth: usize,
        is_indirect: bool,
    ) -> bool {
        if is_structural_dict(dict)
            || dict
                .keys()
                .any(|k| is_forbidden_action_or_file_key(k.as_ref()))
        {
            return false;
        }
        let Ok(ty) = self.opt_name(dict, b"Type") else {
            return false;
        };
        let ty_b = ty.as_ref().map(AsRef::as_ref);
        let Ok(s_val) = self.opt_name(dict, b"S") else {
            return false;
        };
        let s_b = s_val.as_ref().map(AsRef::as_ref);

        match role {
            ApRole::Group => {
                matches!(ty_b, None | Some(b"Group"))
                    && s_b == Some(b"Transparency")
                    && dict.entries().all(|(key, item)| {
                        let kb = key.as_ref();
                        matches!(kb, b"Type" | b"S" | b"CS" | b"I" | b"K")
                            && self.check_ap_field(kb, &item, depth + 1, false)
                    })
            }
            ApRole::SMaskAny => {
                if !matches!(ty_b, None | Some(b"Mask"))
                    || !matches!(s_b, Some(b"Alpha" | b"Luminosity"))
                    || !dict.contains_key(b"G")
                {
                    return false;
                }
                dict.entries().all(|(key, item)| {
                    let kb = key.as_ref();
                    if !matches!(kb, b"Type" | b"S" | b"G" | b"BC" | b"TR") {
                        return false;
                    }
                    if kb == b"G" {
                        self.raw_safe(&item, ApRole::SMaskForm, depth + 1)
                    } else {
                        self.check_ap_field(kb, &item, depth + 1, false)
                    }
                })
            }
            ApRole::Pattern => {
                matches!(ty_b, None | Some(b"Pattern"))
                    && matches!(dict.get::<i32>(b"PatternType"), Some(2))
                    && s_b.is_none()
                    && dict.entries().all(|(key, item)| {
                        let kb = key.as_ref();
                        matches!(
                            kb,
                            b"Type" | b"PatternType" | b"Shading" | b"Matrix" | b"ExtGState"
                        ) && self.check_ap_field(kb, &item, depth + 1, false)
                    })
            }
            ApRole::SubDictEntry | ApRole::Shading | ApRole::Any => {
                if s_b.is_some() {
                    return false;
                }
                if ty_b == Some(b"OCG") {
                    let ocg_ref = is_indirect.then(|| self.active.last().copied()).flatten();
                    return is_valid_ocg_dict(self.pdf, dict, ocg_ref);
                }
                if ty_b == Some(b"OCMD") {
                    return self.ocmd_dict_safe(dict, depth);
                }
                dict.entries().all(|(key, item)| {
                    let kb = key.as_ref();
                    kb != b"S" && self.check_ap_field(kb, &item, depth + 1, false)
                })
            }
            _ => false,
        }
    }

    fn oc_safe(&mut self, raw: &MaybeRef<Object<'a>>, depth: usize) -> bool {
        if !self.step(depth) {
            return false;
        }
        match raw {
            MaybeRef::NotRef(Object::Null(_)) => true,
            MaybeRef::NotRef(Object::Dict(d)) => self.ocmd_dict_safe(d, depth),
            MaybeRef::Ref(r) => {
                if self.active.contains(r) {
                    return false;
                }
                let Some(Object::Dict(d)) = self.pdf.xref().get::<Object<'a>>((*r).into()) else {
                    return false;
                };
                self.active.push(*r);
                let ok =
                    is_valid_ocg_dict(self.pdf, &d, Some(*r)) || self.ocmd_dict_safe(&d, depth);
                self.active.pop();
                ok
            }
            _ => false,
        }
    }

    fn ocmd_dict_safe(&mut self, dict: &Dict<'a>, depth: usize) -> bool {
        if has_non_oc_keys(dict)
            || dict
                .keys()
                .any(|k| !matches!(k.as_ref(), b"Type" | b"OCGs" | b"P" | b"VE"))
        {
            return false;
        }
        match self.opt_name(dict, b"Type") {
            Ok(None) => {}
            Ok(Some(n)) if n.as_ref() == b"OCMD" => {}
            _ => return false,
        }
        if self.opt_name(dict, b"P").is_err() {
            return false;
        }
        let mut has_target = false;
        if let Some(raw_ocgs) = dict.get_raw::<Object<'a>>(b"OCGs") {
            match raw_ocgs {
                MaybeRef::Ref(r) if self.valid_ocg_ref(r) => has_target = true,
                MaybeRef::NotRef(Object::Array(arr)) => {
                    for item in arr.raw_iter() {
                        if !self.step(depth + 1) {
                            return false;
                        }
                        let MaybeRef::Ref(r) = item else {
                            return false;
                        };
                        if !self.valid_ocg_ref(r) {
                            return false;
                        }
                        has_target = true;
                    }
                }
                MaybeRef::NotRef(Object::Null(_)) => {}
                _ => return false,
            }
        }
        if let Some(raw_ve) = dict.get_raw::<Object<'a>>(b"VE") {
            if !self.ve_safe(&raw_ve, 0) {
                return false;
            }
            has_target = true;
        }
        has_target
    }

    fn valid_ocg_ref(&self, r: ObjRef) -> bool {
        !self.active.contains(&r)
            && matches!(
                self.pdf.xref().get::<Object<'a>>(r.into()),
                Some(Object::Dict(d)) if is_valid_ocg_dict(self.pdf, &d, Some(r))
            )
    }

    fn ve_safe(&mut self, raw: &MaybeRef<Object<'a>>, ve_depth: usize) -> bool {
        if ve_depth > MAX_SUBDICT_DEPTH || !self.step(ve_depth + 1) {
            return false;
        }
        let Some((Object::Array(arr), opt_ref)) = self.resolve_checked(raw) else {
            return false;
        };
        if let Some(r) = opt_ref {
            self.active.push(r);
        }
        let mut count = 0usize;
        let ok = arr.raw_iter().all(|item| {
            count += 1;
            self.step(ve_depth + 2)
                && match item {
                    MaybeRef::Ref(r) => self.valid_ocg_ref(r),
                    MaybeRef::NotRef(Object::Name(n)) => n.as_ref().len() <= MAX_SAFE_NAME_BYTES,
                    MaybeRef::NotRef(Object::Array(_)) => self.ve_safe(&item, ve_depth + 1),
                    _ => false,
                }
        }) && count > 0;
        if opt_ref.is_some() {
            self.active.pop();
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use crate::pdf_edit_annots_safe::tests::{empty_stream, first_annot, form_stream, page_doc};
    use crate::pdf_edit_carry_tests::rebuilt;
    use crate::pdf_edit_fixtures::read;
    use hayro_syntax::object::{Dict, Stream};

    #[test]
    fn ap_stream_and_resources_smuggling_is_blocked_at_every_level() {
        let cases: Vec<(&str, Vec<(u32, String)>)> = vec![
            ("extra key on form stream", vec![(6, form_stream("/X 3 0 R"))]),
            ("Group ref to Page 3", vec![(6, form_stream("/Group 3 0 R"))]),
            (
                "Group missing /S",
                vec![(6, form_stream("/Group << /Type /Group >>"))],
            ),
            (
                "Group Transparency smuggling Page 3",
                vec![(6, form_stream("/Group << /Type /Group /S /Transparency /CS 3 0 R >>"))],
            ),
            (
                "Group action /S /Launch",
                vec![(6, form_stream("/Group << /S /Launch /F (/bin/sh) >>"))],
            ),
            ("Metadata ref to Page 3", vec![(6, form_stream("/Metadata 3 0 R"))]),
            (
                "OC omitted Type action on Form",
                vec![
                    (6, form_stream("/OC 7 0 R")),
                    (7, "<< /Name (Evil) /S /Launch /F (/bin/sh) >>".to_string()),
                ],
            ),
            ("Resources ref to Page 3", vec![(6, form_stream("/Resources 3 0 R"))]),
            (
                "Resources non-standard key",
                vec![(6, form_stream("/Resources << /Evil 3 0 R >>"))],
            ),
            (
                "Resources Font ref to Page 3",
                vec![(6, form_stream("/Resources << /Font 3 0 R >>"))],
            ),
            (
                "Resources Font ref to action dict",
                vec![
                    (6, form_stream("/Resources << /Font 7 0 R >>")),
                    (7, "<< /S /Launch /F (/bin/sh) >>".to_string()),
                ],
            ),
            (
                "Resources ProcSet array ref to Page 3",
                vec![(6, form_stream("/Resources << /ProcSet [3 0 R] >>"))],
            ),
            (
                "Font Subtype ref to Page 3",
                vec![
                    (6, form_stream("/Resources << /Font << /F1 7 0 R >> >>")),
                    (7, "<< /Type /Font /Subtype 3 0 R >>".to_string()),
                ],
            ),
            (
                "Properties Subtype inline action dict",
                vec![(6, form_stream("/Resources << /Properties << /MC0 << /Subtype << /S /Launch /F (/bin/sh) >> >> >> >>"))],
            ),
            (
                "ExtGState Subtype array ref to Page 3",
                vec![(6, form_stream("/Resources << /ExtGState << /GS1 << /Subtype [3 0 R] >> >> >>"))],
            ),
            (
                "Font -> FontDescriptor -> FontFile2 smuggling /F",
                vec![
                    (6, form_stream("/Resources << /Font << /F1 7 0 R >> >>")),
                    (7, "<< /Type /Font /Subtype /TrueType /FontDescriptor 8 0 R >>".to_string()),
                    (8, "<< /Type /FontDescriptor /FontName /Evil /FontFile2 9 0 R >>".to_string()),
                    (9, empty_stream("/F (/etc/passwd)")),
                ],
            ),
            (
                "ColorSpace ICCBased stream smuggling Page 3",
                vec![
                    (6, form_stream("/Resources << /ColorSpace << /CS1 [/ICCBased 7 0 R] >> >>")),
                    (7, empty_stream("/N 3 /Evil 3 0 R")),
                ],
            ),
            (
                "ExtGState SMask /G pointing to Page 3",
                vec![(6, form_stream("/Resources << /ExtGState << /GS1 << /Type /ExtGState /SMask << /Type /Mask /S /Alpha /G 3 0 R >> >> >> >>"))],
            ),
            (
                "ExtGState SMask /S /Launch",
                vec![
                    (6, form_stream("/Resources << /ExtGState << /GS1 << /SMask << /S /Launch /F (/bin/sh) /G 7 0 R >> >> >> >>")),
                    (7, form_stream("")),
                ],
            ),
            (
                "ExtGState SMask /G nested Form smuggling",
                vec![
                    (6, form_stream("/Resources << /ExtGState << /GS1 << /SMask << /S /Alpha /G 7 0 R >> >> >> >>")),
                    (7, form_stream("/Resources << /Font << /F1 3 0 R >> >>")),
                ],
            ),
            (
                "XObject PostScript subtype",
                vec![
                    (6, form_stream("/Resources << /XObject << /PS1 7 0 R >> >>")),
                    (7, empty_stream("/Type /XObject /Subtype /PS")),
                ],
            ),
            (
                "Image XObject SMask pointing to Page 3",
                vec![
                    (6, form_stream("/Resources << /XObject << /Im1 7 0 R >> >>")),
                    (7, "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /SMask 3 0 R /Length 1 >>\nstream\nx\nendstream".to_string()),
                ],
            ),
            (
                "Image XObject Mask dict smuggling",
                vec![
                    (6, form_stream("/Resources << /XObject << /Im1 7 0 R >> >>")),
                    (7, "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /Mask << /S /Launch >> /Length 1 >>\nstream\nx\nendstream".to_string()),
                ],
            ),
            (
                "Image XObject ColorSpace dict smuggling",
                vec![
                    (6, form_stream("/Resources << /XObject << /Im1 7 0 R >> >>")),
                    (7, "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace << /Evil 3 0 R >> /Length 1 >>\nstream\nx\nendstream".to_string()),
                ],
            ),
            (
                "Shading stream PostScript subtype",
                vec![
                    (6, form_stream("/Resources << /Shading << /Sh1 7 0 R >> >>")),
                    (7, empty_stream("/ShadingType 4 /ColorSpace /DeviceRGB /Subtype /PS")),
                ],
            ),
            (
                "Shading stream disguised Form XObject",
                vec![
                    (6, form_stream("/Resources << /Shading << /Sh1 7 0 R >> >>")),
                    (7, empty_stream("/Type /XObject /Subtype /Form /Evil 3 0 R")),
                ],
            ),
            (
                "PatternType 1 stream smuggling in Resources",
                vec![
                    (6, form_stream("/Resources << /Pattern << /P1 7 0 R >> >>")),
                    (7, empty_stream("/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 5 5] /XStep 5 /YStep 5 /Resources << /Font 3 0 R >>")),
                ],
            ),
            (
                "PatternType 2 ExtGState SMask smuggling Page 3",
                vec![
                    (6, form_stream("/Resources << /Pattern << /P2 7 0 R >> >>")),
                    (7, "<< /Type /Pattern /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 1] /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> /ExtGState << /Type /ExtGState /SMask << /Type /Mask /S /Alpha /G 3 0 R >> >> >>".to_string()),
                ],
            ),
            (
                "Properties URI action dict",
                vec![(6, form_stream("/Resources << /Properties << /MC0 << /S /URI /URI (https://evil.example) >> >> >>"))],
            ),
            (
                "Type3 CharProcs smuggling Page 3",
                vec![
                    (6, form_stream("/Resources << /Font << /T3 7 0 R >> >>")),
                    (7, "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 10 10] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /A 3 0 R >> >>".to_string()),
                ],
            ),
            (
                "Cyclic nested Form XObject",
                vec![(6, form_stream("/Resources << /XObject << /Self 6 0 R >> >>"))],
            ),
        ];

        for (label, extra) in cases {
            let mut objs = vec![(
                5,
                "<< /Type /Annot /Subtype /Square /Rect [10 10 50 50] /AP << /N 6 0 R >> >>"
                    .to_string(),
            )];
            objs.extend(extra);
            let built = rebuilt(page_doc("", "", "5 0 R", objs));
            assert!(built.warnings.is_empty(), "{label}: {:?}", built.warnings);
            let out = read(&built.bytes);
            assert!(
                !first_annot(&out).contains_key(b"AP"),
                "{label}: /AP must be stripped"
            );
            assert!(
                out.objects().into_iter().count() < 16,
                "{label}: page tree leaked"
            );
        }
    }

    #[test]
    fn valid_nested_ap_resources_and_pattern_type2_survive_preservation() {
        let extra = vec![
            (5, "<< /Type /Annot /Subtype /Square /Rect [10 10 80 80] /AP << /N 6 0 R >> >>".to_string()),
            (6, form_stream("/Group << /Type /Group /S /Transparency /CS /DeviceRGB >> /OC 15 0 R \
                /Resources << /ProcSet [/PDF /Text /ImageC] /ColorSpace << /CS1 [/ICCBased 7 0 R] >> \
                /Font << /F 8 0 R /S 8 0 R /A 18 0 R >> /ExtGState << /GS1 11 0 R /GS2 19 0 R >> \
                /XObject << /Im1 13 0 R /Fm1 12 0 R >> /Pattern << /P1 16 0 R /P2 20 0 R >> \
                /Shading << /Sh1 17 0 R >> /Properties << /OC0 15 0 R >> >>")),
            (7, "<< /N 3 /Alternate /DeviceRGB /Length 3 >>\nstream\nrgb\nendstream".to_string()),
            (8, "<< /Type /Font /Subtype /TrueType /BaseFont /Test /FontDescriptor 9 0 R /ToUnicode 10 0 R >>".to_string()),
            (9, "<< /Type /FontDescriptor /FontName /Test /Flags 4 /FontBBox [0 0 100 100] /FontFile2 10 0 R >>".to_string()),
            (10, "<< /Length 4 >>\nstream\nfont\nendstream".to_string()),
            (11, "<< /Type /ExtGState /ca 0.5 /SMask << /Type /Mask /S /Alpha /G 12 0 R >> >>".to_string()),
            (12, form_stream("")),
            (13, "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace [/ICCBased 7 0 R] /Mask [0 255] /SMask 14 0 R /Length 3 >>\nstream\nabc\nendstream".to_string()),
            (14, "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /Length 1 >>\nstream\na\nendstream".to_string()),
            (15, "<< /Type /OCG /Name (Layer) >>".to_string()),
            (16, empty_stream("/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources << /ProcSet [/PDF] >>")),
            (17, "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 10 10] /Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >> >>".to_string()),
            (18, "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 10 10] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /A 10 0 R /S 10 0 R >> >>".to_string()),
            (19, "<< /Type /ExtGState /SMask /None >>".to_string()),
            (20, "<< /Type /Pattern /PatternType 2 /Shading 17 0 R /ExtGState 19 0 R >>".to_string()),
        ];
        let built = rebuilt(page_doc(
            "/OCProperties << /OCGs [15 0 R] /D << /ON [15 0 R] >> >>",
            "",
            "5 0 R",
            extra,
        ));
        assert!(built.warnings.is_empty(), "{:?}", built.warnings);
        let out = read(&built.bytes);
        let ap = first_annot(&out)
            .get::<Dict<'_>>(b"AP")
            .and_then(|ap| ap.get::<Stream<'_>>(b"N"))
            .expect("nested AP form preserved");
        let res = ap.dict().get::<Dict<'_>>(b"Resources").unwrap();
        for cat in [
            b"ProcSet".as_slice(),
            b"ColorSpace",
            b"Font",
            b"ExtGState",
            b"XObject",
            b"Pattern",
            b"Shading",
            b"Properties",
        ] {
            assert!(res.contains_key(cat), "{}", String::from_utf8_lossy(cat));
        }
        assert!(ap.dict().contains_key(b"Group") && ap.dict().contains_key(b"OC"));
    }

    #[test]
    fn ap_depth_and_node_bounds_reject_excessive_graphs_fail_closed() {
        // 7 nested Form XObjects stay within MAX_AP_DEPTH (24); 9 nested exceed it.
        let nested_forms = |count: usize| -> Vec<(u32, String)> {
            let mut objs = vec![(
                5,
                "<< /Type /Annot /Subtype /Square /Rect [10 10 50 50] /AP << /N 6 0 R >> >>"
                    .to_string(),
            )];
            for i in 0..count {
                let id = 6 + i as u32;
                if i + 1 < count {
                    objs.push((
                        id,
                        form_stream(&format!(
                            "/Resources << /XObject << /F {} 0 R >> >>",
                            id + 1
                        )),
                    ));
                } else {
                    objs.push((id, form_stream("")));
                }
            }
            objs
        };
        let ok_depth = rebuilt(page_doc("", "", "5 0 R", nested_forms(8)));
        assert!(first_annot(&read(&ok_depth.bytes)).contains_key(b"AP"));
        let too_deep = rebuilt(page_doc("", "", "5 0 R", nested_forms(9)));
        assert!(!first_annot(&read(&too_deep.bytes)).contains_key(b"AP"));

        // Shallow diamond DAG in /Resources /ColorSpace exceeding MAX_AP_NODES (65_536).
        let dag_graph = |levels: usize| -> Vec<(u32, String)> {
            let mut objs =
                vec![
                (
                    5,
                    "<< /Type /Annot /Subtype /Square /Rect [10 10 50 50] /AP << /N 6 0 R >> >>"
                        .to_string(),
                ),
                (6, form_stream("/Resources << /ColorSpace << /CS1 7 0 R >> >>")),
            ];
            for i in 0..levels {
                let id = 7 + i as u32;
                let next = id + 1;
                objs.push((id, format!("[{next} 0 R {next} 0 R]")));
            }
            objs.push((7 + levels as u32, "/DeviceRGB".to_string()));
            objs
        };
        let ok_nodes = rebuilt(page_doc("", "", "5 0 R", dag_graph(14)));
        assert!(first_annot(&read(&ok_nodes.bytes)).contains_key(b"AP"));
        let too_many_nodes = rebuilt(page_doc("", "", "5 0 R", dag_graph(17)));
        assert!(!first_annot(&read(&too_many_nodes.bytes)).contains_key(b"AP"));
    }
}
