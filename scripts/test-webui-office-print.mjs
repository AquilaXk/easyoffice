import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// Printing from the document, sheet and slide modules (office-print.js): the hidden print root, the
// page rules, the module-specific content, and the cleanup on afterprint / failure.

class FakeStyleSheet {
  constructor() { this.cssRules = []; }
  insertRule(text, at) {
    this.cssRules.splice(at, 0, { cssText: text, style: { props: {}, setProperty(k, v) { this.props[k] = v; } } });
    return at;
  }
}

async function setup(moduleCode) {
  const h = await createWorkspaceHarness();
  const printed = [];
  h.host.CSSStyleSheet = FakeStyleSheet;
  h.host.requestAnimationFrame = (cb) => cb();
  h.host.getComputedStyle = () => ({ getPropertyValue: (p) => (p === "font-size" ? "11pt" : "") });
  h.host.print = () => { printed.push(h.document.getElementById("eo-print-root")); };
  h.document.adoptedStyleSheets = [];
  if (moduleCode !== "DOC") {
    switchModule(h.document, moduleCode);
    await flushAsyncTurns();
  }
  const print = async () => {
    h.document.defaultView.__EASYOFFICE_NATIVE_MENU__("file:print");
    await flushAsyncTurns(8);
  };
  const root = () => h.document.getElementById("eo-print-root");
  const pageRules = () => h.document.adoptedStyleSheets.flatMap((sheet) => sheet.cssRules.map((r) => r.cssText));
  const feedback = () => h.document.getElementById("status-command-feedback").textContent;
  return { h, printed, print, root, pageRules, feedback };
}

test("document: the editor content goes into the print root without ids or editing state, on A4 with margins", async () => {
  const t = await setup("DOC");
  t.h.document.getElementById("doc-editor").innerHTML = '<h2 id="x">제목</h2><p contenteditable="true">본문 <b>굵게</b></p><table><tbody><tr><td>칸</td></tr></tbody></table>';
  await t.print();
  assert.equal(t.printed.length, 1, "the window print ran once");
  const root = t.printed[0];
  assert.equal(root.querySelectorAll(".eo-print-flow").length, 1);
  assert.equal(root.querySelectorAll("[id]").length, 0, "no id repeats the live editor's");
  assert.equal(root.querySelectorAll("[contenteditable]").length, 0);
  assert.equal(root.textContent, "제목본문 굵게칸");
  assert.ok(t.pageRules().includes("@page { size: 210mm 297mm; margin: 20mm; }"));
  assert.equal(t.h.document.getElementById("doc-editor").querySelectorAll("#x").length, 1, "the live editor is untouched");
});

test("the print root and its page rules are removed on afterprint, and when the print fails", async () => {
  const t = await setup("DOC");
  await t.print();
  assert.ok(t.root(), "the root stays while the print dialog is up");
  assert.equal(t.pageRules().length > 0, true);
  t.h.sandbox.dispatchWindowEvent("afterprint");
  assert.equal(t.root(), null);
  assert.deepEqual(t.pageRules(), []);

  const failing = await setup("DOC");
  failing.h.host.print = () => { throw new Error("프린터 없음"); };
  await failing.print();
  assert.equal(failing.root(), null, "a failed print leaves nothing behind");
  assert.deepEqual(failing.pageRules(), []);
  assert.match(failing.feedback(), /프린터 없음/);
});

test("sheet: the used range becomes a table on A4 pages, wide ranges print landscape", async () => {
  const t = await setup("SHEET");
  await pickFile(
    t.h.document,
    makeFixtureFile("인쇄.xlsx", { sheets: [{ name: "Sheet1", cells: [{ ref: "B2", raw: "이름" }, { ref: "C2", raw: "값" }, { ref: "B3", raw: "가" }, { ref: "C3", raw: "1234.5" }, { ref: "D4", raw: "끝" }] }] }),
  );
  await flushAsyncTurns(12);
  await t.print();
  const [root] = t.printed;
  const rows = root.querySelectorAll("tr");
  assert.equal(rows.length, 3, "rows 2 to 4 only: the range starts at the first used cell");
  assert.deepEqual(rows[0].querySelectorAll("td").map((td) => td.textContent), ["이름", "값", ""]);
  assert.deepEqual(rows[2].querySelectorAll("td").map((td) => td.textContent), ["", "", "끝"]);
  assert.match(rows[1].querySelectorAll("td")[1].className, /eo-print-num/, "numbers align right");
  assert.ok(t.pageRules().includes("@page { size: 210mm 297mm; margin: 0; }"));
  assert.equal(root.querySelector(".eo-print-sheet-page").style.padding, "10mm 10mm 10mm 10mm", "the page keeps the margins of a sheet without a setup");
  t.h.sandbox.dispatchWindowEvent("afterprint");

  await pickFile(
    t.h.document,
    makeFixtureFile("넓음.xlsx", { sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "a" }, { ref: "J1", raw: "j" }] }] }),
  );
  await flushAsyncTurns(12);
  await t.print();
  assert.ok(t.pageRules().includes("@page { size: 297mm 210mm; margin: 0; }"), "ten columns print landscape");
});

