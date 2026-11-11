import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// A stand-in for the engine's session commands: it keeps the cells it was sent and reports a
// formula cell's value ("v:" + its text) when that text is new or changed.
function createFakeEngine(options = {}) {
  const calls = [];
  const sessions = new Map();
  let nextId = 1;
  async function invokeIpc(command, payload) {
    const body = JSON.parse(payload);
    const action = body.sessionAction ? "sheet_session_" + body.sessionAction : command;
    calls.push({ command: action, body });
    if (options.failOpen && action === "sheet_session_open") throw new Error("세션 사용 불가");
    if (action === "sheet_session_open") {
      const id = "s" + nextId++;
      sessions.set(id, Object.assign(new Map(), { pending: [] }));
      return { session: id };
    }
    if (action === "sheet_session_close") return { closed: sessions.delete(body.session) };
    if (action === "sheet_session_apply") {
      const cells = sessions.get(body.session);
      if (!cells) return { expired: true };
      for (const c of body.changes) {
        const key = c.sheet + "!" + c.cell;
        if (c.value === "") cells.delete(key);
        else if (cells.get(key) !== c.value) {
          cells.set(key, c.value);
          if (c.value.startsWith("=")) cells.pending.push({ sheet: c.sheet, cell: c.cell, value: "v:" + c.value });
        }
      }
      if (body.recalc === false) return { session: body.session, staged: body.changes.length };
      const changed = cells.pending.splice(0);
      return { session: body.session, changed, circular: false, recomputed: changed.length };
    }
    if (action === "evaluate_sheet") {
      const targets = body.targets || [];
      return {
        targetValues: targets.map((t) => "legacy:" + t.cell),
        values: {},
        value: "legacy",
        circular: false,
      };
    }
    throw new Error("unexpected command " + command);
  }
  return { invokeIpc, calls, sessions, only: (name) => calls.filter((c) => c.command === name) };
}

async function createStore(engine, enabled = true) {
  const { sandbox } = await createWorkspaceHarness();
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: engine.invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
  });
  store.enableSheetSessions(enabled);
  return store;
}

const entry = (value, formula = "") => ({ value, formula });

function baseStore() {
  return { Sheet1: { A1: entry("1"), B1: entry("", "=A1+1"), C1: entry("x") } };
}

test("the first evaluation opens a session and sends every cell once", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const live = await store.evaluateLiveFormulaTargets(baseStore(), "Sheet1");
  assert.equal(engine.only("sheet_session_open").length, 1);
  const applies = engine.only("sheet_session_apply");
  assert.equal(applies.length, 1);
  assert.equal(applies[0].body.recalc, true);
  assert.deepEqual(
    applies[0].body.changes.map((c) => c.cell + "=" + c.value).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)),
    ["A1=1", "B1==A1+1", "C1=x"]
  );
  assert.deepEqual(live.formulaTargets.map((t) => t.key), ["Sheet1!B1"]);
  assert.deepEqual(live.targetValues, ["v:=A1+1"]);
  assert.equal(live.rpcRes.circular, false);
  assert.equal(engine.only("evaluate_sheet").length, 0, "the stateless command is not used");
});

test("a later edit sends only the changed cell and keeps the unchanged formula values", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const sheets = baseStore();
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  sheets.Sheet1.A1 = entry("5");
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  const applies = engine.only("sheet_session_apply");
  assert.deepEqual(applies[1].body.changes, [{ sheet: "Sheet1", cell: "A1", value: "5" }]);
  assert.deepEqual(live.targetValues, ["v:=A1+1"], "B1's value comes from the page's mirror");
  assert.equal(engine.only("sheet_session_open").length, 1);

  sheets.Sheet1.B1 = entry("", "=A1+2");
  const changedFormula = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(engine.only("sheet_session_apply")[2].body.changes, [
    { sheet: "Sheet1", cell: "B1", value: "=A1+2" },
  ]);
  assert.deepEqual(changedFormula.targetValues, ["v:=A1+2"]);
});

