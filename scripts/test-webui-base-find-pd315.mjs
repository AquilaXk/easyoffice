import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// PD-315: BASE datasheet in-app find & replace (Ctrl+F / Ctrl+F2), virtual window reveal,
// grouped/sorted/filtered visual order, formula skip, validation guard, and 1-step Undo.

const $ = (document, id) => document.getElementById(id);

const ctrl = (h, key) => h.sandbox.dispatchWindowKeydown({ key, ctrlKey: true });

const typeInput = (h, id, value) => {
  const input = $(h.document, id);
  input.value = value;
  input.dispatchEvent({ type: "input", target: input });
};

const pressDialogKey = (h, target, key, extra = {}) => {
  const ev = {
    type: "keydown", key, target, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, isComposing: false,
    stopPropagation() {}, preventDefault() {}, ...extra,
  };
  h.document.dispatchEvent(ev);
};

const countText = (h) => $(h.document, "find-count").textContent || $(h.document, "find-status").textContent;

async function openBase(customState) {
  const h = await createWorkspaceHarness();
  switchModule(h.document, "BASE");
  await flushAsyncTurns(10);
  const studio = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
  if (customState) {
    const res = studio.setState(customState);
    assert.equal(res.ok, true, res.error);
    await flushAsyncTurns(4);
  }
  return { h, studio };
}

function make10kState() {
  const fields = [
    { id: "f1", name: "이름", type: "text" },
    { id: "f2", name: "수량", type: "number" },
    { id: "f3", name: "두배", type: "formula", formula: "{수량} * 2" },
    { id: "f4", name: "상태", type: "select", options: ["대기", "진행", "완료"] },
    { id: "f5", name: "완료", type: "checkbox" },
    { id: "f6", name: "고객", type: "relation", relationTableId: "tc" },
    { id: "f7", name: "태그", type: "multiSelect", options: ["긴급", "일반", "보류"] },
    { id: "f8", name: "날짜", type: "date" },
  ];
  const offscreen = new Set([1200, 8500, 9995]);
  const records = Array.from({ length: 10000 }, (_, i) => ({
    id: "r" + i,
    values: {
      f1: offscreen.has(i) ? `오프스크린_표적_${i}` : `일반_행_${i}`,
      f2: i + 1,
      f4: i < 2 ? "진행" : "대기",
      f5: false,
      f6: i === 8500 ? ["c2"] : ["c1"],
      f7: i === 9995 ? ["긴급", "보류"] : ["일반"],
      f8: "2026-11-23",
    },
  }));
  const customers = {
    id: "tc",
    name: "고객",
    fields: [{ id: "c1f", name: "이름", type: "text" }],
    records: [
      { id: "c1", values: { c1f: "고객가" } },
      { id: "c2", values: { c1f: "고객나_특별" } },
    ],
  };
  return { version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }, customers] };
}

test("BASE enables find/replace menu items and opens find dialog instead of command palette (others keep palette)", async () => {
  const { h } = await openBase(make10kState());
  assert.notEqual(studioItem(h.document, "find")?.getAttribute("aria-disabled"), "true");
  assert.notEqual(studioItem(h.document, "replace")?.getAttribute("aria-disabled"), "true");
  assert.equal(ctrl(h, "f").defaultPrevented, true);
  assert.equal($(h.document, "find-dialog").open, true);
  assert.equal($(h.document, "cmd-palette-modal").open, false);
  assert.equal($(h.document, "find-sheet-options").hidden, true, "sheet-only checkboxes stay hidden in BASE");
  assert.equal($(h.document, "find-replace-row").hidden, true);
  ctrl(h, "h");
  assert.equal($(h.document, "find-replace-row").hidden, false);
  assert.equal($(h.document, "find-replace-buttons").hidden, false);

  pressDialogKey(h, $(h.document, "find-query"), "Escape");
  assert.equal($(h.document, "find-dialog").open, false);

  for (const mod of ["SLIDE", "BOARD", "PDF"]) {
    switchModule(h.document, mod);
    await flushAsyncTurns(4);
    ctrl(h, "f");
    assert.equal($(h.document, "find-dialog").open, true, `${mod} opens find dialog on Ctrl+F`);
    assert.equal($(h.document, "find-replace-row").hidden, true, `${mod} keeps replace hidden`);
    pressDialogKey(h, $(h.document, "find-query"), "Escape");
    assert.equal($(h.document, "find-dialog").open, false);
  }
  switchModule(h.document, "CONV");
  await flushAsyncTurns(4);
  ctrl(h, "f");
  assert.equal($(h.document, "find-dialog").open, false, "CONV still uses command palette on Ctrl+F");
  assert.equal($(h.document, "cmd-palette-modal").open, true, "CONV opens palette");
  $(h.document, "cmd-palette-modal").close();
});

