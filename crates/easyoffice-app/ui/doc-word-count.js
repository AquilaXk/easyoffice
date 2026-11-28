(function (host) {
  "use strict";

  // What the word processor window counts for the 단어 수 dialog and the status bar: words, characters with and
  // without spaces, paragraphs and lines of the body (tables included) and, when asked, of the footnotes and
  // endnotes. The page header and footer, the review comments and the format marks are not text of the document.
  // Words are runs of non-space characters (the Word program counts the same way); a paragraph is a block with no block
  // inside it, so a table cell holding paragraphs counts its paragraphs, not itself.

  const BLOCK_SELECTOR = "p,h1,h2,h3,h4,h5,h6,li,blockquote,pre,td,th,.eo-doc-note-body";
  const ALL_SPACES = /[\s 　﻿]+/gu;
  const SKIPPED_INLINE = ["eo-note-ref", "eo-page-field", "eo-doc-note-num"];

  const tagOf = (el) => String(el?.tagName || "").toUpperCase();
  const isMeta = (el) => tagOf(el) === "FOOTER" || el.id === "hwp-manuscript-preview" || Boolean(el.dataset && (el.dataset.eoHf !== undefined || el.dataset.eoComments !== undefined));
  const isNotes = (el) => Boolean(el.dataset && el.dataset.eoNotes !== undefined);

  function inScope(el, root, includeNotes) {
    for (let cur = el; cur && cur !== root; cur = cur.parentElement) {
      if (isMeta(cur)) return false;
      if (!includeNotes && isNotes(cur)) return false;
    }
    return true;
  }

  // The text of one block: the text nodes of it, without the note numbers and page fields drawn inside it.
  function blockText(block) {
    let text = "";
    const walk = (node) => {
      if (node.nodeType === 3) {
        text += String(node.nodeValue || "");
        return;
      }
      if (node.nodeType !== 1 || SKIPPED_INLINE.some((name) => node.classList?.contains(name))) return;
      if (tagOf(node) === "BR") {
        text += "\n";
        return;
      }
      for (const child of node.childNodes || []) walk(child);
    };
    for (const child of block.childNodes || []) walk(child);
    return text;
  }

  // Rendered lines of a block: the distinct tops of its client rects; without layout, its text lines.
  function lineCount(block, text) {
    const range = block.ownerDocument?.createRange?.();
    if (range && typeof range.getClientRects === "function") {
      range.selectNodeContents(block);
      const tops = new Set();
      for (const rect of range.getClientRects?.() || []) if (rect.height > 0) tops.add(Math.round(rect.top));
      if (tops.size > 0) return tops.size;
    }
    return text.replace(/\n$/u, "").split("\n").length;
  }

  function count(root, options = {}) {
    const stats = { words: 0, charsWithSpaces: 0, charsWithoutSpaces: 0, paragraphs: 0, lines: 0 };
    if (!root?.querySelectorAll) return stats;
    const includeNotes = options.includeNotes !== false;
    for (const block of root.querySelectorAll(BLOCK_SELECTOR)) {
      if (block.querySelector(BLOCK_SELECTOR) || !inScope(block, root, includeNotes)) continue;
      const text = blockText(block);
      const compact = text.replace(ALL_SPACES, "");
      stats.paragraphs += 1;
      stats.charsWithSpaces += Array.from(text.replace(/\n/gu, "")).length;
      stats.charsWithoutSpaces += Array.from(compact).length;
      const tokens = text.split(ALL_SPACES).filter(Boolean);
      stats.words += tokens.length;
      stats.lines += options.lines === false ? 0 : lineCount(block, text);
    }
    return stats;
  }

  host.__EASYOFFICE_DOC_WORD_COUNT__ = { count };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
