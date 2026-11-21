// Reference implementation (ported, safe JS): Univer `sheets-conditional-formatting-ui` and `sheets-conditional-formatting`, Apache
// License 2.0, Copyright 2023-present DreamNum Co., Ltd. (https://github.com/dream-num/univer, commit
// eaf4cfa04fabdbcfc922c692f3450459f8a29546):
//   packages/sheets-conditional-formatting-ui/src/menu/quick-insert.menu.ts lines 38-72 (`DATA_BAR_PARAMS`: a bar from the lowest to
//     the highest value showing the cell value; `createIconSetParams`: one threshold per icon as a percent, the first at 67 and the
//     second at 33 for three icons), menu/manage-rule.ts lines 41-110 (the entries of the menu: cell rules, rank and average,
//     color scale, data bar, icon set, manage, create, clear the range, clear the worksheet);
//   packages/sheets-conditional-formatting-ui/src/views/panel/RuleList.tsx lines 91-112 (`getRuleList`: the rules of the worksheet,
//     or those whose range meets the selection) and 190-204 (`handleClear`);
//   packages/sheets-conditional-formatting/src/models/icon-map.ts lines 41-137 (the four groups of icon sets), src/base/const.ts lines
//     36-70 and 85-107 (the rule kinds, the default values of a rule), src/commands/commands/clear-range-cf.command.ts lines 49-95
//     (clearing a range cuts the range out of each rule that meets it and drops the rule that has nothing left),
//     packages/core/src/shared/rectangle.ts lines 443-502 and 553-575 (`Rectangle.subtract`, `subtractMulti`).
// Modifications: the reference keeps ranges as row and column numbers on a rule model; these functions work on the rule objects of
// `sheet-rules.js` (`{ r: "A1:B9", t, ... }`). The formats, the colors of the data bars and color scales, the thresholds of four
// and five icons, the labels and the order of the gallery follow the spreadsheet program's conditional formatting menu, which the
// reference's menu does not (it has one data bar, three icon sets and no color scale presets). The reference's commands, mutations,
// permission checks and react components are not used.
(function (host) {
  "use strict";

  // ---- the formats the quick dialogs offer ------------------------------------------------------------------------------

  const FORMATS = [
    { id: "red-fill-dark-red", label: "진한 빨강 텍스트가 있는 연한 빨강 채우기", bg: "#ffc7ce", fg: "#9c0006" },
    { id: "yellow-fill-dark-yellow", label: "진한 노랑 텍스트가 있는 노랑 채우기", bg: "#ffeb9c", fg: "#9c5700" },
    { id: "green-fill-dark-green", label: "진한 녹색 텍스트가 있는 녹색 채우기", bg: "#c6efce", fg: "#006100" },
    { id: "red-fill", label: "연한 빨강 채우기", bg: "#ffc7ce" },
    { id: "red-text", label: "빨강 텍스트", fg: "#9c0006" },
    { id: "red-border", label: "빨강 테두리", bd: { t: "thin:ff0000", r: "thin:ff0000", b: "thin:ff0000", l: "thin:ff0000" } },
  ];
  const CUSTOM_FORMAT = { id: "custom", label: "사용자 지정 형식…" };

  // ---- the quick dialogs: one entry per menu item ---------------------------------------------------------------------
  // `fields` are the inputs of the dialog, `build(values)` the rule body the values make, `check(values)` a message when a value is
  // missing or out of range.

  const needs = (...keys) => (v) => (keys.every((k) => String(v[k] ?? "").trim() !== "") ? "" : "값을 입력하세요.");
  const cell = (op, title, lead, fields = ["a"]) => ({
    title,
    lead,
    fields,
    check: needs(...fields),
    build: (v) => ({ t: "cellIs", op, a: v.a.trim(), b: (v.b ?? "").trim() }),
  });
  const rank = (title, lead, bottom, percent) => ({
    title,
    lead,
    fields: ["n"],
    unit: percent ? "%" : "개",
    check: (v) => {
      const n = Number(v.n);
      return Number.isInteger(n) && n >= 1 && n <= (percent ? 100 : 1000) ? "" : "범위 안의 정수를 입력하세요.";
    },
    build: (v) => ({ t: "top", n: Number(v.n), percent, bottom }),
  });
  const average = (title, lead, above) => ({ title, lead, fields: [], check: () => "", build: () => ({ t: "average", above, equal: false, std: 0 }) });

  const QUICK = {
    greater: cell("greaterThan", "보다 큼", "다음 값보다 큰 셀"),
    less: cell("lessThan", "보다 작음", "다음 값보다 작은 셀"),
    between: cell("between", "다음 값의 사이에 있음", "다음 값의 사이에 있는 셀", ["a", "b"]),
    equal: cell("equal", "같음", "다음 값과 같은 셀"),
    text: {
      title: "텍스트 포함",
      lead: "다음 텍스트를 포함하는 셀",
      fields: ["text"],
      check: needs("text"),
      build: (v) => ({ t: "text", op: "contains", text: v.text }),
    },
    date: {
      title: "발생 날짜",
      lead: "다음 날짜에 발생한 셀",
      fields: ["period"],
      check: needs("period"),
      build: (v) => ({ t: "period", period: v.period }),
    },
    dup: {
      title: "중복 값",
      lead: "다음 값에 대해 서식 지정",
      fields: ["unique"],
      check: () => "",
      build: (v) => ({ t: "dup", unique: v.unique === "unique" }),
    },
    top: rank("상위 10개 항목", "다음 순위에 속하는 값", false, false),
    "top-percent": rank("상위 10%", "다음 비율에 속하는 값", false, true),
    bottom: rank("하위 10개 항목", "다음 순위에 속하는 값", true, false),
    "bottom-percent": rank("하위 10%", "다음 비율에 속하는 값", true, true),
    above: average("평균 초과", "선택 범위의 평균보다 높은 셀", true),
    below: average("평균 미만", "선택 범위의 평균보다 낮은 셀", false),
  };
  // The dialogs open filled with 10 (the spreadsheet program's rank dialogs).
  const QUICK_DEFAULTS = { n: "10" };

  // The rule a quick dialog makes, or the message of what is missing. `format` is an entry of FORMATS (a custom format is made
  // in the rule manager, the rule starts with the first format's colors).
  function quickRule(kind, values, format, range) {
    const spec = QUICK[kind];
    if (!spec) return { error: "알 수 없는 규칙입니다." };
    const error = spec.check(values);
    if (error) return { error };
    const style = format && format.id !== CUSTOM_FORMAT.id ? format : FORMATS[0];
    const rule = { r: range, ...spec.build(values), stop: false, fl: 0 };
    if (style.bg) rule.bg = style.bg;
    if (style.fg) rule.fg = style.fg;
    if (style.bd) rule.bd = { ...style.bd };
    return { rule };
  }

  // ---- galleries ----------------------------------------------------------------------------------------------------------

  // Data bars: six colors, each as a gradient and as a solid fill, from the lowest to the highest value, negative bars red.
  const BAR_COLORS = [
    ["blue", "파랑", "#638ec6"],
    ["green", "녹색", "#63c384"],
    ["red", "빨강", "#ff555a"],
    ["orange", "주황", "#ffb628"],
    ["lightblue", "하늘색", "#008aef"],
    ["purple", "보라", "#d6007b"],
  ];
  const BAR_GROUPS = [
    { id: "grad", label: "그라데이션 채우기", fill: "그라데이션", gradient: true },
    { id: "solid", label: "단색 채우기", fill: "단색", gradient: false },
  ];
  const BARS = BAR_GROUPS.flatMap((group) =>
    BAR_COLORS.map(([color, name, pos]) => ({
      id: `${group.id}-${color}`,
      group: group.id,
      label: `${name} ${group.fill} 데이터 막대`,
      pos,
      gradient: group.gradient,
    }))
  );

  // Color scales, named from the highest color to the lowest as the spreadsheet program names them.
  const W = "#fcfcff";
  const scale = (id, label, colors) => ({ id, label, colors });
  const SCALES = [
    scale("gyr", "녹색 - 노랑 - 빨강 색조", ["#63be7b", "#ffeb84", "#f8696b"]),
    scale("ryg", "빨강 - 노랑 - 녹색 색조", ["#f8696b", "#ffeb84", "#63be7b"]),
    scale("gwr", "녹색 - 흰색 - 빨강 색조", ["#63be7b", W, "#f8696b"]),
    scale("rwg", "빨강 - 흰색 - 녹색 색조", ["#f8696b", W, "#63be7b"]),
    scale("bwr", "파랑 - 흰색 - 빨강 색조", ["#5a8ac6", W, "#f8696b"]),
    scale("rwb", "빨강 - 흰색 - 파랑 색조", ["#f8696b", W, "#5a8ac6"]),
    scale("wr", "흰색 - 빨강 색조", [W, "#f8696b"]),
    scale("rw", "빨강 - 흰색 색조", ["#f8696b", W]),
    scale("gw", "녹색 - 흰색 색조", ["#63be7b", W]),
    scale("wg", "흰색 - 녹색 색조", [W, "#63be7b"]),
    scale("gy", "녹색 - 노랑 색조", ["#63be7b", "#ffeb84"]),
    scale("yg", "노랑 - 녹색 색조", ["#ffeb84", "#63be7b"]),
  ];

  // The icon sets in the groups of the gallery (`icon-map.ts` groups), by the file format's identifier.
  const ICON_GROUPS = [
    { id: "direction", label: "방향", sets: ["3Arrows", "3ArrowsGray", "3Triangles", "4ArrowsGray", "4Arrows", "5ArrowsGray", "5Arrows"] },
    { id: "shape", label: "도형", sets: ["3TrafficLights1", "3TrafficLights2", "3Signs", "4TrafficLights", "4RedToBlack"] },
    { id: "mark", label: "표시기", sets: ["3Symbols", "3Symbols2", "3Flags"] },
    { id: "rank", label: "등급", sets: ["3Stars", "4Rating", "5Rating", "5Quarters", "5Boxes"] },
  ];

  const value = (k, v = "") => ({ k, v });

  function barRule(range, preset) {
    return {
      r: range,
      t: "bar",
      min: value("min"),
      max: value("max"),
      pos: preset.pos,
      neg: "#ff0000",
      grad: preset.gradient,
      show: true,
      stop: false,
    };
  }

  // The lowest color is the last of `colors`; a middle color sits at the 50th percentile.
  function scaleRule(range, preset) {
    const [high, ...rest] = preset.colors;
    const low = rest[rest.length - 1];
    const stops = [{ ...value("min"), c: low }];
    if (rest.length === 2) stops.push({ ...value("percentile", "50"), c: rest[0] });
    stops.push({ ...value("max"), c: high });
    return { r: range, t: "scale", stops, stop: false };
  }

  // One threshold per icon, the lowest icon needing none: the rest at equal percents (67 and 33 for three icons).
  function iconRule(range, set) {
    const count = Number(set[0]);
    const th = [value("percent", "0")];
    for (let i = 1; i < count; i += 1) th.push(value("percent", String(Math.round((100 * i) / count))));
    return { r: range, t: "icons", set, th, rev: false, show: true, stop: false };
  }

  // ---- ranges --------------------------------------------------------------------------------------------------------------

  const meets = (a, b) => a.row0 <= b.row1 && a.row1 >= b.row0 && a.col0 <= b.col1 && a.col1 >= b.col0;

  // `Rectangle.subtract`: the parts of `box` outside `cut` as up to four boxes (above, below, left, right).
  function subtract(box, cut) {
    if (!meets(box, cut)) return [box];
    const parts = [];
    if (cut.row0 > box.row0) parts.push({ ...box, row1: cut.row0 - 1 });
    if (cut.row1 < box.row1) parts.push({ ...box, row0: cut.row1 + 1 });
    const row0 = Math.max(box.row0, cut.row0);
    const row1 = Math.min(box.row1, cut.row1);
    if (cut.col0 > box.col0) parts.push({ row0, row1, col0: box.col0, col1: cut.col0 - 1 });
    if (cut.col1 < box.col1) parts.push({ row0, row1, col0: cut.col1 + 1, col1: box.col1 });
    return parts;
  }

  // `Rectangle.subtractMulti`.
  const subtractAll = (boxes, cuts) => cuts.reduce((rest, cut) => rest.flatMap((box) => subtract(box, cut)), boxes);

  // The rules of `cf` shown for a scope: all of them, or those whose range meets `selection` (boxes).
  function rulesInScope(cf, scope, selection, parseRanges) {
    const shown = [];
    cf.forEach((rule, index) => {
      if (scope === "sheet" || (parseRanges(rule.r) || []).some((box) => selection.some((cut) => meets(box, cut)))) shown.push(index);
    });
    return shown;
  }

  // The rules after the cells of `cuts` are cleared from them: a rule keeps what lies outside, one with nothing left is dropped.
  function clearRange(cf, cuts, parseRanges, formatRanges) {
    const out = [];
    for (const rule of cf) {
      const boxes = parseRanges(rule.r) || [];
      if (!boxes.some((box) => cuts.some((cut) => meets(box, cut)))) {
        out.push(rule);
        continue;
      }
      const rest = subtractAll(boxes, cuts);
      if (rest.length > 0) out.push({ ...rule, r: formatRanges(rest) });
    }
    return out;
  }

  // ---- the border row of the custom format form (PD-284) ----------------------------------------------------------------------
  // The form has one line kind and one color for the sides it ticks, as the cell border dialog's presets do; a rule with edges that
  // differ keeps them until the row is changed (`readCfForm` in `sheet-rules-ui.js`).

  const BORDER_SIDES = ["t", "r", "b", "l"];
  // The lines the row offers: a conditional format takes the thin family only (the spreadsheet program's dialog has no thick or
  // double line for it). A rule from a file may draw others; the row then shows a thin line and leaves the rule's edges alone.
  const BORDER_LINES = ["thin", "dashed", "dotted"];

  // The edges a rule draws for the row's inputs `{ on, line, color, sides }`: `{ t, r, b, l }` of `"<line>:<rrggbb>"` for the ticked
  // sides, null when the row is off or no side is ticked or the line or color is not one the row offers.
  function borderFromControls({ on, line, color, sides }) {
    const text = on && BORDER_LINES.includes(line) ? host.__EASYOFFICE_SHEET_LOOK__.edgeText(line, color) : null;
    if (!text) return null;
    const bd = {};
    for (const side of BORDER_SIDES) if (sides.includes(side)) bd[side] = text;
    return Object.keys(bd).length > 0 ? bd : null;
  }

  // The row's inputs for the edges `bd` of a rule: the line and color of its first edge, the sides it has. A rule with none is off
  // with every side ticked, so turning the row on draws the whole outline.
  function controlsFromBorder(bd) {
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const sides = BORDER_SIDES.filter((side) => look.parseEdge(bd?.[side]));
    if (sides.length === 0) return { on: false, line: "thin", color: "#000000", sides: BORDER_SIDES.slice() };
    const first = look.parseEdge(bd[sides[0]]);
    return { on: true, line: BORDER_LINES.includes(first.line) ? first.line : "thin", color: "#" + first.color, sides };
  }

  host.__EASYOFFICE_SHEET_CF_PRESETS__ = {
    BORDER_SIDES,
    BORDER_LINES,
    borderFromControls,
    controlsFromBorder,
    FORMATS,
    CUSTOM_FORMAT,
    QUICK,
    QUICK_DEFAULTS,
    BAR_GROUPS,
    BARS,
    SCALES,
    ICON_GROUPS,
    quickRule,
    barRule,
    scaleRule,
    iconRule,
    subtract,
    subtractAll,
    rulesInScope,
    clearRange,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
