(function (host) {
  "use strict";

  // What a document carries beside its blocks in the structured save payload (moved out of `save-payload.js`): the
  // defaults of an opened file (PD-363), the footnotes, endnotes and comments (PD-252), and the Word-style document
  // settings of PD-395 (properties, line numbers, watermark). They ride on the editor root as data: `eoDefaults` and
  // `eoTrackChanges` since PD-363, `eoWordMeta` (one JSON string) since PD-395.

  const MAX_ANNOTATION_ITEMS = 2048;

  // The document defaults of an opened DOCX, ODT or RTF (PD-363) ride on the editor root as one JSON string (`eoDefaults`)
  // from open to save. Both ends keep only the known keys: integers, and names of at most 256 characters.
  const DEFAULT_INTEGERS = ["sizeHalfPoints", "spaceBeforeTwips", "spaceAfterTwips", "line240ths", "tabStopTwips"];
  function packDocDefaults(raw) {
    if (!raw || typeof raw !== "object" || !DEFAULT_INTEGERS.every((k) => Number.isSafeInteger(raw[k]))) return "";
    const out = Object.fromEntries(DEFAULT_INTEGERS.map((k) => [k, raw[k]]));
    for (const k of ["font", "eastAsianFont", "alignment"]) if (typeof raw[k] === "string" && raw[k].length <= 256) out[k] = raw[k];
    return JSON.stringify(out);
  }
  function unpackDocDefaults(text) {
    try {
      const packed = packDocDefaults(JSON.parse(text));
      return packed ? JSON.parse(packed) : null;
    } catch {
      return null;
    }
  }

  // ---- the Word-style document settings (PD-395)

  const PROPERTY_KEYS = ["title", "author", "subject", "keywords", "comments"];
  const MAX_PROPERTY_CHARS = 1024;
  const MAX_WATERMARK_CHARS = 64;
  const MAX_LINE_NUMBER_VALUE = 31680;
  const LINE_RESTARTS = ["newPage", "newSection", "continuous"];
  const noControls = (text, newline) => Array.from(String(text ?? "")).filter((c) => (c >= " " && c !== "\u007f") || (newline && c === "\n")).join("");

  // The settings of the editor root (`{ props, lineNumbers, watermark }`) as the editor keeps them, cleaned and bounded; the
  // engine checks them again.
  function readWordMeta(text) {
    let raw;
    try {
      raw = text ? JSON.parse(text) : null;
    } catch {
      raw = null;
    }
    const meta = {};
    if (!raw || typeof raw !== "object") return meta;
    const props = {};
    for (const key of PROPERTY_KEYS) {
      const value = noControls(raw.props?.[key], key === "comments").slice(0, MAX_PROPERTY_CHARS);
      if (value) props[key] = value;
    }
    if (Object.keys(props).length > 0) meta.props = props;
    const ln = raw.lineNumbers;
    const inRange = (v, min) => Number.isInteger(v) && v >= min && v <= MAX_LINE_NUMBER_VALUE;
    if (ln && inRange(ln.start, 0) && inRange(ln.countBy, 1) && inRange(ln.distance, 0) && LINE_RESTARTS.includes(ln.restart)) {
      meta.lineNumbers = { start: ln.start, countBy: ln.countBy, distance: ln.distance, restart: ln.restart };
    }
    const watermark = noControls(raw.watermark, false).trim().slice(0, MAX_WATERMARK_CHARS);
    if (watermark) meta.watermark = watermark;
    return meta;
  }

  // The payload keys of the settings: `properties`, `lineNumbers` and `watermark`.
  function wordMetaPayload(text) {
    const meta = readWordMeta(text);
    const out = {};
    if (meta.props) out.properties = meta.props;
    if (meta.lineNumbers) out.lineNumbers = meta.lineNumbers;
    if (meta.watermark) out.watermark = meta.watermark;
    return out;
  }

  // The settings the editor root keeps, as the one string `eoWordMeta` holds (empty when there is none).
  function packWordMeta(meta) {
    const packed = readWordMeta(JSON.stringify(meta ?? {}));
    return Object.keys(packed).length > 0 ? JSON.stringify(packed) : "";
  }

  // The settings of an opened file: the open answer's `properties`, `lineNumbers` and `watermark`.
  function wordMetaOfOpen(res) {
    return packWordMeta({ props: res?.properties, lineNumbers: res?.lineNumbers, watermark: res?.watermark });
  }

  function collectDocAnnotations(root) {
    const out = {};
    if (!root) return out;
    if (root.dataset?.eoTrackChanges === "true") out.trackChanges = true;
    const defaults = unpackDocDefaults(root.dataset?.eoDefaults);
    if (defaults) out.docDefaults = defaults;
    Object.assign(out, wordMetaPayload(root.dataset?.eoWordMeta));
    const refKeys = new Set();
    const commentIds = new Set();
    const scanBody = (node) => {
      for (const child of node.childNodes || []) {
        if (child.nodeType !== 1) continue;
        const tag = (child.tagName || "").toUpperCase();
        const ds = child.dataset;
        if (tag === "FOOTER" || child.id === "hwp-manuscript-preview" || (ds && (ds.eoNotes !== undefined || ds.eoComments !== undefined || ds.eoHf !== undefined))) continue;
        if (tag === "SUP" && (ds?.eoNoteKind === "footnote" || ds?.eoNoteKind === "endnote")) {
          const id = Number(ds.eoNoteId);
          if (Number.isInteger(id) && id >= 1) refKeys.add(ds.eoNoteKind + ":" + id);
          continue;
        }
        if (ds?.eoCommentId !== undefined) {
          const cid = Number(ds.eoCommentId);
          if (Number.isInteger(cid) && cid >= 1) commentIds.add(cid);
        }
        scanBody(child);
      }
    };
    scanBody(root);
    const notes = [];
    for (const kind of ["footnote", "endnote"]) {
      for (const sec of Array.from(root.children || [])) {
        if (sec.dataset?.eoNotes !== kind) continue;
        for (const item of Array.from(sec.children || [])) {
          if (notes.length >= MAX_ANNOTATION_ITEMS) break;
          if (!item.classList?.contains("eo-doc-note-item")) continue;
          const id = Number(item.dataset?.eoNoteId);
          if (!Number.isInteger(id) || id < 1 || !refKeys.has(kind + ":" + id)) continue;
          const bodies = item.querySelectorAll ? Array.from(item.querySelectorAll(".eo-doc-note-body")) : [];
          const text = (bodies.length > 0 ? bodies.map((b) => String(b.textContent || "")).join("\n") : String(item.textContent || "")).slice(0, 65536);
          notes.push({ id, kind, text });
        }
      }
    }
    if (notes.length > 0) out.notes = notes;
    const comments = [];
    for (const sec of Array.from(root.children || [])) {
      if (sec.dataset?.eoComments === undefined) continue;
      for (const art of Array.from(sec.children || [])) {
        if (comments.length >= MAX_ANNOTATION_ITEMS) break;
        const id = Number(art.dataset?.eoCommentId);
        if (!Number.isInteger(id) || id < 1 || !commentIds.has(id)) continue;
        comments.push({
          id,
          author: String(art.dataset?.eoAuthor || "작성자").slice(0, 128),
          date: String(art.dataset?.eoDate || "").slice(0, 64),
          text: String(art.textContent || "").slice(0, 65536),
        });
      }
    }
    if (comments.length > 0) out.comments = comments;
    return out;
  }

  host.__EASYOFFICE_SAVE_DOC_META__ = {
    PROPERTY_KEYS, LINE_RESTARTS, packDocDefaults, unpackDocDefaults, collectDocAnnotations, readWordMeta, packWordMeta, wordMetaPayload, wordMetaOfOpen,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
