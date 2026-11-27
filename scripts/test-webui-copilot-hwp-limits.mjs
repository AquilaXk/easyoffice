import assert from "node:assert/strict";
import test from "node:test";
import { open, answer } from "./test-support/copilot-open.mjs";

// A range of the HWP copilot preview and apply stays inside the paragraphs the editor reported: an `endIndex` of 2e9 must
// not make the preview loop run for billions of turns, and an oversized operation list is refused (PD-262).

const PARAGRAPHS = ["첫째", "둘째", "셋째"];

async function openExtOps() {
  const h = await open({ module: "DOC", run: () => answer([]) });
  const calls = [];
  const hwpHost = {
    usesLegacy: () => false,
    getStatus: () => "ready",
    onChange: () => {},
    onStatus: () => {},
    call: async (method, params) => {
      calls.push({ method, params });
      return {
        documentGeneration: 1,
        pageCount: 1,
        totalParagraphs: PARAGRAPHS.length,
        paragraphs: PARAGRAPHS.map((text, index) => ({ index, section: 0, paragraph: index, text, hasControls: false })),
        selection: { hasSelection: false },
      };
    },
  };
  const ops = h.host.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__({ document: h.document, getHwpHost: () => hwpHost });
  await ops.refreshHwpSnapshot();
  return { ops, calls };
}

const change = (kind, label, before, after) => ({ kind, label, before, after });

test("a preview range past the end of the document is held inside it and answers at once", async () => {
  const { ops } = await openExtOps();
  const entries = [];
  const started = Date.now();
  ops.hwpEntries({ kind: "doc.replace_paragraphs", startIndex: 0, endIndex: 2e9, blocks: [{ text: "새 글" }] }, entries, change);
  assert.ok(Date.now() - started < 1000, "no loop over two billion paragraphs");
  assert.equal(entries.length, 1);
  assert.equal(entries[0].label, "문단 1~3");
  assert.equal(entries[0].before, PARAGRAPHS.join("\n"));
});

test("a start past the end of the document is held on the last paragraph", async () => {
  const { ops } = await openExtOps();
  const entries = [];
  ops.hwpEntries({ kind: "doc.replace_paragraphs", startIndex: 2e9, endIndex: 2e9, blocks: [{ text: "새 글" }] }, entries, change);
  assert.equal(entries[0].label, "문단 3");
  assert.equal(entries[0].before, "셋째");
});

test("applying a range past the end sends the clamped range to the editor", async () => {
  const { ops, calls } = await openExtOps();
  await ops.applyHwpOperations([{ kind: "doc.replace_paragraphs", startIndex: 1, endIndex: 2e9, blocks: [{ text: "새 글" }] }]);
  const edit = calls.find((c) => c.method === "applyCopilotEdits");
  assert.deepEqual(
    edit.params.steps.map((s) => [s.kind, s.startIndex, s.endIndex]),
    [["replace_range", 1, 2]],
  );
});

test("more operations or paragraphs than the limits are refused before the editor is asked", async () => {
  const { ops, calls } = await openExtOps();
  const many = Array.from({ length: 65 }, () => ({ kind: "doc.append", text: "x" }));
  await assert.rejects(() => ops.applyHwpOperations(many), /작업 개수/);
  const replacements = Array.from({ length: 2049 }, (_, index) => ({ index: index % 3, text: "x" }));
  await assert.rejects(() => ops.applyHwpOperations([{ kind: "doc.replace_paragraphs", replacements }]), /문단 개수/);
  const blocks = Array.from({ length: 2049 }, () => ({ text: "x" }));
  await assert.rejects(() => ops.applyHwpOperations([{ kind: "doc.append", blocks }]), /문단 개수/);
  assert.equal(calls.some((c) => c.method === "applyCopilotEdits"), false);
});
