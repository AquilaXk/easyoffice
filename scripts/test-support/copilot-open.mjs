// A workspace page with the copilot panel open and a fake provider (PD-268, PD-304): `run(request, options)` answers
// every `copilot_run`, and `play(ms)` moves the harness's virtual timers and the animation clock on together.
import { createWorkspaceHarness, flushAsyncTurns } from "../webui-dom-harness.mjs";
import { REDUCED_MOTION, FULL_MOTION } from "./copilot-accept.mjs";

export async function open({ run, module = "DOC", motion = false, streaming = false, extra = {} }) {
  const seen = [];
  const clock = { now: 0 };
  const testIpc = async (command, payload, options) => {
    if (command === "run_smart_assistant") {
      const parsed = JSON.parse(payload);
      seen.push(parsed);
      if (parsed.action === "copilot_run") return run(parsed, options);
      if (parsed.action === "copilot_cancel") return { status: "ok", cancelled: true };
    }
    const cat = harness.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
    const offline = harness.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
      getActiveModule: () => module,
      ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
      ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
      contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
      moduleSpecs: cat.MODULE_SPECS,
      localRibbonState: cat.createDefaultRibbonState(),
    });
    return offline[command](payload);
  };
  testIpc.streaming = streaming;
  const harness = await createWorkspaceHarness({
    search: `?module=${module}&win=w-copilot-268-${module}`,
    matchMedia: motion ? FULL_MOTION : REDUCED_MOTION,
    // The animation reads time from `performance.now`; the harness's timers are virtual, so the clock is advanced with them.
    hostEnvExtras: { performance: { now: () => clock.now }, ...extra },
    testIpc,
  });
  const { document, userClick, advanceTimers } = harness;
  await flushAsyncTurns(12);
  userClick(document.getElementById("btn-copilot-toggle"));
  userClick(document.getElementById("btn-copilot-model"));
  const ollama = Array.from(document.getElementById("copilot-model-menu").querySelectorAll('button[role="menuitemradio"]')).find((b) => b.dataset.quickKind === "ollama");
  userClick(ollama);
  await flushAsyncTurns(6);
  const input = document.getElementById("copilot-input");
  const send = async (text, turns = 14) => {
    input.value = text;
    input.dispatchEvent({ type: "input", target: input });
    userClick(document.getElementById("btn-copilot-send"));
    await flushAsyncTurns(turns);
  };
  // Moves the virtual clock and the timers on together, letting promises settle between frames.
  const play = async (ms, step = 16) => {
    for (let t = 0; t < ms; t += step) {
      clock.now += step;
      advanceTimers(step);
      await flushAsyncTurns(2);
    }
  };
  const assistants = () => document.querySelectorAll("#copilot-thread .eo-copilot-msg--assistant");
  const editor = document.getElementById("doc-editor");
  return { ...harness, seen, send, play, assistants, editor, input };
}

export const answer = (operations, extra = {}) => ({ status: "ok", mode: "edit", reply: "다듬었습니다", updatedInPlace: true, operations, locations: [], ...extra });
