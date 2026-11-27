// What the dialog layout steps (dialogs.mjs) find in the dialogs today, per step name: `dialog rule element` keys
// (dialog-layout.mjs). The lists only shrink. A fixed line must leave the list (the step fails on a line that no longer
// applies, except `clip` lines, which depend on the browser's fonts); a new line is not a fix.
// Empty since the Spectrum S size table (PD-378): every control is 24px high with 12px text, so the sort dialog's key rows and the
// hyperlink dialog's address row line up.
export const KNOWN_DIALOG_VIOLATIONS = {};
