// 문서(DOC) 코파일럿 인라인 서식·표 보존 직렬화기, 안전 화이트리스트 DOM 빌더 및 변경 내용 추적 연동 (PD-360).
// 허용 인라인 태그(<strong>, <em>, <u>, <s>, <a href>)와 표(<table>/<tr>/<th>/<td>)만 다루며,
// innerHTML/DOMParser를 쓰지 않고 doc-runs.js(span.eo-run) 및 doc-table.js(table.doc-inline-table) 계약으로 변환한다.

(function (host) {
  "use strict";

  const MAX_PARSE_TOKENS = 4096;
  const MAX_TABLE_ROWS = 64;
  const MAX_TABLE_COLS = 16;
  const MAX_INLINE_DEPTH = 12;
  const ALLOWED_BLOCK_TAGS = new Set(["p", "h1", "h2", "h3", "h4", "blockquote"]);
  const INLINE_OPEN_FORMAT = Object.freeze({
    strong: "bold",
    b: "bold",
    em: "italic",
    i: "italic",
    u: "underline",
    s: "strike",
    del: "strike",
    strike: "strike",
  });

  function isSafeHref(raw) {
    if (typeof raw !== "string") return false;
    const s = raw.trim();
    if (!s || s.length > 2048) return false;
    for (let i = 0; i < s.length; i += 1) {
      const c = s.charCodeAt(i);
      if (c <= 0x1f || (c >= 0x7f && c <= 0x9f)) return false;
    }
    if (s.startsWith("//") || s.startsWith("\\\\")) return false;
    const low = s.toLowerCase();
    return low.startsWith("https://") || low.startsWith("http://") || low.startsWith("mailto:");
  }

  function escapeHtmlText(text) {
    return String(text ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;");
  }

  function escapeAttr(text) {
    return escapeHtmlText(text).replaceAll('"', "&quot;");
  }

  function decodeHtmlEntities(raw) {
    return String(raw ?? "")
      .replaceAll("&lt;", "<")
      .replaceAll("&gt;", ">")
      .replaceAll("&quot;", '"')
      .replaceAll("&#39;", "'")
      .replaceAll("&apos;", "'")
      .replaceAll("&nbsp;", " ")
      .replaceAll("&amp;", "&");
  }

  function isMetaElement(el) {
    if (!el || el.nodeType !== 1) return true;
    const tag = (el.tagName || "").toUpperCase();
    if (tag === "FOOTER" || el.id === "hwp-manuscript-preview" || el.hidden) return true;
    if (el.classList?.contains("manuscript-notice") || el.classList?.contains("eo-doc-toc")) return true;
    const ds = el.dataset || {};
    return ds.eoNotes !== undefined || ds.eoComments !== undefined || ds.eoHf !== undefined;
  }

  function docBlocksOf(editor) {
    if (!editor) return [];
    const out = [];
    const children = editor.children || editor.querySelectorAll?.("p, h1, h2, h3, h4, ul, ol, blockquote, table") || [];
    for (const el of Array.from(children)) {
      if (el.parentElement !== editor || isMetaElement(el)) continue;
      const tag = (el.tagName || "").toLowerCase();
      if (
        tag === "p" || tag === "h1" || tag === "h2" || tag === "h3" || tag === "h4"
        || tag === "ul" || tag === "ol" || tag === "blockquote" || tag === "table"
      ) {
        out.push(el);
      }
    }
    return out;
  }

  function readNodeFormatFlags(el) {
    const tag = (el.tagName || "").toLowerCase();
    const ds = el.dataset || {};
    return {
      bold: tag === "strong" || tag === "b" || ds.eoBold === "true",
      italic: tag === "em" || tag === "i" || ds.eoItalic === "true",
      underline: tag === "u" || ds.eoUnderline === "true",
      strike: tag === "s" || tag === "strike" || (tag === "del" && ds.eoRevId === undefined) || ds.eoStrike === "true",
    };
  }

  const TAG_LIKE_PLAIN_RE = /<\/?(?:strong|b|em|i|u|s|del|strike|a|table|tr|th|td|br)\b/i;

  function serializeInlineInternal(container) {
    let hasMarkup = false;
    const walk = (node) => {
      if (!node) return "";
      if (node.nodeType === 3) {
        return escapeHtmlText(node.data || "");
      }
      if (node.nodeType !== 1) return "";
      const tag = (node.tagName || "").toLowerCase();
      const ds = node.dataset || {};
      // Deleted tracked revisions are excluded from active live text.
      if (tag === "del" && ds.eoRevId !== undefined) return "";
      if (tag === "sup" && ds.eoNoteKind !== undefined) return "";
      if (tag === "br") return "\n";

      let inner = "";
      for (const child of Array.from(node.childNodes || [])) {
        inner += walk(child);
      }
      if (!inner) return "";

      if (tag === "a") {
        const href = node.getAttribute?.("href") || "";
        if (isSafeHref(href)) {
          hasMarkup = true;
          return `<a href="${escapeAttr(href.trim())}">${inner}</a>`;
        }
        return inner;
      }

      const flags = readNodeFormatFlags(node);
      let wrapped = inner;
      if (flags.strike) { hasMarkup = true; wrapped = `<s>${wrapped}</s>`; }
      if (flags.underline) { hasMarkup = true; wrapped = `<u>${wrapped}</u>`; }
      if (flags.italic) { hasMarkup = true; wrapped = `<em>${wrapped}</em>`; }
      if (flags.bold) { hasMarkup = true; wrapped = `<strong>${wrapped}</strong>`; }
      return wrapped;
    };

    const html = walk(container).trim();
    return { html, hasMarkup };
  }

  function serializeInlineChildren(container) {
    const { html, hasMarkup } = serializeInlineInternal(container);
    if (!hasMarkup) {
      const plain = decodeHtmlEntities(html);
      // Keep HTML entity escaping if literal text looks like an allowed formatting/table tag so round-trips stay lossless.
      return TAG_LIKE_PLAIN_RE.test(plain) ? html : plain;
    }
    return html;
  }

  function serializeTableBlock(tableEl) {
    const rows = Array.from(tableEl.querySelectorAll?.("tr") || []).slice(0, MAX_TABLE_ROWS);
    if (rows.length === 0) return "";
    const rowHtml = rows.map((tr, rIdx) => {
      const cells = Array.from(tr.children || [])
        .filter((c) => {
          const t = (c.tagName || "").toLowerCase();
          return t === "td" || t === "th";
        })
        .slice(0, MAX_TABLE_COLS);
      const cellStr = cells.map((cell) => {
        const isHeader = (cell.tagName || "").toLowerCase() === "th" || (rIdx === 0 && cell.dataset?.eoHeader === "true");
        const tag = isHeader ? "th" : "td";
        const { html } = serializeInlineInternal(cell);
        return `<${tag}>${html}</${tag}>`;
      }).join("");
      return `<tr>${cellStr}</tr>`;
    }).join("");
    return `<table>${rowHtml}</table>`;
  }

  function serializeBlockForContext(blockEl) {
    if (!blockEl) return "";
    const tag = (blockEl.tagName || "").toLowerCase();
    if (tag === "table") return serializeTableBlock(blockEl);
    if (tag === "ul" || tag === "ol") {
      const items = Array.from(blockEl.querySelectorAll?.("li") || []);
      return items.map((li, idx) => {
        const prefix = tag === "ol" ? (idx + 1) + ". " : "- ";
        return prefix + serializeInlineChildren(li);
      }).join("\n");
    }
    return serializeInlineChildren(blockEl);
  }

  function appendFormattedRun(document, parent, text, activeFmt, activeHref) {
    if (!text) return;
    let targetParent = parent;
    if (activeHref && isSafeHref(activeHref)) {
      const a = document.createElement("a");
      a.setAttribute("href", activeHref);
      a.dataset.eoLink = "true";
      parent.appendChild(a);
      targetParent = a;
    }
    const hasRunFmt = Boolean(activeFmt.bold || activeFmt.italic || activeFmt.underline || activeFmt.strike);
    if (!hasRunFmt) {
      targetParent.appendChild(document.createTextNode(text));
      return;
    }
    const span = document.createElement("span");
    span.className = "eo-run";
    if (activeFmt.bold) { span.dataset.eoBold = "true"; span.style.fontWeight = "bold"; }
    if (activeFmt.italic) { span.dataset.eoItalic = "true"; span.style.fontStyle = "italic"; }
    const deco = [];
    if (activeFmt.underline) { span.dataset.eoUnderline = "true"; deco.push("underline"); }
    if (activeFmt.strike) { span.dataset.eoStrike = "true"; deco.push("line-through"); }
    if (deco.length > 0) span.style.textDecoration = deco.join(" ");
    span.textContent = text;
    targetParent.appendChild(span);
  }

  function populateSanitizedInline(document, parent, rawInput) {
    const str = String(rawInput ?? "");
    if (!str) return;
    if (!str.includes("<") && !str.includes("&")) {
      const parts = str.split("\n");
      parts.forEach((part, idx) => {
        if (idx > 0) parent.appendChild(document.createElement("br"));
        if (part) parent.appendChild(document.createTextNode(part));
      });
      return;
    }

    const fmtCount = { bold: 0, italic: 0, underline: 0, strike: 0 };
    const stack = [];
    let activeHref = "";
    const tagRe = /<(\/?)([A-Za-z0-9]+)(?:\s+([^>]{0,2048}))?>/g;
    let last = 0;
    let tokens = 0;
    let m;

    const flushText = (slice) => {
      if (!slice) return;
      const decoded = decodeHtmlEntities(slice);
      const lines = decoded.split("\n");
      lines.forEach((line, idx) => {
        if (idx > 0) parent.appendChild(document.createElement("br"));
        if (line) {
          appendFormattedRun(
            document,
            parent,
            line,
            {
              bold: fmtCount.bold > 0,
              italic: fmtCount.italic > 0,
              underline: fmtCount.underline > 0,
              strike: fmtCount.strike > 0,
            },
            activeHref,
          );
        }
      });
    };

    while ((m = tagRe.exec(str)) !== null && tokens < MAX_PARSE_TOKENS) {
      tokens += 1;
      if (m.index > last) flushText(str.slice(last, m.index));
      const closing = m[1] === "/";
      const tag = m[2].toLowerCase();
      const attrs = m[3] || "";

      if (tag === "br" && !closing) {
        parent.appendChild(document.createElement("br"));
      } else if (Object.hasOwn(INLINE_OPEN_FORMAT, tag)) {
        const prop = INLINE_OPEN_FORMAT[tag];
        if (!closing && stack.length < MAX_INLINE_DEPTH) {
          fmtCount[prop] += 1;
          stack.push({ kind: "fmt", prop, tag });
        } else if (closing) {
          for (let s = stack.length - 1; s >= 0; s -= 1) {
            if (stack[s].kind === "fmt" && stack[s].prop === prop) {
              fmtCount[prop] = Math.max(0, fmtCount[prop] - 1);
              stack.splice(s, 1);
              break;
            }
          }
        }
      } else if (tag === "a") {
        if (!closing && stack.length < MAX_INLINE_DEPTH) {
          const hrefMatch = /\bhref\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))/i.exec(attrs);
          const candidate = decodeHtmlEntities(hrefMatch?.[1] ?? hrefMatch?.[2] ?? hrefMatch?.[3] ?? "");
          const prevHref = activeHref;
          if (isSafeHref(candidate)) {
            activeHref = candidate.trim();
          }
          stack.push({ kind: "link", prevHref });
        } else if (closing) {
          for (let s = stack.length - 1; s >= 0; s -= 1) {
            if (stack[s].kind === "link") {
              activeHref = stack[s].prevHref;
              stack.splice(s, 1);
              break;
            }
          }
        }
      }
      last = m.index + m[0].length;
    }
    if (last < str.length) flushText(str.slice(last));
    if (!parent.firstChild) parent.appendChild(document.createElement("br"));
  }

  function parseHtmlTableString(rawHtml) {
    const trimmed = String(rawHtml ?? "").trim();
    if (!/^<table\b/i.test(trimmed)) return null;
    const rows = [];
    let headerRow = false;
    const trRe = /<tr\b[^>]*>([\s\S]*?)<\/tr>/gi;
    let trMatch;
    while ((trMatch = trRe.exec(trimmed)) !== null && rows.length < MAX_TABLE_ROWS) {
      const rowBody = trMatch[1];
      const cellRe = /<(th|td)\b[^>]*>([\s\S]*?)<\/\1>/gi;
      const row = [];
      let cellMatch;
      while ((cellMatch = cellRe.exec(rowBody)) !== null && row.length < MAX_TABLE_COLS) {
        if (rows.length === 0 && cellMatch[1].toLowerCase() === "th") headerRow = true;
        row.push(cellMatch[2].trim());
      }
      if (row.length > 0) rows.push(row);
    }
    return rows.length > 0 ? { rows, headerRow } : null;
  }

  function buildTableElement(document, rows, headerRow) {
    const colCount = Math.min(
      MAX_TABLE_COLS,
      Math.max(1, ...rows.map((r) => (Array.isArray(r) ? r.length : 0))),
    );
    const table = document.createElement("table");
    table.className = "doc-inline-table";
    const tbody = document.createElement("tbody");
    rows.slice(0, MAX_TABLE_ROWS).forEach((row, rIdx) => {
      const tr = document.createElement("tr");
      const isHeader = Boolean(headerRow) && rIdx === 0;
      for (let c = 0; c < colCount; c += 1) {
        const td = document.createElement("td");
        const rawCell = String((Array.isArray(row) ? row[c] : "") ?? "");
        if (isHeader) {
          td.dataset.eoHeader = "true";
          populateSanitizedInline(document, td, rawCell ? `<strong>${rawCell}</strong>` : "");
        } else {
          populateSanitizedInline(document, td, rawCell);
        }
        tr.appendChild(td);
      }
      tbody.appendChild(tr);
    });
    table.appendChild(tbody);
    return table;
  }

  function buildDocBlockElement(document, item) {
    if (typeof item === "string") {
      const tableParsed = parseHtmlTableString(item);
      if (tableParsed) return buildTableElement(document, tableParsed.rows, tableParsed.headerRow);
      const p = document.createElement("p");
      populateSanitizedInline(document, p, item);
      return p;
    }
    if (!item || typeof item !== "object") return null;

    const rawKind = String(item.style || item.tag || item.kind || "p").toLowerCase();
    const tblSpec = item.table && typeof item.table === "object"
      ? item.table
      : (rawKind === "table" || Array.isArray(item.rows) ? item : null);
    if (tblSpec) {
      const rows = Array.isArray(tblSpec.rows) ? tblSpec.rows : [["항목", "내용"]];
      return buildTableElement(document, rows, tblSpec.headerRow !== false);
    }

    const rawText = String(item.text ?? item.content ?? "");
    const htmlTable = parseHtmlTableString(rawText);
    if (htmlTable) {
      return buildTableElement(document, htmlTable.rows, htmlTable.headerRow);
    }

    if (Number.isInteger(item.heading) && item.heading >= 1) {
      const h = document.createElement("h" + Math.min(4, item.heading));
      populateSanitizedInline(document, h, rawText);
      return h;
    }

    const listKind = String(item.list || "").toLowerCase();
    if (listKind || rawKind === "ul" || rawKind === "ol" || rawKind === "bullet" || rawKind === "numbered" || rawKind === "task-list" || Array.isArray(item.items)) {
      const isOl = listKind === "ordered" || rawKind === "ol" || rawKind === "numbered";
      const isTask = listKind === "task" || rawKind === "task-list" || item.task === true;
      const list = document.createElement(isOl ? "ol" : "ul");
      if (isTask && !isOl) {
        list.className = "eo-task-list";
        list.dataset.eoList = "task";
      }
      const entries = Array.isArray(item.items) ? item.items.slice(0, 200) : rawText.split("\n").filter(Boolean);
      for (const entry of entries.length > 0 ? entries : [rawText]) {
        const li = document.createElement("li");
        const entryText = typeof entry === "object" && entry !== null ? String(entry.text || "") : String(entry ?? "");
        if (isTask && !isOl) {
          const checked = typeof entry === "object" && entry !== null ? Boolean(entry.checked) : false;
          li.dataset.eoChecked = checked ? "true" : "false";
        }
        populateSanitizedInline(document, li, entryText);
        list.appendChild(li);
      }
      return list;
    }

    const mappedTag = rawKind === "heading1" ? "h1"
      : rawKind === "heading2" ? "h2"
        : rawKind === "heading3" ? "h3"
          : rawKind === "quote" ? "blockquote"
            : ALLOWED_BLOCK_TAGS.has(rawKind) ? rawKind : "p";
    const el = document.createElement(mappedTag);
    populateSanitizedInline(document, el, rawText);
    return el;
  }

  function isTrackChangesActive(editor) {
    return editor?.dataset?.eoTrackChanges === "true";
  }

  function nextRevisionId(editor) {
    let maxId = 0;
    const all = editor.querySelectorAll?.("[data-eo-rev-id]") || [];
    for (const el of Array.from(all)) {
      maxId = Math.max(maxId, Number(el.dataset?.eoRevId) || 0);
    }
    return maxId + 1;
  }

  function makeRevisionWrapper(document, tag, revId, dateIso) {
    const el = document.createElement(tag);
    el.className = tag === "del" ? "eo-rev-del" : "eo-rev-ins";
    el.dataset.eoRevId = String(revId);
    el.dataset.eoRevAuthor = "코파일럿";
    el.dataset.eoRevDate = dateIso;
    return el;
  }

  function wrapBlockInserted(document, blockEl, revId, dateIso) {
    if (!blockEl) return blockEl;
    const tag = (blockEl.tagName || "").toLowerCase();
    if (tag === "table") {
      for (const cell of Array.from(blockEl.querySelectorAll?.("td, th") || [])) {
        if (!cell.textContent) continue;
        const ins = makeRevisionWrapper(document, "ins", revId, dateIso);
        while (cell.firstChild) ins.appendChild(cell.firstChild);
        cell.appendChild(ins);
      }
      return blockEl;
    }
    if (tag === "ul" || tag === "ol") {
      for (const li of Array.from(blockEl.querySelectorAll?.("li") || [])) {
        const ins = makeRevisionWrapper(document, "ins", revId, dateIso);
        while (li.firstChild) ins.appendChild(li.firstChild);
        li.appendChild(ins);
      }
      return blockEl;
    }
    const ins = makeRevisionWrapper(document, "ins", revId, dateIso);
    while (blockEl.firstChild) ins.appendChild(blockEl.firstChild);
    blockEl.appendChild(ins);
    return blockEl;
  }

  function replaceParagraphContentWithTrackChanges(document, editor, blockEl, rawText) {
    const tableParsed = parseHtmlTableString(rawText);
    if (!isTrackChangesActive(editor)) {
      if (tableParsed) {
        const tbl = buildTableElement(document, tableParsed.rows, tableParsed.headerRow);
        blockEl.replaceWith(tbl);
        return tbl;
      }
      blockEl.replaceChildren();
      populateSanitizedInline(document, blockEl, rawText);
      return blockEl;
    }

    const revId = nextRevisionId(editor);
    const dateIso = new Date().toISOString();
    if (tableParsed) {
      const hadText = Boolean(String(blockEl.textContent || "").trim());
      markBlockDeletedInPlace(document, blockEl, revId, dateIso);
      const tbl = buildTableElement(document, tableParsed.rows, tableParsed.headerRow);
      wrapBlockInserted(document, tbl, revId + (hadText ? 1 : 0), dateIso);
      if (blockEl.parentElement === editor) blockEl.after(tbl);
      else editor.appendChild(tbl);
      return tbl;
    }

    const oldHasText = Boolean(String(blockEl.textContent || "").trim());
    const delWrapper = oldHasText ? makeRevisionWrapper(document, "del", revId, dateIso) : null;
    if (delWrapper) {
      while (blockEl.firstChild) delWrapper.appendChild(blockEl.firstChild);
    } else {
      blockEl.replaceChildren();
    }
    const insWrapper = makeRevisionWrapper(document, "ins", revId + (delWrapper ? 1 : 0), dateIso);
    populateSanitizedInline(document, insWrapper, rawText);
    if (delWrapper) blockEl.appendChild(delWrapper);
    blockEl.appendChild(insWrapper);
    return blockEl;
  }

  function markBlockDeletedInPlace(document, blockEl, revId, dateIso) {
    const tag = (blockEl?.tagName || "").toLowerCase();
    if (!String(blockEl?.textContent || "").trim()) {
      blockEl?.remove?.();
      return;
    }
    if (tag === "table") {
      for (const cell of Array.from(blockEl.querySelectorAll?.("td, th") || [])) {
        if (!String(cell.textContent || "").trim()) continue;
        const del = makeRevisionWrapper(document, "del", revId, dateIso);
        while (cell.firstChild) del.appendChild(cell.firstChild);
        cell.appendChild(del);
      }
      return;
    }
    if (tag === "ul" || tag === "ol") {
      for (const li of Array.from(blockEl.querySelectorAll?.("li") || [])) {
        if (!String(li.textContent || "").trim()) continue;
        const del = makeRevisionWrapper(document, "del", revId, dateIso);
        while (li.firstChild) del.appendChild(li.firstChild);
        li.appendChild(del);
      }
      return;
    }
    const del = makeRevisionWrapper(document, "del", revId, dateIso);
    while (blockEl.firstChild) del.appendChild(blockEl.firstChild);
    blockEl.appendChild(del);
  }

  function plainPreviewTextOfBlock(item) {
    if (typeof item === "string") {
      const tbl = parseHtmlTableString(item);
      if (tbl) return tbl.rows.map((r) => r.map((c) => decodeHtmlEntities(c.replace(/<[^>]+>/g, ""))).join(" | ")).join("\n");
      return decodeHtmlEntities(item.replace(/<br\s*\/?>/gi, "\n").replace(/<[^>]+>/g, ""));
    }
    if (!item || typeof item !== "object") return "";
    const tblObj = item.table && typeof item.table === "object" ? item.table : item;
    if (Array.isArray(tblObj.rows)) {
      return tblObj.rows.map((r) => (Array.isArray(r) ? r.join(" | ") : "")).join("\n");
    }
    if (Array.isArray(item.items)) {
      return item.items.map((e) => (e && typeof e === "object" ? String(e.text ?? "") : String(e ?? ""))).join("\n");
    }
    const raw = String(item.text ?? item.content ?? "");
    return plainPreviewTextOfBlock(raw);
  }

  host.__EASYOFFICE_COPILOT_DOC_HTML__ = Object.freeze({
    docBlocksOf,
    serializeBlockForContext,
    populateSanitizedInline,
    buildDocBlockElement,
    isTrackChangesActive,
    nextRevisionId,
    wrapBlockInserted,
    replaceParagraphContentWithTrackChanges,
    markBlockDeletedInPlace,
    plainPreviewTextOfBlock,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
