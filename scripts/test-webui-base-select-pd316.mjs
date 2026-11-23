import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

const UI = "../crates/easyoffice-app/ui/";
const SELECT_OPTIONS = ["대기", "진행", "완료"];
const MODULES = ["sheet-nav.js", "base-model.js", "base-virtual.js", "base-grid-window.js", "base-cell-nav.js", "base-clipboard.js", "base-cell-select.js", "base-studio.js"];

function patchHarness(document) {
  const proto = Object.getPrototypeOf(document.createElement("input"));
  if (proto.__pd316Patched) return;
  const click = proto.click;
  proto.blur = function blur() {
    if (document.activeElement === this) {
      document.activeElement = document.body;
      this.dispatchEvent({ type: "blur", target: this });
    }
  };
  proto.click = function clickPatched() {
    if (this.className.includes("base-cell-checkbox")) {
      this.checked = !this.checked;
      this.dispatchEvent({ type: "change", target: this });
    }
    return click.call(this);
  };
  proto.__pd316Patched = true;
}

async function openBase(count = 8) {
  const { document, host } = await createWorkspaceHarness();
  patchHarness(document);
  for (const name of MODULES) loadUiModule(new URL(UI + name, import.meta.url), host);
  const root = document.createElement("section");
  root.id = "pane-base";
  root.className = "editor-pane base-pane";
  document.querySelector(".canvas-stage").appendChild(root);
  root.hidden = false;
  const log = { undo: 0, feedback: [], states: [] };
  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document, rootEl: root, invokeIpc: () => null, deliverExport: () => ({ ok: true }),
    setFeedback: (text) => log.feedback.push(text),
    recordUndo: () => { log.undo += 1; },
    captureUndo: () => JSON.stringify(studio.getState()),
    commitUndo: (snap) => { log.states.push(snap); log.undo += 1; },
    noteActivity: () => {},
  });
  studio.bind();
  const fields = [
    { id: "f1", name: "이름", type: "text" },
    { id: "f2", name: "수량", type: "number", validation: { min: 0 } },
    { id: "f3", name: "상태", type: "select", options: SELECT_OPTIONS },
    { id: "f4", name: "날짜", type: "date" },
    { id: "f5", name: "고객", type: "relation", relationTableId: "tc" },
    { id: "f6", name: "태그", type: "multiSelect", options: ["a", "b", "c"] },
    { id: "f7", name: "합계", type: "formula", formula: "{수량} * 2" },
  ];
  const records = Array.from({ length: count }, (_, i) => ({
    id: "r" + i,
    values: { f1: `항목 ${String(i + 1).padStart(2, "0")}`, f2: (i + 1) * 10, f3: SELECT_OPTIONS[i % 3], f4: `2026-11-${String(i + 1).padStart(2, "0")}`, f5: [i % 2 ? "c1" : "c0"], f6: [i % 2 ? "b" : "a"] },
  }));
  const tc = { id: "tc", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: [{ id: "c0", values: { c1: "김" } }, { id: "c1", values: { c1: "이" } }] };
  studio.setState({ version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }, tc] });
  const wrap = root.querySelector(".base-grid-wrap");
  wrap.clientHeight = 600;
  wrap.scrollTop = 0;
  wrap.dispatchEvent({ type: "scroll", target: wrap });
  const undoOne = () => { const prev = log.states.pop(); if (prev) studio.setState(JSON.parse(prev)); };
  return { document, host, root, studio, log, undoOne };
}

