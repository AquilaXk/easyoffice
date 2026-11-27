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

// The field and table counters are in the same file and lag the same way: a new field or table must not take an id that is there.
const fieldTable = (id, fieldIds) => ({ id, name: id, fields: fieldIds.map((f) => ({ id: f, name: "필드 " + f, type: "text" })), records: [] });

test("the field counter is moved past the largest fld-N id of every table", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({
    nextFieldSeq: 1,
    tables: [fieldTable("A", ["fld-3", "fld-12"]), fieldTable("B", ["fld-7"])],
  });
  assert.equal(loaded.ok, true);
  assert.equal(loaded.state.nextFieldSeq, 13);
  const added = M.addField(loaded.state, "A", { name: "새 필드", type: "text" });
  assert.equal(added.field.id, "fld-13");
  const ids = loaded.state.tables[0].fields.map((f) => f.id);
  assert.equal(new Set(ids).size, ids.length, "no field id is taken twice");
});

test("the table counter is moved past the largest tbl-N id", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({
    nextTableSeq: 2,
    tables: [fieldTable("tbl-4", ["a"]), fieldTable("tbl-9", ["b"]), fieldTable("tbl-2", ["c"])],
  });
  assert.equal(loaded.ok, true);
  assert.equal(loaded.state.nextTableSeq, 10);
  const added = M.addTable(loaded.state, {});
  assert.equal(added.table.id, "tbl-10");
  const ids = loaded.state.tables.map((t) => t.id);
  assert.equal(new Set(ids).size, ids.length, "no table id is taken twice");
});

test("field and table counters that are ahead are kept, and a file with none starts after its ids", () => {
  const M = loadModel();
  const ahead = M.hydrateBaseState({ nextFieldSeq: 50, nextTableSeq: 30, tables: [fieldTable("tbl-3", ["fld-3"])] });
  assert.equal(ahead.state.nextFieldSeq, 50);
  assert.equal(ahead.state.nextTableSeq, 30);
  const none = M.hydrateBaseState({ tables: [fieldTable("tbl-5", ["fld-8"])] });
  assert.equal(none.state.nextFieldSeq, 9);
  assert.equal(none.state.nextTableSeq, 6);
  const plain = M.hydrateBaseState({ tables: [fieldTable("sales", ["name"])] });
  assert.equal(plain.state.nextFieldSeq, 1);
  assert.equal(plain.state.nextTableSeq, 2);
});

test("field and table ids that are not fld-N or tbl-N, or have more than 15 digits, do not move the counters", () => {
  const M = loadModel();
  const bad = ["fld-x", "col-90", "fld-99999999999999999999999", "fld-4a", "xfld-50", "fld-", "fld--5", "fld-1e9", "fld-1000000000000000"];
  const badTables = ["tbl-x", "tab-90", "tbl-99999999999999999999999", "tbl-4a", "xtbl-50", "tbl-", "tbl--5", "tbl-1000000000000000"];
  const loaded = M.hydrateBaseState({
    nextFieldSeq: 2,
    nextTableSeq: 2,
    tables: badTables.map((id, i) => fieldTable(id, i === 0 ? bad : ["f" + i])),
  });
  assert.equal(loaded.ok, true);
  assert.equal(loaded.state.nextFieldSeq, 2);
  assert.equal(loaded.state.nextTableSeq, badTables.length + 1);
  // Fifteen digits are the most that count, and the counter stays an exact integer.
  const top = M.hydrateBaseState({ tables: [fieldTable("tbl-999999999999999", ["fld-999999999999999"])] });
  assert.equal(top.state.nextFieldSeq, 1_000_000_000_000_000);
  assert.equal(top.state.nextTableSeq, 1_000_000_000_000_000);
  assert.ok(Number.isSafeInteger(top.state.nextFieldSeq));
});

test("a record saved without an id takes the rec-N of its place and the counter follows it", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({ nextRecordSeq: 1, tables: [table("A", [{ values: {} }, { values: {} }, rec("rec-9")])] });
  assert.equal(loaded.ok, true);
  assert.equal(loaded.state.nextRecordSeq, 10);
});
