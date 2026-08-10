#!/usr/bin/env python3
"""WHAT BINDS THROUGHPUT - the wait-reason ledger, read off an existing
battery's per-run `[DIAG]` logs. Analysis only: it runs nothing, changes
nothing, and needs no VM.

    python3 bind_analyze.py <diag-dir> ledger [cell ...]   per-rep A vs B
    python3 bind_analyze.py <diag-dir> cells  [cell ...]   all arms, one row each
    python3 bind_analyze.py <diag-dir> tc     [cell ...]   tc/netem counters (-q.txt)
    python3 bind_analyze.py <diag-dir> anchor [cell ...]   arm D tax vs symbol rate
    python3 bind_analyze.py <diag-dir> work   [cell ...]   recovery-plane work rate

Written for goal-gate "What Binds Throughput" against the three-term
battery's `diag/` tree (`<cell>-<arm>-s<seed>-r<rep>-{c,s}.log`, optional
`-q.txt`), but the parsers key on `[DIAG]` field names, so any battery whose
driver preserved per-run diag under the same naming works.

THE ONE ASSUMED CONSTANT, and why it is not a fitted one: converting a
symbol rate to a wire rate needs the on-wire bytes per symbol datagram.
That number is MEASURED from the battery's own tc counters wherever a
`-q.txt` exists (`tc` mode prints it: 1264-1287 B across the 32 jit25 runs,
all four arms, against a 1200 B symbol). `BSYM` below is inside that
measured range, and every conclusion in the goal-gate section holds for
anything in 1200-1300, which is why no verdict rests on the choice.
"""
import os
import re
import statistics as st
import sys
from collections import defaultdict

BSYM = 1270  # on-wire bytes per 1200 B symbol datagram; see module docstring

# Shaped bottleneck capacity per cell, in bit/s, from `tools/l1/lib.sh`
# scenario_params and `adv_cells.sh`. Dual-path cells sum their legs: that
# is the aggregate a sender striping over both can occupy.
CAP = {
    "c1": 1000e6,        # c1 single: 1 gbit, RTT 2 ms
    "sc2": 100e6,        # c2 single: 100 mbit
    "sc3": 20e6,         # c3 single: 20 mbit
    "c7": 200e6,         # c2/c2 dual
    "c8": 120e6,         # c2/c3 dual
    "c2r100": 100e6,     # 100 mbit, RTT 100 ms
    "c2r200": 100e6,     # 100 mbit, RTT 200 ms
    "jit25": 100e6,      # 100 mbit, RTT 40 ms, +-25 ms jitter
    "shal8": 100e6,      # 100 mbit tbf, 8-PACKET child queue (the real
                         # ceiling is the queue, not the rate -- util
                         # against 100 mbit is a LOWER bound there)
}

P = {
    "t": r"\[DIAG\] t=([0-9.]+)s",
    "win": r"win=(\d+)/(\d+)",
    "paused": r"paused=([0-9.]+)%",
    "good": r"good=([0-9.]+)Mbit",
    "src": r"src=([0-9.]+)sym/s",
    "cod": r"cod=([0-9.]+)sym/s",
    "cum": r"cum=(\d+)/(\d+)/(\d+)",
    "sidle": r"sidle=(\d+)ms/(\d+)/mx(\d+)ms",
    "cwnd": r"cwnd=(\d+) infl=(\d+) np=(\d+)",
    "rtt": r"rtt=([0-9.]+)ms bdp100",
    "retx": r"retx=(\d+)",
    "wnd2": r"wnd2=(\d+)/(\d+) relgap=(\d+)ms/mx(\d+)ms",
    "rtp": r"rtp([0-9.]+)ms",
    "mpr": r"mpr\[rep=(\d+) seqs=(\d+) fired=(\d+) y=(\d+) r=(\d+) fast=(\d+) coal=(\d+) supp=(\d+)/(\d+)/(\d+)",
}
P = {k: re.compile(v) for k, v in P.items()}
NAME = re.compile(r"^([a-z0-9]+)-([ABCD])-s(\d+)-r(\d+)-([cs])\.log$")
QNAME = re.compile(r"^([a-z0-9]+)-([ABCD])-s(\d+)-r(\d+)-q\.txt$")


def diag_rows(path):
    """Every `[DIAG]` line as a dict of matched field tuples."""
    out = []
    with open(path, errors="replace") as fh:
        for line in fh:
            if "[DIAG]" not in line:
                continue
            r = {k: tuple(float(x) for x in m.groups())
                 for k, p in P.items() for m in [p.search(line)] if m}
            if "t" in r:
                out.append(r)
    return out


def steady(rows, t_lo=2.0):
    """After `t_lo` s, dropping the final window (a partial holding the drain)."""
    s = [x for x in rows if x["t"][0] >= t_lo]
    return s[:-1] if len(s) > 1 else s


