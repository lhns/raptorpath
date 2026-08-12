#!/usr/bin/env python3
"""Scoring pass for THE MODE-HUNT BATTERY.

Scored against goal-gate "Mode-Hunt Battery — VM PRE-REGISTRATION" (commit
`b8fd6d9`), which is the CONTRACT: this file implements its clauses and
inverts none of them. Every bar below is transcribed from that block; nothing
here decides anything the pre-registration did not already fix.

  usage: modehunt_report.py <ledger.log> [<ledger.log> ...]

WHAT IS AND IS NOT A DENOMINATOR (the pre-registration's own split):

  ABORT            no `[GATES]` on EITHER endpoint. NOT in any denominator.
                   `ARMCOUNT` in the driver counts HEADERS and therefore
                   counts aborts too — it is a vanish-detector, not an n.
                   The live n is recomputed here from the gates columns.
  DNF              a completed run that did not transfer. IS a datum, IS in
                   the denominator, reported separately.
  INSTRUMENT-FAIL  completed but a gauge did not report. Excluded from the
                   statistic it voids, WITH THE EXCLUSION COUNTED. Here that
                   is `deadwall is None` (the wait histogram never populated).

NO MEANS. The primary statistic is a per-rep binary and it is reported as a
RATE with a 95% Wilson interval. The guards are MEDIANS — c8's 2-sigma band is
42-46% of its own mean, which is why the pre-registration forbids a mean-based
contrast at this cell.
"""
import json
import math
import sys
from collections import defaultdict

# ── The transcribed constants. These are the PREDECESSOR's numbers, quoted,
#    never recomputed here (goal-gate "Dead-Wall Battery — RESULTS").
PRED_AU = 0.727          # 8/11, the predecessor's AU point rate
S1_BAR = 0.40            # the reproduction / stop bar
KILL_UPPER = 0.25        # the kill bar's absolute ceiling clause
GOODPUT_GUARD = 0.85     # G-GOODPUT class guard
LATENCY_GUARD = 1.25     # G-LATENCY bar, ratio to AU


def wilson(k, n, z=1.96):
    """95% Wilson score interval. Defined at k=0 and k=n, which is why it is
    the interval the pre-registration names."""
    if n == 0:
        return (float("nan"), float("nan"))
    p = k / n
    d = 1.0 + z * z / n
    c = p + z * z / (2 * n)
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return ((c - h) / d, (c + h) / d)


