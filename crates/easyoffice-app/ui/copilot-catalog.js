// 참조 구현: continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., `gui/src/pages/AddNewModel/configs/fetchProviderModels.ts`) 및 dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., `web/src/features/copilot/model-picker.tsx`, `server/src/modules/providers/models.ts`, see THIRD_PARTY_NOTICES.md)

(function (host) {
  "use strict";

  const PROVIDERS = Object.freeze([
    Object.freeze({
      id: "openai",
      label: "OpenAI",
      shortLabel: "OpenAI",
      defaultBaseUrl: "https://api.openai.com/v1",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "anthropic",
      label: "Anthropic",
      shortLabel: "Anthropic",
      defaultBaseUrl: "https://api.anthropic.com/v1",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "google",
      label: "Google",
      shortLabel: "Google",
      defaultBaseUrl: "https://generativelanguage.googleapis.com/v1beta",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "deepseek",
      label: "DeepSeek",
      shortLabel: "DeepSeek",
      defaultBaseUrl: "https://api.deepseek.com",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "openrouter",
      label: "OpenRouter",
      shortLabel: "OpenRouter",
      defaultBaseUrl: "https://openrouter.ai/api/v1",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "mistral",
      label: "Mistral",
      shortLabel: "Mistral",
      defaultBaseUrl: "https://api.mistral.ai/v1",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "groq",
      label: "Groq",
      shortLabel: "Groq",
      defaultBaseUrl: "https://api.groq.com/openai/v1",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "xai",
      label: "xAI",
      shortLabel: "xAI",
      defaultBaseUrl: "https://api.x.ai/v1",
      requiresKey: true,
      isLocal: false,
    }),
    Object.freeze({
      id: "ollama",
      label: "Ollama (로컬)",
      shortLabel: "Ollama",
      defaultBaseUrl: "http://127.0.0.1:11434/v1",
      requiresKey: false,
      isLocal: true,
    }),
    Object.freeze({
      id: "lmstudio",
      label: "LM Studio (로컬)",
      shortLabel: "LM Studio",
      defaultBaseUrl: "http://127.0.0.1:1234/v1",
      requiresKey: false,
      isLocal: true,
    }),
    Object.freeze({
      id: "remote_https",
      label: "OpenAI 호환 HTTPS API",
      shortLabel: "호환 HTTPS",
      defaultBaseUrl: "",
      requiresKey: false,
      isLocal: false,
    }),
    Object.freeze({
      id: "local_http",
      label: "사용자 지정 로컬 서버 (127.0.0.1)",
      shortLabel: "로컬 서버",
      defaultBaseUrl: "",
      requiresKey: false,
      isLocal: true,
    }),
  ]);

  const PROVIDER_MAP = new Map(PROVIDERS.map((p) => [p.id, p]));
  PROVIDER_MAP.set("custom", PROVIDER_MAP.get("remote_https"));

  const QUICK_PROVIDER_ITEMS = Object.freeze([
    Object.freeze({ kind: "openai", title: "OpenAI", subtitle: "api.openai.com" }),
    Object.freeze({ kind: "anthropic", title: "Anthropic", subtitle: "api.anthropic.com" }),
    Object.freeze({ kind: "google", title: "Google", subtitle: "generativelanguage.googleapis.com" }),
    Object.freeze({ kind: "deepseek", title: "DeepSeek", subtitle: "api.deepseek.com" }),
    Object.freeze({ kind: "openrouter", title: "OpenRouter", subtitle: "openrouter.ai" }),
    Object.freeze({ kind: "ollama", title: "Ollama (로컬)", subtitle: "127.0.0.1:11434" }),
    Object.freeze({ kind: "lmstudio", title: "LM Studio (로컬)", subtitle: "127.0.0.1:1234" }),
  ]);

  const DEFAULT_REASONING_EFFORT = "medium";

  const REASONING_EFFORTS = Object.freeze([
    Object.freeze({ value: "low", label: "낮음" }),
    Object.freeze({ value: "medium", label: "보통" }),
    Object.freeze({ value: "high", label: "높음" }),
  ]);

  const EFFORT_LABELS = Object.freeze({
    low: "낮음",
    medium: "보통",
    high: "높음",
  });

  function getProvider(kind) {
    return PROVIDER_MAP.get(String(kind || "")) || null;
  }

  function formatContextWindow(tokens) {
    const n = Number(tokens) || 0;
    if (n <= 0) return "";
    if (n >= 1000000) return Math.round(n / (n % 1048576 === 0 ? 1048576 : 1000000)) + "M";
    if (n % 1000 === 0) return Math.round(n / 1000) + "K";
    return Math.round(n / 1024) + "K";
  }

  function discoveredModelsFor(kind, discoveredMap) {
    const prov = getProvider(kind);
    if (!prov) return [];
    const live = discoveredMap?.get(prov.id);
    return Array.isArray(live) ? live : [];
  }

  function findModelInfo(kind, modelId, discoveredMap) {
    const id = String(modelId || "").trim();
    if (!kind || !id) return null;
    const list = discoveredModelsFor(kind, discoveredMap);
    return list.find((m) => m.id === id) || null;
  }

  function inferReasoningSupport(modelId) {
    const lower = String(modelId || "").trim().toLowerCase();
    if (!lower) return false;
    if (lower.includes("flash-lite") && lower.includes("gemini-2.5")) return false;
    return (
      lower.includes("reason") ||
      lower.includes("thinking") ||
      lower.includes("r1") ||
      lower.includes("qwq") ||
      lower.startsWith("o1") ||
      lower.startsWith("o3") ||
      lower.startsWith("o4") ||
      lower.includes("/o1") ||
      lower.includes("/o3") ||
      lower.includes("/o4") ||
      lower.includes("gpt-5") ||
      lower.includes("gpt-6") ||
      lower.includes("claude-3-7") ||
      lower.includes("claude-sonnet-4") ||
      lower.includes("claude-opus-4") ||
      lower.includes("claude-haiku-4") ||
      lower.includes("claude-4") ||
      lower.includes("claude-5") ||
      lower.includes("gemini-2.5") ||
      lower.includes("gemini-3") ||
      lower.includes("gemini-4") ||
      lower.includes("magistral") ||
      lower.includes("grok-3-mini") ||
      lower.includes("grok-4") ||
      lower.includes("qwen3")
    );
  }

  function supportsReasoning(kind, modelId, discoveredMap) {
    if (!kind || !modelId) return false;
    const info = findModelInfo(kind, modelId, discoveredMap);
    if (info && typeof info.supportsReasoning === "boolean") {
      return info.supportsReasoning;
    }
    return inferReasoningSupport(modelId);
  }

  function formatButtonLabel(kind, modelId, effort, discoveredMap) {
    if (!kind || !modelId) return "모델 선택";
    const info = findModelInfo(kind, modelId, discoveredMap);
    const base = info ? info.name : modelId;
    if (supportsReasoning(kind, modelId, discoveredMap) && effort && EFFORT_LABELS[effort]) {
      return base + " · " + EFFORT_LABELS[effort];
    }
    return base;
  }

  const LIGHT_MODEL_RE = /(-mini\b|-nano\b|flash-lite|\bflash\b|-haiku\b|haiku|deepseek-chat|-small\b|-instant\b|ministral)/i;

  function pickDiscoveredCandidate(candidates, patterns) {
    for (const pat of patterns) {
      const hit = candidates.find((m) => String(m.id || "").toLowerCase().includes(pat));
      if (hit) return hit.id;
    }
    return null;
  }

  function findFastSiblingModel(kind, modelId, discoveredMap) {
    const id = String(modelId || "").trim();
    if (!kind || !id || LIGHT_MODEL_RE.test(id)) return null;
    const list = discoveredModelsFor(kind, discoveredMap).filter((m) => m?.id && m.id !== id);
    if (list.length === 0) return null;
    const lower = id.toLowerCase();
    const slash = lower.indexOf("/");
    const vendorPrefix = kind === "openrouter" && slash > 0 ? lower.slice(0, slash + 1) : "";
    const pool = vendorPrefix
      ? list.filter((m) => String(m.id || "").toLowerCase().startsWith(vendorPrefix))
      : list;
    const scoped = pool.length > 0 ? pool : list;
    const effKind = vendorPrefix.startsWith("anthropic/") ? "anthropic"
      : vendorPrefix.startsWith("openai/") ? "openai"
      : vendorPrefix.startsWith("google/") ? "google"
      : vendorPrefix.startsWith("deepseek/") ? "deepseek"
      : vendorPrefix.startsWith("x-ai/") ? "xai"
      : kind;
    const byVendor = {
      openai: ["gpt-5-mini", "gpt-5-nano", "gpt-4.1-mini", "gpt-4.1-nano", "gpt-4o-mini", "o4-mini", "o3-mini", "-mini", "-nano"],
      anthropic: ["claude-haiku-4-5", "claude-3-5-haiku", "claude-3-haiku", "haiku"],
      google: ["gemini-2.5-flash-lite", "gemini-3-flash", "gemini-2.5-flash", "gemini-2.0-flash", "flash-lite", "flash"],
      deepseek: ["deepseek-chat"],
      xai: ["grok-3-mini", "grok-3-fast", "-mini", "-fast"],
      mistral: ["mistral-small-latest", "mistral-small", "ministral", "-small"],
    };
    const exact = byVendor[effKind] ? pickDiscoveredCandidate(scoped, byVendor[effKind]) : null;
    if (exact) return exact;
    const generic = scoped.find((m) => LIGHT_MODEL_RE.test(String(m.id || "")));
    return generic ? generic.id : null;
  }

  function getModelContextWindow(kind, modelId, discoveredMap) {
    const info = findModelInfo(kind, modelId, discoveredMap);
    if (info && Number(info.contextWindow) > 0) return Number(info.contextWindow);
    const lower = String(modelId || "").trim().toLowerCase();
    if (!lower) return 0;
    if (lower.includes("gemini")) return 1000000;
    if (lower.includes("claude")) return 200000;
    if (lower.includes("gpt-5") || lower.includes("o3") || lower.includes("o4")) return 200000;
    if (lower.includes("gpt-4o") || lower.includes("gpt-4.1") || lower.includes("grok")) return 128000;
    if (lower.includes("deepseek")) return 64000;
    return 0;
  }

  host.__EASYOFFICE_COPILOT_CATALOG__ = Object.freeze({
    PROVIDERS,
    QUICK_PROVIDER_ITEMS,
    DEFAULT_REASONING_EFFORT,
    REASONING_EFFORTS,
    EFFORT_LABELS,
    getProvider,
    formatContextWindow,
    discoveredModelsFor,
    findModelInfo,
    findFastSiblingModel,
    supportsReasoning,
    formatButtonLabel,
    getModelContextWindow,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
