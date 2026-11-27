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
  const MSG_NO_DEFAULT = "기본값으로 정할 글꼴이나 크기가 없습니다";
  const SAMPLE_MAX_PX = 28;
  const SAMPLE_MAX_SHIFT_PX = 10;

  const STYLE_OF = {
    regular: { bold: false, italic: false },
    italic: { bold: false, italic: true },
    bold: { bold: true, italic: false },
    bolditalic: { bold: true, italic: true },
  };

  const STYLE_LABEL = { regular: "보통", italic: "기울임꼴", bold: "굵게", bolditalic: "굵게 기울임꼴" };

  const styleKey = (bold, italic) => Object.keys(STYLE_OF).find((k) => STYLE_OF[k].bold === bold && STYLE_OF[k].italic === italic);
  // The style a 글꼴 스타일 text names; `undefined` for a text that is not in the list.
  const styleOfText = (text) => Object.keys(STYLE_LABEL).find((k) => STYLE_LABEL[k] === String(text).trim());

  const OT = host.__EASYOFFICE_DOC_OPEN_TYPE__;
  const FX = host.__EASYOFFICE_DOC_TEXT_FX__;
  // The color boxes by the run value they set, and the controls that set a part of the packed text effects.
  const COLOR_HOST = { color: "#doc-font-color-host", ucolor: "#doc-font-underline-color-host" };
  // What the sample takes from the effects besides the decoration line (the sample box holds the caps, dimming and marks).
  const FX_SAMPLE_KEYS = ["textDecorationStyle", "textDecorationColor", "textDecorationThickness", "fontVariantCaps", "textTransform", "opacity", "textEmphasisStyle", "textEmphasisPosition"];

  // The packed text effects of the controls on top of `base` (the selection's value; `null` when it is mixed): only a control
  // the user touched replaces its part.
  function composeFx(els, base, touched) {
    const was = FX.parts(base || 0);
    const t = (key) => touched.has(key);
    return FX.pack({
      doubleStrike: t("dstrike") ? els.dstrike.checked : was.doubleStrike,
      smallCaps: t("smallcaps") ? els.smallcaps.checked : was.smallCaps,
      allCaps: t("allcaps") ? els.allcaps.checked : was.allCaps,
      hidden: t("hidden") ? els.hidden.checked : was.hidden,
      emphasis: t("emphasis") ? Number(els.emphasis.value) : was.emphasis,
      underline: t("ulstyle") ? (els.ulStyle.value === "none" ? 0 : Number(els.ulStyle.value)) : was.underline,
    });
  }
  const PLAIN_NUMBER_RE =/^-?(\d+\.?\d*|\.\d+)$/;
  const UNIT_SUFFIX_RE = /\s*(pt|%)$/i;
  const SAMPLE_NOTE = "화면과 프린터 모두에 같은 글꼴이 사용됩니다.";

  // A number typed in a box, with or without its unit (`3`, `3pt`, `150 %`): a decimal comma is allowed, an empty or
  // non-numeric text is NaN.
  const numberOf = (text) => {
    const t = String(text).trim().replace(",", ".").replace(UNIT_SUFFIX_RE, "");
    return PLAIN_NUMBER_RE.test(t) ? Number(t) : Number.NaN;
  };

  // A value as the number field shows it (the unit is the label beside it), and a blank stays blank.
  const shownNumber = (value) => (value === "" || value === null || value === undefined ? "" : String(value));

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

  // A number field (`type="number"`, the unit is a label beside it) takes only digits when typed, so the unit letters of what
  // is pasted (`2pt`, `150 %`, `1,5`) go: the pasted text becomes its number when it is one.
  function bindNumberFields(dialog) {
    for (const input of dialog.querySelectorAll('input[type="number"]')) {
      input.addEventListener("paste", (event) => {
        const n = numberOf(event.clipboardData?.getData?.("text") ?? "");
        if (!Number.isFinite(n)) return;
        event.preventDefault?.();
        input.value = String(n);
        for (const type of ["input", "change"]) {
          input.dispatchEvent(typeof host.Event === "function" ? new host.Event(type, { bubbles: true }) : { type, bubbles: true, target: input });
        }
      });
    }
  }

  // The list boxes of the 글꼴 tab (`data-list` on the field, the `<id>-list` box under it: 글꼴 스타일, 크기). Returns them; the
  // highlighted row follows the field. 배율's presets are the field's own `<datalist>`.
  function bindListBoxes(dialog) {
    const combo = host.__EASYOFFICE_DIALOG_COMBO__;
    return Array.from(dialog.querySelectorAll("input[data-list]")).map((input) => (
      combo?.attach(input, dialog.querySelector(`#${input.id}-list`), input.dataset.list.split(","))
    ));
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

  const PENDING_TITLE = "아직 지원하지 않는 항목입니다";
  let bound = false;
  let current = null;
  let listBoxes = [];

  function open(runs, document) {
    const byId = (id) => document.getElementById(id);
    const els = {
      dialog: byId("doc-font-dialog"), family: byId("doc-font-family"), latin: byId("doc-font-latin"), style: byId("doc-font-style"), size: byId("doc-font-size"),
      strike: byId("doc-font-strike"), sup: byId("doc-font-sup"), sub: byId("doc-font-sub"),
      scale: byId("doc-font-scale"),
      kind: byId("doc-font-spacing-kind"), value: byId("doc-font-spacing-value"),
      posKind: byId("doc-font-position-kind"), posValue: byId("doc-font-position-value"),
      kern: byId("doc-font-kerning"), kernValue: byId("doc-font-kerning-value"),
      grid: byId("doc-font-grid"), ot: OT.controls(document),
      color: byId("doc-font-color"), ulStyle: byId("doc-font-underline-style"),
      dstrike: byId("doc-font-dstrike"), smallcaps: byId("doc-font-smallcaps"), allcaps: byId("doc-font-allcaps"),
      hidden: byId("doc-font-hidden"), emphasis: byId("doc-font-emphasis"),
      sample: byId("doc-font-sample"), sampleText: byId("doc-font-sample-text"), note: byId("doc-font-sample-note"), status: byId("doc-font-status"),
    };
    const R = host.__EASYOFFICE_DOC_RUNS__;
    const state = runs.getState?.() || {};
    // The OpenType controls (and the line-grid box) the user touched: only those replace what the selection had.
    const touched = new Set();
    // The 글꼴 색 and 밑줄 색 the dialog holds (6 hex digits; "" is 자동, the text color of the page).
    const colors = { color: "", ucolor: "" };
    const colorNow = () => colors.color;
    const paintColor = (key) => {
      const host_ = els.dialog.querySelector(COLOR_HOST[key]);
      const chip = host_?.querySelector(".dialog-swatch-chip");
      const label = host_?.querySelector(".dialog-swatch-text");
      if (chip) chip.style.background = colors[key] ? `#${colors[key]}` : "";
      if (label) label.textContent = colors[key] ? `#${colors[key]}` : "자동";
    };
    const setColor = (key, picked) => {
      colors[key] = String(picked || "").replace(/^#/, "").toUpperCase();
      touched.add(key);
      paintColor(key);
      drawSample();
    };
    // The packed text effects of the controls on top of what the selection had (`null`: mixed, nothing known).
    const fxNow = () => composeFx(els, initial.textfx, touched);

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
        else if (!box.value) box.value = shownNumber(fallback);
      }
    };

    // The sample shows the chosen look at a size the box holds. The width scale is drawn for real here (a transform on the
    // sample's own box); the document's runs approximate it (PD-372).
    const drawSample = () => {
      const style = STYLE_OF[styleOfText(els.style.value)] || STYLE_OF.regular;
      const pt = Number.parseFloat(els.size.value);
      const css = els.sample.style;
      css.fontFamily = [els.latin.value, els.family.value].filter(Boolean).map((f) => `"${f}"`).concat("sans-serif").join(", ");
      css.fontWeight = style.bold ? "700" : "400";
      css.fontStyle = style.italic ? "italic" : "normal";
      css.fontSize = Number.isFinite(pt) ? `${Math.min(SAMPLE_MAX_PX, Math.max(8, pt * (4 / 3)))}px` : "";
      const underlined = els.ulStyle.value !== "none";
      css.textDecoration = [underlined ? "underline" : "", els.strike.checked ? "line-through" : ""].filter(Boolean).join(" ") || "none";
      css.color = colorNow() ? `#${colorNow()}` : "";
      const fx = FX.css({ underline: underlined, strike: els.strike.checked, textfx: fxNow(), ucolor: colors.ucolor });
      if (fx.textDecoration) css.textDecoration = fx.textDecoration;
      for (const key of FX_SAMPLE_KEYS) css[key] = fx[key];
      const twips = spacingTwips(els.kind.value, els.value.value);
      css.letterSpacing = twips ? `${(twips / TWIPS_PER_PT) * (4 / 3)}px` : "normal";
      const text = els.sampleText.style;
      const scale = scalePercent(els.scale.value);
      text.transform = scale && scale !== NORMAL_SCALE ? `scaleX(${scale / 100})` : "";
      const half = positionHalfPt(els.posKind.value, els.posValue.value);
      const shift = half ? Math.max(-SAMPLE_MAX_SHIFT_PX, Math.min(SAMPLE_MAX_SHIFT_PX, (half / HALF_PT_PER_PT) * (4 / 3))) : 0;
      text.verticalAlign = shift ? `${shift}px` : "";
      css.fontKerning = els.kern.checked ? "normal" : "";
      Object.assign(text, OT.css(OT.compose(els.ot, initial.opentype, touched)));
    };

    // The font list is the style bar's, so the dialog offers what the bar does.
    // 한글 글꼴 is the East Asian face (the Latin one when the run names no other) and 영어 글꼴 the Latin one (PD-376).
    const named = (v) => (typeof v === "string" && v !== MIXED ? v : "");
    const latin = named(state.font);
    const family = state.eastfont === MIXED ? "" : named(state.eastfont) || latin;
    for (const [select, value] of [[els.family, family], [els.latin, latin]]) {
      select.replaceChildren();
      for (const o of Array.from(byId("doc-font-select")?.options || [])) {
        const opt = document.createElement("option");
        opt.value = o.value;
        opt.textContent = o.textContent;
        select.appendChild(opt);
      }
      if (value && !Array.from(select.querySelectorAll("option")).some((o) => o.value === value)) {
        const opt = document.createElement("option");
        opt.value = value;
        opt.textContent = value;
        select.appendChild(opt);
      }
      select.value = value;
    }
    const integer = (v) => (Number.isInteger(v) ? v : null);
    const initial = {
      family: els.family.value,
      latin: els.latin.value,
      fontsMixed: state.font === MIXED || state.eastfont === MIXED,
      style: state.bold === MIXED || state.italic === MIXED ? "" : styleKey(state.bold === true, state.italic === true),
      size: Number.isInteger(state.size) ? String(state.size / 2) : "",
      strike: state.strike === true, sup: state.superscript === true, sub: state.subscript === true,
      spacing: integer(state.spacing),
      // A mixed selection leaves 배율 blank; text without a scale shows 100.
      scale: state.scale === MIXED ? null : (integer(state.scale) ?? NORMAL_SCALE),
      position: integer(state.position),
      kerning: integer(state.kerning),
      // A mixed selection (null) shows the defaults and keeps each part it did not touch.
      opentype: state.opentype === MIXED ? null : (integer(state.opentype) ?? 0),
      nogrid: state.nogrid === MIXED ? null : state.nogrid === 1,
      color: state.color === MIXED ? null : (typeof state.color === "string" ? state.color : ""),
      underline: state.underline === true,
      // The packed text effects and the underline colour; a mixed selection (null) shows none and keeps each part it did not touch.
      textfx: state.textfx === MIXED ? null : (integer(state.textfx) ?? 0),
      ucolor: state.ucolor === MIXED ? null : (typeof state.ucolor === "string" ? state.ucolor : ""),
    };
    colors.color = initial.color || "";
    colors.ucolor = initial.ucolor || "";
    paintColor("color");
    paintColor("ucolor");
    const fxParts = FX.parts(initial.textfx || 0);
    els.ulStyle.value = initial.underline ? String(fxParts.underline) : "none";
    els.dstrike.checked = fxParts.doubleStrike;
    els.smallcaps.checked = fxParts.smallCaps;
    els.allcaps.checked = fxParts.allCaps;
    els.hidden.checked = fxParts.hidden;
    els.emphasis.value = String(fxParts.emphasis);
    els.style.value = STYLE_LABEL[initial.style || "regular"];
    els.size.value = initial.size;
    els.strike.checked = initial.strike;
    els.sup.checked = initial.sup;
    els.sub.checked = initial.sub;
    const shown = spacingShown(initial.spacing ?? 0);
    els.kind.value = shown.kind;
    els.value.value = shownNumber(shown.value);
    els.scale.value = shownNumber(initial.scale);
    const shownPosition = positionShown(initial.position ?? 0);
    els.posKind.value = shownPosition.kind;
    els.posValue.value = shownNumber(shownPosition.value);
    els.kern.checked = (initial.kerning ?? 0) > 0;
    els.kernValue.value = els.kern.checked ? shownNumber(initial.kerning / HALF_PT_PER_PT) : "";
    OT.show(els.ot, initial.opentype);
    els.grid.checked = initial.nogrid !== true;
    els.grid.indeterminate = initial.nogrid === null;
    syncValue();
    els.status.textContent = "";
    els.note.textContent = SAMPLE_NOTE;
    showPane("font");
    drawSample();

    // A modal dialog takes the selection from the editor (WebKit collapses it), so 확인 applies to the range the dialog opened on.
    current = {
      els, R, initial, runs, document, drawSample, syncValue, showPane, touched, setColor, colors, fxNow,
      range: runs.getLastRange?.() ?? null,
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
      for (const el of [els.family, els.latin, els.style, els.size, els.value, els.scale, els.posValue, els.kernValue, els.strike, els.sup, els.sub]) {
        el.addEventListener("input", () => current.drawSample());
        el.addEventListener("change", () => current.drawSample());
      }
      for (const [key, el] of [...Object.entries(els.ot), ["grid", els.grid]]) {
        el.addEventListener("change", () => {
          current.touched.add(key);
          if (key === "grid") current.els.grid.indeterminate = false;
          current.drawSample();
        });
      }
      bindNumberFields(els.dialog);
      listBoxes = bindListBoxes(els.dialog);
      for (const key of Object.keys(COLOR_HOST)) {
        host.__EASYOFFICE_COLOR_PICKER__?.setupColorPicker(els.dialog.querySelector(COLOR_HOST[key]), (picked) => current.setColor(key, picked), { clear: true, clearLabel: "자동" });
      }
      // The text effects: a check box, the underline style or the emphasis mark marks its part as the user's. A line through
      // the text is single or double, and the capitals are small or all, never both.
      const EXCLUSIVE = { strike: "dstrike", dstrike: "strike", smallcaps: "allcaps", allcaps: "smallcaps" };
      for (const [key, el] of [["ulstyle", els.ulStyle], ["emphasis", els.emphasis], ["dstrike", els.dstrike], ["smallcaps", els.smallcaps], ["allcaps", els.allcaps], ["hidden", els.hidden], ["strike", els.strike]]) {
        el.addEventListener("change", () => {
          if (key !== "strike") current.touched.add(key);
          const other = EXCLUSIVE[key] && current.els[EXCLUSIVE[key]];
          if (el.checked && other) {
            other.checked = false;
            current.touched.add(EXCLUSIVE[key]);
          }
          current.drawSample();
        });
      }
      // Items whose run property the engine does not keep yet stay visible and off.
      for (const el of els.dialog.querySelectorAll("[data-pending]")) {
        el.disabled = true;
        el.title = PENDING_TITLE;
      }
      byId("btn-doc-font-apply").addEventListener("click", () => apply());
      byId("btn-doc-font-default").addEventListener("click", () => setAsDefault());
      els.dialog.addEventListener("keydown", (ev) => {
        onAccessKey(ev, els.dialog);
        if (ev.defaultPrevented) return;
        if (ev.key !== "Enter" || ev.target?.tagName === "BUTTON") return;
        ev.preventDefault?.();
        apply();
      });
    }
    for (const box of listBoxes) box?.sync();
    els.dialog.showModal();
    els.family.focus();
  }

  // The 고급 tab's values the user changed as a patch, or the box whose value is out of range. A 배율 left blank on a mixed
  // selection, and a 글꼴 커닝 size the selection already had, are not read again.
  function advancedPatch(els, initial, touched) {
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
    // The OpenType choices the user touched are a `{ mask, value }` patch: every run takes those bits and keeps the rest.
    const otMask = OT.maskOf(touched);
    if (otMask) patch.opentype = { mask: otMask, value: OT.compose(els.ot, 0, touched) };
    if (touched.has("grid")) patch.nogrid = els.grid.checked ? null : 1;
    return { patch };
  }

  // 기본값으로 설정(D): the font and size of the dialog become the document's defaults after the prompt (only this document:
  // the app has no template for "all documents"). The selection itself is left as it is.
  function setAsDefault() {
    const { els, document, R } = current;
    const defaults = host.__EASYOFFICE_DOC_FONT_DEFAULTS__;
    const font = els.latin.value || els.family.value || "";
    const eastFont = els.family.value || font;
    const pt = Number.parseFloat(els.size.value);
    const sizeHalfPt = Number.isFinite(pt) ? R.ptToHalfPt(Math.max(1, Math.min(1638, pt))) : undefined;
    if (!font && sizeHalfPt === undefined) {
      els.status.textContent = MSG_NO_DEFAULT;
      return;
    }
    const faces = font === eastFont ? [font] : [font, eastFont];
    const summary = `기본 글꼴을 ${[...faces, sizeHalfPt === undefined ? "" : `${sizeHalfPt / 2}pt`].filter(Boolean).join(" ")}로 설정합니다.`;
    defaults?.ask(document, summary, () => {
      const root = document.getElementById("doc-editor");
      if (root && defaults.setDefaults(root, { font, eastFont, sizeHalfPt })) els.dialog.close();
    });
  }

  // The font part of the patch. Another font drops the variant, as the style bar's font list does (`setFont`, PD-368). When the
  // selection is uniform the patch names both faces (the East Asian one kept apart only when it differs); when the faces are mixed
  // it names only the one the user changed, so each run keeps the other (`keepEast` keeps the East Asian face of a run with one font).
  function fontPatch(els, initial) {
    const latinChanged = Boolean(els.latin.value) && els.latin.value !== initial.latin;
    const eastChanged = Boolean(els.family.value) && els.family.value !== initial.family;
    if (!latinChanged && !eastChanged) return {};
    if (!initial.fontsMixed) {
      const latinFont = els.latin.value || els.family.value;
      const eastFont = els.family.value || latinFont;
      // The variant (PostScript name) belongs to the Latin font: only a new Latin font drops it.
      return { font: latinFont, eastfont: eastFont === latinFont ? null : eastFont, ...(latinChanged ? { face: null } : {}) };
    }
    const patch = {};
    if (latinChanged) Object.assign(patch, { font: els.latin.value, face: null });
    if (eastChanged) patch.eastfont = els.family.value;
    else patch.keepEast = true;
    return patch;
  }

  // 확인: the patch has only what differs from what the selection had; a value out of range keeps the dialog open.
  function apply() {
    const { els, initial, runs } = current;
    const patch = {};
    Object.assign(patch, fontPatch(els, initial));
    const chosen = styleOfText(els.style.value);
    if (chosen && chosen !== (initial.style || "regular")) {
      const style = STYLE_OF[chosen];
      patch.bold = style.bold ? true : null;
      patch.italic = style.italic ? true : null;
    }
    const pt = Number.parseFloat(els.size.value);
    if (Number.isFinite(pt) && String(pt) !== initial.size) patch.size = current.R.ptToHalfPt(Math.max(1, Math.min(1638, pt)));
    if (els.strike.checked !== initial.strike) patch.strike = els.strike.checked ? true : null;
    if (els.sup.checked !== initial.sup) patch.superscript = els.sup.checked ? true : null;
    if (els.sub.checked !== initial.sub) patch.subscript = els.sub.checked ? true : null;
    const underlined = els.ulStyle.value !== "none";
    if (underlined !== initial.underline) patch.underline = underlined ? true : null;
    if (current.touched.has("color")) patch.color = current.colors.color || null;
    if (current.touched.has("ucolor")) patch.ucolor = current.colors.ucolor || null;
    const fxMask = FX.maskOf(current.touched);
    if (fxMask) patch.textfx = { mask: fxMask, value: composeFx(els, 0, current.touched) };
    const advanced = advancedPatch(els, initial, current.touched);
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