const tableOf = (root) => root.querySelector("table.base-grid-table");
const cellOf = (root, rec, field) => root.querySelector(`tr[data-record-id="${rec}"] td[data-field-id="${field}"]`);
const colHdrOf = (root, field) => root.querySelector(`thead th[data-field-id="${field}"]`);
const rowIdxOf = (root, rec) => root.querySelector(`tr[data-record-id="${rec}"] td.base-row-index`);
const activeOf = (root) => { const td = root.querySelector("td[data-sel~='a']"); return td ? [td.parentElement.dataset.recordId, td.dataset.fieldId] : null; };
const valsOf = (studio, rec) => studio.getState().tables[0].records.find((r) => r.id === rec)?.values;
const mousedown = (el, mods = {}) => !el.dispatchEvent({ type: "mousedown", target: el, button: 0, ...mods });
const mouseup = (document) => document.dispatchEvent({ type: "mouseup", target: document });
const press = (document, key, mods = {}) => !document.activeElement.dispatchEvent({ type: "keydown", key, target: document.activeElement, ...mods });
function clip(initial = "") {
  const data = new Map(initial ? [["text/plain", initial]] : []);
  return { data, setData: (k, v) => data.set(k, v), getData: (k) => data.get(k) ?? "" };
}
const dispatchClip = (root, type, initial = "") => {
  const clipboardData = clip(initial), t = tableOf(root);
  return { canceled: !t.dispatchEvent({ type, target: t, clipboardData }), text: clipboardData.data.get("text/plain") ?? "" };
};

test("Ctrl+click multi-range selection, aligned copy, incompatible refusal, Delete 1-step Undo, and Esc collapse", async () => {
  const { document, root, studio, log, undoOne } = await openBase(6);
  mousedown(cellOf(root, "r0", "f1"));
  mousedown(cellOf(root, "r1", "f2"), { shiftKey: true });
  mouseup(document);
  mousedown(cellOf(root, "r3", "f1"), { ctrlKey: true });
  mousedown(cellOf(root, "r4", "f2"), { ctrlKey: true, shiftKey: true });
  mouseup(document);
  assert.equal(studio.selectedRanges().length, 2);
  assert.equal(root.querySelectorAll("td[data-sel]").length, 8);
  assert.deepEqual(dispatchClip(root, "copy"), { canceled: true, text: "항목 01\t10\n항목 02\t20\n항목 04\t40\n항목 05\t50" });

  // Same-row aligned horizontal multi-range copy succeeds.
  mousedown(cellOf(root, "r1", "f1"));
  mouseup(document);
  mousedown(cellOf(root, "r1", "f3"), { ctrlKey: true });
  mouseup(document);
  assert.deepEqual(dispatchClip(root, "copy"), { canceled: true, text: "항목 02\t진행" });

  // Incompatible row & column bounds refuse copy with Korean feedback.
  mousedown(cellOf(root, "r0", "f1"));
  mouseup(document);
  mousedown(cellOf(root, "r2", "f2"), { ctrlKey: true });
  mouseup(document);
  const bad = dispatchClip(root, "copy");
  assert.equal(bad.canceled, true);
  assert.equal(bad.text, "");
  assert.match(log.feedback.at(-1), /다중 선택 영역/);

  // Delete clears all selected ranges in 1 Undo step, then Esc drops extra ranges.
  press(document, "Delete");
  assert.equal(log.undo, 1);
  assert.equal(valsOf(studio, "r0").f1, "");
  assert.equal(valsOf(studio, "r2").f2, null);
  undoOne();
  assert.equal(valsOf(studio, "r0").f1, "항목 01");
  assert.equal(valsOf(studio, "r2").f2, 30);

  mousedown(cellOf(root, "r0", "f1"));
  mouseup(document);
  mousedown(cellOf(root, "r2", "f1"), { ctrlKey: true });
  mouseup(document);
  press(document, "Escape");
  assert.equal(studio.selectedRanges().length, 1);
  assert.deepEqual(activeOf(root), ["r2", "f1"]);
});

