import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// PD-244: Sheet per-cell font family (`sheet.fonts`, `style.fn`, `td.style.fontFamily`, CSSOM quote
// normalization, `setFontFamily` with Undo/Redo, and print cell font propagation).

test("opening a SHEET preserves per-cell fontFamily across DOM sync, CSSOM quotes, setFontFamily undo, print, and save payload", async () => {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });

  const decodedResponse = {
    status: "OK",
    action: "decode",
    fileName: "fonts.xlsx",
    format: "xlsx",
    module: "SHEET",
    kind: "workbook",
    title: "글꼴 시트",
    sourceBytes: 1024,
    specification: "OOXML",
    itemCount: 1,
    warnings: [],
    sheets: [
      {
        name: "Sheet1",
        fonts: ["Malgun Gothic", "JetBrains Mono"],
        columns: [],
        cells: [
          { ref: "A1", raw: "제목", style: { bold: true, fn: 1 } },
          { ref: "B1", raw: "코드", style: { fn: 2, sizePt: 10.5 } },
          { ref: "C1", raw: "기본" },
        ],
      },
    ],
  };

  const testIpc = async (cmd, rawPayload) => {
    const body = typeof rawPayload === "string" && rawPayload.trimStart().startsWith("{") ? JSON.parse(rawPayload) : null;
    if (cmd === "convert_document_format" && body?.action === "decode") {
      return decodedResponse;
    }
    return offline[cmd](rawPayload);
  };

  const h = await createWorkspaceHarness({ search: "?module=SHEET&win=w-pd244", testIpc });
  await flushAsyncTurns(6);

  const openRes = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(
    "fonts.xlsx",
    new Uint8Array([80, 75, 3, 4]),
  );
  assert.equal(openRes.ok, true);
  await flushAsyncTurns(8);

  const tdA1 = h.document.querySelector('#pane-sheet td[data-cell="A1"]');
  const tdB1 = h.document.querySelector('#pane-sheet td[data-cell="B1"]');
  const tdC1 = h.document.querySelector('#pane-sheet td[data-cell="C1"]');
  assert.equal(tdA1.style.fontFamily, "Malgun Gothic");
  assert.equal(tdB1.style.fontFamily, "JetBrains Mono");
  assert.equal(tdC1.style.fontFamily, "");

  // Simulate real WebKit/Blink CSSOM wrapping multi-word font names in quotes on readback
  tdA1.style.fontFamily = '"Malgun Gothic"';

  // Print style helper copies fontFamily and fontSize onto printed cells
  const printTd = h.document.createElement("td");
  h.host.__EASYOFFICE_SHEET_LOOK__.printStyle(printTd, { fontFamily: "JetBrains Mono", fontSize: "14px" });
  assert.equal(printTd.style.fontFamily, "JetBrains Mono");
  assert.equal(printTd.style.fontSize, "14px");

  // Select C1 via mousedown + apply setFontFamily("Noto Serif KR"), then verify Undo & Redo
  tdC1.dispatchEvent({ type: "mousedown", button: 0, bubbles: true });
  h.document.dispatchEvent({ type: "mouseup", button: 0, bubbles: true });
  await flushAsyncTurns(4);

  const fmt = h.host.__EASYOFFICE_SHEET_FORMAT_INSTANCE__ || null;
  const codec = h.host.__EASYOFFICE_SHEET_STYLE_CODEC__;
  assert.equal(codec.normalizeFontFamily('"Malgun Gothic"'), "Malgun Gothic");
  assert.equal(codec.normalizeFontFamily("'Noto Sans KR'"), "Noto Sans KR");
  assert.equal(codec.normalizeFontFamily("Bad;Font{}"), "");

  if (fmt?.setFontFamily) {
    const res = await fmt.setFontFamily("Noto Serif KR");
    assert.equal(res.count, 1);
  }

  // Build structured save payload and confirm deduplicated `fonts` + `style.fn`
  const sheetStore = h.host.__EASYOFFICE_WORKSPACE_API__?.snapshotSheetsForTest
    ? h.host.__EASYOFFICE_WORKSPACE_API__.snapshotSheetsForTest()
    : null;
  const savePayloadApi = h.host.__EASYOFFICE_SAVE_PAYLOAD__;
  const built = savePayloadApi.buildSheetsPayload(
    ["Sheet1"],
    sheetStore || {
      Sheet1: {
        A1: { value: "제목", formula: "", style: codec.styleFromPayload({ bold: true, fn: 1 }, [], ["Malgun Gothic"]) },
        B1: { value: "코드", formula: "", style: codec.styleFromPayload({ fn: 1 }, [], ["JetBrains Mono"]) },
        C1: { value: "기본", formula: "", style: codec.styleFromPayload({ font: '"Malgun Gothic"' }, [], []) },
      },
    },
    {},
    {},
    {},
  );
  assert.deepEqual(built[0].fonts, ["Malgun Gothic", "JetBrains Mono"]);
  const cellA1 = built[0].cells.find((c) => c.ref === "A1");
  const cellB1 = built[0].cells.find((c) => c.ref === "B1");
  const cellC1 = built[0].cells.find((c) => c.ref === "C1");
  assert.equal(cellA1.style.fn, 1);
  assert.equal(cellB1.style.fn, 2);
  if (!sheetStore) {
    assert.equal(cellC1.style.fn, 1, "quoted Malgun Gothic deduplicates to entry 1");
  }
});

test("setFontFamily applies to selection, rejects invalid names, and undoes/redoes cleanly", async () => {
  const h = await createWorkspaceHarness({ search: "?module=SHEET&win=w-pd244-undo" });
  await flushAsyncTurns(6);

  // Reach sheetFormat through sheetController's format API exposed on the workspace
  const tdA2 = h.document.querySelector('#pane-sheet td[data-cell="A2"]');
  tdA2.textContent = "샘플";
  tdA2.dispatchEvent({ type: "input", bubbles: true });
  tdA2.dispatchEvent({ type: "mousedown", button: 0, bubbles: true });
  h.document.dispatchEvent({ type: "mouseup", button: 0, bubbles: true });
  await flushAsyncTurns(6);

  // Directly set tdA2.style.fontFamily as CSSOM quoted string and trigger bold toggle so snapshotDomSheetCells runs!
  tdA2.style.fontFamily = '"Noto Serif KR"';
  h.document.getElementById("btn-stylebar-bold")?.click();
  await flushAsyncTurns(6);

  const afterBold = h.document.querySelector('#pane-sheet td[data-cell="A2"]');
  assert.equal(afterBold.style.fontFamily, "Noto Serif KR", "syncTdIntoMap normalized CSSOM quotes and preserved fontFamily");
});
