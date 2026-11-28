// 직접 썼다(PD-392): 시트(SHEET)의 연산을 실제 화면 조작 단계로 바꾸는 표다. 실행은 `copilot-real.js`가 맡는다. 단추는 위치가 아니라 id와 data 속성으로 찾는다
// (메뉴 배치가 바뀌어도 이 표는 그대로다).
//
//   sheet.write(cells)            : 셀을 누르고(click) → 글을 한 자씩 치고(type) → Enter로 확정한다                 기대: 셀의 값·수식이 쓴 글과 같다
//   sheet.format(range, bold)     : 범위를 끌어 선택(drag) → 서식 메뉴를 열고 굵게 항목을 누른다(click)              기대: 범위의 모든 셀이 굵다
//   sheet.structure(insert_rows)  : 행 머리글을 오른쪽 단추로 누르고(contextmenu) → "행 위에 삽입"을 누른다         기대: 아래 셀들이 한 줄 밀렸다
//   sheet.structure(insert_cols)  : 열 머리글 → "열 왼쪽에 삽입"                                                  기대: 오른쪽 셀들이 한 칸 밀렸다
//
// 채우기·글자색·숫자 서식·차트·정렬 같은 나머지 연산, 셀 값이 아닌 글(개행·표지 없는 빈 글), 다른 시트나 보호된 시트는 내부 경로로 적용한다. 셀 하나를 쓰는 단계는 셀을
// 하나씩 센다(`copilot-perform.js`의 `MAX_ANIMATED_CELLS`와 같은 상한 40).

