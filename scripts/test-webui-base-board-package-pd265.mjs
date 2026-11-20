import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// PD-265: BASE and BOARD save their own edit package (.eobase / .eoboard) and open it again as the same editable
// model; XLSX, CSV and PDF are exports. The container is the engine's (`native_package.rs`, tested in Rust); here the
// page side is checked with the engine stubbed: what is sent, what comes back into the editors, and what is refused.

const PNG_1X1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==";

function recordReplies(host) {
  const replies = [];
  const orig = host.__EASYOFFICE_TEST_IPC__;
  host.__EASYOFFICE_TEST_IPC__ = async (cmd, payload) => {
    const reply = await orig(cmd, payload);
    if (cmd === "convert_document_format") replies.push({ request: JSON.parse(payload), reply });
    return reply;
  };
  return replies;
}

const bytesOf = (base64) => Uint8Array.from(Buffer.from(base64, "base64"));
const packageOf = (fakePackage) => bytesOf(Buffer.from(JSON.stringify(fakePackage)).toString("base64"));

async function savedPackage(host, sandbox, replies) {
  const before = replies.length;
  sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns(15);
  const saved = replies.slice(before).find((r) => r.request.action === "encode_package");
  assert.ok(saved, "Ctrl+S sent an encode_package request");
  return bytesOf(saved.reply.convertedBase64);
}

test("a BASE saved to .eobase opens as the same tables, fields, relations, formulas and views", async () => {
  const first = await createWorkspaceHarness({ search: "?module=BASE&win=w-base-a" });
  const replies = recordReplies(first.host);
  const studio = first.host.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
  studio.addField({ name: "수량", type: "number" });
  studio.addField({ name: "금액", type: "formula", formula: "{수량} * 2" });
  studio.addTable({ name: "주문" });
  const tables = studio.getState().tables;
  studio.addRecord({ [tables[0].fields[0].id]: "본사" });
  studio.setView("kanban");
  const bytes = await savedPackage(first.host, first.sandbox, replies);
  const original = studio.getState();

  const second = await createWorkspaceHarness({ search: "?module=BASE&win=w-base-b" });
  const result = await second.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("고객.eobase", bytes);
  assert.equal(result.ok, true, JSON.stringify(result));
  const reopened = second.host.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio().getState();
  assert.deepEqual(reopened, original);
  const again = await second.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("고객.eobase", bytes);
  assert.equal(again.ok, true, "a freshly opened package is not an unsaved edit, so opening it again asks nothing");
  assert.match(second.document.getElementById("status-command-feedback").textContent, /열기 완료.*2개 테이블/);
});

test("a BOARD saved to .eoboard opens with its nodes, connectors, viewport and picture bytes", async () => {
  const first = await createWorkspaceHarness({ search: "?module=BOARD&win=w-board-a" });
  const replies = recordReplies(first.host);
  const studio = first.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  const a = studio.addNode({ kind: "shape", shapeType: "ellipse", x: 40, y: 50, w: 120, h: 80, text: "시작" });
  const b = studio.addNode({ kind: "text", x: 400, y: 60, text: "끝" });
  studio.addConnector({ fromId: a.id, toId: b.id, routing: "curved", label: "흐름" });
  const picture = await studio.insertPictureFromBytes(bytesOf(PNG_1X1), "로고.png");
  assert.equal(picture.ok, true);
  const bytes = await savedPackage(first.host, first.sandbox, replies);
  const sent = replies.find((r) => r.request.action === "encode_package").request;
  assert.equal(sent.media.length, 1);
  assert.equal(sent.media[0].base64, PNG_1X1);
  assert.equal(JSON.parse(sent.model).nodes.some((n) => "imageId" in n), false, "the model holds the picture path, not the in-memory id");

  const second = await createWorkspaceHarness({ search: "?module=BOARD&win=w-board-b" });
  const result = await second.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("흐름.eoboard", bytes);
  assert.equal(result.ok, true, JSON.stringify(result));
  const state = second.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio().getState();
  assert.equal(state.nodes.length, 3);
  assert.equal(state.connectors[0].label, "흐름");
  const image = state.nodes.find((n) => n.kind === "image");
  assert.equal(second.host.__EASYOFFICE_SHEET_UTILS__.getDocImage(image.imageId).base64, PNG_1X1);
  const again = await second.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("흐름.eoboard", bytes);
  assert.equal(again.ok, true, "a freshly opened package is not an unsaved edit");
});

