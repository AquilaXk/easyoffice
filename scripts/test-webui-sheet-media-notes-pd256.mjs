import assert from "node:assert/strict";
import test from "node:test";
import zlib from "node:zlib";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i += 1) {
    c ^= buf[i];
    for (let k = 0; k < 8; k += 1) {
      c = (c >>> 1) ^ (c & 1 ? 0xedb88320 : 0);
    }
  }
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(typeAscii, data) {
  const typeBuf = Buffer.from(typeAscii, "ascii");
  const lenBuf = Buffer.alloc(4);
  lenBuf.writeUInt32BE(data.length, 0);
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([lenBuf, typeBuf, data, crcBuf]);
}

function makeValidPngBytes(width = 240, height = 160) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // RGB
  const rawScanlines = Buffer.alloc(height * (1 + Math.min(width, 4) * 3));
  const idat = zlib.deflateSync(rawScanlines);
  return new Uint8Array(
    Buffer.concat([sig, pngChunk("IHDR", ihdr), pngChunk("IDAT", idat), pngChunk("IEND", Buffer.alloc(0))])
  );
}

function makeValidJpegBytes(width = 180, height = 120) {
  const buf = Buffer.alloc(24);
  let p = 0;
  buf[p++] = 0xff;
  buf[p++] = 0xd8; // SOI
  buf[p++] = 0xff;
  buf[p++] = 0xe0; // APP0
  buf.writeUInt16BE(4, p);
  p += 4;
  buf[p++] = 0xff;
  buf[p++] = 0xc0; // SOF0
  buf.writeUInt16BE(11, p);
  p += 2;
  buf[p++] = 8; // precision
  buf.writeUInt16BE(height, p);
  p += 2;
  buf.writeUInt16BE(width, p);
  p += 2;
  buf[p++] = 1; // components
  buf[p++] = 1;
  buf[p++] = 0x11;
  buf[p++] = 0;
  buf[p++] = 0xff;
  buf[p++] = 0xd9; // EOI
  return new Uint8Array(buf.subarray(0, p));
}

function setCell(document, coord, text) {
  const td = document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  assert.ok(td, `missing cell ${coord}`);
  td.focus();
  td.textContent = text;
  td.dispatchEvent({ type: "input", target: td });
  td.blur();
}

function selectCell(document, coord) {
  const td = document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  assert.ok(td, `missing cell ${coord}`);
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
  td.focus();
  return td;
}

