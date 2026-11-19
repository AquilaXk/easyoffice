// 참조 구현 (reference implementation): Excalidraw (`excalidraw/excalidraw`, MIT, Copyright (c) 2020 Excalidraw,
// commit 091bc0e9d86fef5727c3f4d148a73e441b064ed1, `packages/element/src/elbowArrow.ts`) 비균일 격자 A* 직각 라우터
// (BinaryHeap decrease-key·역방향 금지·16-케이스 굽힘 수 추정·동일선 정점 압축) 및 xyflow (`xyflow/xyflow`, MIT,
// Copyright (c) 2019-2025 webkid GmbH, commit 29ab9e001c2ba320c9d84b00c729c745132d9145,
// `packages/system/src/utils/edges/{bezier-edge,smoothstep-edge}.ts`) 곡률 제어 3차 베지어·직각 모서리 굽힘(`getBend`);
// see `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  const STUB = 18;
  const PAD = 14;
  const TURN_PENALTY = 28;
  const MAX_GRID_AXIS = 48;
  const CORRIDOR_MARGIN = 180;
  const MAX_CORRIDOR_OBSTACLES = 16;
  const DEFAULT_CURVATURE = 0.25;
  const DEFAULT_BEND_RADIUS = 5;
  const EPS = 0.01;
  const DEDUP_THRESHOLD = 0.05;

  // Cardinal headings: 0 = UP, 1 = RIGHT, 2 = DOWN, 3 = LEFT
  const HEADING_UP = 0;
  const HEADING_RIGHT = 1;
  const HEADING_DOWN = 2;
  const HEADING_LEFT = 3;
  const STEP_DX = [0, 1, 0, -1];
  const STEP_DY = [-1, 0, 1, 0];

  const PORT_SET = new Set(["top", "right", "bottom", "left"]);
  const DIR_NORMALS = Object.freeze({
    top: Object.freeze({ nx: 0, ny: -1, heading: HEADING_UP }),
    right: Object.freeze({ nx: 1, ny: 0, heading: HEADING_RIGHT }),
    bottom: Object.freeze({ nx: 0, ny: 1, heading: HEADING_DOWN }),
    left: Object.freeze({ nx: -1, ny: 0, heading: HEADING_LEFT }),
  });

  const round2 = (v) => Math.round((Number(v) || 0) * 100) / 100;
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const compareNum = (a, b) => a - b;
  const flipHeading = (h) => (h + 2) & 3;
  const mDist = (ax, ay, bx, by) => Math.abs(ax - bx) + Math.abs(ay - by);

  function boxOf(node) {
    const x = Number(node?.x) || 0;
    const y = Number(node?.y) || 0;
    const w = Math.max(1, Number(node?.w) || 1);
    const h = Math.max(1, Number(node?.h) || 1);
    return { id: String(node?.id ?? ""), x, y, w, h, x0: x, y0: y, x1: x + w, y1: y + h, cx: x + w * 0.5, cy: y + h * 0.5 };
  }

  function portPoint(node, port) {
    const b = boxOf(node);
    const p = PORT_SET.has(port) ? port : "right";
    const norm = DIR_NORMALS[p];
    if (p === "top") return { x: round2(b.cx), y: round2(b.y0), nx: norm.nx, ny: norm.ny, heading: norm.heading, port: p };
    if (p === "bottom") return { x: round2(b.cx), y: round2(b.y1), nx: norm.nx, ny: norm.ny, heading: norm.heading, port: p };
    if (p === "left") return { x: round2(b.x0), y: round2(b.cy), nx: norm.nx, ny: norm.ny, heading: norm.heading, port: p };
    return { x: round2(b.x1), y: round2(b.cy), nx: norm.nx, ny: norm.ny, heading: norm.heading, port: p };
  }

  function bestAutoTargetPort(anchorPt, targetBox) {
    const refX = anchorPt.x + anchorPt.nx * STUB;
    const refY = anchorPt.y + anchorPt.ny * STUB;
    let bestPort = "left";
    let bestScore = Infinity;
    for (const p of ["top", "right", "bottom", "left"]) {
      const cand = portPoint(targetBox, p);
      const dx = refX - cand.x;
      const dy = refY - cand.y;
      const dist = Math.abs(dx) + Math.abs(dy);
      const dot = cand.nx * dx + cand.ny * dy;
      const score = dist + (dot >= 0 ? 0 : 120);
      if (score < bestScore) { bestScore = score; bestPort = p; }
    }
    return bestPort;
  }

  function resolveAnchorPorts(fromNode, toNode, fromPort = "auto", toPort = "auto") {
    const a = boxOf(fromNode);
    const b = boxOf(toNode);
    let fp = PORT_SET.has(fromPort) ? fromPort : "auto";
    let tp = PORT_SET.has(toPort) ? toPort : "auto";
    if (fp === "auto" && tp === "auto") {
      const dx = b.cx - a.cx;
      const dy = b.cy - a.cy;
      const gapX = Math.abs(dx) - (a.w + b.w) * 0.5;
      const gapY = Math.abs(dy) - (a.h + b.h) * 0.5;
      if (gapX >= gapY) { fp = dx >= 0 ? "right" : "left"; tp = dx >= 0 ? "left" : "right"; }
      else { fp = dy >= 0 ? "bottom" : "top"; tp = dy >= 0 ? "top" : "bottom"; }
    } else if (fp === "auto") {
      fp = bestAutoTargetPort(portPoint(b, tp), a);
    } else if (tp === "auto") {
      tp = bestAutoTargetPort(portPoint(a, fp), b);
    }
    return { fromPort: fp, toPort: tp, from: portPoint(a, fp), to: portPoint(b, tp) };
  }

  // Typed-array BinaryHeap with O(log N) rescoreElement (decrease-key)
  class BinaryStateHeap {
    constructor(totalStates) {
      this.fScores = new Float64Array(totalStates);
      this.heap = new Int32Array(Math.min(totalStates, 1024));
      this.pos = new Int32Array(totalStates).fill(-1);
      this.length = 0;
    }
    size() { return this.length; }
    push(state, f) {
      this.fScores[state] = f;
      if (this.length >= this.heap.length) {
        const next = new Int32Array(this.heap.length * 2);
        next.set(this.heap);
        this.heap = next;
      }
      const idx = this.length++;
      this.heap[idx] = state;
      this.pos[state] = idx;
      this.sinkDown(idx);
    }
    rescoreElement(state, f) {
      this.fScores[state] = f;
      const idx = this.pos[state];
      if (idx >= 0) this.sinkDown(idx);
      else this.push(state, f);
    }
    pop() {
      if (this.length === 0) return -1;
      const top = this.heap[0];
      this.pos[top] = -1;
      const endIdx = --this.length;
      if (endIdx > 0) {
        const moved = this.heap[endIdx];
        this.heap[0] = moved;
        this.pos[moved] = 0;
        this.bubbleUp(0);
      }
      return top;
    }
    sinkDown(n) {
      const state = this.heap[n];
      const score = this.fScores[state];
      let i = n;
      while (i > 0) {
        const p = ((i + 1) >> 1) - 1;
        const parentState = this.heap[p];
        if (score >= this.fScores[parentState]) break;
        this.heap[i] = parentState;
        this.pos[parentState] = i;
        i = p;
      }
      this.heap[i] = state;
      this.pos[state] = i;
    }
    bubbleUp(n) {
      const len = this.length;
      const state = this.heap[n];
      const score = this.fScores[state];
      let i = n;
      for (;;) {
        const c2 = (i + 1) << 1;
        const c1 = c2 - 1;
        let swap = -1;
        let bestScore = score;
        if (c1 < len) {
          const s1 = this.fScores[this.heap[c1]];
          if (s1 < bestScore) { swap = c1; bestScore = s1; }
        }
        if (c2 < len) {
          const s2 = this.fScores[this.heap[c2]];
          if (s2 < bestScore) swap = c2;
        }
        if (swap < 0) break;
        const childState = this.heap[swap];
        this.heap[i] = childState;
        this.pos[childState] = i;
        i = swap;
      }
      this.heap[i] = state;
      this.pos[state] = i;
    }
  }

  // 16-case orthogonal remaining bend estimator for A* heuristic
  function estimateSegmentCount(sx, sy, ex, ey, startHeading, endHeading) {
    if (endHeading === HEADING_RIGHT) {
      if (startHeading === HEADING_RIGHT) return sx >= ex ? 4 : sy === ey ? 0 : 2;
      if (startHeading === HEADING_UP) return sy > ey && sx < ex ? 1 : 3;
      if (startHeading === HEADING_DOWN) return sy < ey && sx < ex ? 1 : 3;
      return sy === ey ? 4 : 2;
    }
    if (endHeading === HEADING_LEFT) {
      if (startHeading === HEADING_RIGHT) return sy === ey ? 4 : 2;
      if (startHeading === HEADING_UP) return sy > ey && sx > ex ? 1 : 3;
      if (startHeading === HEADING_DOWN) return sy < ey && sx > ex ? 1 : 3;
      return sx <= ex ? 4 : sy === ey ? 0 : 2;
    }
    if (endHeading === HEADING_UP) {
      if (startHeading === HEADING_RIGHT) return sy > ey && sx < ex ? 1 : 3;
      if (startHeading === HEADING_UP) return sy >= ey ? 4 : sx === ex ? 0 : 2;
      if (startHeading === HEADING_DOWN) return sx === ex ? 4 : 2;
      return sy > ey && sx > ex ? 1 : 3;
    }
    if (startHeading === HEADING_RIGHT) return sy < ey && sx < ex ? 1 : 3;
    if (startHeading === HEADING_UP) return sx === ex ? 4 : 2;
    if (startHeading === HEADING_DOWN) return sy <= ey ? 4 : sx === ex ? 0 : 2;
    return sy < ey && sx > ex ? 1 : 3;
  }

  const pointInsideBoxStrict = (x, y, box) => x > box.x0 + EPS && x < box.x1 - EPS && y > box.y0 + EPS && y < box.y1 - EPS;

  function segIntersectsBoxStrict(ax, ay, bx, by, box) {
    if (Math.abs(ax - bx) < EPS) {
      return ax > box.x0 + EPS && ax < box.x1 - EPS && Math.min(Math.max(ay, by), box.y1) - Math.max(Math.min(ay, by), box.y0) > EPS;
    }
    if (Math.abs(ay - by) < EPS) {
      return ay > box.y0 + EPS && ay < box.y1 - EPS && Math.min(Math.max(ax, bx), box.x1) - Math.max(Math.min(ax, bx), box.x0) > EPS;
    }
    return false;
  }

  function pathIntersectsAnyBox(points, boxes) {
    for (let i = 0; i < points.length - 1; i += 1) {
      const p = points[i], q = points[i + 1];
      for (let b = 0; b < boxes.length; b += 1) {
        if (segIntersectsBoxStrict(p.x, p.y, q.x, q.y, boxes[b])) return boxes[b];
      }
    }
    return null;
  }

  function dedupeAndBoundAxis(rawValues, requiredValues, maxAxis) {
    const sorted = rawValues.filter(Number.isFinite).sort(compareNum);
    const deduped = [];
    for (let i = 0; i < sorted.length; i += 1) {
      const v = round2(sorted[i]);
      if (deduped.length === 0 || Math.abs(v - deduped[deduped.length - 1]) > DEDUP_THRESHOLD) deduped.push(v);
    }
    if (deduped.length <= maxAxis) return deduped;
    const reqSet = new Set([deduped[0], deduped[deduped.length - 1]]);
    for (const r of requiredValues) {
      if (!Number.isFinite(r)) continue;
      let best = deduped[0], bestD = Infinity;
      for (let i = 0; i < deduped.length; i += 1) {
        const d = Math.abs(deduped[i] - r);
        if (d < bestD) { bestD = d; best = deduped[i]; }
      }
      reqSet.add(best);
    }
    const optional = deduped.filter((v) => !reqSet.has(v));
    const room = Math.max(0, maxAxis - reqSet.size);
    const out = Array.from(reqSet);
    if (room > 0 && optional.length > 0) {
      const step = optional.length / room;
      for (let k = 0; k < room; k += 1) out.push(optional[Math.min(optional.length - 1, Math.floor(k * step))]);
    }
    return out.sort(compareNum);
  }

  function distToSegmentSq(px, py, ax, ay, bx, by) {
    const dx = bx - ax, dy = by - ay, lenSq = dx * dx + dy * dy;
    if (lenSq <= EPS) return (px - ax) * (px - ax) + (py - ay) * (py - ay);
    const t = clamp(((px - ax) * dx + (py - ay) * dy) / lenSq, 0, 1);
    const qx = ax + t * dx, qy = ay + t * dy;
    return (px - qx) * (px - qx) + (py - qy) * (py - qy);
  }

  function selectCorridorObstacles(allObs, s1, e1, extraIds) {
    const minX = Math.min(s1.x, e1.x) - CORRIDOR_MARGIN, maxX = Math.max(s1.x, e1.x) + CORRIDOR_MARGIN;
    const minY = Math.min(s1.y, e1.y) - CORRIDOR_MARGIN, maxY = Math.max(s1.y, e1.y) + CORRIDOR_MARGIN;
    const ranked = [];
    for (let i = 0; i < allObs.length; i += 1) {
      const b = allObs[i], forced = extraIds.has(b.id);
      if (!forced && (b.x1 < minX || b.x0 > maxX || b.y1 < minY || b.y0 > maxY)) continue;
      ranked.push({ b, d: forced ? -1 : distToSegmentSq(b.cx, b.cy, s1.x, s1.y, e1.x, e1.y) });
    }
    ranked.sort((u, v) => u.d - v.d);
    return ranked.slice(0, Math.max(MAX_CORRIDOR_OBSTACLES, extraIds.size + 4)).map((item) => item.b);
  }

  function findAxisIndex(arr, target) {
    let lo = 0, hi = arr.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1, d = arr[mid] - target;
      if (Math.abs(d) <= 0.06) return mid;
      if (d < 0) lo = mid + 1; else hi = mid - 1;
    }
    let best = 0, bestD = Infinity;
    for (let i = 0; i < arr.length; i += 1) {
      const d = Math.abs(arr[i] - target);
      if (d < bestD) { bestD = d; best = i; }
    }
    return best;
  }

  function precomputeBlockedEdges(xs, ys, blockBoxes) {
    const nx = xs.length, ny = ys.length, edgeMask = new Uint8Array(nx * ny);
    for (let iy = 0; iy < ny; iy += 1) {
      const y = ys[iy];
      for (let ix = 0; ix < nx; ix += 1) {
        const cell = iy * nx + ix;
        if (ix + 1 < nx) {
          const xA = xs[ix], xB = xs[ix + 1], midX = (xA + xB) * 0.5;
          for (let k = 0; k < blockBoxes.length; k += 1) {
            const box = blockBoxes[k];
            if (pointInsideBoxStrict(midX, y, box) || (y > box.y0 + EPS && y < box.y1 - EPS && Math.min(xB, box.x1) - Math.max(xA, box.x0) > EPS)) {
              edgeMask[cell] |= 1; break;
            }
          }
        }
        if (iy + 1 < ny) {
          const x = xs[ix], yA = ys[iy], yB = ys[iy + 1], midY = (yA + yB) * 0.5;
          for (let k = 0; k < blockBoxes.length; k += 1) {
            const box = blockBoxes[k];
            if (pointInsideBoxStrict(x, midY, box) || (x > box.x0 + EPS && x < box.x1 - EPS && Math.min(yB, box.y1) - Math.max(yA, box.y0) > EPS)) {
              edgeMask[cell] |= 2; break;
            }
          }
        }
      }
    }
    return edgeMask;
  }

  function runGridAStar(xs, ys, blockBoxes, fromPt, s1, e1, toPt, turnPenalty) {
    const nx = xs.length, ny = ys.length;
    const sIx = findAxisIndex(xs, s1.x), sIy = findAxisIndex(ys, s1.y);
    const eIx = findAxisIndex(xs, e1.x), eIy = findAxisIndex(ys, e1.y);
    const sx = xs[sIx], sy = ys[sIy], gx = xs[eIx], gy = ys[eIy];
    const edgeMask = precomputeBlockedEdges(xs, ys, blockBoxes);

    const startHeading = fromPt.heading;
    const endArrivalHeading = flipHeading(toPt.heading);
    const bendCost = Math.max(turnPenalty, mDist(sx, sy, gx, gy) * 0.25);

    const totalStates = nx * ny * 4;
    const gScore = new Float64Array(totalStates).fill(Infinity);
    const parent = new Int32Array(totalStates).fill(-1);
    const visited = new Uint8Array(totalStates);
    const closed = new Uint8Array(totalStates);
    const open = new BinaryStateHeap(totalStates);

    const startCell = sIy * nx + sIx, goalCell = eIy * nx + eIx;
    const startState = (startCell << 2) | startHeading;
    gScore[startState] = 0;
    visited[startState] = 1;
    open.push(startState, mDist(sx, sy, gx, gy) + estimateSegmentCount(sx, sy, gx, gy, startHeading, endArrivalHeading) * bendCost * 0.5);

    let bestGoalState = -1, bestGoalCost = Infinity;

    while (open.size() > 0) {
      const u = open.pop();
      if (u < 0 || closed[u]) continue;
      closed[u] = 1;

      const uHeading = u & 3, cell = u >> 2;
      const iy = (cell / nx) | 0, ix = cell - iy * nx;
      const ug = gScore[u];
      if (ug >= bestGoalCost) break;

      if (cell === goalCell) {
        if (uHeading === toPt.heading && cell !== startCell) continue;
        const endCost = ug + (uHeading === endArrivalHeading ? 0 : bendCost);
        if (endCost < bestGoalCost) { bestGoalCost = endCost; bestGoalState = u; }
        continue;
      }

      const reverseHeading = flipHeading(uHeading);
      for (let h = 0; h < 4; h += 1) {
        if (h === reverseHeading) continue;
        const nix = ix + STEP_DX[h], niy = iy + STEP_DY[h];
        if (nix < 0 || nix >= nx || niy < 0 || niy >= ny) continue;
        if (h === HEADING_RIGHT && (edgeMask[cell] & 1)) continue;
        if (h === HEADING_LEFT && (edgeMask[iy * nx + nix] & 1)) continue;
        if (h === HEADING_DOWN && (edgeMask[cell] & 2)) continue;
        if (h === HEADING_UP && (edgeMask[niy * nx + ix] & 2)) continue;

        const nCell = niy * nx + nix;
        if (nCell === startCell && h === flipHeading(startHeading)) continue;

        const v = (nCell << 2) | h;
        if (closed[v]) continue;

        const nxPos = xs[nix], nyPos = ys[niy];
        const ng = ug + mDist(nxPos, nyPos, xs[ix], ys[iy]) + (uHeading === h ? 0 : bendCost);
        if (!visited[v] || ng < gScore[v]) {
          gScore[v] = ng;
          parent[v] = u;
          const hVal = mDist(nxPos, nyPos, gx, gy) + estimateSegmentCount(nxPos, nyPos, gx, gy, h, endArrivalHeading) * bendCost * 0.5;
          if (!visited[v]) { visited[v] = 1; open.push(v, ng + hVal); }
          else open.rescoreElement(v, ng + hVal);
        }
      }
    }

    if (bestGoalState < 0) return null;
    const rev = [];
    for (let cur = bestGoalState; cur >= 0; cur = parent[cur]) {
      const c = cur >> 2, iy = (c / nx) | 0, ix = c - iy * nx;
      rev.push({ x: xs[ix], y: ys[iy] });
    }
    rev.reverse();
    return rev;
  }

  // Short-segment removal & collinear corner extraction (`removeElbowArrowShortSegments` + `getElbowArrowCornerPoints`)
  function compressOrthogonalPoints(rawPts) {
    const dedup = [];
    for (let i = 0; i < rawPts.length; i += 1) {
      const p = { x: round2(rawPts[i].x), y: round2(rawPts[i].y) };
      if (dedup.length === 0) { dedup.push(p); continue; }
      const prev = dedup[dedup.length - 1];
      if (Math.hypot(p.x - prev.x, p.y - prev.y) <= DEDUP_THRESHOLD && i < rawPts.length - 1) continue;
      dedup.push(p);
    }
    if (dedup.length <= 2) return dedup;
    const corners = [dedup[0]];
    let prevHorizontal = Math.abs(dedup[0].y - dedup[1].y) < Math.abs(dedup[0].x - dedup[1].x);
    for (let i = 1; i < dedup.length - 1; i += 1) {
      const cur = dedup[i], next = dedup[i + 1];
      const nextHorizontal = Math.abs(cur.y - next.y) < Math.abs(cur.x - next.x);
      if (prevHorizontal !== nextHorizontal) corners.push(cur);
      prevHorizontal = nextHorizontal;
    }
    corners.push(dedup[dedup.length - 1]);
    return corners;
  }

  // Smooth step corner bend builder (`getBend` quadratic `Q` corner rounding)
  function getBendSegment(a, b, c, radius) {
    const d1 = Math.hypot(b.x - a.x, b.y - a.y), d2 = Math.hypot(c.x - b.x, c.y - b.y);
    const bendSize = round2(Math.min(d1 * 0.5, d2 * 0.5, radius));
    if (bendSize <= 0.05 || (Math.abs(a.x - b.x) < DEDUP_THRESHOLD && Math.abs(b.x - c.x) < DEDUP_THRESHOLD) || (Math.abs(a.y - b.y) < DEDUP_THRESHOLD && Math.abs(b.y - c.y) < DEDUP_THRESHOLD)) {
      return ` L ${round2(b.x)} ${round2(b.y)}`;
    }
    if (Math.abs(a.y - b.y) < DEDUP_THRESHOLD) {
      const xDir = a.x < c.x ? -1 : 1, yDir = a.y < c.y ? 1 : -1;
      return ` L ${round2(b.x + bendSize * xDir)} ${round2(b.y)} Q ${round2(b.x)} ${round2(b.y)} ${round2(b.x)} ${round2(b.y + bendSize * yDir)}`;
    }
    const xDir = a.x < c.x ? 1 : -1, yDir = a.y < c.y ? -1 : 1;
    return ` L ${round2(b.x)} ${round2(b.y + bendSize * yDir)} Q ${round2(b.x)} ${round2(b.y)} ${round2(b.x + bendSize * xDir)} ${round2(b.y)}`;
  }

  function buildSmoothStepPathD(points, borderRadius = DEFAULT_BEND_RADIUS) {
    if (!points.length) return "";
    let d = `M ${round2(points[0].x)} ${round2(points[0].y)}`;
    for (let i = 1; i < points.length - 1; i += 1) d += getBendSegment(points[i - 1], points[i], points[i + 1], borderRadius);
    if (points.length > 1) d += ` L ${round2(points[points.length - 1].x)} ${round2(points[points.length - 1].y)}`;
    return d;
  }

  // Simple 1-corner or 2-corner unobstructed route (`getSimpleRoute`)
  function getSimpleRoute(fromPt, s1, e1, toPt, aBox, bBox) {
    const pts = [{ x: fromPt.x, y: fromPt.y }, s1];
    const fromHoriz = Math.abs(fromPt.nx) >= 0.5, toHoriz = Math.abs(toPt.nx) >= 0.5;
    if (fromHoriz !== toHoriz) {
      const corner = fromHoriz ? { x: e1.x, y: s1.y } : { x: s1.x, y: e1.y };
      const aheadStart = (corner.x - fromPt.x) * fromPt.nx + (corner.y - fromPt.y) * fromPt.ny;
      const aheadEnd = (corner.x - toPt.x) * toPt.nx + (corner.y - toPt.y) * toPt.ny;
      if (aheadStart > 2 && aheadEnd > 2) pts.push(corner);
      else pts.push({ x: round2((s1.x + e1.x) * 0.5), y: s1.y }, { x: round2((s1.x + e1.x) * 0.5), y: e1.y });
    } else if (fromHoriz) {
      const midX = round2((s1.x + e1.x) * 0.5);
      pts.push({ x: midX, y: s1.y }, { x: midX, y: e1.y });
    } else {
      const midY = round2((s1.y + e1.y) * 0.5);
      pts.push({ x: s1.x, y: midY }, { x: e1.x, y: midY });
    }
    pts.push(e1, { x: toPt.x, y: toPt.y });
    const clean = compressOrthogonalPoints(pts);
    for (let i = 0; i < clean.length - 1; i += 1) {
      const p = clean[i], q = clean[i + 1];
      if (i > 0 && segIntersectsBoxStrict(p.x, p.y, q.x, q.y, aBox)) return { pts: clean, selfHit: true };
      if (i < clean.length - 2 && segIntersectsBoxStrict(p.x, p.y, q.x, q.y, bBox)) return { pts: clean, selfHit: true };
    }
    return { pts: clean, selfHit: false };
  }

  function buildOrthoResult(points, fromPort, toPort, fromPt, toPt, rawObs, cornerRadius) {
    let totalLength = 0, bestSegLen = -1;
    let labelPos = { x: round2((fromPt.x + toPt.x) * 0.5), y: round2((fromPt.y + toPt.y) * 0.5) };
    for (let i = 0; i < points.length - 1; i += 1) {
      const a = points[i], b = points[i + 1], len = mDist(a.x, a.y, b.x, b.y);
      totalLength += len;
      if (len > bestSegLen) { bestSegLen = len; labelPos = { x: round2((a.x + b.x) * 0.5), y: round2((a.y + b.y) * 0.5) }; }
    }
    const sharpPathD = points.map((p, idx) => (idx === 0 ? "M " : "L ") + round2(p.x) + " " + round2(p.y)).join(" ");
    const smoothPathD = buildSmoothStepPathD(points, cornerRadius > 0 ? cornerRadius : DEFAULT_BEND_RADIUS);
    const hitRaw = pathIntersectsAnyBox(points, rawObs);
    return {
      routing: "orthogonal",
      fromPort,
      toPort,
      from: fromPt,
      to: toPt,
      points,
      pathD: cornerRadius > 0 ? smoothPathD : sharpPathD,
      smoothPathD,
      bendCount: Math.max(0, points.length - 2),
      intersectsObstacle: hitRaw !== null,
      labelPos,
      totalLength: round2(totalLength),
    };
  }

  function buildGridAxes(s1, e1, fromPt, toPt, aBox, bBox, corridorObs, pad, maxAxis) {
    const xs = [fromPt.x, s1.x, (s1.x + e1.x) * 0.5, e1.x, toPt.x, aBox.x0 - pad, aBox.x1 + pad, bBox.x0 - pad, bBox.x1 + pad];
    const ys = [fromPt.y, s1.y, (s1.y + e1.y) * 0.5, e1.y, toPt.y, aBox.y0 - pad, aBox.y1 + pad, bBox.y0 - pad, bBox.y1 + pad];
    let minX = Math.min(fromPt.x, s1.x, e1.x, toPt.x, aBox.x0, bBox.x0);
    let maxX = Math.max(fromPt.x, s1.x, e1.x, toPt.x, aBox.x1, bBox.x1);
    let minY = Math.min(fromPt.y, s1.y, e1.y, toPt.y, aBox.y0, bBox.y0);
    let maxY = Math.max(fromPt.y, s1.y, e1.y, toPt.y, aBox.y1, bBox.y1);
    for (let i = 0; i < corridorObs.length; i += 1) {
      const b = corridorObs[i], lx = b.x0 - pad, rx = b.x1 + pad, ty = b.y0 - pad, by = b.y1 + pad;
      xs.push(lx, rx); ys.push(ty, by);
      if (lx < minX) minX = lx; if (rx > maxX) maxX = rx;
      if (ty < minY) minY = ty; if (by > maxY) maxY = by;
    }
    const outer = pad * 2;
    xs.push(minX - outer, maxX + outer);
    ys.push(minY - outer, maxY + outer);
    return {
      xs: dedupeAndBoundAxis(xs, [s1.x, e1.x, minX - outer, maxX + outer], maxAxis),
      ys: dedupeAndBoundAxis(ys, [s1.y, e1.y, minY - outer, maxY + outer], maxAxis),
    };
  }

  function routeOrthogonal(fromNode, toNode, options = {}) {
    const stubPref = Number.isFinite(Number(options.stub)) ? Number(options.stub) : STUB;
    const pad = Number.isFinite(Number(options.pad)) ? Number(options.pad) : PAD;
    const turnPenalty = Number.isFinite(Number(options.turnPenalty)) ? Number(options.turnPenalty) : TURN_PENALTY;
    const cornerRadius = Number.isFinite(Number(options.cornerRadius)) ? Number(options.cornerRadius) : 0;
    const maxAxis = clamp(Math.trunc(Number(options.maxAxis) || MAX_GRID_AXIS), 12, MAX_GRID_AXIS);
    const { fromPort, toPort, from, to } = resolveAnchorPorts(fromNode, toNode, options.fromPort, options.toPort);

    const aBox = boxOf(fromNode), bBox = boxOf(toNode);
    const dist = Math.hypot(to.x - from.x, to.y - from.y);
    const stub = dist < stubPref * 2.2 ? Math.max(4, round2(dist * 0.35)) : stubPref;
    const s1 = { x: round2(from.x + from.nx * stub), y: round2(from.y + from.ny * stub) };
    const e1 = { x: round2(to.x + to.nx * stub), y: round2(to.y + to.ny * stub) };

    const rawList = Array.isArray(options.obstacles) ? options.obstacles : [];
    const thirdParty = [];
    for (let i = 0; i < rawList.length; i += 1) {
      const b = boxOf(rawList[i]);
      if (b.id && (b.id === aBox.id || b.id === bBox.id)) continue;
      if (Math.abs(b.x0 - aBox.x0) < EPS && Math.abs(b.y0 - aBox.y0) < EPS && Math.abs(b.w - aBox.w) < EPS) continue;
      if (Math.abs(b.x0 - bBox.x0) < EPS && Math.abs(b.y0 - bBox.y0) < EPS && Math.abs(b.w - bBox.w) < EPS) continue;
      thirdParty.push(b);
    }

    const simple = getSimpleRoute(from, s1, e1, to, aBox, bBox);
    const directPts = simple.pts;
    const directBlocked = pathIntersectsAnyBox(directPts, thirdParty) !== null;
    if (!directBlocked && !simple.selfHit) {
      const res = buildOrthoResult(directPts, fromPort, toPort, from, to, thirdParty, cornerRadius);
      res.avoidedObstacles = false;
      return res;
    }

    const extraIds = new Set();
    let finalPts = null;
    for (let pass = 0; pass < 2; pass += 1) {
      const corridorObs = selectCorridorObstacles(thirdParty, s1, e1, extraIds);
      const paddedObs = corridorObs.map((b) => {
        const inf = { id: b.id, x0: b.x0 - pad, y0: b.y0 - pad, x1: b.x1 + pad, y1: b.y1 + pad };
        return pointInsideBoxStrict(s1.x, s1.y, inf) || pointInsideBoxStrict(e1.x, e1.y, inf) ? b : inf;
      });
      const { xs, ys } = buildGridAxes(s1, e1, from, to, aBox, bBox, corridorObs, pad, maxAxis);
      let gridPts = runGridAStar(xs, ys, [aBox, bBox, ...paddedObs], from, s1, e1, to, turnPenalty);
      if (!gridPts) gridPts = runGridAStar(xs, ys, [aBox, bBox, ...corridorObs], from, s1, e1, to, turnPenalty);
      if (!gridPts) break;
      const cand = compressOrthogonalPoints([{ x: from.x, y: from.y }, s1, ...gridPts, e1, { x: to.x, y: to.y }]);
      const hit = pathIntersectsAnyBox(cand, thirdParty);
      finalPts = cand;
      if (!hit || extraIds.has(hit.id)) break;
      extraIds.add(hit.id);
    }

    if (!finalPts) finalPts = directPts;
    const result = buildOrthoResult(finalPts, fromPort, toPort, from, to, thirdParty, cornerRadius);
    result.avoidedObstacles = directBlocked && !result.intersectsObstacle;
    return result;
  }

  // Curvature-aware cubic Bezier control offset (`calculateControlOffset` + `getControlWithCurvature` + `getBezierEdgeCenter`)
  function calculateControlOffset(distance, curvature) {
    const raw = distance >= 0 ? 0.5 * distance : curvature * 25 * Math.sqrt(-distance);
    return clamp(raw, 24, 240);
  }

  function getControlWithCurvature(port, x1, y1, x2, y2, c) {
    if (port === "left") return { x: round2(x1 - calculateControlOffset(x1 - x2, c)), y: round2(y1) };
    if (port === "right") return { x: round2(x1 + calculateControlOffset(x2 - x1, c)), y: round2(y1) };
    if (port === "top") return { x: round2(x1), y: round2(y1 - calculateControlOffset(y1 - y2, c)) };
    return { x: round2(x1), y: round2(y1 + calculateControlOffset(y2 - y1, c)) };
  }

  function getBezierEdgeCenter(sx, sy, tx, ty, scx, scy, tcx, tcy) {
    return {
      x: round2(sx * 0.125 + scx * 0.375 + tcx * 0.375 + tx * 0.125),
      y: round2(sy * 0.125 + scy * 0.375 + tcy * 0.375 + ty * 0.125),
    };
  }

  function routeCurved(fromNode, toNode, options = {}) {
    const { fromPort, toPort, from, to } = resolveAnchorPorts(fromNode, toNode, options.fromPort, options.toPort);
    const curvature = Number.isFinite(Number(options.curvature)) ? clamp(Number(options.curvature), 0.05, 1) : DEFAULT_CURVATURE;
    const c1 = getControlWithCurvature(fromPort, from.x, from.y, to.x, to.y, curvature);
    const c2 = getControlWithCurvature(toPort, to.x, to.y, from.x, from.y, curvature);
    const pathD =
      "M " + round2(from.x) + " " + round2(from.y) +
      " C " + c1.x + " " + c1.y + ", " + c2.x + " " + c2.y + ", " + round2(to.x) + " " + round2(to.y);
    const labelPos = getBezierEdgeCenter(from.x, from.y, to.x, to.y, c1.x, c1.y, c2.x, c2.y);
    return {
      routing: "curved",
      fromPort,
      toPort,
      from,
      to,
      points: [{ x: from.x, y: from.y }, { x: to.x, y: to.y }],
      controlPoints: [c1, c2],
      pathD,
      labelPos,
    };
  }

  function routeConnector(conn, nodeById, allNodes, options = {}) {
    const fromNode = nodeById instanceof Map ? nodeById.get(conn.fromId) : nodeById?.[conn.fromId];
    const toNode = nodeById instanceof Map ? nodeById.get(conn.toId) : nodeById?.[conn.toId];
    if (!fromNode || !toNode) return null;
    const opts = { ...options, obstacles: allNodes, fromPort: conn.fromPort || "auto", toPort: conn.toPort || "auto" };
    return conn.routing === "curved" ? routeCurved(fromNode, toNode, opts) : routeOrthogonal(fromNode, toNode, opts);
  }

  const nodeDisplayLabel = (node) => String(node?.text || node?.title || "").trim() || String(node?.id || "");

  function analyzeBoard(state) {
    const nodes = Array.isArray(state?.nodes) ? state.nodes : [];
    const connectors = Array.isArray(state?.connectors) ? state.connectors : [];
    const vCount = nodes.length;
    const idToIndex = new Map();

    let shapeCount = 0, textCount = 0, imageCount = 0, chartCount = 0;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;

    for (let i = 0; i < vCount; i += 1) {
      const n = nodes[i];
      idToIndex.set(String(n.id), i);
      if (n.kind === "shape") shapeCount += 1;
      else if (n.kind === "text" || n.kind === "textbox") textCount += 1;
      else if (n.kind === "image") imageCount += 1;
      else if (n.kind === "chart") chartCount += 1;
      const x0 = Number(n.x) || 0, y0 = Number(n.y) || 0;
      const x1 = x0 + Math.max(0, Number(n.w) || 0), y1 = y0 + Math.max(0, Number(n.h) || 0);
      if (x0 < minX) minX = x0; if (y0 < minY) minY = y0;
      if (x1 > maxX) maxX = x1; if (y1 > maxY) maxY = y1;
    }

    const bounds = vCount === 0
      ? { minX: 0, minY: 0, maxX: 0, maxY: 0, width: 0, height: 0 }
      : { minX: round2(minX), minY: round2(minY), maxX: round2(maxX), maxY: round2(maxY), width: round2(maxX - minX), height: round2(maxY - minY) };

    const inDeg = new Uint32Array(vCount), outDeg = new Uint32Array(vCount);
    const outAdj = Array.from({ length: vCount }, () => []);
    const undirAdj = Array.from({ length: vCount }, () => []);

    let connectorCount = 0, orthogonalConnectorCount = 0, curvedConnectorCount = 0, hasCycle = false;

    for (let i = 0; i < connectors.length; i += 1) {
      const c = connectors[i];
      const u = idToIndex.get(String(c?.fromId ?? "")), v = idToIndex.get(String(c?.toId ?? ""));
      if (u === undefined || v === undefined) continue;
      connectorCount += 1;
      if (c.routing === "curved") curvedConnectorCount += 1;
      else orthogonalConnectorCount += 1;
      outDeg[u] += 1; inDeg[v] += 1;
      outAdj[u].push(v); undirAdj[u].push(v);
      if (u !== v) undirAdj[v].push(u); else hasCycle = true;
    }

    if (!hasCycle && vCount > 0) {
      const color = new Uint8Array(vCount), stackNode = new Int32Array(vCount), stackPos = new Int32Array(vCount);
      for (let start = 0; start < vCount && !hasCycle; start += 1) {
        if (color[start] !== 0) continue;
        let top = 0;
        stackNode[0] = start; stackPos[0] = 0; color[start] = 1;
        while (top >= 0) {
          const u = stackNode[top], adj = outAdj[u], pos = stackPos[top];
          if (pos < adj.length) {
            stackPos[top] = pos + 1;
            const v = adj[pos];
            if (color[v] === 1) { hasCycle = true; break; }
            if (color[v] === 0) { color[v] = 1; top += 1; stackNode[top] = v; stackPos[top] = 0; }
          } else { color[u] = 2; top -= 1; }
        }
      }
    }

    let connectedComponents = 0;
    if (vCount > 0) {
      const visited = new Uint8Array(vCount), queue = new Int32Array(vCount);
      for (let start = 0; start < vCount; start += 1) {
        if (visited[start]) continue;
        connectedComponents += 1; visited[start] = 1;
        let head = 0, tail = 0;
        queue[tail++] = start;
        while (head < tail) {
          const adj = undirAdj[queue[head++]];
          for (let j = 0; j < adj.length; j += 1) {
            const v = adj[j];
            if (!visited[v]) { visited[v] = 1; queue[tail++] = v; }
          }
        }
      }
    }

    const startNodes = [], endNodes = [], isolatedNodes = [];
    let topHub = null;
    for (let i = 0; i < vCount; i += 1) {
      const n = nodes[i], degIn = inDeg[i], degOut = outDeg[i], deg = degIn + degOut;
      const entry = { id: String(n.id), label: nodeDisplayLabel(n) };
      if (degIn === 0 && degOut > 0) startNodes.push(entry);
      else if (degOut === 0 && degIn > 0) endNodes.push(entry);
      else if (deg === 0) isolatedNodes.push(entry);
      if (deg > 0 && (!topHub || deg > topHub.degree)) {
        topHub = { id: entry.id, label: entry.label, degree: deg, inDegree: degIn, outDegree: degOut };
      }
    }

    return {
      totalNodes: vCount, shapeCount, textCount, imageCount, chartCount,
      connectorCount, orthogonalConnectorCount, curvedConnectorCount,
      connectedComponents, startNodes, endNodes, isolatedNodes, hasCycle, topHub, bounds,
    };
  }

  host.__EASYOFFICE_BOARD_ROUTER__ = {
    STUB, PAD, TURN_PENALTY, MAX_GRID_AXIS,
    resolveAnchorPorts, routeOrthogonal, routeCurved, routeConnector, buildSmoothStepPathD, analyzeBoard,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
