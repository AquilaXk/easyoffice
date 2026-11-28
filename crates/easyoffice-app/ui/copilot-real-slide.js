// 직접 썼다(PD-392): 슬라이드(SLIDE)의 연산을 실제 화면 조작 단계로 바꾸는 표다. 실행은 `copilot-real.js`가 맡는다. 메뉴 항목은 data 속성으로 찾는다.
//
//   slide.add(title[, body])     : 새 슬라이드 단추를 누르고(click) → 제목 칸을 눌러 글을 치고(type) → 본문 칸을 눌러 친다 → 무대를 눌러 확정   기대: 슬라이드 제목·본문 = 쓴 글
//   slide.add_element(textbox)   : 삽입 메뉴의 텍스트 상자 항목을 눌러 상자를 만들고 → 상자를 끌어 옮기고(drag) → 모서리를 끌어 키우고
//                                  → 두 번 눌러 글 편집으로 들어가 전체 선택 후 글을 친다                                             기대: 상자의 자리·크기·글 = 연산의 값
//
// 다른 개체(도형·그림·표·차트), 레이아웃·전환·노트·개체 목록 교체, 서식이 붙은 글상자, 현재 슬라이드가 아닌 슬라이드의 변경은 내부 경로로 적용한다. 상자의 자리와 크기는 끈
// 결과가 연산의 값과 2.5 단위(무대 좌표, 화면 1~2픽셀)보다 어긋나면 실패로 보아(맞춤선 끌림 포함) 내부 경로로 넘긴다.

