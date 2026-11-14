#!/usr/bin/env python3
"""The stub measurement driver's data (test_stubrun.py): what a perf_rwm_c.sh run would leave in /tmp, built from the repo's committed real
logs plus synthetic [DIAG] / [QCC] / event lines, so the REAL row parsers accept the rows of every candidate plan (41c, 46, 47, 48).

    stubrun_gen.py <scenA> <scenB> <hint> <bytes> <n> <single|dual>      (the perf_rwm_c.sh argv)

Reads RWM_BIN (BASE iff it is the bin-base binary), RWM_QUIC_CC (bbr_rs = the plan-48 CAND), RWM_EVENT / RWM_EVENT_QDISC (plan 46),
RWM_PLACE_WDIV / RWM_PLACE_WDIV_DERIVED (plan 52: echoed in both [GATES] lines as the engine does) and LD_PLAN from the environment; prints the driver output (the battery redirects it to /tmp/ld-drv.out) and writes /tmp/rwm-c.log, /tmp/rwm-s.log, and for plan 46 /tmp/rwm-evt.txt (+ /tmp/rwm-evtq.txt).
The shapes are copies of the generators in test_bbr48_parse.py (run_lines) and test_harness46.py (gen, logs_for, qd): those files run their
checks on import, so they cannot be imported."""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import l1common as lc  # noqa: E402

RAW = os.environ.get("STUB_RAW") or os.path.join(HERE, "..", "..", "docs", "l1-raw")
TMP = os.environ.get("STUB_TMP") or "/tmp"          # the bash /tmp (cygpath -m on Windows)
PD = os.path.join(RAW, "thread-pd", "battery")
FIX = {"single": "diag-smoke/c1s-400-MAIN-s42-r1-a1-", "dual": "diag-tpd/c1d-400-MAIN-s42-r1-a1-"}
STOCK_ECHO, BRS_ECHO = "quinn congestion controller: BBR", "burst-robust BBR"      # = bbr48_score.STOCK_ECHO / BRS_ECHO (test_stubrun asserts it); not imported: that import chain costs ~1 s a row
BDP = {"c1s": [250000], "c1d": [250000, 250000], "c2": [125000], "c8": [125000, 100000]}
STOCK = "\x1b[2m2026-11-12T10:00:00Z\x1b[0m INFO quinn congestion controller: BBR (shipped default; RWM_QUIC_CC overrides)"
BRS = "\x1b[2m2026-11-12T10:00:00Z\x1b[0m INFO RWM_QUIC_CC=bbr_rs: burst-robust BBR (in-tree controller, interval-guarded per-flight rate sampler)"


# ── plan 41c / 48: test_bbr48_parse.run_lines ──
def leg48(i, sent, qcwnd, rtt=20, wrtt=3):
    return (f" p{i}:infl=199/sinfl=0/bdp356(cap0) sout=409 khr=1.00/kraw=- btlbw=148831 sr=0/g0d0 est=Y pl=0.0000 cmp=- rtt={rtt}/wrtt={wrtt}/rtp2ms rtp_us=2392 "
            f"sig_us=38/n1 rvar_us=34/n1 qsp_us=1/n1 msd_us=4/n1 tlag_us=9/n1 gapd=0/0 qcwnd={qcwnd} qce=0 qlp=1/{sent} | ANCHOR sent=0 al=0 attr=0 nr=0 "
            f"rej[iv=0 zr=0 al=0] gen=0 fill=1 plu=0.0006 plc=0.0013")


def diag48(t, legs, dgq):
    d = "".join(f" dgq{i}[hand={h} tx={x} full=0 err=0 sp=4167376]" for i, (h, x) in enumerate(dgq))
    return (f"[DIAG] t={t:.1f}s win=838/838 paused=42% good=619.0Mbit cwnd=647 infl=457 np={len(legs)} np_act={len(legs)} rtt=15.8ms bdp100=165sym" + d
            + "".join(leg48(i, *l) for i, l in enumerate(legs)) + " rce=1/c1 " + "".join(f"dgev{i}=3 " for i in range(len(legs))) + "eb_bursts=1")


def qcc(t, blocks):
    return f"[QCC] t={t:.1f}s" + "".join(f" p{i}:qm={m} qrt_us={rt} qsr_us={sr} qbw=104000000 qmin_us={rt - 100} qcns=900/{10 + t:.0f}/880" for i, (m, rt, sr) in enumerate(blocks))


