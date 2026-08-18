#!/usr/bin/env python3
"""Scoring pass for THE LADDER BATTERY.

Scored against goal-gate "Ladder Battery — PRE-REGISTRATION" (commit `91c00dd`),
which is the CONTRACT: this file implements its clauses and inverts none of them.
Every bar below is transcribed from that block; nothing here decides anything the
pre-registration did not already fix.

  usage: ladder_report.py [--calib] <ledger.log> [<ledger.log> ...]

  --calib  print ONLY the invocation accounting, the two-sided liveness audit and
           the discipline-16 headroom table, and score NOTHING. This is what the
           calibration pass (`ladder_calib.sh`, one rep per arm per cell, n = 1)
           is read with: it carries no sigma, no seed-7 evidence, and nothing in
           it is a result. Its output fills the contract's headroom table and is
           committed as the contract's COMPLETION before the scored run.

WHAT IS AND IS NOT A DENOMINATOR (the pre-registration's own split):

  ABORT            no `[GATES]` on EITHER endpoint. NOT in any denominator.
                   `ARMCOUNT` in the driver counts PARSED ROWS and therefore
                   counts aborts too — it is a vanish-detector, NOT an n. The
                   live n is recomputed here from the gates columns, and that
                   recomputation is the only reason the mode-hunt battery's
                   top-up trigger was ever visible.
  DNF              a completed run that did not transfer. IS a datum, IS in the
                   denominator, reported separately.
  INSTRUMENT-FAIL  completed but a gauge did not report. Excluded from the
                   statistic it voids, WITH THE EXCLUSION COUNTED. Here that is a
                   missing `[SUMCAP]` on an N-carrying arm, `eng=0/N` at a DUAL
                   (a WARM-UP FAILURE, not a null result), a missing `[WALL]`, a
                   missing `pl=`, or a missing CPU or ping gauge — each voiding
                   only its own claim.

TWO RULES THIS REPORTER DOES **NOT** INHERIT FROM ITS PREDECESSORS, both from the
contract's INSTRUMENT FACTS, and both would silently void the battery if they
were:

  * `[SUMCAP] eng=0/0` at c1 and sc2 is the CORRECT reading, not a warm-up
    failure: `pooled_store_cap` returns None on `n_live < 2` BEFORE the
    multiplier is read. The parser carries `sumcap_n1_expected` so this is a
    column, not an inference. `eng=0/N` at a DUAL is a warm-up failure and voids
    the rep.
  * `[CCAP] eng=0/0 cap=0.0 mem=0 floor=0` on FULL is BY CONSTRUCTION — the line
    is emitted for either brake door while its bind-fraction accumulator is
    guarded by `composed_cap` alone. The composed battery's "eng=0/N is a warm-up
    fail" rule is right THERE and is not applied here; on FULL the only `[CCAP]`
    field carrying a datum is `brake=`.

MEASUREMENT DISCIPLINE 18(d) IS EVALUATED BEFORE ANY MECHANISM VERDICT: no
verdict about the multiplier may be recorded from an arm whose law was pinned —
it measured the clamp. c8L is PRE-DECLARED pinned by the contract's own
arithmetic and is voided for the N rung before its numbers are read, not after.
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
# default. Without this the script dies on a Windows cp1252 console halfway
# through a table, which is exactly the kind of failure that would otherwise be
# discovered while holding the VM lock.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

# ── The transcribed bars. Every one is quoted from the pre-registration; none is
#    recomputed, softened or added here.
ENG_BAR = 0.90            # N-INTERIOR: [SUMCAP] eng >= 0.90
CHG_BAR = 0.90            # N-INTERIOR: chg_frac >= 0.90
PIN_CLAIM = 0.10          # N-INTERIOR / G-CAPBIND: pin <= 0.10 to CLAIM interior
PIN_VOID = 0.50           # discipline 18: pin > 0.50 on a majority VOIDS the cell
#: N-INTERIOR's cap bands — the published 3271 / 3020 with a +-20% anchor
#: tolerance, wide enough to contain BOTH wire sources for Sigma (16.60's
#: READOUT 3 and ADR-0071's composed-battery inputs disagree by 6%). Both bands
#: are strictly below 4096, which is what makes them falsifiable.
N_BAND = {"c7": (2617.0, 3925.0), "c8": (2416.0, 3624.0)}
#: The cell the contract PRE-DECLARES unscoreable for the N rung: Sigma 4976
#: against an interiority threshold of N*knee/gain = 2048, so the corrected ask
#: is 2.43x the ceiling (2.28x on the worst anchor reading). No multiplier
#: verdict may be recorded there.
N_PREDECLARED_VOID = {"c8L": ("ceiling-governed by arithmetic: Sigma = 4976 vs "
                              "the 2048 interiority threshold -> ask 2.43x the "
                              "4096 ceiling; ADR-0071 finding 1 (W = 7489 = "
                              "1.83x WIN_STORE_MAX before any slack or span)")}
#: The value arm A is EXPECTED to sit on (N-CONTROL). The contract scopes this
#: clause to the DUALS, where ADR-0070 finding 2 rests on 178 dual reps pinned at
#: exactly 4096 across five sessions plus 64 of 65 in the composed battery, and
#: where `N*knee` and `WIN_STORE_MAX` collide at 4096 — one bind with two names.
#: sc2 rides along on the composed battery's own 21-of-22 `RELIABLE_STORE_MAX`
#: reading. c1 is DELIBERATELY ABSENT and is REPORTED rather than scored: the
#: wire's c1-A is the legacy 2*BDP branch and reads INTERIOR (541 in ADR-0071's
#: inputs table) with a `capboot` minority beside it, so a pin bar there would be
#: a bar on a cell whose law is not the one under test.
A_PIN = {"sc2": 1024, "c7": 4096, "c8": 4096, "c8L": 4096}

EPS_DROP_BAR = 5.0        # T-EPS: pl= at least 5x below same-session A at duals
EPS_ABS_BAR = 0.05        # T-EPS: below 0.05 on every dual leg
EPS_FALSIFY = 2.0         # T-EPS falsified if the move is under 2x
EPS_IDENT_MAX = 2.0       # T-IDENT: single-path move must stay UNDER 2x

CPU_BAR = 1.05            # G-CPU point band
PAIRED_MIN = 8            # B-WALL: paired reps with a non-zero difference needed
COLLAPSE_MBIT = 60.0      # F-C8LIABILITY: the uniflip collapse class
HEADROOM_BAR = 5.0        # discipline 16c: below this, no throughput target

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
ARMS = ["A", "N", "T", "NT", "FULL"]
DUALS = ["c7", "c8", "c8L"]
SINGLES = ["c1", "sc2"]

#: The contract's echo-expectations table, restated as data. The driver derives
#: the ARM ENV from its own copy of this table; this is the independent
#: recomputation from the parsed columns, which is the point of asserting a
#: liveness echo at all.
ARM_ON = {
    "sum_cap": {"N", "NT", "FULL"},
    "loss_sent_truth": {"T", "NT", "FULL"},
    "charge_recovery": {"T", "NT", "FULL"},
    "release_1to1": {"T", "NT", "FULL"},
    "store_cap_unified": {"FULL"},
    "late_brake": {"FULL"},
    "composed_cap": set(),
    "three_term": set(),
}


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


print("=" * 92)
print("LADDER BATTERY — " + ("CALIBRATION PASS (n = 1; NOTHING HERE IS A RESULT)"
                             if CALIB else "SCORING PASS"))
print('contract: goal-gate "Ladder Battery — PRE-REGISTRATION" (91c00dd), era main@5ddf7f6')
print("arms: A = shipped | N = SUM_CAP | T = the ledger/loss trio | NT = N+T | "
      "FULL = NT + UNIFIED + LATE_BRAKE")
print("=" * 92)

# ── 1. ACCOUNTING ────────────────────────────────────────────────────────
print("\n### INVOCATION ACCOUNTING (abort != DNF != INSTRUMENT-FAIL)")
print("### ARMCOUNT in the driver is NOT this table's n — it counts aborts too.\n")
print(f"{'cell-arm':<10} {'rows':>6} {'ABORT':>6} {'live':>6} {'DNF':>5} "
      f"{'noSUMCAP':>9} {'noWALL':>7} {'noPL':>6} {'noCPU':>6} {'noPING':>7}")
LIVE = {}
for c in CELLS:
    for a in ARMS:
        rs = by.get((c, a), [])
        lv = live((c, a))
        LIVE[(c, a)] = lv
        nsc = sum(1 for r in lv
                  if a in ARM_ON["sum_cap"] and not r.get("sumcap_lines"))
        nwall = sum(1 for r in lv if not r.get("wall_lines"))
        npl = sum(1 for r in lv if not r.get("pl_n"))
        ncpu = sum(1 for r in lv if r.get("cpucli") is None)
        nping = sum(1 for r in lv if not r.get("ping_n"))
        dnf = sum(1 for r in lv if r.get("dnf"))
        print(f"{c+'-'+a:<10} {len(rs):>6} {len(rs)-len(lv):>6} {len(lv):>6} {dnf:>5} "
              f"{nsc:>9} {nwall:>7} {npl:>6} {ncpu:>6} {nping:>7}")

if not CALIB:
    print("\n  Per-seed live n (the G-TOPUP floor is n = 8 at EITHER seed):\n")
    print(f"{'cell-arm':<10} {'s42':>6} {'s7':>6}   top-up needed?")
    for c in CELLS:
        for a in ARMS:
            n42 = sum(1 for r in LIVE[(c, a)] if r["seed"] == 42)
            n7 = sum(1 for r in LIVE[(c, a)] if r["seed"] == 7)
            need = ("YES — SYMMETRIC over ALL FIVE ARMS of this cell"
                    if min(n42, n7) < 8 else "no")
            print(f"{c+'-'+a:<10} {n42:>6} {n7:>6}   {need}")

# ── 2. LIVENESS, TWO-SIDED, RECOMPUTED FROM THE COLUMNS ──────────────────
print("\n### LIVENESS, TWO-SIDED, RECOMPUTED FROM THE COLUMNS (discipline 15c)")
print("### An arm that cannot show its control was a control has measured one "
      "condition twice.\n")
hdr = f"{'cell-arm':<10} " + " ".join(f"{g[:9]:>10}" for g in ARM_ON) + \
      f" {'instr':>7} {'SUMCAP':>7} {'actU':>6} {'act3T=0':>8} {'CCAP':>6}"
print(hdr)
LIVENESS_CLEAN = True
for c in CELLS:
    for a in ARMS:
        rs = LIVE[(c, a)]
        if not rs:
            continue
        n = len(rs)
        cols = []
        for g, on_arms in ARM_ON.items():
            exp = 1 if a in on_arms else 0
            k = sum(1 for r in rs
                    if r.get("gates_cli_" + g) == exp and r.get("gates_srv_" + g) == exp)
            cols.append(f"{k}/{n}({exp})")
            if k != n:
                LIVENESS_CLEAN = False
        instr = sum(1 for r in rs
                    if all(r.get(f"gates_{s}_{g}") == 1
                           for s in ("cli", "srv")
                           for g in ("diag", "ackdiag", "walldiag")))
        want_sc = a in ARM_ON["sum_cap"]
        sc = sum(1 for r in rs if bool(r.get("sumcap_lines")) == want_sc)
        want_u = a in ARM_ON["store_cap_unified"]
        au = sum(1 for r in rs
                 if bool(r.get("active_u_cli") or r.get("active_u_srv")) == want_u)
        a3 = sum(1 for r in rs
                 if not (r.get("active_3t_cli") or r.get("active_3t_srv")))
        want_cc = a in ARM_ON["late_brake"]
        cc = sum(1 for r in rs if bool(r.get("ccap_lines")) == want_cc)
        for k in (instr, sc, au, a3, cc):
            if k != n:
                LIVENESS_CLEAN = False
        print(f"{c+'-'+a:<10} " + " ".join(f"{x:>10}" for x in cols) +
              f" {f'{instr}/{n}':>7} {f'{sc}/{n}':>7} {f'{au}/{n}':>6} "
              f"{f'{a3}/{n}':>8} {f'{cc}/{n}':>6}")
print("\n  `SUMCAP` counts reps whose [SUMCAP] PRESENCE matches the arm (emitted")
print("  only on the ON arm, fed the counterfactual on both). `act3T=0` counts")
print("  reps with the three-term echo ABSENT, which is the expected state on")
print("  EVERY arm: no ladder arm reaches the three-term pool seat. `CCAP` counts")
print("  reps whose [CCAP] presence matches — PRESENT on FULL through the")
print("  LATE_BRAKE door, ABSENT elsewhere.")
if not LIVENESS_CLEAN:
    print("\n  *** LIVENESS IS NOT CLEAN. Every clause below rests on it; read the")
    print("      driver's ARM-LIVENESS-FAIL / ARM-CONTAMINATION lines before any")
    print("      number in this report (discipline 15c). ***")

# ── 3. HEADROOM (discipline 16b) — the calibration's whole deliverable ────
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
print("  is a bind fraction, a cap magnitude, an eps-hat class, a paired sign test")
print("  or a no-regression guard; goodput appears only in the N rung's OUTRIGHT-")
print("  REFUTATION falsifier and in G-REG, both one-sided DOWNWARD and therefore")
print("  satisfiable at any utilisation. This table is the check that those")
print("  decisions were made against the right arithmetic — and, at the")
print("  calibration, it is what fills the contract's own headroom table.")

if CALIB:
    print("\n" + "=" * 92)
    print("CALIBRATION PASS ENDS HERE. n = 1: no sigma, no seed-7 evidence, and")
    print("nothing above is a result. Commit this table as the contract's")
    print("COMPLETION — in its own commit — BEFORE launching ladder_all.sh, and")
    print("read the ledger's LIVENESS / ARM-LIVENESS-FAIL / ARM-CONTAMINATION")
    print("lines first: this is also the smoke.")
    print("=" * 92)
    sys.exit(0)

# ── 4. CAPBIND + THE `[SUMCAP]` GAUGE — RUNG N ───────────────────────────
print("\n### CAPBIND — the standard bind-fraction readout (ADR-0070 kit item 2)\n")
for ln in capbind_lines(rows, cells=set(CELLS), arms=set(ARMS)):
    print("  " + ln)

print("\n### `[SUMCAP]` — the xN deletion's engagement, counterfactual and pin\n")
print(f"{'cell-arm':<10} {'n':>4} {'eng med':>8} {'eng>=.9':>8} {'chg_frac':>9} "
      f"{'pin med':>8} {'pin<=.10':>9} {'cap med':>9} {'ask med':>9} {'band':>16}")
SUM = {}
for c in CELLS:
    for a in ARMS:
        if a not in ARM_ON["sum_cap"]:
            continue
        rs = [r for r in LIVE[(c, a)] if r.get("sumcap_lines")]
        if not rs:
            print(f"{c+'-'+a:<10} {0:>4}   (no [SUMCAP] on any live rep — INSTRUMENT-FAIL)")
            SUM[(c, a)] = None
            continue
        # INSTRUMENT FACT 2: eng=0/0 at a SINGLE-path cell is the correct
        # reading (the N=1 short-circuit), not a warm-up failure. Those reps
        # are counted separately and excluded from the engagement statistic
        # rather than dragging it to zero.
        n1 = [r for r in rs if r.get("sumcap_n1_expected")]
        eff = [r for r in rs if not r.get("sumcap_n1_expected")]
        warm = [r for r in eff
                if r.get("sumcap_eng_n") == 0 and (r.get("sumcap_eng_d") or 0) > 0]
        d = {
            "n": len(rs), "n1": len(n1), "warmfail": len(warm),
            "eng": med([r["sumcap_eng"] for r in eff]),
            "eng_ok": frac_at_least([r["sumcap_eng"] for r in eff], ENG_BAR),
            "chg": med([r["sumcap_chg_frac"] for r in eff]),
            "chg_ok": frac_at_least([r["sumcap_chg_frac"] for r in eff], CHG_BAR),
            "pin": med([r["sumcap_pin"] for r in eff]),
            "pin_ok": frac_at_most([r["sumcap_pin"] for r in eff], PIN_CLAIM),
            "pin_void": frac_at_least([r["sumcap_pin"] for r in eff], PIN_VOID),
            "cap": med([r["sumcap_cap"] for r in eff]),
            "ask": med([r["sumcap_ask"] for r in eff]),
        }
        SUM[(c, a)] = d
        band = N_BAND.get(c)
        bstr = "-" if not band else (
            "IN" if (d["cap"] is not None and band[0] <= d["cap"] <= band[1])
            else "OUT")
        if band:
            bstr = f"{bstr} [{band[0]:.0f},{band[1]:.0f}]"
        note = ""
        if d["n1"]:
            note = f"  ({d['n1']} reps eng=0/0 = the N=1 short-circuit, EXPECTED)"
        if d["warmfail"]:
            note += f"  ({d['warmfail']} WARM-UP FAILURES at a dual — no datum)"
        print(f"{c+'-'+a:<10} {d['n']:>4} {fmt(d['eng'],3):>8} {fmt(d['eng_ok'],3):>8} "
              f"{fmt(d['chg'],3):>9} {fmt(d['pin'],3):>8} {fmt(d['pin_ok'],3):>9} "
              f"{fmt(d['cap']):>9} {fmt(d['ask']):>9} {bstr:>16}{note}")

print("\n### N-CONTROL — arm A's realized cap, the contrast N is measured against\n")
for c in CELLS:
    caps = [v for v in (r.get("occcap_p50") for r in LIVE[(c, "A")]) if v is not None]
    if c not in A_PIN:
        print(f"  {c:<5} A occcap_p50 med {fmt(med(caps)):>8}  REPORTED, not scored —"
              " c1-A rides the legacy 2*BDP branch and reads interior (541 in"
              " ADR-0071's inputs) with a capboot minority beside it")
        continue
    hit = sum(1 for v in caps if int(round(v)) == A_PIN[c])
    tot = len(caps)
    ok = bool(tot and hit / tot > 0.5)
    print(f"  {c:<5} A occcap_p50 == {A_PIN[c]:<5} on {hit}/{tot} reps   {verdict(ok)}"
          + ("" if ok else "   *** A IS NOT PINNED — something under the transport"
                           " has moved and NOTHING in this battery is scored until"
                           " that is explained ***"))

print("\n" + "=" * 92)
print("RUNG N — THE BIND-FRACTION AXIS (discipline 18(d) evaluated FIRST)")
print("=" * 92 + "\n")
N_VOID = {}
for c in CELLS:
    if c in N_PREDECLARED_VOID:
        N_VOID[c] = "PRE-DECLARED by the contract: " + N_PREDECLARED_VOID[c]
        print(f"  {c:<5} UNSCOREABLE FOR THE N RUNG — {N_VOID[c]}")
        d = SUM.get((c, "N"))
        if d:
            print(f"        observed: pin med {fmt(d['pin'],3)}, chg_frac med "
                  f"{fmt(d['chg'],3)}, cap med {fmt(d['cap'])} — reported, scored on"
                  " nothing about the multiplier")
        continue
    if c in SINGLES:
        continue
    d = SUM.get((c, "N"))
    if d is None:
        N_VOID[c] = "no [SUMCAP] gauge on any live N rep"
        print(f"  {c:<5} UNSCORED — no [SUMCAP] gauge")
        continue
    if d["pin_void"] is not None and d["pin_void"] > 0.5:
        N_VOID[c] = (f"pin >= {PIN_VOID} on {d['pin_void']:.1%} of reps — "
                     "THE ARM MEASURED THE CLAMP")
        print(f"  {c:<5} VOID (discipline 18(d)) — {N_VOID[c]}")
        print("        this law operates as a constant at this cell; no verdict about"
              " the multiplier may be recorded from it")

print("\n  N-INTERIOR (c7, c8) — the primary claim\n")
for c in ("c7", "c8"):
    d = SUM.get((c, "N"))
    if d is None or c in N_VOID:
        print(f"  {c:<5} not scored ({N_VOID.get(c, 'no gauge')})")
        continue
    band = N_BAND[c]
    clauses = [
        (f"pin <= {PIN_CLAIM} on a majority", (d["pin_ok"] or 0) > 0.5),
        (f"eng >= {ENG_BAR} on a majority", (d["eng_ok"] or 0) > 0.5),
        (f"chg_frac >= {CHG_BAR} on a majority", (d["chg_ok"] or 0) > 0.5),
        (f"cap in [{band[0]:.0f}, {band[1]:.0f}]",
         d["cap"] is not None and band[0] <= d["cap"] <= band[1]),
        ("CAPBIND reads interior",
         not any(l.strip().startswith("WARN") and f"{c}/N" in l
                 for l in capbind_lines(rows, cells={c}, arms={"N"}))),
    ]
    for name, ok in clauses:
        print(f"  {c:<5} {name:<38} {verdict(ok)}")

print("\n  N-IDENT (c1, sc2) — the N = 1 identity, bit-identical by construction\n")
for c in SINGLES:
    a_g = [r.get("mbps") for r in LIVE[(c, "A")]]
    for a in ("N", "NT"):
        n_g = [r.get("mbps") for r in LIVE[(c, a)]]
        s2 = two_sigma(a_g + n_g)
        dm = None
        if mean(a_g) is not None and mean(n_g) is not None:
            dm = mean(n_g) - mean(a_g)
        ok = (dm is not None and s2 is not None and abs(dm) <= s2)
        print(f"  {c:<5} {a:<4} goodput delta {fmt(dm,2):>8} Mbit  (2sigma "
              f"{fmt(s2,2)})  {verdict(ok)}"
              + ("" if ok else "   *** a wire difference at N = 1 is a BUILD or"
                               " INSTRUMENT alarm before it is a result ***"))

print("\n  THE OUTRIGHT-REFUTATION FALSIFIER — a PROVABLY INTERIOR N more than")
print("  2sigma_pooled below A on BOTH seeds. c8 is the PRE-NAMED risk cell: the")
print("  corrected cap 3020 is 0.71x the cell's own W+S = 4232, i.e. the deletion")
print("  is predicted to under-fund c8's resequencing span by 29%.\n")
for c in CELLS:
    if c in SINGLES or c in N_VOID:
        continue
    d = SUM.get((c, "N"))
    interior = bool(d and (d["pin_ok"] or 0) > 0.5 and (d["eng_ok"] or 0) > 0.5
                    and (d["chg_ok"] or 0) > 0.5)
    fired = []
    for s in (42, 7):
        ag = [r.get("mbps") for r in LIVE[(c, "A")] if r["seed"] == s]
        ng = [r.get("mbps") for r in LIVE[(c, "N")] if r["seed"] == s]
        s2 = two_sigma(ag + ng)
        if mean(ag) is None or mean(ng) is None or s2 is None:
            continue
        fired.append(mean(ag) - mean(ng) > s2)
    both = bool(fired) and all(fired) and len(fired) == 2
    if interior and both:
        print(f"  {c:<5} *** REFUTATION FIRES: N provably interior AND >2sigma down on"
              " both seeds. The multiplier was buying something real — at c8, the"
              " SPAN. ***")
    else:
        print(f"  {c:<5} interior={interior}  >2sigma-down-both-seeds={both}  "
              "-> refutation does NOT fire")

# ── 5. RUNG T — THE eps-hat AXIS ─────────────────────────────────────────
print("\n" + "=" * 92)
print("RUNG T — THE eps-hat AXIS")
print("=" * 92)
print("\n  `pl=` is the per-path loss estimate THE RECOVERY PLANE ACTUALLY KEYS ON")
print("  (repair_debt, P_lost, NACK budgets). Calibration, labelled calibration and")
print("  NOT a target (item 3's ledger replay, Sigma form): legacy 0.514/0.512 at")
print("  c7 and 0.825 at c8/p1, fixed 0.0069/0.0110 at c7, against realized loss")
print("  0.0055/0.0196. The shipped loss_rate() EWMA carries a KNOWN ~1/2 under-read")
print("  at a rare-loss cell, BOUNDED to [0.3eps, 1.5eps] by the lib test — so the")
print("  bar is a CLASS and a RATIO, never a point.\n")
print(f"{'cell':<6} {'A pl_max':>9} {'T pl_max':>9} {'ratio':>7} {'T pl below .05':>15}  clause")
for c in CELLS:
    a_pl = med([r.get("pl_max") for r in LIVE[(c, "A")]])
    t_pl = med([r.get("pl_max") for r in LIVE[(c, "T")]])
    ratio = (a_pl / t_pl) if (a_pl and t_pl) else None
    below = frac_at_most([r.get("pl_max") for r in LIVE[(c, "T")]], EPS_ABS_BAR)
    if c in DUALS:
        ok = (ratio is not None and ratio >= EPS_DROP_BAR
              and (below or 0) > 0.5)
        cl = f"T-EPS (>= {EPS_DROP_BAR}x AND < {EPS_ABS_BAR}): {verdict(ok)}"
        if ratio is not None and ratio < EPS_FALSIFY:
            cl += "  *** FALSIFIED: under 2x — the correction did not reach the estimator ***"
    else:
        ok = ratio is None or ratio < EPS_IDENT_MAX
        cl = f"T-IDENT (< {EPS_IDENT_MAX}x): {verdict(ok)}"
        if not ok:
            cl += ("  *** a large single-path move is a finding about the ESTIMATOR,"
                   " not a confirmation ***")
    print(f"{c:<6} {fmt(a_pl,4):>9} {fmt(t_pl,4):>9} {fmt(ratio,2):>7} "
          f"{fmt(below,3):>15}  {cl}")

print("\n  The [ACKDIAG] recon WITNESS — reported on BOTH arms whatever the gate")
print("  says, because the gauge counts all three cursors unconditionally. It is")
print("  what makes the `pl=` move attributable: the witness does not move, the")
print("  estimator does.\n")
recon_keys = sorted({k for r in rows for k in r
                     if k.startswith("recon_cecr_") or k.startswith("recon_crs_")})
print(f"{'cell':<6} {'arm':<5} " + " ".join(f"{k[len('recon_'):]:>12}" for k in recon_keys))
for c in CELLS:
    for a in ARMS:
        rs = LIVE[(c, a)]
        if not rs:
            continue
        print(f"{c:<6} {a:<5} " +
              " ".join(f"{fmt(med([r.get(k) for r in rs]),3):>12}" for k in recon_keys))

print("\n  T-MARGIN / T-SF — item 3's wire question 2 and item 5's wire question 1,")
print("  verbatim. Repair volume and retx fall; the `sf=` empty-set tick rate RISES")
print("  at the duals; goodput moves by <<sigma unless the margin or the leak was")
print("  LOAD-BEARING — and if goodput FALLS that is a finding about the MARGIN LAW")
print("  and the ADMISSION GATE respectively, not about eps-hat or the ledger.\n")
print(f"{'cell':<6} {'A retx':>8} {'T retx':>8} {'A sf_zero':>10} {'T sf_zero':>10} "
      f"{'A Mbit':>8} {'T Mbit':>8} {'delta':>8} {'2sigma':>8}  reading")
for c in CELLS:
    ar = med([r.get("retx") for r in LIVE[(c, "A")]])
    tr = med([r.get("retx") for r in LIVE[(c, "T")]])
    az = med([r.get("sf_zero") for r in LIVE[(c, "A")]])
    tz = med([r.get("sf_zero") for r in LIVE[(c, "T")]])
    ag = [r.get("mbps") for r in LIVE[(c, "A")]]
    tg = [r.get("mbps") for r in LIVE[(c, "T")]]
    s2 = two_sigma(ag + tg)
    dm = (mean(tg) - mean(ag)) if (mean(tg) is not None and mean(ag) is not None) else None
    if dm is None or s2 is None:
        rd = "no datum"
    elif abs(dm) <= s2:
        rd = "goodput <<sigma — the honest null"
    elif dm < 0:
        rd = ("GOODPUT FELL >2sigma — the poisoned margin / the leak was LOAD-BEARING"
              " over-provisioning (a finding about the margin law and the admission"
              " gate, NOT about eps-hat)")
    else:
        rd = "goodput ROSE >2sigma — reported; no clause claims it"
    print(f"{c:<6} {fmt(ar,0):>8} {fmt(tr,0):>8} {fmt(az,3):>10} {fmt(tz,3):>10} "
          f"{fmt(mean(ag),1):>8} {fmt(mean(tg),1):>8} {fmt(dm,2):>8} {fmt(s2,2):>8}  {rd}")

print("\n  T-PACER (item 5's wire question 3): RWM_CC_PACE is read from the [GATES]")
print("  echo of the run, as a CODE FACT, never assumed. With it 0 the pacer term of")
print("  RWM_CHARGE_RECOVERY is INERT and the arm is really two-quantity.")
_pace = {r.get("gates_cli_cc_pace") for r in rows if r.get("gates_lines_cli")}
print(f"    observed RWM_CC_PACE on the client [GATES] line: {sorted(x for x in _pace if x is not None)}")

print("\n  THE ATTRIBUTION LIMIT, from the contract: T flips all three ledger/loss")
print("  gates together, both source sections forbid attributing WITHIN it, and this")
print("  report attributes nothing to any one of them. The 2x2 (RELEASE_1TO1 x")
print("  CHARGE_RECOVERY, with and without LOSS_SENT_TRUTH) is OWED and is not")
print("  delivered by this battery.")

# ── 6. RUNG SET/BRAKE — the PAIRED dead-wall design ──────────────────────
print("\n" + "=" * 92)
print("RUNG SET/BRAKE — [CCAP] brake= AND THE PAIRED WITHIN-REP DEAD WALL")
print("=" * 92 + "\n")
print(f"{'cell':<6} {'n':>4} {'brake med':>10} {'ticks med':>10}  B-ARMED")
for c in CELLS:
    rs = [r for r in LIVE[(c, "FULL")] if r.get("ccap_lines")]
    if not rs:
        print(f"{c:<6} {0:>4}   (no [CCAP] on any live FULL rep — INSTRUMENT-FAIL)")
        continue
    bt = med([r.get("ccap_brake_ticks") for r in rs])
    bf = med([r.get("ccap_brake") for r in rs])
    armed = sum(1 for r in rs if (r.get("ccap_brake_ticks") or 0) > 0)
    ok = armed == len(rs)
    print(f"{c:<6} {len(rs):>4} {fmt(bf,4):>10} {fmt(bt,0):>10}  {armed}/{len(rs)} armed "
          f"{verdict(ok)}"
          + ("" if ok else "   *** brake=0/0 is a NULL EFFECT: the extraction did not"
                           " reach the seat and NO claim about the brake may be made ***"))

print("\n  B-WALL — the PAIRED WITHIN-REP-INDEX sign test at c8. The Mode-Hunt")
print("  RESULTS' binding recommendation: two independent measurands have inverted")
print("  between pools minutes apart on a byte-identical binary, so the next attempt")
print("  changes the DESIGN, not the statistic. One sign per (seed, pool, rep index)")
print("  where BOTH arms produced a live [WALL]; never a difference of medians.\n")
paired = defaultdict(list)
for c in ("c8", "c8L"):
    idx = defaultdict(dict)
    for a in ("A", "FULL"):
        for r in LIVE[(c, a)]:
            if r.get("wall_dur_ms") is None:
                continue
            idx[(r["seed"], r["_pool"], r["rep"])][a] = r["wall_dur_ms"]
    for key, d in sorted(idx.items()):
        if "A" in d and "FULL" in d:
            diff = d["FULL"] - d["A"]
            paired[(c, key[0], key[1])].append(diff)
for c in ("c8", "c8L"):
    groups = {k: v for k, v in paired.items() if k[0] == c}
    if not groups:
        print(f"  {c:<5} no paired reps — B-WALL has no datum")
        continue
    signs = {}
    for (cc, seed, pool), diffs in sorted(groups.items()):
        nz = [d for d in diffs if d != 0.0]
        pos = sum(1 for d in nz if d > 0)
        sg = None if not nz else (1 if pos * 2 > len(nz) else (-1 if pos * 2 < len(nz) else 0))
        signs[(seed, pool)] = (sg, len(nz), len(diffs))
        print(f"  {c:<5} seed={seed} pool={pool:<5} paired n={len(diffs):>3} "
              f"non-zero={len(nz):>3} FULL>A on {pos}/{len(nz)}  sign="
              f"{'+' if sg == 1 else ('-' if sg == -1 else '0/undetermined')}")
    enough = all(v[1] >= PAIRED_MIN for v in signs.values())
    ss = {v[0] for v in signs.values() if v[0] is not None}
    consistent = len(ss) == 1 and 0 not in ss
    if c == "c8":
        if enough and consistent:
            print(f"  {c:<5} B-WALL RESOLVES: paired sign consistent across seeds and"
                  " pools, with >= 8 non-zero paired reps everywhere.")
        else:
            print(f"  {c:<5} *** B-WALL CLOSES **NEEDS-MORE**, per the contract's")
            print("        pre-written close. The instrument named: `[WALL] dur_ms` at c8")
            print("        does not resolve at any n this project has been willing to")
            print("        spend (tick-share inverted; onset/duration inverted; the")
            print("        mode-hunt baseline read 0.346 against its predecessor's 0.727")
            print("        on a byte-identical binary). What is owed is a c8 statistic")
            print("        that is not bistable — NOT a fourth measurand. NO dead-wall")
            print("        claim is made from an unpaired contrast, at any n. ***")
    else:
        print(f"  {c:<5} reported, direction only, scored on nothing (16.54's 0/24 at"
              " 8x the transfer is the one clean prior signal in this family)")

# ── 7. RUNG FULL ─────────────────────────────────────────────────────────
print("\n" + "=" * 92)
print("RUNG FULL")
print("=" * 92 + "\n")
print("  F-FACTOR — the wire's test of item 1's INDEPENDENCE claim: the xN gate and")
print("  the live set are independent axes, so FULL's [SUMCAP] cap must land inside")
print("  the SAME band N's does.\n")
for c in ("c7", "c8"):
    dn, df = SUM.get((c, "N")), SUM.get((c, "FULL"))
    band = N_BAND[c]
    ok = bool(df and df["cap"] is not None and band[0] <= df["cap"] <= band[1])
    print(f"  {c:<5} N cap {fmt(dn['cap']) if dn else '-':>8}   FULL cap "
          f"{fmt(df['cap']) if df else '-':>8}   band [{band[0]:.0f},{band[1]:.0f}]   "
          f"{verdict(ok)}"
          + ("" if ok else "   *** the set moves the multiplier's own value: the four"
                           " gate combinations are not four points on two axes ***"))

print("\n  F-C8LIABILITY — RWM_STORE_CAP_UNIFIED cost c8 -19.6% at seed 7 in the")
print("  uniflip-era ledger as a collapse MODE (reps at 16.6/40.5/55.0), not a shift.")
print("  The honest anchor is now DEFAULT ON and was measured to neutralize it.\n")
for c in ("c8", "c8L"):
    for a in ("A", "FULL"):
        lowreps = [r.get("mbps") for r in LIVE[(c, a)]
                   if r.get("mbps") is not None and r["mbps"] < COLLAPSE_MBIT]
        print(f"  {c:<5} {a:<5} reps below {COLLAPSE_MBIT:.0f} Mbit/s: "
              f"{len(lowreps)}/{len(LIVE[(c, a)])}   "
              + (", ".join(f"{v:.1f}" for v in sorted(lowreps)) if lowreps else "—"))
print("\n  If FULL reproduces the collapse class where A does not, the harm is NOT")
print("  era-dead and FULL's verdict is about that composition, not about any rung")
print("  below it.")

# ── 8. GUARDS ────────────────────────────────────────────────────────────
print("\n" + "=" * 92)
print("GUARDS — a win bought by breakage is a TRADE and is reported as one")
print("=" * 92 + "\n")
print("  G-REG — no cell more than 2sigma_pooled below A on either seed, ANY arm\n")
print(f"{'cell':<6} {'arm':<5} {'s42 A':>8} {'s42 arm':>8} {'s7 A':>8} {'s7 arm':>8} "
      f"{'2sigma':>8}  G-REG")
for c in CELLS:
    for a in ARMS[1:]:
        cells_ = []
        breach = False
        for s in (42, 7):
            ag = [r.get("mbps") for r in LIVE[(c, "A")] if r["seed"] == s]
            xg = [r.get("mbps") for r in LIVE[(c, a)] if r["seed"] == s]
            s2 = two_sigma(ag + xg)
            cells_.append((mean(ag), mean(xg), s2))
            if (mean(ag) is not None and mean(xg) is not None and s2 is not None
                    and mean(ag) - mean(xg) > s2):
                breach = True
        s2any = max([x[2] for x in cells_ if x[2] is not None], default=None)
        print(f"{c:<6} {a:<5} {fmt(cells_[0][0]):>8} {fmt(cells_[0][1]):>8} "
              f"{fmt(cells_[1][0]):>8} {fmt(cells_[1][1]):>8} {fmt(s2any,2):>8}  "
              f"{verdict(not breach)}")

print("\n  G-SC2-LAT — the crown-class delivered latency must SURVIVE at sc2:")
print("  ping_p50 not more than 2sigma worse than same-session A, at goodput within")
print("  2sigma of same-session A, both seeds. This battery claims NO reproduction of")
print("  16.50 F6's halved latency — that belonged to the three-term law, which NO")
print("  arm here carries. The guard is SURVIVAL, and it is keyed to LATENCY.\n")
for a in ARMS:
    ap = [r.get("ping_p50") for r in LIVE[("sc2", "A")]]
    xp = [r.get("ping_p50") for r in LIVE[("sc2", a)]]
    ag = [r.get("mbps") for r in LIVE[("sc2", "A")]]
    xg = [r.get("mbps") for r in LIVE[("sc2", a)]]
    s2p, s2g = two_sigma(ap + xp), two_sigma(ag + xg)
    dlat = (mean(xp) - mean(ap)) if (mean(xp) is not None and mean(ap) is not None) else None
    dg = (mean(xg) - mean(ag)) if (mean(xg) is not None and mean(ag) is not None) else None
    ok = (dlat is not None and s2p is not None and dlat <= s2p
          and dg is not None and s2g is not None and abs(dg) <= s2g)
    print(f"  sc2 {a:<5} ping_p50 {fmt(mean(xp),1):>7} vs A {fmt(mean(ap),1):>7} "
          f"(2sigma {fmt(s2p,1)})   goodput delta {fmt(dg,2):>7} (2sigma {fmt(s2g,2)})"
          f"   {verdict(ok)}")

print("\n  G-CPU — sender CPU/byte <= 1.05x A as a POINT band, every cell, both seeds\n")
for c in CELLS:
    ac = mean([r.get("cpucli") for r in LIVE[(c, "A")]])
    for a in ARMS[1:]:
        xc = mean([r.get("cpucli") for r in LIVE[(c, a)]])
        ratio = (xc / ac) if (ac and xc) else None
        ok = ratio is not None and ratio <= CPU_BAR
        print(f"  {c:<5} {a:<5} CPU/byte {fmt(xc,2):>7} vs A {fmt(ac,2):>7}  "
              f"ratio {fmt(ratio,3):>6}  {verdict(ok)}")

print("\n  G-DNF — dnf = 0 in every completed run, every arm, both seeds\n")
tot_dnf = sum(1 for r in rows if r.get("dnf") and (r.get("gates_lines_cli") or r.get("gates_lines_srv")))
print(f"    live reps with dnf: {tot_dnf}   {verdict(tot_dnf == 0)}")
print("    (ABORT != DNF != INSTRUMENT-FAIL: an invocation with no [GATES] on either")
print("     endpoint is an ABORT, carries no datum, and is in NO denominator.)")

print("\n  G-CAPBIND — any arm CLAIMING an interior landing shows CAPBIND")
print("  `name=interior` AND [SUMCAP] pin <= 0.10. A claiming arm carrying a CAPBIND")
print("  WARN line has measured the clamp; discipline 18(d) forbids the claim.\n")
for ln in capbind_lines(rows, cells=set(CELLS), arms=set(ARMS)):
    if ln.strip().startswith("WARN"):
        print("  " + ln)

print("\n" + "=" * 92)
print("Nothing in this report flips a default. Every deliverable is a")
print("RECOMMENDATION with its noise bounds, written so a separate trivial flip")
print("commit can cite it.")
print("=" * 92)
