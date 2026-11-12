#!/usr/bin/env python3
"""Tests for the status 48b harness (plan 48, tools/l1/bbr48_score.py): both [DIAG] shapes, the CAND / BASE row witnesses (a two-leg c8 block
included), R2b on a collapsed row against a trickle leg, the outcome ladder, the plan table, the shell side, and the dry run on the committed
status-47 guard ledger.  Run: python3 test_bbr48_parse.py"""
import io
import json
import os
import random
import re
import sys
import tempfile
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as g41  # noqa: E402
import bbr48_score as b48  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


STOCK = "\x1b[2m2026-11-12T10:00:00Z\x1b[0m INFO quinn congestion controller: BBR (shipped default; RWM_QUIC_CC overrides)"
BRS = "\x1b[2m2026-11-12T10:00:00Z\x1b[0m INFO RWM_QUIC_CC=bbr_rs: burst-robust BBR (in-tree controller, interval-guarded per-flight rate sampler)"
BDP = {"c1s-400": [250000], "c1d-400": [250000, 250000], "c2-100": [125000], "c8-100": [125000, 100000]}


def leg(i, sent, qcwnd, rtt=20, wrtt=3, gauges=True):
    q = f" qcwnd={qcwnd} qce=0 qlp=1/{sent}" if gauges else ""
    return (f" p{i}:infl=199/sinfl=0/bdp356(cap0) sout=409 khr=1.00/kraw=- btlbw=148831 sr=0/g0d0 est=Y pl=0.0000 cmp=- rtt={rtt}/wrtt={wrtt}/rtp2ms rtp_us=2392 "
            f"sig_us=38/n1 rvar_us=34/n1 qsp_us=1/n1 msd_us=4/n1 tlag_us=9/n1 gapd=0/0{q} | ANCHOR sent=0 al=0 attr=0 nr=0 rej[iv=0 zr=0 al=0] gen=0 fill=1 plu=0.0006 plc=0.0013")


def diag(t, legs, dgq=None, gauges=True):
    """legs: [(sent, qcwnd, rtt, wrtt)]; dgq: [(hand, tx)] per leg or None (absent)."""
    d = "".join(f" dgq{i}[hand={h} tx={x} full=0 err=0 sp=4167376]" for i, (h, x) in enumerate(dgq)) if dgq else ""
    return (f"[DIAG] t={t:.1f}s win=838/838 paused=42% good=619.0Mbit cwnd=647 infl=457 np={len(legs)} np_act={len(legs)} rtt=15.8ms bdp100=165sym" + d
            + "".join(leg(i, *l, gauges=gauges) for i, l in enumerate(legs)) + " rce=1/c1 " + "".join(f"dgev{i}=3 " for i in range(len(legs))) + "eb_bursts=1")


def qcc(t, blocks):
    """blocks: [(qm, qrt_us, qsr_us)] per leg."""
    return f"[QCC] t={t:.1f}s" + "".join(f" p{i}:qm={m} qrt_us={rt} qsr_us={sr} qbw=104000000 qmin_us={rt - 100} qcns=900/{10 + t:.0f}/880" for i, (m, rt, sr) in enumerate(blocks))


def run_lines(cell, arm, n=100, qcwnd=None, backlog=None, active=None, qm="B", skip_qcc_leg=None, gauges=True, bad_qm=None):
    """A measured run of n [DIAG] lines at 0.25 s (t = 0.25 .. n/4): per leg qcwnd (default 2 x BDP), B = hand - tx (default 0), active (sent advances) default True."""
    nleg = len(BDP[cell])
    qcwnd = qcwnd or [2 * b for b in BDP[cell]]
    backlog = backlog or [0] * nleg
    active = active or [True] * nleg
    L = ['{"seconds":0.01,"warmup":true}', STOCK if arm == "BASE" else BRS]
    sent = [1000] * nleg
    for k in range(1, n + 1):
        t = 0.25 * k
        legs = []
        for i in range(nleg):
            if active[i]:
                sent[i] += 100
            legs.append((sent[i], qcwnd[i], 20, 3))
        L.append(diag(t, legs, [(5000 + 100 * k, 5000 + 100 * k - backlog[i]) for i in range(nleg)], gauges=gauges))
        if arm == "CAND" and k % 4 == 0:
            blocks = [(bad_qm if (bad_qm and i == 0) else qm, 2000 if i == 0 else 40000, 4000 if i == 0 else 60000) for i in range(nleg) if i != skip_qcc_leg]
            if skip_qcc_leg is None:
                L.append(qcc(t, blocks))
            else:
                L.append(f"[QCC] t={t:.1f}s" + "".join(qcc(t, [b]).split("[QCC] t=%.1fs" % t)[1].replace("p0:", f"p{i}:") for i, b in enumerate(blocks) if i != skip_qcc_leg))
    return L


# ── both [DIAG] shapes ────────────────────────────────────────────────────
ser = b48.diag_series(run_lines("c2-100", "BASE", n=8))
check("[DIAG] with q* gauges: sent / qcwnd / wrtt / hand-tx counters parsed per leg", len(ser) == 8 and ser[0][1][0]["qcwnd"] == 250000 and ser[0][1][0]["sent"] == 1100
      and ser[0][1][0]["wrtt"] == 3 and ser[0][2][0] == (5100, 5100, 0), ser[:1])
