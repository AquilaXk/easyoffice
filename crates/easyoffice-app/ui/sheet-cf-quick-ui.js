(function (host) {
  "use strict";

  // The conditional formatting submenu of the 서식 menu (PD-281): the cell highlight and top/bottom rules, which open a small
  // dialog that previews the rule on the sheet while its value or format changes, the data bar, color scale and icon set galleries,
  // which add a rule at once, and the new rule, clear rules and manage rules items. The presets and the rule builders are in
  // sheet-cf-presets.js, the preview of a rule in sheet-cf-preview.js; the rules are stored by sheet-rules.js and the dialog to
  // edit them is in sheet-rules-ui.js. A menu action is one undo step; a dialog that is cancelled leaves nothing behind.

  const PREVIEW_DELAY_MS = 120;
  // A gallery panel starts 4px above its row (`.md-sub-panel` `top`) and keeps 8px from the window's edges.
  const PANEL_TOP = -4;
  const PANEL_MARGIN = 8;
  const PERIOD_LABELS = [
    ["yesterday", "어제"],
    ["today", "오늘"],
    ["tomorrow", "내일"],
    ["last7Days", "지난 7일"],
    ["lastWeek", "지난주"],
    ["thisWeek", "이번 주"],
    ["nextWeek", "다음 주"],
    ["lastMonth", "지난달"],
    ["thisMonth", "이번 달"],
    ["nextMonth", "다음 달"],
  ];
  // The inputs of the dialog by field name of a rule kind.
  const FIELD_IDS = { a: "sheet-cfq-a", b: "sheet-cfq-b", text: "sheet-cfq-text", n: "sheet-cfq-n", period: "sheet-cfq-period-select", unique: "sheet-cfq-dup-select" };
  // `기타 규칙…` of each submenu opens the rule manager on a new rule of its first kind.
  const MORE_KINDS = {
    highlight: (range, P) => ({ r: range, t: "cellIs", op: "greaterThan", a: "", b: "", stop: false, bg: P.FORMATS[0].bg, fg: P.FORMATS[0].fg, fl: 0 }),
    top: (range, P) => P.quickRule("top", { n: "10" }, P.FORMATS[0], range).rule,
    bar: (range, P) => P.barRule(range, P.BARS[0]),
    scale: (range, P) => P.scaleRule(range, P.SCALES[0]),
    icons: (range, P) => P.iconRule(range, P.ICON_GROUPS[1].sets[0]),
  };

  function createSheetCfQuickUi(deps) {
    const { document, kit, rules, sheetDataStore, getActiveSheetName, getRange, selectionBoxes, selectionText, commitRules, beginStep, finishStep, isBlocked, openManager } = deps;
    const P = host.__EASYOFFICE_SHEET_CF_PRESETS__;
    const PV = host.__EASYOFFICE_SHEET_CF_PREVIEW__;
    const { $, val, setVal, setSelect, openDialog, closeDialog, fillOptions } = kit;
    const iconMask = (shape) => rules.iconMask(shape);
    const paint = (el, rule) => PV.paint(el, rule, rules.ICON_SETS, iconMask);

    // The text of the active cell, which fills the value of a comparison dialog (a formula's own text is not a value).
    function activeCellText() {
      const r = getRange();
      const entry = sheetDataStore[getActiveSheetName()]?.[rules.refOf(r.r1 - 1, r.c1 - 1)];
      return entry && !entry.formula ? String(entry.value ?? "") : "";
    }

    // ---- the quick dialog -----------------------------------------------------------------------------------------------

    const dialog = { ready: false, session: null };

    function formatById(id) {
      return P.FORMATS.find((f) => f.id === id) || P.CUSTOM_FORMAT;
    }

    function readValues() {
      const values = {};
      for (const [field, id] of Object.entries(FIELD_IDS)) values[field] = val(id);
      return values;
    }

    // Shows the inputs of the kind and fills them.
    function arrange(kind) {
      const spec = P.QUICK[kind];
      $("sheet-cfq-title").textContent = spec.title;
      const shown = spec.fields[0] ?? "none";
      for (const row of Array.from(document.querySelectorAll("#sheet-cfq-dialog [data-cfq-row]"))) row.hidden = row.dataset.cfqRow !== shown;
      for (const lead of Array.from(document.querySelectorAll("#sheet-cfq-dialog [data-cfq-lead]"))) lead.textContent = spec.lead;
      $("sheet-cfq-and").hidden = !spec.fields.includes("b");
      $("sheet-cfq-b").hidden = !spec.fields.includes("b");
      if (spec.unit) $("sheet-cfq-unit").textContent = spec.unit;
      const values = { a: "", b: "", text: "", n: P.QUICK_DEFAULTS.n, period: "today", unique: "dup" };
      const cell = activeCellText();
      if (spec.fields[0] === "a" && spec.fields.length === 1 && cell !== "" && Number.isFinite(Number(cell))) values.a = cell;
      if (kind === "text") values.text = cell;
      for (const [field, id] of Object.entries(FIELD_IDS)) setVal(id, values[field]);
    }

    // The dialog's controls are filled once, when the page starts.
    function bindDialog() {
      if (dialog.ready) return;
      dialog.ready = true;
      fillOptions("sheet-cfq-period-select", PERIOD_LABELS);
      fillOptions("sheet-cfq-dup-select", [["dup", "중복"], ["unique", "고유"]]);
      fillOptions("sheet-cfq-format-select", [...P.FORMATS, P.CUSTOM_FORMAT].map((f) => [f.id, f.label]));
      const root = $("sheet-cfq-dialog");
      for (const id of Object.values(FIELD_IDS)) {
        $(id)?.addEventListener("input", () => dialog.session?.refresh(false));
        $(id)?.addEventListener("change", () => dialog.session?.refresh(true));
        $(id)?.addEventListener("keydown", (ev) => {
          if (ev.key === "Enter" && !$("btn-sheet-cfq-ok").disabled) $("btn-sheet-cfq-ok").click();
        });
      }
      $("sheet-cfq-format-select")?.addEventListener("change", () => dialog.session?.refresh(true));
      $("btn-sheet-cfq-ok")?.addEventListener("click", () => dialog.session?.accept());
      for (const closer of Array.from(root?.querySelectorAll("[data-dialog-close]") ?? [])) closer.addEventListener("click", () => dialog.session?.cancel());
      root?.addEventListener("close", () => dialog.session?.cancel());
    }

    // Opens the dialog of a menu item. The rule shows on the sheet while the inputs are valid; accepting makes the one undo step,
    // anything else that closes the dialog puts the rules back.
    async function openQuick(kind) {
      if (!P.QUICK[kind] || dialog.session || (await isBlocked())) return;
      const step = await beginStep();
      if (!step) return;
      const range = selectionText();
      let shown = "";
      let timer = null;

      const revert = () => {
        if (shown === "") return;
        shown = "";
        rules.set(step.sheet, step.before);
      };
      // Puts the rule of the inputs on the sheet, or the sheet as it was while the inputs say nothing.
      function show() {
        timer = null;
        const format = formatById(val("sheet-cfq-format-select"));
        const { rule, error } = P.quickRule(kind, readValues(), format, range);
        $("btn-sheet-cfq-ok").disabled = Boolean(error);
        paint($("sheet-cfq-sample"), error || format.id === P.CUSTOM_FORMAT.id ? null : { t: "style", bg: format.bg, fg: format.fg });
        if (error || format.id === P.CUSTOM_FORMAT.id) return revert();
        const key = JSON.stringify(rule);
        if (key === shown) return;
        shown = key;
        rules.set(step.sheet, { cf: [rule, ...step.before.cf], dv: step.before.dv });
      }
      function end() {
        if (timer !== null) host.clearTimeout?.(timer);
        timer = null;
        dialog.session = null;
        closeDialog("sheet-cfq-dialog");
      }
      dialog.session = {
        refresh(now) {
          if (timer !== null) host.clearTimeout?.(timer);
          if (now || typeof host.setTimeout !== "function") show();
          else timer = host.setTimeout(show, PREVIEW_DELAY_MS);
        },
        cancel() {
          if (!dialog.session) return;
          revert();
          end();
        },
        async accept() {
          if (!dialog.session) return;
          if (timer !== null) show();
          const format = formatById(val("sheet-cfq-format-select"));
          const { rule } = P.quickRule(kind, readValues(), format, range);
          if (!rule) return;
          if (format.id === P.CUSTOM_FORMAT.id) {
            revert();
            end();
            openManager(rule);
            return;
          }
          const next = { cf: [rule, ...step.before.cf], dv: step.before.dv };
          end();
          await finishStep(step, next);
        },
      };
      bindDialog();
      arrange(kind);
      setSelect("sheet-cfq-format-select", P.FORMATS[0].id);
      openDialog("sheet-cfq-dialog");
      show();
    }

    // ---- galleries ------------------------------------------------------------------------------------------------------

    function element(tag, className, text) {
      const el = document.createElement(tag);
      el.className = className;
      if (text !== undefined) el.textContent = text;
      return el;
    }

    // A tile is a menu item: its preview, and its name for the tooltip and for readers (the label is hidden on screen).
    function tile(action, label, rule) {
      const item = element("button", "md-item cfg-tile");
      item.type = "button";
      item.setAttribute("role", "menuitem");
      item.setAttribute("tabindex", "-1");
      item.title = label;
      item.dataset.sheetCf = action;
      const preview = element("span", "cfp");
      paint(preview, rule);
      item.appendChild(preview);
      item.appendChild(element("span", "md-label cfg-label", label));
      return item;
    }

    // A titled grid of tiles placed before the separator and `기타 규칙…` item the panel starts with.
    function group(panel, title) {
      const tail = panel.querySelector(".md-sep");
      if (title) {
        const heading = element("div", "cfg-title", title);
        heading.setAttribute("role", "presentation");
        panel.insertBefore(heading, tail);
      }
      const grid = element("div", "cfg-grid");
      grid.setAttribute("role", "presentation");
      panel.insertBefore(grid, tail);
      return grid;
    }

    function buildGalleries() {
      const range = "A1";
      const bars = $("sheet-cf-gallery-bar");
      const scales = $("sheet-cf-gallery-scale");
      const icons = $("sheet-cf-gallery-icons");
      if (!bars || !scales || !icons || bars.querySelector(".cfg-grid")) return;
      for (const kind of P.BAR_GROUPS) {
        const grid = group(bars, kind.label);
        for (const bar of P.BARS.filter((b) => b.group === kind.id)) grid.appendChild(tile("bar-" + bar.id, bar.label, { ...P.barRule(range, bar), neg: "" }));
      }
      const scaleGrid = group(scales);
      for (const preset of P.SCALES) scaleGrid.appendChild(tile("scale-" + preset.id, preset.label, P.scaleRule(range, preset)));
      for (const kind of P.ICON_GROUPS) {
        const grid = group(icons, kind.label);
        for (const set of kind.sets) grid.appendChild(tile("icons-" + set.toLowerCase(), rules.ICON_SET_LABELS[set], P.iconRule(range, set)));
      }
    }

    // A gallery that would end below the window moves up as far as it must (never above the top edge); one taller than the
    // window scrolls (the panel's own `max-height`).
    function fitPanel(panel) {
      panel.style.removeProperty("top");
      const rect = panel.getBoundingClientRect();
      const room = host.innerHeight - PANEL_MARGIN;
      if (!(rect.height > 0) || !(rect.bottom > room)) return;
      const lift = Math.min(rect.bottom - room, Math.max(0, rect.top - PANEL_MARGIN));
      panel.style.setProperty("top", `${PANEL_TOP - lift}px`);
    }

    // ---- commands -------------------------------------------------------------------------------------------------------

    const add = (rule) => commitRules((next) => void next.cf.unshift(rule));

    function clearSelection() {
      const cuts = selectionBoxes();
      return commitRules((next) => {
        const kept = P.clearRange(next.cf, cuts, rules.parseRanges, rules.formatRanges);
        if (kept.length === next.cf.length && JSON.stringify(kept) === JSON.stringify(next.cf)) return false;
        next.cf = kept;
        return true;
      });
    }

    function clearSheet() {
      return commitRules((next) => {
        if (next.cf.length === 0) return false;
        next.cf = [];
        return true;
      });
    }

    // Runs the menu item `action` (`data-sheet-cf`).
    async function run(action) {
      const at = action.indexOf("-");
      const head = at < 0 ? action : action.slice(0, at);
      const rest = at < 0 ? "" : action.slice(at + 1);
      const range = selectionText();
      if (head === "quick") return openQuick(rest);
      if (head === "bar") return void (await add(P.barRule(range, P.BARS.find((b) => b.id === rest))));
      if (head === "scale") return void (await add(P.scaleRule(range, P.SCALES.find((s) => s.id === rest))));
      if (head === "icons") return void (await add(P.iconRule(range, rules.ICON_SET_NAMES.find((name) => name.toLowerCase() === rest))));
      if (head === "more") return openManager(MORE_KINDS[rest](range, P));
      if (action === "new" || action === "manage") return openManager();
      if (action === "clear-selection") return void (await clearSelection());
      if (action === "clear-sheet") return void (await clearSheet());
    }

    // Menu items carry `data-sheet-cf="<action>"`.
    function bind() {
      buildGalleries();
      for (const panel of Array.from(document.querySelectorAll(".cfg-panel"))) {
        for (const event of ["mouseenter", "focusin"]) panel.parentElement.addEventListener(event, () => fitPanel(panel));
      }
      for (const el of Array.from(document.querySelectorAll("[data-sheet-cf]"))) {
        el.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          host.__EASYOFFICE_CLOSE_MENUS__?.(document);
          if (el.getAttribute("aria-disabled") === "true") return;
          void run(el.dataset.sheetCf).catch(() => {});
        });
      }
    }

    return { bind, run };
  }

  host.__EASYOFFICE_CREATE_SHEET_CF_QUICK_UI__ = createSheetCfQuickUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
