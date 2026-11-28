import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// 데이터 > 고급 필터 and 데이터 > 가상 분석 > 목표값 찾기 (PD-400): the criteria plan and the secant search, then the dialogs on a sheet.

const ui = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const { __EASYOFFICE_SHEET_ADV_FILTER__: adv } = loadUiModule(ui("sheet-adv-filter.js"), {});
const { __EASYOFFICE_SHEET_GOAL_SEEK__: goal } = loadUiModule(ui("sheet-goal-seek.js"), {});

test("a criteria cell is a comparison, an equal text, a begins-with text or a number, and an empty one has no condition", () => {
  assert.equal(adv.criterionOf(""), null);
  assert.deepEqual(adv.criterionOf(">5"), { op: "greaterThan", text: "5" });
  assert.deepEqual(adv.criterionOf(">=5"), { op: "greaterThanOrEqual", text: "5" });
  assert.deepEqual(adv.criterionOf("<=5"), { op: "lessThanOrEqual", text: "5" });
  assert.deepEqual(adv.criterionOf("<5"), { op: "lessThan", text: "5" });
  assert.deepEqual(adv.criterionOf("<>가"), { op: "notEqual", text: "가" });
  assert.deepEqual(adv.criterionOf("<>"), { op: "notEqual", text: " " }, "non-blank");
  assert.deepEqual(adv.criterionOf("=가나"), { text: "가나" }, "= is the whole text");
  assert.deepEqual(adv.criterionOf("가"), { text: "가*" }, "a text begins with");
  assert.deepEqual(adv.criterionOf("12"), { text: "12" }, "a number is itself");
});

test("the criteria rows are alternatives, the cells of a row go together, and a heading twice gives a column two conditions", () => {
  const heads = ["이름", "나이", "부서"];
  const made = adv.plan(heads, [["부서", "나이", "나이"], ["영업", ">20", "<40"], ["", "", ""], ["개발", "", ""]], 1);
  assert.deepEqual(made.groups, [
    [{ col: 2, conditions: [{ op: "greaterThan", text: "20" }, { op: "lessThan", text: "40" }], and: true }, { col: 3, conditions: [{ text: "영업*" }] }],
    [],
    [{ col: 3, conditions: [{ text: "개발*" }] }],
  ]);
  assert.deepEqual(adv.plan(heads, [["부서"]], 1).groups, [[]], "headings only keep every row");
  assert.equal(adv.plan(heads, [["없는 머리글"], ["x"]], 1).error, "heading", "a calculated criterion has no heading in the list");
  assert.equal(adv.plan(heads, [["나이", "나이", "나이"], [">1", ">2", ">3"]], 1).error, "conditions");
  assert.equal(adv.plan(heads, [["나이"], ...Array.from({ length: 65 }, () => ["1"])], 1).error, "many");
  assert.equal(adv.plan(heads, [], 1).error, "criteria");
});

test("a row stays when any criteria row keeps it, and the rows outside are the runs the filter hides", () => {
  assert.deepEqual(adv.visibleRows(2, 9, [[[2, 4]], [[3, 6]]]), [2, 3, 4, 5, 6, 7, 8, 9].filter((r) => !(r >= 3 && r <= 4)), "row 2 stays by the second, 7 to 9 by both; 3 and 4 by none");
  assert.deepEqual(adv.visibleRows(2, 5, [[[2, 5]]]), [], "a group that hides everything");
  assert.deepEqual(adv.visibleRows(2, 5, [[]]), [2, 3, 4, 5]);
  assert.deepEqual(adv.runsOutside(2, 9, [3, 4, 8]), [[2, 2], [5, 7], [9, 9]]);
  assert.deepEqual(adv.runsOutside(2, 5, [2, 3, 4, 5]), []);
  assert.deepEqual(adv.uniqueRows([2, 3, 4], (row) => ({ 2: ["A", "1"], 3: ["a", "1"], 4: ["B", "1"] })[row]), [2, 4], "the texts are compared without case");
});

