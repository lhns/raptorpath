#!/usr/bin/env python3
"""LOCAL GATE for the estimator battery's parser and scorer, on a SYNTHETIC
ledger with a KNOWN answer — run BEFORE the VM, so the scoring chain does not
reach the launch step never having read a row in its life.

WHY THIS FILE. The unit under test on the VM is an ESTIMATOR, and every clause
of the acceptance bar is arithmetic on rows this chain produces. The failure
mode that costs the most is the silent one: a warm-up exclusion that excludes
nothing, a `-` parsed as a zero, a leg quietly dropped because its regex missed,
a `p05` of 0 producing an `R_total` of infinity that prints as a pass. None of
those makes a malformed report. They make a WELL-FORMED WRONG ONE, which is
exactly the class of defect this tree has been bitten by (the `pid < 2` guard;
`retx=` off the last line; the σ column that looked converged at n = 18 000).

WHAT IS ASSERTED, and why each is absolute rather than ordinal:

  1. **THE WARM-UP EXCLUSION ACTUALLY EXCLUDES.** A ledger carrying a wild
     value at `n` below the class bar and tame values above it must score the
     TAME one. Clause `C1` is the only thing standing between the bar and a
     seed artefact, and an exclusion that silently matched everything would
     make every `R_total` in the pass wrong in the same direction.
  2. **`-` IS NOT A ZERO.** A gauge that has not fired renders `-`; a zero
     `p05` would make `R_total` infinite. Both are asserted to be absent from
     the reads.
  3. **`R_total` IS THE POOLED QUANTILE AND NOT `sup/inf`.** They are computed
     from the same synthetic sample and must DISAGREE — the bar rejected
     `sup/inf` on the ground that it grows with rep count, so a chain in which
     the two agree is one that did not implement the distinction.
  4. **THE THIN-LEG GATE FIRES BELOW %d READINGS**, and the derivation is
     checked as arithmetic: at `n < %d` nearest-rank `p05`/`p95` ARE `min`/`max`.
  5. **EVERY LEG IS READ.** Two sites x two paths from a ledger containing
     four; a chain that read only the sender, or only p0, would report half the
     legs and say nothing about it.
  6. **THE PROBE FUNCTIONALS ARE THE CANDIDATES' OWN**, checked against
     hand-computed values on a hand-written ping file — including that `msd` is
     computed in ARRIVAL ORDER (a sort would destroy it entirely and still
     return a plausible number).
  7. **THE BAR'S CONSTANTS ARE THE COMMITTED ONES.** Transcription is the one
     error a test can catch for free.

  usage: python3 test_sigb_report.py     (exit 0 = green)
"""
import io
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import sigb_probe                     # noqa: E402
import sigb_report as R               # noqa: E402

FAILS = []


def check(name, cond, detail=""):
    if cond:
        print("  ok   %s" % name)
    else:
        print("  FAIL %s %s" % (name, detail))
        FAILS.append(name)


# ── 7. THE BAR'S CONSTANTS ────────────────────────────────────────────────
print("\n[7] the bar's constants, transcribed")
check("accept bar 6.0", R.ACCEPT_BAR == 6.0)
check("prefer bar 3.5", R.PREFER_BAR == 3.5)
check("B accept band [0.68, 1.47]",
      (R.B_ACCEPT_LO, R.B_ACCEPT_HI) == (0.68, 1.47))
check("B reject band [0.50, 2.00]",
      (R.B_REJECT_LO, R.B_REJECT_HI) == (0.50, 2.00))
check("C2 fraction 5%", R.C2_FRAC == 0.05)
check("window L = 256", R.WINDOW_L == 256)
check("n_warm EWMA 16 / window 256 / msd 255",
      R.N_WARM == {"sig": 16, "rvar": 16, "qsp": 256, "msd": 255})
# §4's band-width rule is the band itself, not a new number.
check("B band width = 1.47/0.68",
      abs(R.B_BAND_WIDTH - 1.47 / 0.68) < 1e-12)
