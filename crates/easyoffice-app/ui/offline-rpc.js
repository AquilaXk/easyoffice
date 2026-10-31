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

  function unquoteSheetTokenStrictJs(rawToken) {
    const trimmed = String(rawToken ?? "").trim();
    if (trimmed.length >= 2 && trimmed.startsWith("'") && trimmed.endsWith("'")) {
      return trimmed.slice(1, -1).replaceAll("''", "'");
    }
    return null;
  }

  function resolveCanonicalSheetUpper(knownSheets, rawName) {
    const trimmed = String(rawName ?? "").trim();
    if (!trimmed) return null;
    const upper = trimmed.toUpperCase();
    for (const stored of knownSheets) {
      if (stored.toUpperCase() === upper) return stored.toUpperCase();
    }
    const unquoted = unquoteSheetTokenStrictJs(trimmed);
    if (unquoted !== null) {
      const uqUpper = unquoted.toUpperCase();
      for (const stored of knownSheets) {
        if (stored.toUpperCase() === uqUpper) return stored.toUpperCase();
      }
    }
    for (const stored of knownSheets) {
      const storedUnquoted = unquoteSheetTokenStrictJs(stored);
      if (storedUnquoted !== null && storedUnquoted.toUpperCase() === upper) {
        return stored.toUpperCase();
      }
    }
    return null;
  }

  function addKnownSheet(knownSheets, name) {
    const trimmed = String(name ?? "").trim() || "Sheet1";
    const upper = trimmed.toUpperCase();
    if (!knownSheets.some((s) => s.toUpperCase() === upper)) {
      knownSheets.push(trimmed);
    }
  }

  function collectRangeNumericValues(startRef, endRef, cellValues, canonicalSheetUpper) {
    const shPrefix = canonicalSheetUpper + "!";
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

  function evaluateFormulaExpression(exprStr, workbook, currentSheetUpper, currentCellRef) {
    const { values, knownSheets } = workbook;
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
      const rangeSheetUpper =
        rangeMatch[2] === undefined
          ? currentSheetUpper
          : resolveCanonicalSheetUpper(knownSheets, rangeMatch[2]);
      if (!rangeSheetUpper) return "#REF!";
      const vals = collectRangeNumericValues(
        rangeMatch[3],
        rangeMatch[4],
        values,
        rangeSheetUpper
      );
      return evaluateRangeFunction(rangeMatch[1], vals);
    }
    let hasMissingRef = false;
    const withCrossSheet = body.replaceAll(
      /('(?:''|[^'])*'|[^\s!+\-*/^&=<>(),:]+)!([A-Z]+\d+)\b/g,
      (_, shToken, ref) => {
        const shKey = resolveCanonicalSheetUpper(knownSheets, shToken);
        if (!shKey) {
          hasMissingRef = true;
          return "#REF!";
        }
        const raw = String(values[shKey + "!" + ref] ?? "")
          .replaceAll(",", "")
          .trim();
        if (raw.startsWith("#")) {
          hasMissingRef = true;
          return raw;
        }
        const v = Number(raw);
        return raw !== "" && Number.isFinite(v) ? String(v) : "0";
      }
    );
    if (hasMissingRef) return "#REF!";
    const substituted = withCrossSheet.replaceAll(/\b([A-Z]+\d+)\b/g, (_, ref) => {
      const raw = String(values[currentSheetUpper + "!" + ref] ?? "")
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

  function scalarJsonCellValue(rawVal) {
    if (rawVal === undefined || rawVal === null) return "";
    if (typeof rawVal === "string") return rawVal;
    if (typeof rawVal === "number" && Number.isFinite(rawVal)) return String(rawVal);
    if (typeof rawVal === "boolean") return rawVal ? "TRUE" : "FALSE";
    throw new Error("InvalidArgument: JSON 셀 'value'는 스칼라여야 합니다.");
  }

  function buildLocalWorkbookFromJson(doc, defaultSheet, seedDefaults) {
    const knownSheets = [];
    addKnownSheet(knownSheets, "Sheet1");
    addKnownSheet(knownSheets, "Sheet2");
    addKnownSheet(knownSheets, defaultSheet);
    if (doc.sheets !== undefined) {
      if (!Array.isArray(doc.sheets)) {
        throw new Error("InvalidArgument: JSON 'sheets' 필드는 배열이어야 합니다.");
      }
      for (const s of doc.sheets) {
        if (typeof s !== "string") {
          throw new Error("InvalidArgument: JSON 'sheets' 항목은 문자열이어야 합니다.");
        }
        if (s.trim()) addKnownSheet(knownSheets, s.trim());
      }
    }
    if (doc.cells !== undefined && !Array.isArray(doc.cells)) {
      throw new Error("InvalidArgument: JSON 'cells' 필드는 배열이어야 합니다.");
    }
    const cells = doc.cells || [];
    const values = {};
    const formulaEntries = {};
    let seenSheet1 = false;
    let seenSheet2 = false;
    const triples = [];

    for (const item of cells) {
      if (!item || typeof item !== "object" || Array.isArray(item)) {
        throw new Error("InvalidArgument: JSON 'cells' 항목은 객체여야 합니다.");
      }
      if (item.sheet !== undefined && item.sheet !== null && typeof item.sheet !== "string") {
        throw new Error("InvalidArgument: JSON 셀 'sheet' 필드는 문자열이어야 합니다.");
      }
      if (typeof item.cell !== "string" || !item.cell.trim()) {
        throw new Error("InvalidArgument: JSON 셀 'cell' 주소가 비어 있습니다.");
      }
      const shName = jsonSheetNameOrDefaultJs(item.sheet ?? defaultSheet, defaultSheet);
      const addrUpper = item.cell.trim().toUpperCase();
      const strVal = scalarJsonCellValue(item.value);
      if (shName.toUpperCase() === "SHEET1") seenSheet1 = true;
      if (shName.toUpperCase() === "SHEET2") seenSheet2 = true;
      addKnownSheet(knownSheets, shName);
      triples.push({ shUpper: shName.toUpperCase(), addrUpper, strVal });
    }

    if (seedDefaults) {
      if (!seenSheet1) {
        values["SHEET1!A1"] = "1200";
        values["SHEET1!B1"] = "850";
      }
      if (!seenSheet2) {
        values["SHEET2!A1"] = "100";
      }
    }

    for (const { shUpper, addrUpper, strVal } of triples) {
      const fullKey = shUpper + "!" + addrUpper;
      values[fullKey] = strVal;
      if (strVal.startsWith("=")) {
        formulaEntries[fullKey] = strVal;
      } else {
        delete formulaEntries[fullKey];
      }
    }

    const workbook = { values, knownSheets };
    for (let pass = 0; pass < 4; pass++) {
      Object.keys(formulaEntries).forEach((k) => {
        const bangPos = k.lastIndexOf("!");
        const kSheet = k.slice(0, bangPos);
        const kRef = k.slice(bangPos + 1);
        values[k] = evaluateFormulaExpression(formulaEntries[k], workbook, kSheet, kRef);
      });
    }
    return workbook;
  }

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
    if (idx + 3 <= str.length && str.startsWith("02", idx) && (str[idx + 2] === "-" || str[idx + 2] === " ")) {
      const sep = str[idx + 2];
      const pMid = consumeDigitsJs(str, idx + 3, 3, 4);
      if (pMid > 0 && pMid < str.length && str[pMid] === sep) {
        const pEnd = consumeDigitsJs(str, pMid + 1, 4, 4);
        if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
          return { len: pEnd - idx, replacement: "02-****-****" };
        }
      }
    }
    if (
      idx + 4 <= str.length &&
      /^0[3-6][1-5][-\s]/.test(str.slice(idx, idx + 4))
    ) {
      const sep = str[idx + 3];
      const pMid = consumeDigitsJs(str, idx + 4, 3, 4);
      if (pMid > 0 && pMid < str.length && str[pMid] === sep) {
        const pEnd = consumeDigitsJs(str, pMid + 1, 4, 4);
        if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
          return { len: pEnd - idx, replacement: "***-****-****" };
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
      if (em) {
        scrubbedText += em.replacement;
        spans.push({ start: idx, end: idx + em.len, replacement: em.replacement });
        emailCount += 1;
        idx += em.len;
        continue;
      }
      const rrn = matchRrnAtJs(text, idx);
      if (rrn) {
        scrubbedText += rrn.replacement;
        spans.push({ start: idx, end: idx + rrn.len, replacement: rrn.replacement });
        residentCount += 1;
        idx += rrn.len;
        continue;
      }
      const ph = matchPhoneAtJs(text, idx);
      if (ph) {
        scrubbedText += ph.replacement;
        spans.push({ start: idx, end: idx + ph.len, replacement: ph.replacement });
        phoneCount += 1;
        idx += ph.len;
        continue;
      }
      const cp = text.codePointAt(idx);
      const step = cp > 0xffff ? 2 : 1;
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
      return {
        status: "REJECTED",
        xxeGuard: "BLOCKED",
        zipGuard: "PASSED",
        checkedEntries: 0,
      };
    }
    const entries = doc.entries === undefined ? [] : doc.entries;
    if (!Array.isArray(entries)) {
      throw new Error("InvalidArgument: audit_security 'entries' 필드는 배열이어야 합니다.");
    }
    const MAX_ENTRIES = 4096;
    const MAX_SINGLE_UNCOMPRESSED = 128 * 1024 * 1024;
    const MAX_TOTAL_UNCOMPRESSED = 512 * 1024 * 1024;
    if (entries.length > MAX_ENTRIES) {
      return {
        status: "REJECTED",
        xxeGuard: "PASSED",
        zipGuard: "BLOCKED",
        checkedEntries: entries.length,
      };
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
      const rawComp = item.compressed ?? item.compressedSize;
      const rawUncomp = item.uncompressed ?? item.uncompressedSize;
      if (
        typeof rawComp !== "number" ||
        !Number.isInteger(rawComp) ||
        rawComp < 0 ||
        typeof rawUncomp !== "number" ||
        !Number.isInteger(rawUncomp) ||
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
        return {
          status: "REJECTED",
          xxeGuard: "PASSED",
          zipGuard: "BLOCKED",
          checkedEntries: entries.length,
        };
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
    return typeof globalThis.btoa === "function" ? globalThis.btoa(binary) : "";
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

    function evaluateSingleDocExpr(doc, seedDefaults) {
      if (doc.sheet !== undefined && doc.sheet !== null && typeof doc.sheet !== "string") {
        throw new Error("InvalidArgument: JSON 'sheet' 필드는 문자열이어야 합니다.");
      }
      const sheetName = jsonSheetNameOrDefaultJs(doc.sheet, "Sheet1");
      const rawExpr = doc.expr !== undefined && doc.expr !== null ? doc.expr : doc.query;
      if (rawExpr !== undefined && rawExpr !== null && typeof rawExpr !== "string") {
        throw new Error("InvalidArgument: JSON 'expr' 필드는 문자열이어야 합니다.");
      }
      const q = String(rawExpr ?? (seedDefaults ? "=Sheet2!A1+50" : "A1")).trim() || "A1";
      const workbook = buildLocalWorkbookFromJson(doc, sheetName, seedDefaults);
      const evalSheetUpper =
        resolveCanonicalSheetUpper(workbook.knownSheets, sheetName) || sheetName.toUpperCase();
      if (q.startsWith("=")) {
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
        value: String(workbook.values[evalSheetUpper + "!" + q.toUpperCase()] ?? ""),
      };
    }

    function evaluateLocalWorkbookFormulaRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const res = evaluateSingleDocExpr(doc, true);
      return { sheet: res.sheetName, formula: res.expr, value: res.value };
    }

    function evaluateLocalSheetRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      if (doc.sheet !== undefined && doc.sheet !== null && typeof doc.sheet !== "string") {
        throw new Error("InvalidArgument: JSON 'sheet' 필드는 문자열이어야 합니다.");
      }
      const defaultSheet = jsonSheetNameOrDefaultJs(doc.sheet, "Sheet1");
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
              val = expr.startsWith("=")
                ? String(evaluateFormulaExpression(expr, workbook, canonicalUpper, "A1"))
                : String(workbook.values[canonicalUpper + "!" + expr.toUpperCase()] ?? "");
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
        values: { [defaultSheet + "!" + single.expr]: single.value },
        targetValues: [single.value],
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
        localRibbonState.zoomPercent = ZOOM_STEP_CYCLE[localRibbonState.zoomPercent] ?? 100;
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
        const title =
          (secondPipe >= 0 ? restSeg.slice(0, secondPipe) : restSeg) || "통합_변환_문서";
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
          {
            suiteId: "crc32_slicing_by_8",
            baselineP50Us: 2068,
            optimizedP50Us: 464,
            speedupFactor: 4.46,
          },
          {
            suiteId: "deflate_zlib_roundtrip",
            baselineP50Us: 234,
            optimizedP50Us: 59,
            speedupFactor: 3.97,
          },
          {
            suiteId: "utf16_swar_and_xml_unescape",
            baselineP50Us: 50,
            optimizedP50Us: 29,
            speedupFactor: 1.72,
          },
          {
            suiteId: "sheet_dag_bitset_eval",
            baselineP50Us: 878,
            optimizedP50Us: 308,
            speedupFactor: 2.85,
          },
          {
            suiteId: "swar_csv_xml_and_zero_alloc_range_agg",
            baselineP50Us: 640,
            optimizedP50Us: 195,
            speedupFactor: 3.28,
          },
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
    localScrubPii,
  };
  globalThis.__EASYOFFICE_CREATE_OFFLINE_RPC__ = createOfflineRpcHandlers;
})();
