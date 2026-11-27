(function (host) {
  "use strict";

  // 글꼴 dialog of the word processor (서식 > 글꼴): the 글꼴 tab (글꼴, 글꼴 스타일, 크기, 효과) and the 고급 tab
  // (문자 간격: 간격 표준/넓게/좁게 and 값 in points). Only what the user changed is applied, so a mixed selection keeps
  // the properties the dialog did not touch. The spacing is the run's `spacing` in twips (PD-367).

  const TWIPS_PER_PT = 20;
  const MIN_SPACING_PT = 0.1;
  const MAX_SPACING_PT = 1584;
  const DEFAULT_SPACING_PT = 1;
  const MIXED = "mixed";
  const MSG_RANGE = "값이 범위를 벗어났습니다";
  const SAMPLE_MAX_PX = 28;

  const STYLE_OF = {
    regular: { bold: false, italic: false },
    italic: { bold: false, italic: true },
    bold: { bold: true, italic: false },
    bolditalic: { bold: true, italic: true },
  };

  const styleKey = (bold, italic) => Object.keys(STYLE_OF).find((k) => STYLE_OF[k].bold === bold && STYLE_OF[k].italic === italic);

  // The 간격 and 값 the dialog shows for a run spacing in twips.
  function spacingShown(twips) {
    if (!Number.isInteger(twips) || twips === 0) return { kind: "normal", value: "" };
    return { kind: twips > 0 ? "expanded" : "condensed", value: String(Math.abs(twips) / TWIPS_PER_PT) };
  }

  // The twips of a 간격 and 값 the user chose; null when the value is out of range or not a number.
  function spacingTwips(kind, text) {
    if (kind === "normal") return 0;
    const pt = Number(String(text).trim().replace(",", "."));
    if (String(text).trim() === "" || !Number.isFinite(pt) || pt < MIN_SPACING_PT || pt > MAX_SPACING_PT) return null;
    return Math.round(pt * TWIPS_PER_PT) * (kind === "condensed" ? -1 : 1);
  }

  let bound = false;
  let current = null;

  function open(runs, document) {
    const byId = (id) => document.getElementById(id);
    const els = {
      dialog: byId("doc-font-dialog"), family: byId("doc-font-family"), style: byId("doc-font-style"), size: byId("doc-font-size"),
      strike: byId("doc-font-strike"), sup: byId("doc-font-sup"), sub: byId("doc-font-sub"),
      kind: byId("doc-font-spacing-kind"), value: byId("doc-font-spacing-value"),
      sample: byId("doc-font-sample"), status: byId("doc-font-status"),
    };
    const R = host.__EASYOFFICE_DOC_RUNS__;
    const state = runs.getState?.() || {};

    // Tabs: one pane shows at a time.
    const showPane = (name) => {
      for (const tab of document.querySelectorAll("#doc-font-dialog [data-font-tab]")) {
        const on = tab.dataset.fontTab === name;
        tab.setAttribute("aria-selected", String(on));
        tab.classList.toggle("active", on);
        tab.tabIndex = on ? 0 : -1;
      }
      for (const pane of document.querySelectorAll("#doc-font-dialog [data-font-pane]")) pane.hidden = pane.dataset.fontPane !== name;
    };

    const syncValue = () => {
      els.value.disabled = els.kind.value === "normal";
      if (els.kind.value === "normal") els.value.value = "";
      else if (!els.value.value) els.value.value = String(DEFAULT_SPACING_PT);
    };

    // The sample shows the chosen look at a size the box holds.
    const drawSample = () => {
      const style = STYLE_OF[els.style.value] || STYLE_OF.regular;
      const pt = Number.parseFloat(els.size.value);
      const css = els.sample.style;
      css.fontFamily = els.family.value ? `"${els.family.value}", sans-serif` : "";
      css.fontWeight = style.bold ? "700" : "400";
      css.fontStyle = style.italic ? "italic" : "normal";
      css.fontSize = Number.isFinite(pt) ? `${Math.min(SAMPLE_MAX_PX, Math.max(8, pt * (4 / 3)))}px` : "";
      css.textDecoration = els.strike.checked ? "line-through" : "none";
      const twips = spacingTwips(els.kind.value, els.value.value);
      css.letterSpacing = twips ? `${(twips / TWIPS_PER_PT) * (4 / 3)}px` : "normal";
    };

    // The font list is the style bar's, so the dialog offers what the bar does.
    els.family.replaceChildren();
    for (const o of Array.from(byId("doc-font-select")?.options || [])) {
      const opt = document.createElement("option");
      opt.value = o.value;
      opt.textContent = o.textContent;
      els.family.appendChild(opt);
    }
    const family = typeof state.font === "string" && state.font !== MIXED ? state.font : "";
    if (family && !Array.from(els.family.options).some((o) => o.value === family)) {
      const opt = document.createElement("option");
      opt.value = family;
      opt.textContent = family;
      els.family.appendChild(opt);
    }
    els.family.value = family;
    const initial = {
      family: els.family.value,
      style: state.bold === MIXED || state.italic === MIXED ? "" : styleKey(state.bold === true, state.italic === true),
      size: Number.isInteger(state.size) ? String(state.size / 2) : "",
      strike: state.strike === true, sup: state.superscript === true, sub: state.subscript === true,
      spacing: Number.isInteger(state.spacing) ? state.spacing : null,
    };
    els.style.value = initial.style || "regular";
    els.size.value = initial.size;
    els.strike.checked = initial.strike;
    els.sup.checked = initial.sup;
    els.sub.checked = initial.sub;
    const shown = spacingShown(initial.spacing ?? 0);
    els.kind.value = shown.kind;
    els.value.value = shown.value;
    syncValue();
    els.status.textContent = "";
    showPane("font");
    drawSample();

    current = {
      els, R, initial, runs, document, drawSample, syncValue, showPane,
    };
    if (!bound) {
      bound = true;
      for (const tab of document.querySelectorAll("#doc-font-dialog [data-font-tab]")) tab.addEventListener("click", () => current.showPane(tab.dataset.fontTab));
      els.kind.addEventListener("change", () => {
        current.syncValue();
        current.drawSample();
      });
      for (const el of [els.family, els.style, els.size, els.value, els.strike, els.sup, els.sub]) {
        el.addEventListener("input", () => current.drawSample());
        el.addEventListener("change", () => current.drawSample());
      }
      byId("btn-doc-font-apply").addEventListener("click", () => apply());
      els.dialog.addEventListener("keydown", (ev) => {
        if (ev.key !== "Enter" || ev.target?.tagName === "BUTTON") return;
        ev.preventDefault?.();
        apply();
      });
    }
    els.dialog.showModal();
    els.family.focus();
  }

  // 확인: the patch has only what differs from what the selection had; a spacing out of range keeps the dialog open.
  function apply() {
    const { els, initial, runs } = current;
    const patch = {};
    if (els.family.value && els.family.value !== initial.family) patch.font = els.family.value;
    if (els.style.value !== (initial.style || "regular")) {
      const style = STYLE_OF[els.style.value];
      patch.bold = style.bold ? true : null;
      patch.italic = style.italic ? true : null;
    }
    const pt = Number.parseFloat(els.size.value);
    if (Number.isFinite(pt) && String(pt) !== initial.size) patch.size = current.R.ptToHalfPt(Math.max(1, Math.min(1638, pt)));
    if (els.strike.checked !== initial.strike) patch.strike = els.strike.checked ? true : null;
    if (els.sup.checked !== initial.sup) patch.superscript = els.sup.checked ? true : null;
    if (els.sub.checked !== initial.sub) patch.subscript = els.sub.checked ? true : null;
    const twips = spacingTwips(els.kind.value, els.value.value);
    if (twips === null) {
      els.status.textContent = MSG_RANGE;
      current.showPane("advanced");
      els.value.focus();
      return;
    }
    if (twips !== (initial.spacing ?? 0)) patch.spacing = twips;
    els.dialog.close();
    if (Object.keys(patch).length > 0) runs.applyPatch?.(patch);
  }

  host.__EASYOFFICE_DOC_FONT_DIALOG__ = { open, spacingShown, spacingTwips };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
