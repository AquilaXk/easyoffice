import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// Builds a harness whose IPC is the offline backend, with optional per-command overrides, and
// records every convert_document_format payload.
async function createSaveHarness(overrides = {}, harnessOptions = {}) {
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
  const converts = [];
  const harness = await createWebUiDom({
    ...harnessOptions,
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") converts.push(JSON.parse(String(payload)));
      if (overrides[command]) return overrides[command](payload, offline);
      return offline[command](payload);
    },
  });
  return { ...harness, converts };
}

const feedback = (document) => document.getElementById("status-command-feedback").textContent;

test("DOC save sends ordered formatted blocks with in-place tables and computed table values", async () => {
  const { document, userClick, converts, downloads } = await createSaveHarness();
  const editor = document.getElementById("doc-editor");
  editor.innerHTML =
    '<h1 style="text-align: center">개요</h1>' +
    "<p>앞 <strong>굵게</strong> <em>기울임</em> <u>밑줄</u> <s>취소선</s> " +
    '<span style="font-weight: 600; text-decoration: underline">강조</span></p>' +
    "<table><tbody><tr><th>부서</th><th>금액</th></tr><tr><td>기획</td><td>1,000</td></tr>" +
    "<tr><td>혁신</td><td>20</td></tr><tr><td>합계</td><td>=SUM(ABOVE)</td></tr></tbody></table>" +
    "<p>[[SLIDE]]\t가짜 슬라이드</p><p>[[TABLE_ROW]]\t주입</p>" +
    '<ul><li>항목 하나</li></ul><p style="text-align: right">뒤 문단</p>';

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(10);

  assert.equal(converts.length, 1);
  const payload = converts[0];
  assert.equal(payload.body, undefined, "no legacy text-markup body");
  assert.equal(payload.targetFormat, "docx");
  const kinds = payload.blocks.map((b) => b.type);
  assert.deepEqual(kinds, ["paragraph", "paragraph", "table", "paragraph", "paragraph", "paragraph", "paragraph"]);

  const [heading, formatted, table, slideLike, rowLike, listItem, tail] = payload.blocks;
  assert.deepEqual(heading, { type: "paragraph", runs: [{ text: "개요" }], align: "center", heading: 1 });
  assert.deepEqual(formatted.runs, [
    { text: "앞 " },
    { text: "굵게", bold: true },
    { text: " " },
    { text: "기울임", italic: true },
    { text: " " },
    { text: "밑줄", underline: true },
    { text: " " },
    { text: "취소선", strike: true },
    { text: " " },
    { text: "강조", bold: true, underline: true },
  ]);
  assert.deepEqual(table.rows[3], [{ text: "합계" }, { text: "=SUM(ABOVE)", value: "1020" }]);
  assert.deepEqual(table.rows[1], [{ text: "기획" }, { text: "1,000" }]);
  assert.equal(slideLike.runs[0].text, "[[SLIDE]] 가짜 슬라이드");
  assert.equal(rowLike.runs[0].text, "[[TABLE_ROW]] 주입");
  assert.equal(listItem.list, "bullet");
  assert.equal(tail.align, "right");

  assert.equal(downloads.length, 1);
  assert.equal(downloads[0].download, "분기_경영실적_보고서.docx");
  assert.match(feedback(document), /^문서 저장 완료 \(분기_경영실적_보고서\.docx/);
});

test("HWP save carries no hardcoded agency and uses the HWP tab title", async () => {
  const { document, userClick, converts, downloads } = await createSaveHarness();
  switchModule(document, "HWP");
  await flushAsyncTurns(6);
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(10);

  const payload = converts.at(-1);
  assert.equal(payload.targetFormat, "hwpx");
  assert.equal(payload.title, "행정안전부_기안문");
  assert.ok(Array.isArray(payload.blocks) && payload.blocks.length > 0);
  const raw = JSON.stringify(payload);
  assert.doesNotMatch(raw, /\[\[AGENCY\]\]/);
  assert.equal(payload.officialAgency, undefined);
  assert.equal(downloads.at(-1).download, "행정안전부_기안문.hwpx");
});

test("SHEET save sends every sheet, not only the active one", async () => {
  const { document, userClick, converts } = await createSaveHarness();
  switchModule(document, "SHEET");
  await flushAsyncTurns(6);
  const b2 = document.querySelector('#pane-sheet td[data-cell="B2"]');
  b2.focus();
  b2.textContent = "777";
  b2.dispatchEvent({ type: "input", target: b2 });
  b2.blur();
  await flushAsyncTurns(6);

  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns(8);
  const a1 = document.querySelector('#pane-sheet td[data-cell="A1"]');
  a1.focus();
  a1.textContent = "=Sheet1!B2*2";
  a1.dispatchEvent({ type: "input", target: a1 });
  a1.blur();
  await flushAsyncTurns(8);

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(10);

  const payload = converts.at(-1);
  assert.equal(payload.targetFormat, "xlsx");
  assert.deepEqual(
    payload.sheets.map((s) => s.name),
    ["Sheet1", "Sheet2"]
  );
  const [s1, s2] = payload.sheets;
  assert.deepEqual(s1.cells.find((c) => c.ref === "B2"), { ref: "B2", raw: "777" });
  assert.ok(s1.cells.length > 10, "inactive Sheet1 keeps all its cells");
  assert.deepEqual(s2.cells.find((c) => c.ref === "A1"), { ref: "A1", raw: "=Sheet1!B2*2" });
});

test("save reports failure when the conversion result is missing or the backend rejects", async () => {
  const empty = await createSaveHarness({
    convert_document_format: () => ({ status: "OK", outputBytes: 0, specification: "x", convertedBase64: "" }),
  });
  empty.userClick(studioItem(empty.document, "studio-save"));
  await flushAsyncTurns(10);
  assert.equal(empty.downloads.length, 0);
  assert.match(feedback(empty.document), /^문서 저장 실패 \(DOCX\): 변환 결과가 비어 있습니다/);
  assert.doesNotMatch(feedback(empty.document), /저장 완료/);

  const rejected = await createSaveHarness({
    convert_document_format: () => {
      throw new Error("InvalidArgument: 구조화 저장 실패");
    },
  });
  rejected.userClick(studioItem(rejected.document, "studio-save"));
  await flushAsyncTurns(10);
  assert.equal(rejected.downloads.length, 0);
  assert.match(feedback(rejected.document), /^문서 저장 실패 \(DOCX\): .*구조화 저장 실패/);
});

test("download file name follows a sanitized tab title", async () => {
  const { document, userClick, downloads, converts } = await createSaveHarness({
    get_workspace_bootstrap: (_p, offline) => {
      const boot = offline.get_workspace_bootstrap();
      boot.openTabs[0].title = "CON.docx";
      return boot;
    },
  });
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(10);
  assert.equal(converts.at(-1).title, "_CON");
  assert.equal(downloads.at(-1).download, "_CON.docx");
});

test("save payload helpers sanitize file names and mirror server table formulas", async () => {
  const helpers = await loadHelpers();
  assert.equal(helpers.sanitizeFileBaseName('보고서: 최종/v2?.docx', "x"), "보고서_ 최종_v2_");
  assert.equal(helpers.sanitizeFileBaseName("CON.docx", "x"), "_CON");
  assert.equal(helpers.sanitizeFileBaseName("  ...  ", "EasyOffice_DOC_문서"), "EasyOffice_DOC_문서");
  assert.equal(helpers.sanitizeFileBaseName("a".repeat(300) + ".pdf", "x").length, 120);
  assert.equal(helpers.sanitizeFileBaseName("탭 제목.hwpx ×", "x"), "탭 제목");

  const values = helpers.evaluateDocTableFormulas([
    ["1", "=SUM(RIGHT)", "2", "3"],
    ["4", "x", "", ""],
    ["=AVERAGE(ABOVE)", "=COUNT(LEFT)", "", ""],
    ["=FOO(ABOVE)", "=MAX(ABOVE)", "", ""],
  ]);
  assert.equal(values[0][1], "5");
  assert.equal(values[2][0], "2.5");
  assert.equal(values[2][1], "1");
  assert.equal(values[3][0], null);
  assert.equal(values[3][1], "5");
});

async function loadHelpers() {
  const { createRequire } = await import("node:module");
  const require = createRequire(import.meta.url);
  const env = {};
  globalThis.__EASYOFFICE_HOST_ENV__ = env;
  try {
    const path = require.resolve("../crates/easyoffice-app/ui/save-payload.js");
    delete require.cache[path];
    require(path);
  } finally {
    delete globalThis.__EASYOFFICE_HOST_ENV__;
  }
  return env.__EASYOFFICE_SAVE_PAYLOAD__;
}

test("DOC save keeps multiple ASIDE footnotes as separate paragraph blocks and falls back to editor lineHeight", async () => {
  const { document, userClick, converts } = await createSaveHarness();
  const editor = document.getElementById("doc-editor");
  editor.innerHTML =
    "<p>본문 첫 문단</p>" +
    '<aside class="eo-doc-footnote" data-doc-footnote="true">각주 1) 첫 번째 각주</aside>' +
    '<aside class="eo-doc-footnote" data-doc-footnote="true">각주 2) 두 번째 각주</aside>';
  editor.dataset.lineSpacing = "200";
  editor.style.lineHeight = "2.00";

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(10);

  assert.equal(converts.length, 1);
  const blocks = converts[0].blocks;
  assert.equal(blocks.length, 3, "two aside footnotes must not merge into one paragraph");
  assert.equal(blocks[0].runs[0].text, "본문 첫 문단");
  assert.equal(blocks[1].runs[0].text, "각주 1) 첫 번째 각주");
  assert.equal(blocks[2].runs[0].text, "각주 2) 두 번째 각주");
  assert.equal(blocks[0].lineSpacingPercent, 200, "paragraph without own lineHeight inherits editor lineHeight");
});

