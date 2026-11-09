(function (host) {
  "use strict";

  // Combo boxes that look the same on every OS (PD-098, step 4b). The native `<select>` stays in
  // the DOM as the state holder (value, options, change events) and this layer draws the visible
  // control and its menu.
  //
  // Adapted from the reference app `apps/studio-host/src/ui/custom-select.ts` (MIT, Copyright (c) 2025-2026
  // Edward Kim; see THIRD_PARTY_NOTICES.md): the same class names (`custom-select`,
  // `custom-select-trigger/-value/-arrow/-menu/-option/-group`, `native-select-hidden`), the same
  // keyboard model (Enter/Space toggle, Esc closes, arrows step) and the same commit order
  // (set value, dispatch `change`, sync). TypeScript rewritten as JavaScript; programmatic
  // `select.value = x` assignments also refresh the visible label, because the app sets values
  // directly when it loads a deck.

  const SELECT_SELECTOR = "select.sb-combo";
  const ENHANCED = new WeakMap();
  const KEY_STEP = { ArrowDown: 1, ArrowUp: -1 };

  function makeChangeEvent(select) {
    const EventCtor = host.Event;
    return typeof EventCtor === "function"
      ? new EventCtor("change", { bubbles: true })
      : { type: "change", bubbles: true, target: select };
  }

  function el(document, tag, className) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    return node;
  }

  function createOptionRow(document, option, commit) {
    const row = el(document, "button", "custom-select-option");
    row.type = "button";
    row.dataset.value = option.value;
    row.textContent = option.textContent || "";
    row.disabled = Boolean(option.disabled);
    row.setAttribute("role", "option");
    row.addEventListener("mousedown", (event) => {
      event.preventDefault?.();
      event.stopPropagation?.();
      commit(option);
    });
    return row;
  }

  function appendMenuRows(document, menu, select, commit) {
    for (const child of select.children) {
      if (child.tagName === "OPTGROUP") {
        const label = el(document, "div", "custom-select-group");
        label.textContent = child.label || child.getAttribute("label") || "";
        menu.appendChild(label);
        for (const option of child.children) menu.appendChild(createOptionRow(document, option, commit));
      } else if (child.tagName === "OPTION") {
        menu.appendChild(createOptionRow(document, child, commit));
      }
    }
  }

  function selectedOption(select) {
    const options = Array.from(select.querySelectorAll("option"));
    return options.find((option) => option.value === select.value) || options[0] || null;
  }

  function stepSelection(select, delta) {
    const options = Array.from(select.querySelectorAll("option"));
    const start = options.findIndex((option) => option.value === select.value);
    for (let i = 1; i <= options.length; i += 1) {
      const next = options[(start + delta * i + options.length * i) % options.length];
      if (!next.disabled) {
        select.value = next.value;
        return;
      }
    }
  }

  // Wraps the element's `value` accessor so direct assignments refresh the visible label.
  function observeValue(select, onChange) {
    let proto = Object.getPrototypeOf(select);
    let descriptor = null;
    while (proto && !descriptor) {
      descriptor = Object.getOwnPropertyDescriptor(proto, "value");
      proto = Object.getPrototypeOf(proto);
    }
    if (!descriptor?.get || !descriptor.set) return;
    Object.defineProperty(select, "value", {
      configurable: true,
      get() {
        return descriptor.get.call(select);
      },
      set(next) {
        descriptor.set.call(select, next);
        onChange();
      },
    });
  }

  function closeOthers(document, current) {
    for (const other of document.querySelectorAll(".custom-select.open")) {
      if (other === current) continue;
      other.classList.remove("open");
      other.querySelector(".custom-select-trigger")?.setAttribute("aria-expanded", "false");
    }
  }

  function enhanceCustomSelect(document, select) {
    if (ENHANCED.has(select) || select.dataset.nativeSelect === "true") return;
    const root = el(document, "div", ("custom-select " + select.className).trim());
    root.dataset.selectId = select.id || "";
    root.tabIndex = -1;
    const trigger = el(document, "button", "custom-select-trigger");
    trigger.type = "button";
    trigger.setAttribute("aria-haspopup", "listbox");
    trigger.setAttribute("aria-expanded", "false");
    const selectLabel = select.getAttribute("aria-label") || select.getAttribute("title");
    if (selectLabel) trigger.setAttribute("aria-label", selectLabel);
    const value = el(document, "span", "custom-select-value");
    const arrow = el(document, "span", "custom-select-arrow");
    arrow.textContent = "▾";
    arrow.setAttribute("aria-hidden", "true");
    const menu = el(document, "div", "custom-select-menu");
    menu.setAttribute("role", "listbox");
    trigger.appendChild(value);
    trigger.appendChild(arrow);
    select.before(root);
    root.appendChild(trigger);
    root.appendChild(menu);
    root.appendChild(select);
    select.classList.add("native-select-hidden");

    const sync = () => {
      root.classList.toggle("disabled", Boolean(select.disabled));
      root.tabIndex = -1;
      trigger.disabled = Boolean(select.disabled);
      value.textContent = (selectedOption(select)?.textContent || "").trim();
      value.title = value.textContent;
      for (const row of menu.querySelectorAll(".custom-select-option")) {
        const isSelected = row.dataset.value === select.value;
        row.classList.toggle("selected", isSelected);
        row.setAttribute("aria-selected", isSelected ? "true" : "false");
      }
    };
    const close = () => {
      root.classList.remove("open");
      trigger.setAttribute("aria-expanded", "false");
    };
    const open = () => {
      if (select.disabled) return;
      closeOthers(document, root);
      sync();
      root.classList.add("open");
      trigger.setAttribute("aria-expanded", "true");
    };
    const toggle = () => (root.classList.contains("open") ? close() : open());
    const commit = (option) => {
      if (option.disabled) return;
      select.value = option.value;
      select.dispatchEvent(makeChangeEvent(select));
      sync();
      close();
      trigger.focus?.();
    };
    const rebuild = () => {
      menu.replaceChildren();
      appendMenuRows(document, menu, select, commit);
      sync();
    };

    trigger.addEventListener("mousedown", (event) => {
      event.preventDefault?.();
      event.stopPropagation?.();
      toggle();
    });
    root.addEventListener("keydown", (event) => onKeydown(event, { select, close, toggle, sync }));
    document.addEventListener("mousedown", (event) => {
      if (!root.contains(event.target)) close();
    });
    select.addEventListener("change", sync);
    if (typeof host.MutationObserver === "function") {
      new host.MutationObserver(rebuild).observe(select, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "label"] });
    }
    observeValue(select, sync);
    rebuild();
    ENHANCED.set(select, { root, sync });
  }

  function onKeydown(event, ctl) {
    const { select, close, toggle, sync } = ctl;
    if (select.disabled) return;
    if (event.key === "Escape") {
      close();
      event.preventDefault?.();
    } else if (event.key === "Enter" || event.key === " ") {
      toggle();
      event.preventDefault?.();
    } else if (KEY_STEP[event.key]) {
      stepSelection(select, KEY_STEP[event.key]);
      select.dispatchEvent(makeChangeEvent(select));
      sync();
      event.preventDefault?.();
    }
  }

  function enhanceCustomSelects(document, scope) {
    for (const select of (scope || document).querySelectorAll(SELECT_SELECTOR)) enhanceCustomSelect(document, select);
  }

  function syncCustomSelect(select) {
    ENHANCED.get(select)?.sync();
  }

  host.__EASYOFFICE_CUSTOM_SELECT__ = { enhanceCustomSelects, syncCustomSelect };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
