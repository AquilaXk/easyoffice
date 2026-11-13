// Hyperlinks of sheet cells (PD-218): the model the page keeps, the address policy and the moves that
// follow a rename or a row or column edit. A link belongs to the cell entry (`entry.link`), so every move of
// an entry (sort, row and column edits, copy, fill) takes it along; the text a link shows is the cell's own value.
//
// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0, Copyright
// 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9):
//   packages/sheets-hyper-link/src/types/interfaces/i-hyper-link.ts :17-47 (a link: `payload`, `tooltip`, cell),
//   packages/sheets-hyper-link/src/services/parser.service.ts :50-91 (`#`-prefixed targets are jumps inside
//       the workbook, any other target is an address),
//   packages/sheets-hyper-link/src/controllers/set-range.controller.ts :43-139 (a write to the cell or
//       a clear removes its link),
//   packages/sheets-hyper-link-ui/src/controllers/auto-fill.controller.ts :103-136, :145-153 (a fill copies the link in the
//       copy and series modes), copy-paste.controller.ts :157-166 (the special pastes leave links alone).
// Differences: Univer keeps the links in a model apart from the cells and moves them with its reference-range
// service; here the link rides on the cell entry, and only the in-workbook target text is rewritten by the
// engine (`linkTargets` of the row and column edit and the sheet rename answers). A target names its sheet
// (Univer names it by id). See THIRD_PARTY_NOTICES.md.
(function (host) {
  "use strict";

  const MAX_TIP_CHARS = 255;
  const MAX_LOCATION_CHARS = 255;
  const MAX_LINKS_PER_SHEET = 65530;
  // The control characters of a screen tip, except the line break.
  const TIP_CONTROL_RE = /[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/g;

  const quoteSheet = (sheet) => host.__EASYOFFICE_SHEET_UTILS__.quoteFormulaSheetName(sheet);

  // The address as it is kept, or null when the policy refuses it: a plain web or mail address (the document
  // editor's rule, `doc-link.js`), or `#` and an in-workbook location of at most 255 characters.
  function normalizeTarget(raw) {
    const text = String(raw ?? "").trim();
    if (text.startsWith("#")) {
      const location = text.slice(1);
      const bad = location === "" || Array.from(location).length > MAX_LOCATION_CHARS || /[\u0000-\u001f\u007f-\u009f]/.test(location);
      return bad ? null : text;
    }
    return host.__EASYOFFICE_DOC_LINK__?.normalizeUrl(text) ?? null;
  }

  const tipOf = (raw) => Array.from(String(raw ?? "").replace(TIP_CONTROL_RE, "")).slice(0, MAX_TIP_CHARS).join("");

  // A frozen link for `target` and `tip`, or null when the policy refuses the address.
  function make(target, tip) {
    const kept = normalizeTarget(target);
    if (kept === null) return null;
    const link = { target: kept };
    const text = tipOf(tip);
    if (text !== "") link.tip = text;
    return Object.freeze(link);
  }

  const of = (entry) => entry?.link ?? null;
  const isInternal = (target) => String(target ?? "").startsWith("#");
  const locationOf = (target) => String(target ?? "").slice(1);

  // `#Sheet1!B2` or `#'Sheet 2'!A1:B3` for a range on a sheet (1-based corners).
  function internalTarget(sheet, rg) {
    const refs = host.__EASYOFFICE_SHEET_REFS__;
    const corner = (r, c) => refs.colName(c) + r;
    const one = rg.r1 === rg.r2 && rg.c1 === rg.c2;
    return "#" + quoteSheet(sheet) + "!" + (one ? corner(rg.r1, rg.c1) : corner(rg.r1, rg.c1) + ":" + corner(rg.r2, rg.c2));
  }

  // A cell with a hyperlink shows as a link and says where it goes: the screen tip, else the address.
  function paint(td, link) {
    td._link = link || null;
    if (!link) {
      if (td.dataset.link) {
        delete td.dataset.link;
        td.removeAttribute("title");
      }
      return;
    }
    const internal = isInternal(link.target);
    td.dataset.link = internal ? "internal" : "external";
    td.setAttribute("title", link.tip || (internal ? locationOf(link.target) : link.target));
  }

  // The links of a sheet map for a save: `[{ ref, target, tip? }]`, the cells that show something only.
  function payloadOf(map) {
    const out = [];
    for (const ref of Object.keys(map || {})) {
      const entry = map[ref];
      if (!entry?.link || !(entry.value || entry.formula)) continue;
      out.push(entry.link.tip ? { ref, target: entry.link.target, tip: entry.link.tip } : { ref, target: entry.link.target });
    }
    return out;
  }

  // Puts the links of a file on the entries of `map` (a link of a cell with nothing in it is dropped).
  function attach(map, links) {
    let count = 0;
    for (const item of Array.isArray(links) ? links : []) {
      const entry = map[item?.ref];
      const link = entry && (entry.value || entry.formula) ? make(item.target, item.tip) : null;
      if (!link || count >= MAX_LINKS_PER_SHEET) continue;
      entry.link = link;
      count += 1;
    }
  }

  // The in-workbook targets of every sheet, `{ sheet: [target, ...] }`, each once; null when no link jumps inside.
  function internalTargets(sheets) {
    let out = null;
    for (const sheet of Object.keys(sheets || {})) {
      const seen = new Set();
      for (const ref of Object.keys(sheets[sheet] || {})) {
        const target = sheets[sheet][ref]?.link?.target;
        if (target && isInternal(target)) seen.add(target);
      }
      if (seen.size > 0) (out ||= {})[sheet] = [...seen];
    }
    return out;
  }

  // `sheets` with the targets in `changes` (`{ sheet: [[old, new], ...] }`, an answer of the engine) replaced. A sheet
  // or an entry that changes is copied, the others are shared.
  function retarget(sheets, changes) {
    const out = { ...sheets };
    for (const sheet of Object.keys(changes || {})) {
      const map = out[sheet];
      if (!map) continue;
      const to = new Map(changes[sheet]);
      let copy = null;
      for (const ref of Object.keys(map)) {
        const target = map[ref]?.link?.target;
        const next = target === undefined ? undefined : to.get(target);
        if (next === undefined) continue;
        copy ||= { ...map };
        copy[ref] = { ...map[ref], link: make(next, map[ref].link.tip) ?? map[ref].link };
      }
      if (copy) out[sheet] = copy;
    }
    return out;
  }

  // The `linkTargets` field of a request, or an empty object when no link jumps inside the workbook.
  function requestField(sheets) {
    const targets = internalTargets(sheets);
    return targets ? { linkTargets: targets } : {};
  }

  host.__EASYOFFICE_SHEET_LINK__ = {
    MAX_TIP_CHARS,
    MAX_LOCATION_CHARS,
    MAX_LINKS_PER_SHEET,
    normalizeTarget,
    make,
    of,
    isInternal,
    locationOf,
    internalTarget,
    paint,
    payloadOf,
    attach,
    internalTargets,
    retarget,
    requestField,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
