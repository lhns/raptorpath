#!/usr/bin/env python3
"""Tests for the status 47 harness: the promoted `[ETA]` line and gate (no w_mv, no RWM_PLACE_WIRE), the qcwnd-below-BDP witness, the guard on
the merge candidate (plan 41c: BASE n = 12 against CAND n = 12 at c1s, c1d, c2, c8 default), and the combined merge-candidate session (plan 47).
Run: python3 test_harness47.py"""
import json
import os
import random
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as g41  # noqa: E402
import place46_parse as q46  # noqa: E402
import stage3_parse as sp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


# the session-resolution table that score41c prints is slow at its production size (tested in test_guard47.py); a tiny one here
g41.RES_DRAWS, g41.RES_GRID = 2, [0.05]

bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()

# ── the promoted [ETA] line ───────────────────────────────────────────────
OLD = ("[ETA] site=sender fhat_us=1 n=9 zero=0.0000 place_n=40 cold_r=0.0000 cold_ge=0.2500 t_eff=- t_cold=- t_n=0 hol_sh=- hol_n=0 hol_mv=- hol_calls=0 hol_w=- "
       "ref_fl=0.0000 w_cold=0.0100 w_mv=0.2000 w_n=500 w_calls=1000 p0:n=1/1 drop=0 tau_us=100 e_p50=900 e_p90=2000 e_mx=3000 late=0.0000 sig_us=5/n3")
NEW = OLD.replace("w_mv=0.2000 ", "")
e_old, e_new = pp.eta_last([OLD]), pp.eta_last([NEW])
check("run_all: BASE's COMMIT pin is LD_BASE_SHA when set (new main as BASE), else the historical 4926822",
      'BASE_PIN="${LD_BASE_SHA:-4926822}"' in runall and 'in "$BASE_PIN"*) ;;' in runall and "4926822*) ;;" not in runall)
check("[ETA]: the promoted line (ends `ref_fl= w_cold= w_n= w_calls=`, no w_mv) parses: w_mv None, the rest as before",
      e_new is not None and e_new["w_mv"] is None and e_new["w_n"] == 500 and e_new["w_calls"] == 1000 and e_new["ref_fl"] == "0.0000" and e_new["w_cold"] == "0.0100"
      and e_new["e_p50"][0] == "900" and e_new["cold_ge"] == "0.2500", e_new)
check("[ETA]: the old line (with w_mv) still parses, so the committed 38 / 41 / 41b ledgers re-score", e_old is not None and e_old["w_mv"] == "0.2000" and e_old["w_calls"] == 1000)
check("[ETA]: the unmeasured form (`ref_fl=- w_cold=- w_n=0 w_calls=0`) parses", pp.eta_last([NEW.replace("ref_fl=0.0000 w_cold=0.0100", "ref_fl=- w_cold=-").replace("w_n=500 w_calls=1000", "w_n=0 w_calls=0")])["w_n"] == 0)

# ── rows on the promoted binary ───────────────────────────────────────────
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
PD = os.path.join(RAW, "thread-pd", "battery")
FIX = {"c1s-400": "diag-smoke/c1s-400-MAIN-s42-r1-a1-", "c1d-400": "diag-tpd/c1d-400-MAIN-s42-r1-a1-"}


def leg_block(i, infl, rtt, wrtt, wpl, qcwnd):
    return (f" p{i}:infl={infl}/sinfl=0/bdp1977(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0000 cmp=- "
            f"rtt={rtt}/wrtt={wrtt}/rtp3ms rtp_us=2842 sig_us=1/n2 rvar_us=1/n2 qsp_us=1/n2 msd_us=1/n2 tlag_us=1/n2 gapd=0/0 "
            f"qcwnd={qcwnd} qce=16 qlp=360/251405 | ANCHOR sent=0 al=0 attr=0 nr=0 rej[iv=0 zr=0 al=0] gen=0 fill=1454 plu=0.0010 plc=0.0014 wpl_us={wpl}")


def diag(t, legs):
    return f"[DIAG] t={t}s win=768/1024 paused=19% good=842.6Mbit cwnd=1853 infl=437 np={len(legs)} rtt=104.1ms" + "".join(leg_block(i, *l) for i, l in enumerate(legs)) + " dgev0=0"


def gates(lines, drop=(), **kw):
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            for k, v in kw.items():
                ln = re.sub(r"\b%s=\S+" % k, f"{k}={v}", ln) if re.search(r"\b%s=\S+" % k, ln) else ln + f" {k}={v}"
            if drop:
                ln = re.sub(r"\s(?:%s)=\S+" % "|".join(drop), "", ln)
        out.append(ln)
    return out


