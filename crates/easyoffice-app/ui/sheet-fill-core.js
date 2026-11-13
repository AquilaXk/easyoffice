// Auto fill of sheet cells (PD-206): which values a dragged fill handle writes. Pure data and
// string logic, no DOM and no engine call.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9):
//   packages/sheets/src/services/auto-fill/tools.ts   -> matchExtendNumber (:167), isEqualDiff (:279),
//                                                        isEqualRatio (:376), fillSeries (:400),
//                                                        forecast (:447), fillExtendNumber (:480),
//                                                        LOOP_SERIES (:610), fillLoopSeries (:659)
//   packages/sheets/src/services/auto-fill/rules.ts   -> the rule order and the isContinue tests
//                                                        (date :41, number :83, extendNumber :105, loopSeries :366)
//   packages/sheets/src/controllers/auto-fill.controller.ts
//                                                     -> _getCopyData (:250, grouping of runs of one kind),
//                                                        _shouldDisableSeries (:617), _getPreferredApplyType (:628)
//   packages/sheets-ui/src/controllers/auto-fill-ui.controller.ts
//                                                     -> detectAutoFillRange (:305, the double click target)
// Differences: the cycle lists are Korean (요일, 월) and English instead of Chinese; a month name is
// tried before the number-at-the-end rule so "1월" continues as "2월"; the cycle length, not 7, is
// the stride of a repeated list; a negative cycle position wraps by modulo; the first matching
// list wins (the reference concatenates every list that holds the text); a step takes the cell it
// copies from by its place in the source, so a fill above or left of the source repeats the source
// from its far end; a fill is refused above MAX_FILL_STEPS lines or MAX_FILL_CELLS cells.
(function (host) {
  "use strict";

  // Limits of one fill (a larger one is refused): lines past the source edge, and cells written.
  const MAX_FILL_STEPS = 100000;
  const MAX_FILL_CELLS = 1000000;
  const NUMERIC_RE = /^-?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
  const FORECAST_SCALE = 100000;
  // A longer run of digits is not a counter (it stays an exact integer below 2^53).
  const MAX_EXTEND_DIGITS = 15;
  const DIGIT_RUN = /\d+/g;
  const plainStyle = (style) => host.__EASYOFFICE_SHEET_PASTE_CORE__.plainStyle(style);

  const WEEK_KO = ["일", "월", "화", "수", "목", "금", "토"];
  const MONTH_KO = Array.from({ length: 12 }, (_, i) => i + 1 + "월");
  const LOOP_SERIES = Object.freeze({
    koWeek1: WEEK_KO,
    koWeek2: WEEK_KO.map((d) => d + "요일"),
    koMonth: MONTH_KO,
    enWeek1: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    enWeek2: ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
    enMonth1: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    enMonth2: ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
  });
  const LOOP_NAMES = Object.keys(LOOP_SERIES);

  function loopOf(text) {
    for (const name of LOOP_NAMES) {
      if (LOOP_SERIES[name].includes(text)) return { name, series: LOOP_SERIES[name] };
    }
    return null;
  }

  // The last run of digits of a text, with what comes before and after it (null without digits).
  function matchExtendNumber(text) {
    let last = null;
    for (const m of String(text).matchAll(DIGIT_RUN)) last = m;
    if (!last) return null;
    const raw = last[0];
    return {
      raw,
      number: Number(raw),
      before: text.slice(0, last.index),
      after: text.slice(last.index + raw.length),
    };
  }

  const isNumberText = (v) => v !== "" && NUMERIC_RE.test(v.trim()) && Number.isFinite(Number(v));

  // The kind of one cell: "formula", "date", "number", "loop" (a name from a cycle list), "extend"
  // (text that ends in or holds a number) or "other".
  function classify(entry, isDateCode) {
    if (!entry) return { type: "other" };
    if (entry.formula) return { type: "formula" };
    const v = String(entry.value ?? "");
    const code = entry.style?.numFmt || "";
    if (code !== "@" && isNumberText(v)) return { type: code && isDateCode(code) ? "date" : "number" };
    const loop = loopOf(v);
    if (loop) return { type: "loop", key: loop.name };
    const ext = matchExtendNumber(v);
    if (ext && ext.raw.length <= MAX_EXTEND_DIGITS) return { type: "extend", key: ext.before + "\u0000" + ext.after };
    return { type: "other" };
  }

  // Runs of neighbouring cells of one kind: each run gets its own series.
  function groupRuns(kinds) {
    const runs = [];
    kinds.forEach((kind, i) => {
      const run = runs[runs.length - 1];
      if (run && run.type === kind.type && run.key === kind.key) run.idx.push(i);
      else runs.push({ type: kind.type, key: kind.key, idx: [i] });
    });
    return runs;
  }

  // Equal steps; one change of sign is allowed (the reference's `isEqualDiff`).
  function isEqualDiff(arr) {
    if (arr.length < 3) return true;
    let step = arr[1] - arr[0];
    let changed = false;
    for (let i = 2; i < arr.length; i += 1) {
      const cur = arr[i] - arr[i - 1];
      if (cur === step) continue;
      if (changed || cur !== -step) return false;
      step = cur;
      changed = true;
    }
    return true;
  }

  function isEqualRatio(arr) {
    const ratio = arr[1] / arr[0];
    for (let i = 1; i < arr.length; i += 1) {
      if (arr[i] / arr[i - 1] !== ratio) return false;
    }
    return true;
  }

  // Least squares line through (1..n, ys) as `{ a, b }` (y = a + b * x); a single point or equal xs
  // move by +-1 per line.
  function fitLine(ys, forward) {
    const n = ys.length;
    const ax = (n + 1) / 2;
    let ay = 0;
    for (let i = 0; i < n; i += 1) ay += ys[i];
    ay /= n;
    let sd = 0;
    let sn = 0;
    for (let i = 0; i < n; i += 1) {
      sd += (i + 1 - ax) * (ys[i] - ay);
      sn += (i + 1 - ax) * (i + 1 - ax);
    }
    const b = sn === 0 ? (forward ? 1 : -1) : sd / sn;
    return { a: ay - b * ax, b };
  }

  // Each *Values function answers the `len` values that follow the cells of one run, in the order
  // the fill walks away from the source (`cells` are in source order; `reverse` is a fill upwards or
  // to the left), or null when the run is copied.
  function numberValues(cells, len, reverse) {
    const nums = cells.map((c) => Number(c.value));
    if (reverse) nums.reverse();
    const out = [];
    if (nums.length > 2 && isEqualRatio(nums)) {
      const ratio = nums[1] / nums[0];
      for (let i = 1; i <= len; i += 1) out.push(Number((nums[nums.length - 1] * ratio ** i).toPrecision(15)));
    } else {
      const { a, b } = fitLine(nums, !reverse);
      for (let i = 1; i <= len; i += 1) out.push(Math.round((a + b * (nums.length + i)) * FORECAST_SCALE) / FORECAST_SCALE);
    }
    return out.every(Number.isFinite) ? out.map(String) : null;
  }

  function extendValues(cells, len, reverse) {
    const texts = cells.map((c) => String(c.value));
    const parts = texts.map(matchExtendNumber);
    if (reverse) {
      texts.reverse();
      parts.reverse();
    }
    let step = reverse ? -1 : 1;
    if (parts.length > 1) {
      const nums = parts.map((p) => p.number);
      if (!isEqualDiff(nums)) return null;
      step = nums[nums.length - 1] - nums[nums.length - 2];
    }
    const lastPart = parts[parts.length - 1];
    const out = [];
    for (let i = 1; i <= len; i += 1) {
      const next = Math.abs(lastPart.number + step * i);
      if (!Number.isSafeInteger(next)) return null;
      out.push(lastPart.before + String(next).padStart(lastPart.raw.length, "0") + lastPart.after);
    }
    return out;
  }

  function loopValues(cells, len, reverse) {
    const texts = cells.map((c) => String(c.value));
    const { series } = loopOf(texts[0]);
    const size = series.length;
    let step = reverse ? -1 : 1;
    if (texts.length > 1) {
      let cycle = 0;
      let nums = texts.map((t, i) => {
        if (t !== series[0]) return series.indexOf(t) + cycle * size;
        if (i === 0) return 0;
        cycle += 1;
        return cycle * size;
      });
      if (reverse) {
        texts.reverse();
        nums = nums.reverse();
      }
      if (!isEqualDiff(nums)) return null;
      step = nums[nums.length - 1] - nums[nums.length - 2];
    }
    const at = series.indexOf(texts[texts.length - 1]);
    const out = [];
    for (let i = 1; i <= len; i += 1) out.push(series[(((at + step * i) % size) + size) % size]);
    return out;
  }

  const SERIES = { date: numberValues, number: numberValues, extend: extendValues, loop: loopValues };

  // Series is off when nothing in the source can continue (formulas and plain text only).
  function seriesAllowed(lines) {
    return lines.some((runs) => runs.some((run) => run.type !== "other" && run.type !== "formula"));
  }

  // A fill of single numbers copies them; anything else that can continue does (the reference's
  // `_getPreferredApplyType`).
  function prefersCopy(lines) {
    return lines.every((runs) => runs.length === 1 && runs[0].type === "number" && runs[0].idx.length === 1);
  }

  // The fill of one line of the source: for each step away from the source, the index of the source
  // cell it repeats and the series value that replaces the cell's text (null: the cell as it is).
  function fillLine(cells, runs, count, reverse, series) {
    const size = cells.length;
    const runOf = new Array(size);
    runs.forEach((run, at) => run.idx.forEach((i) => (runOf[i] = at)));
    const steps = [];
    const taken = runs.map(() => 0);
    for (let k = 0; k < count; k += 1) {
      const src = reverse ? size - 1 - (k % size) : k % size;
      steps.push({ src, run: runOf[src], ordinal: taken[runOf[src]] });
      taken[runOf[src]] += 1;
    }
    const values = runs.map((run, at) => {
      const make = series ? SERIES[run.type] : null;
      return make && taken[at] > 0 ? make(run.idx.map((i) => cells[i]), taken[at], reverse) : null;
    });
    return steps.map((s) => ({ src: s.src, value: values[s.run] ? values[s.run][s.ordinal] : null }));
  }

  // The cells written by a fill of `source` ({ r1, c1, r2, c2 }, 1-based) by `count` lines in
  // `direction` ("down", "up", "right" or "left"). `mode` is "auto" (the default for the content),
  // "copy" or "toggle" (the other one). `cellAt(row, col)` answers `{ value, formula, style }` or
  // undefined; `shiftFormula(text, dRow, dCol)` moves the relative references of a formula.
  // Answers `{ edits, applyType }` or `{ refused: "limit" }`.
  function planFill(opts) {
    const { source: s, direction, count, cellAt, mode = "auto", shiftFormula, refOf } = opts;
    const isDateCode = (code) => host.__EASYOFFICE_SHEET_NUMFMT__?.patternKind(code) === "date";
    const vertical = direction === "down" || direction === "up";
    const reverse = direction === "up" || direction === "left";
    const lineCount = vertical ? s.c2 - s.c1 + 1 : s.r2 - s.r1 + 1;
    const size = vertical ? s.r2 - s.r1 + 1 : s.c2 - s.c1 + 1;
    if (count < 1) return { edits: [], applyType: "copy" };
    if (count > MAX_FILL_STEPS || count * lineCount > MAX_FILL_CELLS) return { refused: "limit" };
    const at = (line, i) => (vertical ? [s.r1 + i, s.c1 + line] : [s.r1 + line, s.c1 + i]);
    const lines = [];
    const sources = [];
    for (let line = 0; line < lineCount; line += 1) {
      const cells = [];
      for (let i = 0; i < size; i += 1) cells.push(cellAt(...at(line, i)));
      sources.push(cells);
      lines.push(groupRuns(cells.map((c) => classify(c, isDateCode))));
    }
    const allowed = seriesAllowed(lines);
    const preferred = allowed && !prefersCopy(lines) ? "series" : "copy";
    const wanted = mode === "copy" ? "copy" : mode === "toggle" ? (preferred === "copy" ? "series" : "copy") : preferred;
    const applyType = allowed ? wanted : "copy";
    const edits = [];
    for (let line = 0; line < lineCount; line += 1) {
      const steps = fillLine(sources[line], lines[line], count, reverse, applyType === "series");
      steps.forEach((step, k) => {
        const [fromRow, fromCol] = at(line, step.src);
        const away = k + 1;
        const row = vertical ? (reverse ? s.r1 - away : s.r2 + away) : fromRow;
        const col = vertical ? fromCol : reverse ? s.c1 - away : s.c2 + away;
        const from = sources[line][step.src];
        const edit = { ref: refOf(row, col), value: "", formula: "", style: plainStyle(from?.style) };
        // The copy and series fills bring the hyperlink along (PD-218).
        if (from?.link) edit.link = from.link;
        if (from?.formula) edit.formula = shiftFormula(from.formula, row - fromRow, col - fromCol);
        else edit.value = step.value ?? String(from?.value ?? "");
        edits.push(edit);
      });
    }
    return { edits, applyType };
  }

  // The end row of a double click on the handle: the data of the column left of the source (else
  // right of it) decides how far the fill goes, and the fill stops above data in the source columns.
  // `hasContent(row, col)`; answers `{ end }`, null when nothing can be filled, or `{ refused: "limit" }`.
  function detectFillEnd(source, hasContent, maxRow, maxCol) {
    const { r2, c1, c2 } = source;
    const first = r2 + 1;
    if (first > maxRow) return null;
    const side = [c1 - 1, c2 + 1].find((c) => c >= 1 && c <= maxCol && hasContent(first, c));
    if (side === undefined) return null;
    let end = first;
    while (end < maxRow && hasContent(end + 1, side)) {
      end += 1;
      if (end - r2 > MAX_FILL_STEPS) return { refused: "limit" };
    }
    for (let r = first; r <= end; r += 1) {
      for (let c = c1; c <= c2; c += 1) {
        if (hasContent(r, c)) return r > first ? { end: r - 1 } : null;
      }
    }
    return { end };
  }

  host.__EASYOFFICE_SHEET_FILL_CORE__ = {
    MAX_FILL_STEPS,
    MAX_FILL_CELLS,
    LOOP_SERIES,
    classify,
    planFill,
    detectFillEnd,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
