#!/usr/bin/env python3
"""Tests for the status 56 harness (plan 56, the store-pool sweep, REPORTED ONLY: tools/l1/pool56_score.py, the place38_parse parts, the battery / run_all wiring).

Four layers, each with its red case beside the green one:
  1. the plan table (64 slots, blocks, rotation, seeds, the c1s extras at the end, the dump row last) and the shell side (arm env, sizes, scrub, counts);
  2. row readouts on synthetic [DIAG] lines with KNOWN answers (rates, rtt, T-bar, T0, ell, the Little residual, bind fraction, rtp over boot, the late window at 15 s / 5 s at c1s,
     single-path cells) and the row witnesses (pool echo, dump gate, seat sha);
  3. CALIBRATION on section 54b's committed B640 rows (docs/l1-raw/s54/rep54b-diag-ld.tgz): the scorer's definitions reproduce `docs/derivations/store-law/little54b.out` and the
     registration's B640 ranges, the re-basing tolerances come out as the rule computes them, and a control equal to B640 re-bases nothing;
  4. the treatment rules (VOID replaced once, UNBOUND labelled and not replaced, DNF counted as 0, the c1s quinn-collapse flag excluded from F8) and the arithmetic of each falsifier
     F1-F10 at its boundary (FIRES / DOES-NOT-FIRE / UNSCOREABLE), the standing-queue interpolation, the smoke criteria and a whole report on a synthetic ledger.
Run: python3 test_pool56.py. The end-to-end path through loopdiag_run_all.sh (plan 56) runs in test_stubrun.py."""
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES"):
    os.environ.pop(_k, None)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import pool56_score as ps  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol=1e-6):
    return a is not None and b is not None and abs(a - b) <= tol


def capture(fn, *a, **k):
    out = []
    r = fn(*a, out=out.append, **k)
    return r, "\n".join(out)


# ═══ 1. the plan table and the shell side ═══
T = pp.plan56()
check("the table has 64 slots, all distinct (cell, arm, seed, rep)", len(T) == 64 and len({(c, a, s, r) for c, _, a, s, r in T}) == 64, len(T))
cnt = {}
for c, _, a, s, r in T:
    cnt[(c, a)] = cnt.get((c, a), 0) + 1
want_cnt = {**{("c8-100", "P%d" % p): 4 for p in (640, 384, 256, 160, 112, 80)}, **{("c2-100", "P%d" % p): 3 for p in (2048, 384, 256, 192, 160, 128)},
            ("c1s-400", "P2048"): 6, ("c1s-400", "P512"): 6, ("c1s-400", "P1024"): 3, ("c1s-400", "P768"): 3, ("c1s-400", "P384"): 3, ("c8-100", "P640D"): 1}
check("n per arm: c8 4 x 6 pools, c2 3 x 6, c1s 6 (2048, 512) and 3 (1024, 768, 384), the dump row 1 -- 24 + 18 + 21 + 1", cnt == want_cnt, {k: v for k, v in cnt.items() if want_cnt.get(k) != v})
order = [c for c, *_ in T]
check("cells in blocks c8 -> c2 -> c1s, the dump row (a c8 row) last", order == ["c8-100"] * 24 + ["c2-100"] * 18 + ["c1s-400"] * 21 + ["c8-100"] and T[-1][2] == "P640D", order[:3])
ok_rot = True
for cell in ("c8-100", "c2-100", "c1s-400"):
    rows = [x for x in T[:-1] if x[0] == cell]
    reps = sorted({x[4] for x in rows})
    pools_by_rep = {rp: [x[2] for x in rows if x[4] == rp] for rp in reps}
    ok_rot &= all(len(set(v)) == len(v) for v in pools_by_rep.values())                      # every pool at most once per replicate
    ok_rot &= all({x[3] for x in rows if x[4] == rp} == {"42" if (rp - 1) % 2 == 0 else "7"} for rp in reps)       # seeds 42 / 7 alternate by replicate
    base = list(pools_by_rep[reps[0]])
    for k, rp in enumerate(reps):
        if len(pools_by_rep[rp]) == len(base):
            ok_rot &= pools_by_rep[rp] == base[k % len(base):] + base[:k % len(base)]       # a cyclic Latin rotation by replicate
    lead = [pools_by_rep[rp][0] for rp in reps if len(pools_by_rep[rp]) == len(base)]
    ok_rot &= len(set(lead)) == len(lead)                                                    # no pool leads twice
check("per cell: every pool once per replicate, seeds 42 / 7 alternating, a cyclic Latin rotation, no pool leads twice", ok_rot)
c1s = [x for x in T if x[0] == "c1s-400"]
check("c1s: replicates 1-3 hold all five pools (15 rows), the extras (replicates 4-6) hold 2048 and 512 only (6 rows) at the block's end, rotated",
      [x[4] for x in c1s] == [1] * 5 + [2] * 5 + [3] * 5 + [4, 4, 5, 5, 6, 6] and {x[2] for x in c1s[15:]} == {"P2048", "P512"} and c1s[15][2] != c1s[17][2], [(x[2], x[4]) for x in c1s][15:])
check("smoke: one row per cell at its smallest pool (c8 P80, c2 P128, c1s P384) and the dump row", [(c, a) for c, _, a, _, _ in pp.plan56(smoke=True)] ==
      [("c8-100", "P80"), ("c2-100", "P128"), ("c1s-400", "P384"), ("c8-100", "P640D")], pp.plan56(smoke=True))
check("arm names parse to (pool, dump)", pp.arm_pool56("P640") == (640, False) and pp.arm_pool56("P640D") == (640, True) and pp.arm_pool56("B640") == (None, False) and pp.arm_pool56("P") == (None, False))
check("the control pools are c8 640 (section 54b's B640), c2 / c1s 2048; N is 2 at c8 and 1 elsewhere; sizes 600 MB / 600 MB / 5 GB",
      pp.CONTROL56 == {"c8-100": 640, "c2-100": 2048, "c1s-400": 2048} and pp.NLIVE56 == ps.NLIVE and pp.BYTES56 == {"c8-100": 600000000, "c2-100": 600000000, "c1s-400": 5000000000})
shell = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
m = re.search(r"p56_bytes\(\) \{ case \"\$1\" in c8-100\|c2-100\) echo (\d+) ;; c1s-400\) echo (\d+) ;;", shell)
check("the battery's p56_bytes is the scorer's BYTES56", m is not None and int(m.group(1)) == pp.BYTES56["c8-100"] == pp.BYTES56["c2-100"] and int(m.group(2)) == pp.BYTES56["c1s-400"])
check("the battery: P56 is single-seat (P41 stays 0), the arm env sets RWM_STORE_PATH_POOL at every pool but 2048 and RWM_SUCC_DUMP=1 on the D arm",
      'P56=0; [ "$PLAN" = "56" ] && { P56=1; P38=1; PCAND=1; }' in shell and '[ "$p56pool" = "2048" ] || arm_env=(RWM_STORE_PATH_POOL="$p56pool")' in shell
      and '[ "$p56dump" = 1 ] && arm_env+=(RWM_SUCC_DUMP=1)' in shell)
check("the battery: instrument env RWM_RTOBS RWM_RDIAG with RWM_PERF_TIMEOUT_S=300, the scrub withholds RWM_STORE_PATH_POOL / RWM_SUCC_DUMP / RWM_SUCC_DUMP_MAX, 4 smoke rows, 64 slots",
      '[ "$P56" = 1 ] && INSTR_ENV=(RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300)' in shell and "-u RWM_STORE_PATH_POOL -u RWM_SUCC_DUMP -u RWM_SUCC_DUMP_MAX" in shell
      and '-eq 4 ] || { echo "ABORT-PLAN56 smoke' in shell and '-eq 64 ] || { echo "ABORT-PLAN56 table' in shell)
check("the battery replaces once: run_slot56 reruns the slot when place38_parse.replace56 says so, and only the full battery uses it (the smoke runs each row once)",
      'replace56 "$OUT"' in shell and "run_slot56()" in shell and 'if [ "$1" = "1" ]; then run_one' in shell)
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
check("run_all: plan 56 is a candidate plan without a BASE tree (P46 yes, P41 no), pre-flights 1 GB of /dev/shm, and its stage is rep56",
      'P56=0; [ "$PLAN" = "56" ] && { P56=1; P46=1; }' in runall and "LD_SHM_MIN_KB:-1048576" in runall and "stage rep56 56" in runall and "ABORT-SHM-SPACE" in runall)
rc = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "plan56"], capture_output=True, text=True)
lines = [x for x in rc.stdout.replace("\r", "").split("\n") if x]
check("place38_parse plan56 prints the 64-row table the battery reads (first row c8-100 def P640 42 1) and plan56 smoke 4 rows",
      len(lines) == 64 and lines[0] == "c8-100 def P640 42 1" and len([x for x in subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "plan56", "smoke"], capture_output=True, text=True).stdout.split("\n") if x.strip()]) == 4, lines[:2])


