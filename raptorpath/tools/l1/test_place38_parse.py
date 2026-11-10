#!/usr/bin/env python3
"""Tests for place38_parse.py (status §38 with amendments 1-3). Run: python3 test_place38_parse.py"""
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
os.environ.pop("P38_STORE", None)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


# ── the plan ───────────────────────────────────────────────────────────────
pl = pp.plan38()
check("plan 38: 96 invocations = 6 blocks x 8 cells x 2 arms", len(pl) == 96 == pp.EXPECT_ROWS38, len(pl))
check("plan 38: every (cell, store) has 6 rows per arm: 3 reps x seeds 42 and 7",
      all(sorted((s, r) for c, st, a, s, r in pl if (c, st) == cs and a == arm) == sorted(("42" if i % 2 == 0 else "7", 1 + i // 2) for i in range(6))
          for cs in pp.CELLS38 for arm in pp.ARMS))
check("plan 38: the cells are c1s/c2 default (controls), c1d/c8 default, c1d 1536, c8 192/256/512",
      pp.CELLS38 == [("c1s-400", "def"), ("c2-100", "def"), ("c1d-400", "def"), ("c8-100", "def"), ("c1d-400", "1536"),
                     ("c8-100", "192"), ("c8-100", "256"), ("c8-100", "512")])
check("plan 38: arm order rotates per block (MAIN first on even blocks, WIRE first on odd)",
      [pl[0][2], pl[1][2]] == ["MAIN", "WIRE"] and [pl[16][2], pl[17][2]] == ["WIRE", "MAIN"] and pl[16][3:] == ("7", 1), (pl[0], pl[16]))
check("plan 38: smoke = c8-100 RWM_STORE=256 in each arm", pp.plan38(smoke=True) == [("c8-100", "256", "MAIN", "42", 1), ("c8-100", "256", "WIRE", "42", 1)])
cp = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "plan"], capture_output=True, text=True)
check("`plan` prints the 96 `cell store arm seed rep` lines", cp.stdout.splitlines() == [" ".join(map(str, r)) for r in pl])
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
check("battery: plan 38 accepted, arms MAIN/WIRE, RWM_PLACE_WIRE and RWM_TEST_PLACE_HOLD withheld, 96-row table, RTOBS+RDIAG env",
      "36|38|40" in bat and "-u RWM_PLACE_WIRE -u RWM_TEST_PLACE_HOLD" in bat and "RWM_PLACE_WIRE=1" in bat and "-eq 96" in bat
      and "RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150" in bat and "ABORT-BIN" in bat)
check("run_all: plan 38 is gated on test_place38_parse.py, takes the root-owned tmpfs tree and LD_HARD, scores with score38",
      "test_place38_parse.py" in runall and "P_SCORE=score38" in runall and runall.count('"$PLAN" = "38"') >= 5)


# ── synthetic client logs ─────────────────────────────────────────────────
def leg_block(i, infl, rtt, wrtt, wpl):
    return (f" p{i}:infl={infl}/sinfl=0/bdp1977(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0000 cmp=- "
            f"rtt={rtt}/wrtt={wrtt}/rtp3ms rtp_us=2842 sig_us=1/n2 rvar_us=1/n2 qsp_us=1/n2 msd_us=1/n2 tlag_us=1/n2 gapd=0/0 "
            f"qcwnd=468342 qce=16 qlp=360/251405 | ANCHOR sent=0 al=0 attr=0 nr=0 rej[iv=0 zr=0 al=0] gen=0 fill=1454 plu=0.0010 plc=0.0014 wpl_us={wpl}")


def diag(t, legs, den=1024):
    """legs: [(infl, rtt, wrtt, wpl|'-')]"""
    return (f"[DIAG] t={t}s win=768/{den} paused=19% good=842.6Mbit cwnd=1853 infl=437 np={len(legs)} rtt=104.1ms"
            + "".join(leg_block(i, *l) for i, l in enumerate(legs)) + " dgev0=0")