# The functional map is clause B §4's, LITERALLY: rvar is a moment-class
# candidate and reads `sd`, not a mean-absolute-deviation of convenience.
check("functional map: qsp->qsp, msd->msd, sig->sd, rvar->sd",
      R.PROBE_FUNC == {"sig": "sd", "rvar": "sd", "qsp": "qsp", "msd": "msd"})

# ── 4. THE THIN-LEG GATE'S DERIVATION AS ARITHMETIC ───────────────────────
print("\n[4] the thin-leg gate, derived rather than chosen")
bad = [n for n in range(2, R.MIN_READS)
       if not (int(0.05 * n) == 0 and int(0.95 * n) == n - 1)]
check("below MIN_READS the quantiles ARE the range", not bad,
      "counterexamples: %s" % bad[:5])
n = R.MIN_READS
check("at MIN_READS both indices are interior",
      int(0.05 * n) == 1 and int(0.95 * n) == n - 1,
      "got %d and %d" % (int(0.05 * n), int(0.95 * n)))

# ── 6. THE PROBE FUNCTIONALS ──────────────────────────────────────────────
print("\n[6] the probe functionals are the candidates' own")
PING = "\n".join(
    # icmp_seq 5 is MISSING on purpose: a censored probe, so `msd_all` has a
    # gap-straddling pair and `msd_adj` does not.
    ["[16.0] 64 bytes from 10.77.0.2: icmp_seq=%d ttl=64 time=%s ms" % (s, t)
     for s, t in ((1, "10.0"), (2, "12.0"), (3, "11.0"), (4, "20.0"),
                  (6, "10.0"), (7, "13.0"))]
    + ["7 packets transmitted, 6 received, 14% packet loss, time 350ms"])
with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as fh:
    fh.write(PING + "\n")
    PPATH = fh.name
st = sigb_probe.probe_functionals(PPATH, leg=0)
# us, in ARRIVAL order: 10000 12000 11000 20000 10000 13000
check("6 samples read, in us", st["n_samples"] == 6 and st["sd"] is not None)
check("sent from the summary, censor = 1/7", st["sent"] == 7
      and abs(st["censor_frac"] - round(1 / 7, 4)) < 1e-9,
      "sent=%s censor=%s" % (st["sent"], st["censor_frac"]))
# nearest-rank on 6 sorted: [10000,10000,11000,12000,13000,20000]
#   P50 -> index int(0.5*6)=3 -> 12000 ; P90 -> int(0.9*6)=5 -> 20000
check("qsp = P90 - P50 = 8000 us", st["qsp"] == 8000.0, "got %s" % st["qsp"])
# |d| in arrival order: 2000 1000 9000 10000 3000 -> sorted
#   [1000,2000,3000,9000,10000]; P50 -> int(0.5*5)=2 -> 3000
check("msd (arrival order, gaps included) = 3000 us", st["msd"] == 3000.0,
      "got %s" % st["msd"])
# adjacent-seq only drops the 4->6 pair: |d| = 2000 1000 9000 3000 ->
#   sorted [1000,2000,3000,9000]; P50 -> int(0.5*4)=2 -> 3000
check("msd_adj drops the gap-straddling pair", st["msd_adj_pairs"] == 4
      and st["msd_pairs"] == 5, "%s/%s" % (st["msd_adj_pairs"], st["msd_pairs"]))
# A SORT would give |d| over sorted samples = 0,1000,1000,1000,7000 ->
#   median 1000. Assert we did NOT get that.
check("msd is NOT computed on a sorted stream", st["msd"] != 1000.0)
mu = (10000 + 12000 + 11000 + 20000 + 10000 + 13000) / 6.0
sd = (sum((x - mu) ** 2 for x in (10000, 12000, 11000, 20000, 10000, 13000))
      / 5.0) ** 0.5
