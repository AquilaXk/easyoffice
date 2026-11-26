import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

const AUTOSAVE_KEY = "easyoffice.workspace.autosave.v1";

function createMemoryStorage(initial = {}) {
  const store = new Map(Object.entries(initial));
  return {
    getItem: (k) => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: (k) => store.delete(k),
  };
}

function spyOnConvertIpc(host) {
  const requests = [];
  const orig = host.__EASYOFFICE_TEST_IPC__;
  host.__EASYOFFICE_TEST_IPC__ = async (cmd, payload) => {
    if (cmd === "convert_document_format") requests.push(JSON.parse(payload));
    return orig(cmd, payload);
  };
  return requests;
}

test("PD-259 BASE shell integration: launch, undo/redo, dirty guard, edit package save, XLSX/CSV/TSV export, print preview, and Copilot", async () => {
  const storage = createMemoryStorage();
  const { document, host, sandbox, downloads, userClick, advanceTimers } = await createWorkspaceHarness({
    search: "?module=BASE&win=w-base-1",
    localStorage: storage,
  });
  const ipcRequests = spyOnConvertIpc(host);

  assert.equal(document.getElementById("app").dataset.module, "BASE");
  assert.equal(document.getElementById("pane-base").hidden, false);
  assert.equal(document.getElementById("doc-style-bar").hidden, true, "DOC style bar must stay hidden in BASE");
  assert.equal(document.getElementById("app").dataset.dirty, "false", "fresh BASE window is clean");
  advanceTimers(2000);
  await flushAsyncTurns(5);
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0, "fresh BASE window has no beforeunload guard");

  const baseHost = host.__EASYOFFICE_BASE_BOARD_HOST__;
  const baseStudio = baseHost.getBaseStudio();
  assert.ok(baseStudio, "baseStudio mounted");

  // Add typed fields and a record -> marks workspace dirty and records undo snapshot
  baseStudio.addField({ name: "단가", type: "number" });
  baseStudio.addField({ name: "수량", type: "number" });
  baseStudio.addField({ name: "합계", type: "formula", formula: "{단가} * {수량}" });
  const tbl0 = baseStudio.getState().tables[0];
  const fTitle = tbl0.fields[0].id;
  const fPrice = tbl0.fields.find((f) => f.name === "단가").id;
  const fQty = tbl0.fields.find((f) => f.name === "수량").id;
  baseStudio.addRecord({ [fTitle]: "서버 라이선스", [fPrice]: 1500, [fQty]: 4 });

  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 1, "adding records marks BASE dirty");
  assert.match(document.getElementById("pane-base").textContent, /6000/);

  // Undo & Redo via workspace shortcut
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(baseStudio.getState().tables[0].records.length, 0, "Ctrl+Z undoes addRecord");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(10);
  assert.equal(baseStudio.getState().tables[0].records.length, 1, "Ctrl+Shift+Z redoes addRecord");

  // Autosave round-trip under per-window key
  advanceTimers(2000);
  await flushAsyncTurns(10);
  const autosaved = storage.getItem(AUTOSAVE_KEY + ".w-base-1");
  assert.ok(autosaved && autosaved.includes("서버 라이선스"), "BASE state persisted in workspace autosave");

  // The XLSX button is an export: it writes the file and leaves the window unsaved.
  userClick(document.getElementById("btn-base-export-xlsx"));
  await flushAsyncTurns(15);
  const xlsxReq = ipcRequests.at(-1);
  assert.equal(xlsxReq.sourceFormat, "xlsx");
  assert.equal(xlsxReq.targetFormat, "xlsx");
  assert.ok(Array.isArray(xlsxReq.sheets) && xlsxReq.sheets.length === 1);
  assert.ok(downloads.some((d) => d.download.endsWith(".xlsx")), "XLSX download triggered");
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 1, "an export does not clear the unsaved mark");

  // Ctrl+S saves the edit package (.eobase) with the whole model, and that clears the unsaved mark.
  sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns(15);
  const saveReq = ipcRequests.at(-1);
  assert.equal(saveReq.action, "encode_package");
  assert.equal(saveReq.kind, "base");
  assert.match(JSON.parse(saveReq.model).tables[0].fields.map((f) => f.name).join(","), /합계/);
  assert.ok(downloads.some((d) => d.download.endsWith(".eobase")), "edit package download triggered");
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0, "saving the package clears the unsaved mark");

  // CSV and TSV export buttons
  userClick(document.getElementById("btn-base-export-csv"));
  await flushAsyncTurns(10);
  assert.equal(ipcRequests.at(-1).targetFormat, "csv");
  assert.ok(downloads.some((d) => d.download.endsWith(".csv")), "CSV download triggered");

  userClick(document.getElementById("btn-base-export-tsv"));
  await flushAsyncTurns(10);
  assert.equal(ipcRequests.at(-1).targetFormat, "tsv");
  assert.ok(downloads.some((d) => d.download.endsWith(".tsv")), "TSV download triggered");

  // Print preview & Copilot context
  const ppApi = host.__EASYOFFICE_PRINT_PREVIEW__;
  assert.ok(ppApi);
  await ppApi.open();
  await flushAsyncTurns(10);
  assert.equal(document.getElementById("print-preview-pane").hidden, false);
  assert.match(document.getElementById("print-preview-stage").textContent, /서버 라이선스/);
  ppApi.close();

  const copilotCtx = baseHost.extractCopilotContext("BASE");
  const base = copilotCtx.context.base;
  assert.ok(base.tables.length >= 1 && base.table, "the table names reach the model");
  assert.ok(base.fields.length > 0 && base.fields.every((f) => f.name && f.type), "each field has a name and a type");
  assert.match(base.records.map((r) => r.join(" ")).join("\n"), /서버 라이선스/);
  assert.equal(base.totalRecords >= base.records.length, true);
  assert.equal(copilotCtx.context.doc, undefined, "the table is not passed off as document paragraphs");
});

