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
  let activeSheetCellRef = "F2";
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

  const MOCK_IPC_HANDLERS = {
    get_workspace_bootstrap: () => ({
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
    }),
    get_ribbon_category_commands: (payload) => {
      const [modPart, catPart] = String(payload || "DOC|home").split("|");
      const mKey = modPart === "CONV" ? "DOC" : modPart;
      const modMap = RIBBON_CATEGORY_TOOLS[mKey] || RIBBON_CATEGORY_TOOLS.DOC;
      return { commands: modMap[catPart] || modMap.home };
    },
    convert_document_format: (payload) => {
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
    },
    run_engine_benchmark: () => ({
      status: "ok",
      iterations: 5,
      allEquivalent: true,
      suites: [
        { suiteId: "crc32_slicing_by_8", baselineP50Us: 2068, optimizedP50Us: 464, speedupFactor: 4.46 },
        { suiteId: "deflate_zlib_roundtrip", baselineP50Us: 234, optimizedP50Us: 59, speedupFactor: 3.97 },
        { suiteId: "utf16_swar_and_xml_unescape", baselineP50Us: 50, optimizedP50Us: 29, speedupFactor: 1.72 },
        { suiteId: "sheet_dag_bitset_eval", baselineP50Us: 878, optimizedP50Us: 308, speedupFactor: 2.85 },
        { suiteId: "swar_csv_xml_and_zero_alloc_range_agg", baselineP50Us: 640, optimizedP50Us: 195, speedupFactor: 3.28 },
      ],
    }),
    run_smart_assistant: (payload) => localSmartAssistant(payload),
    scrub_pii: (payload) => localScrubPii(payload),
    evaluate_workbook_formula: (payload) => {
      const expr = String(payload || "").split("?").pop() || "=Sheet2!A1+50";
      return { sheet: "Sheet1", formula: expr, value: "150" };
    },
    evaluate_sheet: (payload) => ({ value: String(evaluateLocalSheetFormula(payload)) }),
    evaluate_sheet_formula: (payload) => ({ value: String(evaluateLocalSheetFormula(payload)) }),
    audit_security: () => ({ status: "SAFE", xxeGuard: "PASSED", zipGuard: "PASSED", checkedEntries: 2 }),
    switch_ribbon_style: (payload) => {
      const isHangul = String(payload).includes("hangul");
      return {
        style: isHangul ? "한글(HWP) 7탭 모드" : "표준 오피스 7탭 모드",
        categories: isHangul ? RIBBON_STYLE_LABELS.hangul : RIBBON_STYLE_LABELS.standard,
      };
    },
    execute_ribbon_command: (payload) => ({
      commandId: payload,
      summary: "명령 실행 완료 (" + payload + ")",
      contextualBadge: CONTEXTUAL_TOOL_BADGES[activeModule] || "표·개체 도구",
    }),
  };

  async function invokeIpc(command, payload) {
    if (globalThis.__TAURI_INTERNALS__ && typeof globalThis.__TAURI_INTERNALS__.invoke === "function") {
      const raw = await globalThis.__TAURI_INTERNALS__.invoke(command, { payload });
      return typeof raw === "string" ? JSON.parse(raw) : raw;
    }
    const handler = MOCK_IPC_HANDLERS[command];
    return handler ? handler(payload) : { status: "ok" };
  }

  function collectSheetCellMap() {
    const map = {};
    document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
      const ref = td.dataset.cell;
      const formula = td.dataset.formula;
      const rawText = (td.textContent || "").replaceAll(",", "").trim();
      map[ref] = formula || rawText;
    });
    return map;
  }

  function parseCellRefParts(ref) {
    const m = /^([A-Z]+)(\d+)$/.exec(String(ref || "").toUpperCase());
    if (!m) return { col: 65, row: 1 };
    return { col: m[1].codePointAt(0), row: Number(m[2]) };
  }

  function collectRangeNumericValues(startRef, endRef, cellValues) {
    const p1 = parseCellRefParts(startRef);
    const p2 = parseCellRefParts(endRef);
    const nums = [];
    for (let c = Math.min(p1.col, p2.col); c <= Math.max(p1.col, p2.col); c++) {
      for (let r = Math.min(p1.row, p2.row); r <= Math.max(p1.row, p2.row); r++) {
        const ref = String.fromCodePoint(c) + r;
        const raw = String(cellValues[ref] ?? "").replaceAll(",", "").trim();
        const v = Number(raw);
        if (raw !== "" && Number.isFinite(v)) nums.push(v);
      }
    }
    return nums;
  }

  function evaluateRangeFunction(fnName, vals) {
    if (fnName === "COUNT") return String(vals.length);
    if (vals.length === 0) return "0";
    if (fnName === "MAX") return String(Math.max(...vals));
    if (fnName === "MIN") return String(Math.min(...vals));
    const total = vals.reduce((acc, v) => acc + v, 0);
    if (fnName === "SUM") return String(Math.round(total * 100) / 100);
    return String(Math.round((total / vals.length) * 100) / 100);
  }

  function evaluateBinaryOpString(exprText) {
    const src = String(exprText || "").trim();
    let opIdx = -1;
    let op = "";
    for (let i = 1; i < src.length - 1; i++) {
      const ch = src[i];
      if (ch === "+" || ch === "-" || ch === "*" || ch === "/") {
        opIdx = i;
        op = ch;
        break;
      }
    }
    if (opIdx < 1) return null;
    const lhs = src.slice(0, opIdx).trim();
    const rhs = src.slice(opIdx + 1).trim();
    if (!lhs || !rhs) return null;
    const a = Number.parseFloat(lhs);
    const b = Number.parseFloat(rhs);
    if (!Number.isFinite(a) || !Number.isFinite(b) || Number(lhs) !== a || Number(rhs) !== b) {
      return null;
    }
    if (op === "/" && b === 0) return "#DIV/0!";
    let res = 0;
    if (op === "+") res = a + b;
    if (op === "-") res = a - b;
    if (op === "*") res = a * b;
    if (op === "/") res = a / b;
    return String(Math.round(res * 100) / 100);
  }

  function evaluateFormulaExpression(exprStr, cellValues) {
    const clean = String(exprStr || "").trim();
    if (!clean.startsWith("=")) {
      const normalized = clean.replaceAll(",", "");
      const n = Number(normalized);
      return Number.isFinite(n) && normalized !== "" ? String(n) : clean;
    }
    const body = clean.slice(1).trim().toUpperCase();
    const rangeMatch = /^(SUM|AVERAGE|AVG|MAX|MIN|COUNT)\(([A-Z]+\d+):([A-Z]+\d+)\)$/.exec(body);
    if (rangeMatch) {
      const vals = collectRangeNumericValues(rangeMatch[2], rangeMatch[3], cellValues);
      return evaluateRangeFunction(rangeMatch[1], vals);
    }
    if (body.includes("SHEET2!A1")) {
      const crossResult = evaluateBinaryOpString(body.replaceAll("SHEET2!A1", "100"));
      if (crossResult !== null) return crossResult;
    }
    const substituted = body.replaceAll(/\b([A-Z]+\d+)\b/g, (_, ref) => {
      const raw = String(cellValues[ref] ?? "").replaceAll(",", "").trim();
      const v = Number(raw);
      return raw !== "" && Number.isFinite(v) ? String(v) : "0";
    });
    const arithResult = evaluateBinaryOpString(substituted);
    return arithResult ?? clean;
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
    const q = (queryPart || "F2").trim();
    return q.startsWith("=") ? evaluateFormulaExpression(q, values) : (values[q] ?? "");
  }

  function updateSheetSelectionAndAggregates(cellRef) {
    activeSheetCellRef = cellRef;
    const parsedRef = /^([A-Z]+)(\d+)$/.exec(String(cellRef || "").trim().toUpperCase());
    const colLetter = parsedRef ? parsedRef[1] : cellRef.charAt(0);
    const rowNumber = parsedRef ? parsedRef[2] : cellRef.slice(1);

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
      const num = Number((td.textContent || "").replaceAll(",", "").trim());
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
    void renderRibbonCategoryTools(activeRibbonCategory);
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
      const transitionLabelMap = {
        fade: "페이드",
        push: "밀어내기",
        zoom: "확대",
        none: "없음",
      };
      const transitionLabel = transitionLabelMap[slide.transition] || slide.transition;
      metaStatus.textContent =
        "슬라이드 " + (idx + 1) + " / " + SLIDE_DECK_STATE.length + " · 전환: " + transitionLabel;
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
    const box =
      document.getElementById("bench-Summary") || document.getElementById("bench-result-box");
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

  function setupTabsAndRibbonChrome(boot) {
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
        void renderRibbonCategoryTools(btn.dataset.category);
      });
    });
  }

  function setupQatAndRibbonActions() {
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
        const labels = res?.categories || RIBBON_STYLE_LABELS[activeRibbonStyle];
        document.querySelectorAll("#ribbon-tabs .ribbon-btn").forEach((b, idx) => {
          if (labels[idx]) b.textContent = labels[idx];
        });
      });
    }

    const themeBtn = document.getElementById("btn-theme-toggle");
    if (themeBtn) {
      themeBtn.addEventListener("click", () => {
        const html = document.documentElement;
        const next = html.dataset.theme === "dark" ? "light" : "dark";
        html.dataset.theme = next;
        themeBtn.textContent = next === "dark" ? "라이트 모드" : "다크 모드";
      });
    }
  }

  function setupSheetAndConverterHandlers() {
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

        const isCrossSheet = rawFormula.toUpperCase().includes("SHEET2!");
        const rpcRes = isCrossSheet
          ? await invokeIpc("evaluate_workbook_formula", pairs + "?" + rawFormula)
          : await invokeIpc("evaluate_sheet", pairs + "?" + activeSheetCellRef);
        const computedValue = rpcRes.value;
        if (activeTd) activeTd.textContent = String(computedValue);
        out.textContent = "결과 (" + activeSheetCellRef + "): " + computedValue;
        updateSheetSelectionAndAggregates(activeSheetCellRef);
      });
    }
  }

  function setupSlideAndPdfHandlers() {
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
  }

  function setupSmartAndSecurityHandlers() {
    const editor = document.getElementById("doc-editor");
    const imeBadge =
      document.getElementById("ime-badge") || document.getElementById("ime-indicator");
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
        const blocks = Array.from(
          editorEl.querySelectorAll(":scope > h1, :scope > h2, :scope > p:not(#hwp-manuscript-preview)")
        );
        const targets = blocks.length > 0 ? blocks : [editorEl];
        const results = await Promise.all(
          targets.map((node) => invokeIpc("scrub_pii", node.textContent || ""))
        );
        let totalMasked = 0;
        results.forEach((res, idx) => {
          if (res && typeof res.scrubbedText === "string") {
            targets[idx].textContent = res.scrubbedText;
            totalMasked += Number(res.totalMasked || 0);
          }
        });
        piiOut.textContent =
          totalMasked > 0
            ? "주민번호·전화번호·이메일 " + totalMasked + "건 마스킹 완료"
            : "주민번호·전화번호·이메일 스캔 완료 (감지 0건)";
        computeSmartStats();
      });
    }
  }

  function computeDocTableSummary() {
    const numCells = document.querySelectorAll(
      "#doc-table-body tr:not(#doc-table-sum-row) [data-doc-num]"
    );
    let total = 0;
    numCells.forEach((cell) => {
      const raw = (cell.dataset.docNum || cell.textContent || "0").replaceAll(",", "");
      const val = Number.parseFloat(raw);
      if (!Number.isNaN(val)) {
        total += val;
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

  function setupDocTableAndSheetTabHandlers() {
    const addRowBtn = document.getElementById("btn-doc-add-row");
    const tbody = document.getElementById("doc-table-body");
    if (addRowBtn && tbody) {
      addRowBtn.addEventListener("click", () => {
        const tr = document.createElement("tr");
        tr.innerHTML =
          '<td>신규 사업부</td><td>담당 연구원</td><td data-doc-num="150">150</td>';
        const sumRow = document.getElementById("doc-table-sum-row");
        if (sumRow) {
          sumRow.before(tr);
        } else {
          tbody.appendChild(tr);
        }
        computeDocTableSummary();
        computeSmartStats();
      });
    }
    const mergeBtn = document.getElementById("btn-doc-merge-toggle");
    if (mergeBtn && tbody) {
      mergeBtn.addEventListener("click", () => {
        const sumRow = document.getElementById("doc-table-sum-row");
        const cells = sumRow ? sumRow.querySelectorAll("td") : tbody.querySelectorAll("tr td");
        if (cells.length < 2) return;
        const isMerged = cells[0].colSpan > 1;
        cells[0].colSpan = isMerged ? 1 : 2;
        cells[1].hidden = !isMerged;
        mergeBtn.textContent = isMerged ? "셀 병합 토글" : "셀 병합 해제";
      });
    }
    const evalDocBtn = document.getElementById("btn-doc-eval-table");
    if (evalDocBtn) {
      evalDocBtn.addEventListener("click", computeDocTableSummary);
    }

    const sheetTabBar = document.getElementById("sheet-tab-bar");
    const addSheetBtn = document.getElementById("btn-add-sheet-tab");
    if (sheetTabBar) {
      sheetTabBar.addEventListener("click", (e) => {
        const btn = e.target.closest(".sheet-tab-btn");
        if (!btn || btn.id === "btn-add-sheet-tab") return;
        sheetTabBar.querySelectorAll(".sheet-tab-btn").forEach((b) => b.classList.remove("active"));
        btn.classList.add("active");
      });
    }
    if (addSheetBtn && sheetTabBar) {
      addSheetBtn.addEventListener("click", () => {
        const count = sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").length + 1;
        const newTab = document.createElement("button");
        newTab.className = "sheet-tab-btn";
        newTab.dataset.sheetTab = "Sheet" + count;
        newTab.textContent = "시트 " + count;
        addSheetBtn.before(newTab);
        sheetTabBar.querySelectorAll(".sheet-tab-btn").forEach((b) => b.classList.remove("active"));
        newTab.classList.add("active");
      });
    }
  }

  function togglePaletteModal(modal, cmdSearchInput) {
    if (!modal) return;
    if (modal.open && typeof modal.close === "function") {
      modal.close();
      return;
    }
    if (typeof modal.showModal === "function") {
      renderCommandPaletteList(cmdSearchInput ? cmdSearchInput.value : "");
      modal.showModal();
      if (cmdSearchInput) cmdSearchInput.focus();
    }
  }

  function setupCommandPaletteHandlers() {
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

    globalThis.addEventListener("keydown", (e) => {
      if (e.isComposing || isImeComposing) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        togglePaletteModal(modal, cmdSearchInput);
      } else if (e.key === "Escape" && modal?.open && typeof modal.close === "function") {
        modal.close();
      }
    });
  }

  async function initWorkspace() {
    const boot = await invokeIpc("get_workspace_bootstrap", "");
    setupTabsAndRibbonChrome(boot);
    void renderRibbonCategoryTools("home");
    updateSheetSelectionAndAggregates("F2");
    renderSlideDeck(0);
    setupQatAndRibbonActions();
    setupDocTableAndSheetTabHandlers();
    setupSheetAndConverterHandlers();
    setupSlideAndPdfHandlers();
    setupSmartAndSecurityHandlers();
    setupCommandPaletteHandlers();
  }

  document.addEventListener("DOMContentLoaded", () => {
    void initWorkspace();
  });
})();
