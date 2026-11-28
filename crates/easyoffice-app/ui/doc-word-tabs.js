(function (host) {
  "use strict";

  // 서식 > 탭…: the tab stops of a paragraph (PD-401) and how the editor draws them. Each tab character of a paragraph that has
  // stops sits in its own `span.eo-tab` whose width the stop gives. The rule is `updateTabStop` of the Apache-2.0 DOCX renderer
  // docx-preview (`src/javascript.ts` lines 24-93, commit 28682e30): the stops are sorted, default stops fill the width past the last one,
  // the tab takes the first stop to the right of where it begins, and a right or center stop leaves room for the text up to the
  // next tab (the decimal stop for the text up to the decimal point). docx-preview widens a non-breaking space with
  // `word-spacing`; here the span is an inline box of the width itself and the leader is a background pattern. The text of the
  // paragraph is unchanged by the spans (they hold the tab character), so the save and the undo see the same text.
  // Reuse (구현 원칙 3단계): (1) no library does this on an editable DOM; (2) the dialog kit, the undo wrapper and the model
  // (`doc-word-blocks.js`) are used; (3) the width rule is the one port, with the caret kept through the span changes.

  const TWIPS_PER_PX = 15;
  const TWIPS_PER_MM = 1440 / 25.4;
  const DEFAULT_TAB_TWIPS = 720;
  const TAB_CLASS = "eo-tab";
  const MAX_PARAGRAPHS = 2000;
  // Most tab characters of one paragraph that are measured (a paragraph of more keeps the plain tab for the rest).
  const MAX_TABS_PER_PARAGRAPH = 500;
  const ALIGN_NAMES = { left: "왼쪽", center: "가운데", right: "오른쪽", decimal: "10진수" };
  const blocks = () => host.__EASYOFFICE_DOC_WORD_BLOCKS__;
  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  const isOpaque = (node) => node.nodeType === 1 && (node.isContentEditable === false || node.classList.contains("eo-textbox") || node.classList.contains("eo-math"));

  // The text nodes of a paragraph that are its own (not inside a text box, an equation or a field chip).
  function ownTextNodes(p) {
    const out = [];
    const walk = (node) => {
      for (const child of Array.from(node.childNodes)) {
        if (child.nodeType === 3) out.push(child);
        else if (child.nodeType === 1 && !isOpaque(child)) walk(child);
      }
    };
    walk(p);
    return out;
  }

  // Every tab character in a span of its own, and every span back to one tab character.
  function wrapTabs(doc, p) {
    for (const span of Array.from(p.querySelectorAll("." + TAB_CLASS))) {
      const text = span.textContent;
      if (text === "\t") continue;
      const rest = text.replace("\t", "");
      if (text.includes("\t")) {
        span.textContent = "\t";
        if (rest) span.parentNode.insertBefore(doc.createTextNode(rest), span.nextSibling);
      } else {
        span.parentNode.insertBefore(doc.createTextNode(text), span);
        span.parentNode.removeChild(span);
      }
    }
    let wrapped = p.querySelectorAll("." + TAB_CLASS).length;
    for (const node of ownTextNodes(p)) {
      if (node.parentNode?.classList?.contains(TAB_CLASS) || !node.nodeValue.includes("\t")) continue;
      let current = node;
      for (let at = current.nodeValue.indexOf("\t"); at >= 0; at = current.nodeValue.indexOf("\t")) {
        // The tabs past the bound stay characters of the text.
        if (wrapped >= MAX_TABS_PER_PARAGRAPH) return;
        wrapped += 1;
        if (at > 0) current = current.splitText(at);
        const after = current.nodeValue.length > 1 ? current.splitText(1) : null;
        const span = doc.createElement("span");
        span.className = TAB_CLASS;
        current.parentNode.insertBefore(span, current);
        span.appendChild(current);
        if (!after) break;
        current = after;
      }
    }
  }

  // The stops of a paragraph in pixels from the start of its text, the default ones filling the width past the last.
  function stopsOf(p, widthPx, defaultPx) {
    const stops = blocks().decodeTabs(p.dataset.eoTabs).map((t) => ({ pos: t.pos / TWIPS_PER_PX, align: t.align, leader: t.leader }));
    let pos = (stops.at(-1)?.pos ?? 0) + defaultPx;
    for (let i = 0; pos < widthPx && i < 50; i += 1, pos += defaultPx) stops.push({ pos, align: "left", leader: "none" });
    return stops;
  }

  const widthOfText = (doc, from, to, p, decimal) => {
    const range = doc.createRange();
    range.setStartAfter(from);
    if (to) range.setEndBefore(to);
    else range.setEnd(p, p.childNodes.length);
    if (!decimal) return range.getBoundingClientRect().width;
    const text = range.toString();
    const at = text.indexOf(".");
    if (at < 0) return range.getBoundingClientRect().width;
    // The text up to the decimal point: the range cut after the point's character.
    const cut = doc.createRange();
    cut.setStart(range.startContainer, range.startOffset);
    let remaining = at;
    for (const node of ownTextNodes(p)) {
      if (!range.intersectsNode(node)) continue;
      const len = node.nodeValue.length;
      if (remaining <= len) {
        cut.setEnd(node, remaining);
        return cut.getBoundingClientRect().width;
      }
      remaining -= len;
    }
    return range.getBoundingClientRect().width;
  };

  function layoutTabs(doc, p, defaultTwips) {
    const all = Array.from(p.querySelectorAll("." + TAB_CLASS));
    if (all.length === 0 || typeof host.getComputedStyle !== "function") return;
    // The tabs past the bound keep the plain tab of the editor.
    const spans = all.slice(0, MAX_TABS_PER_PARAGRAPH);
    for (const span of all.slice(MAX_TABS_PER_PARAGRAPH)) {
      span.style.width = "";
      delete span.dataset.eoLeader;
    }
    // One write (every tab at width 0), one read (where each tab begins and how wide the text after it is), then the widths
    // are worked out from the numbers: a tab moves everything after it by its own width, so the place a tab begins at is where
    // it began at width 0 plus the widths before it. This is one layout for the paragraph, not one per tab; a line that wraps
    // is not followed (the stops are measured from the start of the text, as the renderer's rule does).
    for (const span of spans) span.style.width = "0px";
    const cs = host.getComputedStyle(p);
    const box = p.getBoundingClientRect();
    const widthPx = box.width - parseFloat(cs.paddingRight || "0");
    const leftEdge = box.left + parseFloat(cs.paddingLeft || "0") + parseFloat(cs.borderLeftWidth || "0");
    const stops = stopsOf(p, widthPx, Math.max(8, defaultTwips / TWIPS_PER_PX));
    const measured = spans.map((span, i) => ({
      left: span.getBoundingClientRect().left - leftEdge,
      text: widthOfText(doc, span, spans[i + 1] ?? null, p, false),
      decimal: widthOfText(doc, span, spans[i + 1] ?? null, p, true),
    }));
    let shift = 0;
    const widths = measured.map((m) => {
      const left = m.left + shift;
      const stop = stops.find((s) => s.pos > left + 0.5);
      const take = stop?.align === "right" ? m.text : stop?.align === "center" ? m.text / 2 : stop?.align === "decimal" ? m.decimal : 0;
      const width = stop ? Math.max(1, stop.pos - left - take) : 1;
      shift += width;
      return { width, leader: stop?.leader ?? "none" };
    });
    spans.forEach((span, i) => {
      span.style.width = widths[i].width.toFixed(1) + "px";
      span.dataset.eoLeader = widths[i].leader;
    });
  }

  // The selection as the text nodes and offsets it holds: kept through the changes of the spans.
  function keepSelection(doc) {
    const sel = host.getSelection?.();
    if (!sel || sel.rangeCount === 0) return () => {};
    const r = sel.getRangeAt(0);
    const saved = { a: r.startContainer, ao: r.startOffset, b: r.endContainer, bo: r.endOffset };
    return () => {
      if (!saved.a.isConnected || !saved.b.isConnected) return;
      const inTab = (n) => n.nodeType === 3 && n.parentNode?.classList?.contains(TAB_CLASS) && saved.ao > 0;
      let { a, ao, b, bo } = saved;
      // A caret after a tab character stays out of its span, where typing would go into the span.
      if (inTab(a) && a === b && ao === bo) [a, ao, b, bo] = [a.parentNode.parentNode, indexIn(a.parentNode) + 1, a.parentNode.parentNode, indexIn(a.parentNode) + 1];
      try {
        const range = doc.createRange();
        range.setStart(a, ao);
        range.setEnd(b, bo);
        sel.removeAllRanges();
        sel.addRange(range);
      } catch {
        // A node that is gone leaves the selection where the browser put it.
      }
    };
  }
  const indexIn = (node) => Array.prototype.indexOf.call(node.parentNode.childNodes, node);

  const defaultTwips = (editor) => {
    try {
      const n = JSON.parse(editor.dataset.eoDefaults || "{}").tabStopTwips;
      return Number.isInteger(n) && n > 0 ? n : DEFAULT_TAB_TWIPS;
    } catch {
      return DEFAULT_TAB_TWIPS;
    }
  };

  // Draws the tab stops of `paragraphs` (all the paragraphs of the editor that have some when none is given).
  function refresh(editor, paragraphs) {
    const doc = editor.ownerDocument;
    const list = (paragraphs ?? Array.from(editor.querySelectorAll("[data-eo-tabs]"))).slice(0, MAX_PARAGRAPHS);
    const restore = keepSelection(doc);
    for (const p of list) {
      if (p.dataset.eoTabs === undefined) {
        for (const span of Array.from(p.querySelectorAll("." + TAB_CLASS))) {
          span.parentNode.insertBefore(doc.createTextNode("\t"), span);
          span.parentNode.removeChild(span);
        }
        p.normalize();
        continue;
      }
      wrapTabs(doc, p);
      layoutTabs(doc, p, defaultTwips(editor));
    }
    restore();
  }

  // ---- the dialog

  function install(env0) {
    const document = env0.document;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const byId = (id) => document.getElementById(id);
    let env = env0;
    let stops = [];
    let targets = [];

    const label = (s) => (s.pos / TWIPS_PER_MM).toFixed(1) + "mm (" + ALIGN_NAMES[s.align] + (s.leader !== "none" ? ", 채움 " + (s.leader === "dot" ? "·" : s.leader === "hyphen" ? "-" : "_") : "") + ")";
    const showList = () =>
      byId("doc-tab-list")?.replaceChildren(
        ...stops.map((s) => {
          const option = document.createElement("option");
          option.value = String(s.pos);
          option.textContent = label(s);
          return option;
        }),
      );
    const radio = (name) => Array.from(document.querySelectorAll("input[name=\"" + name + "\"]")).find((input) => input.checked)?.value;
    const setRadio = (name, value) => {
      for (const input of document.querySelectorAll("input[name=\"" + name + "\"]")) input.checked = input.value === value;
    };
    const load = (pos) => {
      const s = stops.find((t) => t.pos === pos);
      if (!s) return;
      kit.setVal("doc-tab-pos", (s.pos / TWIPS_PER_MM).toFixed(1));
      setRadio("doc-tab-align", s.align);
      setRadio("doc-tab-leader", s.leader);
    };
    const setStop = () => {
      const pos = Math.round(Number(kit.val("doc-tab-pos")) * TWIPS_PER_MM);
      if (!(pos >= 0 && pos <= 31680)) return kit.note("doc-tab-status", "탭 정지 위치는 0에서 558mm 사이여야 합니다");
      if (stops.length >= 64 && !stops.some((s) => s.pos === pos)) return kit.note("doc-tab-status", "탭 정지는 문단에 64개까지 둘 수 있습니다");
      kit.note("doc-tab-status", "");
      const next = { pos, align: radio("doc-tab-align") ?? "left", leader: radio("doc-tab-leader") ?? "none" };
      stops = blocks().decodeTabs(blocks().encodeTabs([...stops.filter((s) => s.pos !== pos), next]));
      showList();
    };
    const apply = () => {
      const value = stops.length > 0 ? blocks().encodeTabs(stops) : "";
      env.runs.withUndo(() => {
        for (const p of targets) {
          if (value) p.dataset.eoTabs = value;
          else delete p.dataset.eoTabs;
        }
        refresh(env.editor, targets);
        return { changed: true };
      });
      kit.closeDialog("doc-tabs-dialog");
      feedback(env, stops.length > 0 ? "탭 정지 적용 완료" : "탭 정지를 지웠습니다");
    };
    byId("btn-doc-tab-set")?.addEventListener("click", setStop);
    byId("btn-doc-tab-clear")?.addEventListener("click", () => {
      const pos = Math.round(Number(kit.val("doc-tab-pos")) * TWIPS_PER_MM);
      stops = stops.filter((s) => s.pos !== pos);
      showList();
    });
    byId("btn-doc-tab-clear-all")?.addEventListener("click", () => {
      stops = [];
      showList();
    });
    byId("doc-tab-list")?.addEventListener("change", (e) => load(Number(e.target.value)));
    byId("btn-doc-tab-ok")?.addEventListener("click", apply);

    host.__EASYOFFICE_DOC_WORD_COMMANDS__.register("para-tabs", (e) => {
      env = e;
      const range = e.runs.currentRange();
      targets = range ? host.__EASYOFFICE_DOC_RUNS__.blocksInRange(e.editor, range).filter((b) => /^(P|H[1-6])$/.test(b.tagName)) : [];
      if (targets.length === 0) return feedback(e, "탭 정지를 정할 문단 안에 커서를 놓으세요");
      stops = blocks().decodeTabs(targets[0].dataset.eoTabs);
      kit.setVal("doc-tab-pos", "");
      kit.setVal("doc-tab-default", (defaultTwips(e.editor) / TWIPS_PER_MM).toFixed(1));
      setRadio("doc-tab-align", "left");
      setRadio("doc-tab-leader", "none");
      kit.note("doc-tab-status", "");
      showList();
      kit.openDialog("doc-tabs-dialog");
    });
  }

  // The tab stops are drawn after an open, an undo and a typed character; the editor root is watched once it exists.
  function watch(env) {
    const editor = env.editor;
    if (!editor || editor.dataset.eoTabsWatch === "1" || typeof host.MutationObserver !== "function") return;
    editor.dataset.eoTabsWatch = "1";
    let queued = false;
    const run = () => {
      queued = false;
      observer.disconnect();
      refresh(editor);
      observer.observe(editor, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-eo-tabs"] });
    };
    const schedule = () => {
      if (queued) return;
      queued = true;
      (host.requestAnimationFrame ?? ((fn) => host.setTimeout(fn, 16)))(run);
    };
    const observer = new host.MutationObserver((records) => {
      if (records.some((r) => !(r.target.nodeType === 1 && r.target.classList?.contains(TAB_CLASS)))) schedule();
    });
    observer.observe(editor, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-eo-tabs"] });
    editor.addEventListener("input", (e) => {
      const p = (e.target.nodeType === 1 ? e.target : e.target.parentNode)?.closest?.("[data-eo-tabs]");
      if (p) schedule();
    });
    schedule();
  }

  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  let installed = false;
  commands?.register("para-tabs", (env) => {
    if (!installed) {
      installed = true;
      install(env);
    }
    commands.run("para-tabs", env);
  });
  host.__EASYOFFICE_DOC_WORD_TABS__ = { refresh, watch, wrapTabs, stopsOf };
  const start = () => {
    const editor = host.document?.getElementById?.("doc-editor");
    if (editor) watch({ editor });
  };
  if (host.document?.readyState === "loading") host.document.addEventListener("DOMContentLoaded", start);
  else start();
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
