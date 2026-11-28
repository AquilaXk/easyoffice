import assert from "node:assert/strict";
import test from "node:test";
import { applyCopilotEdits, parseApplyCopilotEditsParams } from "../apps/hwp-studio/src/embed/copilot-bridge.ts";

// The Copilot's 아래에 삽입 and 문서 끝에 추가 on paragraphs that hold an inline control (a table, a picture). The fake editor counts
// offsets the way the real one does: `getParagraphLength` is the text length, while `splitParagraph` takes a cursor offset that
// counts each control as one more place, and a control at or after that offset moves into the new paragraph (found in a real
// WebKit run on a sample whose last paragraph holds a table).

function controlWasm(initial) {
  // A paragraph is `{ text, controls }`: `controls` are the text positions of its controls (a control sits before the character there).
  let paragraphs = initial.map((p) => ({ text: p.text, controls: [...(p.controls ?? [])] }));
  const logicalPositions = (p) => p.controls.map((at, i) => at + i);
  const wasm = {
    pageCount: 1,
    documentGeneration: 1,
    getSectionCount: () => 1,
    getParagraphCount: () => paragraphs.length,
    getParagraphLength: (_s, i) => paragraphs[i].text.length,
    getTextRange: (_s, i, off, cnt) => paragraphs[i].text.slice(off, off + cnt),
    getControlTextPositions: (_s, i) => [...paragraphs[i].controls],
    replaceText: (_s, i, off, len, next) => {
      const p = paragraphs[i];
      p.text = p.text.slice(0, off) + next + p.text.slice(off + len);
      return { ok: true };
    },
    insertText: (_s, i, off, txt) => {
      const p = paragraphs[i];
      p.text = p.text.slice(0, off) + txt + p.text.slice(off);
    },
    deleteText: () => {},
    mergeParagraph: () => {},
    splitParagraph: (_s, i, logicalOffset) => {
      const p = paragraphs[i];
      const logical = logicalPositions(p);
      const staying = logical.filter((pos) => pos < logicalOffset).length;
      const textSplit = Math.min(p.text.length, logicalOffset - staying);
      const tail = { text: p.text.slice(textSplit), controls: p.controls.slice(staying).map((at) => at - textSplit) };
      p.text = p.text.slice(0, textSplit);
      p.controls = p.controls.slice(0, staying);
      paragraphs.splice(i + 1, 0, tail);
    },
  };
  const input = {
    getCursorPosition: () => ({ sectionIndex: 0, paragraphIndex: 0, charOffset: 0 }),
    getSelection: () => null,
    executeOperation: (desc) => {
      const before = structuredClone(paragraphs);
      try {
        desc.operation(wasm);
        wasm.documentGeneration += 1;
      } catch (err) {
        paragraphs = before;
        throw err;
      }
    },
  };
  return { wasm, input, view: () => paragraphs.map((p) => `${p.controls.length > 0 ? "[표]" : ""}${p.text}`) };
}

const edit = (steps) => parseApplyCopilotEditsParams({ steps });

test("아래에 삽입 under a paragraph that ends with a control keeps the control in that paragraph", () => {
  const { wasm, input, view } = controlWasm([{ text: "머리" }, { text: "", controls: [0] }]);
  applyCopilotEdits(wasm, input, edit([{ kind: "insert_after", index: 1, texts: ["표 아래 글"] }]));
  assert.deepEqual(view(), ["머리", "[표]", "표 아래 글"]);
});

test("아래에 삽입 under text followed by a control, and under a control before text, leaves both paragraphs whole", () => {
  const { wasm, input, view } = controlWasm([{ text: "앞글", controls: [2] }, { text: "표 앞 글", controls: [0] }]);
  applyCopilotEdits(wasm, input, edit([{ kind: "insert_after", index: 1, texts: ["둘째 아래"] }, { kind: "insert_after", index: 0, texts: ["첫째 아래\n한 줄 더"] }]));
  assert.deepEqual(view(), ["[표]앞글", "첫째 아래", "한 줄 더", "[표]표 앞 글", "둘째 아래"]);
});

test("문서 끝에 추가 after a last paragraph that holds a control keeps the control where it was", () => {
  const { wasm, input, view } = controlWasm([{ text: "본문" }, { text: "", controls: [0] }]);
  applyCopilotEdits(wasm, input, edit([{ kind: "append", texts: ["끝 문단", "하나 더"] }]));
  assert.deepEqual(view(), ["본문", "[표]", "끝 문단", "하나 더"]);
});

test("a paragraph without controls splits at its text length as before", () => {
  const { wasm, input, view } = controlWasm([{ text: "첫째" }, { text: "둘째" }]);
  applyCopilotEdits(wasm, input, edit([{ kind: "insert_after", index: 0, texts: ["사이 글"] }]));
  assert.deepEqual(view(), ["첫째", "사이 글", "둘째"]);
});
