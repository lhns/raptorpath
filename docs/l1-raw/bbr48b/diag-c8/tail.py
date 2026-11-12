# POST HOC, not registered. Tail phase: last [DIAG] line with src>0 (sender still emitting new source) vs run end; per arm/cell.
import os, re, sys, statistics as st
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
res = {}
for f in sorted(os.listdir(L)):
    if not f.endswith("-c.log"): continue
    cell, arm = re.match(r"(c\w+-\d+)@def-(\w+)-", f).groups()
    rows = []
    for ln in open(os.path.join(L, f), errors="replace"):
        m = re.match(r"\[DIAG\] t=([0-9.]+)s .*?good=([0-9.]+)Mbit.*? src=(\d+)sym/s", ln)
        if m:
            B = sum(int(b) - int(c) for _, b, c in re.findall(r"dgq(\d+)\[hand=(\d+) tx=(\d+)", ln))
            rows.append((float(m.group(1)), float(m.group(2)), int(m.group(3)), B))
    drv = open(os.path.join(L, f.replace("-c.log", "-drv.out")), errors="replace").read()
    secs = float(re.search(r'"median_s":([0-9.]+)', drv).group(1))
    live = [r for r in rows if r[0] >= 1.0 and r[0] <= secs]
    t_src_end = max(r[0] for r in live if r[2] > 0)
    tail = [r for r in live if r[0] > t_src_end]
    B_at_end = [r[3] for r in live if abs(r[0] - t_src_end) < 1e-6][0]
    res.setdefault((cell, arm), []).append((secs, t_src_end, secs - t_src_end, B_at_end, st.mean(r[1] for r in tail) if tail else float("nan")))
out = ["POST HOC / NOT REGISTERED. Tail = run seconds - t of the last [DIAG] line with src>0 (all new source emitted); medians over 12 rows",
       f"{'cell':8s} {'arm':5s} {'seconds':>8s} {'t_src_end':>9s} {'tail_s':>7s} {'B(dgq hand-tx, sum legs) at src end':>36s} {'mean good in tail':>18s}"]
for (cell, arm), v in sorted(res.items()):
    med = lambda i: st.median(x[i] for x in v)
    out.append(f"{cell:8s} {arm:5s} {med(0):8.2f} {med(1):9.2f} {med(2):7.2f} {med(3):36.0f} {med(4):18.1f}")
for cell in ("c1s-400", "c1d-400", "c2-100", "c8-100"):
    b, a = res[(cell, "BASE")], res[(cell, "CAND")]
    out.append(f"  {cell}: d_seconds {st.median(x[0] for x in a) - st.median(x[0] for x in b):+.2f}  d_t_src_end {st.median(x[1] for x in a) - st.median(x[1] for x in b):+.2f}  d_tail {st.median(x[2] for x in a) - st.median(x[2] for x in b):+.2f}")
print("\n".join(out)); open(os.path.join(D, "diag-c8/tail_out.txt"), "w").write("\n".join(out) + "\n")
