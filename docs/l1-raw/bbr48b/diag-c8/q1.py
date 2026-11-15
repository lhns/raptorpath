# POST HOC, not registered. Q1: where does the c8 CPU delta live? CPU-seconds ledger per side, then price checks.
import json, os, re, sys, statistics as st
import numpy as np
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
F = {(r["cell"], r["arm"], r["seed"], r["rep"]): r for r in json.load(open(os.path.join(D, "diag-c8/feat.json")))}
CELLS = ["c1s-400", "c1d-400", "c2-100", "c8-100"]
def extra(cell, arm, seed, rep):
    stem = os.path.join(L, f"{cell}@def-{arm}-s{seed}-r{rep}-a1")
    e = {}
    # server receiver task: busy% over in-transfer RDIAG lines (msgs >= 1000/s, the documented filter)
    b = [int(m.group(1)) for m in (re.match(r"\[RDIAG\] busy=(\d+)% msgs=(\d+)/s", l) for l in open(stem + "-s.log", errors="replace")) if m and int(m.group(2)) >= 1000]
    e["rx_task_busy_pct"] = st.mean(b) if b else None
    e["rx_task_lines"] = len(b)
    # client sender task: sum of busy_us over [DIAG] lines (each line covers its own interval `us=`)
    bu = us = 0
    for l in open(stem + "-c.log", errors="replace"):
        m = re.search(r"wait\[.* us=(\d+) busy=\d+% busy_us=(\d+)\]", l)
        if l.startswith("[DIAG] t=") and m: us += int(m.group(1)); bu += int(m.group(2))
    e["tx_task_busy_s"] = bu / 1e6; e["tx_task_cover_s"] = us / 1e6
    # QDISC per-leg delivered bytes (client egress = data direction)
    for l in open(stem + "-drv.out", errors="replace"):
        m = re.search(r"QDISC (cli|srv)(\d): .* Sent (\d+) bytes (\d+) pkt", l)
        if m: e[f"qdisc_{m.group(1)}{m.group(2)}_bytes"] = int(m.group(3)); e[f"qdisc_{m.group(1)}{m.group(2)}_pkt"] = int(m.group(4))
    return e
rows = []
for k, r in F.items():
    r = dict(r); r.update(extra(*k)); rows.append(r)
json.dump(rows, open(os.path.join(D, "diag-c8/feat2.json"), "w"))
def g(r, k, d=0.0):
    v = r.get(k); return d if v is None else v
def ledger(r, side):
    sec = r["seconds"]
    cpu = g(r, f"{side}.thr.cpu_s"); busy = g(r, f"{side}.thr.busy_s")
    quinn = sum((g(r, f"{side}.iown{p}.q_wall_us") - g(r, f"{side}.iown{p}.q_asleep_us")) / 1e6 for p in (0, 1))
    task = (g(r, "rx_task_busy_pct") / 100 * sec) if side == "srv" else g(r, "tx_task_busy_s")
    return {"cpu": cpu, "busy": busy, "nonbusy(cpu-busy)": cpu - busy, "task": task, "quinn_sect": quinn,
            "busy_resid(busy-task-quinn)": busy - task - quinn, "wall": sec, "cores": cpu / sec,
            "park": g(r, f"{side}.thr.park"), "polls": sum(g(r, f"{side}.iown{p}.polls") for p in (0, 1)),
            "batches": sum(g(r, f"{side}.iown{p}.{x}") for p in (0, 1) for x in (("rx_batches", "ctrl_batches") if side == "srv" else ("tx_batches", "ack_batches")))}
out = []
def P(s=""): out.append(s); print(s)
P("POST HOC / NOT REGISTERED. Q1 CPU-seconds ledger, xfer phase ([THR] sum phase=xfer), medians over 12 rows per arm; delta = CAND - BASE (median of each)")
P("task = server receiver task busy (RDIAG busy% x wall, includes lock wait) | client sender task busy (sum of [DIAG] wait busy_us)")
P("quinn_sect = sum over legs of owner quinn-section thread CPU (q_wall_us - q_asleep_us)")
for side in ("srv", "cli"):
    P(f"\n== side {side}")
    keys = list(ledger(rows[0], side).keys())
    P(f"{'cell':8s} {'item':28s} {'BASE':>10s} {'CAND':>10s} {'delta':>9s} {'rel':>7s}")
    for c in CELLS:
        for k in keys:
            b = st.median([ledger(r, side)[k] for r in rows if r["cell"] == c and r["arm"] == "BASE"])
            a = st.median([ledger(r, side)[k] for r in rows if r["cell"] == c and r["arm"] == "CAND"])
            P(f"{c:8s} {k:28s} {b:10.3f} {a:10.3f} {a-b:+9.3f} {100*(a/b-1) if b else float('nan'):+6.1f}%")
