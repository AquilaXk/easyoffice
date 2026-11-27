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

  const ENTITY_RE = /&(?:#\d+|#x[0-9a-f]+|[a-z]+);/i;
  const TAG_LIKE_PLAIN_RE =/<\/?(?:strong|b|em|i|u|s|del|strike|a|table|tr|th|td|br)\b/i;

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
    let anyMarkupOrSpan = false;
    const parsedRows = rows.map((tr, rIdx) => {
      const cells = Array.from(tr.children || [])
        .filter((c) => {
          const t = (c.tagName || "").toLowerCase();
          return t === "td" || t === "th";
        })
        .slice(0, MAX_TABLE_COLS);
      return cells.map((cell) => {
        const colSpan = Number(cell.getAttribute?.("colspan") || cell.colSpan || 1);
        const rowSpan = Number(cell.getAttribute?.("rowspan") || cell.rowSpan || 1);
        if (colSpan > 1 || rowSpan > 1) anyMarkupOrSpan = true;
        const isHeader = (cell.tagName || "").toLowerCase() === "th" || (rIdx === 0 && cell.dataset?.eoHeader === "true");
        const tag = isHeader ? "th" : "td";
        const { html, hasMarkup } = serializeInlineInternal(cell);
        const headerOnlyBold = rIdx === 0 && isHeader && /^<strong>[^<]*<\/strong>$/.test(html);
        if (hasMarkup && !headerOnlyBold) anyMarkupOrSpan = true;
        const plain = headerOnlyBold
          ? decodeHtmlEntities(html.slice("<strong>".length, -"</strong>".length))
          : decodeHtmlEntities(html);
        return { tag, html, plain };
      });
    });

    if (!anyMarkupOrSpan) {
      const cleanCell = (txt) => String(txt || "").trim().replace(/\|/g, "\\|").replace(/\r?\n+/g, " ");
      const headCols = Math.max(1, parsedRows[0]?.length || 1);
      const mdLines = parsedRows.map((row) => "| " + row.map((c) => cleanCell(c.plain)).join(" | ") + " |");
      const sep = "| " + Array(headCols).fill("---").join(" | ") + " |";
      return [mdLines[0], sep, ...mdLines.slice(1)].join("\n");
    }

    const rowHtml = parsedRows
      .map((row) => "<tr>" + row.map((c) => `<${c.tag}>${c.html}</${c.tag}>`).join("") + "</tr>")
      .join("");
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

  // 문서에 있던 평문은 태그·엔티티 문법으로 다시 읽지 않고 그대로 텍스트 노드로 넣는다(`\n`은 줄바꿈).
  function appendPlainRuns(document, parent, text) {
    String(text ?? "").split("\n").forEach((part, idx) => {
      if (idx > 0) parent.appendChild(document.createElement("br"));
      if (part) parent.appendChild(document.createTextNode(part));
    });
  }

  // 빈 글이면 빈 줄 하나가 남는다.
  function appendPlainText(document, parent, text) {
    appendPlainRuns(document, parent, text);
    if (!parent.firstChild) parent.appendChild(document.createElement("br"));
  }

  const blockTagOf = (blockEl) => String(blockEl?.tagName || "").toLowerCase();
  const isListBlock = (blockEl) => blockTagOf(blockEl) === "ul" || blockTagOf(blockEl) === "ol";
  const isTableOrListBlock = (blockEl) => blockTagOf(blockEl) === "table" || isListBlock(blockEl);
  const LIST_MARKER_RE = /^\s*(?:[-*•]|\d+[.)])\s+/;

  // 목록의 새 글: 줄마다 항목 하나(`- a`·`1. a` 표지는 뗀다).
  const listItemLines = (rawText) => String(rawText ?? "").split("\n").map((line) => line.replace(LIST_MARKER_RE, "")).filter((line) => line.trim());

  // 한 줄도 없으면 빈 항목 하나를 둔다.
  function fillListItems(document, list, rawText) {
    const lines = listItemLines(rawText);
    for (const line of lines.length > 0 ? lines : [""]) {
      const li = document.createElement("li");
      populateSanitizedInline(document, li, line);
      list.appendChild(li);
    }
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

  const isWordChar = (ch) => /^[A-Za-z0-9_]$/.test(ch);

  // The next `<name ...>` at or after `from` in `lower` (the text with ASCII capitals folded, so indexes match the
  // text): `{ start, bodyStart }`, or null when there is none. The tag name ends at a word boundary (`<tr` is not
  // `<track`) and the tag at the next `>`; with no `>` left there is no tag left either.
  function nextOpenTag(lower, name, from) {
    let at = lower.indexOf("<" + name, from);
    while (at >= 0) {
      const after = lower.charAt(at + 1 + name.length);
      if (after === "" || !isWordChar(after)) {
        const gt = lower.indexOf(">", at);
        return gt < 0 ? null : { start: at, bodyStart: gt + 1 };
      }
      at = lower.indexOf("<" + name, at + 1);
    }
    return null;
  }

  const foldAscii = (text) => text.replace(/[A-Z]/g, (c) => String.fromCharCode(c.charCodeAt(0) + 32));

  // The cells of one row body: `<th>` or `<td>` up to its own end tag, in order, at most MAX_TABLE_COLS. A cell with
  // no end tag in the rest of the body is skipped, and so is every later one of its kind.
  function parseTableCells(body, firstRow, state) {
    const lower = foldAscii(body);
    const row = [];
    const ended = { th: false, td: false };
    let from = 0;
    while (row.length < MAX_TABLE_COLS) {
      const th = ended.th ? null : nextOpenTag(lower, "th", from);
      const td = ended.td ? null : nextOpenTag(lower, "td", from);
      const tag = th && (!td || th.start < td.start) ? th : td;
      if (!tag) break;
      const name = tag === th ? "th" : "td";
      const close = lower.indexOf("</" + name + ">", tag.bodyStart);
      if (close < 0) {
        ended[name] = true;
        continue;
      }
      if (firstRow && name === "th") state.headerRow = true;
      row.push(body.slice(tag.bodyStart, close).trim());
      from = close + name.length + 3;
    }
    return row;
  }

  // One pass over the text: each `<tr>` up to the first `</tr>` is a row (a `<tr>` with no end tag after it ends the
  // table), so the work grows with the length of the text, however many tags lack an end tag.
  function parseHtmlTableString(rawHtml) {
    const trimmed = String(rawHtml ?? "").trim();
    if (!/^<table\b/i.test(trimmed)) return null;
    const lower = foldAscii(trimmed);
    const rows = [];
    const state = { headerRow: false };
    let from = 0;
    while (rows.length < MAX_TABLE_ROWS) {
      const tag = nextOpenTag(lower, "tr", from);
      if (!tag) break;
      const close = lower.indexOf("</tr>", tag.bodyStart);
      if (close < 0) break;
      const row = parseTableCells(trimmed.slice(tag.bodyStart, close), rows.length === 0, state);
      if (row.length > 0) rows.push(row);
      from = close + 5;
    }
    return rows.length > 0 ? { rows, headerRow: state.headerRow } : null;
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

  // `fill(parent)`: 새 내용을 넣는 함수. 없으면 `rawText`를 모델이 준 글로 읽는다(`populateSanitizedInline`).
  // 표 문단은 글로 바꾸지 않고 거부한다(호출한 쪽이 롤백한다). 목록 문단은 줄마다 항목으로 다시 짓는다.
  function replaceParagraphContentWithTrackChanges(document, editor, blockEl, rawText, fill) {
    const tableParsed = parseHtmlTableString(rawText);
    if (!tableParsed && blockTagOf(blockEl) === "table") {
      throw new Error("표 문단은 글로 바꿀 수 없습니다. 표 안의 글은 부분 교체로 고치세요.");
    }
    const isList = !tableParsed && isListBlock(blockEl);
    const fillContent = fill || ((parent) => populateSanitizedInline(document, parent, rawText));
    if (!isTrackChangesActive(editor)) {
      if (tableParsed) {
        const tbl = buildTableElement(document, tableParsed.rows, tableParsed.headerRow);
        blockEl.replaceWith(tbl);
        return tbl;
      }
      blockEl.replaceChildren();
      if (isList) fillListItems(document, blockEl, rawText);
      else fillContent(blockEl);
      return blockEl;
    }

    const revId = nextRevisionId(editor);
    const dateIso = new Date().toISOString();
    if (tableParsed || isList) {
      const hadText = Boolean(String(blockEl.textContent || "").trim());
      const following = blockEl.nextSibling;
      markBlockDeletedInPlace(document, blockEl, revId, dateIso);
      let next = null;
      if (tableParsed) {
        next = buildTableElement(document, tableParsed.rows, tableParsed.headerRow);
      } else {
        next = document.createElement(blockTagOf(blockEl));
        fillListItems(document, next, rawText);
      }
      wrapBlockInserted(document, next, revId + (hadText ? 1 : 0), dateIso);
      editor.insertBefore(next, following);
      return next;
    }

    const oldHasText = Boolean(String(blockEl.textContent || "").trim());
    const delWrapper = oldHasText ? makeRevisionWrapper(document, "del", revId, dateIso) : null;
    if (delWrapper) {
      while (blockEl.firstChild) delWrapper.appendChild(blockEl.firstChild);
    } else {
      blockEl.replaceChildren();
    }
    const insWrapper = makeRevisionWrapper(document, "ins", revId + (delWrapper ? 1 : 0), dateIso);
    fillContent(insWrapper);
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

  function collectActiveTextEntries(root, out = []) {
    if (!root) return out;
    for (const child of Array.from(root.childNodes || [])) {
      if (child.nodeType === 3) {
        out.push({ node: child, text: String(child.data || "") });
      } else if (child.nodeType === 1) {
        const tag = (child.tagName || "").toLowerCase();
        const ds = child.dataset || {};
        if (tag === "del" && ds.eoRevId !== undefined) continue;
        if (tag === "sup" && ds.eoNoteKind !== undefined) continue;
        if (tag === "br") {
          out.push({ br: child, text: "\n" });
          continue;
        }
        collectActiveTextEntries(child, out);
      }
    }
    return out;
  }

  function plainActiveTextOfBlock(blockEl) {
    return collectActiveTextEntries(blockEl).map((e) => e.text).join("");
  }

  // `node`를 `holder`의 자식들로 갈아 끼운다.
  function swapNodeForChildren(node, holder) {
    const parent = node.parentNode;
    while (holder.firstChild) parent.insertBefore(holder.firstChild, node);
    parent.removeChild(node);
  }

  function replaceSpanWithTrackChanges(document, editor, blockEl, search, replace) {
    const cleanSearch = String(search ?? "");
    const cleanReplace = String(replace ?? "");
    if (!cleanSearch.trim()) {
      throw new Error("검색 대상 문자열은 비어 있을 수 없습니다.");
    }
    const entries = collectActiveTextEntries(blockEl);
    const plain = entries.map((e) => e.text).join("");
    const firstAt = plain.indexOf(cleanSearch);
    if (firstAt < 0) {
      throw new Error("변경할 대상 구간을 문단에서 찾을 수 없습니다.");
    }
    if (plain.indexOf(cleanSearch, firstAt + 1) >= 0) {
      throw new Error("동일한 문구가 여러 번 나타나 바꿀 위치를 특정할 수 없습니다.");
    }
    const endAt = firstAt + cleanSearch.length;
    const hasInlineEl = Boolean(blockEl.querySelector?.("span.eo-run, a, strong, em, u, s, del, ins"));

    // 표와 목록은 칸·항목을 지키며 한 칸 안의 글만 바꾼다(문단 하나로 합치지 않는다).
    if (isTableOrListBlock(blockEl)) {
      if (isTrackChangesActive(editor)) {
        throw new Error("변경 추적 중에는 표·목록 안의 글을 부분 교체할 수 없습니다.");
      }
      let at = 0;
      const touched = entries.filter((entry) => {
        const start = at;
        at += entry.text.length;
        return at > firstAt && start < endAt;
      });
      if (touched.length !== 1 || touched[0].br) {
        throw new Error("표·목록의 여러 칸·항목에 걸친 구간은 바꿀 수 없습니다. 한 칸 안의 구간으로 지정해 주세요.");
      }
    }

    if (isTrackChangesActive(editor)) {
      if (hasInlineEl) {
        const serialized = serializeInlineChildren(blockEl);
        const serAt = serialized.indexOf(cleanSearch);
        const insideTag = serAt >= 0 && serialized.lastIndexOf("<", serAt) > serialized.lastIndexOf(">", serAt);
        if (serAt >= 0 && !insideTag && serialized.indexOf(cleanSearch, serAt + 1) < 0) {
          const nextSer = serialized.slice(0, serAt) + cleanReplace + serialized.slice(serAt + cleanSearch.length);
          return replaceParagraphContentWithTrackChanges(document, editor, blockEl, nextSer);
        }
      }
      // 문서에 있던 글은 그대로 텍스트로 두고 모델이 준 바꿀 글만 서식 표기로 읽는다.
      const nextPlain = plain.slice(0, firstAt) + cleanReplace + plain.slice(endAt);
      return replaceParagraphContentWithTrackChanges(document, editor, blockEl, nextPlain, (parent) => {
        appendPlainRuns(document, parent, plain.slice(0, firstAt));
        populateSanitizedInline(document, parent, cleanReplace);
        appendPlainRuns(document, parent, plain.slice(endAt));
      });
    }

    // 텍스트 노드 단위로 활성 텍스트 범위(<del>/주석 제외)만 부분 치환한다. 인라인 서식·칸·항목은 그대로 남고, 바꾸지 않는 글은 다시 읽지 않는다.
    let offset = 0;
    let wroteReplacement = false;
    const needsInlineParse = TAG_LIKE_PLAIN_RE.test(cleanReplace) || cleanReplace.includes("\n") || ENTITY_RE.test(cleanReplace);
    for (const entry of entries) {
      const len = entry.text.length;
      const nStart = offset;
      const nEnd = offset + len;
      offset = nEnd;
      if (nEnd <= firstAt || nStart >= endAt) continue;
      if (entry.br) {
        if (!wroteReplacement) {
          wroteReplacement = true;
          const holder = document.createElement("span");
          populateSanitizedInline(document, holder, cleanReplace);
          swapNodeForChildren(entry.br, holder);
        } else {
          entry.br.remove?.();
        }
        continue;
      }
      const node = entry.node;
      const localStart = Math.max(0, firstAt - nStart);
      const localEnd = Math.min(len, endAt - nStart);
      const before = entry.text.slice(0, localStart);
      const after = entry.text.slice(localEnd);
      if (!wroteReplacement) {
        wroteReplacement = true;
        if (needsInlineParse) {
          const holder = document.createElement("span");
          if (before) holder.appendChild(document.createTextNode(before));
          populateSanitizedInline(document, holder, cleanReplace);
          if (after) holder.appendChild(document.createTextNode(after));
          swapNodeForChildren(node, holder);
        } else {
          node.data = before + cleanReplace + after;
        }
      } else {
        node.data = before + after;
      }
    }
    return blockEl;
  }

  function scrubDocPiiInEditor(editor, localScrubPii, locations) {
    const blocks = docBlocksOf(editor);
    let changedCount = 0;
    blocks.forEach((blk, idx) => {
      const walk = (node) => {
        for (const child of node.childNodes) {
          if (child.nodeType === 3) {
            const res = localScrubPii(child.data);
            if (res && res.totalMasked > 0 && typeof res.scrubbedText === "string") {
              child.data = res.scrubbedText;
              changedCount += res.totalMasked;
            }
          } else if (child.nodeType === 1) {
            walk(child);
          }
        }
      };
      const before = blk.textContent;
      walk(blk);
      if (blk.textContent !== before) {
        locations.push({ module: "DOC", label: "문단 " + (idx + 1), paragraphIndex: idx });
      }
    });
    return changedCount > 0 || blocks.length > 0;
  }

  host.__EASYOFFICE_COPILOT_DOC_HTML__ = Object.freeze({
    docBlocksOf,
    serializeBlockForContext,
    populateSanitizedInline,
    appendPlainText,
    listItemLines,
    buildDocBlockElement,
    isTrackChangesActive,
    nextRevisionId,
    wrapBlockInserted,
    replaceParagraphContentWithTrackChanges,
    replaceSpanWithTrackChanges,
    markBlockDeletedInPlace,
    scrubDocPiiInEditor,
    plainPreviewTextOfBlock,
    plainActiveTextOfBlock,
    parseHtmlTableString,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