test("PD-256: PNG/JPEG 부동 그림 삽입·드래그 이동·우하단 리사이즈·크기 및 속성 대화상자·Delete 삭제·Undo/Redo가 동작한다", async () => {
  const { document, sandbox, userClick, host } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-media-ops",
  });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);

  const mediaApi = host.__EASYOFFICE_SHEET_MEDIA__;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  assert.ok(mediaApi, "__EASYOFFICE_SHEET_MEDIA__ exported");

  selectCell(document, "B2");
  await flushAsyncTurns(2);

  const pngBytes = makeValidPngBytes(240, 160);
  const res = await mediaApi.insertPictureFromBytes(pngBytes, "분기_로고.png");
  await flushAsyncTurns(6);
  assert.equal(res.ok, true, "PNG 그림 삽입 성공");

  const images0 = layoutApi.imagesOf("Sheet1");
  assert.equal(images0.length, 1);
  assert.equal(images0[0].mime, "image/png");
  assert.equal(images0[0].alt, "분기_로고");
  assert.ok(images0[0].imageId, "layout.images는 Base64 복제 대신 경량 imageId 핸들을 보관");
  assert.equal(images0[0].data, undefined, "layout.images 내부에 무거운 data 문자열 미보관");
  assert.equal(images0[0].naturalWidthPx, 240);
  assert.equal(images0[0].naturalHeightPx, 160);
  assert.equal(images0[0].anchor.fromCol, 1);
  assert.equal(images0[0].anchor.fromRow, 1);
  assert.equal(images0[0].anchor.widthPx, 240);
  assert.equal(images0[0].anchor.heightPx, 160);

  const layer = document.getElementById("sheet-image-layer");
  assert.ok(layer && !layer.hidden, "#sheet-image-layer 표시");
  let card = layer.querySelector(".sheet-image-card");
  assert.ok(card, ".sheet-image-card 생성");
  assert.equal(card.classList.contains("selected"), true);
  const imgEl = card.querySelector("img");
  assert.ok(imgEl && imgEl.getAttribute("src"), "img src 바인딩");
  assert.equal(imgEl.getAttribute("alt"), "분기_로고");

  const initialAnchor = structuredClone(images0[0].anchor);

  // 드래그 이동 (+192px, +72px)
  card.dispatchEvent({ type: "mousedown", target: card, button: 0, clientX: 200, clientY: 80 });
  document.dispatchEvent({ type: "mousemove", clientX: 392, clientY: 152 });
  document.dispatchEvent({ type: "mouseup", clientX: 392, clientY: 152 });
  await flushAsyncTurns(6);

  const movedAnchor = structuredClone(layoutApi.imagesOf("Sheet1")[0].anchor);
  assert.equal(movedAnchor.leftPx, initialAnchor.leftPx + 192);
  assert.equal(movedAnchor.topPx, initialAnchor.topPx + 72);
  assert.ok(movedAnchor.fromCol > initialAnchor.fromCol, "이동 후 fromCol 갱신");
  assert.ok(movedAnchor.fromRow > initialAnchor.fromRow, "이동 후 fromRow 갱신");

  // 우하단 핸들 리사이즈 (+96px, +48px)
  card = layer.querySelector(".sheet-image-card");
  const handle = card.querySelector(".sheet-image-resize-handle");
  assert.ok(handle, ".sheet-image-resize-handle 존재");
  handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, clientX: 500, clientY: 300 });
  document.dispatchEvent({ type: "mousemove", clientX: 596, clientY: 348 });
  document.dispatchEvent({ type: "mouseup", clientX: 596, clientY: 348 });
  await flushAsyncTurns(6);

  const resizedAnchor = structuredClone(layoutApi.imagesOf("Sheet1")[0].anchor);
  assert.equal(resizedAnchor.widthPx, 336);
  assert.equal(resizedAnchor.heightPx, 208);
  assert.ok(resizedAnchor.toCol >= movedAnchor.toCol);
  assert.ok(resizedAnchor.toRow >= movedAnchor.toRow);

  // 더블클릭 → #sheet-image-dialog 크기·원래 크기로·비율 유지·대체 텍스트 변경
  card = layer.querySelector(".sheet-image-card");
  card.dispatchEvent({ type: "dblclick", target: card });
  await flushAsyncTurns(4);

  const dlg = document.getElementById("sheet-image-dialog");
  assert.ok(dlg && dlg.open, "#sheet-image-dialog 열림");
  assert.equal(document.getElementById("sheet-image-width").value, "336");
  assert.equal(document.getElementById("sheet-image-height").value, "208");

  userClick(document.getElementById("btn-sheet-image-reset"));
  assert.equal(document.getElementById("sheet-image-width").value, "240");
  assert.equal(document.getElementById("sheet-image-height").value, "160");

  const wInput = document.getElementById("sheet-image-width");
  wInput.value = "300";
  wInput.dispatchEvent({ type: "input", target: wInput });
  assert.equal(document.getElementById("sheet-image-height").value, "200", "비율 유지로 높이 자동 계산");

  const altInput = document.getElementById("sheet-image-alt");
  altInput.value = "연간 보고서 로고";
  userClick(document.getElementById("btn-sheet-image-apply"));
  await flushAsyncTurns(6);

  assert.equal(dlg.open, false);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.widthPx, 300);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.heightPx, 200);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].alt, "연간 보고서 로고");

  // Delete 키로 개체 지우기
  card = layer.querySelector(".sheet-image-card");
  card.focus();
  card.dispatchEvent({ type: "keydown", key: "Delete", target: card });
  await flushAsyncTurns(6);
  assert.equal(layoutApi.imagesOf("Sheet1").length, 0, "Delete로 개체 지우기");
  assert.equal(layer.hidden, true);

  // Undo / Redo 단계별 완전 복원
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.imagesOf("Sheet1").length, 1, "Undo로 삭제 복원");
  assert.equal(layoutApi.imagesOf("Sheet1")[0].alt, "연간 보고서 로고");
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.widthPx, 300);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.widthPx, 336, "Undo로 대화상자 편집 이전 크기 복원");
  assert.equal(layoutApi.imagesOf("Sheet1")[0].alt, "분기_로고");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.widthPx, 240, "Undo로 리사이즈 이전 크기 복원");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.leftPx, initialAnchor.leftPx, "Undo로 이동 이전 위치 복원");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.leftPx, movedAnchor.leftPx, "Redo로 이동 재적용");

  // JPEG 파일 입력 경로 검증
  const fileInput = document.getElementById("sheet-image-file-input");
  assert.ok(fileInput, "#sheet-image-file-input 존재");
  const jpgBytes = makeValidJpegBytes(180, 120);
  fileInput.files = [{ name: "배너.jpg", size: jpgBytes.length, arrayBuffer: async () => jpgBytes.buffer }];
  fileInput.dispatchEvent({ type: "change", target: fileInput });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.imagesOf("Sheet1").length, 2);
  assert.equal(layoutApi.imagesOf("Sheet1")[1].mime, "image/jpeg");
  assert.equal(layoutApi.imagesOf("Sheet1")[1].anchor.widthPx, 180);
});