# ═══ 2. row readouts on synthetic [DIAG] lines ═══
def diag(t, cap, a, nleg, r, rtt, rtp, qc, src, srcc, ack, q=(18, 1)):
    """One client [DIAG] line. Per leg i: tx = r[i] * t (so d tx / dt = r[i]), hand = tx + q[i]; rtt[i] / wrtt = rtt/2 / rtp_us[i] / qcwnd[i]. A decoy `rtt=7/wrtt=7` and `fp/on` sit before the
    ` pN:infl=` blocks, as in the engine's line."""
    s = (f"[DIAG] t={t:.2f}s win={a}/{cap} paused=0% good=1Mbit src={src}sym/s cod=0sym/s cum={srcc}/0/{ack} cwnd=1 infl=1 np={nleg} np_act={nleg} rtt=15.8ms bdp100=1sym "
         f"mpr[rep=1 seqs=1 fired=1 supp=0/0/0 pf=0/1/2 fp/on p0:3/4 p1:5/6] wnd2=0/0 rtt=7/wrtt=7 ")
    for i in range(nleg):
        s += f"dgq{i}[hand={int(r[i] * t) + q[i]} tx={int(r[i] * t)} full=0 err=0 sp=1] "
    for i in range(nleg):
        s += (f"p{i}:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00 btlbw=1 est=Y pl=0.0000 cmp=- rtt={rtt[i]}/wrtt={rtt[i] // 2}/rtp3ms rtp_us={rtp[i]} sig_us=1/n1 qsp_us=1/n1 msd_us=1/n1 "
              f"tlag_us=1/n1 gapd=0/0 qcwnd={qc[i]} qce=0 qlp=1/1 | ANCHOR sent=0 plu=0.0010 wpl_us=1 ")
    return s.rstrip()


def run_lines(nleg=2, cap=1280, r=(8000, 2000), t_end=40.0, bound=True, qc_dip=0.0, early_rtt=(100, 60), late_rtt=(200, 100)):
    """t = 0.25 .. t_end every 0.25 s, source until t_end - 2. Rtt (ms) is early_rtt before t = 2 s and late_rtt after; rtp_us boots at (12000, 34000) and from t = 10 s sits at
    (42000, 36000) (E03's excursion: +30 ms on leg 0, +2 ms on leg 1). qcwnd is 1 000 000 except in the first `qc_dip` share of ticks, where it is 100 000 (< 1/4 of the median)."""
    out = ['{"seconds":0.01,"warmup":true}']
    ticks = int(t_end / 0.25)
    for k in range(1, ticks + 1):
        t = 0.25 * k
        late = t >= 2.0
        rt = late_rtt if late else early_rtt
        rtp = (42000, 36000) if t >= 10 else (12000, 34000)
        qc = 100000 if k <= int(qc_dip * ticks) else 1000000
        out.append(diag(t, cap if bound else cap - 100, 800, nleg, r[:nleg], rt[:nleg], rtp[:nleg], (qc, qc)[:nleg], 1000 if t <= t_end - 2 else 0, int(1000 * t), max(0, int(1000 * (t - 2)))))
    return out


C8L = run_lines()
m8 = ps.row_metrics(C8L, [], [], 600000000, "c8-100", 640)
check("c8 row: r0 / r1 / R are d(tx)/dt over the LATE window (8000 / 2000 / 10000 per s), rtt LATE medians 200 / 100, q0 18", near(m8["r0"], 8000, 1) and near(m8["r1"], 2000, 1) and near(m8["tx_sum_late"], 10000, 2)
      and m8["rtt_late0"] == 200 and m8["rtt_late1"] == 100 and m8["q_late0"] == 18 and m8["q_late1"] == 1, {k: m8.get(k) for k in ("r0", "r1", "tx_sum_late", "rtt_late0", "q_late0")})
check("T-bar = sum r_i rtt_i / sum r_i = (8000 x 200 + 2000 x 100) / 10000 = 180 ms; W / sum r = 1280 / 10000 = 128 ms; Little residual = 1280 - 1800 = -520",
      near(m8["Tbar_ms"], 180.0, 0.5) and near(m8["TW_ms"], 128.0, 0.2) and near(m8["resid_late"], -520.0, 5.0) and near(m8["resid_rel"], -520.0 / 1280, 0.005),
      {k: m8.get(k) for k in ("Tbar_ms", "TW_ms", "resid_late", "resid_rel")})
check("the first-1s loop is kept as an INFORMATIONAL readout only (rtt 100 / 60: (8000 x 100 + 2000 x 60) / 10000 = 92 ms), no longer T0", near(m8["T0_first1s_ms"], 92.0, 0.5) and "T0_ms" not in m8, m8.get("T0_first1s_ms"))
check("rtp: boot (12, 34 ms) is the first sending tick's value, LATE (42, 36) -> over boot (30, 2); ell = the rate-weighted boot = (8000 x 12 + 2000 x 34) / 10000 = 16.4 ms",
      near(m8["rtp_boot0"], 12.0) and near(m8["rtp_boot1"], 34.0) and near(m8["rtp_late0"], 42.0) and near(m8["rtp_over0"], 30.0) and near(m8["rtp_over1"], 2.0) and near(m8["ell_ms"], 16.4, 0.05),
      {k: m8.get(k) for k in ("rtp_boot0", "rtp_late0", "rtp_over0", "ell_ms")})
check("bound row: bind fraction 1.0 (cap = N x pool = 1280 on every LATE tick), realised cap 1280; the same row with cap 1180 (not at N x pool) binds 0.0 and reports the realised cap 1180",
      m8["bind_frac"] == 1.0 and m8["cap_mean"] == 1280 and ps.row_metrics(run_lines(bound=False), [], [], 600000000, "c8-100", 640)["bind_frac"] == 0.0
      and ps.row_metrics(run_lines(bound=False), [], [], 600000000, "c8-100", 640)["cap_mean"] == 1180)
check("the LATE window starts at 15 s at c8 / c2 and 5 s at c1s: the same single-path ticks give late_n 100 at c2 (15 s - 40 s, src > 0 to 38 s: 93 ticks) vs 133 at c1s (5 s - 38 s)",
      ps.row_metrics(run_lines(1, 603), [], [], 600000000, "c2-100", 2048)["late_n"] == 93 and ps.row_metrics(run_lines(1, 2048), [], [], 5000000000, "c1s-400", 2048)["late_n"] == 133,
      (ps.row_metrics(run_lines(1, 603), [], [], 600000000, "c2-100", 2048)["late_n"], ps.row_metrics(run_lines(1, 2048), [], [], 5000000000, "c1s-400", 2048)["late_n"]))
m2 = ps.row_metrics(run_lines(1, 603, r=(9600, 0)), [], [], 600000000, "c2-100", 603)
check("single-path cell: leg-0 fields filled, leg-1 fields None (not 0), T-bar = rtt0 = 200 ms, R = r0 = 9600, the bind needs cap >= 1 x pool (603 binds, N = 1)",
      near(m2["r0"], 9600, 2) and m2["r1"] is None and m2["rtt_late1"] is None and m2["rtp_late1"] is None and near(m2["Tbar_ms"], 200.0, 0.5) and near(m2["tx_sum_late"], 9600, 2) and m2["bind_frac"] == 1.0, m2)
check("in-order delay late mean is the ack lag (2 s) at t >= 15 s: 2.0 s", near(m8["inorder_mean_late"], 2.0, 0.01), m8["inorder_mean_late"])
dip = ps.row_metrics(run_lines(1, 2048, qc_dip=0.15), [], [], 5000000000, "c1s-400", 2048)
check("quinn-collapse share = the share of ticks (t >= 1 s) with qcwnd < 1/4 of the row median: 0.15 of the ticks dipped -> ~0.15; none -> 0.0",
      0.12 < dip["qc_share"] < 0.16 and ps.row_metrics(run_lines(1, 2048), [], [], 5000000000, "c1s-400", 2048)["qc_share"] == 0.0, dip["qc_share"])
check("a row with no usable LATE ticks reports None readouts, never raises", ps.row_metrics(run_lines(t_end=3.0), [], [], 600000000, "c8-100", 640)["tx_sum_late"] is None)
rank = [f"[RANK] holes={900 + i} pivots=2 deficit=1 tail_overcount=100 max_deficit=1 empty=0 reports={i}" for i in range(40)]
check("old holes = median of holes - tail_overcount over [RANK] readouts[15:-6] (800 + i, i = 15..33 -> 824)", near(ps.row_metrics(C8L, rank, [], 600000000, "c8-100", 640)["old_holes"], 824.0), ps.row_metrics(C8L, rank, [], 600000000, "c8-100", 640)["old_holes"])

# the row witnesses
GAT = "INFO raptorpath::gates: [GATES] RWM_UNIFIED=1 RWM_STORE_PATH_POOL={p} RWM_SUCC_DUMP={d} RWM_STORE=unset"
INFO = "INFO raptorpath::net: path-scaled outstanding pool ACTIVE pool_per_path={p} gain=2.0"


def ends(p, d=0, info=None):
    return [GAT.format(p=p, d=d), INFO.format(p=p if info is None else info)], [GAT.format(p=p, d=d), INFO.format(p=p if info is None else info)]


