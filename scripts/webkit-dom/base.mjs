import assert from "node:assert/strict";
import { paste } from "./support.mjs";

// The BASE steps of the browser suite (PD-290): the rows of the datasheet in a real layout. Run by `run.mjs` in WebKit, and in Chromium
// with `EO_BROWSER=chromium` (and `EO_CHROMIUM` for a browser binary of its own).

const ROW_PX = 24;
const FIELDS = [
  { id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number" }, { id: "f3", name: "날짜", type: "date" },
  { id: "f4", name: "상태", type: "select", options: ["대기", "진행", "완료"] }, { id: "f5", name: "완료", type: "checkbox" },
  { id: "f6", name: "고객", type: "relation", relationTableId: "tc" }, { id: "f7", name: "합계", type: "formula", formula: "{수량} * 2" },
  { id: "f8", name: "태그", type: "multiSelect", options: ["a", "b", "c"] }, { id: "f9", name: "주소", type: "url" },
];

/**
 * Puts a table of `count` records in the studio and returns the milliseconds the redraw took. `relation` is "half" (every other record
 * is linked: a linked row is taller than an empty one, so the rows have two heights), "all" or "none" (every row is the 24 px of the style sheet).
 */
function load(page, count, relation = "half") {
  return page.evaluate(([n, fields, link]) => {
    const records = [];
    for (let i = 0; i < n; i += 1) {
      const linked = link === "all" || (link === "half" && i % 2 === 0);
      records.push({ id: "r" + i, values: { f1: "항목 " + i, f2: i, f3: "2026-11-" + String(1 + (i % 28)).padStart(2, "0"), f4: ["대기", "진행", "완료"][i % 3], f5: i % 2 === 0, f6: linked ? ["c" + (i % 20)] : [], f8: i % 3 === 0 ? ["a", "b"] : ["c"], f9: "https://example.com/" + i } });
    }
    const customers = { id: "tc", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: Array.from({ length: 20 }, (_, i) => ({ id: "c" + i, values: { c1: "고객 " + i } })) };
    const started = performance.now();
    const res = window.__EASYOFFICE_BASE_STUDIO__.setState({ version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }, customers] });
    if (!res.ok) throw new Error(res.error);
    return performance.now() - started;
  }, [count, FIELDS, relation]);
}

const settle = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

const scrollTo = (page, px) => page.evaluate(async ([y, wait]) => {
  const wrap = document.querySelector(".base-grid-wrap");
  wrap.scrollTop = y;
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  return wrap.scrollTop;
}, [px]);

/** Scrolls to the place `fraction` (0..1) of the way down the list. */
const scrollToFraction = (page, fraction) => page.evaluate(async (f) => {
  const wrap = document.querySelector(".base-grid-wrap");
  wrap.scrollTop = f * (wrap.scrollHeight - wrap.clientHeight);
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
}, fraction);

/** Scrolls until the row of record `index` is in the DOM (the first guess uses the mean height, the next ones correct it). */
const revealRow = (page, index) => page.evaluate(async (i) => {
  const wrap = document.querySelector(".base-grid-wrap");
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const tr = document.querySelector(`tr[data-record-id="r${i}"]`);
    if (tr) { tr.scrollIntoView({ block: "center" }); await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))); return true; }
    wrap.scrollTop = (i / 10000) * wrap.scrollHeight - wrap.clientHeight / 2;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  }
  return false;
}, index);

/** The rows in the DOM and what they cover of the view: `gap` is the px of the view that no row covers. */
const windowOf = (page) => page.evaluate(() => {
  const wrap = document.querySelector(".base-grid-wrap"), view = wrap.getBoundingClientRect();
  const head = wrap.querySelector("thead").getBoundingClientRect().height, foot = wrap.querySelector("tfoot").getBoundingClientRect().height;
  const rows = [...document.querySelectorAll("tr.base-grid-row")], rects = rows.map((tr) => tr.getBoundingClientRect());
  const indexes = rows.map((tr) => Number(tr.getAttribute("aria-rowindex")) - 2);
  const pitches = rects.slice(1).map((r, i) => r.top - rects[i].top);
  return {
    rows: rows.length, first: indexes[0], last: indexes.at(-1), contiguous: indexes.every((v, i) => v === indexes[0] + i),
    pitchMin: Math.min(...pitches), pitchMax: Math.max(...pitches), scrollHeight: wrap.scrollHeight, scrollTop: wrap.scrollTop,
    topGap: Math.max(rects[0].top - (view.top + head), 0),
    // After the last record come the new record row and the footer: they are what shows below it, not a gap.
    bottomGap: indexes.at(-1) === Number(wrap.querySelector("table").getAttribute("aria-rowcount")) - 4 ? 0 : Math.max(view.bottom - foot - rects.at(-1).bottom, 0),
    recordIds: rows.map((tr) => tr.dataset.recordId),
  };
});