def run_lines(fam, cand, n=100):
    nleg = len(BDP[fam])
    L = ['{"seconds":0.01,"warmup":true}', BRS if cand else STOCK]
    sent = [1000] * nleg
    for k in range(1, n + 1):
        t = 0.25 * k
        legs = []
        for i in range(nleg):
            sent[i] += 100
            legs.append((sent[i], 2 * BDP[fam][i], 20, 3))
        L.append(diag48(t, legs, [(5000 + 100 * k, 5000 + 100 * k) for _ in range(nleg)]))
        if cand and k % 4 == 0:
            L.append(qcc(t, [("B", 2000 if i == 0 else 40000, 4000 if i == 0 else 60000) for i in range(nleg)]))
    return L


# ── plan 46: test_harness46.gen / diag_line / logs_for / qd ──
T_ON, T_OFF = 6.0, 8.0


def gen(sigma=0.18, back_at=None, g_pre=100.0, g_post=95.0, g_hold=40.0, wrtt_pre=70, delta_w=1500, hit=1, rtt_frozen=False, n_end=45.0, dt=0.25, period=2.0):
    S, tx = [], [0, 0]
    t, w = 0.0, wrtt_pre
    last_sample = T_OFF
    while t < n_end:
        t = round(t + dt, 6)
        recovered = back_at is not None and t >= T_OFF + back_at
        if t < T_ON or recovered:
            d_hit = int(1000 * sigma / (1 - sigma))
        elif T_ON <= t < T_OFF + 1.5 and not recovered:
            d_hit = 1 if t >= T_ON + 0.5 else int(1000 * sigma / (1 - sigma))
        else:
            d_hit = 1
        tx[1 - hit] += 1000
        tx[hit] += d_hit
        infl_hit = 0 if (t >= T_ON + 0.5 and not recovered) else 5
        if T_ON <= t < T_OFF:
            w = wrtt_pre + delta_w
        elif t >= T_OFF and w > wrtt_pre * 1.02 and (recovered or t - last_sample >= period / 2):
            if recovered:
                w = wrtt_pre
            else:
                w = int(w - (w - wrtt_pre) / 8)
                last_sample = t
        g = g_pre if t < T_ON else g_hold if t < T_OFF else g_post
        rtt_hit = w if not rtt_frozen else (wrtt_pre + delta_w)
        legs = [{"infl": 5, "rtt": 70, "wrtt": 70, "wpl": 70000}, {"infl": 5, "rtt": 70, "wrtt": 70, "wpl": 70000}]
        legs[hit] = {"infl": infl_hit, "rtt": rtt_hit, "wrtt": w, "wpl": 1000 * w}
        S.append({"t": t, "good": g, "tx": {0: tx[0], 1: tx[1]}, "infl": {i: legs[i]["infl"] for i in (0, 1)}, "rtt": {i: legs[i]["rtt"] for i in (0, 1)},
                  "wrtt": {i: legs[i]["wrtt"] for i in (0, 1)}, "wpl": {i: legs[i]["wpl"] for i in (0, 1)}})
    return S


def diag46(s, wpl=True):
    legs = "".join(f" p{i}:infl={s['infl'][i]}/sinfl=0/bdp1(cap0) sout=0 khr=1.00/kraw=- btlbw=1 sr=0/g0d0 est=Y pl=0.0000 cmp=- rtt={s['rtt'][i]}/wrtt={s['wrtt'][i]}/rtp3ms rtp_us=1 "
                   f"| ANCHOR sent=0 plu=0.0010 plc=0.0014" + (f" wpl_us={s['wpl'][i]}" if wpl else "") for i in range(2))
    return (f"[DIAG] t={s['t']}s win=1/1 paused=0% good={s['good']}Mbit cwnd=1 infl=1 np=2 rtt=1.0ms dgq0[hand=1 tx={s['tx'][0]} full=0 err=0 sp=1] "
            f"dgq1[hand=1 tx={s['tx'][1]} full=0 err=0 sp=1]{legs} dgev0=0")


