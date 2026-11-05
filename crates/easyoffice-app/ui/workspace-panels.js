(function (host) {
  "use strict";

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
        textAlignMode: "",
        paperSize: "",
        styleRole: "",
        fontLang: "",
        lineSpacing: "",
        showCtrlMarks: "",
        showParaMarks: "true",
        strike: "",
        charfx: "",
        textcolor: "",
        highlight: "",
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
      textAlignMode: editor.dataset.textAlign || "",
      paperSize: editor.dataset.paperSize || "",
      styleRole: editor.dataset.styleRole || "",
      fontLang: editor.dataset.fontLang || "",
      lineSpacing: editor.dataset.lineSpacing || "",
      showCtrlMarks: editor.dataset.showCtrlMarks || "",
      showParaMarks: editor.dataset.showParaMarks || "",
      strike: editor.dataset.strike || "",
      charfx: editor.dataset.charfx || "",
      textcolor: editor.dataset.textcolor || "",
      highlight: editor.dataset.highlight || "",
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
    if (ls.textAlignMode) {
      editor.dataset.textAlign = ls.textAlignMode;
    } else {
      delete editor.dataset.textAlign;
    }
    [
      "paperSize",
      "styleRole",
      "fontLang",
      "lineSpacing",
      "showCtrlMarks",
      "showParaMarks",
      "strike",
      "charfx",
      "textcolor",
      "highlight",
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

  function createWorkspacePanels(deps) {
    const {
      document,
      docTabStore,
      sheetDataStore,
      slideDeckState,
      defaultHwpEditorNodes,
      createEmptySlideStyleFields,
      isExcludedDocChromeElement,
      sheetController,
      getActiveModule,
      getActiveSheetName,
      getActiveSlideIndex,
      setActiveSlideIndex,
      getPdfRotationDeg,
    } = deps;

    let activeDocBlockEl = null;

    function syncDocStyleBarFromTarget(targetEl) {
      const docEditor = document.getElementById("doc-editor");
      const boldBtn = document.getElementById("btn-stylebar-bold");
      const italicBtn = document.getElementById("btn-stylebar-italic");
      const underBtn = document.getElementById("btn-stylebar-underline");
      const sizeBadge = document.getElementById("doc-style-size-badge");
      const sizeInput = document.getElementById("doc-font-size-input");
      if (boldBtn) boldBtn.setAttribute("aria-pressed", targetEl?.style?.fontWeight === "600" ? "true" : "false");
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

    function renderSlideDeck(idx) {
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

      const thumbList = document.getElementById("slide-thumb-list");
      if (thumbList) {
        thumbList.innerHTML = "";
        slideDeckState.forEach((s, i) => {
          const btn = document.createElement("button");
          btn.className = "slide-thumb" + (i === idx ? " active" : "");
          btn.dataset.slideIdx = String(i);
          btn.textContent = i + 1 + ". " + s.title.slice(0, 14);
          btn.addEventListener("click", () => switchToSlideIndex(i));
          thumbList.appendChild(btn);
        });
      }
      if (getActiveModule() === "SLIDE") {
        renderModuleOutline("SLIDE");
      }
    }

    function appendNewSlideDeckItem() {
      syncActiveSlideDomStylesToDeck();
      const nextNum = slideDeckState.length + 1;
      slideDeckState.push({
        title: "새 슬라이드 #" + nextNum + " (16:9 와이드)",
        lower: "핵심 요약 및 다이어그램 영역",
        notes: "발표자 노트를 입력하세요.",
        transition: "fade",
        ...createEmptySlideStyleFields(),
      });
      renderSlideDeck(slideDeckState.length - 1);
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
        cardHeadingNodes: headingEl ? serializeDomChildren(headingEl) : [],
        cardBodyParagraphsNodes: bodyParagraphs.map((p) => serializeDomChildren(p)),
        formStatus: document.getElementById("pdf-form-status")?.textContent ?? "",
        highlightText: card?.querySelector(".pdf-highlight-badge")?.textContent ?? "",
        splitMergeText: card?.querySelector(".pdf-splitmerge-badge")?.textContent ?? "",
      };
    }

    function restorePdfStudioState(pdfStudio) {
      if (!pdfStudio) return;
      const stampSel = document.getElementById("pdf-stamp-select");
      const stampBadge = document.getElementById("pdf-stamp-badge");
      const wmInput = document.getElementById("pdf-watermark-input");
      const wmLayer = document.getElementById("pdf-watermark-layer");
      const appInput = document.getElementById("pdf-form-applicant");
      const deptInput = document.getElementById("pdf-form-dept");
      const card = document.getElementById("pdf-page-card");
      const cardHeading = document.querySelector("#pdf-page-card h3");
      const formStatus = document.getElementById("pdf-form-status");
      if (stampSel && pdfStudio.stampValue !== undefined) stampSel.value = pdfStudio.stampValue;
      if (stampBadge && pdfStudio.stampBadge !== undefined) {
        stampBadge.textContent = pdfStudio.stampBadge;
      }
      if (wmInput && pdfStudio.watermarkInput !== undefined) {
        wmInput.value = pdfStudio.watermarkInput;
      }
      if (wmLayer && pdfStudio.watermarkLayer !== undefined) {
        wmLayer.textContent = pdfStudio.watermarkLayer;
      }
      if (appInput && pdfStudio.applicant !== undefined) appInput.value = pdfStudio.applicant;
      if (deptInput && pdfStudio.dept !== undefined) deptInput.value = pdfStudio.dept;
      if (cardHeading) {
        if (Array.isArray(pdfStudio.cardHeadingNodes)) {
          restoreDomChildren(cardHeading, pdfStudio.cardHeadingNodes, document);
        } else if (pdfStudio.cardHeading !== undefined) {
          cardHeading.textContent = pdfStudio.cardHeading;
        }
      }
      if (Array.isArray(pdfStudio.cardBodyParagraphsNodes) && card) {
        const existingPs = Array.from(card.querySelectorAll("p:not(#pdf-form-status)"));
        while (existingPs.length > pdfStudio.cardBodyParagraphsNodes.length) {
          const extra = existingPs.pop();
          extra.remove();
        }
        while (existingPs.length < pdfStudio.cardBodyParagraphsNodes.length) {
          const newP = document.createElement("p");
          if (formStatus && formStatus.parentElement === card) {
            formStatus.before(newP);
          } else {
            card.appendChild(newP);
          }
          existingPs.push(newP);
        }
        pdfStudio.cardBodyParagraphsNodes.forEach((nodes, idx) => {
          restoreDomChildren(existingPs[idx], nodes, document);
        });
      } else if (pdfStudio.cardBody !== undefined) {
        const firstBody = document.querySelector("#pdf-page-card p:not(#pdf-form-status)");
        if (firstBody) firstBody.textContent = pdfStudio.cardBody;
      }
      if (card) {
        const existingHl = card.querySelector(".pdf-highlight-badge");
        if (pdfStudio.highlightText) {
          const hl = existingHl || document.createElement("span");
          hl.className = "pdf-highlight-badge";
          hl.dataset.pdfHighlight = "true";
          hl.textContent = pdfStudio.highlightText;
          if (!existingHl) card.appendChild(hl);
        } else if (existingHl) {
          existingHl.remove();
        }
        const existingSm = card.querySelector(".pdf-splitmerge-badge");
        if (pdfStudio.splitMergeText) {
          const sm = existingSm || document.createElement("span");
          sm.className = "pdf-splitmerge-badge";
          sm.dataset.pdfSplitmerge = "true";
          sm.textContent = pdfStudio.splitMergeText;
          if (!existingSm) card.appendChild(sm);
        } else if (existingSm) {
          existingSm.remove();
        }
      }
      if (formStatus && pdfStudio.formStatus !== undefined) {
        formStatus.textContent = pdfStudio.formStatus;
      }
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
      const strip = document.getElementById("ribbon-tool-strip");
      const stage = document.querySelector(".canvas-stage");
      return {
        ribbonStripHidden: Boolean(strip?.hidden),
        canvasZoom: stage?.style?.zoom || "",
        canvasSplit: stage?.dataset?.splitView || "",
      };
    }

    function restoreWorkspaceViewState(viewState) {
      if (!viewState) return;
      const strip = document.getElementById("ribbon-tool-strip");
      const stage = document.querySelector(".canvas-stage");
      if (strip) {
        strip.hidden = Boolean(viewState.ribbonStripHidden);
      }
      if (stage) {
        stage.style.zoom = viewState.canvasZoom || "";
        if (viewState.canvasSplit) {
          stage.dataset.splitView = viewState.canvasSplit;
        } else {
          delete stage.dataset.splitView;
        }
        syncSplitComparePane(stage);
      }
      const zoomText = viewState.canvasZoom || "100%";
      const zoomLabel = document.getElementById("status-zoom-label");
      if (zoomLabel) {
        zoomLabel.textContent = zoomText;
      }
      const zoomSlider = document.getElementById("status-zoom-slider");
      if (zoomSlider) {
        const zoomVal = Number.parseInt(zoomText, 10) || 100;
        zoomSlider.value = String(zoomVal);
      }
    }

    function renderModuleOutline(mod) {
      const outlineList = document.getElementById("outline-list");
      if (!outlineList) return;
      let items = [];
      if (mod === "DOC" || mod === "HWP") {
        const headings = document.querySelectorAll("#doc-editor h1, #doc-editor h2");
        items = Array.from(headings).map((h, idx) => ({
          label: (h.textContent || "").trim(),
          action: () => h.scrollIntoView({ behavior: "smooth", block: "nearest" }),
          active: idx === 0,
        }));
      } else if (mod === "SHEET") {
        const curSheet = getActiveSheetName();
        items = Object.keys(sheetDataStore).map((sName, idx) => ({
          label: idx + 1 + ". 시트 워크시트 (" + sName + ")",
          action: () => {
            void sheetController.switchSheetTab(sName).catch(() => {});
          },
          active: curSheet === sName,
        }));
        items.push({
          label: items.length + 1 + ". 연간 합계 행 (F7)",
          action: () => sheetController.updateSheetSelectionAndAggregates("F7"),
          active: false,
        });
      } else if (mod === "SLIDE") {
        const curSlideIdx = getActiveSlideIndex();
        items = slideDeckState.map((s, idx) => ({
          label: idx + 1 + ". " + s.title,
          action: () => switchToSlideIndex(idx),
          active: idx === curSlideIdx,
        }));
      } else if (mod === "PDF") {
        items = [
          {
            label: "1. 결재 스탬프 및 워터마크",
            action: () => document.getElementById("pdf-stamp-select")?.focus(),
            active: true,
          },
          {
            label: "2. 신청인·부서 양식 필드",
            action: () => document.getElementById("pdf-form-applicant")?.focus(),
            active: false,
          },
        ];
      } else {
        items = [
          {
            label: "1. 원본·대상 포맷 선택",
            action: () => document.getElementById("conv-src-fmt")?.focus(),
            active: true,
          },
          {
            label: "2. 컨테이너 무결성 검증",
            action: () => document.getElementById("btn-run-convert")?.focus(),
            active: false,
          },
        ];
      }

      outlineList.innerHTML = "";
      items.forEach((item) => {
        const li = document.createElement("li");
        li.className = "outline-item" + (item.active ? " active" : "");
        li.textContent = item.label;
        li.addEventListener("click", () => {
          outlineList
            .querySelectorAll(".outline-item")
            .forEach((el) => el.classList.remove("active"));
          li.classList.add("active");
          if (typeof item.action === "function") item.action();
        });
        outlineList.appendChild(li);
      });
    }

    const tools = host.__EASYOFFICE_CREATE_WORKSPACE_TOOLS__(deps, {
      saveActiveDocTabState,
      ensureDefaultDocTabStore,
      computeDocTableSummary,
      syncActiveSlideDomStylesToDeck,
      renderSlideDeck,
      applyPdfStudioFormState,
    });

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
      const roleBadge = document.getElementById("doc-style-role-badge");
      if (roleBadge) roleBadge.textContent = roleVal;
      const styleSelect = document.getElementById("doc-style-select");
      if (styleSelect) styleSelect.value = roleVal;
      const fontLang = document.getElementById("doc-font-lang");
      if (fontLang) fontLang.value = editor?.dataset?.fontLang || "대표";
      const fontSelect = document.getElementById("doc-font-select");
      const fontVal = editor?.style?.fontFamily || "Pretendard";
      if (fontSelect) fontSelect.value = fontVal;
      const fontBadge = document.getElementById("doc-style-font-badge");
      if (fontBadge) fontBadge.textContent = fontVal;
      const lsSelect = document.getElementById("doc-linespacing-select");
      if (lsSelect) lsSelect.value = editor?.dataset?.lineSpacing || "160";
      [
        ["btn-stylebar-strike", "strike"],
        ["btn-stylebar-charfx", "charfx"],
        ["btn-stylebar-textcolor", "textcolor"],
        ["btn-stylebar-highlight", "highlight"],
        ["btn-toggle-ctrl-marks", "showCtrlMarks"],
        ["btn-toggle-para-marks", "showParaMarks"],
      ].forEach(([id, key]) => {
        const btn = document.getElementById(id);
        const on = editor?.dataset?.[key] === "true";
        if (btn) {
          btn.classList.toggle("active", on);
          btn.setAttribute("aria-pressed", on ? "true" : "false");
        }
      });
      const isGrid = Boolean(editor?.classList?.contains("manuscript-grid"));
      const gridBtn = document.getElementById("btn-toggle-doc-grid");
      if (gridBtn) {
        gridBtn.classList.toggle("active", isGrid);
        gridBtn.setAttribute("aria-pressed", isGrid ? "true" : "false");
      }
      const alignMode = editor?.dataset?.textAlign || editor?.style?.textAlign || "justify";
      ["left", "center", "right", "justify", "distribute", "divide"].forEach((m) => {
        const b = document.getElementById("btn-align-" + m);
        const on = m === alignMode;
        if (b) {
          b.classList.toggle("active", on);
          b.setAttribute("aria-pressed", on ? "true" : "false");
        }
      });
      const hwpPreview = document.getElementById("hwp-manuscript-preview");
      if (hwpPreview) {
        hwpPreview.classList.toggle("manuscript-grid", isGrid);
      }
      activeDocBlockEl = null;
      const firstBlock = editor ? getFocusedDocBlock(editor) : null;
      syncDocStyleBarFromTarget(firstBlock);
      computeDocTableSummary();
      tools.computeSmartStats();
    }

    function findEditableDocBlock(node, docEditor) {
      if (!node || !docEditor) return null;
      let cur = node.nodeType === 1 ? node : node.parentElement;
      while (cur && cur !== docEditor) {
        if (cur.parentElement === docEditor) {
          if (!isExcludedDocChromeElement(cur)) {
            return cur;
          }
          return null;
        }
        cur = cur.parentElement;
      }
      return null;
    }

    function getFocusedDocBlock(docEditor) {
      if (!docEditor) return null;
      const sel = host.getSelection ? host.getSelection() : null;
      if (sel && sel.anchorNode) {
        const block = findEditableDocBlock(sel.anchorNode, docEditor);
        if (block) {
          activeDocBlockEl = block;
          return block;
        }
      }
      if (activeDocBlockEl && docEditor.contains(activeDocBlockEl)) {
        return activeDocBlockEl;
      }
      const children = Array.from(docEditor.children || []);
      const firstBlock = children.find((el) => !isExcludedDocChromeElement(el));
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

    function insertBeforeDocFooter(docEditor, el) {
      const footer = docEditor.querySelector("footer.doc-page-footer");
      if (footer && footer.parentElement === docEditor) {
        footer.before(el);
      } else {
        docEditor.appendChild(el);
      }
    }

    function applyDocModuleSpecificDomEffect(cmdId, docEditor, execRes) {
      if (!docEditor) return false;
      const summary = execRes?.summary || cmdId;
      if (cmdId === "doc-mod-toc") {
        const headings = Array.from(docEditor.querySelectorAll("h1, h2, h3"))
          .map((h) => (h.textContent || "").trim())
          .filter(Boolean);
        const nav = docEditor.querySelector(".eo-doc-toc") || document.createElement("nav");
        nav.className = "eo-doc-toc";
        nav.dataset.docToc = "true";
        const titleP = document.createElement("p");
        titleP.textContent = "[자동 목차 생성 (" + summary + ")]";
        const ol = document.createElement("ol");
        (headings.length > 0 ? headings : ["1. 문서 개요"]).forEach((hText) => {
          const li = document.createElement("li");
          li.textContent = hText;
          ol.appendChild(li);
        });
        nav.replaceChildren(titleP, ol);
        if (!nav.parentElement) insertBeforeDocFooter(docEditor, nav);
        return true;
      }
      if (cmdId === "doc-mod-footnote") {
        const idx = docEditor.querySelectorAll(".eo-doc-footnote").length + 1;
        const aside = document.createElement("aside");
        aside.className = "eo-doc-footnote";
        aside.dataset.docFootnote = "true";
        aside.textContent =
          "각주 " + idx + ") [" + summary + "] 표준 문서 관리 지침 및 근거 조항 참조";
        insertBeforeDocFooter(docEditor, aside);
        return true;
      }
      if (cmdId === "doc-mod-mailmerge") {
        const mergeMap = {
          "{{성명}}": "이수현",
          "{{기관명}}": "행정안전부",
          "{{일자}}": "2026-10-25",
        };
        let replaced = 0;
        function replaceTokens(node) {
          if (!node) return;
          if (node.nodeType === 3) {
            const val = node.nodeValue || "";
            if (!val.includes("{{")) return;
            const nextVal = val.replace(/\{\{[^}]+\}\}/g, (m) => {
              if (Object.prototype.hasOwnProperty.call(mergeMap, m)) {
                replaced += 1;
                return mergeMap[m];
              }
              return m;
            });
            if (nextVal !== val) node.nodeValue = nextVal;
            return;
          }
          Array.from(node.childNodes || []).forEach(replaceTokens);
        }
        replaceTokens(docEditor);
        docEditor
          .querySelectorAll(
            "#doc-editor p, #doc-editor li, #doc-editor td, #doc-editor th, #doc-editor div"
          )
          .forEach((el) => {
            if (el.querySelector("p, li, td, th, div")) return;
            const raw = el.textContent || "";
            if (!/\{\{[^}]+\}\}/.test(raw)) return;
            const nextText = raw.replace(/\{\{[^}]+\}\}/g, (m) => {
              if (Object.prototype.hasOwnProperty.call(mergeMap, m)) {
                replaced += 1;
                return mergeMap[m];
              }
              return m;
            });
            if (nextText !== raw) el.textContent = nextText;
          });
        const box =
          docEditor.querySelector(".eo-doc-mailmerge") || document.createElement("div");
        box.className = "eo-doc-mailmerge";
        box.dataset.docMailmerge = "true";
        box.textContent =
          "[메일 머지 필드 병합 (" +
          summary +
          ") · 성명: 이수현 · 기관명: 행정안전부 · 일자: 2026-10-25]";
        if (!box.parentElement) insertBeforeDocFooter(docEditor, box);
        return true;
      }
      if (cmdId === "hwp-mod-header") {
        const hdr =
          docEditor.querySelector(".eo-hwp-header") || document.createElement("header");
        hdr.className = "eo-hwp-header";
        hdr.dataset.hwpHeader = "true";
        const titleP = document.createElement("p");
        titleP.textContent = "행정안전부 공문서 표제부 및 결재선 (" + summary + ")";
        const lineP = document.createElement("p");
        lineP.textContent = "[결재: 담당 이수현 / 팀장 박지훈 / 과장 김현우 / 실장 최민석]";
        hdr.replaceChildren(titleP, lineP);
        if (!hdr.parentElement) {
          const firstChild = docEditor.childNodes?.[0];
          if (firstChild && typeof firstChild.before === "function") {
            firstChild.before(hdr);
          } else {
            docEditor.appendChild(hdr);
          }
        }
        return true;
      }
      if (cmdId === "hwp-mod-field") {
        const rowP = document.createElement("p");
        rowP.className = "eo-hwp-field-row";
        const span = document.createElement("span");
        span.className = "eo-hwp-field";
        span.dataset.hwpField = "true";
        span.textContent = "[누름틀 입력: {{성명}} / {{기관명}}]";
        const tail = document.createTextNode(" (" + summary + ")");
        rowP.appendChild(span);
        rowP.appendChild(tail);
        insertBeforeDocFooter(docEditor, rowP);
        return true;
      }
      return false;
    }

    function insertSlideStageObject(cmdId, execRes) {
      const lowerZone = document.getElementById("slide-lower-zone");
      const curSlide = slideDeckState[getActiveSlideIndex()];
      const summary = execRes?.summary || cmdId;
      if (!lowerZone || !curSlide) return false;
      if (cmdId === "slide-insert-table") {
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
        lowerZone.appendChild(tbl);
      } else if (cmdId === "slide-insert-chart") {
        const chartDiv = document.createElement("div");
        chartDiv.className = "slide-chart-obj";
        chartDiv.dataset.slideObject = "chart";
        chartDiv.textContent = "[슬라이드 차트 개체 · 분기 실적 추이 (" + summary + ")]";
        lowerZone.appendChild(chartDiv);
      } else if (cmdId === "slide-insert-shape") {
        const shapeDiv = document.createElement("div");
        shapeDiv.className = "slide-shape-obj";
        shapeDiv.dataset.slideObject = "shape";
        shapeDiv.textContent = "[슬라이드 도형·프로세스 블록 (" + summary + ")]";
        lowerZone.appendChild(shapeDiv);
      } else {
        return false;
      }
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
      appendNewSlideDeckItem,
      capturePdfStudioState,
      restorePdfStudioState,
      applyPdfStudioFormState,
      syncSplitComparePane,
      captureWorkspaceViewState,
      restoreWorkspaceViewState,
      renderModuleOutline,
      ...tools,
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_PANELS__ = createWorkspacePanels;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);

