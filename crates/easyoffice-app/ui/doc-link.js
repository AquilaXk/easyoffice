// Reference implementation: Univer (Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.), commit
// 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, `packages/docs-hyper-link-ui/src/commands/commands/`:
// `add-link.command.ts` 1-58 (wrap the selection in a link range that holds the address),
// `update-link.command.ts` 1-90 (replace the selected text by the label, keep the first run's style, same
// link id and a new address) and `delete-link.command.ts` 1-45 (drop the range, keep the text). The
// reference works on an index-based document model and a React popup; here the same three operations
// run on the editor's DOM, and the dialog follows the app's tool dialog frame. See THIRD_PARTY_NOTICES.md.
(function (host) {
  "use strict";

  // Hyperlinks of the document editor: add, update and delete, plus the address policy and the dialog
  // (#doc-link-dialog). A link is an `<a data-eo-link href>` that wraps the runs it covers, so the run
  // formatting stays on the runs inside it (`doc-runs.js` keeps a link as its own run scope).

  const LINK_DATA_KEY = "eoLink";
  // Addresses are plain web or mail targets (fail-closed): `javascript:`, `file:`, `data:` and the rest are refused.
  const ALLOWED_SCHEMES = ["http", "https", "mailto"];
  const MAX_URL_CHARS = 2048;
  const SCHEME_RE = /^([a-z][a-z0-9+.-]*):/i;
  const WEB_TARGET_RE = /^https?:\/\/[^\s/?#]+/i;
  const MAIL_TARGET_RE = /^mailto:[^\s?]+/i;
  // Whitespace and control characters (C0, DEL, C1) anywhere in an address.
  const UNSAFE_CHARS_RE = /[\u0000- \u007f-\u009f]/;
  const MSG_BAD_URL = "허용되지 않는 주소";
  const MSG_ONE_PARAGRAPH = "한 문단 안에서 선택";
  const MSG_OVERLAP = "다른 링크와 겹침";
  const MSG_NO_POSITION = "삽입 위치 없음";

  // The address as it is stored, or null when the policy refuses it.
  function normalizeUrl(raw) {
    const text = String(raw ?? "").trim();
    if (text === "" || text.length > MAX_URL_CHARS || UNSAFE_CHARS_RE.test(text)) return null;
    const scheme = SCHEME_RE.exec(text)?.[1].toLowerCase();
    if (!ALLOWED_SCHEMES.includes(scheme)) return null;
    return (scheme === "mailto" ? MAIL_TARGET_RE : WEB_TARGET_RE).test(text) ? text : null;
  }

  const isLink = (node) => node?.nodeType === 1 && String(node.tagName || "").toUpperCase() === "A" && Boolean(node.dataset?.[LINK_DATA_KEY]);

  // The link that holds `node`, inside `root`.
  function linkAt(root, node) {
    for (let cur = node; cur && cur !== root; cur = cur.parentNode) if (isLink(cur)) return cur;
    return null;
  }

  function createLink(document, url) {
    const a = document.createElement("a");
    a.setAttribute("href", url);
    a.dataset[LINK_DATA_KEY] = "1";
    return a;
  }

  function textNodesOf(el, out = []) {
    for (const child of Array.from(el.childNodes)) {
      if (child.nodeType === 3) out.push(child);
      else if (child.nodeType === 1) textNodesOf(child, out);
    }
    return out;
  }

  // Splits every inline ancestor of `node` below `container` so that `node` sits alone in a copy of each
  // (the copies keep the run formatting); returns the outermost one, a direct child of the container.
  function isolateInline(node, container) {
    let cur = node;
    while (cur.parentElement && cur.parentElement !== container) {
      const parent = cur.parentElement;
      const siblings = Array.from(parent.childNodes);
      const at = siblings.indexOf(cur);
      const split = (nodes, place) => {
        if (nodes.length === 0) return;
        const copy = parent.cloneNode(false);
        nodes.forEach((n) => copy.appendChild(n));
        parent.parentElement.insertBefore(copy, place);
      };
      split(siblings.slice(0, at), parent);
      split(siblings.slice(at + 1), parent.nextSibling ?? null);
      cur = parent;
    }
    return cur;
  }

  // The link shows `label`: the first text node keeps its run, the rest of the content goes.
  function relabel(a, label) {
    const [first] = textNodesOf(a);
    if (!first) {
      a.appendChild(a.ownerDocument.createTextNode(label));
      return;
    }
    first.data = label;
    let keep = first;
    while (keep.parentElement !== a) keep = keep.parentElement;
    Array.from(a.childNodes).filter((n) => n !== keep).forEach((n) => n.remove());
    if (keep !== first) textNodesOf(keep).filter((t) => t !== first).forEach((t) => t.remove());
  }

  function wrapRange(root, range, a) {
    if (linkAt(root, range.startContainer) || linkAt(root, range.endContainer)) return { error: MSG_OVERLAP };
    const isolated = host.__EASYOFFICE_DOC_RUNS__.isolateRangeText(root, range);
    if (!isolated) return { error: MSG_ONE_PARAGRAPH };
    if (isolated.texts.some((t) => linkAt(root, t))) return { error: MSG_OVERLAP };
    const tops = isolated.texts.map((t) => isolateInline(t, isolated.container));
    const last = tops[tops.length - 1];
    let node = tops[0];
    node.before(a);
    for (;;) {
      const next = node.nextSibling ?? null;
      a.appendChild(node);
      if (node === last) break;
      node = next;
    }
    return {};
  }

  // A new link at a caret holds only its text.
  function insertAtCaret(range, a, label) {
    a.appendChild(a.ownerDocument.createTextNode(label));
    const { startContainer: node, startOffset: offset } = range;
    if (node.nodeType === 3) {
      const tail = offset > 0 && offset < node.data.length ? node.splitText(offset) : null;
      const parent = node.parentNode;
      parent.insertBefore(a, tail ?? (offset <= 0 ? node : node.nextSibling ?? null));
    } else {
      node.insertBefore(a, node.childNodes[offset] ?? null);
    }
  }

  // Add (reference `AddDocHyperLinkCommand`): the selected text becomes the link; at a caret `label` (else the
  // address) is inserted. A different `label` replaces the selected text (`UpdateDocHyperLinkCommand`).
  // `{ changed, link }`, or `{ changed: false, error }`.
  function addLink(root, range, address, label = "") {
    const url = normalizeUrl(address);
    if (!url) return { changed: false, error: MSG_BAD_URL };
    const a = createLink(root.ownerDocument, url);
    if (range.collapsed) {
      if (linkAt(root, range.startContainer)) return { changed: false, error: MSG_OVERLAP };
      insertAtCaret(range, a, label || url);
      return { changed: true, link: a };
    }
    const wrapped = wrapRange(root, range, a);
    if (wrapped.error) return { changed: false, error: wrapped.error };
    if (label !== "" && label !== a.textContent) relabel(a, label);
    return { changed: true, link: a };
  }

  // Update (reference `UpdateDocHyperLinkCommand`): a new address, and a new label when it differs.
  function updateLink(root, a, address, label = "") {
    const url = normalizeUrl(address);
    if (!url) return { changed: false, error: MSG_BAD_URL };
    a.setAttribute("href", url);
    if (label !== "" && label !== a.textContent) relabel(a, label);
    return { changed: true, link: a };
  }

  // Delete (reference `DeleteDocHyperLinkCommand`): the link goes, its text and formatting stay.
  function removeLink(root, a) {
    const parent = a.parentNode;
    while (a.firstChild) parent.insertBefore(a.firstChild, a);
    a.remove();
    host.__EASYOFFICE_DOC_RUNS__.normalizeContainer(host.__EASYOFFICE_DOC_RUNS__.containerOf(parent, root));
    return { changed: true };
  }

  // ---- dialog ---------------------------------------------------------------------------------

  function createDocLink(deps) {
    const { document, env, getRange, withUndo, setFeedback } = deps;
    const byId = (id) => document.getElementById(id);
    let target = null; // { range, link } of the open dialog
    const ui = () => ({
      dialog: byId("doc-link-dialog"), address: byId("doc-link-address"), label: byId("doc-link-label"),
      status: byId("doc-link-status"), remove: byId("btn-doc-link-remove"), apply: byId("btn-doc-link-apply"),
    });

    function selectedText(range) {
      if (range.collapsed) return "";
      const sameText = range.startContainer === range.endContainer && range.startContainer.nodeType === 3;
      if (sameText) return range.startContainer.data.slice(range.startOffset, range.endOffset);
      return String(env.getSelection?.()?.toString() ?? "");
    }

    function open() {
      const root = byId("doc-editor");
      const range = getRange();
      if (!root || !range) {
        setFeedback?.(MSG_NO_POSITION);
        return false;
      }
      const link = linkAt(root, range.startContainer) || (range.collapsed ? null : linkAt(root, range.endContainer));
      const els = ui();
      target = { root, range, link };
      els.address.value = link ? link.getAttribute("href") : "";
      els.label.value = link ? link.textContent : selectedText(range);
      els.status.textContent = "";
      els.remove.hidden = !link;
      els.dialog.showModal();
      els.address.focus();
      return true;
    }

    function run(exec) {
      const result = withUndo(exec);
      if (result.error) {
        ui().status.textContent = result.error;
        return;
      }
      ui().dialog.close();
      target.root.focus?.();
    }

    function apply() {
      const { root, range, link } = target;
      const { address, label } = ui();
      const text = label.value.trim();
      run(() => (link ? updateLink(root, link, address.value, text) : addLink(root, range, address.value, text)));
    }

    function bind() {
      const els = ui();
      if (!els.dialog) return;
      els.apply.addEventListener("click", apply);
      els.remove.addEventListener("click", () => run(() => removeLink(target.root, target.link)));
    }

    return { bind, open };
  }

  host.__EASYOFFICE_DOC_LINK__ = { ALLOWED_SCHEMES, MAX_URL_CHARS, normalizeUrl, linkAt, addLink, updateLink, removeLink };
  host.__EASYOFFICE_CREATE_DOC_LINK__ = createDocLink;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
