//! A rich document as an HWP 5.0 file written by rhwp (PD-172).
//!
//! The engine writes the document as HWPX; rhwp parses it and serializes the HWP records
//! (DocInfo mappings, shapes, 22-byte paragraph headers) the way the editor saves a document
//! opened from HWPX. The bytes are read back with rhwp before they are handed out, so a document
//! the next open would refuse is an error here and never a file.

use crate::doc_pdf::MAX_HWPX_BYTES;
use rhwp::DocumentCore;
use std::thread;

/// Parsing recurses with the document's nesting (same size as the PDF layout thread).
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;

/// The HWP 5.0 bytes of `hwpx`, a document the engine wrote.
pub fn hwpx_to_hwp(hwpx: &[u8]) -> Result<Vec<u8>, String> {
    if hwpx.len() > MAX_HWPX_BYTES {
        return Err("문서가 HWP로 바꿀 수 있는 크기를 넘습니다.".to_string());
    }
    // A panic in the parser or the serializer is an error for this request, not a crash.
    thread::scope(|scope| {
        thread::Builder::new()
            .name("doc-hwp-write".into())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, || write_and_check(hwpx))
            .map_err(|err| format!("HWP 작업을 시작하지 못했습니다: {err}"))?
            .join()
            .map_err(|_| "문서를 HWP로 쓰는 중 오류가 났습니다.".to_string())?
    })
}

fn write_and_check(hwpx: &[u8]) -> Result<Vec<u8>, String> {
    let hwp = DocumentCore::from_bytes(hwpx)
        .map_err(|err| format!("문서를 읽지 못했습니다: {err}"))?
        .export_hwp_with_adapter_snapshot()
        .map_err(|err| format!("HWP를 쓰지 못했습니다: {err}"))?;
    // The next open must accept what is written (the save check of the reference app).
    let reread = DocumentCore::from_bytes(&hwp)
        .map_err(|err| format!("쓴 HWP를 다시 열지 못했습니다: {err}"))?;
    if reread.page_count() == 0 {
        return Err("쓴 HWP에 쪽이 없습니다.".to_string());
    }
    Ok(hwp)
}

#[cfg(test)]
#[path = "doc_hwp_tests.rs"]
mod tests;
