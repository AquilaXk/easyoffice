import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { findHtmlSinkViolations } from "./html-sink-rule.mjs";
import { findBareSorts } from "./sort-comparator-rule.mjs";
import { prohibitedPatternsIn } from "./prohibited-terms.mjs";
import { ossNameHits } from "./oss-names.mjs";

const BASE_MODEL_URL = new URL("../crates/easyoffice-app/ui/base-model.js", import.meta.url);
const BASE_STUDIO_URL = new URL("../crates/easyoffice-app/ui/base-studio.js", import.meta.url);
const BASE_CSS_URL = new URL("../crates/easyoffice-app/ui/base.css", import.meta.url);
const THIS_TEST_URL = new URL("./test-base-engine-pd259.mjs", import.meta.url);

function loadModelOnly() {
  const env = {};
  loadUiModule(BASE_MODEL_URL, env);
  return env.__EASYOFFICE_BASE_MODEL__;
}

function accessibleName(el, document) {
  const aria = (el.getAttribute("aria-label") || "").trim();
  if (aria) return aria;
  const by = (el.getAttribute("aria-labelledby") || "").trim();
  if (by) {
    const text = by.split(/\s+/).map((id) => document.getElementById(id)?.textContent || "").join(" ").trim();
    if (text) return text;
  }
  if (el.id) {
    const label = document.querySelectorAll("label").find((l) => l.getAttribute("for") === el.id);
    if (label?.textContent.trim()) return label.textContent.trim();
  }
  const wrapping = el.closest("label");
  if (wrapping && wrapping.textContent.trim()) return wrapping.textContent.trim();
  if (el.textContent.trim()) return el.textContent.trim();
  return (el.getAttribute("title") || "").trim();
}

function assertAllLabeled(root, document, contextLabel) {
  const controls = root.querySelectorAll('button, select, textarea, input:not([type="hidden"]), [role="tab"]');
  const unlabeled = controls.filter((el) => accessibleName(el, document) === "").map((el) => el.id || el.className || el.tagName);
  assert.deepEqual(unlabeled, [], `unlabeled controls in ${contextLabel}`);
}

test("PD-259 코어 모델: 빈 초기 상태(레코드 0건), 엄격한 예산·스키마 검증 및 직렬화 왕복", () => {
  const M = loadModelOnly();
  assert.ok(M, "base model loaded");
  assert.equal(M.MAX_TABLES, 32);
  assert.equal(M.MAX_FIELDS_PER_TABLE, 64);
  assert.equal(M.MAX_RECORDS_PER_TABLE, 10000);
  assert.equal(M.MAX_CELL_CHARS, 8192);
  assert.equal(M.MAX_FORMULA_DEPTH, 32);

  const state = M.createInitialBaseState();
  assert.equal(state.tables.length, 1);
  assert.equal(state.tables[0].name, "테이블 1");
  assert.equal(state.tables[0].view, "grid");
  assert.equal(state.tables[0].records.length, 0);
  assert.deepEqual(state.tables[0].fields.map((f) => [f.name, f.type]), [
    ["제목", "text"],
    ["상태", "select"],
    ["마감일", "date"],
    ["금액", "number"],
  ]);
  assert.deepEqual(state.tables[0].fields[1].options, ["대기", "진행 중", "완료"]);

  // Budget enforcement: >32 tables, >64 fields, >8192 chars, prototype pollution
  for (let i = 2; i <= 32; i += 1) {
    assert.equal(M.addTable(state, { name: `테이블 ${i}` }).ok, true);
  }
  assert.equal(M.addTable(state, { name: "초과 테이블" }).ok, false);

  const t1 = state.tables[0];
  for (let i = t1.fields.length + 1; i <= 64; i += 1) {
    assert.equal(M.addField(state, t1.id, { name: `필드 ${i}`, type: "text" }).ok, true);
  }
  assert.equal(M.addField(state, t1.id, { name: "초과 필드", type: "text" }).ok, false);

  const longText = "가".repeat(8193);
  assert.equal(M.validateCellValue(t1.fields[0], longText, state).ok, false);
  assert.equal(M.hydrateBaseState('{"__proto__":{"polluted":true}}').ok, false);
  assert.equal(M.hydrateBaseState("{bad json").ok, false);

  const snap = M.serializeBaseState(state);
  const restored = M.hydrateBaseState(JSON.stringify(snap));
  assert.equal(restored.ok, true);
  assert.equal(restored.state.tables.length, 32);
  assert.equal(restored.state.tables[0].fields.length, 64);

  // Max record cap strict guard
  const sCap = M.createInitialBaseState();
  sCap.tables[0].records = Array.from({ length: 10000 }, (_, i) => ({ id: `r-${i}`, values: { "fld-1": `n${i}` } }));
  assert.equal(M.addRecord(sCap, "tbl-1", { "fld-1": "overflow" }).ok, false);
});

