import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-314: BASE datasheet row height, column width resize + best-fit, auto column width recalculation timing,
// and scrollToItem / horizontal reveal padding for sticky chrome and oversized rows/columns.

const UI = "../crates/easyoffice-app/ui/";
const DEFAULT_ROW_PX = 24, HEAD_PX = 26, FOOT_PX = 24, CORNER_PX = 56;

let curRowHeightPx = () => DEFAULT_ROW_PX;
let curColWidthPx = () => 144;
let stickyHeaderPx = HEAD_PX, stickyFooterPx = FOOT_PX;

function patchHarness(document) {
  const proto = Object.getPrototypeOf(document.createElement("tr"));
  if (proto.__pd314Patched) return;
  const origRect = proto.getBoundingClientRect;
  proto.getBoundingClientRect = function () {
    if (this.tagName === "THEAD") return { left: 0, top: 0, width: 640, height: stickyHeaderPx };
    if (this.tagName === "TFOOT") return { left: 0, top: 0, width: 640, height: stickyFooterPx };
    if (this.tagName === "TH") {
      if (this.className.includes("base-grid-corner")) return { left: 0, top: 0, width: CORNER_PX, height: stickyHeaderPx };
      const sw = Number.parseFloat(this.style.width);
      const w = sw > 0 ? sw : curColWidthPx(this.dataset.fieldId, this.closest("table"));
      return { left: 0, top: 0, width: w, height: stickyHeaderPx };
    }
    if (this.className.includes("base-row-index")) return { left: 0, top: 0, width: CORNER_PX, height: curRowHeightPx(this) };
    if (this.className.includes("base-grid-row")) return { left: 0, top: 0, width: 640, height: curRowHeightPx(this) };
    if (this.className.includes("base-group-row")) return { left: 0, top: 0, width: 640, height: DEFAULT_ROW_PX };
    if (this.className.includes("base-grid-cell")) {
      const tbl = this.closest("table"), wrap = tbl?.closest(".base-grid-wrap");
      const ths = tbl ? Array.from(tbl.querySelectorAll("thead th")) : [];
      const colIdx = ths.findIndex((th) => th.dataset.fieldId === this.dataset.fieldId);
      const colEls = tbl?.firstChild?.tagName === "COLGROUP" ? tbl.firstChild.childNodes : [];
      const widthOfIdx = (k) => {
        const cw = Number.parseFloat(colEls[k]?.style?.width);
        if (cw > 0) return cw;
        return k === 0 ? CORNER_PX : curColWidthPx(ths[k]?.dataset?.fieldId, tbl);
      };
      let left = -(wrap?.scrollLeft || 0);
      for (let k = 0; k < colIdx; k += 1) left += widthOfIdx(k);
      const w = widthOfIdx(colIdx);
      return { left, top: 0, right: left + w, bottom: curRowHeightPx(this.parentElement), width: w, height: curRowHeightPx(this.parentElement) };
    }
    return origRect.call(this);
  };
  proto.__pd314Patched = true;
}

async function openBase() {
  const { document, host } = await createWorkspaceHarness();
  patchHarness(document);
  const observers = new Set();
  host.ResizeObserver = class {
    constructor(cb) { this.cb = cb; this.target = null; }
    observe(el) { this.target = el; observers.add(this); }
    disconnect() { observers.delete(this); }
  };
  host.__notifyResize = (el) => { for (const ob of observers) if (ob.target === el) ob.cb([]); };
  stickyHeaderPx = HEAD_PX; stickyFooterPx = FOOT_PX;
  curRowHeightPx = (tr) => {
    const attr = Number(tr?.closest?.("table")?.dataset?.rowHeight || 0);
    return attr > 0 ? attr : DEFAULT_ROW_PX;
  };
  curColWidthPx = () => 144;
  for (const name of ["sheet-nav.js", "base-model.js", "base-virtual.js", "base-grid-window.js", "base-cell-nav.js", "base-cell-select.js", "base-grid-size.js", "base-studio.js"]) {
    loadUiModule(new URL(UI + name, import.meta.url), host);
  }
  const root = document.createElement("section");
  root.id = "pane-base";
  root.className = "editor-pane base-pane";
  document.querySelector(".canvas-stage").appendChild(root);
  root.hidden = false;
  root.__notifyResize = host.__notifyResize;
  const log = { undo: 0, feedback: [] };
  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document, rootEl: root, invokeIpc: () => null, deliverExport: () => ({ ok: true }),
    setFeedback: (text) => log.feedback.push(text), recordUndo: () => { log.undo += 1; },
    captureUndo: () => ({}), commitUndo: () => { log.undo += 1; }, noteActivity: () => {},
  });
  studio.bind();
  return { document, host, root, studio, log };
}

