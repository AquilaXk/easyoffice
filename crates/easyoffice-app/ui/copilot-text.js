// 참조 구현: lautis/unicode-substring (MIT, Copyright (c) 2015 Ville Lautanala, 커밋 088e12b `index.js:1-29`, see THIRD_PARTY_NOTICES.md).
// `charAt`의 서로게이트 쌍 판별(1-11)을 `unitsAt`로, 글자 수를 세며 자르는 `slice`(13-29)를 앞에서 자르는 `clipCodePoints`로 옮겼다.
// 원본은 문자열 전체를 훑고 한 글자씩 이어 붙이지만 여기서는 상한에서 멈추고 한 번만 자른다.

(function (host) {
  "use strict";

  // 코파일럿 요청에 싣는 글은 JSON으로 엔진에 간다. 짝 없는 서로게이트가 하나라도 있으면 엔진이 요청 전체를 거부하므로,
  // 모든 글은 코드 포인트 경계에서 자른다.
  function unitsAt(string, index) {
    const first = string.charCodeAt(index);
    if (first >= 0xd800 && first <= 0xdbff && index + 1 < string.length) {
      const second = string.charCodeAt(index + 1);
      if (second >= 0xdc00 && second <= 0xdfff) return 2;
    }
    return 1;
  }

  // 앞쪽 `max` 코드 포인트까지. 이미 짧으면 그대로 돌려준다(UTF-16 단위 수가 `max` 이하면 코드 포인트도 `max` 이하).
  function clipCodePoints(value, max) {
    const string = String(value ?? "");
    if (string.length <= max) return string;
    let end = 0;
    for (let count = 0; count < max && end < string.length; count += 1) end += unitsAt(string, end);
    return string.slice(0, end);
  }

  // 엔진은 질문을 UTF-8 16,384바이트까지 받는다(`MAX_PROMPT_BYTES`). 글자 수로 재면 한글(3바이트)이 상한을 넘으므로 바이트로 잰다.
  // 코드 포인트 경계에서 자르며, 이미 상한 이하면 그대로 돌려준다. 코드 포인트의 바이트 수는 `unitsAt`과 같은 걸음으로 센다.
  function utf8Length(code) {
    return code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
  }

  function clipUtf8Bytes(value, maxBytes) {
    const string = String(value ?? "");
    // UTF-16 단위 하나는 UTF-8로 최대 3바이트라, 이보다 짧으면 셀 필요가 없다.
    if (string.length * 3 <= maxBytes) return string;
    let bytes = 0;
    let end = 0;
    while (end < string.length) {
      const units = unitsAt(string, end);
      const cost = utf8Length(string.codePointAt(end));
      if (bytes + cost > maxBytes) break;
      bytes += cost;
      end += units;
    }
    return end >= string.length ? string : string.slice(0, end);
  }

  host.__EASYOFFICE_COPILOT_TEXT__ = Object.freeze({ clipCodePoints, clipUtf8Bytes });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
