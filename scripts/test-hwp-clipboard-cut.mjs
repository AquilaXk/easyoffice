import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import {
  cutAfterWrite,
  textCopyPlan,
  textCutPlan,
  writeTextHtmlWithPlainFallback,
} from "../apps/hwp-studio/src/command/clipboard-cut.ts";
import { readText } from "./test-support/read-text.mjs";

// Cut in the Hangul editor (PD-151): the selection is deleted only after the system clipboard accepted
// the text. Plain Node, no browser: the editor's WASM bridge, input handler and clipboard are fakes.

const bodySelection = {
  start: { sectionIndex: 0, paragraphIndex: 2, charOffset: 1 },
  end: { sectionIndex: 0, paragraphIndex: 2, charOffset: 5 },
};

function createFakes({ selection = bodySelection, write = async () => {}, exportFails = false } = {}) {
  const log = [];
  let current = selection;
  const wasm = {
    getClipboardText: () => "선택한 글",
    copySelection: (...a) => log.push(["copySelection", ...a]),
    exportSelectionHtml: (...a) => {
      log.push(["exportSelectionHtml", ...a]);
      if (exportFails) throw new Error("export failed");
      return "<p>선택한 글</p>";
    },
    copySelectionInCell: (...a) => log.push(["copySelectionInCell", ...a]),
    exportSelectionInCellHtml: () => "<td>셀</td>",
    copySelectionInCellByPath: (...a) => log.push(["copySelectionInCellByPath", ...a]),
    exportSelectionInCellHtmlByPath: () => "<td>중첩</td>",
  };
  const handler = {
    getSelection: () => current,
    performDelete: () => log.push(["performDelete"]),
  };
  const deps = {
    markInternalHtml: (html, text) => `[marked ${html}|${text}]`,
    writeTextHtml: async (text, html) => {
      log.push(["write", text, html]);
      await write(text, html);
    },
  };
  return { log, wasm, handler, deps, select: (next) => { current = next; } };
}

test("the selection is deleted after the clipboard accepted the write, not before", async () => {
  const f = createFakes();
  const plan = textCutPlan(f.wasm, f.handler, f.deps);
  assert.equal(await cutAfterWrite(plan), "cut");
  assert.deepEqual(f.log.map((entry) => entry[0]), ["copySelection", "exportSelectionHtml", "write", "performDelete"]);
  assert.deepEqual(f.log.find((entry) => entry[0] === "write").slice(1), ["선택한 글", "[marked <p>선택한 글</p>|선택한 글]"]);
});

test("a clipboard that refuses the write leaves the text where it is", async () => {
  const f = createFakes({ write: async () => { throw new DOMException("denied", "NotAllowedError"); } });
  assert.equal(await cutAfterWrite(textCutPlan(f.wasm, f.handler, f.deps)), "not-written");
  assert.equal(f.log.some((entry) => entry[0] === "performDelete"), false);
});

test("a synchronous throw from the clipboard also deletes nothing", async () => {
  const f = createFakes();
  f.deps.writeTextHtml = () => { throw new TypeError("clipboard unavailable"); };
  assert.equal(await cutAfterWrite(textCutPlan(f.wasm, f.handler, f.deps)), "not-written");
  assert.equal(f.log.some((entry) => entry[0] === "performDelete"), false);
});

test("a selection that moved while the write was pending is not deleted", async () => {
  let f;
  f = createFakes({
    write: async () => {
      f.select({ start: { sectionIndex: 0, paragraphIndex: 3, charOffset: 0 }, end: { sectionIndex: 0, paragraphIndex: 3, charOffset: 2 } });
    },
  });
  assert.equal(await cutAfterWrite(textCutPlan(f.wasm, f.handler, f.deps)), "selection-changed");
  assert.equal(f.log.some((entry) => entry[0] === "performDelete"), false);
  f = createFakes({ write: async () => f.select(null) });
  assert.equal(await cutAfterWrite(textCutPlan(f.wasm, f.handler, f.deps)), "selection-changed");
});

test("nothing is selected: no plan, no clipboard call", () => {
  const f = createFakes({ selection: null });
  assert.equal(textCutPlan(f.wasm, f.handler, f.deps), null);
  assert.deepEqual(f.log, []);
});