old = b48.diag_series(run_lines("c2-100", "BASE", n=8, gauges=False))
check("[DIAG] without q* gauges (the older shape): the leg is still read (rtt, wrtt) with sent / qcwnd None", len(old) == 8 and old[0][1][0]["qcwnd"] is None and old[0][1][0]["sent"] is None and old[0][1][0]["rtt"] == 20)
bo, po = b48.row_readouts(run_lines("c2-100", "BASE", gauges=False), [], "c2-100", "BASE")
check("a row without q* gauges is void (R2 cannot be read), its span is still measured", "no-q-gauges-p0" in po and bo["span_s"] >= 20, (po, bo["span_s"]))
two = b48.diag_series(run_lines("c8-100", "CAND", n=4))
check("[DIAG] on a two-leg cell: both legs and both dgq counters per line", sorted(two[0][1]) == [0, 1] and sorted(two[0][2]) == [0, 1])

# ── witnesses ─────────────────────────────────────────────────────────────
for cell in BDP:
    b, p = b48.row_readouts(run_lines(cell, "CAND"), [], cell, "CAND")
    check(f"CAND {cell}: echo + qm in SDBR on every block + a [QCC] block per leg + >= 20 s of [DIAG] -> no problem", p == [] and b["span_s"] >= 20 and all(q["n"] > 0 for q in b["qcc"]) and len(b["qcc"]) == len(BDP[cell]), (p, b["span_s"]))
b, p = b48.row_readouts(run_lines("c8-100", "CAND"), [], "c8-100", "CAND")
check("CAND two-leg c8 block: leg 0 qm B, margin 2 x 2000/4000 = 1.0, leg 1 margin 2 x 40000/60000 = 1.33, occupancy B = 1",
      b["qcc"][0]["occ"]["B"] == 1.0 and abs(b["qcc"][0]["margin_med"] - 1.0) < 1e-9 and abs(b["qcc"][1]["margin_med"] - 4 / 3) < 1e-9 and b["qcc"][1]["qrt_us_med"] == 40000, b["qcc"])
for qm in "SDBR":
    check(f"CAND qm={qm} is a valid mode", b48.row_readouts(run_lines("c2-100", "CAND", qm=qm), [], "c2-100", "CAND")[1] == [])
_, p = b48.row_readouts(run_lines("c8-100", "CAND", bad_qm="X"), [], "c8-100", "CAND")
check("CAND with qm=X on one path block is void", any(x.startswith("qm-not-SDBR=['X']") for x in p), p)
_, p = b48.row_readouts(run_lines("c8-100", "CAND", skip_qcc_leg=1), [], "c8-100", "CAND")
check("CAND with no [QCC] block for leg 1 at c8 is void", "cand-no-QCC-block-p1" in p, p)
_, p = b48.row_readouts(run_lines("c8-100", "BASE"), [], "c8-100", "CAND")
check("a CAND row with the stock echo and no [QCC] is void on all three witnesses", "cand-no-burst-robust-echo" in p and "cand-has-stock-bbr-echo" in p and "cand-no-QCC-line" in p, p)
b, p = b48.row_readouts(run_lines("c2-100", "BASE"), [], "c2-100", "BASE")
check("BASE: stock echo, no qm=, no [QCC] -> no problem", p == [], p)
_, p = b48.row_readouts(run_lines("c2-100", "CAND"), [], "c2-100", "BASE")
check("a BASE row that carries the burst-robust echo and qm= / [QCC] is void", "base-has-burst-robust-echo" in p and "base-has-qm-or-QCC" in p and "base-no-stock-bbr-echo" in p, p)
_, p = b48.row_readouts(run_lines("c2-100", "BASE") + ["p0:qm=B"], [], "c2-100", "BASE")
check("a stray qm= on BASE's client log is void", "base-has-qm-or-QCC" in p)
_, p = b48.row_readouts(run_lines("c2-100", "BASE"), ["noise p0:qm=S"], "c2-100", "BASE")
check("a stray qm= on BASE's server log is void", "base-has-qm-or-QCC" in p)
_, p = b48.row_readouts(run_lines("c2-100", "CAND", n=70), [], "c2-100", "CAND")
check("17.5 s of post-warm-up [DIAG] (70 lines) is void; 100 lines (24.75 s) is not", any(x.startswith("diag-span=") for x in p) and b48.row_readouts(run_lines("c2-100", "CAND", n=100), [], "c2-100", "CAND")[1] == [])
b, p = b48.row_readouts(run_lines("c2-100", "BASE", n=30), [], "c2-100", "BASE")
check("the span is measured over t >= 1 s only (warm-up second excluded): 30 lines = 6.5 s", abs(b["span_s"] - 6.5) < 1e-9, b["span_s"])
_, p = b48.row_readouts(run_lines("c8-100", "CAND", active=[True, True]), [], "c8-100", "CAND")
check("an active leg whose dgq is absent is void (B unreadable), not a silent zero",
      any(x.startswith("no-dgq-on-") for x in b48.row_readouts([re.sub(r" dgq\d\[[^\]]*\]", "", ln) for ln in run_lines("c8-100", "CAND")], [], "c8-100", "CAND")[1]) and p == [])

