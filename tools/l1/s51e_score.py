#!/usr/bin/env python3
"""Status 51, block E (plan 51e): the row readouts, witnesses, smoke / rehearsal check and score of the evicting-seat block with the spare-clamp gauge.
docs/derivations/s51-evict-design.md sections 5.2 (pacing witness), 6 (the gauge, its witnesses and outcome classes), 7 (rate-spare-cwnd), 8 (readouts, predictions), 10 (go/no-go).

    s51e_score.py smoke51e <smoke|rehearsal|recheck> <ledger>     PASS / FAIL per criterion; rc 1 on a FAIL. smoke = the one PM row; rehearsal = PL, PM, U (section 10 (a)-(h)); recheck = the PL-only row
                                                                  of the calibration rule ((h) gating). Ends SMOKE-PASS | SMOKE-FAIL.
    s51e_score.py score51e <ledger> [...]                          every registered readout, VERDICT51E and one LABEL51E line per label

Reuses (not copies): s60_score._evict / evict_stats / evict_line (m-bar, W1, K4 inputs; `rule_ab` is the one default-preserving extension), mbar59_score (the [DIAG] / [SPAN] readouts), plan60.row_problems
(the evicting seat's liveness), pool56_score.counting (the replacement rule's counting row), plan51e (the tables).

FORMATS (the engine branch s51e-tree owns them; read at github/s51e-tree 77d53d9c: perf.rs Pacer::line / the start echo, gates.rs, slaw.rs and its testdata/slaw_sample.log; the parser was written to the spec first):
  [WSTORE] ... fec=<eps>/<sigma2>/<r>/<A*>/<n> spc=<ab>/<az>/<an>/<rb>/<rn>/<r0> wr=<sum wr/rn>/<sum fr/rn> ...     (spc integers, wr 6 decimals, `spc=0/0/0/0/0/0 wr=-/-` on an empty window)
  perf client start echo:   perf offered-load pacing ACTIVE (RWM_PERF_RATE_MBIT=<v>)            (searched in the client log and the driver output, uncut)
  per timed object, a JSON line on the client log: {"pace":true,"obj":<id>,"rate_mbit":<v>,"chunks":<n>,"feed_s":<s>,"achieved_mbit":<v>,"late":<k>,"late_max_ms":<m>}
      (the engine prints Mbit/s in both rate fields; rate_mbit is not checked, achieved_mbit is accepted in Mbit/s or bit/s within +-2 % of lambda, chunks and late are read as counts)
  [GATES] ... RWM_PERF_RATE_MBIT=<v>|unset                                                       (both ends; compared numerically; a paced row accepts `unset` on the server end)"""
import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import mbar59_score as m59  # noqa: E402
import plan51e  # noqa: E402
import plan60  # noqa: E402
import pool56_score as ps  # noqa: E402
import s60_score as s60  # noqa: E402
import slaw_parse as sp_  # noqa: E402

LATE_T = plan51e.LATE_T
STEADY_MIN = s60.STEADY_MIN                    # 30 K4-steady LATE windows (status 60 section 6)
MIN_LATE_WINDOWS = 120                         # section 10 (e)
EVICT_MAX_PACED, EVICT_MIN_U = 0.001, 0.1      # section 10 (f)
ZETA_VALID, ZETA_PRED = 0.5, 0.8               # section 6.2: validity threshold / prediction
SLACK_MAX, PINNED_MIN = 0.05, 0.95             # section 6.3
ASTAR_LO, ASTAR_HI = 3.0, 7.0                  # section 4 / 10 (h): the oracle's decidable window
GRANT_TOL = 1.5e-5                             # section 6.2: |(sum fr/rn) rn/(rn + r0) - fec r|
TAU_NOT_ENFORCED, TAU_VALID_MIN = 0.5, 0.0     # section 7
SRC_TAU_MIN = 1000.0                           # section 7 / 3.2: sending ticks have src > 1 000 sym/s
MAX_CWND = 10000                               # scheduler/path.rs PathState::MAX_CWND
WARM = '"warmup":true'
ECHO_PACE = re.compile(r"perf offered-load pacing ACTIVE \(RWM_PERF_RATE_MBIT=(\S+?)\)")
_PAUSED = re.compile(r" paused=(\d+)% ")
_CWI = re.compile(r" cwnd=(\d+) infl=(\d+) ")
_QCW = re.compile(r" qcwnd=(\d+) ")
_DGQ0 = re.compile(r" dgq0\[hand=(\d+) tx=(\d+)")
_CUM = re.compile(r" cum=(\d+)/(\d+)/(\d+)")
_WAKE = re.compile(r"wake\[[^\]]*? ack=(\d+)")
_SPAN = re.compile(r"\[SPAN\] t=([\d.]+)s .*? wsize=(\d+) .*? rr=([-+\d.eE]+) ")
# section 8.2: the registered predictions, printed beside each reading (a band, never a pass criterion except where section 10 says so)
PRED = {
    "PL": {"src": "700 +-5 % (the knob)", "last_tick": ">= 58 s", "astar": "4-11 before calibration; 3-7 after (>= 30 steady windows)", "delta": "1-3", "wire": "<= 5.2 Mbit",
           "rr": "> 0, magnitude unpredicted", "phi": "SLACK", "evict": "<= 0.001", "tau": "~0 (n/a: src <= 1000 sym/s)", "mbar": "1-2", "w1": "reported, no prediction"},
    "PM": {"src": "8856 +-5 % (the knob)", "last_tick": ">= 58 s", "astar": "49-133 (K4-UNSCOREABLE predicted)", "delta": "9-27", "wire": "50 typical, <= 61 Mbit",
           "rr": "0.15-0.32", "phi": "SLACK", "evict": "<= 0.001", "tau": "~0", "mbar": "reported (0.7-6.5 on reliable c2 MAIN rows)", "w1": "reported"},
    "U": {"src": "90-108 k sym/s", "last_tick": "~13 s", "rr": "0.000", "phi": "zeta >= 0.8 (validity >= 0.5)", "evict": ">= 0.5", "tau": ">= 0.5", "mbar": "not read", "w1": "not read"},
}


