#!/usr/bin/env python3
"""Tests for the status 57a / 57b harness (the emission-burst block at c1s and the c1d pool sweep with the transfer test T; REPORTED ONLY, no build): tools/l1/pool57_score.py, the
place38_parse parts, the battery / run_all wiring. Written red first (it failed on `import pool57_score` before the module existed).

Layers:
  1. the plan tables (57a: 63 rows, 57b: 30), rotation and seeds, the smoke rows, and the shell side (arm env, scrub, sizes, budgets, stages, HARNESS_PATHS);
  2. row readouts on synthetic [DIAG] lines with KNOWN answers: realised burst, release batch, ack wakes per loop, a/W, busy / ack-wait, Q and q, l-hat, T-bar and q_l (one leg: the top-level
     0.1 ms rtt; two legs: the rate-weighted per-tick means), the row witnesses (pool AND burst echo on both ends) and the replacement rule;
  3. CALIBRATION on committed rows: section 56's c1s rows (docs/l1-raw/s56: B64-512 against P512 = 82.7 k, burst 47.9, release 102-112, the flagged set, the c1s reference curve 2.1 / 2.7 / 4.7 /
     6.8 / 15.7 -> 0.79 / 0.89 / 0.94 / 0.96 / 1), section 53's c1d rows (the 5-of-30 flag, goodput 729 against 827, the control queue 13-21 ms) and section 54's 24 guard rows (flags 0);
  4. the 57a outcome table (every label, each boundary), the gates (CPU, collapse rate, the 8-row minimum, the re-base, E5);
  5. the 57b rules (flag, occupancy, minimum, no extrapolation, the offset rule, both readings, the band at 2.1 / 2.7 ms) and whole reports on synthetic ledgers, the smoke criteria.
Run: python3 test_pool57.py (the VM runs it under `tst parsers`). The end-to-end path through loopdiag_run_all.sh (plans 57a / 57b) runs in test_stubrun.py."""
import json
import math
import os
import re
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES"):
    os.environ.pop(_k, None)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import pool56_score as ps  # noqa: E402
import pool57_score as p57  # noqa: E402

fails = 0
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")


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


def run_py(*args):
    rc = subprocess.run([sys.executable, *args], capture_output=True, text=True)
    return [x for x in rc.stdout.replace("\r", "").split("\n") if x]


# ═══ 1. the plan tables and the shell side ═══
A = pp.plan57a()
check("57a: 63 slots, all distinct (cell, arm, seed, rep), all c1s-400", len(A) == 63 and len({(c, a, s, r) for c, _, a, s, r in A}) == 63 and {c for c, *_ in A} == {"c1s-400"}, len(A))
check("57a: n = 21 per arm; every replicate holds the three arms once, a cyclic Latin rotation by the replicate, seeds 42 / 7 alternating (rep = k + 1)",
      all(sum(1 for x in A if x[2] == a) == 21 for a in pp.ORDER57A)
      and all([x[2] for x in A if x[4] == k + 1] == list(pp.ORDER57A[k % 3:] + pp.ORDER57A[:k % 3]) and {x[3] for x in A if x[4] == k + 1} == {"42" if k % 2 == 0 else "7"} for k in range(21)), A[:4])
check("57a: each arm leads exactly 7 of the 21 replicates (balanced rotation)", all(sum(1 for k in range(21) if [x[2] for x in A if x[4] == k + 1][0] == a) == 7 for a in pp.ORDER57A))
check("57a: the arms' knobs: B64-512 (pool 512, burst 64, the control), B64-448 (448, 64), B256-512 (512, 256); arm tokens carry no hyphen; display names B64-512 / B64-448 / B256-512",
      pp.ARM57A == {"B64_512": (512, 64), "B64_448": (448, 64), "B256_512": (512, 256)} and all("-" not in a for a in pp.ARM57A) and p57.LABEL57A == {"B64_512": "B64-512", "B64_448": "B64-448", "B256_512": "B256-512"}
      and pp.CONTROL57A == "B64_512" and pp.TEST57A == "B256_512")
check("57a smoke: ONE row, B256-512 (the burst smoke)", pp.plan57a(smoke=True) == [("c1s-400", "def", "B256_512", "42", 1)], pp.plan57a(smoke=True))
B = pp.plan57b()
check("57b: 30 slots, all distinct, all c1d-400, n = 5 per pool, six pools (per-path 2048 control, 768, 512, 384, 256, 192 = W 4096 / 1536 / 1024 / 768 / 512 / 384)",
      len(B) == 30 and len({(c, a, s, r) for c, _, a, s, r in B}) == 30 and {c for c, *_ in B} == {"c1d-400"} and pp.POOLS57B == (2048, 768, 512, 384, 256, 192)
      and all(sum(1 for x in B if x[2] == "P%d" % p) == 5 for p in pp.POOLS57B), B[:3])
arms6 = ["P%d" % p for p in pp.POOLS57B]
check("57b: every replicate holds the six pools once, a cyclic rotation by the replicate, seeds 42 / 7 alternating",
      all([x[2] for x in B if x[4] == k + 1] == arms6[k % 6:] + arms6[:k % 6] and {x[3] for x in B if x[4] == k + 1} == {"42" if k % 2 == 0 else "7"} for k in range(5)), B[:7])
check("57b smoke: pools 192 and 2048 per path (two rows)", [(c, a) for c, _, a, _, _ in pp.plan57b(smoke=True)] == [("c1d-400", "P192"), ("c1d-400", "P2048")], pp.plan57b(smoke=True))
check("arm knobs: 57a (pool, burst) per arm; 57b pools with the shipped burst 64 (the echo must read 64); unknown arms (None, None)",
      pp.arm_knobs57("57a", "B256_512") == (512, 256) and pp.arm_knobs57("57b", "P768") == (768, 64) and pp.arm_knobs57("57b", "P2048") == (2048, 64) and pp.arm_knobs57("57b", "P640") == (None, None)
      and pp.arm_knobs57("57a", "P512") == (None, None) and pp.arm_knobs57("57b", "P768D") == (None, None))
check("sizes 5 GB at both cells; 57 NLIVE c1s 1, c1d 2 (copy of pool56's, registered by pool57_score)", pp.BYTES57 == {"c1s-400": 5000000000, "c1d-400": 5000000000}
      and ps.NLIVE["c1s-400"] == 1 and ps.NLIVE["c1d-400"] == 2 and pp.NLIVE57 == {"c1s-400": 1, "c1d-400": 2} and ps.LATE_T["c1d-400"] == 5.0 and ps.LEGS["c1d-400"] == (0, 1))
for plan, n0, nsm, first in (("57a", 63, 1, "c1s-400 def B64_512 42 1"), ("57b", 30, 2, "c1d-400 def P2048 42 1")):
    ln = run_py(os.path.join(HERE, "place38_parse.py"), "plan" + plan)
    sm = run_py(os.path.join(HERE, "place38_parse.py"), "plan" + plan, "smoke")
    check(f"place38_parse plan{plan} prints the {n0}-row table the battery reads (first row {first}) and the {nsm}-row smoke", len(ln) == n0 and ln[0] == first and len(sm) == nsm, (len(ln), ln[:1], sm))
shell = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
m = re.search(r"p57_bytes\(\) \{ case \"\$1\" in c1s-400\|c1d-400\) echo (\d+) ;;", shell)
check("the battery's p57_bytes is the scorer's BYTES57 (5 GB at both cells)", m is not None and int(m.group(1)) == 5000000000 == pp.BYTES57["c1s-400"] == pp.BYTES57["c1d-400"])
check("the battery: plans 57a / 57b are single-seat (P41 stays 0) like plan 56 (P38, PCAND), instrument env RWM_RTOBS RWM_RDIAG with RWM_PERF_TIMEOUT_S=300",
      'case "$PLAN" in 57a) P57=1; P57A=1; P38=1; PCAND=1 ;; 57b) P57=1; P57B=1; P38=1; PCAND=1 ;; esac' in shell and '[ "$P57" = 1 ] && INSTR_ENV=(RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300)' in shell
      and not re.search(r"P57=1;[^\n]*P41=1", shell))
check("the battery's arm env: 57a sets RWM_STORE_PATH_POOL AND RWM_EMIT_BURST on every arm (the table's knobs); 57b sets RWM_STORE_PATH_POOL at every pool but the shipped 2048 and never RWM_EMIT_BURST",
      all(re.search(a_ + r"\)[^\n]*\n\s*arm_env=\(RWM_STORE_PATH_POOL=%d RWM_EMIT_BURST=%d\)" % (pl, bu), shell) for a_, pl, bu in (("B64_512", 512, 64), ("B64_448", 448, 64), ("B256_512", 512, 256)))
      and '[ "$p57pool" = "2048" ] || arm_env=(RWM_STORE_PATH_POOL="$p57pool")' in shell and "EMIT_BURST" not in shell.split('p57pool="${arm#P}"')[1].split("esac")[0])
env_m = re.search(r"^    env -u RWM_EST_CADENCE[^\n]*(?:\n[^\n]*?\\)*\n[^\n]*\"\$\{arm_env\[@\]\}\"", shell, re.M)
check("the scrub withholds RWM_STORE_PATH_POOL and RWM_EMIT_BURST in the same env statement as, and BEFORE, the arm_env assignments (env applies -u, then the assignments)",
      env_m is not None and "-u RWM_STORE_PATH_POOL" in env_m.group(0) and "-u RWM_EMIT_BURST" in env_m.group(0), env_m and env_m.group(0)[:200])
check("the battery: 1 smoke row + 63 slots (57a), 2 smoke rows + 30 slots (57b), a VOID pool / burst echo is replaced once (replace57), only the full battery uses the replacement",
      '-eq 63 ] || { echo "ABORT-PLAN57A table' in shell and '-eq 30 ] || { echo "ABORT-PLAN57B table' in shell and '-eq 1 ] || { echo "ABORT-PLAN57A smoke' in shell and '-eq 2 ] || { echo "ABORT-PLAN57B smoke' in shell
      and 'replace57 "$OUT"' in shell and "run_slot57()" in shell)