test("PD-259 9종 필드 타입, 엄격한 유효성 검사, O(1) 관계 조회 및 수식·순환 방어", () => {
  const M = loadModelOnly();
  const state = M.createInitialBaseState();

  const custRes = M.addTable(state, {
    name: "고객사",
    fields: [
      { name: "회사명", type: "text", validation: { required: true, maxLength: 20 } },
      { name: "계약금", type: "number", validation: { min: 0, max: 100000 } },
    ],
  });
  assert.equal(custRes.ok, true);
  const custTbl = custRes.table;
  const c1 = M.addRecord(state, custTbl.id, { [custTbl.fields[0].id]: "한빛소프트", [custTbl.fields[1].id]: 2500 });
  const c2 = M.addRecord(state, custTbl.id, { [custTbl.fields[0].id]: "푸른테크", [custTbl.fields[1].id]: 1500 });
  assert.equal(c1.ok && c2.ok, true);

  const mainTbl = state.tables[0];
  const fTags = M.addField(state, mainTbl.id, { name: "태그", type: "multiSelect", options: ["긴급", "검토", "승인"] }).field;
  const fDone = M.addField(state, mainTbl.id, { name: "완료여부", type: "checkbox" }).field;
  const fUrl = M.addField(state, mainTbl.id, { name: "링크", type: "url" }).field;
  const fRel = M.addField(state, mainTbl.id, { name: "고객", type: "relation", relationTableId: custTbl.id }).field;
  const fVat = M.addField(state, mainTbl.id, { name: "부가세포함", type: "formula", formula: "ROUND({금액} * 1.1, 0)" }).field;
  const fTier = M.addField(state, mainTbl.id, { name: "등급", type: "formula", formula: 'IF({금액} >= 1000, "고액", "일반")' }).field;
  const fDays = M.addField(state, mainTbl.id, { name: "잔여일", type: "formula", formula: 'DAYS({마감일}, "2026-11-01")' }).field;
  const fSummary = M.addField(state, mainTbl.id, { name: "요약", type: "formula", formula: 'CONCAT(TRIM({제목}), " - ", {상태})' }).field;
  const fDivZero = M.addField(state, mainTbl.id, { name: "나눗셈", type: "formula", formula: "{금액} / 0" }).field;
  const fCycA = M.addField(state, mainTbl.id, { name: "순환A", type: "formula", formula: "{순환B} + 1" }).field;
  const fCycB = M.addField(state, mainTbl.id, { name: "순환B", type: "formula", formula: "{순환A} + 1" }).field;
  const fBad = M.addField(state, mainTbl.id, { name: "오류식", type: "formula", formula: "({금액} +" }).field;

  // Strict validation rejection checks
  assert.equal(M.validateCellValue(custTbl.fields[0], "   ", state).ok, false, "required text rejected");
  assert.equal(M.validateCellValue(custTbl.fields[0], "가".repeat(21), state).ok, false, "maxLength rejected");
  assert.equal(M.validateCellValue(mainTbl.fields[3], -1, Object.assign({}, state)).ok, true);
  assert.equal(M.validateCellValue(custTbl.fields[1], -5, state).ok, false, "min rejected");
  assert.equal(M.validateCellValue(custTbl.fields[1], 100001, state).ok, false, "max rejected");
  assert.equal(M.validateCellValue(custTbl.fields[1], "not-a-num", state).ok, false, "NaN rejected");
  assert.equal(M.validateCellValue(mainTbl.fields[1], "임의상태", state).ok, false, "unlisted select option rejected");
  assert.equal(M.validateCellValue(fTags, ["긴급", "없는옵션"], state).ok, false, "unlisted multiSelect rejected");
  assert.equal(M.validateCellValue(fTags, "긴급, 승인", state).ok, true, "comma-separated multiSelect parsed");
  assert.equal(M.validateCellValue(mainTbl.fields[2], "2026-02-30", state).ok, false, "invalid calendar date rejected");
  assert.equal(M.validateCellValue(mainTbl.fields[2], "2024-02-29", state).ok, true, "leap year date allowed");
  assert.equal(M.validateCellValue(fUrl, "javascript:alert(1)", state).ok, false, "javascript: URL rejected");
  assert.equal(M.validateCellValue(fUrl, "data:text/html,hi", state).ok, false, "data: URL rejected");
  assert.equal(M.validateCellValue(fUrl, "https://example.com/a", state).ok, true, "https URL allowed");
  assert.equal(M.validateCellValue(fRel, "missing-rec", state).ok, false, "missing relation ID rejected");
  assert.equal(M.validateCellValue(fVat, 123, state).ok, false, "formula field read-only");

  // Add valid record and evaluate formulas + O(1) relation display & rollup
  const rAdd = M.addRecord(state, mainTbl.id, {
    "fld-1": "  계약 검토  ",
    "fld-2": "진행 중",
    "fld-3": "2026-11-15",
    "fld-4": 1500,
    [fTags.id]: ["긴급", "검토"],
    [fDone.id]: true,
    [fUrl.id]: "https://easyoffice.example/doc",
    [fRel.id]: [c1.record.id, c2.record.id],
  });
  assert.equal(rAdd.ok, true);
  assert.equal(M.resolveRelationDisplay(state, fRel, rAdd.record.values[fRel.id]), "한빛소프트, 푸른테크");
  assert.equal(M.rollupSum(state, fRel, rAdd.record.values[fRel.id], custTbl.fields[1].id), 4000);

  const q = M.queryTableView(state, mainTbl.id);
  const vals = q.rows[0].values;
  assert.equal(vals[fVat.id], 1650);
  assert.equal(vals[fTier.id], "고액");
  assert.equal(vals[fDays.id], 14);
  assert.equal(vals[fSummary.id], "계약 검토 - 진행 중");
  assert.equal(vals[fDivZero.id], "#DIV/0!");
  assert.equal(vals[fCycA.id], "#CYCLE!");
  assert.equal(vals[fCycB.id], "#CYCLE!");
  assert.equal(vals[fBad.id], "#ERROR!");

  // Additional formula functions & strict nesting depth bound (MAX_FORMULA_DEPTH = 32)
  assert.equal(M.evaluateFormula('AND(1 > 0, NOT(FALSE), OR(FALSE, 2 = 2))', rAdd.record, mainTbl, state), true);
  assert.equal(M.evaluateFormula('SUM(10, ABS(-5), MIN(3, 8), MAX(2, 9))', rAdd.record, mainTbl, state), 27);
  assert.equal(M.evaluateFormula('AVERAGE(10, 20, 30)', rAdd.record, mainTbl, state), 20);
  assert.equal(M.evaluateFormula('LEN(UPPER(LOWER(COALESCE("", "Abc"))))', rAdd.record, mainTbl, state), 3);
  assert.equal(M.evaluateFormula('"A" & "-" & "B"', rAdd.record, mainTbl, state), "A-B");
  const deepFormula = "(".repeat(40) + "1" + ")".repeat(40);
  assert.equal(M.compileFormula(deepFormula).ok, false, "formula exceeding MAX_FORMULA_DEPTH rejected");
  assert.equal(M.addField(state, mainTbl.id, { name: "깊이초과", type: "formula", formula: deepFormula }).ok, false, "addField rejects excessive nesting depth");
});