# ── R2a / R2b ─────────────────────────────────────────────────────────────
b, _ = b48.row_readouts(run_lines("c1s-400", "CAND", qcwnd=[int(0.023 * 250000)], backlog=[2000]), [], "c1s-400", "CAND")
check("R2b on a collapsed row (qcwnd 0.023 x BDP, B = 2000): R2b = R2a = 1.0", b["legs"][0]["r2b"] == 1.0 and b["legs"][0]["r2a"] == 1.0 and b["r2b"] == 1.0 and b["legs"][0]["B_med"] == 2000, b["legs"][0])
b, _ = b48.row_readouts(run_lines("c1s-400", "CAND", qcwnd=[int(0.023 * 250000)], backlog=[0]), [], "c1s-400", "CAND")
check("R2b on a trickle leg (same tiny qcwnd, B = 0): R2b = 0 while R2a (reported) = 1.0", b["legs"][0]["r2b"] == 0.0 and b["legs"][0]["r2a"] == 1.0, b["legs"][0])
b, _ = b48.row_readouts(run_lines("c1s-400", "CAND", qcwnd=[int(0.4 * 250000)], backlog=[2000]), [], "c1s-400", "CAND")
check("a window of 0.4 x BDP with a backlog counts (< 0.5); 0.6 x BDP does not (R2a still counts it)",
      b["legs"][0]["r2b"] == 1.0 and b48.row_readouts(run_lines("c1s-400", "CAND", qcwnd=[150000], backlog=[2000]), [], "c1s-400", "CAND")[0]["legs"][0]["r2b"] == 0.0)
b, _ = b48.row_readouts(run_lines("c1s-400", "CAND", qcwnd=[5750], backlog=[97]), [], "c1s-400", "CAND")
check("B = 97 datagrams is below the 98 limit (R2b 0), B = 98 counts", b["legs"][0]["r2b"] == 0.0 and b48.row_readouts(run_lines("c1s-400", "CAND", qcwnd=[5750], backlog=[98]), [], "c1s-400", "CAND")[0]["legs"][0]["r2b"] == 1.0)
b, _ = b48.row_readouts(run_lines("c8-100", "CAND", qcwnd=[250000, 40000], backlog=[0, 2000]), [], "c8-100", "CAND")
check("c8: the c3 leg's BDP is 100 000 (40 000 < 50 000 collapses), the c2 leg's is 125 000 (250 000 does not); the row's R2b is the max over legs",
      b["legs"][0]["r2b"] == 0.0 and b["legs"][1]["r2b"] == 1.0 and b["r2b"] == 1.0 and b["legs"][1]["bdp"] == 100000, b["legs"])
b, _ = b48.row_readouts(run_lines("c8-100", "CAND", qcwnd=[10000, 10000], backlog=[2000, 2000], active=[True, False]), [], "c8-100", "CAND")
check("an inactive leg (qlp sent does not advance) is not scored: leg 1 has no active line, R2b None", b["legs"][1]["active_n"] == 0 and b["legs"][1]["r2b"] is None and b["legs"][0]["r2b"] == 1.0 and b["r2b"] == 1.0, b["legs"])
b, _ = b48.row_readouts(run_lines("c2-100", "BASE", qcwnd=[1000], backlog=[2000]), [], "c2-100", "BASE")
check("R2a / R2b are computed for BASE rows too (the BASE-collapse line)", b["legs"][0]["r2b"] == 1.0)

# ── scorer on synthetic pools ─────────────────────────────────────────────
g41.RES_DRAWS, g41.RES_GRID = 2, [0.05]
_DEF = g41.statistic_rank.__defaults__
FAST = (2000,) + _DEF[1:]            # the ladder tests run at N_PERM 2000; the dry run on the real ledger at the registered 10 000
g41.statistic_rank.__defaults__ = FAST


def srow(cell, arm, seed, rep, rng, mbps=800.0, cpu=1.0, r2b=0.0, wrtt=(30, 30), dnf=False, status="LIVE", b=None):
    n = len(BDP[cell])
    r = {"cell": cell, "store": "def", "arm": arm, "seed": seed, "rep": rep, "rc": 0, "status": status, "problems": [], "dnf": dnf,
         "mbps": None if dnf else rng.gauss(mbps, 0.02 * mbps), "cpu_cli": rng.gauss(16.0 * cpu, 0.2) * 0.4, "cpu_srv": rng.gauss(19.0, 0.2) * 0.4,
         "cli_lag_E": 1000.0 * rng.lognormvariate(0, 0.3), "srv_lag_E": 1000.0 * rng.lognormvariate(0, 0.3), "bytes": 5000000000 if cell.startswith(("c1s", "c1d")) else 600000000}
    for i in range(n):
        r["rtp_floor_p%d" % i] = rng.gauss(2500.0, 60.0)
    r["b48"] = {"span_s": 40.0, "n_diag": 160, "r2b": r2b,
                "legs": [{"active_n": 100, "r2a": 0.1, "r2b": r2b, "B_med": 10.0, "wrtt_med": float(wrtt[min(i, 1)]), "rtt_med": 20.0, "dgq_full": 0, "dgev": 3, "bdp": BDP[cell][i]} for i in range(n)],
                "qcc": [{"n": 50, "occ": {"S": 0.1, "D": 0.0, "B": 0.85, "R": 0.05}, "qrt_us_med": 2000, "qsr_us_med": 3000, "margin_med": 1.3, "qmin_us_med": 1900, "qcns_mean": 900} for _ in range(n)],
                "echo": {"stock": arm == "BASE", "burst_robust": arm == "CAND", "qm_field": arm == "CAND", "qcc_line": arm == "CAND"}}
    return r


