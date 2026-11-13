(function (host) {
  "use strict";

  // Page layout arithmetic of the document editor (`doc-pages.js`). The editor stays one editable flow; this
  // core decides, from measured block heights, where the page boundaries fall. It touches no DOM, so it runs
  // on fake heights in tests.
  //
  // Coordinates are CSS px from the top of the editor. Page k starts at k * pitch (pitch = paper height + the
  // gap between pages); its content area is [k * pitch + contentTop, k * pitch + contentBottom].
  //   - a block that does not split ("whole": tables, flex headings) moves to the next page as one piece
  //     (a margin), unless it is taller than a page;
  //   - a block of text lines splits at the line that crosses the content bottom: the lines below the
  //     boundary move to the next page through a band (start, end in block-local px) that the page layer
  //     draws as an empty float shape, so the editing DOM keeps one element per paragraph;
  //   - orphan and widow control keep at least two lines of a split paragraph on each page;
  //   - a heading stays with the first line of what follows, and a block marked `brk` starts a page.

  const PAPER_PX = { A4: [794, 1123], Letter: [816, 1056] };
  const DEFAULT_PAPER = "A4";
  // The reference studio's gap between pages at 100% zoom (`rhwp-studio/src/view/page-gap.ts`).
  const PAGE_GAP_PX = 10;
  const EPS = 0.02;
  const MIN_LINES = 2;
  const MAX_BANDS = 4096;

  function pageGeometry({ paper, landscape, size, padding, gap = PAGE_GAP_PX }) {
    const [w, h] = PAPER_PX[paper] || PAPER_PX[DEFAULT_PAPER];
    const width = size ? size[0] : landscape ? h : w;
    const height = size ? size[1] : landscape ? w : h;
    const pad = { top: 0, right: 0, bottom: 0, left: 0, ...padding };
    return {
      width, height, gap, pitch: height + gap,
      padTop: pad.top, padRight: pad.right, padBottom: pad.bottom, padLeft: pad.left,
      contentTop: pad.top, contentBottom: height - pad.bottom, contentHeight: height - pad.top - pad.bottom,
    };
  }

  const round2 = (v) => Math.round(v * 100) / 100;

  // The landscape command marks the editor with this max-width (`ribbon-history.js`); the paper is `data-paper-size`.
  const LANDSCAPE_MAX_WIDTH = "980px";
  // A page setup (`data-eo-page`, `doc-page-setup.js`: width, height in twips first) gives the paper in px itself.
  const PX_PER_TWIP = 1 / 15;
  function setupSize(text) {
    const [width, height] = String(text ?? "").split(",").map(Number);
    return width > 0 && height > 0 ? [Math.round(width * PX_PER_TWIP * 100) / 100, Math.round(height * PX_PER_TWIP * 100) / 100] : null;
  }
  const paperOf = (editor) => ({ paper: editor.dataset.paperSize, landscape: editor.style.maxWidth === LANDSCAPE_MAX_WIDTH, size: setupSize(editor.dataset.eoPage) });

  // Where the lines of a block that starts at `y0` (page `k0`) fall. Returns `{ bands, h }` (bands null when it
  // fits) or null when the first lines would be orphaned and the block should move whole.
  function splitLines(u, y0, k0, geo, atTop) {
    const { pitch, contentBottom, contentTop } = geo;
    const { h, lh } = u;
    if (!(lh > 0) || y0 + h <= k0 * pitch + contentBottom + EPS) return { bands: null, h };
    const total = Math.max(1, Math.round(h / lh));
    const bands = [];
    let k = k0, m = 0, base = 0;
    for (let guard = 0; guard < MAX_BANDS; guard += 1) {
      const s = k * pitch + contentBottom - y0;
      if (base + h - m * lh <= s + EPS) break;
      let j = m + Math.max(0, Math.floor((s - base + EPS) / lh));
      let start = s;
      const after = total - j;
      if (after > 0 && after < MIN_LINES && j - m > MIN_LINES) {
        j -= MIN_LINES - after;
        start = base + (j - m) * lh;
      }
      if (m === 0 && j < MIN_LINES && !atTop) return null;
      const end = (k + 1) * pitch + contentTop - y0;
      bands.push([round2(start), round2(end)]);
      m = j;
      base = end;
      k += 1;
    }
    return { bands, h: base + h - m * lh };
  }

  const bandSignature = (bands) => (bands ? bands.map((b) => b[0] + ":" + b[1]).join(",") : "");

  // `units[i]`: `{ h, gap, lines, lh, brk, keep, keepMin, hAct, actSig }`: natural height, the natural gap to the
  // previous unit's bottom, whether it splits at lines and its line height, a forced break before it, heading
  // keep-with-next (with the height of the next block's first line), and the height measured with the bands
  // whose signature is `actSig`. Returns per unit `top`, `bottom`, `page`, `endPage`, `push` (px added above the
  // unit), `bands`, `sig` and `pageCount`.
  function paginate(units, geo) {
    const n = units.length;
    const out = {
      top: new Float64Array(n), bottom: new Float64Array(n), push: new Float64Array(n), height: new Float64Array(n),
      page: new Int32Array(n), endPage: new Int32Array(n), bands: new Array(n).fill(null), sig: new Array(n).fill(""), pageCount: 1,
    };
    const { pitch, contentTop, contentBottom, contentHeight } = geo;
    let prev = 0;
    for (let i = 0; i < n; i += 1) {
      const u = units[i];
      const natural = prev + u.gap;
      let y = natural;
      let bands = null;
      let h = u.h;
      for (let attempt = 0; attempt < 3; attempt += 1) {
        let k = Math.floor((y + EPS) / pitch);
        let o = y - k * pitch;
        if (o < contentTop - EPS) {
          y = k * pitch + contentTop;
          o = contentTop;
        } else if (o >= contentBottom - EPS) {
          k += 1;
          y = k * pitch + contentTop;
          o = contentTop;
        }
        const atTop = o <= contentTop + EPS;
        if (u.brk && !atTop) {
          y = (k + 1) * pitch + contentTop;
          continue;
        }
        const room = contentBottom - o;
        if (u.keep && !atTop && u.h + (u.keepMin || 0) > room + EPS && u.h + (u.keepMin || 0) <= contentHeight) {
          y = (k + 1) * pitch + contentTop;
          continue;
        }
        if (!u.lines) {
          if (u.h > room + EPS && !atTop && u.h <= contentHeight + EPS) {
            y = (k + 1) * pitch + contentTop;
            continue;
          }
          break;
        }
        const split = splitLines(u, y, k, geo, atTop);
        if (!split) {
          y = (k + 1) * pitch + contentTop;
          continue;
        }
        bands = split.bands;
        h = split.h;
        break;
      }
      const sig = bandSignature(bands);
      if (bands && u.actSig === sig && u.hAct > 0) h = u.hAct;
      out.top[i] = y;
      out.push[i] = Math.max(0, y - natural);
      out.height[i] = h;
      out.bottom[i] = y + h;
      out.page[i] = Math.floor((y + EPS) / pitch);
      out.endPage[i] = Math.max(out.page[i], Math.floor((y + h - EPS) / pitch));
      out.bands[i] = bands;
      out.sig[i] = sig;
      if (out.endPage[i] + 1 > out.pageCount) out.pageCount = out.endPage[i] + 1;
      prev = y + h;
    }
    return out;
  }

  // `shape-outside` polygon of a block's bands (block-local px): one rectangle per band, joined along the left
  // edge so the shape has no area between them, and the height of the float that carries it.
  function bandShape(bands) {
    const pts = [];
    for (const [s, e] of bands) pts.push("0 " + s + "px", "100% " + s + "px", "100% " + e + "px", "0 " + e + "px");
    return { shape: "polygon(" + pts.join(",") + ")", height: bands[bands.length - 1][1] };
  }

  host.__EASYOFFICE_DOC_PAGES_CORE__ = { PAPER_PX, PAGE_GAP_PX, EPS, LANDSCAPE_MAX_WIDTH, paperOf, pageGeometry, paginate, bandShape, bandSignature, round2 };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
