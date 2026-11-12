// EasyOffice extension (host-fonts-v1, PD-128): the fonts installed on the machine reach the editor
// through the host-font contract the editor already has (`HostFontProvider`: a catalog of faces and
// a lazy read of one face). The editor frame is sandboxed with an opaque origin and cannot call the
// shell's commands (PD-086/088), so the provider asks the shell over the embed port and the shell,
// which alone may call them, answers with a `hostFontsReply` request:
//
//   editor -> shell   event   { reason: 'shell_fonts', requestId, op: 'list' }
//                             { reason: 'shell_fonts', requestId, op: 'read', face, revision }
//   shell  -> editor  request hostFontsReply { requestId, list } | { requestId, data } | { requestId, error }
//
// A face is named by an opaque id from the shell's list; the editor never learns a path, and the
// shell re-checks every id and revision it is asked about. Font bytes arrive as a transferred
// ArrayBuffer, once per read.

import type { HostFontData, HostFontProvider, HostFontSnapshot } from '@/core/host-font-provider';

export const SHELL_FONTS_REASON = 'shell_fonts' as const;
/** The first list may scan the font folders; a shell that does not answer in this time failed. */
export const FONT_REQUEST_TIMEOUT_MS = 20_000;
const REPLY_KEYS = ['requestId', 'list', 'data', 'error'] as const;

export type ShellFontsRequest =
  | { schemaVersion: 1; reason: typeof SHELL_FONTS_REASON; requestId: number; op: 'list' }
  | { schemaVersion: 1; reason: typeof SHELL_FONTS_REASON; requestId: number; op: 'read'; face: string; revision: string };

export type ShellFontsSink = (request: ShellFontsRequest) => void;

interface Waiter {
  resolve: (reply: Record<string, unknown>) => void;
  reject: (error: Error) => void;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isSnapshotShape(value: Record<string, unknown>): value is Record<string, unknown> & HostFontSnapshot {
  return typeof value.revision === 'string' && Array.isArray(value.faces);
}

/** The provider the editor installs, the relay that carries its requests to the shell port, and
 *  `reply`, which accepts what the shell answered. */
export function createShellFontBridge(timeoutMs = FONT_REQUEST_TIMEOUT_MS) {
  const sinks = new Set<ShellFontsSink>();
  const waiting = new Map<number, Waiter>();
  let lastId = 0;

  function ask(
    request: (requestId: number) => ShellFontsRequest,
    signal: AbortSignal,
  ): Promise<Record<string, unknown>> {
    if (signal.aborted) return Promise.reject(new Error('Host font request aborted'));
    if (sinks.size === 0) return Promise.reject(new Error('The shell is not listening for font requests'));
    const requestId = ++lastId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => settle(new Error('Host font request timed out')), timeoutMs);
      const onAbort = () => settle(new Error('Host font request aborted'));
      const cleanup = () => {
        clearTimeout(timer);
        signal.removeEventListener('abort', onAbort);
        waiting.delete(requestId);
      };
      const settle = (error: Error) => { cleanup(); reject(error); };
      signal.addEventListener('abort', onAbort, { once: true });
      waiting.set(requestId, { resolve: (reply) => { cleanup(); resolve(reply); }, reject: settle });
      const message = request(requestId);
      for (const sink of sinks) sink(message);
    });
  }

  const provider: HostFontProvider = {
    async getSnapshot(signal): Promise<HostFontSnapshot> {
      const reply = await ask(
        (requestId) => ({ schemaVersion: 1, reason: SHELL_FONTS_REASON, requestId, op: 'list' }),
        signal,
      );
      // `HostFontSource` validates every field of the catalog before it keeps it.
      if (!isRecord(reply.list) || !isSnapshotShape(reply.list)) throw new Error('The shell sent no font list');
      return reply.list;
    },
    async readFace(face, revision, signal): Promise<HostFontData> {
      const reply = await ask(
        (requestId) => ({ schemaVersion: 1, reason: SHELL_FONTS_REASON, requestId, op: 'read', face, revision }),
        signal,
      );
      const data = reply.data;
      if (!isRecord(data) || !(data.bytes instanceof ArrayBuffer)
        || !Number.isSafeInteger(data.faceIndex) || (data.faceIndex as number) < 0) {
        throw new Error('The shell sent no font data');
      }
      return { bytes: data.bytes, faceIndex: data.faceIndex as number };
    },
    // The shell's list is built once per window; a change arrives as a new provider.
    subscribe: () => () => {},
  };

  return {
    provider,
    relay: {
      add(sink: ShellFontsSink): () => void {
        sinks.add(sink);
        return () => { sinks.delete(sink); };
      },
    },
    /** The shell's answer. An unknown or repeated request number is ignored (it timed out or was aborted). */
    reply(params: unknown): { accepted: boolean } {
      if (!isRecord(params)) throw new Error('hostFontsReply params must be an object');
      const unknown = Object.keys(params).filter(key => !(REPLY_KEYS as readonly string[]).includes(key));
      if (unknown.length > 0) throw new Error(`hostFontsReply params contains unknown field: ${unknown[0]}`);
      const requestId = params.requestId;
      if (!Number.isSafeInteger(requestId)) throw new Error('hostFontsReply requestId must be a safe integer');
      const waiter = waiting.get(requestId as number);
      if (!waiter) return { accepted: false };
      if (typeof params.error === 'string') waiter.reject(new Error('The shell refused the font request'));
      else waiter.resolve(params);
      return { accepted: true };
    },
  };
}
