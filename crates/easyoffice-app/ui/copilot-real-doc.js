// 직접 썼다(PD-392): 문서(DOC)와 옛 편집기 한글의 연산을 실제 화면 조작 단계로 바꾸는 표다. 실행은 `copilot-real.js`가 맡는다.
//
//   doc.replace_paragraphs(replacements) : 문단 글을 끌어 선택(select) → 새 글을 친다(type)                      기대: 문단 글 = 새 글
//   doc.replace_span                     : 찾은 글 구간을 끌어 선택 → 바꿀 글을 친다                           기대: 문단 글 = 앞 + 바꿀 글 + 뒤
//   doc.append(blocks|paragraphs)        : 마지막 문단 끝을 눌러 캐럿을 두고 Enter → 글을 친다               기대: 새 문단 HTML = 내부 경로가 만들 문단 HTML
//
// 서식이 섞인 글(인라인 표시, 표, 제목·목록 항목), 인라인 서식이 있는 문단, 변경 추적 중인 문서, 범위 교체(`startIndex`·`endIndex`), 개인정보 가리기는 화면 조작으로 표현하지
// 않고 내부 경로로 적용한다(혼합 허용). 문단은 `docBlocksOf`의 번호로 매 단계 다시 찾는다.

(function (host) {
  "use strict";

  const MARKUP = /<\/?(?:strong|b|em|i|u|s|del|strike|a|table|br)\b/i;
  const TEXT_BLOCK = "p, h1, h2, h3, h4, h5, h6";
  const opKind = (op) => String(op?.op || op?.type || op?.kind || "").trim();
  const isIndex = (value) => Number.isInteger(value) && value >= 0;

  function createDocPlanner({ document, docBlocksOf, blockText, buildDocBlockElement }) {
    const editor = () => document.getElementById("doc-editor");
    const blocks = () => docBlocksOf(editor());
    const docHtml = () => host.__EASYOFFICE_COPILOT_DOC_HTML__;
    const plainOf = (el) => (docHtml()?.plainActiveTextOfBlock ? docHtml().plainActiveTextOfBlock(el) : String(el.textContent || ""));
    const tracking = () => Boolean(docHtml()?.isTrackChangesActive?.(editor()));
    // 글 노드만 가진 글 문단: 글을 쳐 넣어도 서식이 사라지거나 남지 않는 문단이다.
    const isPlainBlock = (el) => Boolean(el) && el.matches(TEXT_BLOCK) && el.children.length === 0;

    // 단순 글(문자열 또는 `{ text }`)이면 그 원문, 아니면 `null`.
    function rawOf(item) {
      if (typeof item === "string") return item;
      return item && typeof item === "object" && Object.keys(item).every((key) => key === "text") ? String(item.text ?? "") : null;
    }
    const plainText = (raw) => raw !== null && raw.trim() !== "" && !MARKUP.test(raw) && !raw.includes("\n");
    const listOf = (op) => (Array.isArray(op.blocks) ? op.blocks : Array.isArray(op.paragraphs) ? op.paragraphs : null);

    /** 화면 조작으로 할 수 있는 모양이면 단계 수(셀·문단 단위), 아니면 0. 문서 상태는 읽지 않는다. */
    function supports(op) {
      const kind = opKind(op);
      if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
        const ok = op.replacements.length > 0 && op.replacements.every((rep) => isIndex(Number(rep?.index)) && plainText(rawOf(String(rep?.text ?? ""))));
        return ok ? op.replacements.length : 0;
      }
      if (kind === "doc.replace_span") {
        return isIndex(Number(op.index)) && String(op.search ?? "") !== "" && plainText(rawOf(String(op.replace ?? ""))) ? 1 : 0;
      }
      if (kind === "doc.append") {
        const items = listOf(op);
        return items && items.length > 0 && items.every((item) => plainText(rawOf(item))) ? items.length : 0;
      }
      return 0;
    }

    const at = (index) => () => blocks()[index];
    const blockLocation = (index) => ({ module: "DOC", label: "문단 " + (index + 1), paragraphIndex: index });

    function replaceSteps(index, from, to, text, expectText) {
      const find = at(index);
      return [
        { act: "select", label: `문단 ${index + 1} 선택`, find, from, to },
        { act: "type", label: `문단 ${index + 1} 입력`, find, text, expect: () => find() && plainOf(find()) === expectText },
      ];
    }

    function planReplace(op) {
      const steps = [];
      const locations = [];
      for (const rep of op.replacements) {
        const index = Number(rep.index);
        const el = blocks()[index];
        if (!isPlainBlock(el)) return null;
        const text = blockText(String(rep.text));
        steps.push(...replaceSteps(index, 0, plainOf(el).length, text, text));
        locations.push(blockLocation(index));
      }
      return { steps, locations };
    }

    function planSpan(op) {
      const index = Number(op.index);
      const el = blocks()[index];
      if (!isPlainBlock(el)) return null;
      const cur = plainOf(el);
      const search = String(op.search);
      const replace = String(op.replace);
      const start = cur.indexOf(search);
      if (start < 0 || cur.indexOf(search, start + 1) >= 0) return null;
      const next = cur.slice(0, start) + replace + cur.slice(start + search.length);
      return { steps: replaceSteps(index, start, start + search.length, replace, next), locations: [blockLocation(index)] };
    }

    function planAppend(op) {
      const list = blocks();
      const last = list.at(-1);
      if (!isPlainBlock(last)) return null;
      // 내부 경로는 빈 문단 하나뿐인 문서의 그 문단을 지우고 덧붙인다: 그 문단에 첫 글을 친다.
      const lone = list.length === 1 && plainOf(last).trim() === "";
      const steps = [];
      const locations = [];
      let count = list.length;
      listOf(op).forEach((item, offset) => {
        const built = buildDocBlockElement(document, item);
        const text = blockText(item);
        if (!built || built.tagName !== "P" || built.children.length > 0) return;
        const typed = { act: "type", label: `새 문단 입력`, text, expect: () => blocks().at(-1)?.outerHTML === built.outerHTML };
        if (offset === 0 && lone) {
          steps.push({ act: "select", label: "빈 문단 선택", find: () => blocks().at(-1), from: 0, to: 0 }, typed);
        } else {
          const expectCount = count + 1;
          steps.push(
            { act: "select", label: "마지막 문단 끝", find: () => blocks().at(-1), from: plainOf(blocks().at(-1)).length, to: plainOf(blocks().at(-1)).length },
            { act: "key", key: "Enter", label: "새 문단", expect: () => blocks().length === expectCount },
            typed,
          );
          count = expectCount;
        }
        locations.push(blockLocation(count - 1));
      });
      return steps.length > 0 && locations.length === listOf(op).length ? { steps, locations } : null;
    }

    function plan(op) {
      if (tracking()) return null;
      const kind = opKind(op);
      if (kind === "doc.replace_paragraphs") return planReplace(op);
      if (kind === "doc.replace_span") return planSpan(op);
      return kind === "doc.append" ? planAppend(op) : null;
    }

    return { supports, plan };
  }

  host.__EASYOFFICE_COPILOT_REAL_DOC__ = Object.freeze({ createDocPlanner });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
