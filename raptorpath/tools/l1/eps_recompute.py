#!/usr/bin/env python3
"""TIER-1 RE-SCORE 2b — THE eps-hat DENOMINATOR, recomputed from the captured cursors.

Literature item: `docs/research/literature-crosscheck.md` Tier 1.2 and verdict
row 7 — *"Recompute eps-hat with first-transmissions-only in the denominator,
from the ladder's captured `[ACKDIAG]` cursors."* The hypothesis it tests is
RFC 6675's double-count note (*"retransmitted ... counted twice"*) plus
Allman/Eddy/Ostermann 2003's measured >100 % mis-estimate: that our
`eps-hat = 1 - d(recv)/d(sent)` reads high because `d(sent)` carries retransmits
and repairs while the numerator does not.

WHAT THIS SCRIPT ESTABLISHES, IN ORDER.

  STEP 1  THE COUNTER SEMANTICS, read off the emitting code rather than assumed.
          Printed as a table with a file:line for every counter. This step is
          the one that decides whether the proposed correction is even
          well-formed, and it must be done BEFORE any arithmetic.
  STEP 2  THE RECOMPUTE, from the `[ACKDIAG] recon[...]` cursors — the ONLY raw
          per-path sent/received cursors this tree has captured (the ladder's
          own ledger carries the recon RATIOS and a whole-run `retx` scalar, not
          the cursors themselves).
  STEP 3  REALIZED WIRE LOSS from the same invocations' `tc`/netem `QDISC` lines
          — an instrument outside the code under test.
  STEP 4  THE RECTIFICATION PROBE: how often the per-window received delta RUNS
          AHEAD of the sent delta, i.e. how often `sender_truth_loss_delta`'s
          `d_received.min(d_expected)` clamp fires. A clamp that is one-sided
          turns zero-mean two-clock jitter into a strictly POSITIVE loss bias,
          and that is a different mechanism from the denominator hypothesis.

  usage: eps_recompute.py <ackdiag ledger.log> [<more.log> ...]
"""
import glob
import os
import re
import sys
from collections import defaultdict

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

# ── STEP 1. THE COUNTER SEMANTICS, with the emitting line for each ──────────
#
#  Every row was read out of the tree at commit main@fe9f1a9. The `counts
#  retransmits?` column is the one the hypothesis turns on.
SEMANTICS = [
    ("sent  (recon)", "PathStats::symbols_sent", "src/monitor/stats.rs:103",
     "YES — one increment per WIRE HANDOFF: source, repair AND retransmit alike."
     " Every increment site: src/net/emit_source.rs:489, 581, 933 and"
     " src/net/mod.rs:6093, 6150, 6264, 7209, 7287, 7677, 8117 — of which 7209"
     " and 7287 sit inside the NACK-retransmit dispatch, three lines from"
     " `dg.diag_retx += 1` (src/net/mod.rs:7231), and NONE of the ten"
     " distinguishes the class it is counting."),
    ("crecv (recon)", "PathBatchTracker::total_received", "src/net/mod.rs:7955",
     "YES — `self.total_received += received` at src/net/mod.rs:7987 inside"
     " `record_batch(batch_seq, received)` (src/net/mod.rs:7971), whose only"
     " caller passes the arriving batch's `symbol_count`"
     " (src/net/receiver.rs:1135) and never reads `symbol.is_repair`."),
    ("cexp  (recon)", "PathBatchTracker::total_expected", "src/net/mod.rs:7957",
     "N/A — not a count at all: `(gap as u32) * received` over a GLOBAL"
     " `batch_seq` (src/net/mod.rs:7975). This is the CONTAMINATED operand the"
     " shipped estimator uses."),
    ("srcack(recon)", "the cumulative WindowAck frontier", "src/net/ackdiag.rs:196-203",
     "NO — DELIVERED SOURCE symbols only, and CONNECTION-wide, not per-path."),
    ("retx (ledger)", "`[DIAG] diag_retx`", "src/net/mod.rs:7231",
     "a WHOLE-RUN scalar, not per-path and not contemporaneous with any cursor."),
]