check("run_all: plans 57a / 57b are candidate plans without a BASE tree (P46 yes, P41 no), pre-flight /dev/shm like plan 56, stages rep57a / rep57b",
      'P57A=0; [ "$PLAN" = "57a" ]' in runall and 'P57B=0; [ "$PLAN" = "57b" ]' in runall and "LD_SHM_MIN_KB:-1048576" in runall and "stage rep57a 57a" in runall and "stage rep57b 57b" in runall)
check("run_all: the budget guard refuses with less than the worst case left: 57a 13 200 s (3 h 40 of the status, LD_HARD = launch + 4 h), 57b 5 400 s (1 h 30, LD_HARD = launch + 2.5 h)",
      "13200" in runall and "5400" in runall and "ABORT-BUDGET plan 57" in runall and "LD_HARD_MIN_S" in runall)
check("run_all: the engine suites are reused only on the reference-manifest hit (the SAME ref56-manifest.sha256: main's engine still equals it), the tools tests always run",
      'if [ "$PLAN" = "56" ] || [ "$P57" = 1 ]; then suites_stage56; return; fi' in runall and "ref56-manifest.sha256" in runall)
refm = subprocess.run(["bash", os.path.join(HERE, "ref_manifest.sh"), "HEAD"], capture_output=True, text=True, cwd=HERE)
if refm.returncode == 0:
    same = refm.stdout.replace("\r", "") == open(os.path.join(HERE, "ref56-manifest.sha256"), newline="").read().replace("\r", "")
    check("the committed reference manifest (ref56) still equals this tree's raptorpath/src + raptorpath/tests (no ref57 needed)", same)
else:
    print("skip the live manifest comparison (no git/bash here):", refm.stderr[:100])
import harness_paths as hp  # noqa: E402
check("HARNESS_PATHS lists 57a and 57b: rehearse, full, reuse-hit, reuse-miss (plan 56's shape: no prior-tests, no historical-base)",
      hp.HARNESS_PATHS.get("57a") == ("rehearse", "full", "reuse-hit", "reuse-miss") and hp.HARNESS_PATHS.get("57b") == ("rehearse", "full", "reuse-hit", "reuse-miss"))


# ═══ 2. row readouts on synthetic [DIAG] lines with known answers ═══
def dl(t, legs, cap, a, top, tx, rtt, rtp, qc, src, cum, wack, eb, busy=45, ackw=82, np_=None):
    """One client [DIAG] line: per leg i dgq tx = tx[i], p-block rtt / rtp_us / qcwnd; top = the top-level 0.1 ms rtt."""
    n = np_ or len(legs)
    s = (f"[DIAG] t={t:.2f}s win={a}/{cap} paused=0% good=1Mbit ackrate_ewma=0sym/s eff_pace=0sym/s src={src}sym/s cod=0sym/s cum={cum}/0/{cum} sidle=0ms/0/mx0ms cwnd=1 infl=1 np={n} np_act={n} rtt={top:.1f}ms bdp100=1sym "
         f"sweeps=0 retx=0 gapdrop=0 nbud=0  mpr[rep=1 seqs=1 fired=1 supp=0/0/0] wnd2=0/0 gate=0 wait[tun=1% paused=17% pace=0% gen=0% nack=0% defc=0% tail=0% flush=0% ack={ackw}% cmd=0% n=670 us=251088 busy={busy}% busy_us=1] "
         f"wake[tun=17954 paused=509 pace=0 gen=0 nack=2 defc=0 tail=2 flush=0 ack={wack} cmd=4 timer_acked=176] ")
    for i in legs:
        s += f"dgq{i}[hand={tx[i] + 5} tx={tx[i]} full=0 err=0 sp=1] "
    for i in legs:
        s += (f"p{i}:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00 btlbw=1 est=Y pl=0.0000 cmp=- rtt={rtt[i]}/wrtt=2/rtp3ms rtp_us={rtp[i]} sig_us=1/n1 qsp_us=1/n1 msd_us=1/n1 tlag_us=1/n1 gapd=0/0 "
              f"qcwnd={qc[i]} qce=0 qlp=1/1 | ANCHOR sent=0 plu=0.0010 wpl_us=1 ")
    return (s + f"rce=1/c1 eb_bursts={eb[0]} eb_syms={eb[1]} eb_depth=1.00 eb_end=cap:1/store:1/tokens:0/drained:0 eb_maxrun=0:64/1.00").rstrip()


def c1s_lines(r=80000, cap=512, a=435, top=5.2, burst=50.0, wake_s=800, batch_src=None, qc=300000, t_end=30.0, collapse_first=0):
    """c1s (np = 1): tx = r t, cum = r t (source symbols), ack wakes wake_s per s, eb_syms = r t, eb_bursts = r t / burst. rtp_us 3000 before 1 s, 2600 in the first 2 s, 2700 later."""
    out = ['{"seconds":0.01,"warmup":true}']
    for k in range(1, int(t_end / 0.25) + 1):
        t = 0.25 * k
        rtp = 3000 if t < 1 else (2600 if t <= 2.0 else 2700)
        q = 1000 if k <= collapse_first else qc
        cum = int(r * t)
        out.append(dl(t, (0,), cap, a, top, [int(r * t)], [5], [rtp], [q], int(r) if t <= t_end - 1 else 0, cum, int(wake_s * t), (int(r * t / burst), cum)))
    return out


def c1d_lines(r=(45000, 44000), cap=768, a=700, top=22.0, qc=(300000, 300000), t_end=30.0, rtt_pat=(20, 22), rtp_boot=(2400, 2600), collapse=(0, 0)):
    out = ['{"seconds":0.01,"warmup":true}']
    nt = int(t_end / 0.25)
    for k in range(1, nt + 1):
        t = 0.25 * k
        rt = rtt_pat[k % 2]
        rtp = [rtp_boot[i] if t <= 2.0 else rtp_boot[i] + 300 for i in (0, 1)]
        q = [1000 if k <= collapse[i] * nt else qc[i] for i in (0, 1)]
        cum = int((r[0] + r[1]) * t)
        out.append(dl(t, (0, 1), cap, a, top, [int(r[0] * t), int(r[1] * t)], [rt, rt], rtp, q, 1000 if t <= t_end - 1 else 0, cum, int(400 * t), (int(cum / 60), cum)))
    return out


t56, t57 = p57.row_metrics57(c1s_lines(), [], [], 5000000000, "c1s-400", 512)
check("c1s readouts: the lap rate r0 = d tx / dt = 80000 /s, the bind fraction 1.0 at pool 512 (cap 512 >= 1 x 512)", near(t56["r0"], 80000, 1) and t56["bind_frac"] == 1.0, (t56["r0"], t56["bind_frac"]))
check("c1s realised mean burst = d eb_syms / d eb_bursts over LATE = 50.0; release batch = d cum / d ack wakes = 80000 / 800 = 100.0",
      near(t57["burst"], 50.0, 0.1) and near(t57["release"], 100.0, 0.1), (t57["burst"], t57["release"]))
check("c1s ack wakes per loop = 800 /s x 5.2 ms = 4.16; a/W = 435 / 512; busy 45 %, ack-wait 82 %", near(t57["wakes_loop"], 4.16, 0.02) and near(t57["aW"], 435 / 512, 1e-6) and t57["busy"] == 45 and t57["ackw"] == 82,
      {k: t57[k] for k in ("wakes_loop", "aW", "busy", "ackw")})
check("c1s Q = a - R x T0 = 435 - 80000 x 0.004 = 115 symbols (R = the per-tick-interval mean); q = 5.2 - 4.0 = 1.2 ms (the 0.1 ms per-tick mean, not an integer median)",
      near(t57["Q"], 115.0, 1.0) and near(t57["q_top"], 1.2, 1e-6), (t57["Q"], t57["q_top"]))
check("c1s l-hat = the lowest rtp_us of the first 2 s (2.6 ms, not the first tick's 3.0 and not a later value); T-bar = the top-level mean 5.2; q_l = T-bar - l-hat = 2.6 ms",
      near(t57["ell_hat_ms"], 2.6, 1e-6) and near(t57["Tbar57_ms"], 5.2, 1e-6) and near(t57["q_ell"], 2.6, 1e-6), (t57["ell_hat_ms"], t57["Tbar57_ms"], t57["q_ell"]))
t56d, t57d = p57.row_metrics57(c1d_lines(), [], [], 5000000000, "c1d-400", 384)
check("c1d readouts: two legs, bind against np x pool (cap 768 >= 2 x 384), R = 89000 /s, a/W = 700 / 768", near(t56d["tx_sum_late"], 89000, 2) and t56d["bind_frac"] == 1.0 and near(t57d["aW"], 700 / 768, 1e-6), (t56d["tx_sum_late"], t56d["bind_frac"], t57d["aW"]))
check("c1d l-hat = the rate-weighted lifetime minima (2.4 ms, 2.6 ms) = 2.499; T-bar = the rate-weighted per-tick MEAN of the legs' echo rtt (20 / 22 alternating -> 21.0), NOT the top-level max form; q_l = 18.50; the top-level form (22.0) is reported as q_l(top) = 19.50",
      near(t57d["ell_hat_ms"], (45000 * 2.4 + 44000 * 2.6) / 89000, 0.005) and near(t57d["Tbar57_ms"], 21.0, 0.05) and near(t57d["q_ell"], 21.0 - t57d["ell_hat_ms"], 0.06) and near(t57d["q_ell_top"], 22.0 - t57d["ell_hat_ms"], 1e-6),
      {k: t57d[k] for k in ("ell_hat_ms", "Tbar57_ms", "q_ell", "q_ell_top")})