test("PD-259 단일 패스 필터(AND/OR)·다중 정렬·그룹화 집계 및 5,000건 성능 측정", () => {
  const M = loadModelOnly();
  const state = M.createInitialBaseState();
  const tbl = state.tables[0];
  const fCalc = M.addField(state, tbl.id, { name: "세후", type: "formula", formula: "ROUND({금액} * 0.9, 0)" }).field;

  const statuses = ["대기", "진행 중", "완료"];
  for (let i = 0; i < 5000; i += 1) {
    tbl.records.push({
      id: `rec-${i + 1}`,
      values: {
        "fld-1": `작업 ${String(5000 - i).padStart(4, "0")}`,
        "fld-2": statuses[i % 3],
        "fld-3": `2026-11-${String((i % 28) + 1).padStart(2, "0")}`,
        "fld-4": (i % 100) * 100,
      },
    });
  }

  tbl.filterConjunction = "and";
  tbl.filters = [
    { fieldId: "fld-4", op: "gte", value: "5000" },
    { fieldId: "fld-2", op: "neq", value: "대기" },
  ];
  tbl.sorts = [
    { fieldId: "fld-4", dir: "desc" },
    { fieldId: "fld-1", dir: "asc" },
  ];
  tbl.groupByFieldId = "fld-2";

  const t0 = performance.now();
  const q = M.queryTableView(state, tbl.id);
  const elapsedMs = performance.now() - t0;

  assert.equal(q.totalCount, 5000);
  assert.ok(q.filteredCount > 1000 && q.filteredCount < 2500);
  assert.equal(q.groups.length, 2, "grouped into 진행 중 and 완료");
  assert.ok(q.rows[0].values["fld-4"] >= q.rows[q.rows.length - 1].values["fld-4"]);
  assert.equal(q.rows[0].values[fCalc.id], Math.round(q.rows[0].values["fld-4"] * 0.9));
  const groupSum = q.groups.reduce((acc, g) => acc + g.numericSums["fld-4"], 0);
  assert.equal(groupSum, q.numericSums["fld-4"]);
  assert.ok(elapsedMs < 1500, `5,000-record single-pass query took ${elapsedMs.toFixed(2)} ms`);
  console.log(`PD-259 bench (5,000 records formula+filter+sort+group): ${elapsedMs.toFixed(2)} ms`);

  // OR conjunction test
  tbl.filterConjunction = "or";
  tbl.filters = [
    { fieldId: "fld-2", op: "eq", value: "대기" },
    { fieldId: "fld-4", op: "eq", value: "9900" },
  ];
  const qOr = M.queryTableView(state, tbl.id);
  assert.ok(qOr.filteredCount > 1600);

  // Natural alphanumeric chunked sort (`항목 2` < `항목 10`)
  const natState = M.createInitialBaseState();
  for (const label of ["항목 10", "항목 2", "항목 1"]) M.addRecord(natState, "tbl-1", { "fld-1": label });
  natState.tables[0].sorts = [{ fieldId: "fld-1", dir: "asc" }];
  const natRows = M.queryTableView(natState, "tbl-1").rows.map((r) => r.values["fld-1"]);
  assert.deepEqual(natRows, ["항목 1", "항목 2", "항목 10"]);
});

