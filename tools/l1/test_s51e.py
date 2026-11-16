#!/usr/bin/env python3
"""Tests for status 51 block E (plan 51e): tools/l1/plan51e.py (tables, run spec, calibration), s51e_score.py (row parsing, the pacing and gauge witnesses, the classes, the outcome, K4 steady windows) on synthetic
logs with KNOWN values. The launch paths run in test_stubrun.py; the shell wiring is test_harness51.py. Registered absolutes (s51-evict-design.md section 4): PL 20 748 000 B at 2.7664 Mbit = 700 sym/s, PM
262 500 000 B at 35 Mbit = 8 856 sym/s, U 600 MB unpaced.
Run: python3 test_s51e.py"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_ONLY", "LD_SHA", "LD_BASE_SHA", "LD_ROW_BYTES"):
    os.environ.pop(_k, None)
import plan51e  # noqa: E402
import s51e_score as sc  # noqa: E402
import s60_score  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


# ═══ 1. the registered load points and tables ═══
check("the PL point derives from ONE number (700 sym/s): lambda 2.7664 Mbit, 20 748 000 bytes, token '2.7664', 75 s", (plan51e.PL_RATE_SYM, plan51e.lam_mbit("PL"), plan51e.arm_bytes("c2-100", "PL"), plan51e.rate_token("PL"), plan51e.arm_timeout("c2-100", "PL"))
      == (700, 2.7664, 20_748_000, "2.7664", 75))
check("PM: 35 Mbit = 8 856 sym/s (rounded down), 262 500 000 bytes, token '35', 75 s; U: unpaced, 600 MB, 30 s, no rate token",
      (plan51e.rate_sym("PM"), plan51e.arm_bytes("c2-100", "PM"), plan51e.rate_token("PM"), plan51e.arm_timeout("c2-100", "PM"), plan51e.arm_bytes("c2-100", "U"), plan51e.arm_timeout("c2-100", "U"), plan51e.rate_token("U"), plan51e.lam_mbit("U"))
      == (8856, 262_500_000, "35", 75, 600_000_000, 30, None, None))
check("armspec env: RWM_SLAW first, the evicting seat, the paced rate on PL / PM only; every arm is Realtime",
      plan51e.arm_env("c2-100", "PL") == ["RWM_SLAW=1", "RWM_C_RELIABLE=0", "RWM_PERF_RATE_MBIT=2.7664"] and plan51e.arm_env("c2-100", "PM") == ["RWM_SLAW=1", "RWM_C_RELIABLE=0", "RWM_PERF_RATE_MBIT=35"]
      and plan51e.arm_env("c2-100", "U") == ["RWM_SLAW=1", "RWM_C_RELIABLE=0"] and all(plan51e.arm_hint(a) == "realtime" for a in plan51e.ARMS))
T = plan51e.plan("51e")
check("the table: 12 slots (PL 5, PM 5, U 2), blocked by replicate, PL / PM alternating (odd replicates PL first), U in the first two replicates after them, seeds 42 / 7 / 42 / 7 / 42",
      len(T) == 12 and [(r[2], r[3], r[4]) for r in T] == [("PL", "42", 1), ("PM", "42", 1), ("U", "42", 1), ("PM", "7", 2), ("PL", "7", 2), ("U", "7", 2), ("PL", "42", 3), ("PM", "42", 3), ("PM", "7", 4), ("PL", "7", 4), ("PL", "42", 5), ("PM", "42", 5)], T)
check("smoke = one PM row; the rehearsal = PL, PM, U one each (seed 42, rep 1); LD_ONLY=PL filters the rehearsal to the PL-only recheck row",
      plan51e.plan("51e", smoke=True) == [("c2-100", "def", "PM", "42", 1)] and [r[2] for r in plan51e.plan("51e", rehearsal=True)] == ["PL", "PM", "U"])
os.environ["LD_ONLY"] = "PL"
check("LD_ONLY=PL: the recheck table is the single PL rehearsal row", plan51e.plan("51e", rehearsal=True) == [("c2-100", "def", "PL", "42", 1)])
del os.environ["LD_ONLY"]
check("armspec refuses an unknown arm (rc 1)", plan51e.main(["armspec", "c2-100", "PX"]) == 1 and plan51e.main(["armspec", "c8-100", "PL"]) == 1)
cal = plan51e.calibrate(12.0)
check("the calibration rule (section 4), worked: median A* 12 -> x-hat 11.5 -> 700 x 4.5 / 11.5 = 273.9 -> 270 sym/s, lambda 1.06704 Mbit, 8 002 800 bytes", cal == {"x_hat": 11.5, "rate_sym": 270, "lam_mbit": 1.06704, "bytes": 8_002_800}, cal)
check("... A* 6.5 -> x-hat 6 -> 525 -> rounds half up to 530 sym/s; A* 5 -> x-hat 4.5 -> 700 (unchanged); A* None / <= 0.5 -> uncomputable",
      plan51e.calibrate(6.5)["rate_sym"] == 530 and plan51e.calibrate(5.0)["rate_sym"] == 700 and plan51e.calibrate(None) is None and plan51e.calibrate(0.5) is None)
plan51e.PL_RATE_SYM = 270
check("the amendment is a ONE-LINE edit of PL_RATE_SYM: lambda, bytes and the rate token follow (270 sym/s -> 1.06704 Mbit, 8 002 800 B, '1.06704')",
      (plan51e.lam_mbit("PL"), plan51e.arm_bytes("c2-100", "PL"), plan51e.rate_token("PL"), plan51e.rate_sym("PL")) == (1.06704, 8_002_800, "1.06704", 270))
plan51e.PL_RATE_SYM = 700
plan51e.PL_DROPPED = True
check("PL dropped by amendment: 7 slots (PM 5 + U 2), no PL in the active arms, the rehearsal has no PL row", len(plan51e.plan("51e")) == 7 and "PL" not in plan51e.active_arms() and [r[2] for r in plan51e.plan("51e", rehearsal=True)] == ["PM", "U"])
plan51e.PL_DROPPED = False
check("budgets: the battery's worst case 2 730 s (every slot twice + build + smoke + tests), the rehearsal 1 500 s", plan51e.BUDGET_S == {"51e": 2730, "51er": 1500} and plan51e.ROWS_ONCE_S == 990)

# ═══ 2. row parsing on a synthetic log with KNOWN values ═══
INFO = "\x1b[2m2026-11-16T00:00:00Z\x1b[0m INFO "


def diag(t, src, cwnd=647, infl=30, paused=0, hand=1000, tx=1000, cum=(0, 0, 0), dgev=0):
    return (f"[DIAG] t={t:.2f}s win=100/200 paused={paused}% good=1.0Mbit cwnd={cwnd} infl={infl} np=1 np_act=1 rtt=15.8ms dgq0[hand={hand} tx={tx} full=0 err=0 sp=1] p0:infl=1/sinfl=0 qcwnd=125000 pl=0.0 "
            f"| ANCHOR plu=0.0006 plc=0.0013 dgev0={dgev} src={src}sym/s cod=0sym/s cum={cum[0]}/{cum[1]}/{cum[2]} wait[a n=100 us=1] wake[ack={int(t * 20)} cmd=1]")


def wstore(t, ab, az, an, rb, rn, r0, wr, fr, fec_r, fec_n, astar=5.0):
    wrs = "-/-" if wr is None else f"{wr:.6f}/{fr:.6f}"
    fec = "-/-/-/-/0" if fec_n == 0 and fec_r is None else f"0.010000/1.2000/{fec_r:.5f}/{astar:.2f}/{fec_n}"
    return (f"[WSTORE] t={t:.2f} k={int(t * 4)} tw=250.0 n=200 W=200 adv=100/100/100/100 ackw=20 fec={fec} spc={ab}/{az}/{an}/{rb}/{rn}/{r0} wr={wrs} dbt_acc=10.00 dbt_em=5 dbt_share=0.5")


def span(t, a_star=5, delta=2, rr=0.25, wsize=25):
    return f"[SPAN] t={t:.1f}s ack=100 win=[1,2] wsize={wsize} a_star=Some({a_star}) delta=Some({delta}) owed=0.1 rr={rr:.3f} debt=0.3 retx_buf=5 shed=3/1 bud=0.0500 D=40ms"


def build(rate_tok="2.7664", paced=True, n=80, src=700, ab=0, az=0, bad=None, gate_cli=None, windows=None, pre_echo=True, pace_line=True, a_star=5, last_src_t=None):
    """A client log: the start echo and [GATES] BEFORE the warm-up marker, then n ticks (src on until last_src_t), a [SPAN] every 0.5 s, one [WSTORE] per tick, the pace line after the feed."""
    lam = rate_tok
    cli = [INFO + "RWM-C pre" + (f" perf offered-load pacing ACTIVE (RWM_PERF_RATE_MBIT={lam})" if (paced and pre_echo) else ""),
           f"[GATES] RWM_DIAG=1 RWM_SLAW=1 RWM_PERF_RATE_MBIT={gate_cli if gate_cli else (rate_tok if paced else 'unset')}", '{"seconds":0.01,"warmup":true}']
    last = last_src_t if last_src_t is not None else n * 0.25
    s = 0
    for k in range(1, n + 1):
        t = 0.25 * k
        on = t <= last
        s += src // 4 if on else 0
        cli.append(diag(t, src if on else 0, cum=(s, int(0.2 * s), max(0, s - 100))))
        if k % 2 == 0:
            cli.append(span(0.5 * (k // 2), a_star=a_star))
        an = src // 4 if on else 0
        if windows is not None:
            cli.append(windows(k, t, an))
        elif on:
            cli.append(wstore(t, ab, az, an, ab, an, 0, 0.25, 0.25, 0.25, an, a_star))
        else:
            cli.append(wstore(t, 0, 0, 0, 0, 0, 0, None, None, None, 0))
    if paced and pace_line:
        cli.append(json.dumps({"pace": True, "obj": 1, "rate_mbit": float(lam), "chunks": 42000, "feed_s": 60.0, "achieved_mbit": float(lam), "late": 0, "late_max_ms": 0.3}))
    return cli


def metrics(cli, arm="PL", srv=None):
    return sc.row_metrics(cli, srv or [], [], 20_748_000, "c2-100", arm, None)


t = metrics(build(n=240))        # 60 s at 700 sym/s
check("row_metrics: median LATE src 700, the last sending tick 60 s, 240 windows, the start echo and both [GATES] tokens read on the UNCUT log (they precede the warm-up marker)",
      t["src_med_late"] == 700 and t["last_src_t"] == 60.0 and len(t["gw"]["t"]) == 240 and t["echo_rate"] == ["2.7664"] and t["gate_rate"] == {"cli": "2.7664", "srv": None} and len(t["pace"]) == 1, (t["src_med_late"], t["last_src_t"], t["echo_rate"], t["gate_rate"]))
check("a healthy PL row: no pacing problem and no gauge problem", sc.pace_problems("PL", t) == [] and sc.gauge_problems(t) == [], (sc.pace_problems("PL", t), sc.gauge_problems(t)))
check("tau_x is n/a on PL (no tick has src > 1 000 sym/s); the eviction share (hand - tx)/hand = 0; the spc fields parse (an = 175 per window, wr = fr = 0.25)",
      t["tau_x"] is None and t["n_tau"] == 0 and t["evict_share"] == 0.0 and t["gw"]["an"][0] == 175 and t["gw"]["wr"][0] == 0.25 and t["gw"]["fr"][0] == 0.25)
g = sc.gauge_sums([sc.RV({"cell": "c2-100", "arm": "PL", "seed": "42", "rep": 1, "status": "LIVE", "t51e": t})])
check("PL gauge: phi = zeta = pi = 0 and the deficit 0 over its LATE windows with accruals (t >= 5 s: 221 windows, 38 675 accruals)", (g["phi"], g["zeta"], g["pi"], g["deficit"], g["windows"], g["an"]) == (0.0, 0.0, 0.0, 0.0, 221, 38675.0), g)

# ═══ 3. the pacing witness (section 5.2) ═══
def pp(**kw):
    cli = build(n=240, **{k: v for k, v in kw.items() if k in ("rate_tok", "paced", "gate_cli", "pre_echo", "pace_line", "last_src_t", "src")})
    arm = kw.get("arm", "PL")
    return sc.pace_problems(arm, metrics(cli, arm))


def has(ps, pref):
    return any(x.startswith(pref) for x in ps)


check("numeric comparison: '2.76640' and '2.7664' are the same value", pp(rate_tok="2.76640") == [])
check("a missing start echo is pace-echo-missing", has(pp(pre_echo=False), "pace-echo-missing"))
check("a missing pace line is pace-line-missing", has(pp(pace_line=False), "pace-line-missing"))
check("[GATES] reading another rate on the client end is pace-gate-cli", has(pp(gate_cli="9"), "pace-gate-cli"))
check("median LATE src 6 % off rate_sym is pace-src-median (the +-5 % band); 4 % off passes", has(pp(src=742), "pace-src-median") and not has(pp(src=728), "pace-src-median"))
check("a sending tick that stops at 56 s (< T_e - 2 s) is pace-last-tick; 58 s passes", has(pp(last_src_t=56.0), "pace-last-tick") and not has(pp(last_src_t=58.0), "pace-last-tick"))
bad_ach = [ln if '"pace"' not in ln else json.dumps({"pace": True, "obj": 1, "rate_mbit": 2.7664, "chunks": 42000, "feed_s": 60.0, "achieved_mbit": 2.7664 * 1.03, "late": 0, "late_max_ms": 0.3}) for ln in build(n=240)]
bad_late = [ln if '"pace"' not in ln else json.dumps({"pace": True, "obj": 1, "rate_mbit": 2.7664, "chunks": 1000, "feed_s": 60.0, "achieved_mbit": 2.7664, "late": 11, "late_max_ms": 9.0}) for ln in build(n=240)]
check("achieved_mbit 3 % off lambda is pace-achieved (+-2 %); late > 1 % of the chunks is pace-late", has(sc.pace_problems("PL", metrics(bad_ach)), "pace-achieved") and has(sc.pace_problems("PL", metrics(bad_late)), "pace-late"))
u = metrics(build(paced=False, n=52, src=100000, ab=0), "U")
check("U (unpaced): [GATES] reads unset, no echo, no pace line -> no pacing problem; an echo or a pace line on U is pace-echo-on-unpaced / pace-line-on-unpaced, a rate in [GATES] is pace-gate-cli",
      sc.pace_problems("U", u) == [] and has(sc.pace_problems("U", metrics(build(paced=True, n=52, src=100000), "U")), "pace-echo-on-unpaced") and has(sc.pace_problems("U", metrics(build(paced=True, n=52, src=100000), "U")), "pace-gate-cli")
      and has(sc.pace_problems("U", metrics(build(paced=True, n=52, src=100000, pre_echo=False), "U")), "pace-line-on-unpaced"))

# ═══ 4. the gauge witnesses (section 6.2), absolute ═══
def win(an=100, ab=0, az=0, rb=0, rn=None, r0=0, wr=0.25, fr=0.25, fec_r=0.25, fec_n=None):
    rn = an - r0 if rn is None else rn
    return lambda k, t, _an: wstore(t, ab, az, an, rb, rn, r0, wr, fr, fec_r, an if fec_n is None else fec_n)


def gp_of(w):
    return sc.gauge_problems(metrics(build(n=40, windows=w)))


check("healthy windows pass; 100 accruals with rn + r0 = an", gp_of(win()) == [])
check("an != the fec= n of the same window is spc-an-ne-fecn", has(gp_of(win(fec_n=101)), "spc-an-ne-fecn"))
check("rn + r0 != an is spc-rn+r0-ne-an", has(gp_of(win(rn=99)), "spc-rn+r0-ne-an"))
check("grant = flat_rate: (sum fr/rn) rn / (rn + r0) = 0.25 x 90/100 = 0.225 against fec r 0.225 passes; 0.25 against it is spc-grant-ne-flat",
      gp_of(win(r0=10, fr=0.25, fec_r=0.225)) == [] and has(gp_of(win(r0=10, fr=0.25, fec_r=0.25)), "spc-grant-ne-flat"))
check("the tolerance is 1.5e-5: 1.4e-5 off passes, 2e-5 off fails", gp_of(win(fec_r=0.25 + 1.4e-5)) == [] and has(gp_of(win(fec_r=0.25 + 2e-5)), "spc-grant-ne-flat"))
check("fec r must be 0 where az + r0 = an (every accrual a zero grant): r 0.25 there is spc-r-nonzero-on-zero-grant; r 0 passes",
      gp_of(win(ab=100, az=100, rb=100, fr=0.0, wr=0.25, fec_r=0.0)) == [] and has(gp_of(win(ab=100, az=100, rb=100, fr=0.0, wr=0.25, fec_r=0.25)), "spc-r-nonzero-on-zero-grant"))
check("az <= ab <= an and rb <= rn: az 5 with ab 0 is spc-order", has(gp_of(win(ab=0, az=5)), "spc-order"))
check("no [WSTORE] window after the warm-up is spc-none (the gauge was not printed)", has(sc.gauge_problems(metrics([ln for ln in build(n=40) if not ln.startswith("[WSTORE]")])), "spc-none"))
check("an empty window (spc=0/0/0/0/0/0 wr=-/- fec=-/-/-/-/0) is not a problem", sc.gauge_problems(metrics(build(n=40, last_src_t=5.0))) == [])
check("a window without the spc= field is spc-missing", has(sc.gauge_problems(metrics([ln.replace(" spc=0/0/100/0/100/0", "") if ln.startswith("[WSTORE]") else ln for ln in build(n=40)])), "spc-missing") or
      has(sc.gauge_problems(metrics([ln.split(" spc=")[0] + " dbt_acc=1" if ln.startswith("[WSTORE]") else ln for ln in build(n=40)])), "spc-missing"))

# ═══ 5. classes, the outcome (section 6.3) and the rate-spare-cwnd reading (section 7) ═══
check("class boundaries: phi 0.0499 SLACK, 0.05 PARTIAL, 0.95 PARTIAL, 0.9501 PINNED", [sc.klass(x) for x in (0.0499, 0.05, 0.95, 0.9501)] == ["SLACK", "PARTIAL", "PARTIAL", "PINNED"] and sc.klass(None) is None)


def rv_of(arm, phi_bind=0.0, zeta=0.0, an=100, nwin=40, tau=None, seed="42", rep=1):
    """A row whose windows carry the given bind / zero shares: ab = phi an, az = zeta an."""
    g = {k: [] for k in ("t", "tw", "ab", "az", "an", "rb", "rn", "r0", "wr", "fr", "fec_n", "fec_r")}
    for i in range(nwin):
        for k, v in (("t", 5.25 + 0.25 * i), ("tw", 250.0), ("ab", phi_bind * an), ("az", zeta * an), ("an", an), ("rb", phi_bind * an), ("rn", an), ("r0", 0), ("wr", 0.2), ("fr", 0.2 - 0.19 * phi_bind), ("fec_n", an), ("fec_r", 0.1)):
            g[k].append(v)
    return sc.RV({"cell": "c2-100", "arm": arm, "seed": seed, "rep": rep, "status": "LIVE", "dnf": True, "t51e": {"gw": g, "tau_x": tau}})


def by(**kw):
    out = {"PL": [rv_of("PL")], "PM": [rv_of("PM")], "U": [rv_of("U", 1.0, 0.9, tau=1.0), rv_of("U", 1.0, 0.9, tau=1.0, seed="7", rep=2)]}
    out.update(kw)
    return out


check("a PARTIAL row: phi 0.3, zeta 0.1, pi 0.2, deficit (wr - fr) = 0.19 x 0.3 = 0.057",
      (lambda g: abs(g["phi"] - 0.3) < 1e-12 and abs(g["zeta"] - 0.1) < 1e-12 and abs(g["pi"] - 0.2) < 1e-12 and abs(g["deficit"] - 0.057) < 1e-12)(sc.gauge_sums([rv_of("PM", 0.3, 0.1)])))
check("outcome 4: PL and PM SLACK and U passes the control -> DEGENERATE-AS-WIRED (the registered prediction)", sc.outcome(by())[0] == "DEGENERATE-AS-WIRED", sc.outcome(by()))
check("outcome 3: PL or PM PARTIAL -> LIVE-RANGE; outcome 2: PL or PM PINNED -> PINNED-PACED (the first match wins: PINNED before PARTIAL)",
      sc.outcome(by(PL=[rv_of("PL", 0.3, 0.1)]))[0] == "LIVE-RANGE" and sc.outcome(by(PM=[rv_of("PM", 0.99, 0.5)]))[0] == "PINNED-PACED" and sc.outcome(by(PL=[rv_of("PL", 0.3, 0.1)], PM=[rv_of("PM", 0.99, 0.5)]))[0] == "PINNED-PACED")
check("outcome 1: U's control failing (zeta < 0.5 on both U rows) or an arm with no live row (a gauge VOID emptied it) -> NO VERDICT, before any other class",
      sc.outcome(by(U=[rv_of("U", 1.0, 0.3, tau=1.0), rv_of("U", 1.0, 0.4, tau=1.0, seed="7", rep=2)]))[0] == "NO VERDICT" and sc.outcome(by(PL=[]))[0] == "NO VERDICT"
      and sc.outcome(by(PM=[rv_of("PM", 0.99, 0.5)], U=[rv_of("U", 1.0, 0.3)]))[0] == "NO VERDICT" and sc.outcome(by(U=[rv_of("U", 1.0, 0.3), rv_of("U", 1.0, 0.9, seed="7", rep=2)]))[0] == "DEGENERATE-AS-WIRED")
plan51e.PL_DROPPED = True
o = sc.outcome({"PM": [rv_of("PM")], "U": by()["U"]})
plan51e.PL_DROPPED = False
check("with PL dropped by amendment the outcome reads PM alone and says so; K4 then has no scoreable arm (the PM steady count decides)", o[0] == "DEGENERATE-AS-WIRED" and "PL dropped" in o[1], o)
check("rate-spare-cwnd: tau_x >= 0.5 on both U rows -> NOT-ENFORCED; 0 on both -> CONTRADICTED; anything else (0.2 / 0.9, or one 0.5) -> MIXED; unread -> UNREAD",
      sc.tau_reading([rv_of("U", tau=1.0), rv_of("U", tau=0.5)])[0] == "NOT-ENFORCED" and sc.tau_reading([rv_of("U", tau=0.0), rv_of("U", tau=0.0)])[0] == "CONTRADICTED"
      and sc.tau_reading([rv_of("U", tau=0.2), rv_of("U", tau=0.9)])[0] == "MIXED" and sc.tau_reading([rv_of("U", tau=0.5), rv_of("U", tau=0.0)])[0] == "MIXED" and sc.tau_reading([rv_of("U", tau=None)])[0] == "UNREAD")

# ═══ 6. tau_x: the three conditions; a paused tick or src <= 1 000 sym/s is not a sending tick ═══
cli = ['{"seconds":0.01,"warmup":true}', diag(1.0, 5000, cwnd=600, infl=700), diag(1.25, 5000, cwnd=600, infl=700, paused=42), diag(1.5, 500, cwnd=600, infl=700), diag(1.75, 5000, cwnd=600, infl=100), diag(2.0, 5000, cwnd=10000, infl=10000)]
tm = metrics(cli, "U")
check("tau_x counts ticks with infl >= cwnd among ticks with src > 1 000 sym/s and paused = 0 % (1 000: yes; 1 250: paused; 1 500: slow; 1 750: infl < cwnd; 2 000: yes): 2 of 3 sending ticks; the 3.2 share (no paused filter) 3 of 4; the MAX_CWND share 1 of 5",
      abs(tm["tau_x"] - 2 / 3) < 1e-12 and abs(tm["tau_sat"] - 3 / 4) < 1e-12 and abs(tm["maxcwnd_share"] - 1 / 5) < 1e-12 and tm["n_tau"] == 3, (tm["tau_x"], tm["tau_sat"], tm["maxcwnd_share"]))
cli = ['{"seconds":0.01,"warmup":true}', diag(1.0, 700, hand=1000, tx=600), diag(2.0, 700, hand=2000, tx=1000, dgev=50)]
tm = metrics(cli, "U")
check("the eviction share is (hand - tx) / hand at the LAST tick (1 000 / 2 000 = 0.5, not the first tick's 0.4)", tm["evict_share"] == 0.5, tm["evict_share"])
check("the warm-up cut: ticks before the LAST marker are dropped (a tick at 1 s before the marker does not count)", metrics([diag(1.0, 5000), '{"seconds":0.01,"warmup":true}', diag(2.0, 5000)], "U")["n_ticks"] == 1)

# ═══ 7. K4 steady windows with the ab = 0 rule (section 6.3) ═══
def k4rv(ab_at=None, n=60):
    """60 windows (t = 5.25 ..) with a constant a_star / delta on every [SPAN] line; ab = 1 on the window index `ab_at`."""
    wt = [5.0 + 0.25 * i for i in range(n)]
    ev = {"cum": {"t": wt, "src": [100] * n, "ack": [i * 10 for i in range(n)], "wack": [i for i in range(n)]},
          "wstore": {"t": wt, "tw": [250.0] * n, "adv": [100] * n, "ackw": [20] * n, "dbt_acc": [10.0] * n, "dbt_em": [5.0] * n, "dbt_share": [0.5] * n, "eps": [0.01] * n, "sigma2": [1.2] * n, "r": [0.25] * n,
                     "astar": [5.0] * n, "fecn": [100] * n, "ab": [1 if i == ab_at else 0 for i in range(n)]},
          "span": {"t": [5.0 + 0.5 * j for j in range(n // 2 + 1)], "a_star": [5.0] * (n // 2 + 1), "delta": [2.0] * (n // 2 + 1)}, "mbar59": {}}
    return sc.RV({"cell": "c2-100", "arm": "PL", "seed": "42", "rep": 1, "status": "LIVE", "t51e": {"evict": ev}})


base = s60_score.evict_stats(k4rv())["n_steady"]
check("without the ab rule the s60 steady count is unchanged (default off); with ab = 0 everywhere the rule changes nothing", s60_score.evict_stats(k4rv(ab_at=30))["n_steady"] == base and k4rv().steady()["n_steady"] == base, (base, k4rv().steady()["n_steady"]))
check("with the rule, one window with ab = 1 removes itself and both neighbours (3 steady windows)", k4rv(ab_at=30).steady()["n_steady"] == base - 3, (base, k4rv(ab_at=30).steady()["n_steady"]))
check("a missing ab column (None) is never steady under the rule", sc.RV({"cell": "c2-100", "arm": "PL", "seed": "42", "rep": 1, "status": "LIVE", "t51e": {"evict": {**k4rv().t["evict"], "wstore": {k: v for k, v in k4rv().t["evict"]["wstore"].items() if k != "ab"}}}}).steady()["n_steady"] == 0)
fires, why, calib = sc.pl_calibration(k4rv(ab_at=None))
check("PL calibration reading: A* undefined (no mbar59 median) fires with the A* reason; the steady count is read with the full rule", fires and any("A*" in w for w in why), (fires, why))

# ═══ 8. the smoke / rehearsal on a ledger of synthetic rows ═══
def ledger_row(arm, t51e, seed="42", rep=1, problems=(), dnf=True, status="LIVE"):
    return {"cell": "c2-100", "arm": arm, "seed": seed, "rep": rep, "status": status, "problems": list(problems), "dnf": dnf, "t51e": t51e, "mbps": None}


def write(rows):
    import tempfile
    d = tempfile.mkdtemp()
    p = os.path.join(d, "ld.log")
    with open(p, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    return p


def smoke_out(rows, which):
    out = []
    rc = sc.smoke(write(rows), which, out.append)
    return rc, "\n".join(out)


ok_rows = [ledger_row("PM", metrics(build(rate_tok="35", n=250, src=8856), "PM"))]
rc, txt = smoke_out(ok_rows, "smoke")
check("the smoke (one PM row): PASS on (b)-(g) for a healthy synthetic PM row, no U / PL criterion claims a PASS over an empty set (NOT-RUN)", rc == 0 and "SMOKE-PASS" in txt and "NOT-RUN (g) U's positive control" in txt, txt[-800:])
bad_t = metrics(build(rate_tok="35", n=250, src=8856, pre_echo=False), "PM")
rc, txt = smoke_out([ledger_row("PM", bad_t)], "smoke")
check("a PM row without its pacing echo FAILS criterion (c)", rc == 1 and "FAIL (c) the pacing witness" in txt, txt[-600:])
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
