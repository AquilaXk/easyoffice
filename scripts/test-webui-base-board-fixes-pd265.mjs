import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// PD-265 fixes of the BASE grid and the BOARD text editing: a relation cell does not carry the whole target table,
// a view setting and a text edit are undo steps, a refused change leaves no step, numbers show 15 digits, links
// open through the shell.

const MODEL = new URL("../crates/easyoffice-app/ui/base-model.js", import.meta.url);
const STUDIO = new URL("../crates/easyoffice-app/ui/base-studio.js", import.meta.url);

async function openBase(extra = {}) {
  const { document, host } = await createWorkspaceHarness();
  loadUiModule(MODEL, host);
  loadUiModule(STUDIO, host);
  const root = document.createElement("section");
  root.id = "pane-base";
  root.className = "editor-pane base-pane";
  document.querySelector(".canvas-stage").appendChild(root);
  root.hidden = false;
  const log = { undo: 0, activity: 0, feedback: [] };
  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document, rootEl: root, invokeIpc: () => null, deliverExport: () => ({ ok: true }),
    setFeedback: (text) => log.feedback.push(text),
    recordUndo: () => { log.undo += 1; },
    noteActivity: () => { log.activity += 1; },
    ...extra,
  });
  studio.bind();
  return { document, host, root, studio, log };
}

function relationState(count) {
  const customers = { id: "tc", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: [] };
  const orders = { id: "to", name: "주문", fields: [{ id: "o1", name: "번호", type: "text" }, { id: "o2", name: "고객", type: "relation", relationTableId: "tc" }], records: [] };
  for (let i = 0; i < count; i += 1) {
    customers.records.push({ id: "rc" + i, values: { c1: "고객 " + i } });
    orders.records.push({ id: "ro" + i, values: { o1: "주문 " + i, o2: ["rc" + i] } });
  }
  return { version: 1, activeTableId: "to", tables: [customers, orders] };
}

test("a relation cell holds its own record until it is used, and the list is built once for the field", async () => {
  const { root, studio } = await openBase();
  studio.setState(relationState(60));
  const selects = root.querySelectorAll('td[data-field-type="relation"] select');
  assert.equal(selects.length, 60);
  assert.ok(selects.every((s) => s.querySelectorAll("option").length <= 2), "at most the empty choice and the held record");
  const optionNodes = root.querySelectorAll("option").length;
  assert.ok(optionNodes < 60 * 3 + 200, "the grid carries about one option per row, not rows x records: " + optionNodes);

  const first = selects[0];
  first.dispatchEvent({ type: "focus", target: first });
  const filled = first.querySelectorAll("option");
  assert.equal(filled.length, 61, "the empty choice and the 60 customers");
  assert.equal(first.value, "rc0", "the held record stays selected");
  const second = selects[1];
  second.dispatchEvent({ type: "mousedown", target: second });
  assert.equal(second.querySelectorAll("option").length, 61);
  assert.equal(second.value, "rc1");
});

test("picking a record in a relation cell that was filled on use writes the relation", async () => {
  const { root, studio } = await openBase();
  studio.setState(relationState(5));
  const select = root.querySelector('td[data-field-type="relation"] select');
  select.dispatchEvent({ type: "focus", target: select });
  select.value = "rc3";
  select.dispatchEvent({ type: "change", target: select });
  assert.deepEqual(studio.getState().tables[1].records[0].values.o2, ["rc3"]);
});

test("a view setting is one undo step, and setting what is already set is none", async () => {
  const { studio, root, log } = await openBase();
  const before = log.undo;
  root.querySelector('[data-base-view="kanban"]').click();
  assert.equal(log.undo, before + 1, "the view change");
  root.querySelector('[data-base-view="kanban"]').click();
  assert.equal(log.undo, before + 1, "the same view again");
  root.querySelector('[data-base-view="calendar"]').click();
  const calendarSteps = log.undo;
  root.querySelector("#btn-base-cal-next").click();
  assert.equal(log.undo, calendarSteps + 1, "the month");
  root.querySelector("#btn-base-cal-today").click();
  assert.equal(log.undo, calendarSteps + 2);
  assert.equal(studio.getState().tables[0].view, "calendar");
});

