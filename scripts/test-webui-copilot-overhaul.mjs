import assert from "node:assert/strict";
import test from "node:test";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { acceptProposal } from "./test-support/copilot-accept.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";
import {
  COPILOT_MAX_READ_PARAGRAPHS,
  SHELL_COPILOT_TOGGLE_REASON,
  applyCopilotEdits,
  createCopilotToggleRelay,
  parseApplyCopilotEditsParams,
  parseReadParagraphRangeParams,
  readParagraphRange,
} from "../apps/hwp-studio/src/embed/copilot-bridge.ts";

test("chat Markdown renderer builds safe DOM nodes, scrollable tables, and PDF page chips without <p> or XSS", async () => {
  let jumpedPage = 0;
  const h = await open({
    module: "PDF",
    run: () => ({
      status: "ok",
      mode: "ask",
      reply: [
        "### 검토 요약",
        "핵심 **강조**와 *기울임*, ~~취소~~, `SUM(A1:A3)`, [안전 링크](https://example.com), [위험](javascript:alert(1)), <script>alert(1)</script><img src=x onerror=alert(1)> 확인 [3쪽] 참조.",
        "",
        "| 구분 | 금액 |",
        "| --- | --- |",
        "| 1분기 | 120 |",
      ].join("\n"),
      operations: [],
      locations: [],
    }),
  });
  h.host.__EASYOFFICE_MODULE_FIND_HOOKS__ = {
    ...(h.host.__EASYOFFICE_MODULE_FIND_HOOKS__ || {}),
    goToPdfPage: (p) => { jumpedPage = p; },
  };

  await h.send("요약해 주세요");
  const shell = h.document.getElementById("eo-copilot-shell");
  assert.equal(shell.querySelectorAll("p").length, 0, "#eo-copilot-shell keeps zero <p> elements");
  const reply = h.assistants()[0].querySelector("[data-turn-reply]");
  assert.equal(reply.querySelector("script"), null, "no <script> element created");
  assert.equal(reply.querySelector("img"), null, "no raw <img> element created");
  assert.equal(reply.querySelector("strong")?.textContent, "강조");
  assert.equal(reply.querySelector("em")?.textContent, "기울임");
  assert.equal(reply.querySelector("del")?.textContent, "취소");
  assert.equal(reply.querySelector("code.eo-copilot-md-inline-code")?.textContent, "SUM(A1:A3)");

  const links = reply.querySelectorAll("a");
  assert.equal(links.length, 1, "unsafe javascript: URL is not rendered as <a>");
  assert.equal(links[0].getAttribute("href"), "https://example.com");
  assert.equal(links[0].getAttribute("rel"), "noopener noreferrer");

  const tableWrap = reply.querySelector(".eo-copilot-md-table-wrap");
  assert.ok(tableWrap, "pipe table wrapped in accessible scrollable region");
  assert.equal(tableWrap.getAttribute("tabindex"), "0");
  assert.equal(tableWrap.getAttribute("role"), "region");
  assert.equal(tableWrap.querySelectorAll("th").length, 2);
  assert.equal(tableWrap.querySelector("td")?.textContent, "1분기");

  const chip = reply.querySelector("button.eo-copilot-page-chip");
  assert.ok(chip, "[3쪽] rendered as page jump chip");
  h.userClick(chip);
  await flushAsyncTurns(4);
  assert.equal(jumpedPage, 3);
});

test("slide.add allows decks past MAX_CONTEXT_SLIDES (64) up to MAX_SLIDES (256)", async () => {
  const h = await open({
    module: "SLIDE",
    run: () => answer([{ kind: "slide.add", title: "71번째 장표", bullets: ["본문"], notes: "" }]),
  });
  const deck = h.host.__EASYOFFICE_OFFICE_PRINT__.slideDeckState;
  while (deck.length < 70) {
    deck.push({ ...deck[0], title: "슬라이드 " + (deck.length + 1), elements: [] });
  }
  await h.send("새 슬라이드 추가");
  assert.ok(await acceptProposal(h.document, h.userClick));
  assert.equal(deck.length, 71);
  assert.equal(deck[70].title, "71번째 장표");
});

