//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., `server/src/modules/copilot/index.ts`, see THIRD_PARTY_NOTICES.md)
//!
//! 진행 중인 모델 요청을 요청 ID로 찾아 끊는다. 참조 구현은 요청마다 `AbortController`를 두고 취소
//! 요청이 오면 `abort()`를 부른다. 여기서는 블로킹 전송이므로 요청이 연 소켓의 복제본을 쥐고 있다가
//! 취소 때 `shutdown`해서 막힌 읽기·쓰기가 곧바로 오류로 돌아오게 한다.

use easyoffice_formats::EasyOfficeError;
use std::collections::VecDeque;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

pub const MAX_REQUEST_ID_BYTES: usize = 64;
/// 동시에 진행 중인 요청 상한. 넘으면 새 요청을 거부한다.
const MAX_LIVE_REQUESTS: usize = 64;
/// 요청이 등록되기 전에 도착한 취소를 기억하는 개수.
const MAX_EARLY_CANCELS: usize = 32;
pub const CANCELLED_MESSAGE: &str = "요청을 취소했습니다.";

#[derive(Default)]
struct CancelState {
    cancelled: AtomicBool,
    socket: Mutex<Option<TcpStream>>,
}

/// 한 모델 요청의 취소 신호. 복제본은 같은 상태를 가리킨다.
#[derive(Clone, Default)]
pub struct CancelHandle(Arc<CancelState>);

impl PartialEq for CancelHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for CancelHandle {}

impl std::fmt::Debug for CancelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CancelHandle(cancelled={})", self.is_cancelled())
    }
}

impl CancelHandle {
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(CANCELLED_MESSAGE.to_string())
        } else {
            Ok(())
        }
    }

    /// 요청이 연 소켓을 등록한다. 이미 취소됐다면 곧바로 닫는다.
    pub fn attach(&self, stream: &TcpStream) {
        if let Ok(clone) = stream.try_clone() {
            *self.0.socket.lock().unwrap_or_else(PoisonError::into_inner) = Some(clone);
        }
        if self.is_cancelled() {
            self.close_socket();
        }
    }

    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
        self.close_socket();
    }

    fn close_socket(&self) {
        if let Some(stream) = self
            .0
            .socket
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

struct Registry {
    live: Vec<(String, CancelHandle)>,
    early: VecDeque<String>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    live: Vec::new(),
    early: VecDeque::new(),
});

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn validate_request_id(raw: &str) -> Result<&str, EasyOfficeError> {
    let id = raw.trim();
    let ok = !id.is_empty()
        && id.len() <= MAX_REQUEST_ID_BYTES
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(id)
    } else {
        Err(EasyOfficeError::InvalidArgument(
            "requestId는 영숫자·-·_ 64자 이하여야 합니다.".to_string(),
        ))
    }
}

/// 등록된 요청이 끝나면 목록에서 지운다.
pub struct RequestGuard {
    id: String,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        registry().live.retain(|(id, _)| *id != self.id);
    }
}

/// 요청 ID로 취소 신호를 등록한다. 이 ID의 취소가 먼저 도착했다면 이미 취소된 상태로 돌려준다.
pub fn register(id: &str) -> Result<(CancelHandle, RequestGuard), EasyOfficeError> {
    let id = validate_request_id(id)?;
    let handle = CancelHandle::default();
    let mut reg = registry();
    if reg.live.iter().any(|(live, _)| live == id) {
        return Err(EasyOfficeError::InvalidArgument(
            "이미 진행 중인 requestId입니다.".to_string(),
        ));
    }
    if reg.live.len() >= MAX_LIVE_REQUESTS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "동시에 진행할 수 있는 코파일럿 요청이 상한({MAX_LIVE_REQUESTS}개)을 넘었습니다."
        )));
    }
    if let Some(pos) = reg.early.iter().position(|early| early == id) {
        reg.early.remove(pos);
        handle.cancel();
    }
    reg.live.push((id.to_string(), handle.clone()));
    Ok((handle, RequestGuard { id: id.to_string() }))
}

/// 진행 중인 요청을 끊는다. 요청이 아직 등록되기 전이면 도착 순서가 뒤집힌 것으로 보고 기억해 둔다.
/// 진행 중인 요청을 찾았으면 `true`.
pub fn cancel_request(id: &str) -> Result<bool, EasyOfficeError> {
    let id = validate_request_id(id)?;
    let mut reg = registry();
    if let Some((_, handle)) = reg.live.iter().find(|(live, _)| live == id) {
        handle.cancel();
        return Ok(true);
    }
    if reg.early.len() >= MAX_EARLY_CANCELS {
        reg.early.pop_front();
    }
    reg.early.push_back(id.to_string());
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn request_id_is_bounded_and_ascii() {
        assert!(validate_request_id("cp-lz3k_1").is_ok());
        for bad in [
            "",
            "  ",
            "a b",
            "한글",
            "a/b",
            &"x".repeat(MAX_REQUEST_ID_BYTES + 1),
        ] {
            assert!(validate_request_id(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn cancel_closes_the_attached_socket_and_early_cancel_is_remembered() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (handle, guard) = register("unit-live-1").unwrap();
        handle.attach(&stream);
        assert!(cancel_request("unit-live-1").unwrap());
        assert!(handle.is_cancelled());
        let mut buf = [0u8; 1];
        let n = std::io::Read::read(&mut &stream, &mut buf).unwrap_or(0);
        assert_eq!(n, 0, "a shut down socket reads as closed");
        drop(guard);
        assert!(
            !cancel_request("unit-live-1").unwrap(),
            "gone after the guard drops"
        );

        assert!(!cancel_request("unit-early-1").unwrap());
        let (late, _guard) = register("unit-early-1").unwrap();
        assert!(
            late.is_cancelled(),
            "a cancel that arrived first still applies"
        );
    }

    #[test]
    fn duplicate_live_id_is_rejected() {
        let (_h, _g) = register("unit-dup-1").unwrap();
        assert!(register("unit-dup-1").is_err());
    }
}