test("the secant search finds a root of a line and a curve inside the tolerance and the 100 iterations", async () => {
  const line = await goal.seek({ evaluate: async (x) => 3 * x + 2, start: 0, goal: 11 });
  assert.equal(line.found, true);
  assert.ok(Math.abs(line.x - 3) < 0.001, String(line.x));
  const curve = await goal.seek({ evaluate: async (x) => x * x, start: 1, goal: 49 });
  assert.equal(curve.found, true);
  assert.ok(Math.abs(Math.abs(curve.x) - 7) < 0.01, "a root of x squared: " + curve.x);
  assert.ok(Math.abs(curve.value - 49) <= goal.TOLERANCE);
  assert.equal(goal.MAX_ITERATIONS, 100);
  assert.equal(goal.TOLERANCE, 0.001);
});

test("a search with no answer, an engine that cannot answer, and a function that leaves the numbers all fail closed and stop", async () => {
  let calls = 0;
  const none = await goal.seek({ evaluate: async (x) => (calls += 1, x * x), start: 1, goal: -1 });
  assert.equal(none.found, false);
  assert.ok(calls <= 2 * goal.MAX_ITERATIONS + 1, "two evaluations per iteration at most: " + calls);
  assert.equal((await goal.seek({ evaluate: async () => null, start: 0, goal: 1 })).found, false);
  assert.equal((await goal.seek({ evaluate: async () => NaN, start: 0, goal: 1 })).found, false);
  const flat = await goal.seek({ evaluate: async () => 5, start: 0, goal: 9 });
  assert.equal(flat.found, false, "a function that does not move");
});

// ---- the dialogs on a sheet ---------------------------------------------------------------------------------

// The engine of the test: the auto filter's request judged by the custom conditions of the page (text begins with `x*`, comparisons),
// and the evaluation of one formula cell by the formula `=B1*3+2` of cell A1.
function judge(payload) {
  const body = JSON.parse(payload);
  const hidden = [];
  const [first] = body.filter.range.split(":");
  const firstRow = Number(first.replace(/\D/g, ""));
  const rows = body.data[0]?.values.length ?? 0;
  const matches = (condition, text) => {
    const number = Number(text);
    const operand = Number(condition.text);
    switch (condition.op) {
      case "greaterThan": return number > operand;
      case "greaterThanOrEqual": return number >= operand;
      case "lessThan": return number < operand;
      case "lessThanOrEqual": return number <= operand;
      case "notEqual": return condition.text.trim() === "" ? text !== "" : text !== condition.text;
      default: return condition.text.endsWith("*") ? text.startsWith(condition.text.slice(0, -1)) : text === condition.text;
    }
  };
  for (let i = 0; i < rows; i += 1) {
    const ok = body.filter.columns.every((column) => {
      const text = body.data.find((d) => d.col === column.col).values[i];
      const results = column.conditions.map((c) => matches(c, text));
      return column.and ? results.every(Boolean) : results.some(Boolean);
    });
    if (!ok) hidden.push([firstRow + 1 + i, firstRow + 1 + i]);
  }
  return { session: "s1", hidden, count: rows - hidden.length };
}

async function openSheet() {
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        if (body.sessionAction === "filter") return judge(args.payload);
        if (body.sessionAction === "structure") return { session: "s1", formulas: [], changed: [], circular: false, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {} };
        return { session: "s1", changed: [], circular: false };
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return harness;
}

async function type(h, ref, text) {
  const td = h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  assert.ok(td, "the cell " + ref + " is in the window");
  td.focus();
  for (const ch of text) {
    const active = h.document.activeElement;
    active.dispatchEvent({ type: "keydown", key: ch, target: active });
    active.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: active });
    active.textContent += ch;
    active.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: active });
  }
  td.dispatchEvent({ type: "keydown", key: "Tab", target: td });
  await flushAsyncTurns(15);
}

const get = (h, id) => h.document.getElementById(id);
const cellAt = (h, ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const act = async (h, name) => {
  h.userClick(h.document.querySelector(`.md-item[data-sheet-act="${name}"]`));
  await flushAsyncTurns(15);
};
async function fill(h, rows, top = 1) {
  for (let r = 0; r < rows.length; r += 1) {
    for (let c = 0; c < rows[r].length; c += 1) if (rows[r][c] !== "") await type(h, String.fromCharCode(65 + c) + (top + r), rows[r][c]);
  }
}
const select = async (h, text) => {
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="goto"]'));
  await flushAsyncTurns(6);
  get(h, "sheet-goto-ref").value = text;
  h.userClick(get(h, "btn-sheet-goto-ok"));
  await flushAsyncTurns(10);
};
// The demo workbook fills A1:H8: the test starts from an empty window (모두 지우기 of the whole window).
async function clearDemo(h) {
  await select(h, "A1:H12");
  await act(h, "clear-all");
  await select(h, "A1");
}
const sheetName = (h) => Object.keys(h.host.__EASYOFFICE_SHEET_LAYOUT__.getAll())[0];

