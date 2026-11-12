(function (host) {
  "use strict";

  // Conditional formatting and data validation state of the sheets (PD-171). The rules are plain
  // JSON (the shape `sheet_rules_json.rs` reads and writes); the engine evaluates them in the
  // sheet session and answers with packed cell appearances, which are decoded here into `fx`
  // and painted onto the cells the grid materializes.

  const MAX_COLS = 16384;

  // Icon sets of the file formats, lowest icon first: [shape, color]. A shape is minimal SVG
  // markup on a 24x24 grid; it is drawn as a CSS mask over the cell, so the color comes from the
  // set (not from characters or emoji, see docs/design-system.md 3).
  const RED = "#d93025";
  const AMBER = "#f9ab00";
  const GREEN = "#1e8e3e";
  const GRAY = "#80868b";
  const DARK = "#202124";
  const BLUE = "#1a73e8";
  const PINK = "#e8a0a0";
  const gray = (shapes) => shapes.map((shape) => [shape, GRAY]);
  const mono = (shapes, color) => shapes.map((shape) => [shape, color]);
  const STAR = "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.5 2.9 1-6.1-4.4-4.3 6.1-.9z";
  const outline = (d) => `<path d="${d}" fill="none" stroke="#000" stroke-width="1.8" stroke-linejoin="round"/>`;
  const fill = (d, extra = "") => `<path d="${d}"${extra}/>`;
  const RING = "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z";
  const ARROW = "M5 4l14 8-14 8z";
  const turned = (deg) => fill(ARROW, ` transform="rotate(${deg} 12 12)"`);
  // `level` of `total` signal bars are solid, the rest faint.
  const bars = (level, total) =>
    Array.from({ length: total }, (_, i) => {
      const x = 3 + i * (18 / total);
      const h = 4 + (i * 14) / (total - 1);
      return fill(`M${x} ${21 - h}h${(18 / total) * 0.7}V21H${x}z`, i < level ? "" : ' opacity=".3"');
    }).join("");
  // A circle with `quarters` quarters filled (pie), clockwise from twelve o'clock.
  const pie = (quarters) => {
    const rim = outline(RING);
    if (quarters === 0) return rim;
    if (quarters === 4) return fill(RING);
    const end = [[12, 3], [21, 12], [12, 21], [3, 12]][quarters];
    const large = quarters > 2 ? 1 : 0;
    return rim + fill(`M12 12V3A9 9 0 ${large} 1 ${end[0]} ${end[1]}z`);
  };
  // A square filled from the bottom by `quarters` quarters.
  const box = (quarters) => {
    const rim = outline("M4 4h16v16H4z");
    return quarters === 0 ? rim : rim + fill(`M4 ${20 - quarters * 4}h16v${quarters * 4}H4z`);
  };
  const SHAPES = {
    up: fill("M12 4l9 16H3z"),
    down: fill("M12 20L3 4h18z"),
    right: fill(ARROW),
    "up-right": turned(-45),
    "down-right": turned(45),
    disc: fill(RING),
    ring: fill(RING + "M12 7a5 5 0 1 1 0 10a5 5 0 1 1 0-10z", ' fill-rule="evenodd"'),
    diamond: fill("M12 3l9 9-9 9-9-9z"),
    flag: fill("M5 3h2v18H5z") + fill("M7 4h12l-3.5 4 3.5 4H7z"),
    cross: fill("M6 8l2-2 4 4 4-4 2 2-4 4 4 4-2 2-4-4-4 4-2-2 4-4z"),
    check: fill("M4 13l2.5-2.5 3.5 3.5 7.5-7.5L20 9 10 19z"),
    mark: fill("M10.5 4h3l-.5 10h-2z") + fill("M10.5 17h3v3h-3z"),
    "star-0": outline(STAR),
    "star-1": outline(STAR) + fill("M12 3v14l-5.5 2.9 1-6.1-4.4-4.3 6.1-.9z"),
    "star-2": fill(STAR),
  };
  for (let n = 0; n <= 4; n += 1) {
    SHAPES["pie-" + n] = pie(n);
    SHAPES["box-" + n] = box(n);
  }
  for (const total of [4, 5]) for (let n = 1; n <= total; n += 1) SHAPES["bars" + total + "-" + n] = bars(n, total);
  const ICON_SETS = {
    "3Arrows": [["down", RED], ["right", AMBER], ["up", GREEN]],
    "3ArrowsGray": gray(["down", "right", "up"]),
    "3Flags": [["flag", RED], ["flag", AMBER], ["flag", GREEN]],
    "3TrafficLights1": [["disc", RED], ["disc", AMBER], ["disc", GREEN]],
    "3TrafficLights2": [["ring", RED], ["ring", AMBER], ["ring", GREEN]],
    "3Signs": [["diamond", RED], ["up", AMBER], ["disc", GREEN]],
    "3Symbols": [["cross", RED], ["mark", AMBER], ["check", GREEN]],
    "3Symbols2": [["cross", RED], ["mark", AMBER], ["check", GREEN]],
    "3Stars": mono(["star-0", "star-1", "star-2"], AMBER),
    "3Triangles": [["down", RED], ["diamond", AMBER], ["up", GREEN]],
    "4Arrows": [["down", RED], ["down-right", AMBER], ["up-right", AMBER], ["up", GREEN]],
    "4ArrowsGray": gray(["down", "down-right", "up-right", "up"]),
    "4RedToBlack": [["disc", DARK], ["disc", GRAY], ["disc", PINK], ["disc", RED]],
    "4Rating": gray(["bars4-1", "bars4-2", "bars4-3", "bars4-4"]),
    "4TrafficLights": [["disc", DARK], ["disc", RED], ["disc", AMBER], ["disc", GREEN]],
    "5Arrows": [["down", RED], ["down-right", AMBER], ["right", AMBER], ["up-right", AMBER], ["up", GREEN]],
    "5ArrowsGray": gray(["down", "down-right", "right", "up-right", "up"]),
    "5Rating": gray(["bars5-1", "bars5-2", "bars5-3", "bars5-4", "bars5-5"]),
    "5Quarters": mono(["pie-0", "pie-1", "pie-2", "pie-3", "pie-4"], BLUE),
    "5Boxes": mono(["box-0", "box-1", "box-2", "box-3", "box-4"], BLUE),
  };
  // Engine order (`ICON_SETS` in sheet_rules.rs).
  const ICON_SET_NAMES = Object.keys(ICON_SETS);

  // Screen labels of the sets (the values stay the file format's identifiers).
  const ICON_SET_LABELS = {
    "3Arrows": "화살표 3개",
    "3ArrowsGray": "회색 화살표 3개",
    "3Flags": "깃발 3개",
    "3TrafficLights1": "신호등 3개",
    "3TrafficLights2": "신호등(속 빈 원) 3개",
    "3Signs": "표지 3개",
    "3Symbols": "기호 3개",
    "3Symbols2": "기호(굵게) 3개",
    "3Stars": "별 3개",
    "3Triangles": "삼각형 3개",
    "4Arrows": "화살표 4개",
    "4ArrowsGray": "회색 화살표 4개",
    "4RedToBlack": "빨강에서 검정 4개",
    "4Rating": "등급 막대 4개",
    "4TrafficLights": "신호등 4개",
    "5Arrows": "화살표 5개",
    "5ArrowsGray": "회색 화살표 5개",
    "5Rating": "등급 막대 5개",
    "5Quarters": "4분면 5개",
    "5Boxes": "상자 5개",
  };

  const maskCache = new Map();
  // `url(data:...)` of one shape, for the CSS mask of an icon cell.
  function iconMask(shape) {
    let mask = maskCache.get(shape);
    if (mask === undefined) {
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">${SHAPES[shape] || ""}</svg>`;
      mask = `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
      maskCache.set(shape, mask);
    }
    return mask;
  }

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

  // Bar geometry (reference conditional-format engine data bar): positive bars grow right of the axis, negative ones left.
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
      for (const name of ["--cf-bg", "--cf-fg", "--cf-bar", "--cf-icon-mask", "--cf-icon-color"]) setVar(td, name, "");
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
      const icon = f.icon ? ICON_SETS[ICON_SET_NAMES[f.icon.set]]?.[f.icon.index] : null;
      if (icon) {
        td.dataset.cfIcon = icon[0];
        setVar(td, "--cf-icon-mask", iconMask(icon[0]));
        setVar(td, "--cf-icon-color", icon[1]);
      } else {
        delete td.dataset.cfIcon;
        setVar(td, "--cf-icon-mask", "");
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
      ICON_SETS,
      ICON_SET_LABELS,
      iconMask,
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
