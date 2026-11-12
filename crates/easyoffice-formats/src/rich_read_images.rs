//! DOCX picture import: resolves `w:drawing` -> `a:blip r:embed` through the part's
//! relationships to a `word/media` entry and builds a validated [`RichImage`]. A picture that is
//! not PNG/JPEG, is damaged or exceeds a size limit is skipped with a warning; the whole-document
//! count and byte limits are enforced by the caller.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::rich_document::safe_link_target;
use crate::xml_stream::{attr, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichImage, XmlSanitizer, ZipArchiveEntry};

const DOCUMENT_RELS_PATH: &str = "word/_rels/document.xml.rels";
const WORD_DIR: &str = "word/";
const IMAGE_REL_SUFFIX: &str = "/image";
const HYPERLINK_REL_SUFFIX: &str = "/hyperlink";

/// Relationship id -> address of the external hyperlinks of `word/document.xml`. An address that
/// is not a [`safe_link_target`] is left out, so its text reads as plain text.
pub(crate) fn load_docx_hyperlinks(
    entries: &[ZipArchiveEntry],
) -> Result<HashMap<String, String>, EasyOfficeError> {
    let mut links = HashMap::new();
    let Some(rels) = entries.iter().find(|e| e.path == DOCUMENT_RELS_PATH) else {
        return Ok(links);
    };
    let xml = std::str::from_utf8(&rels.data)
        .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{DOCUMENT_RELS_PATH} UTF-8 오류")))?;
    XmlSanitizer::validate_safe_xml(xml)?;
    for event in XmlReader::new(xml) {
        let XmlEvent::Start { name, attrs } = event? else {
            continue;
        };
        let is_link = attr(&attrs, "Type").is_some_and(|t| t.ends_with(HYPERLINK_REL_SUFFIX));
        if name != "Relationship" || !is_link {
            continue;
        }
        let (Some(id), Some(target)) = (attr(&attrs, "Id"), attr(&attrs, "Target")) else {
            continue;
        };
        if let Some(url) = safe_link_target(&target) {
            links.insert(id.to_string(), url.to_string());
        }
    }
    Ok(links)
}

/// Result of reading one `w:drawing`.
pub(crate) enum DrawingOutcome {
    Picture(RichImage),
    /// Not an embedded picture, or a picture that was rejected; the text is the warning.
    Skipped(String),
}

/// Relationship id -> media entry of `word/document.xml`.
pub(crate) struct DocxMedia<'a> {
    by_rel: HashMap<String, &'a ZipArchiveEntry>,
    /// The picture of each media entry read so far, so one referenced again is neither checked
    /// (PNG CRC) nor copied again. Its alternative text is set per reference.
    pictures: RefCell<HashMap<&'a str, Result<RichImage, EasyOfficeError>>>,
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
            return Ok(Self::new(by_rel));
        };
        let xml = std::str::from_utf8(&rels.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8(format!("{DOCUMENT_RELS_PATH} UTF-8 오류"))
        })?;
        XmlSanitizer::validate_safe_xml(xml)?;
        for event in XmlReader::new(xml) {
            let XmlEvent::Start { name, attrs } = event? else {
                continue;
            };
            let is_image = attr(&attrs, "Type").is_some_and(|t| t.ends_with(IMAGE_REL_SUFFIX));
            let external = attr(&attrs, "TargetMode").as_deref() == Some("External");
            if name != "Relationship" || !is_image || external {
                continue;
            };
            let (Some(id), Some(target)) = (attr(&attrs, "Id"), attr(&attrs, "Target")) else {
                continue;
            };
            let entry =
                resolve_target(&target).and_then(|path| entry_by_path.get(path.as_str()).copied());
            if let Some(entry) = entry {
                by_rel.insert(id.to_string(), entry);
            }
        }
        Ok(Self::new(by_rel))
    }

    fn new(by_rel: HashMap<String, &'a ZipArchiveEntry>) -> Self {
        Self {
            by_rel,
            pictures: RefCell::default(),
        }
    }

    /// The picture stored in `entry`, validated on its first use only.
    fn picture(&self, entry: &'a ZipArchiveEntry, alt: &str) -> Result<RichImage, EasyOfficeError> {
        self.pictures
            .borrow_mut()
            .entry(entry.path.as_str())
            .or_insert_with(|| RichImage::new(entry.data.clone(), ""))
            .clone()
            .map(|image| image.with_alt(alt))
    }
}

/// Fields collected from the events of one drawing.
#[derive(Default)]
struct DrawingFields<'e> {
    embed: Option<Cow<'e, str>>,
    alt: Option<Cow<'e, str>>,
    extent: Option<(u64, u64)>,
}

fn scan_drawing<'e>(events: &[XmlEvent<'e>]) -> DrawingFields<'e> {
    let mut fields = DrawingFields::default();
    for event in events {
        let XmlEvent::Start { name, attrs } = event else {
            continue;
        };
        match *name {
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
pub(crate) fn drawing_picture(events: &[XmlEvent<'_>], media: &DocxMedia<'_>) -> DrawingOutcome {
    let fields = scan_drawing(events);
    let Some(entry) = fields.embed.as_deref().and_then(|id| media.by_rel.get(id)) else {
        return DrawingOutcome::Skipped(
            "포함된 그림이 아닌 도형·차트 등 그리기 개체는 가져오지 않았습니다.".to_string(),
        );
    };
    let built = media
        .picture(entry, fields.alt.as_deref().unwrap_or_default())
        .map(|mut img| {
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
