(function (host) {
  "use strict";

  // What a grid cell shows and how wide its text is: the number format and the General display of a value, the
  // text measurements the grid and the row heights use, and the copy of a workbook model that undo takes. The
  // functions hold no grid state beyond one shared canvas and the column width lookup the grid hands in.
  const { normalizeFormattedNumericCell } = host.__EASYOFFICE_SHEET_UTILS__;
  const NUMERIC_RE = /^-?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;

  function isNumericText(text) {
    const t = normalizeFormattedNumericCell(text);
    return t !== "" && NUMERIC_RE.test(t);
  }

  // "General" display: long decimals (float noise such as 0.30000000000000004 or
  // 1266.66666666667) show at most 10 significant digits; the raw value is kept for editing.
  function formatGeneral(raw) {
    const t = String(raw ?? "");
    if (!/^-?\d*\.\d+$/.test(t)) return t;
    const digits = t.replace(/[-.]/g, "").replace(/^0+/, "");
    if (digits.length <= 10) return t;
    const n = Number(t);
    return Number.isFinite(n) ? String(Number(n.toPrecision(10))) : t;
  }

  // Copies a workbook model (sheet -> cell -> {value, formula, style}) ~4x faster than a JSON
  // round-trip, which matters for undo snapshots of sheets with tens of thousands of cells. A frozen
  // (shared) style object is kept as it is.
  function cloneSheetStore(store) {
    const out = Object.create(null);
    Object.keys(store || {}).forEach((name) => {
      const src = store[name] || {};
      const dst = Object.create(null);
      Object.keys(src).forEach((ref) => {
        const e = src[ref];
        if (!e) return;
        const copy = { ...e };
        if (e.style) {
          copy.style = Object.isFrozen(e.style) ? e.style : { ...e.style };
          host.__EASYOFFICE_SHEET_LOOK__?.noteRowFit(e.style);
        }
        dst[ref] = copy;
      });
      out[name] = dst;
    });
    return out;
  }

  // Width of `text` in the cell's font, from one shared canvas; -1 when the page cannot measure.
  let measurer;
  const quoteFamilyPrefix = (family) => {
    if (!family) return "";
    const trimmed = String(family).trim();
    if (!trimmed) return "";
    if (trimmed.includes(",") || /^['"]/.test(trimmed)) return trimmed + ", ";
    return '"' + trimmed.replaceAll('"', "") + '", ';
  };
  function ensureMeasurer(td) {
    if (measurer === undefined) {
      try {
        const doc = td?.ownerDocument || host.document;
        measurer = doc?.createElement?.("canvas")?.getContext?.("2d") || null;
      } catch {
        measurer = null;
      }
    }
    if (!measurer) return null;
    const df = host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
    const ver = df?.version?.() ?? -1;
    if (!measurer.eoFamily || measurer.eoFontVer !== ver) {
      if (df) {
        measurer.eoFamily = df.baseFontFamily();
        measurer.eoSize = df.cssFontSize();
        measurer.eoFontVer = ver;
      } else {
        const cs = td ? host.getComputedStyle?.(td) : null;
        if (!cs?.fontFamily) return null;
        measurer.eoFamily = cs.fontFamily;
        measurer.eoSize = cs.fontSize || "11pt";
        measurer.eoFontVer = ver;
      }
    }
    return measurer;
  }

  function textWidth(td, text) {
    const m = ensureMeasurer(td);
    if (!m) return -1;
    const cellFam = td?.style?.fontFamily;
    const fam = cellFam ? quoteFamilyPrefix(cellFam) + m.eoFamily : m.eoFamily;
    m.font = `${td?.style?.fontStyle || "normal"} ${td?.style?.fontWeight || "400"} ${td?.style?.fontSize || m.eoSize} ${fam}`;
    return m.measureText(text).width;
  }

  // Lines `text` takes in `width` px of `font`; null when the page cannot measure (yet).
  function measureWrapped(td, text, width, font, family) {
    const m = ensureMeasurer(td);
    if (!m?.eoFamily) return null;
    const prefix = family && family !== m.eoFamily ? quoteFamilyPrefix(family) : "";
    m.font = font + " " + prefix + m.eoFamily;
    return host.__EASYOFFICE_SHEET_LINE_COUNT__(text, width, (t) => m.measureText(t).width);
  }

  // Width of one line of `text` in `font` (a CSS font without the family) and the cell's own `family`, else the sheet's;
  // null when the page cannot measure (yet).
  function measureLine(td, font, family, text) {
    const m = ensureMeasurer(td);
    if (!m?.eoFamily) return null;
    const prefix = family && family !== m.eoFamily ? quoteFamilyPrefix(family) : "";
    m.font = font + " " + prefix + m.eoFamily;
    return m.measureText(text).width;
  }

  // Width of a column in px (set by the grid that owns the page's sheet).
  let columnWidthOf = null;
  const setColumnWidthLookup = (lookup) => {
    columnWidthOf = lookup;
  };

  // A formatted number wider than its cell shows as hash marks (padding 6px each side, 1px border).
  function fitToCell(td, text) {
    const room = td._col > 0 && columnWidthOf ? columnWidthOf(td._col) - 13 : NaN;
    if (!(room > 0)) return text;
    const fit = host.__EASYOFFICE_SHEET_NUMFMT__?.fitNumber;
    return fit ? fit(text, room, (t) => textWidth(td, t)) : text;
  }

  // Shows `value` with the cell's number format (`td._nf`, PD-193) or else the General format; the
  // raw value is remembered for editing and saving. The section color of a format rides on a custom
  // property, so it never lands in the inline font properties the sheet model reads back; the
  // common red is the theme's negative color.
  const SECTION_RED = "#ff0000";

  function setDisplayValue(td, value) {
    const raw = String(value ?? "");
    const nf = td._nf;
    const textTyped = td._ct === "s" || nf === "@";
    const formatted = nf ? host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__?.shown(nf, raw, td._ct) : null;
    let shown = formatted ? formatted.t : textTyped ? raw : formatGeneral(raw);
    if (formatted && !textTyped && isNumericText(raw)) shown = fitToCell(td, shown);
    if (td.textContent !== shown) td.textContent = shown;
    if (shown === raw) {
      if (td.dataset.raw !== undefined) {
        delete td.dataset.raw;
        delete td.dataset.display;
      }
    } else {
      // An unchanged attribute is not written again: a write restyles the cell.
      if (td.dataset.raw !== raw) td.dataset.raw = raw;
      if (td.dataset.display !== shown) td.dataset.display = shown;
    }
    td.classList.toggle("num", !textTyped && isNumericText(raw));
    const color = formatted?.c || "";
    if (color || td._nfColor) {
      td._nfColor = color;
      const red = color === SECTION_RED;
      td.style.setProperty("--nf-color", red ? "" : color);
      td.classList.toggle("nf-colored", color !== "" && !red);
      td.classList.toggle("nf-red", red);
    }
  }

  function readTdValue(td) {
    const text = (td.textContent || "").trim();
    if (td.dataset.raw !== undefined && text === td.dataset.display) return td.dataset.raw;
    return text;
  }

  host.__EASYOFFICE_SHEET_GRID_TEXT__ = {
    isNumericText, formatGeneral, cloneSheetStore, measureWrapped, measureLine, setColumnWidthLookup, setDisplayValue, readTdValue,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