def rep_stats(path, cell):
    rows = diag_rows(path)
    s = steady(rows)
    if len(s) < 4:
        return None
    med = lambda k, i=0: st.median(x[k][i] for x in s if k in x)
    mean = lambda k, i=0: st.mean(x[k][i] for x in s if k in x)
    has = lambda k: any(k in x for x in s)
    d = dict(
        n=len(s), dur=rows[-1]["t"][0],
        good=mean("good"), src=mean("src"), cod=mean("cod"),
        win=med("win"), cap=med("win", 1), paused=mean("paused"),
        cwnd=med("cwnd"), infl=med("cwnd", 1), np=med("cwnd", 2),
        rtt=med("rtt"), rtp=med("rtp") if has("rtp") else float("nan"),
        head=med("wnd2") if has("wnd2") else float("nan"),
        hole=med("wnd2", 1) if has("wnd2") else float("nan"),
        retx=rows[-1]["retx"][0],
        srccum=rows[-1]["cum"][0], codcum=rows[-1]["cum"][1],
    )
    dt = rows[-1]["t"][0] - s[0]["t"][0]
    d["sidle"] = (rows[-1]["sidle"][0] - s[0]["sidle"][0]) / 10.0 / dt if dt else 0.0
    if "mpr" in rows[-1] and "mpr" in s[0] and dt:
        d["gaprep"] = (rows[-1]["mpr"][0] - s[0]["mpr"][0]) / dt
        d["gapseq"] = (rows[-1]["mpr"][1] - s[0]["mpr"][1]) / dt
        d["seqs_per_rep"] = d["gapseq"] / max(d["gaprep"], 1e-9)
    d["sym"] = d["src"] + d["cod"]
    d["wire"] = d["sym"] * BSYM * 8
    d["util"] = 100.0 * d["wire"] / CAP[cell] if cell in CAP else float("nan")
    d["codpct"] = 100.0 * d["cod"] / d["sym"] if d["sym"] else 0.0
    d["queue"] = d["rtt"] - d["rtp"]
    d["retx_k"] = 1000.0 * d["retx"] / max(d["srccum"], 1)
    d["occ_frac"] = d["win"] / max(d["cap"], 1)
    return d


def group(diag, cells, side="c"):
    g = defaultdict(list)
    for fn in sorted(os.listdir(diag)):
        m = NAME.match(fn)
        if not m:
            continue
        cell, arm, seed, rep, sd = m.groups()
        if sd != side or (cells and cell not in cells):
            continue
        d = rep_stats(os.path.join(diag, fn), cell)
        if d:
            d["rep"] = int(rep)
            g[(cell, arm, seed)].append(d)
    return g


def mean_of(acc, k):
    v = [x[k] for x in acc if k in x and x[k] == x[k]]
    return st.mean(v) if v else float("nan")


ROW = ("{:7s}{:>2s}{:>3s}{:3d} |{good:6.1f}{sym:7.0f}{util:7.1f} |{paused:7.1f}{sidle:7.1f} |"
       "{win:6.0f}/{cap:<5.0f}{occ_frac:5.2f} |{cwnd:6.0f}{infl:5.0f} |"
       "{rtt:6.0f}{rtp:6.0f}{queue:7.0f} |{codpct:5.1f}{retx_k:8.1f}")
HDR = ("cell    arm  s  n |  good  sym/s  util% | paused% sidle% |  win/cap   occ |"
       "  cwnd infl |   RTT RTpro  queue |cod% retx/1k")


def cmd_ledger(diag, cells):
    """Per-rep A vs B: nothing hides behind a mean."""
    g = group(diag, cells)
    for cell in (cells or sorted({k[0] for k in g})):
        for seed in ("42", "7"):
            rows = {a: sorted(g.get((cell, a, seed), []), key=lambda d: d["rep"])
                    for a in "AB"}
            if not any(rows.values()):
                continue
            print(f"\n===== {cell} seed {seed} - steady state, per rep =====")
            print(HDR)
            for arm in "AB":
                for d in rows[arm]:
                    print(ROW.format(cell, arm, seed, d["rep"], **d))
                if rows[arm]:
                    m = {k: mean_of(rows[arm], k) for k in rows[arm][0] if k != "rep"}
                    sd = st.pstdev([x["util"] for x in rows[arm]])
                    print(ROW.format(cell, arm, seed, len(rows[arm]), **m)
                          + f"   (sd util {sd:.1f})")


def cmd_cells(diag, cells):
    """One row per (cell, arm, seed) - the whole battery at a glance."""
    g = group(diag, cells)
    print(HDR)
    print("-" * len(HDR))
    for key in sorted(g, key=lambda k: (k[0], k[2], k[1])):
        acc = g[key]
        m = {k: mean_of(acc, k) for k in acc[0] if k != "rep"}
        print(ROW.format(key[0], key[1], key[2], len(acc), **m))


