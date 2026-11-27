(function (host) {
  "use strict";

  // Style popup of the Pages sidebar's 서체 section (PD-368): the variants of the selection's font family (가는체,
  // 폭좁은체, ...), as the desktop shell lists them from the installed fonts (`list_font_styles`). Without the shell
  // (the browser preview) the popup holds the one entry 보통 and stays off.

  const PLAIN_LABEL = "보통";
  const CACHE_LIMIT = 128;
  const MAX_STYLES = 64;
  const MAX_TEXT = 128;
  const MAX_FULL_NAME_CHARS = 64;

  // One variant of the shell's answer, or null when a field is not what the shell sends (the answer is data, not code).
  function cleanStyle(raw, runs) {
    const text = (v) => (typeof v === "string" && v.trim() !== "" && v.length <= MAX_TEXT && !/[\u0000-\u001f\u007f]/.test(v) ? v.trim() : null);
    const face = typeof raw?.face === "string" ? runs.normalizeProp("face", raw.face) : null;
    const style = text(raw?.style);
    const fullName = typeof raw?.fullName === "string" && raw.fullName.length <= MAX_FULL_NAME_CHARS ? runs.normalizeFont(raw.fullName) : null;
    if (!face || !style || !fullName || !Number.isInteger(raw.weight) || !Number.isInteger(raw.width)) return null;
    return { face, fullName, style, weight: raw.weight, width: raw.width, italic: raw.italic === true };
  }

  const isPlain = (s) => s.weight === 400 && s.width === 100 && !s.italic;

  function create(deps = {}) {
    const runsApi = () => host.__EASYOFFICE_DOC_RUNS__;
    const cache = new Map();
    let token = 0;

    const available = () => typeof deps.systemFonts?.styles === "function";

    // The family of `name` and its variants; an empty group for a name the machine does not have or when the shell fails.
    function groupOf(name) {
      if (!available() || !name) return Promise.resolve({ family: "", styles: [] });
      if (cache.has(name)) return cache.get(name);
      const pending = Promise.resolve()
        .then(() => deps.systemFonts.styles(name))
        .then((raw) => {
          const R = runsApi();
          const styles = (Array.isArray(raw?.styles) ? raw.styles.slice(0, MAX_STYLES) : []).map((s) => cleanStyle(s, R)).filter(Boolean);
          const family = typeof raw?.family === "string" ? R.normalizeFont(raw.family) : null;
          if (!family || styles.length === 0) return { family: "", styles: [] };
          const group = { family, styles };
          cache.set(family, Promise.resolve(group)); // the popup asks again by the family it showed
          return group;
        })
        .catch(() => ({ family: "", styles: [] }));
      if (cache.size >= CACHE_LIMIT) cache.delete(cache.keys().next().value);
      cache.set(name, pending);
      return pending;
    }

    // The variant a run is in: its face, else the one whose full name is the run's font name, else (a plain family) the plain one.
    function currentOf(group, state) {
      const found = group.styles.find((s) => s.face === state.face) || group.styles.find((s) => s.fullName === state.font);
      return found || group.styles.find(isPlain) || group.styles[0];
    }

    function showPlain(select) {
      select.replaceChildren();
      const opt = select.ownerDocument.createElement("option");
      opt.value = "";
      opt.textContent = PLAIN_LABEL;
      select.appendChild(opt);
      select.value = "";
      select.disabled = true;
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(select);
    }

    function fill(select, group, current) {
      select.replaceChildren();
      for (const s of group.styles) {
        const opt = select.ownerDocument.createElement("option");
        opt.value = s.face;
        opt.textContent = s.style;
        select.appendChild(opt);
      }
      select.value = current.face;
      select.disabled = group.styles.length < 2;
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect?.(select);
    }

    // Shows the variants of the selection's font in `select`; the answer of an older selection is dropped.
    async function sync(select, state) {
      const mine = ++token;
      const font = typeof state?.font === "string" && state.font !== "mixed" ? state.font : "";
      if (!select || !font || !available()) return void (select && showPlain(select));
      const group = await groupOf(state.face || font);
      if (mine !== token) return;
      if (group.styles.length === 0) return showPlain(select);
      fill(select, group, currentOf(group, state));
      select.dataset.eoFamily = group.family;
    }

    // The patch that puts `face` (a PostScript name of the group) on the selection: the plain variant is the family itself,
    // any other is its full name with the face the `.pages` file keeps.
    async function patchFor(name, face) {
      const group = await groupOf(name);
      const variant = group.styles.find((s) => s.face === face);
      if (!variant) return null;
      return isPlain(variant) ? { font: group.family, face: null } : { font: variant.fullName, face: variant.face };
    }

    return { available, groupOf, sync, patchFor, PLAIN_LABEL };
  }

  host.__EASYOFFICE_CREATE_DOC_FONT_STYLES__ = create;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