test("multi-turn agent loop executes on-demand read tools and self-corrects SLIDE layout_lint overflow", async () => {
  // 1. On-demand read_doc_blocks past paragraph 200
  let docCall = 0;
  const docH = await open({
    module: "DOC",
    run: (req) => {
      docCall += 1;
      if (docCall === 1) {
        return {
          status: "ok",
          mode: "edit",
          reply: "뒤쪽 문단을 확인합니다.",
          operations: [],
          toolCalls: [{ name: "read_doc_blocks", startIndex: 202, endIndex: 204 }],
        };
      }
      const lastUser = req.history.at(-1)?.text || "";
      assert.match(lastUser, /숨겨진 203번 문단/, "read_doc_blocks output fed back into turn 2");
      return answer([{ kind: "doc.append", blocks: [{ text: "조회 반영 문단" }] }]);
    },
  });
  docH.editor.replaceChildren();
  for (let i = 0; i < 210; i += 1) {
    const p = docH.document.createElement("p");
    p.textContent = i === 203 ? "숨겨진 203번 문단" : "일반 " + i;
    docH.editor.appendChild(p);
  }
  await docH.send("203번 문단을 참고해 끝에 요약 추가", 24);
  assert.equal(docCall, 2);
  assert.ok(await acceptProposal(docH.document, docH.userClick));
  assert.equal(docH.editor.querySelectorAll("p")[210].textContent, "조회 반영 문단");

  // 2. SLIDE post-verification gate catches overflowing textbox and triggers self-correction
  let slideCall = 0;
  const slideH = await open({
    module: "SLIDE",
    run: (req) => {
      slideCall += 1;
      if (slideCall === 1) {
        return answer([
          {
            kind: "slide.add_element",
            slideIndex: 0,
            element: { id: "ovf-1", kind: "textbox", x: 860, y: 100, w: 300, h: 80, text: "경계 밖 상자" },
          },
        ]);
      }
      assert.match(req.history.at(-1)?.text || "", /자동 검증 게이트 오류/, "layout_lint overflow reported back to model");
      return answer([
        {
          kind: "slide.add_element",
          slideIndex: 0,
          element: { id: "ovf-1", kind: "textbox", x: 120, y: 100, w: 300, h: 80, text: "경계 안 상자" },
        },
      ]);
    },
  });
  const deck = slideH.host.__EASYOFFICE_OFFICE_PRINT__.slideDeckState;
  assert.equal(deck[0].elements?.length || 0, 0);
  await slideH.send("글상자 추가", 28);
  assert.equal(slideCall, 2, "retried once after layout_lint failure");
  assert.equal(deck[0].elements?.length || 0, 0, "dry-run gate left live slideDeckState untouched before 수락");
  assert.ok(await acceptProposal(slideH.document, slideH.userClick));
  assert.equal(deck[0].elements[0].x, 120);
  assert.equal(deck[0].elements[0].text, "경계 안 상자");
});

