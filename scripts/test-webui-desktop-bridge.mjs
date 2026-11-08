import test from "node:test";
import assert from "node:assert/strict";
import { shellError, createWebUiDom, flushAsyncTurns, requireShellToken, SHELL_TOKEN, SHELL_TOKEN_HEADER } from "./webui-dom-harness.mjs";
import { switchModule, studioItem, commandItem } from "./test-support/modules.mjs";
import { readText } from "./test-support/read-text.mjs";

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
    invoke: async (command, args, options) => {
      calls.push({ command, args, options });
      requireShellToken(options);
      if (command === "ipc_dispatch") {
        const handler = engineOverrides[args.command] || offline[args.command];
        if (typeof handler !== "function") {
          throw shellError("알 수 없는 IPC 명령입니다: " + args.command);
        }
        try {
          return JSON.stringify(await handler(args.payload, offline));
        } catch (err) {
          throw shellError(err?.message || String(err));
        }
      }
      if (command === "save_document_file") {
        if (!saveFile) throw shellError("save not configured");
        return saveFile(args);
      }
      throw shellError("Command " + command + " not allowed by ACL");
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
  const { document, calls, fetchCalls, userClick } = await createShellHarness();
  const boot = calls.find((c) => c.command === "ipc_dispatch" && c.args.command === "get_workspace_bootstrap");
  assert.ok(boot, "bootstrap goes through the shell");
  assert.deepEqual(boot.args, { command: "get_workspace_bootstrap", payload: "" });
  const engineCalls = calls.filter((c) => c.command !== "report_menu_state");
  assert.ok(engineCalls.every((c) => c.command === "ipc_dispatch"), "only the dispatcher and the menu report are used at startup");
  assert.ok(engineCalls.every((c) => typeof c.args.payload === "string"));
  userClick(commandItem(document, "document-home-bold"));
  await flushAsyncTurns(12);
  const exec = calls.find((c) => c.args.command === "execute_ribbon_command");
  assert.ok(exec, "menu commands reach the engine through the dispatcher");
  assert.equal(typeof exec.args.payload, "string");
  assert.ok(calls.every((c) => c.command === "ipc_dispatch" || c.command === "report_menu_state"));
  assert.equal(fetchCalls.length, 0);
  assert.equal(document.getElementById("status-engine").textContent, "연결됨");
  assert.doesNotMatch(document.getElementById("status-bar").textContent, /로컬 서버/, "the desktop app never shows the local-server text");
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
      return { ok: true, status: 200, json: async () => structuredClone(body) };
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
  assert.match(describe(undefined), /요청을 처리하지 못했습니다/);

  shell.internals.invoke = async () => "<html>not json</html>";
  await assert.rejects(bridge.invokeIpc("detect_format", "x"), /응답을 처리하지 못했습니다/);
  shell.internals.invoke = async () => ({ status: "weird" });
  await assert.rejects(bridge.saveFile("UEsDBA==", "a.docx"), /저장 결과를 확인하지 못했습니다/);
});

test("desktop save uses the native save command instead of a Blob download", async () => {
  const saves = [];
  const { document, userClick, downloads, calls } = await createShellHarness({
    saveFile: (args) => {
      saves.push(args);
      return { status: "saved", name: args.fileName, bytes: 42 };
    },
  });
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);

  assert.equal(saves.length, 1);
  assert.equal(saves[0].fileName, "분기_경영실적_보고서.docx");
  assert.ok(saves[0].base64.length > 0);
  assert.deepEqual(
    Object.keys(saves[0]).sort((a, b) => a.localeCompare(b)),
    ["base64", "fileName", "inPlace"],
    "the WebView never sends a path"
  );
  assert.equal(saves[0].inPlace, true, "Save asks Rust to write back to the opened file when it knows it");
  assert.equal(downloads.length, 0, "no Blob download inside the shell");
  assert.ok(calls.some((c) => c.command === "ipc_dispatch" && c.args.command === "convert_document_format"));
  assert.match(feedback(document), /^문서 저장 완료 \(분기_경영실적_보고서\.docx · /);
});

test("the first page of a Hangul document is sent to the shell only when the page gives one", async () => {
  const saves = [];
  const shell = await createShellHarness({
    saveFile: (args) => {
      saves.push(args);
      return { status: "saved", name: args.fileName, bytes: 1 };
    },
  });
  const bridge = shell.sandbox.__EASYOFFICE_DESKTOP_BRIDGE__;
  const svg = '<svg xmlns="http://www.w3.org/2000/svg"/>';
  await bridge.saveFile("QQ==", "a.hwp", { inPlace: true, thumbnailSvg: svg });
  await bridge.saveFile("QQ==", "b.hwp", { inPlace: true });
  await bridge.saveFile("QQ==", "c.hwp", { thumbnailSvg: "" });
  await bridge.saveFile("QQ==", "d.hwp", { thumbnailSvg: 42 });
  assert.deepEqual(Object.keys(saves[0]).sort((a, b) => a.localeCompare(b)), ["base64", "fileName", "inPlace", "thumbnailSvg"]);
  assert.equal(saves[0].thumbnailSvg, svg);
  for (const save of saves.slice(1)) {
    assert.deepEqual(Object.keys(save).sort((a, b) => a.localeCompare(b)), ["base64", "fileName", "inPlace"]);
  }
});

