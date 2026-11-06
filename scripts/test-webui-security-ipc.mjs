import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";

const VECTORS_PATH = fileURLToPath(
  new URL("../fixtures/pii-scrub-vectors.json", import.meta.url)
);

test("disconnected host refuses IPC fail-closed without mutating DOC, SHEET, or SLIDE content", async () => {
  const { document, sandbox, downloads, userClick } = await createWebUiDom({
    disconnectedHost: true,
  });

  assert.equal(sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__, undefined);
  assert.match(
    document.getElementById("status-engine").textContent,
    /엔진 미연결: 데스크톱 호스트 또는 로컬 서버\(--serve\) 연결이 필요합니다\./
  );

  await assert.rejects(
    () => sandbox.__EASYOFFICE_INVOKE_BACKEND__("get_workspace_bootstrap", ""),
    /엔진 미연결: 데스크톱 호스트 또는 로컬 서버\(--serve\) 연결이 필요합니다\./
  );

  const docEditor = document.getElementById("doc-editor");
  docEditor.innerHTML = "<p>연락처 <b>010-1234-5678</b> / 이메일 <a href=\"mailto:a@b.kr\">a@b.kr</a></p>";
  const docHtmlBefore = docEditor.innerHTML;

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(6);
  assert.equal(docEditor.innerHTML, docHtmlBefore);
  assert.match(
    document.getElementById("status-command-feedback").textContent,
    /\[요청 거부\] 엔진 미연결: 데스크톱 호스트 또는 로컬 서버\(--serve\) 연결이 필요합니다\./
  );
  assert.match(
    document.getElementById("pii-result-text").textContent,
    /\[요청 거부\] 엔진 미연결: 데스크톱 호스트 또는 로컬 서버\(--serve\) 연결이 필요합니다\./
  );

  userClick(document.getElementById("qat-save"));
  await flushAsyncTurns(6);
  assert.equal(downloads.length, 0);
  assert.equal(docEditor.innerHTML, docHtmlBefore);

  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(6);
  assert.match(
    document.getElementById("conv-result-box").textContent,
    /\[요청 거부\] 엔진 미연결/
  );

  userClick(document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns(6);
  assert.match(
    document.getElementById("security-status-text").textContent,
    /\[요청 거부\] 엔진 미연결/
  );

  userClick(document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns(4);
  const f2Cell = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const f2Before = f2Cell.textContent;
  const formulaBar = document.getElementById("formula-input");
  formulaBar.value = "=SUM(B2:E2)+999";
  userClick(document.getElementById("btn-eval-formula"));
  await flushAsyncTurns(6);
  assert.equal(f2Cell.textContent, f2Before);

  userClick(document.querySelector('.mod-btn[data-module="SLIDE"]'));
  await flushAsyncTurns(4);
  const slideTitle = document.getElementById("slide-title-zone");
  slideTitle.textContent = "슬라이드 연락처 011-222-3333";
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(6);
  assert.equal(slideTitle.textContent, "슬라이드 연락처 011-222-3333");
});

test("stored XSS guard preserves literal HTML markup across slide, tab, and table mutations without creating DOM elements", async () => {
  const { document, userClick } = await createWebUiDom();

  userClick(document.querySelector('.mod-btn[data-module="SLIDE"]'));
  await flushAsyncTurns(4);

  const slideTitle = document.getElementById("slide-title-zone");
  const xssPayload = '<img src=x onerror=alert(1)> <b>강조</b>';
  slideTitle.textContent = xssPayload;
  slideTitle.dispatchEvent({ type: "input", target: slideTitle });
  await flushAsyncTurns(2);

  userClick(document.querySelector('#slide-thumb-list .slide-thumb[data-slide-idx="1"]'));
  await flushAsyncTurns(4);
  assert.notEqual(slideTitle.textContent, xssPayload);

  userClick(document.querySelector('#slide-thumb-list .slide-thumb[data-slide-idx="0"]'));
  await flushAsyncTurns(4);
  assert.equal(slideTitle.textContent, xssPayload);
  assert.equal(slideTitle.querySelectorAll("img").length, 0);
  assert.equal(slideTitle.querySelectorAll("b").length, 0);

  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns(4);
  const docEditor = document.getElementById("doc-editor");
  const customPara = document.createElement("p");
  customPara.id = "xss-doc-para";
  customPara.textContent = '<script>alert("xss")</script><img src=x onerror=1>';
  docEditor.replaceChildren(customPara);

  userClick(document.querySelector('.mod-btn[data-module="HWP"]'));
  await flushAsyncTurns(4);
  const addRowBtn = document.getElementById("btn-doc-add-row");
  assert.ok(addRowBtn);
  userClick(addRowBtn);
  await flushAsyncTurns(2);
  const hwpRows = document.querySelectorAll("#doc-table-body tr");
  assert.ok(hwpRows.length >= 4);

  userClick(document.querySelector('.mod-btn[data-module="DOC"]'));
  await flushAsyncTurns(4);
  const restoredPara = document.getElementById("xss-doc-para");
  assert.ok(restoredPara);
  assert.equal(restoredPara.textContent, '<script>alert("xss")</script><img src=x onerror=1>');
  assert.equal(docEditor.querySelectorAll("script").length, 0);
  assert.equal(docEditor.querySelectorAll("img").length, 0);
});

test("PII scrub fails closed on HTTP 400 or empty scrubbedText, makes zero DOM writes on 0 matches, and scrubs table/div/li preserving inline nodes", async () => {
  let httpMode = "http400";
  const { document, userClick } = await createWebUiDom({
    httpFetch: async (_url, init) => {
      const req = JSON.parse(init.body);
      if (req.command === "get_workspace_bootstrap") {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            platform: "Linux (WebKitGTK)",
            shortcutModifier: "Ctrl",
            recentFormatFilters: ["전체"],
            openTabs: [],
          }),
        };
      }
      if (req.command === "get_ribbon_category_commands") {
        return {
          ok: true,
          status: 200,
          json: async () => ({ commands: [] }),
        };
      }
      if (req.command === "scrub_pii") {
        if (httpMode === "http400") {
          return {
            ok: false,
            status: 400,
            json: async () => ({ error: "Invalid surrogate payload" }),
          };
        }
        if (httpMode === "corruptEmpty") {
          return {
            ok: true,
            status: 200,
            json: async () => ({
              scrubbedText: "",
              maskedResidentIds: 0,
              maskedPhoneNumbers: 1,
              maskedEmails: 0,
              totalMasked: 1,
              strippedMetadataFields: [],
            }),
          };
        }
        if (httpMode === "zeroMatch") {
          const parsed = req.payload.startsWith("{") ? JSON.parse(req.payload).text : req.payload;
          return {
            ok: true,
            status: 200,
            json: async () => ({
              scrubbedText: parsed,
              maskedResidentIds: 0,
              maskedPhoneNumbers: 0,
              maskedEmails: 0,
              totalMasked: 0,
              strippedMetadataFields: [],
            }),
          };
        }
      }
      return { ok: true, status: 200, json: async () => ({ status: "ok" }) };
    },
  });

  const docEditor = document.getElementById("doc-editor");
  docEditor.innerHTML =
    '<p id="p1">담당자 <b>김철수</b> 연락처 <a id="mail-link" href="https://example.org">011-123-4567</a></p>';
  const boldNode = docEditor.querySelector("b");
  const linkNode = docEditor.querySelector("a");
  const htmlSnapshot = docEditor.innerHTML;

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(6);
  assert.equal(docEditor.innerHTML, htmlSnapshot);
  assert.equal(docEditor.querySelector("b"), boldNode);
  assert.equal(docEditor.querySelector("a"), linkNode);

  httpMode = "corruptEmpty";
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(6);
  assert.equal(docEditor.innerHTML, htmlSnapshot);
  assert.equal(docEditor.querySelector("b"), boldNode);
  assert.equal(docEditor.querySelector("a"), linkNode);

  httpMode = "zeroMatch";
  docEditor.innerHTML = '<p id="clean-p">개인정보가 전혀 없는 일반 보고서 문단입니다.</p>';
  const cleanP = document.getElementById("clean-p");
  const cleanTextNode = cleanP.childNodes[0];
  let textWrites = 0;
  const origDesc = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(cleanTextNode), "nodeValue");
  Object.defineProperty(cleanTextNode, "nodeValue", {
    get() {
      return origDesc.get.call(this);
    },
    set(v) {
      textWrites += 1;
      origDesc.set.call(this, v);
    },
  });

  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(6);
  assert.equal(textWrites, 0);
  assert.equal(document.getElementById("clean-p"), cleanP);
  assert.equal(cleanP.childNodes[0], cleanTextNode);

  const fullHarness = await createWebUiDom();
  const ed2 = fullHarness.document.getElementById("doc-editor");
  ed2.innerHTML =
    '<div>부서 <b>재무팀</b> 011-987-6543</div>' +
    '<ul><li>문의 <a href="https://example.kr">홍길동@회사.kr</a> 확인</li></ul>' +
    '<table><tbody><tr><td>외국인번호 900101-5123456</td></tr></tbody></table>';
  const bRef = ed2.querySelector("b");
  const aRef = ed2.querySelector("a");
  fullHarness.userClick(fullHarness.document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(8);
  assert.equal(ed2.querySelector("b"), bRef);
  assert.equal(ed2.querySelector("a"), aRef);
  assert.match(ed2.textContent, /011-\*\*\*\*-\*\*\*\*/);
  assert.match(ed2.textContent, /\*\*\*@\*\*\*/);
  assert.match(ed2.textContent, /\*\*\*\*\*\*-\*\*\*\*\*\*\*/);
  assert.doesNotMatch(ed2.textContent, /987-6543|홍길동@회사\.kr|5123456/);
});