test("PD-256: 빈 바이트·손상된 이미지 Fail-Closed 거부, 시트당 64개 상한 및 시트 보호 차단 검증", async () => {
  const { document, userClick, host } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-media-guard",
  });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);

  const mediaApi = host.__EASYOFFICE_SHEET_MEDIA__;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

  const emptyRes = await mediaApi.insertPictureFromBytes(new Uint8Array(0), "empty.png");
  assert.equal(emptyRes.ok, false, "빈 바이트 거부");
  assert.equal(layoutApi.imagesOf("Sheet1").length, 0);

  const corruptPng = await mediaApi.insertPictureFromBytes(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x00, 0x01]), "bad.png");
  assert.equal(corruptPng.ok, false, "손상된 PNG 바이트 거부");
  const gifBytes = await mediaApi.insertPictureFromBytes(new Uint8Array([0x47, 0x49, 0x46, 0x38, 0x39, 0x61]), "anim.gif");
  assert.equal(gifBytes.ok, false, "미지원 포맷 바이트 거부");
  assert.equal(layoutApi.imagesOf("Sheet1").length, 0);

  const okRes = await mediaApi.insertPictureFromBytes(makeValidPngBytes(120, 80), "valid.png");
  assert.equal(okRes.ok, true);
  assert.equal(layoutApi.imagesOf("Sheet1").length, 1);
  const beforeAnchor = structuredClone(layoutApi.imagesOf("Sheet1")[0].anchor);

  // 시트 보호 활성화 (개체 편집 잠금)
  layoutApi.setProtect("Sheet1", { on: true, deny: ["objects"] });
  await flushAsyncTurns(4);

  const blockedInsert = await mediaApi.insertPictureFromBytes(makeValidPngBytes(100, 100), "blocked.png");
  assert.equal(blockedInsert.ok, false, "시트 보호 상태에서 그림 삽입 차단");
  assert.equal(layoutApi.imagesOf("Sheet1").length, 1);

  // 보호 알림 닫기
  document.getElementById("sheet-protect-alert-dialog")?.close?.();

  const card = document.querySelector("#sheet-image-layer .sheet-image-card");
  card.dispatchEvent({ type: "mousedown", target: card, button: 0, clientX: 100, clientY: 100 });
  document.dispatchEvent({ type: "mousemove", clientX: 300, clientY: 250 });
  document.dispatchEvent({ type: "mouseup", clientX: 300, clientY: 250 });
  await flushAsyncTurns(4);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.leftPx, beforeAnchor.leftPx, "보호 상태에서 이동 차단");

  document.getElementById("sheet-protect-alert-dialog")?.close?.();
  card.focus();
  card.dispatchEvent({ type: "keydown", key: "Delete", target: card });
  await flushAsyncTurns(4);
  assert.equal(layoutApi.imagesOf("Sheet1").length, 1, "보호 상태에서 삭제 차단");

  document.getElementById("sheet-protect-alert-dialog")?.close?.();
  userClick(document.querySelector('#menu-bar [data-sheet-act="insert-picture"]'));
  await flushAsyncTurns(4);

  layoutApi.setProtect("Sheet1", null);
  await flushAsyncTurns(4);

  const existing = layoutApi.imagesOf("Sheet1")[0];
  const maxList = Array.from({ length: 64 }, (_, i) => ({ ...existing, id: `pic-${i + 1}` }));
  layoutApi.setImages("Sheet1", maxList);
  assert.equal(layoutApi.imagesOf("Sheet1").length, 64);
  assert.throws(
    () => layoutApi.setImages("Sheet1", [...maxList, { ...existing, id: "pic-65" }]),
    /64/,
    "시트당 64개 초과 Fail-Closed"
  );
});

