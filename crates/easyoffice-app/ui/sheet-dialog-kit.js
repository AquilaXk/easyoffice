(function (host) {
  "use strict";

  // Small helpers the sheet rule dialogs share (PD-171): field access by id, change events a
  // custom-select follows, opening and closing a `<dialog>`, and the selection as an `sqref`.

  function createDialogKit(document) {
    const $ = (id) => document.getElementById(id);
    const val = (id) => $(id)?.value ?? "";
    const checked = (id) => Boolean($(id)?.checked);
    // The chip of a colour field shows its value.
    function paintChip(input) {
      const chip = input.parentElement?.querySelector?.(".dialog-swatch-chip");
      chip?.style?.setProperty("--dialog-swatch", input.value || "");
    }

    const setVal = (id, v) => {
      const el = $(id);
      if (!el) return;
      el.value = v;
      if (el.type === "color") paintChip(el);
    };

    // Colour fields use the shell's palette popup (color-picker.js), not a visible native input.
    function bindColors(ids) {
      const picker = host.__EASYOFFICE_COLOR_PICKER__;
      for (const id of ids) {
        const hostEl = $(id + "-host");
        const input = $(id);
        if (!hostEl || !input) continue;
        picker?.setupColorPicker(hostEl, (color) => {
          if (color) paintChip(input);
        });
        paintChip(input);
      }
    }
    const setChecked = (id, v) => {
      const el = $(id);
      if (el) el.checked = Boolean(v);
    };

    // A custom-select wraps each native select and follows its `change` events, so every
    // programmatic change fires one.
    function fireChange(el) {
      const EventCtor = document.defaultView?.Event || host.Event;
      el.dispatchEvent(
        typeof EventCtor === "function"
          ? new EventCtor("change", { bubbles: true })
          : { type: "change", target: el, bubbles: true }
      );
    }

    function setSelect(id, v) {
      const el = $(id);
      if (!el) return;
      el.value = v;
      fireChange(el);
    }

    // The first visible text field takes the focus, else the primary button (not the close x).
    function focusInitial(dlg) {
      const fields = [...dlg.querySelectorAll('input[type="text"]')].filter((el) => !el.disabled && !el.closest?.("[hidden]"));
      (fields[0] || dlg.querySelector(".dialog-btn-primary"))?.focus?.();
    }

    function openDialog(id) {
      const dlg = $(id);
      if (!dlg) return null;
      if (typeof dlg.showModal === "function") {
        if (!dlg.open) dlg.showModal();
      } else {
        dlg.setAttribute("open", "");
      }
      focusInitial(dlg);
      return dlg;
    }

    function closeDialog(id) {
      const dlg = $(id);
      if (!dlg) return;
      if (typeof dlg.close === "function") dlg.close();
      else dlg.removeAttribute("open");
    }

    const optionsOf = (el) => [...el.querySelectorAll("option")];

    function fillOptions(id, pairs) {
      const el = $(id);
      if (!el || optionsOf(el).length > 0) return;
      for (const [value, label] of pairs) {
        const opt = document.createElement("option");
        opt.value = value;
        opt.textContent = label;
        el.appendChild(opt);
      }
    }

    function note(id, text) {
      const el = $(id);
      if (el) el.textContent = text;
    }

    return { $, val, checked, setVal, setChecked, bindColors, fireChange, setSelect, openDialog, closeDialog, optionsOf, fillOptions, note };
  }

  host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__ = createDialogKit;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
