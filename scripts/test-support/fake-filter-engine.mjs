// Stand-in for the engine's `filter` session action (PD-198, PD-226): values lists, `greaterThan`, `lessThan` and `*x*`
// conditions, shared by the filter and the table filter tests.

function judge(criteria, text) {
  if (criteria.values) return text === "" ? criteria.blank === true : criteria.values.includes(text);
  const one = (c) => {
    if (c.op === "greaterThan") return text !== "" && Number(text) > Number(c.text);
    if (c.op === "lessThan") return text !== "" && Number(text) < Number(c.text);
    return new RegExp("^" + c.text.replaceAll("*", ".*") + "$", "i").test(text);
  };
  const [a, b] = criteria.conditions;
  return b ? (criteria.and ? one(a) && one(b) : one(a) || one(b)) : one(a);
}

export function fakeFilterEngine() {
  return (body) => {
    const rows = body.data[0]?.values.length ?? 0;
    const column = (col) => body.data.find((d) => d.col === col).values;
    const hiddenBy = (skip) => {
      const hidden = new Set();
      for (const c of body.filter.columns) {
        if (c.col === skip) continue;
        column(c.col).forEach((text, i) => {
          if (!judge(c, text)) hidden.add(i);
        });
      }
      return hidden;
    };
    if (body.op === "apply") {
      const hidden = hiddenBy(-1);
      const base = Number(/^[A-Z]+(\d+)/.exec(body.filter.range)[1]) + 1;
      const runs = [];
      for (let i = 0; i < rows; i += 1) {
        if (!hidden.has(i)) continue;
        const last = runs[runs.length - 1];
        if (last && last[1] === base + i - 1) last[1] = base + i;
        else runs.push([base + i, base + i]);
      }
      return { hidden: runs, count: hidden.size };
    }
    const excluded = hiddenBy(body.col);
    const counts = new Map();
    let blank = 0;
    column(body.col).forEach((text, i) => {
      if (excluded.has(i)) return;
      if (text === "") blank += 1;
      else if (!body.query || text.toLowerCase().includes(body.query.toLowerCase())) counts.set(text, (counts.get(text) || 0) + 1);
    });
    const items = [...counts].sort((a, b) => a[0].localeCompare(b[0], "ko"));
    return { items, blank: body.query ? 0 : blank, more: false };
  };
}
