(function (host) {
  "use strict";

  // UI controller for document footnotes (`각주`), endnotes (`미주`), comments (`새 메모`) and track changes
  // (`변경 내용 추적` · `수락` · `거부`). Uses verbatim Korean labels from the reference editor (`각주`, `미주`;
  // MIT, Copyright (c) 2025-2026 Edward Kim, see THIRD_PARTY_NOTICES.md) and standard word processor Korean UI.

  const HANDLED_COMMANDS = new Set(["mod-endnote", "review-accept-all", "review-reject-all"]);

  function formatDisplayDate(iso) {
    const raw = String(iso || "").trim();
    if (!raw) return "";
    const m = /^(\d{4}-\d{2}-\d{2})(?:[T ](\d{2}:\d{2}))?/.exec(raw);
    return m ? (m[2] ? `${m[1]} ${m[2]}` : m[1]) : raw.slice(0, 16);
  }

  function createDocNotesUi(deps) {
    const { document, env = host, getRange, withUndo, setFeedback, saveDocState, getActiveModule } = deps;
    const N = host.__EASYOFFICE_DOC_NOTES__;
    let fallbackCaret = null;
    let savedCommentRange = null;
    let paneDismissed = false;

    const editor = () => document.getElementById("doc-editor");
    const isDocOnly = () => getActiveModule?.() === "DOC";

    function setSelectionPoint(node, offset, focusBody = false) {
      if (!node) return;
      const root = editor();
      if (focusBody) root?.focus?.();
      const safeOff = Math.max(0, Number(offset) || 0);
      fallbackCaret = { startContainer: node, startOffset: safeOff, endContainer: node, endOffset: safeOff, collapsed: true };
      const sel = typeof env.getSelection === "function" ? env.getSelection() : null;
      const r = document.createRange?.();
      if (sel && r) {
        try {
          r.setStart(node, safeOff);
          r.setEnd(node, safeOff);
          sel.removeAllRanges();
          sel.addRange(r);
        } catch {
          // Detached or out-of-bounds node in synthetic DOM: fallbackCaret still holds the position.
        }
      }
    }

    function activeRange() {
      const root = editor();
      if (!root) return null;
      const sel = typeof env.getSelection === "function" ? env.getSelection() : null;
      if (sel && sel.rangeCount > 0) {
        const r = sel.getRangeAt(0);
        if (r && root.contains(r.startContainer)) return r;
      }
      const fromRuns = getRange?.();
      if (fromRuns && root.contains(fromRuns.startContainer)) return fromRuns;
      if (fallbackCaret && root.contains(fallbackCaret.startContainer)) return fallbackCaret;
      return null;
    }

    function notifyMutated(root) {
      if (!root) return;
      if (typeof env.Event !== "function") root.dispatchEvent?.({ type: "eo-doc-mutated", target: root });
    }

    function syncTrackMenu() {
      const root = editor();
      const on = isDocOnly() && root?.dataset?.eoTrackChanges === "true";
      for (const el of document.querySelectorAll('.md-item[data-studio-menu-cmd="review-track"]')) {
        el.setAttribute("aria-checked", on ? "true" : "false");
      }
    }

    function buildCardMeta(author, date) {
      const meta = document.createElement("div");
      meta.className = "doc-review-card-meta";
      const who = document.createElement("strong");
      who.textContent = author || N.DEFAULT_AUTHOR;
      meta.appendChild(who);
      const when = formatDisplayDate(date);
      if (when) {
        const timeEl = document.createElement("span");
        timeEl.textContent = when;
        meta.appendChild(timeEl);
      }
      return meta;
    }

    function buildActionBtn(label, attrName, attrVal) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "dialog-btn doc-review-btn";
      btn.dataset[attrName] = attrVal;
      btn.textContent = label;
      return btn;
    }

    function renderReviewPane(comments, revisions) {
      const pane = document.getElementById("doc-review-pane");
      const list = document.getElementById("doc-review-list");
      const title = document.getElementById("doc-review-title");
      const root = editor();
      if (!pane || !list) return;
      const trackOn = root?.dataset?.eoTrackChanges === "true";
      const hasAny = comments.length > 0 || revisions.length > 0 || trackOn;
      if (!hasAny) paneDismissed = false;
      if (!isDocOnly() || !hasAny || paneDismissed) {
        pane.hidden = true;
        list.replaceChildren();
        return;
      }
      pane.hidden = false;
      if (title) {
        if (comments.length > 0 && (revisions.length > 0 || trackOn)) title.textContent = "메모 · 변경 내용";
        else if (revisions.length > 0 || trackOn) title.textContent = "변경 내용";
        else title.textContent = "메모";
      }
      const cards = [];
      for (const c of comments) {
        const card = document.createElement("div");
        card.className = "doc-review-card";
        card.dataset.reviewCommentId = String(c.id);
        card.appendChild(buildCardMeta(c.author, c.date));
        if (c.quote) {
          const quote = document.createElement("div");
          quote.className = "doc-review-quote";
          quote.textContent = c.quote;
          card.appendChild(quote);
        }
        const body = document.createElement("p");
        body.className = "doc-review-text";
        body.textContent = c.text;
        card.appendChild(body);
        const actions = document.createElement("div");
        actions.className = "doc-review-actions";
        actions.appendChild(buildActionBtn("삭제", "commentAct", "delete"));
        card.appendChild(actions);
        cards.push(card);
      }
      for (const r of revisions) {
        const card = document.createElement("div");
        card.className = "doc-review-card doc-review-card--" + r.kind;
        card.dataset.reviewRevId = String(r.id);
        card.appendChild(buildCardMeta(r.author, r.date));
        const body = document.createElement("p");
        body.className = "doc-review-text";
        body.textContent = r.text;
        card.appendChild(body);
        const actions = document.createElement("div");
        actions.className = "doc-review-actions";
        actions.appendChild(buildActionBtn("수락", "revAct", "accept"));
        actions.appendChild(buildActionBtn("거부", "revAct", "reject"));
        card.appendChild(actions);
        cards.push(card);
      }
      list.replaceChildren(...cards);
    }

    function listRevisions(root) {
      const byKey = new Map();
      const out = [];
      for (const el of root.querySelectorAll("ins[data-eo-rev-id], del[data-eo-rev-id]")) {
        if (N.insideNoteOrMeta(root, el)) continue;
        const id = Number(el.dataset.eoRevId);
        if (!Number.isInteger(id) || id < 1) continue;
        const kind = (el.tagName || "").toUpperCase() === "DEL" ? "delete" : "insert";
        const key = kind + ":" + id;
        let entry = byKey.get(key);
        if (!entry) {
          entry = { id, kind, author: el.dataset.eoRevAuthor || N.DEFAULT_AUTHOR, date: el.dataset.eoRevDate || "", text: "" };
          byKey.set(key, entry);
          out.push(entry);
        }
        entry.text += el.textContent || "";
      }
      return out;
    }

    function syncAll() {
      const root = editor();
      if (!root) return;
      N.syncNotesDom(root);
      const comments = N.syncCommentsDom(root);
      const revisions = listRevisions(root);
      syncTrackMenu();
      renderReviewPane(comments, revisions);
    }

    function runMutation(exec, options = {}) {
      const root = editor();
      if (!root) return { changed: false };
      const wrapped = () => {
        const res = exec(root);
        if (res.changed) {
          N.syncNotesDom(root);
          N.syncCommentsDom(root);
        }
        return res;
      };
      const res = options.undo === false || typeof withUndo !== "function" ? wrapped() : withUndo(wrapped);
      if (res.changed) {
        if (options.undo === false) saveDocState?.();
        else notifyMutated(root);
      }
      syncAll();
      return res;
    }

    function insertNote(kind, text, options = {}) {
      const normKind = kind === "endnote" ? "endnote" : "footnote";
      paneDismissed = false;
      const range = options.range || activeRange();
      const res = runMutation((root) => N.insertNoteDom(root, range, normKind, text), options);
      if (res.changed) {
        if (res.bodyEl && text === undefined) {
          setSelectionPoint(res.bodyEl, 0, true);
        } else if (res.refEl?.parentElement) {
          const idx = Array.prototype.indexOf.call(res.refEl.parentElement.childNodes, res.refEl) + 1;
          setSelectionPoint(res.refEl.parentElement, idx, false);
        }
        if (options.feedback !== false) setFeedback?.(normKind === "endnote" ? "미주" : "각주");
      }
      return res;
    }

    function openNewCommentDialog() {
      if (!isDocOnly()) return false;
      savedCommentRange = activeRange();
      const dlg = document.getElementById("doc-comment-dialog");
      const input = document.getElementById("doc-comment-input");
      if (!dlg) return false;
      if (input) input.value = "";
      if (typeof dlg.showModal === "function" && !dlg.open) dlg.showModal();
      else dlg.open = true;
      input?.focus?.();
      return true;
    }

    function addComment(text, author, date, options = {}) {
      paneDismissed = false;
      const range = options.range || activeRange();
      const res = runMutation((root) => N.addCommentDom(root, range, text, author, date), options);
      if (res.changed && options.feedback !== false) setFeedback?.("새 메모");
      return res;
    }

    function deleteComment(id, options = {}) {
      const res = runMutation((root) => N.deleteCommentDom(root, id), options);
      if (res.changed && options.feedback !== false) setFeedback?.("삭제");
      return res;
    }

    function toggleTrackChanges(force, options = {}) {
      const root = editor();
      if (!root) return { changed: false, enabled: false };
      const cur = root.dataset.eoTrackChanges === "true";
      const next = typeof force === "boolean" ? force : !cur;
      if (next) paneDismissed = false;
      if (next === cur) {
        syncAll();
        return { changed: false, enabled: next };
      }
      const res = runMutation((el) => {
        if (next) el.dataset.eoTrackChanges = "true";
        else delete el.dataset.eoTrackChanges;
        return { changed: true, enabled: next };
      }, options);
      if (options.feedback !== false) setFeedback?.("변경 내용 추적");
      return res;
    }

    function recordInsertion(text, author, date, options = {}) {
      paneDismissed = false;
      const range = options.range || activeRange();
      const res = runMutation((root) => N.recordInsertionDom(root, range, text, author, date), options);
      if (res.changed && res.caret) setSelectionPoint(res.caret.node, res.caret.offset, false);
      return res;
    }

    function recordDeletion(range, authorOrOpts, date, maybeOpts) {
      paneDismissed = false;
      const isOpts = authorOrOpts && typeof authorOrOpts === "object" && !("startContainer" in authorOrOpts);
      const author = isOpts ? undefined : authorOrOpts;
      const options = (isOpts ? authorOrOpts : maybeOpts) || {};
      const dir = options.dir === 1 ? 1 : -1;
      const targetRange = range || options.range || activeRange();
      const res = runMutation((root) => N.recordDeletionDom(root, targetRange, dir, author, date), options);
      if (res.changed && res.caret) setSelectionPoint(res.caret.node, res.caret.offset, false);
      return res;
    }

    function acceptRevision(id, options = {}) {
      const res = runMutation((root) => N.resolveRevisionsDom(root, id, true), options);
      if (res.changed && options.feedback !== false) setFeedback?.("수락");
      return res;
    }

    function rejectRevision(id, options = {}) {
      const res = runMutation((root) => N.resolveRevisionsDom(root, id, false), options);
      if (res.changed && options.feedback !== false) setFeedback?.("거부");
      return res;
    }

    function acceptAllRevisions(options = {}) {
      const res = runMutation((root) => N.resolveRevisionsDom(root, null, true), options);
      if (res.changed && options.feedback !== false) setFeedback?.("모든 변경 내용 수락");
      return res;
    }

    function rejectAllRevisions(options = {}) {
      const res = runMutation((root) => N.resolveRevisionsDom(root, null, false), options);
      if (res.changed && options.feedback !== false) setFeedback?.("모든 변경 내용 거부");
      return res;
    }

    function onBeforeInput(ev) {
      const root = editor();
      if (!root || !isDocOnly() || root.dataset.eoTrackChanges !== "true" || ev.isComposing) return;
      const r = activeRange();
      const checkNode = r?.startContainer || ev.target;
      if (checkNode && N.insideNoteOrMeta(root, checkNode)) return;
      if (ev.inputType === "insertText" && typeof ev.data === "string" && ev.data.length > 0) {
        ev.preventDefault?.();
        ev.stopImmediatePropagation?.();
        recordInsertion(ev.data, undefined, undefined, { feedback: false });
      } else if (ev.inputType === "deleteContentBackward" || ev.inputType === "deleteContentForward") {
        ev.preventDefault?.();
        ev.stopImmediatePropagation?.();
        recordDeletion(r, { dir: ev.inputType === "deleteContentForward" ? 1 : -1, feedback: false });
      }
    }

    function onEditorClick(ev) {
      const root = editor();
      if (!root) return;
      const ref = ev.target?.closest?.("sup.eo-note-ref");
      if (ref && root.contains(ref)) {
        const kind = ref.dataset.eoNoteKind;
        const id = ref.dataset.eoNoteId;
        const body = root.querySelector(`section[data-eo-notes="${kind}"] .eo-doc-note-item[data-eo-note-id="${id}"] .eo-doc-note-body`);
        if (body) {
          ev.preventDefault?.();
          const first = body.firstChild;
          if (first?.nodeType === 3) setSelectionPoint(first, first.data.length, true);
          else setSelectionPoint(body, 0, true);
          body.scrollIntoView?.({ block: "nearest" });
        }
        return;
      }
      const num = ev.target?.closest?.(".eo-doc-note-num");
      if (num && root.contains(num)) {
        const sec = num.closest?.("section[data-eo-notes]");
        const item = num.closest?.(".eo-doc-note-item");
        const targetRef = sec && item
          ? root.querySelector(`sup.eo-note-ref[data-eo-note-kind="${sec.dataset.eoNotes}"][data-eo-note-id="${item.dataset.eoNoteId}"]`)
          : null;
        if (targetRef?.parentElement) {
          ev.preventDefault?.();
          const idx = Array.prototype.indexOf.call(targetRef.parentElement.childNodes, targetRef) + 1;
          setSelectionPoint(targetRef.parentElement, idx, true);
          targetRef.scrollIntoView?.({ block: "nearest" });
        }
      }
    }

    function submitCommentDialog() {
      const dlg = document.getElementById("doc-comment-dialog");
      const input = document.getElementById("doc-comment-input");
      const text = String(input?.value ?? "").trim();
      if (!text) {
        input?.focus?.();
        return;
      }
      if (typeof dlg?.close === "function") dlg.close();
      else if (dlg) dlg.open = false;
      addComment(text, undefined, undefined, { range: savedCommentRange });
      savedCommentRange = null;
    }

    function onReviewPaneClick(ev) {
      const commentBtn = ev.target?.closest?.("button[data-comment-act]");
      if (commentBtn?.dataset.commentAct === "delete") {
        const card = commentBtn.closest("[data-review-comment-id]");
        if (card) deleteComment(Number(card.dataset.reviewCommentId));
        return;
      }
      const revBtn = ev.target?.closest?.("button[data-rev-act]");
      if (revBtn) {
        const card = revBtn.closest("[data-review-rev-id]");
        const id = Number(card?.dataset.reviewRevId);
        if (revBtn.dataset.revAct === "accept") acceptRevision(id);
        else if (revBtn.dataset.revAct === "reject") rejectRevision(id);
      }
    }

    const handles = (cmd) => HANDLED_COMMANDS.has(cmd);

    function run(cmd) {
      if (!isDocOnly()) return { changed: false };
      if (cmd === "mod-endnote") return insertNote("endnote");
      if (cmd === "review-accept-all") return acceptAllRevisions();
      if (cmd === "review-reject-all") return rejectAllRevisions();
      return { changed: false };
    }

    function bind() {
      const root = editor();
      root?.addEventListener("beforeinput", onBeforeInput);
      root?.addEventListener("input", () => { if (isDocOnly()) syncAll(); });
      root?.addEventListener("click", onEditorClick);
      document.getElementById("btn-doc-comment-ok")?.addEventListener("click", submitCommentDialog);
      document.getElementById("doc-comment-input")?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" && (ev.ctrlKey || ev.metaKey)) { ev.preventDefault?.(); submitCommentDialog(); }
      });
      document.getElementById("btn-doc-review-close")?.addEventListener("click", () => {
        paneDismissed = true;
        const pane = document.getElementById("doc-review-pane");
        if (pane) pane.hidden = true;
      });
      document.getElementById("doc-review-pane")?.addEventListener("click", onReviewPaneClick);
      syncAll();
    }

    const api = {
      bind, handles, run, syncAll,
      insertNote, openNewCommentDialog, addComment, deleteComment,
      toggleTrackChanges, recordInsertion, recordDeletion,
      acceptRevision, rejectRevision, acceptAllRevisions, rejectAllRevisions,
    };
    host.__EASYOFFICE_DOC_NOTES_API__ = api;
    return api;
  }

  const textSpec = (v) => ({ t: "text", v: String(v) });
  const elSpec = (tag, children, attrs) => ({ t: "el", tag, attrs: attrs || {}, data: {}, style: {}, children: children || [] });

  function normalizeDecodedRev(raw) {
    const id = Number(raw?.id);
    const kind = raw?.kind;
    if (!Number.isInteger(id) || id < 1 || (kind !== "insert" && kind !== "delete")) return null;
    return { id: String(id), tag: kind === "delete" ? "del" : "ins", author: String(raw.author || "작성자").slice(0, 128), date: String(raw.date || "").slice(0, 64) };
  }

  function pushWrappedPiece(target, piece, commentId, rev) {
    let node = piece;
    if (commentId) {
      const prev = !rev ? target.at(-1) : null;
      if (prev?.tag === "span" && prev.data?.eoCommentId === commentId && !String(prev.attrs?.class || "").includes("eo-comment-point")) return void prev.children.push(node);
      node = { ...elSpec("span", [node], { class: "eo-comment-range" }), data: { eoCommentId: commentId } };
    }
    if (rev) {
      const prev = target.at(-1);
      if (prev?.tag === rev.tag && prev.data?.eoRevId === rev.id && prev.data?.eoRevAuthor === rev.author && prev.data?.eoRevDate === rev.date) {
        const prevInner = commentId ? prev.children.at(-1) : null;
        if (commentId && prevInner?.tag === "span" && prevInner.data?.eoCommentId === commentId && !String(prevInner.attrs?.class || "").includes("eo-comment-point")) prevInner.children.push(piece);
        else prev.children.push(node);
        return;
      }
      node = { ...elSpec(rev.tag, [node], { class: rev.tag === "del" ? "eo-rev-del" : "eo-rev-ins" }), data: { eoRevId: rev.id, eoRevAuthor: rev.author, eoRevDate: rev.date } };
    }
    target.push(node);
  }

  function annotationSectionSpecs(res) {
    const sections = [];
    const rawNotes = Array.isArray(res?.notes) ? res.notes.slice(0, 2048) : [];
    for (const kind of ["footnote", "endnote"]) {
      const items = [];
      rawNotes.forEach((n) => {
        const id = Number(n?.id);
        if (n?.kind !== kind || !Number.isInteger(id) || id < 1) return;
        const clean = String(n.text ?? "").slice(0, 65536);
        const numSpec = elSpec("span", [textSpec(items.length + 1 + ")")], { class: "eo-doc-note-num", contenteditable: "false" });
        const bodySpec = elSpec("p", clean.length > 0 ? [textSpec(clean)] : [elSpec("br")], { class: "eo-doc-note-body" });
        items.push({ ...elSpec("div", [numSpec, bodySpec], { class: "eo-doc-note-item" }), data: { eoNoteId: String(id) } });
      });
      if (items.length > 0) sections.push({ ...elSpec("section", items, { class: "eo-doc-notes eo-doc-" + kind }), data: { eoNotes: kind } });
    }
    const articles = (Array.isArray(res?.comments) ? res.comments.slice(0, 2048) : []).flatMap((c) => {
      const id = Number(c?.id);
      if (!Number.isInteger(id) || id < 1) return [];
      const text = String(c.text ?? "").slice(0, 65536);
      return [{ ...elSpec("article", text ? [textSpec(text)] : []), data: { eoCommentId: String(id), eoAuthor: String(c.author || "작성자").slice(0, 128), eoDate: String(c.date || "").slice(0, 64) } }];
    });
    if (articles.length > 0) sections.push({ ...elSpec("section", articles, { hidden: "" }), data: { eoComments: "" } });
    return sections;
  }

  host.__EASYOFFICE_DOC_NOTES_WIRE__ = { normalizeDecodedRev, pushWrappedPiece, annotationSectionSpecs };
  host.__EASYOFFICE_CREATE_DOC_NOTES_UI__ = createDocNotesUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
