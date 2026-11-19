(function (host) {
  "use strict";

  // Document inline picture insertion, selection, handle resize, size dialog and deletion.
  // UI labels follow the reference editor's Korean locale (`그림`, `크기`, `너비`, `높이`, `비율 유지`,
  // `원래 크기로`, `개체 지우기`; MIT, Copyright (c) 2025-2026 Edward Kim, see THIRD_PARTY_NOTICES.md).
  // The resize frame `#doc-image-resizer` lives in `.doc-scroll-container` outside `#doc-editor` so
  // editor DOM serialization and Undo snapshots never carry handle elements.

  const EMU_PER_PX = 9525;
  const EMU_PER_MM = 36000;
  const EMU_PER_TWIP = 635;
  const BODY_MAX_WIDTH_EMU = 5731510;
  const CELL_DEFAULT_MAX_WIDTH_EMU = 2286000;
  const MIN_RESIZE_PX = 16;
  const MAX_RESIZE_PX = 602;
  const MIN_DIALOG_MM = 1;
  const MAX_DIALOG_MM = 500;
  const MAX_SINGLE_IMAGE_BYTES = 32 * 1024 * 1024;
  const MAX_EDITOR_IMAGES = 256;
  const MAX_EDITOR_IMAGE_BYTES = 128 * 1024 * 1024;
  const MAX_DIMENSION_PX = 65535;
  const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  const IHDR_SIG = [0x49, 0x48, 0x44, 0x52];
  const CMD_INSERT_IMAGE = "insert-image";
  const MSG_INSERT_DONE = "그림 완료";
  const MSG_DELETE_DONE = "개체 지우기 완료";

  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const round1 = (v) => Math.round(v * 10) / 10;
  const readU32BE = (u8, at) => (((u8[at] << 24) >>> 0) + (u8[at + 1] << 16) + (u8[at + 2] << 8) + u8[at + 3]) >>> 0;

  function parsePngHeader(u8) {
    if (u8.length < 24 || readU32BE(u8, 8) !== 13) return null;
    for (let i = 0; i < PNG_MAGIC.length; i += 1) if (u8[i] !== PNG_MAGIC[i]) return null;
    for (let i = 0; i < IHDR_SIG.length; i += 1) if (u8[12 + i] !== IHDR_SIG[i]) return null;
    const widthPx = readU32BE(u8, 16);
    const heightPx = readU32BE(u8, 20);
    if (widthPx < 1 || heightPx < 1 || widthPx > MAX_DIMENSION_PX || heightPx > MAX_DIMENSION_PX) return null;
    return { format: "png", widthPx, heightPx };
  }

  function parseJpegHeader(u8) {
    if (u8.length < 4 || u8[0] !== 0xff || u8[1] !== 0xd8 || u8[2] !== 0xff) return null;
    let pos = 2;
    while (pos < u8.length) {
      while (pos < u8.length && u8[pos] === 0xff) pos += 1;
      if (pos >= u8.length) return null;
      const marker = u8[pos++];
      if (marker === 0xd9 || marker === 0xda) return null;
      if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) continue;
      if (pos + 2 > u8.length) return null;
      const segLen = (u8[pos] << 8) | u8[pos + 1];
      if (segLen < 2 || pos + segLen > u8.length) return null;
      if (marker === 0xc0 || marker === 0xc1 || marker === 0xc2) {
        if (segLen < 7) return null;
        const heightPx = (u8[pos + 3] << 8) | u8[pos + 4];
        const widthPx = (u8[pos + 5] << 8) | u8[pos + 6];
        return widthPx >= 1 && heightPx >= 1 ? { format: "jpg", widthPx, heightPx } : null;
      }
      pos += segLen;
    }
    return null;
  }

  const parseImageHeader = (u8) => parsePngHeader(u8) ?? parseJpegHeader(u8);

  function altFromHint(mimeOrName) {
    const raw = String(mimeOrName || "").trim();
    if (!raw || raw.includes("/")) return "";
    return raw.split(/[\\/]/).pop().trim().replace(/\.(?:png|jpe?g)$/i, "");
  }

  function isTrailingMetaNode(node) {
    if (!node || node.nodeType !== 1) return false;
    return String(node.tagName || "").toUpperCase() === "FOOTER" || node.id === "hwp-manuscript-preview" || node.dataset?.eoHf !== undefined;
  }

  function topBodyBlock(root, range) {
    let node = range?.startContainer ?? null;
    if (!node || !root.contains(node) || node === root) return null;
    while (node.parentNode && node.parentNode !== root) node = node.parentNode;
    return node.parentNode === root && !isTrailingMetaNode(node) ? node : null;
  }

  const firstMetaChild = (root) => Array.from(root.children || []).find(isTrailingMetaNode) ?? null;

  function emptyParagraph(document) {
    const p = document.createElement("p");
    p.appendChild(document.createElement("br"));
    return p;
  }

  function isLonePlaceholderCell(cell) {
    if (String(cell.textContent || "").trim() !== "" || cell.querySelector?.("img")) return false;
    return Array.from(cell.childNodes || []).every(
      (n) => (n.nodeType === 3 && !String(n.nodeValue || "").trim()) || (n.nodeType === 1 && String(n.tagName || "").toUpperCase() === "BR"));
  }

  function initialSizeEmu(widthPx, heightPx, maxWidthEmu) {
    const rawW = widthPx * EMU_PER_PX;
    const rawH = heightPx * EMU_PER_PX;
    if (rawW <= maxWidthEmu) return { widthEmu: rawW, heightEmu: rawH };
    const scale = maxWidthEmu / rawW;
    return { widthEmu: Math.max(EMU_PER_PX, Math.round(rawW * scale)), heightEmu: Math.max(EMU_PER_PX, Math.round(rawH * scale)) };
  }

  function applyImageSizeEmu(img, widthEmu, heightEmu) {
    const w = Math.max(EMU_PER_PX, Math.round(widthEmu));
    const h = Math.max(EMU_PER_PX, Math.round(heightEmu));
    img.dataset.eoImageWidthEmu = String(w);
    img.dataset.eoImageHeightEmu = String(h);
    img.style.width = Math.max(1, Math.round(w / EMU_PER_PX)) + "px";
    img.style.height = Math.max(1, Math.round(h / EMU_PER_PX)) + "px";
  }

  function createDocImage(deps) {
    const { document, env, getRange, withUndo, setFeedback, captureWorkspaceSnapshot, commitDeferredUndoSnapshot, saveDocState, getActiveModule } = deps;
    const grid = host.__EASYOFFICE_DOC_TABLE_GRID__;
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    let selectedImg = null;
    let dialogTarget = null;
    let dialogAspect = 1;
    let openedWMm = 0;
    let openedHMm = 0;
    let exactWEmu = 0;
    let exactHEmu = 0;

    const $ = (id) => document.getElementById(id);
    const editor = () => $("doc-editor");
    const resizerEl = () => $("doc-image-resizer");

    function notifyMutated() {
      saveDocState?.();
      const root = editor();
      if (root && typeof host.Event === "function") root.dispatchEvent(new host.Event("eo-doc-mutated", { bubbles: true }));
    }

    function cellMaxWidthEmu(table, cell) {
      const cols = grid.colsOf(grid.findColgroup(table));
      const pos = cols.length > 0 ? grid.layoutOf(table).at.get(cell) : null;
      if (pos) {
        let sumTwips = 0;
        for (let c = pos.col; c < pos.col + pos.colSpan; c += 1) sumTwips += Number(cols[c]?.dataset?.eoColTwips) || 0;
        if (sumTwips > 0) return Math.max(MIN_RESIZE_PX * EMU_PER_PX, sumTwips * EMU_PER_TWIP);
      }
      return CELL_DEFAULT_MAX_WIDTH_EMU;
    }

    function checkQuotaAndHeader(root, u8) {
      if (u8.length === 0) return { error: "그림을 넣지 못했습니다: 빈 파일입니다" };
      if (u8.length > MAX_SINGLE_IMAGE_BYTES) return { error: "그림을 넣지 못했습니다: 최대 32 MiB까지 넣을 수 있습니다" };
      const existing = root.querySelectorAll("img.doc-inline-image");
      if (existing.length >= MAX_EDITOR_IMAGES) return { error: "그림을 넣지 못했습니다: 최대 256개까지 넣을 수 있습니다" };
      let total = 0;
      for (const im of existing) total += utils.getDocImage(im.dataset?.eoImageId)?.bytesLen || 0;
      if (total + u8.length > MAX_EDITOR_IMAGE_BYTES) return { error: "그림을 넣지 못했습니다: 전체 그림 용량은 128 MiB를 넘을 수 없습니다" };
      const header = parseImageHeader(u8);
      return header ? { header } : { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
    }

    function syncResizerBox() {
      const box = resizerEl();
      const root = editor();
      if (!box) return;
      if (!selectedImg || !root?.contains(selectedImg)) {
        selectedImg = null;
        box.hidden = true;
        return;
      }
      const container = box.parentElement;
      const imgRect = selectedImg.getBoundingClientRect?.() || { left: 0, top: 0 };
      const conRect = container?.getBoundingClientRect?.() || { left: 0, top: 0 };
      const wPx = Number.parseFloat(selectedImg.style.width) || Math.max(MIN_RESIZE_PX, Math.round((Number(selectedImg.dataset.eoImageWidthEmu) || 952500) / EMU_PER_PX));
      const hPx = Number.parseFloat(selectedImg.style.height) || Math.max(MIN_RESIZE_PX, Math.round((Number(selectedImg.dataset.eoImageHeightEmu) || 952500) / EMU_PER_PX));
      box.hidden = false;
      box.style.left = Math.round(imgRect.left - conRect.left + (container?.scrollLeft || 0)) + "px";
      box.style.top = Math.round(imgRect.top - conRect.top + (container?.scrollTop || 0)) + "px";
      box.style.width = Math.round(wPx) + "px";
      box.style.height = Math.round(hPx) + "px";
    }

    const selectImage = (img) => { selectedImg = img; syncResizerBox(); };
    const clearSelection = () => { selectedImg = null; const box = resizerEl(); if (box) box.hidden = true; };

    function insertImageBytes(bytes, mimeOrName) {
      const root = editor();
      if (!root || !utils) return { changed: false, error: "편집 영역을 찾을 수 없습니다" };
      const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const checked = checkQuotaAndHeader(root, u8);
      if (checked.error) {
        setFeedback?.(checked.error);
        return { changed: false, error: checked.error };
      }
      const { format, widthPx, heightPx } = checked.header;
      const range = getRange?.() ?? null;
      const tbl = range ? grid.tableAt(root, range.startContainer) : null;
      const cell = tbl?.classList?.contains("doc-inline-table") ? grid.cellIn(tbl, range.startContainer, range.startOffset) : null;
      const { widthEmu, heightEmu } = initialSizeEmu(widthPx, heightPx, cell ? cellMaxWidthEmu(tbl, cell) : BODY_MAX_WIDTH_EMU);
      const alt = altFromHint(mimeOrName);
      const id = utils.registerDocImage(utils.bytesToBase64(u8), format);
      const url = utils.docImageUrl(id);

      const res = withUndo(() => {
        const img = document.createElement("img");
        img.className = "doc-inline-image";
        img.setAttribute("contenteditable", "false");
        if (url) img.setAttribute("src", url);
        img.setAttribute("alt", alt);
        if (alt) img.setAttribute("title", alt);
        Object.assign(img.dataset, {
          eoImageId: id, eoImageFormat: format, eoImageAlt: alt,
          eoImageNaturalWidthPx: String(widthPx), eoImageNaturalHeightPx: String(heightPx),
        });
        applyImageSizeEmu(img, widthEmu, heightEmu);
        if (cell) {
          if (isLonePlaceholderCell(cell)) cell.replaceChildren();
          cell.appendChild(img);
        } else {
          const blk = topBodyBlock(root, range);
          root.insertBefore(img, (blk ? blk.nextSibling : firstMetaChild(root)) ?? null);
          if (!img.nextSibling || isTrailingMetaNode(img.nextSibling)) {
            root.insertBefore(emptyParagraph(document), img.nextSibling ?? null);
          }
        }
        return { changed: true, img };
      });
      if (res.changed) {
        selectImage(res.img);
        setFeedback?.(MSG_INSERT_DONE);
      }
      return res;
    }

    function naturalDimensionsPx(img) {
      const natW = Number(img.dataset.eoImageNaturalWidthPx) || 0;
      const natH = Number(img.dataset.eoImageNaturalHeightPx) || 0;
      if (natW > 0 && natH > 0) return { widthPx: natW, heightPx: natH };
      const stored = utils?.getDocImage(img.dataset.eoImageId);
      return stored ? parseImageHeader(utils.base64ToBytes(stored.base64)) : null;
    }

    function syncDialogFields(wEmu, hEmu) {
      dialogAspect = wEmu / Math.max(1, hEmu);
      exactWEmu = wEmu;
      exactHEmu = hEmu;
      openedWMm = round1(wEmu / EMU_PER_MM);
      openedHMm = round1(hEmu / EMU_PER_MM);
      const wInput = $("doc-image-width-mm");
      const hInput = $("doc-image-height-mm");
      if (wInput) wInput.value = String(openedWMm);
      if (hInput) hInput.value = String(openedHMm);
    }

    function openSizeDialog(img) {
      const dlg = $("doc-image-size-dialog");
      const keep = $("doc-image-keep-ratio");
      if (!dlg || !keep) return;
      dialogTarget = img;
      const curW = Number(img.dataset.eoImageWidthEmu) || (Number.parseFloat(img.style.width) || 100) * EMU_PER_PX;
      const curH = Number(img.dataset.eoImageHeightEmu) || (Number.parseFloat(img.style.height) || 100) * EMU_PER_PX;
      syncDialogFields(curW, curH);
      keep.checked = true;
      if (!dlg.open) dlg.showModal?.();
    }

    function applySizeDialog() {
      const dlg = $("doc-image-size-dialog");
      const wInput = $("doc-image-width-mm");
      const hInput = $("doc-image-height-mm");
      const keep = $("doc-image-keep-ratio");
      if (!dialogTarget || !editor()?.contains(dialogTarget) || !wInput || !hInput) return;
      let wMm = Number(wInput.value);
      let hMm = Number(hInput.value);
      if (keep?.checked) {
        if (wMm !== openedWMm && hMm === openedHMm && wMm > 0) hMm = round1(wMm / dialogAspect);
        else if (hMm !== openedHMm && wMm === openedWMm && hMm > 0) wMm = round1(hMm * dialogAspect);
      }
      if (!Number.isFinite(wMm) || !Number.isFinite(hMm) || wMm < MIN_DIALOG_MM || wMm > MAX_DIALOG_MM || hMm < MIN_DIALOG_MM || hMm > MAX_DIALOG_MM) {
        setFeedback?.("그림 크기는 1mm 이상 500mm 이하여야 합니다");
        return;
      }
      const nextWEmu = wMm === openedWMm && exactWEmu > 0 ? exactWEmu : wMm * EMU_PER_MM;
      const nextHEmu = hMm === openedHMm && exactHEmu > 0 ? exactHEmu : hMm * EMU_PER_MM;
      const target = dialogTarget;
      withUndo(() => { applyImageSizeEmu(target, nextWEmu, nextHEmu); return { changed: true }; });
      dlg?.close?.();
      selectImage(target);
      setFeedback?.(MSG_INSERT_DONE);
    }

    function startHandleDrag(ev, dir) {
      if (!selectedImg || !editor()?.contains(selectedImg)) return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      const target = selectedImg;
      const preSnap = captureWorkspaceSnapshot?.(false) ?? null;
      const startW = clamp(Number.parseFloat(target.style.width) || Math.round((Number(target.dataset.eoImageWidthEmu) || 952500) / EMU_PER_PX), MIN_RESIZE_PX, MAX_RESIZE_PX);
      const startH = clamp(Number.parseFloat(target.style.height) || Math.round((Number(target.dataset.eoImageHeightEmu) || 952500) / EMU_PER_PX), MIN_RESIZE_PX, MAX_RESIZE_PX);
      const ratio = startW / Math.max(1, startH);
      const startX = Number(ev.clientX) || 0;
      const startY = Number(ev.clientY) || 0;
      let nextW = startW;
      let nextH = startH;

      const onMove = (moveEv) => {
        const dx = (Number(moveEv.clientX) || 0) - startX;
        const dy = (Number(moveEv.clientY) || 0) - startY;
        if (dir === "e") {
          nextW = clamp(Math.round(startW + dx), MIN_RESIZE_PX, MAX_RESIZE_PX);
          nextH = startH;
        } else if (dir === "s") {
          nextW = startW;
          nextH = clamp(Math.round(startH + dy), MIN_RESIZE_PX, MAX_RESIZE_PX);
        } else {
          let rawW = clamp(startW + dx * (dir.includes("w") ? -1 : 1), MIN_RESIZE_PX, MAX_RESIZE_PX);
          let rawH = clamp(rawW / ratio, MIN_RESIZE_PX, MAX_RESIZE_PX);
          rawW = clamp(rawH * ratio, MIN_RESIZE_PX, MAX_RESIZE_PX);
          nextW = Math.round(rawW);
          nextH = Math.round(rawH);
        }
        applyImageSizeEmu(target, nextW * EMU_PER_PX, nextH * EMU_PER_PX);
        syncResizerBox();
      };

      const onUp = () => {
        document.removeEventListener("pointermove", onMove);
        document.removeEventListener("pointerup", onUp);
        if (nextW !== startW || nextH !== startH) {
          notifyMutated();
          if (preSnap) commitDeferredUndoSnapshot?.(preSnap);
          setFeedback?.(MSG_INSERT_DONE);
        }
      };
      document.addEventListener("pointermove", onMove);
      document.addEventListener("pointerup", onUp);
    }

    function deleteSelectedImage(ev) {
      if ((ev.key !== "Delete" && ev.key !== "Backspace") || (getActiveModule && getActiveModule() !== "DOC")) return;
      const root = editor();
      if (!selectedImg || !root?.contains(selectedImg) || document.querySelector("dialog[open]")) return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      const target = selectedImg;
      withUndo(() => {
        const parent = target.parentElement;
        target.remove();
        const pTag = String(parent?.tagName || "").toUpperCase();
        if ((pTag === "TD" || pTag === "TH") && parent.childNodes.length === 0) parent.appendChild(document.createElement("br"));
        else if (parent === root && !Array.from(root.children).some((el) => !isTrailingMetaNode(el))) {
          root.insertBefore(emptyParagraph(document), firstMetaChild(root));
        }
        utils?.releaseDocImageUrls?.(root);
        return { changed: true };
      });
      clearSelection();
      setFeedback?.(MSG_DELETE_DONE);
    }

    function bind() {
      const input = $("doc-image-input");
      input?.addEventListener("change", () => {
        const file = input.files?.[0];
        input.value = "";
        if (!file) return;
        void Promise.resolve(file.arrayBuffer()).then((buf) => insertImageBytes(new Uint8Array(buf), file.name)).catch(() => {
          setFeedback?.("그림 파일을 읽지 못했습니다");
        });
      });
      const root = editor();
      root?.addEventListener("click", (ev) => {
        const img = ev.target?.closest?.("img.doc-inline-image");
        if (img && root.contains(img)) selectImage(img);
        else clearSelection();
      });
      root?.addEventListener("dblclick", (ev) => {
        const img = ev.target?.closest?.("img.doc-inline-image");
        if (img && root.contains(img)) { selectImage(img); openSizeDialog(img); }
      });
      root?.addEventListener("eo-doc-mutated", syncResizerBox);
      document.addEventListener("mousedown", (ev) => {
        if (selectedImg && !ev.target?.closest?.("img.doc-inline-image, #doc-image-resizer, #doc-image-size-dialog")) clearSelection();
      });
      document.addEventListener("keydown", deleteSelectedImage);
      env?.addEventListener?.("resize", syncResizerBox);
      document.querySelector(".doc-scroll-container")?.addEventListener("scroll", syncResizerBox);
      resizerEl()?.querySelectorAll?.("[data-resize-dir]").forEach((h) => h.addEventListener("pointerdown", (ev) => startHandleDrag(ev, h.dataset.resizeDir)));

      const dlg = $("doc-image-size-dialog");
      const wInput = $("doc-image-width-mm");
      const hInput = $("doc-image-height-mm");
      const keep = $("doc-image-keep-ratio");
      wInput?.addEventListener("input", () => {
        const w = Number(wInput.value);
        if (keep?.checked && w > 0) hInput.value = String(Math.max(0.1, round1(w / dialogAspect)));
      });
      hInput?.addEventListener("input", () => {
        const h = Number(hInput.value);
        if (keep?.checked && h > 0) wInput.value = String(Math.max(0.1, round1(h * dialogAspect)));
      });
      $("btn-doc-image-reset-size")?.addEventListener("click", () => {
        const nat = dialogTarget ? naturalDimensionsPx(dialogTarget) : null;
        if (nat) syncDialogFields(nat.widthPx * EMU_PER_PX, nat.heightPx * EMU_PER_PX);
      });
      $("btn-doc-image-size-ok")?.addEventListener("click", applySizeDialog);
      dlg?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => btn.addEventListener("click", () => dlg.close?.()));
    }

    const handles = (cmd) => cmd === CMD_INSERT_IMAGE;
    const run = (cmd) => (cmd === CMD_INSERT_IMAGE ? ($("doc-image-input")?.click(), { changed: false, openedPicker: true }) : { changed: false });

    const api = { bind, handles, run, insertImageBytes, selectImage, clearSelection, syncResizer: syncResizerBox, parseImageHeader };
    host.__EASYOFFICE_DOC_IMAGE_API__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_DOC_IMAGE__ = createDocImage;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
