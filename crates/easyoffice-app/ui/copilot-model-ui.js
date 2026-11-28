// 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., `web/src/features/copilot/model-picker.tsx`, `web/src/features/settings/index.tsx`) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., `gui/src/components/modelSelection/ModelSelect.tsx`, `gui/src/forms/AddModelForm.tsx`, see THIRD_PARTY_NOTICES.md)

(function (host) {
  "use strict";

  const CUSTOM_MODEL_SENTINEL = "__custom__";

  function isValidLocalEndpoint(url) {
    return /^http:\/\/(?:127\.0\.0\.1|\[::1\]):\d{1,5}(?:\/|$)/i.test(url);
  }

  function dispatchChange(el) {
    if (!el) return;
    const ev = typeof host.Event === "function"
      ? new host.Event("change", { bubbles: true })
      : { type: "change", target: el };
    el.dispatchEvent(ev);
  }

  function createCopilotModelUi({ document, invokeIpc, isRunning, onChanged }) {
    const catalog = host.__EASYOFFICE_COPILOT_CATALOG__;
    const byId = (id) => document.getElementById(id);

    let isMenuOpen = false;
    let providerKind = "";
    let providerBaseUrl = "";
    let providerModel = "";
    let reasoningEffort = catalog.DEFAULT_REASONING_EFFORT;

    const keysByProvider = new Map();
    // The address each saved key was entered for: a key is sent to that address only (PD-253).
    const keyUrlsByProvider = new Map();
    const urlsByProvider = new Map();
    const modelsByProvider = new Map();
    const fastModelsByProvider = new Map();
    const discoveredByProvider = new Map();

    const keyFor = (kind, baseUrl) => (keyUrlsByProvider.get(kind) === baseUrl ? keysByProvider.get(kind) || "" : "");

    function isConfigured() {
      if (!providerKind || !providerModel) return false;
      const prov = catalog.getProvider(providerKind);
      if (!prov) return false;
      return !prov.requiresKey || Boolean(keysByProvider.get(providerKind));
    }

    function syncModelButton() {
      const modelBtn = byId("btn-copilot-model");
      const modelText = byId("copilot-model-btn-text");
      if (!modelBtn) return;
      const label = catalog.formatButtonLabel(providerKind, providerModel, reasoningEffort, discoveredByProvider);
      if (modelText) modelText.textContent = label;
      else modelBtn.textContent = label;
      modelBtn.disabled = Boolean(isRunning?.());
      modelBtn.setAttribute("aria-expanded", isMenuOpen ? "true" : "false");
      const prov = catalog.getProvider(providerKind);
      modelBtn.setAttribute(
        "title",
        prov && providerModel ? `${prov.label} · ${providerModel}` : "모델 선택"
      );
    }

    function currentProviderPayload() {
      const canReason = catalog.supportsReasoning(providerKind, providerModel, discoveredByProvider);
      return {
        kind: providerKind,
        baseUrl: providerBaseUrl,
        model: providerModel,
        apiKey: providerKind ? keyFor(providerKind, providerBaseUrl) : "",
        reasoningEffort: canReason ? reasoningEffort : "",
      };
    }

    function setFastModel(modelId, kind = providerKind) {
      const targetKind = String(kind || providerKind || "").trim();
      if (!targetKind) return "";
      const clean = String(modelId || "").trim();
      if (clean) fastModelsByProvider.set(targetKind, clean);
      else fastModelsByProvider.delete(targetKind);
      return clean;
    }

    function getFastModel(kind = providerKind) {
      return fastModelsByProvider.get(String(kind || providerKind || "")) || "";
    }

    function getFastProviderPayload() {
      const base = currentProviderPayload();
      if (!base.kind || !base.model) return base;
      const overrideId = fastModelsByProvider.get(base.kind) || "";
      const fastId = overrideId || catalog.findFastSiblingModel?.(base.kind, base.model, discoveredByProvider) || "";
      if (!fastId || fastId === base.model) return base;
      const canReason = catalog.supportsReasoning(base.kind, fastId, discoveredByProvider);
      return {
        ...base,
        model: fastId,
        reasoningEffort: canReason ? "low" : "",
      };
    }

    function closeMenu(restoreFocus = false) {
      const menu = byId("copilot-model-menu");
      const trigger = byId("btn-copilot-model");
      if (!menu || !isMenuOpen) return;
      isMenuOpen = false;
      menu.hidden = true;
      trigger?.setAttribute("aria-expanded", "false");
      if (restoreFocus) trigger?.focus?.();
    }

    function positionMenu() {
      const menu = byId("copilot-model-menu");
      const trigger = byId("btn-copilot-model");
      if (!menu || !trigger || typeof trigger.getBoundingClientRect !== "function") return;
      const rect = trigger.getBoundingClientRect();
      const vh = Number(host.innerHeight) || 800;
      // 높이는 자리가 넓은 쪽에 맞추고, 쪽은 flip(`copilot-popup-menu.js` `placeMenu`)이 정한다.
      const availAbove = Math.max(220, Math.min(620, Math.round(rect.top - 18)));
      const availBelow = Math.max(220, Math.min(620, Math.round(vh - rect.bottom - 18)));
      menu.style.maxHeight = Math.max(availAbove, availBelow) + "px";
      menu.style.right = "auto";
      host.__EASYOFFICE_COPILOT_POPUP__.placeMenu({ view: host, menu, box: rect, preferred: availAbove >= availBelow ? "up" : "down", align: "start" });
    }

    function applyMenuRolesFallback(groupEl) {
      const hasNativeFocusGroup = Boolean(
        host.HTMLElement?.prototype && "focusGroup" in host.HTMLElement.prototype
      );
      if (hasNativeFocusGroup || !groupEl) return;
      groupEl.setAttribute("role", "menu");
      groupEl.querySelectorAll("button:not([role])").forEach((btn) => {
        btn.setAttribute("role", "menuitem");
      });
    }

    async function discoverModelsLive(kind, baseUrl, apiKey, optional = false) {
      if (!kind) return [];
      const prov = catalog.getProvider(kind);
      if (!prov || (prov.requiresKey && !apiKey)) return [];
      const res = await invokeIpc(
        "run_smart_assistant",
        JSON.stringify({
          action: "copilot_list_models",
          discover: true,
          optional,
          provider: {
            kind,
            baseUrl: baseUrl || prov.defaultBaseUrl,
            model: modelsByProvider.get(kind) || "",
            apiKey: apiKey || "",
          },
        })
      );
      const list = Array.isArray(res?.models) ? res.models : [];
      if (list.length > 0) {
        discoveredByProvider.set(kind, list);
        if (isMenuOpen) renderMenu();
        syncModelButton();
      }
      return list;
    }

    function appendMenuChoice(container, { checked, titleText, subText, hintText, dataset, onSelect }) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.setAttribute("role", "menuitemradio");
      btn.setAttribute("aria-checked", checked ? "true" : "false");
      if (dataset) {
        for (const [k, v] of Object.entries(dataset)) btn.dataset[k] = v;
      }
      const body = document.createElement("span");
      body.className = "eo-copilot-menu-item-body";
      const title = document.createElement("span");
      title.className = "eo-copilot-menu-item-title";
      title.textContent = titleText;
      body.appendChild(title);
      if (subText) {
        const sub = document.createElement("span");
        sub.className = "eo-copilot-menu-item-sub";
        sub.textContent = subText;
        body.appendChild(sub);
      }
      btn.appendChild(body);
      if (hintText) {
        const hint = document.createElement("span");
        hint.className = "eo-copilot-menu-hint";
        hint.setAttribute("aria-hidden", "true");
        hint.textContent = hintText;
        btn.appendChild(hint);
      }
      btn.addEventListener("click", onSelect);
      container.appendChild(btn);
    }

    function selectProviderModel(kind, modelId) {
      const prov = catalog.getProvider(kind);
      if (!prov || !modelId) return;
      providerKind = kind;
      providerModel = modelId;
      providerBaseUrl = urlsByProvider.get(kind) || prov.defaultBaseUrl;
      modelsByProvider.set(kind, providerModel);
      closeMenu(false);
      syncModelButton();
      onChanged?.();
    }

    async function handleQuickLocalSelect(kind) {
      const prov = catalog.getProvider(kind);
      if (!prov) return;
      const savedModel = modelsByProvider.get(kind);
      const cached = catalog.discoveredModelsFor(kind, discoveredByProvider);
      if (cached.length > 0 || savedModel) {
        selectProviderModel(kind, savedModel || cached[0].id);
        return;
      }
      try {
        const list = await discoverModelsLive(kind, urlsByProvider.get(kind) || prov.defaultBaseUrl, "", true);
        if (list.length > 0) {
          selectProviderModel(kind, list[0].id);
          return;
        }
      } catch {
        // Fall through to Model Settings when local server is not running or returns empty list.
      }
      closeMenu(false);
      openDialog(kind, "", `${prov.label}에서 실행 중인 모델을 불러오거나 모델 ID를 입력해 주세요.`);
    }

    function configuredProviderKinds() {
      const out = [];
      for (const p of catalog.PROVIDERS) {
        const hasKey = Boolean(keysByProvider.get(p.id));
        const hasModels = catalog.discoveredModelsFor(p.id, discoveredByProvider).length > 0 || Boolean(modelsByProvider.get(p.id));
        if (p.requiresKey ? hasKey : hasModels) {
          out.push(p.id);
        }
      }
      return out;
    }

    function renderMenu() {
      const groupEl = byId("copilot-model-menu-group");
      if (!groupEl) return;

      const scrollWrap = document.createElement("div");
      scrollWrap.className = "eo-copilot-menu-scroll";
      const scrollItems = [];
      const configuredKinds = configuredProviderKinds();

      // 1. Live-discovered models of each configured provider first (see THIRD_PARTY_NOTICES.md)
      for (const kind of configuredKinds) {
        const prov = catalog.getProvider(kind);
        if (!prov) continue;
        const models = [...catalog.discoveredModelsFor(kind, discoveredByProvider)];
        const activeSaved = kind === providerKind ? providerModel : modelsByProvider.get(kind);
        if (activeSaved && !models.some((m) => m.id === activeSaved)) {
          models.unshift({
            id: activeSaved,
            name: activeSaved,
            contextWindow: 0,
            supportsReasoning: catalog.supportsReasoning(kind, activeSaved, discoveredByProvider),
          });
        }
        if (models.length === 0) continue;

        const secHead = document.createElement("div");
        secHead.className = "eo-copilot-menu-heading";
        secHead.textContent = prov.label;
        scrollItems.push(secHead);

        const companyGroup = document.createElement("div");
        companyGroup.className = "eo-copilot-company-list";
        companyGroup.setAttribute("role", "group");
        companyGroup.setAttribute("aria-label", prov.label);
        for (const m of models) {
          const ctx = catalog.formatContextWindow(m.contextWindow);
          const sub = [m.id !== m.name ? m.id : "", ctx ? `${ctx} 컨텍스트` : ""].filter(Boolean).join(" · ");
          appendMenuChoice(companyGroup, {
            checked: providerKind === kind && providerModel === m.id,
            titleText: m.name,
            subText: sub,
            dataset: { quickKind: kind, quickModel: m.id },
            onSelect: () => selectProviderModel(kind, m.id),
          });
        }
        scrollItems.push(companyGroup);
        scrollItems.push(document.createElement("hr"));
      }

      // 2. 3-tier reasoning effort selector (낮음 / 보통 / 높음) when active model supports reasoning
      if (catalog.supportsReasoning(providerKind, providerModel, discoveredByProvider)) {
        const effHead = document.createElement("div");
        effHead.className = "eo-copilot-menu-heading";
        effHead.textContent = "추론 수준";
        scrollItems.push(effHead);

        const effGroup = document.createElement("div");
        effGroup.className = "eo-copilot-effort-group";
        effGroup.setAttribute("role", "group");
        effGroup.setAttribute("aria-label", "추론 수준");
        for (const eff of catalog.REASONING_EFFORTS) {
          const effBtn = document.createElement("button");
          effBtn.type = "button";
          effBtn.setAttribute("role", "menuitemradio");
          effBtn.setAttribute("aria-checked", reasoningEffort === eff.value ? "true" : "false");
          effBtn.dataset.effortChoice = eff.value;
          const labelSpan = document.createElement("span");
          labelSpan.className = "eo-copilot-effort-label";
          labelSpan.textContent = eff.label;
          effBtn.appendChild(labelSpan);
          effBtn.addEventListener("click", () => {
            reasoningEffort = eff.value;
            closeMenu(false);
            syncModelButton();
            onChanged?.();
          });
          effGroup.appendChild(effBtn);
        }
        scrollItems.push(effGroup);
        scrollItems.push(document.createElement("hr"));
      }

      // 3. Unconfigured cloud companies (API key setup) and local runners (live /models fetch)
      const remainingItems = catalog.QUICK_PROVIDER_ITEMS.filter((it) => !configuredKinds.includes(it.kind));
      if (remainingItems.length > 0) {
        const baseHeading = document.createElement("div");
        baseHeading.className = "eo-copilot-menu-heading";
        baseHeading.textContent = configuredKinds.length > 0 ? "다른 제공자 연결" : "제공자 선택";
        scrollItems.push(baseHeading);

        const radioGroup = document.createElement("div");
        radioGroup.setAttribute("role", "group");
        radioGroup.setAttribute("aria-label", "제공자 선택");
        for (const item of remainingItems) {
          const prov = catalog.getProvider(item.kind);
          if (!prov) continue;
          const needsKey = prov.requiresKey && !keysByProvider.get(item.kind);
          appendMenuChoice(radioGroup, {
            checked: false,
            titleText: item.title,
            subText: item.subtitle,
            hintText: needsKey ? "키 필요" : "로컬 조회",
            dataset: { quickKind: item.kind },
            onSelect: () => {
              if (needsKey) {
                closeMenu(false);
                openDialog(item.kind);
                return;
              }
              void handleQuickLocalSelect(item.kind);
            },
          });
        }
        scrollItems.push(radioGroup);
      } else if (scrollItems.at(-1)?.tagName === "HR") {
        scrollItems.pop();
      }

      scrollWrap.replaceChildren(...scrollItems);

      // 4. Pinned footer actions (never clipped by model list scroll; plain 13px/400 menu item hierarchy)
      const footerWrap = document.createElement("div");
      footerWrap.className = "eo-copilot-menu-footer";

      const activeProv = catalog.getProvider(providerKind);
      if (activeProv && (!activeProv.requiresKey || keysByProvider.get(providerKind))) {
        const refreshBtn = document.createElement("button");
        refreshBtn.type = "button";
        refreshBtn.id = "btn-copilot-menu-refresh";
        refreshBtn.className = "eo-copilot-menu-action";
        const refreshTxt = document.createElement("span");
        refreshTxt.className = "eo-copilot-menu-action-label";
        refreshTxt.textContent = "모델 목록 새로 고침";
        refreshBtn.appendChild(refreshTxt);
        refreshBtn.addEventListener("click", () => {
          void discoverModelsLive(providerKind, providerBaseUrl, keyFor(providerKind, providerBaseUrl), true);
        });
        footerWrap.appendChild(refreshBtn);
      }

      const manageBtn = document.createElement("button");
      manageBtn.type = "button";
      manageBtn.id = "btn-copilot-menu-manage";
      manageBtn.className = "eo-copilot-menu-action";
      const manageTxt = document.createElement("span");
      manageTxt.className = "eo-copilot-menu-action-label";
      manageTxt.textContent = "모델 설정…";
      manageBtn.appendChild(manageTxt);
      manageBtn.addEventListener("click", () => {
        closeMenu(false);
        openDialog();
      });
      footerWrap.appendChild(manageBtn);

      groupEl.replaceChildren(scrollWrap, document.createElement("hr"), footerWrap);
      applyMenuRolesFallback(groupEl);
    }

    function toggleMenu(enterWithKeyboard = false) {
      const menu = byId("copilot-model-menu");
      if (!menu || isRunning?.()) return;
      if (isMenuOpen) {
        closeMenu(enterWithKeyboard);
        return;
      }
      renderMenu();
      isMenuOpen = true;
      menu.hidden = false;
      positionMenu();
      byId("btn-copilot-model")?.setAttribute("aria-expanded", "true");
      if (enterWithKeyboard) {
        menu.querySelector("button:not([disabled])")?.focus?.();
      }
    }

    function clearKeyField() {
      const keyInput = byId("copilot-provider-key");
      if (!keyInput) return;
      keyInput.value = "";
      keyInput.removeAttribute("value");
    }

    function populatePresetOptions(kind, currentModelId) {
      const presetSelect = byId("copilot-provider-preset");
      if (!presetSelect) return;
      const list = catalog.discoveredModelsFor(kind, discoveredByProvider);
      const opts = [];
      for (const m of list) {
        const opt = document.createElement("option");
        opt.value = m.id;
        const ctx = catalog.formatContextWindow(m.contextWindow);
        opt.textContent = ctx ? `${m.name} (${ctx})` : m.name;
        opts.push(opt);
      }
      const customOpt = document.createElement("option");
      customOpt.value = CUSTOM_MODEL_SENTINEL;
      customOpt.textContent = "직접 입력…";
      opts.push(customOpt);
      presetSelect.replaceChildren(...opts);
      const hasMatch = Boolean(currentModelId) && list.some((m) => m.id === currentModelId);
      presetSelect.value = hasMatch ? currentModelId : CUSTOM_MODEL_SENTINEL;
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect(presetSelect);
    }

    function syncDialogEffortState(kind, modelId) {
      const effortSelect = byId("copilot-reasoning-effort");
      if (!effortSelect) return;
      const list = catalog.discoveredModelsFor(kind, discoveredByProvider);
      const matched = list.find((m) => m.id === modelId);
      const explicitlyUnsupported = matched && matched.supportsReasoning === false;
      effortSelect.disabled = Boolean(explicitlyUnsupported);
      host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect(effortSelect);
    }

    function syncDialogInputs(applyDefaultsForKind = false) {
      const kind = byId("copilot-provider-kind")?.value || "openai";
      const prov = catalog.getProvider(kind) || catalog.PROVIDERS[0];
      const urlInput = byId("copilot-provider-url");
      const modelInput = byId("copilot-provider-model");
      const keyInput = byId("copilot-provider-key");

      if (applyDefaultsForKind) {
        const savedUrl = urlsByProvider.get(kind) ?? prov.defaultBaseUrl;
        const knownModels = catalog.discoveredModelsFor(kind, discoveredByProvider);
        const savedModel = modelsByProvider.get(kind) ?? (knownModels[0]?.id || "");
        if (urlInput) urlInput.value = savedUrl;
        if (modelInput) modelInput.value = savedModel;
        clearKeyField();
      }

      const curModel = String(modelInput?.value || "").trim();
      populatePresetOptions(kind, curModel);
      syncDialogEffortState(kind, curModel);

      if (urlInput) {
        urlInput.placeholder = prov.defaultBaseUrl || (prov.isLocal ? "http://127.0.0.1:11434/v1" : "https://api.example.com/v1");
      }
      if (keyInput) keyInput.placeholder = keysByProvider.get(kind) ? "저장됨" : "";
    }

    function openDialog(overrideKind, overrideModel, initialStatus = "") {
      const dlg = byId("copilot-model-dialog");
      if (!dlg || typeof dlg.showModal !== "function") return;
      const targetKind = overrideKind || providerKind || "openai";
      const prov = catalog.getProvider(targetKind) || catalog.PROVIDERS[0];
      const kindSelect = byId("copilot-provider-kind");
      const urlInput = byId("copilot-provider-url");
      const modelInput = byId("copilot-provider-model");
      const effortSelect = byId("copilot-reasoning-effort");
      const statusEl = byId("copilot-model-status");

      if (kindSelect) {
        kindSelect.value = prov.id;
        host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect(kindSelect);
      }
      if (urlInput) {
        urlInput.value = overrideKind || !providerKind
          ? (urlsByProvider.get(prov.id) || prov.defaultBaseUrl)
          : (providerBaseUrl || prov.defaultBaseUrl);
      }
      const knownModels = catalog.discoveredModelsFor(prov.id, discoveredByProvider);
      if (modelInput) {
        modelInput.value = overrideModel !== undefined
          ? overrideModel
          : (overrideKind || !providerKind
              ? (modelsByProvider.get(prov.id) || knownModels[0]?.id || "")
              : (providerModel || knownModels[0]?.id || ""));
      }
      if (effortSelect) {
        effortSelect.value = reasoningEffort || catalog.DEFAULT_REASONING_EFFORT;
        host.__EASYOFFICE_CUSTOM_SELECT__?.syncCustomSelect(effortSelect);
      }
      clearKeyField();
      syncDialogInputs(false);
      if (statusEl) statusEl.textContent = initialStatus;
      dlg.showModal();
      if (prov.requiresKey && !keysByProvider.get(prov.id)) {
        byId("copilot-provider-key")?.focus?.();
      }
    }

    function closeDialog() {
      clearKeyField();
      byId("copilot-model-dialog")?.close?.();
    }

    function readDraftProvider() {
      const kind = byId("copilot-provider-kind")?.value || "openai";
      const prov = catalog.getProvider(kind) || catalog.PROVIDERS[0];
      const rawUrl = String(byId("copilot-provider-url")?.value || "").trim();
      const rawModel = String(byId("copilot-provider-model")?.value || "").trim();
      const typedKey = String(byId("copilot-provider-key")?.value || "").trim();
      const effort = byId("copilot-reasoning-effort")?.value || catalog.DEFAULT_REASONING_EFFORT;
      const baseUrl = rawUrl || prov.defaultBaseUrl;
      const canReuseSavedKey = baseUrl === keyUrlsByProvider.get(kind);
      return {
        kind,
        prov,
        baseUrl,
        model: rawModel,
        typedKey,
        apiKey: typedKey || (canReuseSavedKey ? (keysByProvider.get(kind) || "") : ""),
        reasoningEffort: effort,
      };
    }

    async function fetchProviderModels() {
      const statusEl = byId("copilot-model-status");
      const draft = readDraftProvider();
      if (draft.prov.requiresKey && !draft.apiKey) {
        if (statusEl) statusEl.textContent = `${draft.prov.label} API 키를 먼저 입력해 주세요.`;
        return [];
      }
      if (statusEl) statusEl.textContent = "사용 가능한 모델 목록을 불러오는 중…";
      try {
        const list = await discoverModelsLive(draft.kind, draft.baseUrl, draft.apiKey, false);
        if (list.length > 0) {
          const modelInput = byId("copilot-provider-model");
          const nextModel = draft.model && list.some((m) => m.id === draft.model) ? draft.model : list[0].id;
          if (modelInput) modelInput.value = nextModel;
          populatePresetOptions(draft.kind, nextModel);
          syncDialogEffortState(draft.kind, nextModel);
          if (statusEl) statusEl.textContent = `사용 가능한 모델 ${list.length}개를 불러왔습니다.`;
        } else if (statusEl) {
          statusEl.textContent = "조회된 모델이 없습니다. 모델 ID를 직접 입력해 주세요.";
        }
        return list;
      } catch (err) {
        if (statusEl) statusEl.textContent = String(err?.message || "모델 목록 불러오기 실패");
        return [];
      }
    }

    async function testProviderConnection() {
      const statusEl = byId("copilot-model-status");
      const draft = readDraftProvider();
      if (draft.prov.requiresKey && !draft.apiKey) {
        if (statusEl) statusEl.textContent = `${draft.prov.label} API 키를 먼저 입력해 주세요.`;
        return;
      }
      if (statusEl) statusEl.textContent = "연결 확인 중…";
      try {
        const canReason = catalog.supportsReasoning(draft.kind, draft.model, discoveredByProvider);
        const res = await invokeIpc(
          "run_smart_assistant",
          JSON.stringify({
            action: "copilot_test_provider",
            provider: {
              kind: draft.kind,
              baseUrl: draft.baseUrl,
              model: draft.model,
              apiKey: draft.apiKey,
              reasoningEffort: canReason ? draft.reasoningEffort : "",
            },
          })
        );
        if (statusEl) statusEl.textContent = String(res?.message || "연결 확인 완료");
      } catch (err) {
        if (statusEl) statusEl.textContent = String(err?.message || "연결 확인 실패");
      }
    }

    async function commitDialog() {
      const statusEl = byId("copilot-model-status");
      const draft = readDraftProvider();

      if (draft.prov.isLocal) {
        if (!draft.baseUrl || !isValidLocalEndpoint(draft.baseUrl)) {
          if (statusEl) statusEl.textContent = "로컬 서버는 http://127.0.0.1:<포트> 또는 http://[::1]:<포트> 주소만 허용됩니다.";
          return;
        }
      } else if (!draft.baseUrl || !draft.baseUrl.startsWith("https://")) {
        if (statusEl) statusEl.textContent = "원격 제공자는 https:// 엔드포인트만 허용됩니다.";
        return;
      }
      if (draft.prov.requiresKey && !draft.apiKey) {
        if (statusEl) statusEl.textContent = `${draft.prov.label} API 키를 입력해 주세요.`;
        return;
      }

      let chosenModel = draft.model;
      let discovered = catalog.discoveredModelsFor(draft.kind, discoveredByProvider);
      if (discovered.length === 0 || draft.typedKey) {
        if (statusEl && !chosenModel) statusEl.textContent = "제공자 API에서 사용 가능한 모델을 불러오는 중…";
        try {
          const liveList = await discoverModelsLive(draft.kind, draft.baseUrl, draft.apiKey, Boolean(chosenModel));
          if (liveList.length > 0) discovered = liveList;
        } catch (err) {
          if (!chosenModel) {
            if (statusEl) {
              statusEl.textContent = `${String(err?.message || "모델 목록 조회 실패")} (모델 ID를 직접 입력하거나 API 키를 확인해 주세요.)`;
            }
            return;
          }
        }
      }

      if (!chosenModel && discovered.length > 0) {
        chosenModel = discovered[0].id;
      }
      if (!chosenModel) {
        if (statusEl) statusEl.textContent = "사용 가능한 모델을 조회할 수 없습니다. 모델 ID를 직접 입력해 주세요.";
        return;
      }

      providerKind = draft.kind;
      providerBaseUrl = draft.baseUrl;
      providerModel = chosenModel;
      reasoningEffort = draft.reasoningEffort;

      urlsByProvider.set(draft.kind, providerBaseUrl);
      modelsByProvider.set(draft.kind, providerModel);
      if (draft.typedKey) {
        keysByProvider.set(draft.kind, draft.typedKey);
        keyUrlsByProvider.set(draft.kind, draft.baseUrl);
      } else if (!draft.apiKey) {
        // The address changed and no new key came with it: the saved key is dropped, never sent to the new address.
        keysByProvider.delete(draft.kind);
        keyUrlsByProvider.delete(draft.kind);
      }

      closeDialog();
      syncModelButton();
      onChanged?.();
    }

    function bindMenuKeyboard() {
      const menu = byId("copilot-model-menu");
      const groupEl = byId("copilot-model-menu-group");
      const trigger = byId("btn-copilot-model");
      if (!menu || !groupEl || !trigger) return;

      trigger.addEventListener("keydown", (ev) => {
        if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
          ev.preventDefault?.();
          if (!isMenuOpen) toggleMenu(true);
          else groupEl.querySelector("button:not([disabled])")?.focus?.();
        } else if (ev.key === "Escape" && isMenuOpen) {
          ev.preventDefault?.();
          closeMenu(true);
        }
      });

      groupEl.addEventListener("keydown", (ev) => {
        if (ev.key === "Escape") {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          closeMenu(true);
          return;
        }
        if (ev.key === "Tab") {
          closeMenu(false);
          return;
        }
        const hasNativeFocusGroup = Boolean(
          host.HTMLElement?.prototype && "focusGroup" in host.HTMLElement.prototype
        );
        if (hasNativeFocusGroup) return;
        if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(ev.key)) return;
        const items = Array.from(groupEl.querySelectorAll("button")).filter((b) => !b.disabled);
        if (items.length === 0) return;
        const cur = items.indexOf(document.activeElement);
        let next = 0;
        if (ev.key === "Home") next = 0;
        else if (ev.key === "End") next = items.length - 1;
        else if (cur < 0) next = ev.key === "ArrowUp" ? items.length - 1 : 0;
        else next = (cur + (ev.key === "ArrowUp" ? -1 : 1) + items.length) % items.length;
        ev.preventDefault?.();
        items[next]?.focus?.();
      });

      document.addEventListener("mousedown", (ev) => {
        if (!isMenuOpen) return;
        if (menu.contains?.(ev.target) || trigger.contains?.(ev.target)) return;
        closeMenu(false);
      });
    }

    function bindDialogLightDismiss(dlg) {
      if (!dlg) return;
      const hasNativeClosedBy = Boolean(
        host.HTMLDialogElement?.prototype && "closedBy" in host.HTMLDialogElement.prototype
      );
      if (hasNativeClosedBy) return;
      dlg.addEventListener("click", (ev) => {
        if (ev.target !== dlg) return;
        if (
          typeof dlg.getBoundingClientRect === "function" &&
          Number.isFinite(ev.clientX) &&
          Number.isFinite(ev.clientY)
        ) {
          const rect = dlg.getBoundingClientRect();
          if (rect.width > 0 && rect.height > 0) {
            const inside = rect.top <= ev.clientY
              && ev.clientY <= rect.top + rect.height
              && rect.left <= ev.clientX
              && ev.clientX <= rect.left + rect.width;
            if (inside) return;
          }
        }
        closeDialog();
      });
    }

    function bind() {
      byId("btn-copilot-settings")?.addEventListener("click", () => openDialog());
      byId("btn-copilot-model")?.addEventListener("click", (ev) => {
        toggleMenu(ev?.detail === 0);
      });

      const dlg = byId("copilot-model-dialog");
      byId("copilot-provider-kind")?.addEventListener("change", () => syncDialogInputs(true));
      byId("copilot-provider-preset")?.addEventListener("change", (ev) => {
        const kind = byId("copilot-provider-kind")?.value || "openai";
        const val = ev?.target?.value || byId("copilot-provider-preset")?.value || "";
        const modelInput = byId("copilot-provider-model");
        if (!modelInput) return;
        if (val && val !== CUSTOM_MODEL_SENTINEL) {
          modelInput.value = val;
          syncDialogEffortState(kind, val);
        } else if (val === CUSTOM_MODEL_SENTINEL) {
          modelInput.focus?.();
        }
      });
      byId("copilot-provider-model")?.addEventListener("input", () => {
        const kind = byId("copilot-provider-kind")?.value || "openai";
        const presetSelect = byId("copilot-provider-preset");
        const modelVal = String(byId("copilot-provider-model")?.value || "").trim();
        syncDialogEffortState(kind, modelVal);
        if (!presetSelect) return;
        const list = catalog.discoveredModelsFor(kind, discoveredByProvider);
        const nextPreset = list.some((m) => m.id === modelVal) ? modelVal : CUSTOM_MODEL_SENTINEL;
        if (presetSelect.value !== nextPreset) {
          presetSelect.value = nextPreset;
          dispatchChange(presetSelect);
        }
      });
      byId("btn-copilot-fetch-models")?.addEventListener("click", () => void fetchProviderModels());
      byId("btn-copilot-test-provider")?.addEventListener("click", () => void testProviderConnection());
      byId("btn-copilot-model-ok")?.addEventListener("click", () => void commitDialog());
      dlg?.querySelectorAll("[data-dialog-close]").forEach((btn) => {
        btn.addEventListener("click", () => closeDialog());
      });
      dlg?.addEventListener("close", () => clearKeyField());
      bindDialogLightDismiss(dlg);
      bindMenuKeyboard();
    }

    return {
      bind,
      isConfigured,
      openDialog,
      syncModelButton,
      currentProviderPayload,
      setFastModel,
      getFastModel,
      getFastProviderPayload,
      getContextWindow: () => catalog.getModelContextWindow(providerKind, providerModel, discoveredByProvider),
      closeMenu,
      repositionMenu: () => {
        if (isMenuOpen) positionMenu();
      },
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_MODEL_UI__ = createCopilotModelUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
