import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { hasColorLiteral, rulesFor } from "./test-support/css-rules.mjs";
import { readText } from "./test-support/read-text.mjs";

// Update notice (PD-103): a card in the bottom left corner when the shell reports a newer release.
// The page reads the state, switches the check, asks the shell to install the signed update (the
// shell refuses while a window has unsaved changes) or to open the release page, and polls the
// progress while a download or install runs.

const REPO = new URL("../", import.meta.url);
const read = (rel) => readText(fileURLToPath(new URL(rel, REPO)));
const FIRST_LOOK_MS = 3500;
const VIEW = (version, enabled = true, extra = {}) => ({ status: version ? "available" : "idle", version: version ?? null, enabled, supported: true, ...extra });
const WATCH_MS = 700;

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
    install_update: (args) => { calls.push(["install_update", args]); return handlers?.install ? handlers.install(state) : state.view; },
  };
  const harness = await createWorkspaceHarness({ search: "?module=DOC&win=w-update", shellCommands: commands });
  harness.advanceTimers(FIRST_LOOK_MS);
  await flushAsyncTurns(10);
  return { ...harness, calls, state, card: harness.document.querySelector(".update-notice") };
}

test("a newer release shows the reference app's card with the version and three actions", async () => {
  const { card, document } = await openWithNotice();
  assert.ok(card, "the card exists");
  assert.equal(card.hidden, false);
  assert.equal(card.getAttribute("role"), "region");
  assert.equal(card.querySelector(".update-notice__title").textContent, "새 버전 1.2.0 사용 가능");
  assert.match(card.querySelector(".update-notice__meta").textContent, /서명을 확인한 파일만 설치/);
  const buttons = card.querySelectorAll(".update-notice__actions .update-notice__button");
  assert.deepEqual(buttons.map((b) => b.textContent), ["설치하고 다시 시작", "릴리스 페이지", "나중에"]);
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
  userClick(card.querySelectorAll(".update-notice__button")[1]);
  await flushAsyncTurns(10);
  assert.deepEqual(calls.filter((c) => c[0] === "open_release_page"), [["open_release_page", {}]]);
});

test("a failing open shows the shell's message on the card and keeps it", async () => {
  const { card, userClick } = await openWithNotice({ open: () => { throw Object.assign(new Error("브라우저를 열지 못했습니다."), { status: "error" }); } });
  userClick(card.querySelectorAll(".update-notice__button")[1]);
  await flushAsyncTurns(10);
  const error = card.querySelector(".update-notice__error");
  assert.equal(error.hidden, false);
  assert.match(error.textContent, /브라우저를 열지 못했습니다/);
  assert.equal(card.hidden, false);
});

