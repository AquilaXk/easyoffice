import assert from "node:assert/strict";

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
    assert.deepEqual(await studio.locator("#menu-bar .menu-item > .menu-title").allTextContents(), ["파일", "편집", "보기", "입력", "서식", "쪽", "표", "검토", "도구", "도움말"]);
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
