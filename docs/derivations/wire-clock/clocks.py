"""Post-hoc recovery-clock table for docs/derivations/wire-clock.md section 4.

Usage: python -I clocks.py <extracted diag-ld dir> <cell, e.g. c8-100> [t_min_s]

Per row, over client [DIAG] ticks with t >= t_min and src > 0 (ms):
  rtt<j>, wrtt<j>     app-echo EWMA and quinn smoothed RTT of path j
  tau<j>              quinn send-buffer dwell by Little: (hand - tx) / (d tx / dt)
  echoX<j>            rtt - wrtt
  thr_today<j>        9/8 * rtt_j (the shipped per-path time threshold, floor ignored)
  thr_wireform<j>     max(tau_j + 9/8 * wrtt_j, 10): the rejected wire form
  refire_today<k>     re-fire interval of a retransmit on k today:
                      N > 1: max(max_i rtt_i, 9/8 rtt_k); N = 1: max_i rtt_i (cooldown)
  refire_new<k>       9/8 * rtt_k (the live-flight time threshold, cooldown deleted)
Server [DIAG] ticks give rx_wrtt<j> (the receiving endpoint's own quinn RTT) and
the refresh clamp(2 * agg_j rx_wrtt_j, 25, 100) for agg = max and min.
Prints per arm the median of per-row medians and the row range.
"""
import os, re, sys, statistics as st
from collections import defaultdict

D, CELL = sys.argv[1], sys.argv[2]
TMIN = float(sys.argv[3]) if len(sys.argv) > 3 else 2.0

re_t = re.compile(r"^\[DIAG\] t=([\d.]+)s")
re_src = re.compile(r" src=(\d+)sym/s")
re_dgq = re.compile(r"dgq(\d)\[hand=(\d+) tx=(\d+) full=(\d+)")


def ticks(path):
    out = []
    with open(path, errors="replace") as f:
        for line in f:
            m = re_t.match(line)
            if not m:
                continue
            t = float(m.group(1))
            src = int(re_src.search(line).group(1)) if re_src.search(line) else 0
            rtt, wrtt, dg = {}, {}, {}
            for pm in re.finditer(r" p(\d):infl=\S*.*?rtt=(\d+)/wrtt=(\d+)/", line):
                rtt[int(pm.group(1))] = int(pm.group(2))
                wrtt[int(pm.group(1))] = int(pm.group(3))
            for dm in re_dgq.finditer(line):
                dg[int(dm.group(1))] = (int(dm.group(2)), int(dm.group(3)))
            out.append((t, src, rtt, wrtt, dg))
    return out


def med(xs):
    return st.median(xs) if xs else float("nan")


rows = defaultdict(list)  # arm -> list of per-row dicts
for fn in sorted(os.listdir(D)):
    if not fn.startswith(CELL + "@") or not fn.endswith("-c.log"):
        continue
    arm = fn.split("@def-")[1].split("-s")[0]
    cl = ticks(os.path.join(D, fn))
    sv_fn = fn[:-6] + "-s.log"
    sv = ticks(os.path.join(D, sv_fn)) if os.path.exists(os.path.join(D, sv_fn)) else []
    acc = defaultdict(list)
    prev = None
    for (t, src, rtt, wrtt, dg) in cl:
        if prev is not None and t >= TMIN and src > 0 and rtt:
            dt = t - prev[0]
            paths = sorted(rtt)
            tau = {}
            for j in paths:
                if j in dg and j in prev[4] and dt > 0:
                    r = (dg[j][1] - prev[4][j][1]) / dt
                    q = dg[j][0] - dg[j][1]
                    tau[j] = 1000.0 * q / r if r > 0 else 0.0
                else:
                    tau[j] = 0.0
                acc[f"rtt{j}"].append(rtt[j])
                acc[f"wrtt{j}"].append(wrtt[j])
                acc[f"tau{j}"].append(tau[j])
                acc[f"echoX{j}"].append(rtt[j] - wrtt[j])
                acc[f"thr_today{j}"].append(9 / 8 * rtt[j])
                acc[f"thr_wireform{j}"].append(max(tau[j] + 9 / 8 * wrtt[j], 10.0))
            n = len(paths)
            pool = max(rtt[j] for j in paths)
            for k in paths:
                today = max(pool, 9 / 8 * rtt[k]) if n > 1 else pool
                acc[f"refire_today{k}"].append(today)
                acc[f"refire_new{k}"].append(9 / 8 * rtt[k])
        prev = (t, src, rtt, wrtt, dg)
    for (t, src, rtt, wrtt, dg) in sv:
        if t >= TMIN and wrtt:
            for j in wrtt:
                acc[f"rx_wrtt{j}"].append(wrtt[j])
            acc["refresh_new_max"].append(min(max(25.0, 2 * max(wrtt.values())), 100.0))
            acc["refresh_new_min"].append(min(max(25.0, 2 * min(wrtt.values())), 100.0))
    if acc:
        rows[arm].append({k: med(v) for k, v in acc.items()})

for arm, rs in rows.items():
    keys = sorted(rs[0].keys())
    print(f"== {CELL} {arm} rows={len(rs)} t>={TMIN}")
    for k in keys:
        vals = [r[k] for r in rs if k in r]
        print(f"  {k:12s} {med(vals):8.1f}  [{min(vals):.1f}-{max(vals):.1f}]")