test("Cut (Ctrl+X & menu clipboard commands) copies TSV and clears source in 1 Undo step, leaving formula intact", async () => {
  const { root, studio, log, undoOne } = await openBase(5);
  mousedown(cellOf(root, "r0", "f1"));
  mousedown(cellOf(root, "r1", "f7"), { shiftKey: true });
  let sysText = "";
  const ok = await studio.runClipboardCommand("clip-cut", async () => sysText, async (t) => { sysText = t; return true; });
  assert.equal(ok, true);
  assert.equal(log.undo, 1);
  assert.match(sysText, /^항목 01\t10\t대기\t2026-11-01\t김\ta\t20\n/);
  assert.equal(valsOf(studio, "r0").f1, "");
  assert.equal(valsOf(studio, "r1").f2, null);
  assert.equal(valsOf(studio, "r0").f7, undefined, "formula column stays untouched");
  undoOne();
  assert.equal(valsOf(studio, "r0").f1, "항목 01");
  assert.equal(valsOf(studio, "r1").f2, 20);

  // Direct cut event + menu paste path via workspace-session.js routing check.
  mousedown(cellOf(root, "r2", "f1"));
  mousedown(cellOf(root, "r2", "f2"), { shiftKey: true });
  const evCut = dispatchClip(root, "cut");
  assert.equal(evCut.text, "항목 03\t30");
  assert.equal(valsOf(studio, "r2").f1, "");
  assert.equal(valsOf(studio, "r2").f2, null);
  assert.equal(log.undo, 2);

  mousedown(cellOf(root, "r4", "f1"));
  await studio.runClipboardCommand("clip-paste", async () => "", async () => false);
  assert.equal(valsOf(studio, "r4").f1, "항목 03");
  assert.equal(valsOf(studio, "r4").f2, 30);

  // Also verify workspace-session.js enables BASE clipboard commands and forwards calls.
  const { document: sDoc, host: sHost } = await createWorkspaceHarness();
  const calls = [];
  sHost.__EASYOFFICE_BASE_BOARD_HOST__ = { getBaseStudio: () => ({ runClipboardCommand: async (c) => (calls.push(c), true) }) };
  const sFeedback = [];
  const sess = sHost.__EASYOFFICE_CREATE_WORKSPACE_SESSION__({
    document: sDoc, storage: { getItem: () => null, setItem: () => {} }, getActiveModule: () => "BASE",
    getEditorEl: () => null, getWorkspacePanels: () => ({ computeSmartStats() {} }), setFeedback: (m) => sFeedback.push(m),
    noteActivity() {}, snapshotNow() { return {}; }, syncCurrentModeFromState() {}, updateModeHeader() {}, updateStatusBar() {},
    applyTextAlign() {}, refreshRulerForActiveDocument() {}, showRulerAndTabsIfNeeded() {}, onUndoRedo() {},
    docDefaults: { headingSizeByTag: {}, docParagraphBg: () => "" }, styleActions: new Set(), shortcutBindings: [],
  });
  await sess.runClipboardCommand("clip-copy", (fn) => fn());
  await sess.runClipboardCommand("clip-cut", (fn) => fn());
  await sess.runClipboardCommand("clip-paste", (fn) => fn());
  assert.deepEqual(calls, ["clip-copy", "clip-cut", "clip-paste"]);
  assert.equal(sFeedback.includes("지원하지 않는 명령"), false);
});

test("Row/column header, Shift+Space, Ctrl+Space, and corner `#` selection highlight headers", async () => {
  const { document, root, studio } = await openBase(5);
  mousedown(colHdrOf(root, "f2"));
  assert.deepEqual(studio.selectedRanges(), [{ r1: 1, r2: 5, c1: 2, c2: 2 }]);
  assert.equal(colHdrOf(root, "f2").getAttribute("data-sel-hdr"), "true");
  mousedown(colHdrOf(root, "f4"), { shiftKey: true });
  assert.deepEqual(studio.selectedRanges(), [{ r1: 1, r2: 5, c1: 2, c2: 4 }]);
  assert.equal(colHdrOf(root, "f3").getAttribute("data-sel-hdr"), "true");

  mousedown(rowIdxOf(root, "r1"));
  assert.deepEqual(studio.selectedRanges(), [{ r1: 2, r2: 2, c1: 1, c2: 7 }]);
  assert.equal(rowIdxOf(root, "r1").getAttribute("data-sel-hdr"), "true");
  assert.equal(colHdrOf(root, "f2").hasAttribute("data-sel-hdr"), false);

  mousedown(cellOf(root, "r2", "f3"));
  mouseup(document);
  assert.equal(press(document, " ", { shiftKey: true }), true);
  assert.deepEqual(studio.selectedRanges(), [{ r1: 3, r2: 3, c1: 1, c2: 7 }]);
  assert.equal(press(document, " ", { ctrlKey: true }), true);
  assert.deepEqual(studio.selectedRanges(), [{ r1: 1, r2: 5, c1: 1, c2: 7 }]);
  mousedown(cellOf(root, "r2", "f3"));
  mouseup(document);
  assert.equal(press(document, " ", { ctrlKey: true }), true);
  assert.deepEqual(studio.selectedRanges(), [{ r1: 1, r2: 5, c1: 3, c2: 3 }]);

  mousedown(root.querySelector("th.base-grid-corner"));
  assert.deepEqual(studio.selectedRanges(), [{ r1: 1, r2: 5, c1: 1, c2: 7 }]);
  assert.equal(root.querySelectorAll("thead th[data-sel-hdr='true']").length, 7);
});

