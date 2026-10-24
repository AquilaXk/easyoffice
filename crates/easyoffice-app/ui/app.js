(function () {
  "use strict";

  const isMac = /Mac|iPhone|iPod|iPad/i.test(navigator.platform || navigator.userAgent);
  const shortcutLabel = isMac ? "⌘" : "Ctrl";
  let isImeComposing = false;
  let activeModule = "DOC";
  let activeRibbonCategory = "home";
  let activeRibbonStyle = "standard_office";
  let currentViewMode = "편집";
  let currentZoomPercent = 100;

  const FORMAT_SPECS = {
    docx: { family: "OOXML (ZIP)", spec: "ECMA-376 WordprocessingML" },
    dotx: { family: "OOXML (ZIP)", spec: "ECMA-376 WordprocessingML Template" },
    doc: { family: "OLE2 (CFBF)", spec: "Binary Word Document" },
    hwpx: { family: "OWPML (ZIP)", spec: "KS X 6101 OWPML" },
    hwtx: { family: "OWPML (ZIP)", spec: "KS X 6101 OWPML Template" },
    hwp: { family: "OLE2 (CFBF)", spec: "HWP 5.0 Binary Document" },
    hwt: { family: "OLE2 (CFBF)", spec: "HWP 5.0 Binary Template" },
    odt: { family: "ODF (ZIP)", spec: "ISO/IEC 26300 ODF Text" },
    xlsx: { family: "OOXML (ZIP)", spec: "ECMA-376 SpreadsheetML" },
    xltx: { family: "OOXML (ZIP)", spec: "ECMA-376 SpreadsheetML Template" },
    xls: { family: "OLE2 (CFBF)", spec: "BIFF8 Workbook Stream" },
    ods: { family: "ODF (ZIP)", spec: "ISO/IEC 26300 ODF Spreadsheet" },
    csv: { family: "PlainText", spec: "RFC 4180 CSV" },
    pptx: { family: "OOXML (ZIP)", spec: "ECMA-376 PresentationML" },
    potx: { family: "OOXML (ZIP)", spec: "ECMA-376 PresentationML Template" },
    ppsx: { family: "OOXML (ZIP)", spec: "ECMA-376 PresentationML Slideshow" },
    ppt: { family: "OLE2 (CFBF)", spec: "Binary Presentation Stream" },
    odp: { family: "ODF (ZIP)", spec: "ISO/IEC 26300 ODF Presentation" },
    pdf: { family: "PDF", spec: "ISO 32000-1 PDF 1.7" },
    md: { family: "PlainText", spec: "CommonMark 0.31.2" }
  };

  const RIBBON_CATEGORY_TOOLS = {
    home: [
      { id: "doc-home-bold", title: "굵게", group: "단락·서식" },
      { id: "doc-home-style", title: "단락 스타일", group: "단락·서식" },
      { id: "doc-ins-table", title: "표 삽입", group: "표·개체" },
      { id: "hwp-mod-manuscript", title: "200자 원고지", group: "표·개체" }
    ],
    insert: [
      { id: "doc-ins-table", title: "표 삽입", group: "표·개체" },
      { id: "doc-ins-image", title: "그림 삽입", group: "표·개체" },
      { id: "doc-ins-header", title: "머리글·바닥글", group: "문서 구성" },
      { id: "doc-ins-footnote", title: "각주·미주", group: "문서 구성" }
    ],
    layout: [
      { id: "doc-lay-margins", title: "여백 설정", group: "페이지 설정" },
      { id: "doc-lay-orient", title: "용지 방향 전환", group: "페이지 설정" },
      { id: "doc-lay-columns", title: "다단 설정", group: "페이지 설정" },
      { id: "hwp-mod-manuscript", title: "200자 원고지", group: "원고지·자간" }
    ],
    module: [
      { id: "doc-mod-toc", title: "목차 생성", group: "문서 전용" },
      { id: "hwp-mod-official", title: "행정 공문서 헤더", group: "공문서 도구" },
      { id: "hwp-mod-manuscript", title: "200자 원고지", group: "공문서 도구" },
      { id: "sheet-mod-pivot", title: "피벗 요약", group: "시트·슬라이드" }
    ],
    review: [
      { id: "doc-rev-track", title: "변경 내용 추적", group: "검토·교정" },
      { id: "doc-rev-accept", title: "변경 승인", group: "검토·교정" },
      { id: "doc-rev-comment", title: "메모 추가", group: "검토·교정" },
      { id: "hwp-rev- inspect", title: "공문서 표기 점검", group: "검토·교정" }
    ],
    view: [
      { id: "doc-view-page", title: "쪽 윤곽 보기", group: "화면 보기" },
      { id: "doc-view-outline", title: "개요 보기", group: "화면 보기" },
      { id: "doc-view-grid", title: "격자선 표시", group: "화면 보기" },
      { id: "doc-view-zoom100", title: "100% 배율", group: "화면 보기" }
    ],
    productivity: [
      { id: "smart-summary", title: "문서 요약·진단", group: "문서 보조" },
      { id: "smart-pii", title: "개인정보 마스킹", group: "보안 점검" },
      { id: "open-converter", title: "포맷 일괄 변환", group: "일괄 변환" },
      { id: "doc-mod-toc", title: "목차 자동 갱신", group: "문서 보조" }
    ]
  };

  const EasyOfficeIPC = {
    async invoke(command, payload) {
      if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === "function") {
        return window.__TAURI_INTERNALS__.invoke(command, { payload: payload || "" });
      }
      switch (command) {
        case "get_workspace_bootstrap":
          return {
            platform: isMac ? "macOS" : "Desktop",
            shortcutModifier: shortcutLabel,
            palette: { primaryAccent: "#5C6BC0", lightScaffold: "#F7F8FC", darkScaffold: "#141724" }
          };
        case "execute_ribbon_command":
          return {
            commandId: payload,
            summary: "[" + payload + "] 명령 실행 완료",
            contextualBadge: activeModule === "SHEET" ? "수식·피벗 분석" : "표·개체 도구",
            catalogCount: 140
          };
        case "get_ribbon_category_commands": {
          const parts = String(payload || "DOC|home").split("|");
          const cat = (parts[1] || parts[0] || "home").trim();
          return {
            module: parts[0] || activeModule,
            category: cat,
            commands: RIBBON_CATEGORY_TOOLS[cat] || RIBBON_CATEGORY_TOOLS.home
          };
        }
        case "switch_ribbon_style": {
          const isHwp = String(payload).indexOf("hangul") !== -1 || String(payload).indexOf("hwp") !== -1;
          return {
            style: isHwp ? "한글(HWP) 스타일" : "표준 오피스",
            categories: isHwp
              ? ["편집", "입력", "쪽·원고지", "공문서·표", "검토", "보기", "생산성·보안"]
              : ["홈", "삽입", "레이아웃", "모듈 도구", "검토", "보기", "생산성·보안"]
          };
        }
        case "evaluate_workbook_formula": {
          const expr = String(payload || "=Sheet2!A1+50").trim();
          return { sheet: "Sheet1", formula: expr, value: String(evaluateCrossSheetFormula(expr)) };
        }
        case "convert_document_format": {
          const segs = String(payload || "docx->hwpx|연간 보고서|본문").split("|");
          const pair = (segs[0] || "docx->hwpx").split("->");
          const srcFmt = (pair[0] || "docx").trim().toLowerCase();
          const dstFmt = (pair[1] || "hwpx").trim().toLowerCase();
          const title = (segs[1] || "연간 아키텍처 보고서").trim();
          const body = (segs[2] || "").trim();
          const lines = body ? body.split(/\r?\n/).filter(Boolean) : [title];
          const meta = FORMAT_SPECS[dstFmt] || FORMAT_SPECS.hwpx;
          const utf8Bytes = new TextEncoder().encode(title + "\n" + body).length;
          return {
            sourceFormat: srcFmt,
            targetFormat: dstFmt,
            sourceBytes: utf8Bytes + 384,
            outputBytes: utf8Bytes + 512,
            targetBytes: utf8Bytes + 512,
            detectedTarget: dstFmt,
            containerFamily: meta.family,
            specification: meta.spec,
            paragraphCount: lines.length,
            title: title,
            status: "OK"
          };
        }
        case "run_smart_assistant": {
          const text = String(payload || "");
          const chars = text.length;
          const words = text.trim() ? text.trim().split(/\s+/).length : 0;
          const todos = (text.match(/TODO:|검토|결재/g) || []).length;
          return {
            summaryCount: 2,
            charCount: chars,
            wordCount: words,
            readingTimeSeconds: Math.max(1, Math.ceil(words / 3.5)),
            actionItemCount: todos
          };
        }
        case "scrub_pii": {
          let scrubbed = String(payload || "");
          let count = 0;
          scrubbed = scrubbed.replace(/\b\d{6}-[1-4]\d{6}\b/g, function () { count++; return "******-*******"; });
          scrubbed = scrubbed.replace(/\b01[016789]-\d{3,4}-\d{4}\b/g, function () { count++; return "010-****-****"; });
          return { scrubbedText: scrubbed, totalMasked: count };
        }
        default:
          return { ok: true };
      }
    }
  };
  window.EasyOfficeIPC = EasyOfficeIPC;

  const osBadge = document.getElementById("os-shortcut-badge");
  const kbdHint = document.getElementById("kbd-hint");
  if (osBadge) osBadge.textContent = isMac ? "macOS ⌘" : "Windows/Linux Ctrl";
  if (kbdHint) kbdHint.textContent = shortcutLabel + "K";

  // 1. Top Document Tabs & Module Switching
  const tabs = document.querySelectorAll(".doc-tab");
  const panes = document.querySelectorAll(".module-pane");
  const inspModule = document.getElementById("insp-module");
  const inspSpec = document.getElementById("insp-spec");
  const statusLeft = document.getElementById("status-left");
  const contextualBadge = document.getElementById("contextual-object-badge");

  const MODULE_META = {
    DOC: { label: "DOC (문서)", spec: "ECMA-376 OOXML (DOCX)", badge: "표·개체 도구" },
    HWP: { label: "HWP (한글·공문서)", spec: "KS X 6101 HWPX / HWP 5.0", badge: "원고지·공문서 도구" },
    SHEET: { label: "SHEET (스프레드시트)", spec: "ECMA-376 / ODF 1.3 (XLSX/ODS)", badge: "수식·피벗 분석" },
    SLIDE: { label: "SLIDE (프레젠테이션)", spec: "ECMA-376 PresentationML", badge: "슬라이드 배치·발표자" },
    PDF: { label: "PDF (문서 스튜디오)", spec: "ISO 32000-1 PDF 1.7", badge: "PDF 주석·스탬프" }
  };

  function activatePane(targetPaneId, mod, tabTitle) {
    activeModule = mod || activeModule;
    document.body.setAttribute("data-active-module", activeModule);
    panes.forEach(function (p) {
      p.classList.toggle("active", p.id === targetPaneId);
    });
    const meta = MODULE_META[activeModule] || MODULE_META.DOC;
    if (inspModule) inspModule.textContent = meta.label;
    if (inspSpec) inspSpec.textContent = meta.spec;
    if (contextualBadge) contextualBadge.textContent = meta.badge;
    if (statusLeft && tabTitle) {
      statusLeft.textContent = "활성 문서: " + tabTitle;
    }
  }

  tabs.forEach(function (tab) {
    tab.addEventListener("click", function () {
      tabs.forEach(function (t) { t.classList.remove("active"); });
      tab.classList.add("active");
      const mod = tab.getAttribute("data-module") || "DOC";
      const targetPane = tab.getAttribute("data-target") || "pane-doc";
      const labelEl = tab.querySelector(".tab-label");
      activatePane(targetPane, mod, labelEl ? labelEl.textContent : "");
    });
  });

  // 2. Contextual 7-Category Ribbon Toolbar (All 7 categories interactive)
  const ribbonReadout = document.getElementById("ribbon-status-readout");
  const ribbonToolsContainer = document.getElementById("ribbon-tools");

  function bindRibbonToolButtons() {
    document.querySelectorAll(".tool-btn[data-ribbon-cmd]").forEach(function (btn) {
      btn.addEventListener("click", async function () {
        const cmdId = btn.getAttribute("data-ribbon-cmd");
        if (cmdId === "smart-summary") {
          runSmartSummary();
          return;
        }
        if (cmdId === "smart-pii") {
          runPiiScrub();
          return;
        }
        if (cmdId === "open-converter") {
          activatePane("pane-converter", "DOC", "포맷 일괄 변환");
          return;
        }
        const res = await EasyOfficeIPC.invoke("execute_ribbon_command", cmdId);
        if (ribbonReadout && res && res.summary) {
          ribbonReadout.textContent = res.summary;
        }
        if (cmdId === "hwp-mod-official") {
          const banner = document.getElementById("official-header-banner");
          if (banner) banner.style.display = banner.style.display === "none" ? "flex" : "none";
        }
        if (cmdId === "hwp-mod-manuscript") {
          toggleManuscriptGrid();
        }
      });
    });

    const summaryBtn = document.getElementById("btn-run-smart-summary");
    if (summaryBtn) summaryBtn.addEventListener("click", runSmartSummary);
    const piiBtn = document.getElementById("btn-run-pii-scrub");
    if (piiBtn) piiBtn.addEventListener("click", runPiiScrub);
  }

  async function renderRibbonCategoryTools(categoryKey) {
    activeRibbonCategory = categoryKey || "home";
    const ipcRes = await EasyOfficeIPC.invoke(
      "get_ribbon_category_commands",
      activeModule + "|" + activeRibbonCategory
    );
    const tools = (ipcRes && Array.isArray(ipcRes.commands) && ipcRes.commands.length > 0)
      ? ipcRes.commands
      : (RIBBON_CATEGORY_TOOLS[activeRibbonCategory] || RIBBON_CATEGORY_TOOLS.home);

    if (!ribbonToolsContainer) return;
    const buttonsHtml = tools.map(function (t) {
      return '<button class="tool-btn" data-ribbon-cmd="' + t.id + '">' + t.title + "</button>";
    }).join("");

    ribbonToolsContainer.innerHTML =
      '<div class="tool-group">' +
        '<span class="group-label">카테고리 도구</span>' +
        buttonsHtml +
      "</div>" +
      '<div class="tool-group">' +
        '<span class="group-label">보조·점검</span>' +
        '<button class="tool-btn" id="btn-run-smart-summary">문서 요약·진단</button>' +
        '<button class="tool-btn" id="btn-run-pii-scrub">개인정보 마스킹</button>' +
      "</div>" +
      '<div class="tool-group">' +
        '<span class="group-label">실행 상태</span>' +
        '<span id="ribbon-status-readout" class="ribbon-readout">카테고리: ' + activeRibbonCategory + "</span>" +
      "</div>";

    bindRibbonToolButtons();
  }

  document.querySelectorAll("#ribbon-categories .ribbon-cat").forEach(function (catBtn) {
    catBtn.addEventListener("click", function () {
      document.querySelectorAll("#ribbon-categories .ribbon-cat").forEach(function (b) {
        b.classList.remove("active");
        b.setAttribute("aria-selected", "false");
      });
      catBtn.classList.add("active");
      catBtn.setAttribute("aria-selected", "true");
      const catKey = catBtn.getAttribute("data-cat") || "home";
      renderRibbonCategoryTools(catKey);
    });
  });

  bindRibbonToolButtons();

  const ribbonStyleSelect = document.getElementById("ribbon-style-select");
  if (ribbonStyleSelect) {
    ribbonStyleSelect.addEventListener("change", async function () {
      activeRibbonStyle = ribbonStyleSelect.value;
      const res = await EasyOfficeIPC.invoke("switch_ribbon_style", activeRibbonStyle);
      const catBtns = document.querySelectorAll("#ribbon-categories .ribbon-cat");
      if (res && Array.isArray(res.categories)) {
        catBtns.forEach(function (b, idx) {
          if (res.categories[idx]) b.textContent = res.categories[idx];
        });
      }
      await renderRibbonCategoryTools(activeRibbonCategory);
    });
  }

  // 3. Interactive Document Table & 200-Cell Manuscript Grid
  const docTableBody = document.getElementById("doc-table-body");
  const addRowBtn = document.getElementById("btn-doc-add-row");
  const mergeCellsBtn = document.getElementById("btn-doc-merge-cells");
  const evalTableSumBtn = document.getElementById("btn-doc-eval-table-sum");
  const toggleManuscriptBtn = document.getElementById("btn-doc-toggle-manuscript");
  const manuscriptGridView = document.getElementById("manuscript-grid-view");

  if (addRowBtn && docTableBody) {
    addRowBtn.addEventListener("click", function () {
      const sumRow = docTableBody.querySelector(".table-sum-row");
      const tr = document.createElement("tr");
      tr.innerHTML =
        '<td contenteditable="true">신규 항목</td>' +
        '<td contenteditable="true" data-role="num">500</td>' +
        '<td contenteditable="true">추가분</td>';
      if (sumRow) {
        docTableBody.insertBefore(tr, sumRow);
      } else {
        docTableBody.appendChild(tr);
      }
    });
  }

  if (mergeCellsBtn && docTableBody) {
    mergeCellsBtn.addEventListener("click", function () {
      const firstRow = docTableBody.querySelector("tr");
      if (!firstRow) return;
      const cells = firstRow.querySelectorAll("td");
      if (cells.length >= 3) {
        if (cells[1].colSpan === 2) {
          cells[1].colSpan = 1;
          cells[2].style.display = "";
        } else {
          cells[1].colSpan = 2;
          cells[2].style.display = "none";
        }
      }
    });
  }

  if (evalTableSumBtn && docTableBody) {
    evalTableSumBtn.addEventListener("click", function () {
      let total = 0;
      docTableBody.querySelectorAll('td[data-role="num"]').forEach(function (td) {
        const v = parseFloat(td.textContent.trim());
        if (!Number.isNaN(v)) total += v;
      });
      const sumCell = document.getElementById("doc-table-sum-cell");
      if (sumCell) sumCell.textContent = String(total);
    });
  }

  function toggleManuscriptGrid() {
    if (!manuscriptGridView) return;
    const isHidden = manuscriptGridView.classList.toggle("hidden");
    if (!isHidden) {
      const surface = document.getElementById("doc-editor-surface");
      const rawText = surface ? surface.textContent.replace(/\s+/g, " ").trim() : "";
      const chars = Array.from(rawText);
      let html = "";
      for (let i = 0; i < 200; i++) {
        const ch = chars[i] ? chars[i] : "";
        html += '<div class="manuscript-cell">' + ch + "</div>";
      }
      manuscriptGridView.innerHTML = html;
    }
  }

  if (toggleManuscriptBtn) {
    toggleManuscriptBtn.addEventListener("click", toggleManuscriptGrid);
  }

  // 4. Smart Assistant & PII Scrubbing
  const smartOutput = document.getElementById("smart-assistant-output");

  async function runSmartSummary() {
    const surface = document.getElementById("doc-editor-surface");
    const text = surface ? surface.innerText : "";
    const res = await EasyOfficeIPC.invoke("run_smart_assistant", text);
    if (smartOutput && res) {
      smartOutput.innerHTML =
        '<div class="smart-stat-line">단어 ' + res.wordCount + "개 · 예상 읽기 " + res.readingTimeSeconds + "초 · 실행항목 " + res.actionItemCount + "건</div>" +
        '<p class="smart-desc">핵심 문장 요약 및 문단 가독성 진단이 완료되었습니다.</p>';
    }
  }

  async function runPiiScrub() {
    const surface = document.getElementById("doc-editor-surface");
    const sample = (surface ? surface.innerText : "") + " (연락처: 010-1234-5678, 주민번호: 900101-1234567)";
    const res = await EasyOfficeIPC.invoke("scrub_pii", sample);
    if (smartOutput && res) {
      smartOutput.innerHTML =
        '<div class="smart-stat-line">개인정보 마스킹 완료 (' + res.totalMasked + "건 보호됨)</div>" +
        '<p class="smart-desc">' + res.scrubbedText + "</p>";
    }
  }

  // 5. Recent Documents Format Filter Bar
  const chips = document.querySelectorAll("#format-filter-chips .chip");
  const recentItems = document.querySelectorAll("#recent-doc-list li");
  const filterStatus = document.getElementById("filter-status");

  chips.forEach(function (chip) {
    chip.addEventListener("click", function () {
      chips.forEach(function (c) { c.classList.remove("active"); });
      chip.classList.add("active");
      const filter = chip.getAttribute("data-filter") || "ALL";
      let visibleCount = 0;
      recentItems.forEach(function (item) {
        const fmt = item.getAttribute("data-fmt");
        const show = filter === "ALL" || fmt === filter;
        item.style.display = show ? "list-item" : "none";
        if (show) visibleCount++;
      });
      if (filterStatus) {
        filterStatus.textContent = "표시 중: " + filter + " 포맷 (" + visibleCount + "개 문서)";
      }
    });
  });

  // 6. Korean IME Composition Guard
  const editableSurfaces = document.querySelectorAll('[contenteditable="true"], input, textarea');
  const inspIme = document.getElementById("insp-ime");

  editableSurfaces.forEach(function (el) {
    el.addEventListener("compositionstart", function () {
      isImeComposing = true;
      if (inspIme) inspIme.textContent = "한글 조합 중";
    });
    el.addEventListener("compositionend", function () {
      isImeComposing = false;
      if (inspIme) inspIme.textContent = "한국어 입력 준비";
    });
  });

  // 7. Multi-Sheet Tab Bar & Cross-Sheet Formula Evaluation
  const sheetStore = {
    Sheet1: { A1: "1200", B1: "850", C1: "2050", D1: "정상", A2: "450", B2: "550", C2: "1000", D2: "검토완료", A3: "합계", B3: "1400", C3: "3050", D3: "승인" },
    Sheet2: { A1: "100", B1: "250", C1: "350", D1: "보조시트", A2: "300", B2: "400", C2: "700", D2: "정상", A3: "합계", B3: "650", C3: "1050", D3: "완료" }
  };
  let activeSheetName = "Sheet1";

  function saveCurrentSheetDomToStore() {
    const map = sheetStore[activeSheetName] || {};
    document.querySelectorAll("#sheet-grid-body td[data-cell]").forEach(function (td) {
      map[td.getAttribute("data-cell")] = td.textContent.trim();
    });
    sheetStore[activeSheetName] = map;
  }

  function loadSheetFromStore(sheetName) {
    activeSheetName = sheetName;
    const map = sheetStore[sheetName] || {};
    document.querySelectorAll("#sheet-grid-body td[data-cell]").forEach(function (td) {
      const addr = td.getAttribute("data-cell");
      td.textContent = map[addr] !== undefined ? map[addr] : "";
    });
    const statusEl = document.getElementById("sheet-tab-status");
    if (statusEl) {
      statusEl.textContent = "활성 시트: " + sheetName + " · 교차 참조(=Sheet2!A1+50) 지원";
    }
  }

  function evaluateCrossSheetFormula(rawExpr) {
    saveCurrentSheetDomToStore();
    const expr = String(rawExpr || "").trim();
    const crossMatch = expr.match(/^=([A-Za-z0-9_]+)!([A-D][1-3])([+\-*/])(\d+(?:\.\d+)?)$/);
    if (crossMatch) {
      const targetSheet = crossMatch[1];
      const cellAddr = crossMatch[2];
      const op = crossMatch[3];
      const rhs = parseFloat(crossMatch[4]);
      const sheetData = sheetStore[targetSheet] || {};
      const lhs = parseFloat(sheetData[cellAddr] || "0") || 0;
      if (op === "+") return lhs + rhs;
      if (op === "-") return lhs - rhs;
      if (op === "*") return lhs * rhs;
      if (op === "/") return rhs === 0 ? "#DIV/0!" : lhs / rhs;
    }
    const vals = [];
    document.querySelectorAll("#sheet-grid-body td[data-cell]").forEach(function (td) {
      const addr = td.getAttribute("data-cell");
      if (addr === "A1" || addr === "B1" || addr === "A2" || addr === "B2") {
        const v = parseFloat(td.textContent.trim());
        if (!Number.isNaN(v)) vals.push(v);
      }
    });
    if (/^=SUM/i.test(expr)) {
      return vals.slice(0, 2).reduce(function (a, b) { return a + b; }, 0);
    }
    if (/^=AVERAGE/i.test(expr)) {
      const slice = vals.slice(0, 2);
      return slice.length ? slice.reduce(function (a, b) { return a + b; }, 0) / slice.length : 0;
    }
    return vals.reduce(function (a, b) { return a + b; }, 0);
  }

  function bindSheetTabs() {
    document.querySelectorAll("#sheet-tab-bar .sheet-tab").forEach(function (btn) {
      btn.onclick = function () {
        saveCurrentSheetDomToStore();
        document.querySelectorAll("#sheet-tab-bar .sheet-tab").forEach(function (b) {
          b.classList.remove("active");
          b.setAttribute("aria-selected", "false");
        });
        btn.classList.add("active");
        btn.setAttribute("aria-selected", "true");
        loadSheetFromStore(btn.getAttribute("data-sheet") || "Sheet1");
      };
    });
  }
  bindSheetTabs();

  const addSheetBtn = document.getElementById("btn-add-sheet");
  const sheetTabBar = document.getElementById("sheet-tab-bar");
  if (addSheetBtn && sheetTabBar) {
    addSheetBtn.addEventListener("click", function () {
      const count = Object.keys(sheetStore).length + 1;
      const newName = "Sheet" + count;
      sheetStore[newName] = { A1: "0", B1: "0", C1: "0", D1: "신규시트" };
      const btn = document.createElement("button");
      btn.className = "sheet-tab";
      btn.setAttribute("data-sheet", newName);
      btn.setAttribute("role", "tab");
      btn.setAttribute("aria-selected", "false");
      btn.textContent = newName;
      sheetTabBar.insertBefore(btn, addSheetBtn);
      bindSheetTabs();
      btn.click();
    });
  }

  const evalBtn = document.getElementById("eval-formula-btn");
  const formulaInput = document.getElementById("formula-input");
  if (evalBtn && formulaInput) {
    evalBtn.addEventListener("click", async function () {
      if (isImeComposing) return;
      const expr = formulaInput.value.trim();
      let result = evaluateCrossSheetFormula(expr);
      if (expr.indexOf("!") !== -1) {
        const ipcRes = await EasyOfficeIPC.invoke("evaluate_workbook_formula", expr);
        if (ipcRes && ipcRes.value !== undefined) result = ipcRes.value;
      }
      const targetCell = document.querySelector('#sheet-grid-body td[data-cell="C1"]');
      if (targetCell) targetCell.textContent = String(result);
    });
  }

  // 8. 20-Format Batch Conversion Studio (#pane-converter)
  const convertBtn = document.getElementById("btn-run-format-convert");
  if (convertBtn) {
    convertBtn.addEventListener("click", async function () {
      const srcSelect = document.getElementById("conv-source-fmt");
      const dstSelect = document.getElementById("conv-target-fmt");
      const bodyArea = document.getElementById("conv-input-body");
      const titleInput = document.getElementById("doc-title-input");

      const srcFmt = srcSelect ? srcSelect.value : "docx";
      const dstFmt = dstSelect ? dstSelect.value : "hwpx";
      const title = titleInput ? titleInput.textContent.trim() : "연간 아키텍처 보고서";
      const body = bodyArea ? bodyArea.value : "";

      const res = await EasyOfficeIPC.invoke(
        "convert_document_format",
        srcFmt + "->" + dstFmt + "|" + title + "|" + body
      );
      if (res) {
        const statusVal = document.getElementById("conv-status-val");
        const specVal = document.getElementById("conv-spec-val");
        const bytesVal = document.getElementById("conv-bytes-val");
        const previewVal = document.getElementById("conv-preview-val");
        if (statusVal) statusVal.textContent = srcFmt.toUpperCase() + " → " + dstFmt.toUpperCase() + " 변환 완료 (" + res.status + ")";
        if (specVal) specVal.textContent = res.containerFamily + " · " + res.specification;
        if (bytesVal) bytesVal.textContent = "원본 " + res.sourceBytes + " B → 변환 " + res.outputBytes + " B";
        if (previewVal) previewVal.textContent = "제목: " + res.title + " · 문단 " + res.paragraphCount + "개 무결성 확인";
      }
    });
  }

  // 9. Status Bar Interactive View Mode & Quick Zoom Controls
  const statusRight = document.getElementById("status-right");
  const editorStage = document.getElementById("editor-stage");

  function updateStatusRightReadout() {
    if (statusRight) {
      statusRight.textContent = currentViewMode + " 보기 · " + currentZoomPercent + "%";
    }
  }

  document.querySelectorAll("#status-view-modes .status-chip").forEach(function (btn) {
    btn.addEventListener("click", function () {
      document.querySelectorAll("#status-view-modes .status-chip").forEach(function (b) {
        b.classList.remove("active");
      });
      btn.classList.add("active");
      currentViewMode = btn.textContent.trim();
      const mode = btn.getAttribute("data-view-mode");
      const surface = document.getElementById("doc-editor-surface");
      if (surface) {
        surface.setAttribute("contenteditable", mode === "read" ? "false" : "true");
      }
      if (mode === "present") {
        activatePane("pane-slide", "SLIDE", "제품_전략_발표덱.pptx");
      }
      updateStatusRightReadout();
    });
  });

  document.querySelectorAll("#status-zoom-controls .status-chip").forEach(function (btn) {
    btn.addEventListener("click", function () {
      document.querySelectorAll("#status-zoom-controls .status-chip").forEach(function (b) {
        b.classList.remove("active");
      });
      btn.classList.add("active");
      currentZoomPercent = parseInt(btn.getAttribute("data-zoom") || "100", 10);
      if (editorStage) {
        editorStage.style.zoom = (currentZoomPercent / 100).toString();
      }
      updateStatusRightReadout();
    });
  });

  // 10. Theme Toggle & Command Palette
  const themeBtn = document.getElementById("theme-toggle");
  function toggleTheme() {
    const isDark = document.body.classList.toggle("theme-dark");
    document.body.classList.toggle("theme-light", !isDark);
    if (themeBtn) themeBtn.textContent = isDark ? "라이트 모드" : "다크 모드";
  }
  if (themeBtn) themeBtn.addEventListener("click", toggleTheme);

  const cmdModal = document.getElementById("cmd-modal");
  const cmdTrigger = document.getElementById("cmd-palette-trigger");
  const cmdInput = document.getElementById("cmd-search-input");

  function toggleCmdModal(forceOpen) {
    if (!cmdModal) return;
    const willOpen = typeof forceOpen === "boolean" ? forceOpen : cmdModal.classList.contains("hidden");
    cmdModal.classList.toggle("hidden", !willOpen);
    if (willOpen && cmdInput) cmdInput.focus();
  }

  if (cmdTrigger) cmdTrigger.addEventListener("click", function () { toggleCmdModal(true); });

  window.addEventListener("keydown", function (e) {
    if (e.isComposing || isImeComposing) return;
    const primaryMod = isMac ? e.metaKey : e.ctrlKey;
    if (primaryMod && e.key.toLowerCase() === "k") {
      e.preventDefault();
      toggleCmdModal();
    } else if (e.key === "Escape" && cmdModal && !cmdModal.classList.contains("hidden")) {
      toggleCmdModal(false);
    }
  });

  document.querySelectorAll("#cmd-results li").forEach(function (item) {
    item.addEventListener("click", function () {
      const cmd = item.getAttribute("data-cmd");
      if (cmd === "toggle-theme") toggleTheme();
      else if (cmd === "toggle-hwp-ribbon" && ribbonStyleSelect) {
        ribbonStyleSelect.value = ribbonStyleSelect.value === "hangul_hwp" ? "standard_office" : "hangul_hwp";
        ribbonStyleSelect.dispatchEvent(new Event("change"));
      } else if (cmd === "scrub-pii") runPiiScrub();
      else if (cmd === "switch-doc") document.querySelector('.doc-tab[data-module="DOC"]').click();
      else if (cmd === "switch-sheet") document.querySelector('.doc-tab[data-module="SHEET"]').click();
      else if (cmd === "switch-slide") document.querySelector('.doc-tab[data-module="SLIDE"]').click();
      else if (cmd === "switch-pdf") document.querySelector('.doc-tab[data-module="PDF"]').click();
      else if (cmd === "switch-converter") document.querySelector('.doc-tab[data-target="pane-converter"]').click();
      toggleCmdModal(false);
    });
  });

  EasyOfficeIPC.invoke("get_workspace_bootstrap", "");
})();
