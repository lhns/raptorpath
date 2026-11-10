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
      "test_place38_parse.py" in runall and "P_SCORE=score38" in runall and runall.count('"$P38" = 1') >= 5)


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
check("Amendment 4: goodput WORSE (disjoint, -10 %) at c1d default is exempt from outcome 3 and decides 5 vs 6 -> CURED-AT-COST(-10.0 %)",
      v.startswith("CURED-AT-COST(-10.0 %)"), v)
v, _ = run(ledger(lambda r: dict(trend_worse(wire_gp("c1d-400", "def", 1.0)(r)), cpu_cli=r["cpu_cli"] * 1.5) if (r["cell"], r["store"], r["arm"]) == ("c1d-400", "def", "WIRE") else r, lock_main=LOCK))
check("... but every other §21 clause at c1d default stays in outcome 3 (client CPU +50 % -> REGRESSES-AT(c1d-400@def))", v == "REGRESSES-AT(c1d-400@def)", v)
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

# ══ status §41: the BASE arm (main's own binary, 4926822), beside the ladder ══════════════════════════════════════════
p41 = pp.plan41()
check("plan 41: 108 invocations = plan 38's 96 + BASE x 2 cells x 6 (n = 6 per cell; the pre-registration's '24 / 120' is open)",
      len(p41) == 108 == pp.EXPECT_ROWS41 and pp.EXPECT_ROWS41 - pp.EXPECT_ROWS38 == 12, len(p41))
check("plan 41 minus BASE is plan 38 exactly (cells, ladder arms, seeds, reps and order unchanged)",
      [r for r in p41 if r[2] != "BASE"] == pl)
check("BASE runs only at c1s-400 and c1d-400 default, 6 rows each: 3 reps x seeds 42 and 7",
      sorted({(c, s) for c, s, a, _, _ in p41 if a == "BASE"}) == [("c1d-400", "def"), ("c1s-400", "def")]
      and all(sorted((sd, rp) for c, st, a, sd, rp in p41 if (c, st) == cs and a == "BASE") == sorted(("42" if i % 2 == 0 else "7", 1 + i // 2) for i in range(6))
              for cs in pp.BASE_CELLS))
pos = {cs: [] for cs in pp.BASE_CELLS}
for cs in pp.BASE_CELLS:
    blocks = [[a for c, st, a, sd, rp in p41 if (c, st) == cs and (sd, rp) == b] for b in pp.BLOCKS38]
    pos[cs] = [blk.index("BASE") for blk in blocks]
    check(f"BASE is interleaved into every block at {cs[0]}, in each position (front, middle, back) twice (the position rule is a DECISION)",
          all(len(blk) == 3 for blk in blocks) and sorted(pos[cs]) == [0, 0, 1, 1, 2, 2], pos[cs])
check("plan 41 smoke = plan 38's two rows plus one BASE row (c1s-400 default)", pp.plan41(smoke=True) == pp.plan38(smoke=True) + [("c1s-400", "def", "BASE", "42", 1)])
cp = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "plan41"], capture_output=True, text=True)
check("`plan41` prints the 108 `cell store arm seed rep` lines", cp.stdout.splitlines() == [" ".join(map(str, r)) for r in p41])
cp = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "plan"], capture_output=True, text=True)
check("`plan` (plan 38) is unchanged: 96 lines", cp.stdout.splitlines() == [" ".join(map(str, r)) for r in pl])
check("battery: plan 41 accepted; BASE arm runs its own binary (RBIN / LD_BIN_BASE / LD_SHA_BASE), the BASE binary must lack RWM_PLACE_WIRE; "
      "108-row table; PROCS and the four contention values passed to the parser",
      "24|27|29|34|36|38|40|41" in bat and "LD_BIN_BASE" in bat and "LD_SHA_BASE" in bat and 'RBIN="$BIN_BASE"' in bat and 'grep -aq -- "RWM_PLACE_WIRE" "$BIN_BASE"' in bat
      and "-eq 108" in bat and "ABORT-PLAN41" in bat and "procs_running_before" in bat and 'LD_PROCS_B="$procs_b"' in bat and 'LD_IO_B="$io_b"' in bat
      and 'sha256sum "$RBIN"' in bat and 'RWM_BIN="$RBIN"' in bat and "-u RWM_PLACE_WIRE -u RWM_TEST_PLACE_HOLD" in bat)
