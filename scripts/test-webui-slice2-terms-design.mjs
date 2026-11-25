import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createSelection } from "./test-support/doc-run-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { readText } from "./test-support/read-text.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const readRepo = (rel) => readText(path.join(REPO, rel));

test("Slice 2: HWP vs Office vs .pages official Korean terminology and lossless h1..h6 roundtrip", async () => {
  let savedPayload = null;
  const sel = createSelection();
  const { document, sandbox, host, userClick } = await createWorkspaceHarness({
    selection: sel,
    search: "?module=DOC&win=w-slice2-terms",
    testIpc: async (command, rawPayload) => {
      if (command === "convert_document_format") {
        const req = JSON.parse(rawPayload || "{}");
        if (req.action === "decode" && String(req.fileName || "").endsWith(".pages")) {
          return {
            status: "OK",
            module: "DOC",
            format: "pages",
            kind: "blocks",
            itemCount: 5,
            title: "apple-sample.pages",
            blocks: [
              { type: "paragraph", heading: 1, runs: [{ text: "큰 제목" }] },
              { type: "paragraph", heading: 2, runs: [{ text: "작은 부제" }] },
              { type: "paragraph", heading: 3, runs: [{ text: "머리말 삼" }] },
              { type: "paragraph", heading: 5, runs: [{ text: "다섯 단계" }] },
              { type: "paragraph", runs: [{ text: "본문 문단" }] },
            ],
            pageSetup: { widthTwips: 11906, heightTwips: 16838, marginLeftTwips: 1440, marginRightTwips: 1440, marginTopTwips: 1440, marginBottomTwips: 1440 },
          };
        }
        if (req.forSave || Array.isArray(req.blocks)) {
          savedPayload = req;
          return { status: "OK", convertedBase64: "AAAA", sourceFormat: req.sourceFormat || "docx", targetFormat: req.targetFormat || "docx", sourceBytes: 100, outputBytes: 1200, specification: "OOXML" };
        }
      }
      const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
        getActiveModule: () => document.getElementById("app").dataset.module,
        ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
        ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
        contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
        moduleSpecs: cat.MODULE_SPECS,
        localRibbonState: cat.createDefaultRibbonState(),
      });
      return offline[command](rawPayload);
    },
  });
  await flushAsyncTurns(16);

  const styleSelect = document.getElementById("doc-style-select");
  const styles = () => [...styleSelect.querySelectorAll("option")].map((o) => o.value);
  const paperMenuLabel = () => document.querySelector('#menu-bar .md-item[data-kinds="DOC SLIDE"][data-studio-menu-cmd="layout-paper"] .md-label')?.textContent;

  // 1. DOC (Word official terms)
  assert.deepEqual(styles(), ["표준", "제목 1", "제목 2", "제목 3", "제목 4", "제목 5", "제목 6"]);
  assert.equal(paperMenuLabel(), "용지 설정");
  assert.equal(document.getElementById("btn-stylebar-bold").getAttribute("title"), "굵게 (Ctrl+B)");
  assert.equal(document.getElementById("btn-stylebar-italic").getAttribute("title"), "기울임꼴 (Ctrl+I)");

  // 2. HWP (Hangul ko.ts terms)
  switchModule(document, "HWP");
  await flushAsyncTurns(8);
  assert.deepEqual(styles(), ["바탕글", "개요 1", "개요 2", "개요 3", "개요 4", "개요 5", "개요 6"]);
  assert.equal(document.getElementById("btn-stylebar-bold").getAttribute("title"), "진하게 (Ctrl+B)");
  assert.equal(document.getElementById("btn-stylebar-italic").getAttribute("title"), "기울임 (Ctrl+I)");
  assert.equal(document.getElementById("status-zoom-label").textContent, "100%");

  // 3. Open .pages in DOC -> Apple Pages Korean flavor
  switchModule(document, "DOC");
  await flushAsyncTurns(8);
  await pickFile(document, makeFixtureFile("sample.pages", "pages-bytes"));
  await flushAsyncTurns(20);
  const editor = document.getElementById("doc-editor");
  assert.equal(editor.dataset.eoDocFlavor, "pages");
  assert.deepEqual(styles(), ["본문", "제목", "부제목", "머리말 1", "머리말 2", "머리말 3", "제목 4", "제목 5", "제목 6"]);
  assert.equal(paperMenuLabel(), "페이지 설정");
  assert.equal(document.querySelector('#menu-bar .md-item[data-kinds="DOC"][data-studio-menu-cmd="mod-header"] .md-label').textContent, "머리말");
  assert.equal(document.querySelector('#menu-bar .md-item[data-kinds="DOC"][data-studio-menu-cmd="mod-footer"] .md-label').textContent, "꼬리말");

  // Style application keeps lossless h1/h2/h5 tags and saves them cleanly
  const firstHeading = editor.querySelector("h1");
  assert.ok(firstHeading);
  sel.addRange({ startContainer: firstHeading.firstChild, startOffset: 0, endContainer: firstHeading.firstChild, endOffset: 1, collapsed: false });
  styleSelect.value = "제목";
  styleSelect.dispatchEvent({ type: "change", target: styleSelect });
  await flushAsyncTurns(6);
  assert.equal(editor.children[0].tagName, "H1");
  assert.equal(editor.children[0].dataset.eoPagesStyle, "제목");

  const fifthHeading = editor.querySelector("h5");
  assert.ok(fifthHeading);
  sel.addRange({ startContainer: fifthHeading.firstChild, startOffset: 0, endContainer: fifthHeading.firstChild, endOffset: 1, collapsed: false });
  styleSelect.value = "제목 6";
  styleSelect.dispatchEvent({ type: "change", target: styleSelect });
  await flushAsyncTurns(6);
  assert.equal(editor.querySelector("h6")?.textContent, "다섯 단계");

  // Open paper dialog in .pages flavor -> shows "페이지 설정"
  userClick(document.querySelector('#menu-bar .md-item[data-kinds="DOC"][data-studio-menu-cmd="layout-paper"]'));
  await flushAsyncTurns(6);
  const paperDlg = document.getElementById("doc-page-setup-dialog");
  assert.equal(paperDlg.querySelector(".dialog-title span").textContent, "페이지 설정");
  paperDlg.close();

  // Save payload retains h1..h6 headings cleanly
  const savePayloadApi = host.__EASYOFFICE_SAVE_PAYLOAD__;
  const docBlocks = savePayloadApi.collectDocBlocks(document.getElementById("doc-editor"));
  assert.deepEqual(
    docBlocks.filter((b) => b.type === "paragraph").map((b) => b.heading ?? null),
    [1, 2, 3, 6, null]
  );
  userClick(document.querySelector('#menu-bar .md-item[data-studio-menu-cmd="studio-save"]'));
  await flushAsyncTurns(8);
  assert.equal(savedPayload?.blocks?.filter((b) => b.heading === 6).length, 1);
});

