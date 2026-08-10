#!/usr/bin/env python3
"""Three-Term Law battery — the ledger table, computed from the raw logs.

Reads every `TTRESULT {json}` line the battery drivers emitted and prints:

  (1) LIVENESS  per cell/arm/seed: n, the two-sided [GATES] agreement, the
      resolve-time ACTIVE echo, and the `[3T] eng=1` count. Discipline 15:
      an arm without a verified echo is VOID. This table is printed FIRST
      and no goodput below it may be read if it fails.
  (2) LAW       the measured median window/slack/span/cap against the
      pre-registered table (criterion 3's symbols-and-bytes comparison).
  (3) GOODPUT   per cell/arm/seed mean, sigma, n, and the B/A ratio with
      its noise bound (discipline 5: an effect must exceed the recorded
      same-config spread or be reported as within it).
  (4) SIGMA     c7 vs 2*sc2 and c8 vs sc2+sc3, same session, same arm.

usage: tt_report.py <log> [<log> ...]
"""
import json
import math
import re
import sys
from collections import defaultdict

# The PRE-REGISTERED table (goal-gate "Three-Term Law — PRE-REGISTRATION",
# commit 70833cd). Transcribed, never recomputed: cell -> (K@4ms, K=1,
# window, slack, span, OFF, predicted throughput direction).
PREREG = {
    "jit25":  (1430, 1300, 458, 972,   0, 1024, "UP +5..+15%"),
    "shal8":  (455,   325, 146, 309,   0, 1024, "flat +/-3% (PREDICTED NULL)"),
    "c2r100": (3380, 3250, 1082, 2298, 0, 1024, "UP +25..+60%"),
    "c2r200": (4096, 4096, 2122, 4508, 0, 1024, "CLAMPED (B2) - no verdict"),
    "c1":     (488,   163, 156, 332,   0, 1024, "flat to +5%"),
    "c7":     (910,   650, 291, 619,   0, 4096, "flat +/-3%"),
    "c8":     (1042,  887, 234, 496, 312, 4096, "UP +10..+25%"),
}

rows = []
for path in sys.argv[1:]:
    try:
        for ln in open(path, errors="replace"):
            i = ln.find("TTRESULT ")
            if i < 0:
                continue
            try:
                rows.append(json.loads(ln[i + 9:]))
            except Exception:
                pass
    except OSError as e:
        print(f"# could not read {path}: {e}")

if not rows:
    print("no TTRESULT rows found")
    sys.exit(1)


def key(r):
    return (r["cell"], r["arm"], r["seed"])


by = defaultdict(list)
for r in rows:
    by[key(r)].append(r)


def mean(v):
    return sum(v) / len(v) if v else None


