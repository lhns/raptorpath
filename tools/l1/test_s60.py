#!/usr/bin/env python3
"""Tests for the status 60 scorer (tools/l1/s60_score.py; the registration docs/derivations/s60-registration.md), in the style of test_mbar59.py: plain check(name, ok, detail) calls,
synthetic client logs with KNOWN answers, exit code = number of fails.

Every readout is exercised through the WHOLE chain: a synthetic client log -> s60_score.row_metrics -> a P38ROW ledger line -> s60_score.score / smoke -> the printed lines and LABEL60s.
The generator prints [SLAW] windows whose R / Rb are computed with s60replay.slaw_r from the PRINTED (3-decimal) values, so the per-row replay witness passes by construction and fails on the
tmin+1us negative control.
Run: python3 test_s60.py (the VM runs it under `tst parsers`)."""
import re
import json
import math
import os
import random
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_NOGO"):
    os.environ.pop(_k, None)
import plan60  # noqa: E402
import s60_score as s  # noqa: E402

SR, CR, _ERR = s._lib()
fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol=1e-6):
    return a is not None and b is not None and abs(a - b) <= tol


C2, C8, C1S = s.C2, s.C8, s.C1S

# ═══ the synthetic client log ═══
DEFAULT_B0K = [0.0, 0.2, 0.5, 0.8, 1.0, 1.0, 0.9, 0.7]


def caplog_line(i, ok=True, pool=64):
    r = {"i": i, "t": 1000 * i, "seat": "plain", "br": "pooled", "on": True, "sum": True, "dc": True, "b": 1.0, "q": 0.075, "n": 1, "psum": 100.0, "gain": 1.1, "floor": 10, "pool": pool,
         "smax": 2048, "boot": 128, "cap": 0, "rep": 0}
    r["cap"] = int(CR.replay(r)) + (0 if ok else 1)
    return (f"[CAPLOG] i={i} t={r['t']} seat=plain br=pooled on=1 sum=1 dc=1 b=1.0 q=0.075 n=1 psum=100.0 gain=1.1 floor=10 pool={pool} smax=2048 boot=128 cap={r['cap']} rep=0")


def gen_cli(cell, arm, legs, N=80, dt=0.25, gauge=True, drop=(), qstr="0.100000", tamper_tmin=0.0, caplog_ok=True, rtt_top=5.0, diag_rtt=None, rtp_us=None, n_grow=None,
            warm_junk=True, src=1000, caplog_n=5, tail_nk=None):
    """legs: {leg: dict(tmin, tp50, m, l, nk, sig, sigb, b0k, b0kn)} (constants). diag_rtt / rtp_us / n_grow: per-leg overrides of the [DIAG] ticks."""
    L = []
    if warm_junk:
        L += (['[SLAW] t=9.90 k=1 p=0 n=1 W=1 aw=0.5 nk=5 tmin=1.000 tp50=1.000 tmean=1.000 m=1.000 l=1.000 b0p=- b0run=0.1 b0k=- b0kn=- kw=1 sig=0.100 sigw=0.1 sigb=- gap=- anc=- ancf=0/0 err=- R=- Rb=- used=- xs=0 xb=0 ra=0/0 q=0.050000 qb=- qbt=- qcol=1/1'] if gauge else []) + ['[DIAG] t=9.9s win=1/1 src=5000sym/s cod=9sym/s cum=9/9/9 rtt=99.0ms p0:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0150 cmp=- rtt=99/wrtt=3/rtp2ms rtp_us=1 sig_us=1/n1 rvar_us=1/n1',
              '{"seconds":0.01,"warmup":true}']
    if gauge:
        L += [caplog_line(i, caplog_ok or i != 3) for i in range(1, caplog_n + 1)]
    legn = sorted(legs)
    for k in range(1, N + 1):
        t = dt * k
        ready = k >= s.K_BOOT
        wl = {}
        for p in legn:
            d = dict(legs[p])
            nk = d.get("nk", 100)
            if tail_nk is not None and t >= tail_nk[0]:
                nk = tail_nk[1].get(p, nk)
            l = d["l"]
            m = l if (t < 10.0 and d.get("m_early_eq", True)) else d["m"]
            wl[p] = {"nk": nk, "tmin": d["tmin"], "tp50": d.get("tp50", d["tmin"] + 8.0), "m": m, "l": l, "sig": d.get("sig", 0.1), "sigb": d.get("sigb", 0.1),
                     "b0k": d.get("b0k", DEFAULT_B0K), "b0kn": d.get("b0kn", [20] * 8)}
        printed = {}
        for p in legn:
            w = wl[p]
            printed[p] = {key: float("%.3f" % w[key]) for key in ("tmin", "tp50", "m", "l", "sig", "sigb")}
        b0p = {p: max(wl[p]["b0k"]) for p in legn}
        R = Rb = None
        if ready:
            live = [(wl[p]["nk"], printed[p]["tp50"], printed[p]["tmin"], printed[p]["m"], printed[p]["l"], b0p[p], printed[p]["sig"]) for p in legn if wl[p]["nk"] > 0]
            liveb = [(a, b, c, dd, e, f_, printed[p]["sigb"]) for (a, b, c, dd, e, f_, g), p in zip(live, [p for p in legn if wl[p]["nk"] > 0])]
            if live:
                R, Rb = SR.slaw_r(live, 0.1), SR.slaw_r(liveb, 0.1)
        for p in legn:
            w = wl[p]
            if gauge:
                if "WSTAT" not in drop:
                    L.append(f"[WSTAT] t={t:.2f} k={k} p={p} nk={w['nk']} nkq={w['nk']} tmin={w['tmin']:.3f} p10=1 p50=1 p90=1 tmean=1 m={w['m']:.3f} l={w['l']:.3f} qb50=- qbmax=- qcwnd=500000 bdpb=125000")
                if "SLAW" not in drop:
                    tm = w["tmin"] + tamper_tmin
                    L.append(f"[SLAW] t={t:.2f} k={k} p={p} n=100 W=50 aw=0.5000 nk={w['nk']} tmin={tm:.3f} tp50={w['tp50']:.3f} tmean={w['tp50']:.3f} m={w['m']:.3f} l={w['l']:.3f} "
                             f"b0p={('%.3f' % b0p[p]) if ready else '-'} b0run=0.100 b0k={','.join('%.3f' % x for x in w['b0k']) if k == s.K_BOOT else '-'} "
                             f"b0kn={','.join(str(x) for x in w['b0kn']) if k == s.K_BOOT else '-'} kw={min(k, 8)} sig={w['sig']:.3f} sigw=0.140 sigb={('%.3f' % w['sigb']) if ready else '-'} gap=- "
                             f"anc={'0' if ready else '-'} ancf=0/1 err={'-0.300' if ready else '-'} R={('%.5f' % R) if (ready and R is not None) else '-'} Rb={('%.5f' % Rb) if (ready and Rb is not None) else '-'} "
                             f"used=2 xs=0 xb=0 ra=1/0 q={qstr} qb=- qbt=- qcol=8/8")
        if gauge:
            if "WSTORE" not in drop:
                L.append(f"[WSTORE] t={t:.2f} k={k} tw=250.0 n=100 W=50 aw=0.5000 awmin=0.5000 relb=1/12/12/12 emb=2/16/16/16 adv=2/30/30/30 ackw=1 fec=-/-/-/-/0 dbt_acc=0.00 dbt_em=0 dbt_share=- dbt_rm_ack=0.00")
            if "SLAWC" not in drop:
                L.append(f"[SLAWC] t={t:.2f} k={k} gaprep=0 gaptrunc=0 gaptrunc_frac=- c09rep=0 c09bind=0 c09_frac=-")
        parts = ""
        for p in legn:
            rtt = (diag_rtt or {}).get(p, 5)
            rp = (rtp_us or {}).get(p, 5000)
            ng = (n_grow or {}).get(p, 10)
            parts += (f" p{p}:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0150 cmp=- rtt={rtt}/wrtt=3/rtp2ms rtp_us={rp} sig_us=1/n{ng * k} rvar_us=1/n1 qsp_us=1/n1 qcwnd=500000 | ANCHOR sent=0")
        top = rtt_top(t) if callable(rtt_top) else rtt_top
        L.append(f"[DIAG] t={t:.2f}s win=800/1000 paused=0% good=80.0Mbit src={src}sym/s cod=10sym/s cum={100 * k}/{k}/{90 * k} np={len(legn)} np_act={len(legn)} rtt={top:.1f}ms bdp100=165sym{parts}")
    return L


def mkrow(cell, arm, seed, rep, legs, mbps=100.0, status="LIVE", dnf=False, cpu=None, qcb=None, **kw):
    cli = gen_cli(cell, arm, legs, **kw)
    kind = plan60.arm_parse(arm)[0]
    on = "1" if kind in ("G", "E") else "0"
    srv = [f"[GATES] RWM_SLAW={on} RWM_DIAG=1"]
    cli = [f"[GATES] RWM_SLAW={on} RWM_DIAG=1"] + cli
    t60 = s.row_metrics(cli, srv, [], plan60.arm_bytes(cell, arm), cell, arm, mbps)
    r = {"cell": cell, "store": "def", "arm": arm, "seed": str(seed), "rep": rep, "status": status, "dnf": dnf, "mbps": mbps, "problems": [], "t60": t60,
         "bytes": plan60.arm_bytes(cell, arm), "rc": 0, "cpu_cli": cpu}
    if qcb is not None:
        r["qcwnd_below"] = {"per_leg": [qcb]}
    return r


