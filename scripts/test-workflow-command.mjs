import assert from "node:assert/strict";
import test from "node:test";
import { escapeWorkflowCommand } from "./workflow-command.mjs";

test("escapeWorkflowCommand escapes percent before CR and LF so forged commands stay single-line", () => {
  assert.equal(
    escapeWorkflowCommand("100% failed\r\n::warning::forged\n::error::second"),
    "100%25 failed%0D%0A::warning::forged%0A::error::second",
  );
  assert.equal(escapeWorkflowCommand("%0A"), "%250A");
  assert.equal(escapeWorkflowCommand(null), "");
  assert.equal(escapeWorkflowCommand(undefined), "");
  assert.equal(escapeWorkflowCommand("plain message"), "plain message");
});
