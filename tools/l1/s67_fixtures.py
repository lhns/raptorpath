#!/usr/bin/env python3
"""Plan 67's synthetic log shapes: the ONE source of the lines a stub run (stubrun_gen) and test_s67.py put around a committed real fixture log so the REAL row parser (s67_score.py) sees a plausible row.

    strip_base(plan, arm, cli, srv, drv, defect=None) -> (cli, srv, drv)   remove from the fixture logs what extra_lines replaces (the [DIAG] ticks, summaries, the receiver gauges, the CPU line,
                                                                          the cumulative [SUCC] / [RFA] / [RDIAG] / [C8CONV-R] lines) and rewrite what the arm changes (hint, [GATES] echo, Realtime echo)
    extra_lines(plan, cell, arm, seed, rep, defect=None) -> (cli_lines, srv_lines, drv_lines)   APPENDED to the stripped fixture logs
    tun_lines(defect=None, reps=5, smoke=False) -> [ledger lines]   what tun67_battery.sh writes for stage T

plan: 67a 67r 67p 67o 67d (the battery plans), g1 g2 (stage G: Bulk rows, arm CAND / BASE, cells c1s-400 c1d-400 c2-100 c8-100). Deterministic in (plan, cell, arm, seed, rep).

Seats: B0 prints none of the c8 gauges; S1 prints scw / plx on [DIAG] and rsat / rlaw / pfec on [SPAN] and shed_slot / shed_nack, but no pslot; S2 adds pslot and reads taper = plost = 0. P1 / P1T are S1's family, P2 / P2T S2's.
Defects (the stub-path names of registration 9.8, plus a few the tests need): s2-taper-nonzero s2-no-pslot s2-belowfloor-high s1-no-gauge s1-no-pfec r1-blind twin-no-final twin-under-min twin-worse
twin-no-acct no-succ-times rho-one-end pool-echo-wrong s2-goodput-low s1-goodput-low dnf bulk-rr no-pace-echo,
and (harness 67c) r2-blind (P1 taper = 0, P2 pslot = 0), b0-gauge (B0 prints the gauges), twin-missing-field (P2T's [ACCT] lacks oldest_unchanged_max),
s2-taper-cotenant / s1-crash / s2-crash (the lines of s2-taper-nonzero / a clean row; the STUB makes the driver exit 1, so the row is also a crash void).
Stage G (Bulk) has rr = 0 by design: s2-taper-nonzero / s2-taper-cotenant on a g2 S2 row carry rr > 0 too (the mechanism preconditions of an S06-FAIL hold: pslot > 0, [SPAN] median rr > 0; the row is also a W-ROUTE
witness failure), g2-taper-no-pslot (harness 67c) is the same taper = plost = 40 on an S2 row that prints no pslot= (a seat-witness failure, replaced once): reported as G2-TAPER-NONZERO, the gate fails whatever the replacement shows.
"""
import os
import random
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import plan67  # noqa: E402

INFO = "\x1b[2m2026-11-20T10:00:00Z\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2mraptorpath::net\x1b[0m\x1b[2m:\x1b[0m "
REALTIME_ECHO = "RWM_UNIFIED: Realtime rides the RLC span machine (small-δ parameterization, no code-family switch)"
G_SEAT = {"g1": {"CAND": "S1", "BASE": "B0"}, "g2": {"CAND": "S2", "BASE": "S1"}}
GAUGE_SEATS = ("S1", "S2", "P1", "P2", "P1T", "P2T")
S2_SEATS = ("S2", "P2", "P2T")
JIT = "s67-a"                                    # the jitter salt (tuned so the clean rows of every arm pair read PASS in test_s67)
RUN_BYTES = {"c1s-400": 5000000000, "c1d-400": 5000000000, "c2-100": 600000000, "c8-100": 600000000}
BASE_MBPS = {"c1s-400": 600.0, "c1d-400": 380.0, "c2-100": 85.0, "c8-100": 97.0}
ABOVE = {"c2-100": dict(taper=0.12, bf=4200, bf_s2=900, fill=1500, hb=854), "c8-100": dict(taper=0.20, bf=9000, bf_s2=600, fill=800, hb=122)}
DEFECTS = {"s2-taper-nonzero", "s2-no-pslot", "s2-belowfloor-high", "s1-no-gauge", "s1-no-pfec", "r1-blind", "twin-no-final", "twin-under-min", "twin-worse", "twin-no-acct", "no-succ-times",
           "rho-one-end", "pool-echo-wrong", "s2-goodput-low", "s1-goodput-low", "dnf", "bulk-rr", "no-pace-echo", "r2-blind", "b0-gauge", "twin-missing-field", "s2-taper-cotenant", "s1-crash", "g2-taper-no-pslot", "s2-crash"}