check("c1d: no Q / q_top (c1s-only readouts); wakes per loop = 400 /s x T-bar (21 ms) = 8.4", t57d["Q"] is None and t57d["q_top"] is None and near(t57d["wakes_loop"], 8.4, 0.05), (t57d["Q"], t57d["wakes_loop"]))
e56, e57 = p57.row_metrics57(c1s_lines()[:3], [], [], 5000000000, "c1s-400", 512)
check("a row without a LATE window reads None everywhere (no exception)", e57["burst"] is None and e57["q_ell"] is None and e57["release"] is None)
# the flags on the rows' own qcwnd shares
rows_flag = {}
for name, kw in (("clean", {}), ("leg0-only", {"collapse": (1.0, 0.0)}), ("both", {"collapse": (1.0, 1.0)}), ("both-low", {"collapse": (0.05, 0.05)}), ("both-edge", {"collapse": (0.15, 0.15)})):
    lines = c1d_lines(**kw)
    rows_flag[name] = {"cell": "c1d-400", "arm": "P384", "qcwnd_below": pp.qcwnd_below_bdp(lines, "c1d-400")}
fb = {k: p57.flag_shares_b(v)[:2] for k, v in rows_flag.items()}
check("c1d flag = BOTH legs' below-BDP window shares > 0.10: clean (False, False), one leg only (False, True), both legs (True, False), both at ~0.03 (False, False), both at ~0.13 (True, False)",
      fb == {"clean": (False, False), "leg0-only": (False, True), "both": (True, False), "both-low": (False, False), "both-edge": (True, False)}, fb)
check("the c1d flag is_flagged follows the both-leg rule, the c1s flag is pool56's (leg 0 > 0.10)", p57.is_flagged(rows_flag["both"]) and not p57.is_flagged(rows_flag["leg0-only"])
      and p57.is_flagged({"cell": "c1s-400", "arm": "B64_512", "qcwnd_below": {"per_leg": [0.11]}}) and not p57.is_flagged({"cell": "c1s-400", "arm": "B64_512", "qcwnd_below": {"per_leg": [0.10]}}))
# witnesses
GATE = ("[GATES] RWM_UNIFIED=1 RWM_STORE_PATH_POOL={pool} RWM_STORE=unset RWM_EMIT_BATCH=1 RWM_EMIT_BURST={burst} RWM_RECOV_MP=1 RWM_SUCC_DUMP=0 RWM_SUCC_DUMP_MAX=200000 RWM_DIAG=1")
INFO = "INFO raptorpath::net: path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS: cap = clamp(gain*bdp, floor, n_live*pool)) pool_per_path={n}"


def ends(pool, burst, info=None):
    return [GATE.format(pool=pool, burst=burst), INFO.format(n=info if info is not None else pool)]


ok_c, ok_s = ends(512, 256), ends(512, 256)
w0 = pp.echo57_problems("57a", "B256_512", "c1s-400", ok_c, ok_s, "s" * 64, "s" * 64)
check("witnesses: pool 512 and burst 256 echoed on both ends, pool_per_path 512, the seat's sha: no problem", w0 == [], w0)
w1 = pp.echo57_problems("57a", "B256_512", "c1s-400", ends(512, 64), ends(512, 256), "s" * 64, "s" * 64)
check("witnesses: the test arm echoing the shipped burst 64 on the client is a failure gate-burst-cli (and VOID by the echo, replaced once)", any(x.startswith("gate-burst-cli=64") for x in w1), w1)
w2 = pp.echo57_problems("57a", "B64_448", "c1s-400", ends(448, 64), ends(2048, 64, info=2048), "s" * 64, "s" * 64)
check("witnesses: a server echoing pool 2048 for a 448 request fails gate-pool-srv (the pool is checked on both ends)", any(x.startswith("gate-pool-srv=2048") for x in w2), w2)
w3 = pp.echo57_problems("57b", "P768", "c1d-400", ends(768, 64), ends(768, 64), "s" * 64, "s" * 64)
check("57b witnesses: pool 768 and the shipped burst 64 on both ends: no problem; a leaked RWM_EMIT_BURST=256 fails gate-burst",
      w3 == [] and any(x.startswith("gate-burst-srv=256") for x in pp.echo57_problems("57b", "P768", "c1d-400", ends(768, 64), ends(768, 256), "s" * 64, "s" * 64)))
check("witnesses: pool_per_path 2048 on the sender for a 512 request fails; a missing sha comparison is a problem (never a silent pass)",
      any(x.startswith("pool_per_path-cli=2048") for x in pp.echo57_problems("57a", "B64_512", "c1s-400", ends(512, 64, info=2048), ends(512, 64), "s" * 64, "s" * 64))
      and any(x.startswith("bin-sha-uncheckable") for x in pp.echo57_problems("57a", "B64_512", "c1s-400", ends(512, 64), ends(512, 64), None, None)))
check("an arm outside the plan is unknown (57a with a 57b arm, 57b with a dump arm)", pp.echo57_problems("57a", "P512", "c1s-400", [], []) == ["unknown-arm-P512-at-c1s-400"] and pp.echo57_problems("57b", "P640D", "c1d-400", [], [])[0].startswith("unknown-arm"))
with tempfile.TemporaryDirectory() as td:
    led = os.path.join(td, "ld.log")
    bad = {"cell": "c1s-400", "arm": "B256_512", "seed": "42", "rep": 1, "problems": ["gate-burst-cli=64 (want 256)"], "status": "WITNESS-FAIL"}
    good = {"cell": "c1s-400", "arm": "B256_512", "seed": "42", "rep": 1, "problems": [], "status": "LIVE"}
    open(led, "w").write("P38ROW " + json.dumps(bad) + "\n")
    r1 = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "replace57", led, "c1s-400", "B256_512", "42", "1"], capture_output=True, text=True)
    open(led, "a").write("P38ROW " + json.dumps(good) + "\n")
    r2 = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "replace57", led, "c1s-400", "B256_512", "42", "1"], capture_output=True, text=True)
    open(led, "w").write("P38ROW " + json.dumps({**bad, "problems": ["bin-sha=x (want y for CAND)"]}) + "\n")
    r3 = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "replace57", led, "c1s-400", "B256_512", "42", "1"], capture_output=True, text=True)
check("replace57: a VOID burst echo is replaced once (rc 0, 'echo'); a slot that already has its replacement, or a VOID of another kind (a foreign binary), is not (rc 1)",
      r1.returncode == 0 and r1.stdout.strip() == "echo" and r2.returncode == 1 and r3.returncode == 1, (r1.returncode, r1.stdout, r2.returncode, r3.returncode))

# ═══ 3. CALIBRATION on committed rows ═══
def tgz_rows(tgz, cell, pool_of, keep=lambda name: True):
    """Rows (dicts with t56 / t57 / qcwnd_below, as the ledger holds them) of the committed diag-ld archive `tgz` for `cell`, computed with THIS module's definitions."""
    out = []
    with tempfile.TemporaryDirectory() as td, tarfile.open(tgz) as tf:
        for mem in tf.getmembers():
            b = os.path.basename(mem.name)
            if mem.isfile() and b.startswith(cell + "@def-") and "smoke" not in os.path.dirname(mem.name) and keep(b):
                with open(os.path.join(td, b), "wb") as f:
                    f.write(tf.extractfile(mem).read())
        for n in sorted(os.listdir(td)):
            m = re.match(re.escape(cell) + r"@def-(.+?)-(s\d+)-(r\d+)-a(\d+)-c\.log$", n)
            if not m:
                continue
            base = os.path.join(td, n[:-len("-c.log")])
            cli, srv, drv = lc.read(os.path.join(td, n)), lc.read(base + "-s.log"), lc.read(base + "-drv.out")
            t56, t57 = p57.row_metrics57(cli, srv, drv, 5000000000, cell, pool_of(m.group(1)))
            out.append({"cell": cell, "arm": m.group(1), "seed": m.group(2)[1:], "rep": m.group(3)[1:], "status": "LIVE", "t56": t56, "t57": t57, "qcwnd_below": pp.qcwnd_below_bdp(cli, cell),
                        "dnf": not any('"summary"' in ln for ln in drv), "problems": []})
    return out


