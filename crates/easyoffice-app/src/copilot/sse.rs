//! 참조 구현: PizzasBear/sse-rs `sse-core` (MIT OR Apache-2.0, Copyright (c) 2026 Max Shteimberg, 커밋 aad868a4, `sse-core/src/decode.rs`,
//! see THIRD_PARTY_NOTICES.md)의 `SseDecoder` 상태기계를 엔진의 무의존(PD-053) 규칙에 맞게 옮겼다.
//!
//! 바이트가 끊겨 들어와도 이벤트 경계(빈 줄)에서만 `MessageEvent`를 내보낸다. 원본과 같이 입력 버퍼 밖에 읽다 만 바이트를 두지 않고
//! 상태(`Mode`)에 이어 읽을 자리를 기억한다. 원본에서 바꾼 점은 다음과 같다.
//! - `bytes::Buf` 대신 `&mut &[u8]`를 받고, `memchr`는 `iter().position`으로 바꿨다.
//! - `id:`·`retry:` 줄은 이어받기·재연결에 쓰는 필드인데 이 엔진의 모델 호출은 이어받지 않으므로 건너뛰는 줄(`Ignore`)로 처리한다.
//! - 이벤트 하나의 데이터·이름은 `max_payload_size`를 넘으면 그 이벤트를 통째로 버리고 오류를 낸다(원본과 같다).
//!   본문 전체 상한(512 KiB)은 전송 계층이 따로 지키므로, 이 상한은 한 이벤트의 크기를 묶는 용도다.

use std::fmt;
use std::num::{NonZeroU8, NonZeroUsize};

/// 이벤트 하나가 담을 수 있는 데이터·이름 바이트 상한.
pub const MAX_EVENT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEvent {
    /// 이벤트 이름. `event:` 줄이 없으면 `message`다.
    pub event: String,
    pub data: String,
}

/// 이벤트 하나의 데이터나 이름이 상한을 넘었다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadTooLargeError;

impl fmt::Display for PayloadTooLargeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SSE 이벤트 하나가 허용 크기를 넘었습니다.")
    }
}

// 옮김[sse-mode] 시작
#[derive(Debug, Clone, Copy)]
enum ValueMode {
    Data,
    Event,
}

impl ValueMode {
    const fn field_name(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Event => "event",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Bom { bytes_read: u8 },
    Field(Option<(ValueMode, NonZeroU8)>),
    Value(ValueMode),
    Ignore,
    PostCr,
    PostColon(ValueMode),
}

#[derive(Debug, Clone)]
pub struct SseDecoder {
    mode: Mode,
    event_buf: Vec<u8>,
    data_buf: Vec<u8>,
    max_payload_size: NonZeroUsize,
    corrupted: bool,
}
// 옮김[sse-mode] 끝

impl Default for SseDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// 옮김[sse-decoder] 시작
impl SseDecoder {
    pub fn new() -> Self {
        Self::with_limit(NonZeroUsize::new(MAX_EVENT_BYTES).expect("nonzero"))
    }

    pub fn with_limit(max_payload_size: NonZeroUsize) -> Self {
        Self {
            mode: Mode::Bom { bytes_read: 0 },
            event_buf: Vec::new(),
            data_buf: Vec::new(),
            max_payload_size,
            corrupted: false,
        }
    }

    fn mark_corrupted(&mut self) {
        self.clear_bufs();
        self.corrupted = true;
    }

    /// 두 `data` 줄 사이의 줄바꿈도 내용과 같은 상한을 받는다. `consume_until_newline`이 보지 못하는 바이트라서,
    /// 이걸 세지 않으면 빈 `data` 줄만 이어 보내 `data_buf`를 한없이 키울 수 있다.
    fn push_data_newline(&mut self) -> Result<(), PayloadTooLargeError> {
        if self.max_payload_size.get() < self.data_buf.len() {
            self.mark_corrupted();
            return Err(PayloadTooLargeError);
        }
        self.data_buf.push(b'\n');
        Ok(())
    }

