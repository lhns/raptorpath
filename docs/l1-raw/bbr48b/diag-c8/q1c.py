# POST HOC, not registered. Q1c: out-of-sample price of an I/O batch. Fit cpu = f*batches + v*datagrams (+ w*wall) on the
# OTHER cells (c1s, c1d, c2; BASE+CAND), predict the c8 CAND-BASE delta from c8's own count deltas.
import json, os, sys, statistics as st
import numpy as np
D = sys.argv[1]; R = json.load(open(os.path.join(D, "diag-c8/feat2.json")))
g = lambda r, k: r.get(k) or 0.0
SIDE = {
 "srv": (lambda r: sum(g(r, f"srv.iown{p}.rx_batches") for p in (0, 1)), lambda r: sum(g(r, f"srv.iown{p}.rx_dg") for p in (0, 1))),
 "cli": (lambda r: sum(g(r, f"cli.iown{p}.tx_batches") for p in (0, 1)), lambda r: sum(g(r, f"cli.iown{p}.tx_dg") for p in (0, 1))),
}
out = []
def P(s=""): out.append(s); print(s)
P("POST HOC / NOT REGISTERED. Out-of-sample I/O-batch price: OLS cpu_xfer = f*batches + v*datagrams [+ w*wall] (no intercept) fit on rows of the training cells, applied to c8's mean arm deltas")
for side, (fb, fd) in SIDE.items():
    for train in (("c1s-400", "c1d-400", "c2-100"), ("c1d-400", "c2-100"), ("c2-100", "c8-100"), ("c1d-400", "c2-100", "c8-100")):
        for with_wall in (False, True):
            tr = [r for r in R if r["cell"] in train]
            cols = [np.array([fb(r) for r in tr]), np.array([fd(r) for r in tr])] + ([np.array([r["seconds"] for r in tr])] if with_wall else [])
            y = np.array([r[f"{side}.thr.cpu_s"] for r in tr])
            A = np.column_stack(cols); b, *_ = np.linalg.lstsq(A, y, rcond=None)
            r2 = 1 - ((y - A @ b) ** 2).sum() / ((y - y.mean()) ** 2).sum()
            c8 = lambda arm: [r for r in R if r["cell"] == "c8-100" and r["arm"] == arm]
            mb = lambda arm, f: st.mean(f(r) for r in c8(arm))
            db, dd, dw = mb("CAND", fb) - mb("BASE", fb), mb("CAND", fd) - mb("BASE", fd), mb("CAND", lambda r: r["seconds"]) - mb("BASE", lambda r: r["seconds"])
            dcpu = mb("CAND", lambda r: r[f"{side}.thr.cpu_s"]) - mb("BASE", lambda r: r[f"{side}.thr.cpu_s"])
            pred = b[0] * db + b[1] * dd + (b[2] * dw if with_wall else 0)
            P(f"  {side} train={'+'.join(train):26s} wall={'y' if with_wall else 'n'}  f={b[0]*1e6:7.1f} us/batch  v={b[1]*1e6:6.2f} us/dg" + (f"  w={b[2]:+.3f} s/s" if with_wall else "") +
              f"  R2={r2:.3f}  | c8 d_batches={db:+.0f} d_dg={dd:+.0f} -> predicted dCPU {pred:+.3f}s of actual {dcpu:+.3f}s ({100*pred/dcpu:.0f}%)")
open(os.path.join(D, "diag-c8/q1c_out.txt"), "w").write("\n".join(out) + "\n")
