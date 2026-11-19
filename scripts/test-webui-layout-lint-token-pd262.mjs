import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-262: the page's template-token check walks the text once, like the engine's
// `contains_template_token`, so a text of many unclosed "{{" openers does not cost O(n^2).

test("containsTemplateToken keeps its meaning and scans unclosed openers in linear time", async () => {
  const { host } = await createWorkspaceHarness({ search: "?module=DOC&win=w-pd262-token" });
  const has = host.__EASYOFFICE_LAYOUT_LINT__.containsTemplateToken;
  assert.equal(typeof has, "function");

  assert.equal(has("앞 {{날짜}} 뒤"), true);
  assert.equal(has("{{{{x}}"), true);
  assert.equal(has("{{ }} {{ a b }}"), true);
  assert.equal(has("{{a\nb}}"), false);
  assert.equal(has("{{" + "a".repeat(65) + "}}"), false);
  assert.equal(has("{{" + "a".repeat(64) + "}}"), true);
  assert.equal(has("{{ 열림만"), false);
  assert.equal(has(null), false);

  const hostile = "{{".repeat(800_000) + "}}";
  const start = process.hrtime.bigint();
  assert.equal(has(hostile), false);
  const ms = Number(process.hrtime.bigint() - start) / 1e6;
  assert.ok(ms < 2000, `1.6M characters of openers took ${ms} ms`);
});