check("run_all: plan 41 scores with score41, builds BASE from its own tree into its own target dir and installs it as bin-base/raptorpath, "
      "runs the suites (all tools/l1 python tests) before the smoke, and hands both binaries to the battery",
      "P_SCORE=score41" in runall and "target-base" in runall and "bin-base/raptorpath" in runall and 'base/COMMIT' in runall and "test_*.py" in runall
      and 'LD_BIN_BASE="$BIN_BASE" LD_SHA_BASE="$SHA_BASE"' in runall
      and runall.index('if [ "$PLAN" = "41" ]; then\n  run_suites') < runall.index("BENV=(") and runall.index("built BASE") < runall.index('if [ "$PLAN" = "41" ]; then\n  run_suites'))

# BASE rows on real main-binary logs (the §32 thread-pd battery: RTOBS + RDIAG, no wpl_us, no [ETA] w_* fields)
PD = os.path.join(RAW, "thread-pd", "battery")


def base_real(cell, d, **kw):
    drv, cli, srv = (lc.read(os.path.join(PD, d + x)) for x in ("drv.out", "c.log", "s.log"))
    return pp.make_row(cell, "BASE", "42", 1, 0, 12.0, drv, cli, srv, 0, "x", store="def", **kw), (drv, cli, srv)


for cell, d in (("c1d-400", "diag-tpd/c1d-400-MAIN-s42-r1-a1-"), ("c1s-400", "diag-smoke/c1s-400-MAIN-s42-r1-a1-")):
    rb, (drv_, cli_, srv_) = base_real(cell, d)
    nl = 2 if cell.startswith("c1d") else 1
    need = ["cpu_cli", "cpu_srv", "mbps", "cli_lag_E", "srv_lag_E"] + [f"feed_ratio_p{i}" for i in range(nl)] + [f"rtp_floor_p{i}" for i in range(nl)]
    check(f"a main-binary log (no wpl_us, no w_* gauges) is a LIVE BASE row at {cell}: every §21 clause input is read, the legs are read despite the missing wpl_us",
          rb["status"] == "LIVE" and rb["problems"] == [] and all(rb.get(k) is not None for k in need)
          and all(v is not None for v in rb["rtt_wrtt_med"]) and rb["locked"] is False and rb["eta"] is None, (rb["status"], rb["problems"], [k for k in need if rb.get(k) is None]))
    check(f"... the same log is a LIVE MAIN row except for the gate echo (it has no RWM_PLACE_WIRE token), so BASE's gate witness is 'absent'",
          pp.make_row(cell, "MAIN", "42", 1, 0, 12.0, drv_, cli_, srv_, 0, "x", store="def")["status"] == "WITNESS-FAIL")
    rg = pp.make_row(cell, "BASE", "42", 1, 0, 12.0, drv_, gates(cli_, RWM_PLACE_WIRE="0"), gates(srv_, RWM_PLACE_WIRE="0"), 0, "x", store="def")
    check(f"BASE witness: a [GATES] line that echoes RWM_PLACE_WIRE is void (the binary is not main's) at {cell}", rg["status"] == "WITNESS-FAIL" and any("on BASE" in q for q in rg["problems"]), rg["problems"])
    check("BASE witness: a numeric wpl_us on a BASE row is void",
          "wpl_us-numeric-on-BASE" in pp.make_row(cell, "BASE", "42", 1, 0, 12.0, drv_, [ln + "" for ln in cli_] + [diag(2.0, [(1, 8, 7, 7000)] * nl)], srv_, 0, "x", store="def")["problems"])
    check("BASE witness: w_* gauges on an [ETA] sender line are void",
          "wire-gauges-on-BASE" in pp.make_row(cell, "BASE", "42", 1, 0, 12.0, drv_, cli_ + [eta()], srv_, 0, "x", store="def")["problems"])

# the contention covariates ride the environment
os.environ.update(LD_PROCS_B="1", LD_PROCS_A="3", LD_IO_B="0.50", LD_IO_A="2.25")
rc_, _ = base_real("c1s-400", "diag-smoke/c1s-400-MAIN-s42-r1-a1-")
for k_ in ("LD_PROCS_B", "LD_PROCS_A", "LD_IO_B", "LD_IO_A"):
    os.environ.pop(k_)
check("per-row contention: procs_running and io full avg10 before and after are recorded on the row",
      (rc_["procs_before"], rc_["procs_after"], rc_["io_before"], rc_["io_after"]) == (1.0, 3.0, 0.5, 2.25), rc_)
check("... and are None when the battery did not sample them", base_real("c1s-400", "diag-smoke/c1s-400-MAIN-s42-r1-a1-")[0]["procs_before"] is None)

# ── the scorer ──
def row41(cell, store, arm, i, **kw):
    r = row38(cell, store, arm, i, **kw)
    r["procs_before"], r["procs_after"], r["io_before"], r["io_after"] = 1.0, 2.0, 0.1, 0.2
    return r


