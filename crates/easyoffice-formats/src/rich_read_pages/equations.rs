//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig; see THIRD_PARTY_NOTICES.md), src/drawable.rs
//!
//! Pages equation import: modern equations stored on `TSD.ImageArchive` (type 3005) via proto2
//! extension fields `100..=103` (LaTeX or MathML, direct string or nested submessage), plus
//! standalone `TSWP.EquationInfoArchive` (type 2015).

use super::{reference, Reader};
use crate::pages_proto::{read_varint, ArchiveObject, Message, WireValue};
use crate::pages_schema::{attachment, drawable, equation, ty};
use crate::rich_read_pages_image::{PageImage, CROPPED};
use crate::{EasyOfficeError, RichBlock, RichParagraph, RichRun, XmlSanitizer};

const MAX_SUPER_DEPTH: usize = 6;

fn check_equation_cap(len: usize) -> Result<(), EasyOfficeError> {
    if len > equation::MAX_EQUATION_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "Pages 방정식 문자열 크기 한도(64KiB) 초과".to_string(),
        ));
    }
    Ok(())
}

/// Whether `raw` starts with a length-delimited or varint protobuf field header rather than plain
/// equation text (`0x0A` = field 1 bytes, `0x12` = field 2 bytes, `0x08`/`0x10` = varint fields).
fn looks_like_nested_proto(raw: &[u8]) -> bool {
    let Some(&first) = raw.first() else {
        return false;
    };
    if matches!(first, 0x08 | 0x10 | 0x12 | 0x18 | 0x1A | 0x20 | 0x22) {
        return true;
    }
    if first == 0x0A {
        let mut pos = 1usize;
        if let Ok(len) = read_varint(raw, &mut pos) {
            if usize::try_from(len)
                .ok()
                .and_then(|n| pos.checked_add(n))
                .is_some_and(|end| end <= raw.len())
            {
                return true;
            }
        }
    }
    false
}

fn extract_candidate(parent: &Message<'_>, raw: &[u8]) -> Result<Option<String>, EasyOfficeError> {
    check_equation_cap(raw.len())?;
    if raw.is_empty() {
        return Ok(None);
    }
    if looks_like_nested_proto(raw) {
        if let Ok(sub) = parent.parse_sub(raw) {
            for field_no in equation::NESTED_FIELDS {
                if let Some(inner) = sub.bytes(field_no) {
                    check_equation_cap(inner.len())?;
                    if let Ok(s) = std::str::from_utf8(inner) {
                        let cleaned = clean_equation_text(s);
                        if !cleaned.is_empty() {
                            return Ok(Some(cleaned));
                        }
                    }
                }
            }
        }
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        let cleaned = clean_equation_text(s);
        if !cleaned.is_empty() {
            return Ok(Some(cleaned));
        }
    }
    Ok(None)
}

fn is_mathml(s: &str) -> bool {
    let t = s.trim_start();
    let rest = if let Some(after_decl) = t.strip_prefix("<?xml") {
        after_decl
            .find("?>")
            .map_or(t, |i| after_decl[i + 2..].trim_start())
    } else {
        t
    };
    rest.get(..5)
        .is_some_and(|head| head.eq_ignore_ascii_case("<math"))
}

fn strip_mathml_tags(xml: &str) -> String {
    let mut raw_text = String::with_capacity(xml.len());
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let tag_rest = &xml[i + 1..];
            // Stop before `<annotation` when presentation MathML already yielded visible tokens.
            if !raw_text.trim().is_empty()
                && tag_rest
                    .get(..10)
                    .is_some_and(|h| h.eq_ignore_ascii_case("annotation"))
            {
                if let Some(end_ann) = xml[i..].find("</annotation>") {
                    i += end_ann + "</annotation>".len();
                    continue;
                }
            }
            match xml[i..].find('>') {
                Some(close) => i += close + 1,
                None => break,
            }
        } else {
            let next_tag = xml[i..].find('<').map_or(xml.len(), |d| i + d);
            raw_text.push_str(&xml[i..next_tag]);
            i = next_tag;
        }
    }
    XmlSanitizer::unescape(&raw_text)
}

