(function (host) {
  "use strict";

  // Engine request transport. Every engine call from the page goes through one of three paths, chosen
  // per call: the test hook (`__EASYOFFICE_TEST_IPC__`), the desktop shell bridge, or the local
  // HTTP endpoint of the `--serve` preview. Failures are shown next to the control that asked
  // (`reportFailure`) and rethrown as Errors with `reported = true` (callers need not show them again); connection failures also carry `connection = true`.

  const DISCONNECTED_MESSAGE = "연결할 수 없습니다. 앱을 다시 시작해 주세요.";
  const REQUEST_FAILED_MESSAGE = "요청을 처리하지 못했습니다. 다시 시도해 주세요.";
  const REJECTED_PREFIX = "[요청 거부] ";
  const RESULT_TARGETS = Object.freeze({
    convert_document_format: "conv-result-box",
    audit_security: "security-status-text",
    scrub_pii: "pii-result-text",
    run_smart_assistant: "smart-summary-output",
    run_engine_benchmark: "bench-result-box",
  });

  function reportedError(message) {
    const err = new Error(message);
    err.reported = true;
    return err;
  }

  function connectionError(message) {
    const err = reportedError(message);
    err.connection = true;
    return err;
  }

  function createIpcTransport({ document, desktopBridge }) {
    function setEngineStatus(text) {
      const engineEl = document.getElementById("status-engine");
      if (engineEl) engineEl.textContent = text;
    }

    function reportFailure(command, msg) {
      const feedback = document.getElementById("status-command-feedback");
      if (feedback) feedback.textContent = REJECTED_PREFIX + msg;
      const targetId = RESULT_TARGETS[command];
      const target = targetId ? document.getElementById(targetId) : null;
      if (target) target.textContent = REJECTED_PREFIX + msg;
    }

    function isHttpAvailable() {
      const proto = host.location?.protocol;
      return (proto === "http:" || proto === "https:") && typeof host.fetch === "function";
    }

    async function invokeHttp(command, payload) {
      let resp;
      try {
        resp = await host.fetch("/api/ipc", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ command, payload: String(payload ?? "") }),
        });
      } catch {
        setEngineStatus("연결 오류");
        reportFailure(command, DISCONNECTED_MESSAGE);
        throw connectionError(DISCONNECTED_MESSAGE);
      }
      if (!resp.ok) {
        const errBody = await resp.json().catch(() => ({}));
        const msg = errBody.error || errBody.message || REQUEST_FAILED_MESSAGE + " (" + resp.status + ")";
        reportFailure(command, msg);
        throw reportedError(msg);
      }
      return await resp.json();
    }

    // Runs one transport and keeps the status label and the failure display in step.
    async function viaTransport(command, run, statusText) {
      try {
        const res = await run();
        setEngineStatus(statusText);
        return res;
      } catch (err) {
        const msg = err?.message || String(err);
        reportFailure(command, msg);
        throw reportedError(msg);
      }
    }

    async function invokeIpc(command, payload) {
      if (typeof host.__EASYOFFICE_TEST_IPC__ === "function") {
        return viaTransport(command, async () => {
          const raw = await host.__EASYOFFICE_TEST_IPC__(command, payload);
          return typeof raw === "string" ? JSON.parse(raw) : raw;
        }, "연결됨");
      }
      if (desktopBridge.isAvailable()) {
        return viaTransport(command, () => desktopBridge.invokeIpc(command, payload), "연결됨");
      }
      if (isHttpAvailable()) {
        const res = await invokeHttp(command, payload);
        setEngineStatus("연결됨");
        return res;
      }
      setEngineStatus("연결 안 됨 · " + DISCONNECTED_MESSAGE);
      reportFailure(command, DISCONNECTED_MESSAGE);
      throw connectionError(DISCONNECTED_MESSAGE);
    }

    return { invokeIpc };
  }

  host.__EASYOFFICE_CREATE_IPC_TRANSPORT__ = createIpcTransport;
  host.__EASYOFFICE_IPC_TRANSPORT_UTILS__ = { DISCONNECTED_MESSAGE, REQUEST_FAILED_MESSAGE, REJECTED_PREFIX };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
