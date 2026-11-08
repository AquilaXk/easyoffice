// EasyOffice fork of rhwp-studio/src/embed/protocol.ts: the plugin-loader and hwpctrl capabilities
// are not advertised (the plugin loader is not bundled); blank-document-v1, print-pages-v1, save-protection-v1 and host-fonts-v1 are added.
export const EMBED_PROTOCOL_VERSION = 1 as const;
export const EMBED_CAPABILITIES = [
  'transferable-array-buffer',
  'hml-export',
  'renderer-diagnostics-v1',
  'font-decision-trace-v1',
  'notify-saved-v1',
  // 아래 넷은 브리지 확장(P4). 프로토콜 세대는 1 을 유지하고 capability 로만 넓힌다 —
  // 구버전 studio 에 붙은 신버전 SDK 는 기능만 비활성되고 기존 임베드는 그대로 돈다.
  'automation-v1',
  'chrome-v1',
  'document-state-v1',
  'selection-context-v1',
  'document-agent-command-v1',
  'target-navigation-v1',
  'document-change-events-v1',
  // EasyOffice extension: createBlankDocument RPC.
  'blank-document-v1',
  // EasyOffice extension: verifyBytes RPC.
  'verify-bytes-v1',
  // EasyOffice extension: setShellContext RPC (shell theme and recent documents for the 파일 menu).
  'shell-context-v1',
  // EasyOffice extension: getPrintPages RPC (the shell prints the document from batches of page SVGs).
  'print-pages-v1',
  // EasyOffice extension: getSaveProtection and clearSaveProtection RPCs (a document opened with a
  // password is saved without it; the shell asks before it writes such a copy, PD-120).
  'save-protection-v1',
  // EasyOffice extension: enableHostFonts and hostFontsReply RPCs (the machine's fonts reach the editor through the
  // shell, which alone may read them; the editor asks over the port, PD-127).
  'host-fonts-v1',
] as const;

export interface EmbedConnectAttempt {
  type: 'rhwp-connect';
  version: number;
  sessionId: string;
  capabilities?: unknown;
}

export interface EmbedConnectMessage {
  type: 'rhwp-connect';
  version: typeof EMBED_PROTOCOL_VERSION;
  sessionId: string;
  capabilities: readonly string[];
}

export interface EmbedRequestEnvelope {
  type: 'rhwp-request';
  version: typeof EMBED_PROTOCOL_VERSION;
  sessionId: string;
  id: number;
  method: string;
  params?: unknown;
}

export interface EmbedRequestAttempt {
  type: 'rhwp-request';
  version?: unknown;
  sessionId: string;
  id: number;
  method?: unknown;
  params?: unknown;
}

export interface EmbedResponseEnvelope {
  type: 'rhwp-response';
  version: typeof EMBED_PROTOCOL_VERSION;
  sessionId: string;
  id: number;
  result?: unknown;
  error?: EmbedProtocolError;
}

export interface EmbedProtocolError {
  code: string;
  message: string;
  supportedVersions?: number[];
  recovered?: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function isConnectMessage(value: unknown): value is EmbedConnectMessage {
  return isConnectAttempt(value)
    && value.version === EMBED_PROTOCOL_VERSION
    && Array.isArray(value.capabilities)
    && value.capabilities.includes('transferable-array-buffer');
}

export function isConnectAttempt(value: unknown): value is EmbedConnectAttempt {
  return isRecord(value)
    && value.type === 'rhwp-connect'
    && Number.isSafeInteger(value.version)
    && typeof value.sessionId === 'string'
    && value.sessionId.length > 0;
}

export function isRequestEnvelope(
  value: unknown,
  sessionId: string,
): value is EmbedRequestEnvelope {
  return isRequestAttempt(value, sessionId)
    && value.version === EMBED_PROTOCOL_VERSION
    && typeof value.method === 'string'
    && value.method.length > 0;
}

export function isRequestAttempt(
  value: unknown,
  sessionId: string,
): value is EmbedRequestAttempt {
  return isRecord(value)
    && value.type === 'rhwp-request'
    && value.sessionId === sessionId
    && Number.isSafeInteger(value.id);
}

// The desktop shell serves its pages from `http://tauri.localhost` on Windows (WebView2) and from the
// `tauri://localhost` custom scheme on macOS (WKWebView) and Linux (WebKitGTK), so `tauri:` is a usable
// parent origin as well (PD-094).
const USABLE_PARENT_PROTOCOLS: ReadonlySet<string> = new Set(['http:', 'https:', 'tauri:']);

export function isUsableParentOrigin(origin: string): boolean {
  if (!origin || origin === 'null') return false;
  try {
    return USABLE_PARENT_PROTOCOLS.has(new URL(origin).protocol);
  } catch {
    return false;
  }
}
