#!/usr/bin/env python3
"""THE ESTIMATOR BATTERY'S PER-INVOCATION PARSER — goal #101 item 2's VM half.

  usage: sigb_parse.py <cell> <seed> <rep> <cli.log> <srv.log> <q.txt|-> \
                       [<ping-i.txt> ...]

Writes the ledger rows for ONE invocation to stdout. The driver appends them.

WHAT IT EMITS, and the one rule that governs all of it: **THE LEDGER IS RAW.**

    SIGBREAD  <cell> <seed> <rep> <site> p<id> blk=<i> \
              sig=<v|->/<n> rvar=<v|->/<n> qsp=<v|->/<n> msd=<v|->/<n>

One row per PATH per `[DIAG]` BLOCK per SITE — every emission, warm-up
included, `-` preserved as `-`. **The clause-`C1` warm-up exclusions are NOT
applied here.** Clause `C3` of the acceptance bar makes them a scoring rule on
the parser and forbids them being a gate in the engine; putting them in the
INVOCATION parser instead would have the same effect on the ledger — the
excluded readings would not exist and nobody could re-derive the exclusion or
check it. So they are applied in `sigb_report.py`, against the `n` carried on
each row, and the ledger keeps every reading the engine emitted.

    SIGBPROBE <cell> <seed> <rep> <json>      one per LEG, `sigb_probe.py`
    SIGBMETA  <cell> <seed> <rep> <json>      transfer wall, goodput, rc
    SIGBWITNESS <json>                        W1/W2/W4'/W5/W7 + the band

THE FOUR FIELDS ARE READ AS ONE GROUP, NOT FOUR INDEPENDENT REGEXES. They are
emitted by ONE `format!` in `net/diag.rs` on one line per path, so a row in
which three are present and one is missing is an ENGINE-SURFACE FAULT and not a
missing sample — the `-`-iff-no-sample convention is a biconditional that
`tests/sigma_candidates_reachability.rs` asserts. A path entry matching the
`p<id>:` prefix but not the four-field group is counted as `W7` breakage and
reported, never silently skipped.

**THE `[DIAG]` BLOCK INDEX IS THE POOLING UNIT.** The bar's `R_total` is
`p95/p05` over the POOLED readings of ALL reps at one cell, and a "reading" is
one gauge emission — one path, one block. `blk` numbers the blocks within an
invocation so a reader can tell a long rep from many short ones, and so the
report can state how many independent emissions each quantile rests on.
"""
import json
import os
import re
import sys

import sigb_probe

#: The whole per-path group, in ONE regex, in the diag.rs emission order. The
#: four gauges are `<us>|-` followed by `/n<count>` — the `sig_us` convention,
#: which all four share by construction (`net/diag.rs`'s `cand` closure).
PATH_GROUP = re.compile(
    r"p(?P<id>\d+):infl=.*?"
    r"sig_us=(?P<sig>-|\d+)/n(?P<sig_n>\d+)\s+"
    r"rvar_us=(?P<rvar>-|\d+)/n(?P<rvar_n>\d+)\s+"
    r"qsp_us=(?P<qsp>-|\d+)/n(?P<qsp_n>\d+)\s+"
    r"msd_us=(?P<msd>-|\d+)/n(?P<msd_n>\d+)"
)
#: Every `p<id>:infl=` entry, whether or not it carries the group above. The
#: DIFFERENCE between the two counts is `W7`.
PATH_ANY = re.compile(r"p(\d+):infl=")
DIAG = re.compile(r"\[DIAG\] t=")
ANSI = re.compile(r"\x1b\[[0-9;]*m")

SECONDS = re.compile(r'"seconds":([0-9.]+)')
MBPS = re.compile(r'"mean_mbps":([0-9.]+)')
RACK = re.compile(r"\[RACK\].*?fa=(\d+)/(\d+)")
RETX = re.compile(r"retx=(\d+)")
RFA_GEN = re.compile(r"\[RFA\] gen=([01])")

