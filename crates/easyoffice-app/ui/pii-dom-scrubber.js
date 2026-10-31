(function (host) {
  "use strict";

  const DOC_BLOCK_TAGS = new Set([
    "P",
    "DIV",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "LI",
    "UL",
    "OL",
    "BLOCKQUOTE",
    "SECTION",
    "ARTICLE",
    "PRE",
  ]);

  const INLINE_SCRUB_BLOCK_TAGS = new Set([
    "P",
    "DIV",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "LI",
    "TD",
    "TH",
    "BLOCKQUOTE",
    "SECTION",
    "ARTICLE",
    "PRE",
    "DT",
    "DD",
    "FIGCAPTION",
  ]);

  function isExcludedNonTableDocChrome(el) {
    if (!el || el.nodeType !== 1) return false;
    const tag = (el.tagName || "").toUpperCase();
    if (tag === "FOOTER") return true;
    if (el.id === "hwp-manuscript-preview") return true;
    if (el.classList && el.classList.contains("doc-table-toolbar")) return true;
    return false;
  }

  function isExcludedDocChromeElement(el) {
    if (!el || el.nodeType !== 1) return false;
    const tag = (el.tagName || "").toUpperCase();
    if (tag === "TABLE") return true;
    return isExcludedNonTableDocChrome(el);
  }

  function collectEditableInlineTokens(rootEl, options = {}) {
    const includeTables = Boolean(options.includeTables);
    const out = [];
    function walk(node) {
      if (!node) return;
      if (node.nodeType === 3) {
        out.push({ type: "text", node });
        return;
      }
      if (node.nodeType !== 1) return;
      if (isExcludedNonTableDocChrome(node)) return;
      if (node.id === "doc-table-sum-row" || node.id === "doc-table-sum-cell") return;
      const tag = (node.tagName || "").toUpperCase();
      if (tag === "BR") {
        out.push({ type: "br" });
        return;
      }
      if (tag === "TABLE" && !includeTables) return;
      Array.from(node.childNodes || []).forEach(walk);
    }
    walk(rootEl);
    return out;
  }

  function findNearestScrubBlock(textNode, rootEl) {
    let cur = textNode.parentElement;
    while (cur && cur !== rootEl) {
      if (INLINE_SCRUB_BLOCK_TAGS.has((cur.tagName || "").toUpperCase())) {
        return cur;
      }
      cur = cur.parentElement;
    }
    return rootEl;
  }

  function deriveSingleDiffSpan(origText, scrubbedText) {
    let prefixLen = 0;
    while (
      prefixLen < origText.length &&
      prefixLen < scrubbedText.length &&
      origText[prefixLen] === scrubbedText[prefixLen]
    ) {
      prefixLen += 1;
    }
    let suffixLen = 0;
    while (
      suffixLen < origText.length - prefixLen &&
      suffixLen < scrubbedText.length - prefixLen &&
      origText[origText.length - 1 - suffixLen] ===
        scrubbedText[scrubbedText.length - 1 - suffixLen]
    ) {
      suffixLen += 1;
    }
    return [
      {
        start: prefixLen,
        end: origText.length - suffixLen,
        replacement: scrubbedText.slice(prefixLen, scrubbedText.length - suffixLen),
      },
    ];
  }

  function tokenizeFormulaStringLiterals(formula) {
    const segments = [];
    let i = 0;
    let rawStart = 0;
    while (i < formula.length) {
      const ch = formula[i];
      if (ch === "'") {
        i += 1;
        while (i < formula.length) {
          if (formula[i] === "'") {
            if (i + 1 < formula.length && formula[i + 1] === "'") {
              i += 2;
              continue;
            }
            i += 1;
            break;
          }
          i += 1;
        }
        continue;
      }
      if (ch === '"') {
        if (i > rawStart) {
          segments.push({ isLiteral: false, text: formula.slice(rawStart, i) });
        }
        let j = i + 1;
        let inner = "";
        let closed = false;
        while (j < formula.length) {
          if (formula[j] === '"') {
            if (j + 1 < formula.length && formula[j + 1] === '"') {
              inner += '"';
              j += 2;
              continue;
            }
            closed = true;
            j += 1;
            break;
          }
          inner += formula[j];
          j += 1;
        }
        if (closed) {
          segments.push({ isLiteral: true, text: inner });
          i = j;
          rawStart = j;
        } else {
          break;
        }
        continue;
      }
      i += 1;
    }
    if (rawStart < formula.length) {
      segments.push({ isLiteral: false, text: formula.slice(rawStart) });
    }
    return segments;
  }

  function createPiiDomScrubber(ctx) {
    const { document, localScrubPii } = ctx;

    function resolveScrubSpans(origText, res) {
      if (Array.isArray(res?.spans) && res.spans.length > 0) {
        const valid = res.spans.filter(
          (sp) =>
            sp &&
            Number.isInteger(sp.start) &&
            Number.isInteger(sp.end) &&
            sp.start >= 0 &&
            sp.end > sp.start &&
            sp.end <= origText.length &&
            typeof sp.replacement === "string"
        );
        if (valid.length > 0) return valid;
      }
      if (typeof localScrubPii === "function") {
        const localRes = localScrubPii(origText);
        if (localRes.scrubbedText === res.scrubbedText && localRes.spans.length > 0) {
          return localRes.spans;
        }
      }
      return deriveSingleDiffSpan(origText, res.scrubbedText);
    }

    function applyScrubbedSpansToInlineGroup(nodes, origText, res) {
      if (nodes.length === 1) {
        nodes[0].nodeValue = res.scrubbedText;
      } else {
        const spans = resolveScrubSpans(origText, res);
        const emittedSpans = new Set();
        let offset = 0;
        nodes.forEach((node) => {
          const text = node.nodeValue || "";
          const nodeStart = offset;
          const nodeEnd = offset + text.length;
          offset = nodeEnd;
          let cursor = nodeStart;
          let rebuilt = "";
          for (let sIdx = 0; sIdx < spans.length; sIdx += 1) {
            const sp = spans[sIdx];
            if (sp.end <= nodeStart) continue;
            if (sp.start >= nodeEnd) break;
            if (sp.start > cursor) {
              rebuilt += text.slice(cursor - nodeStart, sp.start - nodeStart);
            }
            if (!emittedSpans.has(sIdx)) {
              rebuilt += sp.replacement;
              emittedSpans.add(sIdx);
            }
            cursor = Math.min(nodeEnd, sp.end);
          }
          if (cursor < nodeEnd) {
            rebuilt += text.slice(cursor - nodeStart, nodeEnd - nodeStart);
          }
          node.nodeValue = rebuilt;
        });
      }
      nodes.forEach((node) => {
        const numCell = node.parentElement?.closest?.("[data-doc-num]");
        if (numCell && numCell.id !== "doc-table-sum-cell") {
          const raw = (numCell.textContent || "").trim().replaceAll(",", "");
          const parsed = Number.parseFloat(raw);
          numCell.dataset.docNum = Number.isNaN(parsed) ? "0" : String(parsed);
        }
      });
    }

    function extractEditableDocumentText(rootEl = document.getElementById("doc-editor")) {
      if (!rootEl) return "";
      const parts = [];
      let inlineBuf = "";
      function flushInline() {
        const trimmed = inlineBuf.trim();
        if (trimmed) parts.push(trimmed);
        inlineBuf = "";
      }
      function walk(node) {
        if (!node) return;
        if (node.nodeType === 3) {
          inlineBuf += node.nodeValue || "";
          return;
        }
        if (node.nodeType !== 1 || isExcludedDocChromeElement(node)) return;
        const tag = (node.tagName || "").toUpperCase();
        if (tag === "BR") {
          flushInline();
          return;
        }
        const isBlock = node !== rootEl && DOC_BLOCK_TAGS.has(tag);
        if (isBlock) flushInline();
        Array.from(node.childNodes || []).forEach(walk);
        if (isBlock) flushInline();
      }
      walk(rootEl);
      flushInline();
      return parts.join("\n");
    }

    function collectActiveScrubGroups(rootEl) {
      if (!rootEl) return [];
      const tokens = collectEditableInlineTokens(rootEl, {
        includeTables: true,
      });
      const groups = [];
      let lineEpoch = 0;
      tokens.forEach((tok) => {
        if (tok.type === "br") {
          lineEpoch += 1;
          return;
        }
        const tn = tok.node;
        const block = findNearestScrubBlock(tn, rootEl);
        const last = groups[groups.length - 1];
        if (last && last.block === block && last.lineEpoch === lineEpoch) {
          last.nodes.push(tn);
        } else {
          groups.push({ block, lineEpoch, nodes: [tn] });
        }
      });
      return groups
        .map((g) => ({
          nodes: g.nodes,
          origText: g.nodes.map((n) => n.nodeValue || "").join(""),
        }))
        .filter((g) => g.origText.trim().length > 0);
    }

    function applyScrubResultsToGroups(activeGroups, results) {
      let masked = 0;
      results.forEach((res, idx) => {
        if (res && typeof res.scrubbedText === "string") {
          const count = Number(res.totalMasked || 0);
          const group = activeGroups[idx];
          if (count > 0 && res.scrubbedText !== group.origText) {
            applyScrubbedSpansToInlineGroup(group.nodes, group.origText, res);
          }
          masked += count;
        }
      });
      return masked;
    }

    return {
      isExcludedDocChromeElement,
      extractEditableDocumentText,
      collectActiveScrubGroups,
      applyScrubResultsToGroups,
      tokenizeFormulaStringLiterals,
    };
  }

  host.__EASYOFFICE_CREATE_PII_SCRUBBER__ = createPiiDomScrubber;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
