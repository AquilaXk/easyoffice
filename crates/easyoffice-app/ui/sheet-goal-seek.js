// 참조 구현 (reference implementation): goal-seek 0.1.4 (https://github.com/adam-hanna/goalSeek.js, MIT, Copyright (c) 2020 Adam Hanna),
// `lib/index.js` lines 6-69 (`goalSeek`: the residual `y = f(x) - goal`, a first step `x + y`, then a secant step `x - y / g` with
// `g = (y1 - y) / y`, each step capped by `maxStep`, until `|y|` is inside the tolerance). Changes: the function is asynchronous (the
// engine judges the sheet, one request per evaluation); the tolerance is an absolute change (0.001, the spreadsheet program's
// default `최대 변화`) and the iterations are bounded at 100 (its default `반복 횟수`) instead of a percentage; a result object
// replaces the thrown errors; a step that leaves the numbers (NaN, infinity) or a residual that stops moving fails closed; and
// the evaluation count is bounded (two requests per iteration). See THIRD_PARTY_NOTICES.md (PD-400).
(function (host) {
  "use strict";

  // 데이터 > 가상 분석 > 목표값 찾기 of the sheet window (PD-400): the 목표값 찾기 dialog (수식 셀, 찾는 값, 값을 바꿀 셀) and the 목표값 찾기
  // 상태 dialog that shows the answer and keeps it (확인) or drops it (취소).
  const MAX_ITERATIONS = 100;
  const TOLERANCE = 0.001;
  const MESSAGES = {
    formula: "수식 셀이 올바르지 않습니다",
    noFormula: "수식 셀에는 수식이 있어야 합니다",
    goal: "찾는 값이 올바르지 않습니다",
    changing: "값을 바꿀 셀이 올바르지 않습니다",
    hasFormula: "값을 바꿀 셀에는 수식이 없어야 합니다",
    same: "수식 셀과 값을 바꿀 셀이 같습니다",
    text: "값을 바꿀 셀의 값이 숫자가 아닙니다",
    engine: "계산하지 못했습니다",
    protectedCell: "보호된 칸은 바꿀 수 없습니다",
  };

  /**
   * Secant search of `x` with `evaluate(x) == goal`. `evaluate` answers a number, or `null` when it cannot (the engine failed or the
   * cell is not a number). `{ found, x, value, iterations }`: `value` is the last `f(x)`; `found` is false when the iterations ran
   * out or a step went off the numbers.
   */
  async function seek({ evaluate, start, goal, tolerance = TOLERANCE, maxIterations = MAX_ITERATIONS, maxStep = Infinity }) {
    let x = start;
    let value = null;
    const capped = (from, to) => (Math.abs(to - from) > maxStep ? from + Math.sign(to - from) * maxStep : to);
    for (let i = 0; i < maxIterations; i += 1) {
      value = await evaluate(x);
      if (value === null || !Number.isFinite(value)) return { found: false, x, value, iterations: i };
      const y = value - goal;
      if (Math.abs(y) <= tolerance) return { found: true, x, value, iterations: i };
      const old = x;
      // The first step is the residual itself; the second is the secant step from the two residuals.
      const probe = capped(old, old + y);
      const probed = await evaluate(probe);
      if (probed === null || !Number.isFinite(probed)) return { found: false, x: old, value, iterations: i };
      const y1 = probed - goal;
      if (Math.abs(y1) <= tolerance) return { found: true, x: probe, value: probed, iterations: i };
      let g = (y1 - y) / y;
      if (g === 0) g = 0.0001;
      const next = capped(old, old - y / g);
      if (!Number.isFinite(next)) return { found: false, x: old, value, iterations: i };
      x = next;
    }
    value = await evaluate(x);
    const found = value !== null && Number.isFinite(value) && Math.abs(value - goal) <= tolerance;
    return { found, x, value, iterations: maxIterations };
  }

  function createSheetGoalSeek(deps) {
    const { document, kit, say, sheetDataStore, getActiveSheetName, getRange, snapshotDomSheetCells, commitIfEditing, applyCellEdits, sheetStoreHelpers } = deps;
    const names = host.__EASYOFFICE_SHEET_NAMES__;
    const refsApi = host.__EASYOFFICE_SHEET_REFS__;
    const $ = (id) => document.getElementById(id);
    let bound = false;
    let pending = null;

    // One cell of the active sheet: `$B$1`, `B1` or `Sheet1!B1`.
    function cellOf(text) {
      const raw = String(text ?? "").trim();
      const bang = raw.lastIndexOf("!");
      if (bang >= 0 && names.fold(raw.slice(0, bang).replace(/^'|'$/g, "").replaceAll("''", "'")) !== names.fold(getActiveSheetName())) return null;
      const at = names.parseAddress(bang >= 0 ? raw.slice(bang + 1) : raw);
      return at && at.r1 === at.r2 && at.c1 === at.c2 ? { ref: refsApi.refOf(at.r1, at.c1), row: at.r1, col: at.c1 } : null;
    }

    // The value the engine gives the formula cell when the changing cell holds `x`: the sheet's cells are copied (only the one cell
    // differs) and nothing in the window changes.
    function evaluator(map, formulaRef, changingRef) {
      const sheet = getActiveSheetName();
      return async (x) => {
        const copy = { ...map, [changingRef]: { ...(map[changingRef] || {}), value: String(x), formula: "" } };
        const live = await sheetStoreHelpers.evaluateLiveFormulaTargets({ ...sheetDataStore, [sheet]: copy }, sheet);
        if (!live) return null;
        const at = live.formulaTargets.findIndex((target) => target.sheet === sheet && target.cell === formulaRef);
        if (at < 0) return null;
        const raw = sheetStoreHelpers.extractEvaluatedTargetValue(live.rpcRes, live.targetValues, live.batchValues, live.formulaTargets[at], at, live.formulaTargets.length);
        const number = Number(String(raw ?? "").trim());
        return raw === undefined || raw === null || String(raw).trim() === "" ? null : number;
      };
    }

    const fail = (key) => {
      kit.note("sheet-goalseek-status", MESSAGES[key]);
      return false;
    };

    function open() {
      commitIfEditing();
      bound ||= bind();
      const at = getRange();
      kit.setVal("sheet-goalseek-formula", refsApi.refOf(at.r1, at.c1));
      kit.setVal("sheet-goalseek-goal", "");
      kit.setVal("sheet-goalseek-changing", "");
      kit.note("sheet-goalseek-status", "");
      kit.openDialog("sheet-goalseek-dialog");
      return true;
    }

    function bind() {
      $("btn-sheet-goalseek-ok")?.addEventListener("click", () => void run());
      $("btn-sheet-goalseek-accept")?.addEventListener("click", () => void accept());
      $("sheet-goalseek-result-dialog")?.addEventListener("close", () => {
        pending = null;
      });
      return true;
    }

    async function run() {
      const formula = cellOf(kit.val("sheet-goalseek-formula"));
      if (!formula) return fail("formula");
      const changing = cellOf(kit.val("sheet-goalseek-changing"));
      if (!changing) return fail("changing");
      const goalText = kit.val("sheet-goalseek-goal").trim();
      const goal = Number(goalText.replaceAll(",", ""));
      if (goalText === "" || !Number.isFinite(goal)) return fail("goal");
      if (formula.ref === changing.ref) return fail("same");
      const map = snapshotDomSheetCells();
      if (!map[formula.ref]?.formula) return fail("noFormula");
      if (map[changing.ref]?.formula) return fail("hasFormula");
      const startText = String(map[changing.ref]?.value ?? "").trim();
      const start = startText === "" ? 0 : Number(startText.replaceAll(",", ""));
      if (!Number.isFinite(start)) return fail("text");
      kit.note("sheet-goalseek-status", "계산 중…");
      const answer = await seek({ evaluate: evaluator(map, formula.ref, changing.ref), start, goal });
      if (answer.value === null) return fail("engine");
      kit.closeDialog("sheet-goalseek-dialog");
      pending = { changing: changing.ref, x: answer.x, found: answer.found };
      $("sheet-goalseek-result-text").textContent = answer.found
        ? "셀 " + formula.ref + " 값 찾기에서 해답을 찾았습니다."
        : "셀 " + formula.ref + " 값 찾기에서 해답을 찾지 못했을 수 있습니다.";
      $("sheet-goalseek-result-goal").textContent = "목표값: " + goalText;
      $("sheet-goalseek-result-value").textContent = "현재값: " + String(answer.value);
      $("btn-sheet-goalseek-accept").disabled = false;
      kit.openDialog("sheet-goalseek-result-dialog");
      return true;
    }

    // 확인: the answer becomes the value of the changing cell (one undo step); 취소 (the dialog closes) leaves the sheet as it was.
    async function accept() {
      const answer = pending;
      kit.closeDialog("sheet-goalseek-result-dialog");
      if (!answer) return false;
      const done = await applyCellEdits([{ ref: answer.changing, value: String(answer.x), formula: "" }], "목표값 찾기");
      say(done ? "목표값 찾기: " + answer.changing + " = " + answer.x : MESSAGES.protectedCell);
      return done;
    }

    return { open, actions: { "goal-seek": () => Promise.resolve(open()) } };
  }

  host.__EASYOFFICE_SHEET_GOAL_SEEK__ = { MAX_ITERATIONS, TOLERANCE, MESSAGES, seek };
  host.__EASYOFFICE_CREATE_SHEET_GOAL_SEEK__ = createSheetGoalSeek;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