S56 = os.path.join(RAW, "s56", "rep56-diag-ld.tgz")
S53 = os.path.join(RAW, "s53", "rep53-diag-ld.tgz")
S54 = os.path.join(RAW, "s54", "guard-diag-ld.tgz")
if os.path.exists(S56):
    rows56 = tgz_rows(S56, "c1s-400", lambda a: int(a[1:]))
    flagged56 = sorted((r["arm"], "s" + r["seed"], "r" + r["rep"]) for r in rows56 if ps.qc_flagged(r))
    check("section 56 (27 c1s rows): pool56's collapse flag recomputed here flags exactly the nine rows of c1srows.py's list (four of them at P512)",
          len(rows56) == 27 and flagged56 == sorted([("P1024", "s7", "r2"), ("P2048", "s42", "r3"), ("P2048", "s42", "r9"), ("P2048", "s7", "r8"), ("P384", "s42", "r1"), ("P512", "s42", "r1"),
                                                      ("P512", "s42", "r5"), ("P512", "s42", "r9"), ("P512", "s7", "r2")]), (len(rows56), flagged56))
    p512 = [r for r in rows56 if r["arm"] == "P512" and not ps.qc_flagged(r)]
    R512 = sorted(r["t56"]["r0"] for r in p512)
    check("CALIBRATION B64-512 against section 56's P512 (same lap-rate definition d(dgq0 tx)/dt over LATE): five unflagged rows, median 82 708 /s, range 80 262 - 83 257 (82.7 k, 80.3 - 83.3 k)",
          len(p512) == 5 and near(lc.med(R512), 82708, 1.0) and near(R512[0], 80262, 1.0) and near(R512[-1], 83257, 1.0), R512)
    check("re-base rule on that median: 82 708 is within 82.7 k +- 3.0 k (not re-based); the edges: 85 700 within, 85 701 re-based, 79 700 within, 79 699 re-based",
          p57.rebase57a(82708.0)[0] is False and p57.rebase57a(85700.0)[0] is False and p57.rebase57a(85701.0)[0] is True and p57.rebase57a(79700.0)[0] is False and p57.rebase57a(79699.0)[0] is True
          and near(p57.rebase57a(82708.0)[1], 3000.0), p57.rebase57a(82708.0))
    bs = [r["t57"]["burst"] for r in p512]
    check("section 56's P512 realised mean burst (d eb_syms / d eb_bursts over LATE) reads 47.9 (median; every row 47.5 - 49.5): the b-bar64 ~ 48 of the pre-registered expectation",
          near(lc.med(bs), 47.9, 0.3) and min(bs) > 47.0 and max(bs) < 49.5, bs)
    rel = [r["t57"]["release"] for r in p512]
    check("section 56's P512 release batch 102 - 112 (c1srows.out), ack wakes per loop 3.7 - 4.3, mean a/W 0.847 - 0.864, busy 45 %, ack-wait 80 - 84 %",
          min(rel) > 101 and max(rel) < 113 and all(3.6 < r["t57"]["wakes_loop"] < 4.4 and 0.845 < r["t57"]["aW"] < 0.865 and abs(r["t57"]["busy"] - 45) < 1 and 80 <= r["t57"]["ackw"] <= 84 for r in p512), rel)
    check("section 56's P512 q = mean(0.1 ms rtt) - 4.0 ms reads 1.1 - 1.3 ms and Q = a - R x T0 reads 85 - 110 symbols (c1srows.out: 92 - 106 from q x R)",
          all(1.05 < r["t57"]["q_top"] < 1.35 and 85 < r["t57"]["Q"] < 110 for r in p512), [(r["t57"]["q_top"], r["t57"]["Q"]) for r in p512])
    e5_rows = {a: lc.med([r["t57"]["release"] for r in rows56 if r["arm"] == a and not ps.qc_flagged(r)]) for a in ("P384", "P512")}
    check("E5 on section 56's data: release batch / W at W 512 is 0.19 - 0.22 (inside 0.17 - 0.24): E5 does not fire on the reference rows", all(0.17 <= r / 512 <= 0.24 for r in rel) and 0.17 <= e5_rows["P384"] / 384 <= 0.24, e5_rows)
    as57 = p57.arm_stats_a([dict(r, arm="B64_512") for r in rows56 if r["arm"] == "P512"], "B64_512")
    check("arm_stats_a on section 56's nine P512 rows: n_u = 5 unflagged bound rows, 4 collapse-flagged, collapse share 4/9, R median 82 708, SE = 1.253 sd / sqrt(5)",
          as57["n"] == 5 and as57["flagged"] == 4 and near(as57["rate"], 4 / 9, 1e-9) and near(as57["R"], 82708, 1.0) and near(as57["se"], 1.253 * as57["sd"] / math.sqrt(5), 1e-6) and as57["unbound"] == 0, as57)
    ref = {a: (n, q, r) for a, n, q, r in p57.c1s_reference(rows56)}
    want_ref = {"P384": (2, 2.1, 0.79), "P512": (5, 2.7, 0.89), "P768": (3, 4.7, 0.94), "P1024": (2, 6.8, 0.96), "P2048": (6, 15.7, 1.0)}
    check("CALIBRATION of q_l and the c1s reference curve: one function reproduces section 56's q_l = 2.1 / 2.7 / 4.7 / 6.8 / 15.7 ms (+- 0.15) and goodput/control 0.79 / 0.89 / 0.94 / 0.96 / 1 (+- 0.02) "
          "on 2 / 5 / 3 / 2 / 6 unflagged rows", all(a in ref and ref[a][0] == n and near(ref[a][1], q, 0.15) and near(ref[a][2], g, 0.02) for a, (n, q, g) in want_ref.items()), ref)
    check("the registered constants C1S_REF are those reference points (the scorer scores against the constants, the test recomputes them)",
          [(round(q, 1), g) for q, g in p57.C1S_REF] == [(2.1, 0.79), (2.7, 0.89), (4.7, 0.94), (6.8, 0.96), (15.7, 1.0)] and p57.A_POINTS == p57.C1S_REF[:2] and p57.B_POINTS == p57.C1S_REF[2:4])
    ells = [r["t57"]["ell_hat_ms"] for r in rows56 if r["t57"]["ell_hat_ms"] and not ps.qc_flagged(r)]
    check("c1s l-hat (the lowest rtp_us of the first 2 s) is 2.3 - 2.9 ms on every unflagged section-56 row (registered 2.4 - 2.6) and the floor offset T0 - l-hat is ~1.5 ms (registered DELTA0_C1S)",
          len(ells) == 18 and 2.3 < min(ells) and max(ells) < 2.9 and near(p57.T0_MS - lc.med(ells), p57.DELTA0_C1S, 0.15), (min(ells), max(ells), lc.med(ells)))
else:
    print("skip the section-56 calibration (docs/l1-raw/s56 not present)")
if os.path.exists(S53):
    rows53 = tgz_rows(S53, "c1d-400", lambda a: 2048)
    fl53 = [r for r in rows53 if p57.is_flagged(r)]
    un53 = [r for r in rows53 if not p57.is_flagged(r)]
    check("CALIBRATION of the c1d flag on section 53's 30 c1d 5 GB rows: BOTH legs below the BDP in > 10 % of the windows flags 5 of 30, goodput median 729 against unflagged 827 (-12 %)",
          len(rows53) == 30 and len(fl53) == 5 and near(lc.med([ps.gp(r) for r in fl53]), 729, 1.0) and near(lc.med([ps.gp(r) for r in un53]), 827, 1.0), (len(rows53), len(fl53)))
    check("the one-leg-only share is reported beside it (11 rows with one leg above 0.10 and the other not; c1dflag.py prints FLAG on those too: 16 of 30)",
          sum(1 for r in rows53 if p57.flag_shares_b(r)[1]) == 11 and sum(1 for r in rows53 if max(p57.flag_shares_b(r)[2]) > 0.10) == 16, None)
    qs = [r["t57"]["q_ell"] for r in un53 if r["t57"]["q_ell"] is not None]
    qt = [r["t57"]["q_ell_top"] for r in un53 if r["t57"]["q_ell_top"] is not None]
    check("CALIBRATION of the c1d control queue: q_l = T-bar - l-hat (legs' echo rtt, per-tick means, rate-weighted) puts section 53's unflagged shipped-pool rows at 13 - 21 ms (median in the registered band); "
          "the top-level (max-of-paths) form would read higher (interpretation 6)", len(qs) == 25 and 13.0 <= lc.med(qs) <= 21.0 and lc.med(qt) > lc.med(qs), (lc.med(qs), lc.med(qt)))
    aw = [r["t57"]["aW"] for r in rows53]
    check("the shipped-pool c1d control is interior: mean a/W 0.42 - 0.70 on 27 or more of the 30 rows (c1dcap.out: 0.42 - 0.69 plus the collapse rows), so the control is not a T point",
          sum(1 for x in aw if x is not None and 0.40 <= x <= 0.70) >= 27, sorted(round(x, 2) for x in aw if x is not None))
    ells = [r["t57"]["ell_hat_ms"] for r in rows53]
    check("c1d l-hat is 2.3 - 3.0 ms on every row (registered ~2.5 ms on both legs)", all(x is not None and 2.3 < x < 3.0 for x in ells), sorted(round(x, 2) for x in ells if x))
else:
    print("skip the section-53 calibration (docs/l1-raw/s53 not present)")
if os.path.exists(S54):
    rows54 = tgz_rows(S54, "c1d-400", lambda a: 2048)
    check("CALIBRATION of the c1d flag on section 54's 24 guard rows (today's engine, 400 MB): the both-leg flag fires on none (c1dflag54.out); 3 rows carry one leg above 0.10 (reported)",
          len(rows54) == 24 and sum(1 for r in rows54 if p57.is_flagged(r)) == 0 and sum(1 for r in rows54 if p57.flag_shares_b(r)[1]) == 3, (len(rows54), sum(1 for r in rows54 if p57.is_flagged(r))))
else:
    print("skip the section-54 calibration (docs/l1-raw/s54 not present)")


# ═══ 4. the 57a outcome table ═══
def vals21(mean, sd):
    """21 values with sample mean = median = `mean` and sample sd = `sd` exactly (10 x +sd, 10 x -sd, 1 x the mean)."""
    return [mean + sd] * 10 + [mean - sd] * 10 + [mean]


def stat(R, sd=1100.0, n=21, b=48.0, cpu=10.0, rate=0.3, release=100.0):
    return {"n": n, "R": R, "sd": sd, "se": p57.se_of(sd, n), "b": b, "cpu": cpu, "rate": rate, "release": release}


def trio(R512=82500.0, R448=78500.0, R256=81000.0, b64=48.0, b256=96.0, sd=1100.0, n=(21, 21, 21), cpu=(10.0, 10.0, 10.0), rate=(0.3, 0.3, 0.3)):
    return {"B64_512": stat(R512, sd, n[0], b64, cpu[0], rate[0]), "B64_448": stat(R448, sd, n[1], 48.0, cpu[1], rate[1]), "B256_512": stat(R256, sd, n[2], b256, cpu[2], rate[2])}


check("vals21: 21 values, median = mean, sample sd exact", near(lc.med(vals21(5000.0, 300.0)), 5000.0) and near(p57.sdev(vals21(5000.0, 300.0)), 300.0, 1e-9) and len(vals21(1, 1)) == 21)
check("SE = 1.253 sd / sqrt(n_u) from each arm's OWN realised n and sd (sample sd, n - 1)", near(p57.se_of(1100.0, 21), 1.253 * 1100.0 / math.sqrt(21)) and near(p57.se_of(1100.0, 12), 1.253 * 1100.0 / math.sqrt(12))
      and p57.se_of(None, 5) is None and p57.sdev([1.0]) is None)