test("PD-259 XLSX sheets 페이로드 및 UTF-8 BOM RFC 4180 CSV/TSV 직렬화", () => {
  const M = loadModelOnly();
  const state = M.createInitialBaseState();
  const tbl = state.tables[0];
  const fDone = M.addField(state, tbl.id, { name: "확인", type: "checkbox" }).field;
  const fTags = M.addField(state, tbl.id, { name: "분류", type: "multiSelect", options: ["A", "B"] }).field;
  M.addRecord(state, tbl.id, {
    "fld-1": '기획, "1차"',
    "fld-2": "완료",
    "fld-3": "2026-11-20",
    "fld-4": 4200,
    [fDone.id]: true,
    [fTags.id]: ["A", "B"],
  });

  const sheets = M.toSheetsPayload(state);
  assert.equal(sheets.length, 1);
  assert.equal(sheets[0].name, "테이블 1");
  const byRef = new Map(sheets[0].cells.map((c) => [c.ref, c]));
  assert.equal(byRef.get("A1").raw, "제목");
  assert.equal(byRef.get("A1").style.bold, true);
  assert.equal(byRef.get("D2").raw, "4200");
  assert.equal(byRef.get("D2").t, "n");
  assert.equal(byRef.get("E2").raw, "TRUE");
  assert.equal(byRef.get("E2").t, "b");
  assert.equal(byRef.get("F2").raw, "A, B");

  const csv = M.toDelimitedText(state, tbl.id, ",");
  assert.ok(csv.startsWith("\uFEFF제목,상태,마감일,금액,확인,분류\r\n"));
  assert.ok(csv.includes('"기획, ""1차""",완료,2026-11-20,4200,TRUE,"A, B"'));

  const tsv = M.toDelimitedText(state, tbl.id, "\t");
  assert.ok(tsv.startsWith("\uFEFF제목\t상태\t마감일\t금액\t확인\t분류\r\n"));
});

