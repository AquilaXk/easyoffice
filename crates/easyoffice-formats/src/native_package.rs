//! Edit package of the BASE database (`.eobase`) and the BOARD canvas (`.eoboard`) (PD-265).
//!
//! The package holds the model as the editor keeps it, so a reopened file edits exactly as it was
//! saved. PDF and XLSX are exports, not this format.
//!
//! Container: a ZIP archive (PKWARE APPNOTE, the container of OOXML and ODF) with these parts, no others:
//!
//! * `mimetype`: first part, stored, the media type of the package (ODF convention).
//! * `manifest.json`: `{"format":"easyoffice-base"|"easyoffice-board","formatVersion":1,"media":[...]}`;
//!   each media item is `{"path","mediaType","bytes"}` and must match a `media/` part exactly.
//! * `model.json`: the module model, a JSON object (RFC 8259).
//! * `media/<name>.png|jpg`: pictures of a BOARD, referenced from the model by path.
//!
//! The reader is Fail-Closed: a package outside any limit below, with a part that is not listed, a
//! listed part that is missing, a picture that is not a sound PNG or JPEG, a newer `formatVersion`,
//! or a model that is not strict JSON is refused whole. The writer re-reads what it wrote, so a
//! package that the next open would refuse is never handed out.

use crate::{
    EasyOfficeError, RichImage, RichImageFormat, StrictJsonValue, ZipArchiveCodec, ZipArchiveEntry,
    ZipSecurityPolicy,
};
use std::collections::HashSet;

/// Format version this build writes and the newest it reads.
pub const FORMAT_VERSION: u64 = 1;
/// Largest package file, written or read. Equal to the engine's source-file limit
/// (`MAX_SOURCE_BYTES`), because a package travels through the same IPC body as any opened file.
pub const MAX_PACKAGE_BYTES: usize = 10 * 1_048_576;
/// Largest `model.json` (uncompressed).
pub const MAX_MODEL_BYTES: usize = 12 * 1_048_576;
/// Largest `manifest.json` (uncompressed).
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Most pictures in one package (the BOARD holds at most 512 elements).
pub const MAX_MEDIA_PARTS: usize = 512;
/// Parts of a package: the three fixed ones and the pictures.
const MAX_PARTS: usize = MAX_MEDIA_PARTS + 3;
/// Largest uncompressed total over all parts.
const MAX_TOTAL_BYTES: u64 = 32 * 1_048_576;
const MAX_MEDIA_NAME_CHARS: usize = 40;
const MIMETYPE_PART: &str = "mimetype";
const MANIFEST_PART: &str = "manifest.json";
const MODEL_PART: &str = "model.json";
const MEDIA_DIR: &str = "media/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Base,
    Board,
}

impl PackageKind {
    pub fn from_extension(ext: &str) -> Option<Self> {
        if ext.eq_ignore_ascii_case("eobase") {
            Some(Self::Base)
        } else if ext.eq_ignore_ascii_case("eoboard") {
            Some(Self::Board)
        } else {
            None
        }
    }

    pub fn from_file_name(name: &str) -> Option<Self> {
        Self::from_extension(name.rsplit_once('.')?.1)
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Base => "eobase",
            Self::Board => "eoboard",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Base => "application/vnd.easyoffice.base+zip",
            Self::Board => "application/vnd.easyoffice.board+zip",
        }
    }

    fn format_id(self) -> &'static str {
        match self {
            Self::Base => "easyoffice-base",
            Self::Board => "easyoffice-board",
        }
    }

    fn max_media(self) -> usize {
        match self {
            Self::Base => 0,
            Self::Board => MAX_MEDIA_PARTS,
        }
    }
}

/// A picture part: `path` is `media/<name>.png|jpg`, `bytes` a sound PNG or JPEG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageMedia {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// The content of a package: the model as JSON text and its pictures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePackage {
    pub kind: PackageKind,
    pub model: String,
    pub media: Vec<PackageMedia>,
}

fn refuse(detail: impl Into<String>) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(detail.into())
}

/// The reader's policy: the part count and the sizes of this format. No ratio rule (a model of
/// empty rows deflates far past any ratio); the absolute caps bound what an entry may inflate to.
fn read_policy() -> ZipSecurityPolicy {
    ZipSecurityPolicy {
        max_entries: MAX_PARTS,
        max_single_entry_uncompressed_bytes: MAX_MODEL_BYTES as u64,
        max_total_uncompressed_bytes: MAX_TOTAL_BYTES,
        max_compression_ratio: f64::INFINITY,
        binary_budget: None,
    }
}