test("Slice 2: Design system icons, token SVGs, BASE/BOARD custom selects, and clean Korean copy", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=BASE&win=w-slice2-design" });
  await flushAsyncTurns(16);

  // Ruler pins accurately named
  assert.equal(document.getElementById("ruler-pin-left-margin").getAttribute("title"), "왼쪽 들여쓰기");
  assert.equal(document.getElementById("ruler-pin-right-margin").getAttribute("title"), "오른쪽 들여쓰기");

  // No trash emoji or symbol glyphs in icons/buttons, PDF stamp normalized
  const html = readRepo("crates/easyoffice-app/ui/index.html");
  assert.equal(html.includes("🗑"), false);
  assert.equal(html.includes("무엇을 도와드릴까요?"), false);
  assert.equal(html.includes("이 모듈"), false);
  assert.equal(html.includes("..."), false, "ellipses unified to … in index.html");
  assert.equal(html.includes("공문서 바탕글"), false);
  assert.equal(html.includes("승인 완료"), false);
  assert.ok(html.includes('value="승인필"'), "PDF stamp uses 승인필");

  // No internal "이 모듈" in UI or desktop confirm dialog
  for (const rel of ["crates/easyoffice-desktop/src/confirm_dialog.rs", "crates/easyoffice-app/ui/office-print.js", "crates/easyoffice-app/ui/app.js"]) {
    assert.equal(readRepo(rel).includes("이 모듈"), false, `${rel} removes internal phrase`);
  }

  // Page setup SVGs and board marker use CSS tokens, no hex colors
  const dlgSvgMatches = [...html.matchAll(/<span class="icon-radio-icon">(<svg[\s\S]*?<\/svg>)<\/span>/g)].map((m) => m[1]);
  assert.equal(dlgSvgMatches.length, 7);
  for (const svg of dlgSvgMatches) {
    assert.equal(/#[0-9a-fA-F]{3,6}\b/.test(svg), false, "SVG uses design tokens, not literal hex");
  }
  const boardJs = readRepo("crates/easyoffice-app/ui/board-studio.js");
  assert.equal(/#335095/i.test(boardJs), false, "board-studio arrow uses tokens.css");

  // BASE & BOARD dynamic selects use custom-select wrapper
  for (const sel of document.querySelectorAll("#pane-base select")) {
    assert.ok(sel.classList.contains("sb-combo") && sel.classList.contains("native-select-hidden"), `BASE select ${sel.id} enhanced`);
  }
  switchModule(document, "BOARD");
  await flushAsyncTurns(10);
  for (const sel of document.querySelectorAll("#pane-board select")) {
    assert.ok(sel.classList.contains("sb-combo") && sel.classList.contains("native-select-hidden"), `BOARD select ${sel.id} enhanced`);
  }
  // Board toolbar uses semantic 24x24 SVG icons instead of borrowed paste/stamp/grid sprites
  assert.ok(document.querySelector('#btn-board-export-pdf [data-icon="file-export-pdf"] svg.eo-icon'));
  assert.ok(document.querySelector('#btn-board-add-chart [data-icon="chart-bar"] svg.eo-icon'));
  assert.ok(document.querySelector('#btn-board-delete [data-icon="trash"] svg.eo-icon'));
});

test("Slice 2: 보기 ▸ 도구 상자 라벨 표시 toggles toolbar labels across windows and syncs macOS native menu state", async () => {
  const reports = [];
  const { document, host } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-slice2-tblabels",
    platform: "MacIntel",
    shellCommands: {
      report_menu_state: (args) => { reports.push(args); return null; },
      report_window_state: () => null,
      take_pending_open: () => null,
      list_recent_documents: () => [],
    },
  });
  await flushAsyncTurns(14);
  assert.equal(document.documentElement.dataset.toolbarLabels, "shown");
  assert.equal(reports.at(-1).toolbarLabels, true);

  // Toggle off via native menu action
  assert.equal(host.__EASYOFFICE_NATIVE_MENU__("view:toolbar-labels"), true);
  assert.equal(document.documentElement.dataset.toolbarLabels, "hidden");
  assert.equal(document.querySelector("[data-view-toolbar-labels]").getAttribute("aria-checked"), "false");
  assert.equal(reports.at(-1).toolbarLabels, false);

  // Toggle back on via WebUI menu click
  document.querySelector("[data-view-toolbar-labels]").click();
  assert.equal(document.documentElement.dataset.toolbarLabels, "shown");
  assert.equal(document.querySelector("[data-view-toolbar-labels]").getAttribute("aria-checked"), "true");
  assert.equal(reports.at(-1).toolbarLabels, true);
});

