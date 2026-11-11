(function (host) {
  "use strict";

  // Windows desktop shell bridge (Tauri v2 / WebView2, `crates/easyoffice-desktop`). Inside the
  // shell every engine request goes through the in-process `ipc_dispatch` command, and saving
  // goes through `save_document_file` (native save dialog + write in Rust). Outside the shell
  // (`--serve` preview, tests) `isAvailable()` is false and callers keep their HTTP/Blob paths.

  const DISCONNECTED_MESSAGE = "연결할 수 없습니다. 앱을 다시 시작해 주세요.";
  const IPC_COMMAND = "ipc_dispatch";
  const SAVE_COMMAND = "save_document_file";
  // Searchable PDF export of the Hangul editor (PD-123): choose the file, send the pages in batches, finish.
  const PDF_EXPORT_COMMANDS = Object.freeze({
    start: "pdf_export_start",
    pages: "pdf_export_pages",
    finish: "pdf_export_finish",
  });
  // Editing the PDF a window opened, on its objects (PD-156): the shell keeps the file after `open`.
  const PDF_EDIT_COMMANDS = Object.freeze({
    open: "pdf_edit_open",
    apply: "pdf_edit_apply",
    splitMerge: "pdf_edit_split_merge",
    save: "pdf_edit_save",
  });
  // Installed fonts for the Hangul editor (PD-128). The list holds names, styles and numbers, never a path;
  // a read quotes the revision of the list and a file number, and gets the font file as an ArrayBuffer.
  const SYSTEM_FONT_COMMANDS = Object.freeze({ list: "list_system_fonts", read: "read_system_font" });
  // Update notice (PD-103): the shell checks for a newer release and installs it (signed package);
  // the page only reads the state, switches the check on or off and asks the shell to start the
  // install or to open the release page (it passes no address and no package).
  const UPDATE_COMMANDS = Object.freeze({
    get: "get_update_notice",
    set: "set_update_check",
    open: "open_release_page",
    install: "install_update",
  });
  // Window commands (PD-088). None of them takes or returns a file system path: the Rust side owns
  // the native dialogs, file reads and the path registry; the page only sees names, kinds and bytes.
  const WINDOW_COMMANDS = Object.freeze({
    openDocumentWindow: "open_document_window",
    openFileInNewWindow: "open_file_in_new_window",
    takePendingOpen: "take_pending_open",
    reportWindowState: "report_window_state",
    closeCurrentWindow: "close_current_window",
    quitApp: "quit_app",
    listRecentDocuments: "list_recent_documents",
    clearRecentDocuments: "clear_recent_documents",
    openRecentDocument: "open_recent_document",
    recentPreview: "get_recent_preview",
    reportMenuState: "report_menu_state",
    endClosePrompt: "end_close_prompt",
    ackClosePrompt: "ack_close_prompt",
    confirmDialog: "confirm_dialog",
    confirmClose: "confirm_close",
    printWebview: "print_webview",
  });

  // Confirmation kinds (`confirm_dialog.rs`, PD-120). The desktop shell shows its own native dialog
  // with its own wording; this text is only for the `--serve` browser preview, where `window.confirm`
  // works. A test keeps it identical to the Rust wording.
  const CONFIRM_BROWSER_TEXT = Object.freeze({
    "password-free-save": "저장한 HWP 파일에는 암호가 적용되지 않습니다. 암호 없이 저장하시겠습니까?",
    "replace-unsaved-hwp": "저장하지 않은 한글 문서 편집 내용이 있습니다.\n다른 문서를 열면 현재 내용이 사라집니다. 계속할까요?",
    "hwpx-fallback-hwp": "HWPX로 저장하기 전 검증에 실패했습니다.\nHWP 형식으로 대신 저장할까요?",
    "restore-hwp-autosave": "이전 세션의 저장하지 않은 한글 문서의 자동 저장본이 있습니다. 복원할까요?\n복원하지 않으면 자동 저장본은 남겨 두고 다음에 다시 묻습니다.",
    "replace-unsaved-module": "이 모듈에 저장하지 않은 편집 내용이 있습니다.\n파일을 열면 현재 내용이 바뀝니다 (실행 취소로 되돌릴 수 있습니다). 계속할까요?",
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
    return "요청을 처리하지 못했습니다. 다시 시도해 주세요.";
  }

  async function callShell(command, args) {
    const internals = shellInternals();
    if (!internals) throw new Error(DISCONNECTED_MESSAGE);
    const token = shellToken();
    if (!token) throw new Error(DISCONNECTED_MESSAGE);
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
      throw new Error("응답을 처리하지 못했습니다. 다시 시도해 주세요.");
    }
  }

  // Resolves `{ status: "saved", name, bytes }` (file name only, never a path) or `{ status: "cancelled" }`; rejects with an
  // Error whose message explains why the file was not written. `options.inPlace` asks Rust to write
  // back to the file this window's document came from (same format; Rust keeps the path and checks
  // the file did not change on disk); without it, or when Rust has no matching file, the dialog opens.
  // `options.thumbnailSvg` is the first page of a Hangul document: Rust draws it small and stores it as
  // the file's preview picture (PD-132), and saves the document without one if that fails.
  async function saveFile(base64Data, fileName, options) {
    const args = {
      fileName: String(fileName),
      base64: String(base64Data),
      inPlace: Boolean(options?.inPlace),
    };
    if (typeof options?.thumbnailSvg === "string" && options.thumbnailSvg) args.thumbnailSvg = options.thumbnailSvg;
    const res = await callShell(SAVE_COMMAND, args);
    if (res && (res.status === "saved" || res.status === "cancelled")) return res;
    throw new Error("저장 결과를 확인하지 못했습니다.");
  }

  // Searchable PDF of the Hangul editor's page SVGs (PD-101, PD-123). Rust owns the save dialog, the
  // file and the OS "open"; the page sends a file name, then SVG text a batch at a time, then asks
  // to finish. Nothing here takes or returns a path.
  //   start(fileName)       -> { status: "started", jobId } | { status: "cancelled" } (no location chosen)
  //   pages(jobId, svgs)    -> { pages } (how many the PDF holds now; at most 10 svgs per call)
  //   finish(jobId, commit) -> { status: "saved", name, bytes, pages, opened, skippedPictures } | { status: "cancelled" }
  //                            (skippedPictures: pictures left out of the pages for being over the pixel limits)
  //                            (commit false drops the export; both are safe to repeat)
  const pdfExport = Object.freeze({
    async start(fileName) {
      const res = await callShell(PDF_EXPORT_COMMANDS.start, { fileName: String(fileName) });
      if (res?.status === "cancelled") return { status: "cancelled" };
      if (res?.status === "started" && Number.isSafeInteger(res.jobId)) return { status: "started", jobId: res.jobId };
      throw new Error("PDF 저장 위치 확인 결과를 받지 못했습니다.");
    },
    async pages(jobId, svgs) {
      const res = await callShell(PDF_EXPORT_COMMANDS.pages, { jobId, pages: svgs });
      if (Number.isSafeInteger(res?.pages)) return res;
      throw new Error("PDF 쪽 변환 결과를 확인하지 못했습니다.");
    },
    async finish(jobId, commit) {
      const res = await callShell(PDF_EXPORT_COMMANDS.finish, { jobId, commit: Boolean(commit) });
      if (res && (res.status === "saved" || res.status === "cancelled")) return res;
      throw new Error("PDF 저장 결과를 확인하지 못했습니다.");
    },
  });
  // The PDF studio on the opened file's own objects (PD-156). Rust owns the file, every dialog and the
  // limits; the page sends the opened bytes once and gets names, counts and sentences back.
  //   open(fileName, base64)  -> { pages, sources, turn, stamp, watermark } | rejects with the reason (encrypted, over a limit, damaged)
  //   apply(op)               -> the same state; op is { kind: "turn", degrees } (every page), { kind: "rotate", degrees, pages? }
  //                              or { kind: "overlay", stamp, watermark } (empty text clears)
  //   splitMerge()            -> { action: "merged" | "split" | "cancelled", count, pages, ... } (Rust asks which, and where)
  //   save()                  -> { status: "saved", name, bytes, pages } | { status: "cancelled" }
  const pdfEdit = Object.freeze({
    async open(fileName, base64) {
      const res = await callShell(PDF_EDIT_COMMANDS.open, { fileName: String(fileName), base64: String(base64) });
      if (Number.isSafeInteger(res?.pages)) return res;
      throw new Error("PDF 열기 결과를 확인하지 못했습니다.");
    },
    async apply(op) {
      const res = await callShell(PDF_EDIT_COMMANDS.apply, { op });
      if (Number.isSafeInteger(res?.pages)) return res;
      throw new Error("PDF 편집 결과를 확인하지 못했습니다.");
    },
    async splitMerge() {
      const res = await callShell(PDF_EDIT_COMMANDS.splitMerge, {});
      if (res && ["merged", "split", "cancelled"].includes(res.action) && Number.isSafeInteger(res.count)) return res;
      throw new Error("분할·병합 결과를 확인하지 못했습니다.");
    },
    async save() {
      const res = await callShell(PDF_EDIT_COMMANDS.save, {});
      if (res && (res.status === "saved" || res.status === "cancelled")) return res;
      throw new Error("PDF 저장 결과를 확인하지 못했습니다.");
    },
  });
  const systemFonts = Object.freeze({
    list: () => callShell(SYSTEM_FONT_COMMANDS.list, {}),
    read: (revision, file) => callShell(SYSTEM_FONT_COMMANDS.read, { revision: String(revision), file }),
  });
  const getUpdateNotice = () => callShell(UPDATE_COMMANDS.get, {});
  const setUpdateCheck = (enabled) => callShell(UPDATE_COMMANDS.set, { enabled: Boolean(enabled) });
  const openReleasePage = () => callShell(UPDATE_COMMANDS.open, {});
  const installUpdate = () => callShell(UPDATE_COMMANDS.install, {});

  // Resolves whether the user accepted the named confirmation. Never true without an answer: a
  // refused call rejects, an unknown kind or a missing browser dialog answers no.
  async function confirm(kind) {
    if (isAvailable()) {
      const answer = await callShell(WINDOW_COMMANDS.confirmDialog, { kind: String(kind) });
      return answer === true;
    }
    const text = Object.hasOwn(CONFIRM_BROWSER_TEXT, kind) ? CONFIRM_BROWSER_TEXT[kind] : "";
    return text !== "" && typeof host.confirm === "function" && host.confirm(text) === true;
  }

  async function windowCommand(key, args) {
    const command = WINDOW_COMMANDS[key];
    if (!command) throw new Error("알 수 없는 창 명령입니다: " + key);
    return callShell(command, args || {});
  }

  host.__EASYOFFICE_DESKTOP_BRIDGE__ = { isAvailable, invokeIpc, saveFile, pdfExport, pdfEdit, systemFonts, getUpdateNotice, setUpdateCheck, openReleasePage, installUpdate, describeShellError, windowCommand, confirm, CONFIRM_BROWSER_TEXT, WINDOW_COMMANDS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