o = p57.outcome57a(trio())
se1 = p57.se_of(1100.0, 21)
check("the hypothesis: dQ_b = (96 - 48)/2 = 24 slots, k = 24/64 = 0.375, chord 4000 -> R_pred = 82 500 - 0.375 x 4000 = 81 000; SE_null = sqrt(2) SE, SE_pred^2 = SE^2 (1 + (1-k)^2 + k^2), S = 1500 / SE_null",
      near(o["dQ"], 24.0) and near(o["k"], 0.375) and near(o["chord"], 4000.0) and near(o["R_pred"], 81000.0) and near(o["se_null"], math.sqrt(2) * se1, 1e-6)
      and near(o["se_pred"], se1 * math.sqrt(1 + 0.625 ** 2 + 0.375 ** 2), 1e-6) and near(o["S"], 1500.0 / (math.sqrt(2) * se1), 1e-6), o)
check("E1: BURST HOLDS STORE (S >= 2, z_null <= -2, |z_hyp| < 2): R256 = R_pred = 81 000 -> z_null = %.2f, z_hyp 0" % o["z_null"], o["label"] == "E1" and o["z_null"] < -2 and abs(o["z_hyp"]) < 1e-9, o)
o = p57.outcome57a(trio(R256=82500.0))
check("E2: NOT THE BURST (S >= 2, |z_null| < 1, z_hyp >= +2): R256 = R512 -> z_null 0, z_hyp %.2f" % o["z_hyp"], o["label"] == "E2" and o["z_null"] == 0 and o["z_hyp"] >= 2 and o["S"] >= 2, o)
o = p57.outcome57a(trio(R256=84000.0))
check("RISING (z_null >= +2): R256 = 84 000 -> z_null %.2f; with S >= 2 no UNDERPOWERED note" % o["z_null"], o["label"] == "RISING" and o["z_null"] >= 2 and o["notes"] == [], o)
o = p57.outcome57a(trio(R448=82000.0, R256=82500.0))
check("UNDERPOWERED (S < 2): chord 500 -> S %.2f; E1 / E2 are not read" % o["S"], o["label"] == "UNDERPOWERED" and o["S"] < 2, o)
o = p57.outcome57a(trio(R448=82000.0, R256=84000.0))
check("RISING wins over UNDERPOWERED (RISING has no S condition) and keeps the note 'S < 2: E1/E2 not read'", o["label"] == "RISING" and o["S"] < 2 and any("S < 2" in n for n in o["notes"]), o)
o = p57.outcome57a(trio(R256=79500.0))
check("PARTIAL (S >= 2, z_null <= -2, |z_hyp| >= 2): R256 = 79 500 -> z_hyp %.2f; the ratio of the observed to the predicted shift is (79500 - 82500)/(81000 - 82500) = 2.0" % o["z_hyp"],
      o["label"] == "PARTIAL" and o["z_null"] <= -2 and abs(o["z_hyp"]) >= 2 and near(o["ratio"], 2.0, 1e-9), o)
o = p57.outcome57a(trio(R256=81860.0))
check("INCONCLUSIVE (S >= 2 but z_null %.2f is neither <= -2 nor |z| < 1)" % o["z_null"], o["label"] == "INCONCLUSIVE", o)
check("INERT: a realised b-bar256 <= 64 (the knob does not act) -> INERT, whatever the rates; 64.5 is not inert", p57.outcome57a(trio(b256=64.0))["label"] == "INERT" and p57.outcome57a(trio(b256=64.0, R256=84000.0))["label"] == "INERT"
      and p57.outcome57a(trio(b256=64.5))["label"] != "INERT")
check("UNREAD: an arm with fewer than 8 unflagged rows (n = 7) -> UNREAD, 8 -> read; zero spread (sd 0) -> UNREAD, never a division by zero",
      p57.outcome57a(trio(n=(21, 7, 21)))["label"] == "UNREAD" and p57.outcome57a(trio(n=(21, 21, 7)))["label"] == "UNREAD" and p57.outcome57a(trio(n=(8, 8, 8)))["label"] == "E1"
      and p57.outcome57a(trio(sd=0.0))["label"] == "UNREAD")
# boundaries z_null = -+2 / +-1, S = 2, z_hyp = +-2 (the function itself is the oracle for R_pred / SE)
base = p57.outcome57a(trio())


def at(zn=None, zh=None, chord=None):
    t = trio()
    if chord is not None:
        t["B64_448"]["R"] = 82500.0 - chord
    ref = p57.outcome57a(t)
    if zn is not None:
        t["B256_512"]["R"] = 82500.0 + zn * ref["se_null"]
    if zh is not None:
        t["B256_512"]["R"] = ref["R_pred"] + zh * ref["se_pred"]
    return p57.outcome57a(t)


check("boundary z_null: -2.01 -> E1 (z_hyp inside), -1.99 -> INCONCLUSIVE; +1.99 -> INCONCLUSIVE, +2.01 -> RISING", at(zn=-2.01)["label"] == "E1" and at(zn=-1.99)["label"] == "INCONCLUSIVE"
      and at(zn=1.99)["label"] == "INCONCLUSIVE" and at(zn=2.01)["label"] == "RISING", [at(zn=z)["label"] for z in (-2.01, -1.99, 1.99, 2.01)])
check("boundary |z_null| < 1 for E2: z_null +0.99 -> E2, +1.01 -> INCONCLUSIVE, -0.99 -> E2 (z_hyp stays >= 2)", at(zn=0.99)["label"] == "E2" and at(zn=1.01)["label"] == "INCONCLUSIVE" and at(zn=-0.99)["label"] == "E2",
      [at(zn=z)["label"] for z in (0.99, 1.01, -0.99)])
check("boundary z_hyp for E1 / PARTIAL (z_null <= -2): z_hyp -1.99 -> E1, -2.01 -> PARTIAL; for E2: z_hyp +2.01 -> E2 with z_null 0 only",
      at(zh=-1.99)["label"] == "E1" and at(zh=-2.01)["label"] == "PARTIAL" and at(zh=1.99)["z_hyp"] < 2, [at(zh=z)["label"] for z in (-1.99, -2.01)])
nse = base["se_null"]
k_ = base["k"]
check("boundary S = 2: chord with S 2.01 -> not UNDERPOWERED (E1 at z_null -3 with R256 = R512 - 3 SE_null), S 1.99 -> UNDERPOWERED",
      p57.outcome57a(dict(trio(), B64_448=stat(82500.0 - 2.01 * nse / k_, b=48.0), B256_512=stat(82500.0 - 3 * nse, b=96.0)))["label"] == "E1"
      and p57.outcome57a(dict(trio(), B64_448=stat(82500.0 - 1.99 * nse / k_, b=48.0), B256_512=stat(82500.0 - 3 * nse, b=96.0)))["label"] == "UNDERPOWERED")
check("a realised b between 64 and 90 is scored with that b-bar (smaller k, smaller S); above 105 is scored with it too (k grows)",
      near(p57.outcome57a(trio(b256=70.0))["k"], (70.0 - 48.0) / 2 / 64) and near(p57.outcome57a(trio(b256=130.0))["k"], (130.0 - 48.0) / 2 / 64) and p57.outcome57a(trio(b256=70.0))["S"] < p57.outcome57a(trio(b256=130.0))["S"])
# gates
c = lambda a, b_: p57.outcome57a(trio(cpu=(10.0, a, b_)))
check("CPU gate (each test arm within 5 % of B64-512's client CPU/GB): 10.49 and 10.51 on B256-512 -> confounded False / True; B64-448 judged separately; the label names the arm",
      c(10.0, 10.49)["cpu"] == {"B64_448": False, "B256_512": False} and c(10.0, 10.51)["cpu"]["B256_512"] is True and c(9.51, 10.0)["cpu"]["B64_448"] is False and c(9.49, 10.0)["cpu"]["B64_448"] is True
      and "CPU-CONFOUNDED(B256-512)" in p57.full_label57a(c(10.0, 10.51)) and "CPU-CONFOUNDED(B64-448)" in p57.full_label57a(c(9.4, 10.0))
      and "CPU-CONFOUNDED" not in p57.full_label57a(c(10.0, 10.0)) and p57.full_label57a(c(10.0, 10.51)).startswith("E1 "), [c(10.0, 10.49)["cpu"], c(10.0, 10.51)["cpu"]])
check("CPU confounding annotates the reading, it never replaces it (E1 stays E1)", c(10.0, 11.0)["label"] == "E1")
bi = lambda r256, r512: p57.outcome57a(trio(rate=(r512, 0.3, r256)))
check("collapse-rate gate: |rate B256-512 - rate B64-512| > 0.25 -> BIASED (0.5 vs 0.25 = 0.25 exactly: not; 0.51 vs 0.25: yes), annotating the reading",
      bi(0.5, 0.25)["bias"] is False and bi(0.51, 0.25)["bias"] is True and bi(0.1, 0.5)["bias"] is True and p57.full_label57a(bi(0.51, 0.25)).endswith(" BIASED") and bi(0.51, 0.25)["label"] == "E1")
check("E5: release batch / W at W 512 outside 0.17 - 0.24 in a 512 arm (inclusive band: 87.04 / 122.88 do not fire; 86.9 and 123 do); B64-448 (W 448) does not fire E5",
      p57.e5_57a(dict(trio(), B64_512=stat(1.0, release=0.17 * 512), B256_512=stat(1.0, release=0.24 * 512)))[0] is False
      and p57.e5_57a(dict(trio(), B64_512=stat(1.0, release=86.9)))[0] is True and p57.e5_57a(dict(trio(), B256_512=stat(1.0, release=123.0)))[0] is True
      and p57.e5_57a(dict(trio(), B64_448=stat(1.0, release=10.0)))[0] is False)


# ═══ 5. the 57a / 57b reports on synthetic ledgers, the T rules, the smoke ═══
def mk_a(arm, i, R, burst, cpu=10.0, flagged=False, dnf=False, bind=1.0, release=100.0):
    return {"cell": "c1s-400", "arm": arm, "seed": "42" if i % 2 == 0 else "7", "rep": i + 1, "status": "LIVE", "problems": [], "rc": 0, "dnf": dnf, "mbps": None if dnf else 800.0, "locked": False,
            "qcwnd_below": {"per_leg": [0.5 if flagged else 0.0]},
            "t56": {"goodput_mbps": 800.0, "r0": R, "tx_sum_late": R, "bind_frac": bind, "cpu_cli_s_per_gb": cpu, "cpu_srv_s_per_gb": 9.0, "qc_share": 0.5 if flagged else 0.0},
            "t57": {"burst": burst, "release": release, "wakes_loop": 4.0, "aW": 0.85, "busy": 45.0, "ackw": 82.0, "Q": 100.0, "q_top": 1.2}}


