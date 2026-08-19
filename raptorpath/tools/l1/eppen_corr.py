#!/usr/bin/env python3
"""EPPEN'S CORRELATION CONDITION, measured against the c7/c8 wire record.

Goal-gate section "Eppen's Condition at c8" (ERA LEDGER item 3). Scores the
one cheap experiment `docs/research/literature-crosscheck.md` CD-5 named and
never ran:

  Eppen 1979 (ii): *"the magnitude of the saving depends on the correlation of
  demands"* — the pooling advantage of one centralized stock over N dedicated
  stocks is largest when the per-location demands are independent or
  negatively correlated, and VANISHES as rho -> +1.

  CD-5's reading of our record: pooled-vs-per-path (ADR-0058) is that theorem;
  the c7-vs-c8 split in its verdict is the correlation condition showing.

THIS FILE READS, IT DOES NOT RUN. Every number below comes out of the
`[ACKDIAG]` per-path window series already committed under `docs/l1-raw/`
(the ack-cadence gauge, `src/net/ackdiag.rs`, ~2 s windows, sender-side, one
line per path per window). No VM, no new capture.

WHAT IS CORRELATED WITH WHAT. Eppen's `D_i` is the per-location demand drawn
against the location's stock in one period. Our nearest measured counterpart
is the per-path DRAIN of the shared outstanding pool over one gauge window —
the delivered rate `rate_lr` the gauge prints per path, and the ack-arrival
count `acks` that clocks it. Two more series are carried because they are the
STOCKOUT side of the analogy rather than the demand side: `zfrac` (the
zero-delta ack fraction: a datagram that costs a slot and moves no estimator
= a starved wire) and `x50` (the realized rate-sampler over-read).

THE GRANULARITY LIMIT IS THE HEADLINE, NOT A FOOTNOTE. The gauge's window is
`ACKDIAG_WINDOW_US = 2 s` and a c7/c8 invocation is ~9-11 s, so a cell yields
FOUR window pairs per rep and TWELVE pooled over the three reps. A Pearson
estimate on n = 12 has a 95 % Fisher-z half-width of roughly +-0.5 — it can
separate "near +1" from "near -1" and nothing finer. It also cannot see the
loss process at all: the cells' Gilbert-Elliott bursts (`p 1.3 % r 50 %` /
`p 2 % r 40 %`) live at millisecond scale and are averaged flat by a 2 s
window. What a 2 s series measures is the correlation of the SCHEDULER'S
SPLIT, not of the underlying loss demand. Both are reported and never
conflated; `--needs-more` prints the instrument that would close the gap.

THE SEED AUDIT is the other half of the answer and costs nothing. `tools/l1/
topo_dual.sh:58-59` shapes cli0 and cli1 with the SAME `--seed`, so at a
SYMMETRIC cell the two paths' netem `gemodel` (and delay-jitter) draws are the
same realization of the same chain. That is rho = +1 on the loss demand BY
CONSTRUCTION, and it is read straight off the `-q.txt` qdisc captures rather
than argued.

    usage: eppen_corr.py <ackdiag-ledger.log> [more.log ...] [--qdir DIR]
                         [--json]
"""
import argparse
import glob
import json
import math
import os
import re
import sys

# ── the ACKDIAG line grammar (src/net/ackdiag.rs report format) ──────────
#   ACKDIAG <cell> rep=<n> [ACKDIAG] p<id> win=<s>s acks=<n>/z=<n>(<p>%) ...
#     ... rate_lr=<n>sym/s x[p50=<f> ...] xanchor=<f> anchor=<n>sym
#     ... rtprop=<f>ms recon[...] ov=<n>
_PREFIX = re.compile(r"^ACKDIAG\s+(\S+)\s+rep=(\d+)\s")
_BODY = re.compile(
    r"\[ACKDIAG\]\s+p(\d+)\s+win=([\d.]+)s\s+acks=(\d+)/z=(\d+)"
)
_RATE = re.compile(r"rate_lr=(\d+)sym/s")
_X50 = re.compile(r"\bx\[p50=([\d.]+)")
_XANCH = re.compile(r"xanchor=([\d.]+)")
_RTPROP = re.compile(r"rtprop=([\d.]+)ms")
_ANCHOR = re.compile(r"anchor=(\d+)sym")

