(function (host) {
  "use strict";

  // 서식 복사 and 서식 붙여넣기 of the slide editor (PD-398): Ctrl+Shift+C (⌘⇧C) takes the look of the selected text box,
  // shape or picture, Ctrl+Shift+V (⌘⇧V) puts it on the selected objects, and the 서식 복사 brush button keeps it until the
  // next object is clicked (Esc drops it), as in the word processor window (`doc-format-tools.js`, PD-395).
  // Reuse (구현 원칙 3단계): (1) a library for it would bring a schema the slide model does not have; (2) the canvas's
  // `updateElement` patch and the undo snapshot do the writing; (3) only the choice of properties is new: what a text box
  // carries (character and paragraph look), what any object carries (fill, line, dash, shadow).
  const TEXT_KINDS = new Set(["textbox", "shape"]);
  const TEXT_PROPS = ["bold", "italic", "underline", "fontSizePt", "color", "font", "eastFont", "align", "para"];
  const LOOK_PROPS = ["dash", "shadow"];
  const MSG_NOTHING = "서식을 복사할 개체를 먼저 선택하세요";
  const MSG_ARMED = "서식을 적용할 개체를 클릭하세요";

  const pick = (el, keys) => Object.fromEntries(keys.filter((k) => el[k] !== undefined).map((k) => [k, structuredClone(el[k])]));

  // The line of an object: a shape keeps it as `stroke`, a text box or picture as `outline`.
  function lineOf(el) {
    if (el.kind === "shape") return el.stroke ? { color: el.stroke, widthPt: el.strokeWidthPt } : null;
    return el.outline ? { color: el.outline.color, widthPt: el.outline.widthPt } : null;
  }

  // What a copied look puts on `target`: the properties the target can carry, and a cleared one for each the source lacked.
  function patchFor(look, target) {
    const patch = {};
    if (TEXT_KINDS.has(target.kind) && look.text) {
      for (const key of TEXT_PROPS) patch[key] = look.text[key];
      patch.fill = look.fill ?? "";
    }
    for (const key of LOOK_PROPS) patch[key] = look.fx[key];
    if (target.kind === "shape") {
      patch.stroke = look.line?.color ?? "";
      if (look.line) patch.strokeWidthPt = look.line.widthPt;
    } else {
      patch.outline = look.line ?? undefined;
    }
    return patch;
  }

  function createSlidePainter(deps, canvas) {
    const { document, pushUndoSnapshot } = deps;
    const say = (msg) => {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = msg;
    };
    const layer = () => document.getElementById("slide-canvas-layer");
    let copied = null;
    let armed = false;

    function copy() {
      const el = canvas.getSelectedElement();
      if (!el || !["textbox", "shape", "image"].includes(el.kind)) {
        say(MSG_NOTHING);
        return false;
      }
      copied = {
        text: TEXT_KINDS.has(el.kind) ? pick(el, TEXT_PROPS) : null,
        fill: TEXT_KINDS.has(el.kind) ? el.fill || "" : "",
        line: lineOf(el),
        fx: pick(el, LOOK_PROPS),
        sourceId: el.id,
      };
      return true;
    }

    function paste() {
      const targets = canvas.getElements().filter((e) => canvas.getSelectedIds().includes(e.id) && ["textbox", "shape", "image"].includes(e.kind) && e.id !== copied?.sourceId);
      if (!copied || targets.length === 0) return say(copied ? MSG_NOTHING : "복사한 서식이 없습니다");
      pushUndoSnapshot(false);
      for (const el of targets) canvas.updateElement(el.id, patchFor(copied, el), { recordUndo: false });
      return say("서식 복사 적용 완료");
    }

    function disarm() {
      if (!armed) return;
      armed = false;
      layer()?.removeEventListener("click", onClick);
      document.getElementById("btn-slide-format-painter")?.setAttribute("aria-pressed", "false");
    }

    // The click selects the object first (the canvas handles the press); the look goes on whatever is selected afterwards.
    function onClick() {
      if (!armed) return;
      if (canvas.getSelectedIds().some((id) => id !== copied?.sourceId)) {
        disarm();
        paste();
      }
    }

    // Esc drops the copied look (the studio hands every key to the tools first).
    function handleKeydown(ev) {
      if (!armed || ev.key !== "Escape") return false;
      ev.preventDefault?.();
      disarm();
      say("서식 복사를 취소했습니다");
      return true;
    }

    // 서식 복사: copies the look and waits for the next object (or Esc); a second use drops it.
    function painter() {
      if (armed) {
        disarm();
        return say("서식 복사를 취소했습니다");
      }
      if (!copy()) return null;
      armed = true;
      layer()?.addEventListener("click", onClick);
      document.getElementById("btn-slide-format-painter")?.setAttribute("aria-pressed", "true");
      return say(MSG_ARMED);
    }

    const ACTS = {
      "format-painter": painter,
      "format-copy": () => { if (copy()) say("서식을 복사했습니다"); },
      "format-paste": () => { disarm(); paste(); },
    };

    function run(act) {
      if (!Object.hasOwn(ACTS, act)) return false;
      ACTS[act]();
      return true;
    }

    function bind() {
      document.getElementById("btn-slide-format-painter")?.addEventListener("click", () => run("format-painter"));
    }

    return { bind, run, handleKeydown, closeAll: disarm, isArmed: () => armed, copy, paste };
  }

  host.__EASYOFFICE_CREATE_SLIDE_PAINTER__ = createSlidePainter;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
