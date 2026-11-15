# §48b POST-HOC supplement (not registered): goodput-weighted classes, BASE comparison, joint counts, c8 split.
import json, os, re, sys, statistics as st
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
LINK = {"c2-100": {0: 12.5e6}, "c8-100": {0: 12.5e6, 1: 2.5e6}}
T = re.compile(r"\] t=([0-9.]+)s")
def lines(path):
    diag, qcc = [], []
    for ln in open(path, encoding="utf-8", errors="replace"):
        m = T.search(ln)
        if not m: continue
        t = float(m.group(1))
        if "[QCC] t=" in ln:
            qcc.append((t, {int(a): b for a, b in re.findall(r"p(\d+):qm=(\S)", ln)}))
        elif "[DIAG] t=" in ln:
            pm = re.search(r"\bpaused=(\d+)%", ln); g = re.search(r"\bgood=([0-9.]+)Mbit", ln)
            dg = {int(a): int(b) - int(c) for a, b, c in re.findall(r"dgq(\d+)\[hand=(\d+) tx=(\d+)", ln)}
            diag.append((t, int(pm.group(1)) if pm else None, float(g.group(1)) if g else None, dg))
    return diag, qcc
for cell in ("c2-100", "c8-100"):
    print(f"== {cell}")
    for arm in ("BASE", "CAND"):
        good_by = {}; joint = 0; n = 0; Bs = []; ps = []
        for f in sorted(os.listdir(L)):
            if not (f.startswith(cell + "@def-" + arm + "-") and f.endswith("-c.log")): continue
            diag, qcc = lines(os.path.join(L, f))
            for t, paused, good, dg in diag:
                if t < 1.0 or good is None: continue
                q = min(qcc, key=lambda x: abs(x[0] - t))[1] if qcc else {}
                qm0 = q.get(0)
                Bmax = max(dg.values()) if dg else None
                n += 1; Bs.append(Bmax); ps.append(paused)
                if paused is not None and paused >= 50 and Bmax is not None and Bmax >= 98: joint += 1
                cls = "R" if "R" in q.values() else ("noqcc" if not q else "nonR")
                good_by.setdefault(cls, []).append(good)
        tot = sum(len(v) for v in good_by.values())
        desc = " ".join(f"{k}: share {len(v)/tot:.3f} mean good {st.mean(v):.1f}" for k, v in sorted(good_by.items()))
        print(f"  {arm}: lines {n}  B(max over legs) p50 {st.median([b for b in Bs if b is not None]):.0f}  paused p50 {st.median([p for p in ps if p is not None]):.0f}%  joint(paused>=50 & B>=98) {joint/n:.3f}  | {desc}")
        if arm == "CAND" and "R" in good_by and "nonR" in good_by:
            gR, gN = st.mean(good_by["R"]), st.mean(good_by["nonR"]); sR = len(good_by["R"]) / tot
            print(f"    ProbeRtt goodput cost estimate: share {sR:.3f} x (1 - {gR:.1f}/{gN:.1f}) = {100*sR*(1-gR/gN):.2f} % of goodput")
rows = []
for ln in open(os.path.join(D, "run/bbr48/ld.log"), encoding="utf-8", errors="replace"):
    if ln.startswith("P38ROW "):
        rows.append(json.loads(ln[7:]))
print("== c8-100 per arm (median over rows): shares per leg, coded share, plc per leg, cpu, seconds")
for arm in ("BASE", "CAND"):
    rr = [r for r in rows if r["cell"] == "c8-100" and r["arm"] == arm]
    med = lambda k: st.median([r[k] for r in rr if r.get(k) is not None])
    s0 = st.median([r["shares"][0] for r in rr]); s1 = st.median([r["shares"][1] for r in rr])
    print(f"  {arm}: share p0 {s0:.3f} p1 {s1:.3f} | coded_share {med('coded_share'):.4f} | plc p0 {med('plc_p0'):.4f} p1 {med('plc_p1'):.4f} | cpu cli {med('cpu_cli'):.2f}s srv {med('cpu_srv'):.2f}s | seconds {med('seconds'):.2f} | mbps {med('mbps'):.2f}")
