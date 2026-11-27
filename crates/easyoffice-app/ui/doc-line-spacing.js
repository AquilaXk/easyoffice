(function (host) {
  "use strict";

  // Line spacing kinds of a paragraph (PD-365): a multiple (줄, Word 배수) plus 최소 (at least), 고정 (exact)
  // and 줄 사이 (between lines). A multiple stays a unitless `line-height` with its percent in
  // `data-eo-line-spacing`. The other kinds keep `data-eo-line-kind` and `data-eo-line-twips` and show as
  // `line-height`: exact is that length, at least is the larger of one line and the length, between lines
  // adds the length to one line. The engine JSON carries the same pair as `lineSpacingKind`/`lineSpacingTwips`.

  const KIND_LABELS = { multiple: "줄", atLeast: "최소", exact: "고정", between: "줄 사이" };
  const LENGTH_KINDS = ["atLeast", "exact", "between"];
  const MAX_TWIPS = 31680;
  const TWIPS_PER_PX = 15;
  const TWIPS_PER_PT = 20;
  // Value of the last entry of the line spacing list, which opens the paragraph dialog instead of choosing a value.
  const OPTIONS_VALUE = "options";

  const validTwips = (kind, twips) =>
    LENGTH_KINDS.includes(kind) && Number.isInteger(twips) && twips >= (kind === "between" ? 0 : 1) && twips <= MAX_TWIPS;

  const px = (twips) => String(Math.round((twips / TWIPS_PER_PX) * 1000) / 1000) + "px";

  function cssOf(kind, twips) {
    if (kind === "exact") return px(twips);
    return kind === "atLeast" ? "max(1em, " + px(twips) + ")" : "calc(1em + " + px(twips) + ")";
  }

  // `{ kind, twips }` of a paragraph element that has a length kind, else null.
  function specialOf(el) {
    const kind = el?.dataset?.eoLineKind;
    const twips = Number(el?.dataset?.eoLineTwips);
    return validTwips(kind, twips) ? { kind, twips } : null;
  }

  // Gives the element a length kind; true when something changed.
  function writeSpecial(el, kind, twips) {
    if (!validTwips(kind, twips)) return false;
    const css = cssOf(kind, twips);
    const same = el.style.lineHeight === css && el.dataset.eoLineKind === kind && el.dataset.eoLineTwips === String(twips);
    el.style.lineHeight = css;
    el.dataset.eoLineKind = kind;
    el.dataset.eoLineTwips = String(twips);
    delete el.dataset.eoLineSpacing;
    return !same;
  }

  function clearSpecial(el) {
    delete el.dataset.eoLineKind;
    delete el.dataset.eoLineTwips;
  }

  // The engine JSON keys of a length kind (`specialOf`), for the save payload.
  const jsonFields = (special) => (special ? { lineSpacingKind: special.kind, lineSpacingTwips: special.twips } : {});

  // `{ kind, twips }` of an engine block or cell paragraph, or null for a multiple or an unknown kind.
  function fromJson(block) {
    const twips = Number(block?.lineSpacingTwips);
    return validTwips(block?.lineSpacingKind, twips) ? { kind: block.lineSpacingKind, twips } : null;
  }

  // The same, as the style and data a freshly decoded element gets.
  function applyToSpec(spec, special) {
    spec.style.lineHeight = cssOf(special.kind, special.twips);
    spec.data.eoLineKind = special.kind;
    spec.data.eoLineTwips = String(special.twips);
  }

  const pt = (twips) => String(Math.round((twips / TWIPS_PER_PT) * 100) / 100);

  // The unit the amount of a kind is typed in: lines for a multiple, points for the rest.
  function amountOf(kind, value) {
    return kind === "multiple" ? String(Math.round(value) / 100) : pt(value);
  }

  // The amount typed for a kind, as the percent (multiple) or twips (lengths) the model keeps; null when out of range.
  function valueOf(kind, text) {
    const n = Number(String(text).trim().replace(/\s*pt$/i, "").replace(",", "."));
    if (!Number.isFinite(n)) return null;
    if (kind === "multiple") {
      const percent = Math.round(n * 100);
      return percent >= 1 && percent <= 1000 ? percent : null;
    }
    const twips = Math.round(n * TWIPS_PER_PT);
    return validTwips(kind, twips) ? twips : null;
  }

  // The line spacing list of the style bar shows a multiple (`percent`) or a length kind (`special`). One custom
  // entry holds the value the list lacks; the last entry, 줄 간격 옵션…, stays last.
  function showInSelect(select, percent, special) {
    const str = special ? special.kind + ":" + special.twips : String(percent);
    Array.from(select.querySelectorAll("option")).filter((o) => o.dataset.custom && o.value !== str).forEach((o) => o.remove());
    const options = Array.from(select.querySelectorAll("option"));
    if (!options.some((o) => o.value === str)) {
      const opt = (select.ownerDocument || host.document).createElement("option");
      opt.value = str;
      opt.dataset.custom = "true";
      opt.textContent = special ? KIND_LABELS[special.kind] + " " + pt(special.twips) + " pt" : percent + " %";
      const more = options.find((o) => o.value === OPTIONS_VALUE) || null;
      select.insertBefore(opt, (special ? null : options.find((o) => Number(o.value) > percent)) || more);
    }
    select.value = str;
  }

  // ---- 단락 dialog (Word, 서식 > 단락: 줄 간격(N) and 값(A)) ------------------------------------------------

  const NAMED_MULTIPLES = { single: 100, oneHalf: 150, double: 200 };
  let dialogReady = false;

  function paragraphDialog(document, runs) {
    const byId = (id) => document.getElementById(id);
    const kind = byId("doc-paragraph-linekind");
    const value = byId("doc-paragraph-linevalue");
    const status = byId("doc-paragraph-status");
    const dialog = byId("doc-paragraph-dialog");
    // 한 줄, 1.5줄 and 2줄 are fixed multiples and take no value.
    const syncValue = () => {
      const named = kind.value in NAMED_MULTIPLES;
      value.disabled = named;
      if (named) value.value = "";
      else if (!value.value) value.value = kind.value === "multiple" ? "1" : "12";
    };
    const apply = () => {
      const named = NAMED_MULTIPLES[kind.value];
      const amount = named ?? valueOf(kind.value, value.value);
      if (amount === null) {
        status.textContent = "값이 범위를 벗어났습니다";
        return;
      }
      runs.setLineSpacingRule(named ? "multiple" : kind.value, amount);
      dialog.close();
    };
    if (!dialogReady) {
      dialogReady = true;
      kind.addEventListener("change", syncValue);
      byId("btn-doc-paragraph-apply").addEventListener("click", apply);
      value.addEventListener("keydown", (ev) => {
        if (ev.key !== "Enter") return;
        ev.preventDefault?.();
        apply();
      });
    }
    return {
      open(current) {
        const named = current.kind === "multiple" ? Object.keys(NAMED_MULTIPLES).find((k) => NAMED_MULTIPLES[k] === current.value) : null;
        kind.value = named || current.kind;
        value.value = named ? "" : amountOf(current.kind, current.value);
        status.textContent = "";
        syncValue();
        dialog.showModal();
        (named ? kind : value).focus();
      },
    };
  }

  // A choice in the line spacing list of the style bar: a multiple, a length kind (`kind:twips`), or 줄 간격 옵션….
  function chooseFromSelect(select, runs, document) {
    const choice = select.value;
    if (choice === OPTIONS_VALUE) {
      const root = document.getElementById("doc-editor");
      const range = runs.currentRange?.();
      const block = root && range ? host.__EASYOFFICE_DOC_RUNS__.blocksInRange(root, range)[0] : null;
      const current = runs.lineSpecOf(block);
      runs.showLineSpacing(current.kind === "multiple" ? current.value : 0, current.kind === "multiple" ? undefined : { kind: current.kind, twips: current.value });
      paragraphDialog(document, runs).open(current);
      return;
    }
    const [kind, twips] = choice.split(":");
    if (twips === undefined) runs.setLineSpacing(Number.parseInt(choice, 10));
    else runs.setLineSpacingRule(kind, Number(twips));
  }

  host.__EASYOFFICE_DOC_LINE_SPACING__ = {
    KIND_LABELS, LENGTH_KINDS, MAX_TWIPS, OPTIONS_VALUE, cssOf, specialOf, writeSpecial, clearSpecial, jsonFields, fromJson, applyToSpec, amountOf, valueOf, showInSelect, chooseFromSelect,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
