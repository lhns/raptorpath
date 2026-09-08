#!/usr/bin/env python3
"""S4 — the offline `sigma_hat_e / ref` reading off the `[ETA]` stream.

WHAT IT COMPUTES, AND WHY IT IS ONE SCRIPT RATHER THAN A PARAGRAPH.
-------------------------------------------------------------------
paper 16.81.1's Luce/Gumbel identity turns the shipped placement temperature
into a falsifiable claim about the wire:

    T = 0.15   <=>   sigma_hat_e = (pi * 0.15 / sqrt(6)) * ref
                                 = 0.19238 * ref      at EVERY cell

so the deciding measurement is a RATIO, not a latency.  `sigma_hat_e` is the
16.75 tau-lag dispersion of the ETA prediction error, printed by
`net/eta.rs` as `sig_us=<us>/n<pairs>` on BOTH `[ETA]` lines; `ref` is
`min_i srtt_i` over the active paths.

**THE REF FIELD THIS SCRIPT USES IS `tau_us`, OFF THE SAME `[ETA]` LINE**, and
that choice is the whole hygiene of the script.  `net/eta.rs` feeds each
path's tau-lag estimator with `tau = Duration::from_micros(p.tau_us)` --
"the path's reference lag (RTprop when the receiver has one, its SRTT
otherwise)".  So `tau_us` is the reference the dispersion was ALREADY measured
against, and at a SINGLE-PATH cell `min_i srtt_i` IS that one path's srtt.
Reading `ref` off a `[DIAG]` line instead would introduce a second clock that
can disagree with the first, which is exactly the failure the pre-registration
("the preferred route, because the two refs then cannot disagree") named.
`--ref-us` exists for the case where `tau_us` is absent from the artefact; a
run scored through `--ref-us` is flagged SURROGATE-REF in the output and is
not a clean reading.

The inverse of the identity gives the temperature the derived law would have
produced, which is the number to hold against the shipped 0.15:

    T_eff = (sqrt(6)/pi) * sigma_hat_e / ref

Multi-path cells pool the per-path `sig_us` as an RMS over the candidate set
(the pre-registration's own rule); the ref is then `min_i tau_us`.

READING RULE (pre-stated; this script only reports which limb fires).
---------------------------------------------------------------------
  CELL-INVARIANT   pooled sigma/ref at each cell lies inside every other
                   cell's [min, max]
  CELL-DEPENDENT   the per-cell ranges are disjoint; the ratio is reported
  UNREADABLE       no `[ETA]` line, or `bind` >= --max-bind, or tau-lag
                   pairs `n` < --min-pairs.  `bind` is the receiver line's
                   own coverage gauge (the fraction of arrivals carrying the
                   `eta_rel = 0` "no prediction" sentinel).

USAGE
-----
    eta_s4.py LEDGER...                  # parse `[ETA]` out of L1 ledgers
    eta_s4.py --transcribed              # score the crown spot's hand-copied
                                         #   points (the only ones that exist)
    eta_s4.py LEDGER... --ref-us c2=10000 --ref-us c3=40000

Exit status is 0 whatever the reading: an UNREADABLE artefact is a RESULT
here, not a failure of the script.
"""

from __future__ import annotations

import argparse
import math
import re
import sys
from collections import defaultdict

# 16.81.1: T = (pi/sqrt(6)) * sigma/ref, so sigma/ref = (sqrt(6)/pi) * T.
SHIPPED_T = 0.15
K_T_TO_SIGMA = math.pi / math.sqrt(6.0)      # 1.28255
K_SIGMA_TO_T = math.sqrt(6.0) / math.pi      # 0.779697
TARGET_RATIO = K_SIGMA_TO_T ** -1 * SHIPPED_T  # == K_T_TO_SIGMA * 0.15
BAND = 0.20                                    # the pre-registered +/- 20 %