def sd(v):
    if len(v) < 2:
        return 0.0
    m = mean(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def f(x, n=1):
    return "-" if x is None else f"{x:.{n}f}"


cells = [c for c in ["c1", "c7", "c8", "sc2", "sc3", "c2r100", "c2r200",
                     "jit25", "shal8"] if any(r["cell"] == c for r in rows)]
extra = sorted({r["cell"] for r in rows} - set(cells))
arms = ["A", "B", "C", "D"]
seeds = sorted({r["seed"] for r in rows})

print("=" * 100)
print("(1) LIVENESS — discipline 1/15. An arm without a verified [3T] eng=1 echo is VOID.")
print("=" * 100)
print(f"{'cell':<8}{'arm':<4}{'seed':<6}{'n':<4}{'gates cli/srv 3T':<18}"
      f"{'rs cli/srv':<12}{'ACTIVE':<8}{'eng1 lines':<12}{'flags'}")
void = []
for c in cells + extra:
    for a in arms:
        for s in seeds:
            rs = by.get((c, a, s))
            if not rs:
                continue
            g3 = {(r["gates_cli_3t"], r["gates_srv_3t"]) for r in rs}
            gr = {(r["gates_cli_rs"], r["gates_srv_rs"]) for r in rs}
            act = sum(1 for r in rs if r["active_echo_cli"] > 0)
            eng = [r["tt_eng1"] for r in rs]
            flags = []
            exp3 = 1 if a in ("B", "C") else 0
            expr = 1 if a in ("B", "D") else 0
            if g3 != {(exp3, exp3)}:
                flags.append("GATE-MISMATCH")
            if gr != {(expr, expr)}:
                flags.append("RS-MISMATCH")
            if exp3 == 1 and min(eng) == 0:
                flags.append(f"VOID-NO-ENG1({sum(1 for e in eng if e == 0)}/{len(eng)})")
                void.append((c, a, s))
            if exp3 == 0 and max(eng) > 0:
                flags.append("CONTAMINATION")
            print(f"{c:<8}{a:<4}{s:<6}{len(rs):<4}"
                  f"{str(sorted(g3)):<18}{str(sorted(gr)):<12}"
                  f"{act}/{len(rs):<6}{min(eng)}..{max(eng):<9}"
                  f"{' '.join(flags) if flags else 'OK'}")
print(f"\nVOID arms: {void if void else 'NONE'}")

print()
print("=" * 100)
print("(2) THE LAW — measured [3T] terms (median over eng=1 lines) vs the PRE-REGISTERED table")
print("=" * 100)
print(f"{'cell':<8}{'arm':<4}{'seed':<6}{'cap':<8}{'window':<10}{'slack':<10}"
      f"{'span':<10}{'rho':<8}{'b':<6}{'| pred(K@4ms/K=1)':<20}{'ratio cap/pred'}")
for c in cells + extra:
    for a in ["B", "C"]:
        for s in seeds:
            rs = by.get((c, a, s))
            if not rs:
                continue
            capv = [r["cap_med"] for r in rs if r["cap_med"] is not None]
            wv = [r["window_med"] for r in rs if r["window_med"] is not None]
            sv = [r["slack_med"] for r in rs if r["slack_med"] is not None]
            pv = [r["span_med"] for r in rs if r["span_med"] is not None]
            rho = sorted({x for r in rs for x in (r["rho"] or [])})
            bb = sorted({x for r in rs for x in (r["b"] or [])})
            p = PREREG.get(c)
            pred = f"{p[0]}/{p[1]}" if p else "-"
            ratio = (mean(capv) / p[0]) if (p and capv) else None
            print(f"{c:<8}{a:<4}{s:<6}{f(mean(capv),0):<8}{f(mean(wv)):<10}"
                  f"{f(mean(sv)):<10}{f(mean(pv),2):<10}{str(rho):<8}{str(bb):<6}"
                  f"| {pred:<18}{f(ratio,2)}")

print()
print("SPAN CHECK (F3: the span term must be 0 at every single path AND at c7)")
for c in cells + extra:
    for a in ["B", "C"]:
        mx = [r["span_max"] for r in rows
              if r["cell"] == c and r["arm"] == a and r["span_max"] is not None]
        if mx:
            worst = max(mx)
            tag = "OK" if (c == "c8" or worst == 0.0) else ("ZERO" if worst == 0.0 else "NON-ZERO")
            print(f"  {c:<8}{a}  span_max over all reps = {worst:.4f}   {tag}")

print()
print("F2 CHECK — slack/window at rho=1 must be 17/8 = 2.125 EXACTLY, whatever delta is.")
print("  (comparing `cap` between hints cannot test F2: the limit is linear in the measured")
print("   rate and three invocations achieve three different rates. This ratio cancels it.)")
for c in cells + extra:
    for a in ["B", "C"]:
        v = [r["sw_ratio_med"] for r in rows
             if r["cell"] == c and r["arm"] == a and r.get("sw_ratio_med") is not None]
        lo = [r["sw_ratio_min"] for r in rows
              if r["cell"] == c and r["arm"] == a and r.get("sw_ratio_min") is not None]
        hi = [r["sw_ratio_max"] for r in rows
              if r["cell"] == c and r["arm"] == a and r.get("sw_ratio_max") is not None]
        if v:
            dev = max(abs(x - 2.125) for x in (lo + hi))
            print(f"  {c:<8}{a}  slack/window med={mean(v):.6f}  "
                  f"range=[{min(lo):.6f},{max(hi):.6f}]  worst|dev from 17/8|={dev:.2e}  "
                  + ("OK" if dev < 1e-6 else "DEVIATES"))

print()
print("=" * 100)
print("(3) GOODPUT — mean +/- sigma (n), and B/A with its noise bound")
print("    (D = RWM_PLAIN_RS=1 alone: the ATTRIBUTION control, not scored)")
print("=" * 100)
print(f"{'cell':<8}{'seed':<6}{'A mean+/-sd (n)':<22}{'B mean+/-sd (n)':<22}"
      f"{'C mean+/-sd (n)':<22}{'D mean+/-sd (n)':<22}{'B/A':<8}{'D/A':<8}{'|B-A| vs 2sd'}")
for c in cells + extra:
    for s in seeds:
        out = []
        stats = {}
        for a in arms:
            rs = by.get((c, a, s), [])
            v = [r["mbps"] for r in rs if r["mbps"] is not None and not r["dnf"]]
            stats[a] = (mean(v), sd(v), len(v))
            out.append(f"{f(mean(v),1)}+/-{f(sd(v),1)} ({len(v)})")
        ma, sda, _ = stats["A"]
        mb, sdb, _ = stats["B"]
        md, _, _ = stats["D"]
        ratio = (mb / ma) if (ma and mb) else None
        dratio = (md / ma) if (ma and md) else None
        bound = ""
        if ma is not None and mb is not None:
            pooled = math.sqrt(sda ** 2 + sdb ** 2)
            d = abs(mb - ma)
            bound = f"{d:.1f} vs {2*pooled:.1f} -> " + ("EXCEEDS" if d > 2 * pooled else "WITHIN NOISE")
        print(f"{c:<8}{s:<6}{out[0]:<22}{out[1]:<22}{out[2]:<22}{out[3]:<22}"
              f"{f(ratio,3):<8}{f(dratio,3):<8}{bound}")

print()
print("DNF (criterion: 0)")
tot = len(rows)
d = [r for r in rows if r["dnf"]]
print(f"  dnf runs: {len(d)}/{tot}" + ("" if not d else
      "  -> " + ", ".join(f'{r["cell"]}-{r["arm"]}-s{r["seed"]}-r{r["rep"]}' for r in d)))

print()
print("=" * 100)
print("(4) SIGMA — same-session, SAME-ARM single-path sum (c7 = 2*sc2, c8 = sc2+sc3)")
print("=" * 100)
print(f"{'dual':<6}{'arm':<4}{'seed':<6}{'dual mbps':<12}{'Sigma':<12}{'dual/Sigma':<12}{'target':<10}{'verdict'}")
for a in arms:
    for s in seeds:
        def gm(c):
            v = [r["mbps"] for r in by.get((c, a, s), [])
                 if r["mbps"] is not None and not r["dnf"]]
            return mean(v)
        sc2, sc3, c7, c8 = gm("sc2"), gm("sc3"), gm("c7"), gm("c8")
        if sc2 and c7:
            sig = 2 * sc2
            print(f"{'c7':<6}{a:<4}{s:<6}{f(c7,1):<12}{f(sig,1):<12}"
                  f"{f(c7/sig,3):<12}{'>=0.97':<10}"
                  f"{'PASS' if c7/sig >= 0.97 else 'FAIL'}")
        if sc2 and sc3 and c8:
            sig = sc2 + sc3
            print(f"{'c8':<6}{a:<4}{s:<6}{f(c8,1):<12}{f(sig,1):<12}"
                  f"{f(c8/sig,3):<12}{'>=0.87':<10}"
                  f"{'PASS' if c8/sig >= 0.87 else 'FAIL'}")

print()
print("=" * 100)
print("(5) OCCUPANCY — measured store occupancy win=<outstanding>/<cap> (steady state)")
print("=" * 100)
print(f"{'cell':<8}{'arm':<4}{'seed':<6}{'occ p50':<10}{'occ p90':<10}{'occ max':<10}{'cap seen':<10}")
for c in cells + extra:
    for a in arms:
        for s in seeds:
            rs = by.get((c, a, s), [])
            o5 = [r["occ_p50"] for r in rs if r["occ_p50"] is not None]
            o9 = [r["occ_p90"] for r in rs if r["occ_p90"] is not None]
            om = [r["occ_max"] for r in rs if r["occ_max"] is not None]
            oc = [r["occcap_p50"] for r in rs if r["occcap_p50"] is not None]
            if o5:
                print(f"{c:<8}{a:<4}{s:<6}{f(mean(o5),0):<10}{f(mean(o9),0):<10}"
                      f"{f(max(om),0):<10}{f(mean(oc),0):<10}")
