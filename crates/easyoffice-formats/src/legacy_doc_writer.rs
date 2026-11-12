//! `.doc` (MS-DOC, Word 97-2003) stream writer.
//!
//! The FIB, style sheet, fonts and document properties come from a small blank-document template
//! (see `assets/legacy/` and `THIRD_PARTY_NOTICES.md`). The writer places the text as UTF-16LE at a
//! 512-aligned file offset, builds the piece table (`Clx`), the character/paragraph FKP pages with
//! their `PlcBte` indexes and the section property block, then patches every FIB field that depends
//! on them. An EasyOffice-private payload trailer (title, paragraphs, tables) is appended behind the
//! last real structure so that a save/open round trip stays lossless; no spec structure refers to it.

use crate::ole2_codec::{append_len_prefixed_utf16le, write_u16_le, write_u32_le};
use crate::{EasyOfficeError, LegacyDocPayload, LegacyOle2Codec};

const WORD_HEAD: &[u8] = include_bytes!("../assets/legacy/doc_word_head.bin");
const TABLE_TEMPLATE: &[u8] = include_bytes!("../assets/legacy/doc_table.bin");
const SEPX: &[u8] = include_bytes!("../assets/legacy/doc_sepx.bin");

const PAGE: usize = 512;
/// FibRgFcLcb97 starts here for the template (csw 14, cslw 22).
const FIB_FCLCB_BASE: usize = 154;
const FIB_FC_MIN: usize = 24;
const FIB_FC_MAC: usize = 28;
const FIB_CB_MAC: usize = 64;
const FIB_CCP_TEXT: usize = 76;
const IDX_PLCF_SED: usize = 6;
const IDX_PLCF_BTE_CHPX: usize = 12;
const IDX_PLCF_BTE_PAPX: usize = 13;
const IDX_CLX: usize = 33;
const IDX_PLCF_BTE_LVC: usize = 86;
/// `PlcfSed` in the template table stream: two CPs followed by one `Sed`.
const TABLE_PLCF_SED: usize = 146;
const TABLE_BKD_MOTHER: usize = 180;
/// One FKP page holds at most this many paragraph runs (4+4n+13n+PAPX+1 <= 512).
const PAPX_RUNS_PER_PAGE: usize = 29;
/// `PapxInFkp` for `istd = 0`: cb = 0, cb' = 1, istd. Stored at `bOffset * 2 = 506`.
const PAPX_B_OFFSET: u8 = 253;

fn too_large(what: &str) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(format!("DOC {what}가 형식 상한을 초과했습니다."))
}

fn to_u32(n: usize, what: &str) -> Result<u32, EasyOfficeError> {
    u32::try_from(n).map_err(|_| too_large(what))
}

fn set_fc_lcb(word: &mut [u8], index: usize, fc: usize, lcb: usize) -> Result<(), EasyOfficeError> {
    let at = FIB_FCLCB_BASE + index * 8;
    write_u32_le(word, at, to_u32(fc, "스트림 오프셋")?);
    write_u32_le(word, at + 4, to_u32(lcb, "구조 길이")?);
    Ok(())
}

fn pad_to(buf: &mut Vec<u8>, align: usize) {
    let rem = buf.len() % align;
    if rem != 0 {
        buf.resize(buf.len() + (align - rem), 0);
    }
}

/// Word paragraphs cannot contain CR/LF; soft breaks are 0x0B and other C0 controls are dropped.
fn sanitize(text: &str, tab_ok: bool) -> String {
    text.chars()
        .map(|c| match c {
            '\r' | '\n' | '\u{000B}' => '\u{000B}',
            '\t' if tab_ok => '\t',
            c if (c as u32) < 0x20 || c == '\u{007F}' => ' ',
            c => c,
        })
        .collect()
}

