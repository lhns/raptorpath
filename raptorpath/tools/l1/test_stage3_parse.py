#!/usr/bin/env python3
"""Offline exercise of `stage3_parse.py` (docs/status.md §5) on SYNTHETIC
lines and rows.

    python3 test_stage3_parse.py

NO ENGINE, NO VM. Row lines are transcribed from their format strings
(`[PIPE]` net/mod.rs; `[GATES]` gates.rs; the cadence echo
control/estimator.rs; `[DIAG]` net/diag.rs; `[TRUTH]` lib.sh truth_line;
the perf JSON perf.rs). Scoring is exercised on hand-built ledgers whose
verdicts are known in advance: an absolute check of every rule constant,
not an ordinal one.
"""
import io
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import stage3_parse as sp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-09-30T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::net: "


def gates(pa=0):
    return TS + f"[GATES] RWM_UNIFIED=1 RWM_POOL_ANCHOR={pa} RWM_ACK_MERGE=1 RWM_GEN=384"


WINL = TS + "reliable window mode (retain until acked): auto-selecting RLC windowed backend"
CADL = TS + ("estimator heavy-math cadence ACTIVE (RWM_EST_CADENCE: BOCD update at 10 ms/"
             "loss-event cadence, accumulated counts)")


def pipe(p, b, h):
    return TS + f"[PIPE] pipeline={p} backend={b} hint={h}"


def diag(busy, paths, cum=(1000, 10, 900)):
    pp = "".join(f" p{i}:infl=0/sinfl=0/bdp1(cap0) sout=1 est=Y pl=0.0000 qlp=1/2 | ANCHOR sent=0 "
                 f"gen=0 fill=1 plu={u:.4f} plc={c:.4f}" for i, (u, c) in enumerate(paths))
    return (f"[DIAG] t=1.0s win=1/2 paused=0% good=1.0Mbit cum={cum[0]}/{cum[1]}/{cum[2]} cwnd=1 "
            f"wait[tun=0% paused=0% n=10 us=100 busy={busy}% busy_us=5]{pp} rce=1")


def truth(i, loss):
    return (f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams=1000 egress_skbs=250 gso=4.00 "
            f"netem_sent_dgrams=975 netem_dropped_skbs=6 backlog=0 lost=25 loss={loss} "
            f"rcvbuf_drops=0 rcvbuf_scope=netns")


def drv(p, h, legs, cpu=2.5):
    return ([f"--- RWM-C perf pipeline={p} mode=single hint={h} A=c2 B=c2 ooo=0 extra='' T=default (1 x 1) start=10:00:00",
             f"    CPU: CPUSRV=1.00s CPUCLI={cpu:.2f}s (srv=decoder cli=sender; whole-invocation incl warmup)"]
            + [truth(i, 0.025) for i in range(legs)])


def acked(mbps, secs):
    return json.dumps({"proto": "rp-native", "hint": "bulk", "bytes": 1, "run": 1,
                       "seconds": secs, "mbps": mbps})


SUMMARY = json.dumps({"summary": True, "dnf": 0})


def logs(arm, cell, cad_echo=None, pa=0, winline=None):
    p, b, h, cad = sp.ARM_SPEC[arm]
    legs = sp.N_LEGS[cell]
    cad_echo = cad if cad_echo is None else cad_echo
    winline = (p == "window") if winline is None else winline
    common = [gates(pa), pipe(p, b, h)] + ([WINL] if winline else []) + ([CADL] if cad_echo else [])
    cli = common + [diag(20, [(0.0354, 0.0250)] * legs), diag(40, [(0.0354, 0.0260)] * legs),
                    acked(88.0, 9.09), SUMMARY]
    srv = list(common)
    return drv(p, h, legs), cli, srv