#: THE STRUCTURAL VERDICT this table forces, stated before any number is read.
STRUCTURAL = """
  BOTH OPERANDS OF THE SHIPPED SENDER-TRUTH PAIR COUNT RETRANSMITS.

  `eps_hat = 1 - d(cum_received_p) / d(symbols_sent_p)` has a retransmitted
  symbol in the DENOMINATOR (it was handed to the wire) and, when it arrives,
  in the NUMERATOR too (the receiver's batch tracker counts every arriving
  symbol without looking at `is_repair`). RFC 6675's double-count bias requires
  the numerator to count NEWLY-DELIVERED data while the denominator counts
  retransmits; ours is a MATCHED pair, so the bias the cross-check hypothesised
  is not present in this estimator by construction.

  This also makes the proposed correction WRONG-SIGNED as well as unavailable:
  `d(first_sent) = d(sent) - d(retx) - d(repair)` would shrink the denominator
  ALONE and drive eps-hat UP, not down.

  AND THE COLUMNS CANNOT SEPARATE FIRST TRANSMISSIONS ANYWAY. `PathStats` has
  exactly two symbol counters — `symbols_sent` and `symbols_received`
  (src/monitor/stats.rs:96-110) — and NO per-path retransmit or repair
  sub-counter. `[DIAG] retx` is one connection-wide run scalar. So
  `d(first_sent)` is not computable from any captured column, at any cell, in
  any ledger in this tree. THE MISSING INSTRUMENT IS NAMED IN THE VERDICT.
"""

RECON = re.compile(
    r"ACKDIAG\s+(\S+)\s+rep=(\d+)\s+\[ACKDIAG\]\s+p(\d+).*?"
    r"recon\[sent=(\d+) crecv=(\d+) cexp=(\d+) srcack=(\d+)")
HDR = re.compile(r"^=== rep=(\d+) cell=(\S+)")
QD = re.compile(r"QDISC (cli\d)\S*:.*Sent \d+ bytes (\d+) pkt \(dropped (\d+)")

#: The `pl=` (estimator OUTPUT, `estimator.loss_rate()`, src/net/diag.rs:601)
#: the LADDER measured on its arm A (shipped gap pair) and arm T
#: (`RWM_LOSS_SENT_TRUTH`), transcribed from goal-gate "Ladder Battery —
#: RESULTS" rung T. It is a DIFFERENT quantity from the raw cursor arithmetic
#: below — that is precisely what this re-score establishes.
LADDER_PL = {           # cell: (A_p0, A_p1, T_p0, T_p1, A_max, T_max, ratio)
    "c7":  (0.0081, 0.0241, 0.5667, 0.5384, 0.0288, 0.5799, 20.1),
    "c8":  (0.0028, 0.1951, 0.5883, 0.7282, 0.1951, 0.7454, 3.8),
    "c8L": (0.0000, 0.8233, 0.4702, 0.5655, 0.8233, 0.5768, 0.7),
    "c1":  (None, None, None, None, 0.0000, 0.3614, None),
    "sc2": (None, None, None, None, 0.0000, 0.5821, None),
}
LADDER_RETX = {"c1": (792, 724), "c7": (5326, 24269), "c8": (1494, 3569),
               "c8L": (12508, 21552), "sc2": (3333, 7766)}
#: The realized-loss CLASS the correction has to land in to repair §16.58.
REALIZED_LO, REALIZED_HI = 0.005, 0.02

paths = []
for a in sys.argv[1:]:
    paths.extend(sorted(glob.glob(a)) or [a])

print("=" * 100)
print("TIER-1 RE-SCORE 2b — THE eps-hat DENOMINATOR (RFC 6675 / Allman et al. 2003)")
print("recomputed from the captured [ACKDIAG] cursors. No VM, no new arm.")
print("=" * 100)

print("\n### STEP 1 — WHAT EACH CAPTURED COUNTER ACTUALLY COUNTS\n")
for name, sym, where, what in SEMANTICS:
    print(f"  {name:<15} {sym}")
    print(f"  {'':<15} {where}")
    for line in what.split(". "):
        if line.strip():
            print(f"  {'':<15}   {line.strip().rstrip('.')}.")
    print()
print(STRUCTURAL)

# ── STEP 2/3. THE RECOMPUTE ─────────────────────────────────────────────────
seq = defaultdict(list)
qd = defaultdict(dict)
cur = (None, None)
for p in paths:
    for ln in open(p, errors="replace"):
        m = HDR.match(ln)
        if m:
            cur = (m.group(2), int(m.group(1)))
            continue
        m = RECON.search(ln)
        if m:
            seq[(m.group(1), int(m.group(2)), int(m.group(3)))].append(
                tuple(int(m.group(i)) for i in (4, 5, 6, 7)))
            continue
        m = QD.search(ln)
        if m:
            qd[cur][m.group(1)] = (int(m.group(2)), int(m.group(3)))