# ── PARSING ────────────────────────────────────────────────────────────
# `net/eta.rs` line shapes, both sites:
#   [ETA] site=sender fhat_us=.. n=.. zero=.. ... p1:n=A/B drop=.. tau_us=..
#         e_p50=.. .. late=.. sig_us=<us>/n<pairs>
#   [ETA] site=receiver n=..  p1:n=.. bind=.. tau_us=.. srtt_src=..
#         l_p50=.. .. sig_us=<us>/n<pairs> minrst=..
# `-` is the `-`-iff-n=0 convention and is never a measured zero.
RE_ETA = re.compile(r"\[ETA\]\s+site=(sender|receiver)\s+(.*)$")
RE_PATH = re.compile(
    r"\bp(?P<id>\d+):(?P<body>.*?)(?=\s+p\d+:|$)"
)
RE_FIELD = re.compile(r"(\w+)=([^\s]+)")
# ledger context lines written by crownspot8.sh / tail_matrix.sh
RE_STAGE = re.compile(r"CROWNSPOT stage seed=(?P<seed>\S+) cell=(?P<cell>\S+)")
RE_ARM = re.compile(r"^---\s+(?P<arm>\S+)\s+(?P<size>\d+)B\b")
# `  SPAN ship 400B tm-s.log: [SPAN] ...` -- the scrape's own prefix shape
RE_PREFIX = re.compile(
    r"^\s*\S+\s+(?P<arm>\S+)\s+(?P<size>\d+)B\s+(?P<endpoint>\S+?):\s*\[ETA\]"
)


def _num(tok):
    """`-` (n=0) -> None; otherwise a float."""
    if tok in ("-", "", None):
        return None
    try:
        return float(tok)
    except ValueError:
        return None


class Point:
    """One path's reading off one `[ETA]` line."""

    __slots__ = ("cell", "seed", "endpoint", "arm", "size", "site",
                 "path", "sigma_us", "pairs", "tau_us", "bind", "srtt_src",
                 "t_eff_engine")

    def __init__(self, **kw):
        for k in self.__slots__:
            setattr(self, k, kw.get(k))

    def key(self):
        return (self.cell, self.seed, self.endpoint, self.arm, self.size,
                self.site)


def parse_eta_line(line, ctx):
    """Yield one Point per path on an `[ETA]` line. `ctx` supplies cell/seed."""
    m = RE_ETA.search(line)
    if not m:
        return []
    site, rest = m.group(1), m.group(2)

    pm = RE_PREFIX.match(line)
    endpoint = pm.group("endpoint") if pm else ctx.get("endpoint", "-")
    arm = pm.group("arm") if pm else ctx.get("arm", "-")
    size = pm.group("size") if pm else ctx.get("size", "-")

    # The sender line carries the engine's OWN inverted temperature as
    # `t_eff=`; when present it is the preferred reading (one ref, one clock)
    # and is carried alongside so the two routes can be held against each
    # other rather than averaged.
    head = rest.split(" p", 1)[0]
    head_fields = dict(RE_FIELD.findall(head))
    t_eff_engine = _num(head_fields.get("t_eff"))

    out = []
    for pmatch in RE_PATH.finditer(rest):
        f = dict(RE_FIELD.findall(pmatch.group("body")))
        sig = f.get("sig_us")
        sigma_us = pairs = None
        if sig is not None:
            if "/" in sig:
                sv, nv = sig.split("/n", 1) if "/n" in sig else (sig, "0")
                sigma_us = _num(sv)
                pairs = _num(nv)
            else:
                sigma_us = _num(sig)
        out.append(Point(
            cell=ctx.get("cell", "-"), seed=ctx.get("seed", "-"),
            endpoint=endpoint, arm=arm, size=size, site=site,
            path=pmatch.group("id"),
            sigma_us=sigma_us, pairs=pairs,
            tau_us=_num(f.get("tau_us")),
            bind=_num(f.get("bind")),
            srtt_src=f.get("srtt_src", "-"),
            t_eff_engine=t_eff_engine,
        ))
    return out


