(function (host) {
  "use strict";

  // Dialogs and in-cell behavior of the sheet rules (PD-171): the conditional formatting rule
  // manager, data validation settings (with the entry check and the list arrow), and the pivot
  // table dialog. The dialogs are static markup in index.html; this file reads and fills them.

  const rules = host.__EASYOFFICE_SHEET_RULES__;

  const VALUE_KINDS = [
    ["min", "최소값"],
    ["max", "최대값"],
    ["num", "숫자"],
    ["percent", "백분율"],
    ["percentile", "백분위수"],
    ["formula", "수식"],
  ];
  // Rule types that format by style (the rest draw their own colors, bars or icons).
  const STYLE_TYPES = new Set(["cellIs", "text", "top", "average", "dup", "expr", "blanks", "errors", "period"]);
  const CMP_LABELS = {
    between: "사이",
    notBetween: "사이 아님",
    equal: "=",
    notEqual: "≠",
    greaterThan: ">",
    greaterThanOrEqual: "≥",
    lessThan: "<",
    lessThanOrEqual: "≤",
  };
  const TYPE_LABELS = {
    cellIs: "셀 값",
    text: "텍스트",
    top: "상위/하위",
    average: "평균",
    dup: "중복/고유",
    expr: "수식",
    blanks: "빈 셀",
    errors: "오류",
    period: "날짜",
    scale: "색조",
    bar: "데이터 막대",
    icons: "아이콘 집합",
  };
  const PERIOD_LABELS = {
    yesterday: "어제",
    today: "오늘",
    tomorrow: "내일",
    last7Days: "지난 7일",
    lastWeek: "지난주",
    thisWeek: "이번 주",
    nextWeek: "다음 주",
    lastMonth: "지난달",
    thisMonth: "이번 달",
    nextMonth: "다음 달",
  };
  const COLOR_FIELDS = [
    "sheet-cf-stop0-color",
    "sheet-cf-stop1-color",
    "sheet-cf-stop2-color",
    "sheet-cf-bar-pos",
    "sheet-cf-bar-neg",
    "sheet-cf-bg",
    "sheet-cf-fg",
  ];
  const LIST_KINDS = new Set(["list", "listMultiple"]);
  const DEFAULT_MESSAGE = "입력한 값이 이 셀에 설정된 제한을 만족하지 않습니다.";

  const strip = (text) => String(text ?? "").trim().replace(/^=/, "");

  function createSheetRulesUi(deps) {
    const {
      document,
      sheetDataStore,
      sheetRowCounts,
      getActiveSheetName,
      getRange,
      isFormulaStr,
      sheetStoreHelpers,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
      flushFocusedSheetCellBeforeSwitch,
      performSwitchSheetTabAsync,
      applyCellEdits,
      readTdValue,
    } = deps;

    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const { $, val, checked, setVal, setChecked, bindColors, setSelect, openDialog, closeDialog, fillOptions, note } = kit;

    // The current selection as "A1:B9" (1-based grid range -> sqref).
    function selectionText() {
      const r = getRange();
      return rules.formatRanges([{ row0: r.r1 - 1, col0: r.c1 - 1, row1: r.r2 - 1, col1: r.c2 - 1 }]);
    }

    // Applies `mutate` to a copy of the active sheet's rules as one undoable step.
    function commitRules(mutate) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        const sheet = getActiveSheetName();
        const next = structuredClone(rules.get(sheet));
        mutate(next);
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        rules.set(sheet, next);
        return true;
      });
    }

    function colorOf(id, fallback) {
      return val(id) || fallback;
    }

    // ---- conditional formatting dialog ---------------------------------------------------------

    const cf = { selected: -1, ready: false };

    function describe(rule) {
      const kind = TYPE_LABELS[rule.t] || rule.t;
      let detail = "";
      if (rule.t === "cellIs") detail = " " + (CMP_LABELS[rule.op] || rule.op) + " " + rule.a + (rule.b ? "~" + rule.b : "");
      else if (rule.t === "text") detail = " " + rule.op + " " + rule.text;
      else if (rule.t === "top") detail = (rule.bottom ? " 하위 " : " 상위 ") + rule.n + (rule.percent ? "%" : "");
      else if (rule.t === "expr") detail = " " + rule.formula;
      else if (rule.t === "period") detail = " " + (PERIOD_LABELS[rule.period] || rule.period);
      else if (rule.t === "icons") detail = " " + (rules.ICON_SET_LABELS[rule.set] || rule.set);
      return rule.r + " · " + kind + detail;
    }

    function refreshKindRows(dialogId, type, styleTypes) {
      const dlg = $(dialogId);
      if (!dlg) return;
      for (const row of dlg.querySelectorAll("[data-cf-kinds]")) {
        const kinds = row.dataset.cfKinds.split(/\s+/);
        row.hidden = !(kinds.includes(type) || (styleTypes && kinds.includes("style") && styleTypes.has(type)));
      }
    }

    function valueRows() {
      return [
        ["sheet-cf-stop0-kind", "sheet-cf-stop0-val"],
        ["sheet-cf-stop1-kind", "sheet-cf-stop1-val"],
        ["sheet-cf-stop2-kind", "sheet-cf-stop2-val"],
        ["sheet-cf-bar-min-kind", "sheet-cf-bar-min-val"],
        ["sheet-cf-bar-max-kind", "sheet-cf-bar-max-val"],
      ];
    }

    function iconCount() {
      const set = val("sheet-cf-icon-set-select") || rules.ICON_SET_NAMES[0];
      return Number(set[0]) || 3;
    }

    // One threshold row per icon (the lowest icon needs none).
    function buildThresholdRows(thresholds) {
      const box = $("sheet-cf-icon-th");
      if (!box) return;
      box.replaceChildren();
      const n = iconCount();
      for (let i = 1; i < n; i += 1) {
        const row = document.createElement("div");
        row.className = "dialog-row";
        const label = document.createElement("label");
        label.className = "dialog-label";
        label.textContent = "아이콘 " + (i + 1);
        const kind = document.createElement("select");
        kind.id = "sheet-cf-icon-kind-" + i;
        kind.className = "sb-combo dialog-select";
        const input = document.createElement("input");
        input.id = "sheet-cf-icon-val-" + i;
        input.type = "text";
        input.className = "dialog-text-input";
        input.setAttribute("aria-label", "아이콘 " + (i + 1) + " 기준 값");
        label.htmlFor = kind.id;
        for (const [value, text] of VALUE_KINDS.filter(([v]) => v !== "min" && v !== "max")) {
          const opt = document.createElement("option");
          opt.value = value;
          opt.textContent = text;
          kind.appendChild(opt);
        }
        const preset = thresholds?.[i] || { k: "percent", v: String(Math.round((100 * i) / n)) };
        kind.value = preset.k;
        input.value = preset.v ?? "";
        for (const part of [label, kind, input]) row.appendChild(part);
        box.appendChild(row);
      }
      host.__EASYOFFICE_CUSTOM_SELECT__?.enhanceCustomSelects?.(document, box);
    }

    function readThresholds() {
      const out = [{ k: "percent", v: "0" }];
      for (let i = 1; i < iconCount(); i += 1) {
        out.push({ k: val("sheet-cf-icon-kind-" + i) || "percent", v: val("sheet-cf-icon-val-" + i) });
      }
      return out;
    }

    // What the dialog shows for a rule kind: filled selects and the rows of the chosen kind. Bound
    // when the page starts, so the dialog is right however it gets opened.
    function bindCfLayout() {
      for (const [kindId] of valueRows()) fillOptions(kindId, VALUE_KINDS);
      fillOptions("sheet-cf-period-select", Object.entries(PERIOD_LABELS));
      fillOptions("sheet-cf-icon-set-select", rules.ICON_SET_NAMES.map((n) => [n, rules.ICON_SET_LABELS[n]]));
      $("sheet-cf-type-select")?.addEventListener("change", () => {
        const type = val("sheet-cf-type-select");
        refreshKindRows("sheet-cf-dialog", type, STYLE_TYPES);
        if (type === "icons" && !$("sheet-cf-icon-th")?.children.length) buildThresholdRows();
      });
      $("sheet-cf-icon-set-select")?.addEventListener("change", () => buildThresholdRows());
    }

    // Buttons and colour popups, bound when the dialog is first opened.
    function ensureCfControls() {
      if (cf.ready) return;
      cf.ready = true;
      bindColors(COLOR_FIELDS);
      $("btn-sheet-cf-apply")?.addEventListener("click", () => void applyCfForm());
      $("btn-sheet-cf-new")?.addEventListener("click", () => selectCf(-1));
      $("btn-sheet-cf-delete")?.addEventListener("click", () => void deleteCf());
      $("btn-sheet-cf-up")?.addEventListener("click", () => void moveCf(-1));
      $("btn-sheet-cf-down")?.addEventListener("click", () => void moveCf(1));
      $("sheet-cf-list")?.addEventListener("click", (ev) => {
        const li = ev.target.closest?.("li[data-index]");
        if (li) selectCf(Number(li.dataset.index));
      });
    }

    function renderCfList() {
      const list = $("sheet-cf-list");
      if (!list) return;
      list.replaceChildren();
      rules.get(getActiveSheetName()).cf.forEach((rule, index) => {
        const li = document.createElement("li");
        li.dataset.index = String(index);
        li.setAttribute("role", "option");
        li.setAttribute("aria-selected", index === cf.selected ? "true" : "false");
        li.textContent = index + 1 + ". " + describe(rule);
        list.appendChild(li);
      });
    }

    function defaultCfForm() {
      setVal("sheet-cf-range", selectionText());
      setSelect("sheet-cf-type-select", "cellIs");
      setSelect("sheet-cf-cmp-select", "greaterThan");
      setVal("sheet-cf-a", "");
      setVal("sheet-cf-b", "");
      setSelect("sheet-cf-textop-select", "contains");
      setVal("sheet-cf-text", "");
      setSelect("sheet-cf-top-select", "top");
      setVal("sheet-cf-n", "10");
      setChecked("sheet-cf-percent", false);
      setSelect("sheet-cf-avg-select", "above");
      setVal("sheet-cf-std", "0");
      setSelect("sheet-cf-dup-select", "dup");
      setChecked("sheet-cf-not", false);
      setSelect("sheet-cf-period-select", "today");
      setVal("sheet-cf-formula", "");
      const stops = [
        ["min", "", "#f8696b"],
        ["percentile", "50", "#ffeb84"],
        ["max", "", "#63be7b"],
      ];
      stops.forEach(([k, v, c], i) => {
        setSelect("sheet-cf-stop" + i + "-kind", k);
        setVal("sheet-cf-stop" + i + "-val", v);
        setVal("sheet-cf-stop" + i + "-color", c);
      });
      setChecked("sheet-cf-mid", true);
      setSelect("sheet-cf-bar-min-kind", "min");
      setVal("sheet-cf-bar-min-val", "");
      setSelect("sheet-cf-bar-max-kind", "max");
      setVal("sheet-cf-bar-max-val", "");
      setVal("sheet-cf-bar-pos", "#638ec6");
      setVal("sheet-cf-bar-neg", "#ff0000");
      setChecked("sheet-cf-bar-grad", true);
      setSelect("sheet-cf-icon-set-select", "3TrafficLights1");
      setChecked("sheet-cf-icon-rev", false);
      setChecked("sheet-cf-show", true);
      buildThresholdRows();
      setChecked("sheet-cf-bg-on", true);
      setVal("sheet-cf-bg", "#ffc7ce");
      setChecked("sheet-cf-fg-on", true);
      setVal("sheet-cf-fg", "#9c0006");
      for (const id of ["bold", "italic", "underline", "strike"]) setChecked("sheet-cf-" + id, false);
      setChecked("sheet-cf-stop-if-true", false);
    }

    function fillCfForm(rule) {
      defaultCfForm();
      setVal("sheet-cf-range", rule.r);
      setSelect("sheet-cf-type-select", rule.t);
      setChecked("sheet-cf-stop-if-true", rule.stop);
      switch (rule.t) {
        case "cellIs":
          setSelect("sheet-cf-cmp-select", rule.op);
          setVal("sheet-cf-a", rule.a);
          setVal("sheet-cf-b", rule.b);
          break;
        case "text":
          setSelect("sheet-cf-textop-select", rule.op);
          setVal("sheet-cf-text", rule.text);
          break;
        case "top":
          setSelect("sheet-cf-top-select", rule.bottom ? "bottom" : "top");
          setVal("sheet-cf-n", String(rule.n));
          setChecked("sheet-cf-percent", rule.percent);
          break;
        case "average":
          setSelect(
            "sheet-cf-avg-select",
            (rule.above ? "above" : "below") + (rule.equal ? "Equal" : "")
          );
          setVal("sheet-cf-std", String(rule.std || 0));
          break;
        case "dup":
          setSelect("sheet-cf-dup-select", rule.unique ? "unique" : "dup");
          break;
        case "blanks":
        case "errors":
          setChecked("sheet-cf-not", rule.not);
          break;
        case "expr":
          setVal("sheet-cf-formula", rule.formula);
          break;
        case "period":
          setSelect("sheet-cf-period-select", rule.period);
          break;
        case "scale": {
          const stops = rule.stops.length === 2 ? [rule.stops[0], null, rule.stops[1]] : rule.stops;
          setChecked("sheet-cf-mid", Boolean(stops[1]));
          stops.forEach((stop, i) => {
            if (!stop) return;
            setSelect("sheet-cf-stop" + i + "-kind", stop.k);
            setVal("sheet-cf-stop" + i + "-val", stop.v);
            setVal("sheet-cf-stop" + i + "-color", stop.c);
          });
          break;
        }
        case "bar":
          setSelect("sheet-cf-bar-min-kind", rule.min.k);
          setVal("sheet-cf-bar-min-val", rule.min.v);
          setSelect("sheet-cf-bar-max-kind", rule.max.k);
          setVal("sheet-cf-bar-max-val", rule.max.v);
          setVal("sheet-cf-bar-pos", rule.pos || "#638ec6");
          setVal("sheet-cf-bar-neg", rule.neg || "#ff0000");
          setChecked("sheet-cf-bar-grad", rule.grad !== false);
          setChecked("sheet-cf-show", rule.show !== false);
          break;
        case "icons":
          setSelect("sheet-cf-icon-set-select", rule.set);
          setChecked("sheet-cf-icon-rev", rule.rev);
          setChecked("sheet-cf-show", rule.show !== false);
          buildThresholdRows(rule.th);
          break;
        default:
      }
      setChecked("sheet-cf-bg-on", Boolean(rule.bg));
      if (rule.bg) setVal("sheet-cf-bg", rule.bg.toLowerCase());
      setChecked("sheet-cf-fg-on", Boolean(rule.fg));
      if (rule.fg) setVal("sheet-cf-fg", rule.fg.toLowerCase());
      const flags = rule.fl || 0;
      setChecked("sheet-cf-bold", flags & 1);
      setChecked("sheet-cf-italic", flags & 2);
      setChecked("sheet-cf-underline", flags & 4);
      setChecked("sheet-cf-strike", flags & 8);
    }

    function selectCf(index) {
      cf.selected = index;
      const rule = rules.get(getActiveSheetName()).cf[index];
      if (rule) fillCfForm(rule);
      else defaultCfForm();
      renderCfList();
      note("sheet-cf-status", "");
    }

    // The rule the form describes, or a message when it is incomplete.
    function readCfForm() {
      const r = val("sheet-cf-range").trim();
      if (!rules.parseRanges(r)) return { error: "적용 범위를 A1:B9 형식으로 입력하세요." };
      const t = val("sheet-cf-type-select");
      const rule = { r, t, stop: checked("sheet-cf-stop-if-true") };
      switch (t) {
        case "cellIs": {
          rule.op = val("sheet-cf-cmp-select");
          rule.a = strip(val("sheet-cf-a"));
          rule.b = strip(val("sheet-cf-b"));
          if (!rule.a || ((rule.op === "between" || rule.op === "notBetween") && !rule.b)) {
            return { error: "비교할 값을 입력하세요." };
          }
          break;
        }
        case "text":
          rule.op = val("sheet-cf-textop-select");
          rule.text = val("sheet-cf-text");
          if (!rule.text) return { error: "찾을 텍스트를 입력하세요." };
          break;
        case "top": {
          rule.n = Math.floor(Number(val("sheet-cf-n")));
          if (!(rule.n >= 1 && rule.n <= 1000000)) return { error: "개수는 1 이상의 정수입니다." };
          rule.bottom = val("sheet-cf-top-select") === "bottom";
          rule.percent = checked("sheet-cf-percent");
          break;
        }
        case "average": {
          const mode = val("sheet-cf-avg-select");
          rule.above = mode.startsWith("above");
          rule.equal = mode.endsWith("Equal");
          rule.std = Math.max(0, Math.min(3, Math.floor(Number(val("sheet-cf-std")) || 0)));
          break;
        }
        case "dup":
          rule.unique = val("sheet-cf-dup-select") === "unique";
          break;
        case "blanks":
        case "errors":
          rule.not = checked("sheet-cf-not");
          break;
        case "expr":
          rule.formula = strip(val("sheet-cf-formula"));
          if (!rule.formula) return { error: "수식을 입력하세요." };
          break;
        case "period":
          rule.period = val("sheet-cf-period-select");
          break;
        case "scale": {
          const stop = (i) => ({
            k: val("sheet-cf-stop" + i + "-kind"),
            v: val("sheet-cf-stop" + i + "-val"),
            c: colorOf("sheet-cf-stop" + i + "-color", "#ffffff"),
          });
          rule.stops = checked("sheet-cf-mid") ? [stop(0), stop(1), stop(2)] : [stop(0), stop(2)];
          break;
        }
        case "bar":
          rule.min = { k: val("sheet-cf-bar-min-kind"), v: val("sheet-cf-bar-min-val") };
          rule.max = { k: val("sheet-cf-bar-max-kind"), v: val("sheet-cf-bar-max-val") };
          rule.pos = colorOf("sheet-cf-bar-pos", "#638ec6");
          rule.neg = colorOf("sheet-cf-bar-neg", "#ff0000");
          rule.grad = checked("sheet-cf-bar-grad");
          rule.show = checked("sheet-cf-show");
          break;
        case "icons":
          rule.set = val("sheet-cf-icon-set-select");
          rule.th = readThresholds();
          rule.rev = checked("sheet-cf-icon-rev");
          rule.show = checked("sheet-cf-show");
          break;
        default:
          return { error: "알 수 없는 규칙입니다." };
      }
      if (STYLE_TYPES.has(t)) {
        if (checked("sheet-cf-bg-on")) rule.bg = colorOf("sheet-cf-bg", "#ffc7ce");
        if (checked("sheet-cf-fg-on")) rule.fg = colorOf("sheet-cf-fg", "#9c0006");
        rule.fl =
          (checked("sheet-cf-bold") ? 1 : 0) |
          (checked("sheet-cf-italic") ? 2 : 0) |
          (checked("sheet-cf-underline") ? 4 : 0) |
          (checked("sheet-cf-strike") ? 8 : 0);
        if (!rule.bg && !rule.fg && !rule.fl) return { error: "배경, 글자 색, 글꼴 중 하나는 정해야 합니다." };
      }
      return { rule };
    }

    async function applyCfForm() {
      const { rule, error } = readCfForm();
      if (error) {
        note("sheet-cf-status", error);
        return;
      }
      const index = cf.selected;
      await commitRules((next) => {
        if (index >= 0 && index < next.cf.length) next.cf[index] = rule;
        else {
          next.cf.push(rule);
          cf.selected = next.cf.length - 1;
        }
      });
      renderCfList();
      note("sheet-cf-status", "규칙을 적용했습니다.");
    }

    async function deleteCf() {
      const index = cf.selected;
      if (index < 0) return;
      await commitRules((next) => next.cf.splice(index, 1));
      selectCf(-1);
    }

    async function moveCf(delta) {
      const index = cf.selected;
      const to = index + delta;
      const size = rules.get(getActiveSheetName()).cf.length;
      if (index < 0 || to < 0 || to >= size) return;
      await commitRules((next) => {
        const [moved] = next.cf.splice(index, 1);
        next.cf.splice(to, 0, moved);
      });
      cf.selected = to;
      renderCfList();
    }

    function openConditionalFormatDialog() {
      ensureCfControls();
      cf.selected = -1;
      defaultCfForm();
      renderCfList();
      note("sheet-cf-status", "");
      return openDialog("sheet-cf-dialog");
    }

    // ---- data validation -----------------------------------------------------------------------

    const dv = { ready: false };

    function bindDvLayout() {
      $("sheet-dv-type-select")?.addEventListener("change", () => {
        refreshKindRows("sheet-dv-dialog", val("sheet-dv-type-select"));
      });
    }

    function ensureDvControls() {
      if (dv.ready) return;
      dv.ready = true;
      $("btn-sheet-dv-apply")?.addEventListener("click", () => void applyDvForm());
      $("btn-sheet-dv-clear")?.addEventListener("click", () => void clearDv());
    }

    function fillDvForm(rule) {
      const d = rule || { t: "whole", op: "between", a: "", b: "", blank: true, drop: true, alert: "stop", showErr: true };
      setSelect("sheet-dv-type-select", d.t);
      setSelect("sheet-dv-cmp-select", d.op || "between");
      const own = LIST_KINDS.has(d.t) || d.t === "checkbox";
      setVal("sheet-dv-a", own ? "" : d.a);
      setVal("sheet-dv-b", own ? "" : d.b || "");
      setVal("sheet-dv-list", LIST_KINDS.has(d.t) ? d.a : "");
      setVal("sheet-dv-on", d.t === "checkbox" ? d.a : "");
      setVal("sheet-dv-off", d.t === "checkbox" ? d.b || "" : "");
      setChecked("sheet-dv-blank", d.blank !== false);
      setChecked("sheet-dv-drop", d.drop !== false);
      setSelect("sheet-dv-alert-select", d.alert || "stop");
      setChecked("sheet-dv-show-error", d.showErr !== false);
      setVal("sheet-dv-err-title", d.errTitle || "");
      setVal("sheet-dv-err", d.err || "");
      setChecked("sheet-dv-show-prompt", Boolean(d.showPrompt));
      setVal("sheet-dv-prompt-title", d.promptTitle || "");
      setVal("sheet-dv-prompt", d.prompt || "");
    }

    function readDvForm() {
      const r = val("sheet-dv-range").trim();
      if (!rules.parseRanges(r)) return { error: "적용 범위를 A1:B9 형식으로 입력하세요." };
      const t = val("sheet-dv-type-select");
      const rule = {
        r,
        t,
        op: val("sheet-dv-cmp-select") || "between",
        a: strip(val("sheet-dv-a")),
        b: strip(val("sheet-dv-b")),
        blank: checked("sheet-dv-blank"),
        drop: checked("sheet-dv-drop"),
        alert: val("sheet-dv-alert-select") || "stop",
        showErr: checked("sheet-dv-show-error"),
        errTitle: val("sheet-dv-err-title"),
        err: val("sheet-dv-err"),
        showPrompt: checked("sheet-dv-show-prompt"),
        promptTitle: val("sheet-dv-prompt-title"),
        prompt: val("sheet-dv-prompt"),
      };
      if (LIST_KINDS.has(t)) {
        rule.a = val("sheet-dv-list").trim();
        rule.op = "between";
        rule.b = "";
        if (!rule.a) return { error: "목록 항목을 쉼표로 구분해 입력하거나 =범위를 입력하세요." };
      } else if (t === "checkbox") {
        rule.a = val("sheet-dv-on").trim();
        rule.b = val("sheet-dv-off").trim();
        rule.op = "between";
        if (rule.a && rule.a.toLowerCase() === rule.b.toLowerCase()) return { error: "체크한 값과 하지 않은 값이 같음" };
      } else if (!rule.a) {
        return { error: "값을 입력하세요." };
      } else if ((rule.op === "between" || rule.op === "notBetween") && t !== "custom" && !rule.b) {
        return { error: "끝 값을 입력하세요." };
      }
      return { rule };
    }

    // Replaces the validation of exactly the same ranges, or adds one.
    async function applyDvForm() {
      const { rule, error } = readDvForm();
      if (error) {
        note("sheet-dv-status", error);
        return;
      }
      await commitRules((next) => {
        const at = next.dv.findIndex((d) => d.r === rule.r);
        if (at >= 0) next.dv[at] = rule;
        else next.dv.push(rule);
      });
      closeDialog("sheet-dv-dialog");
    }

    async function clearDv() {
      const target = rules.parseRanges(val("sheet-dv-range")) || [];
      await commitRules((next) => {
        next.dv = next.dv.filter((d) => {
          const boxes = rules.parseRanges(d.r) || [];
          return !boxes.some((b) =>
            target.some((t) => b.row0 <= t.row1 && b.row1 >= t.row0 && b.col0 <= t.col1 && b.col1 >= t.col0)
          );
        });
      });
      closeDialog("sheet-dv-dialog");
    }

    function openDataValidationDialog() {
      ensureDvControls();
      const sheet = getActiveSheetName();
      const r = getRange();
      const existing = rules.validationAt(sheet, rules.refOf(r.r1 - 1, r.c1 - 1));
      fillDvForm(existing);
      setVal("sheet-dv-range", existing ? existing.r : selectionText());
      note("sheet-dv-status", "");
      return openDialog("sheet-dv-dialog");
    }

    const alertState = { resolve: null, bound: false };

    function settleAlert(answer) {
      const resolve = alertState.resolve;
      alertState.resolve = null;
      if (!resolve) return;
      closeDialog("sheet-dv-alert-dialog");
      resolve(answer);
    }

    // Resolves true to accept, false to cancel; the dialog is the page's own (no native alert).
    function showAlert(title, message, confirmable) {
      const dlg = $("sheet-dv-alert-dialog");
      if (!dlg) return Promise.resolve(!confirmable);
      if (!alertState.bound) {
        alertState.bound = true;
        $("btn-sheet-dv-alert-ok")?.addEventListener("click", () => settleAlert(true));
        $("btn-sheet-dv-alert-cancel")?.addEventListener("click", () => settleAlert(false));
        dlg.addEventListener("close", () => settleAlert(false));
      }
      settleAlert(false);
      return new Promise((resolve) => {
        alertState.resolve = resolve;
        note("sheet-dv-alert-title", title);
        note("sheet-dv-alert-message", message);
        const cancel = $("btn-sheet-dv-alert-cancel");
        if (cancel) cancel.hidden = !confirmable;
        openDialog("sheet-dv-alert-dialog");
      });
    }

    // Judges an entry that is about to be committed. Resolves true to commit, false to take the
    // old value back. A cell without a rule, an empty entry and a formula entry pass at once.
    async function checkEntry(sheet, ref, text) {
      const verdict = await sheetStoreHelpers.validateEntry(sheet, ref, text);
      if (!verdict || verdict.ok) return true;
      if (verdict.show === false) return true;
      const title = verdict.title || (verdict.alert === "stop" ? "입력 제한" : "확인");
      const message = verdict.message || DEFAULT_MESSAGE;
      if (verdict.alert === "stop") {
        await showAlert(title, message, false);
        return false;
      }
      return showAlert(title, message + (verdict.alert === "warning" ? "\n계속하시겠습니까?" : ""), true);
    }

    // A promise for the commit of an edited cell when a validation applies to it; `null` otherwise
    // (the commit then needs no waiting).
    function gateEntry(td, st) {
      if (!st || !rules.hasAny()) return null;
      const sheet = getActiveSheetName();
      if (st.sheetName !== sheet) return null;
      const text = readTdValue(td);
      const changed = Boolean(st.userEdited) || text !== st.initialText;
      const ref = td.dataset.cell;
      if (!changed || !text.trim() || isFormulaStr(text) || !rules.validationAt(sheet, ref)) return null;
      return checkEntry(sheet, ref, text);
    }

    // The arrow and menu of lists and the check box cells (PD-203).
    const widgets = host.__EASYOFFICE_CREATE_SHEET_DV_WIDGETS__({ document, rules, getActiveSheetName, applyCellEdits });
    widgets.bind();

    const pivotUi = host.__EASYOFFICE_CREATE_SHEET_PIVOT_UI__(
      {
        document,
        sheetDataStore,
        sheetRowCounts,
        getActiveSheetName,
        getRange,
        sheetStoreHelpers,
        enqueueWorkspaceHistoryOp,
        pushRawUndoSnapshot,
        captureWorkspaceSnapshot,
        flushFocusedSheetCellBeforeSwitch,
        performSwitchSheetTabAsync,
        selectionText,
      },
      kit
    );

    // The dialogs answer their kind selects from the start, however they get opened.
    bindCfLayout();
    defaultCfForm();
    bindDvLayout();
    fillDvForm();

    return {
      openConditionalFormatDialog,
      openDataValidationDialog,
      openPivotDialog: pivotUi.openPivotDialog,
      gateEntry,
      syncArrow: widgets.sync,
      hideArrow: widgets.hide,
      pivotCells: pivotUi.pivotCells,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_RULES_UI__ = createSheetRulesUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
