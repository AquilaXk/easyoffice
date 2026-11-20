// A virtual clock for the copilot animation tests: timers and animation frames run only when `advance` moves time,
// so a typing run or a pointer glide is exact and takes no real time.

export function createFakeClock() {
  let nowMs = 0;
  let nextId = 1;
  let seq = 0;
  const timers = new Map();

  const schedule = (fn, delay) => {
    const id = nextId;
    nextId += 1;
    seq += 1;
    timers.set(id, { fn, dueAt: nowMs + Math.max(0, Number(delay) || 0), seq });
    return id;
  };

  const clock = {
    now: () => nowMs,
    setTimeout: (fn, delay) => schedule(fn, delay),
    clearTimeout: (id) => timers.delete(id),
    requestAnimationFrame: (fn) => schedule(() => fn(nowMs), 16),
    cancelAnimationFrame: (id) => timers.delete(id),
    get pending() {
      return timers.size;
    },
    /** Runs every timer due within `ms`, in order, and leaves the clock at the end of the span. */
    advance(ms) {
      const target = nowMs + ms;
      for (;;) {
        let best = null;
        for (const [id, entry] of timers) {
          if (entry.dueAt > target) continue;
          if (!best || entry.dueAt < best.entry.dueAt || (entry.dueAt === best.entry.dueAt && entry.seq < best.entry.seq)) best = { id, entry };
        }
        if (!best) break;
        timers.delete(best.id);
        nowMs = Math.max(nowMs, best.entry.dueAt);
        best.entry.fn();
      }
      nowMs = target;
    },
    /** Runs timers until none are left or `limitMs` of virtual time has passed. */
    drain(limitMs = 120000) {
      const end = nowMs + limitMs;
      while (timers.size > 0 && nowMs < end) clock.advance(16);
    },
  };
  return clock;
}

export async function settle(turns = 6) {
  for (let i = 0; i < turns; i += 1) await Promise.resolve();
}
