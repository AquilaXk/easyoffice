// Sample documents the WebUI tests use. The app itself starts empty; a test that needs cells or
// slides to work on gets them here: `demoSheetHtml` fills the first sheet of the page (the
// harness `transformHtml` option) and the `openDemo*` helpers open a sample file like a user would.
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { makeFixtureFile, pickFile } from "./fixture-files.mjs";

const DEMO_DOC_BODY = fs.readFileSync(fileURLToPath(new URL("./demo-doc.html", import.meta.url)), "utf8");

/** The page with a sample report (headings, process flow, table, text) in the document editor. */
export function demoDocHtml(html) {
  return html.replace("                  <p><br></p>\n", DEMO_DOC_BODY);
}


// [ref, shown value, formula] of the sample quarterly table.
const SHEET1_CELLS = [["A1", "부문", ""], ["B1", "1분기", ""], ["C1", "2분기", ""], ["D1", "3분기", ""], ["E1", "4분기", ""], ["F1", "연간합계", ""], ["G1", "목표대비", ""], ["H1", "상태", ""], ["A2", "기획조정실", ""], ["B2", "1150", ""], ["C2", "1200", ""], ["D2", "1250", ""], ["E2", "1350", ""], ["F2", "4950", "=SUM(B2:E2)"], ["G2", "1237.5", "=AVERAGE(B2:E2)"], ["H2", "승인", ""], ["A3", "디지털행정혁신과", ""], ["B3", "920", ""], ["C3", "980", ""], ["D3", "1040", ""], ["E3", "1160", ""], ["F3", "4100", "=SUM(B3:E3)"], ["G3", "1025", "=AVERAGE(B3:E3)"], ["H3", "승인", ""], ["A4", "공공데이터전략과", ""], ["B4", "680", ""], ["C4", "720", ""], ["D4", "750", ""], ["E4", "810", ""], ["F4", "2960", "=SUM(B4:E4)"], ["G4", "740", "=AVERAGE(B4:E4)"], ["H4", "정상", ""], ["A5", "클라우드인프라부", ""], ["B5", "840", ""], ["C5", "890", ""], ["D5", "930", ""], ["E5", "980", ""], ["F5", "3640", "=SUM(B5:E5)"], ["G5", "910", "=AVERAGE(B5:E5)"], ["H5", "정상", ""], ["A6", "정보보안감사팀", ""], ["B6", "510", ""], ["C6", "540", ""], ["D6", "570", ""], ["E6", "620", ""], ["F6", "2240", "=SUM(B6:E6)"], ["G6", "560", "=AVERAGE(B6:E6)"], ["H6", "검토", ""], ["A7", "합계", ""], ["B7", "4100", "=SUM(B2:B6)"], ["C7", "4330", "=SUM(C2:C6)"], ["D7", "4540", "=SUM(D2:D6)"], ["E7", "4920", "=SUM(E2:E6)"], ["F7", "17890", "=SUM(F2:F6)"], ["G7", "894.5", "=AVERAGE(G2:G6)"], ["H7", "완료", ""], ["A8", "평균", ""], ["B8", "820", "=AVERAGE(B2:B6)"], ["C8", "866", "=AVERAGE(C2:C6)"], ["D8", "908", "=AVERAGE(D2:D6)"], ["E8", "984", "=AVERAGE(E2:E6)"], ["F8", "3578", "=AVERAGE(F2:F6)"], ["G8", "4950", "=MAX(F2:F6)"], ["H8", "집계", ""]];

export const DEMO_WORKBOOK = {
  sheets: [
    { name: "Sheet1", cells: SHEET1_CELLS.map(([ref, value, formula]) => ({ ref, raw: formula || value })) },
    {
      name: "Sheet2",
      cells: [
        { ref: "A1", raw: "100" },
        { ref: "B1", raw: "250" },
        { ref: "C1", raw: "=SUM(A1:B1)" },
        { ref: "A2", raw: "기획조정실" },
        { ref: "B2", raw: "1200" },
        { ref: "C2", raw: "=B2+A1" },
      ],
    },
  ],
};

