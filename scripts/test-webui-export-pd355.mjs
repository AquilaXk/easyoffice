import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { pickFile, makeFixtureFile } from "./test-support/fixture-files.mjs";

async function createOffline() {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  return sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
}

test("PDF AcroForm grid stays hidden with empty status for non-form PDFs and activates dynamically for real AcroForm fields", async () => {
  const offline = await createOffline();
  const formApplies = [];
  const { document, userClick } = await createWorkspaceHarness({
    search: "?module=PDF&win=w-pdf-acroform",
    platform: "MacIntel",
    confirm: () => true,
    shellCommands: {
      report_menu_state: () => null,
      confirm_dialog: () => true,
      pdf_edit_open: () => ({ pages: 1, sources: 1, turn: 0, stamp: "", watermark: "" }),
      pdf_edit_apply: (args) => {
        if (args?.op?.kind === "form") formApplies.push(args.op);
        return { pages: 1, turn: 0, stamp: "", watermark: "" };
      },
      pdf_edit_save: () => ({ status: "saved", name: "out.pdf", bytes: 100, pages: 1 }),
    },
    tauriInvoke: async (cmd, { payload }) => JSON.stringify(await offline[cmd](payload)),
  });
  await flushAsyncTurns(10);

  await pickFile(
    document,
    makeFixtureFile("일반.pdf", {
      format: "pdf",
      title: "일반보고서",
      blocks: [{ type: "paragraph", runs: [{ text: "본문 1쪽" }] }],
      pdfOutlines: [],
      pdfForm: { hasAcroForm: false, fieldCount: 0, applicant: null, department: null },
    })
  );
  await flushAsyncTurns(10);

  const grid = document.getElementById("pdf-acroform-grid");
  const status = document.getElementById("pdf-form-status");
  assert.equal(grid.hidden, true, "plain PDF must hide form grid");
  assert.equal(status.textContent, "", "plain PDF must not show fake readiness message");
  assert.equal(document.getElementById("pdf-form-applicant").value, "");
  assert.equal(document.getElementById("pdf-form-dept").value, "");

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(8);
  assert.equal(formApplies.length, 0, "saving plain PDF must not inject synthetic form fields");

  userClick(document.getElementById("btn-pdf-form-toggle"));
  assert.equal(grid.hidden, false, "toggle reveals form grid on demand");

  await pickFile(
    document,
    makeFixtureFile("신청서.pdf", {
      format: "pdf",
      title: "신청서",
      blocks: [{ type: "paragraph", runs: [{ text: "본문 1쪽" }] }],
      pdfOutlines: [],
      pdfForm: { hasAcroForm: true, fieldCount: 2, applicant: "박민준", department: "재무팀" },
    })
  );
  await flushAsyncTurns(10);
  assert.equal(grid.hidden, false, "AcroForm PDF reveals extracted fields");
  assert.equal(document.getElementById("pdf-form-applicant").value, "박민준");
  assert.equal(document.getElementById("pdf-form-dept").value, "재무팀");
  assert.match(status.textContent, /양식 필드 2개 감지됨/);
});

test("파일 ▸ 내보내기 submenu, format dialog, filename sanitization, and .pages overwrite guard enforce inPlace=false", async () => {
  const offline = await createOffline();
  const saves = [];
  const converts = [];
  const { document, userClick } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-export",
    platform: "MacIntel",
    shellCommands: {
      report_menu_state: () => null,
      save_document_file: (args) => {
        saves.push(args);
        return { status: "saved", name: args.fileName, bytes: 12 };
      },
    },
    tauriInvoke: async (cmd, { payload }) => {
      if (cmd !== "convert_document_format") return JSON.stringify(await offline[cmd](payload));
      const req = JSON.parse(payload);
      converts.push(req);
      return JSON.stringify({
        status: "OK",
        convertedBase64: Buffer.from("PK\x03\x04dummy").toString("base64"),
        outputBytes: 12,
        specification: String(req.targetFormat || "").toUpperCase(),
        compatibility: "full",
        warnings: [],
      });
    },
  });
  await flushAsyncTurns(12);

  const bridge = document.defaultView.__EASYOFFICE_WINDOW_BRIDGE__.instance;
  bridge.applyTitles({ ...bridge.getTitles(), DOC: "원본보고서.pages" });

  // Export via menu item -> .pages always keeps inPlace: false and leaves document title unchanged.
  const pagesItem = document.querySelector('.md-item[data-export-format="pages"]');
  assert.ok(pagesItem && !pagesItem.hidden, "DOC shows Pages export menu item");
  userClick(pagesItem);
  await flushAsyncTurns(12);
  assert.equal(converts.at(-1)?.targetFormat, "pages");
  assert.equal(saves.at(-1)?.fileName, "원본보고서.pages");
  assert.equal(saves.at(-1)?.inPlace, false, "explicit .pages export must never overwrite in place");
  assert.equal(bridge.getDocumentName("DOC"), "원본보고서.pages");

  // Format selector dialog sanitizes malicious stems and runs conversion with inPlace: false.
  userClick(document.querySelector(".md-item[data-export-dialog]"));
  await flushAsyncTurns(4);
  const dialog = document.getElementById("export-dialog");
  assert.equal(dialog.open, true);
  const sel = document.getElementById("export-format-select");
  const opts = Array.from(sel.querySelectorAll("option")).map((o) => o.value);
  assert.deepEqual(opts, ["docx", "odt", "rtf", "pages", "md", "txt", "pdf"]);
  sel.value = "odt";
  sel.dispatchEvent({ type: "change" });
  document.getElementById("export-file-name").value = "../../탈출시도\0.docx";
  userClick(document.getElementById("btn-export-run"));
  await flushAsyncTurns(12);
  assert.equal(saves.at(-1)?.fileName, "탈출시도.odt");
  assert.equal(saves.at(-1)?.inPlace, false);
  assert.equal(dialog.open, false);

  // Module kind filtering isolates HWP and swaps SHEET/SLIDE/PDF options.
  switchModule(document, "SHEET");
  await flushAsyncTurns(8);
  assert.equal(document.querySelector('.md-item[data-export-format="pages"]').hidden, true);
  assert.equal(document.querySelector('.md-item[data-export-format="csv"]').hidden, false);

  switchModule(document, "HWP");
  await flushAsyncTurns(8);
  for (const el of document.querySelectorAll(".md-item[data-export-format]")) {
    assert.equal(el.hidden, true, "HWP must hide generic export items");
  }
});