def ledger(cand=None, base=None, seed=5):
    """cand / base: per-cell kwargs dict {cell: kw} or {"*": kw}; one pool of 96 slots in plan 48's order."""
    rng = random.Random(seed)
    rows = []
    for (c, st, a, sd, rp) in pp.plan48():
        kw = dict(((cand if a == "CAND" else base) or {}).get("*", {}))
        kw.update(((cand if a == "CAND" else base) or {}).get(c, {}))
        rows.append(srow(c, a, sd, rp, rng, **kw))
    return rows


def run_score(rows, **kw):
    lines = []
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "l.log")
        open(pth, "w").write("".join("P38ROW " + json.dumps(r) + "\n" for r in rows))
        res = b48.score([pth], out=lines.append, **kw)
    return res, lines


def verdict(lines):
    v = [ln for ln in lines if ln.startswith("VERDICT48 ")]
    return v[0][len("VERDICT48 "):] if len(v) == 1 and lines[-1] == v[0] else None


res, lines = run_score(ledger())
tx = "\n".join(lines)
check("A/A pool: VERDICT48 PROMOTE-ELIGIBLE, and it is the last line", verdict(lines) == "PROMOTE-ELIGIBLE", [ln for ln in lines if "VERDICT" in ln or "p =" in ln])
for needle in ("ROWS 96/96", "GATE-41c[rank-minp] PASS", "R1 (the guard)", "RESOLUTION (computed from BASE rows alone", "P_REST", "p_rest =", "TAIL CLAUSE", "BASE-COLLAPSE", "R2 (the", "R2a", "R2b",
               "NOT-CONTROLLED test c2-100", "R2B-LEG-READING", "R3 (authority", "R4 (CAND bind gauges", "DATAGRAM EVICTIONS", "LADDER", "REGISTERED PREDICTION", "resolution41c"):
    check(f"every registered readout is printed: {needle!r}", needle in tx)
check("R1 is 26 clauses (c1s 6, c1d 7, c2 6, c8 7); p_rest is 24 (no c2 / c8 goodput) and still has the c1 goodput clauses",
      len(res["res"]["z_true"] or {}) in (0, 26) and len(res["p_rest"]["p_true"]) == 24 and ("c1s-400", "goodput") in res["p_rest"]["p_true"] and ("c1d-400", "goodput") in res["p_rest"]["p_true"]
      and ("c2-100", "goodput") not in res["p_rest"]["p_true"] and ("c8-100", "goodput") not in res["p_rest"]["p_true"], len(res["p_rest"]["p_true"]))
check("p_rest uses the guard's relabellings: with no goodput difference it equals or exceeds the guard's p's order, same RNG stream (an exclusion changes only the clause set)",
      g41.statistic_rank(ledger(), "BASE", "CAND", n_perm=300, cells=g41.CELLS_GUARD, design=g41.GUARD_DESIGN)["p"] <= 1.0)

# NOT-CONTROLLED
res, lines = run_score(ledger(cand={"c2-100": {"r2b": 0.06}}))
check("NOT-CONTROLLED: CAND median R2b 0.06 > 0.05 at one cell (ProbeRtt's 0.03 sits below)", verdict(lines) == "NOT-CONTROLLED", verdict(lines))
rows = ledger()
cr = [r for r in rows if r["arm"] == "CAND" and r["cell"] == "c1s-400"][0]
cr["b48"]["r2b"] = 0.30
res, lines = run_score(rows)
check("NOT-CONTROLLED: a single CAND row with R2b > 0.25 (the median is 0)", verdict(lines) == "NOT-CONTROLLED", verdict(lines))
res, lines = run_score(ledger(cand={"c2-100": {"r2b": 0.03}, "c8-100": {"r2b": 0.05}}))
check("R2b at ProbeRtt's duty (0.03) and exactly 0.05 is not NOT-CONTROLLED", verdict(lines) == "PROMOTE-ELIGIBLE", verdict(lines))
res, lines = run_score(ledger(cand={"c2-100": {"r2b": 0.06}}, base={"*": {"r2b": 0.9}}))
check("BASE's R2b never decides (BASE collapse is reported only): NOT-CONTROLLED here comes from CAND", verdict(lines) == "NOT-CONTROLLED")
res, lines = run_score(ledger(base={"*": {"r2b": 0.9}}))
check("a collapsed BASE with a clean CAND is not NOT-CONTROLLED", verdict(lines) != "NOT-CONTROLLED", verdict(lines))