test("PD-256: 셀 메모(Shift+F2·새 메모·메모 삽입·호버/상시 카드·우클릭 컨텍스트 메뉴 불변식·편집/삭제/표시토글·Undo/Redo·시트 보호) 검증", async () => {
  const { document, sandbox, userClick, host } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-notes",
  });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);

  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const noteLayer = document.getElementById("sheet-note-layer");
  const noteDlg = document.getElementById("sheet-note-dialog");
  assert.ok(noteLayer, "#sheet-note-layer 존재");
  assert.ok(noteDlg, "#sheet-note-dialog 존재");

  // 1) Shift+F2로 B2에 새 메모 삽입 (visible: false)
  const b2 = selectCell(document, "B2");
  await flushAsyncTurns(2);
  b2.dispatchEvent({ type: "keydown", key: "F2", shiftKey: true, target: b2, bubbles: true });
  await flushAsyncTurns(4);
  assert.equal(noteDlg.open, true, "Shift+F2로 메모 삽입 대화상자 오픈");
  assert.equal(document.getElementById("sheet-note-dialog-title").textContent, "메모 삽입");
  assert.equal(document.getElementById("btn-sheet-note-delete").hidden, true, "신규 삽입 시 메모 삭제 단추 숨김");

  document.getElementById("sheet-note-author").value = "기획팀";
  document.getElementById("sheet-note-text").value = "목표 대비 차이를 검토하세요.";
  document.getElementById("sheet-note-visible").checked = false;
  userClick(document.getElementById("btn-sheet-note-apply"));
  await flushAsyncTurns(6);

  assert.equal(noteDlg.open, false);
  assert.equal(b2.dataset.note, "true", "셀 메모 표시자 dataset.note 설정");
  assert.deepEqual(layoutApi.noteAtRef("Sheet1", "B2"), {
    ref: "B2",
    author: "기획팀",
    text: "목표 대비 차이를 검토하세요.",
    visible: false,
  });

  // 포커스를 A1으로 옮기면 비표시(visible: false) 메모 카드는 숨겨지고, B2 호버 시 표시됨
  selectCell(document, "A1");
  await flushAsyncTurns(2);
  assert.equal(noteLayer.querySelector('[data-note-ref="B2"]'), null, "포커스/호버 없을 때 숨김 메모 미표시");

  b2.dispatchEvent({ type: "mouseover", target: b2, bubbles: true });
  await flushAsyncTurns(2);
  const hoverCard = noteLayer.querySelector('.sheet-note-card[data-note-ref="B2"]');
  assert.ok(hoverCard && !noteLayer.hidden, "호버 시 .sheet-note-card 렌더링");
  assert.match(hoverCard.textContent, /기획팀/);
  assert.match(hoverCard.textContent, /목표 대비 차이를 검토하세요\./);

  const a1 = document.querySelector('#pane-sheet td[data-cell="A1"]');
  a1.dispatchEvent({ type: "mouseover", target: a1, bubbles: true });
  await flushAsyncTurns(2);
  assert.equal(noteLayer.querySelector('[data-note-ref="B2"]'), null, "다른 빈 셀로 호버 이동 시 숨김");

  // 2) 도구 > 새 메모 (sheet-review-comment) 진입점으로 D4에 상시 표시(visible: true) 메모 삽입
  selectCell(document, "D4");
  await flushAsyncTurns(2);
  userClick(document.querySelector('#menu-bar [data-studio-menu-cmd="review-comment"]'));
  await flushAsyncTurns(6);
  assert.equal(noteDlg.open, true, "도구 > 새 메모 리본 명령으로 메모 대화상자 오픈");
  document.getElementById("sheet-note-author").value = "재무팀";
  document.getElementById("sheet-note-text").value = "예산 상한 고정";
  document.getElementById("sheet-note-visible").checked = true;
  userClick(document.getElementById("btn-sheet-note-apply"));
  await flushAsyncTurns(6);

  selectCell(document, "A1");
  await flushAsyncTurns(2);
  assert.ok(
    noteLayer.querySelector('.sheet-note-card[data-note-ref="D4"]'),
    "visible: true 메모는 다른 셀 선택 중에도 상시 표시"
  );

  // 3) 삽입 > 메모 삽입 (note-insert) 진입점으로 C3에 메모 추가
  selectCell(document, "C3");
  await flushAsyncTurns(2);
  userClick(document.querySelector('#menu-bar [data-sheet-act="note-insert"]'));
  await flushAsyncTurns(4);
  assert.equal(noteDlg.open, true);
  document.getElementById("sheet-note-text").value = "분기 마감 확인";
  document.getElementById("sheet-note-visible").checked = false;
  userClick(document.getElementById("btn-sheet-note-apply"));
  await flushAsyncTurns(6);
  assert.equal(layoutApi.notesOf("Sheet1").length, 3);

  // 4) 우클릭 컨텍스트 메뉴(#sheet-header-menu) 불변식 검증!
  const menu = document.getElementById("sheet-header-menu");
  const shown = () => [...menu.querySelectorAll(".md-item")].filter((el) => !el.hidden).map((el) => el.textContent);

  // 일반 빈 셀(F6) 우클릭: menu.hidden === true 유지
  const f6 = document.querySelector('#pane-sheet td[data-cell="F6"]');
  f6.dispatchEvent({ type: "contextmenu", target: f6, button: 2, clientX: 50, clientY: 50, bubbles: true });
  assert.equal(menu.hidden, true, "링크·메모 없는 일반 셀 우클릭은 메뉴를 열지 않음");

  // 링크 전용 셀(E5) 우클릭: 기존 하이퍼링크 2항목만 표시
  selectCell(document, "E5");
  userClick(document.querySelector('#menu-bar [data-studio-menu-cmd="insert-link"]'));
  await flushAsyncTurns(6);
  const linkAddr = document.getElementById("sheet-link-address");
  linkAddr.value = "https://example.com/report";
  linkAddr.dispatchEvent({ type: "input", target: linkAddr });
  userClick(document.getElementById("btn-sheet-link-apply"));
  await flushAsyncTurns(10);

  const e5 = document.querySelector('#pane-sheet td[data-cell="E5"]');
  e5.dispatchEvent({ type: "contextmenu", target: e5, button: 2, clientX: 60, clientY: 60, bubbles: true });
  assert.equal(menu.hidden, false);
  assert.deepEqual(shown(), ["하이퍼링크 고치기...", "하이퍼링크 지우기"], "링크 전용 셀 불변식 보존");
  document.dispatchEvent({ type: "mousedown", target: document.body });

  // 메모 전용 셀(B2) 우클릭: B2 선택 + 메모 3항목 표시
  b2.dispatchEvent({ type: "contextmenu", target: b2, button: 2, clientX: 40, clientY: 40, bubbles: true });
  assert.equal(menu.hidden, false);
  assert.equal(document.getElementById("active-cell-ref").textContent, "B2", "우클릭 셀 선택");
  assert.deepEqual(shown(), ["메모 편집", "메모 삭제", "메모 표시/숨기기"]);

  // 우클릭 메뉴 > 메모 편집
  userClick(menu.querySelector('[data-sheet-act="note-edit"]'));
  await flushAsyncTurns(4);
  assert.equal(noteDlg.open, true);
  assert.equal(document.getElementById("sheet-note-dialog-title").textContent, "메모 편집");
  assert.equal(document.getElementById("btn-sheet-note-delete").hidden, false);
  document.getElementById("sheet-note-text").value = "수정된 검토 메모";
  userClick(document.getElementById("btn-sheet-note-apply"));
  await flushAsyncTurns(6);
  assert.equal(layoutApi.noteAtRef("Sheet1", "B2").text, "수정된 검토 메모");

  // 링크+메모 동시 보유 셀(E5) 우클릭: 링크 2항목 + 메모 3항목 모두 표시
  selectCell(document, "E5");
  sandbox.dispatchWindowKeydown({ key: "F2", shiftKey: true });
  await flushAsyncTurns(4);
  document.getElementById("sheet-note-text").value = "링크 셀 메모";
  userClick(document.getElementById("btn-sheet-note-apply"));
  await flushAsyncTurns(6);
  e5.dispatchEvent({ type: "contextmenu", target: e5, button: 2, clientX: 60, clientY: 60, bubbles: true });
  assert.deepEqual(shown(), ["하이퍼링크 고치기...", "하이퍼링크 지우기", "메모 편집", "메모 삭제", "메모 표시/숨기기"]);
  document.dispatchEvent({ type: "mousedown", target: document.body });

  // 삽입 > 메모 표시/숨기기 (워크시트 전체 일괄 토글)
  const toggleAllMenu = document.querySelector('#menu-bar [data-sheet-act="note-toggle-all"]');
  assert.equal(toggleAllMenu.getAttribute("aria-checked"), "false");
  userClick(toggleAllMenu);
  await flushAsyncTurns(6);
  assert.equal(toggleAllMenu.getAttribute("aria-checked"), "true", "전체 메모 표시 시 체크됨");
  assert.equal(noteLayer.querySelectorAll(".sheet-note-card").length, 4, "4개 메모 모두 표시");

  userClick(toggleAllMenu);
  await flushAsyncTurns(6);
  assert.equal(toggleAllMenu.getAttribute("aria-checked"), "false", "다시 클릭 시 전체 메모 숨김");

  // 우클릭 > 메모 삭제 & Undo/Redo
  const c3 = document.querySelector('#pane-sheet td[data-cell="C3"]');
  c3.dispatchEvent({ type: "contextmenu", target: c3, button: 2, clientX: 40, clientY: 40, bubbles: true });
  userClick(menu.querySelector('[data-sheet-act="note-delete"]'));
  await flushAsyncTurns(6);
  assert.equal(layoutApi.noteAtRef("Sheet1", "C3"), null);
  assert.equal(c3.dataset.note, undefined);

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.noteAtRef("Sheet1", "C3")?.text, "분기 마감 확인", "Undo로 삭제된 메모 복원");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="C3"]').dataset.note, "true");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(layoutApi.noteAtRef("Sheet1", "C3"), null, "Redo로 메모 재삭제");

  // 시트 보호 시 메모 편집/삭제 차단
  layoutApi.setProtect("Sheet1", { on: true, deny: ["objects"] });
  await flushAsyncTurns(4);
  b2.dispatchEvent({ type: "contextmenu", target: b2, button: 2, clientX: 40, clientY: 40, bubbles: true });
  userClick(menu.querySelector('[data-sheet-act="note-delete"]'));
  await flushAsyncTurns(4);
  assert.ok(layoutApi.noteAtRef("Sheet1", "B2"), "시트 보호 상태에서 메모 삭제 차단");
  document.getElementById("sheet-protect-alert-dialog")?.close?.();
});