(function (host) {
  "use strict";

  const BOX_KEYS = new Set(["op", "type", "kind", "element", "index", "slideIndex"]);
  const ELEMENT_KEYS = new Set(["kind", "x", "y", "w", "h", "text"]);
  // 끈 자리의 허용 오차(무대 좌표). 마우스 좌표는 화면 픽셀 단위로 읽히므로 한 픽셀(무대가 960 단위로 568픽셀이면 1.7 단위)쯤은 어긋난다.
  const SLACK = 2.5;
  const opKind = (op) => String(op?.op || op?.type || op?.kind || "").trim();
  const plainText = (value) => typeof value === "string" && value.trim() !== "" && !/[\n\t\r]/.test(value);

  function createSlidePlanner({ document, slideDeckState, getActiveSlideIndex }) {
    const codec = () => host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
    const aspectOf = () => String(slideDeckState[0]?.aspect || "16:9");
    const byId = (id) => document.getElementById(id);
    const layerCards = () => Array.from(document.querySelectorAll("#slide-canvas-layer .slide-el"));

    function addSupport(op) {
      const keys = Object.keys(op).filter((key) => !["op", "type", "kind", "title", "body"].includes(key));
      return keys.length === 0 && plainText(op.title) && (op.body === undefined || plainText(op.body)) ? 1 : 0;
    }

    function boxSupport(op) {
      const spec = op.element && typeof op.element === "object" ? op.element : null;
      const named = Number.isInteger(op.index) || Number.isInteger(op.slideIndex) ? Number(op.index ?? op.slideIndex) : getActiveSlideIndex();
      if (!spec || named !== getActiveSlideIndex() || !Object.keys(op).every((key) => BOX_KEYS.has(key))) return 0;
      const plain = Object.keys(spec).every((key) => ELEMENT_KEYS.has(key)) && String(spec.kind) === "textbox" && plainText(spec.text);
      return plain && ["x", "y", "w", "h"].every((key) => Number.isFinite(Number(spec[key]))) ? 1 : 0;
    }

    function supports(op) {
      const kind = opKind(op);
      if (kind === "slide.add") return addSupport(op);
      return kind === "slide.add_element" ? boxSupport(op) : 0;
    }

    function planAdd(op) {
      const count = slideDeckState.length;
      const slide = () => slideDeckState[count];
      const body = op.body === undefined ? "" : String(op.body);
      const title = String(op.title);
      const steps = [
        { act: "click", label: "새 슬라이드", find: () => byId("btn-add-slide"), expect: () => slideDeckState.length === count + 1 },
        { act: "click", label: "제목 칸", find: () => byId("slide-title-zone") },
        { act: "type", label: "제목 입력", text: title },
      ];
      if (body) {
        steps.push({ act: "click", label: "본문 칸", find: () => byId("slide-lower-zone") }, { act: "type", label: "본문 입력", text: body });
      }
      steps.push({ act: "click", label: "무대", find: () => byId("slide-stage-box"), hint: [0.5, 0.97], mark: false, expect: () => slide()?.title === title && String(slide()?.lower ?? "") === body });
      return { steps, locations: [{ module: "SLIDE", label: "슬라이드 " + (count + 1), slideIndex: count }] };
    }

    // 무대 좌표(연산의 값)를 화면 좌표로.
    function frameOf(spec) {
      const rect = byId("slide-stage-box").getBoundingClientRect();
      const size = codec().stageSizeOf(aspectOf());
      return {
        left: rect.left + (spec.x / size.w) * rect.width,
        top: rect.top + (spec.y / size.h) * rect.height,
        width: (spec.w / size.w) * rect.width,
        height: (spec.h / size.h) * rect.height,
        scale: rect.width / size.w,
      };
    }

    function planBox(op) {
      const index = getActiveSlideIndex();
      const aspect = aspectOf();
      const want = codec().normalizeSlideElement(op.element, aspect, "el-" + ((slideDeckState[index]?.elements?.length || 0) + 1));
      if (!want || want.kind !== "textbox") return null;
      const before = layerCards().length;
      const model = () => slideDeckState[index]?.elements?.at(-1);
      const near = (a, b) => Math.abs(Number(a) - Number(b)) <= SLACK;
      const card = () => layerCards().at(-1);
      const editor = () => card()?.querySelector(".slide-el-textbox");
      const item = () => document.querySelector('[data-menu="insert-object"] [data-slide-act="insert-textbox"]');
      return {
        steps: [
          { act: "click", label: "삽입 메뉴", find: () => document.querySelector('[data-menu="insert-object"] > .menu-title'), expect: () => (item()?.getBoundingClientRect().height || 0) > 0 },
          { act: "click", label: "텍스트 상자", find: item, expect: () => layerCards().length === before + 1 },
          {
            act: "drag",
            label: "상자 옮기기",
            find: card,
            // 누른 자리(`from`)에서 상자의 왼쪽 위가 목표 자리까지 가는 만큼 끈다.
            to: (el, from) => {
              const now = el.getBoundingClientRect();
              const goal = frameOf(want);
              return { x: from.x + (goal.left - now.left), y: from.y + (goal.top - now.top) };
            },
            expect: () => near(model()?.x, want.x) && near(model()?.y, want.y),
          },
          {
            act: "drag",
            label: "상자 키우기",
            find: () => card()?.querySelector('.slide-el-handle[data-dir="se"]'),
            to: (el, from) => {
              const cur = model();
              const { scale } = frameOf(want);
              return { x: from.x + (want.w - cur.w) * scale, y: from.y + (want.h - cur.h) * scale };
            },
            expect: () => near(model()?.w, want.w) && near(model()?.h, want.h),
          },
          { act: "dblclick", label: "글 편집", find: card, hint: [0.5, 0.5], mark: false, expect: () => editor()?.getAttribute("contenteditable") === "plaintext-only" },
          { act: "key", key: "a", mods: { meta: true }, label: "전체 선택" },
          { act: "type", label: "상자 글 입력", text: want.text, expect: () => model()?.text === want.text },
          { act: "click", label: "무대", find: () => byId("slide-stage-box"), hint: [0.5, 0.97], mark: false, expect: () => editor()?.getAttribute("contenteditable") === null },
        ],
        locations: [{ module: "SLIDE", label: "슬라이드 " + (index + 1), slideIndex: index }],
      };
    }

    function plan(op) {
      return opKind(op) === "slide.add" ? planAdd(op) : planBox(op);
    }

    return { supports, plan };
  }

  host.__EASYOFFICE_COPILOT_REAL_SLIDE__ = Object.freeze({ createSlidePlanner });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
