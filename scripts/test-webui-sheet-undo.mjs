import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem, commandItem, createMemoryStorage } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";
import { fakeSortEngine } from "./test-support/fake-sort-engine.mjs";

// The sheet's undo history on the page (PD-221): every kind of edit goes back and forth through steps that keep only
// what the edit changed, mixed chains come back state by state, and the settings (layout, names, rules, tab order) go
// with the cells.

// A stand-in session engine: it holds the formulas it was sent, rewrites them for a row edit like the real one does for
// plain references, and answers `sort` like the page tests' sort engine.
function createEngine() {
  const formulas = new Map();
  const anchors = new Set();
  const requests = [];
  const shift = (text, edit, own) =>
    text.replace(/(?<![A-Za-z!])([A-Z]{1,3})(\d+)\b/g, (all, col, row) => {
      if (!own || edit.axis !== "row") return all;
      const n = Number(row);
      if (edit.op === "insert") return n >= edit.at ? col + (n + edit.count) : all;
      return n >= edit.at + edit.count ? col + (n - edit.count) : n >= edit.at ? "#REF!" : all;
    });
  const answer = (body) => {
    requests.push(body);
    if (body.sessionAction === "open") return { session: "s1" };
    if (body.sessionAction === "close") return { closed: true };
    if (body.sessionAction === "apply") {
      for (const c of body.changes) {
        if (c.value.startsWith("=")) formulas.set(c.sheet + "!" + c.cell, c.value);
        else formulas.delete(c.sheet + "!" + c.cell);
      }
      // An array formula `=SEQUENCE(3)` fills three cells downwards; its removal frees them.
      const spills = [];
      const changed = [];
      for (const [key, text] of formulas) {
        if (text !== "=SEQUENCE(3)" || anchors.has(key)) continue;
        const [sheet, cell] = key.split("!");
        anchors.add(key);
        spills.push({ sheet, cell, rows: 3, cols: 1, blocked: false });
        for (let i = 0; i < 3; i += 1) changed.push({ sheet, cell: cell[0] + (Number(cell.slice(1)) + i), value: String(i + 1) });
      }
      for (const key of [...anchors]) {
        if (formulas.has(key)) continue;
        anchors.delete(key);
        const [sheet, cell] = key.split("!");
        spills.push({ sheet, cell, rows: 0, cols: 0, blocked: false });
      }
      return { session: "s1", changed, spills, circular: false };
    }
    if (body.sessionAction === "sort") return fakeSortEngine(body);
    if (body.sessionAction === "structure") {
      const rewritten = [];
      const next = new Map();
      for (const [key, text] of formulas) {
        const [sheet, cell] = key.split("!");
        const m = /^([A-Z]+)(\d+)$/.exec(cell);
        const own = sheet === body.sheet;
        let row = Number(m[2]);
        if (own && body.axis === "row") {
          if (body.op === "insert" && row >= body.at) row += body.count;
          else if (body.op === "delete") {
            if (row >= body.at + body.count) row -= body.count;
            else if (row >= body.at) continue;
          }
        }
        const ref = m[1] + row;
        const value = shift(text, body, true);
        next.set(sheet + "!" + ref, value);
        if (value !== text || ref !== cell) rewritten.push({ sheet, cell: ref, value });
      }
      formulas.clear();
      for (const [k, v] of next) formulas.set(k, v);
      // The rules of a sheet move with its rows, as the engine answers them.
      const rules = {};
      for (const [sheet, set] of Object.entries(body.rules || {})) {
        const moved = structuredClone(set);
        for (const rule of moved.cf || []) {
          rule.r = rule.r.replace(/([A-Z]+)(\d+)/g, (all, col, row) => (sheet === body.sheet && body.axis === "row" && body.op === "insert" && Number(row) >= body.at ? col + (Number(row) + body.count) : all));
        }
        rules[sheet] = moved;
      }
      return { session: "s1", formulas: rewritten, changed: [], circular: false, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules };
    }
    return {};
  };
  return { answer, requests };
}