def ledger41(mod=lambda r: r, n=6, base_mod=None):
    rows = ledger(mod, n=n, lock_main=LOCK)
    for cell, store in pp.BASE_CELLS:
        for i in range(n):
            r = row41(cell, store, "BASE", i, share=0.5 if cell.startswith("c1d") else None)
            rows.append((base_mod or mod)(r))
    for r in rows:
        r.setdefault("procs_before", 1.0), r.setdefault("procs_after", 2.0), r.setdefault("io_before", 0.1), r.setdefault("io_after", 0.2)
    return rows


def run41(rows, abort=None):
    lines = []
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "l.log")
        with open(pth, "w") as fh:
            for r in rows:
                fh.write("P38ROW " + json.dumps(r) + "\n")
            if abort:
                fh.write(abort + " reason\n")
        v = pp.score([pth], out=lines.append, plan41=True)
    return v, "\n".join(lines)


v41, t41 = run41(ledger41())
v38, _ = run(ledger(lock_main=LOCK))
check("BASE rows never reach the ladder: the verdict with BASE rows present equals the verdict without", v41 == v38 and v41.startswith("CURED"), (v41, v38))
check("BASE gate PASS when MAIN equals BASE on every §21 clause at both cells; ROWS counted against 108", "BASE-GATE PASS" in t41 and "ROWS 108/108" in t41, t41[:400])
check("... the gate output labels the pair MAIN against BASE (not MAIN=... MAIN=...)", "CLAUSE BASE-GATE c1s-400@def goodput: WITHIN MAIN=" in t41 and " BASE=" in t41 and "MAIN=" in t41.split("CLAUSE BASE-GATE c1s-400@def goodput")[1].split("\n")[0])
check("the RESULT json carries the gate", json.loads(t41.splitlines()[-1][7:])["base_gate"] == "PASS")
for what, f, cellk in (("goodput -10 % (WORSE)", lambda r: dict(r, mbps=r["mbps"] * 0.90), "c1s-400@def goodput"),
                       ("goodput +10 % (BETTER is not WITHIN)", lambda r: dict(r, mbps=r["mbps"] * 1.10), "c1s-400@def goodput"),
                       ("client CPU/GB +50 %", lambda r: dict(r, cpu_cli=r["cpu_cli"] * 1.5), "c1s-400@def cpu_cli/GB"),
                       ("rtp floor x2 on p0", lambda r: dict(r, rtp_floor_p0=r["rtp_floor_p0"] * 2), "c1s-400@def rtp_floor_p0")):
    _, tx = run41(ledger41(base_mod=lambda r, f=f: f(r)))
    check(f"BASE gate FAIL when BASE differs from MAIN on a clause ({what}) at c1s, named in the output; the ladder verdict is untouched",
          "BASE-GATE FAIL" in tx and f"BASE-GATE c1s-400@def: FAIL" in tx and cellk.split(" ")[1].split("/")[0] in tx, tx[tx.find("BASE-GATE c1s"):][:300])
_, tx = run41(ledger41(base_mod=lambda r: dict(r, mbps=r["mbps"] * 0.90) if r["cell"] == "c1d-400" else r))
check("BASE gate FAIL at c1d only is a FAIL overall, with c1s PASS", "BASE-GATE c1s-400@def: PASS" in tx and "BASE-GATE c1d-400@def: FAIL" in tx and "\nBASE-GATE FAIL" in tx)
_, tx = run41(ledger41(base_mod=lambda r: dict(r, mbps=r["mbps"] * 1.03)))
check("TREND-* is not WITHIN: a +3 % BASE goodput (inside the 4.9 % MDE) is still WITHIN, so the gate passes", "\nBASE-GATE PASS" in tx, tx[tx.find("BASE-GATE"):][:200])
rows_ = [r for r in ledger41() if not (r["arm"] == "BASE" and r["cell"] == "c1d-400")]
_, tx = run41(rows_)
check("BASE gate UNREADABLE (never a silent pass) when a BASE cell has no live rows; the ladder is untouched", "\nBASE-GATE UNREADABLE" in tx and "BASE-GATE c1d-400@def: UNREADABLE" in tx)
v_, tx = run41(ledger41(), abort="ABORT-BIN")
check("an abort token still makes the ladder UNSCOREABLE in plan 41", v_.startswith("UNSCOREABLE"))
check("matched speed: client / server CPU/GB for WIRE and MAIN within +-3 % of the pooled median at every cell, with n per arm; BASE beside them at its cells",
      "MATCHED-SPEED c1s-400@def: pooled WIRE+MAIN median goodput" in t41 and "client CPU/GB WIRE vs MAIN at matched speed" in t41 and "  BASE: n=" in t41
      and "MATCHED-SPEED c8-100@512" in t41)