def ledger(rows, path=None):
    fd, p = tempfile.mkstemp(suffix=".ld.log", dir=path)
    with os.fdopen(fd, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    return p


def run_score(rows, plan, nogo=None):
    p = ledger(rows)
    out = []
    old = os.environ.get("LD_NOGO")
    if nogo:
        os.environ["LD_NOGO"] = nogo
    try:
        rc = s.score([p], plan, out=out.append)
    finally:
        os.environ.pop("LD_NOGO", None)
        if old:
            os.environ["LD_NOGO"] = old
        os.remove(p)
    return rc, out


def labels(out):
    return {ln.split()[1]: ln.split()[2] for ln in out if ln.startswith("LABEL60 ")}


def has(out, text):
    return any(text in ln for ln in out)


def seed_of(rep):
    return 42 if rep % 2 else 7


# ═══ 1. plan tables the scorer relies on ═══
check("plan60: 60a 96 slots, 60b 40 slots, the rehearsal 10 rows", len(plan60.plan("60a")) == 96 and len(plan60.plan("60b")) == 40 and len(plan60.plan("60a", rehearsal=True)) == 10,
      (len(plan60.plan("60a")), len(plan60.plan("60b"))))
check("plan60 bytes: floor pools 100/200/300 MB (c2), 200/400/600 MB (c8), 1/1.5/2 GB (c1s); the rest 600 MB or 5 GB",
      [plan60.arm_bytes(C2, "G%d" % w) for w in (16, 32, 48, 160)] == [100_000_000, 200_000_000, 300_000_000, 600_000_000]
      and [plan60.arm_bytes(C8, "G%d" % w) for w in (32, 64, 96)] == [200_000_000, 400_000_000, 600_000_000]
      and [plan60.arm_bytes(C1S, "G%d" % w) for w in (64, 128, 192, 2048)] == [1_000_000_000, 1_500_000_000, 2_000_000_000, 5_000_000_000])
check("plan60 timeouts: 150 s on every floor pool and the c2/c8/evicting rows, 210 s on the c1s 5 GB control", plan60.arm_timeout(C2, "G16") == 150 and plan60.arm_timeout(C8, "G320") == 150
      and plan60.arm_timeout(C1S, "G2048") == 210 and plan60.arm_timeout(C2, "EVICT") == 150)
check("plan60 LATE_T 15/15/5 and the P1 arms 256/512/2048 agree with the scorer's copies", s.LATE_T == {C2: 15.0, C8: 15.0, C1S: 5.0} and plan60.P1_W == {C2: 256, C8: 512, C1S: 2048})
check("the 60b table is 18 floor + 20 paired + 2 extra G and NO EVICT (status 60 Amendment 2); the pairs are G2048/C2048", sum(1 for r in plan60.plan("60b") if r[2] in ("G64", "G128", "G192")) == 18
      and sum(1 for r in plan60.plan("60b") if r[2] == "C2048") == 10 and sum(1 for r in plan60.plan("60b") if r[2] == "EVICT") == 0)
check("sign test k table (s60cond.py): n 6..12 -> 6 7 8 8 9 10 10", [s.sign_k(n) for n in range(6, 13)] == [6, 7, 8, 8, 9, 10, 10], [s.sign_k(n) for n in range(6, 13)])
check("store-law replay modules import (s60replay selftest numbers available)", SR is not None and CR is not None, _ERR)
if SR:
    SR.selftest()

# ═══ 2. row_metrics on a known log ═══
F2 = {0: dict(tmin=9.45, tp50=20.0, m=8.0, l=6.0, nk=100, sig=2.0, sigb=1.5)}
cli = gen_cli(C2, "G16", F2)
t0 = time.time()
m = s.row_metrics(["[GATES] RWM_SLAW=1"] + cli, ["[GATES] RWM_SLAW=1"], [], 100_000_000, C2, "G16", 90.0)
dt_ = time.time() - t0
check("row_metrics: LATE windows = t >= 15 s at c2 (80 windows of 0.25 s -> k=60..80: 21 windows), warm-up junk before the marker ignored",
      len(m["late"]["0"]["k"]) == 21 and m["late"]["0"]["t"][0] == 15.0 and m["nwin"]["0"] == 80, (len(m["late"]["0"]["k"]), m["nwin"]))
check("row_metrics: columns carry every FIELD and the printed values (tmin 9.45, sig 2.0, nk 100, q 0.1)", set(m["late"]["0"]) == set(s.FIELDS) and m["late"]["0"]["tmin"][0] == 9.45
      and m["late"]["0"]["nk"][0] == 100 and m["late"]["0"]["q"][0] == 0.1 and m["late"]["0"]["sig"][0] == 2.0)
check("row_metrics: boot summary b0k (8), b0kn (8 x 20), b0p = max = 1.0, kw 8", m["boot"]["0"]["b0k"] == DEFAULT_B0K and m["boot"]["0"]["b0kn"] == [20.0] * 8 and m["boot"]["0"]["b0p"] == 1.0
      and m["boot"]["0"]["kw"] == 8, m["boot"])
check("row_metrics: tag counts after the cut (80 of each per-window tag, CAPLOG 5), the pre-marker SLAW line only in all_cli", m["tags"]["cli"] == {"WSTAT": 80, "SLAW": 80, "WSTORE": 80, "SLAWC": 80, "CAPLOG": 5}
      and m["tags"]["all_cli"]["SLAW"] == 81 and sum(m["tags"]["srv"].values()) == 0, m["tags"])
check("row_metrics: distinct q strings, RWM_SLAW echo, t < 10 s m = l witness (39 windows, all equal)", m["q"] == ["0.100000"] and m["gate_slaw"] == {"cli": "1", "srv": "1"}
      and m["ml"]["0"]["n"] == 39 and m["ml"]["0"]["eq"] == 39, (m["q"], m["gate_slaw"], m["ml"]))
check("row_metrics: the replay witness PASSes on the generator's log (R / Rb reproduced to 6e-6)", m["witness"]["ok"] is True and m["witness"]["n"] > 0 and m["witness"]["worst"] <= 6e-6, m["witness"])
check("row_metrics: CAPLOG replay PASS (5 records, 0 mismatches), the pooled clamp binds (ub 1.0, fb 0.0), seat plain", m["caplog"]["ok"] is True and m["caplog"]["records"] == 5 and m["caplog"]["mismatches"] == 0
      and m["caplog"]["ub"] == 1.0 and m["caplog"]["fb"] == 0.0 and m["caplog"]["seats"] == ["plain"], m["caplog"])
m_bad = s.row_metrics(gen_cli(C2, "G16", F2, caplog_ok=False), [], [], None, C2, "G16", 1.0)
check("row_metrics: a corrupted CAPLOG cap -> the replay reports 1 mismatch and ok False", m_bad["caplog"]["mismatches"] == 1 and m_bad["caplog"]["ok"] is False, m_bad["caplog"])
slaw_dicts = [dict(s._TOK.findall(ln)) for ln in cli if ln.startswith("[SLAW]") and 'warmup' not in ln][1:]
w_neg = s.replay_witness(slaw_dicts, dtmin=0.001)
w_pos = s.replay_witness(slaw_dicts)
check("replay witness negative control (every tmin +1 us) FAILS, the positive control PASSes", w_neg["ok"] is False and w_neg["bad"] > 0 and w_pos["ok"] is True, (w_neg, w_pos))
m_t = s.row_metrics(gen_cli(C2, "G16", F2, tamper_tmin=0.001), [], [], None, C2, "G16", 1.0)
check("a log whose printed tmin is off by 1 us from the R it printed -> witness FAIL (the per-row gate)", m_t["witness"]["ok"] is False, m_t["witness"])
tmp = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False)
tmp.write("\n".join(cli))
tmp.close()
sr_w = SR.windows(tmp.name)
os.remove(tmp.name)
my_w = s._windows_lines(slaw_dicts)
check("the lines adapter groups exactly as s60replay.windows(path) does (same k's, same dicts)", sorted(sr_w) == sorted(my_w) and all(sr_w[k] == my_w[k] for k in sr_w))
big = gen_cli(C8, "G32", {0: dict(F2[0], nk=50), 1: dict(F2[0], nk=20, tp50=40.0)}, N=600)
t1 = time.time()
mbig = s.row_metrics(big, [], [], None, C8, "G32", 1.0)
tb = time.time() - t1
size = len(json.dumps(mbig))
check(f"row_metrics on a 150 s two-leg row ({tb:.2f} s, {size} bytes) is fast (< 3 s) and stays under 200 KB", tb < 3.0 and size < 200_000, (round(tb, 2), size))
me = s.row_metrics([], [], [], None, C2, "G16", None)
check("an empty log never raises and reads empty (witness not ok, no tags)", me["witness"]["ok"] is False and not me["late"] and me["tags"]["cli"]["SLAW"] == 0)
mg = s.row_metrics(["[SLAW] t=bad k=x p=0 nk=- tmin=-", "[DIAG] garbage", "[CAPLOG] nonsense", '{"seconds":0.01,"warmup":true}', "[SLAW] t=16.00 k=64 p=0 nk=- tmin=- m=- l=-"], [], [], None, C2, "G16", None)
check("garbled / '-' tokens read None and never raise", mg["late"]["0"]["tmin"] == [None] and mg["late"]["0"]["nk"] == [None])
me2 = s.row_metrics(gen_cli(C2, "C192", F2, gauge=False), ["[GATES] RWM_SLAW=0"], [], None, C2, "C192", 1.0)
check("a C row (gauge off): no gauge tags counted, RWM_SLAW echo 0, [DIAG] ticks still parsed", sum(me2["tags"]["cli"].values()) == 0 and me2["diag"]["n_ticks"] == 80 and me2["gate_slaw"]["srv"] == "0")

# ═══ 2b. the engine's OWN print format (not the generator's) ═══
SAMPLE = os.path.join(HERE, "testdata", "slaw_sample.log")
STORE_LAW_T = os.path.join(HERE, "..", "..", "docs", "derivations", "store-law")
KA = os.path.join(STORE_LAW_T, "s60witness_ka.log")


def lines_of(path):
    return [ln.rstrip("\n") for ln in open(path, encoding="utf-8", errors="replace")]


def after_cut(lines):
    idx = [i for i, ln in enumerate(lines) if s.WARM in ln]
    return lines[idx[-1] + 1:] if idx else lines


for nm, pth in (("renderer sample (slaw_sample.log)", SAMPLE), ("hand-formatted KA witness (anchor-bound leg, nk=0 leg, warm-up to cut)", KA)):
    if not os.path.exists(pth):
        print("SKIP", nm)
        continue
    ls_ = lines_of(pth)
    mm = s.row_metrics(ls_, [], [], None, C8, "G32", 1.0)
    win = SR.windows(pth)
    n_ref = sum(2 for k, v in win.items() if SR.ready(v) and v[0].get("R", "-") != "-")
    ok_ref = SR.witness(pth)
    sl = [dict(s._TOK.findall(ln)) for ln in after_cut(ls_) if ln.startswith("[SLAW]")]
    neg = s.replay_witness(sl, dtmin=0.001)
    check(f"engine format, {nm}: row_metrics witness agrees with s60replay.witness (PASS, {n_ref} comparisons)", mm["witness"]["ok"] is True and ok_ref is True and mm["witness"]["n"] == n_ref and n_ref > 0, (mm["witness"], n_ref))
    check(f"engine format, {nm}: the tmin + 1 us negative control FAILS", neg["ok"] is False and neg["bad"] > 0, neg)
check("engine format: the hand-formatted KA log's warm-up window is cut (3 [SLAW] lines remain, the nk=0 leg's line included in the window count)",
      os.path.exists(KA) and s.row_metrics(lines_of(KA), [], [], None, C8, "G32", 1.0)["nwin"] == {"0": 1, "1": 1, "2": 1})
REAL = os.path.join(HERE, "..", "..", "docs", "l1-raw", "anchor-diag", "diag-ad", "c1d-400-MAIN-s42-r1-a1-c.log")
if os.path.exists(REAL):
    dl = [ln for ln in lines_of(REAL) if ln.startswith("[DIAG] t=0.3s")]
    tk = s._tick(dl[0]) if dl else None
    check("engine format: a real two-leg [DIAG] line -> top rtt 5.1, leg rtt 5 / 5, rtp_us 2392 / 2293, echo counts 3693 / 1141 (the [ETA]-line sig_us is not read), win 320 / 348",
          tk == [0.3, 20352, 5.1, 5, 2392, 3693, 5, 2293, 1141, 320, 348], tk)
else:
    print("SKIP the real two-leg [DIAG] line (no l1-raw archive)")

# ═══ 3. collapse rule at the X boundaries (s60 section 2(c)) ═══
def collapse_flag(cell, n_hi, n_tot=100, excess=None, leg=0, **kw):
    X = s.X_COLLAPSE[cell][leg]
    ex = X + 0.1 if excess is None else excess
    base = 8.0
    # LATE ticks: t >= t0 with src > 0; the first n_hi of the n_tot LATE ticks sit above X, the rest at the floor
    t0 = s.LATE_T[cell]
    idx = {"i": 0}

    def rtt(t):
        if t < t0:
            return base
        idx["i"] += 1
        return base + (ex if idx["i"] <= n_hi else 0.5)
    N = int(round(t0 / 0.25)) - 1 + n_tot
    legs = {0: dict(F2[0])}
    mm = s.row_metrics(gen_cli(cell, "G%d" % plan60.FLOOR_W[cell][0], legs, N=N, rtt_top=rtt, rtp_us={0: 8000}), [], [], None, cell, "G%d" % plan60.FLOOR_W[cell][0], 1.0)
    z = mm["diag"]["collapse"]["0"]
    return z["flag"], z["share"], z["n"]


fl10 = collapse_flag(C2, 10)
fl11 = collapse_flag(C2, 11)
check("collapse: exactly 10 % of LATE ticks above X (c2 14.7 ms) is NOT flagged (strictly more than 10 % flags)", fl10[0] is False and near(fl10[1], 0.10) and fl10[2] == 100, fl10)
check("collapse: 11 % of the LATE ticks above X IS flagged", fl11[0] is True, fl11)
check("collapse: an excess just below X (14.6 ms) never counts, however many ticks", collapse_flag(C2, 100, excess=14.6)[0] is False)
check("collapse: c1s X = 6.1 (top-level 0.1 ms rtt), flagged at 6.2 ms excess on 50 % of ticks, not at 6.0", collapse_flag(C1S, 50, excess=6.2)[0] is True and collapse_flag(C1S, 50, excess=6.0)[0] is False)
check("collapse X table = 6.1 / 14.7 / 17.8 / 24.2 ms", s.X_COLLAPSE == {C1S: {0: 6.1}, C2: {0: 14.7}, C8: {0: 17.8, 1: 24.2}})
two = {0: dict(F2[0]), 1: dict(F2[0])}
mc8 = s.row_metrics(gen_cli(C8, "G32", two, diag_rtt={0: 8 + 5, 1: 8 + 30}, rtp_us={0: 8000, 1: 8000}, N=120), [], [], None, C8, "G32", 1.0)
check("c8 reads the PER-LEG rtt (1 ms): leg 0 excess 5 < 17.8 not flagged, leg 1 excess 30 > 24.2 flagged", mc8["diag"]["collapse"]["0"]["flag"] is False and mc8["diag"]["collapse"]["1"]["flag"] is True, mc8["diag"]["collapse"])
check("collapse records E50 / E90 beside X (reported)", fl10[0] is False and mc8["diag"]["collapse"]["0"]["e50"] == 5 and mc8["diag"]["collapse"]["0"]["e90"] == 5)

