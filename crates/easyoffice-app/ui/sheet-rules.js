(function (host) {
  "use strict";

  // Conditional formatting and data validation state of the sheets (PD-165). The rules are plain
  // JSON (the shape `sheet_rules_json.rs` reads and writes); the engine evaluates them in the
  // sheet session and answers with packed cell appearances, which are decoded here into `fx`
  // and painted onto the cells the grid materializes.

  const MAX_COLS = 16384;

  // Icon sets, lowest icon first: [glyph, color]. Glyphs are text, so no image assets are needed.
  const RED = "#d93025";
  const AMBER = "#f9ab00";
  const GREEN = "#1e8e3e";
  const GRAY = "#80868b";
  const DARK = "#202124";
  const BLUE = "#1a73e8";
  const gray = (glyphs) => glyphs.map((g) => [g, GRAY]);
  const ICON_GLYPHS = {
    "3Arrows": [["▼", RED], ["▶", AMBER], ["▲", GREEN]],
    "3ArrowsGray": gray(["▼", "▶", "▲"]),
    "3Flags": [["⚑", RED], ["⚑", AMBER], ["⚑", GREEN]],
    "3TrafficLights1": [["●", RED], ["●", AMBER], ["●", GREEN]],
    "3TrafficLights2": [["◉", RED], ["◉", AMBER], ["◉", GREEN]],
    "3Signs": [["◆", RED], ["▲", AMBER], ["●", GREEN]],
    "3Symbols": [["✖", RED], ["!", AMBER], ["✔", GREEN]],
    "3Symbols2": [["✖", RED], ["!", AMBER], ["✔", GREEN]],
    "3Stars": [["☆", AMBER], ["✮", AMBER], ["★", AMBER]],
    "3Triangles": [["▼", RED], ["◆", AMBER], ["▲", GREEN]],
    "4Arrows": [["▼", RED], ["↘", AMBER], ["↗", AMBER], ["▲", GREEN]],
    "4ArrowsGray": gray(["▼", "↘", "↗", "▲"]),
    "4RedToBlack": [["●", DARK], ["●", GRAY], ["●", "#e8a0a0"], ["●", RED]],
    "4Rating": gray(["▂", "▄", "▆", "█"]),
    "4TrafficLights": [["●", DARK], ["●", RED], ["●", AMBER], ["●", GREEN]],
    "5Arrows": [["▼", RED], ["↘", AMBER], ["▶", AMBER], ["↗", AMBER], ["▲", GREEN]],
    "5ArrowsGray": gray(["▼", "↘", "▶", "↗", "▲"]),
    "5Rating": gray(["▁", "▂", "▄", "▆", "█"]),
    "5Quarters": [["○", BLUE], ["◔", BLUE], ["◑", BLUE], ["◕", BLUE], ["●", BLUE]],
    "5Boxes": [["□", BLUE], ["◰", BLUE], ["◧", BLUE], ["◩", BLUE], ["■", BLUE]],
  };
  // Engine order (`ICON_SETS` in sheet_rules.rs).
  const ICON_SET_NAMES = Object.keys(ICON_GLYPHS);

  const hex = (n) => "#" + (n >>> 0).toString(16).padStart(6, "0");

  function colAt(letters) {
    let n = 0;
    for (let i = 0; i < letters.length; i += 1) n = n * 26 + letters.charCodeAt(i) - 64;
    return n - 1;
  }

  // "A1" / "$B$2" -> { row, col } zero-based, or null.
  function parseCell(ref) {
    const m = /^\$?([A-Za-z]{1,3})\$?(\d{1,7})$/.exec(String(ref || "").trim());
    if (!m) return null;
    const row = Number(m[2]) - 1;
    const col = colAt(m[1].toUpperCase());
    return row >= 0 && row < 1048576 && col < MAX_COLS ? { row, col } : null;
  }

  // "A1:B9 C3" -> [{ row0, col0, row1, col1 }]
  function parseRanges(text) {
    const out = [];
    for (const part of String(text || "").split(/\s+/)) {
      if (!part) continue;
      const [a, b = a] = part.split(":");
      const p = parseCell(a);
      const q = parseCell(b);
      if (!p || !q) return null;
      out.push({
        row0: Math.min(p.row, q.row),
        col0: Math.min(p.col, q.col),
        row1: Math.max(p.row, q.row),
        col1: Math.max(p.col, q.col),
      });
    }
    return out.length > 0 ? out : null;
  }

  function colLetters(col) {
    let s = "";
    for (let c = col + 1; c > 0; c = Math.floor((c - 1) / 26)) s = String.fromCharCode(65 + ((c - 1) % 26)) + s;
    return s;
  }

  const refOf = (row, col) => colLetters(col) + (row + 1);

  function formatRanges(boxes) {
    return boxes
      .map((b) =>
        b.row0 === b.row1 && b.col0 === b.col1
          ? refOf(b.row0, b.col0)
          : refOf(b.row0, b.col0) + ":" + refOf(b.row1, b.col1)
      )
      .join(" ");
  }

  function contains(boxes, row, col) {
    return boxes.some((b) => row >= b.row0 && row <= b.row1 && col >= b.col0 && col <= b.col1);
  }

  // Packed appearances (see `pack.rs`): palette + base64 of little-endian u32 words.
  function decodePacked(packed) {
    const out = [];
    if (!packed || typeof packed.w !== "string") return out;
    const text = host.atob ? host.atob(packed.w) : globalThis.atob(packed.w);
    const bytes = new Uint8Array(text.length);
    for (let i = 0; i < text.length; i += 1) bytes[i] = text.charCodeAt(i);
    const view = new DataView(bytes.buffer);
    const words = Math.floor(bytes.length / 4);
    const pal = Array.isArray(packed.pal) ? packed.pal : [];
    const color = (i) => (i < pal.length ? hex(pal[i]) : "");
    let at = 0;
    while (at + 1 < words) {
      const row = view.getUint32(at * 4, true);
      const head = view.getUint32((at + 1) * 4, true);
      at += 2;
      const fx = { row, col: head & 0x3fff, flags: (head >>> 14) & 0xf, hide: (head & (1 << 22)) !== 0 };
      if (head & ((1 << 18) | (1 << 19))) {
        if (at >= words) break;
        const w = view.getUint32(at * 4, true);
        at += 1;
        if (head & (1 << 18)) fx.bg = color(w & 0xffff);
        if (head & (1 << 19)) fx.fg = color(w >>> 16);
      }
      if (head & (1 << 20)) {
        if (at >= words) break;
        const w = view.getUint32(at * 4, true);
        at += 1;
        fx.bar = {
          color: color(w & 0xffff),
          pct: ((w >>> 16) & 0xff) - 128,
          start: (w >>> 24) & 0xff,
          gradient: (head & (1 << 23)) !== 0,
        };
      }
      if (head & (1 << 21)) {
        const id = head >>> 24;
        fx.icon = { set: id >> 3, index: id & 7 };
      }
      out.push(fx);
    }
    return out;
  }

  // Bar geometry (Univer data bar): positive bars grow right of the axis, negative ones left.
  function barBackground(bar) {
    const start = bar.start;
    const mag = Math.abs(bar.pct) / 100;
    const left = bar.pct >= 0 ? start : start - start * mag;
    const right = bar.pct >= 0 ? start + (100 - start) * mag : start;
    const edge = bar.gradient ? `color-mix(in srgb, ${bar.color} 25%, transparent)` : bar.color;
    const from = bar.pct >= 0 ? bar.color : edge;
    const to = bar.pct >= 0 ? edge : bar.color;
    return (
      `linear-gradient(90deg, transparent ${left}%, ${from} ${left}%, ${to} ${right}%, ` +
      `transparent ${right}%)`
    );
  }

  function createSheetRules() {
    const rules = new Map(); // sheet -> { cf: [], dv: [] }
    const fx = new Map(); // sheet -> Map(row * MAX_COLS + col -> fx)
    const boxCache = new WeakMap(); // rule object -> parsed ranges
    const listeners = { changed: null, repaint: null, readCells: null };
    let version = 0;

    const blank = () => ({ cf: [], dv: [] });
    const isEmpty = (r) => !r || ((r.cf || []).length === 0 && (r.dv || []).length === 0);

    function boxesOf(rule) {
      let boxes = boxCache.get(rule);
      if (boxes === undefined) {
        boxes = parseRanges(rule.r) || [];
        boxCache.set(rule, boxes);
      }
      return boxes;
    }

    function get(sheet) {
      return rules.get(sheet) || blank();
    }

    function hasAny() {
      for (const r of rules.values()) if (!isEmpty(r)) return true;
      return false;
    }

    // Plain-object copy of the non-empty rule sets (snapshots, save payload).
    function getAll() {
      const out = {};
      for (const [sheet, r] of rules) if (!isEmpty(r)) out[sheet] = structuredClone(r);
      return out;
    }

    function clearFx(sheet) {
      if (sheet === undefined) fx.clear();
      else fx.delete(sheet);
    }

    // Replaces every rule set (open file, undo/redo). The engine session learns of it on the next
    // evaluation.
    function replaceAll(bySheet) {
      const hadAny = hasAny();
      rules.clear();
      clearFx();
      for (const [sheet, r] of Object.entries(bySheet || {})) {
        if (!isEmpty(r)) rules.set(sheet, { cf: r.cf || [], dv: r.dv || [] });
      }
      version += 1;
      if (hadAny || hasAny()) listeners.changed?.();
    }

    function set(sheet, next) {
      if (isEmpty(next)) rules.delete(sheet);
      else rules.set(sheet, { cf: next.cf || [], dv: next.dv || [] });
      clearFx(sheet);
      version += 1;
      listeners.changed?.();
    }

    // What the session is told: priority is the list order.
    function payloadFor(sheet) {
      const r = get(sheet);
      return JSON.stringify({
        cf: r.cf.map((rule, i) => ({ ...rule, p: i + 1 })),
        dv: r.dv,
      });
    }

    function applyPacked(sheet, packed, replace) {
      let map = fx.get(sheet);
      if (!map || replace) {
        map = new Map();
        fx.set(sheet, map);
      }
      for (const f of decodePacked(packed)) {
        const key = f.row * MAX_COLS + f.col;
        const empty = !f.bg && !f.fg && !f.flags && !f.bar && !f.icon;
        if (empty) map.delete(key);
        else map.set(key, f);
      }
      listeners.repaint?.();
    }

    function fxAt(sheet, ref) {
      const map = fx.get(sheet);
      if (!map || map.size === 0) return undefined;
      const c = parseCell(ref);
      return c ? map.get(c.row * MAX_COLS + c.col) : undefined;
    }

    const CF_CLASSES = [
      ["cf-bold", 1],
      ["cf-italic", 2],
      ["cf-underline", 4],
      ["cf-strike", 8],
    ];

    // An empty value removes a custom property.
    function setVar(td, name, value) {
      td.style.setProperty(name, value || "");
    }

    function clearTd(td) {
      delete td.dataset.cf;
      delete td.dataset.cfIcon;
      for (const name of ["--cf-bg", "--cf-fg", "--cf-bar", "--cf-icon-color"]) setVar(td, name, "");
      td.classList.remove("cf-hide-value", ...CF_CLASSES.map(([name]) => name));
    }

    // Paints the rule appearance onto a grid cell. Colours and bars go through custom properties
    // and classes, never through the inline font properties the sheet model reads back.
    function paintTd(td, sheet) {
      const f = fxAt(sheet, td.dataset.cell);
      if (!f) {
        if (td.dataset.cf !== undefined) clearTd(td);
        return;
      }
      td.dataset.cf = "1";
      setVar(td, "--cf-bg", f.bg);
      setVar(td, "--cf-fg", f.fg);
      setVar(td, "--cf-bar", f.bar ? barBackground(f.bar) : "");
      for (const [name, bit] of CF_CLASSES) td.classList.toggle(name, (f.flags & bit) !== 0);
      td.classList.toggle("cf-hide-value", Boolean(f.hide));
      const glyph = f.icon ? ICON_GLYPHS[ICON_SET_NAMES[f.icon.set]]?.[f.icon.index] : null;
      if (glyph) {
        td.dataset.cfIcon = glyph[0];
        setVar(td, "--cf-icon-color", glyph[1]);
      } else {
        delete td.dataset.cfIcon;
        setVar(td, "--cf-icon-color", "");
      }
    }

    function validationAt(sheet, ref) {
      const c = parseCell(ref);
      if (!c) return null;
      return get(sheet).dv.find((dv) => contains(boxesOf(dv), c.row, c.col)) || null;
    }

    // Items offered by a list rule: literal items, or the cells of a range on the model.
    function listItems(sheet, dv) {
      const a = String(dv.a || "");
      if (!a.startsWith("=")) {
        return a.split(",").map((s) => s.trim()).filter(Boolean);
      }
      const ref = a.slice(1);
      const bang = ref.lastIndexOf("!");
      const target = (bang >= 0 ? ref.slice(bang + 1) : ref).replace(/\$/g, "");
      const name = bang >= 0 ? ref.slice(0, bang).replace(/^'|'$/g, "").replace(/''/g, "'") : sheet;
      const boxes = parseRanges(target);
      const read = listeners.readCells;
      if (!boxes || !read) return [];
      const items = [];
      const b = boxes[0];
      for (let r = b.row0; r <= b.row1 && items.length < 500; r += 1) {
        for (let c = b.col0; c <= b.col1 && items.length < 500; c += 1) {
          const text = read(name, refOf(r, c));
          if (text) items.push(text);
        }
      }
      return items;
    }

    return {
      ICON_SET_NAMES,
      ICON_GLYPHS,
      parseRanges,
      formatRanges,
      parseCell,
      refOf,
      decodePacked,
      barBackground,
      get,
      getAll,
      hasAny,
      set,
      replaceAll,
      payloadFor,
      applyPacked,
      clearFx,
      fxAt,
      paintTd,
      validationAt,
      listItems,
      version: () => version,
      isEmpty,
      // Wiring: `changed` runs when the rules change, `repaint` after new appearances arrived,
      // `readCells(sheet, ref)` reads a cell's displayed text from the model.
      on(name, fn) {
        listeners[name] = fn;
      },
    };
  }

  host.__EASYOFFICE_SHEET_RULES_CODEC__ = { decodePacked, barBackground, parseRanges };
  host.__EASYOFFICE_SHEET_RULES__ = createSheetRules();
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