test("an HTML export failure still cuts the plain text", async () => {
  const f = createFakes({ exportFails: true });
  assert.equal(await cutAfterWrite(textCutPlan(f.wasm, f.handler, f.deps)), "cut");
  assert.equal(f.log.find((entry) => entry[0] === "write")[2], "[marked |선택한 글]");
});

test("cell and nested cell selections are copied by the address the editor's copy uses", async () => {
  const cell = createFakes({
    selection: {
      start: { sectionIndex: 1, paragraphIndex: 0, charOffset: 0, parentParaIndex: 4, controlIndex: 0, cellIndex: 2, cellParaIndex: 1 },
      end: { sectionIndex: 1, paragraphIndex: 0, charOffset: 3, parentParaIndex: 4, controlIndex: 0, cellIndex: 2, cellParaIndex: 2 },
    },
  });
  await cutAfterWrite(textCutPlan(cell.wasm, cell.handler, cell.deps));
  assert.deepEqual(cell.log[0], ["copySelectionInCell", 1, 4, 0, 2, 1, 0, 2, 3]);

  const path = [{ controlIndex: 0, cellIndex: 1, cellParaIndex: 0 }, { controlIndex: 2, cellIndex: 3, cellParaIndex: 5 }];
  const nested = createFakes({
    selection: {
      start: { sectionIndex: 0, paragraphIndex: 0, charOffset: 1, parentParaIndex: 7, cellPath: path },
      end: { sectionIndex: 0, paragraphIndex: 0, charOffset: 4, parentParaIndex: 7, cellPath: path },
    },
  });
  await cutAfterWrite(textCutPlan(nested.wasm, nested.handler, nested.deps));
  assert.deepEqual(nested.log[0], ["copySelectionInCellByPath", 0, 7, JSON.stringify(path), 5, 1, 5, 4]);
});

test("writeTextHtmlWithPlainFallback falls back to plain writeText when ClipboardItem write rejects, and textCopyPlan copies without deleting", async () => {
  const calls = [];
  await writeTextHtmlWithPlainFallback(
    "본문 텍스트",
    "<p>본문 텍스트</p>",
    async () => {
      calls.push("rich");
      throw new Error("ClipboardItem not supported");
    },
    async (text) => {
      calls.push(["plain", text]);
    },
  );
  assert.deepEqual(calls, ["rich", ["plain", "본문 텍스트"]]);

  const f = createFakes();
  const copy = textCopyPlan(f.wasm, f.handler, f.deps);
  assert.ok(copy);
  await copy.write();
  assert.deepEqual(f.log.map((entry) => entry[0]), ["copySelection", "exportSelectionHtml", "write"]);
  assert.equal(f.log.some((entry) => entry[0] === "performDelete"), false);
});

test("the editor wires safe cut and safe copy over its own commands, and shows a status message when cut cannot write to clipboard", () => {
  const read = (relative) => readText(fileURLToPath(new URL(relative, import.meta.url)));
  const main = read("../apps/hwp-studio/src/main.ts");
  assert.match(main, /const upstreamCut = registry\.get\('edit:cut'\);\s+if \(upstreamCut\) registry\.register\(safeCutCommand\(upstreamCut\)\);/);
  assert.match(main, /const upstreamCopy = registry\.get\('edit:copy'\);\s+if \(upstreamCopy\) registry\.register\(safeCopyCommand\(upstreamCopy\)\);/);
  assert.ok(main.indexOf("safeCutCommand(upstreamCut)") > main.indexOf("registry.registerAll(\n  chromeMode === 'embed'\n    ? editCommands"), "registered after the editor's own edit commands");
  const glue = read("../apps/hwp-studio/src/command/safe-cut.ts");
  for (const keep of ["inPictureObjectSelection", "inTableObjectSelection", "inCellSelectionMode", "isFormMode", "isInHeaderFooter", "isInFootnote"]) {
    assert.ok(glue.includes(keep), `${keep} cuts keep the editor's own command`);
  }
  assert.match(glue, /base\.execute\(services, params\)/);
  assert.ok(glue.includes("클립보드에 쓰지 못해 선택한 내용을 지우지 않았습니다."), "updates status bar on not-written cut");
  assert.ok(glue.includes("#sb-message"), "writes cut refusal notice to #sb-message");
});

