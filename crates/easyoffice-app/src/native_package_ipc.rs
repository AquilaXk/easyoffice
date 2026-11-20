//! IPC side of the BASE/BOARD edit packages (PD-265): `convert_document_format` opens a `.eobase`
//! or `.eoboard` file (`action: "decode"`, the model and the pictures come back as data) and builds
//! one (`action: "encode_package"`). The container rules live in `easyoffice_formats::native_package`;
//! the page checks the model itself (`hydrateBaseState`, `replaceState`) before it takes it over.

use easyoffice_formats::native_package::{
    read_package, write_package, NativePackage, PackageKind, PackageMedia,
};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};

use crate::EasyOfficeIpcRouter;

type JsonEntries = [(String, StrictJsonValue)];

fn invalid(detail: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(detail.to_string())
}

fn push_str_field(out: &mut String, key: &str, value: &str) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    EasyOfficeIpcRouter::escape_json_into(out, value);
    out.push('"');
}

impl EasyOfficeIpcRouter {
    /// The reply to opening a package file: the model as JSON text and the pictures as Base64.
    pub(crate) fn decode_native_package(
        kind: PackageKind,
        bytes: &[u8],
    ) -> Result<String, EasyOfficeError> {
        let package = read_package(kind, bytes)?;
        let module = match kind {
            PackageKind::Base => "BASE",
            PackageKind::Board => "BOARD",
        };
        let mut out = String::with_capacity(package.model.len() * 11 / 10 + 256);
        out.push_str("{\"status\":\"OK\",");
        push_str_field(&mut out, "module", module);
        out.push(',');
        push_str_field(&mut out, "format", kind.extension());
        out.push(',');
        push_str_field(&mut out, "kind", kind.id());
        out.push_str(",\"model\":\"");
        Self::escape_json_into(&mut out, &package.model);
        out.push_str("\",\"media\":[");
        for (idx, item) in package.media.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            out.push('{');
            push_str_field(&mut out, "path", &item.path);
            out.push(',');
            push_str_field(&mut out, "mediaType", item.media_type());
            out.push(',');
            push_str_field(&mut out, "base64", &Self::encode_base64(&item.bytes));
            out.push('}');
        }
        out.push_str("],\"warnings\":[]}");
        Ok(out)
    }

    /// Builds a package from `{kind, model, media: [{path, base64}]}`. The bytes are returned only
    /// when the same reader that opens them accepts them.
    pub(crate) fn encode_native_package(entries: &JsonEntries) -> Result<String, EasyOfficeError> {
        let kind = Self::optional_json_str(entries, "kind")?
            .and_then(PackageKind::from_id)
            .ok_or_else(|| invalid("kind는 base 또는 board여야 합니다."))?;
        let model = Self::optional_json_str(entries, "model")?
            .ok_or_else(|| invalid("model이 필요합니다."))?
            .to_string();
        let mut media = Vec::new();
        if let Some(listed) = StrictJsonValue::get_field(entries, "media") {
            for item in listed
                .as_array()
                .ok_or_else(|| invalid("media는 배열이어야 합니다."))?
            {
                let item = item
                    .as_object()
                    .ok_or_else(|| invalid("media 항목은 객체여야 합니다."))?;
                let path = StrictJsonValue::get_str(item, "path").unwrap_or("");
                let base64 = StrictJsonValue::get_str(item, "base64").unwrap_or("");
                let bytes = Self::decode_base64(base64)
                    .ok_or_else(|| invalid("그림이 유효한 Base64가 아닙니다."))?;
                media.push(PackageMedia {
                    path: path.to_string(),
                    bytes,
                });
            }
        }
        let bytes = write_package(&NativePackage { kind, model, media })?;
        Ok(format!(
            "{{\"status\":\"OK\",\"format\":\"{}\",\"outputBytes\":{},\"convertedBase64\":\"{}\",\"warnings\":[]}}",
            kind.extension(),
            bytes.len(),
            Self::encode_base64(&bytes)
        ))
    }
}
