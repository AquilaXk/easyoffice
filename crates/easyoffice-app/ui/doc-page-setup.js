(function (host) {
  "use strict";

  // Page setup of the document window (PD-213): paper size, orientation and margins. The dialog is the reference
  // editor's edit-paper dialog (`rhwp-studio/src/ui/page-setup-dialog.ts`, MIT, Copyright (c) 2025-2026 Edward Kim,
  // see THIRD_PARTY_NOTICES.md): the paper list, the width and length in mm, the orientation cards, and the margins
  // top, bottom, left, right, header and footer, with the same refusal of a setup that leaves no body
  // (`core/page-body-limits.ts`: 10 mm). Differences: no binding or scope (the engine's page model has neither), the
  // refusal goes to the status line, and the dialog is a `<dialog>` of this shell.
  //
  // The setup lives on the editor as `data-eo-page` ("width,height,left,right,top,bottom,header,footer" in twips, the
  // engine's `RichPaper` and `RichPageMargins`: top and bottom are the distances from the page edge to the body,
  // header and footer those to the header and footer areas) and as the editor's padding. The page layout
  // (`doc-pages.js`) and the header and footer (`doc-page-frame.js`) follow it, the workspace snapshot keeps it (one
  // undo step) and the save payload carries it as `pageSetup`.

  const TWIPS_PER_MM = 1440 / 25.4;
  const PX_PER_TWIP = 1 / 15;
  const HWPUNIT_PER_TWIP = 5;
  // Largest paper side or margin the engine keeps (`MAX_PAGE_MARGIN_TWIPS`, 22 inches) and the least body it leaves
  // (`MIN_PAGE_BODY_TWIPS`, 10 mm).
  const MAX_TWIPS = 31680;
  const MIN_BODY_TWIPS = 567;
  const MIN_BODY_MM = 10;
  // The reference editor's paper list in HWPUNIT (`page-setup-dialog.ts` 38-45) and its match tolerance (line 12).
  const PRESETS = [["A4", 59528, 84188], ["A3", 84188, 119055], ["B4", 72850, 103040], ["B5", 51502, 72850], ["Letter", 62208, 80496], ["Legal", 62208, 102816]];
  const PRESET_TOLERANCE_HU = 3;
  const CUSTOM = "custom";
  // The two papers the page layout knew before a setup existed (`doc-pages-core.js` `PAPER_PX`), in twips.
  const PAPER_TWIPS = { A4: [11906, 16838], Letter: [12240, 15840] };
  const KEYS = ["width", "height", "left", "right", "top", "bottom", "header", "footer"];
  // The binding (제본) follows the eight values when it is not the default: its gutter in twips and its kind (0 한쪽, 1 맞쪽,
  // 2 위로, the engine's `RichBindingKind`). A page bound at the top takes its gutter from the height, the others from the width.
  const BINDING_KEYS = ["gutter", "binding"];
  const BINDING_KINDS = ["single", "duplex", "top"];
  const TOP_BINDING = 2;
  // The new-document margins of a page without a setup: the editor's own padding (48px 56px).
  const DEFAULT_SIDE = { top: 720, bottom: 720, left: 840, right: 840, header: 720, footer: 720 };
  // The margin sets 여백 전환 goes through, in order: the reference editor's new-document margins, then narrow and wide ones
  // (twips, the engine's form; the first is `RichPageMargins::HWP_DEFAULT`).
  const MARGIN_KEYS = ["top", "bottom", "header", "footer", "left", "right"];
  const MARGIN_PRESETS = [
    { top: 1984, bottom: 1700, header: 1134, footer: 850, left: 1701, right: 1701 },
    { top: 1134, bottom: 1134, header: 567, footer: 567, left: 851, right: 851 },
    { top: 2551, bottom: 2268, header: 1417, footer: 1134, left: 2268, right: 2268 },
  ];
  const MSG_NO_EDITOR = "열린 문서가 없습니다";
  const MSG_TOO_BIG = "용지 크기와 여백은 " + Math.floor((MAX_TWIPS / TWIPS_PER_MM) * 10) / 10 + "mm 이하여야 합니다.";
  const MSG_PAGED_ONLY = "쪽 보기에서만 설정";

  const round1 = (v) => Math.round(v * 10) / 10;
  const twipsToMm = (twips) => round1(twips / TWIPS_PER_MM);
  const mmToTwips = (mm) => Math.round(mm * TWIPS_PER_MM);
  const presetTwips = ([, w, h]) => [Math.round(w / HWPUNIT_PER_TWIP), Math.round(h / HWPUNIT_PER_TWIP)];

  // The preset a paper of `width` by `height` twips is, in either orientation, or null.
  function matchPreset(width, height) {
    const same = (a, b) => Math.abs(a * HWPUNIT_PER_TWIP - b) <= PRESET_TOLERANCE_HU;
    return PRESETS.find(([, w, h]) => (same(width, w) && same(height, h)) || (same(width, h) && same(height, w))) ?? null;
  }

  // `data-eo-page` as a setup, or null when it is absent or malformed.
  function parse(text) {
    const parts = String(text ?? "").split(",").map(Number);
    if (parts.length !== KEYS.length && parts.length !== KEYS.length + BINDING_KEYS.length) return null;
    if (!parts.every((v, i) => Number.isInteger(v) && v >= 0 && v <= (i === KEYS.length + 1 ? TOP_BINDING : MAX_TWIPS))) return null;
    return Object.fromEntries([...KEYS, ...BINDING_KEYS].map((key, i) => [key, parts[i] ?? 0]));
  }

  const encode = (setup) => {
    const bound = (setup.gutter ?? 0) > 0 || (setup.binding ?? 0) > 0;
    return [...KEYS, ...(bound ? BINDING_KEYS : [])].map((key) => setup[key] ?? 0).join(",");
  };

  // The setup is usable: every value within the engine's bounds, the header and footer distances inside their
  // margins, and a body of at least 10 mm in both directions. Returns the reason or null.
  function violation(setup) {
    if (![...KEYS, "gutter"].every((key) => Number.isInteger(setup[key] ?? 0) && (setup[key] ?? 0) >= 0 && (setup[key] ?? 0) <= MAX_TWIPS)) return MSG_TOO_BIG;
    const mm = (twips) => (twips / TWIPS_PER_MM).toFixed(1);
    const atTop = setup.binding === TOP_BINDING;
    const gutter = setup.gutter ?? 0;
    const width = setup.width - setup.left - setup.right - (atTop ? 0 : gutter);
    const height = setup.height - setup.top - setup.bottom - (atTop ? gutter : 0);
    if (width < MIN_BODY_TWIPS) {
      return "좌우 여백이 너무 커서 본문이 남지 않습니다 (본문 너비 " + mm(width) + "mm, 최소 " + MIN_BODY_MM + "mm). 왼쪽·오른쪽 여백을 줄여주세요.";
    }
    if (height < MIN_BODY_TWIPS) {
      return "위아래 여백이 너무 커서 본문이 남지 않습니다 (본문 높이 " + mm(height) + "mm, 최소 " + MIN_BODY_MM + "mm). 위쪽·아래쪽·머리말·꼬리말 여백을 줄여주세요.";
    }
    return null;
  }

  // The dialog's fields (mm) as the reference editor shows them: its top margin is the distance to the header area,
  // its header margin the height of that area; the same for the footer. The engine's `top` is the distance to the body.
  function toFields(setup) {
    return {
      width: twipsToMm(setup.width),
      height: twipsToMm(setup.height),
      left: twipsToMm(setup.left),
      right: twipsToMm(setup.right),
      top: twipsToMm(setup.header),
      header: twipsToMm(Math.max(0, setup.top - setup.header)),
      bottom: twipsToMm(Math.max(0, setup.bottom - setup.footer)),
      footer: twipsToMm(setup.footer),
      gutter: twipsToMm(setup.gutter ?? 0),
    };
  }

  // The setup of the dialog's fields (numbers in mm). A field the user left as it was shown keeps its exact twips, so
  // pressing 확인 does not round a file's values to a tenth of a millimetre.
  function fromFields(fields, original) {
    const shown = original ? toFields(original) : null;
    const kept = (key) => shown !== null && fields[key] === shown[key];
    const twips = (key, exact) => (kept(key) ? exact : mmToTwips(Number(fields[key]) || 0));
    const header = twips("top", original?.header);
    const footer = twips("footer", original?.footer);
    const headerArea = kept("header") ? Math.max(0, original.top - original.header) : mmToTwips(Number(fields.header) || 0);
    const footerArea = kept("bottom") ? Math.max(0, original.bottom - original.footer) : mmToTwips(Number(fields.bottom) || 0);
    return {
      width: twips("width", original?.width),
      height: twips("height", original?.height),
      left: twips("left", original?.left),
      right: twips("right", original?.right),
      top: header + headerArea,
      bottom: footer + footerArea,
      header,
      footer,
      gutter: twips("gutter", original?.gutter ?? 0),
      binding: Number(fields.binding) || 0,
    };
  }

  // The setup of an open answer's `pageSetup` (`{ paper, margins }`, either may be missing), or null without one.
  function fromDecoded(pageSetup) {
    const paper = pageSetup?.paper;
    const margins = pageSetup?.margins;
    if (!paper && !margins) return null;
    const kind = BINDING_KINDS.indexOf(pageSetup?.binding?.kind);
    const binding = { gutter: Number(pageSetup?.binding?.gutter) || 0, binding: Math.max(0, kind) };
    const setup = { width: 11906, height: 16838, ...DEFAULT_SIDE, ...(paper ?? {}), ...(margins ?? {}), ...binding };
    return violation(setup) === null && setup.header <= setup.top && setup.footer <= setup.bottom ? setup : null;
  }

  const px = (twips) => Math.round(twips * PX_PER_TWIP * 100) / 100;

  // The layout style fields that carry a setup in a workspace snapshot (`captureDocLayoutStyle`).
  function layoutFields(setup) {
    if (!setup) return { eoPage: "", padding: "", maxWidth: "", paperSize: "" };
    return {
      eoPage: encode(setup),
      // The gutter is margin on the bound side (a facing-pages binding is drawn as the odd page's).
      padding: (setup.binding === TOP_BINDING ? [setup.top + (setup.gutter ?? 0), setup.right, setup.bottom, setup.left] : [setup.top, setup.right, setup.bottom, setup.left + (setup.gutter ?? 0)]).map((v) => px(v) + "px").join(" "),
      maxWidth: "",
      paperSize: "",
    };
  }

  // The save payload's `pageSetup` for a `data-eo-page` value, or nothing.
  function payloadOf(text) {
    const setup = parse(text);
    if (!setup || violation(setup) !== null || setup.header > setup.top || setup.footer > setup.bottom) return {};
    const out = {
      pageSetup: {
        paper: { width: setup.width, height: setup.height },
        margins: { top: setup.top, bottom: setup.bottom, header: setup.header, footer: setup.footer, left: setup.left, right: setup.right },
      },
    };
    if (setup.gutter > 0 || setup.binding > 0) out.pageSetup.binding = { kind: BINDING_KINDS[setup.binding], gutter: setup.gutter };
    return out;
  }

  function applyToEditor(editor, setup) {
    const fields = layoutFields(setup);
    editor.dataset.eoPage = fields.eoPage;
    editor.style.padding = fields.padding;
    editor.style.maxWidth = fields.maxWidth;
    delete editor.dataset.paperSize;
  }

  // ---- dialog ---------------------------------------------------------------------------------

  function createDocPageSetup(deps) {
    const { document, withUndo, setFeedback } = deps;
    const env = deps.env || host;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const byId = (id) => document.getElementById(id);
    const FIELD_IDS = { gutter: "doc-ps-gutter", width: "doc-ps-width", height: "doc-ps-height", left: "doc-ps-left", right: "doc-ps-right", top: "doc-ps-top", bottom: "doc-ps-bottom", header: "doc-ps-header", footer: "doc-ps-footer" };
    let original = null;

    // The setup the document has now: its own, else what the page layout shows (the paper, the padding and the
    // header and footer distances the page frame uses).
    function current(editor) {
      const own = parse(editor.dataset.eoPage);
      if (own) return own;
      const core = host.__EASYOFFICE_DOC_PAGES_CORE__;
      const cs = env.getComputedStyle?.(editor);
      const pad = (name, fallback) => {
        const value = Number.parseFloat(cs?.[name]);
        return Number.isFinite(value) ? Math.round(value / PX_PER_TWIP) : fallback;
      };
      const { paper: name, landscape: turned } = core.paperOf(editor);
      const [short, long] = PAPER_TWIPS[name] ?? PAPER_TWIPS.A4;
      const setup = {
        width: turned ? long : short, height: turned ? short : long,
        left: pad("paddingLeft", DEFAULT_SIDE.left), right: pad("paddingRight", DEFAULT_SIDE.right),
        top: pad("paddingTop", DEFAULT_SIDE.top), bottom: pad("paddingBottom", DEFAULT_SIDE.bottom),
        header: 0, footer: 0,
      };
      const frame = deps.getFrameMargins?.() ?? null;
      Object.assign(setup, frame ? { top: frame.top, bottom: frame.bottom, header: frame.header, footer: frame.footer } : { header: setup.top, footer: setup.bottom });
      return setup;
    }

    function readFields() {
      const fields = Object.fromEntries(Object.entries(FIELD_IDS).map(([key, id]) => [key, Number.parseFloat(kit.val(id)) || 0]));
      fields.binding = BINDING_IDS.findIndex((id) => kit.checked(id));
      return fields;
    }

    function writeFields(fields) {
      for (const [key, id] of Object.entries(FIELD_IDS)) kit.setVal(id, Number(fields[key]).toFixed(1));
    }

    const landscape = () => kit.checked("doc-ps-landscape");
    const BINDING_IDS = ["doc-ps-single", "doc-ps-duplex", "doc-ps-top-binding"];

    function markBinding() {
      for (const id of BINDING_IDS) byId(id)?.closest(".icon-radio-card")?.classList.toggle("selected", kit.checked(id));
    }

    function markOrientation() {
      for (const id of ["doc-ps-portrait", "doc-ps-landscape"]) byId(id)?.closest(".icon-radio-card")?.classList.toggle("selected", kit.checked(id));
    }

    // The paper list's value for the width and length shown, or the custom entry.
    function syncPaper() {
      const width = mmToTwips(kit.val(FIELD_IDS.width));
      const height = mmToTwips(kit.val(FIELD_IDS.height));
      const found = matchPreset(width, height);
      kit.setSelect("doc-ps-paper", found ? found[0] : CUSTOM);
      const fixed = Boolean(found);
      byId(FIELD_IDS.width).disabled = fixed;
      byId(FIELD_IDS.height).disabled = fixed;
    }

    function onPaperChange() {
      const preset = PRESETS.find(([name]) => name === kit.val("doc-ps-paper"));
      const fixed = Boolean(preset);
      if (preset) {
        const [w, h] = presetTwips(preset);
        const [width, height] = landscape() ? [h, w] : [w, h];
        kit.setVal(FIELD_IDS.width, twipsToMm(width).toFixed(1));
        kit.setVal(FIELD_IDS.height, twipsToMm(height).toFixed(1));
      }
      byId(FIELD_IDS.width).disabled = fixed;
      byId(FIELD_IDS.height).disabled = fixed;
    }

    // Turning the page swaps its width and length.
    function onOrientChange() {
      const width = kit.val(FIELD_IDS.width);
      kit.setVal(FIELD_IDS.width, kit.val(FIELD_IDS.height));
      kit.setVal(FIELD_IDS.height, width);
      markOrientation();
    }

    function populate(setup) {
      writeFields(toFields(setup));
      kit.setChecked(setup.width > setup.height ? "doc-ps-landscape" : "doc-ps-portrait", true);
      markOrientation();
      kit.setChecked(BINDING_IDS[setup.binding ?? 0], true);
      markBinding();
      syncPaper();
    }

    function open() {
      const editor = byId("doc-editor");
      if (!editor) {
        setFeedback?.(MSG_NO_EDITOR);
        return false;
      }
      if (editor.classList.contains("manuscript-grid")) {
        setFeedback?.(MSG_PAGED_ONLY);
        return false;
      }
      original = current(editor);
      populate(original);
      kit.note("doc-ps-status", "");
      kit.openDialog("doc-page-setup-dialog");
      byId("doc-ps-paper")?.focus?.();
      return true;
    }

    // 확인: a setup that leaves no body keeps the dialog open with the reason; else it is applied as one undo step.
    function apply() {
      const editor = byId("doc-editor");
      if (!editor) return false;
      const setup = fromFields(readFields(), original);
      // A listed paper keeps its exact size, not the one its rounded millimetres give.
      const preset = PRESETS.find(([name]) => name === kit.val("doc-ps-paper"));
      if (preset) {
        const [w, h] = presetTwips(preset);
        const [width, height] = setup.width > setup.height ? [h, w] : [w, h];
        if (twipsToMm(width) === twipsToMm(setup.width) && twipsToMm(height) === twipsToMm(setup.height)) Object.assign(setup, { width, height });
      }
      const refused = violation(setup);
      if (refused) {
        kit.note("doc-ps-status", refused);
        return false;
      }
      kit.closeDialog("doc-page-setup-dialog");
      const run = () => {
        if (encode(setup) === editor.dataset.eoPage) return { changed: false };
        applyToEditor(editor, setup);
        return { changed: true };
      };
      if (withUndo) withUndo(run);
      else run();
      setFeedback?.("편집 용지 적용");
      editor.focus?.();
      return true;
    }

    // Applies `next` (a setup) as the page's, unless it leaves no body, in which case the reason is on the status line.
    function change(next, label) {
      const refused = violation(next);
      if (refused) {
        setFeedback?.(refused);
        return false;
      }
      applyToEditor(byId("doc-editor"), next);
      setFeedback?.(label);
      return true;
    }

    // 용지 방향 전환: the page's width and length swap (the margins stay).
    function turn() {
      const editor = byId("doc-editor");
      if (!editor || editor.classList.contains("manuscript-grid")) return false;
      const now = current(editor);
      return change({ ...now, width: now.height, height: now.width }, "용지 방향 전환");
    }

    // 여백 전환: the next margin set after the one the page has (the second when it has none of them).
    function nextMargins() {
      const editor = byId("doc-editor");
      if (!editor || editor.classList.contains("manuscript-grid")) return false;
      const now = current(editor);
      const at = MARGIN_PRESETS.findIndex((set) => MARGIN_KEYS.every((key) => set[key] === now[key]));
      return change({ ...now, ...MARGIN_PRESETS[at >= 0 ? (at + 1) % MARGIN_PRESETS.length : 1] }, "여백 전환");
    }

    function bind() {
      byId("btn-doc-ps-apply")?.addEventListener("click", apply);
      byId("doc-ps-paper")?.addEventListener("change", onPaperChange);
      for (const id of ["doc-ps-portrait", "doc-ps-landscape"]) byId(id)?.addEventListener("change", onOrientChange);
      for (const id of BINDING_IDS) byId(id)?.addEventListener("change", markBinding);
      // A field cannot go below zero or past the engine's bound (the reference clamps the minimum on `change`).
      const max = round1(MAX_TWIPS / TWIPS_PER_MM);
      for (const id of Object.values(FIELD_IDS)) {
        const input = byId(id);
        input?.setAttribute("max", String(max));
        input?.addEventListener("change", () => {
          const value = Number.parseFloat(input.value);
          if (Number.isFinite(value)) input.value = String(Math.min(max, Math.max(0, value)));
        });
      }
    }

    return { open, apply, bind, current, turn, nextMargins };
  }

  host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__ = { PRESETS, KEYS, MAX_TWIPS, MIN_BODY_TWIPS, twipsToMm, mmToTwips, matchPreset, parse, encode, violation, toFields, fromFields, fromDecoded, layoutFields, payloadOf, applyToEditor };
  host.__EASYOFFICE_CREATE_DOC_PAGE_SETUP__ = createDocPageSetup;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