def med(v):
    return s60.med(v)


def _f(x, nd=3):
    return s60.f(x, nd)


def _share(num, den):
    return None if not den else num / den


# ───────────────────────────── row parsing (the VM side) ─────────────────────────────
def row_metrics(cli, srv, drv, nbytes=None, cell=plan51e.CELL, arm="PM", mbps=None):
    """The `t51e` of one row. One pass over the client log after the last warm-up marker; the pacing echo and the [GATES] tokens are read on the UNCUT logs (they precede the marker). A missing or garbled token reads
    None; never raises on log content."""
    cli, srv, drv = list(cli or []), list(srv or []), list(drv or [])
    echo = [x for x in (ECHO_PACE.search(ln) for ln in cli + drv) if x]
    out = {"v": 1, "cell": cell, "arm": arm, "gate_rate": {"cli": lc.gate_tok(cli, "RWM_PERF_RATE_MBIT"), "srv": lc.gate_tok(srv, "RWM_PERF_RATE_MBIT")},
           "echo_rate": [m.group(1) for m in echo]}
    ticks, wst, cumt, paces = [], [], [], []
    for ln in cli:
        if WARM in ln:                                   # the warm-up cut at the LAST marker (as s60_score.row_metrics)
            ticks, wst, cumt, paces = [], [], [], []
            continue
        if ln[:1] == "{" and '"pace"' in ln:
            try:
                d = json.loads(ln)
            except ValueError:
                continue
            if isinstance(d, dict) and d.get("pace") is True:
                paces.append(d)
        elif ln.startswith("[WSTORE]"):
            wst.append(ln)
        elif ln.startswith("[DIAG]"):
            try:
                tk = s60._tick(ln)
            except Exception:
                tk = None
            if not tk:
                continue
            pa, cw, qc, dg = _PAUSED.search(ln), _CWI.search(ln), _QCW.search(ln), _DGQ0.search(ln)
            ticks.append({"t": tk[0], "src": tk[1], "paused": int(pa.group(1)) if pa else None, "cwnd": int(cw.group(1)) if cw else None, "infl": int(cw.group(2)) if cw else None,
                          "qcwnd": int(qc.group(1)) if qc else None, "hand": int(dg.group(1)) if dg else None, "tx": int(dg.group(2)) if dg else None})
            m, mw = _CUM.search(ln), _WAKE.search(ln)
            cumt.append((tk[0], tk[1], int(m.group(3)) if m else None, int(mw.group(1)) if mw else None))
    out["pace"] = paces
    out["n_ticks"] = len(ticks)
    send = [k for k in ticks if (k["src"] or 0) > 0]
    sat = [k for k in ticks if (k["src"] or 0) > SRC_TAU_MIN and k["cwnd"] is not None and k["infl"] is not None]
    sending = [k for k in sat if k["paused"] == 0]
    out["n_send"], out["n_tau_sat"], out["n_tau"] = len(send), len(sat), len(sending)
    out["tau_sat"] = _share(sum(1 for k in sat if k["infl"] >= k["cwnd"]), len(sat))                 # section 3.2: infl >= cwnd among ticks with src > 1 000 sym/s
    out["tau_x"] = _share(sum(1 for k in sending if k["infl"] >= k["cwnd"]), len(sending))           # section 7: ... and paused = 0 %
    cw = [k for k in send if k["cwnd"] is not None]
    out["maxcwnd_share"] = _share(sum(1 for k in cw if k["cwnd"] == MAX_CWND), len(cw))
    out["qcwnd_ratio"] = med([k["qcwnd"] / (k["cwnd"] * 512.0) for k in send if k["qcwnd"] is not None and k["cwnd"]])
    last = [k for k in ticks if k["hand"]]
    out["evict_share"] = (last[-1]["hand"] - last[-1]["tx"]) / last[-1]["hand"] if last else None
    out["src_med_late"] = med([k["src"] for k in send if k["t"] >= LATE_T])
    out["last_src_t"] = max((k["t"] for k in send), default=None)
    out["tick_src"] = [[k["t"], k["src"] or 0] for k in ticks if LATE_T <= k["t"] <= plan51e.T_E]      # the offer-window ticks (LATE up to the end of emission): the per-window off-load reading
    # the gauge windows
    g = {k: [] for k in ("t", "tw", "ab", "az", "an", "rb", "rn", "r0", "wr", "fr", "fec_n", "fec_r")}
    ab_col = []
    for ln in wst:
        r = sp_.parse_line(ln)
        if not r or r[0] != "WSTORE":
            continue
        d = r[1]
        for k, key in (("t", "t"), ("tw", "tw"), ("ab", "spc.0"), ("az", "spc.1"), ("an", "spc.2"), ("rb", "spc.3"), ("rn", "spc.4"), ("r0", "spc.5"), ("wr", "wr.0"), ("fr", "wr.1"), ("fec_n", "fec.4"), ("fec_r", "fec.2")):
            g[k].append(d.get(key))
        ab_col.append(d.get("spc.0"))
    out["gw"] = g
    ev = s60._evict(cli, srv, drv, nbytes, cell, mbps, wst, cumt)
    ev["wstore"]["ab"] = ab_col                           # aligned with _evict's own window loop (same lines, same parser)
    out["evict"] = ev
    sp = [(float(m.group(1)), int(m.group(2)), float(m.group(3))) for m in (_SPAN.search(ln) for ln in cli) if m]
    late_sp = [s for s in sp if s[0] >= LATE_T]
    out["span"] = {"n": len(sp), "n_late": len(late_sp), "wsize": med([s[1] for s in late_sp]), "rr500": _share(sum(1 for s in late_sp if s[2] >= 0.4995), len(late_sp))}
    return out


