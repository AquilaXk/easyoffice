(function (host) {
  "use strict";

  // The gallery of table styles of the table design bar (PD-230): the 60 built-in styles in the groups the spreadsheet program
  // lists them (밝게, 보통, 어둡게), the styles the file defines (사용자 지정) and the button that clears the style. A style is a
  // five by five sample of a table in that style with the bar's options (header row, stripes, first and last column) applied.
  // The panel is a popover under the button, placed and closed like the colour palette (`color-picker.js`).

  const GROUPS = [
    ["light", "밝게"],
    ["medium", "보통"],
    ["dark", "어둡게"],
  ];
  const CUSTOM_LABEL = "사용자 지정";
  const CLEAR_LABEL = "지우기";
  const KIND_LABELS = { Light: "밝게", Medium: "보통", Dark: "어둡게" };
  const PER_ROW = 7;

  const TITLE_PATTERN = /^TableStyle(Light|Medium|Dark)(\d+)$/;

  // The name a style shows in its tooltip: `표 스타일 보통 2` for `TableStyleMedium2`, the file's own name otherwise.
  function titleOf(name) {
    const m = TITLE_PATTERN.exec(name);
    return m ? "표 스타일 " + KIND_LABELS[m[1]] + " " + m[2] : name;
  }

  function createSheetTableGallery({ document, onPick, onClear }) {
    const style = host.__EASYOFFICE_SHEET_TABLE_STYLE__;
    const panel = document.getElementById("sheet-table-gallery");
    let opener = null;
    let current = { name: null, options: {} };

    function swatch(name, options, def) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "sheet-table-swatch";
      button.title = titleOf(name);
      button.setAttribute("aria-label", titleOf(name));
      button.dataset.tableStyle = name;
      const selected = name === current.name;
      button.classList.toggle("selected", selected);
      button.setAttribute("aria-pressed", selected ? "true" : "false");
      const cells = style.previewCells(name, options, def);
      for (const row of cells || []) {
        for (const look of row) {
          const cell = document.createElement("span");
          cell.className = "sheet-table-swatch-cell";
          if (look.fill) cell.style.backgroundColor = "#" + look.fill;
          const shadows = style.shadowsOf(look);
          if (shadows.length > 0) cell.style.boxShadow = shadows.join(", ");
          const bar = document.createElement("span");
          bar.className = "sheet-table-swatch-text";
          const ink = look.color && !(look.color === "000000" && !look.fill) ? "#" + look.color : look.fill ? "#" + style.autoTextColor(look.fill) : "var(--color-text)";
          bar.style.backgroundColor = ink;
          cell.appendChild(bar);
          button.appendChild(cell);
        }
      }
      button.addEventListener("click", () => {
        close();
        onPick(name, def);
      });
      return button;
    }

    function group(label, names, options, defs = {}) {
      const box = document.createElement("div");
      box.className = "sheet-table-gallery-group";
      const title = document.createElement("div");
      title.className = "sheet-table-gallery-title";
      title.textContent = label;
      box.appendChild(title);
      for (let i = 0; i < names.length; i += PER_ROW) {
        const row = document.createElement("div");
        row.className = "sb-hl-palette-row sheet-table-gallery-row";
        for (const name of names.slice(i, i + PER_ROW)) row.appendChild(swatch(name, options, defs[name]));
        box.appendChild(row);
      }
      return box;
    }

    // Fills the panel for a table whose style is `name` and whose options are `options` (`header`, `rowStripes`, ...);
    // `custom` is `{ name: definition }` of the styles the workbook defines.
    function render(name, options, custom) {
      current = { name, options };
      panel.replaceChildren();
      for (const [kind, label] of GROUPS) panel.appendChild(group(label, style.BUILT_IN[kind], options));
      const names = Object.keys(custom);
      if (names.length > 0) panel.appendChild(group(CUSTOM_LABEL, names, options, custom));
      const actions = document.createElement("div");
      actions.className = "sb-hl-palette-actions";
      const clear = document.createElement("button");
      clear.type = "button";
      clear.textContent = CLEAR_LABEL;
      clear.addEventListener("click", () => {
        close();
        onClear();
      });
      actions.appendChild(clear);
      panel.appendChild(actions);
    }

    function place() {
      const picker = host.__EASYOFFICE_COLOR_PICKER__;
      const size = panel.getBoundingClientRect();
      const at = picker.placePanel(opener.getBoundingClientRect(), size, { width: host.innerWidth, height: host.innerHeight });
      panel.style.left = at.left + "px";
      panel.style.top = at.top + "px";
    }

    function open(button, name, options, custom) {
      opener = button;
      render(name, options, custom);
      panel.hidden = false;
      panel.showPopover?.();
      place();
      button.setAttribute("aria-expanded", "true");
      panel.querySelector(".sheet-table-swatch.selected, .sheet-table-swatch")?.focus?.();
    }

    function close() {
      if (panel.hidden) return;
      panel.hidePopover?.();
      panel.hidden = true;
      opener?.setAttribute("aria-expanded", "false");
    }

    const isOpen = () => !panel.hidden;

    function bind() {
      panel.setAttribute("popover", "manual");
      document.addEventListener(
        "mousedown",
        (event) => {
          if (isOpen() && !panel.contains(event.target) && !opener?.contains(event.target)) close();
        },
        true,
      );
      document.addEventListener(
        "keydown",
        (event) => {
          if (event.key !== "Escape" || !isOpen()) return;
          event.stopPropagation?.();
          close();
          opener?.focus?.();
        },
        true,
      );
      host.addEventListener?.("resize", close);
    }

    return { open, close, isOpen, bind, render, titleOf };
  }

  host.__EASYOFFICE_CREATE_SHEET_TABLE_GALLERY__ = createSheetTableGallery;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
