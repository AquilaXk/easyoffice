#!/usr/bin/env node
/**
 * Parses `cargo audit --json` reports across Cargo lockfiles, enforces bounded payload parsing,
 * emits CI warning/error annotations, writes a summary table to GITHUB_STEP_SUMMARY, and
 * optionally fails closed under `--strict` when advisories are present.
 *
 * Uses Node.js built-in modules only.
 */

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { escapeWorkflowCommand } from "./workflow-command.mjs";

const MAX_REPORT_BYTES = 16 * 1024 * 1024;

function escapeMarkdownCell(value) {
  return String(value ?? "").replace(/\r?\n/g, " ").replaceAll("|", "\\|");
}

export function parseCargoAuditJson(rawText, label) {
  if (typeof rawText !== "string" || rawText.trim().length === 0) {
    throw new Error(`cargo audit report '${label}' is empty`);
  }
  if (Buffer.byteLength(rawText, "utf8") > MAX_REPORT_BYTES) {
    throw new Error(`cargo audit report '${label}' exceeds ${MAX_REPORT_BYTES} bytes`);
  }
  let doc;
  try {
    doc = JSON.parse(rawText);
  } catch (err) {
    throw new Error(`cargo audit report '${label}' is invalid JSON: ${err.message}`);
  }
  if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
    throw new Error(`cargo audit report '${label}' must be a JSON object`);
  }
  if (!doc.vulnerabilities || !Array.isArray(doc.vulnerabilities.list)) {
    throw new Error(`cargo audit report '${label}' is missing vulnerabilities.list (audit execution may have failed)`);
  }

  const rawVulns = doc.vulnerabilities.list;
  const vulnerabilities = rawVulns.map((item) => ({
    label,
    id: item?.advisory?.id ?? "(unknown)",
    pkg: item?.package?.name ?? "(unknown)",
    version: item?.package?.version ?? "?",
    title: item?.advisory?.title ?? "",
    patched: Array.isArray(item?.versions?.patched) && item.versions.patched.length > 0
      ? item.versions.patched.join(", ")
      : "none",
  }));

  const warnings = [];
  if (doc.warnings && typeof doc.warnings === "object") {
    for (const [kind, entries] of Object.entries(doc.warnings)) {
      if (!Array.isArray(entries)) continue;
      for (const entry of entries) {
        warnings.push({
          label,
          kind,
          id: entry?.advisory?.id ?? "-",
          pkg: entry?.package?.name ?? "(unknown)",
          version: entry?.package?.version ?? "?",
          title: entry?.advisory?.title ?? "",
        });
      }
    }
  }

  return { label, vulnerabilities, warnings };
}

export function aggregateCargoAuditReports(parsedReports) {
  const vulnerabilities = parsedReports.flatMap((r) => r.vulnerabilities);
  const warnings = parsedReports.flatMap((r) => r.warnings);
  return {
    reports: parsedReports,
    vulnerabilityCount: vulnerabilities.length,
    warningCount: warnings.length,
    vulnerabilities,
    warnings,
  };
}

export function formatCargoAuditMarkdown(summary) {
  const lines = ["## Cargo Dependency Audit", ""];
  lines.push(`- **Vulnerabilities**: ${summary.vulnerabilityCount}`);
  lines.push(`- **Informational warnings**: ${summary.warningCount}`, "");

  if (summary.vulnerabilities.length > 0) {
    lines.push("| Target | ID | Package | Version | Patched | Title |", "| --- | --- | --- | --- | --- | --- |");
    for (const v of summary.vulnerabilities) {
      lines.push(
        `| \`${escapeMarkdownCell(v.label)}\` | \`${escapeMarkdownCell(v.id)}\` | \`${escapeMarkdownCell(v.pkg)}\` | \`${escapeMarkdownCell(v.version)}\` | \`${escapeMarkdownCell(v.patched)}\` | ${escapeMarkdownCell(v.title)} |`,
      );
    }
    lines.push("");
  }

  if (summary.warnings.length > 0) {
    lines.push("| Target | Kind | Package | Version | ID |", "| --- | --- | --- | --- | --- |");
    for (const w of summary.warnings) {
      lines.push(
        `| \`${escapeMarkdownCell(w.label)}\` | ${escapeMarkdownCell(w.kind)} | \`${escapeMarkdownCell(w.pkg)}\` | \`${escapeMarkdownCell(w.version)}\` | \`${escapeMarkdownCell(w.id)}\` |`,
      );
    }
    lines.push("");
  }
  return lines.join("\n");
}

function parseArgs(argv) {
  const reports = [];
  let strict = false;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--strict") {
      strict = true;
    } else if (arg === "--report" && argv[i + 1]) {
      const spec = argv[++i];
      const eq = spec.indexOf("=");
      if (eq <= 0 || eq === spec.length - 1) {
        throw new Error(`invalid --report '${spec}' (expected <label>=<path>)`);
      }
      reports.push({ label: spec.slice(0, eq), file: spec.slice(eq + 1) });
    } else {
      throw new Error(`unknown or incomplete argument '${arg}'`);
    }
  }
  if (reports.length === 0) {
    throw new Error("usage: cargo-audit-report.mjs --report <label>=<json-path> [--strict]");
  }
  return { reports, strict };
}

function runCli(argv) {
  const { reports, strict } = parseArgs(argv);
  const parsed = reports.map(({ label, file }) => {
    const stat = fs.statSync(file);
    if (stat.size > MAX_REPORT_BYTES) {
      throw new Error(`cargo audit report '${file}' exceeds ${MAX_REPORT_BYTES} bytes`);
    }
    return parseCargoAuditJson(fs.readFileSync(file, "utf8"), label);
  });
  const summary = aggregateCargoAuditReports(parsed);

  const level = strict ? "error" : "warning";
  for (const v of summary.vulnerabilities) {
    console.warn(
      `::${level}::[${escapeWorkflowCommand(v.label)}] ${escapeWorkflowCommand(v.id)} in ${escapeWorkflowCommand(v.pkg)} ${escapeWorkflowCommand(v.version)} (patched: ${escapeWorkflowCommand(v.patched)}) — ${escapeWorkflowCommand(v.title)}`,
    );
  }
  const summaryPath = process.env.GITHUB_STEP_SUMMARY || "";
  if (summaryPath) {
    fs.appendFileSync(summaryPath, `${formatCargoAuditMarkdown(summary)}\n`);
  }
  console.log(
    `Cargo audit processed ${parsed.length} lockfile(s): ${summary.vulnerabilityCount} vulnerabilities, ${summary.warningCount} warnings.`,
  );
  if (strict && summary.vulnerabilityCount > 0) {
    process.exit(1);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    runCli(process.argv.slice(2));
  } catch (err) {
    console.error(`::error::${escapeWorkflowCommand(err?.message ?? err)}`);
    process.exit(1);
  }
}
