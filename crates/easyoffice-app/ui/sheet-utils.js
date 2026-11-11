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
    const s = String(str ?? "");
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i);
      if (c >= 97 && c <= 122) {
        let out = s.slice(0, i) + String.fromCharCode(c - 32);
        let start = i + 1;
        for (let j = start; j < s.length; j++) {
          const cj = s.charCodeAt(j);
          if (cj >= 97 && cj <= 122) {
            if (j > start) out += s.slice(start, j);
            out += String.fromCharCode(cj - 32);
            start = j + 1;
          }
        }
        return start < s.length ? out + s.slice(start) : out;
      }
    }
    return s;
  }

  function quoteFormulaSheetName(sheetName) {
    const raw = String(sheetName ?? "");
    if (!raw) return "''";
    const up = asciiUpperJs(raw);
    const hasEdgeSpace = raw !== raw.trim();
    const hasNonAlphaNum = !/^\w+$/.test(raw);
    const startsWithDigit = /^\d/.test(raw);
    const looksLikeCell = /^[A-Za-z]{1,3}\d+$/.test(raw);
    const isReserved = up === "TRUE" || up === "FALSE";
    if (hasEdgeSpace || hasNonAlphaNum || startsWithDigit || looksLikeCell || isReserved) {
      return "'" + raw.replaceAll("'", "''") + "'";
    }
    return raw;
  }

  function parseStrictA1Coord(rawToken) {
    const str = String(rawToken ?? "").trim();
    const candidateMatch = /^\$?([A-Za-z]{1,3})\$?(\d+)$/.exec(str);
    if (!candidateMatch) {
      return { ok: false, errorCode: "#NAME?" };
    }
    const letters = asciiUpperJs(candidateMatch[1]);
    const digits = candidateMatch[2];
    if (digits.startsWith("0")) {
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

  const GROUPED_NUMERIC_RE = /^[+-]?\d{1,3}(,\d{3})+(\.\d+)?$/;
  const CURRENCY_NUMERIC_RE = /^([+-]?)\s*[₩$€£¥]\s*([+-]?\d{1,3}(?:,\d{3})*(?:\.\d+)?|[+-]?\d+(?:\.\d+)?)$/;
  const PERCENT_NUMERIC_RE = /^([+-]?\d{1,3}(?:,\d{3})*(?:\.\d+)?|[+-]?\d+(?:\.\d+)?)\s*%$/;

  function normalizeFormattedNumericCell(raw) {
    const trimmed = String(raw || "").trim();
    if (GROUPED_NUMERIC_RE.test(trimmed)) {
      return trimmed.replaceAll(",", "");
    }
    const curMatch = CURRENCY_NUMERIC_RE.exec(trimmed);
    if (curMatch) {
      const sign = curMatch[1] === "-" || curMatch[2].startsWith("-") ? "-" : "";
      const digits = curMatch[2].replace(/^[+-]/, "").replaceAll(",", "");
      return sign + digits;
    }
    const pctMatch = PERCENT_NUMERIC_RE.exec(trimmed);
    if (pctMatch) {
      return pctMatch[1].replaceAll(",", "");
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

  // ASCII_CLASS bitmask: 1=digit, 2=upper, 4=lower, 8=email_punct, 16=card_sep, 32=passport_type
  const ASCII_CLASS = new Uint8Array(128);
  for (let c = 48; c <= 57; c += 1) ASCII_CLASS[c] |= 1;
  for (let c = 65; c <= 90; c += 1) ASCII_CLASS[c] |= 2;
  for (let c = 97; c <= 122; c += 1) ASCII_CLASS[c] |= 4;
  for (const ch of ["_", "%", "+", "-"]) ASCII_CLASS[ch.charCodeAt(0)] |= 8;
  for (const ch of ["-", " ", "\t", "\n", "\r", "\f"]) ASCII_CLASS[ch.charCodeAt(0)] |= 16;
  for (const ch of ["M", "S", "R", "O", "D", "G"]) ASCII_CLASS[ch.charCodeAt(0)] |= 32;

  function isAsciiDigitCode(c) {
    return c < 128 && (ASCII_CLASS[c] & 1) !== 0;
  }

  function isUnicodeAlnum(ch) {
    if (!ch) return false;
    const cp = ch.charCodeAt(0);
    if (cp < 128) return (ASCII_CLASS[cp] & 7) !== 0;
    return UNICODE_ALNUM_RE.test(ch);
  }

  function isUnicodeAlpha(ch) {
    return UNICODE_ALPHA_RE.test(ch);
  }

  function charAlphaScript(ch) {
    const code = ch.codePointAt(0);
    if (code < 128) return (ASCII_CLASS[code] & 6) !== 0 ? 1 : 0;
    if (isUnicodeAlpha(ch)) return 2;
    return 0;
  }

  function isEmailLocalNonDot(ch) {
    if (!ch) return false;
    const cp = ch.charCodeAt(0);
    if (cp < 128) return (ASCII_CLASS[cp] & 15) !== 0;
    return UNICODE_ALNUM_RE.test(ch);
  }

  function readCodePointAt(str, pos) {
    const cp = str.codePointAt(pos);
    const step = cp > 0xffff ? 2 : 1;
    return { ch: str.slice(pos, pos + step), step };
  }

  function readPrevCodePoint(str, pos) {
    if (pos <= 0) return null;
    // A code point above 0xFFFF at pos - 2 means chars pos-2..pos-1 form a surrogate pair.
    if (pos >= 2 && str.codePointAt(pos - 2) > 0xffff) return str.slice(pos - 2, pos);
    return str[pos - 1];
  }

  // One DNS label: alphanumerics (single script) and inner hyphens.
  function scanDomainLabelJs(str, labelStart) {
    let cursor = labelStart;
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
      if (sc === 0) allAlpha = false;
      else if (labelScript !== 0 && labelScript !== sc) break;
      else labelScript = sc;
      prevHyphen = false;
      cursor += step;
      labelChars += 1;
    }
    return { endPos: cursor, allAlpha, labelChars, prevHyphen };
  }

  // Start of the next label when a dot is followed by an alphanumeric, otherwise -1.
  function nextDomainLabelStartJs(str, cursor) {
    const afterDot = cursor + 1;
    if (str[cursor] !== "." || afterDot >= str.length) return -1;
    return isUnicodeAlnum(readCodePointAt(str, afterDot).ch) ? afterDot : -1;
  }

  // The match ends at the last label that can be a TLD (all letters, 2+ chars) with a label before it.
  function domainEndFromLabelsJs(labels) {
    while (labels.length >= 2) {
      const last = labels.at(-1);
      if (last.allAlpha && last.labelChars >= 2) return last.endPos;
      labels.pop();
    }
    return -1;
  }

  function scanDomainAfterAtJs(str, domStart) {
    if (domStart >= str.length) return -1;
    let cursor = domStart;
    const labels = [];
    while (cursor < str.length) {
      const label = scanDomainLabelJs(str, cursor);
      if (label.endPos === cursor || label.prevHyphen) break;
      labels.push({ endPos: label.endPos, allAlpha: label.allAlpha, labelChars: label.labelChars });
      const nextStart = nextDomainLabelStartJs(str, label.endPos);
      if (nextStart < 0) break;
      cursor = nextStart;
    }
    return domainEndFromLabelsJs(labels);
  }

  const EMAIL_BLOCKING_PREV_CHARS = new Set([".", "_", "%", "+", "-"]);

  function emailMissJs(skipUntil) {
    return { matchedLen: 0, skipUntil };
  }

  // A local part cannot start right after these characters (or inside an alphanumeric run of one script).
  function isBlockedByPrevCodePointJs(prevCh, firstCh) {
    if (prevCh === null) return false;
    if (EMAIL_BLOCKING_PREV_CHARS.has(prevCh)) return true;
    if (!isUnicodeAlnum(prevCh)) return false;
    const prevScript = charAlphaScript(prevCh);
    const curScript = charAlphaScript(firstCh);
    return prevScript === 0 || curScript === 0 || prevScript === curScript;
  }

  // Scans the local part; returns { p, prevWasDot } or { abortAt } when the candidate is rejected.
  function scanEmailLocalPartJs(str, idx) {
    let p = idx;
    let prevWasDot = false;
    let atomScript = 0;
    while (p < str.length) {
      const { ch, step } = readCodePointAt(str, p);
      if (ch === ".") {
        if (prevWasDot) return { abortAt: p + 1 };
        prevWasDot = true;
        atomScript = 0;
        p += 1;
        continue;
      }
      if (!isEmailLocalNonDot(ch)) break;
      const sc = charAlphaScript(ch);
      if (sc !== 0) {
        if (atomScript !== 0 && atomScript !== sc) return { abortAt: p };
        atomScript = sc;
      }
      prevWasDot = false;
      p += step;
    }
    return { p, prevWasDot };
  }

  function scanEmailAtJs(str, idx) {
    if (idx >= str.length) return emailMissJs(str.length);
    const first = readCodePointAt(str, idx);
    const stepOne = idx + first.step;
    if (!isEmailLocalNonDot(first.ch)) return emailMissJs(stepOne);
    if (isBlockedByPrevCodePointJs(readPrevCodePoint(str, idx), first.ch)) return emailMissJs(stepOne);
    const local = scanEmailLocalPartJs(str, idx);
    if (local.abortAt !== undefined) return emailMissJs(local.abortAt);
    const { p, prevWasDot } = local;
    if (p >= str.length || str[p] !== "@") return emailMissJs(Math.max(p, stepOne));
    if (prevWasDot) return emailMissJs(p + 1);
    const endDom = scanDomainAfterAtJs(str, p + 1);
    if (endDom < 0) return emailMissJs(p + 1);
    return { matchedLen: endDom - idx, skipUntil: endDom };
  }

  function isValidRrnAtJs(str, idx, genderIdx) {
    const month = (str.charCodeAt(idx + 2) - 48) * 10 + (str.charCodeAt(idx + 3) - 48);
    const day = (str.charCodeAt(idx + 4) - 48) * 10 + (str.charCodeAt(idx + 5) - 48);
    const gender = str.charCodeAt(genderIdx) - 48;
    return month >= 1 && month <= 12 && day >= 1 && day <= 31 && gender >= 1 && gender <= 8;
  }

  function matchRrnAtJs(str, idx) {
    if (!allDigitsJs(str, idx, 6) || (idx > 0 && isAsciiDigitCode(str.charCodeAt(idx - 1)))) return null;
    if (
      idx + 14 <= str.length &&
      str.charCodeAt(idx + 6) === 45 &&
      allDigitsJs(str, idx + 7, 7) &&
      isValidRrnAtJs(str, idx, idx + 7) &&
      (idx + 14 === str.length || !isAsciiDigitCode(str.charCodeAt(idx + 14)))
    ) {
      return { len: 14, replacement: "******-*******" };
    }
    if (
      idx + 13 <= str.length &&
      allDigitsJs(str, idx + 6, 7) &&
      isValidRrnAtJs(str, idx, idx + 6) &&
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

  function skipPhoneSepJs(str, p) {
    return p < str.length && isPhoneSepChar(str[p]) ? p + 1 : p;
  }

  function endsDigitRunJs(str, pos) {
    return pos === str.length || !isAsciiDigitCode(str.charCodeAt(pos));
  }

  // +82 [sep] area(1-3) [sep] mid(3-4) [sep] last(4)
  function matchIntlPhoneJs(str, idx) {
    if (!str.startsWith("+82", idx)) return null;
    const pArea = consumeDigitsJs(str, skipPhoneSepJs(str, idx + 3), 1, 3);
    if (pArea <= 0) return null;
    const pMid = consumeDigitsJs(str, skipPhoneSepJs(str, pArea), 3, 4);
    if (pMid <= 0) return null;
    const pEnd = consumeDigitsJs(str, skipPhoneSepJs(str, pMid), 4, 4);
    if (pEnd > 0 && endsDigitRunJs(str, pEnd)) return { len: pEnd - idx, replacement: "+82-**-****-****" };
    return null;
  }

  // mid(3-4) sep last(4) starting at midStart; returns the end offset or -1.
  function matchSeparatedTailJs(str, midStart, sep) {
    const pMid = consumeDigitsJs(str, midStart, 3, 4);
    if (!(pMid > 0 && pMid < str.length && str[pMid] === sep)) return -1;
    const pEnd = consumeDigitsJs(str, pMid + 1, 4, 4);
    return pEnd > 0 && endsDigitRunJs(str, pEnd) ? pEnd : -1;
  }

  function matchMobilePhoneJs(str, idx) {
    if (idx + 10 > str.length || str.charCodeAt(idx) !== 48 || str.charCodeAt(idx + 1) !== 49) return null;
    const c2 = str.charCodeAt(idx + 2);
    if (c2 !== 48 && c2 !== 49 && (c2 < 54 || c2 > 57)) return null;
    const prefix3 = str.slice(idx, idx + 3);
    const replacement = prefix3 + "-****-****";
    const p = idx + 3;
    if (p < str.length && isPhoneSepChar(str[p])) {
      const pEnd = matchSeparatedTailJs(str, p + 1, str[p]);
      return pEnd > 0 ? { len: pEnd - idx, replacement } : null;
    }
    const compact = allDigitsJs(str, idx + 3, 8) && endsDigitRunJs(str, idx + 11);
    return compact ? { len: 11, replacement } : null;
  }

  function matchLandlinePhoneJs(str, idx) {
    if (idx + 9 > str.length || str.charCodeAt(idx) !== 48) return null;
    const c1 = str.charCodeAt(idx + 1);
    let areaLen = 0;
    if (c1 === 50) {
      areaLen = 2;
    } else if (c1 >= 51 && c1 <= 54) {
      const c2 = str.charCodeAt(idx + 2);
      if (c2 >= 49 && c2 <= 53) areaLen = 3;
    }
    if (areaLen === 0) return null;
    const sep = str[idx + areaLen];
    if (!isPhoneSepChar(sep)) return null;
    const pEnd = matchSeparatedTailJs(str, idx + areaLen + 1, sep);
    if (pEnd < 0) return null;
    return { len: pEnd - idx, replacement: areaLen === 2 ? "02-****-****" : "***-****-****" };
  }

  function matchPhoneAtJs(str, idx) {
    if (idx > 0 && (isAsciiDigitCode(str.charCodeAt(idx - 1)) || str.charCodeAt(idx - 1) === 43)) {
      return null;
    }
    return matchIntlPhoneJs(str, idx) ?? matchMobilePhoneJs(str, idx) ?? matchLandlinePhoneJs(str, idx);
  }

  // Card / bank account / passport / driver's licence / business registration numbers.
  // Mirrors crates/easyoffice-app/src/pii_identifiers.rs.
  const BANK_ACCOUNT_GROUPS = [
    [3, 2, 6], [3, 3, 6], [3, 6, 3], [3, 6, 5], [4, 3, 6], [4, 2, 7], [6, 2, 6], [3, 4, 4, 2], [3, 6, 2, 3],
  ];
  const isHyphenChar = (ch) => ch === "-";
  const isCardSepChar = (ch) => {
    if (!ch) return false;
    const cp = ch.charCodeAt(0);
    return cp < 128 && (ASCII_CLASS[cp] & 16) !== 0;
  };
  const isAsciiUpperChar = (ch) => {
    if (!ch) return false;
    const cp = ch.charCodeAt(0);
    return cp < 128 && (ASCII_CLASS[cp] & 2) !== 0;
  };
  const isAsciiAlnumChar = (ch) => {
    if (!ch) return false;
    const cp = ch.charCodeAt(0);
    return cp < 128 && (ASCII_CLASS[cp] & 7) !== 0;
  };
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
    const first = str.charCodeAt(idx);
    if (!((first >= 50 && first <= 54) || first === 57)) return 0;
    let end = idx;
    while (end < str.length && end - idx < 20 && isAsciiDigitCode(str.charCodeAt(end))) end += 1;
    const len = end - idx;
    if (len < 13 || len > 19 || (end < str.length && isAsciiDigitCode(str.charCodeAt(end)))) return 0;
    return passesLuhnJs(str.slice(idx, end)) ? len : 0;
  }

  function matchPassportJs(str, idx) {
    if ((idx > 0 && isAsciiAlnumChar(str[idx - 1])) || idx + 9 > str.length) return 0;
    const c0 = str.charCodeAt(idx);
    const isType = c0 < 128 && (ASCII_CLASS[c0] & 32) !== 0;
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
      // Malformed JSON is treated as absent input.
      return null;
    }
  }

  function matchNonEmailPiiAtJs(text, idx) {
    const rrn = matchRrnAtJs(text, idx);
    if (rrn) return { kind: "resident", len: rrn.len, replacement: rrn.replacement };
    const ph = matchPhoneAtJs(text, idx);
    if (ph) return { kind: "phone", len: ph.len, replacement: ph.replacement };
    const idLen = matchIdentifierAtJs(text, idx);
    if (idLen <= 0) return null;
    return { kind: "identifier", len: idLen, replacement: text.slice(idx, idx + idLen).replace(/[A-Za-z0-9]/g, "*") };
  }

  function strippedMetadataFieldsJs(metaMap) {
    if (!metaMap) return [];
    return Object.keys(metaMap).filter((k) => {
      const val = metaMap[k];
      return k.trim() && val !== null && val !== undefined && String(val).trim() !== "";
    });
  }

  // Accepts either (text, metadata) or a JSON envelope { text, metadata } as the text argument.
  function resolveScrubInputJs(rawInput, metadataObj) {
    const text = String(rawInput ?? "");
    if (metadataObj && typeof metadataObj === "object") return { text, metaMap: metadataObj };
    const parsed = tryParseJsonEnvelope(text);
    if (!parsed || typeof parsed.text !== "string") return { text, metaMap: null };
    const hasMeta = parsed.metadata && typeof parsed.metadata === "object";
    return { text: parsed.text, metaMap: hasMeta ? parsed.metadata : null };
  }

  function localScrubPii(rawInput, metadataObj) {
    const { text, metaMap } = resolveScrubInputJs(rawInput, metadataObj);
    let scrubbedText = "";
    const counts = { resident: 0, phone: 0, email: 0, identifier: 0 };
    const spans = [];
    let idx = 0;
    let plainStart = 0;
    let emailFailedUntil = 0;

    while (idx < text.length) {
      let hit = null;
      if (idx >= emailFailedUntil) {
        const em = scanEmailAtJs(text, idx);
        if (em.matchedLen > 0) hit = { kind: "email", len: em.matchedLen, replacement: "***@***" };
        else emailFailedUntil = em.skipUntil;
      }
      hit ??= matchNonEmailPiiAtJs(text, idx);
      if (hit) {
        if (idx > plainStart) scrubbedText += text.slice(plainStart, idx);
        scrubbedText += hit.replacement;
        spans.push({ start: idx, end: idx + hit.len, replacement: hit.replacement });
        counts[hit.kind] += 1;
        idx += hit.len;
        plainStart = idx;
        continue;
      }
      idx += text.codePointAt(idx) > 0xffff ? 2 : 1;
    }
    if (plainStart === 0) scrubbedText = text;
    else if (plainStart < text.length) scrubbedText += text.slice(plainStart);

    return {
      scrubbedText,
      maskedResidentIds: counts.resident,
      maskedPhoneNumbers: counts.phone,
      maskedEmails: counts.email,
      maskedIdentifierNumbers: counts.identifier,
      totalMasked: counts.resident + counts.phone + counts.email + counts.identifier,
      spans,
      strippedMetadataFields: strippedMetadataFieldsJs(metaMap),
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
      "paddingRight",
      "textIndent",
      "fontFamily",
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
    for (const child of childList) {
      const serialized = serializeDomNode(child);
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
    if (!container?.childNodes) return [];
    const out = [];
    for (const childNode of container.childNodes) {
      const item = serializeDomNode(childNode);
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
    if (String(spec.tag).toLowerCase() === "img" && data.eoImageId) hydrateDocImage(el, data);
    const kids = Array.isArray(spec.children) ? spec.children : [];
    kids.forEach((childSpec) => {
      const childNode = buildDomNodeFromSpec(childSpec, doc);
      if (childNode) el.appendChild(childNode);
    });
    return el;
  }

  // ---------------------------------------------------------------- document pictures (PD-096)

  // Picture bytes stay out of the DOM and of undo snapshots: the editor holds an id (a data
  // attribute), this store holds the base64 bytes once, and the <img> gets a blob: URL.
  const DOC_IMAGE_MIME = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg" };
  const docImages = new Map();
  let docImageSeq = 0;

  function registerDocImage(base64, format) {
    docImageSeq += 1;
    const id = "img" + docImageSeq;
    docImages.set(id, { base64: String(base64), format: String(format), url: null });
    return id;
  }

  function getDocImage(id) {
    return docImages.get(String(id)) || null;
  }

  const BASE64_CHUNK = 0x8000;

  function base64ToBytes(base64) {
    if (host.atob === globalThis.atob && typeof Uint8Array.fromBase64 === "function") {
      return Uint8Array.fromBase64(String(base64 || ""));
    }
    const bin = host.atob(base64);
    const len = bin.length;
    const out = new Uint8Array(len);
    let i = 0;
    const limit = len - 3;
    for (; i < limit; i += 4) {
      out[i] = bin.charCodeAt(i);
      out[i + 1] = bin.charCodeAt(i + 1);
      out[i + 2] = bin.charCodeAt(i + 2);
      out[i + 3] = bin.charCodeAt(i + 3);
    }
    for (; i < len; i += 1) out[i] = bin.charCodeAt(i);
    return out;
  }

  function bytesToBase64(bytes, btoaFn) {
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
    const enc = btoaFn || host.btoa || globalThis.btoa;
    if (!btoaFn && enc === globalThis.btoa && typeof u8.toBase64 === "function") {
      return u8.toBase64();
    }
    if (u8.length <= BASE64_CHUNK) {
      return enc(String.fromCharCode.apply(null, u8));
    }
    const chunks = [];
    for (let i = 0; i < u8.length; i += BASE64_CHUNK) {
      chunks.push(String.fromCharCode.apply(null, u8.subarray(i, i + BASE64_CHUNK)));
    }
    return enc(chunks.join(""));
  }

  // blob: URL of a stored picture (created once); "" when the host cannot make one.
  function docImageUrl(id) {
    const entry = docImages.get(String(id));
    const mime = entry ? DOC_IMAGE_MIME[entry.format] : null;
    if (!entry || !mime) return "";
    if (entry.url === null) {
      const make = host.URL?.createObjectURL;
      try {
        entry.url = typeof make === "function" && typeof host.Blob === "function"
          ? make.call(host.URL, new host.Blob([base64ToBytes(entry.base64)], { type: mime }))
          : "";
      } catch {
        entry.url = "";
      }
    }
    return entry.url;
  }

  // Revokes the blob: URL of every stored picture that `root` no longer shows. The bytes stay (an
  // undo can bring the picture back, and `docImageUrl` then makes a fresh URL).
  function releaseDocImageUrls(root) {
    const shown = new Set();
    root?.querySelectorAll?.("img[data-eo-image-id]").forEach((img) => shown.add(img.dataset.eoImageId));
    docImages.forEach((entry, id) => {
      if (!entry.url || shown.has(id)) return;
      host.URL?.revokeObjectURL?.(entry.url);
      entry.url = null;
    });
  }

  function hydrateDocImage(el, data) {
    const url = docImageUrl(data.eoImageId);
    if (url) el.setAttribute("src", url);
    el.setAttribute("alt", String(data.eoImageAlt ?? ""));
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

  function aggregateNumbers(nums) {
    const count = Array.isArray(nums) ? nums.length : 0;
    if (count === 0) return { count: 0, sum: 0, min: 0, max: 0, avg: 0 };
    let sum = 0;
    let min = Number(nums[0]);
    let max = min;
    for (let i = 0; i < count; i++) {
      const v = Number(nums[i]);
      sum += v;
      if (v < min) min = v;
      if (v > max) max = v;
    }
    return { count, sum, min, max, avg: sum / count };
  }

  host.__EASYOFFICE_SHEET_UTILS__ = {
    asciiUpperJs,
    quoteFormulaSheetName,
    isFormulaStringJs,
    parseCellRefParts,
    colIndexToLetters,
    colLettersToIndex,
    normalizeFormattedNumericCell,
    aggregateNumbers,
    base64ToBytes,
    bytesToBase64,
    localScrubPii,
    serializeDomChildren,
    restoreDomChildren,
    registerDocImage,
    getDocImage,
    docImageUrl,
    releaseDocImageUrls,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
