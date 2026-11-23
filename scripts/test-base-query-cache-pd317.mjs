import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

const UI = "../crates/easyoffice-app/ui/";
const MODULES = ["sheet-nav.js", "base-model.js", "base-query-cache.js", "base-virtual.js", "base-grid-window.js", "base-cell-nav.js", "base-clipboard.js", "base-cell-select.js", "base-studio.js"];

function normalizeQuery(q) {
  return {
    totalCount: q.totalCount,
    filteredCount: q.filteredCount,
    numericSums: q.numericSums,
    rows: q.rows.map((r) => ({ id: r.record.id, origIdx: r.origIdx, values: r.values, displays: r.displays })),
    groups: q.groups.map((g) => ({
      key: g.key, label: g.label, count: g.count, collapsed: g.collapsed, numericSums: g.numericSums,
      ids: g.records.map((r) => r.record.id),
    })),
  };
}

async function openBase() {
  const { document, host } = await createWorkspaceHarness();
  for (const name of MODULES) loadUiModule(new URL(UI + name, import.meta.url), host);
  const root = document.createElement("section");
  root.id = "pane-base";
  root.className = "editor-pane base-pane";
  document.querySelector(".canvas-stage").appendChild(root);
  root.hidden = false;
  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document, rootEl: root, invokeIpc: () => null, deliverExport: () => ({ ok: true }),
    setFeedback: () => {}, recordUndo: () => {}, captureUndo: () => JSON.stringify(studio.getState()), commitUndo: () => {}, noteActivity: () => {},
  });
  studio.bind();
  return { M: host.__EASYOFFICE_BASE_MODEL__, C: host.__EASYOFFICE_CREATE_BASE_QUERY_CACHE__, studio, root };
}

test("PD-317 incremental query matches stateless queryTableView across edits, paste, sort, filter, group, relations, and floats", async () => {
  const { root, M, studio } = await openBase();
  const fields = [
    { id: "f1", name: "이름", type: "text" },
    { id: "f2", name: "수량", type: "number" },
    { id: "f3", name: "상태", type: "select", options: ["대기", "진행", "완료"] },
    { id: "f4", name: "고객", type: "relation", relationTableId: "tc" },
    { id: "f5", name: "합계", type: "formula", formula: "ROUND({수량} / 3, 4)" },
    { id: "f6", name: "확인", type: "checkbox" },
    { id: "f7", name: "날짜", type: "date" },
  ];
  const records = [
    { id: "r0", values: { f1: "항목 0", f2: 10, f3: "진행", f4: ["c0"], f6: true, f7: "2026-11-01" } },
    { id: "r1", values: { f1: "항목 1", f2: 0.1, f3: "대기", f4: ["c1"], f6: false, f7: "2026-11-02" } },
    { id: "r2", values: { f1: "항목 2", f2: 0.2, f3: "대기", f4: ["c0"], f6: true, f7: "2026-11-03" } },
    { id: "r3", values: { f1: "항목 3", f2: 10, f3: "완료", f4: [], f6: false, f7: "" } },
    { id: "r4", values: { f1: "항목 4", f2: null, f3: "", f4: ["c1"], f6: false, f7: "2026-11-05" } },
  ];
  const tc = { id: "tc", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: [{ id: "c0", values: { c1: "김" } }, { id: "c1", values: { c1: "이" } }] };
  studio.setState({ version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }, tc] });

  const check = (label, tblId = "t1") => {
    const snap = studio.getState();
    snap.activeTableId = tblId;
    const expected = normalizeQuery(M.queryTableView(snap, tblId));
    const actual = normalizeQuery(studio.liveQuery());
    assert.deepEqual(actual, expected, label);
    assert.deepEqual(Object.keys(studio.liveQuery()), Object.keys(M.queryTableView(snap, tblId)), label + " keys");
  };

  check("initial");
  studio.updateCell("r1", "f1", "수정 1"); check("default non-numeric edit");
  studio.updateCell("r0", "f2", 25); check("default numeric edit with float sum in col");
  studio.addRecord({ f1: "추가 A", f2: 30, f3: "완료", f6: true }); check("default addRecord");
  studio.deleteRecord("r2"); check("default delete middle row");

  studio.setSorts([{ fieldId: "f2", dir: "asc" }, { fieldId: "f1", dir: "desc" }]); check("multi-sort set");
  studio.updateCell("r0", "f2", 10); check("sort key edit ties with origIdx");
  studio.updateCell("r1", "f2", 0); check("div/non-int formula update");

  studio.setFilters([{ fieldId: "f2", op: "gte", value: "0.1" }, { fieldId: "f6", op: "eq", value: "true" }], "or"); check("OR filter set");
  studio.updateCell("r4", "f2", 15); check("filter miss -> hit");
  studio.updateCell("r4", "f2", null); check("filter hit -> miss");
  studio.updateCells([{ recordId: "r0", fieldId: "f1", value: "배치" }, { recordId: "r3", fieldId: "f2", value: 12 }]); check("batch updateCells");

  studio.setGroupBy("f3"); check("group by select");
  studio.updateCell("r0", "f3", "완료"); check("group bucket move");
  studio.updateCell("r0", "f2", 40); check("in-bucket sum update");
  studio.setGroupBy("f6"); check("group by checkbox");
  studio.deleteRecord("r0"); check("delete inside grouped view");
  studio.setFieldWidth("f1", 220); studio.setRowHeight(32); check("width/rowHeight keep query exact");

  // Edit target relation table `고객` and switch back to `표` to verify external dependency invalidation.
  const st2 = studio.getState(); st2.activeTableId = "tc"; studio.setState(st2);
  check("customer table", "tc");
  studio.updateCell("c1", "c1", "박");
  root.querySelector('[data-table-id="t1"]').click();
  check("relation target rename propagated to 표");

  // Schema mutations invalidate cache cleanly.
  studio.addField({ name: "메모", type: "text" }); check("addField");
  studio.updateField("f5", { name: "몫", type: "formula", formula: "{수량} * 3" }); check("updateField formula");
  studio.deleteField("f7"); check("deleteField");
});

