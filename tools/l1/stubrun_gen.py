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
BDP = {"c1s": [250000], "c1d": [250000, 250000], "c2": [125000], "c8": [125000, 100000], "c3": [100000]}
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


# ── plan 59 (mbar59_score): the evicting seat at c2-100, 600 MB. STUB_BAD59: noshed (every row is NOT armed: it keeps the retain-until-acked echo, no shed echo, no shed= token), shedonce (the first SCORED slot
# slot is not armed, its replacement is); a comma list combines them. STUB_DNF59=<seed>: that seed's rows time out (an evicting seat's expected outcome). The per-tick deltas are known:
# 250 src / 40 cod / 240 acked symbols and 20 ack wakes per 0.25 s tick, wait n = 100 + k % 5 loop iterations per tick (a non-constant per-period count). ──
BAD59 = {"noshed", "shedonce", "norr"}
SHED_ECHO59 = "unified overload shedding ACTIVE (RWM_UNIFIED_SHED: past-deadline holes shed within the derived 1-rho budget; =0 = serializing arm)"
SHED_ECHO59_RX = "unified overload shedding ACTIVE at receiver (RWM_UNIFIED_SHED: in-order hold = delta-derived b*SRTT within the eps-class give-up budget)"
REALTIME_ECHO59 = "RWM_UNIFIED: Realtime rides the RLC span machine (small-δ parameterization, no code-family switch)"
INFO59 = "\x1b[2m2026-11-12T10:00:00Z\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2mraptorpath::net\x1b[0m\x1b[2m:\x1b[0m "


def seat59(lines, armed, side):
    """A fixture log turned into the evicting seat's: the Realtime echo for the reliable-window one, [PIPE] hint=realtime, the shed echo (an armed seat only). An unarmed row keeps its retention line."""
    out = []
    for ln in lines:
        if "reliable window mode (retain until acked)" in ln:
            out.append(INFO59 + REALTIME_ECHO59 if armed else ln)
            continue
        if "[PIPE]" in ln:
            ln = ln.replace("hint=bulk", "hint=realtime")
        out.append(ln)
    if armed:
        out.append(INFO59 + (SHED_ECHO59 if side == "cli" else SHED_ECHO59_RX))
    return out


def diag59(ln, k, armed, n_src=200):
    """The plan-59 fields of the k-th plan-48-shaped [DIAG] line: src / cod rates, cum = src/cod/ack (cumulative), wait[.. n=] (per period), wake[.. ack=] (cumulative), the shed gauge."""
    s = 250 * min(k, n_src)
    src = 2000 if k <= n_src else 0
    n = 100 + k % 5
    sh = f" shed={3 * k}/{k} bud=0.0500 D=40ms" if armed else ""
    return (ln + f" src={src}sym/s cod={320 if src else 0}sym/s cum={s}/{40 * min(k, n_src)}/{240 * min(k, n_src)} retx=0{sh} "
            f"wait[tun=1% paused=0% pace=0% gen=0% nack=0% defc=0% tail=0% flush=0% ack=82% cmd=0% n={n} us=250000 busy=45% busy_us=112500] "
            f"wake[tun=1 paused=0 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack={20 * k} cmd=1 timer_acked=0]")


def span59(k, armed, rr_zero=False):
    sh = f" shed={3 * k}/{k} bud=0.0500 D=40ms" if armed else ""
    return (f"[SPAN] t={0.5 * k:.1f}s ack={240 * k} win=[{240 * k},{240 * k + 64}] wsize=64 a_star=Some(12) delta=Some(0.5) owed=0.10 rr={0.0 if rr_zero else 0.1 + 0.001 * k:.3f} debt={0.3:.2f} retx_buf=5{sh}")


# ── plans 60a / 60b (s60_score.row_metrics): the store law's gauge rows at c2-100 / c8-100 / c1s-400. A G / EVICT row (RWM_SLAW=1) prints the per-window [WSTAT] / [SLAW] / [WSTORE] / [SLAWC] lines of the
# engine's render_window format (R / Rb computed with the replay's slaw_r, so the per-row witness holds); a C row (RWM_SLAW=0) prints none. The loop dynamics are KNOWN: leg floor + 0.02 ms per slot of window
# (capped), brakes 0 then 0.1 + 0.01 k ms (so b0k[0] = 0 and b0p = 0.18), m = l = tmin - 0.18 afterwards. STUB_BAD60 (comma list): slawonce (the first scored slot echoes RWM_SLAW on the wrong value: its replacement is
# right), nogo8 (every c8 row starves: leg 0 nk = 4, so the rehearsal's c8 go/no-go reads NO-GO), unattached (c8 leg 1 never gets a sampler: nk = 0 on every window while [DIAG]'s echo count grows).
# STUB_DNF60=<arm>:<seed>: that arm's rows at that seed time out. ──
BAD60 = {"slawonce", "nogo8", "unattached"}
FLOOR60 = {"c2": (9.0,), "c8": (9.0, 14.0), "c1s": (2.0,), "c3": (33.0,)}


def _s60replay():
    d = os.path.join(HERE, "..", "..", "docs", "derivations", "store-law")
    if d not in sys.path:
        sys.path.insert(0, d)
    import s60replay
    return s60replay


def gates60(lines, slaw, pool):
    """The engine's [GATES] echo of the knobs the s60 rows vary: RWM_SLAW (appended: the fixture predates it) and RWM_STORE_PATH_POOL; RWM_DIAG=1 and RWM_THREE_TERM=0 are already in the fixture's echo."""
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            ln = re.sub(r"\sRWM_(STORE_PATH_POOL|SLAW)=\S+", "", ln) + f" RWM_STORE_PATH_POOL={pool} RWM_SLAW={slaw}"
        out.append(ln)
    return out


def qf60(w):
    return min(0.02 * w, 10.0)


