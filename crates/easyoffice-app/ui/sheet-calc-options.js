// Workbook calculation options (`수식 > 계산 옵션`, PD-309): calculation mode (`auto`, `autoNoTable`, `manual`),
// iterative circular evaluation bounds, manual pending indicator (`계산`), and `수식` menu state.
(function (host) {
  "use strict";

  const MAX_ITERATE_COUNT = 32767;
  const MAX_ITERATE_DELTA = 1e100;
  const DEFAULT_OPTIONS = Object.freeze({
    mode: "auto",
    calcOnSave: true,
    fullCalcOnLoad: false,
    iterate: false,
    iterateCount: 100,
    iterateDelta: 0.001,
  });
  const MODE_ACTIONS = Object.freeze({
    auto: "calc-auto",
    autoNoTable: "calc-auto-no-table",
    manual: "calc-manual",
  });
  const CALCULATING_LABEL = "계산";

  let state = { ...DEFAULT_OPTIONS };
  let pendingCalc = false;

  function normalize(raw) {
    if (!raw || typeof raw !== "object") return { ...DEFAULT_OPTIONS };
    const mode = Object.hasOwn(MODE_ACTIONS, raw.mode) ? raw.mode : "auto";
    const calcOnSave = typeof raw.calcOnSave === "boolean" ? raw.calcOnSave : true;
    const fullCalcOnLoad = mode === "manual" && raw.fullCalcOnLoad === true;
    const iterate = raw.iterate === true;
    const iterateCount =
      Number.isInteger(raw.iterateCount) && raw.iterateCount >= 1 && raw.iterateCount <= MAX_ITERATE_COUNT
        ? raw.iterateCount
        : DEFAULT_OPTIONS.iterateCount;
    const rawDelta = raw.iterateDelta;
    const iterateDelta =
      typeof rawDelta === "number" && Number.isFinite(rawDelta) && rawDelta >= 0 && rawDelta <= MAX_ITERATE_DELTA
        ? (rawDelta === 0 ? 0 : rawDelta)
        : DEFAULT_OPTIONS.iterateDelta;
    return { mode, calcOnSave, fullCalcOnLoad, iterate, iterateCount, iterateDelta };
  }

  function isDefault(opts = state) {
    return (
      opts.mode === DEFAULT_OPTIONS.mode &&
      opts.calcOnSave === DEFAULT_OPTIONS.calcOnSave &&
      opts.fullCalcOnLoad === DEFAULT_OPTIONS.fullCalcOnLoad &&
      opts.iterate === DEFAULT_OPTIONS.iterate &&
      opts.iterateCount === DEFAULT_OPTIONS.iterateCount &&
      Math.abs(opts.iterateDelta - DEFAULT_OPTIONS.iterateDelta) <= 1e-15
    );
  }

  function syncMenu(doc) {
    if (!doc?.querySelectorAll) return;
    for (const [mode, act] of Object.entries(MODE_ACTIONS)) {
      const checked = mode === state.mode ? "true" : "false";
      doc.querySelectorAll('.md-item[data-sheet-act="' + act + '"]').forEach((el) => {
        el.setAttribute("aria-checked", checked);
      });
    }
  }

  function markPending(doc) {
    pendingCalc = true;
    const fb = doc?.getElementById?.("status-command-feedback");
    if (fb) fb.textContent = CALCULATING_LABEL;
  }

  function clearPending(doc) {
    pendingCalc = false;
    const fb = doc?.getElementById?.("status-command-feedback");
    if (fb && fb.textContent === CALCULATING_LABEL) fb.textContent = "";
  }

  function set(raw, doc) {
    state = normalize(raw);
    if (state.mode !== "manual") clearPending(doc);
    else pendingCalc = false;
    syncMenu(doc);
  }

  function setMode(nextMode, doc) {
    if (!Object.hasOwn(MODE_ACTIONS, nextMode)) return false;
    state = {
      ...state,
      mode: nextMode,
      fullCalcOnLoad: nextMode === "manual" ? state.fullCalcOnLoad : false,
    };
    if (nextMode !== "manual") clearPending(doc);
    syncMenu(doc);
    return true;
  }

  host.__EASYOFFICE_SHEET_CALC_OPTIONS__ = {
    set,
    restore: (snap, doc) => set(snap, doc),
    get: () => (isDefault(state) ? null : { ...state }),
    mode: () => state.mode,
    isManual: () => state.mode === "manual",
    setMode,
    evalField: () =>
      state.mode === "manual" || state.iterate ? { calcOptions: { ...state } } : {},
    saveField: () => (isDefault(state) ? {} : { calcOptions: { ...state } }),
    markPending,
    clearPending,
    isPending: () => pendingCalc,
    syncMenu,
    DEFAULT_OPTIONS,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
