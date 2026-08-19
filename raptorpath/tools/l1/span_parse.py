#!/usr/bin/env python3
"""Parser + scorer for THE SPAN RUN (goal-gate "THE SPAN RUN —
PRE-REGISTRATION" — the CONTRACT; nothing here may reinterpret it).

TWO READOUTS, kept apart on purpose:

  THE SPAN BLOCK   `[CCAP] span= span_sigma= span_ratio= rate_fast= spread_us=`,
                   the instrument MEASUREMENT TRUTH item 4 built and the c9
                   battery lacked. C9-L3 is scored on `span_ratio` — the
                   ANCHOR-FREE quantity — and DISPOSED OF against the anchors
                   in the three-way rule this file implements LITERALLY:

                     ratio inside  [1.95, 2.05]                -> SHIPPED FORM
                     ratio outside, anchors inside             -> SHIPPED FORM
                                                                  FALSIFIED
                     ratio outside AND anchors outside         -> ANCHORS
                                                                  FALSIFIED,
                                                                  no span verdict

                   The bands are CITED from the c9 CONTRACT and appear here as
                   constants precisely so a reader can check them against it.

  THE SIGMA BLOCK  `[DIAG] ... sig_us=<µs>/n<count>` per path. REPORTED, NOT
                   SCORED — the delta prediction needs the full-cell pass. `n`
                   travels beside every sigma because the EWMA is seeded at 0
                   and retains 0.75^n of that seed, so a small-n sigma is
                   biased LOW and the READER discards it rather than the gauge
                   hiding it behind a threshold.

usage: python3 span_parse.py <ledger.log> <artifact-dir>
"""
import re
import sys
from pathlib import Path

# ── THE PRE-REGISTERED BANDS. Cited from the c9 CONTRACT; not derived here. ──
RATIO_PRED = 2.000
RATIO_BAND = (1.95, 2.05)          # rendering tolerance on an EXACT prediction
SPAN_BAND = (265.0, 315.0)         # C9-L3's absolute band, sym
RATE_FAST_BAND = (9370.0 * 0.95, 10400.0 * 1.05)   # +/-5% on the anchor range
SPREAD_BAND_US = (29880.0 * 0.996, 30000.0 * 1.004)  # +/-0.4%

CCAP_RE = re.compile(
    r"\[CCAP\] eng=(\d+)/(\d+) cap=([\d.]+) mem=([\d.]+) floor=([\d.]+) "
    r"floor_val=(\d+) brake=(\d+)/(\d+) brake_frac=([\d.]+) "
    r"span=([\d.]+) span_sigma=([\d.]+) span_ratio=([\d.]+) "
    r"rate_fast=([\d.]+) spread_us=([\d.]+)"
)
LEDGER_CCAP_RE = re.compile(r"^CCAP c9h/(\w+) rep=(\d+) (.*)$")
# ANCHORED ON `p<i>:infl=`, and both restrictions are load-bearing. The
# `[DIAG]` line carries a per-path listing BEFORE the per-path blocks that also
# renders `p<i>:` tokens, so a bare `p(\d+):` picks up a label from the listing
# and pairs it with the FIRST block's sigma — off by one path, silently. And
# `[^|]` rather than `.` because a block ends at its ` | ANCHOR` separator, so
# a dot-star walks into the next path's block. Caught by reading a raw line
# rather than by trusting the pattern: the miswired version reported
# `p0, p2, p3, p0` for a four-path quad, which is a count that cannot happen.
SIG_RE = re.compile(r"\bp(\d+):infl=[^|]*?\bsig_us=(-|\d+)/n(\d+)")


def scan_ledger(path):
    reps, aborts, liveness, emits = [], [], [], []
    for ln in Path(path).read_text(errors="replace").splitlines():
        m = LEDGER_CCAP_RE.match(ln.strip())
        if m:
            arm, rep, rest = m.group(1), int(m.group(2)), m.group(3)
            c = CCAP_RE.search(rest)
            if c:
                reps.append(dict(
                    arm=arm, rep=rep,
                    eng=int(c.group(1)), refresh=int(c.group(2)),
                    cap=float(c.group(3)), mem=float(c.group(4)),
                    floor=float(c.group(5)),
                    brake_frac=float(c.group(9)),
                    span=float(c.group(10)), span_sigma=float(c.group(11)),
                    span_ratio=float(c.group(12)),
                    rate_fast=float(c.group(13)), spread_us=float(c.group(14)),
                ))
        if ln.startswith("WITNESS") or ln.startswith("ABORT") \
                or ln.startswith("INSTRUMENT-FAIL"):
            aborts.append(ln.strip())
        if ln.startswith("LIVENESS ") or ln.startswith("ARM-LIVENESS-FAIL"):
            liveness.append(ln.strip())
        if ln.startswith("LIVENESS-EMIT"):
            emits.append(ln.strip())
    return reps, aborts, liveness, emits