# ───────────────────────────── witnesses ─────────────────────────────
def _num(x):
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


def _close(a, b, rel):
    return a is not None and b is not None and abs(a - b) <= rel * abs(b)


def pace_problems(arm, t):
    """Section 5.2's pacing witness (rule 1: prove the mechanism executes): a paced row needs both echoes, the median LATE src within +-5 % of rate_sym, the last sending tick at t >= T_e - 2 s, achieved_mbit within
    +-2 % of lambda and late_max_ms <= 250 (status 51 Amendment 4; `late` itself is only reported); an unpaced row (U) must read RWM_PERF_RATE_MBIT=unset, with no echo and no pace line. Any failure is a VOID (pace-*), re-run once."""
    p = []
    cli, srv = t["gate_rate"]["cli"], t["gate_rate"]["srv"]
    if arm not in plan51e.PACED:
        if cli != "unset":
            p.append(f"pace-gate-cli={cli} (want unset)")
        if srv not in (None, "unset"):
            p.append(f"pace-gate-srv={srv} (want unset)")
        if t["echo_rate"]:
            p.append("pace-echo-on-unpaced")
        if t["pace"]:
            p.append("pace-line-on-unpaced")
        return p
    lam, rs = plan51e.lam_mbit(arm), plan51e.rate_sym(arm)
    if not t["echo_rate"]:
        p.append("pace-echo-missing")
    elif any(not _close(_num(x), lam, 1e-6) for x in t["echo_rate"]):
        p.append(f"pace-echo={','.join(t['echo_rate'])} (want {lam})")
    if not _close(_num(cli), lam, 1e-6):
        p.append(f"pace-gate-cli={cli} (want {lam})")
    if srv not in (None, "unset") and not _close(_num(srv), lam, 1e-6):
        p.append(f"pace-gate-srv={srv} (want {lam} or unset)")
    if not t["pace"]:
        p.append("pace-line-missing")
    else:
        for d in t["pace"]:
            ach = _num(d.get("achieved_mbit"))
            # the spec writes the field as <8 x bytes / feed_s> (bits/s) under the name achieved_mbit: either unit is accepted (an assumed format, see the module doc)
            if not (_close(ach, lam, plan51e.PACE_ACH_TOL) or _close(ach, lam * 1e6, plan51e.PACE_ACH_TOL)):
                p.append(f"pace-achieved={d.get('achieved_mbit')} (want {lam} Mbit/s or {lam * 1e6:g} bit/s, +-{plan51e.PACE_ACH_TOL:.0%})")
            # status 51 Amendment 4: `late` / `late_max_ms` are ENQUEUE lateness (the pacer's offer into the 8192-chunk feed queue: did the harness offer the load on schedule) and are REPORTED (row_view); only a stall
            # longer than one reading window voids the row. It is a harness-fault detector and claims NO protection below 250 ms.
            lm = _num(d.get("late_max_ms"))
            if lm is None or lm > plan51e.PACE_STALL_MAX_MS:
                p.append(f"pace-stall=late_max {d.get('late_max_ms')} ms (want <= {plan51e.PACE_STALL_MAX_MS:g})")
    if t["src_med_late"] is None or not _close(t["src_med_late"], rs, plan51e.PRED_RATE_BAND):
        p.append(f"pace-src-median={t['src_med_late']} (want {rs} +-{plan51e.PRED_RATE_BAND:.0%})")
    if t["last_src_t"] is None or t["last_src_t"] < plan51e.T_E - plan51e.LAST_TICK_SLACK_S:
        p.append(f"pace-last-tick={t['last_src_t']} (want >= {plan51e.T_E - plan51e.LAST_TICK_SLACK_S:g} s)")
    return p