(function (host) {
  "use strict";

  const MAX_CELLS = 40;
  const MAX_FORMAT_CELLS = 24;
  const COORD = /^[A-Z]{1,3}[1-9][0-9]{0,6}$/;
  const FORMAT_KEYS = new Set(["op", "type", "kind", "sheet", "range", "bold"]);
  const BOLD = /^(600|700|bold)$/;
  const opKind = (op) => String(op?.op || op?.type || op?.kind || "").trim();

  function createSheetPlanner({ document, getActiveSheetName, getSheetController }) {
    const cellEl = (coord) => () => document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
    const store = () => getSheetController().snapshotDomSheetCells();
    const sameSheet = (op) => !op.sheet || String(op.sheet) === getActiveSheetName();
    const nameApi = () => host.__EASYOFFICE_SHEET_NAMES__;
    const refs = () => host.__EASYOFFICE_SHEET_REFS__;

    function writeSupport(op) {
      const cells = Array.isArray(op.cells) ? op.cells : [];
      if (cells.length === 0 || cells.length > MAX_CELLS || !sameSheet(op)) return 0;
      const plain = cells.every((cell) => {
        const raw = cell?.raw ?? cell?.formula ?? cell?.value;
        const text = raw === undefined || raw === null ? "" : String(raw);
        const styled = ["bold", "fill", "color", "numFmt", "t"].some((key) => cell[key] !== undefined);
        return COORD.test(String(cell.coord || "").trim().toUpperCase()) && text !== "" && !/[\n\t\r]/.test(text) && !text.startsWith("'") && !styled;
      });
      return plain ? cells.length : 0;
    }

    function formatSupport(op) {
      if (!sameSheet(op) || op.bold !== true || !Object.keys(op).every((key) => FORMAT_KEYS.has(key))) return 0;
      const range = nameApi()?.parseAddress(String(op.range || "").trim().toUpperCase());
      return range && (range.r2 - range.r1 + 1) * (range.c2 - range.c1 + 1) <= MAX_FORMAT_CELLS ? 1 : 0;
    }

    function structureSupport(op) {
      const action = String(op.action || "");
      const at = Number(op.at ?? op.index);
      const count = op.count === undefined ? 1 : Number(op.count);
      return (action === "insert_rows" || action === "insert_cols") && sameSheet(op) && Number.isInteger(at) && at >= 1 && at <= 99 && count === 1 ? 1 : 0;
    }

    /** 화면 조작으로 할 수 있는 모양이면 단계 수, 아니면 0. */
    function supports(op) {
      const kind = opKind(op);
      if (kind === "sheet.write") return writeSupport(op);
      if (kind === "sheet.format") return formatSupport(op);
      return kind === "sheet.structure" ? structureSupport(op) : 0;
    }

    const written = (coord, raw) => () => {
      const entry = store()[coord];
      return raw.startsWith("=") ? entry?.formula === raw : Boolean(entry) && (entry.formula || "") === "" && String(entry.value ?? "") === raw;
    };

    function planWrite(op) {
      const steps = [];
      for (const cell of op.cells) {
        const coord = String(cell.coord).trim().toUpperCase();
        const raw = String(cell.raw ?? cell.formula ?? cell.value);
        const find = cellEl(coord);
        steps.push(
          { act: "click", label: `${coord} 선택`, find, expect: () => document.activeElement === find() },
          { act: "type", label: `${coord} 입력`, text: raw, fast: true },
          { act: "key", key: "Enter", label: `${coord} 확정`, expect: written(coord, raw) },
        );
      }
      // 내부 경로(`executeSheetWrite`)와 같은 자리 표시: 쓴 셀을 모두 감싸는 범위.
      const { parseRef, refOf } = refs();
      const points = op.cells.map((cell) => parseRef(String(cell.coord).trim().toUpperCase()));
      const span = (pick, key) => pick(...points.map((p) => p[key]));
      const range = `${refOf(span(Math.min, "row"), span(Math.min, "col"))}:${refOf(span(Math.max, "row"), span(Math.max, "col"))}`;
      return { steps, locations: [{ module: "SHEET", label: range, sheet: getActiveSheetName(), range }] };
    }

    function planBold(op) {
      const { refOf } = refs();
      const range = nameApi().parseAddress(String(op.range).trim().toUpperCase());
      const first = refOf(range.r1, range.c1);
      const last = refOf(range.r2, range.c2);
      const coords = [];
      for (let r = range.r1; r <= range.r2; r += 1) for (let c = range.c1; c <= range.c2; c += 1) coords.push(refOf(r, c));
      // 이미 굵은 칸이 있으면 단추가 굵게를 풀어 버린다: 내부 경로가 맞다.
      const snap = store();
      if (coords.some((coord) => BOLD.test(String(snap[coord]?.style?.fontWeight ?? "")))) return null;
      const picked = () => {
        const sel = getSheetController().getSelectionRange?.();
        return Boolean(sel) && sel.r1 === range.r1 && sel.c1 === range.c1 && sel.r2 === range.r2 && sel.c2 === range.c2;
      };
      const steps = [];
      if (first === last) steps.push({ act: "click", label: `${first} 선택`, find: cellEl(first), expect: picked });
      else {
        steps.push({
          act: "drag",
          label: `${first}:${last} 선택`,
          find: cellEl(first),
          to: () => {
            const r = cellEl(last)()?.getBoundingClientRect();
            return r ? { x: r.left + r.width / 2, y: r.top + r.height / 2 } : { x: 0, y: 0 };
          },
          expect: picked,
        });
      }
      // 시트에서는 도구줄의 굵게 단추가 숨겨져 있다: 서식 메뉴를 열고 시트용 굵게 항목을 누른다(메뉴 위치가 아니라 명령 이름과 대상 모듈로 찾는다).
      const menuItem = () => Array.from(document.querySelectorAll('[data-menu="format"] [data-studio-menu-cmd="home-bold"]')).find((el) => (el.dataset.kinds || "").split(" ").includes("SHEET"));
      steps.push({
        act: "click",
        label: "서식 메뉴",
        find: () => document.querySelector('[data-menu="format"] > .menu-title'),
        expect: () => (menuItem()?.getBoundingClientRect().height || 0) > 0,
      });
      steps.push({
        act: "click",
        label: "굵게",
        find: menuItem,
        expect: () => {
          const now = store();
          return coords.every((coord) => BOLD.test(String(now[coord]?.style?.fontWeight ?? "")));
        },
      });
      const label = first === last ? first : `${first}:${last}`;
      return { steps, locations: [{ module: "SHEET", label, sheet: getActiveSheetName(), range: label }] };
    }

    // 머리글 오른쪽 단추 메뉴로 한 줄(칸)을 넣는다. 기대: 넣은 자리 아래(오른쪽)의 값 있는 셀이 한 칸씩 밀렸다.
    function planInsert(op) {
      const { colName, parseRef, refOf } = refs();
      const isRow = op.action === "insert_rows";
      const at = Number(op.at ?? op.index);
      const header = isRow ? `#pane-sheet th[data-row="${at}"]` : `#pane-sheet th[data-col="${colName(at)}"]`;
      const act = isRow ? "row-insert-above" : "col-insert-left";
      const moved = [];
      for (const [coord, entry] of Object.entries(store())) {
        const p = parseRef(coord);
        if (!p || !(entry.formula || String(entry.value ?? "") !== "") || String(entry.formula || "").startsWith("=")) continue;
        if ((isRow ? p.row : p.col) >= at && moved.length < 40) moved.push({ to: isRow ? refOf(p.row + 1, p.col) : refOf(p.row, p.col + 1), value: String(entry.value ?? "") });
      }
      const label = isRow ? `A${at}` : `${colName(at)}1`;
      return {
        steps: [
          { act: "contextmenu", label: "머리글 메뉴", find: () => document.querySelector(header), expect: () => document.querySelector("#sheet-header-menu")?.hidden === false },
          {
            act: "click",
            label: isRow ? "행 위에 삽입" : "열 왼쪽에 삽입",
            find: () => document.querySelector(`#sheet-header-menu [data-sheet-act="${act}"]`),
            expect: () => {
              const now = store();
              return moved.every((item) => String(now[item.to]?.value ?? "") === item.value);
            },
          },
        ],
        locations: [{ module: "SHEET", label, sheet: getActiveSheetName(), range: label }],
      };
    }

    function plan(op) {
      const kind = opKind(op);
      if (kind === "sheet.write") return planWrite(op);
      if (kind === "sheet.format") return planBold(op);
      return kind === "sheet.structure" ? planInsert(op) : null;
    }

    return { supports, plan };
  }

  host.__EASYOFFICE_COPILOT_REAL_SHEET__ = Object.freeze({ createSheetPlanner });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
