import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-265: a BASE file is untrusted input. Relations and formulas that lead back to themselves, or fan out from
// table to table, must show an error in the cell and finish quickly; they must not recurse without end.

const MODEL = new URL("../crates/easyoffice-app/ui/base-model.js", import.meta.url);

function loadModel() {
  const host = {};
  loadUiModule(MODEL, host);
  return host.__EASYOFFICE_BASE_MODEL__;
}

const titleFormulaTable = (id, next, records) => ({
  id, name: id,
  fields: [
    { id: id + "-title", name: "제목", type: "formula", formula: "{참조}" },
    { id: id + "-ref", name: "참조", type: "relation", relationTableId: next },
  ],
  records,
});

test("two tables whose title formulas show each other give the cycle error and do not overflow the stack", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({
    tables: [
      titleFormulaTable("A", "B", [{ id: "ra", values: { "A-ref": ["rb"] } }]),
      titleFormulaTable("B", "A", [{ id: "rb", values: { "B-ref": ["ra"] } }]),
    ],
  });
  assert.equal(loaded.ok, true);
  const row = M.queryTableView(loaded.state, "A").rows[0];
  assert.equal(row.displays["A-ref"], "#CYCLE!");
});

test("a record that points at itself through a relation does not loop", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({ tables: [titleFormulaTable("A", "A", [{ id: "ra", values: { "A-ref": ["ra"] } }])] });
  assert.equal(loaded.ok, true);
  assert.match(M.queryTableView(loaded.state, "A").rows[0].displays["A-ref"], /#CYCLE!/);
});

test("a chain of 14 tables that each point at two records of the next finishes within the evaluation budget", () => {
  const M = loadModel();
  const tables = [];
  for (let n = 0; n < 14; n += 1) {
    const next = n === 13 ? "T13" : "T" + (n + 1);
    tables.push({
      id: "T" + n, name: "T" + n,
      fields: [
        { id: "T" + n + "-title", name: "제목", type: "formula", formula: n === 13 ? '"끝"' : "{참조}" },
        { id: "T" + n + "-ref", name: "참조", type: "relation", relationTableId: next },
      ],
      records: [{ id: "a" + n, values: n === 13 ? {} : { ["T" + n + "-ref"]: ["a" + (n + 1), "b" + (n + 1)] } }, { id: "b" + n, values: n === 13 ? {} : { ["T" + n + "-ref"]: ["a" + (n + 1), "b" + (n + 1)] } }],
    });
  }
  const loaded = M.hydrateBaseState({ tables });
  assert.equal(loaded.ok, true);
  const started = performance.now();
  const row = M.queryTableView(loaded.state, "T0").rows[0];
  assert.ok(performance.now() - started < 3000, "the query ends within seconds, not after 2^14 branches of 14 levels");
  assert.ok(typeof row.displays["T0-ref"] === "string");
});

test("a normal relation still shows the title of the record it points at", () => {
  const M = loadModel();
  const loaded = M.hydrateBaseState({
    tables: [
      { id: "고객", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: [{ id: "rc", values: { c1: "한빛상사" } }] },
      { id: "주문", name: "주문", fields: [{ id: "o1", name: "번호", type: "text" }, { id: "o2", name: "고객", type: "relation", relationTableId: "고객" }], records: [{ id: "ro", values: { o1: "A-1", o2: ["rc"] } }] },
    ],
  });
  assert.equal(M.queryTableView(loaded.state, "주문").rows[0].displays.o2, "한빛상사");
});