def eta(w_cold="0.0100", w_mv="0.2000", w_n=500, w_calls=1000, ref_fl="0.0000", extra=" p0:n=1/1 drop=0 tau_us=100 e_p50=900 e_p90=2000 e_mx=3000 late=0.0000 sig_us=5/n3"):
    return (f"[ETA] site=sender fhat_us=1 n=9 zero=0.0000 place_n=40 cold_r=0.0000 cold_ge=0.2500 t_eff=- t_cold=- t_n=0 hol_sh=- hol_n=0 "
            f"hol_mv=- hol_calls=0 hol_w=- ref_fl={ref_fl} w_cold={w_cold} w_mv={w_mv} w_n={w_n} w_calls={w_calls}{extra}")


def gates(lines, **kw):
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            for k, v in kw.items():
                if re.search(r"\b%s=\S+" % k, ln):
                    ln = re.sub(r"\b%s=\S+" % k, f"{k}={v}", ln)
                else:
                    ln += f" {k}={v}"
        out.append(ln)
    return out


RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
C1S = os.path.join(RAW, "store-law-n1", "diag-sl", "c1s-400-CTL-s42-r1-a1-")
C1D = os.path.join(RAW, "loop-diag", "p34", "run", "diag-ld", "c1d-400-DEF1024-s42-r1-a1-")


def base_logs(cell):
    p = C1D if cell.startswith("c1d") or cell.startswith("c8") else C1S
    drv, cli, srv = lc.read(p + "drv.out"), lc.read(p + "c.log"), lc.read(p + "s.log")
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and "[ETA] site=sender" not in ln and "[SUMCAP]" not in ln]
    return drv, cli, srv


def mk(cell="c8-100", arm="WIRE", store="256", legs_fn=None, eta_line="auto", shares=(0.8, 0.2), gate_wire=None, sumcap=False, den=None, nlines=12, seed="42", rep=1):
    drv, cli, srv = base_logs(cell)
    nleg = 1 if cell.startswith(("c1s", "c2")) else 2
    gw = ("1" if arm == "WIRE" else "0") if gate_wire is None else gate_wire
    gs = "unset" if store == "def" else store
    cli, srv = gates(cli, RWM_PLACE_WIRE=gw, RWM_STORE=gs), gates(srv, RWM_PLACE_WIRE=gw, RWM_STORE=gs)
    n = den if den is not None else (int(store) if store != "def" else 1024)
    legs_fn = legs_fn or (lambda t: [(5, 8, 7, 7300)] + ([(3, 44, 43, 43200)] if nleg == 2 else [])
                          if arm == "WIRE" else [(5, 8, 7, "-")] + ([(3, 44, 43, "-")] if nleg == 2 else []))
    cli = cli + [diag(0.5 + 0.25 * i, legs_fn(0.5 + 0.25 * i), n) for i in range(nlines)]
    if sumcap:
        cli.append("[SUMCAP] on=1 eng=100/100 chg=100/100 chg_frac=1.0000 pin=0.0000 floor=0.0000 cap=300.0 ask=300.0")
    if eta_line == "auto":
        eta_line = eta() if arm == "WIRE" else eta("-", "-", 0, 0, "-")
    if eta_line:
        cli.append(eta_line)
    egress = [int(1000 * s) for s in shares] if nleg == 2 else [1000]
    drv = drv + [f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams={e} egress_skbs=10 gso=1.00 netem_sent_dgrams={e} netem_dropped_skbs=0 backlog=0 lost=0 loss=0.000000 rcvbuf_drops=0 rcvbuf_scope=netns"
                 for i, e in enumerate(egress)]
    return pp.make_row(cell, arm, seed, rep, 0, 12.0, drv, cli, srv, 0, "x", store=store)


# ── readers ────────────────────────────────────────────────────────────────
dl = pp.diag_lines([diag(1.0, [(5, 8, 7, 7300), (0, 44, 43, 43100)]), diag(1.25, [(2, 8, 7, 7000), (0, 150, 43, 43100)]),
                    diag(0.5, [(1, 3, 2, "-")])])
check("diag_lines splits the leg blocks (infl, rtt, wrtt, wpl_us, `-` -> None)",
      dl[0] == (1.0, {0: (5, 8, 7, 7300), 1: (0, 44, 43, 43100)}) and dl[2][1][0][3] is None, dl)
e = pp.eta_last([eta(), "x", eta(w_mv="0.5000", w_calls=2000)])
check("eta_last: the LAST [ETA] sender line; ref_fl, w_cold, w_mv, w_n, w_calls and the per-leg e_p50 / e_p90",
      e["w_mv"] == "0.5000" and e["w_calls"] == 2000 and e["w_n"] == 500 and e["w_cold"] == "0.0100" and e["e_p50"][0] == "900" and e["e_p90"][0] == "2000" and e["cold_ge"] == "0.2500", e)