# ── plan 52 (tail52_score.row_metrics): the sender's recovery fields on the [DIAG] lines, [HOLD], the receiver's [C8CONV-R] ──
def extras52(k, n_src=16):
    """The fields appended to the k-th plan-48-shaped [DIAG] line (t = 0.25 k): source until line n_src, then a tail."""
    src = 1000 if k <= n_src else 0
    s = 250 * min(k, n_src)
    return (f" src={src}sym/s cod=0sym/s cum={s}/5/{s - 700} retx={100 * k} mpr[rep={10 * k} seqs={500 * k} fired={100 * k} supp=0/0/0] "
            f"fp/on p0:{6 * k}/{4 * k} p1:{2 * k}/{4 * k}] wnd2=0/0")


HOLD52 = ("[HOLD] site=sender path={p} gen=0 hn_n=10 fed=99 age_p50_us=3670016 age_p90_us=4718592 cx_n=7 cx_p50_us=229376 cx_p90_us=589824 "
          "cx_leg=0:3,1:4 fa_class=0.0625")
RX52 = "[C8CONV-R] p0:fst=2 dup=7 lead=0 unb=1/5ms p1:fst=2 dup=3 lead=0 unb=1/5ms"


def gates52(lines):
    """The engine's [GATES] tail: RWM_PLACE_WDIV_DERIVED always, RWM_PLACE_WDIV last and only when set."""
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            ln = re.sub(r"\sRWM_PLACE_WDIV(_DERIVED)?=\S+", "", ln)
            ln += f" RWM_PLACE_WDIV_DERIVED={os.environ.get('RWM_PLACE_WDIV_DERIVED') or '0'}"
            if os.environ.get("RWM_PLACE_WDIV"):
                ln += f" RWM_PLACE_WDIV={os.environ['RWM_PLACE_WDIV']}"
        out.append(ln)
    return out


# ── plan 53 (snap53_score.row_metrics): plan 52's fields plus the fire counters (y / r / fast), [HOLD] hy / ripe_frac, [FCAUSE], [SNAPV], the server's final [SUCC] ──
def extras53(k, n_src=16):
    """extras52 with the `y= r= fast=` block of the engine's mpr[..] (fired = y + r; y == fast, as on the s52 rows)."""
    return extras52(k, n_src).replace(f"fired={100 * k} supp=", f"fired={100 * k} y={30 * k} r={70 * k} fast={30 * k} supp=")


HOLD53 = ("[HOLD] site=sender path={p} gen=0 samp_n=1000 fed=99 hn_n=10 hy_n=3 hy_p50_us=5120 cx_n=7 cx_p50_us=229376 cx_p90_us=589824 age_n=50 age_ripe=2 ripe_frac=0.0400 "
          "age_p50_us=3670016 age_p90_us=4718592 cx_leg=0:3,1:4 fa_class=0.0625")
FCAUSE53 = "[FCAUSE] gen=0 n=2000 timer=3 gap_data=1980 gap_refresh=17 other=0 timer_frac=0.0015 gap_frac=0.9900 fired=2000 unattr=0 fa_class=0.0625"
SUCC53 = "[SUCC] gen=0 det=190 res=190 orig_n=100 rep_n=5 final=1"       # the receiver's final line: det 190 against the two paths' fed 99 + 99 -> fed - det = 8 <= cx = 14
BAD53 = {"nosnapv", "snapfire", "nofinal"}          # test_snap53 / test_stubrun red cases: STUB_BAD53 names the defect the stub injects


def snapv53():
    """The sender's [SNAPV] line as the engine prints it (snapv_line_format_is_pinned). Arm on (RWM_RECOV_SNAPVER=1): no stale judgement, as the arm guarantees."""
    on = os.environ.get("RWM_RECOV_SNAPVER") == "1"
    pt, tt = (0, 0) if on else (4, 1)
    if os.environ.get("STUB_BAD53") == "snapfire":
        pt = 3
    return (f"[SNAPV] site=sender p0:rep=900 stale=0 pt_st=0 tt_st=0 p1:rep=120 stale=100 pt_st={pt} tt_st={tt} p-:rep=7 stale=0 pt_st=0 tt_st=0 fa_class=0.0625")


def gates53(lines):
    """The engine's [GATES] tail for plan 53: gates52's WDIV tokens plus RWM_RECOV_SNAPVER=<0|1> (always echoed), from the env the engine received."""
    out = []
    for ln in gates52(lines):
        if "[GATES]" in ln:
            ln = re.sub(r"\sRWM_RECOV_SNAPVER=\S+", "", ln) + f" RWM_RECOV_SNAPVER={os.environ.get('RWM_RECOV_SNAPVER') or '0'}"
        out.append(ln)
    return out


