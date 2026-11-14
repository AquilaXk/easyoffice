import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// The page's side of arrays that spill (PD-217): the model entries the engine's answers produce, what the page
// sends to the engine, what it saves.

async function spillApi() {
  const { host } = await createWorkspaceHarness();
  return { spill: host.__EASYOFFICE_SHEET_SPILL__ };
}

const entry = (value, formula = "") => ({ value, formula });

test("an area is filled from the answer and forgotten when the formula stops spilling", async () => {
  const { spill } = await spillApi();
  const map = { A1: entry("1", "=SEQUENCE(3)"), D1: entry("keep") };
  const values = { A2: "2", A3: "3" };
  spill.applyInfo(map, { cell: "A1", rows: 3, cols: 1, blocked: false }, (ref) => values[ref]);
  assert.deepEqual(map.A2, { value: "2", formula: "", spillOf: "A1" });
  assert.equal(map.A3.value, "3");
  assert.equal(map.A1.spillRows, 3);
  assert.deepEqual(spill.areasOf(map), ["A1:A3"]);
  assert.deepEqual(spill.areaOf(map, "A3"), { anchor: "A1", r0: 1, c0: 1, r1: 3, c1: 1 });
  assert.equal(spill.areaOf(map, "D1"), null);
  // the array shrinks
  spill.applyInfo(map, { cell: "A1", rows: 2, cols: 1, blocked: false }, (ref) => values[ref]);
  assert.equal(map.A3, undefined);
  assert.equal(map.A2.spillOf, "A1");
  // it stops spilling: the cells go, the anchor keeps its formula
  spill.applyInfo(map, { cell: "A1", rows: 0, cols: 0, blocked: false }, () => "");
  assert.equal(map.A2, undefined);
  assert.equal(map.A1.formula, "=SEQUENCE(3)");
  assert.equal(map.A1.spillRows, undefined);
});

test("a cell the user wrote is never overwritten by a result, and a blocked array fills nothing", async () => {
  const { spill } = await spillApi();
  const map = { A1: entry("", "=SEQUENCE(3)"), A3: entry("typed") };
  spill.applyInfo(map, { cell: "A1", rows: 3, cols: 1, blocked: false }, () => "x");
  assert.equal(map.A3.value, "typed");
  assert.equal(map.A3.spillOf, undefined);
  const blocked = { A1: entry("#SPILL!", "=SEQUENCE(3)"), A2: entry("2", "") };
  blocked.A2.spillOf = "A1";
  blocked.A1.spillRows = 2;
  blocked.A1.spillCols = 1;
  spill.applyInfo(blocked, { cell: "A1", rows: 3, cols: 1, blocked: true }, () => "x");
  assert.equal(blocked.A2, undefined);
  assert.equal(blocked.A1.spillRows, undefined);
});

test("a stateless answer replaces every area", async () => {
  const { spill } = await spillApi();
  const sheets = {
    S: { A1: entry("1", "=SEQUENCE(2,2)"), B1: { value: "old", formula: "", spillOf: "Z9" } },
  };
  spill.replaceAll(sheets, [{ sheet: "S", cell: "A1", rows: 2, cols: 2, blocked: false, values: ["1", "2", "3", "4"] }]);
  assert.deepEqual(
    ["A1", "B1", "A2", "B2"].map((r) => sheets.S[r].value),
    ["1", "2", "3", "4"]
  );
  assert.equal(sheets.S.B2.spillOf, "A1");
  spill.replaceAll(sheets, []);
  assert.equal(sheets.S.B1, undefined);
  assert.equal(sheets.S.A1.spillRows, undefined);
});

test("an unchanged result stays a result and an edited one becomes input", async () => {
  const { spill } = await spillApi();
  const child = { value: "2", formula: "", spillOf: "A1" };
  assert.equal(spill.stillResult(child, "2"), true);
  assert.equal(spill.stillResult(child, "typed"), false);
  assert.equal(spill.isInput(child), false);
  assert.equal(spill.isInput(entry("2")), true);
});

test("the session sends no result, and folds the areas of its answer into the model", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const sent = [];
  const invokeIpc = async (command, payload) => {
    const body = JSON.parse(payload);
    if (body.sessionAction === "open") return { session: "s1" };
    if (body.sessionAction === "close") return { closed: true };
    if (body.sessionAction === "apply") {
      sent.push(...body.changes.map((c) => c.cell + "=" + c.value));
      return {
        session: "s1",
        changed: [
          { sheet: "Sheet1", cell: "B1", value: "1" },
          { sheet: "Sheet1", cell: "B2", value: "2" },
          { sheet: "Sheet1", cell: "B3", value: "3" },
        ],
        spills: [{ sheet: "Sheet1", cell: "B1", rows: 3, cols: 1, blocked: false }],
        circular: false,
        recomputed: 1,
      };
    }
    throw new Error("unexpected " + command);
  };
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  const sheets = { Sheet1: { A1: entry("5"), B1: entry("", "=SEQUENCE(A1)") } };
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(sent.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)), ["A1=5", "B1==SEQUENCE(A1)"]);
  assert.equal(live.spillsChanged, true);
  assert.deepEqual(
    ["B2", "B3"].map((r) => sheets.Sheet1[r]),
    [
      { value: "2", formula: "", spillOf: "B1" },
      { value: "3", formula: "", spillOf: "B1" },
    ]
  );
  // the results are not input: a second sync sends nothing new
  sent.length = 0;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(sent, []);
});

