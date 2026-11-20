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

  function isCopilotAssistantPayload(command, payload) {
    return command === "run_smart_assistant" && /"action"\s*:\s*"copilot_/.test(String(payload ?? ""));
  }

  function createIpcTransport({ document, desktopBridge }) {
    function reportFailure(command, msg, payload) {
      const feedback = document.getElementById("status-command-feedback");
      if (feedback) feedback.textContent = REJECTED_PREFIX + msg;
      if (isCopilotAssistantPayload(command, payload)) return;
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
          // Marks the request as the page's own: a script on another origin cannot add this header
          // without a CORS preflight, which the local server never answers.
          headers: { "Content-Type": "application/json", "X-EasyOffice-Ipc": "1" },
          body: JSON.stringify({ command, payload: String(payload ?? "") }),
        });
      } catch {
        reportFailure(command, DISCONNECTED_MESSAGE, payload);
        throw connectionError(DISCONNECTED_MESSAGE);
      }
      if (!resp.ok) {
        const errBody = await resp.json().catch(() => ({}));
        const msg = errBody.error || errBody.message || REQUEST_FAILED_MESSAGE + " (" + resp.status + ")";
        reportFailure(command, msg, payload);
        throw reportedError(msg);
      }
      return await resp.json();
    }

    // Runs one transport and shows its failure.
    async function viaTransport(command, payload, run) {
      try {
        return await run();
      } catch (err) {
        const msg = err?.message || String(err);
        reportFailure(command, msg, payload);
        throw reportedError(msg);
      }
    }

    // Only the desktop shell can stream an answer (the test hook says so with `streaming`); the `--serve` preview answers whole.
    function canStream() {
      if (typeof host.__EASYOFFICE_TEST_IPC__ === "function") return host.__EASYOFFICE_TEST_IPC__.streaming === true;
      return desktopBridge.isAvailable() && desktopBridge.canStream?.() === true;
    }

    // `options.stream = { onPiece }` asks for the model's text pieces while the request runs; the answer itself still arrives whole.
    async function invokeIpc(command, payload, options) {
      if (typeof host.__EASYOFFICE_TEST_IPC__ === "function") {
        return viaTransport(command, payload, async () => {
          const raw = await host.__EASYOFFICE_TEST_IPC__(command, payload, options);
          return typeof raw === "string" ? JSON.parse(raw) : raw;
        });
      }
      if (desktopBridge.isAvailable()) {
        return viaTransport(command, payload, () => desktopBridge.invokeIpc(command, payload, options));
      }
      if (isHttpAvailable()) {
        return invokeHttp(command, payload);
      }
      reportFailure(command, DISCONNECTED_MESSAGE, payload);
      throw connectionError(DISCONNECTED_MESSAGE);
    }

    return { invokeIpc, canStream };
  }

  host.__EASYOFFICE_CREATE_IPC_TRANSPORT__ = createIpcTransport;
  host.__EASYOFFICE_IPC_TRANSPORT_UTILS__ = { DISCONNECTED_MESSAGE, REQUEST_FAILED_MESSAGE, REJECTED_PREFIX };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