async function openIntoBase(model) {
  const h = await createWorkspaceHarness({ search: "?module=BASE&win=w-base-bad" });
  const studio = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
  studio.addRecord({ "fld-1": "남아 있어야 하는 기록" });
  h.sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns(15);
  const before = studio.getState();
  const result = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("나쁨.eobase", packageOf({ kind: "base", model: JSON.stringify(model), media: [] }));
  return { h, studio, before, result };
}

const goodTable = (id, extra = {}) => ({ id, name: id, view: "grid", fields: [{ id: "f1", name: "이름", type: "text" }], records: [], ...extra });

test("a BASE model outside the limits or the schema is refused whole and leaves the open document alone", async () => {
  const bad = {
    "too many tables": { tables: Array.from({ length: 33 }, (_, n) => goodTable("t" + n)) },
    "no tables": { tables: [] },
    "an unknown field type": { tables: [goodTable("t1", { fields: [{ id: "f1", name: "이름", type: "script" }] })] },
    "a polluted key": JSON.parse('{"tables":[{"id":"t1","name":"x","fields":[{"id":"f1","name":"a","type":"text"}],"records":[],"__proto__":{"x":1}}]}'),
    "a relation to a missing table": { tables: [goodTable("t1", { fields: [{ id: "f1", name: "이름", type: "text" }, { id: "f2", name: "참조", type: "relation", relationTableId: "nope" }] })] },
    "duplicate table ids": { tables: [goodTable("t1"), goodTable("t1")] },
    "duplicate field ids": { tables: [goodTable("t1", { fields: [{ id: "f1", name: "a", type: "text" }, { id: "f1", name: "b", type: "text" }] })] },
    "duplicate record ids": { tables: [goodTable("t1", { records: [{ id: "r1", values: {} }, { id: "r1", values: {} }] })] },
    "a value of the wrong type": { tables: [goodTable("t1", { fields: [{ id: "f1", name: "수", type: "number" }], records: [{ id: "r1", values: { f1: "열두 개" } }] })] },
  };
  for (const [label, model] of Object.entries(bad)) {
    const { h, studio, before, result } = await openIntoBase(model);
    assert.equal(result.ok, false, label);
    assert.deepEqual(studio.getState(), before, label + ": the open document is untouched");
    assert.match(h.document.getElementById("status-command-feedback").textContent, /파일 열기 실패/, label);
  }
});

