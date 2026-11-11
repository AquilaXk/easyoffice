(function (host) {
  "use strict";

  // Colour picker of the style bar (PD-098, PD-166): a palette popup with the recent colours and a
  // hidden native `<input type="color">` for "other colour", in place of the visible native input.
  //
  // Reference implementation: the reference app `apps/studio-host/src/ui/color-picker.ts` and
  // `ui/toolbar-popup.ts` (MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md): the
  // same palette rows, class names (`sb-hl-palette`, `sb-hl-swatch`; the picker-specific ones use the `eo-` prefix), recent-colour
  // list (7 entries, merged with what other windows stored) and popup behaviour (outside press and
  // focus move close it, Esc closes and returns focus to the button, ArrowDown on the button opens it
  // and focuses the first swatch). TypeScript rewritten as JavaScript. The storage key is this app's
  // own, and the stale-document guard for the native chooser is left out because one window edits
  // one document (PD-088).

  const RECENT_KEY = "easyoffice.recent-text-colors";
  const RECENT_LIMIT = 7;
  const HEX_COLOR = /^#[0-9a-f]{6}$/i;
  const GRAY_ROW = ["#000000", "#404040", "#808080", "#c0c0c0", "#e0e0e0", "#f0f0f0", "#ffffff"];
  const PALETTE_ROWS = [
    ["#ff0000", "#ff8000", "#ffff00", "#80ff00", "#00ff00", "#00ff80", "#00ffff"],
    ["#0080ff", "#0000ff", "#8000ff", "#ff00ff", "#ff0080", "#c0c0c0", "#808080"],
    ["#ff9999", "#ffcc99", "#ffff99", "#ccff99", "#99ff99", "#99ffcc", "#99ffff"],
    ["#99ccff", "#9999ff", "#cc99ff", "#ff99ff", "#ff99cc", "#e0e0e0", "#404040"],
    ["#cc0000", "#cc6600", "#cccc00", "#66cc00", "#00cc00", "#00cc66", "#00cccc"],
    ["#0066cc", "#0000cc", "#6600cc", "#cc00cc", "#cc0066", "#999999", "#000000"],
  ];

  const ACTIVE_POPUP = new WeakMap();
  const OBSERVED_DOCS = new WeakSet();

  function makeButton(document, label, onClick) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.addEventListener("click", onClick);
    return button;
  }

  function makeSwatch(document, color, onSelect) {
    const swatch = makeButton(document, "", () => onSelect(color));
    swatch.className = "sb-hl-swatch";
    swatch.style.background = color;
    swatch.setAttribute("aria-label", color);
    swatch.title = color;
    return swatch;
  }

  function makeRow(document, colors, onSelect) {
    const row = document.createElement("div");
    row.className = "sb-hl-palette-row";
    for (const color of colors) row.appendChild(makeSwatch(document, color, onSelect));
    return row;
  }

  function readRecent() {
    try {
      const saved = JSON.parse(host.localStorage.getItem(RECENT_KEY) || "[]");
      if (!Array.isArray(saved)) return [];
      const colors = saved.filter((c) => typeof c === "string" && HEX_COLOR.test(c)).map((c) => c.toLowerCase());
      return [...new Set(colors)].slice(0, RECENT_LIMIT);
    } catch {
      return [];
    }
  }

  // Returns the function that records a chosen colour.
  function appendRecentColors(document, panel, onSelect) {
    const recent = document.createElement("div");
    recent.className = "eo-recent-colors";
    const label = document.createElement("div");
    label.textContent = "최근에 사용한 색";
    const row = document.createElement("div");
    row.className = "sb-hl-palette-row";
    recent.appendChild(label);
    recent.appendChild(row);
    panel.appendChild(recent);
    let colors = readRecent();
    let hasUnsaved = false;
    const render = () => {
      row.replaceChildren(...colors.map((color) => makeSwatch(document, color, onSelect)));
      if (colors.length === 0) row.textContent = "아직 없음";
    };
    render();
    return (color) => {
      // Other windows' choices merge in unless an earlier save failed and memory is ahead.
      const shared = hasUnsaved ? [] : readRecent();
      colors = [...new Set([color, ...shared, ...colors])].slice(0, RECENT_LIMIT);
      try {
        host.localStorage.setItem(RECENT_KEY, JSON.stringify(colors));
        hasUnsaved = false;
      } catch {
        hasUnsaved = true;
      }
      render();
    };
  }

  // One set of document listeners tracks the single open popup.
  function observePopups(document) {
    if (OBSERVED_DOCS.has(document)) return;
    OBSERVED_DOCS.add(document);
    const closeOutside = (event) => {
      const active = ACTIVE_POPUP.get(document);
      if (active && !active.host.contains(event.target)) active.close();
    };
    document.addEventListener("mousedown", closeOutside, true);
    document.addEventListener("focusin", closeOutside);
    document.addEventListener("keydown", (event) => {
      const active = ACTIVE_POPUP.get(document);
      if (event.key !== "Escape" || !active) return;
      event.stopPropagation?.();
      active.close();
      active.button.focus();
    }, true);
  }

  function createPopup(document, hostEl, button, panel) {
    observePopups(document);
    const setOpen = (open) => {
      panel.hidden = !open;
      hostEl.classList.toggle("open", open);
      button.setAttribute("aria-expanded", String(open));
    };
    const close = () => {
      setOpen(false);
      if (ACTIVE_POPUP.get(document)?.host === hostEl) ACTIVE_POPUP.delete(document);
    };
    const open = () => {
      if (button.disabled) return;
      ACTIVE_POPUP.get(document)?.close();
      setOpen(true);
      ACTIVE_POPUP.set(document, { host: hostEl, button, close });
    };
    close();
    return { open, close, toggle: () => (panel.hidden ? open() : close()) };
  }

  // `hostEl` holds the toggle button (`[data-color-toggle]`), the native input and the palette
  // panel (`.sb-hl-palette`). `onSelect(color)` gets "#rrggbb", or "" for "색 없음" (`options.clear`).
  // `options.remember` keeps the recent-colour row.
  function setupColorPicker(hostEl, onSelect, options = {}) {
    const document = hostEl.ownerDocument;
    const button = hostEl.querySelector("[data-color-toggle]");
    const input = hostEl.querySelector("input");
    const panel = hostEl.querySelector(".sb-hl-palette");
    input.tabIndex = -1;
    input.classList.add("eo-color-input");
    input.setAttribute("aria-hidden", "true");
    panel.classList.add("eo-color-palette");
    panel.setAttribute("role", "group");
    panel.setAttribute("aria-label", button.title);
    button.setAttribute("aria-controls", panel.id);
    const popup = createPopup(document, hostEl, button, panel);
    let remember = null;
    const applyColor = (color) => {
      remember?.(color);
      input.value = color;
      popup.close();
      onSelect(color);
    };
    for (const colors of [GRAY_ROW, ...PALETTE_ROWS]) panel.appendChild(makeRow(document, colors, applyColor));
    if (options.remember) remember = appendRecentColors(document, panel, applyColor);
    const actions = document.createElement("div");
    actions.className = "sb-hl-palette-actions";
    if (options.clear) {
      actions.appendChild(makeButton(document, "색 없음", () => {
        popup.close();
        onSelect("");
      }));
    }
    actions.appendChild(makeButton(document, "다른 색...", () => input.click()));
    panel.appendChild(actions);
    // Keeps the editor selection: pressing the picker must not move focus out of the document.
    hostEl.addEventListener("mousedown", (event) => event.preventDefault?.());
    button.addEventListener("click", popup.toggle);
    // The native chooser can commit several times while its window stays open; each commit counts.
    input.addEventListener("change", () => applyColor(input.value));
    button.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowDown") return;
      event.preventDefault?.();
      event.stopPropagation?.();
      popup.open();
      panel.querySelector("button").focus();
    });
    return { open: popup.open, close: popup.close };
  }

  host.__EASYOFFICE_COLOR_PICKER__ = { setupColorPicker, RECENT_KEY, RECENT_LIMIT };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