/// Visible paragraphs: body paragraphs, then every table row as one tab-separated paragraph.
fn visible_paragraphs(payload: &LegacyDocPayload) -> Vec<String> {
    let mut out: Vec<String> = payload
        .paragraphs
        .iter()
        .map(|p| sanitize(p, true))
        .collect();
    for table in &payload.tables {
        for row in table {
            let cells: Vec<String> = row.iter().map(|c| sanitize(c, false)).collect();
            out.push(cells.join("\t"));
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// 512-byte CHPX/LVC-style FKP with a single run and no property data.
fn single_run_fkp(fc_first: u32, fc_lim: u32) -> Vec<u8> {
    let mut page = vec![0u8; PAGE];
    write_u32_le(&mut page, 0, fc_first);
    write_u32_le(&mut page, 4, fc_lim);
    page[PAGE - 1] = 1;
    page
}

/// 512-byte PAPX FKP; every run shares one `istd = 0` property block.
fn papx_fkp(bounds: &[u32]) -> Vec<u8> {
    let runs = bounds.len() - 1;
    let mut page = vec![0u8; PAGE];
    for (i, fc) in bounds.iter().enumerate() {
        write_u32_le(&mut page, i * 4, *fc);
    }
    let rgbx = (runs + 1) * 4;
    for i in 0..runs {
        page[rgbx + i * 13] = PAPX_B_OFFSET;
    }
    let papx = usize::from(PAPX_B_OFFSET) * 2;
    page[papx] = 0;
    page[papx + 1] = 1;
    page[PAGE - 1] = runs as u8;
    page
}

fn append_private_trailer(
    word: &mut Vec<u8>,
    payload: &LegacyDocPayload,
) -> Result<(), EasyOfficeError> {
    let start = word.len();
    append_len_prefixed_utf16le(word, &payload.title);
    word.extend_from_slice(&to_u32(payload.paragraphs.len(), "문단 개수")?.to_le_bytes());
    for p in &payload.paragraphs {
        append_len_prefixed_utf16le(word, p);
    }
    word.extend_from_slice(&to_u32(payload.tables.len(), "표 개수")?.to_le_bytes());
    for tbl in &payload.tables {
        let rows = u16::try_from(tbl.len()).map_err(|_| too_large("표 행 개수"))?;
        word.extend_from_slice(&rows.to_le_bytes());
        for row in tbl {
            let cols = u16::try_from(row.len()).map_err(|_| too_large("표 열 개수"))?;
            word.extend_from_slice(&cols.to_le_bytes());
            for cell in row {
                append_len_prefixed_utf16le(word, cell);
            }
        }
    }
    word.extend_from_slice(&to_u32(start, "트레일러 위치")?.to_le_bytes());
    word.extend_from_slice(&LegacyOle2Codec::DOC_TRAILER_MAGIC.to_le_bytes());
    Ok(())
}

/// Builds the `WordDocument` and `1Table` streams of a Word 97-2003 binary document.
pub(crate) fn build_doc_streams(
    payload: &LegacyDocPayload,
) -> Result<(Vec<u8>, Vec<u8>), EasyOfficeError> {
    let paragraphs = visible_paragraphs(payload);
    let mut text16: Vec<u16> = Vec::new();
    // Character offset (CP) where every paragraph (including its terminating CR) ends.
    let mut para_cp_end: Vec<usize> = Vec::with_capacity(paragraphs.len());
    for p in &paragraphs {
        text16.extend(p.encode_utf16());
        text16.push(0x000D);
        para_cp_end.push(text16.len());
    }
    let ccp = text16.len();

    // ---- WordDocument stream
    let mut word = WORD_HEAD.to_vec();
    let text_fc = word.len();
    for unit in &text16 {
        word.extend_from_slice(&unit.to_le_bytes());
    }
    let text_end_fc = word.len();
    pad_to(&mut word, PAGE);

    let fc_of = |cp: usize| -> Result<u32, EasyOfficeError> {
        to_u32(text_fc + cp * 2, "텍스트 오프셋")
    };
    let fc_first = fc_of(0)?;
    let fc_lim = fc_of(ccp)?;

    let chpx_pn = to_u32(word.len() / PAGE, "페이지 번호")?;
    word.extend_from_slice(&single_run_fkp(fc_first, fc_lim));

    let mut papx_bounds: Vec<u32> = Vec::new(); // aFC of PlcBtePapx
    let mut papx_pns: Vec<u32> = Vec::new();
    let mut prev_cp = 0usize;
    for chunk in para_cp_end.chunks(PAPX_RUNS_PER_PAGE) {
        let mut bounds = vec![fc_of(prev_cp)?];
        for end in chunk {
            bounds.push(fc_of(*end)?);
        }
        prev_cp = chunk[chunk.len() - 1];
        papx_pns.push(to_u32(word.len() / PAGE, "페이지 번호")?);
        papx_bounds.push(bounds[0]);
        word.extend_from_slice(&papx_fkp(&bounds));
    }
    papx_bounds.push(fc_lim);

    let lvc_pn = to_u32(word.len() / PAGE, "페이지 번호")?;
    word.extend_from_slice(&single_run_fkp(fc_first, fc_lim));

    let sepx_fc = word.len();
    word.extend_from_slice(SEPX);
    pad_to(&mut word, PAGE);

    append_private_trailer(&mut word, payload)?;

    // ---- 1Table stream
    let mut table = TABLE_TEMPLATE.to_vec();
    pad_to(&mut table, 4);
    write_u32_le(&mut table, TABLE_PLCF_SED + 4, to_u32(ccp, "문자 수")?);
    write_u32_le(
        &mut table,
        TABLE_PLCF_SED + 10,
        to_u32(sepx_fc, "구역 속성 오프셋")?,
    );
    write_u32_le(&mut table, TABLE_BKD_MOTHER + 4, to_u32(ccp, "문자 수")?);

    // Clx: Pcdt with one Pcd that points at the UTF-16LE text (FcCompressed bit 30 clear).
    let fc_clx = table.len();
    table.push(0x02);
    table.extend_from_slice(&16u32.to_le_bytes());
    table.extend_from_slice(&0u32.to_le_bytes());
    table.extend_from_slice(&to_u32(ccp, "문자 수")?.to_le_bytes());
    table.extend_from_slice(&0u16.to_le_bytes());
    table.extend_from_slice(&fc_first.to_le_bytes());
    table.extend_from_slice(&0u16.to_le_bytes());
    let lcb_clx = table.len() - fc_clx;

    let fc_chpx = table.len();
    table.extend_from_slice(&fc_first.to_le_bytes());
    table.extend_from_slice(&fc_lim.to_le_bytes());
    table.extend_from_slice(&chpx_pn.to_le_bytes());
    let lcb_chpx = table.len() - fc_chpx;

    let fc_papx = table.len();
    for fc in &papx_bounds {
        table.extend_from_slice(&fc.to_le_bytes());
    }
    for pn in &papx_pns {
        table.extend_from_slice(&pn.to_le_bytes());
    }
    let lcb_papx = table.len() - fc_papx;

    let fc_lvc = table.len();
    table.extend_from_slice(&fc_first.to_le_bytes());
    table.extend_from_slice(&fc_lim.to_le_bytes());
    table.extend_from_slice(&lvc_pn.to_le_bytes());
    let lcb_lvc = table.len() - fc_lvc;

    // ---- FIB
    write_u32_le(&mut word, FIB_FC_MIN, fc_first);
    write_u32_le(
        &mut word,
        FIB_FC_MAC,
        to_u32(text_end_fc, "텍스트 끝 오프셋")?,
    );
    let cb_mac = to_u32(word.len(), "스트림 크기")?;
    write_u32_le(&mut word, FIB_CB_MAC, cb_mac);
    write_u32_le(&mut word, FIB_CCP_TEXT, to_u32(ccp, "문자 수")?);
    set_fc_lcb(&mut word, IDX_PLCF_SED, TABLE_PLCF_SED, 20)?;
    set_fc_lcb(&mut word, IDX_CLX, fc_clx, lcb_clx)?;
    set_fc_lcb(&mut word, IDX_PLCF_BTE_CHPX, fc_chpx, lcb_chpx)?;
    set_fc_lcb(&mut word, IDX_PLCF_BTE_PAPX, fc_papx, lcb_papx)?;
    set_fc_lcb(&mut word, IDX_PLCF_BTE_LVC, fc_lvc, lcb_lvc)?;
    // Keep the table stream selector and language as shipped by the template.
    let flags = u16::from_le_bytes([word[10], word[11]]);
    write_u16_le(
        &mut word,
        10,
        flags | LegacyOle2Codec::DOC_FLAG_WHICH_TBL_STM,
    );
    Ok((word, table))
}
