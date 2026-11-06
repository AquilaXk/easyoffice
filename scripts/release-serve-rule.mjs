// Fail-closed static rule (PD-086): the release workflow must never build, package, or publish the
// loopback `--serve` CLI. `--serve` is a developer/CI verification mode only.

const FORBIDDEN = [
  { pattern: /cli-serve/i, label: "cli-serve artifact name" },
  { pattern: /serve-preview/i, label: "serve-preview artifact name" },
  { pattern: /--serve\b/, label: "--serve invocation" },
  { pattern: /easyoffice-app\.exe/i, label: "easyoffice-app CLI binary" },
];

export function stripYamlComments(text) {
  return text
    .split(/\r?\n/)
    .map((line) => (line.trimStart().startsWith("#") ? "" : line))
    .join("\n");
}

/** Returns one message per forbidden token found in non-comment lines of the workflow text. */
export function findServeReleaseViolations(workflowText) {
  const lines = stripYamlComments(workflowText).split("\n");
  const found = [];
  lines.forEach((line, index) => {
    for (const { pattern, label } of FORBIDDEN) {
      if (pattern.test(line)) {
        found.push(`line ${index + 1}: ${label}`);
      }
    }
  });
  return found;
}
