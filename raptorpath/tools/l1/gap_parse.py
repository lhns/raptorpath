#!/usr/bin/env python3
"""Collate the missing-half battery's ledger and print the readings the
contract's branch rules are taken on.

    python3 gap_parse.py [--calib] <ledger.log> [<ledger.log> ...]

Reads ONLY lines this battery's driver wrote, so it runs against a committed
ledger with no VM and no binary.

THE TWO PUBLISHED REFERENCES ARE HARD-CODED HERE, WITH THEIR PROVENANCE, and
that is deliberate: the whole battery is a comparison against numbers another
session produced, so those numbers must be in the artifact that does the
comparing rather than transcribed by hand into a results table.

WHAT THIS SCRIPT DOES NOT DO: it does not select a branch. It prints the two
readings (the LEVEL and the RATIO) with their resolution, and the contract's
2x2 rule is applied by a human against the pre-registered bands. A parser that
picked the branch would be picking the verdict.
"""
import argparse
import math
import re
import statistics
import sys
from collections import defaultdict

# ── THE PUBLISHED REFERENCES ────────────────────────────────────────────
# Flip week, 2026-08-08, goal-gate "Ack-Merge Flip ... L1 BATTERY RESULTS",
# binary sha256 fbd6b279..., cell c1/c1 single 400 MB, RWM_GEN=0 RWM_DIAG=1,
# n = 8 per arm per seed.  `prior` = env unset, `am` = RWM_ACK_MERGE=1.
FLIP = {
    42: {"prior": (203.1, 8.5, 8), "am": (228.9, 4.2, 8), "pct": 12.7},
    7:  {"prior": (201.8, 5.6, 8), "am": (228.1, 3.1, 8), "pct": 13.0},
}
# Era week, 2026-08-19, goal-gate "Era Battery - THE SCORED RESULT" E-GOOD.
# SAME binary (4171b58 == c2bfab7 for the engine; both sha256 fbd6b279...),
# SAME cell, n = 8 per arm per seed, but the ERA session's env (which adds
# RWM_ACKDIAG=1 RWM_WALLDIAG=1 RWM_LATPROBE=1).
ERA = {
    42: {"OLD": 175.25, "NEW": 187.23, "two_sigma": 82.74, "pct": 6.84},
    7:  {"OLD": 192.06, "NEW": 210.53, "two_sigma": 10.88, "pct": 9.62},
}