def scan_sigma(artdir):
    """LAST [DIAG] line per client log — the most mature EWMA of that rep."""
    out = {}
    for p in sorted(Path(artdir).glob("cli-*.log")):
        last = None
        for ln in p.read_text(errors="replace").splitlines():
            if "[DIAG]" in ln:
                last = ln
        if last is None:
            out[p.name] = None
            continue
        clean = re.sub(r"\x1b\[[0-9;]*m", "", last)
        out[p.name] = [(int(i), (None if s == "-" else int(s)), int(n))
                       for i, s, n in SIG_RE.findall(clean)]
    return out


def inside(v, band):
    return band[0] <= v <= band[1]


def main():
    ledger, artdir = sys.argv[1], sys.argv[2]
    reps, aborts, liveness, emits = scan_ledger(ledger)

    print("== ABORT / WITNESS (read BEFORE any span number)")
    for a in aborts:
        print("  " + a)
    print("\n== LIVENESS (two-sided [GATES] echo, both endpoints)")
    for l in liveness:
        print("  " + l)
    print("\n== GAUGE EMISSION")
    for e in emits:
        print("  " + e)

    on = [r for r in reps if r["arm"] == "on"]
    print("\n== [CCAP] SPAN BLOCK, per rep (ON arm)")
    hdr = ("rep", "eng", "cap", "span", "span_sigma", "span_ratio",
           "rate_fast", "spread_us", "mem", "floor", "brake_frac")
    print("  " + " | ".join(f"{h:>10}" for h in hdr))
    for r in sorted(on, key=lambda x: x["rep"]):
        print("  " + " | ".join(f"{v:>10}" for v in (
            r["rep"], f"{r['eng']}/{r['refresh']}", f"{r['cap']:.1f}",
            f"{r['span']:.1f}", f"{r['span_sigma']:.1f}",
            f"{r['span_ratio']:.3f}", f"{r['rate_fast']:.1f}",
            f"{r['spread_us']:.1f}", f"{r['mem']:.4f}", f"{r['floor']:.4f}",
            f"{r['brake_frac']:.4f}")))

    live = [r for r in on if r["eng"] > 0]
    if not live:
        print("\nINSTRUMENT-FAIL: no ON rep with eng>0. NO SPAN VERDICT.")
        return
    n = len(live)
    mean = lambda k: sum(r[k] for r in live) / n
    m_ratio, m_span = mean("span_ratio"), mean("span")
    m_rf, m_sp = mean("rate_fast"), mean("spread_us")
    print(f"\n== MEANS over {n} live ON reps")
    print(f"  span_ratio = {m_ratio:.3f}   span = {m_span:.1f} sym   "
          f"rate_fast = {m_rf:.1f} sym/s   spread_us = {m_sp:.1f}")

    r_in = inside(m_ratio, RATIO_BAND)
    a_in = (inside(m_rf, RATE_FAST_BAND) and inside(m_sp, SPREAD_BAND_US))
    print(f"\n== C9-L3, the disposal rule applied LITERALLY")
    print(f"  ratio in {RATIO_BAND}?           {r_in}")
    print(f"  rate_fast in {tuple(round(x,1) for x in RATE_FAST_BAND)}? "
          f"{inside(m_rf, RATE_FAST_BAND)}")
    print(f"  spread_us in {tuple(round(x,1) for x in SPREAD_BAND_US)}? "
          f"{inside(m_sp, SPREAD_BAND_US)}")
    print(f"  span in {SPAN_BAND}?             {inside(m_span, SPAN_BAND)}")
    if r_in:
        print("  VERDICT: the SHIPPED rate_fast*(RTT_max-RTT_min) form is the "
              "one on the wire.")
    elif a_in:
        print("  VERDICT: the SHIPPED form is FALSIFIED (anchors inside their "
              "bands, ratio outside). Adopt nothing.")
    else:
        print("  VERDICT: the ANCHORS are falsified, not either formula. "
              "NO SPAN VERDICT.")

    print("\n== C9-L1 (prior half): span= reads at all, and reads nonzero")
    print(f"  span nonzero on {sum(1 for r in live if r['span'] > 0)}/{n} "
          f"live ON reps; gauge ABSENT on 0 of them")

    off = [r for r in reps if r["arm"] == "off"]
    print(f"\n== CONTROL ARM: [CCAP] lines on OFF reps = {len(off)} "
          f"(pre-registered: 0)")

    print("\n== ITEM 5 sig_us= (REPORTED, NOT SCORED) — last [DIAG] per rep")
    for name, paths in sorted(scan_sigma(artdir).items()):
        if paths is None:
            print(f"  {name}: NO [DIAG]")
            continue
        cells = ", ".join(
            f"p{i}={'-' if s is None else s} us/n{c}" for i, s, c in paths)
        print(f"  {name}: {cells}")


if __name__ == "__main__":
    main()
