#!/usr/bin/env python3
"""Ledger replay for `fix/loss-crosspath` (MECHANICAL DEFECT SWEEP item 3).

What the per-path loss estimator READ on the wire (legacy, gap-derived) vs
what it WOULD have read under `RWM_LOSS_SENT_TRUTH`, recomputed from the
ackdiag battery's own ledgers. NO re-run: every operand is already in the
`recon[...]` block of each `[ACKDIAG]` line, because the gauge was built to
reconcile exactly these three counters.

    legacy:  eps_old = 1 - crecv/cexp   (cexp = PathBatchTracker gap estimate,
                                         summed over a GLOBAL batch_seq)
    fixed :  eps_new = 1 - crecv/sent   (sent  = PathStats::symbols_sent, the
                                         SENDER's own per-path wire-handoff
                                         count -- the ledger's `cr/s` column
                                         is exactly this law's complement)

Usage:  python xpath_loss_replay.py <path-to-docs/l1-raw>
"""
import re, sys, glob, os, statistics as st

ROOT = sys.argv[1]
PAT = re.compile(
    r"ACKDIAG (?P<cell>\S+) rep=(?P<rep>\d+) \[ACKDIAG\] (?P<path>p\d+).*?"
    r"recon\[sent=(?P<sent>\d+) crecv=(?P<crecv>\d+) cexp=(?P<cexp>\d+) "
    r"srcack=(?P<srcack>\d+) cr/s=(?P<crs>[\d.]+) ce/cr=(?P<cecr>[\d.]+)"
)

# realized per-path packet loss from the cell definitions (tc netem), goal-gate
# "Ack-Cadence Measurement (VM)" READOUT 4 + the cell table.
REALIZED = {
    ("c2r100", "p0"): 0.0081,
    ("c7", "p0"): 0.0055, ("c7", "p1"): 0.0055,
    ("c8", "p0"): 0.0055, ("c8", "p1"): 0.0196,
}

rows = {}
for line in open(os.path.join(ROOT, "ackdiag-ackdiag-s42.log"), encoding="utf-8", errors="replace"):
    m = PAT.search(line)
    if not m:
        continue
    d = m.groupdict()
    key = (d["cell"], d["path"])
    sent, crecv, cexp = int(d["sent"]), int(d["crecv"]), int(d["cexp"])
    rows.setdefault(key, []).append((sent, crecv, cexp))

print(f"{'cell/path':<12} {'N':>3} {'ce/cr med':>10} {'e_old med':>10} "
      f"{'s/cr med':>9} {'e_new med':>10} {'realized':>9} "
      f"{'old/real':>9} {'new/real':>9}")
print("-" * 96)
summary = {}
for key in sorted(rows):
    w = rows[key]
    cecr = [c / r for (s, r, c) in w]
    eold = [max(0.0, 1 - r / c) for (s, r, c) in w]
    scr = [s / r for (s, r, c) in w]                  # expected/received, fixed
    enew = [max(0.0, 1 - r / s) for (s, r, c) in w]
    real = REALIZED[key]
    med = st.median
    summary[key] = (med(cecr), med(eold), med(scr), med(enew), real)
    print(f"{key[0]+'/'+key[1]:<12} {len(w):>3} {med(cecr):>10.3f} {med(eold):>10.4f} "
          f"{med(scr):>9.3f} {med(enew):>10.4f} {real:>9.4f} "
          f"{med(eold)/real:>8.1f}x {med(enew)/real:>8.2f}x")

# Aggregate over the whole battery (sums, not per-window medians): the
# window-edge in-flight lag that makes single windows read >1 or <1 cancels.
print()
print("AGGREGATED over all windows (sums; NOTE the c8 legs read NEGATIVE: the")
print("gauge snapshots `sent` at report time while `crecv` accrues over the")
print("window, and c8 runs 3 transfers per rep, so its window edges do not")
print("close. That +/-3-7% is the LEDGER's alignment, not the law's: the")
print("in-engine form diffs both cursors at the same instant. What the replay")
print("establishes is the CLASS -- 0.94-1.01 expected/received, not 2.05-5.59.")
print(f"{'cell/path':<12} {'Sigma sent':>11} {'Sigma crecv':>11} {'Sigma cexp':>11} "
      f"{'e_old':>8} {'e_new':>8} {'realized':>9}")
print("-" * 80)
for key in sorted(rows):
    S = sum(s for s, r, c in rows[key]); R = sum(r for s, r, c in rows[key])
    C = sum(c for s, r, c in rows[key])
    eo, en = 1 - R / C, 1 - R / S
    print(f"{key[0]+'/'+key[1]:<12} {S:>11} {R:>11} {C:>11} {eo:>8.4f} {en:>8.4f} "
          f"{REALIZED[key]:>9.4f}")

# --- downstream: the NACK repair margin, mod.rs:6867 -----------------
# margin = ceil(retransmitted * max-over-active-paths loss_rate)
print()
print("NACK repair margin  margin = ceil(retransmitted * max_p eps_p)  (net/mod.rs:6867)")
print(f"{'cell':<8} {'eps_old(max)':>13} {'eps_new(max)':>13} "
      f"{'margin/100 old':>15} {'margin/100 new':>15} {'inflation':>10}")
print("-" * 80)
import math
for cell in ("c2r100", "c7", "c8"):
    ks = [k for k in rows if k[0] == cell]
    S = {k: sum(s for s, r, c in rows[k]) for k in ks}
    R = {k: sum(r for s, r, c in rows[k]) for k in ks}
    C = {k: sum(c for s, r, c in rows[k]) for k in ks}
    eo = max(1 - R[k] / C[k] for k in ks)
    en = max(max(0.0, 1 - R[k] / S[k]) for k in ks)
    mo, mn = math.ceil(100 * eo), math.ceil(100 * en)
    print(f"{cell:<8} {eo:>13.4f} {en:>13.4f} {mo:>15} {mn:>15} "
          f"{(mo/max(mn,1)):>9.1f}x")