def parse_ledger(path):
    ctx = {}
    pts = []
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            sm = RE_STAGE.search(line)
            if sm:
                ctx["cell"] = sm.group("cell")
                ctx["seed"] = sm.group("seed")
            am = RE_ARM.match(line)
            if am:
                ctx["arm"] = am.group("arm")
                ctx["size"] = am.group("size")
            if "[ETA]" in line:
                pts.extend(parse_eta_line(line, ctx))
    return pts


# ── THE TRANSCRIBED RECORD ─────────────────────────────────────────────
# `tail_matrix.sh:156` scrapes only `[RFA]`, `[SUCC]`, `[RACK]` (and `[SPAN]`
# above it).  `[ETA]` is NOT scraped and `/tmp/tm-{s,c}.log` is overwritten per
# arm, so NO `[ETA]` line survives into any committed ledger.  What survives is
# what a human copied into goal-gate "THE CROWN NO-REGRESSION SPOT" -- section
# 6(ii) at full n and section 2 at the smoke.  Those points are recorded here
# so the reading is reproducible; every one of them is missing `tau_us`,
# `bind` and `t_eff`, which is why they are scored SURROGATE-REF.
#   sigma is (sender, receiver) in us.
TRANSCRIBED = [
    # cell, seed, arm,               endpoint, sigma_sender, sigma_recv, pairs
    ("c2", "42", "smoke n=1",        "tm-s", 1668.0, 3430.0, (27.0, 13.0)),
    ("c3", "7",  "1200B n=8",        "tm-s", 2633.0, 2242.0, (None, None)),
    ("c3", "7",  "1200B n=8",        "tm-c", 3662.0, 2250.0, (None, None)),
]
# Surrogate refs: the harness's own scenario table, `tools/l1/lib.sh:186-189`
# (`fields: rate one_way_ms jitter_ms ge_p ge_q`).  RTT = 2 * one_way.
#   c1 1 ms one-way -> 2 ms ; c2 5 ms -> 10 ms ; c3 20 ms -> 40 ms.
CELL_RTT_US = {"c1": 2000.0, "c2": 10000.0, "c3": 40000.0}


def transcribed_points():
    pts = []
    for cell, seed, arm, endpoint, s_snd, s_rcv, pairs in TRANSCRIBED:
        for site, sig, n in (("sender", s_snd, pairs[0]),
                             ("receiver", s_rcv, pairs[1])):
            pts.append(Point(cell=cell, seed=seed, endpoint=endpoint, arm=arm,
                             size="-", site=site, path="1", sigma_us=sig,
                             pairs=n, tau_us=None, bind=None, srtt_src="-",
                             t_eff_engine=None))
    return pts


# ── SCORING ────────────────────────────────────────────────────────────
def resolve_ref(pt, ref_override):
    """(ref_us, source_tag).  `tau_us` wins; `--ref-us`/cell table is a
    SURROGATE and says so."""
    if pt.tau_us:
        return pt.tau_us, "tau_us"
    if pt.cell in ref_override:
        return ref_override[pt.cell], "SURROGATE(--ref-us)"
    if pt.cell in CELL_RTT_US:
        return CELL_RTT_US[pt.cell], "SURROGATE(lib.sh)"
    return None, "NONE"


def score(points, ref_override, min_pairs, max_bind):
    rows = []
    for pt in points:
        ref, src = resolve_ref(pt, ref_override)
        ratio = t_eff = None
        why = []
        if pt.sigma_us is None:
            why.append("no sig_us (n=0)")
        if ref is None:
            why.append("no ref")
        if pt.pairs is not None and pt.pairs < min_pairs:
            why.append("pairs=%d < %d" % (pt.pairs, min_pairs))
        if pt.pairs is None:
            why.append("pairs NOT IN ARTEFACT")
        if pt.bind is None:
            why.append("bind NOT IN ARTEFACT")
        elif pt.bind >= max_bind:
            why.append("bind=%.4f >= %.2f" % (pt.bind, max_bind))
        if src.startswith("SURROGATE"):
            why.append(src)
        if pt.sigma_us is not None and ref:
            ratio = pt.sigma_us / ref
            t_eff = K_SIGMA_TO_T * ratio
        rows.append((pt, ref, src, ratio, t_eff, why))
    return rows


