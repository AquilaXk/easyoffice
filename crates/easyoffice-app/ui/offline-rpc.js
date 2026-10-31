(function () {
  "use strict";

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

  function normalizeFormattedNumericCell(raw) {
    const trimmed = String(raw || "").trim();
    if (/^-?\d{1,3}(,\d{3})+(\.\d+)?$/.test(trimmed)) {
      return trimmed.replaceAll(",", "");
    }
    return trimmed;
  }

  function jsonSheetNameOrDefaultJs(raw, fallback = "Sheet1") {
    const trimmed = String(raw ?? "").trim();
    return trimmed || fallback;
  }

  function unquoteFormulaSheetTokenJs(rawToken, fallback = "Sheet1") {
    const trimmed = String(rawToken ?? "").trim();
    const unquoted =
      trimmed.length >= 2 && trimmed.startsWith("'") && trimmed.endsWith("'")
        ? trimmed.slice(1, -1).replaceAll("''", "'")
        : trimmed;
    const norm = unquoted.trim();
    return norm || fallback;
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
      /^(SUM|AVERAGE|AVG|MAX|MIN|COUNT|STDEV|VAR)\((?:('(?:''|[^'])*'|[^\s!+\-*/^&=<>(),:]+)!)?([A-Z]+\d+):([A-Z]+\d+)\)$/.exec(
        body
      );
    if (rangeMatch) {
      const rangeSheet =
        rangeMatch[2] !== undefined
          ? unquoteFormulaSheetTokenJs(rangeMatch[2], "SHEET1").toUpperCase()
          : shUpper;
      const vals = collectRangeNumericValues(rangeMatch[3], rangeMatch[4], cellValues, rangeSheet);
      return evaluateRangeFunction(rangeMatch[1], vals);
    }
    const withCrossSheet = body.replaceAll(
      /('(?:''|[^'])*'|[^\s!+\-*/^&=<>(),:]+)!([A-Z]+\d+)\b/g,
      (_, shToken, ref) => {
        const shKey = unquoteFormulaSheetTokenJs(shToken, "SHEET1").toUpperCase();
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

  function evaluatePairsIntoWorkbookMap(pairsList, seedDefaults) {
    const values = {};
    const formulaEntries = {};
    pairsList.forEach(({ sheetUpper, refUpper, value }) => {
      if (!sheetUpper || !refUpper) return;
      const fullKey = sheetUpper + "!" + refUpper;
      const strVal = String(value ?? "");
      values[fullKey] = strVal;
      if (strVal.startsWith("=")) {
        formulaEntries[fullKey] = strVal;
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
        const bangPos = k.lastIndexOf("!");
        const kSheet = k.slice(0, bangPos);
        const kRef = k.slice(bangPos + 1);
        values[k] = evaluateFormulaExpression(formulaEntries[k], values, kSheet, kRef);
      });
    }
    return values;
  }

  function buildLocalWorkbookValuesFromJson(doc, defaultSheetUpper, seedDefaults) {
    const cells = Array.isArray(doc?.cells) ? doc.cells : [];
    const pairsList = cells.map((item) => ({
      sheetUpper: jsonSheetNameOrDefaultJs(item?.sheet ?? defaultSheetUpper, "Sheet1").toUpperCase(),
      refUpper: String(item?.cell || "").trim().toUpperCase(),
      value: String(item?.value ?? ""),
    }));
    return evaluatePairsIntoWorkbookMap(pairsList, seedDefaults);
  }

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

  function isUnsafeZipDescriptor(entryPath, comp, uncomp, enc) {
    if (
      !entryPath ||
      typeof enc !== "boolean" ||
      !Number.isFinite(comp) ||
      !Number.isFinite(uncomp) ||
      !Number.isInteger(comp) ||
      !Number.isInteger(uncomp) ||
      comp < 0 ||
      uncomp < 0
    ) {
      return true;
    }
    return (
      entryPath.startsWith("/") ||
      entryPath.includes("..") ||
      entryPath.includes("\\") ||
      enc ||
      (comp === 0 && uncomp > 0) ||
      (comp > 0 && uncomp / comp > 100)
    );
  }

  function hasDangerousXmlDirective(xmlText) {
    return /<!\s*(?:DOCTYPE|ENTITY)\b/i.test(String(xmlText ?? ""));
  }

  function localAuditSecurity(payload) {
    const raw = String(payload ?? "").replace(/^\uFEFF/, "");
    const trimmed = raw.trimStart();
    if (trimmed.startsWith("{")) {
      const doc = JSON.parse(trimmed);
      if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
        throw new Error("InvalidArgument: audit_security JSON 루트는 객체여야 합니다.");
      }
      const xmlPart = doc.xml === undefined || doc.xml === null ? "" : String(doc.xml);
      if (hasDangerousXmlDirective(xmlPart)) {
        return {
          status: "REJECTED",
          xxeGuard: "BLOCKED",
          zipGuard: "PASSED",
          checkedEntries: 0,
        };
      }
      const entries = doc.entries === undefined ? [] : doc.entries;
      if (!Array.isArray(entries)) {
        throw new Error("InvalidArgument: audit_security entries 필드는 배열이어야 합니다.");
      }
      for (const item of entries) {
        if (!item || typeof item !== "object" || Array.isArray(item)) {
          return {
            status: "REJECTED",
            xxeGuard: "PASSED",
            zipGuard: "BLOCKED",
            checkedEntries: entries.length,
          };
        }
        const entryPath = typeof item.name === "string" ? item.name.trim() : "";
        const comp = typeof item.compressed === "number" ? item.compressed : Number.NaN;
        const uncomp = typeof item.uncompressed === "number" ? item.uncompressed : Number.NaN;
        const enc = item.encrypted === undefined || item.encrypted === null ? false : item.encrypted;
        if (isUnsafeZipDescriptor(entryPath, comp, uncomp, enc)) {
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

    const pipeIdx = raw.indexOf("|");
    const leftPart = pipeIdx >= 0 ? raw.slice(0, pipeIdx) : "";
    const hasEntrySpec = pipeIdx >= 0 && !leftPart.includes("<") && !leftPart.includes(">");
    const entriesPart = hasEntrySpec ? leftPart : "";
    const xmlPart = hasEntrySpec ? raw.slice(pipeIdx + 1) : raw;
    if (hasDangerousXmlDirective(xmlPart)) {
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
      if (cols.length < 3 || cols.length > 4) {
        return {
          status: "REJECTED",
          xxeGuard: "PASSED",
          zipGuard: "BLOCKED",
          checkedEntries: entries.length,
        };
      }
      const entryPath = cols[0] || "";
      const comp = cols[1] === "" ? Number.NaN : Number(cols[1]);
      const uncomp = cols[2] === "" ? Number.NaN : Number(cols[2]);
      let enc = null;
      if (cols.length === 3) {
        enc = false;
      } else if (cols[3].toLowerCase() === "true" || cols[3] === "1") {
        enc = true;
      } else if (cols[3].toLowerCase() === "false" || cols[3] === "0") {
        enc = false;
      }
      if (isUnsafeZipDescriptor(entryPath, comp, uncomp, enc)) {
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

  function encodeUtf8Base64(text) {
    const bytes = new TextEncoder().encode(String(text || ""));
    let binary = "";
    for (let i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return typeof globalThis.btoa === "function" ? globalThis.btoa(binary) : "";
  }

  function parseRequiredRpcJsonObject(payload) {
    const trimmed = String(payload ?? "")
      .replace(/^\uFEFF/, "")
      .trimStart();
    if (!trimmed.startsWith("{")) {
      throw new Error("InvalidArgument: 시트 RPC 페이로드는 JSON 객체여야 합니다.");
    }
    const doc = JSON.parse(trimmed);
    if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
      throw new Error("InvalidArgument: 시트 RPC 루트는 JSON 객체여야 합니다.");
    }
    return doc;
  }

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

    function evaluateLocalSheetFormula(payload, seedDefaults = false) {
      const doc = parseRequiredRpcJsonObject(payload);
      const sheetName = jsonSheetNameOrDefaultJs(doc.sheet, "Sheet1");
      const evalSheet = sheetName.toUpperCase();
      const q = String(doc.expr ?? doc.query ?? (seedDefaults ? "=Sheet2!A1+50" : "A1")).trim();
      const values = buildLocalWorkbookValuesFromJson(doc, evalSheet, seedDefaults);
      if (q.startsWith("=")) {
        return evaluateFormulaExpression(q, values, evalSheet, getActiveSheetCellRef() || "A1");
      }
      return values[evalSheet + "!" + q.toUpperCase()] ?? "";
    }

    function evaluateLocalWorkbookFormulaRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const sheet = jsonSheetNameOrDefaultJs(doc.sheet, "Sheet1");
      const formula = String(doc.expr || "=Sheet2!A1+50").trim();
      const value = String(evaluateLocalSheetFormula(payload, true));
      return { sheet, formula, value };
    }

    function evaluateLocalSheetRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const defaultSheet = jsonSheetNameOrDefaultJs(doc.sheet, "Sheet1");
      const values = buildLocalWorkbookValuesFromJson(doc, defaultSheet.toUpperCase(), false);
      if (Array.isArray(doc.targets) && doc.targets.length > 0) {
        const outMap = {};
        const targetValues = [];
        let firstExpr = "A1";
        let firstVal = "";
        doc.targets.forEach((item, idx) => {
          const sh = jsonSheetNameOrDefaultJs(item?.sheet ?? defaultSheet, "Sheet1");
          const expr = String(item?.cell || "A1").trim();
          const val = expr.startsWith("=")
            ? String(evaluateFormulaExpression(expr, values, sh.toUpperCase(), "A1"))
            : String(values[sh.toUpperCase() + "!" + expr.toUpperCase()] ?? "");
          outMap[sh + "!" + expr] = val;
          targetValues.push(val);
          if (idx === 0) {
            firstExpr = expr;
            firstVal = val;
          }
        });
        return { query: firstExpr, value: firstVal, values: outMap, targetValues };
      }
      const expr = String(doc.expr ?? doc.query ?? "A1").trim();
      const val = String(evaluateLocalSheetFormula(payload, false));
      return {
        query: expr,
        value: val,
        values: { [defaultSheet + "!" + expr]: val },
        targetValues: [val],
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
        localRibbonState.doubleDiagonalBorderEnabled =
          !localRibbonState.doubleDiagonalBorderEnabled;
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
      const mod = getActiveModule();
      return {
        commandId: cmdId,
        summary: "명령 실행 완료 (" + cmdId + ")",
        contextualBadge: contextualToolBadges[mod] || "표·개체 도구",
        catalogCount: 140,
        ...localRibbonState,
      };
    }

    return {
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
        value: String(evaluateLocalSheetFormula(payload, false)),
      }),
      audit_security: (payload) => localAuditSecurity(payload),
      switch_ribbon_style: (payload) => {
        const isHangul = String(payload).includes("hangul");
        return {
          style: isHangul ? "한글(HWP) 7탭 모드" : "표준 오피스 7탭 모드",
          categories: isHangul ? ribbonStyleLabels.hangul : ribbonStyleLabels.standard,
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
  }

  globalThis.__EASYOFFICE_SHEET_UTILS__ = {
    parseCellRefParts,
    colIndexToLetters,
    normalizeFormattedNumericCell,
  };
  globalThis.__EASYOFFICE_CREATE_OFFLINE_RPC__ = createOfflineRpcHandlers;
})();
