(function (host) {
  "use strict";

  // Workspace session guard: local autosave/restore of the undo snapshot (SSOT shared with
  // ribbon-history.js), dirty tracking with a conditional beforeunload prompt, registry-driven
  // keyboard dispatch, per-module toolbar context, selection-aware clipboard and bootstrap
  // failure recovery.
  const AUTOSAVE_STORAGE_KEY = "easyoffice.workspace.autosave.v1";
  const AUTOSAVE_SCHEMA_VERSION = 1;
  const AUTOSAVE_DEBOUNCE_MS = 1200;
// Workbooks above this many cells are not autosaved: fingerprinting/serializing them would stall
// typing and exceed the browser storage quota anyway (the unsaved-changes guard stays active).
const AUTOSAVE_MAX_SHEET_CELLS = 20000;
  const MAX_NODE_SPEC_DEPTH = 64;

  const ALL_MODULES = ["DOC", "HWP", "SHEET", "SLIDE", "PDF", "CONV"];
  const DOC_MODULES = ["DOC", "HWP"];
  const EDIT_MODULES = ["DOC", "HWP", "SHEET", "SLIDE"];
  const DOC_SHEET_MODULES = ["DOC", "HWP", "SHEET"];
  const RIBBON_MODULES = ["DOC", "HWP", "SHEET", "SLIDE", "PDF"];
  // Menu commands that are the suffix of a ribbon command id (`<module>-<suffix>`, ribbon-catalog).
  const RIBBON_SUFFIX_COMMANDS = [
    "home-bold", "home-italic", "home-underline", "home-style",
    "insert-table", "insert-chart", "insert-shape", "insert-link",
    "layout-paper", "layout-orientation", "layout-columns", "layout-margins",
    "data-autosum", "data-fn-browser", "data-sort", "data-validation",
    "review-track", "review-comment", "review-verify", "review-stats",
    "view-split", "view-theme", "view-zoom",
  ];
  // Module-specific ribbon commands are only enabled in their own module.
  const MODULE_SPECIFIC_COMMANDS = Object.fromEntries([
    ["doc", "DOC", ["toc", "toc-update", "page-break", "footnote", "mailmerge", "tableformula"]],
    ["hwp", "HWP", ["header", "field", "border", "manuscript"]],
    ["sheet", "SHEET", ["pivot", "chart", "condfmt", "freeze"]],
    ["slide", "SLIDE", ["master", "snap", "transition", "presenter"]],
    ["pdf", "PDF", ["highlight", "rotate", "splitmerge", "tooffice"]],
  ].flatMap(([prefix, mod, names]) => names.map((name) => [`${prefix}-mod-${name}`, [mod]])));
  // Studio menu/toolbar command -> modules where it has a real effect (default: DOC/HWP only).
  const STUDIO_COMMAND_MODULES = {
    "studio-save": ALL_MODULES, "studio-save-as": ALL_MODULES, "studio-undo": ALL_MODULES, "studio-redo": ALL_MODULES,
    palette: ALL_MODULES, "view-theme": ALL_MODULES, "zoom-in": ALL_MODULES, "zoom-out": ALL_MODULES,
    "zoom-fit-page": ALL_MODULES, "zoom-fit-width": ALL_MODULES, "smart-diagnose": ALL_MODULES,
    "scrub-pii": ALL_MODULES, "security-audit": ALL_MODULES, "bench-run": ALL_MODULES, "view-outline": ALL_MODULES,
    find: ["DOC", "SHEET"], replace: ["DOC", "SHEET"], print: ["DOC", "SHEET", "SLIDE"],
    "clip-copy": EDIT_MODULES, "clip-cut": DOC_SHEET_MODULES, "clip-paste": DOC_SHEET_MODULES,
    "mod-symbol": EDIT_MODULES,
    // Every ribbon command exists for the five editing modules, so its menu item is enabled in all
    // of them (PD-098: the menu bar reaches every command the ribbon has).
    ...Object.fromEntries(RIBBON_SUFFIX_COMMANDS.map((cmd) => [cmd, RIBBON_MODULES])),
    // A PDF page has no style, table, shape or link to apply: there these four only answer with a status line.
    ...Object.fromEntries(["home-style", "insert-table", "insert-shape", "insert-link"].map((cmd) => [cmd, EDIT_MODULES])),
    ...MODULE_SPECIFIC_COMMANDS,
  };
  const TOOLBAR_ID_COMMANDS = {
    "btn-toggle-ctrl-marks": "ctrl-marks",
    "btn-toggle-para-marks": "para-marks",
    "btn-toggle-doc-grid": "doc-grid",
  };
  const BLOCKED_SPEC_TAGS = new Set([
    "script", "iframe", "frame", "frameset", "object", "embed", "link", "meta", "style", "base",
    "form", "template", "svg", "math", "portal",
  ]);
  // Parts of the window that are not the editing area: the web view's own right-click menu has
  // nothing to offer there (the editing area and text fields keep it). Reference implementation (MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md),
  // `apps/studio-host/src/core/desktop-chrome.ts`: the same chrome selector and context-menu guard.
  const NON_EDITOR_CHROME_SELECTOR = "#home-screen, #menu-bar, #icon-toolbar, #doc-style-bar, #status-bar";
  const URL_ATTRS = new Set(["href", "src", "action", "formaction", "xlink:href", "srcset", "poster"]);

  // Commands that still apply while the embedded HWP editor is the active surface.
  // The PII scrub and security audit read the page's own (hidden) fallback DOM, not the embedded document, so they stay off here.
  const HWP_EMBED_COMMANDS = new Set(["studio-save", "studio-save-as", "palette", "view-theme", "print"]);

  // The start window has no document: only the window-level commands apply.
  const START_COMMANDS = new Set(["palette", "view-theme"]);
  const START_SHORTCUT_ACTIONS = new Set([
    "file-open", "file-new", "window-close", "app-quit", "recent-dialog", "palette", "view-theme",
  ]);

  function studioCommandApplies(cmd, mod) {
    if (!cmd || cmd.startsWith("module:")) return true;
    if (mod === "START") return START_COMMANDS.has(cmd);
    if (mod === "HWP_EMBED") return HWP_EMBED_COMMANDS.has(cmd);
    return (STUDIO_COMMAND_MODULES[cmd] || DOC_MODULES).includes(mod);
  }

  // Default string order (UTF-16 code units) as an explicit comparator; keeps fingerprints stable.
  function compareCodeUnits(a, b) {
    if (a < b) return -1;
    return a > b ? 1 : 0;
  }

  function hashFingerprint(text) {
    let h = 0x811c9dc5;
    for (let i = 0; i < text.length; i += 1) {
      h ^= text.codePointAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return text.length.toString(36) + "-" + h.toString(36);
  }

  // Ignore serialization noise that merely viewing a pane produces (empty cells/styles/lists
  // materialized on first render, table summary attributes recomputed from cell text) so that
  // only real edits mark the workspace dirty.
  const DERIVED_DATA_KEYS = new Set(["docNum", "docAvg", "docCount"]);

  function canonicalizeObject(value, key) {
    const out = {};
    for (const k of Object.keys(value).sort(compareCodeUnits)) {
      if (key === "data" && DERIVED_DATA_KEYS.has(k)) continue;
      const c = canonicalizeContent(value[k], k);
      if (c !== undefined) out[k] = c;
    }
    return Object.keys(out).length > 0 ? out : undefined;
  }

  function canonicalizeContent(value, key) {
    if (Array.isArray(value)) {
      return value.length > 0 ? value.map((item) => canonicalizeContent(item, "") ?? null) : undefined;
    }
    if (value && typeof value === "object") return canonicalizeObject(value, key);
    return value === "" || value === null || value === false ? undefined : value;
  }

  // `workbookProtect`: the protection of the workbook (no part of a snapshot); a workbook without one adds nothing.
  function contentFingerprint(snap, workbookProtect) {
    const parts = [
      snap.docTabs, snap.sheets, snap.sheetRowCounts, snap.sheetColumnWidths, snap.sheetRules, snap.sheetLayout, snap.definedNames,
      snap.sheetFrozen, snap.sheetOrder, snap.slides, snap.pdfStudio, snap.pdfRotationDeg,
    ];
    if (workbookProtect) parts.push(workbookProtect);
    return hashFingerprint(JSON.stringify(canonicalizeContent(parts, "")));
  }

  // FNV-1a step over `text`, continuing from `h`.
  function fnvStep(h, text) {
    for (let i = 0; i < text.length; i += 1) {
      h ^= text.charCodeAt(i);
      h = Math.imul(h, 0x01000193);
    }
    return h;
  }

  const isBlankText = (v) => v === undefined || v === null || v === "" || v === false;

  // The hash of a style's canonical text. A frozen style object is shared by many cells and never
  // edited, so its hash is made once; the value is the same for an equal style that is not frozen.
  const NULL_STYLE_HASH = fnvStep(0x811c9dc5, "null") >>> 0;
  const frozenStyleHash = new WeakMap();
  const styleHashOf = (style) => fnvStep(0x811c9dc5, JSON.stringify(canonicalizeContent(style, "style") ?? null)) >>> 0;
  function styleDigest(style) {
    if (!style) return NULL_STYLE_HASH;
    if (!Object.isFrozen(style)) return styleHashOf(style);
    let hash = frozenStyleHash.get(style);
    if (hash === undefined) {
      hash = styleHashOf(style);
      frozenStyleHash.set(style, hash);
    }
    return hash;
  }

  // The digest part of one cell (0 for a cell that canonicalizeContent drops as empty). The plain
  // shape `{ value, formula, style? }` is hashed in place; anything else goes through the canonical form.
  function cellDigest(ref, entry, seed) {
    if (!entry || typeof entry !== "object") return 0;
    const { value, formula, style } = entry;
    const plain = Object.keys(entry).length <= 3 && (value === undefined || typeof value === "string") &&
      (formula === undefined || typeof formula === "string") && (style === undefined || (style && typeof style === "object"));
    let h = fnvStep(seed, ref);
    if (plain) {
      const styleHash = styleDigest(style);
      if (isBlankText(value) && isBlankText(formula) && styleHash === NULL_STYLE_HASH) return 0;
      h = fnvStep(fnvStep(h, "\u0000" + (value ?? "")), "\u0001" + (formula ?? ""));
      h = Math.imul(h ^ styleHash, 0x01000193);
    } else {
      const canonical = canonicalizeContent(entry, ref);
      if (canonical === undefined) return 0;
      h = fnvStep(h, "\u0003" + JSON.stringify(canonical));
    }
    return h >>> 0;
  }

  // Order-independent digest of a sheet store: sums of one hash per non-empty cell (a plain sum and a
  // sum of a scrambled copy), so a 100,000-row workbook needs neither a sort of its keys nor a canonical copy of itself.
  function sheetStoreDigest(sheets) {
    let lo = 0;
    let hi = 0;
    for (const name of Object.keys(sheets || {})) {
      const map = sheets[name];
      if (!map || typeof map !== "object") continue;
      const nameHash = fnvStep(0x811c9dc5, name + "\u0004");
      for (const ref in map) {
        const h = cellDigest(ref, map[ref], nameHash);
        lo = (lo + h) >>> 0;
        hi = (hi + (Math.imul(h ^ (h >>> 16), 0x85ebca6b) ^ (h >>> 13))) >>> 0;
      }
    }
    return lo.toString(36) + "." + hi.toString(36);
  }

  // Fingerprint of one module's document only (file open asks before replacing edited content).
  function moduleFingerprint(snap, mod) {
    if (mod === "SHEET") {
      const rest = [snap.sheetRowCounts, snap.sheetColumnWidths, snap.sheetRules, snap.sheetLayout, snap.definedNames, snap.sheetFrozen, snap.sheetOrder];
      return hashFingerprint(JSON.stringify(canonicalizeContent(rest, ""))) + "-" + sheetStoreDigest(snap.sheets);
    }
    const parts = {
      DOC: [snap.docTabs?.DOC],
      HWP: [snap.docTabs?.HWP],
      SLIDE: [snap.slides],
      PDF: [snap.pdfStudio, snap.pdfRotationDeg],
    }[mod] || [];
    return hashFingerprint(JSON.stringify(canonicalizeContent(parts, "")));
  }

  function isSafeNodeSpecList(specs, depth) {
    if (specs === undefined || specs === null) return true;
    if (!Array.isArray(specs) || depth > MAX_NODE_SPEC_DEPTH) return false;
    return specs.every((spec) => {
      if (!spec || typeof spec !== "object") return false;
      if (spec.t === "text") return true;
      if (spec.t !== "el" || !/^[a-z][a-z0-9-]*$/i.test(String(spec.tag || ""))) return false;
      if (BLOCKED_SPEC_TAGS.has(String(spec.tag).toLowerCase())) return false;
      const attrs = spec.attrs && typeof spec.attrs === "object" ? spec.attrs : {};
      for (const [name, val] of Object.entries(attrs)) {
        const lower = name.toLowerCase();
        if (lower.startsWith("on")) return false;
        if (URL_ATTRS.has(lower) && /^\s*(javascript|vbscript|data):/i.test(String(val))) return false;
      }
      return isSafeNodeSpecList(spec.children, depth + 1);
    });
  }

  function isRestorableSnapshot(snap) {
    if (!snap || typeof snap !== "object") return false;
    if (!snap.docTabs || typeof snap.docTabs !== "object") return false;
    if (!snap.sheets || typeof snap.sheets !== "object" || !Array.isArray(snap.slides)) return false;
    if (snap.activeModule && !ALL_MODULES.includes(snap.activeModule)) return false;
    for (const key of DOC_MODULES) {
      const tab = snap.docTabs[key];
      if (tab && !isSafeNodeSpecList(tab.editorNodes, 0)) return false;
    }
    return snap.slides.every((s) => s && typeof s === "object" && isSafeNodeSpecList(s.insertedNodes, 0));
  }

  function shortcutComboFromEvent(e) {
    let key = String(e.key || "");
    if (e.altKey && /^Key[A-Z]$/.test(String(e.code || ""))) key = e.code.slice(3);
    if (key.length === 1) key = key.toUpperCase();
    const parts = [];
    if (e.metaKey || e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    parts.push(key);
    return parts.join("+");
  }

  function guessClientPlatform() {
    const nav = host.navigator || {};
    const raw = String(nav.userAgentData?.platform || nav.platform || "");
    if (/mac|iphone|ipad/i.test(raw)) return { platform: "macOS", shortcutModifier: "⌘" };
    if (/win/i.test(raw)) return { platform: "Windows", shortcutModifier: "Ctrl" };
    if (/linux|x11|cros/i.test(raw)) return { platform: "Linux", shortcutModifier: "Ctrl" };
    return { platform: "Desktop", shortcutModifier: "Ctrl" };
  }

  function isMacClient() {
    return guessClientPlatform().shortcutModifier === "⌘";
  }

  function resolveStorage() {
    try {
      const storage = host.localStorage;
      return storage && typeof storage.getItem === "function" ? storage : null;
    } catch {
      return null;
    }
  }

  // True when the workbook holds more than `limit` cells; stops counting at the first one past it.
  function exceedsSheetCells(sheets, limit) {
    let n = 0;
    for (const name of Object.keys(sheets || {})) {
      for (const _ref in sheets[name]) {
        n += 1;
        if (n > limit) return true;
      }
    }
    return false;
  }

  function editableRootFor(node) {
    const el = node?.nodeType === 1 ? node : node?.parentElement;
    return el?.closest?.('[contenteditable="true"]') || null;
  }

  function captureSelectionRange() {
    const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
    if (!sel?.rangeCount) return null;
    const r = sel.getRangeAt(0);
    if (!r) return null;
    return typeof r.cloneRange === "function" ? r.cloneRange() : r;
  }

  function createWorkspaceSession(deps) {
    const {
      document,
      shortcutBindings,
      getActiveModule,
      getHwpHost,
      captureWorkspaceSnapshot,
      restoreWorkspaceSnapshot,
      runExclusive,
      getWorkspacePanels,
      getSheetController,
    } = deps;
    const isDesktopShell = typeof deps.isDesktopShell === "function" ? deps.isDesktopShell : () => false;

    // Each window keeps its own recovery record (keyed by the window id); a host without window
    // ids (single-page tests) keeps the shared key.
    const storageKey = () => {
      const id = typeof deps.getDocumentId === "function" ? deps.getDocumentId() : "";
      return id ? AUTOSAVE_STORAGE_KEY + "." + id : AUTOSAVE_STORAGE_KEY;
    };
    let discarded = false;
    let lastReportedDirty = null;
    let baselineHash = null;
    let dirty = false;
    let externalDirty = false; // unsaved changes owned by another editor (the embedded HWP editor)
    let pendingActivity = false;
    let guardRegistered = false;
    let restoring = false;
    let autosaveTimer = null;
    let lastEditableRange = null;
    let internalClipboard = "";
    let bootstrapRetry = null;

    function setFeedback(text) {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    function handleBeforeUnload(ev) {
      flushAutosaveNow();
      if (!dirty && !externalDirty) return undefined;
      ev.preventDefault?.();
      ev.returnValue = "";
      return "";
    }

    function syncUnloadGuard() {
      const want = dirty || pendingActivity || externalDirty;
      const shell = document.getElementById("app");
      const unsaved = dirty || externalDirty;
      if (shell) shell.dataset.dirty = unsaved ? "true" : "false";
      if (lastReportedDirty !== unsaved) {
        lastReportedDirty = unsaved;
        if (typeof deps.onDirtyChange === "function") deps.onDirtyChange(unsaved);
      }
      if (want === guardRegistered) return;
      guardRegistered = want;
      if (want) host.addEventListener?.("beforeunload", handleBeforeUnload);
      else host.removeEventListener?.("beforeunload", handleBeforeUnload);
    }

    const workbookProtectOf = () => host.__EASYOFFICE_SHEET_PROTECT__?.getWorkbook() ?? null;

    function persistRecord(snap) {
      const storage = resolveStorage();
      if (!storage || discarded) return;
      try {
        if (!dirty && storage.getItem(storageKey()) === null) return;
        storage.setItem(
          storageKey(),
          JSON.stringify({
            version: AUTOSAVE_SCHEMA_VERSION,
            savedAt: Date.now(),
            dirty,
            baseline: baselineHash,
            snapshot: snap,
            // The workbook's protection is no part of a snapshot (an undo cannot lift it): the record keeps it beside.
            workbookProtect: workbookProtectOf(),
          })
        );
      } catch {
        setFeedback("자동 저장 실패: 브라우저 저장 공간이 부족하거나 차단되었습니다");
      }
    }

    function isSheetEditPending() {
      const ae = document.activeElement;
      if (!ae || typeof ae.closest !== "function" || !ae.closest("#pane-sheet")) return false;
      return ae.id === "formula-input" || Boolean(ae.dataset?.editMode);
    }

    // A workbook past the limit is not captured at all: copying it only to count it stalls every autosave tick.
    function skipAutosaveForLargeWorkbook(sheets) {
      if (!exceedsSheetCells(sheets, AUTOSAVE_MAX_SHEET_CELLS)) return false;
      pendingActivity = false;
      dirty = true;
      try {
        resolveStorage()?.removeItem(storageKey());
      } catch {
        // storage may be blocked; the unsaved-changes guard still protects the session
      }
      setFeedback("큰 시트(셀 2만 개 초과)는 자동 저장하지 않습니다. 저장(Ctrl+S)으로 보관하세요");
      syncUnloadGuard();
      return true;
    }

    function captureAndPersist() {
      if (deps.getSheetStore && skipAutosaveForLargeWorkbook(deps.getSheetStore())) return;
      const snap = captureWorkspaceSnapshot(false);
      if (skipAutosaveForLargeWorkbook(snap.sheets)) return;
      const hash = contentFingerprint(snap, workbookProtectOf());
      if (baselineHash === null) baselineHash = hash;
      pendingActivity = false;
      dirty = hash !== baselineHash;
      persistRecord(snap);
      syncUnloadGuard();
    }

    function flushAutosaveNow() {
      if (autosaveTimer !== null) host.clearTimeout?.(autosaveTimer);
      autosaveTimer = null;
      if (!pendingActivity && !dirty) return;
      try {
        captureAndPersist();
      } catch {
        dirty = true;
      }
    }

    function scheduleAutosave() {
      if (autosaveTimer !== null) host.clearTimeout?.(autosaveTimer);
      autosaveTimer = host.setTimeout?.(() => {
        autosaveTimer = null;
        // A focused sheet cell may hold an uncommitted edit; capture it on the next activity
        // (or the unload flush) instead of racing the cell commit state machine.
        if (isSheetEditPending()) return;
        void runExclusive(captureAndPersist).catch(() => {});
      }, AUTOSAVE_DEBOUNCE_MS) ?? null;
    }

    function noteActivity() {
      if (restoring) return;
      pendingActivity = true;
      syncUnloadGuard();
      scheduleAutosave();
    }

    function establishBaseline(restoredRecord) {
      const snap = captureWorkspaceSnapshot(false);
      const hash = contentFingerprint(snap, workbookProtectOf());
      baselineHash = typeof restoredRecord?.baseline === "string" ? restoredRecord.baseline : hash;
      dirty = hash !== baselineHash;
      syncUnloadGuard();
    }

    function setExternalDirty(next) {
      externalDirty = Boolean(next);
      syncUnloadGuard();
    }

    function markSaved() {
      return runExclusive(() => {
        const snap = captureWorkspaceSnapshot(false);
        baselineHash = contentFingerprint(snap, workbookProtectOf());
        dirty = false;
        pendingActivity = false;
        persistRecord(snap);
        syncUnloadGuard();
      });
    }

    async function restoreFromStorage() {
      const storage = resolveStorage();
      if (!storage) return null;
      let record = null;
      try {
        const raw = storage.getItem(storageKey());
        if (!raw) return null;
        record = JSON.parse(raw);
      } catch {
        setFeedback("자동 저장 데이터를 읽을 수 없어 기본 문서로 시작합니다");
        return null;
      }
      if (record?.version !== AUTOSAVE_SCHEMA_VERSION || !isRestorableSnapshot(record.snapshot)) {
        setFeedback("자동 저장 데이터 형식이 올바르지 않아 복원하지 않았습니다");
        return null;
      }
      restoring = true;
      try {
        await runExclusive(() => restoreWorkspaceSnapshot(record.snapshot, null));
        host.__EASYOFFICE_SHEET_PROTECT__.setWorkbook(record.workbookProtect ?? null);
      } finally {
        restoring = false;
      }
      setFeedback("이전 세션의 편집 내용을 자동 저장본에서 복원했습니다");
      return record;
    }

    // Closing a window drops its recovery record and stops writing a new one.
    function discardStoredState() {
      discarded = true;
      if (autosaveTimer !== null) host.clearTimeout?.(autosaveTimer);
      autosaveTimer = null;
      try {
        resolveStorage()?.removeItem(storageKey());
      } catch {
        // storage may be blocked; nothing is left to clean up
      }
    }

    function installActivityTracking() {
      const root = document.documentElement;
      for (const type of ["input", "change", "click", "eo-doc-mutated"]) {
        root?.addEventListener?.(type, () => noteActivity());
      }
      host.addEventListener?.("pagehide", () => flushAutosaveNow());
      document.addEventListener?.("selectionchange", () => {
        const r = captureSelectionRange();
        if (r && editableRootFor(r.commonAncestorContainer)) lastEditableRange = r;
      });
    }

    // Hides toolbar groups whose buttons are all hidden, and separators that would dangle.
    function syncToolbarGroupVisibility() {
      const bar = document.getElementById("icon-toolbar");
      if (!bar) return;
      // The header/footer edit group shows by the edit state alone (`doc-page-frame.js`).
      const kids = Array.from(bar.children).filter((el) => el.dataset?.hfGroup === undefined);
      const isGroup = (el) => String(el.className).split(/\s+/).includes("tb-group");
      kids.forEach((el) => {
        if (!isGroup(el)) return;
        const btns = Array.from(el.querySelectorAll("button"));
        el.hidden = btns.length > 0 && btns.every((b) => b.hidden);
      });
      let seenVisible = false;
      kids.forEach((el, i) => {
        if (isGroup(el)) {
          seenVisible = seenVisible || !el.hidden;
          return;
        }
        const next = kids[i + 1];
        el.hidden = !seenVisible || !next || next.hidden;
      });
    }

    // A window whose toolbar has no button but the command search (the PDF window) shows no toolbar row: the search is in the menu bar.
    function syncToolbarRowVisibility() {
      const bar = document.getElementById("icon-toolbar");
      if (!bar) return;
      bar.hidden = false;
      const shown = Array.from(bar.querySelectorAll(".tb-btn")).filter((btn) => !btn.hidden && !btn.closest("[hidden]"));
      bar.hidden = shown.every((btn) => btn.dataset.studioCmd === "palette");
    }

    function syncToolbarButtons(mod, commandModule) {
      // A window's toolbar lists the commands that work in it: the others are hidden.
      for (const btn of document.querySelectorAll("#icon-toolbar button")) {
        const cmd = btn.dataset.studioCmd || TOOLBAR_ID_COMMANDS[btn.id];
        if (!cmd) continue;
        const on = studioCommandApplies(cmd, commandModule);
        btn.disabled = !on;
        btn.hidden = !on;
        btn.setAttribute("aria-disabled", on ? "false" : "true");
      }
    }

    function applyModuleContext(mod) {
      const embed = mod === "HWP" && Boolean(getHwpHost?.()?.isEmbedActive());
      // Only the editor's own toolbar is shown while it owns the HWP module (no duplicate chrome).
      const appShell = document.getElementById("app");
      const start = appShell?.dataset.windowKind === "start";
      const isDoc = DOC_MODULES.includes(mod) && !embed && !start;
      if (appShell) appShell.dataset.hwpEmbed = embed ? "true" : "false";
      for (const item of document.querySelectorAll("[data-hwp-save-as]")) item.hidden = !embed;
      for (const item of document.querySelectorAll("[data-save-as-generic]")) item.hidden = embed;
      const styleBar = document.getElementById("doc-style-bar");
      if (styleBar) styleBar.hidden = !isDoc;
      const commandModule = start ? "START" : embed ? "HWP_EMBED" : mod;
      syncToolbarButtons(mod, commandModule);
      syncToolbarGroupVisibility();
      host.__EASYOFFICE_MENU_KINDS__.applyMenuKind(document, commandModule);
      syncToolbarRowVisibility();
      for (const item of document.querySelectorAll(".md-item[data-studio-menu-cmd]")) {
        const on = studioCommandApplies(item.dataset.studioMenuCmd, commandModule);
        item.setAttribute("aria-disabled", on ? "false" : "true");
      }
      deps.onMenuItemsApplied?.();
      if (typeof deps.onMenuContext === "function") deps.onMenuContext(commandModule);
      const agg = document.getElementById("status-sheet-agg");
      if (agg) agg.hidden = mod !== "SHEET";
      const docStatus = document.querySelector(".statusbar-left-group");
      if (docStatus) docStatus.hidden = !isDoc;
    }

    function rangeBelongsToModule(range, mod) {
      const root = editableRootFor(range?.commonAncestorContainer);
      if (!root) return false;
      const scopeId = DOC_MODULES.includes(mod) ? "doc-editor" : { SHEET: "pane-sheet", SLIDE: "pane-slide" }[mod];
      const scope = scopeId ? document.getElementById(scopeId) : null;
      return Boolean(scope && (scope === root || scope.contains(root)));
    }

    function resolveActiveRange(mod) {
      const live = captureSelectionRange();
      if (live && rangeBelongsToModule(live, mod)) return live;
      return lastEditableRange && rangeBelongsToModule(lastEditableRange, mod) ? lastEditableRange : null;
    }

    async function writeSystemClipboard(text) {
      internalClipboard = text;
      const clip = host.navigator?.clipboard;
      if (clip && typeof clip.writeText === "function") {
        try {
          await clip.writeText(text);
          return true;
        } catch {
          // Permission denied: fall through to the legacy copy command.
        }
      }
      try {
        return Boolean(document.execCommand?.("copy"));
      } catch {
        return false;
      }
    }

    async function readSystemClipboard() {
      const clip = host.navigator?.clipboard;
      if (clip && typeof clip.readText === "function") {
        try {
          return String(await clip.readText());
        } catch {
          // Permission denied: use the in-app clipboard buffer.
        }
      }
      return internalClipboard;
    }

    function insertPlainText(range, text) {
      if (range) {
        range.deleteContents();
        range.insertNode(document.createTextNode(text));
        return;
      }
      const editor = document.getElementById("doc-editor");
      if (!editor) return;
      let blk = getWorkspacePanels().getFocusedDocBlock(editor);
      while (blk?.parentElement && blk.parentElement !== editor) blk = blk.parentElement;
      const p = document.createElement("p");
      p.textContent = text;
      if (blk && blk !== editor && typeof blk.after === "function") blk.after(p);
      else editor.appendChild(p);
    }

    // Replaces the selection (or inserts at the caret) with a text node and puts the caret after it, so
    // inline runs around it keep their markup. False when the document has no selection to use.
    function insertTextAtSelection(text) {
      const range = resolveActiveRange(getActiveModule());
      if (!range) return false;
      const node = document.createTextNode(text);
      range.deleteContents();
      range.insertNode(node);
      range.setStartAfter(node);
      range.collapse(true);
      const sel = typeof host.getSelection === "function" ? host.getSelection() : null;
      sel?.removeAllRanges();
      sel?.addRange(range);
      return true;
    }

    async function runSheetClipboard(cmd) {
      const where = await getSheetController().runSheetClipboardCommand(cmd);
      const verb = { "clip-cut": "오려 두기", "clip-copy": "복사", "clip-paste": "붙여넣기" }[cmd];
      if (where) setFeedback("선택 범위 " + verb + " 완료" + (where === "internal" ? " (앱 내부 클립보드)" : ""));
      else if (cmd === "clip-paste" && where === false) setFeedback("붙일 클립보드 내용이 없습니다");
    }

    async function runDocClipboardPaste(range, withDocStyleUndo) {
      const text = await readSystemClipboard();
      if (!text) {
        setFeedback("붙일 클립보드 내용이 없습니다");
        return;
      }
      withDocStyleUndo(() => insertPlainText(range, text));
      getWorkspacePanels().computeSmartStats();
      noteActivity();
      setFeedback("클립보드 붙이기 완료 (" + Array.from(text).length + "자)");
    }

    async function runClipboardCommand(cmd, withDocStyleUndo) {
      const mod = getActiveModule();
      if (!studioCommandApplies(cmd, mod)) {
        setFeedback("지원하지 않는 명령");
        return;
      }
      if (mod === "SHEET" && typeof getSheetController === "function") return runSheetClipboard(cmd);
      const range = resolveActiveRange(mod);
      const panels = getWorkspacePanels();
      if (cmd === "clip-paste") return runDocClipboardPaste(range, withDocStyleUndo);
      const text = range && !range.collapsed ? range.toString() : "";
      if (!text) {
        setFeedback((cmd === "clip-cut" ? "오려 둘" : "복사할") + " 텍스트를 먼저 선택하세요");
        return;
      }
      const toSystem = await writeSystemClipboard(text);
      if (cmd === "clip-cut") {
        withDocStyleUndo(() => range.deleteContents());
        panels.computeSmartStats();
        noteActivity();
      }
      const verb = cmd === "clip-cut" ? "오려 두기" : "복사";
      setFeedback("선택 영역 " + verb + " 완료 (" + Array.from(text).length + "자" + (toSystem ? ")" : " · 앱 내부 클립보드)"));
    }

    function isTextEntryTarget(el) {
      if (!el || el === document.body || el === document.documentElement) return false;
      const tag = String(el.tagName || "").toUpperCase();
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
      if (el.isContentEditable === true) return true;
      return Boolean(el.closest?.('[contenteditable="true"]'));
    }

    const bindingByKeys = new Map((shortcutBindings || []).map((b) => [b.keys, b]));

    // Browser-reserved combos (new/close window) are only bound inside the desktop shell; `macOnly`
    // combos only on macOS (the other systems keep the key for the editor).
    function bindingFor(combo) {
      const binding = bindingByKeys.get(combo);
      if (!binding || binding.native || binding.external) return null;
      if (binding.macOnly && !isMacClient()) return null;
      return binding.desktopOnly && !isDesktopShell() ? null : binding;
    }

    // The combos the embedded editor is asked to hand over (the frame URL's key list): the table's
    // `forwardFromEditor` entries that are bound in this window.
    function forwardedKeys() {
      return shortcutBindings.filter((b) => b.forwardFromEditor && bindingFor(b.keys) === b).map((b) => b.keys);
    }

    // The embedded HWP editor forwards a combo string, never key events or typed text; only
    // combinations the shared table marks `forwardFromEditor` are accepted.
    function runForwardedShortcut(combo, actions) {
      if (typeof combo !== "string") return false;
      const binding = bindingFor(combo);
      if (!binding?.forwardFromEditor) return false;
      const run = actions[binding.action];
      if (typeof run !== "function") return false;
      run();
      return true;
    }

    function isStartWindow() {
      return getActiveModule() === "START" || document.getElementById("app")?.dataset.windowKind === "start";
    }

    function handleShortcutKeydown(e, actions) {
      const combo = shortcutComboFromEvent(e);
      const binding = bindingFor(combo);
      if (!binding) return false;
      if (isStartWindow() && !START_SHORTCUT_ACTIONS.has(binding.action)) return false;
      if (binding.module && getActiveModule() !== binding.module) return false;
      if (binding.bareKey && isTextEntryTarget(document.activeElement)) return false;
      if (binding.scope && !document.activeElement?.closest?.(binding.scope)) return false;
      const run = binding.ribbon ? () => actions.ribbon(binding.action) : actions[binding.action];
      if (typeof run !== "function") return false;
      e.preventDefault();
      run();
      return true;
    }

    // Writes the menu shortcut labels and toolbar tooltips in the platform's notation (the markup holds the "Ctrl+X" form).
    function normalizeMenuShortcuts(modifier) {
      const { formatPlatformShortcut } = host.__EASYOFFICE_RIBBON_CATALOG__;
      for (const label of document.querySelectorAll(".md-shortcut")) {
        label.textContent = formatPlatformShortcut(label.textContent.trim(), modifier);
      }
      for (const el of document.querySelectorAll("[title]")) {
        const title = el.getAttribute("title") || "";
        const next = title.replace(/(?:(?:Ctrl|Cmd|Option|Alt|Shift)\+)+[^+\s)]+/g, (m) => formatPlatformShortcut(m, modifier));
        if (next !== title) el.setAttribute("title", next);
      }
    }

    let chromeGuardsInstalled = false;

    // One-time window chrome: no web view context menu outside the editing area. The page's own menu
    // row is always drawn, beside the native bar on macOS (the reference app does the same, PD-121).
    function installChromeGuards() {
      if (chromeGuardsInstalled) return;
      chromeGuardsInstalled = true;
      // One listener on the body: the start screen is mounted after this runs.
      document.body.addEventListener("contextmenu", (ev) => {
        if (ev.target?.closest?.(NON_EDITOR_CHROME_SELECTOR) && !isTextEntryTarget(ev.target)) ev.preventDefault();
      });
    }

    // Palette hint and quick-access tooltips; returns the active modifier glyph.
    function applyPlatformChrome(info) {
      const modifier = info?.shortcutModifier || "⌘";
      const hint = document.getElementById("shortcut-hint");
      if (hint) hint.textContent = modifier === "⌘" ? "⌘K" : modifier + "+K";
      normalizeMenuShortcuts(modifier);
      installChromeGuards();
      return modifier;
    }

    function showBootstrapError(message, onRetry) {
      bootstrapRetry = onRetry;
      const banner = document.getElementById("bootstrap-error");
      const text = document.getElementById("bootstrap-error-text");
      if (text) text.textContent = "작업 공간을 불러오지 못했습니다: " + message + " · 앱을 다시 시작한 뒤 다시 연결해 주세요.";
      if (banner) banner.hidden = false;
    }

    function hideBootstrapError() {
      bootstrapRetry = null;
      const banner = document.getElementById("bootstrap-error");
      if (banner) banner.hidden = true;
    }

    document.getElementById("btn-bootstrap-retry")?.addEventListener("click", () => {
      if (typeof bootstrapRetry === "function") bootstrapRetry();
    });

    return {
      noteActivity,
      flushAutosaveNow,
      establishBaseline,
      markSaved,
      setExternalDirty,
      restoreFromStorage,
      installActivityTracking,
      applyModuleContext,
      runClipboardCommand, insertTextAtSelection,
      handleShortcutKeydown,
      runForwardedShortcut,
      isMacClient,
      forwardedKeys,
      discardStoredState,
      applyPlatformChrome,
      showBootstrapError,
      hideBootstrapError,
      isDirty: () => dirty || externalDirty,
    };
  }

  host.__EASYOFFICE_CREATE_WORKSPACE_SESSION__ = createWorkspaceSession;
  host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__ = {
    AUTOSAVE_STORAGE_KEY,
    studioCommandApplies,
    shortcutComboFromEvent,
    guessClientPlatform,
    isRestorableSnapshot,
    moduleFingerprint,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