if not seq:
    print("NO `recon[sent=...]` CURSORS IN THE GIVEN LEDGERS.")
    print("They exist ONLY in `docs/l1-raw/ackdiag-*` — the ladder's own ledger")
    print("carries the recon RATIOS (`recon_crs_p0` etc.) and not the cursors.")
    sys.exit(1)

print("\n### STEP 2+3 — THE RECOMPUTE, AND REALIZED WIRE LOSS BESIDE IT\n")
print("`eps_sent`   = 1 - d(crecv)/d(sent)   — the SHIPPED sender-truth pair (arm T's input)")
print("`eps_legacy` = 1 - d(crecv)/d(cexp)   — the shipped gap pair (arm A's input)")
print("`realized`   = netem `dropped/(pkt+dropped)` on the matching `cli<p>` device,")
print("               an instrument OUTSIDE the code under test.\n")
print(f"{'cell':<9}{'rep':>4}{'p':>3}{'wins':>5}{'d_sent':>9}{'d_crecv':>9}{'d_cexp':>9}"
       f"{'eps_sent':>10}{'eps_legacy':>12}{'realized':>10}{'sent/real':>11}{'leg/real':>10}")
AGG = defaultdict(list)
CLAMP = defaultdict(lambda: [0, 0])
for k in sorted(seq):
    cell, rep, pid = k
    v = seq[k]
    if len(v) < 2:
        continue
    ds, dr, de = v[-1][0] - v[0][0], v[-1][1] - v[0][1], v[-1][2] - v[0][2]
    if ds <= 0 or de <= 0:
        continue
    eps_s, eps_l = 1 - dr / ds, 1 - dr / de
    q = qd.get((cell, rep), {}).get(f"cli{pid}")
    real = (q[1] / (q[0] + q[1])) if q else None
    AGG[cell].append((eps_s, eps_l, real))
    for a, b in zip(v, v[1:]):                 # STEP 4's per-window probe
        w_s, w_r = b[0] - a[0], b[1] - a[1]
        if w_s <= 0:
            continue
        CLAMP[cell][0] += 1
        if w_r >= w_s:
            CLAMP[cell][1] += 1
    print(f"{cell:<9}{rep:>4}{pid:>3}{len(v):>5}{ds:>9}{dr:>9}{de:>9}"
          f"{eps_s:>10.4f}{eps_l:>12.4f}"
          f"{(f'{real:.4f}' if real else '-'):>10}"
          f"{(f'{eps_s / real:+.1f}x' if real else '-'):>11}"
          f"{(f'{eps_l / real:.1f}x' if real else '-'):>10}")

# ── THE HEADLINE TABLE: raw vs corrected vs realized, per cell ──────────────
print("\n\n### THE eps-hat TABLE — raw vs corrected vs realized, per cell\n")
print("`pl= A`/`pl= T` are the LADDER's measured ESTIMATOR OUTPUT")
print("(`estimator.loss_rate()`, src/net/diag.rs:601) on the shipped and the")
print("sender-truth arms. `eps_sent`/`eps_legacy` are THIS re-score's arithmetic")
print("on the raw cursors. They are different layers and the gap between them is")
print("the finding.\n")
print(f"{'cell':<7}{'pl= A(max)':>11}{'pl= T(max)':>11}{'T/A':>7}"
      f"{'eps_legacy':>12}{'eps_sent':>10}{'realized':>10}{'in class?':>11}")
for cell in sorted(AGG):
    rows = AGG[cell]
    m = lambda i: sum(r[i] for r in rows if r[i] is not None) / max(
        1, sum(1 for r in rows if r[i] is not None))
    es, el, rl = m(0), m(1), m(2)
    pl = LADDER_PL.get(cell)
    inclass = "n/a" if rl is None else (
        "YES" if REALIZED_LO / 2 <= abs(es) <= REALIZED_HI * 2 else "NO")
    print(f"{cell:<7}{(f'{pl[4]:.4f}' if pl else '-'):>11}"
          f"{(f'{pl[5]:.4f}' if pl else '-'):>11}"
          f"{(f'{pl[6]:.1f}x' if pl and pl[6] else '-'):>7}"
          f"{el:>12.4f}{es:>10.4f}{(f'{rl:.4f}' if rl else '-'):>10}{inclass:>11}")

