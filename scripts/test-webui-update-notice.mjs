import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { hasColorLiteral, rulesFor } from "./test-support/css-rules.mjs";

// Update notice (PD-103): a card in the top right corner when the shell reports a newer release.
// The page only reads the state, switches the check and asks the shell to open the release page.

const REPO = new URL("../", import.meta.url);
const read = (rel) => fs.readFileSync(fileURLToPath(new URL(rel, REPO)), "utf8");
const FIRST_LOOK_MS = 3500;
const VIEW = (version, enabled = true) => ({ status: version ? "available" : "idle", version: version ?? null, enabled, supported: true });

const IDLE_COMMANDS = {
  list_recent_documents: () => [], open_document_window: () => null, open_file_in_new_window: () => null,
  take_pending_open: () => null, report_window_state: () => null, close_current_window: () => null, open_recent_document: () => null,
};

async function openWithNotice(handlers) {
  const calls = [];
  const state = { view: VIEW("1.2.0") };
  const commands = {
    ...IDLE_COMMANDS,
    get_update_notice: (args) => { calls.push(["get_update_notice", args]); return state.view; },
    set_update_check: (args) => { calls.push(["set_update_check", args]); state.view = VIEW(args.enabled ? "1.2.0" : null, args.enabled); return state.view; },
    open_release_page: (args) => { calls.push(["open_release_page", args]); return handlers?.open ? handlers.open() : null; },
  };
  const harness = await createWorkspaceHarness({ search: "?module=DOC&win=w-update", shellCommands: commands });
  harness.advanceTimers(FIRST_LOOK_MS);
  await flushAsyncTurns(10);
  return { ...harness, calls, state, card: harness.document.querySelector(".update-notice") };
}

test("a newer release shows hop's card with the version and two actions", async () => {
  const { card, document } = await openWithNotice();
  assert.ok(card, "the card exists");
  assert.equal(card.hidden, false);
  assert.equal(card.getAttribute("role"), "region");
  assert.equal(card.querySelector(".update-notice__title").textContent, "새 버전 1.2.0 사용 가능");
  assert.match(card.querySelector(".update-notice__meta").textContent, /자동으로 설치되지 않습니다/);
  const buttons = card.querySelectorAll(".update-notice__actions .update-notice__button");
  assert.deepEqual(buttons.map((b) => b.textContent), ["다운로드 페이지 열기", "나중에"]);
  assert.ok(buttons[0].classList.contains("update-notice__button--primary"));
  assert.equal(document.body.contains(card), true);
});

test("without a newer release there is no card, and outside the desktop app nothing is created or asked", async () => {
  const idle = await openWithNotice();
  idle.state.view = VIEW(null);
  idle.advanceTimers(20000);
  await flushAsyncTurns(10);
  assert.equal(idle.card.hidden, true);
  const web = await createWorkspaceHarness({ search: "?module=DOC&win=w-web" });
  web.advanceTimers(20000);
  assert.equal(web.document.querySelector(".update-notice"), null);
});

test("opening the release page asks the shell without passing any address", async () => {
  const { card, calls, userClick } = await openWithNotice();
  userClick(card.querySelector(".update-notice__button--primary"));
  await flushAsyncTurns(10);
  assert.deepEqual(calls.filter((c) => c[0] === "open_release_page"), [["open_release_page", {}]]);
});

test("a failing open shows the shell's message on the card and keeps it", async () => {
  const { card, userClick } = await openWithNotice({ open: () => { throw { status: "error", message: "브라우저를 열지 못했습니다." }; } });
  userClick(card.querySelector(".update-notice__button--primary"));
  await flushAsyncTurns(10);
  const error = card.querySelector(".update-notice__error");
  assert.equal(error.hidden, false);
  assert.match(error.textContent, /브라우저를 열지 못했습니다/);
  assert.equal(card.hidden, false);
});

test("'나중에' and Escape on the card hide it for that version; a newer version shows it again", async () => {
  const first = await openWithNotice();
  first.userClick(first.card.querySelectorAll(".update-notice__button")[1]);
  assert.equal(first.card.hidden, true);
  first.sandbox.dispatchWindowEvent("focus");
  await flushAsyncTurns(10);
  assert.equal(first.card.hidden, true, "the same version stays dismissed");
  first.state.view = VIEW("1.3.0");
  first.sandbox.dispatchWindowEvent("focus");
  await flushAsyncTurns(10);
  assert.equal(first.card.querySelector(".update-notice__title").textContent, "새 버전 1.3.0 사용 가능");
  assert.equal(first.card.hidden, false, "a newer version is shown again");
  const second = await openWithNotice();
  second.card.dispatchEvent({ type: "keydown", key: "Escape", preventDefault() {}, stopPropagation() {} });
  assert.equal(second.card.hidden, true, "Escape on the card is 'later'");
});

test("the help menu switch turns the check off in the shell and reflects the answer", async () => {
  const { document, calls, userClick, card } = await openWithNotice();
  const toggle = document.querySelector("[data-update-check-toggle]");
  assert.equal(toggle.hidden, false, "shown once the shell reports it supports the check");
  assert.equal(toggle.getAttribute("aria-checked"), "true");
  userClick(toggle);
  await flushAsyncTurns(10);
  assert.deepEqual(calls.filter((c) => c[0] === "set_update_check"), [["set_update_check", { enabled: false }]]);
  assert.equal(toggle.getAttribute("aria-checked"), "false");
  assert.equal(card.hidden, true, "no notice while the check is off");
});

test("the card is built from tokens only, hop's module names its source, and the shell grants exactly the three commands", () => {
  const rules = rulesFor(read("crates/easyoffice-app/ui/home.css"), [".update-notice"]);
  assert.ok(rules.length >= 8, "the card rules were found");
  for (const { selector, body } of rules) assert.equal(hasColorLiteral(body), false, `${selector} has a literal colour`);
  const js = read("crates/easyoffice-app/ui/update-notice.js");
  assert.match(js, /Adapted from hop/);
  assert.ok(js.includes("Edward Kim"));
  assert.ok(read("THIRD_PARTY_NOTICES.md").includes("ui/update-notice.js"));
  const permissions = JSON.parse(read("crates/easyoffice-desktop/capabilities/main-window.json")).permissions;
  for (const name of ["allow-get-update-notice", "allow-set-update-check", "allow-open-release-page"]) assert.ok(permissions.includes(name), name);
  const html = read("crates/easyoffice-app/ui/index.html");
  assert.match(html, /data-update-check-toggle="true"[^>]*\bhidden\b/, "the switch is hidden until the shell supports it");
});
