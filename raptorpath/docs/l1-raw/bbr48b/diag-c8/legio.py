# POST HOC, not registered. Per-leg I/O granularity and driver routing, all cells, medians: client tx dg/batch, server rx dg/batch,
# server ctrl dg/batch, client drv_off share (on_sent called off the owner's last thread = the ConnectionDriver sent it, e.g. on a pacing timer).
import json, os, sys, statistics as st
D = sys.argv[1]; R = json.load(open(os.path.join(D, "diag-c8/feat2.json")))
g = lambda r, k: r.get(k) or 0.0
out = ["POST HOC / NOT REGISTERED. Per-leg I/O granularity (medians over 12 rows)",
       f"{'cell':8s} {'leg':3s} {'arm':4s} {'cli tx dg/batch':>15s} {'cli tx batches':>14s} {'srv rx dg/batch':>15s} {'srv rx batches':>14s} {'srv ctrl dg/batch':>17s} {'cli drv_off share':>17s} {'srv drv_off share':>17s} {'gso':>5s}"]
for cell in ("c1s-400", "c1d-400", "c2-100", "c8-100"):
    for p in (0, 1):
        for arm in ("BASE", "CAND"):
            rr = [r for r in R if r["cell"] == cell and r["arm"] == arm]
            if not g(rr[0], f"cli.iown{p}.tx_dg"): continue
            m = lambda f: st.median(f(r) for r in rr)
            out.append(f"{cell:8s} {p:3d} {arm:4s} {m(lambda r: g(r,f'cli.iown{p}.tx_dg')/g(r,f'cli.iown{p}.tx_batches')):15.2f} {m(lambda r: g(r,f'cli.iown{p}.tx_batches')):14.0f} "
                       f"{m(lambda r: g(r,f'srv.iown{p}.rx_dg')/g(r,f'srv.iown{p}.rx_batches')):15.2f} {m(lambda r: g(r,f'srv.iown{p}.rx_batches')):14.0f} "
                       f"{m(lambda r: g(r,f'srv.iown{p}.ctrl_dg')/g(r,f'srv.iown{p}.ctrl_batches')):17.2f} "
                       f"{m(lambda r: g(r,f'cli.iown{p}.drv_off')/(g(r,f'cli.iown{p}.drv_on')+g(r,f'cli.iown{p}.drv_off'))):17.3f} "
                       f"{m(lambda r: g(r,f'srv.iown{p}.drv_off')/(g(r,f'srv.iown{p}.drv_on')+g(r,f'srv.iown{p}.drv_off'))):17.3f} {m(lambda r: g(r,f'drv.truth{p}.gso')):5.2f}")
print("\n".join(out)); open(os.path.join(D, "diag-c8/legio_out.txt"), "w").write("\n".join(out) + "\n")
