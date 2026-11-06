//! Real source files for `convert_document_format` (PD-075).
//!
//! `{"sourceBase64": ..., "fileName": ...}` carries the bytes of a user's file:
//! - `"action":"decode"` returns the structured editor model (`blocks` / `sheets` / `slides`, the
//!   same shape structured saves send) plus the detected format, target workspace module, and
//!   what the model could not carry;
//! - otherwise the bytes are decoded and converted to `targetFormat` (the editor text is not used).
//!
//! Decoding runs through `UniversalFormatConverter::decode_rich`, so ZIP-bomb ratio/size
//! ceilings, path-traversal checks, and XXE/DTD rejection apply to every path.

use easyoffice_formats::{
    DecodedContent, DecodedDocument, DocumentFormat, EasyOfficeError, OdsUnpacked, RichAlignment,
    RichBlock, RichListKind, RichParagraph, StrictJsonValue, UniversalFormatConverter,
};
use easyoffice_sheet::PackedCellCoordinate;

use crate::convert_structured::{encode_structured, StructuredContent};
use crate::EasyOfficeIpcRouter;

/// Upper bound on a decoded source file (matches the WebUI limit before base64 inflation).
const MAX_SOURCE_BYTES: usize = 12 * 1_048_576;

type JsonEntries = [(String, StrictJsonValue)];

fn limit(entries: &JsonEntries, key: &str) -> Result<Option<u32>, EasyOfficeError> {
    match StrictJsonValue::get_field(entries, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| *n >= 1 && *n <= u64::from(u32::MAX))
            .map(|n| Some(n as u32))
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(format!("'{key}'는 1 이상의 정수여야 합니다."))
            }),
    }
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", EasyOfficeIpcRouter::escape_json_str(s))
}

fn paragraph_json(p: &RichParagraph) -> String {
    let runs: Vec<String> = p
        .runs
        .iter()
        .map(|r| {
            let mut out = format!("{{\"text\":{}", json_str(&r.text));
            for (on, key) in [
                (r.bold, "bold"),
                (r.italic, "italic"),
                (r.underline, "underline"),
                (r.strike, "strike"),
            ] {
                if on {
                    out.push_str(&format!(",\"{key}\":true"));
                }
            }
            if let Some(size) = r.size_half_pt {
                out.push_str(&format!(",\"sizeHalfPt\":{size}"));
            }
            for (value, key) in [
                (&r.font, "font"),
                (&r.color, "color"),
                (&r.highlight, "highlight"),
            ] {
                if let Some(v) = value {
                    out.push_str(&format!(",\"{key}\":{}", json_str(v)));
                }
            }
            out.push('}');
            out
        })
        .collect();
    let mut out = format!("{{\"type\":\"paragraph\",\"runs\":[{}]", runs.join(","));
    if let Some(a) = p.alignment {
        let token = match a {
            RichAlignment::Left => "left",
            RichAlignment::Center => "center",
            RichAlignment::Right => "right",
            RichAlignment::Justify => "justify",
        };
        out.push_str(&format!(",\"align\":\"{token}\""));
    }
    if p.heading_level > 0 {
        out.push_str(&format!(",\"heading\":{}", p.heading_level));
    }
    if let Some(l) = p.list {
        let token = match l {
            RichListKind::Bullet => "bullet",
            RichListKind::Ordered => "ordered",
        };
        out.push_str(&format!(",\"list\":\"{token}\""));
    }
    out.push('}');
    out
}

fn block_json(b: &RichBlock) -> String {
    match b {
        RichBlock::Paragraph(p) => paragraph_json(p),
        RichBlock::Table(rows) => {
            let rows: Vec<String> = rows
                .iter()
                .map(|row| {
                    let cells: Vec<String> = row
                        .iter()
                        .map(|c| format!("{{\"text\":{}}}", json_str(c)))
                        .collect();
                    format!("[{}]", cells.join(","))
                })
                .collect();
            format!("{{\"type\":\"table\",\"rows\":[{}]}}", rows.join(","))
        }
    }
}

/// Keeps cells inside `max_rows` x `max_cols`; returns the number of dropped cells.
fn clip_sheets(sheets: &mut [OdsUnpacked], max_rows: Option<u32>, max_cols: Option<u32>) -> usize {
    let mut dropped = 0usize;
    for (_, cells) in sheets {
        let before = cells.len();
        cells
            .retain(|(r, c, _)| max_rows.is_none_or(|m| *r < m) && max_cols.is_none_or(|m| *c < m));
        dropped += before - cells.len();
    }
    dropped
}