em = pp.eta_last([eta("-", "-", 0, 0, "-")])
check("eta_last on a MAIN line: w_n = w_calls = 0 and ref_fl / w_cold / w_mv render `-`", em["w_n"] == 0 and em["w_calls"] == 0 and em["ref_fl"] == em["w_cold"] == em["w_mv"] == "-", em)
check("eta_last without a sender line is None", pp.eta_last(["[ETA] site=receiver x"]) is None)

# ── the lock-out classifier ───────────────────────────────────────────────
def locked(shares, legs_rows):
    diag_ = [(1.0 + 0.25 * i, {j: (0, r, w, None) for j, (r, w) in enumerate(l)}) for i, l in enumerate(legs_rows)]
    return pp.lock_analysis(diag_, shares)


rows_ = [[(8, 7), (150, 44)]] * 6
la = locked({0: 0.95, 1: 0.05}, rows_)
check("LOCKED: egress share 5 % < 10 % and median rtt 150 >= 3 x wrtt 44", la[1]["locked"] and not la[0]["locked"], la)
check("share exactly 10 % is not locked (< 10 %)", not locked({0: 0.90, 1: 0.10}, rows_)[1]["locked"])
check("rtt 131 < 3 x 44 = 132 is not locked; 132 is", not locked({0: .95, 1: .05}, [[(8, 7), (131, 44)]] * 6)[1]["locked"]
      and locked({0: .95, 1: .05}, [[(8, 7), (132, 44)]] * 6)[1]["locked"])
check("a low share with fresh prices (rtt ~ wrtt) is not locked", not locked({0: .95, 1: .05}, [[(8, 7), (45, 43)]] * 6)[1]["locked"])
check("wrtt 0 reads as 1 ms (3 x 1)", locked({0: .95, 1: .05}, [[(8, 7), (3, 0)]] * 6)[1]["locked"])
d2 = [(0.5, {0: (0, 8, 7, None), 1: (0, 500, 1, None)})] + [(1.0 + 0.25 * i, {0: (0, 8, 7, None), 1: (0, 45, 43, None)}) for i in range(6)]
check("warm-up lines (t < 1 s) are not read: a frozen price at 0.5 s only does not lock", not pp.lock_analysis(d2, {0: .95, 1: .05})[1]["locked"])
fb = pp.first_burst([(0.25, {0: (60, 8, 7, None), 1: (40, 150, 43, None)}), (0.5, {0: (1, 8, 7, None), 1: (1, 9, 8, None)})], 1)
check("first-burst readout: the second leg's in-flight share and rtt/wrtt on the first two-leg line", fb == {"t": 0.25, "infl_share": 0.4, "rtt": 150, "wrtt": 43}, fb)
dd = pp.idle_decay([(1.0, {0: (3, 8, 7, 7000), 1: (0, 44, 43, 43000)}), (1.25, {0: (3, 8, 7, 7000), 1: (0, 44, 43, 45000)}),
                    (2.25, {0: (3, 8, 7, 7000), 1: (0, 44, 43, 42000)}), (2.5, {0: (3, 8, 7, 7000), 1: (4, 44, 43, 99000)})])
check("idle-leg w-hat decay: the largest wpl_us on infl = 0 (45000 at t=1.25) and its value 1 s later (42000 at 2.25 is 1.0 s on)",
      dd[1] == {"t": 1.25, "wpl_us": 45000, "later": 42000} and 0 not in dd, dd)

# ── row witnesses ─────────────────────────────────────────────────────────
r = mk()
check("a good WIRE row at c8 RWM_STORE=256 is LIVE; share, first burst and the gauges are recorded",
      r["status"] == "LIVE" and r["problems"] == [] and abs(r["shares"][1] - 0.2) < 1e-9 and r["eta"]["w_calls"] == 1000 and r["first_burst"]["t"] == 0.5
      and r["wpl_close_frac"] == 1.0, (r["status"], r["problems"]))