test("structured JSON IPC preserves ? and %3B in cross-sheet formulas, literals, and real container security audits", async () => {
  const capturedCalls = [];
  const { document, sandbox } = await createWebUiDom();
  const origInvoke = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_STYLE_LABELS,
    contextualToolBadges: sandbox.__EASYOFFICE_RIBBON_CATALOG__.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: sandbox.__EASYOFFICE_RIBBON_CATALOG__.MODULE_SPECS,
    localRibbonState: sandbox.__EASYOFFICE_RIBBON_CATALOG__.createDefaultRibbonState(),
  });

  const wrappedHarness = await createWebUiDom({
    testIpc: async (command, payload) => {
      capturedCalls.push({ command, payload: String(payload ?? "") });
      return origInvoke[command](payload);
    },
  });

  wrappedHarness.userClick(
    wrappedHarness.document.querySelector('.mod-btn[data-module="SHEET"]')
  );
  await flushAsyncTurns(4);

  const d2 = wrappedHarness.document.querySelector('#pane-sheet td[data-cell="D2"]');
  d2.focus();
  d2.textContent = "100%3B";
  d2.dispatchEvent({ type: "input", target: d2 });
  d2.blur();
  await flushAsyncTurns(4);

  const e2 = wrappedHarness.document.querySelector('#pane-sheet td[data-cell="E2"]');
  e2.focus();
  e2.textContent = '=IF(Sheet2!A1>1,"a?b",0)';
  e2.dispatchEvent({ type: "input", target: e2 });
  e2.blur();
  await flushAsyncTurns(6);
  assert.equal(e2.textContent, "a?b");
  assert.equal(d2.textContent, "100%3B");

  wrappedHarness.userClick(wrappedHarness.document.getElementById("qat-save"));
  await flushAsyncTurns(8);
  const saveCall = capturedCalls.find((c) => c.command === "convert_document_format");
  assert.ok(saveCall);
  assert.equal(saveCall.payload.startsWith("{"), true);
  const parsedSave = JSON.parse(saveCall.payload);
  const savedCells = parsedSave.sheets.flatMap((s) => s.cells);
  assert.ok(savedCells.some((c) => c.ref === "D2" && c.raw === "100%3B"));
  assert.ok(savedCells.some((c) => c.ref === "E2" && c.raw === '=IF(Sheet2!A1>1,"a?b",0)'));

  capturedCalls.length = 0;
  wrappedHarness.userClick(wrappedHarness.document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns(8);
  assert.equal(capturedCalls[0].command, "convert_document_format");
  assert.equal(capturedCalls[1].command, "audit_security");
  const parsedAudit = JSON.parse(capturedCalls[1].payload);
  assert.ok(parsedAudit.containerBase64.length > 0);

  assert.throws(() => origInvoke.audit_security(""), /InvalidArgument|비어 있습니다/);
  assert.throws(() => origInvoke.audit_security(",,,"), /InvalidArgument|비어 있습니다/);
  assert.throws(
    () => origInvoke.audit_security(JSON.stringify({ entries: [] })),
    /InvalidArgument|비어 있습니다/
  );
  assert.ok(document);
});

