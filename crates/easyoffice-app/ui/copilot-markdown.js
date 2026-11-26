// 참조 구현: udecode/plate (MIT, Copyright (c) Ziad Beyens, Dylan Schiemann, Joe Anderson, Felix Feng,
// 커밋 e401aaf9, see THIRD_PARTY_NOTICES.md) `markdown-joiner-transform.ts`와 짝을 이루는 안전 DOM 마크다운 렌더러.
// innerHTML/outerHTML/insertAdjacentHTML을 일절 쓰지 않고 허용된 DOM 요소와 검증된 URL만 생성한다.

(function (host) {
  "use strict";

  const MAX_MD_CHARS = 65536;
  const MAX_MD_LINES = 2048;
  const MAX_TABLE_COLS = 32;

  function isSafeMarkdownUrl(raw) {
    if (typeof raw !== "string") return false;
    const str = raw.trim();
    if (!str || str.length > 2048) return false;
    for (let i = 0; i < str.length; i += 1) {
      const code = str.charCodeAt(i);
      if (code <= 0x1f || (code >= 0x7f && code <= 0x9f)) return false;
    }
    if (str.startsWith("//") || str.startsWith("\\\\")) return false;
    const lower = str.toLowerCase();
    return (
      lower.startsWith("https://")
      || lower.startsWith("http://")
      || lower.startsWith("mailto:")
    );
  }

  function appendTextWithPageChips(document, parent, text, options) {
    if (!text) return;
    const onPageJump = typeof options?.onPageJump === "function" ? options.onPageJump : null;
    if (!onPageJump) {
      parent.appendChild(document.createTextNode(text));
      return;
    }
    const pageRe = /\[(\d{1,5})\s*쪽\]/g;
    let lastIdx = 0;
    let match;
    while ((match = pageRe.exec(text)) !== null) {
      if (match.index > lastIdx) {
        parent.appendChild(document.createTextNode(text.slice(lastIdx, match.index)));
      }
      const pageNum = Number.parseInt(match[1], 10);
      if (Number.isInteger(pageNum) && pageNum >= 1) {
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "eo-copilot-page-chip";
        chip.dataset.pdfPage = String(pageNum);
        chip.textContent = match[0];
        chip.addEventListener("click", () => onPageJump(pageNum));
        parent.appendChild(chip);
      } else {
        parent.appendChild(document.createTextNode(match[0]));
      }
      lastIdx = match.index + match[0].length;
    }
    if (lastIdx < text.length) {
      parent.appendChild(document.createTextNode(text.slice(lastIdx)));
    }
  }

  function appendInlineMarkdown(document, parent, rawText, options, depth = 0) {
    const text = String(rawText ?? "");
    if (!text) return;
    if (depth > 6) {
      appendTextWithPageChips(document, parent, text, options);
      return;
    }
    // Single-pass token regex for inline code, links, bold, strike, and italic.
    const tokenRe = /`([^`\n]+)`|\[([^\]\n]{1,400})\]\(([^)\s\n]{1,2048})\)|\*\*([\s\S]+?)\*\*|~~([\s\S]+?)~~|\*(?!\s)([^*\n]+?)(?<!\s)\*/g;
    let last = 0;
    let m;
    while ((m = tokenRe.exec(text)) !== null) {
      if (m.index > last) {
        appendTextWithPageChips(document, parent, text.slice(last, m.index), options);
      }
      if (m[1] !== undefined) {
        const code = document.createElement("code");
        code.className = "eo-copilot-md-inline-code";
        code.textContent = m[1];
        parent.appendChild(code);
      } else if (m[2] !== undefined && m[3] !== undefined) {
        if (isSafeMarkdownUrl(m[3])) {
          const a = document.createElement("a");
          a.className = "eo-copilot-md-link";
          a.setAttribute("href", m[3].trim());
          a.setAttribute("target", "_blank");
          a.setAttribute("rel", "noopener noreferrer");
          appendInlineMarkdown(document, a, m[2], options, depth + 1);
          parent.appendChild(a);
        } else {
          appendTextWithPageChips(document, parent, m[2] + " (" + m[3] + ")", options);
        }
      } else if (m[4] !== undefined) {
        const strong = document.createElement("strong");
        appendInlineMarkdown(document, strong, m[4], options, depth + 1);
        parent.appendChild(strong);
      } else if (m[5] !== undefined) {
        const del = document.createElement("del");
        appendInlineMarkdown(document, del, m[5], options, depth + 1);
        parent.appendChild(del);
      } else if (m[6] !== undefined) {
        const em = document.createElement("em");
        appendInlineMarkdown(document, em, m[6], options, depth + 1);
        parent.appendChild(em);
      }
      last = m.index + m[0].length;
    }
    if (last < text.length) {
      appendTextWithPageChips(document, parent, text.slice(last), options);
    }
  }

  function splitPipeRow(line) {
    const trimmed = line.trim();
    const inner = trimmed.startsWith("|") ? trimmed.slice(1) : trimmed;
    const body = inner.endsWith("|") ? inner.slice(0, -1) : inner;
    return body.split("|").slice(0, MAX_TABLE_COLS).map((cell) => cell.trim());
  }

  function isTableSeparatorLine(line) {
    const trimmed = line.trim();
    if (!trimmed.includes("|") || !trimmed.includes("-")) return false;
    const cells = splitPipeRow(trimmed);
    return cells.length > 0 && cells.every((c) => /^:?-{2,}:?$/.test(c));
  }

  function buildTableNode(document, headerCells, bodyRows, options) {
    const wrap = document.createElement("div");
    wrap.className = "eo-copilot-md-table-wrap";
    wrap.setAttribute("tabindex", "0");
    wrap.setAttribute("role", "region");
    wrap.setAttribute("aria-label", "코파일럿 표");
    const table = document.createElement("table");
    table.className = "eo-copilot-md-table";
    const thead = document.createElement("thead");
    const headTr = document.createElement("tr");
    for (const cellText of headerCells) {
      const th = document.createElement("th");
      appendInlineMarkdown(document, th, cellText, options);
      headTr.appendChild(th);
    }
    thead.appendChild(headTr);
    table.appendChild(thead);
    if (bodyRows.length > 0) {
      const tbody = document.createElement("tbody");
      for (const row of bodyRows) {
        const tr = document.createElement("tr");
        for (let c = 0; c < headerCells.length; c += 1) {
          const td = document.createElement("td");
          appendInlineMarkdown(document, td, row[c] ?? "", options);
          tr.appendChild(td);
        }
        tbody.appendChild(tr);
      }
      table.appendChild(tbody);
    }
    wrap.appendChild(table);
    return wrap;
  }

  function renderMarkdownInto(document, container, rawSource, options = {}) {
    container.replaceChildren();
    const clipped = String(rawSource ?? "").slice(0, MAX_MD_CHARS);
    if (!clipped.trim()) {
      if (clipped) container.textContent = clipped;
      return container;
    }
    if (options.plainTextOnly) {
      container.textContent = clipped;
      return container;
    }

    const lines = clipped.replaceAll("\r\n", "\n").split("\n").slice(0, MAX_MD_LINES);
    let i = 0;
    let paraLines = [];

    const flushParagraph = () => {
      if (paraLines.length === 0) return;
      const p = document.createElement("div");
      p.className = "eo-copilot-md-p";
      for (let idx = 0; idx < paraLines.length; idx += 1) {
        if (idx > 0) p.appendChild(document.createElement("br"));
        appendInlineMarkdown(document, p, paraLines[idx], options);
      }
      container.appendChild(p);
      paraLines = [];
    };

    while (i < lines.length) {
      const line = lines[i];
      const trimmed = line.trim();

      if (!trimmed) {
        flushParagraph();
        i += 1;
        continue;
      }

      // Fenced code block
      if (trimmed.startsWith("```")) {
        flushParagraph();
        const lang = trimmed.slice(3).trim().slice(0, 32);
        i += 1;
        const codeLines = [];
        while (i < lines.length && !lines[i].trim().startsWith("```")) {
          codeLines.push(lines[i]);
          i += 1;
        }
        if (i < lines.length) i += 1;
        const pre = document.createElement("pre");
        pre.className = "eo-copilot-md-pre";
        const code = document.createElement("code");
        code.className = "eo-copilot-md-code";
        if (lang) code.dataset.lang = lang;
        code.textContent = codeLines.join("\n");
        pre.appendChild(code);
        container.appendChild(pre);
        continue;
      }

      // Heading (# .. ####)
      const headingMatch = /^(#{1,4})\s+(.+)$/.exec(trimmed);
      if (headingMatch) {
        flushParagraph();
        const level = Math.min(4, headingMatch[1].length);
        const h = document.createElement("h" + Math.min(6, level + 2));
        h.className = "eo-copilot-md-heading eo-copilot-md-h" + level;
        appendInlineMarkdown(document, h, headingMatch[2], options);
        container.appendChild(h);
        i += 1;
        continue;
      }

      // Pipe table
      if (trimmed.includes("|") && i + 1 < lines.length && isTableSeparatorLine(lines[i + 1])) {
        flushParagraph();
        const headerCells = splitPipeRow(trimmed);
        i += 2;
        const bodyRows = [];
        while (i < lines.length && lines[i].trim().startsWith("|")) {
          bodyRows.push(splitPipeRow(lines[i]));
          i += 1;
        }
        container.appendChild(buildTableNode(document, headerCells, bodyRows, options));
        continue;
      }

      // Blockquote
      if (trimmed.startsWith(">")) {
        flushParagraph();
        const quoteLines = [];
        while (i < lines.length && lines[i].trim().startsWith(">")) {
          quoteLines.push(lines[i].trim().replace(/^>\s?/, ""));
          i += 1;
        }
        const bq = document.createElement("blockquote");
        bq.className = "eo-copilot-md-quote";
        for (let q = 0; q < quoteLines.length; q += 1) {
          if (q > 0) bq.appendChild(document.createElement("br"));
          appendInlineMarkdown(document, bq, quoteLines[q], options);
        }
        container.appendChild(bq);
        continue;
      }

      // Unordered or ordered list
      const ulMatch = /^[-*]\s+(.+)$/.exec(trimmed);
      const olMatch = /^\d+\.\s+(.+)$/.exec(trimmed);
      if (ulMatch || olMatch) {
        flushParagraph();
        const ordered = Boolean(olMatch);
        const listEl = document.createElement(ordered ? "ol" : "ul");
        listEl.className = "eo-copilot-md-list";
        while (i < lines.length) {
          const cur = lines[i].trim();
          const mItem = ordered ? /^\d+\.\s+(.+)$/.exec(cur) : /^[-*]\s+(.+)$/.exec(cur);
          if (!mItem) break;
          const li = document.createElement("li");
          const taskMatch = !ordered ? /^\[([ xX])\]\s+(.+)$/.exec(mItem[1]) : null;
          if (taskMatch) {
            const mark = taskMatch[1].toLowerCase() === "x" ? "☑ " : "☐ ";
            li.appendChild(document.createTextNode(mark));
            appendInlineMarkdown(document, li, taskMatch[2], options);
          } else {
            appendInlineMarkdown(document, li, mItem[1], options);
          }
          listEl.appendChild(li);
          i += 1;
        }
        container.appendChild(listEl);
        continue;
      }

      paraLines.push(line);
      i += 1;
    }

    flushParagraph();
    return container;
  }

  host.__EASYOFFICE_COPILOT_MARKDOWN__ = Object.freeze({
    isSafeMarkdownUrl,
    renderMarkdownInto,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
