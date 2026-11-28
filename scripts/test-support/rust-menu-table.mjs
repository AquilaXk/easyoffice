import fs from "node:fs";
import { fileURLToPath } from "node:url";

const HWP_ITEMS = fileURLToPath(new URL("../../crates/easyoffice-desktop/src/native_menu/items_hwp.rs", import.meta.url));

const PAGES_ITEMS = fileURLToPath(new URL("../../crates/easyoffice-desktop/src/native_menu/items_pages.rs", import.meta.url));

// The item table of crates/easyoffice-desktop/src/native_menu/items.rs (`BASE_ITEMS`) as source text, with the spreadsheet
// macros written out: `sa!("id", "label", "act")` (or with braces) is `item("id", "label", None, "sheet-act:act")`, and
// `cf!("label", "action")` is `item("format:cf-action", "label", None, "sheet-cf:action")`. The Hangul editor's items
// (`items_hwp.rs`, `HWP_ITEMS`) follow, with `editor("id", "label")` written out as `item("id", "label", None, "hwp-editor")`.
// The Pages window's items (`items_pages.rs`, `PAGES_ITEMS`, written with `sc!`) come last. `source` is the text of items.rs.
export function itemTable(source) {
  const start = source.indexOf("pub const BASE_ITEMS");
  const table = source.slice(start, source.indexOf("];", start));
  const hwp = fs.readFileSync(HWP_ITEMS, "utf8");
  const hwpStart = hwp.indexOf("pub const HWP_ITEMS");
  const hwpTable = hwp.slice(hwpStart, hwp.indexOf("];", hwpStart));
  const pages = fs.readFileSync(PAGES_ITEMS, "utf8");
  const pagesStart = pages.indexOf("pub const PAGES_ITEMS");
  const pagesTable = pages.slice(pagesStart, pages.indexOf("];", pagesStart));
  return (table + "\n" + hwpTable + "\n" + pagesTable)
    .replace(/sa!\s*[({]\s*("[^"]+"),\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item($1, $2, None, "sheet-act:$3")')
    .replace(/sl!\s*[({]\s*("[^"]+"),\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item($1, $2, None, "slide-act:$3")')
    .replace(/sc!\s*[({]\s*("[^"]+"),\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item($1, $2, None, "studio-cmd:$3")')
    .replace(/cf!\s*[({]\s*("[^"]+"),\s*"([^"]+)",?\s*[)}]/g, 'item("format:cf-$2", $1, None, "sheet-cf:$2")')
    .replace(/\beditor\(\s*("[^"]+"),\s*("[^"]+"),?\s*\)/g, 'item($1, $2, None, "hwp-editor")');
}