export const DEMO_DECK = {
  slides: [
    { title: "연간 제품 전략 발표 자료 (16:9 와이드)", bullets: ["핵심 지표 및 요약 다이어그램 영역"], notes: "핵심 메시지: 주요 문서 포맷 호환성과 수식 계산 결과를 요약하여 전달합니다." },
    { title: "문서 포맷 호환성 및 일괄 변환 흐름", bullets: ["DOCX · HWP 5.0 · HWPX · ODF · XLSX · PPTX · PDF · RTF · CSV · Markdown"], notes: "다양한 문서 포맷 간 서식과 표 구조를 유지하며 상호 변환합니다." },
    { title: "대용량 문서 및 다중 시트 처리 성능 요약", bullets: ["빠른 문서 열기 · 수식 재계산 · 피벗 집계 · PDF 출력"], notes: "대용량 스프레드시트와 프레젠테이션 문서를 지연 없이 편집합니다." },
  ],
};

/** The page with the sample table in sheet 1 (F2 selected, as a filled sheet would show) and a second tab. */
export function demoSheetHtml(html) {
  let out = html.replace('data-cell="A1" class="selected-cell"', 'data-cell="A1"');
  for (const [ref, value, formula] of SHEET1_CELLS) {
    const attrs = formula ? ' data-formula="' + formula + '"' : "";
    out = out.replace(
      '<td contenteditable="true" data-cell="' + ref + '"></td>',
      '<td contenteditable="true" data-cell="' + ref + '"' + attrs + ">" + value + "</td>",
    );
  }
  return out
    .replace(
      '<button id="btn-add-sheet-tab"',
      '<button class="sheet-tab-btn" role="tab" aria-selected="false" data-sheet-tab="Sheet2">시트 2</button>\n              <button id="btn-add-sheet-tab"',
    )
    .replace('id="active-cell-ref">A1<', 'id="active-cell-ref">F2<')
    .replace('data-cell="F2" data-formula="=SUM(B2:E2)"', 'data-cell="F2" class="selected-cell" data-formula="=SUM(B2:E2)"');
}

export const openDemoWorkbook = (document) => pickFile(document, makeFixtureFile("샘플.xlsx", DEMO_WORKBOOK));
export const openDemoDeck = (document) => pickFile(document, makeFixtureFile("샘플.pptx", DEMO_DECK));

/** The page with both samples: the report in the document editor and the table in sheet 1. */
export const demoHtml = (html) => demoSheetHtml(demoDocHtml(html));

/** Puts the sample table into the open editor: the table tools act on `#doc-table`. */
export function addDemoTable(document) {
  const editor = document.getElementById("doc-editor");
  editor.innerHTML +=
    '<div class="doc-table-toolbar" contenteditable="false"><button id="btn-doc-add-row" class="secondary-btn">+행 추가</button>' +
    '<button id="btn-doc-merge-toggle" class="secondary-btn">셀 병합 토글</button><button id="btn-doc-eval-table" class="secondary-btn">=SUM(ABOVE)</button></div>' +
    '<table id="doc-table" class="sheet-table doc-inline-table"><thead><tr><th>소관 부서</th><th>담당자</th><th>집행액(백만원)</th></tr></thead>' +
    '<tbody id="doc-table-body"><tr><td>표준화 총괄과</td><td>김민준 서기관</td><td data-doc-num="520">520</td></tr>' +
    '<tr><td>정보보호 감사팀</td><td>박서연 사무관</td><td data-doc-num="340">340</td></tr>' +
    '<tr id="doc-table-sum-row"><td>합계 (=SUM(ABOVE))</td><td>자동 집계</td><td id="doc-table-sum-cell" data-doc-num="860">860</td></tr></tbody></table>';
}

/**
 * Types the sample sums into sheet 2 (the tab `demoSheetHtml` adds) and returns to sheet 1. The
 * engine must answer each evaluation while this runs.
 */
export async function typeDemoSheet2(document, userClick, flushAsyncTurns) {
  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet2"]'));
  await flushAsyncTurns();
  for (const [ref, text] of [["A1", "100"], ["B1", "250"], ["C1", "=SUM(A1:B1)"]]) {
    const td = document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
    td.focus();
    td.textContent = text;
    td.dispatchEvent({ type: "input", target: td });
    td.dispatchEvent({ type: "keydown", key: "Enter", target: td });
    await flushAsyncTurns(12);
  }
  userClick(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns();
}

/** Puts the sample title ahead of the editor's content. */
export function addDemoHeading(document) {
  const editor = document.getElementById("doc-editor");
  editor.innerHTML = "<h1>행정안전부 전자결재 표준 기안문 (KS X 6101 HWPX)</h1>" + editor.innerHTML;
}