test("cancelled and failed desktop saves never claim success", async () => {
  const cancelled = await createShellHarness({ saveFile: () => ({ status: "cancelled", name: null, bytes: 0 }) });
  cancelled.userClick(studioItem(cancelled.document, "studio-save"));
  await flushAsyncTurns(12);
  assert.match(feedback(cancelled.document), /^문서 저장 취소 \(DOCX\): 저장 위치를 선택하지 않았습니다/);
  assert.equal(cancelled.downloads.length, 0);

  const failed = await createShellHarness({
    saveFile: () => {
      throw shellError(String.raw`파일을 저장하지 못했습니다 (D:\ro\a.docx): 액세스가 거부되었습니다.`);
    },
  });
  failed.userClick(studioItem(failed.document, "studio-save"));
  await flushAsyncTurns(12);
  assert.match(feedback(failed.document), /^문서 저장 실패 \(DOCX\): 파일을 저장하지 못했습니다 .*액세스가 거부/);
  assert.doesNotMatch(feedback(failed.document), /저장 완료/);
});

test("converter output is saved through the shell and reports cancellation honestly", async () => {
  let outcome = { status: "saved", name: "통합_변환_문서.hwpx", bytes: 10 };
  const saves = [];
  const { document, userClick, downloads } = await createShellHarness({
    saveFile: (args) => {
      saves.push(args);
      return outcome;
    },
  });
  switchModule(document, "CONV");
  await flushAsyncTurns(6);
  userClick(document.getElementById("btn-run-convert"));
  await flushAsyncTurns(12);
  const box = document.getElementById("conv-result-box");
  assert.equal(saves.length, 1);
  assert.match(saves[0].fileName, /^통합_변환_문서\.[a-z0-9]+$/);
  assert.doesNotMatch(box.textContent, /실패|취소/);
  assert.equal(downloads.length, 0);

  outcome = { status: "cancelled", name: null, bytes: 0 };
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
      return { ok: true, status: 200, json: async () => structuredClone(body) };
    },
  });
  assert.ok(fetched.some((f) => f.url === "/api/ipc" && f.command === "get_workspace_bootstrap"));
  assert.equal(document.getElementById("status-engine").textContent, "연결됨");
  userClick(studioItem(document, "studio-save"));
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
      return { ok: true, status: 200, json: async () => structuredClone(body) };
    },
  });
  assert.ok(fetched.includes("get_workspace_bootstrap"));
  assert.equal(document.getElementById("status-engine").textContent, "연결됨");
});

test("every shell command carries the window's command token as a header", async () => {
  const { calls, sandbox } = await createShellHarness();
  const bridge = sandbox.__EASYOFFICE_DESKTOP_BRIDGE__;
  await bridge.getUpdateNotice().catch(() => null);
  await bridge.windowCommand("takePendingOpen", {}).catch(() => null);
  await bridge.saveFile("QQ==", "a.docx").catch(() => null);
  assert.ok(calls.length >= 4);
  for (const call of calls) {
    assert.equal(call.options?.headers?.[SHELL_TOKEN_HEADER], SHELL_TOKEN, call.command);
  }
});

test("without the token global the bridge refuses to call the shell at all", async () => {
  const calls = [];
  const { sandbox } = await createWebUiDom({
    shellToken: null,
    tauriInternals: { invoke: async (command, args, options) => { calls.push({ command }); requireShellToken(options); return "{}"; } },
  });
  const bridge = sandbox.__EASYOFFICE_DESKTOP_BRIDGE__;
  await assert.rejects(bridge.invokeIpc("get_workspace_bootstrap", ""), /연결할 수 없습니다/);
  await assert.rejects(bridge.getUpdateNotice(), /연결할 수 없습니다/);
  assert.equal(calls.length, 0, "nothing reaches the shell without a token");
});

// ---- native confirmations (confirm_dialog, PD-120) ------------------------------------------------

function confirmHarnessOptions(answers, calls) {
  return {
    tauriInternals: {
      invoke: async (command, args, options) => {
        requireShellToken(options);
        if (command === "confirm_dialog") {
          calls.push({ command, args, options });
          const answer = answers.shift();
          if (answer instanceof Error) throw shellError(answer.message);
          return answer;
        }
        if (command === "ipc_dispatch") return JSON.stringify({ status: "OK" });
        return null;
      },
    },
  };
}

