(function (host) {
  "use strict";

  // Update notice (PD-103): a small card in the top right corner of a window when a newer release
  // exists, with a button that opens the release page in the default browser. The shell does the
  // checking (once a day, in Rust); this module only asks it for the state and shows it. There is no
  // download or install here: the shell has no update signing key.
  //
  // Adapted from the reference app `apps/studio-host/src/ui/update-notice.ts` (MIT, Copyright (c) 2025-2026
  // Edward Kim; see THIRD_PARTY_NOTICES.md): the same card structure and class names (`update-notice`,
  // `update-notice__title|body|meta|error|actions|button`), the same "dismissed per version" rule.
  // TypeScript rewritten as JavaScript, only the idle and available states, and an "open the release
  // page" action instead of the reference app's install and restart actions.

  // The first look waits until the window has finished starting; the shell checks in the background
  // right after start, so it looks again once more when the network answer has had time to arrive.
  const FIRST_LOOK_DELAY_MS = 3000;
  const RECHECK_DELAY_MS = 15000;
  const ESCAPE_KEY = "Escape";

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

    let view = { status: "idle", version: null, enabled: true, supported: false };
    let dismissedKey = null;
    let pending = false;

    const noticeKey = () => (view.status === "available" ? "available:" + view.version : null);

    function button(label, primary, onClick) {
      const node = el(document, "button", "update-notice__button" + (primary ? " update-notice__button--primary" : ""));
      node.type = "button";
      node.textContent = label;
      node.disabled = pending;
      node.addEventListener("click", onClick);
      return node;
    }

    function dismiss() {
      dismissedKey = noticeKey();
      root.hidden = true;
    }

    async function openReleasePage() {
      if (pending) return;
      pending = true;
      error.hidden = true;
      render();
      try {
        await bridge.openReleasePage();
      } catch (err) {
        error.textContent = err?.message || String(err);
        error.hidden = false;
      }
      pending = false;
      render();
    }

    function renderToggles() {
      for (const item of document.querySelectorAll("[data-update-check-toggle]")) {
        item.hidden = !view.supported;
        item.setAttribute("aria-checked", String(view.enabled));
        const mark = item.querySelector(".md-icon");
        if (mark) mark.textContent = view.enabled ? "✓" : "";
      }
    }

    function render() {
      renderToggles();
      const key = noticeKey();
      if (!key || key === dismissedKey) {
        root.hidden = true;
        return;
      }
      root.hidden = false;
      root.dataset.state = view.status;
      title.textContent = "새 버전 " + view.version + " 사용 가능";
      body.textContent = "다운로드 페이지에서 받아 설치할 수 있습니다.";
      meta.textContent = "업데이트는 자동으로 설치되지 않습니다.";
      actions.replaceChildren(button("다운로드 페이지 열기", true, () => void openReleasePage()), button("나중에", false, dismiss));
    }

    function setView(next) {
      view = {
        status: next?.status === "available" ? "available" : "idle",
        version: typeof next?.version === "string" ? next.version : null,
        enabled: next?.enabled !== false,
        supported: next?.supported === true,
      };
      render();
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
