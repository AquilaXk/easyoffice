(function (host) {
  "use strict";

  // Display text of cells that carry a number format (PD-193). The engine renders (`text_format`,
  // the same code path as the TEXT function); the page asks for the cells it is about to draw and
  // keeps the answers by format code and stored value, so a value is sent once. A cell whose answer
  // has not arrived shows its General text and is repainted when it does.

  const MAX_VALUES_PER_CODE = 100000;
  const BATCH_VALUES = 20000;
  const BATCH_CODES = 256;
  const NUMERIC_RE = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/;

  // code -> Map(stored value -> { t: text, c: color }, 0 for "keeps its General display", or null
  // while the engine has been asked and has not answered).
  const cache = new Map();
  // code -> Map(stored value -> text to render), waiting for the next batch.
  let pending = new Map();
  const badCodes = new Set();
  let call = null;
  let onFilled = null;
  let onBad = null;
  let batch = null;
  // Requests go out one after the other, so a value is never in two requests at once.
  let flushing = Promise.resolve();

  function attach(deps) {
    call = deps.call || null;
    onFilled = deps.onFilled || null;
    onBad = deps.onBad || null;
  }

  const normalize = (raw) => host.__EASYOFFICE_SHEET_UTILS__?.normalizeFormattedNumericCell(raw) ?? String(raw).trim();

  function slot(code) {
    let map = cache.get(code);
    if (!map) {
      map = new Map();
      cache.set(code, map);
    }
    return map;
  }

  function takeAnswers(codes, answer) {
    const items = Array.isArray(answer?.items) ? answer.items : [];
    codes.forEach(([code, values], at) => {
      const item = items[at];
      const map = slot(code);
      if (map.size + values.length > MAX_VALUES_PER_CODE) map.clear();
      if (item?.bad === true) {
        badCodes.add(code);
        onBad?.(code);
      }
      let j = 0;
      for (const raw of values.keys()) {
        const text = Array.isArray(item?.t) ? item.t[j] : null;
        map.set(raw, typeof text === "string" ? { t: text, c: item.c?.[j] || "" } : 0);
        j += 1;
      }
    });
  }

  // Sends the waiting values in requests of bounded size and stores the answers.
  async function flush() {
    const waiting = pending;
    pending = new Map();
    batch = null;
    if (!call) return;
    const entries = [...waiting];
    let at = 0;
    while (at < entries.length) {
      const codes = [];
      let count = 0;
      while (at < entries.length && codes.length < BATCH_CODES && count < BATCH_VALUES) {
        const [code, values] = entries[at];
        at += 1;
        codes.push([code, values]);
        count += values.size;
      }
      let answer = null;
      try {
        answer = await call({
          sessionAction: "format",
          items: codes.map(([code, values]) => ({ code, values: [...values.values()] })),
        });
      } catch {
        answer = null;
      }
      takeAnswers(codes, answer);
    }
    onFilled?.();
  }

  function schedule() {
    if (batch === null) {
      batch = flushing.then(flush);
      flushing = batch.catch(() => {});
    }
    return batch;
  }

  function queue(code, raw, map) {
    let values = pending.get(code);
    if (!values) {
      values = new Map();
      pending.set(code, values);
    }
    values.set(raw, normalize(raw));
    map.set(raw, null);
  }

  // The display of `raw` under `code`: { t, c } or null (General). A value the cache lacks is
  // requested and null returned until the answer arrives.
  function shown(code, raw) {
    if (!code || badCodes.has(code)) return null;
    if (code === "@") return { t: raw, c: "" };
    const map = slot(code);
    const hit = map.get(raw);
    if (hit !== undefined) return hit || null;
    if (!call) return null;
    if (!NUMERIC_RE.test(normalize(raw)) && !code.includes("@")) {
      map.set(raw, 0);
      return null;
    }
    queue(code, raw, map);
    schedule();
    return null;
  }

  // Resolves with the display of each of `raws` under `code` ({ t, c } or null), asking the engine now.
  async function preview(code, raws) {
    if (!code || badCodes.has(code) || !call) return raws.map(() => null);
    const map = slot(code);
    const missing = raws.filter((raw) => map.get(raw) === undefined);
    missing.forEach((raw) => queue(code, raw, map));
    if (missing.length > 0) schedule();
    await flushing;
    return raws.map((raw) => map.get(raw) || null);
  }

  host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__ = Object.freeze({
    attach,
    shown,
    preview,
    isBad: (code) => badCodes.has(code),
    // Whether any formatted cell has been drawn (the page then looks ahead of a scroll).
    active: () => cache.size > 0,
    clear: () => {
      cache.clear();
      pending = new Map();
      badCodes.clear();
    },
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
