//! DOCX picture import: resolves `w:drawing` -> `a:blip r:embed` through the part's
//! relationships to a `word/media` entry and builds a validated [`RichImage`]. A picture that is
//! not PNG/JPEG, is damaged or exceeds a size limit is skipped with a warning; the whole-document
//! count and byte limits are enforced by the caller.

use std::collections::HashMap;

use crate::rich_xml::{attr, tokenize, XmlEvent};
use crate::{EasyOfficeError, RichImage, XmlSanitizer, ZipArchiveEntry};

const DOCUMENT_RELS_PATH: &str = "word/_rels/document.xml.rels";
const WORD_DIR: &str = "word/";
const IMAGE_REL_SUFFIX: &str = "/image";

/// Result of reading one `w:drawing`.
pub(crate) enum DrawingOutcome {
    Picture(RichImage),
    /// Not an embedded picture, or a picture that was rejected; the text is the warning.
    Skipped(String),
}

/// Relationship id -> media entry of `word/document.xml`.
pub(crate) struct DocxMedia<'a> {
    by_rel: HashMap<String, &'a ZipArchiveEntry>,
}

/// Part path a relationship `target` refers to, or `None` when it could leave the package.
fn resolve_target(target: &str) -> Option<String> {
    let path = match target.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => format!("{WORD_DIR}{target}"),
    };
    let escapes = path.split('/').any(|seg| seg == ".." || seg == ".");
    (!escapes && !path.contains('\\')).then_some(path)
}

impl<'a> DocxMedia<'a> {
    pub(crate) fn load(entries: &'a [ZipArchiveEntry]) -> Result<Self, EasyOfficeError> {
        let mut by_rel = HashMap::new();
        let entry_by_path: HashMap<&str, &'a ZipArchiveEntry> =
            entries.iter().map(|e| (e.path.as_str(), e)).collect();
        let Some(&rels) = entry_by_path.get(DOCUMENT_RELS_PATH) else {
            return Ok(Self { by_rel });
        };
        let xml = std::str::from_utf8(&rels.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8(format!("{DOCUMENT_RELS_PATH} UTF-8 오류"))
        })?;
        XmlSanitizer::validate_safe_xml(xml)?;
        for event in tokenize(xml)? {
            let XmlEvent::Start { name, attrs } = event else {
                continue;
            };
            let is_image = attr(&attrs, "Type").is_some_and(|t| t.ends_with(IMAGE_REL_SUFFIX));
            let external = attr(&attrs, "TargetMode") == Some("External");
            if name != "Relationship" || !is_image || external {
                continue;
            };
            let (Some(id), Some(target)) = (attr(&attrs, "Id"), attr(&attrs, "Target")) else {
                continue;
            };
            let entry =
                resolve_target(target).and_then(|path| entry_by_path.get(path.as_str()).copied());
            if let Some(entry) = entry {
                by_rel.insert(id.to_string(), entry);
            }
        }
        Ok(Self { by_rel })
    }
}

/// Fields collected from the events of one drawing.
#[derive(Default)]
struct DrawingFields<'e> {
    embed: Option<&'e str>,
    alt: Option<&'e str>,
    extent: Option<(u64, u64)>,
}

fn scan_drawing(events: &[XmlEvent]) -> DrawingFields<'_> {
    let mut fields = DrawingFields::default();
    for event in events {
        let XmlEvent::Start { name, attrs } = event else {
            continue;
        };
        match name.as_str() {
            "a:blip" if fields.embed.is_none() => fields.embed = attr(attrs, "r:embed"),
            "wp:docPr" if fields.alt.is_none() => fields.alt = attr(attrs, "descr"),
            "wp:extent" if fields.extent.is_none() => {
                let side = |key| attr(attrs, key).and_then(|v| v.trim().parse::<u64>().ok());
                fields.extent = side("cx").zip(side("cy"));
            }
            _ => {}
        }
    }
    fields
}

/// Reads the picture of one `w:drawing` subtree (`events` spans its start to its end).
pub(crate) fn drawing_picture(events: &[XmlEvent], media: &DocxMedia<'_>) -> DrawingOutcome {
    let fields = scan_drawing(events);
    let Some(entry) = fields.embed.and_then(|id| media.by_rel.get(id)) else {
        return DrawingOutcome::Skipped(
            "포함된 그림이 아닌 도형·차트 등 그리기 개체는 가져오지 않았습니다.".to_string(),
        );
    };
    let built =
        RichImage::new(entry.data.clone(), fields.alt.unwrap_or_default()).map(|mut img| {
            if let Some((cx, cy)) = fields.extent {
                // An out-of-range extent keeps the default size derived from the pixels.
                let _ = img.set_display_emu(cx, cy);
            }
            img
        });
    match built {
        Ok(img) => DrawingOutcome::Picture(img),
        Err(err) => DrawingOutcome::Skipped(format!(
            "그림 '{}'을 가져오지 않았습니다: {err}",
            entry.path
        )),
    }
}
