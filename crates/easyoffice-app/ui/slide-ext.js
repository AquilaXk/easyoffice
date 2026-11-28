(function (host) {
  "use strict";

  // Slide-level state of a slide (PD-394): the hidden flag, background fill, header/footer fields,
  // groups and animations. They sit on the slide object next to `title`, `notes` and `elements`
  // (`hidden`, `background`, `footer`, `groups`, `anims`), under the names the wire format uses.
  // This module normalizes and prunes them, turns them to and from the wire form and paints the
  // background and footer fields onto a stage or a show frame.
  const CLASS_EFFECTS = Object.freeze({
    entrance: ["appear", "fade", "fly", "wipe", "zoom"],
    emphasis: ["spin", "growShrink"],
    exit: ["appear", "fade", "fly", "wipe", "zoom"],
  });
  const STARTS = new Set(["onClick", "withPrevious", "afterPrevious"]);
  const DIRECTIONS = new Set(["left", "right", "top", "bottom"]);
  const MAX_ANIM_MS = 60000;
  const DEFAULT_ANIM_MS = 500;
  const MAX_FOOTER_BYTES = 1024; // the file engine's limit, counted in UTF-8 bytes
  const MAX_GROUPS = 64;
  const MAX_NESTING = 8;
  const encoder = new TextEncoder();
  // The longest start of `text` within `limit` UTF-8 bytes, cut between characters.
  function clipBytes(text, limit) {
    let out = "", used = 0;
    for (const ch of String(text)) {
      used += encoder.encode(ch).length;
      if (used > limit) break;
      out += ch;
    }
    return out;
  }
  const FOOTER_SLOTS = Object.freeze({
    date: { left: 6.875, width: 22.5, align: "left" },
    text: { left: 33.125, width: 33.75, align: "center" },
    number: { left: 70.625, width: 22.5, align: "right" },
  });
  const FOOTER_TOP_PCT = 92.69;
  const FOOTER_HEIGHT_PCT = 5.32;
  const FOOTER_PT = 12;
  const round2 = (v) => Math.round(v * 100) / 100;
  const hex6 = (raw, fallback) => host.__EASYOFFICE_SLIDE_FX__.hex6(raw, fallback);
  const asMs = (v, fallback) => {
    const n = Number(v);
    return v !== null && v !== "" && Number.isFinite(n) ? Math.max(0, Math.min(MAX_ANIM_MS, Math.round(n))) : fallback;
  };

  function registerPicture(raw) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const b64 = typeof raw.dataBase64 === "string" && raw.dataBase64 ? raw.dataBase64 : (typeof raw.data === "string" ? raw.data : "");
    if (raw.imageId) return { kind: "picture", imageId: String(raw.imageId), mime: raw.mime === "image/jpeg" ? "image/jpeg" : "image/png" };
    if (!b64 || !utils?.registerDocImage) return null;
    const jpeg = raw.mime === "image/jpeg";
    return { kind: "picture", imageId: utils.registerDocImage(b64, jpeg ? "jpg" : "png"), mime: jpeg ? "image/jpeg" : "image/png" };
  }

  function normalizeBackground(raw) {
    if (!raw || typeof raw !== "object") return null;
    if (raw.kind === "solid") return { kind: "solid", color: hex6(raw.color, "FFFFFF") };
    if (raw.kind === "gradient") {
      const angle = ((Math.round(Number(raw.angleDeg) || 0) % 360) + 360) % 360;
      return { kind: "gradient", from: hex6(raw.from, "FFFFFF"), to: hex6(raw.to, "CCCCCC"), angleDeg: angle };
    }
    return raw.kind === "picture" ? registerPicture(raw) : null;
  }

  function normalizeFooter(raw) {
    if (!raw || typeof raw !== "object") return null;
    const out = {};
    if (raw.number === true) out.number = true;
    if (raw.date?.kind === "auto") out.date = { kind: "auto" };
    else if (raw.date?.kind === "fixed" && String(raw.date.text ?? "") !== "") out.date = { kind: "fixed", text: clipBytes(raw.date.text, MAX_FOOTER_BYTES) };
    if (typeof raw.text === "string" && raw.text !== "") out.text = clipBytes(raw.text, MAX_FOOTER_BYTES);
    return Object.keys(out).length > 0 ? out : null;
  }

  function normalizeAnims(raw) {
    return (Array.isArray(raw) ? raw : []).slice(0, 256).flatMap((a) => {
      if (!a || typeof a.target !== "string" || !CLASS_EFFECTS[a.class]?.includes(a.effect)) return [];
      return [{
        target: a.target, class: a.class, effect: a.effect, start: STARTS.has(a.start) ? a.start : "onClick",
        durationMs: asMs(a.durationMs, DEFAULT_ANIM_MS), delayMs: asMs(a.delayMs, 0), direction: DIRECTIONS.has(a.direction) ? a.direction : "bottom",
      }];
    });
  }

  function normalizeGroups(raw) {
    return (Array.isArray(raw) ? raw : []).slice(0, 64).flatMap((g) => {
      const members = Array.isArray(g?.members) ? g.members.filter((m) => typeof m === "string") : [];
      return typeof g?.id === "string" && g.id !== "" && members.length >= 2 ? [{ id: g.id, members }] : [];
    });
  }

  // `id` opened into its members in every group that holds it.
  function dissolve(live, id) {
    const gone = live.find((g) => g.id === id);
    if (!gone) return live;
    return live.filter((g) => g !== gone).map((g) => ({ id: g.id, members: g.members.flatMap((m) => (m === id ? gone.members : [m])) }));
  }

  // Groups past the count or nesting limit (or in a cycle) are opened into their members, as the file
  // readers do, so a slide from a file or a payload never keeps a chain the walkers would not stop on.
  function flattenExcess(groups) {
    let live = groups;
    while (live.length > MAX_GROUPS) live = dissolve(live, live.at(-1).id);
    for (let pass = 0; pass <= groups.length; pass += 1) {
      const parent = new Map(live.flatMap((g) => g.members.map((m) => [m, g.id])));
      let offender = null;
      for (const g of live) {
        let cur = g.id, depth = 0;
        while (parent.has(cur) && depth <= MAX_NESTING + live.length && !(depth > 0 && cur === g.id)) { cur = parent.get(cur); depth += 1; }
        if ((depth > 0 && cur === g.id) || depth > MAX_NESTING) { offender = cur; break; }
      }
      if (!offender) return live;
      live = dissolve(live, offender);
    }
    return live;
  }

  // Groups and animations that point at elements or groups the slide no longer has: a group that falls
  // below two members dissolves, and what pointed at it points at its remaining member.
  function pruneRefs(groups, anims, elementIds) {
    let live = flattenExcess(groups.map((g) => ({ id: g.id, members: [...g.members] })));
    for (let changed = true; changed;) {
      changed = false;
      const known = new Set([...elementIds, ...live.map((g) => g.id)]);
      for (const g of live) {
        const members = [...new Set(g.members.filter((m) => known.has(m) && m !== g.id))];
        if (members.length !== g.members.length) { g.members = members; changed = true; }
      }
      const dead = live.find((g) => g.members.length < 2);
      if (dead) {
        const heir = dead.members[0] || "";
        live = live.filter((g) => g !== dead);
        for (const g of live) g.members = g.members.map((m) => (m === dead.id ? heir : m)).filter(Boolean);
        for (const a of anims) if (a.target === dead.id) a.target = heir;
        changed = true;
      }
    }
    const known = new Set([...elementIds, ...live.map((g) => g.id)]);
    return { groups: live, anims: anims.filter((a) => known.has(a.target)) };
  }

  function normalize(raw, elementIds) {
    const { groups, anims } = pruneRefs(normalizeGroups(raw?.groups), normalizeAnims(raw?.anims), elementIds);
    const out = {};
    if (raw?.hidden === true) out.hidden = true;
    const background = normalizeBackground(raw?.background);
    if (background) out.background = background;
    const footer = normalizeFooter(raw?.footer);
    if (footer) out.footer = footer;
    if (groups.length > 0) out.groups = groups;
    if (anims.length > 0) out.anims = anims;
    return out;
  }

  const idsOf = (elements) => new Set((elements || []).map((e) => e.id));

  // Slide-level fields for the state of a slide read from a file.
  const hydrate = (raw, elements) => ({ hidden: false, background: null, footer: null, groups: [], anims: [], ...normalize(raw, idsOf(elements)) });

  // Slide-level fields of a slide for the save payload.
  function wire(slide, wireElements) {
    const out = normalize(slide, idsOf(wireElements));
    if (out.background?.kind === "picture") {
      const data = host.__EASYOFFICE_SHEET_UTILS__?.getDocImage?.(out.background.imageId)?.base64;
      if (data) out.background = { kind: "picture", mime: out.background.mime, data };
      else delete out.background;
    }
    return out;
  }

  // Drops references to elements the slide no longer has; called after elements are removed.
  function prune(slide) {
    const { groups, anims } = pruneRefs(slide.groups || [], slide.anims || [], idsOf(slide.elements));
    slide.groups = groups;
    slide.anims = anims;
  }

  function backgroundCss(bg) {
    if (!bg) return "";
    if (bg.kind === "solid") return "#" + bg.color;
    if (bg.kind === "gradient") return `linear-gradient(${(bg.angleDeg + 90) % 360}deg, #${bg.from}, #${bg.to})`;
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const url = utils?.docImageUrl?.(bg.imageId) || (utils?.getDocImage?.(bg.imageId)?.base64 ? `data:${bg.mime};base64,${utils.getDocImage(bg.imageId).base64}` : "");
    return url ? `url("${url}") center / 100% 100% no-repeat` : "";
  }

  function todayText(now = new Date()) {
    const p = (n) => String(n).padStart(2, "0");
    return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
  }

  // The footer fields of a slide as text, in date, text, number order.
  function footerParts(slide, index) {
    const f = slide?.footer;
    if (!f) return [];
    const parts = [];
    if (f.date) parts.push(["date", f.date.kind === "auto" ? todayText() : f.date.text]);
    if (f.text) parts.push(["text", f.text]);
    if (f.number) parts.push(["number", String(index + 1)]);
    return parts;
  }

  function footerLayer(document, slide, index, stage) {
    const layer = document.createElement("div");
    layer.className = "slide-footer-layer";
    layer.setAttribute("aria-hidden", "true");
    for (const [slot, text] of footerParts(slide, index)) {
      const spec = FOOTER_SLOTS[slot];
      const item = document.createElement("div");
      item.className = "slide-footer-item";
      item.dataset.slot = slot;
      item.textContent = text;
      item.style.left = spec.left + "%";
      item.style.width = spec.width + "%";
      item.style.top = FOOTER_TOP_PCT + "%";
      item.style.height = FOOTER_HEIGHT_PCT + "%";
      item.style.justifyContent = spec.align === "left" ? "flex-start" : spec.align === "right" ? "flex-end" : "center";
      item.style.fontSize = round2((FOOTER_PT / stage.w) * 100) + "cqw";
      layer.appendChild(item);
    }
    return layer;
  }

  // Background and footer fields of a slide on a stage box or a show frame.
  function paintStage(document, node, slide, index, stage) {
    node.style.background = backgroundCss(slide?.background);
    node.querySelector(":scope > .slide-footer-layer")?.remove();
    if (slide?.footer) node.appendChild(footerLayer(document, slide, index, stage));
  }

  host.__EASYOFFICE_SLIDE_EXT__ = {
    CLASS_EFFECTS, clipBytes, normalize, hydrate, wire, prune, pruneRefs, paintStage, footerParts, backgroundCss, todayText,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
