(function (host) {
  "use strict";

  // Find source for the document editor (#doc-editor). It walks the text nodes once and groups them
  // into blocks (a paragraph, a heading, a table cell, a line ended by <br>), keeping each block's
  // text-node list and start offsets. A MutationObserver marks what changed: typing changes one
  // block's text node (that block is read again), a structure change walks the tree once and keeps
  // the records of unchanged blocks, so the match index behind it never searches twice.

  const { segmentStarts, segmentAt, applyEdits } = host.__EASYOFFICE_FIND_CORE__;
  const BLOCK_TAGS = new Set([
    "P", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "TD", "TH", "BLOCKQUOTE", "PRE", "FIGCAPTION", "CAPTION", "DT", "DD", "DIV",
    "UL", "OL", "TABLE", "TR", "SECTION", "ARTICLE", "HEADER", "FOOTER",
  ]);
  const SKIPPED_TAGS = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE"]);
  const SKIPPED_CLASS = "doc-table-toolbar";
  const HIGHLIGHT_MATCH = "eo-find-match";
  const HIGHLIGHT_CURRENT = "eo-find-current";

  function createDocFindSource(deps) {
    const { root, document, withUndo } = deps;
    const env = deps.env || host;
    const MutationObserverCtor = env.MutationObserver;
    const highlights = env.CSS?.highlights;
    const HighlightCtor = env.Highlight;
    const useHighlights = Boolean(highlights && typeof HighlightCtor === "function");

    let records = null; // block records in document order, null before the first scan
    let recordOfNode = new WeakMap(); // text node -> its record
    let structureDirty = true;
    const dirty = new Set();
    let listener = null;
    let observer = null;

    function newRecord(nodes) {
      const rec = { nodes, text: "", starts: [] };
      refresh(rec);
      return rec;
    }

    function refresh(rec) {
      const segments = rec.nodes.map((node) => node.data);
      rec.text = segments.join("");
      rec.starts = segmentStarts(segments);
    }

    // ---- scanning ---------------------------------------------------------------------------

    function isSkipped(el) {
      return SKIPPED_TAGS.has(el.tagName) || el.classList?.contains(SKIPPED_CLASS) === true;
    }

    function collectGroups() {
      const groups = [];
      let cur = null;
      const walk = (node) => {
        for (const child of node.childNodes) {
          if (child.nodeType === 3) {
            if (!cur) groups.push((cur = []));
            cur.push(child);
          } else if (child.nodeType === 1 && !isSkipped(child)) {
            if (child.tagName === "BR") {
              cur = null;
            } else if (BLOCK_TAGS.has(child.tagName)) {
              cur = null;
              walk(child);
              cur = null;
            } else {
              walk(child);
            }
          }
        }
      };
      walk(root);
      return groups;
    }

    // Walks the editor once. A group of text nodes that was a record before keeps that record, so the
    // index keeps the matches of every block the edit did not touch.
    function rescan() {
      const next = [];
      const nextOfNode = new WeakMap();
      for (const nodes of collectGroups()) {
        const was = recordOfNode.get(nodes[0]);
        const same = was && was.nodes.length === nodes.length && was.nodes.every((n, i) => n === nodes[i]);
        const rec = same ? was : newRecord(nodes);
        if (same) refresh(rec);
        for (const node of nodes) nextOfNode.set(node, rec);
        next.push(rec);
      }
      records = next;
      recordOfNode = nextOfNode;
      structureDirty = false;
    }

    // Brings `index` up to date: a rescan after a structure change, otherwise only the blocks whose
    // text node changed.
    function sync(index) {
      if (structureDirty || records === null) {
        rescan();
        dirty.clear();
        index.setBlocks(records.map((rec) => ({ id: rec, text: rec.text })));
        return;
      }
      for (const rec of dirty) {
        refresh(rec);
        index.updateBlock(rec, rec.text);
      }
      dirty.clear();
    }

    // ---- change tracking --------------------------------------------------------------------

    function onMutations(list) {
      for (const mutation of list) {
        const rec = mutation.type === "characterData" ? recordOfNode.get(mutation.target) : null;
        if (rec) dirty.add(rec);
        else structureDirty = true;
      }
      if (listener) listener();
    }

    function watch(onChange) {
      listener = onChange;
      if (observer || typeof MutationObserverCtor !== "function") return;
      observer = new MutationObserverCtor(onMutations);
      observer.observe(root, { characterData: true, childList: true, subtree: true });
    }

    // ---- ranges, selection, highlight -------------------------------------------------------

    function rangeOf(rec, start, end) {
      const first = segmentAt(rec.starts, start);
      const last = Math.max(first, segmentAt(rec.starts, end, true));
      const range = document.createRange();
      range.setStart(rec.nodes[first], start - rec.starts[first]);
      range.setEnd(rec.nodes[last], end - rec.starts[last]);
      return range;
    }

    // Where the caret is inside the editor, as a (record, offset) anchor; null when it is elsewhere.
    function anchor() {
      const sel = env.getSelection?.();
      const node = sel?.anchorNode;
      const rec = node && recordOfNode.get(node);
      if (!rec) return null;
      return { id: rec, offset: rec.starts[rec.nodes.indexOf(node)] + sel.anchorOffset };
    }

    function clearHighlight() {
      if (!useHighlights) return;
      highlights.delete(HIGHLIGHT_MATCH);
      highlights.delete(HIGHLIGHT_CURRENT);
    }

    // `near` are the matches around the current one, `current` the one to show (or null).
    function show(near, current) {
      if (!current) return clearHighlight();
      const range = rangeOf(current.id, current.start, current.end);
      if (useHighlights) {
        highlights.set(HIGHLIGHT_MATCH, new HighlightCtor(...near.map((m) => rangeOf(m.id, m.start, m.end))));
        highlights.set(HIGHLIGHT_CURRENT, new HighlightCtor(range));
      } else {
        const sel = env.getSelection?.();
        sel?.removeAllRanges();
        sel?.addRange(range);
      }
      range.startContainer.parentElement?.scrollIntoView?.({ block: "center", inline: "nearest" });
    }

    // ---- replacing --------------------------------------------------------------------------

    // `plan` is `[{ id, edits }]`; the text nodes are rewritten in place (run formatting stays) as one undo step.
    function replace(plan) {
      return withUndo(() => {
        let count = 0;
        for (const { id: rec, edits } of plan) {
          const before = rec.nodes.map((node) => node.data);
          const after = applyEdits(before, edits);
          rec.nodes.forEach((node, i) => {
            if (after[i] === before[i]) return;
            node.data = after[i];
          });
          dirty.add(rec);
          count += edits.length;
        }
        return { replaced: count, skipped: 0 };
      });
    }

    function dispose() {
      observer?.disconnect();
      observer = null;
      listener = null;
      clearHighlight();
    }

    return { kind: "doc", sync, watch, anchor, show, clearHighlight, replace, dispose };
  }

  host.__EASYOFFICE_CREATE_DOC_FIND_SOURCE__ = createDocFindSource;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