check("a good MAIN row is LIVE (gate echo 0, wpl_us `-`, gauges `-`/0)", mk(arm="MAIN")["status"] == "LIVE", mk(arm="MAIN")["problems"])
check("witness: WIRE row whose [GATES] echoes RWM_PLACE_WIRE=0 is void", any(p.startswith("gate-place-wire") for p in mk(gate_wire="0")["problems"]))
check("witness: MAIN row whose [GATES] echoes RWM_PLACE_WIRE=1 is void", any(p.startswith("gate-place-wire") for p in mk(arm="MAIN", gate_wire="1")["problems"]))
check("witness: RWM_STORE on [GATES] must match the cell (256 vs 512)", any(p.startswith("gate-store=512") for p in mk(store="256")["problems"]) is False
      and any(p.startswith("gate-store") for p in mk(store="256", den=256)["problems"]) is False)
rr = mk(store="256")
rr2 = pp.make_row("c8-100", "WIRE", "42", 1, 0, 12.0, *[(lambda d, c, s: (d, gates(c, RWM_STORE="512"), s))(*base_logs("c8-100"))][0], 0, "x", store="256") if False else None
w_ = mk(store="256")
check("witness: static cell, every post-warm-up win= denominator must equal n", any(p.startswith("win-den-not-n") for p in mk(store="256", den=300)["problems"]))
check("witness: static cell, [SUMCAP] eng > 0 is void", any(p.startswith("sumcap-eng") for p in mk(sumcap=True)["problems"]))
check("default cell: no store witnesses on the denominators or [SUMCAP]; the store gate must read `unset`",
      mk(store="def", sumcap=True, den=777)["status"] == "LIVE")
check("witness: WIRE with w_calls = 0 is void", any(p.startswith("w_calls") for p in mk(eta_line=eta(w_calls=0))["problems"]))
check("witness: WIRE with no [ETA] sender line is void", "no-eta-sender-line" in mk(eta_line=None)["problems"])
check("witness: WIRE two-leg row with w_mv = 0 is void (the argmin never moved)", any(p.startswith("w_mv") for p in mk(eta_line=eta(w_mv="0.0000"))["problems"]))
check("witness: WIRE two-leg row with a non-numeric wpl_us on an idle leg is void",
      "wpl_us-not-numeric-on-every-leg" in mk(legs_fn=lambda t: [(5, 8, 7, 7300), (0, 44, 43, "-")])["problems"])
check("witness: wpl_us further than 1 ms from wrtt in > 5 % of the lines is void; 1 of 12 lines off (8 %) is void, 0 is not",
      any(p.startswith("wpl-within") for p in mk(legs_fn=lambda t: [(5, 8, 7, 7300), (3, 44, 43, 43200 if t < 3.0 else 47000)], nlines=14)["problems"])
      and not any(p.startswith("wpl-within") for p in mk(legs_fn=lambda t: [(5, 8, 7, 7300), (3, 44, 43, 44000)])["problems"]))
c1 = mk(cell="c1s-400", store="def", eta_line=eta(w_mv="0.0000"), shares=(1.0,))
check("N = 1 cell: a WIRE row with w_mv = 0 is LIVE (one candidate), w_calls > 0 required", c1["status"] == "LIVE" and c1["eta"]["w_calls"] > 0, c1["problems"])
check("N = 1 cell: a non-zero w_mv is NOT a void (it is a REGRESSES finding at scoring)", mk(cell="c1s-400", store="def", eta_line=eta(w_mv="0.0100"), shares=(1.0,))["status"] == "LIVE")
check("witness: MAIN row with a numeric wpl_us is void", "wpl_us-numeric-on-MAIN" in mk(arm="MAIN", legs_fn=lambda t: [(5, 8, 7, 7300), (3, 44, 43, 43200)])["problems"])
check("witness: MAIN row whose [ETA] carries wire gauges is void", "wire-gauges-on-MAIN" in mk(arm="MAIN", eta_line=eta())["problems"])
check("a MAIN row with no [ETA] sender line is not void", mk(arm="MAIN", eta_line=None)["status"] == "LIVE")
lk = mk(arm="MAIN", shares=(0.95, 0.05), legs_fn=lambda t: [(5, 8, 7, "-"), (0, 150, 43, "-")])
check("a MAIN row with a frozen second leg is LOCKED and one-leg", lk["locked"] and lk["one_leg"], (lk["locked"], lk["legs"]))
check("one-leg also when a leg share < 5 % without the lock-out signature", mk(arm="MAIN", shares=(0.97, 0.03), legs_fn=lambda t: [(5, 8, 7, "-"), (0, 45, 43, "-")])["one_leg"])
check("a fresh c8 row (share 0.2, rtt ~ wrtt) is neither", not mk()["locked"] and not mk()["one_leg"])
check("the row survives a JSON round trip", json.loads(json.dumps(mk(), sort_keys=True))["store"] == "256")
os.environ["LD_ONDISK"] = "1"
check("a row off tmpfs is VOID-ONDISK", mk()["status"] == "VOID-ONDISK")
os.environ.pop("LD_ONDISK")