test("shared PII vectors in fixtures/pii-scrub-vectors.json match JS sheet-utils and offline-rpc engines", async () => {
  const vectorsDoc = JSON.parse(fs.readFileSync(VECTORS_PATH, "utf8"));
  const { sandbox } = await createWebUiDom();
  const localScrubPii = sandbox.__EASYOFFICE_SHEET_UTILS__.localScrubPii;
  const offlineRpc = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: sandbox.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_STYLE_LABELS,
    contextualToolBadges: sandbox.__EASYOFFICE_RIBBON_CATALOG__.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: sandbox.__EASYOFFICE_RIBBON_CATALOG__.MODULE_SPECS,
    localRibbonState: sandbox.__EASYOFFICE_RIBBON_CATALOG__.createDefaultRibbonState(),
  });

  const vectors = Array.isArray(vectorsDoc) ? vectorsDoc : vectorsDoc.vectors;
  for (const vec of vectors) {
    const resA = localScrubPii(vec.input);
    const resB = offlineRpc.scrub_pii(vec.input);
    assert.equal(resA.totalMasked, vec.expectedCount, `localScrubPii count mismatch on ${vec.id}`);
    assert.equal(resA.scrubbedText, vec.expectedRustJsMasked, `localScrubPii text mismatch on ${vec.id}`);
    assert.deepEqual(resA.strippedMetadataFields, [], `plain text must report 0 stripped metadata on ${vec.id}`);
    assert.equal(resB.totalMasked, vec.expectedCount, `offlineRpc count mismatch on ${vec.id}`);
    assert.equal(resB.scrubbedText, vec.expectedRustJsMasked, `offlineRpc text mismatch on ${vec.id}`);
    for (const forbidden of vec.forbiddenSubstrings) {
      assert.equal(resA.scrubbedText.includes(forbidden), false);
    }
  }

  const metaRes = offlineRpc.scrub_pii(
    JSON.stringify({
      text: "문의 010.1234.5678",
      metadata: { Author: "홍길동", Company: "이지오피스", EmptyField: "   " },
    })
  );
  assert.equal(metaRes.scrubbedText, "문의 010-****-****");
  assert.deepEqual(metaRes.strippedMetadataFields, ["Author", "Company"]);
});
