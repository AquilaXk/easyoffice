import assert from "node:assert/strict";
import test from "node:test";
import {
  comboFromKeyEvent, createShellShortcutRelay, matchShellShortcut, parseShellKeys, SHELL_SHORTCUT_REASON,
} from "../apps/hwp-studio/src/embed/shell-shortcuts.ts";

// Shortcut forwarding of the embedded HWP editor (PD-088): only Ctrl/Cmd combos the shell asked for
// are handed over, as a combo string over the embed port; typing never leaves the editor.

const key = (init) => ({ key: "", code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...init });
const WANTED = ["Ctrl+S", "Ctrl+O", "Ctrl+N", "Ctrl+K", "Ctrl+W"];

test("the shell's key list is parsed strictly from the frame URL", () => {
  assert.deepEqual(parseShellKeys("?shellKeys=Ctrl%2BS,Ctrl%2BO,Ctrl%2BK"), ["Ctrl+S", "Ctrl+O", "Ctrl+K"]);
  assert.deepEqual(parseShellKeys("?shellKeys=Ctrl%2BS,Ctrl%2BS"), ["Ctrl+S"], "duplicates collapse");
  assert.deepEqual(parseShellKeys("?shellKeys=a,Ctrl%2B,Delete,Ctrl%2BEnter,<script>"), [], "free text and named keys are refused");
  assert.deepEqual(parseShellKeys(""), []);
  const many = Array.from({ length: 40 }, (_, i) => "Ctrl+" + String.fromCodePoint(65 + (i % 26)));
  const parsed = parseShellKeys("?shellKeys=" + encodeURIComponent(many.join(",")));
  assert.ok(parsed.length <= 16, "the list is capped");
});

test("combo strings use the shell's normalisation (Cmd counts as Ctrl)", () => {
  assert.equal(comboFromKeyEvent(key({ key: "s", ctrlKey: true })), "Ctrl+S");
  assert.equal(comboFromKeyEvent(key({ key: "S", metaKey: true })), "Ctrl+S");
  assert.equal(comboFromKeyEvent(key({ key: "z", ctrlKey: true, shiftKey: true })), "Ctrl+Shift+Z");
  assert.equal(comboFromKeyEvent(key({ key: "∂", code: "KeyD", ctrlKey: true, altKey: true })), "Ctrl+Alt+D");
  assert.equal(comboFromKeyEvent(key({ key: "s" })), null, "no modifier, no combo");
  assert.equal(comboFromKeyEvent(key({ key: "Enter", ctrlKey: true })), null, "named keys are never combos");
});

test("only requested Ctrl combos match; typing, IME input and other shortcuts stay in the editor", () => {
  for (const k of ["s", "o", "n", "k", "w"]) assert.equal(matchShellShortcut(key({ key: k, ctrlKey: true }), WANTED), "Ctrl+" + k.toUpperCase());
  assert.equal(matchShellShortcut(key({ key: "c", ctrlKey: true }), WANTED), null, "copy stays in the editor");
  assert.equal(matchShellShortcut(key({ key: "z", ctrlKey: true }), WANTED), null, "undo stays in the editor");
  assert.equal(matchShellShortcut(key({ key: "s", ctrlKey: true, shiftKey: true }), WANTED), null, "Ctrl+Shift+S is another combo");
  for (const typed of ["s", "S", "가", "ㅅ", " ", "1"]) assert.equal(matchShellShortcut(key({ key: typed }), WANTED), null, "typing " + typed);
  assert.equal(matchShellShortcut(key({ key: "s", ctrlKey: true, isComposing: true }), WANTED), null, "never while composing");
  assert.equal(matchShellShortcut(key({ key: "q", ctrlKey: true, altKey: true }), WANTED), null, "AltGr input is typing");
  assert.equal(matchShellShortcut(key({ key: "s", ctrlKey: true }), []), null, "nothing is forwarded without a key list");
});

test("the relay delivers the combo string to the embed port listeners only", () => {
  const relay = createShellShortcutRelay();
  assert.equal(relay.hasListener(), false);
  const seen = [];
  const off = relay.add((payload) => seen.push(payload));
  assert.equal(relay.hasListener(), true);
  relay.forward("Ctrl+S");
  assert.deepEqual(seen, [{ schemaVersion: 1, reason: SHELL_SHORTCUT_REASON, shortcut: "Ctrl+S" }]);
  assert.deepEqual(Object.keys(seen[0]).sort((a, b) => a.localeCompare(b)), ["reason", "schemaVersion", "shortcut"], "no key event, no text, no target");
  off();
  relay.forward("Ctrl+O");
  assert.equal(seen.length, 1);
  assert.equal(relay.hasListener(), false);
});
