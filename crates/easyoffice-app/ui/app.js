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
    hangul: ["편집", "입력", "쪽·원고지", "표·개체", "시트·차트", "보안·도구", "보기"],
  };

  const RIBBON_CATEGORY_ORDER = [
    "home",
    "insert",
    "layout",
    "formulasAndData",
    "moduleSpecific",
    "review",
    "view",
  ];

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

  const MODULE_SHORT_BADGES = {
    DOC: "문서",
    HWP: "한글",
    SHEET: "시트",
    SLIDE: "슬라이드",
    PDF: "PDF",
    CONV: "변환",
  };

  let isImeComposing = false;
  let activeModule = "DOC";
  let activeRibbonStyle = "standard";
  let activeRibbonCategory = "home";
  let activeSheetName = "Sheet1";
  let activeSheetCellRef = "F2";
  let activeSlideIndex = 0;
  let slideLaserActive = false;
  let pdfRotationDeg = 0;
  let activeDocBlockEl = null;

  const workspaceUndoStack = [];
  const workspaceRedoStack = [];
  const MAX_UNDO_SNAPSHOTS = 64;

  const docTabStore = {
    DOC: null,
    HWP: null,
  };

  const localRibbonState = {
    isBold: false,
    isItalic: false,
    isUnderline: false,
    stylePresetIndex: 0,
    paperSize: "A4",
    columnCount: 1,
    isLandscape: false,
    marginPresetIndex: 0,
    sortAscending: true,
    manuscriptGridEnabled: false,
    approvalHeaderEnabled: true,
    doubleDiagonalBorderEnabled: false,
    trackChangesEnabled: false,
    splitViewEnabled: false,
    ribbonCollapsed: false,
    darkThemeEnabled: false,
    zoomPercent: 100,
    pdfRotationDegrees: 0,
    executedCount: 0,
  };

  const sheetDataStore = {
    Sheet1: null,
    Sheet2: {
      A1: { value: "100", formula: "" },
      B1: { value: "250", formula: "" },
      C1: { value: "350", formula: "=SUM(A1:B1)" },
      A2: { value: "기획조정실", formula: "" },
      B2: { value: "1200", formula: "" },
      C2: { value: "1300", formula: "=B2+A1" },
    },
  };

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
      .replace(/\b\d{6}[1-4]\d{6}\b/g, () => {
        residentCount += 1;
        return "*************";
      })
      .replace(
        /(?:\+82[-\s]?10|01[016789]|02|0[3-6][1-5])[-\s]\d{3,4}[-\s]\d{4}\b/g,
        (match) => {
          phoneCount += 1;
          if (match.startsWith("+82")) return "+82-10-****-****";
          if (match.startsWith("02")) return "02-****-****";
          return "010-****-****";
        }
      )
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
    const readingTimeSeconds = Math.max(1, Math.ceil((words / 220) * 60));
    const lines = text
      ? text
          .split(/\n+/)
          .map((l) => l.trim())
          .filter((l) => l.length > 0)
      : [];
    const summaryLines = lines
      .slice()
      .sort((a, b) => b.length - a.length)
      .slice(0, 2);
    const actionItems = lines.filter(
      (l) =>
        l.startsWith("- [ ]") ||
        l.startsWith("[TODO]") ||
        l.startsWith("TODO:") ||
        l.startsWith("할일:") ||
        l.startsWith("조치:")
    );
    const styleSuggestions = [];
    if (lines.some((l) => Array.from(l).length > 120)) {
      styleSuggestions.push("120자를 초과하는 긴 문장이 있습니다. 두 문장으로 나누세요.");
    }
    return {
      summaryCount: summaryLines.length,
      charCount: chars,
      wordCount: words,
      sentenceCount: sentences,
      readingTimeSeconds,
      actionItemCount: actionItems.length,
      suggestionCount: styleSuggestions.length,
      summaryLines,
      styleSuggestions,
      actionItems,
    };
  }

  function localAuditSecurity(payload) {
    const raw = String(payload || "");
    const pipeIdx = raw.indexOf("|");
    const entriesPart = pipeIdx >= 0 ? raw.slice(0, pipeIdx) : "";
    const xmlPart = pipeIdx >= 0 ? raw.slice(pipeIdx + 1) : raw;
    const upperXml = (xmlPart || "").toUpperCase();
    if (upperXml.includes("<!DOCTYPE") || upperXml.includes("<!ENTITY")) {
      return {
        status: "REJECTED",
        xxeGuard: "BLOCKED",
        zipGuard: "PASSED",
        checkedEntries: 0,
      };
    }
    const entries = (entriesPart || "")
      .split(";")
      .map((s) => s.trim())
      .filter(Boolean);
    for (const item of entries) {
      const cols = item.split(",").map((c) => c.trim());
      const entryPath = cols[0] || "";
      const comp = Number(cols[1] || 1);
      const uncomp = Number(cols[2] || 0);
      const enc = cols[3] === "true" || cols[3] === "1";
      if (
        entryPath.startsWith("/") ||
        entryPath.includes("..") ||
        entryPath.includes("\\") ||
        enc ||
        (comp > 0 && uncomp / comp > 100)
      ) {
        return {
          status: "REJECTED",
          xxeGuard: "PASSED",
          zipGuard: "BLOCKED",
          checkedEntries: entries.length,
        };
      }
    }
    return {
      status: "SAFE",
      xxeGuard: "PASSED",
      zipGuard: "PASSED",
      checkedEntries: entries.length || 1,
    };
  }

  function localExecuteRibbonCommand(payload) {
    const cmdId = String(payload || "").trim();
    localRibbonState.executedCount += 1;
    if (cmdId.endsWith("-home-bold")) {
      localRibbonState.isBold = !localRibbonState.isBold;
    } else if (cmdId.endsWith("-home-italic")) {
      localRibbonState.isItalic = !localRibbonState.isItalic;
    } else if (cmdId.endsWith("-home-underline")) {
      localRibbonState.isUnderline = !localRibbonState.isUnderline;
    } else if (cmdId.endsWith("-home-style")) {
      localRibbonState.stylePresetIndex = (localRibbonState.stylePresetIndex + 1) % 4;
    } else if (cmdId.endsWith("-layout-paper")) {
      localRibbonState.paperSize = localRibbonState.paperSize === "A4" ? "Letter" : "A4";
    } else if (cmdId.endsWith("-layout-orientation")) {
      localRibbonState.isLandscape = !localRibbonState.isLandscape;
    } else if (cmdId.endsWith("-layout-columns")) {
      localRibbonState.columnCount =
        localRibbonState.columnCount >= 3 ? 1 : localRibbonState.columnCount + 1;
    } else if (cmdId.endsWith("-layout-margins")) {
      localRibbonState.marginPresetIndex = (localRibbonState.marginPresetIndex + 1) % 3;
    } else if (cmdId.endsWith("-data-sort")) {
      localRibbonState.sortAscending = !localRibbonState.sortAscending;
    } else if (cmdId === "hwp-mod-manuscript") {
      localRibbonState.manuscriptGridEnabled = !localRibbonState.manuscriptGridEnabled;
    } else if (cmdId === "hwp-mod-header") {
      localRibbonState.approvalHeaderEnabled = !localRibbonState.approvalHeaderEnabled;
    } else if (cmdId === "hwp-mod-border") {
      localRibbonState.doubleDiagonalBorderEnabled = !localRibbonState.doubleDiagonalBorderEnabled;
    } else if (cmdId.endsWith("-review-track")) {
      localRibbonState.trackChangesEnabled = !localRibbonState.trackChangesEnabled;
    } else if (cmdId.endsWith("-view-split")) {
      localRibbonState.splitViewEnabled = !localRibbonState.splitViewEnabled;
    } else if (cmdId.endsWith("-view-ribbon")) {
      localRibbonState.ribbonCollapsed = !localRibbonState.ribbonCollapsed;
    } else if (cmdId.endsWith("-view-theme")) {
      localRibbonState.darkThemeEnabled = !localRibbonState.darkThemeEnabled;
    } else if (cmdId.endsWith("-view-zoom")) {
      localRibbonState.zoomPercent =
        localRibbonState.zoomPercent === 100
          ? 125
          : localRibbonState.zoomPercent === 125
            ? 150
            : localRibbonState.zoomPercent === 150
              ? 90
              : 100;
    } else if (cmdId === "pdf-mod-rotate") {
      localRibbonState.pdfRotationDegrees = (localRibbonState.pdfRotationDegrees + 90) % 360;
    }
    return {
      commandId: cmdId,
      summary: "명령 실행 완료 (" + cmdId + ")",
      contextualBadge: CONTEXTUAL_TOOL_BADGES[activeModule] || "표·개체 도구",
      catalogCount: 140,
      ...localRibbonState,
    };
  }

  function encodeUtf8Base64(text) {
    const bytes = new TextEncoder().encode(String(text || ""));
    let binary = "";
    for (let i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return typeof globalThis.btoa === "function" ? globalThis.btoa(binary) : "";
  }

  const MOCK_IPC_HANDLERS = {
    get_workspace_bootstrap: () => ({
      platform: navigator.platform.includes("Mac") ? "macOS" : "Desktop",
      shortcutModifier: navigator.platform.includes("Mac") ? "⌘" : "Ctrl",
      recentFormatFilters: ["전체", "DOCX", "HWP/HWPX", "XLSX", "PPTX", "PDF", "ODF"],
      openTabs: [
        { id: "tab-doc", title: "분기_경영실적_보고서.docx", module: "DOC", isPinned: true },
        { id: "tab-hwp", title: "행정안전부_기안문.hwpx", module: "HWP", isPinned: true },
        { id: "tab-sheet", title: "예산집행_총괄표.xlsx", module: "SHEET", isPinned: false },
        { id: "tab-slide", title: "전략기획_발표자료.pptx", module: "SLIDE", isPinned: false },
        { id: "tab-pdf", title: "최종계약서_검토본.pdf", module: "PDF", isPinned: false },
      ],
    }),
    get_ribbon_category_commands: (payload) => {
      const [modPart, catPart] = String(payload || "DOC|home").split("|");
      const mKey = modPart === "CONV" ? "DOC" : modPart;
      const modMap = RIBBON_CATEGORY_TOOLS[mKey] || RIBBON_CATEGORY_TOOLS.DOC;
      return { commands: modMap[catPart] || modMap.home };
    },
    convert_document_format: (payload) => {
      const raw = String(payload || "docx->hwpx|문서|본문");
      const firstPipe = raw.indexOf("|");
      const firstSeg = firstPipe >= 0 ? raw.slice(0, firstPipe) : raw;
      const restSeg = firstPipe >= 0 ? raw.slice(firstPipe + 1) : "통합_변환_문서|본문";
      const secondPipe = restSeg.indexOf("|");
      const [src, dst] = (firstSeg || "docx->hwpx").split("->");
      const title = (secondPipe >= 0 ? restSeg.slice(0, secondPipe) : restSeg) || "통합_변환_문서";
      const body = (secondPipe >= 0 ? restSeg.slice(secondPipe + 1) : "") || "본문 텍스트";
      const encoder = new TextEncoder();
      const rawBytes = encoder.encode(title + "\n" + body).length;
      const containerOverhead = {
        docx: 640,
        doc: 1024,
        hwpx: 720,
        hwtx: 720,
        hwp: 1024,
        hwt: 1024,
        odt: 600,
        xlsx: 680,
        xls: 1024,
        ods: 620,
        pptx: 760,
        ppsx: 760,
        ppt: 1024,
        odp: 640,
        pdf: 480,
        rtf: 180,
        md: 16,
        csv: 24,
        tsv: 24,
        txt: 0,
      };
      if (!(src in containerOverhead) || !(dst in containerOverhead)) {
        throw new Error("지원하지 않는 포맷: " + src + "->" + dst);
      }
      const srcBytes = rawBytes + (containerOverhead[src] ?? 256);
      const dstBytes = rawBytes + (containerOverhead[dst] ?? 256);
      const b64 = encodeUtf8Base64(title + "\n" + body);
      const tableCount = body.includes("[[TABLE_ROW]]\t")
        ? Math.max(1, (body.match(/\[\[TABLE_NEW\]\]/g) || []).length)
        : 0;
      return {
        sourceFormat: src,
        targetFormat: dst,
        sourceBytes: srcBytes,
        outputBytes: dstBytes,
        targetBytes: dstBytes,
        detectedTarget: dst,
        tableCount,
        specification: MODULE_SPECS[(dst || "").toUpperCase()] || "표준 문서 포맷",
        converted_base64: b64,
        convertedBase64: b64,
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
    evaluate_workbook_formula: (payload) => evaluateLocalWorkbookFormulaRpc(payload),
    evaluate_sheet: (payload) => evaluateLocalSheetRpc(payload),
    evaluate_sheet_formula: (payload) => ({
      value: String(evaluateLocalSheetFormula(payload, false)),
    }),
    audit_security: (payload) => localAuditSecurity(payload),
    switch_ribbon_style: (payload) => {
      const isHangul = String(payload).includes("hangul");
      return {
        style: isHangul ? "한글(HWP) 7탭 모드" : "표준 오피스 7탭 모드",
        categories: isHangul ? RIBBON_STYLE_LABELS.hangul : RIBBON_STYLE_LABELS.standard,
      };
    },
    execute_ribbon_command: (payload) => localExecuteRibbonCommand(payload),
    undo_ribbon_command: () => ({
      commandId: "undo",
      summary: "실행 취소 완료",
      ...localRibbonState,
    }),
    redo_ribbon_command: () => ({
      commandId: "redo",
      summary: "다시 실행 완료",
      ...localRibbonState,
    }),
  };

  async function invokeIpc(command, payload) {
    if (globalThis.__TAURI_INTERNALS__ && typeof globalThis.__TAURI_INTERNALS__.invoke === "function") {
      const raw = await globalThis.__TAURI_INTERNALS__.invoke(command, { payload });
      return typeof raw === "string" ? JSON.parse(raw) : raw;
    }
    if (isHttpIpcTransportAvailable()) {
      return await invokeHttpIpc(command, payload);
    }
    const handler = MOCK_IPC_HANDLERS[command];
    return handler ? handler(payload) : { status: "ok" };
  }

  function isHttpIpcTransportAvailable() {
    const proto = globalThis.location?.protocol;
    return (proto === "http:" || proto === "https:") && typeof globalThis.fetch === "function";
  }

  function reportIpcFailure(command, msg) {
    const fb = document.getElementById("status-command-feedback");
    if (fb) fb.textContent = "[요청 거부] " + msg;
    let targetId = null;
    if (command === "convert_document_format") {
      targetId = "conv-result-box";
    } else if (command === "audit_security") {
      targetId = "security-status-text";
    }
    const targetEl = targetId ? document.getElementById(targetId) : null;
    if (targetEl) targetEl.textContent = "[요청 거부] " + msg;
  }

  async function invokeHttpIpc(command, payload) {
    let resp;
    try {
      resp = await globalThis.fetch("/api/ipc", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ command, payload: String(payload ?? "") }),
      });
    } catch (netErr) {
      const msg = "백엔드 IPC 네트워크 오류: " + (netErr?.message || String(netErr));
      reportIpcFailure(command, msg);
      throw new Error(msg);
    }
    if (!resp.ok) {
      const errBody = await resp.json().catch(() => ({}));
      const msg = errBody.error || errBody.message || "IPC request failed (" + resp.status + ")";
      reportIpcFailure(command, msg);
      throw new Error(msg);
    }
    return await resp.json();
  }

  function snapshotDomSheetCells() {
    const snap = {};
    document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
      const ref = td.dataset.cell;
      snap[ref] = {
        value: (td.textContent || "").trim(),
        formula: td.dataset.formula || "",
      };
    });
    return snap;
  }

  const sheetCellFocusState = new WeakMap();
  let latestSheetRecalcSeq = 0;
  let sheetSnapshotEpoch = 0;

  function syncSheetCellFocusBaseline(td, clearPending) {
    const st = sheetCellFocusState.get(td);
    if (!st) return;
    const text = (td.textContent || "").trim();
    st.initialText = text;
    st.initialRaw = td.dataset.formula || text;
    if (clearPending) {
      st.pendingSnapshot = null;
      st.snapshotPushed = false;
      st.userEdited = false;
      st.sheetName = activeSheetName;
    }
  }

  function refreshFocusedSheetCellPendingSnapshot() {
    const activeEl = document.activeElement;
    const st = activeEl ? sheetCellFocusState.get(activeEl) : null;
    if (st && !st.userEdited && !st.snapshotPushed) {
      st.sheetName = activeSheetName;
      st.pendingSnapshot = captureWorkspaceSnapshot(false);
    }
  }

  function restoreDomSheetCells(sheetName) {
    if (!sheetDataStore.Sheet1) {
      sheetDataStore.Sheet1 = snapshotDomSheetCells();
    }
    const targetSnap = sheetDataStore[sheetName] || {};
    document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
      const ref = td.dataset.cell;
      const entry = targetSnap[ref];
      if (entry) {
        td.textContent = entry.value || "";
        if (entry.formula) {
          td.dataset.formula = entry.formula;
        } else {
          delete td.dataset.formula;
        }
      } else {
        td.textContent = "";
        delete td.dataset.formula;
      }
      syncSheetCellFocusBaseline(td, true);
    });
  }

  function normalizeFormattedNumericCell(raw) {
    const trimmed = String(raw || "").trim();
    if (/^-?\d{1,3}(,\d{3})+(\.\d+)?$/.test(trimmed)) {
      return trimmed.replaceAll(",", "");
    }
    return trimmed;
  }

  function escapeSemicolonToken(value) {
    return String(value || "")
      .replaceAll("\\", "\\\\")
      .replaceAll(";", String.raw`\;`)
      .replaceAll("?", String.raw`\?`);
  }

  function splitUnescapedQueryJs(raw) {
    const str = String(raw || "");
    for (let i = str.length - 1; i >= 0; i--) {
      if (str[i] === "?") {
        let bs = 0;
        for (let k = i - 1; k >= 0 && str[k] === "\\"; k--) {
          bs += 1;
        }
        if (bs % 2 === 0) {
          return { cellsPart: str.slice(0, i), queryPart: str.slice(i + 1) };
        }
      }
    }
    return null;
  }

  function isCellKeyPrefixJs(str) {
    const trimmed = String(str || "").trimStart();
    const eqIdx = trimmed.indexOf("=");
    if (eqIdx <= 0 || eqIdx > 64) return false;
    const lhs = trimmed.slice(0, eqIdx).trim();
    const bangIdx = lhs.indexOf("!");
    const refPart = bangIdx >= 0 ? lhs.slice(bangIdx + 1).trim() : lhs;
    return /^[A-Za-z]{1,3}\d+$/.test(refPart);
  }

  function splitEscapedCellAssignmentsJs(cellsPart) {
    const src = String(cellsPart || "");
    const items = [];
    let current = "";
    let seenEq = false;
    let rhsStarted = false;
    let inFormula = false;
    let inQuotes = false;
    let parenDepth = 0;

    for (let i = 0; i < src.length; i++) {
      const ch = src[i];
      if (ch === "\\" && i + 1 < src.length) {
        const nextCh = src[i + 1];
        if (nextCh === ";" || nextCh === "?" || nextCh === "\\") {
          if (seenEq && !rhsStarted && nextCh.trim() !== "") {
            rhsStarted = true;
          }
          current += nextCh;
          i += 1;
          continue;
        }
      }
      if (!seenEq && ch === "=") {
        seenEq = true;
        current += ch;
        continue;
      }
      if (seenEq && !rhsStarted && ch.trim() !== "") {
        rhsStarted = true;
        if (ch === "=") {
          inFormula = true;
        }
      }
      if (inFormula && ch === '"') {
        inQuotes = !inQuotes;
        current += ch;
        continue;
      }
      if (inFormula && !inQuotes) {
        if (ch === "(") parenDepth += 1;
        else if (ch === ")" && parenDepth > 0) parenDepth -= 1;
      }
      const canSplit = inFormula ? !inQuotes && parenDepth === 0 : true;
      if (ch === ";" && canSplit) {
        const tail = src.slice(i + 1);
        if (tail.trim() === "" || isCellKeyPrefixJs(tail)) {
          const eqPos = current.indexOf("=");
          if (eqPos > 0) {
            items.push({
              key: current.slice(0, eqPos).trim(),
              value: current
                .slice(eqPos + 1)
                .trim()
                .replaceAll("%3B", ";")
                .replaceAll("%3F", "?"),
            });
          }
          current = "";
          seenEq = false;
          rhsStarted = false;
          inFormula = false;
          inQuotes = false;
          parenDepth = 0;
          continue;
        }
      }
      current += ch;
    }
    const lastEqPos = current.indexOf("=");
    if (lastEqPos > 0) {
      items.push({
        key: current.slice(0, lastEqPos).trim(),
        value: current
          .slice(lastEqPos + 1)
          .trim()
          .replaceAll("%3B", ";")
          .replaceAll("%3F", "?"),
      });
    }
    return items;
  }

  function normalizeSheetTokenJs(raw) {
    return String(raw || "")
      .trim()
      .replaceAll(/^'+|'+$/g, "")
      .trim();
  }

  function extractActiveSheetAndExprJs(rawExpr, defaultSheet = "Sheet1") {
    const trimmed = String(rawExpr || "").trim();
    const pipeIdx = trimmed.indexOf("|");
    if (pipeIdx > 0) {
      const left = trimmed.slice(0, pipeIdx).trim();
      if (left && !left.startsWith("=") && !left.includes("(")) {
        const norm = normalizeSheetTokenJs(left);
        return { sheet: norm || defaultSheet, expr: trimmed.slice(pipeIdx + 1).trim() };
      }
    }
    const bangIdx = trimmed.indexOf("!");
    if (bangIdx > 0) {
      const left = trimmed.slice(0, bangIdx).trim();
      if (left && !left.startsWith("=") && !left.includes("(")) {
        const norm = normalizeSheetTokenJs(left);
        return { sheet: norm || defaultSheet, expr: trimmed.slice(bangIdx + 1).trim() };
      }
    }
    return { sheet: defaultSheet, expr: trimmed };
  }

  function collectSheetCellMap() {
    sheetDataStore[activeSheetName] = snapshotDomSheetCells();
    const map = {};
    Object.keys(sheetDataStore).forEach((shName) => {
      const snap = sheetDataStore[shName] || {};
      Object.keys(snap).forEach((ref) => {
        const entry = snap[ref];
        if (entry && (entry.formula || entry.value)) {
          const val = entry.formula
            ? entry.formula
            : normalizeFormattedNumericCell(entry.value);
          map[shName + "!" + ref] = escapeSemicolonToken(val);
        }
      });
    });
    document.querySelectorAll("#pane-sheet td[data-cell]").forEach((td) => {
      const ref = td.dataset.cell;
      const formula = td.dataset.formula;
      const rawText = normalizeFormattedNumericCell(td.textContent || "");
      const val = formula || rawText;
      map[activeSheetName + "!" + ref] = escapeSemicolonToken(val);
    });
    return map;
  }

  function parseCellRefParts(ref) {
    const m = /^([A-Z]+)(\d+)$/.exec(String(ref || "").toUpperCase());
    if (!m) return { col: 0, row: 1 };
    let col = 0;
    for (let i = 0; i < m[1].length; i++) {
      col = col * 26 + (m[1].codePointAt(i) - 64);
    }
    return { col: col - 1, row: Number(m[2]) };
  }

  function colIndexToLetters(colIdx) {
    let n = Math.max(0, Number(colIdx) || 0) + 1;
    let out = "";
    while (n > 0) {
      const rem = (n - 1) % 26;
      out = String.fromCodePoint(65 + rem) + out;
      n = Math.floor((n - 1) / 26);
    }
    return out || "A";
  }

  function collectRangeNumericValues(startRef, endRef, cellValues, currentSheet) {
    const shPrefix = String(currentSheet || "SHEET1").toUpperCase() + "!";
    const p1 = parseCellRefParts(startRef);
    const p2 = parseCellRefParts(endRef);
    const nums = [];
    for (let c = Math.min(p1.col, p2.col); c <= Math.max(p1.col, p2.col); c++) {
      for (let r = Math.min(p1.row, p2.row); r <= Math.max(p1.row, p2.row); r++) {
        const ref = colIndexToLetters(c) + r;
        const raw = String(cellValues[shPrefix + ref] ?? "")
          .replaceAll(",", "")
          .trim();
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
    if (fnName === "STDEV" || fnName === "VAR") {
      if (vals.length < 2) return "#DIV/0!";
      const mean = total / vals.length;
      const m2 = vals.reduce((acc, v) => acc + (v - mean) * (v - mean), 0);
      const variance = m2 / (vals.length - 1);
      const out = fnName === "STDEV" ? Math.sqrt(variance) : variance;
      return String(Math.round(out * 100) / 100);
    }
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

  function evaluateFormulaExpression(exprStr, cellValues, currentSheet, currentCellRef) {
    const shUpper = String(currentSheet || "SHEET1").toUpperCase();
    const clean = String(exprStr || "").trim();
    if (!clean.startsWith("=")) {
      const normalized = clean.replaceAll(",", "");
      const n = Number(normalized);
      return Number.isFinite(n) && normalized !== "" ? String(n) : clean;
    }
    const body = clean
      .slice(1)
      .trim()
      .toUpperCase()
      .replaceAll(/\bROW\(\s*([A-Z]+\d+)?\s*\)/g, (_, ref) =>
        String(parseCellRefParts(ref || currentCellRef || "A1").row)
      )
      .replaceAll(/\bCOLUMN\(\s*([A-Z]+\d+)?\s*\)/g, (_, ref) =>
        String(parseCellRefParts(ref || currentCellRef || "A1").col + 1)
      );
    const rangeMatch =
      /^(SUM|AVERAGE|AVG|MAX|MIN|COUNT|STDEV|VAR)\((?:(?:'([^']+)'|([A-Z0-9_]+))!)?([A-Z]+\d+):([A-Z]+\d+)\)$/.exec(
        body
      );
    if (rangeMatch) {
      const rawSh = rangeMatch[2] || rangeMatch[3];
      const rangeSheet = rawSh ? rawSh.trim().toUpperCase() : shUpper;
      const vals = collectRangeNumericValues(rangeMatch[4], rangeMatch[5], cellValues, rangeSheet);
      return evaluateRangeFunction(rangeMatch[1], vals);
    }
    const withCrossSheet = body.replaceAll(
      /(?:'([^']+)'|\b([A-Z0-9_]+))!([A-Z]+\d+)\b/g,
      (_, quotedSh, bareSh, ref) => {
        const shKey = (quotedSh || bareSh || "").trim().toUpperCase();
        const raw = String(cellValues[shKey + "!" + ref] ?? "")
          .replaceAll(",", "")
          .trim();
        const v = Number(raw);
        return raw !== "" && Number.isFinite(v) ? String(v) : "0";
      }
    );
    const substituted = withCrossSheet.replaceAll(/\b([A-Z]+\d+)\b/g, (_, ref) => {
      const raw = String(cellValues[shUpper + "!" + ref] ?? "")
        .replaceAll(",", "")
        .trim();
      const v = Number(raw);
      return raw !== "" && Number.isFinite(v) ? String(v) : "0";
    });
    const directNum = Number(substituted);
    if (substituted !== "" && Number.isFinite(directNum)) {
      return String(Math.round(directNum * 100) / 100);
    }
    const arithResult = evaluateBinaryOpString(substituted);
    return arithResult ?? clean;
  }

  function buildLocalWorkbookValuesMap(cellsPart, defaultSheetUpper, seedDefaults) {
    const values = {};
    const formulaEntries = {};
    const assignments = splitEscapedCellAssignmentsJs(cellsPart);
    assignments.forEach(({ key, value }) => {
      const bIdx = key.indexOf("!");
      let shUpper = defaultSheetUpper;
      let refUpper = "";
      if (bIdx >= 0) {
        shUpper = normalizeSheetTokenJs(key.slice(0, bIdx)).toUpperCase();
        refUpper = key.slice(bIdx + 1).trim().toUpperCase();
      } else {
        refUpper = key.toUpperCase();
      }
      if (!shUpper || !refUpper) return;
      const fullKey = shUpper + "!" + refUpper;
      values[fullKey] = value;
      if (String(value).startsWith("=")) {
        formulaEntries[fullKey] = value;
      } else {
        delete formulaEntries[fullKey];
      }
    });
    if (seedDefaults) {
      if (!Object.keys(values).some((k) => k.startsWith("SHEET1!"))) {
        values["SHEET1!A1"] = "1200";
        values["SHEET1!B1"] = "850";
      }
      if (!Object.keys(values).some((k) => k.startsWith("SHEET2!"))) {
        values["SHEET2!A1"] = "100";
      }
    }
    for (let pass = 0; pass < 4; pass++) {
      Object.keys(formulaEntries).forEach((k) => {
        const [kSheet, kRef] = k.split("!");
        values[k] = evaluateFormulaExpression(formulaEntries[k], values, kSheet, kRef);
      });
    }
    return values;
  }

  function evaluateLocalSheetFormula(payload, seedDefaults = false) {
    const raw = String(payload || "");
    const splitRes = splitUnescapedQueryJs(raw);
    const cellsPart = splitRes ? splitRes.cellsPart : seedDefaults ? "" : raw;
    const rawQueryPart = splitRes
      ? splitRes.queryPart.trim()
      : seedDefaults
        ? raw.trim() || "=Sheet2!A1+50"
        : "A1";
    const parsedTarget = extractActiveSheetAndExprJs(rawQueryPart, "Sheet1");
    const evalSheet = parsedTarget.sheet.toUpperCase();
    const q = parsedTarget.expr;
    const values = buildLocalWorkbookValuesMap(cellsPart, evalSheet, seedDefaults);
    if (q.startsWith("=")) {
      return evaluateFormulaExpression(q, values, evalSheet, activeSheetCellRef || "A1");
    }
    const qUpper = q.toUpperCase();
    return values[evalSheet + "!" + qUpper] ?? "";
  }

  function evaluateLocalWorkbookFormulaRpc(payload) {
    const raw = String(payload || "");
    const splitRes = splitUnescapedQueryJs(raw);
    const exprCandidate = splitRes ? splitRes.queryPart.trim() : raw.trim();
    const rawExpr = exprCandidate || "=Sheet2!A1+50";
    const parsed = extractActiveSheetAndExprJs(rawExpr, "Sheet1");
    const val = String(evaluateLocalSheetFormula(raw, true));
    return { sheet: parsed.sheet, formula: parsed.expr, value: val };
  }

  function evaluateLocalSheetRpc(payload) {
    const raw = String(payload || "");
    const splitRes = splitUnescapedQueryJs(raw);
    const cellsPart = splitRes ? splitRes.cellsPart : raw;
    const trimmedQuery = (splitRes ? splitRes.queryPart : "A1").trim() || "A1";
    const isBatch =
      trimmedQuery.includes(",") && !trimmedQuery.includes("=") && !trimmedQuery.includes("(");
    if (isBatch) {
      const targets = trimmedQuery
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      const firstParsed = extractActiveSheetAndExprJs(targets[0] || "A1", "Sheet1");
      const values = buildLocalWorkbookValuesMap(
        cellsPart,
        firstParsed.sheet.toUpperCase(),
        false
      );
      const outMap = {};
      let firstExpr = "A1";
      let firstVal = "";
      targets.forEach((t, idx) => {
        const parsed = extractActiveSheetAndExprJs(t, "Sheet1");
        const shUpper = parsed.sheet.toUpperCase();
        const refUpper = parsed.expr.toUpperCase();
        const v = String(values[shUpper + "!" + refUpper] ?? "");
        outMap[parsed.sheet + "!" + parsed.expr] = v;
        if (idx === 0) {
          firstExpr = parsed.expr;
          firstVal = v;
        }
      });
      return { query: firstExpr, value: firstVal, values: outMap };
    }
    const singleParsed = extractActiveSheetAndExprJs(trimmedQuery, "Sheet1");
    const val = String(evaluateLocalSheetFormula(raw, false));
    return {
      query: singleParsed.expr,
      value: val,
      values: { [singleParsed.sheet + "!" + singleParsed.expr]: val },
    };
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
    if (formulaInput && activeTd && document.activeElement !== formulaInput) {
      formulaInput.value = activeTd.dataset.formula || (activeTd.textContent || "").trim();
    }

    const aggEl = document.getElementById("status-sheet-agg");
    if (aggEl) {
      if (numericVals.length > 0) {
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
      } else {
        aggEl.textContent = "선택 집계 · 숫자 데이터 없음";
      }
    }
  }

  function saveActiveDocTabState() {
    if (activeModule !== "DOC" && activeModule !== "HWP") return;
    const editor = document.getElementById("doc-editor");
    if (!editor) return;
    docTabStore[activeModule] = {
      editorHtml: editor.innerHTML,
    };
  }

  function ensureDefaultDocTabStore() {
    const editor = document.getElementById("doc-editor");
    if (!docTabStore.DOC && editor) {
      docTabStore.DOC = {
        editorHtml: editor.innerHTML,
      };
    }
    if (!docTabStore.HWP) {
      const hwpTableRows =
        '<tr><td>표준화 총괄과</td><td>김민준 서기관</td><td data-doc-num="520">520</td></tr>' +
        '<tr><td>정보보호 감사팀</td><td>박서연 사무관</td><td data-doc-num="340">340</td></tr>' +
        '<tr id="doc-table-sum-row"><td>합계 (=SUM(ABOVE))</td><td>자동 집계</td><td id="doc-table-sum-cell" data-doc-num="860">860</td></tr>';
      docTabStore.HWP = {
        editorHtml:
          "<h1>행정안전부 전자결재 표준 기안문 (KS X 6101 HWPX)</h1>" +
          "<p>문서번호: 기획재정담당관-2026-0418 · 시행일자: 2026. 10. 25. · 공개구분: 대국민 공개</p>" +
          "<h2>1. 추진 배경 및 목적</h2>" +
          "<p>공공기관 행정 문서의 HWP 5.0(OLE2 CFBF) 및 HWPX(OWPML) 상호운용성 표준을 준수하고 다중 표·누름틀·원고지 조판 정합성을 확보합니다.</p>" +
          '<div class="doc-table-toolbar" contenteditable="false">' +
          '<button id="btn-doc-add-row" class="secondary-btn">+행 추가</button>' +
          '<button id="btn-doc-merge-toggle" class="secondary-btn">셀 병합 토글</button>' +
          '<button id="btn-doc-eval-table" class="secondary-btn">=SUM(ABOVE)</button>' +
          "</div>" +
          '<table id="doc-table" class="sheet-table doc-inline-table"><thead><tr><th>소관 부서</th><th>담당자</th><th>집행액(백만원)</th></tr></thead><tbody id="doc-table-body">' +
          hwpTableRows +
          "</tbody></table>" +
          '<p id="hwp-manuscript-preview" class="hwp-manuscript-notice">한글(HWP/HWPX) 전용 조판 영역: 결재란 · 누름틀 · 200자 원고지 격자 활성</p>',
      };
    }
  }

  function restoreDocTabState(mod) {
    if (mod !== "DOC" && mod !== "HWP") return;
    ensureDefaultDocTabStore();
    const saved = docTabStore[mod];
    if (!saved) return;
    const editor = document.getElementById("doc-editor");
    if (editor) editor.innerHTML = saved.editorHtml;
    activeDocBlockEl = null;
    computeDocTableSummary();
    computeSmartStats();
  }

  function captureWorkspaceSnapshot(isRibbon = false) {
    saveActiveDocTabState();
    sheetDataStore[activeSheetName] = snapshotDomSheetCells();
    return {
      isRibbon: Boolean(isRibbon),
      activeModule,
      activeSheetName,
      activeSheetCellRef,
      activeSlideIndex,
      pdfRotationDeg,
      theme: document.documentElement.dataset.theme || "light",
      docTabs: JSON.parse(JSON.stringify(docTabStore)),
      sheets: JSON.parse(JSON.stringify(sheetDataStore)),
      slides: JSON.parse(JSON.stringify(SLIDE_DECK_STATE)),
      ribbonState: JSON.parse(JSON.stringify(localRibbonState)),
    };
  }

  function restoreWorkspaceSnapshot(snap) {
    if (!snap) return;
    sheetSnapshotEpoch += 1;
    if (snap.docTabs) {
      docTabStore.DOC = snap.docTabs.DOC;
      docTabStore.HWP = snap.docTabs.HWP;
    }
    if (snap.sheets) {
      Object.keys(sheetDataStore).forEach((k) => delete sheetDataStore[k]);
      Object.assign(sheetDataStore, JSON.parse(JSON.stringify(snap.sheets)));
    }
    if (Array.isArray(snap.slides)) {
      SLIDE_DECK_STATE.splice(0, SLIDE_DECK_STATE.length, ...JSON.parse(JSON.stringify(snap.slides)));
    }
    if (snap.ribbonState) {
      Object.assign(localRibbonState, snap.ribbonState);
    }
    pdfRotationDeg = typeof snap.pdfRotationDeg === "number" ? snap.pdfRotationDeg : 0;
    const badge = document.getElementById("pdf-rotation-badge");
    const card = document.getElementById("pdf-page-card");
    if (badge) badge.textContent = "회전: " + pdfRotationDeg + "°";
    if (card) card.style.transform = "rotate(" + pdfRotationDeg + "deg)";

    if (snap.theme) {
      document.documentElement.dataset.theme = snap.theme;
      const themeBtn = document.getElementById("btn-theme-toggle");
      if (themeBtn) themeBtn.textContent = snap.theme === "dark" ? "라이트 모드" : "다크 모드";
    }

    const targetMod = snap.activeModule || activeModule;
    if (targetMod === "DOC" || targetMod === "HWP") {
      restoreDocTabState(targetMod);
    }
    if (snap.activeSheetName) {
      activeSheetName = snap.activeSheetName;
      restoreDomSheetCells(activeSheetName);
      updateSheetSelectionAndAggregates(snap.activeSheetCellRef || "F2");
    }
    if (typeof snap.activeSlideIndex === "number") {
      renderSlideDeck(Math.min(snap.activeSlideIndex, SLIDE_DECK_STATE.length - 1));
    }
    activatePane(targetMod, true);
    refreshFocusedSheetCellPendingSnapshot();
  }

  function pushUndoSnapshot(isRibbon = false) {
    workspaceUndoStack.push(captureWorkspaceSnapshot(isRibbon));
    if (workspaceUndoStack.length > MAX_UNDO_SNAPSHOTS) {
      workspaceUndoStack.shift();
    }
    workspaceRedoStack.length = 0;
  }

  async function performWorkspaceUndo() {
    const fb = document.getElementById("status-command-feedback");
    if (workspaceUndoStack.length === 0) {
      if (fb) fb.textContent = "실행 취소할 작업이 없습니다";
      return;
    }
    const prev = workspaceUndoStack.pop();
    workspaceRedoStack.push(captureWorkspaceSnapshot(Boolean(prev.isRibbon)));
    if (prev.isRibbon === true) {
      await invokeIpc("undo_ribbon_command", "");
    }
    restoreWorkspaceSnapshot(prev);
    if (fb) fb.textContent = "실행 취소 완료 (" + activeModule + ")";
  }

  async function performWorkspaceRedo() {
    const fb = document.getElementById("status-command-feedback");
    if (workspaceRedoStack.length === 0) {
      if (fb) fb.textContent = "다시 실행할 작업이 없습니다";
      return;
    }
    const next = workspaceRedoStack.pop();
    workspaceUndoStack.push(captureWorkspaceSnapshot(Boolean(next.isRibbon)));
    if (next.isRibbon === true) {
      await invokeIpc("redo_ribbon_command", "");
    }
    restoreWorkspaceSnapshot(next);
    if (fb) fb.textContent = "다시 실행 완료 (" + activeModule + ")";
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
      items = Object.keys(sheetDataStore).map((sName, idx) => ({
        label: idx + 1 + ". 시트 워크시트 (" + sName + ")",
        action: () => switchSheetTab(sName),
        active: activeSheetName === sName,
      }));
      items.push({
        label: items.length + 1 + ". 연간 합계 행 (F7)",
        action: () => updateSheetSelectionAndAggregates("F7"),
        active: false,
      });
    } else if (mod === "SLIDE") {
      items = SLIDE_DECK_STATE.map((s, idx) => ({
        label: idx + 1 + ". " + s.title,
        action: () => renderSlideDeck(idx),
        active: idx === activeSlideIndex,
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
        outlineList.querySelectorAll(".outline-item").forEach((el) => el.classList.remove("active"));
        li.classList.add("active");
        if (typeof item.action === "function") item.action();
      });
      outlineList.appendChild(li);
    });
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
    if (hwpNotice) hwpNotice.hidden = mod !== "HWP" && !localRibbonState.manuscriptGridEnabled;
  }

  function activatePane(mod, skipTabBufferSync) {
    if (!skipTabBufferSync && activeModule === "SHEET" && mod !== "SHEET") {
      void commitPendingFocusedSheetCell(false);
    }
    if (!skipTabBufferSync && (activeModule === "DOC" || activeModule === "HWP") && mod !== activeModule) {
      saveActiveDocTabState();
    }
    if (!skipTabBufferSync && (mod === "DOC" || mod === "HWP") && mod !== activeModule) {
      restoreDocTabState(mod);
    }
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
    renderModuleOutline(mod);
    void renderRibbonCategoryTools(activeRibbonCategory);
  }

  function getFocusedDocBlock(docEditor) {
    if (!docEditor) return null;
    const sel = globalThis.getSelection ? globalThis.getSelection() : null;
    if (sel && sel.anchorNode) {
      const node = sel.anchorNode.nodeType === 1 ? sel.anchorNode : sel.anchorNode.parentElement;
      const block = node
        ? node.closest(
            "#doc-editor > h1, #doc-editor > h2, #doc-editor > h3, #doc-editor > p:not(#hwp-manuscript-preview), #doc-editor > div:not(.doc-table-toolbar)"
          )
        : null;
      if (block) {
        activeDocBlockEl = block;
        return block;
      }
    }
    if (activeDocBlockEl && docEditor.contains(activeDocBlockEl)) {
      return activeDocBlockEl;
    }
    return (
      docEditor.querySelector(
        ":scope > p:not(#hwp-manuscript-preview), :scope > div:not(.doc-table-toolbar), :scope > h2, :scope > h1, :scope > h3"
      ) || docEditor
    );
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
    computeSmartStats();
  }

  function appendNewSlideDeckItem() {
    const nextNum = SLIDE_DECK_STATE.length + 1;
    SLIDE_DECK_STATE.push({
      title: "새 슬라이드 #" + nextNum + " (16:9 와이드)",
      lower: "핵심 요약 및 다이어그램 영역",
      notes: "발표자 노트를 입력하세요.",
      transition: "fade",
    });
    renderSlideDeck(SLIDE_DECK_STATE.length - 1);
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
        pdfRotationDeg +
        "°";
    }
  }

  function applyDocEditorRibbonEffect(docEditor, cmdId, execRes) {
    if (!docEditor) return false;
    const targetEl = getFocusedDocBlock(docEditor);
    if (cmdId.endsWith("-home-bold")) {
      const boldOn = execRes?.isBold ?? targetEl.style.fontWeight !== "600";
      targetEl.style.fontWeight = boldOn ? "600" : "";
      return true;
    }
    if (cmdId.endsWith("-home-italic")) {
      const italicOn = execRes?.isItalic ?? targetEl.style.fontStyle !== "italic";
      targetEl.style.fontStyle = italicOn ? "italic" : "";
      return true;
    }
    if (cmdId.endsWith("-home-underline")) {
      const underOn = execRes?.isUnderline ?? targetEl.style.textDecoration !== "underline";
      targetEl.style.textDecoration = underOn ? "underline" : "";
      return true;
    }
    if (cmdId.endsWith("-home-style")) {
      const presetIdx = Number(execRes?.stylePresetIndex ?? 0) % 4;
      const sizes = ["15px", "18px", "22px", "13.5px"];
      targetEl.style.fontSize = sizes[presetIdx];
      return true;
    }
    if (cmdId.endsWith("-review-track")) {
      const firstP = docEditor.querySelector(":scope > p:not(#hwp-manuscript-preview)");
      if (firstP) {
        firstP.dataset.highlighted = firstP.dataset.highlighted === "true" ? "false" : "true";
        firstP.style.backgroundColor =
          firstP.dataset.highlighted === "true" ? "rgba(92, 107, 192, 0.14)" : "";
      }
      return true;
    }
    if (cmdId.endsWith("-layout-columns")) {
      const cols = execRes?.columnCount || 1;
      docEditor.style.columnCount = cols > 1 ? String(cols) : "";
      return true;
    }
    if (cmdId.endsWith("-layout-orientation")) {
      const landscape = Boolean(execRes?.isLandscape);
      docEditor.style.maxWidth = landscape ? "980px" : "";
      return true;
    }
    if (cmdId.endsWith("-layout-margins")) {
      const idx = Number(execRes?.marginPresetIndex ?? 0) % 3;
      const pads = ["28px 36px", "16px 20px", "36px 52px"];
      docEditor.style.padding = pads[idx];
      return true;
    }
    if (cmdId.endsWith("-layout-paper")) {
      const paper = execRes?.paperSize || "A4";
      docEditor.dataset.paperSize = paper;
      return true;
    }
    if (cmdId === "hwp-mod-border") {
      const tableEl = document.getElementById("doc-table");
      if (tableEl) {
        const borderOn =
          execRes?.doubleDiagonalBorderEnabled ?? tableEl.style.borderStyle !== "double";
        tableEl.style.borderStyle = borderOn ? "double" : "";
      }
      return true;
    }
    if (
      cmdId.endsWith("-insert-chart") ||
      cmdId.endsWith("-insert-shape") ||
      cmdId.endsWith("-insert-link") ||
      cmdId.endsWith("-data-fn-browser") ||
      cmdId.endsWith("-review-comment") ||
      cmdId === "doc-mod-toc" ||
      cmdId === "doc-mod-footnote" ||
      cmdId === "doc-mod-mailmerge" ||
      cmdId === "hwp-mod-header" ||
      cmdId === "hwp-mod-field"
    ) {
      const p = document.createElement("p");
      p.textContent = "[" + (execRes?.summary || cmdId) + "]";
      docEditor.appendChild(p);
      computeSmartStats();
      renderModuleOutline(activeModule);
      return true;
    }
    if (cmdId.endsWith("-insert-table")) {
      appendDocTableRow();
      return true;
    }
    if (cmdId.endsWith("-data-autosum") || cmdId === "doc-mod-tableformula") {
      computeDocTableSummary();
      return true;
    }
    if (cmdId.endsWith("-data-sort")) {
      const tbody = document.getElementById("doc-table-body");
      if (tbody) {
        const rows = Array.from(tbody.querySelectorAll("tr:not(#doc-table-sum-row)"));
        const asc = Boolean(execRes?.sortAscending);
        rows.sort((a, b) => {
          const aText = (a.cells[0]?.textContent || "").trim();
          const bText = (b.cells[0]?.textContent || "").trim();
          return asc ? aText.localeCompare(bText, "ko") : bText.localeCompare(aText, "ko");
        });
        const sumRow = document.getElementById("doc-table-sum-row");
        rows.forEach((r) => (sumRow ? sumRow.before(r) : tbody.appendChild(r)));
      }
      return true;
    }
    return false;
  }

  function applyRibbonDomSideEffects(cmdId, execRes) {
    if (cmdId.endsWith("-view-theme")) {
      const html = document.documentElement;
      const next = html.dataset.theme === "dark" ? "light" : "dark";
      html.dataset.theme = next;
      const themeBtn = document.getElementById("btn-theme-toggle");
      if (themeBtn) themeBtn.textContent = next === "dark" ? "라이트 모드" : "다크 모드";
      return;
    }
    if (cmdId.endsWith("-view-zoom")) {
      const zoom = Number(execRes?.zoomPercent || 100);
      const stage = document.querySelector(".workspace-center");
      if (stage) stage.style.zoom = zoom + "%";
      return;
    }
    if (cmdId.endsWith("-view-ribbon")) {
      const strip = document.getElementById("ribbon-tool-strip");
      if (strip) strip.hidden = !strip.hidden;
      return;
    }
    if (cmdId.endsWith("-view-split")) {
      const stage = document.querySelector(".workspace-center");
      if (stage) {
        stage.dataset.splitView = stage.dataset.splitView === "true" ? "false" : "true";
      }
      return;
    }
    if (cmdId.endsWith("-review-verify") || cmdId.endsWith("-data-validation")) {
      document.getElementById("btn-run-security-audit")?.click();
      return;
    }
    if (cmdId.endsWith("-review-stats")) {
      document.getElementById("btn-smart-diagnose")?.click();
      return;
    }
    if (cmdId === "hwp-mod-manuscript") {
      const hwpNotice = document.getElementById("hwp-manuscript-preview");
      if (hwpNotice) {
        hwpNotice.hidden =
          execRes?.manuscriptGridEnabled === undefined
            ? !hwpNotice.hidden
            : !execRes.manuscriptGridEnabled;
      }
      return;
    }
    const docEditor = document.getElementById("doc-editor");
    if (
      activeModule === "CONV" &&
      (cmdId.startsWith("document-") || cmdId.startsWith("doc-mod-"))
    ) {
      restoreDocTabState("DOC");
      if (applyDocEditorRibbonEffect(docEditor, cmdId, execRes)) {
        if (docEditor) {
          docTabStore.DOC = {
            editorHtml: docEditor.innerHTML,
          };
        }
        return;
      }
    }
    if (
      (activeModule === "DOC" || activeModule === "HWP") &&
      applyDocEditorRibbonEffect(docEditor, cmdId, execRes)
    ) {
      saveActiveDocTabState();
      return;
    }
    if (cmdId.startsWith("sheet-")) {
      const activeTd = document.querySelector(
        '#pane-sheet td[data-cell="' + activeSheetCellRef + '"]'
      );
      if (cmdId === "sheet-home-bold" && activeTd) {
        activeTd.style.fontWeight = execRes?.isBold ? "600" : "";
      } else if (cmdId === "sheet-home-italic" && activeTd) {
        activeTd.style.fontStyle = execRes?.isItalic ? "italic" : "";
      } else if (cmdId === "sheet-home-underline" && activeTd) {
        activeTd.style.textDecoration = execRes?.isUnderline ? "underline" : "";
      } else if (cmdId === "sheet-home-style" && activeTd) {
        const presetIdx = Number(execRes?.stylePresetIndex ?? 0) % 4;
        const sizes = ["13px", "15px", "17px", "12px"];
        activeTd.style.fontSize = sizes[presetIdx];
      } else if (cmdId === "sheet-data-autosum" || cmdId === "sheet-data-fn-browser") {
        void evaluateActiveSheetFormulaBar(false);
      } else if (cmdId === "sheet-insert-table") {
        const tbody = document.querySelector("#pane-sheet tbody");
        if (tbody) {
          const rowIdx = tbody.querySelectorAll("tr").length + 1;
          const tr = document.createElement("tr");
          tr.innerHTML =
            '<th data-row="' +
            rowIdx +
            '">' +
            rowIdx +
            "</th>" +
            ["A", "B", "C", "D", "E", "F", "G", "H"]
              .map((c) => '<td contenteditable="true" data-cell="' + c + rowIdx + '">0</td>')
              .join("");
          tbody.appendChild(tr);
          bindSheetCellEvents(tr);
        }
      } else {
        const out = document.getElementById("sheet-eval-result");
        if (out && execRes?.summary) out.textContent = execRes.summary;
      }
      return;
    }
    if (cmdId.startsWith("slide-")) {
      const titleZone = document.getElementById("slide-title-zone");
      const lowerZone = document.getElementById("slide-lower-zone");
      const stageBox = document.getElementById("slide-stage-box");
      const curSlide = SLIDE_DECK_STATE[activeSlideIndex];
      if (cmdId === "slide-home-bold" && titleZone) {
        titleZone.style.fontWeight = execRes?.isBold ? "700" : "";
      } else if (cmdId === "slide-home-italic" && titleZone) {
        titleZone.style.fontStyle = execRes?.isItalic ? "italic" : "";
      } else if (cmdId === "slide-home-underline" && titleZone) {
        titleZone.style.textDecoration = execRes?.isUnderline ? "underline" : "";
      } else if (cmdId === "slide-home-style" && titleZone) {
        const presetIdx = Number(execRes?.stylePresetIndex ?? 0) % 4;
        const sizes = ["20px", "22px", "24px", "18px"];
        titleZone.style.fontSize = sizes[presetIdx];
      } else if (
        cmdId === "slide-insert-table" ||
        cmdId === "slide-insert-chart" ||
        cmdId === "slide-insert-shape"
      ) {
        appendNewSlideDeckItem();
      } else if (
        cmdId === "slide-insert-link" ||
        cmdId === "slide-data-autosum" ||
        cmdId === "slide-data-fn-browser" ||
        cmdId === "slide-review-comment"
      ) {
        const tag = "[" + (execRes?.summary || cmdId) + "]";
        if (curSlide) {
          curSlide.lower = (curSlide.lower ? curSlide.lower + " · " : "") + tag;
        }
        if (lowerZone && curSlide) {
          const p = document.createElement("p");
          p.textContent = curSlide.lower;
          lowerZone.replaceChildren(p);
        }
      } else if (cmdId === "slide-data-sort" && curSlide && curSlide.lower) {
        const asc = Boolean(execRes?.sortAscending);
        const parts = curSlide.lower
          .split("·")
          .map((s) => s.trim())
          .filter(Boolean);
        parts.sort((a, b) => (asc ? a.localeCompare(b, "ko") : b.localeCompare(a, "ko")));
        curSlide.lower = parts.join(" · ");
        renderSlideDeck(activeSlideIndex);
      } else if (cmdId === "slide-mod-presenter") {
        document.getElementById("btn-slide-laser")?.click();
      } else if (cmdId === "slide-mod-transition") {
        const sel = document.getElementById("slide-transition-select");
        if (sel) {
          const opts = ["fade", "push", "zoom", "none"];
          const nextIdx = (opts.indexOf(sel.value) + 1) % opts.length;
          sel.value = opts[nextIdx];
          SLIDE_DECK_STATE[activeSlideIndex].transition = sel.value;
          renderSlideDeck(activeSlideIndex);
        }
      } else if (stageBox) {
        if (cmdId === "slide-layout-columns" && lowerZone) {
          const cols = execRes?.columnCount || 1;
          lowerZone.style.columnCount = cols > 1 ? String(cols) : "";
        } else if (cmdId === "slide-layout-margins") {
          const idx = Number(execRes?.marginPresetIndex ?? 0) % 3;
          const pads = ["20px", "12px", "28px"];
          stageBox.style.padding = pads[idx];
        } else if (cmdId === "slide-mod-snap" || cmdId === "slide-review-track") {
          stageBox.style.outline = stageBox.style.outline ? "" : "2px dashed var(--eo-primary)";
        } else {
          stageBox.dataset.layoutState = execRes?.summary || cmdId;
        }
      }
      const metaStatus = document.getElementById("slide-meta-status");
      if (metaStatus && execRes?.summary && cmdId !== "slide-mod-transition") {
        metaStatus.textContent =
          "슬라이드 " +
          (activeSlideIndex + 1) +
          " / " +
          SLIDE_DECK_STATE.length +
          " · " +
          execRes.summary;
      }
      return;
    }
    if (cmdId === "pdf-mod-rotate") {
      pdfRotationDeg =
        typeof execRes?.pdfRotationDegrees === "number"
          ? execRes.pdfRotationDegrees
          : (pdfRotationDeg + 90) % 360;
      const badge = document.getElementById("pdf-rotation-badge");
      const card = document.getElementById("pdf-page-card");
      if (badge) badge.textContent = "회전: " + pdfRotationDeg + "°";
      if (card) card.style.transform = "rotate(" + pdfRotationDeg + "deg)";
      return;
    }
    if (cmdId === "pdf-mod-tooffice") {
      activatePane("CONV");
      return;
    }
    if (cmdId.startsWith("pdf-")) {
      applyPdfStudioFormState();
    }
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
        pushUndoSnapshot(true);
        const execRes = await invokeIpc("execute_ribbon_command", cmd.id);
        applyRibbonDomSideEffects(cmd.id, execRes);
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
    if (activeModule === "SLIDE") {
      renderModuleOutline("SLIDE");
    }
  }

  function isExcludedDocChromeElement(el) {
    if (!el || el.nodeType !== 1) return false;
    const tag = (el.tagName || "").toUpperCase();
    if (tag === "TABLE" || tag === "FOOTER") return true;
    if (el.id === "hwp-manuscript-preview") return true;
    if (el.classList && el.classList.contains("doc-table-toolbar")) return true;
    return false;
  }

  function extractEditableDocumentText(rootEl = document.getElementById("doc-editor")) {
    if (!rootEl) return "";
    const parts = [];
    Array.from(rootEl.childNodes || []).forEach((node) => {
      if (node.nodeType === 3) {
        const t = (node.textContent || "").trim();
        if (t) parts.push(t);
        return;
      }
      if (node.nodeType === 1) {
        if (isExcludedDocChromeElement(node)) return;
        const clone = node.cloneNode(true);
        clone
          .querySelectorAll(".doc-table-toolbar, table, #hwp-manuscript-preview, footer")
          .forEach((child) => child.remove());
        const t = (clone.textContent || "").trim();
        if (t) parts.push(t);
      }
    });
    return parts.join("\n");
  }

  function resolveSourceScopeKind(sourceFormatOrModule) {
    const token = String(sourceFormatOrModule || activeModule || "DOC")
      .trim()
      .toLowerCase();
    if (["xlsx", "xls", "ods", "csv", "tsv", "sheet"].includes(token)) return "SHEET";
    if (["pptx", "ppsx", "ppt", "odp", "slide"].includes(token)) return "SLIDE";
    if (["pdf"].includes(token)) return "PDF";
    if (["hwp", "hwt", "hwpx", "hwtx"].includes(token)) return "HWP";
    return "DOC";
  }

  function appendTableRowsFromTrList(rows, lines) {
    if (rows.length === 0) return;
    lines.push("[[TABLE_NEW]]");
    rows.forEach((tr) => {
      const cells = Array.from(tr.querySelectorAll("th, td")).map((cell) =>
        (cell.textContent || "").trim().replaceAll("\t", " ").replaceAll("\n", " ")
      );
      if (cells.length > 0) {
        lines.push("[[TABLE_ROW]]\t" + cells.join("\t"));
      }
    });
  }

  function buildStructuredConversionBody(sourceFormatOrModule) {
    saveActiveDocTabState();
    ensureDefaultDocTabStore();
    const scope = resolveSourceScopeKind(sourceFormatOrModule);
    const lines = [];

    if (scope === "HWP") {
      lines.push("[[AGENCY]]\t행정안전부");
    }

    if (scope === "DOC" || scope === "HWP") {
      const saved = docTabStore[scope];
      if (saved) {
        const tempDiv = document.createElement("div");
        tempDiv.innerHTML = saved.editorHtml || "";
        const docText = extractEditableDocumentText(tempDiv);
        if (docText) {
          lines.push(docText);
        }
        const inlineTable = tempDiv.querySelector("table");
        if (inlineTable) {
          appendTableRowsFromTrList(Array.from(inlineTable.querySelectorAll("tr")), lines);
        }
      }
    } else if (scope === "SHEET") {
      sheetDataStore[activeSheetName] = snapshotDomSheetCells();
      const sheetObj = sheetDataStore[activeSheetName] || {};
      const rowMap = new Map();
      Object.keys(sheetObj).forEach((ref) => {
        const entry = sheetObj[ref];
        const rawVal = (entry?.formula || entry?.value || "")
          .trim()
          .replaceAll("\t", " ")
          .replaceAll("\n", " ");
        const dispVal = (entry?.value || entry?.formula || "")
          .trim()
          .replaceAll("\t", " ")
          .replaceAll("\n", " ");
        if (!rawVal && !dispVal) return;
        lines.push("[[SHEET_A1]]\t" + ref + "\t" + (rawVal || dispVal));
        const p = parseCellRefParts(ref);
        const rIdx = Math.max(0, p.row - 1);
        const cIdx = Math.max(0, p.col);
        if (!rowMap.has(rIdx)) rowMap.set(rIdx, []);
        rowMap.get(rIdx)[cIdx] = dispVal || rawVal;
      });
      if (rowMap.size > 0) {
        lines.push("[[TABLE_NEW]]");
        Array.from(rowMap.keys())
          .sort((a, b) => a - b)
          .forEach((rIdx) => {
            const rowArr = rowMap.get(rIdx) || [];
            const dense = [];
            for (let i = 0; i < rowArr.length; i += 1) {
              dense.push(rowArr[i] || "");
            }
            if (dense.length > 0) {
              lines.push("[[TABLE_ROW]]\t" + dense.join("\t"));
            }
          });
      }
    } else if (scope === "SLIDE") {
      SLIDE_DECK_STATE.forEach((s) => {
        const title = (s.title || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
        const bullets = (s.lower || "")
          .split(/\n+|·/)
          .map((item) => item.trim().replaceAll("\t", " "))
          .filter(Boolean)
          .join(";;");
        const notes = (s.notes || "").trim().replaceAll("\t", " ").replaceAll("\n", " ");
        lines.push("[[SLIDE]]\t" + title + "\t" + bullets + "\t" + notes);
      });
    } else if (scope === "PDF") {
      document.querySelectorAll("#pdf-page-card h3, #pdf-page-card p").forEach((el) => {
        const t = (el.textContent || "").trim();
        if (t) lines.push(t);
      });
    }

    return lines.join("\n") || "본문 텍스트";
  }

  function triggerBase64Download(base64Data, baseFilename, ext) {
    if (!base64Data) return false;
    try {
      const binStr = atob(base64Data);
      const bytes = new Uint8Array(binStr.length);
      for (let i = 0; i < binStr.length; i += 1) {
        bytes[i] = binStr.charCodeAt(i);
      }
      const blob = new Blob([bytes], { type: "application/octet-stream" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = baseFilename + "." + ext.toLowerCase();
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1500);
      return true;
    } catch {
      return false;
    }
  }

  async function saveActiveModuleDocument() {
    await commitPendingFocusedSheetCell(true);
    const extMap = {
      DOC: "docx",
      HWP: "hwpx",
      SHEET: "xlsx",
      SLIDE: "pptx",
      PDF: "pdf",
      CONV: document.getElementById("conv-dst-fmt")?.value || "hwpx",
    };
    const dstExt = extMap[activeModule] || "docx";
    const srcExt =
      activeModule === "CONV"
        ? document.getElementById("conv-src-fmt")?.value || "docx"
        : dstExt;
    const body = buildStructuredConversionBody(activeModule === "CONV" ? srcExt : activeModule);
    const title = "EasyOffice_" + activeModule + "_문서";
    const res = await invokeIpc(
      "convert_document_format",
      srcExt + "->" + dstExt + "|" + title + "|" + body
    );
    const fb = document.getElementById("status-command-feedback");
    if (res && res.convertedBase64) {
      triggerBase64Download(res.convertedBase64, title, dstExt);
    }
    if (fb && res) {
      fb.textContent =
        "문서 저장 완료 (" +
        dstExt.toUpperCase() +
        " · " +
        (res.outputBytes || 0) +
        "B · " +
        (res.specification || "") +
        ")";
    }
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
      const koBadge = MODULE_SHORT_BADGES[mod] || mod;
      Object.keys(cats).forEach((cat) => {
        cats[cat].forEach((c) => {
          allItems.push({
            id: c.id,
            module: mod,
            label: "[" + koBadge + "] " + c.title + " (" + c.shortcut + ")",
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
      .filter(
        (item) =>
          !q ||
          item.label.toLowerCase().includes(q) ||
          item.id.toLowerCase().includes(q) ||
          item.module.toLowerCase().includes(q)
      )
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
            pushUndoSnapshot(true);
            activatePane(item.module);
            const res = await invokeIpc("execute_ribbon_command", item.id);
            applyRibbonDomSideEffects(item.id, res);
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
      document.getElementById("bench-result-box") || document.getElementById("bench-Summary");
    const fb = document.getElementById("status-command-feedback");
    if (res && Array.isArray(res.suites)) {
      const summary = res.suites
        .map(
          (s) =>
            s.suiteId +
            " " +
            s.baselineP50Us +
            "µs→" +
            s.optimizedP50Us +
            "µs (" +
            s.speedupFactor +
            "x)"
        )
        .join(" · ");
      if (box) {
        box.textContent =
          "성능 진단 완료 (" + (res.allEquivalent ? "결과 일치" : "불일치") + ") · " + summary;
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
        void saveActiveModuleDocument();
      });
    }
    const qatUndo = document.getElementById("qat-undo");
    if (qatUndo) {
      qatUndo.addEventListener("click", () => {
        void performWorkspaceUndo();
      });
    }
    const qatRedo = document.getElementById("qat-redo");
    if (qatRedo) {
      qatRedo.addEventListener("click", () => {
        void performWorkspaceRedo();
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
        const keys = RIBBON_CATEGORY_ORDER;
        let nextActiveCat = activeRibbonCategory;
        document.querySelectorAll("#ribbon-tabs .ribbon-btn").forEach((b, idx) => {
          if (labels[idx]) b.textContent = labels[idx];
          if (keys[idx]) {
            b.dataset.category = keys[idx];
            if (b.classList.contains("active")) {
              nextActiveCat = keys[idx];
            }
          }
        });
        await renderRibbonCategoryTools(nextActiveCat);
      });
    }

    const themeBtn = document.getElementById("btn-theme-toggle");
    if (themeBtn) {
      themeBtn.addEventListener("click", () => {
        pushUndoSnapshot(false);
        const html = document.documentElement;
        const next = html.dataset.theme === "dark" ? "light" : "dark";
        html.dataset.theme = next;
        themeBtn.textContent = next === "dark" ? "라이트 모드" : "다크 모드";
      });
    }
  }

  async function recalculateActiveSheetFormulas() {
    const epoch = sheetSnapshotEpoch;
    const seq = latestSheetRecalcSeq + 1;
    latestSheetRecalcSeq = seq;
    const cellMap = collectSheetCellMap();
    const pairs = Object.keys(cellMap)
      .map((k) => k + "=" + cellMap[k])
      .join(";");
    const formulaTargets = [];
    Object.keys(sheetDataStore).forEach((shName) => {
      const snap = sheetDataStore[shName] || {};
      Object.keys(snap).forEach((ref) => {
        if ((snap[ref]?.formula || "").startsWith("=")) {
          formulaTargets.push({ sheet: shName, ref, key: shName + "!" + ref });
        }
      });
    });
    if (formulaTargets.length === 0) {
      return;
    }
    const queryList = formulaTargets.map((t) => t.sheet + "|" + t.ref).join(",");
    const rpcRes = await invokeIpc("evaluate_sheet", pairs + "?" + queryList);
    if (epoch !== sheetSnapshotEpoch || seq !== latestSheetRecalcSeq) {
      return;
    }
    const batchValues =
      rpcRes && typeof rpcRes.values === "object" && rpcRes.values ? rpcRes.values : null;
    formulaTargets.forEach(({ sheet, ref, key }) => {
      let rawVal;
      if (batchValues) {
        rawVal = batchValues[key] ?? batchValues[sheet.toUpperCase() + "!" + ref.toUpperCase()];
      } else if (formulaTargets.length === 1 && rpcRes && rpcRes.value !== undefined) {
        rawVal = rpcRes.value;
      }
      if (rawVal === undefined || rawVal === null) return;
      const value = String(rawVal);
      if (activeSheetName === sheet) {
        const td = document.querySelector('#pane-sheet td[data-cell="' + ref + '"]');
        const focusSt = td ? sheetCellFocusState.get(td) : null;
        if (!td || !(td.dataset.formula || "").startsWith("=")) return;
        if (focusSt?.userEdited || focusSt?.snapshotPushed) return;
        td.textContent = value;
        syncSheetCellFocusBaseline(td, false);
      }
      const storedEntry = sheetDataStore[sheet]?.[ref];
      if (storedEntry && (storedEntry.formula || "").startsWith("=")) {
        storedEntry.value = value;
      }
    });
    refreshFocusedSheetCellPendingSnapshot();
  }

  async function evaluateActiveSheetFormulaBar(recordUndo) {
    const input = document.getElementById("formula-input");
    const out = document.getElementById("sheet-eval-result");
    if (!input || !out) return;
    const epoch = sheetSnapshotEpoch;
    const targetSheet = activeSheetName;
    const targetCellRef = activeSheetCellRef;
    if (recordUndo) pushUndoSnapshot(false);
    const rawFormula = input.value.trim();
    const activeTd = document.querySelector(
      '#pane-sheet td[data-cell="' + targetCellRef + '"]'
    );
    if (activeTd) {
      if (rawFormula.startsWith("=")) {
        activeTd.dataset.formula = rawFormula;
      } else {
        delete activeTd.dataset.formula;
        activeTd.textContent = rawFormula;
      }
      syncSheetCellFocusBaseline(activeTd, true);
      refreshFocusedSheetCellPendingSnapshot();
      await recalculateActiveSheetFormulas();
      if (epoch !== sheetSnapshotEpoch) return;
      const computedValue =
        activeSheetName === targetSheet
          ? (activeTd.textContent || "").trim()
          : sheetDataStore[targetSheet]?.[targetCellRef]?.value || "";
      if (activeSheetName === targetSheet) {
        out.textContent = "결과 (" + targetCellRef + "): " + computedValue;
        updateSheetSelectionAndAggregates(activeSheetCellRef || targetCellRef);
      }
      return;
    }

    const cellMap = collectSheetCellMap();
    cellMap[targetSheet + "!" + targetCellRef] = escapeSemicolonToken(rawFormula);
    const pairs = Object.keys(cellMap)
      .map((k) => k + "=" + cellMap[k])
      .join(";");
    const queryTarget = targetSheet + "|" + targetCellRef;
    const rpcRes = await invokeIpc("evaluate_sheet", pairs + "?" + queryTarget);
    await recalculateActiveSheetFormulas();
    if (epoch !== sheetSnapshotEpoch) return;
    if (activeSheetName === targetSheet) {
      out.textContent = "결과 (" + targetCellRef + "): " + rpcRes.value;
      updateSheetSelectionAndAggregates(activeSheetCellRef || targetCellRef);
    }
  }

  function buildPreEditSnapshotForCell(td, st) {
    if (st.pendingSnapshot) {
      return st.pendingSnapshot;
    }
    const fallbackSnap = captureWorkspaceSnapshot(false);
    const shName = st.sheetName || activeSheetName;
    if (fallbackSnap.sheets && fallbackSnap.sheets[shName] && td.dataset?.cell) {
      fallbackSnap.sheets[shName][td.dataset.cell] = {
        value: st.initialText,
        formula: st.initialRaw.startsWith("=") ? st.initialRaw : "",
      };
    }
    return fallbackSnap;
  }

  function pushCellUndoSnapshotIfNeeded(td, st, effectiveRaw) {
    if (!st || st.snapshotPushed || effectiveRaw === st.initialRaw) return;
    const snap = buildPreEditSnapshotForCell(td, st);
    workspaceUndoStack.push(snap);
    if (workspaceUndoStack.length > MAX_UNDO_SNAPSHOTS) {
      workspaceUndoStack.shift();
    }
    workspaceRedoStack.length = 0;
    st.snapshotPushed = true;
  }

  async function commitSheetCellEdit(td, keepFocus) {
    const st = sheetCellFocusState.get(td);
    if (!st || st.sheetName !== activeSheetName) {
      if (!keepFocus) sheetCellFocusState.delete(td);
      return false;
    }
    const editSheet = st.sheetName;
    const rawText = (td.textContent || "").trim();
    const textChanged = Boolean(st.userEdited) || rawText !== st.initialText;
    if (textChanged) {
      if (rawText.startsWith("=")) {
        td.dataset.formula = rawText;
      } else {
        delete td.dataset.formula;
      }
    }
    const effectiveRaw = td.dataset.formula || rawText;
    pushCellUndoSnapshotIfNeeded(td, st, effectiveRaw);
    const shouldRecalc = st.snapshotPushed || textChanged;
    if (keepFocus) {
      syncSheetCellFocusBaseline(td, true);
      refreshFocusedSheetCellPendingSnapshot();
    } else {
      sheetCellFocusState.delete(td);
    }
    if (shouldRecalc) {
      sheetDataStore[editSheet] = snapshotDomSheetCells();
      await recalculateActiveSheetFormulas();
      if (activeSheetName === editSheet) {
        updateSheetSelectionAndAggregates(activeSheetCellRef || td.dataset.cell);
      }
    }
    return shouldRecalc;
  }

  async function commitPendingFocusedSheetCell(keepFocus) {
    const activeEl = document.activeElement;
    if (!activeEl || !sheetCellFocusState.has(activeEl)) {
      return false;
    }
    return await commitSheetCellEdit(activeEl, keepFocus);
  }

  function bindSheetCellEvents(rootEl) {
    rootEl.querySelectorAll("td[data-cell]").forEach((td) => {
      td.addEventListener("click", () => updateSheetSelectionAndAggregates(td.dataset.cell));
      td.addEventListener("focus", () => {
        updateSheetSelectionAndAggregates(td.dataset.cell);
        const initialText = (td.textContent || "").trim();
        sheetCellFocusState.set(td, {
          initialText,
          initialRaw: td.dataset.formula || initialText,
          pendingSnapshot: captureWorkspaceSnapshot(false),
          snapshotPushed: false,
          userEdited: false,
          sheetName: activeSheetName,
        });
      });
      td.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          td.blur();
        }
      });
      td.addEventListener("blur", async () => {
        await commitSheetCellEdit(td, false);
      });
      td.addEventListener("input", () => {
        const st = sheetCellFocusState.get(td);
        const rawText = (td.textContent || "").trim();
        if (rawText.startsWith("=")) {
          td.dataset.formula = rawText;
        } else {
          delete td.dataset.formula;
        }
        if (st) {
          st.userEdited = true;
          pushCellUndoSnapshotIfNeeded(td, st, td.dataset.formula || rawText);
        }
        const formulaInput = document.getElementById("formula-input");
        if (formulaInput && activeSheetCellRef === td.dataset.cell) {
          formulaInput.value = rawText;
        }
        updateSheetSelectionAndAggregates(td.dataset.cell);
      });
    });
  }

  function setupSheetAndConverterHandlers() {
    const btnConvert = document.getElementById("btn-run-convert");
    if (btnConvert) {
      btnConvert.addEventListener("click", async () => {
        await commitPendingFocusedSheetCell(true);
        const src = document.getElementById("conv-src-fmt").value;
        const dst = document.getElementById("conv-dst-fmt").value;
        const body = buildStructuredConversionBody(src);
        const res = await invokeIpc(
          "convert_document_format",
          src + "->" + dst + "|통합_변환_문서|" + body
        );
        const box = document.getElementById("conv-result-box");
        if (res && res.convertedBase64) {
          triggerBase64Download(res.convertedBase64, "통합_변환_문서", dst);
        }
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

    const sheetPane = document.getElementById("pane-sheet");
    if (sheetPane) {
      bindSheetCellEvents(sheetPane);
    }

    const evalBtn = document.getElementById("btn-eval-formula");
    const formulaInputEl = document.getElementById("formula-input");
    if (formulaInputEl && evalBtn) {
      formulaInputEl.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          evalBtn.click();
        }
      });
    }
    if (evalBtn) {
      evalBtn.addEventListener("click", () => {
        void evaluateActiveSheetFormulaBar(true);
      });
    }
  }

  function setupSlideAndPdfHandlers() {
    const btnAddSlide = document.getElementById("btn-add-slide");
    if (btnAddSlide) {
      btnAddSlide.addEventListener("click", () => {
        pushUndoSnapshot(false);
        appendNewSlideDeckItem();
      });
    }
    const slideTransSelect = document.getElementById("slide-transition-select");
    if (slideTransSelect) {
      slideTransSelect.addEventListener("change", () => {
        pushUndoSnapshot(false);
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
    const slideTitleZone = document.getElementById("slide-title-zone");
    if (slideTitleZone) {
      slideTitleZone.addEventListener("input", () => {
        if (SLIDE_DECK_STATE[activeSlideIndex]) {
          SLIDE_DECK_STATE[activeSlideIndex].title = slideTitleZone.textContent || "";
        }
      });
    }
    const slideLowerZone = document.getElementById("slide-lower-zone");
    if (slideLowerZone) {
      slideLowerZone.addEventListener("input", () => {
        if (SLIDE_DECK_STATE[activeSlideIndex]) {
          SLIDE_DECK_STATE[activeSlideIndex].lower = slideLowerZone.textContent || "";
        }
      });
    }
    const slideStageBox = document.getElementById("slide-stage-box");
    if (slideStageBox) {
      slideStageBox.addEventListener("pointermove", (ev) => {
        if (!slideLaserActive) return;
        const dot = document.getElementById("slide-laser-dot");
        if (!dot) return;
        const rect = slideStageBox.getBoundingClientRect();
        const x = Math.max(8, Math.min(rect.width - 8, ev.clientX - rect.left));
        const y = Math.max(8, Math.min(rect.height - 8, ev.clientY - rect.top));
        dot.style.left = x + "px";
        dot.style.top = y + "px";
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
      btnPdfRotate.addEventListener("click", async () => {
        pushUndoSnapshot(true);
        const execRes = await invokeIpc("execute_ribbon_command", "pdf-mod-rotate");
        applyRibbonDomSideEffects("pdf-mod-rotate", execRes);
      });
    }
    const btnPdfApply = document.getElementById("btn-pdf-apply");
    if (btnPdfApply) {
      btnPdfApply.addEventListener("click", () => {
        pushUndoSnapshot(false);
        applyPdfStudioFormState();
      });
    }
  }

  function collectActiveModuleNodesForScrub() {
    if (activeModule === "SHEET") {
      return Array.from(document.querySelectorAll("#pane-sheet td[data-cell]")).filter(
        (td) => !(td.dataset.formula || "").startsWith("=")
      );
    }
    if (activeModule === "SLIDE") {
      const nodes = [];
      const titleZone = document.getElementById("slide-title-zone");
      const lowerZone = document.getElementById("slide-lower-zone");
      if (titleZone) nodes.push(titleZone);
      if (lowerZone) nodes.push(lowerZone);
      return nodes;
    }
    if (activeModule === "PDF") {
      return Array.from(document.querySelectorAll("#pdf-page-card h3, #pdf-page-card p"));
    }
    const editorEl = document.getElementById("doc-editor");
    if (!editorEl) return [];
    const docNodes = [];
    Array.from(editorEl.childNodes || []).forEach((node) => {
      if (node.nodeType === 3) {
        if ((node.textContent || "").trim().length > 0) {
          docNodes.push(node);
        }
        return;
      }
      if (node.nodeType === 1 && !isExcludedDocChromeElement(node)) {
        if (!node.querySelector(".doc-table-toolbar, table, #hwp-manuscript-preview, footer")) {
          docNodes.push(node);
        }
      }
    });
    const tableCells = Array.from(
      document.querySelectorAll("#doc-table-body td:not([data-doc-num])")
    );
    return docNodes.concat(tableCells);
  }

  async function runPiiScrubAction(recordUndo) {
    const piiOut = document.getElementById("pii-result-text");
    if (!piiOut) return;
    if (recordUndo) pushUndoSnapshot();
    const targets = collectActiveModuleNodesForScrub();
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
    if (activeModule === "SLIDE" && SLIDE_DECK_STATE[activeSlideIndex]) {
      const notesInput = document.getElementById("slide-presenter-notes");
      if (notesInput && notesInput.value) {
        const noteRes = await invokeIpc("scrub_pii", notesInput.value);
        if (noteRes && typeof noteRes.scrubbedText === "string") {
          notesInput.value = noteRes.scrubbedText;
          SLIDE_DECK_STATE[activeSlideIndex].notes = noteRes.scrubbedText;
          totalMasked += Number(noteRes.totalMasked || 0);
        }
      }
    }
    piiOut.textContent =
      totalMasked > 0
        ? "주민번호·전화번호·이메일 " + totalMasked + "건 마스킹 완료 (" + activeModule + ")"
        : "주민번호·전화번호·이메일 스캔 완료 (감지 0건 · " + activeModule + ")";
    computeSmartStats();
  }

  function setupSmartAndSecurityHandlers() {
    const editor = document.getElementById("doc-editor");
    const imeBadge =
      document.getElementById("ime-badge") || document.getElementById("ime-indicator");
    if (editor) {
      editor.addEventListener("click", (ev) => {
        const block = ev.target.closest(
          "#doc-editor > h1, #doc-editor > h2, #doc-editor > h3, #doc-editor > p:not(#hwp-manuscript-preview), #doc-editor > div:not(.doc-table-toolbar)"
        );
        if (block) activeDocBlockEl = block;
      });
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
        const summaryEl = document.getElementById("smart-summary-output");
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
        if (summaryEl && res) {
          const lines = Array.isArray(res.summaryLines) ? res.summaryLines : [];
          const actions = Array.isArray(res.actionItems) ? res.actionItems : [];
          const summaryText = lines.length > 0 ? lines.join(" / ") : "핵심 문장 요약 완료";
          const actionSuffix = actions.length > 0 ? " · 조치: " + actions.join(", ") : "";
          summaryEl.textContent = "요약: " + summaryText + actionSuffix;
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
        const bodyText = buildStructuredConversionBody(activeModule);
        const byteLen = new TextEncoder().encode(bodyText).length;
        const entryNameMap = {
          DOC: "word/document.xml",
          HWP: "Contents/section0.xml",
          SHEET: "xl/worksheets/sheet1.xml",
          SLIDE: "ppt/slides/slide1.xml",
          PDF: "Catalog/Pages.pdf",
          CONV: "package/content.xml",
        };
        const entryName = entryNameMap[activeModule] || "word/document.xml";
        const xmlSnippet =
          '<doc module="' + activeModule + '">' + bodyText.slice(0, 512) + "</doc>";
        const payload =
          entryName +
          "," +
          Math.max(64, Math.floor(byteLen / 2)) +
          "," +
          Math.max(128, byteLen) +
          ",false|" +
          xmlSnippet;
        const res = await invokeIpc("audit_security", payload);
        const txt = document.getElementById("security-status-text");
        if (txt && res) {
          txt.textContent = "문서 보안 검사 완료 (" + activeModule + "): " + res.status;
        }
      });
    }

    const piiBtn = document.getElementById("btn-scrub-pii");
    if (piiBtn) {
      piiBtn.addEventListener("click", () => {
        void runPiiScrubAction(true);
      });
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

  function switchSheetTab(targetSheetName) {
    void commitPendingFocusedSheetCell(false);
    if (!sheetDataStore.Sheet1) {
      sheetDataStore.Sheet1 = snapshotDomSheetCells();
    }
    sheetDataStore[activeSheetName] = snapshotDomSheetCells();
    activeSheetName = targetSheetName;
    if (!sheetDataStore[targetSheetName]) {
      sheetDataStore[targetSheetName] = {
        A1: { value: targetSheetName + " 항목", formula: "" },
        B1: { value: "500", formula: "" },
      };
    }
    restoreDomSheetCells(targetSheetName);
    refreshFocusedSheetCellPendingSnapshot();
    const sheetTabBar = document.getElementById("sheet-tab-bar");
    if (sheetTabBar) {
      sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").forEach((b) => {
        b.classList.toggle("active", b.dataset.sheetTab === targetSheetName);
      });
    }
    const defaultRef = targetSheetName === "Sheet1" ? "F2" : "C1";
    updateSheetSelectionAndAggregates(defaultRef);
    const fb = document.getElementById("status-command-feedback");
    if (fb) fb.textContent = "시트 탭 전환: " + targetSheetName;
  }

  function setupDocTableAndSheetTabHandlers() {
    const docEditor = document.getElementById("doc-editor");
    if (docEditor) {
      docEditor.addEventListener("click", (e) => {
        const addRowBtn = e.target.closest("#btn-doc-add-row");
        if (addRowBtn) {
          pushUndoSnapshot();
          appendDocTableRow();
          return;
        }
        const mergeBtn = e.target.closest("#btn-doc-merge-toggle");
        if (mergeBtn) {
          const tbody = document.getElementById("doc-table-body");
          if (!tbody) return;
          const sumRow = document.getElementById("doc-table-sum-row");
          const cells = sumRow ? sumRow.querySelectorAll("td") : tbody.querySelectorAll("tr td");
          if (cells.length < 2) return;
          pushUndoSnapshot();
          const isMerged = cells[0].colSpan > 1;
          cells[0].colSpan = isMerged ? 1 : 2;
          cells[1].hidden = !isMerged;
          mergeBtn.textContent = isMerged ? "셀 병합 토글" : "셀 병합 해제";
          return;
        }
        const evalDocBtn = e.target.closest("#btn-doc-eval-table");
        if (evalDocBtn) {
          computeDocTableSummary();
        }
      });
    }

    const sheetTabBar = document.getElementById("sheet-tab-bar");
    const addSheetBtn = document.getElementById("btn-add-sheet-tab");
    if (sheetTabBar) {
      sheetTabBar.addEventListener("click", (e) => {
        const btn = e.target.closest(".sheet-tab-btn");
        if (!btn || btn.id === "btn-add-sheet-tab") return;
        const targetSheet = btn.dataset.sheetTab || "Sheet1";
        switchSheetTab(targetSheet);
      });
    }
    if (addSheetBtn && sheetTabBar) {
      addSheetBtn.addEventListener("click", () => {
        pushUndoSnapshot();
        const count = sheetTabBar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]").length + 1;
        const sheetKey = "Sheet" + count;
        const newTab = document.createElement("button");
        newTab.className = "sheet-tab-btn";
        newTab.dataset.sheetTab = sheetKey;
        newTab.textContent = "시트 " + count;
        addSheetBtn.before(newTab);
        switchSheetTab(sheetKey);
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
      const keyLower = e.key.toLowerCase();
      if ((e.metaKey || e.ctrlKey) && keyLower === "k") {
        e.preventDefault();
        togglePaletteModal(modal, cmdSearchInput);
      } else if ((e.metaKey || e.ctrlKey) && keyLower === "s") {
        e.preventDefault();
        void saveActiveModuleDocument();
      } else if ((e.metaKey || e.ctrlKey) && !e.shiftKey && keyLower === "z") {
        e.preventDefault();
        void performWorkspaceUndo();
      } else if ((e.metaKey || e.ctrlKey) && (keyLower === "y" || (e.shiftKey && keyLower === "z"))) {
        e.preventDefault();
        void performWorkspaceRedo();
      } else if (e.key === "Escape" && modal?.open && typeof modal.close === "function") {
        modal.close();
      }
    });
  }

  async function initWorkspace() {
    ensureDefaultDocTabStore();
    sheetDataStore.Sheet1 = snapshotDomSheetCells();
    const boot = await invokeIpc("get_workspace_bootstrap", "");
    setupTabsAndRibbonChrome(boot);
    renderModuleOutline("DOC");
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