test("고급 필터 in place keeps the rows of the criteria rows and clears with 필터 지우기; 고유 레코드만 hides the repeats", async () => {
  const h = await openSheet();
  await clearDemo(h);
  await fill(h, [["이름", "부서", "나이"], ["가", "영업", "30"], ["나", "개발", "25"], ["다", "영업", "50"], ["가", "영업", "30"]], 1);
  await fill(h, [["부서", "나이"], ["영업", ">40"], ["개발", ""]], 8);
  await select(h, "A2");
  await act(h, "adv-filter");
  assert.equal(get(h, "sheet-advfilter-dialog").open, true);
  assert.equal(get(h, "sheet-advfilter-list").value, "$A$1:$C$5", "the list is the region around the cell");
  assert.equal(get(h, "sheet-advfilter-target").disabled, true);
  get(h, "sheet-advfilter-criteria").value = "$A$8:$B$10";
  h.userClick(get(h, "btn-sheet-advfilter-ok"));
  await flushAsyncTurns(40);
  assert.equal(get(h, "sheet-advfilter-dialog").open, false, get(h, "sheet-advfilter-status").textContent);
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  assert.deepEqual(layout.get(sheetName(h)).filter.hidden, [[2, 2], [5, 5]], "row 2 (영업, 30) and row 5 (영업, 30) are out; 다 (영업, 50) and 나 (개발) stay");

  await act(h, "filter-clear");
  assert.deepEqual(layout.get(sheetName(h)).filter.hidden, [], "필터 지우기 shows the rows again");

  await act(h, "adv-filter");
  get(h, "sheet-advfilter-criteria").value = "$C$8:$C$9";
  get(h, "sheet-advfilter-unique").checked = true;
  h.userClick(get(h, "btn-sheet-advfilter-ok"));
  await flushAsyncTurns(40);
  assert.equal(get(h, "sheet-advfilter-status").textContent, "조건 범위의 머리글이 목록에 없습니다", "a heading of the criteria that the list lacks is refused");
  assert.equal(get(h, "sheet-advfilter-dialog").open, true);
  get(h, "sheet-advfilter-criteria").value = "$A$8:$A$9";
  h.userClick(get(h, "btn-sheet-advfilter-ok"));
  await flushAsyncTurns(40);
  assert.equal(get(h, "sheet-advfilter-dialog").open, false);
  assert.deepEqual(layout.get(sheetName(h)).filter.hidden, [[3, 3], [5, 5]], "다른 부서 (row 3) and the repeat of row 2 (row 5) go");
});

const { __EASYOFFICE_SHEET_CELL_STYLES__: cellStyles } = loadUiModule(ui("sheet-cell-styles.js"), {});

test("the cell style list is the program's default list: 47 styles in five groups with the standard's ids and colors", () => {
  assert.deepEqual(cellStyles.GROUPS.map((g) => [g.title, g.styles.length]), [["좋음, 나쁨 및 중립", 4], ["데이터 및 모델", 8], ["제목 및 표제", 6], ["주제 셀 스타일", 24], ["숫자 서식", 5]]);
  const ids = cellStyles.GROUPS.flatMap((g) => g.styles.map((s) => s.id)).sort((a, b) => a - b);
  assert.deepEqual(ids, [0, 3, 4, 5, 6, 7, 10, 11, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, ...Array.from({ length: 24 }, (_, i) => 29 + i), 53], "builtinId of ECMA-376 18.8.7");
  const good = cellStyles.keysOf(cellStyles.BY_NAME.get("좋음"));
  assert.deepEqual([good.fill, good.fontColor], ["c6efce", "006100"]);
  assert.equal(cellStyles.keysOf(cellStyles.BY_NAME.get("표준")), null, "표준 takes the format off");
  assert.equal(cellStyles.mix("4472C4", 0.8), "dae3f3", "20% - 강조색1 is the accent moved 80% toward white");
  assert.equal(cellStyles.keysOf(cellStyles.BY_NAME.get("제목 1")).fontSize, "20px", "15 pt");
  assert.equal(cellStyles.keysOf(cellStyles.BY_NAME.get("백분율")).numFmt, "0%");
});