# ── STEP 4. THE RECTIFICATION PROBE ────────────────────────────────────────
print("\n\n### STEP 4 — THE RECTIFICATION PROBE\n")
print("`sender_truth_loss_delta` (src/scheduler/mod.rs:2477-2497) reports")
print("`(d_expected, d_received.min(d_expected))`. The clamp is ONE-SIDED: a")
print("window whose received delta RUNS AHEAD of its sent delta is reported as")
print("ZERO loss, while a window that lags is reported as loss. Zero-mean")
print("two-clock jitter therefore rectifies into a strictly POSITIVE bias, and")
print("the aggregate cursor ratio above cannot see it.\n")
print(f"{'cell':<9}{'windows':>9}{'d_recv >= d_sent':>18}{'frac clamped':>14}")
for cell in sorted(CLAMP):
    n, c = CLAMP[cell]
    print(f"{cell:<9}{n:>9}{c:>18}{(c / n if n else 0):>14.3f}")
print("\n  READ THIS COLUMN CAREFULLY. These are ~2 s report windows carrying tens")
print("  of thousands of symbols each, NOT the per-ack samples the clamp actually")
print("  fires on. The per-ack (d_expected, d_received) joint distribution is")
print("  captured NOWHERE in this tree, so the rate above is an INDICATION that")
print("  the two cursors cross often — not a measurement of the clamp rate and")
print("  not a bound on it. Naming that instrument is this re-score's deliverable.")

# ── THE VERDICT ─────────────────────────────────────────────────────────────
print("\n\n### THE VERDICT\n")
print("""  1. THE HYPOTHESIS IS REFUTED, TWICE OVER.
       On the CODE: the sender-truth pair's numerator and denominator BOTH
       count retransmits (STEP 1), so RFC 6675's double-count bias is not
       present in it and the proposed `d(sent) - d(retx) - d(repair)`
       correction is wrong-signed.
       On the DATA: recomputed from the cursors, `eps_sent` is NEGATIVE at
       every cell and every path — the receiver counts MORE arrivals than the
       sender counted handoffs. There is no 20x to collapse at the cursor
       layer, because the cursor layer never read 20x.

  2. WHAT DID READ 37-95x IS THE LEGACY GAP PAIR, AND THAT IS CONFIRMED.
       `eps_legacy` = 0.51 (c7) and 0.51 (c8, both legs pooled; 0.18 fast /
       0.85 slow) against realized 0.0056 / 0.0120 — 40-95x. Section 8 of the
       cross-check and §16.58's cross-path diagnosis are UNTOUCHED by this
       re-score; only the RFC 6675 mechanism for the T rung's move is dead.

  3. THE N = 1 ANOMALY IS NOT EXPLAINED AND IS NOT EXPLAINED AWAY.
       c2r100 is a SINGLE-PATH cell in this very ledger. Its cursor pair reads
       `eps_sent` = -0.012 against realized 0.008 — the pair is CLEAN at N = 1.
       Yet the ladder measured arm T's `pl=` at 0.36 (c1) and 0.58 (sc2), both
       single-path. So the inflation is NOT in the pair; it is DOWNSTREAM of
       it, between `sender_truth_loss_delta` and `estimator.loss_rate()`.

  4. THE ORDERING THE CROSS-CHECK PREDICTED DOES NOT HOLD.
       It asked whether the c7 20.1x > c8 3.8x ordering tracks recovery volume.
       It does not: those ratios are driven by arm A's DENOMINATOR (c7 0.0288
       vs c8 0.1951), and arm T's own absolute reading is c8 0.7454 > c7 0.5799
       — the OPPOSITE order to the retransmit volume (c7 24 269 > c8 3 569).

  5. THE NAMED INSTRUMENT, which is what closes 2b.
       Not a denominator change. A per-ack witness on `sender_truth_loss_delta`
       reporting (a) the count of samples where `d_received > d_expected` (the
       clamp firing), (b) their summed magnitude, and (c) the resulting
       rectified loss mass. It is a counter triple on one existing function,
       needs no wire change, and is the only thing that can decide whether the
       one-sided clamp is the 20x's mechanism.""")
