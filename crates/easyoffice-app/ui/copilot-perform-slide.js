// 참조 구현: 같은 폴더의 `copilot-cursor.js`(excalidraw 협업자 포인터)와 `copilot-typing.js`(typed.js 타이핑 루프), `copilot-timeline.js`(rrweb 재생기와 motion 시퀀스의
// 시각 읽기, 몸짓 `gesture`·`click`·`hold`가 쓴다)를 슬라이드에 엮는다.
// `copilot-perform.js`가 문서·시트 연출과 함께 부르는 슬라이드 쪽 부품이다: 어느 연산이 어떤 연출 단계가 되는지(`collect`), 중간에 멈추면 연산을 어디서
// 자르는지(`cut`), 단계마다 포인터가 하는 일(`createDrivers`)을 둔다. 포인터·타이핑·되돌리기 도구는 `copilot-perform.js`가 `session`으로 건넨다.
//
// 개체(글상자·도형·그림·표·차트)를 놓는 몸짓은 excalidraw(MIT, Copyright (c) 2020 Excalidraw, 커밋 2c3dcc06, `packages/excalidraw/components/App.tsx`)의
// 도형 그리기 순서를 따른다: 누른 자리에 새 개체를 만들고(`createGenericElementOnPointerDown`, 8820-8887), 끄는 만큼 키우고(`dragNewElement` 호출, 11114·11183),
// 손을 떼면 개체가 놓인다(`onPointerUpFromPointerDownHandler`, 9454부터). 코드는 옮기지 않았고 몸짓의 순서만 같다. THIRD_PARTY_NOTICES.md에 적었다.
// 화면에 그리는 임시 개체는 슬라이드 캔버스가 쓰는 `buildElementDom`과 `positionElementDom`(`slide-canvas.js`)을 그대로 부른다.

