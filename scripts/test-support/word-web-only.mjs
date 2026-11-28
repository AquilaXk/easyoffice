// Commands of the word processor window's menus that the macOS native bar does not list yet (TODO.md: the native items of the
// Word-style commands). The native-menu tests still fail for a page command that is neither here nor in the native table.
export const WORD_WEB_ONLY = new Set([
  "para-dialog",
  "format-painter",
  "format-clear",
  "paste-text-only",
  "doc-properties",
  "insert-bookmark",
  "insert-xref",
  "insert-caption",
  "insert-datetime",
  "field-update",
  "section-break-nextPage",
  "section-break-continuous",
  "section-break-evenPage",
  "section-break-oddPage",
  "layout-linenumbers",
  "layout-watermark",
  "table-header-repeat",
  "table-from-text",
  "table-to-text",
]);
