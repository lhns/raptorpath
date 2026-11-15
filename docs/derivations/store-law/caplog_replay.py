#!/usr/bin/env python3
"""Offline replay of the engine's `[CAPLOG]` records through the store-cap law.

The B-1 gauge build (store-law.md R5.6, R4.8) logs, per store-cap refresh
whose inputs changed, the inputs and the cap the engine chose. This script
recomputes every cap from the inputs and compares: a mismatch means the logged
cap is not what `pooled_store_cap` (or the chain/boot fall-through) gives for
the logged inputs, i.e. the gauge perturbed the seat or the log is wrong.

The arithmetic mirrors `raptorpath/src/net/store_cap.rs` in operation order,
because IEEE-754 doubles round per operation (1.1 * 1000.0 is 1100.0000000000002):

    pooled_store_cap_unclamped = ((value) * (count)) * psum
        value = 1 + q(delta)    if dc  else gain          (q is logged: engine's own codel_setpoint_q)
        count = 1               if sum else n_live
    pooled_store_cap = clamp(ceil(unclamped), floor, max(n_live * pool, floor))   # None if !on or psum <= 0
    chain  = clamp(ceil(gain * psum), floor, smax)
    boot   = min(boot, smax)
    tt     = the three-term law: not replayable here (reported, never a mismatch)

Usage: caplog_replay.py LOG [LOG...]    exit 1 on any mismatch.
"""
import math
import re
import sys

# codel_setpoint_q's band (RFC 8289 s3.2) and the dial's two named points
# (store_cap.rs); used only to cross-check the logged q, never to replace it.
Q_LO, Q_HI, B_LO, B_HI = 0.05, 0.10, 0.5, 2.0

_KV = re.compile(r"(\w+)=(\S+)")


def parse_line(line):
    """One `[CAPLOG]` line -> dict (typed), or None if it is not one."""
    if "[CAPLOG]" not in line:
        return None
    kv = dict(_KV.findall(line.split("[CAPLOG]", 1)[1]))
    try:
        return {
            "i": int(kv["i"]), "t": int(kv["t"]), "seat": kv["seat"], "br": kv["br"],
            "on": kv["on"] == "1", "sum": kv["sum"] == "1", "dc": kv["dc"] == "1",
            "b": float(kv["b"]), "q": float(kv["q"]), "n": int(kv["n"]),
            "psum": float(kv["psum"]), "gain": float(kv["gain"]),
            "floor": int(kv["floor"]), "pool": int(kv["pool"]), "smax": int(kv["smax"]),
            "boot": int(kv["boot"]), "cap": int(kv["cap"]), "rep": int(kv.get("rep", 0)),
        }
    except (KeyError, ValueError):
        return None


def q_of_b(b):
    """codel_setpoint_q at the shipped dial endpoints (cross-check only)."""
    t = (min(max(b, B_LO), B_HI) - B_LO) / (B_HI - B_LO)
    return Q_LO + (Q_HI - Q_LO) * t


def _clamp(x, lo, hi):
    return min(max(x, lo), hi)


def replay(r):
    """The cap the law gives for record r's inputs; None if not replayable."""
    br = r["br"]
    if br == "pooled":
        if not r["on"] or r["psum"] <= 0.0:
            return None
        value = 1.0 + r["q"] if r["dc"] else r["gain"]
        count = 1.0 if r["sum"] else float(r["n"])
        ceiling = max(r["n"] * r["pool"], r["floor"])
        return _clamp(math.ceil(value * count * r["psum"]), r["floor"], ceiling)
    if br == "chain":
        return _clamp(math.ceil(r["gain"] * r["psum"]), r["floor"], r["smax"])
    if br == "boot":
        return min(r["boot"], r["smax"])
    return None  # tt


def replay_log(lines):
    """-> dict(records, replayed, mismatches[list of (rec, want)], unreplayable, q_off)."""
    out = {"records": 0, "replayed": 0, "mismatches": [], "unreplayable": 0, "q_off": 0, "refreshes": 0}
    for ln in lines:
        r = parse_line(ln)
        if r is None:
            continue
        out["records"] += 1
        out["refreshes"] += 1 + r["rep"]
        if abs(r["q"] - q_of_b(r["b"])) > 1e-9:
            out["q_off"] += 1  # a non-default band or dial override; informational
        want = replay(r)
        if want is None:
            out["unreplayable"] += 1
        else:
            out["replayed"] += 1
            if want != r["cap"]:
                out["mismatches"].append((r, want))
    return out


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    lines = []
    for p in argv:
        with open(p, encoding="utf-8", errors="replace") as f:
            lines.extend(f)
    o = replay_log(lines)
    print(f"caplog records={o['records']} (refreshes covered {o['refreshes']}) replayed={o['replayed']} "
          f"mismatches={len(o['mismatches'])} unreplayable(tt)={o['unreplayable']} q_off_default_band={o['q_off']}")
    for r, want in o["mismatches"][:10]:
        print(f"  MISMATCH i={r['i']} seat={r['seat']} br={r['br']} logged cap={r['cap']} replay={want}")
    return 1 if o["mismatches"] or o["records"] == 0 else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