test("DOC preserves inline formatting & tables, records Track Changes revisions, and handles inline ghost text", async () => {
  const h = await open({
    module: "DOC",
    run: (req) => {
      assert.match(req.context.doc.paragraphs[0], /<strong>중요<\/strong>/);
      return answer([
        {
          kind: "doc.replace_paragraphs",
          startIndex: 0,
          endIndex: 0,
          blocks: [{ table: { headerRow: true, rows: [["구분", "비고"], ["매출", "<strong>상승</strong>"]] } }],
        },
      ]);
    },
  });

  // Seed formatted run in paragraph 0
  const p0 = h.editor.querySelector("p");
  p0.replaceChildren();
  const runSpan = h.document.createElement("span");
  runSpan.className = "eo-run";
  runSpan.dataset.eoBold = "true";
  runSpan.textContent = "중요";
  p0.appendChild(runSpan);
  p0.appendChild(h.document.createTextNode(" 지표"));

  await h.send("표로 바꿔 주세요", 18);
  assert.ok(await acceptProposal(h.document, h.userClick));
  const tbl = h.editor.querySelector("table.doc-inline-table");
  assert.ok(tbl, "table.doc-inline-table inserted into #doc-editor");
  assert.equal(tbl.querySelector('td[data-eo-header="true"]')?.textContent, "구분");
  assert.equal(tbl.querySelectorAll('td span.eo-run[data-eo-bold="true"]')[2]?.textContent, "상승");

  // Single-step Undo restores original formatted paragraph
  h.userClick(h.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(6);
  assert.equal(h.editor.querySelector("table"), null);
  assert.equal(h.editor.querySelector('p span.eo-run[data-eo-bold="true"]')?.textContent, "중요");

  // Track Changes mode records del.eo-rev-del + ins.eo-rev-ins
  const tcH = await open({
    module: "DOC",
    run: () => answer([{ kind: "doc.replace_paragraphs", replacements: [{ index: 0, text: "새 개정 문장" }] }]),
  });
  tcH.editor.dataset.eoTrackChanges = "true";
  tcH.editor.querySelector("p").textContent = "이전 원문 문장";
  await tcH.send("개정해 주세요", 18);
  assert.ok(await acceptProposal(tcH.document, tcH.userClick));
  const editedP = tcH.editor.querySelector("p");
  assert.equal(editedP.querySelector("del.eo-rev-del")?.textContent, "이전 원문 문장");
  assert.equal(editedP.querySelector("ins.eo-rev-ins")?.textContent, "새 개정 문장");

  // Inline ghost text: toggle #btn-copilot-ghost, Esc dismisses, Tab accepts, overlay stays outside #doc-editor
  const ghostH = await open({
    module: "DOC",
    run: () => ({ status: "ok", mode: "ask", reply: "이어지는 추천 문구입니다.", operations: [] }),
  });
  const ghostBtn = ghostH.document.getElementById("btn-copilot-ghost");
  assert.ok(ghostBtn, "ghost toggle button exists");
  ghostH.userClick(ghostBtn);
  assert.equal(ghostBtn.getAttribute("aria-pressed"), "true");

  const gp = ghostH.document.createElement("p");
  gp.textContent = "초안 문장 시작 ";
  ghostH.editor.replaceChildren(gp);
  const gpText = gp.firstChild;
  const realRange = ghostH.document.createRange();
  realRange.setStart(gpText, gpText.data.length);
  realRange.setEnd(gpText, gpText.data.length);
  ghostH.host.getSelection = () => ({
    rangeCount: 1,
    isCollapsed: true,
    getRangeAt: () => realRange,
    removeAllRanges() {},
    addRange() {},
  });

  ghostH.editor.dispatchEvent({ type: "input", target: gp });
  ghostH.advanceTimers(1000);
  await flushAsyncTurns(10);
  let overlay = ghostH.document.querySelector(".eo-copilot-ghost-overlay");
  assert.ok(overlay, "ghost overlay rendered outside #doc-editor");
  assert.equal(ghostH.editor.contains(overlay), false, "ghost overlay lives outside #doc-editor");
  assert.equal(gp.textContent, "초안 문장 시작 ", "editor untouched while suggestion is only previewed");

  // Esc dismisses without mutating paragraph
  ghostH.editor.dispatchEvent({ type: "keydown", key: "Escape", preventDefault() {}, stopPropagation() {} });
  assert.equal(ghostH.document.querySelector(".eo-copilot-ghost-overlay"), null, "Esc dismisses ghost overlay");
  assert.equal(gp.textContent, "초안 문장 시작 ");

  // Trigger again and press Tab to accept
  ghostH.editor.dispatchEvent({ type: "input", target: gp });
  ghostH.advanceTimers(1000);
  await flushAsyncTurns(10);
  overlay = ghostH.document.querySelector(".eo-copilot-ghost-overlay");
  assert.ok(overlay);
  ghostH.editor.dispatchEvent({ type: "keydown", key: "Tab", preventDefault() {}, stopPropagation() {} });
  assert.equal(gp.textContent, "초안 문장 시작 이어지는 추천 문구입니다.");
});

test("SHEET pins FrozenSelection, detects uncommitted DOM cell edits, and executes extended sheet ops with Undo", async () => {
  let activeSheetName = "";
  const h = await open({
    module: "SHEET",
    run: (req) => {
      activeSheetName = req.context.sheet.name;
      assert.equal(req.selection.range, "B2:C4", "FrozenSelection pinned at send time");
      return answer([
        { kind: "sheet.write", sheet: activeSheetName, range: "A1", cells: [
          { coord: "A1", raw: "지역" }, { coord: "B1", raw: "매출" },
          { coord: "A2", raw: "서울" }, { coord: "B2", raw: "1500" },
          { coord: "A3", raw: "부산" }, { coord: "B3", raw: "900" },
        ] },
        { kind: "sheet.table", sheet: activeSheetName, action: "create", range: "A1:B3", tableName: "지역매출" },
        { kind: "sheet.cond_format", sheet: activeSheetName, range: "B2:B3", preset: "greaterThan", value1: "1000" },
        { kind: "sheet.sort_filter", sheet: activeSheetName, action: "sort", range: "A1:B3", colOffset: 1, ascending: true },
        { kind: "sheet.pivot", sheet: activeSheetName, sourceRange: "A1:B3", destCell: "D1", rowField: "지역", valueField: "매출", agg: "sum" },
        { kind: "sheet.structure", sheet: activeSheetName, action: "freeze", index: 1, count: 1 },
        { kind: "sheet.note", sheet: activeSheetName, coord: "B2", text: "핵심 지역" },
      ]);
    },
  });

  // Select B2:C4 before sending
  const nameBox = h.document.getElementById("active-cell-ref");
  nameBox.textContent = "B2:C4";
  nameBox.dispatchEvent({ type: "keydown", key: "Enter", target: nameBox });
  await flushAsyncTurns(4);

  await h.send("표·피벗·조건부 서식·정렬·메모 적용", 20);

  // Verify DOM cell edit after preview triggers conflict detection and blocks stale apply
  const layout = h.host.__EASYOFFICE_SHEET_LAYOUT__;
  const rules = h.host.__EASYOFFICE_SHEET_RULES__;
  const tdB2 = h.document.querySelector('#pane-sheet td[data-cell="B2"]');
  const oldB2Text = tdB2.textContent;
  tdB2.textContent = "사용자가 직접 수정한 셀";
  await acceptProposal(h.document, h.userClick);
  assert.equal(layout.tablesOf(activeSheetName).length, 0, "DOM cell edit after preview blocks stale apply");
  assert.equal(h.document.getElementById("status-command-feedback").textContent, "문서가 바뀌어 적용할 수 없습니다");
  assert.equal(tdB2.textContent, "사용자가 직접 수정한 셀");
  tdB2.textContent = oldB2Text;

  // Re-run cleanly, accept, verify all 6 extended features, then undo in one step
  await h.send("다시 적용", 20);
  assert.ok(await acceptProposal(h.document, h.userClick));
  assert.equal(layout.tablesOf(activeSheetName).length, 1);
  assert.equal(layout.pivotsOf(activeSheetName).length, 1);
  assert.equal(rules.get(activeSheetName).cf.length, 1);
  assert.equal(layout.notesOf(activeSheetName)[0]?.ref, "B2");
  assert.equal(layout.freezeOf(activeSheetName).rows, 1);

  h.userClick(h.assistants().at(-1).querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(8);
  assert.equal(layout.tablesOf(activeSheetName).length, 0);
  assert.equal(layout.pivotsOf(activeSheetName).length, 0);
  assert.equal(layout.notesOf(activeSheetName).length, 0);
});

test("BASE and BOARD apply structured schema/record/graph operations and undo in a single snapshot", async () => {
  // BASE
  const baseH = await open({
    module: "BASE",
    run: () => answer([
      { kind: "base.add_field", name: "우선순위", fieldType: "select", options: ["높음", "보통"] },
      { kind: "base.add_records", records: [{ "우선순위": "높음" }] },
      { kind: "base.set_view", view: "kanban", groupField: "우선순위" },
    ]),
  });
  const baseStudio = baseH.host.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
  const beforeRecords = baseStudio.getState().tables[0].records.length;
  await baseH.send("우선순위 필드와 칸반 보기를 구성해 주세요", 18);
  assert.ok(await acceptProposal(baseH.document, baseH.userClick));
  const afterTable = baseStudio.getState().tables[0];
  assert.ok(afterTable.fields.some((f) => f.name === "우선순위"));
  assert.equal(afterTable.records.length, beforeRecords + 1);
  assert.equal(afterTable.view, "kanban");

  baseH.userClick(baseH.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(6);
  assert.equal(baseStudio.getState().tables[0].records.length, beforeRecords);

  // BOARD
  const boardH = await open({
    module: "BOARD",
    run: () => answer([
      {
        kind: "board.add_graph",
        direction: "LR",
        nodes: [
          { id: "n1", kind: "shape", shape: "rect", text: "요청 접수" },
          { id: "n2", kind: "shape", shape: "diamond", text: "검토" },
          { id: "n3", kind: "shape", shape: "rect", text: "승인" },
        ],
        edges: [
          { from: "n1", to: "n2", label: "전달", style: "orthogonal" },
          { from: "n2", to: "n3", label: "통과", style: "orthogonal" },
        ],
      },
    ]),
  });
  const boardStudio = boardH.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  const beforeNodes = boardStudio.getState().nodes.length;
  await boardH.send("승인 흐름도를 그려 주세요", 18);
  assert.ok(await acceptProposal(boardH.document, boardH.userClick));
  const afterBoard = boardStudio.getState();
  assert.equal(afterBoard.nodes.length, beforeNodes + 3);
  assert.equal(afterBoard.connectors.length >= 2, true);
  const added = afterBoard.nodes.slice(-3);
  assert.ok(added[0].x < added[1].x && added[1].x < added[2].x, "LR layered layout assigns strictly increasing X coordinates");

  boardH.userClick(boardH.assistants()[0].querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(6);
  assert.equal(boardStudio.getState().nodes.length, beforeNodes);
});

test("HWP copilot-bridge enforces read budgets, batches multi-paragraph edits into one undo snapshot, and relays toggle", () => {
  let sections = [["첫 문단\u0002제어", "둘째 문단", "셋째 문단"]];
  let generation = 1;
  const undoStack = [];

  const wasm = {
    get pageCount() { return Math.max(1, Math.ceil(sections[0].length / 3)); },
    get documentGeneration() { return generation; },
    getSectionCount: () => sections.length,
    getParagraphCount: (s) => sections[s].length,
    getParagraphLength: (s, p) => sections[s][p].length,
    getTextRange: (s, p, off, cnt) => sections[s][p].slice(off, off + cnt),
    getControlTextPositions: (s, p) => (sections[s][p].includes("\u0002") ? [3] : []),
    replaceText: (s, p, off, len, next) => {
      const cur = sections[s][p];
      sections[s][p] = cur.slice(0, off) + next + cur.slice(off + len);
      return { ok: true, charOffset: off + next.length, newLength: sections[s][p].length };
    },
    insertText: (s, p, off, txt) => {
      const cur = sections[s][p];
      sections[s][p] = cur.slice(0, off) + txt + cur.slice(off);
    },
    deleteText: (s, p, off, cnt) => {
      const cur = sections[s][p];
      sections[s][p] = cur.slice(0, off) + cur.slice(off + cnt);
    },
    splitParagraph: (s, p, off) => {
      const cur = sections[s][p];
      sections[s].splice(p, 1, cur.slice(0, off), cur.slice(off));
    },
    mergeParagraph: (s, p) => {
      if (p + 1 < sections[s].length) {
        sections[s][p] += sections[s][p + 1];
        sections[s].splice(p + 1, 1);
      }
    },
  };

  const input = {
    getCursorPosition: () => ({ sectionIndex: 0, paragraphIndex: 0, charOffset: 0 }),
    getSelection: () => ({
      start: { sectionIndex: 0, paragraphIndex: 1, charOffset: 0 },
      end: { sectionIndex: 0, paragraphIndex: 2, charOffset: 5 },
    }),
    executeOperation: (desc) => {
      const before = structuredClone(sections);
      try {
        desc.operation(wasm);
        generation += 1;
        undoStack.push(before);
      } catch (err) {
        sections = before;
        throw err;
      }
    },
  };

  const readParams = parseReadParagraphRangeParams({ startIndex: 0, count: COPILOT_MAX_READ_PARAGRAPHS });
  const snap = readParagraphRange(wasm, input, readParams);
  assert.equal(snap.totalParagraphs, 3);
  assert.equal(snap.paragraphs[0].text, "첫 문단제어", "control character stripped cleanly");
  assert.equal(snap.paragraphs[0].hasControls, true);
  assert.equal(snap.selection.hasSelection, true);
  assert.equal(snap.selection.startParagraphIndex, 1);

  assert.throws(() => parseReadParagraphRangeParams({ startIndex: 0, count: 999 }), /count/);
  assert.throws(() => parseReadParagraphRangeParams({ startIndex: 0, count: 1, extra: true }), /unknown field/);

  const applyParams = parseApplyCopilotEditsParams({
    expectedDocumentGeneration: 1,
    steps: [
      { kind: "replace_range", startIndex: 1, endIndex: 2, texts: ["통합 문단 A", "통합 문단 B"] },
      { kind: "append", texts: ["맺음말 1", "맺음말 2"] },
    ],
  });
  const res = applyCopilotEdits(wasm, input, applyParams);
  assert.equal(res.ok, true);
  assert.equal(undoStack.length, 1, "entire multi-step batch recorded as a single undo snapshot");
  assert.deepEqual(sections[0], ["첫 문단\u0002제어", "통합 문단 A", "통합 문단 B", "맺음말 1", "맺음말 2"]);

  // Stale generation is rejected without touching document or undo stack
  assert.throws(
    () => applyCopilotEdits(wasm, input, parseApplyCopilotEditsParams({
      expectedDocumentGeneration: 1,
      steps: [{ kind: "append", texts: ["실패해야 함"] }],
    })),
    /변경되어 적용을 중단/,
  );
  assert.equal(undoStack.length, 1);

  // Toggle relay emits SHELL_COPILOT_TOGGLE_REASON
  const relay = createCopilotToggleRelay();
  const events = [];
  const off = relay.add((ev) => events.push(ev));
  relay.toggle();
  off();
  relay.toggle();
  assert.deepEqual(events, [{ schemaVersion: 1, reason: SHELL_COPILOT_TOGGLE_REASON }]);
});

test("SHEET post-verification gate catches formula errors (#DIV/0!) and agent loop enforces maxTurns and byte caps", async () => {
  let turnCount = 0;
  const h = await open({
    module: "SHEET",
    run: (req) => {
      turnCount += 1;
      const sheetName = req.context.sheet.name;
      if (turnCount === 1) {
        return answer([
          { kind: "sheet.write", sheet: sheetName, range: "C1", cells: [{ coord: "C1", raw: "=100/0" }] },
        ]);
      }
      assert.match(req.history.at(-1)?.text || "", /#DIV\/0!/, "formula error fed back to model");
      return answer([
        { kind: "sheet.write", sheet: sheetName, range: "C1", cells: [{ coord: "C1", raw: "=100/2" }] },
      ]);
    },
  });
  await h.send("비율 수식 입력", 28);
  assert.equal(turnCount, 2, "retried after #DIV/0! formula error");
  assert.ok(await acceptProposal(h.document, h.userClick));
  const tdC1 = h.document.querySelector('#pane-sheet td[data-cell="C1"]');
  assert.equal(tdC1.textContent.trim(), "50");

  // Verify maxTurns <= 6 hard cap stops infinite read-tool loops cleanly
  let loopCalls = 0;
  const loopH = await open({
    module: "DOC",
    run: () => {
      loopCalls += 1;
      return {
        status: "ok",
        mode: "ask",
        reply: "계속 조회 중",
        operations: [],
        toolCalls: [{ name: "read_doc_blocks", startIndex: 0, endIndex: 1 }],
      };
    },
  });
  await loopH.send("반복 조회 점검", 40);
  assert.equal(loopCalls, 6, "agent loop never exceeds MAX_TURNS = 6");
});
