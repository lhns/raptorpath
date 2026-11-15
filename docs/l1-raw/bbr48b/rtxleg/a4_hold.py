# POST HOC. [HOLD] (sender, end of run) per ORIGINAL leg: closed_retx (cx) and heal_retx_young (hy) counts and quantiles,
# timed from the first report; frontier-pruned holes are not fed. Plus fp1-ro1 (measured re-fire differential, see report).
import os, re, sys, statistics as st
D = sys.argv[1]
for cell in ("c8-100", "c1d-400"):
    acc = {}
    for f in sorted(os.listdir(D)):
        if not (f.startswith(cell + "@") and f.endswith("-c.log")): continue
        arm = re.search(r"def-(\w+)-s", f).group(1)
        L = open(os.path.join(D, f), errors="replace").read().splitlines()
        h = {}
        for l in L:
            if l.startswith("[HOLD]") and "path=-" not in l:
                p = int(re.search(r"path=(\d+)", l).group(1))
                g = lambda k: (lambda m: int(m.group(1)) if m else float("nan"))(re.search(" " + k + r"=(\d+)", l))
                h[p] = {k: g(k) for k in ("cx_n", "cx_p50_us", "cx_p90_us", "hy_n", "hy_p50_us", "hn_n", "fed", "sp_n", "xp_n")}
        dl = [l for l in L if l.startswith("[DIAG]")][-1]
        m = re.search(r"fp/on p0:(\d+)/(\d+) p1:(\d+)/(\d+)", dl); cv = [l for l in L if l.startswith("[C8CONV-S]")][-1]
        ro = {int(p): int(x) for p, x in re.findall(r"p(\d+):sp=\d+ ro=(\d+)", cv)}
        h["d"] = int(m.group(3)) - ro.get(1, 0)
        acc.setdefault(arm, []).append(h)
    for arm, v in sorted(acc.items()):
        print(f"== {cell} {arm} (n={len(v)}; medians over rows of the per-row quantiles)")
        for p in (0, 1):
            md = lambda k: st.median(x[p][k] for x in v)
            print(f"  orig leg{p}: cx_n={md('cx_n'):.0f} cx_p50={md('cx_p50_us')/1e3:.0f}ms cx_p90={md('cx_p90_us')/1e3:.0f}ms | hy_n={md('hy_n'):.0f} hy_p50={md('hy_p50_us')/1e3:.0f}ms | hn_n={md('hn_n'):.0f} fed={md('fed'):.0f} report-arrival same/cross={md('sp_n'):.0f}/{md('xp_n'):.0f}")
        print(f"  per-row cx p90 orig0/orig1 (ms): " + " ".join(f"{x[0]['cx_p90_us']/1e3:.0f}/{x[1]['cx_p90_us']/1e3:.0f}" for x in v))
        print(f"  per-row cx p50 orig0/orig1 (ms): " + " ".join(f"{x[0]['cx_p50_us']/1e3:.0f}/{x[1]['cx_p50_us']/1e3:.0f}" for x in v))
        print(f"  fp1-ro1 per row: mean {st.mean(x['d'] for x in v):.0f}, " + " ".join(str(x['d']) for x in v))