def parse(paths):
    rows = defaultdict(list)   # (cell, arm, seed) -> [dict]
    ctld = defaultdict(list)   # (cell, arm, seed) -> [float]
    ctld_last = {}             # (cell, arm, seed, rep) -> float  (last wins)
    pings = defaultdict(list)  # (cell, arm, seed) -> [int attempts]
    steals = defaultdict(list) # (cell, arm, seed) -> [pct_nonidle per invocation]
    problems = []
    for p in paths:
        # `CTLDLINE`/`PINGROW` carry no `seed=`, but the driver emits them AFTER
        # the `GAPROW` of the same invocation, so the last GAPROW's seed is this
        # invocation's seed. Tracked rather than dropped, because phase 2's
        # readings are per-seed and a seed-pooled density hides a split.
        cur_seed = None
        for line in open(p, "r", errors="replace"):
            line = line.rstrip("\n")
            if line.startswith("GAPROW "):
                toks = line.split()
                cell_arm = toks[1]
                kv = dict(t.split("=", 1) for t in toks[2:] if "=" in t)
                cell, _, arm = cell_arm.rpartition("-")
                seed = int(kv.get("seed", 0))
                cur_seed = seed
                out = {}
                for k, v in kv.items():
                    try:
                        out[k] = float(v)
                    except ValueError:
                        out[k] = v
                rows[(cell, arm, seed)].append(out)
            elif line.startswith("CTLDLINE ") and " site=srv " in line:
                toks = line.split()
                cell, _, arm = toks[1].rpartition("-")
                # THE DENSITY IS `tx/rx`, AND THE DRIVER EMITS THEM AS TWO
                # SEPARATE `tx=<n> rx=<n>` TOKENS — never as one `a/b` token.
                # The original single-token scan therefore matched NOTHING on
                # every ledger this battery has ever produced and printed no
                # mechanism table at all, silently. Phase 1's densities had to
                # be computed outside the parser; this is that fix.
                # `[CTLD]` counters are CUMULATIVE and re-emitted many times per
                # invocation, so only the LAST line of each rep is the rep's
                # density. Keyed by rep and overwritten, never averaged over the
                # intermediate snapshots.
                rep = None
                for t in toks:
                    if t.startswith("rep="):
                        rep = t[4:]
                m = re.search(r"\btx=(\d+)\s+rx=(\d+)", line)
                if m and int(m.group(2)) > 0:
                    ctld_last[(cell, arm, cur_seed, rep)] = (
                        int(m.group(1)) / int(m.group(2)))
                    continue
                # Fallback for any era that DID emit a single `a/b` token.
                vals = []
                for t in toks:
                    if "/" in t and t.count("/") == 1:
                        a, b = t.split("/")
                        try:
                            a, b = float(a), float(b)
                            if b > 0:
                                vals.append(a / b)
                        except ValueError:
                            pass
                if vals:
                    ctld_last[(cell, arm, cur_seed, rep)] = vals[-1]
            elif line.startswith("STEALROW "):
                toks = line.split()
                cell, _, arm = toks[1].rpartition("-")
                kv = dict(t.split("=", 1) for t in toks[2:] if "=" in t)
                try:
                    steals[(cell, arm, int(kv.get("seed", cur_seed or 0)))].append(
                        float(kv["pct_nonidle"]))
                except (KeyError, ValueError):
                    pass
            elif line.startswith("PINGROW "):
                # The topo-ping repair's retry histogram, now kept on EVERY
                # invocation rather than only on the aborts (see the amendment).
                toks = line.split()
                cell, _, arm = toks[1].rpartition("-")
                kv = dict(t.split("=", 1) for t in toks[2:] if "=" in t)
                for leg in ("pathA_attempts", "pathB_attempts"):
                    v = kv.get(leg)
                    if v not in (None, "NA"):
                        try:
                            pings[(cell, arm, int(kv.get("seed", cur_seed or 0)))
                                  ].append(int(v))
                        except ValueError:
                            pass
            elif line.startswith(("ABORT ", "INSTRUMENT-FAIL", "ARM-CONTAMINATION",
                                  "ARM-LIVENESS-FAIL", "G-ERA-VIOLATION",
                                  "LIVENESS-FAIL", "ERA-SURPRISE", "ARM-VANISHED",
                                  "QCAP-MISSING", "MISSING BINARY", "G-SHA")):
                problems.append(line)
    for (cell, arm, seed, _rep), v in ctld_last.items():
        ctld[(cell, arm, seed)].append(v)
    return rows, ctld, pings, steals, problems


