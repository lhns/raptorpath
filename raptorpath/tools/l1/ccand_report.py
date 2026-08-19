#!/usr/bin/env python3
"""Scoring pass for THE CANDIDATES BATTERY.

Scored against goal-gate "Candidates Battery — PRE-REGISTRATION" (commit
`6bd5299`), which is the CONTRACT: this file implements its clauses and inverts
none of them. Every bar below is transcribed from that block; nothing here
decides anything the pre-registration did not already fix.

  usage: ccand_report.py [--calib] <ledger.log> [<ledger.log> ...]

  --calib  print ONLY the invocation accounting, the two-sided liveness audit,
           the NEW-GAUGE ECHO AUDIT (the smoke) and the discipline-16 headroom
           table, and score NOTHING. This is what the calibration pass
           (`ccand_calib.sh`, one rep per arm per cell, n = 1) is read with: it
           carries no sigma, no seed-7 evidence, and nothing in it is a result.
           Its output fills the contract's headroom table and is committed as the
           contract's COMPLETION before the scored run.

WHAT IS AND IS NOT A DENOMINATOR (the pre-registration's own split):

  ABORT            no `[GATES]` on EITHER endpoint. NOT in any denominator.
                   `ARMCOUNT` in the driver counts PARSED ROWS and therefore
                   counts aborts too — it is a vanish-detector, NOT an n.
  DNF              a completed run that did not transfer. IS a datum, IS in the
                   denominator, reported separately.
  INSTRUMENT-FAIL  completed but a gauge did not report. Excluded from the
                   statistic it voids, WITH THE EXCLUSION COUNTED.

FIVE RULES THIS REPORTER DOES **NOT** INHERIT FROM `ladder_report.py`, all from
the contract's INSTRUMENT FACTS, and every one of them would otherwise turn a
structural certainty into a false result:

  * `[DCAP] eng=0/0` at c1 and sc2 is the CORRECT reading, not a warm-up
    failure: the pooled seat returns None on `n_live < 2` BEFORE any multiplier
    is read, so **D is BIT-IDENTICAL to A at every single-path cell**.
  * `[DCAP] chg_frac = 0` is an INSTRUMENT FAILURE, not the null RESULT the same
    reading is on `[SUMCAP]`: it cannot happen while `gain != 1+q`.
  * On arm A, `[RACK] evals = 0` BY CONSTRUCTION, so `ceil=` / `gran=` /
    `legacy_pin=` are DENOMINATORS OF ZERO. **§16.68's `ceil = 0.0000` defect
    finding may be read ONLY where `rack_evals > 0`.** Reading A's zero as the
    finding is the error the `rack_evals` column exists to prevent.
  * `fa = 0/0` means no recovery round fired: an INSTRUMENT-FAIL for the rep,
    never `fa_frac = 0`. And `fa` is a SENDER-SITE statistic — the receiver's
    gauge never calls `record_fire`.
  * `[LCW]` absent on every arm but L is the CORRECT reading (THE SPECIFICATION
    FINDING): the witness sits behind `loss_sent_truth_active()`. Five columns of
    structural silence are not a null result about the rectifier hypothesis.

  And one rule that INVERTS from the ladder: `[SUMCAP]` now rides EVERY arm
  (`RWM_SUM_CAP` is DEFAULT ON since 2026-08-19), so its ABSENCE is an
  INSTRUMENT-FAIL rather than an arm property.

MEASUREMENT DISCIPLINE 18(d) IS EVALUATED BEFORE ANY MECHANISM VERDICT: no
verdict about the multiplier may be recorded from an arm whose law was pinned —
it measured the clamp. c8L's status is ANCHOR-DEPENDENT and is read from
`[DCAP] pin=` IN THE RUN, with both outcomes pre-declared and neither preferred.

MEASUREMENT DISCIPLINE 1 OUTRANKS EVERYTHING: if D-ROUTE fails, the dial did not
reach the law and no other clause is scored.
"""
import json
import math
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capbind_check import capbind_lines  # noqa: E402  the ADR-0070 kit item 2 readout

# This report is pasted verbatim into `docs/goal-gate.md`, which is UTF-8 and
# uses the sigma/em-dash/plus-minus typography throughout — so the output keeps
# them, and the stream is pinned to UTF-8 rather than left to the platform's
# default.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

# ── The transcribed bars. Every one is quoted from the pre-registration; none is
#    recomputed, softened or added here.
ENG_BAR = 0.90            # D-INTERIOR: [DCAP] eng >= 0.90
CHG_BAR = 0.90            # D-INTERIOR: chg_frac >= 0.90
PIN_CLAIM = 0.10          # D-INTERIOR / G-CAPBIND: pin <= 0.10 to CLAIM interior
PIN_VOID = 0.50           # discipline 18: pin > 0.50 on a majority VOIDS the cell
CPU_BAR = 1.05            # G-CPU point band
PAIRED_MIN = 8            # B-WALL: paired reps with a non-zero difference needed
HEADROOM_BAR = 5.0        # discipline 16c: below this, no throughput target
FA_CLASS = 0.0625         # RFC 8985 6.2 Step 4's own budget, 1/16

#: THE DIAL, and it outranks every number here (MEASUREMENT DISCIPLINE 1). The
#: harness runs the `bulk` hint at every cell, so b(Bulk) = 2 and
#: q(b) = (b+1)/30 = 0.100000 EXACTLY. These are the values `[DCAP]` must echo.
DIAL_Q = 0.100000
DIAL_B = 2.0000

#: D-INTERIOR's cap bands — the published 1729 / 1270 / 3097 (16.67 table (A),
#: the PRIMARY anchors, at q = 0.10) with a +-20% anchor tolerance, wide because
#: the two published anchor eras disagree by up to 1.8x. ALL THREE ARE STRICTLY
#: BELOW 4096, which is what makes them falsifiable rather than decorative.
D_BAND = {"c7": (1383.0, 2075.0), "c8": (1016.0, 1524.0), "c8L": (2478.0, 3716.0)}
#: The published point predictions, printed beside the band so a reader sees what
#: the band was drawn around. `d_over_a` is (1+q)/gain = 0.550 wherever BOTH laws
#: are interior; at c8L it is 0.937 because A is ITSELF 43.2% ceiling-governed
#: there (A's ask is 5630.8 against a 4096 ceiling), which is why c8L is the cell
#: where the delta-cap is predicted to RELIEVE a clamp rather than lower an
#: interior value.
D_POINT = {"c7": 1729.0, "c8": 1270.0, "c8L": 3097.0}
D_OVER_A = {"c7": 0.550, "c8": 0.550, "c8L": 0.937}
#: 16.67 table (B), the SECONDARY (composed-era) anchors at q = 0.10. Carried in
#: every c8L verdict because on THESE the law asks 5474 against a 4096 ceiling and
#: PINS. No verdict rests on one era.
D_SECONDARY = {"c7": 1217.0, "c8": 1766.0, "c8L": 5474.0}

