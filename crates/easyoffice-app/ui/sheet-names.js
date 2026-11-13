(function (host) {
  "use strict";

  // The defined names of the workbook (PD-214): the list the page holds, the lookup of a name for a sheet, and
  // the text of what a name refers to. The engine owns the meaning of a name (`sheet_names_ipc.rs`: it expands the
  // names in the formulas, checks a new name and moves the references with a row or column edit); this module
  // stores `[{ name, scope?, refersTo, comment?, hidden? }]` and answers the questions that need no engine. `scope`
  // is a sheet's name (absent for the whole workbook); `refersTo` is a formula without the leading `=` whose
  // references name their sheet (`Sheet1!$A$1:$B$2`).

  const MAX_NAMES = 10000;
  const MAX_NAME_CHARS = 255;
  const MAX_COMMENT_CHARS = 255;
  const MAX_REFERS_TO_CHARS = 8192;
  const MAX_ROW = 1048576;
  const MAX_COL = 16384;
  // The name box lists at most this many names (the spreadsheet help: the name box shows up to 100).
  const NAME_BOX_LIMIT = 100;

  const refs = () => host.__EASYOFFICE_SHEET_REFS__;
  const quoteSheet = (sheet) => host.__EASYOFFICE_SHEET_UTILS__.quoteFormulaSheetName(sheet);
  const fold = (text) => String(text ?? "").toLowerCase();

  // ---- what a name refers to ------------------------------------------------------------------------

  // `Sheet1!$A$1:$B$2` or `'My Data'!A1` as `{ sheet, r1, c1, r2, c2 }` (1-based), null for anything that is not one
  // plain range on one sheet (a constant, a formula, several ranges).
  function parseRange(refersTo) {
    const text = String(refersTo ?? "").trim().replace(/^=/, "");
    let sheet;
    let rest;
    if (text.startsWith("'")) {
      let i = 1;
      let name = "";
      for (;;) {
        const q = text.indexOf("'", i);
        if (q < 0) return null;
        name += text.slice(i, q);
        if (text[q + 1] === "'") {
          name += "'";
          i = q + 2;
        } else {
          if (text[q + 1] !== "!") return null;
          sheet = name;
          rest = text.slice(q + 2);
          break;
        }
      }
    } else {
      const bang = text.indexOf("!");
      if (bang <= 0) return null;
      sheet = text.slice(0, bang);
      rest = text.slice(bang + 1);
    }
    const [a, b = a, extra] = rest.replaceAll("$", "").split(":");
    const p = refs().parseRef(a);
    const q = refs().parseRef(b);
    if (extra !== undefined || !p || !q) return null;
    return { sheet, r1: Math.min(p.row, q.row), c1: Math.min(p.col, q.col), r2: Math.max(p.row, q.row), c2: Math.max(p.col, q.col) };
  }

  // The reference text of a range on a sheet: `Sheet1!$A$1:$B$2`.
  function refersToOf(sheet, rg) {
    const corner = (r, c) => "$" + refs().colName(c) + "$" + r;
    const body = rg.r1 === rg.r2 && rg.c1 === rg.c2 ? corner(rg.r1, rg.c1) : corner(rg.r1, rg.c1) + ":" + corner(rg.r2, rg.c2);
    return quoteSheet(sheet) + "!" + body;
  }

  // ---- the list -------------------------------------------------------------------------------------

  let list = [];
  let version = 0;
  let listener = null;

  const normalized = (def) => {
    const out = { name: String(def.name ?? ""), refersTo: String(def.refersTo ?? "") };
    if (def.scope) out.scope = String(def.scope);
    if (def.comment) out.comment = String(def.comment);
    if (def.hidden === true) out.hidden = true;
    return out;
  };

  const sameScope = (a, b) => fold(a) === fold(b);

  function getAll() {
    return list.map((def) => ({ ...def }));
  }

  // Replaces the list (an undo, a file, an answer of the engine); `silent` leaves the page's formulas alone, as when
  // a file is opened and the cells come with it.
  function replaceAll(next, silent) {
    list = (Array.isArray(next) ? next : []).filter((def) => def && typeof def.name === "string").map(normalized);
    version += 1;
    if (!silent) listener?.();
  }

  const hasAny = () => list.length > 0;
  // The list as JSON, made once per change of the list (every evaluation compares it with what the session holds).
  let payloadText = "";
  let payloadVersion = -1;
  function payload() {
    if (payloadVersion !== version) {
      payloadText = JSON.stringify(list);
      payloadVersion = version;
    }
    return payloadText;
  }
  const getVersion = () => version;

  function indexOfName(name, scope) {
    return list.findIndex((def) => fold(def.name) === fold(name) && (def.scope ? sameScope(def.scope, scope) : !scope));
  }

  // The name `name` means for a formula on `sheet`: that sheet's own name first, then the workbook's.
  function find(name, sheet) {
    const hits = list.filter((def) => fold(def.name) === fold(name));
    return hits.find((def) => def.scope && sameScope(def.scope, sheet)) ?? hits.find((def) => !def.scope) ?? null;
  }

  // The names the name box and the formulas of `sheet` see: the workbook's and that sheet's, not the hidden ones, a
  // sheet's own name hiding the workbook's of the same name. At most `NAME_BOX_LIMIT`.
  function visibleFor(sheet) {
    const own = new Set(list.filter((def) => def.scope && sameScope(def.scope, sheet)).map((def) => fold(def.name)));
    return list
      .filter((def) => !def.hidden && (def.scope ? sameScope(def.scope, sheet) : !own.has(fold(def.name))))
      .slice(0, NAME_BOX_LIMIT);
  }

  // The ranges the visible names refer to, by sheet and corners: one pass per change of the list, so the name box
  // answers a selection with one lookup however many names there are.
  let rangeIndex = null;
  let rangeIndexVersion = -1;

  function rangesByCorners() {
    if (rangeIndex && rangeIndexVersion === version) return rangeIndex;
    rangeIndex = new Map();
    rangeIndexVersion = version;
    for (const def of list) {
      if (def.hidden) continue;
      const at = parseRange(def.refersTo);
      if (!at || (def.scope && !sameScope(def.scope, at.sheet))) continue;
      const key = fold(at.sheet) + "|" + at.r1 + "," + at.c1 + "," + at.r2 + "," + at.c2;
      if (!rangeIndex.has(key)) rangeIndex.set(key, def.name);
    }
    return rangeIndex;
  }

  // The name of the definition that refers to exactly the range `rg` of `sheet`, for the name box to show it.
  function nameOfRange(sheet, rg) {
    if (list.length === 0) return "";
    return rangesByCorners().get(fold(sheet) + "|" + rg.r1 + "," + rg.c1 + "," + rg.r2 + "," + rg.c2) ?? "";
  }

  // `A1`, `$B$2` or `A1:C9` as a range, null when the text is not an address; whole rows (`3:5`) and columns (`B:D`)
  // are ranges too.
  function parseAddress(text) {
    const value = String(text ?? "").trim().replaceAll("$", "");
    if (!value) return null;
    const [a, b = a, extra] = value.split(":");
    if (extra !== undefined) return null;
    const p = refs().parseRef(a);
    const q = refs().parseRef(b);
    if (p && q) return { r1: Math.min(p.row, q.row), c1: Math.min(p.col, q.col), r2: Math.max(p.row, q.row), c2: Math.max(p.col, q.col) };
    // A lone word of letters is a name, not a column: whole rows and columns are written with their colon.
    if (value.indexOf(":") < 0) return null;
    if (/^\d{1,7}$/.test(a) && /^\d{1,7}$/.test(b)) {
      const [r1, r2] = [Math.min(Number(a), Number(b)), Math.max(Number(a), Number(b))];
      return r1 >= 1 && r2 <= MAX_ROW ? { r1, c1: 1, r2, c2: MAX_COL } : null;
    }
    if (/^[A-Za-z]{1,3}$/.test(a) && /^[A-Za-z]{1,3}$/.test(b)) {
      const [c1, c2] = [refs().colIndex(a.toUpperCase()), refs().colIndex(b.toUpperCase())].sort((x, y) => x - y);
      return c2 <= MAX_COL ? { r1: 1, c1, r2: MAX_ROW, c2 } : null;
    }
    return null;
  }

  // ---- the check before a definition is sent --------------------------------------------------------

  // The limits that need no engine. `null` when the fields fit; else the problem's code.
  function limitProblem(def) {
    if (!def.name) return "empty";
    if ([...def.name].length > MAX_NAME_CHARS) return "tooLong";
    if ([...(def.comment || "")].length > MAX_COMMENT_CHARS) return "comment";
    if (!String(def.refersTo ?? "").trim()) return "refersEmpty";
    if ([...String(def.refersTo)].length > MAX_REFERS_TO_CHARS) return "formula";
    return null;
  }

  host.__EASYOFFICE_SHEET_NAMES__ = {
    MAX_NAMES,
    MAX_NAME_CHARS,
    MAX_COMMENT_CHARS,
    NAME_BOX_LIMIT,
    parseRange,
    parseAddress,
    refersToOf,
    getAll,
    replaceAll,
    hasAny,
    payload,
    getVersion,
    indexOfName,
    find,
    visibleFor,
    nameOfRange,
    limitProblem,
    fold,
    on(event, fn) {
      if (event === "changed") listener = fn;
    },
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
