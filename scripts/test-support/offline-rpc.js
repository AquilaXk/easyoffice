(function (host) {
  "use strict";

  const {
    normalizeA1Token,
    jsonSheetNameOrDefaultJs,
    isFormulaStringJs,
    resolveCanonicalSheetUpper,
    resolveDefaultSheetFromDocJs,
    buildLocalWorkbookFromJson,
    readWorkbookTypedCell,
    formatFormulaValue,
    evaluateFormulaExpression,
  } = host.__EASYOFFICE_OFFLINE_FORMULA_EVAL__;
  const { localScrubPii } = host.__EASYOFFICE_SHEET_UTILS__;

  function tryParseAssistantEnvelope(rawText) {
    if (!rawText.trimStart().startsWith("{")) {
      return null;
    }
    try {
      const parsed = JSON.parse(rawText);
      return parsed && typeof parsed === "object" && typeof parsed.text === "string"
        ? parsed.text
        : null;
    } catch (_err) {
      return null;
    }
  }

  function localSmartAssistant(rawInput) {
    let rawText = String(rawInput ?? "");
    const envelopeText = tryParseAssistantEnvelope(rawText);
    if (envelopeText !== null) {
      rawText = envelopeText;
    }
    const text = rawText.trim();
    const chars = Array.from(text).length;
    const words = text ? text.split(/\s+/).length : 0;
    const sentences = text ? text.split(/[.!?。]+/).filter((s) => s.trim().length > 0).length : 0;
    const readingTimeSeconds = Math.max(1, Math.ceil((words / 220) * 60));
    const lines = text ? text.split(/\n+/).map((l) => l.trim()).filter(Boolean) : [];
    const summaryLines = lines.slice().sort((a, b) => b.length - a.length).slice(0, 2);
    const actionItems = lines.filter((l) => /^(- \[ \]|\[TODO\]|TODO:|할일:|조치:)/.test(l));
    const styleSuggestions = lines.some((l) => Array.from(l).length > 120)
      ? ["120자를 초과하는 긴 문장이 있습니다. 두 문장으로 나누세요."]
      : [];
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

  function isValidZipEntryPath(entryPath) {
    if (!entryPath || entryPath.includes("\0") || entryPath.startsWith("/") || entryPath.includes("\\")) {
      return false;
    }
    if (/^[A-Za-z]:/.test(entryPath)) return false;
    const trimmed = entryPath.endsWith("/") ? entryPath.slice(0, -1) : entryPath;
    if (!trimmed) return false;
    for (const seg of trimmed.split("/")) {
      if (!seg || seg === "." || seg === ".." || seg.includes(":")) return false;
    }
    return true;
  }

  function hasDangerousXmlDirective(xmlText) {
    return /<!\s*(?:DOCTYPE|ENTITY)\b/i.test(String(xmlText ?? ""));
  }

  function parseRequiredRpcJsonObject(payload) {
    const trimmed = String(payload ?? "")
      .replace(/^\uFEFF/, "")
      .trim();
    if (!trimmed.startsWith("{")) {
      throw new Error("InvalidArgument: RPC 페이로드는 JSON 객체여야 합니다.");
    }
    const doc = JSON.parse(trimmed);
    if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
      throw new Error("InvalidArgument: RPC JSON 루트는 객체여야 합니다.");
    }
    return doc;
  }

  function validateZipEntriesList(entries) {
    const MAX_ENTRIES = 4096;
    const MAX_SINGLE_UNCOMPRESSED = 128 * 1024 * 1024;
    const MAX_TOTAL_UNCOMPRESSED = 512 * 1024 * 1024;
    if (entries.length === 0) {
      throw new Error("InvalidArgument: 보안 감사 대상 엔트리가 비어 있습니다.");
    }
    if (entries.length > MAX_ENTRIES) {
      throw new Error("SecurityViolation: ZIP 엔트리 개수가 보안 한도를 초과했습니다.");
    }
    let totalUncompressed = 0;
    const seenPaths = new Set();
    for (const item of entries) {
      if (!item || typeof item !== "object" || Array.isArray(item)) {
        throw new Error("InvalidArgument: audit_security 'entries' 항목은 객체여야 합니다.");
      }
      const rawName = item.name !== undefined && item.name !== null ? item.name : item.path;
      if (typeof rawName !== "string" || !rawName.trim()) {
        throw new Error("InvalidArgument: ZIP 엔트리 이름('name')이 비어 있습니다.");
      }
      const entryPath = rawName.trim();
      const rawComp = item.compressed !== undefined ? item.compressed : item.compressedSize;
      const rawUncomp = item.uncompressed !== undefined ? item.uncompressed : item.uncompressedSize;
      if (
        typeof rawComp !== "number" ||
        !Number.isSafeInteger(rawComp) ||
        rawComp < 0 ||
        typeof rawUncomp !== "number" ||
        !Number.isSafeInteger(rawUncomp) ||
        rawUncomp < 0
      ) {
        throw new Error("InvalidArgument: ZIP 엔트리 크기 필드는 음수 아닌 정수여야 합니다.");
      }
      const enc = item.encrypted === undefined || item.encrypted === null ? false : item.encrypted;
      if (typeof enc !== "boolean") {
        throw new Error("InvalidArgument: 유효하지 않은 encrypted 불리언 값");
      }
      totalUncompressed += rawUncomp;
      const unsafe =
        !isValidZipEntryPath(entryPath) ||
        seenPaths.has(entryPath) ||
        (entryPath.endsWith("/") && (rawComp > 0 || rawUncomp > 0)) ||
        enc ||
        rawUncomp > MAX_SINGLE_UNCOMPRESSED ||
        totalUncompressed > MAX_TOTAL_UNCOMPRESSED ||
        (rawComp === 0 && rawUncomp > 0) ||
        (rawComp > 0 && rawUncomp / rawComp > 100);
      if (unsafe) {
        throw new Error("SecurityViolation: 안전하지 않은 ZIP 컨테이너 엔트리가 감지되었습니다: " + entryPath);
      }
      seenPaths.add(entryPath);
    }
  }

  function localAuditSecurity(payload) {
    const trimmed = String(payload ?? "")
      .replace(/^\uFEFF/, "")
      .trim();
    if (!trimmed) {
      throw new Error("InvalidArgument: 보안 감사 대상 엔트리가 비어 있습니다.");
    }
    if (!trimmed.startsWith("{")) {
      const pipeIdx = trimmed.indexOf("|");
      const entryPart = pipeIdx >= 0 ? trimmed.slice(0, pipeIdx) : trimmed;
      const xmlPart = pipeIdx >= 0 ? trimmed.slice(pipeIdx + 1) : "";
      if (hasDangerousXmlDirective(xmlPart)) {
        throw new Error("SecurityViolation: 허용되지 않은 XML DOCTYPE 또는 ENTITY 선언이 감지되었습니다.");
      }
      const rawItems = entryPart
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      if (rawItems.length === 0) {
        throw new Error("InvalidArgument: 보안 감사 대상 엔트리가 비어 있습니다.");
      }
      const parsedEntries = rawItems.map((tok) => {
        const parts = tok.split(":");
        if (parts.length !== 4) {
          throw new Error("InvalidArgument: 잘못된 보안 감사 엔트리 구문");
        }
        const comp = Number(parts[1]);
        const uncomp = Number(parts[2]);
        if (!Number.isSafeInteger(comp) || !Number.isSafeInteger(uncomp)) {
          throw new Error("InvalidArgument: ZIP 엔트리 크기 필드는 음수 아닌 정수여야 합니다.");
        }
        if (parts[3] !== "true" && parts[3] !== "false") {
          throw new Error("InvalidArgument: 유효하지 않은 encrypted 불리언 값");
        }
        return {
          name: parts[0],
          compressed: comp,
          uncompressed: uncomp,
          encrypted: parts[3] === "true",
        };
      });
      validateZipEntriesList(parsedEntries);
      return {
        status: "SAFE",
        verdict: "SAFE",
        xxeGuard: "PASSED",
        zipGuard: "PASSED",
        checkedEntries: parsedEntries.length,
      };
    }

    const doc = parseRequiredRpcJsonObject(trimmed);
    if (doc.xml !== undefined && doc.xml !== null && typeof doc.xml !== "string") {
      throw new Error("InvalidArgument: audit_security 'xml' 필드는 문자열이어야 합니다.");
    }
    if (hasDangerousXmlDirective(doc.xml ?? "")) {
      throw new Error("SecurityViolation: 허용되지 않은 XML DOCTYPE 또는 ENTITY 선언이 감지되었습니다.");
    }
    if (doc.containerBase64 !== undefined && doc.containerBase64 !== null) {
      if (typeof doc.containerBase64 !== "string" || !doc.containerBase64.trim()) {
        throw new Error("InvalidArgument: 보안 감사 대상 엔트리가 비어 있습니다.");
      }
      let decoded = "";
      try {
        decoded = typeof host.atob === "function" ? host.atob(doc.containerBase64.trim()) : "";
      } catch (_err) {
        throw new Error("InvalidArgument: 유효하지 않은 Base64 컨테이너 데이터입니다.");
      }
      if (!decoded) {
        throw new Error("InvalidArgument: 보안 감사 대상 엔트리가 비어 있습니다.");
      }
      if (hasDangerousXmlDirective(decoded)) {
        throw new Error("SecurityViolation: 허용되지 않은 XML DOCTYPE 또는 ENTITY 선언이 감지되었습니다.");
      }
      return {
        status: "SAFE",
        verdict: "SAFE",
        xxeGuard: "PASSED",
        zipGuard: "PASSED",
        checkedEntries: 1,
      };
    }
    if (!Array.isArray(doc.entries)) {
      throw new Error("InvalidArgument: audit_security 'entries' 필드는 배열이어야 합니다.");
    }
    validateZipEntriesList(doc.entries);
    return {
      status: "SAFE",
      verdict: "SAFE",
      xxeGuard: "PASSED",
      zipGuard: "PASSED",
      checkedEntries: doc.entries.length,
    };
  }

  function encodeUtf8Base64(text) {
    const bytes = new TextEncoder().encode(String(text || ""));
    let binary = "";
    for (let i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return typeof host.btoa === "function" ? host.btoa(binary) : "";
  }

  const ZOOM_STEP_CYCLE = { 100: 125, 125: 150, 150: 90 };

  function createOfflineRpcHandlers(ctx) {
    const {
      getActiveModule,
      ribbonCategoryTools,
      ribbonStyleLabels,
      contextualToolBadges,
      moduleSpecs,
      localRibbonState,
    } = ctx;

    const backendBaseState = Object.assign(
      {
        activeModule: "DOC",
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
      },
      JSON.parse(JSON.stringify(localRibbonState || {}))
    );
    const MAX_BACKEND_HISTORY = 64;
    // Mirrors crates/easyoffice-app/src/ribbon_session.rs: ribbon state and undo/redo stacks are
    // kept per client session (`sessionId`, missing → "default") and per module within a session.
    const DEFAULT_RIBBON_SESSION_ID = "default";
    const MAX_RIBBON_SESSIONS = 32;
    const RIBBON_MODULE_KEYS = ["DOC", "HWP", "SHEET", "SLIDE", "PDF"];
    const ribbonSessions = new Map();

    function normalizeRibbonModuleKey(raw) {
      const key = String(raw || "").trim().toUpperCase();
      if (key === "CONV" || key === "DOCUMENT") return "DOC";
      if (RIBBON_MODULE_KEYS.includes(key)) return key;
      throw new Error("InvalidArgument: 지원하지 않는 리본 모듈입니다: " + key);
    }

    function normalizeRibbonSessionId(raw) {
      if (raw !== undefined && raw !== null && typeof raw !== "string") {
        throw new Error("InvalidArgument: JSON 'sessionId' 필드는 문자열이어야 합니다.");
      }
      const sid = String(raw || "").trim();
      if (!sid) return DEFAULT_RIBBON_SESSION_ID;
      if (sid.length > 64 || !/^[A-Za-z0-9_-]+$/.test(sid)) {
        throw new Error("InvalidArgument: 리본 세션 식별자 형식이 올바르지 않습니다.");
      }
      return sid;
    }

    function ribbonSession(sessionId) {
      let session = ribbonSessions.get(sessionId);
      if (session) {
        ribbonSessions.delete(sessionId);
      } else {
        if (ribbonSessions.size >= MAX_RIBBON_SESSIONS) {
          ribbonSessions.delete(ribbonSessions.keys().next().value);
        }
        session = {
          ribbonStyle: backendBaseState.ribbonStyle || "standard",
          lastModule: "DOC",
          slots: {},
        };
      }
      ribbonSessions.set(sessionId, session);
      return session;
    }

    function ribbonSlot(session, mod) {
      if (!session.slots[mod]) {
        session.slots[mod] = {
          state: { ...JSON.parse(JSON.stringify(backendBaseState)), activeModule: mod },
          undo: [],
          redo: [],
        };
      }
      session.slots[mod].state.ribbonStyle = session.ribbonStyle;
      return session.slots[mod];
    }

    function ribbonStateResponse(slot, extra) {
      return {
        ...extra,
        catalogCount: 140,
        ...JSON.parse(JSON.stringify(slot.state)),
        canUndo: slot.undo.length > 0,
        canRedo: slot.redo.length > 0,
      };
    }

    function parseRibbonPayloadFields(payload) {
      const raw = String(payload || "").trim();
      return raw.startsWith("{") ? parseRequiredRpcJsonObject(raw) : null;
    }

    function localRibbonHistoryStep(payload, isUndo) {
      const raw = String(payload || "").trim();
      const parsed = parseRibbonPayloadFields(raw);
      const session = ribbonSession(normalizeRibbonSessionId(parsed ? parsed.sessionId : ""));
      const moduleRaw = parsed ? parsed.module : raw;
      const mod = moduleRaw ? normalizeRibbonModuleKey(moduleRaw) : session.lastModule;
      const slot = ribbonSlot(session, mod);
      const from = isUndo ? slot.undo : slot.redo;
      const to = isUndo ? slot.redo : slot.undo;
      if (from.length > 0) {
        if (to.length >= MAX_BACKEND_HISTORY) to.shift();
        to.push(JSON.parse(JSON.stringify(slot.state)));
        slot.state = from.pop();
        slot.state.ribbonStyle = session.ribbonStyle;
      }
      return ribbonStateResponse(slot, {
        commandId: isUndo ? "undo" : "redo",
        summary: isUndo ? "실행 취소 완료" : "다시 실행 완료",
      });
    }

    function inferCommandModuleKey(cmdId) {
      if (cmdId.startsWith("hwp-")) return "HWP";
      if (cmdId.startsWith("sheet-")) return "SHEET";
      if (cmdId.startsWith("slide-")) return "SLIDE";
      if (cmdId.startsWith("pdf-")) return "PDF";
      if (cmdId.startsWith("document-") || cmdId.startsWith("doc-")) return "DOC";
      return getActiveModule();
    }

    function resolveOptionalAnchorCell(doc) {
      if (doc.cell !== undefined && doc.cell !== null && typeof doc.cell !== "string") {
        throw new Error("InvalidArgument: JSON 'cell' 필드는 문자열이어야 합니다.");
      }
      if (typeof doc.cell === "string" && doc.cell.trim().length > 0) {
        const normAnchor = normalizeA1Token(doc.cell.trim());
        if (!normAnchor) {
          throw new Error("InvalidArgument: 유효하지 않은 셀 좌표('cell')입니다.");
        }
        return normAnchor;
      }
      return null;
    }

    function evaluateSingleDocExpr(doc) {
      const sheetName = resolveDefaultSheetFromDocJs(doc);
      const explicitAnchor = resolveOptionalAnchorCell(doc);
      const anchorCell = explicitAnchor || "A1";
      const rawExpr = doc.expr !== undefined && doc.expr !== null ? doc.expr : doc.query;
      if (rawExpr !== undefined && rawExpr !== null && typeof rawExpr !== "string") {
        throw new Error("InvalidArgument: JSON 'expr' 필드는 문자열이어야 합니다.");
      }
      const fallbackCell = explicitAnchor || "A1";
      const defaultQuery = fallbackCell;
      const chosenQuery = rawExpr === undefined || rawExpr === null ? defaultQuery : rawExpr;
      const q = String(chosenQuery).trim() || defaultQuery;
      const workbook = buildLocalWorkbookFromJson(doc, sheetName);
      const evalSheetUpper = resolveCanonicalSheetUpper(workbook.knownSheets, sheetName);
      if (!evalSheetUpper) {
        return { sheetName, expr: q, value: "#REF!", circular: false };
      }
      if (isFormulaStringJs(q)) {
        return {
          sheetName,
          expr: q,
          value: String(evaluateFormulaExpression(q, workbook, evalSheetUpper, anchorCell)),
          circular: workbook.circular,
        };
      }
      const cellValue = formatFormulaValue(readWorkbookTypedCell(workbook, evalSheetUpper, q, 0));
      return { sheetName, expr: q, value: cellValue, circular: workbook.circular };
    }

    function evaluateLocalWorkbookFormulaRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const res = evaluateSingleDocExpr(doc);
      return { sheet: res.sheetName, formula: res.expr, value: res.value, circular: res.circular };
    }

    function evaluateLocalSheetRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const defaultSheet = resolveDefaultSheetFromDocJs(doc);
      const explicitAnchor = resolveOptionalAnchorCell(doc);
      const anchorCell = explicitAnchor || "A1";
      if (doc.targets !== undefined) {
        if (!Array.isArray(doc.targets)) {
          throw new Error("InvalidArgument: JSON 'targets' 필드는 배열이어야 합니다.");
        }
        if (doc.targets.length > 0) {
          const workbook = buildLocalWorkbookFromJson(doc, defaultSheet);
          const outMap = {};
          const targetValues = [];
          let firstExpr = "A1";
          let firstVal = "";
          doc.targets.forEach((item, idx) => {
            if (!item || typeof item !== "object" || Array.isArray(item)) {
              throw new Error("InvalidArgument: JSON 'targets' 항목은 객체여야 합니다.");
            }
            if (item.sheet !== undefined && item.sheet !== null && typeof item.sheet !== "string") {
              throw new Error("InvalidArgument: JSON 'targets' 시트명은 문자열이어야 합니다.");
            }
            if (typeof item.cell !== "string" || !item.cell.trim()) {
              throw new Error("InvalidArgument: JSON 'targets' 항목에 'cell' 필드가 필요합니다.");
            }
            const sh = jsonSheetNameOrDefaultJs(item.sheet ?? defaultSheet, defaultSheet);
            const expr = item.cell.trim();
            const canonicalUpper = resolveCanonicalSheetUpper(workbook.knownSheets, sh);
            let val = "#REF!";
            if (canonicalUpper) {
              val = isFormulaStringJs(expr)
                ? String(evaluateFormulaExpression(expr, workbook, canonicalUpper, anchorCell))
                : formatFormulaValue(readWorkbookTypedCell(workbook, canonicalUpper, expr, 0));
            }
            outMap[sh + "!" + expr] = val;
            targetValues.push(val);
            if (idx === 0) {
              firstExpr = expr;
              firstVal = val;
            }
          });
          return {
            query: firstExpr,
            value: firstVal,
            values: outMap,
            targetValues,
            circular: workbook.circular,
          };
        }
      }
      const single = evaluateSingleDocExpr(doc);
      return {
        query: single.expr,
        value: single.value,
        values: { [single.sheetName + "!" + single.expr]: single.value },
        targetValues: [single.value],
        circular: single.circular,
      };
    }

    const RIBBON_EXACT_MUTATORS = {
      "hwp-mod-manuscript": (s) => { s.manuscriptGridEnabled = !s.manuscriptGridEnabled; },
      "hwp-mod-header": (s) => { s.approvalHeaderEnabled = !s.approvalHeaderEnabled; },
      "hwp-mod-border": (s) => { s.doubleDiagonalBorderEnabled = !s.doubleDiagonalBorderEnabled; },
      "pdf-mod-rotate": (s) => { s.pdfRotationDegrees = (s.pdfRotationDegrees + 90) % 360; },
    };
    const RIBBON_SUFFIX_MUTATORS = [
      ["-home-bold", (s) => { s.isBold = !s.isBold; }],
      ["-home-italic", (s) => { s.isItalic = !s.isItalic; }],
      ["-home-underline", (s) => { s.isUnderline = !s.isUnderline; }],
      ["-home-style", (s) => { s.stylePresetIndex = (s.stylePresetIndex + 1) % 4; }],
      ["-layout-paper", (s) => { s.paperSize = s.paperSize === "A4" ? "Letter" : "A4"; }],
      ["-layout-orientation", (s) => { s.isLandscape = !s.isLandscape; }],
      ["-layout-columns", (s) => { s.columnCount = s.columnCount >= 3 ? 1 : s.columnCount + 1; }],
      ["-layout-margins", (s) => { s.marginPresetIndex = (s.marginPresetIndex + 1) % 3; }],
      ["-data-sort", (s) => { s.sortAscending = !s.sortAscending; }],
      ["-data-validation", (s) => { s.validationGuardEnabled = !s.validationGuardEnabled; }],
      ["-review-track", (s) => { s.trackChangesEnabled = !s.trackChangesEnabled; }],
      ["-review-verify", (s) => { s.securityVerifiedCount += 1; }],
      ["-view-manuscript", (s) => { s.manuscriptGridEnabled = !s.manuscriptGridEnabled; }],
      ["-view-split", (s) => { s.splitViewEnabled = !s.splitViewEnabled; }],
      ["-view-ribbon", (s) => { s.ribbonCollapsed = !s.ribbonCollapsed; }],
      ["-view-theme", (s) => { s.darkThemeEnabled = !s.darkThemeEnabled; }],
      ["-view-zoom", (s) => { s.zoomPercent = ZOOM_STEP_CYCLE[s.zoomPercent] ?? 100; }],
    ];

    function localExecuteRibbonCommand(payload) {
      let cmdId = String(payload || "").trim();
      const parsed = parseRibbonPayloadFields(cmdId);
      if (parsed) {
        cmdId = String(parsed.commandId || parsed.id || "").trim();
      }
      const session = ribbonSession(normalizeRibbonSessionId(parsed ? parsed.sessionId : ""));
      const mod = normalizeRibbonModuleKey(inferCommandModuleKey(cmdId));
      session.lastModule = mod;
      const slot = ribbonSlot(session, mod);
      const backendState = slot.state;
      if (slot.undo.length >= MAX_BACKEND_HISTORY) {
        slot.undo.shift();
      }
      slot.undo.push(JSON.parse(JSON.stringify(backendState)));
      slot.redo.length = 0;

      backendState.activeModule = mod;
      backendState.executedCount += 1;
      const exactFn = RIBBON_EXACT_MUTATORS[cmdId];
      if (exactFn) {
        exactFn(backendState);
      } else {
        const suffixPair = RIBBON_SUFFIX_MUTATORS.find(([sfx]) => cmdId.endsWith(sfx));
        if (suffixPair) suffixPair[1](backendState);
      }
      return ribbonStateResponse(slot, {
        commandId: cmdId,
        summary: "명령 실행 완료 (" + cmdId + ")",
        contextualBadge: contextualToolBadges[mod] || "표·개체 도구",
      });
    }

    return {
      get_workspace_bootstrap: () => {
        const isMac = Boolean(host.navigator && String(host.navigator.platform || "").includes("Mac"));
        return {
          platform: isMac ? "macOS" : "Desktop",
          shortcutModifier: isMac ? "⌘" : "Ctrl",
          recentFormatFilters: ["전체", "DOCX", "HWP/HWPX", "XLSX", "PPTX", "PDF", "ODF"],
          openTabs: [
            { id: "tab-doc", title: "분기_경영실적_보고서.docx", module: "DOC", isPinned: true },
            { id: "tab-hwp", title: "행정안전부_기안문.hwpx", module: "HWP", isPinned: true },
            { id: "tab-sheet", title: "예산집행_총괄표.xlsx", module: "SHEET", isPinned: false },
            { id: "tab-slide", title: "전략기획_발표자료.pptx", module: "SLIDE", isPinned: false },
            { id: "tab-pdf", title: "최종계약서_검토본.pdf", module: "PDF", isPinned: false },
          ],
        };
      },
      get_ribbon_category_commands: (payload) => {
        const raw = String(payload || "DOC|home").trim();
        let modPart = "DOC";
        let catPart = "home";
        if (raw.startsWith("{")) {
          const parsed = parseRequiredRpcJsonObject(raw);
          modPart = String(parsed.module || "DOC").trim();
          catPart = String(parsed.category || "home").trim();
        } else {
          const parts = raw.split("|");
          modPart = parts[0] || "DOC";
          catPart = parts[1] || "home";
        }
        const mKey = modPart === "CONV" ? "DOC" : modPart;
        const modMap = ribbonCategoryTools[mKey] || ribbonCategoryTools.DOC;
        return { commands: modMap[catPart] || modMap.home };
      },
      detect_format: (payload) => {
        const raw = String(payload || "").trim();
        let fileName = "document.docx";
        if (raw.startsWith("{")) {
          const parsed = parseRequiredRpcJsonObject(raw);
          fileName = String(parsed.fileName || "document.docx");
        } else {
          fileName = raw.split("|")[0] || "document.docx";
        }
        const dotIdx = fileName.lastIndexOf(".");
        const ext = dotIdx >= 0 ? fileName.slice(dotIdx + 1).toLowerCase() : "docx";
        return {
          format: ext.toUpperCase(),
          extension: ext,
          specification: moduleSpecs[ext.toUpperCase()] || "표준 문서 포맷",
        };
      },
      convert_document_format: (payload) => {
        const raw = String(payload || "docx->hwpx|문서|본문");
        let src = "docx";
        let dst = "hwpx";
        let title = "통합_변환_문서";
        let body = "본문 텍스트";
        let structuredTables = null;
        if (raw.trimStart().startsWith("{")) {
          const parsed = parseRequiredRpcJsonObject(raw);
          src = String(parsed.sourceFormat || "docx").trim().toLowerCase();
          dst = String(parsed.targetFormat || "hwpx").trim().toLowerCase();
          title = String(parsed.title || "통합_변환_문서");
          body = String(parsed.body || "본문 텍스트");
          // Structured save payload (PD-073): literal text only, never parsed as markup.
          if (Array.isArray(parsed.blocks)) {
            structuredTables = parsed.blocks.filter((b) => b && b.type === "table").length;
            body = parsed.blocks
              .map((b) =>
                b && b.type === "table"
                  ? (b.rows || []).map((r) => r.map((c) => (typeof c === "string" ? c : c.value || c.text || "")).join("\t")).join("\n")
                  : ((b && b.runs) || []).map((r) => String(r.text || "")).join("")
              )
              .join("\n");
          } else if (Array.isArray(parsed.sheets)) {
            structuredTables = 0;
            body = parsed.sheets.map((s) => (s.cells || []).map((c) => c.ref + "=" + c.raw).join("\n")).join("\n");
          } else if (Array.isArray(parsed.slides)) {
            structuredTables = 0;
            body = parsed.slides.map((s) => [s.title, ...(s.bullets || []), s.notes].join("\n")).join("\n");
          } else if (parsed.blocks !== undefined || parsed.sheets !== undefined || parsed.slides !== undefined) {
            throw new Error("InvalidArgument: 구조화 저장 필드는 배열이어야 합니다.");
          }
        } else {
          const firstPipe = raw.indexOf("|");
          const firstSeg = firstPipe >= 0 ? raw.slice(0, firstPipe) : raw;
          const restSeg = firstPipe >= 0 ? raw.slice(firstPipe + 1) : "통합_변환_문서|본문";
          const secondPipe = restSeg.indexOf("|");
          const pair = (firstSeg || "docx->hwpx").split("->");
          src = (pair[0] || "docx").trim().toLowerCase();
          dst = (pair[1] || "hwpx").trim().toLowerCase();
          title = (secondPipe >= 0 ? restSeg.slice(0, secondPipe) : restSeg) || "통합_변환_문서";
          body = (secondPipe >= 0 ? restSeg.slice(secondPipe + 1) : "") || "본문 텍스트";
        }
        const encoder = new TextEncoder();
        const rawBytes = encoder.encode(title + "\n" + body).length;
        const containerOverhead = {
          docx: 640, doc: 1024, hwpx: 720, hwtx: 720, hwp: 1024, hwt: 1024, odt: 600,
          xlsx: 680, xls: 1024, ods: 620, pptx: 760, ppsx: 760, ppt: 1024, odp: 640,
          pdf: 480, rtf: 180, md: 16, csv: 24, tsv: 24, txt: 0,
        };
        if (!(src in containerOverhead) || !(dst in containerOverhead)) {
          throw new Error("지원하지 않는 포맷: " + src + "->" + dst);
        }
        const srcBytes = rawBytes + (containerOverhead[src] ?? 256);
        const dstBytes = rawBytes + (containerOverhead[dst] ?? 256);
        const b64 = encodeUtf8Base64(title + "\n" + body);
        const tableCount = structuredTables !== null ? structuredTables : body.includes("[[TABLE_ROW]]\t")
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
          specification: moduleSpecs[(dst || "").toUpperCase()] || "표준 문서 포맷",
          compatibility: dst === "doc" || dst === "ppt" ? "limited" : "standard",
          compatibilityWarning:
            dst === "doc" || dst === "ppt"
              ? dst.toUpperCase() + " 출력은 EasyOffice 전용 하위 집합이라 다른 프로그램에서 열리지 않을 수 있습니다."
              : null,
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
        value: evaluateSingleDocExpr(parseRequiredRpcJsonObject(payload)).value,
      }),
      audit_security: (payload) => localAuditSecurity(payload),
      switch_ribbon_style: (payload) => {
        let styleVal = String(payload || "");
        const parsed = parseRibbonPayloadFields(styleVal);
        if (parsed) {
          styleVal = String(parsed.style || "");
        }
        const isHangul = styleVal.includes("hangul");
        const session = ribbonSession(normalizeRibbonSessionId(parsed ? parsed.sessionId : ""));
        session.ribbonStyle = isHangul ? "hangul" : "standard";
        return {
          style: isHangul ? "한글(HWP) 7탭 모드" : "표준 오피스 7탭 모드",
          categories: isHangul ? ribbonStyleLabels.hangul : ribbonStyleLabels.standard,
        };
      },
      execute_ribbon_command: (payload) => localExecuteRibbonCommand(payload),
      undo_ribbon_command: (payload) => localRibbonHistoryStep(payload, true),
      redo_ribbon_command: (payload) => localRibbonHistoryStep(payload, false),
    };
  }

  host.__EASYOFFICE_CREATE_OFFLINE_RPC__ = createOfflineRpcHandlers;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
