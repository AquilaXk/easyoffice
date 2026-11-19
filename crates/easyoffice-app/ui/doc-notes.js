(function (host) {
  "use strict";

  // Document footnotes (`각주`), endnotes (`미주`), comments (`새 메모`) and track changes (`변경 내용 추적`).
  // Korean UI labels follow the reference editor's locale (`각주`, `미주`; MIT, Copyright (c) 2025-2026
  // Edward Kim, see THIRD_PARTY_NOTICES.md) and standard word processor Korean UI (`새 메모`, `변경 내용 추적`).

  const NOTE_KINDS = ["footnote", "endnote"];
  const NOTE_KIND_SET = new Set(NOTE_KINDS);
  const BODY_BLOCK_TAGS = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "BLOCKQUOTE", "PRE", "DIV"]);
  const MAX_NOTES = 2048, MAX_COMMENTS = 2048, MAX_REVISIONS = 8192, MAX_TEXT_CHARS = 65536;
  const DEFAULT_AUTHOR = "작성자";
  const nowIso = () => new Date().toISOString();
  const sanitizeMeta = (raw, fallback, maxLen = 256) => String(raw ?? "").trim().slice(0, maxLen) || fallback;
  const tagOf = (el) => (el?.tagName || "").toUpperCase();
  const childIdx = (el) => Array.prototype.indexOf.call(el.parentElement.childNodes, el);
  const isLoneBrBlock = (b) => b?.childNodes?.length === 1 && tagOf(b.firstChild) === "BR";

  function isNoteOrMetaSection(el) {
    if (!el || el.nodeType !== 1) return false;
    if (tagOf(el) === "FOOTER" || el.id === "hwp-manuscript-preview") return true;
    const ds = el.dataset;
    return Boolean(ds && (ds.eoNotes !== undefined || ds.eoComments !== undefined || ds.eoHf !== undefined));
  }
  function insideNoteOrMeta(root, node) {
    for (let cur = node?.nodeType === 1 ? node : node?.parentElement; cur && cur !== root; cur = cur.parentElement) {
      if (isNoteOrMetaSection(cur)) return true;
    }
    return false;
  }
  function firstTrailingAnchor(root, stopAtEndnote, stopAtComments) {
    for (const child of Array.from(root.children || [])) {
      const ds = child.dataset || {};
      if ((stopAtEndnote && ds.eoNotes === "endnote") || (stopAtComments && ds.eoComments !== undefined)) return child;
      if (ds.eoHf !== undefined || child.id === "hwp-manuscript-preview" || tagOf(child) === "FOOTER") return child;
    }
    return null;
  }
  function ensureBodyParagraph(root) {
    const doc = root.ownerDocument;
    for (let i = 0; i < root.children.length; i += 1) {
      const el = root.children[i];
      if (isNoteOrMetaSection(el) || el.classList?.contains("eo-doc-toc")) continue;
      if (BODY_BLOCK_TAGS.has(tagOf(el))) return el;
      const nested = el.querySelector?.("li, td, th, p");
      if (nested) return nested;
    }
    const p = doc.createElement("p");
    p.appendChild(doc.createElement("br"));
    root.insertBefore(p, Array.from(root.children || []).find(isNoteOrMetaSection) ?? null);
    return p;
  }
  function enclosingBodyBlock(root, node) {
    for (let cur = node?.nodeType === 1 ? node : node?.parentElement; cur && cur !== root; cur = cur.parentElement) {
      const tag = tagOf(cur);
      if (tag === "TD" || tag === "TH" || tag === "LI" || BODY_BLOCK_TAGS.has(tag)) return cur;
    }
    return null;
  }

  function resolveBodyPoint(root, range, atStart = false) {
    const node = atStart ? range?.startContainer : range?.endContainer;
    const offset = atStart ? range?.startOffset : range?.endOffset;
    if (node && root.contains(node) && node !== root && !insideNoteOrMeta(root, node)) {
      const block = enclosingBodyBlock(root, node);
      if (block) return { block, node, offset: Number(offset) || 0 };
    }
    const block = ensureBodyParagraph(root);
    if (isLoneBrBlock(block)) return { block, node: block, offset: 0 };
    const last = block.lastChild;
    return last?.nodeType === 3 ? { block, node: last, offset: last.data.length } : { block, node: block, offset: block.childNodes.length };
  }

  function insertNodeAtPoint(point, newNode) {
    const { block, node, offset } = point;
    if (isLoneBrBlock(block)) block.replaceChildren();
    if (node.nodeType === 3) {
      if (offset > 0 && offset < node.data.length) node.splitText(offset).before(newNode);
      else if (offset <= 0) node.before(newNode);
      else node.parentElement.insertBefore(newNode, node.nextSibling ?? null);
      return;
    }
    const target = block.contains(node) ? node : block;
    target.insertBefore(newNode, target.childNodes[offset] ?? null);
  }

  function isolateInline(node, container) {
    let cur = node;
    while (cur.parentElement && cur.parentElement !== container) {
      const parent = cur.parentElement;
      const siblings = Array.from(parent.childNodes);
      const at = siblings.indexOf(cur);
      for (const [slice, ref] of [[siblings.slice(0, at), parent], [siblings.slice(at + 1), parent.nextSibling ?? null]]) {
        if (!slice.length) continue;
        const copy = parent.cloneNode(false);
        slice.forEach((n) => copy.appendChild(n));
        parent.parentElement.insertBefore(copy, ref);
      }
      cur = parent;
    }
    return cur;
  }

  function ensureSection(root, sel, anchor, setup) {
    let sec = root.querySelector(sel);
    if (!sec) { sec = root.ownerDocument.createElement("section"); setup(sec); root.insertBefore(sec, anchor); }
    else if (anchor && sec.nextSibling !== anchor) root.insertBefore(sec, anchor);
    return sec;
  }

  const ensureNotesSection = (root, kind) => ensureSection(root, `:scope > section[data-eo-notes="${kind}"]`, firstTrailingAnchor(root, kind === "footnote", true), (s) => {
    s.className = "eo-doc-notes eo-doc-" + kind;
    s.dataset.eoNotes = kind;
  });

  const ensureCommentsSection = (root) => {
    const sec = ensureSection(root, ":scope > section[data-eo-comments]", firstTrailingAnchor(root, false, false), (s) => { s.dataset.eoComments = ""; });
    sec.hidden = true;
    return sec;
  };

  function nextNoteId(root, kind) {
    let maxId = 0;
    const refs = root.querySelectorAll(`sup.eo-note-ref[data-eo-note-kind="${kind}"]`);
    if (refs.length >= MAX_NOTES) return 0;
    for (const r of refs) maxId = Math.max(maxId, Number(r.dataset.eoNoteId) || 0);
    for (const it of root.querySelectorAll(`section[data-eo-notes="${kind}"] .eo-doc-note-item`)) maxId = Math.max(maxId, Number(it.dataset.eoNoteId) || 0);
    return maxId + 1;
  }

  function buildNoteItem(doc, id, numLabel, text) {
    const item = Object.assign(doc.createElement("div"), { className: "eo-doc-note-item" });
    item.dataset.eoNoteId = String(id);
    const num = Object.assign(doc.createElement("span"), { className: "eo-doc-note-num", textContent: numLabel });
    num.setAttribute("contenteditable", "false");
    const body = Object.assign(doc.createElement("p"), { className: "eo-doc-note-body" });
    const clean = text === undefined || text === null ? "" : String(text).slice(0, MAX_TEXT_CHARS);
    body.appendChild(clean.length > 0 ? doc.createTextNode(clean) : doc.createElement("br"));
    item.replaceChildren(num, body);
    return item;
  }

  function syncNotesDom(root) {
    if (!root) return;
    const doc = root.ownerDocument;
    const byKind = { footnote: [], endnote: [] };
    const walk = (node) => {
      for (const child of node.childNodes || []) {
        if (child.nodeType !== 1 || isNoteOrMetaSection(child)) continue;
        if (tagOf(child) === "SUP" && child.classList?.contains("eo-note-ref") && NOTE_KIND_SET.has(child.dataset?.eoNoteKind)) byKind[child.dataset.eoNoteKind].push(child);
        else walk(child);
      }
    };
    walk(root);
    for (const kind of NOTE_KINDS) {
      const refs = byKind[kind];
      if (!refs.length) { root.querySelector(`:scope > section[data-eo-notes="${kind}"]`)?.remove(); continue; }
      const sec = ensureNotesSection(root, kind);
      const itemMap = new Map();
      let maxId = 0;
      for (const item of Array.from(sec.children)) {
        const id = item.classList?.contains("eo-doc-note-item") ? Number(item.dataset?.eoNoteId) : NaN;
        if (Number.isInteger(id) && id >= 1 && !itemMap.has(id)) { itemMap.set(id, item); maxId = Math.max(maxId, id); } else item.remove();
      }
      for (const ref of refs) maxId = Math.max(maxId, Number(ref.dataset?.eoNoteId) || 0);
      const seen = new Set();
      refs.forEach((ref, idx) => {
        let id = Number(ref.dataset?.eoNoteId);
        if (!Number.isInteger(id) || id < 1 || seen.has(id)) { id = ++maxId; ref.dataset.eoNoteId = String(id); }
        seen.add(id);
        const numText = idx + 1 + ")";
        if (ref.textContent !== numText) ref.textContent = numText;
        let item = itemMap.get(id);
        if (!item) { item = buildNoteItem(doc, id, numText, ""); itemMap.set(id, item); }
        else { const numEl = item.querySelector(".eo-doc-note-num"); if (numEl && numEl.textContent !== numText) numEl.textContent = numText; }
        if (sec.children[idx] !== item) sec.insertBefore(item, sec.children[idx] ?? null);
      });
      for (const [id, item] of itemMap.entries()) if (!seen.has(id)) item.remove();
    }
  }

  function insertNoteDom(root, range, kind, initialText) {
    if (!root || !NOTE_KIND_SET.has(kind)) return { changed: false };
    const id = nextNoteId(root, kind);
    if (id <= 0) return { changed: false };
    const doc = root.ownerDocument;
    const sup = doc.createElement("sup");
    sup.className = "eo-note-ref";
    sup.setAttribute("contenteditable", "false");
    sup.dataset.eoNoteKind = kind;
    sup.dataset.eoNoteId = String(id);
    sup.textContent = "1)";
    insertNodeAtPoint(resolveBodyPoint(root, range, false), sup);
    const item = buildNoteItem(doc, id, "1)", initialText);
    ensureNotesSection(root, kind).appendChild(item);
    syncNotesDom(root);
    return { changed: true, id, kind, refEl: sup, bodyEl: item.querySelector(".eo-doc-note-body") };
  }

  function wrapIsolatedRange(isolated, wrapper) {
    const tops = isolated.texts.map((t) => isolateInline(t, isolated.container));
    const last = tops.at(-1);
    let node = tops[0];
    node.before(wrapper);
    for (;;) { const next = node.nextSibling ?? null; wrapper.appendChild(node); if (node === last) break; node = next; }
  }

  function addCommentDom(root, range, text, author, date) {
    const cleanText = String(text ?? "").trim().slice(0, MAX_TEXT_CHARS);
    if (!root || !cleanText) return { changed: false };
    let maxId = 0;
    let count = 0;
    for (const el of root.querySelectorAll("[data-eo-comment-id]")) { maxId = Math.max(maxId, Number(el.dataset.eoCommentId) || 0); if (tagOf(el) === "ARTICLE") count += 1; }
    if (count >= MAX_COMMENTS) return { changed: false };
    const id = maxId + 1;
    const doc = root.ownerDocument;
    const R = host.__EASYOFFICE_DOC_RUNS__;
    const span = doc.createElement("span");
    span.className = "eo-comment-range";
    span.dataset.eoCommentId = String(id);
    const hasSel = range && !range.collapsed && root.contains(range.startContainer) && !insideNoteOrMeta(root, range.startContainer);
    const isolated = hasSel ? R?.isolateRangeText(root, range) : null;
    if (isolated?.texts.length) wrapIsolatedRange(isolated, span);
    else {
      const pt = resolveBodyPoint(root, range, true);
      const word = range?.collapsed && R ? R.wordRangeAt(root, pt.node, pt.offset) : null;
      const isolatedWord = word ? R.isolateRangeText(root, word) : null;
      if (isolatedWord?.texts.length) wrapIsolatedRange(isolatedWord, span);
      else { span.classList.add("eo-comment-point"); span.setAttribute("contenteditable", "false"); insertNodeAtPoint(pt, span); }
    }
    const article = doc.createElement("article");
    article.dataset.eoCommentId = String(id);
    article.dataset.eoAuthor = sanitizeMeta(author, DEFAULT_AUTHOR, 128);
    article.dataset.eoDate = sanitizeMeta(date, nowIso(), 64);
    article.textContent = cleanText;
    ensureCommentsSection(root).appendChild(article);
    return { changed: true, id, span };
  }

  function deleteCommentDom(root, id) {
    const numId = Number(id);
    if (!root || !Number.isInteger(numId) || numId < 1) return { changed: false };
    const R = host.__EASYOFFICE_DOC_RUNS__;
    const spans = root.querySelectorAll(`span.eo-comment-range[data-eo-comment-id="${numId}"]`);
    const articles = root.querySelectorAll(`section[data-eo-comments] article[data-eo-comment-id="${numId}"]`);
    if (!spans.length && !articles.length) return { changed: false };
    const containers = new Set();
    for (const sp of spans) {
      if (sp.parentElement && R) containers.add(R.containerOf(sp.parentElement, root));
      if (!sp.classList?.contains("eo-comment-point")) while (sp.firstChild) sp.before(sp.firstChild);
      sp.remove();
    }
    containers.forEach((c) => { if (root.contains(c)) R.normalizeContainer(c); });
    for (const art of articles) art.remove();
    const sec = root.querySelector(":scope > section[data-eo-comments]");
    if (sec && !sec.children.length) sec.remove();
    return { changed: true };
  }

  function syncCommentsDom(root) {
    if (!root) return [];
    const quotes = new Map();
    const order = [];
    for (const sp of root.querySelectorAll("span.eo-comment-range[data-eo-comment-id]")) {
      if (insideNoteOrMeta(root, sp)) continue;
      const id = Number(sp.dataset.eoCommentId);
      if (!Number.isInteger(id) || id < 1) continue;
      if (!quotes.has(id)) { quotes.set(id, ""); order.push(id); }
      if (sp.textContent) quotes.set(id, quotes.get(id) + sp.textContent);
    }
    const sec = root.querySelector(":scope > section[data-eo-comments]");
    if (!sec) return [];
    const byId = new Map();
    for (const art of Array.from(sec.children)) {
      const id = Number(art.dataset?.eoCommentId);
      if (!Number.isInteger(id) || !quotes.has(id) || byId.has(id)) art.remove(); else byId.set(id, art);
    }
    if (!sec.children.length) { sec.remove(); return []; }
    return order.filter((id) => byId.has(id)).map((id) => {
      const art = byId.get(id);
      return { id, author: art.dataset.eoAuthor || DEFAULT_AUTHOR, date: art.dataset.eoDate || "", text: art.textContent || "", quote: quotes.get(id) || "" };
    });
  }

  function nextRevId(root) {
    let maxId = 0;
    const all = root.querySelectorAll("[data-eo-rev-id]");
    if (all.length >= MAX_REVISIONS) return 0;
    for (const el of all) maxId = Math.max(maxId, Number(el.dataset.eoRevId) || 0);
    return maxId + 1;
  }

  function ancestorRev(node, root, tagUpper) {
    for (let cur = node?.nodeType === 1 ? node : node?.parentElement; cur && cur !== root; cur = cur.parentElement) {
      if (tagOf(cur) === tagUpper && cur.dataset?.eoRevId !== undefined) return cur;
    }
    return null;
  }

  function edgeTextInside(el, doc, atEnd) {
    const step = atEnd ? -1 : 1;
    for (let i = atEnd ? el.childNodes.length - 1 : 0; i >= 0 && i < el.childNodes.length; i += step) {
      const c = el.childNodes[i];
      if (c.nodeType === 3) return c;
      if (c.nodeType === 1 && c.getAttribute?.("contenteditable") !== "false") return edgeTextInside(c, doc, atEnd);
    }
    const fresh = doc.createTextNode("");
    if (atEnd) el.appendChild(fresh); else el.insertBefore(fresh, el.firstChild ?? null);
    return fresh;
  }

  function stepCharInBlock(block, node, offset, dir) {
    const R = host.__EASYOFFICE_DOC_RUNS__;
    const entries = [];
    const walk = (cur) => {
      for (const child of cur.childNodes || []) {
        if (child.nodeType === 3 && child.data.length > 0) entries.push(child);
        else if (child.nodeType === 1 && child.getAttribute?.("contenteditable") !== "false" && !child.hidden && !(tagOf(child) === "DEL" && child.dataset?.eoRevId !== undefined)) walk(child);
      }
    };
    walk(block);
    const backLen = (s, end) => (end >= 2 && s.codePointAt(end - 2) > 0xffff ? 2 : 1);
    const fwdLen = (s, start) => (s.codePointAt(start) > 0xffff ? 2 : 1);
    for (let i = 0; i < entries.length; i += 1) {
      const t = entries[i];
      const len = t.data.length;
      if (dir < 0) {
        if (t === node && offset > 0) return { node: t, start: offset - backLen(t.data, offset), end: offset };
        const cmpEnd = R ? R.comparePoints(block, t, len, node, offset) : 0;
        const cmpStart = R ? R.comparePoints(block, t, 0, node, offset) : 0;
        if (cmpEnd <= 0 && (i === entries.length - 1 || (R && R.comparePoints(block, entries[i + 1], 0, node, offset) >= 0))) return { node: t, start: len - backLen(t.data, len), end: len };
        if (cmpStart < 0 && cmpEnd > 0) return { node: t, start: Math.max(0, offset - backLen(t.data, offset)), end: offset };
      } else {
        if (t === node && offset < len) return { node: t, start: offset, end: Math.min(len, offset + fwdLen(t.data, offset)) };
        if ((R ? R.comparePoints(block, t, 0, node, offset) : 0) >= 0) return { node: t, start: 0, end: Math.min(len, fwdLen(t.data, 0)) };
      }
    }
    return null;
  }

  function makeRevEl(doc, tag, id, author, date) {
    const el = Object.assign(doc.createElement(tag), { className: tag === "del" ? "eo-rev-del" : "eo-rev-ins" });
    Object.assign(el.dataset, { eoRevId: String(id), eoRevAuthor: author, eoRevDate: date });
    return el;
  }

  function markRangeDeletedDom(root, range, author, date) {
    const isolated = host.__EASYOFFICE_DOC_RUNS__?.isolateRangeText(root, range);
    if (!isolated?.texts.length) return { changed: false };
    const doc = root.ownerDocument;
    const revAuthor = sanitizeMeta(author, DEFAULT_AUTHOR, 128);
    const revDate = sanitizeMeta(date, nowIso(), 64);
    const block = enclosingBodyBlock(root, isolated.container) || isolated.container;
    let revId = 0;
    let lastAnchor = null;
    for (const t of isolated.texts) {
      const inDel = ancestorRev(t, root, "DEL");
      if (inDel) { lastAnchor = inDel; continue; }
      const inIns = ancestorRev(t, root, "INS");
      if (inIns) {
        const parent = inIns.parentElement;
        const idx = childIdx(inIns);
        t.remove();
        if (!inIns.textContent) { inIns.remove(); lastAnchor = { parent, offset: idx }; } else lastAnchor = inIns;
        continue;
      }
      if (!revId) revId = nextRevId(root);
      if (revId <= 0) break;
      const top = isolateInline(t, isolated.container);
      const prev = top.previousSibling;
      if (prev?.nodeType === 1 && tagOf(prev) === "DEL" && Number(prev.dataset?.eoRevId) === revId) { prev.appendChild(top); lastAnchor = prev; }
      else { const del = makeRevEl(doc, "del", revId, revAuthor, revDate); top.before(del); del.appendChild(top); lastAnchor = del; }
    }
    if (!block.childNodes.length) { block.appendChild(doc.createElement("br")); return { changed: true, caret: { node: block, offset: 0 } }; }
    if (lastAnchor?.parent) return { changed: true, caret: { node: lastAnchor.parent, offset: lastAnchor.offset } };
    if (lastAnchor?.parentElement) return { changed: true, id: revId || undefined, caret: { node: lastAnchor.parentElement, offset: childIdx(lastAnchor) + 1 } };
    return { changed: true };
  }

  function recordDeletionDom(root, range, dir, author, date) {
    if (!root) return { changed: false };
    if (range && !range.collapsed && root.contains(range.startContainer) && !insideNoteOrMeta(root, range.startContainer)) return markRangeDeletedDom(root, range, author, date);
    const pt = resolveBodyPoint(root, range, true);
    const step = stepCharInBlock(pt.block, pt.node, pt.offset, dir < 0 ? -1 : 1);
    if (!step) return { changed: false };
    const doc = root.ownerDocument;
    const { node: t, start, end } = step;
    const inIns = ancestorRev(t, root, "INS");
    if (inIns) {
      t.data = t.data.slice(0, start) + t.data.slice(end);
      if (!inIns.textContent) {
        const parent = inIns.parentElement;
        const idx = childIdx(inIns);
        inIns.remove();
        if (!pt.block.childNodes.length) { pt.block.appendChild(doc.createElement("br")); return { changed: true, caret: { node: pt.block, offset: 0 } }; }
        return { changed: true, caret: { node: parent, offset: idx } };
      }
      if (!t.data.length) { const parent = t.parentElement; const idx = childIdx(t); t.remove(); return { changed: true, caret: { node: parent, offset: idx } }; }
      return { changed: true, caret: { node: t, offset: start } };
    }
    const revAuthor = sanitizeMeta(author, DEFAULT_AUTHOR, 128);
    const revDate = sanitizeMeta(date, nowIso(), 64);
    const deleted = t.data.slice(start, end);
    const beforeStr = t.data.slice(0, start);
    const afterStr = t.data.slice(end);
    const parent = t.parentElement;
    let refAfter = t.nextSibling ?? null;
    if (beforeStr && afterStr) { t.data = beforeStr; refAfter = doc.createTextNode(afterStr); parent.insertBefore(refAfter, t.nextSibling ?? null); }
    else if (beforeStr) t.data = beforeStr;
    else if (afterStr) { t.data = afterStr; refAfter = t; }
    else t.remove();
    const prevSib = refAfter ? refAfter.previousSibling : parent.lastChild;
    if (dir < 0 && refAfter?.nodeType === 1 && tagOf(refAfter) === "DEL" && refAfter.dataset?.eoRevAuthor === revAuthor) {
      const ft = edgeTextInside(refAfter, doc, false);
      ft.data = deleted + ft.data;
      return { changed: true, id: Number(refAfter.dataset.eoRevId), caret: { node: parent, offset: childIdx(refAfter) } };
    }
    if (dir > 0 && prevSib?.nodeType === 1 && tagOf(prevSib) === "DEL" && prevSib.dataset?.eoRevAuthor === revAuthor) {
      edgeTextInside(prevSib, doc, true).data += deleted;
      return { changed: true, id: Number(prevSib.dataset.eoRevId), caret: { node: parent, offset: childIdx(prevSib) + 1 } };
    }
    const id = nextRevId(root);
    if (id <= 0) return { changed: false };
    const del = makeRevEl(doc, "del", id, revAuthor, revDate);
    del.appendChild(doc.createTextNode(deleted));
    parent.insertBefore(del, refAfter);
    const delIdx = childIdx(del);
    return { changed: true, id, caret: { node: parent, offset: dir < 0 ? delIdx : delIdx + 1 } };
  }

  function recordInsertionDom(root, range, text, author, date) {
    const str = String(text ?? "").slice(0, MAX_TEXT_CHARS);
    if (!root || !str.length) return { changed: false };
    const doc = root.ownerDocument;
    const revAuthor = sanitizeMeta(author, DEFAULT_AUTHOR, 128);
    const revDate = sanitizeMeta(date, nowIso(), 64);
    let working = range;
    if (working && !working.collapsed && root.contains(working.startContainer) && !insideNoteOrMeta(root, working.startContainer)) {
      const delRes = markRangeDeletedDom(root, working, revAuthor, revDate);
      if (delRes.caret) working = { startContainer: delRes.caret.node, startOffset: delRes.caret.offset, endContainer: delRes.caret.node, endOffset: delRes.caret.offset, collapsed: true };
    }
    let pt = resolveBodyPoint(root, working, false);
    const inDel = ancestorRev(pt.node, root, "DEL");
    if (inDel?.parentElement) pt = { block: pt.block, node: inDel.parentElement, offset: childIdx(inDel) + 1 };
    const inIns = ancestorRev(pt.node, root, "INS");
    if (inIns && (!author || inIns.dataset.eoRevAuthor === revAuthor)) {
      if (pt.node.nodeType === 3) {
        pt.node.data = pt.node.data.slice(0, pt.offset) + str + pt.node.data.slice(pt.offset);
        return { changed: true, id: Number(inIns.dataset.eoRevId), caret: { node: pt.node, offset: pt.offset + str.length } };
      }
      const lt = edgeTextInside(inIns, doc, true);
      lt.data += str;
      return { changed: true, id: Number(inIns.dataset.eoRevId), caret: { node: lt, offset: lt.data.length } };
    }
    const adjIns = pt.node.nodeType === 1 ? pt.node.childNodes[pt.offset - 1] : pt.offset === 0 ? pt.node.previousSibling : null;
    if (adjIns?.nodeType === 1 && tagOf(adjIns) === "INS" && adjIns.dataset?.eoRevId !== undefined && (!author || adjIns.dataset.eoRevAuthor === revAuthor)) {
      const lt = edgeTextInside(adjIns, doc, true);
      lt.data += str;
      return { changed: true, id: Number(adjIns.dataset.eoRevId), caret: { node: lt, offset: lt.data.length } };
    }
    const id = nextRevId(root);
    if (id <= 0) return { changed: false };
    const ins = makeRevEl(doc, "ins", id, revAuthor, revDate);
    const tn = doc.createTextNode(str);
    ins.appendChild(tn);
    insertNodeAtPoint(pt, ins);
    return { changed: true, id, caret: { node: tn, offset: str.length } };
  }

  function resolveRevisionsDom(root, targetId, accept) {
    if (!root) return { changed: false };
    const R = host.__EASYOFFICE_DOC_RUNS__;
    const all = Array.from(root.querySelectorAll("ins[data-eo-rev-id], del[data-eo-rev-id]")).filter((el) => targetId === null || Number(el.dataset.eoRevId) === Number(targetId));
    if (!all.length) return { changed: false };
    const containers = new Set();
    const blocks = new Set();
    for (const el of all) {
      const parent = el.parentElement;
      if (!parent) continue;
      if (R) containers.add(R.containerOf(parent, root));
      const blk = enclosingBodyBlock(root, parent);
      if (blk) blocks.add(blk);
      if ((accept && tagOf(el) === "INS") || (!accept && tagOf(el) !== "INS")) while (el.firstChild) el.before(el.firstChild);
      el.remove();
    }
    containers.forEach((c) => { if (root.contains(c)) R.normalizeContainer(c); });
    blocks.forEach((b) => { if (root.contains(b) && !b.childNodes.length) b.appendChild(root.ownerDocument.createElement("br")); });
    syncNotesDom(root);
    syncCommentsDom(root);
    return { changed: true };
  }

  host.__EASYOFFICE_DOC_NOTES__ = {
    NOTE_KINDS, NOTE_KIND_SET, DEFAULT_AUTHOR, MAX_NOTES, MAX_COMMENTS, MAX_TEXT_CHARS,
    isNoteOrMetaSection, insideNoteOrMeta, syncNotesDom, insertNoteDom,
    addCommentDom, deleteCommentDom, syncCommentsDom,
    recordInsertionDom, recordDeletionDom, resolveRevisionsDom,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
