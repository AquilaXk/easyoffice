import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { pickFile as pickFixtureFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

const INDEX_HTML = fs.readFileSync(
  fileURLToPath(new URL("../crates/easyoffice-app/ui/index.html", import.meta.url)),
  "utf8"
);
const AUTOSAVE_KEY = "easyoffice.workspace.autosave.v1";
const enc = new TextEncoder();

function makeFile(name, bytes, size = bytes.length) {
  return {
    name,
    size,
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  };
}

// The offline backend cannot parse real containers: binary files carry the decoded model.
function fixtureFile(name, model) {
  return makeFile(name, enc.encode("EOFIXTURE:" + JSON.stringify(model)));
}

function memoryStorage() {
  const map = new Map();
  return {
    map,
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
  };
}

async function createOpenHarness(options = {}) {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const calls = [];
  const harness = await createWorkspaceHarness({
    ...options,
    testIpc: async (command, payload) => {
      calls.push({ command, payload: String(payload) });
      return offline[command](payload);
    },
  });
  const converts = () =>
    calls
      .filter((c) => c.command === "convert_document_format")
      .map((c) => JSON.parse(c.payload));
  return { ...harness, calls, converts };
}

const pickFile = (document, file, turns = 40) => pickFixtureFile(document, file, turns);

const feedback = (document) => document.getElementById("status-command-feedback").textContent;
const activeModule = (document) => document.getElementById("app").dataset.module;
// Document names are owned by the window bridge (they feed the native window title).
const tabTitle = (document, mod) =>
  document.defaultView.__EASYOFFICE_WINDOW_BRIDGE__.instance.getDocumentName(mod);

const DOC_MODEL = {
  title: "보고서",
  blocks: [
    { type: "paragraph", runs: [{ text: "개요" }], align: "center", heading: 1 },
    {
      type: "paragraph",
      runs: [{ text: "앞 " }, { text: "굵게", bold: true }, { text: " 기울임", italic: true, underline: true }],
    },
    { type: "table", rows: [[{ text: "부서" }, { text: "금액" }], [{ text: "기획" }, { text: "1,000" }]] },
    { type: "paragraph", runs: [{ text: "항목" }], list: "bullet" },
  ],
  warnings: ["그림은 가져오지 않았습니다."],
};

const SHEET_MODEL = {
  sheets: [
    {
      name: "매출",
      cells: [
        { ref: "A1", raw: "월" },
        { ref: "B1", raw: "금액" },
        { ref: "A2", raw: "1월" },
        { ref: "B2", raw: "100" },
        { ref: "B3", raw: "250" },
        { ref: "B4", raw: "=SUM(B2:B3)" },
      ],
    },
    { name: "요약", cells: [{ ref: "A1", raw: "=매출!B4" }, { ref: "B2", raw: "메모" }] },
  ],
};

test("Ctrl+O is in the single shortcut table, labelled in the File menu, and opens the picker", async () => {
  const { document, sandbox } = await createOpenHarness();
  const bindings = sandbox.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS;
  assert.ok(bindings.some((b) => b.keys === "Ctrl+O" && b.action === "file-open"));
  const item = document.querySelector('.md-item[data-file-open="true"]');
  assert.ok(item, "File menu has an open item");
  assert.match(item.textContent, /열기/);
  assert.match(item.textContent, /Ctrl\+O/);

  const input = document.getElementById("file-open-input");
  let picks = 0;
  input.addEventListener("click", () => {
    picks += 1;
  });
  const ev = sandbox.dispatchWindowKeydown({ key: "o", ctrlKey: true });
  assert.equal(ev.defaultPrevented, true, "browser's own open dialog is suppressed");
  assert.equal(picks, 1);
  item.click();
  assert.equal(picks, 2);
});

test("the hidden input accepts exactly the supported extensions", async () => {
  const { sandbox } = await createOpenHarness();
  const exts = sandbox.__EASYOFFICE_FILE_OPEN_UTILS__.SUPPORTED_EXTENSIONS;
  const inputs = [...INDEX_HTML.matchAll(/accept="([^"]+)"/g)].map((m) => m[1]);
  assert.equal(inputs.length, 2, "workspace picker and converter picker");
  for (const accept of inputs) {
    const byName = (a, b) => a.localeCompare(b);
    assert.deepEqual(accept.split(",").map((e) => e.replace(/^\./, "")).sort(byName), [...exts].sort(byName));
  }
});

test("opening a DOCX-style model loads formatted blocks into the DOC module and names the tab", async () => {
  const { document, calls } = await createOpenHarness();
  await pickFile(document, fixtureFile("보고서.docx", DOC_MODEL));

  assert.equal(activeModule(document), "DOC");
  const editor = document.getElementById("doc-editor");
  assert.equal(editor.querySelector("h1").textContent, "개요");
  assert.equal(editor.querySelector("h1").style.textAlign, "center");
  assert.equal(editor.querySelector("strong").textContent, "굵게");
  assert.ok(editor.querySelector("em u") || editor.querySelector("u em"), "italic+underline run");
  assert.equal(editor.querySelectorAll("table tr").length, 2, "table stays in the document");
  assert.equal(editor.querySelector("ul li").textContent, "항목");
  assert.equal(editor.querySelector("h2"), null, "previous default content is replaced");
  assert.equal(tabTitle(document, "DOC"), "보고서.docx");
  assert.match(feedback(document), /^열기 완료: 보고서\.docx \(DOCX → 문서 모듈/);
  assert.match(feedback(document), /그림은 가져오지 않았습니다/);

  const decode = JSON.parse(calls.find((c) => c.command === "convert_document_format").payload);
  assert.equal(decode.action, "decode");
  assert.equal(decode.fileName, "보고서.docx");
  assert.deepEqual([decode.maxRows, decode.maxCols], [1048576, 16384]);
  assert.ok(decode.sourceBase64.length > 10);
});

test("open then save round-trips the structured blocks and names the download after the file", async () => {
  const { document, userClick, converts, downloads } = await createOpenHarness();
  await pickFile(document, fixtureFile("보고서.docx", DOC_MODEL));
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(20);

  const save = converts().at(-1);
  assert.equal(save.targetFormat, "docx");
  assert.deepEqual(save.blocks, DOC_MODEL.blocks);
  assert.equal(save.title, "보고서");
  assert.equal(downloads.at(-1).download, "보고서.docx");
});

test("HWP-family, slide, PDF and sheet files are routed to their own modules", async () => {
  const { document } = await createOpenHarness();

  await pickFile(document, fixtureFile("기안문.hwpx", { blocks: [{ type: "paragraph", runs: [{ text: "한글 본문" }] }] }));
  assert.equal(activeModule(document), "HWP");
  assert.equal(document.getElementById("doc-editor").querySelector("p").textContent, "한글 본문");
  assert.equal(tabTitle(document, "HWP"), "기안문.hwpx");

  await pickFile(
    document,
    fixtureFile("발표.pptx", {
      slides: [
        { title: "첫 장", bullets: ["가", "나"], notes: "노트 하나" },
        { title: "둘째 장", bullets: [], notes: "" },
      ],
    })
  );
  assert.equal(activeModule(document), "SLIDE");
  assert.equal(document.getElementById("slide-title-zone").textContent, "첫 장");
  assert.equal(document.getElementById("slide-presenter-notes").value, "노트 하나");
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 2);
  assert.equal(tabTitle(document, "SLIDE"), "발표.pptx");

  await pickFile(
    document,
    fixtureFile("계약서.pdf", { title: "계약서", blocks: [{ type: "paragraph", runs: [{ text: "제1조 목적" }] }] })
  );
  assert.equal(activeModule(document), "PDF");
  assert.equal(document.querySelector("#pdf-page-card h3").textContent, "계약서");
  assert.match(document.getElementById("pdf-page-card").textContent, /제1조 목적/);
  assert.equal(tabTitle(document, "PDF"), "계약서.pdf");

  await pickFile(document, fixtureFile("통계.xlsx", SHEET_MODEL));
  assert.equal(activeModule(document), "SHEET");
  assert.equal(tabTitle(document, "SHEET"), "통계.xlsx");
});

test("a Pages file opens into the DOC module and shows what was skipped in the status bar", async () => {
  const { document, calls } = await createOpenHarness();
  await pickFile(
    document,
    fixtureFile("메모.pages", {
      warnings: ["가져오지 않은 요소: 그림·도형 등 개체(형식 3005) 2개"],
      blocks: [{ type: "paragraph", runs: [{ text: "애플 본문", bold: true }] }],
    })
  );
  assert.equal(activeModule(document), "DOC");
  assert.equal(document.getElementById("doc-editor").querySelector("p").textContent, "애플 본문");
  assert.equal(tabTitle(document, "DOC"), "메모.pages");
  assert.match(feedback(document), /^열기 완료: 메모\.pages/);
  assert.match(feedback(document), /참고: .*그림·도형 등 개체\(형식 3005\) 2개/);
  const decode = JSON.parse(calls.find((c) => c.command === "convert_document_format").payload);
  assert.equal(decode.fileName, "메모.pages");
});

test("the converter offers Pages as a source format only", async () => {
  const src = /id="conv-src-fmt"[\s\S]*?<\/select>/.exec(INDEX_HTML)[0];
  const dst = /id="conv-dst-fmt"[\s\S]*?<\/select>/.exec(INDEX_HTML)[0];
  assert.match(src, /value="pages"/);
  assert.doesNotMatch(dst, /value="pages"/);
});
test("XLSX sheets keep every sheet name, values and formulas, and save back unchanged", async () => {
  const { document, userClick, converts } = await createOpenHarness();
  await pickFile(document, fixtureFile("통계.xlsx", SHEET_MODEL));

  const tabs = [...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => b.dataset.sheetTab);
  assert.deepEqual(tabs, ["매출", "요약"], "default Sheet1/Sheet2 tabs are gone");
  const b4 = document.querySelector('#pane-sheet td[data-cell="B4"]');
  assert.equal(b4.dataset.formula, "=SUM(B2:B3)");
  assert.equal(b4.textContent, "350");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "월");

  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="요약"]'));
  await flushAsyncTurns(20);
  const a1 = document.querySelector('#pane-sheet td[data-cell="A1"]');
  assert.equal(a1.dataset.formula, "=매출!B4");
  assert.equal(a1.textContent, "350", "cross-sheet formula evaluates against the imported sheet");

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(20);
  const save = converts().at(-1);
  assert.deepEqual(save.sheets.map((s) => s.name), ["매출", "요약"]);
  assert.deepEqual(save.sheets[0].cells.find((c) => c.ref === "B4"), { ref: "B4", raw: "=SUM(B2:B3)" });
  assert.deepEqual(save.sheets[1].cells.find((c) => c.ref === "B2"), { ref: "B2", raw: "메모" });
});

