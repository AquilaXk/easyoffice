//! Preview picture inside a saved document (PD-132).
//!
//! File managers and recent-document lists show the picture a program stored in the file itself:
//! the `PrvImage` stream of an HWP 5.0 OLE2 container, `Preview/PrvImage.png` of an HWPX package
//! (KS X 6101) and `docProps/thumbnail.png` of an OOXML package, which the package root points to
//! with a `metadata/thumbnail` relationship (ECMA-376 Part 2 section 10.1.3.2). The engine draws no
//! pages, so it never makes the picture: the caller hands over PNG bytes and this module puts them
//! in the file. Every input is checked before anything is parsed (Fail-Closed): the picture must be
//! a structurally valid PNG within the byte and pixel limits, and the document within the size the
//! rewrite is allowed to hold in memory.

use crate::rich_image_probe::probe_png;
use crate::{
    DocumentFormat, EasyOfficeError, Ole2ContainerCodec, ZipArchiveCodec, ZipArchiveEntry,
    ZipSecurityPolicy, HANGUL_EDITOR_MAX_ENTRY_BYTES,
};

/// Largest picture accepted (encoded bytes). Half of the reader's limit for a start-window card,
/// so a picture this module wrote is always shown.
pub const MAX_THUMBNAIL_PNG_BYTES: usize = 128 * 1024;
/// Longest side accepted, in pixels. File managers scale a stored preview to what they show; a
/// larger picture only grows every saved file.
pub const MAX_THUMBNAIL_SIDE_PX: u32 = 256;
/// Largest document the rewrite handles. Unpacking holds the decoded parts next to the packed
/// result, so a very large document keeps the picture it already has (the caller saves it as it
/// is) instead of tripling its memory use.
pub const MAX_EMBED_DOCUMENT_BYTES: usize = 64 * 1_048_576;

const HWP_PREVIEW_STREAM: &str = "PrvImage";
const HWPX_PREVIEW_ENTRY: &str = "Preview/PrvImage.png";
const OOXML_THUMBNAIL_ENTRY: &str = "docProps/thumbnail.png";
const OOXML_CONTENT_TYPES: &str = "[Content_Types].xml";
const OOXML_ROOT_RELS: &str = "_rels/.rels";
const OOXML_THUMBNAIL_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";
const PNG_CONTENT_TYPE_DEFAULT: &str = "<Default Extension=\"png\" ContentType=\"image/png\"/>";
/// Relationship id of a thumbnail the module adds (a number follows when the id is taken).
const OOXML_THUMBNAIL_REL_ID: &str = "rIdThumbnail";

/// Whether [`embed_thumbnail`] writes a preview picture into files of `format`.
pub fn supports_thumbnail(format: DocumentFormat) -> bool {
    matches!(
        format,
        DocumentFormat::Hwp
            | DocumentFormat::Hwt
            | DocumentFormat::Hwpx
            | DocumentFormat::Hwtx
            | DocumentFormat::Docx
    )
}

fn invalid(detail: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(detail.to_string())
}

/// Requires a structurally valid PNG (every chunk CRC) within the byte and pixel limits.
pub fn check_thumbnail_png(png: &[u8]) -> Result<(), EasyOfficeError> {
    if png.len() > MAX_THUMBNAIL_PNG_BYTES {
        return Err(EasyOfficeError::PayloadTooLarge {
            actual: png.len(),
            max: MAX_THUMBNAIL_PNG_BYTES,
        });
    }
    let header = probe_png(png)?;
    let side = header.width.max(header.height);
    if header.width == 0 || header.height == 0 || side > MAX_THUMBNAIL_SIDE_PX {
        return Err(invalid("미리보기 그림의 크기가 허용 범위를 벗어났습니다."));
    }
    Ok(())
}

