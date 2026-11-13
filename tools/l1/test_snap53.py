#!/usr/bin/env python3
"""Tests for the status 53 harness (plan 53, tools/l1/snap53_score.py + the place38_parse plan-53 parts + tail52_score's plan-53 fields): the parsers on lines copied from the
committed s52 logs (CAND s42 r1: absolute values) and on [SNAPV] lines built from the engine's pinned format, E, the liveness rules and the rehearsal criteria (each
with its red case first), the plan table, the arm witnesses, the rank-sum report, the shell side. Run: python3 test_snap53.py

The end-to-end path through loopdiag_run_all.sh runs in test_stubrun.py; this file needs no ledger fixture (the lines below are copied from
docs/l1-raw/s52/rep52-diag-ld.tgz, row c8-100@def-CAND-s42-r1, and from dup-diag's snapv_line_format_is_pinned)."""
import copy
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
_BASE_SHA_AT_START = os.environ.get("LD_BASE_SHA")
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import snap53_score as sn  # noqa: E402
import tail52_score as t52  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


# ── lines copied from the s52 CAND s42-r1 row (client = sender, server = receiver) ──
DIAG_SRC = ("[DIAG] t=10.0s win=446/2212 paused=0% good=141.7Mbit src=1000sym/s cod=48sym/s cum=300000/2260/299000 cwnd=14927 infl=169 np=2 np_act=2 rtt=366.6ms bdp100=3819sym "
            "sweeps=417 retx=9000 gapdrop=1474 nbud=10  mpr[rep=9000 seqs=3000000 fired=9000 y=2000 r=7000 fast=2000 coal=3557 supp=1728282/0/3781864 stale=9982 plost=0 taper=0 "
            "age=457ms pf=0/3831487/10000 fp/on p0:6000/2000 p1:2000/6000] wnd2=63/383")
DIAG_END = ("[DIAG] t=44.6s win=446/2212 paused=0% good=141.7Mbit ackrate_ewma=0sym/s eff_pace=0sym/s src=0sym/s cod=48sym/s cum=507616/2260/497422 sidle=31248ms/5918/mx163ms cwnd=14927 "
            "infl=169 np=2 np_act=2 rtt=366.6ms bdp100=3819sym sweeps=417 retx=16541 gapdrop=1474 nbud=10  mpr[rep=16711 seqs=5536669 fired=16541 y=4968 r=11573 fast=4968 coal=3557 "
            "supp=1728282/0/3781864 stale=9982 plost=0 taper=0 age=457ms pf=0/3831487/10000 fp/on p0:12105/4622 p1:4436/11919] wnd2=63/383")
HOLD0 = ("[HOLD] site=sender path=0 gen=0 samp_n=1000 fed=257845 obs_p50_us=2774 obs_p90_us=406327 obs_p99_us=663145 evals=12445 hn_n=249122 hn_p50_us=6144 hn_p90_us=98304 hy_n=1689 "
         "hy_p50_us=5120 hy_p90_us=10240 cx_n=7034 cx_p50_us=294912 cx_p90_us=720896 sp_n=237406 xp_n=20358 up_n=81 xp_frac=0.0790 age_n=255723 age_ripe=6325 ripe_frac=0.0247 "
         "age_p50_us=327680 age_p90_us=393216 thr_p50_us=393216 cx_leg=0:457,1:6577 fa_class=0.0625")
HOLD1 = ("[HOLD] site=sender path=1 gen=0 samp_n=1000 fed=83255 obs_p50_us=5184 obs_p90_us=60607 obs_p99_us=678042 evals=4118 hn_n=79369 hn_p50_us=6144 hn_p90_us=73728 hy_n=819 "
         "hy_p50_us=2816 hy_p90_us=7680 cx_n=3067 cx_p50_us=327680 cx_p90_us=851968 sp_n=1685 xp_n=81518 up_n=52 xp_frac=0.9797 age_n=82737 age_ripe=727 ripe_frac=0.0088 "
         "age_p50_us=360448 age_p90_us=720896 thr_p50_us=458752 cx_leg=0:3066,1:1 fa_class=0.0625")
FCAUSE = "[FCAUSE] gen=0 n=16563 timer=3 gap_data=16543 gap_refresh=17 other=0 timer_frac=0.0002 gap_frac=0.9998 fired=16563 unattr=0 fa_class=0.0625"
RX = "[C8CONV-R] p0:fst=420084 dup=1394 lead=8297 unb=175/10547ms p1:fst=80472 dup=1754 lead=6495 unb=413/32297ms"
SUCC_RUN = "[SUCC] gen=0 det=150000 res=149000 orig_n=1 rep_n=1"                          # a running value earlier in the run: never read
SUCC_FINAL = ("[SUCC] gen=0 det=206359 res=206345 orig_n=204896 orig_p50_us=114688 orig_p90_us=327680 orig_p99_us=851968 orig_mx_us=1978954 orig_mean_us=167492 rep_n=1449 "
              "rep_p50_us=327680 rep_p90_us=851968 rep_p99_us=1179648 rep_mx_us=1898651 rep_mean_us=397039 aban_n=14 aban_p50_us=90112 aban_p90_us=90112 aban_p99_us=90112 "
              "aban_mx_us=91166 aban_mean_us=91166 open=0 over=0 orig_frac=0.9930 cross_us=- dump=0/0 sp_n=6037 xp_n=200308 xp_frac=0.9707 sp_p50_us=491520 sp_p90_us=983040 "
              "xp_p50_us=106496 xp_p90_us=294912 rtx_n=12874 final=1")
SUCC_CLIENT_ZERO = "[SUCC] gen=0 det=0 res=0 orig_n=0 orig_p50_us=- rep_n=0 final=1"      # the client's [SUCC] lines are all zeros: never read
# [SNAPV]: dup-diag's pinned example (snapv_line_format_is_pinned, net/tests.rs), and a c8-shaped one
SNAPV_PIN = "[SNAPV] site=sender p0:rep=1 stale=0 pt_st=0 tt_st=0 p1:rep=2 stale=1 pt_st=2 tt_st=1 p-:rep=1 stale=0 pt_st=0 tt_st=0 fa_class=0.0625"


