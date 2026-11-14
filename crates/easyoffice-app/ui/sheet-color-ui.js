(function (host) {
  "use strict";

  // The 채우기 색 and 글꼴 색 buttons of the sheet toolbar (PD-227). Each is the reference editor's colour button
  // (`color-picker.js`: the glyph button applies the colour it shows, the arrow opens the palette with the recent
  // colours and "다른 색..."); the palette has a button that removes the colour: "채우기 없음" for the fill, "자동"
  // for the text. Every press is one undoable step over the selection (`ops.run`).

  const BUTTONS = [
    { id: "fill", action: "fill", clear: "fill-none", clearLabel: "채우기 없음", initial: "ffff00", recentKey: "easyoffice.recent-fill-colors" },
    { id: "font-color", action: "font-color", clear: "font-color-auto", clearLabel: "자동", initial: "ff0000", recentKey: "easyoffice.recent-font-colors" },
  ];

  function createSheetColorUi({ document, ops }) {
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const picker = host.__EASYOFFICE_COLOR_PICKER__;
    const run = (action) => void Promise.resolve(ops.run(action)).then(ops.sync).catch(() => {});
    for (const spec of BUTTONS) {
      const hostEl = document.getElementById("sheet-" + spec.id + "-host");
      const button = document.getElementById("btn-sheet-" + spec.id);
      if (!hostEl || !button || !picker) continue;
      let last = spec.initial;
      const showBar = () => button.style.setProperty("--bar-color", "#" + last);
      showBar();
      button.addEventListener("click", () => run(spec.action + ":" + last));
      picker.setupColorPicker(
        hostEl,
        (picked) => {
          if (picked === "") return run(spec.clear);
          const rgb = look.normalizeColor(picked);
          if (!rgb) return undefined;
          last = rgb;
          showBar();
          return run(spec.action + ":" + rgb);
        },
        { remember: true, clear: true, clearLabel: spec.clearLabel, recentKey: spec.recentKey }
      );
    }
  }

  host.__EASYOFFICE_CREATE_SHEET_COLOR_UI__ = createSheetColorUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
