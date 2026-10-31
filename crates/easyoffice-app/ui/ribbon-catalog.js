(function (host) {
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

  const MODULE_SHORT_BADGES = {
    DOC: "문서",
    HWP: "한글",
    SHEET: "시트",
    SLIDE: "슬라이드",
    PDF: "PDF",
    CONV: "변환",
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

  function createEmptySlideStyleFields() {
    return {
      titleStyle: {
        fontWeight: "",
        fontStyle: "",
        textDecoration: "",
        color: "",
        fontSize: "",
      },
      lowerColumnCount: "",
      stagePadding: "",
      stageOutline: "",
      stageLayoutState: "",
    };
  }

  function createInitialSlideDeckState() {
    return [
      {
        title: "연간 제품 전략 발표 자료 (16:9 와이드)",
        lower: "핵심 지표 및 요약 다이어그램 영역",
        notes: "핵심 메시지: 주요 문서 포맷 호환성과 수식 계산 결과를 요약하여 전달합니다.",
        transition: "fade",
        ...createEmptySlideStyleFields(),
      },
      {
        title: "문서 포맷 호환성 및 일괄 변환 흐름",
        lower: "DOCX · HWP 5.0 · HWPX · ODF · XLSX · PPTX · PDF · RTF · CSV · Markdown",
        notes: "다양한 문서 포맷 간 서식과 표 구조를 유지하며 상호 변환합니다.",
        transition: "push",
        ...createEmptySlideStyleFields(),
      },
      {
        title: "대용량 문서 및 다중 시트 처리 성능 요약",
        lower: "빠른 문서 열기 · 수식 재계산 · 피벗 집계 · PDF 출력",
        notes: "대용량 스프레드시트와 프레젠테이션 문서를 지연 없이 편집합니다.",
        transition: "zoom",
        ...createEmptySlideStyleFields(),
      },
    ];
  }

  const DEFAULT_HWP_TABLE_ROWS =
    '<tr><td>표준화 총괄과</td><td>김민준 서기관</td><td data-doc-num="520">520</td></tr>' +
    '<tr><td>정보보호 감사팀</td><td>박서연 사무관</td><td data-doc-num="340">340</td></tr>' +
    '<tr id="doc-table-sum-row"><td>합계 (=SUM(ABOVE))</td><td>자동 집계</td><td id="doc-table-sum-cell" data-doc-num="860">860</td></tr>';

  const DEFAULT_HWP_EDITOR_HTML =
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
    DEFAULT_HWP_TABLE_ROWS +
    "</tbody></table>" +
    '<p id="hwp-manuscript-preview" class="hwp-manuscript-notice" hidden>한글(HWP/HWPX) 전용 조판 영역: 결재란 · 누름틀 · 200자 원고지 격자 활성</p>';

  function createDefaultRibbonState() {
    return {
      activeModule: "DOC",
      activeCategory: "home",
      ribbonStyle: "standard",
      isBold: false,
      isItalic: false,
      isUnderline: false,
      stylePresetIndex: 0,
      insertedTableCount: 0,
      insertedChartCount: 0,
      insertedShapeCount: 0,
      insertedLinkCount: 0,
      paperSize: "A4",
      columnCount: 1,
      isLandscape: false,
      marginPresetIndex: 0,
      autosumAppliedCount: 0,
      formulaWizardOpen: false,
      sortAscending: true,
      validationGuardEnabled: true,
      tocGenerated: false,
      footnoteCount: 0,
      mailMergeAppliedCount: 0,
      tableFormulaEvaluatedCount: 0,
      manuscriptGridEnabled: false,
      approvalHeaderEnabled: true,
      clickHereFieldCount: 0,
      doubleDiagonalBorderEnabled: false,
      pivotSummaryEnabled: false,
      conditionalFormatEnabled: false,
      freezePanesEnabled: false,
      masterLayoutIndex: 0,
      snapGuideEnabled: true,
      slideTransitionIndex: 0,
      presenterModeActive: false,
      pdfHighlightCount: 0,
      pdfRotationDegrees: 0,
      pdfSplitMergeCount: 0,
      pdfConvertedToOfficeCount: 0,
      trackChangesEnabled: false,
      reviewCommentCount: 0,
      securityVerifiedCount: 0,
      statsInspectedCount: 0,
      splitViewEnabled: false,
      ribbonCollapsed: false,
      darkThemeEnabled: false,
      zoomPercent: 100,
      executedCount: 0,
    };
  }

  const RIBBON_STATE_SYNC_KEYS = Object.keys(createDefaultRibbonState());

  function normalizeModuleCode(rawMod) {
    const up = String(rawMod || "").trim().toUpperCase();
    if (up === "DOCUMENT" || up === "DOC") return "DOC";
    if (up === "HWP" || up === "HWPX") return "HWP";
    if (up === "SHEET" || up === "XLSX") return "SHEET";
    if (up === "SLIDE" || up === "PPTX") return "SLIDE";
    if (up === "PDF") return "PDF";
    if (up === "CONV") return "CONV";
    return null;
  }

  function syncLocalRibbonStateFromBackend(localRibbonState, rawOrParsed) {
    if (!localRibbonState || rawOrParsed === undefined || rawOrParsed === null) {
      return localRibbonState;
    }
    let parsed = rawOrParsed;
    if (typeof rawOrParsed === "string") {
      try {
        parsed = JSON.parse(rawOrParsed);
      } catch {
        return localRibbonState;
      }
    }
    if (!parsed || typeof parsed !== "object") return localRibbonState;
    for (let i = 0; i < RIBBON_STATE_SYNC_KEYS.length; i += 1) {
      const key = RIBBON_STATE_SYNC_KEYS[i];
      if (parsed[key] === undefined) continue;
      if (key === "activeModule") {
        const modCode = normalizeModuleCode(parsed.activeModule);
        if (modCode) localRibbonState.activeModule = modCode;
      } else {
        localRibbonState[key] = parsed[key];
      }
    }
    return localRibbonState;
  }

  host.__EASYOFFICE_RIBBON_CATALOG__ = {
    MODULE_ACCENTS,
    MODULE_LABELS,
    MODULE_SPECS,
    CONTEXTUAL_TOOL_BADGES,
    MODULE_SHORT_BADGES,
    RIBBON_STYLE_LABELS,
    RIBBON_CATEGORY_ORDER,
    RIBBON_CATEGORY_TOOLS,
    DEFAULT_HWP_EDITOR_HTML,
    createEmptySlideStyleFields,
    createInitialSlideDeckState,
    createDefaultRibbonState,
    syncLocalRibbonStateFromBackend,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
