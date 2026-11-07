//! Pages picture import. A picture drawable (`TSD.ImageArchive`) references a data identifier;
//! the package metadata maps identifiers to file names and the file lives under `Data/`.
//! Only PNG and JPEG data are imported; anything else is reported as skipped.

use std::collections::HashMap;

use crate::pages_proto::{ArchiveObject, Message, ObjectTable};
use crate::pages_schema::{data_info, drawable, geometry, image, package_metadata, ty};
use crate::{
    EasyOfficeError, RichImage, MAX_IMAGES_PER_DOCUMENT, MAX_IMAGE_BYTES, MAX_TOTAL_IMAGE_BYTES,
};

const DATA_DIR: &str = "Data/";
/// Points to EMU (12,700 per point).
const EMU_PER_POINT: f32 = 12_700.0;

/// Outcome of reading one picture drawable.
pub(crate) enum PageImage {
    Picture(RichImage),
    /// Skipped; the text names the reason for the warning list.
    Skipped(&'static str),
}

/// Resolves picture drawables against the package's `Data/` files and enforces document limits.
pub(crate) struct PageImages<'a> {
    objects: &'a ObjectTable,
    files: &'a HashMap<String, Vec<u8>>,
    names: Option<HashMap<u64, String>>,
    count: usize,
    bytes: usize,
}

impl<'a> PageImages<'a> {
    pub(crate) fn new(objects: &'a ObjectTable, files: &'a HashMap<String, Vec<u8>>) -> Self {
        Self {
            objects,
            files,
            names: None,
            count: 0,
            bytes: 0,
        }
    }

    /// Data identifier -> file name, from every package-metadata object (built on first use).
    fn file_name(&mut self, data_id: u64) -> Option<&str> {
        let objects = self.objects;
        self.names
            .get_or_insert_with(|| collect_file_names(objects))
            .get(&data_id)
            .map(String::as_str)
    }

    pub(crate) fn read(&mut self, obj: &ArchiveObject) -> Result<PageImage, EasyOfficeError> {
        let Ok(msg) = Message::parse(&obj.payload) else {
            return Ok(PageImage::Skipped("읽을 수 없는 그림"));
        };
        let Some(data_id) = msg.sub(image::DATA)?.and_then(|d| d.varint(1)) else {
            return Ok(PageImage::Skipped("그림 데이터 참조가 없는 그림"));
        };
        let Some(name) = self.file_name(data_id).map(str::to_string) else {
            return Ok(PageImage::Skipped("파일 이름을 찾을 수 없는 그림"));
        };
        let Some(bytes) = self.files.get(&format!("{DATA_DIR}{name}")) else {
            return Ok(PageImage::Skipped("파일이 없는 그림"));
        };
        if bytes.len() > MAX_IMAGE_BYTES {
            return Ok(PageImage::Skipped("크기 한도를 넘은 그림"));
        }
        self.count += 1;
        self.bytes = self.bytes.saturating_add(bytes.len());
        if self.count > MAX_IMAGES_PER_DOCUMENT || self.bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 그림이 문서당 한도(개수·합계 크기)를 넘었습니다.".to_string(),
            ));
        }
        let Ok(mut picture) = RichImage::new(bytes.clone(), "") else {
            return Ok(PageImage::Skipped("PNG·JPEG가 아니거나 손상된 그림"));
        };
        if let Some((w, h)) = display_size_emu(&msg) {
            // An out-of-range size keeps the default derived from the pixels.
            let _ = picture.set_display_emu(w, h);
        }
        Ok(PageImage::Picture(picture))
    }
}

fn collect_file_names(objects: &ObjectTable) -> HashMap<u64, String> {
    let mut names = HashMap::new();
    for obj in objects.objects.values() {
        if obj.type_id != ty::PACKAGE_METADATA {
            continue;
        }
        let Ok(meta) = Message::parse(&obj.payload) else {
            continue;
        };
        for info in meta.subs(package_metadata::DATA_INFOS).unwrap_or_default() {
            let name = info
                .string(data_info::FILE_NAME)
                .ok()
                .flatten()
                .filter(|n| !n.is_empty())
                .or_else(|| info.string(data_info::PREFERRED_NAME).ok().flatten());
            if let (Some(id), Some(name)) = (info.varint(data_info::IDENTIFIER), name) {
                names.insert(id, name);
            }
        }
    }
    names
}

/// Display size from the drawable geometry (points), in EMU.
fn display_size_emu(msg: &Message<'_>) -> Option<(u64, u64)> {
    let size = msg
        .sub(image::DRAWABLE)
        .ok()??
        .sub(drawable::GEOMETRY)
        .ok()??
        .sub(geometry::SIZE)
        .ok()??;
    let to_emu = |points: f32| {
        (points.is_finite() && points >= 1.0).then_some((points * EMU_PER_POINT) as u64)
    };
    Some((to_emu(size.float(1)?)?, to_emu(size.float(2)?)?))
}
