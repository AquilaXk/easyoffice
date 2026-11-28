// 직접 썼다(PD-387): 시트의 서식·차트·행/열 작업과 그 밖의 범위 작업(표, 조건부 서식, 정렬·필터, 피벗, 메모)에 포인터가 하는 몸짓이다.
// 포인터·몸짓(`moveTo`, `click`, `hold`, `gesture`)과 되돌리기 도구는 `copilot-perform.js`가 `session`으로 건넨다. 셀 쓰기(`sheet.write`)의 한 자씩 치기는
// `copilot-perform.js`에 있고, 여기는 글을 치지 않는 작업만 맡는다: 범위를 끌어 선택하고 → 리본·메뉴 단추로 가서 누르고 → 적용한다.
//
// 연출은 눈에 보이는 임시 표시일 뿐이다(선택 강조와 포인터). 시트 DOM도 값도 바꾸지 않으므로 되돌릴 것이 없고, 쓰는 것은 연출이 끝난 뒤 기존 적용 경로 한 번이다.
// 멈추면 보이던 단계의 작업은 쓰지 않는다(절반만 적용된 서식이나 차트가 없도록).

(function (host) {
  "use strict";

  // 한 응답에서 연출하는 시트 작업(서식·차트·행/열·범위) 수 상한. 넘는 작업은 연출 없이 마지막에 한꺼번에 적용한다.
  const MAX_SHEET_STEPS = 24;
  const CLICK_MS = 160;
  const HOLD_MS = 220;
  const DRAG_MS = 380;
  const DRAW_MS = 420;
  // 행·열 머리글을 찾아 보는 수 상한(한 작업이 많은 행을 지워도 머리글 조회는 이만큼에서 멈춘다).
  const MAX_HEADERS = 8;
  // 차트 기본 크기(`createChartModel`과 같다)와 데이터 범위 오른쪽 간격.
  const CHART_W = 420;
  const CHART_H = 260;
  const CHART_GAP = 16;
  const MIN_CHART_W = 120;

  // 서식 필드 → 누르는 단추. 숫자 서식은 백분율이면 백분율 단추, 그 밖은 통화 단추로 본다.
  const FORMAT_BUTTONS = Object.freeze({
    bold: "#btn-stylebar-bold",
    fill: "#btn-sheet-fill",
    color: "#btn-sheet-font-color",
    percent: '[data-sheet-numfmt="percent"]',
    currency: '[data-sheet-numfmt="currency"]',
  });
  const MENU_TITLE = (name) => `[data-menu="${name}"] > .menu-title`;
  // 범위 작업 → 메뉴. 차트와 메모는 삽입 메뉴, 조건부 서식은 서식 메뉴, 나머지는 데이터 메뉴다.
  const RANGE_MENUS = Object.freeze({
    "sheet.table": "data",
    "sheet.pivot": "data",
    "sheet.sort_filter": "data",
    "sheet.cond_format": "format",
    "sheet.note": "insert-object",
  });
  const CHART_MENU = "insert-object";
  const STRUCTURE_ACTIONS = Object.freeze({
    insert_rows: { axis: "row", insert: true },
    delete_rows: { axis: "row", insert: false },
    insert_cols: { axis: "col", insert: true },
    delete_cols: { axis: "col", insert: false },
  });

  const union = (rects) => {
    const left = Math.min(...rects.map((r) => r.left));
    const top = Math.min(...rects.map((r) => r.top));
    const right = Math.max(...rects.map((r) => r.right));
    const bottom = Math.max(...rects.map((r) => r.bottom));
    return { left, top, width: right - left, height: bottom - top, right, bottom };
  };

  function createSheetPerformance(deps) {
    const { document, getActiveSheetName } = deps;
    const refs = () => host.__EASYOFFICE_SHEET_REFS__;

    // ---- 목표: 연산에서 연출 단계를 순서대로 뽑는다.

    /** `A1`, `A1:C5`, `시트!A1:C5`를 상자로 읽는다. 읽히지 않으면 `null`이고, 그 연산은 마지막 쓰기가 오류로 알린다. */
    function parseBox(raw) {
      const api = refs();
      const text = String(raw ?? "").trim().replaceAll("$", "").replace(/^[^!]+!/, "");
      if (!text || !api) return null;
      const [from, to = from] = text.split(":").map((part) => api.parseRef(part));
      if (!from || !to) return null;
      return { r1: Math.min(from.row, to.row), c1: Math.min(from.col, to.col), r2: Math.max(from.row, to.row), c2: Math.max(from.col, to.col) };
    }

    const sheetOf = (op, raw) => String(op.sheet || /^([^!]+)!/.exec(String(raw ?? "").trim())?.[1] || getActiveSheetName());

    function formatButtons(op) {
      const names = [];
      if (typeof op.bold === "boolean") names.push("bold");
      if (op.fill !== undefined) names.push("fill");
      if (op.color !== undefined) names.push("color");
      if (typeof op.numFmt === "string") names.push(op.numFmt.includes("%") ? "percent" : "currency");
      return names;
    }

    function rangeTarget(op, opIndex, kind, raw) {
      const box = parseBox(raw);
      return box ? [{ owner: "sheet", opIndex, kind: "sheet-range", op: kind, sheet: sheetOf(op, raw), box, text: "" }] : [];
    }

    /** 연산 하나의 연출 단계들. 읽히지 않는 범위와 틀 고정처럼 보여 줄 곳이 없는 작업은 단계가 없다. */
    function collect(op, opIndex, kind) {
      if (kind === "sheet.format") {
        const box = parseBox(op.range);
        const buttons = formatButtons(op);
        return box ? [{ owner: "sheet", opIndex, kind: "sheet-format", sheet: sheetOf(op, op.range), box, buttons, text: "" }] : [];
      }
      if (kind === "sheet.chart") {
        const raw = op.dataRange ?? op.range;
        const box = parseBox(raw);
        const size = { width: Math.min(1600, Math.max(180, Number(op.width) || CHART_W)), height: Math.min(1200, Math.max(140, Number(op.height) || CHART_H)) };
        return box ? [{ owner: "sheet", opIndex, kind: "sheet-chart", sheet: sheetOf(op, raw), box, ...size, text: "" }] : [];
      }
      if (kind === "sheet.structure") {
        const plan = STRUCTURE_ACTIONS[String(op.action || "").trim()];
        if (!plan) return [];
        const at = Math.max(1, Number(op.at ?? op.index) || 1);
        const count = Math.max(1, Math.min(64, Number(op.count) || 1));
        return [{ owner: "sheet", opIndex, kind: "sheet-structure", sheet: sheetOf(op), ...plan, at, count, text: "" }];
      }
      if (kind === "sheet.table") return String(op.action || "create").trim() === "create" ? rangeTarget(op, opIndex, kind, op.range) : [];
      if (kind === "sheet.pivot") return rangeTarget(op, opIndex, kind, op.sourceRange);
      if (kind === "sheet.sort_filter" || kind === "sheet.cond_format") return rangeTarget(op, opIndex, kind, op.range);
      if (kind === "sheet.note") return rangeTarget(op, opIndex, kind, op.coord ?? op.cell);
      return [];
    }

    // 멈춘 시트 작업은 쓰지 않는다. 앞 연산은 그대로 남는다(`copilot-perform.js`의 `cutOperations`).
    const cut = () => null;

    // ---- 연출: 단계마다 포인터가 하는 일.

    function createDrivers(session) {
      const { cursor } = session;
      const cellAt = (r, c) => session.visibleRect(document.querySelector(`#sheet-grid td[data-cell="${refs().colName(c)}${r}"]`));

      /** 상자의 두 모서리 칸으로 잰 화면 사각형. 가상 스크롤로 한쪽이 없으면 있는 쪽만 쓰고, 둘 다 없으면 `null`이다. */
      function boxRect(box) {
        const corners = [cellAt(box.r1, box.c1), cellAt(box.r2, box.c2)].filter(Boolean);
        return corners.length > 0 ? union(corners) : null;
      }

      // 범위 안쪽 왼쪽 위에서 눌러 오른쪽 아래까지 끌어 선택한다.
      async function selectRange(rect) {
        await session.moveTo(rect.left + 4, rect.top + 4);
        await session.gesture(DRAG_MS, (k) => {
          const width = rect.width * k;
          const height = rect.height * k;
          cursor.setSelection([{ left: rect.left, top: rect.top, width, height }]);
          cursor.place(rect.left + width, rect.top + height);
        });
        if (!session.stopped()) cursor.setSelection([rect]);
      }

      const pressButton = async (selector) => {
        const rect = session.visibleRect(document.querySelector(selector));
        if (!rect || session.stopped()) return;
        await session.clickAt(rect);
        await session.hold(HOLD_MS);
      };

      // 현재 시트에서 범위를 보여 줄 수 있으면 그 사각형을, 없으면 `null`을 돌려준다(그러면 그 단계는 연출 없이 지나간다).
      function rangeRect(target) {
        if (target.sheet !== getActiveSheetName()) return null;
        document.querySelector(`#sheet-grid td[data-cell="${refs().colName(target.box.c1)}${target.box.r1}"]`)?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
        return boxRect(target.box);
      }

      async function formatRange(target) {
        const rect = rangeRect(target);
        if (!rect) return { typed: 0 };
        await selectRange(rect);
        for (const name of target.buttons) await pressButton(FORMAT_BUTTONS[name]);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      // 차트가 놓일 자리: 데이터 범위 오른쪽 첫 행에서 시작하고, 창 오른쪽 끝을 넘지 않게 줄인다.
      function chartFrame(target, rect) {
        const bound = session.visibleRect(document.querySelector("#sheet-viewport"));
        const left = rect.right + CHART_GAP;
        const room = bound ? bound.right - left - 8 : target.width;
        return { left, top: rect.top, width: Math.max(MIN_CHART_W, Math.min(target.width, room)), height: target.height };
      }

      async function drawChart(target) {
        const rect = rangeRect(target);
        if (!rect) return { typed: 0 };
        await selectRange(rect);
        await pressButton(MENU_TITLE(CHART_MENU));
        if (session.stopped()) return { typed: 0 };
        const frame = chartFrame(target, rect);
        await session.moveTo(frame.left, frame.top);
        await session.gesture(DRAW_MS, (k) => {
          const width = frame.width * k;
          const height = frame.height * k;
          cursor.setSelection([{ left: frame.left, top: frame.top, width, height }]);
          cursor.place(frame.left + width, frame.top + height);
        });
        if (!session.stopped()) await session.hold(HOLD_MS);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      // 행·열 머리글: 지금 시트에서 보이는 것만 잰다(위쪽 `MAX_HEADERS`개).
      function headerRect(target) {
        if (target.sheet !== getActiveSheetName()) return null;
        const found = [];
        for (let n = target.at; n < target.at + Math.min(target.count, MAX_HEADERS); n += 1) {
          const key = target.axis === "row" ? `th[data-row="${n}"]` : `th[data-col="${refs().colName(n)}"]`;
          const rect = session.visibleRect(document.querySelector("#pane-sheet " + key));
          if (rect) found.push(rect);
        }
        return found.length > 0 ? union(found) : null;
      }

      // 머리글을 눌러 선택한다(삽입·삭제 메뉴가 뜨는 곳). 삽입·삭제 자체는 마지막 적용이 한 번에 한다.
      async function editStructure(target) {
        const rect = headerRect(target);
        if (!rect) return { typed: 0 };
        cursor.setSelection([rect]);
        await session.clickAt(rect);
        if (!session.stopped()) await session.hold(HOLD_MS);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      async function pointRange(target) {
        const rect = rangeRect(target);
        if (!rect) return { typed: 0 };
        await selectRange(rect);
        await pressButton(MENU_TITLE(RANGE_MENUS[target.op]));
        cursor.setSelection([]);
        return { typed: 0 };
      }

      return { "sheet-format": formatRange, "sheet-chart": drawChart, "sheet-structure": editStructure, "sheet-range": pointRange };
    }

    return { collect, cut, createDrivers, MAX_SHEET_STEPS };
  }

  host.__EASYOFFICE_COPILOT_PERFORM_SHEET__ = Object.freeze({ createSheetPerformance, MAX_SHEET_STEPS });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