# ═══ 4. leg-1 sampler attachment (s60 section 5) ═══
def attach(n_grow1, nk1):
    legs = {0: dict(F2[0]), 1: dict(F2[0], nk=nk1)}
    mm = s.row_metrics(gen_cli(C8, "G96", legs, n_grow={0: 10, 1: n_grow1}, N=40), [], [], None, C8, "G96", 1.0)
    return mm["diag"]["attach"]["status"]


check("attachment: DIAG leg-1 echo count grows while [SLAW] leg 1 nk = 0 on every window -> UNATTACHED", attach(10, 0) == "UNATTACHED", attach(10, 0))
check("attachment: both grow -> ATTACHED", attach(10, 50) == "ATTACHED")
check("attachment: neither grows -> STARVED (not a sampler fault)", attach(0, 0) == "STARVED")

# ═══ 5. statistics ═══
fit = s.ols([16, 32, 48, 16, 32, 48], [9.45, 9.93, 10.41, 9.45, 9.93, 10.41])
check("ols: exact line slope 0.03, se 0", fit and near(fit[0], 0.03) and near(fit[2], 0.0, 1e-9), fit)
ys = [9.45 + 0.03 * (x - 16) + e for x, e in zip([16, 32, 48, 16, 32, 48], [0.1, -0.1, 0.1, -0.1, 0.1, -0.1])]
fit = s.ols([16, 32, 48, 16, 32, 48], ys)
sxx = sum((x - 32) ** 2 for x in [16, 32, 48, 16, 32, 48])
res = sum((y - fit[1] - fit[0] * x) ** 2 for x, y in zip([16, 32, 48, 16, 32, 48], ys))
check("ols: se_b equals the closed form from its own residuals", near(fit[2], math.sqrt(res / 4 / sxx), 1e-12) and sxx == 1024)
p_mono = s.perm_p([1, 2, 3, 4, 5, 6], [1, 2, 3, 4, 5, 6])
check("permutation p of a perfectly monotone n = 6 series = 2/720", near(p_mono, 2 / 720, 1e-9), p_mono)
check("cross_up / interp: knee at 169 from 0.9 @160 and 0.98889 @176", near(s.cross_up([(160, 0.9), (176, 0.9 + 16 * 0.05 / 9)], 0.95)[0], 169.0, 1e-9)
      and near(s.interp([(160, 12.05), (176, 13.65)], 169)[0], 12.95, 1e-9))

# ═══ 6. the c2 session: floors, gate, D, labels ═══
def c2_rows(floor_slope=0.0, floor0=9.45, knee_rise=(12.05, 13.65), sig=2.0, l_c=6.0, reps=6, spread=None, bad_rows=(), drop_arms=(), b0_by_rep=None, p1_b0=12.6, l_arm=None,
            floor_noise=None, floor_nk=100, latch_rep=None):
    """A c2 session: floor pools 16/32/48, knee arms 160/176/192/256, control 2048. The knee sits at 169 (goodput 0.9 @160, 0.98889 @176); Tmin(160)=12.05, Tmin(176)=13.65 -> Tmin(169)=12.95."""
    rows = []
    for rep in range(1, reps + 1):
        sd_ = seed_of(rep)
        for W in (16, 32, 48, 160, 176, 192, 256, 2048):
            if W in drop_arms:
                continue
            if W in (16, 32, 48):
                tm = floor0 + floor_slope * (W - 16) + ((floor_noise[(rep + W) % len(floor_noise)]) if floor_noise else 0.0)
                b0k = list(DEFAULT_B0K)
                if b0_by_rep:
                    b0k[-1] = b0_by_rep(rep, W)
                if latch_rep and rep in latch_rep:
                    b0k[-1] = 9.0
                leg = dict(tmin=tm, tp50=20.0, m=8.0, l=l_c, sig=sig, sigb=1.5, b0k=b0k, nk=floor_nk if W == 16 else 100)
                g = 10.0 + W / 10.0
            else:
                tm = {160: knee_rise[0], 176: knee_rise[1], 192: knee_rise[1] + 1.0, 256: knee_rise[1] + 4.0, 2048: knee_rise[1] + 8.0}[W]
                if spread and W in (160, 176):
                    tm += spread[(rep + W) % len(spread)]
                b0k = list(DEFAULT_B0K)
                if W == 256:
                    b0k[-1] = p1_b0
                leg = dict(tmin=tm, tp50=20.0, m=9.0, l=(l_arm[(rep) % len(l_arm)] if (l_arm and W == 256) else l_c), sig=sig, sigb=1.5, b0k=b0k)
                g = {160: 90.0, 176: 90.0 + 16 * 0.05 / 9 * 100, 192: 100.0, 256: 100.0, 2048: 100.0}[W]
            rows.append(mkrow(C2, "G%d" % W, sd_, rep, {0: leg}, mbps=g, N=80))
    return rows


SH = c2_rows()
rc, out = run_score(SH, "60a")
L = labels(out)
check("score60 60a on the known c2 session returns 0 and prints VERDICT60 + LABEL60 lines", rc == 0 and any(ln.startswith("VERDICT60 ") for ln in out) and L, out[-3:])
check("calibration: floor pool = W16 (100 % of LATE windows nk >= 10), F_c 9.45, l_c 6.0, b0_c 1.0 (b0p = max of the 8 brakes), T_c 7.0", has(out, "CAL c2 leg0: pool-share[W16:1.00") and has(out, "floor pool W=16")
      and has(out, "F_c=9.450 l_c=6.000 b0_c=1.000 T_c=7.000"), [ln for ln in out if "CAL c2" in ln][:4])
dline = [ln for ln in out if ln.startswith("  C2-D s42r1")]
check("c2 D known answer: Tmin(W95)=12.95 (floor 9.45 + 3.5), sig 2.0, q*l 0.6 -> D_r = +0.900 exactly, W95 = 169.0", dline and "W95=169.0" in dline[0] and "Tmin(W95)=12.950" in dline[0] and "D_r=+0.900" in dline[0], dline)
check("c2 D: all 6 replicates agree, bootstrap interval is the point [0.9, 0.9] -> SHORT; the sign witness holds (R~67 - 1 = -D/T = -0.045 -> 0.955)", L.get("C2-SHORT") == "SHORT"
      and has(out, "pooled: D = median over 6 replicates = +0.900 ms, 90 % bootstrap interval [+0.900, +0.900]") and not has(out, "SCORER-BUG"), [ln for ln in out if "C2-D pooled" in ln])
check("c2 D: the bracket is reported (lower arm D 0.0 at 160, upper +1.6 at 176)", has(out, "lower arm median 0.000 (n=6), upper arm median 1.600"))
rc, o_rob = run_score(c2_rows(knee_rise=(14.0, 15.6)), "60a")
check("c2 D: SHORT is ROBUST when the lower bracket arm (160) is also SHORT (D(160) = +1.95)", labels(o_rob)["C2-SHORT"] == "SHORT" and has(o_rob, "SHORT ROBUST (both read SHORT)"), [ln for ln in o_rob if "bracket" in ln])
check("c2 D arms printed: D(160) = 0.000, D(176) = +1.600", has(out, "C2-D arm W160: D median 0.000") and has(out, "C2-D arm W176: D median 1.600"), [ln for ln in out if "C2-D arm" in ln])
check("floor gate (flat floor): slope FLAT, CONVERGED (E = 0)", L.get("FLOOR-SLOPE-c2") == "FLAT" and L.get("FLOOR-CONV-c2") == "CONVERGED", L)
check("crossing W_r is reported per replicate beside Appendix A's bands", has(out, "CROSS c2 s42r1"))

rc, o_sl = run_score(c2_rows(floor_slope=0.02), "60a")
L_sl = labels(o_sl)
check("floor gate sloped 0.020 ms/slot (R6.7's low secant): SLOPED, E = 0.32 > tau 0.3 with se 0 -> NOT-CONVERGED (registration: c2 E = 0.32-0.43)", L_sl["FLOOR-SLOPE-c2"] == "SLOPED" and L_sl["FLOOR-CONV-c2"] == "NOT-CONVERGED"
      and has(o_sl, "E=b*W1=0.32"), [ln for ln in o_sl if "GATE c2" in ln])
rc, o_cv = run_score(c2_rows(knee_rise=(10.85, 12.45)), "60a")
check("c2 COVERED: Tmin(W95) = floor + 2.3 -> D = -0.3, CONVERGED floor -> COVERED", labels(o_cv)["C2-SHORT"] == "COVERED" and has(o_cv, "= -0.300 ms"))
rc, o_cv = run_score(c2_rows(knee_rise=(10.15, 11.75)), "60a")
check("c2 COVERED: D = -1.0 (Tmin(W95) = floor + 1.6), floor CONVERGED -> COVERED", labels(o_cv)["C2-SHORT"] == "COVERED" and has(o_cv, "= -1.000 ms"), [ln for ln in o_cv if "C2-D pooled" in ln])
rc, o_inad = run_score(c2_rows(floor_slope=0.03, knee_rise=(10.15, 11.75)), "60a")
check("c2 COVERED is INADMISSIBLE when the floor is NOT-CONVERGED: the label is UNRESOLVED and the line says so", labels(o_inad)["C2-SHORT"] == "UNRESOLVED" and has(o_inad, "INADMISSIBLE")
      and labels(o_inad)["FLOOR-CONV-c2"] == "NOT-CONVERGED")
rc, o_sh2 = run_score(c2_rows(floor_slope=0.03), "60a")
check("c2 SHORT stands as a LOWER BOUND when the floor is not CONVERGED (printed)", labels(o_sh2)["C2-SHORT"] == "SHORT" and has(o_sh2, "LOWER BOUND"))
rc, o_un = run_score(c2_rows(spread=[-3.0, 3.0, -2.0, 2.5, -3.5, 3.5]), "60a")
check("c2 UNRESOLVED when the replicate scatter straddles 0", labels(o_un)["C2-SHORT"] == "UNRESOLVED", [ln for ln in o_un if "C2-D pooled" in ln])
rc, o_f = run_score(c2_rows(floor0=8.15), "60a")
check("registration prediction: a floor 1.3 ms lower (the secant extended to W 16) raises D from +0.9 to +2.2", has(o_f, "= +2.200 ms"), [ln for ln in o_f if "C2-D pooled" in ln])
rc, o_few = run_score(c2_rows(reps=2), "60a")
check("c2 D with fewer than 3 valid floor rows / 2 replicates -> UNSCOREABLE (no guess)", labels(o_few)["C2-SHORT"] == "UNSCOREABLE" and labels(o_few)["P1-c2"] == "UNSCOREABLE", labels(o_few))

# SCORER BUG line when the sign witness is broken (patched replay returns the wrong side)
_sr = s._lib()[0]
_orig67 = _sr.r67_r
_sr.r67_r = lambda legs, q: 2.0 - _orig67(legs, q)      # a broken R~67: the wrong side of 1
rc, o_bug = run_score(c2_rows(), "60a")
_sr.r67_r = _orig67
check("sign witness: a broken R~67 (wrong sign vs D) prints a SCORER-BUG line", has(o_bug, "SCORER-BUG"))

# ═══ 7. validity: LATCHED boundary, COLLAPSED, floor-pool 80 % rule, minimum rows ═══
def rv_of(legs, cell=C2, arm="G16", **kw):
    r = mkrow(cell, arm, 42, 1, legs, **kw)
    return s.RV(r, 0)


lat_lo = rv_of({0: dict(F2[0], l=8.0, tmin=9.45, m=8.0 + (9.45 - 8.0) * 0 + 0.0, b0k=[0, 0, 0, 0, 0, 0, 0, 3.0])})      # brake = 1.45
thr = s.Q_BULK * 8.0 + s.OMEGA[C2][0]
def latch_gap(b0p, brake):
    return rv_of({0: dict(F2[0], l=8.0, tmin=8.0 + brake, m=8.0, b0k=[0, 0, 0, 0, 0, 0, 0, b0p])}).latched(0)
