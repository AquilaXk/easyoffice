import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";

// Desktop shell (Tauri v2 / WebView2) bridge: a fake `__TAURI_INTERNALS__` stands in for the
// WebView IPC. The Rust side registers exactly two commands, `ipc_dispatch` (returns the engine's
// JSON text) and `save_document_file` (native save dialog + write), and rejects with the same
// `{ status: "error", message }` body that the `--serve` HTTP transport returns with status 400.

async function createOfflineBackend() {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  return sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
}

async function createShellHarness({ engineOverrides = {}, saveFile } = {}) {
  const offline = await createOfflineBackend();
  const calls = [];
  const fetchCalls = [];
  const internals = {
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "ipc_dispatch") {
        const handler = engineOverrides[args.command] || offline[args.command];
        if (typeof handler !== "function") {
          throw { status: "error", message: "알 수 없는 IPC 명령입니다: " + args.command };
        }
        try {
          return JSON.stringify(await handler(args.payload, offline));
        } catch (err) {
          throw { status: "error", message: err?.message || String(err) };
        }
      }
      if (command === "save_document_file") {
        if (!saveFile) throw { status: "error", message: "save not configured" };
        return saveFile(args);
      }
      throw { status: "error", message: "Command " + command + " not allowed by ACL" };
    },
  };
  const harness = await createWebUiDom({
    // Inside the Windows shell the page origin is http://tauri.localhost, so the HTTP transport
    // would look available; the bridge must still win and fetch must never be used.
    protocol: "http:",
    httpFetch: async (url) => {
      fetchCalls.push(url);
      throw new Error("fetch must not be used inside the desktop shell");
    },
    tauriInternals: internals,
  });
  return { ...harness, calls, fetchCalls, internals };
}

const feedback = (document) => document.getElementById("status-command-feedback").textContent;

test("desktop shell routes every engine request through ipc_dispatch with string payloads", async () => {
  const { document, calls, fetchCalls } = await createShellHarness();
  const boot = calls.find((c) => c.command === "ipc_dispatch" && c.args.command === "get_workspace_bootstrap");
  assert.ok(boot, "bootstrap goes through the shell");
  assert.deepEqual(boot.args, { command: "get_workspace_bootstrap", payload: "" });
  assert.ok(calls.every((c) => c.command === "ipc_dispatch"), "only the dispatcher is used at startup");
  assert.ok(calls.every((c) => typeof c.args.payload === "string"));
  assert.ok(calls.some((c) => c.args.command === "get_ribbon_category_commands"));
  assert.equal(fetchCalls.length, 0);
  assert.equal(document.getElementById("status-engine").textContent, "엔진: 데스크톱 연결됨");
});

