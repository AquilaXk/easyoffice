(function (host) {
  "use strict";

  // Bounded CSV / TSV / XLSX table import for BASE: delegates container/delimiter parsing to the
  // existing Rust `convert_document_format` (`action: "decode"`) IPC codec (`easyoffice-formats`)
  // and enforces strict byte/row/col/character bounds and CSV formula neutralization.

  const MAX_IMPORT_BYTES = 10 * 1024 * 1024;
  const MAX_IMPORT_RECORDS = 10000;
  const MAX_IMPORT_FIELDS = 64;
  const MAX_HEADER_CHARS = 128;
  const MAX_CELL_CHARS = 8192;
  const MAX_SHEETS_PER_FILE = 64;
  const MAX_TABLES = 32;
  const SUPPORTED_EXTS = new Set(["csv", "tsv", "xlsx"]);
  const FORMULA_PREFIX_RE = /^[=+\-@\t\r]/;
  const ISO_DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
  const CHECKBOX_TRUE = new Set(["true", "예"]);
  const CHECKBOX_FALSE = new Set(["false", "아니오"]);

  const M = () => host.__EASYOFFICE_BASE_MODEL__;
  const U = () => host.__EASYOFFICE_SHEET_UTILS__;
  const fail = (error) => ({ ok: false, error });

  function fileExtOf(name) {
    const raw = String(name || "").trim();
    const dot = raw.lastIndexOf(".");
    return dot > 0 && dot < raw.length - 1 ? raw.slice(dot + 1).toLowerCase() : "";
  }

  function fileBaseName(name) {
    const raw = String(name || "").trim().replace(/^.*[\\/]/, "");
    const dot = raw.lastIndexOf(".");
    return (dot > 0 ? raw.slice(0, dot) : raw).trim().slice(0, 128) || "가져온 테이블";
  }

  function uint8ToBase64(bytes) {
    let bin = "";
    const chunkSize = 0x8000;
    for (let i = 0; i < bytes.length; i += chunkSize) {
      bin += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
    }
    return btoa(bin);
  }

  function hasControlOrSurrogateDanger(text) {
    return String(text ?? "").includes("\u0000");
  }

  function neutralizeFormulaText(raw) {
    const s = String(raw ?? "");
    if (!s) return s;
    const trimmed = s.replace(/^[ \t\r]+/, "");
    if (!FORMULA_PREFIX_RE.test(s) && !/^[=+\-@]/.test(trimmed)) return s;
    const model = M();
    if (model?.parseStrictNumber && model.parseStrictNumber(s) !== null) return s;
    return "'" + s;
  }

  function validateRawCellText(raw) {
    const s = raw == null ? "" : String(raw);
    if (hasControlOrSurrogateDanger(s)) return fail("보안상 허용되지 않는 널(NUL) 문자가 포함되어 있습니다.");
    if (s.length > MAX_CELL_CHARS) return fail(`셀 값이 최대 글자 수(${MAX_CELL_CHARS}자)를 초과했습니다.`);
    return { ok: true, text: s };
  }

  function sanitizeHeaderCell(raw, index, usedNames) {
    const check = validateRawCellText(raw);
    if (!check.ok) return check;
    let base = check.text.trim();
    if (!base) base = `필드 ${index + 1}`;
    base = neutralizeFormulaText(base);
    if (base.length > MAX_HEADER_CHARS) return fail(`필드 이름 '${base.slice(0, 24)}…'이 최대 길이(${MAX_HEADER_CHARS}자)를 초과했습니다.`);
    let candidate = base;
    let suffix = 2;
    while (usedNames.has(candidate.toLowerCase())) {
      const tag = ` (${suffix++})`;
      if (base.length + tag.length > MAX_HEADER_CHARS) {
        candidate = base.slice(0, MAX_HEADER_CHARS - tag.length) + tag;
      } else {
        candidate = base + tag;
      }
    }
    usedNames.add(candidate.toLowerCase());
    return { ok: true, name: candidate };
  }

  function inferColumnType(samples) {
    const model = M();
    let seen = false;
    let allBool = true;
    let allNum = true;
    let allDate = true;
    let allUrl = true;
    for (const item of samples) {
      const raw = String(item?.raw ?? "").trim();
      if (!raw) continue;
      seen = true;
      const low = raw.toLowerCase();
      if (item?.t !== "b" && !CHECKBOX_TRUE.has(low) && !CHECKBOX_FALSE.has(low)) allBool = false;
      if (!model?.parseStrictNumber || model.parseStrictNumber(raw) === null) allNum = false;
      if (!ISO_DATE_RE.test(raw) || !model?.parseIsoDateUtcMs || model.parseIsoDateUtcMs(raw) === null) allDate = false;
      if (!model?.normalizeUrlValue || !/^https?:\/\//i.test(raw) || !model.normalizeUrlValue(raw).ok) allUrl = false;
    }
    if (!seen) return "text";
    if (allBool) return "checkbox";
    if (allNum) return "number";
    if (allDate) return "date";
    if (allUrl) return "url";
    return "text";
  }

  function cellCoordFromEntry(cell) {
    if (Number.isInteger(cell?.r) && Number.isInteger(cell?.c)) {
      return { row: cell.r + 1, col: cell.c };
    }
    const utils = U();
    if (utils?.parseStrictA1Coord) {
      const parsed = utils.parseStrictA1Coord(String(cell?.ref || ""));
      if (!parsed.ok) return null;
      return { row: parsed.row, col: parsed.col };
    }
    const parts = utils?.parseCellRefParts?.(String(cell?.ref || "")) || { row: 1, col: 0 };
    return parts;
  }

  function sheetToTableGrid(sheet, options = {}) {
    if (!sheet || !Array.isArray(sheet.cells)) return fail("가져올 시트 데이터가 올바르지 않습니다.");
    const useHeader = options.hasHeader !== false;
    let maxRow = 0;
    let maxCol = -1;
    const cellEntries = [];
    for (const cell of sheet.cells) {
      const coord = cellCoordFromEntry(cell);
      if (!coord || coord.row < 1 || coord.col < 0) return fail("가져올 셀 좌표가 올바르지 않습니다.");
      const valCheck = validateRawCellText(cell?.raw ?? "");
      if (!valCheck.ok) return valCheck;
      if (coord.row > maxRow) maxRow = coord.row;
      if (coord.col > maxCol) maxCol = coord.col;
      if (maxCol + 1 > MAX_IMPORT_FIELDS) return fail(`가져올 수 있는 최대 필드 수(${MAX_IMPORT_FIELDS}개)를 초과했습니다.`);
      if (maxRow - (useHeader ? 1 : 0) > MAX_IMPORT_RECORDS) {
        return fail(`가져올 수 있는 최대 레코드 수(${MAX_IMPORT_RECORDS}건)를 초과했습니다.`);
      }
      if (valCheck.text !== "") {
        cellEntries.push({ row: coord.row, col: coord.col, raw: valCheck.text, t: cell?.t || "s" });
      }
    }
    if (cellEntries.length === 0) return fail("가져올 데이터가 비어 있습니다.");

    const colCount = maxCol + 1;
    const rowMap = new Map();
    for (const entry of cellEntries) {
      let rowArr = rowMap.get(entry.row);
      if (!rowArr) {
        rowArr = new Array(colCount).fill(null);
        rowMap.set(entry.row, rowArr);
      }
      rowArr[entry.col] = entry;
    }

    const usedNames = new Set();
    const headers = [];
    const headerRowEntries = useHeader ? (rowMap.get(1) || []) : [];
    for (let c = 0; c < colCount; c += 1) {
      const rawName = useHeader ? (headerRowEntries[c]?.raw ?? "") : `필드 ${c + 1}`;
      const res = sanitizeHeaderCell(rawName, c, usedNames);
      if (!res.ok) return res;
      headers.push(res.name);
    }

    const startRow = useHeader ? 2 : 1;
    const dataRows = [];
    const samplesByCol = Array.from({ length: colCount }, () => []);
    for (let r = startRow; r <= maxRow; r += 1) {
      const rowEntries = rowMap.get(r);
      if (!rowEntries) continue;
      let anyNonEmpty = false;
      const rowCells = new Array(colCount);
      for (let c = 0; c < colCount; c += 1) {
        const item = rowEntries[c];
        const raw = item?.raw ?? "";
        if (raw !== "") anyNonEmpty = true;
        rowCells[c] = item || { raw: "", t: "s" };
        if (raw !== "") samplesByCol[c].push(rowCells[c]);
      }
      if (anyNonEmpty) dataRows.push(rowCells);
    }
    if (dataRows.length > MAX_IMPORT_RECORDS) {
      return fail(`가져올 수 있는 최대 레코드 수(${MAX_IMPORT_RECORDS}건)를 초과했습니다.`);
    }

    const fieldSpecs = headers.map((name, c) => ({
      name,
      type: inferColumnType(samplesByCol[c]),
    }));

    const rows = dataRows.map((rowCells) => rowCells.map((item, c) => {
      const ft = fieldSpecs[c].type;
      const raw = String(item?.raw ?? "");
      if (ft === "checkbox") {
        const low = raw.trim().toLowerCase();
        return raw.trim() === "" ? false : (item?.t === "b" ? CHECKBOX_TRUE.has(low) || low === "1" : CHECKBOX_TRUE.has(low));
      }
      if (ft === "number") return raw.trim() === "" ? null : raw.trim();
      if (ft === "date" || ft === "url") return raw.trim();
      const safe = neutralizeFormulaText(raw);
      return safe.length > MAX_CELL_CHARS ? safe.slice(0, MAX_CELL_CHARS) : safe;
    }));

    return { ok: true, sheetName: String(sheet.name || "가져온 테이블"), fields: fieldSpecs, rows };
  }

  function decodeRawBytesPrecheck(fileName, bytes) {
    const ext = fileExtOf(fileName);
    if (!SUPPORTED_EXTS.has(ext)) return fail("지원하지 않는 파일 형식입니다. CSV, TSV 또는 XLSX 파일을 선택해 주세요.");
    if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0) return fail("가져올 파일이 비어 있습니다.");
    if (bytes.byteLength > MAX_IMPORT_BYTES) return fail("가져올 파일 크기가 상한(10 MiB)을 초과했습니다.");
    if (ext === "csv" || ext === "tsv") {
      try {
        const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
        if (hasControlOrSurrogateDanger(text)) return fail("보안상 허용되지 않는 널(NUL) 문자가 포함되어 있습니다.");
      } catch {
        return fail("올바른 UTF-8 인코딩 텍스트 파일이 아닙니다.");
      }
    }
    return { ok: true, ext };
  }

  async function decodeImportFile(fileName, bytes, invokeIpc) {
    const pre = decodeRawBytesPrecheck(fileName, bytes);
    if (!pre.ok) return pre;
    if (typeof invokeIpc !== "function") return fail("백엔드 디코더를 사용할 수 없습니다.");
    let reply;
    try {
      reply = await invokeIpc("convert_document_format", JSON.stringify({
        action: "decode",
        fileName: String(fileName || ""),
        sourceBase64: uint8ToBase64(bytes),
        maxRows: MAX_IMPORT_RECORDS + 2,
        maxCols: MAX_IMPORT_FIELDS + 1,
      }));
    } catch (err) {
      return fail(err?.message || String(err) || "파일을 해독하지 못했습니다.");
    }
    const warnings = Array.isArray(reply?.warnings) ? reply.warnings : [];
    if (warnings.some((w) => {
      const s = String(w);
      return s.includes("편집기 격자(최대") || (s.includes("셀 ") && s.includes("가져오지 않았습니다"));
    })) {
      return fail(`파일 크기 또는 행/열 상한(${MAX_IMPORT_RECORDS}건 · ${MAX_IMPORT_FIELDS}열)을 초과했습니다.`);
    }
    const sheets = Array.isArray(reply?.sheets) ? reply.sheets : [];
    if (sheets.length === 0) return fail("파일에서 표 시트를 찾지 못했습니다.");
    if (sheets.length > MAX_SHEETS_PER_FILE) return fail(`시트 수가 최대 상한(${MAX_SHEETS_PER_FILE}개)을 초과했습니다.`);
    return { ok: true, ext: pre.ext, sheets };
  }

  function applyImportToState(state, grid, options = {}) {
    const model = M();
    if (!model || !state || !Array.isArray(state.tables)) return fail("데이터베이스 모델을 사용할 수 없습니다.");
    if (!grid?.ok && (!Array.isArray(grid?.fields) || !Array.isArray(grid?.rows))) return fail(grid?.error || "가져올 데이터가 올바르지 않습니다.");
    if (grid.fields.length === 0 || grid.fields.length > MAX_IMPORT_FIELDS) return fail(`필드 수는 1~${MAX_IMPORT_FIELDS}개여야 합니다.`);
    if (grid.rows.length > MAX_IMPORT_RECORDS) return fail(`레코드 수는 최대 ${MAX_IMPORT_RECORDS}건까지 허용됩니다.`);

    const mode = options.mode === "append" || options.mode === "replace" ? options.mode : "new";
    if (mode === "new") {
      if (state.tables.length >= MAX_TABLES) return fail(`최대 테이블 수(${MAX_TABLES}개)를 초과했습니다.`);
      let fSeq = Number(state.nextFieldSeq) || 1;
      const fields = grid.fields.map((spec) => ({ id: `fld-${fSeq++}`, name: spec.name, type: spec.type, validation: {} }));
      let rSeq = Number(state.nextRecordSeq) || 1;
      const records = [];
      for (const row of grid.rows) {
        const values = {};
        for (let c = 0; c < fields.length; c += 1) {
          const fld = fields[c];
          const vRes = model.validateCellValue(fld, row[c], state);
          if (!vRes.ok) return vRes;
          values[fld.id] = vRes.value;
        }
        records.push({ id: `rec-${rSeq++}`, values });
      }
      let tSeq = Number(state.nextTableSeq) || 1;
      const name = neutralizeFormulaText(String(options.tableName || grid.sheetName || "가져온 테이블").trim()).slice(0, MAX_HEADER_CHARS) || `테이블 ${tSeq}`;
      const newTbl = {
        id: `tbl-${tSeq++}`,
        name,
        view: "grid",
        kanbanFieldId: "",
        calendarFieldId: fields.find((f) => f.type === "date")?.id || "",
        calendarMonth: new Date().toISOString().slice(0, 7),
        filters: [],
        filterConjunction: "and",
        sorts: [],
        groupByFieldId: "",
        collapsedGroups: [],
        fields,
        records,
      };
      state.tables.push(newTbl);
      state.activeTableId = newTbl.id;
      state.nextTableSeq = tSeq;
      state.nextFieldSeq = fSeq;
      state.nextRecordSeq = rSeq;
      return { ok: true, table: newTbl, importedCount: records.length };
    }

    const activeTbl = model.getTable(state, options.tableId || state.activeTableId);
    if (!activeTbl) return fail("활성 테이블이 없습니다.");

    if (mode === "replace") {
      let fSeq = Number(state.nextFieldSeq) || 1;
      const fields = grid.fields.map((spec) => ({ id: `fld-${fSeq++}`, name: spec.name, type: spec.type, validation: {} }));
      let rSeq = Number(state.nextRecordSeq) || 1;
      const records = [];
      for (const row of grid.rows) {
        const values = {};
        for (let c = 0; c < fields.length; c += 1) {
          const vRes = model.validateCellValue(fields[c], row[c], state);
          if (!vRes.ok) return vRes;
          values[fields[c].id] = vRes.value;
        }
        records.push({ id: `rec-${rSeq++}`, values });
      }
      activeTbl.fields = fields;
      activeTbl.records = records;
      activeTbl.view = "grid";
      activeTbl.filters = [];
      activeTbl.sorts = [];
      activeTbl.groupByFieldId = "";
      activeTbl.collapsedGroups = [];
      activeTbl.kanbanFieldId = "";
      activeTbl.calendarFieldId = fields.find((f) => f.type === "date")?.id || "";
      state.nextFieldSeq = fSeq;
      state.nextRecordSeq = rSeq;
      return { ok: true, table: activeTbl, importedCount: records.length };
    }

    // mode === "append"
    if (activeTbl.records.length + grid.rows.length > MAX_IMPORT_RECORDS) {
      return fail(`기존 레코드와 합쳐 최대 레코드 수(${MAX_IMPORT_RECORDS}건)를 초과합니다.`);
    }
    const writable = activeTbl.fields.filter((f) => f.type !== "formula");
    const byName = new Map(writable.map((f) => [String(f.name || "").trim().toLowerCase(), f]));
    const useHeader = options.hasHeader !== false;
    const mapping = grid.fields.map((spec, idx) => (useHeader ? byName.get(spec.name.trim().toLowerCase()) || null : writable[idx] || null));
    if (mapping.every((m) => m === null)) {
      return fail("현재 테이블의 필드와 일치하는 열이 없습니다. '새 테이블로 가져오기'를 선택해 주세요.");
    }
    const idx = model.buildStateIndex ? model.buildStateIndex(state) : undefined;
    let rSeq = Number(state.nextRecordSeq) || 1;
    const newRecords = [];
    for (const row of grid.rows) {
      const rawMap = {};
      for (let c = 0; c < mapping.length; c += 1) {
        const fld = mapping[c];
        if (!fld) continue;
        rawMap[fld.id] = row[c];
      }
      const clean = {};
      for (const fld of activeTbl.fields) {
        if (fld.type === "formula") continue;
        const def = fld.type === "checkbox" ? false : fld.type === "multiSelect" ? [] : "";
        const val = Object.hasOwn(rawMap, fld.id) ? rawMap[fld.id] : def;
        const vRes = model.validateCellValue(fld, val, state, idx);
        if (!vRes.ok) return vRes;
        clean[fld.id] = vRes.value;
      }
      newRecords.push({ id: `rec-${rSeq++}`, values: clean });
    }
    activeTbl.records.push(...newRecords);
    state.nextRecordSeq = rSeq;
    return { ok: true, table: activeTbl, importedCount: newRecords.length };
  }

  function createBaseImportController(studioApi, deps = {}) {
    const doc = deps.document || host.document;
    const getInvoke = () => deps.invokeIpc || host.__EASYOFFICE_INVOKE_BACKEND__;
    const setFeedback = typeof deps.setFeedback === "function" ? deps.setFeedback : () => {};
    let decoded = null;
    let selectedFileName = "";
    let dlg = null;

    const byId = (id) => doc?.getElementById?.(id) || null;
    const el = (tag, cls, text) => {
      const node = doc.createElement(tag);
      if (cls) node.className = cls;
      if (text !== undefined) node.textContent = text;
      return node;
    };
    const appendAll = (parent, ...kids) => {
      for (const k of kids) if (k) parent.appendChild(k);
      return parent;
    };

    function closeDlg() {
      if (!dlg) return;
      if (typeof dlg.close === "function" && dlg.open) dlg.close();
      else dlg.removeAttribute?.("open");
    }

    function openDlg() {
      if (!dlg) ensureDialog();
      if (!dlg) return;
      decoded = null;
      selectedFileName = "";
      const fileInp = byId("base-import-file");
      if (fileInp) fileInp.value = "";
      const sheetSel = byId("base-import-sheet");
      if (sheetSel) { sheetSel.replaceChildren(); sheetSel.hidden = true; host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(sheetSel); }
      const sheetRow = byId("base-import-sheet-row");
      if (sheetRow) sheetRow.hidden = true;
      const modeSel = byId("base-import-mode");
      if (modeSel) { modeSel.value = "new"; host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(modeSel); }
      const tblInp = byId("base-import-table-name");
      if (tblInp) tblInp.value = "";
      const nameRow = byId("base-import-name-row");
      if (nameRow) nameRow.hidden = false;
      const hdrChk = byId("base-import-header");
      if (hdrChk) hdrChk.checked = true;
      const prev = byId("base-import-preview");
      if (prev) prev.textContent = "CSV, TSV, XLSX 파일(최대 10 MiB · 10,000건 · 64열)을 선택해 주세요.";
      const status = byId("base-import-status");
      if (status) status.textContent = "";
      const applyBtn = byId("btn-base-import-apply");
      if (applyBtn) applyBtn.disabled = true;
      host.__EASYOFFICE_SHELL_CHROME__?.enableDialogDrag?.(dlg);
      if (typeof dlg.showModal === "function" && !dlg.open) dlg.showModal();
      else dlg.setAttribute?.("open", "");
    }

    function updatePreview() {
      const status = byId("base-import-status");
      const prev = byId("base-import-preview");
      const applyBtn = byId("btn-base-import-apply");
      const sheetSel = byId("base-import-sheet");
      const hdrChk = byId("base-import-header");
      if (!decoded?.sheets?.length) {
        if (applyBtn) applyBtn.disabled = true;
        return null;
      }
      const idx = Math.max(0, Math.min(decoded.sheets.length - 1, Number(sheetSel?.value) || 0));
      const grid = sheetToTableGrid(decoded.sheets[idx], { hasHeader: hdrChk?.checked !== false });
      if (!grid.ok) {
        if (status) status.textContent = grid.error;
        if (prev) prev.textContent = "";
        if (applyBtn) applyBtn.disabled = true;
        return null;
      }
      if (status) status.textContent = "";
      if (prev) {
        const typesSummary = grid.fields.slice(0, 5).map((f) => `${f.name}(${M()?.FIELD_TYPE_LABELS?.[f.type] || f.type})`).join(", ");
        prev.textContent = `${grid.fields.length}열 · ${grid.rows.length}건 준비됨 (${typesSummary}${grid.fields.length > 5 ? " …" : ""})`;
      }
      if (applyBtn) applyBtn.disabled = false;
      return grid;
    }

    async function handleFileSelect(ev) {
      const file = ev?.target?.files?.[0];
      const status = byId("base-import-status");
      const applyBtn = byId("btn-base-import-apply");
      decoded = null;
      if (!file) { if (applyBtn) applyBtn.disabled = true; return; }
      selectedFileName = file.name || "";
      if ((file.size || 0) > MAX_IMPORT_BYTES) {
        if (status) status.textContent = "가져올 파일 크기가 상한(10 MiB)을 초과했습니다.";
        if (applyBtn) applyBtn.disabled = true;
        return;
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      const res = await decodeImportFile(selectedFileName, bytes, getInvoke());
      if (!res.ok) {
        if (status) status.textContent = res.error;
        if (applyBtn) applyBtn.disabled = true;
        return;
      }
      decoded = res;
      const sheetSel = byId("base-import-sheet");
      const sheetRow = byId("base-import-sheet-row");
      if (sheetSel && sheetRow) {
        sheetSel.replaceChildren();
        res.sheets.forEach((s, i) => {
          const opt = el("option", "", `${s.name || `시트 ${i + 1}`}`);
          opt.value = String(i);
          sheetSel.appendChild(opt);
        });
        sheetSel.value = "0";
        sheetRow.hidden = res.sheets.length <= 1;
        sheetSel.hidden = res.sheets.length <= 1;
        host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(sheetSel);
      }
      const tblInp = byId("base-import-table-name");
      if (tblInp) tblInp.value = res.sheets[0]?.name || fileBaseName(selectedFileName);
      updatePreview();
    }

    function applyImport() {
      const grid = updatePreview();
      if (!grid) return;
      const mode = byId("base-import-mode")?.value || "new";
      const tableName = byId("base-import-table-name")?.value || grid.sheetName;
      const hasHeader = byId("base-import-header")?.checked !== false;
      const res = studioApi.runStateMutation((st) => applyImportToState(st, grid, { mode, tableName, hasHeader }));
      if (!res.ok) {
        const status = byId("base-import-status");
        if (status) status.textContent = res.error;
        return;
      }
      closeDlg();
      setFeedback(`가져오기 완료: ${res.importedCount}건 (${res.table?.name || ""})`);
    }

    function ensureDialog() {
      if (!doc?.body) return null;
      byId("base-import-dialog")?.remove?.();
      dlg = el("dialog", "tool-dialog base-field-dialog base-import-dialog");
      dlg.id = "base-import-dialog";
      dlg.setAttribute("aria-labelledby", "base-import-dialog-title");
      const wrap = el("div", "dialog-wrap");
      const titleBar = el("div", "dialog-title");
      const titleSpan = el("span", "", "표 데이터 가져오기");
      titleSpan.id = "base-import-dialog-title";
      const closeX = el("button", "dialog-close", "×");
      closeX.type = "button";
      closeX.setAttribute("aria-label", "닫기");
      closeX.addEventListener("click", closeDlg);
      appendAll(titleBar, titleSpan, closeX);

      const mkRow = (labelText, inputEl, rowId) => {
        const row = el("div", "dialog-row");
        if (rowId) row.id = rowId;
        const lbl = el("label", "dialog-label", labelText);
        if (inputEl.id) lbl.setAttribute("for", inputEl.id);
        return appendAll(row, lbl, inputEl);
      };

      const fileInp = el("input", "dialog-text-input base-dlg-input");
      fileInp.id = "base-import-file";
      fileInp.type = "file";
      fileInp.accept = ".csv,.tsv,.xlsx";
      fileInp.addEventListener("change", (ev) => { void handleFileSelect(ev); });

      const sheetSel = el("select", "sb-select sb-combo base-dlg-select");
      sheetSel.id = "base-import-sheet";
      sheetSel.addEventListener("change", updatePreview);
      const sheetRow = mkRow("시트 선택", sheetSel, "base-import-sheet-row");
      sheetRow.hidden = true;

      const modeSel = el("select", "sb-select sb-combo base-dlg-select");
      modeSel.id = "base-import-mode";
      for (const [val, txt] of [["new", "새 테이블로 가져오기"], ["append", "현재 테이블 끝에 추가"], ["replace", "현재 테이블 내용 바꾸기"]]) {
        const o = el("option", "", txt);
        o.value = val;
        modeSel.appendChild(o);
      }

      const nameInp = el("input", "dialog-text-input base-dlg-input");
      nameInp.id = "base-import-table-name";
      nameInp.type = "text";
      const nameRow = mkRow("테이블 이름", nameInp, "base-import-name-row");
      modeSel.addEventListener("change", () => { nameRow.hidden = modeSel.value !== "new"; });

      const hdrChk = el("input", "");
      hdrChk.id = "base-import-header";
      hdrChk.type = "checkbox";
      hdrChk.checked = true;
      hdrChk.addEventListener("change", updatePreview);
      const hdrLbl = el("label", "dialog-checkbox");
      hdrLbl.setAttribute("for", "base-import-header");
      appendAll(hdrLbl, hdrChk, el("span", "", " 첫 행을 필드 이름으로 사용"));

      const preview = el("p", "base-import-preview", "");
      preview.id = "base-import-preview";
      const status = el("p", "base-import-status", "");
      status.id = "base-import-status";

      const body = appendAll(el("div", "dialog-body"),
        mkRow("파일 (CSV·TSV·XLSX)", fileInp), sheetRow, mkRow("가져오기 방식", modeSel), nameRow, hdrLbl, preview, status
      );

      const cancelBtn = el("button", "dialog-btn", "취소");
      cancelBtn.id = "btn-base-import-cancel";
      cancelBtn.type = "button";
      cancelBtn.addEventListener("click", closeDlg);

      const applyBtn = el("button", "dialog-btn dialog-btn-primary", "가져오기");
      applyBtn.id = "btn-base-import-apply";
      applyBtn.type = "button";
      applyBtn.disabled = true;
      applyBtn.addEventListener("click", applyImport);

      const actions = appendAll(el("div", "dialog-actions"), cancelBtn, applyBtn);
      appendAll(wrap, titleBar, body, actions);
      dlg.appendChild(wrap);
      doc.body.appendChild(dlg);
      host.__EASYOFFICE_SHELL_CHROME__?.enableDialogKeys?.(dlg);
      host.__EASYOFFICE_CUSTOM_SELECT__?.enhanceCustomSelects?.(doc, dlg);
      return dlg;
    }

    // Wire `.md-item[data-base-act="import"]` once per document.
    const menuItem = doc?.querySelector?.('.md-item[data-base-act="import"]');
    if (menuItem && !menuItem.dataset.baseImportBound) {
      menuItem.dataset.baseImportBound = "1";
      menuItem.addEventListener("click", () => {
        host.__EASYOFFICE_CLOSE_MENUS__?.(doc);
        openDlg();
      });
    }

    return {
      open: openDlg,
      close: closeDlg,
      importDecodedSheets: (sheets, options = {}) => {
        const idx = Math.max(0, Math.min(sheets.length - 1, Number(options.sheetIndex) || 0));
        const grid = sheetToTableGrid(sheets[idx], options);
        if (!grid.ok) return grid;
        return studioApi.runStateMutation((st) => applyImportToState(st, grid, options));
      },
      importBytes: async (fileName, bytes, options = {}) => {
        const res = await decodeImportFile(fileName, bytes, getInvoke());
        if (!res.ok) return res;
        const idx = Math.max(0, Math.min(res.sheets.length - 1, Number(options.sheetIndex) || 0));
        const grid = sheetToTableGrid(res.sheets[idx], options);
        if (!grid.ok) return grid;
        return studioApi.runStateMutation((st) => applyImportToState(st, grid, {
          ...options,
          tableName: options.tableName || res.sheets[idx]?.name || fileBaseName(fileName),
        }));
      },
    };
  }

  let activeImportController = null;
  host.__EASYOFFICE_BASE_IMPORT__ = {
    MAX_IMPORT_BYTES,
    MAX_IMPORT_RECORDS,
    MAX_IMPORT_FIELDS,
    MAX_HEADER_CHARS,
    MAX_CELL_CHARS,
    neutralizeFormulaText,
    decodeRawBytesPrecheck,
    decodeImportFile,
    sheetToTableGrid,
    applyImportToState,
    bind: (studioApi, deps) => {
      activeImportController = createBaseImportController(studioApi, deps);
      return activeImportController;
    },
    open: () => activeImportController?.open?.(),
    controller: () => activeImportController,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
