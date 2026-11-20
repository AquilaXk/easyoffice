//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
//!
//! 루프백 모델 서버용 HTTP/1.1 전송. 응답 헤더·본문 크기 상한, 리다이렉트 거부, 청크 상한 해석을 한다.
//! 요청이 연 소켓은 `CancelHandle`에 등록해 취소 때 끊는다.

use easyoffice_formats::EasyOfficeError;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use super::cancel::CANCELLED_MESSAGE;
use super::types::{CopilotLlmRequest, HttpMethod, MAX_LLM_RESPONSE_BYTES};

const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const LOCAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_IO_TIMEOUT: Duration = Duration::from_secs(90);

fn format_host_header(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// 루프백 HTTP 요청을 보내고 본문을 돌려준다. 취소되면 소켓이 닫히고 `CANCELLED_MESSAGE`로 끝난다.
pub fn send_local_http(req: &CopilotLlmRequest) -> Result<String, EasyOfficeError> {
    req.cancel
        .check()
        .map_err(EasyOfficeError::InvalidArgument)?;
    let addr: SocketAddr = *req.resolved_addrs.first().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("로컬 엔드포인트 주소를 확인할 수 없습니다.".to_string())
    })?;
    let mut stream = TcpStream::connect_timeout(&addr, LOCAL_CONNECT_TIMEOUT)
        .map_err(|err| io_failure(req, format!("로컬 모델 서버에 연결할 수 없습니다: {err}")))?;
    req.cancel.attach(&stream);
    let _ = stream.set_read_timeout(Some(LOCAL_IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));

    let host_hdr = format_host_header(&req.host, req.port);
    let mut wire = String::with_capacity(256 + req.body_json.len());
    let _ = write!(
        wire,
        "{} {} HTTP/1.1\r\nHost: {host_hdr}\r\nAccept: application/json\r\nAccept-Encoding: identity\r\n",
        req.method.as_str(),
        req.path
    );
    if req.method == HttpMethod::Post {
        let _ = write!(
            wire,
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            req.body_json.len()
        );
    }
    for (k, v) in &req.headers {
        let _ = write!(wire, "{k}: {v}\r\n");
    }
    wire.push_str("Connection: close\r\n\r\n");
    if req.method == HttpMethod::Post {
        wire.push_str(&req.body_json);
    }

    stream
        .write_all(wire.as_bytes())
        .map_err(|err| io_failure(req, format!("로컬 모델 요청 전송에 실패했습니다: {err}")))?;
    let _ = stream.flush();

    let max_wire = MAX_HTTP_HEADER_BYTES + MAX_LLM_RESPONSE_BYTES + 4096;
    let mut raw = Vec::with_capacity(8192);
    let mut buf = [0u8; 8192];
    let mut hdr_end_cache: Option<usize> = None;
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if raw.len().saturating_add(n) > max_wire {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "로컬 모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string(),
                    ));
                }
                raw.extend_from_slice(&buf[..n]);
                if hdr_end_cache.is_none() {
                    hdr_end_cache = raw.windows(4).position(|w| w == b"\r\n\r\n");
                    if hdr_end_cache.is_none() && raw.len() > MAX_HTTP_HEADER_BYTES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "모델 서버 HTTP 응답 헤더가 너무 큽니다.".to_string(),
                        ));
                    }
                }
                if let Some(hdr_end) = hdr_end_cache {
                    if http_frame_complete(&raw, hdr_end)? {
                        break;
                    }
                }
            }
            Err(err) => {
                return Err(io_failure(
                    req,
                    format!("로컬 모델 응답을 읽지 못했습니다: {err}"),
                ));
            }
        }
    }

    // 소켓을 닫으면 읽기가 오류 없이 끝으로 돌아올 수 있다. 취소를 불완전한 응답보다 먼저 판정한다.
    req.cancel
        .check()
        .map_err(EasyOfficeError::InvalidArgument)?;
    parse_http_response(&raw)
}

/// 취소 때문에 끊긴 입출력은 취소 사유로, 그 밖에는 주어진 설명으로 보고한다.
fn io_failure(req: &CopilotLlmRequest, message: String) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(if req.cancel.is_cancelled() {
        CANCELLED_MESSAGE.to_string()
    } else {
        message
    })
}