check("sd is the n-1 sample standard deviation",
      abs(st["sd"] - round(sd, 3)) < 1e-6, "got %s want %s" % (st["sd"], sd))
check("P90 dies structurally at censor 14% > 10%", st["qsp_structural_dead"])
check("leg survives the 20% contract bar", not st["leg_unscoreable"])
os.unlink(PPATH)

# ── 1/2/3/5. THE LEDGER CHAIN ─────────────────────────────────────────────
print("\n[1,2,3,5] the ledger chain: exclusion, `-`, quantile vs range, legs")


def row(cell, seed, rep, site, pid, blk, sig, sign, rvar, rvarn, qsp, qspn,
        msd, msdn):
    return ("SIGBREAD %s %s %s %s p%d blk=%d sig=%s/%d rvar=%s/%d qsp=%s/%d "
            "msd=%s/%d" % (cell, seed, rep, site, pid, blk, sig, sign, rvar,
                           rvarn, qsp, qspn, msd, msdn))


led = []
# 30 TAME readings above every class bar, and 6 WILD ones BELOW the bar. If
# clause C1 is not applied, the wild ones enter and every R_total is wrong.
# Values chosen so the tame p05/p95 are exactly 1000 and 3000 -> R = 3.0.
tame = [1000] * 2 + list(range(1100, 1100 + 26)) + [3000] * 2   # 30 values
for site in ("cli", "srv"):
    for pid in (0, 1):
        b = 0
        for i, v in enumerate(tame):
            b += 1
            led.append(row("c8", "42", str(1 + i // 10), site, pid, b,
                           str(v), 5000, str(v), 5000, str(v), 300, str(v), 300))
        for j in range(6):
            b += 1
            # WILD, and below EVERY class bar: sig/rvar n=3 (<16), qsp n=10
            # (<256), msd n=10 (<255). Also one `-` per gauge.
            led.append(row("c8", "42", "9", site, pid, b,
                           "999999", 3, "-", 3, "999999", 10, "-", 10))
led.append('SIGBMETA c8 42 1 {"seconds": 2.25, "mbps": 88.0, "tc_bytes": 24000000, "tc_s": 5}')
led.append('SIGBWITNESS {"cell": "c8", "seed": 42, "rep": 1, "mbps": 88.0, '
           '"W1_rfa_gen": 0, "W2_pfrac_lines": 0, "W4_retx_max": 700, '
           '"W5_rack_fa": "164/694", "W7_group_misses_cli": 0, '
           '"W7_group_misses_srv": 0, "gen_plateau": false}')

with tempfile.NamedTemporaryFile("w", suffix=".log", delete=False) as fh:
    fh.write("\n".join(led) + "\n")
    LPATH = fh.name

L = R.Ledger()
L.load(LPATH)

check("[5] all four legs read (2 sites x 2 paths)",
      sorted(L.raw_n) == [("c8", "cli", 0), ("c8", "cli", 1),
                          ("c8", "srv", 0), ("c8", "srv", 1)],
      "got %s" % sorted(L.raw_n))

key = ("c8", "cli", 0)
for g in R.GAUGES:
    vals = [v for _, _, v in L.reads[key][g]]
    check("[1] %s: warm-up excluded the 6 sub-bar rows (30 kept)" % g,
          len(vals) == 30, "got %d" % len(vals))
    check("[1] %s: the wild sub-bar value did NOT enter" % g,
          999999 not in vals)
    check("[2] %s: `-` is not a zero" % g, 0 not in vals)

st = R.stats([v for _, _, v in L.reads[key]["sig"]])
check("[3] R_total = p95/p05 = 3000/1000 = 3.0", st["R_total"] == 3.0,
      "got %s (p05=%s p95=%s)" % (st["R_total"], st["p05"], st["p95"]))
check("[3] sup/inf is reported and EQUALS 3.0 here by construction",
      st["sup_inf"] == 3.0)
check("[3] the verdict is the PREFER tier at 3.0 <= 3.5",
      R.s_verdict(st) == "PASS-PREFER", R.s_verdict(st))
# And the distinction is real: a single outlier moves sup/inf and not p95/p05.
st2 = R.stats([v for _, _, v in L.reads[key]["sig"]] + [90000])
check("[3] one outlier moves sup/inf but NOT the quantile verdict",
      st2["sup_inf"] > 3.0 and st2["R_total"] <= R.ACCEPT_BAR,
      "sup/inf=%s R=%s" % (st2["sup_inf"], st2["R_total"]))

# The thin gate on a real short leg.
check("[4] a 5-reading leg is UNSCOREABLE-THIN, not a pass",
      R.s_verdict(R.stats([1, 2, 3, 4, 5])).startswith("UNSCOREABLE-THIN"))
check("[4] an empty leg is UNSCOREABLE-NO-SAMPLE, not a pass",
      R.s_verdict(R.stats([])) == "UNSCOREABLE-NO-SAMPLE")

# b_verdict never says "unbiased".
print("\n[B] clause B never acquits in words")
check("inside the band reads NOT-SHOWN-BIASED",
      R.b_verdict(1.0) == "NOT-SHOWN-BIASED")
check("outside [0.5, 2.0] reads REJECT",
      R.b_verdict(2.5) == "REJECT" and R.b_verdict(0.4) == "REJECT")
check("the carried-bias tier exists on both sides",
      R.b_verdict(0.6) == "ADMISSIBLE-BIAS-CARRIED"
      and R.b_verdict(1.8) == "ADMISSIBLE-BIAS-CARRIED")
check("no verdict string contains the word 'unbiased'",
      all("unbiased" not in R.b_verdict(x).lower()
          for x in (0.4, 0.6, 1.0, 1.8, 2.5)))

# The whole report must RUN on this ledger and must not print a verdict it
# cannot support.
print("\n[report] end to end")
rc = subprocess.run([sys.executable, os.path.join(HERE, "sigb_report.py"), LPATH],
                    capture_output=True, text=True)
check("sigb_report.py exits 0", rc.returncode == 0, rc.stderr[-400:])
out = rc.stdout
check("the report prints the abort block FIRST",
      out.index("## 1 —") < out.index("## 3 —") < out.index("## 5 —"))
check("clause B prints its 'CANNOT ACQUIT' warning",
      "CANNOT ACQUIT" in out)
check("the scope note is unconditional", "PLAIN-WINDOW SEAT" in out)

# THE VERDICT PATHS THE SYNTHETIC LEDGER EXERCISES, ASSERTED BY NAME.
# It carries NO SIGBPROBE row at all, so clause B was never evaluated for any
# gauge. An ACCEPT here would mean the scorer handed out a pass on two clauses
# out of three because the third produced no rows — the exact silent failure
# this file exists to make loud.
check("[verdict] no probe row anywhere => NOTHING reaches ACCEPT or PREFER",
      "==> ACCEPT" not in out and "==> PREFER" not in out,
      out[out.index("## 6 —"):][:900])
check("[verdict] sig/rvar land on ADMISSIBLE-ON-S, B-UNSCOREABLE",
      out.count("ADMISSIBLE-ON-S, B-UNSCOREABLE") >= 2)
# N = 5000 at every leg, so the C2 bar is 250. The EWMA class (16) clears it;
# BOTH window-class gauges (256 and 255) do NOT. That is arithmetic, it is
# checked here, and it exercises the REJECT-C path on real rows.
check("[verdict] the window-class pair is REJECT-C at a C2 bar of 250",
      out.count("REJECT-C") >= 2, out[out.index("## 6 —"):][:1200])
os.unlink(LPATH)

print("\n%s  (%d failure(s))"
      % ("GREEN" if not FAILS else "RED: " + ", ".join(FAILS), len(FAILS)))
sys.exit(1 if FAILS else 0)
