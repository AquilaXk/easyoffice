(function (host) {
  "use strict";

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
    HWP: "쪽·결재·원고지 도구",
    SHEET: "수식·피벗 분석",
    SLIDE: "발표자·레이저 도구",
    PDF: "전자결재·주석 도구",
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
      { suffix: "data-sort", title: "오름차순·내림차순 정렬", shortcut: "⌃⌥S" },
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
      aspect: "16:9",
    };
  }

  function createInitialSlideDeckState() {
    return [{ title: "", lower: "", notes: "", transition: "fade", ...createEmptySlideStyleFields() }];
  }

  function textSpec(v) {
    return { t: "text", v };
  }

  function elSpec(tag, attrs, data, children) {
    return { t: "el", tag, attrs: attrs || {}, data: data || {}, children: children || [] };
  }

  const DEFAULT_HWP_EDITOR_NODES = [
    elSpec("p", {}, {}, [elSpec("br")]),
    elSpec("p", { id: "hwp-manuscript-preview", class: "manuscript-notice", hidden: "" }, {}, [
      textSpec("한글(HWP/HWPX) 전용 조판 영역: 결재란 · 누름틀 · 200자 원고지 격자 활성"),
    ]),
  ];

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
    for (const key of RIBBON_STATE_SYNC_KEYS) {
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

  // Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md),
  // `apps/studio-host/src/core/platform.ts` `normalizeShortcutLabel`: the same macOS glyph notation.
  // "Ctrl+Alt+O" -> "⌥⌘O": macOS writes the modifiers as glyphs in the order ⌃⌥⇧⌘, with no "+".
  const MAC_MODIFIER_GLYPHS = [["Ctrl", "⌘"], ["Cmd", "⌘"], ["Option", "⌥"], ["Alt", "⌥"], ["Shift", "⇧"]];
  const MAC_MODIFIER_ORDER = "⌃⌥⇧⌘";
  const PLUS_SHORTCUT = /^(?:(?:Ctrl|Cmd|Option|Alt|Shift)\+)+[^+\s]+$/;

  function toMacShortcut(text) {
    if (!PLUS_SHORTCUT.test(text)) return text;
    const parts = text.split("+");
    const key = parts.pop();
    const glyphs = parts.map((name) => MAC_MODIFIER_GLYPHS.find(([word]) => word === name)[1]);
    return [...new Set(glyphs)].sort((a, b) => MAC_MODIFIER_ORDER.indexOf(a) - MAC_MODIFIER_ORDER.indexOf(b)).join("") + key;
  }

  function formatPlatformShortcut(rawShortcut, shortcutModifier) {
    const text = String(rawShortcut || "");
    if (!text) return text;
    if (shortcutModifier === "⌘") return toMacShortcut(text);
    if (shortcutModifier !== "Ctrl") return text;
    return text
      .replaceAll("⌃⌘", "Ctrl+Alt+")
      .replace(/\bCtrl\+Cmd\+/g, "Ctrl+Alt+")
      .replaceAll("⌘", "Ctrl+")
      .replaceAll("⌃", "Ctrl+")
      .replaceAll("⌥", "Alt+")
      .replaceAll("⇧", "Shift+")
      .replace(/\bCmd\+/g, "Ctrl+")
      .replace(/\bOption\+/g, "Alt+")
      .replaceAll("Ctrl+Ctrl+", "Ctrl+");
  }

  // Single source of truth for every shortcut label the WebUI renders (menus, tooltips, ribbon,
  // palette). Each entry is dispatched by app.js `handleWorkspaceShortcutKeydown`, except
  // `native` entries (browser editing keys left untouched) and `external` entries (handled by
  // workspace-tools.js). Browser-reserved keys (Ctrl+N/T/W/R, Ctrl+Shift+R/T/J/C/N/I, F5, ...)
  // must never be listed here, except the two window commands marked `desktopOnly`: they are bound
  // only inside the desktop shell, which owns its windows (PD-088). `forwardFromEditor` marks the
  // combos the embedded HWP editor forwards to the shell over the embed port (the editor sends the
  // combo string only, never typed text). `macOnly` binds the combo on macOS alone: the reference
  // app gives PDF export ⌘E through its macOS menu and leaves Ctrl+E to the editor's delete everywhere else.
  const WORKSPACE_SHORTCUT_BINDINGS = Object.freeze([
    { keys: "Ctrl+O", action: "file-open", forwardFromEditor: true },
    { keys: "Ctrl+N", action: "file-new", desktopOnly: true, forwardFromEditor: true },
    { keys: "Ctrl+W", action: "window-close", desktopOnly: true, forwardFromEditor: true },
    { keys: "Ctrl+Q", action: "app-quit", desktopOnly: true, forwardFromEditor: true },
    { keys: "Ctrl+S", action: "studio-save", forwardFromEditor: true },
    { keys: "Ctrl+Shift+S", action: "file-save-as", forwardFromEditor: true },
    { keys: "Ctrl+Z", action: "studio-undo" },
    { keys: "Ctrl+Y", action: "studio-redo" },
    { keys: "Ctrl+Shift+Z", action: "studio-redo" },
    { keys: "Ctrl+Alt+O", action: "recent-dialog", forwardFromEditor: true },
    { keys: "Ctrl+P", action: "print", desktopOnly: true, forwardFromEditor: true },
    { keys: "Ctrl+E", action: "hwp-export-pdf", module: "HWP", desktopOnly: true, macOnly: true, forwardFromEditor: true },
    { keys: "Ctrl+K", action: "palette", forwardFromEditor: true },
    // Find and replace as in the reference editor (Ctrl+F, Ctrl+F2). Ctrl+H is left alone: it is the
    // system "hide" key on macOS and the history page in browsers.
    { keys: "Ctrl+F", action: "find" },
    { keys: "Ctrl+F2", action: "replace" },
    { keys: "Ctrl+B", action: "home-bold", ribbon: true },
    { keys: "Ctrl+I", action: "home-italic", ribbon: true },
    { keys: "Ctrl+U", action: "home-underline", ribbon: true },
    // Zoom keys as in the reference app (Ctrl+= / Ctrl+- / Ctrl+0). The embedded editor keeps its own
    // zoom keys, so they are not forwarded; Ctrl+Shift++ is the same key as Ctrl+= on layouts where + needs Shift.
    { keys: "Ctrl+=", action: "zoom-in" },
    { keys: "Ctrl+Shift++", action: "zoom-in" },
    { keys: "Ctrl+-", action: "zoom-out" },
    { keys: "Ctrl+0", action: "view-zoom", ribbon: true },
    { keys: "Ctrl+Alt+D", action: "view-theme", ribbon: true },
    { keys: "Ctrl+X", action: "clip-cut", native: true },
    { keys: "Ctrl+C", action: "clip-copy", native: true },
    { keys: "Ctrl+V", action: "clip-paste", native: true },
    { keys: "/", action: "palette", bareKey: true },
    { keys: "Insert", action: "insert-mode", bareKey: true, external: true },
    // Slide module only. `module` gates the binding to the active module (every other module keeps
    // the browser default, e.g. F5 reload); `bareKey` keeps keys out of text fields; `scope`
    // requires focus inside the given selector. F5 is browser-reserved, so it is only taken over
    // in the slide module while focus is outside every text field (documented in PD-080).
    { keys: "F5", action: "slide-show-start", module: "SLIDE", bareKey: true },
    { keys: "Shift+F5", action: "slide-show-current", module: "SLIDE", bareKey: true },
    { keys: "Alt+ArrowUp", action: "slide-move-up", module: "SLIDE", scope: "#slide-thumb-list" },
    { keys: "Alt+ArrowDown", action: "slide-move-down", module: "SLIDE", scope: "#slide-thumb-list" },
    { keys: "Delete", action: "slide-delete", module: "SLIDE", bareKey: true, scope: "#slide-thumb-list" },
  ]);

  // ---- menu bar -> ribbon command (PD-098) ----------------------------------------------------
  // Menu items carry `data-studio-menu-cmd`: either the suffix of a ribbon command id (`home-bold`,
  // run as `<module>-home-bold`), a module-specific id (`sheet-mod-pivot`, only in its module), or
  // one of three legacy aliases the editor menus used before. This is the one place that maps them.
  const MENU_MODULE_PREFIX = { DOC: "document", HWP: "hwp", SHEET: "sheet", SLIDE: "slide", PDF: "pdf", CONV: "document" };
  const MODULE_SPECIFIC_PREFIX = { DOC: "doc", HWP: "hwp", SHEET: "sheet", SLIDE: "slide", PDF: "pdf" };
  const MODULE_SPECIFIC_ID = /^(doc|hwp|sheet|slide|pdf)-mod-/;
  const LEGACY_MENU_ALIASES = {
    "mod-footnote": { DOC: "doc-mod-footnote" },
    "mod-header": { HWP: "hwp-mod-header" },
  };

  function allRibbonCommandIds() {
    const ids = [];
    for (const mod of Object.keys(RIBBON_CATEGORY_TOOLS)) {
      for (const cat of Object.keys(RIBBON_CATEGORY_TOOLS[mod])) {
        for (const tool of RIBBON_CATEGORY_TOOLS[mod][cat]) ids.push(tool.id);
      }
    }
    return ids;
  }
  const ALL_RIBBON_COMMAND_ID_SET = new Set(allRibbonCommandIds());

  // The ribbon command id a menu command runs in `moduleCode`, or null when it has none there.
  function resolveMenuCommand(cmd, moduleCode) {
    const alias = LEGACY_MENU_ALIASES[cmd];
    if (alias) return alias[moduleCode] || null;
    if (MODULE_SPECIFIC_ID.test(cmd)) {
      return MODULE_SPECIFIC_PREFIX[moduleCode] && cmd.startsWith(MODULE_SPECIFIC_PREFIX[moduleCode] + "-mod-") ? cmd : null;
    }
    const id = (MENU_MODULE_PREFIX[moduleCode] || "document") + "-" + cmd;
    return ALL_RIBBON_COMMAND_ID_SET.has(id) ? id : null;
  }

  function resolveRibbonShortcutLabel(commandId, rawShortcut) {
    const suffix = String(commandId || "").replace(/^[a-z]+-/, "");
    const normalized = formatPlatformShortcut(String(rawShortcut || ""), "Ctrl");
    const bound = WORKSPACE_SHORTCUT_BINDINGS.some(
      (b) => b.ribbon && b.action === suffix && b.keys === normalized
    );
    return bound ? String(rawShortcut) : "";
  }

  host.__EASYOFFICE_RIBBON_CATALOG__ = {
    WORKSPACE_SHORTCUT_BINDINGS,
    resolveRibbonShortcutLabel,
    resolveMenuCommand,
    allRibbonCommandIds,
    MODULE_LABELS,
    MODULE_SPECS,
    CONTEXTUAL_TOOL_BADGES,
    MODULE_SHORT_BADGES,
    RIBBON_STYLE_LABELS,
    RIBBON_CATEGORY_ORDER,
    RIBBON_CATEGORY_TOOLS,
    DEFAULT_HWP_EDITOR_NODES,
    createEmptySlideStyleFields,
    createInitialSlideDeckState,
    createDefaultRibbonState,
    syncLocalRibbonStateFromBackend,
    formatPlatformShortcut,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
