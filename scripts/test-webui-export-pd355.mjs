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
      if (req.action === "decode") return JSON.stringify(await offline[cmd](payload));
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

  // Export via menu item -> .pages always keeps inPlace: false, exportOnly: true and leaves document title unchanged.
  const pagesItem = document.querySelector('.md-item[data-export-format="pages"]');
  assert.ok(pagesItem && !pagesItem.hidden, "DOC shows Pages export menu item");
  userClick(pagesItem);
  await flushAsyncTurns(12);
  assert.equal(converts.at(-1)?.targetFormat, "pages");
  assert.equal(saves.at(-1)?.fileName, "원본보고서.pages");
  assert.equal(saves.at(-1)?.inPlace, false, "explicit .pages export must never overwrite in place");
  assert.equal(saves.at(-1)?.exportOnly, true, "explicit export must carry exportOnly: true");
  assert.equal(bridge.getDocumentName("DOC"), "원본보고서.pages");

  // Format selector dialog sanitizes malicious stems (C0/C1 control chars) and runs conversion with inPlace: false, exportOnly: true.
  userClick(document.querySelector(".md-item[data-export-dialog]"));
  await flushAsyncTurns(4);
  const dialog = document.getElementById("export-dialog");
  assert.equal(dialog.open, true);
  const sel = document.getElementById("export-format-select");
  const opts = Array.from(sel.querySelectorAll("option")).map((o) => o.value);
  assert.deepEqual(opts, ["docx", "odt", "rtf", "pages", "md", "txt", "pdf"]);
  sel.value = "odt";
  sel.dispatchEvent({ type: "change" });
  document.getElementById("export-file-name").value = "../../탈출\u0085시도\0.docx";
  userClick(document.getElementById("btn-export-run"));
  await flushAsyncTurns(12);
  assert.equal(saves.at(-1)?.fileName, "탈출시도.odt");
  assert.equal(saves.at(-1)?.inPlace, false);
  assert.equal(saves.at(-1)?.exportOnly, true);
  assert.equal(dialog.open, false);

  // Exporting a same-extension copy from an opened .docx keeps title and subsequent Save inPlace: true.
  bridge.applyTitles({ ...bridge.getTitles(), DOC: "원본.docx" });
  userClick(document.querySelector(".md-item[data-export-dialog]"));
  await flushAsyncTurns(4);
  document.getElementById("export-format-select").value = "docx";
  document.getElementById("export-file-name").value = "배포본.docx";
  userClick(document.getElementById("btn-export-run"));
  await flushAsyncTurns(12);
  assert.equal(saves.at(-1)?.fileName, "배포본.docx");
  assert.equal(saves.at(-1)?.inPlace, false);
  assert.equal(saves.at(-1)?.exportOnly, true);
  assert.equal(bridge.getDocumentName("DOC"), "원본.docx");
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);
  assert.equal(saves.at(-1)?.fileName, "원본.docx");
  assert.equal(saves.at(-1)?.inPlace, true);
  assert.equal(saves.at(-1)?.exportOnly, undefined);

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