def ledger_a(R512, R448, R256, b256=96.0, n=21, flagged=None, cpu=(10.0, 10.0, 10.0), extra=(), extra_flagged=None):
    flagged = flagged or {}
    rows = []
    for arm, R, b, cp in (("B64_512", R512, 48.0, cpu[0]), ("B64_448", R448, 48.0, cpu[1]), ("B256_512", R256, b256, cpu[2])):
        for i, v in enumerate(vals21(R, 1100.0)[:n]):
            fl = i < flagged.get(arm, 0)
            rows.append(mk_a(arm, i, v, b, cp, flagged=fl))
        for j in range((extra_flagged or {}).get(arm, 0)):
            rows.append(mk_a(arm, 100 + j, 30000.0, b, cp, flagged=True))        # collapse rows on top of the 21 unflagged ones: n_u stays 21
    return rows + list(extra)


def run_report(rows, plan="57a"):
    with tempfile.TemporaryDirectory() as td:
        led = os.path.join(td, "ld.log")
        with open(led, "w") as fh:
            fh.write("".join("P38ROW " + json.dumps(r) + "\n" for r in rows) + "LD-BATTERY-DONE tag=ld\n")
        _, txt = capture(p57.score, plan, [led])
    return txt


def verdict(txt, key):
    m = re.search(r"^VERDICT57[AB] .*?\b%s=(\S+)" % key, txt, re.M)
    return m.group(1) if m else None


for want_lab, args in (("E1", (82500.0, 78500.0, 81000.0)), ("E2", (82500.0, 78500.0, 82500.0)), ("RISING", (82500.0, 78500.0, 84000.0)), ("UNDERPOWERED", (82500.0, 82000.0, 82500.0)),
                       ("PARTIAL", (82500.0, 78500.0, 79500.0)), ("INCONCLUSIVE", (82500.0, 78500.0, 81860.0))):
    txt = run_report(ledger_a(*args))
    check(f"57a report on a synthetic 63-row ledger ({want_lab}): VERDICT57A outcome={want_lab}, the OUTCOME block, the gates, the re-base line and E5 are printed", verdict(txt, "outcome") == want_lab and "OUTCOME" in txt
          and "CPU GATE" in txt and "COLLAPSE GATE" in txt and "RE-BASING" in txt and "E5 (" in txt and "slots=63/63" in txt and "live=63" in txt, txt[-1200:])
txt = run_report(ledger_a(82500.0, 78500.0, 81000.0, b256=60.0))
check("57a report: a realised B256-512 burst <= 64 -> outcome=INERT", verdict(txt, "outcome") == "INERT", txt[-700:])
txt = run_report(ledger_a(82500.0, 78500.0, 81000.0, cpu=(10.0, 10.0, 11.0), extra_flagged={"B256_512": 9}))
check("57a report: CPU +10 % on B256-512 -> label E1_CPU-CONFOUNDED(B256-512); 9 collapse rows on top of its 21 (rate 9/30 = 0.30 against 0 at B64-512 > 0.25) -> BIASED; the flagged rows enter no median (n_u stays 21)",
      verdict(txt, "label") == "E1_CPU-CONFOUNDED(B256-512)_BIASED" and verdict(txt, "outcome") == "E1" and "n_u 21" in txt and "COLLAPSE 9" in txt, (verdict(txt, "label"), txt[-1500:]))
txt = run_report(ledger_a(82500.0, 78500.0, 81000.0, flagged={"B64_448": 14}))
check("57a report: a 448 arm with 7 unflagged rows (14 of 21 collapse-flagged) is UNREAD (the minimum of 8 unflagged rows per arm); the flagged rows enter no median", verdict(txt, "outcome") == "UNREAD" and "n_u 7" in txt, txt[-500:])
txt = run_report(ledger_a(82500.0, 78500.0, 81000.0, extra=[mk_a("B256_512", 30, 50000.0, 96.0, bind=0.5)]))
check("57a report: an UNBOUND row (bind fraction 0.5 < 0.95) is reported at its cap and stays out of the arm medians (n_u unchanged at 21, UNBOUND 1)", "UNBOUND 1" in txt and verdict(txt, "outcome") == "E1", txt[-900:])
txt = run_report(ledger_a(88000.0, 84000.0, 86500.0))
check("57a report: B64-512 in-session median 88 000 is outside 82.7 k +- 3.0 k -> RE-BASED (drift +5300), reported, the reading still comes from the in-session arms only", "RE-BASED (drift +5300" in txt and verdict(txt, "outcome") == "E1", txt[-900:])
txt = run_report(ledger_a(82500.0, 78500.0, 81000.0))
check("57a report: B64-512 at 82 500 is within tolerance (no re-base) and E5 does not fire at release 100 / 512 = 0.195", "within tolerance" in txt and "E5 (release batch / W at W 512 outside 0.17-0.24 in a 512 arm): DOES-NOT-FIRE" in txt, txt[-900:])


# 57b
def mk_b(pool, i, gp, q, aw=0.9, flagged=False, flag_legs=None, dnf=False, bind=1.0):
    legs = flag_legs if flag_legs is not None else ([0.5, 0.5] if flagged else [0.0, 0.0])
    return {"cell": "c1d-400", "arm": "P%d" % pool, "seed": "42" if i % 2 == 0 else "7", "rep": i + 1, "status": "LIVE", "problems": [], "rc": 0, "dnf": dnf, "mbps": None if dnf else gp, "locked": False,
            "qcwnd_below": {"per_leg": legs},
            "t56": {"goodput_mbps": gp, "tx_sum_late": 90000.0, "bind_frac": bind, "cpu_cli_s_per_gb": 10.0},
            "t57": {"q_ell": q, "aW": aw, "ell_hat_ms": 2.5, "release": 150.0, "wakes_loop": 8.0}}


CTRL_GP = 100.0
# pools -> (q_l, goodput / control): the c1s curve shape, c1d-like; interpolated at 2.1 / 2.7 ms the three tests below read 0.771 / 0.864 (PASS), shifted by -0.12 (LOW) or +0.12 (HIGH)
CURVE = {192: (1.9, 0.74), 256: (3.0, 0.91), 384: (4.5, 0.94), 512: (7.0, 0.96), 768: (12.0, 0.99)}


def ledger_b(curve=None, shift=0.0, n=5, aw=None, ctrl_q=18.0, flagged_rows=None, ctrl_gp=CTRL_GP):
    curve = curve or CURVE
    aw = aw or {}
    fr = flagged_rows or {}
    nfl_c = fr.get(2048, (0, 0))[0]
    rows = [mk_b(2048, i, (fr[2048][1] if i < nfl_c else 1.0) * ctrl_gp, ctrl_q, aw=0.5, flagged=i < nfl_c) for i in range(n)]
    for pool, (q, ratio) in curve.items():
        for i in range(n):
            nfl = fr.get(pool, (0, 0))[0] if pool != 2048 else 0
            if i < nfl:
                rows.append(mk_b(pool, i, fr[pool][1] * ctrl_gp, q, aw=aw.get(pool, 0.9), flagged=True))
            else:
                rows.append(mk_b(pool, i, (ratio + shift) * ctrl_gp, q, aw=aw.get(pool, 0.9)))
    return rows


def sb(rows):
    by = {p: [r for r in rows if r["arm"] == "P%d" % p] for p in pp.POOLS57B}
    return p57.score_b(by)


S0 = sb(ledger_b())
check("T on the c1s-like curve: both readings PASS (c1d reads 0.771 / 0.864 at q_l 2.1 / 2.7 against 0.79 / 0.89 +- 0.08), the points come from the five store-bound pools, the control is only the denominator",
      S0["T"] == "PASS" and S0["a_all"]["state"] == "PASS" and S0["a_unf"]["state"] == "PASS" and S0["unf"]["points_pools"] == [192, 256, 384, 512, 768]
      and [round(v, 3) for _, v, _ in S0["a_unf"]["detail"]] == [0.771, 0.864], (S0["T"], S0["a_unf"]["detail"]))
check("(a) fires LOW when c1d reads 0.12 below at both points, HIGH when 0.12 above; LOW at one point and HIGH at the other is MIXED", sb(ledger_b(shift=-0.12))["T"] == "LOW" and sb(ledger_b(shift=0.12))["T"] == "HIGH"
      and p57.combine_points(["LOW", "HIGH"]) == "MIXED" and p57.combine_points(["LOW", "PASS"]) == "LOW" and p57.combine_points(["HIGH", "UNSCOREABLE"]) == "HIGH"
      and p57.combine_points(["PASS", "UNSCOREABLE"]) == "UNSCOREABLE" and p57.combine_points(["PASS", "PASS"]) == "PASS", (sb(ledger_b(shift=-0.12))["T"], sb(ledger_b(shift=0.12))["T"]))
check("the band is +-0.08: d = -0.08 fires LOW (at least 0.08 below), -0.0799 passes; +0.08 fires HIGH, +0.0799 passes", p57.point_state(-0.08) == "LOW" and p57.point_state(-0.0799) == "PASS" and p57.point_state(0.08) == "HIGH"
      and p57.point_state(0.0799) == "PASS" and p57.point_state(0.0) == "PASS" and p57.BAND == 0.08)
check("no extrapolation: interpolation outside the arm medians' q_l range is None; inside, linear; at an end point exact",
      p57.interp([(2.0, 0.7), (4.0, 0.9)], 1.99) is None and p57.interp([(2.0, 0.7), (4.0, 0.9)], 4.01) is None and near(p57.interp([(2.0, 0.7), (4.0, 0.9)], 3.0), 0.8) and near(p57.interp([(2.0, 0.7), (4.0, 0.9)], 2.0), 0.7)
      and p57.interp([], 2.0) is None)