check("LATCHED boundary (c2: q*l + omega = 0.1*8 + 0.9 = 1.7): b0p - brake = 1.69 not latched, 1.71 latched", latch_gap(3.0, 1.31) is False and latch_gap(3.0, 1.29) is True and near(thr, 1.7), (latch_gap(3.0, 1.31), latch_gap(3.0, 1.29)))
check("LATCHED omega-bar table c2 0.90, c8 1.8 / 6.3, c1s 0.20", s.OMEGA == {C2: {0: 0.9}, C8: {0: 1.8, 1: 6.3}, C1S: {0: 0.2}})
inv = rv_of({0: dict(F2[0], b0k=[0, 0.1, 0.1])})
check("validity (a): a boot with fewer than 8 brakes is incomplete", s.valid_for_cal(inv, 0)[1] == "boot-incomplete", s.valid_for_cal(inv, 0))
low = rv_of({0: dict(F2[0], b0kn=[20, 20, 20, 9, 20, 20, 20, 20])})
check("validity (a): leg-0 b0kn < 10 on any brake -> boot incomplete", s.valid_for_cal(low, 0)[1] == "boot-incomplete")
check("validity: a good row is valid", s.valid_for_cal(rv_of({0: dict(F2[0])}), 0) == (True, ""))
legs1 = {0: dict(F2[0]), 1: dict(F2[0], b0kn=[5, 5, 5, 5, 20, 20, 20, 20])}
check("leg 1 b0 = max over b0k entries with b0kn >= 10, needs >= 4 such entries: 4 entries (k=5..8: 1.0 1.0 0.9 0.7) -> 1.0; with only 3 -> None",
      near(rv_of(legs1, C8, "G32").b0(1), 1.0) and rv_of({0: dict(F2[0]), 1: dict(F2[0], b0kn=[5, 5, 5, 5, 5, 20, 20, 20])}, C8, "G32").b0(1) is None)
col = rv_of({0: dict(F2[0])}, rtt_top=lambda t: 8 + 20 if t >= 15 else 8, rtp_us={0: 8000})
check("validity (c): a COLLAPSED row is not valid", s.valid_for_cal(col, 0)[1] == "COLLAPSED", s.valid_for_cal(col, 0))

def pool_rows(shares):
    """floor-pool rows with a given fraction of LATE windows with nk >= 10 (the rest nk = 5), W16/W32/W48, 3 rows each"""
    rows = []
    for W, sh in shares.items():
        for rep in range(1, 4):
            n_low = int(round((1 - sh) * 21))
            tail = None
            leg = dict(F2[0])
            mkr = mkrow(C2, "G%d" % W, seed_of(rep), rep, {0: leg}, N=80)
            # rebuild with nk=5 on the first n_low LATE windows
            cli_ = gen_cli(C2, "G%d" % W, {0: leg}, N=80)
            out_ = []
            cnt = 0
            for ln in cli_:
                if ln.startswith("[SLAW] t=") and "k=" in ln:
                    k = int(ln.split(" k=")[1].split()[0])
                    if 60 <= k < 60 + n_low:
                        ln = ln.replace("nk=100", "nk=5")
                out_.append(ln)
            mkr["t60"] = s.row_metrics(out_, [], [], None, C2, "G%d" % W, 100.0)
            rows.append(mkr)
    return rows


pr = pool_rows({16: 0.85, 32: 0.95, 48: 1.0})
rvs = [s.RV(r, i) for i, r in enumerate(pr)]
cal = s.calibrate(rvs, C2, 0)
check("floor-pool 80 % rule: W16 at 85 % qualifies -> the floor pool is W16 (smallest)", cal["W"] == 16 and near(cal["pool_share"][16], 0.85, 0.02), cal["pool_share"])
pr = pool_rows({16: 0.50, 32: 0.80, 48: 1.0})
cal = s.calibrate([s.RV(r, i) for i, r in enumerate(pr)], C2, 0)
check("floor-pool 80 % rule: W16 at 50 % fails, W32 at exactly 80 % qualifies -> W32 and the move is reported", cal["W"] == 32 and cal["moved"], cal["pool_share"])
pr = pool_rows({16: 0.50, 32: 0.70, 48: 0.2})
cal = s.calibrate([s.RV(r, i) for i, r in enumerate(pr)], C2, 0)
check("floor-pool 80 % rule: no pool reaches 80 % -> no floor pool, calibration not ok", cal["W"] is None and cal["ok"] is False)
cal_nogo = s.calibrate([s.RV(r, i) for i, r in enumerate(pool_rows({16: 0.9, 32: 0.9, 48: 0.9}))], C2, 0, nogo=("c2",))
check("LD_NOGO=c2: the c2 floor pool moves to W32 (the registered consequence)", cal_nogo["W"] == 32 and cal_nogo["moved"])
rows3 = [r for r in c2_rows(reps=6) if not (r["arm"] == "G16" and r["rep"] > 2)]
cal2 = s.calibrate([s.RV(r, i) for i, r in enumerate(rows3)], C2, 0)
check("minimum: fewer than 3 valid non-latched floor rows -> not ok (UNSCOREABLE), 3 -> ok", cal2["n"] == 2 and cal2["ok"] is False
      and s.calibrate([s.RV(r, i) for i, r in enumerate(c2_rows(reps=3))], C2, 0)["ok"] is True)
rowsL = c2_rows(latch_rep={1, 2, 3, 4})
calL = s.calibrate([s.RV(r, i) for i, r in enumerate(rowsL)], C2, 0)
check("latched rows leave the session medians (4 of 6 latched -> 2 valid -> not ok)", calL["n"] == 2 and calL["ok"] is False and list(calL["reasons"].values()).count("LATCHED") == 4)

# the registered pool moves (s60 section 2 and 5)
def c1s_pool(collapse_w64):
    rows = []
    for W in (64, 128, 192):
        for rep in range(1, 4):
            kw = dict(rtt_top=lambda t: 40.0, rtp_us={0: 5000}) if (W == 64 and collapse_w64) else {}
            rows.append(mkrow(C1S, "G%d" % W, seed_of(rep), rep, {0: dict(F2[0])}, N=80, **kw))
    return rows


cal_m = s.calibrate([s.RV(r, i) for i, r in enumerate(c1s_pool(True))], C1S, 0)
check("a floor pool that collapses on EVERY row moves to the next pool (c1s W64 -> W128), the skip is recorded", cal_m["W"] == 128 and cal_m["moved"] and cal_m["skipped"] == [(64, "collapsed on every row")], (cal_m["W"], cal_m["skipped"]))
check("without the collapse the c1s floor pool stays W64", s.calibrate([s.RV(r, i) for i, r in enumerate(c1s_pool(False))], C1S, 0)["W"] == 64)
rc, o_mv = run_score(c1s_pool(True), "60b")
check("score prints the skipped pool and the move", has(o_mv, "(pool W64 skipped: collapsed on every row)") and has(o_mv, "floor pool W=128"))


def c8_starved(l1_at_256=True):
    rows = []
    for W in (32, 64, 96, 256):
        for rep in range(1, 4):
            nk1 = 0 if W < 256 else (30 if l1_at_256 else 0)
            rows.append(mkrow(C8, "G%d" % W, seed_of(rep), rep, {0: dict(F2[0], b0kn=[20] * 8), 1: dict(F2[0], l=10.0, m=10.0, tmin=12.0, nk=nk1)}, N=80))
    return rows


cal_s = s.calibrate([s.RV(r, i) for i, r in enumerate(c8_starved())], C8, 1)
cal_s0 = s.calibrate([s.RV(r, i) for i, r in enumerate(c8_starved())], C8, 0)
check("c8 leg 1 STARVED at 32 / 64 / 96 takes the smallest arm where it qualifies: W256 (leg 0 stays on W32); the registered latest", cal_s["W"] == 256 and cal_s["starved_ext"] and cal_s["ok"] and cal_s0["W"] == 32, (cal_s["W"], cal_s["pool_share"]))
cal_n = s.calibrate([s.RV(r, i) for i, r in enumerate(c8_starved(False))], C8, 1)
check("c8 leg 1 qualifying nowhere (not even W256): no floor pool, B2 not scorable", cal_n["W"] is None and not cal_n["ok"])
rc, o_st = run_score(c8_starved(), "60a")
check("score prints the leg-1 move to W256 (starved) in its CAL line", has(o_st, "CAL c8 leg1: pool-share[") and has(o_st, "STARVED at the floor pools: leg 1 takes arm W256"))

# ═══ 8. P1 / P3 ═══
rc, o1 = run_score(c2_rows(b0_by_rep=lambda rep, W: 2.87, p1_b0=12.6), "60a")
check("P1 at c2: 12.6 / 2.87 = 4.39 >= 3 -> HOLDS (registration's predicted ~4.4)", labels(o1)["P1-c2"] == "HOLDS" and has(o1, "= 4.39"), [ln for ln in o1 if "P1 c2" in ln])
rc, o1b = run_score(c2_rows(b0_by_rep=lambda rep, W: 2.87, p1_b0=6.0), "60a")
check("P1 at c2: 6.0 / 2.87 = 2.09 < 3 -> FAILS (reported only)", labels(o1b)["P1-c2"] == "FAILS")
rc, o3 = run_score(c2_rows(l_arm=[9.5]), "60a")
check("P3 RISES: arm l 9.5 vs floor 6.0, delta 3.5, SE 0, q*l_c 0.6 -> RISES", labels(o3)["P3-c2"] == "RISES")
rc, o3f = run_score(c2_rows(l_arm=[6.4]), "60a")
check("P3 FLAT: delta 0.4 < q*l_c 0.6 with SE 0 -> FLAT", labels(o3f)["P3-c2"] == "FLAT")
rc, o3u = run_score(c2_rows(l_arm=[6.0, 8.0, 7.0, 9.0, 6.5, 7.5]), "60a")
check("P3 UNRESOLVED: noisy arm l straddles the threshold", labels(o3u)["P3-c2"] == "UNRESOLVED", [ln for ln in o3u if "P3 c2" in ln])
check("P3 witness m = l for t < 10 s is reported", has(o3, "P3 witness c2: m = l for t < 10 s on"))
check("P2 (reported): printed R per arm is printed with a >1 flag", has(o3, "printedR=") and has(o3, "(>1)"))

# ═══ 9. R~ and R~67 beside each other: the engine's hand values ═══
def hand_row(legs_spec, cell):
    """legs_spec: {leg: (w, T, Tmin, m, l, b0, sig)} -> a row whose every window prints these"""
    ld = {}
    for leg, (w, T, tmn, mm, l, b0, sg) in legs_spec.items():
        ld[leg] = dict(tmin=tmn, tp50=T, m=mm, l=l, nk=w, sig=sg, sigb=sg, b0k=[0, 0, 0, 0, 0, 0, 0, b0])
    return mkrow(cell, "G16" if cell == C2 else "G32", 42, 1, ld, N=80)


r1 = s.RV(hand_row({0: (100, 20.0, 18.0, 10.0, 9.0, 2.0, 1.5)}, C2), 0)
cal1 = {0: {"b0": 2.0, "T": 11.0, "l": 9.0, "F": 16.0}}
check("R~ hand value 0.82 (slaw_r one leg: m=10 below l+b0=11, anchor-free) through replay_row with l = T_c - b0_c", near(s.replay_row(r1, C2, cal1, "r")["med"], 0.82, 1e-9), s.replay_row(r1, C2, cal1, "r"))
check("R~67 hand value 1.02 (r67_r one leg: F 16, l_c 9) beside it", near(s.replay_row(r1, C2, cal1, "r67")["med"], 1.02, 1e-9))
r2 = s.RV(hand_row({0: (100, 20.0, 18.0, 10.0, 9.0, 2.0, 1.5), 1: (50, 40.0, 36.0, 30.0, 25.0, 3.0, 2.0)}, C8), 0)
cal2_ = {0: {"b0": 2.0, "T": 11.0, "l": 9.0, "F": 16.0}, 1: {"b0": 3.0, "T": 28.0, "l": 25.0, "F": 30.0}}
check("R~ two legs 1.11625 and R~67 two legs 1.16625 (the engine / s60replay hand values)", near(s.replay_row(r2, C8, cal2_, "r")["med"], 1.11625, 1e-9) and near(s.replay_row(r2, C8, cal2_, "r67")["med"], 1.16625, 1e-9),
      (s.replay_row(r2, C8, cal2_, "r"), s.replay_row(r2, C8, cal2_, "r67")))
