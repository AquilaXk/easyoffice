(function (host) {
  "use strict";

  // Groups, rotation and flips on the elements of a slide (PD-394). A group is `{ id, members }` in
  // `slide.groups`; its members are element ids or ids of other groups. Elements keep their own
  // coordinates and a group has no frame of its own: it moves, turns and flips its members. All
  // functions work on slide objects, so they run the same in the editor and in tests.
  const round2 = (v) => Math.round(v * 100) / 100;
  const norm360 = (deg) => round2(((deg % 360) + 360) % 360) % 360;
  const groupsOf = (slide) => (Array.isArray(slide?.groups) ? slide.groups : []);
  const groupById = (slide, id) => groupsOf(slide).find((g) => g.id === id) || null;
  const parentOf = (slide, id) => groupsOf(slide).find((g) => g.members.includes(id)) || null;

  function topOf(slide, id) {
    let cur = id;
    for (let p = parentOf(slide, cur); p; p = parentOf(slide, cur)) cur = p.id;
    return cur;
  }

  // Element ids an id stands for: itself, or every element of a group.
  function leavesOf(slide, id) {
    const g = groupById(slide, id);
    return g ? g.members.flatMap((m) => leavesOf(slide, m)) : [id];
  }

  // Everything that moves with `id`: all elements of the outermost group it is in.
  const expand = (slide, id) => leavesOf(slide, topOf(slide, id));

  function nextGroupId(slide) {
    const used = new Set([...(slide.elements || []).map((e) => e.id), ...groupsOf(slide).map((g) => g.id)]);
    let n = groupsOf(slide).length + 1;
    while (used.has("grp-" + n)) n += 1;
    return "grp-" + n;
  }

  // Ties the outermost groups or elements of `ids` into one group and gathers its elements in the draw
  // order at the place of the topmost one. Returns the new group id, or null when fewer than two remain.
  function group(slide, ids) {
    const units = [...new Set(ids.map((id) => topOf(slide, id)))];
    if (units.length < 2) return null;
    const gid = nextGroupId(slide);
    slide.groups = [...groupsOf(slide), { id: gid, members: units }];
    const leaves = new Set(units.flatMap((u) => leavesOf(slide, u)));
    const els = slide.elements;
    const last = els.reduce((at, e, i) => (leaves.has(e.id) ? i : at), -1);
    const picked = els.filter((e) => leaves.has(e.id));
    slide.elements = els.flatMap((e, i) => (i === last ? picked : leaves.has(e.id) ? [] : [e]));
    return gid;
  }

  // Dissolves the outermost group of each id; returns the ids of the elements that were in them.
  function ungroup(slide, ids) {
    const freed = new Set();
    for (const id of ids) {
      const top = topOf(slide, id);
      if (!groupById(slide, top)) continue;
      leavesOf(slide, top).forEach((l) => freed.add(l));
      slide.groups = groupsOf(slide).filter((g) => g.id !== top);
    }
    return [...freed];
  }

  function rotatedBounds(el) {
    const rad = ((el.rot || 0) * Math.PI) / 180;
    const s = Math.abs(Math.sin(rad)), c = Math.abs(Math.cos(rad));
    const w = el.w * c + el.h * s, h = el.w * s + el.h * c;
    return { x: el.x + (el.w - w) / 2, y: el.y + (el.h - h) / 2, w, h };
  }

  function unionBounds(els) {
    const bs = els.map(rotatedBounds);
    const x = Math.min(...bs.map((b) => b.x)), y = Math.min(...bs.map((b) => b.y));
    return { x, y, w: Math.max(...bs.map((b) => b.x + b.w)) - x, h: Math.max(...bs.map((b) => b.y + b.h)) - y };
  }

  const centerOf = (els) => {
    const b = unionBounds(els);
    return { x: b.x + b.w / 2, y: b.y + b.h / 2 };
  };

  // The state a turn or flip starts from, kept so a drag can be recomputed from the same start.
  const snapshot = (els) => els.map((el) => ({ el, cx: el.x + el.w / 2, cy: el.y + el.h / 2, rot: el.rot || 0, flipH: Boolean(el.flipH), flipV: Boolean(el.flipV) }));

  // Turns the snapshot's elements by `angle` degrees clockwise about `center`, after an optional mirror
  // across the vertical (`flipH`) or horizontal (`flipV`) axis through it. A mirror negates the
  // element's own rotation and toggles its flip flag; a half turn needs no such change.
  function transformAbout(snaps, center, { angle = 0, flipH = false, flipV = false } = {}) {
    const rad = (angle * Math.PI) / 180;
    const sin = Math.sin(rad), cos = Math.cos(rad);
    for (const s of snaps) {
      let dx = s.cx - center.x, dy = s.cy - center.y, rot = s.rot, fh = s.flipH, fv = s.flipV;
      if (flipH) { dx = -dx; fh = !fh; rot = -rot; }
      if (flipV) { dy = -dy; fv = !fv; rot = -rot; }
      const el = s.el;
      el.x = round2(center.x + dx * cos - dy * sin - el.w / 2);
      el.y = round2(center.y + dx * sin + dy * cos - el.h / 2);
      const turned = norm360(rot + angle);
      for (const [key, value] of [["rot", turned || null], ["flipH", fh || null], ["flipV", fv || null]]) {
        if (value) el[key] = value;
        else delete el[key];
      }
    }
  }

  // One-shot turn or flip of `els` about their common center.
  function transform(els, change) {
    if (els.length === 0) return;
    transformAbout(snapshot(els), centerOf(els), change);
  }

  host.__EASYOFFICE_SLIDE_GROUP__ = {
    groupById, parentOf, topOf, leavesOf, expand, group, ungroup, nextGroupId,
    rotatedBounds, unionBounds, centerOf, snapshot, transformAbout, transform, norm360,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
