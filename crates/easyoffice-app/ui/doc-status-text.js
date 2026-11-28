(function (host) {
  "use strict";

  // The wording of the document window's status bar. The word processor window shows what the Word program's status bar
  // shows (`N/M 페이지`, `N개 단어`, the word count opening the 단어 수 dialog); the Hangul and Pages windows keep the
  // position (단·줄·칸) and section words of their own products.

  function statusTexts({ word, current, total, cols, line, col, words, insertMode }) {
    if (word) {
      return {
        pos: current + "/" + total + " 페이지",
        zone: current + " / " + total + " 페이지 · " + insertMode,
        wordCount: words + "개 단어",
      };
    }
    return {
      pos: current + "/" + total + " 쪽   " + cols + "단   " + line + "줄   " + col + "칸 | 구역: 1 / 1 |",
      zone: current + " / " + total + " 쪽 · 구역: 1 / 1 · " + insertMode,
      wordCount: "",
    };
  }

  // Writes the position, the zone indicator and the word count button. `caretLineCol` is only asked outside the Word window.
  function render({ document, module, current, total, caretLineCol, insertMode }) {
    const editor = document.getElementById("doc-editor");
    const word = module === "DOC" && editor?.dataset?.eoDocFlavor !== "pages";
    const { line, col } = (!word && caretLineCol?.(editor)) || { line: 1, col: 1 };
    const words = word ? host.__EASYOFFICE_DOC_WORD_COUNT__.count(editor, { lines: false }).words : 0;
    const cols = Number.parseInt(editor?.style?.columnCount || "1", 10) || 1;
    const text = statusTexts({ word, current, total, cols, line, col, words, insertMode });
    for (const [id, value] of [["status-studio-pos", text.pos], ["doc-page-zone-indicator", text.zone], ["status-word-count", text.wordCount]]) {
      const el = document.getElementById(id);
      if (!el) continue;
      el.textContent = value;
      if (id === "status-word-count") el.hidden = !word;
    }
  }

  host.__EASYOFFICE_DOC_STATUS_TEXT__ = { statusTexts, render };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