def spec(plan, cell, arm):
    """The row's identity: seat, hint, bytes, legs."""
    if plan in G_SEAT:
        seat, hint = G_SEAT[plan][arm], "bulk"
    else:
        seat = plan67.seat_of(arm)
        hint = "realtime" if plan in ("67r", "67p") else "auto"
    nbytes = {"67r": 100000000, "67p": 262500000}.get(plan) or RUN_BYTES[cell]
    return {"seat": seat, "hint": hint, "bytes": nbytes, "nleg": 2 if cell in ("c8-100", "c1d-400") else 1}


def _rng(plan, cell, arm, seed, rep, salt=""):
    return random.Random(f"{JIT}|{plan}|{cell}|{arm}|{seed}|{rep}|{salt}")


def _group(seed, rep):
    return (0 if str(seed) == "42" else 1) + 2 * (int(rep) - 1)


def strip_base(plan, arm, cli, srv, drv, defect=None):
    sp = spec(plan, "c2-100", arm) if plan not in G_SEAT else {"hint": "bulk"}
    hint = sp["hint"]
    drop = ("[SUCC]", "[RFA]", "[RDIAG]", "[C8CONV-R]", "[RANK]")

    def rt(lines):
        out = []
        for ln in lines:
            if hint == "realtime" and "reliable window mode (retain until acked)" in ln:
                out.append(INFO + REALTIME_ECHO)
                continue
            if "[PIPE]" in ln:
                ln = ln.replace("hint=bulk", "hint=" + hint)
            out.append(ln)
        return out
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and '"summary"' not in ln and '"run":' not in ln and not ln.startswith(drop)]
    srv = [ln for ln in srv if not ln.startswith(drop)]
    drv = [ln for ln in drv if '"summary"' not in ln and "CPU: CPUSRV=" not in ln]
    drv = [re.sub(r"hint=bulk( |$)", "hint=" + hint + r"\1", ln) if "--- RWM-C perf" in ln else ln for ln in drv]
    pool = None if plan != "67o" else plan67.pool_of(arm)
    if defect == "pool-echo-wrong":
        pool = 2048
    rate = plan67.PACE_MBIT if plan == "67p" else None

    def gates(lines):
        out = []
        for ln in lines:
            if "[GATES]" in ln:
                if pool is not None:
                    ln = re.sub(r"\sRWM_STORE_PATH_POOL=\S+", "", ln) + f" RWM_STORE_PATH_POOL={pool}"
                if rate is not None:
                    ln = re.sub(r"\sRWM_PERF_RATE_MBIT=\S+", "", ln) + f" RWM_PERF_RATE_MBIT={rate}"
                if plan == "67d":
                    ln = re.sub(r"\sRWM_SUCC_DUMP=\S+", "", ln) + " RWM_SUCC_DUMP=1"
            out.append(ln)
        return out
    return gates(rt(cli)), gates(rt(srv)), gates(drv)