fn http_frame_complete(raw: &[u8], hdr_end: usize) -> Result<bool, EasyOfficeError> {
    let header_text = std::str::from_utf8(&raw[..hdr_end]).map_err(|_| {
        EasyOfficeError::InvalidArgument("모델 서버 HTTP 응답 헤더가 UTF-8이 아닙니다.".to_string())
    })?;
    let mut is_chunked = false;
    let mut content_length: Option<usize> = None;
    for line in header_text.split("\r\n").skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            let k = name.trim();
            let v = value.trim();
            if k.eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
            {
                is_chunked = true;
            } else if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse::<usize>().ok();
            }
        }
    }
    let body = &raw[hdr_end + 4..];
    if is_chunked {
        return Ok(decode_chunked_bounded(body, MAX_LLM_RESPONSE_BYTES)?.is_some());
    }
    if let Some(cl) = content_length {
        if cl > MAX_LLM_RESPONSE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string(),
            ));
        }
        return Ok(body.len() >= cl);
    }
    Ok(false)
}

fn parse_http_response(raw: &[u8]) -> Result<String, EasyOfficeError> {
    let Some(hdr_end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Err(EasyOfficeError::InvalidArgument(
            "모델 서버의 HTTP 응답 헤더가 올바르지 않습니다.".to_string(),
        ));
    };
    if hdr_end > MAX_HTTP_HEADER_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "모델 서버 HTTP 응답 헤더가 너무 큽니다.".to_string(),
        ));
    }
    let header_text = std::str::from_utf8(&raw[..hdr_end]).map_err(|_| {
        EasyOfficeError::InvalidArgument("모델 서버 HTTP 응답 헤더가 UTF-8이 아닙니다.".to_string())
    })?;
    let mut lines = header_text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| {
            EasyOfficeError::InvalidArgument(
                "모델 서버의 HTTP 상태 줄을 해석할 수 없습니다.".to_string(),
            )
        })?;

    if (300..400).contains(&status_code) {
        return Err(EasyOfficeError::SecurityViolation(
            "모델 API 리다이렉트는 보안 정책상 허용되지 않습니다.".to_string(),
        ));
    }

    let mut is_chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            if name.eq_ignore_ascii_case("content-encoding")
                && !value.eq_ignore_ascii_case("identity")
            {
                return Err(EasyOfficeError::InvalidArgument(
                    "지원하지 않는 콘텐츠 인코딩 응답입니다.".to_string(),
                ));
            }
            if name.eq_ignore_ascii_case("transfer-encoding")
                && value.to_ascii_lowercase().contains("chunked")
            {
                is_chunked = true;
            }
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse::<usize>().ok();
            }
        }
    }

    if !(200..300).contains(&status_code) {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "모델 서버가 HTTP {status_code} 오류를 반환했습니다."
        )));
    }

    let body_raw = &raw[hdr_end + 4..];
    let body_bytes = if is_chunked {
        decode_chunked_bounded(body_raw, MAX_LLM_RESPONSE_BYTES)?.ok_or_else(|| {
            EasyOfficeError::InvalidArgument("청크 응답 본문이 불완전합니다.".to_string())
        })?
    } else {
        if let Some(cl) = content_length {
            if cl > MAX_LLM_RESPONSE_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string(),
                ));
            }
        }
        let slice = match content_length {
            Some(cl) if cl <= body_raw.len() => &body_raw[..cl],
            _ => body_raw,
        };
        if slice.len() > MAX_LLM_RESPONSE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string(),
            ));
        }
        slice.to_vec()
    };

    String::from_utf8(body_bytes).map_err(|_| {
        EasyOfficeError::InvalidArgument("모델 응답 본문이 유효한 UTF-8이 아닙니다.".to_string())
    })
}

fn decode_chunked_bounded(raw: &[u8], limit: usize) -> Result<Option<Vec<u8>>, EasyOfficeError> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < raw.len() {
        let Some(rel_crlf) = raw[pos..].windows(2).position(|w| w == b"\r\n") else {
            return Ok(None);
        };
        let size_line = std::str::from_utf8(&raw[pos..pos + rel_crlf]).map_err(|_| {
            EasyOfficeError::InvalidArgument("청크 크기 헤더가 올바르지 않습니다.".to_string())
        })?;
        let hex_part = size_line.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(hex_part, 16).map_err(|_| {
            EasyOfficeError::InvalidArgument("청크 크기를 해석할 수 없습니다.".to_string())
        })?;
        pos += rel_crlf + 2;
        if chunk_size == 0 {
            if raw[pos..].windows(2).any(|w| w == b"\r\n") || raw.len() >= pos + 2 {
                return Ok(Some(out));
            }
            return Ok(None);
        }
        if out.len().saturating_add(chunk_size) > limit {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string(),
            ));
        }
        if pos.saturating_add(chunk_size + 2) > raw.len() {
            return Ok(None);
        }
        out.extend_from_slice(&raw[pos..pos + chunk_size]);
        pos += chunk_size;
        if raw.get(pos..pos + 2) != Some(b"\r\n") {
            return Err(EasyOfficeError::InvalidArgument(
                "청크 응답 종결자가 올바르지 않습니다.".to_string(),
            ));
        }
        pos += 2;
    }
    Ok(None)
}