    fn clear_bufs(&mut self) {
        self.data_buf.clear();
        self.event_buf.clear();
    }

    fn dispatch(&mut self, cr: bool) -> Option<MessageEvent> {
        self.mode = if cr { Mode::PostCr } else { Mode::Field(None) };
        if self.corrupted {
            self.corrupted = false;
            return None;
        }
        // 데이터가 없으면 이벤트도 없다.
        if self.data_buf.is_empty() {
            self.event_buf.clear();
            return None;
        }
        if self.data_buf.last() == Some(&b'\n') {
            self.data_buf.pop();
        }
        let data = String::from_utf8_lossy(&self.data_buf).into_owned();
        self.data_buf.clear();
        let event = if self.event_buf.is_empty() {
            "message".to_string()
        } else {
            String::from_utf8_lossy(&self.event_buf).into_owned()
        };
        self.event_buf.clear();
        Some(MessageEvent { event, data })
    }

    /// 버퍼에서 바이트를 읽어 이벤트 하나를 낸다. 읽은 만큼만 `buf`를 앞으로 옮기고, `None`이면 버퍼를 다 읽은 것이니
    /// 다음 바이트가 오면 다시 부른다. 상한을 넘으면 그 이벤트를 버리고 다음 경계에서 이어 읽는다.
    pub fn next(&mut self, buf: &mut &[u8]) -> Option<Result<MessageEvent, PayloadTooLargeError>> {
        loop {
            let chunk = *buf;
            if chunk.is_empty() {
                return None;
            }
            match &mut self.mode {
                Mode::Bom { bytes_read } => {
                    const BOM: &[u8; 3] = b"\xef\xbb\xbf";
                    if chunk[0] != BOM[*bytes_read as usize] {
                        self.mode = match *bytes_read {
                            0 => Mode::Field(None),
                            _ => Mode::Ignore,
                        };
                        continue;
                    }
                    *buf = &chunk[1..];
                    *bytes_read += 1;
                    if BOM.len() <= *bytes_read as usize {
                        self.mode = Mode::Field(None);
                    }
                }
                Mode::Field(None) => {
                    let b0 = chunk[0];
                    *buf = &chunk[1..];
                    let mode = match b0 {
                        b'd' if !self.corrupted => ValueMode::Data,
                        b'e' if !self.corrupted => ValueMode::Event,
                        b'\n' | b'\r' => match self.dispatch(b0 == b'\r') {
                            Some(ev) => return Some(Ok(ev)),
                            None => continue,
                        },
                        // 주석과 쓰지 않는 필드(`id`, `retry` 포함)는 줄 끝까지 건너뛴다.
                        _ => {
                            self.mode = Mode::Ignore;
                            skip_line(&mut self.mode, buf);
                            continue;
                        }
                    };
                    self.mode = Mode::Field(Some((mode, NonZeroU8::MIN)));
                }
                &mut Mode::Field(Some((mode, ref mut len))) => {
                    let cmp = &mode.field_name().as_bytes()[len.get() as usize..];
                    if std::iter::zip(chunk, cmp).any(|(a, b)| a != b) {
                        self.mode = Mode::Ignore;
                        continue;
                    }
                    let Some(&post) = chunk.get(cmp.len()) else {
                        *len = NonZeroU8::new(len.get() + chunk.len() as u8).expect("small");
                        *buf = &chunk[chunk.len()..];
                        continue;
                    };
                    *buf = &chunk[cmp.len() + 1..];
                    match post {
                        b'\n' => self.mode = Mode::Field(None),
                        b'\r' => self.mode = Mode::PostCr,
                        b':' => {
                            if matches!(mode, ValueMode::Event) {
                                self.event_buf.clear();
                            }
                            // 콜론 바로 뒤의 공백 한 칸은 값에 넣지 않는다. 버퍼가 콜론에서 끝났으면 `PostColon`에서 이어 본다.
                            self.mode = match buf.first() {
                                Some(&b' ') => {
                                    *buf = &buf[1..];
                                    Mode::Value(mode)
                                }
                                Some(_) => Mode::Value(mode),
                                None => Mode::PostColon(mode),
                            };
                            continue;
                        }
                        _ => {
                            self.mode = Mode::Ignore;
                            continue;
                        }
                    }
                    // 콜론 없는 필드 이름은 빈 값이다: `data`는 빈 줄을 하나 더하고 `event`는 이름을 지운다.
                    match mode {
                        ValueMode::Data => {
                            if let Err(err) = self.push_data_newline() {
                                return Some(Err(err));
                            }
                        }
                        ValueMode::Event => self.event_buf.clear(),
                    }
                }
                Mode::Value(ValueMode::Data) => {
                    match consume_until_newline(
                        &mut self.mode,
                        Some(&mut self.data_buf),
                        self.max_payload_size,
                        buf,
                    ) {
                        Ok(true) => {
                            if let Err(err) = self.push_data_newline() {
                                return Some(Err(err));
                            }
                        }
                        Ok(false) => {}
                        Err(err) => {
                            self.mark_corrupted();
                            return Some(Err(err));
                        }
                    }
                }
                Mode::Value(ValueMode::Event) => {
                    if let Err(err) = consume_until_newline(
                        &mut self.mode,
                        Some(&mut self.event_buf),
                        self.max_payload_size,
                        buf,
                    ) {
                        self.mark_corrupted();
                        return Some(Err(err));
                    }
                }
                Mode::Ignore => skip_line(&mut self.mode, buf),
                Mode::PostCr => {
                    if chunk[0] == b'\n' {
                        *buf = &chunk[1..];
                    }
                    self.mode = Mode::Field(None);
                }
                Mode::PostColon(value) => {
                    if chunk[0] == b' ' {
                        *buf = &chunk[1..];
                    }
                    self.mode = Mode::Value(*value);
                }
            }
        }
    }
}

/// 줄 끝까지 읽어 `out`에 더한다. 줄이 끝났으면 `true`, 버퍼가 먼저 끝났으면 `false`.
fn consume_until_newline(
    mode: &mut Mode,
    out: Option<&mut Vec<u8>>,
    max_size: NonZeroUsize,
    buf: &mut &[u8],
) -> Result<bool, PayloadTooLargeError> {
    let chunk = *buf;
    if chunk.is_empty() {
        return Ok(false);
    }
    let Some(i) = chunk.iter().position(|&b| b == b'\r' || b == b'\n') else {
        if let Some(out) = out {
            if max_size.get() < out.len() + chunk.len() {
                out.clear();
                *mode = Mode::Ignore;
                return Err(PayloadTooLargeError);
            }
            out.extend_from_slice(chunk);
        }
        *buf = &chunk[chunk.len()..];
        return Ok(false);
    };
    if let Some(out) = out {
        if max_size.get() < out.len() + i {
            out.clear();
            *mode = Mode::Ignore;
            return Err(PayloadTooLargeError);
        }
        out.extend_from_slice(&chunk[..i]);
    }
    *mode = if chunk[i] == b'\r' {
        Mode::PostCr
    } else {
        Mode::Field(None)
    };
    *buf = &chunk[i + 1..];
    Ok(true)
}

/// 값을 모으지 않고 줄 끝까지 건너뛴다.
fn skip_line(mode: &mut Mode, buf: &mut &[u8]) {
    let _ = consume_until_newline(mode, None, NonZeroUsize::MIN, buf);
}
// 옮김[sse-decoder] 끝

#[cfg(test)]
mod tests {
    use super::*;

