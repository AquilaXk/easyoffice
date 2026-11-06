(function (host) {
  "use strict";

  const MAX_ROWS = 1048576;
  const MAX_COLS = 16384;
  const UNICODE_ALNUM_RE = /^[\p{L}\p{N}]$/u;
  const UNICODE_ALPHA_RE = /^\p{L}$/u;
  const SAFE_SERIALIZED_ATTRS = [
    "id",
    "class",
    "contenteditable",
    "href",
    "target",
    "rel",
    "colspan",
    "rowspan",
    "title",
    "role",
    "aria-label",
  ];

  function asciiUpperJs(str) {
    return String(str ?? "").replaceAll(/[a-z]/g, (c) =>
      String.fromCodePoint(c.codePointAt(0) - 32)
    );
  }

  function quoteFormulaSheetName(sheetName) {
    const raw = String(sheetName ?? "");
    if (!raw) return "''";
    const up = asciiUpperJs(raw);
    const hasEdgeSpace = raw !== raw.trim();
    const hasNonAlphaNum = !/^[A-Za-z0-9_]+$/.test(raw);
    const startsWithDigit = /^[0-9]/.test(raw);
    const looksLikeCell = /^[A-Za-z]{1,3}[0-9]+$/.test(raw);
    const isReserved = up === "TRUE" || up === "FALSE";
    if (hasEdgeSpace || hasNonAlphaNum || startsWithDigit || looksLikeCell || isReserved) {
      return "'" + raw.replaceAll("'", "''") + "'";
    }
    return raw;
  }

  function parseStrictA1Coord(rawToken) {
    const str = String(rawToken ?? "").trim();
    const candidateMatch = /^\$?([A-Za-z]{1,3})\$?([0-9]+)$/.exec(str);
    if (!candidateMatch) {
      return { ok: false, errorCode: "#NAME?" };
    }
    const letters = asciiUpperJs(candidateMatch[1]);
    const digits = candidateMatch[2];
    if (digits[0] === "0") {
      return { ok: false, errorCode: "#REF!" };
    }
    let col1 = 0;
    for (let i = 0; i < letters.length; i += 1) {
      col1 = col1 * 26 + (letters.codePointAt(i) - 64);
    }
    const row1 = Number(digits);
    if (!Number.isSafeInteger(row1) || row1 < 1 || row1 > MAX_ROWS || col1 < 1 || col1 > MAX_COLS) {
      return { ok: false, errorCode: "#REF!" };
    }
    return {
      ok: true,
      col: col1 - 1,
      row: row1,
      normRef: letters + String(row1),
    };
  }

  function parseCellRefParts(ref) {
    const parsed = parseStrictA1Coord(String(ref ?? ""));
    if (!parsed.ok) return { col: 0, row: 1 };
    return { col: parsed.col, row: parsed.row };
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

  function colLettersToIndex(lettersRaw) {
    const letters = asciiUpperJs(String(lettersRaw ?? "").trim());
    if (!/^[A-Z]{1,3}$/.test(letters)) return 0;
    let col1 = 0;
    for (let i = 0; i < letters.length; i += 1) {
      col1 = col1 * 26 + (letters.codePointAt(i) - 64);
    }
    return Math.max(0, col1 - 1);
  }

  function normalizeFormattedNumericCell(raw) {
    const trimmed = String(raw || "").trim();
    if (/^-?\d{1,3}(,\d{3})+(\.\d+)?$/.test(trimmed)) {
      return trimmed.replaceAll(",", "");
    }
    return trimmed;
  }

  function isFormulaStringJs(rawSrc) {
    const trimmed = String(rawSrc ?? "").trim();
    const up = asciiUpperJs(trimmed.slice(0, 6));
    let body = trimmed;
    if (up.startsWith("OF:")) body = trimmed.slice(3).trimStart();
    else if (up.startsWith("OOOC:")) body = trimmed.slice(5).trimStart();
    else if (up.startsWith("MSOXL:")) body = trimmed.slice(6).trimStart();
    return body.startsWith("=");
  }

  function isAsciiDigitCode(c) {
    return c >= 48 && c <= 57;
  }

  function isUnicodeAlnum(ch) {
    return UNICODE_ALNUM_RE.test(ch);
  }

  function isUnicodeAlpha(ch) {
    return UNICODE_ALPHA_RE.test(ch);
  }

  function charAlphaScript(ch) {
    const code = ch.codePointAt(0);
    if ((code >= 65 && code <= 90) || (code >= 97 && code <= 122)) return 1;
    if (code > 127 && isUnicodeAlpha(ch)) return 2;
    return 0;
  }

  function isEmailLocalNonDot(ch) {
    return isUnicodeAlnum(ch) || ch === "_" || ch === "%" || ch === "+" || ch === "-";
  }

  function readCodePointAt(str, pos) {
    const cp = str.codePointAt(pos);
    const step = cp > 0xffff ? 2 : 1;
    return { ch: str.slice(pos, pos + step), step };
  }

  function readPrevCodePoint(str, pos) {
    if (pos <= 0) return null;
    if (pos >= 2) {
      const hi = str.charCodeAt(pos - 2);
      const lo = str.charCodeAt(pos - 1);
      if (hi >= 0xd800 && hi <= 0xdbff && lo >= 0xdc00 && lo <= 0xdfff) {
        return str.slice(pos - 2, pos);
      }
    }
    return str[pos - 1];
  }

  function scanDomainAfterAtJs(str, domStart) {
    if (domStart >= str.length) return -1;
    let cursor = domStart;
    const labels = [];
    while (cursor < str.length) {
      const labelStart = cursor;
      let labelChars = 0;
      let allAlpha = true;
      let prevHyphen = false;
      let labelScript = 0;
      while (cursor < str.length) {
        const { ch, step } = readCodePointAt(str, cursor);
        if (ch === "-") {
          if (cursor === labelStart) break;
          allAlpha = false;
          prevHyphen = true;
          cursor += 1;
          labelChars += 1;
          continue;
        }
        if (!isUnicodeAlnum(ch)) break;
        const sc = charAlphaScript(ch);
        if (sc !== 0) {
          if (labelScript !== 0 && labelScript !== sc) break;
          labelScript = sc;
        } else {
          allAlpha = false;
        }
        prevHyphen = false;
        cursor += step;
        labelChars += 1;
      }
      if (cursor === labelStart || prevHyphen) break;
      labels.push({ endPos: cursor, allAlpha, labelChars });
      if (cursor < str.length && str[cursor] === ".") {
        const afterDot = cursor + 1;
        if (afterDot < str.length) {
          const nextCp = readCodePointAt(str, afterDot);
          if (isUnicodeAlnum(nextCp.ch)) {
            cursor = afterDot;
            continue;
          }
        }
      }
      break;
    }
    while (labels.length >= 2) {
      const last = labels[labels.length - 1];
      if (last.allAlpha && last.labelChars >= 2) {
        return last.endPos;
      }
      labels.pop();
    }
    return -1;
  }

  function scanEmailAtJs(str, idx) {
    if (idx >= str.length) return { matchedLen: 0, skipUntil: str.length };
    const first = readCodePointAt(str, idx);
    const stepOne = idx + first.step;
    if (!isEmailLocalNonDot(first.ch)) {
      return { matchedLen: 0, skipUntil: stepOne };
    }
    const prevCh = readPrevCodePoint(str, idx);
    if (prevCh !== null) {
      if (prevCh === "." || prevCh === "_" || prevCh === "%" || prevCh === "+" || prevCh === "-") {
        return { matchedLen: 0, skipUntil: stepOne };
      }
      if (isUnicodeAlnum(prevCh)) {
        const prevScript = charAlphaScript(prevCh);
        const curScript = charAlphaScript(first.ch);
        if (prevScript === 0 || curScript === 0 || prevScript === curScript) {
          return { matchedLen: 0, skipUntil: stepOne };
        }
      }
    }

    let p = idx;
    let prevWasDot = false;
    let atomScript = 0;
    while (p < str.length) {
      const { ch, step } = readCodePointAt(str, p);
      if (ch === ".") {
        if (prevWasDot) return { matchedLen: 0, skipUntil: p + 1 };
        prevWasDot = true;
        atomScript = 0;
        p += 1;
        continue;
      }
      if (!isEmailLocalNonDot(ch)) break;
      const sc = charAlphaScript(ch);
      if (sc !== 0) {
        if (atomScript !== 0 && atomScript !== sc) {
          return { matchedLen: 0, skipUntil: p };
        }
        atomScript = sc;
      }
      prevWasDot = false;
      p += step;
    }

    if (p >= str.length || str[p] !== "@") {
      return { matchedLen: 0, skipUntil: Math.max(p, stepOne) };
    }
    if (prevWasDot) {
      return { matchedLen: 0, skipUntil: p + 1 };
    }
    const endDom = scanDomainAfterAtJs(str, p + 1);
    if (endDom < 0) {
      return { matchedLen: 0, skipUntil: p + 1 };
    }
    return { matchedLen: endDom - idx, skipUntil: endDom };
  }

  function isValidRrnDateAndGenderJs(front6, genderChar) {
    if (!/^\d{6}$/.test(front6)) return false;
    const month = Number(front6.slice(2, 4));
    const day = Number(front6.slice(4, 6));
    const gender = Number(genderChar);
    return month >= 1 && month <= 12 && day >= 1 && day <= 31 && gender >= 1 && gender <= 8;
  }

  function matchRrnAtJs(str, idx) {
    if (idx > 0 && isAsciiDigitCode(str.charCodeAt(idx - 1))) return null;
    if (
      idx + 14 <= str.length &&
      /^\d{6}-\d{7}$/.test(str.slice(idx, idx + 14)) &&
      isValidRrnDateAndGenderJs(str.slice(idx, idx + 6), str[idx + 7]) &&
      (idx + 14 === str.length || !isAsciiDigitCode(str.charCodeAt(idx + 14)))
    ) {
      return { len: 14, replacement: "******-*******" };
    }
    if (
      idx + 13 <= str.length &&
      /^\d{13}$/.test(str.slice(idx, idx + 13)) &&
      isValidRrnDateAndGenderJs(str.slice(idx, idx + 6), str[idx + 6]) &&
      (idx + 13 === str.length || !isAsciiDigitCode(str.charCodeAt(idx + 13)))
    ) {
      return { len: 13, replacement: "******-*******" };
    }
    return null;
  }

  function consumeDigitsJs(str, pos, minLen, maxLen) {
    let end = pos;
    while (end < str.length && end - pos < maxLen && isAsciiDigitCode(str.charCodeAt(end))) {
      end += 1;
    }
    return end - pos >= minLen ? end : -1;
  }

  function isPhoneSepChar(ch) {
    return ch === "-" || ch === "." || ch === " ";
  }

  function matchPhoneAtJs(str, idx) {
    if (idx > 0 && (isAsciiDigitCode(str.charCodeAt(idx - 1)) || str[idx - 1] === "+")) {
      return null;
    }
    if (str.startsWith("+82", idx)) {
      let p = idx + 3;
      if (p < str.length && isPhoneSepChar(str[p])) p += 1;
      const pArea = consumeDigitsJs(str, p, 1, 3);
      if (pArea > 0) {
        let p2 = pArea;
        if (p2 < str.length && isPhoneSepChar(str[p2])) p2 += 1;
        const pMid = consumeDigitsJs(str, p2, 3, 4);
        if (pMid > 0) {
          let p3 = pMid;
          if (p3 < str.length && isPhoneSepChar(str[p3])) p3 += 1;
          const pEnd = consumeDigitsJs(str, p3, 4, 4);
          if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
            return { len: pEnd - idx, replacement: "+82-**-****-****" };
          }
        }
      }
    }
    const prefix3 = str.slice(idx, idx + 3);
    if (/^01[016789]$/.test(prefix3)) {
      const mobileMask = prefix3 + "-****-****";
      const p = idx + 3;
      if (p < str.length && isPhoneSepChar(str[p])) {
        const sep = str[p];
        const pMid = consumeDigitsJs(str, p + 1, 3, 4);
        if (pMid > 0 && pMid < str.length && str[pMid] === sep) {
          const pEnd = consumeDigitsJs(str, pMid + 1, 4, 4);
          if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
            return { len: pEnd - idx, replacement: mobileMask };
          }
        }
      } else if (
        idx + 11 <= str.length &&
        /^\d{11}$/.test(str.slice(idx, idx + 11)) &&
        (idx + 11 === str.length || !isAsciiDigitCode(str.charCodeAt(idx + 11)))
      ) {
        return { len: 11, replacement: mobileMask };
      }
    }
    const landlineMatch = /^(02|0[3-6][1-5])([-.\s])/.exec(str.slice(idx, idx + 4));
    if (landlineMatch) {
      const area = landlineMatch[1];
      const sep = landlineMatch[2];
      const pMid = consumeDigitsJs(str, idx + area.length + 1, 3, 4);
      if (pMid > 0 && pMid < str.length && str[pMid] === sep) {
        const pEnd = consumeDigitsJs(str, pMid + 1, 4, 4);
        if (pEnd > 0 && (pEnd === str.length || !isAsciiDigitCode(str.charCodeAt(pEnd)))) {
          const mask = area === "02" ? "02-****-****" : "***-****-****";
          return { len: pEnd - idx, replacement: mask };
        }
      }
    }
    return null;
  }

  // Card / bank account / passport / driver's licence / business registration numbers.
  // Mirrors crates/easyoffice-app/src/pii_identifiers.rs and the Swift regex rules.
  const BANK_ACCOUNT_GROUPS = [
    [3, 2, 6], [3, 3, 6], [3, 6, 3], [3, 6, 5], [4, 3, 6], [4, 2, 7], [6, 2, 6], [3, 4, 4, 2], [3, 6, 2, 3],
  ];
  const isHyphenChar = (ch) => ch === "-";
  const isCardSepChar = (ch) => ch === "-" || ch === " " || ch === "\t" || ch === "\n" || ch === "\r" || ch === "\f";
  const isAsciiUpperChar = (ch) => ch !== undefined && ch >= "A" && ch <= "Z";
  const isAsciiAlnumChar = (ch) =>
    ch !== undefined && (isAsciiUpperChar(ch) || (ch >= "a" && ch <= "z") || (ch >= "0" && ch <= "9"));
  // Card 4-4-4-4, driver's licence 2-2-6-2, business registration 3-2-5, then bank accounts.
  const IDENTIFIER_DIGIT_LAYOUTS = [
    [[4, 4, 4, 4], isCardSepChar],
    [[2, 2, 6, 2], isHyphenChar],
    [[3, 2, 5], isHyphenChar],
    ...BANK_ACCOUNT_GROUPS.map((groups) => [groups, isHyphenChar]),
  ];

  function allDigitsJs(str, from, len) {
    if (from + len > str.length) return false;
    for (let i = from; i < from + len; i += 1) {
      if (!isAsciiDigitCode(str.charCodeAt(i))) return false;
    }
    return true;
  }

  function matchDigitGroupsJs(str, idx, groups, isSep) {
    let p = idx;
    for (let i = 0; i < groups.length; i += 1) {
      if (i > 0) {
        if (p >= str.length || !isSep(str[p])) return 0;
        p += 1;
      }
      if (!allDigitsJs(str, p, groups[i])) return 0;
      p += groups[i];
    }
    if (p < str.length && isAsciiDigitCode(str.charCodeAt(p))) return 0;
    return p - idx;
  }

  function passesLuhnJs(digits) {
    let sum = 0;
    for (let i = 0; i < digits.length; i += 1) {
      let v = digits.charCodeAt(digits.length - 1 - i) - 48;
      if (i % 2 === 1) {
        v *= 2;
        if (v > 9) v -= 9;
      }
      sum += v;
    }
    return sum % 10 === 0;
  }

  function matchContiguousCardJs(str, idx) {
    if (!/[2-69]/.test(str[idx])) return 0;
    let end = idx;
    while (end < str.length && end - idx < 20 && isAsciiDigitCode(str.charCodeAt(end))) end += 1;
    const len = end - idx;
    if (len < 13 || len > 19 || (end < str.length && isAsciiDigitCode(str.charCodeAt(end)))) return 0;
    return passesLuhnJs(str.slice(idx, end)) ? len : 0;
  }

  function matchPassportJs(str, idx) {
    if ((idx > 0 && isAsciiAlnumChar(str[idx - 1])) || idx + 9 > str.length) return 0;
    const isType = "MSRODG".includes(str[idx]);
    const legacy = isType && allDigitsJs(str, idx + 1, 8);
    const newFormat =
      isType && allDigitsJs(str, idx + 1, 3) && isAsciiUpperChar(str[idx + 4]) && allDigitsJs(str, idx + 5, 4);
    const twoLetter = isAsciiUpperChar(str[idx]) && isAsciiUpperChar(str[idx + 1]) && allDigitsJs(str, idx + 2, 7);
    if (!(legacy || newFormat || twoLetter)) return 0;
    if (idx + 9 < str.length && isAsciiAlnumChar(str[idx + 9])) return 0;
    return 9;
  }

  function matchIdentifierAtJs(str, idx) {
    if (isAsciiUpperChar(str[idx])) return matchPassportJs(str, idx);
    if (!isAsciiDigitCode(str.charCodeAt(idx)) || (idx > 0 && isAsciiDigitCode(str.charCodeAt(idx - 1)))) {
      return 0;
    }
    for (const [groups, isSep] of IDENTIFIER_DIGIT_LAYOUTS) {
      const len = matchDigitGroupsJs(str, idx, groups, isSep);
      if (len > 0) return len;
    }
    return matchContiguousCardJs(str, idx);
  }

  function tryParseJsonEnvelope(rawText) {
    if (!rawText.trimStart().startsWith("{")) {
      return null;
    }
    try {
      const parsed = JSON.parse(rawText);
      return parsed && typeof parsed === "object" ? parsed : null;
    } catch (_err) {
      return null;
    }
  }

  function localScrubPii(rawInput, metadataObj) {
    let text = String(rawInput ?? "");
    let metaMap = metadataObj && typeof metadataObj === "object" ? metadataObj : null;
    if (metaMap === null) {
      const parsed = tryParseJsonEnvelope(text);
      if (parsed && typeof parsed.text === "string") {
        text = parsed.text;
        if (parsed.metadata && typeof parsed.metadata === "object") {
          metaMap = parsed.metadata;
        }
      }
    }
    let scrubbedText = "";
    let residentCount = 0;
    let phoneCount = 0;
    let emailCount = 0;
    let identifierCount = 0;
    const spans = [];
    let idx = 0;
    let emailFailedUntil = 0;

    while (idx < text.length) {
      if (idx >= emailFailedUntil) {
        const em = scanEmailAtJs(text, idx);
        if (em.matchedLen > 0) {
          scrubbedText += "***@***";
          spans.push({ start: idx, end: idx + em.matchedLen, replacement: "***@***" });
          emailCount += 1;
          idx += em.matchedLen;
          continue;
        }
        emailFailedUntil = em.skipUntil;
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
      const idLen = matchIdentifierAtJs(text, idx);
      if (idLen > 0) {
        const idMask = text.slice(idx, idx + idLen).replace(/[A-Za-z0-9]/g, "*");
        scrubbedText += idMask;
        spans.push({ start: idx, end: idx + idLen, replacement: idMask });
        identifierCount += 1;
        idx += idLen;
        continue;
      }
      const step = text.codePointAt(idx) > 0xffff ? 2 : 1;
      scrubbedText += text.slice(idx, idx + step);
      idx += step;
    }

    const strippedMetadataFields = [];
    if (metaMap) {
      Object.keys(metaMap).forEach((k) => {
        const val = metaMap[k];
        if (k.trim() && val !== null && val !== undefined && String(val).trim() !== "") {
          strippedMetadataFields.push(k);
        }
      });
    }

    return {
      scrubbedText,
      maskedResidentIds: residentCount,
      maskedPhoneNumbers: phoneCount,
      maskedEmails: emailCount,
      maskedIdentifierNumbers: identifierCount,
      totalMasked: residentCount + phoneCount + emailCount + identifierCount,
      spans,
      strippedMetadataFields,
    };
  }

  function serializeDomNode(node) {
    if (!node) return null;
    if (node.nodeType === 3) {
      return { t: "text", v: String(node.nodeValue ?? node.textContent ?? "") };
    }
    if (node.nodeType !== 1) return null;
    const tag = String(node.tagName || "div").toLowerCase();
    const attrs = {};
    SAFE_SERIALIZED_ATTRS.forEach((name) => {
      if (typeof node.getAttribute === "function") {
        const val = node.getAttribute(name);
        if (val !== null && val !== undefined) attrs[name] = String(val);
      } else if (name === "id" && node.id) {
        attrs.id = String(node.id);
      } else if (name === "class" && node.className) {
        attrs.class = String(node.className);
      }
    });
    if (node.hidden) {
      attrs.hidden = "";
    }
    const style = {};
    const styleProps = [
      "fontWeight",
      "fontStyle",
      "textDecoration",
      "fontSize",
      "color",
      "backgroundColor",
      "borderStyle",
      "columnCount",
      "padding",
      "paddingLeft",
      "lineHeight",
      "textAlign",
      "outline",
      "transform",
    ];
    if (node.style) {
      styleProps.forEach((prop) => {
        const val = node.style[prop];
        if (val) style[prop] = String(val);
      });
    }
    const data = {};
    if (node.dataset && typeof node.dataset === "object") {
      Object.keys(node.dataset).forEach((k) => {
        if (node.dataset[k] !== undefined) data[k] = String(node.dataset[k]);
      });
    }
    const children = [];
    const childList = node.childNodes || [];
    for (let i = 0; i < childList.length; i += 1) {
      const serialized = serializeDomNode(childList[i]);
      if (serialized) children.push(serialized);
    }
    const spec = { t: "el", tag, attrs, data, style, children };
    const cs = Number(node.getAttribute?.("colspan") || node.colSpan || 1);
    if (cs > 1) {
      attrs.colspan = String(cs);
      spec.colSpan = cs;
    }
    const rs = Number(node.getAttribute?.("rowspan") || node.rowSpan || 1);
    if (rs > 1) {
      attrs.rowspan = String(rs);
      spec.rowSpan = rs;
    }
    return spec;
  }

  function serializeDomChildren(container) {
    if (!container || !container.childNodes) return [];
    const out = [];
    for (let i = 0; i < container.childNodes.length; i += 1) {
      const item = serializeDomNode(container.childNodes[i]);
      if (item) out.push(item);
    }
    return out;
  }

  function buildDomNodeFromSpec(spec, doc) {
    if (!spec) return null;
    if (spec.t === "text") {
      return doc.createTextNode(String(spec.v ?? ""));
    }
    if (spec.t !== "el") return null;
    const el = doc.createElement(String(spec.tag || "div"));
    const attrs = spec.attrs || {};
    Object.keys(attrs).forEach((name) => {
      const lower = name.toLowerCase();
      if (lower === "style" || lower.startsWith("on")) return;
      if (lower === "hidden") {
        el.hidden = true;
      } else if (typeof el.setAttribute === "function") {
        el.setAttribute(name, String(attrs[name]));
      } else if (lower === "id") {
        el.id = String(attrs[name]);
      } else if (lower === "class") {
        el.className = String(attrs[name]);
      }
    });
    if (typeof spec.colSpan === "number" && spec.colSpan > 1) {
      el.colSpan = spec.colSpan;
    }
    if (typeof spec.rowSpan === "number" && spec.rowSpan > 1) {
      el.rowSpan = spec.rowSpan;
    }
    if (spec.style && el.style) {
      Object.keys(spec.style).forEach((prop) => {
        if (spec.style[prop]) {
          el.style[prop] = String(spec.style[prop]);
        }
      });
    }
    const data = spec.data || {};
    Object.keys(data).forEach((k) => {
      el.dataset[k] = String(data[k]);
    });
    const kids = Array.isArray(spec.children) ? spec.children : [];
    kids.forEach((childSpec) => {
      const childNode = buildDomNodeFromSpec(childSpec, doc);
      if (childNode) el.appendChild(childNode);
    });
    return el;
  }

  function restoreDomChildren(container, specs, doc) {
    if (!container || !doc) return;
    const built = [];
    (Array.isArray(specs) ? specs : []).forEach((spec) => {
      const node = buildDomNodeFromSpec(spec, doc);
      if (node) built.push(node);
    });
    if (typeof container.replaceChildren === "function") {
      container.replaceChildren(...built);
    } else {
      container.innerHTML = "";
      built.forEach((n) => container.appendChild(n));
    }
  }

  host.__EASYOFFICE_SHEET_UTILS__ = {
    asciiUpperJs,
    quoteFormulaSheetName,
    isFormulaStringJs,
    parseCellRefParts,
    colIndexToLetters,
    colLettersToIndex,
    normalizeFormattedNumericCell,
    localScrubPii,
    serializeDomChildren,
    restoreDomChildren,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