def snapv_line(p0=(9000, 0, 0, 40), p1=(1000, 900, 2500, 300), tail=(50, 0, 0, 0)):
    f = lambda i, v: f" p{i}:rep={v[0]} stale={v[1]} pt_st={v[2]} tt_st={v[3]}"  # noqa: E731
    return "[SNAPV] site=sender" + f(0, p0) + f(1, p1) + f("-", tail) + " fa_class=0.0625"


def gates(snapver="0", wdiv=None):
    return "[GATES] RWM_UNIFIED=1 RWM_RECOV_SP=0" + (f" RWM_RECOV_SNAPVER={snapver}" if snapver is not None else "") + " RWM_PFRAC=0" + (f" RWM_PLACE_WDIV={wdiv}" if wdiv else "")


def logs(snap=None, snapver="0", wdiv=None, drop=(), extra_cli=()):
    cli = [gates(snapver, wdiv), DIAG_SRC, DIAG_END, HOLD0, HOLD1, FCAUSE, SUCC_CLIENT_ZERO] + list(extra_cli)
    if snap is not False:
        cli.append(snap or snapv_line())
    srv = [gates(snapver, wdiv), RX, SUCC_RUN, SUCC_FINAL]
    return [x for x in cli if x not in drop], srv


DRV = ['{"summary":true,"median_s":44.5,"mean_mbps":107.2}', "CPU: CPUSRV=57.5s CPUCLI=80.0s"]
CLI, SRV = logs()
M = sn.row_metrics(CLI, SRV, DRV, 600_000_000)

# ── the parsers: absolute values of the real row ──
check("[SNAPV]: the pinned line parses per leg (p0, p1 and the tail-sweep p-); a line with no counts reads None; the LAST sender line wins",
      sn.snapv([SNAPV_PIN]) == {"0": {"rep": 1, "stale": 0, "pt_st": 0, "tt_st": 0}, "1": {"rep": 2, "stale": 1, "pt_st": 2, "tt_st": 1}, "-": {"rep": 1, "stale": 0, "pt_st": 0, "tt_st": 0}}
      and sn.snapv(["[DIAG] t=1s"]) is None and sn.snapv([snapv_line(), SNAPV_PIN])["1"]["rep"] == 2
      and sn.snapv([SNAPV_PIN.replace("site=sender", "site=receiver")]) is None, sn.snapv([SNAPV_PIN]))
src = os.path.join(HERE, "..", "..", "raptorpath", "src", "net", "tests.rs")
if os.path.exists(src):
    tx = open(src, encoding="utf-8").read()
    if "fn snapv_line_format_is_pinned" not in tx:
        print("SKIP the engine tree has no snapv_line_format_is_pinned (harness54 is main's engine; cand54 / levb54 carry it)")
    else:
      check("the parser's pinned example is the engine's own: dup-diag's snapv_line_format_is_pinned carries the same per-leg text",
          "fn snapv_line_format_is_pinned" in tx and all(x in tx for x in ("p0:rep=1 stale=0 pt_st=0 tt_st=0", "p1:rep=2 stale=1 pt_st=2 tt_st=1", "p-:rep=1 stale=0 pt_st=0 tt_st=0 fa_class=0.0625")))
check("fed, det, fed - det and cx on the real CAND s42-r1 lines: fed 257 845 + 83 255 = 341 100, det 206 359 (the server's FINAL [SUCC], not the running one, not the client's zeros), "
      "fed - det = 134 741 (inside the §52 Erratum's +114 k to +177 k), cx 7 034 + 3 067",
      (M["fed_sum"], M["det"], M["fed_minus_det"], M["cx_sum"]) == (341100, 206359, 134741, 10101), (M["fed_sum"], M["det"], M["fed_minus_det"], M["cx_sum"]))
check("det: no final=1 [SUCC] in the server log reads None (the running line is not read), so fed - det is None",
      sn.det_of([SUCC_RUN]) is None and sn.row_metrics(CLI, [RX, SUCC_RUN], DRV, 600_000_000)["fed_minus_det"] is None)
check("fire counters of the real row: fired 16 541 = y 4 968 + r 11 573, fast 4 968 (= y)", M["fires"] == {"fired": 16541, "y": 4968, "r": 11573, "fast": 4968}, M["fires"])
check("an older [DIAG] without `y= r= fast=` reads the fire counters as None, fired kept",
      t52.diag_ticks([DIAG_END.replace(" y=4968 r=11573 fast=4968", "")])[0][1]["fast"] is None and t52.diag_ticks([DIAG_END.replace(" y=4968 r=11573 fast=4968", "")])[0][1]["fired"] == 16541)
check("receiver duplicates per arrival leg: p0 1 394, p1 1 754, total 3 148 (the last [C8CONV-R] line)", M["rx_dup_legs"] == {"0": 1394, "1": 1754} and M["rx_dup"] == 3148, (M["rx_dup_legs"], M["rx_dup"]))
check("[HOLD] per path: fed / hn / hy / hy_p50 / cx / ripe_frac (ripe_frac is a FLOAT: 0.0247, not 0)",
      (M["hold"][0]["fed"], M["hold"][0]["hn_n"], M["hold"][0]["hy_n"], M["hold"][0]["hy_p50_us"], M["hold"][0]["cx_n"], M["hold"][0]["ripe_frac"]) == (257845, 249122, 1689, 5120, 7034, 0.0247)
      and M["hold"][1]["ripe_frac"] == 0.0088 and M["hold"][1]["hy_p50_us"] == 2816, M["hold"])
check("[HOLD] hy_p50_us printing `-` (no hy) reads None, not an error", t52.hold_by_path([HOLD0.replace("hy_p50_us=5120", "hy_p50_us=-")])[0]["hy_p50_us"] is None)
check("[FCAUSE]: n / timer / gap_data / gap_refresh / other / fired / unattr", M["fcause"] == {"n": 16563, "timer": 3, "gap_data": 16543, "gap_refresh": 17, "other": 0, "fired": 16563, "unattr": 0}, M["fcause"])

