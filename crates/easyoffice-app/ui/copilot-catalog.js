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
    supportsReasoning,
    formatButtonLabel,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