def logs(cell, qcwnds, eta_line=NEW, nlines=12, drop_gate=True, gate_kw=None):
    key = "c1d-400" if cell.startswith(("c1d", "c8")) else "c1s-400"
    drv, cli, srv = (lc.read(os.path.join(PD, FIX[key] + x)) for x in ("drv.out", "c.log", "s.log"))
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and "[ETA] site=sender" not in ln]
    nleg = 1 if cell.startswith(("c1s", "c2")) else 2
    d = ["RWM_PLACE_WIRE"] if drop_gate else []
    cli, srv = gates(cli, d, **(gate_kw or {})), gates(srv, d, **(gate_kw or {}))
    cli = cli + [diag(0.5 + 0.25 * i, [(5, 8, 7, 7300, qcwnds[l][i % len(qcwnds[l])]) for l in range(nleg)]) for i in range(nlines)] + ([eta_line] if eta_line else [])
    egress = [500, 500] if nleg == 2 else [1000]
    drv = drv + [f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams={e} egress_skbs=10 gso=1.00 netem_sent_dgrams={e} netem_dropped_skbs=0 backlog=0 lost=0 loss=0.000000 rcvbuf_drops=0 rcvbuf_scope=netns"
                 for i, e in enumerate(egress)]
    return drv, cli, srv


def mkrow(cell, arm, qcwnds=None, **kw):
    nleg = 1 if cell.startswith(("c1s", "c2")) else 2
    qcwnds = qcwnds or [[1000000]] * nleg
    drv, cli, srv = logs(cell, qcwnds, **kw)
    return pp.make_row(cell, arm, "42", 1, 0, 12.0, drv, cli, srv, 0, "x", store="def")


for arm in ("MAIN", "CAND"):
    r = mkrow("c1s-400", arm)
    check(f"promoted tree: a {arm} row whose [GATES] has no RWM_PLACE_WIRE and whose [ETA] has no w_mv is LIVE", r["status"] == "LIVE" and r["problems"] == [], (r["status"], r["problems"]))
check("promoted tree: a two-leg CAND row (wpl_us numeric, new [ETA]) is LIVE", mkrow("c1d-400", "CAND")["status"] == "LIVE")
check("a MAIN row with the gate token present must still read 0 (=1 is void)", any(p.startswith("gate-place-wire") for p in mkrow("c1s-400", "MAIN", drop_gate=False, gate_kw={"RWM_PLACE_WIRE": "1"})["problems"]))
check("BASE and CAND stay strict: a RWM_PLACE_WIRE token on them is void (the binary is not main's / not promoted)",
      all(any("on " + a in p for p in mkrow("c1s-400", a, drop_gate=False, gate_kw={"RWM_PLACE_WIRE": "0"})["problems"]) for a in ("BASE", "CAND")))
check("a WIRE row needs the token: absent is void; =1 with the new [ETA] (no w_mv to check) is LIVE at a two-leg cell",
      any(p.startswith("gate-place-wire") for p in mkrow("c1d-400", "WIRE")["problems"])
      and not any(p.startswith("w_mv") for p in mkrow("c1d-400", "WIRE", drop_gate=False, gate_kw={"RWM_PLACE_WIRE": "1"})["problems"]))

import subprocess  # noqa: E402
cp = subprocess.run([sys.executable, "-c", "import place38_parse as p, stage3_parse as s; assert 'CAND' in s.ARM_SPEC and 'BASE' in s.ARM_SPEC"], cwd=HERE, capture_output=True, text=True)
check("place38_parse alone (the plan 41c row parser, no place46 import) knows the CAND and BASE arms", cp.returncode == 0, cp.stderr[-200:])

# ── the qcwnd-below-BDP witness ───────────────────────────────────────────
check("BDP per leg = link x 2 x one-way delay: c1s 250 000, c1d 250 000 x2, c2 125 000, c8 125 000 (c2 leg) and 100 000 (c3 leg); the 46 cells match",
      [pp.bdp_bytes(c) for c in ("c1s-400", "c1d-400", "c2-100", "c8-100", "c8-500", "c1d-3500")]
      == [[250000], [250000, 250000], [125000], [125000, 100000], [125000, 100000], [250000, 250000]], [pp.bdp_bytes(c) for c in ("c1s-400", "c1d-400", "c2-100", "c8-100")])