const columnWidths = (page) => page.$$eval(".base-grid-table thead th", (ths) => ths.map((th) => th.getBoundingClientRect().width));

/** The cell of record `index` and field `field` (the row has to be in the window). */
const cellOf = (page, index, field) => page.locator(`tr[data-record-id="r${index}"] td[data-field-id="${field}"]`);

/** The active cell (PD-294): its record and field, whether it shows between the sticky header and the sticky footer, and what has the focus. */
const activeInfo = (page) => page.evaluate(() => {
  const td = document.querySelector("td[data-sel~='a']"), wrap = document.querySelector(".base-grid-wrap");
  if (!td) return null;
  const box = td.getBoundingClientRect(), view = wrap.getBoundingClientRect();
  const head = wrap.querySelector("thead").getBoundingClientRect().height, foot = wrap.querySelector("tfoot").getBoundingClientRect().height;
  return {
    rec: td.parentElement.dataset.recordId, field: td.dataset.fieldId, focus: document.activeElement.tagName, scrollTop: wrap.scrollTop,
    inView: box.top >= view.top + head - 0.5 && box.bottom <= view.bottom - foot + 0.5, selected: document.querySelectorAll("td[data-sel]").length,
  };
});

const recordValues = (page, index) => page.evaluate((i) => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[i].values, index);

