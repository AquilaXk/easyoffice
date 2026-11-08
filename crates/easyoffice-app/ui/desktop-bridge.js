(function (host) {
  "use strict";

  // Windows desktop shell bridge (Tauri v2 / WebView2, `crates/easyoffice-desktop`). Inside the
  // shell every engine request goes through the in-process `ipc_dispatch` command, and saving
  // goes through `save_document_file` (native save dialog + write in Rust). Outside the shell
  // (`--serve` preview, tests) `isAvailable()` is false and callers keep their HTTP/Blob paths.

  const IPC_COMMAND = "ipc_dispatch";
  const SAVE_COMMAND = "save_document_file";
  const EXPORT_PDF_COMMAND = "export_searchable_pdf";
  // Update notice (PD-103): the shell checks for a newer release; the page only reads the state,
  // switches the check on or off and asks the shell to open the release page (it passes no address).
  const UPDATE_COMMANDS = Object.freeze({
    get: "get_update_notice",
    set: "set_update_check",
    open: "open_release_page",
  });
  // Window commands (PD-088). None of them takes or returns a file system path: the Rust side owns
  // the native dialogs, file reads and the path registry; the page only sees names, kinds and bytes.
  const WINDOW_COMMANDS = Object.freeze({
    openDocumentWindow: "open_document_window",
    openFileInNewWindow: "open_file_in_new_window",
    takePendingOpen: "take_pending_open",
    reportWindowState: "report_window_state",
    closeCurrentWindow: "close_current_window",
    listRecentDocuments: "list_recent_documents",
    openRecentDocument: "open_recent_document",
    recentPreview: "get_recent_preview",
    reportMenuState: "report_menu_state",
    endClosePrompt: "end_close_prompt",
  });

  // Command token (`crates/easyoffice-desktop/src/shell_caller.rs`): the shell defines it in the
  // top-level document of each window only and refuses a command that does not carry it, so the
  // sandboxed editor frame cannot use the Tauri IPC objects it may be handed on Windows.
  const TOKEN_GLOBAL = "__EASYOFFICE_SHELL_TOKEN__";
  const TOKEN_HEADER = "x-easyoffice-shell";

  function shellToken() {
    const token = host[TOKEN_GLOBAL];
    return typeof token === "string" && token !== "" ? token : null;
  }

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
    const token = shellToken();
    if (!token) throw new Error("데스크톱 셸 호출 토큰이 없어 요청하지 않았습니다.");
    try {
      return await internals.invoke(command, args, { headers: { [TOKEN_HEADER]: token } });
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

  // Resolves `{ status: "saved", name, bytes }` (file name only, never a path) or `{ status: "cancelled" }`; rejects with an
  // Error whose message explains why the file was not written. `options.inPlace` asks Rust to write
  // back to the file this window's document came from (same format; Rust keeps the path and checks
  // the file did not change on disk); without it, or when Rust has no matching file, the dialog opens.
  async function saveFile(base64Data, fileName, options) {
    const res = await callShell(SAVE_COMMAND, {
      fileName: String(fileName),
      base64: String(base64Data),
      inPlace: Boolean(options?.inPlace),
    });
    if (res && (res.status === "saved" || res.status === "cancelled")) return res;
    throw new Error("저장 결과를 확인하지 못했습니다.");
  }

  // Searchable PDF of the Hangul editor's page SVGs (PD-101). The page sends SVG text only; Rust
  // builds the PDF and asks where to save it. Resolves like `saveFile`.
  async function exportSearchablePdf(fileName, pages) {
    const res = await callShell(EXPORT_PDF_COMMAND, { fileName: String(fileName), pages });
    if (res && (res.status === "saved" || res.status === "cancelled")) return res;
    throw new Error("PDF 저장 결과를 확인하지 못했습니다.");
  }
  const getUpdateNotice = () => callShell(UPDATE_COMMANDS.get, {});
  const setUpdateCheck = (enabled) => callShell(UPDATE_COMMANDS.set, { enabled: Boolean(enabled) });
  const openReleasePage = () => callShell(UPDATE_COMMANDS.open, {});

  async function windowCommand(key, args) {
    const command = WINDOW_COMMANDS[key];
    if (!command) throw new Error("알 수 없는 창 명령입니다: " + key);
    return callShell(command, args || {});
  }

  host.__EASYOFFICE_DESKTOP_BRIDGE__ = { isAvailable, invokeIpc, saveFile, exportSearchablePdf, getUpdateNotice, setUpdateCheck, openReleasePage, describeShellError, windowCommand, WINDOW_COMMANDS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
