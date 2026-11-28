(function (host) {
  "use strict";

  // Equations of the word processor window (PD-401): the OMML of a DOCX equation (`span.eo-math[data-eo-omml]`) is drawn as
  // MathML, which WebKit shows. Reference implementation: the Apache-2.0 DOCX renderer docx-preview (Copyright (c) 2016-2023
  // Volodymyr Baydalka, see THIRD_PARTY_NOTICES.md; `src/document-parser.ts` `mmlTagMap` lines 29-54 and `parseMathElement` 747-766, `src/html-renderer.ts`
  // the math cases of `renderElement` lines 806-871, `renderMmlRadical` 1237, `renderMmlDelimiter` 1248, `renderMmlNary` 1258,
  // `renderMmlPreSubSuper` 1284, `renderMmlGroupChar` 1300, `renderMmlBar` 1311, `renderMllList` 1326, commit 28682e30). The
  // element map is ported as it is; five places differ because the original draws them wrongly: an argument (degree,
  // superscript, subscript, function name) is an `mrow` and not an `mn`/`ms` (a token element holds no elements), a run is an
  // `mi`, `mn` or `mo` by its character and not an `ms` (a string literal), an n-ary operator with only one limit is an
  // `munder` or `mover` (the original tests for both first, so those two cases never ran), and `m:sSubSup`, `m:limUpp` and
  // `m:acc`, which the original leaves out, are `msubsup`, `mover` and `mover`. Equations are shown, kept and saved as they were
  // read; editing them is outside PD-401. The OMML reaches the page only after the engine has rewritten it (a list of elements,
  // `m:val` and `m:t` text only, `rich_word_blocks.rs`), so it is read here by a small reader of that form, not by an XML
  // parser, and the MathML is built from element names and text only. Reuse (구현 원칙 3단계): (1) the converters found are LGPL
  // or tied to a product's own stylesheet; (2) no dependency of the project reads OMML; (3) the renderer above is ported and
  // bounded (depth 24, 2000 elements, as the engine's).

  const MML = "http://www.w3.org/1998/Math/MathML";
  const MAX_DEPTH = 24;
  const MAX_NODES = 2000;
  const TOKEN = /<(\/?)m:([A-Za-z]+)(?:\s+m:val="([^"]*)")?\s*>|([^<]+)/g;
  const ENTITIES = { "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": "\"", "&apos;": "'" };
  const unescapeXml = (s) => s.replace(/&(?:amp|lt|gt|quot|apos);/g, (e) => ENTITIES[e]);

  // The elements of an OMML string in the engine's form: `{ name, val, text, children }`, or null when it is not that form.
  function parseOmml(omml) {
    const root = { name: "", val: null, text: "", children: [] };
    const stack = [root];
    let nodes = 0;
    let consumed = 0;
    for (const m of omml.matchAll(TOKEN)) {
      consumed += m[0].length;
      const top = stack[stack.length - 1];
      if (m[4] !== undefined) {
        top.text += unescapeXml(m[4]);
      } else if (m[1] === "/") {
        if (stack.length < 2 || top.name !== m[2]) return null;
        stack.pop();
      } else {
        nodes += 1;
        if (nodes > MAX_NODES || stack.length > MAX_DEPTH + 1) return null;
        const node = { name: m[2], val: m[3] === undefined ? null : unescapeXml(m[3]), text: "", children: [] };
        top.children.push(node);
        stack.push(node);
      }
    }
    return stack.length === 1 && consumed === omml.length ? root : null;
  }

  const make = (doc, name, children, attrs) => {
    const el = doc.createElementNS(MML, name);
    for (const [k, v] of Object.entries(attrs ?? {})) el.setAttribute(k, v);
    for (const child of children ?? []) if (child) el.appendChild(child);
    return el;
  };

  // A run's text: digits are numbers, letters identifiers, anything else an operator.
  function tokens(doc, text) {
    const out = [];
    for (const piece of text.match(/\d+(?:\.\d+)?|\p{L}+|\s+|./gu) ?? []) {
      if (/^\s+$/.test(piece)) continue;
      out.push(make(doc, /^\d/.test(piece) ? "mn" : /^\p{L}/u.test(piece) ? "mi" : "mo", [doc.createTextNode(piece)]));
    }
    return out;
  }

  const child = (el, name) => el.children.find((c) => c.name === name);
  // A property element of the `...Pr` of `el` that is on (`m:val` absent, or not 0, false or off).
  function flag(el, name, key) {
    const f = child(child(el, name + "Pr") ?? { children: [] }, key);
    return Boolean(f) && (f.val === null || !["0", "false", "off"].includes(f.val));
  }
  const charOf = (el, name, key, fallback) => child(child(el, name + "Pr") ?? { children: [] }, key)?.val ?? fallback;

  // The MathML children of an OMML element.
  function render(doc, el) {
    const row = (e) => make(doc, "mrow", e ? render(doc, e) : []);
    const arg = (e, name) => row(child(e, name));
    const mo = (text) => make(doc, "mo", [doc.createTextNode(text)]);
    const out = [];
    for (const c of el.children) {
      switch (c.name) {
        case "r": for (const t of c.children.filter((x) => x.name === "t")) out.push(...tokens(doc, t.text)); break;
        case "f": out.push(make(doc, "mfrac", [arg(c, "num"), arg(c, "den")])); break;
        case "rad": out.push(flag(c, "rad", "degHide") ? make(doc, "msqrt", [arg(c, "e")]) : make(doc, "mroot", [arg(c, "e"), arg(c, "deg")])); break;
        case "sSup": out.push(make(doc, "msup", [arg(c, "e"), arg(c, "sup")])); break;
        case "sSub": out.push(make(doc, "msub", [arg(c, "e"), arg(c, "sub")])); break;
        case "sSubSup": out.push(make(doc, "msubsup", [arg(c, "e"), arg(c, "sub"), arg(c, "sup")])); break;
        case "sPre": out.push(make(doc, "mrow", [make(doc, "msubsup", [make(doc, "mo"), arg(c, "sub"), arg(c, "sup")]), arg(c, "e")])); break;
        case "d":
          out.push(make(doc, "mrow", [mo(charOf(c, "d", "begChr", "(")), ...c.children.filter((x) => x.name === "e").map(row), mo(charOf(c, "d", "endChr", ")"))]));
          break;
        case "nary": {
          const op = mo(charOf(c, "nary", "chr", "∫"));
          const sub = child(c, "sub") && !flag(c, "nary", "subHide") ? arg(c, "sub") : null;
          const sup = child(c, "sup") && !flag(c, "nary", "supHide") ? arg(c, "sup") : null;
          const head = sub && sup ? make(doc, "munderover", [op, sub, sup]) : sup ? make(doc, "mover", [op, sup]) : sub ? make(doc, "munder", [op, sub]) : op;
          out.push(make(doc, "mrow", [head, arg(c, "e")]));
          break;
        }
        case "func": out.push(make(doc, "mrow", [arg(c, "fName"), arg(c, "e")])); break;
        case "limLow": out.push(make(doc, "munder", [arg(c, "e"), arg(c, "lim")])); break;
        case "limUpp": out.push(make(doc, "mover", [arg(c, "e"), arg(c, "lim")])); break;
        case "acc": out.push(make(doc, "mover", [arg(c, "e"), mo(charOf(c, "acc", "chr", "̂"))], { accent: "true" })); break;
        case "groupChr": {
          const above = charOf(c, "groupChr", "pos", "bot") === "top";
          out.push(make(doc, above ? "mover" : "munder", [arg(c, "e"), mo(charOf(c, "groupChr", "chr", "⏟"))]));
          break;
        }
        case "bar": out.push(make(doc, "mrow", render(doc, c), { style: "text-decoration: " + (charOf(c, "bar", "pos", "bot") === "top" ? "overline" : "underline") })); break;
        case "eqArr": out.push(make(doc, "mtable", c.children.filter((x) => x.name === "e").map((e) => make(doc, "mtr", [make(doc, "mtd", [row(e)])])))); break;
        case "m": out.push(make(doc, "mtable", c.children.filter((x) => x.name === "mr").map((mr) => make(doc, "mtr", mr.children.filter((x) => x.name === "e").map((e) => make(doc, "mtd", [row(e)])))))); break;
        case "e": case "num": case "den": case "box": case "borderBox": case "phant": case "oMath": out.push(row(c)); break;
        default: break;
      }
    }
    return out;
  }

  // The MathML `math` element of an OMML string, or null when it is not an equation in the engine's form.
  function toMathML(doc, omml) {
    if (typeof omml !== "string" || !omml.startsWith("<m:oMath")) return null;
    const tree = parseOmml(omml);
    const top = tree?.children[0];
    if (!top || tree.children.length !== 1) return null;
    const display = top.name === "oMathPara";
    const roots = display ? top.children.filter((c) => c.name === "oMath") : [top];
    return make(doc, "math", render(doc, { children: roots }), display ? { display: "block" } : {});
  }

  // Draws the equations of `root` that have no MathML yet (the editor's snapshots keep the OMML, not the MathML).
  function hydrate(root) {
    const doc = root.ownerDocument;
    for (const span of root.querySelectorAll?.(".eo-math[data-eo-omml]") ?? []) {
      if (span.firstElementChild?.namespaceURI === MML) continue;
      const math = toMathML(doc, span.dataset.eoOmml);
      span.replaceChildren(...(math ? [math] : [doc.createTextNode(span.dataset.eoMathText ?? "")]));
    }
  }

  function watch(editor) {
    if (!editor || typeof host.MutationObserver !== "function" || editor.dataset.eoMathWatch === "1") return;
    editor.dataset.eoMathWatch = "1";
    let queued = false;
    const observer = new host.MutationObserver(() => {
      if (queued) return;
      queued = true;
      (host.requestAnimationFrame ?? ((fn) => host.setTimeout(fn, 16)))(() => {
        queued = false;
        observer.disconnect();
        hydrate(editor);
        observer.observe(editor, { childList: true, subtree: true });
      });
    });
    observer.observe(editor, { childList: true, subtree: true });
    hydrate(editor);
  }

  host.__EASYOFFICE_DOC_WORD_MATH__ = { toMathML, parseOmml, hydrate, watch, MML };
  const start = () => watch(host.document?.getElementById?.("doc-editor"));
  if (host.document?.readyState === "loading") host.document.addEventListener("DOMContentLoaded", start);
  else start();
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
