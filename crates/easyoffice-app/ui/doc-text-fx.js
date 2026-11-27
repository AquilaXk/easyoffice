(function (host) {
  "use strict";

  // Text effects of a run beyond bold, italic, single underline and single strikethrough (PD-375), the 글꼴 tab's 이중 취소선,
  // 작은 대문자, 모두 대문자, 숨김, 밑줄 스타일 and 강조점. A run carries them as one integer (`textfx`), packed like the engine's
  // `TextFx::pack`: bit 0 double strikethrough, bit 1 small capitals, bit 2 all capitals, bit 3 hidden, bits 4-6 the emphasis
  // mark (0 none, 1 dot, 2 comma, 3 circle, 4 dot below) and bits 7-11 the underline style (0 single ... 16, the index of the
  // `ST_Underline` names that have a line). The underline colour is a separate run value (`ucolor`).

  const MAX_PACKED = 2 ** 12 - 1;
  const MAX_EMPHASIS = 4;
  const MAX_UNDERLINE_STYLE = 16;
  const DOUBLE_STRIKE = 1;
  const SMALL_CAPS = 2;
  const ALL_CAPS = 4;
  const HIDDEN = 8;
  const EMPHASIS_SHIFT = 4;
  const UNDERLINE_SHIFT = 7;
  const HIDDEN_OPACITY = "0.55";
  const HEAVY_THICKNESS = "2px";

  // CSS `text-decoration-style` and whether the line is heavy, per underline style index.
  const UNDERLINE_CSS = [
    ["solid", false], ["solid", false], ["double", false], ["solid", true], ["dotted", false], ["dotted", true], ["dashed", false],
    ["dashed", true], ["dashed", false], ["dashed", true], ["dashed", false], ["dashed", true], ["dashed", false], ["dashed", true],
    ["wavy", false], ["wavy", true], ["wavy", false],
  ];
  // CSS `text-emphasis-style` and position per emphasis mark (CSS has no comma: its nearest is the sesame).
  const EMPHASIS_CSS = [null, ["filled dot", "over right"], ["filled sesame", "over right"], ["open circle", "over right"], ["filled dot", "under right"]];

  const parts = (packed) => ({
    doubleStrike: (packed & DOUBLE_STRIKE) !== 0,
    smallCaps: (packed & SMALL_CAPS) !== 0,
    allCaps: (packed & ALL_CAPS) !== 0,
    hidden: (packed & HIDDEN) !== 0,
    emphasis: (packed >> EMPHASIS_SHIFT) & 7,
    underline: (packed >> UNDERLINE_SHIFT) & 31,
  });

  // Whether `packed` is a value the engine accepts: an integer in range with a known emphasis mark and underline style.
  function valid(packed) {
    if (!Number.isInteger(packed) || packed < 0 || packed > MAX_PACKED) return false;
    const p = parts(packed);
    return p.emphasis <= MAX_EMPHASIS && p.underline <= MAX_UNDERLINE_STYLE;
  }

  function pack({ doubleStrike = false, smallCaps = false, allCaps = false, hidden = false, emphasis = 0, underline = 0 }) {
    return (doubleStrike ? DOUBLE_STRIKE : 0) | (smallCaps ? SMALL_CAPS : 0) | (allCaps ? ALL_CAPS : 0) | (hidden ? HIDDEN : 0)
      | (emphasis << EMPHASIS_SHIFT) | (underline << UNDERLINE_SHIFT);
  }

  // The bits of the packed value the dialog's controls in `keys` (`dstrike`, `smallcaps`, `allcaps`, `hidden`, `emphasis`,
  // `ulstyle`) own, for a `{ mask, value }` patch.
  const KEY_MASK = { dstrike: DOUBLE_STRIKE, smallcaps: SMALL_CAPS, allcaps: ALL_CAPS, hidden: HIDDEN, emphasis: 7 << EMPHASIS_SHIFT, ulstyle: 31 << UNDERLINE_SHIFT };
  const maskOf = (keys) => [...keys].reduce((mask, key) => mask | (KEY_MASK[key] || 0), 0);

  // Screen drawing of a run format `fmt` (`underline`, `strike`, `textfx`, `ucolor`): every key is set, empty when it does not
  // apply. `textDecoration` stays empty for a run without effects, so the run's plain decoration is kept. Hidden text is drawn
  // dimmed (a word processor hides it unless it shows hidden text; the editor keeps it visible so it can be selected).
  function css(fmt) {
    const out = {
      textDecoration: "", textDecorationStyle: "", textDecorationColor: "", textDecorationThickness: "", fontVariantCaps: "",
      textTransform: "", opacity: "", textEmphasisStyle: "", textEmphasisPosition: "",
    };
    const packed = valid(fmt.textfx || 0) ? fmt.textfx || 0 : 0;
    if (packed === 0 && !fmt.ucolor) return out;
    const p = parts(packed);
    const lines = [fmt.underline ? "underline" : "", fmt.strike || p.doubleStrike ? "line-through" : ""].filter(Boolean);
    out.textDecoration = lines.join(" ");
    if (fmt.underline) {
      const [style, heavy] = UNDERLINE_CSS[p.underline];
      out.textDecorationStyle = style;
      if (heavy) out.textDecorationThickness = HEAVY_THICKNESS;
    } else if (p.doubleStrike) {
      out.textDecorationStyle = "double";
    }
    if (fmt.ucolor && fmt.underline) out.textDecorationColor = `#${fmt.ucolor}`;
    if (p.smallCaps) out.fontVariantCaps = "small-caps";
    if (p.allCaps) out.textTransform = "uppercase";
    if (p.hidden) out.opacity = HIDDEN_OPACITY;
    const mark = EMPHASIS_CSS[p.emphasis];
    if (mark) [out.textEmphasisStyle, out.textEmphasisPosition] = mark;
    return out;
  }

  host.__EASYOFFICE_DOC_TEXT_FX__ = { MAX_PACKED, MAX_UNDERLINE_STYLE, valid, pack, parts, css, maskOf };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
