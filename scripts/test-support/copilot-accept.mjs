// Helpers for the copilot page tests (PD-268, PD-385): an edit answer shows what it would change and waits for 바꾸기, and the
// pointer and typing run only when the user has not asked for reduced motion.
import { flushAsyncTurns } from "../webui-dom-harness.mjs";

/** `matchMedia` for the harness: reduced motion on, so an accepted change is applied at once with no timers. */
export const REDUCED_MOTION = (query) => ({
  matches: /prefers-reduced-motion/.test(String(query)),
  media: String(query),
  addEventListener() {},
  removeEventListener() {},
  addListener() {},
  removeListener() {},
});

/** The same, with motion allowed (the animation runs on the harness's virtual timers). */
export const FULL_MOTION = (query) => ({ ...REDUCED_MOTION(query), matches: false });

export const reviewButton = (document, action) => Array.from(document.querySelectorAll(`#copilot-thread button[data-copilot-action="${action}"]`)).at(-1);

/**
 * Settles the latest answer that is waiting and lets the apply finish: 바꾸기 for a change that was only previewed, 유지 for
 * a draft (new content only; it is in the document already). Returns whether there was one to settle.
 */
export async function acceptProposal(document, userClick, turns = 16) {
  const button = reviewButton(document, "accept") || reviewButton(document, "keep");
  if (!button) return false;
  userClick(button);
  await flushAsyncTurns(turns);
  return true;
}

export async function rejectProposal(document, userClick, turns = 6) {
  const button = reviewButton(document, "reject");
  if (!button) return false;
  userClick(button);
  await flushAsyncTurns(turns);
  return true;
}