def med(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2.0


def fisher(a, b, c, d):
    """Two-sided Fisher exact on [[a,b],[c,d]]. Used ONLY to ask whether two
    pools of the SAME arm at the SAME seed are exchangeable — a heterogeneity
    check on the instrument, never a treatment contrast."""
    n = a + b + c + d
    r1, r2, c1 = a + b, c + d, a + c

    def pr(x):
        return (math.comb(r1, x) * math.comb(r2, c1 - x)) / math.comb(n, c1)

    obs = pr(a)
    lo = max(0, c1 - r2)
    hi = min(r1, c1)
    return sum(pr(x) for x in range(lo, hi + 1) if pr(x) <= obs * (1 + 1e-9))


rows = []
for path in sys.argv[1:]:
    pool = "topup" if "topup" in path.replace("\\", "/").split("/")[-1] else "main"
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

# ── Live / abort / voided split, per the definitions above.
by = defaultdict(list)
for r in rows:
    by[(r["cell"], r["arm"])].append(r)

CELLARMS = [("c8", "A"), ("c8", "AU"), ("c8", "AUR"), ("c8L", "AU")]

print("=" * 78)
print("MODE-HUNT BATTERY — SCORING PASS")
print("contract: goal-gate \"Mode-Hunt Battery — VM PRE-REGISTRATION\" (b8fd6d9)")
print("=" * 78)

print("\n### INVOCATION ACCOUNTING (abort != DNF != INSTRUMENT-FAIL)\n")
print(f"{'cell-arm':<10} {'headers':>8} {'ABORT':>6} {'live':>6} {'voided':>7} {'scored':>7} {'DNF':>5}")
stat = {}
for ck in CELLARMS:
    rs = by.get(ck, [])
    hdr = len(rs)
    live = [r for r in rs if r.get("gates_lines_cli") or r.get("gates_lines_srv")]
    abort = hdr - len(live)
    scored = [r for r in live if r.get("deadwall") is not None]
    voided = len(live) - len(scored)
    dnf = sum(1 for r in scored if r.get("dnf"))
    stat[ck] = scored
    print(f"{ck[0]+'-'+ck[1]:<10} {hdr:>8} {abort:>6} {len(live):>6} {voided:>7} {len(scored):>7} {dnf:>5}")

# ── Two-sided echo audit, from the parsed columns (not the driver's flags).
print("\n### LIVENESS, TWO-SIDED, RECOMPUTED FROM THE COLUMNS\n")
print(f"{'cell-arm':<10} {'GATES U cli/srv':>16} {'GATES DS cli/srv':>17} {'actU':>8} {'actDS':>8} {'divDS':>8}")
for ck in CELLARMS:
    rs = stat[ck]
    if not rs:
        continue
    exp_u = 1 if ck[1] in ("AU", "AUR") else 0
    exp_d = 1 if ck[1] == "AUR" else 0
    gu = sum(1 for r in rs if r["gates_cli_u"] == exp_u and r["gates_srv_u"] == exp_u)
    gd = sum(1 for r in rs if r["gates_cli_ds"] == exp_d and r["gates_srv_ds"] == exp_d)
    au = sum(1 for r in rs if (r["active_u_cli"] > 0) == bool(exp_u) and (r["active_u_srv"] > 0) == bool(exp_u))
    ad = sum(1 for r in rs if (r["active_ds_cli"] > 0 or r["active_ds_srv"] > 0) == bool(exp_d))
    dd = sum(1 for r in rs if (r["diverged_ds_cli"] > 0 or r["diverged_ds_srv"] > 0) == bool(exp_d))
    n = len(rs)
    print(f"{ck[0]+'-'+ck[1]:<10} {f'{gu}/{n} (exp {exp_u})':>16} {f'{gd}/{n} (exp {exp_d})':>17} "
          f"{f'{au}/{n}':>8} {f'{ad}/{n}':>8} {f'{dd}/{n}':>8}")

# ── THE PRIMARY STATISTIC.
print("\n### THE DEAD-WALL RATE (per-rep: wait_tun == 0 AND wait_paused == 0)\n")
print(f"{'cell-arm':<10} {'s42':>8} {'s7':>8} {'pooled':>9} {'rate':>7}   95% Wilson CI")
rate = {}
for ck in CELLARMS:
    rs = stat[ck]
    if not rs:
        continue
    k = sum(1 for r in rs if r["deadwall"])
    n = len(rs)
    per = {}
    for s in (42, 7):
        ss = [r for r in rs if r["seed"] == s]
        per[s] = (sum(1 for r in ss if r["deadwall"]), len(ss))
    lo, hi = wilson(k, n)
    rate[ck] = (k, n, k / n, lo, hi)
    print(f"{ck[0]+'-'+ck[1]:<10} {f'{per[42][0]}/{per[42][1]}':>8} {f'{per[7][0]}/{per[7][1]}':>8} "
          f"{f'{k}/{n}':>9} {k/n:>7.3f}   [{lo:.3f}, {hi:.3f}]")

# ── POOL PROVENANCE + heterogeneity. The top-up is pooled INTO the scoring
#    set (the §16.52 precedent: its collapse reps are what moved that verdict),
#    and its own contribution is printed so the pooling is auditable rather
#    than retrofitted.
print("\n### POOL PROVENANCE — main vs symmetric top-up, and are they exchangeable?\n")
print(f"{'cell-arm':<10} {'pool':>6} {'seed':>5} {'k/n':>8} {'rate':>7}")
for ck in CELLARMS:
    for pool in ("main", "topup"):
        for s in (42, 7):
            rs = [r for r in stat[ck] if r["_pool"] == pool and r["seed"] == s]
            if not rs:
                continue
            k = sum(1 for r in rs if r["deadwall"])
            print(f"{ck[0]+'-'+ck[1]:<10} {pool:>6} {s:>5} {f'{k}/{len(rs)}':>8} {k/len(rs):>7.3f}")

print("\n  Heterogeneity, SAME arm / SAME seed / SAME binary, main vs top-up")
print("  (Fisher exact, two-sided — an instrument check, NOT a treatment contrast):\n")
for ck in CELLARMS:
    m = [r for r in stat[ck] if r["_pool"] == "main" and r["seed"] == 7]
    t = [r for r in stat[ck] if r["_pool"] == "topup" and r["seed"] == 7]
    if not m or not t:
        continue
    a = sum(1 for r in m if r["deadwall"])
    c = sum(1 for r in t if r["deadwall"])
    p = fisher(a, len(m) - a, c, len(t) - c)
    verdict = "HETEROGENEOUS" if p < 0.05 else "exchangeable at p>=0.05"
    print(f"  {ck[0]+'-'+ck[1]:<10} main {a}/{len(m)} vs topup {c}/{len(t)}   Fisher p = {p:.4f}   {verdict}")

# ── GUARDS (medians only; no means at this cell).
print("\n### GUARDS — medians, and ratios against AU (the baseline)\n")
base = stat[("c8", "AU")]
b_gp, b_pp = med([r["mbps"] for r in base]), med([r["ping_p99"] for r in base])
print(f"{'cell-arm':<10} {'goodput':>9} {'/AU':>7} {'ping_p99':>9} {'/AU':>7} "
      f"{'wait_tun':>9} {'wait_paus':>10} {'sf_zero':>8} {'retx':>7}")
guard = {}
for ck in CELLARMS:
    rs = stat[ck]
    if not rs:
        continue
    gp, pp = med([r["mbps"] for r in rs]), med([r["ping_p99"] for r in rs])
    wt, wp = med([r["wait_tun"] for r in rs]), med([r["wait_paused"] for r in rs])
    sz, rx = med([r["sf_zero"] for r in rs]), med([r["retx"] for r in rs])
    guard[ck] = (gp, gp / b_gp if b_gp else None, pp, pp / b_pp if b_pp else None)
    print(f"{ck[0]+'-'+ck[1]:<10} {gp:>9.1f} {gp/b_gp:>7.3f} {pp:>9.1f} {pp/b_pp:>7.3f} "
          f"{wt:>9} {wp:>10} {sz:>8.3f} {rx:>7.0f}")

# ── THE BARS, verbatim.
print("\n" + "=" * 78)
print("THE PRE-REGISTERED BARS, SCORED VERBATIM")
print("=" * 78)

k_au, n_au, p_au, lo_au, hi_au = rate[("c8", "AU")]

print(f"\n[S1] THE REPRODUCTION / STOP BAR — p_AU pooled >= {S1_BAR}")
print(f"     p_AU = {k_au}/{n_au} = {p_au:.3f}, 95% CI [{lo_au:.3f}, {hi_au:.3f}]")
s1 = p_au >= S1_BAR
print(f"     ==> {'PASS — the baseline carries the mode; treatment rows ARE scored' if s1 else 'FAIL — BATTERY REPORTED UNSCORED'}")
print(f"     (predecessor AU = {PRED_AU} [0.434, 0.903], SAME BINARY -> pure substrate test)")

for arm in ("AUR",):
    k, n, p, lo, hi = rate[("c8", arm)]
    print(f"\n[T-KILL] {arm}: CI excludes p_AU={p_au:.3f} AND CI upper < {KILL_UPPER}")
    print(f"     p_{arm} = {k}/{n} = {p:.3f}, 95% CI [{lo:.3f}, {hi:.3f}]")
    ex = not (lo <= p_au <= hi)
    up = hi < KILL_UPPER
    print(f"     clause 1 (CI excludes p_AU):   {'PASS' if ex else 'FAIL'}")
    print(f"     clause 2 (CI upper < {KILL_UPPER}):    {'PASS' if up else 'FAIL'} (upper = {hi:.3f})")
    print(f"     ==> T-KILL {'MET' if (ex and up) else 'NOT MET'}")

    print(f"\n[T-HALVE] {arm}: CI excludes the transcribed {PRED_AU}")
    hv = not (lo <= PRED_AU <= hi)
    print(f"     ==> T-HALVE {'MET' if hv else 'NOT MET'} ({PRED_AU} {'outside' if hv else 'INSIDE'} [{lo:.3f}, {hi:.3f}])")

    gp, gpr, pp, ppr = guard[("c8", arm)]
    print(f"\n[G-GOODPUT] {arm}/AU median goodput >= {GOODPUT_GUARD}")
    print(f"     {gp:.1f} / {b_gp:.1f} = {gpr:.3f}  ==> {'PASS (class intact)' if gpr >= GOODPUT_GUARD else 'FAIL — any kill here is a TRADE'}")
    print(f"\n[G-LATENCY] {arm}/AU median ping_p99 <= {LATENCY_GUARD}")
    print(f"     {pp:.1f} / {b_pp:.1f} = {ppr:.3f}  ==> {'PASS' if ppr <= LATENCY_GUARD else 'FAIL — TRADE'}")

# ── The pin.
k_a, n_a, p_a, lo_a, hi_a = rate[("c8", "A")]
print(f"\n[ERA PIN] A = {k_a}/{n_a} = {p_a:.3f}, 95% CI [{lo_a:.3f}, {hi_a:.3f}]")
print(f"     falsifier: pin ABOVE {KILL_UPPER} would void the session.")
print(f"     ==> {'PASS (era did not move)' if p_a <= KILL_UPPER else 'FAIL — SESSION VOID'}")
print("     NOT A CONTRAST. Pre-disqualified at n=8 by the pre-registration.")

# ── C8L.
k_l, n_l, p_l, lo_l, hi_l = rate[("c8L", "AU")]
print(f"\n[C8L] THE BYTE-COUNT ARTIFACT, asked on the arm that fires")
print(f"     p_AU(200MB) = {k_l}/{n_l} = {p_l:.3f}, 95% CI [{lo_l:.3f}, {hi_l:.3f}]")
print(f"     p_AU(25MB)  = {k_au}/{n_au} = {p_au:.3f}, 95% CI [{lo_au:.3f}, {hi_au:.3f}]")
conf = (not (lo_l <= p_au <= hi_l)) and hi_l < KILL_UPPER
overlap = not (hi_l < lo_au or hi_au < lo_l)
ref = overlap and p_l >= 0.5 * p_au
print(f"     ARTIFACT CONFIRMED (CI excludes p_AU(25) AND upper < {KILL_UPPER}): {'YES' if conf else 'no'}")
print(f"     ARTIFACT REFUTED  (CIs overlap AND point >= 0.5*p_AU(25) = {0.5*p_au:.3f}): {'YES' if ref else 'no'}")
if conf:
    print("     ==> ARTIFACT CONFIRMED")
elif ref:
    print("     ==> ARTIFACT REFUTED — the wall survives an 8x transfer")
else:
    print("     ==> UNDERPOWERED, reported as underpowered (the pre-registered third branch)")
print()