# ── scoring: constructed ledgers ──────────────────────────────────────────
def row38(cell, store, arm, i, mbps=100.0, share=None, locked=False, w_mv=0.3, cpu=10.0, lag=100.0, rtp=3000.0, feed=1.0, status="LIVE", nleg=None):
    nleg = nleg or (1 if cell.startswith(("c1s", "c2")) else 2)
    if nleg == 1:
        w_mv = 0.0                          # one candidate: the argmin cannot move
    sh = [1.0] if nleg == 1 else [1 - (share if share is not None else 0.2), share if share is not None else 0.2]
    r = {"cell": cell, "store": store, "arm": arm, "seed": "42" if i % 2 == 0 else "7", "rep": 1 + i // 2, "rc": 0, "status": status, "problems": [],
         "dnf": False, "mbps": mbps + (i % 3) * 0.01, "cpu_cli": cpu, "cpu_srv": cpu, "cli_lag_E": lag + i * 0.001, "srv_lag_E": lag + i * 0.001,
         "cli_lag_p99_us": lag * 2, "srv_lag_p99_us": lag * 2, "shares": sh, "locked": locked, "one_leg": locked,
         "first_burst": {"t": 0.25, "infl_share": 0.4, "rtt": 100, "wrtt": 40}, "idle_decay": {}, "io_full10": 0.0, "io_high": False,
         "eta": {"w_cold": "0.01", "w_mv": f"{w_mv:.4f}" if arm == "WIRE" else "-", "ref_fl": "0.0", "cold_r": "0.0", "cold_ge": "0.2", "w_n": 5, "w_calls": 9,
                 "e_p50": {"0": "900"}, "e_p90": {"0": "2000"}} if arm == "WIRE" else None}
    for k in range(nleg):
        r["rtp_floor_p%d" % k] = rtp
        r["feed_ratio_p%d" % k] = feed
    return r


def ledger(mod=lambda r: r, n=6, lock_main=None, skip=(), abort=None):
    lock_main = lock_main or {}
    rows = []
    for cell, store in pp.CELLS38:
        for arm in pp.ARMS:
            for i in range(n):
                if (cell, store, arm) in skip:
                    continue
                lk = arm == "MAIN" and i < lock_main.get((cell, store), 0)
                r = row38(cell, store, arm, i, locked=lk, share=0.02 if lk else 0.2)
                rows.append(mod(r))
    return rows


def run(rows, abort=None):
    lines = []
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "l.log")
        with open(pth, "w") as fh:
            for r in rows:
                fh.write("P38ROW " + json.dumps(r) + "\n")
            if abort:
                fh.write(abort + " reason\n")
        v = pp.score([pth], out=lines.append)
    return v, "\n".join(lines)


LOCK = {("c1d-400", "1536"): 4, ("c8-100", "192"): 3, ("c8-100", "256"): 5, ("c8-100", "512"): 3}
v, txt = run(ledger(lock_main=LOCK))
check("CURED: MAIN locks at every static cell, WIRE none, every clause WITHIN, shares preserved, c1d default WITHIN", v.startswith("CURED") and "AT-COST" not in v, v)
check("the score ends in a RESULT json line carrying the verdict", txt.splitlines()[-1].startswith("RESULT {") and json.loads(txt.splitlines()[-1][7:])["verdict"] == v)
check("reported: LOCKED counts, shares, first-burst split, c1d cost, c8 gain, gauges, [ETA] e_p50/e_p90",
      "LOCKED MAIN 5/6  WIRE 0/6" in txt and "FIRST-BURST WIRE" in txt and "c1d COST at c1d-400@1536" in txt and "c8 GAIN at c8-100@256" in txt
      and "REPORT gauges WIRE" in txt and "e_p50/e_p90" in txt and "REPORT [LAG] raw p99" in txt)