test("PD-256: 행·열 삽입/삭제(structure) 시 부동 그림 anchor(imageId 보존)와 셀 메모 ref 이동·삭제 및 Undo 복원 검증", async () => {
  const { sandbox: bootSandbox } = await createWebUiDom();
  const cat = bootSandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = bootSandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const structureCalls = [];
  const { document, sandbox, userClick, host } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-structure-media",
    testIpc: async (command, payload) => {
      if (command === "get_workspace_bootstrap") {
        return { ...offline.get_workspace_bootstrap(payload), sheetSessions: true };
      }
      if (command === "evaluate_sheet") {
        const body = JSON.parse(payload);
        if (body.sessionAction === "structure") structureCalls.push(body);
      }
      return offline[command](payload);
    },
  });
  await flushAsyncTurns(10);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);

  const mediaApi = host.__EASYOFFICE_SHEET_MEDIA__;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

  // C3(fromCol: 2, fromRow: 2)에 그림 삽입
  selectCell(document, "C3");
  await flushAsyncTurns(2);
  await mediaApi.insertPictureFromBytes(makeValidPngBytes(192, 96), "구조이동.png");
  await flushAsyncTurns(6);
  const origImageId = layoutApi.imagesOf("Sheet1")[0].imageId;
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.fromCol, 2);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.fromRow, 2);

  // B2, C3, D5에 셀 메모 설정
  layoutApi.setNotes("Sheet1", [
    { ref: "B2", author: "작성자", text: "위쪽 메모", visible: false },
    { ref: "C3", author: "작성자", text: "이동 후 삭제될 메모", visible: true },
    { ref: "D5", author: "작성자", text: "아래쪽 메모", visible: false },
  ]);
  await flushAsyncTurns(4);

  // 3행 위에 2행 삽입 (C3 선택 후 row-insert-above -> 1행 삽입, 3~4행 범위 선택 시 2행 삽입)
  selectCell(document, "C3");
  document.activeElement.dispatchEvent({ type: "keydown", key: "ArrowDown", shiftKey: true, target: document.activeElement, bubbles: true });
  userClick(document.querySelector('.md-item[data-sheet-act="row-insert-above"]'));
  await flushAsyncTurns(20);

  assert.equal(structureCalls.length, 1);
  assert.equal(structureCalls[0].images[0].data, undefined, "structure 요청에 대용량 Base64 data 미포함");
  assert.equal(layoutApi.imagesOf("Sheet1")[0].imageId, origImageId, "이동 후에도 imageId 보존");
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.fromRow, 4, "2행 삽입 후 그림 fromRow: 2 -> 4 이동");
  assert.ok(layoutApi.noteAtRef("Sheet1", "B2"), "삽입 지점 위 B2 메모 유지");
  assert.equal(layoutApi.noteAtRef("Sheet1", "C3"), null);
  assert.equal(layoutApi.noteAtRef("Sheet1", "C5")?.text, "이동 후 삭제될 메모", "C3 -> C5 이동");
  assert.equal(layoutApi.noteAtRef("Sheet1", "D7")?.text, "아래쪽 메모", "D5 -> D7 이동");

  // 5행 삭제 -> C5 메모 제거, D7 -> D6 이동
  selectCell(document, "C5");
  userClick(document.querySelector('.md-item[data-sheet-act="row-delete"]'));
  await flushAsyncTurns(20);
  assert.equal(layoutApi.noteAtRef("Sheet1", "C5"), null, "삭제된 행의 메모 제거");
  assert.equal(layoutApi.noteAtRef("Sheet1", "D6")?.text, "아래쪽 메모", "D7 -> D6 당겨짐");

  // C열 왼쪽에 1열 삽입 -> 그림 fromCol: 2 -> 3, D6 -> E6 이동
  selectCell(document, "C2");
  userClick(document.querySelector('.md-item[data-sheet-act="col-insert-left"]'));
  await flushAsyncTurns(20);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.fromCol, 3, "열 삽입 후 그림 fromCol: 2 -> 3 이동");
  assert.equal(layoutApi.noteAtRef("Sheet1", "E6")?.text, "아래쪽 메모", "D6 -> E6 이동");

  // Undo로 열 삽입 취소 -> 행 삭제 취소(C5 메모 복원!)
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.fromCol, 2);
  assert.equal(layoutApi.noteAtRef("Sheet1", "D6")?.text, "아래쪽 메모");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.equal(layoutApi.noteAtRef("Sheet1", "C5")?.text, "이동 후 삭제될 메모", "행 삭제 Undo 시 삭제됐던 메모 복원");
});