# ── plan 54 (pool54_score): the guard's three witnesses (LD_WITNESS54=1 on a plan-41d stage) and the block (plan 54b) ──
BAD54 = {"nosnapv", "snapfire", "nohn", "poolfallback", "nopoolinfo"}          # test_pool54 / test_stubrun red cases: STUB_BAD54 names the defect the stub injects
POOL_INFO = "\x1b[2m2026-11-13T08:20:48Z\x1b[0m INFO raptorpath::net: path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS: cap = clamp(gain*bdp, floor, n_live*pool)) pool_per_path={n}"


def snapv54():
    """The sender's [SNAPV] line of a post-law build: the law judges only the newest snapshot, so pt_st = tt_st = 0 on every leg (STUB_BAD54=snapfire breaks it)."""
    pt = 2 if os.environ.get("STUB_BAD54") == "snapfire" else 0
    return f"[SNAPV] site=sender p0:rep=900 stale=0 pt_st=0 tt_st=0 p1:rep=120 stale=100 pt_st={pt} tt_st=0 p-:rep=7 stale=0 pt_st=0 tt_st=0 fa_class=0.0625"


def hold54(cand):
    """[HOLD] lines with the p0 hn_n of the arm: collapsed with the law (CAND), main's ~10 000 without (BASE); STUB_BAD54=nohn leaves CAND uncollapsed."""
    hn = 120 if cand else 9800
    if cand and os.environ.get("STUB_BAD54") == "nohn":
        hn = 5000
    return [HOLD53.format(p=0).replace("hn_n=10", f"hn_n={hn}"), HOLD53.format(p=1)]


def witness54(cand):
    """Appended to a plan-41d row's client log when LD_WITNESS54=1: the [HOLD] lines on every row, [SNAPV] on CAND rows only (main has no gauge)."""
    return hold54(cand) + ([snapv54()] if cand and os.environ.get("STUB_BAD54") != "nosnapv" else [])


def gates54(lines, pool):
    """The engine's [GATES] RWM_STORE_PATH_POOL echo: the value it parsed (STUB_BAD54=poolfallback = an unparsable request, silently 2048)."""
    echo = 2048 if os.environ.get("STUB_BAD54") == "poolfallback" else pool
    return [re.sub(r"\sRWM_STORE_PATH_POOL=\S+", "", ln) + f" RWM_STORE_PATH_POOL={echo}" if "[GATES]" in ln else ln for ln in lines]


def rank54(old):
    """The server's [RANK] readouts (about 1 s apart): holes = old + the tail's overcount; 40 of them so readouts 15..-6 exist."""
    return [f"[RANK] holes={old + 100} pivots=2 deficit={old} tail_overcount=100 max_deficit={old} empty=0 reports={i}" for i in range(1, 41)]


def summary54(secs, nbytes=600000000):
    return ('{"bytes":%d,"dnf":0,"hint":"bulk","max_s":%s,"mean_mbps":%.3f,"mean_s":%s,"median_s":%s,"min_s":%s,"proto":"rp-native","runs":1,"stdev_s":0.0,"summary":true}'
            % (nbytes, secs, nbytes * 8 / secs / 1e6, secs, secs, secs))


# ── plan 56 (pool56_score): the sweep's single-seat rows. STUB_BAD56: poolfallback (every row echoes 2048), poolonce (the FIRST c8 seed-42 P384 row echoes 2048, its replacement is right),
# dumpshort (the FIRST dump row writes 50 000 records, the re-run 120 000), nodump (the dump row writes no record at all); a comma list combines them. STUB_DNF56=P<pool>:<seed>: that arm's rows at that seed time out. ──
BAD56 = {"poolfallback", "poolonce", "dumpshort", "nodump", "poolinfo"}
POOL_INFO56 = POOL_INFO


def gates56(lines, pool, dump):
    """The engine's [GATES] echo of the two knobs this sweep varies: RWM_STORE_PATH_POOL and RWM_SUCC_DUMP (the value the engine parsed)."""
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            ln = re.sub(r"\sRWM_(STORE_PATH_POOL|SUCC_DUMP)=\S+", "", ln) + f" RWM_STORE_PATH_POOL={pool} RWM_SUCC_DUMP={1 if dump else 0}"
        out.append(ln)
    return out