# ── row: statuses and gauges ─────────────────────────────────────────────
d, c, s = logs("A1", "c8-100")
r = sp.make_row("c8-100", "A1", 42, 1, 0, 12, d, c, s)
check(r["status"] == "LIVE", f"A1 live: {r['status']} {r['problems']}")
check(r["busy_med"] == 30.0 and r["busy_last"] == 40.0, f"busy med/last {r['busy_med']} {r['busy_last']}")
check(r["plc_p0"] == 0.026 and r["plc_p1"] == 0.026, "plc from the LAST diag line, per path")
check(abs(r["plu_med_p1"] - 0.0354) < 1e-12, "plu median")
check(abs(r["coded_share"] - 10 / 1010) < 1e-12, "coded share = cod/(src+cod)")
check(abs(r["feed_ratio_p0"] - 0.026 / 0.025) < 1e-9, "feed ratio = plc/truth")
check(r["cpu_cli"] == 2.5 and abs(r["util"] - 2.5 / 9.09) < 1e-12, "cpu and util")
d, c, s = logs("CAD", "c2-100")
check(sp.make_row("c2-100", "CAD", 42, 1, 0, 12, d, c, s)["status"] == "LIVE", "CAD live with echo both ends")
d, c, s = logs("CAD", "c2-100", cad_echo=False)
check(sp.make_row("c2-100", "CAD", 42, 1, 0, 12, d, c, s)["status"] == "WITNESS-FAIL", "CAD without echo fails")
d, c, s = logs("A2", "c2-100", cad_echo=True)
check(sp.make_row("c2-100", "A2", 42, 1, 0, 12, d, c, s)["status"] == "WITNESS-FAIL", "echo on CTL fails")
d, c, s = logs("CAD", "c2-100", pa=1)
check(sp.make_row("c2-100", "CAD", 42, 1, 0, 12, d, c, s)["status"] == "WITNESS-FAIL", "pool anchor 1 fails")
d, c, s = logs("BLKa", "c3-25")
rb = sp.make_row("c3-25", "BLKa", 42, 1, 0, 30, d, c, s)
check(rb["status"] == "LIVE", f"BLKa live {rb['problems']}")
d, c, s = logs("BLKa", "c3-25", winline=True)
check(sp.make_row("c3-25", "BLKa", 42, 1, 0, 30, d, c, s)["status"] == "WITNESS-FAIL", "window line on block")
d, c, s = logs("WINa", "c3-25")
check(sp.make_row("c3-25", "BLKa", 42, 1, 0, 30, d, c, s)["status"] == "CONTAMINATED", "wrong pipe")
check(sp.make_row("c3-25", "A1", 42, 1, 3, 30, d, c, s)["status"] == "VOID-RC", "rc void")


# ── score: hand-built ledgers ────────────────────────────────────────────
def mk(cell, arm, seed, rep, mbps, cpu=5.0, busy=30.0, dnf=False, plc=0.025, tr=0.025):
    legs = sp.N_LEGS[cell]
    row = {"cell": cell, "arm": arm, "seed": str(seed), "rep": rep, "rc": 0, "status": "LIVE",
           "problems": [], "dnf": dnf, "mbps": None if dnf else mbps,
           "seconds": None if dnf else 100.0 / mbps, "cpu_cli": cpu, "busy_med": busy,
           "util": None if dnf else cpu / (100.0 / mbps), "wall_s": 10}
    for i in range(legs):
        row["plc_p%d" % i] = plc
        row["truth_loss_p%d" % i] = tr
        row["feed_ratio_p%d" % i] = plc / tr
        row["plu_med_p%d" % i] = 0.0354
    row["coded_share"] = 0.001
    return row


def ledger(rows, extra=""):
    f = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8")
    f.write(extra)
    for r in rows:
        f.write("S3ROW " + json.dumps(r, sort_keys=True) + "\n")
    f.close()
    return f.name


def base(gp=None, overrides=None):
    """Every planned (cell, arm) at 3 reps x 2 seeds, A1 = 100,101,102 and
    A2 = 101,102,103 per seed; every other arm by `gp` (default 101.5)."""
    rows = []
    gp = gp or {}
    for cell in sp.CELLS:
        for arm in sp.PLAN[cell]:
            for seed in (42, 7):
                for rep in (1, 2, 3):
                    if arm == "A1":
                        v = 99.0 + rep
                    elif arm == "A2":
                        v = 100.0 + rep
                    else:
                        v = gp.get((cell, arm), 101.5)
                    rows.append(mk(cell, arm, seed, rep, v))
    for fn in (overrides or []):
        rows = [fn(r) for r in rows]
    return rows


def run(rows, extra=""):
    buf = io.StringIO()
    bv, dv = sp.score([ledger(rows, extra)], out=lambda s: buf.write(s + "\n"))
    return bv, dv, buf.getvalue()


# the MDE formula: A1 = {100,101,102}x2, A2 = {101,102,103}x2 -> |dmed| = 1,
# half-range = 1.5 -> MDE = max(2, 1.5) = 2; med(A1uA2) = 101.5 -> rel 1.97 %.
bv, dv, txt = run(base())
line = [ln for ln in txt.splitlines() if ln.startswith("  MDE c2-100 gp ")][0]
check(" 2.000 " in line and "2.0%" in line and "MDE-COMMITTED" in line, f"MDE formula: {line}")
check(bv == "WINDOW-NOT-WORSE", f"all-equal battery -> WINDOW-NOT-WORSE, got {bv}")
check(dv == "INERT-AS-DERIVED", f"all-equal CAD -> INERT, got {dv}")

