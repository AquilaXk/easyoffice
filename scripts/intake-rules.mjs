// External intake rules (PD-274): only bug reports come in from outside. The tracked tree may hold the
// bug report form, its config and CONTRIBUTING.md; pull request templates, other issue forms and label
// configuration stay forbidden.

const ALLOWED_ISSUE_TEMPLATE_FILES = new Set(["bug_report.yml", "config.yml"]);
const ISSUE_TEMPLATE_DIR = ".github/ISSUE_TEMPLATE/";
const FORBIDDEN_NAME = /(^|\/)(pull_request_template(\.[^/]*)?|labeler\.ya?ml|labels\.ya?ml|issue_template\.[^/]*)$/i;
const FORBIDDEN_DIR = /(^|\/)pull_request_template\//i;

/** Problems among tracked paths, one string per path that breaks the intake rules. */
export function findIntakeViolations(paths) {
  const problems = [];
  for (const rel of paths) {
    if (rel.startsWith(ISSUE_TEMPLATE_DIR)) {
      if (!ALLOWED_ISSUE_TEMPLATE_FILES.has(rel.slice(ISSUE_TEMPLATE_DIR.length))) {
        problems.push(`${rel}: only bug_report.yml and config.yml may live in ${ISSUE_TEMPLATE_DIR}`);
      }
    } else if (FORBIDDEN_NAME.test(rel) || FORBIDDEN_DIR.test(rel)) {
      problems.push(`${rel}: pull request templates and label configuration are not allowed`);
    }
  }
  return problems;
}