test("Slice 2: Command palette filters cross-module/retired commands, sorts results beyond 18, and includes BASE/BOARD/toolbar-labels", async () => {
  const { document, userClick } = await createWorkspaceHarness({
    search: "?module=PDF&win=w-slice2-palette",
    platform: "MacIntel",
  });
  await flushAsyncTurns(14);

  const openAndQuery = (q = "") => {
    const modal = document.getElementById("cmd-palette-modal");
    if (!modal.open) userClick(document.getElementById("btn-command-palette"));
    const input = document.getElementById("cmd-search-input");
    input.value = q;
    input.dispatchEvent({ type: "input", target: input });
    return [...document.querySelectorAll("#cmd-palette-list .cp-item")].map((li) => ({
      title: li.querySelector(".cp-item-label").textContent,
      shortcut: li.querySelector(".cp-item-shortcut")?.textContent ?? "",
    }));
  };

  // PDF filters out editing/sheet/slide commands + retired bench-run/view-ribbon
  const pdfAll = openAndQuery("");
  assert.ok(pdfAll.some((i) => i.title === "다시 실행" && i.shortcut === "⇧⌘Z"));
  assert.ok(pdfAll.some((i) => i.title === "도구 상자 라벨 표시"));
  assert.equal(pdfAll.some((i) => /리본 접기|엔진 성능 진단|자동 합계|굵게/.test(i.title)), false);

  // SLIDE does not truncate at 18 and filters sheet-only commands
  switchModule(document, "SLIDE");
  await flushAsyncTurns(6);
  const slideAll = openAndQuery("");
  assert.ok(slideAll.length > 18, "palette shows > 18 items");
  assert.equal(slideAll.some((i) => /자동 합계|함수 마법사/.test(i.title)), false);

  // HWP uses 되돌리기 and 찾아 바꾸기 while DOC uses 실행 취소 and 찾기 및 바꾸기
  switchModule(document, "HWP");
  await flushAsyncTurns(6);
  assert.ok(openAndQuery("되돌리기").some((i) => i.title === "되돌리기"));
  assert.ok(openAndQuery("찾아 바꾸기").some((i) => i.title === "찾아 바꾸기"));

  switchModule(document, "DOC");
  await flushAsyncTurns(6);
  assert.ok(openAndQuery("실행 취소").some((i) => i.title === "실행 취소"));
  assert.ok(openAndQuery("찾기 및 바꾸기").some((i) => i.title === "찾기 및 바꾸기"));

  // BASE and BOARD core commands searchable
  switchModule(document, "BASE");
  await flushAsyncTurns(6);
  assert.ok(openAndQuery("새 레코드").some((i) => i.title === "새 레코드"));
  switchModule(document, "BOARD");
  await flushAsyncTurns(6);
  assert.ok(openAndQuery("직각 연결선").some((i) => i.title === "직각 연결선"));
  assert.ok(openAndQuery("도구 상자 라벨").some((i) => i.title === "도구 상자 라벨 표시"));
});