test("deleted cells are cleared in the session and an unchanged store sends no cells", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const sheets = baseStore();
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  delete sheets.Sheet1.C1;
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(engine.only("sheet_session_apply")[1].body.changes, [
    { sheet: "Sheet1", cell: "C1", value: "" },
  ]);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.deepEqual(engine.only("sheet_session_apply")[2].body.changes, []);
});

test("an expired session is reopened and every cell is sent again without a visible error", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const sheets = baseStore();
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  engine.sessions.clear();
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.only("sheet_session_open").length, 2);
  const applies = engine.only("sheet_session_apply");
  assert.equal(applies.at(-1).body.changes.length, 3, "the full store is resent");
  assert.deepEqual(live.targetValues, ["v:=A1+1"]);
  assert.equal(engine.only("evaluate_sheet").length, 0);
});

test("a changed sheet set opens a new session and closes the old one", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const sheets = baseStore();
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  sheets.Sheet2 = { A1: entry("", "=Sheet1!A1*2") };
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.only("sheet_session_open").length, 2);
  assert.deepEqual(engine.only("sheet_session_open")[1].body.sheets, ["Sheet1", "Sheet2"]);
  assert.equal(engine.only("sheet_session_close").length, 1);
  assert.equal(engine.only("sheet_session_apply").at(-1).body.changes.length, 4);
});

test("a session that cannot be opened falls back to the stateless command", async () => {
  const engine = createFakeEngine({ failOpen: true });
  const store = await createStore(engine);
  const live = await store.evaluateLiveFormulaTargets(baseStore(), "Sheet1");
  assert.equal(engine.only("evaluate_sheet").length, 1);
  assert.equal(engine.only("evaluate_sheet")[0].body.cells.length, 3, "the full payload");
  assert.deepEqual(live.targetValues, ["legacy:B1"]);
});

test("the stateless command is used until the shell advertises sessions", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine, false);
  await store.evaluateLiveFormulaTargets(baseStore(), "Sheet1");
  assert.equal(engine.only("sheet_session_open").length, 0);
  assert.equal(engine.only("evaluate_sheet").length, 1);
});

test("a large store is sent in chunks and only the last one recalculates", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const big = "x".repeat(200_000);
  const sheets = { Sheet1: { A1: entry("", "=B1") } };
  for (let i = 1; i <= 20; i += 1) sheets.Sheet1["B" + i] = entry(big + i);
  const live = await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  const applies = engine.only("sheet_session_apply");
  assert.ok(applies.length > 1, "split into several requests");
  assert.ok(applies.slice(0, -1).every((a) => a.body.recalc === false));
  assert.equal(applies.at(-1).body.recalc, true);
  assert.equal(applies.reduce((n, a) => n + a.body.changes.length, 0), 21);
  for (const a of applies) assert.ok(JSON.stringify(a.body).length < 2_000_000);
  assert.deepEqual(live.targetValues, ["v:=B1"]);
});

test("overlapping evaluations are serialised so no change is sent twice", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const sheets = baseStore();
  const first = store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  const second = store.evaluateLiveFormulaTargets(structuredClone(sheets), "Sheet1");
  await Promise.all([first, second]);
  const applies = engine.only("sheet_session_apply");
  assert.equal(applies.length, 2);
  assert.equal(applies[0].body.changes.length, 3);
  assert.equal(applies[1].body.changes.length, 0);
});

test("the formula bar evaluation reads its value from the session; a plain value uses the stateless command", async () => {
  const engine = createFakeEngine();
  const store = await createStore(engine);
  const sheets = baseStore();
  sheets.Sheet1.D1 = entry("", "=B1*3");
  const evaluated = await store.computeEvaluatedSheetStore(sheets, "Sheet1", { sheet: "Sheet1", cell: "D1" });
  assert.equal(evaluated.extraValue, "v:=B1*3");
  assert.equal(evaluated.sheets.Sheet1.B1.value, "v:=A1+1");
  assert.equal(engine.only("evaluate_sheet").length, 0);

  sheets.Sheet1.E1 = entry("42");
  const plain = await store.computeEvaluatedSheetStore(sheets, "Sheet1", { sheet: "Sheet1", cell: "E1" });
  assert.equal(engine.only("evaluate_sheet").length, 1);
  assert.equal(plain.extraValue, "legacy:E1");
});