test("BASE finds offscreen matches in 10,000 virtualized rows, cycles Enter/Shift+Enter, and scrolls/selects cell without stealing focus", async () => {
  const { h, studio } = await openBase(make10kState());
  const queryEl = $(h.document, "find-query");
  ctrl(h, "f");
  typeInput(h, "find-query", "오프스크린_표적");
  assert.equal(countText(h), "1 / 3");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r1200", fieldId: "f1" });
  assert.ok(h.document.querySelector('tr[data-record-id="r1200"] td[data-field-id="f1"][data-sel~="a"]'), "r1200 rendered and active in DOM");
  assert.equal(h.document.activeElement, queryEl, "query input keeps focus across offscreen jump");

  pressDialogKey(h, queryEl, "Enter");
  assert.equal(countText(h), "2 / 3");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r8500", fieldId: "f1" });
  assert.ok(h.document.querySelector('tr[data-record-id="r8500"] td[data-field-id="f1"][data-sel~="a"]'), "r8500 rendered and active in DOM");
  assert.equal(h.document.activeElement, queryEl);

  pressDialogKey(h, queryEl, "Enter");
  assert.equal(countText(h), "3 / 3");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r9995", fieldId: "f1" });

  pressDialogKey(h, queryEl, "Enter");
  assert.equal(countText(h), "1 / 3", "wraps forward to first match");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r1200", fieldId: "f1" });

  pressDialogKey(h, queryEl, "Enter", { shiftKey: true });
  assert.equal(countText(h), "3 / 3", "Shift+Enter wraps backward to last match");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r9995", fieldId: "f1" });
});

test("BASE find follows sort order, skips collapsed groups and checkbox TRUE/FALSE noise, and indexes formula/relation/multiSelect display", async () => {
  const baseState = make10kState();
  // Shrink to 4 rows for focused sort/group/filter/relation/multiSelect inspection plus editing check.
  baseState.tables[0].records = [
    { id: "r0", values: { f1: "알파_표적", f2: 10, f4: "진행", f5: false, f6: ["c2"], f7: ["긴급"], f8: "2026-11-01" } },
    { id: "r1", values: { f1: "베타_일반", f2: 20, f4: "대기", f5: true, f6: ["c1"], f7: ["일반"], f8: "2026-11-02" } },
    { id: "r2", values: { f1: "감마_표적", f2: 30, f4: "대기", f5: false, f6: ["c1"], f7: ["보류"], f8: "2026-11-03" } },
    { id: "r3", values: { f1: "델타_표적", f2: 40, f4: "진행", f5: true, f6: ["c2"], f7: ["긴급", "보류"], f8: "2026-11-04" } },
  ];
  const { h, studio } = await openBase(baseState);
  studio.setSorts([{ fieldId: "f2", dir: "desc" }]);
  await flushAsyncTurns(4);

  ctrl(h, "f");
  typeInput(h, "find-query", "FALSE");
  assert.equal(countText(h), "일치하는 항목이 없습니다", "checkbox booleans do not produce false TRUE/FALSE matches");

  typeInput(h, "find-query", "고객나_특별");
  assert.equal(countText(h), "1 / 2", "relation target display labels are searchable");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r3", fieldId: "f6" }, "desc sort finds r3 before r0");

  typeInput(h, "find-query", "표적");
  assert.equal(countText(h), "2 / 3", "starts from current origin (r3, f6) and hits next match r2");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r2", fieldId: "f1" });
  $(h.document, "btn-find-prev").click();
  assert.equal(countText(h), "1 / 3");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r3", fieldId: "f1" });

  // Group by 상태 and collapse "대기" (holds r2), leaving only "진행" (r3, r0) expanded.
  studio.setGroupBy("f4");
  const st = studio.getState();
  st.tables[0].collapsedGroups = ["대기"];
  studio.setState(st);
  await flushAsyncTurns(6);
  assert.equal(countText(h), "1 / 2", "collapsed group row r2 is excluded and count auto-refreshes via watch");

  // Uncollapse and check formula display search ("80" from 40*2 on r3) & view switch from kanban -> grid.
  studio.setGroupBy("");
  studio.setView("kanban");
  await flushAsyncTurns(4);
  typeInput(h, "find-query", "80");
  assert.equal(countText(h), "1 / 1", "formula result display is searchable");
  assert.equal(studio.getState().tables[0].view, "grid", "showing match switches subview back to datasheet grid");
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r3", fieldId: "f3" });
});

