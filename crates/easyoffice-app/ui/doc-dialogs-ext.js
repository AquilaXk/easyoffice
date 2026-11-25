(function (host) {
  "use strict";

  const MAIL_MERGE_TOKEN_RE = /\{\{([^}]+)\}\}/g;
  const MAIL_MERGE_TOKEN_TEST_RE = /\{\{([^}]+)\}\}/;
  const DEFAULT_MERGE_FIELDS = ["성명", "기관명", "일자"];
  const SHAPE_KINDS = new Set(["roundRect", "rect", "callout", "ellipse"]);
  const SVG_NS = "http://www.w3.org/2000/svg";
  const DEFAULT_CHART_COLORS = ["2563EB", "DC2626", "16A34A", "D97706"];

  function collectMergeTokens(docEditor) {
    const seen = new Set();
    const tokens = [];
    const text = String(docEditor?.textContent || "");
    for (const m of text.matchAll(MAIL_MERGE_TOKEN_RE)) {
      const key = m[1].trim();
      if (key && !seen.has(key)) {
        seen.add(key);
        tokens.push(key);
      }
    }
    return tokens;
  }

  function replaceTokensInText(text, valueMap) {
    return String(text || "").replace(MAIL_MERGE_TOKEN_RE, (full, rawKey) => {
      const key = String(rawKey || "").trim();
      return Object.hasOwn(valueMap, key) ? valueMap[key] : full;
    });
  }

  function mergeTokensInDom(docEditor, valueMap) {
    let replaced = 0;
    const walk = (node) => {
      if (!node) return;
      if (node.nodeType === 3) {
        const val = node.nodeValue || "";
        if (!val.includes("{{")) return;
        const nextVal = replaceTokensInText(val, valueMap);
        if (nextVal !== val) {
          node.nodeValue = nextVal;
          replaced += 1;
        }
        return;
      }
      Array.from(node.childNodes || []).forEach(walk);
    };
    walk(docEditor);
    docEditor
      ?.querySelectorAll?.("#doc-editor p, #doc-editor li, #doc-editor td, #doc-editor th, #doc-editor div")
      .forEach((el) => {
        if (el.childElementCount > 0 || el.children?.length > 0) return;
        const raw = el.textContent || "";
        if (!MAIL_MERGE_TOKEN_TEST_RE.test(raw)) return;
        const next = replaceTokensInText(raw, valueMap);
        if (next !== raw) {
          el.textContent = next;
          replaced += 1;
        }
      });
    return replaced;
  }

  function createDocDialogsExt(deps) {
    const {
      document,
      getDocRuns,
      getDocTable,
      insertTextAtSelection,
      withDocStyleUndo,
      computeSmartStats,
      renderModuleOutline,
      getActiveModule,
      openToolDialog,
      closeDialog,
      setFeedback,
    } = deps;

    const $ = (id) => document.getElementById(id);
    let pendingMailMergeRun = null;
    let bound = false;

    function addMergeFieldRow(name, val) {
      const list = $("doc-mailmerge-rows");
      if (!list) return;
      const row = document.createElement("div");
      row.className = "dialog-row doc-mailmerge-row";
      const keyInput = document.createElement("input");
      keyInput.type = "text";
      keyInput.className = "dialog-input doc-mm-key";
      keyInput.value = name;
      keyInput.setAttribute("aria-label", "필드 이름");
      const valInput = document.createElement("input");
      valInput.type = "text";
      valInput.className = "dialog-text-input doc-mm-val";
      valInput.value = val;
      valInput.setAttribute("aria-label", "필드 값");
      row.appendChild(keyInput);
      row.appendChild(valInput);
      list.appendChild(row);
      return { keyInput, valInput };
    }

    function openMailMergeDialog(onSubmit) {
      bind();
      pendingMailMergeRun = onSubmit;
      const editor = $("doc-editor");
      const list = $("doc-mailmerge-rows");
      if (!list) return;
      list.replaceChildren();
      const tokens = collectMergeTokens(editor);
      const keys = tokens.length > 0 ? tokens : DEFAULT_MERGE_FIELDS;
      const rows = keys.map((k) => addMergeFieldRow(k, ""));
      const st = $("doc-mailmerge-status");
      if (st) st.textContent = tokens.length > 0 ? "문서 필드 " + tokens.length + "개" : "";
      openToolDialog("doc-mailmerge-dialog");
      rows[0]?.valInput?.focus?.();
    }

    function readMergeMapFromDialog() {
      const list = $("doc-mailmerge-rows");
      const map = Object.create(null);
      if (!list) return map;
      for (const row of list.querySelectorAll(".doc-mailmerge-row")) {
        const key = String(row.querySelector(".doc-mm-key")?.value || "").trim();
        const val = String(row.querySelector(".doc-mm-val")?.value ?? "");
        if (key && val !== "") map[key] = val;
      }
      return map;
    }

    function applyMailMergeDialog() {
      const values = readMergeMapFromDialog();
      const st = $("doc-mailmerge-status");
      if (Object.keys(values).length === 0) {
        if (st) st.textContent = "치환할 필드 값이 없습니다.";
        return;
      }
      const run = pendingMailMergeRun;
      pendingMailMergeRun = null;
      closeDialog?.("doc-mailmerge-dialog");
      run?.(values);
    }

    function insertMergeTokenField() {
      const firstKey = String($("doc-mailmerge-rows")?.querySelector(".doc-mm-key")?.value || "").trim() || "필드";
      const token = "{{" + firstKey + "}}";
      closeDialog?.("doc-mailmerge-dialog");
      withDocStyleUndo(() => {
        if (!insertTextAtSelection(token)) {
          const editor = $("doc-editor");
          const p = document.createElement("p");
          p.textContent = token;
          editor?.appendChild(p);
        }
        computeSmartStats();
      });
      setFeedback?.("병합 필드 삽입: " + token);
    }

    function buildShapeSvg(kind, wMm, hMm, fillHex, strokeHex) {
      const svg = document.createElementNS(SVG_NS, "svg");
      svg.setAttribute("viewBox", "0 0 240 100");
      svg.setAttribute("aria-hidden", "true");
      svg.classList.add("eo-doc-shape-svg");
      svg.style.width = Math.max(24, Math.min(180, wMm)) + "mm";
      svg.style.height = Math.max(12, Math.min(120, hMm)) + "mm";
      let shapeEl;
      if (kind === "ellipse") {
        shapeEl = document.createElementNS(SVG_NS, "ellipse");
        shapeEl.setAttribute("cx", "120");
        shapeEl.setAttribute("cy", "50");
        shapeEl.setAttribute("rx", "114");
        shapeEl.setAttribute("ry", "44");
      } else if (kind === "callout") {
        shapeEl = document.createElementNS(SVG_NS, "path");
        shapeEl.setAttribute("d", "M 8 6 H 232 V 76 H 96 L 68 96 V 76 H 8 Z");
      } else {
        shapeEl = document.createElementNS(SVG_NS, "rect");
        shapeEl.setAttribute("x", "6");
        shapeEl.setAttribute("y", "6");
        shapeEl.setAttribute("width", "228");
        shapeEl.setAttribute("height", "88");
        if (kind === "roundRect") shapeEl.setAttribute("rx", "14");
      }
      shapeEl.setAttribute("fill", fillHex);
      shapeEl.setAttribute("stroke", strokeHex);
      shapeEl.setAttribute("stroke-width", "3");
      svg.appendChild(shapeEl);
      return svg;
    }

    function insertBlockAfterCaret(editor, node) {
      const range = getDocRuns?.()?.currentRange?.();
      let block = range?.startContainer || null;
      while (block && block.parentNode !== editor) block = block.parentNode;
      if (block && block.parentNode === editor) {
        editor.insertBefore(node, block.nextSibling);
        return;
      }
      const tail = Array.from(editor.children || []).find((el) => {
        const tag = String(el.tagName || "").toUpperCase();
        const ds = el.dataset;
        return tag === "FOOTER" || el.id === "hwp-manuscript-preview" || Boolean(ds && (ds.eoHf !== undefined || ds.eoNotes !== undefined || ds.eoComments !== undefined));
      });
      if (tail) editor.insertBefore(node, tail);
      else editor.appendChild(node);
    }

    function openShapeDialog() {
      bind();
      const st = $("doc-shape-status");
      if (st) st.textContent = "";
      openToolDialog("doc-shape-dialog");
      $("doc-shape-text")?.focus?.();
    }

    function applyShapeDialog() {
      const editor = $("doc-editor");
      if (!editor) return;
      const kindRaw = $("doc-shape-type")?.value || "roundRect";
      const kind = SHAPE_KINDS.has(kindRaw) ? kindRaw : "roundRect";
      const text = String($("doc-shape-text")?.value || "").trim() || "도형";
      const fillHex = $("doc-shape-fill")?.value || "#dbeafe";
      const strokeHex = $("doc-shape-stroke")?.value || "#2563eb";
      const wMm = Number($("doc-shape-w")?.value) || 64;
      const hMm = Number($("doc-shape-h")?.value) || 28;
      closeDialog?.("doc-shape-dialog");
      withDocStyleUndo(() => {
        const fig = document.createElement("figure");
        fig.className = "eo-doc-shape";
        fig.dataset.eoShape = kind;
        fig.appendChild(buildShapeSvg(kind, wMm, hMm, fillHex, strokeHex));
        const cap = document.createElement("p");
        cap.className = "eo-doc-shape-text";
        cap.setAttribute("contenteditable", "true");
        cap.textContent = text;
        fig.appendChild(cap);
        insertBlockAfterCaret(editor, fig);
        computeSmartStats();
        renderModuleOutline(getActiveModule());
      });
      setFeedback?.("도형 삽입 완료");
    }

    function parseChartInputs() {
      const rawCats = $("doc-chart-categories")?.value || $("doc-chart-categories")?.textContent || "";
      const categories = String(rawCats)
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      const rawSeries = $("doc-chart-series")?.value || $("doc-chart-series")?.textContent || "";
      const series = [];
      for (const line of String(rawSeries).split("\n")) {
        const idx = line.indexOf(":");
        if (idx < 0) continue;
        const name = line.slice(0, idx).trim();
        const values = line.slice(idx + 1).split(",").map((v) => Number(v.trim())).filter(Number.isFinite);
        if (name && values.length > 0) series.push({ name, values });
      }
      return { categories, series };
    }

    function openChartDialog() {
      bind();
      const seriesEl = $("doc-chart-series");
      if (seriesEl && !seriesEl.value) seriesEl.value = seriesEl.textContent || "실적: 40, 65, 90";
      const st = $("doc-chart-status");
      if (st) st.textContent = "";
      openToolDialog("doc-chart-dialog");
      $("doc-chart-title-input")?.focus?.();
    }

    function applyChartDialog() {
      const editor = $("doc-editor");
      if (!editor) return;
      const chartType = $("doc-chart-type")?.value || "column";
      const title = String($("doc-chart-title-input")?.value || "").trim() || "차트";
      const { categories, series } = parseChartInputs();
      const st = $("doc-chart-status");
      if (categories.length === 0 || series.length === 0) {
        if (st) st.textContent = "항목과 계열 값이 비어 있습니다.";
        return;
      }
      closeDialog?.("doc-chart-dialog");
      withDocStyleUndo(() => {
        const fig = document.createElement("figure");
        fig.className = "eo-doc-chart";
        fig.dataset.eoChart = chartType;
        const cap = document.createElement("figcaption");
        const heading = document.createElement("p");
        heading.className = "eo-doc-chart-title";
        heading.textContent = title;
        cap.appendChild(heading);
        fig.appendChild(cap);

        const svgApi = host.__EASYOFFICE_SHEET_CHART_SVG__;
        const utils = host.__EASYOFFICE_SHEET_UTILS__;
        if (svgApi?.renderChartSvg && utils?.colIndexToLetters) {
          const map = {};
          map.A1 = { value: "항목", t: "s" };
          categories.forEach((cat, r) => { map["A" + (r + 2)] = { value: cat, t: "s" }; });
          series.forEach((ser, sIdx) => {
            const col = utils.colIndexToLetters(sIdx + 1);
            map[col + "1"] = { value: ser.name, t: "s" };
            categories.forEach((_, r) => {
              const num = Number.isFinite(ser.values[r]) ? ser.values[r] : 0;
              map[col + (r + 2)] = { value: String(num) };
            });
          });
          const svg = svgApi.renderChartSvg({
            document, sheetName: "DocChart", sheetDataStore: { DocChart: map },
            chart: { chartType, title, legend: "right", dataRange: "A1:" + utils.colIndexToLetters(series.length) + (categories.length + 1) },
            width: 460, height: 240,
          });
          if (svg) fig.appendChild(svg);
        }

        const table = document.createElement("table");
        table.className = "doc-inline-table eo-doc-chart-data";
        const tbody = document.createElement("tbody");
        const hdr = document.createElement("tr");
        const th0 = document.createElement("td");
        th0.textContent = "항목";
        hdr.appendChild(th0);
        for (const s of series) {
          const td = document.createElement("td");
          td.textContent = s.name;
          hdr.appendChild(td);
        }
        tbody.appendChild(hdr);
        categories.forEach((cat, rIdx) => {
          const tr = document.createElement("tr");
          const c0 = document.createElement("td");
          c0.textContent = cat;
          tr.appendChild(c0);
          for (const s of series) {
            const td = document.createElement("td");
            td.textContent = String(Number.isFinite(s.values[rIdx]) ? s.values[rIdx] : 0);
            tr.appendChild(td);
          }
          tbody.appendChild(tr);
        });
        table.appendChild(tbody);
        fig.appendChild(table);
        insertBlockAfterCaret(editor, fig);
        computeSmartStats();
        renderModuleOutline(getActiveModule());
      });
      setFeedback?.("차트 삽입 완료 (" + DEFAULT_CHART_COLORS.length + "색 팔레트)");
    }

    function syncFnPreview() {
      const fn = $("doc-fn-name")?.value || "SUM";
      const dir = $("doc-fn-dir")?.value || "ABOVE";
      const prev = $("doc-fn-preview");
      if (prev) prev.textContent = "=" + fn + "(" + dir + ")";
    }

    function openFnWizardDialog() {
      bind();
      const evalBox = $("doc-fn-eval");
      if (evalBox && evalBox.checked === undefined) {
        evalBox.checked = evalBox.getAttribute?.("checked") !== null;
      }
      syncFnPreview();
      const st = $("doc-fn-status");
      const inTable = Boolean(getDocTable()?.selectionAt?.());
      if (st) st.textContent = inTable ? "" : "표 안의 셀을 먼저 선택해야 수식을 삽입할 수 있습니다.";
      openToolDialog("doc-fn-dialog");
      $("btn-doc-fn-ok")?.focus?.();
    }

    function applyFnWizardDialog() {
      const docTable = getDocTable();
      const sel = docTable?.selectionAt?.();
      const st = $("doc-fn-status");
      if (!sel || !sel.from) {
        if (st) st.textContent = "표 안에서 사용";
        return;
      }
      const fn = $("doc-fn-name")?.value || "SUM";
      const dir = $("doc-fn-dir")?.value || "ABOVE";
      const formula = "=" + fn + "(" + dir + ")";
      const autoEval = Boolean($("doc-fn-eval")?.checked);
      closeDialog?.("doc-fn-dialog");
      withDocStyleUndo(() => {
        sel.from.replaceChildren(document.createTextNode(formula));
        if (autoEval) docTable.recalculate();
        computeSmartStats();
      });
      setFeedback?.("표 수식 적용: " + formula);
    }

    function bind() {
      if (bound) return;
      bound = true;
      $("btn-doc-mailmerge-add")?.addEventListener("click", () => {
        const added = addMergeFieldRow("", "");
        added?.keyInput?.focus?.();
      });
      $("btn-doc-mailmerge-token")?.addEventListener("click", insertMergeTokenField);
      $("btn-doc-mailmerge-apply")?.addEventListener("click", applyMailMergeDialog);
      $("btn-doc-shape-ok")?.addEventListener("click", applyShapeDialog);
      $("btn-doc-chart-ok")?.addEventListener("click", applyChartDialog);
      $("doc-fn-name")?.addEventListener("change", syncFnPreview);
      $("doc-fn-dir")?.addEventListener("change", syncFnPreview);
      $("btn-doc-fn-ok")?.addEventListener("click", applyFnWizardDialog);
      const cancelMm = () => {
        pendingMailMergeRun = null;
        closeDialog?.("doc-mailmerge-dialog");
      };
      $("doc-mailmerge-dialog")?.querySelectorAll?.("[data-dialog-close]").forEach((b) => b.addEventListener("click", cancelMm));
      $("doc-mailmerge-dialog")?.addEventListener("close", () => { pendingMailMergeRun = null; });
    }

    return {
      openMailMergeDialog,
      mergeTokensInDom,
      openShapeDialog,
      openChartDialog,
      openFnWizardDialog,
    };
  }

  function buildSplitCompareBody(document, mod, ctx) {
    const body = document.createElement("div");
    body.className = "split-compare-body";
    if (mod === "DOC" || mod === "HWP") {
      const editor = document.getElementById("doc-editor");
      for (const el of Array.from(editor?.children || [])) {
        if (ctx.isNonParagraphStudioContainer(el)) continue;
        const tag = String(el.tagName || "").toUpperCase();
        if (tag === "TABLE") {
          const tbl = document.createElement("table");
          tbl.className = "split-compare-table";
          for (const srcTr of Array.from(el.querySelectorAll("tr")).slice(0, 20)) {
            const tr = document.createElement("tr");
            for (const srcCell of Array.from(srcTr.children).slice(0, 8)) {
              const td = document.createElement("td");
              td.textContent = (srcCell.textContent || "").trim();
              tr.appendChild(td);
            }
            tbl.appendChild(tr);
          }
          body.appendChild(tbl);
          continue;
        }
        const txt = (el.textContent || "").trim();
        if (!txt) continue;
        const p = document.createElement(/^H[1-6]$/.test(tag) ? tag.toLowerCase() : "p");
        p.textContent = txt;
        body.appendChild(p);
      }
      return body;
    }
    if (mod === "SHEET") {
      const sName = ctx.getActiveSheetName();
      const map = ctx.sheetDataStore[sName] || {};
      const tbl = document.createElement("table");
      tbl.className = "split-compare-table";
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      let maxR = 1, maxC = 1;
      for (const ref of Object.keys(map)) {
        const at = ctx.parseCellRefParts?.(ref) || host.__EASYOFFICE_SHEET_GRID_UTILS__?.parseRef?.(ref);
        if (at) { maxR = Math.min(25, Math.max(maxR, at.row)); maxC = Math.min(8, Math.max(maxC, at.col)); }
      }
      for (let r = 1; r <= maxR; r += 1) {
        const tr = document.createElement("tr");
        for (let c = 1; c <= maxC; c += 1) {
          const td = document.createElement("td");
          const col = utils?.colIndexToLetters ? utils.colIndexToLetters(c - 1) : String.fromCharCode(64 + c);
          const ent = map[col + r];
          td.textContent = ent ? String(ent.value ?? ent.formula ?? "") : "";
          tr.appendChild(td);
        }
        tbl.appendChild(tr);
      }
      body.appendChild(tbl);
      return body;
    }
    if (mod === "SLIDE") {
      ctx.slideDeckState.forEach((s, idx) => {
        const card = document.createElement("div");
        const h = document.createElement("div");
        h.className = "split-compare-heading";
        h.textContent = (idx + 1) + ". " + (s.title || "제목 없음");
        card.appendChild(h);
        const sub = [s.lower, s.notes].filter(Boolean).join(" · ");
        if (sub) {
          const p = document.createElement("p");
          p.textContent = sub;
          card.appendChild(p);
        }
        body.appendChild(card);
      });
      return body;
    }
    const text = String(ctx.extractEditableDocumentText() || "");
    for (const line of text.split("\n").slice(0, 60)) {
      if (!line.trim()) continue;
      const p = document.createElement("p");
      p.textContent = line;
      body.appendChild(p);
    }
    return body;
  }

  function syncSplitComparePane(document, stage, ctx) {
    if (!stage) return;
    const existing = stage.querySelector(".split-compare-pane");
    if (stage.dataset.splitView !== "true") {
      if (existing) existing.remove();
      return;
    }
    const mod = ctx.getActiveModule();
    const pane = existing || document.createElement("aside");
    pane.className = "split-compare-pane";
    pane.dataset.splitCompare = "true";
    const heading = document.createElement("div");
    heading.className = "split-compare-heading";
    const docName = ctx.getWindowBridge?.()?.getDocumentName(mod) || "";
    heading.textContent = mod === "SHEET" ? ctx.getActiveSheetName() : (docName || document.querySelector("#doc-editor h1")?.textContent?.trim() || "분할 보기");
    pane.replaceChildren(heading, buildSplitCompareBody(document, mod, ctx));
    if (!existing) stage.appendChild(pane);
  }

  host.__EASYOFFICE_SPLIT_COMPARE__ = { syncSplitComparePane };
  host.__EASYOFFICE_CREATE_DOC_DIALOGS_EXT__ = createDocDialogsExt;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
