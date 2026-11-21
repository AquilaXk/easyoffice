import assert from "node:assert/strict";

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
    await input.click();
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
    await input.click();
    await input.fill("스크롤 전에 쓴 값");
    await scrollToFraction(page, 0.8);
    await page.waitForFunction(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[3].values.f1 === "스크롤 전에 쓴 값");
  }],

  ["Tab walks down through the rows beyond the window and scrolls them into view", async (page) => {
    await load(page, 10000);
    await page.locator('tr[data-record-id="r0"] td[data-field-id="f1"] input').click();
    let rowIndex = 0;
    for (let presses = 0; presses < 1500 && rowIndex < 80; presses += 1) {
      await page.keyboard.press("Tab");
      const at = await page.evaluate(() => {
        const tr = document.activeElement?.closest("tr.base-grid-row");
        return tr ? Number(tr.getAttribute("aria-rowindex")) - 2 : -1;
      });
      assert.ok(at >= rowIndex, `Tab went from row ${rowIndex} to ${at} (${await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 80))})`);
      rowIndex = Math.max(rowIndex, at);
    }
    assert.ok(rowIndex >= 80, `Tab reached row ${rowIndex}`);
    const visible = await page.evaluate(() => {
      const view = document.querySelector(".base-grid-wrap").getBoundingClientRect(), r = document.activeElement.getBoundingClientRect();
      return r.top >= view.top && r.bottom <= view.bottom;
    });
    assert.ok(visible, "the focused control is in view");
    for (let presses = 0; presses < 30; presses += 1) await page.keyboard.press("Shift+Tab");
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
];