st, det = p57.eval_a([(2.4, 0.85), (5.0, 0.94)])
check("UNSCOREABLE outside c1d's measured range: the smallest arm at q_l 2.4 leaves the 2.1 point outside -> that point UNSCOREABLE; 2.7 inside and passing -> the pair is UNSCOREABLE (no partial pass)",
      st == "UNSCOREABLE" and det[0][2] == "UNSCOREABLE" and det[1][2] in ("PASS", "LOW", "HIGH"), (st, det))
st2, _ = p57.eval_a([(2.4, 0.60), (5.0, 0.80)])
check("... but a scoreable point that FIRES is reported even when the other point is outside the range (2.7 reads 0.69 vs 0.89: LOW)", st2 == "LOW", st2)
check("at least one c1d arm must fall in q_l <= 3.0 ms: smallest arm at 3.4 -> UNSCOREABLE both points, at exactly 3.0 -> the 2.1 point is outside (no extrapolation) but the check passes",
      p57.eval_a([(3.4, 0.9), (6.0, 0.95)])[0] == "UNSCOREABLE" and all(s == "UNSCOREABLE" for _, _, s in p57.eval_a([(3.4, 0.9), (6.0, 0.95)])[1])
      and p57.eval_a([(3.0, 0.9), (6.0, 0.95)])[1][0][2] == "UNSCOREABLE" and p57.eval_a([(3.0, 0.9), (6.0, 0.95)])[0] == "UNSCOREABLE")
nocurve = {k: v for k, v in CURVE.items() if k != 192}
check("a sweep whose smallest store-bound arm sits at q_l 3.0 ms has no point at 2.1: T is UNSCOREABLE", sb(ledger_b(curve=nocurve))["T"] == "UNSCOREABLE", sb(ledger_b(curve=nocurve))["T"])
# occupancy
S1 = sb(ledger_b(aw={192: 0.79}))
S2 = sb(ledger_b(aw={192: 0.80}))
check("the occupancy rule: a pool with mean a/W 0.79 is NOT-STORE-BOUND and out of T (T then has no arm at q_l 2.1: UNSCOREABLE); at exactly 0.80 it is READ",
      S1["unf"]["pools"][192]["status"] == "NOT-STORE-BOUND" and 192 not in S1["unf"]["points_pools"] and S1["T"] == "UNSCOREABLE" and S2["unf"]["pools"][192]["status"] == "READ" and 192 in S2["unf"]["points_pools"] and S2["T"] == "PASS",
      (S1["unf"]["pools"][192]["status"], S1["T"], S2["T"]))
check("the control (mean a/W 0.5 < 0.80) is never a T point: it supplies the goodput denominator and the (c) readout only", 2048 not in S0["unf"]["points_pools"] and S0["unf"]["pools"][2048]["status"] == "READ" and S0["unf"]["control_ok"])
# minimum 3 unflagged rows
S3 = sb(ledger_b(flagged_rows={192: (3, 0.5)}))
S4 = sb(ledger_b(flagged_rows={192: (2, 0.5)}))
check("at least 3 unflagged rows per pool for any reading: P192 with 2 unflagged of 5 is UNREAD (out of both readings), with 3 it is READ", S3["unf"]["pools"][192]["status"] == "UNREAD" and S3["all"]["pools"][192]["status"] == "UNREAD"
      and 192 not in S3["unf"]["points_pools"] and S4["unf"]["pools"][192]["status"] == "READ" and S3["T"] == "UNSCOREABLE" and S4["unf"]["pools"][192]["n_u"] == 3, (S3["unf"]["pools"][192]["status"], S4["unf"]["pools"][192]["status"]))
S5 = sb(ledger_b(flagged_rows={2048: (3, 0.3)}))
check("a control with fewer than 3 unflagged rows makes T UNSCOREABLE (no denominator)", S5["T"] == "UNSCOREABLE" and not S5["unf"]["control_ok"], S5["T"])
# the two readings
S6 = sb(ledger_b(flagged_rows={192: (2, 0.30), 256: (2, 0.30)}))
check("T is read on all rows AND on unflagged rows: with 2 collapse rows of 5 (goodput 0.3 x the control) in P192 and P256 the medians of 3 good rows stay good in both readings -> they agree, T PASS",
      S6["a_unf"]["state"] == "PASS" and S6["a_all"]["state"] == "PASS" and S6["T"] == "PASS" and S6["unf"]["pools"][192]["n_u"] == 3 and S6["all"]["pools"][192]["n"] == 5, (S6["a_all"]["state"], S6["a_unf"]["state"]))
rows9 = ledger_b()
for r in rows9:
    if r["arm"] == "P192":
        g = {1: 50.0, 2: 74.0, 3: 90.0}.get(r["rep"])
        if g is not None:
            r["t56"]["goodput_mbps"], r["mbps"] = g, g
        else:
            r["qcwnd_below"] = {"per_leg": [0.5, 0.5]}
            r["t56"]["goodput_mbps"], r["mbps"] = 100.0, 100.0        # the collapse rows read ABOVE the good ones: the all-rows median (90) leaves the unflagged median (74)
S9 = sb(rows9)
check("flag disagreement: P192 unflagged 50 / 74 / 90 (median 74 -> PASS) with two flagged rows at 100 (all-rows median 90 -> HIGH at q_l 2.1): the readings disagree -> T is UNSCOREABLE",
      S9["a_unf"]["state"] == "PASS" and S9["a_all"]["state"] == "HIGH" and S9["T"] == "UNSCOREABLE", (S9["a_all"]["state"], S9["a_unf"]["state"], S9["T"]))
rows10 = ledger_b()
for r in rows10:
    if r["arm"] in ("P192", "P256") and r["rep"] in (3, 4, 5):
        r["qcwnd_below"] = {"per_leg": [0.5, 0.5]}
        r["t56"]["goodput_mbps"], r["mbps"] = 5.0, 5.0
S10 = sb(rows10)
check("P192 / P256 with 3 of 5 rows flagged are UNREAD in both readings (2 unflagged rows) and drop out: no arm at q_l <= 3.0 remains -> T UNSCOREABLE",
      S10["unf"]["pools"][192]["status"] == "UNREAD" and S10["T"] == "UNSCOREABLE", (S10["a_all"]["state"], S10["a_unf"]["state"], S10["T"]))
check("combine_readings: equal states stand; PASS against LOW (one passes, the other fails) -> UNSCOREABLE; UNSCOREABLE with PASS -> UNSCOREABLE", p57.combine_readings("PASS", "PASS") == "PASS" and p57.combine_readings("LOW", "LOW") == "LOW"
      and p57.combine_readings("PASS", "LOW") == "UNSCOREABLE" and p57.combine_readings("UNSCOREABLE", "PASS") == "UNSCOREABLE" and p57.combine_readings("LOW", "HIGH") == "UNSCOREABLE")
# the flag
rows11 = ledger_b()
for r in rows11:
    if r["arm"] == "P384" and r["rep"] == 1:
        r["qcwnd_below"] = {"per_leg": [0.2, 0.0]}      # one leg only: NOT flagged
    if r["arm"] == "P384" and r["rep"] == 2:
        r["qcwnd_below"] = {"per_leg": [0.2, 0.2]}      # both legs: flagged
S11 = sb(rows11)
pl = S11["unf"]["pools"][384]
check("the c1d flag in the sweep: a row with ONE leg above 0.10 stays unflagged (counted as one-leg-only), a row with BOTH legs flagged leaves the unflagged reading; shares reported per pool",
      pl["both"] == 1 and pl["one_leg"] == 1 and pl["flagged"] == 1 and pl["n_u"] == 4 and S11["all"]["pools"][384]["n"] == 5, pl)
# the offset rule
OFFSET_CURVE = {192: (0.8, 0.50), 256: (3.0, 0.90), 384: (4.5, 0.94), 512: (7.0, 0.96), 768: (12.0, 0.99)}
S12 = sb(ledger_b(curve=OFFSET_CURVE))
check("OFFSET DIFFERS: d0(c1d) = the smallest pool's q_l = 0.8 ms (|0.8 - 1.5| = 0.7 > 0.5): raw axis PASSes (0.736 / 0.845 against 0.79 / 0.89) but the offset-corrected axis (c1d read at 2.1 - 0.7 = 1.4 and 2.0) FIRES LOW -> OFFSET DIFFERS, "
      "the queue relation is not tested", near(S12["unf"]["delta0"], 0.8) and S12["a_unf"]["raw"] == "PASS" and S12["a_unf"]["corrected"] == "LOW" and S12["T"] == "OFFSET DIFFERS", (S12["a_unf"], S12["T"]))
S13 = sb(ledger_b(curve={**CURVE, 192: (2.0, 0.74)}))
check("offsets within 0.5 ms (d0 = 2.0, |2.0 - 1.5| = 0.5 is not > 0.5): no corrected axis is read, T is the raw axis", near(S13["unf"]["delta0"], 2.0) and S13["a_unf"]["corrected"] is None and S13["a_unf"]["shift"] is not None, S13["a_unf"])
check("read_a combination: raw PASS + corrected PASS = PASS; raw LOW + corrected LOW = LOW; either axis UNSCOREABLE = UNSCOREABLE; raw LOW + corrected HIGH = MIXED",
      p57.read_a([(0.8, 0.79), (2.7, 0.89), (5.0, 0.95)], 0.8)["state"] in ("PASS", "OFFSET DIFFERS", "LOW", "HIGH", "MIXED")
      and p57.read_a([(0.5, 0.50), (6.0, 0.95)], 0.8)["raw"] != "PASS" and p57.read_a([(2.0, 0.78), (6.0, 0.95)], 0.2)["state"] == "UNSCOREABLE")
# (c)
for q, want in ((8.0, "LOW"), (9.99, "LOW"), (10.0, "OUT"), (12.9, "OUT"), (13.0, "IN"), (21.0, "IN"), (21.1, "OUT")):
    check(f"(c) the c1d control's own queue q_l = {q} ms -> {want} (predicted 13 - 21 ms; below 10 ms the reference is a different operating point)", sb(ledger_b(ctrl_q=q))["ctrl_q_state"] == want, sb(ledger_b(ctrl_q=q))["ctrl_q_state"])
