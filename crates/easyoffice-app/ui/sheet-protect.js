(function (host) {
  "use strict";

  // Sheet and workbook protection of the sheet window (PD-216): what a protected sheet allows, which cells may be
  // changed, and the guards every edit entry point asks before it changes anything. A sheet's protection is the
  // engine's `protect` JSON in the layout store (`{ on, deny: [flag], pwd, ranges }`); a cell's lock is the
  // `unlocked`, `locked` and `hideFormula` keys of its style (`sheet-look.js`) and the unlocked rows and columns of
  // the layout. The workbook's structure protection is `{ structure, windows, pwd, revisions }` kept here and in the
  // undo snapshots. A password is only ever the engine's digest (`pwd`), never text.
  //
  // The flags are the attributes of `sheetProtection` (ECMA-376 18.3.1.85); their names in the dialog and their
  // order are those of the Korean spreadsheet help for 시트 보호 (the pages 시트 보호, 통합 문서 보호,
  // 셀 서식: 보호). The set matches the permission points of the open spreadsheet engines compared (cell value, cell
  // style, row and column style, insert and delete of rows and columns, sort, filter, select protected and
  // unprotected cells, hyperlink, extra objects, pivot table).

  const FLAGS = [
    ["selectLockedCells", "잠긴 셀 선택"],
    ["selectUnlockedCells", "잠기지 않은 셀 선택"],
    ["formatCells", "셀 서식"],
    ["formatColumns", "열 서식"],
    ["formatRows", "행 서식"],
    ["insertColumns", "열 삽입"],
    ["insertRows", "행 삽입"],
    ["insertHyperlinks", "하이퍼링크 삽입"],
    ["deleteColumns", "열 삭제"],
    ["deleteRows", "행 삭제"],
    ["sort", "정렬"],
    ["autoFilter", "자동 필터 사용"],
    ["pivotTables", "피벗 테이블 보고서 사용"],
    ["objects", "개체 편집"],
    ["scenarios", "시나리오 편집"],
  ];
  const FLAG_NAMES = FLAGS.map(([name]) => name);
  // A sheet protected from the dialog allows selecting cells and nothing else.
  const DEFAULT_ALLOWED = ["selectLockedCells", "selectUnlockedCells"];
  // What the status line says when an edit of a protected sheet is refused (the help's warning that a protected sheet takes no input, cut to the 28 characters of the status line); it answers every blocked edit.
  const BLOCKED = "보호된 시트의 셀을 변경할 수 없음";
  const WORKBOOK_BLOCKED = "통합 문서 보호";
  // Ranges up to this many cells are tested cell by cell.
  const EACH_CELL_LIMIT = 40000;

  const layout = () => host.__EASYOFFICE_SHEET_LAYOUT__;
  const refs = () => host.__EASYOFFICE_SHEET_GRID_UTILS__;

  function runHas(runs, i) {
    let lo = 0;
    let hi = runs.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (runs[mid][1] < i) lo = mid + 1;
      else if (runs[mid][0] > i) hi = mid - 1;
      else return true;
    }
    return false;
  }

  // Whether one run holds all of `a..b`.
  function runCovers(runs, a, b) {
    const hit = runs.find((r) => r[0] <= a && r[1] >= a);
    return Boolean(hit) && hit[1] >= b;
  }

  // ---- the sheet's protection ------------------------------------------------------------------------
  const protectOf = (sheet) => {
    const p = layout().protectOf(sheet);
    return p && p.on === true ? p : null;
  };
  const isOn = (sheet) => protectOf(sheet) !== null;
  const allows = (sheet, flag) => {
    const p = protectOf(sheet);
    return !p || !p.deny.includes(flag);
  };

  // The allowed ranges as boxes, parsed once per protection object.
  const boxCache = new WeakMap();
  function rangesOf(p) {
    let list = boxCache.get(p);
    if (list) return list;
    list = (p.ranges || []).map((range) => ({
      name: range.name,
      open: !range.pwd,
      boxes: (range.sqref || []).map(parseBox).filter(Boolean),
    }));
    boxCache.set(p, list);
    return list;
  }

  function parseBox(text) {
    const [a, b = a] = String(text).split(":");
    const p = refs().parseRef(a);
    const q = refs().parseRef(b);
    return p && q ? { r1: Math.min(p.row, q.row), c1: Math.min(p.col, q.col), r2: Math.max(p.row, q.row), c2: Math.max(p.col, q.col) } : null;
  }

  // Ranges a password opened in this window (not kept in a file or a snapshot).
  const unlockedRanges = new Set();
  const rangeKey = (sheet, name) => sheet + "\u0000" + name;

  function inOpenRange(sheet, p, row, col) {
    return rangesOf(p).some((range) => (range.open || unlockedRanges.has(rangeKey(sheet, range.name))) && range.boxes.some((b) => row >= b.r1 && row <= b.r2 && col >= b.c1 && col <= b.c2));
  }

  // The allowed range with a password that holds the cell, or null.
  function lockedRangeAt(sheet, row, col) {
    const p = protectOf(sheet);
    if (!p) return null;
    return rangesOf(p).find((range) => !range.open && !unlockedRanges.has(rangeKey(sheet, range.name)) && range.boxes.some((b) => row >= b.r1 && row <= b.r2 && col >= b.c1 && col <= b.c2)) || null;
  }

  // Whether the cell's own format or its row or column unlocks it.
  function cellUnlocked(sheet, row, col, style) {
    if (style?.unlocked) return true;
    if (style?.locked) return false;
    const api = layout();
    return runHas(api.unlockedOf(sheet, "row"), row) || runHas(api.unlockedOf(sheet, "col"), col);
  }

  // Whether the cell may be changed: the sheet is not protected, the cell is unlocked, or an allowed range holds it.
  function cellEditable(sheet, row, col, style) {
    const p = protectOf(sheet);
    return !p || cellUnlocked(sheet, row, col, style) || inOpenRange(sheet, p, row, col);
  }

  // Whether every cell of `rg` may be changed. `styleAt(row, col)` answers a cell's style; a range too large to
  // test cell by cell is editable only when one unlocked row or column run holds it whole.
  function rangeEditable(sheet, rg, styleAt) {
    const p = protectOf(sheet);
    if (!p) return true;
    const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
    if (area > EACH_CELL_LIMIT) {
      const api = layout();
      return runCovers(api.unlockedOf(sheet, "col"), rg.c1, rg.c2) || runCovers(api.unlockedOf(sheet, "row"), rg.r1, rg.r2);
    }
    for (let r = rg.r1; r <= rg.r2; r += 1) {
      for (let c = rg.c1; c <= rg.c2; c += 1) if (!cellEditable(sheet, r, c, styleAt(r, c))) return false;
    }
    return true;
  }

  // Whether the cell may be selected (잠긴 셀 선택, 잠기지 않은 셀 선택).
  function canSelect(sheet, row, col, style) {
    const p = protectOf(sheet);
    if (!p) return true;
    return p.deny.includes(cellUnlocked(sheet, row, col, style) ? "selectUnlockedCells" : "selectLockedCells") === false;
  }

  // Whether a protected sheet hides the formula of the cell.
  const formulaHidden = (sheet, row, col, style) => Boolean(style?.hideFormula) && isOn(sheet);

  // ---- the answers every blocked edit gives ------------------------------------------------------------
  function say(text) {
    const fb = host.document?.getElementById?.("status-command-feedback");
    if (fb) fb.textContent = text;
  }

  // True when the sheet allows `flag`; else the status line says so.
  function guardAction(sheet, flag) {
    if (allows(sheet, flag)) return true;
    say(BLOCKED);
    return false;
  }

  // True when every cell of `rg` may be changed.
  function guardCells(sheet, rg, styleAt) {
    if (rangeEditable(sheet, rg, styleAt)) return true;
    say(BLOCKED);
    return false;
  }

  // True when the sheet is not protected (a change of the protection or of a cell's lock needs that).
  function guardUnprotected(sheet) {
    if (!isOn(sheet)) return true;
    say(BLOCKED);
    return false;
  }

  // ---- the workbook ----------------------------------------------------------------------------------
  let workbook = null;
  const listeners = [];
  const workbookJson = () => (workbook ? structuredClone(workbook) : null);

  function setWorkbook(next) {
    workbook = next && (next.structure || next.windows || next.pwd || (next.revisions || []).length) ? structuredClone(next) : null;
    listeners.forEach((fn) => fn());
  }

  const structureLocked = () => workbook?.structure === true;

  // True when the workbook's structure may change (sheets added, moved, renamed, copied, hidden, deleted).
  function guardStructure() {
    if (!structureLocked()) return true;
    say(WORKBOOK_BLOCKED);
    return false;
  }

  // A password the page has no digest for (an unknown algorithm) cannot lift a protection.
  const UNSUPPORTED = {
    algorithm: "지원하지 않는 암호 방식",
    spin: "지원하지 않는 암호 방식",
    encoding: "지원하지 않는 암호 방식",
    long: "암호가 너무 깁니다",
    time: "암호 확인이 오래 걸립니다",
  };

  host.__EASYOFFICE_SHEET_PROTECT__ = {
    FLAGS,
    FLAG_NAMES,
    DEFAULT_ALLOWED,
    BLOCKED,
    UNSUPPORTED,
    protectOf,
    isOn,
    allows,
    cellUnlocked,
    cellEditable,
    rangeEditable,
    canSelect,
    formulaHidden,
    lockedRangeAt,
    unlockRange: (sheet, name) => unlockedRanges.add(rangeKey(sheet, name)),
    forgetUnlocked: () => unlockedRanges.clear(),
    say,
    guardAction,
    guardCells,
    guardUnprotected,
    guardStructure,
    structureLocked,
    getWorkbook: workbookJson,
    setWorkbook,
    onWorkbookChange: (fn) => listeners.push(fn),
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
