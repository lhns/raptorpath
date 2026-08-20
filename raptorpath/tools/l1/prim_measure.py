#!/usr/bin/env python3
"""THE RECOVERY PRICE (goal #100), item 1 — THE PASSIVE PRIMITIVES.

Reads h and p off COMMITTED ledgers with no VM run, and reads sigma / nu / d
off a pass's raw logs when one is handed to it. Read-only in every mode: it
opens files and prints; it never writes, never touches the engine, and never
needs a gate flipped to produce h or p.

  h   per-symbol WIRE OVERHEAD, bytes beyond the symbol payload T.
  p   realised per-leg loss, from the netem qdisc's own drop counter.
  sig sigma, the srtt dispersion, from `[DIAG] sig_us=<us>/n<count>`.
  nu  fires per delivered symbol, from `[RACK] fa=<spur>/<fired>` / dgq_hand.
  d   delivery stall per frontier-blocking hole, from `[FDIAG]`.

WHY h IS BRACKETED AND NOT A POINT VALUE.  The committed ledgers carry two
counters on the data-direction legs and they count DIFFERENT populations:

  `dgq_hand`  symbols handed to the datagram queue, INCLUDING the ones netem
              subsequently dropped and the ones the queue never drained.
  `tc_pkts`   packets the qdisc actually SENT, INCLUDING the sender's own
              QUIC ACKs of the receiver's control stream, which carry no
              symbol at all.

Neither ratio is h on its own, but they bracket it from opposite sides and
the bracket is tight where the confounds are small:

  h_hi = tc_bytes/dgq_hand - T    charges every non-payload byte on the leg
                                  to a symbol, so ACK packets inflate it.
  h_lo = tc_bytes/tc_pkts  - T    charges every SENT packet with a full T of
                                  payload, so ACK packets deflate it.

At `c1` both confounds are near zero (drop 0.015 %, dgq_gap ~5 symbols,
FEC+retx under 1 % of handoffs) and the bracket closes; at the loaded lossy
duals it does not, and the script prints the bracket rather than picking a
number inside it.  MEASUREMENT DISCIPLINE: a quantity whose estimator has a
known bias gets its bias printed, not averaged away.

Usage
  prim_measure.py wire  <ledger-dir>            # h and p, committed data only
  prim_measure.py pass  <logdir>                # sigma / nu / d from a pass
"""

import json
import os
import re
import sys
from glob import glob

# Symbol size T for the Bulk/Auto profile (`net/mod.rs` BlockProfile).
T_BULK = 1200

RESULT_RE = re.compile(r"^[A-Z0-9_]*RESULT (\{.*\})\s*$")


# ── committed-ledger side: h and p ──────────────────────────────────────────

def load_records(ledger_dir):
    out = []
    for path in sorted(glob(os.path.join(ledger_dir, "**", "*.log"), recursive=True)):
        with open(path, "r", errors="replace") as fh:
            for line in fh:
                m = RESULT_RE.match(line.rstrip("\n"))
                if not m:
                    continue
                try:
                    rec = json.loads(m.group(1))
                except ValueError:
                    continue
                if not isinstance(rec, dict):
                    continue
                rec["_file"] = os.path.basename(path)
                out.append(rec)
    return out


def usable(rec):
    return (rec.get("tc_bytes") and rec.get("tc_pkts") and rec.get("dgq_hand")
            and rec.get("tc_drop") is not None and not rec.get("dnf"))


