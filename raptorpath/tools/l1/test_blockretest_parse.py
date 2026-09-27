#!/usr/bin/env python3
"""Offline exercise of `blockretest_parse.py` on SYNTHETIC lines.

    python3 test_blockretest_parse.py

NO ENGINE, NO VM. The lines are transcribed from their format strings:
  * `[PIPE]`          net/mod.rs `pipe_echo_line`
  * the perf JSON     perf.rs `client` (acked carries `mbps`; a DNF carries
                      `dnf: true` and no `mbps`; the summary `summary: true`)
  * the driver header perf_rwm_c.sh `--- RWM-C perf pipeline=...`
  * the crown rep     tail_matrix.sh `run_arm` rep line
"""
import io
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blockretest_parse as bp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-09-27T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::net: "
GATES = TS + "[GATES] RWM_UNIFIED=1 RWM_GEN=384 RWM_PIPELINE=0"
WINL = TS + "reliable window mode (retain until acked): auto-selecting RLC windowed backend"


def pipe(p, b, h="bulk"):
    return TS + f"[PIPE] pipeline={p} backend={b} hint={h}"


def acked(mbps, secs, n=100000000):
    return json.dumps({"proto": "rp-native", "hint": "bulk", "bytes": n, "run": 1,
                       "seconds": secs, "mbps": mbps})


def dnf_line():
    return json.dumps({"proto": "rp-native", "hint": "bulk", "bytes": 1, "run": 1,
                       "dnf": True, "timeout_s": 150})


SUMMARY = json.dumps({"summary": True, "proto": "rp-native", "hint": "bulk",
                      "bytes": 1, "runs": 1, "dnf": 0})


def hdr(p):
    return f"--- RWM-C perf pipeline={p} mode=single hint=bulk A=c2 B=c2 ooo=0 extra='' T=default (1 x 1) start=10:00:00"


def logs(arm, run_line=None, cli_pipe=None, srv_pipe=None, header=None, gates=True,
         summary=True):
    p, b = bp.ARM_PIPE[arm]
    cli = [GATES if gates else "x", cli_pipe or pipe(p, b)]
    srv = [GATES if gates else "x", srv_pipe or pipe(p, b)]
    if arm == "WIN":
        cli.append(WINL)
        srv.append(WINL)
    cli.append(json.dumps({"warmup": True, "seconds": 0.1}))
    if run_line:
        cli.append(run_line)
    if summary:
        cli.append(SUMMARY)
    drv = [header or hdr(p)]
    return [bp.lc.split_interleaved(x)[0] for x in drv], \
        [s for x in cli for s in bp.lc.split_interleaved(x)], \
        [s for x in srv for s in bp.lc.split_interleaved(x)]


def row(arm, rc=0, **kw):
    drv, cli, srv = logs(arm, **kw)
    return bp.make_row("c2", arm, "bulk", "42", 1, rc, drv, cli, srv)


# ── ROW STATUS ───────────────────────────────────────────────────────────
r = row("BLK", run_line=acked(95.0, 8.4))
check(r["status"] == "LIVE" and r["mbps"] == 95.0 and r["dnf"] is False, f"BLK live {r}")
check(r["pipe_cli"] == ["block", "RaptorQ", "bulk"], f"pipe echo parsed through colour {r['pipe_cli']}")
r = row("WIN", run_line=acked(90.0, 8.9))
check(r["status"] == "LIVE" and r["seconds"] == 8.9, f"WIN live {r}")
r = row("WIN", run_line=dnf_line())
check(r["status"] == "LIVE" and r["dnf"] is True and r["mbps"] is None, f"DNF is a live datum {r}")
r = row("BLK", rc=3, run_line=acked(95.0, 8.4))
check(r["status"] == "VOID-RC", f"rc!=0 voids {r['status']}")
drv, cli, srv = logs("BLK", run_line=acked(95.0, 8.4))
r = bp.make_row("c2", "BLK", "bulk", "42", 1, 0, drv, cli, srv, cotenant=1)
check(r["status"] == "VOID-COTENANT", f"co-tenant voids {r['status']}")
r = row("BLK", run_line=None, summary=False)
check(r["status"] == "NO_DATA", f"no summary = NO_DATA {r['status']}")
r = row("BLK", run_line=acked(95.0, 8.4), srv_pipe=pipe("window", "Rlc"))
check(r["status"] == "CONTAMINATED", f"server echo disagrees -> CONTAMINATED {r}")
r = row("WIN", run_line=acked(95.0, 8.4), cli_pipe=pipe("block", "RaptorQ"))
check(r["status"] == "CONTAMINATED", f"client echo disagrees -> CONTAMINATED {r['status']}")
r = row("WIN", run_line=acked(95.0, 8.4), cli_pipe=pipe("window", "Rlc", "auto"))
check(r["status"] == "CONTAMINATED", f"hint disagrees -> CONTAMINATED {r['status']}")
r = row("BLK", run_line=acked(95.0, 8.4), header=hdr("window"))
check(r["status"] == "CONTAMINATED", f"header disagrees -> CONTAMINATED {r['status']}")
r = row("BLK", run_line=acked(95.0, 8.4), gates=False)
check(r["status"] == "WITNESS-FAIL", f"no [GATES] -> WITNESS-FAIL {r['status']}")
drv, cli, srv = logs("BLK", run_line=acked(95.0, 8.4))
cli.append(WINL)
r = bp.make_row("c2", "BLK", "bulk", "42", 1, 0, drv, cli, srv)
check(r["status"] == "WITNESS-FAIL" and "window-line-on-BLK" in r["problems"], f"RLC line on BLK {r}")


