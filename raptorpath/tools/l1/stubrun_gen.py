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
    elif plan == "52":
        rl = run_lines(fam, False, n=20)         # 5 s of [DIAG] (the fixture's run is 5.09 s): source to 4.0 s, a tail after it
        k = 0
        for i, ln in enumerate(rl):
            if ln.startswith("[DIAG]"):
                k += 1
                rl[i] = ln + extras52(k)
        cli = gates52(cli) + rl + [HOLD52.format(p=0), HOLD52.format(p=1)]
        srv = gates52(srv) + [RX52]
    else:
        cli = cli + run_lines(fam, cand and os.environ.get("RWM_QUIC_CC") == "bbr_rs")
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
