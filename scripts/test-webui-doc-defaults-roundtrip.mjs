import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The defaults of an opened DOCX, ODT or RTF (font, size, alignment, paragraph spacing, line height, tab stop; PD-363) come with
// the open answer as `docDefaults` and go back with the save, so a file opened and saved from the editor keeps its own defaults.

const DEFAULTS = {
  font: "Calibri",
  eastAsianFont: "맑은 고딕",
  sizeHalfPoints: 22,
  alignment: "left",
  spaceBeforeTwips: 0,
  spaceAfterTwips: 200,
  line240ths: 276,
  tabStopTwips: 720,
};

async function open(name, answer) {
  const converts = [];
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "convert_document_format") {
        const body = JSON.parse(args.payload);
        if (body.action === "decode") {
          return JSON.stringify({
            status: "OK", module: "DOC", format: name.split(".").pop(), kind: "document", itemCount: 1, warnings: [],
            blocks: [{ type: "paragraph", runs: [{ text: "첫 문단" }] }],
            ...answer,
          });
        }
        converts.push(body);
        return JSON.stringify({ status: "OK", convertedBase64: "", fileName: "x." + name.split(".").pop() });
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "DOC");
  await flushAsyncTurns();
  const result = await harness.sandbox.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(name, new TextEncoder().encode("doc"));
  await flushAsyncTurns(40);
  assert.equal(result.ok, true);
  const save = async () => {
    harness.userClick(studioItem(harness.document, "studio-save"));
    await flushAsyncTurns(40);
    return converts.at(-1);
  };
  return { ...harness, save };
}

test("the defaults of the opened file go back with the save", async () => {
  const t = await open("book.docx", { docDefaults: DEFAULTS });
  const sent = await t.save();
  assert.ok(sent, "a conversion was sent");
  assert.deepEqual(sent.docDefaults, DEFAULTS);
});

test("a file without defaults, and a new document, send none", async () => {
  const t = await open("plain.docx", {});
  const sent = await t.save();
  assert.equal(Object.hasOwn(sent, "docDefaults"), false);
});

test("a defaults object of the wrong shape is dropped on the way in, unknown keys never go back", async () => {
  const hostile = await open("bad.docx", { docDefaults: { ...DEFAULTS, sizeHalfPoints: "22" } });
  assert.equal(Object.hasOwn(await hostile.save(), "docDefaults"), false);
  const extra = await open("extra.docx", { docDefaults: { ...DEFAULTS, evil: "<script>", font: "x".repeat(300) } });
  const sent = (await extra.save()).docDefaults;
  assert.equal(Object.hasOwn(sent, "evil"), false);
  assert.equal(Object.hasOwn(sent, "font"), false, "a name over 256 characters is left out");
  assert.equal(sent.sizeHalfPoints, 22);
});

test("the defaults stay with the document when the tab is saved and restored", async () => {
  const t = await open("keep.docx", { docDefaults: DEFAULTS });
  const editor = t.document.getElementById("doc-editor");
  assert.ok(editor.dataset.eoDefaults, "the editor holds the defaults");
  const sent = await t.save();
  assert.deepEqual(sent.docDefaults, DEFAULTS);
});