test("desktop confirm asks the shell by kind name only, with the command token, and only a true answer accepts", async () => {
  const calls = [];
  const { sandbox } = await createWebUiDom(confirmHarnessOptions([true, false, "yes", null, 1], calls));
  const bridge = sandbox.__EASYOFFICE_DESKTOP_BRIDGE__;
  assert.equal(await bridge.confirm("password-free-save"), true);
  assert.equal(await bridge.confirm("replace-unsaved-hwp"), false);
  assert.equal(await bridge.confirm("hwpx-fallback-hwp"), false, "a truthy string is not an acceptance");
  assert.equal(await bridge.confirm("restore-hwp-autosave"), false);
  assert.equal(await bridge.confirm("replace-unsaved-module"), false);
  assert.deepEqual(calls.map((c) => c.args), [
    { kind: "password-free-save" }, { kind: "replace-unsaved-hwp" }, { kind: "hwpx-fallback-hwp" },
    { kind: "restore-hwp-autosave" }, { kind: "replace-unsaved-module" },
  ], "no wording and no file name travels to the shell");
  for (const call of calls) assert.equal(call.options.headers[SHELL_TOKEN_HEADER], SHELL_TOKEN);
});

test("desktop confirm rejects when the shell refuses, and without the token it never reaches the shell", async () => {
  const calls = [];
  const { sandbox } = await createWebUiDom(confirmHarnessOptions([new Error("확인 창을 열지 못했습니다")], calls));
  await assert.rejects(sandbox.__EASYOFFICE_DESKTOP_BRIDGE__.confirm("password-free-save"), /확인 창을 열지 못했습니다/);
  const none = [];
  const untokened = await createWebUiDom({ ...confirmHarnessOptions([true], none), shellToken: null });
  await assert.rejects(untokened.sandbox.__EASYOFFICE_DESKTOP_BRIDGE__.confirm("password-free-save"), /연결할 수 없습니다/);
  assert.equal(none.length, 0);
});

test("browser preview: confirm uses window.confirm with the kind's text; unknown kinds and a missing dialog answer no", async () => {
  const asked = [];
  const { sandbox } = await createWebUiDom({ confirm: (text) => { asked.push(text); return true; } });
  const bridge = sandbox.__EASYOFFICE_DESKTOP_BRIDGE__;
  assert.equal(await bridge.confirm("password-free-save"), true);
  assert.equal(asked[0], "저장한 HWP 파일에는 암호가 적용되지 않습니다. 암호 없이 저장하시겠습니까?");
  assert.equal(await bridge.confirm("anything else"), false);
  assert.equal(await bridge.confirm("constructor"), false);
  assert.equal(asked.length, 1, "an unknown kind asks nothing");
  const silent = await createWebUiDom({});
  silent.sandbox.confirm = undefined;
  assert.equal(await silent.sandbox.__EASYOFFICE_DESKTOP_BRIDGE__.confirm("password-free-save"), false);
});

test("the browser texts equal the shell's wording and the command is granted like the other app commands", async () => {
  const rust = readText(new URL("../crates/easyoffice-desktop/src/confirm_dialog.rs", import.meta.url));
  const production = rust.split("#[cfg(test)]")[0];
  const fromRust = {};
  for (const m of production.matchAll(/Self::(\w+) => ConfirmSpec \{\s*title: "[^"]*",\s*message: "([^"]*)",/g)) {
    fromRust[m[1].replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase()] = m[2].replaceAll("\\n", "\n");
  }
  const { sandbox } = await createWebUiDom({});
  const browser = sandbox.__EASYOFFICE_DESKTOP_BRIDGE__.CONFIRM_BROWSER_TEXT;
  assert.equal(Object.keys(fromRust).length, 5);
  assert.deepEqual({ ...browser }, fromRust);
  const caps = JSON.parse(readText(new URL("../crates/easyoffice-desktop/capabilities/main-window.json", import.meta.url)));
  assert.ok(caps.permissions.includes("allow-confirm-dialog"));
  const commands = readText(new URL("../crates/easyoffice-desktop/src/app_commands.rs", import.meta.url));
  assert.ok(commands.includes('"confirm_dialog"'));
  assert.ok(caps.permissions.includes("allow-confirm-close"), "the close prompt is granted like the other app commands");
  assert.ok(commands.includes('"confirm_close"'));
  assert.equal(sandbox.__EASYOFFICE_DESKTOP_BRIDGE__.WINDOW_COMMANDS.confirmClose, "confirm_close");
  assert.equal(sandbox.__EASYOFFICE_DESKTOP_BRIDGE__.WINDOW_COMMANDS.printWebview, "print_webview");
});

test("the close prompt wording lives in the shell only: the page holds no text for it", () => {
  const bridge = readText(new URL("../crates/easyoffice-app/ui/window-bridge.js", import.meta.url));
  const html = readText(new URL("../crates/easyoffice-app/ui/index.html", import.meta.url));
  assert.ok(!html.includes("close-confirm-dialog") && !html.includes("btn-close-save"), "no page dialog for the save prompt");
  assert.ok(!bridge.includes("변경 내용을 저장할까요"), "the document name and wording come from the shell");
  const rust = readText(new URL("../crates/easyoffice-desktop/src/confirm_dialog.rs", import.meta.url));
  assert.ok(rust.includes('"변경 내용을 저장할까요?"') || rust.includes("의 변경 내용을 저장할까요?"));
});
