// 참조 구현: mattboldt/typed.js (MIT, Copyright (c) 2023 Matt Boldt, 태그 v2.1.0 커밋 3daa3a47, `src/typed.js`, see THIRD_PARTY_NOTICES.md)
// `stop`·`start`(30-57: 멈춤 상태에서 글자 위치를 기억했다가 이어 쓴다), `begin`(89-110: 시작 지연 뒤 첫 글자), `typewrite`(112-196: 글자 간격을 흔들어
// 타이머로 한 걸음씩 나아가는 루프), `keepTyping`(198-217: 지금까지 쓴 글을 대상에 다시 쓴다), `doneTyping`(219-241), `setPauseStatus`(318-325),
// `toggleBlinking`(329-345: 쉬는 동안 캐럿이 깜박인다), `humanizer`(347-353)를 옮겼다.
// 원본의 `^숫자` 쉼 문법과 백틱 건너뛰기는 문서 글 안의 `^`·`` ` ``을 먹어 버리므로 옮기지 않았다. 문장 끝 쉼과 글자 수 단위 걸음(원본의 `numChars`)은
// 총 시간이 상한을 넘지 않도록 직접 정했다. 원본은 한 번에 한 글자를 UTF-16 단위로 자르지만 여기서는 코드 포인트 단위로 걷는다.

(function (host) {
  "use strict";

  // 글자 간격(ms)의 기준과 한 대상의 총 입력 시간 상한. 긴 글은 한 걸음에 여러 글자를 써서 상한 안에 마친다.
  const DEFAULT_TYPE_SPEED = 24;
  const MAX_TOTAL_MS = 6000;
  const SENTENCE_PAUSE_MS = 180;
  const SENTENCE_END = /[.!?。！？…]$|\n$/;

  // 글을 코드 포인트 경계 인덱스(UTF-16 위치)의 배열로 나눈다. 위치 0은 넣지 않고 끝 위치까지 넣는다.
  function stopsOf(text) {
    const stops = [];
    for (let i = 0; i < text.length; ) {
      const code = text.codePointAt(i);
      i += code > 0xffff ? 2 : 1;
      stops.push(i);
    }
    return stops;
  }

  function createTypist({
    setTimeout = host.setTimeout.bind(host),
    clearTimeout = host.clearTimeout.bind(host),
    random = Math.random,
    typeSpeed = DEFAULT_TYPE_SPEED,
    maxTotalMs = MAX_TOTAL_MS,
    sentencePauseMs = SENTENCE_PAUSE_MS,
    onBlink = () => {},
  } = {}) {
    // 옮김[typed-state] 시작
    let timeout = null;
    // `pause.status`가 참이면 걸음을 멈추고 위치를 기억한다(원본 `setPauseStatus`).
    const pause = { status: false, resume: null };
    // 옮김[typed-state] 끝
    let current = null;

    // 옮김[typed-humanizer] 시작
    const humanizer = (speed) => Math.round((random() * speed) / 2) + speed;
    // 옮김[typed-humanizer] 끝

    function finishRun(run, completed) {
      if (current === run) current = null;
      clearTimeout(timeout);
      timeout = null;
      pause.status = false;
      pause.resume = null;
      onBlink(true);
      run.resolve({ typed: run.stops[run.at - 1] ?? 0, completed });
    }

    // 옮김[typed-typewrite] 시작
    function typewrite(run) {
      if (pause.status === true) {
        pause.resume = () => typewrite(run);
        return;
      }
      const delay = humanizer(typeSpeed);
      timeout = setTimeout(() => {
        if (run.at >= run.stops.length) return doneTyping(run);
        onBlink(false);
        keepTyping(run);
      }, delay);
    }

    function keepTyping(run) {
      run.at = Math.min(run.stops.length, run.at + run.step);
      const typed = run.text.slice(0, run.stops[run.at - 1]);
      run.onText(typed);
      // 문장 끝에서 쉬었다 이어 쓴다.
      const rest = run.at < run.stops.length;
      if (rest && SENTENCE_END.test(typed)) {
        onBlink(true);
        timeout = setTimeout(() => typewrite(run), sentencePauseMs);
        return;
      }
      typewrite(run);
    }

    function doneTyping(run) {
      finishRun(run, true);
    }
    // 옮김[typed-typewrite] 끝

    /**
     * `text`를 한 걸음씩 써 나간다. `onText(prefix)`는 걸음마다 지금까지 쓴 글을 받는다.
     * 끝나면 `{ typed, completed }`로 풀린다(`typed`는 쓴 UTF-16 길이). 한 번에 하나만 돌린다.
     */
    function type(text, onText) {
      const value = String(text ?? "");
      if (current) finishRun(current, false);
      const stops = stopsOf(value);
      // 총 시간이 상한을 넘지 않도록 걸음당 글자 수를 정한다.
      const step = Math.max(1, Math.ceil(stops.length / Math.max(1, Math.floor(maxTotalMs / Math.max(1, typeSpeed * 1.25)))));
      return new Promise((resolve) => {
        const run = { text: value, stops, at: 0, step, onText, resolve };
        current = run;
        if (stops.length === 0) return finishRun(run, true);
        typewrite(run);
      });
    }

    // 옮김[typed-stop] 시작
    function stopTyping() {
      if (!current || pause.status) return;
      onBlink(true);
      pause.status = true;
    }

    function startTyping() {
      if (!current || !pause.status) return;
      pause.status = false;
      const resume = pause.resume;
      pause.resume = null;
      resume?.();
    }
    // 옮김[typed-stop] 끝

    /** 지금까지 쓴 곳에서 멈추고 끝낸다(쓴 글은 남는다). */
    function stop() {
      if (current) finishRun(current, false);
    }

    /** 남은 글을 한꺼번에 쓰고 끝낸다. */
    function finish() {
      const run = current;
      if (!run) return;
      run.at = run.stops.length;
      run.onText(run.text);
      finishRun(run, true);
    }

    return {
      type, stop, finish,
      pause: stopTyping,
      resume: startTyping,
      get running() { return current !== null; },
      get paused() { return pause.status; },
    };
  }

  host.__EASYOFFICE_COPILOT_TYPING__ = Object.freeze({
    DEFAULT_TYPE_SPEED, MAX_TOTAL_MS, SENTENCE_PAUSE_MS, createTypist, stopsOf,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