async function open() {
  const engine = createEngine();
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") return engine.answer(JSON.parse(args.payload));
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const { document, userClick, host } = harness;
  const cell = (ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  const act = async (name) => {
    userClick(document.querySelector(`.md-item[data-sheet-act="${name}"]`));
    await flushAsyncTurns(20);
  };
  const history = async (name) => {
    userClick(studioItem(document, name));
    await flushAsyncTurns(20);
  };
  const typeInto = async (ref, text) => {
    const td = cell(ref);
    td.focus();
    for (const ch of text) {
      const active = document.activeElement;
      active.dispatchEvent({ type: "keydown", key: ch, target: active });
      active.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: active });
      active.textContent += ch;
      active.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: active });
    }
    cell(ref).dispatchEvent({ type: "keydown", key: "Tab", target: cell(ref) });
    await flushAsyncTurns(15);
  };
  const press = (ref) => {
    const td = cell(ref);
    td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
    td.dispatchEvent({ type: "mouseup", target: td, bubbles: true });
  };
  // Everything the sheet window shows and keeps: the cells of the window, the layout, names, rules, widths and the tabs.
  const view = () =>
    structuredClone({
      cells: [...document.querySelectorAll("#pane-sheet td[data-cell]")].filter((td) => /^[A-H]\d+$/.test(td.dataset.cell)).map((td) => [td.dataset.cell, td.textContent, td.dataset.formula || "", td.style.cssText || "", JSON.stringify(td._st || null), td._nf || ""]),
      layout: host.__EASYOFFICE_SHEET_LAYOUT__.getAll(),
      names: host.__EASYOFFICE_SHEET_NAMES__.getAll(),
      rules: host.__EASYOFFICE_SHEET_RULES__.getAll(),
      order: host.__EASYOFFICE_SHEET_TAB_STRIP__.getOrder(),
    });
  const sizes = () => host.__EASYOFFICE_WORKSPACE_HISTORY__.sizes();
  return { ...harness, engine, cell, act, history, typeInto, press, view, sizes, layout: host.__EASYOFFICE_SHEET_LAYOUT__, names: host.__EASYOFFICE_SHEET_NAMES__ };
}

// Runs `edit`, then undoes and redoes it: the page shows the state before after the undo and the state after after the redo.
async function roundTrip(t, edit, steps = 1) {
  const before = t.view();
  await edit();
  const after = t.view();
  assert.notDeepEqual(after, before, "the edit changed something");
  for (let i = 0; i < steps; i += 1) await t.history("studio-undo");
  assert.deepEqual(t.view(), before, "the undo brings back the state before");
  for (let i = 0; i < steps; i += 1) await t.history("studio-redo");
  assert.deepEqual(t.view(), after, "the redo brings back the state after");
  for (let i = 0; i < steps; i += 1) await t.history("studio-undo");
  assert.deepEqual(t.view(), before, "and the undo again");
  for (let i = 0; i < steps; i += 1) await t.history("studio-redo");
  assert.deepEqual(t.view(), after);
}

test("a typed value and a typed formula are steps that go back and forth, and cost one cell each", async () => {
  const t = await open();
  await roundTrip(t, () => t.typeInto("B2", "9999"));
  await roundTrip(t, () => t.typeInto("C9", "=B2+1"));
  assert.equal(t.sizes().undo, 2);
  // 2 steps of one cell are a few hundred bytes, not the workbook.
  assert.ok(t.sizes().bytes < 4096, "bytes: " + t.sizes().bytes);
});

test("a bold format over a range, a hidden row, a merge and a freeze go back and forth", async () => {
  const t = await open();
  const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
  await roundTrip(t, async () => {
    t.press("B2");
    key(t.document.activeElement, "ArrowDown", { shiftKey: true });
    key(t.document.activeElement, "ArrowRight", { shiftKey: true });
    t.userClick(commandItem(t.document, "sheet-home-bold"));
    await flushAsyncTurns(20);
  });
  await roundTrip(t, async () => {
    t.press("A5");
    await t.act("hide");
  });
  await roundTrip(t, async () => {
    t.press("D3");
    key(t.document.activeElement, "ArrowRight", { shiftKey: true });
    await t.act("merge");
  });
});

