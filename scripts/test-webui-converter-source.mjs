import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// C3/C4: every converter entry (즉시 변환 검증, Ctrl+S in the converter, the security audit) decides its
// input with one function: the chosen source file, else the open HWP document, else the editor.

const enc = new TextEncoder();

function makeFile(name, bytes) {
  return {
    name,
    size: bytes.length,
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  };
}

const DOC_MODEL = { title: "보고서", blocks: [{ type: "paragraph", runs: [{ text: "개요" }] }] };
const fixtureFile = (name) => makeFile(name, enc.encode("EOFIXTURE:" + JSON.stringify(DOC_MODEL)));

async function createConverterHarness(options = {}) {
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
    testIpc: async (command, payload) => {
      calls.push({ command, payload: String(payload) });
      return options.respond?.(command, payload) ?? offline[command](payload);
    },
  });
  const converts = () => calls.filter((c) => c.command === "convert_document_format").map((c) => JSON.parse(c.payload));
  return { ...harness, calls, converts };
}

async function chooseSourceFile(document, file) {
  const input = document.getElementById("conv-source-file");
  input.files = [file];
  input.dispatchEvent({ type: "change", target: input });
  await flushAsyncTurns(20);
}

test("Ctrl+S in the converter saves the chosen source file, not the default editor", async () => {
  const { document, userClick, converts, downloads } = await createConverterHarness();
  switchModule(document, "CONV");
  await flushAsyncTurns(10);
  await chooseSourceFile(document, fixtureFile("원본.docx"));
  document.getElementById("conv-dst-fmt").value = "md";
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(30);
  const sent = converts().at(-1);
  assert.equal(sent.fileName, "원본.docx");
  assert.ok(sent.sourceBase64.length > 10);
  assert.equal(sent.blocks, undefined, "editor content is not used");
  assert.equal(downloads.at(-1).download, "원본.md");
});