cal_own = {0: {"b0": 2.0, "T": 12.0, "l": 9.0, "F": 16.0}}
check("scored recipe l = T_c - b0_c differs from the row's own l when T_c - b0_c != l (T 12 -> l_used 10: anchor-bound at m = 10)", not near(s.replay_row(r1, C2, cal_own, "r")["med"], s.replay_row(r1, C2, cal_own, "own")["med"], 1e-9)
      and near(s.replay_row(r1, C2, cal_own, "own")["med"], 0.82, 1e-9))
check("a window with no live leg or a missing calibration leg gives no R~", s.replay_row(r2, C8, {0: cal2_[0]}, "r") is None)

# ═══ 10. c8: calibration per leg, B2 fire / not fire / anc exclusion ═══
def c8_rows(reps=6, rise=4.0, m_bound=True, unbound_reps=(), jitter=0.0):
    """c8 session: floor pools 32/64/96 (two legs), knee arms 320/384, control 1280. Knee at W 352 from goodput (0.9 @320, 1.0 @384). tmin at the knee arms = F + sig + q l + rise, so R~(W95) ~ 1 - rise/T."""
    rows = []
    for rep in range(1, reps + 1):
        sd_ = seed_of(rep)
        for W in (32, 64, 96, 320, 384, 1280):
            if W in (32, 64, 96):
                legs = {0: dict(tmin=20.0, tp50=40.0, m=18.0, l=16.0, sig=2.0, sigb=1.5, nk=90, b0k=[0, 0.2, 0.5, 0.8, 1.0, 1.0, 0.9, 0.7]),
                        1: dict(tmin=30.0, tp50=60.0, m=28.0, l=24.0, sig=3.0, sigb=2.0, nk=10, b0k=[0, 0.2, 0.5, 0.8, 1.0, 1.0, 0.9, 0.7])}
                g = 20.0 + W / 10
            else:
                # T_c(leg0) = l_c + b0_c = 17.0; T_c(leg1) = 25.0. F_leg = T_c + b0_c (anchor bound) = 18.0 / 26.0
                bound = m_bound and rep not in unbound_reps
                f0, f1 = 18.0 + 2.0 + 0.1 * 16.0, 26.0 + 3.0 + 0.1 * 24.0
                tm0 = f0 + rise + (jitter * ((rep * 7 + W) % 5 - 2) if jitter else 0.0)
                tm1 = f1 + rise
                legs = {0: dict(tmin=tm0, tp50=40.0, m=(20.0 if bound else 12.0), l=16.0, sig=2.0, sigb=1.5, nk=90),
                        1: dict(tmin=tm1, tp50=60.0, m=(30.0 if bound else 15.0), l=24.0, sig=3.0, sigb=2.0, nk=10)}
                g = {320: 90.0, 384: 100.0, 1280: 100.0}[W]
            rows.append(mkrow(C8, "G%d" % W, sd_, rep, legs, mbps=g, N=80))
    return rows


rc, o8 = run_score(c8_rows(rise=4.0, jitter=0.01), "60a")
L8 = labels(o8)
check("c8 calibration per leg: both legs floor pool W32, leg 0 F/l/b0/T = 20/16/1.0/17, leg 1 b0 = 1.0 (>= 4 entries with b0kn >= 10)", has(o8, "CAL c8 leg0: F_c=20.000 l_c=16.000 b0_c=1.000 T_c=17.000")
      and has(o8, "CAL c8 leg1: F_c=30.000 l_c=24.000 b0_c=1.000 T_c=25.000"), [ln for ln in o8 if "CAL c8" in ln])
check("c8 B2 FIRES: R~(W95) well below 1 - 2 sigma_R on 6 anc-bound replicates", L8.get("C8-B2") == "FIRES", [ln for ln in o8 if "C8-B2" in ln][:4])
rc, o8n = run_score(c8_rows(rise=-4.0, jitter=0.01), "60a")
check("c8 B2 NOT-FIRES: R~(W95) above 1 on all 6 replicates", labels(o8n).get("C8-B2") == "NOT-FIRES", [ln for ln in o8n if "C8-B2" in ln][:3])
rc, o8a = run_score(c8_rows(rise=4.0, jitter=0.01, unbound_reps=(1, 2, 3, 4, 5)), "60a")
check("c8 B2 anc exclusion: replay bind fraction < 0.9 on 5 replicates -> they are reported, not counted; only 1 counted -> UNSCOREABLE", labels(o8a).get("C8-B2") == "UNSCOREABLE" and has(o8a, "reported, not counted"),
      [ln for ln in o8a if "C8-B2" in ln][:2])
rc, o8b = run_score(c8_rows(rise=4.0, jitter=0.01, unbound_reps=(1, 2, 3, 4)), "60a")
check("c8 B2 anc exclusion: 4 unbound replicates excluded, the 2 bound ones fire -> UNSCOREABLE (UNDERPOWERED: 2 eligible < 5; the doc's '2 of the 5-6 scoreable')", labels(o8b).get("C8-B2") == "UNSCOREABLE" and has(o8b, "UNDERPOWERED: 2 anc-bound eligible"))
rc, o8b5 = run_score(c8_rows(rise=4.0, jitter=0.01, unbound_reps=(1,)), "60a")
check("c8 B2: 1 unbound replicate excluded, the 5 bound ones fire -> FIRES on 5 of 5 eligible", labels(o8b5).get("C8-B2") == "FIRES" and has(o8b5, "fires on 5 of 5 anc-bound"))
rc, o8c = run_score(c8_rows(rise=4.0, jitter=0.01), "60a", nogo="c8")
check("LD_NOGO=c8: B2 is NOT scored (printed consequence), P1/P3/gate lines still printed", labels(o8c).get("C8-B2") == "NOT-SCORED-NOGO" and has(o8c, "ARM c8 W320"))
rc, o8d = run_score(c8_rows(reps=4, rise=-4.0, jitter=0.01), "60a")
check("c8 B2 NOT-FIRES with fewer than 5 eligible replicates -> UNSCOREABLE", labels(o8d).get("C8-B2") == "UNSCOREABLE", [ln for ln in o8d if "C8-B2" in ln][-1:])
rc, o8e = run_score(c8_rows(reps=4, rise=4.0, jitter=0.01), "60a")
check("c8 B2 with 4 eligible replicates that all fire -> UNSCOREABLE (UNDERPOWERED: a FIRE needs >= 2 fires among >= 5 eligible)", labels(o8e).get("C8-B2") == "UNSCOREABLE" and has(o8e, "UNDERPOWERED: 4 anc-bound eligible"), [ln for ln in o8e if "RESULT C8-B2" in ln])
check("c8 B2 prints how often its W95 / interpolation clamps bound (the clamp gauge)", has(o8, "C8-B2 clamp gauge: W95 clamped at the first goodput arm 0/"))
check("c8 knee: W95 = 352 from goodput 0.9 @320 and 1.0 @384", has(o8, "W95=352.0"), [ln for ln in o8 if "C8-B2 [scored" in ln][:1])
check("c8 own-l R~ is reported beside the scored recipe, with the masking sentence available", has(o8, "C8-B2 [own-l (reported beside, no veto)]"))

# ═══ 11. c1s control sign and the sign-test labels ═══
def c1s_rows(deltas, floor_reps=4, collapsed_idx=(), n_pairs=0, gc=None):
    """c1s: floor pools 64/128/192 (3 rows at 64+), control G2048 rows whose R~ - 1 = deltas[i]. R~ = (F + sig + q*l + T - tmin)/T with F = l + 2 b0 (anchor-bound): tmin chosen from delta."""
    rows = []
    b0 = 1.0
    l = 3.0
    sig = 0.5
    T = 10.0
    for rep in range(1, floor_reps + 1):
        for W in (64, 128, 192):
            rows.append(mkrow(C1S, "G%d" % W, seed_of(rep), rep, {0: dict(tmin=5.0, tp50=T, m=4.0, l=l, sig=sig, sigb=sig, b0k=[0, 0, 0.2, 0.5, 0.6, b0, 0.8, 0.9][:7] + [b0])}, mbps=50.0, N=80))
    for i, d in enumerate(deltas):
        f_ = l + 2 * b0
        tmin = f_ + sig + 0.1 * l - d * T          # R~ - 1 = (F + sig + q l - tmin) / T
        kw = {}
        if i in collapsed_idx:
            kw = dict(rtt_top=lambda t: 5.0 + 30.0, rtp_us={0: 5000})
        rows.append(mkrow(C1S, "G2048", seed_of(i + 1), i + 1, {0: dict(tmin=tmin, tp50=T, m=50.0, l=l, sig=sig, sigb=sig, b0k=[0, 0, 0.2, 0.5, 0.6, b0, 0.8, 0.9][:7] + [b0])}, mbps=100.0, N=80, **kw))
    return rows


def c1s_label(deltas, **kw):
    rc, o = run_score(c1s_rows(deltas, **kw), "60b")
    return labels(o).get("C1S-CONTROL"), o


pin, o_pin = c1s_label([0.05] * 12)
check("c1s control: 12 replicates all R~ - 1 > 0 -> PINNED", pin == "PINNED", [ln for ln in o_pin if "C1S-CONTROL" in ln])
check("c1s control: 12 replicates all R~ - 1 < 0 -> CROSSES", c1s_label([-0.2] * 12)[0] == "CROSSES")
check("c1s control: 6 positive / 6 negative -> SPLIT", c1s_label([0.05, -0.05] * 6)[0] == "SPLIT")
check("c1s control: 5 scoreable replicates -> UNDERPOWERED", c1s_label([0.05] * 5)[0] == "UNDERPOWERED")
lab_c, o_c = c1s_label([0.05] * 12, collapsed_idx=(0, 1, 2, 3, 4, 5, 6))
check("c1s control: COLLAPSED control rows are excluded from the sign test (7 of 12 collapsed -> 5 left -> UNDERPOWERED)", lab_c == "UNDERPOWERED" and has(o_c, "COLLAPSED"), [ln for ln in o_c if "C1S-CONTROL" in ln][:2])
lab9, o9 = c1s_label([0.05] * 8 + [-0.05])
check("c1s sign test boundary: 8 of 9 (k for n=9 is 8) -> PINNED", lab9 == "PINNED")
check("c1s sign test boundary: 7 of 9 -> SPLIT", c1s_label([0.05] * 7 + [-0.05] * 2)[0] == "SPLIT")
check("c1s: R~ value matches the closed form (+0.05 -> prints +0.0500)", has(o_pin, "s42r1=+0.0500"), [ln for ln in o_pin if "C1S-CONTROL: R~" in ln][:1])
rc, o_nog = run_score(c1s_rows([0.05] * 12), "60b", nogo="c1s")
check("LD_NOGO=c1s: the control sign is DROPPED (printed consequence)", labels(o_nog).get("C1S-CONTROL") == "DROPPED-NOGO")
check("c1s floor gate: no tau registered -> FLOOR-CONV-c1s NO-TAU-REGISTERED, slope label still given", labels(o_pin).get("FLOOR-CONV-c1s") == "NO-TAU-REGISTERED" and labels(o_pin).get("FLOOR-SLOPE-c1s") in ("FLAT", "SLOPED", "UNRESOLVED"))

# ═══ 12. the floor gate labels at c2 (UNRESOLVED with noise) ═══
rc, o_noise = run_score(c2_rows(floor_slope=0.0, floor_noise=[-0.5, 0.5, 0.0, 0.4, -0.4, 0.3]), "60a")
check("floor gate with flat floor + row noise ~0.4 ms: slope UNRESOLVED (b + 2 se = 0.015 > 0.010), E = 0 +- 0.12 -> CONVERGED", labels(o_noise)["FLOOR-SLOPE-c2"] == "UNRESOLVED" and labels(o_noise)["FLOOR-CONV-c2"] == "CONVERGED")
rc, o_noise2 = run_score(c2_rows(floor_slope=0.02, floor_noise=[-0.5, 0.5, 0.0, 0.4, -0.4, 0.3]), "60a")
check("floor gate on R6.7's slope 0.020 with noise: E = 0.32 +- 0.24 straddles tau 0.3 -> UNRESOLVED (registration: NOT-CONVERGED or UNRESOLVED)", labels(o_noise2)["FLOOR-CONV-c2"] == "UNRESOLVED", labels(o_noise2))
rc, o_sl2 = run_score(c2_rows(floor_slope=0.027, floor_noise=[-0.2, 0.2, 0.0, 0.1, -0.1, 0.15]), "60a")
check("floor gate at R6.7's high secant 0.027 with small noise: SLOPED", labels(o_sl2)["FLOOR-SLOPE-c2"] == "SLOPED")

