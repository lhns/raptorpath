# POST HOC, not registered. Does the c8 tail carry CPU? (a) within-arm seed-centred slope of CPU on tail/main length;
# (b) client sender-task busy (per-[DIAG] busy_us) split into main vs tail; (c) tail-phase activity per arm.
import json, os, re, sys, statistics as st
import numpy as np
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
F = {(r["cell"], r["arm"], r["seed"], r["rep"]): r for r in json.load(open(os.path.join(D, "diag-c8/feat2.json")))}
rows = []
for f in sorted(os.listdir(L)):
    if not (f.startswith("c8-100@") and f.endswith("-c.log")): continue
    arm, seed, rep = re.search(r"def-(\w+)-s(\d+)-r(\d+)-", f).groups()
    secs = float(re.search(r'"median_s":([0-9.]+)', open(os.path.join(L, f.replace("-c.log", "-drv.out")), errors="replace").read()).group(1))
    dl = []
    for ln in open(os.path.join(L, f), errors="replace"):
        m = re.match(r"\[DIAG\] t=([0-9.]+)s .*? src=(\d+)sym/s cod=(\d+)sym/s", ln)
        if m:
            b = re.search(r"wait\[.* us=(\d+) busy=\d+% busy_us=(\d+)\]", ln)
            tx = sum(int(c) for _, c in re.findall(r"dgq\d+\[hand=\d+ tx=(\d+)", ln).__iter__() for c in [c]) if False else sum(int(x) for x in re.findall(r"dgq\d+\[hand=\d+ tx=(\d+)", ln))
            rtx = re.search(r"\bretx=(\d+)", ln)
            dl.append((float(m.group(1)), int(m.group(2)), int(b.group(2)) if b else 0, tx, int(rtx.group(1)) if rtx else 0))
    tse = max(t for t, s, *_ in dl if 1.0 <= t <= secs and s > 0)
    main_b = sum(x[2] for x in dl if x[0] <= tse) / 1e6; tail_b = sum(x[2] for x in dl if tse < x[0] <= secs + 0.3) / 1e6
    tx_at = lambda t: max([x[3] for x in dl if x[0] <= t] or [0])
    tail_tx = tx_at(secs + 0.3) - tx_at(tse)
    r = F[("c8-100", arm, seed, int(rep))]
    rows.append(dict(arm=arm, seed=seed, secs=secs, main=tse, tail=secs - tse, main_busy=main_b, tail_busy=tail_b, tail_tx=tail_tx,
                     srv=r["srv.thr.cpu_s"], cli=r["cli.thr.cpu_s"]))
out = []
def P(s=""): out.append(s); print(s)
P("POST HOC / NOT REGISTERED. c8 tail vs CPU")
for arm in ("BASE", "CAND"):
    rr = [r for r in rows if r["arm"] == arm]
    m = lambda k: st.mean(r[k] for r in rr)
    P(f"  {arm}: main {m('main'):.2f}s tail {m('tail'):.2f}s | client sender-task busy main {m('main_busy'):.3f}s tail {m('tail_busy'):.3f}s | datagrams sent in tail {m('tail_tx'):.0f} | cpu srv {m('srv'):.2f} cli {m('cli'):.2f}")
def slope(xk, yk):
    X, Y = [], []
    for arm in ("BASE", "CAND"):
        for s in ("42", "7"):
            rr = [r for r in rows if r["arm"] == arm and r["seed"] == s]
            x = np.array([r[xk] for r in rr]); y = np.array([r[yk] for r in rr]); X += list(x - x.mean()); Y += list(y - y.mean())
    X, Y = np.array(X), np.array(Y); return (X @ Y) / (X @ X), np.corrcoef(X, Y)[0, 1]
for yk in ("srv", "cli"):
    for xk in ("tail", "main", "tail_tx"):
        s, c = slope(xk, yk)
        dx = st.mean(r[xk] for r in rows if r["arm"] == "CAND") - st.mean(r[xk] for r in rows if r["arm"] == "BASE")
        dy = st.mean(r[yk] for r in rows if r["arm"] == "CAND") - st.mean(r[yk] for r in rows if r["arm"] == "BASE")
        P(f"  within-arm seed-centred: cpu_{yk} on {xk}: slope {s:+.3f} per unit, r={c:+.2f}; arm dx {dx:+.3f} -> implied {s*dx:+.3f}s of actual {dy:+.3f}s")
open(os.path.join(D, "diag-c8/tailcpu_out.txt"), "w").write("\n".join(out) + "\n")