# ── SCORING ──────────────────────────────────────────────────────────────
def synth(blk, win, cells=bp.CELLS, hints=bp.HINTS, seeds=bp.SEEDS):
    """blk/win: callables (cell, hint, seed, rep) -> (mbps|None for DNF, secs)."""
    out = []
    for s in seeds:
        for rep in (1, 2, 3):
            for c in cells:
                for h in hints:
                    for arm, fn in (("BLK", blk), ("WIN", win)):
                        m, t = fn(c, h, s, rep)
                        out.append({"cell": c, "arm": arm, "hint": h, "seed": s, "rep": rep,
                                    "rc": 0, "status": "LIVE", "dnf": m is None,
                                    "mbps": m, "seconds": t, "problems": []})
    return out


def run_score(rows, extra=""):
    with tempfile.NamedTemporaryFile("w", delete=False, suffix=".log") as f:
        f.write(extra)
        for x in rows:
            f.write("BRROW " + json.dumps(x) + "\n")
        path = f.name
    buf = io.StringIO()
    v = bp.score([path], out=lambda s: buf.write(s + "\n"))
    os.unlink(path)
    return v, buf.getvalue()


same = lambda c, h, s, rep: (90.0 + rep, 10.0 - rep * 0.1)
v, txt = run_score(synth(same, same))
check(v == "WINDOW-NOT-WORSE", f"identical arms -> WINDOW-NOT-WORSE, got {v}\n{txt}")

# WIN median exactly at BLK min passes (>=); one below fails at that cell only.
blk = lambda c, h, s, rep: (90.0 + rep, 10.0)            # min 91
win_at = lambda c, h, s, rep: (91.0, 10.0)               # median 91 = BLK min
v, _ = run_score(synth(blk, win_at))
check(v == "WINDOW-NOT-WORSE", f"median == BLK min passes, got {v}")
win_lo = lambda c, h, s, rep: (90.9 if c == "c2" else 91.0, 10.0)
v, txt = run_score(synth(blk, win_lo))
check(v == "BLOCK-BETTER-AT-c2", f"goodput below BLK min at c2 -> {v}\n{txt}")

# Completion: WIN p50 above BLK max fails.
win_slow = lambda c, h, s, rep: (95.0, 12.0 if (c, h) == ("c3", "auto") else 10.0)
v, _ = run_score(synth(blk, win_slow))
check(v == "BLOCK-BETTER-AT-c3", f"slow completion on c3/auto -> {v}")

# Per-seed failure alone suffices (pooled would pass).
win_s7 = lambda c, h, s, rep: (80.0 if (c, s) == ("c7", "7") else 95.0, 10.0)
v, txt = run_score(synth(blk, win_s7))
check(v == "BLOCK-BETTER-AT-c7", f"seed-7-only failure -> {v}\n{txt}")

