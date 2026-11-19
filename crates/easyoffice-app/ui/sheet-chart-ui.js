// 참조 구현 (reference implementation): rhwp-studio `src/ui/chart-data-dialog.ts` and `src/i18n/locales/ko.ts` line 155
// (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`) for Korean chart dialog labels (`차트 데이터 편집`)
// and modal editing workflow, paired with floating sheet card positioning (`sheet-slicer.js`).

(function (host) {
  "use strict";

  const MIN_WIDTH = 180;
  const MAX_WIDTH = 1600;
  const MIN_HEIGHT = 140;
  const MAX_HEIGHT = 1200;
  const MAX_POS = 50000000;
  const DIALOG_LEGENDS = new Set(["right", "bottom", "top", "none"]);

  function createSheetChartUi(deps) {
    const {
      document,
      grid,
      sheetDataStore,
      getActiveSheetName,
      getActiveModule,
      getSelectionRange,
      getExtent,
      snapshotDomSheetCells,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
      focusAnchor,
    } = deps;

    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const svgApi = host.__EASYOFFICE_SHEET_CHART_SVG__;
    const refs = host.__EASYOFFICE_SHEET_REFS__;
    const $ = (id) => document.getElementById(id);

    let bound = false;
    let editingChartId = null;
    let selectedChartId = null;
    let activeDrag = null;

    function isProtected(sheet, notifyUser) {
      const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
      if (!protectApi?.isOn(sheet)) return false;
      if (notifyUser) protectApi.alertBlocked?.();
      return true;
    }

    function setStatus(msg) {
      const el = $("sheet-chart-status");
      if (el) el.textContent = msg || "";
    }

    function rowAtContentY(gy) {
      const target = Math.max(0, gy);
      let r = grid.rowAtVisible(Math.max(0, Math.floor(target / grid.ROW_H)));
      while (r > 1 && grid.rowTop(r) > target) r -= 1;
      while (r < grid.MAX_ROWS && grid.rowTop(r + 1) <= target) r += 1;
      return r;
    }

    function anchorFromRect(rawLeft, rawTop, rawWidth, rawHeight) {
      const left = Math.max(0, Math.min(MAX_POS, Math.round(Number(rawLeft) || 0)));
      const top = Math.max(0, Math.min(MAX_POS, Math.round(Number(rawTop) || 0)));
      const width = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, Math.round(Number(rawWidth) || 420)));
      const height = Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, Math.round(Number(rawHeight) || 260)));
      const gx0 = Math.max(0, left - grid.ROW_HEAD_W);
      const gy0 = Math.max(0, top - grid.HEAD_H);
      const fromCol = Math.max(0, Math.min(16383, grid.colAtX(gx0) - 1));
      const fromRow = Math.max(0, Math.min(1048575, rowAtContentY(gy0) - 1));
      const toCol = Math.max(fromCol, Math.min(16383, grid.colAtX(gx0 + width) - 1));
      const toRow = Math.max(fromRow, Math.min(1048575, rowAtContentY(gy0 + height) - 1));
      return { fromCol, fromRow, toCol, toRow, left, top, width, height };
    }

    function formatRangeBox(box) {
      const a = refs.refOf(box.r1, box.c1);
      const b = refs.refOf(box.r2, box.c2);
      return a === b ? a : a + ":" + b;
    }

    function detectDefaultRange(sheet) {
      snapshotDomSheetCells?.();
      const rawSel = getSelectionRange?.() || { r1: 1, c1: 1, r2: 1, c2: 1 };
      const sel = {
        r1: Math.min(rawSel.r1, rawSel.r2),
        c1: Math.min(rawSel.c1, rawSel.c2),
        r2: Math.max(rawSel.r1, rawSel.r2),
        c2: Math.max(rawSel.c1, rawSel.c2),
      };
      if (sel.r2 > sel.r1 || sel.c2 > sel.c1) return formatRangeBox(sel);
      const map = sheetDataStore[sheet] || {};
      const has = (r, c) => {
        const entry = map[refs.refOf(r, c)];
        return Boolean(entry && String(entry.value ?? entry.formula ?? "").trim() !== "");
      };
      const ext = getExtent?.() || { rows: 60, cols: 26 };
      const filterCore = host.__EASYOFFICE_SHEET_FILTER_CORE__;
      const expanded = filterCore?.regionFrom ? filterCore.regionFrom(sel, has, ext) : sel;
      if (expanded && (expanded.r2 > expanded.r1 || expanded.c2 > expanded.c1)) {
        return formatRangeBox(expanded);
      }
      if (has(sel.r1, sel.c1)) return refs.refOf(sel.r1, sel.c1);
      return refs.refOf(sel.r1, sel.c1) + ":" + refs.refOf(Math.min(grid.MAX_ROWS, sel.r1 + 4), Math.min(grid.MAX_COLS, sel.c1 + 2));
    }

    function commitChartsMutation(sheet, nextCharts) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const step = sheetHistory.begin({ parts: ["sheetLayout"] });
        layoutApi.setCharts(sheet, nextCharts);
        sheetHistory.commit(step);
      });
    }

    function openChartDialog(chartId = null) {
      ensureBound();
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      const charts = layoutApi.chartsOf(sheet);
      const existing = chartId ? charts.find((c) => c.id === chartId) || null : null;
      editingChartId = existing ? existing.id : null;

      const titleEl = $("sheet-chart-title");
      if (titleEl) titleEl.textContent = existing ? "차트 데이터 편집" : "차트 삽입";
      const deleteBtn = $("btn-sheet-chart-delete");
      if (deleteBtn) deleteBtn.hidden = !existing;

      kit.setSelect("sheet-chart-type", existing ? existing.chartType : "column");
      kit.setVal("sheet-chart-range", existing ? existing.dataRange : detectDefaultRange(sheet));
      kit.setVal("sheet-chart-heading", existing ? existing.title : "");
      const legendVal = existing && DIALOG_LEGENDS.has(existing.legend) ? existing.legend : "right";
      kit.setSelect("sheet-chart-legend", legendVal);
      kit.setChecked("sheet-chart-switch", Boolean(existing?.switchRowCol));
      setStatus("");

      if (!kit.openDialog("sheet-chart-dialog")) return false;
      $("btn-sheet-chart-ok")?.focus?.();
      return true;
    }

    async function handleSubmit() {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) {
        kit.closeDialog("sheet-chart-dialog");
        return;
      }
      snapshotDomSheetCells?.();
      const chartType = kit.val("sheet-chart-type") || "column";
      const dataRange = kit.val("sheet-chart-range").trim();
      const title = kit.val("sheet-chart-heading").trim().slice(0, 160);
      const legend = DIALOG_LEGENDS.has(kit.val("sheet-chart-legend")) ? kit.val("sheet-chart-legend") : "right";
      const switchRowCol = kit.checked("sheet-chart-switch");

      const parsedBox = svgApi.parseSheetRangeRef(dataRange, sheet);
      if (!parsedBox) {
        setStatus("유효한 데이터 범위를 입력하세요.");
        return;
      }

      const current = layoutApi.chartsOf(sheet);
      try {
        let nextCharts;
        if (editingChartId) {
          const idx = current.findIndex((c) => c.id === editingChartId);
          if (idx < 0) {
            kit.closeDialog("sheet-chart-dialog");
            return;
          }
          const prev = current[idx];
          const series = svgApi.deriveChartSeries({ chartType, dataRange, switchRowCol }, sheet, sheetDataStore);
          const updated = {
            ...prev,
            chartType,
            title,
            dataRange,
            switchRowCol,
            legend,
          };
          if (series.length > 0) updated.series = series;
          else delete updated.series;
          nextCharts = current.slice();
          nextCharts[idx] = updated;
          selectedChartId = updated.id;
        } else {
          if (current.length >= layoutApi.MAX_CHARTS_PER_SHEET) {
            setStatus("시트당 차트 수 한도(" + layoutApi.MAX_CHARTS_PER_SHEET + "개)를 초과했습니다.");
            return;
          }
          const baseLeft = grid.ROW_HEAD_W + grid.colLeft(Math.min(grid.MAX_COLS, parsedBox.c2 + 1)) + 16 + (current.length % 4) * 24;
          const baseTop = grid.HEAD_H + grid.rowTop(parsedBox.r1) + 4 + (current.length % 4) * 20;
          const created = svgApi.createChartModel({
            sheetName: sheet,
            sheetDataStore,
            existingCharts: current,
            spec: { chartType, dataRange, title, legend, switchRowCol },
            anchorFromRect: (_l, _t, w, h) => anchorFromRect(baseLeft, baseTop, w, h),
          });
          nextCharts = [...current, created];
          selectedChartId = created.id;
        }
        await commitChartsMutation(sheet, nextCharts);
        editingChartId = null;
        kit.closeDialog("sheet-chart-dialog");
        focusAnchor?.();
      } catch (err) {
        setStatus(err?.message || "차트를 저장할 수 없습니다.");
      }
    }

    async function deleteChartById(chartId) {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      const current = layoutApi.chartsOf(sheet);
      const next = current.filter((c) => c.id !== chartId);
      if (next.length === current.length) return false;
      if (selectedChartId === chartId) selectedChartId = null;
      await commitChartsMutation(sheet, next);
      focusAnchor?.();
      return true;
    }

    function finishDrag() {
      if (!activeDrag) return;
      const drag = activeDrag;
      activeDrag = null;
      document.removeEventListener("mousemove", onDragMove);
      document.removeEventListener("mouseup", onDragEnd);
      if (!drag.moved) return;
      if (isProtected(drag.sheet, false)) {
        syncLayer();
        return;
      }
      const current = layoutApi.chartsOf(drag.sheet);
      const idx = current.findIndex((c) => c.id === drag.chartId);
      if (idx < 0) return;
      const nextAnchor = anchorFromRect(drag.left, drag.top, drag.width, drag.height);
      const prev = current[idx].anchor;
      if (
        prev.left === nextAnchor.left &&
        prev.top === nextAnchor.top &&
        prev.width === nextAnchor.width &&
        prev.height === nextAnchor.height
      ) {
        return;
      }
      const nextCharts = current.slice();
      nextCharts[idx] = { ...current[idx], anchor: nextAnchor };
      void commitChartsMutation(drag.sheet, nextCharts).catch(() => {});
    }

    function onDragMove(ev) {
      if (!activeDrag) return;
      const dx = (Number(ev.clientX) || 0) - activeDrag.startX;
      const dy = (Number(ev.clientY) || 0) - activeDrag.startY;
      if (dx === 0 && dy === 0 && !activeDrag.moved) return;
      activeDrag.moved = true;
      if (activeDrag.kind === "move") {
        activeDrag.left = Math.max(0, Math.min(MAX_POS, Math.round(activeDrag.origLeft + dx)));
        activeDrag.top = Math.max(0, Math.min(MAX_POS, Math.round(activeDrag.origTop + dy)));
        activeDrag.card.style.left = activeDrag.left + "px";
        activeDrag.card.style.top = activeDrag.top + "px";
      } else {
        activeDrag.width = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, Math.round(activeDrag.origWidth + dx)));
        activeDrag.height = Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, Math.round(activeDrag.origHeight + dy)));
        activeDrag.card.style.width = activeDrag.width + "px";
        activeDrag.card.style.height = activeDrag.height + "px";
        const body = activeDrag.card.querySelector(".sheet-chart-body");
        if (body) {
          const bodyH = Math.max(60, activeDrag.height - 28);
          body.replaceChildren(
            svgApi.renderChartSvg({
              document,
              chart: { ...activeDrag.chart, anchor: { ...activeDrag.chart.anchor, width: activeDrag.width, height: activeDrag.height } },
              sheetName: activeDrag.sheet,
              sheetDataStore,
              width: activeDrag.width - 2,
              height: bodyH,
            })
          );
        }
      }
    }

    function onDragEnd(ev) {
      if (activeDrag && ev) onDragMove(ev);
      finishDrag();
    }

    function startDrag(kind, ev, sheet, chart, card) {
      if (ev.button !== undefined && ev.button !== 0) return;
      if (isProtected(sheet, true)) return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      selectedChartId = chart.id;
      activeDrag = {
        kind,
        sheet,
        chartId: chart.id,
        chart,
        card,
        startX: Number(ev.clientX) || 0,
        startY: Number(ev.clientY) || 0,
        origLeft: chart.anchor.left,
        origTop: chart.anchor.top,
        origWidth: chart.anchor.width,
        origHeight: chart.anchor.height,
        left: chart.anchor.left,
        top: chart.anchor.top,
        width: chart.anchor.width,
        height: chart.anchor.height,
        moved: false,
      };
      document.addEventListener("mousemove", onDragMove);
      document.addEventListener("mouseup", onDragEnd);
    }

    function renderCard(sheet, chart) {
      const card = document.createElement("div");
      card.className = "sheet-chart-card" + (selectedChartId === chart.id ? " is-selected" : "");
      card.dataset.chartId = chart.id;
      card.dataset.chartType = chart.chartType;
      card.tabIndex = 0;
      card.setAttribute("role", "region");
      const labelTitle = chart.title || svgApi.CHART_TYPE_LABELS[chart.chartType] || "차트";
      card.setAttribute("aria-label", "차트: " + labelTitle);
      card.style.left = chart.anchor.left + "px";
      card.style.top = chart.anchor.top + "px";
      card.style.width = chart.anchor.width + "px";
      card.style.height = chart.anchor.height + "px";

      const head = document.createElement("div");
      head.className = "sheet-chart-head";
      const heading = document.createElement("span");
      heading.className = "sheet-chart-card-title";
      heading.textContent = labelTitle;
      head.appendChild(heading);

      const actions = document.createElement("div");
      actions.className = "sheet-chart-actions";

      const editBtn = document.createElement("button");
      editBtn.type = "button";
      editBtn.className = "sheet-chart-btn";
      editBtn.dataset.chartAct = "edit";
      editBtn.title = "편집";
      editBtn.setAttribute("aria-label", "편집");
      editBtn.textContent = "편집";
      editBtn.addEventListener("click", (ev) => {
        ev.stopPropagation?.();
        openChartDialog(chart.id);
      });

      const delBtn = document.createElement("button");
      delBtn.type = "button";
      delBtn.className = "sheet-chart-btn";
      delBtn.dataset.chartAct = "delete";
      delBtn.title = "삭제";
      delBtn.setAttribute("aria-label", "삭제");
      delBtn.textContent = "삭제";
      delBtn.addEventListener("click", (ev) => {
        ev.stopPropagation?.();
        void deleteChartById(chart.id).catch(() => {});
      });

      actions.appendChild(editBtn);
      actions.appendChild(delBtn);
      head.appendChild(actions);

      head.addEventListener("mousedown", (ev) => {
        if (ev.target?.closest?.("button")) return;
        card.focus?.();
        startDrag("move", ev, sheet, chart, card);
      });

      const body = document.createElement("div");
      body.className = "sheet-chart-body";
      const bodyH = Math.max(60, chart.anchor.height - 28);
      body.appendChild(
        svgApi.renderChartSvg({
          document,
          chart,
          sheetName: sheet,
          sheetDataStore,
          width: Math.max(120, chart.anchor.width - 2),
          height: bodyH,
        })
      );

      const resize = document.createElement("span");
      resize.className = "sheet-chart-resize";
      resize.setAttribute("aria-hidden", "true");
      resize.addEventListener("mousedown", (ev) => {
        startDrag("resize", ev, sheet, chart, card);
      });

      card.addEventListener("mousedown", () => {
        selectedChartId = chart.id;
      });
      card.addEventListener("dblclick", (ev) => {
        if (ev.target?.closest?.("button")) return;
        openChartDialog(chart.id);
      });
      card.addEventListener("keydown", (ev) => {
        if (ev.key === "Delete" || ev.key === "Backspace") {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          void deleteChartById(chart.id).catch(() => {});
        } else if (ev.key === "Enter") {
          ev.preventDefault?.();
          openChartDialog(chart.id);
        }
      });

      card.appendChild(head);
      card.appendChild(body);
      card.appendChild(resize);
      return card;
    }

    function syncLayer() {
      const layer = $("sheet-chart-layer");
      if (!layer) return;
      if (getActiveModule && getActiveModule() !== "SHEET") {
        layer.replaceChildren();
        layer.hidden = true;
        return;
      }
      const sheet = getActiveSheetName();
      const charts = layoutApi.chartsOf(sheet);
      if (!charts.length) {
        layer.replaceChildren();
        layer.hidden = true;
        return;
      }
      const cards = charts.map((chart) => renderCard(sheet, chart));
      layer.replaceChildren(...cards);
      layer.hidden = false;
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      $("btn-sheet-chart-ok")?.addEventListener("click", () => {
        void handleSubmit().catch(() => {});
      });
      $("btn-sheet-chart-delete")?.addEventListener("click", () => {
        const id = editingChartId;
        editingChartId = null;
        kit.closeDialog("sheet-chart-dialog");
        if (id) void deleteChartById(id).catch(() => {});
      });
      const dialog = $("sheet-chart-dialog");
      dialog?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" && (ev.target?.id === "sheet-chart-range" || ev.target?.id === "sheet-chart-heading")) {
          ev.preventDefault?.();
          void handleSubmit().catch(() => {});
        }
      });
      layoutApi.onChange(syncLayer);
    }

    ensureBound();
    return { openChartDialog, syncLayer, ensureBound, anchorFromRect };
  }

  host.__EASYOFFICE_CREATE_SHEET_CHART_UI__ = createSheetChartUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
