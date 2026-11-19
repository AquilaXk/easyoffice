// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` (MIT, Copyright (c) 2025-2026 Edward Kim;
// see `THIRD_PARTY_NOTICES.md`) — 셸 통합 어댑터 (PD-259 BASE · PD-260 BOARD).

(function (host) {
  "use strict";

  let baseStudio = null;
  let boardStudio = null;
  let runtime = null;

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
        const res = await bridge.saveFile(base64Data, fileName, { inPlace: false });
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

  function buildConversionContent(mod, title) {
    if (mod === "BASE") {
      const M = host.__EASYOFFICE_BASE_MODEL__;
      const st = baseStudio ? baseStudio.getState() : M.createInitialBaseState();
      return { sheets: M.toSheetsPayload(st) };
    }
    const payload = boardStudio ? boardStudio.toSlideDeckPdfPayload(title) : { aspect: "16:9", slides: [{ title: title || "보드", bullets: [], notes: "", layout: "blank", transition: "none", elements: [] }] };
    return { aspect: payload.aspect || "16:9", slides: payload.slides };
  }

  function buildPrintPages(mod, doc, rules) {
    if (mod === "BASE") {
      if (!baseStudio) throw new Error("인쇄할 내용이 없습니다.");
      return [baseStudio.buildPrintPage(doc, rules)];
    }
    if (mod === "BOARD") {
      if (!boardStudio) throw new Error("인쇄할 내용이 없습니다.");
      return [boardStudio.buildPrintPage(doc, rules)];
    }
    throw new Error("이 모듈은 인쇄를 지원하지 않습니다.");
  }

  function outlineItems(mod) {
    if (mod === "BASE" && baseStudio) {
      const st = baseStudio.getState();
      return (st.tables || []).map((t, idx) => ({
        label: idx + 1 + ". " + t.name + " (" + (t.records?.length || 0) + "건)",
        action: () => {
          const btn = runtime?.document?.querySelector(`.base-table-tab[data-table-id="${t.id}"]`);
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

  function extractCopilotContext(mod) {
    if (mod === "BASE" && baseStudio) {
      const M = host.__EASYOFFICE_BASE_MODEL__, st = baseStudio.getState();
      const tbl = M.getTable(st, st.activeTableId) || st.tables?.[0];
      const q = tbl ? M.queryTableView(st, tbl.id) : { fields: [], rows: [] };
      return {
        selection: { range: tbl?.name || "테이블 1" },
        context: {
          doc: {
            paragraphs: [
              `테이블: ${tbl?.name || "테이블 1"} · 보기: ${tbl?.view || "grid"} · 레코드: ${q.rows.length}건`,
              ...q.rows.slice(0, 64).map((r) => q.fields.map((f) => `${f.name}=${r.displays[f.id] || ""}`).join(", ")),
            ],
            selectedText: "",
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
          doc: {
            paragraphs: [
              `보드 요약: 요소 ${rep.totalNodes}개, 연결선 ${rep.connectorCount}개, 그룹 ${rep.connectedComponents || 0}개`,
              ...(st.nodes || []).slice(0, 64).map((n) => `[${n.kind}] ${n.text || n.title || n.alt || n.id} (${n.x},${n.y})`),
            ],
            selectedText: "",
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
    buildConversionContent,
    buildPrintPages,
    outlineItems,
    extractPlainText,
    extractCopilotContext,
    deliverExport,
  };

  host.__EASYOFFICE_BASE_BOARD_HOST__ = api;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