    fn message(event: &str, data: &str) -> MessageEvent {
        MessageEvent {
            event: event.to_string(),
            data: data.to_string(),
        }
    }

    // 옮김[sse-tests] 시작
    // 원본 `hard_parse`: 한 바이트씩 먹인 결과와 통째로 먹인 결과가 같아야 한다.
    #[test]
    fn hard_parse_byte_by_byte_equals_whole_buffer() {
        let bytes = "\u{FEFF}data: x\n\n:\n\nevent: my-event\r\ndata:line1\ndata: line2\ndata\n:\nid: my-id\n:should be ignored too\rretry:42\nretry:\n\ndata:second\n\ndata:ignored\n";
        let expected = [
            message("message", "x"),
            message("my-event", "line1\nline2\n"),
            message("message", "second"),
        ];
        let mut decoder = SseDecoder::new();
        let byte_by_byte: Vec<MessageEvent> = bytes
            .bytes()
            .filter_map(|b| decoder.next(&mut std::slice::from_ref(&b)))
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(byte_by_byte, expected);
        let mut decoder = SseDecoder::new();
        let mut buf = bytes.as_bytes();
        let whole: Vec<MessageEvent> = std::iter::from_fn(|| decoder.next(&mut buf))
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(whole, expected);
    }

    // 원본 `test_valueless_fields`: 콜론 없는 `event`는 이름을 지우고 콜론 없는 `data`는 빈 줄을 더한다.
    #[test]
    fn valueless_fields_follow_the_spec() {
        fn messages(bytes: &str) -> Vec<MessageEvent> {
            let mut decoder = SseDecoder::new();
            let mut buf = bytes.as_bytes();
            std::iter::from_fn(|| decoder.next(&mut buf))
                .map(Result::unwrap)
                .collect()
        }
        assert_eq!(
            messages("event: custom\nevent\ndata: x\n\n"),
            [message("message", "x")]
        );
        assert_eq!(
            messages("data: a\ndata\ndata: b\n\n"),
            [message("message", "a\n\nb")]
        );
    }