def stat(vals):
    vals = [v for v in vals if isinstance(v, float) and not math.isnan(v)]
    if not vals:
        return None, None, 0
    if len(vals) == 1:
        return vals[0], 0.0, 1
    return statistics.mean(vals), statistics.stdev(vals), len(vals)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("logs", nargs="+")
    ap.add_argument("--calib", action="store_true")
    ap.add_argument("--nproc", type=int, default=6)
    ap.add_argument("--target-pct", type=float, default=5.0,
                    help="the goodput difference the design is sized to resolve")
    a = ap.parse_args()

    rows, ctld, pings, steals, problems = parse(a.logs)
    if not rows:
        print("NO GAPROW ROWS — the battery produced no parseable invocation", file=sys.stderr)
        return 1

    print("=" * 78)
    print("INSTRUMENT AND ABORT TABLE — READ THIS BEFORE ANY NUMBER BELOW")
    print("=" * 78)
    for p in problems or ["  (none)"]:
        print("  " + p if problems else p)
    print()

    # ── THE PER-ARM TABLE.
    print("=" * 78)
    print("PER-ARM READINGS — goodput, and the CPU columns item 2's A7 needs")
    print("=" * 78)
    hdr = (f"{'cell-arm':<10} {'seed':<5} {'Mbit/s':<22} {'ms/MB':<20} "
           f"{'cores':<16} {'pred Mbit/s':<14}")
    print(hdr)
    print("-" * len(hdr))
    means = {}
    for key in sorted(rows):
        cell, arm, seed = key
        r = rows[key]
        mb = stat([x.get("mbit") for x in r])
        ms = stat([x.get("ms_per_MB") for x in r])
        co = stat([x.get("cores") for x in r])
        pr = stat([x.get("pred_mbit") for x in r])
        means[key] = mb
        f = lambda t, p=2: "-" if t[0] is None else f"{t[0]:.{p}f} (s {t[1]:.{p}f}, n={t[2]})"
        print(f"{cell + '-' + arm:<10} {seed:<5} {f(mb):<22} {f(ms):<20} "
              f"{f(co, 3):<16} {'-' if pr[0] is None else f'{pr[0]:.1f}':<14}")
    print()

    # ── READING 1: THE LEVEL. This is the drift measurement, and it is the
    #    better-powered of the two — a level needs sigma/sqrt(n), a difference
    #    needs sigma*sqrt(2/n).
    print("=" * 78)
    print("READING 1 — THE LEVEL: today's `Op` against the two published means")
    print("=" * 78)
    print("  Op is the flip-era `prior` arm, BYTE-EXACT env, on the SAME binary")
    print("  (sha256 fbd6b279...). A level shift here IS substrate drift.")
    print()
    for seed in (42, 7):
        key = ("c1", "Op", seed)
        if key not in means or means[key][0] is None:
            continue
        m, s, n = means[key]
        for label, ref, refs, refn in (
            ("flip week 2026-08-08", FLIP.get(seed, {}).get("prior", (None,))[0],
             FLIP.get(seed, {}).get("prior", (None, None))[1],
             FLIP.get(seed, {}).get("prior", (None, None, None))[2]),
            ("era week 2026-08-19", ERA.get(seed, {}).get("OLD"), None, 8),
        ):
            if ref is None:
                continue
            d = m - ref
            # Combined 2-sigma of the difference of two session means. Where
            # the reference published no dispersion, today's is used for both
            # and the substitution is PRINTED, never silent.
            s_ref = refs if refs is not None else s
            sub = "" if refs is not None else "  [ref sigma unpublished; today's substituted]"
            two = 2 * math.sqrt((s * s) / max(n, 1) + (s_ref * s_ref) / max(refn, 1))
            verdict = "RESOLVED" if abs(d) > two else "NOT RESOLVED"
            print(f"  s{seed}  Op={m:.2f} (s {s:.2f}, n={n})  vs {label} {ref:.2f}"
                  f"   d={d:+.2f} ({100*d/ref:+.1f} %)  2s_comb={two:.2f}  {verdict}{sub}")
        print()

    # ── READING 2: THE RATIO. Independent of any level shift.
    print("=" * 78)
    print("READING 2 — THE RATIO: today's `Oa/Op` against the published +12.7/+13.0 %")
    print("=" * 78)
    print("  A ratio is invariant to a pure level shift, so READING 1 and READING 2")
    print("  are INDEPENDENT and the contract's branch is their 2x2.")
    print()
    for seed in (42, 7):
        kp, ka = ("c1", "Op", seed), ("c1", "Oa", seed)
        if kp not in means or ka not in means:
            continue
        mp, sp, npn = means[kp]
        ma, sa, na = means[ka]
        if mp is None or ma is None or not mp:
            continue
        pct = 100 * (ma - mp) / mp
        # Unpaired 2-sigma_pooled of the DIFFERENCE, the era battery's own
        # convention. The PAIRED reading is the primary one and needs per-rep
        # joining, which the contract does by rep index.
        sp2 = math.sqrt((sp * sp + sa * sa) / 2)
        two = 2 * sp2 * math.sqrt(2.0 / max(min(npn, na), 1))
        ref = FLIP.get(seed, {}).get("pct")
        print(f"  s{seed}  Op={mp:.2f}  Oa={ma:.2f}  ratio={pct:+.2f} %"
              f"   2s_pooled(diff)={two:.2f} Mbit/s ({100*two/mp:.2f} %)"
              f"   published={ref:+.1f} %")
        # The design's own resolution bar, printed so an unresolved reading is
        # visibly unresolved rather than quietly averaged.
        need = a.target_pct * mp / 100.0
        print(f"        design bar: resolves {a.target_pct:.0f} % (= {need:.2f} Mbit/s) iff "
              f"sigma_pooled <= {need / (2 * math.sqrt(2.0 / max(min(npn, na), 1))):.2f}; "
              f"realized sigma_pooled = {sp2:.2f} -> "
              f"{'RESOLVING' if 2 * sp2 * math.sqrt(2.0 / max(min(npn, na), 1)) <= need else 'UNDERPOWERED'}")
        print()

    # ── READING 3: INSTRUMENT LOAD.
    print("=" * 78)
    print("READING 3 — INSTRUMENT LOAD: `Oe` (the era session's env) against `Op`")
    print("=" * 78)
    print("  RWM_ACKDIAG and RWM_WALLDIAG name gauges that DO NOT EXIST at 4171b58,")
    print("  so Oe - Op isolates the HARNESS-SIDE RWM_LATPROBE cost.")
    print()
    for seed in (42, 7):
        kp, ke = ("c1", "Op", seed), ("c1", "Oe", seed)
        if kp not in means or ke not in means:
            continue
        mp = means[kp][0]
        me = means[ke][0]
        if not mp or me is None:
            continue
        era_old = ERA.get(seed, {}).get("OLD")
        line = f"  s{seed}  Op={mp:.2f}  Oe={me:.2f}   d={me - mp:+.2f} ({100*(me-mp)/mp:+.1f} %)"
        if era_old:
            line += f"   era week's OLD (same env) = {era_old:.2f}, Oe-vs-era d={me - era_old:+.2f}"
        print(line)
    print()

    if ctld:
        print("=" * 78)
        print("THE MECHANISM GAUGE — receiver `[CTLD]` density (1.96 pre-flip)")
        print("=" * 78)
        for key in sorted(ctld, key=lambda k: (k[0], k[1], k[2] or 0)):
            m, s, n = stat(ctld[key])
            label = f"  {key[0]}-{key[1]} s{key[2]}"
            print(f"{label}: {m:.3f} (s {s:.3f}, n={n})" if m else f"{label}: -")
        print()
        print("  [CTLD] is era-invariant and RWM_DIAG-only. Op reproducing ~1.96 says")
        print("  the MECHANISM side of the comparison is intact before goodput is read.")
        print()

    if steals:
        print("=" * 78)
        print("HOST CPU STEAL, PER ARM — the drift candidate, published beside its arm")
        print("=" * 78)
        print("  % of NON-IDLE ticks taken by the hypervisor during each invocation.")
        print("  Phase 1 could only publish one figure for a whole session; a burst")
        print("  that lands on one arm and not its neighbour is a confound, so the")
        print("  counter is now read either side of EVERY invocation.")
        print()
        for key in sorted(steals, key=lambda k: (k[0], k[1], k[2])):
            m, s, n = stat(steals[key])
            mx = max(steals[key])
            print(f"  {key[0]}-{key[1]} s{key[2]}: {m:.2f} % (s {s:.2f}, n={n}) max {mx:.2f} %")
        print()

    if pings:
        print("=" * 78)
        print("THE TOPO-PING RETRY HISTOGRAM — field data, kept on EVERY invocation")
        print("=" * 78)
        print("  `1` is the healthy single draw. Anything above it is a loss draw")
        print("  that USED to be an abort and is now a recorded retry, so this table")
        print("  is the repair's own evidence rather than an argument about it.")
        print()
        allv = []
        for key in sorted(pings, key=lambda k: (k[0], k[1], k[2])):
            v = pings[key]
            allv += v
            hist = {}
            for x in v:
                hist[x] = hist.get(x, 0) + 1
            worst = max(v) if v else 0
            print(f"  {key[0]}-{key[1]} s{key[2]}: legs={len(v)} max={worst} "
                  f"histogram={dict(sorted(hist.items()))}")
        if allv:
            retried = sum(1 for x in allv if x > 1)
            print()
            print(f"  ACROSS THE BATTERY: {len(allv)} legs pinged, {retried} needed "
                  f"more than one draw ({100.0 * retried / len(allv):.2f} %), "
                  f"worst {max(allv)} of the 26 allowed.")
        print()

    if a.calib:
        print("=" * 78)
        print("CALIBRATION — n = 1. NOTHING ABOVE OR BELOW IS A RESULT.")
        print("=" * 78)
        for key in sorted(rows):
            r = rows[key][0]
            co = r.get("cores")
            head = (100 * (1 - co / a.nproc)) if isinstance(co, float) else None
            print(f"  {key[0]}-{key[1]} s{key[2]}: cores={co} of {a.nproc} -> "
                  f"CPU headroom {('%.1f %%' % head) if head is not None else '-'}"
                  f"   ms/MB={r.get('ms_per_MB')}   pred={r.get('pred_mbit')} "
                  f"meas={r.get('mbit')}")
        print()
        print("  LINK headroom comes from the -q.txt qdisc captures (discipline 16,")
        print("  TRANSFER wall denominator) and is filled by hand into the contract's")
        print("  table. AT c1 THE LINK PERMISSION HAS NEVER BEEN THE BINDING ONE:")
        print("  ~21 % utilisation of a 1 Gbit pipe. If pred ~= meas above, the cell is")
        print("  SENDER-BOUND and every goodput number in this battery is a CPU number.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