fn image_format_of(path: &str) -> Option<RichImageFormat> {
    match path.strip_prefix(MEDIA_DIR)?.rsplit_once('.')?.1 {
        "png" => Some(RichImageFormat::Png),
        "jpg" => Some(RichImageFormat::Jpeg),
        _ => None,
    }
}

fn is_media_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(MEDIA_DIR) else {
        return false;
    };
    let Some((name, _)) = rest.rsplit_once('.') else {
        return false;
    };
    image_format_of(path).is_some()
        && !name.is_empty()
        && name.len() <= MAX_MEDIA_NAME_CHARS
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// A sound PNG or JPEG whose kind is the one the part name says (the engine's picture checks:
/// structure, CRC, and the pixel-count limit).
fn checked_picture(path: &str, bytes: &[u8]) -> Result<RichImageFormat, EasyOfficeError> {
    let image = RichImage::new(bytes.to_vec(), "")?;
    if Some(image.format()) != image_format_of(path) {
        return Err(refuse(format!("그림 형식이 파트 이름과 다릅니다: {path}")));
    }
    Ok(image.format())
}

fn check_media(kind: PackageKind, media: &[PackageMedia]) -> Result<(), EasyOfficeError> {
    if media.len() > kind.max_media() {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "그림 수({})가 한도({})를 넘었습니다.",
            media.len(),
            kind.max_media()
        )));
    }
    let mut seen = HashSet::with_capacity(media.len());
    for item in media {
        if !is_media_path(&item.path) {
            return Err(refuse(format!("허용되지 않는 그림 경로: {}", item.path)));
        }
        if !seen.insert(item.path.as_str()) {
            return Err(refuse(format!("그림 경로가 중복되었습니다: {}", item.path)));
        }
        checked_picture(&item.path, &item.bytes)?;
    }
    Ok(())
}

fn check_model(model: &str) -> Result<(), EasyOfficeError> {
    if model.len() > MAX_MODEL_BYTES {
        return Err(EasyOfficeError::PayloadTooLarge {
            actual: model.len(),
            max: MAX_MODEL_BYTES,
        });
    }
    match StrictJsonValue::parse(model)? {
        StrictJsonValue::Object(_) => Ok(()),
        _ => Err(refuse("모델 루트는 객체여야 합니다.")),
    }
}

fn manifest_json(kind: PackageKind, media: &[PackageMedia]) -> String {
    let mut out = format!(
        "{{\"format\":\"{}\",\"formatVersion\":{FORMAT_VERSION},\"media\":[",
        kind.format_id()
    );
    for (idx, item) in media.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let media_type = image_format_of(&item.path).map_or("", RichImageFormat::mime);
        out.push_str(&format!(
            "{{\"path\":\"{}\",\"mediaType\":\"{media_type}\",\"bytes\":{}}}",
            item.path,
            item.bytes.len()
        ));
    }
    out.push_str("]}");
    out
}

/// Writes `package` and re-reads the result: the bytes are returned only when this reader accepts them.
pub fn write_package(package: &NativePackage) -> Result<Vec<u8>, EasyOfficeError> {
    check_model(&package.model)?;
    check_media(package.kind, &package.media)?;
    let mut parts = vec![
        ZipArchiveEntry::new(MIMETYPE_PART, package.kind.mime().as_bytes().to_vec()),
        ZipArchiveEntry::new(
            MANIFEST_PART,
            manifest_json(package.kind, &package.media).into_bytes(),
        ),
        ZipArchiveEntry::new(MODEL_PART, package.model.clone().into_bytes()),
    ];
    parts.extend(
        package
            .media
            .iter()
            .map(|item| ZipArchiveEntry::new(item.path.clone(), item.bytes.clone())),
    );
    let bytes = ZipArchiveCodec::pack_compressed(&parts)?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(EasyOfficeError::PayloadTooLarge {
            actual: bytes.len(),
            max: MAX_PACKAGE_BYTES,
        });
    }
    read_package(package.kind, &bytes)?;
    Ok(bytes)
}