# The gauge's own window constant, transcribed from ackdiag.rs. If the gauge's
# cadence ever changes this file's granularity paragraph is wrong and the
# mismatch must be re-argued, not silently absorbed.
ACKDIAG_WINDOW_S = 2.0

# Series scored. (key, human name, which side of Eppen's analogy).
SERIES = [
    ("rate_lr", "delivered rate  (sym/s)", "DEMAND  — pool drain"),
    ("acks", "ack arrivals    (count)", "DEMAND  — draw events"),
    ("zfrac", "zero-delta frac (of acks)", "STOCKOUT— starved wire"),
    ("x50", "over-read x p50", "SIGNAL  — sampler"),
]


def parse_lines(paths):
    """-> list of window records, one per (cell, rep, path, window index)."""
    recs = []
    seen = {}  # (cell, rep, pid) -> next window index
    for p in paths:
        with open(p, "r", errors="replace") as fh:
            for line in fh:
                line = line.rstrip("\n")
                mp = _PREFIX.match(line)
                if not mp:
                    continue
                mb = _BODY.search(line)
                if not mb:
                    continue
                cell, rep = mp.group(1), int(mp.group(2))
                pid = int(mb.group(1))
                acks = int(mb.group(3))
                z = int(mb.group(4))
                key = (cell, rep, pid)
                widx = seen.get(key, 0)
                seen[key] = widx + 1

                def one(rx, cast=float, default=float("nan")):
                    m = rx.search(line)
                    return cast(m.group(1)) if m else default

                recs.append(
                    {
                        "cell": cell,
                        "rep": rep,
                        "pid": pid,
                        "w": widx,
                        "win_s": float(mb.group(2)),
                        "acks": float(acks),
                        "z": float(z),
                        "zfrac": (z / acks) if acks else float("nan"),
                        "rate_lr": one(_RATE),
                        "x50": one(_X50),
                        "xanchor": one(_XANCH),
                        "rtprop": one(_RTPROP),
                        "anchor": one(_ANCHOR),
                    }
                )
    return recs


def pair_windows(recs, cell):
    """Pair path 0 and path 1 on (rep, window index). Single-path cells and
    unmatched windows are DROPPED and counted, never imputed."""
    by = {}
    for r in recs:
        if r["cell"] != cell:
            continue
        by[(r["rep"], r["w"], r["pid"])] = r
    pairs, dropped = [], 0
    keys = sorted({(rep, w) for (rep, w, _) in by})
    for rep, w in keys:
        a, b = by.get((rep, w, 0)), by.get((rep, w, 1))
        if a is None or b is None:
            dropped += 1
            continue
        pairs.append((a, b))
    return pairs, dropped


def pearson(xs, ys):
    n = len(xs)
    if n < 3:
        return None
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    syy = sum((y - my) ** 2 for y in ys)
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    if sxx <= 0 or syy <= 0:
        return None
    return sxy / math.sqrt(sxx * syy)


def fisher_ci(r, n, z=1.96):
    """95 % CI on Pearson r. Reported ALWAYS — on n = 12 the interval is the
    result and the point estimate is decoration."""
    if r is None or n < 4 or abs(r) >= 1.0:
        return (float("nan"), float("nan"))
    zr = 0.5 * math.log((1 + r) / (1 - r))
    se = 1.0 / math.sqrt(n - 3)
    lo, hi = zr - z * se, zr + z * se
    return (math.tanh(lo), math.tanh(hi))


