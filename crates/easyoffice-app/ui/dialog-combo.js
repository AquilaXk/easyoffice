(function (host) {
  "use strict";

  // The behavior of the dialogs' "field + list box" pair (the 글꼴 tab's 글꼴 스타일 and 크기): the list is always shown, a row
  // click or the arrow keys put the row's value in the field, and the highlighted row follows what is typed. It draws nothing of
  // its own: the field is a `.dialog-input` and the list a `.goto-bookmark-list` of `.goto-bookmark-item` rows in the markup
  // (the reference's dialog list, `dialogs.css` 1023-1040).
  //
  // Reference implementation (ported): Awesomplete, https://github.com/LeaVerou/awesomplete, `awesomplete.js` (MIT,
  // Copyright (c) 2015 Lea Verou; see THIRD_PARTY_NOTICES.md). Kept from the original: the instance fields (`index`, `ul`),
  // `goto`/`next`/`previous`/`select`, the `combobox` / `listbox` / `option` ARIA roles with `aria-controls` and
  // `aria-activedescendant`, the mousedown-then-click selection that keeps the focus in the field, and the Up / Down handling
  // (original lines 14-141, 191-319). Changed to fit: plain `{ label, value }` items without the filter, sort, `innerHTML`
  // highlight and status line (a preset list has a dozen rows and is always shown whole), `event.key` in place of `keyCode`,
  // a list that is part of the markup and never closes, and the `change` event after a pick so the dialog reads the field as
  // if it was typed.

  const SELECTED_CLASS = "selected";

  function fire(input, type) {
    const EventCtor = host.Event;
    input.dispatchEvent(typeof EventCtor === "function" ? new EventCtor(type, { bubbles: true }) : { type, bubbles: true, target: input });
  }

  class ListBox {
    constructor(input, ul, items) {
      this.input = input;
      this.ul = ul;
      this.index = -1;

      input.setAttribute("autocomplete", "off");
      input.setAttribute("role", "combobox");
      input.setAttribute("aria-autocomplete", "list");
      input.setAttribute("aria-expanded", "true");
      input.setAttribute("aria-controls", ul.id);
      ul.setAttribute("role", "listbox");

      input.addEventListener("keydown", (event) => this.onKey(event));
      input.addEventListener("input", () => this.sync(true));
      // The mousedown keeps the field focused; the pick happens on the click, so dragging off a row cancels it.
      ul.addEventListener("mousedown", (event) => event.preventDefault?.());
      ul.addEventListener("click", (event) => {
        const row = event.target?.closest?.(".goto-bookmark-item");
        if (row && ul.contains(row)) this.select(this.rows().indexOf(row));
      });
      this.setItems(items);
    }

    rows() {
      return Array.from(this.ul.children);
    }

    setItems(items) {
      this.items = items.map((item) => (typeof item === "object" ? item : { label: String(item), value: String(item) }));
      const document = this.input.ownerDocument;
      this.ul.replaceChildren();
      this.items.forEach((item, i) => {
        const row = document.createElement("div");
        row.className = "goto-bookmark-item";
        row.id = `${this.ul.id}-item-${i}`;
        row.setAttribute("role", "option");
        row.setAttribute("aria-selected", "false");
        row.textContent = item.label;
        this.ul.appendChild(row);
      });
      this.index = -1;
    }

    // Highlights the row for the field's text: the row with that value, or with `prefix` the first that starts with it.
    sync(prefix = false) {
      const text = this.input.value.trim().toLowerCase();
      const same = (item) => (prefix ? item.value.toLowerCase().startsWith(text) : item.value.toLowerCase() === text);
      this.goto(text === "" ? -1 : this.items.findIndex(same));
    }

    next() {
      const count = this.items.length;
      this.goto(this.index < count - 1 ? this.index + 1 : count ? 0 : -1);
    }

    previous() {
      const count = this.items.length;
      const pos = this.index - 1;
      this.goto(this.index > -1 && pos !== -1 ? pos : count - 1);
    }

    // Highlights one row (`-1` none) and scrolls it into view.
    goto(i) {
      const rows = this.rows();
      const old = rows[this.index];
      if (old) {
        old.setAttribute("aria-selected", "false");
        old.classList.remove(SELECTED_CLASS);
      }
      this.index = i;
      const row = rows[i];
      if (!row) {
        this.input.setAttribute("aria-activedescendant", "");
        return;
      }
      row.setAttribute("aria-selected", "true");
      row.classList.add(SELECTED_CLASS);
      this.input.setAttribute("aria-activedescendant", row.id);
      this.ul.scrollTop = Math.max(0, row.offsetTop - this.ul.clientHeight + row.clientHeight);
    }

    // Puts row `i`'s value in the field and tells the dialog, as if the user had typed it.
    select(i = this.index) {
      const item = this.items[i];
      if (!item) return;
      this.input.value = item.value;
      this.goto(i);
      fire(this.input, "input");
      fire(this.input, "change");
    }

    onKey(event) {
      const { key } = event;
      if (key !== "ArrowDown" && key !== "ArrowUp") return;
      event.preventDefault?.();
      if (key === "ArrowDown") this.next();
      else this.previous();
      this.select(this.index);
    }
  }

  host.__EASYOFFICE_DIALOG_COMBO__ = { attach: (input, ul, items) => new ListBox(input, ul, items) };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
