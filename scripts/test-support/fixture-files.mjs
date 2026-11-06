// Shared WebUI test helpers for feeding fixture documents through the file-open input.
import { flushAsyncTurns } from "../webui-dom-harness.mjs";

const encoder = new TextEncoder();

/** A File-like object whose bytes carry a structured model the offline decoder understands. */
export function makeFixtureFile(name, model) {
  const bytes = encoder.encode("EOFIXTURE:" + JSON.stringify(model));
  return {
    name,
    size: bytes.length,
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  };
}

/** Selects `file` in the workspace file-open input and waits for the open pipeline to settle. */
export async function pickFile(document, file, turns = 60) {
  const input = document.getElementById("file-open-input");
  input.files = [file];
  input.dispatchEvent({ type: "change", target: input });
  await flushAsyncTurns(turns);
}