(function (host) {
  "use strict";

  const BODY_KEYS = ["bullets", "body", "lower"];
  // 한 슬라이드 연산의 필드를 적용하는 순서(`executeSlideOperation`과 같다). 멈추면 멈춘 필드 앞까지만 남긴다.
  const FIELD_ORDER = ["title", "body", "notes", "layout", "transition", "elements"];
  // 새 슬라이드의 레이아웃·전환은 슬라이드를 열 때 이미 보였으므로 멈춰도 남긴다.
  const OPENED_FIELDS = new Set(["layout", "transition"]);
  const MAX_LIST_ELEMENTS = 64;
  const CLICK_MS = 160;
  const HOLD_MS = 220;
  const DRAW_MS = 420;
  const MOVE_MS = 480;
  // 발표자 노트 입력칸의 글자 폭 근사값(px). 입력칸에는 글자 위치를 재는 범위가 없어 캐럿을 이 폭으로 옮긴다.
  const FIELD_CHAR_PX = 7;
  const TEXT_NODE_CLASS = { textbox: ".slide-el-textbox", shape: ".slide-el-shape-text" };

  const hasText = (element) => element.kind === "textbox" || element.kind === "shape";
  const textOf = (element) => (hasText(element) ? String(element.text ?? "") : "");
  const sameJson = (a, b) => JSON.stringify(a) === JSON.stringify(b);

  function createSlidePerformance(deps) {
    const { document, slideDeckState, getActiveSlideIndex, getWorkspacePanels } = deps;
    const codec = () => host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
    const aspectOf = () => String(slideDeckState[0]?.aspect || "16:9");
    const byId = (id) => document.getElementById(id);

    // ---- 목표: 연산에서 연출 단계를 순서대로 뽑는다.

    function bodyField(op) {
      const key = BODY_KEYS.find((name) => (name === "bullets" ? Array.isArray(op.bullets) : op[name] !== undefined));
      if (!key) return null;
      return { key, text: key === "bullets" ? op.bullets.map((b) => String(b ?? "")).join("\n") : String(op.body ?? op.lower ?? "") };
    }

    // 그림 바이트는 연산을 적용할 때 저장소에 등록된다. 연출용 개체는 바이트 없이 틀만 만들어 같은 그림이 두 번 등록되지 않게 한다.
    function stageElement(raw, fallbackId) {
      const api = codec();
      if (!api || !raw || typeof raw !== "object") return null;
      const stand = raw.kind === "image" ? { ...raw, imageId: raw.imageId || "pending", data: undefined, dataBase64: undefined } : raw;
      return api.normalizeSlideElement(stand, aspectOf(), fallbackId);
    }

    function objectTarget(base, plan, change, element, prev) {
      const nextText = textOf(element);
      const text = change === "change" && nextText === textOf(prev) ? "" : change === "remove" ? "" : nextText;
      return { ...base, kind: "slide-object", field: "elements", change, element, prev, plan, text };
    }

    // `elements` 목록 교체: 새 목록과 지금 목록을 같은 id끼리 맞춰 추가·변경·삭제 단계로 나눈다.
    function listTargets(base, list, oldList) {
      const next = list.slice(0, MAX_LIST_ELEMENTS).map((raw, i) => ({ raw, element: stageElement(raw, "el-" + (i + 1)) })).filter((entry) => entry.element);
      const plan = { old: oldList, next };
      const oldById = new Map(oldList.map((el) => [el.id, el]));
      const nextIds = new Set(next.map((entry) => entry.element.id));
      const targets = [];
      for (const entry of next) {
        entry.prev = oldById.get(entry.element.id);
        if (!entry.prev) targets.push(objectTarget(base, plan, "add", entry.element, null));
        else if (!sameJson(entry.prev, entry.element)) targets.push(objectTarget(base, plan, "change", entry.element, entry.prev));
      }
      for (const el of oldList) if (!nextIds.has(el.id)) targets.push(objectTarget(base, plan, "remove", el, el));
      return targets;
    }

    function textTargets(op, base) {
      const targets = [];
      if (op.title !== undefined) targets.push({ ...base, kind: "slide-title", field: "title", text: String(op.title) });
      const body = bodyField(op);
      if (body) targets.push({ ...base, kind: "slide-body", field: "body", key: body.key, text: body.text });
      if (op.notes !== undefined) targets.push({ ...base, kind: "slide-notes", field: "notes", text: String(op.notes) });
      return targets;
    }

    function formatTargets(op, base) {
      const api = codec();
      const targets = [];
      if (op.layout !== undefined && api) targets.push({ ...base, kind: "slide-format", field: "layout", value: api.normalizeLayout(op.layout), text: "" });
      if (op.transition !== undefined && api) targets.push({ ...base, kind: "slide-format", field: "transition", value: api.normalizeTransition(op.transition), text: "" });
      return targets;
    }

    /**
     * 슬라이드 연산 하나의 단계들. `state.length`는 앞 연산이 더한 슬라이드까지 센 슬라이드 수다
     * (그 슬라이드는 아직 없어서 화면에는 임시 빈 슬라이드로 열린다).
     */
    function collect(op, opIndex, kind, state) {
      if (kind === "slide.add") {
        const index = state.length;
        state.length += 1;
        const base = { opIndex, index, fresh: true };
        const open = { ...base, kind: "slide-add", field: "add", layout: op.layout, text: "" };
        const elements = Array.isArray(op.elements) ? listTargets(base, op.elements, []) : [];
        return [open, ...textTargets(op, base), ...elements];
      }
      const index = Number.isInteger(op.index) ? op.index : Number.isInteger(op.slideIndex) ? op.slideIndex : getActiveSlideIndex();
      if (index < 0 || index >= state.length) return [];
      const base = { opIndex, index, fresh: index >= slideDeckState.length };
      // 끌어서 놓은 개체는 소수점이 길 수 있다. 새 목록과 같은 정규형으로 맞춰 비교한다.
      const oldElements = () => (Array.isArray(slideDeckState[index]?.elements) ? slideDeckState[index].elements : []).map((el) => stageElement(el, el.id) || el);
      if (kind === "slide.add_element") {
        const oldList = oldElements();
        const element = stageElement(op.element && typeof op.element === "object" ? op.element : op, "el-" + (oldList.length + 1));
        return element ? [objectTarget(base, { old: oldList, next: [] }, "add", element, null)] : [];
      }
      if (kind !== "slide.update") return [];
      return [...textTargets(op, base), ...formatTargets(op, base), ...(Array.isArray(op.elements) ? listTargets(base, op.elements, oldElements()) : [])];
    }

    // ---- 자르기: 멈춘 곳까지만 남긴 연산.

    // 멈춘 개체 단계까지 쓴 새 목록. 끝낸 단계는 새 모양, 못 끝낸 것은 지금 모양을 쓰고, 쓰던 글은 쓴 만큼만 남긴다.
    function keptList(target, prefix, done) {
      const { plan } = target;
      const finished = (change) => new Set(done.filter((t) => t.plan === plan && t.change === change).map((t) => t.element.id));
      const added = finished("add");
      const changed = finished("change");
      const removed = finished("remove");
      const nextById = new Map(plan.next.map((entry) => [entry.element.id, entry]));
      const rawOf = (entry, text) => ({ ...entry.raw, id: entry.element.id, ...(text === undefined ? {} : { text }) });
      const partial = prefix && target.change !== "remove" ? target.element.id : null;
      const list = [];
      for (const old of plan.old) {
        const entry = nextById.get(old.id);
        if (removed.has(old.id)) continue;
        if (entry && changed.has(old.id)) list.push(rawOf(entry));
        else if (entry && partial === old.id) list.push(rawOf(entry, prefix));
        else list.push(old);
      }
      for (const entry of plan.next) {
        if (entry.prev) continue;
        if (added.has(entry.element.id)) list.push(rawOf(entry));
        else if (partial === entry.element.id) list.push(rawOf(entry, prefix));
      }
      return list;
    }

    function cutField(op, target, prefix, done) {
      if (target.field === "title" || target.field === "notes") {
        if (prefix) op[target.field] = prefix;
        else delete op[target.field];
      } else if (target.field === "body") {
        if (!prefix) delete op[target.key];
        else op[target.key] = target.key === "bullets" ? prefix.split("\n") : prefix;
      } else if (target.field === "layout" || target.field === "transition") {
        delete op[target.field];
      } else if (target.field === "elements") {
        // 이 목록의 단계를 하나도 끝내지 못했고 쓴 글도 없으면 개체는 그대로다.
        if (!prefix && !done.some((t) => t.plan === target.plan)) delete op.elements;
        else op.elements = keptList(target, prefix, done);
      }
    }

    // 멈춘 필드 뒤에 있는 필드를 지운다.
    function dropAfter(op, kind, field) {
      for (const later of FIELD_ORDER.slice(FIELD_ORDER.indexOf(field) + 1)) {
        if (kind === "slide.add" && OPENED_FIELDS.has(later)) continue;
        if (later === "body") for (const key of BODY_KEYS) delete op[key];
        else delete op[later];
      }
    }

    const hasContent = (op) => ["title", "notes", "layout", "transition", "elements", ...BODY_KEYS].some((key) => op[key] !== undefined);

    /**
     * 멈춘 슬라이드 연산의 복사본(`op`)을 자른다. `target`은 멈춘 단계, `typed`는 거기서 쓴 글자 수, `done`은 이미 끝낸 앞 단계들이다.
     * 남길 것이 없으면 `null`을 돌려준다. 새 슬라이드는 연 뒤에 멈추면 쓴 만큼 채운 슬라이드로 남는다.
     */
    function cut(op, kind, target, typed, done) {
      const prefix = target.text.slice(0, typed);
      if (kind === "slide.add_element") {
        if (!prefix) return null;
        const holder = op.element && typeof op.element === "object" ? op.element : op;
        holder.text = prefix;
        return op;
      }
      if (target.kind === "slide-add") return null;
      cutField(op, target, prefix, done);
      dropAfter(op, kind, target.field);
      return kind === "slide.add" || hasContent(op) ? op : null;
    }

    // ---- 연출: 단계마다 포인터가 하는 일.

    function createDrivers(session) {
      const { cursor } = session;
      // 새 슬라이드 연출이 열어 둔 임시 빈 슬라이드(`{ index }`). 실제 슬라이드로 옮기면 닫는다.
      let blank = null;

      const clickAt = async (rect) => {
        await session.moveTo(rect.left + rect.width / 2, rect.top + rect.height / 2);
        await session.click(CLICK_MS);
      };

      // 단계가 일할 슬라이드를 화면에 둔다. 새 슬라이드는 임시 빈 슬라이드가 열려 있을 때만 할 수 있다(`false`면 건너뛴다).
      function showSlide(target) {
        if (target.fresh) return blank?.index === target.index;
        if (blank || target.index !== getActiveSlideIndex()) {
          // 임시로 쓴 것을 돌려 놓아야 앞 슬라이드의 글이 이 슬라이드에 덮이지 않는다.
          session.revert();
          blank = null;
          if (target.index !== getActiveSlideIndex()) getWorkspacePanels()?.renderSlideDeck?.(target.index);
        }
        return true;
      }

      function openBlank(target) {
        const list = byId("slide-thumb-list");
        const active = list?.querySelector(".slide-thumb.active");
        const model = active || Array.from(list?.children || []).at(-1);
        if (list && model) {
          const thumb = model.cloneNode(true);
          thumb.className = "slide-thumb active";
          thumb.dataset.slideIdx = String(target.index);
          thumb.removeAttribute("aria-current");
          const label = (selector, text) => {
            const node = thumb.querySelector(selector);
            if (node) node.textContent = text;
          };
          label(".slide-thumb-num", String(target.index + 1));
          label(".slide-thumb-title", "");
          label(".slide-thumb-snippet", "");
          if (active) session.toggleClass(active, "active", false);
          list.appendChild(thumb);
          session.addTemp(thumb);
        }
        for (const id of ["slide-title-zone", "slide-lower-zone"]) {
          const zone = byId(id);
          if (!zone) continue;
          session.remember(zone);
          zone.textContent = "";
        }
        for (const card of Array.from(byId("slide-canvas-layer")?.children || [])) session.hide(card);
        setStageLayout(target.layout);
        blank = { index: target.index };
      }

      // 스테이지와 레이아웃 선택칸에 그 레이아웃을 임시로 보인다.
      function setStageLayout(layout) {
        const stage = byId("slide-stage-box");
        const api = codec();
        if (!stage || !api) return;
        const value = api.normalizeLayout(layout);
        const previous = stage.dataset.layout;
        stage.dataset.layout = value;
        session.onRevert(() => {
          stage.dataset.layout = previous;
        });
        const select = byId("slide-layout-select");
        if (select) session.setProp(select, "value", value);
      }

      async function addSlide(target) {
        const button = byId("btn-add-slide");
        if (button) await clickAt(session.rectOf(button));
        if (session.stopped()) return { typed: 0 };
        openBlank(target);
        return { typed: 0 };
      }

      async function writeZone(target) {
        if (!showSlide(target)) return { typed: target.text.length };
        const el = byId(target.kind === "slide-title" ? "slide-title-zone" : "slide-lower-zone");
        if (!el) return { typed: 0 };
        session.remember(el);
        await session.glideTo(session.caretAt(el));
        if (!target.fresh) await session.drag([el]);
        if (session.stopped()) return { typed: 0 };
        cursor.setSelection([]);
        el.textContent = "";
        return session.typeInto(el, target.text, (prefix) => {
          el.textContent = prefix;
        });
      }

      async function writeNotes(target) {
        if (!showSlide(target)) return { typed: target.text.length };
        const input = byId("slide-presenter-notes");
        if (!input) return { typed: 0 };
        const rect = session.rectOf(input);
        await session.glideTo({ left: rect.left + 8, top: rect.top, height: rect.height });
        if (session.stopped()) return { typed: 0 };
        cursor.setSelection([rect]);
        session.setProp(input, "value", "");
        const caret = (prefix) => ({
          left: Math.min(rect.left + 8 + Array.from(prefix).length * FIELD_CHAR_PX, rect.left + rect.width - 8),
          top: rect.top + 4,
          height: Math.max(8, rect.height - 8),
        });
        const result = await session.typeInto(input, target.text, (prefix) => {
          input.value = prefix;
        }, { caret });
        cursor.setSelection([]);
        return result;
      }

      async function setFormat(target) {
        if (!showSlide(target)) return { typed: 0 };
        const select = byId(target.field === "layout" ? "slide-layout-select" : "slide-transition-select");
        if (!select) return { typed: 0 };
        const rect = session.rectOf(select);
        cursor.setSelection([rect]);
        await clickAt(rect);
        if (session.stopped()) return { typed: 0 };
        if (target.field === "layout") setStageLayout(target.value);
        else session.setProp(select, "value", target.value);
        await session.hold(HOLD_MS);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      // 슬라이드 개체의 화면 위 사각형(뷰포트 좌표).
      function frameOf(el) {
        const rect = session.rectOf(byId("slide-stage-box"));
        const size = codec().stageSizeOf(aspectOf());
        return {
          left: rect.left + (el.x / size.w) * rect.width,
          top: rect.top + (el.y / size.h) * rect.height,
          width: (el.w / size.w) * rect.width,
          height: (el.h / size.h) * rect.height,
        };
      }

      function cardOf(layer, id) {
        return Array.from(layer.children).find((card) => card.dataset.elementId === id && !card.classList.contains("eo-copilot-ghost"));
      }

      function buildCard(layer, el) {
        const api = codec();
        const card = api.buildElementDom(document, el, api.stageSizeOf(aspectOf()), { interactive: false });
        layer.appendChild(card);
        session.addTemp(card);
        return card;
      }

      async function typeObjectText(card, target) {
        if (!target.text) return { typed: 0 };
        const node = card.querySelector(TEXT_NODE_CLASS[target.element.kind]);
        if (!node) return { typed: target.text.length };
        node.textContent = "";
        return session.typeInto(node, target.text, (prefix) => {
          node.textContent = prefix;
        });
      }

      // 빈 곳에서 끌어 상자를 키우고 손을 떼면 개체가 선택된 채 놓인다.
      async function drawObject(layer, target) {
        const frame = frameOf(target.element);
        await session.moveTo(frame.left, frame.top);
        await session.gesture(DRAW_MS, (k) => {
          const width = frame.width * k;
          const height = frame.height * k;
          cursor.setSelection([{ left: frame.left, top: frame.top, width, height }]);
          cursor.place(frame.left + width, frame.top + height);
        });
        if (session.stopped()) return { typed: 0 };
        const card = buildCard(layer, hasText(target.element) ? { ...target.element, text: "" } : target.element);
        cursor.setSelection([frame]);
        return typeObjectText(card, target);
      }

      // 있던 개체를 잡아 새 자리와 크기로 끌고 가고, 글이 바뀌었으면 새 글을 친다.
      async function reshapeObject(layer, old, target) {
        const api = codec();
        const size = api.stageSizeOf(aspectOf());
        const from = frameOf(target.prev);
        await session.moveTo(from.left + from.width / 2, from.top + from.height / 2);
        cursor.setSelection([from]);
        session.hide(old);
        const start = { x: target.prev.x, y: target.prev.y, w: target.prev.w, h: target.prev.h };
        const end = { x: target.element.x, y: target.element.y, w: target.element.w, h: target.element.h };
        const card = buildCard(layer, { ...target.element, ...start, ...(hasText(target.element) ? { text: textOf(target.prev) } : {}) });
        await session.gesture(MOVE_MS, (k) => {
          const at = { x: start.x + (end.x - start.x) * k, y: start.y + (end.y - start.y) * k, w: start.w + (end.w - start.w) * k, h: start.h + (end.h - start.h) * k };
          api.positionElementDom(card, at, size);
          const frame = frameOf(at);
          cursor.setSelection([frame]);
          cursor.place(frame.left + frame.width / 2, frame.top + frame.height / 2);
        });
        if (session.stopped()) return { typed: 0 };
        return typeObjectText(card, target);
      }

      async function removeObject(old, target) {
        const frame = frameOf(target.element);
        await session.moveTo(frame.left + frame.width / 2, frame.top + frame.height / 2);
        cursor.setSelection([frame]);
        await session.hold(HOLD_MS);
        if (session.stopped()) return { typed: 0 };
        session.hide(old);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      async function placeObject(target) {
        const layer = byId("slide-canvas-layer");
        if (!layer || !byId("slide-stage-box") || !codec()?.buildElementDom) return { typed: target.text.length };
        if (!showSlide(target)) return { typed: target.text.length };
        const old = target.change === "add" ? null : cardOf(layer, target.element.id);
        let result = { typed: 0 };
        if (target.change === "remove") {
          if (old) result = await removeObject(old, target);
        } else {
          result = old ? await reshapeObject(layer, old, target) : await drawObject(layer, target);
        }
        cursor.setSelection([]);
        return result;
      }

      return {
        "slide-add": addSlide,
        "slide-title": writeZone,
        "slide-body": writeZone,
        "slide-notes": writeNotes,
        "slide-format": setFormat,
        "slide-object": placeObject,
      };
    }

    return { collect, cut, createDrivers };
  }

  host.__EASYOFFICE_COPILOT_PERFORM_SLIDE__ = Object.freeze({ createSlidePerformance });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