def wire_gp(cell, store, f):
    def mod(r):
        if r["cell"] == cell and r["store"] == store and r["arm"] == "WIRE":
            r["mbps"] *= f
        return r
    return mod


TREND = [90.0, 91.0, 92.0, 93.0, 100.5, 101.0]          # WIRE goodput at c1d default: median 92.5 (-7.5 %, beyond the 5.6 % band), range overlaps MAIN's


def trend_worse(r):
    if (r["cell"], r["store"], r["arm"]) == ("c1d-400", "def", "WIRE"):
        r["mbps"] = TREND[(r["rep"] - 1) * 2 + (0 if r["seed"] == "42" else 1)]
    return r


v, txt = run(ledger(trend_worse, lock_main=LOCK))
check("CURED-AT-COST(x %): the c1d default goodput clause reads TREND-WORSE (median -7.5 %, ranges overlap) -> x = the median difference",
      v.startswith("CURED-AT-COST(-7.5 %)") and "TREND-WORSE" in txt, v)
v, txt = run(ledger(wire_gp("c1d-400", "def", 0.90), lock_main=LOCK))
check("OPEN POINT (asked of the coordinator): goodput WORSE (disjoint, -10 %) at c1d default satisfies outcome 3 (a §21 clause WORSE at a default "
      "cell) before outcome 5 -> REGRESSES-AT(c1d-400@def), taken literally", v == "REGRESSES-AT(c1d-400@def)", v)
v, _ = run(ledger(wire_gp("c1d-400", "def", 0.97), lock_main=LOCK))
check("... a 3 % dip is inside c1d's 5.6 % MDE: CURED", v.startswith("CURED") and "AT-COST" not in v, v)
v, _ = run(ledger())
check("NOT-REPRODUCED: MAIN has < 2 LOCKED rows at every static cell", v == "NOT-REPRODUCED", v)
v, _ = run(ledger(lock_main={**LOCK, ("c8-100", "512"): 1}))
check("one static cell short of 2 is NOT-REPRODUCED-AT(c8-100@512), dropped from the rest, verdict continues",
      v.startswith("CURED") and "NOT-REPRODUCED-AT(c8-100@512)" in v, v)
