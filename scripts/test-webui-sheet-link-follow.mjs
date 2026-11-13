import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The in-workbook targets of cell links follow a row or column edit and a sheet rename or deletion (PD-218). The engine's own
// rules are tested in Rust; here a small stand-in answers the requests, to see what the page sends and what it does with the answer.

const HERE = (sheet, ref) => `#${sheet}!${ref}`;

function fakeEngine() {
  const requests = [];
  async function evaluate(body) {
    requests.push(body);
    switch (body.sessionAction) {
      case "open":
        return { session: "s1" };
      case "apply":
        return { session: body.session, changed: [], circular: false };
      case "structure": {
        const linkTargets = {};
        for (const [sheet, targets] of Object.entries(body.linkTargets || {})) {
          const pairs = targets
            .map((target) => [target, /^#(\w+)!([A-Z]+)(\d+)$/.exec(target)])
            .filter(([, m]) => m && m[1] === body.sheet && body.axis === "row" && body.op === "insert" && Number(m[3]) >= body.at)
            .map(([target, m]) => [target, `#${m[1]}!${m[2]}${Number(m[3]) + body.count}`]);
          if (pairs.length > 0) linkTargets[sheet] = pairs;
        }
        return { session: body.session, formulas: [], changed: [], circular: false, removed: 0, merges: [], hiddenRows: [], hiddenCols: [], widths: [], rules: {}, ...(body.linkTargets ? { linkTargets } : {}) };
      }
      case "sheetRefs": {
        const linkTargets = {};
        for (const [sheet, targets] of Object.entries(body.linkTargets || {})) {
          const pairs = targets
            .filter((target) => target.startsWith(`#${body.sheet}!`))
            .map((target) => [target, body.op === "delete" ? "##REF!" : target.replace(`#${body.sheet}!`, `#'${body.newName}'!`)]);
          if (pairs.length > 0) linkTargets[sheet] = pairs;
        }
        return { session: body.session, formulas: [], rules: {}, linkTargets };
      }
      default:
        return {};
    }
  }
  return { requests, evaluate };
}

async function open(sheets) {
  const engine = fakeEngine();
  const converts = [];
  const model = { sheets: sheets.map(({ name, cells, links }) => ({ name, cells: Object.entries(cells).map(([ref, raw]) => ({ ref, raw })), links })) };
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") return engine.evaluate(JSON.parse(args.payload));
      if (command === "convert_document_format") {
        const body = JSON.parse(args.payload);
        if (body.action === "decode") return JSON.stringify({ status: "OK", module: "SHEET", format: "xlsx", kind: "workbook", itemCount: model.sheets.length, warnings: [], ...model });
        converts.push(body);
        return JSON.stringify({ status: "OK", convertedBase64: "", fileName: "x.xlsx" });
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const result = await harness.sandbox.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("book.xlsx", new TextEncoder().encode("book"));
  await flushAsyncTurns(40);
  assert.equal(result.ok, true);
  const { document, userClick } = harness;
  const cell = (ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  const tab = (name) => document.querySelector(`#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="${name}"]`);
  const saved = async () => {
    userClick(studioItem(document, "studio-save"));
    await flushAsyncTurns(40);
    return Object.fromEntries(converts.at(-1).sheets.map((sheet) => [sheet.name, sheet.links ?? []]));
  };
  const undo = async () => {
    userClick(studioItem(document, "studio-undo"));
    await flushAsyncTurns(30);
  };
  return { ...harness, engine, cell, tab, saved, undo };
}

const BOOK = [
  {
    name: "Sheet1",
    cells: { A1: "첫째", A2: "둘째", A3: "셋째", B5: "대상" },
    links: [
      { ref: "A1", target: HERE("Sheet1", "B5") },
      { ref: "A2", target: "https://example.com/a" },
      { ref: "A3", target: HERE("Sheet2", "B2") },
    ],
  },
  { name: "Sheet2", cells: { A1: "다른 시트", B2: "도착" }, links: [{ ref: "A1", target: HERE("Sheet1", "B5") }] },
];

test("a row insert sends the in-workbook targets, moves the links with their cells and rewrites the ones that point below", async () => {
  const t = await open(BOOK);
  const a1 = t.cell("A1");
  a1.focus();
  t.userClick(t.document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(40);
  const sent = t.engine.requests.find((r) => r.sessionAction === "structure");
  assert.deepEqual(sent.linkTargets, { Sheet1: ["#Sheet1!B5", "#Sheet2!B2"], Sheet2: ["#Sheet1!B5"] }, "each target once, no address outside the workbook");
  const links = await t.saved();
  assert.deepEqual(links.Sheet1, [
    { ref: "A2", target: "#Sheet1!B6" },
    { ref: "A3", target: "https://example.com/a" },
    { ref: "A4", target: "#Sheet2!B2" },
  ]);
  assert.deepEqual(links.Sheet2, [{ ref: "A1", target: "#Sheet1!B6" }], "a link on another sheet follows too");
  await t.undo();
  const back = await t.saved();
  assert.deepEqual(back.Sheet1[0], { ref: "A1", target: "#Sheet1!B5" });
  assert.deepEqual(back.Sheet2, [{ ref: "A1", target: "#Sheet1!B5" }]);
});

test("a workbook without a link to a place in it sends no targets", async () => {
  const t = await open([{ name: "Sheet1", cells: { A1: "x" }, links: [{ ref: "A1", target: "https://example.com/a" }] }]);
  t.cell("A1").focus();
  t.userClick(t.document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(40);
  const sent = t.engine.requests.find((r) => r.sessionAction === "structure");
  assert.equal("linkTargets" in sent, false);
});

test("a rename follows the targets that name the sheet; one undo step restores them", async () => {
  const t = await open(BOOK);
  const tab = t.tab("Sheet2");
  tab.dispatchEvent({ type: "dblclick", target: tab });
  const input = t.document.querySelector("#sheet-tab-bar .sheet-tab-edit");
  input.value = "둘째 시트";
  input.dispatchEvent({ type: "keydown", key: "Enter", target: input });
  await flushAsyncTurns(40);
  const sent = t.engine.requests.find((r) => r.sessionAction === "sheetRefs");
  assert.deepEqual(sent.linkTargets, { Sheet1: ["#Sheet1!B5", "#Sheet2!B2"], Sheet2: ["#Sheet1!B5"] });
  const links = await t.saved();
  assert.deepEqual(links.Sheet1.find((l) => l.ref === "A3"), { ref: "A3", target: "#'둘째 시트'!B2" });
  assert.deepEqual(links["둘째 시트"], [{ ref: "A1", target: "#Sheet1!B5" }], "the links on the renamed sheet go with it");
  await t.undo();
  const back = await t.saved();
  assert.deepEqual(back.Sheet1.find((l) => l.ref === "A3"), { ref: "A3", target: "#Sheet2!B2" });
});

test("deleting a sheet turns the links to it into reference errors and drops its own links", async () => {
  const t = await open(BOOK);
  const tab = t.tab("Sheet2");
  tab.dispatchEvent({ type: "contextmenu", target: tab, clientX: 40, clientY: 700 });
  t.userClick(t.document.querySelector('#sheet-tab-menu [data-tab-act="delete"]'));
  await flushAsyncTurns(20);
  t.userClick(t.document.getElementById("btn-sheet-tab-delete-ok"));
  await flushAsyncTurns(40);
  const links = await t.saved();
  assert.deepEqual(links.Sheet1.find((l) => l.ref === "A3"), { ref: "A3", target: "##REF!" });
  assert.equal(links.Sheet2, undefined);
  t.cell("A3").focus();
  assert.equal(t.cell("A3").dataset.link, "internal");
});
