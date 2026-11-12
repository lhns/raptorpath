# POST HOC, not registered. Q1b: mediation at c8 (OLS cpu ~ arm + seed [+ X]); driver of server parks; run conditions.
import json, os, sys, statistics as st
import numpy as np
D = sys.argv[1]
R = json.load(open(os.path.join(D, "diag-c8/feat2.json")))
LD = {}
for ln in open(os.path.join(D, "run/bbr48/ld.log"), encoding="utf-8", errors="replace"):
    if ln.startswith("P38ROW "):
        r = json.loads(ln[7:]); LD[(r["cell"], r["arm"], str(r["seed"]), int(r["rep"]))] = r
order = {}
for i, ln in enumerate(open(os.path.join(D, "run/bbr48/ld.log"), encoding="utf-8", errors="replace")):
    if ln.startswith("=== rep="):
        kv = dict(t.split("=", 1) for t in ln.split() if "=" in t)
        order[(kv["cell"], kv["arm"], kv["seed"], int(kv["rep"]))] = len(order)
g = lambda r, k: r.get(k) or 0.0
X = {
 "wall_s": lambda r: r["seconds"],
 "srv.park": lambda r: g(r, "srv.thr.park"),
 "srv.leg1 rx_batches": lambda r: g(r, "srv.iown1.rx_batches"),
 "srv.leg0 rx_batches": lambda r: g(r, "srv.iown0.rx_batches"),
 "srv.all io batches": lambda r: sum(g(r, f"srv.iown{p}.{k}") for p in (0, 1) for k in ("rx_batches", "ctrl_batches")),
 "srv.all polls": lambda r: sum(g(r, f"srv.iown{p}.polls") for p in (0, 1)),
 "srv.dec_rows": lambda r: g(r, "srv.rdiag.mean_rows"),
 "srv.reorder lead(p0+p1)": lambda r: g(r, "srv.C8CONV-R.p0.lead") + g(r, "srv.C8CONV-R.p1.lead"),
 "srv.LATE.d_us": lambda r: g(r, "srv.LATE.d_us"),
 "coded_share": lambda r: g(r, "coded_share"),
 "srv.rtx_n": lambda r: g(r, "srv.SUCC.rtx_n"),
 "srv.RFA.fires": lambda r: g(r, "srv.RFA.fires"),
 "srv.LAT.over": lambda r: g(r, "srv.LAT.over"),
 "egress_skbs": lambda r: g(r, "drv.truth0.egress_skbs") + g(r, "drv.truth1.egress_skbs"),
 "cli.leg1 tx_batches": lambda r: g(r, "cli.iown1.tx_batches"),
 "cli.all io batches": lambda r: sum(g(r, f"cli.iown{p}.{k}") for p in (0, 1) for k in ("tx_batches", "ack_batches")),
 "cli.sender task busy": lambda r: g(r, "tx_task_busy_s"),
 "cli.park": lambda r: g(r, "cli.thr.park"),
 "run order": lambda r: order.get((r["cell"], r["arm"], r["seed"], r["rep"]), 0),
 "load/io_before": lambda r: LD[(r["cell"], r["arm"], r["seed"], r["rep"])].get("io_before") or 0,
}
def ols(y, cols):
    A = np.column_stack(cols); b, *_ = np.linalg.lstsq(A, y, rcond=None)
    res = y - A @ b; return b, res
