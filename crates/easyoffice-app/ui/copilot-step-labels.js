(function (host) {
  "use strict";

  // 진행 단계 접힘에 보이는 조회 도구 호출의 한국어 문구(PD-388). 도구 이름마다 문구 함수 하나이고, 모르는 도구는 FALLBACK이다.
  // 문구에는 숫자와 셀 범위만 넣는다. 질의어·시트 이름 같은 문서 내용은 넣지 않는다.
  const FALLBACK = "문서 내용 확인";
  const DEFAULT_DOC_SPAN = 50;
  const CELL_RANGE = /^[A-Za-z]{1,3}\d{1,7}(?::[A-Za-z]{1,3}\d{1,7})?$/;

  const count = (value) => (Number.isInteger(value) && value >= 0 ? value : null);

  // 문단 번호는 사람이 세는 대로 1부터다(인수는 0부터).
  function docBlocks(tc) {
    const start = count(tc.startIndex) ?? 0;
    const end = count(tc.endIndex) ?? start + (count(tc.count) ?? DEFAULT_DOC_SPAN) - 1;
    return end > start ? `문단 ${start + 1}~${end + 1} 읽음` : `문단 ${start + 1} 읽음`;
  }

  function sheetRange(tc) {
    const range = String(tc.range || "").trim();
    return CELL_RANGE.test(range) ? `${range.toUpperCase()} 범위 읽음` : "시트 범위 읽음";
  }

  function baseRecords(tc) {
    const offset = count(tc.offset) ?? 0;
    const limit = count(tc.limit);
    return limit && limit > 1 ? `레코드 ${offset + 1}~${offset + limit} 읽음` : `레코드 ${offset + 1} 읽음`;
  }

  const LABELS = Object.freeze({
    read_doc_blocks: docBlocks,
    read_hwp_paragraphs: docBlocks,
    read_sheet_range: sheetRange,
    find_sheet_cells: () => "시트에서 셀 찾음",
    search_pdf_pages: () => "PDF 쪽 검색",
    read_base_records: baseRecords,
  });

  /** 조회 도구 호출 하나의 사람용 문구. */
  function describeToolCall(tc) {
    const label = Object.hasOwn(LABELS, tc?.name) ? LABELS[tc.name] : null;
    return label ? label(tc) : FALLBACK;
  }

  host.__EASYOFFICE_COPILOT_STEP_LABELS__ = { describeToolCall, FALLBACK };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
