(function (host) {
  "use strict";

  // The zoom a window may take and the steps of its zoom buttons. A document, slide or PDF window zooms
  // from 50 to 200 percent by the steps of the shell's status bar; a sheet window from 10 to 400 percent
  // (the range a sheet view of a file may carry, PD-208) by 10 percent per press, the step of the
  // spreadsheet program's zoom buttons.

  const SHEET = { min: 10, max: 400 };
  const BOARD = { min: 25, max: 400, steps: [25, 33, 50, 67, 75, 90, 100, 125, 150, 200, 300, 400] };
  const OTHER = { min: 50, max: 200, steps: [75, 90, 100, 125, 150] };

  const rangeOf = (module) => (module === "SHEET" ? SHEET : module === "BOARD" ? BOARD : OTHER);

  // The zoom `delta` button presses from `current`.
  function step(module, current, delta) {
    const range = rangeOf(module);
    const cur = Math.round(Number(current) || 100);
    if (module === "SHEET") return Math.max(range.min, Math.min(range.max, Math.round(cur / 10) * 10 + delta * 10));
    if (module === "BOARD") {
      if (delta > 0) return range.steps.find((z) => z > cur) ?? range.max;
      if (delta < 0) {
        for (let i = range.steps.length - 1; i >= 0; i -= 1) {
          if (range.steps[i] < cur) return range.steps[i];
        }
        return range.min;
      }
      return Math.max(range.min, Math.min(range.max, cur));
    }
    const at = range.steps.indexOf(cur);
    return range.steps[Math.max(0, Math.min(range.steps.length - 1, (at < 0 ? 2 : at) + delta))];
  }

  // The status bar's slider takes the range of the window.
  function syncSlider(slider, module) {
    const range = rangeOf(module);
    if (!slider) return;
    slider.min = String(range.min);
    slider.max = String(range.max);
  }

  host.__EASYOFFICE_ZOOM_RANGE__ = { rangeOf, step, syncSlider, SHEET_MIN: SHEET.min, SHEET_MAX: SHEET.max };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
