// 참조 구현 (reference implementation): Apache POI `poi/src/main/java/org/apache/poi/ss/usermodel/TableStyleType.java` lines 41-250
// and `poi-ooxml/src/main/java/org/apache/poi/xssf/usermodel/XSSFTableStyle.java` lines 41-95 (the 13 table style elements, their
// application priority, differential format properties and stripe sizes 1..9), Apache-2.0, Copyright The Apache Software Foundation,
// commit 47dcff769bb395034dddfd59c4609659b6dbc6a4. Source and license text: `THIRD_PARTY_NOTICES.md`.
// Modifications: interactive element editor and 5x5 live preview rendered directly with `sheet-table-style.js` and `tokens.css`
// variables, storing compact `TableStyleDef` JSON (`f`, `c`, `b`, `i`, `sz`, `bd`) validated within bounds.

(function (host) {
  "use strict";

  const ELEMENTS = [
    ["w", "전체 표"],
    ["h", "머리글 행"],
    ["t", "요약 행"],
    ["r1", "첫째 행 줄무늬"],
    ["r2", "둘째 행 줄무늬"],
    ["c1", "첫째 열 줄무늬"],
    ["c2", "둘째 열 줄무늬"],
    ["fc", "첫째 열"],
    ["lc", "마지막 열"],
    ["fh", "첫째 머리글 셀"],
    ["lh", "마지막 머리글 셀"],
    ["ft", "첫째 요약 셀"],
    ["lt", "마지막 요약 셀"],
  ];
  const STRIPE_KEYS = new Set(["r1", "r2", "c1", "c2"]);
  const EDGE_LIST = [
    ["t", "위쪽"],
    ["b", "아래쪽"],
    ["l", "왼쪽"],
    ["r", "오른쪽"],
    ["h", "가로 안쪽"],
    ["v", "세로 안쪽"],
  ];
  const THEME_LABELS = ["배경 1", "텍스트 1", "배경 2", "텍스트 2", "강조 1", "강조 2", "강조 3", "강조 4", "강조 5", "강조 6"];
  const MAX_CUSTOM_STYLES = 1000;
  const DEFAULT_PREFIX = "표 스타일 ";

  function defaultStyleName(styleApi, customMap) {
    const taken = new Set(Object.keys(customMap || {}).map((k) => k.toLowerCase()));
    let n = 1;
    while (taken.has((DEFAULT_PREFIX + n).toLowerCase()) || styleApi.isBuiltIn(DEFAULT_PREFIX + n)) n += 1;
    return DEFAULT_PREFIX + n;
  }

  function normalizeHex(raw) {
    const s = String(raw || "").trim().replace(/^#/, "").toLowerCase();
    return /^[0-9a-f]{6}$/.test(s) ? s : "";
  }

  function cleanElement(el, key) {
    if (!el || typeof el !== "object") return null;
    const out = {};
    if (el.f) out.f = el.f;
    if (el.c) out.c = el.c;
    if (el.b) out.b = 1;
    if (el.i) out.i = 1;
    if (STRIPE_KEYS.has(key) && Number(el.sz) >= 2 && Number(el.sz) <= 9) out.sz = Math.trunc(Number(el.sz));
    const bd = {};
    for (const [edgeKey] of EDGE_LIST) {
      if (el.bd?.[edgeKey]) bd[edgeKey] = el.bd[edgeKey];
    }
    if (Object.keys(bd).length > 0) out.bd = bd;
    return Object.keys(out).length > 0 ? out : null;
  }

  function buildCleanDef(draft) {
    const out = {};
    for (const [key] of ELEMENTS) {
      const cleaned = cleanElement(draft[key], key);
      if (cleaned) out[key] = cleaned;
    }
    return out;
  }

  function createSheetTableStyleUi({ document, kit, getCustomStyles, onSave }) {
    const styleApi = host.__EASYOFFICE_SHEET_TABLE_STYLE__;
    const picker = host.__EASYOFFICE_COLOR_PICKER__;
    const { $, openDialog, closeDialog, note, setSelect, fireChange } = kit;
    let draft = {};
    let activeKey = "h";
    let borderColor = "000000";
    let bound = false;

    function currentEl() {
      return draft[activeKey] || (draft[activeKey] = { f: "", c: "", b: false, i: false, sz: 1, bd: {} });
    }

    function paintSwatchChip(hostId, hex, fallbackCss) {
      const chip = $(hostId)?.querySelector(".dialog-swatch-chip");
      if (chip) chip.style.background = hex ? "#" + hex : fallbackCss;
    }

    function renderThemeSwatches(containerId, selectedHex, onSelect) {
      const box = $(containerId);
      if (!box) return;
      box.replaceChildren();
      const colors = styleApi.themeColors().slice(0, 10);
      colors.forEach((rgb, idx) => {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "sb-hl-swatch" + (selectedHex === rgb ? " selected" : "");
        btn.style.background = "#" + rgb;
        btn.title = THEME_LABELS[idx] + " (#" + rgb + ")";
        btn.setAttribute("aria-label", THEME_LABELS[idx]);
        btn.dataset.themeSlot = String(idx);
        btn.addEventListener("click", () => onSelect(rgb));
        box.appendChild(btn);
      });
    }

    function renderPreview() {
      const hostEl = $("sheet-ts-preview");
      if (!hostEl) return;
      hostEl.replaceChildren();
      const def = buildCleanDef(draft);
      const options = { header: true, totals: true, firstColumn: true, lastColumn: true, rowStripes: true, columnStripes: true };
      const cells = styleApi.previewCells("__preview__", options, Object.keys(def).length > 0 ? def : { w: {} });
      for (const row of cells || []) {
        for (const look of row) {
          const cell = document.createElement("span");
          cell.className = "sheet-table-swatch-cell";
          if (look.fill) cell.style.backgroundColor = "#" + look.fill;
          const shadows = styleApi.shadowsOf(look);
          if (shadows.length > 0) cell.style.boxShadow = shadows.join(", ");
          const bar = document.createElement("span");
          bar.className = "sheet-table-swatch-text";
          if (look.bold) bar.classList.add("bold");
          const ink = look.color && !(look.color === "000000" && !look.fill) ? "#" + look.color : look.fill ? "#" + styleApi.autoTextColor(look.fill) : "var(--color-text)";
          bar.style.backgroundColor = ink;
          cell.appendChild(bar);
          hostEl.appendChild(cell);
        }
      }
    }

    function syncControlsFromElement() {
      const el = draft[activeKey] || { f: "", c: "", b: false, i: false, sz: 1, bd: {} };
      const bold = $("sheet-ts-bold");
      const italic = $("sheet-ts-italic");
      if (bold) bold.checked = Boolean(el.b);
      if (italic) italic.checked = Boolean(el.i);
      const stripeRow = $("sheet-ts-stripe-row");
      if (stripeRow) stripeRow.hidden = !STRIPE_KEYS.has(activeKey);
      const stripeInput = $("sheet-ts-stripe-size");
      if (stripeInput) stripeInput.value = String(Math.max(1, Math.min(9, Number(el.sz) || 1)));

      paintSwatchChip("sheet-ts-fill-host", el.f || "", "transparent");
      paintSwatchChip("sheet-ts-font-host", el.c || "", "var(--color-text)");
      paintSwatchChip("sheet-ts-border-host", borderColor, "var(--color-text)");

      renderThemeSwatches("sheet-ts-fill-theme", el.f || "", (rgb) => {
        currentEl().f = rgb;
        syncControlsFromElement();
      });
      renderThemeSwatches("sheet-ts-font-theme", el.c || "", (rgb) => {
        currentEl().c = rgb;
        syncControlsFromElement();
      });

      const firstEdge = EDGE_LIST.map(([k]) => el.bd?.[k]).find(Boolean);
      if (firstEdge) {
        const [line, col] = firstEdge.split(":");
        if ($("sheet-ts-border-line")) setSelect("sheet-ts-border-line", line || "thin");
        if (col) {
          borderColor = col;
          paintSwatchChip("sheet-ts-border-host", borderColor, "var(--color-text)");
        }
      }
      for (const [edgeKey] of EDGE_LIST) {
        const btn = document.querySelector(`#sheet-table-style-dialog [data-ts-edge="${edgeKey}"]`);
        if (btn) btn.setAttribute("aria-pressed", el.bd?.[edgeKey] ? "true" : "false");
      }
      renderPreview();
    }

    function updateEdge(edgeKey, forceOn) {
      const el = currentEl();
      el.bd = el.bd || {};
      const on = forceOn === undefined ? !el.bd[edgeKey] : forceOn;
      if (on) {
        const line = $("sheet-ts-border-line")?.value || "thin";
        el.bd[edgeKey] = line + ":" + (borderColor || "000000");
      } else {
        delete el.bd[edgeKey];
      }
      syncControlsFromElement();
    }

    function refreshActiveEdges() {
      const el = draft[activeKey];
      if (!el?.bd) return;
      const line = $("sheet-ts-border-line")?.value || "thin";
      let changed = false;
      for (const [k] of EDGE_LIST) {
        if (el.bd[k]) {
          el.bd[k] = line + ":" + (borderColor || "000000");
          changed = true;
        }
      }
      if (changed) renderPreview();
    }

    function open(seedDef = null) {
      ensureBound();
      const custom = getCustomStyles();
      const theme = styleApi.themeColors();
      draft = seedDef ? structuredClone(seedDef) : {
        h: { f: theme[4], c: theme[0], b: true, i: false, sz: 1, bd: { b: "medium:" + theme[4] } },
        r1: { f: styleApi.applyTint(theme[4], 0.8), c: "", b: false, i: false, sz: 1, bd: {} },
        t: { f: "", c: "", b: true, i: false, sz: 1, bd: { t: "double:" + theme[4] } },
      };
      for (const k of Object.keys(draft)) {
        draft[k].b = Boolean(draft[k].b);
        draft[k].i = Boolean(draft[k].i);
        draft[k].bd = { ...(draft[k].bd || {}) };
      }
      activeKey = "h";
      borderColor = theme[4] || "000000";
      const nameInput = $("sheet-ts-name");
      if (nameInput) nameInput.value = defaultStyleName(styleApi, custom);
      setSelect("sheet-ts-element", activeKey);
      setSelect("sheet-ts-border-line", "thin");
      note("sheet-ts-status", "");
      syncControlsFromElement();
      openDialog("sheet-table-style-dialog");
      nameInput?.focus?.();
      nameInput?.select?.();
      return true;
    }

    function submit() {
      const name = String($("sheet-ts-name")?.value || "").trim();
      if (!name) {
        note("sheet-ts-status", "스타일 이름을 입력");
        return;
      }
      if (Array.from(name).length > 255) {
        note("sheet-ts-status", "이름은 255자까지");
        return;
      }
      if (styleApi.isBuiltIn(name)) {
        note("sheet-ts-status", "기본 제공 스타일 이름은 쓸 수 없음");
        return;
      }
      const custom = getCustomStyles();
      if (!custom[name] && Object.keys(custom).length >= MAX_CUSTOM_STYLES) {
        note("sheet-ts-status", "사용자 지정 스타일은 1,000개까지");
        return;
      }
      const def = buildCleanDef(draft);
      closeDialog("sheet-table-style-dialog");
      onSave(name, def);
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      const elemSel = $("sheet-ts-element");
      if (elemSel && kit.optionsOf(elemSel).length === 0) {
        for (const [val, label] of ELEMENTS) {
          const opt = document.createElement("option");
          opt.value = val;
          opt.textContent = label;
          elemSel.appendChild(opt);
        }
        fireChange(elemSel);
      }
      elemSel?.addEventListener("change", () => {
        activeKey = elemSel.value || "h";
        syncControlsFromElement();
      });
      $("sheet-ts-bold")?.addEventListener("change", (ev) => {
        currentEl().b = Boolean(ev.target.checked);
        renderPreview();
      });
      $("sheet-ts-italic")?.addEventListener("change", (ev) => {
        currentEl().i = Boolean(ev.target.checked);
        renderPreview();
      });
      $("sheet-ts-stripe-size")?.addEventListener("input", (ev) => {
        const v = Math.max(1, Math.min(9, Math.trunc(Number(ev.target.value) || 1)));
        currentEl().sz = v;
        renderPreview();
      });
      $("sheet-ts-border-line")?.addEventListener("change", refreshActiveEdges);
      $("btn-sheet-ts-clear-elem")?.addEventListener("click", () => {
        delete draft[activeKey];
        syncControlsFromElement();
      });
      $("sheet-table-style-dialog")?.addEventListener("click", (ev) => {
        const edgeBtn = ev.target?.closest?.("[data-ts-edge]");
        if (edgeBtn) updateEdge(edgeBtn.dataset.tsEdge);
      });
      if (picker) {
        const fillHost = $("sheet-ts-fill-host");
        if (fillHost) {
          picker.setupColorPicker(fillHost, (picked) => {
            currentEl().f = picked ? normalizeHex(picked) : "";
            syncControlsFromElement();
          }, { clear: true, clearLabel: "색 없음" });
        }
        const fontHost = $("sheet-ts-font-host");
        if (fontHost) {
          picker.setupColorPicker(fontHost, (picked) => {
            currentEl().c = picked ? normalizeHex(picked) : "";
            syncControlsFromElement();
          }, { clear: true, clearLabel: "자동" });
        }
        const borderHost = $("sheet-ts-border-host");
        if (borderHost) {
          picker.setupColorPicker(borderHost, (picked) => {
            borderColor = picked ? normalizeHex(picked) || "000000" : "000000";
            paintSwatchChip("sheet-ts-border-host", borderColor, "var(--color-text)");
            refreshActiveEdges();
          }, { clear: true, clearLabel: "자동" });
        }
      }
      $("btn-sheet-ts-ok")?.addEventListener("click", submit);
      $("sheet-ts-name")?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          submit();
        }
      });
    }

    return { open, ensureBound };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABLE_STYLE_UI__ = createSheetTableStyleUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