r = mkrow("c2-100", "CAND", qcwnds=[[100000, 125000, 130000, 200000]], nlines=12)
check("qcwnd witness (c2, BDP 125 000): the strict share of lines below it (100 000 only; 125 000 is not below): 2 of the 10 lines at t >= 1 s (the 2 lines before are skipped)",
      abs(r["qcwnd_below"]["per_leg"][0] - 0.2) < 1e-9 and r["qcwnd_below"]["any"] == r["qcwnd_below"]["per_leg"][0] and r["qcwnd_below"]["n"] == 10 and r["qcwnd_below"]["bdp"] == [125000], r["qcwnd_below"])
r = mkrow("c8-100", "CAND", qcwnds=[[130000], [90000, 110000]], nlines=12)
check("qcwnd witness (c8, per leg 125 000 / 100 000): leg 0 never below, leg 1 below on half of the lines (90 000 < 100 000, 110 000 not); any leg = leg 1",
      r["qcwnd_below"]["per_leg"] == [0.0, 0.5] and r["qcwnd_below"]["any"] == 0.5, r["qcwnd_below"])
r = mkrow("c1s-400", "CAND", qcwnds=[[249999]])
check("qcwnd witness (c1s, 250 000): 249 999 is below on every line", r["qcwnd_below"]["any"] == 1.0)
drv, cli, srv = logs("c2-100", [[10]])
cli = ['{"seconds":0.01,"warmup":true}'] + [diag(0.25 * i, [(5, 8, 7, 7300, 1)]) for i in range(1, 4)] + cli      # a warm-up run's lines (t < 1 s ignored), then more
check("qcwnd witness: lines before t = 1 s are not read (start-up), and it is never a void", pp.qcwnd_below_bdp(cli, "c2-100")["n"] == 10)
check("qcwnd witness: no data -> None, an unknown cell -> None", pp.qcwnd_below_bdp([], "c2-100") is None and pp.qcwnd_below_bdp(cli, "nope") is None)
check("the witness never voids a row: a row with every line below the BDP is still LIVE", mkrow("c1s-400", "CAND", qcwnds=[[1]])["status"] == "LIVE")

# ── plan 41c: the guard on the merge candidate ────────────────────────────
p41c = pp.plan41c()
check("plan 41c: 96 rows = 4 cells x (BASE 12 + CAND 12); smoke = one row per binary at c1s", len(p41c) == 96 == pp.EXPECT_ROWS41C and pp.plan41c(smoke=True) == [("c1s-400", "def", "BASE", "42", 1), ("c1s-400", "def", "CAND", "42", 1)])
check("plan 41c: c1s, c1d, c2 and c8 default only, in that order within each group", [c for c, *_ in p41c[:8:2]] == ["c1s-400", "c1d-400", "c2-100", "c8-100"] and {a for *_, a, _, _ in p41c} == {"BASE", "CAND"})
for cs in pp.GUARD_CELLS:
    rs = [r for r in p41c if (r[0], r[1]) == cs]
    allsr = sorted((sd, rp) for sd in ("42", "7") for rp in range(1, 7))
    check(f"plan 41c {cs[0]}: BASE = seeds 42 and 7 x reps 1-6 (12) and CAND = the same 12 (6 + 6 per seed): the registered design, one interleaved session",
          sorted((sd, rp) for c, st, a, sd, rp in rs if a == "BASE") == allsr and sorted((sd, rp) for c, st, a, sd, rp in rs if a == "CAND") == allsr)
got = [[r[2] for r in p41c[i * 2:(i + 1) * 2]] for i in range(48)]
check("plan 41c: the order is pinned: per group and cell the pair BASE,CAND (even groups) or CAND,BASE (odd groups); each arm leads half the groups at every cell",
      all(got[j::4] == [["BASE", "CAND"], ["CAND", "BASE"]] * 6 for j in range(4)), got[:8])
check("plan 41c: 12 groups, rep-major, seed 42 before 7", [(r[3], r[4]) for r in p41c if r[0] == "c2-100" and r[2] == "CAND"] == [(sd, rp) for rp in range(1, 7) for sd in ("42", "7")])

