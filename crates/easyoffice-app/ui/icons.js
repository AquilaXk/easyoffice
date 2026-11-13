(function (host) {
  "use strict";

  // The one icon set of the shell (PD-088). Every icon is a 24x24 stroke drawing built from plain
  // path data (no characters, no emoji, no external files) and inserted as an SVG element, so it
  // follows `currentColor` in both themes. Controls declare `data-icon="name"`; their accessible
  // name always comes from `aria-label` or visible text, never from the drawing.

  const SVG_NS = "http://www.w3.org/2000/svg";
  const circle = (cx, cy, r) =>
    "M" + (cx - r) + " " + cy + "a" + r + " " + r + " 0 1 0 " + 2 * r + " 0a" + r + " " + r + " 0 1 0 " + -2 * r + " 0";

  const ICONS = Object.freeze({
    "rotate-cw": ["M20 5v5h-5", "M20 10a8 8 0 1 0 1 4"],
    save: ["M5 4h11l3 3v13H5z", "M8 4v5h7V4", "M8 20v-6h8v6"],
    undo: ["M9 14L4 9l5-5", "M4 9h10a6 6 0 0 1 0 12h-3"],
    redo: ["M15 14l5-5-5-5", "M20 9H10a6 6 0 0 0 0 12h3"],
    cut: ["M6 3l9 11", "M18 3l-9 11", circle(6, 18, 2.5), circle(18, 18, 2.5)],
    copy: ["M9 9h10v11H9z", "M5 15V4h10"],
    paste: ["M8 5H5v15h14v-3", "M8 3h6v4H8z", "M12 11h8v9h-8z"],
    table: ["M4 5h16v14H4z", "M4 10h16", "M4 15h16", "M10 5v14"],
    link: ["M10 14a4 4 0 0 0 5.7 0l3.3-3.3a4 4 0 0 0-5.7-5.7L12 6.3", "M14 10a4 4 0 0 0-5.7 0L5 13.3a4 4 0 0 0 5.7 5.7l1.3-1.3"],
    header: ["M4 4h16v16H4z", "M4 9h16"],
    footnote: ["M5 5h14v10H5z", "M8 19h8", "M9 9h6"],
    find: ["M3 20l5-5", circle(14, 10, 5.5), "M11 10h6"],
    search: [circle(10.5, 10.5, 6.5), "M15.5 15.5L21 21"],
    "chevron-down": ["M6 9l6 6 6-6"],
    "chevron-up": ["M6 15l6-6 6 6"],
    "chevron-right": ["M9 6l6 6-6 6"],
    bold: ["M7 4h6a4 4 0 0 1 0 8H7z", "M7 12h7a4 4 0 0 1 0 8H7z"],
    italic: ["M10 4h8", "M6 20h8", "M15 4L9 20"],
    underline: ["M7 4v7a5 5 0 0 0 10 0V4", "M5 20h14"],
    strikethrough: ["M4 12h16", "M16 6.5C15 5 13.5 4.5 12 4.5c-2.5 0-4 1.2-4 3 0 4.5 8 2.5 8 7 0 1.8-1.7 3-4 3-2 0-3.7-.8-4.5-2.2"],
    "text-color": ["M6 16L12 4l6 12", "M8 12h8", "M5 20h14"],
    highlighter: ["M14 4l6 6-8 8H6v-6z", "M4 21h10"],
    "valign-top": ["M4 4h16", "M12 20V8", "M8 12l4-4 4 4"],
    "valign-middle": ["M4 12h16", "M12 3v6", "M9 6l3 3 3-3", "M12 21v-6", "M9 18l3-3 3 3"],
    "valign-bottom": ["M4 20h16", "M12 4v12", "M8 12l4 4 4-4"],
    "border-all": ["M4 4h16v16H4z", "M4 12h16", "M12 4v16"],
    "align-left": ["M4 6h16", "M4 10h10", "M4 14h16", "M4 18h10"],
    "align-center": ["M4 6h16", "M7 10h10", "M4 14h16", "M7 18h10"],
    "align-right": ["M4 6h16", "M10 10h10", "M4 14h16", "M10 18h10"],
    "align-justify": ["M4 6h16", "M4 10h16", "M4 14h16", "M4 18h16"],
    "align-distribute": ["M4 6h16", "M4 10h16", "M4 14h16", "M4 18h12", "M18 18h2"],
    "align-divide": ["M4 6h16", "M4 10h16", "M4 14h8", "M14 14h6", "M4 18h16"],
    "line-spacing": ["M9 6h11", "M9 12h11", "M9 18h11", "M4 8V4l-1.5 1.5", "M4 16v4l-1.5-1.5"],
    "zoom-in": [circle(10.5, 10.5, 6.5), "M15.5 15.5L21 21", "M10.5 8v5", "M8 10.5h5"],
    "zoom-out": [circle(10.5, 10.5, 6.5), "M15.5 15.5L21 21", "M8 10.5h5"],
    "fit-width": ["M4 12h16", "M8 8l-4 4 4 4", "M16 8l4 4-4 4"],
    "fit-page": ["M5 4h14v16H5z", "M9 9h6v6H9z"],
    percent: ["M19 5L5 19", circle(7.5, 7.5, 2.5), circle(16.5, 16.5, 2.5)],
    plus: ["M12 5v14", "M5 12h14"],
    minus: ["M5 12h14"],
    close: ["M6 6l12 12", "M18 6L6 18"],
    sun: [circle(12, 12, 4), "M12 3v2", "M12 19v2", "M3 12h2", "M19 12h2", "M5.6 5.600l1.4 1.4", "M17 17l1.4 1.4", "M5.6 18.400L7 17", "M17 7l1.4-1.4"],
    moon: ["M20 14.500A8 8 0 1 1 9.5 4 6.5 6.5 0 0 0 20 14.500z"],
    file: ["M6 3h8l5 5v13H6z", "M14 3v5h5"],
    "file-doc": ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M9 12h7", "M9 15h7", "M9 18h4"],
    "file-hwp": ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M10 11h5", "M12.500 11v1.500", circle(12.500, 16, 2.200)],
    "file-sheet": ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M9 12h7", "M9 16h7", "M12.500 12v8"],
    "file-slide": ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M9 12h7v5H9z"],
    "file-pdf": ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M9 17c1.500-1.500 2.500-3.500 2.500-5.500 0 2 1 4 3.500 5.500-2-.2-4-.2-6 0z"],
    "file-convert": ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M9 13h6", "M13 11l2 2-2 2", "M15 18H9", "M11 16l-2 2 2 2"],
    "file-plus": ["M6 3h8l5 5v13H6z", "M14 3v5h5", "M12 12v6", "M9 15h6"],
    "folder-open": ["M3 7h6l2 2h10v10H3z", "M3 19l3-8h17l-3 8"],
    activity: ["M3 12h4l3-8 4 16 3-8h4"],
    ribbon: ["M4 5h16v5H4z", "M4 14h5v5H4z", "M12 14h8v5h-8z"],
    sidebar: ["M4 4h16v16H4z", "M9 4v16"],
    command: ["M9 9V6.500A2.5 2.5 0 1 0 6.5 9H9zm0 0h6m-6 0v6m6-6V6.500A2.5 2.5 0 1 1 17.5 9H15zm0 0v6m0 0h2.500a2.5 2.5 0 1 1-2.5 2.500V15zm0 0H9m0 0H6.500A2.5 2.5 0 1 0 9 17.500V15z"],
    shield: ["M12 3l8 3v6c0 4.5-3.2 8-8 9-4.8-1-8-4.5-8-9V6z"],
    check: ["M5 12l5 5 9-10"],
  });

  function createIcon(document, name) {
    const paths = ICONS[name];
    if (!paths) throw new Error("알 수 없는 아이콘입니다: " + name);
    const make = typeof document.createElementNS === "function"
      ? (tag) => document.createElementNS(SVG_NS, tag)
      : (tag) => document.createElement(tag);
    const svg = make("svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", "16");
    svg.setAttribute("height", "16");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.75");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    svg.setAttribute("class", "eo-icon");
    for (const d of paths) {
      const path = make("path");
      path.setAttribute("d", d);
      svg.appendChild(path);
    }
    return svg;
  }

  // Replaces whatever the element showed (a glyph fallback) with the named drawing.
  function setIcon(document, el, name) {
    if (!el) return;
    el.dataset.icon = name;
    while (el.firstChild) el.firstChild.remove();
    el.appendChild(createIcon(document, name));
  }

  function applyIcons(document, root) {
    const scope = root || document;
    for (const el of scope.querySelectorAll("[data-icon]")) {
      if (el.querySelector("svg")) continue;
      setIcon(document, el, el.dataset.icon);
    }
  }

  // The theme toggle shows the theme it would switch to, announced through its accessible name.
  function syncThemeToggle(document, theme) {
    const btn = document.getElementById("btn-theme-toggle");
    if (!btn) return;
    const dark = theme === "dark";
    const label = dark ? "라이트 모드로 전환" : "다크 모드로 전환";
    btn.setAttribute("aria-label", label);
    btn.title = label;
    setIcon(document, btn, dark ? "sun" : "moon");
  }

  host.__EASYOFFICE_ICONS__ = { ICONS, createIcon, setIcon, applyIcons, syncThemeToggle };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
