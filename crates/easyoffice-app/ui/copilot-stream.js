// 참조 구현 (see THIRD_PARTY_NOTICES.md):
// - tauri-apps/tauri (MIT OR Apache-2.0, Copyright (c) 2017 - Present Tauri Apps Contributors, 커밋 9bc364d8) `packages/api/src/core.ts:69-75, 119-222`:
//   `transformCallback`과 `Channel`(Rust가 보낸 `{ message, index }`를 번호 순서대로 넘기고 `{ end, index }`로 정리한다).
// - danny-avila/LibreChat (MIT, Copyright (c) 2026 LibreChat, 커밋 4700c44e) `packages/chat/src/core/transport/sse.ts:176-258`:
//   `reconnectToStream`의 `final`·`failureSeen` 판정(스트림이 `final` 전에 끊기면 오류이고, 이미 오류를 알렸으면 또 알리지 않는다)과 `emitFrame`(모양이 틀린 프레임은 건너뛴다).
// - promplate/partial-json-parser-js (MIT, Copyright (c) 2023 Promplate Dev Team, 커밋 86fd4bcb) `src/index.ts:7-189`, `src/options.ts`:
//   덜 온 JSON을 읽히는 데까지 파싱한다(모델이 쓰는 JSON에서 `reply` 글을 먼저 꺼내 보이려고 쓴다).
// - udecode/plate (MIT, Copyright (c) Ziad Beyens, Dylan Schiemann, Joe Anderson, Felix Feng, 태그 main 커밋 e401aaf9)
//   `apps/www/src/registry/lib/markdown-joiner-transform.ts:60-233`: 끊긴 마크다운 조각(`**`, `[글](주소)`, 목록 표지)을 짝이 맞을 때까지 모아 이어 붙인다.
// 원본 `Channel`은 `@tauri-apps/api` 번들이 필요해서 `__TAURI_INTERNALS__`의 `transformCallback`만으로 다시 옮겼고, `window` 전역 대신 주입한 내부 객체를 쓴다.
// 부분 JSON 파서는 `Allow` 비트 플래그를 모두 허용(`Allow.ALL`)으로 고정해 분기를 걷어 냈다. 조이너는 원본의 지연(`delay`) 없이 글만 이어 준다.
// 이 파일의 나머지(답글 꺼내기, 조각 번호 검사, 프레임 묶음 처리)는 직접 썼다.

