(function (host) {
  "use strict";

  // In-cell widgets of the data validation kinds (PD-200): the arrow and menu of a list or a
  // multiple list beside the selected cell, and the check box of a check box cell (a click on the
  // box, or Space on the selected cell, writes the other value). The values written are the rule's
  // own, so they pass its check. The drawing of the box is CSS on `td[data-dv-box]` (sheet-rules.js
  // sets the attribute while it paints the cell).
  //
  // The check box rule and the multiple list follow the reference sheet validators
  // (`packages/sheets-data-validation/src/validators/checkbox-validator.ts`,
  // `list-multiple-validator.ts`, Apache-2.0, see THIRD_PARTY_NOTICES.md): a cell holds the checked
  // or the unchecked value, a multiple list cell holds the chosen items separated by commas.

  const BOX_HIT_PX = 10; // half the clickable box, around the cell's center
  const MENU_WIDTH_PX = 96;

  // The items chosen in the text of a multiple list cell.
  const chosenOf = (text) =>
    String(text || "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);

  // The text a multiple list cell holds after `item` is toggled: the known items in list order,
  // then any other item the cell already had.
  function toggledText(items, current, item) {
    const chosen = new Set(chosenOf(current));
    if (chosen.has(item)) chosen.delete(item);
    else chosen.add(item);
    const known = items.filter((i) => chosen.has(i));
    const extra = [...chosen].filter((i) => !items.includes(i));
    return [...known, ...extra].join(",");
  }

  // The value a check box cell takes next: unchecked and anything else become checked.
  function nextCheckValue(current, on, off) {
    return current.trim().toLowerCase() === on.toLowerCase() ? off : on;
  }

  const CHECK_DEFAULTS = ["TRUE", "FALSE"];
  const boxValues = (dv) => [dv.a || CHECK_DEFAULTS[0], dv.b || CHECK_DEFAULTS[1]];

  host.__EASYOFFICE_SHEET_DV_CORE__ = { chosenOf, toggledText, nextCheckValue, boxValues };

  function createDvWidgets(deps) {
    const { document, rules, getActiveSheetName, applyCellEdits } = deps;
    const arrow = { button: null, menu: null, ref: "", index: -1 };

    const selectedTd = () => document.querySelector("#pane-sheet td.selected-cell");
    const cellText = (td) => String(td?.textContent ?? "").trim();

    function hide() {
      if (arrow.button) arrow.button.hidden = true;
      if (arrow.menu) arrow.menu.hidden = true;
      arrow.index = -1;
    }

    function menuItems() {
      return arrow.menu ? [...arrow.menu.querySelectorAll("li[data-value]")] : [];
    }

    function highlight(index) {
      const rows = menuItems();
      arrow.index = Math.max(0, Math.min(rows.length - 1, index));
      rows.forEach((li, i) => li.setAttribute("aria-selected", i === arrow.index ? "true" : "false"));
    }

    // Writes the cell for a picked item; a multiple list keeps its menu open.
    function pick(item) {
      const sheet = getActiveSheetName();
      const rule = rules.validationAt(sheet, arrow.ref);
      if (!rule) return;
      const multiple = rule.t === "listMultiple";
      const value = multiple ? toggledText(rules.listItems(sheet, rule), cellText(selectedTd()), item) : item;
      if (!multiple) hide();
      void Promise.resolve(applyCellEdits([{ ref: arrow.ref, value, formula: "" }], multiple ? "다중 선택" : "목록 선택")).then(() => {
        if (multiple && arrow.menu && !arrow.menu.hidden) fillMenu(rule, sheet);
      });
    }

    function ensure() {
      if (arrow.button) return;
      const button = document.createElement("button");
      button.type = "button";
      button.id = "sheet-dv-dropdown-btn";
      button.className = "cf-dropdown-btn";
      button.setAttribute("aria-label", "목록에서 선택");
      button.setAttribute("aria-haspopup", "listbox");
      host.__EASYOFFICE_ICONS__?.setIcon(document, button, "chevron-down");
      button.hidden = true;
      // Keep the cell selected while the arrow is pressed.
      button.addEventListener("mousedown", (ev) => ev.preventDefault());
      button.addEventListener("click", () => toggleMenu());
      const menu = document.createElement("ul");
      menu.id = "sheet-dv-dropdown-menu";
      menu.className = "cf-dropdown-menu";
      menu.setAttribute("role", "listbox");
      menu.hidden = true;
      menu.addEventListener("mousedown", (ev) => ev.preventDefault());
      menu.addEventListener("click", (ev) => {
        const li = ev.target.closest?.("li[data-value]");
        if (li) pick(li.dataset.value);
      });
      document.body.appendChild(button);
      document.body.appendChild(menu);
      arrow.button = button;
      arrow.menu = menu;
      document.getElementById("sheet-viewport")?.addEventListener("scroll", hide);
    }

    function fillMenu(rule, sheet) {
      const multiple = rule.t === "listMultiple";
      const chosen = new Set(chosenOf(cellText(selectedTd())));
      arrow.menu.replaceChildren();
      arrow.menu.setAttribute("aria-multiselectable", multiple ? "true" : "false");
      rules.listItems(sheet, rule).forEach((item, i) => {
        const li = document.createElement("li");
        li.setAttribute("role", "option");
        li.dataset.value = item;
        li.textContent = item;
        li.setAttribute("aria-selected", "false");
        li.setAttribute("aria-checked", chosen.has(item) ? "true" : "false");
        li.dataset.index = String(i);
        arrow.menu.appendChild(li);
      });
      const current = menuItems().findIndex((li) => chosen.has(li.dataset.value));
      highlight(current >= 0 ? current : 0);
    }

    function toggleMenu() {
      const sheet = getActiveSheetName();
      const rule = rules.validationAt(sheet, arrow.ref);
      if (!rule || !arrow.menu) return;
      if (!arrow.menu.hidden) {
        arrow.menu.hidden = true;
        return;
      }
      fillMenu(rule, sheet);
      const box = arrow.button.getBoundingClientRect();
      arrow.menu.style.left = box.right - MENU_WIDTH_PX + "px";
      arrow.menu.style.top = box.bottom + 2 + "px";
      arrow.menu.hidden = false;
    }

    // Shows the arrow beside the selected cell when it has a list or multiple list validation.
    function sync() {
      const td = selectedTd();
      const ref = td?.dataset?.cell;
      const rule = ref && rules.hasAny() ? rules.validationAt(getActiveSheetName(), ref) : null;
      if (!rule || (rule.t !== "list" && rule.t !== "listMultiple") || rule.drop === false) {
        hide();
        return;
      }
      ensure();
      // A pick in a multiple list rewrites the cell and the menu stays open on it.
      const keepOpen = arrow.ref === ref && !arrow.menu.hidden && rule.t === "listMultiple";
      arrow.ref = ref;
      const box = td.getBoundingClientRect();
      arrow.button.style.left = box.right - 20 + "px";
      arrow.button.style.top = box.top + Math.max(0, (box.height - 18) / 2) + "px";
      arrow.button.hidden = false;
      arrow.menu.hidden = !keepOpen;
      if (rule.showPrompt && (rule.prompt || rule.promptTitle)) {
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = [rule.promptTitle, rule.prompt].filter(Boolean).join(": ");
      }
    }

    // ---- check box ------------------------------------------------------------------------------
    function toggleBox(td) {
      const ref = td?.dataset?.cell;
      const rule = ref ? rules.validationAt(getActiveSheetName(), ref) : null;
      if (!rule || rule.t !== "checkbox") return false;
      const [on, off] = boxValues(rule);
      void applyCellEdits([{ ref, value: nextCheckValue(cellText(td), on, off), formula: "" }], "체크박스");
      return true;
    }

    // A click on the box of a check box cell.
    function onClick(ev) {
      const td = ev.target?.closest?.("td[data-dv-box]");
      if (!td || td.classList.contains("editing")) return;
      const box = td.getBoundingClientRect();
      const dx = ev.clientX - (box.left + box.right) / 2;
      const dy = ev.clientY - (box.top + box.bottom) / 2;
      if (Math.abs(dx) <= BOX_HIT_PX && Math.abs(dy) <= BOX_HIT_PX) toggleBox(td);
    }

    function onKeydown(ev) {
      const td = ev.target?.closest?.("td");
      if (ev.key === " " && td?.dataset?.dvBox && td.classList.contains("selected-cell") && !td.classList.contains("editing")) {
        ev.preventDefault();
        ev.stopPropagation?.();
        toggleBox(td);
        return;
      }
      const open = arrow.menu && !arrow.menu.hidden;
      if (ev.key === "ArrowDown" && ev.altKey && arrow.button && !arrow.button.hidden && !open) {
        ev.preventDefault();
        ev.stopPropagation?.();
        toggleMenu();
        return;
      }
      if (!open) return;
      const step = { ArrowDown: 1, ArrowUp: -1 }[ev.key];
      if (step) highlight(arrow.index + step);
      else if (ev.key === "Enter" && menuItems()[arrow.index]) pick(menuItems()[arrow.index].dataset.value);
      else if (ev.key === "Escape") arrow.menu.hidden = true;
      else return;
      ev.preventDefault();
      ev.stopPropagation?.();
    }

    function bind() {
      const viewport = document.getElementById("sheet-viewport");
      viewport?.addEventListener("click", onClick);
      viewport?.addEventListener("keydown", onKeydown, true);
    }

    return { sync, hide, bind, toggleBox };
  }

  host.__EASYOFFICE_CREATE_SHEET_DV_WIDGETS__ = createDvWidgets;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