test("sheet: alignment, wrapping and borders print, and a bordered empty cell is part of the range", async () => {
  const t = await setup("SHEET");
  const style = { ha: "center", va: "top", wrap: true, bd: { t: "double:ff0000", b: "thin:000000" } };
  await pickFile(
    t.h.document,
    makeFixtureFile("서식.xlsx", { sheets: [{ name: "Sheet1", cells: [{ ref: "B2", raw: "가", style }, { ref: "C2", raw: "", style: { bd: { l: "dashed:00ff00" } } }] }] })
  );
  await flushAsyncTurns(8);
  await t.print();
  const tds = t.printed[0].querySelectorAll("td");
  assert.equal(tds.length, 2, "the range is B2:C2 although C2 holds no text");
  assert.equal(tds[0].style.textAlign, "center");
  assert.equal(tds[0].style.verticalAlign, "top");
  assert.equal(tds[0].style.whiteSpace, "pre-wrap");
  assert.equal(tds[0].style.borderTopStyle, "double");
  assert.equal(tds[0].style.borderTopColor, "#ff0000");
  assert.equal(tds[0].style.borderBottomStyle, "solid");
  assert.equal(tds[1].style.borderLeftStyle, "dashed");
  assert.equal(tds[1].style.borderTopStyle, "", "an edge nobody set keeps the table's");
});

test("sheet: fills and text colors print, a filled empty cell is part of the range, and the print root keeps the colors (PD-227)", async () => {
  const t = await setup("SHEET");
  await pickFile(
    t.h.document,
    makeFixtureFile("색.xlsx", {
      sheets: [{ name: "Sheet1", cells: [
        { ref: "B2", raw: "가", style: { fill: "ffeb9c", fc: "9c5700" } },
        { ref: "C2", raw: "", style: { fill: "00b050" } },
        { ref: "D2", raw: "나", style: { fill: "000000" } },
        { ref: "E2", raw: "다", style: { fc: "ff0000" } },
      ] }],
    })
  );
  await flushAsyncTurns(8);
  await t.print();
  const tds = t.printed[0].querySelectorAll("td");
  assert.equal(tds.length, 4, "the range is B2:E2 although C2 holds no text");
  assert.equal(tds[0].style.backgroundColor, "#ffeb9c");
  assert.equal(tds[0].style.color, "#9c5700");
  assert.equal(tds[1].style.backgroundColor, "#00b050");
  assert.equal(tds[2].style.color, "#ffffff", "automatic text on a black fill prints white");
  assert.equal(tds[3].style.color, "#ff0000");
  assert.equal(tds[3].style.backgroundColor, "");
  const css = (await import("node:fs")).readFileSync(new URL("../crates/easyoffice-app/ui/shell.css", import.meta.url), "utf8");
  assert.match(css, /#eo-print-root \{[^}]*print-color-adjust: exact/);
});

test("sheet: a number format's text, color and fill print, and a text format prints the stored text (PD-234)", async () => {
  const t = await setup("SHEET");
  await pickFile(
    t.h.document,
    makeFixtureFile("서식표시.xlsx", {
      sheets: [{ name: "Sheet1", numFmts: ["0.0%", "#,##0;[Red]-#,##0", '_("$"* #,##0.00_)', "@"], cells: [
        { ref: "B2", raw: "0.256", style: { nf: 1 } },
        { ref: "B3", raw: "-1234.6", style: { nf: 2 } },
        { ref: "B4", raw: "7.5", style: { nf: 3 } },
        { ref: "B5", raw: "007", style: { nf: 4 } },
        { ref: "B6", raw: "0.30000000000000004" },
      ] }],
    })
  );
  await flushAsyncTurns(8);
  await t.print();
  assert.equal(t.printed.length, 1, t.feedback());
  const tds = [...t.printed[0].querySelectorAll("td")];
  assert.deepEqual(tds.map((td) => td.textContent), ["25.6%", "-1,235", " $7.50 ", "007", "0.3"]);
  assert.equal(tds[1].style.color, "#ff0000", "the [Red] section prints red");
  assert.equal(tds[0].style.color, "", "a section without a color keeps the cell's");
  assert.match(tds[0].className, /eo-print-num/);
  assert.doesNotMatch(tds[3].className, /eo-print-num/, "a text format is text, not a number");
  const fill = tds[2].querySelector(".eo-print-fill");
  assert.deepEqual([...fill.children].map((el) => el.textContent), [" $", "", "7.50 "]);
  assert.ok(fill.children[1].dataset.fill.length > 100 && fill.children[1].dataset.fill.trim() === "", "the fill repeats its character");
  t.h.sandbox.dispatchWindowEvent("afterprint");
});

test("sheet: an engine that does not answer leaves the cells in their General display", async () => {
  const t = await setup("SHEET");
  await pickFile(
    t.h.document,
    makeFixtureFile("답없음.xlsx", { sheets: [{ name: "Sheet1", numFmts: ["0.0%"], cells: [{ ref: "A1", raw: "0.256", style: { nf: 1 } }] }] })
  );
  await flushAsyncTurns(8);
  t.h.host.__EASYOFFICE_SHEET_NUMFMT_DISPLAY__.attach({ call: async () => { throw new Error("엔진 없음"); } });
  await t.print();
  assert.equal(t.printed[0].querySelector("td").textContent, "0.256", "the stored value shows when the engine does not answer");
});

test("sheet: an empty sheet and a range past the cell cap are refused with a message", async () => {
  const t = await setup("SHEET");
  await pickFile(t.h.document, makeFixtureFile("빈.xlsx", { sheets: [{ name: "Sheet1", cells: [] }] }));
  await flushAsyncTurns(12);
  await t.print();
  assert.equal(t.root(), null);
  assert.equal(t.feedback(), "인쇄할 내용이 없습니다.");

  await pickFile(t.h.document, makeFixtureFile("큼.xlsx", { sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "a" }, { ref: "Z9999", raw: "z" }] }] }));
  await flushAsyncTurns(12);
  await t.print();
  assert.equal(t.root(), null);
  assert.match(t.feedback(), /너무 커서/);
  assert.equal(t.printed.length, 0, "nothing reached the print dialog");
});

