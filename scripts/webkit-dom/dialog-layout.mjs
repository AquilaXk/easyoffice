// Layout guard of the dialogs (docs/design-system.md, "대화상자 부품 키트"): opens a dialog in the real browser, measures
// it, and names every control that breaks the size standard. `collectDialog` runs in the page and returns plain numbers;
// `analyzeDialog` judges them in Node, so the rules are testable without a browser (scripts/test-design-guard.mjs).
//
// Rules (all are about the controls, never the labels):
//   same-height / same-top   the controls of one row (`.dialog-row`, `.dialog-footer`, `.dialog-btn-group`) share one height and one top
//   height                   a control is as tall as one of the `--control-height-*` tokens of tokens.css (the Spectrum S size table, PD-378)
//   font                     a control's text is one of the `--control-font-size*` tokens
//   unit-gap                 a unit label (`.dialog-unit`) after a control sits next to it (at most 16px away) on its middle line
//   unit-column              the unit labels of one container (the margin grid of the page setup) are all the same distance from their control, so they share a left edge per column
//   clip                     no box of the dialog holds more than it shows (`scrollWidth > clientWidth`, same for the height)
//   class                    a control-like class (also one a script makes) is in the kit list or grandfathered
import fs from "node:fs";
import path from "node:path";
import { REPO } from "./support.mjs";
import { kitClasses, unregisteredWidgets } from "../design-guard.mjs";
import { KNOWN_WIDGET_CLASSES } from "../design-guard-known.mjs";

/** Row containers whose controls must line up. */
export const ROW_SELECTOR = ".dialog-row, .dialog-footer, .dialog-btn-group, .find-buttons";
/** What counts as a control: the kit's fields and buttons (a number field is its stepper box, `.eo-stepper`, PD-377). Parts inside another control (a combo's trigger, a swatch's chip, the field and the buttons inside a stepper box) are not measured on their own; cells of a gallery (`.sheet-line-item`, `.symbol-cell`) are widgets of their own. */
export const CONTROL_SELECTOR = [
  "input:not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=color]):not([type=range]):not([type=file])",
  "select", ".custom-select", ".eo-stepper", ".sb-color-host", ".dialog-swatch",
  "button.dialog-btn", ".dialog-btn-group button",
].join(", ");
const TOLERANCE_PX = 0.5;
const MAX_UNIT_GAP_PX = 16;
const NON_BOX_TAGS = ["INPUT", "TEXTAREA", "SELECT", "OPTION", "svg", "path"];

/** The px values of `--<prefix>*` in tokens.css, ascending. */
export function tokenSizes(prefix, css = fs.readFileSync(path.join(REPO, "crates/easyoffice-app/ui/tokens.css"), "utf8")) {
  const values = [...css.matchAll(new RegExp(`--${prefix}[a-z0-9-]*:\\s*([\\d.]+)px\\s*;`, "g"))].map((m) => Number(m[1]));
  return [...new Set(values)].sort((a, b) => a - b);
}

/** Runs in the page: `[dialogId, rowSelector, controlSelector, nonBoxTags]` in, the rows, controls and overflowing boxes out (numbers only). */
export function collectDialog([dialogId, rowSelector, controlSelector, nonBoxTags]) {
  const dialog = document.getElementById(dialogId);
  const shown = (el) => {
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0 && getComputedStyle(el).visibility !== "hidden" && !el.closest("[hidden]");
  };
  const name = (el) => {
    const cls = [...el.classList].slice(0, 2).map((c) => `.${c}`).join("");
    return `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ""}${cls}`;
  };
  const rows = [...dialog.querySelectorAll(rowSelector)].filter(shown);
  // Only controls in a row: a glyph grid or a palette has cells of its own size, not form fields.
  const controls = [...dialog.querySelectorAll(controlSelector)]
    .filter((el) => shown(el) && el.closest(rowSelector) && !el.parentElement.closest(controlSelector))
    .map((el) => {
      const rect = el.getBoundingClientRect();
      const textBox = el.querySelector(".custom-select-value") || el;
      const row = el.closest(rowSelector);
      return {
        name: name(el), row: rows.indexOf(row), stacked: getComputedStyle(row).flexDirection.startsWith("column"),
        top: rect.top, height: rect.height, fontSize: Number.parseFloat(getComputedStyle(textBox).fontSize),
      };
    });
  // A unit label ("mm", "pt") is the next sibling of the field it belongs to: where it sits relative to that field, per container.
  const parents = new Map();
  const units = [...dialog.querySelectorAll(".dialog-unit")].filter(shown).flatMap((el) => {
    const field = el.previousElementSibling;
    if (!field || !field.matches(controlSelector) || !shown(field)) return [];
    const a = field.getBoundingClientRect();
    const b = el.getBoundingClientRect();
    if (!parents.has(el.parentElement)) parents.set(el.parentElement, parents.size);
    return [{ name: name(el), parent: parents.get(el.parentElement), gap: b.left - a.right, dy: (b.top + b.bottom) / 2 - (a.top + a.bottom) / 2 }];
  });
  // A box holds more than it shows when its own text is wider than it, or a visible descendant (not a 1px helper such as the
  // colour field's hidden input) reaches past its edge.
  const overflows = (el) => {
    const wide = el.scrollWidth > el.clientWidth + 1;
    const tall = el.scrollHeight > el.clientHeight + 1;
    if (!wide && !tall) return false;
    if ([...el.childNodes].some((node) => node.nodeType === 3 && node.textContent.trim())) return true;
    const box = el.getBoundingClientRect();
    return [...el.querySelectorAll("*")].some((child) => {
      const rect = child.getBoundingClientRect();
      return shown(child) && rect.width > 1 && rect.height > 1 && ((wide && rect.right > box.right + 1) || (tall && rect.bottom > box.bottom + 1));
    });
  };
  const clipped = [...dialog.querySelectorAll("*")]
    .filter((el) => shown(el) && !nonBoxTags.includes(el.tagName) && el.clientWidth > 0 && el.clientHeight > 0)
    .filter((el) => !/auto|scroll/.test(getComputedStyle(el).overflowX + getComputedStyle(el).overflowY))
    .filter(overflows)
    .map((el) => ({ name: name(el), scrollWidth: el.scrollWidth, clientWidth: el.clientWidth, scrollHeight: el.scrollHeight, clientHeight: el.clientHeight }));
  const classes = [...new Set([...dialog.querySelectorAll("*")].flatMap((el) => [...el.classList]))];
  return { dialogId, controls, units, clipped, classes };
}