test("the converter's security audit checks the chosen source file", async () => {
  const { document, userClick, converts } = await createConverterHarness();
  switchModule(document, "CONV");
  await flushAsyncTurns(10);
  await chooseSourceFile(document, fixtureFile("원본.docx"));
  userClick(document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns(30);
  const sent = converts().at(-1);
  assert.equal(sent.fileName, "원본.docx");
  assert.ok(sent.sourceBase64.length > 10);
});

test("a conversion that fails after the request shows the reason in the result box and the status line", async () => {
  const { document, userClick } = await createConverterHarness({
    respond: (command) => (command === "convert_document_format" ? { status: "OK", convertedBase64: "" } : undefined),
  });
  switchModule(document, "CONV");
  await flushAsyncTurns(10);
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(30);
  assert.match(document.getElementById("conv-result-box").textContent, /^변환 실패: /);
  assert.match(document.getElementById("status-command-feedback").textContent, /^변환 실패: /);
});

test("resolveConverterInput: chosen file, then the open HWP document, then the editor", async () => {
  const { host, document } = await createWebUiDom();
  const hwpCalls = [];
  const fileSource = { baseName: "고른파일", size: 3, fields: { fileName: "고른파일.docx", sourceBase64: "AAAA" } };
  const hwpSource = { baseName: "열린문서", size: 4, format: "hwpx", fields: { fileName: "열린문서.hwpx", sourceBase64: "BBBB" } };
  let chosen = fileSource;
  let hwp = hwpSource;
  const tools = host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__(
    {
      document, URL, invokeIpc: async () => ({}), getActiveModule: () => "CONV",
      docTabStore: {}, sheetDataStore: {}, slideDeckState: [],
      sheetController: { commitPendingFocusedSheetCell: async () => {} },
      enqueueWorkspaceHistoryOp: (op) => op(), getWindowBridge: () => null,
      getFileOpen: () => ({ getConverterSource: () => chosen, converterSourceProblem: (size) => (size > 100 ? "너무 큽니다" : "") }),
      getHwpSession: () => ({ getConverterSource: async (fmt) => { hwpCalls.push(fmt); return hwp; } }),
    },
    { saveActiveDocTabState() {}, ensureDefaultDocTabStore() {} }
  );

  const file = await tools.resolveConverterInput("docx");
  assert.deepEqual([file.title, file.content.fileName, file.srcFormat], ["고른파일", "고른파일.docx", "docx"]);
  assert.deepEqual(hwpCalls, [], "a chosen file wins before the HWP document is exported");

  chosen = null;
  const open = await tools.resolveConverterInput("hwp");
  assert.deepEqual([open.title, open.content.fileName, open.srcFormat], ["열린문서", "열린문서.hwpx", "hwpx"]);
  assert.deepEqual(hwpCalls, ["hwp"]);

  hwp = null;
  const editor = await tools.resolveConverterInput("docx");
  assert.equal(editor.title, null);
  assert.ok(Array.isArray(editor.content.blocks));
  assert.equal(editor.srcFormat, "docx");

  chosen = { ...fileSource, size: 101 };
  const big = await tools.resolveConverterInput("docx");
  assert.match(big.problem, /너무 큽니다/);
});

test("opening a file in the embedded HWP editor selects its format as the converter source", async () => {
  const { host, document } = await createWebUiDom();
  const select = document.getElementById("conv-src-fmt");
  let result = { ok: true, module: "HWP", format: "hwtx" };
  const fileOpen = host.__EASYOFFICE_CREATE_FILE_OPEN__({
    document, invokeIpc: async () => ({}), getActiveModule: () => "HWP",
    getHwpSession: () => ({ openFromBytes: async () => result }),
  });
  select.value = "docx";
  await fileOpen.openDocumentFromBytes("서식.hwtx", new Uint8Array([1, 2, 3]));
  assert.equal(select.value, "hwtx");

  result = { ok: false, error: "열 수 없습니다" };
  select.value = "docx";
  await fileOpen.openDocumentFromBytes("서식.hwp", new Uint8Array([1, 2, 3]));
  assert.equal(select.value, "docx", "a failed open leaves the converter alone");

  result = { ok: true, module: "DOC", format: "hwp", textOnly: true };
  await fileOpen.openDocumentFromBytes("서식.hwp", new Uint8Array([1, 2, 3]));
  assert.equal(select.value, "docx", "a text-only fallback is not an embedded HWP document");
});

test("saving the PDF Studio sends rotation, watermark and stamp as page fields, not as body text", async () => {
  const { document, userClick, converts } = await createConverterHarness();
  switchModule(document, "PDF");
  await flushAsyncTurns(10);
  document.getElementById("pdf-watermark-input").value = "대외비 사본";
  document.getElementById("pdf-stamp-select").value = "최종본";
  userClick(document.getElementById("btn-pdf-apply"));
  await flushAsyncTurns(10);
  userClick(document.getElementById("btn-pdf-rotate"));
  await flushAsyncTurns(20);
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(30);
  const sent = converts().at(-1);
  assert.equal(sent.targetFormat, "pdf");
  assert.deepEqual(sent.pdf, { rotationDegrees: 90, watermark: "대외비 사본", stamp: "최종본" });
  const body = JSON.stringify(sent.blocks);
  assert.doesNotMatch(body, /워터마크: |결재 스탬프: /);
  assert.match(body, /신청인/, "form fields stay in the body");
});

test("Ctrl+S in the converter shows why the source could not be read instead of failing silently", async () => {
  const { host, document } = await createWebUiDom();
  const tools = host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__(
    {
      document, URL, invokeIpc: async () => ({}), getActiveModule: () => "CONV",
      docTabStore: {}, sheetDataStore: {}, slideDeckState: [],
      sheetController: { commitPendingFocusedSheetCell: async () => {} },
      enqueueWorkspaceHistoryOp: (op) => op(), getWindowBridge: () => null,
      getFileOpen: () => ({ getConverterSource: () => null, converterSourceProblem: () => "" }),
      getHwpSession: () => ({ getConverterSource: async () => { throw new Error("한글 문서를 변환용으로 내보내지 못했습니다: 편집기 응답 없음"); } }),
    },
    { saveActiveDocTabState() {}, ensureDefaultDocTabStore() {} }
  );
  document.getElementById("conv-src-fmt").value = "hwp";
  document.getElementById("conv-dst-fmt").value = "docx";
  assert.equal(await tools.saveActiveModuleDocument(), false);
  assert.match(
    document.getElementById("status-command-feedback").textContent,
    /^문서 저장 실패 \(DOCX\): 원본 파일을 사용할 수 없습니다: 한글 문서를 변환용으로 내보내지 못했습니다/
  );
});

test("the watermark input is limited to the length the engine accepts", async () => {
  const { document } = await createConverterHarness();
  assert.equal(document.getElementById("pdf-watermark-input").getAttribute("maxlength"), "200");
});

test("the PDF Studio takes a phrase, a kind and a color for highlight annotations, in the form's own input shape", async () => {
  const { document } = await createConverterHarness();
  const input = document.getElementById("pdf-highlight-input");
  assert.ok(input.classList.contains("sb-select"), "same input class as the form fields");
  assert.equal(input.getAttribute("maxlength"), "200");
  assert.equal(input.getAttribute("aria-label"), "형광펜·밑줄을 칠할 문구");
  assert.equal(input.getAttribute("title"), "형광펜·밑줄을 칠할 문구");
  assert.equal(input.value, "", "no phrase, no annotation by default");
  // Short one-line labels like "신청인 성명": the grid reads 문구 | 종류 and then 색.
  const grid = document.querySelector(".pdf-acroform-grid");
  const labels = [...grid.querySelectorAll("label")].map((l) => l.textContent.trim().split(/\s*\n\s*/)[0].trim());
  assert.deepEqual(labels.slice(-3), ["주석 문구", "주석 종류", "주석 색"]);
  assert.ok(labels.every((l) => l.length <= 8), labels.join("|"));
  for (const id of ["pdf-highlight-kind", "pdf-highlight-color"]) {
    const select = document.getElementById(id);
    assert.ok(select.classList.contains("sb-combo") && select.classList.contains("native-select-hidden"), id);
  }
  const values = (id) => [...document.getElementById(id).querySelectorAll("option")].map((o) => o.value);
  assert.deepEqual(values("pdf-highlight-kind"), ["highlight", "underline"]);
  assert.ok(values("pdf-highlight-color").every((v) => /^#[0-9A-F]{6}$/.test(v)));
});

test("saving the PDF Studio sends the highlight phrase, kind and color in the pdf object", async () => {
  const { document, userClick, converts } = await createConverterHarness();
  switchModule(document, "PDF");
  await flushAsyncTurns(10);
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(30);
  assert.equal(converts().at(-1).pdf.highlight, undefined, "empty phrase sends nothing");

  document.getElementById("pdf-highlight-input").value = "  핵심 문구 ";
  document.getElementById("pdf-highlight-kind").value = "underline";
  document.getElementById("pdf-highlight-color").value = "#00FFFF";
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(30);
  assert.deepEqual(converts().at(-1).pdf.highlight, { text: "핵심 문구", kind: "underline", color: "#00FFFF" });
});

test("the highlight inputs are part of the PDF Studio's undoable state", async () => {
  const { document, userClick } = await createConverterHarness();
  switchModule(document, "PDF");
  await flushAsyncTurns(10);
  const input = document.getElementById("pdf-highlight-input");
  const kind = document.getElementById("pdf-highlight-kind");
  input.value = "첫 문구";
  kind.value = "underline";
  userClick(document.getElementById("btn-pdf-rotate"));
  await flushAsyncTurns(20);
  input.value = "바뀐 문구";
  kind.value = "highlight";
  userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(30);
  assert.equal(input.value, "첫 문구");
  assert.equal(kind.value, "underline");
});

test("the PII scrub also masks the highlight phrase", async () => {
  const { document, userClick } = await createConverterHarness();
  switchModule(document, "PDF");
  await flushAsyncTurns(10);
  const input = document.getElementById("pdf-highlight-input");
  input.value = "연락처 010-5555-6666";
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(30);
  assert.ok(!input.value.includes("010-5555-6666"), input.value);
  assert.match(input.value, /010-\*\*\*\*-\*\*\*\*/);
});