test("셀 스타일 lists the styles, applies the chosen one to the selection in one undo step, and 표준 takes it off", async () => {
  const h = await openSheet();
  await clearDemo(h);
  await type(h, "A1", "x");
  await type(h, "B1", "y");
  await select(h, "A1:B1");
  await act(h, "cell-styles");
  assert.equal(get(h, "sheet-cellstyle-dialog").open, true);
  assert.equal(get(h, "sheet-cellstyle-list").querySelectorAll("[data-cell-style]").length, 47);
  h.userClick(get(h, "btn-sheet-cellstyle-ok"));
  await flushAsyncTurns(10);
  assert.equal(get(h, "sheet-cellstyle-status").textContent, "적용할 스타일을 고르세요");
  get(h, "sheet-cellstyle-list").querySelector('[data-cell-style="좋음"]').click();
  h.userClick(get(h, "btn-sheet-cellstyle-ok"));
  await flushAsyncTurns(30);
  assert.equal(get(h, "sheet-cellstyle-dialog").open, false);
  assert.deepEqual([cellAt(h, "A1")._st?.fill, cellAt(h, "B1")._st?.fontColor], ["c6efce", "006100"]);
  assert.equal(cellAt(h, "A1").textContent, "x", "the text stays");
  h.userClick(h.document.querySelector('[data-studio-menu-cmd="studio-undo"]'));
  await flushAsyncTurns(30);
  assert.equal(cellAt(h, "A1")._st?.fill, undefined, "one undo step");
  await act(h, "cell-styles");
  get(h, "sheet-cellstyle-list").querySelector('[data-cell-style="입력"]').click();
  h.userClick(get(h, "btn-sheet-cellstyle-ok"));
  await flushAsyncTurns(30);
  assert.equal(cellAt(h, "A1")._st?.border?.t, "thin:7f7f7f");
  await act(h, "cell-styles");
  get(h, "sheet-cellstyle-list").querySelector('[data-cell-style="표준"]').click();
  h.userClick(get(h, "btn-sheet-cellstyle-ok"));
  await flushAsyncTurns(30);
  assert.deepEqual([cellAt(h, "A1")._st?.fill, cellAt(h, "A1")._st?.border], [undefined, undefined].map((v) => v), "표준 clears the format");
});

// A sheet whose A1 is `=B1*3+2` for the engine: the answer is read from the value of B1 in the request.
async function openLinearSheet() {
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: false, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        const b1 = Number(body.cells?.find((c) => c.cell === "B1")?.value ?? 0);
        return { targetValues: (body.targets || []).map((t) => (t.cell === "A1" ? String(b1 * 3 + 2) : "")) };
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return harness;
}

