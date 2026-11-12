// Input and page helpers shared by the scenario modules of scripts/bench-webui.mjs.
import path from "node:path";

const KEYS = {
  Enter: [13, "Enter"], Tab: [9, "Tab"], Escape: [27, "Escape"], Home: [36, "Home"], End: [35, "End"],
  ArrowLeft: [37, "ArrowLeft"], ArrowUp: [38, "ArrowUp"], ArrowRight: [39, "ArrowRight"], ArrowDown: [40, "ArrowDown"],
  F2: [113, "F2"], a: [65, "KeyA"], f: [70, "KeyF"], z: [90, "KeyZ"], Backspace: [8, "Backspace"],
};

/** Modifiers: 1 Alt, 2 Ctrl, 4 Meta, 8 Shift. */
export async function press(cdp, name, modifiers = 0) {
  const [vk, code] = KEYS[name];
  const base = { key: name, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}

export async function typeChar(cdp, ch) {
  const vk = ch.toUpperCase().charCodeAt(0);
  await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: ch, text: ch, windowsVirtualKeyCode: vk });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: ch, windowsVirtualKeyCode: vk });
}

export async function insertText(cdp, text) {
  await cdp.send("Input.insertText", { text });
}

export async function click(cdp, selector) {
  const r = await cdp.evalJs(`(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; el.scrollIntoView({ block: "nearest" }); const b = el.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
  if (!r) throw new Error("not found: " + selector);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: r.x, y: r.y });
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: r.x, y: r.y, button: "left", clickCount: 1 });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r.x, y: r.y, button: "left", clickCount: 1 });
}

/** Waits for `n` frames (style, layout and paint of the last change have run by then). */
export function settle(cdp, frames, n = 2) {
  return cdp.evalJs(frames(n));
}

/** Loads one benchmark input into the page (outside any measured step). */
export async function stageFile(cdp, file) {
  await cdp.evalJs(`(async () => { window.__staged = new Uint8Array(await (await fetch("/__bench/${path.basename(file)}")).arrayBuffer()); return window.__staged.length; })()`);
}

/** Opens the staged bytes through the workspace entry point (the same one the picker and drop use). */
export function openStaged(cdp, name, frames) {
  return cdp.evalJs(`(async () => { const r = await window.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(${JSON.stringify(name)}, window.__staged); await ${frames(2)}; return JSON.stringify(r); })()`);
}

/** Runs the menu command whose ribbon-catalog id is `commandId` for the module. */
export function runCommand(cdp, mod, commandId) {
  return cdp.evalJs(`(() => { const cat = window.__EASYOFFICE_RIBBON_CATALOG__; const el = [...document.querySelectorAll(".md-item[data-studio-menu-cmd]")].find((e) => cat.resolveMenuCommand(e.dataset.studioMenuCmd, ${JSON.stringify(mod)}) === ${JSON.stringify(commandId)}); if (!el) return "missing"; el.click(); return "ok"; })()`);
}

/** Self time per function of one step, from the sampling profiler (top `limit` rows). */
export async function profile(cdp, step, limit = 12) {
  await cdp.send("Profiler.enable");
  await cdp.send("Profiler.setSamplingInterval", { interval: 200 });
  await cdp.send("Profiler.start");
  await step();
  const { result } = await cdp.send("Profiler.stop");
  const nodes = new Map(result.profile.nodes.map((n) => [n.id, n]));
  const self = new Map();
  const dt = result.profile.timeDeltas;
  result.profile.samples.forEach((id, i) => {
    const f = nodes.get(id).callFrame;
    const key = `${f.functionName || "(anon)"} ${f.url.split("/").pop()}:${f.lineNumber + 1}`;
    self.set(key, (self.get(key) || 0) + (dt[i] || 0));
  });
  await cdp.send("Profiler.disable");
  return [...self].sort((a, b) => b[1] - a[1]).slice(0, limit).map(([k, us]) => `${(us / 1000).toFixed(1).padStart(8)} ms  ${k}`);
}