# BLK beyond MDE at c3 bulk: BLK 104 vs WIN med 101.5, band 101.5 >= 104*(1-0.0197)=101.95 -> fail
bv, _, _ = run(base({("c3-25", "BLKb"): 104.0}))
check(bv == "BLOCK-BETTER-AT-c3-25/bulk", f"block better at c3 bulk: {bv}")
# inside MDE: BLK 103 -> 103*(0.9803)=100.97 <= 101.5 -> not worse
bv, _, _ = run(base({("c3-25", "BLKb"): 103.0}))
check(bv == "WINDOW-NOT-WORSE", f"within MDE: {bv}")

# CAD better at c1d by more than MDE -> FLIP; worse at c8 -> WORSE-AT
_, dv, _ = run(base({("c1d-400", "CAD"): 110.0}))
check(dv.startswith("FLIP-RECOMMENDED") and "c1d-400:gp" in dv, f"flip: {dv}")
_, dv, _ = run(base({("c1d-400", "CAD"): 110.0, ("c8-100", "CAD"): 95.0}))
check(dv == "WORSE-AT-c8-100", f"worse wins over better: {dv}")


# the feed clause: CAD plc/truth 1.4 vs CTL 1.0 -> FEED-MOVED
def feed(r):
    if r["arm"] == "CAD" and r["cell"] == "c2-100":
        r["plc_p0"] = 0.035
        r["feed_ratio_p0"] = 0.035 / 0.025
    return r


_, dv, _ = run(base(overrides=[feed]))
check(dv == "FEED-MOVED-AT-c2-100:p0", f"feed moved: {dv}")


# CPU lower beyond MDE is 'better'; CTL cpu 5.0 constant -> MDE 0 -> rel 0 ->
# any change moves; make the A/A cpu spread explicit
def cpu_spread(r):
    if r["arm"] in ("A1", "A2"):
        r["cpu_cli"] = 5.0 + 0.1 * r["rep"]
    if r["arm"] == "CAD":
        r["cpu_cli"] = 4.0
    return r


_, dv, _ = run(base(overrides=[cpu_spread]))
check(dv.startswith("FLIP-RECOMMENDED") and "cpu" in dv, f"cpu better: {dv}")


# NOISE-BOUND: A/A range ~60 % of median at c8 -> c8 gp unscoreable in (d)
def noisy(r):
    if r["cell"] == "c8-100" and r["arm"] == "A1" and r["rep"] == 1:
        r["mbps"] = 40.0
        r["seconds"] = 100.0 / 40.0
    return r


bv, dv, txt = run(base(overrides=[noisy]))
check("NOISE-BOUND" in [ln for ln in txt.splitlines() if ln.startswith("  MDE c8-100 gp ")][0],
      "c8 gp noise-bound")
check(dv.startswith("UNSCOREABLE") and "c8-100" in dv, f"(d) unscoreable at noise-bound cell: {dv}")
check(bv.startswith("UNSCOREABLE") and "c8-100" in bv, f"(b) unscoreable at noise-bound cell: {bv}")


# DNF clause: WINa DNF 3 of 6 at c7 vs BLKa 0 -> 0.5 > 0.2 -> fails
def dnfs(r):
    if r["cell"] == "c7-100" and r["arm"] == "WINa" and r["rep"] <= 2 and r["seed"] == "42":
        r["dnf"] = True
        r["mbps"] = r["seconds"] = None
    return r


bv, _, _ = run(base(overrides=[dnfs]))
check(bv == "BLOCK-BETTER-AT-c7-100/auto", f"dnf clause: {bv}")

# min live: drop BLKa rows at c2 to 2 -> UNSCOREABLE there
rows = [r for r in base() if not (r["cell"] == "c2-100" and r["arm"] == "BLKa" and r["rep"] > 1)]
bv, _, _ = run(rows)
check(bv.startswith("UNSCOREABLE") and "c2-100/auto" in bv, f"min live: {bv}")

# an abort token makes both verdicts UNSCOREABLE
bv, dv, _ = run(base(), extra="ABORT-SHA x\n")
check(bv.startswith("UNSCOREABLE") and dv.startswith("UNSCOREABLE"), f"abort: {bv} / {dv}")

# the Auto-on-block c3 finding: BLKa c3 at 7.0 -> AS-RECORDED; at 5.0 -> REGRESSED
_, _, txt = run(base({("c3-25", "BLKa"): 7.0, ("c3-25", "WINa"): 7.0}))
check("AUTO-BLOCK-C3 AUTO-BLOCK-C3-AS-RECORDED" in txt, "auto block c3 as recorded")
_, _, txt = run(base({("c3-25", "BLKa"): 5.0, ("c3-25", "WINa"): 5.0}))
check("AUTO-BLOCK-C3 AUTO-BLOCK-C3-REGRESSED" in txt, "auto block c3 regressed")

print(f"{CHECKS - len(FAILS)}/{CHECKS} checks passed")
sys.exit(1 if FAILS else 0)
