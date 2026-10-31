(function (host) {
  "use strict";

  function captureDocLayoutStyle(editor) {
    if (!editor) {
      return {
        columnCount: "",
        maxWidth: "",
        padding: "",
        minHeight: "",
        paperSize: "",
        manuscriptGrid: false,
      };
    }
    return {
      columnCount: editor.style.columnCount || "",
      maxWidth: editor.style.maxWidth || "",
      padding: editor.style.padding || "",
      minHeight: editor.style.minHeight || "",
      paperSize: editor.dataset.paperSize || "",
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
    if (ls.paperSize) {
      editor.dataset.paperSize = ls.paperSize;
    } else {
      delete editor.dataset.paperSize;
    }
    editor.classList.toggle("manuscript-grid", Boolean(ls.manuscriptGrid));
  }

  function createWorkspacePanels(deps) {
    const {
      document,
      docTabStore,
      sheetDataStore,
      slideDeckState,
      defaultHwpEditorHtml,
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

    function setActiveDocBlock(el) {
      activeDocBlockEl = el;
    }

    function saveDocTabStateForModule(mod) {
      if (mod !== "DOC" && mod !== "HWP") return;
      const editor = document.getElementById("doc-editor");
      if (!editor) return;
      docTabStore[mod] = {
        editorHtml: editor.innerHTML,
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
          editorHtml: editor.innerHTML,
          layoutStyle: captureDocLayoutStyle(editor),
        };
      }
      if (!docTabStore.HWP) {
        docTabStore.HWP = {
          editorHtml: defaultHwpEditorHtml,
          layoutStyle: captureDocLayoutStyle(null),
        };
      }
    }

    function computeDocTableSummary() {
      const numCells = document.querySelectorAll(
        "#doc-table-body tr:not(#doc-table-sum-row) [data-doc-num]"
      );
      let total = 0;
      numCells.forEach((cell) => {
        const textCandidate = (cell.textContent || "").trim();
        const raw = (textCandidate || cell.dataset.docNum || "0").replaceAll(",", "");
        const val = Number.parseFloat(raw);
        if (!Number.isNaN(val)) {
          cell.dataset.docNum = String(val);
          total += val;
        } else if (textCandidate !== "") {
          cell.dataset.docNum = "0";
        }
      });
      const sumCell = document.getElementById("doc-table-sum-cell");
      if (sumCell) {
        sumCell.textContent = String(total);
        sumCell.dataset.docNum = String(total);
      }
      const statusEl =
        document.getElementById("doc-table-summary") ||
        document.getElementById("status-message") ||
        document.getElementById("status-command-feedback");
      if (statusEl) {
        statusEl.textContent = "표 =SUM(ABOVE) 합계: " + total + "백만원";
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
      }
      if (stageBox) {
        slide.stagePadding = stageBox.style.padding || "";
        slide.stageOutline = stageBox.style.outline || "";
        slide.stageLayoutState = stageBox.dataset.layoutState || "";
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
      return {
        stampValue: document.getElementById("pdf-stamp-select")?.value ?? "승인필",
        stampBadge: document.getElementById("pdf-stamp-badge")?.textContent ?? "승인필",
        watermarkInput: document.getElementById("pdf-watermark-input")?.value ?? "",
        watermarkLayer: document.getElementById("pdf-watermark-layer")?.textContent ?? "",
        applicant: document.getElementById("pdf-form-applicant")?.value ?? "",
        dept: document.getElementById("pdf-form-dept")?.value ?? "",
        cardHeadingHtml: document.querySelector("#pdf-page-card h3")?.innerHTML ?? "",
        cardBodyParagraphsHtml: bodyParagraphs.map((p) => p.innerHTML),
        formStatus: document.getElementById("pdf-form-status")?.textContent ?? "",
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
        if (pdfStudio.cardHeadingHtml !== undefined) {
          cardHeading.innerHTML = pdfStudio.cardHeadingHtml;
        } else if (pdfStudio.cardHeading !== undefined) {
          cardHeading.textContent = pdfStudio.cardHeading;
        }
      }
      if (Array.isArray(pdfStudio.cardBodyParagraphsHtml) && card) {
        const existingPs = Array.from(card.querySelectorAll("p:not(#pdf-form-status)"));
        while (existingPs.length > pdfStudio.cardBodyParagraphsHtml.length) {
          const extra = existingPs.pop();
          extra.remove();
        }
        while (existingPs.length < pdfStudio.cardBodyParagraphsHtml.length) {
          const newP = document.createElement("p");
          if (formStatus && formStatus.parentElement === card) {
            formStatus.before(newP);
          } else {
            card.appendChild(newP);
          }
          existingPs.push(newP);
        }
        pdfStudio.cardBodyParagraphsHtml.forEach((html, idx) => {
          existingPs[idx].innerHTML = html;
        });
      } else if (pdfStudio.cardBody !== undefined) {
        const firstBody = document.querySelector("#pdf-page-card p:not(#pdf-form-status)");
        if (firstBody) firstBody.textContent = pdfStudio.cardBody;
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
        editor.innerHTML = saved.editorHtml;
        applyDocLayoutStyle(editor, saved.layoutStyle);
      }
      activeDocBlockEl = null;
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
      tr.innerHTML = '<td>신규 사업부</td><td>담당 연구원</td><td data-doc-num="150">150</td>';
      const sumRow = document.getElementById("doc-table-sum-row");
      if (sumRow) {
        sumRow.before(tr);
      } else {
        tbody.appendChild(tr);
      }
      computeDocTableSummary();
      tools.computeSmartStats();
    }

    return {
      setActiveDocBlock,
      saveDocTabStateForModule,
      saveActiveDocTabState,
      ensureDefaultDocTabStore,
      restoreDocTabState,
      findEditableDocBlock,
      getFocusedDocBlock,
      computeDocTableSummary,
      appendDocTableRow,
      syncActiveSlideDomStylesToDeck,
      applySlideDomStyles,
      renderSlideDeck,
      appendNewSlideDeckItem,
      capturePdfStudioState,
      restorePdfStudioState,
      applyPdfStudioFormState,
      captureWorkspaceViewState,
      restoreWorkspaceViewState,
      renderModuleOutline,
      ...tools,
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_PANELS__ = createWorkspacePanels;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