def cmd_work(diag, cells):
    """The recovery plane's O(outstanding) gap walk vs the outstanding limit."""
    g = group(diag, cells)
    print("cell    arm  s  n |  win  sym/s | gaprep/s  gapseq/s  seqs/rep | sidle%")
    print("-" * 70)
    for key in sorted(g, key=lambda k: (k[0], k[2], k[1])):
        acc = g[key]
        m = lambda k: mean_of(acc, k)
        print(f"{key[0]:7s}{key[1]:>3s}{key[2]:>3s}{len(acc):3d} | {m('win'):4.0f}"
              f" {m('sym'):6.0f} | {m('gaprep'):8.0f} {m('gapseq'):9.0f}"
              f" {m('seqs_per_rep'):9.0f} | {m('sidle'):5.1f}")


def cmd_anchor(diag, cells):
    """Arm D (RWM_PLAIN_RS alone) vs arm A, ordered by arm A's symbol rate.

    A store-sizing tax would track how hard the store binds (occ, paused).
    A per-symbol cost would track the absolute symbol rate and vanish below
    the sender's software ceiling. The ordering separates the two by eye.
    """
    g = group(diag, cells)
    out = []
    for (cell, arm, seed), acc in g.items():
        if arm != "A":
            continue
        d = g.get((cell, "D", seed))
        if d:
            out.append((cell, seed, acc, d))
    out.sort(key=lambda x: -mean_of(x[2], "sym"))
    print("ordered by arm A symbol rate - the tax's argument, if it has one")
    print("cell    s  nA nD |  A sym/s  D sym/s |  D/A  | A paused%  A occ/cap | A util%")
    print("-" * 80)
    for cell, seed, a, d in out:
        sa, sd_ = mean_of(a, "sym"), mean_of(d, "sym")
        # n < 4 on either arm is the seed-7/c8 abort class: printed so it is
        # visible, never silently pooled with the full cells.
        flag = "  <- n too small, NOT a datum" if min(len(a), len(d)) < 4 else ""
        print(f"{cell:7s}{seed:>3s}{len(a):3d}{len(d):3d} | {sa:8.0f} {sd_:8.0f}"
              f" | {sd_/sa:5.3f} | {mean_of(a,'paused'):9.1f}"
              f" {mean_of(a,'occ_frac'):10.2f} | {mean_of(a,'util'):7.1f}{flag}")


QSENT = re.compile(r"Sent (\d+) bytes (\d+) pkt \(dropped (\d+),")


def cmd_tc(diag, cells):
    """tc/netem counters - the ONLY unmediated view of what reached the wire.

    `B/symbol` is tc's own byte count divided by the symbols the sender says
    it handed off. It should equal the true on-wire datagram size; well below
    it means handoffs did not reach the qdisc.
    """
    g = defaultdict(list)
    for fn in sorted(os.listdir(diag)):
        m = QNAME.match(fn)
        if not m:
            continue
        cell, arm, seed, rep = m.groups()
        if cells and cell not in cells:
            continue
        cur, q = None, {}
        for line in open(os.path.join(diag, fn), errors="replace"):
            if line.startswith("== CLI0"):
                cur = "cli"; continue
            if line.startswith("== SRV0-INGRESS"):
                cur = None; continue
            if line.startswith("== SRV0"):
                cur = "srv"; continue
            mm = QSENT.search(line) if cur else None
            if mm and cur not in q:
                q[cur] = tuple(int(x) for x in mm.groups())
        cpath = os.path.join(diag, f"{cell}-{arm}-s{seed}-r{rep}-c.log")
        if "cli" not in q or not os.path.exists(cpath):
            continue
        rows = diag_rows(cpath)
        if not rows:
            continue
        dur = rows[-1]["t"][0]
        syms = rows[-1]["cum"][0] + rows[-1]["cum"][1]
        g[(cell, arm, seed)].append(q["cli"] + (dur, syms))
    print("cell    arm  s  n |  dur s | cli0 Mbit/s | LINK util% | B/symbol | tc drop%")
    print("-" * 76)
    for key in sorted(g, key=lambda k: (k[0], k[2], k[1])):
        rows = g[key]
        cap = CAP.get(key[0], 100e6)
        util = [r[0] * 8 / r[3] / cap * 100 for r in rows]
        print(f"{key[0]:7s}{key[1]:>3s}{key[2]:>3s}{len(rows):3d} |"
              f" {st.mean(r[3] for r in rows):6.1f} |"
              f" {st.mean(r[0]*8/r[3]/1e6 for r in rows):11.1f} |"
              f" {st.mean(util):5.1f} +-{st.pstdev(util):4.1f} |"
              f" {st.mean(r[0]/max(r[4],1) for r in rows):8.0f} |"
              f" {st.mean(100.0*r[2]/max(r[1]+r[2],1) for r in rows):8.2f}")


CMDS = {"ledger": cmd_ledger, "cells": cmd_cells, "tc": cmd_tc,
        "anchor": cmd_anchor, "work": cmd_work}


def main(argv):
    if len(argv) < 3 or argv[2] not in CMDS:
        print(__doc__)
        return 2
    diag, mode, cells = argv[1], argv[2], argv[3:]
    if not os.path.isdir(diag):
        print(f"no such diag dir: {diag}", file=sys.stderr)
        return 2
    CMDS[mode](diag, cells)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