cli, srv = ends(384)
check("witness green: pool 384 echoed on both ends and in pool_per_path, dump 0 / SUCC_DUMP=0, the seat's sha", pp.row_problems56("P384", "c8-100", cli, srv, "abc", "abc") == [], pp.row_problems56("P384", "c8-100", cli, srv, "abc", "abc"))
cli, srv = ends(2048)
pr = pp.row_problems56("P384", "c8-100", cli, srv, "abc", "abc")
check("witness red: a reduced pool that echoes 2048 (the silent fallback) is a failure on both ends and in pool_per_path, and is_pool_void56 sees it",
      any(x.startswith("gate-pool-cli") for x in pr) and any(x.startswith("gate-pool-srv") for x in pr) and any(x.startswith("pool_per_path-cli") for x in pr) and pp.is_pool_void56({"problems": pr}), pr)
cli, srv = ends(2048)
check("the shipped 2048 arm echoes 2048 and sets nothing: green", pp.row_problems56("P2048", "c2-100", cli, srv, "abc", "abc") == [])
cli, srv = ends(384, info=2048)
check("witness red: pool_per_path != the request fails even when [GATES] is right", any(x.startswith("pool_per_path") for x in pp.row_problems56("P384", "c8-100", cli, srv, "abc", "abc")))
cli, srv = ends(640, d=1)
check("dump gate: the dump arm needs RWM_SUCC_DUMP=1 on both ends; a plain arm echoing 1 fails; the dump arm echoing 0 fails",
      pp.row_problems56("P640D", "c8-100", cli, srv, "abc", "abc") == [] and any(x.startswith("gate-succ-dump") for x in pp.row_problems56("P640", "c8-100", cli, srv, "abc", "abc"))
      and any(x.startswith("gate-succ-dump") for x in pp.row_problems56("P640D", "c8-100", *ends(640, d=0), "abc", "abc")))
cli, srv = ends(640)
check("seat sha: a foreign binary or an uncheckable comparison is a problem, never a silent pass",
      any(x.startswith("bin-sha") for x in pp.row_problems56("P640", "c8-100", cli, srv, "def", "abc")) and any("uncheckable" in x for x in pp.row_problems56("P640", "c8-100", cli, srv, "abc", None)))
check("an arm not in its cell's pool list (P512 at c8) is an unknown-arm problem", any(x.startswith("unknown-arm") for x in pp.row_problems56("P512", "c8-100", *ends(512), "abc", "abc")))
check("succdump_stats sums the [SUCCDUMP] batches' n and bytes and reads the cap line",
      pp.succdump_stats(["[SUCCDUMP] n=3 d=o,1;o,2;r,3", "x", "[SUCCDUMP] n=2 d=a,1;x,2", "[SUCCDUMP-CAP] dumped=5"])["records"] == 5
      and pp.succdump_stats(["[SUCCDUMP] n=3 d=o,1;o,2;r,3"])["bytes"] == len("[SUCCDUMP] n=3 d=o,1;o,2;r,3") + 1 and pp.succdump_stats(["[SUCCDUMP-CAP] dumped=5"])["capped"] == 5)

# replace56 (the battery's replacement rule)
with tempfile.TemporaryDirectory() as td:
    led = os.path.join(td, "ld.log")

    def put(*rows):
        with open(led, "w") as fh:
            for r in rows:
                fh.write("P38ROW " + json.dumps(r) + "\n")

    def row(arm, cell="c8-100", seed="42", rep=1, status="LIVE", problems=(), rec=0):
        return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "status": status, "problems": list(problems), "succdump": {"records": rec}}
    void = row("P384", status="WITNESS-FAIL", problems=["gate-pool-cli=2048 (want 384)"])
    put(void)
    r1 = pp.replace56(led, "c8-100", "P384", "42", 1)
    put(void, row("P384"))
    r2 = pp.replace56(led, "c8-100", "P384", "42", 1)
    put(void, void)
    r3 = pp.replace56(led, "c8-100", "P384", "42", 1)
    put(row("P384"))
    r4 = pp.replace56(led, "c8-100", "P384", "42", 1)
    put(row("P640D", rec=99999))
    r5 = pp.replace56(led, "c8-100", "P640D", "42", 1)
    put(row("P640D", rec=100000))
    r6 = pp.replace56(led, "c8-100", "P640D", "42", 1)
    put(row("P640D", rec=10), row("P640D", rec=10))
    r7 = pp.replace56(led, "c8-100", "P640D", "42", 1)
    put(row("P640", rec=0))
    r8 = pp.replace56(led, "c8-100", "P640", "42", 1)
    put(row("P384", status="WITNESS-FAIL", problems=["bin-sha=abc (want def for CAND)"]))
    r9 = pp.replace56(led, "c8-100", "P384", "42", 1)
    cap = []
    import io
    from contextlib import redirect_stdout
    bf = io.StringIO()
    with redirect_stdout(bf):
        put(void)
        pp.replace56(led, "c8-100", "P384", "42", 1)
    check("replace56: a first row with a VOID pool echo -> 'pool-echo' (rc 0); once replaced (two rows) or twice void: no third run (rc 1); a LIVE row: no run; a foreign-sha VOID is not replaced",
          r1 == 0 and bf.getvalue().strip() == "pool-echo" and r2 == 1 and r3 == 1 and r4 == 1 and r9 == 1, (r1, r2, r3, r4, r9))
    check("replace56: the dump row under 100 000 records -> re-run once (99 999 yes, 100 000 no, two rows no); a plain arm is never re-run for records", r5 == 0 and r6 == 1 and r7 == 1 and r8 == 1, (r5, r6, r7, r8))


# ═══ 3. calibration on section 54b's B640 rows ═══
S54B = os.path.join(HERE, "..", "..", "docs", "l1-raw", "s54", "rep54b-diag-ld.tgz")
LITTLE = os.path.join(HERE, "..", "..", "docs", "derivations", "store-law", "little54b.out")
if os.path.exists(S54B) and os.path.exists(LITTLE):
    ms = ps.s54b_rows(S54B)
    check("calibration: section 54b's B640 rows: 6 rows, every one bound (bind fraction 1.0 at cap 1280), the smoke dir's same-named rows not mixed in",
          len(ms) == 6 and all(x["bind_frac"] == 1.0 and x["cap_mean"] == 1280 for x in ms), len(ms))
    ref = ps.ref_stats(ms)
    ranges = {}
    for ln in open(LITTLE, encoding="utf-8"):
        mm = re.match(r"^(r0|r1|R|Tbar_ms|rtt1)\t(.*)$", ln)
        if mm:
            ranges[mm.group(1)] = [tuple(int(v) for v in re.findall(r"-?\d+", seg)) for seg in mm.group(2).split("\t")][2]       # the B640 column
    mine = {"r0": ref["r0"], "r1": ref["r1"], "R": ref["tx_sum_late"], "Tbar_ms": ref["Tbar_ms"], "rtt1": ref["rtt_late1"]}
    check("calibration: the scorer's B640 min..max equal little54b.out's B640 column (r0, r1, R, T-bar, rtt1) to the printed integer",
          all((int(round(v[1])), int(round(v[2]))) == ranges[k] for k, v in mine.items()), {k: ((int(round(v[1])), int(round(v[2]))), ranges[k]) for k, v in mine.items()})
    check("calibration: the registration's B640 ranges (sum 11 629-11 732, r0 9 776-9 840, r1 1 839-1 913, T-bar 103-107, goodput 106.5-107.6, in-order late 0.34-0.39, old holes 0)",
          (round(ref["tx_sum_late"][1]), round(ref["tx_sum_late"][2])) == (11629, 11732) and (round(ref["r0"][1]), round(ref["r0"][2])) == (9776, 9840) and (round(ref["r1"][1]), round(ref["r1"][2])) == (1839, 1913)
          and (round(ref["Tbar_ms"][1]), round(ref["Tbar_ms"][2])) == (103, 107) and (round(ref["goodput_mbps"][1], 1), round(ref["goodput_mbps"][2], 1)) == (106.5, 107.6)
          and (round(ref["inorder_mean_late"][1], 2), round(ref["inorder_mean_late"][2], 2)) == (0.34, 0.39) and ref["old_holes"][:3] == (0.0, 0.0, 0.0), {k: v for k, v in ref.items()})
    check("re-basing tolerances: tol = max(full range, 1% of the median): r1 74 (the registration's example), in-order late 0.05 (the registration's 0.05 is 0.346-0.388 rounded; 0.046 here), "
          "sum rate 117 = the 1% floor (the registration's example reads 103 = the bare range 11 732 - 11 629, below its own floor)",
          near(ref["r1"][3], 74.4, 0.2) and near(ref["inorder_mean_late"][3], 0.046, 0.005) and near(ref["tx_sum_late"][3], 117.0, 0.5) and 102 < ref["tx_sum_late"][2] - ref["tx_sum_late"][1] < 104
          and near(ref["tx_sum_late"][3], 0.01 * ref["tx_sum_late"][0], 0.01) and ref["old_holes"][3] == 0.0, {k: v[3] for k, v in ref.items()})
    # a control equal to B640 re-bases nothing; a shifted one re-bases exactly the shifted readouts, by the offset
    ctrl = [{"cell": "c8-100", "arm": "P640", "seed": "42", "rep": i, "status": "LIVE", "problems": [], "dnf": False, "t56": dict(x)} for i, x in enumerate(ms)]
    buf = []
    offs0 = ps.rebase(ctrl, ref, buf.append)
    check("re-basing: the c8 control P640 built from section 54b's own B640 rows re-bases nothing (every readout within tol)", offs0 == {} and "REBASE-NEEDED: NO" in "\n".join(buf), "\n".join(buf)[-400:])
    shifted = [{**r, "t56": {**r["t56"], "tx_sum_late": r["t56"]["tx_sum_late"] + 300.0, "r1": r["t56"]["r1"] + 40.0}} for r in ctrl]
    buf = []
    offs1 = ps.rebase(shifted, ref, buf.append)
    check("re-basing: +300/s on the sum rate re-bases it (offset +300; tol 117), +40/s on r1 does not (tol 74); the rule reads medians, not min-max",
          set(offs1) == {"tx_sum_late"} and near(offs1["tx_sum_late"], 300.0, 0.01) and any("r1" in ln and "within tol" in ln for ln in buf), (offs1, buf))
    one_out = [{**ctrl[0], "t56": {**ctrl[0]["t56"], "tx_sum_late": 12500.0}}] + ctrl[1:]
    check("re-basing: ONE outlier row among six does not move the median past tol (a min-max rule would fire)", ps.rebase(one_out, ref, lambda *_: None) == {})
    # whole-report calibration: the B640 rows as the control of a synthetic sweep reproduce the bands' control column IN
    arms_c = {("c8-100", "P640"): ctrl}
    buf = []
    ps.band_report(arms_c, {}, buf.append)
    txt = "\n".join(buf)
    check("the C8 bands' control column (the B640 ranges) holds the B640 rows: sum rate, r0, r1, T-bar, goodput, in-order late and old holes all IN at W 1280",
          all(re.search(rf"{re.escape(lab)}\s+.*?W1280: \S+ IN", txt) for lab in ("sum rate R /s", "r0 /s", "r1 /s", "T-bar ms", "goodput Mbit/s", "in-order late mean s", "old holes")), txt[:1200])
