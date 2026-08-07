#!/usr/bin/env python3
"""Parse a patience_battery.sh log into the pre-registered verdict tables.

Goal-gate "Unlock The Default 2: derived patience". Emits, per arm:
  * goodput mean +/- sample sigma with n (MEASUREMENT DISCIPLINE 4/8 --
    n is always quoted, nothing is discarded),
  * the same-session Sigma ratios the c7 and c8 clauses are stated in
    (Sigma is built from THAT ARM'S OWN singles, never a pooled one),
  * the mechanism gauges the falsification clause requires: the pf=
    floor/clock split, sidle vs the derived sidle2, evt/sthr, sweeps,
    retx, gapdrop, paused.

  usage: patience_parse.py <battery-s42.log> [battery-s7.log ...]
"""
import re
import sys
from collections import defaultdict

RUN = re.compile(r"^=== rep=(\d+) arm=(\S+) attempt=(\d+) ")
MBPS = re.compile(r'"mean_mbps":([0-9.]+)')
DNF = re.compile(r'"dnf":(\d+)')


def stats(xs):
    n = len(xs)
    if n == 0:
        return (0.0, 0.0, 0)
    m = sum(xs) / n
    if n < 2:
        return (m, 0.0, n)
    var = sum((x - m) ** 2 for x in xs) / (n - 1)
    return (m, var ** 0.5, n)


def parse(path):
    good = defaultdict(list)
    mech = defaultdict(list)
    dnfs = defaultdict(list)
    retries = lost = 0
    flags = defaultdict(int)
    cur = None
    for line in open(path, errors="replace"):
        m = RUN.match(line)
        if m:
            cur = m.group(2)
            continue
        if line.startswith("RUN-RETRY"):
            retries += 1
        elif line.startswith("RUN-LOST"):
            lost += 1
        elif line.startswith("ARM-LIVENESS-FAIL"):
            flags["liveness"] += 1
        elif line.startswith("ARM-CONTAMINATION"):
            flags["contamination"] += 1
        elif line.startswith("ARM-GAUGE-FAIL"):
            flags["gauge"] += 1
        if cur is None:
            continue
        g = MBPS.search(line)
        if g:
            good[cur].append(float(g.group(1)))
        d = DNF.search(line)
        if d:
            dnfs[cur].append(int(d.group(1)))
        if line.startswith("MECH "):
            mech[cur].append(line[5:].strip())
    return good, mech, dnfs, retries, lost, flags


def field(entries, pat):
    """Collect a numeric field across an arm's MECH lines."""
    out = []
    rx = re.compile(pat)
    for e in entries:
        m = rx.search(e)
        if m:
            out.append(tuple(float(x) for x in m.groups()))
    return out


def rng(vals, i=0, fmt="{:.0f}"):
    if not vals:
        return "-"
    xs = [v[i] for v in vals]
    lo, hi = min(xs), max(xs)
    return (fmt + "-" + fmt).format(lo, hi) if lo != hi else fmt.format(lo)


ARMS = ["prior", "est", "pat", "patonly"]
CELLS = ["c1", "sc2", "sc3", "c7", "c8"]

for path in sys.argv[1:]:
    good, mech, dnfs, retries, lost, flags = parse(path)
    print("=" * 78)
    print(path)
    print(
        f"run health: RUN-RETRY={retries} RUN-LOST={lost} "
        f"liveness-fail={flags['liveness']} contamination={flags['contamination']} "
        f"gauge-fail={flags['gauge']}"
    )
    alldnf = [d for v in dnfs.values() for d in v]
    print(f"dnf: max={max(alldnf) if alldnf else 'n/a'} over {len(alldnf)} completed runs")
    print()

    # Goodput + same-session Sigma ratios.
    print("GOODPUT (Mbit/s, mean +/- sigma_s (n))")
    for cell in CELLS:
        for arm in ARMS:
            k = f"{cell}-{arm}"
            m, s, n = stats(good.get(k, []))
            if n == 0:
                continue
            extra = ""
            if cell == "c7":
                sm, _, sn = stats(good.get(f"sc2-{arm}", []))
                if sn:
                    extra = f"  = {m / (2 * sm):.3f}xSigma"
            if cell == "c8":
                s2, _, n2 = stats(good.get(f"sc2-{arm}", []))
                s3, _, n3 = stats(good.get(f"sc3-{arm}", []))
                if n2 and n3:
                    extra = f"  = {m / (s2 + s3):.3f}xSigma"
            vals = sorted(good[k])
            print(f"  {k:<14} {m:7.1f} +/- {s:5.1f} ({n})  [{vals[0]:.1f}-{vals[-1]:.1f}]{extra}")
        print()

    # The mechanism gauges.
    print("MECHANISM GAUGES (end-of-run, per-rep ranges)")
    hdr = f"  {'arm':<14} {'pf floor/clock/mean':<26} {'sidle ms/n':<16} {'sidle2 ms/n':<16} {'evt us':<12} {'sthr us':<10} {'sweeps':<9} {'retx':<12} {'gapdrop':<10}"
    for cell in CELLS:
        print(f"-- {cell}")
        print(hdr)
        for arm in ARMS:
            e = mech.get(f"{cell}-{arm}", [])
            if not e:
                continue
            pf = field(e, r"pf=(\d+)/(\d+)/(\d+)")
            sid = field(e, r"sidle=(\d+)ms/(\d+)/")
            sid2 = field(e, r"sidle2=(\d+)ms/(\d+)/")
            evt = field(e, r"evt=(\d+)us")
            sthr = field(e, r"sthr=(\d+)us")
            sw = field(e, r"sweeps=(\d+)")
            rx = field(e, r"retx=(\d+)")
            gd = field(e, r"gapdrop=(\d+)")
            pfs = (
                f"{rng(pf,0)}/{rng(pf,1)}/{rng(pf,2)}" if pf else "-"
            )
            print(
                f"  {arm:<14} {pfs:<26} "
                f"{rng(sid,0)+'/'+rng(sid,1):<16} {rng(sid2,0)+'/'+rng(sid2,1):<16} "
                f"{rng(evt):<12} {rng(sthr):<10} {rng(sw):<9} {rng(rx):<12} {rng(gd):<10}"
            )
        print()
