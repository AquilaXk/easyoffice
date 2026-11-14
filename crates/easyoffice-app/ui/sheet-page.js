(function (host) {
  "use strict";

  // The page setup of the sheet window (PD-213): the 쪽 설정 dialog and the print area commands. The tab and item names are
  // those of the Korean spreadsheet help (쪽 설정: 쪽 / 여백 / 시트): 쪽 (용지 방향, 용지 크기, 배율 with 확대/축소
  // 배율 and 자동 맞춤 by 용지 너비 and 용지 높이), 여백 (위쪽, 아래쪽, 왼쪽, 오른쪽, 머리글, 꼬리말, 쪽 가운데에 배치), 시트 (인쇄
  // 영역, 인쇄 제목 with 반복할 행 and 반복할 열, 행/열 머리글, 눈금 선); the 머리글/바닥글 tab, 쪽 순서 and the other print
  // options have no model in the engine. The orientation cards and the margin grid are the reference editor's edit-paper
  // dialog frame (`doc-page-setup.js`). The setup is the engine's `page` JSON (`sheet_page_json.rs`): margins in thousandths
  // of an inch, rows and columns 1-based. Each change is one undo step over the layout store (`sheet-view-ui.js`).

  const MM_PER_MIL = 0.0254;
  const MAX_PERCENT = 400;
  const MIN_PERCENT = 10;
  const MAX_FIT_PAGES = 1000;
  const MAX_PRINT_RANGES = 64;
  const MAX_TITLE_ROWS = 256;
  const MAX_TITLE_COLS = 64;
  const MAX_MARGIN_MILS = 22000;
  const MAX_ROW = 1048576;
  const MAX_COL = 16384;
  // The paper the list offers: `pageSetup@paperSize` code, name, width and height in mm (upright).
  const PAPERS = [[9, "A4", 210, 297], [8, "A3", 297, 420], [12, "B4", 257, 364], [13, "B5", 182, 257], [1, "Letter", 215.9, 279.4], [5, "Legal", 215.9, 355.6]];
  const CUSTOM = "0";
  const DEFAULT_PAPER = 9;
  const NORMAL_MARGINS = { left: 700, right: 700, top: 750, bottom: 750, header: 300, footer: 300 };
  const MARGIN_KEYS = ["top", "bottom", "left", "right", "header", "footer"];
  const MSG_AREA = "인쇄 영역이 올바르지 않습니다 (예: A1:D10, F15)";
  const MSG_ROWS = "반복할 행이 올바르지 않습니다 (예: 1:2)";
  const MSG_COLS = "반복할 열이 올바르지 않습니다 (예: A:B)";
  const MSG_SCALE = "배율은 " + MIN_PERCENT + "에서 " + MAX_PERCENT + " 사이여야 합니다";
  const MSG_FIT = "맞출 쪽 수는 " + MAX_FIT_PAGES + " 이하여야 합니다";
  const MSG_MARGIN = "여백은 558.8mm 이하여야 합니다";

  const round1 = (v) => Math.round(v * 10) / 10;
  const milsToMm = (mils) => round1(mils * MM_PER_MIL);
  const mmToMils = (mm) => Math.round(mm / MM_PER_MIL);

  const refs = () => host.__EASYOFFICE_SHEET_REFS__;

  // ---- ranges as text ---------------------------------------------------------------------------

  // `A1:D10, F15` as normalised range texts, or null when one is not a range or there are too many.
  function parseRanges(text) {
    const out = [];
    for (const token of String(text ?? "").split(/[\s,;]+/).filter(Boolean)) {
      const [a, b = a, extra] = token.replaceAll("$", "").split(":");
      const p = refs().parseRef(a);
      const q = refs().parseRef(b);
      if (extra !== undefined || !p || !q) return null;
      const [r1, r2] = [Math.min(p.row, q.row), Math.max(p.row, q.row)];
      const [c1, c2] = [Math.min(p.col, q.col), Math.max(p.col, q.col)];
      out.push(r1 === r2 && c1 === c2 ? refs().refOf(r1, c1) : refs().refOf(r1, c1) + ":" + refs().refOf(r2, c2));
      if (out.length > MAX_PRINT_RANGES) return null;
    }
    return out;
  }

  // `1:2` or `3` as `[first, last]` (1-based) within `limit` lines and `max` of them, `[]` for empty text, null when wrong.
  function parseLines(text, kind) {
    const value = String(text ?? "").trim().replaceAll("$", "");
    if (value === "") return [];
    const [a, b = a, extra] = value.split(":");
    const toIndex = (part) => {
      if (kind === "row") return /^\d{1,7}$/.test(part) ? Number(part) : 0;
      return /^[A-Za-z]{1,3}$/.test(part) ? refs().colIndex(part.toUpperCase()) : 0;
    };
    const [first, last] = [toIndex(a), toIndex(b)];
    const limit = kind === "row" ? MAX_ROW : MAX_COL;
    const most = kind === "row" ? MAX_TITLE_ROWS : MAX_TITLE_COLS;
    const ok = extra === undefined && first >= 1 && last >= first && last <= limit && last - first + 1 <= most;
    return ok ? [first, last] : null;
  }

  const linesText = (run, kind) => {
    if (!Array.isArray(run) || run.length !== 2) return "";
    const name = (n) => (kind === "row" ? String(n) : refs().colName(n));
    return run[0] === run[1] ? name(run[0]) : name(run[0]) + ":" + name(run[1]);
  };

  // ---- the setup as the dialog's fields -----------------------------------------------------------

  const marginsOf = (page) => ({ ...NORMAL_MARGINS, ...(page?.margins ?? {}) });

  // The fields of a setup as the dialog shows them (millimetres, text).
  function toFields(page) {
    const m = marginsOf(page);
    const scale = page?.scale ?? { percent: 100 };
    return {
      paper: page?.paper === undefined ? DEFAULT_PAPER : page.paper,
      landscape: page?.landscape === true,
      margins: Object.fromEntries(MARGIN_KEYS.map((key) => [key, milsToMm(m[key])])),
      fit: "fitWidth" in scale,
      percent: scale.percent ?? 100,
      fitWidth: scale.fitWidth ?? 1,
      fitHeight: scale.fitHeight ?? 1,
      centerH: page?.centerH === true,
      centerV: page?.centerV === true,
      area: (page?.printArea ?? []).join(", "),
      rows: linesText(page?.titleRows, "row"),
      cols: linesText(page?.titleCols, "col"),
      grid: page?.grid === true,
      headings: page?.headings === true,
    };
  }

  // The setup of the fields, or `{ error }` for the first one that is wrong. A margin left as shown keeps its exact
  // thousandths of an inch (`original`), so pressing 확인 does not round a file's margins to a tenth of a millimetre.
  function fromFields(fields, original) {
    const area = parseRanges(fields.area);
    if (area === null) return { error: MSG_AREA };
    const rows = parseLines(fields.rows, "row");
    if (rows === null) return { error: MSG_ROWS };
    const cols = parseLines(fields.cols, "col");
    if (cols === null) return { error: MSG_COLS };
    const page = {};
    if (fields.landscape) page.landscape = true;
    if (Number(fields.paper) > 0) page.paper = Number(fields.paper);
    const shown = toFields(original).margins;
    const exact = marginsOf(original);
    const margins = {};
    for (const key of MARGIN_KEYS) {
      const mm = Number(fields.margins[key]);
      if (!Number.isFinite(mm) || mm < 0 || mmToMils(mm) > MAX_MARGIN_MILS) return { error: MSG_MARGIN };
      margins[key] = mm === shown[key] ? exact[key] : mmToMils(mm);
    }
    if (original?.margins || MARGIN_KEYS.some((key) => margins[key] !== NORMAL_MARGINS[key])) page.margins = margins;
    if (fields.fit) {
      const [width, height] = [Number(fields.fitWidth), Number(fields.fitHeight)];
      if (![width, height].every((n) => Number.isInteger(n) && n >= 0 && n <= MAX_FIT_PAGES)) return { error: MSG_FIT };
      page.scale = { fitWidth: width, fitHeight: height };
    } else {
      const percent = Number(fields.percent);
      if (!Number.isInteger(percent) || percent < MIN_PERCENT || percent > MAX_PERCENT) return { error: MSG_SCALE };
      if (percent !== 100) page.scale = { percent };
    }
    for (const [key, on] of [["centerH", fields.centerH], ["centerV", fields.centerV], ["grid", fields.grid], ["headings", fields.headings]]) {
      if (on) page[key] = true;
    }
    if (area.length > 0) page.printArea = area;
    if (rows.length === 2) page.titleRows = rows;
    if (cols.length === 2) page.titleCols = cols;
    return { page };
  }

  // The print area as 1-based rectangles `{ r1, c1, r2, c2 }`.
  function areaBoxes(page) {
    const out = [];
    for (const text of page?.printArea ?? []) {
      const [a, b = a] = text.split(":");
      const p = refs().parseRef(a);
      const q = refs().parseRef(b);
      if (p && q) out.push({ r1: p.row, c1: p.col, r2: q.row, c2: q.col });
    }
    return out;
  }

  // The paper's size in mm as printed: the list's, turned for a landscape page; A4 for a paper the list does not have.
  function paperMm(page) {
    const found = PAPERS.find(([code]) => code === (page?.paper ?? DEFAULT_PAPER)) ?? PAPERS[0];
    return page?.landscape ? { width: found[3], height: found[2] } : { width: found[2], height: found[3] };
  }

  // ---- dialog and commands ------------------------------------------------------------------------

  function createSheetPage(deps) {
    const { document, getRange, getActiveSheetName, enqueueWorkspaceHistoryOp, drainInFlightSheetWork, commitIfEditing, grid, refreshChrome, sheetHistory } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const byId = (id) => document.getElementById(id);
    let original = null;

    function say(text) {
      const fb = byId("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    // One undo step over the layout store: `change(name)` returns a message, or null to leave the sheet as it is.
    function step(change) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        const record = sheetHistory.begin({ parts: ["sheetLayout"] });
        const message = change(name);
        if (message === null) return false;
        sheetHistory.commit(record);
        grid.layoutChanged();
        refreshChrome();
        say(message);
        return true;
      });
    }

    // ---- the dialog

    function showPane(name) {
      for (const tab of document.querySelectorAll("#sheet-page-dialog [data-sp-tab]")) {
        const on = tab.dataset.spTab === name;
        tab.classList.toggle("active", on);
        tab.setAttribute("aria-selected", on ? "true" : "false");
        tab.tabIndex = on ? 0 : -1;
      }
      for (const pane of document.querySelectorAll("#sheet-page-dialog [data-sp-pane]")) pane.hidden = pane.dataset.spPane !== name;
    }

    const markOrientation = () => {
      for (const id of ["sp-portrait", "sp-landscape"]) byId(id)?.closest(".icon-radio-card")?.classList.toggle("selected", kit.checked(id));
    };

    // The scale's fields that do not apply are disabled, the other kind's value stays.
    function syncScale() {
      const fit = kit.checked("sp-scale-fit");
      byId("sp-percent").disabled = fit;
      byId("sp-fit-width").disabled = !fit;
      byId("sp-fit-height").disabled = !fit;
    }

    function populate(page) {
      const f = toFields(page);
      const known = PAPERS.some(([code]) => code === f.paper);
      kit.setSelect("sp-paper", known ? String(f.paper) : CUSTOM);
      byId("sp-paper").dataset.exact = String(f.paper);
      kit.setChecked(f.landscape ? "sp-landscape" : "sp-portrait", true);
      markOrientation();
      for (const key of MARGIN_KEYS) kit.setVal("sp-" + key, f.margins[key].toFixed(1));
      kit.setChecked(f.fit ? "sp-scale-fit" : "sp-scale-percent", true);
      kit.setVal("sp-percent", String(f.percent));
      kit.setVal("sp-fit-width", String(f.fitWidth));
      kit.setVal("sp-fit-height", String(f.fitHeight));
      kit.setChecked("sp-center-h", f.centerH);
      kit.setChecked("sp-center-v", f.centerV);
      kit.setVal("sp-print-area", f.area);
      kit.setVal("sp-title-rows", f.rows);
      kit.setVal("sp-title-cols", f.cols);
      kit.setChecked("sp-grid", f.grid);
      kit.setChecked("sp-headings", f.headings);
      syncScale();
    }

    function readFields() {
      const paper = kit.val("sp-paper");
      return {
        // A paper the list does not have (a code from a file) stays as it was.
        paper: paper === CUSTOM ? Number(byId("sp-paper").dataset.exact) || 0 : Number(paper),
        landscape: kit.checked("sp-landscape"),
        margins: Object.fromEntries(MARGIN_KEYS.map((key) => [key, Number.parseFloat(kit.val("sp-" + key))])),
        fit: kit.checked("sp-scale-fit"),
        percent: kit.val("sp-percent"),
        fitWidth: kit.val("sp-fit-width"),
        fitHeight: kit.val("sp-fit-height"),
        centerH: kit.checked("sp-center-h"),
        centerV: kit.checked("sp-center-v"),
        area: kit.val("sp-print-area"),
        rows: kit.val("sp-title-rows"),
        cols: kit.val("sp-title-cols"),
        grid: kit.checked("sp-grid"),
        headings: kit.checked("sp-headings"),
      };
    }

    function open() {
      original = layoutApi.pageOf(getActiveSheetName());
      populate(original);
      kit.note("sp-status", "");
      showPane("paper");
      kit.openDialog("sheet-page-dialog");
      byId("sp-paper")?.focus?.();
      return true;
    }

    // 확인: a field that is wrong keeps the dialog open with the reason; else the setup is the sheet's, as one undo step.
    function apply() {
      const result = fromFields(readFields(), original);
      if (result.error) {
        kit.note("sp-status", result.error);
        return Promise.resolve(false);
      }
      kit.closeDialog("sheet-page-dialog");
      return step((name) => {
        layoutApi.setPage(name, result.page);
        return "쪽 설정 적용";
      });
    }

    // 인쇄 영역 설정: the selection is the print area.
    function setPrintArea() {
      return step((name) => {
        const rg = getRange();
        const ref = rg.r1 === rg.r2 && rg.c1 === rg.c2 ? refs().refOf(rg.r1, rg.c1) : refs().refOf(rg.r1, rg.c1) + ":" + refs().refOf(rg.r2, rg.c2);
        layoutApi.setPage(name, { ...(layoutApi.pageOf(name) ?? {}), printArea: [ref] });
        return "인쇄 영역 설정 " + ref;
      });
    }

    function clearPrintArea() {
      return step((name) => {
        const { printArea, ...rest } = layoutApi.pageOf(name) ?? {};
        if (!printArea) return null;
        layoutApi.setPage(name, rest);
        return "인쇄 영역 해제";
      });
    }

    function bind() {
      byId("btn-sheet-page-apply")?.addEventListener("click", () => void apply());
      for (const tab of document.querySelectorAll("#sheet-page-dialog [data-sp-tab]")) tab.addEventListener("click", () => showPane(tab.dataset.spTab));
      for (const id of ["sp-portrait", "sp-landscape"]) byId(id)?.addEventListener("change", markOrientation);
      for (const id of ["sp-scale-percent", "sp-scale-fit"]) byId(id)?.addEventListener("change", syncScale);
    }

    const actions = {
      "page-setup": () => Promise.resolve(open()),
      "print-area-set": setPrintArea,
      "print-area-clear": clearPrintArea,
    };
    return { actions, open, apply, bind, setPrintArea, clearPrintArea };
  }

  host.__EASYOFFICE_SHEET_PAGE_CORE__ = { PAPERS, NORMAL_MARGINS, MARGIN_KEYS, milsToMm, mmToMils, parseRanges, parseLines, linesText, toFields, fromFields, areaBoxes, paperMm, MAX_FIT_PAGES, MAX_PERCENT, MIN_PERCENT };
  host.__EASYOFFICE_CREATE_SHEET_PAGE__ = createSheetPage;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