# ── E ──
E = sn.enrichment(sn.snapv([snapv_line()]), 4968)
check("E = (sum pt_st / fast) / (sum stale / sum rep) over the numeric legs: (2500 / 4968) / (900 / 10000) = 5.592; the tail-sweep leg p- does not enter",
      abs(E - (2500 / 4968) / (900 / 10000)) < 1e-12 and abs(E - 5.5918) < 1e-3
      and sn.enrichment(sn.snapv([snapv_line(tail=(10 ** 6, 0, 0, 0))]), 4968) == E, E)
check("E is None (undefined) without packet-threshold fires, without stale reports, without [SNAPV]",
      sn.enrichment(sn.snapv([snapv_line()]), 0) is None and sn.enrichment(sn.snapv([snapv_line(p1=(1000, 0, 0, 0))]), 4968) is None and sn.enrichment(None, 4968) is None)
check("row_metrics carries E on the real row", abs(M["E"] - E) < 1e-12, M["E"])

# ── liveness (a red case for every clause) ──
def row(arm, cell, t, status="LIVE", **kw):
    r = {"cell": cell, "arm": arm, "seed": "42", "rep": 1, "status": status, "problems": [], "t53": t, "rc": 0, "dnf": False, "summary": True, "seconds": 44.5, "mbps": 107.2,
         "wall_s": 60, "store": "def", "cpu_cli": 80.0, "cpu_srv": 57.5, "bytes": 600_000_000, "cli_lag_E": 900, "srv_lag_E": 700, "rtp_floor_p0": 2000, "rtp_floor_p1": 40000}
    r.update(kw)
    return r


def metrics(snap=None, **kw):
    c, s = logs(snap=snap)
    return sn.row_metrics(c, s, DRV, 600_000_000)


def live(arm, cell, t, **kw):
    return sn.liveness(row(arm, cell, t, **kw))


v, why = live("CAND", "c8-100", metrics())
check("liveness: the real-shaped arm-off row is LIVE (BASE / CAND rows may show stale judgements; they are the instrument's reading)", v == "LIVE" and why == [], why)
check("liveness red: no [SNAPV] line -> VOID", live("CAND", "c8-100", metrics(snap=False))[0] == "VOID" and "no-[SNAPV]" in live("CAND", "c8-100", metrics(snap=False))[1])
check("liveness red: rep = 0 on p1 -> VOID; a MISSING leg line (only p0 printed) -> VOID, never a vacuous pass",
      "rep<=0-on-p1" in live("CAND", "c8-100", metrics(snapv_line(p1=(0, 0, 0, 0))))[1]
      and "rep<=0-on-p1" in live("CAND", "c8-100", metrics("[SNAPV] site=sender p0:rep=9000 stale=0 pt_st=0 tt_st=0 fa_class=0.0625"))[1])
check("liveness red: pt_st + tt_st above the row's fires (16 541) -> VOID; exactly at the fires is still LIVE (the rule is <=)",
      live("CAND", "c8-100", metrics(snapv_line(p0=(9000, 0, 0, 0), p1=(1000, 900, 17000, 0))))[1] == ["pt_st+tt_st=17000>fires=16541"]
      and live("CAND", "c8-100", metrics(snapv_line(p0=(9000, 0, 0, 0), p1=(1000, 900, 16541, 0))))[0] == "LIVE")
ON = snapv_line(p1=(1000, 900, 0, 0), p0=(9000, 0, 0, 0))
check("liveness: a CANDS row with pt_st = tt_st = 0 on every leg, stale > 0 on p1 and fed - det (134 741) <= cx -> needs fed - det <= cx: the real row has cx 10 101, so it is VOID",
      live("CANDS", "c8-100", metrics(ON))[0] == "VOID" and any(x.startswith("fed-det=134741>cx=10101") for x in live("CANDS", "c8-100", metrics(ON))[1]), live("CANDS", "c8-100", metrics(ON)))
c_ok, s_ok = logs(snap=ON)
s_ok = [x.replace("det=206359", "det=340000") for x in s_ok]                                    # det so that fed - det = 1 100 <= cx = 10 101: the filter removed the re-stamp excess
m_ok = sn.row_metrics(c_ok, s_ok, DRV, 600_000_000)
check("liveness: CANDS with the filter's signature (no stale judgement, stale on p1, fed - det <= cx) is LIVE", sn.liveness(row("CANDS", "c8-100", m_ok)) == ("LIVE", []), sn.liveness(row("CANDS", "c8-100", m_ok)))
for what, snap, want in (("pt_st > 0 on p1", snapv_line(p1=(1000, 900, 3, 0), p0=(9000, 0, 0, 0)), "pt_st+tt_st=3-on-p1"), ("tt_st > 0 on p0", snapv_line(p1=(1000, 900, 0, 0), p0=(9000, 0, 0, 2)), "pt_st+tt_st=2-on-p0"),
                         ("stale = 0 on p1 at c8", snapv_line(p1=(1000, 0, 0, 0), p0=(9000, 0, 0, 0)), "stale=0-on-p1")):
    c, s = logs(snap=snap)
    m = sn.row_metrics(c, [x.replace("det=206359", "det=340000") for x in s], DRV, 600_000_000)
    check(f"liveness red: CANDS with {what} -> VOID ({want})", want in sn.liveness(row("CANDS", "c8-100", m))[1] and sn.liveness(row("CANDS", "c8-100", m))[0] == "VOID", sn.liveness(row("CANDS", "c8-100", m)))
    check(f"liveness red: the same line on BASES -> VOID", want in sn.liveness(row("BASES", "c8-100", m))[1])
