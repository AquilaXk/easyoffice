(function (host) {
  "use strict";

  // 보기 > 여러 페이지 of the word processor window (PD-401): the pages of the document side by side, each one whole. The view is
  // the print preview's page layout (`print-preview.js` and `doc-page-print.js`), which already breaks the pages where the
  // screen does and draws the header, the footer and the page number: this command opens it in its two-page arrangement at the
  // fit-page zoom, and a second use closes it. Reuse (구현 원칙 3단계): (1) no library has the layout; (2) the print preview is
  // reused as it is; (3) nothing new but the command.

  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  async function toggleMultiPage(env) {
    const preview = host.__EASYOFFICE_PRINT_PREVIEW__;
    if (!preview) return feedback(env, "여러 페이지 보기를 쓸 수 없습니다");
    if (preview.isOpen()) {
      preview.close();
      return feedback(env, "여러 페이지 보기를 닫았습니다");
    }
    preview.setArrangement("spread");
    preview.setZoomMode("fit-page");
    if (await preview.open()) feedback(env, "여러 페이지 보기");
  }

  host.__EASYOFFICE_DOC_WORD_COMMANDS__?.register("view-multipage", toggleMultiPage);
  host.__EASYOFFICE_DOC_VIEW_PAGES__ = { toggleMultiPage };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