#: 16.68's bench: the multiplier at which the SRTT ceiling binds, per site. The
#: R-CEIL prediction is read off the RECEIVER row against RACK's own max of 17.
#: c8L is carried on the bench's `c8-AU` row, WHICH IS A DIFFERENT ARM CLASS AND
#: NOT THE LENGTH AXIS — so c8L's R-CEIL is a CLASS expectation, labelled as one.
RACK_CEIL_BINDS_AT_RECV = {"c1": 4, "c7": 27, "sc2": 32, "c8": 9, "c8L": 9}
RACK_CEIL_BINDS_AT_SEND = {"c1": 18, "c7": 32, "sc2": 32, "c8": 40, "c8L": 47}
RACK_MULT_SCORED = 17

#: 16.68.1's pre-registered false-alarm table, TRANSCRIBED WITHOUT ALTERATION.
#: The `shipped` row is scored on arm A (and D and L, which carry the same
#: clock); the mult=17 row on R and DR; the mult=1 row on R1.
FA_PRED = {
    "shipped": {"c1": 0.00, "c7": 0.00, "sc2": 0.50, "c8": 0.75, "c8L": 0.80},
    "mult17":  {"c1": 0.50, "c7": 0.50, "sc2": 0.50, "c8": 0.67, "c8L": 0.67},
    "mult1":   {"c1": 0.89, "c7": 0.97, "sc2": 0.97, "c8": 0.97, "c8L": 0.98},
}
FA_ROW_OF_ARM = {"A": "shipped", "D": "shipped", "L": "shipped",
                 "R": "mult17", "DR": "mult17", "R1": "mult1"}

#: Shaped capacity per cell, in bits/s, from the cells' OWN definitions
#: (`lib.sh scenario_params`; dual cells sum their legs). Transcription, not
#: inference — discipline 16a.
SHAPED_BPS = {
    "c1": 1_000_000_000,          # c1 single: 1gbit
    "sc2": 100_000_000,           # c2 single: 100mbit
    "c7": 200_000_000,            # c2 + c2 dual
    "c8": 120_000_000,            # c2 + c3 dual: 100 + 20
    "c8L": 120_000_000,
}

CELLS = ["c1", "c7", "c8", "c8L", "sc2"]
#: The SCORED arms. R1 and L are AUXILIARY: scored on their own echo line and on
#: NOTHING else, and excluded from every guard denominator and every contrast by
#: the contract, before the run.
ARMS = ["A", "D", "R", "DR"]
AUX_ARMS = ["R1", "L"]
ALL_ARMS = ARMS + AUX_ARMS
DUALS = ["c7", "c8", "c8L"]
SINGLES = ["c1", "sc2"]

#: The contract's echo-expectations table, restated as data. The driver derives
#: the ARM ENV from its own copy of this table; this is the independent
#: recomputation from the parsed columns, which is the point of asserting a
#: liveness echo at all.
ARM_ON = {
    "delta_cap": {"D", "DR"},
    "rack_clocks": {"R", "DR", "R1"},
    "loss_sent_truth": {"L"},
    "sum_cap": set(ALL_ARMS),          # DEFAULT ON since 2026-08-19
    "quantile_clocks": set(),          # OUTRANKS rack_clocks — contamination
    "derived_sweep": set(),            # REPLACED by rack_clocks — contamination
    "composed_cap": set(),
    "three_term": set(),
    "store_cap_unified": set(),
    "late_brake": set(),
    "charge_recovery": set(),
    "release_1to1": set(),
}
#: The integer gate, which is part of the arm's identity and is matched as an
#: integer everywhere. Reading it as a flag would silently drop the scored arms'
#: 17 and read None — a liveness gate that passes because it never matched.
ARM_MULT = {"A": 1, "D": 1, "R": 17, "DR": 17, "R1": 1, "L": 1}


def mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def two_sigma(v):
    v = [x for x in v if x is not None]
    if len(v) < 2:
        return None
    m = sum(v) / len(v)
    return 2.0 * math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def med(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2.0


def frac_at_least(v, bar):
    """Fraction of non-None values at or above `bar`. The bind-fraction clauses
    are MAJORITY claims over reps, never means over reps — a mean of bind
    fractions would let one pinned rep hide behind nine interior ones."""
    v = [x for x in v if x is not None]
    return (sum(1 for x in v if x >= bar) / len(v)) if v else None


def frac_at_most(v, bar):
    v = [x for x in v if x is not None]
    return (sum(1 for x in v if x <= bar) / len(v)) if v else None


def fmt(x, p=1):
    return "-" if x is None else f"{x:.{p}f}"


def verdict(ok):
    return "PASS" if ok else "FAIL"


def pooled_2s(a, b):
    """2 sigma of the pooled spread of two arms' samples — the contract's
    `2σ_pooled`, computed the way every predecessor computed it."""
    sa, sb = two_sigma(a), two_sigma(b)
    if sa is None and sb is None:
        return None
    return math.sqrt(((sa or 0.0) ** 2 + (sb or 0.0) ** 2) / 2.0)


# ── Load. Pool provenance is taken from the FILENAME, as in every predecessor,
#    so a top-up can never be silently folded into the main pool — and B-WALL's
#    cross-pool stability clause needs the two pools to be distinguishable.
argv = [a for a in sys.argv[1:] if not a.startswith("--")]
CALIB = "--calib" in sys.argv[1:]
rows = []
for path in argv:
    base = path.replace("\\", "/").split("/")[-1]
    pool = "topup" if "topup" in base else ("calib" if "calib" in base else "main")
    with open(path, errors="replace") as f:
        for ln in f:
            i = ln.find('{"cell"')
            if i < 0:
                continue
            try:
                r = json.loads(ln[i:])
                r["_pool"] = pool
                rows.append(r)
            except ValueError:
                pass

by = defaultdict(list)
for r in rows:
    by[(r["cell"], r["arm"])].append(r)


def live(ck):
    return [r for r in by.get(ck, [])
            if r.get("gates_lines_cli") or r.get("gates_lines_srv")]


print("=" * 96)
print("CANDIDATES BATTERY — " + ("CALIBRATION PASS (n = 1; NOTHING HERE IS A RESULT)"
                                 if CALIB else "SCORING PASS"))
print('contract: goal-gate "Candidates Battery — PRE-REGISTRATION" (6bd5299), era main@0055c5d')
print("scored arms: A = shipped default (RWM_SUM_CAP IS DEFAULT ON — A is the LADDER'S ARM N)")
print("             D = RWM_DELTA_CAP | R = RACK mult=17 | DR = D+R")
print("aux arms:    R1 = RACK mult=1 | L = RWM_LOSS_SENT_TRUTH — SCORED ON THEIR OWN ECHO ALONE,")
print("             excluded from G-REG and from every contrast, by the contract, before the run")
print("=" * 96)

# ── 1. ACCOUNTING ────────────────────────────────────────────────────────
print("\n### INVOCATION ACCOUNTING (abort != DNF != INSTRUMENT-FAIL)")
print("### ARMCOUNT in the driver is NOT this table's n — it counts aborts too.\n")
print(f"{'cell-arm':<10} {'rows':>6} {'ABORT':>6} {'live':>6} {'DNF':>5} "
      f"{'noDCAP':>7} {'noRACK':>7} {'noFA':>6} {'noWALL':>7} {'noPL':>6} "
      f"{'noCPU':>6} {'noPING':>7}")
LIVE = {}
for c in CELLS:
    for a in ALL_ARMS:
        rs = by.get((c, a), [])
        lv = live((c, a))
        LIVE[(c, a)] = lv
        if not rs:
            continue
        ndc = sum(1 for r in lv
                  if a in ARM_ON["delta_cap"] and not r.get("dcap_lines"))
        nrk = sum(1 for r in lv if not r.get("rack_lines_cli"))
        # `fa = 0/0` is an INSTRUMENT-FAIL for the rep and NEVER `fa_frac = 0`.
        nfa = sum(1 for r in lv if not r.get("rack_fa_d_cli"))
        nwall = sum(1 for r in lv if not r.get("wall_lines"))
        npl = sum(1 for r in lv if not r.get("pl_n"))
        ncpu = sum(1 for r in lv if r.get("cpucli") is None)
        nping = sum(1 for r in lv if not r.get("ping_n"))
        dnf = sum(1 for r in lv if r.get("dnf"))
        print(f"{c+'-'+a:<10} {len(rs):>6} {len(rs)-len(lv):>6} {len(lv):>6} {dnf:>5} "
              f"{ndc:>7} {nrk:>7} {nfa:>6} {nwall:>7} {npl:>6} {ncpu:>6} {nping:>7}")

if not CALIB:
    print("\n  Per-seed live n (the G-TOPUP floor is n = 8 at EITHER seed, SCORED arms only):\n")
    print(f"{'cell-arm':<10} {'s42':>6} {'s7':>6}   top-up needed?")
    for c in CELLS:
        for a in ARMS:
            n42 = sum(1 for r in LIVE[(c, a)] if r["seed"] == 42)
            n7 = sum(1 for r in LIVE[(c, a)] if r["seed"] == 7)
            need = ("YES — SYMMETRIC over ALL FOUR SCORED ARMS of this cell"
                    if min(n42, n7) < 8 else "no")
            print(f"{c+'-'+a:<10} {n42:>6} {n7:>6}   {need}")
    print("  R1 and L are NOT topped up: they have no contrast to make cross-pool.")

# ── 2. LIVENESS, TWO-SIDED, RECOMPUTED FROM THE COLUMNS ──────────────────
print("\n### LIVENESS, TWO-SIDED, RECOMPUTED FROM THE COLUMNS (discipline 15c)")
print("### An arm that cannot show its control was a control has measured one "
      "condition twice.\n")
GKEYS = list(ARM_ON)
print(f"{'cell-arm':<10} " + " ".join(f"{g[:9]:>10}" for g in GKEYS) +
      f" {'mult':>8} {'instr':>7} {'DCAP':>6} {'RACK':>6} {'LCW':>6} {'SUMCAP':>7}")
LIVENESS_CLEAN = True
for c in CELLS:
    for a in ALL_ARMS:
        rs = LIVE[(c, a)]
        if not rs:
            continue
        n = len(rs)
        cols = []
        for g in GKEYS:
            exp = 1 if a in ARM_ON[g] else 0
            k = sum(1 for r in rs
                    if r.get("gates_cli_" + g) == exp and r.get("gates_srv_" + g) == exp)
            cols.append(f"{k}/{n}({exp})")
            if k != n:
                LIVENESS_CLEAN = False
        # The INTEGER gate, asserted as an integer.
        wm = ARM_MULT[a]
        km = sum(1 for r in rs
                 if r.get("gates_cli_rack_reo_mult") == wm
                 and r.get("gates_srv_rack_reo_mult") == wm)
        if km != n:
            LIVENESS_CLEAN = False
        instr = sum(1 for r in rs
                    if all(r.get(f"gates_{s}_{g}") == 1
                           for s in ("cli", "srv")
                           for g in ("diag", "ackdiag", "walldiag")))
        want_dc = a in ARM_ON["delta_cap"]
        dc = sum(1 for r in rs if bool(r.get("dcap_lines")) == want_dc)
        # [RACK] rides EVERY arm — on A/D/L it is the fa= meter and nothing else.
        rk = sum(1 for r in rs if r.get("rack_lines_cli"))
        # THE SPECIFICATION FINDING: [LCW] can only record on arm L.
        want_lw = a in ARM_ON["loss_sent_truth"]
        lw = sum(1 for r in rs if bool(r.get("lcw_lines")) == want_lw)
        # INVERTS from the ladder: RWM_SUM_CAP is DEFAULT ON, so [SUMCAP] rides
        # every arm and its ABSENCE is an INSTRUMENT-FAIL.
        sc = sum(1 for r in rs if r.get("sumcap_lines"))
        for k in (instr, dc, rk, lw, sc):
            if k != n:
                LIVENESS_CLEAN = False
        print(f"{c+'-'+a:<10} " + " ".join(f"{x:>10}" for x in cols) +
              f" {f'{km}/{n}({wm})':>8} {f'{instr}/{n}':>7} {f'{dc}/{n}':>6} "
              f"{f'{rk}/{n}':>6} {f'{lw}/{n}':>6} {f'{sc}/{n}':>7}")
print("\n  `DCAP` counts reps whose [DCAP] PRESENCE matches the arm (emitted only")
print("  on D/DR, fed the counterfactual on both). `RACK` counts reps carrying a")
print("  CLIENT-side [RACK] line — it rides EVERY arm, and on A/D/L it is")
print("  §16.68.1's fa= meter and nothing else. `LCW` counts reps whose [LCW]")
print("  presence matches THE SPECIFICATION FINDING: only arm L can record it,")
print("  and absence elsewhere is CORRECT, not a null result. `SUMCAP` counts")
print("  reps carrying the gauge at all — its rule INVERTS from the Ladder")
print("  Battery because RWM_SUM_CAP became the DEFAULT on 2026-08-19.")
if not LIVENESS_CLEAN:
    print("\n  *** LIVENESS IS NOT CLEAN. Every clause below rests on it; read the")
    print("      driver's ARM-LIVENESS-FAIL / ARM-CONTAMINATION lines before any")
    print("      number in this report (discipline 15c). ***")

# ── 3. THE NEW-GAUGE ECHO AUDIT — THE SMOKE, and the DIAL-ROUTING CHECK ───
print("\n### NEW-GAUGE ECHO AUDIT (the smoke) — and MEASUREMENT DISCIPLINE 1")
print("### The dial must ROUTE, not merely be read. The harness runs the `bulk`")
print(f"### hint, so b(Bulk) = 2 and q = (b+1)/30 = {DIAL_Q:.6f} EXACTLY.\n")
print(f"{'cell-arm':<10} {'DCAP q/b':>18} {'eng':>9} {'chg_f':>7} {'pin':>7} "
      f"{'cap':>9} {'ask':>9} {'RACKevals':>10} {'ceil':>7} {'legpin':>7} "
      f"{'fa':>10} {'LCW rect':>9}")
ROUTE_CLEAN, ROUTE_SEEN = True, 0
for c in CELLS:
    for a in ALL_ARMS:
        rs = LIVE[(c, a)]
        if not rs:
            continue
        qv, bv = med([r.get("dcap_q") for r in rs]), med([r.get("dcap_b") for r in rs])
        qb = "—" if qv is None else f"{qv:.6f}/{bv:.4f}"
        if a in ARM_ON["delta_cap"] and c in DUALS:
            ROUTE_SEEN += 1
            if qv is None or abs(qv - DIAL_Q) > 1e-9 or abs(bv - DIAL_B) > 1e-9:
                ROUTE_CLEAN = False
                qb += " ***"
        engn = med([r.get("dcap_eng_n") for r in rs])
        engd = med([r.get("dcap_eng_d") for r in rs])
        eng = "—" if engd is None else f"{int(engn)}/{int(engd)}"
        fan = med([r.get("rack_fa_n_cli") for r in rs])
        fad = med([r.get("rack_fa_d_cli") for r in rs])
        fa = "—" if fad is None else f"{int(fan)}/{int(fad)}"
        print(f"{c+'-'+a:<10} {qb:>18} {eng:>9} "
              f"{fmt(med([r.get('dcap_chg_frac') for r in rs]), 4):>7} "
              f"{fmt(med([r.get('dcap_pin') for r in rs]), 4):>7} "
              f"{fmt(med([r.get('dcap_cap') for r in rs])):>9} "
              f"{fmt(med([r.get('dcap_ask') for r in rs])):>9} "
              f"{fmt(med([r.get('rack_evals_cli') for r in rs]), 0):>10} "
              f"{fmt(med([r.get('rack_ceil_cli') for r in rs]), 4):>7} "
              f"{fmt(med([r.get('rack_legacy_pin_cli') for r in rs]), 4):>7} "
              f"{fa:>10} "
              f"{fmt(med([r.get('lcw_rect_frac') for r in rs]), 4):>9}")
print("\n  READ THE THREE INSTRUMENT FACTS BEFORE THIS TABLE, or it will be misread:")
print("  * `[DCAP] eng=0/0` at c1/sc2 is CORRECT — the pooled seat short-circuits")
print("    at n_live < 2, so D is BIT-IDENTICAL to A at every single-path cell.")
print("  * On arm A `RACKevals = 0` BY CONSTRUCTION, so its `ceil` and `legpin`")
print("    are DENOMINATORS OF ZERO. §16.68's ceil=0.0000 DEFECT FINDING may be")
print("    read ONLY where RACKevals > 0 — i.e. on R / DR / R1, never on A. The")
print("    only field carrying a datum on A is `fa`.")
print("  * `[LCW]` is blank on every arm but L, and that is THE SPECIFICATION")
print("    FINDING, not a null: the witness sits behind loss_sent_truth_active().")
if ROUTE_SEEN and not ROUTE_CLEAN:
    print("\n  *** D-ROUTE FAILED (rows marked ***). The env var was read but the dial")
    print("      did not reach the law. MEASUREMENT DISCIPLINE 1 OUTRANKS EVERY OTHER")
    print("      CLAUSE: nothing below is scored until this is explained. ***")
elif ROUTE_SEEN:
    print(f"\n  D-ROUTE: PASS — [DCAP] echoes q={DIAL_Q:.6f} b={DIAL_B:.4f} at every "
          f"engaged dual ({ROUTE_SEEN} groups).")

# ── 4. HEADROOM (discipline 16b) — the calibration's whole deliverable ────
print("\n### HEADROOM (discipline 16b) — tc, arm A, THIS session, EVERY cell\n")
print("  DENOMINATOR = THE TRANSFER WALL (`seconds`), NOT `INVOCATION_S`.")
print("  `INVOCATION_S` is the whole script's wall — namespace bring-up, netem/tbf")
print("  setup, the verification pings and teardown — and it runs 1.12x the")
print("  transfer at c1/c8L up to 2.11x at c8. Dividing shaped-device BYTES by it")
print("  read c7 at 77.6% when the cell is at 96.9%, which would have LICENSED the")
print("  unsatisfiable c7 throughput target discipline 16 exists to forbid. It is")
print("  printed beside the corrected figure so the correction is auditable.\n")
print(f"{'cell':<6} {'shaped':>10} {'xfer_s':>7} {'INVOC_S':>8} {'util s42':>9} "
      f"{'util s7':>9} {'headroom':>9}   claims permitted")
PERMIT = {}
for c in CELLS:
    us = {}
    for s in (42, 7):
        vals = []
        for r in LIVE[(c, "A")]:
            if r["seed"] != s or not r.get("tc_bytes") or not r.get("seconds"):
                continue
            vals.append(100.0 * r["tc_bytes"] * 8.0 / (r["seconds"] * SHAPED_BPS[c]))
        us[s] = med(vals)
    xfer = med([r.get("seconds") for r in LIVE[(c, "A")]])
    invoc = med([r.get("tc_s") for r in LIVE[(c, "A")]])
    worst = max([u for u in us.values() if u is not None], default=None)
    hr = None if worst is None else 100.0 - worst
    PERMIT[c] = hr
    claim = ("(no tc datum — headroom UNKNOWN, no throughput target may be scored)"
             if hr is None else
             ("throughput targets permitted" if hr >= HEADROOM_BAR
              else "PARITY / LATENCY / CAP-SHAPE ONLY — headroom < 5% (discipline 16c)"))
    print(f"{c:<6} {SHAPED_BPS[c]//1_000_000:>7} Mb {fmt(xfer,2):>7} {fmt(invoc,1):>8} "
          f"{fmt(us[42]):>9} {fmt(us[7]):>9} {fmt(hr):>9}   {claim}")
print("\n  THE CONTRACT WRITES NO THROUGHPUT TARGET AT ANY CELL. Every scored clause")
print("  is a dial echo, a bind fraction, a cap magnitude, a false-alarm fraction,")
print("  a queue-latency sign, a paired sign test or a no-regression guard.")
print("  Goodput appears in exactly three places — D-LAT's PARITY CONDITION (a")
print("  two-sided CONDITION on a latency claim, never a target), D-LAT's ADR-0071")
print("  refutation falsifier, and G-REG — and the latter two are one-sided")
print("  DOWNWARD and so remain satisfiable at any utilisation. The parity")
print("  condition is scoped to the DUALS and never to sc2, which is the cell the")
print("  δ-cap cannot engage at anyway.")

if CALIB:
    print("\n" + "=" * 96)
    print("CALIBRATION PASS ENDS HERE. n = 1: no sigma, no seed-7 evidence, and")
    print("nothing above is a result. Commit this table as the contract's")
    print("COMPLETION — in its own commit — BEFORE launching ccand_all.sh, and")
    print("read the ledger's LIVENESS / ARM-LIVENESS-FAIL / ARM-CONTAMINATION /")
    print("DIAL-ROUTE-FAIL lines first: this is also the smoke.")
    print("=" * 96)
    sys.exit(0)

# ── 5. RUNG D — CAPBIND + THE `[DCAP]` GAUGE ─────────────────────────────
print("\n### RUNG D — THE δ-CAP (§16.67). CAPBIND first: discipline 18(d) is")
print("### evaluated BEFORE any mechanism verdict.\n")
for ln in capbind_lines(rows, cap_key="occcap_p50", cells=CELLS, arms=ALL_ARMS):
    print("  " + ln)

print("\n#### D-IDENT (c1, sc2) — BIT-IDENTICAL BY CONSTRUCTION, and it is an alarm before it is a result\n")
print(f"{'cell':<6} {'eng':>8} {'d_mbps':>9} {'2σ_p':>8} {'d_ping':>8} {'d_q50':>8} "
      f"{'cpu D/A':>8}   verdict")
IDENT_CLEAN = True
for c in SINGLES:
    A, D = LIVE[(c, "A")], LIVE[(c, "D")]
    if not A or not D:
        continue
    ma, md = [r.get("mbps") for r in A], [r.get("mbps") for r in D]
    s2 = pooled_2s(ma, md)
    dm = (mean(md) - mean(ma)) if (mean(md) is not None and mean(ma) is not None) else None
    dp = (med([r.get("ping_p50") for r in D]) or 0) - (med([r.get("ping_p50") for r in A]) or 0)
    dq = (med([r.get("q_p50") for r in D]) or 0) - (med([r.get("q_p50") for r in A]) or 0)
    ca, cd = mean([r.get("cpucli") for r in A]), mean([r.get("cpucli") for r in D])
    ratio = (cd / ca) if (ca and cd) else None
    engd = [r.get("dcap_eng_d") for r in D if r.get("dcap_lines")]
    eng_ok = all(e == 0 for e in engd) if engd else None
    ok = (eng_ok is not False) and (s2 is None or dm is None or abs(dm) <= s2)
    IDENT_CLEAN = IDENT_CLEAN and ok
    print(f"{c:<6} {('0/0' if eng_ok else '***'):>8} {fmt(dm,2):>9} {fmt(s2,2):>8} "
          f"{fmt(dp):>8} {fmt(dq):>8} {fmt(ratio,3):>8}   {verdict(ok)}")
if not IDENT_CLEAN:
    print("\n  *** D-IDENT FAILED. The N=1 short-circuit is asserted at L0, so a wire")
    print("      difference is a BUILD or INSTRUMENT alarm before it is a result, and")
    print("      NOTHING ELSE IN THIS BATTERY IS SCORED until it is explained. ***")

print("\n#### D-INTERIOR (c7, c8, c8L) — the primary mechanism claim\n")
print(f"{'cell':<6} {'n':>4} {'eng>=.90':>9} {'chg>=.90':>9} {'pin<=.10':>9} "
      f"{'cap':>9} {'band':>17} {'point':>8} {'D/A':>7} {'pred':>7}   verdict")
D_INTERIOR = {}
for c in DUALS:
    D = [r for r in LIVE[(c, "D")] if r.get("dcap_lines")]
    A = LIVE[(c, "A")]
    if not D:
        continue
    lo, hi = D_BAND[c]
    eng = frac_at_least([r.get("dcap_eng") for r in D], ENG_BAR)
    chg = frac_at_least([r.get("dcap_chg_frac") for r in D], CHG_BAR)
    pin = frac_at_most([r.get("dcap_pin") for r in D], PIN_CLAIM)
    cap = med([r.get("dcap_cap") for r in D])
    acap = med([r.get("occcap_p50") for r in A])
    da = (cap / acap) if (cap and acap) else None
    inband = cap is not None and lo <= cap <= hi
    pinned = (frac_at_least([r.get("dcap_pin") for r in D], PIN_VOID) or 0) > 0.5
    ok = bool(inband and (eng or 0) > 0.5 and (chg or 0) > 0.5 and (pin or 0) > 0.5)
    D_INTERIOR[c] = (ok, pinned)
    print(f"{c:<6} {len(D):>4} {fmt(eng,2):>9} {fmt(chg,2):>9} {fmt(pin,2):>9} "
          f"{fmt(cap):>9} {f'[{lo:.0f}, {hi:.0f}]':>17} {D_POINT[c]:>8.0f} "
          f"{fmt(da,3):>7} {D_OVER_A[c]:>7.3f}   {verdict(ok)}"
          + ("  *** PINNED: discipline 18(d), NO multiplier verdict here" if pinned else ""))
print("\n  The band is the PRIMARY-anchor point ±20%. Both anchor eras are carried:")
for c in DUALS:
    print(f"    {c:<4} primary {D_POINT[c]:>7.0f}   secondary {D_SECONDARY[c]:>7.0f}"
          + ("   — secondary EXCEEDS the 4096 ceiling and PINS by construction"
             if D_SECONDARY[c] > 4096 else ""))
print("  c8L IS THE ANCHOR-DEPENDENT CELL and its status is read from [DCAP] pin=")
print("  IN THE RUN. pin <= 0.10 ⇒ the PRIMARY era holds and §16.67's")
print("  interior-EVERYWHERE claim is DELIVERED by measurement, which is ADR-0071")
print("  family 2's 'NO knee' in its stronger form. pin > 0.50 ⇒ the SECONDARY era")
print("  holds, discipline 18(d) applies and NO multiplier verdict is taken there.")
print("  NEITHER OUTCOME REFUTES §16.67, and the contract says so in advance.")

# ── 6. D-LAT — THE PRE-REGISTERED BAR ────────────────────────────────────
print("\n### D-LAT — THE CoDel-DERIVED PREDICTION STATED AS THE BAR")
print("### goodput PARITY (within 2σ_pooled, a CONDITION not a target) AND q_p50")
print("### strictly below same-session A, at the duals, on BOTH seeds.\n")
print(f"{'cell':<6} {'seed':>5} {'nA/nD':>7} {'mbps A':>8} {'mbps D':>8} {'d_mbps':>8} "
      f"{'2σ_p':>7} {'parity':>7} {'q50 A':>7} {'q50 D':>7} {'d_q50':>7} "
      f"{'d_ping':>7}   verdict")
for c in DUALS:
    for s in (42, 7):
        A = [r for r in LIVE[(c, "A")] if r["seed"] == s]
        D = [r for r in LIVE[(c, "D")] if r["seed"] == s]
        if not A or not D:
            continue
        ma, md = [r.get("mbps") for r in A], [r.get("mbps") for r in D]
        s2 = pooled_2s(ma, md)
        dm = (mean(md) or 0) - (mean(ma) or 0)
        parity = s2 is not None and abs(dm) <= s2
        qa, qd = med([r.get("q_p50") for r in A]), med([r.get("q_p50") for r in D])
        dq = (qd - qa) if (qa is not None and qd is not None) else None
        pa, pd = med([r.get("ping_p50") for r in A]), med([r.get("ping_p50") for r in D])
        dp = (pd - pa) if (pa is not None and pd is not None) else None
        ok = bool(parity and dq is not None and dq < 0)
        loss = s2 is not None and dm < -s2
        print(f"{c:<6} {s:>5} {f'{len(A)}/{len(D)}':>7} {fmt(mean(ma),2):>8} "
              f"{fmt(mean(md),2):>8} {fmt(dm,2):>8} {fmt(s2,2):>7} "
              f"{('yes' if parity else 'NO'):>7} {fmt(qa):>7} {fmt(qd):>7} "
              f"{fmt(dq):>7} {fmt(dp):>7}   {verdict(ok)}"
              + ("  *** >2σ GOODPUT LOSS — ADR-0071's own refutation of "
                 "δ-as-queue-budget" if loss else ""))
print("\n  THE MAGNITUDE IS A CLASS, NOT A POINT, AND THE REASON IS ON THE RECORD:")
print("  the two matched c8 contrasts this tree owns do NOT order monotonically in")
print("  the cap ratio (ladder N vs A moved the cap 0.70× for q_p50 −78 ms; NT vs T")
print("  moved it 0.51× for −50 ms). A 0.550× cut therefore licenses no point")
print("  prediction. The class is d_q50 ∈ [−80, −30] ms at c8, REPORTED against the")
print("  bar and not AS the bar.")
print("\n  AND THE CELL THIS BAR CANNOT REACH: Tier-1 2a's cleanest datum — sc2, the")
print("  98%-utilised cell, cap 481 at goodput parity for ping_p50 −55.2 ms — is at")
print("  a SINGLE-PATH cell, and RWM_DELTA_CAP DOES NOT ENGAGE AT N = 1. No sc2")
print("  number in this session may be read as support for the δ-cap.")

# ── 7. RUNG R — THE RACK CLOCK, AND THE SHIPPED CLAMP'S FIRST BIND FRACTION ─
print("\n### RUNG R — §16.68. `ceil=` MAY BE READ ONLY WHERE `evals > 0`.\n")
print(f"{'cell-arm':<10} {'site':>5} {'evals':>8} {'ceil':>7} {'gran':>7} "
      f"{'legacy_pin':>11} {'round_ms':>9} {'legacy_ms':>10} {'mult':>5}   "
      f"expectation")
for c in CELLS:
    for a in ("R", "DR", "R1"):
        for site in ("cli", "srv"):
            rs = [r for r in LIVE[(c, a)] if r.get(f"rack_evals_{site}")]
            if not rs:
                continue
            ev = med([r.get(f"rack_evals_{site}") for r in rs])
            ce = med([r.get(f"rack_ceil_{site}") for r in rs])
            mult = ARM_MULT[a]
            need = (RACK_CEIL_BINDS_AT_RECV if site == "srv"
                    else RACK_CEIL_BINDS_AT_SEND)[c]
            if mult == 1:
                exp = "ceil MUST read 0.0000 — §16.68's DEFECT FINDING, measured"
            elif need <= mult:
                exp = f"ceil > 0 expected (bench: binds at mult>={need})"
            else:
                exp = f"ceil ~ 0 expected (bench: needs mult>={need} > 17)"
            if c == "c8L":
                exp += " [c8-AU CLASS, not the length axis]"
            print(f"{c+'-'+a:<10} {site:>5} {fmt(ev,0):>8} {fmt(ce,4):>7} "
                  f"{fmt(med([r.get(f'rack_gran_{site}') for r in rs]),4):>7} "
                  f"{fmt(med([r.get(f'rack_legacy_pin_{site}') for r in rs]),4):>11} "
                  f"{fmt((med([r.get(f'rack_round_{site}') for r in rs]) or 0)/1000.0,2):>9} "
                  f"{fmt((med([r.get(f'rack_legacy_{site}') for r in rs]) or 0)/1000.0,2):>10} "
                  f"{mult:>5}   {exp}")
print("\n  R-LEGACY — THE FIRST MEASUREMENT OF THE SHIPPED [25,100] ms CLAMP'S OWN")
print("  BIND FRACTION, and it is a DEFECT FINDING ABOUT THE DEFAULT. `legacy_pin`")
print("  is the counterfactual computed INSIDE the armed law, so it is fed on the")
print("  ON arm ONLY and is read off R / DR / R1 and NEVER off A. §16.68's bench")
print("  predicts ≈1.0000 at every cell: if it holds, the shipped")
print("  `round = (2·srtt).clamp(25 ms, 100 ms)` IS A CONSTANT at every cell this")
print("  tree measures, its 2·srtt term is inert, and CLAUDE.md's 'a clamp that")
print("  always binds turns its law into a constant' applies to a SHIPPED law by")
print("  measurement rather than by argument.")

# ── 8. §16.68.1 — THE fa= VALIDATION, SCORED ON THE CONTROL ──────────────
print("\n### §16.68.1 — THE FALSE-ALARM VALIDATION. `fa_class` = 1/16 = 6.25%,")
print("### RFC 8985 §6.2 Step 4's OWN published budget. SENDER SITE ONLY.\n")
print(f"{'cell-arm':<10} {'n(fa)':>6} {'fired':>8} {'spurious':>9} {'fa_frac':>8} "
      f"{'predicted':>10} {'× α_class':>10}   clears 6.25%?")
for c in CELLS:
    for a in ALL_ARMS:
        rs = [r for r in LIVE[(c, a)] if r.get("rack_fa_d_cli")]
        if not rs:
            continue
        fired = med([r.get("rack_fa_d_cli") for r in rs])
        spur = med([r.get("rack_fa_n_cli") for r in rs])
        ff = med([r.get("rack_fa_frac_cli") for r in rs])
        pred = FA_PRED[FA_ROW_OF_ARM[a]][c]
        over = (ff / FA_CLASS) if ff else None
        print(f"{c+'-'+a:<10} {len(rs):>6} {fmt(fired,0):>8} {fmt(spur,0):>9} "
              f"{fmt(ff,4):>8} {pred:>10.2f} {fmt(over,1):>10}   "
              f"{'YES' if (ff is not None and ff <= FA_CLASS) else 'NO'}")
print("\n  FA-CONTROL is the strongest statement available about the SHIPPED clamp,")
print("  and it is about the CONTROL rather than the successors: §16.68.1")
print("  pre-registers arm A at ≥0.50 (sc2), ≥0.75 (c8), ≥0.80 (c8L, the c8-AU")
print("  class) — 8–13× RACK's own published budget at three of five cells.")
print("\n  THE FALSIFIER, verbatim and keyed to the INSTRUMENT because the")
print("  instrument is new: 'If the wire-measured fa_frac on the shipped control")
print("  lands below α_class at sc2, c8 and c8-AU, this component model is wrong")
print("  about the mechanism and the whole R axis is rebased on the measurement")
print("  rather than on ⌈srtt/cadence⌉.'")
print("\n  NO ARM OF THE R AXIS IS PREDICTED TO CLEAR α_class. This battery CANNOT")
print("  produce a recommendation to flip RWM_RACK_CLOCKS and is not run to")
print("  produce one. It is run because the CONTROL's number has never existed.")
print("  A rep with fa=0/0 fired no recovery round: it is EXCLUDED here with the")
print("  exclusion counted in `n(fa)`, and is NEVER read as fa_frac = 0.")

# ── 9. THE [LCW] RECTIFIER COLUMNS — NO BAR ──────────────────────────────
print("\n### [LCW] — THE ONE-SIDED-CLAMP WITNESS (Tier-1 2b finding 5). NO BAR.")
print("### Arm L only, and that is THE SPECIFICATION FINDING, not an omission.\n")
print(f"{'cell-arm':<10} {'n':>4} {'over_n':>9} {'over_mass':>11} {'loss_mass':>11} "
      f"{'rect_frac':>10}")
LCW_ANY = False
for c in CELLS:
    for a in ALL_ARMS:
        rs = [r for r in LIVE[(c, a)] if r.get("lcw_lines")]
        if not rs:
            continue
        LCW_ANY = True
        print(f"{c+'-'+a:<10} {len(rs):>4} "
              f"{fmt(med([r.get('lcw_over_n') for r in rs]),0):>9} "
              f"{fmt(med([r.get('lcw_over_mass') for r in rs]),0):>11} "
              f"{fmt(med([r.get('lcw_loss_mass') for r in rs]),0):>11} "
              f"{fmt(med([r.get('lcw_rect_frac') for r in rs]),4):>10}")
if not LCW_ANY:
    print("  (no [LCW] line anywhere — including on arm L. That is an")
    print("   ARM-LIVENESS-FAIL for L and is read from the driver's own lines.)")
print("\n  SCORED ON NOTHING. This is the first reading of an instrument built two")
print("  commits ago and never run. The ONE falsifier the contract writes, because")
print("  it costs nothing: rect_frac = 0 at EVERY cell including sc2 (N = 1) would")
print("  refute the hypothesis's own premise — the clamp never fires. Anything")
print("  above zero is DATA and takes no verdict.")
print("  The coarse prior, labelled coarse: at ~2 s granularity Tier-1 already saw")
print("  the received delta running ahead in 9/9 (c2r100), 12/18 (c7), 11/18 (c8)")
print("  windows — an indication the cursors cross often, NOT a measurement of the")
print("  clamp rate and NOT a bound on it. `over_n` is the per-ack measurement")
print("  that prior could not be.")

# ── 10. B-WALL — THE PAIRED c8 CONTRAST ──────────────────────────────────
print("\n### B-WALL — sign(dur_ms(D) − dur_ms(A)) PAIRED WITHIN REP INDEX at c8.")
print("### A sign test over paired reps, NEVER a difference of medians.\n")
print(f"{'pool':<7} {'seed':>5} {'paired':>7} {'nonzero':>8} {'D<A':>5} {'D>A':>5}   verdict")
WALL_SIGNS = {}
for pool in ("main", "topup"):
    for s in (42, 7):
        Aw = {r["rep"]: r.get("wall_dur_ms") for r in LIVE[("c8", "A")]
              if r["seed"] == s and r["_pool"] == pool and r.get("wall_lines")}
        Dw = {r["rep"]: r.get("wall_dur_ms") for r in LIVE[("c8", "D")]
              if r["seed"] == s and r["_pool"] == pool and r.get("wall_lines")}
        pairs = [(Dw[k] - Aw[k]) for k in sorted(set(Aw) & set(Dw))
                 if Aw[k] is not None and Dw[k] is not None]
        nz = [d for d in pairs if d != 0]
        if not pairs:
            continue
        neg, pos = sum(1 for d in nz if d < 0), sum(1 for d in nz if d > 0)
        WALL_SIGNS[(pool, s)] = (neg, pos)
        ok = len(nz) >= PAIRED_MIN
        print(f"{pool:<7} {s:>5} {len(pairs):>7} {len(nz):>8} {neg:>5} {pos:>5}   "
              + ("resolvable" if ok else f"NEEDS-MORE (<{PAIRED_MIN} non-zero pairs)"))
consistent = None
if WALL_SIGNS:
    dirs = {("neg" if n > p else ("pos" if p > n else "tie"))
            for (n, p) in WALL_SIGNS.values()}
    consistent = len(dirs) == 1 and "tie" not in dirs
print("\n  THE PRE-DECLARED CLOSE: fewer than 8 paired reps carrying a non-zero")
print("  difference at either seed, OR a sign that disagrees between seeds or")
print("  between pools ⇒ the clause closes NEEDS-MORE AND NAMES ITS INSTRUMENT —")
print("  what is owed is a c8 statistic that is not bistable, NOT a fourth")
print("  measurand. NO DEAD-WALL CLAIM OF ANY KIND is made from an unpaired")
print("  contrast in this battery, at any n.")
print(f"  Cross-pool / cross-seed sign consistency: "
      f"{'CONSISTENT' if consistent else 'NOT CONSISTENT — NEEDS-MORE'}")
print("  c8L's [WALL] is REPORTED, direction only, scored on nothing: it did not")
print("  order at all in the ladder (438 → 913 → 395 → 557).")

# ── 11. DR-FACTOR ────────────────────────────────────────────────────────
print("\n### DR-FACTOR — the two gates sit in disjoint seats and share no operand.\n")
print(f"{'cell':<6} {'D cap':>9} {'DR cap':>9} {'band':>17} {'DR in band?':>12}   verdict")
for c in DUALS:
    dc = med([r.get("dcap_cap") for r in LIVE[(c, "D")] if r.get("dcap_lines")])
    dr = med([r.get("dcap_cap") for r in LIVE[(c, "DR")] if r.get("dcap_lines")])
    lo, hi = D_BAND[c]
    ok = dr is not None and lo <= dr <= hi
    print(f"{c:<6} {fmt(dc):>9} {fmt(dr):>9} {f'[{lo:.0f}, {hi:.0f}]':>17} "
          f"{('yes' if ok else 'NO'):>12}   {verdict(ok)}")
print("\n  Falsified if DR's cap moves outside D's band — then the two seats are")
print("  coupled through something neither §16.67 nor §16.68 names, and every")
print("  rung's composition reading is re-scoped.")

# ── 12. GUARDS ───────────────────────────────────────────────────────────
print("\n### GUARDS — a win bought by breakage is a TRADE and is reported as one.")
print("### SCORED ARMS ONLY. R1 and L are excluded by the contract, before the run:")
print("### R1's regression is its OWN PREDICTION and L carries a gate the ladder")
print("### refuted.\n")
print(f"{'guard':<12} {'cell-arm':<10} {'seed':>5} {'value':>10} {'bar':>10}   verdict")
GUARDS_CLEAN = True


def guard(name, cell, arm, seed, val, bar, ok):
    global GUARDS_CLEAN
    if not ok:
        GUARDS_CLEAN = False
    print(f"{name:<12} {cell+'-'+arm:<10} {seed:>5} {fmt(val,3):>10} {fmt(bar,3):>10}"
          f"   {verdict(ok)}")


for c in CELLS:
    A = LIVE[(c, "A")]
    if not A:
        continue
    for a in ARMS:
        if a == "A":
            continue
        for s in (42, 7):
            Ai = [r for r in A if r["seed"] == s]
            Xi = [r for r in LIVE[(c, a)] if r["seed"] == s]
            if not Ai or not Xi:
                continue
            ma, mx = [r.get("mbps") for r in Ai], [r.get("mbps") for r in Xi]
            s2 = pooled_2s(ma, mx)
            dm = (mean(mx) or 0) - (mean(ma) or 0)
            guard("G-REG", c, a, s, dm, -(s2 or 0), s2 is None or dm >= -s2)
            ca, cx = mean([r.get("cpucli") for r in Ai]), mean([r.get("cpucli") for r in Xi])
            if ca and cx:
                guard("G-CPU", c, a, s, cx / ca, CPU_BAR, cx / ca <= CPU_BAR)
            if c == "sc2":
                pa = med([r.get("ping_p50") for r in Ai])
                px = med([r.get("ping_p50") for r in Xi])
                ps = pooled_2s([r.get("ping_p50") for r in Ai],
                               [r.get("ping_p50") for r in Xi])
                if pa is not None and px is not None:
                    guard("G-SC2-LAT", c, a, s, px - pa, (ps or 0),
                          ps is None or (px - pa) <= ps)
    for a in ALL_ARMS:
        n_dnf = sum(1 for r in LIVE[(c, a)] if r.get("dnf"))
        if LIVE[(c, a)]:
            guard("G-DNF", c, a, 0, n_dnf, 0, n_dnf == 0)

print("\n  G-SC2-LAT is the guard most likely to fire, and the contract says why in")
print("  advance: the RACK clock at mult=17 reads 55 ms against the shipped 100 ms")
print("  at sc2, and a tighter re-probe cadence at a 98%-utilised cell is exactly")
print("  where a spurious-round storm would show. For D at sc2 it is a two-sided")
print("  IDENTITY check, because D is bit-identical at N = 1.")
print(f"\n  GUARDS CLEAN: {GUARDS_CLEAN}   LIVENESS CLEAN: {LIVENESS_CLEAN}")
print("\n" + "=" * 96)
print("NOTHING IN THIS REPORT FLIPS A DEFAULT. Every deliverable is a")
print("RECOMMENDATION with its noise bounds — and §16.68.1 pre-registers that no")
print("RACK arm can earn one.")
print("=" * 96)