else:
    print("SKIP calibration: docs/l1-raw/s54/rep54b-diag-ld.tgz or docs/derivations/store-law/little54b.out missing")


# ═══ 4. treatment rules, falsifiers, the standing queue, the report ═══
def mk(cell, pool, seed="42", rep=1, status="LIVE", dnf=False, arm=None, problems=(), **t):
    n = pp.NLIVE56[cell]
    base = {"goodput_mbps": 100.0, "tail_s": 1.0, "H0": 0, "tx_sum_late": 11000.0, "r0": 9500.0, "r1": 1500.0 if n == 2 else None, "rtt_late0": 96.0, "rtt_late1": 140.0 if n == 2 else None,
            "rtp_over0": 5.0, "rtp_over1": 0.0 if n == 2 else None, "old_holes": 0.0, "resid_late": 10.0, "resid_rel": 0.01, "cap_med": float(n * pool), "cap_mean": float(n * pool),
            "bind_frac": 1.0, "Tbar_ms": 100.0, "T0_ms": 50.0, "ell_ms": 15.0, "inorder_mean_late": 0.3, "qc_share": 0.0, "rx_dup": 1, "retx_per_gb": 1, "cpu_cli_s_per_gb": 1.0,
            "cpu_srv_s_per_gb": 1.0, "TW_ms": 100.0, "rtp_boot0": 12.0, "rtp_late0": 17.0, "rtp_boot1": 34.0 if n == 2 else None, "rtp_late1": 34.0 if n == 2 else None,
            "q_late0": 10, "q_late1": 1 if n == 2 else None, "late_n": 100}
    base.update(t)
    return {"cell": cell, "arm": arm or "P%d" % pool, "seed": seed, "rep": rep, "status": status, "problems": list(problems), "rc": 0, "dnf": dnf, "shares": [0.8, 0.2],
            "locked": False, "one_leg": False, "succdump": {"records": 0, "bytes": 0, "cli": {"records": 0}, "srv": {"records": 0}}, "t56": base}


def arms_of(rows):
    return ps.by_arm(ps.counting(rows)[0])


def states(arms, sq=None, dump=()):
    return {i: (s, d) for i, _, s, d in ps.falsifiers(arms, sq, dump)}


# classification
v = mk("c8-100", 384, status="WITNESS-FAIL", problems=["gate-pool-cli=2048 (want 384)"])
r2 = mk("c8-100", 384)
cnt_rows, replaced = ps.counting([v, r2, mk("c8-100", 256)])
check("a slot's LAST row counts and the earlier VOID one is listed as replaced; a VOID counting row is excluded from the arm tables",
      len(cnt_rows) == 2 and replaced == [v] and ps.by_arm(ps.counting([v])[0]) == {} and len(ps.by_arm(cnt_rows)[("c8-100", "P384")]) == 1)
check("UNBOUND: a non-control row under bind fraction 0.95 is UNBOUND; exactly 0.95 is bound; the c8 control (640), c2 / c1s control (2048) and the dump row (pool 640) are exempt at any bind",
      ps.is_unbound(mk("c8-100", 384, bind_frac=0.94)) and not ps.is_unbound(mk("c8-100", 384, bind_frac=0.95)) and not ps.is_unbound(mk("c8-100", 640, bind_frac=0.1))
      and not ps.is_unbound(mk("c1s-400", 2048, bind_frac=0.7)) and not ps.is_unbound(mk("c2-100", 2048, bind_frac=0.0)) and not ps.is_unbound(mk("c8-100", 640, arm="P640D", bind_frac=0.0))
      and ps.is_unbound(mk("c1s-400", 512, bind_frac=None)), None)
check("a VOID row is never UNBOUND (it is excluded, not labelled)", not ps.is_unbound(mk("c8-100", 384, status="WITNESS-FAIL", bind_frac=0.1)))
check("DNF: goodput reads 0 whatever t56 holds; the other readouts stay", ps.gp(mk("c8-100", 384, dnf=True, goodput_mbps=77.0)) == 0.0 and ps.gp(mk("c8-100", 384, goodput_mbps=77.0)) == 77.0
      and ps.tv(mk("c8-100", 384, dnf=True), "r0") == 9500.0)
check("quinn-collapse flag: c1s only, strictly above 0.10 of the windows (0.11 flagged, 0.10 and 0.09 not; a c2 row with 0.5 is not a c1s flag)",
      ps.qc_flagged(mk("c1s-400", 512, qc_share=0.11)) and not ps.qc_flagged(mk("c1s-400", 512, qc_share=0.10)) and not ps.qc_flagged(mk("c1s-400", 512, qc_share=0.09)) and not ps.qc_flagged(mk("c2-100", 192, qc_share=0.5)))

# F1 - F4 (c8)
def c8_arms(over=()):
    """A c8 sweep at the registration's bands: control R 11700 / r1 1880, P384 R 11500, P160 r1 900, P112 r0 9600, rtp over small; override per (pool, key)."""
    spec = {640: dict(tx_sum_late=11700.0, r0=9810.0, r1=1880.0, rtp_over0=30.0), 384: dict(tx_sum_late=11500.0, r0=9800.0, r1=1750.0, rtp_over0=20.0),
            256: dict(tx_sum_late=11300.0, r0=9760.0, r1=1600.0, rtp_over0=15.0), 160: dict(tx_sum_late=10600.0, r0=9700.0, r1=900.0, rtp_over0=6.0),
            112: dict(tx_sum_late=9900.0, r0=9600.0, r1=300.0, rtp_over0=4.0), 80: dict(tx_sum_late=9000.0, r0=9000.0, r1=200.0, rtp_over0=3.0)}
    for (pool, k), val in dict(over).items():
        spec[pool][k] = val
    return [mk("c8-100", p, **kw) for p, kw in spec.items()]


check("F1: r1 at W 320 (P160) >= 0.8 x r1 at 1280 FIRES at equality (1504 vs 0.8 x 1880 = 1504), not just below; 900 does not fire",
      states(arms_of(c8_arms()))[1][0] == ps.NOFIRE and states(arms_of(c8_arms({(160, "r1"): 1504.0})))[1][0] == ps.FIRES and states(arms_of(c8_arms({(160, "r1"): 1503.9})))[1][0] == ps.NOFIRE)
check("F2: sum rate at 768 (P384) < 0.97 x control: 11 349 (= 0.97 x 11 700) does not fire (strict), 11 348 fires", states(arms_of(c8_arms({(384, "tx_sum_late"): 11349.0})))[2][0] == ps.NOFIRE
      and states(arms_of(c8_arms({(384, "tx_sum_late"): 11348.0})))[2][0] == ps.FIRES)
check("F3: r0 at W 224 (P112) < 0.97 x 9 800 = 9 506: 9 506 does not fire (strict), 9 505 fires", states(arms_of(c8_arms({(112, "r0"): 9506.0})))[3][0] == ps.NOFIRE
      and states(arms_of(c8_arms({(112, "r0"): 9505.0})))[3][0] == ps.FIRES)
check("F4: rtp0 - boot at W <= 320 (P160, P112, P80) > 10 ms: 10.0 does not fire (strict), 10.1 on any of the three fires; the control's 30 ms is not read",
      states(arms_of(c8_arms({(160, "rtp_over0"): 10.0})))[4][0] == ps.NOFIRE and states(arms_of(c8_arms({(80, "rtp_over0"): 10.1})))[4][0] == ps.FIRES
      and states(arms_of(c8_arms({(112, "rtp_over0"): 10.1})))[4][0] == ps.FIRES)
