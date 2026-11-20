import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// Board view (PD-263): the cards are grouped by a select or multi-select field only, so a drop never writes to the
// title field; a drop on the card's own column changes nothing; a multi-select card keeps its other tags.

const MODEL = new URL("../crates/easyoffice-app/ui/base-model.js", import.meta.url);
const STUDIO = new URL("../crates/easyoffice-app/ui/base-studio.js", import.meta.url);

async function openBoard() {
  const { document, host } = await createWorkspaceHarness();
  loadUiModule(MODEL, host);
  loadUiModule(STUDIO, host);
  const stage = document.querySelector(".canvas-stage");
  const root = document.createElement("section");
  root.id = "pane-base";
  root.className = "editor-pane base-pane";
  stage.appendChild(root);
  root.hidden = false;
  const log = { undo: 0 };
  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document,
    rootEl: root,
    invokeIpc: () => null,
    deliverExport: () => ({ ok: true }),
    setFeedback: () => {},
    recordUndo: () => { log.undo += 1; },
    noteActivity: () => {},
  });
  studio.bind();
  return { document, root, studio, log };
}

const drag = (root, recId, fromKey, toKey) => {
  const card = root.querySelector(`.base-kanban-col[data-kanban-value="${fromKey}"] .base-kanban-card[data-record-id="${recId}"]`);
  const col = root.querySelector(`.base-kanban-col[data-kanban-value="${toKey}"]`);
  assert.ok(card && col, `card in "${fromKey}" and column "${toKey}"`);
  card.dispatchEvent({ type: "dragstart", target: card, dataTransfer: { setData: () => {}, getData: () => recId } });
  col.dispatchEvent({ type: "drop", target: col, dataTransfer: { getData: () => recId } });
};

test("a drop on the card's own column changes nothing, not even the title", async () => {
  const { root, studio, log } = await openBoard();
  studio.addRecord({ "fld-1": "기획안" });
  const recId = studio.getState().tables[0].records[0].id;
  root.querySelector('[data-base-view="kanban"]').click();
  const before = log.undo;
  drag(root, recId, "", "");
  const values = studio.getState().tables[0].records[0].values;
  assert.equal(values["fld-1"], "기획안");
  assert.equal(log.undo, before, "no undo step for a no-op drop");
});

test("a table with no select field has no board to group by", async () => {
  const { root, studio } = await openBoard();
  studio.deleteField("fld-2");
  studio.addRecord({ "fld-1": "기획안" });
  root.querySelector('[data-base-view="kanban"]').click();
  assert.equal(root.querySelectorAll(".base-kanban-col").length, 0);
  assert.ok(root.querySelector(".base-empty-hint"));
  assert.equal(studio.getState().tables[0].records[0].values["fld-1"], "기획안");
});

test("a multi-select card moves between columns and keeps its other tags", async () => {
  const { root, studio } = await openBoard();
  studio.deleteField("fld-2");
  const tags = studio.addField({ name: "분류", type: "multiSelect", options: ["A", "B", "C"] }).field;
  studio.addRecord({ "fld-1": "기획안", [tags.id]: ["A", "B"] });
  const recId = studio.getState().tables[0].records[0].id;
  root.querySelector('[data-base-view="kanban"]').click();
  assert.ok(root.querySelector('.base-kanban-col[data-kanban-value="A"] .base-kanban-card'));
  assert.ok(root.querySelector('.base-kanban-col[data-kanban-value="B"] .base-kanban-card'));
  drag(root, recId, "A", "C");
  assert.deepEqual(studio.getState().tables[0].records[0].values[tags.id], ["B", "C"]);
  drag(root, recId, "B", "B");
  assert.deepEqual(studio.getState().tables[0].records[0].values[tags.id], ["B", "C"]);
  drag(root, recId, "C", "");
  assert.deepEqual(studio.getState().tables[0].records[0].values[tags.id], ["B"]);
});