export const baseSteps = [
  ["10,000 records are a few dozen rows in the DOM, and the redraw is fast", async (page) => {
    const ms = await load(page, 10000);
    const w = await windowOf(page);
    assert.ok(w.rows > 15 && w.rows < 90, `rows in the DOM: ${w.rows}`);
    assert.equal(w.first, 0);
    assert.ok(w.contiguous);
    assert.equal(await page.$eval("table.base-grid-table", (t) => t.getAttribute("aria-rowcount")), "10003");
    assert.ok(ms < 1500, `redraw of 10,000 records: ${ms.toFixed(0)} ms`);
    console.log(`     10,000 records: ${ms.toFixed(0)} ms`);
  }],

  ["rows of one height keep their pitch, and the list keeps its height", async (page) => {
    await load(page, 10000, "none");
    const w0 = await windowOf(page), P = w0.pitchMin;
    assert.equal(w0.pitchMax, P, "every row has one height");
    for (const y of [1, 25, 240, 5000 * P + 7, 5000 * P + 8, 123456, 9000 * P, 10000 * P]) {
      await scrollTo(page, y);
      const w = await windowOf(page);
      assert.ok(w.contiguous && w.pitchMin === P && w.pitchMax === P, `pitch ${w.pitchMin}..${w.pitchMax} at ${y}`);
      assert.ok(w.topGap < 1 && w.bottomGap < 1, `gap at ${y}: top ${w.topGap}, bottom ${w.bottomGap}`);
      assert.ok(Math.abs(w.scrollHeight - w0.scrollHeight) < 1, `the height of the list moved: ${w.scrollHeight} against ${w0.scrollHeight}`);
    }
    assert.equal((await windowOf(page)).last, 9999, "the end of the list shows the last record");
  }],

  ["the rows of a table of text fields are the 24 px of the style sheet, from the first window to the end", async (page) => {
    await page.evaluate(() => {
      const records = Array.from({ length: 10000 }, (_, i) => ({ id: "r" + i, values: { f1: "항목 " + i, f2: i } }));
      window.__EASYOFFICE_BASE_STUDIO__.setState({ version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields: [{ id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number" }], records }] });
    });
    const w0 = await windowOf(page);
    assert.equal(w0.pitchMin, ROW_PX);
    assert.equal(w0.pitchMax, ROW_PX);
    await scrollTo(page, 7777 * ROW_PX + 5);
    const w = await windowOf(page);
    assert.ok(w.pitchMin === ROW_PX && w.pitchMax === ROW_PX && w.topGap < 1 && w.bottomGap < 1);
    assert.ok(Math.abs(w.scrollHeight - w0.scrollHeight) < 1);
    await scrollTo(page, 10000 * ROW_PX);
    assert.equal((await windowOf(page)).last, 9999);
  }],

  ["rows of two heights leave no gap wherever the scroll goes, and a scroll up does not jump", async (page) => {
    await load(page, 10000, "half");
    const w0 = await windowOf(page);
    assert.ok(w0.pitchMax - w0.pitchMin > 5, `the rows have two heights: ${w0.pitchMin}, ${w0.pitchMax}`);
    for (const f of [0.5, 0.9, 0.2, 1, 0.75, 0.01, 0.6]) {
      await scrollToFraction(page, f);
      const w = await windowOf(page);
      assert.ok(w.contiguous && w.topGap < 1 && w.bottomGap < 1, `gap at ${f}: top ${w.topGap}, bottom ${w.bottomGap}`);
      if (f === 1) assert.equal(w.last, 9999, "the end of the list shows the last record");
    }
    // A step up of a few rows: the record under the top of the view moves up by the rows of the step, never by a jump.
    await scrollToFraction(page, 0.6);
    for (let step = 0; step < 40; step += 1) {
      const moved = await page.evaluate(async () => {
        const wrap = document.querySelector(".base-grid-wrap"), probe = () => {
          const y = wrap.getBoundingClientRect().top + wrap.querySelector("thead").getBoundingClientRect().height + 2;
          return Number(document.elementFromPoint(wrap.getBoundingClientRect().left + 200, y).closest("tr.base-grid-row")?.dataset.recordId.slice(1));
        };
        const before = probe();
        wrap.scrollTop -= 100;
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        return before - probe();
      });
      assert.ok(moved >= 0 && moved <= 6, `step ${step}: the record under the top of the view moved by ${moved} rows`);
    }
  }],

  ["the columns keep the widths they have when the table is short", async (page) => {
    await load(page, 100);
    const small = await columnWidths(page);
    await load(page, 10000);
    const long = await columnWidths(page);
    assert.equal(long.length, small.length);
    // The first column holds the footer text (`개수: 100건`), which is longer for 10,000 records; the others do not depend on the rows.
    long.slice(1).forEach((w, i) => assert.ok(Math.abs(w - small[i + 1]) <= 1, `column ${i + 1}: ${w} against ${small[i + 1]}`));
    assert.ok(long[0] >= 56);
    await scrollToFraction(page, 0.7);
    const after = await columnWidths(page);
    after.forEach((w, i) => assert.ok(Math.abs(w - long[i]) < 0.5, `column ${i} moved while scrolling: ${w} against ${long[i]}`));
  }],

  ["an edit far down the table keeps the scroll offset, the window and the value", async (page) => {
    await load(page, 10000);
    assert.ok(await revealRow(page, 5003));
    const input = page.locator('tr[data-record-id="r5003"] td[data-field-id="f1"] input');
    const topBefore = await page.$eval('tr[data-record-id="r5003"]', (tr) => tr.getBoundingClientRect().top);
    await page.locator('tr[data-record-id="r5003"] td[data-field-id="f1"]').click();
    await page.keyboard.press("Enter");
    await input.fill("편집된 값");
    await page.keyboard.press("Tab");
    await page.waitForFunction(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[5003].values.f1 === "편집된 값");
    const w = await windowOf(page);
    assert.ok(w.first <= 5003 && w.last >= 5003, `window ${w.first}..${w.last}`);
    assert.equal(await page.inputValue('tr[data-record-id="r5003"] td[data-field-id="f1"] input'), "편집된 값");
    const topAfter = await page.$eval('tr[data-record-id="r5003"]', (tr) => tr.getBoundingClientRect().top);
    assert.ok(Math.abs(topAfter - topBefore) < 2, `the row moved from ${topBefore} to ${topAfter}`);
  }],

  ["a text that is typed in a row and scrolled away from is committed", async (page) => {
    await load(page, 10000);
    const input = page.locator('tr[data-record-id="r3"] td[data-field-id="f1"] input');
    await page.locator('tr[data-record-id="r3"] td[data-field-id="f1"]').dblclick();
    await input.fill("스크롤 전에 쓴 값");
    await scrollToFraction(page, 0.8);
    await page.waitForFunction(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[3].values.f1 === "스크롤 전에 쓴 값");
  }],

  ["the arrow keys walk down through the rows beyond the window and scroll them into view", async (page) => {
    await load(page, 10000);
    await cellOf(page, 0, "f1").click();
    for (let presses = 1; presses <= 120; presses += 1) {
      await page.keyboard.press("ArrowDown");
      const at = await activeInfo(page);
      assert.equal(at?.rec, "r" + presses, `after ${presses} presses`);
      assert.ok(at.inView, `row ${presses} is under a sticky part or outside the view`);
    }
    for (let presses = 0; presses < 30; presses += 1) await page.keyboard.press("ArrowUp");
    assert.equal((await activeInfo(page)).rec, "r90");
    assert.ok((await windowOf(page)).contiguous);
  }],

  ["sort, filter and group show their result in the window", async (page) => {
    await load(page, 10000);
    await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.setSorts([{ fieldId: "f2", dir: "desc" }]));
    assert.equal((await windowOf(page)).recordIds[0], "r9999");
    await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.setFilters([{ fieldId: "f4", op: "eq", value: "완료" }], "and"));
    assert.match(await page.textContent(".base-footer-count"), /3333 \/ 10000/);
    await page.evaluate(() => { const s = window.__EASYOFFICE_BASE_STUDIO__; s.setFilters([], "and"); s.setSorts([]); s.setGroupBy("f4"); });
    assert.equal(await page.locator("tr.base-group-row").count(), 1, "the first group row; the others are far below");
    // The second group row is item 3,334 of 10,003: a third of the way down.
    for (let attempt = 0; attempt < 6 && (await page.locator("tr.base-group-row").count()) < 1; attempt += 1) await scrollToFraction(page, 0.3334 + attempt * 0.005);
    await page.waitForSelector("tr.base-group-row");
    assert.ok((await windowOf(page)).contiguous);
  }],

  ["a taller view takes more rows", async (page) => {
    await load(page, 10000);
    const before = (await windowOf(page)).rows;
    await page.setViewportSize({ width: 1100, height: 2600 });
    await page.waitForFunction((n) => document.querySelectorAll("tr.base-grid-row").length > n, before).catch(async () => {
      assert.fail(`rows ${before} before and ${(await windowOf(page)).rows} after, view ${await page.$eval(".base-grid-wrap", (w) => w.clientHeight)} px`);
    });
    const w = await windowOf(page);
    assert.ok(w.topGap < 1 && w.bottomGap < 1, `gap: top ${w.topGap}, bottom ${w.bottomGap}`);
  }],

  // ---- the selection of cells (PD-294) ----

  ["a click selects a cell with a border, a double click edits it, Esc leaves it, Shift+click selects a range", async (page) => {
    await load(page, 10000);
    await cellOf(page, 3, "f2").click();
    let at = await activeInfo(page);
    assert.deepEqual([at.rec, at.field, at.focus, at.selected], ["r3", "f2", "TABLE", 1]);
    const border = await page.$eval("td[data-sel~='a']", (td) => { const s = getComputedStyle(td); return [s.outlineStyle, s.outlineWidth]; });
    assert.deepEqual(border, ["solid", "2px"], "the active cell has a border");
    assert.equal(await page.$eval("td[data-sel~='a'] input", (el) => el.readOnly), true, "a click does not edit");
    await cellOf(page, 3, "f2").dblclick();
    assert.equal(await page.evaluate(() => document.activeElement.className), "base-cell-input base-cell-num");
    assert.equal(await page.evaluate(() => document.activeElement.readOnly), false);
    await page.keyboard.press("Escape");
    assert.equal((await activeInfo(page)).focus, "TABLE");
    await cellOf(page, 6, "f4").click({ modifiers: ["Shift"] });
    at = await activeInfo(page);
    assert.deepEqual([at.rec, at.field, at.selected], ["r3", "f2", 4 * 3], "the range runs from the active cell: rows 3..6, fields 2..4");
    const edges = await page.$$eval("td[data-sel]", (tds) => tds.map((td) => td.getAttribute("data-sel")).filter((s) => s.includes("t") || s.includes("b") || s.includes("l") || s.includes("r")).length);
    assert.equal(edges, 10, "the cells on the four sides of the range");
  }],

  ["a drag selects a range and scrolls the list when the pointer leaves it below", async (page) => {
    await load(page, 10000);
    const from = await cellOf(page, 2, "f1").boundingBox(), to = await cellOf(page, 4, "f3").boundingBox();
    await page.mouse.move(from.x + 8, from.y + 5);
    await page.mouse.down();
    await page.mouse.move(to.x + 8, to.y + 5, { steps: 6 });
    assert.equal((await activeInfo(page)).selected, 9);
    const view = await page.$eval(".base-grid-wrap", (w) => { const r = w.getBoundingClientRect(); return { bottom: r.bottom, x: r.left + 200 }; });
    for (let i = 0; i < 12; i += 1) await page.mouse.move(view.x, view.bottom + 6 + i);
    await page.mouse.up();
    const at = await activeInfo(page);
    assert.equal(at.rec, "r2", "the active cell stays");
    assert.ok(at.scrollTop > 0, "the list scrolled");
    assert.ok(at.selected > 9, `the range grew: ${at.selected}`);
  }],

  ["the keys move the selection through 10,000 rows of two heights, and the cell shows after each key", async (page) => {
    await load(page, 10000, "half");
    await cellOf(page, 0, "f1").click();
    let last = 0;
    for (let presses = 0; presses < 6; presses += 1) {
      await page.keyboard.press("PageDown");
      const at = await activeInfo(page), n = Number(at.rec.slice(1));
      assert.ok(n > last + 3, `PageDown went from ${last} to ${n}`);
      assert.ok(at.inView && at.focus === "TABLE", `row ${n} is in view after PageDown`);
      last = n;
    }
    await page.keyboard.press("PageUp");
    assert.ok(Number((await activeInfo(page)).rec.slice(1)) < last);
    await page.keyboard.press("Control+ArrowDown");
    let at = await activeInfo(page);
    assert.equal(at.rec, "r9999");
    assert.ok(at.inView, "the last row is in view");
    assert.ok(await page.$eval(".base-grid-wrap", (w) => w.scrollTop + w.clientHeight >= w.scrollHeight - 1), "the list is at its end");
    const w = await windowOf(page);
    assert.ok(w.contiguous && w.last === 9999 && w.rows < 90, `window ${w.first}..${w.last}, ${w.rows} rows`);
    await page.keyboard.press("Control+ArrowUp");
    at = await activeInfo(page);
    assert.deepEqual([at.rec, at.scrollTop], ["r0", 0]);
    await page.keyboard.press("End");
    assert.equal((await activeInfo(page)).field, "f9");
    await page.keyboard.press("Home");
    assert.equal((await activeInfo(page)).field, "f1");
    await page.keyboard.press("Control+ArrowRight");
    assert.equal((await activeInfo(page)).field, "f9", "r0 has a value in every field");
    await page.keyboard.press("Control+End");
    at = await activeInfo(page);
    assert.deepEqual([at.rec, at.field, at.inView], ["r9999", "f9", true]);
    await page.keyboard.press("Control+Home");
    assert.deepEqual([(await activeInfo(page)).rec, (await activeInfo(page)).field], ["r0", "f1"]);
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowRight");
    assert.equal((await activeInfo(page)).selected, 4);
  }],

  ["a typed key edits the cell, Enter commits and goes down, Esc cancels, Delete empties a range", async (page) => {
    await load(page, 10000);
    await cellOf(page, 10, "f1").click();
    await page.keyboard.type("abc");
    assert.equal(await page.evaluate(() => document.activeElement.value), "abc", "the first key replaces the text, the others follow it");
    await page.keyboard.press("Enter");
    assert.equal((await recordValues(page, 10)).f1, "abc");
    let at = await activeInfo(page);
    assert.deepEqual([at.rec, at.field, at.focus], ["r11", "f1", "TABLE"]);
    await page.keyboard.press("F2");
    await page.keyboard.type("zzz");
    assert.equal(await page.evaluate(() => document.activeElement.value), "항목 11zzz");
    await page.keyboard.press("Escape");
    assert.equal((await recordValues(page, 11)).f1, "항목 11");
    assert.equal((await activeInfo(page)).focus, "TABLE");
    await cellOf(page, 12, "f1").click();
    await cellOf(page, 13, "f2").click({ modifiers: ["Shift"] });
    await page.keyboard.press("Delete");
    for (const i of [12, 13]) assert.deepEqual([(await recordValues(page, i)).f1, (await recordValues(page, i)).f2], ["", null]);
    assert.equal((await recordValues(page, 14)).f1, "항목 14");
    at = await activeInfo(page);
    assert.deepEqual([at.rec, at.field, at.selected, at.focus], ["r12", "f1", 4, "TABLE"], "the range stays after the edit");
    await cellOf(page, 20, "f5").click();
    await page.keyboard.press("Space");
    assert.equal((await recordValues(page, 20)).f5, false, "r20 was checked (even), Space unchecks it");
  }],

  ["Ctrl+C puts the cells on the clipboard as TSV, a paste writes a block, and the selection goes with it", async (page) => {
    await load(page, 10000);
    await page.evaluate(() => { window.__copied = null; document.addEventListener("copy", (ev) => { window.__copied = ev.clipboardData.getData("text/plain"); }); });
    await cellOf(page, 1, "f1").click();
    await cellOf(page, 2, "f2").click({ modifiers: ["Shift"] });
    await page.keyboard.press("ControlOrMeta+C");
    assert.equal(await page.evaluate(() => window.__copied), "항목 1\t1\n항목 2\t2");
    await cellOf(page, 5, "f1").click();
    assert.equal(await paste(page, { "text/plain": "새 A\t90\n새 \"B\"\t91\r\n" }), true);
    assert.deepEqual([(await recordValues(page, 5)).f1, (await recordValues(page, 5)).f2, (await recordValues(page, 6)).f1, (await recordValues(page, 6)).f2], ["새 A", 90, '새 "B"', 91]);
    const at = await activeInfo(page);
    assert.deepEqual([at.rec, at.selected, at.focus], ["r5", 4, "TABLE"]);
  }],

  ["the grid is one tab stop: Tab enters it, walks the cells, and leaves it after the last one", async (page) => {
    await load(page, 10000);
    await page.locator("#base-view-tabs button").last().focus();
    await page.keyboard.press("Tab");
    assert.equal(await page.evaluate(() => document.activeElement.tagName), "TABLE");
    let at = await activeInfo(page);
    assert.deepEqual([at.rec, at.field], ["r0", "f1"], "the first cell is selected when the table gets the focus");
    for (let presses = 0; presses < 8; presses += 1) await page.keyboard.press("Tab");
    at = await activeInfo(page);
    assert.deepEqual([at.rec, at.field, at.focus], ["r0", "f9", "TABLE"]);
    await page.keyboard.press("Tab");
    // The focus goes on to what follows the table (the buttons of the header, in a browser that tabs to buttons): no control of a cell takes it, and the selection stays.
    assert.ok(!(await page.evaluate(() => /^(INPUT|SELECT)$/.test(document.activeElement.tagName) || document.activeElement.closest("td.base-grid-cell") !== null)));
    assert.deepEqual([(await activeInfo(page)).rec, (await activeInfo(page)).field], ["r0", "f9"]);
    await page.keyboard.press("Shift+Tab");
    assert.equal(await page.evaluate(() => document.activeElement.tagName), "TABLE", "Shift+Tab comes back to the grid");
    assert.equal(await page.$eval("table.base-grid-table", (t) => t.getAttribute("aria-activedescendant")), await page.$eval("td[data-sel~='a']", (td) => td.id));
  }],

  ["a key and a page key are fast on 10,000 records", async (page) => {
    await load(page, 10000, "half");
    await cellOf(page, 0, "f1").click();
    const times = await page.evaluate(async () => {
      const frame = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const median = (list) => list.slice().sort((a, b) => a - b)[list.length >> 1];
      const measure = async (key, times) => {
        const sync = [], full = [];
        for (let i = 0; i < times; i += 1) {
          const tbl = document.querySelector("table.base-grid-table"), started = performance.now();
          tbl.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
          sync.push(performance.now() - started);
          await frame();
          full.push(performance.now() - started);
        }
        return { sync: median(sync), full: median(full) };
      };
      return { arrow: await measure("ArrowDown", 40), page: await measure("PageDown", 40) };
    });
    console.log(`     ArrowDown: ${times.arrow.sync.toFixed(2)} ms, to the frame ${times.arrow.full.toFixed(1)} ms; PageDown: ${times.page.sync.toFixed(2)} ms, to the frame ${times.page.full.toFixed(1)} ms`);
    assert.ok(times.arrow.sync < 20 && times.page.sync < 60, `${JSON.stringify(times)}`);
    assert.ok((await windowOf(page)).contiguous);
  }],

  ["column and row resizer drags, offscreen best-fit dblclick, row height PageDown/Ctrl+End, and reset work in live layout (PD-314)", async (page) => {
    await page.evaluate(() => {
      const fields = [{ id: "f1", name: "이름", type: "text", width: 144 }, { id: "f2", name: "수량", type: "number", width: 120 }];
      const records = Array.from({ length: 10000 }, (_, i) => ({
        id: "r" + i,
        values: { f1: i === 8500 ? "오프스크린_긴_제목_" + "가".repeat(32) : "항목 " + i, f2: i + 1 },
      }));
      const res = window.__EASYOFFICE_BASE_STUDIO__.setState({ version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }] });
      if (!res.ok) throw new Error(res.error);
    });
    const initW = (await columnWidths(page))[1];
    assert.ok(Math.abs(initW - 144) <= 2, `explicit initial width respected: ${initW}`);
    const colBox = await page.locator('th[data-field-id="f1"] .col-resizer').boundingBox();
    await page.mouse.move(colBox.x + colBox.width / 2, colBox.y + colBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(colBox.x + colBox.width / 2 + 80, colBox.y + colBox.height / 2, { steps: 4 });
    await page.mouse.up();
    const draggedW = (await columnWidths(page))[1];
    const modelW = await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].fields[0].width);
    assert.ok(Math.abs(draggedW - modelW) <= 2 && modelW >= 215 && modelW <= 235, `dragged col width ${draggedW} / ${modelW}`);

    await page.locator('th[data-field-id="f1"] .col-resizer').dblclick();
    const fitW = await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].fields[0].width);
    const renderedFitW = (await columnWidths(page))[1];
    assert.ok(fitW > draggedW && fitW > 400 && Math.abs(renderedFitW - fitW) <= 2, `best-fit widened for offscreen r8500: ${fitW} (rendered ${renderedFitW})`);

    const rowBox = await page.locator('tr[data-record-id="r0"] .row-resizer').boundingBox();
    await page.mouse.move(rowBox.x + 12, rowBox.y + rowBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(rowBox.x + 12, rowBox.y + rowBox.height / 2 + 18, { steps: 4 });
    await page.mouse.up();
    const rh = await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].rowHeight);
    assert.ok(rh >= 40 && rh <= 45, `row height dragged to ${rh}`);
    const w = await windowOf(page);
    assert.ok(Math.abs(w.pitchMin - rh) < 1 && Math.abs(w.pitchMax - rh) < 1 && w.topGap < 1 && w.bottomGap < 1, `42px window gap check: ${JSON.stringify(w)}`);

    await cellOf(page, 0, "f1").click();
    await page.keyboard.press("PageDown");
    assert.ok((await activeInfo(page)).inView, "active cell remains clear of sticky header/footer after PageDown");
    await page.keyboard.press("Control+End");
    assert.deepEqual([(await activeInfo(page)).rec, (await activeInfo(page)).inView], ["r9999", true], "Control+End reveals last row cleanly");

    const resetBox = await page.locator('tr[data-record-id="r9999"] .row-resizer').boundingBox();
    await page.mouse.dblclick(resetBox.x + 12, resetBox.y + resetBox.height / 2);
    assert.equal(await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].rowHeight), undefined);
  }],

  ["in-app find and replace jumps to offscreen r8500 in 10,000 records, keeps query focus, and closes on Esc (PD-315)", async (page) => {
    await load(page, 10000);
    await page.evaluate(() => {
      const studio = window.__EASYOFFICE_BASE_STUDIO__;
      const st = studio.getState();
      st.tables[0].records[8500].values.f1 = "유일한_오프스크린_찾기_표적";
      studio.setState(st);
    });
    await page.keyboard.press("Control+f");
    await page.waitForSelector("#find-dialog[open]");
    await page.fill("#find-query", "오프스크린_찾기_표적");
    await page.waitForFunction(() => document.getElementById("find-count")?.textContent === "1 / 1");
    const info = await activeInfo(page);
    assert.equal(info?.rec, "r8500");
    assert.equal(info?.field, "f1");
    assert.equal(info?.inView, true, "r8500 scrolled clear of sticky chrome");
    assert.equal(await page.evaluate(() => document.activeElement?.id), "find-query");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !document.getElementById("find-dialog")?.open);

    await page.keyboard.press("Control+F2");
    await page.waitForSelector("#find-replacement", { state: "visible" });
    await page.fill("#find-replacement", "고친_오프스크린_값");
    await page.click("#btn-find-replace");
    await page.waitForFunction(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[8500].values.f1 === "유일한_고친_오프스크린_값");
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => !document.getElementById("find-dialog")?.open);
  }],

  ["multi-range cut, header selection, fill handle series, append paste, and sorted edit selection preservation work in live layout (PD-316)", async (page) => {
    const mod = process.platform === "darwin" ? "Meta" : "Control";
    await load(page, 10, "none");
    await page.locator('thead th[data-field-id="f2"]').click();
    assert.equal(await page.$eval('thead th[data-field-id="f2"]', (th) => th.getAttribute("data-sel-hdr")), "true");
    assert.deepEqual(await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.selectedRanges()), [{ r1: 1, r2: 10, c1: 2, c2: 2 }]);
    await page.locator('tr[data-record-id="r1"] td.base-row-index').click();
    assert.equal(await page.$eval('tr[data-record-id="r1"] td.base-row-index', (td) => td.getAttribute("data-sel-hdr")), "true");

    await cellOf(page, 0, "f1").click();
    await cellOf(page, 2, "f1").click({ modifiers: [mod] });
    assert.equal(await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.selectedRanges().length), 2);
    await page.keyboard.press("ControlOrMeta+x");
    assert.deepEqual([(await recordValues(page, 0)).f1, (await recordValues(page, 2)).f1], ["", ""]);

    await cellOf(page, 0, "f2").click();
    await cellOf(page, 1, "f2").click({ modifiers: ["Shift"] });
    const hBox = await page.locator(".base-fill-handle").boundingBox();
    const dstBox = await cellOf(page, 4, "f2").boundingBox();
    await page.mouse.move(hBox.x + hBox.width / 2, hBox.y + hBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(dstBox.x + dstBox.width / 2, dstBox.y + dstBox.height / 2, { steps: 4 });
    await page.mouse.up();
    assert.deepEqual([2, 3, 4], await page.evaluate(() => [2, 3, 4].map((i) => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[i].values.f2)));

    await page.locator("tr.base-new-record-row td.base-new-row-marker").click();
    assert.equal(await paste(page, { "text/plain": "추가 A\t50\n추가 B\t60" }), true);
    assert.equal(await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records.length), 12);

    await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.setSorts([{ fieldId: "f2", dir: "asc" }]));
    await cellOf(page, 5, "f2").click();
    await page.keyboard.press("Enter");
    await page.locator('tr[data-record-id="r5"] td[data-field-id="f2"] input').fill("999");
    await page.keyboard.press("Tab");
    const after = await activeInfo(page);
    assert.deepEqual([after?.rec, after?.field], ["r5", "f3"]);
  }],

  ["incremental query cache keeps 10,000-row cell edits fast and exact across sort, group, and relation updates (PD-317)", async (page) => {
    await load(page, 10000);
    await page.evaluate(() => {
      const studio = window.__EASYOFFICE_BASE_STUDIO__;
      studio.setSorts([{ fieldId: "f2", dir: "desc" }]);
      studio.setFilters([{ fieldId: "f2", op: "gte", value: "100" }], "and");
      studio.setGroupBy("f4");
    });
    const bench = await page.evaluate(() => {
      const studio = window.__EASYOFFICE_BASE_STUDIO__, M = window.__EASYOFFICE_BASE_MODEL__, samples = [];
      const cache = window.__EASYOFFICE_CREATE_BASE_QUERY_CACHE__(M);
      const st = studio.getState();
      cache.query(st, "t1");
      const qSamples = [];
      for (let i = 0; i < 8; i += 1) {
        M.updateCell(st, "t1", "r9000", "f2", 9000 + i + 1, cache.index(st));
        cache.noteUpdate(st, "t1", ["r9000"]);
        const q0 = performance.now();
        cache.query(st, "t1");
        qSamples.push(performance.now() - q0);
        const t0 = performance.now();
        studio.updateCell("r9000", "f2", 9000 + i + 1);
        samples.push(performance.now() - t0);
      }
      return {
        queryMedian: qSamples.sort((a, b) => a - b)[qSamples.length >> 1],
        studioMedian: samples.sort((a, b) => a - b)[samples.length >> 1],
      };
    });
    console.log(`     PD-317 10k filter+sort+group WebKit median: query=${bench.queryMedian.toFixed(2)} ms, studio.updateCell=${bench.studioMedian.toFixed(2)} ms`);
    assert.ok(bench.queryMedian < 3 && bench.studioMedian < 30, `expected fast 10k incremental edit, got ${JSON.stringify(bench)}`);
    const q = await page.evaluate(() => {
      const studio = window.__EASYOFFICE_BASE_STUDIO__;
      const lq = studio.liveQuery(), exact = window.__EASYOFFICE_BASE_MODEL__.queryTableView(studio.getState(), "t1");
      return { countOk: lq.filteredCount === exact.filteredCount, sumEq: lq.numericSums.f2 === exact.numericSums.f2, row0: lq.rows[0]?.record.id === exact.rows[0]?.record.id };
    });
    assert.deepEqual(q, { countOk: true, sumEq: true, row0: true });
  }],
];