out = []
def P(s=""): out.append(s); print(s)
P("POST HOC / NOT REGISTERED. Mediation: OLS  cpu = a + b_arm*CAND + b_seed*s7 [+ c*X(z-scored)], per cell, n=24. 'kept' = b_arm with X / b_arm without X.")
for cell in ("c8-100", "c1d-400", "c2-100"):
    rr = [r for r in R if r["cell"] == cell]
    arm = np.array([1.0 if r["arm"] == "CAND" else 0.0 for r in rr]); s7 = np.array([1.0 if r["seed"] == "7" else 0.0 for r in rr]); one = np.ones(len(rr))
    for side in ("srv", "cli"):
        y = np.array([r[f"{side}.thr.cpu_s"] for r in rr])
        b0, res0 = ols(y, [one, arm, s7])
        P(f"\n== {cell} {side}: b_arm (no mediator) = {b0[1]:+.3f} s  (resid sd {res0.std():.3f})")
        for name, f in X.items():
            x = np.array([f(r) for r in rr], float)
            if x.std() == 0: continue
            z = (x - x.mean()) / x.std()
            b, res = ols(y, [one, arm, s7, z])
            # arm effect on X itself
            bx, _ = ols(x, [one, arm, s7])
            P(f"  {name:24s} dX(arm)={bx[1]:+11.4g} ({100*bx[1]/x[arm==0].mean() if x[arm==0].mean() else float('nan'):+6.1f}%)  c={b[3]:+.3f}s/sd  b_arm|X={b[1]:+.3f}  kept={b[1]/b0[1]:+.2f}  resid sd {res.std():.3f}")
# what drives server parks at c8?
P("\n== c8: server parks per I/O event, per arm (medians)")
for arm in ("BASE", "CAND"):
    rr = [r for r in R if r["cell"] == "c8-100" and r["arm"] == arm]
    def m(f): return st.median(f(r) for r in rr)
    P(f"  {arm}: park {m(lambda r: g(r,'srv.thr.park')):.0f}  rx_batches L0 {m(lambda r: g(r,'srv.iown0.rx_batches')):.0f} L1 {m(lambda r: g(r,'srv.iown1.rx_batches')):.0f}  "
      f"dg/rx_batch L0 {m(lambda r: g(r,'srv.iown0.rx_dg')/g(r,'srv.iown0.rx_batches')):.2f} L1 {m(lambda r: g(r,'srv.iown1.rx_dg')/g(r,'srv.iown1.rx_batches')):.2f}  "
      f"ctrl dg/batch L0 {m(lambda r: g(r,'srv.iown0.ctrl_dg')/g(r,'srv.iown0.ctrl_batches')):.2f} L1 {m(lambda r: g(r,'srv.iown1.ctrl_dg')/g(r,'srv.iown1.ctrl_batches')):.2f}  "
      f"cli tx dg/batch L0 {m(lambda r: g(r,'cli.iown0.tx_dg')/g(r,'cli.iown0.tx_batches')):.2f} L1 {m(lambda r: g(r,'cli.iown1.tx_dg')/g(r,'cli.iown1.tx_batches')):.2f}  "
      f"gso L0 {m(lambda r: g(r,'drv.truth0.gso')):.2f} L1 {m(lambda r: g(r,'drv.truth1.gso')):.2f}  "
      f"park/(rx_batches) {m(lambda r: g(r,'srv.thr.park')/(g(r,'srv.iown0.rx_batches')+g(r,'srv.iown1.rx_batches'))):.2f}  "
      f"srv drv_off/(on+off) L0 {m(lambda r: g(r,'srv.iown0.drv_off')/(g(r,'srv.iown0.drv_on')+g(r,'srv.iown0.drv_off'))):.3f} L1 {m(lambda r: g(r,'srv.iown1.drv_off')/(g(r,'srv.iown1.drv_on')+g(r,'srv.iown1.drv_off'))):.3f}  "
      f"cli drv_off/(on+off) L0 {m(lambda r: g(r,'cli.iown0.drv_off')/(g(r,'cli.iown0.drv_on')+g(r,'cli.iown0.drv_off'))):.3f} L1 {m(lambda r: g(r,'cli.iown1.drv_off')/(g(r,'cli.iown1.drv_on')+g(r,'cli.iown1.drv_off'))):.3f}")
open(os.path.join(D, "diag-c8/q1b_out.txt"), "w").write("\n".join(out) + "\n")
