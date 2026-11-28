// 참조 구현 (reference implementation): reveal.js (MIT, Copyright (C) 2011-2025 Hakim El Hattab, https://github.com/hakimel/reveal.js),
// commit e240e175232fa2de660a02bc0d7bf5f655b159bb, `js/reveal.js` lines 2328-2398 (`cueAutoSlide`: the slide's own wait first,
// the timer is cued only when the show is not paused and it is not the last slide, or `loop` is on; when it fires the show
// moves to the next slide and cues again). See `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  // 쇼 설정 timing of the slide show (PD-398): the automatic advance after a slide's own wait (`advanceAfterMs`, `advTm`),
  // the repeat of the show after its last slide (`showLoop`, `p:showPr loop`) and the manual advance that ignores the waits
  // (`manualAdvance`, `useTimings="0"`).
  // Reuse (구현 원칙 3단계): (1) a timer library adds nothing to one `setTimeout`; (2) the show's own `goTo`/`nextShown` move the
  // slides; (3) the cue rule of reveal.js's `cueAutoSlide` and its loop condition are ported, without fragments, media and the
  // pause state the editor has no counterpart for.
  const MAX_WAIT_MS = 86_400_000;

  // The wait before `slide` moves on by itself; `0` when there is none (manual advance, no wait, last slide of a show that does not repeat).
  function waitFor(slide, { manual, loop, isLast }) {
    const ms = Number(slide?.advanceAfterMs);
    if (manual || !Number.isFinite(ms) || ms < 0 || slide?.advanceAfterMs === null || slide?.advanceAfterMs === "") return 0;
    if (isLast && !loop) return 0;
    return Math.min(ms, MAX_WAIT_MS);
  }

  // The index the show shows after `idx`: the next shown slide, the first one again when the show repeats, or `-1` at its end.
  function nextIndex(idx, count, loop, nextShown) {
    const next = nextShown(idx + 1);
    if (next < count) return next;
    if (!loop) return -1;
    const first = nextShown(0);
    return first < count ? first : -1;
  }

  function createShowTiming(env = host) {
    let timer = null;
    const cancel = () => {
      if (timer !== null) env.clearTimeout?.(timer);
      timer = null;
    };
    // Cues the move after the wait of `slide`; `fire` runs then.
    function cue(slide, state, fire) {
      cancel();
      const ms = waitFor(slide, state);
      if (ms <= 0) return false;
      timer = env.setTimeout?.(() => {
        timer = null;
        fire();
      }, ms) ?? null;
      return timer !== null;
    }
    return { cue, cancel, isCued: () => timer !== null };
  }

  host.__EASYOFFICE_SHOW_TIMING__ = { waitFor, nextIndex };
  host.__EASYOFFICE_CREATE_SHOW_TIMING__ = createShowTiming;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
