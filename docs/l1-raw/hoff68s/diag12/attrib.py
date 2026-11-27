"""§12 CPU attribution, GATE against R4 (same binary, gate arm off), within-session pairs only.

Rule (fixed before counting):
  1. Pairs: plan 68 K2, K8 (G vs R); 68r PC, LR (G vs R); 68s BU (G vs R). Excluded: A2 and PH
     (the gate changes the bytes sent and the run time), and every cross-session pair.
  2. Each counter is the median of the arm's 3 rows; deltas are G minus R.
  3. Producer term = delta of sum(busy_us) over the [DIAG] lines (the sender loop's body time).
  4. Off-producer residual = delta CPUCLI - producer term.
  5. The owner-timer unit cost c_w is fitted on the cells where hand-offs and GSO sends do not
     change (PC, BU, LR): c_w = residual / owner_timer_wakes. It is then applied to K2, K8; what
     is left there is booked to the extra hand-offs (delta eb_bursts) and run time.
Usage: python -I attrib.py <l1-raw dir>
"""
import statistics as st
import sys

import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rows import rows  # noqa: E402

PAIRS = [("68", "K2"), ("68", "K8"), ("68r", "PC"), ("68r", "LR"), ("68s", "BU")]
KEYS = ["cpu_cli", "secs", "bytes", "skbs", "iters", "busy_us", "eb_bursts", "hand", "otw", "refus", "batch0", "wk_paused"]

R = rows()


def med(led, cell, arm, k):
    v = [r[k] for r in R if r["ledger"] == led and r["cell"] == cell and r["arm"] == arm and r.get(k) is not None]
    return st.median(v) if v else 0.0


print("cell     cpuR   cpuG  dCPU  d%    dbusy  resid  otw    resid/otw_us  d_eb   d_skbs  d_secs  batchR batchG  GB   per-GB: dCPU dbusy resid")
fit = []
res = {}
for led, cell in PAIRS:
    g = {k: med(led, cell, "G", k) for k in KEYS}
    r = {k: med(led, cell, "R", k) for k in KEYS}
    gb = g["bytes"] / 1e9
    d = g["cpu_cli"] - r["cpu_cli"]
    db = (g["busy_us"] - r["busy_us"]) / 1e6
    resid = d - db
    res[cell] = (g, r, d, db, resid)
    per = resid / g["otw"] * 1e6 if g["otw"] else float("nan")
    if cell in ("PC", "BU", "LR"):
        fit.append((resid, g["otw"]))
    print(f"{led:4}{cell}  {r['cpu_cli']:6.2f} {g['cpu_cli']:6.2f} {d:5.2f} {100*d/r['cpu_cli']:5.1f} {db:6.3f} {resid:6.3f} {g['otw']:6.0f} {per:8.1f}   "
          f"{g['eb_bursts']-r['eb_bursts']:7.0f} {g['skbs']-r['skbs']:6.0f} {g['secs']-r['secs']:6.2f}  {r['batch0']:5.1f} {g['batch0']:5.1f}  {gb:5.3f}  "
          f"{d/gb:6.2f} {db/gb:6.2f} {resid/gb:6.2f}")

# least squares through the origin on the clean cells: resid = c_w * otw
cw = sum(a * b for a, b in fit) / sum(b * b for _, b in fit)
print(f"\nc_w (fit on PC, BU, LR, through origin) = {cw*1e6:.1f} us per owner-timer wake")
for cell in ("PC", "BU", "LR"):
    g, r, d, db, resid = res[cell]
    print(f"  {cell}: resid {resid:.3f} s, c_w*otw {cw*g['otw']:.3f} s")
print("\nK cells: producer / owner timer (c_w*otw) / left (hand-offs, run time), seconds and s per GB")
for cell in ("K2", "K8"):
    g, r, d, db, resid = res[cell]
    gb = g["bytes"] / 1e9
    ow = cw * g["otw"]
    left = d - db - ow
    deb = g["eb_bursts"] - r["eb_bursts"]
    print(f"  {cell}: dCPU {d:.3f} = producer {db:.3f} + owner {ow:.3f} + left {left:.3f}   "
          f"(per GB {d/gb:.2f} = {db/gb:.2f} + {ow/gb:.2f} + {left/gb:.2f}; d_eb {deb:.0f}, left/d_eb {left/deb*1e6 if deb else float('nan'):.1f} us)")
    # producer term per first send and per loop iteration
    print(f"      producer per first send {db/g['hand']*1e6:.2f} us ; busy per iter R {r['busy_us']/r['iters']:.1f} G {g['busy_us']/g['iters']:.1f} us ; "
          f"time-proportional share if left is run time: {left/(g['secs']-r['secs']) if g['secs']!=r['secs'] else float('nan'):.3f} s/s")
for cell in ("PC", "BU", "LR"):
    g, r, d, db, resid = res[cell]
    print(f"  {cell}: producer per first send {db/g['hand']*1e6:.2f} us ; busy per iter R {r['busy_us']/r['iters']:.1f} G {g['busy_us']/g['iters']:.1f} us")