c, s = logs(snap=snapv_line(p1=(1000, 0, 0, 0), p0=(9000, 0, 0, 0)))
m = sn.row_metrics(c, [x.replace("det=206359", "det=340000") for x in s], DRV, 600_000_000)
check("liveness: stale = 0 on p1 is a c8 clause only (c1d's legs are symmetric): a CANDS row at c1d-400 with it is LIVE", sn.liveness(row("CANDS", "c1d-400", m)) == ("LIVE", []), sn.liveness(row("CANDS", "c1d-400", m)))
check("liveness: a CANDS row whose fed - det cannot be computed (no final [SUCC]) is VOID", "fed-det-unavailable" in sn.liveness(row("CANDS", "c8-100", sn.row_metrics(c_ok, [RX, SUCC_RUN], DRV, 600_000_000)))[1])
check("liveness: a row whose status is not LIVE (an arm env missing from [GATES]) is VOID whatever its counters say",
      live("CAND", "c8-100", metrics(), status="WITNESS-FAIL")[0] == "VOID" and "status=WITNESS-FAIL" in live("CAND", "c8-100", metrics(), status="WITNESS-FAIL")[1])
check("liveness: no t53 (a parse error) is VOID and says why", sn.liveness(row("CAND", "c8-100", None, t53_error="KeyError: x")) == ("VOID", ["no-t53(KeyError: x)"]))

# ── the arm witnesses ([GATES] on both ends) ──
def wit(arm, cli_g, srv_g):
    return pp.snapver_problems(arm, [cli_g], [srv_g]) + pp.wdiv_problems(arm, [cli_g], [srv_g])

check("witness: BASE / CAND / CW0 read RWM_RECOV_SNAPVER=0 on both ends, CANDS / BASES read 1; RWM_PLACE_WDIV=0 on CW0 only",
      wit("BASE", gates("0"), gates("0")) == [] and wit("CAND", gates("0"), gates("0")) == [] and wit("CW0", gates("0", "0"), gates("0", "0")) == []
      and wit("CANDS", gates("1"), gates("1")) == [] and wit("BASES", gates("1"), gates("1")) == [])
check("witness red: BASE with RWM_RECOV_SNAPVER=1 on one end, CANDS with 0, an ABSENT token (a pre-dup-diag binary) on any arm are all failures, not 'off'",
      wit("BASE", gates("1"), gates("0")) != [] and wit("BASE", gates("0"), gates("1")) != [] and wit("CANDS", gates("0"), gates("0")) != [] and wit("BASES", gates("1"), gates(None)) != []
      and wit("CAND", gates(None), gates(None)) != [] and wit("BASE", gates(None), gates("0")) != [])
check("witness red: CW0 without RWM_PLACE_WDIV (or with 0.5), and RWM_PLACE_WDIV echoed on CAND / CANDS / BASE / BASES, fail",
      wit("CW0", gates("0"), gates("0")) != [] and wit("CW0", gates("0", "0.5"), gates("0", "0.5")) != [] and wit("CAND", gates("0", "0"), gates("0", "0")) != []
      and wit("CANDS", gates("1", "0"), gates("1", "0")) != [] and wit("BASES", gates("1", "0"), gates("1", "0")) != [] and wit("BASE", gates("0", "0"), gates("0", "0")) != [])

# ── the rehearsal / smoke criteria: PASS on a good ledger, a FAIL for each defect ──
def smoke_rows(mutate=None):
    out = []
    for cell, store, arm, seed, rep in pp.SMOKE53:
        on = arm in sn.ARMS_ON
        c, s = logs(snap=ON if on else None, snapver="1" if on else "0", wdiv="0" if arm == "CW0" else None)
        s = [x.replace("det=206359", "det=340000") for x in s]          # fed - det = 1 100 <= cx: live on the filter arms
        out.append(row(arm, cell, sn.row_metrics(c, s, DRV, 5_000_000_000 if cell == "c1d-400" else 600_000_000), seed=seed, rep=rep, bytes=5_000_000_000 if cell == "c1d-400" else 600_000_000))
    if mutate:
        mutate(out)
    return out


def run_smoke(rows):
    fd, p = tempfile.mkstemp(suffix=".log")
    with os.fdopen(fd, "w") as fh:
        fh.write("".join("P38ROW " + json.dumps(r, sort_keys=True) + "\n" for r in rows))
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = sn.smoke(p)
    os.remove(p)
    return rc, buf.getvalue()


rc, txt = run_smoke(smoke_rows())
check("smoke: a good 8-row ledger -> six PASS lines ([SNAPV], fed - det, [GATES] env, the filter arms, the c1d timeout, no None) + every row LIVE + SMOKE-PASS, rc 0",
      rc == 0 and txt.count("\nPASS ") + txt.startswith("PASS ") == 7 and "FAIL" not in txt and "SMOKE-PASS" in txt and txt.count(": LIVE") == 8, txt[-1500:])


def mut_arm(arm, cell, fn):
    def f(rows):
        for r in rows:
            if r["arm"] == arm and r["cell"] == cell:
                fn(r)
    return f


def red(name, mutate, criterion):
    rc, txt = run_smoke(smoke_rows(mutate))
    check(f"smoke red: {name} -> `FAIL {criterion}` and SMOKE-FAIL, rc 1", rc == 1 and re.search(r"^FAIL " + re.escape(criterion), txt, re.M) is not None and "SMOKE-FAIL" in txt, txt[-900:])


def drop_snapv(r):
    r["t53"]["snapv"] = None


red("no [SNAPV] on a row", mut_arm("CAND", "c8-100", drop_snapv), "[SNAPV] parsed")


def zero_rep(r):
    r["t53"]["snapv"]["1"]["rep"] = 0


red("rep = 0 on p1 of a BASE row", mut_arm("BASE", "c1d-400", zero_rep), "[SNAPV] parsed")


def over_fires(r):
    r["t53"]["snapv"]["1"]["pt_st"] = 10 ** 6


red("pt_st above the fires", mut_arm("CAND", "c1d-400", over_fires), "[SNAPV] parsed")


def no_det(r):
    r["t53"]["det"] = r["t53"]["fed_minus_det"] = None


red("fed - det not computed", mut_arm("BASE", "c8-100", no_det), "fed - det computed")