test("PDF form toggle does not dirty clean document, extracted AcroForm fields render safely, and unapplied stamp does not leak", async () => {
  const offline = await createOffline();
  const converts = [];
  const pdfEditSaves = [];
  const windowStates = [];
  const confirmCalls = [];
  const { document, userClick } = await createWorkspaceHarness({
    search: "?module=PDF&win=w-pdf-clean-export",
    platform: "MacIntel",
    confirm: (msg) => {
      confirmCalls.push(String(msg || ""));
      return true;
    },
    shellCommands: {
      report_menu_state: () => null,
      report_window_state: (args) => {
        windowStates.push(args);
        return null;
      },
      confirm_dialog: (args) => {
        confirmCalls.push(JSON.stringify(args));
        return true;
      },
      pdf_edit_open: () => ({ pages: 1, sources: 1, turn: 0, stamp: "", watermark: "" }),
      pdf_edit_apply: () => ({ pages: 1, turn: 0, stamp: "", watermark: "" }),
      pdf_edit_save: (args) => {
        pdfEditSaves.push(args);
        return { status: "saved", name: args?.fileName || "edited.pdf", bytes: 120, pages: 1 };
      },
      save_document_file: (args) => ({ status: "saved", name: args.fileName, bytes: 12 }),
    },
    tauriInvoke: async (cmd, { payload }) => {
      if (cmd !== "convert_document_format") return JSON.stringify(await offline[cmd](payload));
      const req = JSON.parse(payload);
      if (req.action === "decode") return JSON.stringify(await offline[cmd](payload));
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
  await flushAsyncTurns(10);

  // Empty PDF must refuse export cleanly without calling backend.
  userClick(document.querySelector('.md-item[data-export-format="docx"]'));
  await flushAsyncTurns(8);
  assert.equal(converts.length, 0, "empty PDF must not convert");
  assert.match(document.getElementById("status-command-feedback").textContent, /내보낼 PDF 문서가 없습니다/);

  // Open plain PDF -> toggle form panel -> flush autosave -> opening another PDF must NOT trigger unsaved prompt.
  await pickFile(
    document,
    makeFixtureFile("단순문서.pdf", {
      format: "pdf",
      title: "단순문서",
      blocks: [{ type: "paragraph", runs: [{ text: "본문 1줄" }] }],
      pdfOutlines: [],
      pdfForm: { hasAcroForm: false, fieldCount: 0, applicant: null, department: null, fields: [] },
    })
  );
  await flushAsyncTurns(12);
  const promptsBeforeToggle = confirmCalls.length;
  userClick(document.getElementById("btn-pdf-form-toggle"));
  await flushAsyncTurns(8);
  assert.equal(document.getElementById("pdf-acroform-grid").hidden, false);

  // Export DOCX before clicking Apply -> unapplied default #pdf-stamp-select ("승인필") must NOT leak.
  userClick(document.querySelector('.md-item[data-export-format="docx"]'));
  await flushAsyncTurns(10);
  assert.equal(converts.at(-1)?.sourceFormat, "pdf");
  assert.equal(converts.at(-1)?.targetFormat, "docx");
  assert.equal(converts.at(-1)?.pdf?.stamp, undefined, "unapplied stamp select must not leak");

  // Open PDF with arbitrary AcroForm fields while grid is merely toggled (no edits) -> must NOT ask unsaved confirm.
  await pickFile(
    document,
    makeFixtureFile("계약.pdf", {
      format: "pdf",
      title: "계약",
      blocks: [{ type: "paragraph", runs: [{ text: "계약 조항" }] }],
      pdfOutlines: [],
      pdfForm: {
        hasAcroForm: true,
        fieldCount: 2,
        applicant: null,
        department: null,
        fields: [
          { name: "계약 번호", value: "2026-042" },
          { name: "동의", value: "Yes" },
        ],
      },
    })
  );
  await flushAsyncTurns(12);
  assert.equal(confirmCalls.length, promptsBeforeToggle, "form toggle alone must not trigger unsaved prompt");
  assert.notEqual(windowStates.at(-1)?.dirty, true);
  const extractedBox = document.getElementById("pdf-form-extracted");
  assert.equal(extractedBox.hidden, false);
  assert.match(extractedBox.textContent, /계약 번호:\s*2026-042/);
  assert.match(extractedBox.textContent, /동의:\s*Yes/);

  // Typing a form field value marks dirty and opening another file asks confirm.
  const applicantInput = document.getElementById("pdf-form-applicant");
  applicantInput.value = "김철수";
  applicantInput.dispatchEvent({ type: "input", bubbles: true });
  await flushAsyncTurns(8);
  await pickFile(
    document,
    makeFixtureFile("계약2.pdf", {
      format: "pdf",
      title: "계약2",
      blocks: [{ type: "paragraph", runs: [{ text: "본문" }] }],
      pdfOutlines: [],
      pdfForm: { hasAcroForm: true, fieldCount: 1, applicant: null, department: null, fields: [{ name: "계약 번호", value: "2026-043" }] },
    })
  );
  await flushAsyncTurns(12);
  assert.ok(confirmCalls.length > promptsBeforeToggle, "editing form field must prompt before replacing");

  userClick(document.querySelector(".md-item[data-export-dialog]"));
  await flushAsyncTurns(4);
  document.getElementById("export-format-select").value = "pdf";
  document.getElementById("export-file-name").value = "CON.pdf";
  userClick(document.getElementById("btn-export-run"));
  await flushAsyncTurns(12);
  assert.equal(pdfEditSaves.at(-1)?.fileName, "_CON.pdf", "reserved Windows stem is sanitized and forwarded to pdf_edit_save");
});

test("BASE/BOARD export send valid backend sourceFormats, reject empty BOARD PDF, sanitize reserved names, and START/CONV disable export", async () => {
  const offline = await createOffline();
  const converts = [];
  const saves = [];
  const { document } = await createWorkspaceHarness({
    search: "?module=BASE&win=w-base-board-export",
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
      if (req.action === "decode") return JSON.stringify(await offline[cmd](payload));
      converts.push(req);
      return JSON.stringify({
        status: "OK",
        convertedBase64: Buffer.from("a,b\n1,2\n").toString("base64"),
        outputBytes: 8,
        specification: String(req.targetFormat || "").toUpperCase(),
        compatibility: "full",
        warnings: [],
      });
    },
  });
  await flushAsyncTurns(10);

  document.querySelector(".md-item[data-export-dialog]").click();
  await flushAsyncTurns(4);
  document.getElementById("export-format-select").value = "csv";
  document.getElementById("export-file-name").value = "NUL:보고서?.xlsx";
  document.getElementById("btn-export-run").click();
  await flushAsyncTurns(12);
  assert.equal(converts.at(-1)?.sourceFormat, "xlsx");
  assert.equal(converts.at(-1)?.targetFormat, "csv");
  assert.equal(saves.at(-1)?.fileName, "NUL_보고서_.csv");
  assert.equal(saves.at(-1)?.inPlace, false);
  assert.equal(saves.at(-1)?.exportOnly, true);

  // Direct BASE studio export (deliverExport) and BOARD PDF export also carry exportOnly: true.
  const baseStudio = document.defaultView.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
  await baseStudio.exportAs("xlsx");
  assert.equal(saves.at(-1)?.inPlace, false);
  assert.equal(saves.at(-1)?.exportOnly, true);

  switchModule(document, "BOARD");
  await flushAsyncTurns(8);
  const boardStudio = document.defaultView.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  boardStudio.replaceState({ version: 1, nodes: [], connectors: [], viewport: { x: 0, y: 0, zoom: 1 } });
  document.querySelector('.md-item[data-export-format="pdf"]').click();
  await flushAsyncTurns(8);
  assert.match(document.getElementById("status-command-feedback").textContent, /PDF로 내보낼 보드 요소가 없습니다/);

  boardStudio.replaceState({
    version: 1,
    nodes: [{ id: "n1", kind: "text", x: 20, y: 20, w: 180, h: 60, text: "보드 텍스트" }],
    connectors: [],
    viewport: { x: 0, y: 0, zoom: 1 },
  });
  document.querySelector('.md-item[data-export-format="pdf"]').click();
  await flushAsyncTurns(12);
  assert.equal(converts.at(-1)?.sourceFormat, "pptx");
  assert.equal(converts.at(-1)?.targetFormat, "pdf");
  assert.equal(saves.at(-1)?.inPlace, false);
  assert.equal(saves.at(-1)?.exportOnly, true);

  switchModule(document, "CONV");
  await flushAsyncTurns(6);
  assert.equal(document.querySelector(".md-item[data-export-dialog]").getAttribute("aria-disabled"), "true");
});