function makeState(count, extra = {}) {
  const fields = [
    { id: "f1", name: "이름", type: "text" },
    { id: "f2", name: "수량", type: "number" },
    { id: "f3", name: "상태", type: "select", options: ["대기", "완료"] },
  ];
  const records = Array.from({ length: count }, (_, i) => ({
    id: "r" + i, values: { f1: extra.textAt?.(i) ?? `항목 ${i}`, f2: i + 1, f3: i % 2 === 0 ? "대기" : "완료" },
  }));
  return { version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records, ...extra.table }] };
}

const wrapOf = (root) => root.querySelector(".base-grid-wrap");
const tableOf = (root) => root.querySelector("table.base-grid-table");
const dataRows = (root) => root.querySelectorAll("tr.base-grid-row");
const colWidthsOf = (root) => Array.from(tableOf(root).querySelector("colgroup")?.childNodes || []).map((c) => Number.parseFloat(c.style.width));
const view = (root, h = 240, w = 640) => {
  const wrap = wrapOf(root);
  wrap.clientHeight = h;
  wrap.clientWidth = w;
  wrap.scrollTop ||= 0;
  wrap.scrollHeight ||= 0;
  root.__notifyResize?.(wrap);
  wrap.dispatchEvent({ type: "scroll", target: wrap });
  return wrap;
};

test("model roundtrip & Fail-Closed bounds for field width (64..1200) and table rowHeight (20..400)", async () => {
  const { host } = await openBase();
  const M = host.__EASYOFFICE_BASE_MODEL__;
  const st = M.createInitialBaseState();
  assert.equal(st.tables[0].rowHeight, undefined);
  assert.equal(st.tables[0].fields[0].width, undefined);

  assert.equal(M.setFieldWidth(st, "tbl-1", "f-none", 200).ok, false);
  assert.equal(M.setRowHeight({ tables: [] }, "t-none", 40).ok, false);
  assert.equal(M.setFieldWidth(st, "tbl-1", "fld-1", 220.4).width, 220);
  assert.equal(M.setRowHeight(st, "tbl-1", 36).rowHeight, 36);
  const serialized = M.serializeBaseState(st);
  assert.equal(serialized.tables[0].rowHeight, 36);
  assert.equal(serialized.tables[0].fields[0].width, 220);
  assert.equal(serialized.tables[0].fields[1].width, undefined);

  const hydrated = M.hydrateBaseState(JSON.stringify(serialized));
  assert.equal(hydrated.ok, true);
  assert.equal(hydrated.state.tables[0].rowHeight, 36);
  assert.equal(hydrated.state.tables[0].fields[0].width, 220);

  for (const badW of [NaN, Infinity, -10, 63, 1201, "200", {}]) {
    const candidate = M.serializeBaseState(st);
    candidate.tables[0].fields[0].width = badW;
    assert.equal(M.hydrateBaseState(candidate).ok, false, `reject width=${String(badW)}`);
  }
  for (const badH of [NaN, Infinity, 0, 19, 401, "40", {}]) {
    const candidate = M.serializeBaseState(st);
    candidate.tables[0].rowHeight = badH;
    assert.equal(M.hydrateBaseState(candidate).ok, false, `reject rowHeight=${String(badH)}`);
  }
});

