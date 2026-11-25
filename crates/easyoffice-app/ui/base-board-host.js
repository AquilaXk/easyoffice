// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` (MIT, Copyright (c) 2025-2026 Edward Kim;
// see `THIRD_PARTY_NOTICES.md`) — 셸 통합 어댑터 (PD-259 BASE · PD-260 BOARD).

(function (host) {
  "use strict";

  let baseStudio = null;
  let boardStudio = null;
  let runtime = null;

  // A table id comes from the file, so it goes into a selector escaped.
  const cssEscape = (value) => (typeof host.CSS?.escape === "function" ? host.CSS.escape(value) : String(value).replace(/["\\]/g, "\\$&"));

  function triggerBlobDownload(doc, env, base64Data, fileName) {
    try {
      const URLApi = env?.URL || host.URL;
      if (!URLApi || typeof URLApi.createObjectURL !== "function") return true;
      const bytes = host.__EASYOFFICE_SHEET_UTILS__.base64ToBytes(base64Data);
      const blob = new Blob([bytes], { type: "application/octet-stream" });
      const url = URLApi.createObjectURL(blob);
      const a = doc.createElement("a");
      a.href = url;
      a.download = fileName;
      doc.body.appendChild(a);
      a.click();
      a.remove();
      (env?.setTimeout || host.setTimeout)(() => URLApi.revokeObjectURL(url), 1500);
      return true;
    } catch {
      return false;
    }
  }

  async function deliverExport(base64Data, baseFilename, ext) {
    const fileName = baseFilename + "." + String(ext || "").toLowerCase();
    if (!base64Data) return { ok: false, cancelled: false, name: "", error: "" };
    const bridge = runtime?.desktopBridge || host.__EASYOFFICE_DESKTOP_BRIDGE__;
    if (bridge?.isAvailable?.()) {
      try {
        const res = await bridge.saveFile(base64Data, fileName, { inPlace: false, exportOnly: true });
        if (res?.status === "cancelled") return { ok: false, cancelled: true, name: "", error: "" };
        return { ok: true, cancelled: false, name: String(res?.name || fileName), error: "" };
      } catch (err) {
        return { ok: false, cancelled: false, name: "", error: err?.message || String(err) };
      }
    }
    const ok = triggerBlobDownload(runtime?.document || host.document, runtime?.env || host, base64Data, fileName);
    return { ok, cancelled: false, name: ok ? fileName : "", error: "" };
  }

  function resolveModuleFileBase(mod, fallback) {
    const raw = runtime?.getWindowBridge?.()?.getDocumentName?.(mod) || "";
    const sanitize = host.__EASYOFFICE_SAVE_PAYLOAD__?.sanitizeFileBaseName;
    return sanitize ? sanitize(raw, fallback) : raw || fallback;
  }

  function init(deps) {
    runtime = deps;
    const { document, env, invokeIpc, desktopBridge, getActiveModule, setFeedback, recordUndo, noteActivity } = deps;
    if (typeof host.__EASYOFFICE_CREATE_BASE_STUDIO__ === "function") {
      baseStudio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
        document,
        rootEl: document.getElementById("pane-base"),
        invokeIpc,
        deliverExport,
        getFileBase: () => resolveModuleFileBase("BASE", "EasyOffice_BASE_문서"),
        setFeedback,
        recordUndo,
        captureUndo: deps.captureUndo,
        commitUndo: deps.commitUndo,
        recordCellUndo: deps.commitUndo ? (delta) => deps.commitUndo(delta) : null,
        noteActivity,
      });
      baseStudio.bind();
    }
    if (typeof host.__EASYOFFICE_CREATE_BOARD_STUDIO__ === "function") {
      boardStudio = host.__EASYOFFICE_CREATE_BOARD_STUDIO__({
        document,
        env: env || host,
        rootEl: document.getElementById("pane-board"),
        getActiveModule,
        invokeIpc,
        desktopBridge,
        deliverExport,
        notify: setFeedback,
        recordUndo,
        noteActivity,
        getTitle: () => resolveModuleFileBase("BOARD", "EasyOffice_BOARD_문서"),
      });
    }
    (env || host).addEventListener?.("keydown", (ev) => {
      if (getActiveModule?.() === "BOARD") boardStudio?.handleKeydown?.(ev);
    });
    for (const id of ["base-field-dialog", "board-analysis-dialog", "board-chart-dialog"]) {
      const dlg = document.getElementById(id);
      if (dlg && deps.shellChrome) {
        deps.shellChrome.enableDialogDrag?.(dlg);
        deps.shellChrome.enableDialogKeys?.(dlg);
      }
    }
    return api;
  }

  function captureSnapshotFields() {
    return {
      baseState: baseStudio ? baseStudio.getState() : null,
      boardState: boardStudio ? boardStudio.getState() : null,
    };
  }

  function restoreSnapshotFields(snap) {
    if (snap?.baseState && baseStudio) baseStudio.setState(snap.baseState);
    if (snap?.boardState && boardStudio) boardStudio.replaceState(snap.boardState);
  }

  function applyCellUndo(record) {
    return (baseStudio || host.__EASYOFFICE_BASE_STUDIO__)?.applyUndoEdits?.(record?.tableId, record?.edits) || null;
  }

  function buildConversionContent(mod, title) {
    if (mod === "BASE") {
      const M = host.__EASYOFFICE_BASE_MODEL__;
      const st = baseStudio ? baseStudio.getState() : M.createInitialBaseState();
      return { sheets: M.toSheetsPayload(st) };
    }
    const payload = boardStudio ? boardStudio.toSlideDeckPdfPayload(title) : { aspect: "16:9", slides: [{ title: title || "보드", bullets: [], notes: "", layout: "blank", transition: "none", elements: [] }] };
    return { aspect: payload.aspect || "16:9", slides: payload.slides };
  }

  // ---- edit package (.eobase / .eoboard, PD-265) ----------------------------------------------------------------------
  // The package holds the module's model; PDF and XLSX are exports. The engine builds and reads the container and checks
  // its limits (`native_package.rs`); the model is checked here by the module's own reader before anything is replaced.

  const PACKAGE_EXT = Object.freeze({ BASE: "eobase", BOARD: "eoboard" });
  const MODEL_VERSION = 1;

  function boardPackageParts() {
    const utils = host.__EASYOFFICE_SHEET_UTILS__, state = boardStudio.getState();
    const media = [], pathByImage = new Map();
    const nodes = state.nodes.map((node) => {
      if (node.kind !== "image") return node;
      let path = pathByImage.get(node.imageId);
      if (!path) {
        const image = utils.getDocImage(node.imageId);
        if (!image) throw new Error("그림 데이터를 찾을 수 없어 저장하지 못했습니다: " + (node.alt || node.id));
        path = "media/img-" + (media.length + 1) + "." + (image.format === "jpg" ? "jpg" : "png");
        media.push({ path, base64: image.base64 });
        pathByImage.set(node.imageId, path);
      }
      const { imageId, ...rest } = node;
      return { ...rest, media: path };
    });
    return { model: { version: MODEL_VERSION, nodes, connectors: state.connectors, viewport: state.viewport }, media };
  }

  // The `convert_document_format` request that builds the package of the open BASE or BOARD document.
  function buildPackageRequest(mod) {
    if (mod === "BASE") {
      if (!baseStudio) throw new Error("데이터베이스를 사용할 수 없습니다.");
      return { action: "encode_package", kind: "base", model: JSON.stringify(baseStudio.getState()), media: [] };
    }
    if (!boardStudio) throw new Error("보드를 사용할 수 없습니다.");
    const { model, media } = boardPackageParts();
    return { action: "encode_package", kind: "board", model: JSON.stringify(model), media };
  }

  async function savePackage(mod, deliver, fileBase) {
    let reply;
    try {
      reply = await runtime.invokeIpc("convert_document_format", JSON.stringify(buildPackageRequest(mod)));
    } catch (err) {
      return { ok: false, failure: err?.message || String(err) };
    }
    const base64 = String(reply?.convertedBase64 || "");
    if (!base64) return { ok: false, failure: "패키지 결과가 비어 있습니다" };
    const delivery = await deliver(base64, fileBase, PACKAGE_EXT[mod]);
    return { ...delivery, bytes: Number(reply.outputBytes) || 0 };
  }

  function parseModelText(text) {
    let model;
    try { model = JSON.parse(String(text)); } catch { throw new Error("파일의 모델을 읽을 수 없습니다."); }
    if (!model || typeof model !== "object" || Array.isArray(model)) throw new Error("파일의 모델 형식이 올바르지 않습니다.");
    return model;
  }

  function decodeBoardModel(model, media) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__, paths = new Set(media.map((item) => item.path));
    const nodes = Array.isArray(model.nodes) ? model.nodes : [];
    for (const node of nodes) {
      if (node?.kind === "image" && !paths.has(node.media)) throw new Error("그림 파일이 패키지에 없습니다: " + String(node.media));
    }
    // The model is checked first (an image node stands in with a placeholder id); the pictures go into the store only once it is accepted.
    const checked = host.__EASYOFFICE_BOARD_CODEC__.normalizeBoardState({ ...model, nodes: nodes.map((node) => (node?.kind === "image" ? { ...node, imageId: node.media } : node)) }, true);
    const idByPath = new Map(media.map((item) => [item.path, utils.registerDocImage(item.base64, item.mediaType === "image/jpeg" ? "jpg" : "png")]));
    const state = { ...checked, nodes: checked.nodes.map((node) => (node.kind === "image" ? { ...node, imageId: idByPath.get(node.imageId) } : node)) };
    return { fields: { boardState: { nodes: state.nodes, connectors: state.connectors, viewport: state.viewport } }, itemCount: state.nodes.length };
  }

  // The decoded reply of an opened package as the workspace snapshot fields it replaces; throws when the model is not valid.
  function decodePackage(reply) {
    const model = parseModelText(reply.model);
    if (reply.module === "BASE") {
      const hydrated = host.__EASYOFFICE_BASE_MODEL__.hydrateBaseState(model);
      if (!hydrated.ok) throw new Error(hydrated.error);
      return { fields: { baseState: hydrated.state }, itemCount: hydrated.state.tables.length };
    }
    return decodeBoardModel(model, Array.isArray(reply.media) ? reply.media : []);
  }

  function buildPrintPages(mod, doc, rules) {
    if (mod === "BASE") {
      if (!baseStudio) throw new Error("인쇄할 내용이 없습니다.");
      return baseStudio.buildPrintPages(doc, rules);
    }
    if (mod === "BOARD") {
      if (!boardStudio) throw new Error("인쇄할 내용이 없습니다.");
      return [boardStudio.buildPrintPage(doc, rules)];
    }
    throw new Error("현재 창에서는 인쇄를 지원하지 않습니다.");
  }

  function outlineItems(mod) {
    if (mod === "BASE" && baseStudio) {
      const st = baseStudio.getState();
      return (st.tables || []).map((t, idx) => ({
        label: idx + 1 + ". " + t.name + " (" + (t.records?.length || 0) + "건)",
        action: () => {
          const btn = runtime?.document?.querySelector(`.base-table-tab[data-table-id="${cssEscape(t.id)}"]`);
          btn?.click?.();
        },
        active: t.id === st.activeTableId,
      }));
    }
    if (mod === "BOARD" && boardStudio) {
      const st = boardStudio.getState();
      if (!st.nodes?.length) return [{ label: "1. 빈 보드 캔버스", action: () => {}, active: true }];
      return st.nodes.slice(0, 32).map((n, idx) => ({
        label: idx + 1 + ". " + String(n.text || n.title || n.alt || n.kind),
        action: () => boardStudio.selectNode(n.id),
        active: idx === 0,
      }));
    }
    return null;
  }

  function extractPlainText(mod) {
    if (mod === "BASE" && baseStudio) {
      const M = host.__EASYOFFICE_BASE_MODEL__, st = baseStudio.getState();
      return (st.tables || []).map((t) => {
        const q = M.queryTableView(st, t.id);
        const lines = [t.name, q.fields.map((f) => f.name).join(" ")];
        for (const r of q.rows) lines.push(q.fields.map((f) => r.displays[f.id] || "").filter(Boolean).join(" "));
        return lines.join("\n");
      }).join("\n");
    }
    if (mod === "BOARD" && boardStudio) {
      const st = boardStudio.getState();
      return (st.nodes || []).map((n) => String(n.text || n.title || n.alt || "").trim()).filter(Boolean).join("\n");
    }
    return "";
  }

  // The limits match what the engine accepts (`MAX_BASE_*`, `MAX_BOARD_*` in `copilot/types.rs`); it refuses a request over
  // them, so the page cuts first and tells the engine how much there is (`totalRecords`, `totalNodes`).
  const COPILOT_MAX_TABLES = 64, COPILOT_MAX_FIELDS = 64, COPILOT_MAX_RECORDS = 100, COPILOT_MAX_NODES = 256, COPILOT_MAX_CONNECTORS = 1024, COPILOT_MAX_TEXT = 200;
  const clipText = (v) => host.__EASYOFFICE_COPILOT_TEXT__.clipCodePoints(v, COPILOT_MAX_TEXT);

  function extractCopilotContext(mod) {
    if (mod === "BASE" && baseStudio) {
      const M = host.__EASYOFFICE_BASE_MODEL__, st = baseStudio.getState();
      const tbl = M.getTable(st, st.activeTableId) || st.tables?.[0];
      const q = tbl ? M.queryTableView(st, tbl.id) : { fields: [], rows: [] };
      const fields = q.fields.slice(0, COPILOT_MAX_FIELDS);
      return {
        selection: { range: tbl?.name || "테이블 1" },
        context: {
          base: {
            tables: (st.tables || []).slice(0, COPILOT_MAX_TABLES).map((t) => clipText(t.name)),
            table: clipText(tbl?.name || "테이블 1"),
            fields: fields.map((f) => ({ name: clipText(f.name), type: String(f.type || "text") })),
            records: q.rows.slice(0, COPILOT_MAX_RECORDS).map((r) => fields.map((f) => clipText(r.displays[f.id]))),
            totalRecords: q.rows.length,
          },
        },
      };
    }
    if (mod === "BOARD" && boardStudio) {
      const rep = boardStudio.analyze?.() || { totalNodes: 0, connectorCount: 0 };
      const st = boardStudio.getState();
      return {
        selection: { range: `요소 ${rep.totalNodes}개 · 연결선 ${rep.connectorCount}개` },
        context: {
          board: {
            nodes: (st.nodes || []).slice(0, COPILOT_MAX_NODES).map((n) => ({ id: clipText(n.id), kind: String(n.kind || ""), text: clipText(n.text || n.title || n.alt) })),
            connectors: (st.connectors || []).slice(0, COPILOT_MAX_CONNECTORS).map((c) => ({ from: clipText(c.fromId), to: clipText(c.toId) })),
            totalNodes: (st.nodes || []).length,
          },
        },
      };
    }
    return null;
  }

  const api = {
    init,
    getBaseStudio: () => baseStudio || host.__EASYOFFICE_BASE_STUDIO__,
    getBoardStudio: () => boardStudio || host.__EASYOFFICE_BOARD_STUDIO__,
    captureSnapshotFields,
    restoreSnapshotFields,
    applyCellUndo,
    buildConversionContent,
    savePackage,
    decodePackage,
    buildPrintPages,
    outlineItems,
    extractPlainText,
    extractCopilotContext,
    deliverExport,
  };

  host.__EASYOFFICE_BASE_BOARD_HOST__ = api;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
