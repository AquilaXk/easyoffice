(function (host) {
  "use strict";

  // 텍스트 줄 바꿈, 자르기 and 회전 of the picture the word processor window has selected (PD-395): the commands of the 서식 menu and
  // the two handles of the selection frame (the round one above it turns the picture, the four on the sides cut it while 자르기 is
  // on). The three values are the picture's `data-eo-image-wrap`, `data-eo-image-crop` and `data-eo-image-rotation`
  // (`doc-word-model.js`), which the save payload sends and the engine keeps as DOCX `wp:anchor`, `a:srcRect` and `a:xfrm rot`.
  // Reuse (구현 원칙 3단계): (1) a DOCX renderer library maps the same three elements to CSS float, `object-view-box`/clip and
  // `transform: rotate`, which is what the visual below does; its code is not taken (it renders files, the editor edits a DOM);
  // (2) the selection frame, the undo wrapper and the dialog-free commands of the editor are used; (3) the handle drags are new.

  const M = () => host.__EASYOFFICE_DOC_WORD_MODEL__;
  const api = () => host.__EASYOFFICE_DOC_IMAGE_API__;
  const ROTATE_SNAP_DEGREES = 15;
  const WRAP_LABELS = { inline: "줄 안", square: "정사각형", topBottom: "위/아래", behind: "텍스트 뒤", front: "텍스트 앞" };

  const feedback = (document, text) => {
    const el = document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  // The picture takes `layout`: its data, its class and the two style values.
  function applyLayout(img, layout) {
    for (const key of ["eoImageWrap", "eoImageCrop", "eoImageRotation"]) delete img.dataset[key];
    Object.assign(img.dataset, M().imageData(layout));
    const visual = M().imageVisual(layout);
    img.className = visual.className;
    img.style.transform = visual.transform;
    img.style.clipPath = visual.clipPath;
  }

  const selectedPicture = (env) => {
    const img = api()?.selected?.();
    if (img && env.editor.contains(img)) return img;
    feedback(env.document, "그림을 먼저 선택하세요");
    return null;
  };

  // One undo step: the picture's layout changes by `change(layout)` (a copy it may edit).
  function change(env, change, message) {
    const img = selectedPicture(env);
    if (!img) return false;
    const layout = M().imageLayoutOf(img);
    change(layout);
    env.runs.withUndo(() => {
      applyLayout(img, layout);
      return { changed: true };
    });
    api().syncResizer?.();
    feedback(env.document, message);
    return true;
  }

  const normalize = (degrees) => ((Math.round(degrees) % 360) + 360) % 360;

  // ---- the handles of the selection frame

  function dragRotation(document, img, startEvent) {
    const before = M().imageLayoutOf(img);
    const rect = img.getBoundingClientRect?.() ?? { left: 0, top: 0, width: 0, height: 0 };
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    let latest = before;
    const onMove = (ev) => {
      let degrees = (Math.atan2(Number(ev.clientX) - cx, cy - Number(ev.clientY)) * 180) / Math.PI;
      if (ev.shiftKey) degrees = Math.round(degrees / ROTATE_SNAP_DEGREES) * ROTATE_SNAP_DEGREES;
      latest = { ...before, rotation: normalize(degrees) };
      applyLayout(img, latest);
      api().syncResizer?.();
    };
    finishDrag(document, img, before, () => latest, onMove, startEvent, "그림 회전");
  }

  function dragCrop(document, img, dir, startEvent) {
    const before = M().imageLayoutOf(img);
    const width = Math.max(1, Number.parseFloat(img.style.width) || img.getBoundingClientRect?.().width || 1);
    const height = Math.max(1, Number.parseFloat(img.style.height) || img.getBoundingClientRect?.().height || 1);
    const x0 = Number(startEvent.clientX) || 0;
    const y0 = Number(startEvent.clientY) || 0;
    const limit = M().MAX_CROP_SIDE;
    let latest = before;
    const onMove = (ev) => {
      const dx = ((Number(ev.clientX) - x0) / width) * 100000;
      const dy = ((Number(ev.clientY) - y0) / height) * 100000;
      const crop = { ...before.crop };
      if (dir === "w") crop.left = before.crop.left + dx;
      if (dir === "e") crop.right = before.crop.right - dx;
      if (dir === "n") crop.top = before.crop.top + dy;
      if (dir === "s") crop.bottom = before.crop.bottom - dy;
      for (const side of ["left", "top", "right", "bottom"]) crop[side] = Math.max(0, Math.min(limit, Math.round(crop[side])));
      if (crop.left + crop.right > limit) crop.left = limit - crop.right;
      if (crop.top + crop.bottom > limit) crop.top = limit - crop.bottom;
      latest = { ...before, crop };
      applyLayout(img, latest);
    };
    finishDrag(document, img, before, () => latest, onMove, startEvent, "그림 자르기");
  }

  // Follows the pointer until it is released, then makes the whole drag one undo step (the picture goes back to what it was
  // and the final layout is applied inside the undo wrapper).
  function finishDrag(document, img, before, latest, onMove, startEvent, message) {
    startEvent.preventDefault?.();
    startEvent.stopPropagation?.();
    const onUp = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
      const after = latest();
      if (JSON.stringify(after) === JSON.stringify(before)) return;
      applyLayout(img, before);
      api().withUndo?.(() => {
        applyLayout(img, after);
        return { changed: true };
      });
      api().syncResizer?.();
      feedback(document, message + " 완료");
    };
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }

  // `doc-image.js` binds the handles of the selection frame to this (`handle` is the span that was pressed).
  function onHandleDown(document, handle, ev) {
    const img = api()?.selected?.();
    if (!img) return;
    if (handle.dataset.rotateHandle !== undefined) dragRotation(document, img, ev);
    else if (document.getElementById("doc-image-resizer")?.classList.contains("is-cropping")) dragCrop(document, img, handle.dataset.cropDir, ev);
  }

  // ---- the commands

  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  for (const wrap of ["inline", "square", "topBottom", "behind", "front"]) {
    commands?.register("image-wrap-" + wrap, (env) => change(env, (layout) => (layout.wrap = wrap), "텍스트 줄 바꿈: " + WRAP_LABELS[wrap]));
  }
  commands?.register("image-rotate-right", (env) => change(env, (layout) => (layout.rotation = normalize(layout.rotation + 90)), "오른쪽으로 90도 회전"));
  commands?.register("image-rotate-left", (env) => change(env, (layout) => (layout.rotation = normalize(layout.rotation - 90)), "왼쪽으로 90도 회전"));
  commands?.register("image-crop", (env) => {
    if (!selectedPicture(env)) return;
    const frame = env.document.getElementById("doc-image-resizer");
    const on = !frame?.classList.contains("is-cropping");
    frame?.classList.toggle("is-cropping", on);
    feedback(env.document, on ? "자르기: 가장자리 핸들을 끌어 자릅니다" : "자르기를 마쳤습니다");
  });

  host.__EASYOFFICE_DOC_WORD_IMAGE__ = { applyLayout, onHandleDown, WRAP_LABELS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