def gauge_problems(t):
    """Section 6.2's on-row witnesses, exact, on EVERY window of every E row; a mismatch VOIDs the gauge (spc-*, re-run once):
    an = fec= n; rn + r0 = an (a recompute per source symbol at this seat); grant = flat_rate: |(sum fr/rn) rn/(rn + r0) - fec r| <= 1.5e-5; fec r = 0 where az + r0 = an; plus the counts' order
    (az <= ab <= an, rb <= rn)."""
    g = t["gw"]
    n = len(g["t"])
    if n == 0:
        return ["spc-none (no [WSTORE] window after the warm-up)"]
    bad = {}

    def hit(key, i, msg):
        bad.setdefault(key, []).append(f"t={g['t'][i]}: {msg}")
    for i in range(n):
        ab, az, an, rb, rn, r0 = (g[k][i] for k in ("ab", "az", "an", "rb", "rn", "r0"))
        if None in (ab, az, an, rb, rn, r0):
            hit("spc-missing", i, "no spc= field")
            continue
        if not (0 <= az <= ab <= an and 0 <= rb <= rn):
            hit("spc-order", i, f"spc={ab:g}/{az:g}/{an:g}/{rb:g}/{rn:g}/{r0:g}")
        fn, fr_r = g["fec_n"][i], g["fec_r"][i]
        if fn is None and an == 0:
            fn = 0      # an empty window prints fec=-/-/-/-/0 (n = 0); a missing n there is not a fault
        if fn is None:
            hit("spc-fec-missing", i, "no fec= n")
            continue
        if an != fn:
            hit("spc-an-ne-fecn", i, f"an={an:g} fec n={fn:g}")
        if rn + r0 != an:
            hit("spc-rn+r0-ne-an", i, f"rn={rn:g} r0={r0:g} an={an:g}")
        if rn + r0 > 0 and fr_r is not None:
            fr = g["fr"][i]
            if rn > 0 and fr is None:
                hit("spc-fr-missing", i, "wr=-/- with rn > 0")
            else:
                exp = (fr or 0.0) * rn / (rn + r0)
                if abs(exp - fr_r) > GRANT_TOL:
                    hit("spc-grant-ne-flat", i, f"(sum fr/rn) rn/(rn+r0)={exp:.6f} fec r={fr_r}")
        if fr_r is not None and az + r0 == an and an > 0 and abs(fr_r) > GRANT_TOL:
            hit("spc-r-nonzero-on-zero-grant", i, f"az+r0=an={an:g} but fec r={fr_r}")
    return [f"{k} on {len(v)}/{n} windows (first {v[0]})" for k, v in sorted(bad.items())]


# ───────────────────────────── the scoring side ─────────────────────────────
class RV:
    """A ledger row with its `t51e`."""

    def __init__(self, r, order=0):
        self.r, self.t, self.order = r, (r.get("t51e") or {}), order
        self.cell, self.arm = r.get("cell"), r.get("arm")
        self.rep, self.seed = int(r.get("rep") or 0), str(r.get("seed"))
        self.live = r.get("status") == "LIVE" and bool(self.t)
        self.dnf = bool(r.get("dnf"))

    def windows(self):
        """Indices of the gauge windows the arm's class is read over: windows with accruals (an > 0); LATE (t >= 5 s) for the paced arms, every sending window for U (section 6.3)."""
        g = (self.t.get("gw") or {})
        return [i for i, (tt, an) in enumerate(zip(g.get("t", []), g.get("an", []))) if an and an > 0 and (self.arm == "U" or (tt is not None and tt >= LATE_T))]

    def offload(self):
        """The ENGINE-side reading (status 51 Amendment 4): of the offer-window ticks (LATE, t <= T_e) of a PACED row, the share whose src is off rate_sym by more than 5 % (the per-window version of gate (c)'s median src
        check; an empty tick, src = 0, is off). A reading, never a void: an engine falling behind the offered load is the thing under test. -> {"n", "off", "share", "off_t"}; U has no offered load."""
        rs = plan51e.rate_sym(self.arm)
        ts = self.t.get("tick_src") or []
        if rs is None or not ts:
            return {"n": len(ts), "off": 0, "share": None, "off_t": []}
        off = [t for t, s_ in ts if not _close(s_, rs, plan51e.OFFLOAD_BAND)]
        return {"n": len(ts), "off": len(off), "share": len(off) / len(ts), "off_t": off}

    def steady(self):
        """K4 steady windows, the full rule (section 6.3): n >= 1, one a_star and one delta over the window and both neighbours, ab = 0 in the window and both neighbours.
        REGISTERED (status 51 Amendment 4): a window that passes that rule but whose offer-window tick is OFF-LOAD (src off rate_sym by more than 5 %) may NOT be scored: it is excluded from n_steady (and from the K4 input
        means) and counted in n_offload_excluded, which is printed. A steady window can be off-load: the rule reads the engine's own n, a_star and delta, not the offered load. U has no offered load and excludes nothing."""
        off = self.offload()
        offs = off["off_t"]
        st = s60.evict_stats(self, rule_ab=True, skip=(lambda tw: any(abs(tw - o) <= 0.13 for o in offs)) if offs else None)
        st["n_offload_excluded"] = st.get("n_skipped", 0)
        return st


def gauge_sums(rvs):
    """Pooled over the rows' windows (RV.windows): -> dict(an, ab, az, rn, rb, r0, phi, zeta, pi, deficit, windows). None readouts where an = 0."""
    s = {k: 0.0 for k in ("an", "ab", "az", "rn", "rb", "r0", "wr_w", "fr_w")}
    n = 0
    for rv in rvs:
        g = rv.t.get("gw") or {}
        for i in rv.windows():
            n += 1
            for k in ("an", "ab", "az", "rn", "rb", "r0"):
                s[k] += g[k][i] or 0
            rn = g["rn"][i] or 0
            s["wr_w"] += (g["wr"][i] or 0.0) * rn
            s["fr_w"] += (g["fr"][i] or 0.0) * rn
    an = s["an"]
    s["windows"] = n
    s["phi"] = _share(s["ab"], an)
    s["zeta"] = _share(s["az"], an)
    s["pi"] = _share(s["ab"] - s["az"], an)
    s["deficit"] = _share(s["wr_w"] - s["fr_w"], s["rn"])
    return s


