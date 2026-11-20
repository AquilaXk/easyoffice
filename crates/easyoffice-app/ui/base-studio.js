// 참조 구현 (reference implementation): rhwp-studio (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`)
// 크롬·시트 탭·대화상자 프레임 및 glide-data-grid (`glideapps/glide-data-grid`, MIT, Copyright (c) 2021 typeguard, Inc.,
// commit 0875d78cc41535a39f8b417ff88ce1ebe129c363, `packages/core/src/cells/{bubble-cell,marker-cell,new-row-cell}.tsx`;
// see `THIRD_PARTY_NOTICES.md`) · 마이크로소프트 오피스 (Lists / Access) 공식 한국어 화면 명칭을 적용한 데이터베이스 스튜디오 UI.

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

    const el = (tag, cls, text) => {
      const n = doc.createElement(tag);
      if (cls) n.className = cls;
      if (text !== undefined) n.textContent = text;
      return n;
    };
    const appendAll = (p, ...nodes) => { for (const n of nodes) if (n) p.appendChild(n); return p; };
    const mkBtn = (cls, label, onClick, id, aria) => {
      const b = el("button", cls, label);
      b.type = "button";
      if (id) b.id = id;
      b.setAttribute("aria-label", aria || label);
      if (onClick) b.addEventListener("click", onClick);
      return b;
    };
    const mkSel = (cls, aria, id) => {
      const s = el("select", cls);
      if (id) s.id = id;
      if (aria) s.setAttribute("aria-label", aria);
      return s;
    };
    const mkInput = (id, type, cls, aria, placeholder) => {
      const inp = el("input", cls);
      inp.type = type;
      if (id) inp.id = id;
      inp.setAttribute("aria-label", aria);
      if (placeholder) inp.placeholder = placeholder;
      return inp;
    };
    const addOpt = (sel, value, text) => { const o = el("option", "", text); o.value = value; sel.appendChild(o); return o; };
    const setAttr = (node, k, v) => { node.setAttribute(k, v); return node; };

    const activeTable = () => M.getTable(state, state.activeTableId);
    const setFeedback = (msg) => { deps.setFeedback?.(String(msg || "")); };
    const recordUndo = () => { deps.recordUndo?.(); };
    const noteActivity = () => { deps.noteActivity?.(); };
    const byId = (id) => doc.getElementById(id);

    function ensureRoot() {
      if (rootEl) {
        rootEl.classList.add("base-pane");
        if (!rootEl.getAttribute("aria-label")) rootEl.setAttribute("aria-label", "데이터베이스 편집기");
        return rootEl;
      }
      const existing = byId("pane-base");
      if (existing) { rootEl = existing; rootEl.classList.add("base-pane"); return rootEl; }
      const pane = setAttr(el("section", "editor-pane base-pane"), "aria-label", "데이터베이스 편집기");
      pane.id = "pane-base"; pane.hidden = true;
      (doc.querySelector(".canvas-stage") || doc.body)?.appendChild(pane);
      rootEl = pane;
      return rootEl;
    }

    const openDialog = (dlg) => {
      if (!dlg) return;
      if (typeof dlg.showModal === "function") { if (!dlg.open) dlg.showModal(); } else dlg.setAttribute("open", "");
    };
    const closeDialog = (dlg) => {
      if (!dlg) return;
      if (typeof dlg.close === "function") dlg.close(); else dlg.removeAttribute("open");
    };

    function buildShellDom() {
      const container = ensureRoot();
      if (refs && container.contains(refs.viewport)) return refs;

      const toolbar = setAttr(setAttr(el("div", "eo-doc-style-bar base-toolbar"), "role", "toolbar"), "aria-label", "데이터베이스 도구 모음");
      toolbar.id = "base-toolbar";
      const grpEdit = appendAll(el("div", "tb-group"),
        mkBtn("tb-btn base-tb-btn", "새 레코드", () => addRecord({}), "btn-base-add-record"),
        mkBtn("tb-btn base-tb-btn", "필드 추가", () => openFieldDialog(null), "btn-base-add-field")
      );
      const sep1 = setAttr(el("span", "tb-sep"), "aria-hidden", "true");
      const btnFilter = setAttr(mkBtn("tb-btn base-tb-btn", "필터", () => toggleQueryPanel("filter"), "btn-base-filter"), "aria-pressed", "false");
      const btnSort = setAttr(mkBtn("tb-btn base-tb-btn", "정렬", () => toggleQueryPanel("sort"), "btn-base-sort"), "aria-pressed", "false");
      const groupSelect = mkSel("sb-select base-group-select", "그룹화 기준", "base-group-select");
      groupSelect.addEventListener("change", () => setGroupBy(groupSelect.value));
      const grpQuery = appendAll(el("div", "tb-group"), btnFilter, btnSort, groupSelect);
      const sep2 = setAttr(el("span", "tb-sep"), "aria-hidden", "true");
      const grpExport = appendAll(el("div", "tb-group base-export-group"),
        mkBtn("tb-btn base-tb-btn", "XLSX로 내보내기", () => { void exportAs("xlsx"); }, "btn-base-export-xlsx"),
        mkBtn("tb-btn base-tb-btn", "CSV로 내보내기", () => { void exportAs("csv"); }, "btn-base-export-csv"),
        mkBtn("tb-btn base-tb-btn", "TSV로 내보내기", () => { void exportAs("tsv"); }, "btn-base-export-tsv")
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

      const queryBar = el("div", "base-query-bar");
      queryBar.id = "base-query-bar"; queryBar.hidden = true;
      const viewport = el("div", "base-viewport");
      viewport.id = "base-viewport";

      const bottomBar = setAttr(setAttr(el("div", "sheet-tab-bar base-sheet-tab-bar"), "role", "region"), "aria-label", "테이블 탭");
      const btnAddTable = mkBtn("sheet-tab-btn sheet-tab-add base-add-table-btn", "+ 새 테이블", () => addTable({}), "btn-base-add-table", "새 테이블");
      const tableTabs = setAttr(setAttr(el("div", "sheet-tabs base-table-tabs"), "role", "tablist"), "aria-label", "테이블 목록");
      tableTabs.id = "base-table-tabs";
      appendAll(bottomBar, btnAddTable, tableTabs);

      byId("base-field-dialog")?.remove?.();
      const fieldDlg = buildFieldDialogDom();
      doc.body.appendChild(fieldDlg);

      container.replaceChildren(toolbar, queryBar, viewport, bottomBar);
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

      const nameInput = mkInput("base-field-name", "text", "dialog-text-input base-dlg-input", "필드 이름");
      const typeSelect = mkSel("sb-select base-dlg-select", "필드 유형", "base-field-type");
      for (const ft of M.FIELD_TYPES) addOpt(typeSelect, ft, M.FIELD_TYPE_LABELS[ft]);
      const optsInput = mkInput("base-field-options", "text", "dialog-text-input base-dlg-input", "선택 옵션 (쉼표 구분)", "대기, 진행 중, 완료");
      const relSelect = mkSel("sb-select base-dlg-select", "연결 테이블", "base-field-relation-table");
      const formulaInput = mkInput("base-field-formula", "text", "dialog-text-input base-dlg-input", "수식", "예: ROUND({금액} * 1.1, 0)");

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

      const btnDel = mkBtn("dialog-btn base-danger-btn", "필드 삭제", () => {
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
    }

    // ---- Cell Controls & Invalid Handling ----

    function markInvalid(td, control, msg) {
      td.dataset.invalid = "true"; td.setAttribute("aria-invalid", "true"); td.title = msg;
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
        const targetTbl = query.index.tablesById.get(field.relationTableId), primary = targetTbl?.fields?.[0];
        const sel = mkSel("base-cell-select", label);
        addOpt(sel, "", "(연결 없음)");
        for (const tRec of targetTbl?.records || []) {
          const optLbl = primary ? M.formatCellDisplay(primary, tRec.values?.[primary.id], state, query.index) : tRec.id;
          addOpt(sel, tRec.id, optLbl || tRec.id);
        }
        const curSingle = Array.isArray(val) ? (val[0] || "") : String(val || "");
        sel.value = curSingle;
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
        td.appendChild(link);
      }
    }

    // ---- 3 Synchronized View Renderers ----

    function renderGridView(table, query) {
      const wrap = el("div", "base-grid-wrap"), tblEl = setAttr(el("table", "base-grid-table"), "aria-label", "데이터시트 보기");
      const thead = el("thead"), headRow = el("tr"), cornerTh = setAttr(el("th", "base-grid-corner", "#"), "scope", "col");
      headRow.appendChild(cornerTh);

      for (const field of query.fields) {
        const th = setAttr(el("th", "base-grid-th"), "scope", "col");
        th.dataset.fieldId = field.id;
        const inner = appendAll(el("div", "base-th-inner"),
          el("span", "base-th-name", field.name),
          el("span", "base-th-type", M.FIELD_TYPE_LABELS[field.type] || field.type),
          mkBtn("base-field-edit-btn", "▾", () => openFieldDialog(field), "", `필드 설정: ${field.name}`)
        );
        th.appendChild(inner); headRow.appendChild(th);
      }
      thead.appendChild(headRow);

      const tbody = el("tbody");
      let rowNum = 0;
      for (const group of query.groups) {
        if (table.groupByFieldId) {
          const gr = el("tr", "base-group-row"), gtd = el("td", "base-group-cell");
          gr.dataset.groupKey = group.key; gtd.colSpan = query.fields.length + 1;
          const arrow = group.collapsed ? "▸ " : "▾ ";
          const toggle = setAttr(mkBtn("base-group-toggle", `${arrow}${group.label} (${group.count}건)`, () => {
            const set = new Set(table.collapsedGroups || []);
            if (set.has(group.key)) set.delete(group.key); else set.add(group.key);
            table.collapsedGroups = Array.from(set);
            render();
          }, "", `그룹 접기/펼치기: ${group.label}`), "aria-expanded", group.collapsed ? "false" : "true");
          gtd.appendChild(toggle); gr.appendChild(gtd); tbody.appendChild(gr);
        }
        if (group.collapsed) continue;
        for (const rowData of group.records) {
          const tr = el("tr", "base-grid-row");
          tr.dataset.recordId = rowData.record.id;
          tr.appendChild(appendAll(el("td", "base-row-index"),
            el("span", "base-row-num", String(rowNum + 1)),
            mkBtn("base-row-delete-btn", "×", () => deleteRecord(rowData.record.id), "", "레코드 삭제")
          ));
          for (const field of query.fields) {
            const td = el("td", "base-grid-cell");
            td.dataset.fieldId = field.id; td.dataset.fieldType = field.type;
            buildCellControl(td, rowData, field, rowNum, query);
            tr.appendChild(td);
          }
          tbody.appendChild(tr); rowNum += 1;
        }
      }

      const newRowTr = el("tr", "base-new-record-row");
      const newRowIdx = el("td", "base-row-index base-new-row-marker", "+");
      const newRowCell = el("td", "base-new-record-cell");
      newRowCell.colSpan = query.fields.length;
      newRowCell.appendChild(mkBtn("base-inline-add-btn", "새 레코드", () => addRecord({}), "", "새 레코드 추가"));
      appendAll(newRowTr, newRowIdx, newRowCell);
      tbody.appendChild(newRowTr);

      const tfoot = el("tfoot"), footRow = el("tr", "base-grid-footer");
      footRow.appendChild(el("td", "base-footer-count", query.filteredCount === query.totalCount ? `개수: ${query.totalCount}건` : `개수: ${query.filteredCount} / ${query.totalCount}건`));
      for (const field of query.fields) {
        const sum = query.numericSums[field.id];
        const hasSum = field.type === "number" || (field.type === "formula" && query.rows.some((r) => typeof r.values[field.id] === "number"));
        footRow.appendChild(el("td", "base-footer-sum", hasSum && Number.isFinite(sum) ? `합계: ${Math.round(sum * 1000) / 1000}` : ""));
      }
      tfoot.appendChild(footRow);
      appendAll(tblEl, thead, tbody, tfoot);
      wrap.appendChild(tblEl);
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
      sel.addEventListener("change", () => { table.kanbanFieldId = sel.value; noteActivity(); render(); });
      wrap.appendChild(appendAll(el("div", "base-subview-bar"), lbl, sel));
      if (!chosen) { wrap.appendChild(el("p", "base-empty-hint", "보드로 분류할 필드가 없습니다.")); return wrap; }

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
        const next = keysOf(rec.values[chosen.id]).filter((k) => k !== "" && k !== fromKey);
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
      fSel.addEventListener("change", () => { table.calendarFieldId = fSel.value; noteActivity(); render(); });

      const { year, month } = parseMonthStr(table.calendarMonth);
      const calTitle = el("span", "base-cal-title", `${year}년 ${month}월`);
      calTitle.id = "base-cal-title";
      const navGroup = appendAll(el("div", "base-cal-nav"),
        mkBtn("dialog-btn", "이전 달", () => { table.calendarMonth = stepMonthStr(table.calendarMonth, -1); noteActivity(); render(); }, "btn-base-cal-prev"),
        calTitle,
        mkBtn("dialog-btn", "다음 달", () => { table.calendarMonth = stepMonthStr(table.calendarMonth, 1); noteActivity(); render(); }, "btn-base-cal-next"),
        mkBtn("dialog-btn", "오늘", () => { table.calendarMonth = currentMonthStr(); noteActivity(); render(); }, "btn-base-cal-today")
      );
      wrap.appendChild(appendAll(el("div", "base-subview-bar base-cal-header"), lbl, fSel, navGroup));
      if (!chosen) { wrap.appendChild(el("p", "base-empty-hint", "일정 보기를 사용하려면 날짜 유형 필드를 추가하세요.")); return wrap; }

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
      const table = activeTable();
      if (!table) return;
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
      refs.btnFilter.textContent = fCount > 0 ? `필터 (${fCount})` : "필터";
      refs.btnFilter.setAttribute("aria-pressed", String(queryPanel === "filter" || fCount > 0));
      refs.btnSort.textContent = sCount > 0 ? `정렬 (${sCount})` : "정렬";
      refs.btnSort.setAttribute("aria-pressed", String(queryPanel === "sort" || sCount > 0));

      refs.groupSelect.replaceChildren();
      addOpt(refs.groupSelect, "", "그룹화 기준: 없음");
      for (const f of table.fields) addOpt(refs.groupSelect, f.id, `그룹화 기준: ${f.name}`);
      refs.groupSelect.value = table.groupByFieldId || "";

      renderQueryBar(table);
      const query = M.queryTableView(state, table.id);
      const viewNode = table.view === "kanban" ? renderKanbanView(table, query)
        : table.view === "calendar" ? renderCalendarView(table, query)
        : renderGridView(table, query);
      refs.viewport.replaceChildren(viewNode);
    }

    // ---- Public Studio API ----

    function bind() { buildShellDom(); bound = true; render(); return api; }
    const getState = () => M.serializeBaseState(state);

    function setState(next) {
      const res = M.hydrateBaseState(next);
      if (!res.ok) { setFeedback(res.error); return res; }
      state = res.state; invalidCell = null;
      if (bound) render();
      return { ok: true, state: getState() };
    }

    function resetBlank() {
      state = M.createInitialBaseState(); invalidCell = null; queryPanel = "";
      if (bound) render();
      return getState();
    }

    function runMut(fn, clearInvalid = false) {
      recordUndo();
      const res = fn();
      if (!res.ok) { setFeedback(res.error); return res; }
      if (clearInvalid) invalidCell = null;
      noteActivity();
      if (bound) render();
      return res;
    }
    function withActive(fn, clearInvalid = false) {
      const tbl = activeTable();
      return tbl ? runMut(() => fn(tbl), clearInvalid) : { ok: false, error: "활성 테이블이 없습니다." };
    }

    const addTable = (spec = {}) => runMut(() => M.addTable(state, spec), true);
    const addField = (spec = {}) => withActive((t) => M.addField(state, t.id, spec));
    const updateField = (fieldId, patch = {}) => withActive((t) => M.updateField(state, t.id, fieldId, patch));
    const deleteField = (fieldId) => withActive((t) => M.deleteField(state, t.id, fieldId));
    const addRecord = (values = {}) => withActive((t) => M.addRecord(state, t.id, values), true);
    const deleteRecord = (recordId) => withActive((t) => M.deleteRecord(state, t.id, recordId), true);

    function updateCell(recordId, fieldId, value) {
      const tbl = activeTable();
      if (!tbl) return { ok: false, error: "활성 테이블이 없습니다." };
      const prevRec = tbl.records.find((r) => r.id === recordId);
      const prevCopy = prevRec ? (Array.isArray(prevRec.values[fieldId]) ? [...prevRec.values[fieldId]] : prevRec.values[fieldId]) : undefined;
      const res = M.updateCell(state, tbl.id, recordId, fieldId, value);
      if (!res.ok) { invalidCell = { recordId, fieldId, error: res.error }; setFeedback(res.error); return res; }
      if (prevRec && prevCopy !== res.value) {
        const committed = res.value;
        prevRec.values[fieldId] = prevCopy;
        recordUndo();
        prevRec.values[fieldId] = committed;
      }
      invalidCell = null; noteActivity();
      if (bound) render();
      return res;
    }

    function setView(viewMode) {
      const tbl = activeTable();
      if (!tbl || !M.VIEW_MODES.has(viewMode)) return { ok: false, error: "유효하지 않은 보기 모드입니다." };
      tbl.view = viewMode; noteActivity();
      if (bound) render();
      return { ok: true, view: viewMode };
    }

    const setFilters = (filters = [], conjunction = "and") => withActive((tbl) => {
      tbl.filters = Array.isArray(filters) ? filters.slice(0, 64).map((f) => ({
        fieldId: String(f?.fieldId || ""), op: M.FILTER_OPS.includes(f?.op) ? f.op : "contains", value: String(f?.value ?? ""),
      })) : [];
      tbl.filterConjunction = conjunction === "or" ? "or" : "and";
      return { ok: true };
    });

    const setSorts = (sorts = []) => withActive((tbl) => {
      tbl.sorts = Array.isArray(sorts) ? sorts.slice(0, 16).map((s) => ({ fieldId: String(s?.fieldId || ""), dir: s?.dir === "desc" ? "desc" : "asc" })) : [];
      return { ok: true };
    });

    const setGroupBy = (fieldId = "") => withActive((tbl) => {
      tbl.groupByFieldId = String(fieldId || "");
      tbl.collapsedGroups = [];
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
        if (delivery && delivery.ok === false) {
          const error = delivery.error || "파일 저장에 실패했습니다.";
          setFeedback("내보내기 실패: " + error);
          return { ok: false, error };
        }
      }
      setFeedback(`내보내기 완료 (${fileBase}.${fmt})`);
      return { ok: true, format: fmt, fileBase, base64, sheets, res: ipcRes, delivery };
    }

    // The printed pages of the active table: the rows are cut by the paper's height (every row is PRINT_ROW_MM high, the title is on
    // the first page only) and each page repeats the header row.
    function buildPrintPages(docArg, rules) {
      const d = docArg || doc, tbl = activeTable();
      if (!tbl) throw new Error("인쇄할 내용이 없습니다.");
      const { fields, rows } = M.queryTableView(state, tbl.id);
      if (rows.length === 0) throw new Error("인쇄할 내용이 없습니다.");
      const wide = fields.length > 6, paperHeight = wide ? 210 : 297;
      const paperClass = rules?.classFor ? rules.classFor(wide ? 297 : 210, paperHeight) : "";
      const room = paperHeight - 2 * PRINT_MARGIN_MM - PRINT_GUARD_MM;
      const rowsOnFirst = Math.max(1, Math.floor((room - PRINT_TITLE_MM) / PRINT_ROW_MM) - 1);
      const rowsOnOther = Math.max(1, Math.floor(room / PRINT_ROW_MM) - 1);
      const pages = [];
      for (let from = 0; from < rows.length; ) {
        const first = pages.length === 0, take = first ? rowsOnFirst : rowsOnOther;
        const page = d.createElement("div");
        page.className = ("eo-print-page base-print-page " + paperClass).trim();
        if (first) {
          const titleEl = d.createElement("h3");
          titleEl.textContent = tbl.name || "데이터베이스";
          titleEl.style.height = PRINT_TITLE_MM + "mm";
          page.appendChild(titleEl);
        }
        const tableEl = d.createElement("table");
        tableEl.className = "eo-print-sheet";
        const thead = d.createElement("thead"), htr = d.createElement("tr");
        htr.style.height = PRINT_ROW_MM + "mm";
        for (const fld of fields) {
          const th = d.createElement("th");
          th.textContent = fld.name || "";
          htr.appendChild(th);
        }
        thead.appendChild(htr);
        const tbody = d.createElement("tbody");
        for (const row of rows.slice(from, from + take)) {
          const tr = d.createElement("tr");
          tr.style.height = PRINT_ROW_MM + "mm";
          for (const fld of fields) {
            const td = d.createElement("td");
            if (fld.type === "number") td.className = "eo-print-num";
            td.textContent = row.displays[fld.id] ?? "";
            tr.appendChild(td);
          }
          tbody.appendChild(tr);
        }
        tableEl.appendChild(thead);
        tableEl.appendChild(tbody);
        page.appendChild(tableEl);
        pages.push(page);
        from += take;
      }
      return pages;
    }

    const api = {
      bind, render, getState, setState, resetBlank,
      addTable, addField, updateField, deleteField, addRecord, updateCell, deleteRecord,
      setView, setFilters, setSorts, setGroupBy, exportAs, buildPrintPages,
    };
    host.__EASYOFFICE_BASE_STUDIO__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_BASE_STUDIO__ = createBaseStudio;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