test("PD-256: office-print(.eo-print-image)·시트 복사·save-payload Base64 물질화 및 file-open 100% 재적재 복원 검증", async () => {
  class FakeStyleSheet {
    constructor() { this.cssRules = []; }
    insertRule(text, at) {
      this.cssRules.splice(at, 0, { cssText: text, style: { props: {}, setProperty(k, v) { this.props[k] = v; } } });
      return at;
    }
  }
  const printedRoots = [];
  const { document, sandbox, userClick, host } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-sheet-media-io-print",
  });
  host.CSSStyleSheet = FakeStyleSheet;
  host.requestAnimationFrame = (cb) => cb();
  host.getComputedStyle = () => ({ getPropertyValue: (p) => (p === "font-size" ? "11pt" : "") });
  host.print = () => { printedRoots.push(document.getElementById("eo-print-root")); };
  document.adoptedStyleSheets = [];
  await flushAsyncTurns(10);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);

  const mediaApi = host.__EASYOFFICE_SHEET_MEDIA__;
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

  // 빈 셀만 있는 시트에 부동 그림만 넣고 인쇄 미리보기 실행
  selectCell(document, "B2");
  const pngBytes = makeValidPngBytes(200, 120);
  await mediaApi.insertPictureFromBytes(pngBytes, "인쇄로고.png");
  await flushAsyncTurns(6);

  document.defaultView.__EASYOFFICE_NATIVE_MENU__("file:print");
  await flushAsyncTurns(10);
  assert.equal(printedRoots.length, 1, "셀 텍스트 없이 그림만 있는 시트도 인쇄 실행");
  const printImg = printedRoots[0].querySelector(".eo-print-sheet-box .eo-print-image");
  assert.ok(printImg, ".eo-print-image 오버레이 렌더링");
  assert.equal(printImg.tagName, "IMG");
  assert.equal(printImg.getAttribute("alt"), "인쇄로고");
  assert.ok(printImg.getAttribute("src"), ".eo-print-image src 설정");
  sandbox.dispatchWindowEvent("afterprint");

  // 데이터 셀 및 시트 하단(B45) 메모 추가
  setCell(document, "A1", "항목");
  await flushAsyncTurns(4);
  layoutApi.setNotes("Sheet1", [
    { ref: "B2", author: "감사실", text: "원본 증빙 대조", visible: true },
    { ref: "B45", author: "감사실", text: "하단 빈 셀 메모", visible: false },
  ]);
  await flushAsyncTurns(4);

  // 시트 복사 시 images와 notes 복제 검증
  userClick(document.querySelector('#menu-bar [data-sheet-act="tab-copy"]'));
  await flushAsyncTurns(14);
  assert.equal(layoutApi.imagesOf("시트 1 (2)").length, 1, "시트 복사본에 그림 복제");
  assert.equal(layoutApi.notesOf("시트 1 (2)").length, 2, "시트 복사본에 메모 복제");

  // save-payload 직렬화 검증 (Base64 data 물질화 및 내부 전용 필드 제거)
  const saveApi = host.__EASYOFFICE_SAVE_PAYLOAD__;
  const sheetsPayload = saveApi.buildSheetsPayload(
    ["Sheet1"],
    { Sheet1: { A1: { value: "항목", formula: "" } } },
    {},
    {},
    layoutApi.getAll()
  );
  assert.equal(sheetsPayload[0].images.length, 1);
  assert.equal(typeof sheetsPayload[0].images[0].data, "string");
  assert.ok(sheetsPayload[0].images[0].data.length > 40, "Base64 data 물질화");
  assert.equal(sheetsPayload[0].images[0].imageId, undefined, "내부 imageId 필드 제거");
  assert.equal(sheetsPayload[0].images[0].naturalWidthPx, undefined, "내부 naturalWidthPx 필드 제거");
  assert.equal(sheetsPayload[0].notes.length, 2);
  assert.deepEqual(sheetsPayload[0].notes[0], {
    ref: "B2",
    author: "감사실",
    text: "원본 증빙 대조",
    visible: true,
  });

  // 새 하니스에서 file-open 재적재 복원 검증
  const reopened = await createWorkspaceHarness({ search: "?module=SHEET&win=w-sheet-reopen" });
  await flushAsyncTurns(8);
  await pickFile(reopened.document, makeFixtureFile("media-notes.xlsx", { sheets: sheetsPayload }), 30);

  const reLayout = reopened.host.__EASYOFFICE_SHEET_LAYOUT__;
  assert.equal(reLayout.imagesOf("Sheet1").length, 1, "재적재 후 부동 그림 복원");
  assert.equal(reLayout.imagesOf("Sheet1")[0].alt, "인쇄로고");
  assert.ok(reLayout.imagesOf("Sheet1")[0].imageId, "재적재 시 imageId로 등록");
  assert.equal(reopened.document.getElementById("sheet-image-layer").hidden, false);
  assert.ok(reopened.document.querySelector("#sheet-image-layer .sheet-image-card img")?.getAttribute("src"));

  assert.equal(reLayout.notesOf("Sheet1").length, 2, "재적재 후 셀 메모 2개 복원");
  assert.equal(reopened.document.querySelector('#pane-sheet td[data-cell="B2"]')?.dataset.note, "true");
  assert.ok(
    reopened.document.querySelector('#sheet-note-layer .sheet-note-card[data-note-ref="B2"]'),
    "재적재 직후 visible: true 메모 카드 즉시 렌더링"
  );
  const built = reopened.host.__EASYOFFICE_FILE_OPEN_UTILS__.buildSheetStore(sheetsPayload);
  assert.ok(built.rowCounts.Sheet1 >= 45, "값 없는 하단 셀(B45) 메모도 maxRow에 반영");
});