def klass(phi):
    """Section 6.3: SLACK phi < 0.05; PARTIAL 0.05 <= phi <= 0.95; PINNED phi > 0.95."""
    if phi is None:
        return None
    return "SLACK" if phi < SLACK_MAX else ("PINNED" if phi > PINNED_MIN else "PARTIAL")


def outcome(by_arm_rows):
    """Section 6.3, the first class that matches wins. by_arm_rows = {arm: [live RV]} (the counting, non-void rows). -> (name, why).
    1 NO VERDICT: U's positive control fails (zeta < 0.5 on every U row) or an active arm has no live row (a gauge-witness VOID emptied it); 2 PINNED-PACED: PL or PM is PINNED; 3 LIVE-RANGE: PL or PM is PARTIAL;
    4 DEGENERATE-AS-WIRED: every paced arm run is SLACK and U passes. With PL dropped by amendment (plan51e.PL_DROPPED) the paced set is PM alone, and the line says so."""
    arms = plan51e.active_arms()
    empty = [a for a in arms if not by_arm_rows.get(a)]
    if empty:
        return "NO VERDICT", f"no live row for {','.join(empty)}"
    zs = [gauge_sums([rv])["zeta"] for rv in by_arm_rows["U"]]
    if all(z is None or z < ZETA_VALID for z in zs):
        return "NO VERDICT", f"U's positive control failed: zeta {','.join(_f(z, 3) for z in zs)} < {ZETA_VALID} on every U row"
    cls = {a: klass(gauge_sums(by_arm_rows[a])["phi"]) for a in plan51e.PACED if a in arms}
    pinned = [a for a, c in cls.items() if c == "PINNED"]
    if pinned:
        return "PINNED-PACED", f"{','.join(pinned)} PINNED: the rate law operates as a constant there (a defect finding, MEASUREMENT DISCIPLINE 18)"
    part = [a for a, c in cls.items() if c == "PARTIAL"]
    if part:
        return "LIVE-RANGE", f"{','.join(part)} PARTIAL: the clamp operates in its continuous range at a paced load"
    if all(c == "SLACK" for c in cls.values()) and cls:
        return "DEGENERATE-AS-WIRED", ("PL and PM SLACK" if len(cls) == 2 else f"{','.join(cls)} SLACK (PL dropped by amendment)") + " and U passes the control: a saturation switch, not a headroom law (a defect finding, MEASUREMENT DISCIPLINE 18)"
    return "NO VERDICT", "a paced arm has no phi (no accruals)"


def tau_reading(urows):
    """Section 7's rate-spare-cwnd reading over the U rows: NOT-ENFORCED (tau_x >= 0.5 on every U row), CONTRADICTED (tau_x = 0 on every U row), MIXED (anything else)."""
    taus = [rv.t.get("tau_x") for rv in urows]
    if not taus or any(x is None for x in taus):
        return "UNREAD", taus
    if all(x >= TAU_NOT_ENFORCED for x in taus):
        return "NOT-ENFORCED", taus
    if all(x == 0 for x in taus):
        return "CONTRADICTED", taus
    return "MIXED", taus


def pl_calibration(rv):
    """Section 4: PL's rehearsal reading -> (fires, reasons, calibrate-dict). It fires iff the LATE median A* is outside [3, 7] or there are fewer than 30 K4-steady LATE windows."""
    ev = rv.t.get("evict") or {}
    a = (ev.get("mbar59") or {}).get("a_star")
    st = rv.steady()
    why = []
    if a is None or not (ASTAR_LO <= a <= ASTAR_HI):
        why.append(f"median A* {_f(a, 2)} outside [{ASTAR_LO:g}, {ASTAR_HI:g}]")
    if st["n_steady"] < STEADY_MIN:
        why.append(f"{st['n_steady']} K4-steady LATE windows (< {STEADY_MIN})")
    return bool(why), why, plan51e.calibrate(a)


def pace_view(rv):
    """Reported per paced row (status 51 Amendment 4): the pacer's ENQUEUE lateness (late / chunks, late_max_ms: the harness's offer into the feed queue; a stall > 250 ms voids, below it nothing is claimed), the engine's off-load
    share of the offer-window ticks, and the K4 steady windows scored / excluded as off-load."""
    if rv.arm not in plan51e.PACED:
        return ""
    pl = (rv.t.get("pace") or [{}])[-1]
    ch, late = _num(pl.get("chunks")), _num(pl.get("late"))
    off, st = rv.offload(), rv.steady()
    return (f"enqueue-late={pl.get('late')}/{pl.get('chunks')} ({_f(100.0 * late / ch if ch and late is not None else None, 2)} %) enqueue-late-max={pl.get('late_max_ms')} ms "
            f"offload={off['off']}/{off['n']} ({_f(100.0 * off['share'] if off['share'] is not None else None, 1)} % of the offer-window ticks) steady={st['n_steady']}/{st['n_late']} (excluded off-load: {st['n_offload_excluded']})")