fn utf8_part<'a>(parts: &'a [ZipArchiveEntry], path: &str) -> Result<&'a str, EasyOfficeError> {
    let part = parts
        .iter()
        .find(|part| part.path == path)
        .ok_or_else(|| refuse(format!("패키지에 {path} 파트가 없습니다.")))?;
    std::str::from_utf8(&part.data).map_err(|_| refuse(format!("{path}가 UTF-8이 아닙니다.")))
}

/// The pictures the manifest lists must be the `media/` parts of the archive, byte for byte.
fn check_manifest(
    kind: PackageKind,
    manifest: &str,
    media: &[PackageMedia],
) -> Result<(), EasyOfficeError> {
    let StrictJsonValue::Object(fields) = StrictJsonValue::parse(manifest)? else {
        return Err(refuse("manifest.json의 루트는 객체여야 합니다."));
    };
    if StrictJsonValue::get_str(&fields, "format") != Some(kind.format_id()) {
        return Err(refuse("이 파일은 다른 종류의 패키지입니다."));
    }
    let version = StrictJsonValue::get_field(&fields, "formatVersion")
        .and_then(StrictJsonValue::as_u64)
        .ok_or_else(|| refuse("formatVersion이 없거나 정수가 아닙니다."))?;
    if version > FORMAT_VERSION {
        return Err(refuse(format!(
            "더 새로운 버전({version})으로 저장된 파일입니다. 이 앱은 {FORMAT_VERSION}까지 읽습니다."
        )));
    }
    if version == 0 {
        return Err(refuse("formatVersion은 1 이상이어야 합니다."));
    }
    let listed = StrictJsonValue::get_field(&fields, "media")
        .and_then(StrictJsonValue::as_array)
        .ok_or_else(|| refuse("media 목록이 없습니다."))?;
    if listed.len() != media.len() {
        return Err(refuse("manifest의 그림 목록이 패키지의 그림과 다릅니다."));
    }
    for item in listed {
        let item = item
            .as_object()
            .ok_or_else(|| refuse("media 항목은 객체여야 합니다."))?;
        let path = StrictJsonValue::get_str(item, "path").unwrap_or("");
        let part = media
            .iter()
            .find(|part| part.path == path)
            .ok_or_else(|| refuse(format!("manifest에만 있는 그림입니다: {path}")))?;
        let expected_type = image_format_of(path).map_or("", RichImageFormat::mime);
        let size = StrictJsonValue::get_field(item, "bytes").and_then(StrictJsonValue::as_u64);
        if StrictJsonValue::get_str(item, "mediaType") != Some(expected_type)
            || size != Some(part.bytes.len() as u64)
        {
            return Err(refuse(format!("manifest와 그림 파트가 다릅니다: {path}")));
        }
    }
    Ok(())
}

/// Reads a package of `kind`, refusing it whole when anything is outside the format.
pub fn read_package(kind: PackageKind, bytes: &[u8]) -> Result<NativePackage, EasyOfficeError> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(EasyOfficeError::PayloadTooLarge {
            actual: bytes.len(),
            max: MAX_PACKAGE_BYTES,
        });
    }
    let parts = ZipArchiveCodec::unpack(bytes, &read_policy())?;
    if parts.first().map(|part| part.path.as_str()) != Some(MIMETYPE_PART) {
        return Err(refuse("첫 파트가 mimetype이 아닙니다."));
    }
    if parts[0].data != kind.mime().as_bytes() {
        return Err(refuse("이 파일은 다른 종류의 패키지입니다."));
    }
    let mut media = Vec::new();
    let (mut manifests, mut models) = (0, 0);
    for part in &parts[1..] {
        match part.path.as_str() {
            MANIFEST_PART => manifests += 1,
            MODEL_PART => models += 1,
            path if is_media_path(path) => media.push(PackageMedia {
                path: part.path.clone(),
                bytes: part.data.clone(),
            }),
            path => return Err(refuse(format!("허용되지 않는 파트입니다: {path}"))),
        }
    }
    if manifests != 1 || models != 1 {
        return Err(refuse("manifest.json과 model.json이 하나씩 있어야 합니다."));
    }
    let manifest = utf8_part(&parts, MANIFEST_PART)?;
    if manifest.len() > MAX_MANIFEST_BYTES {
        return Err(refuse("manifest.json이 너무 큽니다."));
    }
    check_media(kind, &media)?;
    check_manifest(kind, manifest, &media)?;
    let model = utf8_part(&parts, MODEL_PART)?;
    check_model(model)?;
    Ok(NativePackage {
        kind,
        model: model.to_string(),
        media,
    })
}
