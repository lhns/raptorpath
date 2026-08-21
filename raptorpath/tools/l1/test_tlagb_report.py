#!/usr/bin/env python3
"""LOCAL GATE for the t-lag battery's parser and scorer, on SYNTHETIC ledgers
with KNOWN answers — run BEFORE the VM, so the scoring chain does not reach the
launch step never having read a row in its life.

  usage: python3 test_tlagb_report.py     (exit 0 = green)

WHY THIS FILE. The unit under test on the VM is an ESTIMATOR, and every clause
of the acceptance bar is arithmetic on rows this chain produces. The failure
mode that costs the most is the silent one: a warm-up exclusion that excludes
nothing, a `-` parsed as a zero, a leg quietly dropped because its regex missed,
a `p05` of 0 producing an `R_total` of infinity that prints as a pass. None of
those makes a malformed report. They make a WELL-FORMED WRONG ONE.

WHAT IS ASSERTED HERE THAT `test_sigb_report.py` DID NOT ASSERT, and why:

  A. **THE `tlag` FLOOR IS 32 PAIRS AND IT ACTUALLY EXCLUDES.** A ledger whose
     `tlag` readings all rest on fewer than `K = 32` pairs must produce NO
     pooled reading and must be reported `UNSCOREABLE-THIN-PAIRS` — not
     `UNSCOREABLE-NO-SAMPLE`, because a gauge that fired and was excluded is a
     different finding from a gauge that never fired, and not a score, because
     the whole point of the floor is that such a reading is not one.

  B. **THE REBUILT CLAUSE B CAN ACQUIT, AND beta IS EXACT.** A gauge whose
     ONLINE reading equals its OWN population functional over the SAME dumped
     samples must read beta = 1.0 and must ACQUIT. This is asserted END TO END
     — through a real `[RTTDUMP]` wire-format file, `tlagb_rttdump`'s parser,
     the ledger's per-leg τ, and the report's own arithmetic — because every
     one of those links is a place a leg can be paired with the wrong stream.

  C. **CONTROL-DRIFT SUPPRESSES THE `tlag` VERDICT.** The successor's claim is
     a comparison against four controls measured in the same run. A ledger in
     which a control has moved 3x must print `CONTROL-DRIFT` and must print NO
     verdict in the `tlag` column. This is the pre-registered consequence and
     an unenforced pre-registration is a sentence, not a rule.

  D. **A REPRODUCING LEDGER DOES NOT FIRE IT.** Asserted in the same breath as
     C, because a drift check that fires on everything is not a check.

And it carries forward, unchanged in substance, the assertions the sigma
battery's gate earned the hard way: the warm-up exclusion actually excludes;
`-` is not a zero; `R_total` is the pooled quantile and not `sup/inf`; the
thin-leg gate's derivation as arithmetic; every leg is read; the bar's
constants are the committed ones; and A VOIDED INVOCATION IS IN NO DENOMINATOR.
"""
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

# THE REPORT IS WRITTEN IN THE TREE'S OWN PROSE — it cites `§16.75`, prints
# `τ`, and uses em dashes, exactly as `sigb_report.py` does. On a console whose
# default encoding is not UTF-8 that is a `UnicodeEncodeError` inside the
# child, which would fail this gate for a reason that has nothing to do with
# the estimator. The ENCODING IS DECLARED HERE rather than the prose narrowed
# there: a scorer that had to drop the clause numbers to be testable would be
# a worse scorer.
ENV = dict(os.environ, PYTHONIOENCODING="utf-8")
try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except (AttributeError, ValueError):        # pragma: no cover
    pass

import tlagb_report as R              # noqa: E402
import tlagb_rttdump as D             # noqa: E402

FAILS = []
TMP = []


def check(name, cond, detail=""):
    if cond:
        print("  ok   %s" % name)
    else:
        print("  FAIL %s %s" % (name, detail))
        FAILS.append(name)


def tmpfile(text, suffix=".log"):
    with tempfile.NamedTemporaryFile("w", suffix=suffix, delete=False) as fh:
        fh.write(text)
        TMP.append(fh.name)
        return fh.name


def row(cell, seed, rep, site, pid, blk, rtp, g5):
    """One TLAGBREAD row. `g5` = ((v, n) x 5) in GAUGES order."""
    return ("TLAGBREAD %s %s %s %s p%d blk=%d rtp=%d "
            "sig=%s/%d rvar=%s/%d qsp=%s/%d msd=%s/%d tlag=%s/%d"
            % (cell, seed, rep, site, pid, blk, rtp,
               g5[0][0], g5[0][1], g5[1][0], g5[1][1], g5[2][0], g5[2][1],
               g5[3][0], g5[3][1], g5[4][0], g5[4][1]))


def run(script, *args):
    return subprocess.run([sys.executable, os.path.join(HERE, script)]
                          + list(args), capture_output=True, text=True,
                          encoding="utf-8", errors="replace", env=ENV)