# CONTROLLED-WITH-COST: goodput -5 % at c2 and c8 (R1 fails there, p_rest passes), wrtt c2 30 -> 8, c8 slow leg 30 -> 10
cost = {"c2-100": {"mbps": 760.0, "wrtt": (8, 8)}, "c8-100": {"mbps": 760.0, "wrtt": (8, 10)}}
base_w = {"c2-100": {"wrtt": (30, 30)}, "c8-100": {"wrtt": (30, 30)}}
res, lines = run_score(ledger(cand=cost, base=base_w))
check("CONTROLLED-WITH-COST: goodput 0.95 x BASE at c2 and c8, wrtt 0.27 x at c2 and 0.33 x on c8's slow leg, tail holds; R1 fails, p_rest > 0.05",
      verdict(lines) == "CONTROLLED-WITH-COST" and res["res"]["gate"] == "FAIL" and res["p_rest"]["p"] > 0.05, (verdict(lines), res["res"]["p"], res["p_rest"]["p"]))
res, lines = run_score(ledger(cand={"c2-100": {"mbps": 760.0, "wrtt": (8, 8)}, "c8-100": {"mbps": 760.0, "wrtt": (8, 25)}}, base=base_w))
check("c8's slow-leg wrtt ratio 0.83 > 0.7 fails the cost conditions -> REGRESSION", verdict(lines) == "REGRESSION", verdict(lines))
res, lines = run_score(ledger(cand={"c2-100": {"mbps": 760.0, "wrtt": (8, 8)}, "c8-100": {"mbps": 760.0, "wrtt": (20, 10)}}, base=base_w))
check("c8's FAST leg is not tested (wrtt ratio 0.67 there, 0.33 on the slow leg): still CONTROLLED-WITH-COST", verdict(lines) == "CONTROLLED-WITH-COST", verdict(lines))
res, lines = run_score(ledger(cand={"c2-100": {"mbps": 700.0, "wrtt": (8, 8)}, "c8-100": {"mbps": 760.0, "wrtt": (8, 10)}}, base=base_w))
check("goodput 0.875 x BASE at c2 (< 0.90): REGRESSION", verdict(lines) == "REGRESSION", verdict(lines))
res, lines = run_score(ledger(cand={"c2-100": {"mbps": 760.0, "wrtt": (24, 24)}, "c8-100": {"mbps": 760.0, "wrtt": (8, 10)}}, base=base_w))
check("wrtt ratio 0.8 at c2 (> 0.7): REGRESSION", verdict(lines) == "REGRESSION", verdict(lines))
# REGRESSION by R1 (a big CPU cost everywhere in p_rest, so p_rest <= 0.05)
res, lines = run_score(ledger(cand={"*": {"cpu": 1.4}}))
check("REGRESSION: a +40 % CPU at every cell fails R1 and p_rest", verdict(lines) == "REGRESSION" and res["p_rest"]["p"] <= 0.05, (verdict(lines), res["p_rest"]["p"]))
# tail clause: one CAND row at 0.5 x median, R1 still passes -> not PROMOTE-ELIGIBLE
rows = ledger()
cr = [r for r in rows if r["arm"] == "CAND" and r["cell"] == "c8-100"][3]
cr["mbps"] *= 0.5
res, lines = run_score(rows)
check("tail clause: a CAND row at 0.5 x its cell median blocks PROMOTE-ELIGIBLE (R1 passing) -> REGRESSION by the literal ladder", verdict(lines) == "REGRESSION" and "c8-100: lowest" in "\n".join(lines) and "FAILS" in "\n".join(lines), verdict(lines))
rows = ledger()
cr = [r for r in rows if r["arm"] == "CAND" and r["cell"] == "c1d-400"][5]
cr.update({"dnf": True, "mbps": None, "cpu_cli": None, "cpu_srv": None})
res, lines = run_score(rows)
check("a CAND DNF fails the tail clause (counts as goodput 0) and is not void; R2 still reads its lines", "DNF" in "\n".join(l for l in lines if "lowest" in l) and verdict(lines) != "PROMOTE-ELIGIBLE", verdict(lines))
rows = ledger()
br = [r for r in rows if r["arm"] == "BASE" and r["cell"] == "c2-100"][2]
br.update({"dnf": True, "mbps": None})
res, lines = run_score(rows)
check("a BASE DNF is reported as collapse evidence (BASE-COLLAPSE line) and does not stop the verdict", any("c2-100: BASE DNF/crash rows 1 of 12" in l for l in lines) and verdict(lines) is not None)
# NO VERDICT
rows = ledger()
vr = [r for r in rows if r["arm"] == "CAND" and r["cell"] == "c2-100"][0]
vr["status"] = "WITNESS-FAIL"
res, lines = run_score(rows)
check("an unreplaced witness-void slot: NO VERDICT, never a label", verdict(lines) == "NO VERDICT" and not any(l.startswith("VERDICT48 P") for l in lines), verdict(lines))
rows = ledger()
for r in [r for r in rows if r["arm"] == "CAND"][:6]:
    r["status"] = "WITNESS-FAIL"