# DNF: WIN DNF > BLK DNF fails; BLK all-DNF makes the other clauses vacuous.
win_dnf = lambda c, h, s, rep: (None if (c, rep) == ("c2", 1) else 95.0, 10.0)
v, _ = run_score(synth(blk, win_dnf))
check(v == "BLOCK-BETTER-AT-c2", f"extra WIN DNF at c2 -> {v}")
blk_dnf = lambda c, h, s, rep: (None if c == "c1" else 95.0, 10.0)
win_dnf1 = lambda c, h, s, rep: (None if (c, rep) == ("c1", 1) else 50.0 if c == "c1" else 95.0, 10.0)
v, txt = run_score(synth(blk_dnf, win_dnf1))
check(v == "WINDOW-NOT-WORSE", f"BLK completed nothing at c1: DNF clause decides -> {v}\n{txt}")
win_none = lambda c, h, s, rep: (None if c == "c3" else 95.0, 10.0)
blk_some = lambda c, h, s, rep: (None if (c, rep) != ("c3", 1) and c == "c3" else 95.0, 10.0)
v, _ = run_score(synth(blk_some, win_none))
check(v == "BLOCK-BETTER-AT-c3", f"WIN completed none where BLK completed some -> {v}")

# UNSCOREABLE: a missing seed; a first-five abort token; witness failures.
v, txt = run_score(synth(same, same, seeds=["42"]))
check(v == "UNSCOREABLE" and "s7 live=0" in txt, f"seed 7 absent -> UNSCOREABLE {v}")
v, _ = run_score(synth(same, same), extra="ABORT-SHA binary changed\n")
check(v == "UNSCOREABLE", f"ABORT-SHA -> UNSCOREABLE {v}")
rows = synth(same, same)
for x in rows:
    if x["arm"] == "WIN" and x["cell"] == "c8" and x["rep"] == 1 and x["hint"] == "bulk":
        x["status"] = "CONTAMINATED"
v, txt = run_score(rows)
check(v == "UNSCOREABLE" and "WIN-c8" in txt, f"2 contaminated reps of WIN-c8 -> {v}")
rows = synth(same, same)
for x in rows:
    if x["arm"] == "BLK" and x["cell"] == "c1" and x["rep"] == 1 and x["hint"] == "bulk" and x["seed"] == "7":
        x["status"] = "NO_DATA"
v, _ = run_score(rows)
check(v == "WINDOW-NOT-WORSE", f"one NO_DATA leaves 2 live reps -> scoreable, got {v}")

# ── CROWN ────────────────────────────────────────────────────────────────
def crown_ledger(seed, p99=40.0, p50_c2=8.0, p50_c3=24.0, cnt=1000, nreps=8, p99_c3=120.0):
    lines = []
    for cell in ("c2", "c3"):
        lines.append(f"=== CROWNSPOT stage seed={seed} cell={cell} start=2026-09-27T10:00:00Z")
        for size in (400, 1200):
            for rp in range(1, nreps + 1):
                p = p99 if cell == "c2" else p99_c3
                p5 = p50_c2 if cell == "c2" else p50_c3
                lines.append(f"  ship {size}B rep{rp}: p50={p5}ms p99={p}ms p999=60ms max=70ms n={cnt}")
        lines.append("=== done 10:10:00")
    return "\n".join(lines) + "\n"


def run_crown(a, b):
    ps = []
    for t in (a, b):
        f = tempfile.NamedTemporaryFile("w", delete=False, suffix=".log")
        f.write(t)
        f.close()
        ps.append(f.name)
    buf = io.StringIO()
    v = bp.crown(ps, out=lambda s: buf.write(s + "\n"))
    for p in ps:
        os.unlink(p)
    return v, buf.getvalue()


v, txt = run_crown(crown_ledger("42"), crown_ledger("7"))
check(v == "REPAIRS-INERT-ON-CROWN", f"crown inside bands -> {v}\n{txt}")
v, txt = run_crown(crown_ledger("42"), crown_ledger("7", p99=60.0))
check(v == "CROWN-MOVED" and "CROWN-MOVED(c2, 7, p99@400B, up)" in txt, f"c2 s7 p99 60 > 56 -> {v}")
v, txt = run_crown(crown_ledger("42", p50_c2=6.5), crown_ledger("7"))
check(v == "CROWN-MOVED" and "p50@400B, down" in txt, f"improvement counts as moved -> {v}")
v, _ = run_crown(crown_ledger("42", nreps=5), crown_ledger("7"))
check(v == "SPOT-UNSCOREABLE", f"5 of 8 reps -> SPOT-UNSCOREABLE {v}")
v, _ = run_crown(crown_ledger("42", cnt=994), crown_ledger("7"))
check(v == "CROWN-MOVED", f"count below 995 -> moved {v}")

print(f"{CHECKS - len(FAILS)}/{CHECKS} checks passed")
sys.exit(1 if FAILS else 0)
