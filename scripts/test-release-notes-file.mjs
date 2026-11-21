import assert from "node:assert/strict";
import test from "node:test";
import { readText } from "./test-support/read-text.mjs";

const workflow = readText(new URL("../.github/workflows/release.yml", import.meta.url));

test("the release workflow prefers docs/releases/v<version>.md over generated notes", () => {
  assert.match(workflow, /NOTES_FILE="docs\/releases\/v\$\{VERSION\}\.md"/);
  assert.match(workflow, /--notes-file "\$NOTES_FILE"/);
  assert.match(workflow, /FLAGS\+=\(--generate-notes\)/);
  assert.doesNotMatch(workflow, /--title "EasyOffice v\$\{VERSION\}"[^\n]*--generate-notes/);
});

test("a rerun on a draft re-applies the notes file and a published release keeps its body", () => {
  const draftEdit = workflow.match(/EDIT_FLAGS=\([^\n]*\n[\s\S]*?gh release edit "\$TAG" "\$\{EDIT_FLAGS\[@\]\}"/);
  assert.ok(draftEdit, "draft rerun builds its edit flags");
  assert.match(draftEdit[0], /--notes-file "\$NOTES_FILE"/);
  assert.doesNotMatch(workflow, /gh release edit "\$TAG"[^\n]*--draft=false[^\n]*--notes/);
});
