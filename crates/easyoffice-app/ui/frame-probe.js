// Smoke-only probe for `EasyOffice.exe --smoke` step `frame_ipc` (PD-083). hwp-host.js loads
// this page in a frame with the editor's sandbox ("allow-scripts allow-modals", opaque origin) and the
// editor's response policy (`studio_frame.rs`), so it sees what a compromised editor would see.
// It tries to run a harmless app command (a read) with whatever IPC objects the webview put into
// the frame and reports the outcome to the shell by postMessage:
//   internals  "present" when the frame has its own Tauri IPC objects (Windows injects them into every frame)
//   webview    "present" when the WebView2 message channel exists in the frame
//   invoke     "absent" (no IPC objects), "rejected" (the shell answered with an error),
//              "no-answer" (nothing came back in time) or "allowed" (the command ran: the step fails)
(() => {
  const ANSWER_TIMEOUT_MS = 5000;
  const internals = window.__TAURI_INTERNALS__;
  const canInvoke = Boolean(internals) && typeof internals.invoke === "function";

  async function tryCommand() {
    if (!canInvoke) return "absent";
    const answer = internals.invoke("get_update_notice", {}).then(() => "allowed", () => "rejected");
    const silence = new Promise((resolve) => window.setTimeout(() => resolve("no-answer"), ANSWER_TIMEOUT_MS));
    return Promise.race([answer, silence]);
  }

  void tryCommand().then((invoke) => {
    window.parent.postMessage({
      type: "eo-frame-probe",
      internals: internals ? "present" : "absent",
      webview: window.chrome && window.chrome.webview ? "present" : "absent",
      invoke,
    }, "*");
  });
})();