test("an area is forgotten even when the entry of its formula was edited and lost the size", async () => {
  const { spill } = await spillApi();
  const map = { A1: entry("", "=SEQUENCE(3)") };
  spill.applyInfo(map, { cell: "A1", rows: 3, cols: 1, blocked: false }, () => "x");
  map.A1 = entry("#SPILL!", "=SEQUENCE(4)");
  spill.applyInfo(map, { cell: "A1", rows: 4, cols: 1, blocked: true }, () => "");
  assert.equal(map.A2, undefined);
  assert.equal(map.A3, undefined);
  map.A1 = entry("typed");
  spill.applyInfo(map, { cell: "A1", rows: 0, cols: 0, blocked: false }, () => "");
  assert.equal(Object.keys(map).length, 1);
});

test("a names answer that shrinks an area removes the cells the area filled", async () => {
  const { sandbox, host } = await createWorkspaceHarness();
  let applied = 0;
  const invokeIpc = async (command, payload) => {
    const body = JSON.parse(payload);
    if (body.sessionAction === "open") return { session: "s1" };
    if (body.sessionAction === "close") return { closed: true };
    if (body.sessionAction === "apply") {
      if (applied++ > 0) return { session: "s1", changed: [], circular: false, recomputed: 0 };
      return {
        session: "s1",
        changed: [1, 2, 3].map((n) => ({ sheet: "Sheet1", cell: "B" + n, value: String(n) })),
        spills: [{ sheet: "Sheet1", cell: "B1", rows: 3, cols: 1, blocked: false }],
        circular: false,
        recomputed: 1,
      };
    }
    if (body.sessionAction === "names") {
      return {
        session: "s1",
        changed: [{ sheet: "Sheet1", cell: "B1", value: "1" }],
        spills: [{ sheet: "Sheet1", cell: "B1", rows: 1, cols: 1, blocked: false }],
        circular: false,
      };
    }
    throw new Error("unexpected " + command);
  };
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(true);
  const sheets = { Sheet1: { A1: entry("5"), B1: entry("", "=SEQUENCE(N)") } };
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(sheets.Sheet1.B3.value, "3");
  host.__EASYOFFICE_SHEET_NAMES__.replaceAll([{ name: "N", refersTo: "1" }], true);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(sheets.Sheet1.B2, undefined);
  assert.equal(sheets.Sheet1.B3, undefined);
});

test("masking personal data skips the cells an array filled and counts only what the user wrote", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const scrubbed = [];
  const invokeIpc = async (command, payload) => {
    if (command !== "scrub_pii") throw new Error("unexpected " + command);
    scrubbed.push(payload);
    const hit = payload.includes("010-1234-5678");
    return { scrubbedText: payload.replaceAll("010-1234-5678", "010-****-****"), totalMasked: hit ? 1 : 0 };
  };
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  const sheets = {
    Sheet1: {
      A1: entry("010-1234-5678"),
      B1: entry("010-1234-5678"),
      C2: { value: "010-1234-5678", formula: "", spillOf: "C1" },
    },
  };
  const out = await store.scrubSheetStoreMap(sheets, "Sheet1", null);
  assert.equal(out.totalMasked, 2, "the typed cells A1 and B1 count; the cell C2 an array filled does not");
  assert.deepEqual(scrubbed, ["010-1234-5678", "010-1234-5678"]);
  assert.equal(out.nextSheets.Sheet1.C2.value, "010-1234-5678", "a result is left to the next calculation");
});

test("scrubSheetStoreMap batches more than 32 entries in 1024-item chunks and offlineRpc supports texts arrays", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const offlineRpc = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_STYLE_LABELS,
    contextualToolBadges: sandbox.__EASYOFFICE_RIBBON_CATALOG__.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: sandbox.__EASYOFFICE_RIBBON_CATALOG__.MODULE_SPECS,
    localRibbonState: sandbox.__EASYOFFICE_RIBBON_CATALOG__.createDefaultRibbonState(),
  });

  const directBatch = offlineRpc.scrub_pii(JSON.stringify({ texts: ["010-1234-5678", "홍길동", "a@b.kr"] }));
  assert.equal(directBatch.results.length, 3);
  assert.equal(directBatch.results[0].scrubbedText, "010-****-****");
  assert.equal(directBatch.results[2].scrubbedText, "***@***");
  assert.equal(directBatch.totalMasked, 2);
  assert.throws(() => offlineRpc.scrub_pii(JSON.stringify({ texts: "bad" })));

  const payloads = [];
  const invokeIpc = async (command, payload) => {
    if (command !== "scrub_pii") throw new Error("unexpected " + command);
    payloads.push(payload);
    return offlineRpc.scrub_pii(payload);
  };
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  const sheet1 = {};
  for (let r = 1; r <= 1030; r += 1) {
    sheet1[`A${r}`] = entry(r === 1 || r === 1030 ? "010-1234-5678" : `항목_${r}`);
  }
  const out = await store.scrubSheetStoreMap({ Sheet1: sheet1 }, "Sheet1", null);
  assert.equal(payloads.length, 2, "1030 items must batch into two chunks (1024 + 6)");
  assert.equal(JSON.parse(payloads[0]).texts.length, 1024);
  assert.equal(JSON.parse(payloads[1]).texts.length, 6);
  assert.equal(out.totalMasked, 2);
  assert.equal(out.nextSheets.Sheet1.A1.value, "010-****-****");
  assert.equal(out.nextSheets.Sheet1.A1030.value, "010-****-****");
});
