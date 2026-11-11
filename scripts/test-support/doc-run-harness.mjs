import { createWebUiDom, flushAsyncTurns } from "../webui-dom-harness.mjs";

// Selection holder honoring the Selection API surface the controller uses.
export function createSelection() {
  return {
    _range: null,
    get rangeCount() {
      return this._range ? 1 : 0;
    },
    getRangeAt() {
      return this._range;
    },
    removeAllRanges() {
      this._range = null;
    },
    addRange(r) {
      this._range = r;
    },
  };
}

export function rangeOf(sc, so, ec, eo) {
  return {
    startContainer: sc,
    startOffset: so,
    endContainer: ec,
    endOffset: eo,
    get collapsed() {
      return sc === ec && so === eo;
    },
  };
}

export function build(document, parent, spec) {
  const el = document.createElement(spec.tag);
  (spec.children || []).forEach((c) => {
    if (typeof c === "string") el.appendChild(document.createTextNode(c));
    else el.appendChild(build(document, el, c));
  });
  parent.appendChild(el);
  return el;
}

export async function createRunHarness(content, options = {}) {
  const selection = createSelection();
  const converts = [];
  const harness = await createWebUiDom({
    selection,
    ...options,
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") converts.push(JSON.parse(String(payload)));
      const cat = harness.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = harness.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
        getActiveModule: () => "DOC",
        ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
        ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
        contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
        moduleSpecs: cat.MODULE_SPECS,
        localRibbonState: cat.createDefaultRibbonState(),
      });
      return offline[command](payload);
    },
  });
  const { document, sandbox } = harness;
  const editor = document.getElementById("doc-editor");
  editor.replaceChildren();
  const blocks = (content || []).map((spec) => build(document, editor, spec));
  await flushAsyncTurns(30);
  return { ...harness, selection, converts, editor, blocks, runs: sandbox.__EASYOFFICE_DOC_RUNS__ };
}

export const p = (...children) => ({ tag: "p", children });
export const span = (text) => ({ tag: "span", children: [text] });
export const textNodeOf = (block) => block.childNodes.find((n) => n.nodeType === 3);
export const flatRuns = (block) =>
  block.childNodes.map((n) =>
    n.nodeType === 3 ? { text: n.data } : { text: n.textContent, ...Object.fromEntries(Object.entries(dataOf(n))) }
  );
export function dataOf(el) {
  const out = {};
  for (const key of Object.keys(el.dataset)) out[key] = el.dataset[key];
  return out;
}

export function fireChange(document, id, value) {
  const input = document.getElementById(id);
  input.value = value;
  input.dispatchEvent({ type: "change", target: input });
}

export async function click(harness, id) {
  harness.userClick(harness.document.getElementById(id));
  await flushAsyncTurns(30);
}

