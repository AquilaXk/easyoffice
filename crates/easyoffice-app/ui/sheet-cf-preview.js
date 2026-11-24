// Reference implementation (ported, safe JS): Univer `sheets-conditional-formatting-ui`, Apache License 2.0, Copyright 2023-present
// DreamNum Co., Ltd. (https://github.com/dream-num/univer, commit eaf4cfa04fabdbcfc922c692f3450459f8a29546):
//   packages/sheets-conditional-formatting-ui/src/views/ConditionalFormattingRulePreview.tsx lines 30-143 (what a rule shows without
//   looking at the cells: a data bar as a negative and a positive half, a color scale as five samples between its colors, an icon set
//   as its icons, any other rule as "123" in its font and fill);
//   packages/sheets-conditional-formatting/src/models/calculate-unit-v2/utils.ts lines 224-262 (`getColorScaleFromValue`: the color
//   between two stops is the channel-wise interpolation).
// Modifications: the reference draws react elements with CSS utility classes; `previewOf` answers a plain description of the
// preview and `paint` builds its DOM (spans with custom properties, no inline style text). A gradient fades to transparent, not to
// white, so it reads in the dark theme. The icons are the masks of `sheet-rules.js`, listed from the highest as in the reference.
(function (host) {
  "use strict";

  const SAMPLES = 5;
  const SAMPLE_TEXT = "123";

  const channels = (hex) => [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16));
  const toHex = (rgb) => "#" + rgb.map((c) => Math.round(c).toString(16).padStart(2, "0")).join("");

  // The color at `t` (0..1) along `colors` placed at equal distances (`getColorScaleFromValue`).
  function colorAt(colors, t) {
    if (colors.length === 1) return colors[0];
    const at = Math.min(Math.max(t, 0), 1) * (colors.length - 1);
    const lo = Math.min(Math.floor(at), colors.length - 2);
    const from = channels(colors[lo]);
    const to = channels(colors[lo + 1]);
    return toHex(from.map((c, i) => c + (to[i] - c) * (at - lo)));
  }

  const isValidColor = (c) => /^#[0-9a-f]{6}$/i.test(c || "");

  // What `rule` shows: { kind: "bar", neg, pos, gradient } | { kind: "scale", colors } | { kind: "icons", icons: [[shape, color]] } |
  // { kind: "style", text, bg, fg, flags }.
  function previewOf(rule, iconSets) {
    switch (rule?.t) {
      case "bar":
        return { kind: "bar", neg: rule.neg ?? "#ff0000", pos: rule.pos || "#638ec6", gradient: rule.grad !== false };
      case "scale": {
        const stops = (rule.stops || []).map((s) => s.c).filter(isValidColor);
        if (stops.length === 0) return null;
        return { kind: "scale", colors: Array.from({ length: SAMPLES }, (_, i) => colorAt(stops, i / (SAMPLES - 1))) };
      }
      case "icons": {
        const icons = iconSets[rule.set];
        return icons ? { kind: "icons", icons: icons.slice().reverse() } : null;
      }
      case undefined:
        return null;
      default:
        return { kind: "style", text: SAMPLE_TEXT, bg: rule.bg || "", fg: rule.fg || "", flags: rule.fl || 0, ...(rule.bd ? { bd: rule.bd } : {}) };
    }
  }

  function span(document, className) {
    const el = document.createElement("span");
    el.className = className;
    return el;
  }

  // The gradient of one half of a bar: from the bar color to nothing, toward the middle for the negative half.
  function half(document, color, gradient, towardEnd, borderColor) {
    const el = span(document, "cfp-seg");
    el.style.setProperty("--cfp-color", color);
    if (gradient) el.dataset.gradient = towardEnd ? "end" : "start";
    if (borderColor) {
      el.dataset.barBorder = "true";
      el.style.setProperty("--cfp-border", borderColor);
    }
    return el;
  }

  // The edges of a rule (`{ t, r, b, l }` of `"<line>:<rrggbb>"`) as the custom properties the sample's CSS reads; black is the
  // theme's text color, as on the grid.
  function paintEdges(el, border) {
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    for (const [side, name] of [["t", "top"], ["r", "right"], ["b", "bottom"], ["l", "left"]]) {
      const css = look?.edgeCss(border?.[side]);
      if (!css) continue;
      const color = css.color === "#000000" ? "var(--color-text)" : css.color;
      el.style.setProperty("--cfp-b" + side, `${css.width} ${css.style} ${color}`);
    }
  }

  // Builds the preview into `el` (a `.cfp` box). A rule without one leaves it empty.
  function paint(el, rule, iconSets, maskOf) {
    const document = el.ownerDocument;
    el.replaceChildren();
    delete el.dataset.rtl;
    const look = previewOf(rule, iconSets);
    el.dataset.kind = look?.kind || "";
    if (!look) return;
    if (look.kind === "bar") {
      const rtl = rule?.dir === "rightToLeft";
      const hasBorder = Boolean(rule?.bdOn);
      const bpos = hasBorder ? rule.bpos || look.pos : "";
      const bneg = hasBorder ? rule.bneg || look.neg : "";
      if (rtl) el.dataset.rtl = "true";
      if (look.neg !== "") el.appendChild(half(document, look.neg, look.gradient, rtl, bneg));
      el.appendChild(half(document, look.pos, look.gradient, !rtl, bpos));
    } else if (look.kind === "scale") {
      for (const color of look.colors) {
        const seg = span(document, "cfp-seg");
        seg.style.setProperty("--cfp-color", color);
        el.appendChild(seg);
      }
    } else if (look.kind === "icons") {
      for (const [shape, color] of look.icons) {
        const icon = span(document, "cfp-icon");
        icon.style.setProperty("--cfp-icon-mask", maskOf(shape));
        icon.style.setProperty("--cfp-color", color);
        el.appendChild(icon);
      }
    } else {
      const text = span(document, "cfp-text");
      text.textContent = look.text;
      if (look.bg) text.style.setProperty("--cfp-bg", look.bg);
      if (look.fg) text.style.setProperty("--cfp-fg", look.fg);
      paintEdges(text, look.bd);
      for (const [name, bit] of [["bold", 1], ["italic", 2], ["underline", 4], ["strike", 8]]) {
        if (look.flags & bit) text.classList.add("cfp-" + name);
      }
      el.appendChild(text);
    }
  }

  host.__EASYOFFICE_SHEET_CF_PREVIEW__ = { previewOf, paint };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