test("PD-299: 그림의 칸 앵커는 행 높이와 기본 행 높이를 따라 계산된다", async () => {
  const { document, host } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-sheet-media-rows" });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const media = host.__EASYOFFICE_SHEET_MEDIA__;
  // 기본 행 30pt(40px)라 행 위쪽 끝은 0, 40, 80, 120, 160, 200. (행마다 정한 높이는 글자 폭을 재는 격자가 있어야 하므로
  // 실제 WebKit 단계 `scripts/webkit-dom/sheet.mjs`가 본다.)
  layoutApi.setDefaultRowHeight("Sheet1", 30);
  await flushAsyncTurns(4);
  // 5행(0부터 4)은 160에서 시작하니 170은 그 안 10px 아래이고, 60px 높이의 끝 230은 6행(0부터 5) 안 30px 아래다.
  const a = media.anchorFromContentRect(0, 170, 100, 60);
  assert.deepEqual([a.fromRow, a.rowOffEmu, a.toRow, a.toRowOffEmu], [4, 10 * 9525, 5, 30 * 9525]);
  assert.deepEqual([a.topPx, a.heightPx], [170, 60]);
  const inside = media.anchorFromContentRect(0, 100, 50, 40);
  assert.deepEqual([inside.fromRow, inside.rowOffEmu, inside.toRow, inside.toRowOffEmu], [2, 20 * 9525, 3, 20 * 9525]);
  // 기본 행 높이가 없던 시트는 24px 행이다.
  layoutApi.setDefaultRowHeight("Sheet1", null);
  await flushAsyncTurns(4);
  const plain = media.anchorFromContentRect(0, 50, 50, 24);
  assert.deepEqual([plain.fromRow, plain.rowOffEmu, plain.toRow, plain.toRowOffEmu], [2, 2 * 9525, 3, 2 * 9525]);
});

