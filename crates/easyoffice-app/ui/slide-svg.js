(function (host) {
  "use strict";

  // Vector body of a slide shape: the preset geometry with its fill (none when the fill is empty),
  // stroke width and dash pattern. Split from slide-canvas.js, which hands in its helpers.
  const SVG_NS = "http://www.w3.org/2000/svg";
  const DASH_PATTERNS = Object.freeze({ dash: [4, 2], dot: [1, 1.5] });

  function createSlideSvg({ clamp, round2, normalizeHex6 }) {
    function strokeDash(dash, sw) {
      const pattern = DASH_PATTERNS[dash];
      if (!pattern) return "";
      const unit = Math.max(1, sw);
      return pattern.map((n) => round2(n * unit)).join(" ");
    }

    function buildShapeSvg(document, el) {
      const vw = Math.max(24, Math.round(el.w || 200)), vh = Math.max(20, Math.round(el.h || 100));
      const sw = clamp(Number(el.strokeWidthPt ?? 2), 0, 24), pad = Math.max(1, sw / 2);
      const svg = document.createElementNS(SVG_NS, "svg");
      svg.setAttribute("class", "slide-el-shape-svg");
      svg.setAttribute("viewBox", "0 0 " + vw + " " + vh);
      svg.setAttribute("preserveAspectRatio", "none");
      svg.setAttribute("aria-hidden", "true");

      let geom;
      const st = el.shapeType || "rect";
      if (st === "ellipse") {
        geom = document.createElementNS(SVG_NS, "ellipse");
        geom.setAttribute("cx", String(round2(vw / 2)));
        geom.setAttribute("cy", String(round2(vh / 2)));
        geom.setAttribute("rx", String(round2(Math.max(2, vw / 2 - pad))));
        geom.setAttribute("ry", String(round2(Math.max(2, vh / 2 - pad))));
      } else if (st === "diamond" || st === "rightArrow" || st === "callout") {
        geom = document.createElementNS(SVG_NS, "polygon");
        let pts;
        if (st === "diamond") {
          pts = [round2(vw / 2) + "," + round2(pad), round2(vw - pad) + "," + round2(vh / 2), round2(vw / 2) + "," + round2(vh - pad), round2(pad) + "," + round2(vh / 2)];
        } else if (st === "rightArrow") {
          const headX = round2(Math.max(pad + 6, vw * 0.66)), yTop = round2(vh * 0.26), yBot = round2(vh * 0.74);
          pts = [round2(pad) + "," + yTop, headX + "," + yTop, headX + "," + round2(pad), round2(vw - pad) + "," + round2(vh / 2), headX + "," + round2(vh - pad), headX + "," + yBot, round2(pad) + "," + yBot];
        } else {
          const bodyBot = round2(Math.max(pad + 8, vh * 0.78));
          pts = [round2(pad) + "," + round2(pad), round2(vw - pad) + "," + round2(pad), round2(vw - pad) + "," + bodyBot, round2(vw * 0.36) + "," + bodyBot, round2(vw * 0.16) + "," + round2(vh - pad), round2(vw * 0.22) + "," + bodyBot, round2(pad) + "," + bodyBot];
        }
        geom.setAttribute("points", pts.join(" "));
      } else {
        geom = document.createElementNS(SVG_NS, "rect");
        geom.setAttribute("x", String(round2(pad)));
        geom.setAttribute("y", String(round2(pad)));
        geom.setAttribute("width", String(round2(Math.max(2, vw - pad * 2))));
        geom.setAttribute("height", String(round2(Math.max(2, vh - pad * 2))));
        if (st === "roundRect") {
          const r = String(round2(Math.min(18, Math.min(vw, vh) * 0.2)));
          geom.setAttribute("rx", r);
          geom.setAttribute("ry", r);
        }
      }
      geom.setAttribute("fill", el.fill ? "#" + normalizeHex6(el.fill, "2563EB") : "none");
      geom.setAttribute("stroke", sw > 0 && el.stroke ? "#" + normalizeHex6(el.stroke, "1D4ED8") : "none");
      geom.setAttribute("stroke-width", String(sw));
      const dash = strokeDash(el.dash, sw);
      if (dash) geom.setAttribute("stroke-dasharray", dash);
      svg.appendChild(geom);
      return svg;
    }

    return { buildShapeSvg };
  }

  host.__EASYOFFICE_CREATE_SLIDE_SVG__ = createSlideSvg;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