/** Collects one open dialog of `page`. */
export const measureDialog = (page, dialogId) => page.evaluate(collectDialog, [dialogId, ROW_SELECTOR, CONTROL_SELECTOR, NON_BOX_TAGS]);

const near = (a, b) => Math.abs(a - b) <= TOLERANCE_PX;
const fmt = (n) => (Number.isInteger(n) ? String(n) : n.toFixed(1));

/** The classes a dialog may use for a control: the kit list of docs/design-system.md and the grandfathered ones. */
export function widgetRegistry() {
  return { kit: kitClasses(fs.readFileSync(path.join(REPO, "docs/design-system.md"), "utf8")), legacy: new Set(KNOWN_WIDGET_CLASSES) };
}

/**
 * Violations of one collected dialog as `{ key, message }`. `key` (`dialog rule element`) is what the known-violation list holds;
 * `message` carries the measured values.
 */
export function analyzeDialog(raw, tokens = { height: tokenSizes("control-height-"), font: tokenSizes("control-font-size") }, registry = widgetRegistry()) {
  const out = [];
  const { dialogId, controls, clipped } = raw;
  const add = (rule, element, message) => out.push({ key: `${dialogId} ${rule} ${element}`, message: `#${dialogId} ${rule} ${element}: ${message}` });
  for (const c of controls) {
    if (!tokens.height.some((t) => near(t, c.height))) add("height", c.name, `${fmt(c.height)}px is none of the control heights ${tokens.height.join("/")}`);
    if (!tokens.font.some((t) => near(t, c.fontSize))) add("font", c.name, `font-size ${fmt(c.fontSize)}px is none of the sizes ${tokens.font.join("/")}`);
  }
  const rows = new Map();
  // A row laid out as a column (the buttons beside the fields of the sheet hyperlink dialog) stacks its controls on purpose.
  for (const c of controls.filter((x) => !x.stacked)) rows.set(c.row, [...(rows.get(c.row) ?? []), c]);
  for (const [row, list] of rows) {
    const first = list[0];
    for (const c of list.slice(1)) {
      if (!near(c.height, first.height)) add("same-height", c.name, `row ${row}: ${fmt(c.height)}px beside ${first.name} ${fmt(first.height)}px`);
      else if (!near(c.top, first.top)) add("same-top", c.name, `row ${row}: top ${fmt(c.top)} beside ${first.name} top ${fmt(first.top)}`);
    }
  }
  for (const u of raw.units ?? []) {
    if (u.gap < 0 || u.gap > MAX_UNIT_GAP_PX || Math.abs(u.dy) > TOLERANCE_PX * 2) add("unit-gap", u.name, `${fmt(u.gap)}px from its field, ${fmt(u.dy)}px off the field's middle line (at most ${MAX_UNIT_GAP_PX}px and 1px)`);
  }
  const byParent = new Map();
  for (const u of raw.units ?? []) byParent.set(u.parent, [...(byParent.get(u.parent) ?? []), u]);
  for (const list of byParent.values()) {
    for (const u of list.slice(1)) {
      if (!near(u.gap, list[0].gap)) add("unit-column", u.name, `${fmt(u.gap)}px from its field beside ${list[0].name} ${fmt(list[0].gap)}px`);
    }
  }
  for (const el of clipped) add("clip", el.name, `content ${el.scrollWidth}x${el.scrollHeight} in a ${el.clientWidth}x${el.clientHeight} box`);
  for (const name of unregisteredWidgets(raw.classes ?? [], registry.kit, registry.legacy)) add("class", name, "a control class that is not in the kit list of docs/design-system.md 4-2");
  return out;
}