unb = [mk("c8-100", 640, tx_sum_late=11700.0, r1=1880.0)] + [mk("c8-100", 160, bind_frac=0.5, cap_mean=300.0, r1=1900.0, r0=9700.0, tx_sum_late=11600.0)]
st_u = states(arms_of(unb))
check("an UNBOUND arm is not read at its nominal W: F1 on a P160 whose only rows are UNBOUND is UNSCOREABLE (not FIRES on a row that measured another cap)", st_u[1][0] == ps.UNSC and "unbound 1" in st_u[1][1], st_u[1])
check("a missing arm is UNSCOREABLE, never DOES-NOT-FIRE: F1-F4 on a control-only c8", all(states(arms_of([mk("c8-100", 640)]))[i][0] == ps.UNSC for i in (1, 2, 3)) and states(arms_of([mk("c8-100", 640)]))[4][0] == ps.UNSC)
# F5, F6
check("F5: |W - sum r_i rtt_i - old holes| > 0.1 W at a bound W >= 320: resid 10 vs old holes 0 on W 1280 is fine; resid 10, old holes 140 (|10 - 140| = 130 > 128) fires; W < 320 is not read",
      states(arms_of(c8_arms()))[5][0] == ps.NOFIRE and states(arms_of([mk("c8-100", 640, old_holes=140.0)]))[5][0] == ps.FIRES and states(arms_of([mk("c8-100", 640, old_holes=130.0)]))[5][0] == ps.NOFIRE
      and states(arms_of([mk("c8-100", 80, old_holes=500.0, resid_late=0.0)]))[5][0] == ps.UNSC)
check("F6: old holes > 50 at any W <= 1280: 50 does not fire (strict), 51 fires; a c1s arm at W 2048 is outside the range", states(arms_of([mk("c8-100", 384, old_holes=50.0)]))[6][0] == ps.NOFIRE
      and states(arms_of([mk("c8-100", 384, old_holes=51.0)]))[6][0] == ps.FIRES and states(arms_of([mk("c1s-400", 2048, old_holes=900.0)]))[6][0] == ps.UNSC)
# F7, F8
def c2_arms(g192, dnf192=False):
    return [mk("c2-100", 2048, goodput_mbps=100.0), mk("c2-100", 192, goodput_mbps=g192, dnf=dnf192)]


check("F7: c2 goodput at 192 < 0.95 x control: 95.0 does not fire (strict), 94.9 fires; a DNF at 192 counts as 0 and fires; a missing control is UNSCOREABLE",
      states(arms_of(c2_arms(95.0)))[7][0] == ps.NOFIRE and states(arms_of(c2_arms(94.9)))[7][0] == ps.FIRES and states(arms_of(c2_arms(99.0, dnf192=True)))[7][0] == ps.FIRES
      and states(arms_of([mk("c2-100", 192)]))[7][0] == ps.UNSC)


def c1s_arms(n512, flagged=0, g512=94.0, g2048=100.0, gflag=10.0):
    rows = [mk("c1s-400", 2048, rep=i, goodput_mbps=g2048) for i in range(1, 7)]
    rows += [mk("c1s-400", 512, rep=i, goodput_mbps=(gflag if i <= flagged else g512), qc_share=(0.5 if i <= flagged else 0.0)) for i in range(1, n512 + 1)]
    return rows


s8 = states(arms_of(c1s_arms(6)))[8]
check("F8: c1s goodput at 512 >= 0.95 x control FIRES at equality (95.0 vs 95.0), 94.9 does not", states(arms_of(c1s_arms(6, g512=95.0)))[8][0] == ps.FIRES and states(arms_of(c1s_arms(6, g512=94.9)))[8][0] == ps.NOFIRE)
f8_flag = states(arms_of(c1s_arms(6, flagged=2, g512=96.0, gflag=10.0)))[8]
check("F8 excludes the quinn-collapsed rows and states the count: two collapsed P512 rows at 10 Mbit/s would drag the median under 0.95 x control, excluded they leave 96 -> FIRES, 'excluded 2' printed",
      f8_flag[0] == ps.FIRES and "quinn-collapse excluded 2" in f8_flag[1] and states(arms_of(c1s_arms(6, flagged=0, g512=96.0)))[8][0] == ps.FIRES and
      sorted(ps.gp(r) for r in arms_of(c1s_arms(6, flagged=2, g512=96.0))[("c1s-400", "P512")])[:2] == [10.0, 10.0], f8_flag)
check("F8 with every P512 row collapsed is UNSCOREABLE (nothing left to read), with the exclusion count", states(arms_of(c1s_arms(3, flagged=3)))[8][0] == ps.UNSC and "excluded 3" in states(arms_of(c1s_arms(3, flagged=3)))[8][1])
# F9
f9 = states(arms_of([mk("c8-100", 640)]), None, [dict(mk("c8-100", 640, arm="P640D"), succdump={"records": 123456})])[9]
check("F9 is UNSCOREABLE by the scorer: the dump holds (outcome, us) records only -- no path, no seq -- and the reason and the dump size are printed", f9[0] == ps.UNSC and "no path and no seq" in f9[1] and "123456" in f9[1], f9)
# F10 and the standing queue
def sq_cell(cell, spec, r=(9000.0, 1000.0), extra=None):
    """Arms for the standing-queue tests: spec = {pool: (goodput, rtt0, rtt1)} with fixed per-leg rates r."""
    rows = [mk(cell, p, goodput_mbps=g, r0=r[0], r1=r[1], tx_sum_late=sum(r), rtt_late0=a, rtt_late1=b, Tbar_ms=(r[0] * a + r[1] * b) / sum(r), ell_ms=10.0) for p, (g, a, b) in spec.items()]
    return arms_of(rows + (extra or []))


SPEC8 = {640: (100.0, 104.0, 150.0), 384: (97.0, 64.0, 90.0), 256: (90.0, 45.0, 70.0), 160: (70.0, 30.0, 60.0), 80: (60.0, 20.0, 50.0)}
a8 = sq_cell("c8-100", SPEC8)
q8 = ps.standing_queue(a8, "c8-100")
# per-leg floors = the lowest arm-median loop per leg: (20, 50) from the smallest arm. T0 with these fixed rates = (9000 x 20 + 1000 x 50) / 10000 = 23 ms at every arm.
# q(arm) = (9000 (rtt0 - 20) + 1000 (rtt1 - 50)) / 10000: P256 24.5, P384 43.6. Goodput by W: (160, 60) (320, 70) (512, 90) (768, 97) (1280, 100): the 0.95 point is at f = 5/7 between W 512 and 768.
w95 = 512 + 5 / 7 * (768 - 512)
tb95 = 47.5 + 5 / 7 * (66.6 - 47.5)
q95 = 24.5 + 5 / 7 * (43.6 - 24.5)
p95 = q8["points"][0.95]
check("T0 is per leg: the floors are the lowest arm-median loop of each leg (20 and 50 ms), not a first-second statistic", q8["floors"] == {0: 20.0, 1: 50.0}, q8["floors"])
check("standing queue: the W where the arm-median goodput first reaches 0.95 x control is interpolated linearly in W (512 + 5/7 x 256) with T-bar and q interpolated alongside; T0 = T-bar - q = 23 ms "
      "(the floors weighted by the arm's own rates), q/ell follows", near(p95[0], w95, 0.01) and near(p95[1], tb95, 0.01) and near(p95[2], q95, 0.01) and near(p95[3], q95 / 10.0, 0.001)
      and p95[4] == "interpolated" and near(p95[5], 23.0, 0.01), p95)
check("T0 <= T-bar at EVERY arm by construction: every point of the curve has q >= 0 (never a negative queue), the smallest arm has q = 0", all(q >= -1e-9 for _, _, _, q in q8["curve"]) and near(q8["curve"][0][3], 0.0), q8["curve"])
p98 = q8["points"][0.98]
check("standing queue: 0.98 x control (98.0) lies between W 768 (97) and 1280 (100): interpolated there", 768 < p98[0] < 1280 and near(p98[0], 768 + (98 - 97) / (100 - 97) * (1280 - 768), 0.01), p98)
# the leg mix cannot masquerade as a queue: leg 1's share collapses at small W (r1 100 /s) -- the weighted T0 follows the arm's own rates
a_mix = sq_cell("c8-100", {640: (100.0, 104.0, 150.0), 80: (60.0, 20.0, 50.0)}, extra=[mk("c8-100", 80, goodput_mbps=60.0, r0=9900.0, r1=100.0, tx_sum_late=10000.0, rtt_late0=20.0, rtt_late1=50.0, rep=2)])
check("a small-W arm whose leg-1 share collapsed still reads q ~ 0 (the arm's own rates weight the floors); a fixed-rate T0 would read it as a queue", near(ps.standing_queue(a_mix, "c8-100")["curve"][0][3], 0.0, 0.5), ps.standing_queue(a_mix, "c8-100")["curve"])
a_flat = sq_cell("c8-100", {640: (100.0, 104.0, 150.0), 80: (99.0, 20.0, 50.0)})
check("standing queue: when the smallest W already reaches the threshold the point is reported AT that W and says so (the knee is at or below it)", ps.standing_queue(a_flat, "c8-100")["points"][0.95][4].startswith("at the smallest W"))
rows_ub = [mk("c8-100", 640, goodput_mbps=100.0), mk("c8-100", 384, goodput_mbps=96.0)] + [mk("c8-100", 160, goodput_mbps=50.0, bind_frac=0.4, cap_mean=500.0)]
cv = ps.curve(arms_of(rows_ub), "c8-100")
check("an UNBOUND row is a curve point at its REALISED cap (W 500, not 320), the bound arms at their own W", [round(w) for w, *_ in cv] == [500, 768, 1280], cv)
mult = lambda c8, c2, c1s: states({}, {"c8-100": {"points": {0.95: (0, 0, 0, c8, "", 0)}}, "c2-100": {"points": {0.95: (0, 0, 0, c2, "", 0)}}, "c1s-400": {"points": {0.95: (0, 0, 0, c1s, "", 0)}}}, [])[10]
check("F10: the multiples (T-bar - T0)/ell at the 0.95 point across c1s, c2, c8: max/min > 3 FIRES (3.01), = 3 does not (strict); a non-positive multiple or a missing cell is UNSCOREABLE",
      mult(1.0, 2.0, 3.01)[0] == ps.FIRES and mult(1.0, 2.0, 3.0)[0] == ps.NOFIRE and mult(0.5, 1.0, 1.4)[0] == ps.NOFIRE and mult(-0.1, 1.0, 1.4)[0] == ps.UNSC
      and states({}, {"c8-100": {"points": {0.95: None}}}, [])[10][0] == ps.UNSC)

