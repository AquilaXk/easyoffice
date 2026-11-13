(function (host) {
  "use strict";

  // The pages of a printed document (PD-213). Each page is a window on the layout the screen shows: the paper of the
  // page setup, the units (`doc-pages.js` `printPlan`) that fall on the page placed at the tops the page layout gave
  // them, the header and footer of the page with its number and the page count, and the page number the screen draws
  // when there is no footer. A paragraph that continues on the next page is in both windows, each showing its own lines
  // (the float shape the page layout put on it is kept), so a printed page breaks where the screen does and the browser
  // breaks nothing itself. A page is one box of the paper size (`break-after: page`, `shell.css`).
  //
  // Cost: one pass over the units, each cloned once for every page it spans (a paragraph over two pages twice), so the
  // work follows the document, not pages times document.

  const PX_PER_INCH = 96;
  const MM_PER_INCH = 25.4;
  // The page geometry the header and footer are placed with (`doc-page-frame.js`), copied from the editor to each page.
  const GEOMETRY_PROPS = ["--hf-hd", "--hf-band-h", "--hf-ft", "--hf-fband-h", "--pg-pad-l", "--pg-pad-r", "--pg-w", "--pg-h"];
  const PAGE_BOX_CLASS = "eo-print-doc-page";
  const BODY_CLASS = "eo-print-body";
  const UNIT_CLASS = "eo-print-unit";
  const NUMBER_CLASS = "eo-print-pageno";
  const FLOW_CLASS = "eo-print-flow";
  // More pages than this are refused instead of building them (fail-closed).
  const MAX_PRINT_PAGES = 2000;
  const MAX_UNIT_CLONES = 200000;
  const MSG_TOO_BIG = "인쇄 범위가 너무 커서 한 번에 인쇄할 수 없습니다.";

  const mm = (px) => Math.round((px * MM_PER_INCH * 100) / PX_PER_INCH) / 100;
  const px = (value) => Math.round(value * 100) / 100 + "px";

  // The units of every page: `pages[k]` lists the units that start, continue or end on page k, in document order.
  function bucketUnits(plan) {
    const pages = Array.from({ length: plan.pageCount }, () => []);
    let clones = 0;
    for (const unit of plan.units) {
      for (let k = unit.page; k <= unit.endPage && k < plan.pageCount; k += 1) {
        pages[k].push(unit);
        clones += 1;
      }
      if (clones > MAX_UNIT_CLONES) throw new Error(MSG_TOO_BIG);
    }
    return pages;
  }

  // The clone of a unit as the page shows it. A list item or a table-of-contents entry keeps the list or block it sits in
  // (the marker and the number come from it); the page layout's own margin is not applied (the unit is placed by its top).
  function cloneUnit(unit, editor) {
    const el = unit.el;
    const parent = el.parentElement;
    const inShell = parent && parent !== editor;
    const copy = el.cloneNode(true);
    for (const node of [copy, ...copy.querySelectorAll("[id], [contenteditable]")]) {
      node.removeAttribute("id");
      node.removeAttribute("contenteditable");
    }
    copy.style.marginTop = "0";
    if (!inShell) return copy;
    const shell = parent.cloneNode(false);
    shell.removeAttribute("id");
    shell.style.margin = "0";
    shell.style.paddingTop = "0";
    shell.style.paddingBottom = "0";
    if (parent.tagName === "OL") {
      const before = Array.from(parent.children).indexOf(el);
      shell.start = (Number.parseInt(parent.getAttribute("start") ?? "1", 10) || 1) + before;
    }
    shell.appendChild(copy);
    return shell;
  }

  // The printed pages of the editor's document, or null when the editor has no page layout (the caller then prints the
  // flow). `frame` gives the header and footer of a page (`printCopy`).
  function buildPages({ document, editor, plan, frame, rules, pageClass }) {
    if (!plan) return null;
    if (plan.pageCount > MAX_PRINT_PAGES) throw new Error(MSG_TOO_BIG);
    const { geo } = plan;
    const paper = rules.classFor(mm(geo.width), mm(geo.height));
    const buckets = bucketUnits(plan);
    const style = editor.style;
    const pages = [];
    for (let k = 0; k < plan.pageCount; k += 1) {
      const page = document.createElement("div");
      page.className = pageClass + " " + paper + " " + PAGE_BOX_CLASS;
      for (const name of GEOMETRY_PROPS) {
        const value = style.getPropertyValue(name);
        if (value) page.style.setProperty(name, value);
      }
      const body = document.createElement("div");
      body.className = BODY_CLASS + " " + FLOW_CLASS;
      body.style.left = px(geo.padLeft);
      body.style.top = px(geo.contentTop);
      body.style.width = px(geo.width - geo.padLeft - geo.padRight);
      body.style.height = px(geo.contentHeight);
      const windowTop = k * geo.pitch + geo.contentTop;
      for (const unit of buckets[k]) {
        const slot = document.createElement("div");
        slot.className = UNIT_CLASS;
        slot.style.top = px(unit.top - windowTop);
        slot.appendChild(cloneUnit(unit, editor));
        body.appendChild(slot);
      }
      page.appendChild(body);
      const header = frame?.printCopy("header", k + 1, plan.pageCount) ?? null;
      const footer = frame?.printCopy("footer", k + 1, plan.pageCount) ?? null;
      if (header) page.appendChild(header);
      if (footer) {
        page.appendChild(footer);
      } else {
        const number = document.createElement("div");
        number.className = NUMBER_CLASS;
        number.textContent = "- " + (k + 1) + " -";
        page.appendChild(number);
      }
      pages.push(page);
    }
    return pages;
  }

  host.__EASYOFFICE_DOC_PAGE_PRINT__ = { buildPages, bucketUnits, MAX_PRINT_PAGES, MSG_TOO_BIG };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