test("CSV with a BOM opens as a sheet", async () => {
  const { document } = await createOpenHarness();
  const bytes = enc.encode("\uFEFF이름,값\r\n가,1\r\n나,2\r\n");
  await pickFile(document, makeFile("데이터.csv", bytes));
  assert.equal(activeModule(document), "SHEET");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "이름");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="B3"]').textContent, "2");
});

test("dropping a file on the workspace opens it; non-file drags are ignored; extra files are reported", async () => {
  const { document } = await createOpenHarness();
  const app = document.querySelector(".canvas-stage");
  // dispatchEvent returns false when a listener called preventDefault (the drop is accepted).
  assert.equal(app.dispatchEvent({ type: "dragover", dataTransfer: { types: ["Files"], files: [] } }), false);
  assert.ok(app.classList.contains("eo-drop-active"), "drop target is highlighted");
  assert.equal(app.dispatchEvent({ type: "dragover", dataTransfer: { types: ["text/plain"], files: [] } }), true);

  app.dispatchEvent({ type: "drop", dataTransfer: { types: ["text/plain"], files: [] } });
  await flushAsyncTurns(10);
  assert.doesNotMatch(feedback(document), /열기 완료/);

  const first = fixtureFile("첫째.docx", DOC_MODEL);
  const second = fixtureFile("둘째.docx", DOC_MODEL);
  app.dispatchEvent({ type: "drop", dataTransfer: { types: ["Files"], files: [first, second] } });
  await flushAsyncTurns(40);
  assert.equal(tabTitle(document, "DOC"), "첫째.docx");
  assert.match(feedback(document), /첫 번째 파일만/);
  assert.ok(!app.classList.contains("eo-drop-active"));
});