def quart(vals):
    v = sorted(vals)
    n = len(v)
    if not n:
        return (None, None, None)
    return (v[int(0.25 * n)], v[n // 2], v[min(n - 1, int(0.75 * n))])


def report_wire(ledger_dir, symbol_size=T_BULK):
    recs = [r for r in load_records(ledger_dir) if usable(r)]
    by = {}
    for r in recs:
        by.setdefault(str(r.get("cell")), []).append(r)

    print("h — PER-SYMBOL WIRE OVERHEAD, bytes beyond T=%d" % symbol_size)
    print("   (h_lo, h_hi bracket h; see the module docstring for the bias of each)")
    print()
    print("%-8s %6s %10s %10s %10s %9s %9s %9s" % (
        "cell", "n", "h_lo_med", "h_hi_med", "width", "pkt/sym", "drop", "gap/sym"))
    for cell in sorted(by):
        rs = by[cell]
        lo = [r["tc_bytes"] / r["tc_pkts"] - symbol_size for r in rs]
        hi = [r["tc_bytes"] / r["dgq_hand"] - symbol_size for r in rs]
        ps = [r["tc_pkts"] / r["dgq_hand"] for r in rs]
        dr = [r["tc_drop"] / (r["tc_pkts"] + r["tc_drop"]) for r in rs]
        gp = [(r.get("dgq_gap") or 0) / r["dgq_hand"] for r in rs]
        print("%-8s %6d %10.1f %10.1f %10.1f %9.4f %9.5f %9.5f" % (
            cell, len(rs), quart(lo)[1], quart(hi)[1],
            quart(hi)[1] - quart(lo)[1], quart(ps)[1], quart(dr)[1], quart(gp)[1]))

    print()
    print("p — REALISED LOSS from the netem qdisc's own drop counter")
    print("   NOTE: tc_drop/tc_pkts are SUMMED over the data-direction legs by")
    print("   the parsers, so a dual cell's p is a packet-weighted POOL of its")
    print("   two legs, not either leg. Per-leg p needs the sectioned -q.txt.")
    print()
    print("%-8s %6s %10s %10s %10s" % ("cell", "n", "p_p25", "p_med", "p_p75"))
    for cell in sorted(by):
        rs = by[cell]
        p = [r["tc_drop"] / (r["tc_pkts"] + r["tc_drop"]) for r in rs]
        a, b, c = quart(p)
        print("%-8s %6d %10.5f %10.5f %10.5f" % (cell, len(rs), a, b, c))


# ── per-leg p, from the sectioned qdisc captures ────────────────────────────

QDEV = re.compile(r"^==\s+(CLI\d|SRV\d)")
QSENT = re.compile(r"Sent (\d+) bytes (\d+) pkts? \(dropped (\d+)")
QNETEM = re.compile(r"gemodel p ([\d.]+)% r ([\d.]+)%")


def report_qdisc(paths):
    print("p PER LEG — sectioned qdisc captures (data-direction legs only)")
    print()
    print("%-34s %-6s %12s %10s %10s %10s" % (
        "capture", "leg", "sent_pkts", "dropped", "p_real", "p_ge_mean"))
    for path in paths:
        cur, ge = None, None
        with open(path, "r", errors="replace") as fh:
            for line in fh:
                m = QDEV.match(line)
                if m:
                    cur = m.group(1)
                    g = QNETEM.search(line)
                    ge = None
                    continue
                if cur is None:
                    continue
                g = QNETEM.search(line)
                if g:
                    pp, rr = float(g.group(1)), float(g.group(2))
                    ge = pp / (pp + rr) if (pp + rr) else 0.0
                s = QSENT.search(line)
                if s and cur.startswith("CLI"):
                    sent, pkts, drop = (int(x) for x in s.groups())
                    tot = pkts + drop
                    print("%-34s %-6s %12d %10d %10.5f %10s" % (
                        os.path.basename(path), cur, pkts, drop,
                        drop / tot if tot else 0.0,
                        ("%.5f" % ge) if ge is not None else "-"))
                    cur = None


# ── pass side: sigma / nu / d ───────────────────────────────────────────────

SIG = re.compile(r"p(\d+):.*?sig_us=(\d+)/n(\d+)")
DGQ = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+)")
RACK = re.compile(r"\[RACK\].*?fa=(\d+)/(\d+)")
FDIAG = re.compile(
    r"\[FDIAG\] frontier=(\d+) seen=(\d+) gap=(\d+) probe_holes=(\d+) "
    r"probe_buffered=(\d+) \| DECODE n=(\d+) avg=(\d+)us present_at_stall=(\d+) "
    r"\| SOURCE n=(\d+) avg=(\d+)us")


def scan_sigma(path):
    """LAST [DIAG] per path — the pre-committed reading rule of the c8 sigma
    pass (the estimator is an EWMA; the latest emission is the converged one).
    """
    last = {}
    with open(path, "r", errors="replace") as fh:
        for line in fh:
            for m in SIG.finditer(line):
                last[int(m.group(1))] = (int(m.group(2)), int(m.group(3)))
    return last


def scan_dgq(path):
    last = {}
    with open(path, "r", errors="replace") as fh:
        for line in fh:
            for m in DGQ.finditer(line):
                last[int(m.group(1))] = (int(m.group(2)), int(m.group(3)))
    return last


def scan_rack(path):
    """`fa=<spurious>/<fired>`, selected by MAX denominator — the same rule
    `ccand_parse.py` uses, because the gauge emits cumulatively on Drop and a
    later line with a smaller denominator is a second gauge instance."""
    best = None
    with open(path, "r", errors="replace") as fh:
        for line in fh:
            m = RACK.search(line)
            if m:
                spur, fired = int(m.group(1)), int(m.group(2))
                if best is None or fired > best[1]:
                    best = (spur, fired)
    return best


RETX = re.compile(r"retx=(\d+)")


def scan_retx(path):
    last = None
    with open(path, "r", errors="replace") as fh:
        for line in fh:
            for m in RETX.finditer(line):
                last = int(m.group(1))
    return last


def count_tag(path, tag):
    n = 0
    with open(path, "r", errors="replace") as fh:
        for line in fh:
            if tag in line:
                n += 1
    return n


def scan_fdiag(path):
    """Every [FDIAG] line, in order. The counters are CUMULATIVE, so the
    windowed mean between two lines is (us_b - us_a)/(n_b - n_a) with
    us = avg*n recovered to integer-division precision."""
    out = []
    with open(path, "r", errors="replace") as fh:
        for line in fh:
            m = FDIAG.search(line)
            if m:
                (fr, seen, gap, holes, buf,
                 dn, davg, pres, sn, savg) = (int(x) for x in m.groups())
                out.append(dict(frontier=fr, seen=seen, gap=gap, holes=holes,
                                buffered=buf, dec_n=dn, dec_avg=davg,
                                present=pres, src_n=sn, src_avg=savg))
    return out


def windowed(lines, warm_frac=0.2):
    """d, with the WARM-UP EXCLUSION the pre-registration fixes: drop the
    first `warm_frac` of emitted [FDIAG] lines and difference the cumulative
    counters across the remainder. Returns (d_decode_us, n_decode,
    d_source_us, n_source) or None when the surviving window carries no
    resolved hole."""
    if len(lines) < 2:
        return None
    a = lines[int(warm_frac * len(lines))]
    b = lines[-1]
    dn = b["dec_n"] - a["dec_n"]
    sn = b["src_n"] - a["src_n"]
    dus = b["dec_avg"] * b["dec_n"] - a["dec_avg"] * a["dec_n"]
    sus = b["src_avg"] * b["src_n"] - a["src_avg"] * a["src_n"]
    return ((dus / dn if dn > 0 else None), dn,
            (sus / sn if sn > 0 else None), sn)


def report_pass(logdir):
    cli = sorted(glob(os.path.join(logdir, "*-c.log")))
    srv = sorted(glob(os.path.join(logdir, "*-s.log")))
    print("sigma — LAST [DIAG] per path, sender log (us / sample count)")
    print("%-30s %-40s" % ("rep", "per-path sig_us/n"))
    for path in cli:
        s = scan_sigma(path)
        txt = "  ".join("p%d:%d/n%d" % (k, v[0], v[1]) for k, v in sorted(s.items()))
        print("%-30s %s" % (os.path.basename(path), txt or "(none)"))

    print()
    print("nu — fires per symbol handed, sender log")
    print("   ABSENCE OF [RACK] IS A DATUM, NOT A MISSING FILE. The gauge's")
    print("   Drop emits on `self.on || self.fired > 0` (net/mod.rs:4380-4390),")
    print("   so with RWM_RACK_CLOCKS off — the shipped default — no line at")
    print("   all means fired == 0 exactly. `retx=` is the corroborating")
    print("   counter and is printed beside it.")
    print("%-30s %10s %10s %10s %8s %12s" % (
        "rep", "fired", "spurious", "dgq_hand", "retx", "nu"))
    for path in cli:
        rk = scan_rack(path)
        dg = scan_dgq(path)
        hand = sum(v[0] for v in dg.values()) or 0
        rtx = scan_retx(path)
        fired = rk[1] if rk else 0
        spur = rk[0] if rk else 0
        nu = ("%.5f" % (fired / hand)) if hand else "-"
        print("%-30s %10d %10d %10d %8s %12s" % (
            os.path.basename(path), fired, spur, hand,
            rtx if rtx is not None else "-", nu))

    print()
    print("d — per-hole delivery stall, receiver log, WARM-UP EXCLUDED")
    print("   `gap`/`holes` are the BURST-WIDTH evidence the attribution rule")
    print("   requires beside every d: an episode covers every hole open at")
    print("   the time, so d bounds per-symbol stall from ABOVE.")
    print("%-30s %6s %12s %7s %12s %7s %8s %7s %6s" % (
        "rep", "lines", "d_decode_us", "n_dec", "d_source_us", "n_src",
        "gap_max", "hol_max", "wedge"))
    for path in srv:
        lines = scan_fdiag(path)
        wedge = count_tag(path, "[WEDGE]")
        w = windowed(lines)
        gmax = max((x["gap"] for x in lines), default=0)
        hmax = max((x["holes"] for x in lines), default=0)
        if not w:
            print("%-30s %6d %12s %7s %12s %7s %8d %7d %6d" % (
                os.path.basename(path), len(lines), "-", "-", "-", "-",
                gmax, hmax, wedge))
            continue
        dus, dn, sus, sn = w
        print("%-30s %6d %12s %7d %12s %7d %8d %7d %6d" % (
            os.path.basename(path), len(lines),
            ("%.0f" % dus) if dus is not None else "-", dn,
            ("%.0f" % sus) if sus is not None else "-", sn,
            gmax, hmax, wedge))


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    mode = sys.argv[1]
    if mode == "wire":
        report_wire(sys.argv[2])
    elif mode == "qdisc":
        report_qdisc(sys.argv[2:])
    elif mode == "pass":
        report_pass(sys.argv[2])
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
