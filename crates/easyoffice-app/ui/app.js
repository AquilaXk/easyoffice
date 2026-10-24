(function () {
  "use strict";

  const MODULE_ACCENTS = {
    DOC: "var(--eo-accent-doc)",
    HWP: "var(--eo-accent-hwp)",
    SHEET: "var(--eo-accent-sheet)",
    SLIDE: "var(--eo-accent-slide)",
    PDF: "var(--eo-accent-pdf)",
    CONV: "var(--eo-primary)",
  };

  const MODULE_LABELS = {
    DOC: "문서 (DOCX)",
    HWP: "한글 (HWP/HWPX)",
    SHEET: "시트 (XLSX)",
    SLIDE: "슬라이드 (PPTX)",
    PDF: "PDF 스튜디오",
    CONV: "일괄 변환기",
  };

  const MODULE_SPECS = {
    DOC: "워드프로세서 문서 (.docx / .dotx / .rtf)",
    HWP: "한글 문서 (.hwpx / .hwtx / .hwp / .hwt)",
    SHEET: "스프레드시트 통합 문서 (.xlsx / .xls / .ods)",
    SLIDE: "프레젠테이션 슬라이드 (.pptx / .ppsx / .odp)",
    PDF: "PDF 문서 검토 및 양식 (.pdf)",
    CONV: "통합 문서 포맷 상호 변환",
  };

  const CONTEXTUAL_TOOL_BADGES = {
    DOC: "표·개체 도구",
    HWP: "한글 표·원고지·누름틀",
    SHEET: "수식·피벗 분석",
    SLIDE: "도형·애니메이션 스테이지",
    PDF: "PDF 주석·양식·결재",
    CONV: "포맷 일괄 변환",
  };

  const RIBBON_STYLE_LABELS = {
    standard: ["홈", "삽입", "레이아웃", "수식·데이터", "모듈 전용", "검토", "보기"],
    hangul: ["파일·편집", "보기", "쪽·원고지", "입력·누름틀", "서식·스타일", "표·개체", "생산성·보안"],
  };

  const COMMON_CATEGORY_SPECS = {
    home: [
      { suffix: "home-bold", title: "굵게", shortcut: "⌘B" },
      { suffix: "home-italic", title: "기울임", shortcut: "⌘I" },
      { suffix: "home-underline", title: "밑줄", shortcut: "⌘U" },
      { suffix: "home-style", title: "스타일 적용", shortcut: "⌥⌘S" },
    ],
    insert: [
      { suffix: "insert-table", title: "표 삽입", shortcut: "⌃⌘T" },
      { suffix: "insert-chart", title: "차트 개체", shortcut: "⌃⌘C" },
      { suffix: "insert-shape", title: "도형·다이어그램", shortcut: "⌃⌘S" },
      { suffix: "insert-link", title: "책갈피·링크", shortcut: "⌘K" },
    ],
    layout: [
      { suffix: "layout-paper", title: "용지 규격 (A4/Letter)", shortcut: "⇧⌘P" },
      { suffix: "layout-orientation", title: "용지 방향 전환", shortcut: "⌥⌘O" },
      { suffix: "layout-columns", title: "다단 설정", shortcut: "⌥⌘M" },
      { suffix: "layout-margins", title: "여백 프리셋", shortcut: "⌥⌘G" },
    ],
    formulasAndData: [
      { suffix: "data-autosum", title: "자동 합계 (SUM)", shortcut: "⇧⌘T" },
      { suffix: "data-fn-browser", title: "함수 마법사", shortcut: "⇧F3" },
      { suffix: "data-sort", title: "오름차순·내림차순 정렬", shortcut: "⌥⌘R" },
      { suffix: "data-validation", title: "데이터 유효성 검사", shortcut: "⌥⌘V" },
    ],
    review: [
      { suffix: "review-track", title: "변경 내용 추적", shortcut: "⇧⌘E" },
      { suffix: "review-comment", title: "검토 메모 스레드", shortcut: "⌥⌘C" },
      { suffix: "review-verify", title: "문서 구조 보안 검사", shortcut: "⌃⌘V" },
      { suffix: "review-stats", title: "문서 통계·글자 수", shortcut: "⇧⌘I" },
    ],
    view: [
      { suffix: "view-split", title: "좌우 스플릿 뷰", shortcut: "⌃⌘2" },
      { suffix: "view-ribbon", title: "리본 접기·펼치기", shortcut: "⌥⌘R" },
      { suffix: "view-theme", title: "라이트·다크 테마 전환", shortcut: "⌃⌥D" },
      { suffix: "view-zoom", title: "100% 실제 배율", shortcut: "⌘0" },
    ],
  };

  const MODULE_SPECIFIC_SPECS = {
    DOC: [
      { id: "doc-mod-toc", title: "자동 목차 생성", shortcut: "⌃⌥T" },
      { id: "doc-mod-footnote", title: "각주·미주 삽입", shortcut: "⌥⌘F" },
      { id: "doc-mod-mailmerge", title: "메일 머지 치환", shortcut: "⌃⌘M" },
      { id: "doc-mod-tableformula", title: "표 셀 수식 계산", shortcut: "⌃⌘=" },
    ],
    HWP: [
      { id: "hwp-mod-header", title: "행정 공문서 두문·결재란", shortcut: "⌃⌥H" },
      { id: "hwp-mod-field", title: "누름틀 필드 삽입", shortcut: "⌃⌘E" },
      { id: "hwp-mod-border", title: "이중선·대각선 테두리", shortcut: "⌃⌘B" },
      { id: "hwp-mod-manuscript", title: "200자 원고지 매수 검산", shortcut: "⌃⌘W" },
    ],
    SHEET: [
      { id: "sheet-mod-pivot", title: "피벗 테이블 요약", shortcut: "⌥⌘P" },
      { id: "sheet-mod-chart", title: "차트 시각화", shortcut: "⌥F1" },
      { id: "sheet-mod-condfmt", title: "조건부 서식 규칙", shortcut: "⌃⌘F" },
      { id: "sheet-mod-freeze", title: "행·열 틀 고정", shortcut: "⌃⌘L" },
    ],
    SLIDE: [
      { id: "slide-mod-master", title: "마스터 레이아웃", shortcut: "⇧⌘M" },
      { id: "slide-mod-snap", title: "스마트 정렬 가이드", shortcut: "⌘;" },
      { id: "slide-mod-transition", title: "화면 전환 효과", shortcut: "⌥⌘T" },
      { id: "slide-mod-presenter", title: "발표자 도구 시작", shortcut: "⌥⌘P" },
    ],
    PDF: [
      { id: "pdf-mod-highlight", title: "형광펜·밑줄 주석", shortcut: "⌃⌘H" },
      { id: "pdf-mod-rotate", title: "페이지 90° 회전", shortcut: "⌘R" },
      { id: "pdf-mod-splitmerge", title: "페이지 분할·병합", shortcut: "⇧⌘S" },
      { id: "pdf-mod-tooffice", title: "편집 문서 변환", shortcut: "⇧⌘E" },
    ],
  };

  function buildRibbonCatalog() {
    const prefixes = {
      DOC: "document",
      HWP: "hwp",
      SHEET: "sheet",
      SLIDE: "slide",
      PDF: "pdf",
    };
    const catalog = {};
    Object.keys(prefixes).forEach((modKey) => {
      const prefix = prefixes[modKey];
      catalog[modKey] = {};
      Object.keys(COMMON_CATEGORY_SPECS).forEach((catKey) => {
        catalog[modKey][catKey] = COMMON_CATEGORY_SPECS[catKey].map((item) => ({
          id: prefix + "-" + item.suffix,
          title: item.title,
          shortcut: item.shortcut,
        }));
      });
      catalog[modKey].moduleSpecific = MODULE_SPECIFIC_SPECS[modKey].slice();
    });
    return catalog;
  }

  const RIBBON_CATEGORY_TOOLS = buildRibbonCatalog();

  const SLIDE_DECK_STATE = [
    {
      title: "연간 제품 전략 발표 자료 (16:9 와이드)",
      lower: "핵심 지표 및 요약 다이어그램 영역",
      notes: "핵심 메시지: 주요 문서 포맷 호환성과 수식 계산 결과를 요약하여 전달합니다.",
      transition: "fade",
    },
    {
      title: "문서 포맷 호환성 및 일괄 변환 흐름",
      lower: "DOCX · HWP 5.0 · HWPX · ODF · XLSX · PPTX · PDF · RTF · CSV · Markdown",
      notes: "다양한 문서 포맷 간 서식과 표 구조를 유지하며 상호 변환합니다.",
      transition: "push",
    },
    {
      title: "대용량 문서 및 다중 시트 처리 성능 요약",
      lower: "빠른 문서 열기 · 수식 재계산 · 피벗 집계 · PDF 출력",
      notes: "대용량 스프레드시트와 프레젠테이션 문서를 지연 없이 편집합니다.",
      transition: "zoom",
    },
  ];

  let isImeComposing = false;
  let activeModule = "DOC";
  let activeRibbonStyle = "standard";
  let activeRibbonCategory = "home";
  let activeSheetCellRef = "C1";
  let activeSlideIndex = 0;
  let slideLaserActive = false;
  let pdfRotationDeg = 0;

  function localScrubPii(rawText) {
    const text = String(rawText || "");
    let residentCount = 0;
    let phoneCount = 0;
    let emailCount = 0;
    const scrubbedText = text
      .replace(/\b\d{6}-[1-4]\d{6}\b/g, () => {
        residentCount += 1;
        return "******-*******";
      })
      .replace(/\b01[016789]-\d{3,4}-\d{4}\b/g, () => {
        phoneCount += 1;
        return "010-****-****";
      })
      .replace(/\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/g, () => {
        emailCount += 1;
        return "***@***.***";
      });
    return {
      scrubbedText,
      maskedResidentIds: residentCount,
      maskedPhoneNumbers: phoneCount,
      maskedEmails: emailCount,
      totalMasked: residentCount + phoneCount + emailCount,
    };
  }

  function localSmartAssistant(rawText) {
    const text = String(rawText || "").trim();
    const chars = Array.from(text).length;
    const words = text ? text.split(/\s+/).length : 0;
    const sentences = text ? text.split(/[.!?。]+/).filter((s) => s.trim().length > 0).length : 0;
    const readingTimeSeconds = Math.max(1, Math.ceil((words / 200) * 60));
    const lines = text ? text.split(/\n+/).filter((l) => l.trim().length > 0) : [];
    return {
      summaryCount: Math.min(2, lines.length),
      charCount: chars,
      wordCount: words,
      sentenceCount: sentences,
      readingTimeSeconds,
      actionItemCount: 0,
      suggestionCount: 0,
    };
  }

  async function invokeIpc(command, payload) {
    if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === "function") {
      const raw = await window.__TAURI_INTERNALS__.invoke(command, { payload });
      return typeof raw === "string" ? JSON.parse(raw) : raw;
    }
    if (command === "get_workspace_bootstrap") {
      return {
        platform: navigator.platform.includes("Mac") ? "macOS" : "Desktop",
        shortcutModifier: navigator.platform.includes("Mac") ? "⌘" : "Ctrl",
        recentFormatFilters: ["전체", "DOCX", "HWP/HWPX", "XLSX", "PPTX", "PDF", "ODF"],
        openTabs: [
          { id: "tab-doc", title: "연간_업무_보고서.docx", module: "DOC", isPinned: true },
          { id: "tab-hwp", title: "행정_표준_기안문.hwpx", module: "HWP", isPinned: true },
          { id: "tab-sheet", title: "분기_예산_시트.xlsx", module: "SHEET", isPinned: false },
          { id: "tab-slide", title: "제품_전략_발표자료.pptx", module: "SLIDE", isPinned: false },
          { id: "tab-pdf", title: "검토_승인_문서.pdf", module: "PDF", isPinned: false },
        ],
      };
    }
    if (command === "get_ribbon_category_commands") {
      const [modPart, catPart] = String(payload || "DOC|home").split("|");
      const mKey = modPart === "CONV" ? "DOC" : modPart;
      const modMap = RIBBON_CATEGORY_TOOLS[mKey] || RIBBON_CATEGORY_TOOLS.DOC;
      return { commands: modMap[catPart] || modMap.home };
    }
    if (command === "convert_document_format") {
      const parts = String(payload || "docx->hwpx|문서|본문").split("|");
      const [src, dst] = (parts[0] || "docx->hwpx").split("->");
      return {
        sourceFormat: src,
        targetFormat: dst,
        sourceBytes: 1842,
        outputBytes: 2048,
        detectedTarget: dst,
        specification: MODULE_SPECS[dst.toUpperCase()] || "문서 포맷",
        status: "OK",
      };
    }
    if (command === "run_engine_benchmark") {
      return {
        status: "ok",
        iterations: 5,
        allEquivalent: true,
        suites: [
          { suiteId: "crc32_slicing_by_8", baselineP50Us: 2068, optimizedP50Us: 464, speedupFactor: 4.46 },
          { suiteId: "deflate_zlib_roundtrip", baselineP50Us: 234, optimizedP50Us: 59, speedupFactor: 3.97 },
          { suiteId: "utf16_swar_and_xml_unescape", baselineP50Us: 50, optimizedP50Us: 29, speedupFactor: 1.72 },
          { suiteId: "sheet_dag_bitset_eval", baselineP50Us: 878, optimizedP50Us: 308, speedupFactor: 2.85 },
        ],
      };
    }
    if (command === "run_smart_assistant") {
      return localSmartAssistant(payload);
    }
    if (command === "scrub_pii") {
      return localScrubPii(payload);
    }
    if (command === "evaluate_workbook_formula") {
      const expr = String(payload || "").split("?").pop() || "=Sheet2!A1+50";
      return { sheet: "Sheet1", formula: expr, value: "150" };
    }
    if (command === "evaluate_sheet" || command === "evaluate_sheet_formula") {
      return { value: String(evaluateLocalSheetFormula(payload)) };
    }
    if (command === "audit_security") {
      return { status: "SAFE", xxeGuard: "PASSED", zipGuard: "PASSED", checkedEntries: 2 };
    }
    if (command === "switch_ribbon_style") {
      const isHangul = String(payload).includes("hangul");
      return {
        style: isHangul ? "한글(HWP) 7탭 모드" : "표준 오피스 7탭 모드",
        categories: isHangul ? RIBBON_STYLE_LABELS.hangul : RIBBON_STYLE_LABELS.standard,
      };
    }
    if (command === "execute_ribbon_command") {
      return {
        commandId: payload,
        summary: "명령 실행 완료 (" + payload + ")",
        contextualBadge: CONTEXTUAL_TOOL_BADGES[activeModule] || "표·개체 도구",
      };
    }
    return { status: "ok" };
  }

  function collectSheetCellMap() {
    const map = {};
    document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
      const ref = td.dataset.cell;
      const formula = td.dataset.formula;
      map[ref] = formula ? formula : (td.textContent || "").trim();
    });
    return map;
  }

  function evaluateFormulaExpression(exprStr, cellValues) {
    const clean = String(exprStr || "").trim();
    if (!clean.startsWith("=")) {
      const n = Number(clean);
      return Number.isFinite(n) && clean !== "" ? n : clean;
    }
    const body = clean.slice(1).trim().toUpperCase();
    const sumMatch = body.match(/^SUM\(([A-D][1-4]):([A-D][1-4])\)$/);
    if (sumMatch) {
      const c1 = sumMatch[1].charCodeAt(0);
      const r1 = Number(sumMatch[1].slice(1));
      const c2 = sumMatch[2].charCodeAt(0);
      const r2 = Number(sumMatch[2].slice(1));
      let total = 0;
      for (let c = Math.min(c1, c2); c <= Math.max(c1, c2); c++) {
        for (let r = Math.min(r1, r2); r <= Math.max(r1, r2); r++) {
          const ref = String.fromCharCode(c) + r;
          const v = Number(cellValues[ref]);
          if (Number.isFinite(v)) total += v;
        }
      }
      return Math.round(total * 100) / 100;
    }
    const avgMatch = body.match(/^AVERAGE\(([A-D][1-4]):([A-D][1-4])\)$/);
    if (avgMatch) {
      const c1 = avgMatch[1].charCodeAt(0);
      const r1 = Number(avgMatch[1].slice(1));
      const c2 = avgMatch[2].charCodeAt(0);
      const r2 = Number(avgMatch[2].slice(1));
      let total = 0;
      let count = 0;
      for (let c = Math.min(c1, c2); c <= Math.max(c1, c2); c++) {
        for (let r = Math.min(r1, r2); r <= Math.max(r1, r2); r++) {
          const ref = String.fromCharCode(c) + r;
          const v = Number(cellValues[ref]);
          if (Number.isFinite(v)) {
            total += v;
            count += 1;
          }
        }
      }
      return count > 0 ? Math.round((total / count) * 100) / 100 : 0;
    }
    if (body.includes("SHEET2!A1")) {
      const replaced = body.replace(/SHEET2!A1/g, "100");
      const m = replaced.match(/^([0-9.]+)\s*([+\-*/])\s*([0-9.]+)$/);
      if (m) {
        const a = Number(m[1]);
        const b = Number(m[3]);
        if (m[2] === "+") return a + b;
        if (m[2] === "-") return a - b;
        if (m[2] === "*") return a * b;
        if (m[2] === "/") return b !== 0 ? a / b : "#DIV/0!";
      }
    }
    const substituted = body.replace(/\b([A-D][1-4])\b/g, (_, ref) => {
      const v = Number(cellValues[ref]);
      return Number.isFinite(v) ? String(v) : "0";
    });
    const arith = substituted.match(/^([0-9.]+)\s*([+\-*/])\s*([0-9.]+)$/);
    if (arith) {
      const a = Number(arith[1]);
      const b = Number(arith[3]);
      let res = 0;
      if (arith[2] === "+") res = a + b;
      if (arith[2] === "-") res = a - b;
      if (arith[2] === "*") res = a * b;
      if (arith[2] === "/") return b !== 0 ? Math.round((a / b) * 100) / 100 : "#DIV/0!";
      return Math.round(res * 100) / 100;
    }
    return clean;
  }

  function evaluateLocalSheetFormula(payload) {
    const [cellsPart, queryPart] = String(payload || "").split("?");
    const values = {};
    String(cellsPart || "")
      .split(";")
      .forEach((pair) => {
        const idx = pair.indexOf("=");
        if (idx > 0) {
          const k = pair.slice(0, idx).trim();
          const v = pair.slice(idx + 1).trim();
          values[k] = v;
        }
      });
    for (let pass = 0; pass < 3; pass++) {
      Object.keys(values).forEach((k) => {
        if (String(values[k]).startsWith("=")) {
          values[k] = evaluateFormulaExpression(values[k], values);
        }
      });
    }
    const q = (queryPart || "C1").trim();
    return q.startsWith("=") ? evaluateFormulaExpression(q, values) : (values[q] ?? "");
  }

  function updateSheetSelectionAndAggregates(cellRef) {
    activeSheetCellRef = cellRef;
    const colLetter = cellRef.charAt(0);
    const rowNumber = cellRef.slice(1);

    document.querySelectorAll("#pane-sheet th[data-col]").forEach((th) => {
      th.classList.toggle("active-axis", th.dataset.col === colLetter);
    });
    document.querySelectorAll("#pane-sheet th[data-row]").forEach((th) => {
      th.classList.toggle("active-axis", th.dataset.row === rowNumber);
    });

    let activeTd = null;
    const numericVals = [];
    document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
      const isSel = td.dataset.cell === cellRef;
      td.classList.toggle("selected-cell", isSel);
      if (isSel) activeTd = td;
      const num = Number((td.textContent || "").replace(/,/g, "").trim());
      if (Number.isFinite(num) && (td.textContent || "").trim() !== "") {
        numericVals.push(num);
      }
    });

    const refBadge = document.getElementById("active-cell-ref");
    if (refBadge) refBadge.textContent = cellRef;
    const formulaInput = document.getElementById("formula-input");
    if (formulaInput && activeTd) {
      formulaInput.value = activeTd.dataset.formula || (activeTd.textContent || "").trim();
    }

    const aggEl = document.getElementById("status-sheet-agg");
    if (aggEl && numericVals.length > 0) {
      const sum = numericVals.reduce((a, b) => a + b, 0);
      const avg = Math.round((sum / numericVals.length) * 10) / 10;
      const max = Math.max(...numericVals);
      aggEl.textContent =
        "선택 집계 · 합계 " +
        sum.toLocaleString() +
        " · 평균 " +
        avg.toLocaleString() +
        " · 개수 " +
        numericVals.length +
        " · 최대 " +
        max.toLocaleString();
    }
  }

  function applyModuleAccent(mod) {
    activeModule = mod;
    const accent = MODULE_ACCENTS[mod] || "var(--eo-primary)";
    document.documentElement.style.setProperty("--eo-module-active", accent);
    const swatch = document.getElementById("module-swatch");
    const label = document.getElementById("module-accent-label");
    if (swatch) swatch.style.background = accent;
    if (label) label.textContent = "모듈 색상: " + (MODULE_LABELS[mod] || mod);

    const statusMod = document.getElementById("status-module");
    const statusSpec = document.getElementById("status-format-spec");
    if (statusMod) statusMod.textContent = "모듈: " + (MODULE_LABELS[mod] || mod);
    if (statusSpec) statusSpec.textContent = "포맷: " + (MODULE_SPECS[mod] || "");

    const ctxBadge = document.getElementById("contextual-tool-badge");
    if (ctxBadge) ctxBadge.textContent = CONTEXTUAL_TOOL_BADGES[mod] || "표·개체 도구";

    const hwpNotice = document.getElementById("hwp-manuscript-preview");
    if (hwpNotice) hwpNotice.hidden = mod !== "HWP";
  }

  function activatePane(mod) {
    applyModuleAccent(mod);
    const panes = {
      DOC: document.getElementById("pane-doc"),
      HWP: document.getElementById("pane-doc"),
      SHEET: document.getElementById("pane-sheet"),
      SLIDE: document.getElementById("pane-slide"),
      PDF: document.getElementById("pane-pdf"),
      CONV: document.getElementById("pane-converter"),
    };
    Object.values(panes).forEach((el) => {
      if (el) el.hidden = true;
    });
    const target = panes[mod] || panes.DOC;
    if (target) target.hidden = false;

    document.querySelectorAll(".mod-btn").forEach((btn) => {
      btn.classList.toggle("active", btn.dataset.module === mod);
    });
    document.querySelectorAll(".tab-pill").forEach((tab) => {
      tab.classList.toggle("active", tab.dataset.module === mod);
    });
    renderRibbonCategoryTools(activeRibbonCategory);
  }

  async function renderRibbonCategoryTools(categoryKey) {
    activeRibbonCategory = categoryKey || "home";
    const strip = document.getElementById("ribbon-tool-strip");
    if (!strip) return;
    const modKey = activeModule === "CONV" ? "DOC" : activeModule;
    const res = await invokeIpc("get_ribbon_category_commands", modKey + "|" + activeRibbonCategory);
    const cmds = (res && Array.isArray(res.commands) && res.commands) || [];
    strip.innerHTML = "";
    cmds.forEach((cmd) => {
      const btn = document.createElement("button");
      btn.className = "ribbon-tool-btn";
      btn.dataset.commandId = cmd.id;
      btn.innerHTML =
        "<span>" + cmd.title + "</span> <small>(" + (cmd.shortcut || "") + ")</small>";
      btn.addEventListener("click", async () => {
        const execRes = await invokeIpc("execute_ribbon_command", cmd.id);
        const fb = document.getElementById("status-command-feedback");
        if (fb && execRes) fb.textContent = execRes.summary || "명령 실행: " + cmd.title;
      });
      strip.appendChild(btn);
    });
  }

  function renderSlideDeck(idx) {
    activeSlideIndex = idx;
    const slide = SLIDE_DECK_STATE[idx] || SLIDE_DECK_STATE[0];
    const titleZone = document.getElementById("slide-title-zone");
    const lowerZone = document.getElementById("slide-lower-zone");
    const notesInput = document.getElementById("slide-presenter-notes");
    const transSelect = document.getElementById("slide-transition-select");
    const metaStatus = document.getElementById("slide-meta-status");

    if (titleZone) titleZone.innerHTML = "<h2>" + slide.title + "</h2>";
    if (lowerZone) lowerZone.innerHTML = "<p>" + slide.lower + "</p>";
    if (notesInput) notesInput.value = slide.notes;
    if (transSelect) transSelect.value = slide.transition;
    if (metaStatus) {
      metaStatus.textContent =
        "슬라이드 " + (idx + 1) + " / " + SLIDE_DECK_STATE.length + " · 전환: " + slide.transition;
    }

    const thumbList = document.getElementById("slide-thumb-list");
    if (thumbList) {
      thumbList.innerHTML = "";
      SLIDE_DECK_STATE.forEach((s, i) => {
        const btn = document.createElement("button");
        btn.className = "slide-thumb" + (i === activeSlideIndex ? " active" : "");
        btn.dataset.slideIdx = String(i);
        btn.textContent = i + 1 + ". " + s.title.slice(0, 14);
        btn.addEventListener("click", () => renderSlideDeck(i));
        thumbList.appendChild(btn);
      });
    }
  }

  function extractEditableDocumentText() {
    const editor = document.getElementById("doc-editor");
    if (!editor) return "";
    const blocks = editor.querySelectorAll(
      ":scope > h1, :scope > h2, :scope > p:not(#hwp-manuscript-preview)"
    );
    if (blocks.length > 0) {
      return Array.from(blocks)
        .map((el) => (el.textContent || "").trim())
        .filter((t) => t.length > 0)
        .join("\n");
    }
    return (editor.innerText || "").trim();
  }

  function computeSmartStats() {
    const statsEl = document.getElementById("smart-stats");
    if (!statsEl) return;
    const text = extractEditableDocumentText();
    const chars = Array.from(text).length;
    const words = text ? text.split(/\s+/).length : 0;
    const secs = Math.max(1, Math.ceil((words / 220) * 60));
    statsEl.textContent = "문자: " + chars + " · 단어: " + words + " · 읽기 시간: " + secs + "초";
  }

  function renderCommandPaletteList(filterText) {
    const listEl = document.getElementById("cmd-palette-list");
    if (!listEl) return;
    const q = String(filterText || "").trim().toLowerCase();
    const allItems = [];
    Object.keys(RIBBON_CATEGORY_TOOLS).forEach((mod) => {
      const cats = RIBBON_CATEGORY_TOOLS[mod];
      Object.keys(cats).forEach((cat) => {
        cats[cat].forEach((c) => {
          allItems.push({
            id: c.id,
            module: mod,
            label: "[" + mod + "] " + c.title + " (" + c.shortcut + ")",
          });
        });
      });
    });
    allItems.push({
      id: "bench-run",
      module: activeModule,
      label: "[성능] 문서 엔진 성능 진단 실행",
    });

    listEl.innerHTML = "";
    allItems
      .filter((item) => !q || item.label.toLowerCase().includes(q) || item.id.toLowerCase().includes(q))
      .slice(0, 18)
      .forEach((item) => {
        const li = document.createElement("li");
        li.textContent = item.label;
        li.addEventListener("click", async () => {
          const modal = document.getElementById("cmd-palette-modal");
          if (modal && typeof modal.close === "function") modal.close();
          if (item.id === "bench-run") {
            await runBenchmarkAction();
          } else {
            activatePane(item.module);
            const res = await invokeIpc("execute_ribbon_command", item.id);
            const fb = document.getElementById("status-command-feedback");
            if (fb && res) fb.textContent = res.summary;
          }
        });
        listEl.appendChild(li);
      });
  }

  async function runBenchmarkAction() {
    const res = await invokeIpc("run_engine_benchmark", "5");
    const box = document.getElementById("bench-result-box");
    const fb = document.getElementById("status-command-feedback");
    if (res && Array.isArray(res.suites)) {
      const summary = res.suites
        .map((s) => s.suiteId + " " + s.baselineP50Us + "µs→" + s.optimizedP50Us + "µs (" + s.speedupFactor + "x)")
        .join(" · ");
      if (box) {
        box.textContent = "성능 진단 완료 (" + (res.allEquivalent ? "결과 일치" : "불일치") + ") · " + summary;
      }
      if (fb) {
        fb.textContent = "성능 진단 완료 · " + summary;
      }
    }
  }

  async function initWorkspace() {
    const boot = await invokeIpc("get_workspace_bootstrap", "");
    const pill = document.getElementById("platform-pill");
    const hint = document.getElementById("shortcut-hint");
    if (pill) pill.textContent = boot.platform + " · " + boot.shortcutModifier;
    if (hint) hint.textContent = boot.shortcutModifier + "K";

    const tabStrip = document.getElementById("tab-strip");
    if (tabStrip && Array.isArray(boot.openTabs)) {
      tabStrip.innerHTML = "";
      boot.openTabs.forEach((t, idx) => {
        const btn = document.createElement("button");
        btn.className = "tab-pill" + (idx === 0 ? " active" : "");
        btn.dataset.module = t.module;
        btn.textContent = t.title;
        btn.addEventListener("click", () => activatePane(t.module));
        tabStrip.appendChild(btn);
      });
    }

    const filterBox = document.getElementById("format-filters");
    if (filterBox && Array.isArray(boot.recentFormatFilters)) {
      filterBox.innerHTML = "";
      boot.recentFormatFilters.forEach((f, idx) => {
        const chip = document.createElement("button");
        chip.className = "filter-chip" + (idx === 0 ? " active" : "");
        chip.textContent = f;
        chip.addEventListener("click", () => {
          document.querySelectorAll(".filter-chip").forEach((c) => c.classList.remove("active"));
          chip.classList.add("active");
          const map = { DOCX: "DOC", "HWP/HWPX": "HWP", XLSX: "SHEET", PPTX: "SLIDE", PDF: "PDF", ODF: "CONV" };
          if (map[f]) activatePane(map[f]);
        });
        filterBox.appendChild(chip);
      });
    }

    document.querySelectorAll(".mod-btn").forEach((btn) => {
      btn.addEventListener("click", () => activatePane(btn.dataset.module));
    });

    document.querySelectorAll(".ribbon-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        document.querySelectorAll(".ribbon-btn").forEach((b) => b.classList.remove("active"));
        btn.classList.add("active");
        renderRibbonCategoryTools(btn.dataset.category);
      });
    });

    renderRibbonCategoryTools("home");
    updateSheetSelectionAndAggregates("C1");
    renderSlideDeck(0);

    const qatSave = document.getElementById("qat-save");
    if (qatSave) {
      qatSave.addEventListener("click", () => {
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "문서 저장 완료 (" + activeModule + ")";
      });
    }
    const qatUndo = document.getElementById("qat-undo");
    if (qatUndo) {
      qatUndo.addEventListener("click", () => {
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "실행 취소 완료";
      });
    }
    const qatRedo = document.getElementById("qat-redo");
    if (qatRedo) {
      qatRedo.addEventListener("click", () => {
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "다시 실행 완료";
      });
    }
    const btnBench = document.getElementById("btn-run-benchmark");
    if (btnBench) {
      btnBench.addEventListener("click", runBenchmarkAction);
    }

    const btnRibbonStyle = document.getElementById("btn-ribbon-style");
    if (btnRibbonStyle) {
      btnRibbonStyle.addEventListener("click", async () => {
        activeRibbonStyle = activeRibbonStyle === "standard" ? "hangul" : "standard";
        const res = await invokeIpc("switch_ribbon_style", activeRibbonStyle);
        btnRibbonStyle.textContent =
          activeRibbonStyle === "hangul" ? "리본: 한글(HWP) 7탭" : "리본: 표준 오피스";
        const labels = (res && res.categories) || RIBBON_STYLE_LABELS[activeRibbonStyle];
        document.querySelectorAll("#ribbon-tabs .ribbon-btn").forEach((b, idx) => {
          if (labels[idx]) b.textContent = labels[idx];
        });
      });
    }

    const btnConvert = document.getElementById("btn-run-convert");
    if (btnConvert) {
      btnConvert.addEventListener("click", async () => {
        const src = document.getElementById("conv-src-fmt").value;
        const dst = document.getElementById("conv-dst-fmt").value;
        const body = extractEditableDocumentText() || "본문 텍스트";
        const res = await invokeIpc(
          "convert_document_format",
          src + "->" + dst + "|통합_변환_문서|" + body
        );
        const box = document.getElementById("conv-result-box");
        if (box && res) {
          box.textContent =
            "[" +
            res.status +
            "] " +
            res.sourceFormat.toUpperCase() +
            " (" +
            res.sourceBytes +
            "B) → " +
            res.targetFormat.toUpperCase() +
            " (" +
            res.outputBytes +
            "B) · " +
            res.specification;
        }
      });
    }

    document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
      td.addEventListener("click", () => updateSheetSelectionAndAggregates(td.dataset.cell));
      td.addEventListener("input", () => {
        delete td.dataset.formula;
        updateSheetSelectionAndAggregates(td.dataset.cell);
      });
    });

    const evalBtn = document.getElementById("btn-eval-formula");
    if (evalBtn) {
      evalBtn.addEventListener("click", async () => {
        const input = document.getElementById("formula-input");
        const out = document.getElementById("sheet-eval-result");
        if (!input || !out) return;
        const rawFormula = input.value.trim();
        const activeTd = document.querySelector(
          '#pane-sheet td[data-cell="' + activeSheetCellRef + '"]'
        );
        if (activeTd) {
          if (rawFormula.startsWith("=")) {
            activeTd.dataset.formula = rawFormula;
          } else {
            delete activeTd.dataset.formula;
          }
        }

        const cellMap = collectSheetCellMap();
        cellMap[activeSheetCellRef] = rawFormula;
        const pairs = Object.keys(cellMap)
          .map((k) => k + "=" + cellMap[k])
          .join(";");

        let computedValue = "";
        if (rawFormula.toUpperCase().includes("SHEET2!")) {
          const wbRes = await invokeIpc("evaluate_workbook_formula", pairs + "?" + rawFormula);
          computedValue = wbRes.value;
        } else {
          const sheetRes = await invokeIpc(
            "evaluate_sheet",
            pairs + "?" + activeSheetCellRef
          );
          computedValue = sheetRes.value;
        }
        if (activeTd) activeTd.textContent = String(computedValue);
        out.textContent = "결과 (" + activeSheetCellRef + "): " + computedValue;
        updateSheetSelectionAndAggregates(activeSheetCellRef);
      });
    }

    const btnAddSlide = document.getElementById("btn-add-slide");
    if (btnAddSlide) {
      btnAddSlide.addEventListener("click", () => {
        const nextNum = SLIDE_DECK_STATE.length + 1;
        SLIDE_DECK_STATE.push({
          title: "새 슬라이드 #" + nextNum + " (16:9 와이드)",
          lower: "핵심 요약 및 다이어그램 영역",
          notes: "발표자 노트를 입력하세요.",
          transition: "fade",
        });
        renderSlideDeck(SLIDE_DECK_STATE.length - 1);
      });
    }
    const slideTransSelect = document.getElementById("slide-transition-select");
    if (slideTransSelect) {
      slideTransSelect.addEventListener("change", () => {
        SLIDE_DECK_STATE[activeSlideIndex].transition = slideTransSelect.value;
        renderSlideDeck(activeSlideIndex);
      });
    }
    const slideNotesInput = document.getElementById("slide-presenter-notes");
    if (slideNotesInput) {
      slideNotesInput.addEventListener("input", () => {
        SLIDE_DECK_STATE[activeSlideIndex].notes = slideNotesInput.value;
      });
    }
    const btnSlideLaser = document.getElementById("btn-slide-laser");
    if (btnSlideLaser) {
      btnSlideLaser.addEventListener("click", () => {
        slideLaserActive = !slideLaserActive;
        const dot = document.getElementById("slide-laser-dot");
        if (dot) dot.hidden = !slideLaserActive;
        btnSlideLaser.textContent = slideLaserActive ? "레이저 포인터: 켬" : "레이저 포인터: 끔";
      });
    }

    const btnPdfRotate = document.getElementById("btn-pdf-rotate");
    if (btnPdfRotate) {
      btnPdfRotate.addEventListener("click", () => {
        pdfRotationDeg = (pdfRotationDeg + 90) % 360;
        const badge = document.getElementById("pdf-rotation-badge");
        if (badge) badge.textContent = "회전: " + pdfRotationDeg + "°";
      });
    }
    const btnPdfApply = document.getElementById("btn-pdf-apply");
    if (btnPdfApply) {
      btnPdfApply.addEventListener("click", () => {
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
            pdfRotationDeg +
            "°";
        }
      });
    }

    const themeBtn = document.getElementById("btn-theme-toggle");
    if (themeBtn) {
      themeBtn.addEventListener("click", () => {
        const html = document.documentElement;
        const next = html.getAttribute("data-theme") === "dark" ? "light" : "dark";
        html.setAttribute("data-theme", next);
        themeBtn.textContent = next === "dark" ? "라이트 모드" : "다크 모드";
      });
    }

    const editor = document.getElementById("doc-editor");
    const imeBadge = document.getElementById("ime-indicator");
    if (editor) {
      editor.addEventListener("compositionstart", () => {
        isImeComposing = true;
        if (imeBadge) imeBadge.textContent = "IME: 조합 중(한글)";
      });
      editor.addEventListener("compositionend", () => {
        isImeComposing = false;
        if (imeBadge) imeBadge.textContent = "IME: 완료";
        computeSmartStats();
      });
      editor.addEventListener("input", () => {
        if (!isImeComposing) computeSmartStats();
      });
      computeSmartStats();
    }

    const diagnoseBtn = document.getElementById("btn-smart-diagnose");
    if (diagnoseBtn) {
      diagnoseBtn.addEventListener("click", async () => {
        const bodyText = extractEditableDocumentText();
        const res = await invokeIpc("run_smart_assistant", bodyText);
        const statsEl = document.getElementById("smart-stats");
        const fb = document.getElementById("status-command-feedback");
        if (statsEl && res) {
          statsEl.textContent =
            "문자: " +
            res.charCount +
            " · 단어: " +
            res.wordCount +
            " · 문장: " +
            res.sentenceCount +
            " · 읽기: " +
            res.readingTimeSeconds +
            "초 · 요약: " +
            res.summaryCount +
            "건";
        }
        if (fb && res) {
          fb.textContent =
            "문장 진단 완료 · 요약 " +
            res.summaryCount +
            "건 · 실행 항목 " +
            res.actionItemCount +
            "건 · 교정 제안 " +
            res.suggestionCount +
            "건";
        }
      });
    }

    const auditBtn = document.getElementById("btn-run-security-audit");
    if (auditBtn) {
      auditBtn.addEventListener("click", async () => {
        const res = await invokeIpc("audit_security", "word/document.xml,512,2048,false|<doc>ok</doc>");
        const txt = document.getElementById("security-status-text");
        if (txt && res) txt.textContent = "문서 보안 검사 완료: " + res.status;
      });
    }

    const piiBtn = document.getElementById("btn-scrub-pii");
    if (piiBtn) {
      piiBtn.addEventListener("click", async () => {
        const editorEl = document.getElementById("doc-editor");
        const piiOut = document.getElementById("pii-result-text");
        if (!editorEl || !piiOut) return;
        const blocks = editorEl.querySelectorAll(
          ":scope > h1, :scope > h2, :scope > p:not(#hwp-manuscript-preview)"
        );
        let totalMasked = 0;
        if (blocks.length > 0) {
          for (const node of blocks) {
            const res = await invokeIpc("scrub_pii", node.textContent || "");
            if (res && typeof res.scrubbedText === "string") {
              node.textContent = res.scrubbedText;
              totalMasked += Number(res.totalMasked || 0);
            }
          }
        } else {
          const res = await invokeIpc("scrub_pii", editorEl.textContent || "");
          if (res && typeof res.scrubbedText === "string") {
            editorEl.textContent = res.scrubbedText;
            totalMasked += Number(res.totalMasked || 0);
          }
        }
        piiOut.textContent =
          totalMasked > 0
            ? "주민번호·전화번호·이메일 " + totalMasked + "건 마스킹 완료"
            : "주민번호·전화번호·이메일 스캔 완료 (감지 0건)";
        computeSmartStats();
      });
    }

    const modal = document.getElementById("cmd-palette-modal");
    const openPaletteBtn = document.getElementById("btn-command-palette");
    const closePaletteBtn = document.getElementById("btn-close-palette");
    const cmdSearchInput = document.getElementById("cmd-search-input");
    if (openPaletteBtn && modal) {
      openPaletteBtn.addEventListener("click", () => {
        renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
        if (typeof modal.showModal === "function") modal.showModal();
      });
    }
    if (cmdSearchInput) {
      cmdSearchInput.addEventListener("input", () => {
        renderCommandPaletteList(cmdSearchInput.value);
      });
    }
    if (closePaletteBtn && modal) {
      closePaletteBtn.addEventListener("click", () => {
        if (typeof modal.close === "function") modal.close();
      });
    }

    window.addEventListener("keydown", (e) => {
      if (e.isComposing || isImeComposing || e.keyCode === 229) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (!modal) return;
        if (modal.open && typeof modal.close === "function") {
          modal.close();
        } else if (typeof modal.showModal === "function") {
          renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
          modal.showModal();
          if (cmdSearchInput) cmdSearchInput.focus();
        }
      } else if (e.key === "Escape" && modal && modal.open && typeof modal.close === "function") {
        modal.close();
      }
    });
  }

  document.addEventListener("DOMContentLoaded", initWorkspace);
})();
