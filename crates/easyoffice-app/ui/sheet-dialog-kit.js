(function (host) {
  "use strict";

  // Small helpers the sheet rule dialogs share (PD-165): field access by id, change events a
  // custom-select follows, opening and closing a `<dialog>`, and the selection as an `sqref`.

  function createDialogKit(document) {
    const $ = (id) => document.getElementById(id);
    const val = (id) => $(id)?.value ?? "";
    const checked = (id) => Boolean($(id)?.checked);
    const setVal = (id, v) => {
      const el = $(id);
      if (el) el.value = v;
    };
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

    function openDialog(id) {
      const dlg = $(id);
      if (!dlg) return null;
      if (typeof dlg.showModal === "function") {
        if (!dlg.open) dlg.showModal();
      } else {
        dlg.setAttribute("open", "");
      }
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

    return { $, val, checked, setVal, setChecked, fireChange, setSelect, openDialog, closeDialog, optionsOf, fillOptions, note };
  }

  host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__ = createDialogKit;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