res, lines = run_score(rows)
check("more than 4 voided slots: NO VERDICT (beyond the replacement cap)", verdict(lines) == "NO VERDICT" and any("beyond the replacement cap" in l for l in lines))
res, lines = run_score(ledger()[:-2])
check("a missing slot (94 rows): NO VERDICT", verdict(lines) == "NO VERDICT", verdict(lines))
# the replacement rule still applies: a replaced void is scored
rows = ledger()
orig = [r for r in rows if r["arm"] == "CAND" and r["cell"] == "c2-100"][0]
rep_ = dict(orig)
orig = dict(orig)
orig["status"] = "WITNESS-FAIL"
rows[rows.index([r for r in rows if r["arm"] == "CAND" and r["cell"] == "c2-100"][0])] = orig
res, lines = run_score(rows + [rep_])
check("a witness-voided slot re-run once (the replacement row) scores: REPLACED SLOTS 1 and a label", any(l.startswith("REPLACED SLOTS: 1 ") for l in lines) and verdict(lines) == "PROMOTE-ELIGIBLE", verdict(lines))
rows = ledger()
nrow = [r for r in rows if r["arm"] == "CAND"][0]
nrow["b48"] = None
res, lines = run_score(rows)
check("a CAND row without an R2b: NO VERDICT (R2b unreadable)", verdict(lines) == "NO VERDICT", verdict(lines))
lines = []
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "l.log")
    open(pth, "w").write("".join("P38ROW " + json.dumps(r) + "\n" for r in ledger()) + "ABORT-CAND is not a token\nABORT-SMOKE\n")
    b48.score([pth], out=lines.append)
check("an ABORT token in the ledger: NO VERDICT", verdict(lines) == "NO VERDICT")

# the R2B leg-reading line flags a per-leg disagreement
rows = ledger()
for r in rows:
    if r["arm"] == "CAND" and r["cell"] == "c8-100":
        r["b48"]["legs"][1]["r2b"] = 0.2
        r["b48"]["r2b"] = 0.0
res, lines = run_score(rows)
check("R2B-LEG-READING states when the per-leg median would call NOT-CONTROLLED where the row maximum does not", any("would ALSO call NOT-CONTROLLED at c8-100 leg1" in l for l in lines))

# ── plan 48: the table ────────────────────────────────────────────────────
t = pp.plan48()
check("plan 48: 96 rows, smoke = CAND then BASE at c8, cells c2, c1s, c8, c1d per group", len(t) == 96 and pp.plan48(smoke=True) == [("c8-100", "def", "CAND", "42", 1), ("c8-100", "def", "BASE", "42", 1)]
      and [c for c, *_ in t[:16:2]] == ["c2-100", "c1s-400", "c8-100", "c1d-400"] * 2)
for c in ("c2-100", "c1s-400", "c8-100", "c1d-400"):
    allsr = sorted((sd, rp) for sd in ("42", "7") for rp in range(1, 7))
    check(f"plan 48 {c}: BASE and CAND each seeds 42 / 7 x reps 1-6 (12, 6 + 6 per seed)", sorted((sd, rp) for cc, s_, a, sd, rp in t if cc == c and a == "BASE") == allsr
          and sorted((sd, rp) for cc, s_, a, sd, rp in t if cc == c and a == "CAND") == allsr)
got = [[r[2] for r in t[i * 2:(i + 1) * 2]] for i in range(48)]
check("plan 48: interleave pinned as 41c: BASE,CAND on even groups, CAND,BASE on odd; each arm leads half the groups at every cell", all(got[j::4] == [["BASE", "CAND"], ["CAND", "BASE"]] * 6 for j in range(4)), got[:8])
check("plan 48: 12 groups rep-major, seed 42 before 7", [(r[3], r[4]) for r in t if r[0] == "c2-100" and r[2] == "CAND"] == [(sd, rp) for rp in range(1, 7) for sd in ("42", "7")])
check("plan 48: the same slots as plan 41c (so the replacement rule and the guard design apply unchanged)", sorted(t) == sorted(pp.plan41c()))
check("plan 48: the sizes are section 48's (c1s / c1d 5 GB, c2 / c8 600 MB)", pp.BYTES48 == {"c1s-400": 5000000000, "c1d-400": 5000000000, "c2-100": 600000000, "c8-100": 600000000})
import subprocess  # noqa: E402
cp = subprocess.run([sys.executable, "place38_parse.py", "plan48", "smoke"], cwd=HERE, capture_output=True, text=True)
cp2 = subprocess.run([sys.executable, "place38_parse.py", "plan48"], cwd=HERE, capture_output=True, text=True)
check("place38_parse.py plan48 prints the table the battery reads (96 lines; smoke 2)", len(cp2.stdout.split("\n")) == 97 and cp.stdout.split("\n")[0] == "c8-100 def CAND 42 1", cp.stderr[-200:])

# ── smoke ──────────────────────────────────────────────────────────────────
sm = [srow("c8-100", "CAND", "42", 1, random.Random(1)), srow("c8-100", "BASE", "42", 1, random.Random(2))]
for r in sm:
    r["feed_ratio_p0"] = r["feed_ratio_p1"] = 1.0
    r["rtp_floor_p1"] = 2000.0
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "s.log")
    wr = lambda rs: open(pth, "w").write("".join("P38ROW " + json.dumps(r) + "\n" for r in rs))  # noqa: E731
    buf = io.StringIO()
    wr(sm)
    with redirect_stdout(buf):
        ok1 = b48.smoke48(pth) == 0
    wr(sm[1:])
    with redirect_stdout(buf):
        ok2 = b48.smoke48(pth) == 1
    bad = json.loads(json.dumps(sm[0]))
    bad["b48"]["qcc"][1]["n"] = 0
    wr([bad, sm[1]])
    with redirect_stdout(buf):
        ok3 = b48.smoke48(pth) == 1
    bad = json.loads(json.dumps(sm[0]))
    bad["status"], bad["problems"] = "WITNESS-FAIL", ["qm-not-SDBR=['X'] unparsed-blocks=0"]
    wr([bad, sm[1]])
    with redirect_stdout(buf):
        ok4 = b48.smoke48(pth) == 1
