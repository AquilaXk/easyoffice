// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` lines 132, 135, 188, 202, 1274, 1300
// (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`) for Korean slide object dialog labels.

(function (host) {
  "use strict";

  const DIALOG_IDS = [
    "slide-layout-dialog",
    "slide-shape-dialog",
    "slide-table-dialog",
    "slide-chart-dialog",
    "slide-image-dialog",
  ];

  function parseCsvTokens(raw) {
    return String(raw || "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
  }

  function parseSeriesTextarea(raw) {
    const lines = String(raw || "")
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter(Boolean);
    const out = [];
    lines.forEach((line, idx) => {
      const colon = line.indexOf(":");
      const name = colon >= 0 ? line.slice(0, colon).trim() || "계열 " + (idx + 1) : "계열 " + (idx + 1);
      const rest = colon >= 0 ? line.slice(colon + 1) : line;
      const values = rest
        .split(",")
        .map((t) => t.trim())
        .filter((t) => t !== "")
        .map((t) => Number(t));
      if (values.some((v) => Number.isFinite(v))) {
        out.push({ name, values: values.map((v) => (Number.isFinite(v) ? v : 0)) });
      }
    });
    return out;
  }

  function formatSeriesTextarea(series) {
    if (!Array.isArray(series) || series.length === 0) return "매출: 120, 165, 210";
    return series.map((s, idx) => (s.name || "계열 " + (idx + 1)) + ": " + (s.values || []).join(", ")).join("\n");
  }

  function buildInitialTableRows(rowCount, colCount, headerRow) {
    const rows = [];
    for (let r = 0; r < rowCount; r += 1) {
      const row = [];
      for (let c = 0; c < colCount; c += 1) {
        if (r === 0 && headerRow) row.push("항목 " + (c + 1));
        else row.push("");
      }
      rows.push(row);
    }
    return rows;
  }

  function resizeExistingRows(prevRows, rowCount, colCount) {
    const out = [];
    for (let r = 0; r < rowCount; r += 1) {
      const src = Array.isArray(prevRows?.[r]) ? prevRows[r] : [];
      const row = [];
      for (let c = 0; c < colCount; c += 1) row.push(String(src[c] ?? ""));
      out.push(row);
    }
    return out;
  }

  function createSlideDialogs(deps, canvas) {
    const { document, slideDeckState: deck, getActiveSlideIndex } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
    const $ = (id) => document.getElementById(id);

    let editingElId = null;
    let pendingStubKind = "";
    let imageRatio = 1.5;
    let openedImgW = 240;
    let openedImgH = 160;

    function closeMenuDropdowns() {
      document.querySelectorAll("#menu-bar .menu-item.open, #menu-bar .md-sub.open").forEach((el) => {
        el.classList.remove("open");
        el.querySelector?.(".menu-title")?.setAttribute("aria-expanded", "false");
      });
    }

    function closeAll() {
      editingElId = null;
      pendingStubKind = "";
      for (const id of DIALOG_IDS) kit.closeDialog(id);
    }

    function readBoxFields(prefix, defaults) {
      const num = (suffix, def) => {
        const v = Number(kit.val(prefix + "-" + suffix));
        return Number.isFinite(v) ? v : def;
      };
      return {
        x: num("x", defaults.x),
        y: num("y", defaults.y),
        w: num("w", defaults.w),
        h: num("h", defaults.h),
      };
    }

    function writeBoxFields(prefix, box) {
      kit.setVal(prefix + "-x", String(Math.round(box.x)));
      kit.setVal(prefix + "-y", String(Math.round(box.y)));
      kit.setVal(prefix + "-w", String(Math.round(box.w)));
      kit.setVal(prefix + "-h", String(Math.round(box.h)));
    }

    function setDeleteVisible(btnId, show) {
      const btn = $(btnId);
      if (btn) btn.hidden = !show;
    }

    // ---- 1. Master layout dialog -------------------------------------------------------------

    function openLayoutDialog() {
      const slide = deck[getActiveSlideIndex()] || deck[0];
      kit.note("slide-layout-status", "");
      kit.setSelect("slide-layout-dialog-select", codec.normalizeLayout(slide?.layout));
      kit.openDialog("slide-layout-dialog");
      $("btn-slide-layout-ok")?.focus?.();
    }

    function applyLayoutDialog() {
      const chosen = codec.normalizeLayout(kit.val("slide-layout-dialog-select"));
      kit.closeDialog("slide-layout-dialog");
      canvas.setSlideLayout(chosen, { recordUndo: true });
    }

    // ---- 2. Shape dialog ---------------------------------------------------------------------

    function openShapeDialog(existing = null, stubKind = "") {
      editingElId = existing?.id || null;
      pendingStubKind = existing ? "" : stubKind;
      const stage = canvas.stageSize();
      const stagger = (canvas.getElements().length % 4) * 18;
      const box = existing || { x: Math.round(stage.w / 2 - 110 + stagger), y: Math.round(150 + stagger), w: 220, h: 100 };
      kit.note("slide-shape-status", "");
      kit.setSelect("slide-shape-type", existing?.shapeType || "roundRect");
      kit.setVal("slide-shape-text", existing?.text ?? "도형");
      kit.setChecked("slide-shape-bold", existing ? Boolean(existing.bold) : true);
      kit.setChecked("slide-shape-italic", Boolean(existing?.italic));
      kit.setChecked("slide-shape-underline", Boolean(existing?.underline));
      kit.setVal("slide-shape-fill", "#" + codec.normalizeHex6(existing?.fill, "2563EB").toLowerCase());
      kit.setVal("slide-shape-stroke", "#" + codec.normalizeHex6(existing?.stroke, "1D4ED8").toLowerCase());
      kit.setVal("slide-shape-stroke-width", String(existing?.strokeWidthPt ?? 2));
      writeBoxFields("slide-shape", box);
      setDeleteVisible("btn-slide-shape-delete", Boolean(existing));
      kit.openDialog("slide-shape-dialog");
      $("btn-slide-shape-ok")?.focus?.();
    }

    function applyShapeDialog() {
      const box = readBoxFields("slide-shape", { x: 360, y: 160, w: 220, h: 100 });
      if (box.w < 24 || box.h < 20) {
        kit.note("slide-shape-status", "도형 크기는 가로 24pt, 세로 20pt 이상이어야 합니다.");
        return;
      }
      const spec = {
        kind: "shape",
        ...box,
        shapeType: kit.val("slide-shape-type") || "rect",
        text: kit.val("slide-shape-text"),
        bold: kit.checked("slide-shape-bold"),
        italic: kit.checked("slide-shape-italic"),
        underline: kit.checked("slide-shape-underline"),
        fill: codec.normalizeHex6(kit.val("slide-shape-fill"), "2563EB"),
        stroke: codec.normalizeHex6(kit.val("slide-shape-stroke"), "1D4ED8"),
        strokeWidthPt: Number(kit.val("slide-shape-stroke-width") ?? 2),
      };
      const targetId = editingElId;
      const stub = pendingStubKind;
      editingElId = null;
      pendingStubKind = "";
      kit.closeDialog("slide-shape-dialog");
      if (targetId) canvas.updateElement(targetId, spec);
      else canvas.addElement(spec, { recordUndo: !stub, clearStubKind: stub });
    }

    // ---- 3. Table dialog ---------------------------------------------------------------------

    function openTableDialog(existing = null, stubKind = "") {
      editingElId = existing?.id || null;
      pendingStubKind = existing ? "" : stubKind;
      const rows = existing?.rows || [];
      const rCount = rows.length || 3;
      const cCount = rows[0]?.length || 3;
      const box = existing || { x: 240, y: 200, w: 420, h: 160 };
      kit.note("slide-table-status", "");
      kit.setVal("slide-table-rows", String(rCount));
      kit.setVal("slide-table-cols", String(cCount));
      kit.setChecked("slide-table-header", existing ? Boolean(existing.headerRow) : true);
      kit.setVal("slide-table-fill", "#" + codec.normalizeHex6(existing?.fill, "DBEAFE").toLowerCase());
      writeBoxFields("slide-table", box);
      setDeleteVisible("btn-slide-table-delete", Boolean(existing));
      kit.openDialog("slide-table-dialog");
      $("btn-slide-table-ok")?.focus?.();
    }

    function applyTableDialog() {
      const rCount = Math.trunc(Number(kit.val("slide-table-rows")));
      const cCount = Math.trunc(Number(kit.val("slide-table-cols")));
      if (!Number.isFinite(rCount) || rCount < 1 || rCount > 64 || !Number.isFinite(cCount) || cCount < 1 || cCount > 32) {
        kit.note("slide-table-status", "표 크기는 1~64줄, 1~32칸 범위여야 합니다.");
        return;
      }
      const box = readBoxFields("slide-table", { x: 240, y: 200, w: 420, h: 160 });
      const headerRow = kit.checked("slide-table-header");
      const fill = codec.normalizeHex6(kit.val("slide-table-fill"), "DBEAFE");
      const targetId = editingElId;
      const stub = pendingStubKind;
      const prev = targetId ? canvas.getElements().find((e) => e.id === targetId) : null;
      editingElId = null;
      pendingStubKind = "";
      kit.closeDialog("slide-table-dialog");
      if (prev) {
        canvas.updateElement(targetId, {
          ...box,
          headerRow,
          fill,
          rows: resizeExistingRows(prev.rows, rCount, cCount),
        });
      } else {
        canvas.addElement(
          { kind: "table", ...box, headerRow, fill, rows: buildInitialTableRows(rCount, cCount, headerRow) },
          { recordUndo: !stub, clearStubKind: stub }
        );
      }
    }

    // ---- 4. Chart dialog ---------------------------------------------------------------------

    function openChartDialog(existing = null, stubKind = "") {
      editingElId = existing?.id || null;
      pendingStubKind = existing ? "" : stubKind;
      const box = existing || { x: 480, y: 260, w: 380, h: 200 };
      kit.note("slide-chart-status", "");
      kit.setSelect("slide-chart-type", existing?.chartType || "column");
      kit.setVal("slide-chart-heading", existing?.title ?? "분기 실적");
      kit.setSelect("slide-chart-legend", existing?.legend || "right");
      kit.setVal("slide-chart-categories", (existing?.categories || ["1분기", "2분기", "3분기"]).join(", "));
      kit.setVal("slide-chart-series", formatSeriesTextarea(existing?.series));
      writeBoxFields("slide-chart", box);
      setDeleteVisible("btn-slide-chart-delete", Boolean(existing));
      kit.openDialog("slide-chart-dialog");
      $("btn-slide-chart-ok")?.focus?.();
    }

    function applyChartDialog() {
      const categories = parseCsvTokens(kit.val("slide-chart-categories"));
      const series = parseSeriesTextarea(kit.val("slide-chart-series"));
      if (categories.length === 0 || series.length === 0) {
        kit.note("slide-chart-status", "항목과 최소 1개 이상의 숫자 계열을 입력하세요.");
        return;
      }
      const box = readBoxFields("slide-chart", { x: 480, y: 260, w: 380, h: 200 });
      const spec = {
        kind: "chart",
        ...box,
        chartType: kit.val("slide-chart-type") || "column",
        title: kit.val("slide-chart-heading").trim(),
        legend: kit.val("slide-chart-legend") || "right",
        categories,
        series,
      };
      const targetId = editingElId;
      const stub = pendingStubKind;
      editingElId = null;
      pendingStubKind = "";
      kit.closeDialog("slide-chart-dialog");
      if (targetId) canvas.updateElement(targetId, spec);
      else canvas.addElement(spec, { recordUndo: !stub, clearStubKind: stub });
    }

    // ---- 5. Image properties dialog ----------------------------------------------------------

    function openImageDialog(existing) {
      if (!existing || existing.kind !== "image") return;
      editingElId = existing.id;
      pendingStubKind = "";
      openedImgW = Math.max(1, Math.round(existing.w));
      openedImgH = Math.max(1, Math.round(existing.h));
      imageRatio = openedImgW / openedImgH;
      kit.note("slide-image-status", "");
      writeBoxFields("slide-image", existing);
      kit.setChecked("slide-image-lock-ratio", true);
      kit.setVal("slide-image-alt", existing.alt || "");
      setDeleteVisible("btn-slide-image-delete", true);
      kit.openDialog("slide-image-dialog");
      $("btn-slide-image-ok")?.focus?.();
    }

    function applyImageDialog() {
      const targetId = editingElId;
      const prev = targetId ? canvas.getElements().find((e) => e.id === targetId) : null;
      if (!prev) {
        kit.closeDialog("slide-image-dialog");
        return;
      }
      const box = readBoxFields("slide-image", prev);
      let { w, h } = box;
      if (kit.checked("slide-image-lock-ratio")) {
        if (Math.round(w) !== openedImgW && Math.round(h) === openedImgH && w > 0) {
          h = Math.max(20, Math.round(w / imageRatio));
        } else if (Math.round(h) !== openedImgH && Math.round(w) === openedImgW && h > 0) {
          w = Math.max(24, Math.round(h * imageRatio));
        }
      }
      if (w < 24 || h < 20) {
        kit.note("slide-image-status", "그림 크기는 가로 24pt, 세로 20pt 이상이어야 합니다.");
        return;
      }
      editingElId = null;
      kit.closeDialog("slide-image-dialog");
      canvas.updateElement(targetId, { x: box.x, y: box.y, w, h, alt: kit.val("slide-image-alt").trim() });
    }

    function deleteFromOpenDialog(dialogId) {
      const targetId = editingElId;
      editingElId = null;
      pendingStubKind = "";
      kit.closeDialog(dialogId);
      if (targetId) canvas.deleteElement(targetId);
    }

    function openForRibbonCommand(cmdId) {
      if (cmdId === "slide-mod-master") openLayoutDialog();
      else if (cmdId === "slide-insert-shape") openShapeDialog(null);
      else if (cmdId === "slide-insert-table") openTableDialog(null);
      else if (cmdId === "slide-insert-chart") openChartDialog(null);
    }

    function runSlideMenuAction(act) {
      closeMenuDropdowns();
      if (act === "insert-textbox") {
        const stagger = (canvas.getElements().length % 4) * 18;
        canvas.addElement({ kind: "textbox", x: 100 + stagger, y: 130 + stagger, w: 340, h: 84, text: "글상자" });
      } else if (act === "insert-picture") {
        $("slide-image-file-input")?.click();
      } else if (act === "bring-front") {
        canvas.bringToFront();
      } else if (act === "send-back") {
        canvas.sendToBack();
      } else if (act === "delete-element") {
        canvas.deleteElement();
      }
    }

    function bind() {
      kit.bindColors(["slide-shape-fill", "slide-shape-stroke", "slide-table-fill"]);

      canvas.setDoubleClickHandler((el) => {
        if (el.kind === "shape") openShapeDialog(el);
        else if (el.kind === "table") openTableDialog(el);
        else if (el.kind === "chart") openChartDialog(el);
        else if (el.kind === "image") openImageDialog(el);
      });

      for (const id of DIALOG_IDS) {
        const dlg = $(id);
        dlg?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => {
          btn.addEventListener("click", () => {
            editingElId = null;
            pendingStubKind = "";
            kit.closeDialog(id);
          });
        });
      }

      $("btn-slide-layout-ok")?.addEventListener("click", applyLayoutDialog);
      $("btn-slide-shape-ok")?.addEventListener("click", applyShapeDialog);
      $("btn-slide-table-ok")?.addEventListener("click", applyTableDialog);
      $("btn-slide-chart-ok")?.addEventListener("click", applyChartDialog);
      $("btn-slide-image-ok")?.addEventListener("click", applyImageDialog);

      $("btn-slide-shape-delete")?.addEventListener("click", () => deleteFromOpenDialog("slide-shape-dialog"));
      $("btn-slide-table-delete")?.addEventListener("click", () => deleteFromOpenDialog("slide-table-dialog"));
      $("btn-slide-chart-delete")?.addEventListener("click", () => deleteFromOpenDialog("slide-chart-dialog"));
      $("btn-slide-image-delete")?.addEventListener("click", () => deleteFromOpenDialog("slide-image-dialog"));

      $("btn-slide-image-reset")?.addEventListener("click", () => {
        const cur = editingElId ? canvas.getElements().find((e) => e.id === editingElId) : null;
        if (!cur) return;
        const stage = canvas.stageSize();
        const nw = Math.min(stage.w, Math.max(24, cur.naturalWidthPx || cur.w));
        const nh = Math.min(stage.h, Math.max(20, cur.naturalHeightPx || cur.h));
        kit.setVal("slide-image-w", String(Math.round(nw)));
        kit.setVal("slide-image-h", String(Math.round(nh)));
      });

      const imgInput = $("slide-image-file-input");
      imgInput?.addEventListener("change", () => {
        const file = imgInput.files?.[0];
        imgInput.value = "";
        if (!file) return;
        void Promise.resolve(file.arrayBuffer())
          .then((buf) => canvas.insertPictureFromBytes(new Uint8Array(buf), file.name))
          .catch(() => {});
      });

      document.querySelectorAll("#menu-bar [data-slide-act]").forEach((item) => {
        item.addEventListener("click", () => runSlideMenuAction(item.dataset.slideAct));
      });
    }

    return {
      bind,
      closeAll,
      openLayoutDialog,
      openShapeDialog,
      openTableDialog,
      openChartDialog,
      openImageDialog,
      openForRibbonCommand,
      runSlideMenuAction,
    };
  }

  host.__EASYOFFICE_CREATE_SLIDE_DIALOGS__ = createSlideDialogs;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