/// Returns `document` with `png` as its preview picture, replacing the one it has. Nothing else
/// in the document changes; a document the format's own reader would refuse is an error.
pub fn embed_thumbnail(
    format: DocumentFormat,
    document: &[u8],
    png: &[u8],
) -> Result<Vec<u8>, EasyOfficeError> {
    check_thumbnail_png(png)?;
    if !supports_thumbnail(format) {
        return Err(EasyOfficeError::UnsupportedExtension(
            format.extension().to_string(),
        ));
    }
    if document.len() > MAX_EMBED_DOCUMENT_BYTES {
        return Err(EasyOfficeError::PayloadTooLarge {
            actual: document.len(),
            max: MAX_EMBED_DOCUMENT_BYTES,
        });
    }
    match format {
        DocumentFormat::Hwp | DocumentFormat::Hwt => embed_hwp(document, png),
        DocumentFormat::Hwpx | DocumentFormat::Hwtx => embed_hwpx(document, png),
        _ => embed_docx(document, png),
    }
}

/// Replaces the entry named `name` (CFB and ZIP names are compared without regard to case by the
/// readers that matter here) or appends it.
fn upsert<T>(items: &mut Vec<T>, name: &str, item: T, name_of: impl Fn(&T) -> &str) {
    match items
        .iter_mut()
        .find(|existing| name_of(existing).eq_ignore_ascii_case(name))
    {
        Some(existing) => *existing = item,
        None => items.push(item),
    }
}

fn embed_hwp(document: &[u8], png: &[u8]) -> Result<Vec<u8>, EasyOfficeError> {
    let mut streams: Vec<(String, Vec<u8>)> = Ole2ContainerCodec::unpack(document)?
        .into_iter()
        .map(|stream| (stream.name, stream.data))
        .collect();
    upsert(
        &mut streams,
        HWP_PREVIEW_STREAM,
        (HWP_PREVIEW_STREAM.to_string(), png.to_vec()),
        |stream| &stream.0,
    );
    Ole2ContainerCodec::pack(&streams)
}

/// The policy the Hangul editor's own reader applies (no ratio rule, its entry limit): the same
/// one a save is verified with, so a package the editor wrote can be rewritten.
fn hangul_zip_policy() -> ZipSecurityPolicy {
    ZipSecurityPolicy {
        max_compression_ratio: f64::INFINITY,
        max_single_entry_uncompressed_bytes: HANGUL_EDITOR_MAX_ENTRY_BYTES,
        ..ZipSecurityPolicy::default()
    }
}

fn embed_hwpx(document: &[u8], png: &[u8]) -> Result<Vec<u8>, EasyOfficeError> {
    let mut entries = ZipArchiveCodec::unpack(document, &hangul_zip_policy())?;
    upsert(
        &mut entries,
        HWPX_PREVIEW_ENTRY,
        ZipArchiveEntry::new(HWPX_PREVIEW_ENTRY, png.to_vec()),
        |entry| &entry.path,
    );
    ZipArchiveCodec::pack_compressed(&entries)
}

fn embed_docx(document: &[u8], png: &[u8]) -> Result<Vec<u8>, EasyOfficeError> {
    let mut entries = ZipArchiveCodec::unpack(document, &ZipSecurityPolicy::default())?;
    add_ooxml_thumbnail(&mut entries, png)?;
    ZipArchiveCodec::pack_compressed(&entries)
}

fn text_of(entries: &[ZipArchiveEntry], path: &str) -> Result<String, EasyOfficeError> {
    let entry = entries
        .iter()
        .find(|entry| entry.path == path)
        .ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(format!("패키지에 {path}가 없습니다."))
        })?;
    String::from_utf8(entry.data.clone())
        .map_err(|_| EasyOfficeError::InvalidUtf8(path.to_string()))
}

fn set_text(entries: &mut [ZipArchiveEntry], path: &str, text: String) {
    if let Some(entry) = entries.iter_mut().find(|entry| entry.path == path) {
        *entry = ZipArchiveEntry::new(path, text.into_bytes());
    }
}

/// Value of the attribute `name` in one start tag (double or single quotes).
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let needle = format!(" {name}={quote}");
        if let Some(at) = tag.find(&needle) {
            let rest = &tag[at + needle.len()..];
            return rest.find(quote).map(|end| &rest[..end]);
        }
    }
    None
}