txt = run_report(ledger_b(ctrl_q=8.0), "57b")
check("57b report: the control-queue line is printed BEFORE (a) and says CONTROL-QUEUE-LOW below 10 ms", "CONTROL-QUEUE-LOW" in txt and txt.index("(c) THE C1D CONTROL") < txt.index("(a) unflagged rows"), txt[:1500])
txt = run_report(ledger_b(), "57b")
check("57b report on a synthetic 30-row ledger: per-pool table, the offsets line, (a) on both readings, (b) at 4.7 / 6.8 ms (reported only) and the 0.95 crossing, T = PASS, VERDICT57B with T=PASS and slots=30/30",
      verdict(txt, "T") == "PASS" and "slots=30/30" in txt and "OFFSETS d0" in txt and "(b) unflagged rows (reported only)" in txt and "c1d q_l at the 0.95 point" in txt and "P192" in txt and "T = PASS" in txt, txt[-1500:])
check("57b (b): reported only - c1d reads 0.942 at q_l 4.7 (interpolated on the arm medians) and the 0.95 crossing sits at q_l 5.75 ms",
      near(p57.interp(S0["unf"]["points"], 4.7), 0.9416, 0.001) and near(p57.crossing(S0["unf"]["points"], 0.95), 5.75, 0.01) and p57.crossing([(1.0, 0.5), (2.0, 0.6)], 0.95) is None and near(p57.crossing([(1.0, 0.9), (3.0, 1.0)], 0.95), 2.0))
# readouts
pls = S0["unf"]["pools"]
check("per-pool readouts: goodput/control, q_l, R, a/W, release batch, ack wakes per loop, both-leg / one-leg flag counts and CPU/GB are present", pls[192]["ratio"] is not None and pls[192]["q_ell"] == 1.9 and pls[192]["R"] == 90000.0
      and pls[192]["aW"] == 0.9 and pls[192]["release"] == 150.0 and pls[192]["wakes_loop"] == 8.0 and pls[192]["cpu"] == 10.0 and "both" in pls[192] and "one_leg" in pls[192] and pls[192]["W"] == 384)


# ═══ the smoke criteria, on rows built by the real row parser from the stub driver's logs (stubrun_gen.py) ═══
def real_row(plan, arm, bad="", seed="42", rep=1):
    """One ledger row of `arm` as the battery would parse it: stubrun_gen.py writes the logs a driver run leaves (the arm's env RWM_STORE_PATH_POOL / RWM_EMIT_BURST set as the battery sets
    them), place38_parse.make_row parses them under LD_PLAN=<plan>."""
    pool, burst = pp.arm_knobs57(plan, arm)
    cell = "c1s-400" if plan == "57a" else "c1d-400"
    with tempfile.TemporaryDirectory() as td:
        env = {k: v for k, v in os.environ.items() if not k.startswith(("RWM_", "STUB_", "LD_"))}
        env.update({"LD_PLAN": plan, "STUB_TMP": td, "SEED": seed, "STUB_BAD57": bad})
        if pool != 2048:
            env["RWM_STORE_PATH_POOL"] = str(pool)
        if plan == "57a":
            env["RWM_EMIT_BURST"] = str(burst)
        cp = subprocess.run([sys.executable, os.path.join(HERE, "stubrun_gen.py"), "c1", "c1", "bulk", "5000000000", "1", "single" if plan == "57a" else "dual"], env=env, capture_output=True, text=True)
        assert cp.returncode == 0, cp.stderr[-500:]
        drv, cli, srv = cp.stdout.replace("\r", "").split("\n"), lc.read(os.path.join(td, "rwm-c.log")), lc.read(os.path.join(td, "rwm-s.log"))
    os.environ["LD_PLAN"], os.environ["LD_SHA"], os.environ["LD_ROW_BYTES"] = plan, "s" * 64, "5000000000"
    try:
        return pp.make_row(cell, arm, seed, rep, 0, 100, drv, cli, srv, 0, "s" * 64)
    finally:
        for k in ("LD_PLAN", "LD_SHA", "LD_ROW_BYTES"):
            os.environ.pop(k, None)


def smoke_text(rows, plan):
    with tempfile.TemporaryDirectory() as td:
        led = os.path.join(td, "smoke.log")
        with open(led, "w") as fh:
            fh.write("".join("P38ROW " + json.dumps(r) + "\n" for r in rows))
        return capture(p57.smoke, plan, led)


good = real_row("57a", "B256_512")
rc_, txt = smoke_text([good], "57a")
check("57a smoke PASS: the B256-512 row echoes pool 512 and burst 256 on both ends, a realised mean burst of 192 > 64 (stub: 0.75 x the knob), every readout computed",
      rc_ == 0 and "SMOKE-PASS" in txt and re.search(r"^PASS the knob acts", txt, re.M) and good["status"] == "LIVE" and near(good["t57"]["burst"], 192.0, 1.0), (rc_, txt[-1300:], good["problems"], good["t57"]["burst"]))
inert = real_row("57a", "B256_512", bad="burstinert")
rc_, txt = smoke_text([inert], "57a")
check("57a smoke ABORT: the B256-512 row with a realised mean burst of 48 (<= 64: RWM_EMIT_BURST does not act) FAILS the 'knob acts' criterion (SMOKE-FAIL, rc 1: the stage prints ABORT-SMOKE)",
      inert["status"] == "LIVE" and rc_ == 1 and re.search(r"^FAIL the knob acts", txt, re.M) and "SMOKE-FAIL" in txt and near(inert["t57"]["burst"], 48.0, 1.0), (rc_, txt[-900:], inert["problems"]))
edge = dict(good, t57=dict(good["t57"], burst=64.0))
edge2 = dict(good, t57=dict(good["t57"], burst=64.01))
check("the burst criterion is strict: a realised burst of exactly 64.0 fails, 64.01 passes (> 64 required)", smoke_text([edge], "57a")[0] == 1 and smoke_text([edge2], "57a")[0] == 0)
wrong = real_row("57a", "B256_512", bad="burstfallback")
rc_, txt = smoke_text([wrong], "57a")
check("57a smoke: a row whose [GATES] echo reads the shipped burst 64 for the 256 request is VOID by the echo (WITNESS-FAIL gate-burst on both ends) and fails the smoke on the pool-AND-burst criterion",
      wrong["status"] == "WITNESS-FAIL" and any(x.startswith("gate-burst-cli=64") for x in wrong["problems"]) and any(x.startswith("gate-burst-srv=64") for x in wrong["problems"]) and rc_ == 1
      and re.search(r"^FAIL each row's pool AND burst", txt, re.M), (wrong["problems"], txt[-700:]))
wrong2 = real_row("57a", "B256_512", bad="poolfallback")
check("57a smoke: a row echoing pool 2048 for the 512 request fails on both ends (gate-pool-cli, gate-pool-srv, pool_per_path)",
      any(x.startswith("gate-pool-cli=2048") for x in wrong2["problems"]) and any(x.startswith("gate-pool-srv=2048") for x in wrong2["problems"]) and smoke_text([wrong2], "57a")[0] == 1 and pp.is_echo_void57(wrong2), wrong2["problems"])
good_b = [real_row("57b", "P192"), real_row("57b", "P2048")]
rc_, txt = smoke_text(good_b, "57b")
check("57b smoke PASS: pools 192 and 2048, the pool echo on both ends and the shipped burst 64, every readout (q_l, release, wakes) computed; the 57b smoke has no burst criterion",
      rc_ == 0 and "SMOKE-PASS" in txt and "the knob acts" not in txt and all(r["status"] == "LIVE" for r in good_b), (rc_, txt[-900:], [r["problems"] for r in good_b]))
rc_, txt = smoke_text(good_b[:1], "57b")
check("57b smoke: one row missing -> FAIL 'one row per smoke slot ran'", rc_ == 1 and re.search(r"^FAIL one row per smoke slot ran", txt, re.M) is not None)
dnf_row = dict(good_b[0], dnf=True)
rc_, txt = smoke_text([dnf_row, good_b[1]], "57b")
check("57b smoke: an unflagged DNF row FAILS the 300 s criterion (a collapse-flagged one would not)", rc_ == 1 and re.search(r"^FAIL every smoke row finished", txt, re.M) is not None, txt[-500:])
leak = real_row("57b", "P192", bad="burstfallback")
check("57b: the stub's burst fallback leaves the shipped 64 echoed (what 57b requires), so no gate-burst problem; a leaked RWM_EMIT_BURST=256 would fail it (witness tests above)", not any(x.startswith("gate-burst") for x in leak["problems"]))
# the whole T report on rows from the stub driver: the pipeline row parser -> t56 / t57 -> score_b
stub_rows = [real_row("57b", "P%d" % pool, seed="42" if k % 2 == 0 else "7", rep=k + 1) for k in range(2) for pool in pp.POOLS57B]
txt = run_report(stub_rows, "57b")
check("57b report on 12 parsed stub rows (2 replicates x 6 pools): every pool listed with its q_l (the stub's echo rtt grows with the pool), T and the VERDICT57B line print", "VERDICT57B" in txt and "(c) THE C1D CONTROL" in txt and "T = " in txt
      and all(("P%d" % p) in txt for p in pp.POOLS57B) and "T57-ERROR" not in txt, txt[-1500:])
stub_a = [real_row("57a", a, seed="42" if k % 2 == 0 else "7", rep=k + 1) for k in range(2) for a in pp.ORDER57A]
txt = run_report(stub_a, "57a")
check("57a report on 6 parsed stub rows: b-bar 48 / 48 / 192, below 8 unflagged rows per arm -> outcome UNREAD, the readouts and gates still print", verdict(txt, "outcome") == "UNREAD" and "b-bar256 192" in txt and "T57-ERROR" not in txt, txt[-1300:])


# ═══ END ═══
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
