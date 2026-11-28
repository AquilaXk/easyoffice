// 참조 구현 (reference implementation): PptxGenJS (MIT, Copyright (c) 2015-present Brent Ely) `src/gen-xml.ts`
// for the unit conventions of rotation, shadow and paragraph spacing; see `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  // Element effects of the slide canvas (PD-394): rotation, flips, outline dash and line, shadow and
  // paragraph format. They live on the element object itself (`rot`, `flipH`, `flipV`, `dash`,
  // `outline`, `shadow`, `para`), the same names the wire and file formats use. This module
  // normalizes them, paints them onto an element's DOM and builds the text of a formatted paragraph list.
  const DASHES = new Set(["solid", "dash", "dot"]);
  const NUMBER_STYLES = Object.freeze({
    arabicPeriod: "1.", arabicParenR: "1)", alphaUcPeriod: "A.", alphaLcPeriod: "a.", romanUcPeriod: "I.",
  });
  const BULLET_CHARS = Object.freeze(["•", "–", "■", "○", "✓"]);
  const LINE_FACTOR = 1.2;
  const MIN_LINE_PCT = 50;
  const MAX_LINE_PCT = 500;
  const MAX_SPACE_PT = 200;
  const MAX_SHADOW_PT = 100;
  const MAX_FACE_CHARS = 64;
  const FX_KINDS = new Set(["textbox", "shape", "image"]);
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const round2 = (v) => Math.round(v * 100) / 100;

  function hex6(raw, fallback) {
    const clean = String(raw ?? "").trim().replace(/^#/, "");
    if (/^[0-9A-Fa-f]{6}$/.test(clean)) return clean.toUpperCase();
    if (/^[0-9A-Fa-f]{3}$/.test(clean)) return (clean[0] + clean[0] + clean[1] + clean[1] + clean[2] + clean[2]).toUpperCase();
    return fallback;
  }

  const finite = (v, fallback) => (Number.isFinite(Number(v)) && v !== null && v !== "" ? Number(v) : fallback);

  function normalizeBullet(raw) {
    if (raw?.kind === "char" && typeof raw.char === "string" && [...raw.char].length === 1 && !/\s/.test(raw.char)) {
      return { kind: "char", char: raw.char };
    }
    if (raw?.kind === "number") {
      return { kind: "number", style: Object.hasOwn(NUMBER_STYLES, raw.style) ? raw.style : "arabicPeriod" };
    }
    return null;
  }

  function normalizePara(raw) {
    if (!raw || typeof raw !== "object") return null;
    const out = {};
    if (raw.justify === true) out.justify = true;
    const bullet = normalizeBullet(raw.bullet);
    if (bullet) out.bullet = bullet;
    const pct = finite(raw.lineSpacingPct, null);
    if (pct !== null && Math.round(pct) !== 100) out.lineSpacingPct = Math.round(clamp(pct, MIN_LINE_PCT, MAX_LINE_PCT));
    for (const key of ["spaceBeforePt", "spaceAfterPt"]) {
      const v = finite(raw[key], null);
      if (v !== null && v > 0) out[key] = round2(clamp(v, 0, MAX_SPACE_PT));
    }
    return Object.keys(out).length > 0 ? out : null;
  }

  function normalizeShadow(raw) {
    if (!raw || typeof raw !== "object") return null;
    return {
      color: hex6(raw.color, "000000"),
      alphaPct: Math.round(clamp(finite(raw.alphaPct, 40), 0, 100)),
      blurPt: round2(clamp(finite(raw.blurPt, 4), 0, MAX_SHADOW_PT)),
      distPt: round2(clamp(finite(raw.distPt, 3), 0, MAX_SHADOW_PT)),
      angleDeg: round2(((finite(raw.angleDeg, 45) % 360) + 360) % 360),
    };
  }

  // The effect properties of `raw` that apply to an element of `kind`; tables and charts take none.
  function normalize(raw, kind) {
    const out = {};
    if (!raw || !FX_KINDS.has(kind)) return out;
    const rot = round2(((finite(raw.rot, 0) % 360) + 360) % 360);
    if (rot > 0 && rot < 360) out.rot = rot;
    if (raw.flipH === true) out.flipH = true;
    if (raw.flipV === true) out.flipV = true;
    if (DASHES.has(raw.dash) && raw.dash !== "solid") out.dash = raw.dash;
    if (kind !== "shape" && raw.outline && typeof raw.outline === "object") {
      out.outline = { color: hex6(raw.outline.color, "000000"), widthPt: round2(clamp(finite(raw.outline.widthPt, 0.75), 0, 24)) };
    }
    const shadow = normalizeShadow(raw.shadow);
    if (shadow) out.shadow = shadow;
    const crop = kind === "image" ? normalizeCrop(raw.crop) : null;
    if (crop) out.crop = crop;
    // A picture padded by its file (a negative `a:srcRect` side) keeps that rect as it is unless it is cut in the editor.
    const pad = kind === "image" && !crop ? normalizeSrcPad(raw.srcPad) : null;
    if (pad) out.srcPad = pad;
    if (kind !== "image") {
      const para = normalizePara(raw.para);
      if (para) out.para = para;
      // Typefaces of the text (PD-398): the Latin face and the East Asian one, as `a:latin` and `a:ea`.
      for (const key of ["font", "eastFont"]) {
        const face = normalizeFace(raw[key]);
        if (face) out[key] = face;
      }
    }
    return out;
  }

  // A typeface name: one line of at most 64 characters, no control characters (C0 and C1, as the file engine's `clean_face`
  // drops them) and no markup characters.
  function normalizeFace(raw) {
    const face = String(raw ?? "").replace(/[\u0000-\u001f\u007f-\u009f<>"&]/g, "").trim();
    return [...face].length > 0 ? [...face].slice(0, MAX_FACE_CHARS).join("") : "";
  }

  const FX_KEYS = ["rot", "flipH", "flipV", "dash", "outline", "shadow", "para", "font", "eastFont", "crop", "srcPad"];

  // 그림 자르기 (PD-402): the part of each side of a picture that is cut away, in whole thousandths of a percent (`a:srcRect`, the
  // unit and limits of the document window's picture crop: each side up to 99000, and at least 1% left across and down). A value
  // that does not fit is no crop.
  const MAX_CROP_SIDE = 99000;
  function normalizeCrop(raw) {
    if (!raw || typeof raw !== "object") return null;
    const crop = {};
    for (const side of ["left", "top", "right", "bottom"]) {
      const v = raw[side] === undefined ? 0 : raw[side];
      if (!Number.isInteger(v) || v < 0 || v > MAX_CROP_SIDE) return null;
      crop[side] = v;
    }
    if (crop.left + crop.right > MAX_CROP_SIDE || crop.top + crop.bottom > MAX_CROP_SIDE) return null;
    return crop.left + crop.top + crop.right + crop.bottom > 0 ? crop : null;
  }

  // `[l, t, r, b]` of a padded `a:srcRect` (some side negative, none past -1000000, the cut sides leaving 1%), or null.
  function normalizeSrcPad(raw) {
    if (!Array.isArray(raw) || raw.length !== 4 || !raw.every((v) => Number.isInteger(v) && v >= -1000000 && v <= MAX_CROP_SIDE)) return null;
    const [l, t, r, b] = raw;
    return raw.some((v) => v < 0) && l + r <= MAX_CROP_SIDE && t + b <= MAX_CROP_SIDE ? [l, t, r, b] : null;
  }

  // The picture inside its frame: drawn at the size of the whole image and shifted, the frame cuts the rest away.
  function cropImage(img, crop) {
    const kx = 1 - (crop.left + crop.right) / 100000;
    const ky = 1 - (crop.top + crop.bottom) / 100000;
    Object.assign(img.style, {
      position: "absolute", maxWidth: "none", maxHeight: "none", objectFit: "fill",
      width: round2(100 / kx) + "%", height: round2(100 / ky) + "%",
      left: round2((-crop.left / 100000 / kx) * 100) + "%", top: round2((-crop.top / 100000 / ky) * 100) + "%",
    });
  }

  // Removes the effect properties of an element, so a merge can bring back only the ones it still has.
  function clear(el) {
    for (const key of FX_KEYS) delete el[key];
  }

  // The element with only its normalized effect properties replaced.
  function withFx(el, raw) {
    if (!el) return el;
    clear(el);
    return Object.assign(el, normalize(raw, el.kind));
  }

  function markerFor(bullet, index) {
    if (bullet.kind === "char") return bullet.char;
    const n = index + 1;
    const letters = (k) => {
      let s = "";
      for (let v = k; v > 0; v = Math.floor((v - 1) / 26)) s = String.fromCharCode(97 + ((v - 1) % 26)) + s;
      return s;
    };
    const roman = (k) => {
      const table = [[1000, "M"], [900, "CM"], [500, "D"], [400, "CD"], [100, "C"], [90, "XC"], [50, "L"], [40, "XL"], [10, "X"], [9, "IX"], [5, "V"], [4, "IV"], [1, "I"]];
      let s = "", v = k;
      for (const [value, sym] of table) while (v >= value) { s += sym; v -= value; }
      return s;
    };
    switch (bullet.style) {
      case "arabicParenR": return n + ")";
      case "alphaUcPeriod": return letters(n).toUpperCase() + ".";
      case "alphaLcPeriod": return letters(n) + ".";
      case "romanUcPeriod": return roman(n) + ".";
      default: return n + ".";
    }
  }

  // Text of an element: plain, or one block per paragraph when it has a bullet, line spacing or spacing.
  function paintText(document, node, el) {
    const para = el.para;
    const text = String(el.text ?? "");
    node.style.lineHeight = "";
    if (!para) {
      node.textContent = text;
      return;
    }
    if (para.justify) node.style.textAlign = "justify";
    if (!para.bullet && !para.lineSpacingPct && !para.spaceBeforePt && !para.spaceAfterPt) {
      node.textContent = text;
      return;
    }
    const size = el.fontSizePt || 18;
    const wrap = document.createElement("div");
    wrap.className = "slide-paras";
    if (para.lineSpacingPct) wrap.style.lineHeight = String(round2((para.lineSpacingPct / 100) * LINE_FACTOR));
    text.split("\n").forEach((line, i) => {
      const p = document.createElement("div");
      p.className = "slide-para" + (para.bullet ? " has-bullet" : "");
      if (para.spaceBeforePt && i > 0) p.style.marginTop = round2(para.spaceBeforePt / size) + "em";
      if (para.spaceAfterPt) p.style.marginBottom = round2(para.spaceAfterPt / size) + "em";
      if (para.bullet) {
        const mark = document.createElement("span");
        mark.className = "slide-bullet";
        mark.textContent = markerFor(para.bullet, i);
        p.appendChild(mark);
      }
      p.appendChild(document.createTextNode(line));
      wrap.appendChild(p);
    });
    node.replaceChildren(wrap);
  }

  // Editing works on the plain text; the formatted blocks come back when the box loses focus.
  function beginTextEdit(node, el, onDone) {
    if (!node.querySelector?.(".slide-paras")) return;
    node.textContent = String(el.text ?? "");
    node.addEventListener("blur", () => onDone?.(), { once: true });
  }

  function dropShadow(shadow, stage) {
    const rad = (shadow.angleDeg * Math.PI) / 180;
    const unit = (pt) => round2((pt / stage.w) * 100) + "cqw";
    const r = parseInt(shadow.color.slice(0, 2), 16), g = parseInt(shadow.color.slice(2, 4), 16), b = parseInt(shadow.color.slice(4, 6), 16);
    return `drop-shadow(${unit(shadow.distPt * Math.cos(rad))} ${unit(shadow.distPt * Math.sin(rad))} ${unit(shadow.blurPt)} rgba(${r}, ${g}, ${b}, ${shadow.alphaPct / 100}))`;
  }

  // Rotation, flips, shadow and outline of a freshly built element card; an interactive card also gets
  // the rotation handle.
  function applyDom(document, card, el, stage, interactive) {
    card.style.transform = el.rot ? `rotate(${el.rot}deg)` : "";
    card.style.fontFamily = [el.font, el.eastFont].filter(Boolean).map((f) => `"${f}"`).join(", ");
    card.style.filter = el.shadow ? dropShadow(el.shadow, stage) : "";
    const mirror = [el.flipH ? "scaleX(-1)" : "", el.flipV ? "scaleY(-1)" : ""].filter(Boolean).join(" ");
    const body = card.querySelector(".slide-el-shape-svg, .slide-el-image");
    if (body && mirror) body.style.transform = mirror;
    if (el.outline && el.outline.widthPt > 0) {
      const ring = document.createElement("div");
      ring.className = "slide-el-outline";
      ring.style.borderWidth = round2((el.outline.widthPt / stage.w) * 100) + "cqw";
      ring.style.borderStyle = el.dash === "dash" ? "dashed" : el.dash === "dot" ? "dotted" : "solid";
      ring.style.borderColor = "#" + el.outline.color;
      card.appendChild(ring);
    }
    if (interactive) {
      const handle = document.createElement("span");
      handle.className = "slide-el-rotate";
      handle.setAttribute("aria-hidden", "true");
      card.appendChild(handle);
    }
  }

  host.__EASYOFFICE_SLIDE_FX__ = {
    BULLET_CHARS, NUMBER_STYLES, MAX_CROP_SIDE, normalize, normalizeCrop, normalizeSrcPad, cropImage, clear, withFx, paintText, beginTextEdit, applyDom, markerFor, hex6,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
