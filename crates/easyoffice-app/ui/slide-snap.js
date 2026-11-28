(function (host) {
  "use strict";

  // Smart-guide snapping of an element being moved or resized to the stage center and the edges and
  // centers of its siblings. Split from slide-canvas.js, which hands in its limits and helpers.
  function createSlideSnap({ clamp, round2, SNAP_TOLERANCE_PT, MIN_EL_W, MIN_EL_H }) {
    // `guides` (PD-402) are the drawing guides shown: `{ v, h }`, the x of the vertical lines and the y of the horizontal ones, which
    // snap like the edges of a sibling.
    function computeSmartSnap(box, siblings, stage, snapEnabled, mode, dir, guides) {
      if (!snapEnabled) return { x: box.x, y: box.y, w: box.w, h: box.h, vLine: null, hLine: null };
      let { x, y, w, h } = box;
      let bestDx = SNAP_TOLERANCE_PT + 0.001, bestDy = SNAP_TOLERANCE_PT + 0.001;
      let vLine = null, hLine = null;
      const vTargets = [stage.w / 2, ...(guides?.v ?? [])], hTargets = [stage.h / 2, ...(guides?.h ?? [])];
      for (let i = 0; i < siblings.length; i += 1) {
        const s = siblings[i];
        vTargets.push(s.x, s.x + s.w / 2, s.x + s.w);
        hTargets.push(s.y, s.y + s.h / 2, s.y + s.h);
      }
      if (mode === "move") {
        const myX = [0, w / 2, w], myY = [0, h / 2, h];
        for (let t = 0; t < vTargets.length; t += 1) {
          for (let m = 0; m < 3; m += 1) {
            const ad = Math.abs(vTargets[t] - (x + myX[m]));
            if (ad <= SNAP_TOLERANCE_PT && ad < bestDx) { bestDx = ad; vLine = vTargets[t]; x = round2(vTargets[t] - myX[m]); }
          }
        }
        for (let t = 0; t < hTargets.length; t += 1) {
          for (let m = 0; m < 3; m += 1) {
            const ad = Math.abs(hTargets[t] - (y + myY[m]));
            if (ad <= SNAP_TOLERANCE_PT && ad < bestDy) { bestDy = ad; hLine = hTargets[t]; y = round2(hTargets[t] - myY[m]); }
          }
        }
        x = clamp(x, 0, Math.max(0, stage.w - w));
        y = clamp(y, 0, Math.max(0, stage.h - h));
      } else {
        if (dir.includes("e")) {
          const right = x + w;
          for (let t = 0; t < vTargets.length; t += 1) {
            const ad = Math.abs(vTargets[t] - right);
            if (ad <= SNAP_TOLERANCE_PT && ad < bestDx) { bestDx = ad; vLine = vTargets[t]; w = Math.max(MIN_EL_W, round2(vTargets[t] - x)); }
          }
        } else if (dir.includes("w")) {
          const right = x + w;
          for (let t = 0; t < vTargets.length; t += 1) {
            const ad = Math.abs(vTargets[t] - x);
            if (ad <= SNAP_TOLERANCE_PT && ad < bestDx && right - vTargets[t] >= MIN_EL_W) { bestDx = ad; vLine = vTargets[t]; x = round2(vTargets[t]); w = round2(right - x); }
          }
        }
        if (dir.includes("s")) {
          const bottom = y + h;
          for (let t = 0; t < hTargets.length; t += 1) {
            const ad = Math.abs(hTargets[t] - bottom);
            if (ad <= SNAP_TOLERANCE_PT && ad < bestDy) { bestDy = ad; hLine = hTargets[t]; h = Math.max(MIN_EL_H, round2(hTargets[t] - y)); }
          }
        } else if (dir.includes("n")) {
          const bottom = y + h;
          for (let t = 0; t < hTargets.length; t += 1) {
            const ad = Math.abs(hTargets[t] - y);
            if (ad <= SNAP_TOLERANCE_PT && ad < bestDy && bottom - hTargets[t] >= MIN_EL_H) { bestDy = ad; hLine = hTargets[t]; y = round2(hTargets[t]); h = round2(bottom - y); }
          }
        }
      }
      return { x: round2(x), y: round2(y), w: round2(w), h: round2(h), vLine, hLine };
    }

    return { computeSmartSnap };
  }

  host.__EASYOFFICE_CREATE_SLIDE_SNAP__ = createSlideSnap;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