check("smoke48: PASS on LIVE CAND (a [QCC] block per leg) + BASE rows at c8; ABORT-SMOKE (rc 1) without the CAND row, with a leg missing its [QCC] block, or with a void CAND row", ok1 and ok2 and ok3 and ok4, (ok1, ok2, ok3, ok4))

# ── make_row under LD_PLAN=48: the witnesses void the row, the readouts land in the row ──────────────────────────
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
PD = os.path.join(RAW, "thread-pd", "battery")
FIX = {"c1s-400": "diag-smoke/c1s-400-MAIN-s42-r1-a1-", "c1d-400": "diag-tpd/c1d-400-MAIN-s42-r1-a1-"}


def mk(cell, arm, lines):
    key = "c1d-400" if cell.startswith(("c1d", "c8")) else "c1s-400"
    drv, cli, srv = (lc.read(os.path.join(PD, FIX[key] + x)) for x in ("drv.out", "c.log", "s.log"))
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and "[ETA] site=sender" not in ln and b48.STOCK_ECHO not in ln and b48.BRS_ECHO not in ln]
    cli = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in cli]
    srv = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in srv]
    nleg = len(BDP[cell])
    egress = [500, 500] if nleg == 2 else [1000]
    drv = drv + [f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams={e} egress_skbs=10 gso=1.00 netem_sent_dgrams={e} netem_dropped_skbs=0 backlog=0 lost=0 loss=0.000000 rcvbuf_drops=0 rcvbuf_scope=netns"
                 for i, e in enumerate(egress)]
    os.environ["LD_PLAN"] = "48"
    os.environ["LD_ROW_BYTES"] = "5000000000"
    try:
        return pp.make_row(cell, arm, "42", 1, 0, 60.0, drv, cli + lines, srv, 0, "x", store="def")
    finally:
        os.environ.pop("LD_PLAN"), os.environ.pop("LD_ROW_BYTES")


r = mk("c1s-400", "CAND", run_lines("c1s-400", "CAND"))
check("make_row (LD_PLAN=48): a CAND row with every witness is LIVE, carries b48 and its own byte count", r["status"] == "LIVE" and r["b48"]["legs"][0]["active_n"] > 0 and r["bytes"] == 5000000000, (r["status"], r["problems"]))
r = mk("c1s-400", "CAND", run_lines("c1s-400", "BASE"))
check("make_row (LD_PLAN=48): a CAND row that echoes the stock BBR and prints no [QCC] is WITNESS-FAIL", r["status"] == "WITNESS-FAIL" and "cand-no-QCC-line" in r["problems"], (r["status"], r["problems"]))
r = mk("c1s-400", "BASE", run_lines("c1s-400", "BASE"))
check("make_row (LD_PLAN=48): a BASE row with the stock echo and no qm= is LIVE", r["status"] == "LIVE", (r["status"], r["problems"]))
r = mk("c1s-400", "BASE", run_lines("c1s-400", "CAND"))
check("make_row (LD_PLAN=48): a BASE row that shows qm= is WITNESS-FAIL", r["status"] == "WITNESS-FAIL" and "base-has-qm-or-QCC" in r["problems"], (r["status"], r["problems"]))
r = mk("c1d-400", "CAND", run_lines("c1d-400", "CAND", n=40))
check("make_row (LD_PLAN=48): under 20 s of [DIAG] is WITNESS-FAIL", r["status"] == "WITNESS-FAIL" and any(p.startswith("diag-span") for p in r["problems"]), r["problems"])
os.environ.pop("LD_PLAN", None)
# other plans' rows carry no b48
drv, cli, srv = (lc.read(os.path.join(PD, FIX["c1s-400"] + x)) for x in ("drv.out", "c.log", "s.log"))
r = pp.make_row("c1s-400", "BASE", "42", 1, 0, 12.0, drv, cli, srv, 0, "x", store="def")
check("other plans' rows (LD_PLAN unset) carry no b48 and no bytes field: the committed plans parse as before", "b48" not in r and "bytes" not in r)

# per-row bytes: CPU/GB uses the row's own size when present, the cell table's otherwise
a = {"cell": "c1s-400", "cpu_cli": 20.0, "dnf": False}
check("CPU/GB uses the row's own bytes (5 GB) when present and the cell table's (400 MB) otherwise",
      abs(g41.value(dict(a, bytes=5000000000), "c1s-400", "cpu_cli") - 4.0) < 1e-9 and abs(g41.value(a, "c1s-400", "cpu_cli") - 50.0) < 1e-9)