test("a BASE with formula fields that refer to each other opens and shows the cycle error instead of hanging", async () => {
  const cyclic = {
    tables: [{
      id: "t1", name: "순환", view: "grid",
      fields: [
        { id: "f0", name: "이름", type: "text" },
        { id: "f1", name: "가", type: "formula", formula: "{나} + 1" },
        { id: "f2", name: "나", type: "formula", formula: "{가} + 1" },
      ],
      records: [{ id: "r1", values: { f0: "x" } }],
    }],
  };
  const { h, result } = await openIntoBase(cyclic);
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.match(h.document.getElementById("pane-base").textContent, /#CYCLE!/);
});

test("a BASE model with a table over the record limit is refused", async () => {
  const records = Array.from({ length: 10001 }, (_, n) => ({ id: "r" + n, values: {} }));
  const { result } = await openIntoBase({ tables: [goodTable("t1", { records })] });
  assert.equal(result.ok, false);
});

async function openIntoBoard(model, media = []) {
  const h = await createWorkspaceHarness({ search: "?module=BOARD&win=w-board-bad" });
  const studio = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  studio.addNode({ kind: "text", x: 10, y: 10, text: "남아 있어야 하는 요소" });
  h.sandbox.dispatchWindowKeydown({ key: "s", ctrlKey: true });
  await flushAsyncTurns(15);
  const before = studio.getState();
  const result = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("나쁨.eoboard", packageOf({ kind: "board", model: JSON.stringify(model), media }));
  return { h, studio, before, result };
}

const textNode = (id, extra = {}) => ({ id, kind: "text", x: 0, y: 0, w: 100, h: 40, text: id, ...extra });

test("a BOARD model outside the limits or the schema is refused whole and leaves the open document alone", async () => {
  const bad = {
    "a coordinate out of range": { nodes: [textNode("n1"), textNode("n2", { x: 9e9 })] },
    "an unknown element kind": { nodes: [textNode("n1", { kind: "script" })] },
    "duplicate element ids": { nodes: [textNode("n1"), textNode("n1")] },
    "too many elements": { nodes: Array.from({ length: 513 }, (_, n) => textNode("n" + n)) },
    "a connector to a missing element": { nodes: [textNode("n1")], connectors: [{ id: "c1", fromId: "n1", toId: "ghost" }] },
    "an element that is not an object": { nodes: [7] },
    "a picture the package does not hold": { nodes: [{ id: "i1", kind: "image", x: 0, y: 0, w: 50, h: 50, media: "media/missing.png" }] },
  };
  for (const [label, model] of Object.entries(bad)) {
    const { studio, before, result } = await openIntoBoard(model);
    assert.equal(result.ok, false, label);
    assert.deepEqual(studio.getState(), before, label + ": the open document is untouched");
  }
});

test("restoring a snapshot with one invalid element keeps the others and says how many were left out", async () => {
  const { h, studio } = await openIntoBoard({ nodes: [textNode("n1")] });
  const notes = [];
  const feedback = h.document.getElementById("status-command-feedback");
  const outcome = studio.replaceState({ nodes: [textNode("n1"), textNode("n2", { x: 9e9 }), textNode("n3")], connectors: [{ id: "c1", fromId: "n1", toId: "n3" }, { id: "c2", fromId: "n1", toId: "n2" }] });
  notes.push(feedback.textContent);
  assert.deepEqual(studio.getState().nodes.map((n) => n.id), ["n1", "n3"]);
  assert.deepEqual(studio.getState().connectors.map((c) => c.id), ["c1"]);
  assert.equal(outcome.dropped, 2, "the element and the connector that pointed at it");
  assert.match(notes[0], /건너뛰었습니다/);
});

test("the edit package extensions open in a BASE or BOARD window", async () => {
  const { host } = await createWorkspaceHarness();
  const kindOf = host.__EASYOFFICE_WINDOW_BRIDGE__?.kindForFileName ?? ((name) => host.__EASYOFFICE_CREATE_WINDOW_BRIDGE_UTILS__?.kindForFileName(name));
  assert.equal(kindOf("a.eobase"), "BASE");
  assert.equal(kindOf("a.EOBOARD"), "BOARD");
  assert.equal(kindOf("a.pdf"), "PDF");
});

test("a BOARD file whose model is refused registers none of its pictures", async () => {
  const { host } = await createWorkspaceHarness({ search: "?module=BOARD&win=w-board-reg" });
  const utils = host.__EASYOFFICE_SHEET_UTILS__;
  const registered = [];
  const original = utils.registerDocImage;
  Object.defineProperty(utils, "registerDocImage", { value: (...args) => { registered.push(args); return original(...args); }, configurable: true });
  const media = [{ path: "media/img-1.png", mediaType: "image/png", base64: PNG_1X1 }];
  const image = { id: "i1", kind: "image", x: 0, y: 0, w: 50, h: 50, media: "media/img-1.png" };
  const bad = { nodes: [image, { id: "t1", kind: "text", x: 9e9, y: 0, w: 10, h: 10, text: "범위 밖" }] };
  const refused = await host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("나쁨.eoboard", packageOf({ kind: "board", model: JSON.stringify(bad), media }));
  assert.equal(refused.ok, false);
  assert.equal(registered.length, 0, "nothing was put in the picture store");
  const good = await host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("좋음.eoboard", packageOf({ kind: "board", model: JSON.stringify({ nodes: [image] }), media }));
  assert.equal(good.ok, true);
  assert.equal(registered.length, 1);
});

test("saving a BASE with no database editor mounted fails instead of writing an empty database", async () => {
  const { loadUiModule } = await import("./test-support/load-ui-module.mjs");
  const env = { document: {}, __EASYOFFICE_BASE_MODEL__: {} };
  loadUiModule(new URL("../crates/easyoffice-app/ui/base-board-host.js", import.meta.url), env);
  const api = env.__EASYOFFICE_BASE_BOARD_HOST__;
  const requests = [];
  api.init({ document: { getElementById: () => null, addEventListener() {} }, env: { addEventListener() {} }, invokeIpc: async (...a) => requests.push(a) });
  const saved = await api.savePackage("BASE", async () => ({ ok: true }), "x");
  assert.equal(saved.ok, false);
  assert.match(saved.failure, /데이터베이스/);
  assert.equal(requests.length, 0, "no package was requested");
});