def bad_gate(r):
    r["problems"] = ["gate-snapver-srv=None (want 1)"]
    r["status"] = "WITNESS-FAIL"


red("a BASES row whose env is not in the server's [GATES]", mut_arm("BASES", "c8-100", bad_gate), "each arm's env in [GATES]")


def fire_on(r):
    r["t53"]["snapv"]["1"]["pt_st"] = 3


red("a CANDS row with pt_st > 0", mut_arm("CANDS", "c8-100", fire_on), "the filter arms' liveness")
red("a CANDS row with pt_st > 0 at c1d", mut_arm("CANDS", "c1d-400", fire_on), "the filter arms' liveness")


def dnf(r):
    r["dnf"], r["seconds"], r["mbps"] = True, None, None


red("the c1d row is a DNF (it did not finish inside RWM_PERF_TIMEOUT_S)", mut_arm("BASE", "c1d-400", dnf), "the c1d 5 GB row")


def rc1(r):
    r["rc"] = 124


red("the c1d row exited rc 124 (the timeout)", mut_arm("CAND", "c1d-400", rc1), "the c1d 5 GB row")


def none_lag(r):
    r["cli_lag_E"] = None


red("a registered readout is None (lag_E_cli)", mut_arm("CAND", "c8-100", none_lag), "no None among")


def none_dup(r):
    r["t53"]["rx_dup_legs"] = None


red("the per-leg duplicates are None", mut_arm("CAND", "c8-100", none_dup), "no None among")


def none_hold(r):
    r["t53"]["hold"] = {}


red("[HOLD] is absent", mut_arm("BASE", "c1d-400", none_hold), "no None among")
rows_x = smoke_rows()
rows_x.pop()
rc, txt = run_smoke(rows_x)
check("smoke red: a missing smoke row -> FAIL `every smoke row of the table ran`", rc == 1 and "FAIL every smoke row of the table ran" in txt, txt[-500:])


def short_tail(rows):
    for r in rows:
        r["t53"]["H0"] = None          # a tail shorter than one [DIAG] tick: undefined, not missing (calibrated: 1 of 6 s52 CAND rows, 24 of 24 guard c1d rows)


rc, txt = run_smoke(smoke_rows(short_tail))
check("smoke: H0 None with a tail value present is undefined (lever B's tail can be shorter than a tick), not a missing readout; hy_p50 `-` without a hy, E without stale reports likewise",
      rc == 0 and sn.missing(smoke_rows(short_tail)[0]) == [], (rc, sn.missing(smoke_rows(short_tail)[0])))

# ── the plan table ──
P = pp.plan53()
cnt = {}
for c, st, a, sd, rp in P:
    cnt[(c, a)] = cnt.get((c, a), 0) + 1
want = {("c8-100", a): 6 for a in pp.ARMS53_C8}
want.update({("c1d-400", "BASE"): 12, ("c1d-400", "CAND"): 12, ("c1d-400", "CANDS"): 6})
check("plan table: 60 rows = c8-100 5 arms x 6 + c1d-400 BASE 12, CAND 12, CANDS 6; no row repeats; c8 block first", len(P) == 60 and cnt == want and len(set(P)) == 60
      and [x[0] for x in P] == ["c8-100"] * 30 + ["c1d-400"] * 30, cnt)
check("plan table: every c8 group holds the five arms once, rotated by group (every arm leads at least one group); c1d CANDS is 3 + 3 per seed, one per group at most",
      all(sorted(a for c, _, a, sd, rp in P if c == "c8-100" and (sd, rp) == g) == sorted(pp.ARMS53_C8) for g in pp.BLOCKS52)
      and len({[a for c, _, a, sd, rp in P if c == "c8-100" and (sd, rp) == g][0] for g in pp.BLOCKS52}) == 5
      and {sd: sum(1 for c, _, a, s2, rp in P if c == "c1d-400" and a == "CANDS" and s2 == sd) for sd in ("42", "7")} == {"42": 3, "7": 3}
      and all(sum(1 for c, _, a, sd, rp in P if c == "c1d-400" and a == "CANDS" and (sd, rp) == g) <= 1 for g in pp.BLOCKS41C)
      and {sd: sum(1 for c, _, a, s2, rp in P if c == "c1d-400" and a == "BASE" and s2 == sd) for sd in ("42", "7")} == {"42": 6, "7": 6})
check("plan table: smoke = one row of each arm per block (c8: 5, c1d: 3)", pp.plan53(smoke=True) == pp.SMOKE53 and len(pp.SMOKE53) == 8)
check("ARM_ENV53 (the scorer's copy of the battery's arm table): CANDS / BASES set RWM_RECOV_SNAPVER=1, CW0 RWM_PLACE_WDIV=0, BASES on BASE's binary, CANDS / CW0 on CAND's",
      pp.ARM_ENV53 == {"BASE": ("BASE", None, None), "CAND": ("CAND", None, None), "CANDS": ("CAND", "1", None), "BASES": ("BASE", "1", None), "CW0": ("CAND", None, "0")})

# ── the shell side: sizes, the forwarded env, the arm table ──
bash = shutil.which("bash")
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
if bash:
    fns = "\n".join(ln for ln in bat.splitlines() if re.match(r"p(48|53)_bytes\(\)", ln))
    got = {c: subprocess.run([bash, "-c", fns + f'\np53_bytes {c}'], capture_output=True, text=True).stdout.strip() for c in ("c8-100", "c1d-400", "c2-100")}
    check("battery p53_bytes (the shell) == place38_parse.BYTES53: 600 MB at c8-100, 5 GB at c1d-400 (plan 48's count), nothing else", got == {"c8-100": "600000000", "c1d-400": "5000000000", "c2-100": ""}
          and pp.BYTES53 == {"c8-100": 600000000, "c1d-400": 5000000000}, got)
    # RWM_RECOV_SNAPVER / RWM_PLACE_WDIV are gone from the post-law engine (the law is shipped, status 54), so lib.sh no longer forwards them; the knob plan 54b needs is the store pool.
    out = subprocess.run([bash, "-c", f'cd "{HERE.replace(chr(92), "/")}" && source ./lib.sh >/dev/null 2>&1; RWM_STORE_PATH_POOL=640 rwm_forward_env'], capture_output=True, text=True).stdout
    out0 = subprocess.run([bash, "-c", f'cd "{HERE.replace(chr(92), "/")}" && source ./lib.sh >/dev/null 2>&1; env -u RWM_STORE_PATH_POOL bash -c "source ./lib.sh >/dev/null 2>&1; rwm_forward_env"'], capture_output=True, text=True).stdout
    check("lib.sh's rwm_forward_env (what perf_rwm_c.sh hands both ends) carries RWM_STORE_PATH_POOL when set and nothing when unset",
          "RWM_STORE_PATH_POOL=640" in out and "RWM_STORE_PATH_POOL" not in out0, (out, out0))
