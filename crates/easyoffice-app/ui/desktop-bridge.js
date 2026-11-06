(function (host) {
  "use strict";

  // Windows desktop shell bridge (Tauri v2 / WebView2, `crates/easyoffice-desktop`). Inside the
  // shell every engine request goes through the in-process `ipc_dispatch` command, and saving
  // goes through `save_document_file` (native save dialog + write in Rust). Outside the shell
  // (`--serve` preview, tests) `isAvailable()` is false and callers keep their HTTP/Blob paths.

  const IPC_COMMAND = "ipc_dispatch";
  const SAVE_COMMAND = "save_document_file";

  function shellInternals() {
    const internals = host.__TAURI_INTERNALS__;
    return internals && typeof internals.invoke === "function" ? internals : null;
  }

  function isAvailable() {
    return shellInternals() !== null;
  }

  // Shell commands reject with `{ status: "error", message }` (same body as the `--serve` HTTP
  // 400 response); also accept plain strings/Errors so every rejection yields a readable message.
  function describeShellError(err) {
    if (typeof err === "string" && err) return err;
    if (err && typeof err === "object") {
      if (typeof err.message === "string" && err.message) return err.message;
      if (typeof err.error === "string" && err.error) return err.error;
    }
    return "데스크톱 셸 요청 실패: " + String(err);
  }

  async function callShell(command, args) {
    const internals = shellInternals();
    if (!internals) throw new Error("데스크톱 셸이 연결되어 있지 않습니다.");
    try {
      return await internals.invoke(command, args);
    } catch (err) {
      throw new Error(describeShellError(err));
    }
  }

  async function invokeIpc(command, payload) {
    const raw = await callShell(IPC_COMMAND, {
      command: String(command),
      payload: String(payload ?? ""),
    });
    if (typeof raw !== "string") return raw;
    try {
      return JSON.parse(raw);
    } catch {
      throw new Error("엔진 응답을 해석하지 못했습니다 (" + command + ")");
    }
  }

  // Resolves `{ status: "saved", path, bytes }` or `{ status: "cancelled" }`; rejects with an
  // Error whose message explains why the file was not written.
  async function saveFile(base64Data, fileName) {
    const res = await callShell(SAVE_COMMAND, { fileName: String(fileName), base64: String(base64Data) });
    if (res && (res.status === "saved" || res.status === "cancelled")) return res;
    throw new Error("저장 결과를 확인하지 못했습니다.");
  }

  host.__EASYOFFICE_DESKTOP_BRIDGE__ = { isAvailable, invokeIpc, saveFile, describeShellError };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