test("PD-260 BOARD shell integration: launch, pan/zoom non-dirty, undo/redo, Delete key, edit package save, PDF export, print preview, and Copilot", async () => {
  const { document, host, sandbox, downloads, userClick, advanceTimers } = await createWorkspaceHarness({
    search: "?module=BOARD&win=w-board-1",
  });
  const ipcRequests = spyOnConvertIpc(host);

  assert.equal(document.getElementById("app").dataset.module, "BOARD");
  assert.equal(document.getElementById("pane-board").hidden, false);
  assert.equal(document.getElementById("doc-style-bar").hidden, true, "DOC style bar must stay hidden in BOARD");
  assert.equal(document.getElementById("app").dataset.dirty, "false");
  advanceTimers(2000);
  await flushAsyncTurns(5);
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0);

  const boardHost = host.__EASYOFFICE_BASE_BOARD_HOST__;
  const boardStudio = boardHost.getBoardStudio();
  assert.ok(boardStudio, "boardStudio mounted");

  // Zooming/fitting empty or clean board must NOT mark window dirty and shares unified status zoom state
  boardStudio.setZoom(125);
  await flushAsyncTurns(5);
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0, "viewport zoom alone does not mark BOARD dirty");
  assert.equal(document.getElementById("btn-board-zoom-reset"), null, "BOARD omits duplicate canvas zoom island in favor of status bar zoom");
  assert.equal(document.getElementById("status-zoom-label").textContent, "125%");
  assert.equal(document.querySelector(".canvas-stage").style.zoom, "", "BOARD uses transform scale only, never double stage CSS zoom");
  userClick(document.getElementById("btn-zoom-in"));
  await flushAsyncTurns(4);
  assert.equal(boardStudio.getState().viewport.zoom, 150);
  assert.equal(document.getElementById("status-zoom-label").textContent, "150%");
  assert.equal(document.querySelector(".canvas-stage").style.zoom, "");
  userClick(document.getElementById("btn-board-fit-all"));
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("status-zoom-label").textContent, "100%");

  // Adding shapes, chart, and obstacle-avoiding orthogonal connector marks dirty and supports Undo/Redo
  const a = boardStudio.addNode({ kind: "shape", shapeType: "roundRect", x: 80, y: 120, w: 150, h: 80, text: "요청 수집" });
  const blocker = boardStudio.addNode({ kind: "shape", shapeType: "rect", x: 280, y: 100, w: 140, h: 120, text: "장애물 게이트웨이" });
  const b = boardStudio.addNode({ kind: "shape", shapeType: "diamond", x: 500, y: 120, w: 150, h: 80, text: "승인 판정" });
  const chart = boardStudio.addNode({ kind: "chart", chartType: "column", x: 80, y: 280, w: 320, h: 200, title: "처리량 추이" });
  assert.ok(blocker && chart);
  const conn = boardStudio.addConnector({ fromId: a.id, toId: b.id, routing: "orthogonal", label: "검증 경로" });
  assert.ok(conn);
  const route = boardStudio.getConnectorRoute(conn.id);
  assert.equal(route.avoidedObstacles, true, "orthogonal connector avoids middle blocker");
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 1);

  // Undo & Redo
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(10);
  assert.equal(boardStudio.getState().connectors.length, 0, "Ctrl+Z undoes connector creation");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(10);
  assert.equal(boardStudio.getState().connectors.length, 1, "Ctrl+Shift+Z restores connector");

  // Delete key removes selected node and incident connectors
  boardStudio.selectNode(b.id);
  sandbox.dispatchWindowKeydown({ key: "Delete" });
  await flushAsyncTurns(5);
  assert.equal(boardStudio.getState().nodes.some((n) => n.id === b.id), false);
  assert.equal(boardStudio.getState().connectors.length, 0, "incident connector cleaned up with deleted node");

  // The PDF button is an export and leaves the window unsaved.
  userClick(document.getElementById("btn-board-export-pdf"));
  await flushAsyncTurns(15);
  const pdfReq = ipcRequests.at(-1);
  assert.equal(pdfReq.sourceFormat, "pptx");
  assert.equal(pdfReq.targetFormat, "pdf");
  assert.ok(Array.isArray(pdfReq.slides) && pdfReq.slides[0].elements.length >= 3);
  assert.ok(downloads.some((d) => d.download.endsWith(".pdf")), "PDF download triggered");
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 1, "an export does not clear the unsaved mark");

  // Ctrl+S saves the edit package (.eoboard) with nodes and connectors.
  sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns(15);
  const saveReq = ipcRequests.at(-1);
  assert.equal(saveReq.action, "encode_package");
  assert.equal(saveReq.kind, "board");
  assert.ok(JSON.parse(saveReq.model).nodes.length >= 3);
  assert.ok(downloads.some((d) => d.download.endsWith(".eoboard")), "edit package download triggered");
  assert.equal(sandbox.getWindowListenerCount("beforeunload"), 0, "saving the package clears the unsaved mark");

  // Element analysis dialog + Print preview + Copilot context
  userClick(document.getElementById("btn-board-analyze"));
  assert.match(document.getElementById("board-analysis-body").textContent, /전체 요소/);

  const ppApi = host.__EASYOFFICE_PRINT_PREVIEW__;
  await ppApi.open();
  await flushAsyncTurns(10);
  assert.equal(document.getElementById("print-preview-pane").hidden, false);
  ppApi.close();

  const copilotCtx = boardHost.extractCopilotContext("BOARD");
  assert.match(copilotCtx.selection.range, /요소 3개/);
  const board = copilotCtx.context.board;
  assert.equal(board.totalNodes, 3);
  assert.ok(board.nodes.every((n) => n.id && n.kind), "each element carries an id and a kind");
  assert.equal(copilotCtx.context.doc, undefined);
});