def gauge60(fam, w, n, bad, evict):
    pool60 = w // len(FLOOR60[fam])
    """-> {k: [lines]}: window k's [WSTAT] / [SLAW] / [WSTORE] / [SLAWC] lines (all legs), k = 1..n."""
    sr = _s60replay()
    floors = FLOOR60[fam]
    nleg = len(floors)
    q = 0.05 if evict else 0.1      # Realtime rows print q = 0.05, Bulk rows 0.10 (s60 section 5)
    brakes = {p: [] for p in range(nleg)}
    out = {}
    anc_bind = [0, 0]
    for k in range(1, n + 1):
        t = 0.25 * k
        legs = []
        for p in range(nleg):
            nk = min(5 * w, 1000) if fam == "c3" else 20      # c3 (status 61): ~5 echo samples per slot per window, so a floor window holds 60-120 samples and G(250) / G(2100) pool 3-5 windows
            if fam == "c8" and p == 1 and "unattached" in bad:
                nk = 0
            if fam == "c8" and p == 0 and "nogo8" in bad:
                nk = 4
            tmin = floors[p] + qf60(w) * (0.6 if p else 1.0)
            if "gatedrop" in bad and w == 12 and k > 8 and k % 10 == 0:
                tmin -= 4.0      # plan 62 NO-GO-GATE: the floor minimum falls with the sample count (D0 >= 0.5 ms)
            tp50, tmean = tmin + 1.2, tmin + 1.25
            brake = 0.0 if k == 1 else 0.1 + 0.01 * k
            if k <= 8:
                brakes[p].append(brake)
            else:
                brake = 0.15
            m = tmin - (max(brakes[p][:8]) if k > 8 else brake)
            legs.append(dict(p=p, nk=nk, tmin=tmin, tp50=tp50, tmean=tmean, m=m, l=m, brake=brake, ready=k >= 8))
        lines = []
        live = [d for d in legs if d["nk"] > 0]
        ready = k >= 8 and all(d["ready"] for d in live)
        r = rb = None
        if ready and live:
            for boot in (False, True):
                inp = [(d["nk"], d["tp50"], d["tmin"], d["m"], d["l"], round(max(brakes[d["p"]][:8]), 3), 0.3 if boot else 0.3) for d in live]
                v = sr.slaw_r(inp, q)
                if boot:
                    rb = v
                else:
                    r = v
        for d in legs:
            p = d["p"]
            if d["nk"] == 0:
                lines.append(f"[WSTAT] t={t:.2f} k={k} p={p} nk=0 qb50=- qbmax=- qcwnd=5000 bdpb=8000")
                lines.append(f"[SLAW] t={t:.2f} k={k} p={p} n={w} W={w} nk=0 aw=0.5000 qb=- qbt=- qcol=1/{k}")
                continue
            b0 = round(max(brakes[p][:8]), 3) if k >= 8 else None
            lines.append(f"[WSTAT] t={t:.2f} k={k} p={p} nk={d['nk']} nkq={d['nk']} tmin={d['tmin']:.3f} p10={d['tmin'] + .04:.3f} p50={d['tp50']:.3f} p90={d['tp50'] + .14:.3f} "
                         f"tmean={d['tmean']:.3f} m={d['m']:.3f} l={d['l']:.3f} qb50=7 qbmax=7 qcwnd=125000 bdpb=250000")
            b0k = ",".join(f"{x:.3f}" for x in brakes[p][:8]) if k == 8 else "-"
            b0kn = ",".join(["20"] * 8) if k == 8 else "-"
            err = f"{d['brake'] - b0 - q * d['l']:.3f}" if b0 is not None else "-"
            lines.append(f"[SLAW] t={t:.2f} k={k} p={p} n={w} W={w} aw=0.5000 nk={d['nk']} tmin={d['tmin']:.3f} tp50={d['tp50']:.3f} tmean={d['tmean']:.3f} m={d['m']:.3f} l={d['l']:.3f} "
                         f"b0p={'-' if b0 is None else f'{b0:.3f}'} b0run={b0 if b0 is not None else 0.1:.3f} b0k={b0k} b0kn={b0kn} kw={k} sig=0.300 sigw=0.140 "
                         f"sigb={'-' if b0 is None else '0.300'} gap={'-' if b0 is None else '0.000'} anc={'-' if b0 is None else '0'} ancf=0/{max(0, k - 7)} err={err} "
                         f"R={'-' if r is None else f'{r:.5f}'} Rb={'-' if rb is None else f'{rb:.5f}'} used={'-' if r is None else len(live)} xs=0 xb=0 ra=0/0 q={q:.6f} qb=7 qbt=7.000 qcol=1/{k}")
        rr = 0.1 + 0.001 * (k // 2)
        fec = f"fec=0.010000/1.2000/{rr:.5f}/12.00/20" if evict else "fec=-/-/-/-/0"
        lines.append(f"[WSTORE] t={t:.2f} k={k} tw=250.0 n={w} W={w} aw=0.5000 awmin=0.5000 relb=1/12/12/12 emb=2/16/16/16 adv=20/50/60/70 ackw=20 {fec} dbt_acc=10.00 dbt_em=5 dbt_share=0.5000 "
                     f"dbt_rm_ack=0.00 dbt_ask_ack=0.00 dbt_rm_nack=0.00 dbt_ask_nack=0.00")
        if k % 4 == 0:
            import math
            nl = len(floors)
            psum = 400.0 + k
            ceiling = max(nl * pool60, 10)
            cap = min(max(math.ceil(2.0 * psum), 10), ceiling)      # caplog_replay's pooled branch
            lines.append(f"[CAPLOG] i={k // 4} t={k * 250000} seat=plain br=pooled on=1 sum=1 dc=0 b=1.0 q={0.05 + 0.05 * (0.5 / 1.5):.9f} n={nl} psum={psum} gain=2.0 floor=10 pool={pool60} smax=4096 boot=128 cap={cap} rep=0")
        lines.append(f"[SLAWC] t={t:.2f} k={k} gaprep=0 gaptrunc=0 gaptrunc_frac=- c09rep=0 c09bind=0 c09_frac=-")
        out[k] = lines
    return out


def plan60_cli(cli, fam, w, nbytes, slaw, evict, bad):
    """-> the client log lines of an s60 row: the fixture's non-[DIAG] lines, then 220 ticks of [DIAG] (known loop dynamics) interleaved with the gauge's window lines (G / EVICT rows) and, for EVICT, plan 59's
    per-tick src / cod / cum / wait / wake fields and a [SPAN] line every 0.5 s."""
    n = 120 if "short" in bad else 220      # plan 62 NO-GO-SHORT: 120 ticks leave 60 native LATE windows (< 100)
    floors = FLOOR60[fam]
    rl = run_lines(fam, False, n=n)
    g = gauge60(fam, w, n, bad, evict) if slaw else {}
    out, k = [], 0
    for ln in rl:
        if not ln.startswith("[DIAG]"):
            out.append(ln)
            continue
        k += 1
        legs = [(1000 + 100 * k, 125000, int(round(floors[i] + qf60(w) * (0.6 if i else 1.0) + 1.0)), 3) for i in range(len(floors))]
        d = diag48(0.25 * k, legs, [(5000 + 100 * k, 5000 + 100 * k) for _ in floors])
        d = d.replace("rtt=15.8ms", f"rtt={floors[0] + qf60(w) + 1.0:.1f}ms").replace("win=838/838", f"win={int(0.85 * w)}/{w}")
        parts = d.split(" rtp_us=2392")
        d = "".join(parts[i] + (f" rtp_us={int(floors[i] * 1000)}" if i < len(floors) else "") for i in range(len(parts)))
        d = re.sub(r"sig_us=38/n1", f"sig_us=38/n{20 * k}", d)
        if evict:
            d = diag59(d, k, True)
        else:
            d = d + extras53(k, n_src=200)
        out.append(d)
        if evict and k % 2 == 0:
            out.append(span59(k // 2, True))
        out.extend(g.get(k, []))
    return out


def gen60(cli, srv, drv, nbytes, fam, hint):
    bad = set(x for x in os.environ.get("STUB_BAD60", "").split(",") if x)
    slaw_env = os.environ.get("RWM_SLAW", "1")
    evict = os.environ.get("RWM_C_RELIABLE") == "0"
    nlive = 2 if fam == "c8" else 1
    pool = int(os.environ.get("RWM_STORE_PATH_POOL") or 2048)
    w = nlive * pool
    echo_slaw = slaw_env
    if "slawonce" in bad:
        # the smoke is invocations 0 and 1 (right), the first scored slot invocation 2 (the wrong RWM_SLAW on the client end), its replacement invocation 3 (right)
        marker = os.path.join(TMP, "slaw-count")
        seen = len(open(marker).read()) if os.path.exists(marker) else 0
        open(marker, "a").write("x")
        if seen == int(os.environ.get("STUB_SLAWONCE_AT") or 2):      # plans 61a / 61b smoke 1 row, so the first scored slot is invocation 1 there (STUB_SLAWONCE_AT=1)
            echo_slaw = "0" if slaw_env == "1" else "1"
    rl = plan60_cli(cli, fam, w, nbytes, slaw_env == "1", evict, bad)
    info = [POOL_INFO.format(n=pool)]
    cli = gates60(cli, echo_slaw, pool)
    srv = gates60([ln for ln in srv if not ln.startswith(("[SUCC]", "[C8CONV-R]", "[RANK]"))], slaw_env, pool)
    if evict:
        cli = seat59(cli, True, "cli")
        srv = seat59(srv, True, "srv")
        drv = [re.sub(r"hint=bulk( .*)$", r"hint=realtime\1 reliable=0", ln) if "--- RWM-C perf" in ln else ln for ln in drv]
    else:
        cli = cli + info
        srv = srv + info
    cli = cli + rl + hold54(True)[:nlive]
    srv = srv + [RX52, SUCC53] + rank54(0)
    mbps = 100.0 * w / (w + 100.0)
    if fam == "c3":
        mbps = min(0.19 * w, 17.3)      # status 61 section 1: 0.19 Mbit/s per slot below the knee, the 17.3 Mbit/s control rate above it (the knee u = 0.95 falls at W 86.5)
        if ("pilotabove1" in bad and w in (80, 128)) or ("pilotabove2" in bad and w in (80, 128, 64, 96)):
            mbps = 17.3      # plan 62: a probe at full goodput (case both-above); pilotabove1 = the first pair only (the re-probe then reads proportional), pilotabove2 = the re-probe's too (NO-GO-PILOT)
    secs = max(20.0, int(nbytes) * 8 / 1e6 / mbps)
    hnt = "realtime" if evict else "bulk"
    drv = [ln for ln in drv if '"summary"' not in ln] + [summary54(secs, int(nbytes))]
    cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + ['{"bytes":%s,"hint":"%s","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%s}' % (nbytes, hnt, int(nbytes) * 8 / secs / 1e6, secs),
                                                                                summary54(secs, int(nbytes))]
    arm = ("G" if slaw_env == "1" else "C") + str(w)
    if os.environ.get("STUB_DNF60") == "%s:%s" % ("EVICT" if evict else arm, os.environ.get("SEED")):
        dnf = '{"bytes":%s,"dnf":true,"hint":"%s","proto":"rp-native","run":1}' % (nbytes, hnt)
        tail = '{"bytes":%s,"dnf":1,"hint":"%s","proto":"rp-native","runs":1,"summary":true}' % (nbytes, hnt)
        cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + [dnf, tail]
        drv = [ln for ln in drv if '"summary"' not in ln] + [tail]
    if os.environ.get("LD_PLAN") in ("62r0", "62a"):
        cli, srv, drv = gen62(cli, srv, drv, w, bad)
    return cli, srv, drv


# ── plan 64 (plan64.py): F2's merge guard. Stage G rows (LD_PLAN=41d with LD_WITNESS64=1) are plan 41d's rows plus what an F2 CAND build prints (`[SYMSZ] local=1200 decoder=1200` on both ends, `undeliv=0` on every [CTLD]
# line); BASE (main) prints neither. Stage R rows (LD_PLAN=64, Realtime c2-100 at rho = 1) carry the Realtime echo, `window_reliable: true` on both ends and 12 s of [DIAG] ticks. STUB_BAD64 names the defect the stub injects
# into the FIRST matching scored row (LD_TAG=ld; a marker file counts invocations; the smoke and the replacement runs are right unless the kind says otherwise):
#   unframable / dnf-unframable / symsz-missing / ctld-send-only   stage G, the first CAND scored row (dnf-unframable is also a DNF)
#   base-rt-dead (the first RB row's warm-up never acks, the driver exits 1) / rt-rho-one-end (the first RC row's server prints window_reliable: false) / rc-void-twice (the first RC slot's row AND its replacement do)
#   rc-nodata (the first RC row ends rc 0 with no run / summary line: NO_DATA, replaced once)   rt-dnf / rt-base-dnf (the first RC row / also the first RB row is a DNF: warm-up acked, transfer timed out) / rt-ctld-send-only / rt-symsz-1200 (the first RC row)   bare (BASE rows print no [CTLD] line at all)
BAD64 = {"unframable", "dnf-unframable", "symsz-missing", "ctld-send-only", "base-rt-dead", "base-rt-dead-rc0", "rc-nodata", "base-dead-twice", "base-dead-twice-x2", "rb-dead-smoke", "rb-dead-smoke2", "rt-rho-one-end", "rc-void-twice", "rt-dnf", "rt-base-dnf", "rt-ctld-send-only", "rt-symsz-1200", "bare"}


def bad64():
    return set(x for x in os.environ.get("STUB_BAD64", "").split(",") if x)


def hit64(kind, upto=1, tag="ld"):
    """True for the first `upto` invocations of the battery pass `tag` (ld = the scored rows, smoke = the smoke) that ask for `kind` (a marker file counts them)."""
    if os.environ.get("LD_TAG") != tag:
        return False
    marker = os.path.join(TMP, "bad64-" + kind)
    seen = len(open(marker).read()) if os.path.exists(marker) else 0
    if seen >= upto:
        return False
    open(marker, "a").write("x")
    return True


def dnf_lines64(cli, drv, nbytes, hint):
    dnf = '{"bytes":%s,"dnf":true,"hint":"%s","proto":"rp-native","run":1}' % (nbytes, hint)
    tail = '{"bytes":%s,"dnf":1,"hint":"%s","proto":"rp-native","runs":1,"summary":true}' % (nbytes, hint)
    return ([ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + [dnf, tail], [ln for ln in drv if '"summary"' not in ln] + [tail])


def f2_ends64(cli, srv, size, cand, ctld_srv=True, symsz_srv=True):
    """What an F2 CAND build prints on top of a fixture log pair: `undeliv=0` on every [CTLD] line (the server's only when ctld_srv), `[SYMSZ] local=<n> decoder=<n>` once per process (the server's only when symsz_srv)."""
    if not cand:
        return cli, srv
    add = lambda lines: [ln + " undeliv=0" if "[CTLD]" in ln and "undeliv=" not in ln else ln for ln in lines]      # noqa: E731
    sz = INFO59 + f"[SYMSZ] local={size} decoder={size}"
    return add(cli) + [sz], (add(srv) if ctld_srv else list(srv)) + ([sz] if symsz_srv else [])


def gen64g(cli, srv, drv, nbytes, cand):
    """Stage G: plan 41d's rows plus the F2 witnesses (CAND) and, for the first scored CAND row, the injected defect."""
    bad = bad64()
    if not cand:
        if "bare" in bad:       # BASE logs with no [CTLD] line at all: the perf script's F2 lines grep nothing (`set -e` territory)
            cli, srv = [ln for ln in cli if "[CTLD]" not in ln], [ln for ln in srv if "[CTLD]" not in ln]
        return cli, srv, drv
    kind = next((k for k in ("unframable", "dnf-unframable", "symsz-missing", "ctld-send-only") if k in bad), None)
    inj = bool(kind) and hit64(kind)
    cli, srv = f2_ends64(cli, srv, 1200, True, ctld_srv=not (inj and kind == "ctld-send-only"), symsz_srv=not (inj and kind == "symsz-missing"))
    if inj and kind in ("unframable", "dnf-unframable"):
        cli = cli + [INFO59 + "\x1b[31mERROR\x1b[0m [UNFRAMABLE] symbol of 1200 B does not fit the packet; tunnel fails closed"]
    if inj and kind == "dnf-unframable":
        cli, drv = dnf_lines64(cli, drv, nbytes, "bulk")
    return cli, srv, drv


def gen64r(cli, srv, drv, fam, nbytes, base, cand):
    """Stage R: a Realtime row at rho = 1: the Realtime echo on both ends (no reliable-window line), window_reliable: true on both ends, [SYMSZ] 512/512 and undeliv=0 on a CAND row, 12 s of [DIAG] ticks, a summary of 12 s."""
    bad = bad64()
    n_ticks = 48
    rl = run_lines(fam, False, n=n_ticks)
    k = 0
    for i, ln in enumerate(rl):
        if ln.startswith("[DIAG]"):
            k += 1
            rl[i] = ln + extras52(k, n_src=n_ticks - 20)
    # post-promotion binaries (the F2 build and main before it, ADR-0073) print the wire gauges: a numeric wpl_us on every leg of every [DIAG] line. A stub row without them hid a parser rule that read them as a violation on plan 65's RB rows.
    rl = [re.sub(r"(" + chr(92) + "bplc=[" + chr(92) + "d.]+)", chr(92) + "1 wpl_us=3000", ln) if ln.startswith("[DIAG]") else ln for ln in rl]

    def rt(lines):
        out = []
        for ln in lines:
            if "reliable window mode (retain until acked)" in ln:
                out.append(INFO59 + REALTIME_ECHO59)
                continue
            out.append(ln.replace("hint=bulk", "hint=realtime") if "[PIPE]" in ln else ln)
        return out
    cli, srv = rt(cli), rt(srv)
    drv = [re.sub(r"hint=bulk( .*)$", r"hint=realtime\1", ln) if "--- RWM-C perf" in ln else ln for ln in drv]
    symsz = 512
    ctld_srv, rho_bad, dnf, dead, nodata = True, False, False, False, False
    if cand:
        if "rt-rho-one-end" in bad and hit64("rt-rho-one-end"):
            rho_bad = True
        if "rc-void-twice" in bad and hit64("rc-void-twice", upto=2):
            rho_bad = True
        if "rc-nodata" in bad and hit64("rc-nodata"):
            nodata = True      # the first RC row: the driver exits 0 but the client log holds no run / summary line (NO_DATA, the warm-up acked): a non-F2 void
        if "rt-ctld-send-only" in bad and hit64("rt-ctld-send-only"):
            ctld_srv = False
        if "rt-symsz-1200" in bad and hit64("rt-symsz-1200"):
            symsz = 1200
        if ("rt-dnf" in bad or "rt-base-dnf" in bad) and hit64("rt-dnf"):
            dnf = True
    else:
        if ("base-rt-dead" in bad or "base-rt-dead-rc0" in bad) and hit64("base-rt-dead"):
            dead = True
        # plan 65: the first RB scored slot dies on both draws (base-dead-twice), the first TWO RB slots do (base-dead-twice-x2: four draws), the first / the first two RB SMOKE rows die (the rehearsal)
        if "base-dead-twice" in bad and hit64("base-dead-twice", upto=2):
            dead = True
        if "base-dead-twice-x2" in bad and hit64("base-dead-twice-x2", upto=4):
            dead = True
        if "rb-dead-smoke" in bad and hit64("rb-dead-smoke", 1, "smoke"):
            dead = True
        if "rb-dead-smoke2" in bad and hit64("rb-dead-smoke2", 2, "smoke"):
            dead = True
        if "rt-base-dnf" in bad and hit64("rt-base-dnf"):
            dnf = True
    cli, srv = f2_ends64(cli, srv, symsz, cand, ctld_srv=ctld_srv)
    shift = float(os.environ.get("STUB_SHIFT65") or 0)
    if cand and shift:
        # plan 65: the CAND rows deliver in order `shift` later: the receiver's cumulative ack (third cum= field) lags the source curve (first field) by (1 + shift) times as much
        def lagged(ln):
            m = re.search(r"cum=(\d+)/(\d+)/(-?\d+)", ln)
            if not m:
                return ln
            a, c = int(m.group(1)), int(m.group(3))
            return ln[:m.start()] + f"cum={a}/{m.group(2)}/{a - int((a - c) * (1 + shift))}" + ln[m.end():]
        rl = [lagged(ln) if ln.startswith("[DIAG]") else ln for ln in rl]
    if rho_bad:
        srv = [ln.replace("window_reliable: true", "window_reliable: false") for ln in srv]
    secs = 12.0
    summ = summary54(secs, int(nbytes)).replace('"hint":"bulk"', '"hint":"realtime"')
    cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + rl + ['{"bytes":%s,"hint":"realtime","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%s}' % (nbytes, int(nbytes) * 8 / secs / 1e6, secs), summ]
    drv = [ln for ln in drv if '"summary"' not in ln] + [summ]
    if nodata:
        cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln]
        drv = [ln for ln in drv if '"summary"' not in ln]
    if dnf or dead:
        cli, drv = dnf_lines64(cli, drv, nbytes, "realtime")
        if dead and "base-rt-dead-rc0" in bad:
            cli = [ln for ln in cli if '"summary"' not in ln]
            drv = [ln for ln in drv if '"summary"' not in ln]
    if dead:
        cli = cli + [INFO59 + "\x1b[31mERROR\x1b[0m warm-up object never acked within the timeout"]
        if "base-rt-dead-rc0" in bad:      # the driver exits 0 and the client log holds no summary line: the battery's bring-up retry would draw the row again if the stage allowed it (TRIES)
            cli = [ln for ln in cli if '"summary"' not in ln]
            drv = [ln for ln in drv if '"summary"' not in ln]
        else:
            open(os.path.join(TMP, "stub-rc1"), "w").close()      # the client died after the full timeout: the driver exits non-zero (the stub shell reads this marker)
    return cli, srv, drv


GOOD_NETEM = "qdisc netem 8001: root refcnt 2 limit 1000 delay 20ms loss gemodel p 2% r 40% 1-h 100% 1-k 0% rate 20Mbit seed 42 "


def gen62(cli, srv, drv, w, bad):
    """Plan 62 on the c3j0 stub: the engine's RWM_RTT_DUMP echo, the live-netem witness the real driver prints (`[NETEM] tag=pre|post leg=cli|srv`; STUB_NETEM62=<invocation>:<pre|post>:<jitter|missing|delay> corrupts one
    read of one row: invocation 0 is the first row the driver is called for), the RTT dump of a D arm (4 352 samples of a path whose estimators saw 4 400: coverage 0.989) and plan 62's control-loss knob."""
    dump = os.environ.get("RWM_RTT_DUMP", "0")
    cli = [re.sub(r"\sRWM_RTT_DUMP=\S+", "", ln) + f" RWM_RTT_DUMP={dump}" if "[GATES]" in ln else ln for ln in cli]
    srv = [re.sub(r"\sRWM_RTT_DUMP=\S+", "", ln) + f" RWM_RTT_DUMP={dump}" if "[GATES]" in ln else ln for ln in srv]
    marker = os.path.join(TMP, "netem-count")
    seen = len(open(marker).read()) if os.path.exists(marker) else 0
    open(marker, "a").write("x")
    want = ((os.environ.get("STUB_NETEM62") or "::") + ":").split(":")      # <invocation>:<pre|post>:<jitter|delay|missing>[:<cli|srv>] (the leg defaults to the data leg)
    bad_leg = want[3] or "cli"
    nl = []
    for tag in ("pre", "post"):
        for leg, dev in (("cli", "cli0"), ("srv", "srv0")):
            text = GOOD_NETEM
            if want[0] == str(seen) and want[1] == tag and leg == bad_leg:
                if want[2] == "jitter":
                    text = text.replace("delay 20ms", "delay 20ms  5ms")
                elif want[2] == "delay":
                    text = text.replace("delay 20ms", "delay 5ms")
                elif want[2] == "missing":
                    continue
            nl.append(f"[NETEM] tag={tag} leg={leg} dev={dev} qdisc: {text}")
    drv = drv + nl
    if dump == "1":
        t0, rows = 1_000_000, []
        for b in range(17):
            rows.append(f"[RTTDUMP] p=0 t0={t0 + b * 256 * 12500} n=256 d=" + ";".join(f"{0 if i == 0 else 12500},{40000 + (i * 37) % 700}" for i in range(256)))
        cli = cli + rows
    if "ctlloss" in bad and w == 512:
        drv = [ln.replace("loss=0.005000", "loss=0.090000") for ln in drv]
    return cli, srv, drv


# ── plan 51e (s51e_score.row_metrics): the evicting seat with the spare-clamp gauge, paced. KNOWN TRUTH per arm (the tests assert these absolute values):
#   PL  700 sym/s, 60 s of emission, A* = 5, Delta = 2, rr 0.25, the clamp never binds: spc=0/0/n/0/n/0, wr = fr = 0.25, eviction share 0, tau_x n/a (src <= 1000 sym/s)
#   PM  8856 sym/s, A* alternating 100 / 101 each [SPAN] (a window is steady only if it sees one [SPAN] line), the same gauge as PL
#   U   unpaced, 100 000 sym/s for 13 s, infl >= cwnd (tau_x = 1), eviction share 0.7, the clamp binds on every recompute (ab = an, phi = 1) and the grant is 0 on 90 % of them (az = 0.9 an, zeta = 0.9, pi = 0.1,
#       wr = 0.2, fr = 0.01 so the deficit is 0.19, fec r = 0.01)
# [WSTORE] carries `fec=<eps>/<sigma2>/<r>/<A*>/<n> spc=<ab>/<az>/<an>/<rb>/<rn>/<r0> wr=<sum wr/rn>/<sum fr/rn>`; the perf client's start echo, the {"pace":...} line and the [GATES] token are the engine branch's formats
# (assumed; see s51e_score.py). STUB_BAD51E (comma list): noecho (PL / PM print no start echo), nopaceline, anfec (fec n = an + 1 on every window), ulow (U zeta 0.3), plastar (PL A* = 12), evictpl (PL eviction 0.9),
# plab (PL: ab = 1 on every window, so no K4-steady window), rr0 (PL / PM rr = 0), paceonce (the FIRST scored slot prints no pace echo, its replacement is right), spconce (the FIRST scored slot has fec n = an + 1, its replacement is right).
# STUB_SMOKE_N51E=<n>: the smoke / rehearsal rows that precede the first scored slot. ──
BAD51E = {"noecho", "nopaceline", "anfec", "ulow", "plastar", "plab", "evictpl", "rr0", "paceonce", "spconce"}


def _once51e(name):
    """True exactly on the FIRST scored invocation (STUB_SMOKE_N51E smoke / rehearsal rows come first); the replacement run of the slot finds the marker and is right."""
    marker = os.path.join(TMP, name + "-count")
    seen = len(open(marker).read()) if os.path.exists(marker) else 0
    open(marker, "a").write("x")
    return seen == int(os.environ.get("STUB_SMOKE_N51E", "1"))


def gates51e(lines, slaw, rate):
    out = []
    for ln in lines:
        if "[GATES]" in ln:
            ln = re.sub(r"\sRWM_(SLAW|PERF_RATE_MBIT)=\S+", "", ln) + f" RWM_SLAW={slaw} RWM_PERF_RATE_MBIT={rate}"
        out.append(ln)
    return out


def gen51e(cli, srv, drv, nbytes, fam, hint):
    bad = set(x for x in os.environ.get("STUB_BAD51E", "").split(",") if x)
    rate_env = os.environ.get("RWM_PERF_RATE_MBIT")
    if "paceonce" in bad and rate_env and _once51e("pace"):
        bad.add("noecho")
    if "spconce" in bad and _once51e("spc"):
        bad.add("anfec")
    paced = rate_env is not None
    lam = float(rate_env) if paced else None
    arm = "U" if not paced else ("PL" if lam < 10 else "PM")      # PL is a few Mbit at any rate the calibration may set, PM is 35
    rate = 100000 if arm == "U" else int(round(lam * 1e6 / (8 * 494)))
    k_on, n_t = (240, 250) if paced else (52, 120)
    nb = int(nbytes)
    if arm == "PL":
        astar_fn, delta = (lambda j: 12 if "plastar" in bad else 5), 2
    elif arm == "PM":
        astar_fn, delta = (lambda j: 100 + (j % 2)), 9
    else:
        astar_fn, delta = (lambda j: 12), 1
    rr = 0.0 if (("rr0" in bad and paced) or arm == "U") else 0.25
    rl = run_lines("c2", False, n=n_t)
    out, k, src_cum = [], 0, 0
    for ln in rl:
        if not ln.startswith("[DIAG]"):
            out.append(ln)
            continue
        k += 1
        on = k <= k_on
        d_src = int(round(rate * 0.25)) if on else 0
        src_cum += d_src
        cod_cum = int(0.2 * src_cum)
        t = 0.25 * k
        hand = 1000 + 100 * k + src_cum
        tx = hand if (arm != "U" and not ("evictpl" in bad and arm == "PL")) else int(hand * (0.3 if arm == "U" else 0.1))
        d = diag48(t, [(1000 + 100 * k, 125000, 20, 3)], [(hand, tx)])
        infl = 700 if arm == "U" else max(10, int(rate * 0.03))
        d = d.replace("paused=42%", "paused=0%").replace("cwnd=647 infl=457", "cwnd=647 infl=%d" % infl)
        d = d + (f" src={rate if on else 0}sym/s cod={int(rate * 0.2) if on else 0}sym/s cum={src_cum}/{cod_cum}/{max(0, src_cum - 100)} retx=0 shed={3 * k}/{k} bud=0.0500 D=40ms "
                 f"wait[tun=1% paused=0% pace=0% gen=0% nack=0% defc=0% tail=0% flush=0% ack=82% cmd=0% n={100 + k % 5} us=250000 busy=45% busy_us=112500] "
                 f"wake[tun=1 paused=0 pace=0 gen=0 nack=0 defc=0 tail=0 flush=0 ack={20 * k} cmd=1 timer_acked=0]")
        out.append(d)
        if k % 2 == 0:
            j = k // 2
            out.append(f"[SPAN] t={0.5 * j:.1f}s ack={max(0, src_cum - 100)} win=[{src_cum},{src_cum + 64}] wsize={25 if arm == 'PL' else 200} a_star=Some({astar_fn(j)}) delta=Some({delta}) owed=0.10 "
                       f"rr={rr:.3f} debt=0.30 retx_buf=5 shed={3 * k}/{k} bud=0.0500 D=40ms")
        if on:
            an = d_src
            fn = an + 1 if "anfec" in bad else an
            if arm == "U":
                az = int(round((0.3 if "ulow" in bad else 0.9) * an))
                spc, wr, fr, rwin = f"{an}/{az}/{an}/{an}/{an}/0", 0.2, 0.01, 0.01
            elif arm == "PL" and "plab" in bad:      # the clamp binds a hair on every window: ab > 0 voids every K4-steady window (the ab = 0 rule), the law r is still handed (fr within the 1.5e-5 grant tolerance)
                spc, wr, fr, rwin = f"1/0/{an}/1/{an}/0", 0.25, 0.249999, 0.25
            else:
                spc, wr, fr, rwin = f"0/0/{an}/0/{an}/0", 0.25, 0.25, 0.25
            out.append(f"[WSTORE] t={t:.2f} k={k} tw=250.0 n=200 W=200 aw=0.5000 awmin=0.5000 relb=1/12/12/12 emb=2/16/16/16 adv=100/100/100/100 ackw=20 "
                       f"fec=0.010000/1.2000/{rwin:.5f}/{float(astar_fn(k // 2)):.2f}/{fn} spc={spc} wr={wr:.6f}/{fr:.6f} dbt_acc=10.00 dbt_em=5 dbt_share=0.5000 "
                       f"dbt_rm_ack=0.00 dbt_ask_ack=0.00 dbt_rm_nack=0.00 dbt_ask_nack=0.00")
        else:
            out.append(f"[WSTORE] t={t:.2f} k={k} tw=250.0 n=200 W=200 aw=0.5000 awmin=0.5000 relb=1/12/12/12 emb=2/16/16/16 adv=0/0/0/0 ackw=0 fec=-/-/-/-/0 spc=0/0/0/0/0/0 wr=-/- dbt_acc=0.00 dbt_em=0 dbt_share=- "
                       f"dbt_rm_ack=0.00 dbt_ask_ack=0.00 dbt_rm_nack=0.00 dbt_ask_nack=0.00")
    cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln]
    echo = [] if (not paced or "noecho" in bad) else [f"perf offered-load pacing ACTIVE (RWM_PERF_RATE_MBIT={rate_env})"]
    cli = seat59(gates51e(cli, os.environ.get("RWM_SLAW", "1"), rate_env or "unset"), True, "cli") + echo + out
    if paced and "nopaceline" not in bad:
        cli = cli + ['{"pace":true,"obj":1,"rate_mbit":%s,"chunks":%d,"feed_s":60.0,"achieved_mbit":%s,"late":0,"late_max_ms":0.4}' % (rate_env, nb // 494, rate_env)]
    srv = seat59(gates51e([ln for ln in srv if not ln.startswith(("[SUCC]", "[C8CONV-R]", "[RANK]"))], os.environ.get("RWM_SLAW", "1"), "unset"), True, "srv")
    drv = [re.sub(r"hint=bulk( .*)$", r"hint=realtime\1 reliable=0", ln) if "--- RWM-C perf" in ln else ln for ln in drv]
    dnf = '{"bytes":%s,"dnf":true,"hint":"realtime","proto":"rp-native","run":1}' % nbytes
    tail = '{"bytes":%s,"dnf":1,"hint":"realtime","proto":"rp-native","runs":1,"summary":true}' % nbytes
    cli = cli + [dnf, tail]
    drv = [ln for ln in drv if '"summary"' not in ln] + [tail]
    return cli, srv, drv


# ── plan 63 (plan63.row_metrics): F2's first contact. gen51e's rows (the arm follows RWM_PERF_RATE_MBIT) with the F2 witnesses. KNOWN TRUTH: the CONTROL seat (RWM_BIN under bin-base) fails the warm-up on the paced arms
# ("warm-up object never acked", a DNF line) and completes U; the CAND seat completes every arm with `[CTLD] undeliv=0`, no [UNFRAMABLE] and `[SYMSZ] local=512 decoder=512` on both ends; the driver prints the
# `--- F2 undeliv=.. unframable=..` line. STUB_BAD63 (comma list): candfail (the CAND's paced arms fail closed: undeliv=2, one [UNFRAMABLE] line, the warm-up never acked), controlpass (the CONTROL's paced arms
# complete: the trigger did not reproduce), noctld (the CAND prints NO [CTLD] line: a missing witness), ctldsend (only the SENDING end, the client, prints [CTLD]: the receiving server's witness is missing). The CONTROL has no counter: its [CTLD] lines carry no undeliv= (status 63: undeliv is n/a there). ──
BAD63 = {"candfail", "controlpass", "noctld", "ctldsend"}


def gen63(cli, srv, drv, nbytes, fam, hint):
    bad = set(x for x in os.environ.get("STUB_BAD63", "").split(",") if x)
    control = "bin-base" in os.environ.get("RWM_BIN", "")
    paced = os.environ.get("RWM_PERF_RATE_MBIT") is not None
    cli, srv, drv = gen51e(cli, srv, drv, nbytes, fam, hint)
    cli = [ln for ln in cli if '"dnf"' not in ln]                       # gen51e's DNF datum and tail: this plan's outcome is the warm-up's
    drv = [ln for ln in drv if '"dnf"' not in ln]
    fail = paced and ((control and "controlpass" not in bad) or (not control and "candfail" in bad))
    undeliv, unf = (2, 1) if (not control and "candfail" in bad and paced) else (0, 0)
    ctld = "[CTLD] t=5.0s mode=wide gen=0" if control else f"[CTLD] t=5.0s mode=wide undeliv={undeliv} gen=0"
    if not (not control and "noctld" in bad):
        cli = cli + [ctld]
        if not (not control and "ctldsend" in bad):
            srv = srv + [ctld]
    if not control:
        cli = cli + ["[SYMSZ] local=512 decoder=512"]
        srv = srv + ["[SYMSZ] local=512 decoder=512"]
    if unf:
        srv = srv + ["\x1b[31mERROR\x1b[0m [UNFRAMABLE] symbol of 1200 B does not fit the peer's 512 B carrier"]
    if fail:
        cli = cli + ["Error: warm-up object never acked - tunnel not passing traffic", '{"dnf":true,"mode":"single"}']
        drv = drv + ['{"dnf":true,"mode":"single"}']
    else:
        secs = 60.0
        cli = cli + ['{"bytes":%s,"hint":"realtime","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%s}' % (nbytes, int(nbytes) * 8 / secs / 1e6, secs), summary54(secs, int(nbytes))]
        drv = drv + [summary54(secs, int(nbytes))]
    return cli, srv, drv


# ── plan 51 (s51_score.row_metrics): the delta sweep's rows. A row is a 220-tick [DIAG] series (55 s), a [SPAN] line every 0.5 s whose rr follows the rate-mix weight (beta = 1: 0.000; beta = 0: 0.2), the sender's
# emission-batching echo, [GATES] RWM_DELTA on both ends, source symbols matching the carrier's symbol size, a summary whose rate varies with the pid. STUB_BAD51 (comma list): deltaonce (the FIRST scored slot echoes
# RWM_DELTA wrongly on the client end; its replacement is right), nobatch (no batching echo), overdrive (dgev / hand 0.1 and infl >= cwnd), shortonce (the FIRST scored slot is a 10 s row), hbrr (rr > 0 at beta = 1),
# wrongsize (source symbols for a 1 182 B carrier on a realtime row). STUB_SMOKE_N51=<n>: the smoke rows that precede the first scored slot. ──
BAD51 = {"deltaonce", "nobatch", "overdrive", "shortonce", "hbrr", "wrongsize"}
PAYLOAD51 = {"bulk": 1182, "auto": 1182, "realtime": 494}
BATCH51 = {"bulk": "emission batching ACTIVE (RWM_EMIT_BATCH: pacer-quantum TUN intake + per-burst taper/span refresh)", "auto": "emission batching ACTIVE (RWM_EMIT_BATCH: pacer-quantum TUN intake + per-burst taper/span refresh)",
           "realtime": "emission batching out of scope (RWM_EMIT_BATCH=1 but per-symbol emission: the burst intake serves the window-reliable plain-source emitter only)"}


def _once51(name, nsmoke):
    marker = os.path.join(TMP, name + "-count")
    seen = len(open(marker).read()) if os.path.exists(marker) else 0
    open(marker, "a").write("x")
    return seen == nsmoke


def gen51(cli, srv, drv, nbytes, fam, hint):
    import math
    bad = set(x for x in os.environ.get("STUB_BAD51", "").split(",") if x)
    nsmoke = int(os.environ.get("STUB_SMOKE_N51", "4"))
    delta_env = os.environ.get("RWM_DELTA")
    echo_cli = delta_env or "unset"
    if "deltaonce" in bad and _once51("delta", nsmoke):
        echo_cli = "0.123"
    n_t = 220
    if "shortonce" in bad and _once51("short", nsmoke):
        n_t = 40
    if delta_env is None:
        beta = 1.0 if hint == "bulk" else 0.0
    else:
        beta = min(1.0, max(0.0, math.log10(0.5 / float(delta_env)) / 2.0))
    rr = 0.0 if beta >= 1.0 else round(0.2 * (1 - beta), 3)
    if "hbrr" in bad and beta >= 1.0:
        rr = 0.2
    pay = 1182 if "wrongsize" in bad else PAYLOAD51[hint]
    nb = int(nbytes)
    total = int(nb / pay)
    rl = run_lines(fam, False, n=n_t)
    out, k = [], 0
    k_on = min(n_t, 200)
    legs = 1 if fam in ("c1s", "c2") else 2
    for ln in rl:
        if not ln.startswith("[DIAG]"):
            out.append(ln)
            continue
        k += 1
        on = k <= k_on
        src_cum = int(total * min(k, k_on) / k_on)
        rate = int(total / k_on * 4) if on else 0
        t = 0.25 * k
        cod = int(0.01 * src_cum)
        hand = 1000 + 100 * k + src_cum
        d = diag48(t, [(1000 + 100 * k, 125000, 20, 3) for _ in range(legs)], [(hand, hand - (int(hand * 0.1) if "overdrive" in bad else 0)) for _ in range(legs)])
        d = d.replace("paused=42%", "paused=0%").replace("cwnd=647 infl=457", "cwnd=647 infl=%d" % (700 if "overdrive" in bad else 457))
        if "overdrive" in bad:
            d = re.sub(r"dgev(\d)=3", lambda m: "dgev%s=%d" % (m.group(1), int(hand * 0.1)), d)
        d = d + (f" src={rate}sym/s cod=0sym/s cum={src_cum}/{cod}/{max(0, src_cum - 100)} retx={100 * k} mpr[rep={10 * k} seqs={500 * k} fired={100 * k} y={30 * k} r={70 * k} fast={30 * k} supp=0/0/0 taper={k // 10}] "
                 f"fp/on p0:{6 * k}/{4 * k} p1:{2 * k}/{4 * k}] wnd2=0/0")
        out.append(d)
        if k % 2 == 0:
            out.append(f"[SPAN] t={0.5 * (k // 2):.1f}s ack={max(0, src_cum - 100)} win=[{src_cum},{src_cum + 64}] wsize=200 a_star=Some(220) delta=Some(0.5) owed=0.10 rr={rr:.3f} debt=0.30 retx_buf=5")
    cli = [ln for ln in cli if ln and '"summary"' not in ln and '"run":' not in ln and "emission batching" not in ln]      # the fixture's own echo is the Bulk one: the carrier's replaces it
    cli = [re.sub(r"\sRWM_DELTA=\S+", "", ln) + f" RWM_DELTA={echo_cli}" if "[GATES]" in ln else ln for ln in cli]
    srv = [re.sub(r"\sRWM_DELTA=\S+", "", ln) + f" RWM_DELTA={delta_env or 'unset'}" if "[GATES]" in ln else ln for ln in srv if not ln.startswith(("[SUCC]", "[C8CONV-R]", "[RANK]"))]
    if hint != "bulk":
        cli = [ln.replace("hint=bulk", "hint=" + hint) if "[PIPE]" in ln else ln for ln in cli]
        srv = [ln.replace("hint=bulk", "hint=" + hint) if "[PIPE]" in ln else ln for ln in srv]
        drv = [re.sub(r"hint=bulk( |$)", r"hint=" + hint + r"\1", ln) if "--- RWM-C perf" in ln else ln for ln in drv]
    if "nobatch" not in bad:
        cli = cli + [INFO59 + BATCH51[hint]]
    secs = 54.0 + (0.3 if os.environ.get("SEED") == "7" else 0.0)      # arm-independent by design: the stub's rows carry no step, so a FULL run's T1 / T2 are PASS deterministically
    if n_t == 40:
        secs = 9.0
    drv = [ln for ln in drv if '"summary"' not in ln] + [summary54(secs, nb)]
    cli = cli + out + ['{"bytes":%s,"hint":"%s","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%s}' % (nbytes, hint, nb * 8 / secs / 1e6, secs), summary54(secs, nb)]
    return cli, srv, drv


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
    fam = {"c1": "c1d" if mode == "dual" else "c1s", "c2": "c8" if cb == "c3" else "c2", "c3": "c3", "c3j0": "c3"}[ca]
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
    elif plan == "59":
        # Status 59, the evicting seat: 55 s of [DIAG] (source until 50 s) with the known per-tick deltas above, a [SPAN] line every 0.5 s, the Realtime echo and the shed echo on the right ends,
        # a driver summary (or a DNF). The row is armed unless STUB_BAD59 says otherwise.
        bad = set(x for x in os.environ.get("STUB_BAD59", "").split(",") if x)
        armed = "noshed" not in bad
        if "shedonce" in bad:
            # the smoke row is invocation 0 (armed), the first scored slot invocation 1 (NOT armed), its replacement invocation 2 (armed)
            marker = os.path.join(TMP, "shed-count")
            seen = len(open(marker).read()) if os.path.exists(marker) else 0
            open(marker, "a").write("x")
            if seen == 1:
                armed = False
        rl = run_lines(fam, False, n=220)
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = diag59(ln, k, armed)
                if k % 2 == 0:
                    rl[i] = rl[i] + "\n" + span59(k // 2, armed, "norr" in bad)
        rl = "\n".join(rl).split("\n")
        cli = seat59(cli, armed, "cli") + rl
        srv = seat59(srv, armed, "srv")
        drv = [re.sub(r"hint=bulk( .*)$", r"hint=realtime\1 reliable=0", ln) if "--- RWM-C perf" in ln else ln for ln in drv]
        secs = 54.0 + (os.getpid() % 7) * 0.3
        drv = [ln for ln in drv if '"summary"' not in ln] + [summary54(secs, int(nbytes))]
        cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + ['{"bytes":%s,"hint":"realtime","mbps":%.3f,"proto":"rp-native","run":1,"seconds":%s}' % (nbytes, int(nbytes) * 8 / secs / 1e6, secs),
                                                                                    summary54(secs, int(nbytes))]
        if os.environ.get("STUB_DNF59") == os.environ.get("SEED"):
            dnf = '{"bytes":%s,"dnf":true,"hint":"realtime","proto":"rp-native","run":1}' % nbytes
            tail = '{"bytes":%s,"dnf":1,"hint":"realtime","proto":"rp-native","runs":1,"summary":true}' % nbytes
            cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + [dnf, tail]
            drv = [ln for ln in drv if '"summary"' not in ln] + [tail]
    elif plan in ("60a", "60b", "61a", "61b", "62r0", "62a"):
        cli, srv, drv = gen60(cli, srv, drv, nbytes, fam, _hint)
    elif plan in ("64", "65"):
        cli, srv, drv = gen64r(cli, srv, drv, fam, nbytes, base, cand)
    elif plan == "51e":
        cli, srv, drv = gen51e(cli, srv, drv, nbytes, fam, _hint)
    elif plan == "63":
        cli, srv, drv = gen63(cli, srv, drv, nbytes, fam, _hint)
    elif plan == "51":
        cli, srv, drv = gen51(cli, srv, drv, nbytes, fam, _hint)
    else:
        cli = cli + run_lines(fam, cand and os.environ.get("RWM_QUIC_CC") == "bbr_rs")
    if plan == "41d" and os.environ.get("LD_WITNESS64") == "1":
        cli, srv, drv = gen64g(cli, srv, drv, nbytes, cand)         # plan 64's stage G = plan 41d + the F2 row witnesses
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
