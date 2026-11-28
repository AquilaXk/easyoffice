import assert from "node:assert/strict";
import { editorCall, openSampleWithPanel } from "./hwp.mjs";

// The Hangul editor's own menu row in a real WebKit (PD-396): it is the window's one menu bar outside macOS (the desktop shell
// has no native bar there) and in a browser, it names the official 한글 menus and keys, and it gives way when the frame URL says
// that the macOS desktop shell draws the native bar (`nativeMenu=1`).

const READY_MS = 40000;
const STUDIO_URL = "/hwp-studio/index.html";

async function connectedEditor(page) {
  await page.waitForSelector("#hwp-frame");
  await page.waitForFunction(() => document.querySelector(".hwp-host-status")?.hidden === true, undefined, { timeout: READY_MS });
  return page.frameLocator("#hwp-frame");
}

/** A menu item's text with the Mac modifier symbols turned back into names (WebKit on a Mac shows ⌘ and ⌥). */
const keyText = (text) => text.replaceAll("⌘", "Ctrl+").replaceAll("⌥", "Alt+").replaceAll("⇧", "Shift+");

export const hwpMenuSteps = [
  ["the editor's menu row is the window's menu bar outside the macOS desktop app, with the official menus and keys", async (page) => {
    const studio = await connectedEditor(page);
    assert.equal(await studio.locator("#menu-bar").isVisible(), true, "the editor's own menu row is shown");
    assert.deepEqual(await studio.locator("#menu-bar .menu-item > .menu-title").allTextContents(), ["파일", "편집", "보기", "입력", "서식", "쪽", "표", "보안", "검토", "도구", "도움말"]);
    assert.equal(await studio.locator('#menu-bar .menu-item[data-menu="security"] .md-item[data-cmd="file:set-password"]').count(), 1, "보안 ▸ 문서 암호 설정");
    const item = (cmd) => studio.locator(`#menu-bar .md-item[data-cmd="${cmd}"]`);
    const shortcut = async (cmd) => keyText((await item(cmd).locator(".md-shortcut").textContent()) ?? "");
    assert.equal(await shortcut("insert:symbols"), "Ctrl+F10", "문자표");
    assert.equal(await shortcut("insert:hyperlink"), "Ctrl+K,H", "하이퍼링크");
    assert.equal(await shortcut("insert:field"), "Ctrl+K,E", "필드 입력");
    assert.equal(await shortcut("table:insert-row-col"), "Alt+Insert", "줄/칸 추가하기");
    assert.equal(await shortcut("table:delete-row-col"), "Alt+Delete", "줄/칸 지우기");
    assert.equal((await item("view:zoom-dialog").locator(".md-label").textContent()).trim(), "화면 확대/축소");
    assert.equal((await item("table:insert-row-col").locator(".md-label").textContent()).trim(), "줄/칸 추가하기");
    assert.equal(await item("table:insert-row-above").count(), 0, "the six direct row and column items are the two dialogs now");
    assert.equal(await item("table:delete-col").count(), 0);
    const pageMenu = studio.locator('#menu-bar .menu-item[data-menu="page"]');
    assert.deepEqual(await pageMenu.locator(":scope > .menu-dropdown > .md-sub > .md-label").allTextContents(), ["머리말/꼬리말", "단"]);
    assert.deepEqual(await pageMenu.locator(":scope > .menu-dropdown > .md-sub > .md-sub-panel > .md-sub > .md-label").allTextContents(), ["머리말", "꼬리말"], "머리말 and 꼬리말 are inside the one official item");
    assert.equal(await studio.locator('#menu-bar .menu-item[data-menu="review"] .md-item[data-cmd="edit:document-history"]').count(), 1, "문서 이력 관리 is in 검토");
    const tool = studio.locator('#menu-bar .menu-item[data-menu="tool"] .menu-dropdown');
    assert.equal(await tool.locator('.md-item[data-shell-cmd="theme-dark"]').count(), 1, "the theme follows 환경 설정");
    assert.equal(await studio.locator('#menu-bar .menu-item[data-menu="view"] [data-shell-cmd^="theme-"]').count(), 0, "the theme is not in 보기");
  }],

  ["보안 ▸ 문서 암호 설정: the reference dialog sets a password in the editor, the protected bytes need it to read back, and the shell sees only a flag", async (page) => {
    await openSampleWithPanel(page);
    const studio = page.frameLocator("#hwp-frame");
    const before = await editorCall(page, "getSaveProtection", {});
    assert.deepEqual(before, { passwordProtected: false, passwordSet: false });
    await assert.rejects(editorCall(page, "exportHwpProtected", {}), /암호가 설정되어 있지 않습니다/, "nothing is exported without a password");
    // The command opens the dialog and answers when it is closed, so it is started and not awaited.
    void editorCall(page, "automation.execute", { id: "file:set-password", allowDialog: true }).catch(() => {});
    await studio.locator("#hwp-save-password-input").fill("12345");
    await studio.locator("#hwp-save-password-confirmation").fill("different");
    await studio.locator(".dialog-btn-primary").last().click();
    assert.equal(await studio.locator("#hwp-save-password-input").count(), 1, "a confirmation that differs keeps the dialog open");
    await studio.locator("#hwp-save-password-confirmation").fill("12345");
    await studio.locator(".dialog-btn-primary").last().click();
    await page.waitForFunction(async () => (await window.__eoCall("getSaveProtection", {})).passwordSet === true, undefined, { timeout: 10000 });
    assert.deepEqual(await editorCall(page, "getSaveProtection", {}), { passwordProtected: false, passwordSet: true });
    // The bytes stay inside the page: only sizes and verdicts come back.
    const result = await page.evaluate(async () => {
      const bytes = new Uint8Array(await window.__eoCall("exportHwpProtected", {}));
      const plain = new Uint8Array(await window.__eoCall("exportHwp", {}));
      const verdict = await window.__eoCall("verifyProtectedBytes", { data: bytes.slice() });
      const withoutPassword = await window.__eoCall("verifyBytes", { data: bytes.slice() }).then(() => "read", () => "refused");
      const seen = JSON.stringify([await window.__eoCall("getSaveProtection", {}), verdict]);
      return {
        size: bytes.length, magic: [bytes[0], bytes[1]], verdict, withoutPassword,
        sameAsPlain: bytes.length === plain.length && bytes.every((value, index) => value === plain[index]),
        leaked: seen.includes("12345"),
      };
    });
    assert.ok(result.size > 100 && result.magic[0] === 0xd0 && result.magic[1] === 0xcf, "an HWP 5.0 container");
    assert.equal(result.verdict.ok, true);
    assert.ok(result.verdict.pageCount > 0);
    assert.equal(result.withoutPassword, "refused", "without the password the same bytes cannot be read");
    assert.equal(result.sameAsPlain, false);
    assert.equal(result.leaked, false, "the password appears nowhere in what the shell can read");
    // The password belongs to this document: a new one does not carry it.
    await editorCall(page, "createBlankDocument", { skipUnsavedGuard: true });
    assert.deepEqual(await editorCall(page, "getSaveProtection", {}), { passwordProtected: false, passwordSet: false });
    await assert.rejects(editorCall(page, "exportHwpProtected", {}), /암호가 설정되어 있지 않습니다/);
  }],

  ["the editor hides its own menu row when the shell says the native menu bar is drawn (nativeMenu=1) and keeps the toolbar", async (page) => {
    await page.goto(`${page.url().split("/?")[0]}${STUDIO_URL}?nativeMenu=1`);
    await page.waitForSelector("#icon-toolbar");
    assert.equal(await page.locator("#menu-bar").isVisible(), false, "no second menu row");
    assert.equal(await page.locator("#icon-toolbar").isVisible(), true, "the toolbar stays");
    assert.equal(await page.evaluate(() => document.documentElement.dataset.nativeMenu), "true");
    await page.goto(`${page.url().split("/hwp-studio/")[0]}${STUDIO_URL}`);
    await page.waitForSelector("#icon-toolbar");
    assert.equal(await page.locator("#menu-bar").isVisible(), true, "without the flag the row is shown");
  }],
];