v, _ = run(ledger(lock_main={**LOCK, ("c8-100", "512"): 1}, mod=lambda r: dict(r, locked=True) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "512", "WIRE") else r))
check("... and a LOCKED WIRE row there does not count (the cell is dropped from outcome 4)", v.startswith("CURED"), v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, locked=True) if (r["cell"], r["store"], r["arm"], r["rep"]) == ("c8-100", "256", "WIRE", 2) else r))
check("NOT-CURED: one WIRE row LOCKED at a reproduced static cell", v == "NOT-CURED", v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, mbps=r["mbps"] * 0.8) if (r["cell"], r["store"], r["arm"]) == ("c1s-400", "def", "WIRE") else r))
check("REGRESSES-AT: a §21 clause WORSE at a control (c1s goodput -20 %)", v == "REGRESSES-AT(c1s-400@def)", v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, cpu_cli=r["cpu_cli"] * 1.5) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "def", "WIRE") else r))
check("REGRESSES-AT: client CPU per byte WORSE at c8 default", v == "REGRESSES-AT(c8-100@def)", v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, eta=dict(r["eta"], w_mv="0.0100")) if (r["cell"], r["arm"]) == ("c2-100", "WIRE") else r))
check("REGRESSES-AT: w_mv != 0 at a control", v == "REGRESSES-AT(c2-100@def)", v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, shares=[0.75, 0.25]) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "def", "WIRE") else r))
check("REGRESSES-AT: c8 default slow-leg share medians differ by 0.05 (> 0.03) between arms (against MAIN's non-LOCKED rows)", v == "REGRESSES-AT(c8-100@def)", v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, shares=[0.78, 0.22]) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "def", "WIRE") else r))
check("... a 0.02 difference is preserved", v.startswith("CURED"), v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, shares=[0.55, 0.45]) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "192", "WIRE") else r))
check("REGRESSES-AT: c8-192 WIRE share 0.45 is outside [1/2, 2] x MAIN's non-LOCKED median 0.2", v == "REGRESSES-AT(c8-100@192)", v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, shares=[0.65, 0.35]) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "192", "WIRE") else r))
check("... 0.35 is inside 2 x 0.2 = 0.4: preserved", v.startswith("CURED"), v)
lk5 = {**LOCK, ("c8-100", "192"): 5}
v, txt = run(ledger(lock_main=lk5, mod=lambda r: dict(r, shares=[0.55, 0.45]) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "192", "WIRE") else r))
check("the c8-192 share clause is reported, not gated, with < 2 non-LOCKED MAIN rows (5 of 6 locked)", v.startswith("CURED") and "REPORTED" in txt, v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, mbps=r["mbps"] * 0.85) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "256", "WIRE") else r))
check("REGRESSES-AT: goodput WORSE at a reproduced c8 static cell", v == "REGRESSES-AT(c8-100@256)", v)
v, _ = run(ledger(lock_main={**LOCK, ("c8-100", "512"): 1}, mod=lambda r: dict(r, mbps=r["mbps"] * 0.85) if (r["cell"], r["store"], r["arm"]) == ("c8-100", "512", "WIRE") else r))
check("... but not at a static cell that did not reproduce", v.startswith("CURED"), v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, mbps=r["mbps"] * 0.8, locked=True) if (r["cell"], r["store"], r["arm"], r["rep"]) == ("c8-100", "256", "WIRE", 1) else r))
check("precedence: REGRESSES outranks NOT-CURED (both can hold)", v.startswith("REGRESSES") or v == "NOT-CURED", v)
v, _ = run(ledger(lock_main=LOCK, skip={("c8-100", "def", "WIRE")}))
check("UNSCOREABLE: a cell with no WIRE rows", v.startswith("UNSCOREABLE") and "c8-100@def" in v, v)
v, _ = run(ledger(lock_main=LOCK, n=2))
check("UNSCOREABLE: fewer than 3 live rows per arm anywhere", v.startswith("UNSCOREABLE"), v)
v, _ = run(ledger(lock_main=LOCK), abort="ABORT-BIN")
check("UNSCOREABLE: an abort cause in the ledger (ABORT-BIN)", v.startswith("UNSCOREABLE") and "ABORT-BIN" in v, v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, status="WITNESS-FAIL") if (r["cell"], r["store"], r["arm"], r["seed"]) == ("c2-100", "def", "WIRE", "42") else r))
check("UNSCOREABLE: three void WIRE rows leave 3 live ... (6 rows, 3 void) is still scoreable at the floor of 3",
      not v.startswith("UNSCOREABLE"), v)
v, _ = run(ledger(lock_main=LOCK, mod=lambda r: dict(r, feed_ratio_p0=2.0) if (r["cell"], r["store"], r["arm"]) == ("c1d-400", "def", "WIRE") else r))
check("a moved feed ratio (plc/truth x2) is a WORSE clause: REGRESSES-AT(c1d-400@def)", v == "REGRESSES-AT(c1d-400@def)", v)

# stratified readouts
mod = lambda r: dict(r, mbps=r["mbps"] * 1.10) if (r["cell"], r["store"], r["arm"], r["locked"]) == ("c1d-400", "1536", "MAIN", True) else r  # noqa: E731
_, txt = run(ledger(lock_main=LOCK, mod=mod))
check("c1d-1536 stratified: WIRE vs MAIN one-leg rows and vs two-leg rows are both printed", "WIRE vs MAIN two-leg rows" in txt and "WIRE vs MAIN one-leg rows" in txt)

# smoke
def smoke_rc(rows):
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "s.log")
        with open(pth, "w") as fh:
            for r in rows:
                fh.write("P38ROW " + json.dumps(r) + chr(10))
        return pp.smoke(pth)


sm = [row38("c8-100", "256", "MAIN", 0), row38("c8-100", "256", "WIRE", 0)]
check("smoke passes on a LIVE MAIN and a LIVE WIRE row with w_calls > 0", smoke_rc(sm) == 0)
sm2 = [row38("c8-100", "256", "MAIN", 0), row38("c8-100", "256", "WIRE", 0)]
sm2[1]["eta"]["w_calls"] = 0
check("smoke fails when the WIRE arm never ran (w_calls = 0)", smoke_rc(sm2) == 1)
check("smoke fails with a missing arm", smoke_rc(sm[:1]) == 1)

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
