import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The page's checker of sheet names (PD-209) says what the engine says (`sheet_name_problem`): both
// read the same table, and the names the page makes (a new sheet, a copy) pass the rules.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("sheet-tab-names.js"), env);
const N = env.__EASYOFFICE_SHEET_TAB_NAMES__;
const table = JSON.parse(fs.readFileSync(fileURLToPath(new URL("../fixtures/sheet-name-rules.json", import.meta.url)), "utf8"));

test("every name of the shared table gets the problem the engine gives it", () => {
  assert.ok(table.length >= 20);
  for (const { name, problem } of table) {
    assert.equal(N.problemOf(name)?.code ?? null, problem, JSON.stringify(name));
  }
});

test("the messages say what to change", () => {
  assert.equal(N.messageOf(N.problemOf("")), "시트 이름을 입력");
  assert.equal(N.messageOf(N.problemOf("a".repeat(32))), "시트 이름은 31자까지");
  assert.equal(N.messageOf(N.problemOf("a[b")), "시트 이름에 쓸 수 없는 글자: [");
  assert.equal(N.messageOf(N.problemOf("a\u0001b")), "시트 이름에 쓸 수 없는 글자: 제어 문자");
  assert.equal(N.messageOf(N.problemOf("'a")), "시트 이름은 작은따옴표로 시작하거나 끝날 수 없음");
  assert.equal(N.messageOf(N.problemOf("History")), "예약된 시트 이름");
  assert.equal(N.messageOf(null), "");
});

test("a name is taken by another sheet's key or by the label the page shows for it, whatever the case", () => {
  const sheets = ["Sheet1", "매출", "Q1"];
  assert.equal(N.isTaken("sheet1", sheets), true);
  assert.equal(N.isTaken("시트 1", sheets), true, "the label of Sheet1");
  assert.equal(N.isTaken("q1", sheets), true);
  assert.equal(N.isTaken("시트 2", sheets), false);
  assert.equal(N.isTaken("Sheet1", sheets, "Sheet1"), false, "a sheet does not conflict with itself");
  assert.equal(N.checkName("매출", sheets, "Sheet1"), "이미 있는 시트 이름");
  assert.equal(N.checkName("새 이름", sheets, "Sheet1"), "");
  assert.equal(N.checkName("a:b", sheets, "Sheet1"), "시트 이름에 쓸 수 없는 글자: :");
});

test("a new sheet takes the first free number from the sheet count up", () => {
  assert.equal(N.nextDefault(["Sheet1"]), "Sheet2");
  assert.equal(N.nextDefault(["Sheet1", "Sheet3"]), "Sheet4", "Sheet3 is in use");
  assert.equal(N.nextDefault(["Sheet1", "시트 3"]), "Sheet4", "so is the label 시트 3");
  assert.equal(N.nextDefault(["Sheet2", "Sheet3"]), "Sheet4");
  assert.equal(N.labelOf("Sheet12"), "시트 12");
  assert.equal(N.labelOf("Sheet"), "Sheet");
  assert.equal(N.labelOf("Sheet1x"), "Sheet1x");
});

test("a copy is named 'name (2)' and counts up, cut to fit and made of allowed characters", () => {
  assert.equal(N.copyName("시트 1", ["Sheet1"]), "시트 1 (2)");
  assert.equal(N.copyName("시트 1", ["Sheet1", "시트 1 (2)"]), "시트 1 (3)");
  assert.equal(N.copyName("매출", ["매출", "매출 (2)", "매출 (4)"]), "매출 (3)");
  const long = "가".repeat(31);
  assert.equal(N.copyName(long, [long]), "가".repeat(27) + " (2)");
  const emoji = "😀".repeat(15);
  const copy = N.copyName(emoji, [emoji]);
  assert.ok(copy.length <= 31 && copy.endsWith(" (2)") && !N.problemOf(copy), copy);
  // A name a file gave with a character the page refuses is made of allowed ones.
  assert.equal(N.problemOf(N.copyName("a/b", ["a/b"])), null);
  // Over the sheets a book can hold the loop still ends.
  const taken = Array.from({ length: 256 }, (_, i) => (i === 0 ? "X" : `X (${i + 1})`));
  assert.equal(N.copyName("X", taken), "X (257)");
});

test("names the page makes always pass its own rules", () => {
  let names = ["Sheet1"];
  for (let i = 0; i < 300; i += 1) {
    const name = i % 2 ? N.nextDefault(names) : N.copyName(N.labelOf(names[names.length - 1]), names);
    assert.equal(N.problemOf(name), null, name);
    assert.equal(N.isTaken(name, names), false, name);
    names = [...names, name];
  }
});
