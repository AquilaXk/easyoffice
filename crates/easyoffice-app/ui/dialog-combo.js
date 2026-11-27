(function (host) {
  "use strict";

  // Editable combo box of the dialogs: a text field you can type into, with a list of presets that opens from an arrow or
  // the arrow keys (the Word-style "배율" and "글꼴 스타일" fields). `inline` keeps the list open under the field, the list
  // box of the 글꼴 tab, and the field follows the highlighted row.
  //
  // Reference implementation (ported): Awesomplete, https://github.com/LeaVerou/awesomplete, `awesomplete.js` (MIT,
  // Copyright (c) 2015 Lea Verou; see THIRD_PARTY_NOTICES.md). Kept from the original: the instance fields (`index`, `ul`,
  // `container`, `isOpened`), `open`/`close`/`goto`/`next`/`previous`/`select`, the `combobox` / `listbox` / `option` ARIA
  // roles with `aria-expanded`, `aria-controls` and `aria-activedescendant`, the mousedown-then-click selection that keeps
  // the focus in the field, and the Enter / Esc / Up / Down handling (original lines 14-141, 191-319).
  // Changed to fit: plain `{ label, value }` items without the filter, sort, `innerHTML` highlight and status line (a preset
  // list has nine rows and is always shown whole), `event.key` in place of `keyCode`, a toggle arrow, an `inline` list and
  // the `change` event after a pick so the dialog reads the field as if it was typed.

  const MIN_ROW_SCROLL_PX = 0;
  let instances = 0;

  function el(document, tag, className) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    return node;
  }

  function fire(input, type) {
    const EventCtor = host.Event;
    input.dispatchEvent(typeof EventCtor === "function" ? new EventCtor(type, { bubbles: true }) : { type, bubbles: true, target: input });
  }

  class Combo {
    constructor(input, options = {}) {
      const document = input.ownerDocument;
      instances += 1;
      this.count = instances;
      this.input = input;
      this.inline = options.inline === true;
      this.items = [];
      this.index = -1;
      this.isOpened = this.inline;

      input.setAttribute("autocomplete", "off");
      input.setAttribute("role", "combobox");
      input.setAttribute("aria-autocomplete", "list");
      input.setAttribute("aria-expanded", String(this.isOpened));
      input.setAttribute("aria-controls", `dialog-combo-list-${this.count}`);

      this.container = el(document, "span", this.inline ? "dialog-combo dialog-combo-inline" : "dialog-combo");
      input.before(this.container);
      this.container.appendChild(input);
      this.ul = el(document, "ul", "dialog-combo-list");
      this.ul.id = `dialog-combo-list-${this.count}`;
      this.ul.setAttribute("role", "listbox");
      this.ul.hidden = !this.inline;
      if (options.label) this.ul.setAttribute("aria-label", options.label);
      if (!this.inline) {
        this.arrow = el(document, "button", "dialog-combo-arrow");
        this.arrow.type = "button";
        this.arrow.tabIndex = -1;
        this.arrow.textContent = "▾";
        this.arrow.setAttribute("aria-label", options.arrowLabel || "목록 열기");
        this.container.appendChild(this.arrow);
        // The arrow keeps the focus in the field, as the list rows do.
        this.arrow.addEventListener("mousedown", (event) => {
          event.preventDefault?.();
          input.focus?.();
          if (this.isOpened) this.close();
          else this.open();
        });
      }
      this.container.appendChild(this.ul);

      input.addEventListener("keydown", (event) => this.onKey(event));
      input.addEventListener("blur", () => this.close());
      // The mousedown keeps the field focused; the pick happens on the click, so dragging off a row cancels it.
      this.ul.addEventListener("mousedown", (event) => event.preventDefault?.());
      this.ul.addEventListener("click", (event) => {
        const row = event.target?.closest?.("li");
        if (row && this.ul.contains(row)) this.select(this.rows().indexOf(row));
      });
      this.setItems(options.items || []);
    }

    rows() {
      return Array.from(this.ul.children);
    }

    setItems(items) {
      this.items = items.map((item) => (typeof item === "object" ? item : { label: String(item), value: String(item) }));
      const document = this.input.ownerDocument;
      this.ul.replaceChildren();
      this.items.forEach((item, i) => {
        const row = el(document, "li");
        row.id = `${this.ul.id}-item-${i}`;
        row.setAttribute("role", "option");
        row.setAttribute("aria-selected", "false");
        row.textContent = item.label;
        this.ul.appendChild(row);
      });
      this.index = -1;
    }

    get selected() {
      return this.index > -1;
    }

    get opened() {
      return this.isOpened;
    }

    open() {
      if (this.items.length === 0) return;
      this.input.setAttribute("aria-expanded", "true");
      this.ul.hidden = false;
      this.isOpened = true;
      // The row that holds the field's value starts highlighted.
      const at = this.items.findIndex((item) => item.value === this.input.value.trim());
      if (at >= 0) this.goto(at);
    }

    close() {
      if (this.inline || !this.isOpened) return;
      this.input.setAttribute("aria-expanded", "false");
      this.ul.hidden = true;
      this.isOpened = false;
      this.goto(-1);
      this.input.setAttribute("aria-activedescendant", "");
    }

    next() {
      const count = this.items.length;
      this.goto(this.index < count - 1 ? this.index + 1 : count ? 0 : -1);
    }

    previous() {
      const count = this.items.length;
      const pos = this.index - 1;
      this.goto(this.selected && pos !== -1 ? pos : count - 1);
    }

    // Highlights one row (`-1` none) and scrolls it into view; an inline list also puts the row's value in the field.
    goto(i) {
      const rows = this.rows();
      if (this.selected && rows[this.index]) rows[this.index].setAttribute("aria-selected", "false");
      this.index = i;
      if (i < 0 || !rows[i]) return;
      rows[i].setAttribute("aria-selected", "true");
      this.input.setAttribute("aria-activedescendant", rows[i].id);
      this.ul.scrollTop = Math.max(MIN_ROW_SCROLL_PX, rows[i].offsetTop - this.ul.clientHeight + rows[i].clientHeight);
    }

    // Puts row `i`'s value in the field and tells the dialog, as if the user had typed it.
    select(i = this.index) {
      const item = this.items[i];
      if (!item) return;
      this.input.value = item.value;
      this.close();
      fire(this.input, "input");
      fire(this.input, "change");
    }

    onKey(event) {
      const { key } = event;
      if (key === "ArrowDown" || key === "ArrowUp") {
        event.preventDefault?.();
        if (!this.opened) this.open();
        else if (key === "ArrowDown") this.next();
        else this.previous();
        if (this.inline) this.select(this.index);
      } else if (this.opened && !this.inline && key === "Enter" && this.selected) {
        event.preventDefault?.();
        event.stopPropagation?.();
        this.select();
      } else if (this.opened && !this.inline && key === "Escape") {
        event.preventDefault?.();
        event.stopPropagation?.();
        this.close();
      }
    }
  }

  host.__EASYOFFICE_DIALOG_COMBO__ = { attach: (input, options) => new Combo(input, options) };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
