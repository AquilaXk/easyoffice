// 참조 구현 (reference implementation): rhwp-studio (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`)
// 크롬·시트 탭·대화상자 프레임 및 glide-data-grid (`glideapps/glide-data-grid`, MIT, Copyright (c) 2021 typeguard, Inc.,
// commit 0875d78cc41535a39f8b417ff88ce1ebe129c363, `packages/core/src/cells/{bubble-cell,marker-cell,new-row-cell}.tsx`;
// see `THIRD_PARTY_NOTICES.md`) 데이터베이스 스튜디오 UI.

(function (host) {
  "use strict";

  const WEEKDAYS_KO = ["일", "월", "화", "수", "목", "금", "토"];
  const VIEW_TABS = Object.freeze([
    { id: "grid", label: "데이터시트 보기" },
    { id: "kanban", label: "보드" },
    { id: "calendar", label: "일정" },
  ]);
  const pad2 = (n) => String(n).padStart(2, "0");
  // Printed table (mm): the page margin, the height of every row and of the title, and a little room the paper keeps free.
  const PRINT_MARGIN_MM = 10, PRINT_ROW_MM = 6, PRINT_TITLE_MM = 10, PRINT_GUARD_MM = 2;

  function parseMonthStr(raw) {
    const m = /^(\d{4})-(0[1-9]|1[0-2])$/.exec(String(raw || ""));
    return m ? { year: Number(m[1]), month: Number(m[2]) } : { year: 2026, month: 11 };
  }

  function stepMonthStr(raw, delta) {
    const { year, month } = parseMonthStr(raw);
    const d = new Date(Date.UTC(year, month - 1 + delta, 1));
    return d.getUTCFullYear() + "-" + pad2(d.getUTCMonth() + 1);
  }

  function currentMonthStr() {
    const now = new Date();
    return now.getFullYear() + "-" + pad2(now.getMonth() + 1);
  }

  function monthGridCells(monthStr) {
    const { year, month } = parseMonthStr(monthStr);
    const firstDow = new Date(Date.UTC(year, month - 1, 1)).getUTCDay();
    const startMs = Date.UTC(year, month - 1, 1 - firstDow);
    const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
    const totalCells = (firstDow + daysInMonth > 35 ? 6 : 5) * 7;
    const cells = [];
    for (let i = 0; i < totalCells; i += 1) {
      const d = new Date(startMs + i * 86400000);
      const y = d.getUTCFullYear(), m = d.getUTCMonth() + 1, day = d.getUTCDate();
      cells.push({ date: `${y}-${pad2(m)}-${pad2(day)}`, day, outside: y !== year || m !== month });
    }
    return cells;
  }

  function createBaseStudio(deps = {}) {
    const doc = deps.document || host.document, M = host.__EASYOFFICE_BASE_MODEL__;
    if (!doc || !M) throw new Error("데이터베이스 모델 또는 문서 객체가 필요합니다.");

    let state = M.createInitialBaseState(), rootEl = deps.rootEl || null, bound = false;
    let queryPanel = "", draftFilters = [], draftConjunction = "and", draftSorts = [];
    let editingFieldId = null, draggedRecordId = "", invalidCell = null, refs = null;
    // The rows of the datasheet around the visible ones (`base-grid-window.js`); the scroll offset, the measured row heights and the fixed column widths outlive a redraw.
    let gridWin = null, gridLive = null, liveQuery = null;
    const changeListeners = new Set();
    const gridMemo = { key: "", top: 0, left: 0, sizes: null, rowPx: 0, groupPx: 0, cfgRowPx: 0, margin: 0 }, gridWidths = new Map();

    const el = (tag, cls, text) => { const n = doc.createElement(tag); if (cls) n.className = cls; if (text !== undefined) n.textContent = text; return n; };
    const appendAll = (p, ...nodes) => { for (const n of nodes) if (n) p.appendChild(n); return p; };
    const mkBtn = (cls, label, onClick, id, aria) => { const b = el("button", cls, label); b.type = "button"; if (id) b.id = id; b.setAttribute("aria-label", aria || label); if (onClick) b.addEventListener("click", onClick); return b; };
    // A toolbar button in the reference format: the sprite icon above the label (`toolbar.css`).
    const mkTbBtn = (icon, label, onClick, id) =>
      appendAll(mkBtn("tb-btn", "", onClick, id, label), setAttr(el("span", "tb-sprite icon-" + icon), "aria-hidden", "true"), el("span", "tb-label", label));
    const mkTbSvg = (iconName, label, onClick, id) => appendAll(mkBtn("tb-btn", "", onClick, id, label), setAttr(setAttr(el("span", "tb-icon-svg"), "data-icon", iconName), "aria-hidden", "true"), el("span", "tb-label", label));
    const setTbLabel = (btn, text) => { btn.querySelector(".tb-label").textContent = text; };
    const mkSel = (cls, aria, id) => { const s = el("select", cls.includes("sb-combo") ? cls : "sb-combo " + cls); if (id) s.id = id; if (aria) s.setAttribute("aria-label", aria); return s; };
    const enhanceSelects = (scope) => host.__EASYOFFICE_CUSTOM_SELECT__?.enhanceCustomSelects?.(doc, scope);
    const syncSelect = (sel) => host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(sel);
    const mkInput = (id, type, cls, aria, ph) => { const inp = el("input", cls); inp.type = type; if (id) inp.id = id; inp.setAttribute("aria-label", aria); if (ph) inp.placeholder = ph; return inp; };
    const addOpt = (sel, value, text) => { const o = el("option", "", text); o.value = value; sel.appendChild(o); return o; };
    const setAttr = (node, k, v) => { node.setAttribute(k, v); return node; };

    const activeTable = () => M.getTable(state, state.activeTableId);
    const qCache = host.__EASYOFFICE_CREATE_BASE_QUERY_CACHE__?.(M) || null;
    const queryOf = (id) => (qCache ? qCache.query(state, id) : M.queryTableView(state, id));
    const setFeedback = (msg) => { deps.setFeedback?.(String(msg || "")); };
    // Cell selection, keys, clipboard of the datasheet (`base-cell-select.js`): it outlives a redraw, so an edit keeps the selection.
    const cellSel = host.__EASYOFFICE_CREATE_BASE_CELL_SELECT__(doc, { updateCells: (edits) => updateCells(edits), applyPaste: (edits, newRows) => applyPaste(edits, newRows), setFeedback, relationIds: (field, text) => relationIds(field, text) });
    const gridSize = host.__EASYOFFICE_CREATE_BASE_GRID_SIZE__?.(doc, { setFieldWidth: (id, w) => setFieldWidth(id, w), setRowHeight: (h) => setRowHeight(h), setFeedback });
    const recordUndo = () => { deps.recordUndo?.(); };
    const noteActivity = () => { deps.noteActivity?.(); };
    const byId = (id) => doc.getElementById(id);

    function ensureRoot() {
      if (rootEl) { rootEl.classList.add("base-pane"); if (!rootEl.getAttribute("aria-label")) rootEl.setAttribute("aria-label", "데이터베이스 편집기"); return rootEl; }
      const existing = byId("pane-base");
      if (existing) { rootEl = existing; rootEl.classList.add("base-pane"); return rootEl; }
      const pane = setAttr(el("section", "editor-pane base-pane"), "aria-label", "데이터베이스 편집기");
      pane.id = "pane-base"; pane.hidden = true;
      (doc.querySelector(".canvas-stage") || doc.body)?.appendChild(pane);
      return (rootEl = pane);
    }

    const openDialog = (dlg) => { if (dlg) { if (typeof dlg.showModal === "function") { if (!dlg.open) dlg.showModal(); } else dlg.setAttribute("open", ""); } };
    const closeDialog = (dlg) => { if (dlg) { if (typeof dlg.close === "function") dlg.close(); else dlg.removeAttribute("open"); } };

    function buildShellDom() {
      const container = ensureRoot();
      if (refs && container.contains(refs.viewport)) return refs;

      const toolbar = setAttr(setAttr(el("div", "eo-doc-style-bar base-toolbar"), "role", "toolbar"), "aria-label", "데이터베이스 도구 모음");
      toolbar.id = "base-toolbar";
      const grpEdit = appendAll(el("div", "tb-group"),
        mkTbBtn("add", "새 레코드", () => addRecord({}), "btn-base-add-record"),
        mkTbBtn("table", "필드 추가", () => openFieldDialog(null), "btn-base-add-field")
      );
      const sep1 = setAttr(el("span", "tb-sep"), "aria-hidden", "true");
      const btnFilter = setAttr(mkTbBtn("find", "필터", () => toggleQueryPanel("filter"), "btn-base-filter"), "aria-pressed", "false");
      const btnSort = setAttr(mkTbBtn("arrow-down", "정렬", () => toggleQueryPanel("sort"), "btn-base-sort"), "aria-pressed", "false");
      const groupSelect = mkSel("sb-select base-group-select", "그룹화 기준", "base-group-select");
      groupSelect.addEventListener("change", () => setGroupBy(groupSelect.value));
      const grpQuery = appendAll(el("div", "tb-group"), btnFilter, btnSort, groupSelect);
      const sep2 = setAttr(el("span", "tb-sep"), "aria-hidden", "true");
      const grpExport = appendAll(el("div", "tb-group base-export-group"),
        mkTbSvg("folder-open", "가져오기", () => host.__EASYOFFICE_BASE_IMPORT__?.open?.(), "btn-base-import"),
        mkTbBtn("grid", "XLSX로 내보내기", () => { void exportAs("xlsx"); }, "btn-base-export-xlsx"),
        mkTbBtn("grid", "CSV로 내보내기", () => { void exportAs("csv"); }, "btn-base-export-csv"),
        mkTbBtn("grid", "TSV로 내보내기", () => { void exportAs("tsv"); }, "btn-base-export-tsv")
      );
      const viewTabs = setAttr(setAttr(el("div", "base-view-tabs"), "role", "tablist"), "aria-label", "보기 전환");
      viewTabs.id = "base-view-tabs";
      const viewButtons = VIEW_TABS.map((vt) => {
        const b = mkBtn("base-view-tab", vt.label, () => setView(vt.id));
        b.dataset.baseView = vt.id;
        setAttr(setAttr(b, "role", "tab"), "aria-selected", "false");
        viewTabs.appendChild(b);
        return b;
      });
      appendAll(toolbar, grpEdit, sep1, grpQuery, sep2, grpExport, viewTabs);

      const queryBar = setAttr(el("div", "base-query-bar"), "id", "base-query-bar");
      queryBar.hidden = true;
      const viewport = setAttr(el("div", "base-viewport"), "id", "base-viewport");

      const bottomBar = setAttr(setAttr(el("div", "sheet-tab-bar base-sheet-tab-bar"), "role", "region"), "aria-label", "테이블 탭");
      const btnAddTable = mkBtn("sheet-tab-btn sheet-tab-add base-add-table-btn", "+ 새 테이블", () => addTable({}), "btn-base-add-table", "새 테이블");
      const tableTabs = setAttr(setAttr(el("div", "sheet-tabs base-table-tabs"), "role", "tablist"), "aria-label", "테이블 목록");
      tableTabs.id = "base-table-tabs";
      appendAll(bottomBar, tableTabs, btnAddTable);

      byId("base-field-dialog")?.remove?.();
      const fieldDlg = buildFieldDialogDom();
      doc.body.appendChild(fieldDlg);

      container.replaceChildren(toolbar, queryBar, viewport, bottomBar);
      host.__EASYOFFICE_TOOLBAR_SCROLL__?.attach?.(toolbar, doc);
      host.__EASYOFFICE_ICONS__?.applyIcons?.(doc, toolbar);
      enhanceSelects(container);
      enhanceSelects(fieldDlg);
      refs = { container, tableTabs, viewTabs, viewButtons, btnFilter, btnSort, groupSelect, queryBar, viewport, fieldDlg };
      return refs;
    }

    function buildFieldDialogDom() {
      const dlg = setAttr(el("dialog", "tool-dialog base-field-dialog"), "aria-labelledby", "base-field-dialog-title");
      dlg.id = "base-field-dialog";
      const wrap = el("div", "dialog-wrap"), titleBar = el("div", "dialog-title");
      const titleSpan = el("span", "", "필드 추가");
      titleSpan.id = "base-field-dialog-title";
      appendAll(titleBar, titleSpan, mkBtn("dialog-close", "×", () => closeDialog(dlg), "", "닫기"));

      const mkRow = (labelText, inputEl, rowId) => {
        const row = el("div", "dialog-row"), lbl = el("label", "dialog-label", labelText);
        if (rowId) row.id = rowId;
        if (inputEl.id) lbl.setAttribute("for", inputEl.id);
        return appendAll(row, lbl, inputEl);
      };

      const nameInput = mkInput("base-field-name", "text", "dialog-text-input base-dlg-wide-text", "필드 이름");
      const typeSelect = mkSel("base-dlg-wide-pick", "필드 유형", "base-field-type");
      for (const ft of M.FIELD_TYPES) addOpt(typeSelect, ft, M.FIELD_TYPE_LABELS[ft]);
      const optsInput = mkInput("base-field-options", "text", "dialog-text-input base-dlg-wide-text", "선택 옵션 (쉼표 구분)");
      const relSelect = mkSel("base-dlg-wide-pick", "연결 테이블", "base-field-relation-table");
      const formulaInput = mkInput("base-field-formula", "text", "dialog-text-input base-dlg-wide-text", "수식");

      const reqCheck = mkInput("base-field-required", "checkbox", "", "필수 입력");
      const reqLabel = setAttr(el("label", "dialog-checkbox"), "for", "base-field-required");
      appendAll(reqLabel, reqCheck, doc.createTextNode(" 필수 입력"));
      const valRow = appendAll(el("div", "dialog-row"), el("span", "dialog-label", "유효성 검사"), reqLabel);
      valRow.id = "base-field-row-validation";

      const minInput = mkInput("base-field-min", "number", "dialog-input", "최솟값"), maxInput = mkInput("base-field-max", "number", "dialog-input", "최댓값");
      const minLbl = setAttr(el("label", "dialog-label", "최솟값"), "for", "base-field-min");
      const maxLbl = setAttr(el("label", "dialog-label", "최댓값"), "for", "base-field-max");
      const numBoundsRow = appendAll(el("div", "dialog-row"), minLbl, minInput, maxLbl, maxInput);
      numBoundsRow.id = "base-field-row-num-bounds";

      const maxLenInput = mkInput("base-field-max-length", "number", "dialog-input", "최대 글자 수");
      const statusNote = setAttr(el("p", "dialog-pdf-note", ""), "role", "status");
      statusNote.id = "base-field-dialog-status";

      const rowOpts = mkRow("선택 옵션", optsInput, "base-field-row-options");
      const rowRel = mkRow("관계 테이블", relSelect, "base-field-row-relation");
      const rowFormula = mkRow("수식", formulaInput, "base-field-row-formula");
      const rowMaxLen = mkRow("최대 글자 수", maxLenInput, "base-field-row-maxlen");
      const body = appendAll(el("div", "dialog-body"),
        mkRow("필드 이름", nameInput), mkRow("필드 유형", typeSelect),
        rowOpts, rowRel, rowFormula, valRow, numBoundsRow, rowMaxLen, statusNote
      );

      const syncTypeRows = () => {
        const t = typeSelect.value;
        rowOpts.hidden = t !== "select" && t !== "multiSelect";
        rowRel.hidden = t !== "relation";
        rowFormula.hidden = t !== "formula";
        valRow.hidden = t === "formula";
        numBoundsRow.hidden = t !== "number";
        rowMaxLen.hidden = t !== "text" && t !== "url";
      };
      typeSelect.addEventListener("change", syncTypeRows);

      const btnDel = mkBtn("dialog-btn base-danger-action", "필드 삭제", () => {
        if (!editingFieldId) return;
        const res = deleteField(editingFieldId);
        if (!res.ok) statusNote.textContent = res.error; else closeDialog(dlg);
      }, "btn-base-field-delete");
      btnDel.hidden = true;
      const footer = appendAll(el("div", "dialog-footer"),
        btnDel,
        mkBtn("dialog-btn dialog-btn-primary", "확인", () => applyFieldDialog(), "btn-base-field-ok"),
        mkBtn("dialog-btn", "취소", () => closeDialog(dlg), "btn-base-field-cancel")
      );
      appendAll(wrap, titleBar, body, footer);
      dlg.appendChild(wrap);
      dlg._syncTypeRows = syncTypeRows;
      return dlg;
    }

    function openFieldDialog(field = null) {
      buildShellDom();
      const dlg = refs.fieldDlg;
      editingFieldId = field ? field.id : null;
      byId("base-field-dialog-title").textContent = field ? "필드 편집" : "필드 추가";
      byId("base-field-name").value = field ? field.name : "";
      byId("base-field-type").value = field ? field.type : "text";
      byId("base-field-options").value = field?.options ? field.options.join(", ") : "";
      byId("base-field-formula").value = field?.formula ?? "";
      byId("base-field-required").checked = Boolean(field?.validation?.required);
      byId("base-field-min").value = field?.validation?.min !== undefined ? String(field.validation.min) : "";
      byId("base-field-max").value = field?.validation?.max !== undefined ? String(field.validation.max) : "";
      byId("base-field-max-length").value = field?.validation?.maxLength !== undefined ? String(field.validation.maxLength) : "";
      byId("base-field-dialog-status").textContent = "";

      const relSel = byId("base-field-relation-table");
      relSel.replaceChildren();
      for (const tbl of state.tables) addOpt(relSel, tbl.id, tbl.name);
      relSel.value = field?.relationTableId || state.tables[0]?.id || "";
      byId("btn-base-field-delete").hidden = !field || (activeTable()?.fields?.length || 0) <= 1;
      syncSelect(byId("base-field-type"));
      syncSelect(relSel);
      dlg._syncTypeRows?.();
      openDialog(dlg);
      byId("base-field-name")?.focus?.();
    }

    function applyFieldDialog() {
      const spec = {
        name: byId("base-field-name").value, type: byId("base-field-type").value,
        options: byId("base-field-options").value, relationTableId: byId("base-field-relation-table").value,
        formula: byId("base-field-formula").value,
        validation: {
          required: byId("base-field-required").checked, min: byId("base-field-min").value,
          max: byId("base-field-max").value, maxLength: byId("base-field-max-length").value,
        },
      };
      const res = editingFieldId ? updateField(editingFieldId, spec) : addField(spec);
      if (!res.ok) { byId("base-field-dialog-status").textContent = res.error; return; }
      editingFieldId = null;
      closeDialog(refs.fieldDlg);
    }

    // ---- Query Bar (Filter & Sort) ----

    function toggleQueryPanel(kind) {
      const tbl = activeTable();
      if (queryPanel === kind) queryPanel = "";
      else {
        queryPanel = kind;
        draftFilters = (tbl?.filters || []).map((f) => ({ ...f }));
        draftConjunction = tbl?.filterConjunction === "or" ? "or" : "and";
        draftSorts = (tbl?.sorts || []).map((s) => ({ ...s }));
      }
      render();
    }

    function populateFieldOpts(sel, fields, curId) {
      for (const f of fields) addOpt(sel, f.id, f.name);
      sel.value = curId || fields[0]?.id || "";
      return sel.value;
    }

    function renderQueryBar(table) {
      const bar = refs.queryBar;
      if (!queryPanel || !table) { bar.hidden = true; bar.replaceChildren(); return; }
      bar.hidden = false;
      const isFilter = queryPanel === "filter";
      const panel = el("div", "base-query-panel");
      panel.id = isFilter ? "base-filter-panel" : "base-sort-panel";
      const list = el("div", "base-query-rows"), actions = el("div", "base-query-actions");

      if (isFilter) {
        const conjSel = mkSel("sb-select", "필터 결합 조건", "base-filter-conjunction");
        addOpt(conjSel, "and", "모두 일치 (AND)");
        addOpt(conjSel, "or", "하나라도 일치 (OR)");
        conjSel.value = draftConjunction;
        conjSel.addEventListener("change", () => { draftConjunction = conjSel.value; });
        panel.appendChild(appendAll(el("div", "base-query-head"), el("strong", "", "필터"), conjSel));

        draftFilters.forEach((rule, idx) => {
          const fSel = mkSel("sb-select", "필터 필드");
          rule.fieldId = populateFieldOpts(fSel, table.fields, rule.fieldId);
          fSel.addEventListener("change", () => { rule.fieldId = fSel.value; });
          const opSel = mkSel("sb-select", "필터 조건");
          for (const op of M.FILTER_OPS) addOpt(opSel, op, M.FILTER_OP_LABELS[op]);
          opSel.value = rule.op || "contains";
          const valInp = mkInput("", "text", "sb-select base-query-val", "필터 값", "비교 값");
          valInp.value = rule.value || "";
          valInp.hidden = opSel.value === "empty" || opSel.value === "notEmpty";
          valInp.addEventListener("input", () => { rule.value = valInp.value; });
          opSel.addEventListener("change", () => { rule.op = opSel.value; valInp.hidden = opSel.value === "empty" || opSel.value === "notEmpty"; });
          list.appendChild(appendAll(el("div", "base-query-row"), fSel, opSel, valInp,
            mkBtn("dialog-btn", "삭제", () => { draftFilters.splice(idx, 1); renderQueryBar(table); }, "", "필터 조건 삭제")));
        });
        appendAll(actions,
          mkBtn("dialog-btn", "필터 추가", () => { draftFilters.push({ fieldId: table.fields[0]?.id || "", op: "contains", value: "" }); renderQueryBar(table); }, "btn-base-filter-add"),
          mkBtn("dialog-btn dialog-btn-primary", "적용", () => setFilters(draftFilters, draftConjunction), "btn-base-filter-apply"),
          mkBtn("dialog-btn", "초기화", () => { draftFilters = []; setFilters([], "and"); }, "btn-base-filter-reset"),
          mkBtn("dialog-btn", "닫기", () => { queryPanel = ""; render(); }, "btn-base-query-close")
        );
      } else {
        panel.appendChild(el("div", "base-query-head", "정렬"));
        draftSorts.forEach((srt, idx) => {
          const fSel = mkSel("sb-select", "정렬 필드");
          srt.fieldId = populateFieldOpts(fSel, table.fields, srt.fieldId);
          fSel.addEventListener("change", () => { srt.fieldId = fSel.value; });
          const dirSel = mkSel("sb-select", "정렬 방향");
          addOpt(dirSel, "asc", "오름차순"); addOpt(dirSel, "desc", "내림차순");
          dirSel.value = srt.dir === "desc" ? "desc" : "asc";
          dirSel.addEventListener("change", () => { srt.dir = dirSel.value; });
          list.appendChild(appendAll(el("div", "base-query-row"), fSel, dirSel,
            mkBtn("dialog-btn", "삭제", () => { draftSorts.splice(idx, 1); renderQueryBar(table); }, "", "정렬 기준 삭제")));
        });
        appendAll(actions,
          mkBtn("dialog-btn", "정렬 추가", () => { draftSorts.push({ fieldId: table.fields[0]?.id || "", dir: "asc" }); renderQueryBar(table); }, "btn-base-sort-add"),
          mkBtn("dialog-btn dialog-btn-primary", "적용", () => setSorts(draftSorts), "btn-base-sort-apply"),
          mkBtn("dialog-btn", "초기화", () => { draftSorts = []; setSorts([]); }, "btn-base-sort-reset"),
          mkBtn("dialog-btn", "닫기", () => { queryPanel = ""; render(); }, "btn-base-query-close")
        );
      }
      appendAll(panel, list, actions);
      bar.replaceChildren(panel);
      enhanceSelects(bar);
    }

    // Per render: relation field id -> [[record id, label]] of its target table, built the first time a cell of the field is used.
    let relationOptions = new Map();
    function relationOptionsOf(field, query) {
      let list = relationOptions.get(field.id);
      if (!list) {
        const target = query.index.tablesById.get(field.relationTableId), primary = target?.fields?.[0];
        list = (target?.records || []).map((rec) => [rec.id, (primary ? M.formatCellDisplay(primary, rec.values?.[primary.id], state, query.index) : rec.id) || rec.id]);
        relationOptions.set(field.id, list);
      }
      return list;
    }

    // The record ids that a pasted text of labels ("Kim, Lee") stands for. A text that is no label stays as it is, for the model to check.
    const relationLabels = new WeakMap();
    function relationIds(field, text) {
      const list = relationOptions.get(field.id) || relationOptionsOf(field, { index: qCache ? qCache.index(state) : M.buildStateIndex(state) });
      let byLabel = relationLabels.get(list);
      if (!byLabel) {
        byLabel = new Map();
        for (const [id, label] of list) if (!byLabel.has(label)) byLabel.set(label, id);
        relationLabels.set(list, byLabel);
      }
      return text.split(",").map((s) => s.trim()).filter(Boolean).map((s) => byLabel.get(s) || s);
    }

    // ---- Cell Controls & Invalid Handling ----

    function markInvalid(td, control, msg) {
      td.dataset.invalid = "true"; td.setAttribute("aria-invalid", "true"); td.title = msg;
      if (td.parentElement) td.parentElement._eoInvalid = true;
      if (control) { control.dataset.invalid = "true"; control.setAttribute("aria-invalid", "true"); }
    }

    function commitGridEdit(td, control, recId, fieldId, nextValue, revertFn) {
      const res = updateCell(recId, fieldId, nextValue);
      if (!res.ok) { revertFn(); markInvalid(td, control, res.error); }
    }

    function buildCellControl(td, rowData, field, rowIndex, query) {
      const recId = rowData.record.id, val = rowData.values[field.id], disp = rowData.displays[field.id];
      const label = `${field.name} (${rowIndex + 1})`;
      const showPrevInvalid = (ctrl) => {
        if (invalidCell && invalidCell.recordId === recId && invalidCell.fieldId === field.id) markInvalid(td, ctrl, invalidCell.error);
      };

      if (field.type === "formula") {
        const span = setAttr(el("span", "base-formula-val", disp), "aria-label", `${field.name} 계산 결과`);
        span.dataset.error = String(val === "#CYCLE!" || val === "#DIV/0!" || val === "#ERROR!");
        td.appendChild(span);
        return;
      }
      if (field.type === "checkbox") {
        const cb = mkInput("", "checkbox", "base-cell-checkbox", label);
        cb.checked = Boolean(val);
        cb.addEventListener("change", () => commitGridEdit(td, cb, recId, field.id, cb.checked, () => { cb.checked = Boolean(val); }));
        showPrevInvalid(cb); td.appendChild(cb);
        return;
      }
      if (field.type === "select") {
        const sel = mkSel("base-cell-select", label);
        addOpt(sel, "", "(선택 안 함)");
        for (const opt of field.options || []) addOpt(sel, opt, opt);
        sel.value = String(val || "");
        sel.addEventListener("change", () => commitGridEdit(td, sel, recId, field.id, sel.value, () => { sel.value = String(val || ""); }));
        showPrevInvalid(sel); td.appendChild(sel);
        return;
      }
      if (field.type === "multiSelect") {
        const box = el("div", "base-multi-cell"), selected = new Set(Array.isArray(val) ? val : []);
        for (const opt of field.options || []) {
          const pill = mkBtn("base-multi-opt" + (selected.has(opt) ? " is-on" : ""), opt, () => {
            const next = new Set(selected);
            if (next.has(opt)) next.delete(opt); else next.add(opt);
            commitGridEdit(td, pill, recId, field.id, Array.from(next), () => {});
          }, "", `${field.name}: ${opt}`);
          pill.setAttribute("aria-pressed", selected.has(opt) ? "true" : "false");
          box.appendChild(pill);
        }
        showPrevInvalid(null); td.appendChild(box);
        return;
      }
      if (field.type === "relation") {
        const wrap = el("div", "base-relation-cell"), items = M.resolveRelationItems(state, field, val, query.index);
        if (items.length > 0) {
          const chips = el("div", "base-relation-chips");
          for (const item of items) chips.appendChild(el("span", "base-relation-chip", item.label));
          wrap.appendChild(chips);
        }
        const sel = mkSel("base-cell-select", label), curSingle = Array.isArray(val) ? (val[0] || "") : String(val || "");
        addOpt(sel, "", "(연결 없음)"); if (items[0]) addOpt(sel, items[0].id, items[0].label);
        sel.value = curSingle;
        let optionsLoaded = false;
        const ensureOptions = () => {
          if (optionsLoaded) return; optionsLoaded = true;
          sel.replaceChildren(); addOpt(sel, "", "(연결 없음)");
          for (const [id, txt] of relationOptionsOf(field, query)) addOpt(sel, id, txt);
          sel.value = curSingle; syncSelect(sel);
        };
        for (const t of ["focus", "mousedown"]) sel.addEventListener(t, ensureOptions, { once: true });
        for (const t of ["mousedown", "focusin", "keydown"]) wrap.addEventListener(t, ensureOptions, { once: true, capture: true });
        sel.addEventListener("change", () => commitGridEdit(td, sel, recId, field.id, sel.value ? [sel.value] : [], () => { sel.value = curSingle; }));
        showPrevInvalid(sel); wrap.appendChild(sel); td.appendChild(wrap);
        return;
      }

      const inp = mkInput("", field.type === "date" ? "date" : (field.type === "url" ? "url" : "text"),
        "base-cell-input" + (field.type === "number" ? " base-cell-num" : ""), label);
      if (field.type === "number") inp.setAttribute("inputmode", "decimal");
      const prevStr = val == null ? "" : String(val);
      inp.value = prevStr;
      inp.addEventListener("change", () => commitGridEdit(td, inp, recId, field.id, inp.value, () => { inp.value = prevStr; }));
      showPrevInvalid(inp); td.appendChild(inp);

      if (field.type === "url" && prevStr) {
        const link = setAttr(el("a", "base-url-link", "열기"), "aria-label", `웹 주소 열기: ${field.name}`);
        link.href = prevStr; link.target = "_blank"; link.rel = "noopener noreferrer";
        // In the desktop shell the address opens through the shell's own command, which applies its link policy; the preview server lets the anchor open it.
        link.addEventListener("click", (ev) => {
          const bridge = host.__EASYOFFICE_DESKTOP_BRIDGE__;
          if (!bridge?.isAvailable?.()) return;
          ev.preventDefault?.();
          Promise.resolve(bridge.openLink(prevStr)).catch((err) => setFeedback("웹 주소를 열지 못했습니다: " + (err?.message || String(err))));
        });
        td.appendChild(link);
      }
    }

    // ---- 3 Synchronized View Renderers ----

    function gridFootTexts(query) {
      const footCount = query.filteredCount === query.totalCount ? `개수: ${query.totalCount}건` : `개수: ${query.filteredCount} / ${query.totalCount}건`;
      const footSums = query.fields.map((field) => {
        const sum = query.numericSums[field.id], hasSum = field.type === "number" || (field.type === "formula" && (query.numericCounts ? query.numericCounts[field.id] > 0 : query.rows.some((r) => typeof r.values[field.id] === "number")));
        return hasSum && Number.isFinite(sum) ? `합계: ${Math.round(sum * 1000) / 1000}` : "";
      });
      return { footCount, footSums };
    }

    function renderGridView(table, query) {
      const wrap = el("div", "base-grid-wrap"), tblEl = setAttr(el("table", "base-grid-table"), "aria-label", "데이터시트 보기");
      const thead = el("thead"), headRow = el("tr"), cornerTh = setAttr(el("th", "base-grid-corner", "#"), "scope", "col");
      headRow.appendChild(cornerTh);

      for (const field of query.fields) {
        const th = setAttr(el("th", "base-grid-th"), "scope", "col");
        th.dataset.fieldId = field.id;
        if (field.width !== undefined) th.style.width = field.width + "px";
        const inner = appendAll(el("div", "base-th-inner"),
          el("span", "base-th-name", field.name),
          el("span", "base-th-type", M.FIELD_TYPE_LABELS[field.type] || field.type),
          appendAll(mkBtn("base-field-edit-btn", "", () => openFieldDialog(field), "", `필드 설정: ${field.name}`), setAttr(el("span", "eo-chevron"), "aria-hidden", "true"))
        );
        appendAll(th, inner, setAttr(el("span", "col-resizer"), "aria-hidden", "true")); headRow.appendChild(th);
      }
      thead.appendChild(headRow);

      // Only the rows around the visible ones are built (`base-grid-window.js`); a short table is built whole.
      const buildGroupRow = (group) => {
        const gr = el("tr", "base-group-row"), gtd = el("td", "base-group-cell");
        gr.dataset.groupKey = group.key; gtd.colSpan = query.fields.length + 1;
        const arrow = setAttr(el("span", "base-group-arrow"), "aria-hidden", "true"); host.__EASYOFFICE_ICONS__?.setIcon?.(doc, arrow, group.collapsed ? "chevron-right" : "chevron-down");
        const toggle = setAttr(mkBtn("base-group-toggle", "", () => setViewSetting((t) => {
          const set = new Set(t.collapsedGroups || []);
          if (set.has(group.key)) set.delete(group.key); else set.add(group.key);
          t.collapsedGroups = Array.from(set);
          gridSize?.invalidateTable(t.id, gridWidths);
        }), "", `그룹 접기/펼치기: ${group.label}`), "aria-expanded", group.collapsed ? "false" : "true");
        gtd.appendChild(appendAll(toggle, arrow, el("span", "base-group-label", `${group.label} (${group.count}건)`))); gr.appendChild(gtd);
        return gr;
      };
      const buildDataRow = (rowData, rowNum, q = query) => {
        const tr = el("tr", "base-grid-row");
        tr.dataset.recordId = rowData.record.id;
        tr.appendChild(appendAll(el("td", "base-row-index"),
          el("span", "base-row-num", String(rowNum + 1)),
          mkBtn("base-row-delete-btn", "×", () => deleteRecord(rowData.record.id), "", "레코드 삭제"),
          setAttr(el("span", "row-resizer"), "aria-hidden", "true")
        ));
        for (const field of q.fields) {
          const td = el("td", "base-grid-cell");
          td.dataset.fieldId = field.id; td.dataset.fieldType = field.type;
          buildCellControl(td, rowData, field, rowNum, q);
          tr.appendChild(td);
        }
        return tr;
      };
      if (table.rowHeight !== undefined) { tblEl.style.setProperty("--base-row-height", table.rowHeight + "px"); tblEl.dataset.rowHeight = String(table.rowHeight); }
      const { footCount, footSums } = gridFootTexts(query);
      const { widthKey, growOnly } = gridSize ? gridSize.prepareAutoWidths(table, query, [footCount, ...footSums])
        : { widthKey: table.id + "|" + query.fields.map((f) => `${f.id}:${f.name}:${f.type}`).join("|"), growOnly: false };
      gridWin = host.__EASYOFFICE_BASE_GRID_WINDOW__.createGridWindow(doc, {
        query, groups: query.groups, grouped: Boolean(table.groupByFieldId), colSpan: query.fields.length + 1, buildGroupRow, buildDataRow,
        memo: gridMemo, widths: gridWidths, scrollKey: table.id, widthKey, fields: query.fields, rowHeight: table.rowHeight, growOnly, onRowBuilt: (tr, item) => { enhanceSelects(tr); cellSel.rowBuilt(tr, item); },
      });
      const tbody = gridWin.tbody, itemCount = gridWin.count;
      // The selection is kept while the table is drawn again for an edit; another table, filter, sort or group starts without one.
      const viewKey = JSON.stringify([table.id, table.filters, table.filterConjunction, table.sorts, table.groupByFieldId, table.collapsedGroups]);
      cellSel.bind({ tbl: tblEl, wrap, win: gridWin, fields: query.fields, tableId: table.id }, viewKey);
      gridSize?.bind({ tbl: tblEl, table, query, win: gridWin, footSums });
      setAttr(tblEl, "aria-rowcount", String(itemCount + 3)); setAttr(headRow, "aria-rowindex", "1");

      const newRowTr = setAttr(el("tr", "base-new-record-row"), "aria-rowindex", String(itemCount + 2));
      const newRowIdx = el("td", "base-row-index base-new-row-marker", "+");
      const newRowCell = el("td", "base-new-record-cell");
      newRowCell.colSpan = query.fields.length;
      newRowCell.appendChild(mkBtn("base-inline-add-btn", "새 레코드", () => addRecord({}), "", "새 레코드 추가"));
      appendAll(newRowTr, newRowIdx, newRowCell);
      tbody.appendChild(newRowTr);

      const tfoot = el("tfoot"), footRow = setAttr(el("tr", "base-grid-footer"), "aria-rowindex", String(itemCount + 3));
      const footCountTd = el("td", "base-footer-count", footCount);
      footRow.appendChild(footCountTd);
      const footSumTds = footSums.map((txt) => { const td = el("td", "base-footer-sum", txt); footRow.appendChild(td); return td; });
      tfoot.appendChild(footRow);
      appendAll(tblEl, thead, tbody, tfoot);
      wrap.appendChild(tblEl);
      gridLive = { tableId: table.id, viewKey, wrap, tblEl, newRowTr, footRow, footCountTd, footSumTds };
      return wrap;
    }

    function compactBadgeText(field, disp) {
      if (field.type === "url") {
        try { return new URL(disp).hostname || disp; } catch { return disp; }
      }
      return disp;
    }

    function renderKanbanView(table, query) {
      const wrap = el("div", "base-kanban-wrap"), lbl = setAttr(el("label", "base-subview-label", "기준 필드"), "for", "base-kanban-field-select");
      const sel = mkSel("sb-select", "보드 기준 필드", "base-kanban-field-select");
      const candidates = query.fields.filter((f) => f.type === "select" || f.type === "multiSelect");
      for (const f of candidates) addOpt(sel, f.id, f.name);
      const chosen = candidates.find((f) => f.id === table.kanbanFieldId) || candidates[0] || null;
      if (chosen) { table.kanbanFieldId = chosen.id; sel.value = chosen.id; }
      sel.addEventListener("change", () => setViewSetting((t) => { t.kanbanFieldId = sel.value; }));
      wrap.appendChild(appendAll(el("div", "base-subview-bar"), lbl, sel));
      if (!chosen) { wrap.appendChild(el("p", "base-empty-hint", "선택 유형 필드 없음")); return wrap; }

      const multi = chosen.type === "multiSelect", options = chosen.options || [];
      const columns = [...options.map((o) => ({ key: o, label: o })), { key: "", label: "미분류" }];
      const buckets = new Map(columns.map((c) => [c.key, []]));
      const keysOf = (value) => {
        const own = (Array.isArray(value) ? value : [value]).map((v) => String(v ?? "").trim()).filter((v) => buckets.has(v) && v !== "");
        return own.length > 0 ? own : [""];
      };
      for (const row of query.rows) for (const key of keysOf(row.values[chosen.id])) buckets.get(key).push(row);
      // A card moves from its column to another one of the field's options (or to 미분류); the same column, or a value
      // outside the options, writes nothing, and a multi-select card keeps the tags of its other columns.
      const moveCard = (recId, fromKey, toKey) => {
        if (toKey === fromKey || (toKey !== "" && !options.includes(toKey))) return;
        const rec = activeTable()?.records.find((r) => r.id === recId);
        if (!rec) return;
        if (!multi) { updateCell(recId, chosen.id, toKey); return; }
        // The tags outside the field's options stay: only the column the card leaves is taken off.
        const held = Array.isArray(rec.values[chosen.id]) ? rec.values[chosen.id] : [];
        const next = held.filter((k) => k !== fromKey);
        if (toKey !== "") next.push(toKey);
        updateCell(recId, chosen.id, next);
      };
      let draggedFromKey = "";
      const colsEl = el("div", "base-kanban-columns"), primaryField = query.fields[0];
      for (const col of columns) {
        const colEl = el("div", "base-kanban-col");
        colEl.dataset.kanbanValue = col.key;
        colEl.addEventListener("dragover", (e) => { e.preventDefault?.(); });
        colEl.addEventListener("drop", (e) => {
          e.preventDefault?.();
          const recId = e.dataTransfer?.getData?.("text/plain") || draggedRecordId;
          draggedRecordId = "";
          if (recId) moveCard(recId, draggedFromKey, col.key);
        });
        const items = buckets.get(col.key) || [];
        const colHead = appendAll(el("div", "base-kanban-col-head"), el("span", "base-kanban-col-title", col.label), el("span", "base-kanban-count", String(items.length)));
        const cardsEl = el("div", "base-kanban-cards");
        for (const row of items) {
          const recId = row.record.id, card = setAttr(el("article", "base-kanban-card"), "draggable", "true");
          card.dataset.recordId = recId;
          card.addEventListener("dragstart", (e) => { draggedRecordId = recId; draggedFromKey = col.key; e.dataTransfer?.setData?.("text/plain", recId); });
          const titleTxt = (primaryField ? row.displays[primaryField.id] : "") || "(제목 없음)";
          card.appendChild(el("div", "base-kanban-card-title", titleTxt));
          const badges = el("div", "base-kanban-badges");
          for (const f of query.fields) {
            if (f.id === primaryField?.id || f.id === chosen.id || !row.displays[f.id]) continue;
            if (f.type === "checkbox" && row.values[f.id] === false) continue;
            const bTxt = f.type === "checkbox" ? f.name : `${f.name}: ${compactBadgeText(f, row.displays[f.id])}`;
            badges.appendChild(el("span", "base-kanban-badge", bTxt));
          }
          if (badges.childNodes.length > 0) card.appendChild(badges);
          const moveSel = mkSel("sb-select base-kanban-move-select", `상태 변경: ${titleTxt}`);
          for (const optCol of columns) addOpt(moveSel, optCol.key, optCol.label);
          moveSel.value = col.key;
          moveSel.addEventListener("change", () => moveCard(recId, col.key, moveSel.value));
          card.appendChild(moveSel); cardsEl.appendChild(card);
        }
        const fresh = multi ? (col.key ? [col.key] : []) : col.key;
        appendAll(colEl, colHead, cardsEl, mkBtn("base-kanban-add-btn", "+ 새 레코드", () => addRecord({ [chosen.id]: fresh }), "", `${col.label} 새 레코드`));
        colsEl.appendChild(colEl);
      }
      wrap.appendChild(colsEl);
      return wrap;
    }

    function renderCalendarView(table, query) {
      const wrap = el("div", "base-cal-wrap"), lbl = setAttr(el("label", "base-subview-label", "날짜 필드"), "for", "base-calendar-field-select");
      const fSel = mkSel("sb-select", "일정 날짜 필드", "base-calendar-field-select");
      const dateFields = query.fields.filter((f) => f.type === "date");
      for (const f of dateFields) addOpt(fSel, f.id, f.name);
      const chosen = dateFields.find((f) => f.id === table.calendarFieldId) || dateFields[0] || null;
      if (chosen) { table.calendarFieldId = chosen.id; fSel.value = chosen.id; }
      fSel.addEventListener("change", () => setViewSetting((t) => { t.calendarFieldId = fSel.value; }));

      const { year, month } = parseMonthStr(table.calendarMonth);
      const calTitle = el("span", "base-cal-title", `${year}년 ${month}월`);
      calTitle.id = "base-cal-title";
      const navGroup = appendAll(el("div", "base-cal-nav"),
        mkBtn("dialog-btn", "이전 달", () => setViewSetting((t) => { t.calendarMonth = stepMonthStr(t.calendarMonth, -1); }), "btn-base-cal-prev"),
        calTitle,
        mkBtn("dialog-btn", "다음 달", () => setViewSetting((t) => { t.calendarMonth = stepMonthStr(t.calendarMonth, 1); }), "btn-base-cal-next"),
        mkBtn("dialog-btn", "오늘", () => setViewSetting((t) => { t.calendarMonth = currentMonthStr(); }), "btn-base-cal-today")
      );
      wrap.appendChild(appendAll(el("div", "base-subview-bar base-cal-header"), lbl, fSel, navGroup));
      if (!chosen) { wrap.appendChild(el("p", "base-empty-hint", "날짜 유형 필드 없음")); return wrap; }

      const byDate = new Map();
      for (const row of query.rows) {
        const dStr = String(row.values[chosen.id] || "").trim();
        if (!dStr) continue;
        let list = byDate.get(dStr);
        if (!list) { list = []; byDate.set(dStr, list); }
        list.push(row);
      }
      const grid = el("div", "base-cal-grid"), primaryField = query.fields[0];
      for (const wd of WEEKDAYS_KO) grid.appendChild(el("div", "base-cal-weekday", wd));
      for (const cellInfo of monthGridCells(table.calendarMonth)) {
        const cell = el("div", "base-cal-cell" + (cellInfo.outside ? " is-outside" : ""));
        cell.dataset.date = cellInfo.date; cell.dataset.outsideMonth = String(cellInfo.outside);
        cell.appendChild(appendAll(el("div", "base-cal-cell-head"),
          el("span", "base-cal-day-num", String(cellInfo.day)),
          mkBtn("base-cal-add-btn", "+", () => addRecord({ [chosen.id]: cellInfo.date }), "", `${cellInfo.date} 새 레코드`)
        ));
        for (const row of byDate.get(cellInfo.date) || []) {
          const chip = el("div", "base-cal-chip", (primaryField ? row.displays[primaryField.id] : "") || "(제목 없음)");
          chip.dataset.recordId = row.record.id;
          cell.appendChild(chip);
        }
        grid.appendChild(cell);
      }
      wrap.appendChild(grid);
      return wrap;
    }

    function render() {
      buildShellDom();
      relationOptions = new Map();
      const table = activeTable();
      if (!table) { liveQuery = null; return; }
      refs.tableTabs.replaceChildren();
      for (const tbl of state.tables) {
        const active = tbl.id === table.id;
        const btn = mkBtn("sheet-tab-btn base-table-tab" + (active ? " active" : ""), tbl.name, () => {
          state.activeTableId = tbl.id; invalidCell = null; queryPanel = ""; render();
        });
        btn.dataset.tableId = tbl.id;
        setAttr(setAttr(btn, "role", "tab"), "aria-selected", active ? "true" : "false");
        refs.tableTabs.appendChild(btn);
      }
      for (const btn of refs.viewButtons) {
        const active = btn.dataset.baseView === table.view;
        btn.classList.toggle("active", active);
        btn.setAttribute("aria-selected", active ? "true" : "false");
      }
      const fCount = (table.filters || []).length, sCount = (table.sorts || []).length;
      setTbLabel(refs.btnFilter, fCount > 0 ? `필터 (${fCount})` : "필터");
      refs.btnFilter.setAttribute("aria-pressed", String(queryPanel === "filter" || fCount > 0));
      setTbLabel(refs.btnSort, sCount > 0 ? `정렬 (${sCount})` : "정렬");
      refs.btnSort.setAttribute("aria-pressed", String(queryPanel === "sort" || sCount > 0));

      refs.groupSelect.replaceChildren();
      addOpt(refs.groupSelect, "", "그룹화 기준: 없음");
      for (const f of table.fields) addOpt(refs.groupSelect, f.id, `그룹화 기준: ${f.name}`);
      refs.groupSelect.value = table.groupByFieldId || "";
      syncSelect(refs.groupSelect);

      renderQueryBar(table);
      cellSel.beforeRender();
      gridWin?.dispose();
      gridWin = null; gridLive = null;
      const query = (liveQuery = queryOf(table.id));
      const viewNode = table.view === "kanban" ? renderKanbanView(table, query)
        : table.view === "calendar" ? renderCalendarView(table, query)
        : renderGridView(table, query);
      refs.viewport.replaceChildren(viewNode);
      enhanceSelects(refs.viewport);
      if (gridWin) { gridWin.attach(viewNode, viewNode.querySelector("table")); cellSel.afterAttach(); } else { cellSel.release(); gridSize?.release?.(); }
      for (const fn of changeListeners) fn();
    }

    function refreshGridCells() {
      const table = bound ? activeTable() : null;
      if (!table || table.view !== "grid" || !gridWin || !gridLive || gridLive.tableId !== table.id || !refs?.viewport?.contains(gridLive.wrap)) return false;
      relationOptions = new Map();
      const query = (liveQuery = queryOf(table.id)), { footCount, footSums } = gridFootTexts(query);
      const { growOnly } = gridSize ? gridSize.prepareAutoWidths(table, query, [footCount, ...footSums]) : { growOnly: false };
      cellSel.beforeRender();
      const winBefore = gridWin, liveBefore = gridLive;
      if (!winBefore.refresh(query, growOnly)) return false;
      if (gridWin !== winBefore || gridLive !== liveBefore) return true;
      if (gridLive.footCountTd.textContent !== footCount) gridLive.footCountTd.textContent = footCount;
      footSums.forEach((txt, i) => { if (gridLive.footSumTds[i] && gridLive.footSumTds[i].textContent !== txt) gridLive.footSumTds[i].textContent = txt; });
      const rc = winBefore.count;
      setAttr(gridLive.tblEl, "aria-rowcount", String(rc + 3)); setAttr(gridLive.newRowTr, "aria-rowindex", String(rc + 2)); setAttr(gridLive.footRow, "aria-rowindex", String(rc + 3));
      cellSel.bind({ tbl: gridLive.tblEl, wrap: gridLive.wrap, win: winBefore, fields: query.fields, tableId: table.id }, gridLive.viewKey);
      gridSize?.bind({ tbl: gridLive.tblEl, table, query, win: winBefore, footSums });
      cellSel.afterAttach();
      for (const fn of changeListeners) fn();
      return true;
    }
    // ---- Public Studio API ----
    function bind() { buildShellDom(); bound = true; host.__EASYOFFICE_BASE_IMPORT__?.bind?.(api, { document: doc, invokeIpc: deps.invokeIpc, setFeedback, shellChrome: deps.shellChrome }); render(); return api; }
    const getState = () => M.serializeBaseState(state);
    function setState(next) {
      const res = M.hydrateBaseState(next);
      if (!res.ok) { setFeedback(res.error); return res; }
      state = res.state; invalidCell = null; qCache?.reset(state); gridMemo.key = ""; if (gridSize) gridSize.clearAll(gridWidths); else gridWidths.clear();
      if (bound) render();
      return { ok: true, state: getState() };
    }

    function resetBlank() {
      state = M.createInitialBaseState(); invalidCell = null; qCache?.reset(state); queryPanel = ""; gridMemo.key = ""; if (gridSize) gridSize.clearAll(gridWidths); else gridWidths.clear();
      if (bound) render();
      return getState();
    }

    // One undo step per change that was made: the state before it is taken first and kept only when the change succeeded,
    // so a refused change (a validation error) leaves no empty step. A host without that split records before the change.
    const equalCellVal = (a, b) => a === b || (Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((x, i) => x === b[i]));
    const finishEdit = () => { invalidCell = null; noteActivity(); if (bound && !refreshGridCells()) render(); };
    const recordCellDelta = (tableId, edits) => edits.length > 0 && deps.recordCellUndo?.({ kind: "base-cells", activeModule: "BASE", tableId, edits });

    function runMut(fn, clearInvalid = false) {
      const before = deps.captureUndo?.();
      if (!before) recordUndo();
      const res = fn();
      if (!res.ok) { setFeedback(res.error); return res; }
      if (before) deps.commitUndo(before);
      if (clearInvalid) invalidCell = null;
      noteActivity(); if (bound) render();
      return res;
    }
    function withActive(fn, clearInvalid = false) {
      const tbl = activeTable();
      return tbl ? runMut(() => fn(tbl), clearInvalid) : { ok: false, error: "활성 테이블이 없습니다." };
    }

    // A view setting (the view, the board or calendar field, the month, a collapsed group) is saved with the model: one undo step each.
    const setViewSetting = (apply) => withActive((tbl) => { apply(tbl); return { ok: true }; });
    const addTable = (spec = {}) => runMut(() => { const r = M.addTable(state, spec); if (r.ok) qCache?.invalidateSchema(state); return r; }, true);
    const addField = (spec = {}) => withActive((t) => { const r = M.addField(state, t.id, spec); if (r.ok) qCache?.invalidateSchema(state); return r; });
    const updateField = (fieldId, patch = {}) => withActive((t) => { const r = M.updateField(state, t.id, fieldId, patch); if (r.ok) { gridSize?.invalidateTable(t.id, gridWidths); qCache?.invalidateSchema(state); } return r; });
    const deleteField = (fieldId) => withActive((t) => { const r = M.deleteField(state, t.id, fieldId); if (r.ok) { gridSize?.invalidateTable(t.id, gridWidths); qCache?.invalidateSchema(state); } return r; });
    const setFieldWidth = (fieldId, widthPx) => withActive((t) => M.setFieldWidth(state, t.id, fieldId, widthPx));
    const setRowHeight = (heightPx) => withActive((t) => M.setRowHeight(state, t.id, heightPx));
    const fitFieldWidth = (fieldId) => withActive((t) => { const px = gridSize?.fitField(queryOf(t.id), fieldId); return px ? M.setFieldWidth(state, t.id, fieldId, px) : { ok: false, error: "대상 필드를 찾을 수 없습니다." }; });
    const addRecord = (values = {}) => withActive((t) => { const r = M.addRecord(state, t.id, values, qCache?.index(state)); if (r.ok) qCache?.noteAdd(state, t.id, r.record); return r; }, true);
    const deleteRecord = (recordId) => withActive((t) => { const r = M.deleteRecord(state, t.id, recordId); if (r.ok) qCache?.noteDelete(state, t.id, recordId); return r; }, true);

    function updateCell(recordId, fieldId, value) {
      const tbl = activeTable(), idx = tbl ? qCache?.index(state) : null;
      if (!tbl) return { ok: false, error: "활성 테이블이 없습니다." };
      const prevRec = idx?.recordMapsByTableId.get(tbl.id)?.get(recordId) || tbl.records.find((r) => r.id === recordId);
      const had = Boolean(prevRec && Object.hasOwn(prevRec.values, fieldId));
      const prevCopy = had ? (Array.isArray(prevRec.values[fieldId]) ? [...prevRec.values[fieldId]] : prevRec.values[fieldId]) : undefined;
      const res = M.updateCell(state, tbl.id, recordId, fieldId, value, idx);
      if (!res.ok) { invalidCell = { recordId, fieldId, error: res.error }; setFeedback(res.error); return res; }
      if (prevRec && !equalCellVal(prevCopy, res.value)) {
        if (deps.recordCellUndo) recordCellDelta(tbl.id, [{ recordId, fieldId, value: prevCopy }]);
        else { const committed = res.value; if (had) prevRec.values[fieldId] = prevCopy; else delete prevRec.values[fieldId]; recordUndo(); prevRec.values[fieldId] = committed; }
      }
      qCache?.noteUpdate(state, tbl.id, [recordId]); finishEdit();
      return res;
    }

    // Many cells in one undo step and one redraw (a paste, Delete over a range): `edits` are `{ recordId, fieldId, value }`; the cells whose value the
    // field refuses are left as they are and counted in the feedback.
    function updateCells(edits) {
      const tbl = activeTable(), canDelta = Boolean(tbl && deps.recordCellUndo && qCache);
      const before = canDelta ? qCache.snapshotEdits(state, tbl.id, edits) : null;
      const run = (t) => { const r = M.updateCells(state, t.id, edits, qCache?.index(state)); if (r.ok) qCache?.noteUpdate(state, t.id, r.dirtyIds); return r; };
      let res;
      if (!canDelta) res = withActive(run, true);
      else { res = run(tbl); if (!res.ok) setFeedback(res.error); else { if (res.dirtyIds.length > 0) recordCellDelta(tbl.id, before); finishEdit(); } }
      if (res.ok && res.failed > 0) setFeedback(`${res.failed}칸은 값이 맞지 않아 바꾸지 못했습니다: ${res.error}`);
      return res;
    }

    function applyUndoEdits(tableId, edits) {
      const tbl = M.getTable(state, tableId), idx = tbl && Array.isArray(edits) && qCache ? qCache.index(state) : null;
      if (!tbl || !idx) return null;
      const inverse = qCache.snapshotEdits(state, tbl.id, edits), recs = idx.recordMapsByTableId.get(tbl.id), dirty = new Set();
      if (!recs || inverse.length === 0) return null;
      for (const e of edits) {
        const rec = recs.get(e?.recordId);
        if (!rec) continue;
        if (e.value === undefined) delete rec.values[e.fieldId]; else rec.values[e.fieldId] = Array.isArray(e.value) ? [...e.value] : e.value;
        dirty.add(e.recordId);
      }
      state.activeTableId = tbl.id; qCache.noteUpdate(state, tbl.id, Array.from(dirty)); finishEdit();
      return { kind: "base-cells", activeModule: "BASE", tableId: tbl.id, edits: inverse };
    }
    const applyPaste = (edits, newRows) => {
      const res = withActive((tbl) => { const r = host.__EASYOFFICE_BASE_CLIPBOARD__.applyPasteMutation(M, state, tbl, edits, newRows, qCache?.index(state)); if (r.ok) qCache?.notePaste(state, tbl.id, r.dirtyIds, r.createdRecords); return r; }, true);
      if (res.ok && res.failed > 0) setFeedback(`${res.failed}칸은 값이 맞지 않아 바꾸지 못했습니다: ${res.error}`);
      return res;
    };

    function setView(viewMode) {
      const tbl = activeTable();
      if (!tbl || !M.VIEW_MODES.has(viewMode)) return { ok: false, error: "유효하지 않은 보기 모드입니다." };
      if (tbl.view !== viewMode) setViewSetting((t) => { t.view = viewMode; if (viewMode === "grid") gridSize?.invalidateTable(t.id, gridWidths); });
      return { ok: true, view: viewMode };
    }

    const setFilters = (filters = [], conjunction = "and") => withActive((tbl) => {
      tbl.filters = Array.isArray(filters) ? filters.slice(0, 64).map((f) => ({
        fieldId: String(f?.fieldId || ""), op: M.FILTER_OPS.includes(f?.op) ? f.op : "contains", value: String(f?.value ?? ""),
      })) : [];
      tbl.filterConjunction = conjunction === "or" ? "or" : "and";
      gridSize?.invalidateTable(tbl.id, gridWidths);
      return { ok: true };
    });

    const setSorts = (sorts = []) => withActive((tbl) => {
      tbl.sorts = Array.isArray(sorts) ? sorts.slice(0, 16).map((s) => ({ fieldId: String(s?.fieldId || ""), dir: s?.dir === "desc" ? "desc" : "asc" })) : [];
      gridSize?.invalidateTable(tbl.id, gridWidths);
      return { ok: true };
    });

    const setGroupBy = (fieldId = "") => withActive((tbl) => {
      tbl.groupByFieldId = String(fieldId || "");
      tbl.collapsedGroups = [];
      gridSize?.invalidateTable(tbl.id, gridWidths);
      return { ok: true };
    });

    async function exportAs(format) {
      const fmt = String(format || "").trim().toLowerCase();
      if (fmt !== "xlsx" && fmt !== "csv" && fmt !== "tsv") {
        const error = "지원하지 않는 내보내기 형식입니다: " + fmt;
        setFeedback(error);
        return { ok: false, error };
      }
      const tbl = activeTable(), rawTitle = tbl?.name || "데이터베이스";
      const sanitize = host.__EASYOFFICE_SAVE_PAYLOAD__?.sanitizeFileBaseName;
      const fileBase = deps.getFileBase?.() || (sanitize ? sanitize(rawTitle, "데이터베이스") : rawTitle);
      const sheets = fmt === "xlsx" ? M.toSheetsPayload(state) : M.toSheetsPayload(state, { tableId: tbl?.id, single: true });
      let base64 = "", ipcRes = null;
      if (typeof deps.invokeIpc === "function") {
        try {
          ipcRes = await deps.invokeIpc("convert_document_format", JSON.stringify({ sourceFormat: "xlsx", targetFormat: fmt, title: fileBase, sheets }));
          base64 = String(ipcRes?.convertedBase64 || "");
        } catch (err) {
          const error = err?.message || String(err);
          setFeedback("내보내기 실패: " + error);
          return { ok: false, error };
        }
      } else if (fmt === "csv" || fmt === "tsv") {
        const bytes = new TextEncoder().encode(M.toDelimitedText(state, tbl?.id, fmt === "tsv" ? "\t" : ","));
        base64 = host.__EASYOFFICE_SHEET_UTILS__?.bytesToBase64?.(bytes) || (typeof Buffer !== "undefined" ? Buffer.from(bytes).toString("base64") : "");
      }
      if (!base64) { setFeedback("내보내기 결과가 비어 있습니다."); return { ok: false, error: "내보내기 결과가 비어 있습니다." }; }
      let delivery = null;
      if (typeof deps.deliverExport === "function") {
        delivery = await deps.deliverExport(base64, fileBase, fmt);
        if (delivery?.cancelled) { setFeedback("내보내기 취소: 저장 위치를 선택하지 않았습니다"); return { ok: false, cancelled: true }; }
        if (delivery && delivery.ok === false) { const error = delivery.error || "파일 저장에 실패했습니다."; setFeedback("내보내기 실패: " + error); return { ok: false, error }; }
      }
      setFeedback(`내보내기 완료 (${fileBase}.${fmt})`);
      return { ok: true, format: fmt, fileBase, base64, sheets, res: ipcRes, delivery };
    }

    // The printed pages of the active table: the rows are cut by the paper's height (every row is PRINT_ROW_MM high, the title is on
    // the first page only) and each page repeats the header row.
    function buildPrintPages(docArg, rules) {
      const d = docArg || doc, tbl = activeTable();
      if (!tbl) throw new Error("인쇄할 내용이 없습니다.");
      const { fields, rows } = queryOf(tbl.id);
      if (rows.length === 0) throw new Error("인쇄할 내용이 없습니다.");
      const wide = fields.length > 6, paperHeight = wide ? 210 : 297;
      const paperClass = rules?.classFor ? rules.classFor(wide ? 297 : 210, paperHeight) : "";
      const room = paperHeight - 2 * PRINT_MARGIN_MM - PRINT_GUARD_MM;
      const rowsOnFirst = Math.max(1, Math.floor((room - PRINT_TITLE_MM) / PRINT_ROW_MM) - 1);
      const rowsOnOther = Math.max(1, Math.floor(room / PRINT_ROW_MM) - 1);
      const pages = [];
      for (let from = 0; from < rows.length; ) {
        const first = pages.length === 0, take = first ? rowsOnFirst : rowsOnOther, page = d.createElement("div");
        page.className = ("eo-print-page base-print-page " + paperClass).trim();
        if (first) { const titleEl = d.createElement("h3"); titleEl.textContent = tbl.name || "데이터베이스"; titleEl.style.height = PRINT_TITLE_MM + "mm"; page.appendChild(titleEl); }
        const tableEl = d.createElement("table"), thead = d.createElement("thead"), htr = d.createElement("tr"), tbody = d.createElement("tbody");
        tableEl.className = "eo-print-sheet"; htr.style.height = PRINT_ROW_MM + "mm";
        for (const fld of fields) { const th = d.createElement("th"); th.textContent = fld.name || ""; htr.appendChild(th); }
        thead.appendChild(htr);
        for (const row of rows.slice(from, from + take)) {
          const tr = d.createElement("tr"); tr.style.height = PRINT_ROW_MM + "mm";
          for (const fld of fields) {
            const td = d.createElement("td"); if (fld.type === "number") td.className = "eo-print-num";
            td.textContent = row.displays[fld.id] ?? ""; tr.appendChild(td);
          }
          tbody.appendChild(tr);
        }
        tableEl.appendChild(thead); tableEl.appendChild(tbody); page.appendChild(tableEl);
        pages.push(page); from += take;
      }
      return pages;
    }

    const api = {
      bind, render, getState, setState, resetBlank,
      addTable, addField, updateField, deleteField, setFieldWidth, setRowHeight, fitFieldWidth, addRecord, updateCell, updateCells, applyUndoEdits, deleteRecord,
      selection: cellSel.selection, selectedRanges: cellSel.selectedRanges, runClipboardCommand: cellSel.runClipboardCommand, setView, setFilters, setSorts, setGroupBy, exportAs, buildPrintPages,
      liveQuery: () => liveQuery || (activeTable() ? queryOf(activeTable().id) : null),
      onChange: (fn) => (changeListeners.add(fn), () => changeListeners.delete(fn)),
      cellCount: () => state.tables.reduce((s, t) => s + t.records.length * t.fields.length, 0),
      activeCellTarget: cellSel.activeTarget, selectCellById: cellSel.selectRecordField, relationIds,
      runStateMutation: (fn) => runMut(() => { const r = fn(state); if (r?.ok) { qCache?.invalidateSchema(state); gridSize?.clearAll?.(gridWidths); gridMemo.key = ""; queryPanel = ""; } return r || { ok: false, error: "변경에 실패했습니다." }; }, true),
    };
    host.__EASYOFFICE_BASE_STUDIO__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_BASE_STUDIO__ = createBaseStudio;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
