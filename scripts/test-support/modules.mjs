// Shared helpers for the WebUI DOM tests.

// Windows host exactly one module chosen at launch, so there is no on-screen module switcher;
// tests use the same activation entry the launch code uses.
export function switchModule(document, moduleName) {
  return document.defaultView.__EASYOFFICE_ACTIVATE_MODULE__(moduleName);
}

export function typeIntoDocParagraph(document, text) {
  const docEditor = document.getElementById("doc-editor");
  docEditor.focus();
  const p = docEditor.querySelector("p");
  docEditor.dispatchEvent({ type: "beforeinput", target: p });
  p.textContent = text;
  docEditor.dispatchEvent({ type: "input", target: p });
  return p;
}

export function createMemoryStorage(seed = {}) {
  const map = new Map(Object.entries(seed));
  return {
    map,
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => {
      map.set(k, String(v));
    },
    removeItem: (k) => {
      map.delete(k);
    },
  };
}