def run_report(*args):
    return run("tlagb_report.py", *args)


# ── THE BAR'S CONSTANTS ───────────────────────────────────────────────────
print("\n[bar] the constants, transcribed")
check("accept bar 6.0", R.ACCEPT_BAR == 6.0)
check("prefer bar 3.5", R.PREFER_BAR == 3.5)
check("B accept band [0.68, 1.47] — UNCHANGED by the reference swap",
      (R.B_ACCEPT_LO, R.B_ACCEPT_HI) == (0.68, 1.47))
check("B reject band [0.50, 2.00]",
      (R.B_REJECT_LO, R.B_REJECT_HI) == (0.50, 2.00))
check("C2 fraction 5%", R.C2_FRAC == 0.05)
check("window L = 256", R.WINDOW_L == 256)
check("tlag floor K = L/8 = 32 PAIRS (paper 16.75.6 F1)", R.TLAG_K == 32)
check("n_warm EWMA 16 / window 256 / msd 255 / tlag 32",
      R.N_WARM == {"sig": 16, "rvar": 16, "qsp": 256, "msd": 255, "tlag": 32})
check("tlag's n counts PAIRS, msd's counts DIFFERENCES",
      R.UNIT["tlag"] == "pairs" and R.UNIT["msd"] == "differences")
check("B band width = 1.47/0.68", abs(R.B_BAND_WIDTH - 1.47 / 0.68) < 1e-12)
check("the SCORED map is each gauge's OWN functional",
      R.POP_FUNC == {"sig": "sd", "rvar": "mad", "qsp": "qsp", "msd": "msd",
                     "tlag": "tlag"})
check("the scored map is `tlagb_rttdump`'s own, not a second copy",
      R.POP_FUNC == D.POP_FUNC, "%s vs %s" % (R.POP_FUNC, D.POP_FUNC))
check("the SUPERSEDED map keeps the 20 Hz probe's literal rvar->sd, and has",
      R.PROBE_FUNC["rvar"] == "sd" and R.PROBE_FUNC["tlag"] is None,
      "no tlag entry (the probe never computed that functional)")
check("the controls are the four the previous battery scored",
      R.CONTROLS == ("sig", "rvar", "qsp", "msd"))
check("the committed control values are transcribed, worst leg",
      R.CONTROL_R_WORST == {"sig": 256.3, "rvar": 351.3, "qsp": 78.6,
                            "msd": 34.6})
check("the committed control values are transcribed, data path",
      R.CONTROL_R_DATAPATH == {"sig": 86.6, "rvar": 103.9, "qsp": 78.6,
                               "msd": 8.667})
check("the drift factor is 2x", R.CONTROL_DRIFT_X == 2.0)

# ── THE THIN-LEG GATE'S DERIVATION AS ARITHMETIC ──────────────────────────
print("\n[thin] the thin-leg gate, derived rather than chosen")
bad = [n for n in range(2, R.MIN_READS)
       if not (int(0.05 * n) == 0 and int(0.95 * n) == n - 1)]
check("below MIN_READS the quantiles ARE the range", not bad,
      "counterexamples: %s" % bad[:5])
n = R.MIN_READS
check("at MIN_READS both indices are interior",
      int(0.05 * n) == 1 and int(0.95 * n) == n - 1,
      "got %d and %d" % (int(0.05 * n), int(0.95 * n)))
check("a 5-reading leg is UNSCOREABLE-THIN, not a pass",
      R.s_verdict(R.stats([1, 2, 3, 4, 5])).startswith("UNSCOREABLE-THIN"))
check("an empty leg with no rows at all is UNSCOREABLE-NO-SAMPLE",
      R.s_verdict(R.stats([])) == "UNSCOREABLE-NO-SAMPLE")

# ── THE VERDICT VOCABULARY ────────────────────────────────────────────────
print("\n[B] the rebuilt clause B ACQUITS; the superseded one still cannot")
check("inside the band, the REBUILT B acquits", R.b_verdict(1.0) == "ACQUIT")
check("outside [0.5, 2.0] the rebuilt B rejects",
      R.b_verdict(2.5) == "REJECT" and R.b_verdict(0.4) == "REJECT")
check("the carried-bias tier survives on both sides",
      R.b_verdict(0.6) == "ADMISSIBLE-BIAS-CARRIED"
      and R.b_verdict(1.8) == "ADMISSIBLE-BIAS-CARRIED")
check("the SUPERSEDED verdict function is unchanged and never says 'unbiased'",
      R.b_verdict_probe(1.0) == "NOT-SHOWN-BIASED"
      and all("unbiased" not in R.b_verdict_probe(x).lower()
              for x in (0.4, 0.6, 1.0, 1.8, 2.5)))