else:
    print("SKIP no bash for the shell-side checks")
check("battery: the env -u list clears RWM_RECOV_SNAPVER (an ambient value never leaks into a BASE / CAND row), INSTR_ENV for plan 53 carries RWM_PERF_TIMEOUT_S=200 like plan 48, RWM_DIAG=1 on every row",
      "-u RWM_PLACE_WDIV -u RWM_PLACE_WDIV_DERIVED -u RWM_RECOV_SNAPVER" in bat and '[ "$P53" = 1 ] ) && INSTR_ENV' not in bat and '"$P53" = 1 ] || [ "$P54B" = 1 ]; } && INSTR_ENV=(RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=200)' in bat
      and "RWM_GEN=0 RWM_DIAG=1" in bat)
check("battery: plan 53's preflight pins both binaries to the RWM_RECOV_SNAPVER echo and the CAND binary to RWM_PLACE_WDIV (CW0 runs on it); the arm table has CANDS / BASES / CW0",
      'grep -aqF -- "RWM_RECOV_SNAPVER" "$b"' in bat and 'grep -aqF -- " RWM_PLACE_WDIV=" "$BIN"' in bat and "CANDS|BASES|CW0)" in bat and 'BASES) arm_env=(RWM_RECOV_SNAPVER=1); RBIN="$BIN_BASE"' in bat)

