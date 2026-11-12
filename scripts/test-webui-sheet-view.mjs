import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The sheet's view (PD-208): frozen rows and columns in the grid window, the groups with their bars
// and buttons, the tab colour, each as one undo step, and the view kept when rows move.

const STRUCTURE = { session: "s1", formulas: [], changed: [], circular: false, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {} };

async function open() {
  const requests = [];
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        requests.push(body);
        if (body.sessionAction === "structure") return { ...STRUCTURE, ...(body.outline ? { outline: body.outline } : {}) };
        return { open: { session: "s1" }, apply: { session: "s1", changed: [], circular: false }, close: { closed: true } }[body.sessionAction] ?? {};
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const { document, userClick } = harness;
  const viewport = document.getElementById("sheet-viewport");
  viewport.clientHeight = 520;
  viewport.clientWidth = 960;
  viewport.scrollTop = 0;
  viewport.scrollLeft = 0;
  const act = async (name) => {
    userClick(document.querySelector(`.md-item[data-sheet-act="${name}"]`));
    await flushAsyncTurns(20);
  };
  const history = async (name) => {
    userClick(studioItem(document, name));
    await flushAsyncTurns(20);
  };
  const selectRows = (first, last) => {
    for (const [row, shiftKey] of [[first, false], [last, true]]) {
      const th = document.querySelector(`#pane-sheet th[data-row="${row}"]`);
      th.dispatchEvent({ type: "mousedown", target: th, shiftKey, button: 0 });
    }
  };
  const layout = () => harness.host.__EASYOFFICE_SHEET_LAYOUT__;
  return { ...harness, viewport, act, history, selectRows, layout, requests };
}

const rowsInDom = (document) => [...document.querySelectorAll("#pane-sheet tbody tr[data-row]")].map((tr) => Number(tr.dataset.row));

test("frozen rows and columns stay in the window and pin with sticky offsets", async () => {
  const { document, viewport, act, layout } = await open();
  await act("freeze-first-row");
  assert.deepEqual(layout().freezeOf("Sheet1"), { rows: 1, cols: 0 });
  const row1 = document.querySelector('#pane-sheet tr[data-row="1"]');
  assert.equal(row1.children[1].classList.contains("frozen-row"), true);
  assert.equal(row1.children[1].style.top, "26px");
  assert.equal(row1.children[1].classList.contains("frozen-edge-row"), true);

  await act("freeze-first-col");
  assert.deepEqual(layout().freezeOf("Sheet1"), { rows: 0, cols: 1 }, "a first-column freeze replaces the first-row freeze");
  const a2 = document.querySelector('#pane-sheet td[data-cell="A2"]');
  assert.equal(a2.classList.contains("frozen-col"), true);
  assert.equal(a2.style.left, "56px");
  assert.equal(document.querySelector('#pane-sheet th[data-col="A"]').classList.contains("frozen-edge-col"), true);
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B2"]').classList.contains("frozen-col"), false);
  assert.equal(row1.children[1].classList.contains("frozen-row"), false, "the old pins are cleared");

  // Scrolled far down, row 1 and column A are still drawn, in order and before the top spacer.
  userCall(layout(), "freeze", { rows: 2, cols: 1 });
  viewport.scrollTop = 24 * 3000;
  viewport.dispatchEvent({ type: "scroll" });
  await flushAsyncTurns();
  const rows = rowsInDom(document);
  assert.deepEqual(rows.slice(0, 2), [1, 2]);
  assert.ok(rows[2] > 2900, `the window follows the scroll: ${rows[2]}`);
  assert.deepEqual(rows, [...rows].sort((a, b) => a - b), "rows stay in order");
  const tbody = document.querySelector("#pane-sheet tbody");
  assert.match(tbody.children[2].className, /sheet-spacer-top/);
  const far = document.querySelector(`#pane-sheet tr[data-row="${rows[3]}"]`);
  assert.equal(far.children[1].dataset.cell.startsWith("A"), true, "column A is kept in every row");
  assert.equal(far.children[1].classList.contains("frozen-col"), true);

  await act("freeze-clear");
  assert.equal(layout().freezeOf("Sheet1"), null);
  assert.equal(document.querySelector('#pane-sheet th[data-col="A"]').classList.contains("frozen-col"), false);
});

function userCall(layout, what, value) {
  if (what === "freeze") layout.setFreeze("Sheet1", value);
}

test("freezing at the selection pins the rows above and the columns left of it, and over the bounds is refused", async () => {
  const { document, act, layout, history } = await open();
  document.querySelector('#pane-sheet td[data-cell="C4"]').focus();
  userClickStudio(document, "sheet-mod-freeze");
  await flushAsyncTurns(20);
  assert.deepEqual(layout().freezeOf("Sheet1"), { rows: 3, cols: 2 });
  await history("studio-undo");
  assert.equal(layout().freezeOf("Sheet1"), null);
  // Cell A1 has nothing above or left of it.
  document.querySelector('#pane-sheet td[data-cell="A1"]').focus();
  userClickStudio(document, "sheet-mod-freeze");
  await flushAsyncTurns(20);
  assert.equal(layout().freezeOf("Sheet1"), null);
  assert.match(document.getElementById("status-command-feedback").textContent, /고정할 행·열 없음/);
  await act("freeze-clear");
});

function userClickStudio(document, name) {
  studioItem(document, name).click();
}