test("목표값 찾기 asks for the three cells, shows the answer and sets the changing cell on 확인 in one undo step", async () => {
  const h = await openLinearSheet();
  await clearDemo(h);
  await type(h, "A1", "=B1*3+2");
  await type(h, "B1", "1");
  await select(h, "A1");
  await act(h, "goal-seek");
  assert.equal(get(h, "sheet-goalseek-dialog").open, true);
  assert.equal(get(h, "sheet-goalseek-formula").value, "A1", "the 수식 셀 is the active cell");
  const press = async () => {
    h.userClick(get(h, "btn-sheet-goalseek-ok"));
    await flushAsyncTurns(40);
  };
  get(h, "sheet-goalseek-goal").value = "abc";
  get(h, "sheet-goalseek-changing").value = "B1";
  await press();
  assert.equal(get(h, "sheet-goalseek-status").textContent, "찾는 값이 올바르지 않습니다");
  get(h, "sheet-goalseek-goal").value = "11";
  get(h, "sheet-goalseek-changing").value = "A1";
  await press();
  assert.equal(get(h, "sheet-goalseek-status").textContent, "수식 셀과 값을 바꿀 셀이 같습니다");
  get(h, "sheet-goalseek-formula").value = "C5";
  get(h, "sheet-goalseek-changing").value = "B1";
  await press();
  assert.equal(get(h, "sheet-goalseek-status").textContent, "수식 셀에는 수식이 있어야 합니다");
  get(h, "sheet-goalseek-formula").value = "A1";
  await press();
  assert.equal(get(h, "sheet-goalseek-dialog").open, false, get(h, "sheet-goalseek-status").textContent);
  assert.equal(get(h, "sheet-goalseek-result-dialog").open, true);
  assert.equal(get(h, "sheet-goalseek-result-text").textContent, "셀 A1 값 찾기에서 해답을 찾았습니다.");
  assert.equal(get(h, "sheet-goalseek-result-goal").textContent, "목표값: 11");
  assert.match(get(h, "sheet-goalseek-result-value").textContent, /^현재값: 1[01]\.?\d*$/);
  assert.equal(cellAt(h, "B1").textContent, "1", "nothing changes before 확인");
  h.userClick(get(h, "btn-sheet-goalseek-accept"));
  await flushAsyncTurns(40);
  assert.ok(Math.abs(Number(cellAt(h, "B1").textContent) - 3) < 0.001, cellAt(h, "B1").textContent);
  h.userClick(h.document.querySelector('[data-studio-menu-cmd="studio-undo"]'));
  await flushAsyncTurns(30);
  assert.equal(cellAt(h, "B1").textContent, "1", "one undo step");
});

test("목표값 찾기 does not change the sheet when the dialog is cancelled", async () => {
  const h = await openLinearSheet();
  await clearDemo(h);
  await type(h, "A1", "=B1*3+2");
  await type(h, "B1", "1");
  await select(h, "A1");
  await act(h, "goal-seek");
  get(h, "sheet-goalseek-goal").value = "11";
  get(h, "sheet-goalseek-changing").value = "B1";
  h.userClick(get(h, "btn-sheet-goalseek-ok"));
  await flushAsyncTurns(40);
  get(h, "sheet-goalseek-result-dialog").querySelector("[data-dialog-close]").click();
  await flushAsyncTurns(10);
  assert.equal(cellAt(h, "B1").textContent, "1");
});

test("고급 필터 copies the header and the rows to another place with the unique ones, and refuses a place on the list", async () => {
  const h = await openSheet();
  await clearDemo(h);
  await fill(h, [["이름", "나이"], ["가", "30"], ["나", "25"], ["다", "50"], ["가", "30"]], 1);
  await fill(h, [["나이"], [">26"]], 8);
  await select(h, "A2");
  await act(h, "adv-filter");
  get(h, "sheet-advfilter-copy").checked = true;
  get(h, "sheet-advfilter-copy").dispatchEvent({ type: "change", target: get(h, "sheet-advfilter-copy") });
  assert.equal(get(h, "sheet-advfilter-target").disabled, false);
  get(h, "sheet-advfilter-criteria").value = "$A$8:$A$9";
  get(h, "sheet-advfilter-unique").checked = true;
  get(h, "sheet-advfilter-target").value = "$B$3";
  h.userClick(get(h, "btn-sheet-advfilter-ok"));
  await flushAsyncTurns(40);
  assert.equal(get(h, "sheet-advfilter-status").textContent, "복사 위치가 목록 범위와 겹칩니다");
  get(h, "sheet-advfilter-target").value = "$E$1";
  h.userClick(get(h, "btn-sheet-advfilter-ok"));
  await flushAsyncTurns(40);
  assert.equal(get(h, "sheet-advfilter-dialog").open, false);
  const text = (ref) => cellAt(h, ref).textContent;
  assert.deepEqual([text("E1"), text("F1"), text("E2"), text("F2"), text("E3"), text("F3"), text("E4")], ["이름", "나이", "가", "30", "다", "50", ""], "the header and the rows over 26, the repeat of 가 once");
  assert.equal(text("A2"), "가", "the list is untouched");
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.get(sheetName(h))?.filter?.hidden?.length ?? 0, 0, "nothing is hidden when the rows are copied");
});
