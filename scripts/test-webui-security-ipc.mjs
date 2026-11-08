import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

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

  userClick(studioItem(document, "studio-save"));
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

  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  const f2Cell = document.querySelector('#pane-sheet td[data-cell="F2"]');
  const f2Before = f2Cell.textContent;
  const formulaBar = document.getElementById("formula-input");
  formulaBar.value = "=SUM(B2:E2)+999";
  userClick(document.getElementById("btn-eval-formula"));
  await flushAsyncTurns(6);
  assert.equal(f2Cell.textContent, f2Before);

  switchModule(document, "SLIDE");
  await flushAsyncTurns(4);
  const slideTitle = document.getElementById("slide-title-zone");
  slideTitle.textContent = "슬라이드 연락처 011-222-3333";
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(6);
  assert.equal(slideTitle.textContent, "슬라이드 연락처 011-222-3333");
});

test("stored XSS guard preserves literal HTML markup across slide, tab, and table mutations without creating DOM elements", async () => {
  const { document, userClick } = await createWebUiDom();

  switchModule(document, "SLIDE");
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

  switchModule(document, "DOC");
  await flushAsyncTurns(4);
  const docEditor = document.getElementById("doc-editor");
  const customPara = document.createElement("p");
  customPara.id = "xss-doc-para";
  customPara.textContent = '<script>alert("xss")</script><img src=x onerror=1>';
  docEditor.replaceChildren(customPara);

  switchModule(document, "HWP");
  await flushAsyncTurns(4);
  const addRowBtn = document.getElementById("btn-doc-add-row");
  assert.ok(addRowBtn);
  userClick(addRowBtn);
  await flushAsyncTurns(2);
  const hwpRows = document.querySelectorAll("#doc-table-body tr");
  assert.ok(hwpRows.length >= 4);

  switchModule(document, "DOC");
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

  switchModule(wrappedHarness.document, "SHEET");
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

  wrappedHarness.userClick(studioItem(wrappedHarness.document, "studio-save"));
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

// ---- HWP editor frame isolation (PD-083) ------------------------------------------------
// Tauri decides IPC access per window/webview and per origin (local or remote), never per frame, so
// an editor frame that shares the shell's origin inherits every command the window may call. The
// frame therefore gets an opaque origin; these checks pin the pieces that keep it that way.

const ROOT = new URL("../", import.meta.url);
const readText = (rel) => fs.readFileSync(new URL(rel, ROOT), "utf8");

test("editor frame: the sandbox grants scripts and modals only, never the shell's origin", () => {
  const env = {};
  loadUiModule(new URL("crates/easyoffice-app/ui/hwp-host.js", ROOT), env);
  const { FRAME_SANDBOX } = env.__EASYOFFICE_HWP_HOST_CONSTANTS__;
  assert.deepEqual(FRAME_SANDBOX.split(" "), ["allow-scripts", "allow-modals"]);
  for (const forbidden of ["allow-same-origin", "allow-top-navigation", "allow-popups", "allow-forms", "allow-downloads"]) {
    assert.ok(!FRAME_SANDBOX.includes(forbidden), forbidden);
  }
});

test("editor frame: no code in the shell UI reaches into the frame or hands it Tauri objects", () => {
  const uiDir = new URL("crates/easyoffice-app/ui/", ROOT);
  for (const name of fs.readdirSync(uiDir).filter((n) => n.endsWith(".js") || n.endsWith(".html"))) {
    const source = fs.readFileSync(new URL(name, uiDir), "utf8");
    assert.doesNotMatch(source, /contentDocument|contentWindow\.(?!postMessage)/, `${name} reads the editor frame`);
  }
  const bridge = readText("crates/easyoffice-app/ui/desktop-bridge.js");
  assert.doesNotMatch(bridge, /allow-same-origin/);
});

test("editor frame: the window capability is local-only, global Tauri stays off and the editor gets a strict CSP", () => {
  const capability = JSON.parse(readText("crates/easyoffice-desktop/capabilities/main-window.json"));
  assert.equal(capability.local, true);
  assert.equal(capability.remote, undefined, "no remote origin may call commands");
  assert.deepEqual(capability.windows, ["main", "doc-*"]);
  assert.equal(capability.webviews, undefined);
  const conf = JSON.parse(readText("crates/easyoffice-desktop/tauri.conf.json"));
  assert.equal(conf.app.withGlobalTauri, false);
  assert.equal(conf.app.security.csp["frame-src"], "'self'");
  // The editor document is answered with the --serve policy (no ipc: endpoints) by the app protocol hook.
  const windows = readText("crates/easyoffice-desktop/src/window_commands.rs");
  assert.match(windows, /\.on_web_resource_request\([^)]*\|request, response\|/);
  assert.match(windows, /studio_frame::harden_studio_response\(&request, response\)/);
  const studioCsp = readText("crates/easyoffice-app/src/hwp_bundle.rs").match(/STUDIO_CSP: &str = "([^"]+)"/)[1];
  assert.match(studioCsp, /^default-src 'self';/);
  assert.doesNotMatch(studioCsp, /connect-src|ipc|'unsafe-/);
  // WebKit (macOS) does not match 'self' for the opaque-origin frame and blocks all its subresources:
  // the desktop answers the editor document with the policy that spells the app origin out.
  const studioFrame = readText("crates/easyoffice-desktop/src/studio_frame.rs");
  assert.match(studioFrame, /CONTENT_SECURITY_POLICY,\s+studio_csp_value\(app_origin\(cfg!\(windows\)\)\)/);
  // The origin is a platform constant, not the request URI: wry hands the hook `tauri://localhost/...`
  // even on Windows, where the document's origin is `http://tauri.localhost`.
  assert.match(studioFrame, /WINDOWS_APP_ORIGIN: &str = "http:\/\/tauri\.localhost"/);
  assert.match(studioFrame, /CUSTOM_SCHEME_APP_ORIGIN: &str = "tauri:\/\/localhost"/);
  assert.doesNotMatch(studioFrame.split("#[cfg(test)]")[0], /request\.uri\(\)\.(authority|scheme)|uri\.authority/);
  assert.match(readText("crates/easyoffice-app/src/hwp_bundle.rs"), /STUDIO_CSP\.replace\("'self'", origin\)/);
});

test("shell commands: only the desktop bridge calls the Tauri IPC, and it always sends the window token", () => {
  const uiDir = new URL("crates/easyoffice-app/ui/", ROOT);
  const callers = [];
  for (const name of fs.readdirSync(uiDir).filter((n) => n.endsWith(".js"))) {
    const code = fs.readFileSync(new URL(name, uiDir), "utf8").replace(/\/\/.*$/gm, "");
    if (/__TAURI_INTERNALS__|\bwindow\.ipc\b|chrome\.webview/.test(code)) callers.push(name);
  }
  // frame-probe.js is the smoke probe that plays the part of a compromised editor frame.
  assert.deepEqual(callers.sort((a, b) => a.localeCompare(b)), ["desktop-bridge.js", "frame-probe.js"]);
  const bridge = readText("crates/easyoffice-app/ui/desktop-bridge.js");
  assert.equal([...bridge.matchAll(/internals\.invoke\(/g)].length, 1, "one call site");
  assert.match(bridge, /internals\.invoke\(command, args, \{ headers: \{ \[TOKEN_HEADER\]: token \} \}\)/);
  const rust = readText("crates/easyoffice-desktop/src/shell_caller.rs");
  assert.ok(rust.includes(`TOKEN_HEADER: &str = "x-easyoffice-shell"`));
  assert.match(bridge, /const TOKEN_HEADER = "x-easyoffice-shell"/);
  assert.match(rust, /TOKEN_GLOBAL: &str = "__EASYOFFICE_SHELL_TOKEN__"/);
  assert.match(bridge, /const TOKEN_GLOBAL = "__EASYOFFICE_SHELL_TOKEN__"/);
});