test("PD-259 DOM 뷰 동기화(데이터시트 보기 ↔ 보드 드래그/선택 이동 ↔ 일정 월 이동), 유효성 경고, 내보내기, 접근성", async () => {
  const { document, host } = await createWorkspaceHarness();
  loadUiModule(BASE_MODEL_URL, host);
  loadUiModule(BASE_STUDIO_URL, host);

  const stage = document.querySelector(".canvas-stage");
  let root = document.getElementById("pane-base");
  if (!root) {
    root = document.createElement("section");
    root.id = "pane-base";
    root.className = "editor-pane base-pane";
    root.setAttribute("aria-label", "데이터베이스 편집기");
    stage.appendChild(root);
  }
  root.hidden = false;

  const feedbackLog = [];
  const exportsDelivered = [];
  let undoCount = 0;
  let activityCount = 0;

  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document,
    rootEl: root,
    invokeIpc: (cmd, payload) => host.__EASYOFFICE_INVOKE_BACKEND__(cmd, payload),
    deliverExport: (base64, fileBase, format) => {
      exportsDelivered.push({ base64, fileBase, format });
      return { ok: true, name: `${fileBase}.${format}` };
    },
    setFeedback: (msg) => feedbackLog.push(msg),
    recordUndo: () => { undoCount += 1; },
    noteActivity: () => { activityCount += 1; },
  });
  studio.bind();

  assert.equal(root.querySelectorAll(".base-grid-row").length, 0, "starts with 0 sample records");
  assert.ok(root.querySelector(".sheet-tab-bar #base-table-tabs"), "bottom sheet-tab-bar renders table tabs");
  assert.ok(root.querySelector(".base-new-record-row .base-inline-add-btn"), "inline trailing add-record row present");
  assertAllLabeled(root, document, "grid empty");

  // Add record via toolbar button
  document.getElementById("btn-base-add-record").click();
  await flushAsyncTurns(2);
  assert.equal(root.querySelectorAll(".base-grid-row").length, 1);
  assert.ok(undoCount >= 1 && activityCount >= 1);

  const recId = studio.getState().tables[0].records[0].id;
  // Invalid date via grid cell DOM fails closed without overwriting previous value
  const dateInput = root.querySelector(`.base-grid-row[data-record-id="${recId}"] td[data-field-id="fld-3"] input`);
  dateInput.value = "2026-02-30";
  dateInput.dispatchEvent({ type: "change", target: dateInput });
  assert.equal(studio.getState().tables[0].records[0].values["fld-3"], "");
  assert.equal(dateInput.dataset.invalid, "true");
  assert.equal(dateInput.getAttribute("aria-invalid"), "true");
  assert.ok(feedbackLog.length >= 1);

  // Set valid values via studio API and edit via field dialog
  assert.equal(studio.updateCell(recId, "fld-1", "기획안 작성").ok, true);
  assert.equal(studio.updateCell(recId, "fld-3", "2026-11-18").ok, true);
  assert.equal(studio.updateCell(recId, "fld-4", 5000).ok, true);

  document.getElementById("btn-base-add-field").click();
  const fieldDlg = document.getElementById("base-field-dialog");
  assert.equal(fieldDlg.open, true);
  assertAllLabeled(fieldDlg, document, "field dialog");
  document.getElementById("base-field-name").value = "두배금액";
  document.getElementById("base-field-type").value = "formula";
  document.getElementById("base-field-formula").value = "{금액} * 2";
  document.getElementById("btn-base-field-ok").click();
  assert.equal(fieldDlg.open, false);
  assert.match(root.querySelector(".base-formula-val").textContent, /10000/);

  // Switch to Kanban ("보드") and move card via drag-and-drop
  root.querySelector('[data-base-view="kanban"]').click();
  assertAllLabeled(root, document, "kanban view");
  const card = root.querySelector(`.base-kanban-card[data-record-id="${recId}"]`);
  const doneCol = root.querySelector('.base-kanban-col[data-kanban-value="완료"]');
  assert.ok(card && doneCol);
  card.dispatchEvent({ type: "dragstart", target: card, dataTransfer: { setData: () => {}, getData: () => recId } });
  doneCol.dispatchEvent({ type: "dragover", target: doneCol });
  doneCol.dispatchEvent({ type: "drop", target: doneCol, dataTransfer: { getData: () => recId } });
  assert.equal(studio.getState().tables[0].records[0].values["fld-2"], "완료");

  // Switch to Calendar ("일정"), verify record chip on 2026-11-18, navigate months, add record on 2026-12-05
  root.querySelector('[data-base-view="calendar"]').click();
  assertAllLabeled(root, document, "calendar view");
  assert.equal(document.getElementById("base-cal-title").textContent, "2026년 11월");
  const novCell = root.querySelector('.base-cal-cell[data-date="2026-11-18"]');
  assert.ok(novCell?.textContent.includes("기획안 작성"));
  document.getElementById("btn-base-cal-next").click();
  assert.equal(document.getElementById("base-cal-title").textContent, "2026년 12월");
  const decAddBtn = root.querySelector('.base-cal-cell[data-date="2026-12-05"] .base-cal-add-btn');
  decAddBtn.click();
  assert.equal(studio.getState().tables[0].records.length, 2);
  assert.equal(studio.getState().tables[0].records[1].values["fld-3"], "2026-12-05");
  document.getElementById("btn-base-cal-prev").click();
  assert.equal(document.getElementById("base-cal-title").textContent, "2026년 11월");

  // Switch back to Grid ("데이터시트 보기") and confirm synchronization
  root.querySelector('[data-base-view="grid"]').click();
  assert.equal(root.querySelectorAll(".base-grid-row").length, 2);
  assert.equal(studio.getState().tables[0].records[0].values["fld-2"], "완료");

  // Export XLSX / CSV / TSV
  for (const fmt of ["xlsx", "csv", "tsv"]) {
    const res = await studio.exportAs(fmt);
    assert.equal(res.ok, true, `export ${fmt} succeeded`);
  }
  assert.deepEqual(exportsDelivered.map((e) => e.format), ["xlsx", "csv", "tsv"]);
});