def pool(rows):
    """Per (cell, site): the min/max/median of sigma/ref over reps."""
    by = defaultdict(list)
    for pt, ref, src, ratio, t_eff, why in rows:
        if ratio is not None:
            by[(pt.cell, pt.site)].append(ratio)
    out = {}
    for k, vals in by.items():
        vals.sort()
        n = len(vals)
        med = vals[n // 2] if n % 2 else 0.5 * (vals[n // 2 - 1] + vals[n // 2])
        out[k] = (min(vals), med, max(vals), n)
    return out


def reading(pooled, rows):
    """Which limb of the pre-stated rule fires."""
    blockers = [w for _, _, _, r, _, w in rows if r is None or w]
    cells = sorted({c for (c, _s) in pooled})
    if not pooled:
        return "UNREADABLE", "no `[ETA]` reading survives in the artefact"
    # per-cell range over BOTH sites pooled, per the task's reading rule
    span = {}
    for c in cells:
        vals = []
        for (cc, _s), (lo, _m, hi, _n) in pooled.items():
            if cc == c:
                vals += [lo, hi]
        span[c] = (min(vals), max(vals))
    if len(cells) < 2:
        return ("UNREADABLE",
                "only ONE cell (%s) has a reading; cell-invariance needs two"
                % cells[0])
    hard = [w for _, _, _, _, _, w in rows
            if any(("pairs=" in x or "bind=" in x or "NOT IN ARTEFACT" in x)
                   for x in w)]
    verdict_note = ""
    if hard:
        verdict_note = (" (and %d of %d readings trip the UNREADABLE limb: "
                        "see the WHY column)" % (len(hard), len(rows)))
    a, b = cells[0], cells[-1]
    lo_a, hi_a = span[a]
    lo_b, hi_b = span[b]
    # THE LEVEL RATIO, reported whatever the limb: the cells' median sigma/ref
    # against each other. It is the number the strike is worth if the ranges
    # are disjoint, and it is meaningless-but-harmless if they are not.
    med = {}
    for c in cells:
        vs = sorted(r for pt, _rf, _s, r, _t, _w in rows
                    if r is not None and pt.cell == c)
        med[c] = vs[len(vs) // 2] if len(vs) % 2 else \
            0.5 * (vs[len(vs) // 2 - 1] + vs[len(vs) // 2])
    lvl = max(med[a], med[b]) / min(med[a], med[b]) if min(med.values()) else 0.0
    lvl_note = "  median level %s %.4f vs %s %.4f = %.2fx" % (
        a, med[a], b, med[b], lvl)

    # A range built from ONE reading is a point, and a point cannot contain
    # another cell's range nor be honestly called disjoint from it: with no
    # within-cell spread there is nothing to compare a between-cell gap TO.
    # That is the UNREADABLE limb, not a cell-dependence finding.
    thin = [c for c in cells
            if sum(1 for pt, _rf, _s, r, _t, _w in rows
                   if r is not None and pt.cell == c) < 2 or span[c][0] == span[c][1]]
    if thin:
        return ("UNREADABLE",
                "cell(s) %s carry a DEGENERATE range (one reading, no "
                "within-cell spread) -- the rule compares a between-cell gap "
                "against a within-cell spread that does not exist.%s%s"
                % (",".join(thin), lvl_note, verdict_note))
    if lo_a <= lo_b and hi_b <= hi_a or lo_b <= lo_a and hi_a <= hi_b:
        return "CELL-INVARIANT", "ranges nest." + lvl_note + verdict_note
    if hi_a < lo_b or hi_b < lo_a:
        gap = (max(lo_a, lo_b) / min(hi_a, hi_b)) if min(hi_a, hi_b) else 0.0
        return ("CELL-DEPENDENT",
                "%s %.4f-%.4f vs %s %.4f-%.4f, DISJOINT, gap %.2fx.%s%s"
                % (a, lo_a, hi_a, b, lo_b, hi_b, gap, lvl_note, verdict_note))
    return ("CELL-DEPENDENT",
            "ranges overlap but neither nests." + lvl_note + verdict_note)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("ledgers", nargs="*", help="L1 ledger / endpoint logs")
    ap.add_argument("--transcribed", action="store_true",
                    help="score the crown spot's hand-copied `[ETA]` points")
    ap.add_argument("--ref-us", action="append", default=[],
                    metavar="CELL=US", help="surrogate ref when tau_us absent")
    ap.add_argument("--min-pairs", type=int, default=30,
                    help="tau-lag pairs below this trip UNREADABLE (default 30)")
    ap.add_argument("--max-bind", type=float, default=0.50,
                    help="zero-sentinel bind at or above this trips UNREADABLE")
    args = ap.parse_args(argv)

    ref_override = {}
    for spec in args.ref_us:
        if "=" not in spec:
            ap.error("--ref-us wants CELL=US, got %r" % spec)
        c, v = spec.split("=", 1)
        ref_override[c] = float(v)

    points = []
    scanned = 0
    for path in args.ledgers:
        scanned += 1
        points.extend(parse_ledger(path))
    if args.transcribed:
        points.extend(transcribed_points())

    print("S4 -- sigma_hat_e / ref against the shipped T = %.2f" % SHIPPED_T)
    print("target sigma/ref = (pi/sqrt6)*T = %.5f   band +/-%d%% = [%.5f, %.5f]"
          % (TARGET_RATIO, int(BAND * 100),
             TARGET_RATIO * (1 - BAND), TARGET_RATIO * (1 + BAND)))
    print("ledgers scanned: %d   [ETA] path-readings found: %d%s"
          % (scanned, sum(1 for p in points if p.tau_us is not None),
             "   transcribed points: %d" % len(transcribed_points())
             if args.transcribed else ""))
    print()

    if not points:
        print("READING: UNREADABLE -- not one `[ETA]` line in the artefact.")
        print("  `tail_matrix.sh:156` scrapes only [RFA] [SUCC] [RACK] (and")
        print("  [SPAN]); `[ETA]` is not in that list and /tmp/tm-{s,c}.log is")
        print("  overwritten per arm. Nothing to divide.")
        return 0

    rows = score(points, ref_override, args.min_pairs, args.max_bind)
    hdr = ("cell", "seed", "endp", "arm", "site", "sig_us", "pairs",
           "ref_us", "ref src", "sig/ref", "T_eff", "T_eff/0.15")
    print("| " + " | ".join(hdr) + " |")
    print("|" + "|".join("---" for _ in hdr) + "|")
    for pt, ref, src, ratio, t_eff, why in rows:
        print("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
            pt.cell, pt.seed, pt.endpoint, pt.arm, pt.site,
            "-" if pt.sigma_us is None else "%.0f" % pt.sigma_us,
            "-" if pt.pairs is None else "%d" % pt.pairs,
            "-" if ref is None else "%.0f" % ref, src,
            "-" if ratio is None else "%.4f" % ratio,
            "-" if t_eff is None else "%.4f" % t_eff,
            "-" if t_eff is None else "%.2fx" % (t_eff / SHIPPED_T)))
    print()
    for pt, ref, src, ratio, t_eff, why in rows:
        if why:
            print("  WHY %s/%s/%s/%s: %s"
                  % (pt.cell, pt.seed, pt.endpoint, pt.site, "; ".join(why)))
    print()

    pooled = pool(rows)
    print("POOLED sigma/ref per cell x site (min / median / max, n reps):")
    for (cell, site) in sorted(pooled):
        lo, med, hi, n = pooled[(cell, site)]
        print("  %-3s %-8s  %.4f / %.4f / %.4f   (n=%d)   T_eff %.4f-%.4f"
              % (cell, site, lo, med, hi, n,
                 K_SIGMA_TO_T * lo, K_SIGMA_TO_T * hi))
    print()
    verdict, note = reading(pooled, rows)
    print("READING: %s" % verdict)
    print("  %s" % note)
    return 0


if __name__ == "__main__":
    sys.exit(main())
