// 참조 구현 (reference implementation): reveal.js (MIT, Copyright (c) 2011-2024 Hakim El Hattab,
// `js/controllers/fragments.js`): a slide's fragments are revealed one step per "next" and taken back one
// step per "previous", each step being a group of effects. The effects here are Web Animations on the
// element cards and the steps follow the start modes of the file formats; see `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  // Plays the animations of a slide in the slide show (PD-394). A step is one click: its effects run
  // together, an "after previous" effect starts when the group before it ends, and a first effect that does
  // not start on click plays when the slide appears. Elements an entrance effect targets start hidden.
  const DIRECTED_OFFSET = Object.freeze({ left: "inset(0 100% 0 0)", right: "inset(0 0 0 100%)", top: "inset(0 0 100% 0)", bottom: "inset(100% 0 0 0)" });
  const OPEN = "inset(0 0 0 0)";
  const GROW = 1.5;

  // The clicks of a slide: each is a list of groups, each group the effects that start together.
  function clickSteps(anims) {
    const clicks = [];
    for (const a of anims) {
      if (a.start === "onClick") clicks.push([[a]]);
      else if (clicks.length === 0) clicks.push([[a]]);
      else if (a.start === "afterPrevious") clicks.at(-1).push([a]);
      else clicks.at(-1).at(-1).push(a);
    }
    return clicks;
  }

  function createShowAnim() {
    const groupApi = () => host.__EASYOFFICE_SLIDE_GROUP__;
    let layer = null;
    let slide = null;
    let steps = [];
    let index = 0;
    let running = [];

    const cardsOf = (id) => groupApi().leavesOf(slide, id).map((leaf) => layer.querySelector(`[data-element-id="${leaf}"]`)).filter(Boolean);

    function baseTransform(card) {
      return card.dataset.baseTransform ?? (card.dataset.baseTransform = card.style.transform || "");
    }

    function keyframes(card, a) {
      const base = baseTransform(card);
      const entering = a.class === "entrance";
      const both = (from, to) => (entering ? [from, to] : [to, from]);
      const L = layer.getBoundingClientRect(), r = card.getBoundingClientRect();
      const away = {
        left: `translateX(${-(r.right - L.left)}px)`, right: `translateX(${L.right - r.left}px)`,
        top: `translateY(${-(r.bottom - L.top)}px)`, bottom: `translateY(${L.bottom - r.top}px)`,
      }[a.direction] || "";
      switch (a.effect) {
        case "appear": return both({ visibility: "hidden" }, { visibility: "visible" });
        case "fade": return both({ opacity: 0, visibility: "visible" }, { opacity: 1, visibility: "visible" });
        case "fly": return both({ transform: `${away} ${base}`, visibility: "visible" }, { transform: base, visibility: "visible" });
        case "wipe": return both({ clipPath: DIRECTED_OFFSET[a.direction] || DIRECTED_OFFSET.bottom, visibility: "visible" }, { clipPath: OPEN, visibility: "visible" });
        case "zoom": return both({ transform: `${base} scale(0)`, visibility: "visible" }, { transform: `${base} scale(1)`, visibility: "visible" });
        case "spin": return [{ transform: base }, { transform: `${base} rotate(360deg)` }];
        default: return [{ transform: base }, { transform: `${base} scale(${GROW})` }];
      }
    }

    // Final look of an effect, applied at once when a step is replayed from the start or taken back.
    function settle(a) {
      for (const card of cardsOf(a.target)) {
        if (a.class === "entrance") card.style.visibility = "visible";
        else if (a.class === "exit") card.style.visibility = "hidden";
        else if (a.effect === "growShrink") card.style.transform = `${baseTransform(card)} scale(${GROW})`;
      }
    }

    function resetCards() {
      for (const a of slide.anims || []) {
        for (const card of cardsOf(a.target)) {
          card.style.visibility = (slide.anims || []).some((o) => o.target === a.target && o.class === "entrance") ? "hidden" : "";
          card.style.transform = baseTransform(card);
        }
      }
    }

    function stopRunning() {
      for (const anim of running) anim.cancel();
      running = [];
    }

    function settleUpTo(count) {
      stopRunning();
      resetCards();
      for (const click of steps.slice(0, count)) for (const group of click) group.forEach(settle);
      index = count;
    }

    function play(click) {
      let start = 0;
      for (const group of click) {
        for (const a of group) {
          for (const card of cardsOf(a.target)) {
            if (a.class === "entrance") card.style.visibility = "visible";
            if (typeof card.animate !== "function") {
              settle(a);
              continue;
            }
            const anim = card.animate(keyframes(card, a), { duration: Math.max(1, a.durationMs), delay: start + a.delayMs, fill: "both", easing: "ease-in-out" });
            running.push(anim);
            anim.finished.then(() => { if (a.class === "exit") card.style.visibility = "hidden"; settle(a); }, () => {});
          }
        }
        start += Math.max(0, ...group.map((a) => a.delayMs + a.durationMs));
      }
    }

    function finishRunning() {
      for (const anim of running) anim.finish();
      running = [];
    }

    return {
      // Prepares a slide frame; `complete` shows the slide as it is after its last click.
      start(frame, shownSlide, { complete = false } = {}) {
        layer = frame.querySelector(".slide-canvas-layer--static");
        slide = shownSlide;
        steps = layer && Array.isArray(slide.anims) ? clickSteps(slide.anims) : [];
        index = 0;
        running = [];
        if (steps.length === 0) return;
        settleUpTo(complete ? steps.length : 0);
        const first = slide.anims[0];
        if (!complete && first.start !== "onClick") {
          play(steps[0]);
          index = 1;
        }
      },
      hasStep: () => index < steps.length,
      // Plays the next click; false when the slide has none left.
      next() {
        if (index >= steps.length) return false;
        finishRunning();
        play(steps[index]);
        index += 1;
        return true;
      },
      // Takes the last played click back; false at the start of the slide.
      back() {
        if (index <= 0) return false;
        settleUpTo(index - 1);
        return true;
      },
      clickCount: () => steps.length,
      playedCount: () => index,
    };
  }

  host.__EASYOFFICE_CREATE_SHOW_ANIM__ = createShowAnim;
  host.__EASYOFFICE_SHOW_CLICK_STEPS__ = clickSteps;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