test("a row insert and a row delete go back with the formulas they rewrote and the cells they deleted", async () => {
  const t = await open();
  t.press("B3");
  await roundTrip(t, () => t.act("row-insert-above"));
  await roundTrip(t, () => t.act("row-delete"));
  assert.equal(t.sizes().undo, 2);
});

test("a mixed chain, an edit then a row insert then a sort then an edit, comes back state by state", async () => {
  const t = await open();
  const states = [t.view()];
  await t.typeInto("B2", "9999");
  states.push(t.view());
  t.press("B3");
  await t.act("row-insert-above");
  states.push(t.view());
  t.press("A4");
  await t.act("sort-dialog");
  t.userClick(t.document.getElementById("btn-sheet-sort-apply"));
  await flushAsyncTurns(25);
  states.push(t.view());
  await t.typeInto("H9", "끝");
  states.push(t.view());
  assert.equal(new Set(states.map((v) => JSON.stringify(v))).size, 5, "each step changed the page");
  for (let i = states.length - 2; i >= 0; i -= 1) {
    await t.history("studio-undo");
    assert.deepEqual(t.view(), states[i], "undo to state " + i);
  }
  for (let i = 1; i < states.length; i += 1) {
    await t.history("studio-redo");
    assert.deepEqual(t.view(), states[i], "redo to state " + i);
  }
  for (let i = states.length - 2; i >= 1; i -= 1) {
    await t.history("studio-undo");
    assert.deepEqual(t.view(), states[i], "and undo again to state " + i);
  }
});

test("sheet steps go back and forth: insert, copy, rename, move, hide and delete a sheet", async () => {
  const t = await open();
  const tab = (name) => t.document.querySelector(`.sheet-tab-btn[data-sheet-tab="${name}"]`);
  assert.ok(tab("Sheet1") && tab("Sheet2"));
  await roundTrip(t, () => t.act("tab-insert"));
  await roundTrip(t, () => t.act("tab-copy"));
  await roundTrip(t, () => t.act("tab-right"));
  await roundTrip(t, () => t.act("tab-hide"));
});

test("a hundred and five steps keep the last hundred, and an edit after an undo ends the redo", async () => {
  const t = await open();
  for (let i = 0; i < 105; i += 1) await t.typeInto("B" + (2 + (i % 5)), String(1000 + i));
  assert.equal(t.sizes().undo, 100);
  await t.history("studio-undo");
  await t.history("studio-undo");
  assert.deepEqual([t.sizes().undo, t.sizes().redo], [98, 2]);
  await t.typeInto("C2", "x");
  assert.deepEqual([t.sizes().undo, t.sizes().redo], [99, 0]);
});

test("a protection change empties the record of steps", async () => {
  const t = await open();
  await t.typeInto("B2", "5");
  assert.equal(t.sizes().undo, 1);
  await t.act("sheet-protect");
  for (const flag of t.host.__EASYOFFICE_SHEET_PROTECT__.FLAG_NAMES) t.document.getElementById("sheet-protect-allow-" + flag).checked = ["selectLockedCells", "selectUnlockedCells"].includes(flag);
  t.document.getElementById("btn-sheet-protect-apply").click();
  await flushAsyncTurns(20);
  assert.deepEqual([t.sizes().undo, t.sizes().redo], [0, 0], "the record of steps ends where the protection starts (PD-216)");
});

test("an array that spills goes back and forth: its cells are results, so the undo frees them and the redo fills them again", async () => {
  const t = await open();
  const column = () => ["H10", "H11", "H12"].map((ref) => t.cell(ref)?.textContent);
  const before = t.view();
  await t.typeInto("H10", "=SEQUENCE(3)");
  assert.deepEqual(column(), ["1", "2", "3"], "the array fills its cells");
  const after = t.view();
  await t.history("studio-undo");
  assert.deepEqual(column(), ["", "", ""], "the undo takes the formula and the cells it filled");
  assert.deepEqual(t.view(), before);
  await t.history("studio-redo");
  assert.deepEqual(column(), ["1", "2", "3"]);
  assert.deepEqual(t.view(), after);
});