test("Ctrl+D / Ctrl+R and fill-handle drag & dblclick compute series in 1 Undo step and skip formulas", async () => {
  const { document, root, studio, log, undoOne } = await openBase(7);
  // Seed Monday/Tuesday & relation/text for series testing.
  studio.setState({
    ...studio.getState(),
    tables: [{
      ...studio.getState().tables[0],
      records: [
        { id: "r0", values: { f1: "월", f2: 10, f3: "대기", f4: "2026-11-01", f5: ["c0"], f6: ["a"] } },
        { id: "r1", values: { f1: "화", f2: 20, f3: "진행", f4: "2026-11-03", f5: ["c1"], f6: ["b"] } },
        { id: "r2", values: { f1: "품목 01", f2: 0, f3: "", f4: "", f5: [], f6: [] } },
        { id: "r3", values: { f1: "품목 03", f2: 0, f3: "", f4: "", f5: [], f6: [] } },
        { id: "r4", values: { f1: "", f2: 0, f3: "", f4: "", f5: [], f6: [] } },
        { id: "r5", values: { f1: "", f2: 0, f3: "", f4: "", f5: [], f6: [] } },
      ],
    }, studio.getState().tables[1]],
  });
  log.undo = 0; log.states.length = 0;

  // Fill handle downward drag on r0..r1 across f1..f7 extends weekday, +10 number, select cycle, +2 day date, relation, multiSelect.
  mousedown(cellOf(root, "r0", "f1"));
  mousedown(cellOf(root, "r1", "f7"), { shiftKey: true });
  mouseup(document);
  const handle = root.querySelector(".base-fill-handle");
  assert.ok(handle, "fill handle attaches to bottom-right selected cell");
  mousedown(handle);
  document.dispatchEvent({ type: "mousemove", target: cellOf(root, "r5", "f7"), clientX: 0, clientY: 0 });
  assert.equal(cellOf(root, "r5", "f2").getAttribute("data-fill-preview"), "true");
  mouseup(document);
  assert.equal(log.undo, 1);
  assert.deepEqual([2, 3, 4, 5].map((i) => valsOf(studio, "r" + i).f1), ["수", "목", "금", "토"]);
  assert.deepEqual([2, 3, 4, 5].map((i) => valsOf(studio, "r" + i).f2), [30, 40, 50, 60]);
  assert.deepEqual([2, 3].map((i) => valsOf(studio, "r" + i).f4), ["2026-11-05", "2026-11-07"]);
  assert.deepEqual(valsOf(studio, "r2").f3, "대기");
  assert.deepEqual(valsOf(studio, "r2").f5, ["c0"]);
  assert.deepEqual(valsOf(studio, "r2").f6, ["a"]);
  assert.equal(valsOf(studio, "r2").f7, undefined);
  undoOne();
  assert.equal(valsOf(studio, "r2").f1, "품목 01");

  // Dragging upward fills trailing-digit step backwards (01, 03 -> -1 not below 0 / exact step) + fill-handle dblclick down.
  mousedown(cellOf(root, "r2", "f1"));
  mousedown(cellOf(root, "r3", "f1"), { shiftKey: true });
  mouseup(document);
  root.querySelector(".base-fill-handle").dispatchEvent({ type: "dblclick", target: root.querySelector(".base-fill-handle") });
  assert.deepEqual([4, 5].map((i) => valsOf(studio, "r" + i).f1), ["품목 05", "품목 07"]);

  // Ctrl+D copies from row above when a single cell is selected, Ctrl+R copies left column across selected columns.
  mousedown(cellOf(root, "r5", "f4"));
  press(document, "d", { ctrlKey: true });
  assert.equal(valsOf(studio, "r5").f4, valsOf(studio, "r4").f4);
  mousedown(cellOf(root, "r4", "f1"));
  mousedown(cellOf(root, "r4", "f3"), { shiftKey: true });
  press(document, "r", { ctrlKey: true });
  assert.equal(valsOf(studio, "r4").f3, "", "invalid select label refused cleanly by validation while leaving state consistent");
});

