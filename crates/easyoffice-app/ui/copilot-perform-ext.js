// 직접 썼다(PD-387): 데이터베이스(BASE)와 보드(BOARD) 변경에 포인터가 하는 몸짓이다. 포인터·몸짓과 되돌리기 도구는 `copilot-perform.js`가 `session`으로 건넨다.
// 보드에서 도형을 끌어 그리는 몸짓의 순서는 슬라이드의 개체 놓기(`copilot-perform-slide.js`의 `drawObject`)와 같다: 빈 곳에서 눌러 끄는 만큼 키우고 손을 뗀다.
//
// 연출은 표·보드 DOM도 상태도 바꾸지 않는 임시 표시(포인터와 선택 강조)다. 레코드·필드·노드는 연출이 끝난 뒤 기존 적용 경로(`applyBaseOperationsBatch`,
// `applyBoardOperationsBatch`) 한 번에 들어간다. 멈추면 끝낸 레코드·노드까지만 연산으로 잘라 적용한다.

(function (host) {
  "use strict";

  // 한 응답에서 연출하는 BASE 단계(필드·레코드·수정 행)와 BOARD 노드 수 상한. 넘는 것은 연출 없이 마지막에 한꺼번에 적용한다.
  const MAX_BASE_STEPS = 20;
  const MAX_BOARD_NODES = 12;
  const CLICK_MS = 160;
  const HOLD_MS = 220;
  const DRAW_MS = 420;
  // 머리글 끝에서 필드 추가를 누르는 자리: 마지막 머리글 오른쪽 가장자리에서 이만큼 떨어진 곳.
  const HEADER_END_GAP = 12;
  // 보드 맞춤(`board-studio.js`의 `fitAll`)과 같은 여백과 배율 범위.
  const FIT_MARGIN = 80;
  const MIN_SCALE = 0.25;
  const MAX_SCALE = 4;

  const listOf = (value) => (Array.isArray(value) ? value : []);

  function createExtPerformance(deps) {
    const { document, boardApi } = deps;

    // ---- 목표: 연산에서 연출 단계를 순서대로 뽑는다.

    function collectBase(op, opIndex, kind) {
      const base = { owner: "ext", opIndex, text: "" };
      if (kind === "base.add_field") return [{ ...base, kind: "base-field", entry: 0, name: String(op.name ?? "") }];
      if (kind === "base.add_records") return listOf(op.records).map((_, entry) => ({ ...base, kind: "base-record", entry }));
      if (kind !== "base.update_records") return [];
      return listOf(op.updates).map((item, entry) => ({
        ...base,
        kind: "base-row",
        entry,
        rowIndex: Number(item?.rowIndex),
        fieldName: Object.keys(item?.values || {})[0] ?? "",
      }));
    }

    const nodeId = (node, index) => String(node?.id || "g-" + (index + 1));

    // `placed`: 이 응답에서 앞선 그래프가 이미 놓은 틀들(뒤 그래프는 그 아래에 놓인다).
    function collectBoard(op, opIndex, kind, placed) {
      if (kind !== "board.add_graph" || !boardApi) return [];
      const nodes = listOf(op.nodes);
      const existing = [...boardApi.existing(), ...placed];
      const frames = boardApi.frames(op, existing);
      placed.push(...frames);
      const world = [...existing, ...frames];
      const last = Math.min(nodes.length, MAX_BOARD_NODES) - 1;
      // `drawn`: 이 그래프에서 이미 그린 노드의 화면 사각형들(뒤 노드를 그리는 동안 선택 강조로 남는다). 한 그래프의 노드가 나눠 쓴다.
      const drawn = [];
      return nodes.map((_, entry) => ({ owner: "ext", opIndex, kind: "board-node", entry, frame: frames[entry], world, last: entry === last, drawn, text: "" }));
    }

    /** `state`: 응답 하나에서 연산 사이로 이어지는 값(`{ placed: [] }`). */
    function collect(mod, op, opIndex, kind, state) {
      return mod === "BASE" ? collectBase(op, opIndex, kind) : collectBoard(op, opIndex, kind, state.placed);
    }

    // ---- 자르기: 멈춘 곳까지 끝낸 레코드·노드만 남긴다. 남길 것이 없으면 `null`.

    function cutGraph(op, entry) {
      const nodes = listOf(op.nodes).slice(0, entry);
      if (nodes.length === 0) return null;
      const kept = new Set(nodes.map(nodeId));
      return { ...op, nodes, edges: listOf(op.edges).filter((edge) => kept.has(String(edge?.from)) && kept.has(String(edge?.to))) };
    }

    function cut(op, kind, target) {
      if (kind === "board.add_graph") return cutGraph(op, target.entry);
      const key = kind === "base.add_records" ? "records" : kind === "base.update_records" ? "updates" : null;
      if (!key) return null;
      const kept = listOf(op[key]).slice(0, target.entry);
      return kept.length > 0 ? { ...op, [key]: kept } : null;
    }

    // ---- 연출: 단계마다 포인터가 하는 일.

    function createDrivers(session) {
      const { cursor } = session;
      const headers = () => Array.from(document.querySelectorAll("th.base-grid-th"));

      // 필드 추가: 머리글 줄 끝(마지막 머리글 오른쪽)을 누른다. 머리글을 찾지 못하면 도구줄의 필드 추가 단추를 누른다.
      async function addField() {
        const last = session.visibleRect(headers().at(-1));
        if (last) {
          await session.moveTo(last.right + HEADER_END_GAP, last.top + last.height / 2);
          if (!session.stopped()) await session.click(CLICK_MS);
        } else {
          const button = session.visibleRect(document.querySelector("#btn-base-add-field"));
          if (button) await session.clickAt(button);
        }
        if (!session.stopped()) await session.hold(HOLD_MS);
        return { typed: 0 };
      }

      // 새 레코드: 표 맨 아래 빈 행의 "새 레코드" 단추를 누른다.
      async function addRecord() {
        const button = session.visibleRect(document.querySelector(".base-new-record-row .base-inline-add-btn"));
        if (!button) return { typed: 0 };
        cursor.setSelection([button]);
        await session.clickAt(button);
        if (!session.stopped()) await session.hold(HOLD_MS);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      // 고칠 칸: 필드 이름이 가리키는 열의 칸. 열을 찾지 못하면 행 전체로 본다.
      function cellOf(row, fieldName) {
        const wanted = fieldName.toLowerCase();
        const head = headers().find((th) => String(th.querySelector(".base-th-name")?.textContent ?? "").toLowerCase() === wanted);
        const fieldId = head?.dataset?.fieldId;
        return fieldId ? row.querySelector(`td[data-field-id="${fieldId}"]`) : null;
      }

      async function editRow(target) {
        const row = Array.from(document.querySelectorAll("tr.base-grid-row"))[target.rowIndex];
        const rowRect = session.visibleRect(row);
        if (!rowRect) return { typed: 0 };
        row.scrollIntoView?.({ block: "nearest" });
        cursor.setSelection([rowRect]);
        await session.clickAt(session.visibleRect(cellOf(row, target.fieldName)) || rowRect);
        if (!session.stopped()) await session.hold(HOLD_MS);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      // 노드 틀을 보드 캔버스 안 화면 사각형으로 옮긴다: 새 노드와 있던 노드를 모두 담는 맞춤(`fitAll`)과 같은 배율과 자리.
      function screenFrame(target, canvas) {
        const world = target.world;
        const minX = Math.min(...world.map((f) => f.x));
        const minY = Math.min(...world.map((f) => f.y));
        const width = Math.max(...world.map((f) => f.x + f.w)) - minX;
        const height = Math.max(...world.map((f) => f.y + f.h)) - minY;
        const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, Math.min(canvas.width / (width + FIT_MARGIN), canvas.height / (height + FIT_MARGIN))));
        const originX = canvas.left + (canvas.width - width * scale) / 2 - minX * scale;
        const originY = canvas.top + (canvas.height - height * scale) / 2 - minY * scale;
        const { frame } = target;
        return { left: originX + frame.x * scale, top: originY + frame.y * scale, width: frame.w * scale, height: frame.h * scale };
      }

      // 놓을 자리로 가서 눌러 끌어 도형을 키운다. 그린 노드는 마지막까지 선택 강조로 남는다.
      async function drawNode(target) {
        const canvas = session.visibleRect(document.querySelector("#board-viewport"));
        if (!canvas || !target.frame) return { typed: 0 };
        const box = screenFrame(target, canvas);
        await session.moveTo(box.left, box.top);
        await session.gesture(DRAW_MS, (k) => {
          const width = box.width * k;
          const height = box.height * k;
          cursor.setSelection([...target.drawn, { left: box.left, top: box.top, width, height }]);
          cursor.place(box.left + width, box.top + height);
        });
        if (session.stopped()) return { typed: 0 };
        target.drawn.push(box);
        cursor.setSelection(target.drawn);
        if (target.last) {
          await session.hold(HOLD_MS);
          cursor.setSelection([]);
        }
        return { typed: 0 };
      }

      return { "base-field": addField, "base-record": addRecord, "base-row": editRow, "board-node": drawNode };
    }

    return { collect, cut, createDrivers, MAX_BASE_STEPS, MAX_BOARD_NODES };
  }

  host.__EASYOFFICE_COPILOT_PERFORM_EXT__ = Object.freeze({ createExtPerformance, MAX_BASE_STEPS, MAX_BOARD_NODES });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
