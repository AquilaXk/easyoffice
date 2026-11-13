// The item table of crates/easyoffice-desktop/src/native_menu.rs as source text, with the spreadsheet
// macro written out: `sa!("id", "label", "act")` is `item("id", "label", None, "sheet-act:act")`.
export function itemTable(source) {
  const start = source.indexOf("pub const ITEMS");
  const table = source.slice(start, source.indexOf("];", start));
  return table.replace(/sa!\(\s*("[^"]+"),\s*("[^"]+"),\s*"([^"]+)",?\s*\)/g, 'item($1, $2, None, "sheet-act:$3")');
}
