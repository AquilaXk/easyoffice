// What the dialog layout steps (dialogs.mjs) find in the dialogs today, per step name: `dialog rule element` keys
// (dialog-layout.mjs). The lists only shrink. A fixed line must leave the list (the step fails on a line that no longer
// applies, except `clip` lines, which depend on the browser's fonts); a new line is not a fix.
export const KNOWN_DIALOG_VIOLATIONS = {
  // The sort dialog's key rows put a 26px dialog button beside the 22px combo box.
  "정렬": ["sheet-sort-dialog same-height button.dialog-btn"],
  // The preview button of the address row is a 26px dialog button beside the 22px address field.
  "하이퍼링크": ["sheet-link-dialog same-height button#btn-sheet-link-preview.dialog-btn.dialog-hyperlink-preview"],
};