fn decode_response(
    decoded: &DecodedDocument,
    file_name: &str,
    source_bytes: usize,
    max_rows: Option<u32>,
    max_cols: Option<u32>,
) -> Result<String, EasyOfficeError> {
    let mut warnings = decoded.warnings.clone();
    let (kind, payload, count) = match &decoded.content {
        DecodedContent::Document(doc) => {
            let blocks: Vec<String> = doc.blocks.iter().map(block_json).collect();
            (
                "document",
                format!("\"blocks\":[{}]", blocks.join(",")),
                doc.blocks.len(),
            )
        }
        DecodedContent::Workbook(sheets) => {
            let mut sheets = sheets.clone();
            let dropped = clip_sheets(&mut sheets, max_rows, max_cols);
            if dropped > 0 {
                warnings.push(format!(
                    "편집기 격자(최대 {}행 × {}열) 밖의 셀 {dropped}개는 가져오지 않았습니다.",
                    max_rows.map_or("무제한".to_string(), |m| m.to_string()),
                    max_cols.map_or("무제한".to_string(), |m| m.to_string()),
                ));
            }
            let count = sheets.len();
            let items: Vec<String> = sheets
                .iter()
                .map(|(name, cells)| {
                    let cells: Vec<String> = cells
                        .iter()
                        .filter_map(|(r, c, raw)| {
                            let a1 = PackedCellCoordinate::new(*r, *c).ok()?.to_a1();
                            Some(format!("{{\"ref\":\"{a1}\",\"raw\":{}}}", json_str(raw)))
                        })
                        .collect();
                    format!(
                        "{{\"name\":{},\"cells\":[{}]}}",
                        json_str(name),
                        cells.join(",")
                    )
                })
                .collect();
            (
                "workbook",
                format!("\"sheets\":[{}]", items.join(",")),
                count,
            )
        }
        DecodedContent::Slides(slides) => {
            let items: Vec<String> = slides
                .iter()
                .map(|(title, bullets, notes)| {
                    let bullets: Vec<String> = bullets.iter().map(|b| json_str(b)).collect();
                    format!(
                        "{{\"title\":{},\"bullets\":[{}],\"notes\":{}}}",
                        json_str(title),
                        bullets.join(","),
                        json_str(notes)
                    )
                })
                .collect();
            (
                "slides",
                format!("\"slides\":[{}]", items.join(",")),
                slides.len(),
            )
        }
    };
    let warnings_json: Vec<String> = warnings.iter().map(|w| json_str(w)).collect();
    Ok(format!(
        concat!(
            "{{\"status\":\"OK\",\"action\":\"decode\",\"fileName\":{},\"format\":\"{}\",",
            "\"module\":\"{}\",\"kind\":\"{}\",\"title\":{},\"sourceBytes\":{},",
            "\"specification\":{},\"itemCount\":{},\"warnings\":[{}],{}}}"
        ),
        json_str(file_name),
        decoded.format.extension(),
        decoded.format.editor_module(),
        kind,
        json_str(&decoded.title),
        source_bytes,
        json_str(decoded.format.standard_specification()),
        count,
        warnings_json.join(","),
        payload
    ))
}

impl From<DecodedContent> for StructuredContent {
    fn from(content: DecodedContent) -> Self {
        match content {
            DecodedContent::Document(doc) => Self::Document(doc),
            DecodedContent::Workbook(sheets) => Self::Workbook(sheets),
            DecodedContent::Slides(slides) => Self::Slides(slides),
        }
    }
}

