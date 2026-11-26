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

  const SELECT_SELECTOR = [
    "select.sb-combo",
    "select.sb-ls-select",
    "select.dialog-select",
    "select.formula-select",
  ].join(",");
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
    row.tabIndex = -1;
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

  function appendMenuRows(document, menu, select, commit, options, optionByValue, rowByValue) {
    const addOption = (option) => {
      const row = createOptionRow(document, option, commit);
      options.push(option);
      if (!optionByValue.has(option.value)) optionByValue.set(option.value, option);
      if (!rowByValue.has(option.value)) rowByValue.set(option.value, row);
      menu.appendChild(row);
    };
    for (const child of select.children) {
      if (child.tagName === "OPTGROUP") {
        const label = el(document, "div", "custom-select-group");
        label.textContent = child.label || child.getAttribute("label") || "";
        menu.appendChild(label);
        for (const option of child.children) addOption(option);
      } else if (child.tagName === "OPTION") {
        addOption(child);
      }
    }
  }

  function selectedOption(select, options, optionByValue) {
    return optionByValue.get(select.value) || options[0] || null;
  }

  function stepSelection(select, options, delta) {
    if (options.length === 0) return;
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

  const BOUND_DOCS = new WeakSet();
  const OPEN_SELECT_BY_DOC = new WeakMap();

  function closeOpenSelect(document, exceptRoot) {
    const openSelectEl = OPEN_SELECT_BY_DOC.get(document);
    if (!openSelectEl || openSelectEl.root === exceptRoot) return;
    openSelectEl.close();
  }

  function isInactivePopupHost(document, root) {
    if (!root || root.isConnected === false) return true;
    const ownDialog = root.closest?.("dialog");
    if (ownDialog && !ownDialog.open) return true;
    const openModals = Array.from(document.querySelectorAll?.("dialog") || []).filter(
      (dlg) => dlg.open && !dlg.classList?.contains?.("find-dialog")
    );
    return openModals.length > 0 && !openModals.some((dlg) => dlg.contains?.(root));
  }

  function closeOpenIn(container) {
    const doc = container?.ownerDocument || container;
    if (!doc) return;
    const openSelectEl = OPEN_SELECT_BY_DOC.get(doc);
    if (!openSelectEl) return;
    if (!container?.ownerDocument || container.contains?.(openSelectEl.root) || openSelectEl.root.isConnected === false) {
      openSelectEl.close();
    }
  }

  function ensureDocListeners(document) {
    if (BOUND_DOCS.has(document)) return;
    BOUND_DOCS.add(document);
    const closeOutside = (event) => {
      const openSelectEl = OPEN_SELECT_BY_DOC.get(document);
      if (openSelectEl && !openSelectEl.root.contains(event.target)) {
        openSelectEl.close();
      }
    };
    document.addEventListener("mousedown", closeOutside, true);
    document.addEventListener("focusin", closeOutside);
    document.addEventListener("keydown", (event) => {
      if (event.key !== "Escape") return;
      const openSelectEl = OPEN_SELECT_BY_DOC.get(document);
      if (!openSelectEl) return;
      if (isInactivePopupHost(document, openSelectEl.root)) {
        openSelectEl.close();
        return;
      }
      event.preventDefault?.();
      event.stopPropagation?.();
      openSelectEl.close();
      openSelectEl.trigger?.focus?.();
    }, true);
    document.addEventListener("close", (event) => {
      const openSelectEl = OPEN_SELECT_BY_DOC.get(document);
      if (openSelectEl && event.target?.contains?.(openSelectEl.root)) {
        openSelectEl.close();
      }
    }, true);
  }

  function enhanceCustomSelect(document, select) {
    if (ENHANCED.has(select) || select.dataset.nativeSelect === "true") return;
    ensureDocListeners(document);
    const root = el(document, "div", ("custom-select " + select.className).trim());
    root.dataset.selectId = select.id || "";
    root.tabIndex = -1;
    const trigger = el(document, "button", "custom-select-trigger");
    trigger.type = "button";
    trigger.setAttribute("aria-haspopup", "listbox");
    trigger.setAttribute("aria-expanded", "false");
    // The native select leaves the accessibility tree, so a `<label for>` name moves to the trigger.
    const labelEl = select.id ? document.querySelector(`label[for="${select.id}"]`) : null;
    const selectLabel = select.getAttribute("aria-label") || select.getAttribute("title") || (labelEl?.textContent || "").trim();
    if (selectLabel) trigger.setAttribute("aria-label", selectLabel);
    const value = el(document, "span", "custom-select-value");
    const arrow = el(document, "span", "custom-select-arrow");
    arrow.textContent = "▾";
    arrow.setAttribute("aria-hidden", "true");
    const menu = el(document, "div", "custom-select-menu");
    menu.setAttribute("role", "listbox");
    const sizer = el(document, "span", "custom-select-sizer");
    sizer.setAttribute("aria-hidden", "true");
    trigger.appendChild(value);
    trigger.appendChild(sizer);
    trigger.appendChild(arrow);
    select.before(root);
    root.appendChild(trigger);
    root.appendChild(menu);
    root.appendChild(select);
    select.classList.add("native-select-hidden");
    // The trigger is the one Tab stop and the accessible control; the native select only holds state.
    select.tabIndex = -1;
    select.setAttribute("aria-hidden", "true");

    const options = [];
    const optionByValue = new Map();
    const rowByValue = new Map();
    let selectedRow = null;

    const sync = () => {
      root.classList.toggle("disabled", Boolean(select.disabled));
      root.tabIndex = -1;
      trigger.disabled = Boolean(select.disabled);
      value.textContent = (selectedOption(select, options, optionByValue)?.textContent || "").trim();
      value.title = value.textContent;
      const label = select.getAttribute("aria-label") || select.getAttribute("title") || selectLabel;
      if (label) trigger.setAttribute("aria-label", value.textContent ? `${label}: ${value.textContent}` : label);
      const nextRow = rowByValue.get(select.value) || null;
      if (selectedRow && selectedRow !== nextRow) {
        selectedRow.classList.remove("selected");
        selectedRow.setAttribute("aria-selected", "false");
      }
      if (nextRow) {
        nextRow.classList.add("selected");
        nextRow.setAttribute("aria-selected", "true");
      }
      selectedRow = nextRow;
    };
    let detachedLayer = false;
    let cleanupLayer = null;
    const clearDetachedLayer = () => {
      cleanupLayer?.();
      cleanupLayer = null;
      if (!detachedLayer) return;
      detachedLayer = false;
      try { menu.hidePopover?.(); } catch {}
      menu.removeAttribute?.("popover");
      for (const prop of ["position", "inset", "margin", "left", "top", "right", "bottom", "minWidth", "maxHeight", "zIndex"]) {
        menu.style[prop] = "";
      }
    };
    const placeDetachedLayer = () => {
      if (!root.closest?.(".tb-scroll-viewport, .tb-scrollable, #board-style-bar, .eo-style-bar")) return;
      const rect = trigger.getBoundingClientRect?.();
      if (!rect || !(rect.width > 0) || !(rect.height > 0)) return;
      detachedLayer = true;
      if (typeof menu.showPopover === "function") {
        menu.setAttribute("popover", "manual");
        try { menu.showPopover(); } catch {}
      }
      const win = document.defaultView || globalThis;
      const vw = win.innerWidth || 1024;
      const vh = win.innerHeight || 768;
      menu.style.position = "fixed";
      menu.style.inset = "auto";
      menu.style.margin = "0";
      menu.style.right = "auto";
      menu.style.bottom = "auto";
      menu.style.zIndex = "9999";
      menu.style.minWidth = Math.round(rect.width) + "px";
      const spaceBelow = Math.max(80, vh - rect.bottom - 8);
      const spaceAbove = Math.max(80, rect.top - 8);
      const placeAbove = spaceBelow < 160 && spaceAbove > spaceBelow;
      const maxH = Math.min(240, placeAbove ? spaceAbove : spaceBelow);
      menu.style.maxHeight = maxH + "px";
      const mr = menu.getBoundingClientRect?.();
      const mw = mr?.width || rect.width;
      const mh = mr?.height || maxH;
      const left = Math.max(8, Math.min(rect.left, vw - mw - 8));
      const top = placeAbove ? Math.max(8, rect.top - mh - 2) : Math.max(8, Math.min(vh - mh - 8, rect.bottom + 2));
      menu.style.left = Math.round(left) + "px";
      menu.style.top = Math.round(top) + "px";
      const onScrollOrResize = (ev) => {
        if (ev?.type === "scroll" && ev.target && menu.contains?.(ev.target)) return;
        close();
      };
      document.addEventListener?.("scroll", onScrollOrResize, true);
      win.addEventListener?.("resize", onScrollOrResize);
      cleanupLayer = () => {
        document.removeEventListener?.("scroll", onScrollOrResize, true);
        win.removeEventListener?.("resize", onScrollOrResize);
      };
    };
    const close = () => {
      clearDetachedLayer();
      root.classList.remove("open");
      trigger.setAttribute("aria-expanded", "false");
      if (OPEN_SELECT_BY_DOC.get(document)?.root === root) {
        OPEN_SELECT_BY_DOC.delete(document);
      }
    };
    const open = () => {
      if (select.disabled) return;
      closeOpenSelect(document, root);
      sync();
      root.classList.add("open");
      trigger.setAttribute("aria-expanded", "true");
      OPEN_SELECT_BY_DOC.set(document, { root, trigger, close });
      placeDetachedLayer();
    };
    const toggle = () => {
      if (root.classList.contains("open")) {
        const fromMenu = menu.contains?.(document.activeElement);
        close();
        if (fromMenu) trigger.focus?.();
      } else {
        open();
      }
    };
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
      options.length = 0;
      optionByValue.clear();
      rowByValue.clear();
      selectedRow = null;
      appendMenuRows(document, menu, select, commit, options, optionByValue, rowByValue);
      sizer.replaceChildren(
        ...options.map((option) => {
          const choice = el(document, "span");
          choice.textContent = (option.textContent || "").trim();
          return choice;
        }),
      );
      for (const row of rowByValue.values()) {
        row.classList.remove("selected");
        row.setAttribute("aria-selected", "false");
      }
      sync();
    };

    trigger.addEventListener("mousedown", (event) => {
      if (select.classList?.contains("base-cell-select")) return;
      event.preventDefault?.();
      event.stopPropagation?.();
      toggle();
    });
    trigger.addEventListener("dblclick", (event) => {
      if (select.classList?.contains("base-cell-select")) return;
      close();
      const MouseEventCtor = host.MouseEvent;
      const dbl = typeof MouseEventCtor === "function"
        ? new MouseEventCtor("dblclick", { bubbles: true, cancelable: true })
        : { type: "dblclick", bubbles: true, cancelable: true, target: select };
      select.dispatchEvent(dbl);
      event.preventDefault?.();
      event.stopPropagation?.();
    });
    root.addEventListener("keydown", (event) => onKeydown(event, { root, trigger, menu, select, options, rowByValue, close, toggle, sync }));
    select.addEventListener("change", sync);
    if (typeof host.MutationObserver === "function") {
      new host.MutationObserver(rebuild).observe(select, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "label", "aria-label"] });
    }
    observeValue(select, sync);
    rebuild();
    ENHANCED.set(select, { root, sync, rebuild, open, toggle });
  }

  function stepEdgeSelection(select, options, toEnd) {
    const ordered = toEnd ? [...options].reverse() : options;
    const hit = ordered.find((option) => !option.disabled);
    if (hit) select.value = hit.value;
  }

  function onKeydown(event, ctl) {
    const { root, trigger, menu, select, options, rowByValue, close, toggle, sync } = ctl;
    if (select.disabled) return;
    if (event.key === "Escape") {
      if (!root.classList.contains("open")) return;
      close();
      trigger?.focus?.();
      event.preventDefault?.();
      event.stopPropagation?.();
    } else if (event.key === "Enter" || event.key === " ") {
      toggle();
      event.preventDefault?.();
    } else if (KEY_STEP[event.key] || event.key === "Home" || event.key === "End") {
      if (KEY_STEP[event.key]) stepSelection(select, options, KEY_STEP[event.key]);
      else stepEdgeSelection(select, options, event.key === "End");
      select.dispatchEvent(makeChangeEvent(select));
      sync();
      if (root.classList.contains("open") || menu.contains?.(event.target)) {
        rowByValue.get(select.value)?.focus?.();
      }
      event.preventDefault?.();
    }
  }

  function enhanceCustomSelects(document, scope) {
    for (const select of (scope || document).querySelectorAll(SELECT_SELECTOR)) enhanceCustomSelect(document, select);
  }

  function syncCustomSelect(select) {
    const entry = ENHANCED.get(select);
    if (entry) (entry.rebuild || entry.sync)();
  }

  function openCustomSelect(select) {
    ENHANCED.get(select)?.open?.();
  }

  function toggleCustomSelect(select) {
    ENHANCED.get(select)?.toggle?.();
  }

  host.__EASYOFFICE_CUSTOM_SELECT__ = { enhanceCustomSelects, syncCustomSelect, openCustomSelect, toggleCustomSelect, closeOpenIn };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
