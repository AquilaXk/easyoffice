(function (host) {
  "use strict";

  function syncDatasetFlag(el, key, val) {
    if (!el || typeof val !== "string") return;
    if (val) el.dataset[key] = val;
    else delete el.dataset[key];
  }

  function captureDocLayoutStyle(editor) {
    if (!editor) {
      return {
        columnCount: "",
        maxWidth: "",
        padding: "",
        minHeight: "",
        fontFamily: "",
        fontSize: "",
        lineHeight: "",
        textAlign: "",
        paperSize: "",
        styleRole: "",
        showCtrlMarks: "",
        showParaMarks: "true",
        manuscriptGrid: false,
      };
    }
    return {
      columnCount: editor.style.columnCount || "",
      maxWidth: editor.style.maxWidth || "",
      padding: editor.style.padding || "",
      minHeight: editor.style.minHeight || "",
      fontFamily: editor.style.fontFamily || "",
      fontSize: editor.style.fontSize || "",
      lineHeight: editor.style.lineHeight || "",
      textAlign: editor.style.textAlign || "",
      paperSize: editor.dataset.paperSize || "",
      styleRole: editor.dataset.styleRole || "",
      showCtrlMarks: editor.dataset.showCtrlMarks || "",
      showParaMarks: editor.dataset.showParaMarks || "",
      manuscriptGrid: editor.classList.contains("manuscript-grid"),
    };
  }

  function applyDocLayoutStyle(editor, layoutStyle) {
    if (!editor) return;
    const ls = layoutStyle || {};
    editor.style.columnCount = ls.columnCount || "";
    editor.style.maxWidth = ls.maxWidth || "";
    editor.style.padding = ls.padding || "";
    editor.style.minHeight = ls.minHeight || "";
    editor.style.fontFamily = ls.fontFamily || "";
    editor.style.fontSize = ls.fontSize || "";
    editor.style.lineHeight = ls.lineHeight || "";
    editor.style.textAlign = ls.textAlign || "";
    [
      "paperSize",
      "styleRole",
      "showCtrlMarks",
      "showParaMarks",
    ].forEach((k) => {
      if (ls[k]) {
        editor.dataset[k] = ls[k];
      } else {
        delete editor.dataset[k];
      }
    });
    editor.classList.toggle("manuscript-grid", Boolean(ls.manuscriptGrid));
  }

  const { serializeDomChildren, restoreDomChildren } = host.__EASYOFFICE_SHEET_UTILS__;

  function insertBeforeDocFooter(docEditor, el) {
    const footer = docEditor.querySelector("footer.doc-page-footer");
    if (footer && footer.parentElement === docEditor) {
      footer.before(el);
    } else {
      docEditor.appendChild(el);
    }
  }

  // Picking an entry closes the dialog first, so the focus the action sets is not undone by the
  // dialog handing focus back to its opener.
  function activateOutlineItem(outlineList, li, item) {
    for (const el of outlineList.querySelectorAll(".outline-item")) {
      el.classList.remove("active");
      el.setAttribute("aria-selected", "false");
      el.tabIndex = -1;
    }
    li.classList.add("active");
    li.setAttribute("aria-selected", "true");
    li.tabIndex = 0;
    const dialog = outlineList.closest?.("dialog");
    if (dialog?.open) dialog.close();
    if (typeof item.action === "function") item.action();
  }

  const OUTLINE_KEY_STEP = { ArrowDown: 1, ArrowUp: -1 };

  // Arrow keys move between entries (roving tabindex), Home/End jump, Enter/Space pick.
  function onOutlineKeydown(event, outlineList) {
    const rows = Array.from(outlineList.querySelectorAll(".outline-item"));
    const at = rows.indexOf(event.target);
    if (at < 0) return;
    let next = null;
    if (OUTLINE_KEY_STEP[event.key]) next = rows[Math.max(0, Math.min(rows.length - 1, at + OUTLINE_KEY_STEP[event.key]))];
    else if (event.key === "Home") next = rows[0];
    else if (event.key === "End") next = rows[rows.length - 1];
    else if (event.key === "Enter" || event.key === " ") return void (event.preventDefault(), event.target.click());
    else return;
    event.preventDefault();
    for (const row of rows) row.tabIndex = row === next ? 0 : -1;
    next.focus();
  }

  function createWorkspacePanels(deps) {
    const {
      document,
      docTabStore,
      sheetDataStore,
      slideDeckState,
      defaultHwpEditorNodes,
      isExcludedDocChromeElement,
      sheetController,
      getActiveModule,
      getActiveSheetName,
      getActiveSlideIndex,
      setActiveSlideIndex,
      getPdfRotationDeg,
      onSlideRendered,
    } = deps;

    let activeDocBlockEl = null;

    function syncDocStyleBarFromTarget(targetEl) {
      // With a live selection the character state (run model) wins over the block fallback below.
      if (deps.getDocRuns?.()?.syncToolbar()) return;
      const docEditor = document.getElementById("doc-editor");
      const boldBtn = document.getElementById("btn-stylebar-bold");
      const italicBtn = document.getElementById("btn-stylebar-italic");
      const underBtn = document.getElementById("btn-stylebar-underline");
      const sizeBadge = document.getElementById("doc-style-size-badge");
      const sizeInput = document.getElementById("doc-font-size-input");
      const fw = targetEl?.style?.fontWeight || "";
      const isBold = fw === "600" || fw === "700" || fw === "bold" || Number(fw) >= 600;
      if (boldBtn) boldBtn.setAttribute("aria-pressed", isBold ? "true" : "false");
      if (italicBtn) italicBtn.setAttribute("aria-pressed", targetEl?.style?.fontStyle === "italic" ? "true" : "false");
      if (underBtn) underBtn.setAttribute("aria-pressed", targetEl?.style?.textDecoration === "underline" ? "true" : "false");
      const rawSize = targetEl?.style?.fontSize || docEditor?.style?.fontSize || "11.0pt";
      const parsedNum = Number.parseFloat(rawSize);
      const formattedNum = Number.isFinite(parsedNum) ? parsedNum.toFixed(1) : "11.0";
      if (sizeInput) sizeInput.value = formattedNum;
      if (sizeBadge) {
        sizeBadge.textContent = rawSize.endsWith("px") ? rawSize : formattedNum + " pt";
      }
    }

    function setActiveDocBlock(el) {
      activeDocBlockEl = el;
      syncDocStyleBarFromTarget(el);
    }

    function saveDocTabStateForModule(mod) {
      if (mod !== "DOC" && mod !== "HWP") return;
      const editor = document.getElementById("doc-editor");
      if (!editor) return;
      docTabStore[mod] = {
        editorNodes: serializeDomChildren(editor),
        layoutStyle: captureDocLayoutStyle(editor),
      };
    }

    function saveActiveDocTabState() {
      saveDocTabStateForModule(getActiveModule());
    }

    function ensureDefaultDocTabStore() {
      const editor = document.getElementById("doc-editor");
      if (!docTabStore.DOC && editor) {
        docTabStore.DOC = {
          editorNodes: serializeDomChildren(editor),
          layoutStyle: captureDocLayoutStyle(editor),
        };
      }
      if (!docTabStore.HWP) {
        docTabStore.HWP = {
          editorNodes: structuredClone(defaultHwpEditorNodes || []),
          layoutStyle: captureDocLayoutStyle(null),
        };
      }
    }

    function computeDocTableSummary() {
      const numCells = document.querySelectorAll(
        "#doc-table-body tr:not(#doc-table-sum-row) [data-doc-num]"
      );
      let total = 0;
      let count = 0;
      numCells.forEach((cell) => {
        const textCandidate = (cell.textContent || "").trim();
        const raw = (textCandidate || cell.dataset.docNum || "0").replaceAll(",", "");
        const val = Number.parseFloat(raw);
        if (!Number.isNaN(val)) {
          cell.dataset.docNum = String(val);
          total += val;
          count += 1;
        } else if (textCandidate !== "") {
          cell.dataset.docNum = "0";
        }
      });
      const avg = count > 0 ? Math.round((total / count) * 100) / 100 : 0;
      const sumCell = document.getElementById("doc-table-sum-cell");
      if (sumCell) {
        sumCell.textContent = String(total);
        sumCell.dataset.docNum = String(total);
        sumCell.dataset.docAvg = String(avg);
        sumCell.dataset.docCount = String(count);
      }
      const statusEl =
        document.getElementById("doc-table-summary") ||
        document.getElementById("status-message") ||
        document.getElementById("status-command-feedback");
      if (statusEl) {
        statusEl.textContent =
          "표 =SUM(ABOVE) 합계: " +
          total +
          "백만원 · 평균(AVERAGE): " +
          avg +
          "백만원 · 개수(COUNT): " +
          count +
          "건";
      }
    }

    function syncActiveSlideDomStylesToDeck() {
      const idx = getActiveSlideIndex();
      const slide = slideDeckState[idx];
      if (!slide) return;
      const titleZone = document.getElementById("slide-title-zone");
      const lowerZone = document.getElementById("slide-lower-zone");
      const stageBox = document.getElementById("slide-stage-box");
      if (titleZone) {
        slide.titleStyle = {
          fontWeight: titleZone.style.fontWeight || "",
          fontStyle: titleZone.style.fontStyle || "",
          textDecoration: titleZone.style.textDecoration || "",
          color: titleZone.style.color || "",
          fontSize: titleZone.style.fontSize || "",
        };
      }
      if (lowerZone) {
        slide.lowerColumnCount = lowerZone.style.columnCount || "";
        const extraHolder = document.createElement("div");
        const bodyParts = [];
        Array.from(lowerZone.childNodes || []).forEach((node) => {
          if (node.nodeType === 1 && node.dataset?.slideObject) {
            extraHolder.appendChild(node.cloneNode(true));
          } else {
            const txt = node.textContent || "";
            if (node.nodeType === 3 ? txt.trim() : txt) bodyParts.push(txt);
          }
        });
        slide.lower = bodyParts.join("\n");
        slide.insertedNodes = serializeDomChildren(extraHolder);
      }
      if (stageBox) {
        slide.stagePadding = stageBox.style.padding || "";
        slide.stageOutline = stageBox.style.outline || "";
        slide.stageLayoutState = stageBox.dataset.layoutState || "";
        slide.presenterMode = stageBox.dataset.presenterMode || "";
      }
    }

    function applySlideDomStyles(slide) {
      const titleZone = document.getElementById("slide-title-zone");
      const lowerZone = document.getElementById("slide-lower-zone");
      const stageBox = document.getElementById("slide-stage-box");
      const ts = slide?.titleStyle || {};
      if (titleZone) {
        titleZone.style.fontWeight = ts.fontWeight || "";
        titleZone.style.fontStyle = ts.fontStyle || "";
        titleZone.style.textDecoration = ts.textDecoration || "";
        titleZone.style.color = ts.color || "";
        titleZone.style.fontSize = ts.fontSize || "";
      }
      if (lowerZone) {
        lowerZone.style.columnCount = slide?.lowerColumnCount || "";
      }
      if (stageBox) {
        stageBox.style.padding = slide?.stagePadding || "";
        stageBox.style.outline = slide?.stageOutline || "";
        if (slide?.stageLayoutState) {
          stageBox.dataset.layoutState = slide.stageLayoutState;
        } else {
          delete stageBox.dataset.layoutState;
        }
        if (slide?.presenterMode) {
          stageBox.dataset.presenterMode = slide.presenterMode;
        } else {
          delete stageBox.dataset.presenterMode;
        }
      }
    }

    function switchToSlideIndex(idx) {
      syncActiveSlideDomStylesToDeck();
      renderSlideDeck(idx);
    }

    function renderSlideDeck(requestedIdx) {
      const idx = Math.max(0, Math.min(Number(requestedIdx) || 0, slideDeckState.length - 1));
      setActiveSlideIndex(idx);
      const slide = slideDeckState[idx] || slideDeckState[0];
      const titleZone = document.getElementById("slide-title-zone");
      const lowerZone = document.getElementById("slide-lower-zone");
      const notesInput = document.getElementById("slide-presenter-notes");
      const transSelect = document.getElementById("slide-transition-select");
      const metaStatus = document.getElementById("slide-meta-status");

      if (titleZone) {
        const h2 = document.createElement("h2");
        h2.textContent = slide.title || "";
        titleZone.replaceChildren(h2);
      }
      if (lowerZone) {
        const p = document.createElement("p");
        p.textContent = slide.lower || "";
        lowerZone.replaceChildren(p);
        if (Array.isArray(slide.insertedNodes) && slide.insertedNodes.length > 0) {
          const extraHolder = document.createElement("div");
          restoreDomChildren(extraHolder, slide.insertedNodes, document);
          Array.from(extraHolder.childNodes).forEach((node) => {
            lowerZone.appendChild(node);
          });
        }
      }
      applySlideDomStyles(slide);
      if (notesInput) notesInput.value = slide.notes;
      if (transSelect) transSelect.value = slide.transition;
      if (metaStatus) {
        const transitionLabelMap = {
          fade: "페이드",
          push: "밀어내기",
          zoom: "확대",
          none: "없음",
        };
        const transitionLabel = transitionLabelMap[slide.transition] || slide.transition;
        metaStatus.textContent =
          "슬라이드 " + (idx + 1) + " / " + slideDeckState.length + " · 전환: " + transitionLabel;
      }

      onSlideRendered?.(idx);
      if (getActiveModule() === "SLIDE") {
        renderModuleOutline("SLIDE");
      }
    }

    function capturePdfStudioState() {
      const bodyParagraphs = Array.from(
        document.querySelectorAll("#pdf-page-card p:not(#pdf-form-status)")
      );
      const headingEl = document.querySelector("#pdf-page-card h3");
      const card = document.getElementById("pdf-page-card");
      return {
        stampValue: document.getElementById("pdf-stamp-select")?.value ?? "승인필",
        stampBadge: document.getElementById("pdf-stamp-badge")?.textContent ?? "승인필",
        watermarkInput: document.getElementById("pdf-watermark-input")?.value ?? "",
        watermarkLayer: document.getElementById("pdf-watermark-layer")?.textContent ?? "",
        applicant: document.getElementById("pdf-form-applicant")?.value ?? "",
        dept: document.getElementById("pdf-form-dept")?.value ?? "",
        highlightInput: document.getElementById("pdf-highlight-input")?.value ?? "",
        highlightKind: document.getElementById("pdf-highlight-kind")?.value,
        highlightColor: document.getElementById("pdf-highlight-color")?.value,
        cardHeadingNodes: headingEl ? serializeDomChildren(headingEl) : [],
        cardBodyParagraphsNodes: bodyParagraphs.map((p) => serializeDomChildren(p)),
        formStatus: document.getElementById("pdf-form-status")?.textContent ?? "",
        highlightText: card?.querySelector(".pdf-highlight-badge")?.textContent ?? "",
        splitMergeText: card?.querySelector(".pdf-splitmerge-badge")?.textContent ?? "",
      };
    }

    // [element id, property on that element, key in the saved pdfStudio state]
    const PDF_STUDIO_FIELD_RESTORES = [
      ["pdf-stamp-select", "value", "stampValue"],
      ["pdf-stamp-badge", "textContent", "stampBadge"],
      ["pdf-watermark-input", "value", "watermarkInput"],
      ["pdf-watermark-layer", "textContent", "watermarkLayer"],
      ["pdf-form-applicant", "value", "applicant"],
      ["pdf-form-dept", "value", "dept"],
      ["pdf-highlight-input", "value", "highlightInput"],
      ["pdf-highlight-kind", "value", "highlightKind"],
      ["pdf-highlight-color", "value", "highlightColor"],
    ];

    function restorePdfCardHeading(pdfStudio) {
      const cardHeading = document.querySelector("#pdf-page-card h3");
      if (!cardHeading) return;
      if (Array.isArray(pdfStudio.cardHeadingNodes)) {
        restoreDomChildren(cardHeading, pdfStudio.cardHeadingNodes, document);
      } else if (pdfStudio.cardHeading !== undefined) {
        cardHeading.textContent = pdfStudio.cardHeading;
      }
    }

    function restorePdfCardParagraphs(card, nodesPerParagraph) {
      const formStatus = document.getElementById("pdf-form-status");
      const existingPs = Array.from(card.querySelectorAll("p:not(#pdf-form-status)"));
      while (existingPs.length > nodesPerParagraph.length) {
        existingPs.pop().remove();
      }
      while (existingPs.length < nodesPerParagraph.length) {
        const newP = document.createElement("p");
        if (formStatus && formStatus.parentElement === card) formStatus.before(newP);
        else card.appendChild(newP);
        existingPs.push(newP);
      }
      nodesPerParagraph.forEach((nodes, idx) => restoreDomChildren(existingPs[idx], nodes, document));
    }

    function restorePdfCardBody(pdfStudio, card) {
      if (Array.isArray(pdfStudio.cardBodyParagraphsNodes) && card) {
        restorePdfCardParagraphs(card, pdfStudio.cardBodyParagraphsNodes);
      } else if (pdfStudio.cardBody !== undefined) {
        const firstBody = document.querySelector("#pdf-page-card p:not(#pdf-form-status)");
        if (firstBody) firstBody.textContent = pdfStudio.cardBody;
      }
    }

    // Adds, updates or removes an annotation badge so the card matches the saved text.
    function syncPdfCardBadge(card, spec, text) {
      const existing = card.querySelector("." + spec.cls);
      if (!text) {
        existing?.remove();
        return;
      }
      const badge = existing || document.createElement("span");
      badge.className = spec.cls;
      badge.dataset[spec.dataKey] = "true";
      badge.textContent = text;
      if (!existing) card.appendChild(badge);
    }

    function restorePdfStudioState(pdfStudio) {
      if (!pdfStudio) return;
      for (const [id, prop, key] of PDF_STUDIO_FIELD_RESTORES) {
        const el = document.getElementById(id);
        if (el && pdfStudio[key] !== undefined) el[prop] = pdfStudio[key];
      }
      restorePdfCardHeading(pdfStudio);
      const card = document.getElementById("pdf-page-card");
      restorePdfCardBody(pdfStudio, card);
      if (card) {
        syncPdfCardBadge(card, { cls: "pdf-highlight-badge", dataKey: "pdfHighlight" }, pdfStudio.highlightText);
        syncPdfCardBadge(card, { cls: "pdf-splitmerge-badge", dataKey: "pdfSplitmerge" }, pdfStudio.splitMergeText);
      }
      const formStatus = document.getElementById("pdf-form-status");
      if (formStatus && pdfStudio.formStatus !== undefined) formStatus.textContent = pdfStudio.formStatus;
    }

    // The highlight annotation lives in three hidden state fields (so snapshots, the PII scrub and the
    // save payload read one place); the dialog edits a copy and the command applies it.
    function openPdfHighlightDialog() {
      const field = (id) => document.getElementById(id);
      const sync = host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect;
      field("pdf-highlight-text").value = field("pdf-highlight-input")?.value ?? "";
      for (const [pick, state] of [["pdf-highlight-kind-select", "pdf-highlight-kind"], ["pdf-highlight-color-select", "pdf-highlight-color"]]) {
        const select = field(pick);
        if (!select) continue;
        select.value = field(state)?.value || select.querySelector("option")?.value || "";
        sync?.(select);
      }
      tools.openToolDialog("pdf-highlight-dialog");
      field("pdf-highlight-text").focus?.();
    }

    // Stores the confirmed phrase, kind and color (an empty phrase clears them) and shows the card badge.
    function applyPdfHighlight(highlight) {
      const text = String(highlight?.text ?? "").trim();
      const set = (id, value) => {
        const el = document.getElementById(id);
        if (el) el.value = value;
      };
      set("pdf-highlight-input", text);
      if (text) {
        set("pdf-highlight-kind", highlight.kind === "underline" ? "underline" : "highlight");
        set("pdf-highlight-color", highlight.color || "#FFFF00");
      }
      const card = document.getElementById("pdf-page-card");
      const kind = document.getElementById("pdf-highlight-kind")?.value;
      if (card) syncPdfCardBadge(card, { cls: "pdf-highlight-badge", dataKey: "pdfHighlight" }, text ? tools.describePdfHighlight(text, kind) : "");
    }

    function applyPdfStudioFormState() {
      const stampSel = document.getElementById("pdf-stamp-select");
      const wmInput = document.getElementById("pdf-watermark-input");
      const appInput = document.getElementById("pdf-form-applicant");
      const deptInput = document.getElementById("pdf-form-dept");
      const stampBadge = document.getElementById("pdf-stamp-badge");
      const wmLayer = document.getElementById("pdf-watermark-layer");
      const formStatus = document.getElementById("pdf-form-status");

      if (stampBadge && stampSel) stampBadge.textContent = stampSel.value;
      if (wmLayer && wmInput) wmLayer.textContent = wmInput.value;
      if (formStatus && appInput && deptInput) {
        formStatus.textContent =
          "양식 반영 완료 · 신청인: " +
          appInput.value +
          " / 부서: " +
          deptInput.value +
          " / 회전: " +
          getPdfRotationDeg() +
          "°";
      }
    }

    function syncSplitComparePane(stage) {
      if (!stage) return;
      const existing = stage.querySelector(".split-compare-pane");
      if (stage.dataset.splitView === "true") {
        const pane = existing || document.createElement("aside");
        pane.className = "split-compare-pane";
        pane.dataset.splitCompare = "true";
        const heading = document.createElement("h3");
        heading.textContent = "화면 분할 참조 보기";
        const body = document.createElement("p");
        const docHeading = document.querySelector("#doc-editor h1")?.textContent || "활성 문서";
        body.textContent = "참조 비교 영역 · " + docHeading.trim();
        pane.replaceChildren(heading, body);
        if (!existing) stage.appendChild(pane);
      } else if (existing) {
        existing.remove();
      }
    }

    function captureWorkspaceViewState() {
      const stage = document.querySelector(".canvas-stage");
      return {
        canvasZoom: stage?.style?.zoom || "",
        canvasSplit: stage?.dataset?.splitView || "",
      };
    }

    function restoreWorkspaceViewState(viewState) {
      if (!viewState) return;
      const stage = document.querySelector(".canvas-stage");
      if (stage) {
        stage.style.zoom = viewState.canvasZoom || "";
        syncDatasetFlag(stage, "splitView", viewState.canvasSplit || "");
        syncSplitComparePane(stage);
      }
      const zoomText = viewState.canvasZoom || "100%";
      const zoomLabel = document.getElementById("status-zoom-label");
      if (zoomLabel) zoomLabel.textContent = zoomText;
      const zoomSlider = document.getElementById("status-zoom-slider");
      if (zoomSlider) zoomSlider.value = String(Number.parseInt(zoomText, 10) || 100);
    }

    function focusOutlineItem(label, targetId, active) {
      return { label, action: () => document.getElementById(targetId)?.focus(), active };
    }

    function docOutlineItems() {
      const headings = document.querySelectorAll("#doc-editor h1, #doc-editor h2");
      return Array.from(headings).map((h, idx) => ({
        label: (h.textContent || "").trim(),
        action: () => h.scrollIntoView({ behavior: "smooth", block: "nearest" }),
        active: idx === 0,
      }));
    }

    function switchSheetTabQuietly(sName) {
      void sheetController.switchSheetTab(sName).catch(() => {});
    }

    function sheetOutlineItems() {
      const curSheet = getActiveSheetName();
      const tabLabels = new Map();
      for (const btn of document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")) {
        tabLabels.set(btn.dataset.sheetTab, (btn.textContent || "").trim());
      }
      const items = Object.keys(sheetDataStore).map((sName, idx) => ({
        label: idx + 1 + ". " + (tabLabels.get(sName) || sName),
        action: () => switchSheetTabQuietly(sName),
        active: curSheet === sName,
      }));
      items.push({
        label: items.length + 1 + ". 연간 합계 행 (F7)",
        action: () => sheetController.updateSheetSelectionAndAggregates("F7"),
        active: false,
      });
      return items;
    }

    function slideOutlineItems() {
      const curSlideIdx = getActiveSlideIndex();
      return slideDeckState.map((s, idx) => ({
        label: idx + 1 + ". " + s.title,
        action: () => switchToSlideIndex(idx),
        active: idx === curSlideIdx,
      }));
    }

    function buildOutlineItems(mod) {
      if (mod === "DOC" || mod === "HWP") return docOutlineItems();
      if (mod === "SHEET") return sheetOutlineItems();
      if (mod === "SLIDE") return slideOutlineItems();
      if (mod === "PDF") {
        return [
          focusOutlineItem("1. 결재 스탬프 및 워터마크", "pdf-stamp-select", true),
          focusOutlineItem("2. 신청인·부서 양식 필드", "pdf-form-applicant", false),
        ];
      }
      return [
        focusOutlineItem("1. 원본·대상 포맷 선택", "conv-src-fmt", true),
        focusOutlineItem("2. 컨테이너 무결성 검증", "btn-run-convert", false),
      ];
    }

    function renderModuleOutline(mod) {
      const outlineList = document.getElementById("outline-list");
      if (!outlineList) return;
      const items = buildOutlineItems(mod);
      outlineList.innerHTML = "";
      outlineList.setAttribute("role", "listbox");
      if (!outlineList.dataset.keysBound) {
        outlineList.dataset.keysBound = "true";
        outlineList.addEventListener("keydown", (event) => onOutlineKeydown(event, outlineList));
      }
      const tabStop = Math.max(0, items.findIndex((item) => item.active));
      for (const [index, item] of items.entries()) {
        const li = document.createElement("li");
        li.className = "outline-item" + (item.active ? " active" : "");
        li.textContent = item.label;
        li.setAttribute("role", "option");
        li.setAttribute("aria-selected", item.active ? "true" : "false");
        li.tabIndex = index === tabStop ? 0 : -1;
        li.addEventListener("click", () => activateOutlineItem(outlineList, li, item));
        outlineList.appendChild(li);
      }
    }

    const tools = host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__(deps, {
      saveActiveDocTabState,
      ensureDefaultDocTabStore,
      getFocusedDocBlock,
      computeDocTableSummary,
      syncActiveSlideDomStylesToDeck,
      renderSlideDeck,
      applyPdfStudioFormState,
    });

    function syncBtnPressed(id, on) {
      const btn = document.getElementById(id);
      if (!btn) return;
      btn.classList.toggle("active", on);
      btn.setAttribute("aria-pressed", on ? "true" : "false");
    }

    function setElField(id, prop, val) {
      const el = document.getElementById(id);
      if (el) el[prop] = val;
    }

    function restoreDocTabState(mod) {
      if (mod !== "DOC" && mod !== "HWP") return;
      ensureDefaultDocTabStore();
      const saved = docTabStore[mod];
      if (!saved) return;
      const editor = document.getElementById("doc-editor");
      if (editor) {
        restoreDomChildren(editor, saved.editorNodes || [], document);
        applyDocLayoutStyle(editor, saved.layoutStyle);
      }
      const roleVal = editor?.dataset?.styleRole || (mod === "HWP" ? "공문서 바탕글" : "바탕글");
      setElField("doc-style-role-badge", "textContent", roleVal);
      setElField("doc-style-select", "value", roleVal);
      const fontVal = editor?.style?.fontFamily || "휴먼명조";
      setElField("doc-font-select", "value", fontVal);
      setElField("doc-style-font-badge", "textContent", fontVal);
      // Without a selection the controls show the document-level values; a selection shows its paragraph (doc-runs-ui).
      const alignMode = editor?.style?.textAlign || "justify";
      ["left", "center", "right", "justify", "distribute", "divide"].forEach((m) =>
        syncBtnPressed("btn-align-" + m, m === alignMode)
      );
      const rootLine = Math.round(Number.parseFloat(editor?.style?.lineHeight || "") * 100);
      deps.getDocRuns?.()?.showLineSpacing(rootLine > 0 ? rootLine : 160);
      [
        ["btn-toggle-ctrl-marks", "showCtrlMarks"],
        ["btn-toggle-para-marks", "showParaMarks"],
      ].forEach(([id, key]) => syncBtnPressed(id, editor?.dataset?.[key] === "true"));
      const isGrid = Boolean(editor?.classList?.contains("manuscript-grid"));
      syncBtnPressed("btn-toggle-doc-grid", isGrid);
      document.getElementById("hwp-manuscript-preview")?.classList.toggle("manuscript-grid", isGrid);
      activeDocBlockEl = null;
      syncDocStyleBarFromTarget(editor ? getFocusedDocBlock(editor) : null);
      computeDocTableSummary();
      tools.computeSmartStats();
    }

    const STUDIO_CONTAINER_CLASSES = [
      "eo-hwp-header",
      "eo-doc-toc",
      "eo-doc-footnote",
      "eo-process-flow",
      "eo-criteria-panel",
    ];

    function isNonParagraphStudioContainer(el) {
      if (el?.nodeType !== 1) return false;
      if (isExcludedDocChromeElement(el)) return true;
      const tag = (el.tagName || "").toUpperCase();
      if (tag === "HEADER" || tag === "NAV" || tag === "ASIDE") return true;
      return STUDIO_CONTAINER_CLASSES.some((cls) => el.classList?.contains(cls));
    }

    function findEditableDocBlock(node, docEditor) {
      if (!node || !docEditor) return null;
      let cur = node.nodeType === 1 ? node : node.parentElement;
      let candidateLi = null;
      while (cur && cur !== docEditor) {
        if (!candidateLi && (cur.tagName || "").toUpperCase() === "LI") {
          candidateLi = cur;
        }
        if (cur.parentElement === docEditor) {
          if (cur.classList?.contains("eo-criteria-panel")) return candidateLi;
          return isNonParagraphStudioContainer(cur) ? null : candidateLi || cur;
        }
        cur = cur.parentElement;
      }
      return null;
    }

    function getFocusedDocBlock(docEditor) {
      if (!docEditor) return null;
      const sel = host.getSelection ? host.getSelection() : null;
      if (sel?.anchorNode) {
        const block = findEditableDocBlock(sel.anchorNode, docEditor);
        if (block) {
          activeDocBlockEl = block;
          return block;
        }
      }
      if (
        activeDocBlockEl &&
        docEditor.contains(activeDocBlockEl) &&
        !isNonParagraphStudioContainer(activeDocBlockEl)
      ) {
        return activeDocBlockEl;
      }
      const children = Array.from(docEditor.children || []);
      const firstBlock = children.find((el) => !isNonParagraphStudioContainer(el));
      return firstBlock || docEditor;
    }

    function appendDocTableRow() {
      const tbody = document.getElementById("doc-table-body");
      if (!tbody) return;
      const tr = document.createElement("tr");
      const tdDept = document.createElement("td");
      tdDept.textContent = "신규 사업부";
      const tdOwner = document.createElement("td");
      tdOwner.textContent = "담당 연구원";
      const tdAmount = document.createElement("td");
      tdAmount.dataset.docNum = "150";
      tdAmount.textContent = "150";
      tr.appendChild(tdDept);
      tr.appendChild(tdOwner);
      tr.appendChild(tdAmount);
      const sumRow = document.getElementById("doc-table-sum-row");
      if (sumRow) {
        sumRow.before(tr);
      } else {
        tbody.appendChild(tr);
      }
      computeDocTableSummary();
      tools.computeSmartStats();
    }

    const MAIL_MERGE_TOKEN_RE = /\{\{(?=([^}]+))\1\}\}/g;
    const MAIL_MERGE_TOKEN_TEST_RE = /\{\{(?=([^}]+))\1\}\}/;
    const MAIL_MERGE_VALUES = {
      "{{성명}}": "이수현",
      "{{기관명}}": "행정안전부",
      "{{일자}}": "2026-10-25",
    };

    function mergeKnownTokens(text) {
      return text.replace(MAIL_MERGE_TOKEN_RE, (m) => (Object.hasOwn(MAIL_MERGE_VALUES, m) ? MAIL_MERGE_VALUES[m] : m));
    }

    function mergeTokensInTextNodes(node) {
      if (!node) return;
      if (node.nodeType === 3) {
        const val = node.nodeValue || "";
        if (!val.includes("{{")) return;
        const nextVal = mergeKnownTokens(val);
        if (nextVal !== val) node.nodeValue = nextVal;
        return;
      }
      Array.from(node.childNodes || []).forEach(mergeTokensInTextNodes);
    }

    function mergeTokensInLeafBlock(el) {
      if (el.childElementCount > 0 || el.children?.length > 0) return false;
      const raw = el.textContent || "";
      if (!MAIL_MERGE_TOKEN_TEST_RE.test(raw)) return false;
      const nextText = mergeKnownTokens(raw);
      if (nextText !== raw) el.textContent = nextText;
      return true;
    }

    function docEffectToc(docEditor, summary) {
      const headings = Array.from(docEditor.querySelectorAll("h1, h2, h3"))
        .map((h) => (h.textContent || "").trim())
        .filter(Boolean);
      const nav = docEditor.querySelector(".eo-doc-toc") || document.createElement("nav");
      nav.className = "eo-doc-toc";
      nav.dataset.docToc = "true";
      const titleP = document.createElement("p");
      titleP.textContent = "[자동 목차 생성 (" + summary + ")]";
      const ol = document.createElement("ol");
      for (const hText of headings.length > 0 ? headings : ["1. 문서 개요"]) {
        const li = document.createElement("li");
        li.textContent = hText;
        ol.appendChild(li);
      }
      nav.replaceChildren(titleP, ol);
      if (!nav.parentElement) insertBeforeDocFooter(docEditor, nav);
    }

    function docEffectFootnote(docEditor, summary) {
      const idx = docEditor.querySelectorAll(".eo-doc-footnote").length + 1;
      const aside = document.createElement("aside");
      aside.className = "eo-doc-footnote";
      aside.dataset.docFootnote = "true";
      aside.textContent = "각주 " + idx + ") [" + summary + "] 표준 문서 관리 지침 및 근거 조항 참조";
      insertBeforeDocFooter(docEditor, aside);
    }

    function docEffectMailMerge(docEditor, summary) {
      mergeTokensInTextNodes(docEditor);
      docEditor
        .querySelectorAll("#doc-editor p, #doc-editor li, #doc-editor td, #doc-editor th, #doc-editor div")
        .forEach(mergeTokensInLeafBlock);
      const box = docEditor.querySelector(".eo-doc-mailmerge") || document.createElement("div");
      box.className = "eo-doc-mailmerge";
      box.dataset.docMailmerge = "true";
      box.textContent =
        "[메일 머지 필드 병합 (" + summary + ") · 성명: 이수현 · 기관명: 행정안전부 · 일자: 2026-10-25]";
      if (!box.parentElement) insertBeforeDocFooter(docEditor, box);
    }

    function docEffectHwpHeader(docEditor, summary) {
      const hdr = docEditor.querySelector(".eo-hwp-header") || document.createElement("header");
      hdr.className = "eo-hwp-header";
      hdr.dataset.hwpHeader = "true";
      const titleP = document.createElement("p");
      titleP.textContent = "행정안전부 공문서 표제부 및 결재선 (" + summary + ")";
      const lineP = document.createElement("p");
      lineP.textContent = "[결재: 담당 이수현 / 팀장 박지훈 / 과장 김현우 / 실장 최민석]";
      hdr.replaceChildren(titleP, lineP);
      if (hdr.parentElement) return;
      const firstChild = docEditor.childNodes?.[0];
      if (firstChild && typeof firstChild.before === "function") firstChild.before(hdr);
      else docEditor.appendChild(hdr);
    }

    function docEffectHwpField(docEditor, summary) {
      const rowP = document.createElement("p");
      rowP.className = "eo-hwp-field-row";
      const span = document.createElement("span");
      span.className = "eo-hwp-field";
      span.dataset.hwpField = "true";
      span.textContent = "[누름틀 입력: {{성명}} / {{기관명}}]";
      rowP.appendChild(span);
      rowP.appendChild(document.createTextNode(" (" + summary + ")"));
      insertBeforeDocFooter(docEditor, rowP);
    }

    const DOC_MODULE_EFFECTS = {
      "doc-mod-toc": docEffectToc,
      "doc-mod-footnote": docEffectFootnote,
      "doc-mod-mailmerge": docEffectMailMerge,
      "hwp-mod-header": docEffectHwpHeader,
      "hwp-mod-field": docEffectHwpField,
    };

    function applyDocModuleSpecificDomEffect(cmdId, docEditor, execRes) {
      if (!docEditor) return false;
      if (!Object.hasOwn(DOC_MODULE_EFFECTS, cmdId)) return false;
      DOC_MODULE_EFFECTS[cmdId](docEditor, execRes?.summary || cmdId);
      return true;
    }

    const SLIDE_STAGE_OBJECT_BUILDERS = {
      "slide-insert-table": (summary) => {
        const tbl = document.createElement("table");
        tbl.className = "sheet-table slide-table-obj";
        tbl.dataset.slideObject = "table";
        const tbody = document.createElement("tbody");
        const r1 = document.createElement("tr");
        ["구분", "목표", "달성"].forEach((txt) => {
          const th = document.createElement("th");
          th.textContent = txt;
          r1.appendChild(th);
        });
        const r2 = document.createElement("tr");
        ["분기 지표 (" + summary + ")", "100%", "115%"].forEach((txt) => {
          const td = document.createElement("td");
          td.textContent = txt;
          r2.appendChild(td);
        });
        tbody.appendChild(r1);
        tbody.appendChild(r2);
        tbl.appendChild(tbody);
        return tbl;
      },
      "slide-insert-chart": (summary) => {
        const chartDiv = document.createElement("div");
        chartDiv.className = "slide-chart-obj";
        chartDiv.dataset.slideObject = "chart";
        chartDiv.textContent = "[슬라이드 차트 개체 · 분기 실적 추이 (" + summary + ")]";
        return chartDiv;
      },
      "slide-insert-shape": (summary) => {
        const shapeDiv = document.createElement("div");
        shapeDiv.className = "slide-shape-obj";
        shapeDiv.dataset.slideObject = "shape";
        shapeDiv.textContent = "[슬라이드 도형·프로세스 블록 (" + summary + ")]";
        return shapeDiv;
      },
    };

    function insertSlideStageObject(cmdId, execRes) {
      const lowerZone = document.getElementById("slide-lower-zone");
      const curSlide = slideDeckState[getActiveSlideIndex()];
      const buildNode = SLIDE_STAGE_OBJECT_BUILDERS[cmdId];
      if (!lowerZone || !curSlide || typeof buildNode !== "function") return false;
      lowerZone.appendChild(buildNode(execRes?.summary || cmdId));
      syncActiveSlideDomStylesToDeck();
      return true;
    }

    return {
      setActiveDocBlock,
      syncDocStyleBarFromTarget,
      saveDocTabStateForModule,
      saveActiveDocTabState,
      ensureDefaultDocTabStore,
      restoreDocTabState,
      findEditableDocBlock,
      getFocusedDocBlock,
      computeDocTableSummary,
      appendDocTableRow,
      applyDocModuleSpecificDomEffect,
      insertSlideStageObject,
      syncActiveSlideDomStylesToDeck,
      applySlideDomStyles,
      renderSlideDeck,
      switchToSlideIndex,
      capturePdfStudioState,
      restorePdfStudioState,
      applyPdfStudioFormState,
      openPdfHighlightDialog,
      applyPdfHighlight,
      syncSplitComparePane,
      captureWorkspaceViewState,
      restoreWorkspaceViewState,
      renderModuleOutline,
      ...tools,
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_PANELS__ = createWorkspacePanels;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);