test("grouping rows draws a bar and a button, folding hides them, a level button folds by depth, undo restores", async () => {
  const { document, act, history, selectRows, layout, userClick } = await open();
  selectRows(3, 5);
  await act("group");
  assert.deepEqual(layout().outlineOf("Sheet1").rows, [[3, 5, 1]]);
  const side = document.getElementById("sheet-outline-rows");
  assert.equal(side.hidden, false);
  assert.equal(document.getElementById("sheet-frame").style.getPropertyValue("--outline-w"), "28px");
  const toggle = () => side.querySelector(".outline-toggle");
  assert.equal(toggle().dataset.first, "3");
  assert.equal(toggle().getAttribute("aria-label"), "행 3~5 하위 수준 숨기기");
  assert.equal(toggle().getAttribute("aria-expanded"), "true");
  // The button sits on the summary row 6: 26px of the header and five rows of 24px, centred.
  assert.equal(toggle().style.top, 6 * 24 - 24 + 12 - 5.5 + "px");
  assert.equal(side.querySelectorAll(".outline-bar").length, 1);

  userClick(toggle());
  await flushAsyncTurns(20);
  assert.deepEqual(layout().outlineOf("Sheet1").foldedRows, [[3, 5]]);
  assert.equal(layout().get("Sheet1").hiddenRows.length, 0, "folded rows are not the user's hidden rows");
  assert.deepEqual(rowsInDom(document).filter((r) => r >= 2 && r <= 7), [2, 6, 7]);
  assert.equal(toggle().getAttribute("aria-label"), "행 3~5 하위 수준 표시");
  assert.equal(side.querySelectorAll(".outline-bar").length, 0);

  // Level button 2 shows every group again, button 1 hides them all.
  userClick(side.querySelector('.outline-level[data-level="2"]'));
  await flushAsyncTurns(20);
  assert.equal(layout().outlineOf("Sheet1").foldedRows, undefined);
  userClick(side.querySelector('.outline-level[data-level="1"]'));
  await flushAsyncTurns(20);
  assert.deepEqual(layout().outlineOf("Sheet1").foldedRows, [[3, 5]]);

  await history("studio-undo");
  await history("studio-undo");
  await history("studio-undo");
  assert.deepEqual(layout().outlineOf("Sheet1").foldedRows, undefined);
  assert.deepEqual(layout().outlineOf("Sheet1").rows, [[3, 5, 1]]);
  await history("studio-undo");
  assert.equal(layout().outlineOf("Sheet1"), null);
  assert.equal(side.hidden, true);
});

test("groups of columns come from selected column headers and ungrouping takes a level out", async () => {
  const { document, act, layout } = await open();
  for (const [col, shiftKey] of [["B", false], ["C", true]]) {
    const th = document.querySelector(`#pane-sheet th[data-col="${col}"]`);
    th.dispatchEvent({ type: "mousedown", target: th, shiftKey, button: 0 });
  }
  await act("group");
  assert.deepEqual(layout().outlineOf("Sheet1").cols, [[2, 3, 1]]);
  assert.equal(document.getElementById("sheet-outline-cols").hidden, false);
  await act("hide-detail");
  assert.deepEqual(layout().outlineOf("Sheet1").foldedCols, [[2, 3]]);
  assert.equal(layout().colHidden("Sheet1", 2), true);
  assert.equal(layout().get("Sheet1").hiddenCols.length, 0);
  await act("show-detail");
  await act("ungroup");
  assert.equal(layout().outlineOf("Sheet1"), null);
  await act("ungroup");
  assert.match(document.getElementById("status-command-feedback").textContent, /대상 그룹 없음/);
});

test("a row edit sends the groups and keeps the frozen panes and the tab colour the answer does not carry", async () => {
  const { document, act, selectRows, layout, requests } = await open();
  selectRows(3, 5);
  await act("group");
  layout().setFreeze("Sheet1", { rows: 1, cols: 0 });
  layout().setTabColor("Sheet1", "3366cc");
  selectRows(2, 2);
  await act("row-insert-above");
  const sent = requests.filter((r) => r.sessionAction === "structure").at(-1);
  assert.deepEqual(sent.outline.rows, [[3, 5, 1]]);
  assert.deepEqual(layout().freezeOf("Sheet1"), { rows: 1, cols: 0 });
  assert.equal(layout().tabColorOf("Sheet1"), "3366cc");
  assert.ok(document);
});

test("the tab colour is picked from the palette at the tab, shown as a line, and undone", async () => {
  const { document, layout, history, userClick } = await open();
  const tab = document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]');
  tab.dispatchEvent({ type: "contextmenu", target: tab, clientX: 40, clientY: 700 });
  const menu = document.getElementById("sheet-tab-menu");
  assert.equal(menu.hidden, false);
  assert.equal(menu.querySelector(".md-label").textContent, "탭 색");
  userClick(menu.querySelector('[data-tab-act="color"]'));
  await flushAsyncTurns(20);
  assert.equal(menu.hidden, true);
  const palette = document.getElementById("sheet-tab-color-palette");
  assert.equal(palette.hidden, false);
  const swatch = palette.querySelector(".sb-hl-swatch");
  swatch.click();
  await flushAsyncTurns(20);
  const color = layout().tabColorOf("Sheet1");
  assert.match(color, /^[0-9a-f]{6}$/);
  assert.equal(tab.dataset.tabColor, color);
  assert.equal(tab.style.getPropertyValue("--tab-color"), "#" + color);
  await history("studio-undo");
  assert.equal(layout().tabColorOf("Sheet1"), null);
  assert.equal(tab.dataset.tabColor, undefined);
  await history("studio-redo");
  assert.equal(tab.dataset.tabColor, color);
});