(function (host) {
  "use strict";

  // 모델 글을 살피는 한도: 이보다 길면 미리 보여 주기를 멈추고 최종 응답만 쓴다(조각마다 처음부터 다시 파싱하므로 시간을 묶는다).
  const MAX_LIVE_PARSE_CHARS = 64 * 1024;
  const CHANNEL_PREFIX = "__CHANNEL__:";

  // 옮김[tauri-channel] 시작
  /**
   * Rust가 `{ message, index }`로 보내는 조각을 번호 순서대로 `onmessage`에 넘기는 IPC 통로. 번호가 앞서 온 조각은 빈자리가 메워질 때까지 쥔다.
   * `internals`는 데스크톱 브리지(`desktop-bridge.js`)가 넘겨주는 셸 내부 객체다. `id`를 `"__CHANNEL__:번호"` 문자열로 명령 인자에 실어 보낸다.
   */
  function createStreamChannel(internals, onmessage, reportError = (error) => queueMicrotask(() => { throw error; })) {
    let nextMessageIndex = 0;
    const pendingMessages = [];
    let messageEndIndex;
    const deliver = (message) => {
      try {
        onmessage(message);
      } catch (error) {
        // 던지는 처리기가 통로를 멈추게 하면 번호가 안 나아가 뒤 조각이 모두 갇힌다. 오류는 따로 보고한다(기본은 비동기로 다시 던진다).
        reportError(error);
      }
    };
    const channel = {
      id: 0,
      cleanup() {
        internals.unregisterCallback?.(channel.id);
      },
      toJSON() {
        return CHANNEL_PREFIX + channel.id;
      },
    };
    channel.id = internals.transformCallback((rawMessage) => {
      const index = rawMessage.index;
      if ("end" in rawMessage) {
        if (index === nextMessageIndex) channel.cleanup();
        else messageEndIndex = index;
        return;
      }
      const message = rawMessage.message;
      if (index === nextMessageIndex) {
        deliver(message);
        nextMessageIndex += 1;
        while (nextMessageIndex in pendingMessages) {
          const queued = pendingMessages[nextMessageIndex];
          deliver(queued);
          delete pendingMessages[nextMessageIndex];
          nextMessageIndex += 1;
        }
        if (nextMessageIndex === messageEndIndex) channel.cleanup();
      } else {
        pendingMessages[index] = message;
      }
    });
    return channel;
  }
  // 옮김[tauri-channel] 끝

  // 옮김[partial-json] 시작
  class PartialJSON extends Error {}
  class MalformedJSON extends Error {}

  /** 덜 온 JSON 글을 읽히는 데까지 파싱한다. 비었거나 뼈대가 틀리면 던진다. */
  function parseJSON(jsonString) {
    if (typeof jsonString !== "string") throw new TypeError(`expecting str, got ${typeof jsonString}`);
    if (!jsonString.trim()) throw new Error(`${jsonString} is empty`);
    return parsePartial(jsonString.trim());
  }

  function parsePartial(jsonString) {
    const length = jsonString.length;
    let index = 0;

    const markPartialJSON = (msg) => {
      throw new PartialJSON(`${msg} at position ${index}`);
    };
    const throwMalformedError = (msg) => {
      throw new MalformedJSON(`${msg} at position ${index}`);
    };

    const parseAny = () => {
      skipBlank();
      if (index >= length) markPartialJSON("Unexpected end of input");
      if (jsonString[index] === '"') return parseStr();
      if (jsonString[index] === "{") return parseObj();
      if (jsonString[index] === "[") return parseArr();
      if (jsonString.substring(index, index + 4) === "null" || (length - index < 4 && "null".startsWith(jsonString.substring(index)))) {
        index += 4;
        return null;
      }
      if (jsonString.substring(index, index + 4) === "true" || (length - index < 4 && "true".startsWith(jsonString.substring(index)))) {
        index += 4;
        return true;
      }
      if (jsonString.substring(index, index + 5) === "false" || (length - index < 5 && "false".startsWith(jsonString.substring(index)))) {
        index += 5;
        return false;
      }
      return parseNum();
    };

    const parseStr = () => {
      const start = index;
      let escape = false;
      index++; // 여는 따옴표
      while (index < length && (jsonString[index] !== '"' || (escape && jsonString[index - 1] === "\\"))) {
        escape = jsonString[index] === "\\" ? !escape : false;
        index++;
      }
      if (jsonString.charAt(index) === '"') {
        try {
          return JSON.parse(jsonString.substring(start, ++index - Number(escape)));
        } catch (e) {
          throwMalformedError(String(e));
        }
      }
      // 닫는 따옴표가 아직 안 왔다: 온 데까지가 글이다.
      try {
        return JSON.parse(jsonString.substring(start, index - Number(escape)) + '"');
      } catch {
        // 끝이 잘린 이스케이프(`\u12`)는 그 앞까지만 읽는다.
        return JSON.parse(jsonString.substring(start, jsonString.lastIndexOf("\\")) + '"');
      }
    };

    const parseObj = () => {
      index++; // 여는 중괄호
      skipBlank();
      const obj = {};
      try {
        while (jsonString[index] !== "}") {
          skipBlank();
          if (index >= length) return obj;
          const key = parseStr();
          skipBlank();
          index++; // 콜론
          try {
            obj[key] = parseAny();
          } catch {
            return obj;
          }
          skipBlank();
          if (jsonString[index] === ",") index++;
        }
      } catch {
        return obj;
      }
      index++; // 닫는 중괄호
      return obj;
    };

    const parseArr = () => {
      index++; // 여는 대괄호
      const arr = [];
      try {
        while (jsonString[index] !== "]") {
          arr.push(parseAny());
          skipBlank();
          if (jsonString[index] === ",") index++;
        }
      } catch {
        return arr;
      }
      index++; // 닫는 대괄호
      return arr;
    };

    const parseNum = () => {
      if (index === 0) {
        if (jsonString === "-") throwMalformedError("Not sure what '-' is");
        try {
          return JSON.parse(jsonString);
        } catch (e) {
          try {
            return JSON.parse(jsonString.substring(0, jsonString.lastIndexOf("e")));
          } catch {
            throwMalformedError(String(e));
          }
        }
      }
      const start = index;
      if (jsonString[index] === "-") index++;
      while (jsonString[index] && ",]}".indexOf(jsonString[index]) === -1) index++;
      try {
        return JSON.parse(jsonString.substring(start, index));
      } catch (e) {
        if (jsonString.substring(start, index) === "-") markPartialJSON("Not sure what '-' is");
        try {
          return JSON.parse(jsonString.substring(start, jsonString.lastIndexOf("e")));
        } catch {
          throwMalformedError(String(e));
        }
      }
      return undefined;
    };

    const skipBlank = () => {
      while (index < length && " \n\r\t".includes(jsonString[index])) index++;
    };
    return parseAny();
  }
  // 옮김[partial-json] 끝

  // 옮김[markdown-joiner] 시작
  const NEST_BLOCK_LIMIT = 2500;
  const BOLD_PATTERN = /\*\*.*?\*\*/;
  const CODE_LINE_PATTERN = /```[^\s]+/;
  const LINK_PATTERN = /^\[.*?\]\(.*?\)$/;
  const UNORDERED_LIST_PATTERN = /^[*-]\s+.+/;
  const TODO_LIST_PATTERN = /^[*-]\s+\[[ xX]\]\s+.+/;
  const ORDERED_LIST_PATTERN = /^\d+\.\s+.+/;
  const MDX_TAG_PATTERN = /<([A-Za-z][A-Za-z0-9\-_]*)>/;
  const DIGIT_PATTERN = /^[0-9]$/;

  class MarkdownJoiner {
    constructor() {
      this.buffer = "";
      this.documentCharacterCount = 0;
      this.isBuffering = false;
      this.streamingCodeBlock = false;
      this.streamingLargeDocument = false;
      this.streamingTable = false;
    }

    clearBuffer() {
      this.buffer = "";
      this.isBuffering = false;
    }

    isCompleteBold() { return BOLD_PATTERN.test(this.buffer); }
    isCompleteCodeBlockEnd() { return this.buffer.trimEnd() === "```"; }
    isCompleteCodeBlockStart() { return CODE_LINE_PATTERN.test(this.buffer); }
    isCompleteLink() { return LINK_PATTERN.test(this.buffer); }

    isCompleteList() {
      if (UNORDERED_LIST_PATTERN.test(this.buffer) && this.buffer.includes("[")) return TODO_LIST_PATTERN.test(this.buffer);
      return UNORDERED_LIST_PATTERN.test(this.buffer) || ORDERED_LIST_PATTERN.test(this.buffer) || TODO_LIST_PATTERN.test(this.buffer);
    }

    isCompleteMdxTag() { return MDX_TAG_PATTERN.test(this.buffer); }
    isCompleteTableStart() { return this.buffer.startsWith("|") && this.buffer.endsWith("|"); }

    isFalsePositive(char) {
      // 링크가 덜 닫혔으면 버퍼가 30자를 넘어도 오탐이 아니다.
      if (this.buffer.startsWith("[") && this.buffer.includes("http")) return false;
      return char === "\n" || this.buffer.length > 30;
    }

    isLargeDocumentStart() { return this.documentCharacterCount > NEST_BLOCK_LIMIT; }
    isListStartChar(char) { return char === "-" || char === "*" || DIGIT_PATTERN.test(char); }
    isTableExisted() { return this.buffer.length > 10 && !this.buffer.includes("|"); }

    flush() {
      const remaining = this.buffer;
      this.clearBuffer();
      return remaining;
    }

    processText(text) {
      let output = "";
      for (const char of text) {
        if (this.streamingCodeBlock || this.streamingTable || this.streamingLargeDocument) {
          this.buffer += char;
          if (char === "\n") {
            output += this.buffer;
            this.clearBuffer();
          }
          if (this.isCompleteCodeBlockEnd() && this.streamingCodeBlock) {
            this.streamingCodeBlock = false;
            output += this.buffer;
            this.clearBuffer();
          }
          if (this.isTableExisted() && this.streamingTable) {
            this.streamingTable = false;
            output += this.buffer;
            this.clearBuffer();
          }
        } else if (this.isBuffering) {
          this.buffer += char;
          if (this.isCompleteCodeBlockStart()) {
            this.streamingCodeBlock = true;
            continue;
          }
          if (this.isCompleteTableStart()) {
            this.streamingTable = true;
            continue;
          }
          if (this.isLargeDocumentStart()) {
            this.streamingLargeDocument = true;
            continue;
          }
          if (this.isCompleteBold() || this.isCompleteMdxTag() || this.isCompleteList() || this.isCompleteLink()) {
            output += this.buffer;
            this.clearBuffer();
          } else if (this.isFalsePositive(char)) {
            // 오탐: 쥐고 있던 글을 그대로 내보낸다.
            output += this.buffer;
            this.clearBuffer();
          }
        } else if (char === "*" || char === "<" || char === "`" || char === "|" || char === "[" || this.isListStartChar(char)) {
          this.buffer = char;
          this.isBuffering = true;
        } else {
          output += char;
        }
      }
      this.documentCharacterCount += text.length;
      return output;
    }
  }
  // 옮김[markdown-joiner] 끝

  const FENCE_OPEN = /^```[A-Za-z]*\s*\n?/;

  /**
   * 모델이 쓴 글 전체(덜 왔을 수 있다)에서 사용자에게 보일 답글을 꺼낸다. 모델은 `{"reply": "...", ...}` JSON으로 답하므로 `reply` 글만 보인다.
   * JSON이 아니면(`{`로 시작하지 않으면) 글 전체가 답글이다. 연산(`operations`)은 여기서 읽지 않는다: 전송이 끝난 뒤 엔진이 검증한 결과만 쓴다.
   */
  function extractReply(raw) {
    const text = String(raw ?? "").replace(/^\s+/, "").replace(FENCE_OPEN, "");
    if (!text.startsWith("{")) return text;
    if (text.length > MAX_LIVE_PARSE_CHARS) return null;
    try {
      const parsed = parseJSON(text);
      return typeof parsed?.reply === "string" ? parsed.reply : "";
    } catch {
      return "";
    }
  }

  /**
   * 한 요청의 스트림을 화면 글로 바꾼다. `accept(piece)`로 Rust가 보낸 `{ requestId, seq, text }`를 받고, 글이 늘 때마다 `onText(shown)`을 부른다.
   * 번호가 빠지거나 다른 요청의 조각이 섞이면 스트림 글은 믿지 않고(`failureSeen`) 이어 보여 주지 않는다. 이어 받을 방법은 없다: 최종 응답(`finish`)이 항상 온전한 글을 갖고 있어
   * 그 글로 바꾸면 된다(LibreChat은 서버가 이어 주는 스트림이라 다시 붙지만 이 호출은 한 번에 하나의 요청이다).
   */
  function createStreamSession({ requestId, onText }) {
    let raw = "";
    let nextSeq = 0;
    let final = false;
    let failureSeen = false;
    let joiner = new MarkdownJoiner();
    let replySoFar = "";
    let shown = "";
    let stalled = false;

    const refresh = () => {
      const reply = extractReply(raw);
      if (reply === null) {
        stalled = true;
        return;
      }
      if (!reply.startsWith(replySoFar)) {
        // 글이 이어 붙지 않고 바뀌었다(모델이 앞 글을 고쳐 썼다): 처음부터 다시 잇는다.
        joiner = new MarkdownJoiner();
        shown = joiner.processText(reply);
      } else if (reply.length > replySoFar.length) {
        shown += joiner.processText(reply.slice(replySoFar.length));
      }
      replySoFar = reply;
      onText(shown);
    };

    return {
      /** 조각 하나. 받아들였으면 `true`. */
      accept(piece) {
        if (final || failureSeen || stalled) return false;
        if (!piece || piece.requestId !== requestId || typeof piece.text !== "string") return false;
        if (piece.seq !== nextSeq) {
          failureSeen = true;
          return false;
        }
        nextSeq += 1;
        raw += piece.text;
        refresh();
        return true;
      },
      /** 최종 응답이 왔다. 이어 쥔 글을 마저 내보내고 스트림을 닫는다. */
      finish() {
        if (final) return shown;
        final = true;
        shown += joiner.flush();
        return shown;
      },
      get shown() { return shown; },
      get failed() { return failureSeen; },
      get done() { return final; },
    };
  }

  host.__EASYOFFICE_COPILOT_STREAM__ = Object.freeze({
    MAX_LIVE_PARSE_CHARS, CHANNEL_PREFIX, createStreamChannel, parseJSON, PartialJSON, MalformedJSON, MarkdownJoiner, extractReply, createStreamSession,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
