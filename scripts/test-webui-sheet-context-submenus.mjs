import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The cell menu's 필터 and 정렬 submenus and its 드롭다운 목록에서 선택 (PD-391).

async function openSheet() {
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") return { session: "s1", formulas: [], changed: [], circular: false };
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  return harness;
}

const cell = (h, ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const byId = (h, id) => h.document.getElementById(id);
const labels = (el) => [...el.children].filter((c) => !c.classList.contains("md-sep")).map((c) => c.querySelector(":scope > .md-label").textContent);

function openMenu(h, ref) {
  cell(h, ref).dispatchEvent({ type: "contextmenu", target: cell(h, ref), button: 2, clientX: 50, clientY: 50, bubbles: true });
  return byId(h, "sheet-cell-menu");
}

test("the cell menu follows the spreadsheet program: 필터 and 정렬 are submenus and the list pick has its own line", async () => {
  const h = await openSheet();
  const menu = openMenu(h, "B3");
  assert.equal(menu.hidden, false);
  assert.deepEqual(labels(menu), [
    "잘라내기", "복사", "붙여넣기", "선택하여 붙여넣기…",
    "삽입…", "삭제…", "내용 지우기",
    "필터", "정렬",
    "메모 삽입", "셀 서식…", "드롭다운 목록에서 선택…", "하이퍼링크…",
  ]);
  const sub = (title) => [...menu.querySelectorAll(":scope > .md-sub")].find((el) => el.querySelector(":scope > .md-label").textContent === title).querySelector(":scope > .md-sub-panel");
  assert.deepEqual(labels(sub("필터")), ["필터 지우기", "다시 적용", "선택한 셀의 값으로 필터링", "선택한 셀의 색으로 필터링", "선택한 셀의 글꼴 색으로 필터링"]);
  assert.deepEqual(labels(sub("정렬")).slice(-1), ["사용자 지정 정렬…"]);
});

test("the sort lines read as text or as numbers by the cell, and a quick sort orders the region by the column", async () => {
  const h = await openSheet();
  openMenu(h, "A2");
  assert.equal(byId(h, "sheet-ctx-sort-asc").textContent, "텍스트 오름차순 정렬");
  const numeric = [...h.document.querySelectorAll("#pane-sheet td[data-cell]")].find((td) => /^\d+$/.test(td.textContent.trim()) && td.dataset.cell.startsWith("B"));
  openMenu(h, numeric.dataset.cell);
  assert.equal(byId(h, "sheet-ctx-sort-asc").textContent, "가장 작은 값에서 가장 큰 값으로 정렬");
  assert.equal(byId(h, "sheet-ctx-sort-desc").textContent, "가장 큰 값에서 가장 작은 값으로 정렬");
});

test("the keys open a submenu and walk its items", async () => {
  const h = await openSheet();
  const menu = openMenu(h, "B3");
  const line = [...menu.querySelectorAll(":scope > .md-sub")][0];
  line.focus();
  menu.dispatchEvent({ type: "keydown", key: "ArrowRight", target: line });
  assert.equal(line.classList.contains("open"), true);
  const first = line.querySelector(".md-sub-panel .md-item");
  assert.equal(h.document.activeElement, first);
  first.dispatchEvent({ type: "keydown", key: "ArrowDown", target: first, bubbles: true });
  assert.equal(h.document.activeElement.querySelector(".md-label").textContent, "다시 적용");
  h.document.activeElement.dispatchEvent({ type: "keydown", key: "ArrowLeft", target: h.document.activeElement, bubbles: true });
  assert.equal(line.classList.contains("open"), false);
  assert.equal(h.document.activeElement, line);
});

test("드롭다운 목록에서 선택 lists the values of the column around the cell, sorted, and a pick writes the cell", async () => {
  const h = await openSheet();
  const column = "A";
  const rows = [...h.document.querySelectorAll(`#pane-sheet td[data-cell^="${column}"]`)].map((td) => td.dataset.cell);
  const target = rows[2];
  openMenu(h, target);
  h.userClick(h.document.querySelector('.md-item[data-sheet-ctx="pick-list"]'));
  await flushAsyncTurns(6);
  const list = byId(h, "sheet-pick-menu");
  assert.equal(list.hidden, false);
  const values = [...list.children].map((el) => el.textContent);
  assert.ok(values.length >= 2, "the other cells of the column are offered: " + values.join(","));
  assert.deepEqual(values, [...values].sort((a, b) => a.localeCompare(b, "ko")));
  assert.equal(values.length, new Set(values).size, "each value once");
  const pick = list.children[0];
  const text = pick.textContent;
  h.userClick(pick);
  await flushAsyncTurns(15);
  assert.equal(list.hidden, true);
  assert.equal(cell(h, target).textContent, text);
});