test("the browser's drop behaviour is off for the whole document and only the editing area shows the drop outline", async () => {
  const { document } = await createOpenHarness();
  for (const type of ["dragover", "drop"]) {
    const ev = { type, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
    document.dispatchEvent(ev);
    assert.equal(ev.defaultPrevented, true, `${type} on the document is cancelled`);
  }
  const shell = document.getElementById("app");
  shell.dispatchEvent({ type: "dragover", dataTransfer: { types: ["Files"], files: [] } });
  assert.ok(!shell.classList.contains("eo-drop-active"), "the window is not outlined");
  const area = document.querySelector(".canvas-stage");
  area.dispatchEvent({ type: "dragover", dataTransfer: { types: ["Files"], files: [] } });
  assert.ok(area.classList.contains("eo-drop-active"), "the editing area is outlined");
  area.dispatchEvent({ type: "dragleave" });
  assert.ok(!area.classList.contains("eo-drop-active"));
});

test("opening over unsaved edits asks first; declining changes nothing, accepting replaces it", async () => {
  const asked = [];
  let answer = false;
  const { document } = await createOpenHarness({
    confirm: (msg) => {
      asked.push(msg);
      return answer;
    },
  });
  // A clean module opens without a prompt.
  await pickFile(document, fixtureFile("깨끗.hwpx", { blocks: [{ type: "paragraph", runs: [{ text: "한글" }] }] }));
  assert.equal(asked.length, 0);

  switchModule(document, "DOC");
  await flushAsyncTurns(10);
  const editor = document.getElementById("doc-editor");
  editor.innerHTML = "<p>내가 쓴 내용</p>";
  editor.dispatchEvent({ type: "input", target: editor });
  await flushAsyncTurns(10);

  await pickFile(document, fixtureFile("새문서.docx", DOC_MODEL));
  assert.equal(asked.length, 1);
  assert.match(asked[0], /저장하지 않은 편집 내용/);
  assert.match(asked[0], /새문서\.docx/);
  assert.equal(editor.textContent, "내가 쓴 내용", "declined: untouched");
  assert.notEqual(tabTitle(document, "DOC"), "새문서.docx");
  assert.match(feedback(document), /취소/);

  answer = true;
  await pickFile(document, fixtureFile("새문서.docx", DOC_MODEL));
  assert.equal(asked.length, 2);
  assert.equal(editor.querySelector("h1").textContent, "개요");
  assert.equal(tabTitle(document, "DOC"), "새문서.docx");

  // The freshly opened file is not "unsaved edits": opening again does not prompt.
  await pickFile(document, fixtureFile("또다른.docx", DOC_MODEL));
  assert.equal(asked.length, 2);
});

test("a file over the size limit is refused before any backend call and changes nothing", async () => {
  const { document, calls } = await createOpenHarness();
  const before = document.getElementById("doc-editor").innerHTML;
  const huge = makeFile("거대.docx", new Uint8Array(4), 11 * 1024 * 1024);
  await pickFile(document, huge);
  assert.match(feedback(document), /파일 열기 실패: 파일이 너무 큽니다/);
  assert.match(feedback(document), /10 MiB/);
  assert.equal(calls.filter((c) => c.command === "convert_document_format").length, 0);
  assert.equal(document.getElementById("doc-editor").innerHTML, before);

  await pickFile(document, makeFile("빈.docx", new Uint8Array(0)));
  assert.match(feedback(document), /빈 파일은 열 수 없습니다/);
});

test("corrupt and unsupported files show the backend error and change nothing", async () => {
  const { document } = await createOpenHarness();
  const before = document.getElementById("doc-editor").innerHTML;
  const convBefore = document.getElementById("conv-result-box").textContent;

  await pickFile(document, makeFile("깨짐.docx", enc.encode("이건 zip이 아닙니다")));
  assert.match(feedback(document), /^파일 열기 실패: 깨짐\.docx — .*무결성/);
  assert.equal(document.getElementById("doc-editor").innerHTML, before);
  assert.equal(tabTitle(document, "DOC"), "분기_경영실적_보고서.docx");
  assert.equal(document.getElementById("conv-result-box").textContent, convBefore, "converter panel untouched");

  await pickFile(document, makeFile("프로그램.exe", enc.encode("MZ")));
  assert.match(feedback(document), /^파일 열기 실패: 프로그램\.exe — .*지원하지 않는 확장자/);
  assert.equal(document.getElementById("doc-editor").innerHTML, before);
});

test("opening is one undoable workspace state (and redoable)", async () => {
  const { document, userClick } = await createOpenHarness();
  const editor = document.getElementById("doc-editor");
  const before = editor.textContent;
  await pickFile(document, fixtureFile("보고서.docx", DOC_MODEL));
  assert.equal(editor.querySelector("h1").textContent, "개요");

  userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(30);
  assert.equal(editor.textContent, before, "default document is back");
  assert.equal(tabTitle(document, "DOC"), "분기_경영실적_보고서.docx", "tab title is part of the undone state");

  userClick(studioItem(document, "studio-redo"));
  await flushAsyncTurns(30);
  assert.equal(editor.querySelector("h1").textContent, "개요");
  assert.equal(tabTitle(document, "DOC"), "보고서.docx");

  // Undo of a sheet import brings the default workbook tabs back.
  await pickFile(document, fixtureFile("통계.xlsx", SHEET_MODEL));
  userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(30);
  const tabs = [...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => b.dataset.sheetTab);
  assert.deepEqual(tabs, ["Sheet1", "Sheet2"]);
  assert.equal(tabTitle(document, "SHEET"), "예산집행_총괄표.xlsx");
});

test("autosave captures an opened file including its tab title", async () => {
  const storage = memoryStorage();
  const first = await createOpenHarness({ localStorage: storage });
  await pickFile(first.document, fixtureFile("자동저장.docx", DOC_MODEL));
  first.advanceTimers(5000);
  await flushAsyncTurns(20);
  const record = JSON.parse(storage.getItem(AUTOSAVE_KEY));
  assert.equal(record.snapshot.tabTitles.DOC, "자동저장.docx");
  assert.match(JSON.stringify(record.snapshot.docTabs.DOC), /굵게/);

  const second = await createOpenHarness({ localStorage: storage });
  assert.equal(second.document.getElementById("doc-editor").querySelector("h1").textContent, "개요");
  assert.equal(tabTitle(second.document, "DOC"), "자동저장.docx", "title survives the tab strip rebuild");
});

test("openDocumentFromBytes is the shared entry point for a desktop shell", async () => {
  const { document, sandbox } = await createOpenHarness();
  const api = sandbox.__EASYOFFICE_WORKSPACE_API__;
  assert.equal(typeof api.openDocumentFromBytes, "function");
  const bytes = enc.encode("EOFIXTURE:" + JSON.stringify(DOC_MODEL));
  const result = await api.openDocumentFromBytes(String.raw`C:\Users\me\바탕화면\공유.docx`, bytes);
  assert.deepEqual([result.ok, result.module, result.title], [true, "DOC", "공유.docx"]);
  assert.equal(tabTitle(document, "DOC"), "공유.docx");
  const bad = await api.openDocumentFromBytes("x.docx", new Uint8Array(0));
  assert.equal(bad.ok, false);
});

test("converter converts the chosen source file's bytes, and keeps editor conversion without one", async () => {
  const { document, userClick, converts, downloads } = await createOpenHarness();
  switchModule(document, "CONV");
  await flushAsyncTurns(10);

  // No file: existing behaviour (structured editor content, fixed title).
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(20);
  const editorConvert = converts().at(-1);
  assert.ok(Array.isArray(editorConvert.blocks));
  assert.equal(editorConvert.sourceBase64, undefined);
  assert.equal(downloads.at(-1).download, "통합_변환_문서.hwpx");

  const input = document.getElementById("conv-source-file");
  input.files = [fixtureFile("원본.docx", DOC_MODEL)];
  input.dispatchEvent({ type: "change", target: input });
  await flushAsyncTurns(20);
  assert.match(document.getElementById("conv-source-name").textContent, /원본\.docx/);
  assert.equal(document.getElementById("conv-src-fmt").value, "docx");

  document.getElementById("conv-dst-fmt").value = "md";
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(20);
  const fileConvert = converts().at(-1);
  assert.equal(fileConvert.blocks, undefined, "editor content is not used");
  assert.equal(fileConvert.fileName, "원본.docx");
  assert.ok(fileConvert.sourceBase64.length > 10);
  assert.equal(fileConvert.title, "원본");
  assert.equal(downloads.at(-1).download, "원본.md");
  assert.match(document.getElementById("conv-result-box").textContent, /^\[OK\] DOCX/);

  userClick(document.getElementById("btn-conv-clear-source"));
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(20);
  assert.ok(Array.isArray(converts().at(-1).blocks), "clearing the file restores editor conversion");
});

test("converter rejects an oversized or corrupt source without converting", async () => {
  const { document, userClick, converts } = await createOpenHarness();
  switchModule(document, "CONV");
  await flushAsyncTurns(10);
  const input = document.getElementById("conv-source-file");
  input.files = [makeFile("거대.docx", new Uint8Array(4), 11 * 1024 * 1024)];
  input.dispatchEvent({ type: "change", target: input });
  await flushAsyncTurns(10);
  assert.match(document.getElementById("conv-result-box").textContent, /원본 파일을 사용할 수 없습니다: 파일이 너무 큽니다/);
  assert.equal(document.getElementById("btn-conv-clear-source").hidden, true);

  input.files = [makeFile("깨짐.docx", enc.encode("not a zip"))];
  input.dispatchEvent({ type: "change", target: input });
  await flushAsyncTurns(10);
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(20);
  assert.match(document.getElementById("conv-result-box").textContent, /요청 거부|무결성/);
  assert.ok(converts().some((c) => c.sourceBase64));
});

test("dropping a file while the converter is active selects it as the conversion source", async () => {
  const { document } = await createOpenHarness();
  switchModule(document, "CONV");
  await flushAsyncTurns(10);
  const app = document.querySelector(".canvas-stage");
  app.dispatchEvent({ type: "drop", dataTransfer: { types: ["Files"], files: [fixtureFile("끌어온.docx", DOC_MODEL)] } });
  await flushAsyncTurns(20);
  assert.match(document.getElementById("conv-source-name").textContent, /끌어온\.docx/);
  assert.equal(tabTitle(document, "DOC"), "분기_경영실적_보고서.docx", "workspace documents are untouched");
});

// 1x1 PNG (valid structure); the offline backend passes the decoded model through unchanged.
const TINY_PNG_B64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

const RICH_MODEL = {
  title: "그림 문서",
  blocks: [
    {
      type: "paragraph",
      runs: [{ text: "X" }, { text: "2", script: "superscript" }, { text: " H" }, { text: "2", script: "subscript" }],
      leftIndentTwips: 720,
      firstLineIndentTwips: 361,
      lineSpacingPercent: 200,
    },
    {
      type: "image", format: "png", base64: TINY_PNG_B64, widthPx: 1, heightPx: 1,
      widthEmu: 9525, heightEmu: 9525, alt: "작은 \"그림\" <b>",
    },
    { type: "paragraph", runs: [{ text: "끝" }], heading: 2 },
  ],
};

// The pixel size is informational on open; a save sends the bytes and the display size only.
const SAVED_RICH_BLOCKS = RICH_MODEL.blocks.map(({ widthPx, heightPx, ...rest }) => rest);

test("pictures, super/subscripts, indents and line spacing show in the editor", async () => {
  const { document } = await createOpenHarness();
  await pickFile(document, fixtureFile("그림.docx", RICH_MODEL));
  const editor = document.getElementById("doc-editor");

  const img = editor.querySelector("img.doc-inline-image");
  assert.ok(img, "picture block becomes an <img>");
  assert.equal(img.getAttribute("alt"), '작은 "그림" <b>');
  assert.equal(img.getAttribute("src"), "blob:easyoffice-test", "bytes are shown through a blob: URL");
  assert.equal(img.getAttribute("contenteditable"), "false");

  assert.equal(editor.querySelector("sup").textContent, "2");
  assert.equal(editor.querySelector("sub").textContent, "2");
  const para = editor.querySelector("p");
  assert.equal(para.style.paddingLeft, "48px");
  assert.equal(para.style.lineHeight, "2");
  assert.ok(para.style.textIndent.endsWith("px"));
  assert.equal(editor.querySelector("h2").textContent, "끝");
});

test("saving an opened document sends the picture bytes, scripts, indents and spacing unchanged", async () => {
  const { document, userClick, converts } = await createOpenHarness();
  await pickFile(document, fixtureFile("그림.docx", RICH_MODEL));
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(20);
  const save = converts().at(-1);
  assert.deepEqual(save.blocks, SAVED_RICH_BLOCKS);
});

test("an <img> without stored bytes degrades to its alternative text on save", async () => {
  const { document, userClick, converts } = await createOpenHarness();
  await pickFile(document, fixtureFile("그림.docx", RICH_MODEL));
  const editor = document.getElementById("doc-editor");
  editor.querySelector("img").dataset.eoImageId = "img-missing";
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(20);
  const blocks = converts().at(-1).blocks;
  assert.deepEqual(blocks[1], { type: "paragraph", runs: [{ text: '작은 "그림" <b>' }] });
});

test("undo and redo of an opened picture keep it displayable and savable", async () => {
  const { document, userClick, converts } = await createOpenHarness();
  await pickFile(document, fixtureFile("그림.docx", RICH_MODEL));
  const editor = document.getElementById("doc-editor");
  assert.ok(editor.querySelector("img"));
  const snapshot = document.defaultView.__EASYOFFICE_SHEET_UTILS__.serializeDomChildren(editor);
  document.defaultView.__EASYOFFICE_SHEET_UTILS__.restoreDomChildren(editor, snapshot, document);
  const img = editor.querySelector("img");
  assert.equal(img.getAttribute("src"), "blob:easyoffice-test");
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(20);
  assert.deepEqual(converts().at(-1).blocks, SAVED_RICH_BLOCKS);
});
