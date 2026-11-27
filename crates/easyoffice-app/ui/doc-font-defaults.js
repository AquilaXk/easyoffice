(function (host) {
  "use strict";

  // 기본값으로 설정(D) of the 글꼴 dialog (PD-375): the font and size of the dialog become the document's defaults
  // (`RichDocument::defaults`, PD-363), the font and size an unstyled paragraph takes. The official dialog asks whether the
  // change is for this document only or for every document built on the default template; this app keeps defaults per
  // document and has no template, so the prompt offers the first choice only.

  // What a new document has (PD-307): the Korean defaults of the engine's `RichDefaults::korean`.
  const NEW_DOCUMENT_DEFAULTS = {
    font: "맑은 고딕", eastAsianFont: "맑은 고딕", sizeHalfPoints: 20, alignment: "justify",
    spaceBeforeTwips: 0, spaceAfterTwips: 160, line240ths: 259, tabStopTwips: 800,
  };

  // The defaults the editor root carries, or those of a new document.
  function currentDefaults(root) {
    try {
      const packed = host.__EASYOFFICE_SAVE_PAYLOAD__?.packDocDefaults?.(JSON.parse(root.dataset?.eoDefaults));
      if (packed) return JSON.parse(packed);
    } catch {
      // no defaults of its own, or text that is not JSON: a new document's defaults
    }
    return { ...NEW_DOCUMENT_DEFAULTS };
  }

  // Sets the default font and size of the document in `root` (a missing `font` or `sizeHalfPt` keeps the current one): the
  // defaults the save sends, and the font and size an unstyled paragraph draws with.
  function setDefaults(root, { font, eastFont, sizeHalfPt }) {
    const pack = host.__EASYOFFICE_SAVE_PAYLOAD__;
    const next = { ...currentDefaults(root) };
    if (font) Object.assign(next, { font, eastAsianFont: eastFont || font });
    if (Number.isInteger(sizeHalfPt)) next.sizeHalfPoints = sizeHalfPt;
    const packed = pack?.packDocDefaults?.(next);
    if (!packed) return false;
    root.dataset.eoDefaults = packed;
    root.style.fontFamily = [next.font, next.eastAsianFont].filter((f, i, all) => f && all.indexOf(f) === i).map((f) => `"${f}"`).join(", ");
    root.style.fontSize = `${next.sizeHalfPoints / 2}pt`;
    const EventCtor = host.Event;
    if (typeof EventCtor === "function") root.dispatchEvent(new EventCtor("eo-doc-mutated", { bubbles: true }));
    return true;
  }

  let bound = false;
  let onConfirm = null;

  // Asks to make `summary` (the font and size) the document's defaults; `confirm` runs on 확인.
  function ask(document, summary, confirm) {
    const dialog = document.getElementById("doc-font-default-dialog");
    if (!dialog) return;
    document.getElementById("doc-font-default-summary").textContent = summary;
    onConfirm = confirm;
    if (!bound) {
      bound = true;
      document.getElementById("btn-doc-font-default-apply").addEventListener("click", () => {
        dialog.close();
        onConfirm?.();
      });
    }
    dialog.showModal();
  }

  host.__EASYOFFICE_DOC_FONT_DEFAULTS__ = { NEW_DOCUMENT_DEFAULTS, setDefaults, ask };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