def row_view(rv):
    """The one-line reading of a row (ROW51E): the registered readouts beside their predictions."""
    t = rv.t
    ev = t.get("evict") or {}
    m = ev.get("mbar59") or {}
    gs = gauge_sums([rv])
    pr = PRED.get(rv.arm, {})
    return (f"ROW51E {rv.arm} s{rv.seed} r{rv.rep}: status={rv.r.get('status')} dnf={int(rv.dnf)} src={_f(t.get('src_med_late'), 0)} [{pr.get('src')}] last-tick={_f(t.get('last_src_t'), 1)} [{pr.get('last_tick')}] "
            f"A*={_f(m.get('a_star'), 1)} [{pr.get('astar', '-')}] Delta={_f(m.get('delta'), 2)} [{pr.get('delta', '-')}] W={_f((t.get('span') or {}).get('wsize'), 0)} rr={_f(m.get('rr'), 3)} [{pr.get('rr')}] "
            f"rr=0.500 share={_f((t.get('span') or {}).get('rr500'), 3)} shed={m.get('shed_total')}/bud={_f(m.get('bud'), 4)} evict-share={_f(t.get('evict_share'), 4)} [{pr.get('evict')}] "
            f"tau_x={_f(t.get('tau_x'), 3)} [{pr.get('tau')}] tau(3.2)={_f(t.get('tau_sat'), 3)} MAX_CWND-share={_f(t.get('maxcwnd_share'), 3)} qcwnd/(cwnd*512)={_f(t.get('qcwnd_ratio'), 2)} "
            f"phi={_f(gs['phi'], 4)} zeta={_f(gs['zeta'], 4)} pi={_f(gs['pi'], 4)} deficit={_f(gs['deficit'], 5)} windows={gs['windows']} [{pr.get('phi')}] " + pace_view(rv))


def score(paths, out=print):
    import place38_parse as pp
    rows_all, tokens = pp.rows_of(paths)
    rows_all = [r for r in rows_all if r.get("cell") == plan51e.CELL and r.get("arm") in plan51e.ARMS]
    cnt, replaced = ps.counting(rows_all)
    idx = {id(r): i for i, r in enumerate(rows_all)}
    cnt.sort(key=lambda r: idx[id(r)])
    n_exp = plan51e.expect_rows()
    rvs = [RV(r, i) for i, r in enumerate(cnt)]
    live = [rv for rv in rvs if rv.live]
    void = [rv for rv in rvs if not rv.live]
    by = {a: [rv for rv in live if rv.arm == a] for a in plan51e.ARMS}
    out(f"S51E SCORE (evicting seat with the spare-clamp gauge, c2-100 single, Realtime, RWM_C_RELIABLE=0, RWM_SLAW=1; paced load RWM_PERF_RATE_MBIT; LATE t >= {LATE_T:g} s; REPORTED): "
        f"PL rate {plan51e.PL_RATE_SYM} sym/s{' (DROPPED by amendment)' if plan51e.PL_DROPPED else ''}, PM {plan51e.PM_LAMBDA_MBIT:g} Mbit; ABORT-TOKENS {' '.join(sorted({t for t in tokens if t in pp.ABORT_TOKENS})) or 'none'}")
    for r in replaced:
        out(f"  REPLACED {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} {' '.join(r['problems'])}")
    for rv in void:
        out(f"  VOID {rv.arm} s{rv.seed} rep{rv.rep} {rv.r.get('status')} {' '.join(rv.r.get('problems') or [])}")
    for rv in rvs:
        if rv.r.get("t51e_error"):
            out(f"  T51E-ERROR {rv.arm} s{rv.seed} rep{rv.rep}: {rv.r['t51e_error']}")
    labels = []

    def lab(name, val):
        labels.append((name, val))
    out("ROWS (counting rows, one line each; the registered prediction in [ ]):")
    for rv in live:
        out("  " + row_view(rv))
    # pacing witness, gauge witness
    for rv in live:
        pp_ = pace_problems(rv.arm, rv.t)
        gp = gauge_problems(rv.t)
        if pp_ or gp:
            out(f"  WITNESS {rv.arm} s{rv.seed} r{rv.rep}: {'; '.join(pp_ + gp)}")
    lab("PACE", "PASS" if live and all(not pace_problems(rv.arm, rv.t) for rv in live) else "FAIL")
    lab("GAUGE-WITNESS", "PASS" if live and all(not gauge_problems(rv.t) for rv in live) else "FAIL")
    out("THE SPARE-CLAMP GAUGE (section 6; pooled over each arm's windows with accruals: LATE for PL / PM, every sending window for U):")
    for a in plan51e.ARMS:
        if not by[a]:
            out(f"  {a:<2}: no live row")
            lab(f"SPARE-CLAMP-{a}", "NO-ROWS")
            continue
        gs = gauge_sums(by[a])
        cl = klass(gs["phi"])
        out(f"  {a:<2}: phi={_f(gs['phi'], 4)} (bind)  zeta={_f(gs['zeta'], 4)} (zero)  pi={_f(gs['pi'], 4)} (partial)  clamp deficit={_f(gs['deficit'], 5)}  windows={gs['windows']} accruals={gs['an']:g}  class={cl}   [predicted {PRED[a]['phi']}]")
        lab(f"SPARE-CLAMP-{a}", f"{cl} phi={_f(gs['phi'], 4)} zeta={_f(gs['zeta'], 4)} pi={_f(gs['pi'], 4)} deficit={_f(gs['deficit'], 5)}")
    name, why = outcome(by)
    out(f"OUTCOME {name}: {why}   [predicted DEGENERATE-AS-WIRED]")
    lab("SPARE-CLAMP-OUTCOME", name)
    tr, taus = tau_reading(by["U"])
    out(f"RATE-SPARE-CWND {tr}: tau_x on the U rows {','.join(_f(x, 3) for x in taus)} (NOT-ENFORCED needs >= {TAU_NOT_ENFORCED} on every U row; CONTRADICTED needs 0 on every U row)   [predicted NOT-ENFORCED]")
    lab("RATE-SPARE-CWND", tr)
    for a in plan51e.ARMS:
        if by[a]:
            out(f"  reported {a}: evict-share {','.join(_f(rv.t.get('evict_share'), 4) for rv in by[a])}; MAX_CWND-share {','.join(_f(rv.t.get('maxcwnd_share'), 3) for rv in by[a])}; "
                f"qcwnd/(cwnd*512) {','.join(_f(rv.t.get('qcwnd_ratio'), 2) for rv in by[a])}; rr=0.500 bind {','.join(_f((rv.t.get('span') or {}).get('rr500'), 3) for rv in by[a])}")
    out("K4 INPUTS (m-bar, W1, the steady-window rule with ab = 0; K4 itself is computed off-VM on pfec-oracle; the U rows are not read):")
    for a in ("PL", "PM"):
        if a not in plan51e.active_arms():
            out(f"  {a}: dropped by amendment")
            continue
        for rv in by[a]:
            out("  " + s60.evict_line(rv, rule_ab=True).replace("EVICT ", f"{a} ", 1))
        steady = [rv.steady()["n_steady"] for rv in by[a]]
        lab(f"K4-{a}", f"steady={','.join(str(x) for x in steady)} {'SCOREABLE' if steady and all(x >= STEADY_MIN for x in steady) else 'K4-UNSCOREABLE'}")
    if "PL" in plan51e.active_arms() and by["PL"]:
        for rv in by["PL"]:
            fires, why_, cal = pl_calibration(rv)
            if fires:
                out(f"PL-CALIBRATION s{rv.seed} r{rv.rep}: FIRES ({'; '.join(why_)}); " + (f"x-hat {_f(cal['x_hat'], 2)} -> PL rate {cal['rate_sym']} sym/s, lambda {cal['lam_mbit']} Mbit, bytes {cal['bytes']}" if cal else "uncomputable"))
            else:
                out(f"PL-CALIBRATION s{rv.seed} r{rv.rep}: does not fire (A* in [{ASTAR_LO:g}, {ASTAR_HI:g}] and >= {STEADY_MIN} steady windows)")
        lab("PL-CALIBRATION", "FIRES" if any(pl_calibration(rv)[0] for rv in by["PL"]) else "OK")
    ndnf = sum(1 for rv in live if rv.dnf)
    out(f"VERDICT51E REPORTED-ONLY slots={len(ps.slots(rows_all))}/{n_exp} live={len(live)} void={len(void)} replaced={len(replaced)} dnf={ndnf} outcome={name.replace(' ', '-')} rate_spare_cwnd={tr} labels={len(labels)}")
    for n_, v in labels:
        out(f"LABEL51E {n_} {v}")
    return 0