# ── the shell side ────────────────────────────────────────────────────────
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
check("battery plan 48: accepted, 96-row table, 2-row smoke, p48 sizes 5 GB / 600 MB, timeout 200, CAND runs RWM_QUIC_CC=bbr_rs, BASE no env, replacement file allowed",
      "|46|48|52)" in bat and '-eq 96' in bat and "ABORT-PLAN48" in bat and "place38_parse.py plan48" in bat and "c1s-400|c1d-400) echo 5000000000" in bat and "c2-100|c8-100) echo 600000000" in bat
      and "RWM_PERF_TIMEOUT_S=200" in bat and "[ \"$P48\" = 1 ] && arm_env=(RWM_QUIC_CC=bbr_rs)" in bat and "LD_ROW_BYTES=" in bat and "plan48 0" in bat and "plan48 1" in bat)
check("battery plan 48: the CAND binary must carry [QCC] and RWM_QUIC_CC, BASE must not carry [QCC] (grep -F)", 'grep -aqF -- "[QCC] t=" "$BIN"' in bat and 'grep -aqF -- "[QCC] t=" "$BIN_BASE"' in bat)
check("battery: every row's env unsets RWM_QUIC_CC before the arm env (BASE never inherits it)", "-u RWM_QUIC_CC" in bat and bat.index("-u RWM_QUIC_CC") < bat.index('"${arm_env[@]}" "${evt_env[@]}"'))
check("run_all plan 48: selectable, needs LD_CAND_SHA (ABORT-CAND) and LD_BASE_SHA, builds BASE, suites first on CAND's tree, then ONE stage (smoke -> battery -> replacements -> score48)",
      re.search(r"^  48\) P_SMOKE=", runall, re.M) and "ABORT-CAND LD_CAND_SHA is unset" in runall and "LD_BASE_SHA" in runall and 'P48=0; [ "$PLAN" = "48" ] && { P48=1; P46=1; P41=1; }' in runall
      and runall.index('SD="$RUN/suites"; mkdir -p "$SD"\n  suites_stage\n') < runall.index("stage bbr48 48") and "48) smk=smoke48; scr=score48; chk=check48 ;;" in runall
      and 'test_bbr48_parse.py >>' in runall)
check("run_all plan 48: ABORT-SMOKE when smoke-check fails, ABORT-ROWS unless 96 rows, the 41c replacement rule applies, a REAL suite failure is ABORT-TESTS",
      "ABORT-SMOKE stage" in runall and '[ "$nrows" != "96" ]' in runall and '[ "$splan" = "41c" ] || [ "$splan" = "48" ]' in runall and 'else run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"; fi' in runall
      and '[ "$PLAN" = "41" ] || [ "$PLAN" = "47" ] || [ "$PLAN" = "48" ]' in runall)
check("run_all / battery crlf_guard list the new files", "bbr48_score.py test_bbr48_parse.py" in runall and "bbr48_score.py test_bbr48_parse.py" in bat)
lp = open(os.path.join(HERE, "loopdiag_parse.py"), encoding="utf-8").read()
check("loopdiag_parse dispatches smoke48 / check48 / score48", all(f'cmd == "{c}48"' in lp for c in ("smoke", "check", "score")))
cp = subprocess.run(["bash", "-n", os.path.join(HERE, "loopdiag_run_all.sh")], capture_output=True, text=True)
cq = subprocess.run(["bash", "-n", os.path.join(HERE, "loopdiag_battery.sh")], capture_output=True, text=True)
check("both shell scripts parse (bash -n)", cp.returncode == 0 and cq.returncode == 0, cp.stderr + cq.stderr)

# ── the dry run on the committed status-47 guard ledger ───────────────────
LEDGER = os.path.join(RAW, "mergecand47", "s47b", "run", "guard", "ld.log")
g41.statistic_rank.__defaults__ = _DEF
if os.path.exists(LEDGER):
    lines = []
    out = b48.score([LEDGER], out=lines.append, dry=True)
    tx = "\n".join(lines)
    check("dry run on status 47's real guard ledger (session 2): GATE-41c p = 0.2256 and min-p 0.01160 at c1d-400 rtp_floor_p0, bit for bit",
          "  p = 0.2256  (share of the 10000 relabellings" in tx and "min-p (true) = 0.01160 at c1d-400 rtp_floor_p0" in tx and "GATE-41c[rank-minp] PASS (p=0.2256;" in tx, [l for l in lines if "p = " in l])
    check("dry run: p_rest is computed beside it (24 clauses)", out["p_rest"]["p"] is not None and len(out["p_rest"]["p_true"]) == 24)
else:
    check("dry-run ledger present", False, LEDGER)

check("run_all plan 48: LD_REHEARSE=1 skips the suites, stops after the smoke check with REHEARSAL-PASS (return 2) and exits 0 with LD-ALL-REHEARSAL-DONE (status 48b amendment 1)",
      '[ "$REHEARSE" = 1 ]; then era "LD-ALL suites SKIPPED (rehearsal)"' in runall
      and 'touch "$dir/REHEARSAL-PASS"' in runall and 'return 2; fi' in runall
      and 'if [ "$src" = "2" ]; then persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0; fi' in runall
      and runall.index('touch "$dir/SMOKE-PASS"') < runall.index('touch "$dir/REHEARSAL-PASS"') < runall.index('LD_TAG=ld bash ./loopdiag_battery.sh'))
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