# ═══ 13. R-sign gate, drift fallback, per-K, replacement ═══
rc, o_rs = run_score(c2_rows(), "60a")
check("R-sign gate: the c2 control R~ < 1 on replicates 1-2 -> PASS (no STOP)", labels(o_rs).get("R-SIGN-c2") == "PASS" and not has(o_rs, "STOP R-SIGN"), [ln for ln in o_rs if "R-SIGN" in ln][:2])
rows_hi = c2_rows(knee_rise=(10.0, 11.0))
for r in rows_hi:
    if r["arm"] == "G2048":
        r["t60"] = mkrow(C2, "G2048", r["seed"], r["rep"], {0: dict(tmin=9.0, tp50=20.0, m=9.0, l=6.0, sig=2.0, sigb=1.5)}, mbps=100.0)["t60"]
rc, o_stop = run_score(rows_hi, "60a")
check("R-sign gate: control R~ >= 1 on replicate 1 -> a STOP line and label STOP", has(o_stop, "STOP R-SIGN c2") and labels(o_stop).get("R-SIGN-c2") == "STOP", [ln for ln in o_stop if "R-SIGN" in ln][:2])
rc, o_dr = run_score(c2_rows(b0_by_rep=lambda rep, W: 0.4 + 0.2 * rep), "60a")
check("drift check: b0p rising with run order (Spearman +1, p = 0.003, fitted change > q*l_c) -> FALLBACK to per-replicate pairing, printed", has(o_dr, "-> FALLBACK") and has(o_dr, "PER-REPLICATE pairing"), [ln for ln in o_dr if "DRIFT" in ln][:2])
arm_dr = [ln for ln in o_dr if ln.startswith("  ARM c2 W2048")]
check("under the drift fallback R~ is still replayed (per-replicate pairing) and printed for the control arm", arm_dr and "R~=-" not in arm_dr[0] and "R~=None" not in arm_dr[0] and "session-value sensitivity R~=" in arm_dr[0] and "R~=-)" not in arm_dr[0], arm_dr)
rc, o_ng2 = run_score(c2_rows(), "60a", nogo="c2")
check("score with LD_NOGO=c2: the floor pool is W32 (printed), the calibration is read there", has(o_ng2, "floor pool W=32") and has(o_ng2, "NO-GO c2: forced to W32"))
check("drift check: no trend -> no fallback, the check is printed", not has(o_rs, "-> FALLBACK") and has(o_rs, "no drift detected"))
check("per-K readout: K = 1 brake witness (b0k[0] = 0 on every row) and K = 1..8 lines", has(o_rs, "BRAKE1 c2: b0k[0] != 0 on 0/") and sum(1 for ln in o_rs if "PERK c2 K=" in ln) == 8)
check("per-K: K = 8 b0p(K) = 1.0 and T_c(8) = 7.0; K = 3 b0p(3) = 0.5 (max of the first 3 brakes)", has(o_rs, "PERK c2 K=8: valid 6/6 latched 0 b0p(K)=1.000 T_c(K)=7.000") and has(o_rs, "PERK c2 K=3: valid 6/6 latched 0 b0p(K)=0.500"),
      [ln for ln in o_rs if "PERK c2 K=3" in ln or "PERK c2 K=8" in ln])
dup = c2_rows(reps=3)
first = dup[0]
rep_row = dict(first, status="WITNESS-FAIL", problems=["gate-slaw-cli=0 (want 1)"])
rc, o_rp = run_score([rep_row] + dup, "60a")
check("replacement: the LAST row of a slot counts, the earlier VOID one is reported REPLACED and counted in VERDICT60", has(o_rp, "REPLACED c2-100 G16") and any("replaced=1" in ln for ln in o_rp if ln.startswith("VERDICT60")),
      [ln for ln in o_rp if ln.startswith("VERDICT60")])
check("VERDICT60 carries slots/live/void/replaced/dnf/witness_void/labels", all(k in [ln for ln in o_rp if ln.startswith("VERDICT60")][0] for k in ("slots=", "live=", "void=", "replaced=", "dnf=", "witness_void=", "labels=")))
vv = c2_rows(reps=3)
vv[0] = dict(vv[0], status="WITNESS-FAIL", problems=["gate-pool-cli=None (want 16)"])
rc, o_v = run_score(vv, "60a")
check("a VOID row (echo mismatch, not replaced here) is excluded from every reading and listed VOID", has(o_v, "VOID c2-100 G16"))
wrow = c2_rows(reps=3)
wrow[0] = dict(wrow[0], t60=mkrow(C2, "G16", wrow[0]["seed"], 1, {0: dict(F2[0], b0k=DEFAULT_B0K)}, tamper_tmin=0.001)["t60"])
rc, o_w = run_score(wrow, "60a")
check("a row failing the replay witness is WITNESS-VOID for every R reading (listed) and counted in witness_void", has(o_w, "WITNESS-VOID c2-100 G16") and any("witness_void=1" in ln for ln in o_w if ln.startswith("VERDICT60")))
dnf_row = c2_rows(reps=3)
dnf_row[0] = dict(dnf_row[0], dnf=True, mbps=None)
rc, o_dn = run_score(dnf_row, "60a")
check("a floor-pool DNF row is data: counted live, dnf=1 in VERDICT60", any("dnf=1" in ln for ln in o_dn if ln.startswith("VERDICT60")))

# ═══ 14. G/C pairs ═══
def gc_rows(ratios, collapsed_pairs=()):
    rows = []
    for j, rt in enumerate(ratios, start=1):
        sd_ = "42" if j % 2 else "7"
        kw = dict(rtt_top=lambda t: 40.0, rtp_us={0: 5000}) if j in collapsed_pairs else {}
        rows.append(mkrow(C1S, "G2048", sd_, j, {0: dict(F2[0])}, mbps=100.0, cpu=2.0 * rt, N=80, **kw))
        rows.append(mkrow(C1S, "C2048", sd_, j, {0: dict(F2[0])}, mbps=100.0, cpu=2.0, gauge=False, N=80, **kw))
    return rows


rc, ogc = run_score(gc_rows([1.15] * 10), "60b")
check("G/C: median log(G/C) of CPU s/GB = log 1.15 = +14 % > +10 % -> PERTURBING", labels(ogc).get("GC-PAIRS") == "PERTURBING" and has(ogc, "median log(G/C) of client CPU s/GB = +0.1398"), [ln for ln in ogc if "GC" in ln][:3])
rc, ogc2 = run_score(gc_rows([1.05] * 10), "60b")
check("G/C: +5 % -> NOT-PERTURBING", labels(ogc2).get("GC-PAIRS") == "NOT-PERTURBING")
check("C rows (no [SLAW], witness not ok by construction) are NOT reported WITNESS-VOID and do not count in witness_void", not has(ogc, "WITNESS-VOID") and any("witness_void=0" in ln for ln in ogc if ln.startswith("VERDICT60")), [ln for ln in ogc if "WITNESS" in ln][:2])
rc, ogc3 = run_score(gc_rows([1.15] * 10, collapsed_pairs=(1, 2, 3, 4, 5, 6, 7)), "60b")
check("G/C: only 3 unflagged pairs (< 4) -> UNRESOLVED", labels(ogc3).get("GC-PAIRS") == "UNRESOLVED" and has(ogc3, "3 with both rows unflagged"), [ln for ln in ogc3 if "G/C PAIRS" in ln])
rc, ogc4 = run_score(gc_rows([1.15] * 4, collapsed_pairs=()), "60b")
check("G/C: exactly 4 unflagged pairs is resolvable", labels(ogc4).get("GC-PAIRS") == "PERTURBING")
rr = [1.10, 1.30, 1.00, 1.20, 1.05, 1.15]
rc, ogc5 = run_score(gc_rows(rr), "60b")
mlog = s.med([math.log(x) for x in rr])
check("G/C: the median of the pair log-ratios (not the ratio of medians)", has(ogc5, f"= {mlog:+.4f}"), [ln for ln in ogc5 if "median log" in ln])
check("G/C: goodput minimum detectable difference reported (s60size.py t quantiles)", has(ogc5, "minimum detectable difference at n=6"))

# ═══ 15. EVICT rows: m-bar, W1, K4 steady windows ═══
def evict_cli(adv=10, dbt_em=1, n_ticks=60, span_none_at=None, a_change_at=None, fec_n=3):
    L = ['{"seconds":0.01,"warmup":true}']
    for k in range(1, n_ticks + 1):
        t = 0.25 * k
        kk = min(k, 48)
        ack = sum(200 if j % 2 == 0 else 280 for j in range(1, kk + 1))
        src = 2000 if k <= 48 else 0
        L.append(f"[DIAG] t={t:.1f}s win=800/1000 paused=0% good={80.0 + k:.1f}Mbit src={src}sym/s cod={320 if src else 0}sym/s cum={250 * kk}/{40 * kk}/{ack} np=1 np_act=1 rtt=5.0ms bdp100=165sym "
                 f"p0:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0150 cmp=- rtt=5/wrtt=3/rtp2ms rtp_us=1 "
                 f"wait[tun=1% ack=82% cmd=0% n={80 if k % 2 == 0 else 120} us=250000 busy=45% busy_us=112500] wake[tun=1 ack={20 * k} cmd=1]")
        L.append(f"[WSTORE] t={t:.2f} k={k} tw=250.0 n=100 W=50 aw=0.5 awmin=0.5 relb=1/12/12/12 emb=2/16/16/16 adv={adv}/30/30/30 ackw=1 fec=0.020000/4.0000/0.20000/12.00/{fec_n} "
                 f"dbt_acc=1.00 dbt_em={dbt_em} dbt_share=0.5000 dbt_rm_ack=0.00")
        if k % 2 == 0:
            j = k // 2
            a, d = "Some(12)", "Some(0.5)"
            if span_none_at == j:
                a, d = "None", "None"
            if a_change_at is not None and j >= a_change_at:
                a = "Some(13)"
            L.append(f"[SPAN] t={0.5 * j:.1f}s ack={240 * j} win=[{240 * j},{240 * j + 64}] wsize=64 a_star={a} delta={d} owed=0.10 rr={0.1 + 0.001 * j:.3f} debt=0.30 retx_buf=5 | shim")
    return L


def evict_stats_of(cli_, arm="EVICT"):
    t60 = s.row_metrics(cli_, [], [], 600_000_000, C2, arm, 100.0)
    r = {"cell": C2, "arm": arm, "seed": "42", "rep": 1, "status": "LIVE", "dnf": False, "t60": t60, "mbps": 100.0}
    return s.evict_stats(s.RV(r, 0)), t60


