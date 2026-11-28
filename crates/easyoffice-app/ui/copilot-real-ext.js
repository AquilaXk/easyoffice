// 직접 썼다(PD-392): 데이터베이스(BASE)와 보드(BOARD)의 연산을 실제 화면 조작 단계로 바꾸는 표다. 실행은 `copilot-real.js`가 맡는다.
//
//   base.add_records : "새 레코드" 단추를 누르고(click) → 새 행의 칸마다 누르고 글을 치고(type) Enter로 확정한다     기대: 레코드 값 = 쓴 글
//   base.update_records : 행의 칸을 누르고 → 새 글을 치고 → Enter                                                    기대: 레코드 값 = 쓴 글
//   board.add_graph  : (보드 계획은 같은 파일 아래)
//
// 글자·숫자 필드만 화면 조작으로 쓴다. 선택·다중 선택·날짜·체크·수식·관계 필드, 필드 추가, 보기 설정은 내부 경로로 적용한다(칸마다 위젯이 달라 같은 입력으로 쓸 수 없다).
// 칸은 필드 id와 레코드 id로 찾는다(열 순서나 행 번호에 묶이지 않는다).

(function (host) {
  "use strict";

  const MAX_RECORDS = 20;
  const TYPED_FIELDS = new Set(["text", "number"]);
  const opKind = (op) => String(op?.op || op?.type || op?.kind || "").trim();

  function createBasePlanner({ document }) {
    const hostApi = () => host.__EASYOFFICE_BASE_BOARD_HOST__;
    const model = () => host.__EASYOFFICE_BASE_MODEL__;
    const stateOf = () => hostApi()?.getBaseStudio?.()?.getState?.();
    const tableOf = (state) => model()?.getTable(state, state.activeTableId) || state?.tables?.[0];
    const fieldByName = (table, name) => table.fields.find((f) => f.name === name || f.name.toLowerCase() === String(name).toLowerCase());
    const plainValue = (value) => value !== undefined && value !== null && typeof value !== "object" && String(value) !== "" && !/[\n\t\r]/.test(String(value));

    function supports(op) {
      const kind = opKind(op);
      if (kind === "base.add_records") {
        const list = Array.isArray(op.records) ? op.records : [];
        return list.length > 0 && list.length <= MAX_RECORDS && list.every((rec) => rec && typeof rec === "object" && !Array.isArray(rec)) ? list.length : 0;
      }
      if (kind === "base.update_records") {
        const list = Array.isArray(op.updates) ? op.updates : [];
        return list.length > 0 && list.length <= MAX_RECORDS && list.every((item) => Number.isInteger(Number(item?.rowIndex)) && item.values && typeof item.values === "object") ? list.length : 0;
      }
      return 0;
    }

    const rowEl = (recordId) => document.querySelector(`tr.base-grid-row[data-record-id="${recordId}"]`);
    const cellEl = (recordId, fieldId) => () => rowEl(recordId)?.querySelector(`td[data-field-id="${fieldId}"]`) || null;
    const valueOf = (recordId, fieldId) => {
      const state = stateOf();
      return String(tableOf(state)?.records.find((r) => r.id === recordId)?.values?.[fieldId] ?? "");
    };

    // 칸 하나를 쓰는 단계: 누르고, 글을 치고, Enter로 확정한다. `recordId`는 쓸 때 정해지는 값이면 함수다.
    function cellSteps(recordId, field, raw, label) {
      const id = () => (typeof recordId === "function" ? recordId() : recordId);
      return [
        { act: "click", label: `${label} 선택`, find: () => cellEl(id(), field.id)() },
        { act: "type", label: `${label} 입력`, text: raw, fast: true },
        { act: "key", key: "Enter", label: `${label} 확정`, expect: () => valueOf(id(), field.id) === raw },
      ];
    }

    // 쓸 칸: 레코드 객체의 값 중 필드가 있는 것. 글자·숫자가 아닌 필드가 있으면 `null`(내부 경로).
    function fieldsOf(table, values) {
      const out = [];
      for (const [name, value] of Object.entries(values)) {
        const field = fieldByName(table, name);
        if (!field) continue;
        if (!TYPED_FIELDS.has(field.type) || !plainValue(value)) return null;
        out.push({ field, raw: String(value) });
      }
      return out;
    }

    function planAdd(op, table) {
      const steps = [];
      let count = table.records.length;
      for (const [index, record] of op.records.entries()) {
        const fields = fieldsOf(table, record);
        if (!fields) return null;
        const expected = count + 1;
        steps.push({
          act: "click",
          label: `새 레코드 ${index + 1}`,
          find: () => document.querySelector(".base-new-record-row .base-inline-add-btn"),
          expect: () => tableOf(stateOf()).records.length === expected,
        });
        const newest = () => tableOf(stateOf()).records.at(-1).id;
        for (const { field, raw } of fields) steps.push(...cellSteps(newest, field, raw, `${field.name}`));
        count = expected;
      }
      return { steps, locations: [{ module: "BASE", label: `${table.name} (+${op.records.length}건)`, range: table.name }] };
    }

    function planUpdate(op, table, state) {
      const view = model().queryTableView(state, table.id);
      const steps = [];
      for (const item of op.updates) {
        const row = view.rows[Number(item.rowIndex)] || table.records[Number(item.rowIndex)];
        const recordId = row?.record ? row.record.id : row?.id;
        if (!recordId) return null;
        const fields = fieldsOf(table, item.values);
        if (!fields) return null;
        for (const { field, raw } of fields) steps.push(...cellSteps(recordId, field, raw, `${field.name}`));
      }
      return steps.length > 0 ? { steps, locations: [{ module: "BASE", label: table.name, range: table.name }] } : null;
    }

    function plan(op) {
      const state = stateOf();
      const table = state && model() ? tableOf(state) : null;
      if (!table) return null;
      return opKind(op) === "base.add_records" ? planAdd(op, table) : planUpdate(op, table, state);
    }

    return { supports, plan };
  }

  // ---- BOARD: 도구줄의 도형 단추로 노드를 만들고, 끌어 옮기고, 모서리를 끌어 키우고, 두 번 눌러 글을 친다. 끝에 전체 맞춤 단추를 누른다(내부 경로의 `fitAll`과 같다).
  // 간선이 있는 그래프, 도형 종류·색이 정해진 노드, 텍스트 노드는 내부 경로로 적용한다(연결선과 색은 이 표가 다루지 않는다). 노드 id는 보드가 매기므로 연산의 id와 다르다.
  const MAX_BOARD_NODES = 12;
  const PLAIN_SHAPES = new Set([undefined, "rect", "roundedRect", "roundRect"]);
  // 끈 자리의 허용 오차(화면 2픽셀). 마우스 좌표는 화면 픽셀 단위로 읽힌다.
  const BOARD_SLACK_PX = 2;

  function createBoardPlanner({ document, boardApi }) {
    const studio = () => host.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.();
    const nodes = () => studio()?.getState?.().nodes || [];

    function supports(op) {
      if (opKind(op) !== "board.add_graph" || !boardApi) return 0;
      const list = Array.isArray(op.nodes) ? op.nodes : [];
      const plain = list.length > 0 && list.length <= MAX_BOARD_NODES && (op.edges === undefined || (Array.isArray(op.edges) && op.edges.length === 0))
        && list.every((node) => node && !node.fill && node.kind !== "text" && PLAIN_SHAPES.has(node.shape) && String(node.text ?? node.label ?? node.id ?? "").trim() !== "" && !/[\n\t\r]/.test(String(node.text ?? node.label ?? node.id ?? "")));
      return plain ? list.length : 0;
    }

    function plan(op) {
      const existing = boardApi.existing();
      const frames = boardApi.frames(op, existing);
      if (!Array.isArray(frames) || frames.length !== op.nodes.length) return null;
      const steps = [];
      const locations = [];
      let count = nodes().length;
      op.nodes.forEach((spec, index) => {
        const want = frames[index];
        const text = String(spec.text ?? spec.label ?? spec.id ?? `g-${index + 1}`);
        const before = count;
        const node = () => nodes().at(-1);
        const card = () => Array.from(document.querySelectorAll("#board-node-layer .board-node")).find((el) => el.dataset.nodeId === node()?.id);
        // 화면 한 픽셀이 세계 좌표 몇 단위인지: 카드가 그려진 너비 / 노드 너비.
        const unitsPerPx = () => (node().w || 1) / Math.max(1, card().getBoundingClientRect().width);
        const near = (a, b) => Math.abs(Number(a) - Number(b)) <= Math.max(2.5, BOARD_SLACK_PX * unitsPerPx());
        steps.push(
          { act: "click", label: `도형 ${index + 1}`, find: () => document.querySelector("#btn-board-add-shape"), expect: () => nodes().length === before + 1 },
          {
            act: "drag",
            label: `노드 ${index + 1} 옮기기`,
            find: card,
            to: (el, from) => ({ x: from.x + (want.x - node().x) / unitsPerPx(), y: from.y + (want.y - node().y) / unitsPerPx() }),
            expect: () => near(node()?.x, want.x) && near(node()?.y, want.y),
          },
          {
            act: "drag",
            label: `노드 ${index + 1} 키우기`,
            find: () => card()?.querySelector('.board-resize-handle[data-dir="se"]'),
            to: (el, from) => ({ x: from.x + (want.w - node().w) / unitsPerPx(), y: from.y + (want.h - node().h) / unitsPerPx() }),
            expect: () => near(node()?.w, want.w) && near(node()?.h, want.h),
          },
          { act: "dblclick", label: `노드 ${index + 1} 글 편집`, find: card, mark: false, expect: () => card()?.querySelector(".board-node-label")?.getAttribute("contenteditable") === "plaintext-only" },
          { act: "key", key: "a", mods: { meta: true }, label: "전체 선택" },
          { act: "type", label: `노드 ${index + 1} 글 입력`, text, expect: () => node()?.text === text },
          { act: "click", label: "빈 곳", find: () => document.querySelector("#board-viewport"), hint: [0.5, 0.98], mark: false, expect: () => card()?.querySelector(".board-node-label")?.getAttribute("contenteditable") === null },
        );
        count += 1;
      });
      steps.push({ act: "click", label: "전체 맞춤", find: () => document.querySelector("#btn-board-fit-all") });
      locations.push({ module: "BOARD", label: `다이어그램 요소 ${op.nodes.length}개`, range: `요소 ${count}개` });
      return { steps, locations };
    }

    return { supports, plan };
  }

  host.__EASYOFFICE_COPILOT_REAL_EXT__ = Object.freeze({ createBasePlanner, createBoardPlanner });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