test("shell rejections surface exactly like the --serve HTTP 400 error body", async () => {
  const message = "SecurityViolation: XXE DOCTYPE blocked";
  const shell = await createShellHarness({
    engineOverrides: {
      audit_security: () => {
        throw new Error(message);
      },
    },
  });
  shell.userClick(shell.document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns(10);
  const shellText = shell.document.getElementById("security-status-text").textContent;
  const shellFeedback = feedback(shell.document);

  const offline = await createOfflineBackend();
  const http = await createWebUiDom({
    httpFetch: async (_url, init) => {
      const { command, payload } = JSON.parse(init.body);
      if (command === "audit_security") {
        return { ok: false, status: 400, json: async () => ({ status: "error", error: message, message }) };
      }
      const body = await offline[command](payload);
      return { ok: true, status: 200, json: async () => JSON.parse(JSON.stringify(body)) };
    },
  });
  http.userClick(http.document.getElementById("btn-run-security-audit"));
  await flushAsyncTurns(10);

  assert.match(shellText, /XXE DOCTYPE blocked/);
  assert.equal(shellText, http.document.getElementById("security-status-text").textContent);
  assert.equal(shellFeedback, feedback(http.document));
});

test("bridge accepts string and Error rejections and reports unparsable engine output", async () => {
  const shell = await createShellHarness();
  const bridge = shell.sandbox.__EASYOFFICE_DESKTOP_BRIDGE__;
  const describe = bridge?.describeShellError;
  assert.equal(typeof describe, "function");
  assert.equal(describe("plain text"), "plain text");
  assert.equal(describe(new Error("boom")), "boom");
  assert.equal(describe({ status: "error", message: "shaped" }), "shaped");
  assert.match(describe(undefined), /데스크톱 셸 요청 실패/);

  shell.internals.invoke = async () => "<html>not json</html>";
  await assert.rejects(bridge.invokeIpc("detect_format", "x"), /엔진 응답을 해석하지 못했습니다 \(detect_format\)/);
  shell.internals.invoke = async () => ({ status: "weird" });
  await assert.rejects(bridge.saveFile("UEsDBA==", "a.docx"), /저장 결과를 확인하지 못했습니다/);
});

test("desktop save uses the native save command instead of a Blob download", async () => {
  const saves = [];
  const { document, userClick, downloads, calls } = await createShellHarness({
    saveFile: (args) => {
      saves.push(args);
      return { status: "saved", path: "C:\\Users\\me\\Documents\\" + args.fileName, bytes: 42 };
    },
  });
  userClick(document.getElementById("qat-save"));
  await flushAsyncTurns(12);

  assert.equal(saves.length, 1);
  assert.equal(saves[0].fileName, "분기_경영실적_보고서.docx");
  assert.ok(saves[0].base64.length > 0);
  assert.deepEqual(Object.keys(saves[0]).sort(), ["base64", "fileName"], "the WebView never sends a path");
  assert.equal(downloads.length, 0, "no Blob download inside the shell");
  assert.ok(calls.some((c) => c.command === "ipc_dispatch" && c.args.command === "convert_document_format"));
  assert.match(feedback(document), /^문서 저장 완료 \(C:\\Users\\me\\Documents\\분기_경영실적_보고서\.docx · /);
});

test("cancelled and failed desktop saves never claim success", async () => {
  const cancelled = await createShellHarness({ saveFile: () => ({ status: "cancelled", path: null, bytes: 0 }) });
  cancelled.userClick(cancelled.document.getElementById("qat-save"));
  await flushAsyncTurns(12);
  assert.match(feedback(cancelled.document), /^문서 저장 취소 \(DOCX\): 저장 위치를 선택하지 않았습니다/);
  assert.equal(cancelled.downloads.length, 0);

  const failed = await createShellHarness({
    saveFile: () => {
      throw { status: "error", message: "파일을 저장하지 못했습니다 (D:\\ro\\a.docx): 액세스가 거부되었습니다." };
    },
  });
  failed.userClick(failed.document.getElementById("qat-save"));
  await flushAsyncTurns(12);
  assert.match(feedback(failed.document), /^문서 저장 실패 \(DOCX\): 파일을 저장하지 못했습니다 .*액세스가 거부/);
  assert.doesNotMatch(feedback(failed.document), /저장 완료/);
});

test("converter output is saved through the shell and reports cancellation honestly", async () => {
  let outcome = { status: "saved", path: "C:\\out\\통합_변환_문서.hwpx", bytes: 10 };
  const saves = [];
  const { document, userClick, downloads } = await createShellHarness({
    saveFile: (args) => {
      saves.push(args);
      return outcome;
    },
  });
  userClick(document.querySelector('.mod-btn[data-module="CONV"]'));
  await flushAsyncTurns(6);
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(12);
  const box = document.getElementById("conv-result-box");
  assert.equal(saves.length, 1);
  assert.match(saves[0].fileName, /^통합_변환_문서\.[a-z0-9]+$/);
  assert.doesNotMatch(box.textContent, /실패|취소/);
  assert.equal(downloads.length, 0);

  outcome = { status: "cancelled", path: null, bytes: 0 };
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(12);
  assert.match(box.textContent, / · 저장 취소: 저장 위치를 선택하지 않았습니다$/);
});

test("without the shell the --serve preview keeps the HTTP transport and Blob download", async () => {
  const offline = await createOfflineBackend();
  const fetched = [];
  const { document, userClick, downloads } = await createWebUiDom({
    httpFetch: async (url, init) => {
      const { command, payload } = JSON.parse(init.body);
      fetched.push({ url, command });
      const body = await offline[command](payload);
      return { ok: true, status: 200, json: async () => JSON.parse(JSON.stringify(body)) };
    },
  });
  assert.ok(fetched.some((f) => f.url === "/api/ipc" && f.command === "get_workspace_bootstrap"));
  assert.equal(document.getElementById("status-engine").textContent, "엔진: 로컬 서버 연결됨");
  userClick(document.getElementById("qat-save"));
  await flushAsyncTurns(12);
  assert.equal(downloads.length, 1);
  assert.equal(downloads[0].download, "분기_경영실적_보고서.docx");
  assert.match(feedback(document), /^문서 저장 완료 \(분기_경영실적_보고서\.docx · /);
});

test("a shell object without an invoke function is ignored", async () => {
  const offline = await createOfflineBackend();
  const fetched = [];
  const { document } = await createWebUiDom({
    tauriInternals: { invoke: "not-a-function" },
    httpFetch: async (url, init) => {
      const { command, payload } = JSON.parse(init.body);
      fetched.push(command);
      const body = await offline[command](payload);
      return { ok: true, status: 200, json: async () => JSON.parse(JSON.stringify(body)) };
    },
  });
  assert.ok(fetched.includes("get_workspace_bootstrap"));
  assert.equal(document.getElementById("status-engine").textContent, "엔진: 로컬 서버 연결됨");
});