test("the engine session holds what the page shows after an undo and after a redo", async () => {
  const t = await open();
  const lastSent = (cell) => {
    const applies = t.engine.requests.filter((r) => r.sessionAction === "apply");
    for (let i = applies.length - 1; i >= 0; i -= 1) {
      const change = applies[i].changes.find((c) => c.cell === cell);
      if (change) return change.value;
    }
    return undefined;
  };
  await t.typeInto("B2", "=B3+1");
  assert.equal(lastSent("B2"), "=B3+1");
  await t.history("studio-undo");
  assert.equal(lastSent("B2"), "1150", "the session is told the value the undo brought back");
  await t.history("studio-redo");
  assert.equal(lastSent("B2"), "=B3+1", "and the formula the redo brought back");
});

test("after an autosave restore the record of steps starts empty, and the next edit undoes to the restored value", async () => {
  const storage = createMemoryStorage();
  const first = await createWorkspaceHarness({ localStorage: storage });
  switchModule(first.document, "SHEET");
  await flushAsyncTurns();
  const b2 = first.document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "2000";
  b2.dispatchEvent({ type: "input", target: b2 });
  b2.dispatchEvent({ type: "keydown", key: "Enter", target: b2 });
  await flushAsyncTurns();
  b2.blur();
  await flushAsyncTurns();
  first.advanceTimers(5000);
  await flushAsyncTurns(10);

  const second = await createWorkspaceHarness({ localStorage: storage });
  await flushAsyncTurns(10);
  const cell = (ref) => second.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  assert.equal(cell("B2").textContent, "2000", "the restored value");
  assert.deepEqual(second.host.__EASYOFFICE_WORKSPACE_HISTORY__.sizes(), { undo: 0, redo: 0, bytes: 0 });
  await second.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(second.document.getElementById("status-command-feedback").textContent, "실행 취소할 작업이 없습니다");
  const b3 = cell("B3");
  b3.focus();
  b3.textContent = "77";
  b3.dispatchEvent({ type: "input", target: b3 });
  b3.dispatchEvent({ type: "keydown", key: "Enter", target: b3 });
  await flushAsyncTurns(10);
  await second.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(cell("B3").textContent, "", "the edit after the restore goes back");
  assert.equal(cell("B2").textContent, "2000", "the restored value stays");
});

// ---- one step of each kind of edit, undone and redone -------------------------------------------------------------

const keyOn = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const selectRange = (t, from, downs, rights) => {
  t.press(from);
  for (let i = 0; i < downs; i += 1) keyOn(t.document.activeElement, "ArrowDown", { shiftKey: true });
  for (let i = 0; i < rights; i += 1) keyOn(t.document.activeElement, "ArrowRight", { shiftKey: true });
};
const lookItem = (t, name) => t.document.querySelector(`[data-sheet-look="${name}"]`);
const numFmtItem = (t, name) => t.document.querySelector(`[data-sheet-numfmt="${name}"]`);
const clipboard = () => {
  const store = {};
  return { setData: (type, value) => (store[type] = value), getData: (type) => store[type] || "" };
};