def succdump56(records, batch=5000):
    """The receiver's raw successor dump: `[SUCCDUMP] n=<k> d=<tag>,<us>;...` batches (tags o / x / r / a, as the engine prints them)."""
    out, left = [], records
    while left > 0:
        k = min(batch, left)
        out.append(f"[SUCCDUMP] n={k} d=" + ";".join("o,%d" % (20000 + (i * 37) % 90000) for i in range(k)))
        left -= k
    return out


# ── plans 57a / 57b (pool57_score): c1s-400 (57a) / c1d-400 (57b) rows at 5 GB. STUB_BAD57: poolfallback (every row echoes pool 2048), burstfallback (every row echoes the shipped burst 64),
# burstinert (the knob does not act: the realised mean burst stays 48 whatever RWM_EMIT_BURST says), poolonce (the FIRST B64_448 (57a) / P384 (57b) seed-42 row echoes pool 2048, its replacement is right);
# a comma list combines them. STUB_DNF57=<arm>:<seed>: that arm's rows at that seed time out. ──
BAD57 = {"poolfallback", "burstfallback", "burstinert", "poolonce"}


def gates57(lines, pool, burst):
    """The engine's [GATES] echo of the knobs plans 57a / 57b vary: RWM_STORE_PATH_POOL and RWM_EMIT_BURST (the values the engine parsed), the dump off."""
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            ln = re.sub(r"\sRWM_(STORE_PATH_POOL|EMIT_BURST|SUCC_DUMP)=\S+", "", ln) + f" RWM_STORE_PATH_POOL={pool} RWM_EMIT_BURST={burst} RWM_SUCC_DUMP=0"
        out.append(ln)
    return out


def diag57(ln, k, n_src, burst_real, rtt_top, rtt_leg, release=100):
    """The plan-57 fields of the k-th plan-48-shaped [DIAG] line (extras53 has added src / cum): the top-level and per-leg echo rtt, the ack wakes (one per `release` symbols sent), wait[]
    and the emission-burst gauges (eb_syms follows the cumulative symbols sent, eb_bursts = eb_syms / burst_real)."""
    cum = 250 * min(k, n_src)
    ln = ln.replace("rtt=15.8ms", f"rtt={rtt_top:.1f}ms").replace("rtt=20/wrtt=3", f"rtt={rtt_leg}/wrtt=3")
    ln = re.sub(r"eb_bursts=\d+", f"eb_bursts={int(cum / burst_real)} eb_syms={cum}", ln)
    return ln + f" wait[tun=1% ack=82% cmd=0% busy=45%] wake[tun=1 ack={int(cum / release)}]"


def qd(delay, backlog=3):
    return (f"qdisc netem 1: root refcnt 2 limit 1000 delay {delay}  5ms loss gemodel 2% 40% 1-h 0% 1-k 100% rate 20Mbit seed 1042 \n"
            f" Sent 1000 bytes 10 pkt (dropped 1, overlimits 0 requeues 0)\n backlog 0b {backlog}p requeues 0\n")


def qcap(before, durings, after):
    s = "### before wall=1.0\n" + before
    for i, d in enumerate(durings):
        s += f"### during wall={2 + i}.0\n" + d
    return s + "### after wall=9.0\n" + after