test("slides: one page per slide in the deck's paper, the title and body text of each", async () => {
  const t = await setup("SLIDE");
  await t.print();
  const [root] = t.printed;
  const pages = root.querySelectorAll(".eo-print-page");
  assert.ok(pages.length >= 1);
  for (const page of pages) {
    assert.equal(page.querySelectorAll(".slide-stage").length, 1);
    assert.equal(page.querySelectorAll(".slide-title-zone h2").length, 1);
  }
  const rules = t.pageRules();
  assert.ok(rules.some((r) => /^@page \{ size: 297mm 167mm; margin: 0; \}$/.test(r)), "16:9 paper");
  assert.ok(rules.some((r) => r.startsWith("@page eo-paper-0")), "a named page rule per paper size");
});

test("pdf: prints opened card text, stamp, watermark, rotation, and active form fields, and refuses empty PDF", async () => {
  const t = await setup("PDF");
  await t.print();
  assert.equal(t.root(), null);
  assert.equal(t.feedback(), "인쇄할 내용이 없습니다.");

  const doc = t.h.document;
  doc.getElementById("pane-pdf").dataset.empty = "false";
  const card = doc.getElementById("pdf-page-card");
  card.hidden = false;
  card.querySelector("h3").textContent = "PDF 보고서";
  const p = doc.createElement("p");
  p.textContent = "본문 1쪽 내용";
  doc.getElementById("pdf-form-status").before(p);
  doc.getElementById("pdf-stamp-badge").textContent = "대외비";
  doc.getElementById("pdf-watermark-layer").textContent = "사내한";
  doc.getElementById("pdf-rotation-badge").textContent = "회전: 90°";
  doc.getElementById("pdf-acroform-grid").hidden = false;
  doc.getElementById("pdf-form-applicant").value = "이서연";
  doc.getElementById("pdf-form-dept").value = "기획팀";

  await t.print();
  assert.equal(t.printed.length, 1, t.feedback());
  const page = t.printed[0].querySelector(".eo-print-pdf-page");
  assert.ok(page);
  assert.equal(page.dataset.rotation, "90");
  assert.equal(page.querySelector(".eo-print-pdf-stamp").textContent, "대외비");
  assert.equal(page.querySelector(".eo-print-pdf-wm").textContent, "사내한");
  assert.match(page.querySelector(".eo-print-pdf-form").textContent, /신청인:\s*이서연/);
  assert.match(page.textContent, /PDF 보고서/);
  assert.match(page.textContent, /본문 1쪽 내용/);
  assert.ok(t.pageRules().some((r) => /^@page\s*\{\s*size:\s*297mm 210mm;/.test(r)));
  t.h.sandbox.dispatchWindowEvent("afterprint");
});

test("the print menu item follows the module: on in document, sheet, slide, and PDF", async () => {
  for (const [moduleCode, grayed] of [["DOC", false], ["SHEET", false], ["SLIDE", false], ["PDF", false]]) {
    const reports = [];
    const h = await createWorkspaceHarness({ platform: "MacIntel", shellCommands: { report_menu_state: (a) => { reports.push(a); return null; } } });
    await flushAsyncTurns(10);
    switchModule(h.document, moduleCode);
    await flushAsyncTurns(10);
    assert.equal(reports.at(-1)?.disabled.includes("file:print"), grayed, moduleCode);
  }
});
