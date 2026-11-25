(function (host) {
  "use strict";

  // Non-contiguous multi-range selection helpers (PD-356).
  // Bounds keep adversarial Ctrl+click chains or whole-sheet multi-ranges safe.
  const MAX_MULTI_RANGES = 64;
  const MAX_MULTI_OPS_CELLS = 200000;
  const DIRECT_AREA_CELLS = 4096;

  function normRect(r) {
    if (!r) return null;
    const r1 = Math.min(Number(r.r1) || 1, Number(r.r2) || 1);
    const r2 = Math.max(Number(r.r1) || 1, Number(r.r2) || 1);
    const c1 = Math.min(Number(r.c1) || 1, Number(r.c2) || 1);
    const c2 = Math.max(Number(r.c1) || 1, Number(r.c2) || 1);
    return { r1, c1, r2, c2 };
  }

  function sameRect(a, b) {
    return Boolean(a && b && a.r1 === b.r1 && a.c1 === b.c1 && a.r2 === b.r2 && a.c2 === b.c2);
  }

  function containsCoord(r, row, col) {
    return Boolean(r) && row >= r.r1 && row <= r.r2 && col >= r.c1 && col <= r.c2;
  }

  function anyContainsCoord(ranges, row, col) {
    for (let i = 0; i < ranges.length; i += 1) {
      const r = ranges[i];
      if (row >= r.r1 && row <= r.r2 && col >= r.c1 && col <= r.c2) return true;
    }
    return false;
  }

  // Returns normalized deduplicated list of active ranges: extra ranges followed by active primary rect.
  function rangesOf(primary, extras) {
    const p = normRect(primary);
    const out = [];
    if (Array.isArray(extras)) {
      for (let i = 0; i < extras.length && out.length < MAX_MULTI_RANGES; i += 1) {
        const nr = normRect(extras[i]);
        if (nr && !out.some((x) => sameRect(x, nr)) && !(p && sameRect(p, nr))) out.push(nr);
      }
    }
    if (p) out.push(p);
    return out;
  }

  function boundingBox(ranges) {
    if (!Array.isArray(ranges) || ranges.length === 0) return { r1: 1, c1: 1, r2: 1, c2: 1 };
    let r1 = ranges[0].r1, c1 = ranges[0].c1, r2 = ranges[0].r2, c2 = ranges[0].c2;
    for (let i = 1; i < ranges.length; i += 1) {
      const r = ranges[i];
      if (r.r1 < r1) r1 = r.r1;
      if (r.c1 < c1) c1 = r.c1;
      if (r.r2 > r2) r2 = r.r2;
      if (r.c2 > c2) c2 = r.c2;
    }
    return { r1, c1, r2, c2 };
  }

  function totalArea(ranges) {
    let total = 0;
    for (let i = 0; i < ranges.length; i += 1) {
      const r = ranges[i];
      total += (r.r2 - r.r1 + 1) * (r.c2 - r.c1 + 1);
    }
    return total;
  }

  // Visits every unique (row, col) across `ranges` once, refusing above `limit`.
  function forEachUniqueCoord(ranges, packCoord, cb, limit = MAX_MULTI_OPS_CELLS) {
    if (!Array.isArray(ranges) || ranges.length === 0) return { count: 0, refused: false };
    if (ranges.length === 1) {
      const rg = ranges[0];
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      if (area > limit) return { count: 0, refused: true };
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) cb(r, c);
      }
      return { count: area, refused: false };
    }
    if (totalArea(ranges) > limit * 2) return { count: 0, refused: true };
    const seen = new Set();
    let count = 0;
    for (let i = 0; i < ranges.length; i += 1) {
      const rg = ranges[i];
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) {
          const packed = packCoord(r, c);
          if (seen.has(packed)) continue;
          seen.add(packed);
          count += 1;
          if (count > limit) return { count: 0, refused: true };
          cb(r, c);
        }
      }
    }
    return { count, refused: false };
  }

  // Collects numeric values across `ranges` without double-counting overlapping coordinates.
  function collectNumbersMulti(ranges, liveTd, api, grid, numericOf) {
    const nums = [];
    const liveRef = liveTd ? liveTd.dataset.cell : "";
    const push = (ref) => {
      if (ref === liveRef) {
        const liveRaw = (liveTd.textContent || "").trim();
        if (liveRaw.startsWith("'") || liveTd._nf === "@") return;
        const n = numericOf(liveRaw);
        if (n !== null) nums.push(n);
        return;
      }
      const entry = api.getEntry(ref);
      if (!entry || entry.t === "s" || entry.style?.numFmt === "@") return;
      const n = numericOf(entry.value);
      if (n !== null) nums.push(n);
    };
    const area = totalArea(ranges);
    const mapSize =
      area <= DIRECT_AREA_CELLS
        ? area
        : typeof api.getEntryCount === "function"
          ? api.getEntryCount()
          : (api.activeSheetMap?.size ?? 20000);
    if (area <= mapSize) {
      if (ranges.length === 1) {
        const rg = ranges[0];
        for (let r = rg.r1; r <= rg.r2; r += 1) {
          for (let c = rg.c1; c <= rg.c2; c += 1) push(grid.refOf(r, c));
        }
        return nums;
      }
      const seen = new Set();
      for (let i = 0; i < ranges.length; i += 1) {
        const rg = ranges[i];
        for (let r = rg.r1; r <= rg.r2; r += 1) {
          for (let c = rg.c1; c <= rg.c2; c += 1) {
            const k = grid.packCoord(r, c);
            if (seen.has(k)) continue;
            seen.add(k);
            push(grid.refOf(r, c));
          }
        }
      }
      return nums;
    }
    let visitedLive = false;
    api.forEachEntryRef((ref, row, col) => {
      if (anyContainsCoord(ranges, row, col)) {
        if (ref === liveRef) visitedLive = true;
        push(ref);
      }
    });
    if (liveRef && !visitedLive) {
      const p = grid.parseRef(liveRef);
      if (p && anyContainsCoord(ranges, p.row, p.col)) push(liveRef);
    }
    return nums;
  }

  host.__EASYOFFICE_SHEET_MULTI_RANGE__ = Object.freeze({
    MAX_MULTI_RANGES,
    MAX_MULTI_OPS_CELLS,
    normRect,
    sameRect,
    containsCoord,
    anyContainsCoord,
    rangesOf,
    boundingBox,
    totalArea,
    forEachUniqueCoord,
    collectNumbersMulti,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