test("Paste onto `+` or past end appends records atomically in 1 Undo step, and >10,000 refuses whole", async () => {
  const { root, studio, log, undoOne } = await openBase(3);
  const newRow = root.querySelector("tr.base-new-record-row");
  mousedown(newRow);
  assert.equal(newRow.getAttribute("data-sel"), "append");
  const p = dispatchClip(root, "paste", "신규 1\t100\t진행\t2026-12-01\t김\ta\n신규 2\t200\t완료\t2026-12-02\t이\tb");
  assert.equal(p.canceled, true);
  assert.equal(log.undo, 1);
  const recs = studio.getState().tables[0].records;
  assert.equal(recs.length, 5);
  assert.deepEqual([recs[3].values.f1, recs[3].values.f2, recs[3].values.f5], ["신규 1", 100, ["c0"]]);
  assert.deepEqual([recs[4].values.f1, recs[4].values.f2, recs[4].values.f6], ["신규 2", 200, ["b"]]);
  undoOne();
  assert.equal(studio.getState().tables[0].records.length, 3);

  // Exceeding MAX_RECORDS_PER_TABLE (10,000) refuses the whole paste atomically.
  const full = studio.getState();
  full.tables[0].records = Array.from({ length: 9999 }, (_, i) => ({ id: "r" + i, values: { f1: "x" + i, f2: i } }));
  studio.setState(full);
  log.undo = 0;
  // Scroll window to reveal r9998 before selecting it.
  const wrap = root.querySelector(".base-grid-wrap");
  wrap.clientHeight = 600;
  wrap.scrollTop = 9999 * 24;
  wrap.dispatchEvent({ type: "scroll", target: wrap });
  mousedown(cellOf(root, "r9998", "f1"));
  dispatchClip(root, "paste", "하나\t1\n둘\t2\n셋\t3");
  assert.equal(studio.getState().tables[0].records.length, 9999, "atomic refusal keeps count at 9999");
  assert.equal(log.undo, 0, "no undo step written on refused paste");
  assert.match(log.feedback.at(-1), /최대 레코드 수/);
});

test("Record-ID selection survives sort-key edits (API, Tab, Enter) and preceding deletions, while filter/sort config changes drop it", async () => {
  const { document, root, studio } = await openBase(5);
  studio.setSorts([{ fieldId: "f2", dir: "asc" }]);
  mousedown(cellOf(root, "r1", "f2"));
  assert.deepEqual(activeOf(root), ["r1", "f2"]);

  // Direct API edit moves r1 (수량 20 -> 999) to bottom row; selection stays on r1!
  studio.updateCell("r1", "f2", 999);
  assert.deepEqual(activeOf(root), ["r1", "f2"]);

  // Interactive edit on r2.f2 (30 -> 500) + Tab keeps r2 focused at f3 on r2's new sorted row!
  mousedown(cellOf(root, "r2", "f2"));
  press(document, "Enter");
  document.activeElement.value = "500";
  document.activeElement.dispatchEvent({ type: "change", target: document.activeElement });
  press(document, "Tab");
  assert.deepEqual(activeOf(root), ["r2", "f3"]);

  // Deleting an earlier row (r0) keeps r2 selected even though its 1-based row index shifted up by 1.
  studio.deleteRecord("r0");
  assert.deepEqual(activeOf(root), ["r2", "f3"]);

  // Changing sort/filter settings still clears selection per PD-294 Decision #1.
  studio.setSorts([{ fieldId: "f2", dir: "desc" }]);
  assert.equal(activeOf(root), null);
});