test("PD-317 handles self-cyclic and mutual-cyclic relation tables safely without stale cache (PD-265)", async () => {
  const { root, M, studio } = await openBase();
  studio.setState({
    version: 1,
    activeTableId: "A",
    tables: [
      { id: "A", name: "A", fields: [{ id: "t", name: "제목", type: "formula", formula: "{참조}" }, { id: "r", name: "참조", type: "relation", relationTableId: "B" }], records: [{ id: "ra", values: { r: ["rb"] } }] },
      { id: "B", name: "B", fields: [{ id: "t2", name: "제목", type: "formula", formula: "{참조}" }, { id: "r2", name: "참조", type: "relation", relationTableId: "A" }], records: [{ id: "rb", values: { r2: ["ra"] } }] },
    ],
  });
  assert.equal(studio.liveQuery().rows[0].displays.r, "#CYCLE!");
  studio.updateCell("ra", "r", []);
  assert.deepEqual(normalizeQuery(studio.liveQuery()), normalizeQuery(M.queryTableView(studio.getState(), "A")));
});

test("PD-317 benchmark compares stateless queryTableView vs incremental cache on 10,000 and 100,000 rows", async () => {
  const { M, C, studio } = await openBase();
  const makeState = (n) => ({
    version: 1,
    activeTableId: "t1",
    tables: [
      {
        id: "t1", name: "표",
        fields: [
          { id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number" },
          { id: "f3", name: "상태", type: "select", options: ["대기", "진행", "완료"] },
          { id: "f4", name: "고객", type: "relation", relationTableId: "tc" },
          { id: "f5", name: "합계", type: "formula", formula: "{수량} * 2" },
        ],
        records: Array.from({ length: n }, (_, i) => ({
          id: "r" + i, values: { f1: "항목 " + i, f2: i, f3: ["대기", "진행", "완료"][i % 3], f4: i % 2 === 0 ? ["c" + (i % 20)] : [] },
        })),
      },
      { id: "tc", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: Array.from({ length: 20 }, (_, i) => ({ id: "c" + i, values: { c1: "고객 " + i } })) },
    ],
  });

  // 10,000 rows via full studio.updateCell (DOM virtual window + incremental query)
  const st10k = makeState(10000);
  studio.setState(st10k);
  const full10kTimes = [];
  for (let i = 0; i < 6; i += 1) {
    const t0 = performance.now();
    M.queryTableView(st10k, "t1");
    full10kTimes.push(performance.now() - t0);
  }
  studio.updateCell("r9000", "f2", 9001);
  const studio10kTimes = [];
  for (let i = 0; i < 12; i += 1) {
    const t0 = performance.now();
    studio.updateCell("r9000", "f2", 9100 + i);
    studio10kTimes.push(performance.now() - t0);
  }
  const median = (arr) => arr.slice().sort((a, b) => a - b)[arr.length >> 1];

  // 100,000 rows isolated query comparison (bypassing MAX_RECORDS_PER_TABLE hydrate limit for PD-290 comparison scale)
  const st100k = makeState(100000);
  const cache = C(M);
  cache.query(st100k, "t1");
  const full100k = [];
  for (let i = 0; i < 3; i += 1) {
    const t0 = performance.now();
    M.queryTableView(st100k, "t1");
    full100k.push(performance.now() - t0);
  }
  const inc100k = [];
  for (let i = 0; i < 10; i += 1) {
    M.updateCell(st100k, "t1", "r95000", "f2", 50000 + i, cache.index(st100k));
    cache.noteUpdate(st100k, "t1", ["r95000"]);
    const t0 = performance.now();
    cache.query(st100k, "t1");
    inc100k.push(performance.now() - t0);
  }

  st100k.tables[0].sorts = [{ fieldId: "f2", dir: "desc" }];
  st100k.tables[0].filters = [{ fieldId: "f2", op: "gte", value: "100" }];
  st100k.tables[0].groupByFieldId = "f3";
  cache.query(st100k, "t1");
  const inc100kGrouped = [];
  for (let i = 0; i < 10; i += 1) {
    M.updateCell(st100k, "t1", "r95000", "f2", 95000 + i, cache.index(st100k));
    cache.noteUpdate(st100k, "t1", ["r95000"]);
    const t0 = performance.now();
    cache.query(st100k, "t1");
    inc100kGrouped.push(performance.now() - t0);
  }

  const mFull10k = median(full10kTimes), mStudio10k = median(studio10kTimes);
  const mFull100k = median(full100k), mInc100k = median(inc100k), mInc100kGrp = median(inc100kGrouped);
  console.log(`PD-317 bench 10k: stateless query=${mFull10k.toFixed(2)}ms -> incremental studio.updateCell=${mStudio10k.toFixed(2)}ms`);
  console.log(`PD-317 bench 100k: stateless query=${mFull100k.toFixed(2)}ms -> incremental plain=${mInc100k.toFixed(3)}ms, filter+sort+group=${mInc100kGrp.toFixed(3)}ms`);
  assert.ok(mStudio10k < 10, `10k incremental studio edit took ${mStudio10k.toFixed(2)}ms`);
  assert.ok(mInc100k < 5 && mInc100kGrp < 5, `100k incremental query took plain=${mInc100k.toFixed(2)}ms, grouped=${mInc100kGrp.toFixed(2)}ms`);
});