def _gauge_leg(i, sent, gauge):
    return (f" p{i}:infl=199/sinfl=0/bdp356(cap0) sout=409 khr=1.00/kraw=- btlbw=148831 sr=0/g0d0 est=Y pl=0.0000 cmp=- rtt=20/wrtt=3/rtp2ms rtp_us=2392 sig_us=38/n1 rvar_us=34/n1 "
            f"qsp_us=1/n1 msd_us=4/n1 tlag_us=9/n1 gapd=0/0 qcwnd=250000 qce=0 qlp=1/{sent} | ANCHOR sent=0 al=0 attr=0 nr=0 rej[iv=0 zr=0 al=0] gen=0 fill=1 plu=0.0006 plc=0.0013"
            + (" scw=3 plx=1.250000e-5" if gauge else ""))


def extra_lines(plan, cell, arm, seed, rep, defect=None):
    assert defect is None or defect in DEFECTS, defect
    sp = spec(plan, cell, arm)
    seat, hint, nbytes, nleg = sp["seat"], sp["hint"], sp["bytes"], sp["nleg"]
    rng = _rng(plan, cell, arm, seed, rep)
    jit = lambda s=0.012: 1.0 + s * (rng.random() * 2 - 1)      # noqa: E731
    bulk = plan in G_SEAT
    paced = plan == "67p"
    gauge = (seat in GAUGE_SEATS and not (defect == "s1-no-gauge" and seat in ("S1", "P1", "P1T"))) or (defect == "b0-gauge" and seat == "B0")
    pfec = gauge and defect != "s1-no-pfec"
    pslot_on = seat in S2_SEATS and defect not in ("s2-no-pslot", "g2-taper-no-pslot")
    ab = ABOVE.get(cell, ABOVE["c2-100"])
    mbps = BASE_MBPS.get(cell, 85.0) * jit()
    if hint == "realtime" and not paced:
        mbps *= 0.8
    if paced:
        mbps = 35.0
    if defect == "s2-goodput-low" and seat == "S2":
        mbps *= 0.85
    if defect == "s1-goodput-low" and seat == "S1":
        mbps *= 0.85
    secs = nbytes * 8 / (mbps * 1e6)
    dnf = defect == "dnf"
    pay = 494 if hint == "realtime" else 1182
    total = int(nbytes / pay)
    on_s2 = seat in S2_SEATS
    # the copy / coded repair counters (cumulative totals at the end of the row)
    copies_on = (not bulk) and not on_s2
    tf = ab["taper"] if (copies_on and not (defect == "r1-blind" and seat == "S1") and not (defect == "r2-blind" and seat == "P1")) else 0.0
    if plan == "67r" and not on_s2 and defect != "r1-blind":
        tf = 0.10
    taper_tot = int(total * tf * jit(0.05))
    plost_tot = taper_tot
    if on_s2 and defect in ("s2-taper-nonzero", "s2-taper-cotenant", "g2-taper-no-pslot"):
        taper_tot = plost_tot = 40
    pslot_tot = int(total * (ab["taper"] if plan != "67r" else 0.10) * jit(0.05)) if (pslot_on and not (defect == "r2-blind" and seat == "P2")) else 0
    cod_tot = int(total * 0.0056 * jit(0.05))
    if on_s2 and not bulk:
        cod_tot += pslot_tot
    retx_tot = int(total * 0.029 * jit(0.03))
    bulk_rr = defect == "bulk-rr"      # stage G: rr > 0 on a Bulk row (W-ROUTE)
    rr_val = 0.0 if (bulk and not bulk_rr) else (0.2 if not on_s2 else 0.22) * jit(0.05)
    if bulk and bulk_rr:
        rr_val = 0.1
    if seat == "B0" and not bulk:
        rr_val = 0.2 * jit(0.05)
    lag = 0.12 * jit(0.15)
    dt, t_src_end = 1.0, 0.9 * secs
    if dnf:
        secs = {"67a": 200, "67o": 200, "67d": 200, "67r": 150, "67p": 75}.get(plan, 200)
        t_src_end = 0.9 * secs
    n_ticks = int(secs / dt) + 2
    cli, srv, drv = [], [], []
    shed_tot = 0
    for k in range(1, n_ticks + 1):
        t = round(dt * k, 3)
        f = min(1.0, t / t_src_end)
        fa = min(1.0, max(0.0, (t - lag) / t_src_end))
        s_c, a_c = int(total * f), int(total * fa)
        c_c, tp, ps, rx = int(cod_tot * f), int(taper_tot * f), int(pslot_tot * f), int(retx_tot * f)
        rate = int(total / t_src_end) if t < t_src_end else 0
        shed_tot = int(120 * f) if not on_s2 else int(130 * f)
        legs = "".join(_gauge_leg(i, 1000 + 100 * k, gauge) for i in range(nleg))
        mpr = (f" mpr[rep=0 seqs=0 fired={rx} y=0 r=0 fast=0 coal=0 supp=0/0/0 stale=0 plost={int(plost_tot * f)} taper={tp}" + (f" pslot={ps}" if pslot_on else "") + " age=1ms]")
        shed = f" shed={shed_tot}/{int(shed_tot * 0.1)} bud=0.0500 D=40ms" + (f" shed_slot={0 if on_s2 or plan == '67p' and seat == 'P2' else int(100 * f)} shed_nack={int(20 * f)}" if gauge else "")
        cli.append(f"[DIAG] t={t:.1f}s win=838/838 paused=0% good={mbps:.1f}Mbit cwnd=647 infl=457 np={nleg} np_act={nleg} rtt=15.8ms bdp100=165sym" + legs
                   + f" src={rate}sym/s cod={int(rate * 0.0056)}sym/s cum={s_c}/{c_c}/{a_c} retx={rx}{mpr}{shed}")
        if k % 1 == 0 and not (k % 2):
            rr = rr_val if t < t_src_end else 0.0
            sp_ = (f"[SPAN] t={t:.1f}s ack={a_c} win=[{a_c},{a_c + 64}] wsize=200 a_star=Some(12) delta=Some(0.5) owed=0.10 rr={rr:.3f} debt=0.30 retx_buf=5")
            if gauge:
                win = ("core", "burst", "mass")[(k // 2) % 3] if (k // 2) % 5 == 0 else "core"
                bound = "sat" if (k // 2) % 4 else "cap"
                sp_ += f" rsat={0.25 + 0.01 * ((k // 2) % 7):.3f} rlaw=a:{win}/{bound},l:core/free,b=0.000" + (f" pfec={1e-4 * (1 + (k // 2) % 5):.4f}" if pfec else "")
            cli.append(sp_)
    # the summary lines (the driver's run line and summary)
    if dnf:
        run = '{"bytes":%d,"dnf":true,"hint":"%s","proto":"rp-native","run":1}' % (nbytes, hint)
        summ = '{"bytes":%d,"dnf":1,"hint":"%s","proto":"rp-native","runs":1,"summary":true}' % (nbytes, hint)
    else:
        run = '{"bytes":%d,"hint":"%s","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%.3f}' % (nbytes, hint, mbps, secs)
        summ = ('{"bytes":%d,"dnf":0,"hint":"%s","max_s":%.3f,"mean_mbps":%.3f,"mean_s":%.3f,"median_s":%.3f,"min_s":%.3f,"proto":"rp-native","runs":1,"stdev_s":0.0,"summary":true}'
                % (nbytes, hint, secs, mbps, secs, secs, secs))
    cli += [run, summ]
    drv += [summ]
    cpu_c = secs * 0.80 * jit(0.02)
    drv.append(f"    CPU: CPUSRV={secs * 1.1 * jit(0.02):.2f}s CPUCLI={cpu_c:.2f}s (srv=decoder cli=sender; whole-invocation incl warmup)")
    if nleg == 2:
        drv += [f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams=500 egress_skbs=10 gso=1.00 netem_sent_dgrams=500 netem_dropped_skbs=0 backlog=0 lost=5 loss=0.005000 rcvbuf_drops=0 rcvbuf_scope=netns" for i in range(2)]
    # the receiver (server): [RFA] [RDIAG] [C8CONV-R] [SUCC]
    bf = ab["bf_s2"] if on_s2 else ab["bf"]
    fill = ab["fill"]
    if on_s2 and defect == "s2-belowfloor-high":
        bf = {"c2-100": 3500, "c8-100": 8000}.get(cell, 3500)
    if bulk:
        bf, fill = 40, 1
    bf = int(bf * jit(0.04))
    srv += [f"[RDIAG] busy=50% msgs=60000/s q_avg=0 q_max=1 cap=128 below_floor={bf // 2}", f"[RDIAG] busy=60% msgs=60000/s q_avg=0 q_max=1 cap=128 below_floor={bf}"]
    srv += [f"[RFA] gen=0 fires=200 false=4 false_frac=0.0195 fill_coded={fill // 2} fill_src=200 dup_src=3 preempt_src=1 src_n={total // 2} rep_n=26 nu_recv=0.00146 fa_class=0.0625 rep_redundant=14 late_after_aban=0",
            f"[RFA] gen=0 fires=400 false=8 false_frac=0.0195 fill_coded={fill} fill_src=400 dup_src={int(40 * jit(0.1))} preempt_src=1 src_n={total} rep_n=50 nu_recv=0.00146 fa_class=0.0625 rep_redundant=14 late_after_aban=0"]
    srv += ["[C8CONV-R] p0:fst=%d dup=%d lead=0 unb=1/5ms" % (total // 2, int(30 * jit(0.1))) + (" p1:fst=%d dup=%d lead=0 unb=1/5ms" % (total // 3, int(20 * jit(0.1))) if nleg == 2 else "")]
    rep50, rep90 = int(9000 * jit(0.1) * (0.8 if on_s2 else 1.0)), int(20000 * jit(0.1))
    if defect == "no-succ-times":
        succ = ("[SUCC] gen=0 det=0 res=0 orig_n=0 orig_p50_us=- orig_p90_us=- orig_p99_us=- orig_mx_us=- orig_mean_us=- rep_n=0 rep_p50_us=- rep_p90_us=- rep_p99_us=- rep_mx_us=- rep_mean_us=- "
                "aban_n=0 open=0 over=0 orig_frac=- cross_us=- dump=0/0 sp_n=0 xp_n=0 xp_frac=- rtx_n=0")
    else:
        succ = (f"[SUCC] gen=0 det=300 res=300 orig_n=200 orig_p50_us={int(22000 * jit(0.1))} orig_p90_us={int(45000 * jit(0.1))} orig_p99_us=53248 orig_mx_us=53267 orig_mean_us=26386 "
                f"rep_n=100 rep_p50_us={rep50} rep_p90_us={rep90} rep_p99_us=9216 rep_mx_us=9887 rep_mean_us=9887 aban_n=0 open=0 over=0 orig_frac=0.66 cross_us=- dump={1 if plan == '67d' else 0}/0 "
                "sp_n=131 xp_n=0 xp_frac=0.0000 rtx_n=130")
    srv += [succ, succ + " final=1"]
    if plan == "67d":
        srv += ["[SUCCDUMP] n=3 d=o,22000;r,9000;x,30000", "[SUCCDUMP] n=2 d=o,23000;r,9500"]
    # the twins' [ACCT] / [ACCT-RX] (test-seams builds)
    if seat in ("P1T", "P2T"):
        cl, sv = _acct(plan, arm, seed, rep, seat, defect, nbytes, total)
        cli += cl
        srv += sv
    # the pacer echo and the per-object pace line
    if paced and defect != "no-pace-echo":
        cli = [f"perf offered-load pacing ACTIVE (RWM_PERF_RATE_MBIT={plan67.PACE_MBIT})"] + cli + ['{"pace":true,"obj":1,"rate_mbit":%s,"chunks":%d,"feed_s":60.0,"achieved_mbit":%s,"late":0,"late_max_ms":0.4}' % (plan67.PACE_MBIT, nbytes // 494, plan67.PACE_MBIT)]
    # R1: one end with rho < 1
    if defect == "rho-one-end":
        srv.append(INFO + 'perf configuration peer_config=PeerConfig { is_server: true, window_reliable: false, enable_pi_feedback: true }')
    return cli, srv, drv


# U / oldest_unchanged_max of the twins: the same six values on both seats in a different order (equal rank sums), so a clean pair never reads R2-HOLD
_U1, _U2 = (2, 5, 3, 6, 4, 1), (1, 4, 6, 3, 5, 2)
_O1, _O2 = (30, 50, 40, 20, 60, 10), (20, 60, 10, 40, 30, 50)


def _acct(plan, arm, seed, rep, seat, defect, nbytes, total):
    g = _group(seed, rep) % 6
    p2 = seat == "P2T"
    u = (_U2 if p2 else _U1)[g]
    o = (_O2 if p2 else _O1)[g] * 100
    if p2 and defect == "twin-worse":
        u, o = u + 30, o + 5000
    no_acct = defect in ("twin-no-acct", "twin-under-min") and p2
    cli = []
    if not no_acct:
        a = total - 40 - u
        h, e, o_strand = 20, 20 - 0, 0
        s_sent = a + h + e + u + o_strand
        base = (f"[ACCT] reliable=0 S_sent={s_sent} A={a} H={h} E={e} O_serv={u} O_strand={o_strand} O_max={u + 4} closes=0 inert_sweeps={g} sweeps_nosend=3 sweeps=50 oldest_unchanged={o // 2} "
                f"oldest_unchanged_max={o} O_hwm={u + 4} cap_at_hwm=0 store_cap=2048 bud=0.0500 shed_slot={0 if p2 else 90} shed_nack={110 if p2 else 20} shed_visit_slot=0 shed_visit_nack=0 clamp_evals=10 clamp_excl=0")
        if defect == "twin-missing-field" and p2:
            base = base.replace(f" oldest_unchanged_max={o}", "")
        cli = [base.replace(f"S_sent={s_sent}", f"S_sent={s_sent - 5}").replace(f"A={a} ", f"A={a - 5} "), base + (" final=1" if defect != "twin-no-final" else "")]
        if defect == "twin-no-final":
            cli = [cli[0], base]
    srv = [] if no_acct else ["[ACCT-RX] frontier=%d holes_given_up=%d final=1" % (total - 40, g % 3)]
    return cli, srv


# ── stage T ──
TUN_KEYS = ("window_out_of_order", "window_coded_only", "window_generation_coding", "window_systematic_repair")
TUN_TEXT = "{key} is a perf/native-object mode; the tunnel always delivers in order: remove it from the config file"


def tun_lines(defect=None, reps=5, smoke=False):
    """The ledger tun67_battery.sh writes. defects: t-refusal-missing (an S1 refusal line absent), t-base-refuses (B0's run refuses with the guard text), b0-run-key-false (B0's run reads cfg=false for one key),
    s1-tun-nodata (an S1 transfer carries no data where B0's does), s1-cfg-true (S1's TUNCFG reads ooo:true), tun-no-cfg (every TUNCFG field `-`: the log level is too low),
    t-bringup-fail / t-bringup-fail-s1 (tun_bulk.sh's TUN-BRINGUP-FAIL for B0 / S1 at realtime), t-refusal-and-void (t-refusal-missing and b0-run-key-false together),
    t-refusal-and-bringup (t-refusal-missing and t-bringup-fail together)."""
    parts = set((defect or "").split("+")) | ({"t-refusal-missing", "b0-run-key-false"} if defect == "t-refusal-and-void" else set()) | ({"t-refusal-missing", "t-bringup-fail"} if defect == "t-refusal-and-bringup" else set())
    has = lambda d: d in parts or defect == d      # noqa: E731
    out = []
    for seat in ("B0", "S1"):
        for hint in ("auto",) if smoke else ("auto", "realtime"):
            if hint == "realtime" and ((has("t-bringup-fail") and seat == "B0") or (has("t-bringup-fail-s1") and seat == "S1")):
                out += [f"T67SEAT seat={seat} hint={hint}", f"TUNARM label={seat}-{hint} cell=c2-100 hint={hint} bytes=100000000 reps={reps} seed=42 bin={seat}", "TUNSHA " + "a" * 64,
                        f"TUN-BRINGUP-RETRY label={seat}-{hint} attempt=1", f"TUN-BRINGUP-RETRY label={seat}-{hint} attempt=2", f"TUN-BRINGUP-RETRY label={seat}-{hint} attempt=3",
                        f"TUN-BRINGUP-FAIL label={seat}-{hint}", f"TUNARM-DONE label={seat}-{hint} reps=0 2026-11-21T10:00:00Z", f"T67END seat={seat} hint={hint} rc=0 10:00:00"]
                continue
            out += [f"T67SEAT seat={seat} hint={hint}", f"TUNARM label={seat}-{hint} cell=c2-100 hint={hint} bytes=100000000 reps={reps} seed=42 bin={seat}", "TUNSHA " + "a" * 64, "TUNMTU cli=1196 srv=1196",
                    f"TUNPIPE cli=window/Rlc/{hint} srv=window/Rlc/{hint}", "TUNCAD cli=ACTIVE srv=ACTIVE"]
            v = "-" if defect == "tun-no-cfg" else "false"
            if defect == "s1-cfg-true" and seat == "S1" and hint == "auto":
                out.append("TUNCFG cli=ooo:true,coded:false,gen:false,sysrep:false srv=ooo:false,coded:false,gen:false,sysrep:false")
            else:
                out.append(f"TUNCFG cli=ooo:{v},coded:{v},gen:{v},sysrep:{v} srv=ooo:{v},coded:{v},gen:{v},sysrep:{v}")
            for r in range(1, reps + 1):
                if defect == "s1-tun-nodata" and seat == "S1" and hint == "auto" and r == 2:
                    out.append(f'TUNREP label={seat}-{hint} rep={r} rc=1 {{"nodata": true}}')
                else:
                    out.append(f'TUNREP label={seat}-{hint} rep={r} rc=0 {{"mbps": {80 + r + (2 if seat == "S1" else 0):.1f}, "seconds": {10.0 - 0.1 * r:.2f}}}')
            out.append("TUNARM-DONE")
    guard = TUN_TEXT
    for seat in ("S1", "B0"):
        for cmd in ("check", "run"):
            for key in TUN_KEYS:
                if seat == "S1":
                    rc, cfg, err = 1, "-", "Error: " + guard.format(key=key)
                    if has("t-refusal-missing") and cmd == "run" and key == TUN_KEYS[2]:
                        continue
                else:
                    rc, cfg, err = (0, "true", "") if cmd == "check" else (124, "true", "")
                    if cmd == "run" and (has("b0-run-key-false") or defect == "t-refusal-and-void") and key == TUN_KEYS[1]:
                        cfg = "false"
                    if cmd == "run" and has("t-base-refuses") and key == TUN_KEYS[0]:
                        rc, cfg, err = 1, "-", "Error: " + guard.format(key=key)
                    if defect == "tun-no-cfg":
                        cfg = "-"
                out.append(f"T67REF seat={seat} cmd={cmd} key={key} rc={rc} cfg={cfg} err={err}")
    out.append("T67DONE")
    return out