test("PD-334 BASE and BOARD isolate their menubars cleanly and keep every shown command enabled", async () => {
  const shown = (el) => !el.closest("[hidden]");
  const checkWindow = async (mod, expectFind, expectReplace, expectClip) => {
    const { document, host, sandbox } = await createWorkspaceHarness({ search: `?module=${mod}&win=w-menu-${mod}` });
    await flushAsyncTurns(10);
    assert.equal(host.__EASYOFFICE_MENU_KINDS__.menuKindOf(mod), mod);
    const topMenus = document.querySelectorAll(".studio-menubar > .menu-item").filter(shown).map((el) => el.querySelector(".menu-title").textContent);
    assert.deepEqual(topMenus, ["파일", "편집", "보기", "도구", "도움말"]);
    const grayed = document.querySelectorAll(".md-item[data-studio-menu-cmd]").filter((el) => shown(el) && el.getAttribute("aria-disabled") === "true").map((el) => el.dataset.studioMenuCmd);
    assert.deepEqual(grayed, [], `${mod} must have zero disabled visible items`);
    for (const hwpItem of document.querySelectorAll("[data-hwp-save-as]")) {
      assert.equal(shown(hwpItem), false, `${mod} hides HWP save-as items`);
    }
    const cmds = new Set(document.querySelectorAll(".md-item[data-studio-menu-cmd]").filter(shown).map((el) => el.dataset.studioMenuCmd));
    for (const required of ["studio-save", "studio-save-as", "studio-undo", "studio-redo", "palette", "zoom-in", "zoom-out", "view-zoom", "view-theme", "copilot-toggle", "print"]) {
      assert.ok(cmds.has(required), `${mod} shows ${required}`);
    }
    for (const excluded of ["ctrl-marks", "para-marks", "doc-grid", "view-outline", "view-split", "zoom-fit-page", "smart-diagnose", "scrub-pii", "security-audit", "bench-run", "review-verify", "review-stats"]) {
      assert.ok(!cmds.has(excluded), `${mod} hides ${excluded}`);
    }
    assert.equal(cmds.has("find"), expectFind);
    assert.equal(cmds.has("replace"), expectReplace);
    assert.equal(cmds.has("clip-copy"), expectClip);
    document.querySelector('.md-item[data-studio-menu-cmd="zoom-in"]').click();
    assert.notEqual(document.getElementById("status-zoom-label").textContent, "100%");
    document.querySelector('.md-item[data-studio-menu-cmd="view-zoom"]').click();
    await flushAsyncTurns(6);
    assert.equal(document.getElementById("status-zoom-label").textContent, "100%");
    if (mod === "BOARD") {
      const boardStudio = host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
      const slider = document.getElementById("status-zoom-slider");
      assert.equal(slider.min, "25");
      assert.equal(slider.max, "400");
      boardStudio.setZoom(200);
      sandbox.dispatchWindowKeydown({ key: "=", metaKey: true });
      assert.equal(boardStudio.getState().viewport.zoom, 300, "Cmd+= from 200% in BOARD steps up to 300%");
      assert.equal(document.getElementById("status-zoom-label").textContent, "300%");
      boardStudio.setZoom(25);
      assert.equal(document.getElementById("status-zoom-label").textContent, "25%");
      document.getElementById("btn-zoom-fit-width").click();
      assert.equal(document.getElementById("status-zoom-label").textContent, "100%");
    }
  };
  await checkWindow("BASE", true, true, true);
  await checkWindow("BOARD", true, false, true);

  // Switching out of BOARD restores non-BOARD zoom percentage and 50..200 slider bounds
  const { document, host } = await createWorkspaceHarness({ search: "?module=BASE&win=w-board-switch" });
  await flushAsyncTurns(10);
  host.__EASYOFFICE_ZOOM__.apply(125);
  document.defaultView.__EASYOFFICE_ACTIVATE_MODULE__("BOARD");
  await flushAsyncTurns(5);
  host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio().setZoom(300);
  assert.equal(document.getElementById("status-zoom-label").textContent, "300%");
  document.defaultView.__EASYOFFICE_ACTIVATE_MODULE__("BASE");
  await flushAsyncTurns(5);
  assert.equal(document.getElementById("status-zoom-label").textContent, "125%");
  assert.equal(document.getElementById("status-zoom-slider").min, "50");
  assert.equal(document.getElementById("status-zoom-slider").max, "200");
});