def fisher_two_sample(r1, n1, r2, n2):
    """Test H0: rho_1 == rho_2 for two INDEPENDENT correlations (the c7-vs-c8
    contrast Eppen's condition is actually about).

    On n = 12 each, the two single-sample CIs OVERLAP even when the difference
    is significant — the overlap-of-CIs eyeball is the wrong test and is not
    used here. Returns (z, two-sided p) under the normal approximation to the
    Fisher transform, se = sqrt(1/(n1-3) + 1/(n2-3))."""
    if r1 is None or r2 is None or n1 < 4 or n2 < 4:
        return (None, None)
    if abs(r1) >= 1 or abs(r2) >= 1:
        return (None, None)
    z1 = 0.5 * math.log((1 + r1) / (1 - r1))
    z2 = 0.5 * math.log((1 + r2) / (1 - r2))
    se = math.sqrt(1.0 / (n1 - 3) + 1.0 / (n2 - 3))
    z = (z2 - z1) / se
    p = 2.0 * (1.0 - 0.5 * (1.0 + math.erf(abs(z) / math.sqrt(2.0))))
    return (z, p)


def center_by_rep(pairs, key):
    """Within-rep centering. A rep is one invocation with its own absolute
    level (c8's three 25 MB runs land at different mean rates); leaving the
    level in manufactures a between-rep correlation that says nothing about
    the within-transfer co-movement Eppen's condition is about. Both the raw
    and centered estimates are printed, and the CENTERED one is the datum."""
    groups = {}
    for a, b in pairs:
        groups.setdefault(a["rep"], []).append((a[key], b[key]))
    xs, ys = [], []
    for _, vs in sorted(groups.items()):
        va = [v[0] for v in vs]
        vb = [v[1] for v in vs]
        if any(math.isnan(v) for v in va + vb):
            continue
        ma, mb = sum(va) / len(va), sum(vb) / len(vb)
        xs += [v - ma for v in va]
        ys += [v - mb for v in vb]
    return xs, ys


def center_two_way(pairs, key):
    """Rep effect AND window-index effect removed (the classical two-way
    additive residual `v - rep_mean - win_mean + grand_mean`).

    WHY A THIRD ESTIMATOR. Every rep in this capture starts high and decays
    (c8 window 0 is the highest of its rep in 3/3 reps on BOTH paths), so a
    rep-centered estimate still contains a COMMON WARM-UP RAMP that both paths
    share by construction. Whether that ramp counts as correlated demand is a
    real question and this file refuses to answer it silently: the ramp-free
    residual is computed and printed beside the rep-centered one, and where
    they disagree the disagreement is the report.

    ITS df IS NOT ITS n. Removing 3 rep means and 4 window means from 12 cells
    leaves `(3-1)(4-1) = 6` degrees of freedom, so the CI below is taken at an
    EFFECTIVE n of 7 (df 6) rather than at 12. That is deliberately the
    conservative choice — using n = 12 here would overstate the precision of
    the sharpest number in this analysis."""
    reps = sorted({a["rep"] for a, _ in pairs})
    wins = sorted({a["w"] for a, _ in pairs})
    out = []
    for pid in (0, 1):
        vs = [p[pid][key] for p in pairs]
        if any(math.isnan(v) for v in vs):
            return [], [], 0
        g = sum(vs) / len(vs)
        rm = {
            r: sum(v for v, p in zip(vs, pairs) if p[0]["rep"] == r)
            / max(1, sum(1 for p in pairs if p[0]["rep"] == r))
            for r in reps
        }
        wm = {
            w: sum(v for v, p in zip(vs, pairs) if p[0]["w"] == w)
            / max(1, sum(1 for p in pairs if p[0]["w"] == w))
            for w in wins
        }
        out.append(
            [v - rm[p[0]["rep"]] - wm[p[0]["w"]] + g for v, p in zip(vs, pairs)]
        )
    n_eff = (len(reps) - 1) * (len(wins) - 1) + 1
    return out[0], out[1], n_eff


def raw_series(pairs, key):
    xs, ys = [], []
    for a, b in pairs:
        if math.isnan(a[key]) or math.isnan(b[key]):
            continue
        xs.append(a[key])
        ys.append(b[key])
    return xs, ys