es, t60e = evict_stats_of(evict_cli())
check("EVICT m-bar = d(cum ack)/sum(adv count): LATE ticks 5.0..12.0 s -> d(ack) 6720, 28 windows x adv 10 = 280 -> 24.0, and mbar_lo (2.4) <= m-bar witness ok", near(es["mbar"], 24.0) and near(es["mbar_lo"], 2.4) and es["mbar_ok"] is True, es)
es_bad, _ = evict_stats_of(evict_cli(adv=3000))
check("EVICT witness mbar_lo <= m-bar FAILS when the adv counts are huge (m-bar 0.08 < 2.4)", es_bad["mbar_ok"] is False, es_bad["mbar"])
check("EVICT row W1 = sum dbt_em / sum dbt_acc over LATE windows (t >= 5): 1.0, inside plan 59's bracket [0, 1.333]", near(es["w1"], 1.0) and es["w1_ok"] is True and near(es["w1_hi"], 0.16 / 0.12, 1e-9), es)
es_w, _ = evict_stats_of(evict_cli(dbt_em=3))
check("EVICT W1 witness FAILS when W1 (3.0) lies outside the bracket", es_w["w1_ok"] is False)
check("EVICT K4: every [SPAN] line constant -> steady windows = the 40 LATE windows with a successor (k=20..59)", es["n_steady"] == 40 and es["n_late"] == 41 and es["unscoreable"] is False, (es["n_steady"], es["n_late"]))
es_n, _ = evict_stats_of(evict_cli(span_none_at=12))
check("EVICT K4: a [SPAN] None line at t = 6.0 s removes the 3 windows whose neighbourhood contains it (40 - 3 = 37)", es_n["n_steady"] == 37, es_n["n_steady"])
es_c, _ = evict_stats_of(evict_cli(a_change_at=20))
check("EVICT K4: an a_star change at t = 10 s (SPAN j=20) removes exactly the one window (k=39) whose neighbourhood sees 12 and 13: 39 steady; the 20 steady windows after it carry a fec astar token 12 != 13 (witness mismatches)",
      es_c["n_steady"] == 39 and es_c["astar_bad"] == 20, (es_c["n_steady"], es_c["astar_bad"]))
es_u, _ = evict_stats_of(evict_cli(n_ticks=40, fec_n=3))
check("EVICT K4: fewer than 30 steady LATE windows -> K4-UNSCOREABLE", es_u["unscoreable"] is True and es_u["n_steady"] < 30)
es_z, _ = evict_stats_of(evict_cli(fec_n=0))
check("EVICT K4: windows with fec n = 0 are never steady", es_z["n_steady"] == 0)
check("EVICT K4 inputs: means of eps 0.02, sigma2 4, r 0.2, astar 12, Delta 0.5, dbt_share 0.5; astar-token witness mismatches 0", near(es["means"]["eps"], 0.02) and near(es["means"]["sigma2"], 4.0) and near(es["means"]["r"], 0.2)
      and near(es["means"]["astar"], 12.0) and near(es["means"]["delta"], 0.5) and near(es["means"]["dbt_share"], 0.5) and es["astar_bad"] == 0, es["means"])
check("EVICT extras are stored with the mbar59 readouts and stay small", "mbar59" in t60e["evict"] and len(json.dumps(t60e)) < 200_000)
rows_ev = []
for k in range(5):
    t60 = s.row_metrics(evict_cli(), [], [], 600_000_000, C2, "EVICT", 100.0)
    rows_ev.append({"cell": C2, "store": "def", "arm": "EVICT", "seed": plan60.EVICT_SEEDS[k], "rep": k + 1, "status": "LIVE", "dnf": False, "t60": t60, "mbps": 100.0, "problems": []})
rc, oev = run_score(rows_ev, "60b")
check("score60 60b prints the EVICT lines with the K4 inputs and says the oracle comparison is off-VM", has(oev, "EVICT s42r1: m-bar=24.000") and has(oev, "OFF-VM"), [ln for ln in oev if "EVICT" in ln][:2])

# ═══ 16. smoke60 and the rehearsal ═══
def smoke_of(rows, plan):
    p = ledger(rows)
    out_ = []
    rc_ = s.smoke(p, plan, out=out_.append)
    os.remove(p)
    return rc_, out_


G192 = dict(F2[0], b0k=DEFAULT_B0K)
smoke_rows = [mkrow(C2, "C192", 42, 1, {0: G192}, gauge=False), mkrow(C2, "G192", 42, 1, {0: G192})]
rc, osm = smoke_of(smoke_rows, "60a")
check("smoke60 60a PASS: C row has no gauge tags, G row has all, RWM_SLAW echo, q 0.100000, kw/b0k, witness PASS, CAPLOG PASS + bind fraction reported", rc == 0 and osm[-1] == "SMOKE-PASS" and has(osm, "CAPLOG c2 G192:") and has(osm, "bind(ceiling)=1.000"), osm[-3:])
c_tag = mkrow(C2, "C192", 42, 1, {0: G192}, gauge=True)
c_tag["t60"]["gate_slaw"] = {"cli": "0", "srv": "0"}
rc, o_ct = smoke_of([c_tag, smoke_rows[1]], "60a")
check("smoke60 FAIL: a C row carrying gauge tags", rc == 1 and o_ct[-1].startswith("SMOKE-FAIL") and has(o_ct, "FAIL a C row"))
rc, o_mt = smoke_of([smoke_rows[0], mkrow(C2, "G192", 42, 1, {0: G192}, drop=("WSTORE",))], "60a")
check("smoke60 FAIL: a G row missing a gauge tag (WSTORE)", rc == 1 and has(o_mt, "no WSTORE"))
rc, o_q = smoke_of([smoke_rows[0], mkrow(C2, "G192", 42, 1, {0: G192}, qstr="0.050000")], "60a")
check("smoke60 FAIL: [SLAW] q = 0.050000 (Realtime) on a Bulk G row", rc == 1 and has(o_q, "q=['0.050000']"))
rc, o_w2 = smoke_of([smoke_rows[0], mkrow(C2, "G192", 42, 1, {0: G192}, tamper_tmin=0.001)], "60a")
check("smoke60 FAIL: the per-row replay witness fails", rc == 1 and has(o_w2, "FAIL the per-row replay witness"))
rc, o_cp = smoke_of([smoke_rows[0], mkrow(C2, "G192", 42, 1, {0: G192}, caplog_ok=False)], "60a")
check("smoke60 FAIL: the CAPLOG replay fails", rc == 1 and has(o_cp, "FAIL the CAPLOG replay"))
rc, o_gt = smoke_of([smoke_rows[0], dict(smoke_rows[1], t60=dict(smoke_rows[1]["t60"], gate_slaw={"cli": "0", "srv": "1"}))], "60a")
check("smoke60 FAIL: RWM_SLAW echo mismatch", rc == 1 and has(o_gt, "FAIL RWM_SLAW echoed"))
rc, o_nr = smoke_of([smoke_rows[0]], "60a")
check("smoke60 FAIL: a missing row", rc == 1 and has(o_nr, "no row"))
rc, o_kb = smoke_of([smoke_rows[0], mkrow(C2, "G192", 42, 1, {0: dict(G192, b0k=[0.0, 0.1])})], "60a")
check("smoke60 FAIL: b0k with fewer than K=8 entries after the cut", rc == 1 and has(o_kb, "FAIL kw and b0k"))
rc, o_b = smoke_of([mkrow(C1S, "G64", 42, 1, {0: G192}), mkrow(C1S, "C2048", 42, 1, {0: G192}, gauge=False)], "60b")
check("smoke60 60b PASS (c1s W64 G, W2048 C)", rc == 0 and o_b[-1] == "SMOKE-PASS", o_b[-2:])


def rehearsal_rows(collapse_c1s=False, collapse_c8=False, unattached=False, latch_c2=False, dnf_c2=False):
    rows = []
    for (cell, _, arm, sd_, rp) in plan60.plan("60a", rehearsal=True):
        kind = plan60.arm_parse(arm)[0]
        if cell == C8:
            legs = {0: dict(F2[0], b0kn=[20] * 8), 1: dict(F2[0], l=10.0, m=10.0, tmin=12.0, nk=(0 if unattached else 30))}
            kw = dict(n_grow={0: 10, 1: 10}) if True else {}
            if collapse_c8:
                kw.update(diag_rtt={0: 8 + 40, 1: 8}, rtp_us={0: 8000, 1: 8000})
            rows.append(mkrow(C8, arm, sd_, rp, legs, N=80, **kw))
        else:
            leg = dict(F2[0], b0k=([0, 0, 0, 0, 0, 0, 0, 9.0] if (latch_c2 and cell == C2 and arm == "G16") else DEFAULT_B0K))
            kw = {}
            if cell == C1S and collapse_c1s:
                kw = dict(rtt_top=lambda t: 40.0, rtp_us={0: 5000})
            rows.append(mkrow(cell, arm, sd_, rp, {0: leg}, gauge=(kind == "G"), N=80, dnf=(dnf_c2 and arm == "G16"), **kw))
    return rows


rr_ok = rehearsal_rows()
rc, orh = smoke_of(rr_ok, "60r")
check("smoke60 60r: 10 rehearsal rows, SMOKE-PASS, GO for c2 / c8 / c1s", rc == 0 and orh[-1] == "SMOKE-PASS" and all(has(orh, f"GO-NOGO {c} GO") for c in ("c2", "c8", "c1s")), [ln for ln in orh if "GO-NOGO" in ln])
check("60r reports the b0k[0] = 0 witness and the m = l (t < 10 s) witness per leg (REPORTED)", has(orh, "WITNESS c2 G16") and has(orh, "b0k[0]=0.000 (expected 0: ok)") and has(orh, "m = l for t < 10 s on 39/39"))
check("60r reports the entry-8 absolute b0p limits and the R~ sensitivities (REPORTED, not gating)", has(orh, "B0P-LIMIT c1s G64") and has(orh, "(2/T,") and has(orh, "(2*w0/T,") and has(orh, "not gating"))
check("60r c8 attachment read on the W96 row: ATTACHED (both counts grow)", has(orh, "ATTACH c8 G96 s42 r1 leg1: ATTACHED") and has(orh, "the W96 row the registration reads"))
rc, orh_u = smoke_of(rehearsal_rows(unattached=True), "60r")
check("60r c8 leg 1: echo count grows but [SLAW] leg-1 nk = 0 -> UNATTACHED, B2 not scored (a decision line, not a SMOKE-FAIL)", has(orh_u, "ATTACH c8 G96 s42 r1 leg1: UNATTACHED") and has(orh_u, "LEG1-ATTACH c8 G96: UNATTACHED -> c8 has no leg-1 floor")
      and orh_u[-1] == "SMOKE-PASS")
rc, orh_c = smoke_of(rehearsal_rows(collapse_c1s=True), "60r")
check("60r NO-GO c1s (all three floor rows collapsed): printed GO-NOGO NO-GO with the registered consequence, NOT a SMOKE-FAIL", has(orh_c, "GO-NOGO c1s NO-GO") and has(orh_c, "control sign is dropped") and orh_c[-1] == "SMOKE-PASS" and has(orh_c, "GO-NOGO c2 GO"))
rc, orh_c8 = smoke_of(rehearsal_rows(collapse_c8=True), "60r")
check("60r NO-GO c8 (leg 0 collapsed on every row): consequence 'B2 is not scored'", has(orh_c8, "GO-NOGO c8 NO-GO") and has(orh_c8, "c8 B2 is not scored"))
rc, orh_l = smoke_of(rehearsal_rows(latch_c2=True), "60r")
check("60r NO-GO c2 when its only floor row is LATCHED: floor pool moves to W32", has(orh_l, "GO-NOGO c2 NO-GO") and has(orh_l, "moves to W 32"))
rc, orh_d = smoke_of(rehearsal_rows(dnf_c2=True), "60r")
check("60r NO-GO criterion (a): a DNF floor row does not pass; the DNF is not a SMOKE-FAIL", has(orh_d, "GO-NOGO c2 NO-GO") and not any(ln.startswith("FAIL kw") for ln in orh_d))
cs8 = [r for r in rr_ok if r["cell"] == C8]
check("60r holds the registered rows: c2 G16, c8 G32 x2 + G96, c1s G64 x3 (smoke + 2), the two smokes", sorted((r["cell"], r["arm"]) for r in rr_ok).count((C8, "G32")) == 2 and len(rr_ok) == 10)

# ═══ 16b. plan "all" runs every section without error ═══
rc, o_all = run_score(c2_rows() + c8_rows() + c1s_rows([0.05] * 12) + rows_ev, "all")
La = labels(o_all)
check("score60 all: c2 + c8 + c1s + EVICT in one ledger -> every labelled readout family present, rc 0", rc == 0 and all(k in La for k in ("P1-c2", "P1-c8", "P1-c1s", "P3-c2", "P3-c8l0", "P3-c8l1", "FLOOR-SLOPE-c2", "FLOOR-CONV-c2", "FLOOR-SLOPE-c8-leg0",
      "FLOOR-SLOPE-c8-leg1", "FLOOR-CONV-c8-leg1", "FLOOR-SLOPE-c1s", "FLOOR-CONV-c1s", "C2-SHORT", "C8-B2", "C1S-CONTROL", "GC-PAIRS")), sorted(La))
