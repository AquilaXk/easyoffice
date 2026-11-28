(function (host) {
  "use strict";

  // Object animations of the slide editor (PD-394): the effects of the "애니메이션" menu (entrance,
  // emphasis, exit; start on click, with the previous or after the previous), and the "애니메이션 창"
  // dialog that lists them in order for changing the effect, the start, the side, the duration and
  // the delay, for moving them up and down and for removing them. The player is in slide-show-anim.js.
  const CLASS_LABELS = Object.freeze({ entrance: "나타내기", emphasis: "강조", exit: "끝내기" });
  const EFFECT_LABELS = Object.freeze({
    "entrance:appear": "나타내기", "entrance:fade": "페이드", "entrance:fly": "날아오기", "entrance:wipe": "닦아내기", "entrance:zoom": "확대",
    "emphasis:spin": "회전", "emphasis:growShrink": "확대/축소",
    "exit:appear": "사라지기", "exit:fade": "페이드", "exit:fly": "날아가기", "exit:wipe": "닦아내기", "exit:zoom": "축소",
  });
  const KIND_LABELS = Object.freeze({ textbox: "텍스트 상자", shape: "도형", image: "그림", table: "표", chart: "차트" });
  const START_LABELS = Object.freeze({ onClick: "클릭", withPrevious: "함께", afterPrevious: "다음" });
  const DIRECTED = new Set(["fly", "wipe"]);
  const DEFAULT_MS = 500;
  const DIALOG = "slide-anim-dialog";

  function createSlideAnim(deps, canvas) {
    const { document, slideDeckState: deck, getActiveSlideIndex, pushUndoSnapshot, noteActivity } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const groupApi = () => host.__EASYOFFICE_SLIDE_GROUP__;
    const $ = (id) => document.getElementById(id);
    const say = (msg) => {
      const fb = $("status-command-feedback");
      if (fb) fb.textContent = msg;
    };
    let working = [];
    let current = -1;
    let filling = false;

    const slide = () => deck[getActiveSlideIndex()];
    const effectKey = (a) => a.class + ":" + a.effect;

    function targetLabel(id) {
      const sl = slide();
      const el = (sl?.elements || []).find((e) => e.id === id);
      if (!el) return "그룹";
      const text = el.kind === "textbox" || el.kind === "shape" ? String(el.text || "").replace(/\s+/g, " ").trim().slice(0, 16) : "";
      return (KIND_LABELS[el.kind] || "개체") + (text ? ": " + text : "");
    }

    // Effects of the effect menu for what is selected: one per selected object or group.
    function addEffect(cls, effect) {
      const sl = slide();
      const ids = canvas.getSelectedIds();
      if (!sl || ids.length === 0) return say("개체를 먼저 선택하세요");
      const targets = [...new Set(ids.map((id) => groupApi().topOf(sl, id)))];
      pushUndoSnapshot(false);
      const added = targets.map((target) => ({ target, class: cls, effect, start: "onClick", durationMs: DEFAULT_MS, delayMs: 0, direction: "bottom" }));
      sl.anims = [...(sl.anims || []), ...added];
      noteActivity();
      return say("애니메이션 추가: " + EFFECT_LABELS[cls + ":" + effect]);
    }

    // ---- 애니메이션 창 ---------------------------------------------------------------------------

    function describe(a, i) {
      return `${i + 1}. ${targetLabel(a.target)} (${CLASS_LABELS[a.class]}: ${EFFECT_LABELS[effectKey(a)]}, ${START_LABELS[a.start]})`;
    }

    function renderList() {
      const list = $("slide-anim-list");
      if (!list) return;
      list.replaceChildren(...working.map((a, i) => {
        const li = document.createElement("li");
        li.dataset.index = String(i);
        li.setAttribute("role", "option");
        li.setAttribute("aria-selected", String(i === current));
        li.textContent = describe(a, i);
        return li;
      }));
      const a = working[current];
      for (const id of ["slide-anim-effect", "slide-anim-start", "slide-anim-direction", "slide-anim-duration", "slide-anim-delay", "btn-slide-anim-up", "btn-slide-anim-down", "btn-slide-anim-remove"]) {
        const el = $(id);
        if (el) el.disabled = !a;
      }
      if (!a) return;
      filling = true;
      kit.setSelect("slide-anim-effect", effectKey(a));
      kit.setSelect("slide-anim-start", a.start);
      kit.setSelect("slide-anim-direction", a.direction);
      kit.setVal("slide-anim-duration", String(a.durationMs / 1000));
      kit.setVal("slide-anim-delay", String(a.delayMs / 1000));
      filling = false;
      const direction = $("slide-anim-direction");
      if (direction) direction.disabled = !DIRECTED.has(a.effect);
    }

    function openPane() {
      const sl = slide();
      if (!sl) return;
      working = structuredClone(sl.anims || []);
      current = working.length > 0 ? 0 : -1;
      kit.note("slide-anim-status", "");
      renderList();
      kit.openDialog(DIALOG);
      $("btn-slide-anim-ok")?.focus?.();
    }

    // The edit controls write into the entry that is selected in the list.
    function readControls() {
      const a = working[current];
      if (!a || filling) return;
      const [cls, effect] = kit.val("slide-anim-effect").split(":");
      const ms = (id) => Math.round(Math.max(0, Math.min(60, Number(kit.val(id)) || 0)) * 1000);
      Object.assign(a, { class: cls, effect, start: kit.val("slide-anim-start"), direction: kit.val("slide-anim-direction") || "bottom", durationMs: ms("slide-anim-duration"), delayMs: ms("slide-anim-delay") });
      const li = $("slide-anim-list")?.children[current];
      if (li) li.textContent = describe(a, current);
      const direction = $("slide-anim-direction");
      if (direction) direction.disabled = !DIRECTED.has(a.effect);
    }

    function move(by) {
      const to = current + by;
      if (current < 0 || to < 0 || to >= working.length) return;
      [working[current], working[to]] = [working[to], working[current]];
      current = to;
      renderList();
    }

    function remove() {
      if (current < 0) return;
      working.splice(current, 1);
      current = Math.min(current, working.length - 1);
      renderList();
    }

    function commit() {
      const sl = slide();
      kit.closeDialog(DIALOG);
      if (!sl) return;
      pushUndoSnapshot(false);
      sl.anims = working.map((a) => ({ ...a }));
      noteActivity();
    }

    // ---- menu entry ------------------------------------------------------------------------------

    // Runs a menu act of this module (`anim-pane`, or `anim:<class>:<effect>`); false for any other act.
    function run(act) {
      if (act === "anim-pane") {
        openPane();
        return true;
      }
      const m = /^anim:(entrance|emphasis|exit):(\w+)$/.exec(act);
      if (!m || !Object.hasOwn(EFFECT_LABELS, m[1] + ":" + m[2])) return false;
      addEffect(m[1], m[2]);
      return true;
    }

    function bind() {
      $("slide-anim-list")?.addEventListener("click", (ev) => {
        const li = ev.target?.closest?.("li[data-index]");
        if (!li) return;
        current = Number(li.dataset.index);
        renderList();
      });
      for (const id of ["slide-anim-effect", "slide-anim-start", "slide-anim-direction", "slide-anim-duration", "slide-anim-delay"]) $(id)?.addEventListener("change", readControls);
      $("btn-slide-anim-up")?.addEventListener("click", () => move(-1));
      $("btn-slide-anim-down")?.addEventListener("click", () => move(1));
      $("btn-slide-anim-remove")?.addEventListener("click", remove);
      $("btn-slide-anim-ok")?.addEventListener("click", commit);
      $(DIALOG)?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => btn.addEventListener("click", () => kit.closeDialog(DIALOG)));
    }

    return { bind, run, addEffect, openPane, closeAll: () => kit.closeDialog(DIALOG) };
  }

  host.__EASYOFFICE_CREATE_SLIDE_ANIM__ = createSlideAnim;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