# the smoke criteria
def smoke_ledger(td, mut=None):
    rows = []
    for c, _, a, sd, rp in pp.plan56(smoke=True):
        pool, dump = pp.arm_pool56(a)
        r = mk(c, pool, arm=a, rep=rp)
        r.update({"rc": 0, "summary": True, "seconds": 50.0, "bytes": pp.BYTES56[c], "mbps": 90.0, "wall_s": 60, "gate_pool_cli": str(pool), "gate_pool_srv": str(pool),
                  "pool_info_cli": str(pool), "pool_info_srv": str(pool), "seed": sd, "succdump": {"records": 120000 if dump else 0, "bytes": 1000, "capped": None, "cli": {"records": 0}, "srv": {"records": 120000 if dump else 0}}})
        if mut:
            mut(r)
        rows.append(r)
    path = os.path.join(td, "smoke.log")
    with open(path, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    return path


with tempfile.TemporaryDirectory() as td:
    rc, txt = capture(ps.smoke, smoke_ledger(td))
    check("smoke green: four rows, every criterion PASS, SMOKE-PASS, rc 0", rc == 0 and "SMOKE-PASS" in txt and "FAIL" not in txt and txt.count("PASS ") >= 7, txt[-700:])
    rc, txt = capture(ps.smoke, smoke_ledger(td, lambda r: r.update({"status": "WITNESS-FAIL", "problems": ["gate-pool-cli=2048 (want 384)"]}) if r["arm"] == "P384" else None))
    check("smoke red: a pool-echo failure -> FAIL on the pool criterion, rc 1", rc == 1 and re.search(r"^FAIL each row's pool is in \[GATES\]", txt, re.M) is not None, txt[-500:])
    rc, txt = capture(ps.smoke, smoke_ledger(td, lambda r: r["succdump"].update({"records": 0}) if r["arm"] == "P640D" else None))
    check("smoke red: a dump row with no record -> FAIL on the dump criterion", rc == 1 and re.search(r"^FAIL the dump row wrote a raw dump", txt, re.M) is not None, txt[-500:])
    rc, txt = capture(ps.smoke, smoke_ledger(td, lambda r: r["t56"].update({"Tbar_ms": None}) if r["cell"] == "c2-100" else None))
    check("smoke red: a missing LATE readout (T-bar None) -> FAIL on the readout criterion, naming it", rc == 1 and "Tbar_ms" in txt and re.search(r"^FAIL the LATE-window readouts", txt, re.M) is not None, txt[-500:])
    rc, txt = capture(ps.smoke, smoke_ledger(td, lambda r: r.update({"seconds": 300.0}) if r["cell"] == "c1s-400" else None))
    check("smoke red: a row at or over RWM_PERF_TIMEOUT_S=300 -> FAIL on the timeout criterion", rc == 1 and re.search(r"^FAIL every smoke row finished inside RWM_PERF_TIMEOUT_S=300", txt, re.M) is not None, txt[-500:])
    rc, txt = capture(ps.smoke, smoke_ledger(td, lambda r: r.update({"bytes": 400000000}) if r["cell"] == "c1s-400" else None))
    check("smoke red: c1s at 400 MB instead of 5 GB -> FAIL (the cell's size)", rc == 1 and "bytes=400000000" in txt, txt[-500:])
    rc, txt = capture(ps.smoke, smoke_ledger(td, lambda r: r.update({"problems": ["bin-sha=ab (want cd for CAND)"], "status": "WITNESS-FAIL"})))
    check("smoke red: a foreign binary -> FAIL on the seat criterion", rc == 1 and re.search(r"^FAIL every row ran the one seat's binary", txt, re.M) is not None, txt[-400:])

# a whole report on a synthetic ledger: replaced slot, VOID, UNBOUND, DNF, collapse flag, the dump row
with tempfile.TemporaryDirectory() as td:
    rows = []
    rows += [mk("c8-100", p, goodput_mbps=g, rep=1) for p, g in ((640, 100.0), (384, 98.0), (256, 92.0), (160, 75.0), (112, 60.0), (80, 40.0))]
    rows += [mk("c8-100", 384, status="WITNESS-FAIL", problems=["gate-pool-cli=2048 (want 384)"], seed="7", rep=2), mk("c8-100", 384, seed="7", rep=2, goodput_mbps=97.0)]      # replaced once
    rows += [mk("c8-100", 112, seed="7", rep=2, dnf=True, goodput_mbps=None)]
    rows += [mk("c2-100", p, goodput_mbps=g) for p, g in ((2048, 100.0), (384, 100.0), (192, 97.0), (160, 90.0), (128, 75.0))]
    rows += [mk("c1s-400", 2048, rep=i, goodput_mbps=100.0, bind_frac=0.7) for i in (1, 2)] + [mk("c1s-400", 512, rep=1, goodput_mbps=85.0), mk("c1s-400", 512, rep=2, goodput_mbps=20.0, qc_share=0.4),
                                                                                              mk("c1s-400", 384, goodput_mbps=80.0, bind_frac=0.5, cap_mean=300.0)]
    d = mk("c8-100", 640, arm="P640D", rep=5)
    d["succdump"] = {"records": 150000, "bytes": 1234567, "capped": None, "cli": {"records": 0}, "srv": {"records": 150000}}
    rows.append(d)
    led = os.path.join(td, "ld.log")
    with open(led, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    buf = []
    ps.score([led], out=buf.append, s54b_src=os.path.join(td, "absent.tgz"))
    sc = "\n".join(buf)
    check("report: VERDICT56 counts the slots, the replaced row, the DNF, the UNBOUND rows (P384 at c1s) and the collapse flag; the dump row's records and bytes are printed",
          re.search(r"^VERDICT56 REPORTED-ONLY slots=\d+/64 live=\d+ void=0 replaced=1 dnf=1 unbound=1 qcollapse=1 falsifiers=F1:\S+,F2:\S+", sc, re.M) is not None
          and "records 150000" in sc and "1234567 bytes" in sc, [ln for ln in buf if ln.startswith("VERDICT56")])
    check("report: the replaced row and the UNBOUND / QCOLLAPSE / DNF tags are listed, the control is exempt from UNBOUND (c1s P2048 binds 0.7: not tagged), F9 is UNSCOREABLE",
          "REPLACED c8-100 P384 s7 rep2 WITNESS-FAIL" in sc and re.search(r"c1s-400\s+P384\s+s42\s+r1 .*UNBOUND", sc) and re.search(r"c1s-400\s+P512\s+s42\s+r2 .*QCOLLAPSE", sc) and re.search(r"c8-100\s+P112\s+s7\s+r2 .*DNF", sc)
          and not re.search(r"c1s-400\s+P2048\s+s\d+.*UNBOUND", sc) and re.search(r"^  F9 UNSCOREABLE", sc, re.M), sc[-1500:])
    check("report: the standing-queue block, the re-basing line (unavailable without the s54b logs), the band report and the prediction blocks are printed, each falsifier line carries a state",
          "STANDING QUEUE AT THE KNEE" in sc and "REBASE-CHECK UNAVAILABLE" in sc and "C8 BANDS" in sc and "C2 PREDICTIONS" in sc and "C1S PREDICTIONS" in sc and "ANCHOR" in sc
          and all(re.search(r"^  F%d (FIRES|DOES-NOT-FIRE|UNSCOREABLE) " % i, sc, re.M) for i in range(1, 11)), sc[-800:])

# the CLI plumbing the run_all stage uses
lp = open(os.path.join(HERE, "loopdiag_parse.py"), encoding="utf-8").read()
check("loopdiag_parse dispatches smoke56 / check56 / score56 (the names the stage calls)", all(f'cmd == "{c}"' in lp for c in ("smoke56", "check56", "score56")))
with tempfile.TemporaryDirectory() as td:
    led = os.path.join(td, "ld.log")
    slots56 = pp.plan56()
    with open(led, "w") as fh:
        for c, _, a, sd, rp in slots56:
            fh.write("P38ROW " + json.dumps({"cell": c, "arm": a, "seed": sd, "rep": rp, "status": "LIVE", "problems": []}) + "\n")
        fh.write("LD-BATTERY-DONE tag=ld\n")
    ok_all = pp.check56(led)
    with open(led, "w") as fh:
        for c, _, a, sd, rp in slots56[:-1]:
            fh.write("P38ROW " + json.dumps({"cell": c, "arm": a, "seed": sd, "rep": rp, "status": "LIVE", "problems": []}) + "\n")
        fh.write("LD-BATTERY-DONE tag=ld\n")
    ok_short = pp.check56(led)
    with open(led, "w") as fh:
        for c, _, a, sd, rp in slots56:
            fh.write("P38ROW " + json.dumps({"cell": c, "arm": a, "seed": sd, "rep": rp, "status": "LIVE", "problems": []}) + "\n")
    ok_nodone = pp.check56(led)
check("check56: rc 0 iff the battery finished and every slot of the table has a row; 63 of 64 slots, or no LD-BATTERY-DONE, is rc 1", ok_all == 0 and ok_short == 1 and ok_nodone == 1, (ok_all, ok_short, ok_nodone))

# ═══ 4b. review additions: the tick's own n_live, F8's minimum, F10's T0-spread rule ═══
def drop_to_one_leg(lines, t_from):
    """The same c8 lines with one live path (np=1, cap = 1 x pool = 640) from t_from on."""
    out = []
    for ln in lines:
        mt = re.match(r"\[DIAG\] t=([0-9.]+)s ", ln)
        if mt and float(mt.group(1)) >= t_from:
            ln = ln.replace(" np=2 np_act=2", " np=1 np_act=1").replace("/1280 ", "/640 ")
        out.append(ln)
    return out


mnl = ps.row_metrics(drop_to_one_leg(C8L, 28.0), [], [], 600000000, "c8-100", 640)
check("n_live per tick: with one live path from 28 s on (cap 640 = 1 x pool) the LATE share with n_live < N is read (~ 49 of 100 sending ticks), and the row still BINDS (bind judged against the tick's n_live x pool, "
      "where a fixed N x pool = 1280 would read ~0.5)", 0.4 < mnl["nlive_lt_share"] < 0.6 and mnl["bind_frac"] == 1.0, (mnl["nlive_lt_share"], mnl["bind_frac"], mnl["nlive_med"]))
check("n_live: a row at two live paths throughout reads median 2 and share 0; a single-path cell reads 1 and 0", m8["nlive_med"] == 2 and m8["nlive_lt_share"] == 0.0
      and ps.row_metrics(run_lines(1, 603), [], [], 600000000, "c2-100", 603)["nlive_med"] == 1 and ps.row_metrics(run_lines(1, 603), [], [], 600000000, "c2-100", 603)["nlive_lt_share"] == 0.0)
check("n_live: a cap BELOW n_live x pool on every LATE tick is not a bind (0.0), two live paths", ps.row_metrics(run_lines(bound=False), [], [], 600000000, "c8-100", 640)["bind_frac"] == 0.0)


def c1s_n(n512, n2048, flagged=0):
    rows = [mk("c1s-400", 2048, rep=i, goodput_mbps=100.0) for i in range(1, n2048 + 1)]
    rows += [mk("c1s-400", 512, rep=i, goodput_mbps=96.0, qc_share=(0.5 if i <= flagged else 0.0)) for i in range(1, n512 + 1)]
    return rows


check("F8 minimum: 3 unflagged P512 rows -> UNSCOREABLE (a 6-row arm with 3 collapsed), 4 unflagged -> read; 3 unflagged control rows -> UNSCOREABLE too",
      states(arms_of(c1s_n(6, 6, flagged=3)))[8][0] == ps.UNSC and states(arms_of(c1s_n(6, 6, flagged=2)))[8][0] == ps.FIRES and states(arms_of(c1s_n(6, 3)))[8][0] == ps.UNSC
      and states(arms_of(c1s_n(3, 6)))[8][0] == ps.UNSC and "needs >= 4 unflagged" in states(arms_of(c1s_n(6, 6, flagged=3)))[8][1])


def sq_pts(dq, spread):
    return {"points": {0.95: (0, 0, dq, 1.0, "", 0)}, "T0_spread": spread, "T0_n": 6}


mk10 = lambda d1, s1, d2, s2, d3, s3: states({}, {"c1s-400": sq_pts(d1, s1), "c2-100": sq_pts(d2, s2), "c8-100": sq_pts(d3, s3)}, [])[10]
check("F10 rule: T-bar - T0 at the 0.95 point within T0's spread (max - min over the smallest-W rows) for ANY cell -> UNSCOREABLE (equal to the spread counts as within); beyond it in all three, F10 reads",
      mk10(10.0, 3.0, 12.0, 4.0, 30.0, 40.9)[0] == ps.UNSC and "within T0's spread" in mk10(10.0, 3.0, 12.0, 4.0, 30.0, 40.9)[1] and mk10(10.0, 3.0, 12.0, 4.0, 40.9, 40.9)[0] == ps.UNSC
      and mk10(10.0, 3.0, 12.0, 4.0, 41.0, 40.9)[0] == ps.NOFIRE)
a_sp = sq_cell("c8-100", {640: (100.0, 104.0, 150.0), 80: (60.0, 20.0, 50.0)}, extra=[mk("c8-100", 80, goodput_mbps=60.0, rep=i_, r0=9000.0, r1=1000.0, tx_sum_late=10000.0, rtt_late0=20.0 + 10.0 * i_, rtt_late1=50.0, Tbar_ms=23.0 + 9.0 * i_) for i_ in (2, 3)])
check("T0 spread guard = max - min over the smallest-W arm's rows of the row T-bar (23 / 41 / 50 ms -> 27 ms over 3 rows)", near(ps.standing_queue(a_sp, "c8-100")["T0_spread"], 27.0) and ps.standing_queue(a_sp, "c8-100")["T0_n"] == 3, ps.standing_queue(a_sp, "c8-100")["T0_spread"])

# a c1s collapse row near the 300 s timeout: 5 GB at ~150 Mbit/s takes ~267 s, finishes (a summary, not a DNF), is FLAGGED by its qcwnd and EXCLUDED from F8 -- never goodput 0
def c1s_collapse_rows(n_collapse, n_ok, dnf_collapse=False):
    rows = [mk("c1s-400", 2048, rep=i, goodput_mbps=400.0) for i in range(1, 7)]
    rows += [mk("c1s-400", 512, rep=i, goodput_mbps=340.0) for i in range(1, n_ok + 1)]
    rows += [mk("c1s-400", 512, rep=10 + i, goodput_mbps=(None if dnf_collapse else 150.0), qc_share=0.4, seconds=(None if dnf_collapse else 267.0), dnf=dnf_collapse) for i in range(1, n_collapse + 1)]
    return rows


cr = c1s_collapse_rows(2, 4)
collapsed = [r for r in cr if r["arm"] == "P512" and r["t56"]["qc_share"] == 0.4]
f8c = states(arms_of(cr))[8]
check("c1s collapse near the timeout: a 150 Mbit/s row (267 s < 300 s) is not a DNF (goodput 150, not 0), is flagged by its qcwnd share, and F8 excludes it with the count stated "
      "(unflagged P512 n 4, excluded 2): 340 < 0.95 x 400 -> DOES-NOT-FIRE",
      all(not r["dnf"] and ps.gp(r) == 150.0 and ps.qc_flagged(r) for r in collapsed) and f8c[0] == ps.NOFIRE and "unflagged n 4" in f8c[1] and "quinn-collapse excluded 2" in f8c[1], f8c)
dn = c1s_collapse_rows(2, 4, dnf_collapse=True)
f8d = states(arms_of(dn))[8]
check("a collapse row that does run past 300 s (a DNF, goodput 0 elsewhere) is ALSO flagged by its qcwnd and excluded from F8, not scored as 0: same verdict, same counts",
      f8d[0] == ps.NOFIRE and "unflagged n 4" in f8d[1] and "quinn-collapse excluded 2" in f8d[1] and all(ps.qc_flagged(r) for r in dn if r["dnf"]), f8d)
with_dnf_scored = states(arms_of([r for r in dn if not ps.qc_flagged(r)] + [dict(r, t56=dict(r["t56"], qc_share=0.0)) for r in dn if r["dnf"]]))[8]
check("(the red case) the same two rows WITHOUT the flag would count as goodput 0 and flip F8's median reading: with 4 healthy + 2 DNF the median is still 340 but with 3 healthy + 3 DNF it is 170", states(arms_of(
      [mk("c1s-400", 2048, rep=i, goodput_mbps=400.0) for i in range(1, 7)] + [mk("c1s-400", 512, rep=i, goodput_mbps=340.0) for i in (1, 2, 3)] + [mk("c1s-400", 512, rep=i + 10, dnf=True) for i in (1, 2, 3)]))[8][0] == ps.NOFIRE
      and with_dnf_scored[0] in (ps.NOFIRE, ps.FIRES))
cl = ps.row_metrics(run_lines(1, 2048, qc_dip=0.3), [], [], 5000000000, "c1s-400", 2048)
check("row_metrics: a c1s row whose qcwnd sits below 1/4 of its median on 30 % of the ticks reads a collapse share ~0.3 (> 0.10: flagged)", 0.25 < cl["qc_share"] < 0.35, cl["qc_share"])

# ═══ 5. the row path on REAL logs (make_row under LD_PLAN=56), the suite-reuse reference, the xp readouts, the shell details ═══
import tarfile  # noqa: E402

RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")


def real_row(tgz, stem, cell, arm, nbytes):
    """make_row on one committed row's logs as a plan-56 row (LD_PLAN=56, LD_SHA = the row's sha); None when the archive is absent."""
    path = os.path.join(RAW, tgz)
    if not os.path.exists(path):
        return None
    with tempfile.TemporaryDirectory() as td, tarfile.open(path) as tf:
        for mem in tf.getmembers():
            b = os.path.basename(mem.name)
            if mem.isfile() and b.startswith(stem) and "smoke" not in os.path.dirname(mem.name):
                with open(os.path.join(td, b), "wb") as fh:
                    fh.write(tf.extractfile(mem).read())
        old = {k: os.environ.get(k) for k in ("LD_PLAN", "LD_SHA", "LD_ROW_BYTES")}
        os.environ.update({"LD_PLAN": "56", "LD_SHA": "abc", "LD_ROW_BYTES": str(nbytes)})
        try:
            return pp.make_row(cell, arm, "42", 1, "0", "60", lc.read(os.path.join(td, stem + "-drv.out")), lc.read(os.path.join(td, stem + "-c.log")), lc.read(os.path.join(td, stem + "-s.log")), 0, "abc")
        finally:
            for k, v in old.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v


for tgz, stem, cell, arm, nb, label in (("s54/rep54b-diag-ld.tgz", "c8-100@def-B640-s42-r1-a1", "c8-100", "P640", 600000000, "a section-54b B640 row as c8 P640"),
                                        ("s54/guard-diag-ld.tgz", "c2-100@def-CAND-s42-r1-a1", "c2-100", "P2048", 600000000, "a section-54 guard c2 600 MB row as c2 P2048"),
                                        ("bbr48b/diag-ld-full.tgz", "c1s-400@def-BASE-s42-r1-a1", "c1s-400", "P2048", 5000000000, "a 5 GB c1s row (section 48b) as c1s P2048")):
    rr = real_row(tgz, stem, cell, arm, nb)
    if rr is None:
        print("SKIP real-row check: " + tgz + " missing")
        continue
    t = rr["t56"]
    check(f"real logs: {label} is LIVE with no problem (the pool echoed on both ends, RWM_SUCC_DUMP=0 on a real [GATES] line next to RWM_SUCC_DUMP_MAX, the seat's sha) and every REQUIRED readout is computed (a None would be ABORT-SMOKE on the VM)",
          rr["status"] == "LIVE" and rr["problems"] == [] and ps.missing(rr) == [] and rr["sha_checked"] is True and rr["succdump"]["records"] == 0, (rr["status"], rr["problems"], ps.missing(rr)))
    check(f"real logs: {label}: bound (cap = N x pool on every LATE tick), the LATE window is long enough (>= 100 ticks), single-path cells leave leg 1 None, rx_dup is read at every cell",
          t["bind_frac"] == 1.0 and t["cap_mean"] == pp.NLIVE56[cell] * pp.arm_pool56(arm)[0] and t["late_n"] >= 100 and (t["r1"] is None) == (cell != "c8-100") and t["rx_dup"] is not None, {k: t.get(k) for k in ("bind_frac", "cap_mean", "late_n", "r1", "rx_dup")})
r54 = real_row("s54/rep54b-diag-ld.tgz", "c8-100@def-B640-s42-r1-a1", "c8-100", "P640", 600000000)
if r54 is not None:
    t = r54["t56"]
    check("real logs: the B640 row's [HOLD] path-split counters and the receiver's [SUCC] xp_frac are read (p0 holes are overwhelmingly same-path: xp_frac < 0.1)",
          t["hold_sp0"] and t["hold_xp0"] is not None and t["hold_xpf0"] is not None and t["hold_xpf0"] < 0.1 and t["succ_xpf"] is not None, {k: t.get(k) for k in ("hold_sp0", "hold_xp0", "hold_xpf0", "succ_sp", "succ_xp", "succ_xpf")})
    check("real logs: the B640 row's late n_live is 2 on every tick (share with n_live < N = 0.0) and it binds against n_live x pool", t["nlive_med"] == 2 and t["nlive_lt_share"] == 0.0 and t["bind_frac"] == 1.0, (t["nlive_med"], t["nlive_lt_share"]))
    f9 = {i: (s_, d) for i, _, s_, d in ps.falsifiers({("c8-100", "P640"): [r54]}, None, [dict(r54, arm="P640D", succdump={"records": 150000})])}[9]
    check("F9 stays UNSCOREABLE and prints the [HOLD] p0 / [SUCC] cross-path counters beside it for the reader", f9[0] == ps.UNSC and "xp_frac" in f9[1] and "[HOLD] p0" in f9[1], f9)
mm = ps.hold_xp(["[HOLD] site=sender path=0 gen=0 fed=1 sp_n=61692 xp_n=1461 up_n=0 xp_frac=0.0231 age_n=1", "[HOLD] site=sender path=1 gen=0 sp_n=5 xp_n=7 up_n=0 xp_frac=-", "[HOLD] site=sender path=- sp_n=1 xp_n=1 up_n=0 xp_frac=0.5"])
check("hold_xp reads the last [HOLD] line of each numeric path (xp_frac '-' is None, the pooled '-' path is skipped)", mm == {0: (61692, 1461, 0.0231), 1: (5, 7, None)}, mm)
check("succ_xp reads the receiver's final [SUCC] line only", ps.succ_xp(["[SUCC] gen=0 det=9 sp_n=1 xp_n=2 xp_frac=0.6667 final=1", "[SUCC] gen=0 det=99 sp_n=9 xp_n=9 xp_frac=0.5"]) == (1, 2, 0.6667) and ps.succ_xp(["[SUCC] det=1 sp_n=1"]) is None)
check("c8 tags: LOCKOUT is section 3.9's classifier (locked), ONE-LEG the informational share < 10 % without the rtt condition (by design at small W); neither at c2",
      ps.row_tags(dict(mk("c8-100", 80), locked=True)) == ["LOCKOUT"] and ps.row_tags(dict(mk("c8-100", 80), one_leg=True)) == ["ONE-LEG"] and ps.row_tags(dict(mk("c2-100", 192), locked=True, one_leg=True)) == [])

# the suite-reuse reference manifest
refp = os.path.join(HERE, "ref56-manifest.sha256")
reflines = open(refp, encoding="utf-8", newline="").read().split("\n") if os.path.exists(refp) else []
refrows = [x for x in reflines if x]
paths_ = [x[66:] for x in refrows]
check("the reference manifest: sha256 lines `<64 hex>  <path>` for raptorpath/src and raptorpath/tests files only, in C-locale path order, LF line endings, no duplicates",
      len(refrows) > 100 and all(re.fullmatch(r"[0-9a-f]{64}  raptorpath/(src|tests)/\S+", x) for x in refrows) and paths_ == sorted(paths_) and len(set(paths_)) == len(paths_)
      and b"\r" not in open(refp, "rb").read(), (len(refrows), refrows[:1]))
import shutil  # noqa: E402

git_ok = bool(shutil.which("git")) and subprocess.run(["git", "cat-file", "-e", "7fd92dd5"], capture_output=True, cwd=HERE).returncode == 0
bash_ = next((c for c in [shutil.which("bash"), r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"] if c and "system32" not in c.lower() and os.path.exists(c)), None)      # System32\bash.exe is WSL's launcher
if git_ok and bash_:
    regen = subprocess.run([bash_, "ref_manifest.sh", "7fd92dd5"], capture_output=True, cwd=HERE).stdout
    check("the committed reference manifest is exactly what ref_manifest.sh generates from git at cand54 7fd92dd5 (the tree that passed section 54's suites)", regen == open(refp, "rb").read(), (len(regen), len(open(refp, "rb").read())))
else:
    print("SKIP the reference-manifest regeneration check: no git history (7fd92dd5) or no bash here")
runall2 = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
check("suite reuse: engine suites are guarded by ENGINE_SUITES, a skipped run writes NO TESTS-OK / SRC-MANIFEST.sha256 (TOOLS-TESTS-OK instead), the bash tests are the two of section 54, and plan 56's crlf guard covers the manifest",
      'if [ "$ENGINE_SUITES" = 1 ]; then src_manifest > "$SD/SRC-MANIFEST.sha256"; fi' in runall2 and 'touch "$SD/TOOLS-TESTS-OK"' in runall2 and "for t in test_lib_battery.sh test_abort_witness.sh; do" in runall2
      and "ref56-manifest.sha256" in re.search(r"^crlf_guard [^\n]*(?:\\\n[^\n]*)*", runall2, re.M).group(0))
check("the replacement run keeps the VOID row's per-row logs: ATTEMPT_OFF (a11 / a12) is initialised for every plan and set only around run_slot56's second run",
      shell.count("ATTEMPT_OFF=0") >= 2 and "ATTEMPT_OFF=10" in shell and shell.index("ATTEMPT_OFF=0") < shell.index("run_one() {") and "a$(( attempt + ATTEMPT_OFF ))" in shell)

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