/// Cleans LaTeX or MathML equation text: strips MathML tags when present, removes control chars
/// and collapses line breaks to spaces.
pub(crate) fn clean_equation_text(source: &str) -> String {
    let body = if is_mathml(source) {
        strip_mathml_tags(source)
    } else {
        source.to_string()
    };
    let mut out = String::with_capacity(body.len());
    let mut prev_space = false;
    for ch in body.chars() {
        if matches!(ch, '\n' | '\r' | '\t' | '\u{2028}' | ' ') {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else if !ch.is_control() {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// Whether a `TSD.ImageArchive` (3005) carries any equation extension field (`100..=103`).
pub(crate) fn is_image_equation(msg: &Message<'_>) -> bool {
    msg.fields
        .iter()
        .any(|f| (equation::IMAGE_EXT_FIRST..=equation::IMAGE_EXT_LAST).contains(&f.number))
}

/// Equation source string of a `TSD.ImageArchive` (3005), enforcing the 64 KiB cap.
pub(crate) fn image_equation_text(msg: &Message<'_>) -> Result<Option<String>, EasyOfficeError> {
    for field in &msg.fields {
        if !(equation::IMAGE_EXT_FIRST..=equation::IMAGE_EXT_LAST).contains(&field.number) {
            continue;
        }
        if let WireValue::Bytes(raw) = field.value {
            if let Some(found) = extract_candidate(msg, raw)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

/// Equation source string of a standalone `TSWP.EquationInfoArchive` (2015), enforcing the 64 KiB cap.
pub(crate) fn standalone_equation_text(
    msg: &Message<'_>,
) -> Result<Option<String>, EasyOfficeError> {
    for field_no in equation::INFO_FIELDS {
        for raw in msg.all_bytes(field_no) {
            if let Some(found) = extract_candidate(msg, raw)? {
                return Ok(Some(found));
            }
        }
    }
    let mut cur = msg.clone();
    for _ in 0..MAX_SUPER_DEPTH {
        if let Some(desc) = cur.bytes(drawable::DESCRIPTION) {
            check_equation_cap(desc.len())?;
            if let Ok(s) = std::str::from_utf8(desc) {
                let cleaned = clean_equation_text(s);
                if !cleaned.is_empty() {
                    return Ok(Some(cleaned));
                }
            }
        }
        let Some(next) = cur.sub(drawable::GEOMETRY)? else {
            break;
        };
        cur = next;
    }
    Ok(None)
}

impl Reader<'_> {
    /// Resolves an inline text equation behind `att_id` (`2015`, or `2003` -> `2015` / `3005`
    /// without a supported PNG/JPEG raster). Returns `Ok(None)` when the attachment should go
    /// through block attachment handling instead.
    pub(super) fn inline_equation_at_attachment(
        &mut self,
        att_id: u64,
    ) -> Result<Option<String>, EasyOfficeError> {
        let target_id = match self.objects.get(att_id) {
            Some(o) if o.type_id == ty::EQUATION => att_id,
            Some(o) if o.type_id == ty::ATTACHMENT => {
                let Ok(m) = Message::parse(&o.payload) else {
                    return Ok(None);
                };
                let Some(did) = reference(&m, attachment::DRAWABLE) else {
                    return Ok(None);
                };
                did
            }
            _ => return Ok(None),
        };
        let Some(obj) = self.objects.get(target_id) else {
            return Ok(None);
        };
        match obj.type_id {
            ty::EQUATION => {
                let msg = Message::parse(&obj.payload)?;
                let Some(eq) = standalone_equation_text(&msg)? else {
                    return Ok(None);
                };
                self.text_budget.charge(eq.len(), "Pages 방정식 텍스트")?;
                Ok(Some(eq))
            }
            ty::IMAGE => {
                let msg = Message::parse(&obj.payload)?;
                if !is_image_equation(&msg) {
                    return Ok(None);
                }
                // Validate the 64 KiB cap even when a raster exists!
                let eq = image_equation_text(&msg)?;
                if self.images.has_supported_raster(obj) {
                    return Ok(None);
                }
                if let Some(eq) = eq {
                    self.text_budget.charge(eq.len(), "Pages 방정식 텍스트")?;
                    return Ok(Some(eq));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Reads a `3005` drawable that may be an equation or a plain picture.
    pub(super) fn read_picture_or_equation(
        &mut self,
        obj: &ArchiveObject,
        floating: bool,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        if let Ok(msg) = Message::parse(&obj.payload) {
            if is_image_equation(&msg) {
                let eq = image_equation_text(&msg)?;
                match self.images.read_with_fallback_alt(obj, eq.as_deref())? {
                    PageImage::Picture { image, cropped } => {
                        if cropped {
                            self.report_many(CROPPED, 1);
                        }
                        return Ok(vec![RichBlock::Image(image)]);
                    }
                    PageImage::Skipped(_) => {
                        if let Some(text) = eq {
                            self.text_budget.charge(text.len(), "Pages 방정식 텍스트")?;
                            return Ok(vec![RichBlock::Paragraph(RichParagraph {
                                runs: vec![RichRun::plain(text)],
                                ..RichParagraph::default()
                            })]);
                        }
                        let label = if floating {
                            "쪽 위의 개체: 방정식"
                        } else {
                            "방정식"
                        };
                        self.skip(label);
                        return Ok(Vec::new());
                    }
                }
            }
        }
        Ok(self.read_picture(obj)?.into_iter().collect())
    }

    /// Reads a standalone `2015` equation drawable (used by floating drawables and empty body attachments).
    pub(super) fn read_standalone_equation(
        &mut self,
        obj: &ArchiveObject,
        floating: bool,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let msg = Message::parse(&obj.payload)?;
        if let Some(text) = standalone_equation_text(&msg)? {
            self.text_budget.charge(text.len(), "Pages 방정식 텍스트")?;
            return Ok(vec![RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(text)],
                ..RichParagraph::default()
            })]);
        }
        if floating {
            self.skip("쪽 위의 개체: 방정식(형식 2015)");
        } else {
            self.skip("방정식(형식 2015)");
        }
        Ok(Vec::new())
    }
}