# ── the guard's scorer on synthetic rows ──────────────────────────────────
def grow(cell, arm, seed, rep, rng, cli=16.0, mbps=800.0, lag=1000.0, qany=0.1):
    n = 1 if cell.startswith(("c1s", "c2")) else 2
    r = {"cell": cell, "store": "def", "arm": arm, "seed": seed, "rep": rep, "rc": 0, "status": "LIVE", "problems": [], "dnf": False,
         "mbps": rng.gauss(mbps, 0.02 * mbps), "cpu_cli": rng.gauss(cli, 0.012 * cli) * 0.4, "cpu_srv": rng.gauss(19.0, 0.012 * 19.0) * 0.4,
         "cli_lag_E": lag * rng.lognormvariate(0, 0.3), "srv_lag_E": lag * rng.lognormvariate(0, 0.3), "shares": [1.0], "locked": False, "eta": None,
         "qcwnd_below": {"bdp": pp.bdp_bytes(cell), "n": 40, "per_leg": [qany] * n, "any": qany}}
    for i in range(n):
        r["rtp_floor_p%d" % i] = rng.gauss(2500.0, 60.0)
        r["feed_ratio_p%d" % i] = 1.0
    return r


def gledger(cand_kw=None, only=None, seed=5):
    rng = random.Random(seed)
    rows = []
    for (c, st, a, sd, rp) in pp.plan41c():
        kw = (cand_kw if (a == "CAND" and (only is None or c == only)) else None) or {}
        rows.append(grow(c, a, sd, rp, rng, **kw))
    return rows


def grun(rows):
    lines = []
    res = g41.report(rows, out=lines.append, cells=g41.CELLS_GUARD, test_arm="CAND", with_wire=False, title="§41b GUARD ON THE MERGE CANDIDATE", subject="the candidate build")
    return res, "\n".join(lines)


resA, tA = grun(gledger())
check("guard A/A: CAND drawn from BASE's distribution passes (p > 0.05) at four cells", resA["gate"] == "PASS" and resA["p"] > 0.05 and "GATE-41b PASS" in tA and "the candidate build may reach main" in tA, resA)
check("guard: all four cells and 26 clauses are in the statistic (c1s 6, c1d 7, c2 6, c8 7)", len(resA["z_true"]) == 26 and {k[0] for k in resA["z_true"]} == {c for c, _ in pp.GUARD_CELLS}, len(resA["z_true"]))
check("guard: BASE n = 12 + CAND n = 12 are pooled per cell and the relabelling keeps 6 + 6", tA.count("pool 12 BASE + 12 CAND; relabelled CAND rows per seed s42:6 s7:6") == 4, tA[:900])
check("guard: the resolution table covers the four cells with their own §21 tolerances (c2 goodput 1.4 %, c8 goodput 4.0 %) and is printed before CAND",
      "c2-100 goodput" in tA and "§21 tol=0.0140" in tA and "c8-100 goodput" in tA and "§21 tol=0.0400" in tA and tA.index("RESOLUTION") < tA.index("CAND against BASE"))
check("guard: no WIRE arm anywhere in the report", "WIRE" not in tA)
check("guard: the tail line is printed per arm and cell (BASE and CAND at all four cells)", tA.count("  TAIL ") == 8 and "TAIL c2-100 CAND" in tA and "TAIL c8-100 BASE" in tA)
def rk(rows):
    return g41.statistic_rank(rows, "BASE", "CAND", n_perm=3000, cells=g41.CELLS_GUARD)


resB = rk(gledger(cand_kw={"cli": 16.0 * 1.30}, only="c2-100"))
check("guard (rank-minp, 12/12): a +30 % client CPU on CAND at c2 alone is caught (p <= 0.01, the smallest clause p at a CPU clause)",
      resB["p"] <= 0.01 and resB["argmin"][0] == "c2-100" and resB["argmin"][1].startswith("cpu_"), (resB["p"], resB["argmin"]))
resS, _ = grun(gledger(cand_kw={"cli": 16.0 * 1.60}, only="c2-100"))
check("the registered 41b statistic (relabel-max) stays blind to a +60 % client CPU at c2 on this design (p well above 0.05): why it was replaced", resS["p"] > 0.05, resS["p"])
resC = rk(gledger(cand_kw={"mbps": 800.0 * 0.90}, only="c8-100"))
check("guard (rank-minp): 10 % lower goodput at c8 is caught", resC["p"] <= 0.05, resC["p"])
resD, _ = grun(gledger(cand_kw={"mbps": 880.0, "lag": 600.0}))
check("guard: one-sided - a CAND that is better on goodput and lag passes (rank-minp)", rk(gledger(cand_kw={"mbps": 880.0, "lag": 600.0}))["p"] > 0.05)
# the committed 41b path is unchanged by the generalisation
rows41b = pp.rows_of([os.path.join(RAW, "place41b", "ld.log")])[0]
l41b = []
g41.report(rows41b, out=l41b.append)
check("the §41b report on the committed ledger is unchanged in form (MAIN against BASE, WIRE reported, two cells, GATE-41b PASS p=0.7195)",
      any("GATE-41b PASS (p=0.7195; the gate-off binary may reach main)" in ln for ln in l41b) and any("WIRE (RWM_PLACE_WIRE=1), reported only" in ln for ln in l41b))
