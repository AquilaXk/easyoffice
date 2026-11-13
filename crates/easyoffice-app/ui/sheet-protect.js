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
  // The message of the window an edit of a protected sheet opens: the spreadsheet programs' warning, whole. A window that is
  // open already is not opened again.
  const BLOCKED = "변경하려는 셀 또는 차트가 보호된 시트에 있습니다. 변경하려면 시트의 보호를 해제하세요. 암호를 입력해야 할 수도 있습니다.";
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

  // Opens the message window of a refused edit (a single 확인 button, the confirmation dialog's frame); nothing while it is open.
  function alertBlocked() {
    const doc = host.document;
    const dialog = doc?.getElementById?.("sheet-protect-alert-dialog");
    if (!dialog) return say(BLOCKED);
    if (dialog.open) return undefined;
    const message = doc.getElementById("sheet-protect-alert-message");
    if (message) message.textContent = BLOCKED;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    doc.getElementById("btn-sheet-protect-alert-ok")?.focus?.();
    return undefined;
  }

  // True when the sheet allows `flag`; else the message window says so.
  function guardAction(sheet, flag) {
    if (allows(sheet, flag)) return true;
    alertBlocked();
    return false;
  }

  // True when every cell of `rg` may be changed.
  function guardCells(sheet, rg, styleAt) {
    if (rangeEditable(sheet, rg, styleAt)) return true;
    alertBlocked();
    return false;
  }

  // True when the sheet is not protected (a change of the protection or of a cell's lock needs that).
  function guardUnprotected(sheet) {
    if (!isOn(sheet)) return true;
    alertBlocked();
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

  // The entry without its formula when the protected sheet hides it: what a fill, a copy or a search may see.
  function visibleEntry(sheet, row, col, entry) {
    return entry?.formula && formulaHidden(sheet, row, col, entry.style) ? { ...entry, formula: "" } : entry;
  }

  // ---- what an undo, a redo and a save may do to the protection ----------------------------------------------
  // The protection `current` as it is, its allowed ranges at the place `then` has them (a row or column edit moves them);
  // a range the snapshot lacks stays, and one only the snapshot has is not brought back.
  function withRangesOf(current, then) {
    const out = structuredClone(current);
    const placed = new Map((then?.ranges || []).map((range) => [range.name, range.sqref]));
    for (const range of out.ranges || []) if (placed.has(range.name)) range.sqref = structuredClone(placed.get(range.name));
    return out;
  }

  // The layout of `snap` with the protection of every sheet in `currentNames` and in `snap` as it is now.
  function keepProtection(snap, currentNames) {
    const out = structuredClone(snap.sheetLayout);
    const known = new Set(snap.sheetOrder || Object.keys(snap.sheets || {}));
    const now = layout().getAll();
    for (const name of currentNames.filter((n) => known.has(n))) {
      const entry = { ...(out[name] || {}) };
      if (now[name]?.protect) entry.protect = withRangesOf(now[name].protect, out[name]?.protect);
      else delete entry.protect;
      if (Object.keys(entry).length > 0) out[name] = entry;
      else delete out[name];
    }
    return out;
  }

  // The cells of a step recorded before a row or column edit, set where that edit puts them: a cell the edit
  // deleted is gone. `edit`: { axis, op, at, count } is the edit that leads from the step's state to the current one.
  function shiftedCells(cells, edit) {
    const out = {};
    const { refOf } = host.__EASYOFFICE_SHEET_REFS__;
    for (const [ref, entry] of Object.entries(cells)) {
      const p = refs().parseRef(ref);
      if (!p) continue;
      const index = edit.axis === "row" ? p.row : p.col;
      let moved = index;
      if (edit.op === "insert" && index >= edit.at) moved = index + edit.count;
      if (edit.op === "delete") {
        if (index >= edit.at + edit.count) moved = index - edit.count;
        else if (index >= edit.at) continue;
      }
      out[edit.axis === "row" ? refOf(moved, p.col) : refOf(p.row, moved)] = entry;
    }
    return out;
  }

  // The edit that leads back from the state `edit` led to.
  const inverseEdit = (edit) => (edit ? { ...edit, op: edit.op === "insert" ? "delete" : "insert" } : undefined);

  // Whether restoring `snap` would change a locked cell of a sheet that is protected now. Formula cells compare by
  // their text, because their value follows the cells they read; the step of a row or column edit is compared where
  // the edit puts its cells, and there a formula the edit rewrote is the same formula.
  function restoreWouldChange(snap, store) {
    if (!snap.sheets) return false;
    const raw = (e) => e?.formula || e?.value || "";
    const edit = snap.structureEdit;
    for (const name of Object.keys(store)) {
      if (!isOn(name) || !snap.sheets[name]) continue;
      const moved = edit?.sheet === name;
      const now = store[name] || {};
      const then = moved ? shiftedCells(snap.sheets[name], edit) : snap.sheets[name];
      for (const ref of new Set([...Object.keys(now), ...Object.keys(then)])) {
        if (raw(now[ref]) === raw(then[ref]) || (moved && now[ref]?.formula && then[ref]?.formula)) continue;
        const at = refs().parseRef(ref);
        if (at && !cellEditable(name, at.row, at.col, now[ref]?.style)) return true;
      }
    }
    return false;
  }

  host.__EASYOFFICE_SHEET_PROTECT__ = {
    visibleEntry,
    keepProtection,
    restoreWouldChange,
    inverseEdit,
    alertBlocked,
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