# ───────────────────────────── the smoke / rehearsal ─────────────────────────────
SETS = {"smoke": lambda: plan51e.SMOKE, "rehearsal": lambda: plan51e.REHEARSAL, "recheck": lambda: [r for r in plan51e.REHEARSAL if r[2] == "PL"]}


def smoke(path, which, out=print):
    """Section 10's go/no-go as PASS / FAIL lines ending SMOKE-PASS | SMOKE-FAIL (the launch aborts on a FAIL; a NO-GO is an amendment before the battery). smoke = the one PM row ((b)-(g) on it); rehearsal = PL, PM, U
    ((a) is the launch's bin_sha / manifest gate, here the row's own sha; (b)-(g); (h) NOT gating: it prints the calibration rule); recheck = the PL-only row, (h) gating. A criterion over no row prints NOT-RUN."""
    import place38_parse as pp
    if which not in SETS:
        out(f"unknown set {which}")
        return 2
    slots = [r for r in SETS[which]() if r[2] in plan51e.only_arms() or which == "smoke"]
    rows, _ = pp.rows_of([path])
    got = {}
    for r in rows:
        got[(r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep")))] = r          # the last row of a slot
    bad = []

    def crit(name, fails, ran=True):
        if not ran:
            out(f"NOT-RUN {name} (no such row in this set)")
            return
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    out(f"SMOKE51E set={which} rows={len(rows)} slots={len(slots)} (section 10; a DNF is the expected outcome of an evicting seat and is not a failure)")
    rvs, miss = [], []
    for (c, _, a, sd, rp) in slots:
        r = got.get((c, a, str(sd), str(rp)))
        if r is None:
            miss.append(f"{c} {a} s{sd} r{rp}: no row")
        else:
            rvs.append(RV(r, len(rvs)))
    crit("one row per slot ran with a t51e", miss + [f"{rv.arm}: no t51e ({rv.r.get('t51e_error')})" for rv in rvs if not rv.t])
    rvs = [rv for rv in rvs if rv.t]
    for rv in rvs:
        out("  " + row_view(rv))
    paced = [rv for rv in rvs if rv.arm in plan51e.PACED]
    u = [rv for rv in rvs if rv.arm == "U"]
    pl = [rv for rv in rvs if rv.arm == "PL"]

    def probs(rv, prefixes):
        return [x for x in rv.r.get("problems") or [] if x.startswith(prefixes)]
    crit("(a) every row ran the one seat's binary (LD_SHA; the registered bin_sha and the manifest HIT are the launch's gates)", [f"{rv.arm}: {x}" for rv in rvs for x in probs(rv, ("bin-sha",))], bool(rvs))
    crit("(b) plan 59's liveness: the shed echo, shed= on [SPAN], RWM_UNIFIED / RWM_UNIFIED_SHED on both ends, no retain-until-acked echo, the Realtime echo; RWM_SLAW=1 and RWM_DIAG=1 on both ends",
         [f"{rv.arm}: {' '.join(probs(rv, ('evict-', 'gate-unified', 'no-realtime-echo', 'gate-slaw', 'gate-diag', 'gate-three-term')))}" for rv in rvs if probs(rv, ('evict-', 'gate-unified', 'no-realtime-echo', 'gate-slaw', 'gate-diag', 'gate-three-term'))], bool(rvs))
    crit("(c) the pacing witness (section 5.2): both echoes, median LATE src within +-5 % of rate_sym, the last sending tick at t >= T_e - 2 s, achieved_mbit within +-2 % of lambda, late_max_ms <= 250 (pace-stall; late is reported); U reads unset with no echo / pace line",
         [f"{rv.arm}: {x}" for rv in rvs for x in pace_problems(rv.arm, rv.t)], bool(rvs))
    rr = lambda rv: ((rv.t.get("evict") or {}).get("mbar59") or {}).get("rr")      # noqa: E731
    crit("(d) the debt loop runs: the median LATE [SPAN] rr > 0 on PL and PM (Amendment 2's go/no-go)", [f"{rv.arm}: rr median {rr(rv)}" for rv in paced if not ((rr(rv) or 0) > 0)], bool(paced))
    nwin = lambda rv: sum(1 for i in rv.windows())      # noqa: E731
    crit(f"(e) >= {MIN_LATE_WINDOWS} LATE windows with src > 0 on PL and PM", [f"{rv.arm}: {nwin(rv)} windows" for rv in paced if nwin(rv) < MIN_LATE_WINDOWS], bool(paced))
    ev_f = [f"{rv.arm}: eviction share {rv.t.get('evict_share')} (want <= {EVICT_MAX_PACED})" for rv in paced if rv.t.get("evict_share") is None or rv.t["evict_share"] > EVICT_MAX_PACED]
    ev_f += [f"U: eviction share {rv.t.get('evict_share')} (want > {EVICT_MIN_U})" for rv in u if rv.t.get("evict_share") is None or rv.t["evict_share"] <= EVICT_MIN_U]
    crit(f"(f) the eviction share (hand - tx)/hand <= {EVICT_MAX_PACED} on PL and PM, > {EVICT_MIN_U} on U", ev_f, bool(paced or u))
    crit("(g) the section 6.2 on-row gauge witnesses (an = fec n, rn + r0 = an, grant = flat_rate, r = 0 on a zero grant) on every row", [f"{rv.arm}: {x}" for rv in rvs for x in gauge_problems(rv.t)], bool(rvs))
    crit(f"(g) U's positive control: zeta >= {ZETA_VALID} over the sending windows (validity; the prediction is >= {ZETA_PRED})",
         [f"U s{rv.seed}: zeta {gauge_sums([rv])['zeta']}" for rv in u if (gauge_sums([rv])["zeta"] or 0) < ZETA_VALID], bool(u))
    for rv in u:
        out(f"  REPORTED tau_x on U s{rv.seed}: {_f(rv.t.get('tau_x'), 3)} (predicted >= 0.5; rate-spare-cwnd NOT-ENFORCED if >= 0.5 on both U rows)")
    for rv in paced:
        st = rv.steady()
        if rv.arm == "PM":
            out(f"  REPORTED PM K4-steady LATE windows: {st['n_steady']}/{st['n_late']} (predicted < {STEADY_MIN}: K4-UNSCOREABLE)")
    if which in ("rehearsal", "recheck"):
        for rv in pl:
            a = (rv.t.get("evict") or {}).get("mbar59", {}).get("a_star")
            st = rv.steady()
            ok = a is not None and ASTAR_LO <= a <= ASTAR_HI and st["n_steady"] >= STEADY_MIN
            out(f"  (h) PL s{rv.seed}: median A* {_f(a, 2)} in [{ASTAR_LO:g}, {ASTAR_HI:g}], {st['n_steady']} K4-steady LATE windows (>= {STEADY_MIN}, full rule incl. ab = 0): {'OK' if ok else 'NOT MET'}")
            if which == "rehearsal":
                if not ok:
                    fires, why_, cal = pl_calibration(rv)
                    out("CALIBRATE PL (section 4; (h) is handled by the calibration, not by NO-GO): " + "; ".join(why_)
                        + ((f". x-hat = A* - 0.5 = {cal['x_hat']:.2f}; new PL rate {cal['rate_sym']} sym/s = lambda {cal['lam_mbit']} Mbit, bytes {cal['bytes']} "
                            f"(edit plan51e.PL_RATE_SYM, record the amendment, then LD_REHEARSE=1 LD_ONLY=PL for the one PL-only recheck row)") if cal else ". uncomputable (no median A*)"))
                else:
                    out("PL calibration rule: does not fire")
            else:
                crit("(h) PL passes: median A* in [3, 7] and >= 30 K4-steady LATE windows; otherwise PL is dropped from the battery by amendment (plan51e.PL_DROPPED = True)",
                     [] if ok else [f"PL s{rv.seed}: A* {_f(a, 2)}, {st['n_steady']} steady windows -> DROP-PL by amendment"])
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "smoke51e" and len(a) == 2:
        return smoke(a[1], a[0])
    if cmd == "score51e" and a:
        return score(a)
    print(f"usage: {sys.argv[0]} smoke51e <smoke|rehearsal|recheck> <ledger> | score51e <ledger> [...]")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
