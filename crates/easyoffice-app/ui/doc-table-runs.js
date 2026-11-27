(function (host) {
  "use strict";

  // Helper for DOC paragraph spacing (0..400pt) and rich table-cell formatting
  // (inner runs + per-cell paragraph alignment / line spacing) across DOM <-> Rust JSON.

  const MAX_PARA_SPACING_PT = 400;
  const MAX_PARA_SPACING_TWIPS = 8000;
  const TWIPS_PER_PT = 20;
  const TWIPS_PER_PX = 15;
  const PX_PER_TWIP = 1 / TWIPS_PER_PX;
  const PX_RE = /^(-?\d+(?:\.\d+)?)px$/i;
  const PT_RE = /^(-?\d+(?:\.\d+)?)pt$/i;
  const BLOCK_CELL_TAGS = new Set(["P", "DIV", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "BLOCKQUOTE", "PRE"]);
  const NON_DEFAULT_CELL_ALIGNS = new Set(["center", "right", "justify"]);

  function twipsToPx(twips) {
    return String(Math.round(twips * PX_PER_TWIP * 1000) / 1000) + "px";
  }

  function clampTwips(val) {
    const n = Math.round(Number(val) || 0);
    return n > 0 ? Math.min(MAX_PARA_SPACING_TWIPS, n) : 0;
  }

  function styleSpacingTwips(rawStyle) {
    const s = String(rawStyle || "").trim();
    if (!s) return null;
    if (s === "0" || s === "0px" || s === "0pt") return 0;
    const pm = PX_RE.exec(s);
    if (pm) return clampTwips(Number(pm[1]) * TWIPS_PER_PX);
    const ptm = PT_RE.exec(s);
    if (ptm) return clampTwips(Number(ptm[1]) * TWIPS_PER_PT);
    return null;
  }

  function spacingProps(which) {
    return which === "before"
      ? { styleProp: "marginTop", dataKey: "eoSpaceBeforeTwips", jsonKey: "spaceBeforeTwips" }
      : { styleProp: "marginBottom", dataKey: "eoSpaceAfterTwips", jsonKey: "spaceAfterTwips" };
  }

  function readSpacingTwips(node, which) {
    if (!node || !node.style) return 0;
    const { styleProp, dataKey } = spacingProps(which);
    const rawStyle = node.style[styleProp];
    const fromStyle = styleSpacingTwips(rawStyle);
    if (fromStyle === 0) return 0;
    const storedRaw = node.dataset?.[dataKey];
    const stored = storedRaw !== undefined && storedRaw !== "" ? Number(storedRaw) : NaN;
    if (fromStyle !== null) {
      if (Number.isFinite(stored) && stored >= 0 && stored <= MAX_PARA_SPACING_TWIPS) {
        const expectedPx = Math.round(stored * PX_PER_TWIP * 1000) / 1000;
        const rawPx = PX_RE.exec(String(rawStyle || "").trim());
        if (rawPx && Math.abs(Number(rawPx[1]) - expectedPx) <= 0.15) return Math.round(stored);
      }
      return fromStyle;
    }
    if (Number.isFinite(stored) && stored > 0) return clampTwips(stored);
    return 0;
  }

  function readSpacingPt(node, which) {
    const twips = readSpacingTwips(node, which);
    return Math.round((twips / TWIPS_PER_PT) * 10) / 10;
  }

  function writeSpacingPt(node, which, rawPt) {
    if (!node || !node.style) return false;
    const n = Number(rawPt);
    if (!Number.isFinite(n)) return false;
    const clampedPt = Math.max(0, Math.min(MAX_PARA_SPACING_PT, n));
    const twips = Math.round(clampedPt * TWIPS_PER_PT);
    const { styleProp, dataKey } = spacingProps(which);
    const prevTwips = readSpacingTwips(node, which);
    const prevInline = String(node.style[styleProp] || "");
    if (twips > 0) {
      const nextPx = twipsToPx(twips);
      if (prevTwips === twips && prevInline === nextPx && node.dataset?.[dataKey] === String(twips)) return false;
      node.style[styleProp] = nextPx;
      if (node.dataset) node.dataset[dataKey] = String(twips);
      return true;
    }
    const needExplicitZero = which === "after" && (node.tagName || "").toUpperCase() === "P";
    const nextInline = needExplicitZero ? "0px" : "";
    if (prevTwips === 0 && prevInline === nextInline && node.dataset?.[dataKey] === undefined) return false;
    node.style[styleProp] = nextInline;
    if (node.dataset) delete node.dataset[dataKey];
    return true;
  }

  function paragraphSpacingLayout(node) {
    const out = {};
    const before = readSpacingTwips(node, "before");
    if (before > 0) out.spaceBeforeTwips = before;
    const after = readSpacingTwips(node, "after");
    if (after > 0) out.spaceAfterTwips = after;
    return out;
  }

  function applyParagraphSpacingSpec(spec, block, twipsToPxFn) {
    const fmtPx = twipsToPxFn || twipsToPx;
    const before = Number(block?.spaceBeforeTwips);
    const after = Number(block?.spaceAfterTwips);
    if (Number.isFinite(before) && before > 0 && before <= MAX_PARA_SPACING_TWIPS) {
      const rounded = Math.round(before);
      spec.style.marginTop = fmtPx(rounded);
      spec.data.eoSpaceBeforeTwips = String(rounded);
    }
    if (Number.isFinite(after) && after > 0 && after <= MAX_PARA_SPACING_TWIPS) {
      const rounded = Math.round(after);
      spec.style.marginBottom = fmtPx(rounded);
      spec.data.eoSpaceAfterTwips = String(rounded);
    }
  }

  function runHasRichFormat(run) {
    return Boolean(
      run &&
        (run.bold ||
          run.italic ||
          run.underline ||
          run.strike ||
          run.script ||
          run.sizeHalfPt !== undefined ||
          run.letterSpacingTwips !== undefined ||
          run.scalePercent !== undefined ||
          run.positionHalfPt !== undefined ||
          run.kerningHalfPt !== undefined ||
          run.openType !== undefined ||
          run.snapToGrid !== undefined ||
          run.textFx !== undefined ||
          run.underlineColor !== undefined ||
          run.font !== undefined ||
          run.fontFace !== undefined ||
          run.color !== undefined ||
          run.highlight !== undefined)
    );
  }

  function cellParasNeedSave(paragraphs) {
    return paragraphs.some(
      (p) =>
        NON_DEFAULT_CELL_ALIGNS.has(p.align) ||
        (Number.isInteger(p.lineSpacingPercent) && p.lineSpacingPercent !== 100) ||
        Boolean(p.lineSpacingKind) ||
        (Array.isArray(p.runs) && p.runs.some(runHasRichFormat))
    );
  }

  function finalizeParagraphRuns(para) {
    const runs = para.runs;
    if (runs.length > 0) {
      runs[0].text = runs[0].text.replace(/^ +/, "");
      runs[runs.length - 1].text = runs[runs.length - 1].text.replace(/ +$/, "").replace(/\n$/, "").replace(/ +$/, "");
    }
    const kept = runs.filter((r) => r.text.length > 0);
    const out = { runs: kept };
    if (para.align) out.align = para.align;
    if (para.special) Object.assign(out, host.__EASYOFFICE_DOC_LINE_SPACING__.jsonFields(para.special));
    else if (para.lineSpacingPercent) out.lineSpacingPercent = para.lineSpacingPercent;
    return out;
  }

  function extractCellParagraphsRaw(cell, helpers) {
    const { isSkippedElement, styleFormat, normalizeAlign, parseLineSpacing, applyFmtFields, sameFormat } = helpers;
    const cellAlign = normalizeAlign(cell.style?.textAlign);
    const cellLs = parseLineSpacing(cell.style?.lineHeight || "");
    const lineKinds = host.__EASYOFFICE_DOC_LINE_SPACING__;
    const cellKind = lineKinds?.specialOf(cell) || null;
    const paragraphs = [];
    let hasBlockChild = false;

    function pushStream(nodes, align, lineSpacingPercent, initialFmt, special) {
      let curRuns = [];
      let pushedInStream = 0;
      const startFmt = initialFmt || { bold: false, italic: false, underline: false, strike: false };
      const pushCurPara = () => {
        paragraphs.push(finalizeParagraphRuns({ runs: curRuns, align, lineSpacingPercent, special }));
        curRuns = [];
      };
      const appendText = (text, fmt) => {
        if (!text) return;
        const last = curRuns[curRuns.length - 1];
        if (sameFormat(last, fmt)) {
          last.text += text;
        } else {
          curRuns.push(applyFmtFields({ text }, fmt, true));
        }
      };
      const walkInline = (node, fmt) => {
        if (node.nodeType === 3) {
          const prev = curRuns[curRuns.length - 1]?.text || "";
          let text = String(node.nodeValue || "").replaceAll(/\s+/g, " ");
          if ((curRuns.length === 0 || /[ \n]$/.test(prev)) && text.startsWith(" ")) text = text.slice(1);
          appendText(text, fmt);
          return;
        }
        if (node.nodeType !== 1 || isSkippedElement(node)) return;
        const tag = (node.tagName || "").toUpperCase();
        if (tag === "IMG") return;
        if (tag === "BR") {
          pushCurPara();
          pushedInStream += 1;
          return;
        }
        const nextFmt = styleFormat(node, fmt);
        const kids = node.childNodes;
        if (!kids) return;
        for (let i = 0; i < kids.length; i += 1) walkInline(kids[i], nextFmt);
      };
      for (const n of nodes) walkInline(n, startFmt);
      if (curRuns.length > 0 || pushedInStream === 0) pushCurPara();
    }

    let pendingInline = [];
    const flushInline = () => {
      if (pendingInline.length === 0) return;
      pushStream(pendingInline, cellAlign, cellLs, null, cellKind);
      pendingInline = [];
    };
    for (const child of Array.from(cell.childNodes || [])) {
      if (child.nodeType === 1 && isSkippedElement(child)) continue;
      if (child.nodeType === 1 && BLOCK_CELL_TAGS.has((child.tagName || "").toUpperCase())) {
        hasBlockChild = true;
        flushInline();
        const blkAlign = normalizeAlign(child.style?.textAlign) || cellAlign;
        const blkLs = parseLineSpacing(child.style?.lineHeight || "") || cellLs;
        const blkFmt = styleFormat(child, { bold: false, italic: false, underline: false, strike: false });
        const dr = host.__EASYOFFICE_DOC_RUNS__, st = child.style || {};
        if (dr) {
          const sz = st.fontSize ? dr.cssSizeToHalfPt?.(st.fontSize) : null;
          const fn = st.fontFamily ? dr.normalizeFont?.(st.fontFamily) : null;
          const col = st.color ? dr.normalizeColor?.(st.color) : null;
          const hi = st.backgroundColor ? dr.normalizeColor?.(st.backgroundColor) : null;
          if (sz && blkFmt.size === undefined) blkFmt.size = sz;
          if (fn && blkFmt.font === undefined) blkFmt.font = fn;
          if (col && blkFmt.color === undefined) blkFmt.color = col;
          if (hi && blkFmt.highlight === undefined) blkFmt.highlight = hi;
        }
        pushStream(Array.from(child.childNodes || []), blkAlign, blkLs, blkFmt, lineKinds?.specialOf(child) || cellKind);
      } else {
        pendingInline.push(child);
      }
    }
    if (pendingInline.length > 0 || paragraphs.length === 0) flushInline();

    while (
      paragraphs.length > 0 &&
      paragraphs[paragraphs.length - 1].runs.length === 0 &&
      !paragraphs[paragraphs.length - 1].align &&
      !paragraphs[paragraphs.length - 1].lineSpacingPercent &&
      !paragraphs[paragraphs.length - 1].lineSpacingKind
    ) {
      paragraphs.pop();
    }
    return { paragraphs, hasBlockChild };
  }

  function readCellParagraphs(cell, helpers) {
    const { paragraphs } = extractCellParagraphsRaw(cell, helpers);
    return cellParasNeedSave(paragraphs) ? paragraphs : null;
  }

  function paragraphsToTextAndLinks(paragraphs) {
    const enc = new (host.TextEncoder || TextEncoder)();
    let text = "";
    let bytePos = 0;
    const links = [];
    for (let pIdx = 0; pIdx < paragraphs.length; pIdx += 1) {
      if (pIdx > 0) {
        text += "\n";
        bytePos += 1;
      }
      for (const run of paragraphs[pIdx].runs || []) {
        const rt = String(run?.text || "");
        if (!rt) continue;
        const bLen = enc.encode(rt).length;
        const start = bytePos;
        const end = start + bLen;
        if (run.link) {
          const prev = links[links.length - 1];
          if (prev && prev.end === start && prev.url === run.link) prev.end = end;
          else links.push({ start, end, url: run.link });
        }
        text += rt;
        bytePos = end;
      }
    }
    return { text, links };
  }

  function readCellSaveEntry(cell, helpers, fallbackReadCellContent) {
    const { paragraphs, hasBlockChild } = extractCellParagraphsRaw(cell, helpers);
    const rich = cellParasNeedSave(paragraphs) ? paragraphs : null;
    if ((hasBlockChild || paragraphs.length > 1) && paragraphs.length > 0) {
      const { text, links } = paragraphsToTextAndLinks(paragraphs);
      return { text, links, paragraphs: rich };
    }
    const base = fallbackReadCellContent ? fallbackReadCellContent(cell) : { text: "", links: [] };
    return { text: base.text, links: base.links, paragraphs: rich };
  }

  function applyCellLinksToParagraphs(paragraphs, cellLinks) {
    if (!Array.isArray(cellLinks) || cellLinks.length === 0) return paragraphs;
    if (paragraphs.some((p) => Array.isArray(p.runs) && p.runs.some((r) => r.link))) return paragraphs;
    const enc = new (host.TextEncoder || TextEncoder)();
    let bytePos = 0;
    return paragraphs.map((p, pIdx) => {
      if (pIdx > 0) bytePos += 1;
      const outRuns = [];
      for (const run of p.runs || []) {
        const rt = String(run?.text || "");
        if (!rt) continue;
        const chars = Array.from(rt);
        const charStarts = new Array(chars.length + 1);
        charStarts[0] = bytePos;
        for (let i = 0; i < chars.length; i += 1) {
          charStarts[i + 1] = charStarts[i] + enc.encode(chars[i]).length;
        }
        const runEnd = charStarts[chars.length];
        let segStart = 0;
        const urlAt = (bs, be) => cellLinks.find((l) => l && bs >= l.start && be <= l.end)?.url || null;
        let curUrl = chars.length > 0 ? urlAt(charStarts[0], charStarts[1]) : null;
        for (let i = 1; i < chars.length; i += 1) {
          const nextUrl = urlAt(charStarts[i], charStarts[i + 1]);
          if (nextUrl !== curUrl) {
            const slice = chars.slice(segStart, i).join("");
            if (slice) outRuns.push(curUrl ? { ...run, text: slice, link: curUrl } : { ...run, text: slice });
            segStart = i;
            curUrl = nextUrl;
          }
        }
        const tail = chars.slice(segStart).join("");
        if (tail) outRuns.push(curUrl ? { ...run, text: tail, link: curUrl } : { ...run, text: tail });
        bytePos = runEnd;
      }
      return { ...p, runs: outRuns };
    });
  }

  function buildCellChildrenSpecs(text, cellLinks, entry, imgs, helpers) {
    const { textSpec, elSpec, runSpecs, imageSpec, cellInlineSpecs } = helpers;
    if (!entry || !Array.isArray(entry.paragraphs) || entry.paragraphs.length === 0) {
      const inlineKids = cellInlineSpecs(text, cellLinks);
      const children = imgs?.length
        ? [...(text === "" && (!cellLinks || cellLinks.length === 0) ? [] : inlineKids), ...imgs.map(imageSpec)]
        : inlineKids;
      return { children, cellStyle: {}, cellData: {} };
    }
    const paragraphs = applyCellLinksToParagraphs(entry.paragraphs, cellLinks);
    if (paragraphs.length === 1) {
      const p = paragraphs[0];
      const cellStyle = {};
      const cellData = {};
      if (p.align) cellStyle.textAlign = p.align;
      const lengthRule = host.__EASYOFFICE_DOC_LINE_SPACING__?.fromJson(p);
      if (lengthRule) host.__EASYOFFICE_DOC_LINE_SPACING__.applyToSpec({ style: cellStyle, data: cellData }, lengthRule);
      else if (Number.isInteger(p.lineSpacingPercent) && p.lineSpacingPercent > 0 && p.lineSpacingPercent !== 100) {
        cellStyle.lineHeight = String(p.lineSpacingPercent / 100);
        cellData.eoLineSpacing = String(p.lineSpacingPercent);
      }
      const inline = Array.isArray(p.runs) && p.runs.length > 0 ? runSpecs(p.runs) : [];
      const base = inline.length > 0 ? inline : imgs?.length ? [] : [textSpec(text)];
      const children = imgs?.length ? [...base, ...imgs.map(imageSpec)] : base;
      return { children, cellStyle, cellData };
    }
    const children = paragraphs.map((p) => {
      const st = {};
      const ds = {};
      if (p.align) st.textAlign = p.align;
      const lengthRule = host.__EASYOFFICE_DOC_LINE_SPACING__?.fromJson(p);
      if (lengthRule) host.__EASYOFFICE_DOC_LINE_SPACING__.applyToSpec({ style: st, data: ds }, lengthRule);
      else if (Number.isInteger(p.lineSpacingPercent) && p.lineSpacingPercent > 0 && p.lineSpacingPercent !== 100) {
        st.lineHeight = String(p.lineSpacingPercent / 100);
        ds.eoLineSpacing = String(p.lineSpacingPercent);
      }
      const inline = Array.isArray(p.runs) && p.runs.length > 0 ? runSpecs(p.runs) : [];
      if (inline.length === 0) inline.push(elSpec("br"));
      const pSpec = elSpec("p", inline, st);
      pSpec.data = ds;
      return pSpec;
    });
    if (imgs?.length) children.push(...imgs.map(imageSpec));
    return { children, cellStyle: {}, cellData: {} };
  }

  host.__EASYOFFICE_DOC_TABLE_RUNS__ = {
    MAX_PARA_SPACING_PT,
    MAX_PARA_SPACING_TWIPS,
    readSpacingTwips,
    readSpacingPt,
    writeSpacingPt,
    paragraphSpacingLayout,
    applyParagraphSpacingSpec,
    readCellParagraphs,
    readCellSaveEntry,
    buildCellChildrenSpecs,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
