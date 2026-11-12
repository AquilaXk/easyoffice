(function (host) {
  "use strict";

  // Number format commands of the sheet window (PD-193): the 표시 형식 submenu and 셀 서식 item of the
  // 서식 menu, the toolbar buttons, and the 셀 서식 dialog. The presets, the decimal-place edits and
  // the format lists come from sheet-numfmt.js (ported from the reference spreadsheet engine); the
  // text a code shows comes from the engine through sheet-numfmt-display.js.

  // [value, label] of the dialog's categories.
  const KINDS = [
    ["general", "일반"],
    ["number", "숫자"],
    ["currency", "통화"],
    ["accounting", "회계"],
    ["percent", "백분율"],
    ["date", "날짜"],
    ["time", "시간"],
    ["scientific", "지수"],
    ["fraction", "분수"],
    ["text", "텍스트"],
    ["custom", "사용자 지정"],
  ];
  // The fields each category shows.
  const FIELDS = {
    number: ["decimals", "thousands", "negative"],
    currency: ["decimals", "symbol", "negative"],
    accounting: ["decimals", "symbol"],
    percent: ["decimals"],
    scientific: ["decimals"],
    date: ["list"],
    time: ["list"],
    fraction: ["list"],
  };
  const MENU_PRESETS = new Set(["general", "number", "currency", "accounting", "percent", "date", "time", "text"]);
  const SAMPLE_NUMBER = "-1234.5678";

  function createSheetNumfmtUi(deps) {
    const { document, format, kit } = deps;
    const N = host.__EASYOFFICE_SHEET_NUMFMT__;
    const D = host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__;
    const { $, val, checked, setVal, setChecked, setSelect, openDialog, closeDialog, fillOptions, note } = kit;
    let anchor = { code: "", raw: "" };
    let sampleToken = 0;
    let building = false;
    let ready = false;

    const fieldRows = () => [...document.querySelectorAll("#sheet-numfmt-dialog [data-numfmt-field]")];
    const kindOfSelect = () => val("sheet-numfmt-kind-select") || "general";

    function applyPreset(id) {
      const preset = N.presetById(id);
      return preset ? format.formatNumbers((cells) => cells.map(() => preset.code), preset.label) : null;
    }

    function changeDecimals(delta) {
      const label = delta > 0 ? "소수 자릿수 늘리기" : "소수 자릿수 줄이기";
      return format.formatNumbers((cells) => N.planDecimalChange(cells, delta), label);
    }

    function run(action) {
      if (action === "dialog") return openNumfmtDialog();
      if (action === "decimal-up") return changeDecimals(1);
      if (action === "decimal-down") return changeDecimals(-1);
      return MENU_PRESETS.has(action) ? applyPreset(action) : null;
    }

    // Menu items and toolbar buttons carry `data-sheet-numfmt="<action>"`.
    function bindActions() {
      for (const el of document.querySelectorAll("[data-sheet-numfmt]")) {
        el.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          host.__EASYOFFICE_CLOSE_MENUS__?.(document);
          if (el.getAttribute("aria-disabled") === "true") return;
          void Promise.resolve(run(el.dataset.sheetNumfmt)).catch(() => {});
        });
      }
    }

    function setOptions(id, pairs, selected) {
      const el = $(id);
      if (!el) return;
      el.replaceChildren();
      for (const [value, label] of pairs) {
        const opt = document.createElement("option");
        opt.value = value;
        opt.textContent = label;
        el.appendChild(opt);
      }
      el.value = pairs.some(([value]) => value === selected) ? selected : (pairs[0]?.[0] ?? "");
      kit.fireChange(el);
    }

    const listCodes = (kind) => (kind === "date" ? N.DATE_CODES : kind === "time" ? N.TIME_CODES : N.FRACTION_CODES);

    // The list of a date, time or fraction category: the codes, labelled with what they show once
    // the engine has answered.
    function fillList(kind, selected) {
      const codes = listCodes(kind);
      setOptions("sheet-numfmt-list-select", codes.map((code) => [code, code]), selected);
      const raw = kind === "fraction" ? "0.7" : N.DATE_SAMPLE;
      void Promise.all(codes.map((code) => D.preview(code, [raw]))).then((shown) => {
        const select = $("sheet-numfmt-list-select");
        if (!select || kindOfSelect() !== kind) return;
        kit.optionsOf(select).forEach((opt, at) => {
          const text = shown[at]?.[0]?.t;
          if (text) opt.textContent = text;
        });
      });
    }

    function fields() {
      return {
        decimals: Number(val("sheet-numfmt-decimals")) || 0,
        thousands: checked("sheet-numfmt-thousands"),
        symbol: val("sheet-numfmt-symbol-select") || N.DEFAULT_CURRENCY,
        negative: val("sheet-numfmt-negative-select") || "minus",
        code: val("sheet-numfmt-code"),
      };
    }

    // The sample line: what the code shows for the anchor cell's value (or a stock number).
    async function refreshSample() {
      const code = val("sheet-numfmt-code");
      const kind = kindOfSelect();
      if (!N.codeWithinBounds(code)) {
        sampleToken += 1;
        $("sheet-numfmt-sample").textContent = "";
        note("sheet-numfmt-status", "서식 코드가 한도를 넘음");
        return;
      }
      const raw = N.isNumberText(anchor.raw) ? anchor.raw.trim() : kind === "date" || kind === "time" ? N.DATE_SAMPLE : SAMPLE_NUMBER;
      const token = (sampleToken += 1);
      const [shown] = N.isDefaultFormat(code) ? [null] : await D.preview(code, [raw]);
      if (token !== sampleToken) return;
      const out = $("sheet-numfmt-sample");
      if (!out) return;
      out.textContent = shown ? shown.t : host.__EASYOFFICE_SHEET_GRID_UTILS__.formatGeneral(raw);
      const red = shown?.c === "#ff0000";
      out.style.setProperty("--nf-color", red ? "" : shown?.c || "");
      out.classList.toggle("nf-colored", Boolean(shown?.c) && !red);
      out.classList.toggle("nf-red", red);
      note("sheet-numfmt-status", D.isBad(code) ? "서식 코드를 해석할 수 없음" : "");
    }

    function syncFieldRows(kind) {
      const shown = new Set(FIELDS[kind] || []);
      for (const row of fieldRows()) row.hidden = !shown.has(row.dataset.numfmtField);
    }

    // A change of the category or one of its fields rebuilds the code.
    function rebuild() {
      if (building) return;
      const kind = kindOfSelect();
      syncFieldRows(kind);
      if (kind === "date" || kind === "time" || kind === "fraction") {
        if (!kit.optionsOf($("sheet-numfmt-list-select")).some((o) => listCodes(kind).includes(o.value))) fillList(kind, "");
        setVal("sheet-numfmt-code", val("sheet-numfmt-list-select"));
      } else if (kind !== "custom") {
        setVal("sheet-numfmt-code", N.buildCode(kind, fields()));
      }
      void refreshSample();
    }

    function onKindChange() {
      if (building) return;
      const kind = kindOfSelect();
      if (kind === "date" || kind === "time" || kind === "fraction") fillList(kind, "");
      rebuild();
    }

    function onCodeInput() {
      if (building) return;
      building = true;
      setSelect("sheet-numfmt-kind-select", "custom");
      building = false;
      syncFieldRows("custom");
      void refreshSample();
    }

    function negativeOf(code) {
      if (code.includes("[Red](")) return "red-paren";
      if (/_\);\(/.test(code)) return "paren";
      return /\[Red\]-/.test(code) ? "red-minus" : "minus";
    }

    // Fills the controls from the anchor cell's code without changing the code itself.
    function loadFields(code) {
      let kind = N.patternKind(code);
      building = true;
      if ((kind === "date" || kind === "time" || kind === "fraction") && !listCodes(kind).includes(code)) kind = "custom";
      setSelect("sheet-numfmt-kind-select", kind);
      setVal("sheet-numfmt-decimals", String(N.decimalsOf(code, 2)));
      setChecked("sheet-numfmt-thousands", code.includes("#,##"));
      setSelect("sheet-numfmt-symbol-select", N.CURRENCY_SYMBOLS.find((s) => code.includes(s)) || N.DEFAULT_CURRENCY);
      setSelect("sheet-numfmt-negative-select", negativeOf(code));
      if (kind === "date" || kind === "time" || kind === "fraction") fillList(kind, code);
      setVal("sheet-numfmt-code", code);
      syncFieldRows(kind);
      building = false;
    }

    function ensureControls() {
      if (ready) return;
      ready = true;
      fillOptions("sheet-numfmt-kind-select", KINDS);
      fillOptions("sheet-numfmt-symbol-select", N.CURRENCY_SYMBOLS.map((s) => [s, s]));
      fillOptions("sheet-numfmt-negative-select", N.NEGATIVES.map((n) => [n.id, n.label]));
      $("sheet-numfmt-kind-select")?.addEventListener("change", onKindChange);
      for (const id of ["sheet-numfmt-decimals", "sheet-numfmt-thousands", "sheet-numfmt-symbol-select", "sheet-numfmt-negative-select"]) {
        $(id)?.addEventListener("change", rebuild);
      }
      $("sheet-numfmt-list-select")?.addEventListener("change", rebuild);
      $("sheet-numfmt-code")?.addEventListener("input", onCodeInput);
      $("btn-sheet-numfmt-apply")?.addEventListener("click", () => void applyDialog());
    }

    function openNumfmtDialog() {
      ensureControls();
      anchor = format.anchorCell();
      loadFields(anchor.code);
      note("sheet-numfmt-status", "");
      void refreshSample();
      openDialog("sheet-numfmt-dialog");
      $("sheet-numfmt-code")?.focus?.();
    }

    async function applyDialog() {
      const code = N.isDefaultFormat(val("sheet-numfmt-code")) ? "" : val("sheet-numfmt-code");
      if (code && N.codeWithinBounds(code)) await D.preview(code, ["0"]);
      if (code && (!N.codeWithinBounds(code) || D.isBad(code))) {
        sampleToken += 1;
        note("sheet-numfmt-status", "서식 코드가 한도를 넘음");
        return;
      }
      const result = await format.formatNumbers((cells) => cells.map(() => code), "셀 서식");
      if (result?.refused) {
        note("sheet-numfmt-status", "서식 코드는 시트마다 " + N.MAX_CODES + "개까지");
        return;
      }
      closeDialog("sheet-numfmt-dialog");
    }

    bindActions();
    return { openNumfmtDialog, run };
  }

  host.__EASYOFFICE_CREATE_SHEET_NUMFMT_UI__ = createSheetNumfmtUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