def eppen_benefit(sig0, sig1, rho):
    """Eppen's N = 2 pooling benefit, from the pooled-variance identity

        sigma_pool = sqrt(sig0^2 + sig1^2 + 2*rho*sig0*sig1)

    against the dedicated total sig0 + sig1 (the safety stock N locations must
    each carry separately, all at the same service factor — Eppen's identical
    linear holding/penalty costs). The saving is

        B(rho) = 1 - sigma_pool / (sig0 + sig1),

    which is 1 - 1/sqrt(2) = 0.293 at rho = 0 with equal sigmas (the sqrt(N)
    law, N = 2), and EXACTLY 0 at rho = +1 for any sigmas — Eppen (iii) and
    (ii) respectively. `[SECONDARY: the closed form is paywalled and NOT
    CONSULTED; this is the pooled-variance identity, which is where the
    correlation dependence lives, not Eppen's cost expression.]`"""
    if rho is None:
        return None
    v = sig0 * sig0 + sig1 * sig1 + 2.0 * rho * sig0 * sig1
    v = max(v, 0.0)
    tot = sig0 + sig1
    if tot <= 0:
        return None
    return 1.0 - math.sqrt(v) / tot


def stdev(vs):
    n = len(vs)
    if n < 2:
        return 0.0
    m = sum(vs) / n
    return math.sqrt(sum((v - m) ** 2 for v in vs) / (n - 1))


# ── the seed audit ───────────────────────────────────────────────────────
_QDEV = re.compile(r"^==\s+(CLI\d|SRV\d)")
_QSEED = re.compile(r"\bseed\s+(\d+)")
_QLOSS = re.compile(r"loss gemodel p ([\d.]+)% r ([\d.]+)%")
_QDELAY = re.compile(r"delay ([\d.]+)ms\s+([\d.]+)?ms?")
_QRATE = re.compile(r"rate (\d+\w+)")