test("'나중에' and Escape on the card hide it for that version; a newer version shows it again", async () => {
  const first = await openWithNotice();
  first.userClick(first.card.querySelectorAll(".update-notice__button")[2]);
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

test("the card is built from tokens only, the reference app's module names its source, and the shell grants exactly the four commands", () => {
  const rules = rulesFor(read("crates/easyoffice-app/ui/home.css"), [".update-notice"]);
  assert.ok(rules.length >= 8, "the card rules were found");
  for (const { selector, body } of rules) assert.equal(hasColorLiteral(body), false, `${selector} has a literal colour`);
  const js = read("crates/easyoffice-app/ui/update-notice.js");
  assert.match(js, /Adapted from the reference app/);
  assert.ok(js.includes("Edward Kim"));
  assert.ok(read("THIRD_PARTY_NOTICES.md").includes("ui/update-notice.js"));
  const permissions = JSON.parse(read("crates/easyoffice-desktop/capabilities/main-window.json")).permissions;
  for (const name of ["allow-get-update-notice", "allow-set-update-check", "allow-open-release-page", "allow-install-update"]) assert.ok(permissions.includes(name), name);
  assert.equal(permissions.some((name) => name.startsWith("updater:")), false, "the updater plugin's own commands are not granted to a page");
  const html = read("crates/easyoffice-app/ui/index.html");
  assert.match(html, /data-update-check-toggle="true"[^>]*\bhidden\b/, "the switch is hidden until the shell supports it");
});

const INSTALLING = (extra) => VIEW("1.2.0", true, extra);

test("'설치하고 다시 시작' asks the shell to install without passing anything and shows its answer", async () => {
  const { card, calls, userClick, state } = await openWithNotice({
    install: (st) => { st.view = INSTALLING({ status: "downloading", downloadedBytes: 0, totalBytes: null }); return st.view; },
  });
  userClick(card.querySelector(".update-notice__button--primary"));
  await flushAsyncTurns(10);
  assert.deepEqual(calls.filter((c) => c[0] === "install_update"), [["install_update", {}]]);
  assert.equal(state.view.status, "downloading");
  assert.equal(card.querySelector(".update-notice__title").textContent, "버전 1.2.0 준비 중");
  assert.equal(card.querySelectorAll(".update-notice__button").length, 0, "no action while the download runs");
});

test("the shell's refusal for unsaved changes is shown on the card and the card stays", async () => {
  const refusal = "저장하지 않은 변경 사항이 있는 창이 있어 지금은 설치할 수 없습니다. 저장한 뒤 다시 시도하세요.";
  const { card, userClick } = await openWithNotice({ install: () => { throw Object.assign(new Error(refusal), { status: "error" }); } });
  userClick(card.querySelector(".update-notice__button--primary"));
  await flushAsyncTurns(10);
  const error = card.querySelector(".update-notice__error");
  assert.equal(error.hidden, false);
  assert.equal(error.textContent, refusal);
  assert.equal(card.hidden, false);
  assert.equal(card.dataset.state, "available");
  assert.equal(card.querySelector(".update-notice__button--primary").textContent, "설치하고 다시 시작", "it can be tried again");
});

test("the progress is polled while a download runs and ends on the install state", async () => {
  const { card, calls, state, advanceTimers } = await openWithNotice();
  const progressCalls = () => calls.filter((c) => c[0] === "get_update_notice").length;
  state.view = INSTALLING({ status: "downloading", downloadedBytes: 5 * 1024 * 1024, totalBytes: 20 * 1024 * 1024 });
  advanceTimers(20000);
  await flushAsyncTurns(10);
  assert.equal(card.dataset.state, "downloading");
  assert.equal(card.querySelector(".update-notice__meta").textContent, "5.0 MB / 20.0 MB (25%)");
  const before = progressCalls();
  state.view = INSTALLING({ status: "downloading", downloadedBytes: 12 * 1024 * 1024, totalBytes: 20 * 1024 * 1024 });
  advanceTimers(WATCH_MS);
  await flushAsyncTurns(10);
  assert.ok(progressCalls() > before, "asks again while downloading");
  assert.equal(card.querySelector(".update-notice__meta").textContent, "12.0 MB / 20.0 MB (60%)");
  state.view = INSTALLING({ status: "downloading", downloadedBytes: 2048, totalBytes: null });
  advanceTimers(WATCH_MS);
  await flushAsyncTurns(10);
  assert.equal(card.querySelector(".update-notice__meta").textContent, "2 KB 다운로드됨");
  state.view = INSTALLING({ status: "installing" });
  advanceTimers(WATCH_MS);
  await flushAsyncTurns(10);
  assert.equal(card.querySelector(".update-notice__title").textContent, "버전 1.2.0 설치 중");
  const during = progressCalls();
  state.view = VIEW("1.2.0");
  advanceTimers(WATCH_MS);
  await flushAsyncTurns(10);
  const afterEnd = progressCalls();
  assert.equal(card.dataset.state, "available");
  advanceTimers(WATCH_MS * 5);
  await flushAsyncTurns(10);
  assert.equal(progressCalls(), afterEnd, "no polling once nothing is running");
  assert.ok(afterEnd > during);
});

test("a running install cannot be dismissed; a failed one shows the reason with retry, release page and close", async () => {
  const { card, calls, state, userClick, advanceTimers } = await openWithNotice();
  state.view = INSTALLING({ status: "downloading", downloadedBytes: 1, totalBytes: 10 });
  advanceTimers(20000);
  await flushAsyncTurns(10);
  card.dispatchEvent({ type: "keydown", key: "Escape", preventDefault() {}, stopPropagation() {} });
  assert.equal(card.hidden, false, "Escape does not hide a running install");

  state.view = INSTALLING({ status: "error", message: "업데이트 파일의 서명을 확인하지 못해 설치하지 않았습니다." });
  advanceTimers(WATCH_MS);
  await flushAsyncTurns(10);
  assert.equal(card.querySelector(".update-notice__title").textContent, "버전 1.2.0 업데이트 실패");
  assert.equal(card.querySelector(".update-notice__body").textContent, "업데이트 파일의 서명을 확인하지 못해 설치하지 않았습니다.");
  const buttons = card.querySelectorAll(".update-notice__button");
  assert.deepEqual(buttons.map((b) => b.textContent), ["다시 시도", "릴리스 페이지", "닫기"]);
  state.view = INSTALLING({ status: "downloading", downloadedBytes: 0, totalBytes: null });
  userClick(buttons[0]);
  await flushAsyncTurns(10);
  assert.equal(calls.filter((c) => c[0] === "install_update").length, 1, "retry asks the shell again");
  assert.equal(card.dataset.state, "downloading");
});

test("an install held back by unsaved changes tells the user to restart and can be closed", async () => {
  const { card, state, userClick, advanceTimers } = await openWithNotice();
  state.view = INSTALLING({ status: "installed" });
  advanceTimers(20000);
  await flushAsyncTurns(10);
  assert.equal(card.querySelector(".update-notice__title").textContent, "버전 1.2.0 설치 완료");
  assert.match(card.querySelector(".update-notice__body").textContent, /저장한 뒤 앱을 다시 시작하세요/);
  userClick(card.querySelector(".update-notice__button"));
  assert.equal(card.hidden, true);
});

test("a state the page does not know, or one without a version, shows no card", async () => {
  const { card, state, advanceTimers } = await openWithNotice();
  for (const view of [{ ...VIEW("1.2.0"), status: "unknown" }, { ...VIEW("1.2.0"), status: "downloading", version: null }, { status: "error" }]) {
    state.view = view;
    advanceTimers(20000);
    await flushAsyncTurns(10);
    assert.equal(card.hidden, true, JSON.stringify(view));
  }
});