# mean-based deltas (additive ledger: mean deltas add up, medians do not)
P("\n== additive ledger (MEANS, so components sum): delta CAND - BASE in CPU-seconds")
for side in ("srv", "cli"):
    for c in CELLS:
        m = {}
        for arm in ("BASE", "CAND"):
            ls = [ledger(r, side) for r in rows if r["cell"] == c and r["arm"] == arm]
            m[arm] = {k: st.mean(l[k] for l in ls) for k in ls[0]}
        d = {k: m["CAND"][k] - m["BASE"][k] for k in m["BASE"]}
        P(f"  {side} {c:8s} dCPU={d['cpu']:+.3f}s ({100*d['cpu']/m['BASE']['cpu']:+.1f}%) = dTask {d['task']:+.3f} + dQuinn {d['quinn_sect']:+.3f} + dBusyResid {d['busy_resid(busy-task-quinn)']:+.3f} + dNonBusy {d['nonbusy(cpu-busy)']:+.3f}   | dWall {d['wall']:+.3f}s dPark {d['park']:+.0f} dPolls {d['polls']:+.0f} dBatches {d['batches']:+.0f}")
# seed split at c8
P("\n== c8 per seed (means): dCPU srv / cli, dWall")
for s in ("42", "7"):
    def mm(arm, f): return st.mean(f(r) for r in rows if r["cell"] == "c8-100" and r["arm"] == arm and r["seed"] == s)
    P(f"  s{s}: srv {mm('CAND', lambda r: r['srv.thr.cpu_s']) - mm('BASE', lambda r: r['srv.thr.cpu_s']):+.3f}s  cli {mm('CAND', lambda r: r['cli.thr.cpu_s']) - mm('BASE', lambda r: r['cli.thr.cpu_s']):+.3f}s  wall {mm('CAND', lambda r: r['seconds']) - mm('BASE', lambda r: r['seconds']):+.3f}s")
# price checks: within-arm, seed-centred slopes of cpu on candidate x; implied cost from the arm delta
def centred(c, x, y, arms=("BASE", "CAND")):
    X, Y = [], []
    for arm in arms:
        for s in ("42", "7"):
            rr = [r for r in rows if r["cell"] == c and r["arm"] == arm and r["seed"] == s]
            xs = np.array([x(r) for r in rr], float); ys = np.array([y(r) for r in rr], float)
            X += list(xs - xs.mean()); Y += list(ys - ys.mean())
    X, Y = np.array(X), np.array(Y)
    slope = (X @ Y) / (X @ X) if X @ X > 0 else float("nan")
    rr_ = np.corrcoef(X, Y)[0, 1] if X.std() > 0 and Y.std() > 0 else float("nan")
    return slope, rr_
CAND_X = {
    "wall_s": lambda r, s: r["seconds"],
    "park": lambda r, s: g(r, f"{s}.thr.park"),
    "polls": lambda r, s: ledger(r, s)["polls"],
    "io_batches": lambda r, s: ledger(r, s)["batches"],
    "dec_rows(srv)": lambda r, s: g(r, "srv.rdiag.mean_rows"),
    "coded_share": lambda r, s: g(r, "coded_share"),
    "rtx_n(srv SUCC)": lambda r, s: g(r, "srv.SUCC.rtx_n"),
    "egress_skbs(1/gso)": lambda r, s: g(r, "drv.truth0.egress_skbs") + g(r, "drv.truth1.egress_skbs"),
}
P("\n== price checks (within cell, within arm, seed-centred; n=24): slope of side CPU-s on x, r; arm delta of x (means); implied dCPU = slope x dx; actual dCPU")
for side in ("srv", "cli"):
    for c in CELLS:
        dcpu = st.mean(r[f"{side}.thr.cpu_s"] for r in rows if r["cell"] == c and r["arm"] == "CAND") - st.mean(r[f"{side}.thr.cpu_s"] for r in rows if r["cell"] == c and r["arm"] == "BASE")
        for name, fx in CAND_X.items():
            sl, rr_ = centred(c, lambda r: fx(r, side), lambda r: r[f"{side}.thr.cpu_s"])
            dx = st.mean(fx(r, side) for r in rows if r["cell"] == c and r["arm"] == "CAND") - st.mean(fx(r, side) for r in rows if r["cell"] == c and r["arm"] == "BASE")
            imp = sl * dx
            P(f"  {side} {c:8s} {name:20s} slope={sl:+.4g} r={rr_:+.2f} dx={dx:+.4g} implied={imp:+.3f}s of actual {dcpu:+.3f}s (unit cost needed dCPU/dx={dcpu/dx if dx else float('nan'):+.4g})")
open(os.path.join(D, "diag-c8/q1_out.txt"), "w").write("\n".join(out) + "\n")
