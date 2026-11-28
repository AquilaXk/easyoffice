(function (host) {
  "use strict";

  // Paste into the document editor (`#doc-editor`). Reference implementation: `onPaste` of rhwp-studio
  // (https://github.com/edwardkim/rhwp, commit 1a76570e833917d15817415a53c09ad61ab3203f,
  // `rhwp-studio/src/engine/input-handler-keyboard.ts` lines 1934-2072, with `pasteExternalHtml` 2130-2170,
  // `pastePlainText` 419-438 and `pasteImageFile` 2173-2230, MIT, Copyright (c) 2025-2026 Edward Kim). The branches keep the
  // reference's order: the paste is taken over from the browser (`preventDefault`); in a header, footer or note only the plain
  // text is pasted (as one line); a picture file on the clipboard becomes a picture; HTML is cleaned (`doc-paste-sanitize.js`),
  // the pictures of an office or Hangul clipboard are filled in from its RTF (`doc-clipboard-images.js`), and the result goes
  // in (`doc-paste-model.js` reads it, this file puts it in); when the HTML is refused or has nothing in it, the plain text
  // goes in, one paragraph per line; the last branch is plain text.
  // The adapter differences: the reference keeps its own clipboard (`pasteInternal`) for what was copied inside the
  // editor, here the copy of the browser carries the runs as `span.eo-run` data attributes and takes the same path as any
  // HTML; there is no form mode and no object selection; the Hangul clipboard's document model (`[data-hwpjson]`) is not
  // read (that is the Hangul editor frame's); a selection is deleted and the paste is one undo step (`withUndo`).
  //
  // Where the content goes: the first block of the paste joins the block at the caret, the blocks in between follow it, and
  // the last block joins what was after the caret (`rhwp-studio` splits the paragraph at the caret the same way). Tables
  // and pictures come between the two halves. In a list the new paragraphs are items of that list; in a table cell a
  // pasted table is flattened to paragraphs.

  const TEXT_BLOCK = /^(?:P|H[1-6]|LI|BLOCKQUOTE|PRE|DIV|DD|DT|FIGCAPTION)$/;
  const CELL_TAGS = new Set(["TD", "TH"]);
  const LIST_TAGS = new Set(["UL", "OL"]);
  // What the browser copies to the half of a split block and must not be on both: the page break mark and the frame copy mark.
  const SPLIT_UNIQUE_DATA = ["eoBreak", "eoFrameCopy"];
  const MSG_TOO_LARGE = "붙여넣을 내용이 너무 커서 글자만 붙였습니다";
  const MSG_TEXT_TOO_LARGE = "붙여넣을 내용이 너무 큽니다";
  const MSG_NO_CONTENT = "붙일 클립보드 내용이 없습니다";
  const MSG_DONE = "붙여넣기 완료";
  const MSG_TEXT_ONLY_DONE = "텍스트만 유지하고 붙여넣기 완료";

  const tagOf = (el) => String(el?.tagName || "").toUpperCase();
  const isElement = (node) => node?.nodeType === 1;

  // The block that keeps the part of `block` after the point `(node, offset)`: the nodes after the point move into a clone of
  // each ancestor up to `block`, so a run that the point is inside is split in two runs of the same format.
  function splitUpTo(block, node, offset) {
    let parent = node;
    let next;
    if (node.nodeType === 3) {
      next = offset >= node.data.length ? node.nextSibling : offset <= 0 ? node : node.splitText(offset);
      parent = node.parentNode;
    } else {
      next = node.childNodes[offset] ?? null;
    }
    let right = null;
    for (;;) {
      const copy = parent.cloneNode(false);
      if (right) copy.appendChild(right);
      while (next) {
        const following = next.nextSibling;
        copy.appendChild(next);
        next = following;
      }
      right = copy;
      if (parent === block) break;
      next = parent.nextSibling;
      parent = parent.parentNode;
    }
    return right;
  }

  // A block with no text, or with only the line break that keeps an empty block open.
  const isEmptyBlock = (block) => !Array.from(block.childNodes).some((n) => (n.nodeType === 3 ? n.data !== "" : tagOf(n) !== "BR" || n !== block.lastChild));

  function createDocPaste(deps) {
    const { document, env, withUndo, setFeedback, getActiveModule } = deps;
    const R = () => host.__EASYOFFICE_DOC_RUNS__;
    const grid = () => host.__EASYOFFICE_DOC_TABLE_GRID__;
    const editor = () => document.getElementById("doc-editor");
    const isDocModule = () => ["DOC", "HWP"].includes(getActiveModule());

    // ------------------------------------------------------------ building nodes

    function runNode(run) {
      if (run.br) return document.createElement("br");
      const text = document.createTextNode(run.text);
      if (!run.fmt || Object.keys(run.fmt).length === 0) return text;
      const span = R().createRun(document, run.fmt);
      span.appendChild(text);
      return span;
    }

    // The inline nodes of runs; neighbours with the same address share one link element.
    function inlineNodes(runs) {
      const nodes = [];
      let link = null;
      let href = null;
      for (const run of runs) {
        const node = runNode(run);
        if (!run.href) {
          link = null;
          href = null;
          nodes.push(node);
          continue;
        }
        if (link === null || href !== run.href) {
          link = document.createElement("a");
          link.setAttribute("href", run.href);
          link.dataset.eoLink = "1";
          href = run.href;
          nodes.push(link);
        }
        link.appendChild(node);
      }
      return nodes;
    }

    function appendInline(parent, runs) {
      for (const node of inlineNodes(runs)) parent.appendChild(node);
    }

    function styleBlock(el, item) {
      if (item.align) el.style.textAlign = item.align;
      return el;
    }

    function listElement(item) {
      const list = document.createElement(item.list === "ol" ? "ol" : "ul");
      if (item.list === "task") {
        list.className = "eo-task-list";
        list.dataset.eoList = "task";
      }
      return list;
    }

    function listItemElement(item) {
      const li = styleBlock(document.createElement("li"), item);
      if (item.level > 0) li.dataset.eoListLevel = String(item.level);
      if (item.list === "ol" && item.listNumber) li.dataset.eoListNum = item.listNumber;
      if (item.list === "task") li.dataset.eoChecked = item.checked ? "true" : "false";
      appendInline(li, item.runs);
      if (item.runs.length === 0) li.appendChild(document.createElement("br"));
      return li;
    }

    function plainBlockElement(item) {
      const tag = /^h[1-6]$/.test(item.tag) ? item.tag : "p";
      const el = styleBlock(document.createElement(tag), item);
      appendInline(el, item.runs);
      if (item.runs.length === 0) el.appendChild(document.createElement("br"));
      return el;
    }

    function cellNodes(blocks, place) {
      const nodes = blocksToNodes(flattenTables(blocks), place);
      if (nodes.length === 1 && tagOf(nodes[0]) === "P" && !nodes[0].getAttribute("style")) return Array.from(nodes[0].childNodes);
      return nodes.length === 0 ? [document.createElement("br")] : nodes;
    }

    function tableElement(item) {
      const table = document.createElement("table");
      table.className = "doc-inline-table";
      const body = document.createElement("tbody");
      table.appendChild(body);
      for (const row of item.rows) {
        const tr = document.createElement("tr");
        for (const cellItem of row) {
          const td = document.createElement("td");
          if (cellItem.colSpan > 1) td.colSpan = cellItem.colSpan;
          if (cellItem.rowSpan > 1) td.rowSpan = cellItem.rowSpan;
          for (const node of cellNodes(cellItem.blocks, { table, cell: td })) td.appendChild(node);
          tr.appendChild(td);
        }
        body.appendChild(tr);
      }
      return table;
    }

    function imageElement(item, place) {
      const made = host.__EASYOFFICE_DOC_IMAGE_API__?.createImageElement(item.bytes, item.hint, place);
      if (made?.error) setFeedback?.(made.error);
      return made?.img ?? null;
    }

    const sameList = (el, item) => tagOf(el) === (item.list === "ol" ? "OL" : "UL") && (el.dataset.eoList === "task") === (item.list === "task");

    // The block-level nodes of blocks, consecutive list items in one list.
    function blocksToNodes(blocks, place) {
      const nodes = [];
      let list = null;
      for (const item of blocks) {
        if (item.type === "text" && item.list) {
          if (!list || !sameList(list, item)) {
            list = listElement(item);
            nodes.push(list);
          }
          list.appendChild(listItemElement(item));
          continue;
        }
        list = null;
        const node = item.type === "text" ? plainBlockElement(item) : item.type === "image" ? imageElement(item, place) : tableElement(item);
        if (node) nodes.push(node);
      }
      return nodes;
    }

    // ------------------------------------------------------------ the place of the caret

    // The children of the editor that are not the body: the frames of header and footer, the notes, the footer layer.
    const isFrame = (el) => host.__EASYOFFICE_DOC_PAGE_META__.isTrailingMetaNode(el);

    // The frame that holds the node, if any.
    const zoneOf = (root, node) => {
      for (let el = isElement(node) ? node : node?.parentNode; el && el !== root; el = el.parentNode) if (isFrame(el)) return el;
      return null;
    };

    // The leaf block that holds the point, made when the point is between blocks. Returns `{ block, node, offset }`.
    function leafBlockAt(root, node, offset) {
      let block = null;
      for (let el = isElement(node) ? node : node.parentNode; el && el !== root; el = el.parentNode) {
        if (TEXT_BLOCK.test(tagOf(el)) || CELL_TAGS.has(tagOf(el))) {
          block = el;
          break;
        }
      }
      const holder = isElement(node) ? node : node.parentNode;
      const holdsBlocks = block && Array.from(block.childNodes).some((n) => isElement(n) && TEXT_BLOCK.test(tagOf(n)) && tagOf(n) !== "DIV");
      if (block && !holdsBlocks) return { block, node, offset };
      // A point between blocks (the editor itself, a cell that holds paragraphs): an empty paragraph there. Anywhere else (a
      // table row, a list) the paste goes after the last block of the body.
      if (holder !== root && !CELL_TAGS.has(tagOf(holder))) {
        const end = appendPoint(root);
        return end.node === root ? leafBlockAt(root, root, end.offset) : leafBlockAt(root, end.node, end.offset);
      }
      const fresh = emptyParagraph();
      const ref = node.nodeType === 3 ? node : holder.childNodes[offset] ?? null;
      holder.insertBefore(fresh, ref && ref.parentNode === holder ? ref : null);
      if (node.nodeType === 3) {
        fresh.insertBefore(node, null);
        fresh.removeChild(fresh.firstChild);
        return { block: fresh, node, offset };
      }
      return { block: fresh, node: fresh, offset: 0 };
    }

    function endPoint(block) {
      const last = block.lastChild;
      return { node: block, offset: tagOf(last) === "BR" ? block.childNodes.length - 1 : block.childNodes.length };
    }

    function placeCaretAt(node, offset) {
      const sel = env.getSelection?.();
      const range = document.createRange?.();
      if (!node || !sel || !range) return;
      range.setStart(node, offset);
      range.setEnd(node, offset);
      sel.removeAllRanges();
      sel.addRange(range);
    }

    const TABLE_STRUCTURE = new Set(["TABLE", "THEAD", "TBODY", "TFOOT", "TR"]);

    // The table whose rows a boundary sits on (not a boundary inside a cell), or null.
    function structureTable(root, node) {
      if (!isElement(node) || !TABLE_STRUCTURE.has(tagOf(node))) return null;
      let table = node;
      for (let el = node.parentNode; el && el !== root; el = el.parentNode) if (tagOf(el) === "TABLE") table = el;
      return tagOf(table) === "TABLE" ? table : null;
    }

    const indexIn = (node) => Array.from(node.parentNode.childNodes).indexOf(node);

    // The boundaries of the range pulled to places that delete whole blocks, never part of the frame around the body or of a
    // table's rows. A boundary on the editor itself (a select-all) goes to the first and last block of the body, a boundary
    // on the rows of a table to just before or after that table: the table is deleted whole or not at all. False when the
    // range is not a text selection (it lies in the rows of one table).
    function clampToBody(root, range) {
      const kids = Array.from(root.childNodes);
      if (kids.every(isFrame)) return false;
      const endContainer = range.endContainer ?? range.startContainer;
      if (range.startContainer === root) {
        const first = kids.slice(range.startOffset ?? 0).find((n) => !isFrame(n));
        if (first) range.setStart(root, kids.indexOf(first));
      }
      if (endContainer === root) {
        const last = kids.slice(0, range.endOffset ?? 0).findLast((n) => !isFrame(n));
        if (last) range.setEnd(root, kids.indexOf(last) + 1);
      }
      const from = structureTable(root, range.startContainer);
      const to = structureTable(root, range.endContainer ?? range.startContainer);
      if (from && from === to) return false;
      if (from) range.setStart(from.parentNode, indexIn(from));
      if (to) range.setEnd(to.parentNode, indexIn(to) + 1);
      return true;
    }

    // A selection is deleted first, as the reference does before it inserts. A selection over several cells, from outside a
    // table into it, or into a frame keeps its content (nothing is deleted) and the paste goes in at its start. Returns that
    // start, or null when the selection could not be used. The boundary of a range that was deleted moves to the parent of the
    // first block when the ends are in two blocks, so the point is taken before the deletion (the node it names is cut, never
    // removed; a boundary on the editor itself names a place between blocks, which stays).
    function deleteSelection(root, range) {
      if (!clampToBody(root, range)) return { start: { node: range.startContainer, offset: range.startOffset ?? 0 }, deleted: false };
      const endNode = range.endContainer ?? range.startContainer;
      const start = { node: range.startContainer, offset: range.startOffset ?? 0 };
      if (closestCell(root, range.startContainer) !== closestCell(root, endNode) || zoneOf(root, range.startContainer) !== zoneOf(root, endNode)) return { start, deleted: false };
      const startBlock = leafBlockOf(root, range.startContainer);
      const endBlock = leafBlockOf(root, endNode);
      range.deleteContents();
      if (startBlock && endBlock && startBlock !== endBlock && endBlock.parentNode && !startBlock.contains(endBlock) && !endBlock.contains(startBlock)) {
        while (endBlock.firstChild) startBlock.appendChild(endBlock.firstChild);
        endBlock.parentNode.removeChild(endBlock);
      }
      return { start, deleted: true };
    }

    const closestCell = (root, node) => {
      for (let el = isElement(node) ? node : node?.parentNode; el && el !== root; el = el.parentNode) if (CELL_TAGS.has(tagOf(el))) return el;
      return null;
    };
    const leafBlockOf = (root, node) => {
      for (let el = isElement(node) ? node : node?.parentNode; el && el !== root; el = el.parentNode) if (TEXT_BLOCK.test(tagOf(el))) return el;
      return null;
    };

    // ------------------------------------------------------------ putting the model in

    function insertAfter(anchor, node) {
      anchor.parentNode.insertBefore(node, anchor.nextSibling);
      return node;
    }

    function cleanTail(tail) {
      tail.removeAttribute("id");
      for (const key of SPLIT_UNIQUE_DATA) delete tail.dataset[key];
      return tail;
    }

    const runChars = (runs) => (runs ?? []).reduce((sum, run) => sum + (run.text?.length ?? 0), 0);
    const flattenTables = (blocks) => blocks.flatMap((item) => (item.type === "table" ? flattenTables(item.rows.flat().flatMap((cell) => cell.blocks)) : [item]));

    // The point after `chars` characters of text inside `holder`, found again after neighbouring text nodes were merged.
    function caretIn(root, holder, chars) {
      const hit = R().pointAtTextOffset(root, chars, false, undefined, holder);
      return hit.node === root ? { node: holder, offset: 0 } : hit;
    }

    // A paste into one line: header, footer, notes.
    function insertInline(root, block, node, offset, blocks) {
      const runs = [];
      for (const item of blocks) {
        if (item.type !== "text") continue;
        if (runs.length > 0 && item.runs.length > 0) runs.push({ text: " ", fmt: {}, href: null });
        runs.push(...item.runs.filter((run) => !run.br));
      }
      if (runs.length === 0) return null;
      const tail = cleanTail(splitUpTo(block, node, offset));
      const headChars = block.textContent.length;
      appendInline(block, runs);
      while (tail.firstChild) block.appendChild(tail.firstChild);
      R().normalizeContainer(block);
      return caretIn(root, block, headChars + runChars(runs));
    }

    // A table cell's own text: the paragraphs of the paste are lines of that text, and pictures follow it.
    function insertIntoCellText(root, located, items) {
      const { block } = located;
      const place = tablePlace(root, block);
      const lines = items[0].runs;
      if (isEmptyBlock(block)) block.replaceChildren();
      const tail = splitUpTo(block, located.node, located.offset);
      const headChars = block.textContent.length;
      appendInline(block, lines);
      let sawPicture = false;
      for (const item of items.slice(1)) {
        const img = imageElement(item, place);
        if (img) block.appendChild(img);
        sawPicture = sawPicture || Boolean(img);
      }
      while (tail.firstChild) block.appendChild(tail.firstChild);
      if (!block.firstChild) block.appendChild(document.createElement("br"));
      R().normalizeContainer(block);
      return sawPicture ? endPoint(block) : caretIn(root, block, headChars + runChars(lines));
    }

    // A paste into a list item: text becomes items of the list; a table or picture ends the list there and the rest of
    // the list follows it. Returns where the pasted nodes end and the list that holds what came after, if the list ended.
    function placeInList(block, items, place) {
      let list = block.parentNode;
      let anchor = block;
      let rest = null;
      for (const item of items) {
        let node;
        if (item.type === "text") {
          node = list ? listItemElement({ ...item, list: null, level: 0 }) : plainBlockElement(item);
        } else {
          node = item.type === "image" ? imageElement(item, place) : tableElement(item);
          if (node && list) {
            rest = list.cloneNode(false);
            while (anchor.nextSibling) rest.appendChild(anchor.nextSibling);
            anchor = list;
            list = null;
          }
        }
        if (node) anchor = insertAfter(anchor, node);
      }
      return { anchor, rest };
    }

    // The part after the caret goes behind what was put in, or back into the block when the paste was inline only.
    function attachTail(block, tail, anchor, rest, spread, needsTail) {
      if (!spread) {
        while (tail.firstChild) block.appendChild(tail.firstChild);
        return;
      }
      const keep = !isEmptyBlock(tail);
      if (rest) {
        if (keep) rest.insertBefore(tail, rest.firstChild);
        if (rest.firstChild) insertAfter(anchor, rest);
        else if (!keep) insertAfter(anchor, emptyParagraph());
      } else if (keep) {
        insertAfter(anchor, tail);
      } else if (needsTail && tagOf(block) !== "LI") {
        // A table or a picture cannot end the page: an empty paragraph follows it.
        tail.replaceChildren(document.createElement("br"));
        insertAfter(anchor, tail);
      }
    }

    // The paragraph the caret is in when it is empty: the heading level and alignment of the pasted first block.
    function adoptBlockLook(block, lead) {
      const heading = /^h[1-6]$/.test(lead.tag) ? document.createElement(lead.tag) : null;
      const target = heading ?? block;
      if (heading) {
        for (const key of Object.keys(block.dataset)) heading.dataset[key] = block.dataset[key];
        block.parentNode.insertBefore(heading, block);
        block.parentNode.removeChild(block);
      }
      if (lead.align && !target.style.textAlign) target.style.textAlign = lead.align;
      return target;
    }

    function emptyParagraph() {
      const p = document.createElement("p");
      p.appendChild(document.createElement("br"));
      return p;
    }

    // Everything of the model into the block at the point. Returns the place for the caret.
    function insertBlocks(root, blocks, located) {
      let { block } = located;
      const place = tablePlace(root, block);
      const items = closestCell(root, block) ? flattenTables(blocks) : blocks.slice();
      if (CELL_TAGS.has(tagOf(block))) return insertIntoCellText(root, located, flattenForCell(items));
      const inList = tagOf(block) === "LI" && LIST_TAGS.has(tagOf(block.parentNode));
      const wasEmpty = isEmptyBlock(block);
      if (wasEmpty) block.replaceChildren();
      delete block.dataset.eoFrameCopy;
      const tail = cleanTail(splitUpTo(block, located.node, located.offset));
      // The first block of the paste joins the text before the caret, the last one the text after it. An empty paragraph
      // takes the heading level and the alignment of the first block instead (except list items, which keep their list).
      const lead = items[0]?.type === "text" && (inList || !items[0].list) ? items.shift() : null;
      if (lead && wasEmpty && tagOf(block) === "P") block = adoptBlockLook(block, lead);
      const headChars = block.textContent.length;
      if (lead) appendInline(block, lead.runs);
      const final = items.at(-1);
      const last = final?.type === "text" && (inList || (final.tag === "p" && !final.list)) ? items.pop() : null;
      let anchor = block;
      let rest = null;
      if (inList) ({ anchor, rest } = placeInList(block, items, place));
      else for (const node of blocksToNodes(items, place)) anchor = insertAfter(anchor, node);
      const spread = anchor !== block || last !== null;
      const needsTail = last !== null || tagOf(anchor) === "TABLE" || tagOf(anchor) === "IMG" || rest !== null;
      if (last) {
        if (isEmptyBlock(tail)) adoptBlockLook(tail, last);
        const first = tail.firstChild;
        for (const node of inlineNodes(last.runs)) tail.insertBefore(node, first);
      }
      attachTail(block, tail, anchor, rest, spread, needsTail);
      if (wasEmpty && !lead && (tagOf(anchor) === "UL" || tagOf(anchor) === "OL") && tagOf(block) === "P" && block.parentNode) {
        block.parentNode.removeChild(block);
      } else if (!block.firstChild) {
        block.appendChild(document.createElement("br"));
      }
      R().normalizeContainer(block);
      if (root.contains(tail)) R().normalizeContainer(tail);
      if (!spread) return caretIn(root, block, headChars + runChars(lead?.runs));
      if (last) return caretIn(root, tail, runChars(last.runs));
      if (root.contains(tail)) return { node: tail, offset: 0 };
      return endPoint(tagOf(anchor) === "UL" || tagOf(anchor) === "OL" ? anchor.lastChild : anchor);
    }

    function tablePlace(root, block) {
      const table = grid().tableAt(root, block);
      const cell = table?.classList?.contains("doc-inline-table") ? grid().cellIn(table, block, 0) : null;
      return cell ? { table, cell } : null;
    }

    // A paste into the text of a cell: the paragraphs of the paste become lines of that text, the pictures come after it.
    function flattenForCell(items) {
      const runs = [];
      const pictures = [];
      for (const item of items) {
        if (item.type === "image") pictures.push(item);
        else if (item.type === "text") {
          if (runs.length > 0) runs.push({ br: true });
          runs.push(...item.runs);
        }
      }
      return [{ type: "text", tag: "p", list: null, runs }, ...pictures];
    }

    // ------------------------------------------------------------ the events

    function activeEditorRange(root, fallbackRange) {
      const sel = env.getSelection?.();
      const liveRange = sel?.rangeCount > 0 ? sel.getRangeAt(0) : null;
      if (liveRange && root.contains(liveRange.startContainer) && root.contains(liveRange.endContainer ?? liveRange.startContainer)) {
        return { range: liveRange, fromLive: true };
      }
      if (fallbackRange && root.contains(fallbackRange.startContainer) && root.contains(fallbackRange.endContainer ?? fallbackRange.startContainer)) {
        return { range: fallbackRange, fromLive: false };
      }
      return { range: null, fromLive: false };
    }

    // The model into the editor as one undo step; false when there was nowhere to put it.
    function insertModel(blocks, fallbackRange = null) {
      const root = editor();
      if (!root) return false;
      const sel = env.getSelection?.();
      const { range, fromLive } = activeEditorRange(root, fallbackRange);
      const inside = Boolean(range);
      // Nothing to put in (a line break alone in a header) leaves the selection where it is.
      const zone = inside ? zoneOf(root, range.startContainer) : null;
      const insertable = zone ? blocks.some((item) => item.type === "text" && item.runs.some((run) => !run.br)) : blocks.length > 0;
      if (!insertable) {
        setFeedback?.(MSG_NO_CONTENT);
        return false;
      }
      let done = false;
      withUndo(() => {
        const cut = inside && !range.collapsed ? deleteSelection(root, range) : null;
        const liveAfter = inside && fromLive && sel?.rangeCount > 0 ? sel.getRangeAt(0) : range;
        const at = cut?.start ?? (liveAfter ? { node: liveAfter.startContainer, offset: liveAfter.startOffset ?? 0 } : appendPoint(root));
        const located = leafBlockAt(root, at.node, at.offset);
        const locZone = zoneOf(root, located.block);
        const caret = locZone ? insertInline(root, located.block, located.node, located.offset, blocks) : insertBlocks(root, blocks, located);
        if (caret) {
          placeCaretAt(caret.node, caret.offset);
          done = true;
        }
        return { changed: done || cut?.deleted === true };
      });
      return done;
    }

    // With no caret in the editor the paste goes after the last block of the body.
    function appendPoint(root) {
      const last = Array.from(root.childNodes).filter((n) => isElement(n) && TEXT_BLOCK.test(tagOf(n)) && !zoneOf(root, n)).at(-1);
      if (last) return { node: last, offset: last.childNodes.length };
      return { node: root, offset: root.childNodes.length };
    }

    async function pasteImageFile(file, fallbackRange = null) {
      const limits = host.__EASYOFFICE_DOC_PASTE_MODEL__?.LIMITS;
      if (typeof file?.size === "number" && limits && file.size > limits.MAX_IMAGE_BYTES) {
        setFeedback?.("그림을 넣지 못했습니다: 최대 32 MiB까지 넣을 수 있습니다");
        return false;
      }
      try {
        const raw = new Uint8Array(await file.arrayBuffer());
        const images = host.__EASYOFFICE_DOC_CLIPBOARD_IMAGES__;
        const norm = await images.normalizeClipboardImageBytes(raw, file.type || file.name || "", env);
        if (norm.error) {
          setFeedback?.(norm.error);
          return false;
        }
        if (!isDocModule() || !editor()) return false;
        const api = host.__EASYOFFICE_DOC_IMAGE_API__;
        const check = api?.validateImageBytes?.(norm.bytes);
        if (check?.error) {
          setFeedback?.(check.error);
          return false;
        }
        const ok = insertModel([{ type: "image", bytes: norm.bytes, hint: file.name || file.type || "" }], fallbackRange);
        if (ok) setFeedback?.(MSG_DONE);
        return ok;
      } catch {
        setFeedback?.("그림을 넣지 못했습니다");
        return false;
      }
    }

    // Ref `pastePlainText`. `doneMessage` is what the status line says when the text went in.
    function pasteText(text, doneMessage = MSG_DONE, fallbackRange = null) {
      const parsed = host.__EASYOFFICE_DOC_PASTE_MODEL__.parseText(text);
      if (parsed.error === "limit") {
        setFeedback?.(MSG_TEXT_TOO_LARGE);
        return false;
      }
      if (parsed.error) {
        setFeedback?.(MSG_NO_CONTENT);
        return false;
      }
      if (insertModel(parsed.blocks, fallbackRange)) {
        setFeedback?.(doneMessage);
        return true;
      }
      return false;
    }

    // Ref `pasteExternalHtml`: cleaned HTML, and the plain text when the HTML has nothing to paste or is refused.
    function pasteExternalHtml(html, text, fallbackRange = null) {
      const cleaned = host.__EASYOFFICE_DOC_PASTE_SANITIZE__.sanitizeOfficeHtml(html);
      const parsed = host.__EASYOFFICE_DOC_PASTE_MODEL__.parseHtml(cleaned, env);
      if (parsed.blocks) {
        if (insertModel(parsed.blocks, fallbackRange)) {
          setFeedback?.(MSG_DONE);
          return true;
        }
        return false;
      }
      if (text) return pasteText(text, parsed.error === "limit" ? MSG_TOO_LARGE : MSG_DONE, fallbackRange);
      if (parsed.error === "limit") setFeedback?.(MSG_TEXT_TOO_LARGE);
      return false;
    }

    async function pasteHtmlOrText(html, text, rtf, { fallbackRange = null, requireLiveCaret = false } = {}) {
      const limits = host.__EASYOFFICE_DOC_PASTE_MODEL__.LIMITS;
      if (html.length > limits.MAX_HTML_CHARS) {
        if (text) return pasteText(text, MSG_TOO_LARGE, fallbackRange);
        setFeedback?.(MSG_TEXT_TOO_LARGE);
        return false;
      }
      if (!html) return pasteText(text, MSG_DONE, fallbackRange);
      const images = host.__EASYOFFICE_DOC_CLIPBOARD_IMAGES__;
      if (!images.needsAsyncImagePrep(html, rtf)) {
        return pasteExternalHtml(html, text, fallbackRange);
      }
      // Prepare RTF/GIF/SVG pictures, then paste only if the document window is still active (and caret remains if event paste).
      let merged = html;
      try {
        merged = await images.prepareClipboardHtmlImages(html, rtf, env);
      } catch {
        // without converted pictures the rest of the HTML is still pasted
      }
      const root = editor();
      if (!isDocModule() || !root) return false;
      if (requireLiveCaret) {
        const now = env.getSelection?.();
        const caret = now?.rangeCount > 0 ? now.getRangeAt(0).startContainer : null;
        if (!caret || !root.contains(caret)) return false;
      }
      return pasteExternalHtml(merged, text, fallbackRange);
    }

    // Ref `onPaste`.
    function onPaste(event) {
      if (!isDocModule()) return;
      const root = editor();
      const data = event.clipboardData;
      if (!root || !data) return;
      event.preventDefault?.();
      const html = data.getData("text/html") || "";
      const text = data.getData("text/plain") || "";
      const rtf = data.getData("text/rtf") || "";
      const sel = env.getSelection?.();
      const caretNode = sel?.rangeCount > 0 ? sel.getRangeAt(0).startContainer : null;
      // A header, footer or note takes the plain text only.
      if (caretNode && root.contains(caretNode) && zoneOf(root, caretNode)) {
        if (text) pasteText(text);
        return;
      }
      // A picture file on the clipboard becomes a picture.
      for (const item of Array.from(data.items || [])) {
        if (item.kind === "file" && String(item.type).startsWith("image/")) {
          const file = item.getAsFile();
          if (file) {
            void pasteImageFile(file);
            return;
          }
        }
      }
      void pasteHtmlOrText(html, text, rtf, { requireLiveCaret: imagesNeedAsync(html, rtf) });
    }

    function imagesNeedAsync(html, rtf) {
      const limits = host.__EASYOFFICE_DOC_PASTE_MODEL__.LIMITS;
      return Boolean(html && html.length <= limits.MAX_HTML_CHARS && host.__EASYOFFICE_DOC_CLIPBOARD_IMAGES__?.needsAsyncImagePrep(html, rtf));
    }

    async function pasteFromClipboard(fallbackRange, fallbackReadText) {
      const root = editor();
      if (!root || !isDocModule()) return false;
      const { range } = activeEditorRange(root, fallbackRange);
      const inZone = Boolean(range && zoneOf(root, range.startContainer));
      const limits = host.__EASYOFFICE_DOC_PASTE_MODEL__.LIMITS;
      const images = host.__EASYOFFICE_DOC_CLIPBOARD_IMAGES__;
      const clip = env.navigator?.clipboard;
      if (clip && typeof clip.read === "function") {
        try {
          for (const item of await clip.read()) {
            const types = Array.from(item.types || []);
            const imgType = !inZone ? types.find((t) => String(t).startsWith("image/")) : null;
            if (imgType && !types.includes("text/html")) {
              return await pasteImageFile(await item.getType(imgType), range);
            }
            const htmlBlob = !inZone && types.includes("text/html") ? await item.getType("text/html") : null;
            const plainBlob = types.includes("text/plain") ? await item.getType("text/plain") : null;
            if (plainBlob && plainBlob.size > limits.MAX_TEXT_CHARS * 4) {
              setFeedback?.(MSG_TEXT_TOO_LARGE);
              return false;
            }
            const plain = plainBlob ? await plainBlob.text() : "";
            if (htmlBlob) {
              if (htmlBlob.size > limits.MAX_HTML_CHARS * 3) {
                if (plain) return pasteText(plain, MSG_TOO_LARGE, range);
                setFeedback?.(MSG_TEXT_TOO_LARGE);
                return false;
              }
              const html = await htmlBlob.text();
              const rtfBlob = types.includes("text/rtf") ? await item.getType("text/rtf") : null;
              const rtf = rtfBlob && rtfBlob.size <= images.LIMITS.MAX_RTF_CHARS ? await rtfBlob.text() : "";
              if (html || plain) return await pasteHtmlOrText(html, plain, rtf, { fallbackRange: range });
            }
            if (plain) return pasteText(plain, MSG_DONE, range);
          }
        } catch {
          // Permission denied or clipboard.read unavailable: fall back to text-only clipboard read.
        }
      }
      const text = typeof fallbackReadText === "function" ? await fallbackReadText() : "";
      if (!text) {
        setFeedback?.(MSG_NO_CONTENT);
        return false;
      }
      return pasteText(text, MSG_DONE, range);
    }

    // 붙여넣기 옵션 > 텍스트만 유지: the clipboard's plain text only, in the look of the place it goes to.
    async function pasteTextOnly(fallbackRange, fallbackReadText) {
      const root = editor();
      if (!root || !isDocModule()) return false;
      const { range } = activeEditorRange(root, fallbackRange);
      let text = "";
      try {
        text = (await env.navigator?.clipboard?.readText?.()) ?? "";
      } catch {
        // Permission denied: the caller's reader (the system clipboard of the shell) is the fallback.
      }
      if (!text && typeof fallbackReadText === "function") text = await fallbackReadText();
      if (!text) {
        setFeedback?.(MSG_NO_CONTENT);
        return false;
      }
      return pasteText(text, MSG_TEXT_ONLY_DONE, range);
    }

    function bind() {
      editor()?.addEventListener("paste", onPaste);
      host.__EASYOFFICE_DOC_WORD_COMMANDS__?.register("paste-text-only", () => void pasteTextOnly(null, null));
    }

    return { bind, onPaste, insertModel, pasteFromClipboard, pasteTextOnly };
  }

  host.__EASYOFFICE_CREATE_DOC_PASTE__ = createDocPaste;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
