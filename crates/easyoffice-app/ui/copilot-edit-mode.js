// 코파일럿 편집 방식(PD-385). 기존 내용을 고치는 응답은 미리보기 뒤 바꾸기·아래에 삽입·버리기로, 새 내용만 더하는 응답은 문서에 초안으로 바로 넣고 유지·버리기로 다룬다.
// 이 파일은 그 판정(`classifyEditMode`), 단추 목록, 아래에 삽입으로의 연산 변환과 실행을 둔다. 직접 썼다: 공식 오피스 코파일럿의 화면 동작을 따르는 정책이라 가져올 구현이 없다.

(function (host) {
  "use strict";

  const REVIEW = "review";
  const DRAFT = "draft";
  const opKind = (op) => String(op?.op || op?.type || op?.kind || "").trim();

  // 원본을 건드리지 않고 새 것만 더하는 연산. 시트 쓰기는 빈 셀에만 쓸 때(아래 `isAdditive`)만 여기에 든다.
  const ADDITIVE_KINDS = Object.freeze({
    DOC: new Set(["doc.append"]),
    HWP: new Set(["doc.append"]),
    SLIDE: new Set(["slide.add", "slide.add_element"]),
    BASE: new Set(["base.add_field", "base.add_records"]),
    BOARD: new Set(["board.add_graph"]),
  });
  // 문단·범위의 글을 바꾸는 연산: 아래에 삽입은 이 연산에서만 보인다.
  const TEXT_REPLACE_KINDS = new Set(["doc.replace_span", "doc.replace_paragraphs"]);
  const INSERT_BELOW_KIND = "doc.insert_below";

  function sheetWriteIsAdditive(op, probe) {
    const cells = Array.isArray(op.cells) ? op.cells : [];
    if (cells.length === 0 || typeof probe?.isCellFilled !== "function") return false;
    return cells.every((cell) => !probe.isCellFilled(op, String(cell?.coord || "")));
  }

  function isAdditive(mod, op, probe) {
    const kind = opKind(op);
    if (mod === "SHEET") return kind === "sheet.write" && sheetWriteIsAdditive(op, probe);
    return ADDITIVE_KINDS[mod]?.has(kind) === true;
  }

  /**
   * `"draft"`: 모든 연산이 새 내용만 더한다(문서에 초안으로 바로 넣는다). `"review"`: 하나라도 기존 내용을 고치면(섞인 응답 포함) 미리보기를 거친다.
   * `probe.isCellFilled(op, coord)`는 시트 쓰기 대상 셀이 차 있는지 알려 준다. 없으면 시트 쓰기는 고치는 연산으로 본다.
   */
  function classifyEditMode(mod, operations, probe) {
    const ops = Array.isArray(operations) ? operations : [];
    if (ops.length === 0) return REVIEW;
    return ops.every((op) => op && typeof op === "object" && isAdditive(mod, op, probe)) ? DRAFT : REVIEW;
  }

  /** 아래에 삽입은 DOC·HWP의 문단·범위 글 교체에만 있다. 글이 아닌 연산(개인정보 가리기 등)이 섞이면 없다. */
  function canInsertBelow(mod, operations) {
    if (mod !== "DOC" && mod !== "HWP") return false;
    const kinds = (Array.isArray(operations) ? operations : []).map(opKind);
    return kinds.some((kind) => TEXT_REPLACE_KINDS.has(kind)) && kinds.every((kind) => TEXT_REPLACE_KINDS.has(kind) || kind === "doc.append");
  }

  const REVIEW_ACTIONS = new Set(["accept", "insert-below", "reject", "keep", "discard"]);

  /** 응답 아래에 놓을 단추: `[action, 라벨]`. 대기 중인 미리보기는 바꾸기·(아래에 삽입)·버리기, 초안은 유지·버리기. */
  function reviewActions(change) {
    if (change?.state === "pending") {
      return [["accept", "바꾸기"], ...(change.insertBelow ? [["insert-below", "아래에 삽입"]] : []), ["reject", "버리기"]];
    }
    return change?.state === "draft" ? [["keep", "유지"], ["discard", "버리기"]] : [];
  }

  /** 다음 요청이나 문서 전환으로 결정이 없어진 변경: 대기 중인 미리보기는 버리고 초안은 유지한다. */
  function settle(change) {
    if (change?.state === "pending") change.state = "rejected";
    else if (change?.state === "draft") change.state = "kept";
  }

  const isDiscarded = (change) => change?.state === "rejected" || change?.state === "discarded";
  const DISCARDED_LABEL = "버림";

  /**
   * 고치는 연산을 "원문은 두고 새 글을 원 문단(범위) 바로 아래 새 문단으로" 넣는 연산 하나(`doc.insert_below`)로 바꾼다.
   * `reader.count`는 문단 수, `reader.text(index)`는 지금 문단의 글(모르면 null). 문단 번호는 모두 바꾸기 전 문서의 번호다.
   * 문단 끝 너머를 가리키는 교체나 `doc.append`는 문서 끝에 더하는 연산으로 남긴다.
   */
  function toInsertBelowOperations(operations, reader) {
    const inserts = [];
    const rest = [];
    const lastIndex = reader.count - 1;
    for (const op of operations) {
      const kind = opKind(op);
      if (kind === "doc.replace_span") {
        const index = Number(op?.index);
        if (!Number.isInteger(index) || index < 0 || index > lastIndex) throw new Error("아래에 삽입할 문단 번호가 범위를 벗어났습니다.");
        const current = reader.text(index);
        if (current === null) throw new Error("아래에 삽입할 문단의 글을 읽지 못했습니다.");
        const search = String(op?.search ?? "");
        const at = search ? current.indexOf(search) : -1;
        if (at < 0 || current.indexOf(search, at + 1) >= 0) throw new Error("변경할 대상 구간을 문단에서 유일하게 찾을 수 없습니다.");
        inserts.push({ after: index, plain: true, blocks: [current.slice(0, at) + String(op?.replace ?? "") + current.slice(at + search.length)] });
      } else if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
        for (const rep of op.replacements) {
          const index = Number(rep?.index);
          if (!Number.isInteger(index) || index < 0) continue;
          if (index > lastIndex) rest.push({ op: "doc.append", blocks: [String(rep?.text ?? "")] });
          else inserts.push({ after: index, blocks: [String(rep?.text ?? "")] });
        }
      } else if (kind === "doc.replace_paragraphs") {
        const blocks = Array.isArray(op.blocks) ? op.blocks : Array.isArray(op.paragraphs) ? op.paragraphs : op.text !== undefined ? [String(op.text)] : [];
        const after = Number.isInteger(op.endIndex) ? Math.min(lastIndex, Math.max(0, op.endIndex)) : lastIndex;
        if (after < 0) rest.push({ op: "doc.append", blocks });
        else inserts.push({ after, blocks });
      } else {
        rest.push(op);
      }
    }
    return inserts.length > 0 ? [{ op: INSERT_BELOW_KIND, inserts }, ...rest] : rest;
  }

  /**
   * `doc.insert_below`를 편집기에 넣는다. `existing`은 바꾸기 전 블록 목록이라 앞에서 넣은 문단이 뒤 번호를 밀지 않는다.
   * 같은 문단 아래에 여러 번 넣으면 넣은 순서대로 쌓는다. 변경 추적이 켜져 있으면 넣은 문단을 삽입으로 표시한다.
   */
  function applyInserts({ document, docHtml, editor, existing, inserts, tracking, buildBlock, locations }) {
    const tails = new Map();
    const revisionId = tracking ? docHtml.nextRevisionId(editor) : 0;
    const dateIso = tracking ? new Date().toISOString() : "";
    for (const entry of inserts) {
      const anchor = existing[entry.after];
      if (!anchor) throw new Error("아래에 삽입할 문단을 찾을 수 없습니다.");
      entry.blocks.forEach((item, offset) => {
        let block = null;
        if (entry.plain) {
          block = document.createElement("p");
          docHtml.appendPlainText(document, block, item);
        } else {
          block = buildBlock(item);
        }
        if (!block) return;
        if (tracking) docHtml.wrapBlockInserted(document, block, revisionId, dateIso);
        const previous = tails.get(anchor) || anchor;
        previous.parentNode.insertBefore(block, previous.nextSibling);
        tails.set(anchor, block);
        const index = entry.after + 1 + offset;
        locations.push({ module: "DOC", label: "문단 " + (index + 1), paragraphIndex: index });
      });
    }
  }

  host.__EASYOFFICE_COPILOT_EDIT_MODE__ = Object.freeze({
    REVIEW,
    DRAFT,
    INSERT_BELOW_KIND,
    REVIEW_ACTIONS,
    DISCARDED_LABEL,
    classifyEditMode,
    canInsertBelow,
    reviewActions,
    settle,
    isDiscarded,
    toInsertBelowOperations,
    applyInserts,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
