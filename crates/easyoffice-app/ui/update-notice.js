(function (host) {
  "use strict";

  // Update notice (PD-103): a small card in the bottom left corner of a window when a newer release
  // exists, with a button that installs it and restarts the app, and one that opens the release page
  // in the default browser. The shell does everything else (the daily check, the signed download,
  // the refusal while any window has unsaved changes, the install and the restart); this module asks
  // it to start, shows the state it reports and polls it while a download or install runs.
  //
  // Adapted from the reference app `apps/studio-host/src/ui/update-notice.ts` (MIT, Copyright (c) 2025-2026
  // Edward Kim; see THIRD_PARTY_NOTICES.md): the same card structure and class names (`update-notice`,
  // `update-notice__title|body|meta|error|actions|button`), the same states (available, downloading,
  // error), the progress text and the "dismissed per version" rule. TypeScript rewritten as
  // JavaScript; the page polls the shell instead of receiving an event; the install and the restart
  // are one action ("설치하고 다시 시작"), so there is no separate ready state; added: the release
  // page button (installations the signed install does not cover) and an "installed" state for a
  // restart held back by unsaved changes.

  // The first look waits until the window has finished starting; the shell checks in the background
  // right after start, so it looks again once more when the network answer has had time to arrive.
  const FIRST_LOOK_DELAY_MS = 3000;
  const RECHECK_DELAY_MS = 15000;
  // While a download or install runs the shell is asked for the progress this often.
  const WATCH_INTERVAL_MS = 700;
  const ESCAPE_KEY = "Escape";
  const STATUSES = ["available", "downloading", "installing", "installed", "error"];
  // A card the user may hide; the progress of a running install stays visible.
  const DISMISSIBLE = ["available", "installed", "error"];

  function el(document, tag, className) {
    const node = document.createElement(tag);
    node.className = className;
    return node;
  }

  function createUpdateNotice(deps) {
    const { document, bridge } = deps;
    const env = deps.env || host;
    const root = el(document, "div", "update-notice");
    root.hidden = true;
    root.setAttribute("role", "region");
    root.setAttribute("aria-label", "업데이트 알림");
    const title = el(document, "h2", "update-notice__title");
    const body = el(document, "p", "update-notice__body");
    const meta = el(document, "p", "update-notice__meta");
    const error = el(document, "p", "update-notice__error");
    error.hidden = true;
    const actions = el(document, "div", "update-notice__actions");
    for (const child of [title, body, meta, error, actions]) root.appendChild(child);

    let view = { status: "idle", version: null, enabled: true, supported: false, downloadedBytes: null, totalBytes: null, message: null };
    let dismissedKey = null;
    let pending = false;
    let watching = false;

    const noticeKey = () => (DISMISSIBLE.includes(view.status) ? view.status + ":" + view.version : null);

    function button(label, primary, onClick) {
      const node = el(document, "button", "update-notice__button" + (primary ? " update-notice__button--primary" : ""));
      node.type = "button";
      node.textContent = label;
      node.disabled = pending;
      node.addEventListener("click", onClick);
      return node;
    }

    function dismiss() {
      if (!DISMISSIBLE.includes(view.status)) return;
      dismissedKey = noticeKey();
      root.hidden = true;
    }

    function showError(err) {
      error.textContent = err?.message || String(err);
      error.hidden = false;
    }

    async function openReleasePage() {
      if (pending) return;
      pending = true;
      error.hidden = true;
      render();
      try {
        await bridge.openReleasePage();
      } catch (err) {
        showError(err);
      }
      pending = false;
      render();
    }

    // The shell refuses while a window has unsaved changes and says so; that text is shown here.
    async function installUpdate() {
      if (pending) return;
      pending = true;
      error.hidden = true;
      render();
      try {
        setView(await bridge.installUpdate());
      } catch (err) {
        showError(err);
      }
      pending = false;
      render();
    }

    function formatBytes(bytes) {
      if (bytes < 1024) return bytes + " B";
      if (bytes < 1024 * 1024) return Math.round(bytes / 1024) + " KB";
      return (bytes / (1024 * 1024)).toFixed(1) + " MB";
    }

    function progressText() {
      const done = view.downloadedBytes;
      if (done === null) return "";
      if (view.totalBytes && view.totalBytes > 0) {
        const percent = Math.max(0, Math.min(100, Math.round((done / view.totalBytes) * 100)));
        return formatBytes(done) + " / " + formatBytes(view.totalBytes) + " (" + percent + "%)";
      }
      return formatBytes(done) + " 다운로드됨";
    }

    function setText(node, text) {
      node.textContent = text || "";
      node.hidden = !text;
    }

    function renderToggles() {
      for (const item of document.querySelectorAll("[data-update-check-toggle]")) {
        item.hidden = !view.supported;
        item.setAttribute("aria-checked", String(view.enabled));
      }
    }

    function render() {
      renderToggles();
      const key = noticeKey();
      const shown = view.status !== "idle" && (key === null || key !== dismissedKey);
      if (!shown) {
        root.hidden = true;
        return;
      }
      root.hidden = false;
      root.dataset.state = view.status;
      const releaseButton = () => button("릴리스 페이지", false, () => void openReleasePage());
      const laterButton = (label) => button(label, false, dismiss);
      if (view.status === "available") {
        title.textContent = "새 버전 " + view.version + " 사용 가능";
        body.textContent = "";
        setText(meta, "");
        actions.replaceChildren(button("설치하고 다시 시작", true, () => void installUpdate()), releaseButton(), laterButton("나중에"));
      } else if (view.status === "downloading") {
        title.textContent = "버전 " + view.version + " 준비 중";
        body.textContent = "업데이트 파일을 내려받고 있습니다.";
        setText(meta, progressText());
        actions.replaceChildren();
      } else if (view.status === "installing") {
        title.textContent = "버전 " + view.version + " 설치 중";
        body.textContent = "설치가 끝나면 앱이 다시 시작됩니다. 잠시 기다려 주세요.";
        setText(meta, "");
        actions.replaceChildren();
      } else if (view.status === "installed") {
        title.textContent = "버전 " + view.version + " 설치 완료";
        body.textContent = "저장하지 않은 변경 사항이 있어 다시 시작하지 않았습니다. 저장한 뒤 앱을 다시 시작하세요.";
        setText(meta, "");
        actions.replaceChildren(laterButton("닫기"));
      } else {
        title.textContent = "버전 " + view.version + " 업데이트 실패";
        body.textContent = view.message || "업데이트를 설치하지 못했습니다.";
        setText(meta, "");
        actions.replaceChildren(button("다시 시도", true, () => void installUpdate()), releaseButton(), laterButton("닫기"));
      }
      watch();
    }

    function setView(next) {
      const count = (value) => (Number.isFinite(value) && value >= 0 ? value : null);
      view = {
        status: STATUSES.includes(next?.status) && typeof next?.version === "string" ? next.status : "idle",
        version: typeof next?.version === "string" ? next.version : null,
        enabled: next?.enabled !== false,
        supported: next?.supported === true,
        downloadedBytes: count(next?.downloadedBytes),
        totalBytes: count(next?.totalBytes),
        message: typeof next?.message === "string" ? next.message : null,
      };
      render();
    }

    // Looks again soon while a download or install runs (one timer at a time).
    function watch() {
      if (watching || (view.status !== "downloading" && view.status !== "installing")) return;
      watching = true;
      env.setTimeout(() => {
        watching = false;
        void refresh();
      }, WATCH_INTERVAL_MS);
    }

    async function refresh() {
      try {
        setView(await bridge.getUpdateNotice());
      } catch {
        setView(null);
      }
    }

    async function toggleCheck() {
      try {
        setView(await bridge.setUpdateCheck(!view.enabled));
      } catch {
        // the switch stays as it was
      }
    }

    function bind() {
      document.body.appendChild(root);
      root.addEventListener("keydown", (event) => {
        if (event.key === ESCAPE_KEY) dismiss();
      });
      for (const item of document.querySelectorAll("[data-update-check-toggle]")) {
        item.addEventListener("click", () => {
          for (const open of document.querySelectorAll(".menu-item.open")) open.classList.remove("open");
          void toggleCheck();
        });
      }
      env.setTimeout(() => void refresh(), FIRST_LOOK_DELAY_MS);
      env.setTimeout(() => void refresh(), RECHECK_DELAY_MS);
      env.addEventListener?.("focus", () => void refresh());
    }

    return { bind, refresh, getView: () => ({ ...view }), root };
  }

  host.__EASYOFFICE_CREATE_UPDATE_NOTICE__ = createUpdateNotice;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