test("BASE replace & replace-all commit in 1 Ctrl+Z undo step, skip formula cells, reject invalid values Fail-Closed, and support DOC<->BASE switch", async () => {
  const stData = make10kState();
  stData.tables[0].records = stData.tables[0].records.slice(0, 4);
  stData.tables[0].records[1].values.f1 = "바꿀값_하나";
  stData.tables[0].records[3].values.f1 = "바꿀값_둘";
  const { h, studio } = await openBase(stData);

  ctrl(h, "h");
  typeInput(h, "find-query", "바꿀값");
  typeInput(h, "find-replacement", "고친값");
  assert.equal(countText(h), "1 / 2");

  // Multi-replace in 1 undo step.
  $(h.document, "btn-find-replace-all").click();
  await flushAsyncTurns(6);
  assert.match(countText(h), /^2개를 바꿨습니다/);
  const recs = () => studio.getState().tables[0].records;
  assert.equal(recs()[1].values.f1, "고친값_하나");
  assert.equal(recs()[3].values.f1, "고친값_둘");

  // Single Ctrl+Z outside dialog restores both cells at once.
  typeInput(h, "find-query", "");
  pressDialogKey(h, $(h.document, "find-query"), "Escape");
  ctrl(h, "z");
  await flushAsyncTurns(6);
  assert.equal(recs()[1].values.f1, "바꿀값_하나");
  assert.equal(recs()[3].values.f1, "바꿀값_둘");

  // Relation label replacement -> stores relation target id.
  studio.selectCellById("r0", "f1");
  ctrl(h, "h");
  typeInput(h, "find-query", "고객가");
  typeInput(h, "find-replacement", "고객나_특별");
  assert.equal(countText(h), "1 / 4");
  $(h.document, "btn-find-replace").click();
  await flushAsyncTurns(6);
  assert.deepEqual(recs()[0].values.f6, ["c2"]);

  // Formula field replacement: skipped without mutating state.
  typeInput(h, "find-query", "4");
  typeInput(h, "find-replacement", "999");
  // Navigate to formula match on r1 (f3 shows "4" while f2 shows "2", f8 has no "4").
  const formulaMatch = () => studio.activeCellTarget()?.fieldId === "f3";
  for (let i = 0; i < 4 && !formulaMatch(); i += 1) $(h.document, "btn-find-next").click();
  assert.equal(formulaMatch(), true);
  const rec1Id = studio.activeCellTarget().recordId;
  $(h.document, "btn-find-replace").click();
  await flushAsyncTurns(6);
  assert.match($(h.document, "find-status").textContent, /수식 셀/);
  assert.equal(recs().find((r) => r.id === rec1Id).values.f3, undefined);

  // Validation Fail-Closed: replacing a number field value with non-numeric text must reject without dirtying Undo.
  typeInput(h, "find-query", "3");
  typeInput(h, "find-replacement", "숫자아님");
  // Navigate to r2.f2 (3) with a bounded loop.
  for (let i = 0; i < 8 && (studio.activeCellTarget()?.fieldId !== "f2" || studio.activeCellTarget()?.recordId !== "r2"); i += 1) {
    $(h.document, "btn-find-next").click();
  }
  assert.deepEqual(studio.activeCellTarget(), { recordId: "r2", fieldId: "f2" });
  const beforeNum = recs()[2].values.f2;
  $(h.document, "btn-find-replace").click();
  await flushAsyncTurns(6);
  assert.match($(h.document, "find-status").textContent, /올바른 숫자/);
  assert.equal(recs()[2].values.f2, beforeNum, "invalid number replacement refused");

  // Switching BASE <-> DOC while find dialog remains open swaps find source cleanly.
  switchModule(h.document, "DOC");
  await flushAsyncTurns(4);
  $(h.document, "doc-editor").innerHTML = "<p>문서전용검색어</p>";
  ctrl(h, "f");
  typeInput(h, "find-query", "문서전용검색어");
  assert.equal(countText(h), "1 / 1", "open find dialog switches source to DOC cleanly");
  switchModule(h.document, "BASE");
  await flushAsyncTurns(4);
  ctrl(h, "f");
  typeInput(h, "find-query", "고객나_특별");
  assert.equal(countText(h), "1 / 1", "open find dialog switches source back to BASE cleanly");
});
