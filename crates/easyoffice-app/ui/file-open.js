(function (host) {
  "use strict";

  // Opens a user's own file (PD-075): file picker (Ctrl+O), drag and drop, and the single entry
  // point `openDocumentFromBytes(fileName, bytes)` that a desktop shell can call as well. The
  // backend decodes the real bytes into the structured save shape (`blocks` / `sheets` / `slides`);
  // this module loads that model into the matching workspace module as one undoable state.
  // Also lets the converter panel convert a chosen source file instead of the editor text.

  // Files that travel through IPC (decoded by the engine, or sent as a converter source). The same number as
  // `MAX_SOURCE_BYTES` in convert_open.rs (scripts/test-file-size-limits.mjs keeps them equal); base64 inflates
  // 4/3, so 10 MiB stays under the 16 MiB HTTP IPC body limit.
  const MAX_FILE_BYTES = 10 * 1024 * 1024;
  const MAX_FILE_LABEL = "10 MiB";
  const MAX_HWP_BYTES = 64 * 1024 * 1024; // HWP bytes go straight to the editor frame, not through the IPC body
  const MAX_HWP_LABEL = "64 MiB";
  const HWP_EXTENSIONS = new Set(["hwp", "hwpx", "hwt", "hwtx"]);
  const GRID_LIMITS = { maxRows: 1048576, maxCols: 16384 }; // the sheet editor addresses the full XLSX grid (A..XFD)
  const MODULE_LABELS = { DOC: "문서", HWP: "한글 문서", SHEET: "스프레드시트", SLIDE: "프레젠테이션", BASE: "데이터베이스", BOARD: "보드", PDF: "PDF 스튜디오" };
  const SUPPORTED_EXTENSIONS = [
    "docx", "doc", "odt", "rtf", "md", "markdown", "txt",
    "hwp", "hwpx", "hwt", "hwtx",
    "xlsx", "xls", "ods", "csv", "tsv",
    "pptx", "ppt", "ppsx", "odp",
    "pdf", "pages",
  ];
  // What the workspace picker offers on top of the converter's: the edit packages of BASE and BOARD (PD-265).
  const PACKAGE_EXTENSIONS = ["eobase", "eoboard"];

  function baseName(raw) {
    return String(raw || "").split(/[\\/]/).pop().trim();
  }

  function extensionOf(name) {
    const s = String(name || "");
    const dot = s.lastIndexOf(".");
    return dot >= 0 && dot < s.length - 1 ? s.slice(dot + 1).toLowerCase() : "";
  }

  // The converter's format selector value for a file name, or "" when it has none.
  function converterFormatOption(name) {
    const ext = extensionOf(name);
    return ext === "markdown" ? "md" : ext;
  }

  const tooLarge = (size) => "파일이 너무 큽니다 (" + (size / 1048576).toFixed(1) + " MiB). ";

  // The converter takes its source through IPC whatever the file type: an HWP file is not routed to the editor.
  function converterSourceProblem(size) {
    return size > MAX_FILE_BYTES ? tooLarge(size) + "변환기 원본은 최대 " + MAX_FILE_LABEL + "까지 선택할 수 있습니다" : "";
  }

  // `hwpRoute`: the file goes to the embedded editor, which takes the larger HWP limit. `"converter"`: the
  // file is the converter's source. Otherwise an HWP file is read as text through IPC like any other, and the
  // message says why the limit is lower.
  function validateFile(name, size, hwpRoute) {
    if (!name) return "파일 이름을 확인할 수 없습니다";
    if (size === 0) return "빈 파일은 열 수 없습니다";
    if (hwpRoute === "converter") return converterSourceProblem(size);
    const hwpFamily = HWP_EXTENSIONS.has(extensionOf(name));
    const hwp = hwpRoute === true && hwpFamily;
    if (size > (hwp ? MAX_HWP_BYTES : MAX_FILE_BYTES)) {
      if (hwp) return tooLarge(size) + "최대 " + MAX_HWP_LABEL + "까지 열 수 있습니다";
      if (hwpFamily) return tooLarge(size) + "한글 편집기를 쓰지 못하는 경우 텍스트로 불러올 수 있는 한도는 " + MAX_FILE_LABEL + "입니다";
      return tooLarge(size) + "최대 " + MAX_FILE_LABEL + "까지 열 수 있습니다";
    }
    return "";
  }

  // ---- model -> editor state -------------------------------------------------------------

  const textSpec = (v) => ({ t: "text", v: String(v) });
  const elSpec = (tag, children, style, attrs) => ({
    t: "el", tag, attrs: attrs || {}, data: {}, style: style || {}, children: children || [],
  });

  const SCRIPT_TAGS = { superscript: "sup", subscript: "sub" };
  const PX_PER_TWIP = 1 / 15; // 1 twip = 1/20 pt, 1 pt = 4/3 px
  const EMU_PER_PX = 9525;
  const IMAGE_FORMATS = new Set(["png", "jpg"]);
  const IMAGE_ID_RE = /^[A-Za-z0-9+/=]+$/;
  const TABLE_BORDER_SIDES = [["top", "borderTop"], ["right", "borderRight"], ["bottom", "borderBottom"], ["left", "borderLeft"]];

  function twipsToPx(twips) {
    return String(Math.round(twips * PX_PER_TWIP * 1000) / 1000) + "px";
  }

  // Paragraph indents / line spacing of a decoded block as inline style (what the indent buttons
  // and ruler write) plus the exact twip values, so saving gives back the same numbers.
  function applyParagraphLayout(spec, block) {
    const left = Number(block.leftIndentTwips);
    const right = Number(block.rightIndentTwips);
    const first = Number(block.firstLineIndentTwips);
    const spacing = Number(block.lineSpacingPercent);
    if (Number.isFinite(left) && left > 0) {
      spec.style.paddingLeft = twipsToPx(left);
      spec.data.eoLeftTwips = String(left);
    }
    if (Number.isFinite(right) && right > 0) {
      spec.style.paddingRight = twipsToPx(right);
      spec.data.eoRightTwips = String(right);
    }
    if (Number.isFinite(first) && first !== 0) {
      spec.style.textIndent = twipsToPx(first);
      spec.data.eoFirstLineTwips = String(first);
    }
    const lengthRule = host.__EASYOFFICE_DOC_LINE_SPACING__?.fromJson(block);
    if (lengthRule) host.__EASYOFFICE_DOC_LINE_SPACING__.applyToSpec(spec, lengthRule);
    else if (Number.isFinite(spacing) && spacing > 0 && spacing !== 100) {
      spec.style.lineHeight = String(spacing / 100);
      spec.data.eoLineSpacing = String(spacing);
    }
    if (block.pageBreakBefore === true) spec.data.eoBreak = "page";
    // A copy of the page frame's text shown after the body: it stays marked so a save does not write it as body text.
    if (block.frameCopy === true) spec.data.eoFrameCopy = "1";
    else if (typeof block.frameCopy === "string" && block.frameCopy) spec.data.eoFrameCopy = "s:" + block.frameCopy;
    const lvl = Number(block.listLevel);
    if (Number.isInteger(lvl) && lvl >= 1 && lvl <= 8) spec.data.eoListLevel = String(lvl);
    if (typeof block.listNumber === "string" && block.listNumber) spec.data.eoListNum = block.listNumber;
    host.__EASYOFFICE_DOC_TABLE_RUNS__?.applyParagraphSpacingSpec?.(spec, block, twipsToPx);
    const cols = block.columns;
    if (cols && Number.isInteger(cols.count) && cols.count >= 2 && cols.count <= 16) {
      const gap = Number.isInteger(cols.gapTwips) && cols.gapTwips >= 0 ? cols.gapTwips : 720;
      spec.data.eoCols = cols.count + "," + (cols.equal === false ? 0 : 1) + "," + gap;
    }
    return host.__EASYOFFICE_DOC_WORD_MODEL__.paragraph(spec, block);
  }

  // A picture block: bytes live in the image store, the node carries only the id, size and alt.
  function imageSpec(block) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const format = String(block.format || "");
    const base64 = String(block.base64 || "");
    if (!utils || !IMAGE_FORMATS.has(format) || !IMAGE_ID_RE.test(base64)) {
      return elSpec("p", [textSpec(String(block.alt || "[그림]"))]);
    }
    const alt = String(block.alt ?? "");
    const widthEmu = Number(block.widthEmu) || 0;
    const heightEmu = Number(block.heightEmu) || 0;
    const style = widthEmu > 0 && heightEmu > 0
      ? {
          width: Math.max(1, Math.round(widthEmu / EMU_PER_PX)) + "px",
          height: Math.max(1, Math.round(heightEmu / EMU_PER_PX)) + "px",
        }
      : {};
    const spec = elSpec("img", [], style, { class: "doc-inline-image", contenteditable: "false", title: alt });
    spec.data = {
      eoImageId: utils.registerDocImage(base64, format),
      eoImageFormat: format,
      eoImageWidthEmu: String(widthEmu),
      eoImageHeightEmu: String(heightEmu),
      eoImageAlt: alt,
      eoImageNaturalWidthPx: String(Number(block.widthPx) || 0),
      eoImageNaturalHeightPx: String(Number(block.heightPx) || 0),
    };
    return spec;
  }

  // The page-number field of a header or footer (`doc-page-frame.js`): drawn by the page, not text.
  const FIELD_MARKS = { page: "page", pages: "pages", filename: "filename" };
  const fieldSpec = (mark) => ({ ...elSpec("span", [], {}, { class: "eo-page-field", contenteditable: "false" }), data: { eoField: mark } });

  // `spec` inside the character formatting of a decoded run.
  function formatSpec(spec, run) {
    const runs = host.__EASYOFFICE_DOC_RUNS__;
    const fmt = runs ? runs.fromModelRun(run) : {};
    if (fmt.size || fmt.spacing || fmt.scale || fmt.position || fmt.kerning || fmt.opentype || fmt.nogrid || fmt.textfx || fmt.ucolor || fmt.font || fmt.eastfont || fmt.face || fmt.color || fmt.highlight) {
      spec = runs.buildRunSpec(fmt, [spec]);
    } else {
      [["strike", "s"], ["underline", "u"], ["italic", "em"], ["bold", "strong"]].forEach(([flag, tag]) => {
        if (run[flag] === true) spec = elSpec(tag, [spec]);
      });
    }
    const scriptTag = SCRIPT_TAGS[run.script];
    return scriptTag ? elSpec(scriptTag, [spec]) : spec;
  }

  function runSpecs(runs, noteSeq) {
    const out = [];
    const wire = host.__EASYOFFICE_DOC_NOTES_WIRE__;
    const pushPiece = wire ? wire.pushWrappedPiece : (target, piece) => target.push(piece);
    (runs || []).forEach((run) => {
      const cidNum = Number(run?.commentId);
      const cid = Number.isInteger(cidNum) && cidNum >= 1 ? String(cidNum) : "";
      const rev = wire ? wire.normalizeDecodedRev(run?.revision) : null;
      if (Object.hasOwn(FIELD_MARKS, run.field)) {
        pushPiece(out, formatSpec(fieldSpec(FIELD_MARKS[run.field]), run), cid, rev);
        return;
      }
      const nr = run?.noteRef;
      if (nr && (nr.kind === "footnote" || nr.kind === "endnote") && Number.isInteger(Number(nr.id)) && Number(nr.id) >= 1) {
        const seq = noteSeq ? ((noteSeq[nr.kind] = (noteSeq[nr.kind] || 0) + 1)) : 1;
        const sup = {
          ...elSpec("sup", [textSpec(seq + ")")], {}, { class: "eo-note-ref", contenteditable: "false" }),
          data: { eoNoteKind: nr.kind, eoNoteId: String(Number(nr.id)) },
        };
        pushPiece(out, sup, cid, rev);
        return;
      }
      const rawText = String(run.text ?? "");
      if (rawText === "" && cid) {
        const pt = {
          ...elSpec("span", [], {}, { class: "eo-comment-range eo-comment-point", contenteditable: "false" }),
          data: { eoCommentId: cid },
        };
        pushPiece(out, pt, "", rev);
        return;
      }
      const lines = rawText.split("\n");
      lines.forEach((line, idx) => {
        if (idx > 0) pushPiece(out, elSpec("br"), cid, rev);
        if (!line) return;
        const styled = formatSpec(textSpec(line), run);
        const href = host.__EASYOFFICE_DOC_LINK__?.normalizeUrl(run.link);
        pushPiece(out, href ? { ...elSpec("a", [styled], {}, { href }), data: { eoLink: "1" } } : styled, cid, rev);
      });
    });
    return out;
  }

  // A safe integer: `(row << 16) | col` wraps at 32 bits, so rows 65536 apart shared a key.
  const cellKey = (row, col) => row * 2 ** 32 + col;

  // Anchors (with their spans) and covered cells of a decoded table's `merges` (PD-130). A region is
  // used only when it is inside the grid, every cell of it exists and no other region has it.
  function mergeLayout(rows, merges) {
    const anchors = new Map();
    const covered = new Set();
    (Array.isArray(merges) ? merges : []).forEach((m) => {
      const { row, col, rowSpan, colSpan } = m && typeof m === "object" ? m : {};
      const ints = [row, col, rowSpan, colSpan];
      if (!ints.every(Number.isInteger) || row < 0 || col < 0 || rowSpan < 1 || colSpan < 1) return;
      if (rowSpan * colSpan < 2 || row + rowSpan > rows.length) return;
      for (let r = row; r < row + rowSpan; r += 1) {
        for (let c = col; c < col + colSpan; c += 1) {
          if (c >= rows[r].length || anchors.has(cellKey(r, c)) || covered.has(cellKey(r, c))) return;
        }
      }
      for (let r = row; r < row + rowSpan; r += 1) {
        for (let c = col; c < col + colSpan; c += 1) covered.add(cellKey(r, c));
      }
      covered.delete(cellKey(row, col));
      anchors.set(cellKey(row, col), { rowSpan, colSpan });
    });
    return { anchors, covered };
  }

  const utf8Encoder = new TextEncoder();
  const utf8Decoder = new TextDecoder();

  function pushCellLines(out, raw, wrapFn) {
    const parts = String(raw ?? "").split("\n");
    parts.forEach((line, idx) => {
      if (idx > 0) out.push(elSpec("br"));
      if (line || parts.length === 1) out.push(wrapFn ? wrapFn(line) : textSpec(line));
    });
  }

  function cellInlineSpecs(text, cellLinks) {
    const out = [];
    if (!cellLinks || cellLinks.length === 0) {
      pushCellLines(out, text);
      return out;
    }
    const bytes = utf8Encoder.encode(text);
    let pos = 0;
    for (const link of cellLinks) {
      const s = Number(link.start);
      const e = Number(link.end);
      const href = host.__EASYOFFICE_DOC_LINK__?.normalizeUrl(link.url);
      if (!href || !Number.isInteger(s) || !Number.isInteger(e) || s < pos || e <= s || e > bytes.length) continue;
      if (pos < s) pushCellLines(out, utf8Decoder.decode(bytes.subarray(pos, s)));
      const piece = utf8Decoder.decode(bytes.subarray(s, e));
      pushCellLines(out, piece, (line) => ({ ...elSpec("a", [textSpec(line)], {}, { href }), data: { eoLink: "1" } }));
      pos = e;
    }
    if (pos < bytes.length || out.length === 0) pushCellLines(out, utf8Decoder.decode(bytes.subarray(pos)));
    return out;
  }

  function applyDecodedCellStyle(tdSpec, styleEntry) {
    const ts = host.__EASYOFFICE_DOC_TABLE_STYLE__;
    if (!ts || !styleEntry || typeof styleEntry !== "object") return;
    const rgb = ts.normalizeHex6(styleEntry.fill);
    if (rgb) {
      tdSpec.data.eoFill = rgb;
      tdSpec.style.backgroundColor = "#" + rgb;
    }
    const b = styleEntry.borders;
    if (b && typeof b === "object") {
      const slots = TABLE_BORDER_SIDES.map(([side]) => ts.normalizeEdgeSlot(b[side]));
      if (slots.some((s) => s !== "-")) {
        tdSpec.data.eoBorders = slots.join(",");
        slots.forEach((slot, idx) => {
          const css = ts.edgeToCss(slot);
          if (css) tdSpec.style[TABLE_BORDER_SIDES[idx][1]] = css;
        });
      }
    }
  }

  // A table block: merged regions as colspan/rowspan on their top-left cell, covered cells not drawn,
  // and each row's original index so a save can tell which rows of a region were deleted.
  function tableSpec(block) {
    const rows = Array.isArray(block.rows) ? block.rows.map((r) => (Array.isArray(r) ? r : [])) : [];
    const { anchors, covered } = mergeLayout(rows, block.merges);
    const linksByCell = new Map();
    (Array.isArray(block.links) ? block.links : []).forEach((l) => {
      if (!l || !Number.isInteger(l.row) || !Number.isInteger(l.col) || l.row < 0 || l.col < 0) return;
      const key = cellKey(l.row, l.col);
      let list = linksByCell.get(key);
      if (!list) {
        list = [];
        linksByCell.set(key, list);
      }
      list.push(l);
    });
    linksByCell.forEach((list) => list.sort((a, b) => Number(a.start) - Number(b.start)));
    const stylesByCell = new Map();
    (Array.isArray(block.cellStyles) ? block.cellStyles : []).forEach((s) => {
      if (s && Number.isInteger(s.row) && Number.isInteger(s.col) && s.row >= 0 && s.col >= 0) {
        stylesByCell.set(cellKey(s.row, s.col), s);
      }
    });
    const imagesByCell = new Map();
    (Array.isArray(block.cellImages) ? block.cellImages : []).forEach((im) => {
      if (!im || !Number.isInteger(im.row) || !Number.isInteger(im.col) || im.row < 0 || im.col < 0) return;
      const key = cellKey(im.row, im.col);
      let list = imagesByCell.get(key);
      if (!list) {
        list = [];
        imagesByCell.set(key, list);
      }
      list.push(im);
    });
    const parasByCell = new Map();
    (Array.isArray(block.cellParagraphs) ? block.cellParagraphs : []).forEach((cp) => {
      if (cp && Number.isInteger(cp.row) && Number.isInteger(cp.col) && cp.row >= 0 && cp.col >= 0 && Array.isArray(cp.paragraphs)) {
        parasByCell.set(cellKey(cp.row, cp.col), cp);
      }
    });
    const docTableRuns = host.__EASYOFFICE_DOC_TABLE_RUNS__;
    const trs = rows.map((row, r) => {
      const cells = [];
      row.forEach((cell, c) => {
        const key = cellKey(r, c);
        if (covered.has(key)) return;
        const span = anchors.get(key);
        const attrs = {};
        if (span?.colSpan > 1) attrs.colspan = String(span.colSpan);
        if (span?.rowSpan > 1) attrs.rowspan = String(span.rowSpan);
        const text = typeof cell === "string" ? cell : cell?.text ?? "";
        const cellLinks = linksByCell.get(key);
        const imgs = imagesByCell.get(key);
        const cellEntry = parasByCell.get(key);
        const built = docTableRuns?.buildCellChildrenSpecs
          ? docTableRuns.buildCellChildrenSpecs(text, cellLinks, cellEntry, imgs, { textSpec, elSpec, runSpecs, imageSpec, cellInlineSpecs })
          : (() => {
              const inlineKids = cellInlineSpecs(text, cellLinks);
              const children = imgs?.length
                ? [...(text === "" && (!cellLinks || cellLinks.length === 0) ? [] : inlineKids), ...imgs.map(imageSpec)]
                : inlineKids;
              return { children, cellStyle: {}, cellData: {} };
            })();
        const td = elSpec("td", built.children, built.cellStyle, attrs);
        if (built.cellData) Object.assign(td.data, built.cellData);
        applyDecodedCellStyle(td, stylesByCell.get(key));
        cells.push(td);
      });
      const tr = elSpec("tr", cells);
      tr.data = { eoRow: String(r) };
      return tr;
    });
    if (Array.isArray(block.colWidthsTwips) && block.colWidthsTwips.length > 0) {
      const colSpecs = [];
      let sumPx = 0;
      for (const rawW of block.colWidthsTwips) {
        const twips = Math.round(Number(rawW) || 0);
        if (twips <= 0) continue;
        const px = Math.max(1, Math.round(twips * PX_PER_TWIP));
        const col = elSpec("col", [], { width: px + "px" });
        col.data = { eoColTwips: String(twips) };
        colSpecs.push(col);
        sumPx += px;
      }
      if (colSpecs.length > 0) {
        return elSpec("table", [elSpec("colgroup", colSpecs), elSpec("tbody", trs)], { tableLayout: "fixed", width: sumPx + "px" }, { class: "doc-inline-table" });
      }
    }
    return elSpec("table", [elSpec("tbody", trs)], {}, { class: "doc-inline-table" });
  }

  // One entry or title paragraph of a table-of-contents block; entries are links to their headings (`doc-toc.js`).
  const TOC_TARGET_RE = /^#_Toc([1-9]\d{0,9})$/;

  function tocParagraphSpec(block) {
    const inline = runSpecs(block.runs);
    const target = TOC_TARGET_RE.exec(block.runs?.find((r) => TOC_TARGET_RE.test(r.link))?.link ?? "")?.[1];
    const p = elSpec("p", block.toc === 0 ? inline : [elSpec("a", inline, {}, { href: target ? "#eo-h-" + target : "#" })]);
    p.data.eoToc = String(block.toc);
    if (Number.isInteger(block.tocPage) && block.tocPage > 0) {
      p.data.eoTocPage = String(block.tocPage);
      p.data.eoTocDots = String(block.tocDots === true);
    }
    return p;
  }

  function buildEditorNodes(blocks) {
    const nodes = [];
    const noteSeq = { footnote: 0, endnote: 0 };
    let openList = null;
    let openToc = null;
    (blocks || []).forEach((block) => {
      if (block.type === "paragraph" && Number.isInteger(block.toc)) {
        if (!openToc) {
          openToc = elSpec("nav", [], {}, { class: "eo-doc-toc", "data-doc-toc": "true", contenteditable: "false" });
          nodes.push(openToc);
        }
        openList = null;
        openToc.children.push(tocParagraphSpec(block));
        return;
      }
      openToc = null;
      if (block.type === "table") {
        openList = null;
        nodes.push(host.__EASYOFFICE_DOC_WORD_MODEL__.table(tableSpec(block), block));
        return;
      }
      if (block.type === "image") {
        openList = null;
        nodes.push(imageSpec(block));
        return;
      }
      const style = block.align ? { textAlign: block.align } : {};
      const inline = host.__EASYOFFICE_DOC_WORD_MODEL__.runSpecs(block.runs, noteSeq, runSpecs);
      const heading = Number(block.heading) || 0;
      if (heading >= 1 && heading <= 6) {
        openList = null;
        const h = applyParagraphLayout(elSpec("h" + heading, inline, style), block);
        if (Number.isInteger(block.bookmark) && block.bookmark > 0) h.attrs.id = "eo-h-" + block.bookmark;
        nodes.push(h);
      } else if (block.list === "bullet" || block.list === "ordered" || block.list === "task") {
        const tag = block.list === "ordered" ? "ol" : "ul";
        const isTask = block.list === "task";
        if (openList?.tag !== tag || (openList?.data?.eoList === "task") !== isTask) {
          openList = isTask
            ? { ...elSpec("ul", [], {}, { class: "eo-task-list" }), data: { eoList: "task" } }
            : elSpec(tag);
          nodes.push(openList);
        }
        const li = applyParagraphLayout(elSpec("li", inline, style), block);
        if (isTask) li.data.eoChecked = block.checked === true ? "true" : "false";
        openList.children.push(li);
      } else {
        openList = null;
        nodes.push(applyParagraphLayout(elSpec("p", inline, style), block));
      }
    });
    return nodes;
  }

  // The header and footer sections (including first/even page variants and 3-zone strips) of a decoded document (`doc-page-frame.js`), after the body.
  const PAGE_FRAME_KINDS = ["header", "footer", "firstHeader", "firstFooter", "evenHeader", "evenFooter"];
  const ZONE_NAMES = ["left", "center", "right"];
  function splitDecodedZones(blocks) {
    if (!Array.isArray(blocks) || blocks.length < 2 || blocks.length > 3) return null;
    const zones = [null, null, null];
    let prev = -1;
    for (const b of blocks) {
      if (b?.type !== "paragraph" || b.heading || b.list || b.leftIndentTwips || b.rightIndentTwips || b.firstLineIndentTwips || b.lineSpacingPercent || b.lineSpacingKind || b.toc !== undefined || b.pageBreakBefore) {
        return null;
      }
      const slot = b.align === "center" ? 1 : b.align === "right" ? 2 : (!b.align || b.align === "left") ? 0 : -1;
      if (slot <= prev) return null;
      zones[slot] = b;
      prev = slot;
    }
    return zones;
  }
  function pageFrameNodes(res) {
    return PAGE_FRAME_KINDS
      .filter((kind) => Array.isArray(res[kind]) && res[kind].length > 0)
      .map((kind) => {
        const zones = splitDecodedZones(res[kind]);
        if (!zones) {
          return { ...elSpec("section", buildEditorNodes(res[kind]), {}, { class: "eo-page-frame" }), data: { eoHf: kind } };
        }
        const children = ZONE_NAMES.map((zone, idx) => {
          const block = zones[idx];
          const p = block ? buildEditorNodes([block])[0] : elSpec("p", [elSpec("br")]);
          p.data = { ...(p.data || {}), eoZone: zone };
          p.style = { ...(p.style || {}), textAlign: zone };
          return p;
        });
        return { ...elSpec("section", children, {}, { class: "eo-page-frame" }), data: { eoHf: kind, eoHfZones: "true" } };
      });
  }

  const annotationSectionSpecs = (res) => host.__EASYOFFICE_DOC_NOTES_WIRE__?.annotationSectionSpecs(res) ?? [];

  // `{ [1-based column]: px }` from a decoded sheet's `columns` (PD-089); invalid entries are skipped.
  function columnWidthMap(columns) {
    const widths = {};
    (Array.isArray(columns) ? columns : []).forEach((col) => {
      const index = Number(col?.index);
      const px = Number(col?.widthPx);
      if (Number.isInteger(index) && index >= 1 && Number.isFinite(px) && px > 0) widths[index] = px;
    });
    return widths;
  }

  // Cell entries keep the decoded `style` (bold/italic/underline/strike/size) so a file's formatting
  // shows after opening and is written again by the next save; styled blank cells are kept too.
  // The position-bound parts of a sheet in an open answer: merges, hidden rows and columns, the filter
  // (PD-194, PD-198), the view (PD-208) and the tables (PD-224).
  const LAYOUT_KEYS = ["merges", "hiddenRows", "hiddenCols", "rowHeights", "defaultRowHeight", "defaultColWidth", "defaultColPx", "filter", "freeze", "tabColor", "hidden", "veryHidden", "outline", "page", "protect", "unlockedRows", "unlockedCols", "tables", "charts", "pivots", "images", "notes"];

  function buildSheetStore(sheets) {
    const codec = host.__EASYOFFICE_SHEET_STYLE_CODEC__;
    const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
    const store = {};
    const rowCounts = {};
    const columnWidths = {};
    const rulesBySheet = {};
    const layoutBySheet = {};
    const viewBySheet = {};
    (sheets || []).forEach((sheet) => {
      const map = {};
      let maxRow = 0;
      const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;
      (sheet.cells || []).forEach((cell) => {
        const raw = String(cell.raw ?? "");
        const style = codec ? codec.styleFromPayload(cell.style, sheet.numFmts, sheet.fonts) : null;
        if (!raw && !style) return;
        const isFormula = cell.t !== "s" && raw.startsWith("=");
        const entry = isFormula ? { value: "", formula: raw } : { value: raw, formula: "" };
        const t = isFormula ? undefined : cellTypeApi?.explicitTag(raw, cell.t);
        if (t) entry.t = t;
        if (!isFormula && raw && cell.ph && typeof cell.ph.r === "string") entry.ph = Object.freeze({ ...cell.ph, b: raw });
        if (style) entry.style = style;
        map[cell.ref] = entry;
        const packed = gridUtils ? gridUtils.packRef(cell.ref) : 0;
        if (packed > 0) {
          const r = Math.floor(packed / 32768);
          if (r > maxRow) maxRow = r;
        } else {
          const m = /(?<!\d)(\d+)$/.exec(cell.ref);
          if (m) maxRow = Math.max(maxRow, Number(m[1]));
        }
      });
      host.__EASYOFFICE_SHEET_LINK__?.attach(map, sheet.links);
      (sheet.notes || []).forEach((note) => {
        const packed = gridUtils && note?.ref ? gridUtils.packRef(note.ref) : 0;
        if (packed > 0) maxRow = Math.max(maxRow, Math.floor(packed / 32768));
      });
      (sheet.images || []).forEach((img) => {
        const tr = Number(img?.anchor?.toRow);
        if (Number.isFinite(tr) && tr >= 0) maxRow = Math.max(maxRow, Math.trunc(tr) + 1);
      });
      store[sheet.name] = map;
      rowCounts[sheet.name] = Math.max(12, maxRow);
      const widths = columnWidthMap(sheet.columns);
      if (Object.keys(widths).length > 0) columnWidths[sheet.name] = widths;
      if ((sheet.rules?.cf || []).length > 0 || (sheet.rules?.dv || []).length > 0) rulesBySheet[sheet.name] = sheet.rules;
      if (LAYOUT_KEYS.some((key) => sheet[key]?.length || (sheet[key] && !Array.isArray(sheet[key])))) {
        layoutBySheet[sheet.name] = Object.fromEntries(LAYOUT_KEYS.map((key) => [key, sheet[key]]));
      }
      // The selection, zoom and scroll position (PD-208) are not part of the layout: they are no undo step.
      if (sheet.view) viewBySheet[sheet.name] = sheet.view;
    });
    return { store, rowCounts, columnWidths, rulesBySheet, layoutBySheet, viewBySheet };
  }

  function hasFiles(ev) {
    const dt = ev?.dataTransfer;
    if (!dt) return false;
    return (dt.files && dt.files.length > 0) || Array.from(dt.types || []).includes("Files");
  }

  function applyPdf(next, res, name) {
    const lines = [];
    (res.blocks || []).forEach((b) => {
      if (b.type === "table") (b.rows || []).forEach((r) => lines.push(r.map((c) => c.text).join(" | ")));
      else if (b.type === "image") lines.push(String(b.alt || "[그림]"));
      else lines.push((b.runs || []).map((r) => r.text).join(""));
    });
    const title = res.title || name;
    const formInfo = res.pdfForm || {};
    const applicant = String(formInfo.applicant || "");
    const dept = String(formInfo.department || "");
    const fieldCount = Number(formInfo.fieldCount) || 0;
    const rawFields = Array.isArray(formInfo.fields) ? formInfo.fields : [];
    const formFields = rawFields
      .filter((f) => f && typeof f.name === "string" && f.name.trim())
      .map((f) => ({ name: f.name.trim(), value: f.value == null ? "" : String(f.value) }));
    const hasForm = Boolean(
      formInfo.hasAcroForm && (fieldCount > 0 || applicant || dept || formFields.length > 0)
    );
    next.pdfStudio = {
      ...next.pdfStudio,
      stampValue: "", stampBadge: "", watermarkInput: "", watermarkLayer: "",
      applicant, dept,
      formFields,
      formVisible: hasForm,
      formStatus: hasForm ? (fieldCount > 0 ? "양식 필드 " + fieldCount + "개 감지됨" : "양식 필드 감지됨") : "",
      cardHeadingNodes: [textSpec(title)],
      cardBodyParagraphsNodes: lines.filter((l) => l.trim()).map((l) => [textSpec(l)]),
      outlines: Array.isArray(res.pdfOutlines) ? res.pdfOutlines : [],
      pages: Array.isArray(res.pdfPages)
        ? res.pdfPages.map((p, idx) => ({ pageNumber: Number(p?.pageNumber) || idx + 1, text: String(p?.text || "") }))
        : [],
    };
    next.pdfRotationDeg = 0;
  }

  function createFileOpen(deps) {
    const {
      document, invokeIpc, enqueueOp, runExclusive, captureWorkspaceSnapshot, pushRawUndoSnapshot,
      restoreWorkspaceSnapshot, commitPendingSheetCell, computeEvaluatedSheetStore,
      createEmptySlideStyleFields, getActiveModule, replaceColumnWidths, replaceSheetRules,
    } = deps;
    const hasHwpRoute = () => Boolean(deps.getHwpSession?.());
    // A named kind answered by the native dialog (`confirm_dialog` in the shell); without a way to ask, no.
    const confirmFn = deps.confirm || (async () => false);
    const btoaFn = (bin) => host.btoa(bin);
    const baselines = {};
    let converterSource = null;

    const feedbackEl = () => document.getElementById("status-command-feedback");
    function setFeedback(text) {
      const el = feedbackEl();
      if (el) el.textContent = text;
    }
    function fingerprint(snap, mod) {
      return host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__.moduleFingerprint(snap, mod);
    }

    // Per-module baselines: what each module held when last opened/saved/initialised.
    function captureBaselines() {
      return runExclusive(async () => {
        const snap = captureWorkspaceSnapshot(false);
        Object.keys(MODULE_LABELS).forEach((mod) => { baselines[mod] = fingerprint(snap, mod); });
      });
    }

    function markModuleSaved() {
      return runExclusive(async () => {
        const mod = getActiveModule();
        if (!MODULE_LABELS[mod]) return;
        baselines[mod] = fingerprint(captureWorkspaceSnapshot(false), mod);
      });
    }

    function isModuleEdited(snap, mod) {
      return Object.hasOwn(baselines, mod) && baselines[mod] !== fingerprint(snap, mod);
    }

    async function applyDecoded(next, res, name) {
      const mod = res.module;
      if (mod === "DOC" || mod === "HWP") {
        next.docTabs = next.docTabs || {};
        const paraBlocks = (res.blocks || []).filter((b) => b.type === "paragraph");
        const firstCount = paraBlocks[0]?.columns?.count;
        const uniformCols = paraBlocks.length > 0 && Number.isInteger(firstCount) && firstCount >= 2
          && paraBlocks.every((b) => b.columns?.count === firstCount)
          ? String(firstCount)
          : "";
        next.docTabs[mod] = {
          editorNodes: [...buildEditorNodes(res.blocks), ...annotationSectionSpecs(res), ...pageFrameNodes(res)],
          // The page setup is the file's own (`doc-page-setup.js`): a file without one does not take the last document's.
          layoutStyle: {
            ...(next.docTabs[mod]?.layoutStyle || { showParaMarks: "true" }),
            eoDocFlavor: mod === "DOC" && String(res.format || "").toLowerCase() === "pages" ? "pages" : "",
            columnCount: uniformCols,
            eoFirstPage: res.firstPageDifferent ? "true" : "",
            eoEvenOdd: res.evenOddDifferent ? "true" : "",
            eoTrackChanges: res.trackChanges === true ? "true" : "",
            eoDefaults: host.__EASYOFFICE_SAVE_PAYLOAD__.packDocDefaults(res.docDefaults),
            eoWordMeta: host.__EASYOFFICE_SAVE_DOC_META__.wordMetaOfOpen(res),
            ...host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__.layoutFields(host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__.fromDecoded(res.pageSetup, mod)),
          },
        };
      } else if (mod === "SHEET") {
        const sheets = Array.isArray(res.sheets) && res.sheets.length > 0 ? res.sheets : [{ name: "Sheet1", cells: [] }];
        const { store, rowCounts, columnWidths, rulesBySheet, layoutBySheet, viewBySheet } = buildSheetStore(sheets);
        host.__EASYOFFICE_SHEET_VIEW_STATE__.replaceAll(viewBySheet);
        host.__EASYOFFICE_SHEET_LAYOUT__.replaceAll(layoutBySheet);
        host.__EASYOFFICE_SHEET_LAYOUT__.resetTableHeaders();
        next.sheetLayout = layoutBySheet;
        // The tabs follow the file's order; the book opens on its first sheet that is shown.
        const order = sheets.map((sheet) => sheet.name);
        host.__EASYOFFICE_SHEET_TAB_STRIP__.replaceOrder(order);
        next.sheetOrder = order;
        const first = order.find((name) => layoutBySheet[name]?.hidden !== true) ?? order[0];
        host.__EASYOFFICE_SHEET_TAB_STRIP__.clearGroup?.(first);
        // The rules and the defined names go in first, so the evaluation below learns them with the cells.
        if (typeof replaceSheetRules === "function") replaceSheetRules(rulesBySheet);
        next.sheetRules = rulesBySheet;
        host.__EASYOFFICE_SHEET_NAMES__.replaceAll(res.names, true);
        next.definedNames = host.__EASYOFFICE_SHEET_NAMES__.getAll();
        // The workbook's protection is the file's own; the ranges a password opened in this window are forgotten (PD-216).
        host.__EASYOFFICE_SHEET_PROTECT__.setWorkbook(res.workbookProtect ?? null);
        host.__EASYOFFICE_SHEET_PROTECT__.forgetUnlocked();
        next.workbookProtect = host.__EASYOFFICE_SHEET_PROTECT__.getWorkbook();
        host.__EASYOFFICE_SHEET_UTILS__.setDate1904(res.date1904);
        // The font of the Normal style, the unit of the widths in characters (PD-306); none for the standard font.
        host.__EASYOFFICE_SHEET_DEFAULT_FONT__?.set(res.defaultFont, res.defaultColPx, document);
        next.defaultFont = host.__EASYOFFICE_SHEET_DEFAULT_FONT__?.get() ?? null;
        host.__EASYOFFICE_SHEET_CALC_OPTIONS__?.set(res.calcOptions, document);
        next.calcOptions = host.__EASYOFFICE_SHEET_CALC_OPTIONS__?.get() ?? null;
        // The displays the page cached belong to the previous workbook's date system.
        host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__?.clear?.();
        host.__EASYOFFICE_SHEET_TABLE_STYLE__?.setTheme(res.workbookTheme ?? null);
        host.__EASYOFFICE_SHEET_TABLE_STYLE__?.setCustomStyles({});
        next.workbookTheme = host.__EASYOFFICE_SHEET_TABLE_STYLE__?.getTheme() ?? null;
        next.date1904 = host.__EASYOFFICE_SHEET_UTILS__.getDate1904();
        next.customTableStyles = {};
        const evaluated = await computeEvaluatedSheetStore(store, first, null);
        next.sheets = evaluated.sheets;
        next.sheetRowCounts = rowCounts;
        next.sheetColumnWidths = columnWidths;
        next.activeSheetName = first;
        const active = host.__EASYOFFICE_SHEET_VIEW_STATE__.get(first)?.active;
        next.activeSheetCellRef = active ? host.__EASYOFFICE_SHEET_GRID_UTILS__.refOf(active[0], active[1]) : "A1";
        if (typeof replaceColumnWidths === "function") replaceColumnWidths(columnWidths);
      } else if (mod === "SLIDE") {
        next.slides = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__.hydrateDecodedDeck(res, createEmptySlideStyleFields);
        next.activeSlideIndex = 0;
      } else if (mod === "BASE" || mod === "BOARD") {
        // A package whose model is not valid is refused whole, before any state is replaced.
        const decoded = host.__EASYOFFICE_BASE_BOARD_HOST__.decodePackage(res);
        Object.assign(next, decoded.fields);
        res.itemCount = decoded.itemCount;
      } else if (mod === "PDF") {
        applyPdf(next, res, name);
      } else {
        throw new Error("지원하지 않는 모듈입니다: " + mod);
      }
      next.activeModule = mod;
      next.tabTitles = { ...next.tabTitles, [mod]: name };
    }

    // A file replaces the whole workbook: the sample formula result goes away.
    function clearSheetEvalResult() {
      const result = document.getElementById("sheet-eval-result");
      if (result) result.textContent = "";
    }

    const ITEM_UNIT_BY_KIND = new Map([
      ["workbook", "개 시트"],
      ["slides", "개 슬라이드"],
      ["base", "개 테이블"],
      ["board", "개 요소"],
    ]);
    const DEFAULT_ITEM_UNIT = "개 블록";

    function describe(res, name) {
      const label = MODULE_LABELS[res.module] || res.module;
      const notes = Array.isArray(res.warnings) && res.warnings.length > 0 ? " · 참고: " + res.warnings.join(" ") : "";
      return "열기 완료: " + name + " (" + String(res.format).toUpperCase() + ", " + label + " 모듈 · " + res.itemCount +
        (ITEM_UNIT_BY_KIND.get(res.kind) ?? DEFAULT_ITEM_UNIT) + ")" + notes;
    }

    function fail(message) {
      setFeedback("파일 열기 실패: " + message);
      return { ok: false, error: message };
    }

    // The one entry point: file picker, drag and drop, and the desktop shell all call this.
    // HWP family files go to the embedded editor (hwp-session.js) unless `textOnly` forces the decode path.
    async function openDocumentFromBytes(fileName, bytes, options) {
      const name = baseName(fileName);
      const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const hwpSession = options?.textOnly ? null : deps.getHwpSession?.();
      if (hwpSession && HWP_EXTENSIONS.has(extensionOf(name))) {
        const opened = await hwpSession.openFromBytes(name, data, {
          textFallback: () => openDocumentFromBytes(name, data, { textOnly: true }),
        });
        // The embedded editor now holds this document, which the converter offers as its source.
        if (opened?.ok && opened.module === "HWP") selectConverterFormat(name);
        return opened;
      }
      const invalid = validateFile(name, data.length);
      if (invalid) return fail(invalid + (name ? " (" + name + ")" : ""));
      const convBox = document.getElementById("conv-result-box");
      const convText = convBox ? convBox.textContent : "";
      let res;
      try {
        const rawB64 = typeof options?.sourceBase64 === "string" ? options.sourceBase64 : "";
        const sourceBase64 = rawB64 && Math.abs(Math.floor((rawB64.length * 3) / 4) - data.length) <= 4
          ? rawB64
          : host.__EASYOFFICE_SHEET_UTILS__.bytesToBase64(data, btoaFn);
        res = await invokeIpc("convert_document_format", JSON.stringify({
          action: "decode", fileName: name, sourceBase64, ...GRID_LIMITS,
        }));
      } catch (err) {
        if (convBox) convBox.textContent = convText;
        return fail(name + " — " + (err?.message || String(err)));
      }
      if (res?.status !== "OK" || !MODULE_LABELS[res.module]) {
        return fail(name + " — 응답을 처리하지 못했습니다. 다시 시도해 주세요.");
      }
      setFeedback("파일 여는 중: " + name);
      const outcome = await enqueueOp(async () => {
        await commitPendingSheetCell(true);
        const pre = captureWorkspaceSnapshot(false);
        if (isModuleEdited(pre, res.module)) {
          if ((await confirmFn("replace-unsaved-module")) !== true) return { cancelled: true };
        }
        const next = structuredClone(pre);
        await applyDecoded(next, res, name);
        pushRawUndoSnapshot(pre);
        await restoreWorkspaceSnapshot(next, null);
        if (res.module === "SHEET") {
          clearSheetEvalResult();
          deps.restoreSheetView?.();
        } else {
          host.__EASYOFFICE_SHEET_TAB_STRIP__?.clearGroup?.();
        }
        baselines[res.module] = fingerprint(captureWorkspaceSnapshot(false), res.module);
        return { ok: true };
      }).catch((err) => ({ error: err?.message || String(err) }));
      if (outcome.cancelled) {
        setFeedback("파일 열기를 취소했습니다 (" + name + ")");
        return { ok: false, cancelled: true };
      }
      if (outcome.error) return fail(name + " — " + outcome.error);
      await deps.onOpenedClean?.();
      // A PDF is also handed to the shell, so that the studio edits its objects (PD-170).
      const pdfNote = res.module === "PDF" && deps.pdfEdit ? await deps.pdfEdit.open(name, data, options) : "";
      setFeedback(describe(res, name) + pdfNote);
      return { ok: true, module: res.module, format: res.format, warnings: res.warnings || [], title: name };
    }

    async function readFileBytes(file, hwpRoute) {
      const invalid = validateFile(baseName(file?.name), Number(file?.size ?? 0), hwpRoute);
      if (invalid) return { error: invalid };
      try {
        return { bytes: new Uint8Array(await file.arrayBuffer()) };
      } catch (err) {
        return { error: "파일을 읽지 못했습니다: " + (err?.message || String(err)) };
      }
    }

    async function openFile(file) {
      const name = baseName(file?.name);
      const read = await readFileBytes(file, hasHwpRoute());
      if (read.error) return fail(read.error + (name ? " (" + name + ")" : ""));
      return openDocumentFromBytes(name, read.bytes);
    }

    // ---- converter source file ----------------------------------------------------------

    function renderConverterSource() {
      const label = document.getElementById("conv-source-name");
      const clearBtn = document.getElementById("btn-conv-clear-source");
      if (label) label.textContent = converterSource ? converterSource.name + " (" + converterSource.bytes.length + "B)" : "선택 안 함";
      if (clearBtn) clearBtn.hidden = !converterSource;
    }

    function selectConverterFormat(name) {
      const srcSel = document.getElementById("conv-src-fmt");
      const option = converterFormatOption(name);
      if (srcSel && option && srcSel.querySelector('option[value="' + option + '"]')) srcSel.value = option;
    }

    async function setConverterSource(file) {
      const box = document.getElementById("conv-result-box");
      const name = baseName(file?.name);
      const read = await readFileBytes(file, "converter");
      if (read.error) {
        if (box) box.textContent = "원본 파일을 사용할 수 없습니다: " + read.error + (name ? " (" + name + ")" : "");
        return false;
      }
      converterSource = { name, bytes: read.bytes };
      selectConverterFormat(name);
      renderConverterSource();
      if (box) box.textContent = "원본 파일 선택됨: " + name;
      return true;
    }

    function clearConverterSource() {
      converterSource = null;
      renderConverterSource();
    }

    // Fields merged into the convert request, or null when the editor content is converted.
    function getConverterSource() {
      if (!converterSource) return null;
      const stem = converterSource.name.replace(/\.[A-Za-z0-9]+$/, "") || "변환_문서";
      return {
        baseName: stem,
        size: converterSource.bytes.length,
        fields: { fileName: converterSource.name, sourceBase64: host.__EASYOFFICE_SHEET_UTILS__.bytesToBase64(converterSource.bytes, btoaFn) },
      };
    }

    // ---- DOM wiring ---------------------------------------------------------------------

    function closeMenus() {
      host.__EASYOFFICE_CLOSE_MENUS__(document);
    }

    // In a managed window (PD-088) opening a file means a new window: `onOpenRequest` handles the
    // menu/shortcut entry and `routeFile` the picker/drop entry; both fall through to in-place open.
    function openFilePicker() {
      if (deps.onOpenRequest?.()) return;
      const input = document.getElementById("file-open-input");
      if (!input) return setFeedback("파일 열기 입력을 찾을 수 없습니다");
      input.click();
    }

    function bindUi() {
      const input = document.getElementById("file-open-input");
      input?.addEventListener("change", () => {
        const file = input.files?.[0];
        input.value = "";
        if (file && !deps.routeFile?.(file)) void openFile(file);
      });
      document.querySelectorAll("[data-file-open]").forEach((el) => {
        el.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          closeMenus();
          openFilePicker();
        });
      });
      const convInput = document.getElementById("conv-source-file");
      convInput?.addEventListener("change", () => {
        const file = convInput.files?.[0];
        convInput.value = "";
        if (file) void setConverterSource(file);
      });
      document.getElementById("btn-conv-clear-source")?.addEventListener("click", clearConverterSource);
      renderConverterSource();

      // The browser's own drop behaviour (opening or downloading the file) is off for the whole
      // document; only the editing area accepts a file and shows the drop outline. Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md),
      // `apps/studio-host/src/main.ts`: the same document-wide guard and editing-area drop target.
      document.addEventListener("dragover", (ev) => ev.preventDefault());
      document.addEventListener("drop", (ev) => ev.preventDefault());
      const dropArea = document.querySelector(".canvas-stage");
      dropArea?.addEventListener("dragover", (ev) => {
        if (!hasFiles(ev)) return;
        ev.preventDefault();
        dropArea.classList.add("eo-drop-active");
      });
      dropArea?.addEventListener("dragleave", () => dropArea.classList.remove("eo-drop-active"));
      dropArea?.addEventListener("drop", (ev) => {
        if (!hasFiles(ev)) return;
        ev.preventDefault();
        dropArea.classList.remove("eo-drop-active");
        const files = Array.from(ev.dataTransfer.files || []);
        if (files.length === 0) return;
        if (getActiveModule() !== "CONV" && deps.routeFile?.(files[0])) return;
        const pending = getActiveModule() === "CONV" ? setConverterSource(files[0]) : openFile(files[0]);
        void pending.then(() => {
          if (files.length > 1) {
            setFeedback(feedbackEl()?.textContent + " · 여러 파일 중 첫 번째 파일만 열었습니다 (" + files.length + "개 선택됨)");
          }
        });
      });
    }

    // Window namespace entry point for the desktop shell (OS file associations).
    host.__EASYOFFICE_WORKSPACE_API__ = Object.assign(host.__EASYOFFICE_WORKSPACE_API__ || {}, {
      openDocumentFromBytes,
    });

    return {
      openDocumentFromBytes, openFile, openFilePicker, bindUi, captureBaselines, markModuleSaved,
      getConverterSource, setConverterSource, clearConverterSource, converterSourceProblem,
    };
  }

  const EXT_TO_FORMAT = Object.freeze({
    docx: "docx", doc: "docx", odt: "odt", rtf: "rtf", md: "md", markdown: "md", txt: "txt",
    hwpx: "hwpx", hwp: "hwp", hwt: "hwp", hwtx: "hwpx",
    xlsx: "xlsx", xls: "xls", ods: "ods", csv: "csv", tsv: "tsv",
    pptx: "pptx", ppsx: "ppsx", ppt: "pptx", odp: "odp", pdf: "pdf",
  });

  function inferFormatFromFileName(fileName) {
    const raw = String(fileName || "").trim();
    const dot = raw.lastIndexOf(".");
    return dot >= 0 && dot < raw.length - 1 ? EXT_TO_FORMAT[raw.slice(dot + 1).toLowerCase()] || "" : "";
  }
  host.__EASYOFFICE_INFER_FORMAT__ = inferFormatFromFileName;

  host.__EASYOFFICE_CREATE_FILE_OPEN__ = createFileOpen;
  host.__EASYOFFICE_FILE_OPEN_UTILS__ = {
    MAX_FILE_BYTES, MAX_HWP_BYTES, SUPPORTED_EXTENSIONS, PACKAGE_EXTENSIONS, GRID_LIMITS, buildEditorNodes, buildSheetStore,
    converterSourceProblem, converterFormatOption,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
