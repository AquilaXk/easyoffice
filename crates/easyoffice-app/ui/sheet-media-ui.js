// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` lines 188, 1274, 1300
// (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`) for Korean picture labels
// (`그림`, `비율 유지`, `원래 크기로`, `개체 지우기`), paired with two-cell floating sheet anchoring (`sheet-chart-ui.js`).

(function (host) {
  "use strict";

  const MIN_DRAG_PX = 16;
  const MIN_DIALOG_PX = 1;
  const MAX_SIZE_PX = 4096;
  const MAX_INIT_WIDTH_PX = 480;
  const MAX_INIT_HEIGHT_PX = 360;
  const MAX_POS_PX = 50000000;

  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

  function altFromHint(mimeOrName) {
    const raw = String(mimeOrName || "").trim();
    if (!raw || raw.includes("/")) return "";
    return raw.split(/[\\/]/).pop().trim().replace(/\.(?:png|jpe?g)$/i, "");
  }

  function createSheetMediaUi(deps) {
    const {
      document,
      grid,
      getActiveSheetName,
      getActiveModule,
      getSelection,
      getRange,
      commitIfEditing,
      focusAnchor,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
    } = deps;

    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const $ = (id) => document.getElementById(id);

    let bound = false;
    let selectedImageId = null;
    let editingImageId = null;
    let dialogAspect = 1;
    let openedW = 0;
    let openedH = 0;
    let activeDrag = null;

    function say(text) {
      const fb = $("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    function setDialogStatus(msg) {
      const el = $("sheet-image-status");
      if (el) el.textContent = msg || "";
    }

    function isProtected(sheet, notifyUser) {
      const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
      if (!protectApi?.isOn(sheet)) return false;
      const p = layoutApi.protectOf(sheet);
      if (Array.isArray(p?.deny) && !p.deny.includes("objects")) return false;
      if (notifyUser) {
        protectApi.alertBlocked?.();
        say("보호된 시트에서는 개체를 편집할 수 없습니다.");
      }
      return true;
    }

    function rowAtContentY(gy) {
      const target = Math.max(0, gy);
      let r = grid.rowAtVisible(Math.max(0, Math.floor(target / grid.ROW_H)));
      while (r > 1 && grid.rowTop(r) > target) r -= 1;
      while (r < grid.MAX_ROWS && grid.rowTop(r + 1) <= target) r += 1;
      return r;
    }

    function contentOriginOf(anchor) {
      const gx = grid.colLeft(anchor.fromCol + 1) + Math.round((anchor.colOffEmu || 0) / layoutApi.EMU_PER_PX);
      const gy = grid.rowTop(anchor.fromRow + 1) + Math.round((anchor.rowOffEmu || 0) / layoutApi.EMU_PER_PX);
      return { gx, gy };
    }

    function screenPosOf(gx, gy) {
      const vp = $("sheet-viewport");
      return {
        left: grid.ROW_HEAD_W + gx - (vp?.scrollLeft || 0),
        top: grid.HEAD_H + gy - (vp?.scrollTop || 0),
      };
    }

    function anchorFromContentRect(gxRaw, gyRaw, wRaw, hRaw) {
      const gx = clamp(Math.round(Number(gxRaw) || 0), 0, MAX_POS_PX);
      const gy = clamp(Math.round(Number(gyRaw) || 0), 0, MAX_POS_PX);
      const widthPx = clamp(Math.round(Number(wRaw) || 240), MIN_DIALOG_PX, MAX_SIZE_PX);
      const heightPx = clamp(Math.round(Number(hRaw) || 160), MIN_DIALOG_PX, MAX_SIZE_PX);
      const c1 = grid.colAtX(gx);
      const r1 = rowAtContentY(gy);
      const fromCol = clamp(c1 - 1, 0, 16383);
      const fromRow = clamp(r1 - 1, 0, 1048575);
      const colOffEmu = Math.max(0, Math.round(Math.max(0, gx - grid.colLeft(c1)) * layoutApi.EMU_PER_PX));
      const rowOffEmu = Math.max(0, Math.round(Math.max(0, gy - grid.rowTop(r1)) * layoutApi.EMU_PER_PX));
      const gx1 = gx + widthPx;
      const gy1 = gy + heightPx;
      const toCol = Math.max(fromCol, clamp(grid.colAtX(gx1) - 1, 0, 16383));
      const toRow = Math.max(fromRow, clamp(rowAtContentY(gy1) - 1, 0, 1048575));
      const toColOffEmu = Math.max(0, Math.round(Math.max(0, gx1 - grid.colLeft(toCol + 1)) * layoutApi.EMU_PER_PX));
      const toRowOffEmu = Math.max(0, Math.round(Math.max(0, gy1 - grid.rowTop(toRow + 1)) * layoutApi.EMU_PER_PX));
      return {
        fromCol,
        fromRow,
        colOffEmu,
        rowOffEmu,
        toCol,
        toRow,
        toColOffEmu,
        toRowOffEmu,
        leftPx: gx,
        topPx: gy,
        widthPx,
        heightPx,
      };
    }

    function workbookImageTotals() {
      const all = layoutApi.getAll();
      let count = 0;
      let bytes = 0;
      const seenIds = new Set();
      for (const sh of Object.keys(all)) {
        for (const im of all[sh]?.images || []) {
          count += 1;
          if (im.imageId && !seenIds.has(im.imageId)) {
            seenIds.add(im.imageId);
            bytes += utils.getDocImage(im.imageId)?.bytesLen || 0;
          }
        }
      }
      return { count, bytes };
    }

    function nextPictureNumbers(existing) {
      let n = existing.length + 1;
      const taken = new Set(existing.map((im) => im.id));
      while (taken.has("pic-" + n)) n += 1;
      return n;
    }

    function commitImagesMutation(sheet, nextImages) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing?.();
        const step = sheetHistory.begin({ parts: ["sheetLayout"] });
        layoutApi.setImages(sheet, nextImages);
        sheetHistory.commit(step);
      });
    }

    async function insertPictureFromBytes(bytes, fileNameOrMime) {
      ensureBound();
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) {
        return { ok: false, error: "보호된 시트에서는 그림을 삽입할 수 없습니다." };
      }
      const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      if (u8.length === 0) {
        const error = "그림을 넣지 못했습니다: 빈 파일입니다.";
        say(error);
        return { ok: false, error };
      }
      if (u8.length > layoutApi.MAX_SINGLE_IMAGE_BYTES) {
        const error = "그림을 넣지 못했습니다: 최대 32 MiB까지 넣을 수 있습니다.";
        say(error);
        return { ok: false, error };
      }
      const header = layoutApi.parseImageHeader(u8);
      if (!header) {
        const error = "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다.";
        say(error);
        return { ok: false, error };
      }
      const area = host.__EASYOFFICE_SHEET_UTILS__.imageAreaProblem(header);
      if (area) {
        const error = "그림을 넣지 못했습니다: " + area + ".";
        say(error);
        return { ok: false, error };
      }
      const current = layoutApi.imagesOf(sheet);
      if (current.length >= layoutApi.MAX_IMAGES_PER_SHEET) {
        const error = "시트당 그림 수 한도(" + layoutApi.MAX_IMAGES_PER_SHEET + "개)를 초과했습니다.";
        say(error);
        return { ok: false, error };
      }
      const totals = workbookImageTotals();
      if (totals.count >= layoutApi.MAX_IMAGES_WORKBOOK) {
        const error = "통합 문서 그림 수 한도(" + layoutApi.MAX_IMAGES_WORKBOOK + "개)를 초과했습니다.";
        say(error);
        return { ok: false, error };
      }
      if (totals.bytes + u8.length > layoutApi.MAX_TOTAL_IMAGE_BYTES) {
        const error = "전체 그림 용량 한도(128 MiB)를 초과했습니다.";
        say(error);
        return { ok: false, error };
      }

      let initW = header.widthPx;
      let initH = header.heightPx;
      if (initW > MAX_INIT_WIDTH_PX || initH > MAX_INIT_HEIGHT_PX) {
        const scale = Math.min(MAX_INIT_WIDTH_PX / initW, MAX_INIT_HEIGHT_PX / initH);
        initW = Math.max(1, Math.round(initW * scale));
        initH = Math.max(1, Math.round(initH * scale));
      }

      const sel = getSelection?.() || getRange?.() || { ar: 1, ac: 1, r1: 1, c1: 1 };
      const r1 = sel.ar || sel.r1 || 1;
      const c1 = sel.ac || sel.c1 || 1;
      const stagger = (current.length % 4) * 16;
      const gx = grid.colLeft(c1) + stagger;
      const gy = grid.rowTop(r1) + stagger;
      const anchor = anchorFromContentRect(gx, gy, initW, initH);

      const imageId = utils.registerDocImage(utils.bytesToBase64(u8), header.format);
      const seq = nextPictureNumbers(current);
      const created = {
        id: "pic-" + seq,
        name: "그림 " + seq,
        alt: layoutApi.trimUtf8(altFromHint(fileNameOrMime), 1024),
        mime: header.format === "jpg" ? "image/jpeg" : "image/png",
        imageId,
        naturalWidthPx: header.widthPx,
        naturalHeightPx: header.heightPx,
        anchor,
      };

      selectedImageId = created.id;
      await commitImagesMutation(sheet, [...current, created]);
      say("그림 삽입");
      return { ok: true, image: created };
    }

    async function deleteImageById(imageId) {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      const current = layoutApi.imagesOf(sheet);
      const next = current.filter((im) => im.id !== imageId);
      if (next.length === current.length) return false;
      if (selectedImageId === imageId) selectedImageId = null;
      await commitImagesMutation(sheet, next);
      focusAnchor?.();
      say("개체 지우기");
      return true;
    }

    function syncDialogSizeFields(wPx, hPx) {
      openedW = Math.max(1, Math.round(wPx));
      openedH = Math.max(1, Math.round(hPx));
      dialogAspect = openedW / Math.max(1, openedH);
      kit.setVal("sheet-image-width", String(openedW));
      kit.setVal("sheet-image-height", String(openedH));
    }

    function openImageDialog(imageId) {
      ensureBound();
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      const img = layoutApi.imagesOf(sheet).find((im) => im.id === imageId);
      if (!img) return false;
      editingImageId = img.id;
      selectedImageId = img.id;
      syncDialogSizeFields(img.anchor.widthPx, img.anchor.heightPx);
      kit.setChecked("sheet-image-lock-ratio", true);
      kit.setVal("sheet-image-alt", img.alt || "");
      setDialogStatus("");
      if (!kit.openDialog("sheet-image-dialog")) return false;
      $("btn-sheet-image-apply")?.focus?.();
      return true;
    }

    async function applyImageDialog() {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) {
        kit.closeDialog("sheet-image-dialog");
        return;
      }
      const current = layoutApi.imagesOf(sheet);
      const idx = current.findIndex((im) => im.id === editingImageId);
      if (idx < 0) {
        kit.closeDialog("sheet-image-dialog");
        return;
      }
      let w = Math.round(Number(kit.val("sheet-image-width")));
      let h = Math.round(Number(kit.val("sheet-image-height")));
      if (kit.checked("sheet-image-lock-ratio")) {
        if (w !== openedW && h === openedH && w > 0) h = Math.max(1, Math.round(w / dialogAspect));
        else if (h !== openedH && w === openedW && h > 0) w = Math.max(1, Math.round(h * dialogAspect));
      }
      if (!Number.isFinite(w) || !Number.isFinite(h) || w < MIN_DIALOG_PX || w > MAX_SIZE_PX || h < MIN_DIALOG_PX || h > MAX_SIZE_PX) {
        setDialogStatus("그림 크기는 1px 이상 4096px 이하여야 합니다.");
        return;
      }
      const prev = current[idx];
      const { gx, gy } = contentOriginOf(prev.anchor);
      const nextAnchor = anchorFromContentRect(gx, gy, w, h);
      const alt = layoutApi.trimUtf8(kit.val("sheet-image-alt").trim(), 1024);
      const nextImages = current.slice();
      nextImages[idx] = { ...prev, alt, anchor: nextAnchor };
      selectedImageId = prev.id;
      await commitImagesMutation(sheet, nextImages);
      editingImageId = null;
      kit.closeDialog("sheet-image-dialog");
      say("그림 크기 및 속성 적용");
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
      const current = layoutApi.imagesOf(drag.sheet);
      const idx = current.findIndex((im) => im.id === drag.imageId);
      if (idx < 0) return;
      const nextAnchor = anchorFromContentRect(drag.gx, drag.gy, drag.width, drag.height);
      const prev = current[idx].anchor;
      if (
        prev.leftPx === nextAnchor.leftPx &&
        prev.topPx === nextAnchor.topPx &&
        prev.widthPx === nextAnchor.widthPx &&
        prev.heightPx === nextAnchor.heightPx
      ) {
        return;
      }
      const nextImages = current.slice();
      nextImages[idx] = { ...current[idx], anchor: nextAnchor };
      void commitImagesMutation(drag.sheet, nextImages).catch(() => {});
    }

    function onDragMove(ev) {
      if (!activeDrag) return;
      const dx = (Number(ev.clientX) || 0) - activeDrag.startX;
      const dy = (Number(ev.clientY) || 0) - activeDrag.startY;
      if (dx === 0 && dy === 0 && !activeDrag.moved) return;
      activeDrag.moved = true;
      if (activeDrag.kind === "move") {
        activeDrag.gx = clamp(Math.round(activeDrag.origGx + dx), 0, MAX_POS_PX);
        activeDrag.gy = clamp(Math.round(activeDrag.origGy + dy), 0, MAX_POS_PX);
        const pos = screenPosOf(activeDrag.gx, activeDrag.gy);
        activeDrag.card.style.left = pos.left + "px";
        activeDrag.card.style.top = pos.top + "px";
      } else {
        let nextW = clamp(Math.round(activeDrag.origWidth + dx), MIN_DRAG_PX, MAX_SIZE_PX);
        let nextH = clamp(Math.round(activeDrag.origHeight + dy), MIN_DRAG_PX, MAX_SIZE_PX);
        if (ev.shiftKey) {
          const ratio = activeDrag.origWidth / Math.max(1, activeDrag.origHeight);
          nextH = clamp(Math.round(nextW / ratio), MIN_DRAG_PX, MAX_SIZE_PX);
          nextW = clamp(Math.round(nextH * ratio), MIN_DRAG_PX, MAX_SIZE_PX);
        }
        activeDrag.width = nextW;
        activeDrag.height = nextH;
        activeDrag.card.style.width = nextW + "px";
        activeDrag.card.style.height = nextH + "px";
      }
    }

    function onDragEnd(ev) {
      if (activeDrag && ev) onDragMove(ev);
      finishDrag();
    }

    function startDrag(kind, ev, sheet, img, card) {
      if (ev.button !== undefined && ev.button !== 0) return;
      selectCard(img.id);
      if (isProtected(sheet, true)) return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      const { gx, gy } = contentOriginOf(img.anchor);
      activeDrag = {
        kind,
        sheet,
        imageId: img.id,
        card,
        startX: Number(ev.clientX) || 0,
        startY: Number(ev.clientY) || 0,
        origGx: gx,
        origGy: gy,
        origWidth: img.anchor.widthPx,
        origHeight: img.anchor.heightPx,
        gx,
        gy,
        width: img.anchor.widthPx,
        height: img.anchor.heightPx,
        moved: false,
      };
      document.addEventListener("mousemove", onDragMove);
      document.addEventListener("mouseup", onDragEnd);
    }

    function selectCard(imageId) {
      selectedImageId = imageId;
      const layer = $("sheet-image-layer");
      if (!layer) return;
      for (const el of layer.querySelectorAll(".sheet-image-card")) {
        el.classList.toggle("selected", el.dataset.imageId === imageId);
      }
    }

    function renderCard(sheet, img) {
      const card = document.createElement("div");
      card.className = "sheet-image-card" + (selectedImageId === img.id ? " selected" : "");
      card.dataset.imageId = img.id;
      card.tabIndex = 0;
      card.setAttribute("role", "region");
      card.setAttribute("aria-label", img.alt || img.name || "그림");
      const { gx, gy } = contentOriginOf(img.anchor);
      const pos = screenPosOf(gx, gy);
      card.style.left = pos.left + "px";
      card.style.top = pos.top + "px";
      card.style.width = img.anchor.widthPx + "px";
      card.style.height = img.anchor.heightPx + "px";

      const imgEl = document.createElement("img");
      imgEl.className = "sheet-image-el";
      const url = utils.docImageUrl(img.imageId);
      if (url) imgEl.setAttribute("src", url);
      imgEl.setAttribute("alt", img.alt || "");
      imgEl.setAttribute("draggable", "false");
      imgEl.dataset.eoImageId = img.imageId;

      const actions = document.createElement("div");
      actions.className = "sheet-image-actions";

      const editBtn = document.createElement("button");
      editBtn.type = "button";
      editBtn.className = "sheet-image-btn";
      editBtn.dataset.imageAct = "edit";
      editBtn.title = "편집";
      editBtn.setAttribute("aria-label", "편집");
      editBtn.textContent = "편집";
      editBtn.addEventListener("click", (ev) => {
        ev.stopPropagation?.();
        openImageDialog(img.id);
      });

      const delBtn = document.createElement("button");
      delBtn.type = "button";
      delBtn.className = "sheet-image-btn";
      delBtn.dataset.imageAct = "delete";
      delBtn.title = "개체 지우기";
      delBtn.setAttribute("aria-label", "개체 지우기");
      delBtn.textContent = "개체 지우기";
      delBtn.addEventListener("click", (ev) => {
        ev.stopPropagation?.();
        void deleteImageById(img.id).catch(() => {});
      });

      actions.appendChild(editBtn);
      actions.appendChild(delBtn);

      const resize = document.createElement("span");
      resize.className = "sheet-image-resize-handle";
      resize.setAttribute("aria-hidden", "true");
      resize.addEventListener("mousedown", (ev) => startDrag("resize", ev, sheet, img, card));

      card.addEventListener("mousedown", (ev) => {
        if (ev.target?.closest?.("button, .sheet-image-resize-handle")) return;
        card.focus?.();
        startDrag("move", ev, sheet, img, card);
      });
      card.addEventListener("dblclick", (ev) => {
        if (ev.target?.closest?.("button")) return;
        openImageDialog(img.id);
      });
      card.addEventListener("keydown", (ev) => {
        if (ev.key === "Delete" || ev.key === "Backspace") {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          void deleteImageById(img.id).catch(() => {});
        } else if (ev.key === "Enter") {
          ev.preventDefault?.();
          openImageDialog(img.id);
        }
      });

      card.appendChild(imgEl);
      card.appendChild(actions);
      card.appendChild(resize);
      return card;
    }

    function syncScrollPositions() {
      const layer = $("sheet-image-layer");
      if (!layer || layer.hidden) return;
      const images = layoutApi.imagesOf(getActiveSheetName());
      if (!images.length) return;
      const byId = new Map(images.map((im) => [im.id, im]));
      for (const card of layer.querySelectorAll(".sheet-image-card")) {
        const img = byId.get(card.dataset.imageId);
        if (!img) continue;
        const { gx, gy } = contentOriginOf(img.anchor);
        const pos = screenPosOf(gx, gy);
        card.style.left = pos.left + "px";
        card.style.top = pos.top + "px";
      }
    }

    function syncLayer() {
      const layer = $("sheet-image-layer");
      if (!layer) return;
      if ((getActiveModule && getActiveModule() !== "SHEET") || $("pane-sheet")?.hidden) {
        layer.replaceChildren();
        layer.hidden = true;
        return;
      }
      const sheet = getActiveSheetName();
      const images = layoutApi.imagesOf(sheet);
      if (!images.length) {
        layer.replaceChildren();
        layer.hidden = true;
        return;
      }
      if (selectedImageId && !images.some((im) => im.id === selectedImageId)) selectedImageId = null;
      layer.replaceChildren(...images.map((img) => renderCard(sheet, img)));
      layer.hidden = false;
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      const input = $("sheet-image-file-input");
      input?.addEventListener("change", () => {
        const file = input.files?.[0];
        input.value = "";
        if (!file) return;
        void Promise.resolve(file.arrayBuffer())
          .then((buf) => insertPictureFromBytes(new Uint8Array(buf), file.name))
          .catch(() => say("그림 파일을 읽지 못했습니다."));
      });
      $("sheet-viewport")?.addEventListener("scroll", syncScrollPositions);
      document.addEventListener("mousedown", (ev) => {
        if (selectedImageId && !ev.target?.closest?.("#sheet-image-layer .sheet-image-card, #sheet-image-dialog")) {
          selectCard(null);
        }
      });
      const wInput = $("sheet-image-width");
      const hInput = $("sheet-image-height");
      wInput?.addEventListener("input", () => {
        const w = Number(wInput.value);
        if (kit.checked("sheet-image-lock-ratio") && w > 0 && hInput) {
          hInput.value = String(Math.max(1, Math.round(w / dialogAspect)));
        }
      });
      hInput?.addEventListener("input", () => {
        const h = Number(hInput.value);
        if (kit.checked("sheet-image-lock-ratio") && h > 0 && wInput) {
          wInput.value = String(Math.max(1, Math.round(h * dialogAspect)));
        }
      });
      $("btn-sheet-image-reset")?.addEventListener("click", () => {
        const img = layoutApi.imagesOf(getActiveSheetName()).find((im) => im.id === editingImageId);
        if (img) syncDialogSizeFields(img.naturalWidthPx, img.naturalHeightPx);
      });
      $("btn-sheet-image-apply")?.addEventListener("click", () => {
        void applyImageDialog().catch(() => {});
      });
      $("btn-sheet-image-delete")?.addEventListener("click", () => {
        const id = editingImageId;
        editingImageId = null;
        kit.closeDialog("sheet-image-dialog");
        if (id) void deleteImageById(id).catch(() => {});
      });
      layoutApi.onChange(syncLayer);
    }

    const actions = {
      "insert-picture": () => {
        ensureBound();
        if (isProtected(getActiveSheetName(), true)) return Promise.resolve(false);
        $("sheet-image-file-input")?.click?.();
        return Promise.resolve(true);
      },
    };

    const api = {
      bind: ensureBound,
      actions,
      insertPictureFromBytes,
      openImageDialog,
      deleteImageById,
      syncLayer,
      anchorFromContentRect,
      getSelectedImageId: () => selectedImageId,
      selectImageById: selectCard,
    };
    host.__EASYOFFICE_SHEET_MEDIA__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_SHEET_MEDIA_UI__ = createSheetMediaUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
