use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, UniversalDocumentModel, UniversalFormatConverter,
};
use easyoffice_sheet::PackedCellCoordinate;

use crate::EasyOfficeIpcRouter;

impl EasyOfficeIpcRouter {
    pub(crate) fn handle_convert_document_format(payload: &str) -> Result<String, EasyOfficeError> {
        let (src_ext, dst_ext, title, body) = if let Some((first, rest)) = payload.split_once('|') {
            if let Some((s, d)) = first.split_once("->") {
                let (t, b) = rest.split_once('|').unwrap_or((rest, ""));
                (s.trim(), d.trim(), t.trim(), b.trim())
            } else if let Some((second, rest2)) = rest.split_once('|') {
                let (t, b) = rest2.split_once('|').unwrap_or((rest2, ""));
                (first.trim(), second.trim(), t.trim(), b.trim())
            } else {
                (first.trim(), rest.trim(), "변환_문서", "본문 텍스트")
            }
        } else if let Some((s, d)) = payload.split_once("->") {
            (s.trim(), d.trim(), "변환_문서", "본문 텍스트")
        } else {
            ("docx", "hwpx", "변환_문서", "본문 텍스트")
        };

        let title = if title.is_empty() {
            "변환_문서"
        } else {
            title
        };

        let src_fmt = DocumentFormat::from_extension(src_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 원본 포맷: {src_ext}"))
        })?;
        let dst_fmt = DocumentFormat::from_extension(dst_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 대상 포맷: {dst_ext}"))
        })?;

        let mut paragraphs: Vec<String> = Vec::new();
        let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
        let mut current_table: Vec<Vec<String>> = Vec::new();
        let mut sheet_cells: Vec<(u32, u32, String)> = Vec::new();
        let mut slides: Vec<(String, Vec<String>, String)> = Vec::new();
        let mut official_agency: Option<String> = None;

        for raw_line in body.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(agency) = line.strip_prefix("[[AGENCY]]\t") {
                if !agency.trim().is_empty() {
                    official_agency = Some(agency.trim().to_string());
                }
            } else if line == "[[TABLE_NEW]]" {
                if !current_table.is_empty() {
                    tables.push(std::mem::take(&mut current_table));
                }
            } else if let Some(row_str) = line.strip_prefix("[[TABLE_ROW]]\t") {
                let row: Vec<String> = row_str.split('\t').map(|c| c.trim().to_string()).collect();
                if !row.is_empty() {
                    current_table.push(row);
                }
            } else if let Some(cell_str) = line.strip_prefix("[[SHEET_CELL]]\t") {
                let mut cols = cell_str.splitn(3, '\t');
                if let (Some(r_s), Some(c_s), Some(v_s)) = (cols.next(), cols.next(), cols.next()) {
                    if let (Ok(r), Ok(c)) = (r_s.trim().parse::<u32>(), c_s.trim().parse::<u32>()) {
                        sheet_cells.push((r, c, v_s.to_string()));
                    }
                }
            } else if let Some(a1_str) = line.strip_prefix("[[SHEET_A1]]\t") {
                if let Some((a1, v_s)) = a1_str.split_once('\t') {
                    if let Ok(coord) = PackedCellCoordinate::parse_a1(a1.trim()) {
                        sheet_cells.push((coord.row(), coord.col(), v_s.to_string()));
                    }
                }
            } else if let Some(slide_str) = line.strip_prefix("[[SLIDE]]\t") {
                let mut cols = slide_str.splitn(3, '\t');
                let s_title = cols.next().unwrap_or(title).trim().to_string();
                let bullets: Vec<String> = cols
                    .next()
                    .unwrap_or("")
                    .split(";;")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
                    .collect();
                let notes = cols.next().unwrap_or("").trim().to_string();
                slides.push((s_title, bullets, notes));
            } else {
                paragraphs.push(line.to_string());
            }
        }
        if !current_table.is_empty() {
            tables.push(current_table);
        }
        if paragraphs.is_empty() && !sheet_cells.is_empty() {
            paragraphs = sheet_cells.iter().map(|(_, _, v)| v.clone()).collect();
        }
        if paragraphs.is_empty() && !slides.is_empty() {
            for (s_t, bullets, notes) in &slides {
                if !s_t.is_empty() {
                    paragraphs.push(s_t.clone());
                }
                paragraphs.extend(bullets.iter().cloned());
                if !notes.is_empty() {
                    paragraphs.push(notes.clone());
                }
            }
        }
        if paragraphs.is_empty() {
            paragraphs.push(title.to_string());
        }

        let model = UniversalDocumentModel {
            title: title.to_string(),
            paragraphs,
            tables,
            sheet_cells,
            slides,
            official_agency,
        };

        let src_bytes = UniversalFormatConverter::encode(&model, src_fmt)?;
        let dst_bytes = UniversalFormatConverter::encode(&model, dst_fmt)?;
        let (detected_dst, decoded) =
            UniversalFormatConverter::decode(&dst_bytes, Some(dst_fmt.extension()))?;
        let b64 = Self::encode_base64(&dst_bytes);

        Ok(format!(
            concat!(
                "{{",
                "\"sourceFormat\":\"{}\",",
                "\"targetFormat\":\"{}\",",
                "\"sourceBytes\":{},",
                "\"outputBytes\":{},",
                "\"targetBytes\":{},",
                "\"detectedTarget\":\"{}\",",
                "\"containerFamily\":\"{}\",",
                "\"specification\":\"{}\",",
                "\"paragraphCount\":{},",
                "\"tableCount\":{},",
                "\"title\":\"{}\",",
                "\"converted_base64\":\"{}\",",
                "\"convertedBase64\":\"{}\",",
                "\"status\":\"OK\"",
                "}}"
            ),
            src_fmt.extension(),
            dst_fmt.extension(),
            src_bytes.len(),
            dst_bytes.len(),
            dst_bytes.len(),
            detected_dst.extension(),
            dst_fmt.container_family().as_str(),
            dst_fmt.standard_specification(),
            decoded.paragraphs.len(),
            decoded.tables.len(),
            Self::escape_json_str(&decoded.title),
            b64,
            b64
        ))
    }
}
