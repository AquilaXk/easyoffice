(function (host) {
  "use strict";

  // 편집 > 지우기 of the sheet window (PD-400): 모두 지우기, 서식 지우기, 내용 지우기, 메모 지우기, 하이퍼링크 지우기, as in the
  // spreadsheet program's 지우기 list. Each item works on the cells of the selection that exist in the model (one pass over the
  // sheet's cells, so a whole column costs no more than the sheet holds) and goes through the cell writer: one undo step,
  // and the protection of the sheet decides. 내용 지우기 is the Delete key (`clearSelectionContents`), which leaves format,
  // hyperlink and note as they are. 서식 지우기 takes the cell format and the merge off (and the conditional formats of
  // the selection, in the step the rules own), 모두 지우기 does all of it and drops the hyperlink and the note too.
  // Reuse (구현 원칙 3단계): (1) no library: this is a composition of the window's own writers; (2) `applyCellEdits` (`style: null`
  // and `link: null` remove), `sheet-note-ui.js` (`deleteNotesIn`), the conditional format clear of the 서식 menu; (3) nothing ported.

  const MESSAGES = {
    nothing: "지울 내용이 없습니다",
    large: "선택 범위가 너무 큼",
  };
  const MAX_RANGES = 1000;

  function createSheetClearUi(deps) {
    const { document, getRanges, getActiveSheetName, snapshotDomSheetCells, applyCellEdits, clearSelectionContents, gridUtils, say, getNoteUi } = deps;

    const inside = (ranges, at) => ranges.some((rg) => at.row >= rg.r1 && at.row <= rg.r2 && at.col >= rg.c1 && at.col <= rg.c2);

    // The model entries of the selection that `wanted` accepts, as `[ref, entry]`.
    function entriesIn(ranges, wanted) {
      const map = snapshotDomSheetCells();
      const found = [];
      for (const ref of Object.keys(map)) {
        const entry = map[ref];
        const at = entry ? gridUtils.parseRef(ref) : null;
        if (at && inside(ranges, at) && wanted(entry)) found.push([ref, entry]);
      }
      return found;
    }

    // The unchanged cell of an edit that changes one thing: its text and its type tag ride along.
    const keep = (ref, entry) => ({ ref, value: entry.value ?? "", formula: entry.formula ?? "", ...(entry.t ? { t: entry.t } : {}) });

    function selection() {
      const ranges = getRanges().slice(0, MAX_RANGES + 1);
      if (ranges.length > MAX_RANGES) {
        say(MESSAGES.large);
        return null;
      }
      return ranges;
    }

    // The merges of a single selected range go with the format; of several ranges they stay (the writer takes one area).
    const mergeArea = (ranges) => (ranges.length === 1 ? ranges[0] : null);
    const hasMerge = (ranges) => {
      const area = mergeArea(ranges);
      return area !== null && host.__EASYOFFICE_SHEET_LAYOUT__.mergesIn(getActiveSheetName(), area.r1, area.c1, area.r2, area.c2).length > 0;
    };
    const mergesOf = (ranges) => (mergeArea(ranges) ? { clear: mergeArea(ranges), add: [] } : undefined);

    // The conditional formats of the selection go in the step the rules own (서식 > 조건부 서식 > 규칙 지우기 > 선택한 셀에서).
    const clearRules = () => host.__EASYOFFICE_SHEET_CF_RUN__?.("clear-selection");

    async function clearFormats() {
      const ranges = selection();
      if (!ranges) return false;
      const edits = entriesIn(ranges, (entry) => Boolean(entry.style)).map(([ref, entry]) => ({ ...keep(ref, entry), style: null }));
      const done = edits.length > 0 || hasMerge(ranges) ? await applyCellEdits(edits, "서식 지우기", undefined, mergesOf(ranges)) : false;
      await clearRules();
      if (!done) say(MESSAGES.nothing);
      return done;
    }

    async function clearLinks() {
      const ranges = selection();
      if (!ranges) return false;
      const edits = entriesIn(ranges, (entry) => Boolean(entry.link)).map(([ref, entry]) => ({ ...keep(ref, entry), link: null }));
      if (edits.length === 0) {
        say(MESSAGES.nothing);
        return false;
      }
      return applyCellEdits(edits, "하이퍼링크 지우기");
    }

    async function clearNotes() {
      const ranges = selection();
      if (!ranges) return false;
      const done = (await getNoteUi()?.deleteNotesIn(ranges)) === true;
      say(done ? "메모 지우기" : MESSAGES.nothing);
      return done;
    }

    async function clearAll() {
      const ranges = selection();
      if (!ranges) return false;
      const edits = entriesIn(ranges, () => true).map(([ref]) => ({ ref, value: "", formula: "", style: null, link: null }));
      let done = edits.length > 0 || hasMerge(ranges) ? await applyCellEdits(edits, "모두 지우기", undefined, mergesOf(ranges)) : false;
      if ((await getNoteUi()?.deleteNotesIn(ranges)) === true) done = true;
      await clearRules();
      if (!done) say(MESSAGES.nothing);
      return done;
    }

    async function clearContents() {
      return (await clearSelectionContents()) !== false;
    }

    return {
      actions: {
        "clear-all": clearAll,
        "clear-formats": clearFormats,
        "clear-contents": clearContents,
        "clear-notes": clearNotes,
        "clear-links": clearLinks,
      },
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_CLEAR_UI__ = createSheetClearUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