# ── 1/2/3/5. THE LEDGER CHAIN ─────────────────────────────────────────────
print("\n[chain] exclusion, `-`, quantile vs range, every leg, the tlag floor")
led = []
# 30 TAME readings above every class bar, and 6 WILD ones BELOW every bar. If
# clause C1 is not applied, the wild ones enter and every R_total is wrong.
# Values chosen so the tame p05/p95 are exactly 1000 and 3000 -> R = 3.0.
tame = [1000] * 2 + list(range(1100, 1100 + 26)) + [3000] * 2   # 30 values
for site in ("cli", "srv"):
    for pid in (0, 1):
        b = 0
        for i, v in enumerate(tame):
            b += 1
            led.append(row("c8", "42", str(1 + i // 10), site, pid, b, 38,
                           ((v, 5000), (v, 5000), (v, 300), (v, 300),
                            (v, 64))))
        for _ in range(6):
            b += 1
            # WILD, and below EVERY class bar: sig/rvar n=3 (<16), qsp n=10
            # (<256), msd n=10 (<255), tlag n=10 (<32). Also one `-` per pair.
            led.append(row("c8", "42", "9", site, pid, b, 38,
                           (("999999", 3), ("-", 3), ("999999", 10), ("-", 10),
                            ("999999", 10))))
led.append('TLAGBMETA c8 42 1 {"seconds": 2.25, "mbps": 88.0, '
           '"tc_bytes": 24000000, "tc_s": 5}')
led.append('TLAGBWITNESS {"cell": "c8", "seed": 42, "rep": 1, "mbps": 88.0, '
           '"W1_rfa_gen": 0, "W2_pfrac_lines": 0, "W4_retx_max": 700, '
           '"W5_rack_fa": "164/694", "W7_group_misses_cli": 0, '
           '"W7_group_misses_srv": 0, "gen_plateau": false}')
LPATH = tmpfile("\n".join(led) + "\n")

L = R.Ledger()
L.load(LPATH)

check("all four legs read (2 sites x 2 paths)",
      sorted(L.raw_n) == [("c8", "cli", 0), ("c8", "cli", 1),
                          ("c8", "srv", 0), ("c8", "srv", 1)],
      "got %s" % sorted(L.raw_n))
key = ("c8", "cli", 0)
for g in R.GAUGES:
    vals = [v for _, _, v in L.reads[key][g]]
    check("%s: the warm-up floor excluded the 6 sub-bar rows (30 kept)" % g,
          len(vals) == 30, "got %d" % len(vals))
    check("%s: the wild sub-bar value did NOT enter" % g, 999999 not in vals)
    check("%s: `-` is not a zero" % g, 0 not in vals)
check("the block's own rtp<>ms is carried per row, all 36 of them",
      len(L.rtp[key]) == 36 and {v for _, _, v in L.rtp[key]} == {38},
      str(L.rtp[key][:3]))

st = R.stats([v for _, _, v in L.reads[key]["sig"]])
check("R_total = p95/p05 = 3000/1000 = 3.0", st["R_total"] == 3.0,
      "got %s (p05=%s p95=%s)" % (st["R_total"], st["p05"], st["p95"]))
check("the verdict is the PREFER tier at 3.0 <= 3.5",
      R.s_verdict(st) == "PASS-PREFER", R.s_verdict(st))
st2 = R.stats([v for _, _, v in L.reads[key]["sig"]] + [90000])
check("one outlier moves sup/inf but NOT the quantile verdict",
      st2["sup_inf"] > 3.0 and st2["R_total"] <= R.ACCEPT_BAR,
      "sup/inf=%s R=%s" % (st2["sup_inf"], st2["R_total"]))

# ── A. THE tlag FLOOR: 32 PAIRS, AND A LEG OF NOTHING ELSE IS THIN ────────
print("\n[A] the tlag floor of 32 PAIRS excludes, and says which kind of empty")
thin = []
for b in range(1, 41):
    # `tlag` at n = 31 pairs — ONE BELOW the floor — carrying a wild value, on
    # a leg whose other four gauges are all healthy. If the floor is off by one
    # or reads the wrong column, the wild value enters and R_total is 999.
    thin.append(row("c7", "42", "1", "cli", 0, b, 12,
                    ((1000 + b, 5000), (1000 + b, 5000), (1000 + b, 4000),
                     (1000 + b, 4000), (999999, 31))))
TPATH = tmpfile("\n".join(thin) + "\n")
TL = R.Ledger()
TL.load(TPATH)
tkey = ("c7", "cli", 0)
check("every n=31 tlag reading is excluded (K = 32 PAIRS)",
      len(TL.reads[tkey]["tlag"]) == 0,
      str(TL.reads[tkey]["tlag"][:3]))
check("the four controls on the SAME rows are untouched",
      all(len(TL.reads[tkey][g]) == 40 for g in R.CONTROLS),
      str({g: len(TL.reads[tkey][g]) for g in R.CONTROLS}))
raw_ns = [n for _, _, n in TL.raw_n[tkey]["tlag"]]
v = R.s_verdict(R.stats([]), "tlag", raw_ns)
check("a leg of nothing but sub-K readings is UNSCOREABLE-THIN, not scored",
      v.startswith("UNSCOREABLE-THIN") and "PAIRS" in v, v)
check("and it is NOT reported as 'no sample' — the gauge fired, it was excluded",
      "NO-SAMPLE" not in v, v)
check("a leg with no rows at all still reports NO-SAMPLE",
      R.s_verdict(R.stats([]), "tlag", []) == "UNSCOREABLE-NO-SAMPLE")
rc = run_report(TPATH)
check("tlagb_report.py exits 0 on the thin ledger", rc.returncode == 0,
      rc.stderr[-500:])
check("the report prints UNSCOREABLE-THIN-PAIRS for that leg",
      "UNSCOREABLE-THIN-PAIRS" in rc.stdout,
      rc.stdout[-600:])
check("and the wild sub-K value reaches no statistic anywhere",
      "999999" not in rc.stdout)

# ── THE VOID RULE, CARRIED OVER ───────────────────────────────────────────
print("\n[void] a voided invocation is dropped from every container")
vled = []
for rep, val in ((1, 1000), (2, 999999)):        # rep 2 will be VOIDed
    for b in range(1, 31):
        vled.append(row("c7", "42", str(rep), "cli", 0, b, 30,
                        ((val, 5000), (val, 5000), (val, 300), (val, 300),
                         (val, 64))))
    vled.append('TLAGBPROBE c7 42 %d {"leg": 0, "n_samples": 40, "sent": 41, '
                '"recv": 40, "censor_frac": 0.02, "censor_pct": 2.0, '
                '"leg_unscoreable": false, "qsp": 100, "msd": 50, "sd": 200, '
                '"spacing_ms": 50.0, "qsp_structural_dead": false}' % rep)
    # Rep 2 is the plateau rep AND its W1 is DIRTY (gen=1), so it is a real
    # configuration fault and voids. Amendment §7 makes the WITNESS the void
    # cause, not the goodput band — the retention direction is pinned
    # separately below.
    vled.append('TLAGBWITNESS {"cell": "c7", "seed": 42, "rep": %d, '
                '"mbps": %s, "W1_rfa_gen": %d, "W2_pfrac_lines": 0, '
                '"W4_retx_max": 700, "W5_rack_fa": "1/694", '
                '"W7_group_misses_cli": 0, "W7_group_misses_srv": 0, '
                '"gen_plateau": %s}'
                % (rep, "161.0" if rep == 1 else "28.767",
                   0 if rep == 1 else 1,
                   "false" if rep == 1 else "true"))
VPATH = tmpfile("\n".join(vled) + "\n")
VL = R.Ledger()
VL.load(VPATH)
pre = len(VL.reads[("c7", "cli", 0)]["sig"])
pre_rtp = len(VL.rtp[("c7", "cli", 0)])
voided = VL.apply_voids()
post = [v for _, _, v in VL.reads[("c7", "cli", 0)]["sig"]]
check("the void set is built from the witness row, seed-tagged",
      voided == {("c7", "42", "2")}, str(voided))
check("the aborted rep's 30 readings are gone (60 -> 30)",
      pre == 60 and len(post) == 30, "pre=%d post=%d" % (pre, len(post)))
check("the aborted rep's VALUE is absent, so R_total is 1.0 not 1000",
      999999 not in post and R.stats(post)["R_total"] == 1.0,
      str(R.stats(post)))
check("its probe row is gone too (2 -> 1)",
      len(VL.probe[("c7", 0)]) == 1, str(len(VL.probe[("c7", 0)])))
check("its rtp rows are gone too (60 -> 30) — tau must not come from a void",
      pre_rtp == 60 and len(VL.rtp[("c7", "cli", 0)]) == 30,
      str(len(VL.rtp[("c7", "cli", 0)])))
check("a clean ledger voids nothing", R.Ledger().apply_voids() == set())
# THE DRIVER'S OWN WITNESS ROW VOIDS TOO. `TLAGBBAND` carries the same three
# keys and the same `gen_plateau` field as `TLAGBWITNESS`, and a rep aborted
# BEFORE the parser ran has only the driver's row — leaving it in the
# denominator is the very defect the void rule exists to close. It is still a
# WITNESS ROW and not marker text: it carries the seed, so it cannot void the
# innocent seed's rep the way a marker line matched on cell and rep would.
bled = [row("c1", "7", "1", "cli", 0, 1, 10,
            ((5, 5000), (5, 5000), (5, 300), (5, 300), (5, 64))),
        'TLAGBBAND {"cell":"c1","seed":7,"rep":1,"rc":0,"mbps":30.0,'
        '"band":[80,200],"in_band":0,"gen_plateau":1,"lossy":0}']
BL0 = R.Ledger()
BL0.load(tmpfile("\n".join(bled) + "\n"))
check("aborted before the parser ran still leaves no row in the denominator",
      BL0.apply_voids() == {("c1", "7", "1")}
      and BL0.reads[("c1", "cli", 0)]["sig"] == [],
      str(BL0.reads[("c1", "cli", 0)]["sig"]))
check("...and unknown witnesses are NOT treated as clean",
      BL0.plateau_retained == [], str(BL0.plateau_retained))

# ── THE WITNESS-FIRST PLATEAU RULE, THE RETENTION DIRECTION ──────────────
# Amendment §7, committed BEFORE the VM was touched. The previous battery
# hardened the goodput plateau into an unconditional abort, voided a rep on
# it, and then recorded in its own §2 that the hardening was WRONG: a goodput
# band cannot discriminate a configuration, because generation-on and "this
# rep lost badly and retransmitted hard" both land at ~30 Mbit/s, and only
# W1/W2 tell them apart. So a plateau reading with W1 gen=0 and zero [PFRAC]
# is generation-OFF BY DIRECT ENGINE ECHO and is a RESULT, retained.
#
# Retaining is also the honest direction for an ESTIMATOR battery: a plateau
# rep is a heavy-loss, heavy-retransmit rep, and a HIGH-DISPERSION rep is
# exactly the rep this battery must not discard. Voiding it would flatter
# every gauge's R_total, which is the direction a bar must never drift.
rled = [row("c7", "42", "1", "cli", 0, b, 30,
            ((7, 5000), (7, 5000), (7, 300), (7, 300), (7, 64)))
        for b in range(1, 4)]
rled.append('TLAGBBAND {"cell":"c7","seed":42,"rep":1,"rc":0,"mbps":33.291,'
            '"band":[140,180],"in_band":0,"gen_plateau":1,"lossy":1}')
rled.append('TLAGBWITNESS {"cell": "c7", "seed": 42, "rep": 1, '
            '"mbps": 33.291, "W1_rfa_gen": 0, "W2_pfrac_lines": 0, '
            '"W4_retx_max": 9000, "W5_rack_fa": "1/8900", '
            '"W7_group_misses_cli": 0, "W7_group_misses_srv": 0, '
            '"gen_plateau": true}')
RL = R.Ledger()
RL.load(tmpfile("\n".join(rled) + "\n"))
n_before = len(RL.reads[("c7", "cli", 0)]["sig"])
rvoid = RL.apply_voids()
check("a plateau reading with W1 gen=0 and zero PFRAC is NOT voided",
      rvoid == set(), str(rvoid))
check("its readings stay in the denominator",
      len(RL.reads[("c7", "cli", 0)]["sig"]) == n_before == 3,
      "before=%d after=%d" % (n_before,
                              len(RL.reads[("c7", "cli", 0)]["sig"])))
check("and it is RECORDED as a retention, with its acquitting witness",
      len(RL.plateau_retained) == 1
      and RL.plateau_retained[0][0] == ("c7", "42", "1")
      and RL.plateau_retained[0][2] == 0,
      str(RL.plateau_retained))
# The plateau flag reaches the report from the DRIVER's row while the
# witnesses reach it only from the PARSER's row, so the evidence must be
# MERGED across both kinds. A reader that looked at either row alone would
# void this rep (driver row: plateau, no witnesses) or miss the plateau
# entirely (parser row read without the band).
check("the rule merges the DRIVER's plateau flag with the PARSER's witnesses",
      RL.plateau_retained[0][1] == 33.291, str(RL.plateau_retained))

# ── B. THE REBUILT CLAUSE B, END TO END, THROUGH THE REAL WIRE FORMAT ─────
print("\n[B-e2e] beta = online / population, exact, ACQUITTING")
# A 1 kHz stream alternating 1000/2000 us. tau = 9 ms = 9 SPACINGS, an ODD
# number, so every admitted pair straddles the alternation and |diff| = 1000
# exactly. msd = 1000 and tlag = 1000 by construction, so a chain that paired
# the leg with the wrong stream, or read tau off the wrong leg, cannot land on
# 1.0 by luck.
NS, SPACING, TAU_MS = 300, 1000, 9
series = [(i * SPACING, 1000 if i % 2 == 0 else 2000) for i in range(NS)]
pop = D.population_functionals(series, TAU_MS * 1000)
check("the synthetic stream's population msd is exactly 1000 us",
      pop["msd"] == 1000.0, str(pop["msd"]))
check("its population tlag is exactly 1000 us over a non-empty band",
      pop["tlag"] == 1000.0 and pop["tlag_pairs"] > 0,
      "%s / %s pairs" % (pop["tlag"], pop["tlag_pairs"]))
check("its population mad is exactly 500 us", pop["mad"] == 500.0,
      str(pop["mad"]))

dump = []
for start in range(0, NS, 256):
    batch = series[start:start + 256]
    ents, prev = [], None
    for t, rtt in batch:
        ents.append("%d,%d" % (0 if prev is None else t - prev, rtt))
        prev = t
    dump.append("[RTTDUMP] p=0 t0=%d n=%d d=%s"
                % (batch[0][0], len(batch), ";".join(ents)))
DDIR = tempfile.mkdtemp()
# THE B PASS'S OWN CAPTURE NAME, transcribed from `tlagb_bpass.sh`:
# `${cell}-s${SEED}-r${REP}-c.log` for the sender, `-s.log` for the receiver,
# `.gz` after the post-run compression. Naming the fixture anything else would
# test a convention no driver produces.
check("the driver's capture name maps to a leg",
      R.map_dump_name("c7-s42-r1-c.log") == ("c7", "42", "1", "cli")
      and R.map_dump_name("c8L-s42-r3-s.log") == ("c8L", "42", "3", "srv"),
      str(R.map_dump_name("c7-s42-r1-c.log")))
check("a gzipped capture maps too — the B pass gzips every log after the run",
      R.map_dump_name("c1-s42-r1-c.log.gz") == ("c1", "42", "1", "cli"))
check("the B pass's OTHER captures map to NOTHING and are scored nowhere",
      R.map_dump_name("c7-s42-r1-abort.txt") is None
      and R.map_dump_name("bpass-era.txt") is None,
      str(R.map_dump_name("c7-s42-r1-abort.txt")))
DUMPF = os.path.join(DDIR, "c7-s42-r1-c.log")
with open(DUMPF, "w") as fh:
    fh.write("\n".join(dump) + "\n")

# The ONLINE readings, each set to its own population functional: sig=501
# (sd = 500.836...), rvar=500 (mad), msd=1000, tlag=1000.
bl = []
for b in range(1, 21):
    bl.append(row("c7", "42", "1", "cli", 0, b, TAU_MS,
                  ((501, 20000), (500, 20000), (1, 20000), (1000, 20000),
                   (1000, 200))))
BPATH = tmpfile("\n".join(bl) + "\n")
rc = run_report(BPATH, "--bpass", BPATH, "--dump", DDIR)
check("tlagb_report.py exits 0 with a dump", rc.returncode == 0,
      rc.stderr[-800:])
out = rc.stdout
check("clause B says, in the report itself, that it CAN ACQUIT",
      "CAN ACQUIT" in out)
check("the old REJECT-only language is gone", "CANNOT ACQUIT" not in out)
check("the old CONFOUNDED- marking on msd is gone from every verdict",
      re.search(r"^\s+msd\s+.*CONFOUNDED", out, re.M) is None
      and "MARKING ON `msd` IS GONE" in out)
check("the dump was mapped to the leg (no UNMAPPED-DUMP)",
      "UNMAPPED-DUMP" not in out)
check("tau came from the leg's own rtp rows, not a constant",
      re.search(r"tau=9000 us \(from invocation\)", out) is not None,
      out[out.index("## 5 "):][:400] if "## 5 " in out else out[-500:])
mline = re.search(r"^\s+msd\s+1000\s+msd\s+1000\.000\s+1\.0\s+ACQUIT",
                  out, re.M)
check("msd: online == its own population functional => beta 1.0, ACQUIT",
      mline is not None,
      out[out.index("## 5 "):][:1400] if "## 5 " in out else out[-800:])
tline = re.search(r"^\s+tlag\s+1000\s+tlag\s+1000\.000\s+1\.0\s+ACQUIT",
                  out, re.M)
check("tlag: same, through the tau band the ledger's own rtp selected",
      tline is not None,
      out[out.index("## 5 "):][:1400] if "## 5 " in out else out[-800:])
check("the superseded 20 Hz beta is printed and marked NOT READ / absent",
      "SUPERSEDED 20Hz beta" in out
      and "the 20 Hz probe computes no such functional" in out)
check("the cross-functional level table is printed and scored nowhere",
      "## 5b" in out and "SCORED NOWHERE" in out
      and "NO BAR IS APPLIED TO ANY COLUMN ABOVE" in out)
check("the level table carries all five functionals on the one stream",
      re.search(r"^\s+c7\s+cli\s+p0\s+300\s", out, re.M) is not None,
      out[out.index("## 5b"):][:700] if "## 5b" in out else "")
check("dump coverage is printed per leg, emitted vs the leg's own sig n",
      "## 5c" in out and re.search(r"^\s+c7\s+cli\s+p0\s+300\s+20000",
                                   out, re.M) is not None,
      out[out.index("## 5c"):][:600] if "## 5c" in out else "")
check("the narrowing is recorded: the delivered-latency probe is STILL MISSING",
      "STILL MISSING" in out)

# A CAPPED leg must be marked PREFIX-SCORED rather than scored silently.
CAPF = os.path.join(DDIR, "c7-s42-r2-c.log")
with open(CAPF, "w") as fh:
    fh.write("\n".join(dump) + "\n")
    fh.write("[RTTDUMP-CAP] p=0 emitted=300 seen=99999 (cap bound)\n")
bl2 = bl + [row("c7", "42", "2", "cli", 0, b, TAU_MS,
                ((501, 20000), (500, 20000), (1, 20000), (1000, 20000),
                 (1000, 200))) for b in range(1, 21)]
BPATH2 = tmpfile("\n".join(bl2) + "\n")
rc = run_report(BPATH2, "--bpass", BPATH2, "--dump", DDIR)
check("a capped leg is marked PREFIX-SCORED", rc.returncode == 0
      and "PREFIX-SCORED" in rc.stdout, rc.stderr[-400:])

# AND THE GZIPPED CAPTURE READS, because that is the form the dump directory
# is in from the moment the B pass finishes compressing it. A report that could
# only read `.log` would silently score nothing the day after the pass.
import gzip                            # noqa: E402
os.unlink(DUMPF)
os.unlink(CAPF)
GZF = os.path.join(DDIR, "c7-s42-r1-c.log.gz")
with gzip.open(GZF, "wt") as fh:
    fh.write("\n".join(dump) + "\n")
rc = run_report(BPATH, "--bpass", BPATH, "--dump", DDIR)
check("a .gz capture is read, not skipped",
      rc.returncode == 0
      and re.search(r"^\s+tlag\s+1000\s+tlag\s+1000\.000\s+1\.0\s+ACQUIT",
                    rc.stdout, re.M) is not None,
      rc.stderr[-400:] or rc.stdout[-600:])
os.unlink(GZF)
os.rmdir(DDIR)

# ── C/D. THE CONTROL REGRESSION CHECK ────────────────────────────────────
print("\n[C,D] CONTROL-DRIFT fires on a moved control, and not otherwise")


def control_ledger(cell, sig_worst_mult=1.0):
    """A ledger whose FOUR CONTROLS reproduce their committed readings exactly.

    Two legs: `p0` is the DATA PATH (the higher sig `n`, which is how the data
    path identifies itself in this tree) and carries the committed data-path
    R_total; `p1` carries the committed WORST-leg R_total. 20 readings per
    gauge per leg, built so nearest-rank p05 = 1000 and p95 = 1000*R exactly.
    `tlag` sits at R = 3.0 — inside the PREFER tier — so that a suppressed
    verdict is visibly a suppression and not a failure.
    """
    lines_ = []
    for pid, table, nsig in ((0, R.CONTROL_R_DATAPATH, 20000),
                             (1, R.CONTROL_R_WORST, 19000)):
        mult = {g: 1.0 for g in R.CONTROLS}
        if pid == 1:
            mult["sig"] = sig_worst_mult
        vals = {}
        for g in R.CONTROLS:
            top = int(round(1000 * table[g] * mult[g]))
            vals[g] = [1000] * 19 + [top]
        vals["tlag"] = [1000] * 19 + [3000]
        for b in range(20):
            lines_.append(row(cell, "42", "1", "cli", pid, b + 1, 25,
                              ((vals["sig"][b], nsig),
                               (vals["rvar"][b], nsig),
                               (vals["qsp"][b], nsig),
                               (vals["msd"][b], nsig),
                               (vals["tlag"][b], 200))))
    return "\n".join(lines_) + "\n"


CPATH = tmpfile(control_ledger("c8"))
rc = run_report(CPATH)
check("tlagb_report.py exits 0 on the reproducing ledger", rc.returncode == 0,
      rc.stderr[-500:])
ok_out = rc.stdout
check("[D] a reproducing ledger does NOT print CONTROL-DRIFT",
      "CONTROL-DRIFT" not in ok_out,
      ok_out[ok_out.index("## 3c"):][:1200] if "## 3c" in ok_out else "")
check("[D] and it says so: THE CONTROLS REPRODUCE",
      "THE CONTROLS REPRODUCE" in ok_out)
check("[D] the tlag verdict is READ when the controls reproduce",
      "TLAG-VERDICT-WITHHELD" not in ok_out)
check("[D] the four controls each read back at 1.00x on both domains",
      len(re.findall(r"\breproduces\b", ok_out)) == 4,
      ok_out[ok_out.index("## 3c"):][:1400] if "## 3c" in ok_out else "")

DPATH = tmpfile(control_ledger("c8", sig_worst_mult=3.0))
rc = run_report(DPATH)
check("tlagb_report.py exits 0 on the drifted ledger", rc.returncode == 0,
      rc.stderr[-500:])
dr_out = rc.stdout
check("[C] a control moved 3x prints CONTROL-DRIFT",
      "CONTROL-DRIFT" in dr_out,
      dr_out[dr_out.index("## 3c"):][:1200] if "## 3c" in dr_out else "")
check("[C] the drifted control is NAMED with its factor",
      re.search(r"sig: worst 3\.00x", dr_out) is not None,
      dr_out[dr_out.index("## 3c"):][:1400] if "## 3c" in dr_out else "")
check("[C] the pre-registered consequence is printed",
      "NO VERDICT IS READ" in dr_out)
check("[C] and it is APPLIED: the tlag column carries no verdict",
      "TLAG-VERDICT-WITHHELD-CONTROL-DRIFT" in dr_out,
      dr_out[dr_out.index("## 6 "):][:1600] if "## 6 " in dr_out else "")
tl_block = dr_out[dr_out.index("=== tlag"):] if "=== tlag" in dr_out else ""
tl_verdict = tl_block[:tl_block.index("\n\n")] if "\n\n" in tl_block else tl_block
check("[C] tlag does not reach ACCEPT or PREFER under drift",
      "==> ACCEPT" not in tl_verdict and "==> PREFER" not in tl_verdict,
      tl_verdict)
check("[C] the tie-break excludes tlag by pre-registration",
      "EXCLUDES `tlag` BY PRE-REGISTRATION" in dr_out)
check("[C] the controls' own statistics are STILL PRINTED under drift",
      "R_total" in dr_out and "768.900" in dr_out,
      "the drifted sig worst leg is 256.3 x 3 = 768.9 and must still print")

# ── THE PARSER ────────────────────────────────────────────────────────────
print("\n[parse] the parser reads the five-gauge group and the block's rtp")
import tlagb_parse as PZ              # noqa: E402

DIAGLINE = (
    "[DIAG] t=12.3 cw=1 fl=0 np=2 rtt=41000"
    " p0:infl=3/sinfl=0/bdp1(cap2) sout=1/2/b3 ln=0/0 khr=1.00/kraw=1"
    " btlbw=100 sr=100/g0d0 dr=100/a1s0g0d0 est=1 pl=0.0100 cmp=x"
    " rtt=41000/wrtt=40000/rtp38ms sig_us=1234/n5000 rvar_us=999/n5000"
    " qsp_us=-/n10 msd_us=77/n4999 tlag_us=88/n64 gapd=0/0 qcwnd=1 qce=0"
    " qlp=0/1 | ANCHOR sent=1 al=0 attr=0 nr=0 rej[iv=0 zr=0 al=0] gen=0"
    " fill=0"
    " p1:infl=3/sinfl=0/bdp1(cap2) sout=1/2/b3 ln=0/0 khr=1.00/kraw=1"
    " btlbw=100 sr=100/g0d0 dr=100/a1s0g0d0 est=1 pl=0.0100 cmp=x"
    " rtt=41000/wrtt=40000/rtp12ms sig_us=1/n16 rvar_us=2/n16"
    " qsp_us=3/n256 msd_us=4/n255 tlag_us=-/n0 gapd=0/0 qcwnd=1 qce=0"
    " qlp=0/1 | ANCHOR sent=1 al=0 attr=0 nr=0 rej[iv=0 zr=0 al=0] gen=0"
    " fill=0\n")
CLI = tmpfile(DIAGLINE, suffix=".txt")
SRV = tmpfile("", suffix=".txt")
pr = run("tlagb_parse.py", "c8", "42", "1", CLI, SRV, "-")
check("tlagb_parse.py exits 0", pr.returncode == 0, pr.stderr[-500:])
prows = [l for l in pr.stdout.splitlines() if l.startswith("TLAGBREAD")]
check("both path blocks produced a row", len(prows) == 2, str(prows))
check("the five gauges and the block's own rtp are on the row",
      prows[0].endswith("rtp=38 sig=1234/5000 rvar=999/5000 qsp=-/10 "
                        "msd=77/4999 tlag=88/64"), prows[0])
check("`-` is preserved as `-`, on tlag too",
      prows[1].endswith("tlag=-/0") and "qsp=-/10" in prows[0])
check("the two path blocks carry DIFFERENT rtp — tau is per LEG",
      "rtp=38" in prows[0] and "rtp=12" in prows[1])
check("W7 counts no group miss on a well-formed line",
      '"W7_group_misses_cli": 0' in pr.stdout)
check("the ledger is RAW: the parser applies NO tlag floor",
      "tlag=88/64" in prows[0] and '"tlag_thin_K": 32' in pr.stdout)
check("the parser declares the floor it did not enforce",
      PZ.TLAG_THIN_K == R.N_WARM["tlag"] == 32)
check("the parser's gauge tuple is the report's",
      PZ.GAUGES == R.GAUGES)
# A path entry MISSING the group is W7 breakage, never a silent skip.
BROKEN = DIAGLINE.replace(" tlag_us=88/n64", "")
CLI2 = tmpfile(BROKEN, suffix=".txt")
pr2 = run("tlagb_parse.py", "c8", "42", "1", CLI2, SRV, "-")
check("a path entry without the five-gauge group is counted as W7, not skipped",
      '"W7_group_misses_cli": 1' in pr2.stdout,
      pr2.stdout[-400:])

for p in TMP:
    try:
        os.unlink(p)
    except OSError:
        pass

print("\n%s  (%d failure(s))"
      % ("GREEN" if not FAILS else "RED: " + ", ".join(FAILS), len(FAILS)))
sys.exit(1 if FAILS else 0)
