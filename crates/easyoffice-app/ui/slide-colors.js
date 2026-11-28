(function (host) {
  "use strict";

  // 디자인 > 색 구성표 of the slide editor (PD-398): the twelve colors of the theme (`a:clrScheme`: dark 1, light 1, dark 2,
  // light 2, accent 1 to 6, hyperlink, followed hyperlink) the deck is saved with. Choosing a scheme sets it on the deck and
  // moves every object, background and gradient stop that uses a color of the old scheme to the same slot of the new one, so the
  // slides follow the scheme as theme-colored objects do in the Korean presentation program. Colors that are in no slot stay.
  // The schemes below are palettes of this editor (the first is the one every saved theme starts with); the file part is written by
  // `pptx_deck_settings.rs`.
  const SLOT_LABELS = ["어두운 색 1", "밝은 색 1", "어두운 색 2", "밝은 색 2", "강조 1", "강조 2", "강조 3", "강조 4", "강조 5", "강조 6", "하이퍼링크", "열어 본 하이퍼링크"];
  const SCHEMES = Object.freeze([
    { name: "EasyOffice", colors: ["000000", "FFFFFF", "1C1F2E", "EEF0F6", "4F5BD5", "7A5AF8", "2E9E6B", "D98E04", "C2410C", "0E7490", "2563EB", "7C3AED"] },
    { name: "Office", colors: ["000000", "FFFFFF", "44546A", "E7E6E6", "4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5", "70AD47", "0563C1", "954F72"] },
    { name: "Office 2007–2010", colors: ["000000", "FFFFFF", "1F497D", "EEECE1", "4F81BD", "C0504D", "9BBB59", "8064A2", "4BACC6", "F79646", "0000FF", "800080"] },
    { name: "회색조", colors: ["000000", "FFFFFF", "333333", "E6E6E6", "595959", "7F7F7F", "A6A6A6", "BFBFBF", "D9D9D9", "404040", "404040", "7F7F7F"] },
    { name: "파랑", colors: ["000000", "FFFFFF", "17375E", "DCE6F1", "1F5C99", "2E86C1", "5DADE2", "85C1E9", "1B4F72", "2874A6", "0563C1", "6C3483"] },
    { name: "녹색", colors: ["000000", "FFFFFF", "1E4D2B", "E8F1E4", "2E7D32", "66A04B", "9CCC65", "C5E1A5", "1B5E20", "558B2F", "0563C1", "6C3483"] },
    { name: "주황", colors: ["000000", "FFFFFF", "5D3A1A", "F8EBDD", "E65100", "F57C00", "FFA726", "FFCC80", "BF360C", "EF6C00", "0563C1", "6C3483"] },
  ]);
  const DIALOG = "slide-colors-dialog";
  const DEFAULT_SCHEME = SCHEMES[0];

  const sameColors = (a, b) => a.length === b.length && a.every((c, i) => c === b[i]);

  // `from` slot colors mapped to the same slot of `to`; a color the old scheme has twice goes to its first slot.
  function slotMap(from, to) {
    const map = new Map();
    from.forEach((c, i) => { if (!map.has(c) && c !== to[i]) map.set(c, to[i]); });
    return map;
  }

  // Moves the colors of one slide through `map`.
  function recolorSlide(slide, map) {
    const swap = (c) => (typeof c === "string" && map.has(c.toUpperCase().replace(/^#/, "")) ? map.get(c.toUpperCase().replace(/^#/, "")) : c);
    for (const el of slide.elements || []) {
      for (const key of ["fill", "stroke", "color"]) if (el[key]) el[key] = swap(el[key]);
      if (el.outline?.color) el.outline = { ...el.outline, color: swap(el.outline.color) };
      if (el.shadow?.color) el.shadow = { ...el.shadow, color: swap(el.shadow.color) };
    }
    const bg = slide.background;
    if (bg?.kind === "solid") slide.background = { ...bg, color: swap(bg.color) };
    else if (bg?.kind === "gradient") slide.background = { ...bg, from: swap(bg.from), to: swap(bg.to) };
  }

  function createSlideColors(deps) {
    const { document, slideDeckState: deck, getActiveSlideIndex, getPanels, pushUndoSnapshot, noteActivity } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const $ = (id) => document.getElementById(id);
    const current = () => deck[0]?.colorScheme || DEFAULT_SCHEME;

    function preview(scheme) {
      const box = $("slide-colors-preview");
      if (!box) return;
      box.replaceChildren(...scheme.colors.map((c, i) => {
        const chip = document.createElement("span");
        chip.className = "dialog-swatch-chip";
        chip.style.setProperty("--dialog-swatch", "#" + c);
        chip.title = SLOT_LABELS[i] + " #" + c;
        return chip;
      }));
    }

    function open() {
      const select = $("slide-colors-scheme");
      const now = current();
      // The last entry is the deck's own scheme when it is none of the built-in ones (a theme from a file).
      const list = [...SCHEMES, { name: "사용자 지정", colors: now.colors }];
      kit.fillOptions("slide-colors-scheme", list.map((s, i) => [String(i), s.name]));
      select.__schemes = list;
      const at = SCHEMES.findIndex((s) => sameColors(s.colors, now.colors));
      kit.setSelect("slide-colors-scheme", String(at < 0 ? SCHEMES.length : at));
      kit.note("slide-colors-status", "");
      preview(list[at < 0 ? SCHEMES.length : at]);
      kit.openDialog(DIALOG);
      $("btn-slide-colors-ok")?.focus?.();
    }

    const chosen = () => $("slide-colors-scheme")?.__schemes?.[Number(kit.val("slide-colors-scheme"))] || DEFAULT_SCHEME;

    function apply() {
      const next = chosen();
      const old = current();
      kit.closeDialog(DIALOG);
      if (sameColors(next.colors, old.colors)) return;
      pushUndoSnapshot(false);
      const map = slotMap(old.colors, next.colors);
      const stored = sameColors(next.colors, DEFAULT_SCHEME.colors) ? undefined : { name: next.name === "사용자 지정" ? old.name || "" : next.name, colors: [...next.colors] };
      for (const s of deck) {
        recolorSlide(s, map);
        s.colorScheme = stored;
      }
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
    }

    function run(act) {
      if (act !== "color-scheme") return false;
      open();
      return true;
    }

    function bind() {
      $("btn-slide-colors-ok")?.addEventListener("click", apply);
      $("slide-colors-scheme")?.addEventListener("change", () => preview(chosen()));
      $(DIALOG)?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => btn.addEventListener("click", () => kit.closeDialog(DIALOG)));
    }

    return { bind, run, closeAll: () => kit.closeDialog(DIALOG) };
  }

  host.__EASYOFFICE_SLIDE_COLORS_MATH__ = { SCHEMES, slotMap, recolorSlide };
  host.__EASYOFFICE_CREATE_SLIDE_COLORS__ = createSlideColors;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