check("score60 all: EVICT rows appear once (5 slots) and slots are counted against 96 + 40", has(o_all, "EVICT s42r1") and any("/136" in ln for ln in o_all if ln.startswith("VERDICT60")), [ln for ln in o_all if ln.startswith("VERDICT60")])

# ═══ 17. CLI ═══
import subprocess
pl = ledger(smoke_rows)
rcli = subprocess.run([sys.executable, "-I", os.path.join(HERE, "s60_score.py"), "smoke60", "60a", pl], capture_output=True, text=True)
check("CLI: python -I s60_score.py smoke60 60a <ledger> -> rc 0 and SMOKE-PASS", rcli.returncode == 0 and "SMOKE-PASS" in rcli.stdout, rcli.stdout[-300:] + rcli.stderr[-300:])
pr_ = ledger(rr_ok)
rcli = subprocess.run([sys.executable, "-I", os.path.join(HERE, "s60_score.py"), "smoke60", "60r", pr_], capture_output=True, text=True)
check("CLI: python -I s60_score.py smoke60 60r <ledger> -> rc 0, GO-NOGO lines", rcli.returncode == 0 and "GO-NOGO c8 GO" in rcli.stdout, rcli.stdout[-300:] + rcli.stderr[-300:])
pc = ledger(SH)
rcli = subprocess.run([sys.executable, "-I", os.path.join(HERE, "s60_score.py"), "score60", "60a", pc], capture_output=True, text=True)
check("CLI: python -I s60_score.py score60 60a <ledger> -> rc 0, VERDICT60 + LABEL60 C2-SHORT SHORT", rcli.returncode == 0 and "VERDICT60 " in rcli.stdout and "LABEL60 C2-SHORT SHORT" in rcli.stdout, rcli.stdout[-300:] + rcli.stderr[-300:])
for pth in (pl, pr_, pc):
    os.remove(pth)
# ── harness60 review round: R-sign at the end of 60a, DNF bookkeeping, the sign witness resolution, the ackw clock join, the evicting rehearsal ──
rows_late = c2_rows()
for r in rows_late:
    if r["arm"] == "G2048" and r["rep"] == 5:
        r["t60"] = mkrow(C2, "G2048", r["seed"], r["rep"], {0: dict(tmin=9.0, tp50=20.0, m=9.0, l=6.0, sig=2.0, sigb=1.5)}, mbps=100.0)["t60"]
rc, o_rl = run_score(rows_late, "60a")
check("R-sign gate is read at the END of the launch on the full calibration: control R~ >= 1 on replicate 5 -> STOP (60b's launch aborts on it)", has(o_rl, "STOP R-SIGN c2 s42 rep5") and labels(o_rl).get("R-SIGN-c2") == "STOP", [ln for ln in o_rl if "R-SIGN" in ln][:3])
# PASS needs >= 2 replayable control rows (Amendment 1); one is UNSCOREABLE, and UNSCOREABLE does not block 60b
rows_one = c2_rows()
ctl = [r for r in rows_one if r["arm"] == "G2048"]
rows_one = [r for r in rows_one if r["arm"] != "G2048" or r is ctl[0]]
rc, o_one = run_score(rows_one, "60a")
check("R-sign gate: one replayable control row -> UNSCOREABLE (PASS needs >= 2), no STOP", labels(o_one).get("R-SIGN-c2") == "UNSCOREABLE" and not has(o_one, "STOP R-SIGN"), [ln for ln in o_one if "R-SIGN" in ln][:3])
# a c8 NO-GO: R-SIGN-c8 is UNSCOREABLE (reported, it is read before any STOP and never blocks 60b); the same rows PASS without it
rc, o_p8 = run_score(c8_rows(), "60a")
rc, o_n8 = run_score(c8_rows(), "60a", nogo="c8")
check("R-sign gate under LD_NOGO=c8: R-SIGN-c8 UNSCOREABLE (NO-GO), the same rows PASS without it", labels(o_p8).get("R-SIGN-c8") == "PASS" and labels(o_n8).get("R-SIGN-c8") == "UNSCOREABLE"
      and has(o_n8, "NO-GO c8"), ([ln for ln in o_p8 if "R-SIGN" in ln][:2], [ln for ln in o_n8 if "R-SIGN" in ln][:2]))
# the launcher's gate regex reads the scorer's own label lines (a scorer-generated score, not a hand-written one)
_runall = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "loopdiag_run_all.sh"), encoding="utf-8").read()
_gate = re.compile(r"^LABEL60 R-SIGN-c2 (PASS|UNSCOREABLE)( |$)")
check("ABORT-RSIGN contract: the launcher's regex is the one tested here, and it admits the scorer's PASS and UNSCOREABLE lines and refuses its STOP line",
      '"^LABEL60 R-SIGN-$_c (PASS|UNSCOREABLE)( |$)"' in _runall
      and any(_gate.match(ln) for ln in o_rs) and any(_gate.match(ln) for ln in o_one) and not any(_gate.match(ln) for ln in o_stop)
      and any(ln.startswith("LABEL60 R-SIGN-c2 STOP") for ln in o_stop))
# a DNF row with too few LATE windows does not calibrate; one with >= 120 does
dn_rows = c2_rows()
fl = [r for r in dn_rows if r["arm"] == "G16" and r["rep"] == 1][0]
short = dict(fl, dnf=True, mbps=None)
short["t60"] = json.loads(json.dumps(fl["t60"]))
for leg in short["t60"]["late"].values():
    for k in leg:
        leg[k] = leg[k][:40]
rv_short = s.RV(short)
check("valid_for_cal: a DNF row with 40 LATE windows (< 120 = 30 s) is excluded ('DNF-too-few-late-windows')", s.valid_for_cal(rv_short, 0) == (False, "DNF-too-few-late-windows"), s.valid_for_cal(rv_short, 0))
long_ = dict(fl, dnf=True, mbps=None)
long_["t60"] = json.loads(json.dumps(fl["t60"]))
for leg in long_["t60"]["late"].values():
    for k in leg:
        leg[k] = (leg[k] * 7)[:130]
rv_long = s.RV(long_)
check("valid_for_cal: a DNF row that still holds >= 120 LATE windows calibrates (it is a datum with its windows)", s.valid_for_cal(rv_long, 0)[1] != "DNF-too-few-late-windows", (len(long_["t60"]["late"]["0"]["k"]), s.valid_for_cal(rv_long, 0)))
rows_rep = c2_rows()
d0 = [r for r in rows_rep if r["arm"] == "G16" and r["rep"] == 2][0]
dd = dict(d0, dnf=True, mbps=None)
rows_rep.insert(rows_rep.index(d0), dd)      # a DNF row first, then its replacement (the same slot)
rc, o_rp2 = run_score(rows_rep, "60a")
check("a replaced DNF row is reported beside its replacement: REPLACED + DNF-DATUM lines, the replacement counts", has(o_rp2, "REPLACED c2-100 G16") and has(o_rp2, "DNF-DATUM c2-100 G16"), [ln for ln in o_rp2 if "DNF-DATUM" in ln][:1])
# the sign witness: |D| below its resolution is UNRESOLVED, not a bug
rows_d0 = c2_rows(knee_rise=(12.05 - 0.0, 13.65), sig=2.0, l_c=6.0)
check("sign witness resolution constant D_WIT_RES = 0.01 ms", s.D_WIT_RES == 0.01)
# the ackw clock join (a known answer): ackw per window == the delta of the cumulative wake ack counter
tt = [0.25 * k for k in range(1, 41)]
ev_ok = {"wstore": {"t": tt, "ackw": [20] * 40}, "cum": {"t": tt, "wack": [20 * k for k in range(1, 41)]}}
n_, e_, f_ = s.ackw_join(ev_ok)
check("ackw join: ackw = 20 per window against wake ack = 20 k -> 39 of 39 windows equal (the first window has no previous tick)", (n_, e_, f_) == (39, 39, None), (n_, e_, f_))
ev_bad = {"wstore": {"t": tt, "ackw": [20] * 10 + [21] + [20] * 29}, "cum": ev_ok["cum"]}
n_, e_, f_ = s.ackw_join(ev_bad)
check("ackw join: one window off by one -> 38 of 39 equal and the first mismatch is named", (n_, e_) == (39, 38) and f_ is not None, (n_, e_, f_))
check("ackw join: no ackw column -> 0 compared (the smoke fails on < 10)", s.ackw_join({"wstore": {"t": tt}, "cum": {"t": tt, "wack": [1] * 40}})[0] == 0)
# 60br: the evicting rehearsal row in the smoke
def evict_smoke_rows(ack_delta=20, rr=0.1, q="0.050000", problems=()):
    rws = [mkrow(C1S, "G64", 42, 1, {0: dict(tmin=2.5, tp50=3.5, m=2.3, l=2.3, sig=0.3, sigb=0.3)}, mbps=50.0), mkrow(C1S, "C2048", 42, 1, {}, mbps=50.0)]
    return rws
check("plan60 'plan 60br' is the 2-row smoke only and neither 60b nor 60br holds an EVICT row (status 60 Amendment 2)", plan60.plan("60br") == plan60.SMOKE["60b"] and len(plan60.plan("60br")) == 2
      and not any(r[2] == plan60.EVICT_ARM for n in ("60b", "60br") for r in plan60.plan(n)))
# 60br with no evicting row: explicit NOT-RUN lines, no PASS over an empty set
rc, o_br = smoke_of([mkrow(C1S, "G64", 42, 1, {0: G192}), mkrow(C1S, "C2048", 42, 1, {0: G192}, gauge=False)], "60br")
check("smoke60 60br without an E row: rc 0, EVICTING ROWS none + NOT-RUN lines, no PASS for the evicting-row criteria",
      rc == 0 and has(o_br, "EVICTING ROWS: none in this session") and has(o_br, "NOT-RUN the evicting-row rehearsal checks") and has(o_br, "NOT-RUN [SLAW] q = 0.05")
      and not any(ln.startswith("PASS") and ("debt loop" in ln or "evicting" in ln or "q = 0.050000" in ln) for ln in o_br), o_br)
rc, o_sc = run_score(c1s_rows([0.05] * 12), "60b")
check("score60 60b with no E row prints 'EVICTING ROWS: none in this session'", has(o_sc, "EVICTING ROWS: none in this session"))
# the kept code: an E row injected into the 60br slots (the future registration's shape); an E row has no CAPLOG and must not fail the G-tag / CAPLOG checks
_plan0 = plan60.plan
plan60.plan = lambda name, *a, **k: _plan0(name, *a, **k) + ([plan60.EVICT_REHEARSAL] if name == "60br" else [])
try:
    e_row = mkrow(C2, "EVICT", 42, 1, {0: G192}, drop=("CAPLOG",))
    rc, o_e = smoke_of([mkrow(C1S, "G64", 42, 1, {0: G192}), mkrow(C1S, "C2048", 42, 1, {0: G192}, gauge=False), e_row], "60br")
finally:
    plan60.plan = _plan0
check("smoke60 60br WITH an E row: the G-tag check passes without CAPLOG on the E row, the CAPLOG replay line covers G rows only, the evicting-row checks run (no NOT-RUN)",
      any(ln.startswith("PASS a G row carries ALL gauge tags") for ln in o_e) and not has(o_e, "CAPLOG c2 EVICT") and not has(o_e, "NOT-RUN the evicting-row") and has(o_e, "GO-NOGO-EVICT"), o_e)
check("the budget guard carries the worst-case floor re-runs: 60a 11 520 + 36 x 190 = 18 360 s, 60b 10 440 + 18 x 190 = 13 860 s", plan60.BUDGET_S == {"60a": 18360, "60b": 13860}, plan60.BUDGET_S)
check("a wrong binary (bin-sha-*) and a missing Realtime echo are classified: the echo VOIDs re-run once, the binary never", plan60.is_echo_void({"problems": ["no-realtime-echo-cli"]}) and not plan60.is_echo_void({"problems": ["bin-sha=abc (want def for CAND)"]})
      and not plan60.is_echo_void({"problems": ["bin-sha-uncheckable (row sha None)"]}))

check("the module docstring carries the DECISIONS list", "DECISIONS" in s.__doc__ and len(s.DECISIONS) >= 15)

print(f"\ntest_s60: {fails} fail(s)")
sys.exit(fails)
