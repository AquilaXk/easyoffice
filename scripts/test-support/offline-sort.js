(function (host) {
  "use strict";

  // Stand-in for the engine's `sort` session action (PD-202) in the page tests: stable, numbers
  // before text, blanks last, `a` before `A` when asked, and the formulas of moved rows rewritten
  // for the simple `A2` references the tests use (a `$` column is kept; absolute rows are not modeled).
  function offlineSort(body) {
  const n = body.keys[0].values.length;
  const order = Array.from({ length: n }, (_, i) => i);
  const cmpValue = (a, b, asc) => {
    if (a === null || b === null) return a === null ? (b === null ? 0 : 1) : -1;
    const kind = (v) => (typeof v === "number" ? 0 : 1);
    let c;
    if (kind(a) !== kind(b)) c = kind(a) - kind(b);
    else if (kind(a) === 0) c = a - b;
    else {
      const fold = (s) => s.toLowerCase().replaceAll("-", "").replaceAll("'", "");
      c = fold(a) < fold(b) ? -1 : fold(a) > fold(b) ? 1 : 0;
      if (c === 0 && body.caseSensitive) c = a === b ? 0 : a[0] === a[0].toLowerCase() ? -1 : 1;
    }
    return asc ? c : -c;
  };
  order.sort((x, y) => {
    for (const key of body.keys) {
      const c = cmpValue(key.values[x], key.values[y], key.asc);
      if (c !== 0) return c;
    }
    return x - y;
  });
  const place = [];
  order.forEach((u, p) => (place[u] = p));
  // PD-181: a relative reference into the sorted rows moves as far as its formula's row.
  const formulas = (body.formulas || []).map(([u, text]) => {
    const delta = body.rows[place[u]] - body.rows[u];
    return text.replace(/(\$?[A-Z]+)(\d+)/g, (all, col, row) =>
      Number(row) >= body.range.r1 && Number(row) <= body.range.r2 ? col + (Number(row) + delta) : all
    );
  });
  return { order, formulas };
}

  host.__EASYOFFICE_OFFLINE_SORT__ = offlineSort;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
