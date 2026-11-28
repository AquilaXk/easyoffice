(function (host) {
  "use strict";

  // Page setup of the document window (PD-213): paper size, orientation and margins. The dialog is the reference
  // editor's edit-paper dialog (`rhwp-studio/src/ui/page-setup-dialog.ts`, MIT, Copyright (c) 2025-2026 Edward Kim,
  // see THIRD_PARTY_NOTICES.md): the paper list, the width and length in mm, the orientation cards, and the margins
  // top, bottom, left, right, header and footer, with the same refusal of a setup that leaves no body
  // (`core/page-body-limits.ts`: 10 mm). Differences: no scope (the engine's page model has none), the binding is
  // shown the way each product words it (see `syncDialogLabels`), the refusal goes to the status line, and the dialog
  // is a `<dialog>` of this shell.
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
  const BINDING_KINDS = ["single", "duplex", "top", "bookfold"];
  const DUPLEX_BINDING = 1;
  const TOP_BINDING = 2;
  // 책 접기 (DOCX `w:bookFoldPrinting`): the pages are a folded booklet; Word turns the page to landscape for it.
  const BOOKFOLD_BINDING = 3;
  // The new-document margins of a page without a setup (`RichPageMargins::DOC_DEFAULT` / `HWP_DEFAULT`).
  const DEFAULT_SIDE = { top: 1701, bottom: 1440, left: 1440, right: 1440, header: 851, footer: 992 };
  const HWP_DEFAULT_SIDE = { top: 1984, bottom: 1701, left: 1701, right: 1701, header: 1134, footer: 850 };
  // The margin sets 여백 전환 goes through, in order: the new-document margins (`RichPageMargins::DOC_DEFAULT`),
  // then narrow and wide ones (twips, the engine's form).
  const MARGIN_KEYS = ["top", "bottom", "header", "footer", "left", "right"];
  const MARGIN_PRESETS = [
    { top: 1701, bottom: 1440, header: 851, footer: 992, left: 1440, right: 1440 },
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
    if (!parts.every((v, i) => Number.isInteger(v) && v >= 0 && v <= (i === KEYS.length + 1 ? BOOKFOLD_BINDING : MAX_TWIPS))) return null;
    return Object.fromEntries([...KEYS, ...BINDING_KEYS].map((key, i) => [key, parts[i] ?? 0]));
  }

  const encode = (setup) => {
    const bound = (setup.gutter ?? 0) > 0 || (setup.binding ?? 0) > 0;
    return [...KEYS, ...(bound ? BINDING_KEYS : [])].map((key) => setup[key] ?? 0).join(",");
  };

  // The setup is usable: every value within the engine's bounds, the header and footer distances inside their
  // margins, and a body of at least 10 mm in both directions. Returns the reason or null.
  function violation(setup, mod = "DOC", pages = false) {
    if (![...KEYS, "gutter"].every((key) => Number.isInteger(setup[key] ?? 0) && (setup[key] ?? 0) >= 0 && (setup[key] ?? 0) <= MAX_TWIPS)) return MSG_TOO_BIG;
    const hfPair = mod === "HWP" || pages ? "머리말·꼬리말" : "머리글·바닥글";
    const tbPair = pages ? "상단·하단" : "위쪽·아래쪽";
    if ((setup.header ?? 0) > (setup.top ?? 0) || (setup.footer ?? 0) > (setup.bottom ?? 0)) {
      return hfPair + " 여백은 " + tbPair + " 여백보다 클 수 없습니다.";
    }
    const mm = (twips) => (twips / TWIPS_PER_MM).toFixed(1);
    const atTop = setup.binding === TOP_BINDING;
    const gutter = setup.gutter ?? 0;
    const width = setup.width - setup.left - setup.right - (atTop ? 0 : gutter);
    const height = setup.height - setup.top - setup.bottom - (atTop ? gutter : 0);
    if (width < MIN_BODY_TWIPS) {
      return "좌우 여백이 너무 커서 본문이 남지 않습니다 (본문 너비 " + mm(width) + "mm, 최소 " + MIN_BODY_MM + "mm). 왼쪽·오른쪽 여백을 줄여주세요.";
    }
    if (height < MIN_BODY_TWIPS) {
      return "위아래 여백이 너무 커서 본문이 남지 않습니다 (본문 높이 " + mm(height) + "mm, 최소 " + MIN_BODY_MM + "mm). " + tbPair + "·" + hfPair + " 여백을 줄여주세요.";
    }
    return null;
  }

  // The dialog's fields (mm). HWP displays top/bottom as distances to the header/footer bands and header/footer as
  // band heights; DOC displays independent distances from each paper edge. The engine stores body, header, and footer
  // distances from paper edges.
  function toFields(setup, mod = "DOC") {
    const hwp = mod === "HWP";
    return {
      width: twipsToMm(setup.width),
      height: twipsToMm(setup.height),
      left: twipsToMm(setup.left),
      right: twipsToMm(setup.right),
      top: twipsToMm(hwp ? setup.header : setup.top),
      header: twipsToMm(hwp ? Math.max(0, setup.top - setup.header) : setup.header),
      bottom: twipsToMm(hwp ? setup.footer : setup.bottom),
      footer: twipsToMm(hwp ? Math.max(0, setup.bottom - setup.footer) : setup.footer),
      gutter: twipsToMm(setup.gutter ?? 0),
    };
  }

  // The setup of the dialog's fields (numbers in mm). A field the user left as it was shown keeps its exact twips, so
  // pressing 확인 does not round a file's values to a tenth of a millimetre.
  function fromFields(fields, original, mod = "DOC") {
    const shown = original ? toFields(original, mod) : null;
    const kept = (key) => shown !== null && fields[key] === shown[key];
    const twips = (key, exact) => (kept(key) ? exact : mmToTwips(Number(fields[key]) || 0));
    let top;
    let bottom;
    let header;
    let footer;
    if (mod === "HWP") {
      header = twips("top", original?.header);
      footer = twips("bottom", original?.footer);
      const headerArea = kept("header") ? Math.max(0, original.top - original.header) : mmToTwips(Number(fields.header) || 0);
      const footerArea = kept("footer") ? Math.max(0, original.bottom - original.footer) : mmToTwips(Number(fields.footer) || 0);
      top = header + headerArea;
      bottom = footer + footerArea;
    } else {
      top = twips("top", original?.top);
      bottom = twips("bottom", original?.bottom);
      header = twips("header", original?.header);
      footer = twips("footer", original?.footer);
    }
    return {
      width: twips("width", original?.width),
      height: twips("height", original?.height),
      left: twips("left", original?.left),
      right: twips("right", original?.right),
      top,
      bottom,
      header,
      footer,
      gutter: twips("gutter", original?.gutter ?? 0),
      binding: Number(fields.binding) || 0,
    };
  }

  // The setup of an open answer's `pageSetup` (`{ paper, margins }`, either may be missing), or null without one.
  function fromDecoded(pageSetup, mod) {
    const paper = pageSetup?.paper;
    const margins = pageSetup?.margins;
    if (!paper && !margins) return null;
    const kind = BINDING_KINDS.indexOf(pageSetup?.binding?.kind);
    const binding = { gutter: Number(pageSetup?.binding?.gutter) || 0, binding: Math.max(0, kind) };
    const side = mod === "HWP" ? HWP_DEFAULT_SIDE : DEFAULT_SIDE;
    const setup = { width: 11906, height: 16838, ...side, ...(paper ?? {}), ...(margins ?? {}), ...binding };
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

    const isHwp = () => (deps.getActiveModule?.() ?? byId("app")?.dataset?.module) === "HWP";
    const activeMod = () => (isHwp() ? "HWP" : "DOC");
    const defaultSide = () => (isHwp() ? HWP_DEFAULT_SIDE : DEFAULT_SIDE);

    // The setup the document has now: its own, else what the page layout shows (the paper, the padding and the
    // header and footer distances the page frame uses).
    function current(editor) {
      const own = parse(editor.dataset.eoPage);
      if (own) return own;
      const side = defaultSide();
      const core = host.__EASYOFFICE_DOC_PAGES_CORE__;
      const cs = env.getComputedStyle?.(editor);
      const pad = (name, fallback) => {
        const value = Number.parseFloat(cs?.[name]);
        return Number.isFinite(value) ? Math.round(value / PX_PER_TWIP) : fallback;
      };
      const { paper: name, landscape: turned } = core.paperOf(editor);
      const [short, long] = PAPER_TWIPS[name] ?? PAPER_TWIPS.A4;
      const topPx = Number.parseFloat(cs?.paddingTop);
      const defTopPx = isHwp() ? 132.27 : 113.4;
      const isDefaultPad = !Number.isFinite(topPx) || Math.abs(topPx - defTopPx) <= 0.2;
      const setup = {
        width: turned ? long : short, height: turned ? short : long,
        left: isDefaultPad ? side.left : pad("paddingLeft", side.left),
        right: isDefaultPad ? side.right : pad("paddingRight", side.right),
        top: isDefaultPad ? side.top : pad("paddingTop", side.top),
        bottom: isDefaultPad ? side.bottom : pad("paddingBottom", side.bottom),
        header: side.header, footer: side.footer,
      };
      const frame = deps.getFrameMargins?.() ?? null;
      Object.assign(setup, frame ? { top: frame.top, bottom: frame.bottom, header: frame.header, footer: frame.footer } : isDefaultPad ? {} : { header: setup.top, footer: setup.bottom });
      return setup;
    }

    const isPages = () => !isHwp() && byId("doc-editor")?.dataset?.eoDocFlavor === "pages";

    // The binding kind the dialog shows. Each product has its own controls over the engine's one `RichBindingKind`: the
    // reference editor's 한쪽·맞쪽·위로 cards (HWP), 여러 페이지 (기본 · 여백 미러) with 제본용 여백 위치 (왼쪽 · 위쪽) (Word) and
    // the 페이지 마주보기 check box (Pages, which has no gutter and no top binding).
    function bindingNow() {
      if (isHwp()) return BINDING_IDS.findIndex((id) => kit.checked(id));
      if (isPages()) return kit.checked("doc-ps-facing") ? DUPLEX_BINDING : 0;
      const multi = Number(kit.val("doc-ps-multi"));
      return multi === DUPLEX_BINDING || multi === BOOKFOLD_BINDING ? multi : Number(kit.val("doc-ps-gutter-pos")) || 0;
    }

    function readFields() {
      const fields = Object.fromEntries(Object.entries(FIELD_IDS).map(([key, id]) => [key, Number.parseFloat(kit.val(id)) || 0]));
      fields.binding = bindingNow();
      return fields;
    }

    function writeFields(fields) {
      for (const [key, id] of Object.entries(FIELD_IDS)) kit.setVal(id, Number(fields[key]).toFixed(1));
    }

    const landscape = () => kit.checked("doc-ps-landscape");
    const BINDING_IDS = ["doc-ps-single", "doc-ps-duplex", "doc-ps-top-binding"];

    const setTxt = (id, text) => {
      const el = byId(id);
      if (el) el.textContent = text;
    };

    // Facing pages (맞쪽, 여백 미러, 페이지 마주보기) name the side margins 안쪽 and 바깥쪽; Word keeps its position box off then.
    function markBinding() {
      for (const id of BINDING_IDS) byId(id)?.closest(".icon-radio-card")?.classList.toggle("selected", kit.checked(id));
      const facing = !isHwp() && [DUPLEX_BINDING, BOOKFOLD_BINDING].includes(bindingNow());
      setTxt("doc-ps-left-label", facing ? "안쪽" : "왼쪽");
      setTxt("doc-ps-right-label", facing ? "바깥쪽" : "오른쪽");
      const position = byId("doc-ps-gutter-pos");
      if (position) position.disabled = facing;
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
      writeFields(toFields(setup, activeMod()));
      kit.setChecked(setup.width > setup.height ? "doc-ps-landscape" : "doc-ps-portrait", true);
      markOrientation();
      // Pages has no top binding: what the dialog cannot show reads as 한쪽.
      const kind = isPages() && [TOP_BINDING, BOOKFOLD_BINDING].includes(setup.binding) ? 0 : (setup.binding ?? 0);
      kit.setChecked(BINDING_IDS[kind] ?? BINDING_IDS[DUPLEX_BINDING], true);
      kit.setSelect("doc-ps-multi", [DUPLEX_BINDING, BOOKFOLD_BINDING].includes(kind) ? String(kind) : "0");
      kit.setSelect("doc-ps-gutter-pos", kind === TOP_BINDING ? "2" : "0");
      kit.setChecked("doc-ps-facing", kind === DUPLEX_BINDING);
      markBinding();
      syncPaper();
    }

    // The dialog of each product: the items and words of its own page setup (the engine model is shared).
    //  - HWP (편집 용지, as the reference editor): 용지 종류, 폭·길이, 용지 방향, 제본 (한쪽 · 맞쪽 · 위로), 용지 여백, 제본.
    //  - Word (페이지 설정): 용지 크기, 너비·높이, 용지 방향, 여러 페이지 (기본 · 여백 미러), 제본용 여백 위치, 여백, 제본용 여백.
    //  - Pages (페이지 설정): 용지 크기, 용지 방향, 문서 여백 (상단 · 하단 · 왼쪽 · 오른쪽), 페이지 마주보기. No gutter, no top binding.
    function syncDialogLabels() {
      const hwp = isHwp();
      const pages = isPages();
      const show = (id, on) => {
        const el = byId(id);
        if (el) el.hidden = !on;
      };
      setTxt("doc-page-setup-title", hwp ? "편집 용지" : "페이지 설정");
      setTxt("doc-ps-paper-title", hwp ? "용지 종류" : "용지 크기");
      setTxt("doc-ps-width-label", hwp ? "폭" : "너비");
      setTxt("doc-ps-height-label", hwp ? "길이" : "높이");
      setTxt("doc-ps-margin-title", hwp ? "용지 여백" : pages ? "문서 여백" : "여백");
      setTxt("doc-ps-margin-top-label", pages ? "상단" : "위쪽");
      setTxt("doc-ps-margin-bottom-label", pages ? "하단" : "아래쪽");
      setTxt("doc-ps-header-label", hwp || pages ? "머리말" : "머리글");
      setTxt("doc-ps-footer-label", hwp || pages ? "꼬리말" : "바닥글");
      setTxt("doc-ps-gutter-label", hwp ? "제본" : "제본용 여백");
      show("doc-ps-binding-section", !pages);
      show("doc-ps-binding-title", hwp);
      show("doc-ps-binding-cards", hwp);
      show("doc-ps-binding-word", !hwp && !pages);
      for (const id of ["doc-ps-gutter-label", "doc-ps-gutter", "doc-ps-gutter-unit"]) show(id, !pages);
      show("doc-ps-facing-row", pages);
      setTxt("doc-ps-layout-title", hwp || pages ? "머리말·꼬리말" : "머리글 및 바닥글");
      // Word's page setup has the three tabs 여백, 용지 and 레이아웃; the other products show every section at once.
      show("doc-ps-tabs", !hwp && !pages);
      showPane(!hwp && !pages ? "margin" : null);
    }

    // One pane at a time under the tabs (`name` null: no tabs, every pane shows).
    function showPane(name) {
      const dialog = byId("doc-page-setup-dialog");
      for (const tab of dialog?.querySelectorAll("[data-ps-tab]") ?? []) {
        const on = tab.dataset.psTab === name;
        tab.classList.toggle("active", on);
        tab.setAttribute("aria-selected", on ? "true" : "false");
        tab.tabIndex = on ? 0 : -1;
      }
      for (const pane of dialog?.querySelectorAll("[data-ps-pane]") ?? []) pane.hidden = name !== null && pane.dataset.psPane !== name;
    }

    // 여러 페이지 > 책 접기 turns the page to landscape, as the Word program does (two pages of the booklet on one sheet).
    function onMultiChange() {
      if (kit.val("doc-ps-multi") === String(BOOKFOLD_BINDING) && !landscape()) {
        kit.setChecked("doc-ps-landscape", true);
        onOrientChange();
      }
      markBinding();
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
      syncDialogLabels();
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
      const mod = activeMod();
      const pages = mod !== "HWP" && editor.dataset?.eoDocFlavor === "pages";
      const setup = fromFields(readFields(), original, mod);
      // Pages keeps no gutter (`TP.SettingsArchive` holds only the facing-pages flag), so the hidden field cannot carry one.
      if (pages) setup.gutter = 0;
      // A listed paper keeps its exact size, not the one its rounded millimetres give.
      const preset = PRESETS.find(([name]) => name === kit.val("doc-ps-paper"));
      if (preset) {
        const [w, h] = presetTwips(preset);
        const [width, height] = setup.width > setup.height ? [h, w] : [w, h];
        if (twipsToMm(width) === twipsToMm(setup.width) && twipsToMm(height) === twipsToMm(setup.height)) Object.assign(setup, { width, height });
      }
      const refused = violation(setup, mod, pages);
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
      setFeedback?.(mod === "HWP" ? "편집 용지 적용" : "페이지 설정 적용");
      editor.focus?.();
      return true;
    }

    // Applies `next` (a setup) as the page's, unless it leaves no body, in which case the reason is on the status line.
    function change(next, label) {
      const mod = activeMod();
      const ed = byId("doc-editor");
      const refused = violation(next, mod, mod !== "HWP" && ed?.dataset?.eoDocFlavor === "pages");
      if (refused) {
        setFeedback?.(refused);
        return false;
      }
      applyToEditor(ed, next);
      setFeedback?.(label);
      return true;
    }

    // 용지 방향: the page's width and length swap (the margins stay).
    function turn() {
      const editor = byId("doc-editor");
      if (!editor || editor.classList.contains("manuscript-grid")) return false;
      const now = current(editor);
      return change({ ...now, width: now.height, height: now.width }, "용지 방향");
    }

    // 여백: the next margin set after the one the page has (the second when it has none of them).
    function nextMargins() {
      const editor = byId("doc-editor");
      if (!editor || editor.classList.contains("manuscript-grid")) return false;
      const now = current(editor);
      const presets = [defaultSide(), MARGIN_PRESETS[1], MARGIN_PRESETS[2]];
      const at = presets.findIndex((set) => MARGIN_KEYS.every((key) => set[key] === now[key]));
      return change({ ...now, ...presets[at >= 0 ? (at + 1) % presets.length : 1] }, "여백");
    }

    function bind() {
      byId("btn-doc-ps-apply")?.addEventListener("click", apply);
      byId("doc-ps-paper")?.addEventListener("change", onPaperChange);
      for (const id of ["doc-ps-portrait", "doc-ps-landscape"]) byId(id)?.addEventListener("change", onOrientChange);
      for (const id of [...BINDING_IDS, "doc-ps-gutter-pos", "doc-ps-facing"]) byId(id)?.addEventListener("change", markBinding);
      byId("doc-ps-multi")?.addEventListener("change", onMultiChange);
      for (const tab of byId("doc-page-setup-dialog")?.querySelectorAll("[data-ps-tab]") ?? []) tab.addEventListener("click", () => showPane(tab.dataset.psTab));
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

  host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__ = { PRESETS, KEYS, MAX_TWIPS, MIN_BODY_TWIPS, HWP_DEFAULT_SIDE, twipsToMm, mmToTwips, matchPreset, parse, encode, violation, toFields, fromFields, fromDecoded, layoutFields, payloadOf, applyToEditor };
  host.__EASYOFFICE_CREATE_DOC_PAGE_SETUP__ = createDocPageSetup;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