/// The `Relationship` start tags of a `.rels` part.
fn relationship_tags(rels: &str) -> impl Iterator<Item = &str> {
    rels.split("<Relationship")
        .skip(1)
        .filter(|rest| rest.starts_with([' ', '\t', '\r', '\n']))
        .filter_map(|rest| rest.find('>').map(|end| &rest[..end]))
}

fn free_relationship_id(rels: &str) -> String {
    let taken: Vec<&str> = relationship_tags(rels)
        .filter_map(|tag| attribute(tag, "Id"))
        .collect();
    let mut id = OOXML_THUMBNAIL_REL_ID.to_string();
    let mut number = 1_u32;
    while taken.contains(&id.as_str()) {
        number += 1;
        id = format!("{OOXML_THUMBNAIL_REL_ID}{number}");
    }
    id
}

/// Puts `png` into an OOXML package as `docProps/thumbnail.png`: the part, the `png` default
/// content type when the package has none, and the relationship from the package root. A package
/// that already points at a PNG thumbnail gets that part replaced; one whose thumbnail is another
/// kind of picture is refused rather than left with two.
fn add_ooxml_thumbnail(
    entries: &mut Vec<ZipArchiveEntry>,
    png: &[u8],
) -> Result<(), EasyOfficeError> {
    let rels = text_of(entries, OOXML_ROOT_RELS)?;
    let content_types = text_of(entries, OOXML_CONTENT_TYPES)?;
    let existing = relationship_tags(&rels)
        .find(|tag| attribute(tag, "Type") == Some(OOXML_THUMBNAIL_REL_TYPE))
        .map(|tag| attribute(tag, "Target").unwrap_or_default().to_string());
    let (part, rels) = match existing {
        Some(target) => {
            let part = target.trim_start_matches('/').to_string();
            if !part.to_ascii_lowercase().ends_with(".png") {
                return Err(invalid("패키지의 미리보기 그림이 PNG가 아닙니다."));
            }
            (part, rels)
        }
        None => {
            let id = free_relationship_id(&rels);
            let relationship = format!(
                "<Relationship Id=\"{id}\" Type=\"{OOXML_THUMBNAIL_REL_TYPE}\" Target=\"{OOXML_THUMBNAIL_ENTRY}\"/>"
            );
            let at = rels
                .rfind("</Relationships>")
                .ok_or_else(|| EasyOfficeError::CorruptedContainer("관계 목록 형식".to_string()))?;
            let mut patched = rels.clone();
            patched.insert_str(at, &relationship);
            (OOXML_THUMBNAIL_ENTRY.to_string(), patched)
        }
    };
    let content_types = ensure_png_default(content_types)?;
    for xml in [&rels, &content_types] {
        crate::XmlSanitizer::validate_safe_xml(xml)?;
    }
    set_text(entries, OOXML_ROOT_RELS, rels);
    set_text(entries, OOXML_CONTENT_TYPES, content_types);
    upsert(
        entries,
        &part,
        ZipArchiveEntry::new(part.clone(), png.to_vec()),
        |entry| &entry.path,
    );
    Ok(())
}

fn ensure_png_default(content_types: String) -> Result<String, EasyOfficeError> {
    let has_png = content_types
        .split("<Default")
        .skip(1)
        .filter_map(|rest| rest.find('>').map(|end| &rest[..end]))
        .any(|tag| attribute(tag, "Extension").is_some_and(|ext| ext.eq_ignore_ascii_case("png")));
    if has_png {
        return Ok(content_types);
    }
    let at = content_types
        .rfind("</Types>")
        .ok_or_else(|| EasyOfficeError::CorruptedContainer("콘텐츠 형식 목록 형식".to_string()))?;
    let mut patched = content_types;
    patched.insert_str(at, PNG_CONTENT_TYPE_DEFAULT);
    Ok(patched)
}