test("collapsing a group is an undo step and counts as an edit", async () => {
  const { studio, root, log } = await openBase();
  studio.addRecord({ "fld-1": "가", "fld-2": "대기" });
  studio.setGroupBy("fld-2");
  const steps = log.undo, activity = log.activity;
  root.querySelector(".base-group-toggle").click();
  assert.equal(log.undo, steps + 1);
  assert.ok(log.activity > activity);
  assert.equal(studio.getState().tables[0].collapsedGroups.length, 1);
});

test("a change the model refuses leaves no undo step when the host takes the state before and keeps it on success", async () => {
  const taken = [], kept = [];
  const { studio } = await openBase({
    captureUndo: () => { const snap = { n: taken.length }; taken.push(snap); return snap; },
    commitUndo: (snap) => kept.push(snap),
  });
  const refused = studio.addField({ name: "", type: "text" });
  assert.equal(refused.ok, false);
  assert.equal(kept.length, 0, "no step for the refused field");
  const accepted = studio.addField({ name: "비고", type: "text" });
  assert.equal(accepted.ok, true);
  assert.equal(kept.length, 1);
  assert.equal(kept[0], taken[1], "the step holds the state taken before the accepted change");
});

test("a formula number shows 15 significant digits", async () => {
  const { root, studio } = await openBase();
  studio.addField({ name: "합", type: "formula", formula: "0.1 + 0.2" });
  studio.addRecord({ "fld-1": "x" });
  assert.match(root.querySelector(".base-formula-val").textContent, /^0\.3$/);
});

test("a web address opens through the shell when the shell is there, and through the anchor otherwise", async () => {
  const { host, root, studio } = await openBase();
  studio.addField({ name: "링크", type: "url" });
  const fieldId = studio.getState().tables[0].fields.at(-1).id;
  studio.addRecord({ [fieldId]: "https://example.com/a" });
  const link = root.querySelector("a.base-url-link");
  assert.ok(link);
  const clicked = () => link.dispatchEvent({ type: "click", target: link });
  clicked();

  const opened = [];
  const bridge = host.__EASYOFFICE_DESKTOP_BRIDGE__;
  Object.defineProperties(bridge, { isAvailable: { value: () => true, configurable: true }, openLink: { value: async (url) => opened.push(url), configurable: true } });
  assert.deepEqual(opened, [], "outside the shell the anchor does the work");
  clicked();
  assert.deepEqual(opened, ["https://example.com/a"]);
});

test("the table list of the outline finds a table whose id has quotes in it", async () => {
  const { document, host } = await createWorkspaceHarness({ search: "?module=BASE&win=w-base-esc" });
  const bridge = host.__EASYOFFICE_BASE_BOARD_HOST__;
  const studio = bridge.getBaseStudio();
  const state = studio.getState();
  state.tables[0].id = 'a"]b';
  state.activeTableId = 'a"]b';
  state.tables.push({ ...structuredClone(state.tables[0]), id: "t2", name: "둘째", fields: state.tables[0].fields.map((f) => ({ ...f })) });
  assert.equal(studio.setState(state).ok, true);
  const items = bridge.outlineItems("BASE");
  assert.equal(items.length, 2);
  assert.doesNotThrow(() => items[0].action());
  assert.ok(document.getElementById("pane-base"));
});

test("editing the text of a board element is one undo step for the whole session", async () => {
  const { document, host } = await createWorkspaceHarness({ search: "?module=BOARD&win=w-board-undo" });
  const studio = host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  studio.addNode({ kind: "text", x: 10, y: 10, text: "처음" });
  await flushAsyncTurns(5);
  const steps = () => host.__EASYOFFICE_WORKSPACE_HISTORY__.sizes().undo;
  const base = steps();
  const card = document.querySelector(".board-node--text");
  card.dispatchEvent({ type: "dblclick", target: card });
  const body = card.querySelector(".board-node-text");
  for (const text of ["처", "처음에", "처음에 쓴 글"]) {
    body.textContent = text;
    body.dispatchEvent({ type: "input", target: body });
  }
  assert.equal(steps(), base + 1, "three keystrokes, one step");
  assert.equal(studio.getState().nodes[0].text, "처음에 쓴 글");
  const untouched = document.querySelector(".board-node--text");
  untouched.dispatchEvent({ type: "dblclick", target: untouched });
  assert.equal(steps(), base + 1, "opening the editor without typing is no step");
});
