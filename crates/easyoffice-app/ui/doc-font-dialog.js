(function (host) {
  "use strict";

  // 글꼴 dialog of the word processor (서식 > 글꼴): the 글꼴 tab (글꼴, 글꼴 스타일, 크기, 효과) and the 고급 tab
  // (문자 간격: 배율 in percent, 간격 표준/넓게/좁게, 위치 표준/올림/내림 and 값 in points, 글꼴 커닝 with its 포인트 이상 size).
  // Only what the user changed is applied, so a mixed selection keeps the properties the dialog did not touch. The spacing is
  // the run's `spacing` in twips (PD-367); `scale` (percent), `position` and `kerning` (half-points) are PD-372's.

  const TWIPS_PER_PT = 20;
  const HALF_PT_PER_PT = 2;
  const MIN_SPACING_PT = 0.1;
  const MAX_SPACING_PT = 1584;
  const DEFAULT_SPACING_PT = 1;
  const MIN_SCALE = 1;
  const MAX_SCALE = 600;
  const NORMAL_SCALE = 100;
  const MIN_POSITION_PT = 0.5;
  const MAX_POSITION_PT = 1584;
  const DEFAULT_POSITION_PT = 3;
  const MIN_KERNING_PT = 1;
  const MAX_KERNING_PT = 1638;
  const DEFAULT_KERNING_PT = 1;
  const MIXED = "mixed";
  const MSG_RANGE = "값이 범위를 벗어났습니다";
  const SAMPLE_MAX_PX = 28;
  const SAMPLE_MAX_SHIFT_PX = 10;

  const STYLE_OF = {
    regular: { bold: false, italic: false },
    italic: { bold: false, italic: true },
    bold: { bold: true, italic: false },
    bolditalic: { bold: true, italic: true },
  };

  const styleKey = (bold, italic) => Object.keys(STYLE_OF).find((k) => STYLE_OF[k].bold === bold && STYLE_OF[k].italic === italic);

  const PLAIN_NUMBER_RE = /^-?(\d+\.?\d*|\.\d+)$/;
  const UNIT_SUFFIX_RE = /\s*(pt|%)$/i;
  const SAMPLE_NOTE = "화면과 프린터 모두에 같은 글꼴이 사용됩니다.";

  // A number typed in a box, with or without its unit (`3`, `3pt`, `150 %`): a decimal comma is allowed, an empty or
  // non-numeric text is NaN.
  const numberOf = (text) => {
    const t = String(text).trim().replace(",", ".").replace(UNIT_SUFFIX_RE, "");
    return PLAIN_NUMBER_RE.test(t) ? Number(t) : Number.NaN;
  };

  // A value as the dialog shows it: the unit sits inside the field (`2pt`), and a blank stays blank.
  const withUnit = (value, unit) => (value === "" || value === null || value === undefined ? "" : `${value}${unit}`);

  // The 간격 and 값 the dialog shows for a run spacing in twips.
  function spacingShown(twips) {
    if (!Number.isInteger(twips) || twips === 0) return { kind: "normal", value: "" };
    return { kind: twips > 0 ? "expanded" : "condensed", value: String(Math.abs(twips) / TWIPS_PER_PT) };
  }

  // The twips of a 간격 and 값 the user chose; null when the value is out of range or not a number.
  function spacingTwips(kind, text) {
    if (kind === "normal") return 0;
    const pt = numberOf(text);
    if (!Number.isFinite(pt) || pt < MIN_SPACING_PT || pt > MAX_SPACING_PT) return null;
    return Math.round(pt * TWIPS_PER_PT) * (kind === "condensed" ? -1 : 1);
  }

  // The 위치 and 값 the dialog shows for a run position in half-points (positive raises).
  function positionShown(halfPt) {
    if (!Number.isInteger(halfPt) || halfPt === 0) return { kind: "normal", value: "" };
    return { kind: halfPt > 0 ? "raised" : "lowered", value: String(Math.abs(halfPt) / HALF_PT_PER_PT) };
  }

  // The half-points of a 위치 and 값 the user chose (a half-point is the smallest step); null when out of range.
  function positionHalfPt(kind, text) {
    if (kind === "normal") return 0;
    const pt = numberOf(text);
    if (!Number.isFinite(pt) || pt < MIN_POSITION_PT || pt > MAX_POSITION_PT) return null;
    return Math.round(pt * HALF_PT_PER_PT) * (kind === "lowered" ? -1 : 1);
  }

  // The 배율 in percent the user typed (a whole number, an optional trailing %); null when out of range.
  function scalePercent(text) {
    const n = numberOf(String(text).replace(/%\s*$/, ""));
    return Number.isInteger(n) && n >= MIN_SCALE && n <= MAX_SCALE ? n : null;
  }

  // The kerning threshold in half-points of a 글꼴 커닝 box and its 포인트 이상 value: 0 when unchecked, null when out of range.
  function kerningHalfPt(checked, text) {
    if (!checked) return 0;
    const pt = numberOf(text);
    if (!Number.isFinite(pt) || pt < MIN_KERNING_PT || pt > MAX_KERNING_PT) return null;
    return Math.round(pt * HALF_PT_PER_PT);
  }

  // A unit field shows its number with the unit inside (`2pt`): leaving it completes a bare number, and a text
  // that is not a number stays as typed so 확인 can name the box.
  function bindUnitFields(dialog) {
    for (const input of dialog.querySelectorAll("input[data-unit]")) {
      const complete = () => {
        const n = numberOf(input.value);
        if (Number.isFinite(n)) input.value = withUnit(n, input.dataset.unit);
      };
      input.addEventListener("change", complete);
      input.addEventListener("blur", complete);
    }
  }

  // The preset lists of the combo boxes (`data-presets`: 배율's 200 % ... 33 %).
  function bindPresetCombos(dialog) {
    for (const input of dialog.querySelectorAll("input[data-presets]")) {
      const items = input.dataset.presets.split(",").map((n) => withUnit(n, input.dataset.unit));
      host.__EASYOFFICE_DIALOG_COMBO__?.attach(input, { items, arrowLabel: "배율 목록 열기" });
    }
  }

  // Alt+letter (Option+letter on a Mac, whose `key` is a symbol, so the physical key is read) moves the focus to the control
  // whose label shows that letter in the visible tab, or to the button that does: a check box flips, a button presses.
  function accessKeyTarget(dialog, letter) {
    const visible = (el) => !el.closest("[hidden]");
    const hit = Array.from(dialog.querySelectorAll("[data-accesskey]")).find((el) => el.dataset.accesskey === letter && visible(el));
    if (!hit) return null;
    return hit.tagName === "SELECT" ? dialog.querySelector(`.custom-select[data-select-id="${hit.id}"] .custom-select-trigger`) || hit : hit;
  }

  function onAccessKey(ev, dialog) {
    if (!ev.altKey || ev.ctrlKey || ev.metaKey) return;
    const letter = /^Key[A-Z]$/.test(ev.code || "") ? ev.code.slice(3).toLowerCase() : String(ev.key).toLowerCase();
    const target = /^[a-z]$/.test(letter) ? accessKeyTarget(dialog, letter) : null;
    if (!target) return;
    ev.preventDefault?.();
    target.focus?.();
    if (target.getAttribute("type") === "checkbox" || target.tagName === "BUTTON") target.click();
  }

  let bound = false;
  let current = null;

  function open(runs, document) {
    const byId = (id) => document.getElementById(id);
    const els = {
      dialog: byId("doc-font-dialog"), family: byId("doc-font-family"), style: byId("doc-font-style"), size: byId("doc-font-size"),
      strike: byId("doc-font-strike"), sup: byId("doc-font-sup"), sub: byId("doc-font-sub"),
      scale: byId("doc-font-scale"),
      kind: byId("doc-font-spacing-kind"), value: byId("doc-font-spacing-value"),
      posKind: byId("doc-font-position-kind"), posValue: byId("doc-font-position-value"),
      kern: byId("doc-font-kerning"), kernValue: byId("doc-font-kerning-value"),
      sample: byId("doc-font-sample"), sampleText: byId("doc-font-sample-text"), note: byId("doc-font-sample-note"), status: byId("doc-font-status"),
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

    // A 값 box takes a number only while its kind (or the 글꼴 커닝 box) is on, and starts at the kind's default.
    const syncValue = () => {
      for (const [on, box, fallback] of [
        [els.kind.value !== "normal", els.value, DEFAULT_SPACING_PT],
        [els.posKind.value !== "normal", els.posValue, DEFAULT_POSITION_PT],
        [els.kern.checked, els.kernValue, DEFAULT_KERNING_PT],
      ]) {
        box.disabled = !on;
        if (!on) box.value = "";
        else if (!box.value) box.value = withUnit(fallback, "pt");
      }
    };

    // The sample shows the chosen look at a size the box holds. The width scale is drawn for real here (a transform on the
    // sample's own box); the document's runs approximate it (PD-372).
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
      const text = els.sampleText.style;
      const scale = scalePercent(els.scale.value);
      text.transform = scale && scale !== NORMAL_SCALE ? `scaleX(${scale / 100})` : "";
      const half = positionHalfPt(els.posKind.value, els.posValue.value);
      const shift = half ? Math.max(-SAMPLE_MAX_SHIFT_PX, Math.min(SAMPLE_MAX_SHIFT_PX, (half / HALF_PT_PER_PT) * (4 / 3))) : 0;
      text.verticalAlign = shift ? `${shift}px` : "";
      css.fontKerning = els.kern.checked ? "normal" : "";
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
    if (family && !Array.from(els.family.querySelectorAll("option")).some((o) => o.value === family)) {
      const opt = document.createElement("option");
      opt.value = family;
      opt.textContent = family;
      els.family.appendChild(opt);
    }
    els.family.value = family;
    const integer = (v) => (Number.isInteger(v) ? v : null);
    const initial = {
      family: els.family.value,
      style: state.bold === MIXED || state.italic === MIXED ? "" : styleKey(state.bold === true, state.italic === true),
      size: Number.isInteger(state.size) ? String(state.size / 2) : "",
      strike: state.strike === true, sup: state.superscript === true, sub: state.subscript === true,
      spacing: integer(state.spacing),
      // A mixed selection leaves 배율 blank; text without a scale shows 100.
      scale: state.scale === MIXED ? null : (integer(state.scale) ?? NORMAL_SCALE),
      position: integer(state.position),
      kerning: integer(state.kerning),
    };
    els.style.value = initial.style || "regular";
    els.size.value = initial.size;
    els.strike.checked = initial.strike;
    els.sup.checked = initial.sup;
    els.sub.checked = initial.sub;
    const shown = spacingShown(initial.spacing ?? 0);
    els.kind.value = shown.kind;
    els.value.value = withUnit(shown.value, "pt");
    els.scale.value = initial.scale === null ? "" : withUnit(initial.scale, "%");
    const shownPosition = positionShown(initial.position ?? 0);
    els.posKind.value = shownPosition.kind;
    els.posValue.value = withUnit(shownPosition.value, "pt");
    els.kern.checked = (initial.kerning ?? 0) > 0;
    els.kernValue.value = els.kern.checked ? withUnit(initial.kerning / HALF_PT_PER_PT, "pt") : "";
    syncValue();
    els.status.textContent = "";
    els.note.textContent = SAMPLE_NOTE;
    showPane("font");
    drawSample();

    // A modal dialog takes the selection from the editor (WebKit collapses it), so 확인 applies to the range the dialog opened on.
    current = {
      els, R, initial, runs, document, drawSample, syncValue, showPane, range: runs.getLastRange?.() ?? null,
    };
    if (!bound) {
      bound = true;
      for (const tab of document.querySelectorAll("#doc-font-dialog [data-font-tab]")) tab.addEventListener("click", () => current.showPane(tab.dataset.fontTab));
      for (const el of [els.kind, els.posKind, els.kern]) {
        el.addEventListener("change", () => {
          current.syncValue();
          current.drawSample();
        });
      }
      for (const el of [els.family, els.style, els.size, els.value, els.scale, els.posValue, els.kernValue, els.strike, els.sup, els.sub]) {
        el.addEventListener("input", () => current.drawSample());
        el.addEventListener("change", () => current.drawSample());
      }
      bindUnitFields(els.dialog);
      bindPresetCombos(els.dialog);
      byId("btn-doc-font-apply").addEventListener("click", () => apply());
      els.dialog.addEventListener("keydown", (ev) => {
        onAccessKey(ev, els.dialog);
        if (ev.defaultPrevented) return;
        if (ev.key !== "Enter" || ev.target?.tagName === "BUTTON") return;
        ev.preventDefault?.();
        apply();
      });
    }
    els.dialog.showModal();
    els.family.focus();
  }

  // The 고급 tab's values the user changed as a patch, or the box whose value is out of range. A 배율 left blank on a mixed
  // selection, and a 글꼴 커닝 size the selection already had, are not read again.
  function advancedPatch(els, initial) {
    const patch = {};
    const bad = (box) => ({ box });
    const twips = spacingTwips(els.kind.value, els.value.value);
    if (twips === null) return bad(els.value);
    if (twips !== (initial.spacing ?? 0)) patch.spacing = twips;
    if (!(initial.scale === null && els.scale.value.trim() === "")) {
      const scale = scalePercent(els.scale.value);
      if (scale === null) return bad(els.scale);
      if (scale !== initial.scale) patch.scale = scale;
    }
    const position = positionHalfPt(els.posKind.value, els.posValue.value);
    if (position === null) return bad(els.posValue);
    if (position !== (initial.position ?? 0)) patch.position = position;
    const sameKerning = els.kern.checked && initial.kerning && numberOf(els.kernValue.value) === initial.kerning / HALF_PT_PER_PT;
    const kerning = sameKerning ? initial.kerning : kerningHalfPt(els.kern.checked, els.kernValue.value);
    if (kerning === null) return bad(els.kernValue);
    if (kerning !== (initial.kerning ?? 0)) patch.kerning = kerning;
    return { patch };
  }

  // 확인: the patch has only what differs from what the selection had; a value out of range keeps the dialog open.
  function apply() {
    const { els, initial, runs } = current;
    const patch = {};
    // Another font drops the variant, as the style bar's font list does (`setFont`, PD-368).
    if (els.family.value && els.family.value !== initial.family) Object.assign(patch, { font: els.family.value, face: null });
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
    const advanced = advancedPatch(els, initial);
    if (advanced.box) {
      els.status.textContent = MSG_RANGE;
      current.showPane("advanced");
      advanced.box.focus();
      return;
    }
    Object.assign(patch, advanced.patch);
    els.dialog.close();
    if (Object.keys(patch).length > 0) runs.applyPatch?.(patch, current.range ? { range: current.range } : {});
  }

  host.__EASYOFFICE_DOC_FONT_DIALOG__ = {
    open, spacingShown, spacingTwips, positionShown, positionHalfPt, scalePercent, kerningHalfPt,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