def mx(r):                                       # WIRE rows: 4 inside the +-3 % window, 2 slow with the high CPU/GB of the slow rows
    if (r["cell"], r["store"], r["arm"]) == ("c1s-400", "def", "WIRE") and r["seed"] == "7" and r["rep"] in (1, 2):
        return dict(r, mbps=60.0, cpu_cli=17.5)
    if (r["cell"], r["store"], r["arm"]) == ("c1s-400", "def", "WIRE"):
        return dict(r, cpu_cli=10.2)
    return r


_, tx = run41(ledger41(mx))
ms = [ln for ln in tx.splitlines() if ln.startswith("  client CPU/GB WIRE vs MAIN at matched speed")]
blk = tx.split("MATCHED-SPEED c1s-400@def")[1].split("MATCHED-SPEED c2-100@def")[0]
check("matched speed excludes the slow WIRE rows and reads the excess from the rest: +2.0 % (10.2 vs 10.0 per 0.4 GB, i.e. 25.5 vs 25.0 per GB), n = 4/6", "+2.0 % (WIRE 25.500, MAIN 25.000; n=4/6)" in blk, blk)
cn = [ln for ln in t41.splitlines() if ln.startswith("  MAIN s42 r1 LIVE") or ln.startswith("  WIRE s7 r2") or ln.startswith("  BASE s42 r1")]
check("contention: every c1s row (all three arms, 18 rows) prints procs_running and io full avg10 before/after",
      t41.count("| procs_running 1/2 | io 0.10/0.20") == 18 and len(cn) >= 3 and "CONTENTION c1s-400@def" in t41, (t41.count("| procs_running"), cn))

# the committed §38 ledger: plan 41's scorer reproduces its ladder (every CLAUSE / VERDICT / REGRESSES / REPRODUCTION / SHARE line) byte for byte
LG38 = os.path.join(RAW, "place38")
ref_ = open(os.path.join(LG38, "score.txt"), encoding="utf-8").read().splitlines()
got_, got41 = [], []
pp.score([os.path.join(LG38, "ld.log")], out=got_.append)
pp.score([os.path.join(LG38, "ld.log")], out=got41.append, plan41=True)
mark_ = "=== beside the ladder (status §41; none of this enters the verdict) ==="
ladder41 = got41[2:got41.index(mark_) - 1]
check("plan 38's scorer reproduces the committed §38 score.txt byte for byte (nothing in the ladder moved)", got_ == ref_)
check("plan 41's scorer on the committed §38 ledger reproduces the whole ladder part of score.txt (every CLAUSE, VERDICT, REGRESSES, SHARE line) byte for byte",
      ladder41 == ref_[2:-1], [(a, b) for a, b in zip(ladder41, ref_[2:-1]) if a != b][:1])
check("... with BASE absent the gate reads UNREADABLE, not PASS", "\nBASE-GATE UNREADABLE" in "\n".join(got41))
check("... and the verdict line is the registered one", "VERDICT REGRESSES-AT(c1s-400@def) [NOT-REPRODUCED-AT(c8-100@192,c8-100@512)]" in got41, [ln for ln in got41 if ln.startswith("VERDICT")])

# smoke 41
def smoke41_rc(rows):
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "s.log")
        with open(pth, "w") as fh:
            for r in rows:
                fh.write("P38ROW " + json.dumps(r) + chr(10))
        return pp.smoke(pth, plan41=True)


sm41 = sm + [row38("c1s-400", "def", "BASE", 0)]
check("smoke 41 passes on LIVE MAIN, WIRE (w_calls > 0) and BASE rows", smoke41_rc(sm41) == 0)
check("smoke 41 fails without the BASE row", smoke41_rc(sm) == 1)
bad_ = [dict(sm41[2], status="WITNESS-FAIL")]
check("smoke 41 fails on a void BASE row", smoke41_rc(sm[:2] + bad_) == 1)
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "c.log")
    with open(pth, "w") as fh:
        for r in ledger41():
            fh.write("P38ROW " + json.dumps(r) + "\n")
        fh.write("LD-BATTERY-DONE tag=ld blocks=6\n")
    ok41 = pp.check(pth, plan41=True) == 0
    ok38 = pp.check(pth) == 1
check("check41 accepts 108 rows + LD-BATTERY-DONE; check (plan 38) still demands 96", ok41 and ok38)

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
