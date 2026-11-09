import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { readText } from "./test-support/read-text.mjs";

// The size limits a file meets on its way in (PD-150): the page checks before it sends, the engine
// checks what it receives. The two sides have to name the same numbers, so this reads both.

const read = (relative) => readText(fileURLToPath(new URL(relative, import.meta.url)));
const FILE_OPEN_JS = read("../crates/easyoffice-app/ui/file-open.js");
const HWP_SESSION_JS = read("../crates/easyoffice-app/ui/hwp-session.js");
const CONVERT_OPEN_RS = read("../crates/easyoffice-app/src/convert_open.rs");
const WINDOW_COMMANDS_RS = read("../crates/easyoffice-desktop/src/window_commands.rs");
const MIB = 1024 * 1024;

const constant = (source, pattern, what) => {
  const match = pattern.exec(source);
  assert.ok(match, `${what} not found`);
  return Number(match[1]) * MIB;
};

test("the page's source limit for files sent through IPC equals the engine's", () => {
  const page = constant(FILE_OPEN_JS, /const MAX_FILE_BYTES = (\d+) \* 1024 \* 1024;/, "file-open.js MAX_FILE_BYTES");
  const engine = constant(CONVERT_OPEN_RS, /const MAX_SOURCE_BYTES: usize = (\d+) \* 1_048_576;/, "convert_open.rs MAX_SOURCE_BYTES");
  assert.equal(page, engine);
  assert.match(FILE_OPEN_JS, new RegExp(`const MAX_FILE_LABEL = "${page / MIB} MiB";`), "the label names the same number");
});

test("the Hangul editor limit is one number in both page files and fits the shell's open limit", () => {
  const fileOpen = constant(FILE_OPEN_JS, /const MAX_HWP_BYTES = (\d+) \* 1024 \* 1024;/, "file-open.js MAX_HWP_BYTES");
  const session = constant(HWP_SESSION_JS, /const MAX_HWP_BYTES = (\d+) \* 1024 \* 1024;/, "hwp-session.js MAX_HWP_BYTES");
  assert.equal(fileOpen, session);
  for (const source of [FILE_OPEN_JS, HWP_SESSION_JS]) {
    assert.match(source, new RegExp(`const MAX_HWP_LABEL = "${fileOpen / MIB} MiB";`));
  }
  const shell = constant(WINDOW_COMMANDS_RS, /pub const MAX_OPEN_BYTES: u64 = (\d+) \* 1_048_576;/, "window_commands.rs MAX_OPEN_BYTES");
  assert.ok(fileOpen <= shell, "the shell opens at least what the editor accepts");
});