# score41c entry + the qcwnd section
lines = []
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "l.log")
    rr = gledger()
    rr[0]["qcwnd_below"]["any"] = 0.9
    with open(pth, "w") as fh:
        for r in rr:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    pp.score41c([pth], out=lines.append)
tx = "\n".join(lines)
check("score41c: ROWS 96/96, the guard, the QCWND-BELOW-BDP witness per cell and arm with the BDP values used, the c1s contention list and a VERDICT line",
      "ROWS 96/96" in tx and "QCWND-BELOW-BDP (reported only)" in tx and "c8-100@def: BDP per leg = [125000, 100000] B" in tx and "c1d-400@def: BDP per leg = [250000, 250000] B" in tx
      and "c2-100@def: BDP per leg = [125000] B" in tx and "c1s-400@def: BDP per leg = [250000] B" in tx and "any leg: share below" in tx and "CONTENTION c1s-400@def" in tx
      and any(ln.startswith("VERDICT GATE-41c ") for ln in lines) and not any(ln.startswith("VERDICT GATE-41b") for ln in lines), lines[-3:])
sm = []
for arm in ("BASE", "CAND"):
    r = grow("c1s-400", arm, "42", 1, random.Random(1))
    r["feed_ratio_p0"] = 1.0
    sm.append(r)
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "s.log")
    open(pth, "w").write("".join("P38ROW " + json.dumps(r) + "\n" for r in sm))
    ok1 = pp.smoke(pth, plan41c=True) == 0
    open(pth, "w").write("P38ROW " + json.dumps(sm[0]) + "\n")
    ok2 = pp.smoke(pth, plan41c=True) == 1
    open(pth, "w").write("".join("P38ROW " + json.dumps(r) + "\n" for r in rr) + "LD-BATTERY-DONE tag=ld blocks=6\n")
    ok3 = pp.check(pth, plan41c=True) == 0
check("smoke41c passes on LIVE BASE + CAND rows and fails without the CAND row; check41c wants 96 rows + LD-BATTERY-DONE", ok1 and ok2 and ok3)

# ── plan 46: the qcwnd witness is reported ────────────────────────────────
def r46(arm, variant, i, kind="scored", qany=0.2):
    r = {"cell": "c8-500", "store": "def", "arm": arm, "seed": "42", "rep": 1 + i, "rc": 0, "status": "LIVE", "problems": [], "variant": variant, "kind": kind,
         "qcwnd_below": {"bdp": [125000, 100000], "n": 30, "per_leg": [0.0, qany], "any": qany}, "inject_ok": True, "path_inactive": 0, "path_recovered": 0,
         "m": {"R": 5.0, "R_cens": False, "R0": 3.0, "R0_cens": False, "T_w": 4.0, "T_w_cens": False, "g_post": 90.0, "g_pre": 100.0, "g_hold": 40.0, "G": 0.1, "sigma_pre": 0.18,
               "event_ok": True, "exit_ok": True, "peak_rtt": 1500, "peak_wpl_us": 1500000, "event_peak_wrtt": 1500, "idle_lines": 5, "refresh_per_2s": 1.0}}
    return r


lines = []
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "l.log")
    open(pth, "w").write("".join("P46ROW " + json.dumps(r) + "\n" for r in [r46(a, "SLOW", i) for a in ("BASE", "CAND") for i in range(4)]))
    q46.score([pth], out=lines.append)
tx = "\n".join(lines)
check("plan 46 score: the qcwnd-below-BDP witness is printed per variant and arm with the c8 BDP values", "QCWND-BELOW-BDP (reported only)" in tx and "SLOW    CAND BDP [125000, 100000] B: leg0" in tx and "any leg" in tx, tx[:200])
cb, cc = r46("BASE", "SLOW", 0, "check"), r46("CAND", "FAST", 0, "check")
for c in (cb, cc):
    c["qdisc"] = {"ok": True, "notes": [], "peak_backlog": 10}
