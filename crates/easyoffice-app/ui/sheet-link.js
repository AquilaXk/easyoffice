// Hyperlinks of sheet cells (PD-218): the model the page keeps, the address policy and the moves that
// follow a rename or a row or column edit. A link belongs to the cell entry (`entry.link`), so every move of
// an entry (sort, row and column edits, copy, fill) takes it along; the text a link shows is the cell's own value.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0, Copyright
// 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9):
//   packages/sheets-hyper-link/src/types/interfaces/i-hyper-link.ts :17-47 (a link: `payload`, `tooltip`, cell),
//   packages/sheets-hyper-link/src/services/parser.service.ts :50-91 (`#`-prefixed targets are jumps inside
//       the workbook, any other target is an address),
//   packages/sheets-hyper-link-ui/src/controllers/auto-fill.controller.ts :103-136, :145-153 (a fill copies the link in the
//       copy and series modes), copy-paste.controller.ts :157-166 (the special pastes leave links alone).
// Differences: Univer removes the link when the cell is written or cleared (`set-range.controller.ts :43-139`); here a
// link stays through any text the cell is given, as in the spreadsheet programs, and goes with a cut, a paste or fill of
// cells, or the removal command. Univer keeps the links in a model apart from the cells and moves them with its reference-range
// service; here the link rides on the cell entry, and only the in-workbook target text is rewritten by the
// engine (`linkTargets` of the row and column edit and the sheet rename answers). A target names its sheet
// (Univer names it by id). See THIRD_PARTY_NOTICES.md.
(function (host) {
  "use strict";

  const MAX_TIP_CHARS = 255;
  const MAX_LOCATION_CHARS = 255;
  const MAX_LINKS_PER_SHEET = 65530;
  // The control characters of a screen tip, except the line break.
  // A cell address of a file: letters then a row (the key of an entry; anything else could be `__proto__`).
  const CELL_REF_RE = /^[A-Z]{1,3}[1-9][0-9]{0,6}$/;
  const TIP_CONTROL_RE = /[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/g;

  const quoteSheet = (sheet) => host.__EASYOFFICE_SHEET_UTILS__.quoteFormulaSheetName(sheet);

  // The address as it is kept, or null when the policy refuses it: a plain web or mail address (the document
  // editor's rule, `doc-link.js`), or `#` and an in-workbook location of at most 255 characters.
  function normalizeTarget(raw) {
    const text = String(raw ?? "").trim();
    if (text.startsWith("#")) {
      const location = text.slice(1);
      const bad = location === "" || Array.from(location).length > MAX_LOCATION_CHARS || /[\u0000-\u001f\u007f-\u009f]/.test(location);
      return bad ? null : text;
    }
    return host.__EASYOFFICE_DOC_LINK__?.normalizeUrl(text) ?? null;
  }

  const tipOf = (raw) => Array.from(String(raw ?? "").replace(TIP_CONTROL_RE, "")).slice(0, MAX_TIP_CHARS).join("");

  // A frozen link for `target` and `tip`, or null when the policy refuses the address.
  function make(target, tip) {
    const kept = normalizeTarget(target);
    if (kept === null) return null;
    const link = { target: kept };
    const text = tipOf(tip);
    if (text !== "") link.tip = text;
    return Object.freeze(link);
  }

  const of = (entry) => entry?.link ?? null;
  const isInternal = (target) => String(target ?? "").startsWith("#");
  const locationOf = (target) => String(target ?? "").slice(1);

  // `#Sheet1!B2` or `#'Sheet 2'!A1:B3` for a range on a sheet (1-based corners).
  function internalTarget(sheet, rg) {
    const refs = host.__EASYOFFICE_SHEET_REFS__;
    const corner = (r, c) => refs.colName(c) + r;
    const one = rg.r1 === rg.r2 && rg.c1 === rg.c2;
    return "#" + quoteSheet(sheet) + "!" + (one ? corner(rg.r1, rg.c1) : corner(rg.r1, rg.c1) + ":" + corner(rg.r2, rg.c2));
  }

  // Formulas whose evaluated result can carry a HYPERLINK target (PD-223, PD-236). When the engine has evaluated the cell
  // (`entry.formulaLink` boolean), its exact per-cell handle state wins (covering defined names, branch selection, and
  // spilled child cells); before evaluation, a single-pass scan checks for a 1..2-argument HYPERLINK call not stripped by a
  // top-level operator.
  const ERROR_VALUE_RE = /^#(VALUE!|REF!|NAME\?|N\/A|NUM!|DIV\/0!|NULL!|SPILL!|CALC!)$/;
  const isIdentCont = (ch) =>
    (ch >= 65 && ch <= 90) || (ch >= 97 && ch <= 122) || (ch >= 48 && ch <= 57) || ch === 95 || ch === 46 || ch >= 128;

  function skipQuoted(src, i, quote) {
    let p = i + 1;
    while (p < src.length) {
      if (src.charCodeAt(p) === quote) {
        if (p + 1 < src.length && src.charCodeAt(p + 1) === quote) p += 2;
        else return p + 1;
      } else p += 1;
    }
    return p;
  }

  function validHyperlinkCallAt(src, openIdx) {
    let depth = 1, commas = 0, hasToken = false, p = openIdx + 1;
    while (p < src.length && depth > 0) {
      const ch = src.charCodeAt(p);
      if (ch === 34 || ch === 39) {
        hasToken = true;
        p = skipQuoted(src, p, ch);
        continue;
      }
      if (ch === 40) {
        depth += 1;
        hasToken = true;
      } else if (ch === 41) {
        depth -= 1;
        if (depth === 0) return hasToken && commas <= 1;
      } else if (ch === 44 && depth === 1) {
        commas += 1;
        if (commas > 1) return false;
      } else if (ch > 32) {
        hasToken = true;
      }
      p += 1;
    }
    return false;
  }

  function mayYieldHyperlink(formula) {
    const src = String(formula ?? "").trim();
    if (!src.startsWith("=")) return false;
    let i = 1;
    while (i < src.length && src.charCodeAt(i) <= 32) i += 1;
    if (i < src.length && src.charCodeAt(i) === 43) i += 1;
    let depth = 0, sawHyperlink = false;
    while (i < src.length) {
      const ch = src.charCodeAt(i);
      if (ch === 34 || ch === 39) {
        i = skipQuoted(src, i, ch);
        continue;
      }
      if (ch === 40) {
        depth += 1;
        i += 1;
        continue;
      }
      if (ch === 41) {
        if (depth === 0) return false;
        depth -= 1;
        i += 1;
        continue;
      }
      if (depth === 0 && (ch === 38 || ch === 43 || ch === 45 || ch === 42 || ch === 47 || ch === 94 || ch === 61 || ch === 60 || ch === 62 || ch === 37)) {
        return false;
      }
      if ((ch >= 65 && ch <= 90) || (ch >= 97 && ch <= 122) || ch === 95 || ch >= 128) {
        const start = i;
        i += 1;
        while (i < src.length && isIdentCont(src.charCodeAt(i))) i += 1;
        let p = i;
        while (p < src.length && src.charCodeAt(p) <= 32) p += 1;
        if (p < src.length && src.charCodeAt(p) === 40 && i - start === 9 && src.slice(start, i).toUpperCase() === "HYPERLINK") {
          if (validHyperlinkCallAt(src, p)) sawHyperlink = true;
        }
        continue;
      }
      i += 1;
    }
    return depth === 0 && sawHyperlink;
  }

  function isHyperlinkFormula(entry) {
    if (!entry || ERROR_VALUE_RE.test(String(entry.value ?? ""))) return false;
    if (typeof entry.formulaLink === "boolean") return entry.formulaLink;
    return mayYieldHyperlink(entry.formula || "");
  }

  // O(1) ASCII lookup table for RFC 5322 dot-atom characters in bare email AutoCorrect (PD-236).
  const EMAIL_LOCAL_OK = (() => {
    const tbl = new Uint8Array(128);
    const mark = (s) => {
      for (let i = 0; i < s.length; i += 1) tbl[s.charCodeAt(i)] = 1;
    };
    for (let c = 48; c <= 57; c += 1) tbl[c] = 1;
    for (let c = 65; c <= 90; c += 1) tbl[c] = 1;
    for (let c = 97; c <= 122; c += 1) tbl[c] = 1;
    mark("!#$%&'*+/=?^_`{|}~-.");
    return tbl;
  })();

  function isBareEmail(typed) {
    const len = typed.length;
    if (len < 6 || len > 254) return false;
    const at = typed.indexOf("@");
    if (at < 1 || at > 64 || typed.indexOf("@", at + 1) !== -1) return false;
    if (typed.charCodeAt(0) === 46 || typed.charCodeAt(at - 1) === 46) return false;
    let prevDot = false;
    for (let i = 0; i < at; i += 1) {
      const c = typed.charCodeAt(i);
      if (c >= 128 || EMAIL_LOCAL_OK[c] === 0) return false;
      if (c === 46) {
        if (prevDot) return false;
        prevDot = true;
      } else prevDot = false;
    }
    const domLen = len - at - 1;
    if (domLen < 4 || domLen > 253) return false;
    let labelLen = 0, labels = 1, allAlpha = true, prevHyphen = false;
    for (let i = at + 1; i < len; i += 1) {
      const c = typed.charCodeAt(i);
      if (c === 46) {
        if (labelLen === 0 || labelLen > 63 || prevHyphen) return false;
        labels += 1;
        labelLen = 0;
        allAlpha = true;
        prevHyphen = false;
        continue;
      }
      const alpha = (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
      const digit = c >= 48 && c <= 57;
      if (!alpha && !digit && c !== 45) return false;
      if (c === 45 && labelLen === 0) return false;
      if (!alpha) allAlpha = false;
      prevHyphen = c === 45;
      labelLen += 1;
    }
    return labels >= 2 && labelLen >= 2 && labelLen <= 63 && !prevHyphen && allAlpha;
  }

  // The address a typed text becomes under the AutoCorrect rule of the spreadsheet programs, "Internet and network paths with
  // hyperlinks" (PD-223, PD-236): the whole text is one word starting with http://, https:// or mailto:, with www. and a dot
  // after it (http://), or a bare email address local@domain.tld (mailto:). Null for any other text or refused target.
  const MAX_ADDRESS_CHARS = 2048;
  const AUTO_SCHEME_RE = /^(?:https?:\/\/|mailto:)/i;
  const AUTO_WHITESPACE_RE = /\s/;
  function autoTarget(text) {
    const typed = String(text ?? "").trim();
    if (typed.length > MAX_ADDRESS_CHARS || AUTO_WHITESPACE_RE.test(typed)) return null;
    const scheme = AUTO_SCHEME_RE.exec(typed);
    if (scheme) return typed.length > scheme[0].length ? normalizeTarget(typed) : null;
    if (typed.slice(0, 4).toLowerCase() === "www.") {
      const rest = typed.slice(4);
      const dot = rest.indexOf(".", 1);
      return rest[0] !== "." && dot > 0 && dot < rest.length - 1 ? normalizeTarget("http://" + typed) : null;
    }
    return isBareEmail(typed) ? normalizeTarget("mailto:" + typed) : null;
  }

  // A cell with a hyperlink shows as a link and says where it goes: the screen tip, else the address. A cell whose
  // formula is a HYPERLINK call shows as a link too (`data-link="formula"`); its address is evaluated when it is clicked.
  function paint(td, link, entry) {
    td._link = link || null;
    if (!link) {
      const formula = isHyperlinkFormula(entry);
      if (formula) {
        td.dataset.link = "formula";
        td.removeAttribute("title");
      } else if (td.dataset.link) {
        delete td.dataset.link;
        td.removeAttribute("title");
      }
      return;
    }
    const internal = isInternal(link.target);
    td.dataset.link = internal ? "internal" : "external";
    td.setAttribute("title", link.tip || (internal ? locationOf(link.target) : link.target));
  }

  // The links of a sheet map for a save: `[{ ref, target, tip? }]`.
  function payloadOf(map) {
    const out = [];
    for (const ref of Object.keys(map || {})) {
      const entry = map[ref];
      if (!entry?.link) continue;
      out.push(entry.link.tip ? { ref, target: entry.link.target, tip: entry.link.tip } : { ref, target: entry.link.target });
    }
    return out;
  }

  // Puts the links of a file on the entries of `map`; a link of a cell with nothing in it makes an empty entry.
  function attach(map, links) {
    let count = 0;
    for (const item of Array.isArray(links) ? links : []) {
      const link = CELL_REF_RE.test(item?.ref) ? make(item.target, item.tip) : null;
      if (!link || count >= MAX_LINKS_PER_SHEET) continue;
      (map[item.ref] ||= { value: "", formula: "" }).link = link;
      count += 1;
    }
  }

  // The in-workbook targets of every sheet, `{ sheet: [target, ...] }`, each once; null when no link jumps inside.
  function internalTargets(sheets) {
    let out = null;
    for (const sheet of Object.keys(sheets || {})) {
      const seen = new Set();
      for (const ref of Object.keys(sheets[sheet] || {})) {
        const target = sheets[sheet][ref]?.link?.target;
        if (target && isInternal(target)) seen.add(target);
      }
      if (seen.size > 0) (out ||= {})[sheet] = [...seen];
    }
    return out;
  }

  // `sheets` with the targets in `changes` (`{ sheet: [[old, new], ...] }`, an answer of the engine) replaced. A sheet
  // or an entry that changes is copied, the others are shared.
  function retarget(sheets, changes) {
    const out = { ...sheets };
    for (const sheet of Object.keys(changes || {})) {
      const map = out[sheet];
      if (!map) continue;
      const to = new Map(changes[sheet]);
      let copy = null;
      for (const ref of Object.keys(map)) {
        const target = map[ref]?.link?.target;
        const next = target === undefined ? undefined : to.get(target);
        if (next === undefined) continue;
        copy ||= { ...map };
        copy[ref] = { ...map[ref], link: make(next, map[ref].link.tip) ?? map[ref].link };
      }
      if (copy) out[sheet] = copy;
    }
    return out;
  }

  // The `linkTargets` field of a request, or an empty object when no link jumps inside the workbook.
  function requestField(sheets) {
    const targets = internalTargets(sheets);
    return targets ? { linkTargets: targets } : {};
  }

  host.__EASYOFFICE_SHEET_LINK__ = {
    MAX_TIP_CHARS,
    MAX_LOCATION_CHARS,
    MAX_LINKS_PER_SHEET,
    normalizeTarget,
    make,
    of,
    mayYieldHyperlink,
    isHyperlinkFormula,
    autoTarget,
    isInternal,
    locationOf,
    internalTarget,
    paint,
    payloadOf,
    attach,
    internalTargets,
    retarget,
    requestField,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
