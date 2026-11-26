/**
 * Escapes GitHub Actions workflow command data (`::error::`, `::warning::`, `::notice::`)
 * so embedded `%`, CR, and LF characters cannot cross annotation command boundaries.
 */
export function escapeWorkflowCommand(value) {
  return String(value ?? "").replaceAll("%", "%25").replaceAll("\r", "%0D").replaceAll("\n", "%0A");
}
