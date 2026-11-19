# Per-row I/O stall (PSI io full total delta between consecutive rows, us) against
# the row's goodput residual within its arm. Read-only. Usage: iopsi.py <ld.log>...
import sys, re, json, statistics as st
for path in sys.argv[1:]:
    rows = []  # (arm, block, total_at_row, mbps)
    cur = None
    for ln in open(path, errors="replace"):
        m = re.match(r"IOPRESSURE (\S+)@def-(\w+) s(\d+) rep=(\d+) .*total=(\d+)", ln)
        if m:
            cur = [m.group(2), int(m.group(4)), int(m.group(5)), None]
        if ln.startswith("P38ROW ") and cur:
            r = json.loads(ln[7:])
            cur[3] = r.get("mbps")
            rows.append(cur); cur = None
    stall = []
    for i, r in enumerate(rows):
        nxt = rows[i + 1][2] if i + 1 < len(rows) else None
        stall.append(None if nxt is None else (nxt - r[2]) / 1e6)
    arms = {}
    for r in rows:
        arms.setdefault(r[0], []).append(r[3])
    med = {a: st.median([x for x in v if x]) for a, v in arms.items() if any(v)}
    pairs = [(s, (r[3] - med[r[0]]) / med[r[0]]) for r, s in zip(rows, stall) if s is not None and r[3] and r[0] in med]
    xs = [p[0] for p in pairs]; ys = [p[1] for p in pairs]
    mx, my = st.mean(xs), st.mean(ys)
    cov = sum((x - mx) * (y - my) for x, y in pairs)
    r = cov / ((sum((x - mx) ** 2 for x in xs) * sum((y - my) ** 2 for y in ys)) ** 0.5 or 1)
    print(f"{path}: rows {len(rows)}; io-full stall per row (s): median {st.median(xs):.3f} p90 {sorted(xs)[int(.9*len(xs))]:.3f} max {max(xs):.3f}; corr(stall, goodput residual) {r:+.3f}")
    blk = {}
    for rr, s in zip(rows, stall):
        if s is not None:
            blk.setdefault(rr[1], []).append(s)
    print("  per block stall sum (s):", {b: round(sum(v), 2) for b, v in sorted(blk.items())})
    worst = sorted(((s, rr[0], rr[1], rr[3]) for rr, s in zip(rows, stall) if s is not None), reverse=True)[:3]
    print("  worst rows (stall s, arm, block, mbps vs arm median):", [(round(s, 2), a, b, m, round(med.get(a, 0), 1)) for s, a, b, m in worst])