def main(argv):
    ca, cb, _hint, nbytes, _n, mode = argv[:6]
    fam = {"c1": "c1d" if mode == "dual" else "c1s", "c2": "c8" if cb == "c3" else "c2"}[ca]
    plan = os.environ.get("LD_PLAN", "")
    base = "bin-base" in os.environ.get("RWM_BIN", "")
    cand = not base
    drv, cli, srv = (lc.read(os.path.join(PD, FIX["dual" if mode == "dual" else "single"] + x)) for x in ("drv.out", "c.log", "s.log"))
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and "[ETA] site=sender" not in ln and STOCK_ECHO not in ln and BRS_ECHO not in ln]
    cli = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in cli]
    srv = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in srv]
    nleg = len(BDP[fam])
    egress = [500, 500] if nleg == 2 else [1000]
    drv = drv + [f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams={e} egress_skbs=10 gso=1.00 netem_sent_dgrams={e} netem_dropped_skbs=0 backlog=0 lost=5 loss=0.005000 rcvbuf_drops=0 rcvbuf_scope=netns"
                 for i, e in enumerate(egress)]
    evt = evtq = None
    if plan in ("46",):
        hit, delta = (int(x) for x in os.environ["RWM_EVENT"].split(":"))
        if base:      # BASE at SLOW-HIT: the frozen-rtt mechanism witness; the leg never comes back inside the horizon
            S = gen(back_at=None, rtt_frozen=True, hit=hit, sigma=0.18 if hit == 1 else 0.82, delta_w=delta)
        else:
            S = gen(back_at=3.0 if hit == 1 else 2.0, hit=hit, sigma=0.18 if hit == 1 else 0.82, delta_w=delta)
        cli = cli + [diag46(s, wpl=cand) for s in S]
        evt = (f"[EVENT] start wall=1000.000\n[EVENT] on wall_before={1000 + T_ON - 0.02:.3f} wall_after={1000 + T_ON + 0.02:.3f} rc=0 delay_ms={delta + 20} diag_t={T_ON}\n"
               f"[EVENT] off wall_before={1000 + T_OFF - 0.02:.3f} wall_after={1000 + T_OFF + 0.02:.3f} rc=0 delay_ms=20 diag_t={T_OFF}\n[EVENT] done wall=1020.000\n"
               + "".join(f"[DIAGSEEN] t={s['t']} wall={1000 + s['t'] + 0.01:.3f}\n" for s in S))
        if os.environ.get("RWM_EVENT_QDISC") == "1":
            evtq = qcap(qd("20ms"), [qd("1.52s", 120), qd("1.52s", 340), qd("1.52s", 600)], qd("20ms", 0))
    elif plan == "41d" and nbytes == "600000000":
        # Status 41d: a 600 MB row (c2 / c8) carries 50 s of [DIAG] ticks with src= and cum= (the in-order delay reads them). With STUB_SHORT_ONCE=1 the FIRST
        # BASE invocation at seed 7 of each long cell leaves a 10 s row (the VOID rule's input); the replacement run of the same slot finds the marker and is full length.
        n_ticks = 200
        marker = os.path.join(TMP, "short-done-%s-%s" % (fam, os.environ.get("SEED", "")))
        if os.environ.get("STUB_SHORT_ONCE") == "1" and base and os.environ.get("SEED") == "7" and not os.path.exists(marker):
            open(marker, "w").close()
            n_ticks = 40
        rl = run_lines(fam, False, n=n_ticks)
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = ln + extras52(k, n_src=n_ticks - 20)
        cli = cli + rl
    elif plan == "52":
        rl = run_lines(fam, False, n=20)         # 5 s of [DIAG] (the fixture's run is 5.09 s): source to 4.0 s, a tail after it
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = ln + extras52(k)
        cli = gates52(cli) + rl + [HOLD52.format(p=0), HOLD52.format(p=1)]
        srv = gates52(srv) + [RX52]
    elif plan == "53":
        rl = run_lines(fam, False, n=20)
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = ln + extras53(k)
        tail = [HOLD53.format(p=0), HOLD53.format(p=1), FCAUSE53] + ([] if os.environ.get("STUB_BAD53") == "nosnapv" else [snapv53()])
        cli = gates53(cli) + rl + tail
        srv = gates53([ln for ln in srv if not ln.startswith(("[SUCC]", "[C8CONV-R]"))]) + [RX52] + ([] if os.environ.get("STUB_BAD53") == "nofinal" else [SUCC53])
    elif plan == "54b":
        # Status 54, the block: 55 s of [DIAG] (source until 50 s, a tail after it) whose cap is 2 x the requested pool, the pool's [GATES] echo and INFO line on both ends, the server's
        # [RANK] readouts, a driver summary matching the run. The B arms leave old holes (1 200), the L arms none, as the registration predicts at W = 4096.
        pool = int(os.environ.get("RWM_STORE_PATH_POOL") or 2048)
        rl = [ln.replace("win=838/838", f"win=800/{2 * pool}") for ln in run_lines(fam, False, n=220)]
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = ln + extras53(k, n_src=200)
        info = [] if os.environ.get("STUB_BAD54") == "nopoolinfo" else [POOL_INFO.format(n=2048 if os.environ.get("STUB_BAD54") == "poolfallback" else pool)]
        cli = gates54(cli, pool) + info + rl + hold54(cand) + [FCAUSE53, snapv54()]
        srv = (gates54([ln for ln in srv if not ln.startswith(("[SUCC]", "[C8CONV-R]", "[RANK]"))], pool)
               + [POOL_INFO.format(n=2048 if os.environ.get("STUB_BAD54") == "poolfallback" else pool)] + [RX52, SUCC53] + rank54(0 if cand else 1200))
        drv = [ln for ln in drv if '"summary"' not in ln] + [summary54(54.0)]
        cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + ['{"bytes":600000000,"hint":"bulk","mbps":88.889,"proto":"rp-native","run":1,"seconds":54.0}',
                                                                                    summary54(54.0)]      # the row's seconds / mbps / dnf read the CLIENT log's run line
        if os.environ.get("STUB_DNF54") == ("B" if base else "L") + str(pool) and os.environ.get("SEED") == "7":
            # STUB_DNF54=B640: that arm's seed-7 rows time out (the [DIAG] ticks above are what the run printed before the timeout): a DNF row with its readouts
            dnf = '{"bytes":600000000,"dnf":true,"hint":"bulk","proto":"rp-native","run":1}'
            cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + [dnf, '{"bytes":600000000,"dnf":1,"hint":"bulk","proto":"rp-native","runs":1,"summary":true}']
            drv = [ln for ln in drv if '"summary"' not in ln] + ['{"bytes":600000000,"dnf":1,"hint":"bulk","proto":"rp-native","runs":1,"summary":true}']
    elif plan == "56":
        # Status 56, the sweep: 55 s of [DIAG] (source until 50 s) whose cap is N x the requested pool (N = 2 live paths at c8, 1 at c2 / c1s), the two knobs' [GATES] echo and the pool INFO line on
        # both ends, the server's [RANK] readouts (no old holes), a driver summary for the cell's size. The dump row (RWM_SUCC_DUMP=1) leaves its [SUCCDUMP] records in the server log.
        pool = int(os.environ.get("RWM_STORE_PATH_POOL") or 2048)
        dump = os.environ.get("RWM_SUCC_DUMP") == "1"
        nlive = 2 if fam == "c8" else 1
        bad = set(x for x in os.environ.get("STUB_BAD56", "").split(",") if x)
        echo = pool
        if "poolfallback" in bad:
            echo = 2048
        elif "poolonce" in bad and fam == "c8" and pool == 384 and os.environ.get("SEED") == "42":
            marker = os.path.join(TMP, "pool-once-done")
            if not os.path.exists(marker):
                open(marker, "w").close()
                echo = 2048
        rl = [ln.replace("win=838/838", f"win=800/{nlive * pool}") for ln in run_lines(fam, False, n=220)]
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = ln + extras53(k, n_src=200)
        info = [] if "poolinfo" in bad else [POOL_INFO56.format(n=echo)]
        nrec = 0
        if dump and "nodump" not in bad:
            nrec = 120000
            if "dumpshort" in bad:
                # the smoke's dump row is invocation 0 (full), the scored row invocation 1 (short), its replacement invocation 2 (full)
                marker = os.path.join(TMP, "dump-count")
                seen = len(open(marker).read()) if os.path.exists(marker) else 0
                open(marker, "a").write("x")
                if seen == 1:
                    nrec = 50000
        secs = 54.0 if nbytes == "600000000" else 100.0
        cli = gates56(cli, echo, dump) + info + rl + hold54(True) + [FCAUSE53, snapv54()]
        srv = (gates56([ln for ln in srv if not ln.startswith(("[SUCC]", "[C8CONV-R]", "[RANK]"))], echo, dump)
               + [POOL_INFO56.format(n=echo)] + [RX52, SUCC53] + rank54(0) + succdump56(nrec))
        drv = [ln for ln in drv if '"summary"' not in ln] + [summary54(secs, int(nbytes))]
        cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + ['{"bytes":%s,"hint":"bulk","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%s}' % (nbytes, int(nbytes) * 8 / secs / 1e6, secs),
                                                                                    summary54(secs, int(nbytes))]
        if os.environ.get("STUB_DNF56") == "P%d:%s" % (pool, os.environ.get("SEED")):
            dnf = '{"bytes":%s,"dnf":true,"hint":"bulk","proto":"rp-native","run":1}' % nbytes
            tail = '{"bytes":%s,"dnf":1,"hint":"bulk","proto":"rp-native","runs":1,"summary":true}' % nbytes
            cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + [dnf, tail]
            drv = [ln for ln in drv if '"summary"' not in ln] + [tail]
    elif plan in ("57a", "57b"):
        # Status 57a / 57b: 55 s of [DIAG] (source until 50 s) whose cap is n_live x the requested pool, the two knobs' [GATES] echo and the pool INFO line on both ends, the burst / ack-wake /
        # wait gauges (realised mean burst 0.75 x RWM_EMIT_BURST, 100 symbols per ack wake), a per-leg echo rtt that grows with the pool (q_l ~ pool / 60 ms at c1d), the server's [RANK]
        # readouts and a driver summary whose rate falls with a small pool.
        pool = int(os.environ.get("RWM_STORE_PATH_POOL") or 2048)
        burst = int(os.environ.get("RWM_EMIT_BURST") or 64)
        nlive = 2 if fam == "c1d" else 1
        bad = set(x for x in os.environ.get("STUB_BAD57", "").split(",") if x)
        echo_pool, echo_burst = pool, burst
        if "poolfallback" in bad:
            echo_pool = 2048
        if "burstfallback" in bad:
            echo_burst = 64
        if "poolonce" in bad and ((fam == "c1s" and pool == 448) or (fam == "c1d" and pool == 384)) and os.environ.get("SEED") == "42":
            marker = os.path.join(TMP, "pool-once-done")
            if not os.path.exists(marker):
                open(marker, "w").close()
                echo_pool = 2048
        burst_real = 48.0 if "burstinert" in bad else 0.75 * burst
        cap = nlive * pool
        rtt_leg = max(3, int(round(2.4 + pool / 60.0)))
        rtt_top = (4.0 + pool / 400.0) if fam == "c1s" else float(rtt_leg)
        rl = [ln.replace("win=838/838", f"win={int(0.85 * cap)}/{cap}") for ln in run_lines(fam, False, n=220)]
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = diag57(ln + extras53(k, n_src=200), k, 200, burst_real, rtt_top, rtt_leg)
        info = [POOL_INFO.format(n=echo_pool)]
        secs = 100.0 + 25.0 * max(0, 600 - pool) / 600.0 + (os.getpid() % 7) * 0.3
        cli = gates57(cli, echo_pool, echo_burst) + info + rl + [HOLD53.format(p=i) for i in range(nlive)] + [FCAUSE53, snapv54()]
        srv = (gates57([ln for ln in srv if not ln.startswith(("[SUCC]", "[C8CONV-R]", "[RANK]"))], echo_pool, echo_burst)
               + [POOL_INFO.format(n=echo_pool)] + [RX52, SUCC53] + rank54(0))
        drv = [ln for ln in drv if '"summary"' not in ln] + [summary54(secs, int(nbytes))]
        cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + ['{"bytes":%s,"hint":"bulk","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%s}' % (nbytes, int(nbytes) * 8 / secs / 1e6, secs),
                                                                                    summary54(secs, int(nbytes))]
        armtag = {512: "B64_512", 448: "B64_448"}.get(pool) if fam == "c1s" else "P%d" % pool
        if fam == "c1s" and burst == 256:
            armtag = "B256_512"
        if os.environ.get("STUB_DNF57") == "%s:%s" % (armtag, os.environ.get("SEED")):
            dnf = '{"bytes":%s,"dnf":true,"hint":"bulk","proto":"rp-native","run":1}' % nbytes
            tail = '{"bytes":%s,"dnf":1,"hint":"bulk","proto":"rp-native","runs":1,"summary":true}' % nbytes
            cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + [dnf, tail]
            drv = [ln for ln in drv if '"summary"' not in ln] + [tail]
    else:
        cli = cli + run_lines(fam, cand and os.environ.get("RWM_QUIC_CC") == "bbr_rs")
    if plan == "41d" and os.environ.get("LD_WITNESS54") == "1":
        cli = cli + witness54(cand)         # plan 54 = plan 41d + the three witnesses: the stub carries what a post-law (CAND) / pre-law (BASE) build prints
    sys.stdout.write("\n".join(drv) + "\n")            # the driver's stdout: the battery redirects it to /tmp/ld-drv.out
    for name, lines in (("rwm-c.log", cli), ("rwm-s.log", srv)):
        path = os.path.join(TMP, name)
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
    for name, text in (("rwm-evt.txt", evt), ("rwm-evtq.txt", evtq)):
        path = os.path.join(TMP, name)
        if text is not None:
            with open(path, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
