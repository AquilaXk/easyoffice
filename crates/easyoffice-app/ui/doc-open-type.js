(function (host) {
  "use strict";

  // OpenType features of a run (PD-373), the 고급 tab's "OpenType 기능" group: ligatures, number spacing, number form,
  // stylistic sets and contextual alternates. A run carries them as one integer, packed like the engine's `OpenType::pack`:
  // bits 0-3 are the ligature kinds (standard 1, contextual 2, historical 4, discretional 8), bits 4-5 the number spacing
  // (0 default, 1 proportional, 2 tabular), bits 6-7 the number form (0 default, 1 lining, 2 old style), bit 8 the contextual
  // alternates and bits 9-28 the stylistic sets 1 to 20 (bit `8 + n` is set `n`).

  const MAX_PACKED = 2 ** 29 - 1;
  const MAX_STYLE_SET = 20;
  const SPACING_SHIFT = 4;
  const FORM_SHIFT = 6;
  const ALTERNATES_BIT = 1 << 8;
  const SETS_SHIFT = 9;
  const LIGATURE_STANDARD = 1;
  const LIGATURE_CONTEXTUAL = 2;
  const LIGATURE_HISTORICAL = 4;
  const LIGATURE_DISCRETIONAL = 8;
  const MAX_ENUM = 2;

  // The ligature kinds the dialog's list offers: 없음, 표준만, 표준 및 상황에 맞는, 기본 및 사용자 지정 (historical and
  // discretional) and 모두. A file may hold another mix of the four kinds, which the list shows as a custom choice.
  const LIGATURE_CHOICES = [0, LIGATURE_STANDARD, LIGATURE_STANDARD | LIGATURE_CONTEXTUAL, LIGATURE_HISTORICAL | LIGATURE_DISCRETIONAL, 15];

  const parts = (packed) => ({
    ligatures: packed & 15,
    spacing: (packed >> SPACING_SHIFT) & 3,
    form: (packed >> FORM_SHIFT) & 3,
    alternates: (packed & ALTERNATES_BIT) !== 0,
    sets: packed >>> SETS_SHIFT,
  });

  // Whether `packed` is a value the engine accepts: an integer in range whose number spacing and form are 0-2.
  function valid(packed) {
    if (!Number.isInteger(packed) || packed < 0 || packed > MAX_PACKED) return false;
    const p = parts(packed);
    return p.spacing <= MAX_ENUM && p.form <= MAX_ENUM;
  }

  // `{ ligatures, spacing, form, alternates, set | sets }` as the packed integer: `set` is one stylistic set 0-20 (0 none),
  // `sets` the bit set of several (a file may hold more than one, which the dialog's single list cannot show).
  function pack({ ligatures = 0, spacing = 0, form = 0, alternates = false, set = 0, sets }) {
    const mask = sets ?? (set >= 1 && set <= MAX_STYLE_SET ? 1 << (set - 1) : 0);
    return (ligatures & 15) | (spacing << SPACING_SHIFT) | (form << FORM_SHIFT) | (alternates ? ALTERNATES_BIT : 0) | (mask << SETS_SHIFT);
  }

  // The fields of a packed value as the dialog shows them; `set` is the lowest stylistic set, `sets` all of them, and
  // `custom` is true when the value holds more than the lists show (several sets, or a ligature mix the list lacks).
  function unpack(packed) {
    const p = parts(packed);
    const set = p.sets ? Math.log2(p.sets & -p.sets) + 1 : 0;
    const custom = (p.sets & (p.sets - 1)) !== 0 || !LIGATURE_CHOICES.includes(p.ligatures);
    return { ligatures: p.ligatures, spacing: p.spacing, form: p.form, alternates: p.alternates, set, sets: p.sets, custom };
  }

  // Screen drawing of the features through CSS (`font-variant-ligatures`, `font-variant-numeric`, `font-feature-settings`);
  // only a font that has a feature changes with it.
  function css(packed) {
    if (!valid(packed) || packed === 0) return { fontVariantLigatures: "", fontVariantNumeric: "", fontFeatureSettings: "" };
    const p = parts(packed);
    const ligatures = [
      p.ligatures & LIGATURE_STANDARD ? "common-ligatures" : "",
      p.ligatures & LIGATURE_HISTORICAL ? "historical-ligatures" : "",
      p.ligatures & LIGATURE_DISCRETIONAL ? "discretionary-ligatures" : "",
      p.alternates ? "contextual" : "",
    ];
    const numeric = [["", "proportional-nums", "tabular-nums"][p.spacing], ["", "lining-nums", "oldstyle-nums"][p.form]];
    const features = p.ligatures & LIGATURE_CONTEXTUAL ? ['"clig" 1'] : [];
    for (let n = 1; n <= MAX_STYLE_SET; n += 1) {
      if ((p.sets >> (n - 1)) & 1) features.push(`"ss${String(n).padStart(2, "0")}" 1`);
    }
    return {
      fontVariantLigatures: ligatures.filter(Boolean).join(" "),
      fontVariantNumeric: numeric.filter(Boolean).join(" "),
      fontFeatureSettings: features.join(", "),
    };
  }

  // The five controls of the 글꼴 dialog's OpenType 기능 group.
  const CONTROL_IDS = { ligatures: "doc-font-ligatures", spacing: "doc-font-num-spacing", form: "doc-font-num-form", set: "doc-font-style-set", alternates: "doc-font-alternates" };
  const controls = (document) => Object.fromEntries(Object.entries(CONTROL_IDS).map(([key, id]) => [key, document.getElementById(id)]));

  // Shows `packed` in the controls (`null`, a mixed selection, shows the defaults). A ligature mix the list lacks leaves its
  // list blank.
  function show(ctl, packed) {
    const u = unpack(packed || 0);
    ctl.ligatures.value = String(u.ligatures);
    ctl.spacing.value = String(u.spacing);
    ctl.form.value = String(u.form);
    ctl.set.value = String(u.set);
    ctl.alternates.checked = u.alternates;
  }

  // The packed value of the controls on top of `base` (the selection's value): only a control in `touched` replaces its part,
  // so a custom ligature mix or several stylistic sets the controls cannot show stay as the file had them.
  function compose(ctl, base, touched) {
    const u = unpack(base || 0);
    return pack({
      ligatures: touched.has("ligatures") ? Number(ctl.ligatures.value) : u.ligatures,
      spacing: touched.has("spacing") ? Number(ctl.spacing.value) : u.spacing,
      form: touched.has("form") ? Number(ctl.form.value) : u.form,
      alternates: touched.has("alternates") ? ctl.alternates.checked : u.alternates,
      ...(touched.has("set") ? { set: Number(ctl.set.value) } : { sets: u.sets }),
    });
  }

  host.__EASYOFFICE_DOC_OPEN_TYPE__ = { MAX_PACKED, MAX_STYLE_SET, LIGATURE_CHOICES, valid, pack, unpack, css, controls, show, compose };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
