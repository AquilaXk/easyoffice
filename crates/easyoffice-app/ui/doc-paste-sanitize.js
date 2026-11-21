(function (host) {
  "use strict";

  // Clears the parts of office (word processor, spreadsheet, presentation) and Hangul clipboard HTML that are not
  // content, before the HTML is parsed. Reference implementation: `sanitizeOfficeHtmlForCore` of rhwp-studio
  // (https://github.com/edwardkim/rhwp, commit 1a76570e833917d15817415a53c09ad61ab3203f,
  // `rhwp-studio/src/engine/office-html-sanitize.ts` lines 1-78, MIT, Copyright (c) 2025-2026 Edward Kim), moved to plain
  // JavaScript with the same scan: no chain of regular expression replacements, one pass that drops a start marker up
  // to its end marker, or to the end of the text when there is no end marker, so no remainder is left behind. The pass is
  // linear in the length (each marker search moves forward only). Nothing is changed in the scan itself; the reference
  // notes the cases it covers: `<!--<!-- -->` (a replace leaves the opening marker), `</script >` (a space after the
  // name), conditional comments `<!--[if ...]>`, `<![if ...]>...<![endif]>` and the fragment markers of the clipboard
  // format (comments too).

  // Elements that are never content. A `<style>` in the body is text for a parser that does not know it.
  const NON_CONTENT_ELEMENTS = ["script", "style", "xml"];
  const CONDITIONAL_END = "<![endif]>";

  // The tag name ends here: `<style>`, `<style ...>` and `<style/>` are the element, `<styles>` is not.
  function isTagNameBoundary(code) {
    return Number.isNaN(code) || code === 0x3e /* > */ || code === 0x2f /* / */ || code <= 0x20;
  }

  // The index where the element that opens at `start` ends; the end of the text when it has no end tag.
  function skipNonContentElement(html, lower, start, name) {
    const tagEnd = html.indexOf(">", start);
    if (tagEnd < 0) return html.length; // a cut-off start tag
    if (html.charCodeAt(tagEnd - 1) === 0x2f) return tagEnd + 1; // a tag that closes itself, `<xml/>`
    let from = tagEnd + 1;
    for (;;) {
      const close = lower.indexOf(`</${name}`, from);
      if (close < 0) return html.length;
      // `</script >` and `</script\n>` end it too: after the name and the white space the next character is `>`.
      let cursor = close + 2 + name.length;
      while (cursor < html.length && html.charCodeAt(cursor) <= 0x20) cursor += 1;
      if (cursor >= html.length) return html.length;
      if (html.charCodeAt(cursor) === 0x3e) return cursor + 1;
      from = close + 2 + name.length;
    }
  }

  function sanitizeOfficeHtml(html) {
    const lower = html.toLowerCase();
    let out = "";
    let index = 0;
    while (index < html.length) {
      const lt = html.indexOf("<", index);
      if (lt < 0) {
        out += html.slice(index);
        break;
      }
      out += html.slice(index, lt);

      if (lower.startsWith("<!--", lt)) {
        const end = lower.indexOf("-->", lt + 4);
        index = end < 0 ? html.length : end + 3;
        continue;
      }
      // downlevel-revealed `<![if !supportEmptyParas]>...<![endif]>`
      if (lower.startsWith("<![if", lt)) {
        const end = lower.indexOf(CONDITIONAL_END, lt);
        index = end < 0 ? html.length : end + CONDITIONAL_END.length;
        continue;
      }
      const dropped = NON_CONTENT_ELEMENTS.find((name) => lower.startsWith(`<${name}`, lt) && isTagNameBoundary(lower.charCodeAt(lt + 1 + name.length)));
      if (dropped) {
        index = skipNonContentElement(html, lower, lt, dropped);
        continue;
      }
      out += "<";
      index = lt + 1;
    }
    return out;
  }

  host.__EASYOFFICE_DOC_PASTE_SANITIZE__ = { sanitizeOfficeHtml };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
