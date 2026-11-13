(function (host) {
  "use strict";

  // Pages of the document editor. `#doc-editor` stays one contenteditable flow; this module measures its blocks,
  // runs the page arithmetic of `doc-pages-core.js` and shows the result: a layer of paper sheets (with the
  // page number) behind the text, a margin on blocks that move to the next page, and a float shape on the
  // paragraphs that split between two pages. It owns the page numbers: `pageCount()`, `currentPage()`,
  // `pageOf(block)` and `headingPages()` are the API the status bar and the table of contents read.
  //
  // Cost: a keystroke marks one block dirty; the next animation frame measures that block (and the one before
  // it, for the gap) in one read phase, and only when its height or gap changed does it re-run the arithmetic
  // over the cached heights (no DOM reads) and write the margins and bands that moved, then read the heights
  // of the split paragraphs once to confirm the estimate.

  const core = host.__EASYOFFICE_DOC_PAGES_CORE__;
  const LAYER_CLASS = "doc-page-layer";
  const SHEET_CLASS = "doc-page-sheet";
  const PAGED_CLASS = "eo-paged";
  const SKIPPED_TAGS = new Set(["BR", "SCRIPT", "STYLE", "TEMPLATE", "FOOTER"]);
  const FLOW_CONTAINERS = new Set(["UL", "OL"]);
  const WHOLE_TAGS = new Set(["TABLE", "FIGURE", "IMG", "HR", "SVG", "CANVAS", "VIDEO"]);
  const HEADING_RE = /^H[1-6]$/;
  const TEXT_BLOCK_RE = /^(P|H[1-6]|LI|DIV)$/;
  const SPLIT_DISPLAYS = new Set(["block", "list-item", "flow-root"]);
  const MAX_VERIFY_ROUNDS = 3;
  const VERIFY_TOLERANCE_PX = 0.5;
  const WATCHED_ATTRS = ["style", "class", "data-eo-break", "hidden", "data-paper-size"];
  const DEFAULT_LINE_FACTOR = 1.2;

  function createDocPages(deps) {
    const document = deps.document;
    const env = deps.env || host;
    const raf = (fn) => (env.requestAnimationFrame ? env.requestAnimationFrame(fn) : env.setTimeout(fn, 16));
    const canSplit = !env.CSS?.supports || env.CSS.supports("shape-outside", "polygon(0 0, 1px 0, 1px 1px)");

    let editor = null;
    let layer = null;
    let observer = null;
    let sizeObserver = null;
    let geo = null;
    let geoKey = "";
    let scheduled = false;
    let allDirty = true;
    let active = false;
    let units = [];
    let result = null;
    let lastCurrent = 1;
    let lastCount = 1;
    let selectionQueued = false;
    const recs = new WeakMap();
    const dirty = new Set();
    const styleCache = new Map();
    const listeners = new Set(deps.onChange ? [deps.onChange] : []);
    const sheetListeners = new Set();
    const bound = [];

    // ---- units ------------------------------------------------------------------------------

    // The header and footer sections (`doc-page-frame.js`) are drawn in the margins, not laid out in the flow.
    const isUnitSkipped = (el) => SKIPPED_TAGS.has(el.tagName) || el.hidden || el.dataset?.eoHf !== undefined;
    const isFlowContainer = (el) => FLOW_CONTAINERS.has(el.tagName) || el.classList.contains("eo-doc-toc");

    // The blocks the pages are cut between: the editor's children, with lists and the table-of-contents block
    // replaced by their items so a long list splits between its items.
    function collectUnits() {
      const out = [];
      for (const child of editor.children) {
        if (isUnitSkipped(child)) continue;
        if (isFlowContainer(child) && child.children.length > 0) {
          for (const item of child.children) if (!isUnitSkipped(item)) out.push(item);
        } else {
          out.push(child);
        }
      }
      return out;
    }

    function unitOf(node) {
      let el = node?.nodeType === 1 ? node : node?.parentElement;
      while (el && el !== editor) {
        const parent = el.parentElement;
        if (parent === editor || (parent && parent.parentElement === editor && isFlowContainer(parent))) return recs.has(el) ? el : null;
        el = parent;
      }
      return null;
    }

    function styleInfo(el) {
      const key = el.tagName + "|" + el.className + "|" + (el.getAttribute("style") || "");
      let info = styleCache.get(key);
      if (!info) {
        const cs = env.getComputedStyle(el);
        const size = Number.parseFloat(cs.fontSize) || 16;
        const lh = Number.parseFloat(cs.lineHeight) || size * DEFAULT_LINE_FACTOR;
        const lines = canSplit && !WHOLE_TAGS.has(el.tagName) && SPLIT_DISPLAYS.has(cs.display);
        info = { lines, lh, keep: HEADING_RE.test(el.tagName) };
        styleCache.set(key, info);
      }
      return info;
    }

    // ---- geometry ---------------------------------------------------------------------------

    function readGeometry() {
      const cs = env.getComputedStyle(editor);
      const px = (v) => Number.parseFloat(v) || 0;
      // A header or footer that needs more room than the margin raises the body's start or end on every page
      // (`doc-page-frame.js`); the first block of the document is then pushed down like any block that starts early.
      const hf = (name) => px(cs.getPropertyValue?.(name));
      const padding = {
        top: Math.max(px(cs.paddingTop), hf("--pg-hf-top")),
        right: px(cs.paddingRight),
        bottom: Math.max(px(cs.paddingBottom), hf("--pg-hf-bottom")),
        left: px(cs.paddingLeft),
      };
      const next = core.pageGeometry({ ...core.paperOf(editor), padding });
      next.cssPadTop = px(cs.paddingTop);
      const key = [next.width, next.height, next.padTop, next.padBottom, next.padLeft, next.padRight].join(",");
      return { next, changed: key !== geoKey, key };
    }

    function applyGeometry(next) {
      editor.style.setProperty("--pg-w", next.width + "px");
      editor.style.setProperty("--pg-h", next.height + "px");
      // The margins the header and footer are drawn in.
      editor.style.setProperty("--pg-pad-t", next.padTop + "px");
      editor.style.setProperty("--pg-pad-b", next.padBottom + "px");
      editor.style.setProperty("--pg-pad-l", next.padLeft + "px");
      editor.style.setProperty("--pg-pad-r", next.padRight + "px");
    }

    // ---- paper layer ------------------------------------------------------------------------

    function ensureLayer() {
      if (layer?.parentNode === editor) return layer;
      layer = editor.querySelector(":scope > footer." + LAYER_CLASS);
      if (!layer) {
        layer = document.createElement("footer");
        layer.className = LAYER_CLASS;
        layer.setAttribute("aria-hidden", "true");
        layer.setAttribute("contenteditable", "false");
        editor.appendChild(layer);
      }
      return layer;
    }

    function syncSheets(count, force) {
      const host = ensureLayer();
      if (force) while (host.lastChild) host.lastChild.remove();
      while (host.children.length > count) host.lastChild.remove();
      for (let k = host.children.length; k < count; k += 1) {
        const sheet = document.createElement("i");
        sheet.className = SHEET_CLASS;
        sheet.style.top = k * geo.pitch + "px";
        host.appendChild(sheet);
      }
      editor.style.setProperty("--pg-doc-h", count * geo.pitch - geo.gap + "px");
    }

    // ---- measuring --------------------------------------------------------------------------

    function clearApplied(el, rec) {
      if (!rec.flags) return;
      el.removeAttribute("data-eo-pg");
      el.style.removeProperty("--pg-mt");
      el.style.removeProperty("--pg-shape");
      el.style.removeProperty("--pg-fh");
      rec.flags = "";
      rec.mt = 0;
      rec.sig = "";
    }

    // Reads the natural height and gap of every block in `need` (their page styling cleared first): one write
    // phase, then one read phase.
    function measure(list, need) {
      for (const el of need) {
        const rec = recs.get(el);
        if (rec) clearApplied(el, rec);
      }
      const ed = editor.getBoundingClientRect();
      const scale = editor.offsetWidth > 0 ? ed.width / editor.offsetWidth : 1;
      const rects = new Map();
      const rectOf = (el) => {
        let r = rects.get(el);
        if (!r) {
          r = el.getBoundingClientRect();
          rects.set(el, r);
        }
        return r;
      };
      let changed = false;
      for (const el of need) {
        const index = list.indexOf(el);
        const r = rectOf(el);
        const prevBottom = index > 0 ? rectOf(list[index - 1]).bottom : ed.top;
        const info = styleInfo(el);
        const h = r.height / scale;
        const gap = (r.top - prevBottom) / scale;
        let rec = recs.get(el);
        if (!rec) {
          rec = { flags: "", mt: 0, sig: "", h: NaN, gap: NaN, hAct: 0, actSig: "" };
          recs.set(el, rec);
        }
        if (Math.abs(rec.h - h) > 0.01 || Math.abs(rec.gap - gap) > 0.01) changed = true;
        Object.assign(rec, { h, gap, lines: info.lines, lh: info.lh, keep: info.keep, brk: el.dataset.eoBreak === "page", hAct: 0, actSig: "" });
      }
      return changed;
    }

    // The first measure of a whole document: one pass in order, the previous block's bottom carried along.
    function measureAll(list) {
      const ed = editor.getBoundingClientRect();
      const scale = editor.offsetWidth > 0 ? ed.width / editor.offsetWidth : 1;
      for (const el of list) {
        const rec = recs.get(el);
        if (rec) clearApplied(el, rec);
      }
      let prevBottom = ed.top;
      for (const el of list) {
        const r = el.getBoundingClientRect();
        const info = styleInfo(el);
        const rec = { flags: "", mt: 0, sig: "", h: r.height / scale, gap: (r.top - prevBottom) / scale, hAct: 0, actSig: "",
          lines: info.lines, lh: info.lh, keep: info.keep, brk: el.dataset.eoBreak === "page" };
        recs.set(el, rec);
        prevBottom = r.bottom;
      }
    }

    // ---- applying ---------------------------------------------------------------------------

    // The margin that carries a block to where the plan puts it: its measured gap plus the push. The first block's
    // gap is measured from the editor's edge, so it holds the editor's own top padding, which is not margin.
    function marginFor(rec, plan, i) {
      if (!(plan.push[i] > core.EPS)) return 0;
      return core.round2(rec.gap - (i === 0 ? geo.cssPadTop : 0) + plan.push[i]);
    }

    function applyPlan(list, plan) {
      for (let i = 0; i < list.length; i += 1) {
        const rec = recs.get(list[i]);
        const mt = marginFor(rec, plan, i);
        const sig = plan.sig[i];
        if (mt === rec.mt && sig === rec.sig) continue;
        const el = list[i];
        if (mt !== rec.mt) {
          if (mt) el.style.setProperty("--pg-mt", mt + "px");
          else el.style.removeProperty("--pg-mt");
          rec.mt = mt;
        }
        if (sig !== rec.sig) {
          if (plan.bands[i]) {
            const { shape, height } = core.bandShape(plan.bands[i]);
            el.style.setProperty("--pg-shape", shape);
            el.style.setProperty("--pg-fh", height + "px");
          } else {
            el.style.removeProperty("--pg-shape");
            el.style.removeProperty("--pg-fh");
          }
          rec.sig = sig;
        }
        const flags = (mt && sig ? "m b" : mt ? "m" : sig ? "b" : "");
        if (flags !== rec.flags) {
          if (flags) el.setAttribute("data-eo-pg", flags);
          else el.removeAttribute("data-eo-pg");
          rec.flags = flags;
        }
      }
    }

    // Heights of the split paragraphs as the browser laid them out; a block whose height differs from the plan
    // keeps the measured value for the same bands, and the plan is made again.
    function verify(list, plan, scale) {
      let again = false;
      for (let i = 0; i < list.length; i += 1) {
        if (!plan.bands[i]) continue;
        const actual = list[i].getBoundingClientRect().height / scale;
        if (Math.abs(actual - plan.height[i]) <= VERIFY_TOLERANCE_PX) continue;
        const rec = recs.get(list[i]);
        rec.hAct = actual;
        rec.actSig = plan.sig[i];
        again = true;
      }
      return again;
    }

    // ---- the pass ---------------------------------------------------------------------------

    function isShown() {
      return editor.isConnected && editor.getClientRects().length > 0;
    }

    function setActive(on) {
      if (on === active) return;
      active = on;
      editor.classList.toggle(PAGED_CLASS, on);
      if (!on) {
        for (const el of units) {
          const rec = recs.get(el);
          if (rec) clearApplied(el, rec);
        }
        layer?.remove();
        layer = null;
        units = [];
        result = null;
      }
      allDirty = true;
    }

    function run() {
      scheduled = false;
      if (!editor || !isShown()) return;
      setActive(!editor.classList.contains("manuscript-grid"));
      if (!active) return;
      const g = readGeometry();
      if (g.changed || !geo) {
        geo = g.next;
        geoKey = g.key;
        applyGeometry(geo);
        allDirty = true;
      }
      if (allDirty) styleCache.clear();
      const list = collectUnits();
      const need = list.filter((el) => allDirty || dirty.has(el) || !recs.has(el));
      const structure = list.length !== units.length || list.some((el, i) => el !== units[i]);
      units = list;
      let changed = structure;
      if (need.length === list.length && need.length > 64) {
        measureAll(list);
        changed = true;
      } else if (need.length > 0) {
        changed = measure(list, need) || changed;
      }
      allDirty = false;
      dirty.clear();
      if (!changed && result && !needsWrites(list, result)) return finish();
      plan(list);
    }

    // Whether the measure cleared page styling that the current plan still wants on a block.
    function needsWrites(list, plan) {
      for (let i = 0; i < list.length; i += 1) {
        const rec = recs.get(list[i]);
        if (rec.mt !== marginFor(rec, plan, i) || rec.sig !== plan.sig[i]) return true;
      }
      return false;
    }

    function plan(list) {
      const recList = list.map((el) => recs.get(el));
      for (let i = 0; i < recList.length; i += 1) {
        recList[i].keepMin = recList[i + 1]?.lh || 0;
        recList[i].index = i;
      }
      const ed = editor.getBoundingClientRect();
      const scale = editor.offsetWidth > 0 ? ed.width / editor.offsetWidth : 1;
      let out = null;
      for (let round = 0; round < MAX_VERIFY_ROUNDS; round += 1) {
        out = core.paginate(recList, geo);
        applyPlan(list, out);
        if (!verify(list, out, scale)) break;
      }
      result = out;
      finish();
    }

    function finish() {
      const count = result ? result.pageCount : 1;
      syncSheets(count, false);
      // The writes of this pass (styles, attributes, sheets) are not changes to measure.
      observer?.takeRecords();
      // What the listeners write (the header and footer's room, `--pg-hf-top`) is: the next pass sees it.
      for (const fn of sheetListeners) fn(layer, count);
      const countChanged = count !== lastCount;
      lastCount = count;
      const current = computeCurrentPage();
      if (countChanged || current !== lastCurrent) {
        lastCurrent = current;
        emit();
      }
    }

    function emit() {
      const state = { pageCount: lastCount, currentPage: lastCurrent };
      for (const fn of listeners) fn(state);
    }

    function schedule() {
      if (scheduled || !editor) return;
      scheduled = true;
      raf(run);
    }

    // ---- change tracking --------------------------------------------------------------------

    function onMutations(records) {
      for (const record of records) {
        const target = record.target;
        if (target === editor) {
          if (record.type === "attributes") allDirty = true;
          else markNext(record);
          continue;
        }
        if (layer && layer.contains(target)) continue;
        const el = unitOf(target);
        if (el) dirty.add(el);
        else markNext(record);
      }
      schedule();
    }

    // A block added or removed changes the gap of the one after it.
    function markNext(record) {
      if (record.nextSibling?.nodeType === 1) dirty.add(record.nextSibling);
    }

    function queueSelection() {
      if (selectionQueued || !editor) return;
      selectionQueued = true;
      raf(() => {
        selectionQueued = false;
        if (!result) return;
        const current = computeCurrentPage();
        if (current === lastCurrent) return;
        lastCurrent = current;
        emit();
      });
    }

    // ---- API --------------------------------------------------------------------------------

    function pageIndexAtY(y) {
      return Math.max(0, Math.min(lastCount - 1, Math.floor((y + core.EPS) / geo.pitch)));
    }

    function caretPage(rec) {
      if (!rec.sig) return result.page[rec.index] + 1;
      const sel = env.getSelection?.();
      const range = sel?.rangeCount ? sel.getRangeAt(0).cloneRange() : null;
      range?.collapse(false);
      const rect = range?.getClientRects()[0] || range?.getBoundingClientRect();
      if (!rect || (rect.top === 0 && rect.height === 0)) return result.page[rec.index] + 1;
      const ed = editor.getBoundingClientRect();
      const scale = editor.offsetWidth > 0 ? ed.width / editor.offsetWidth : 1;
      return pageIndexAtY((rect.top - ed.top) / scale) + 1;
    }

    function computeCurrentPage() {
      if (!result || !editor) return lastCurrent;
      const sel = env.getSelection?.();
      const node = sel?.focusNode;
      if (!node || !editor.contains(node)) return lastCurrent;
      const el = unitOf(node);
      if (!el) return lastCurrent;
      return caretPage(recs.get(el));
    }

    function pageOf(block) {
      if (!result) return 1;
      const rec = recs.get(unitOf(block));
      return rec ? result.page[rec.index] + 1 : 1;
    }

    function headingPages() {
      const out = [];
      if (!result) return out;
      units.forEach((el, i) => {
        if (HEADING_RE.test(el.tagName)) out.push({ el, level: Number(el.tagName[1]), page: result.page[i] + 1 });
      });
      return out;
    }

    function blockPages() {
      return result ? units.map((el, i) => ({ el, page: result.page[i] + 1, endPage: result.endPage[i] + 1 })) : [];
    }

    // ---- forced page breaks -----------------------------------------------------------------

    function caretBlock() {
      const node = env.getSelection?.()?.focusNode;
      return node && editor.contains(node) ? unitOf(node) || topBlockOf(node) : null;
    }

    function topBlockOf(node) {
      let el = node?.nodeType === 1 ? node : node?.parentElement;
      while (el && el.parentElement !== editor) el = el.parentElement;
      return el;
    }

    // The browser copies a block's attributes to the half it splits off; the break stays on the first half.
    function onInput(ev) {
      if (ev.inputType !== "insertParagraph") return;
      const block = caretBlock();
      if (block?.dataset.eoBreak && block.previousElementSibling?.dataset.eoBreak) delete block.dataset.eoBreak;
    }

    // Splits the block at the caret and starts the page with the second half. The split is done on
    // the range, not by `execCommand`, so it is one undo step and fires no `input` event of its own.
    function splitAtCaret() {
      const sel = env.getSelection();
      const block = caretBlock();
      if (!sel?.rangeCount || !block || !TEXT_BLOCK_RE.test(block.tagName)) return { changed: false };
      const range = sel.getRangeAt(0);
      range.deleteContents();
      const tail = document.createRange();
      tail.setStart(range.startContainer, range.startOffset);
      tail.setEnd(block, block.childNodes.length);
      const next = block.cloneNode(false);
      next.removeAttribute("id");
      next.append(tail.extractContents());
      for (const part of [block, next]) if (!part.hasChildNodes()) part.append(document.createElement("br"));
      next.dataset.eoBreak = "page";
      block.after(next);
      const caret = document.createRange();
      caret.setStart(next, 0);
      caret.collapse(true);
      sel.removeAllRanges();
      sel.addRange(caret);
      return { changed: true };
    }

    function insertPageBreak() {
      if (!editor || !caretBlock()) return false;
      const res = deps.withUndo ? deps.withUndo(splitAtCaret) : splitAtCaret();
      schedule();
      return res.changed;
    }

    function onKeydown(ev) {
      if (ev.key !== "Enter" || !(ev.ctrlKey || ev.metaKey) || ev.shiftKey || ev.altKey) return;
      ev.preventDefault();
      insertPageBreak();
    }

    function listen(target, type, fn, options) {
      target.addEventListener(type, fn, options);
      bound.push(() => target.removeEventListener(type, fn, options));
    }

    function attach(root) {
      detach();
      editor = root;
      allDirty = true;
      const Observer = env.MutationObserver;
      if (typeof Observer === "function") {
        observer = new Observer(onMutations);
        observer.observe(editor, { childList: true, characterData: true, subtree: true, attributes: true, attributeFilter: WATCHED_ATTRS });
      }
      if (typeof env.ResizeObserver === "function") {
        sizeObserver = new env.ResizeObserver(() => {
          if (!active || !result) schedule();
        });
        sizeObserver.observe(editor);
      }
      listen(editor, "load", (ev) => {
        const el = unitOf(ev.target);
        if (el) dirty.add(el);
        schedule();
      }, true);
      listen(document, "selectionchange", queueSelection);
      listen(editor, "input", onInput);
      listen(editor, "keydown", onKeydown);
      document.fonts?.addEventListener?.("loadingdone", () => {
        allDirty = true;
        schedule();
      });
      schedule();
    }

    function detach() {
      observer?.disconnect();
      sizeObserver?.disconnect();
      observer = sizeObserver = null;
      for (const off of bound.splice(0)) off();
      if (editor) setActive(false);
      editor = null;
    }

    // Runs the pass now (tests, the benchmark and callers that need current numbers).
    function layoutNow() {
      if (observer) onMutations(observer.takeRecords());
      scheduled = false;
      run();
      return { pageCount: lastCount, currentPage: lastCurrent };
    }

    const api = {
      attach, detach, layoutNow, schedule,
      pageCount: () => lastCount,
      currentPage: () => lastCurrent,
      pageOf, headingPages, blockPages, insertPageBreak,
      geometry: () => geo,
      isActive: () => active,
      onChange(fn) {
        listeners.add(fn);
        return () => listeners.delete(fn);
      },
      // Called after the sheets of the page layer were made or removed.
      onSheets(fn) {
        sheetListeners.add(fn);
        return () => sheetListeners.delete(fn);
      },
    };
    host.__EASYOFFICE_DOC_PAGES__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_DOC_PAGES__ = createDocPages;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