test("column resizer drag clamps 64..1200px in 1 undo step on short and windowed tables (0 undo without move)", async () => {
  for (const count of [5, 150]) {
    const { document, root, studio, log } = await openBase();
    studio.setState(makeState(count));
    view(root);
    const undo0 = log.undo;
    const handle = root.querySelector('th[data-field-id="f1"] .col-resizer');
    handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, clientX: 200 });
    document.dispatchEvent({ type: "mouseup", target: document });
    assert.equal(log.undo, undo0, "click without move leaves no undo step");

    handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, clientX: 200 });
    document.dispatchEvent({ type: "mousemove", target: document, clientX: 280 });
    assert.equal(colWidthsOf(root)[1], 224);
    document.dispatchEvent({ type: "mousemove", target: document, clientX: 5000 });
    assert.equal(colWidthsOf(root)[1], 1200, "upper clamp 1200px");
    document.dispatchEvent({ type: "mousemove", target: document, clientX: -200 });
    assert.equal(colWidthsOf(root)[1], 64, "lower clamp 64px");
    document.dispatchEvent({ type: "mousemove", target: document, clientX: 260 });
    document.dispatchEvent({ type: "mouseup", target: document });
    assert.equal(log.undo, undo0 + 1, "one drag = one undo step");
    assert.equal(studio.getState().tables[0].fields[0].width, 204);
    assert.equal(colWidthsOf(root)[1], 204);
  }
});

test("double-clicking col-resizer best-fits offscreen long cell (r8500) in 1 undo step without clipping or stealing selection", async () => {
  const { root, studio, log } = await openBase();
  const longText = "오프스크린_긴_제목_" + "가".repeat(38);
  studio.setState(makeState(10000, { textAt: (i) => (i === 8500 ? longText : `r${i}`) }));
  view(root);
  root.querySelector('tr[data-record-id="r0"] td[data-field-id="f2"]').dispatchEvent({ type: "mousedown", target: root.querySelector('tr[data-record-id="r0"] td[data-field-id="f2"]'), button: 0 });
  const beforeSel = studio.selection();
  const undo0 = log.undo;
  const handle = root.querySelector('th[data-field-id="f1"] .col-resizer');
  handle.dispatchEvent({ type: "dblclick", target: handle });
  assert.equal(log.undo, undo0 + 1);
  const w = studio.getState().tables[0].fields[0].width;
  assert.ok(w > 500 && w <= 1200, `best-fit width includes r8500: ${w}`);
  assert.deepEqual(studio.selection(), beforeSel, "resizer dblclick preserves active selection");
  assert.match(log.feedback.at(-1) || "", /열 너비 자동 맞춤/);
});

test("auto column width cache recalculates on filter/sort/group/reset/footer growth, preserves user width, and never shifts on scroll", async () => {
  const { root, studio } = await openBase();
  const colW = { f1: 150, f2: 144, f3: 144 };
  curColWidthPx = (fid) => colW[fid] ?? 144;
  studio.setState(makeState(150));
  view(root);
  studio.setFieldWidth("f3", 210);
  assert.deepEqual(colWidthsOf(root), [56, 150, 144, 210]);

  const wBeforeScroll = colWidthsOf(root);
  wrapOf(root).scrollTop = 1200;
  wrapOf(root).dispatchEvent({ type: "scroll", target: wrapOf(root) });
  assert.deepEqual(colWidthsOf(root), wBeforeScroll, "scrolling never alters column widths");

  colW.f2 = 190;
  studio.updateCell("r10", "f2", 999999999);
  view(root);
  assert.equal(colWidthsOf(root)[2], 190, "footer sum growth widens auto column monotonically");
  assert.equal(colWidthsOf(root)[3], 210, "explicit user width stays 210");

  colW.f1 = 260;
  studio.setFilters([{ fieldId: "f3", op: "eq", value: "완료" }], "and");
  view(root);
  assert.equal(colWidthsOf(root)[1], 260, "filter change recalculates auto column f1");
  assert.equal(colWidthsOf(root)[3], 210, "filter change still preserves explicit width");

  colW.f1 = 160;
  studio.setState(makeState(150));
  view(root);
  assert.equal(colWidthsOf(root)[1], 160, "setState clears prior auto width memo");
});