test("PD-299: 두 칸 앵커 그림은 행 높이를 따라 크기가 변하고, 한 칸 앵커는 크기를 지키고, 절대 앵커는 자리를 지킨다", async () => {
  const { document, host } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-sheet-media-edit-as" });
  await flushAsyncTurns(8);
  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const media = host.__EASYOFFICE_SHEET_MEDIA__;
  selectCell(document, "B2");
  await flushAsyncTurns(2);
  assert.equal((await media.insertPictureFromBytes(makeValidPngBytes(240, 160), "p.png")).ok, true);
  await flushAsyncTurns(6);
  const [created] = layoutApi.imagesOf("Sheet1");
  assert.equal(created.anchor.editAs, "twoCell", "새 그림은 두 칸 앵커다");
  const card = () => document.querySelector(".sheet-image-card");
  const rect = () => ({ top: card().style.top, width: card().style.width, height: card().style.height });
  const before = rect();
  assert.equal(before.height, "160px");
  // 기본 행 높이 30pt(40px): 시작 칸(2행)은 40px 아래, 끝 칸은 8행 안 16px이라 높이는 7 x 40 + 16 - 40 = 256px.
  layoutApi.setDefaultRowHeight("Sheet1", 30);
  await flushAsyncTurns(4);
  assert.equal(card().style.height, "256px", "크기가 칸을 따른다");
  assert.equal(card().style.width, before.width, "열은 그대로다");
  const kinds = async (editAs) => {
    layoutApi.setImages("Sheet1", [{ ...created, anchor: { ...created.anchor, editAs } }]);
    await flushAsyncTurns(4);
    return rect();
  };
  const one = await kinds("oneCell");
  assert.equal(one.height, "160px", "한 칸 앵커는 크기를 지킨다");
  assert.notEqual(one.top, before.top, "시작 칸을 따라 내려간다");
  const abs = await kinds("absolute");
  assert.equal(abs.height, "160px");
  assert.equal(abs.top, before.top, "절대 앵커는 자리를 지킨다");
  // 알 수 없는 값은 두 칸 앵커다.
  const odd = await kinds("sideways");
  assert.equal(odd.height, "256px");
  assert.equal(layoutApi.imagesOf("Sheet1")[0].anchor.editAs, "twoCell");
});