GAUGES = ("sig", "rvar", "qsp", "msd")

#: THE GENERATION PLATEAU, transcribed from goal-gate "THE 31 Mbit/s ANOMALY —
#: THE SCORED RESULT" and "THE PASSIVE PRIMITIVES — PLAIN WINDOW" §1: 26.8-34.1
#: Mbit/s at EVERY cell with generation on, against 83-203 with it off. A
#: reading inside this interval is the anomaly's own signature and the battery
#: treats it as a configuration ABORT, not a slow rep.
PLATEAU_LO, PLATEAU_HI = 26.8, 34.1


def lines(path):
    try:
        with open(path, "r", errors="replace") as fh:
            for ln in fh:
                yield ANSI.sub("", ln)
    except OSError:
        return


def scan_diag(path, site, cell, seed, rep):
    """Every gauge emission, in order, one row per path per block."""
    rows, blk, w7 = [], 0, 0
    for ln in lines(path):
        if not DIAG.search(ln):
            continue
        blk += 1
        got = 0
        for m in PATH_GROUP.finditer(ln):
            got += 1
            g = m.groupdict()
            rows.append("SIGBREAD %s %s %s %s p%s blk=%d "
                        "sig=%s/%s rvar=%s/%s qsp=%s/%s msd=%s/%s"
                        % (cell, seed, rep, site, g["id"], blk,
                           g["sig"], g["sig_n"], g["rvar"], g["rvar_n"],
                           g["qsp"], g["qsp_n"], g["msd"], g["msd_n"]))
        w7 += max(0, len(PATH_ANY.findall(ln)) - got)
    return rows, blk, w7


def last(rx, path, cast=float):
    v = None
    for ln in lines(path):
        m = rx.search(ln)
        if m:
            v = cast(m.group(1))
    return v


def maxi(rx, path):
    v = 0
    for ln in lines(path):
        for m in rx.finditer(ln):
            v = max(v, int(m.group(1)))
    return v


def scan_rack(path):
    """`fa=<spur>/<fired>` by MAX denominator — `prim_measure.py`'s rule."""
    best = None
    for ln in lines(path):
        m = RACK.search(ln)
        if m:
            spur, fired = int(m.group(1)), int(m.group(2))
            if best is None or fired > best[1]:
                best = (spur, fired)
    return best


def transfer_seconds(path):
    """The TRANSFER wall, as the MAXIMUM `"seconds"` in the client summary.

    `perf` prints a warm-up summary and a transfer summary; the warm-up's
    `seconds` is an order of magnitude smaller. The maximum is the transfer's
    and it is the denominator the discipline-16 headroom rule names — never the
    invocation wall, which includes namespace bring-up and teardown and runs
    1.12-2.11x the transfer (goal-gate `alpha_calib.sh` header).
    """
    v = 0.0
    for ln in lines(path):
        for m in SECONDS.finditer(ln):
            v = max(v, float(m.group(1)))
    return v or None


#: `tc -s qdisc show`'s per-device counter line. MEASUREMENT DISCIPLINE 16's
#: headroom input, TRANSCRIBED from `alpha_parse.py`'s block (itself
#: `ccand_parse.py`'s verbatim) so the utilisation number pools with those
#: ledgers instead of resembling them.
QSENT = re.compile(r"Sent (\d+) bytes (\d+) pkts? \(dropped (\d+)")


