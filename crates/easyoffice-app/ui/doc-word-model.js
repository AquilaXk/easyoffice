(function (host) {
  "use strict";

  // How the Word-style features of PD-395 are kept in the editor, and the two directions between that and the engine's
  // document JSON (`convert_structured_word.rs`):
  //  - a field (date, time, cross reference, caption number) is an inline `span.eo-field-code[data-eo-field-code]` that the
  //    caret cannot enter; its text is the result the file shows;
  //  - a bookmark is a pair of empty inline markers `span[data-eo-bookmark-start]` and `span[data-eo-bookmark-end]`
  //    (the editor shows them as brackets when the format marks are on), so a bookmark may cross blocks and overlap another;
  //  - a caption paragraph has `data-eo-caption`, a paragraph that ends a section has `data-eo-section-break`;
  //  - a table's `data-eo-header-rows` is the number of rows from the top that repeat on every page.
  // Open side: `runSpecs` (wraps the runs that are fields or bookmarks), `paragraph`, `table`. Save side: `carry`,
  // `sameMarks`, `applyMarks`, `bookmarkRun`, `paragraphFlags`, `tableFlags`.

  const FIELD_CLASS = "eo-field-code";
  const SECTION_BREAKS = ["nextPage", "continuous", "evenPage", "oddPage"];
  const NAME_RE = /^[\p{L}_][\p{L}\p{N}_]{0,39}$/u;
  const MAX_FIELD_CODE = 256;

  const el = (tag, attrs, data, children) => ({ t: "el", tag, attrs: attrs || {}, data: data || {}, style: {}, children: children || [] });

  const isBookmarkName = (name) => NAME_RE.test(String(name ?? "")) && !String(name).startsWith("_Toc") && name !== "_GoBack";
  const isFieldCode = (code) => typeof code === "string" && code.length > 0 && code.length <= MAX_FIELD_CODE && /^(DATE|TIME|REF|SEQ)\b/i.test(code.trim());

  // ---- open: the decoded document to editor specs

  const fieldSpec = (code, children) => el("span", { class: FIELD_CLASS, contenteditable: "false" }, { eoFieldCode: code }, children);
  const markerSpec = (kind, name) => el("span", { class: "eo-bookmark-" + kind, contenteditable: "false" }, kind === "start" ? { eoBookmarkStart: name } : { eoBookmarkEnd: name });

  // The specs of `runs`: the runs that are fields or open or close a bookmark are made one at a time and wrapped; the others go
  // to `base` together, as before (revision and comment wrappers span runs).
  function runSpecs(runs, noteSeq, base) {
    const special = (run) => isFieldCode(run?.fieldCode) || isBookmarkName(run?.bookmarkStart) || isBookmarkName(run?.bookmarkEnd);
    if (!Array.isArray(runs) || !runs.some(special)) return base(runs, noteSeq);
    const out = [];
    let plain = [];
    const flush = () => {
      if (plain.length > 0) out.push(...base(plain, noteSeq));
      plain = [];
    };
    for (const run of runs) {
      if (!special(run)) {
        plain.push(run);
        continue;
      }
      flush();
      if (isBookmarkName(run.bookmarkStart)) out.push(markerSpec("start", run.bookmarkStart));
      const pieces = base([{ ...run, fieldCode: undefined, bookmarkStart: undefined, bookmarkEnd: undefined }], noteSeq);
      out.push(...(isFieldCode(run.fieldCode) ? [fieldSpec(run.fieldCode.trim(), pieces)] : pieces));
      if (isBookmarkName(run.bookmarkEnd)) out.push(markerSpec("end", run.bookmarkEnd));
    }
    flush();
    return out;
  }

  // The caption and the section break of a decoded paragraph.
  function paragraph(spec, block) {
    if (block.caption === true) {
      spec.attrs.class = [spec.attrs.class, "eo-caption"].filter(Boolean).join(" ");
      spec.data.eoCaption = "1";
    }
    if (SECTION_BREAKS.includes(block.sectionBreak)) spec.data.eoSectionBreak = block.sectionBreak;
    return spec;
  }

  const table = (spec, block) => {
    if (Number.isInteger(block.headerRows) && block.headerRows > 0) spec.data.eoHeaderRows = String(block.headerRows);
    return spec;
  };

  // ---- save: the editor to the payload

  // The field and the bookmark an element sets on the runs inside it (the innermost wins).
  function carry(node, outer, next) {
    const code = node.dataset?.eoFieldCode;
    next.fieldCode = typeof code === "string" && isFieldCode(code) ? code.trim() : outer.fieldCode;
  }

  const sameMarks = (a, b) => a.fieldCode === b.fieldCode;

  function applyMarks(run, fmt) {
    if (fmt.fieldCode) run.fieldCode = fmt.fieldCode;
  }

  // The empty run a bookmark marker stands for: `{ text: "", bookmarkStart }` or `{ text: "", bookmarkEnd }`.
  function bookmarkRun(node) {
    const start = node.dataset?.eoBookmarkStart;
    const end = node.dataset?.eoBookmarkEnd;
    if (isBookmarkName(start)) return { text: "", bookmarkStart: start, _keepEmpty: true };
    if (isBookmarkName(end)) return { text: "", bookmarkEnd: end, _keepEmpty: true };
    return null;
  }

  // `caption` and `sectionBreak` of a paragraph element.
  function paragraphFlags(node) {
    const kind = node.dataset?.eoSectionBreak;
    return { caption: node.dataset?.eoCaption === "1", sectionBreak: SECTION_BREAKS.includes(kind) ? kind : undefined };
  }

  const tableFlags = (node) => {
    const rows = Number(node.dataset?.eoHeaderRows);
    return Number.isInteger(rows) && rows > 0 ? { headerRows: rows } : {};
  };

  host.__EASYOFFICE_DOC_WORD_MODEL__ = {
    FIELD_CLASS, SECTION_BREAKS, isBookmarkName, isFieldCode, runSpecs, paragraph, table, carry, sameMarks, applyMarks, bookmarkRun, paragraphFlags, tableFlags,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
