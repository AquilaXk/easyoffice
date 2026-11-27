import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// A saved BASE file may carry a record counter that is behind the `rec-N` ids it holds (an older writer, a hand edit, a
// merge). Loading moves the counter past the largest id, so the next record never takes an id that is already there.

const MODEL = new URL("../crates/easyoffice-app/ui/base-model.js", import.meta.url);

function loadModel() {
  const host = {};
  loadUiModule(MODEL, host);
  return host.__EASYOFFICE_BASE_MODEL__;
}

const table = (id, records) => ({ id, name: id, fields: [{ id: id + "-f", name: "이름", type: "text" }], records });
const rec = (id) => ({ id, values: {} });

test("the record counter is moved past the largest rec-N id of every table", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({
    nextRecordSeq: 1,
    tables: [table("A", [rec("rec-3"), rec("rec-12")]), table("B", [rec("rec-7")])],
  });
  assert.equal(loaded.ok, true);
  assert.equal(loaded.state.nextRecordSeq, 13);
  const added = M.addRecord(loaded.state, "A", {});
  assert.equal(added.record.id, "rec-13");
  const ids = loaded.state.tables.flatMap((t) => t.records.map((r) => r.id));
  assert.equal(new Set(ids).size, ids.length, "no id is taken twice");
});

test("a counter that is already ahead is kept, and a file with no counter starts after its ids", () => {
  const M = loadModel();
  const ahead = M.hydrateBaseState({ nextRecordSeq: 40, tables: [table("A", [rec("rec-3")])] });
  assert.equal(ahead.state.nextRecordSeq, 40);
  const none = M.hydrateBaseState({ tables: [table("A", [rec("rec-5")])] });
  assert.equal(none.state.nextRecordSeq, 6);
  const empty = M.hydrateBaseState({ tables: [table("A", [])] });
  assert.equal(empty.state.nextRecordSeq, 1);
});

test("ids that are not rec-N, or too large to count, do not move the counter", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({
    nextRecordSeq: 2,
    tables: [table("A", [rec("rec-x"), rec("row-90"), rec("rec-99999999999999999999999"), rec("rec-4a"), rec("xrec-50"), rec("rec-"), rec("rec--5")])],
  });
  assert.equal(loaded.ok, true);
  assert.equal(loaded.state.nextRecordSeq, 2);
});

test("a record saved without an id takes the rec-N of its place and the counter follows it", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({ nextRecordSeq: 1, tables: [table("A", [{ values: {} }, { values: {} }, rec("rec-9")])] });
  assert.equal(loaded.ok, true);
  assert.equal(loaded.state.nextRecordSeq, 10);
});