# ── the rank-sum report and the whole score, on a synthetic 60-row ledger ──
def synth(i, cell, arm, seed, rep, dup, cpu, lag):
    c, s = logs(snap=ON if arm in sn.ARMS_ON else None, snapver="1" if arm in sn.ARMS_ON else "0", wdiv="0" if arm == "CW0" else None)
    s = [x.replace("det=206359", "det=340000") for x in s]
    m = sn.row_metrics(c, s, DRV, 5_000_000_000 if cell == "c1d-400" else 600_000_000)
    m["rx_dup"], m["rx_dup_legs"] = dup, {"0": dup // 2, "1": dup - dup // 2}
    return row(arm, cell, m, seed=seed, rep=rep, cpu_cli=cpu, cpu_srv=cpu, cli_lag_E=lag, srv_lag_E=lag, bytes=5_000_000_000 if cell == "c1d-400" else 600_000_000, mbps=100.0 + i * 0.01)


rows = []
for i, (c, st, a, sd, rp) in enumerate(P):
    sep = a in ("CAND", "CANDS") and c == "c1d-400"
    rows.append(synth(i, c, a, sd, rp, dup=(2000 + 10 * rp + (50 if sd == "7" else 0)) + (800 if sep else 0), cpu=80.0 + rp + (30 if sep else 0), lag=900 + rp))
live = [r for r in rows if sn.liveness(r)[0] == "LIVE"]
check("synthetic 60-row ledger: every row is LIVE under the liveness rules", len(live) == 60, [(r["cell"], r["arm"], sn.liveness(r)) for r in rows if sn.liveness(r)[0] != "LIVE"][:3])
buf = io.StringIO()
res = sn.compare(live, "c1d-400", "BASE", "CAND", lambda x: buf.write(x + "\n"), n_perm=2000)
ptxt = buf.getvalue()
check("rank-sum (place41b_score.statistic_rank's per-clause p, one-sided worse side): a CAND that duplicates +800 and costs +30 s CPU separates from BASE (p < 0.01 on dup_total and both CPU/GB) "
      "while the readouts built identical (lag_E, goodput order aside) do not (p(lag_E_cli) > 0.05); the median ratio of dup_total is (2800 + ...) / (2000 + ...) = 1.3-1.4",
      res[("c1d-400", "dup_total")] < 0.01 and res[("c1d-400", "cpu_cli/GB")] < 0.01 and res[("c1d-400", "cpu_srv/GB")] < 0.01 and res[("c1d-400", "lag_E_cli")] > 0.05
      and re.search(r"dup_total\s+0\.\d+\s+1\.3\d\d", ptxt) is not None and "ONE-SIDED worse side" in ptxt and "1 - " not in ptxt and "min-p" not in ptxt, ptxt)
check("rank-sum: every registered c1d readout is in the table (goodput, CPU/GB both ends, lag_E both ends, rtp_floor_p0, dup total / p0 / p1, retx/GB)",
      all(re.search(r"^\s+" + re.escape(n) + r"\s", ptxt, re.M) for n in ("goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "rtp_floor_p0", "dup_total", "dup_p0", "dup_p1", "retx/GB")), ptxt)
fd, lp = tempfile.mkstemp(suffix=".log")
with os.fdopen(fd, "w") as fh:
    fh.write("".join("P38ROW " + json.dumps(r, sort_keys=True) + "\n" for r in rows) + "LD-BATTERY-DONE tag=ld blocks=2\n")
buf = io.StringIO()
with redirect_stdout(buf):
    sn.score([lp])
sc = buf.getvalue()
check("score: reported only (header, last line `VERDICT53 REPORTED-ONLY rows=60/60 live=60 void=0`), per-arm blocks at both cells with median and range, the c1d CAND-against-BASE table, no PASS / NO VERDICT word",
      "REPORTED ONLY" in sc and sc.rstrip().splitlines()[-1] == "VERDICT53 REPORTED-ONLY rows=60/60 live=60 void=0" and "-- CANDS: n = 6" in sc and "-- BASE: n = 12" in sc and "-- BASES: n = 6" in sc
      and "== c1d: CAND against BASE (n = 12 + 12)" in sc and "[SNAPV] p1 stale/rep" in sc and "E (enrichment factor)" in sc and "median" in sc and "PASS" not in sc and "NO VERDICT" not in sc, sc[-1200:])
rows_v = copy.deepcopy(rows)
rows_v[0]["t53"]["snapv"] = None
with open(lp, "w") as fh:
    fh.write("".join("P38ROW " + json.dumps(r, sort_keys=True) + "\n" for r in rows_v))
buf = io.StringIO()
with redirect_stdout(buf):
    sn.score([lp])
sv = buf.getvalue()
check("score with a void row: `VOID c8-100 ... no-[SNAPV]` listed, that arm's n is 5, the verdict line reads live=59 void=1",
      "VOID c8-100" in sv and "no-[SNAPV]" in sv and "live=59 void=1" in sv and re.search(r"-- %s: n = 5 counting" % rows_v[0]["arm"], sv) is not None, sv[:1500])
rows_y = copy.deepcopy(rows)
rows_y[1]["t53"]["fires"]["y"] += 1
with open(lp, "w") as fh:
    fh.write("".join("P38ROW " + json.dumps(r, sort_keys=True) + "\n" for r in rows_y))
buf = io.StringIO()
with redirect_stdout(buf):
    sn.score([lp])
check("score: a row whose y differs from fast is flagged (Y-NE-FAST)", "Y-NE-FAST" in buf.getvalue(), buf.getvalue()[:600])
os.remove(lp)

# ── review changes: means beside medians, pooled ratios, the stale = 0 escape ──
check("arm_stats prints the labelled mean, median and range: [1, 2, 9] -> mean 4.0, median 2.0, range [1.0 - 9.0]",
      sn.arm_stats([{"t53": {"x": v}} for v in (1.0, 2.0, 9.0)], lambda r, t: t["x"], 1) == "mean 4.0  median 2.0  range [1.0 - 9.0]  (n 3)", sn.arm_stats([{"t53": {"x": v}} for v in (1.0, 2.0, 9.0)], lambda r, t: t["x"], 1))


def srow(arm, p0, p1, fast=1000, cell="c8-100", **kw):
    c, s = logs(snap=snapv_line(p0=p0, p1=p1), snapver="1" if arm in sn.ARMS_ON else "0")
    s = [x.replace("det=206359", "det=340000") for x in s]
    m = sn.row_metrics(c, s, DRV, 600_000_000)
    m["fires"]["fast"] = fast
    return row(arm, cell, m, **kw)


rA, rB = srow("CAND", (1000, 0, 0, 0), (100, 90, 40, 0), fast=1000), srow("CAND", (1000, 0, 0, 0), (900, 100, 10, 0), fast=3000)
pv = sn.pooled([rA, rB], "[SNAPV] p1 stale/rep")
check("pooled stale/rep is the ratio of sums (190 / 1000 = 0.19), not the mean of per-row ratios (0.5)", abs(pv[0] - 0.19) < 1e-12 and abs((0.9 + 100 / 900) / 2 - 0.5) > 0.0 and pv[1] == 2, pv)
check("pooled p1 share of rep = (100 + 900) / (2000 + 1000) over the numeric legs", abs(sn.pooled([rA, rB], "[SNAPV] p1 share of rep")[0] - 1000 / 3000) < 1e-12)
pe = sn.pooled([rA, rB], "E (enrichment factor)")
check("pooled E = (sum pt_st / sum fast) / (sum stale / sum rep) = (50 / 4000) / (190 / 3000); the mean of the per-row E differs",
      abs(pe[0] - (50 / 4000) / (190 / 3000)) < 1e-12 and abs(pe[0] - (sn.enrichment(sn.snapv([snapv_line(p0=(1000, 0, 0, 0), p1=(100, 90, 40, 0))]), 1000) + sn.enrichment(sn.snapv([snapv_line(p0=(1000, 0, 0, 0), p1=(900, 100, 10, 0))]), 3000)) / 2) > 0.05, pe)
check("pooled is None without any [SNAPV] row", sn.pooled([row("CAND", "c8-100", {"snapv": None, "fires": {}})], "E (enrichment factor)") is None)
ln = sn.readout_line([rA, rB], "[SNAPV] p1 stale/rep", None, 3)
ln2 = sn.readout_line([rA, rB], "receiver duplicates", lambda r, t: t.get("rx_dup"), 0)
check("the report labels each statistic: ratios read `pooled`, counts read `mean ... median ... range`", ln.startswith("pooled 0.190") and ln2.startswith("mean ") and " median " in ln2 and " range [" in ln2, (ln, ln2))
buf = io.StringIO()
sn.report_cell([rA, rB], "c8-100", ["CAND"], buf.write if False else (lambda x: buf.write(x + "\n")))
tb = buf.getvalue()
check("the per-row table keeps the per-row ratios (0.000 0.900 and 0.000 0.111), beside the pooled value", "per-row table, c8-100" in tb and "0.000 0.900" in tb and "0.000 0.111" in tb and "pooled 0.190" in tb, tb[-900:])

# the stale = 0 escape
Z0, Z1 = (9000, 0, 0, 0), (1000, 0, 0, 0)               # p0 / p1 with no stale report
def zrow(arm, p1=Z1, **kw):
    r = srow(arm, Z0, p1, **kw)
    return r


off_zero = [zrow("BASE"), zrow("CAND")]
on_zero = zrow("CANDS")
on_zero["t53"]["fed_minus_det"], on_zero["t53"]["cx_sum"] = 5, 10
check("stale_absent: every LIVE arm-off row (BASE and CAND) with p1 stale = 0 -> True; one with stale > 0, no arm-off row, or only a void arm-off row -> False",
      sn.stale_absent(off_zero, "c8-100") and not sn.stale_absent(off_zero + [zrow("CAND", p1=(1000, 5, 0, 0))], "c8-100") and not sn.stale_absent([on_zero], "c8-100")
      and sn.stale_absent(off_zero + [zrow("BASE", p1=(1000, 5, 0, 0), status="WITNESS-FAIL")], "c8-100") and not sn.stale_absent([zrow("BASE", status="WITNESS-FAIL")], "c8-100")
      and not sn.stale_absent(off_zero, "c1d-400"), None)
check("escape: a CANDS row with p1 stale = 0 is VOID without the escape and LIVE with it", sn.liveness(on_zero)[0] == "VOID" and "stale=0-on-p1" in sn.liveness(on_zero)[1] and sn.liveness(on_zero, True) == ("LIVE", []), (sn.liveness(on_zero), sn.liveness(on_zero, True)))
bad_pt = zrow("CANDS", p1=(1000, 0, 3, 0))
bad_pt["t53"]["fed_minus_det"], bad_pt["t53"]["cx_sum"] = 5, 10
bad_fd = zrow("BASES")
bad_fd["t53"]["fed_minus_det"], bad_fd["t53"]["cx_sum"] = 500, 10
check("escape: the other arm-row rules still apply (pt_st > 0 -> VOID, fed - det > cx -> VOID)",
      sn.liveness(bad_pt, True)[0] == "VOID" and any("pt_st+tt_st=3" in x for x in sn.liveness(bad_pt, True)[1]) and sn.liveness(bad_fd, True)[0] == "VOID" and any("fed-det=500>cx=10" in x for x in sn.liveness(bad_fd, True)[1]))


def score_text(rows_):
    fd_, lp_ = tempfile.mkstemp(suffix=".log")
    with os.fdopen(fd_, "w") as fh:
        fh.write("".join("P38ROW " + json.dumps(r, sort_keys=True) + "\n" for r in rows_))
    b = io.StringIO()
    with redirect_stdout(b):
        sn.score([lp_])
    os.remove(lp_)
    return b.getvalue()


for i, r in enumerate(off_zero):
    r["seed"], r["rep"] = "42", i + 1
on_zero["seed"], on_zero["rep"] = "42", 3
txt0 = score_text(off_zero + [on_zero])
check("score: when the arm-off rows have p1 stale = 0 it prints `STALE-ABSENT block=c8-100: arm-off p1 stale = 0, the arm-off falsifier decides` and the CANDS row is not voided",
      "STALE-ABSENT block=c8-100: arm-off p1 stale = 0, the arm-off falsifier decides" in txt0 and "VOID c8-100 CANDS" not in txt0, txt0[:900])
txt1 = score_text([srow("BASE", Z0, (1000, 900, 0, 0), seed="42", rep=1), on_zero])
check("score: with stale reports on the arm-off rows there is no escape: no STALE-ABSENT line, the CANDS row with p1 stale = 0 is VOID", "STALE-ABSENT" not in txt1 and "VOID c8-100 CANDS" in txt1 and "stale=0-on-p1" in txt1, txt1[:900])


def zero_smoke(arm_zero):
    def m(rows_):
        for r in rows_:
            if r["cell"] == "c8-100" and r["arm"] in arm_zero:
                for g in ("1",):
                    r["t53"]["snapv"][g]["stale"] = 0
                if r["arm"] in sn.ARMS_ON:
                    pass
    return m


rc, tx = run_smoke(smoke_rows(zero_smoke(("BASE", "CAND", "CANDS", "BASES"))))
check("smoke: the smoke's own BASE / CAND rows show p1 stale = 0 -> the escape applies (STALE-ABSENT printed) and the filter arms pass", rc == 0 and "STALE-ABSENT block=c8-100" in tx and "SMOKE-PASS" in tx, tx[-900:])
rc, tx = run_smoke(smoke_rows(zero_smoke(("CAND", "CANDS", "BASES"))))
check("smoke: BASE still shows p1 stale > 0 -> no escape: a CANDS row with p1 stale = 0 fails `the filter arms' liveness`", rc == 1 and "STALE-ABSENT" not in tx and re.search(r"^FAIL the filter arms' liveness", tx, re.M) is not None, tx[-900:])

# ── the dispatcher the shell calls ──
fd, lp = tempfile.mkstemp(suffix=".log")
with os.fdopen(fd, "w") as fh:
    fh.write("".join("P38ROW " + json.dumps(r, sort_keys=True) + "\n" for r in rows) + "LD-BATTERY-DONE tag=ld blocks=2\n")
outs = {c: subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), c, lp], capture_output=True, text=True, errors="replace") for c in ("check53", "score53")}
check("loopdiag_parse.py check53 / score53 dispatch (the commands run_all calls)", outs["check53"].returncode == 0 and "rows=60/60" in outs["check53"].stdout and outs["score53"].returncode == 0
      and "VERDICT53 REPORTED-ONLY" in outs["score53"].stdout, (outs["check53"].stdout[-200:], outs["score53"].stderr[-400:]))
fd2, sp_ = tempfile.mkstemp(suffix=".log")
with os.fdopen(fd2, "w") as fh:
    fh.write("".join("P38ROW " + json.dumps(r, sort_keys=True) + "\n" for r in smoke_rows()))
ps = subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), "smoke53", sp_], capture_output=True, text=True, errors="replace")
check("loopdiag_parse.py smoke53 dispatch passes a good smoke ledger", ps.returncode == 0 and "SMOKE-PASS" in ps.stdout, ps.stdout[-300:] + ps.stderr[-300:])
os.remove(lp)
os.remove(sp_)
plan = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "plan53"], capture_output=True, text=True).stdout.split("\n")
check("place38_parse.py plan53 prints the 60-row table the battery reads (`cell store arm seed rep`)", len([x for x in plan if x.strip()]) == 60 and plan[0].split() == ["c8-100", "def", "BASE", "42", "1"], plan[:2])

if _BASE_SHA_AT_START is not None:
    os.environ["LD_BASE_SHA"] = _BASE_SHA_AT_START
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
