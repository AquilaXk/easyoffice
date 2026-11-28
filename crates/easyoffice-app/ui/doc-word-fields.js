(function (host) {
  "use strict";

  // 날짜 및 시간, 책갈피, 상호 참조, 캡션 삽입 and 필드 업데이트 of the word processor window (PD-395). The fields and the bookmark
  // markers are the editor's own elements (`doc-word-model.js` keeps how they are stored and saved). The date and time
  // pictures are the Word program's (`yyyy`, `MM`, `dddd`, `AM/PM`, ...): the field instruction `DATE \@ "<picture>"` is what a
  // DOCX file keeps, and the text is what the picture shows now. Reuse (구현 원칙 3단계): (1) the picture syntax is the
  // field-code grammar of ECMA-376 17.16.4; no maintained library formats it, (2) the dialog kit, the undo wrapper and the
  // selection of `doc-runs-ui.js` are used as they are, (3) the formatter below is written for the tokens the list needs.

  const M = () => host.__EASYOFFICE_DOC_WORD_MODEL__;
  const DAYS = ["일요일", "월요일", "화요일", "수요일", "목요일", "금요일", "토요일"];
  const DATE_PICTURES = ["yyyy-MM-dd", "yyyy. M. d.", "yyyy'년' M'월' d'일'", "yyyy'년' M'월' d'일' dddd", "yy-MM-dd", "AM/PM h:mm", "HH:mm", "AM/PM h:mm:ss", "HH:mm:ss"];
  const DEFAULT_PICTURE = { DATE: "yyyy-MM-dd", TIME: "AM/PM h:mm" };
  const MAX_FIELDS = 5000;
  const MAX_SEQ_LABELS = 64;
  const MAX_REF_TEXT = 4096;
  const MSG_MISSING_BOOKMARK = "오류! 책갈피가 정의되어 있지 않습니다.";

  // ---- the date and time pictures

  const two = (n) => String(n).padStart(2, "0");

  // `picture` (`yyyy'년' M'월'`) shown for `date`: yyyy yy MMMM MM M dddd ddd dd d HH H hh h mm m ss s AM/PM; text in single
  // quotes is literal, any other character stands for itself.
  function formatPicture(picture, date) {
    let out = "";
    const src = String(picture);
    for (let i = 0; i < src.length; ) {
      const ch = src[i];
      if (ch === "'") {
        const end = src.indexOf("'", i + 1);
        out += src.slice(i + 1, end < 0 ? src.length : end);
        i = end < 0 ? src.length : end + 1;
        continue;
      }
      if (/^am\/pm/i.test(src.slice(i))) {
        out += date.getHours() < 12 ? "오전" : "오후";
        i += 5;
        continue;
      }
      let n = 1;
      while (src[i + n] === ch) n += 1;
      const h12 = date.getHours() % 12 || 12;
      const token = {
        y: n >= 3 ? String(date.getFullYear()) : two(date.getFullYear() % 100),
        M: n >= 4 ? date.getMonth() + 1 + "월" : n === 3 ? date.getMonth() + 1 + "월" : n === 2 ? two(date.getMonth() + 1) : String(date.getMonth() + 1),
        d: n >= 4 ? DAYS[date.getDay()] : n === 3 ? DAYS[date.getDay()].slice(0, 1) : n === 2 ? two(date.getDate()) : String(date.getDate()),
        H: n >= 2 ? two(date.getHours()) : String(date.getHours()),
        h: n >= 2 ? two(h12) : String(h12),
        m: n >= 2 ? two(date.getMinutes()) : String(date.getMinutes()),
        s: n >= 2 ? two(date.getSeconds()) : String(date.getSeconds()),
      }[ch];
      out += token === undefined ? ch.repeat(n) : token;
      i += n;
    }
    return out;
  }

  const pictureOf = (code) => /\\@\s*"([^"]*)"/.exec(code)?.[1];
  const verbOf = (code) => String(code).trim().split(/\s+/)[0].toUpperCase();

  // ---- reading the fields and bookmarks of the editor

  const fieldsOf = (root) => Array.from(root.querySelectorAll("[data-eo-field-code]")).slice(0, MAX_FIELDS);
  const markers = (root) => Array.from(root.querySelectorAll("[data-eo-bookmark-start], [data-eo-bookmark-end]"));

  const targetOf = (code) => String(code).trim().split(/\s+/)[1] ?? "";
  const isRefField = (node) => typeof node.dataset?.eoFieldCode === "string" && verbOf(node.dataset.eoFieldCode) === "REF";

  // The text between the markers of each bookmark in `wanted` (a Set of names), in document order, in one pass over `root`:
  // a name that is not there is not in the result. The result of another `REF` field is skipped (it would make nested
  // references grow with every level), each text stops at `MAX_REF_TEXT` characters, and the pass ends when every wanted
  // bookmark is closed.
  function bookmarkTexts(root, wanted) {
    const texts = new Map();
    const open = new Set();
    const filling = new Set();
    let remaining = wanted.size;
    const stack = [root];
    while (stack.length > 0 && remaining > 0) {
      const node = stack.pop();
      if (node.nodeType === 3) {
        const value = String(node.nodeValue || "");
        if (value) {
          for (const name of filling) {
            const next = (texts.get(name) + value).slice(0, MAX_REF_TEXT);
            texts.set(name, next);
            if (next.length >= MAX_REF_TEXT) filling.delete(name);
          }
        }
        continue;
      }
      if (node.nodeType !== 1 || isRefField(node)) continue;
      const start = node.dataset?.eoBookmarkStart;
      const end = node.dataset?.eoBookmarkEnd;
      if (start !== undefined && wanted.has(start) && !texts.has(start)) {
        texts.set(start, "");
        open.add(start);
        filling.add(start);
      }
      if (end !== undefined && open.delete(end)) {
        filling.delete(end);
        remaining -= 1;
      }
      const kids = node.childNodes || [];
      for (let i = kids.length - 1; i >= 0; i -= 1) stack.push(kids[i]);
    }
    return texts;
  }

  // The text between a bookmark's markers, in document order; null when the bookmark is not there.
  const bookmarkText = (root, name) => bookmarkTexts(root, new Set([name])).get(name) ?? null;

  // The result a field shows now: the date or time as its picture says, the next number of a `SEQ` label (`counters`).
  function fieldResult(code, counters, now) {
    const verb = verbOf(code);
    if (verb === "DATE" || verb === "TIME") return formatPicture(pictureOf(code) ?? DEFAULT_PICTURE[verb], now);
    const target = targetOf(code);
    if (!counters.has(target) && counters.size >= MAX_SEQ_LABELS) return null;
    counters.set(target, (counters.get(target) ?? 0) + 1);
    return String(counters.get(target));
  }

  // 필드 업데이트: every field of the editor shows its result again. Numbers first, then the references to the text that holds
  // them (read once for all `REF` fields).
  function updateFields(root, now = new Date()) {
    const counters = new Map();
    let changed = 0;
    const show = (field, result) => {
      if (result !== null && field.textContent !== result) {
        field.textContent = result;
        changed += 1;
      }
    };
    const fields = fieldsOf(root);
    const refs = [];
    for (const field of fields) {
      const code = field.dataset.eoFieldCode;
      const verb = verbOf(code);
      if (verb === "REF") refs.push(field);
      else if (verb === "DATE" || verb === "TIME" || verb === "SEQ") show(field, fieldResult(code, counters, now));
    }
    if (refs.length > 0) {
      const texts = bookmarkTexts(root, new Set(refs.map((field) => targetOf(field.dataset.eoFieldCode))));
      for (const field of refs) show(field, texts.get(targetOf(field.dataset.eoFieldCode)) ?? MSG_MISSING_BOOKMARK);
    }
    return changed;
  }

  // ---- inserting at the caret

  const inBody = (node) => {
    for (let cur = node; cur; cur = cur.parentNode) {
      const ds = cur.dataset;
      if (ds && (ds.eoHf !== undefined || ds.eoNotes !== undefined || ds.eoComments !== undefined)) return false;
    }
    return true;
  };

  // Puts `node` at the point (`container`, `offset`), splitting a text node in two.
  function insertAt(document, container, offset, node) {
    if (container.nodeType === 3) {
      const text = String(container.nodeValue ?? "");
      const parent = container.parentNode;
      const after = text.slice(offset);
      container.nodeValue = text.slice(0, offset);
      parent.insertBefore(node, container.nextSibling);
      if (after) parent.insertBefore(document.createTextNode(after), node.nextSibling);
      return;
    }
    container.insertBefore(node, container.childNodes[offset] ?? null);
  }

  function placeCaretAfter(document, node) {
    try {
      const sel = host.getSelection?.();
      const range = document.createRange?.();
      if (!sel || !range) return;
      range.setStartAfter(node);
      range.collapse(true);
      sel.removeAllRanges();
      sel.addRange(range);
    } catch {
      // The caret stays where the browser keeps it.
    }
  }

  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  function createField(document, code, text) {
    const span = document.createElement("span");
    span.className = M().FIELD_CLASS;
    span.dataset.eoFieldCode = code;
    span.setAttribute("contenteditable", "false");
    span.textContent = text;
    return span;
  }

  // The point the next insertion goes to: the end of the selection, inside the body.
  function caretPoint(env) {
    const range = env.runs.currentRange();
    if (!range || !env.editor.contains(range.endContainer) || !inBody(range.endContainer)) {
      feedback(env, "본문에서 삽입할 위치를 먼저 선택하세요");
      return null;
    }
    return { container: range.endContainer, offset: range.endOffset };
  }

  function insertField(env, code, text) {
    const point = caretPoint(env);
    if (!point) return null;
    const field = createField(env.document, code, text);
    env.runs.withUndo(() => {
      insertAt(env.document, point.container, point.offset, field);
      return { changed: true };
    });
    placeCaretAfter(env.document, field);
    return field;
  }

  // ---- 날짜 및 시간

  function openDateTime(env, kit) {
    const select = env.document.getElementById("doc-dt-format");
    const now = new Date();
    select.replaceChildren(
      ...DATE_PICTURES.map((picture) => {
        const option = env.document.createElement("option");
        option.value = picture;
        option.textContent = formatPicture(picture, now);
        return option;
      })
    );
    kit.setSelect("doc-dt-format", DATE_PICTURES[0]);
    kit.setChecked("doc-dt-update", false);
    kit.openDialog("doc-datetime-dialog");
  }

  function applyDateTime(env, kit) {
    const picture = kit.val("doc-dt-format") || DATE_PICTURES[0];
    const now = new Date();
    const text = formatPicture(picture, now);
    const timeOnly = !/[yMd]/.test(picture.replaceAll(/'[^']*'/g, ""));
    kit.closeDialog("doc-datetime-dialog");
    if (kit.checked("doc-dt-update")) {
      insertField(env, `${timeOnly ? "TIME" : "DATE"} \\@ "${picture}"`, text);
    } else {
      const point = caretPoint(env);
      if (!point) return;
      const node = env.document.createTextNode(text);
      env.runs.withUndo(() => {
        insertAt(env.document, point.container, point.offset, node);
        return { changed: true };
      });
      placeCaretAfter(env.document, node);
    }
    feedback(env, "날짜 및 시간 삽입 완료");
  }

  // ---- 책갈피

  const visibleBookmarks = (root) => {
    const starts = markers(root).filter((m) => m.dataset.eoBookmarkStart !== undefined);
    const ends = new Set(markers(root).map((m) => m.dataset.eoBookmarkEnd).filter(Boolean));
    return starts.map((m, position) => ({ name: m.dataset.eoBookmarkStart, position, marker: m })).filter((b) => ends.has(b.name) && !b.name.startsWith("_"));
  };

  function removeBookmark(root, name) {
    for (const m of markers(root)) if (m.dataset.eoBookmarkStart === name || m.dataset.eoBookmarkEnd === name) m.remove();
  }

  const marker = (document, kind, name) => {
    const span = document.createElement("span");
    span.className = "eo-bookmark-" + kind;
    if (kind === "start") span.dataset.eoBookmarkStart = name;
    else span.dataset.eoBookmarkEnd = name;
    span.setAttribute("contenteditable", "false");
    return span;
  };

  // The bookmark `name` over the selection: a bookmark of that name is moved. The end marker goes in first so the start point stays valid.
  function addBookmark(env, name) {
    if (!M().isBookmarkName(name)) return "책갈피 이름은 글자나 밑줄로 시작하는 40자 이하의 글자·숫자·밑줄이어야 합니다";
    const range = env.runs.currentRange();
    if (!range || range.collapsed || !env.editor.contains(range.startContainer) || !inBody(range.startContainer)) return "책갈피로 지정할 글을 먼저 선택하세요";
    env.runs.withUndo(() => {
      removeBookmark(env.editor, name);
      insertAt(env.document, range.endContainer, range.endOffset, marker(env.document, "end", name));
      insertAt(env.document, range.startContainer, range.startOffset, marker(env.document, "start", name));
      return { changed: true };
    });
    return "";
  }

  function goToBookmark(env, name) {
    const start = markers(env.editor).find((m) => m.dataset.eoBookmarkStart === name);
    if (!start) return;
    try {
      const sel = host.getSelection?.();
      const range = env.document.createRange?.();
      range?.setStartAfter(start);
      range?.collapse(true);
      sel?.removeAllRanges();
      if (range) sel?.addRange(range);
    } catch {
      // The marker still scrolls into view below.
    }
    start.scrollIntoView?.({ block: "center" });
  }

  function paintList(env, listId, items, selected, onPick) {
    const list = env.document.getElementById(listId);
    list.replaceChildren(
      ...items.map((item) => {
        const li = env.document.createElement("li");
        li.textContent = item.label;
        li.setAttribute("role", "option");
        li.setAttribute("aria-selected", item.value === selected ? "true" : "false");
        li.dataset.value = item.value;
        li.addEventListener("click", () => onPick(item.value));
        return li;
      })
    );
  }

  function bookmarkDialog(env, kit) {
    let picked = "";
    const name = env.document.getElementById("doc-bm-name");
    const status = (text) => kit.note("doc-bm-status", text);
    const refresh = () => {
      const byName = kit.val("doc-bm-sort") !== "position";
      const items = visibleBookmarks(env.editor).sort((a, b) => (byName ? a.name.localeCompare(b.name) : a.position - b.position));
      paintList(env, "doc-bm-list", items.map((b) => ({ label: b.name, value: b.name })), picked, (value) => {
        picked = value;
        name.value = value;
        refresh();
      });
    };
    return { refresh, status, name, pick: () => picked || name.value, reset: () => { picked = ""; } };
  }

  // ---- 상호 참조

  const headings = (root) => Array.from(root.querySelectorAll("h1, h2, h3, h4, h5, h6")).filter((h) => inBody(h) && String(h.textContent).trim());
  const captions = (root) => Array.from(root.querySelectorAll("[data-eo-caption]")).map((p) => ({ p, field: p.querySelector('[data-eo-field-code^="SEQ"]') })).filter((c) => c.field);
  const labelOfSeq = (field) => String(field.dataset.eoFieldCode).trim().split(/\s+/)[1] ?? "";
  const captionLabels = (root) => [...new Set(captions(root).map((c) => labelOfSeq(c.field)))];

  function xrefTypes(root) {
    return [{ value: "bookmark", label: "책갈피" }, { value: "heading", label: "제목" }, ...captionLabels(root).map((label) => ({ value: "caption:" + label, label }))];
  }

  function xrefItems(root, type) {
    if (type === "bookmark") return visibleBookmarks(root).map((b) => ({ value: b.name, label: b.name }));
    if (type === "heading") return headings(root).map((h, i) => ({ value: String(i), label: String(h.textContent).trim().slice(0, 80) }));
    const label = type.slice("caption:".length);
    return captions(root).filter((c) => labelOfSeq(c.field) === label).map((c, i) => ({ value: String(i), label: String(c.p.textContent).trim().slice(0, 80) }));
  }

  const xrefKinds = (type) => (type === "bookmark" ? ["책갈피 텍스트"] : type === "heading" ? ["제목 텍스트"] : ["레이블 및 번호 전체", "전체 캡션"]);

  // A hidden bookmark (`_Ref<number>`) over `first` to `last` of `parent`'s children, which the reference points at; one made before is reused.
  let refCounter = 0;
  function hiddenBookmark(env, parent, firstIndex, lastIndex) {
    const nodes = Array.from(parent.childNodes);
    const before = nodes[firstIndex - 1];
    const after = nodes[lastIndex + 1];
    const existing = before?.dataset?.eoBookmarkStart;
    if (existing?.startsWith("_Ref") && after?.dataset?.eoBookmarkEnd === existing) return existing;
    refCounter += 1;
    const name = "_Ref" + String((Date.now() % 1e8) * 10 + (refCounter % 10)).padStart(9, "0");
    const last = nodes[lastIndex];
    parent.insertBefore(marker(env.document, "end", name), last.nextSibling);
    parent.insertBefore(marker(env.document, "start", name), nodes[firstIndex]);
    return name;
  }

  // The bookmark a reference to `target` needs and the part of the target it covers.
  function referenceTarget(env, type, value, kind) {
    if (type === "bookmark") return value;
    const owner = type === "heading" ? headings(env.editor)[Number(value)] : captions(env.editor).filter((c) => labelOfSeq(c.field) === type.slice(8))[Number(value)]?.p;
    if (!owner || owner.childNodes.length === 0) return null;
    const children = Array.from(owner.childNodes);
    let last = children.length - 1;
    if (kind === 0 && type !== "heading") last = children.findIndex((n) => n.nodeType === 1 && n.dataset?.eoFieldCode?.startsWith("SEQ"));
    return last < 0 ? null : hiddenBookmark(env, owner, 0, last);
  }

  function insertReference(env, kit, type, value, kind) {
    const name = referenceTarget(env, type, value, kind);
    if (!name) return "참조할 항목을 선택하세요";
    const text = bookmarkText(env.editor, name) ?? MSG_MISSING_BOOKMARK;
    const field = insertField(env, `REF ${name}${kit.checked("doc-xref-link") ? " \\h" : ""}`, text);
    return field ? "" : "삽입할 위치를 먼저 선택하세요";
  }

  // ---- 캡션

  const nextNumber = (root, label) => captions(root).filter((c) => labelOfSeq(c.field) === label).length + 1;

  // The block a caption goes next to: the table or picture at the caret, else the paragraph the caret is in.
  function captionAnchor(env) {
    const range = env.runs.currentRange();
    const node = range?.startContainer;
    const element = node?.nodeType === 1 ? node : node?.parentNode;
    const table = element?.closest?.("table");
    if (table) return { block: table, label: "표" };
    const child = node?.nodeType === 1 ? node.childNodes?.[range.startOffset] : null;
    const image = element?.closest?.("img.doc-inline-image") ?? (child?.tagName === "IMG" ? child : null) ?? element?.querySelector?.("img.doc-inline-image");
    const block = image ? image.parentNode : element?.closest?.("p, h1, h2, h3, h4, h5, h6, li");
    return block && env.editor.contains(block) && inBody(block) ? { block, label: "그림" } : null;
  }

  function insertCaption(env, text, label, below) {
    const anchor = captionAnchor(env);
    if (!anchor) return false;
    const document = env.document;
    const paragraph = document.createElement("p");
    paragraph.className = "eo-caption";
    paragraph.dataset.eoCaption = "1";
    const number = String(nextNumber(env.editor, label));
    const lead = `${label} ${number}`;
    const rest = text.startsWith(lead) ? text.slice(lead.length) : text.startsWith(label) ? text.slice(label.length).replace(/^\s*\d*/, "") : text;
    paragraph.appendChild(document.createTextNode(label + " "));
    paragraph.appendChild(createField(document, `SEQ ${label} \\* ARABIC`, number));
    if (rest) paragraph.appendChild(document.createTextNode(rest));
    env.runs.withUndo(() => {
      anchor.block.parentNode.insertBefore(paragraph, below ? anchor.block.nextSibling : anchor.block);
      updateFields(env.editor);
      return { changed: true };
    });
    return true;
  }

  // ---- the commands

  function install(env0) {
    const document = env0.document;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const byId = (id) => document.getElementById(id);
    let env = env0;
    const bm = bookmarkDialog({ ...env0, get editor() { return env.editor; } }, kit);
    let xref = { type: "bookmark", value: "", kind: 0 };

    const paintXref = () => {
      kit.setSelect("doc-xref-kind", String(xref.kind));
      paintList(env, "doc-xref-list", xrefItems(env.editor, xref.type), xref.value, (value) => {
        xref.value = value;
        paintXref();
      });
    };

    byId("btn-doc-dt-ok")?.addEventListener("click", () => applyDateTime(env, kit));
    byId("btn-doc-bm-add")?.addEventListener("click", () => {
      const problem = addBookmark(env, bm.name.value.trim());
      bm.status(problem);
      if (!problem) kit.closeDialog("doc-bookmark-dialog");
    });
    byId("btn-doc-bm-delete")?.addEventListener("click", () => {
      const name = bm.pick();
      env.runs.withUndo(() => (name ? (removeBookmark(env.editor, name), { changed: true }) : { changed: false }));
      bm.reset();
      bm.name.value = "";
      bm.refresh();
    });
    byId("btn-doc-bm-go")?.addEventListener("click", () => {
      goToBookmark(env, bm.pick());
      kit.closeDialog("doc-bookmark-dialog");
    });
    byId("doc-bm-sort")?.addEventListener("change", bm.refresh);
    byId("doc-xref-type")?.addEventListener("change", () => {
      xref = { type: kit.val("doc-xref-type"), value: "", kind: 0 };
      const kinds = xrefKinds(xref.type);
      byId("doc-xref-kind").replaceChildren(...kinds.map((label, i) => Object.assign(document.createElement("option"), { value: String(i), textContent: label })));
      paintXref();
    });
    byId("doc-xref-kind")?.addEventListener("change", () => {
      xref.kind = Number(kit.val("doc-xref-kind")) || 0;
    });
    byId("btn-doc-xref-insert")?.addEventListener("click", () => {
      const problem = insertReference(env, kit, xref.type, xref.value, xref.kind);
      kit.note("doc-xref-status", problem);
      if (!problem) kit.closeDialog("doc-xref-dialog");
    });
    byId("btn-doc-cap-ok")?.addEventListener("click", () => {
      const label = kit.val("doc-cap-label");
      const ok = insertCaption(env, kit.val("doc-cap-text"), label, kit.val("doc-cap-pos") !== "above");
      if (ok) kit.closeDialog("doc-caption-dialog");
      else feedback(env, "캡션을 붙일 그림이나 표를 먼저 선택하세요");
    });
    byId("doc-cap-label")?.addEventListener("change", () => {
      const label = kit.val("doc-cap-label");
      kit.setVal("doc-cap-text", `${label} ${nextNumber(env.editor, label)}`);
    });

    const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
    commands.register("insert-datetime", (e) => ((env = e), openDateTime(e, kit)));
    commands.register("insert-bookmark", (e) => {
      env = e;
      bm.reset();
      bm.name.value = "";
      bm.status("");
      bm.refresh();
      kit.openDialog("doc-bookmark-dialog");
    });
    commands.register("insert-xref", (e) => {
      env = e;
      const types = xrefTypes(e.editor);
      byId("doc-xref-type").replaceChildren(...types.map((t) => Object.assign(document.createElement("option"), { value: t.value, textContent: t.label })));
      kit.setSelect("doc-xref-type", "bookmark");
      kit.setChecked("doc-xref-link", true);
      kit.note("doc-xref-status", "");
      kit.openDialog("doc-xref-dialog");
    });
    commands.register("insert-caption", (e) => {
      env = e;
      const anchor = captionAnchor(e);
      const label = anchor?.label ?? "그림";
      kit.setSelect("doc-cap-label", label);
      kit.setVal("doc-cap-text", `${label} ${nextNumber(e.editor, label)}`);
      kit.setSelect("doc-cap-pos", label === "표" ? "above" : "below");
      kit.openDialog("doc-caption-dialog");
    });
    commands.register("field-update", (e) => {
      const changed = e.runs.withUndo(() => ({ changed: updateFields(e.editor) > 0 })).changed;
      feedback(e, changed ? "필드 업데이트 완료" : "업데이트할 필드가 없습니다");
    });
  }

  // The dialogs bind on the first use of one of the commands (the editor window is then there).
  let installed = false;
  for (const cmd of ["insert-datetime", "insert-bookmark", "insert-xref", "insert-caption", "field-update"]) {
    host.__EASYOFFICE_DOC_WORD_COMMANDS__?.register(cmd, (env) => {
      if (!installed) {
        installed = true;
        install(env);
      }
      host.__EASYOFFICE_DOC_WORD_COMMANDS__.run(cmd, env);
    });
  }

  host.__EASYOFFICE_DOC_WORD_FIELDS__ = { DATE_PICTURES, formatPicture, updateFields, bookmarkText, visibleBookmarks, addBookmark, removeBookmark, insertCaption, install };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
