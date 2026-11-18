// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0, Copyright 2023-present DreamNum
// Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9): `packages/sheets-table/src/models/table-filter.ts` lines 27-82
// (`TableFilters`: per-column filter conditions and filtered-out rows). Source and license text: `THIRD_PARTY_NOTICES.md`.
// Modifications: interactive table slicers (`슬라이서 삽입` dialog and floating `.sheet-slicer-card` cards over `#sheet-frame`)
// scanning distinct column values in a single O(R) pass and synchronizing bidirectionally with `sheet-filter.js`.

(function (host) {
  "use strict";

  const MAX_SLICERS_PER_TABLE = 64;
  const MAX_SLICERS_TOTAL = 256;
  const MAX_SLICER_ITEMS = 2000;
  const MAX_SCAN_ROWS = 200000;
  const DEFAULT_WIDTH = 184;
  const DEFAULT_HEIGHT = 208;
  const BASE_LEFT = 24;
  const BASE_TOP = 24;
  const STEP_X = 196;
  const STEP_Y = 24;
  const MAX_POS = 65535;
  const BLANK_LABEL = "(빈 셀)";

  function compareItems(a, b) {
    if (a.hasData !== b.hasData) return a.hasData ? -1 : 1;
    if (a.num !== null && b.num !== null) return a.num - b.num;
    if (a.num !== null) return -1;
    if (b.num !== null) return 1;
    return a.key.localeCompare(b.key, "ko");
  }

  function createSheetSlicerUi(deps) {
    const {
      document,
      kit,
      sheetDataStore,
      getActiveSheetName,
      getActiveModule,
      everyTable,
      allowed,
      changeTable,
      filter,
      say,
    } = deps;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const refOf = host.__EASYOFFICE_SHEET_REFS__.refOf;
    const normalizeNum = (raw) => host.__EASYOFFICE_SHEET_UTILS__.normalizeFormattedNumericCell(raw);
    const { $, openDialog, closeDialog, note } = kit;
    let activeTable = null;
    let bound = false;

    function checkboxes() {
      return [...($("sheet-slicer-columns")?.querySelectorAll("input[data-slicer-col]") || [])];
    }

    function openInsert(table) {
      const sheet = getActiveSheetName();
      if (!table || !allowed(sheet)) return false;
      if (table.header === false) {
        say("머리글 행이 있는 표에만 슬라이서를 넣을 수 있음");
        return false;
      }
      ensureBound();
      activeTable = table;
      const list = $("sheet-slicer-columns");
      if (list) {
        list.replaceChildren();
        table.columns.forEach((colName, idx) => {
          const item = document.createElement("li");
          const label = document.createElement("label");
          label.className = "dialog-checkbox";
          const input = document.createElement("input");
          input.setAttribute("type", "checkbox");
          input.type = "checkbox";
          input.checked = false;
          input.dataset.slicerCol = String(idx);
          label.appendChild(input);
          label.appendChild(document.createTextNode(" " + colName));
          item.appendChild(label);
          list.appendChild(item);
        });
      }
      note("sheet-slicer-status", "");
      openDialog("sheet-slicer-dialog");
      checkboxes()[0]?.focus?.();
      return true;
    }

    function totalSlicers() {
      return everyTable().reduce((sum, t) => sum + (Array.isArray(t.slicers) ? t.slicers.length : 0), 0);
    }

    function submitInsert() {
      if (!activeTable) return;
      const cols = checkboxes()
        .filter((b) => b.checked)
        .map((b) => Number(b.dataset.slicerCol));
      if (cols.length === 0) {
        note("sheet-slicer-status", "열을 하나 이상 선택");
        return;
      }
      const existingCount = Array.isArray(activeTable.slicers) ? activeTable.slicers.length : 0;
      if (existingCount + cols.length > MAX_SLICERS_PER_TABLE || totalSlicers() + cols.length > MAX_SLICERS_TOTAL) {
        note("sheet-slicer-status", "슬라이서 개수 한도를 초과함");
        return;
      }
      const target = activeTable;
      const sheetCountBefore = layoutApi
        .tablesOf(getActiveSheetName())
        .reduce((sum, t) => sum + (Array.isArray(t.slicers) ? t.slicers.length : 0), 0);
      const anchor = (() => {
        const b = layoutApi.tableBox(target);
        const frame = $("sheet-frame");
        const edgeTd = document.querySelector?.('#pane-sheet td[data-cell="' + refOf(b.r1, b.c2) + '"]');
        if (frame?.getBoundingClientRect && edgeTd?.getBoundingClientRect) {
          const fr = frame.getBoundingClientRect();
          const tr = edgeTd.getBoundingClientRect();
          if (fr.width > 0 && tr.width > 0) {
            return {
              left: Math.max(BASE_LEFT, Math.min(Math.max(BASE_LEFT, Math.round(fr.width - DEFAULT_WIDTH - 16)), Math.round(tr.right - fr.left + 16))),
              top: Math.max(BASE_TOP, Math.round(tr.top - fr.top)),
            };
          }
        }
        return { left: Math.min(960, 48 + b.c2 * 100), top: BASE_TOP };
      })();
      closeDialog("sheet-slicer-dialog");
      void changeTable(target, (held) => {
        if (held.header === false) return { error: "머리글 행이 있는 표에만 슬라이서를 넣을 수 있음" };
        const prev = Array.isArray(held.slicers) ? [...held.slicers] : [];
        cols.forEach((col, k) => {
          const n = sheetCountBefore + k;
          prev.push({
            col,
            caption: held.columns[col] || "Column" + (col + 1),
            left: Math.min(MAX_POS, anchor.left + (n % 3) * STEP_X),
            top: Math.min(MAX_POS, anchor.top + Math.floor(n / 3) * STEP_Y),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            multiSelect: false,
          });
        });
        const next = { ...held, slicers: prev };
        if (!Array.isArray(next.filter)) next.filter = [];
        return { table: next, text: "슬라이서 삽입", recalc: false };
      });
    }

    function cellDisplayVal(entry) {
      let val = entry?.value === undefined || entry?.value === null ? "" : String(entry.value);
      if (entry?.style?.numFmt && val !== "") val = normalizeNum(val);
      return val;
    }

    // Single-pass O(R) scan of distinct displayed keys in column `absCol` of `table`, tracking cross-filter availability.
    function collectItems(sheet, table, absCol) {
      const b = layoutApi.tableBox(table);
      const d1 = b.r1 + 1;
      const d2 = Math.min(b.r2 - (table.totals === true ? 1 : 0), d1 + MAX_SCAN_ROWS - 1);
      const map = sheetDataStore[sheet] || {};
      const ownCond = (table.filter || []).find((c) => c.col === absCol);
      const otherConds = (table.filter || []).filter((c) => c.col !== absCol);
      const useHiddenSpans = otherConds.length > 0 && !ownCond && Array.isArray(table.filterHidden);
      const hiddenSpans = useHiddenSpans ? table.filterHidden : [];
      const compiledOthers = !useHiddenSpans && otherConds.length > 0
        ? otherConds.map((oc) => (Array.isArray(oc.values) || oc.blank !== undefined
          ? { col: oc.col, set: new Set(oc.values || []), blank: Boolean(oc.blank) }
          : null))
        : [];
      let hi = 0;
      const seen = new Map();
      let hasBlank = false;
      let blankHasData = false;
      for (let r = d1; r <= d2; r += 1) {
        let survivesOther = true;
        if (otherConds.length > 0) {
          if (useHiddenSpans) {
            while (hi < hiddenSpans.length && hiddenSpans[hi][1] < r) hi += 1;
            survivesOther = !(hi < hiddenSpans.length && hiddenSpans[hi][0] <= r && r <= hiddenSpans[hi][1]);
          } else {
            survivesOther = compiledOthers.every((co) => {
              if (!co) return true;
              const v = cellDisplayVal(map[refOf(r, co.col)]);
              return v === "" ? co.blank : co.set.has(v);
            });
          }
        }
        const entry = map[refOf(r, absCol)];
        const val = cellDisplayVal(entry);
        if (val === "") {
          hasBlank = true;
          if (survivesOther) blankHasData = true;
          continue;
        }
        const existing = seen.get(val);
        if (existing) {
          if (survivesOther) existing.hasData = true;
        } else if (seen.size < MAX_SLICER_ITEMS) {
          const num = entry?.t !== "s" && val.trim() !== "" && Number.isFinite(Number(val)) ? Number(val) : null;
          seen.set(val, { key: val, label: val, isBlank: false, num, hasData: survivesOther });
        }
      }
      const items = [...seen.values()].sort(compareItems);
      if (hasBlank) items.push({ key: "", label: BLANK_LABEL, isBlank: true, num: null, hasData: blankHasData });
      return items;
    }

    async function applyItemClick(sheet, table, slicerIdx, item, additive) {
      const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
      if (protectApi && protectApi.isOn(sheet) && !protectApi.guardAction(sheet, "autoFilter")) return;
      const held = layoutApi.tablesOf(sheet).find((t) => t.name === table.name);
      if (!held || held.header === false) return;
      const slicer = held.slicers?.[slicerIdx];
      if (!slicer) return;
      if (!Array.isArray(held.filter)) layoutApi.setTableFilter(sheet, held.name, [], []);

      const b = layoutApi.tableBox(held);
      const absCol = b.c1 + slicer.col;
      const items = collectItems(sheet, held, absCol);
      const cond = (held.filter || []).find((c) => c.col === absCol);
      const listed = Boolean(cond && (Array.isArray(cond.values) || cond.blank !== undefined));
      const pickedValues = new Set(listed ? cond.values || [] : items.filter((i) => !i.isBlank).map((i) => i.key));
      let pickedBlank = listed ? Boolean(cond.blank) : items.some((i) => i.isBlank);

      const multi = Boolean(slicer.multiSelect || additive);
      if (!multi) {
        pickedValues.clear();
        pickedBlank = false;
        if (item.isBlank) pickedBlank = true;
        else pickedValues.add(item.key);
      } else if (item.isBlank) {
        if (pickedBlank && pickedValues.size === 0) return;
        pickedBlank = !pickedBlank;
      } else if (pickedValues.has(item.key)) {
        if (pickedValues.size === 1 && !pickedBlank) return;
        pickedValues.delete(item.key);
      } else {
        pickedValues.add(item.key);
      }

      const nonBlankTotal = items.filter((i) => !i.isBlank).length;
      const hasBlankItem = items.some((i) => i.isBlank);
      const allChosen = pickedValues.size === nonBlankTotal && (!hasBlankItem || pickedBlank);
      const criteria = allChosen
        ? null
        : { ...(pickedBlank ? { blank: true } : {}), values: [...pickedValues] };
      await filter.setCriteria(absCol, criteria, held.name);
    }

    function updateSlicerMeta(table, slicerIdx, patch, statusText) {
      return changeTable(table, (held) => {
        if (!Array.isArray(held.slicers) || !held.slicers[slicerIdx]) return null;
        const nextSlicers = held.slicers.map((s, idx) => (idx === slicerIdx ? { ...s, ...patch } : s));
        return { table: { ...held, slicers: nextSlicers }, text: statusText, recalc: false };
      });
    }

    function removeSlicer(table, slicerIdx) {
      return changeTable(table, (held) => {
        if (!Array.isArray(held.slicers) || !held.slicers[slicerIdx]) return null;
        const nextSlicers = held.slicers.filter((_, idx) => idx !== slicerIdx);
        const next = { ...held };
        if (nextSlicers.length > 0) next.slicers = nextSlicers;
        else delete next.slicers;
        return { table: next, text: "슬라이서 삭제", recalc: false };
      });
    }

    function bindDrag(headEl, cardEl, table, slicerIdx, initialLeft, initialTop) {
      headEl.addEventListener("mousedown", (ev) => {
        if (ev.button !== 0 || ev.target?.closest?.("button")) return;
        const sheet = getActiveSheetName();
        if (host.__EASYOFFICE_SHEET_PROTECT__?.isOn(sheet)) return;
        ev.preventDefault?.();
        const startX = Number(ev.clientX || 0);
        const startY = Number(ev.clientY || 0);
        let curLeft = initialLeft;
        let curTop = initialTop;
        const onMove = (mv) => {
          curLeft = Math.max(0, Math.min(MAX_POS, Math.round(initialLeft + Number(mv.clientX || 0) - startX)));
          curTop = Math.max(0, Math.min(MAX_POS, Math.round(initialTop + Number(mv.clientY || 0) - startY)));
          cardEl.style.left = curLeft + "px";
          cardEl.style.top = curTop + "px";
        };
        const onUp = () => {
          document.removeEventListener("mousemove", onMove);
          document.removeEventListener("mouseup", onUp);
          if (curLeft !== initialLeft || curTop !== initialTop) {
            void updateSlicerMeta(table, slicerIdx, { left: curLeft, top: curTop }, "슬라이서 이동");
          }
        };
        document.addEventListener("mousemove", onMove);
        document.addEventListener("mouseup", onUp);
      });
    }

    function renderCard(sheet, table, slicer, slicerIdx) {
      const b = layoutApi.tableBox(table);
      const absCol = b.c1 + slicer.col;
      const cond = (table.filter || []).find((c) => c.col === absCol);
      const listed = Boolean(cond && (Array.isArray(cond.values) || cond.blank !== undefined));
      const picked = listed ? new Set(cond.values || []) : null;
      const pickedBlank = listed ? Boolean(cond.blank) : true;

      const iconsApi = host.__EASYOFFICE_ICONS__;
      const card = document.createElement("div");
      card.className = "sheet-slicer-card" + (listed ? " is-filtered" : "");
      card.dataset.slicerTable = table.name;
      card.dataset.slicerCol = String(slicer.col);
      card.tabIndex = 0;
      card.style.left = slicer.left + "px";
      card.style.top = slicer.top + "px";
      card.style.width = slicer.width + "px";
      card.style.height = slicer.height + "px";
      card.setAttribute("role", "region");
      card.setAttribute("aria-label", "슬라이서: " + slicer.caption);
      card.addEventListener("keydown", (ev) => {
        if ((ev.key === "Delete" || ev.key === "Backspace") && ev.target === card) {
          ev.preventDefault?.();
          void removeSlicer(table, slicerIdx);
        } else if (ev.altKey && (ev.key === "s" || ev.key === "S")) {
          ev.preventDefault?.();
          void updateSlicerMeta(table, slicerIdx, { multiSelect: !slicer.multiSelect }, "슬라이서 다중 선택");
        } else if (ev.altKey && (ev.key === "c" || ev.key === "C") && cond) {
          ev.preventDefault?.();
          void filter.setCriteria(absCol, null, table.name);
        }
      });

      const head = document.createElement("div");
      head.className = "sheet-slicer-head";
      const title = document.createElement("span");
      title.className = "sheet-slicer-title";
      title.textContent = slicer.caption;
      head.appendChild(title);

      const actions = document.createElement("div");
      actions.className = "sheet-slicer-actions";

      const multiBtn = document.createElement("button");
      multiBtn.type = "button";
      multiBtn.className = "sheet-slicer-btn" + (slicer.multiSelect ? " active" : "");
      multiBtn.dataset.slicerAct = "multi";
      multiBtn.title = "다중 선택 (Alt+S)";
      multiBtn.setAttribute("aria-label", "다중 선택");
      multiBtn.setAttribute("aria-pressed", slicer.multiSelect ? "true" : "false");
      if (iconsApi?.setIcon) iconsApi.setIcon(document, multiBtn, "slicer-multi");
      multiBtn.addEventListener("click", () => {
        void updateSlicerMeta(table, slicerIdx, { multiSelect: !slicer.multiSelect }, "슬라이서 다중 선택");
      });

      const clearBtn = document.createElement("button");
      clearBtn.type = "button";
      clearBtn.className = "sheet-slicer-btn" + (cond ? " is-clearable" : "");
      clearBtn.dataset.slicerAct = "clear";
      clearBtn.title = "필터 해제 (Alt+C)";
      clearBtn.setAttribute("aria-label", "필터 해제");
      clearBtn.disabled = !cond;
      if (iconsApi?.setIcon) iconsApi.setIcon(document, clearBtn, "filter-clear");
      clearBtn.addEventListener("click", () => {
        void filter.setCriteria(absCol, null, table.name);
      });

      const removeBtn = document.createElement("button");
      removeBtn.type = "button";
      removeBtn.className = "sheet-slicer-btn sheet-slicer-remove";
      removeBtn.dataset.slicerAct = "remove";
      removeBtn.title = "슬라이서 삭제 (Delete)";
      removeBtn.setAttribute("aria-label", "슬라이서 삭제");
      if (iconsApi?.setIcon) iconsApi.setIcon(document, removeBtn, "close");
      removeBtn.addEventListener("click", () => {
        void removeSlicer(table, slicerIdx);
      });

      actions.appendChild(multiBtn);
      actions.appendChild(clearBtn);
      actions.appendChild(removeBtn);
      head.appendChild(actions);
      card.appendChild(head);
      bindDrag(head, card, table, slicerIdx, slicer.left, slicer.top);

      const body = document.createElement("div");
      body.className = "sheet-slicer-items";
      const items = collectItems(sheet, table, absCol);
      for (const item of items) {
        const btn = document.createElement("button");
        btn.type = "button";
        const isSelected = !listed || (item.isBlank ? pickedBlank : picked.has(item.key));
        btn.className = "sheet-slicer-item" + (isSelected ? " selected" : "") + (!item.hasData ? " is-muted" : "");
        btn.dataset.slicerValue = item.key;
        btn.setAttribute("aria-pressed", isSelected ? "true" : "false");
        btn.textContent = item.label;
        btn.addEventListener("click", (ev) => {
          void applyItemClick(sheet, table, slicerIdx, item, Boolean(ev.ctrlKey || ev.metaKey));
        });
        body.appendChild(btn);
      }
      card.appendChild(body);
      return card;
    }

    function syncLayer() {
      const layer = $("sheet-slicer-layer");
      if (!layer) return;
      if (getActiveModule && getActiveModule() !== "SHEET") {
        layer.hidden = true;
        return;
      }
      const sheet = getActiveSheetName();
      const cards = [];
      for (const table of layoutApi.tablesOf(sheet)) {
        if (table.header === false || !Array.isArray(table.slicers)) continue;
        table.slicers.forEach((slicer, idx) => {
          if (slicer && slicer.col >= 0 && slicer.col < table.columns.length) {
            cards.push(renderCard(sheet, table, slicer, idx));
          }
        });
      }
      layer.replaceChildren(...cards);
      layer.hidden = cards.length === 0;
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      $("btn-sheet-slicer-ok")?.addEventListener("click", submitInsert);
      filter.onChange(syncLayer);
      layoutApi.onChange(syncLayer);
    }

    return { openInsert, syncLayer, ensureBound };
  }

  host.__EASYOFFICE_CREATE_SHEET_SLICER_UI__ = createSheetSlicerUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
