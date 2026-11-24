import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

function createRecordingEngine(onMeta) {
  const calls = [];
  async function invokeIpc(command, payload) {
    const body = JSON.parse(payload);
    calls.push({ command, body });
    if (body.sessionAction === "open") return { session: "s-meta-1" };
    if (body.sessionAction === "close") return { closed: true };
    if (body.sessionAction === "cellMeta") {
      return onMeta ? onMeta(body) : { session: body.session, changed: [], spills: [], circular: false };
    }
    if (body.sessionAction === "apply") {
      return { session: body.session, changed: [], spills: [], circular: false };
    }
    if (!body.sessionAction) {
      return { targetValues: body.targets.map(() => "ok"), circular: false };
    }
    throw new Error("unexpected action " + body.sessionAction);
  }
  return { invokeIpc, calls, byAction: (a) => calls.filter((c) => c.body.sessionAction === a) };
}

test("plain sheets never send cellMeta RPC calls", async () => {
  const engine = createRecordingEngine();
  const { sandbox } = await createWorkspaceHarness();
  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: engine.invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
    getFileName: () => "PlainBook.xlsx",
    getColumnWidths: () => ({ Sheet1: { 1: 120 } }),
  });
  store.enableSheetSessions(true);
  await store.evaluateLiveFormulaTargets({ Sheet1: { A1: { value: "10" }, B1: { value: "", formula: "=A1*2" } } }, "Sheet1");
  assert.equal(engine.byAction("cellMeta").length, 0);
});

test("CELL and INFO formulas lazily sync fileName, cell styles, widths, locks, and scroll origin before apply", async () => {
  let colWidthB = 165;
  let docTitle = "제목 없음";
  const engine = createRecordingEngine((body) => ({
    session: body.session,
    changed: [{ sheet: "Sheet1", cell: "B1", value: body.fileName ? `[${body.fileName}]Sheet1` : "" }],
    spills: [],
    circular: false,
  }));
  const { host, sandbox } = await createWorkspaceHarness();
  host.__EASYOFFICE_SHEET_LAYOUT__.setUnlocked("Sheet1", "row", [[2, 2]]);
  host.__EASYOFFICE_SHEET_VIEW_STATE__.set("Sheet1", { top: [5, 3] });

  const store = sandbox.__EASYOFFICE_CREATE_SHEET_STORE__({
    invokeIpc: engine.invokeIpc,
    normalizeFormattedNumericCell: (v) => v,
    tokenizeFormulaStringLiterals: () => [],
    getFileName: () => docTitle,
    getColumnWidths: () => ({ Sheet1: { 2: colWidthB } }),
  });
  store.enableSheetSessions(true);

  const sheets = {
    Sheet1: {
      A1: { value: "hello", style: { numFmt: "#,##0", textAlign: "center" } },
      B1: { value: "", formula: '=CELL("filename")' },
      B2: { value: "", formula: '=CELL("format", A1)' },
    },
  };

  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  const actions = engine.calls.map((c) => c.body.sessionAction);
  assert.equal(actions.indexOf("cellMeta") < actions.indexOf("apply"), true, `expected cellMeta before apply: ${actions}`);

  const meta1 = engine.byAction("cellMeta")[0].body;
  assert.equal(meta1.fileName, "");
  assert.deepEqual(meta1.cellMeta.Sheet1.numFmts, ["#,##0"]);
  assert.deepEqual(meta1.cellMeta.Sheet1.cells, { A1: { nf: 1, ha: "c" } });
  assert.deepEqual(meta1.cellMeta.Sheet1.cols, [[2, 165]]);
  assert.deepEqual(meta1.cellMeta.Sheet1.lock, { unlocked: [], locked: [], rows: [[2, 2]], cols: [] });
  assert.deepEqual(meta1.cellMeta.Sheet1.origin, [5, 3]);
  assert.equal(sheets.Sheet1.B1.value, "");

  // Repeating evaluation with unchanged metadata does not emit another cellMeta RPC.
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.byAction("cellMeta").length, 1);
  assert.equal(store.cellMetaStale(sheets), false);

  // Changing column width and safe document basename marks cellMeta stale and triggers update.
  colWidthB = 200;
  docTitle = "/Users/ignored/Budget.xlsx×";
  assert.equal(store.cellMetaStale(sheets), true);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  assert.equal(engine.byAction("cellMeta").length, 2);
  const meta2 = engine.byAction("cellMeta")[1].body;
  assert.equal(meta2.fileName, "Budget.xlsx");
  assert.deepEqual(meta2.cellMeta.Sheet1.cols, [[2, 200]]);
  assert.equal(sheets.Sheet1.B1.value, "[Budget.xlsx]Sheet1");

  // Stateless RPC fallback also carries safe fileName and cellMeta when sessions are disabled.
  store.enableSheetSessions(false);
  await store.evaluateLiveFormulaTargets(sheets, "Sheet1");
  const stateless = engine.calls.at(-1).body;
  assert.equal(stateless.sessionAction, undefined);
  assert.equal(stateless.fileName, "Budget.xlsx");
  assert.deepEqual(stateless.cellMeta.Sheet1.cols, [[2, 200]]);
});