test("PD-259 CSS 토큰 준수(리터럴 색상 0건) 및 저장소 위생·줄 수 상한 준수", () => {
  const css = fs.readFileSync(BASE_CSS_URL, "utf8");
  const cssWithoutComments = css.replace(/\/\*[\s\S]*?\*\//g, "");
  assert.equal(/#[0-9a-fA-F]{3,8}\b|\brgba?\s*\(|\bhsla?\s*\(/i.test(cssWithoutComments), false, "base.css must use tokens.css vars only");
  assert.equal(/font-size\s*:\s*\d/i.test(cssWithoutComments), false, "base.css must use tokens.css --font-size-* hierarchy tokens only");

  const BASE_SIZE_URL = new URL("../crates/easyoffice-app/ui/base-grid-size.js", import.meta.url);
  const files = [
    ["crates/easyoffice-app/ui/base-model.js", BASE_MODEL_URL, 990],
    ["crates/easyoffice-app/ui/base-grid-size.js", BASE_SIZE_URL, 400],
    ["crates/easyoffice-app/ui/base-studio.js", BASE_STUDIO_URL, 990],
    ["crates/easyoffice-app/ui/base.css", BASE_CSS_URL, 500],
    ["scripts/test-base-engine-pd259.mjs", THIS_TEST_URL, 500],
  ];
  for (const [rel, url, maxLines] of files) {
    const src = fs.readFileSync(fileURLToPath(url), "utf8");
    const lines = src.split("\n").length - (src.endsWith("\n") ? 1 : 0);
    assert.ok(lines <= maxLines, `${rel} line count ${lines} <= ${maxLines}`);
    assert.deepEqual(prohibitedPatternsIn(rel, src), [], `no prohibited terms in ${rel}`);
    assert.deepEqual(ossNameHits(rel, src), [], `no OSS names in ${rel}`);
    if (rel.endsWith(".js") || rel.endsWith(".mjs")) {
      assert.deepEqual(findHtmlSinkViolations(src, rel), [], `no HTML sinks in ${rel}`);
      assert.deepEqual(findBareSorts(src), [], `no bare .sort() in ${rel}`);
      assert.equal(/\.setAttribute\(\s*["']style["']/i.test(src), false, `no inline style attr in ${rel}`);
    }
  }
});