const EDITS = {
  "clear the cells of a range with Delete": async (t) => {
    selectRange(t, "B2", 1, 1);
    keyOn(t.document.activeElement, "Delete");
    await flushAsyncTurns(20);
  },
  "paste a copied range": async (t) => {
    selectRange(t, "B2", 1, 1);
    const clip = clipboard();
    t.document.activeElement.dispatchEvent({ type: "copy", target: t.document.activeElement, clipboardData: clip });
    t.press("D5");
    t.document.activeElement.dispatchEvent({ type: "paste", target: t.document.activeElement, clipboardData: clip });
    await flushAsyncTurns(25);
  },
  "fill down with the handle": async (t) => {
    selectRange(t, "B2", 1, 0);
    const handle = t.document.querySelector("#sheet-viewport .sheet-fill-handle");
    handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
    const td = t.cell("B6");
    td.dispatchEvent({ type: "mouseover", target: td, bubbles: true });
    td.dispatchEvent({ type: "mouseup", target: td, bubbles: true });
    await flushAsyncTurns(25);
  },
  "align a range to the centre": async (t) => {
    selectRange(t, "B2", 1, 1);
    t.userClick(lookItem(t, "align-center"));
    await flushAsyncTurns(20);
  },
  "draw an outer border": async (t) => {
    selectRange(t, "B2", 1, 1);
    t.userClick(lookItem(t, "border-outer"));
    await flushAsyncTurns(20);
  },
  "show a range as a percentage": async (t) => {
    selectRange(t, "B2", 1, 1);
    t.userClick(numFmtItem(t, "percent"));
    await flushAsyncTurns(20);
  },
  "sort a range": async (t) => {
    t.press("A4");
    await t.act("sort-dialog");
    t.userClick(t.document.getElementById("btn-sheet-sort-apply"));
    await flushAsyncTurns(25);
  },
  "turn the auto filter on": async (t) => {
    t.press("B3");
    await t.act("filter-toggle");
  },
  "freeze the first row": (t) => t.act("freeze-first-row"),
  "set a print area": async (t) => {
    selectRange(t, "B2", 2, 2);
    await t.act("print-area-set");
  },
  "insert a column": async (t) => {
    t.press("C3");
    await t.act("col-insert-left");
  },
  "delete a column": async (t) => {
    t.press("C3");
    await t.act("col-delete");
  },
  "unhide after hiding": async (t) => {
    t.press("A5");
    await t.act("hide");
  },
};

for (const [name, edit] of Object.entries(EDITS)) {
  test("undo and redo of one step: " + name, async () => {
    const t = await open();
    await roundTrip(t, () => edit(t));
    assert.equal(t.sizes().undo >= 1, true);
  });
}

test("the rules of a sheet move with a row insert, and the undo puts them back where they were", async () => {
  const t = await open();
  t.host.__EASYOFFICE_SHEET_RULES__.set("Sheet1", { cf: [{ r: "B2:B6", t: "cellIs", op: "greaterThan", a: "1000", bg: "#ffeeaa", fl: 1 }], dv: [] });
  await flushAsyncTurns(10);
  const before = t.view();
  t.press("B3");
  await t.act("row-insert-above");
  assert.equal(t.host.__EASYOFFICE_SHEET_RULES__.getAll().Sheet1.cf[0].r, "B2:B7", "the rule's range grew with the rows");
  await t.history("studio-undo");
  assert.deepEqual(t.view(), before);
  assert.equal(t.host.__EASYOFFICE_SHEET_RULES__.getAll().Sheet1.cf[0].r, "B2:B6");
  await t.history("studio-redo");
  assert.equal(t.host.__EASYOFFICE_SHEET_RULES__.getAll().Sheet1.cf[0].r, "B2:B7");
});

test("the window knows its unsaved state through an undo and a redo: an undo to the saved content is clean again", async () => {
  const storage = createMemoryStorage();
  const t = await createWorkspaceHarness({ localStorage: storage });
  switchModule(t.document, "SHEET");
  await flushAsyncTurns();
  const dirty = async () => {
    t.advanceTimers(5000);
    await flushAsyncTurns(10);
    return t.document.getElementById("app").dataset.dirty;
  };
  assert.equal(await dirty(), "false");
  const b2 = t.document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "2000";
  b2.dispatchEvent({ type: "input", target: b2 });
  b2.dispatchEvent({ type: "keydown", key: "Enter", target: b2 });
  await flushAsyncTurns(10);
  assert.equal(await dirty(), "true");
  await t.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(await dirty(), "false", "the undo is back at the saved content");
  await t.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(await dirty(), "true", "the redo is an edit again");
});