impl EasyOfficeIpcRouter {
    /// Handles a `convert_document_format` request that carries `sourceBase64`.
    pub(crate) fn handle_source_file_request(
        entries: &JsonEntries,
        source_base64: &str,
    ) -> Result<String, EasyOfficeError> {
        let b64 = source_base64.trim();
        if b64.len() > MAX_SOURCE_BYTES / 3 * 4 + 8 {
            return Err(EasyOfficeError::PayloadTooLarge {
                actual: b64.len() / 4 * 3,
                max: MAX_SOURCE_BYTES,
            });
        }
        let bytes = Self::decode_base64(b64).ok_or_else(|| {
            EasyOfficeError::InvalidArgument("유효하지 않은 Base64 원본 파일입니다.".to_string())
        })?;
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(EasyOfficeError::PayloadTooLarge {
                actual: bytes.len(),
                max: MAX_SOURCE_BYTES,
            });
        }
        let file_name = Self::optional_json_str(entries, "fileName")?
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let source_hint = match file_name {
            Some(n) => Some(n),
            None => Self::optional_json_str(entries, "sourceFormat")?
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        };
        let decoded = UniversalFormatConverter::decode_rich(&bytes, source_hint)?;
        let action = Self::optional_json_str(entries, "action")?.unwrap_or("convert");
        if action == "decode" {
            let max_rows = limit(entries, "maxRows")?;
            let max_cols = limit(entries, "maxCols")?;
            return decode_response(
                &decoded,
                file_name.unwrap_or(""),
                bytes.len(),
                max_rows,
                max_cols,
            );
        }
        if action != "convert" {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "알 수 없는 action: {action}"
            )));
        }
        let dst_ext = match Self::optional_json_str(entries, "targetFormat")? {
            Some(d) => d,
            None => Self::optional_json_str(entries, "target")?.unwrap_or("hwpx"),
        }
        .trim();
        let dst_fmt = DocumentFormat::from_extension(dst_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 대상 포맷: {dst_ext}"))
        })?;
        let title = Self::optional_json_str(entries, "title")?
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("변환_문서");
        let src_fmt = decoded.format;
        let warnings = decoded.warnings.clone();
        let content = StructuredContent::from(decoded.content);
        let dst_bytes = encode_structured(&content, dst_fmt, title)?;
        Self::conversion_response(src_fmt, dst_fmt, &bytes, &dst_bytes, &warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{
        OoxmlCodec, RichDocument, RichParagraph, RichRun, ZipArchiveCodec, ZipSecurityPolicy,
    };

    fn request(action: Option<&str>, file: &str, bytes: &[u8], extra: &str) -> String {
        let action = action.map_or(String::new(), |a| format!("\"action\":\"{a}\","));
        format!(
            "{{{action}\"fileName\":\"{file}\",\"sourceBase64\":\"{}\"{extra}}}",
            EasyOfficeIpcRouter::encode_base64(bytes)
        )
    }

    fn docx_bytes() -> Vec<u8> {
        let doc = RichDocument {
            title: "t".into(),
            blocks: vec![
                RichBlock::Paragraph(RichParagraph {
                    runs: vec![
                        RichRun::plain("앞 "),
                        RichRun {
                            text: "굵게".into(),
                            bold: true,
                            ..RichRun::default()
                        },
                    ],
                    alignment: Some(RichAlignment::Center),
                    heading_level: 1,
                    list: None,
                }),
                RichBlock::Table(vec![vec!["가".into(), "나".into()]]),
            ],
            official_agency: None,
        };
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap()
    }

    fn json_field<'a>(json: &'a str, key: &str) -> &'a str {
        let start = json.find(&format!("\"{key}\":\"")).unwrap() + key.len() + 4;
        &json[start..start + json[start..].find('"').unwrap()]
    }

    const RUN_PAYLOAD: &str = concat!(
        "{\"targetFormat\":\"docx\",\"title\":\"런\",\"blocks\":[{\"type\":\"paragraph\",\"runs\":[",
        "{\"text\":\"앞 \"},",
        "{\"text\":\"강조\",\"bold\":true,\"sizeHalfPt\":28,\"color\":\"#ff0000\",\"font\":\"함초롬바탕\",\"highlight\":\"FFFF00\"},",
        "{\"text\":\" 뒤\"}]}]}"
    );

    #[test]
    fn structured_runs_with_size_color_font_survive_docx_save_and_reopen() {
        let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", RUN_PAYLOAD).unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
        let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
        let xml = String::from_utf8(
            entries
                .iter()
                .find(|e| e.path == "word/document.xml")
                .unwrap()
                .data
                .clone(),
        )
        .unwrap();
        assert!(
            xml.contains("<w:color w:val=\"FF0000\"/><w:sz w:val=\"28\"/>"),
            "{xml}"
        );
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), "런.docx", &bytes, ""),
        )
        .unwrap();
        assert!(json.contains("{\"text\":\"앞 \"},"), "{json}");
        assert!(
            json.contains("{\"text\":\"강조\",\"bold\":true,\"sizeHalfPt\":28,\"font\":\"함초롬바탕\",\"color\":\"FF0000\",\"highlight\":\"FFFF00\"}"),
            "{json}"
        );
        assert!(json.contains("{\"text\":\" 뒤\"}"), "{json}");
    }

    #[test]
    fn invalid_run_properties_are_rejected_and_old_payloads_still_work() {
        for bad in [
            "\"sizeHalfPt\":1",
            "\"sizeHalfPt\":99999",
            "\"sizeHalfPt\":\"큼\"",
            "\"color\":\"red\"",
            "\"highlight\":\"#12\"",
            "\"font\":\"a\\u0007b\"",
        ] {
            let payload = format!(
                "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"x\",{bad}}}]}}]}}"
            );
            assert!(
                EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err(),
                "{bad}"
            );
        }
        let old = "{\"targetFormat\":\"docx\",\"blocks\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"x\",\"bold\":true}]}]}";
        assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", old).is_ok());
    }

    #[test]
    fn decode_action_returns_structured_blocks_module_and_format() {
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), "보고서.docx", &docx_bytes(), ""),
        )
        .unwrap();
        assert!(json.contains("\"action\":\"decode\""), "{json}");
        assert!(json.contains("\"format\":\"docx\""), "{json}");
        assert!(json.contains("\"module\":\"DOC\""), "{json}");
        assert!(json.contains("\"kind\":\"document\""), "{json}");
        assert!(json.contains("{\"text\":\"굵게\",\"bold\":true}"), "{json}");
        assert!(
            json.contains("\"align\":\"center\",\"heading\":1"),
            "{json}"
        );
        assert!(
            json.contains("{\"type\":\"table\",\"rows\":[[{\"text\":\"가\"},{\"text\":\"나\"}]]}"),
            "{json}"
        );
    }

    #[test]
    fn decode_action_routes_sheets_and_clips_to_the_editor_grid() {
        let sheets = vec![
            (
                "매출".to_string(),
                vec![
                    (0, 0, "1".to_string()),
                    (0, 9, "멀리".to_string()),
                    (5, 1, "=SUM(A1:A1)".to_string()),
                ],
            ),
            ("요약".to_string(), vec![(0, 0, "x".to_string())]),
        ];
        let bytes = OoxmlCodec::pack_xlsx_multi(&sheets).unwrap();
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(
                Some("decode"),
                "a.xlsx",
                &bytes,
                ",\"maxRows\":3,\"maxCols\":8",
            ),
        )
        .unwrap();
        assert!(json.contains("\"module\":\"SHEET\""), "{json}");
        assert!(json.contains("\"kind\":\"workbook\""), "{json}");
        assert!(json.contains("\"name\":\"매출\""), "{json}");
        assert!(json.contains("\"name\":\"요약\""), "{json}");
        assert!(json.contains("{\"ref\":\"A1\",\"raw\":\"1\"}"), "{json}");
        assert!(
            !json.contains("멀리"),
            "column J is outside the grid: {json}"
        );
        assert!(
            !json.contains("SUM(A1:A1)"),
            "row 6 is outside the grid: {json}"
        );
        assert!(json.contains("셀 2개는 가져오지 않았습니다"), "{json}");
    }

    #[test]
    fn convert_uses_the_source_file_bytes_and_ignores_editor_text() {
        let bytes = docx_bytes();
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(
                None,
                "보고서.docx",
                &bytes,
                ",\"sourceFormat\":\"hwpx\",\"targetFormat\":\"md\",\"title\":\"변환\",\"body\":\"에디터 텍스트\"",
            ),
        )
        .unwrap();
        assert!(json.contains("\"sourceFormat\":\"docx\""), "{json}");
        assert!(json.contains("\"targetFormat\":\"md\""), "{json}");
        assert!(
            json.contains(&format!("\"sourceBytes\":{}", bytes.len())),
            "{json}"
        );
        let key = "\"convertedBase64\":\"";
        let start = json.find(key).unwrap() + key.len();
        let end = json[start..].find('"').unwrap() + start;
        let out = EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap();
        let md = String::from_utf8(out).unwrap();
        assert!(md.contains("굵게"), "{md}");
        assert!(md.contains("| 가 | 나 |"), "{md}");
        assert!(!md.contains("에디터 텍스트"), "{md}");
    }

    #[test]
    fn convert_rejects_corrupt_bomb_and_missing_source() {
        let corrupt = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(
                Some("decode"),
                "a.docx",
                b"PK\x03\x04junk-not-a-zip-at-all-0123456789",
                "",
            ),
        );
        assert!(corrupt.is_err());

        let zeros = vec![b' '; 24 * 1024 * 1024];
        let bomb = ZipArchiveCodec::pack_deflate(&[("word/document.xml", zeros)]).unwrap();
        let err = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), "bomb.docx", &bomb, ""),
        )
        .unwrap_err();
        assert!(
            !matches!(err, EasyOfficeError::InvalidArgument(_)),
            "{err:?}"
        );

        assert!(EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            "{\"action\":\"decode\",\"fileName\":\"a.docx\"}",
        )
        .is_err());
        assert!(EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            "{\"sourceBase64\":\"!!!\",\"fileName\":\"a.docx\"}",
        )
        .is_err());
    }
}