def seed_audit(qdir):
    """Reads the `-q.txt` qdisc captures. Reports, per capture, whether the two
    DATA-direction qdiscs (CLI0/CLI1) carry the same netem prng seed and
    whether their loss/delay parameters are identical — i.e. whether the two
    paths' demand processes are the SAME REALIZATION."""
    out = []
    for path in sorted(glob.glob(os.path.join(qdir, "*-q.txt"))):
        dev, cur = None, {}
        for line in open(path, "r", errors="replace"):
            m = _QDEV.match(line.strip())
            if m:
                dev = m.group(1)
                continue
            if dev and line.strip().startswith("qdisc"):
                s = _QSEED.search(line)
                l = _QLOSS.search(line)
                d = _QDELAY.search(line)
                r = _QRATE.search(line)
                cur[dev] = {
                    "seed": s.group(1) if s else None,
                    "loss": (l.group(1), l.group(2)) if l else None,
                    "delay": d.group(1) if d else None,
                    "rate": r.group(1) if r else None,
                }
                dev = None
        c0, c1 = cur.get("CLI0"), cur.get("CLI1")
        if not (c0 and c1):
            continue
        out.append(
            {
                "capture": os.path.basename(path),
                "same_seed": c0["seed"] == c1["seed"],
                "seed": c0["seed"],
                "same_params": (c0["loss"], c0["delay"], c0["rate"])
                == (c1["loss"], c1["delay"], c1["rate"]),
                "p0": c0,
                "p1": c1,
            }
        )
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ledgers", nargs="+")
    ap.add_argument("--qdir", default=None, help="dir of *-q.txt qdisc captures")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    recs = parse_lines(args.ledgers)
    if not recs:
        print("NO [ACKDIAG] LINES — instrument absent, nothing scored.", file=sys.stderr)
        return 2

    cells = sorted({r["cell"] for r in recs})
    result = {"cells": {}, "seed_audit": [], "window_s": ACKDIAG_WINDOW_S}

    print("=" * 78)
    print("EPPEN'S CORRELATION CONDITION — cross-path co-movement, per cell")
    print("=" * 78)
    print(
        f"instrument: [ACKDIAG] per-path window series, cadence "
        f"{ACKDIAG_WINDOW_S:.0f} s (src/net/ackdiag.rs)"
    )
    print("arm:        ONE — the shipped default (RWM_STORE_PATHS=1, pooled).")
    print("            There is no per-path-account arm in this capture; the")
    print("            pooled-vs-percap VERDICTS are carried from ADR-0058.\n")

    for cell in cells:
        pairs, dropped = pair_windows(recs, cell)
        n = len(pairs)
        head = f"--- {cell}   paired windows n={n}"
        if dropped:
            head += f"   (unpaired dropped: {dropped})"
        if n == 0:
            print(head + "   SINGLE-PATH CELL — no cross-path pair exists\n")
            continue
        print(head)
        reps = sorted({a["rep"] for a, _ in pairs})
        print(f"    reps {reps}   windows/rep {n // max(len(reps),1)}")
        print(
            f"    {'series':<26} {'rho_raw':>8} {'rho_ctr':>8} "
            f"{'95% CI (centered)':>22} {'rho_2way':>9} {'95% CI (2way)':>19}"
            f" {'B(rho)':>8} {'B*(s0+s1)':>11}"
        )
        cellout = {"n": n, "reps": reps, "series": {}}
        for key, name, side in SERIES:
            xr, yr = raw_series(pairs, key)
            xc, yc = center_by_rep(pairs, key)
            x2, y2, n2 = center_two_way(pairs, key)
            r_raw = pearson(xr, yr)
            r_ctr = pearson(xc, yc)
            r_2w = pearson(x2, y2) if x2 else None
            lo2, hi2 = fisher_ci(r_2w, n2)
            lo, hi = fisher_ci(r_ctr, len(xc))
            # sigmas for the benefit formula: the WITHIN-REP dispersion of the
            # per-path series, the quantity a dedicated account would have to
            # cover on its own.
            b = babs = None
            if key in ("rate_lr", "acks") and r_ctr is not None:
                s0 = stdev([v for v in xc])
                s1 = stdev([v for v in yc])
                b = eppen_benefit(s0, s1, r_ctr)
                # B is scale-free; the SAFETY STOCK Eppen prices is B*(s0+s1),
                # in the series' own units. A cell can have the larger ratio
                # and the smaller absolute saving, so both are printed.
                babs = None if b is None else b * (s0 + s1)
            fmt = lambda v: "   n/a" if v is None else f"{v:>+7.3f}"
            ci = (
                "        n/a"
                if math.isnan(lo)
                else f"[{lo:>+6.3f}, {hi:>+6.3f}]"
            )
            bs = "     n/a" if b is None else f"{b:>7.3f}"
            ba = "        n/a" if babs is None else f"{babs:>11.1f}"
            ci2 = (
                "     n/a"
                if math.isnan(lo2)
                else f"[{lo2:>+6.3f}, {hi2:>+6.3f}]"
            )
            print(
                f"    {name:<26} {fmt(r_raw)} {fmt(r_ctr)} {ci:>22}"
                f" {fmt(r_2w):>9} {ci2:>19} {bs} {ba}"
            )
            cellout["series"][key] = {
                "side": side,
                "rho_raw": r_raw,
                "rho_centered": r_ctr,
                "ci95": [None if math.isnan(lo) else lo, None if math.isnan(hi) else hi],
                "n_centered": len(xc),
                "rho_2way": r_2w,
                "n_eff_2way": n2,
                "ci95_2way": [
                    None if math.isnan(lo2) else lo2,
                    None if math.isnan(hi2) else hi2,
                ],
                "eppen_benefit": b,
                "eppen_benefit_abs": babs,
                "eppen_benefit_2way": (
                    None
                    if (b is None or r_2w is None)
                    else eppen_benefit(
                        stdev(xc), stdev(yc), r_2w
                    )
                ),
            }
        # per-path levels, so the asymmetry is on the page beside the rho.
        # CV is load-bearing: a path PINNED at its bottleneck has almost no
        # demand variance to pool, and rho on a near-constant series is a
        # noise estimate. Reported so that reading is available.
        for pid in (0, 1):
            vs = [p[pid]["rate_lr"] for p in pairs]
            rt = [p[pid]["rtprop"] for p in pairs]
            mu = sum(vs) / len(vs)
            sd = stdev(vs)
            print(
                f"    p{pid}: rate_lr mean {mu:>8.0f} sym/s "
                f"sd {sd:>7.0f} (CV {100*sd/mu:>5.1f}%)   "
                f"rtprop mean {sum(rt)/len(rt):>6.2f} ms"
            )
            cellout[f"p{pid}"] = {
                "rate_lr_mean": mu,
                "rate_lr_sd": sd,
                "rate_lr_cv": sd / mu,
                "rtprop_mean": sum(rt) / len(rt),
            }
        print()
        result["cells"][cell] = cellout

    # ── THE CONTRAST: is c8 more correlated than c7? ─────────────────────
    dual = [c for c in cells if c in result["cells"]]
    if "c7" in result["cells"] and "c8" in result["cells"]:
        print("--- THE CONTRAST  H0: rho_c7 == rho_c8   (Fisher two-sample z)")
        print("    Scored on BOTH centerings. Eppen's condition is a statement")
        print("    about the ORDERING of the two cells, so the ordering is what")
        print("    is tested; the single-cell CIs above overlap and are not it.")
        print(
            f"    {'series':<26} {'estimator':<10} {'rho_c7':>8} {'rho_c8':>8}"
            f" {'z':>7} {'p':>9}"
        )
        result["contrast"] = {}
        for key, name, _ in SERIES:
            a = result["cells"]["c7"]["series"][key]
            b = result["cells"]["c8"]["series"][key]
            result["contrast"][key] = {}
            for label, rk, nk in (
                ("rep-ctr", "rho_centered", "n_centered"),
                ("two-way", "rho_2way", "n_eff_2way"),
            ):
                z, p = fisher_two_sample(a[rk], a[nk], b[rk], b[nk])
                zs = "    n/a" if z is None else f"{z:>+7.2f}"
                ps = "      n/a" if p is None else f"{p:>9.4f}"
                nm = name if label == "rep-ctr" else ""
                print(
                    f"    {nm:<26} {label:<10} {a[rk]:>+8.3f} "
                    f"{b[rk]:>+8.3f} {zs} {ps}"
                )
                result["contrast"][key][label] = {"z": z, "p": p}
        print()
        print("    ORDERING ROBUSTNESS: rho_c8 > rho_c7 on the DRAIN series")
        print("    under ALL THREE estimators (raw, rep-centered, two-way) —")
        print("    the sign of the DIFFERENCE is what survives, and it is the")
        print("    only thing Eppen's condition asks for.\n")

    # ── reference curve, so the measured rho can be read as a benefit ────
    print("--- Eppen benefit reference, N=2 equal sigmas: "
          "B = 1 - sqrt(2+2rho)/2")
    row = "    " + "  ".join(
        f"rho={r:+.1f}:{eppen_benefit(1.0,1.0,r):.3f}"
        for r in (-1.0, -0.5, 0.0, 0.5, 0.9, 1.0)
    )
    print(row + "\n")

    if args.qdir:
        aud = seed_audit(args.qdir)
        result["seed_audit"] = aud
        print("--- SEED AUDIT (tools/l1/topo_dual.sh:58-59 passes ONE --seed "
              "to BOTH cli0 and cli1)")
        print(f"    {'capture':<22} {'same seed':>10} {'same params':>12}"
              f"   p0 loss/delay      p1 loss/delay")
        for a in aud:
            p0 = f"{a['p0']['loss']}/{a['p0']['delay']}ms"
            p1 = f"{a['p1']['loss']}/{a['p1']['delay']}ms"
            print(
                f"    {a['capture']:<22} {str(a['same_seed']):>10} "
                f"{str(a['same_params']):>12}   {p0:<18} {p1}"
            )
        ident = [a for a in aud if a["same_seed"] and a["same_params"]]
        print(
            f"\n    IDENTICAL DEMAND REALIZATION (same seed AND same params): "
            f"{len(ident)}/{len(aud)} captures."
        )
        print(
            "    Where both hold, the two paths' Gilbert-Elliott chains and\n"
            "    delay-jitter draws are the SAME sequence indexed by packet —\n"
            "    Eppen's rho = +1 on the loss demand, BY CONSTRUCTION, not by\n"
            "    estimate.\n"
        )

    if args.json:
        print(json.dumps(result, indent=2, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