def scan_qdisc(path):
    """Shaped-device bytes, summed over the CLI legs, FIRST capture per leg.

    `INVOCATION_S` is carried ONLY so the correction is auditable: the headroom
    denominator is the TRANSFER wall, never the invocation wall, which runs
    1.12-2.11x the transfer and read `c7` at 77.6 % when the cell was at 96.9 %.
    """
    tc = {"tc_bytes": None, "tc_pkts": None, "tc_drop": None, "tc_s": None}
    if not path or path == "-" or not os.path.exists(path):
        return tc
    cur, secs_q, seen = None, None, {}
    for ln in lines(path):
        if ln.startswith("== "):
            if ln.startswith("== CLI0"):
                cur = "cli0"
            elif ln.startswith("== CLI1"):
                cur = "cli1"
            elif ln.startswith("== INVOCATION_S"):
                cur = None
                m = re.search(r"INVOCATION_S (\d+)", ln)
                secs_q = int(m.group(1)) if m else None
            else:
                cur = None
            continue
        m = QSENT.search(ln) if cur else None
        if m and cur not in seen:
            seen[cur] = tuple(int(x) for x in m.groups())
    if seen:
        tc = {"tc_bytes": sum(v[0] for v in seen.values()),
              "tc_pkts": sum(v[1] for v in seen.values()),
              "tc_drop": sum(v[2] for v in seen.values()),
              "tc_s": secs_q}
    return tc


def main(argv):
    if len(argv) < 6:
        print(__doc__.splitlines()[2].strip(), file=sys.stderr)
        return 2
    cell, seed, rep, cli, srv = argv[0], argv[1], argv[2], argv[3], argv[4]
    qpath = argv[5]
    pings = argv[6:]
    out = []

    rows_c, blk_c, w7_c = scan_diag(cli, "cli", cell, seed, rep)
    rows_s, blk_s, w7_s = scan_diag(srv, "srv", cell, seed, rep)
    out.extend(rows_c)
    out.extend(rows_s)

    secs = transfer_seconds(cli)
    mbps = last(MBPS, cli)
    meta = {"seconds": secs, "mbps": mbps,
            "diag_blocks_cli": blk_c, "diag_blocks_srv": blk_s,
            "reads_cli": len(rows_c), "reads_srv": len(rows_s)}
    meta.update(scan_qdisc(qpath))
    out.append("SIGBMETA %s %s %s %s" % (cell, seed, rep, json.dumps(meta)))

    for i, p in enumerate(pings):
        if not os.path.exists(p):
            out.append("SIGBPROBE %s %s %s %s"
                       % (cell, seed, rep,
                          json.dumps({"leg": i, "file": p, "missing": True})))
            continue
        s = sigb_probe.probe_functionals(p, leg=i)
        out.append("SIGBPROBE %s %s %s %s" % (cell, seed, rep, json.dumps(s)))

    rack = scan_rack(cli)
    gen = last(RFA_GEN, srv, int)
    pfrac = sum(1 for ln in lines(cli) if "[PFRAC]" in ln)
    plateau = (mbps is not None and PLATEAU_LO <= mbps <= PLATEAU_HI)
    out.append("SIGBWITNESS " + json.dumps({
        "cell": cell, "seed": int(seed), "rep": int(rep),
        "mbps": mbps, "seconds": secs,
        # W1/W2 — the generation-off witnesses that are SOUND (primitives-pw
        # §2 retired W3 and repaired W4). W3 is NOT cited here.
        "W1_rfa_gen": gen,
        "W2_pfrac_lines": pfrac,
        # W4' — the MAXIMUM over all [DIAG] lines. `retx=` in the DIAG tail is
        # an INTERVAL counter and reading it off the last line reported this
        # witness failing at 5 of 15 clean reps (primitives-pw §2).
        "W4_retx_max": maxi(RETX, cli),
        "W5_rack_fa": ("%d/%d" % rack if rack else None),
        # W7 — THIS BATTERY'S OWN. The count of `p<id>:` entries that did NOT
        # carry all four gauge tokens. The unit under test is the estimator, so
        # a block missing a candidate's field is the measurement failing, not a
        # column being absent.
        "W7_group_misses_cli": w7_c,
        "W7_group_misses_srv": w7_s,
        "diag_blocks_cli": blk_c, "diag_blocks_srv": blk_s,
        # The configuration witness with teeth: a reading inside the generation
        # plateau means generation leaked in and the arm is ABORTED.
        "gen_plateau": plateau,
        "plateau_band": [PLATEAU_LO, PLATEAU_HI],
    }))

    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
