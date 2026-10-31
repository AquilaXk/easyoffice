(function (host) {
  "use strict";

  const {
    parseCellRefParts,
    colIndexToLetters,
    normalizeFormattedNumericCell,
    jsonSheetNameOrDefaultJs,
    quoteFormulaSheetName,
    isFormulaStringJs,
    resolveCanonicalSheetUpper,
    resolveDefaultSheetFromDocJs,
    buildLocalWorkbookFromJson,
    readWorkbookTypedCell,
    formatFormulaValue,
    evaluateFormulaExpression,
  } = host.__EASYOFFICE_OFFLINE_FORMULA_EVAL__;

  function isAsciiDigitCode(c) {
    return c >= 48 && c <= 57;
  }

  function isAsciiAlphaNumCode(c) {
    return (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
  }

  function consumeDigitsJs(str, pos, minLen, maxLen) {
    let end = pos;
    while (end < str.length && end - pos < maxLen && isAsciiDigitCode(str.charCodeAt(end))) {
      end += 1;
    }
    return end - pos >= minLen ? end : -1;
  }

  function matchEmailAtJs(str, idx) {
    if (!isAsciiAlphaNumCode(str.charCodeAt(idx))) return null;
    let p = idx;
    while (p < str.length) {
      const c = str.charCodeAt(p);
      if (isAsciiAlphaNumCode(c) || c === 46 || c === 95 || c === 45 || c === 43) {
        p += 1;
      } else {
        break;
      }
    }
    if (p === idx || p >= str.length || str.charCodeAt(p) !== 64) return null;
    const domStart = p + 1;
    let q = domStart;
    while (q < str.length) {
      const c = str.charCodeAt(q);
      if (isAsciiAlphaNumCode(c) || c === 46 || c === 45) {
        q += 1;
      } else {
        break;
      }
    }
    while (q > domStart && !isAsciiAlphaNumCode(str.charCodeAt(q - 1))) {
      q -= 1;
    }
    const domain = str.slice(domStart, q);
    if (domain.length >= 3 && isAsciiAlphaNumCode(domain.charCodeAt(0)) && domain.includes(".")) {
      return { len: q - idx, replacement: "***@***" };
    }
    return null;
  }

  function matchRrnAtJs(str, idx) {
    if (idx > 0 && isAsciiDigitCode(str.charCodeAt(idx - 1))) return null;
    if (
      idx + 14 <= str.length &&
      /^\d{6}-\d{7}$/.test(str.slice(idx, idx + 14)) &&
      (idx + 14 === str.length || !isAsciiDigitCode(str.charCodeAt(idx + 14)))
    ) {
      return { len: 14, replacement: "******-*******" };
    }
    if (
      idx + 13 <= str.length &&
      /^\d{13}$/.test(str.slice(idx, idx + 13)) &&
      (idx + 13 === str.length || !isAsciiDigitCode(str.charCodeAt(idx + 13)))
    ) {
      const month = Number(str.slice(idx + 2, idx + 4));
      const day = Number(str.slice(idx + 4, idx + 6));
      const gender = Number(str[idx + 6]);
      if (month >= 1 && month <= 12 && day >= 1 && day <= 31 && gender >= 1 && gender <= 8) {
        return { len: 13, replacement: "******-*******" };
      }
    }
    return null;
  }

  function matchPhoneAtJs(str, idx) {
    if (idx > 0 && (isAsciiDigitCode(str.charCodeAt(idx - 1)) || str[idx - 1] === "+")) {
      return null;
    }
    if (str.startsWith("+82", idx)) {
      let p = idx + 3;
      if (p < str.length && (str[p] === "-" || str[p] === " ")) p += 1;
      const pArea = consumeDigitsJs(str, p, 1, 3);
      if (pArea > 0) {
        let p2 = pArea;
        if (p2 < str.length && (str[p2] === "-" || str[p2] === " ")) p2 += 1;
        const pMid = consumeDigitsJs(str, p2, 3, 4);
        if (pMid > 0) {
          let p3 = pMid;
          if (p3 < str.length && (str[p3] === "-" || str[p3] === " ")) p3 += 1;
          const pEnd = consumeDigitsJs(str, p3, 4, 4);
          if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
            return { len: pEnd - idx, replacement: "+82-**-****-****" };
          }
        }
      }
    }
    if (/^01[016789]/.test(str.slice(idx, idx + 3))) {
      const p = idx + 3;
      if (p < str.length && (str[p] === "-" || str[p] === " ")) {
        const sep = str[p];
        const pMid = consumeDigitsJs(str, p + 1, 3, 4);
        if (pMid > 0 && pMid < str.length && str[pMid] === sep) {
          const pEnd = consumeDigitsJs(str, pMid + 1, 4, 4);
          if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
            return { len: pEnd - idx, replacement: "010-****-****" };
          }
        }
      } else if (
        idx + 11 <= str.length &&
        /^\d{11}$/.test(str.slice(idx, idx + 11)) &&
        (idx + 11 === str.length || !isAsciiDigitCode(str.charCodeAt(idx + 11)))
      ) {
        return { len: 11, replacement: "010-****-****" };
      }
    }
    const landlineMatch = /^(02|0[3-6][1-5])([-\s])/.exec(str.slice(idx, idx + 4));
    if (landlineMatch) {
      const area = landlineMatch[1];
      const sep = landlineMatch[2];
      const pMid = consumeDigitsJs(str, idx + area.length + 1, 3, 4);
      if (pMid > 0 && pMid < str.length && str[pMid] === sep) {
        const pEnd = consumeDigitsJs(str, pMid + 1, 4, 4);
        if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
          return { len: pEnd - idx, replacement: area === "02" ? "02-****-****" : "***-****-****" };
        }
      }
    }
    return null;
  }

  function localScrubPii(rawText) {
    const text = String(rawText ?? "");
    let scrubbedText = "";
    let residentCount = 0;
    let phoneCount = 0;
    let emailCount = 0;
    const spans = [];
    let idx = 0;
    while (idx < text.length) {
      const em = matchEmailAtJs(text, idx);
      const rrn = em ? null : matchRrnAtJs(text, idx);
      const ph = em || rrn ? null : matchPhoneAtJs(text, idx);
      const hit = em || rrn || ph;
      if (hit) {
        scrubbedText += hit.replacement;
        spans.push({ start: idx, end: idx + hit.len, replacement: hit.replacement });
        if (em) emailCount += 1;
        else if (rrn) residentCount += 1;
        else phoneCount += 1;
        idx += hit.len;
        continue;
      }
      const step = text.codePointAt(idx) > 0xffff ? 2 : 1;
      scrubbedText += text.slice(idx, idx + step);
      idx += step;
    }
    return {
      scrubbedText,
      maskedResidentIds: residentCount,
      maskedPhoneNumbers: phoneCount,
      maskedEmails: emailCount,
      totalMasked: residentCount + phoneCount + emailCount,
      spans,
    };
  }

  function localSmartAssistant(rawText) {
    const text = String(rawText || "").trim();
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

  function localAuditSecurity(payload) {
    const doc = parseRequiredRpcJsonObject(payload);
    if (doc.xml !== undefined && doc.xml !== null && typeof doc.xml !== "string") {
      throw new Error("InvalidArgument: audit_security 'xml' 필드는 문자열이어야 합니다.");
    }
    const xmlPart = doc.xml ?? "";
    if (hasDangerousXmlDirective(xmlPart)) {
      throw new Error("SecurityViolation: 허용되지 않은 XML DOCTYPE 또는 ENTITY 선언이 감지되었습니다.");
    }
    const entries = doc.entries === undefined ? [] : doc.entries;
    if (!Array.isArray(entries)) {
      throw new Error("InvalidArgument: audit_security 'entries' 필드는 배열이어야 합니다.");
    }
    const MAX_ENTRIES = 4096;
    const MAX_SINGLE_UNCOMPRESSED = 128 * 1024 * 1024;
    const MAX_TOTAL_UNCOMPRESSED = 512 * 1024 * 1024;
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
    return {
      status: "SAFE",
      xxeGuard: "PASSED",
      zipGuard: "PASSED",
      checkedEntries: entries.length,
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
      getActiveSheetCellRef,
      ribbonCategoryTools,
      ribbonStyleLabels,
      contextualToolBadges,
      moduleSpecs,
      localRibbonState,
    } = ctx;

    const backendState = Object.assign(
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
    const backendUndoStack = [];
    const backendRedoStack = [];
    const MAX_BACKEND_HISTORY = 64;

    function inferCommandModuleKey(cmdId) {
      if (cmdId.startsWith("hwp-")) return "HWP";
      if (cmdId.startsWith("sheet-")) return "SHEET";
      if (cmdId.startsWith("slide-")) return "SLIDE";
      if (cmdId.startsWith("pdf-")) return "PDF";
      if (cmdId.startsWith("document-") || cmdId.startsWith("doc-")) return "DOC";
      return getActiveModule();
    }

    function evaluateSingleDocExpr(doc, seedDefaults) {
      const sheetName = resolveDefaultSheetFromDocJs(doc);
      const rawExpr = doc.expr !== undefined && doc.expr !== null ? doc.expr : doc.query;
      if (rawExpr !== undefined && rawExpr !== null && typeof rawExpr !== "string") {
        throw new Error("InvalidArgument: JSON 'expr' 필드는 문자열이어야 합니다.");
      }
      const defaultQuery = seedDefaults ? "=Sheet2!A1+50" : "A1";
      const chosenQuery = rawExpr === undefined || rawExpr === null ? defaultQuery : rawExpr;
      const q = String(chosenQuery).trim() || "A1";
      const workbook = buildLocalWorkbookFromJson(doc, sheetName, seedDefaults);
      const evalSheetUpper = resolveCanonicalSheetUpper(workbook.knownSheets, sheetName);
      if (!evalSheetUpper) {
        return { sheetName, expr: q, value: "#REF!" };
      }
      if (isFormulaStringJs(q)) {
        return {
          sheetName,
          expr: q,
          value: String(
            evaluateFormulaExpression(q, workbook, evalSheetUpper, getActiveSheetCellRef() || "A1")
          ),
        };
      }
      return {
        sheetName,
        expr: q,
        value: formatFormulaValue(readWorkbookTypedCell(workbook, evalSheetUpper, q, 0)),
      };
    }

    function evaluateLocalWorkbookFormulaRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const res = evaluateSingleDocExpr(doc, true);
      return { sheet: res.sheetName, formula: res.expr, value: res.value };
    }

    function evaluateLocalSheetRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const defaultSheet = resolveDefaultSheetFromDocJs(doc);
      if (doc.targets !== undefined) {
        if (!Array.isArray(doc.targets)) {
          throw new Error("InvalidArgument: JSON 'targets' 필드는 배열이어야 합니다.");
        }
        if (doc.targets.length > 0) {
          const workbook = buildLocalWorkbookFromJson(doc, defaultSheet, false);
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
                ? String(evaluateFormulaExpression(expr, workbook, canonicalUpper, "A1"))
                : formatFormulaValue(readWorkbookTypedCell(workbook, canonicalUpper, expr, 0));
            }
            outMap[sh + "!" + expr] = val;
            targetValues.push(val);
            if (idx === 0) {
              firstExpr = expr;
              firstVal = val;
            }
          });
          return { query: firstExpr, value: firstVal, values: outMap, targetValues };
        }
      }
      const single = evaluateSingleDocExpr(doc, false);
      return {
        query: single.expr,
        value: single.value,
        values: { [single.sheetName + "!" + single.expr]: single.value },
        targetValues: [single.value],
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
      const cmdId = String(payload || "").trim();
      const prevSnap = JSON.parse(JSON.stringify(backendState));
      if (backendUndoStack.length >= MAX_BACKEND_HISTORY) {
        backendUndoStack.shift();
      }
      backendUndoStack.push(prevSnap);
      backendRedoStack.length = 0;

      const mod = inferCommandModuleKey(cmdId);
      backendState.activeModule = mod;
      backendState.executedCount += 1;
      const exactFn = RIBBON_EXACT_MUTATORS[cmdId];
      if (exactFn) {
        exactFn(backendState);
      } else {
        const suffixPair = RIBBON_SUFFIX_MUTATORS.find(([sfx]) => cmdId.endsWith(sfx));
        if (suffixPair) suffixPair[1](backendState);
      }
      return {
        commandId: cmdId,
        summary: "명령 실행 완료 (" + cmdId + ")",
        contextualBadge: contextualToolBadges[mod] || "표·개체 도구",
        catalogCount: 140,
        ...JSON.parse(JSON.stringify(backendState)),
        canUndo: backendUndoStack.length > 0,
        canRedo: backendRedoStack.length > 0,
      };
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
        const [modPart, catPart] = String(payload || "DOC|home").split("|");
        const mKey = modPart === "CONV" ? "DOC" : modPart;
        const modMap = ribbonCategoryTools[mKey] || ribbonCategoryTools.DOC;
        return { commands: modMap[catPart] || modMap.home };
      },
      convert_document_format: (payload) => {
        const raw = String(payload || "docx->hwpx|문서|본문");
        const firstPipe = raw.indexOf("|");
        const firstSeg = firstPipe >= 0 ? raw.slice(0, firstPipe) : raw;
        const restSeg = firstPipe >= 0 ? raw.slice(firstPipe + 1) : "통합_변환_문서|본문";
        const secondPipe = restSeg.indexOf("|");
        const [src, dst] = (firstSeg || "docx->hwpx").split("->");
        const title =
          (secondPipe >= 0 ? restSeg.slice(0, secondPipe) : restSeg) || "통합_변환_문서";
        const body = (secondPipe >= 0 ? restSeg.slice(secondPipe + 1) : "") || "본문 텍스트";
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
          specification: moduleSpecs[(dst || "").toUpperCase()] || "표준 문서 포맷",
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
        value: evaluateSingleDocExpr(parseRequiredRpcJsonObject(payload), false).value,
      }),
      audit_security: (payload) => localAuditSecurity(payload),
      switch_ribbon_style: (payload) => {
        const isHangul = String(payload).includes("hangul");
        backendState.ribbonStyle = isHangul ? "hangul" : "standard";
        return {
          style: isHangul ? "한글(HWP) 7탭 모드" : "표준 오피스 7탭 모드",
          categories: isHangul ? ribbonStyleLabels.hangul : ribbonStyleLabels.standard,
        };
      },
      execute_ribbon_command: (payload) => localExecuteRibbonCommand(payload),
      undo_ribbon_command: () => {
        if (backendUndoStack.length > 0) {
          const cur = JSON.parse(JSON.stringify(backendState));
          if (backendRedoStack.length >= MAX_BACKEND_HISTORY) {
            backendRedoStack.shift();
          }
          backendRedoStack.push(cur);
          const prev = backendUndoStack.pop();
          Object.assign(backendState, prev);
        }
        return {
          commandId: "undo",
          summary: "실행 취소 완료",
          catalogCount: 140,
          ...JSON.parse(JSON.stringify(backendState)),
          canUndo: backendUndoStack.length > 0,
          canRedo: backendRedoStack.length > 0,
        };
      },
      redo_ribbon_command: () => {
        if (backendRedoStack.length > 0) {
          const cur = JSON.parse(JSON.stringify(backendState));
          if (backendUndoStack.length >= MAX_BACKEND_HISTORY) {
            backendUndoStack.shift();
          }
          backendUndoStack.push(cur);
          const next = backendRedoStack.pop();
          Object.assign(backendState, next);
        }
        return {
          commandId: "redo",
          summary: "다시 실행 완료",
          catalogCount: 140,
          ...JSON.parse(JSON.stringify(backendState)),
          canUndo: backendUndoStack.length > 0,
          canRedo: backendRedoStack.length > 0,
        };
      },
    };
  }

  host.__EASYOFFICE_SHEET_UTILS__ = {
    parseCellRefParts,
    colIndexToLetters,
    normalizeFormattedNumericCell,
    quoteFormulaSheetName,
    isFormulaStringJs,
    localScrubPii,
  };
  host.__EASYOFFICE_CREATE_OFFLINE_RPC__ = createOfflineRpcHandlers;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
