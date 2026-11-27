// The item table of crates/easyoffice-desktop/src/native_menu/items.rs as source text, with the spreadsheet
// macros written out: `sa!("id", "label", "act")` (or with braces) is `item("id", "label", None, "sheet-act:act")`, and
// `cf!("label", "action")` is `item("format:cf-action", "label", None, "sheet-cf:action")`.
export function itemTable(source) {
  const start = source.indexOf("pub const ITEMS");
  const table = source.slice(start, source.indexOf("];", start));
  return table
    .replace(/sa!\s*[({]\s*("[^"]+"),\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item($1, $2, None, "sheet-act:$3")')
    .replace(/sl!\s*[({]\s*("[^"]+"),\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item($1, $2, None, "slide-act:$3")')
    .replace(/sc!\s*[({]\s*("[^"]+"),\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item($1, $2, None, "studio-cmd:$3")')
    .replace(/cf!\s*[({]\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item("format:cf-$2", $1, None, "sheet-cf:$2")');
}
