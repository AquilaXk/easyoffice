(function (host) {
  "use strict";

  // Extended data bar controls (direction, border, axis) and Table/Pivot scope lookup & clear-menu
  // enablement for the conditional formatting dialogs and menu (PD-347).

  const BAR_DIRS = Object.freeze(["context", "leftToRight", "rightToLeft"]);
  const BAR_AXES = Object.freeze(["automatic", "midpoint", "none"]);
  const BAR_COLOR_FIELDS = Object.freeze([
    "sheet-cf-bar-pos",
    "sheet-cf-bar-neg",
    "sheet-cf-bar-bpos",
    "sheet-cf-bar-bneg",
    "sheet-cf-bar-axis-color",
  ]);

  function defaultBarForm(kit) {
    const { setVal, setChecked, setSelect } = kit;
    setSelect("sheet-cf-bar-min-kind", "min");
    setVal("sheet-cf-bar-min-val", "");
    setSelect("sheet-cf-bar-max-kind", "max");
    setVal("sheet-cf-bar-max-val", "");
    setVal("sheet-cf-bar-pos", "#638ec6");
    setVal("sheet-cf-bar-neg", "#ff0000");
    setChecked("sheet-cf-bar-grad", true);
    setSelect("sheet-cf-bar-dir", "context");
    setSelect("sheet-cf-bar-border", "none");
    setVal("sheet-cf-bar-bpos", "#638ec6");
    setVal("sheet-cf-bar-bneg", "#ff0000");
    setSelect("sheet-cf-bar-axis", "automatic");
    setVal("sheet-cf-bar-axis-color", "#000000");
  }

  function fillBarForm(kit, rule) {
    const { setVal, setChecked, setSelect } = kit;
    const pos = (rule.pos || "#638ec6").toLowerCase();
    const neg = (rule.neg || "#ff0000").toLowerCase();
    setSelect("sheet-cf-bar-min-kind", rule.min.k);
    setVal("sheet-cf-bar-min-val", rule.min.v);
    setSelect("sheet-cf-bar-max-kind", rule.max.k);
    setVal("sheet-cf-bar-max-val", rule.max.v);
    setVal("sheet-cf-bar-pos", pos);
    setVal("sheet-cf-bar-neg", neg);
    setChecked("sheet-cf-bar-grad", rule.grad !== false);
    setChecked("sheet-cf-show", rule.show !== false);
    setSelect("sheet-cf-bar-dir", BAR_DIRS.includes(rule.dir) ? rule.dir : "context");
    setSelect("sheet-cf-bar-border", rule.bdOn ? "solid" : "none");
    setVal("sheet-cf-bar-bpos", (rule.bpos || pos).toLowerCase());
    setVal("sheet-cf-bar-bneg", (rule.bneg || neg).toLowerCase());
    setSelect("sheet-cf-bar-axis", BAR_AXES.includes(rule.axis) ? rule.axis : "automatic");
    setVal("sheet-cf-bar-axis-color", (rule.axisColor || "#000000").toLowerCase());
  }

  function readBarForm(kit, colorOf) {
    const { val, checked } = kit;
    const pos = colorOf("sheet-cf-bar-pos", "#638ec6");
    const neg = colorOf("sheet-cf-bar-neg", "#ff0000");
    const dir = val("sheet-cf-bar-dir");
    const axis = val("sheet-cf-bar-axis");
    return {
      min: { k: val("sheet-cf-bar-min-kind"), v: val("sheet-cf-bar-min-val") },
      max: { k: val("sheet-cf-bar-max-kind"), v: val("sheet-cf-bar-max-val") },
      pos,
      neg,
      grad: checked("sheet-cf-bar-grad"),
      show: checked("sheet-cf-show"),
      dir: BAR_DIRS.includes(dir) ? dir : "context",
      bdOn: val("sheet-cf-bar-border") === "solid",
      bpos: colorOf("sheet-cf-bar-bpos", pos),
      bneg: colorOf("sheet-cf-bar-bneg", neg),
      axis: BAR_AXES.includes(axis) ? axis : "automatic",
      axisColor: colorOf("sheet-cf-bar-axis-color", "#000000"),
    };
  }

  function activeCellPos1(getRange, getSelection) {
    const sel = typeof getSelection === "function" ? getSelection() : null;
    if (sel && Number.isInteger(sel.ar) && sel.ar >= 1 && Number.isInteger(sel.ac) && sel.ac >= 1) {
      return { row: sel.ar, col: sel.ac };
    }
    const r = getRange();
    return { row: r.r1, col: r.c1 };
  }

  function toZeroBox(box) {
    if (!box || box.r2 < box.r1 || box.c2 < box.c1) return null;
    return { row0: box.r1 - 1, col0: box.c1 - 1, row1: box.r2 - 1, col1: box.c2 - 1 };
  }

  function activeTableBox(sheet, getRange, getSelection) {
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    if (!layout) return null;
    const pos = activeCellPos1(getRange, getSelection);
    const tbl =
      host.__EASYOFFICE_SHEET_TABLE_STYLE__?.tableAt?.(sheet, pos.row, pos.col) ||
      layout.tablesIn?.(sheet, pos.row, pos.col, pos.row, pos.col)?.[0] ||
      null;
    return tbl && layout.tableBox ? toZeroBox(layout.tableBox(tbl)) : null;
  }

  function activePivotBox(sheet, getRange, getSelection) {
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    if (!layout?.pivotAt || !layout.pivotBox) return null;
    const pos = activeCellPos1(getRange, getSelection);
    const pv = layout.pivotAt(sheet, pos.row, pos.col);
    return pv ? toZeroBox(layout.pivotBox(pv)) : null;
  }

  function syncScopeOptions(document, hasTable, hasPivot) {
    const select = document.getElementById("sheet-cf-scope-select");
    if (!select) return;
    for (const [value, enabled] of [["table", hasTable], ["pivot", hasPivot]]) {
      const opt = select.querySelector(`option[value="${value}"]`);
      if (opt) opt.disabled = !enabled;
    }
    host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(select);
  }

  function syncClearMenu(document, hasTable, hasPivot) {
    let changed = false;
    const setItem = (action, enabled) => {
      const next = enabled ? "false" : "true";
      for (const el of Array.from(document.querySelectorAll(`.md-item[data-sheet-cf="${action}"]`))) {
        if (el.getAttribute("aria-disabled") !== next) {
          el.setAttribute("aria-disabled", next);
          changed = true;
        }
      }
    };
    setItem("clear-table", hasTable);
    setItem("clear-pivot", hasPivot);
    return changed;
  }

  host.__EASYOFFICE_SHEET_CF_BAR_UI__ = {
    BAR_DIRS,
    BAR_AXES,
    BAR_COLOR_FIELDS,
    defaultBarForm,
    fillBarForm,
    readBarForm,
    activeTableBox,
    activePivotBox,
    syncScopeOptions,
    syncClearMenu,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
