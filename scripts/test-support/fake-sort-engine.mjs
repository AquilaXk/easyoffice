// The page tests' stand-in for the engine's `sort` session action (see offline-sort.js).
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./load-ui-module.mjs";

export const fakeSortEngine = loadUiModule(fileURLToPath(new URL("./offline-sort.js", import.meta.url)), {}).__EASYOFFICE_OFFLINE_SORT__;