    // 원본 `test_valueless_data_respects_the_limit`: 빈 `data` 줄만 이어도 상한이 걸리고, 그 뒤에도 디코더를 쓸 수 있다.
    #[test]
    fn valueless_data_respects_the_limit() {
        let limit = NonZeroUsize::new(10).unwrap();
        let input = "data\n".repeat(100) + "\n";
        let mut buf = input.as_bytes();
        let mut decoder = SseDecoder::with_limit(limit);
        assert_eq!(decoder.next(&mut buf), Some(Err(PayloadTooLargeError)));
        assert_eq!(decoder.next(&mut buf), None);
        assert!(buf.is_empty());
        let mut buf: &[u8] = b"data: after\n\n";
        assert_eq!(
            decoder.next(&mut buf),
            Some(Ok(message("message", "after")))
        );
        let input = "data\n".repeat(limit.get() + 1) + "\n";
        let mut buf = input.as_bytes();
        let mut decoder = SseDecoder::with_limit(limit);
        assert_eq!(
            decoder.next(&mut buf),
            Some(Ok(message("message", &"\n".repeat(limit.get()))))
        );
    }
    // 옮김[sse-tests] 끝

    #[test]
    fn an_oversized_event_is_dropped_whole_and_the_stream_goes_on() {
        let limit = NonZeroUsize::new(16).unwrap();
        let mut decoder = SseDecoder::with_limit(limit);
        let input = format!("data: {}\n\ndata: ok\n\n", "가".repeat(40));
        let mut buf = input.as_bytes();
        assert_eq!(decoder.next(&mut buf), Some(Err(PayloadTooLargeError)));
        assert_eq!(decoder.next(&mut buf), Some(Ok(message("message", "ok"))));
        assert_eq!(decoder.next(&mut buf), None);
    }

    #[test]
    fn a_multibyte_character_split_across_reads_survives() {
        let mut decoder = SseDecoder::new();
        let full = "data: 안녕\n\n".as_bytes();
        let (head, tail) = full.split_at(8);
        let mut out = Vec::new();
        for mut part in [head, tail] {
            while let Some(ev) = decoder.next(&mut part) {
                out.push(ev.unwrap());
            }
        }
        assert_eq!(out, [message("message", "안녕")]);
    }
}