test("row-resizer drag, standard double-click reset, visibleSpan, virtual spacers, and oversized scroll padding", async () => {
  const { document, root, studio, log } = await openBase();
  studio.setState(makeState(10000));
  view(root, 240, 320);

  const rowHandle = root.querySelector('tr[data-record-id="r0"] .row-resizer');
  const undo0 = log.undo;
  rowHandle.dispatchEvent({ type: "mousedown", target: rowHandle, button: 0, clientY: 100 });
  document.dispatchEvent({ type: "mousemove", target: document, clientY: 116 });
  assert.equal(tableOf(root).dataset.rowHeight, "40", "live preview updates row height");
  document.dispatchEvent({ type: "mouseup", target: document });
  assert.equal(log.undo, undo0 + 1);
  assert.equal(studio.getState().tables[0].rowHeight, 40);
  view(root, 240, 320);

  const [topSp, botSp] = root.querySelectorAll("tr.base-grid-spacer");
  const rows40 = dataRows(root);
  const firstIdx = Number(rows40[0].getAttribute("aria-rowindex")) - 2;
  const lastIdx = Number(rows40.at(-1).getAttribute("aria-rowindex")) - 2;
  assert.equal(topSp.hidden ? 0 : Number.parseFloat(topSp.firstChild.style.height), firstIdx * 40);
  assert.equal(Number.parseFloat(botSp.firstChild.style.height), (9999 - lastIdx) * 40);

  const td0 = root.querySelector('tr[data-record-id="r0"] td[data-field-id="f1"]');
  td0.dispatchEvent({ type: "mousedown", target: td0, button: 0 });
  tableOf(root).dispatchEvent({ type: "keydown", key: "PageDown", target: tableOf(root) });
  // (240 - 26 - 24) / 40 = floor(190 / 40) = 4 rows per PageDown.
  assert.equal(studio.selection().ar, 5, "PageDown uses updated 40px row height in visibleSpan");

  // Oversized 300px row in 240px viewport aligns to start (just below sticky header), not end.
  studio.setRowHeight(300);
  view(root, 240, 320);
  tableOf(root).dispatchEvent({ type: "keydown", key: "ArrowDown", target: tableOf(root) });
  assert.equal(studio.selection().ar, 6);
  const expectedTop = 5 * 300;
  assert.equal(wrapOf(root).scrollTop, expectedTop, "oversized row aligns start edge under sticky thead");

  // Oversized 500px column in 320px viewport (56px sticky row index -> 264px inner width) aligns left edge after row index.
  studio.setFieldWidth("f2", 500);
  view(root, 240, 320);
  wrapOf(root).scrollLeft = 0;
  tableOf(root).dispatchEvent({ type: "keydown", key: "ArrowRight", target: tableOf(root) });
  assert.equal(studio.selection().ac, 2);
  assert.equal(wrapOf(root).scrollLeft, 144, "oversized column reveals its start edge at corner right");

  // Short table (<= 120 rows) also measures sticky thead/tfoot heights so scrollToItem keeps rows clear of custom chrome padding.
  stickyHeaderPx = 30; stickyFooterPx = 28;
  studio.setState(makeState(15, { table: { rowHeight: 40 } }));
  view(root, 200, 320);
  root.querySelector('tr[data-record-id="r0"] td[data-field-id="f1"]').dispatchEvent({ type: "mousedown", target: root.querySelector('tr[data-record-id="r0"] td[data-field-id="f1"]'), button: 0 });
  for (let s = 0; s < 5; s += 1) tableOf(root).dispatchEvent({ type: "keydown", key: "ArrowDown", target: tableOf(root) });
  // Row 5 (0-based) sits at top = 30 + 5*40 = 230, bottom = 270; view inner bottom = 200 - 28 = 172 -> scrollTop = 270 + 28 - 200 = 98.
  assert.equal(wrapOf(root).scrollTop, 98, "short table scrollToItem uses measured thead(30)+tfoot(28) padding");
  stickyHeaderPx = HEAD_PX; stickyFooterPx = FOOT_PX;

  // Double-click row resizer restores standard 24px row height in 1 undo step.
  const undo1 = log.undo;
  const resetHandle = root.querySelector(".row-resizer");
  resetHandle.dispatchEvent({ type: "dblclick", target: resetHandle });
  assert.equal(log.undo, undo1 + 1);
  assert.equal(studio.getState().tables[0].rowHeight, undefined);
  assert.match(log.feedback.at(-1) || "", /표준 높이/);
});
