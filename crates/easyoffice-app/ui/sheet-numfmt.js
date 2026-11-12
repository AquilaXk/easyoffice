// Number format presets and decimal-place editing of sheet cells (PD-193). Pure data and string
// logic, no DOM and no engine call.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9):
//   packages/sheets-numfmt/src/utils/decimal.ts            -> setPatternDecimal, getDecimalString,
//                                                              isPatternHasDecimal, decimalsOf
//   packages/sheets-numfmt/src/commands/commands/add-decimal.command.ts and
//   subtract-decimal.command.ts                             -> planDecimalChange
//   packages/sheets-numfmt/src/utils/scientific-notation.ts -> scientific helpers
//   packages/sheets-numfmt/src/base/const/currency-symbols.ts (getCurrencyFormat, symbols),
//   formatdetail.ts and date-format.ts (the Korean profile)  -> currencyFormat, PRESETS, date lists
//   packages/sheets-numfmt-ui/src/menu/number-format-options.ts -> PRESETS (menu order and codes)
// Differences: quoted text and bracket sections are masked while a code is edited (the original
// splits on `.` and `;` inside them); a text cell is left alone by the decimal commands.
(function (host) {
  "use strict";

  const MAX_DECIMALS = 30;
  const MAX_CODE_BYTES = 255;
  const MAX_CODES = 512;
  const DEFAULT_CURRENCY = "₩";
  // The currency symbols of localeCurrencySymbolMap, the won sign first.
  const CURRENCY_SYMBOLS = ["₩", "$", "¥", "€", "₽", "₫", "NT$", "HK$", "R$", "Rp", "zł", "﷼"];

  const isDefaultFormat = (code) => !code || String(code).trim().toLowerCase() === "general";
  const isTextFormat = (code) => String(code || "").trim() === "@";

  const getDecimalString = (length) => "0".repeat(Math.min(Math.max(0, Number(length) || 0), MAX_DECIMALS));

  // Quoted text, `[..]` sections and `\x` escapes are swapped for one private-use character each while
  // a code is edited, so their digits, dots and semicolons are not taken for placeholders.
  const MASK_RE = /"[^"]*"|\[[^\]]*\]|\\./g;
  const MASK_BASE = 0xe000;
  function mask(code) {
    const kept = [];
    const text = String(code).replace(MASK_RE, (m) => {
      kept.push(m);
      return String.fromCharCode(MASK_BASE + kept.length - 1);
    });
    return { text, restore: (s) => s.replace(/[\ue000-\uf8ff]/g, (ch) => kept[ch.charCodeAt(0) - MASK_BASE] ?? ch) };
  }

  // Port of setPatternDecimal: every section gets `decimalLength` decimal places.
  function setPatternDecimal(patterns, decimalLength) {
    const { text, restore } = mask(patterns);
    const digits = getDecimalString(decimalLength);
    const dot = decimalLength > 0 ? "." : "";
    const out = text.split(";").map((pattern) => {
      if (/\.0?/.test(pattern)) return pattern.replace(/\.0*/g, dot + digits);
      if (/0([^0]?)|0$/.test(pattern)) return pattern.replace(/0([^0]+)|0$/, `0${dot}${digits}$1`);
      return pattern;
    });
    return restore(out.join(";"));
  }

  const isPatternHasDecimal = (pattern) => {
    const { text } = mask(pattern);
    return /\.0?/.test(text) || /0([^0]?)|0$/.test(text);
  };

  // Decimal places of the first section (the original reads them from the format info).
  function decimalsOf(pattern, fallback = 0) {
    if (!pattern) return fallback;
    const first = mask(pattern).text.split(";")[0];
    const m = /\.([0#?]*)/.exec(first);
    return m ? m[1].length : fallback;
  }

  const NUMBER_RE = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/;
  const isNumberText = (raw) => NUMBER_RE.test(String(raw ?? "").trim());
  const isScientificText = (raw) => isNumberText(raw) && /e/i.test(String(raw));

  function scientificFormat(raw) {
    const text = String(raw);
    const e = text.search(/e/i);
    const dot = text.indexOf(".");
    const len = e > -1 && dot > -1 ? e - dot - 1 : 0;
    return `0.${"0".repeat(Math.max(2, len))}E+00`;
  }

  // Codes whose digits the decimal commands edit; dates, times, fractions and text are left alone.
  const DECIMAL_KINDS = new Set(["number", "currency", "accounting", "percent", "scientific", "custom"]);

  // New codes for `cells` ([{ code, raw }]) after adding (`delta` 1) or removing (-1) a decimal place.
  // Returns one entry per cell: the new code, or undefined for a cell that stays as it is.
  function planDecimalChange(cells, delta) {
    let maxDecimals = 0;
    for (const { code, raw } of cells) {
      if (!isDefaultFormat(code)) {
        if (DECIMAL_KINDS.has(patternKind(code))) maxDecimals = Math.max(maxDecimals, decimalsOf(code));
      } else if (isScientificText(raw)) {
        maxDecimals = Math.max(maxDecimals, decimalsOf(scientificFormat(raw)));
      } else if (!maxDecimals && isNumberText(raw)) {
        const m = /\.(\d*)$/.exec(String(raw).trim());
        if (m && m[1].length) maxDecimals = Math.max(maxDecimals, m[1].length);
      }
    }
    const decimals = maxDecimals + delta;
    const fallback = setPatternDecimal(`0${decimals > 0 ? ".0" : delta > 0 ? "" : "."}`, decimals);
    return cells.map(({ code, raw }) => {
      if (isDefaultFormat(code)) {
        if (!isNumberText(raw)) return undefined;
        return isScientificText(raw) ? setPatternDecimal(scientificFormat(raw), decimals) : fallback;
      }
      if (!DECIMAL_KINDS.has(patternKind(code))) return undefined;
      const next = setPatternDecimal(code, decimalsOf(code) + delta);
      return next === code ? undefined : next;
    });
  }

  // Port of getCurrencyFormat.
  function currencyFormat(symbol = DEFAULT_CURRENCY, digits = 2) {
    const n = Math.min(Math.max(0, digits), MAX_DECIMALS);
    const decimal = n > 0 ? "." + "0".repeat(n) : "";
    return `"${symbol}"#,##0${decimal}_);[Red]("${symbol}"#,##0${decimal})`;
  }

  // Presets in the menu order of number-format-options.ts; `code` "" is General.
  const PRESETS = Object.freeze([
    { id: "general", label: "일반", code: "" },
    { id: "text", label: "텍스트", code: "@" },
    { id: "number", label: "숫자", code: "0" },
    { id: "percent", label: "백분율", code: "0.00%" },
    { id: "scientific", label: "지수", code: "0.00E+00" },
    { id: "accounting", label: "회계", code: `"${DEFAULT_CURRENCY}" #,##0.00_);[Red]("${DEFAULT_CURRENCY}"#,##0.00)` },
    { id: "financial", label: "재무", code: "#,##0.00;[Red]#,##0.00" },
    { id: "currency", label: "통화", code: currencyFormat(DEFAULT_CURRENCY, 2) },
    { id: "rounding-currency", label: "통화(정수)", code: `"${DEFAULT_CURRENCY}"#,##0;[Red]"${DEFAULT_CURRENCY}"#,##0` },
    { id: "date", label: "날짜", code: "yyyy-mm-dd;@" },
    { id: "time", label: "시간", code: 'am/pm h":"mm":"ss' },
    { id: "datetime", label: "날짜 시간", code: "yyyy-m-d am/pm h:mm" },
    { id: "duration", label: "경과 시간", code: "[h]:mm:ss" },
  ]);
  const presetById = (id) => PRESETS.find((p) => p.id === id) || null;

  // Date and time codes of the Korean profile of date-format.ts (period before the time).
  const KO_DATES = ["yyyy-MM-dd", "yyyy. mm. dd.", 'yyyy"년" m"월" d"일"', "mm-dd", 'm"월" d"일"'];
  const KO_TIMES = ["h:mm:ss", "h:mm", "AM/PM hh:mm", "AM/PM h:mm", "AM/PM h:mm:ss"];
  const KO_DATE_TIMES = ["mm-dd AM/PM hh:mm"];
  const DATE_CODES = Object.freeze([...KO_DATES, ...KO_DATE_TIMES]);
  const TIME_CODES = Object.freeze(KO_TIMES);
  // 1930-08-05 13:30:30, the sample of the original's date list.
  const DATE_SAMPLE = "11175.562847222222";

  const FRACTION_CODES = Object.freeze(["# ?/?", "# ??/??", "# ???/???", "?/2", "?/4", "?/8", "?/16", "?/10", "?/100"]);

  // Negative number styles of formatdetail.ts (NUMBERFORMAT, CURRENCYFORMAT): how a negative value reads.
  const NEGATIVES = Object.freeze([
    { id: "minus", label: "-1,234.00" },
    { id: "red-minus", label: "-1,234.00 (빨강)" },
    { id: "paren", label: "(1,234.00)" },
    { id: "red-paren", label: "(1,234.00) (빨강)" },
  ]);

  // `body` is the digits part (`#,##0.00`, `"₩"#,##0`); `negative` is a NEGATIVES id.
  function withNegative(body, negative) {
    switch (negative) {
      case "red-minus":
        return `${body}_);[Red]-${body}`;
      case "paren":
        return `${body}_);(${body})`;
      case "red-paren":
        return `${body}_);[Red](${body})`;
      default:
        return body;
    }
  }

  function digitsBody(decimals, thousands) {
    const n = Math.min(Math.max(0, decimals | 0), MAX_DECIMALS);
    return (thousands ? "#,##0" : "0") + (n > 0 ? "." + "0".repeat(n) : "");
  }

  // The code a dialog category builds from its fields; `fields` is { decimals, thousands, symbol, negative, code }.
  function buildCode(kind, fields) {
    const f = { decimals: 2, thousands: true, symbol: DEFAULT_CURRENCY, negative: "minus", code: "", ...fields };
    switch (kind) {
      case "general":
        return "";
      case "text":
        return "@";
      case "number":
        return withNegative(digitsBody(f.decimals, f.thousands), f.negative);
      case "currency":
        return withNegative(`"${f.symbol}"${digitsBody(f.decimals, true)}`, f.negative);
      case "accounting":
        return `"${f.symbol}" ${digitsBody(f.decimals, true)}_);[Red]("${f.symbol}"${digitsBody(f.decimals, true)})`;
      case "percent":
        return digitsBody(f.decimals, false) + "%";
      case "scientific":
        return digitsBody(f.decimals, false) + "E+00";
      default:
        return String(f.code || "");
    }
  }

  // The dialog category of a code (port of getPatternType, with the editor's own categories).
  function patternKind(code) {
    const c = String(code || "");
    if (isDefaultFormat(c)) return "general";
    if (isTextFormat(c)) return "text";
    const literal = c.split(";")[0];
    const plain = mask(literal).text;
    if (/E[+-]/i.test(plain)) return "scientific";
    if (plain.includes("%")) return "percent";
    if (/\?\/[?\d]/.test(plain)) return "fraction";
    const clock = plain.replace(/AM\/PM|A\/P/gi, "");
    const hasTime = /\[(?:h+|m+|s+)\]/i.test(literal) || /[hs]/i.test(clock) || clock !== plain;
    const hasDate = /[yd]/i.test(clock) || /m{3,}/i.test(clock) || (/m/i.test(clock) && !hasTime && !/[0#]/.test(clock));
    if (hasDate) return "date";
    if (hasTime) return "time";
    if (CURRENCY_SYMBOLS.some((sym) => literal.includes(sym)) || literal.includes("[$")) {
      return /"\s+#|\*\s/.test(literal) ? "accounting" : "currency";
    }
    return /[0#]/.test(plain) ? "number" : "custom";
  }

  // A number wider than its cell shows as hash marks, as spreadsheets do: `measure(text)` is the text's
  // width and `width` the room in the cell; the marks fill the room (at least one).
  function fitNumber(text, width, measure) {
    if (measure(text) <= width) return text;
    const mark = measure("#");
    return "#".repeat(Math.max(1, mark > 0 ? Math.floor(width / mark) : 1));
  }

  // The code is something the sheet can hold: within the length and count bounds.
  const codeWithinBounds = (code) => new TextEncoder().encode(String(code)).length <= MAX_CODE_BYTES && !/[\u0000-\u001f]/.test(code);

  host.__EASYOFFICE_SHEET_NUMFMT__ = Object.freeze({
    MAX_DECIMALS,
    MAX_CODE_BYTES,
    MAX_CODES,
    DEFAULT_CURRENCY,
    CURRENCY_SYMBOLS,
    PRESETS,
    DATE_CODES,
    TIME_CODES,
    DATE_SAMPLE,
    FRACTION_CODES,
    NEGATIVES,
    presetById,
    isDefaultFormat,
    isTextFormat,
    getDecimalString,
    setPatternDecimal,
    isPatternHasDecimal,
    decimalsOf,
    isNumberText,
    planDecimalChange,
    currencyFormat,
    buildCode,
    patternKind,
    codeWithinBounds,
    fitNumber,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
