#!/usr/bin/env python3
"""Which witnesses can make a plan-41c guard row (BASE or CAND, default store) non-LIVE, and how often each fired in the committed ledgers.

    void_rate.py [ledger ...]      default: docs/l1-raw/place38, place41, place41b (ld.log)

Order of the status decision in stage3_parse.make_row (first match wins): rc != 0 -> VOID-RC; a cotenant (cargo / rustc running) -> VOID-COTENANT;
no summary JSON -> NO_DATA; pipe echo / header contamination -> CONTAMINATED; any other witness problem -> WITNESS-FAIL; place38_parse.make_row then
adds its own witnesses (WITNESS-FAIL) and VOID-ONDISK. DNF is NOT a void: perf prints `dnf` in the run JSON and exits 0, the row is LIVE with dnf true
(harness 43's COLLAPSE). A crash is rc != 0 (VOID-RC) or no data (NO_DATA).
"""
import collections
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import place38_parse as pp  # noqa: E402

RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")

WITNESSES = [
    # (status, witness token, applies to, text)
    ("VOID-RC", "rc != 0", "BASE, CAND", "the driver / client exited non-zero (crash, timeout kill, topology failure): a CRASH, never replaced, worst rank"),
    ("VOID-COTENANT", "cargo / rustc running before or after the row", "BASE, CAND", "a co-tenant build on the box: replaceable"),
    ("NO_DATA", "no summary JSON in the client log", "BASE, CAND", "bring-up produced no result (retried once by the battery): a CRASH, never replaced, worst rank"),
    ("CONTAMINATED", "no-pipe-echo-cli/srv, pipe-echo-cli/srv=..., header=...", "BASE, CAND", "the pipeline / backend / hint echo is not window/Rlc/bulk: replaceable"),
    ("WITNESS-FAIL", "no-gates-cli / no-gates-srv", "BASE, CAND", "no [GATES] line on an endpoint: replaceable"),
    ("WITNESS-FAIL", "window-line-missing-on-window", "BASE, CAND", "the --window-reliable echo is missing: replaceable"),
    ("WITNESS-FAIL", "generation-guard-present", "BASE, CAND", "the generation guard fired: replaceable"),
    ("WITNESS-FAIL", "cadence-ACTIVE-missing / cadence-OFF-on-ACTIVE-arm", "BASE, CAND", "the estimator-cadence echo is not ACTIVE on both endpoints: replaceable"),
    ("WITNESS-FAIL", "pool-anchor=...", "BASE, CAND", "RWM_POOL_ANCHOR present and not 0 (absent = off): replaceable"),
    ("WITNESS-FAIL", "gate-place-wire=... on BASE|CAND (want absent)", "BASE, CAND", "an RWM_PLACE_WIRE token on [GATES]: the binary is not main's / not promoted: replaceable"),
    ("WITNESS-FAIL", "gate-store=... (want unset)", "BASE, CAND", "RWM_STORE not unset at a default cell: replaceable"),
    ("WITNESS-FAIL", "wpl_us-numeric-on-BASE", "BASE only", "main's binary printed a numeric wpl_us: replaceable"),
    ("WITNESS-FAIL", "wire-gauges-on-BASE", "BASE only", "main's [ETA] line carried the wire gauges: replaceable"),
    ("VOID-ONDISK", "binary or run dir not on /dev/shm", "BASE, CAND", "tmpfs rule: every row would fire (so > 4 voids -> NO VERDICT)"),
]


def tag(problem):
    m = re.match(r"[A-Za-z_][A-Za-z0-9_\-/]*", problem)
    return m.group(0) if m else problem


def main(argv):
    paths = argv or [os.path.join(RAW, d, "ld.log") for d in ("place38", "place41", "place41b")]
    print("WITNESSES THAT CAN MAKE A plan-41c ROW NON-LIVE (decision order: rc, cotenant, no summary, contamination, other witnesses; DNF is not a void)")
    for st, w, who, txt in WITNESSES:
        print(f"  {st:<14} {w:<62} [{who}] {txt}")
    print()
    for path in paths:
        rows, _ = pp.rows_of([path])
        name = os.path.basename(os.path.dirname(os.path.abspath(path)))
        st = collections.Counter((r["arm"], r["status"]) for r in rows)
        arms = sorted({r["arm"] for r in rows})
        print(f"== {name}: {len(rows)} rows")
        for a in arms:
            n = sum(v for (aa, _), v in st.items() if aa == a)
            tally = ", ".join(f"{s} {v}" for (aa, s), v in sorted(st.items()) if aa == a)
            dnf = sum(1 for r in rows if r["arm"] == a and r.get("dnf"))
            print(f"   {a:<5} rows {n}: {tally}; DNF rows (LIVE, dnf true) {dnf}")
        pt = collections.Counter()
        for r in rows:
            for p in r.get("problems") or []:
                pt[(r["arm"], tag(p))] += 1
        if pt:
            print("   witness problems fired: " + "; ".join(f"{a} {t} x{v}" for (a, t), v in sorted(pt.items())))
        else:
            print("   witness problems fired: none")
        hi = sum(1 for r in rows if r.get("io_high"))
        print(f"   io full avg10 >= 20 rows (a covariate, never a void): {hi}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