import io  # noqa: E402
from contextlib import redirect_stdout  # noqa: E402
buf = io.StringIO()
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "s.log")
    open(pth, "w").write("".join("P46ROW " + json.dumps(r) + "\n" for r in (cb, cc)))
    with redirect_stdout(buf):
        rc_ = q46.smoke(pth)
check("plan 46 smoke prints the qcwnd-below-BDP witness of each check row", rc_ == 0 and buf.getvalue().count("qcwnd_below_bdp=leg0") == 2, buf.getvalue()[:300])

# ── the shell side: every plan still accepted, plan 41c, the combined plan 47 ──
PLANS = ["24", "27", "29", "34", "36", "38", "40", "41", "41b", "41c", "43", "43b", "46", "48"]       # 48: status 48b, tested in test_bbr48_parse.py
m_b = re.search(r'case "\$PLAN" in ([^)]*)\) ;;', bat)
check("battery: every plan is accepted (24/27/29/34/36/38/40/41/41b/41c/43/43b/46/48)", m_b is not None and sorted(m_b.group(1).split("|")) == sorted(PLANS), m_b and m_b.group(1))
check("run_all: every plan is selectable (the plan case lists 24..46, 47 and 48)", all(re.search(r"^  %s\) P_SMOKE=" % re.escape(p), runall, re.M) for p in PLANS + ["47"]))
check("battery: plan 41c = 96-row table, smoke 2, the candidate must lack RWM_PLACE_WIRE (PCAND), CAND arm allowed for 41c and 46, parsed by place38_parse",
      '-eq 96' in bat and "ABORT-PLAN41C" in bat and 'PCAND=0; { [ "$PLAN" = "46" ] || [ "$PLAN" = "41c" ]; } && PCAND=1' in bat and 'elif [ "$PCAND" = 1 ]; then' in bat
      and 'CAND) [ "$PCAND" = 1 ]' in bat and "plan41c 0" in bat and "place38_parse.py plan41c" in bat)
i_suites, i_guard, i_p46 = runall.index('run_suites || fail_all "ABORT-TESTS (suites/TESTS.txt)"'), runall.index("stage guard 41c"), runall.index("stage p46 46")
check("run_all plan 47: suites on CAND's tree first, then the guard (41c), then plan 46; each in its own subdir (suites/, guard/, p46/)",
      i_suites < i_guard < i_p46 and 'SD="$RUN/suites"' in runall and 'dir="$RUN/$1"' in runall and "stage guard 41c" in runall and "stage p46 46" in runall)
check("run_all plan 47: a REAL suite failure is ABORT-TESTS before anything else runs; the FLAKE rule is the existing tst; all tools/l1 python tests run",
      'fail_all "ABORT-TESTS (suites/TESTS.txt)"' in runall and "FLAKE $name $t" in runall and '[ "$PLAN" = "41" ] || [ "$PLAN" = "47" ]' in runall
      and "test_*.py" in runall and 'tst doc' in runall and 'tst wasm' in runall and 'tst debug-witness' in runall and 'tst main' in runall)
check("run_all plan 47: a failed guard VERDICT does not stop plan 46 (only a stage that cannot run does); budget checked before each battery",
      "stage guard did not complete" in runall and 'if [ "$bto" -lt 1800 ]' in runall and "ABORT-INJECT stage" in runall)
check("run_all plan 47 / 41c / 46 are pinned by LD_CAND_SHA (refuse to launch while unset) and the parser gates include test_harness47.py",
      "ABORT-CAND LD_CAND_SHA is unset" in runall and 'test_harness47.py >>' in runall and 'P46=0; { [ "$PLAN" = "46" ] || [ "$PLAN" = "41c" ] || [ "$PLAN" = "47" ]; } && P46=1' in runall)
check("run_all: the suite logs go to $SD (so plan 47 keeps them under suites/) and other plans keep $RUN", 'SD="$RUN"' in runall and runall.count('"$SD/TESTS.txt"') >= 4)

check("run_all plan 47: the guard step is scored by score41c (never 41b's report) and asserts the 96 rows of the design",
      "41c) smk=smoke41c; scr=score41c; chk=check41c" in runall and '[ "$splan" = "41c" ] || [ "$splan" = "48" ]' in runall and '[ "$nrows" != "96" ]' in runall
      and "ABORT-ROWS stage" in runall and "score41b" not in runall.split("stage() {")[1].split("GOK=0")[0])
check("the guard's plan table seeds are 42 and 7 (rep-major, 6 + 6 per seed): the SEED passed to each row is the seed column", sorted({sd for *_, sd, _ in pp.plan41c()}) == ["42", "7"])

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
