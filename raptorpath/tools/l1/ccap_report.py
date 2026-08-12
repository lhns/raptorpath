#!/usr/bin/env python3
"""Scoring pass for THE COMPOSED-CAP BATTERY.

Scored against goal-gate "Composed-Cap Battery — VM PRE-REGISTRATION" (commit
`1e09c00`), which is the CONTRACT: this file implements its clauses and inverts
none of them. Every bar below is transcribed from that block; nothing here
decides anything the pre-registration did not already fix.

  usage: ccap_report.py <ledger.log> [<ledger.log> ...]

WHAT IS AND IS NOT A DENOMINATOR (the pre-registration's own split):

  ABORT            no `[GATES]` on EITHER endpoint. NOT in any denominator.
                   `ARMCOUNT` in the driver counts PARSED ROWS and therefore
                   counts aborts too — it is a vanish-detector, NOT an n. The
                   live n is recomputed here from the gates columns, and that
                   recomputation is the only reason the predecessor's top-up
                   trigger was ever visible.
  DNF              a completed run that did not transfer. IS a datum, IS in the
                   denominator, reported separately.
  INSTRUMENT-FAIL  completed but a gauge did not report. Excluded from the
                   statistic it voids, WITH THE EXCLUSION COUNTED. Here that is
                   a missing `[CCAP]` on a C rep, `eng=0/N` (a WARM-UP FAILURE,
                   not a null result), a missing `[WALL]`, or a missing CPU or
                   ping gauge — each voiding only its own claim.

THE STOP RULE IS EVALUATED FIRST AND PER CELL, because discipline 18(d) forbids
recording a verdict about a MECHANISM from an arm whose law was pinned: it
measured the clamp. A voided cell's goodput is still PRINTED — suppressing it
would be its own dishonesty — but it is printed under its void, and no
mechanism verdict is taken from it.
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
# through the goodput table, which is exactly the kind of failure that would
# otherwise be discovered while holding the VM lock.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

# ── The transcribed bars. Every one is quoted from the pre-registration; none
#    is recomputed, softened or added here.
ENG_BAR = 0.90            # P-ENGAGE / stop rule S3
BIND_BAR = 0.50           # stop rule S1 (mem) and S2 (floor); also capbind's WARN level
BRAKE_BAR = 0.005         # P-BRAKE
BRAKE_PROVEN = 0.01       # the "PROVABLY ENGAGED" clause of the load-bearing falsifier
CPU_BAR = 1.05            # G-CPU point band
SC2_P50_MAX = 55.0        # P-LATENCY-SC2, absolute clause
CAPBAND = {"c1": (150, 1000), "sc2": (150, 1000),
           "c7": (200, 2500), "c8": (200, 2500), "c8L": (200, 2500)}

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
#: The pinned value arm A is EXPECTED to sit on, per cell (P-PINNED). At a dual
#: `N*knee` and `WIN_STORE_MAX` collide at 4096 — one bind with two names.
A_PIN = {"c1": 1024, "sc2": 1024, "c7": 4096, "c8": 4096, "c8L": 4096}

CELLS = ["c1", "c7", "c8", "c8L", "sc2"]
ARMS = ["A", "C"]


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
    """Fraction of non-None values at or above `bar`. The stop rule and
    P-BRAKE are MAJORITY claims over reps, never means over reps — a mean of
    bind fractions would let one pinned rep hide behind nine interior ones."""
    v = [x for x in v if x is not None]
    return (sum(1 for x in v if x >= bar) / len(v)) if v else None


def fmt(x, p=1):
    return "-" if x is None else f"{x:.{p}f}"


# ── Load. Pool provenance is taken from the FILENAME, as in the predecessor,
#    so a top-up can never be silently folded into the main pool.
rows = []
for path in sys.argv[1:]:
    base = path.replace("\\", "/").split("/")[-1]
    pool = "topup" if "topup" in base else "main"
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


print("=" * 88)
print("COMPOSED-CAP BATTERY — SCORING PASS")
print('contract: goal-gate "Composed-Cap Battery — VM PRE-REGISTRATION" (1e09c00)')
print("arms: A = shipped default | C = RWM_COMPOSED_CAP=1")
print("=" * 88)

# ── 1. ACCOUNTING ────────────────────────────────────────────────────────
print("\n### INVOCATION ACCOUNTING (abort != DNF != INSTRUMENT-FAIL)")
print("### ARMCOUNT in the driver is NOT this table's n — it counts aborts too.\n")
print(f"{'cell-arm':<10} {'rows':>6} {'ABORT':>6} {'live':>6} {'DNF':>5} "
      f"{'noCCAP':>7} {'noWALL':>7} {'noCPU':>6} {'noPING':>7}")
LIVE = {}
for c in CELLS:
    for a in ARMS:
        rs = by.get((c, a), [])
        lv = live((c, a))
        LIVE[(c, a)] = lv
        nccap = sum(1 for r in lv if a == "C" and not r.get("ccap_lines"))
        nwall = sum(1 for r in lv if not r.get("wall_lines"))
        ncpu = sum(1 for r in lv if r.get("cpucli") is None)
        nping = sum(1 for r in lv if not r.get("ping_n"))
        dnf = sum(1 for r in lv if r.get("dnf"))
        print(f"{c+'-'+a:<10} {len(rs):>6} {len(rs)-len(lv):>6} {len(lv):>6} {dnf:>5} "
              f"{nccap:>7} {nwall:>7} {ncpu:>6} {nping:>7}")

print("\n  Per-seed live n (the G-TOPUP floor is n = 8 at EITHER seed):\n")
print(f"{'cell-arm':<10} {'s42':>6} {'s7':>6}   top-up needed?")
for c in CELLS:
    for a in ARMS:
        n42 = sum(1 for r in LIVE[(c, a)] if r["seed"] == 42)
        n7 = sum(1 for r in LIVE[(c, a)] if r["seed"] == 7)
        need = "YES — SYMMETRIC over both arms of this cell" if min(n42, n7) < 8 else "no"
        print(f"{c+'-'+a:<10} {n42:>6} {n7:>6}   {need}")

# ── 2. LIVENESS, TWO-SIDED, RECOMPUTED FROM THE COLUMNS ──────────────────
print("\n### LIVENESS, TWO-SIDED, RECOMPUTED FROM THE COLUMNS (discipline 15c)\n")
print(f"{'cell-arm':<10} {'GATES CC cli/srv':>17} {'WALLDIAG':>9} {'ACKDIAG':>8} "
      f"{'act3T':>8} {'actU(exp 0)':>12} {'CCAP':>7} {'WALL':>7}")
for c in CELLS:
    for a in ARMS:
        rs = LIVE[(c, a)]
        if not rs:
            continue
        exp = 1 if a == "C" else 0
        n = len(rs)
        gcc = sum(1 for r in rs if r["gates_cli_cc"] == exp and r["gates_srv_cc"] == exp)
        gw = sum(1 for r in rs if r["gates_cli_wall"] == 1 and r["gates_srv_wall"] == 1)
        ga = sum(1 for r in rs if r["gates_cli_ack"] == 1 and r["gates_srv_ack"] == 1)
        a3 = sum(1 for r in rs if (r["active_3t_cli"] > 0 or r["active_3t_srv"] > 0) == bool(exp))
        au = sum(1 for r in rs if r["active_u_cli"] == 0 and r["active_u_srv"] == 0)
        cc = sum(1 for r in rs if bool(r.get("ccap_lines")) == bool(exp))
        wl = sum(1 for r in rs if r.get("wall_lines"))
        print(f"{c+'-'+a:<10} {f'{gcc}/{n} (exp {exp})':>17} {f'{gw}/{n}':>9} {f'{ga}/{n}':>8} "
              f"{f'{a3}/{n}':>8} {f'{au}/{n}':>12} {f'{cc}/{n}':>7} {f'{wl}/{n}':>7}")
print("\n  `actU` counts reps with the unified echo ABSENT ON BOTH SIDES, which is")
print("  the EXPECTED state on BOTH arms: RWM_COMPOSED_CAP does not set the U bit.")
print("  The pool law already reads live_paths(); C's unified set is at the BRAKE,")
print("  whose liveness is `[CCAP] brake=`. A count below n here is CONTAMINATION.")

# ── 3. HEADROOM RE-MEASURE (discipline 16b) ──────────────────────────────
print("\n### HEADROOM RE-MEASURE (discipline 16b) — tc, arm A, THIS session, EVERY cell\n")
print(f"{'cell':<6} {'shaped':>10} {'util s42':>9} {'util s7':>9} {'headroom':>9}   claims permitted")
PERMIT = {}
for c in CELLS:
    us = {}
    for s in (42, 7):
        vals = []
        for r in LIVE[(c, "A")]:
            if r["seed"] != s or not r.get("tc_bytes") or not r.get("tc_s"):
                continue
            vals.append(100.0 * r["tc_bytes"] * 8.0 / (r["tc_s"] * SHAPED_BPS[c]))
        us[s] = med(vals)
    worst = max([u for u in us.values() if u is not None], default=None)
    hr = None if worst is None else 100.0 - worst
    PERMIT[c] = hr
    claim = ("(no tc datum — headroom UNKNOWN, no throughput target may be scored)"
             if hr is None else
             ("throughput targets permitted" if hr >= 5.0
              else "PARITY / LATENCY / CAP-SHAPE ONLY — headroom < 5% (discipline 16c)"))
    print(f"{c:<6} {SHAPED_BPS[c]//1_000_000:>7} Mb {fmt(us[42]):>9} {fmt(us[7]):>9} "
          f"{fmt(hr):>9}   {claim}")
print("\n  The pre-registration wrote NO throughput target at any cell: c7 and sc2")
print("  because they have none to give, c1 because the law has never run at 1 Gbit")
print("  in any layer, c8/c8L because the statistic there is bistable. This table is")
print("  the check that those decisions were made against the RIGHT arithmetic.")

# ── 4. THE [CCAP] GAUGE, AND THE STOP RULE ───────────────────────────────
print("\n### THE `[CCAP]` GAUGE — engagement, the two surviving bounds, the brake\n")
print(f"{'cell':<6} {'n':>4} {'eng med':>8} {'eng>=0.9':>9} {'cap med':>9} "
      f"{'mem>0.5':>8} {'floor>0.5':>10} {'brake med':>10} {'brake>0.005':>12}")
CCAP = {}
for c in CELLS:
    rs = [r for r in LIVE[(c, "C")] if r.get("ccap_lines")]
    if not rs:
        print(f"{c:<6} {0:>4}   (no [CCAP] on any live C rep — INSTRUMENT-FAIL, cell UNSCORED)")
        CCAP[c] = None
        continue
    d = {
        "n": len(rs),
        "eng": med([r["ccap_eng"] for r in rs]),
        "eng_ok": frac_at_least([r["ccap_eng"] for r in rs], ENG_BAR),
        "cap": med([r["ccap_cap"] for r in rs]),
        "mem_hi": frac_at_least([r["ccap_mem"] for r in rs], BIND_BAR),
        "floor_hi": frac_at_least([r["ccap_floor"] for r in rs], BIND_BAR),
        "brake": med([r["ccap_brake"] for r in rs]),
        "brake_ok": frac_at_least([r["ccap_brake"] for r in rs], BRAKE_BAR),
    }
    CCAP[c] = d
    print(f"{c:<6} {d['n']:>4} {fmt(d['eng'],3):>8} {fmt(d['eng_ok'],3):>9} {fmt(d['cap']):>9} "
          f"{fmt(d['mem_hi'],3):>8} {fmt(d['floor_hi'],3):>10} {fmt(d['brake'],4):>10} "
          f"{fmt(d['brake_ok'],3):>12}")

print("\n### CAPBIND — the standard bind-fraction readout (ADR-0070 kit item 2)\n")
for ln in capbind_lines(rows, cells=set(CELLS), arms=set(ARMS)):
    print("  " + ln)

print("\n" + "=" * 88)
print("THE STOP RULE — evaluated FIRST and PER CELL (discipline 18(d))")
print("=" * 88 + "\n")
VOID = {}
capbind_warn = {ln.split()[1].split("/")[0] + "/" + ln.split()[1].split("/")[1].rstrip(":")
                for ln in capbind_lines(rows, cells=set(CELLS), arms=set(ARMS))
                if ln.strip().startswith("WARN")}
for c in CELLS:
    d = CCAP.get(c)
    if d is None:
        VOID[c] = ["no [CCAP] gauge on any live C rep"]
        print(f"  {c:<5} UNSCORED — no [CCAP] gauge (the arm cannot be read as either a "
              f"null RESULT or a null EFFECT)")
        continue
    why = []
    if d["mem_hi"] is not None and d["mem_hi"] > 0.5:
        why.append(f"S1 mem>0.5 on {d['mem_hi']:.1%} of reps — THE MEMORY BOUND HAS BECOME THE LAW")
    if d["floor_hi"] is not None and d["floor_hi"] > 0.5:
        why.append(f"S2 floor>0.5 on {d['floor_hi']:.1%} of reps — the paroled constant binds")
    if d["eng_ok"] is not None and d["eng_ok"] <= 0.5:
        why.append(f"S3 eng>=0.9 on only {d['eng_ok']:.1%} of reps — WARM-UP FAILURE, not a null result")
    if f"{c}/C" in capbind_warn:
        why.append("S4 capbind_check WARNed for this (cell, C) group")
    VOID[c] = why
    if why:
        print(f"  {c:<5} UNSCORED:")
        for w in why:
            print(f"          {w}")
    else:
        print(f"  {c:<5} scored — the law engaged, and neither surviving bound is the law")
if all(VOID[c] for c in CELLS):
    print("\n  *** THE STOP RULE FIRED AT EVERY CELL: THE BATTERY IS UNSCORED IN FULL. ***")
    print("  The finding is that the composed law does not express itself on the wire —")
    print("  a result about the law's INPUTS, and a DIFFERENT result from the law losing.")

# ── 5. THE PREDICTIONS ───────────────────────────────────────────────────
print("\n" + "=" * 88)
print("THE PRE-REGISTERED PREDICTIONS, SCORED VERBATIM")
print("=" * 88)

print("\n[P-INTERIOR] C reads mem = 0 AND floor = 0 at EVERY cell; capbind = interior")
for c in CELLS:
    d = CCAP.get(c)
    if d is None:
        print(f"     {c:<5} no gauge — not scoreable")
        continue
    ok = (d["mem_hi"] == 0.0) and (d["floor_hi"] == 0.0)
    print(f"     {c:<5} mem>0.5 frac {fmt(d['mem_hi'],3)}  floor>0.5 frac {fmt(d['floor_hi'],3)} "
          f" ==> {'PASS (interior)' if ok else 'FAIL'}")

print(f"\n[P-PINNED] A re-measured PINNED in THIS session (the contrast P-INTERIOR is against)")
for c in CELLS:
    caps = [r["occcap_p50"] for r in LIVE[(c, "A")] if r.get("occcap_p50") is not None]
    if not caps:
        print(f"     {c:<5} no occcap datum")
        continue
    pin = A_PIN[c]
    k = sum(1 for x in caps if int(round(x)) == pin)
    print(f"     {c:<5} occcap_p50 == {pin} in {k}/{len(caps)} reps ({k/len(caps):.1%}) "
          f" ==> {'PASS (pinned, as in 178 prior dual reps)' if k/len(caps) > 0.5 else 'FAIL — A IS INTERIOR, the era moved; SCORE NOTHING until explained'}")

print(f"\n[P-ENGAGE] every C rep reads eng >= {ENG_BAR}")
for c in CELLS:
    d = CCAP.get(c)
    if d is None:
        continue
    print(f"     {c:<5} eng>=0.9 on {fmt(d['eng_ok'],3)} of reps, median eng {fmt(d['eng'],3)} "
          f" ==> {'PASS' if (d['eng_ok'] or 0) > 0.5 else 'FAIL — WARM-UP FAILURE, not a null result'}")

print(f"\n[P-BRAKE] brake_frac > {BRAKE_BAR} on a majority of C reps at every cell")
for c in CELLS:
    d = CCAP.get(c)
    if d is None:
        continue
    print(f"     {c:<5} brake>{BRAKE_BAR} on {fmt(d['brake_ok'],3)} of reps, median {fmt(d['brake'],4)} "
          f" ==> {'PASS' if (d['brake_ok'] or 0) > 0.5 else 'FAIL — a NULL EFFECT, not a null result; no claim about THE COMPOSITION'}")
duals = [CCAP[c]["brake"] for c in ("c8", "c8L") if CCAP.get(c) and CCAP[c]["brake"] is not None]
others = [CCAP[c]["brake"] for c in ("c1", "c7", "sc2") if CCAP.get(c) and CCAP[c]["brake"] is not None]
if duals and others:
    print(f"     asymmetric-dual clause: max(c8,c8L) = {max(duals):.4f} vs max(others) = {max(others):.4f}"
          f"  ==> {'PASS' if max(duals) > max(others) else 'FAIL (direction only; reported, not fatal)'}")

print("\n[P-CAPBAND] C's cap strictly below A's at the duals, and inside the pre-registered band")
for c in CELLS:
    d = CCAP.get(c)
    a_cap = med([r["occcap_p50"] for r in LIVE[(c, "A")]])
    c_cap = med([r["occcap_p50"] for r in LIVE[(c, "C")]])
    lo, hi = CAPBAND[c]
    if c_cap is None:
        print(f"     {c:<5} no C occcap datum")
        continue
    inband = lo <= c_cap <= hi
    below = (a_cap is None) or (c_cap < a_cap)
    print(f"     {c:<5} A {fmt(a_cap):>7}  C {fmt(c_cap):>7}  band [{lo}, {hi}] "
          f" in-band {'YES' if inband else 'NO'}  below-A {'YES' if below else 'NO'} "
          f" (CCAP mean cap {fmt(d['cap']) if d else '-'})")

# ── 6. GOODPUT / CPU / LATENCY ───────────────────────────────────────────
print("\n### THE TABLE — goodput (Mbit/s, mean ±2σ, live n), sender CPU beside it\n")
print(f"{'cell':<6} {'seed':>5} {'A':>20} {'C':>20} {'C/A':>7} {'2σ_Δ':>8} {'verdict':>10}")
GP = {}
for c in CELLS:
    for s in (42, 7):
        va = [r["mbps"] for r in LIVE[(c, "A")] if r["seed"] == s and r["mbps"] is not None]
        vc = [r["mbps"] for r in LIVE[(c, "C")] if r["seed"] == s and r["mbps"] is not None]
        ma, mc = mean(va), mean(vc)
        sa, sc_ = two_sigma(va), two_sigma(vc)
        if ma is None or mc is None:
            continue
        sd = math.sqrt((sa or 0) ** 2 + (sc_ or 0) ** 2)
        ratio = mc / ma
        verdict = "REGRESSION" if (mc < ma and (ma - mc) > sd) else "within 2σ"
        GP[(c, s)] = (ma, mc, ratio, sd, verdict)
        print(f"{c:<6} {s:>5} {f'{ma:.1f}±{fmt(sa)} ({len(va)})':>20} "
              f"{f'{mc:.1f}±{fmt(sc_)} ({len(vc)})':>20} {ratio:>7.3f} {sd:>8.1f} {verdict:>10}")

print(f"\n[G-REG] no cell more than 2σ down under C, either seed")
regs = [f"{c}-s{s}" for (c, s), v in GP.items() if v[4] == "REGRESSION"]
print(f"     ==> {'PASS' if not regs else 'FAIL at ' + ', '.join(regs)}")

print(f"\n[G-CPU] CPU/byte C <= {CPU_BAR}x A as a POINT band, every cell, both seeds")
print("     (a DISCRIMINATING prediction: C omits RWM_PLAIN_RS, whose 1.09-1.10x c7")
print("      class is 16.50's F4 blocker — C is expected to PASS at c7 where BHU failed)\n")
print(f"{'cell':<6} {'seed':>5} {'A CPU s':>9} {'C CPU s':>9} {'C/A':>7} {'verdict':>8}")
cpu_fail = []
for c in CELLS:
    for s in (42, 7):
        ca = mean([r["cpucli"] for r in LIVE[(c, "A")] if r["seed"] == s])
        cc = mean([r["cpucli"] for r in LIVE[(c, "C")] if r["seed"] == s])
        if ca is None or cc is None or ca == 0:
            continue
        rr = cc / ca
        ok = rr <= CPU_BAR
        if not ok:
            cpu_fail.append(f"{c}-s{s} ({rr:.3f})")
        print(f"{c:<6} {s:>5} {ca:>9.2f} {cc:>9.2f} {rr:>7.3f} {'PASS' if ok else 'FAIL':>8}")
print(f"     ==> G-CPU {'PASS' if not cpu_fail else 'FAIL at ' + ', '.join(cpu_fail)}")

print(f"\n[P-LATENCY-SC2 + G-SC2] the halved-RTT result must SURVIVE, at parity")
print(f"     bars: C p50 <= {SC2_P50_MAX} ms AND more than 2σ below SAME-SESSION A;"
      f" goodput within 2σ\n")
print(f"{'seed':>5} {'A p50 ±2σ':>18} {'C p50 ±2σ':>18} {'C/A':>7} {'2σ_Δ':>8} {'p99 C':>8} {'verdict':>8}")
for s in (42, 7):
    va = [r["ping_p50"] for r in LIVE[("sc2", "A")] if r["seed"] == s]
    vc = [r["ping_p50"] for r in LIVE[("sc2", "C")] if r["seed"] == s]
    ma, mc = mean(va), mean(vc)
    if ma is None or mc is None:
        continue
    sa, sc_ = two_sigma(va), two_sigma(vc)
    sd = math.sqrt((sa or 0) ** 2 + (sc_ or 0) ** 2)
    p99 = med([r["ping_p99"] for r in LIVE[("sc2", "C")] if r["seed"] == s])
    gpv = GP.get(("sc2", s))
    par = gpv is not None and abs(gpv[1] - gpv[0]) <= gpv[3]
    ok = (mc <= SC2_P50_MAX) and ((ma - mc) > sd) and par
    print(f"{s:>5} {f'{ma:.1f}±{fmt(sa)}':>18} {f'{mc:.1f}±{fmt(sc_)}':>18} {mc/ma:>7.3f} "
          f"{sd:>8.1f} {fmt(p99):>8} {'PASS' if ok else 'FAIL':>8}")
print("     (16.50's F6 context, NOT a bar: BHU read 42.6±8.1 / 44.2±8.6 against A's")
print("      96.2±13.6 / 93.9±15.8 — 0.44x/0.47x. C is not BHU; the bar above is")
print("      keyed to THIS session's A, per the mode-hunt lesson.)")

# ── 7. THE [WALL] GAUGE ──────────────────────────────────────────────────
print("\n### THE `[WALL]` GAUGE — the terminal window, per run (NOT a tick-share)\n")
print(f"{'cell':<6} {'arm':>4} {'n':>4} {'onset med':>10} {'dur_ms med':>11} {'dur_ms p90':>11} "
      f"{'retx med':>9} {'it_ms med':>10} {'| tick-share witness':>21}")
WALL = {}
for c in CELLS:
    for a in ARMS:
        rs = [r for r in LIVE[(c, a)] if r.get("wall_lines")]
        if not rs:
            continue
        durs = sorted(r["wall_dur_ms"] for r in rs)
        p90 = durs[min(len(durs) - 1, int(round(0.9 * (len(durs) - 1))))]
        dwv = [r["deadwall"] for r in rs if r.get("deadwall") is not None]
        wit = f"{sum(1 for x in dwv if x)}/{len(dwv)}" if dwv else "-"
        WALL[(c, a)] = med(durs)
        print(f"{c:<6} {a:>4} {len(rs):>4} {fmt(med([r['wall_onset'] for r in rs]),4):>10} "
              f"{fmt(med(durs)):>11} {fmt(p90):>11} "
              f"{fmt(med([r['wall_retx'] for r in rs]),0):>9} "
              f"{fmt(med([r['wall_it_ms'] for r in rs]),3):>10} {wit:>21}")
print("\n  The tick-share column is a WITNESS and is scored on NOTHING. It is here so")
print("  the old and new measurands can be compared on IDENTICAL reps — the only way")
print("  'the new one is stable' can be checked rather than asserted.")

print("\n[P-WALL-LENGTH] median dur_ms LARGER at c8 (25 MB) than at c8L (200 MB), BOTH arms")
for a in ARMS:
    d8, d8l = WALL.get(("c8", a)), WALL.get(("c8L", a))
    if d8 is None or d8l is None:
        print(f"     {a}: insufficient [WALL] data")
        continue
    print(f"     {a}: c8 {d8:.1f} ms vs c8L {d8l:.1f} ms "
          f" ==> {'PASS' if d8 > d8l else 'FAIL'}")

print("\n[S-WALL] THE INSTRUMENT'S OWN SCORED CLAIM — the arm ordering on dur_ms at")
print("         c8@25MB must NOT invert between the main pool and any top-up, nor")
print("         between seeds. This is the exact event that voided the predecessor.\n")
orderings = {}
for pool in ("main", "topup"):
    for s in (42, 7):
        da = med([r["wall_dur_ms"] for r in LIVE[("c8", "A")]
                  if r["_pool"] == pool and r["seed"] == s and r.get("wall_lines")])
        dc = med([r["wall_dur_ms"] for r in LIVE[("c8", "C")]
                  if r["_pool"] == pool and r["seed"] == s and r.get("wall_lines")])
        if da is None or dc is None:
            continue
        sg = 0 if dc == da else (1 if dc > da else -1)
        orderings[(pool, s)] = sg
        print(f"     pool={pool:<6} seed={s:<3} A {da:>8.1f} ms   C {dc:>8.1f} ms   "
              f"sign(C-A) = {sg:+d}")
sgs = set(orderings.values())
if len(orderings) < 2:
    print("     ==> UNDERPOWERED: fewer than two pools/seeds carry both arms. S-WALL "
          "is NOT scored,\n         and it is reported as unscored rather than as a pass.")
elif len(sgs) == 1:
    print("     ==> PASS — the ordering is STABLE across every pool and seed.")
else:
    print("     ==> FAIL — THE ORDERING INVERTED. Every [WALL]-scored claim in this")
    print("         session is UNSCORED, and the INSTRUMENT — not the law — is the")
    print("         finding. The goodput, cap and latency claims stand: none reads [WALL].")

# ── 8. THE LOAD-BEARING FALSIFIER ────────────────────────────────────────
print("\n" + "=" * 88)
print("THE LOAD-BEARING FALSIFIER — is any loss the LAW's, or the INSTRUMENTS'?")
print("=" * 88 + "\n")
print("  C losing where the cap is PROVABLY INTERIOR (mem = 0 AND floor = 0) and the")
print(f"  brake is PROVABLY ENGAGED (brake_frac > {BRAKE_PROVEN}) is THE LAW BEING WRONG.")
print("  No re-run, no 'the composition needs one more piece'.\n")
any_law = False
for c in CELLS:
    d = CCAP.get(c)
    if d is None:
        continue
    proven = (d["mem_hi"] == 0.0 and d["floor_hi"] == 0.0
              and (d["brake"] or 0) > BRAKE_PROVEN)
    for s in (42, 7):
        v = GP.get((c, s))
        if v is None or v[4] != "REGRESSION":
            continue
        any_law = any_law or proven
        tag = ("THE LAW IS WRONG (interior + engaged + lost)" if proven
               else "an INSTRUMENT finding — the law did not express itself here")
        print(f"     {c}-s{s}: C/A = {v[2]:.3f}, more than 2σ down  ==> {tag}")
if not any_law:
    print("     No regression sits on a provably-interior, provably-engaged cell.")
print()
