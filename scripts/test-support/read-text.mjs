import fs from "node:fs";

/**
 * Reads a repo text file with LF line endings. A Windows checkout may carry CRLF, and the
 * source checks in the tests split and match on "\n".
 */
export function readText(location) {
  return fs.readFileSync(location, "utf8").replaceAll("\r\n", "\n");
}
