#!/usr/bin/env python3
"""Status 60: the store law's gauge session (B-1), plans 60a / 60b. The SCORER: row readouts (`t60`), the smoke / rehearsal check and the session score.
Spec: docs/derivations/s60-registration.md (cited per section as `s60 section N`); tables and the replacement rule: plan60.py.

    s60_score.py smoke60 <60a|60b|60r|60br> <ledger>        PASS / FAIL lines ending SMOKE-PASS | SMOKE-FAIL; 60r adds the rehearsal witnesses and the GO-NOGO decisions
    s60_score.py score60 <60a|60b|all> <ledger> [...]  every registered readout, then VERDICT60 and one LABEL60 line per scored readout

Two layers, so the VM-side row parser stays fast and the scoring stays a pure function of the ledger:
  row_metrics(cli, srv, drv, nbytes, cell, arm, mbps) -> `t60` (stored in the ledger row): EXTRACTION ONLY, one pass over the client log, columns not dicts. Nothing in it depends on the
      session calibration; R~, R~67, LATCHED, validity and the floor-pool choice are all computed at score time from the stored per-window fields.
  smoke() / score(): read the ledger (place38_parse.rows_of), the last row of a slot counts (pool56_score.counting).

What `t60` carries (the keys other code relies on): cell, arm, kind, W, late{leg: {FIELDS columns}} (LATE [SLAW] windows, every field of section 8 item 5 that the scoring needs), nwin{leg},
boot{leg: b0k, b0kn, b0p, kw}, ml{leg: n, eq, maxdiff} (m = l for t < 10 s), witness{n, bad, worst, ok, error} (the per-row replay gate), caplog{records, replayed, mismatches, unreplayable,
ok, n_pooled, ub, fb, seats, rate_hz}, tags{cli, srv, all_cli}, gate_slaw{cli, srv}, q[distinct printed q strings], diag{min_rtp_us, collapse{leg: n, share, flag, e50, e90, x}, attach{...},
n_ticks}, evict{...} (EVICT rows only: the mbar59 readouts, per-window adv / dbt / fec columns, [SPAN] delta / a_star, the cum-ack series).
"""
import json
import math
import os
import random
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
STORE_LAW = os.path.normpath(os.path.join(HERE, "..", "..", "docs", "derivations", "store-law"))
for _p in (HERE,):
    if _p not in sys.path:
        sys.path.insert(0, _p)
import plan60  # noqa: E402  (plan60 imports place38_parse / l1common lazily, inside functions)

C2, C8, C1S = plan60.C2, plan60.C8, plan60.C1S
SHORT = {C2: "c2", C8: "c8", C1S: "c1s"}
LEGS = {C2: (0,), C8: (0, 1), C1S: (0,)}
LATE_T = plan60.LATE_T                         # s60 section 1.1: t >= 15 s at c2 / c8, >= 5 s at c1s
NK_MIN = 10                                    # s60 section 2: windows with nk >= 10
EVICT_LATE_T = 5.0                             # s60 section 6: m-bar over LATE (t >= 5 s) (= mbar59_score.LATE_T)
Q_BULK = 0.10                                  # s60 section 3.1: q = 0.10 (Bulk); Realtime prints 0.050000
FIELDS = ("k", "t", "nk", "tmin", "tp50", "tmean", "m", "l", "b0p", "sig", "sigb", "anc", "err", "R", "Rb", "q")
GAUGE_TAGS = ("WSTAT", "SLAW", "WSTORE", "SLAWC", "CAPLOG")
K_BOOT = 8                                     # s60 section 2: K = 8 boot windows (the scored K)
B0KN_MIN = 10                                  # s60 section 2: leg-0 b0kn >= 10 on each of the K brakes; leg 1 uses the b0k entries with b0kn >= 10
LEG1_MIN_ENTRIES = 4                           # s60 section 2: leg 1 needs >= 4 such entries of K
STARVED_LEG1_W = 256                           # s60 section 5: the latest arm a starved c8 leg 1 may calibrate on
MIN_CAL_ROWS = 3                               # s60 section 2: >= 3 valid, non-latched floor-pool rows per cell and leg
SHARE_NK = 0.80                                # s60 section 2: >= 80 % of a leg's LATE windows with nk >= 10 qualifies a floor pool
OMEGA = {C2: {0: 0.90}, C8: {0: 1.8, 1: 6.3}, C1S: {0: 0.20}}      # s60 section 2: LATCHED omega-bar (ms)
X_COLLAPSE = {C1S: {0: 6.1}, C2: {0: 14.7}, C8: {0: 17.8, 1: 24.2}}   # s60 section 2(c): COLLAPSED X (ms) = E90 + 2 (E90 - E50) per cell and leg
COLLAPSE_SHARE = 0.10                          # s60 section 2(c): more than 10 % of LATE ticks above X
SLOPE_FLAT = 0.010                             # s60 section 2 reading 1: ms/slot (half the low end of R6.7's secant)
TAU = {C2: {0: 0.3}, C8: {0: 1.0, 1: 2.5}}     # s60 section 2 reading 2: convergence tolerance (ms); c1s has none registered
P1_RATIO = 3.0                                 # s60 section 1.2
SIGN_ALPHA = 0.05                              # s60 section 3.3 (s60cond.py K table): two-sided sign test
R_SIGN_MIN = 2                                 # Amendment 1: the R-sign gate PASSes on >= 2 replayable control rows (the registered reading used replicates 1-2); fewer is UNSCOREABLE
MIN_SIGN_N = 6                                 # s60 section 3.3: fewer scoreable replicates is UNDERPOWERED
B2_FIRES_MIN = 2                               # s60 section 3.2: >= 2 of the scoreable replicates
MIN_DNF_LATE_WIN = 120                          # a DNF calibration row needs >= 30 s of LATE windows (120 x 250 ms, the doc's "with >= 30 s of LATE windows"), else it is not a valid calibration row
D_WIT_RES = 0.01                                # ms: |D_r| below this is under the sign witness's resolution (printed D has 3 decimals): reported UNRESOLVED, never SCORER-BUG
B2_MIN_SCOREABLE = 5                           # s60 section 3.2: "the 5-6 scoreable replicates" (DECISION 12)
B2_ANC_MIN = 0.9                               # s60 section 3.2: replay anc bind fraction >= 0.9 at both bracketing arms
KNEE_FRAC = 0.95                               # s60 section 3.1 / 3.2: W95 = the goodput knee against the same replicate's control
D_MIN_REPS = 3                                 # DECISION 13
BOOT_N, BOOT_SEED, BOOT_LO, BOOT_HI = 2000, 60, 0.05, 0.95      # s60 section 3.1: bootstrap 2000, seed 60, 90 % interval
GC_PERTURB = math.log(1.10)                    # s60 section 4: PERTURBING iff median log(G/C) of CPU s/GB exceeds +10 %
GC_MIN_PAIRS = 4                               # s60 section 4: below 4 unflagged pairs UNRESOLVED
STEADY_MIN = 30                                # s60 section 6: >= 30 steady LATE windows or K4-UNSCOREABLE
SMOKE_B0P_LIMIT = {C1S: 0.6, C8: 3.9}          # s60 section 5: the absolute b0p limits of entry 8, REPORTED only
ML_T = 10.0                                    # s60 section 1.3: m = l for t < 10 s
T975 = [12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160, 2.145, 2.131,
        2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056, 2.052, 2.048, 2.045, 2.042,
        2.040, 2.037, 2.035, 2.032, 2.030, 2.028, 2.026, 2.024, 2.023, 2.021]          # s60size.py: t 0.975, df 1..40
T80 = [1.376, 1.061, 0.978, 0.941, 0.920, 0.906, 0.896, 0.889, 0.883, 0.879, 0.876, 0.873, 0.870, 0.868, 0.866,
       0.865, 0.863, 0.862, 0.861, 0.860, 0.859, 0.858, 0.858, 0.857, 0.856, 0.856, 0.855, 0.855, 0.854, 0.854,
       0.853, 0.853, 0.853, 0.852, 0.852, 0.852, 0.851, 0.851, 0.851, 0.851]           # s60size.py: t 0.80
BANDS = {C2: (148, 164), C8: (440, 446), C1S: (651, 685)}       # Appendix A: R6.7's honest-floor crossings (cap-128-based context, reported)
BANDS_A3 = {C2: (158, 242), C8: (433, 586), C1S: (406, 981)}    # Appendix A.3 (cap-128-based; reported)
NOGO_TEXT = {C2: "the c2 floor pool moves to W 32 and the move is reported (s60 section 5)",
             C8: "c8 B2 is not scored; its rows still run for P1, P3, the gate and the reported R~ / R~67 (s60 section 5)",
             C1S: "the c1s control sign is dropped; 60b runs only the G/C pairs (s60 section 5)"}

_LIB = None


def _lib():
    """(s60replay, caplog_replay, error): imported once from docs/derivations/store-law (python -I drops the script dir; the path is inserted here). Never raises."""
    global _LIB
    if _LIB is None:
        try:
            if STORE_LAW not in sys.path:
                sys.path.insert(0, STORE_LAW)
            import caplog_replay
            import s60replay
            _LIB = (s60replay, caplog_replay, None)
        except Exception as e:      # recorded in the row; the smoke fails on it
            _LIB = (None, None, f"{type(e).__name__}: {e}")
    return _LIB


# ───────────────────────────── small statistics ─────────────────────────────
def med(v):
    v = sorted(x for x in v if x is not None)
    n = len(v)
    if not n:
        return None
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def sd(v):
    v = [x for x in v if x is not None]
    if len(v) < 2:
        return None
    m = sum(v) / len(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def pq(v, p):
    v = sorted(v)
    return v[int(p * (len(v) - 1))] if v else None          # s60collapse56.py's percentile


def ols(xs, ys):
    """Least squares y = a + b x -> (b, a, se_b); se_b from the residuals with n - 2 dof (s60short.py: s / sqrt(sum (x - xbar)^2)). None below 3 points / without x spread."""
    n = len(xs)
    if n < 3:
        return None
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    if sxx <= 0:
        return None
    b = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx
    a = my - b * mx
    res = sum((y - a - b * x) ** 2 for x, y in zip(xs, ys))
    return b, a, math.sqrt(res / (n - 2) / sxx)


def ranks(v):
    o = sorted(range(len(v)), key=lambda i: v[i])
    r = [0.0] * len(v)
    i = 0
    while i < len(o):
        j = i
        while j + 1 < len(o) and v[o[j + 1]] == v[o[i]]:
            j += 1
        for k in range(i, j + 1):
            r[o[k]] = (i + j) / 2.0
        i = j + 1
    return r


def spearman(x, y):
    rx, ry = ranks(x), ranks(y)
    mx, my = sum(rx) / len(rx), sum(ry) / len(ry)
    num = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    den = (sum((a - mx) ** 2 for a in rx) * sum((b - my) ** 2 for b in ry)) ** 0.5
    return num / den if den else 0.0


def perm_p(x, y):
    """Two-sided permutation p of the Spearman rho (s60 section 2 drift check): exact for n <= 8, else 20 000 shuffles with seed 60 (s60drift.py)."""
    import itertools
    rho = abs(spearman(x, y))
    n = len(x)
    if n <= 8:
        hits = tot = 0
        for p in itertools.permutations(y):
            tot += 1
            hits += abs(spearman(x, list(p))) >= rho - 1e-12
        return hits / tot
    rnd = random.Random(60)
    hits = 0
    for _ in range(20000):
        hits += abs(spearman(rnd.sample(x, n), y)) >= rho - 1e-12
    return hits / 20000


def sign_k(n):
    """The smallest count k of one sign for which the two-sided exact sign test over n non-zero replicates is <= SIGN_ALPHA (s60cond.py K table: 6:6 7:7 8:8 9:8 10:9 11:10 12:10); None if never."""
    for k in range(n // 2 + 1, n + 1):
        tail = sum(math.comb(n, j) for j in range(k, n + 1)) / 2.0 ** n
        if 2 * tail <= SIGN_ALPHA + 1e-12:
            return k
    return None


def cross_up(pts, y):
    """First W where the piecewise-linear [(W, v)] reaches y rising (s60flip.py cross). -> (W|None, clamped_at_first_point)."""
    pts = sorted(pts)
    if not pts:
        return None, False
    if pts[0][1] >= y:
        return float(pts[0][0]), True
    for (w1, a), (w2, b) in zip(pts, pts[1:]):
        if a < y <= b:
            return w1 + (y - a) * (w2 - w1) / (b - a), False
    return None, False


def interp(pts, x):
    """Linear interpolation of [(W, v)] at x; outside the range it clamps to the end point. -> (value|None, clamped, (lo_pt, hi_pt))."""
    pts = sorted(pts)
    if not pts:
        return None, False, None
    if x <= pts[0][0]:
        return pts[0][1], x < pts[0][0], (pts[0], pts[0])
    if x >= pts[-1][0]:
        return pts[-1][1], x > pts[-1][0], (pts[-1], pts[-1])
    for (w1, a), (w2, b) in zip(pts, pts[1:]):
        if w1 <= x <= w2:
            return (a if w2 == w1 else a + (b - a) * (x - w1) / (w2 - w1)), False, ((w1, a), (w2, b))
    return None, False, None


def f(x, nd=2):
    return "-" if x is None else (f"{x:.{nd}f}" if isinstance(x, float) else str(x))


# ───────────────────────────── row parsing (the VM side) ─────────────────────────────
_TOK = re.compile(r"(\w+)=(\S+)")
_DT = re.compile(r"^\[DIAG\] t=([\d.]+)s ")
_DSRC = re.compile(r" src=(\d+)sym/s")
_DTOP = re.compile(r" rtt=([\d.]+)ms ")
_DWIN = re.compile(r" win=(\d+)/(\d+)")
_DLEG = re.compile(r"\bp(\d+):infl=.*?rtt=(\d+)/wrtt=\d+/rtp\d+ms rtp_us=(\d+)(?: sig_us=\d+/n(\d+))?")
_DSPLIT = re.compile(r"(?= p\d+:infl=)")
WARM = '"warmup":true'


def _num(s):
    if s is None or s == "-":
        return None
    try:
        return float(s)
    except ValueError:
        return None


def _list(s):
    return None if s is None or s == "-" else [_num(x) for x in s.split(",")]


def _tick(ln):
    """A [DIAG] line -> [t, src, top_rtt_ms, rtt0, rtp0_us, n0, rtt1, rtp1_us, n1, win, cap] (None where absent), or None."""
    m = _DT.match(ln)
    if not m:
        return None
    sr, tp, wn = _DSRC.search(ln), _DTOP.search(ln), _DWIN.search(ln)
    legs = {}
    for part in _DSPLIT.split(ln)[1:]:
        g = _DLEG.search(part)
        if g:
            legs[int(g.group(1))] = (int(g.group(2)), int(g.group(3)), int(g.group(4)) if g.group(4) is not None else None)
    a, b = legs.get(0, (None, None, None)), legs.get(1, (None, None, None))
    return [float(m.group(1)), int(sr.group(1)) if sr else None, float(tp.group(1)) if tp else None, a[0], a[1], a[2], b[0], b[1], b[2],
            int(wn.group(1)) if wn else None, int(wn.group(2)) if wn else None]


def _windows_lines(slaw):
    """s60replay.windows() over parsed [SLAW] dicts instead of a file path: {k: [fields of each line with nk > 0]} (the warm-up cut is the caller's)."""
    out = {}
    for d in slaw:
        if d.get("nk", "0") == "0":
            continue
        try:
            out.setdefault(int(d["k"]), []).append(d)
        except (KeyError, ValueError):
            pass
    return out


def replay_witness(slaw, dtmin=0.0):
    """s60 section 1.1, the per-row gate: R~ replayed with the row's OWN b0p and l must reproduce printed R and Rb on every defined window to s60replay.TOL.
    Same grouping as s60replay.witness (ready(), legs_of(), slaw_r(), num()), `n > 0 and bad == 0` is PASS. -> {n, bad, worst, ok, error}."""
    sr, _, err = _lib()
    if sr is None:
        return {"n": 0, "bad": 0, "worst": None, "ok": None, "error": err}
    n = bad = 0
    worst = 0.0
    try:
        for k, ls in sorted(_windows_lines(slaw).items()):
            if not sr.ready(ls) or ls[0].get("R", "-") == "-":
                continue
            q = sr.num(ls[0], "q")
            for boot, key in ((False, "R"), (True, "Rb")):
                want = sr.num(ls[0], key)
                if want is None or q is None:
                    continue
                e = abs(sr.slaw_r(sr.legs_of(ls, boot=boot, dtmin=dtmin), q) - want)
                worst = max(worst, e)
                n += 1
                bad += e > sr.TOL
    except Exception as e:
        return {"n": n, "bad": bad, "worst": worst, "ok": False, "error": f"{type(e).__name__}: {e}"}
    return {"n": n, "bad": bad, "worst": worst, "ok": n > 0 and bad == 0, "error": None}


def caplog_gate(lines):
    """s60 section 5: every logged cap reproduced by caplog_replay (a mismatch is a FAIL), plus the bind fraction of the pooled law's clamp (reported):
    ub = share of replayed pooled records whose unclamped cap exceeds the ceiling max(n * pool, floor), fb = share below the floor."""
    out = {"records": 0, "replayed": 0, "mismatches": 0, "unreplayable": 0, "q_off": 0, "ok": None, "n_pooled": 0, "ub": None, "fb": None, "seats": [], "rate_hz": None, "error": None}
    if not lines:
        out["ok"] = False
        return out
    _, cr, err = _lib()
    if cr is None:
        out["error"] = err
        return out
    try:
        o = cr.replay_log(lines)
        out.update(records=o["records"], replayed=o["replayed"], mismatches=len(o["mismatches"]), unreplayable=o["unreplayable"], q_off=o["q_off"])
        out["ok"] = o["records"] > 0 and not o["mismatches"]
        ub = fb = npool = 0
        seats, ts = set(), []
        for ln in lines:
            r = cr.parse_line(ln)
            if not r:
                continue
            seats.add(r["seat"])
            ts.append(r["t"])
            if r["br"] == "pooled" and r["on"] and r["psum"] > 0:
                npool += 1
                value = 1.0 + r["q"] if r["dc"] else r["gain"]
                un = math.ceil(value * (1.0 if r["sum"] else float(r["n"])) * r["psum"])
                ub += un > max(r["n"] * r["pool"], r["floor"])
                fb += un < r["floor"]
        out["n_pooled"], out["seats"] = npool, sorted(seats)
        out["ub"], out["fb"] = (ub / npool, fb / npool) if npool else (None, None)
        if len(ts) >= 2 and max(ts) > min(ts):
            out["rate_hz"] = (len(ts) - 1) / ((max(ts) - min(ts)) / 1e6)          # t is in microseconds
    except Exception as e:
        out["ok"], out["error"] = False, f"{type(e).__name__}: {e}"
    return out


def _collapse(ticks, cell, minrtp):
    """s60 section 2(c): excess = the tick's `rtt=` (top-level 0.1 ms at one leg, per-leg 1 ms at c8; both in ms) minus the row's minimum `rtp_us` (> 0) over the leg; LATE ticks with src > 0;
    COLLAPSED iff more than 10 % of them exceed X(cell, leg). Also E50 / E90 (s60collapse56.py percentiles) for the report."""
    one_leg = len(LEGS.get(cell, (0,))) == 1
    t0 = LATE_T.get(cell, 15.0)
    out = {}
    for leg, x in X_COLLAPSE.get(cell, {}).items():
        if minrtp.get(leg) is None:
            continue
        ex = []
        for t in ticks:
            if t[0] < t0 or not (t[1] or 0) > 0:
                continue
            rtt = t[2] if one_leg else t[3 + 3 * leg]
            if rtt is None:
                continue
            ex.append(rtt - minrtp[leg] / 1000.0)
        if ex:
            share = sum(e > x for e in ex) / len(ex)
            out[str(leg)] = {"n": len(ex), "share": share, "flag": share > COLLAPSE_SHARE, "e50": pq(ex, 0.5), "e90": pq(ex, 0.9), "x": x}
    return out


def _attach(ticks, slaw1_nk):
    """s60 section 5, c8 leg-1 sampler attachment: [DIAG]'s leg-1 echo count (`sig_us=../n`, cumulative) against [SLAW] leg 1's nk.
    echo grows and nk = 0 on every window => UNATTACHED; both grow => ATTACHED; neither => STARVED (nk > 0 with a flat count reads ATTACHED: the sampler plainly exists)."""
    ns = [t[8] for t in ticks if t[8] is not None]
    grows = len(ns) >= 2 and ns[-1] > ns[0]
    nk_grows = slaw1_nk > 0
    status = "ATTACHED" if nk_grows else ("UNATTACHED" if grows else "STARVED")
    return {"echo_first": ns[0] if ns else None, "echo_last": ns[-1] if ns else None, "echo_grows": grows, "slaw1_nk_total": slaw1_nk, "status": status}


def _evict(cli, srv, drv, nbytes, cell, mbps, wst, ticks_cum):
    """s60 section 6: the evicting rows' extras: mbar59_score.row_metrics (m-bar lower bound, W1 bracket), per-window adv / dbt / fec columns, [SPAN] delta / a_star, the cum-ack series."""
    import mbar59_score as m59
    import slaw_parse as sp_
    cols = {k: [] for k in ("t", "tw", "adv", "ackw", "dbt_acc", "dbt_em", "dbt_share", "eps", "sigma2", "r", "astar", "fecn")}
    for ln in wst:
        r = sp_.parse_line(ln)
        if not r or r[0] != "WSTORE":
            continue
        d = r[1]
        for k, key in (("t", "t"), ("tw", "tw"), ("adv", "adv.0"), ("ackw", "ackw"), ("dbt_acc", "dbt_acc"), ("dbt_em", "dbt_em"), ("dbt_share", "dbt_share"),
                       ("eps", "fec.0"), ("sigma2", "fec.1"), ("r", "fec.2"), ("astar", "fec.3"), ("fecn", "fec.4")):
            cols[k].append(d.get(key))
    sp = m59.span_lines(cli)
    return {"mbar59": m59.row_metrics(cli, srv, drv, nbytes, cell, mbps), "wstore": cols,
            "span": {"t": [s["t"] for s in sp], "a_star": [s["a_star"] for s in sp], "delta": [s["delta"] for s in sp]},
            "cum": {"t": [c[0] for c in ticks_cum], "src": [c[1] for c in ticks_cum], "ack": [c[2] for c in ticks_cum], "wack": [c[3] for c in ticks_cum]}}


def row_metrics(cli, srv, drv, nbytes=None, cell=C2, arm="G192", mbps=None):
    """The `t60` of one row (docstring). One pass over the client log; a missing or garbled token reads None; never raises on log content."""
    cli, srv = list(cli or []), list(srv or [])
    kind, W = plan60.arm_parse(arm)
    t0 = LATE_T.get(cell, 15.0)
    tags_all = dict.fromkeys(GAUGE_TAGS, 0)
    slaw, ticks, caplog, wstore, cumt = [], [], [], [], []
    tags = dict.fromkeys(GAUGE_TAGS, 0)
    want_evict = kind == "E"
    for ln in cli:
        if WARM in ln:                      # s60 section 1.1: the warm-up cut at the LAST marker; nothing before it is read
            slaw, ticks, caplog, wstore, cumt = [], [], [], [], []
            tags = dict.fromkeys(GAUGE_TAGS, 0)
            continue
        if ln[:1] != "[":
            continue
        end = ln.find("]")
        tag = ln[1:end] if end > 0 else ""
        if tag in tags:
            tags[tag] += 1
            tags_all[tag] += 1
            if tag == "SLAW":
                slaw.append(dict(_TOK.findall(ln)))
            elif tag == "CAPLOG":
                caplog.append(ln)
            elif tag == "WSTORE" and want_evict:
                wstore.append(ln)
        elif tag == "DIAG":
            try:
                tk = _tick(ln)
            except Exception:
                tk = None
            if tk:
                ticks.append(tk)
                if want_evict:
                    m = re.search(r" cum=(\d+)/(\d+)/(\d+)", ln)
                    mw = re.search(r"wake\[[^\]]*? ack=(\d+)", ln)
                    cumt.append((tk[0], tk[1], int(m.group(3)) if m else None, int(mw.group(1)) if mw else None))
    srv_tags = dict.fromkeys(GAUGE_TAGS, 0)
    for ln in srv:
        end = ln.find("]") if ln[:1] == "[" else -1
        if end > 0 and ln[1:end] in srv_tags:
            srv_tags[ln[1:end]] += 1
    try:
        import l1common as lc
        gate = {"cli": lc.gate_tok(cli, "RWM_SLAW"), "srv": lc.gate_tok(srv, "RWM_SLAW")}
    except Exception:
        gate = {"cli": None, "srv": None}
    late = {}
    nwin, boot, ml, qset = {}, {}, {}, set()
    nk_tot = {}
    for d in slaw:
        leg = int(_num(d.get("p")) or 0)
        t = _num(d.get("t"))
        nwin[leg] = nwin.get(leg, 0) + 1
        qset.add(d.get("q", "-"))
        b = boot.setdefault(leg, {"b0k": None, "b0kn": None, "b0p": None, "kw": None})
        if _list(d.get("b0k")) is not None:
            b["b0k"], b["b0kn"] = _list(d.get("b0k")), _list(d.get("b0kn"))
        if _num(d.get("b0p")) is not None:
            b["b0p"] = _num(d.get("b0p"))
        if _num(d.get("kw")) is not None:
            b["kw"] = _num(d.get("kw"))
        nk_tot[leg] = nk_tot.get(leg, 0) + (_num(d.get("nk")) or 0)
        m, l = _num(d.get("m")), _num(d.get("l"))
        if t is not None and t < ML_T and m is not None and l is not None:
            z = ml.setdefault(leg, {"n": 0, "eq": 0, "maxdiff": 0.0})
            z["n"] += 1
            z["eq"] += abs(m - l) <= 5.1e-4
            z["maxdiff"] = max(z["maxdiff"], abs(m - l))
        if t is not None and t >= t0:
            cols = late.setdefault(leg, {k: [] for k in FIELDS})
            for k in FIELDS:
                cols[k].append(_num(d.get(k)))
    minrtp = {}
    for leg in (0, 1):
        v = [t[4 + 3 * leg] for t in ticks if t[4 + 3 * leg]]
        minrtp[leg] = min(v) if v else None
    out = {"v": 1, "cell": cell, "arm": arm, "kind": kind, "W": W,
           "late": {str(k): v for k, v in sorted(late.items())}, "nwin": {str(k): v for k, v in sorted(nwin.items())},
           "boot": {str(k): v for k, v in sorted(boot.items())}, "ml": {str(k): v for k, v in sorted(ml.items())},
           "witness": replay_witness(slaw) if slaw else {"n": 0, "bad": 0, "worst": None, "ok": False, "error": None},
           "caplog": caplog_gate(caplog), "tags": {"cli": tags, "srv": srv_tags, "all_cli": tags_all}, "gate_slaw": gate,
           "q": sorted(qset - {"-"}), "mbps": mbps,
           "diag": {"n_ticks": len(ticks), "min_rtp_us": {str(k): v for k, v in minrtp.items() if v is not None},
                    "collapse": _collapse(ticks, cell, minrtp), "attach": _attach(ticks, nk_tot.get(1, 0)) if 1 in LEGS.get(cell, ()) else None}}
    if want_evict:
        out["evict"] = _evict(cli, srv, drv, nbytes, cell, mbps, wstore, cumt)
    return out


# ───────────────────────────── the scoring side: a row view, validity, calibration, the replays ─────────────────────────────
class RV:
    """A ledger row with its `t60`, as the scorer reads it."""

    def __init__(self, r, order=0):
        self.r, self.t, self.order = r, (r.get("t60") or {}), order
        self.cell, self.arm = r.get("cell"), r.get("arm")
        self.kind, self.W = plan60.arm_parse(self.arm)
        self.rep, self.seed = int(r.get("rep") or 0), str(r.get("seed"))
        self.live = r.get("status") == "LIVE" and bool(self.t)
        self.dnf = bool(r.get("dnf"))
        self._w, self._s = {}, {}

    def legs(self):
        return [int(k) for k in sorted(self.t.get("late", {}))]

    def wins(self, leg):
        if leg not in self._w:
            c = self.t.get("late", {}).get(str(leg))
            self._w[leg] = [dict(zip(FIELDS, vals)) for vals in zip(*(c[k] for k in FIELDS))] if c else []
        return self._w[leg]

    def st(self, leg):
        """Per-leg LATE statistics; the nk >= 10 windows carry the floor-type medians (s60 section 2)."""
        if leg not in self._s:
            ws = self.wins(leg)
            w10 = [w for w in ws if (w["nk"] or 0) >= NK_MIN]
            both = [w["tmin"] - w["m"] for w in w10 if w["tmin"] is not None and w["m"] is not None]
            self._s[leg] = {"n": len(ws), "n10": len(w10), "tmin": med([w["tmin"] for w in w10]), "l": med([w["l"] for w in w10]), "sig": med([w["sig"] for w in w10]),
                            "brake": med(both), "err": med([w["err"] for w in w10]), "tp": med([w["tp50"] for w in w10]), "m": med([w["m"] for w in w10]),
                            "R": med([w["R"] for w in ws]), "q": med([w["q"] for w in ws]) or Q_BULK,
                            "nk": sum(w["nk"] or 0 for w in ws)}
        return self._s[leg]

    def boot(self, leg):
        return self.t.get("boot", {}).get(str(leg)) or {}

    def b0(self, leg, K=K_BOOT):
        """b0p(K): leg 0 = the max of the first K b0k brakes; leg 1 = the max over the first K entries with b0kn >= 10, needing >= 4 such entries (s60 section 2)."""
        b = self.boot(leg)
        ks, kn = b.get("b0k"), b.get("b0kn")
        if not ks:
            return None
        if leg == 0:
            v = [x for x in ks[:K] if x is not None]
            return max(v) if v else None
        v = [x for i, x in enumerate(ks[:K]) if x is not None and i < len(kn or []) and (kn[i] or 0) >= B0KN_MIN]
        return max(v) if len(v) >= LEG1_MIN_ENTRIES else None

    def complete(self, leg, K=K_BOOT):
        """(a) boot complete: K brakes, and leg-0 b0kn >= 10 on each; leg 1 as in b0()."""
        b = self.boot(leg)
        ks, kn = b.get("b0k"), b.get("b0kn")
        if not ks or len(ks) < K:
            return False
        if leg == 0:
            return all(x is not None for x in ks[:K]) and kn is not None and len(kn) >= K and all((x or 0) >= B0KN_MIN for x in kn[:K])
        return self.b0(1, K) is not None

    def collapsed(self, leg=0):
        c = (self.t.get("diag") or {}).get("collapse", {}).get(str(leg))
        return None if c is None else c["flag"]

    def latched(self, leg, K=K_BOOT):
        """s60 section 2: LATCHED iff b0p(K) - median_LATE(brake) > q * l_row + omega-bar (formula-first: the brake the boot read, less the brake the load reads, against what the allowance covers).
        l_row is the row's own median LATE l (DECISION 4)."""
        b0, s = self.b0(leg, K), self.st(leg)
        if b0 is None or s["brake"] is None or s["l"] is None:
            return None
        return b0 - s["brake"] > s["q"] * s["l"] + OMEGA.get(self.cell, {}).get(leg, 0.0)

    def gp(self):
        if self.dnf:
            return 0.0
        return self.r.get("mbps")


def valid_for_cal(rv, leg, K=K_BOOT):
    """s60 section 2: a calibration row is valid iff (a) boot complete, (b) not LATCHED, (c) not COLLAPSED. -> (ok, reason). Leg 1 is also removed by a leg-0 collapse (the row state is read on leg 0)."""
    s = rv.st(leg)
    if not rv.live:
        return False, "void"
    if s["n10"] == 0 or s["tmin"] is None or s["l"] is None:
        return False, "no-late-nk10-windows"
    if rv.dnf and s["n"] < MIN_DNF_LATE_WIN:
        return False, "DNF-too-few-late-windows"      # a DNF row is a datum; it calibrates only if it still holds >= 30 s of LATE windows (the replacement, if any, is the slot's counting row)
    if not rv.complete(leg, K):
        return False, "boot-incomplete"
    lt = rv.latched(leg, K)
    if lt is None:
        return False, "latch-unreadable"
    if lt:
        return False, "LATCHED"
    flags = [rv.collapsed(0)] + ([rv.collapsed(1)] if leg == 1 else [])
    if any(flags):
        return False, "COLLAPSED"
    return True, ""


def pool_share(rows, leg):
    """The share of a floor pool's LATE windows (all its live rows) with nk >= 10 on `leg` (s60 section 2: >= 80 % qualifies the pool; DECISION 5: pooled over the pool's rows)."""
    n = sum(rv.st(leg)["n"] for rv in rows if leg in rv.legs())
    n10 = sum(rv.st(leg)["n10"] for rv in rows if leg in rv.legs())
    return (n10 / n) if n else None


def calibrate(rvs, cell, leg, K=K_BOOT, nogo=(), only=None):
    """The session calibration of one cell and leg (s60 section 2). `only` (a list of rows) is the per-replicate pairing of the drift fallback.
    -> {leg, W, pool_share{W: share}, rows, valid, reasons, F, l, b0, T, n, ok, moved, rows_vals}. ok iff >= MIN_CAL_ROWS valid rows (>= 1 under pairing)."""
    pools = plan60.FLOOR_W[cell]
    cands = pools + ((STARVED_LEG1_W,) if (cell == C8 and leg == 1) else ())       # s60 section 5: a starved c8 leg 1 takes the smallest ARM where it qualifies (256 at the latest)
    sel = {W: [rv for rv in rvs if rv.arm == "G%d" % W and rv.live and leg in rv.legs()] for W in cands}
    shares = {W: pool_share(sel[W], leg) for W in cands}
    chosen, skipped = None, []
    if cell == C2 and "c2" in nogo:                    # NO-GO c2: the floor pool moves to W 32 (s60 section 5)
        chosen = pools[1]
    else:
        for W in cands:
            if shares[W] is None or shares[W] < SHARE_NK:
                continue
            if sel[W] and all(rv.collapsed(0) for rv in sel[W]):       # s60 section 2: a floor pool that collapses on every row moves to the next pool
                skipped.append((W, "collapsed on every row"))
                continue
            chosen = W
            break
    out = {"leg": leg, "W": chosen, "pool_share": shares, "rows": [], "valid": [], "reasons": {}, "F": None, "l": None, "b0": None, "T": None, "n": 0, "ok": False,
           "moved": chosen is not None and chosen != pools[0], "rows_vals": [], "skipped": skipped, "starved_ext": chosen == STARVED_LEG1_W and cell == C8 and leg == 1}
    if chosen is None:
        return out
    rows = sel[chosen] if only is None else [rv for rv in only if rv.arm == "G%d" % chosen]
    out["rows"] = rows
    for rv in rows:
        ok, why = valid_for_cal(rv, leg, K)
        out["reasons"][rv.order] = why
        if ok:
            out["valid"].append(rv)
    vals = []
    for rv in out["valid"]:
        s = rv.st(leg)
        b0 = rv.b0(leg, K)
        vals.append((rv, s["tmin"], s["l"], b0, s["l"] + b0))
    out["rows_vals"] = vals
    out["n"] = len(vals)
    need = MIN_CAL_ROWS if only is None else 1
    out["ok"] = len(vals) >= need
    if vals:
        out["F"], out["l"] = med([v[1] for v in vals]), med([v[2] for v in vals])
        out["b0"], out["T"] = med([v[3] for v in vals]), med([v[4] for v in vals])
    return out


def _tup_r(w, c, cell, mode):
    """One leg's (w, T, Tmin, m, l, b0, sig) for slaw_r, or None if a printed field is missing. Scored recipe (s60 section 1.1): b0 = b0_c and, at c2 / c8, l = T_c - b0_c;
    at c1s (and in mode 'own', the reported own-l R~) the row's own l."""
    if None in (w["tp50"], w["tmin"], w["m"], w["sig"], w["nk"]):
        return None
    if mode == "r" and cell != C1S:
        l = c["T"] - c["b0"]
    else:
        l = w["l"]
    if l is None:
        return None
    return (w["nk"], w["tp50"], w["tmin"], w["m"], l, c["b0"], w["sig"])


def replay_row(rv, cell, cal, mode="r", legs=None):
    """The replayed R~ (mode 'r' scored, 'own' own-l) or R~67 (mode 'r67') per LATE window, and the row median (s60 section 1.1).
        R~   = sum_i w_i (F_i + sig_i + q*l_i + T_i - Tmin_i) / sum_i w_i T_i,  F_i = min(m_i, l_i + b0_i) + b0_i + 1/2 max(0, max_j (l_j + b0_j) - (l_i + b0_i))      [s60replay.slaw_r]
        R~67 = sum_i w_i (F_i + 1/2 max(0, max_j F_j - F_i) + sig_i + q*l_c,i + T_i - Tmin_i) / sum_i w_i T_i       [s60replay.r67_r]
    `cal` = {leg: calibration dict (F, l, b0, T)}. -> {"med", "vals", "bind": (n_bind, n_terms), "n"} or None if a live leg has no calibration."""
    sr, _, _ = _lib()
    if sr is None:
        return None
    ls = list(legs) if legs is not None else rv.legs()
    wins = {leg: {int(w["k"]): w for w in rv.wins(leg) if w["k"] is not None} for leg in ls}
    vals, nb, nt = [], 0, 0
    for k in sorted(set().union(*[set(m) for m in wins.values()])):
        live = [(leg, wins[leg][k]) for leg in ls if k in wins[leg] and (wins[leg][k]["nk"] or 0) > 0]
        if not live:
            continue
        tup, ok = [], True
        for leg, w in live:
            c = cal.get(leg)
            if not c or c.get("b0") is None:
                return None
            if mode == "r67":
                if None in (w["tp50"], w["tmin"], w["sig"]) or c.get("F") is None:
                    ok = False
                    break
                tup.append((w["nk"], w["tp50"], w["tmin"], w["sig"], c["F"], c["l"]))
            else:
                t = _tup_r(w, c, cell, mode)
                if t is None:
                    ok = False
                    break
                tup.append(t)
        if not ok:
            continue
        q = live[0][1]["q"] if live[0][1]["q"] is not None else Q_BULK
        if mode == "r67":
            vals.append(sr.r67_r(tup, q))
        else:
            vals.append(sr.slaw_r(tup, q))
            nb += sum(1 for x in tup if x[3] >= x[4] + x[5])          # the replay's anchor binds iff m >= l + b0 (s60 section 3.2: NOT the printed anc)
            nt += len(tup)
    if not vals:
        return None
    return {"med": med(vals), "vals": vals, "bind": (nb, nt), "n": len(vals)}


def cross_down(pts):
    """The W where the piecewise-linear [(W, v)] falls through 1 (r67floor.py rcell: r0 >= 1 > r1); '<W0' if the first point is already below, None if it never does."""
    pts = sorted(pts)
    if not pts:
        return None
    if pts[0][1] < 1:
        return "<%d" % pts[0][0]
    for (w0, r0), (w1, r1) in zip(pts, pts[1:]):
        if r0 >= 1 > r1:
            return w0 + (r0 - 1) * (w1 - w0) / (r0 - r1)
    return ">%d" % pts[-1][0]


def band_word(x, band):
    if not isinstance(x, float):
        return "n/a"
    return "BELOW" if x < band[0] else ("ABOVE" if x > band[1] else "IN")


class Session:
    """The scored readouts of one `score60` call. Every method prints its lines and appends (name, label) pairs."""

    def __init__(self, cnt, replaced, cells, out, nogo, slots_info):
        self.out, self.nogo, self.cells, self.labels = out, set(nogo), cells, []
        self.replaced, self.slots_info = replaced, slots_info
        self.rvs = [RV(r, i) for i, r in enumerate(cnt)]
        self.by = {c: [rv for rv in self.rvs if rv.cell == c and rv.kind in ("G", "C")] for c in plan60.CELLS}
        self.cal, self.gate, self.fallback, self._rt, self._pair = {}, {}, {}, {}, {}
        self.void = [rv for rv in self.rvs if not rv.live]
        self.counters = {"w95_clamped": 0, "w95_n": 0, "interp_clamped": 0, "interp_n": 0}

    def p(self, s=""):
        self.out(s)

    def label(self, name, lab, note=""):
        self.labels.append((name, lab))
        self.p(f"RESULT {name} {lab}" + (f"  ({note})" if note else ""))

    # ---- replays (cached; a row failing the replay witness is VOID for every R reading, s60 section 1.1) ----
    def rt(self, rv, mode, cell=None, legs=None, cal=None):
        cell = cell or rv.cell
        key = (rv.order, mode, tuple(legs) if legs else None, id(cal) if cal is not None else self.fallback.get(cell))
        if key not in self._rt:
            r = None
            if rv.live and rv.t.get("witness", {}).get("ok") is True:
                c = cal if cal is not None else self.calmap(cell, rv)
                if c:
                    r = replay_row(rv, cell, c, mode, legs)
            self._rt[key] = r
        return self._rt[key]

    def calmap(self, cell, rv=None):
        """{leg: calibration} for the replay, or None if a leg of the cell has none (then no R~ at the cell). Under the drift fallback each replicate uses its own floor row (s60 section 2)."""
        if self.fallback.get(cell) and rv is not None:
            key = (cell, rv.rep, rv.seed)
            if key not in self._pair:
                rows = [x for x in self.by[cell] if x.rep == rv.rep and x.seed == rv.seed]
                cs = {leg: calibrate(self.by[cell], cell, leg, nogo=self.nogo, only=rows) for leg in LEGS[cell]}
                self._pair[key] = cs if all(c["ok"] for c in cs.values()) else None
            return self._pair[key]
        cs = self.cal.get(cell)
        return cs if cs and all(c["ok"] for c in cs.values()) else None

    # ---- section: rows ----
    def rows_table(self):
        self.p("ROWS (counting): cell arm seed rep | status dnf | witness caplog | collapsed latched(K=8) | goodput | LATE nk>=10 windows leg0")
        for rv in self.rvs:
            if rv.cell not in self.cells and rv.arm != plan60.EVICT_ARM:
                continue
            w = rv.t.get("witness") or {}
            self.p(f"  {SHORT.get(rv.cell, rv.cell):<3} {rv.arm:<6} s{rv.seed:<3} r{rv.rep:<2} {str(rv.r.get('status')):<10} dnf={int(rv.dnf)} wit={w.get('ok')} cap={(rv.t.get('caplog') or {}).get('ok')} "
                   f"coll={rv.collapsed(0)} latch={rv.latched(0) if rv.t else None} gp={f(rv.r.get('mbps'), 1)} n10={rv.st(0)['n10'] if rv.t and 0 in rv.legs() else None}"
                   + (f" T60-ERROR {rv.r.get('t60_error')}" if rv.r.get("t60_error") else ""))
        for r in self.replaced:
            self.p(f"  REPLACED {r.get('cell')} {r.get('arm')} s{r.get('seed')} rep{r.get('rep')} {r.get('status')} {' '.join(r.get('problems') or [])}")
            if r.get("dnf"):
                lw = len(((r.get("t60") or {}).get("late") or {}).get("0", {}).get("k") or [])
                self.p(f"  DNF-DATUM {r.get('cell')} {r.get('arm')} s{r.get('seed')} rep{r.get('rep')}: the DNF row is kept as a datum ({lw} LATE windows on leg 0); its replacement is the slot's counting row (s60 section 2: 'a DNF is a datum and the row is re-run once')")
        for rv in self.void:
            self.p(f"  VOID {rv.cell} {rv.arm} s{rv.seed} rep{rv.rep} {rv.r.get('status')} {' '.join(rv.r.get('problems') or [])}")
        bad = [rv for rv in self.rvs if rv.live and rv.kind in ("G", "E") and rv.t.get("witness", {}).get("ok") is not True]       # a C row prints no [SLAW]: nothing to witness
        for rv in bad:
            self.p(f"  WITNESS-VOID {rv.cell} {rv.arm} s{rv.seed} rep{rv.rep}: the per-row replay witness {(rv.t.get('witness') or {})} -> VOID for every R reading")

    # ---- section: session calibration, validity, B1, drift ----
    def calibration(self):
        self.p("SESSION CALIBRATION (s60 section 2): per cell and leg, the floor pool = the smallest of the three at which the leg has >= 80 % of its LATE windows with nk >= 10")
        for cell in self.cells:
            self.cal[cell] = {}
            for leg in LEGS[cell]:
                c = calibrate(self.by[cell], cell, leg, nogo=self.nogo)
                self.cal[cell][leg] = c
                sh = " ".join(f"W{W}:{f(s, 2)}" for W, s in c["pool_share"].items())
                self.p(f"  CAL {SHORT[cell]} leg{leg}: pool-share[{sh}] -> floor pool W={c['W']}{' (MOVED from the smallest pool)' if c['moved'] else ''}"
                       f"{' (NO-GO c2: forced to W32)' if (cell == C2 and 'c2' in self.nogo) else ''}"
                       f"{' (STARVED at the floor pools: leg 1 takes arm W%d, s60 section 5)' % c['W'] if c['starved_ext'] else ''}"
                       f"{''.join(' (pool W%d skipped: %s)' % x for x in c['skipped'])}")
                if c["W"] is None:
                    self.p(f"  CAL {SHORT[cell]} leg{leg}: UNSCOREABLE - no floor pool reaches 80 % of LATE windows with nk >= 10")
                    continue
                reasons = {}
                for rv in c["rows"]:
                    reasons.setdefault(c["reasons"][rv.order] or "valid", []).append(f"s{rv.seed}r{rv.rep}")
                why = "; ".join(k + ": " + " ".join(v) for k, v in sorted(reasons.items()))
                self.p(f"  CAL {SHORT[cell]} leg{leg}: rows {len(c['rows'])} valid {c['n']} ({why})")
                self.p(f"  CAL {SHORT[cell]} leg{leg}: F_c={f(c['F'], 3)} l_c={f(c['l'], 3)} b0_c={f(c['b0'], 3)} T_c={f(c['T'], 3)} ms (medians over the valid non-latched rows; "
                       f"{'OK' if c['ok'] else 'UNSCOREABLE: fewer than %d valid rows' % MIN_CAL_ROWS})")
                for rv in c["rows"]:
                    z = ((rv.t.get("diag") or {}).get("collapse") or {}).get(str(leg))
                    if z:
                        self.p(f"    excess E50={f(z['e50'])} E90={f(z['e90'])} ms vs X={z['x']} (collapse share {f(z['share'], 3)}) s{rv.seed}r{rv.rep}")
                flagged = [rv for rv in c["rows"] if rv.collapsed(0)]
                self.p(f"  CAL {SHORT[cell]} leg{leg}: collapse flag share on the calibration rows {len(flagged)}/{len(c['rows'])} (reported; the flag removes a row from the medians, s60 section 2(c))")
                lat = [rv for rv in c["rows"] if rv.latched(leg)]
                self.p(f"  CAL {SHORT[cell]} leg{leg}: LATCHED {len(lat)}/{len(c['rows'])} rows (omega-bar {OMEGA[cell][leg]} ms)")
            self.b1(cell)
            self.drift(cell)

    def b1(self, cell):
        vals = [(rv, leg, rv.st(leg)["err"]) for leg in LEGS[cell] for rv in (self.cal[cell][leg]["valid"] if self.cal[cell].get(leg) else []) if rv.st(leg)["err"] is not None]
        if not vals:
            self.label(f"B1-{SHORT[cell]}", "UNSCOREABLE", "no valid calibration row with a median LATE err")
            return
        fire = [v for v in vals if v[2] > 0]
        self.p(f"  B1 {SHORT[cell]}: median LATE err per valid calibration row: {' '.join(f(v[2], 3) for v in vals)} (predicted not to fire)")
        self.label(f"B1-{SHORT[cell]}", "FIRES" if fire else "DOES-NOT-FIRE", f"{len(fire)} of {len(vals)} rows err > 0")

    def drift(self, cell):
        """s60 section 2: per cell and leg, Spearman of the calibration rows' b0p and l against run order, with a permutation p; fallback to per-replicate pairing iff p < 0.05
        AND the fitted change across the session exceeds q * l_c (either series)."""
        trig = False
        for leg in LEGS[cell]:
            c = self.cal[cell].get(leg)
            if not c or c["n"] < 4:
                self.p(f"  DRIFT {SHORT[cell]} leg{leg}: UNSCOREABLE ({c['n'] if c else 0} calibration rows; the check needs >= 4)")
                continue
            order = [float(v[0].order) for v in c["rows_vals"]]
            for name, idx in (("b0p", 3), ("l", 2)):
                ys = [v[idx] for v in c["rows_vals"]]
                rho, pv = spearman(order, ys), perm_p(order, ys)
                fit = ols(order, ys)
                change = fit[0] * (max(order) - min(order)) if fit else 0.0
                hit = pv < 0.05 and abs(change) > Q_BULK * c["l"]
                trig = trig or hit
                self.p(f"  DRIFT {SHORT[cell]} leg{leg} {name}: Spearman {rho:+.2f} p={pv:.3f} fitted change {change:+.3f} ms vs q*l_c {Q_BULK * c['l']:.3f} -> {'FALLBACK' if hit else 'no drift detected'}")
        self.fallback[cell] = trig
        if trig:
            self.p(f"  DRIFT {SHORT[cell]}: R~ switches to PER-REPLICATE pairing (each replicate with its own calibration row); the session value is the sensitivity below")

    # ---- section: floor gate ----
    def floor_gate(self):
        self.p("FLOOR GATE (s60 section 2): per leg, the least-squares slope b of the per-row median LATE tmin against W over the three floor pools; E = b*W1")
        for cell in self.cells:
            for leg in LEGS[cell]:
                rows = [rv for rv in self.by[cell] if plan60.is_floor(cell, rv.arm) and rv.kind == "G" and rv.live and leg in rv.legs() and rv.st(leg)["tmin"] is not None
                        and rv.st(leg)["n10"] > 0 and not rv.collapsed(0)]
                xs, ys = [float(rv.W) for rv in rows], [rv.st(leg)["tmin"] for rv in rows]
                tag = SHORT[cell] + (f"-leg{leg}" if cell == C8 else "")
                fit = ols(xs, ys) if len(set(xs)) >= 2 else None
                c = self.cal.get(cell, {}).get(leg)
                if not fit:
                    self.p(f"  GATE {tag}: UNSCOREABLE ({len(rows)} rows over {len(set(xs))} pools)")
                    self.gate[(cell, leg)] = {"slope": "UNSCOREABLE", "conv": "UNSCOREABLE"}
                    self.label(f"FLOOR-SLOPE-{tag}", "UNSCOREABLE")
                    self.label(f"FLOOR-CONV-{tag}", "UNSCOREABLE")
                    continue
                b, a, se = fit
                slope = "FLAT" if b + 2 * se < SLOPE_FLAT else ("SLOPED" if b - 2 * se > SLOPE_FLAT else "UNRESOLVED")
                W1 = float(c["W"] if c and c["W"] else min(xs))
                E, seE = b * W1, W1 * se
                tau = TAU.get(cell, {}).get(leg)
                if tau is None:
                    conv = "NO-TAU-REGISTERED"
                else:
                    conv = "CONVERGED" if E + 2 * seE <= tau else ("NOT-CONVERGED" if E - 2 * seE > tau else "UNRESOLVED")
                ext = (c["F"] - E) if (c and c["F"] is not None) else None
                self.gate[(cell, leg)] = {"slope": slope, "conv": conv, "b": b, "se": se, "E": E, "seE": seE}
                self.p(f"  GATE {tag}: n={len(rows)} rows, b={b:.4f} +- {se:.4f} ms/slot (flat iff b+2se < {SLOPE_FLAT}), E=b*W1={E:.2f} +- {seE:.2f} ms at W1={W1:.0f}, tau={tau}; "
                       f"F_c={f(c['F'], 3) if c else '-'} extrapolated floor F_c-E={f(ext, 3)} (reported, not scored)")
                self.label(f"FLOOR-SLOPE-{tag}", slope, f"b={b:.4f}")
                self.label(f"FLOOR-CONV-{tag}", conv, f"E={E:.2f}+-{seE:.2f}")
                if cell == C1S:
                    self.p("  GATE c1s: the gate does not cover c1s's scored sign (R~ is fed b0_c and the row's own l); it bears only on R~67 at c1s (s60 section 2)")

    # ---- section: per-arm replays ----
    def arms(self):
        self.p("ARMS (per cell: medians over the live rows of an arm; R~ = the scored recipe, R~own = the row's own l, R~67 = the R6.7-form beside it; printed R for P2)")
        for cell in self.cells:
            ws = sorted({rv.W for rv in self.by[cell] if rv.kind == "G"})
            for W in ws:
                rows = [rv for rv in self.by[cell] if rv.W == W and rv.kind == "G" and rv.live]
                if not rows:
                    continue
                r = [self.rt(rv, "r") for rv in rows]
                o = [self.rt(rv, "own") for rv in rows]
                r67 = [self.rt(rv, "r67") for rv in rows]
                binds = [x["bind"][0] / x["bind"][1] for x in r if x and x["bind"][1]]
                prR = med([rv.st(0)["R"] for rv in rows if 0 in rv.legs()])
                line = (f"  ARM {SHORT[cell]} W{W:<5} n={len(rows)} R~={f(med([x['med'] for x in r if x]), 4)} R~own={f(med([x['med'] for x in o if x]), 4)} "
                        f"R~67={f(med([x['med'] for x in r67 if x]), 4)} bind={f(med(binds), 2)} printedR={f(prR, 4)}{' (>1)' if prR is not None and prR > 1 else ''} "
                        f"b0p={f(med([rv.b0(0) for rv in rows]), 3)} l={f(med([rv.st(0)['l'] for rv in rows if 0 in rv.legs()]), 3)} tmin={f(med([rv.st(0)['tmin'] for rv in rows if 0 in rv.legs()]), 3)}")
                sc_ = self.cal.get(cell)
                if self.fallback.get(cell) and sc_ and all(c["ok"] for c in sc_.values()):
                    line += f" | session-value sensitivity R~={f(med([(self.rt(rv, 'r', cal=sc_) or {}).get('med') for rv in rows]), 4)} (the drift fallback is on: R~ above is per-replicate paired)"
                if cell == C8:
                    l1 = [rv for rv in rows if 1 in rv.legs()]
                    pull = []
                    for rv in l1:
                        both, only0 = self.rt(rv, "r"), self.rt(rv, "r", legs=(0,))
                        if both and only0:
                            pull.append(both["med"] - only0["med"])
                    line += f" | leg1 sig={f(med([rv.st(1)['sig'] for rv in l1]), 3)} pull on R~={f(med(pull), 4)} (A.5 readout)"
                self.p(line)

    def r_sign(self):
        """s60 section 5: R~ < 1 at the c2 / c8 controls. The doc reads it 'after replicate 2', which cannot be computed (2 floor rows < MIN_CAL_ROWS = 3): it is read at the END of the launch on the FULL calibration,
        on every control row (Amendment 1). Any control R~ >= 1 is a STOP, and 60b's launch checks this label in 60a's score before any row (loopdiag_run_all.sh: ABORT-RSIGN)."""
        for cell in (C2, C8):
            if cell not in self.cells:
                continue
            ctl = [rv for rv in self.by[cell] if rv.W == plan60.CONTROL_W[cell] and rv.kind == "G" and rv.live]
            vals = [(rv, self.rt(rv, "r")) for rv in sorted(ctl, key=lambda x: (x.rep, x.order))]
            have = [(rv, r["med"]) for rv, r in vals if r]
            if not have:
                self.label(f"R-SIGN-{SHORT[cell]}", "UNSCOREABLE", "no control row with a replayable R~ (calibration or control rows missing)")
                continue
            self.p(f"  R-SIGN {SHORT[cell]}: control R~ by replicate: {' '.join(f's{rv.seed}r{rv.rep}={v:.4f}' for rv, v in have)}")
            if cell == C8 and "c8" in self.nogo:      # Amendment 1: a c8 NO-GO means its floor failed go/no-go, so its R~ is reported and never gates 60b
                self.label("R-SIGN-c8", "UNSCOREABLE", "NO-GO c8: the floor failed the rehearsal go/no-go; control R~ reported, not gating")
                continue
            bad = [(rv, v) for rv, v in have if v >= 1]
            for rv, v in bad:
                self.p(f"STOP R-SIGN {SHORT[cell]} s{rv.seed} rep{rv.rep}: control R~ = {v:.4f} >= 1 (the registered gate is R~ < 1 at the control, read at the end of 60a on the full calibration; 60b does not launch and the session is amended)")
            self.label(f"R-SIGN-{SHORT[cell]}", "STOP" if bad else ("PASS" if len(have) >= R_SIGN_MIN else "UNSCOREABLE"), f"{len(have)} control rows (PASS needs >= {R_SIGN_MIN}), read at the end of the launch on the full calibration")

    # ---- section: P1 / P2 / P3 ----
    def p123(self):
        for cell in self.cells:
            W = plan60.P1_W[cell]
            c0 = self.cal.get(cell, {}).get(0)
            arm = [rv for rv in self.by[cell] if rv.W == W and rv.kind == "G" and rv.live and rv.b0(0) is not None]
            base = c0["b0"] if c0 and c0["ok"] else None
            if not arm or base is None or base <= 0:
                self.p(f"  P1 {SHORT[cell]}: UNSCOREABLE ({len(arm)} arm rows with a b0p; floor-pool b0p {f(base, 3)})")
                self.label(f"P1-{SHORT[cell]}", "UNSCOREABLE")
            else:
                ratio = med([rv.b0(0) for rv in arm]) / base
                self.p(f"  P1 {SHORT[cell]}: b0p(W{W})/b0p(floor pool W{c0['W']}) = {med([rv.b0(0) for rv in arm]):.3f}/{base:.3f} = {ratio:.2f} (registered >= {P1_RATIO:g}; a failure is reported only)")
                self.label(f"P1-{SHORT[cell]}", "HOLDS" if ratio >= P1_RATIO else "FAILS", f"ratio {ratio:.2f}")
        for cell in (C2, C8):
            if cell not in self.cells:
                continue
            for leg in LEGS[cell]:
                W = plan60.P1_W[cell]
                c = self.cal.get(cell, {}).get(leg)
                name = f"P3-{SHORT[cell]}" + (f"l{leg}" if cell == C8 else "")
                arm = [rv.st(leg)["l"] for rv in self.by[cell] if rv.W == W and rv.kind == "G" and rv.live and leg in rv.legs() and rv.st(leg)["l"] is not None]
                fl = [v[2] for v in c["rows_vals"]] if c and c["ok"] else []
                if len(arm) < 2 or len(fl) < 2:
                    self.p(f"  P3 {name}: UNSCOREABLE (arm rows {len(arm)}, floor rows {len(fl)})")
                    self.label(name, "UNSCOREABLE")
                    continue
                d = med(arm) - med(fl)
                se = math.sqrt(sd(arm) ** 2 / len(arm) + sd(fl) ** 2 / len(fl))
                thr = Q_BULK * c["l"]
                lab = "RISES" if d - 2 * se > thr else ("FLAT" if d + 2 * se < thr else "UNRESOLVED")
                self.p(f"  P3 {name}: delta_l = {med(arm):.3f} - {med(fl):.3f} = {d:+.3f} ms, SE_delta {se:.3f}, q*l_c {thr:.3f} (RISES iff d-2SE > q*l_c, FLAT iff d+2SE < q*l_c)")
                self.label(name, lab, f"delta_l {d:+.3f}")
            ml = [(rv, z) for rv in self.by[cell] if rv.kind == "G" for z in [rv.t.get("ml", {}).get("0")] if z]
            if ml:
                self.p(f"  P3 witness {SHORT[cell]}: m = l for t < 10 s on {sum(z['eq'] for _, z in ml)}/{sum(z['n'] for _, z in ml)} windows (max |m-l| {max(z['maxdiff'] for _, z in ml):.4f} ms)")

    # ---- section: replicates, knee, brackets ----
    def rep_map(self, cell):
        m = {}
        for rv in self.by[cell]:
            if rv.kind == "G" and rv.live and 0 in rv.legs():
                m.setdefault((rv.rep, rv.seed), {})[rv.W] = rv
        return dict(sorted(m.items()))

    def knee(self, cell, wm):
        """W95 for one replicate: the goodput against the same replicate's control crossing 0.95 (s60 section 3.1)."""
        ctl = wm.get(plan60.CONTROL_W[cell])
        gc = ctl.gp() if ctl else None
        if not gc:
            return None
        pts = [(W, rv.gp() / gc) for W, rv in wm.items() if W != plan60.CONTROL_W[cell] and rv.gp() is not None]
        w, cl = cross_up(pts, KNEE_FRAC)
        self.counters["w95_n"] += 1
        self.counters["w95_clamped"] += bool(cl)
        return w

    def at_knee(self, cell, wm, w95, fn):
        """fn(row) interpolated at W95 over the replicate's knee-bracket arms (+ the P1 arm); -> (value, (lo_row, hi_row)) or None."""
        arms = [W for W in plan60.KNEE_W.get(cell, ()) + (plan60.P1_W[cell],) if W in wm]
        pts = [(W, fn(wm[W])) for W in arms if fn(wm[W]) is not None]
        v, cl, br = interp(pts, w95)
        self.counters["interp_n"] += 1
        self.counters["interp_clamped"] += bool(cl)
        if v is None:
            return None
        return v, (wm[br[0][0]], wm[br[1][0]])

    # ---- section: c2 allowance shortfall D ----
    def c2_short(self):
        cell = C2
        c = self.cal.get(cell, {}).get(0)
        if cell not in self.cells:
            return
        if not c or not c["ok"]:
            self.p("  C2-D: UNSCOREABLE (no floor calibration)")
            self.label("C2-SHORT", "UNSCOREABLE", "fewer than 3 valid floor rows")
            return
        F, l_c = c["F"], c["l"]
        items, bracket, wit = [], [], []
        for (rep, seed), wm in self.rep_map(cell).items():
            w95 = self.knee(cell, wm)
            if w95 is None:
                self.p(f"  C2-D s{seed}r{rep}: replicate not scoreable (no control goodput or the goodput never reaches {KNEE_FRAC} of it)")
                continue
            tm = self.at_knee(cell, wm, w95, lambda rv: rv.st(0)["tmin"])
            if not tm:
                continue
            rows = tm[1]
            sig = med([w["sig"] for rv in set(rows) for w in rv.wins(0) if (w["nk"] or 0) >= NK_MIN])
            if sig is None:
                continue
            D = (tm[0] - F) - (sig + Q_BULK * l_c)
            items.append((rep, seed, tm[0], sig, D, w95))
            ka = sorted(W for W in plan60.KNEE_W[cell] if W in wm)
            lo_arm = max([W for W in ka if W <= w95], default=(ka[0] if ka else None))
            hi_arm = min([W for W in ka if W >= w95], default=(ka[-1] if ka else None))
            br = []
            for W in (lo_arm, hi_arm):
                rv = wm.get(W)
                s = med([w["sig"] for w in rv.wins(0) if (w["nk"] or 0) >= NK_MIN]) if rv else None
                br.append(((rv.st(0)["tmin"] - F) - (s + Q_BULK * l_c)) if (rv and s is not None and rv.st(0)["tmin"] is not None) else None)
            bracket.append((rep, seed, w95, lo_arm, hi_arm, br[0], br[1], br[0], tm[0], sig))
            # The sign witness (s60 section 1.1): R~67 on ONE leg with the same F_c, sigma, q*l_c as D, so R~67(W95) - 1 = -D_r / T exactly. It is computed through s60replay.r67_r from the SAME session calibration (F_c, l_c)
            # and the SAME window set as D: the nk >= 10 windows' medians of tmin / sig / tp50, interpolated at the knee like tmin (a per-window median of ratios would differ from D's ratio of medians).
            tp = self.at_knee(cell, wm, w95, lambda rv: rv.st(0)["tp"])
            sr, _, _ = _lib()
            if tp and sr is not None and tp[0]:
                r67v = sr.r67_r([(1.0, tp[0], tm[0], sig, F, l_c)], Q_BULK)
                if abs(D) < D_WIT_RES:
                    wit_txt = f"UNRESOLVED (|D_r| < {D_WIT_RES} ms, under the witness's resolution)"
                else:
                    s_ok = (r67v - 1 > 0) == (D < 0)
                    wit.append(s_ok)
                    wit_txt = "ok" if s_ok else "MISMATCH"
                    if not s_ok:
                        self.p(f"SCORER-BUG c2 s{seed}r{rep}: sign(R~67(W95)-1) = {'+' if r67v > 1 else '-'} but -sign(D_r) = {'+' if D < 0 else '-'}; the identity R~67-1 = -D/T on one leg is broken")
                self.p(f"  C2-D s{seed}r{rep}: W95={w95:.1f} Tmin(W95)={tm[0]:.3f} F_c={F:.3f} sig={sig:.3f} q*l_c={Q_BULK * l_c:.3f} -> D_r={D:+.3f} ms; R~67(W95)-1={r67v - 1:+.4f} (sign witness {wit_txt})")
            else:
                self.p(f"  C2-D s{seed}r{rep}: W95={w95:.1f} Tmin(W95)={tm[0]:.3f} F_c={F:.3f} sig={sig:.3f} q*l_c={Q_BULK * l_c:.3f} -> D_r={D:+.3f} ms (R~67 sign witness not evaluable)")
        self.p(f"  C2-D: W95 clamped at the first goodput arm on {self.counters['w95_clamped']}/{self.counters['w95_n']} replicates; interpolation clamped at an end arm on {self.counters['interp_clamped']}/{self.counters['interp_n']}")
        if len(items) < D_MIN_REPS:
            self.label("C2-SHORT", "UNSCOREABLE", f"{len(items)} scoreable replicates, need {D_MIN_REPS}")
            return
        floors = sorted([(v[1], v[2]) for v in c["rows_vals"]])
        its = sorted([(i[2], i[3]) for i in items])
        lo, hi = boot_D(its, floors)
        Dm = med([i[4] for i in items])
        conv = self.gate.get((cell, 0), {}).get("conv")
        if lo > 0:
            lab = "SHORT"
        elif hi < 0:
            lab = "COVERED" if conv == "CONVERGED" else "UNRESOLVED"
        else:
            lab = "UNRESOLVED"
        inad = hi < 0 and conv != "CONVERGED"
        # bracket arms (reported; SHORT is read on the lower arm as well)
        lower = [b[5] for b in bracket if b[5] is not None]
        upper = [b[6] for b in bracket if b[6] is not None]
        lo_items = []
        for b in bracket:
            if b[5] is not None:
                wm = self.rep_map(cell)[(b[0], b[1])]
                rv = wm[b[3]]
                lo_items.append((rv.st(0)["tmin"], med([w["sig"] for w in rv.wins(0) if (w["nk"] or 0) >= NK_MIN])))
        lo_ci = boot_D(sorted(lo_items), floors) if len(lo_items) >= D_MIN_REPS else None
        robust = lab == "SHORT" and lo_ci is not None and lo_ci[0] > 0
        self.p(f"  C2-D pooled: D = median over {len(items)} replicates = {Dm:+.3f} ms, 90 % bootstrap interval [{lo:+.3f}, {hi:+.3f}] (2000 draws, seed 60); predicted +0.9 (flat) to +2.2 (secant)")
        self.p(f"  C2-D bracket: lower arm median {f(med(lower), 3)} (n={len(lower)}), upper arm median {f(med(upper), 3)} (n={len(upper)}); lower-arm interval "
               f"{'[%+.3f, %+.3f]' % lo_ci if lo_ci else 'n/a'}; SHORT {'ROBUST (both read SHORT)' if robust else 'not shown robust'}")
        for W in plan60.KNEE_W[cell]:
            v = []
            for (rep, seed), wm in self.rep_map(cell).items():
                rv = wm.get(W)
                if rv and rv.st(0)["tmin"] is not None:
                    s = med([w["sig"] for w in rv.wins(0) if (w["nk"] or 0) >= NK_MIN])
                    if s is not None:
                        v.append((rv.st(0)["tmin"] - F) - (s + Q_BULK * l_c))
            self.p(f"  C2-D arm W{W}: D median {f(med(v), 3)} (n={len(v)})")
        if inad:
            self.p("  C2-D: COVERED would be read but is INADMISSIBLE: the floor gate did not read CONVERGED (a floor read too high biases D toward COVERED); the label is UNRESOLVED")
        if lab == "SHORT" and conv != "CONVERGED":
            self.p("  C2-D: SHORT stands as a LOWER BOUND (the floor gate did not read CONVERGED)")
        if wit and not all(wit):
            self.p(f"  C2-D: sign witness FAILED on {wit.count(False)}/{len(wit)} replicates: SCORER BUG, the label below is not to be trusted")
        self.label("C2-SHORT", lab, f"D={Dm:+.3f} ms [{lo:+.3f},{hi:+.3f}]; floor {conv}; {'ROBUST' if robust else 'bracket not both SHORT'}")

    # ---- section: per-K flip and brakes (REPORTED) ----
    def per_k(self):
        self.p("PER-K READOUT (s60 section 3.1, REPORTED; K = 8 is the scored K): b0p(K) = max of the first K b0k; the valid set is recomputed for each K; the K = 1 brake is 0 by construction")
        for cell in self.cells:
            for leg in LEGS[cell]:
                rows = [rv for rv in self.by[cell] if rv.kind == "G" and rv.live and leg in rv.legs() and (rv.boot(leg).get("b0k") or [None])[0] is not None]
                nz = [rv for rv in rows if abs(rv.boot(leg)["b0k"][0]) > 1e-9]
                tag = SHORT[cell] + (f"-leg{leg}" if cell == C8 else "")
                self.p(f"  BRAKE1 {tag}: b0k[0] != 0 on {len(nz)}/{len(rows)} rows (witness: expected 0)")
                for K in range(1, K_BOOT + 1):
                    c = calibrate(self.by[cell], cell, leg, K=K, nogo=self.nogo)
                    lat = sum(1 for rv in c["rows"] if rv.latched(leg, K))
                    line = f"  PERK {tag} K={K}: valid {c['n']}/{len(c['rows'])} latched {lat} b0p(K)={f(c['b0'], 3)} T_c(K)={f(c['T'], 3)}"
                    if cell in (C2, C8) and leg == 0 and c["T"] is not None:
                        d = []
                        for (rep, seed), wm in self.rep_map(cell).items():
                            w95 = self.knee(cell, wm)
                            arms = [W for W in plan60.KNEE_W[cell] + (plan60.P1_W[cell],) if W in wm and wm[W].st(0)["m"] is not None]
                            wf, _ = cross_up([(W, wm[W].st(0)["m"]) for W in arms], c["T"])
                            if w95 is not None and wf is not None:
                                d.append(wf - w95)
                        line += f" | flip - W95 median {f(med(d), 1)} slots over {len(d)} replicates"
                    self.p(line)

    # ---- section: c8 B2 ----
    def c8_b2(self):
        cell = C8
        if cell not in self.cells:
            return
        if "c8" in self.nogo:
            self.p("  C8-B2: NOT SCORED (LD_NOGO=c8, the registered consequence: " + NOGO_TEXT[C8] + ")")
            self.label("C8-B2", "NOT-SCORED-NOGO")
            return
        cm = self.calmap(cell)
        if not cm:
            self.p("  C8-B2: UNSCOREABLE (a leg has no floor calibration: " + "; ".join(f"leg{l}: W={c['W']} n={c['n']}" for l, c in self.cal[cell].items()) + ")")
            self.label("C8-B2", "UNSCOREABLE", "no leg-1 (or leg-0) floor calibration")
            return
        res = {}
        c0 = dict(self.counters)
        for mode in ("r", "own"):
            reps = []
            for (rep, seed), wm in self.rep_map(cell).items():
                w95 = self.knee(cell, wm)
                if w95 is None:
                    continue
                v = self.at_knee(cell, wm, w95, lambda rv, m=mode: (self.rt(rv, m) or {}).get("med"))
                if not v:
                    continue
                binds = []
                for rv in v[1]:
                    x = self.rt(rv, mode)
                    binds.append(x["bind"][0] / x["bind"][1] if x and x["bind"][1] else None)
                reps.append({"rep": rep, "seed": seed, "w95": w95, "R": v[0], "rows": v[1], "bind": binds})
            # sigma_R: the pooled within-arm sd over the COUNTED (anc-bound) replicates at the bracketing arms (DECISION 12: the unbound ones are reported, not read)
            el = [r for r in reps if all(b is not None and b >= B2_ANC_MIN for b in r["bind"])]
            arms_used = sorted({rv.W for r in el for rv in r["rows"]})
            var = []
            for W in arms_used:
                xs = [(self.rt(rv, mode) or {}).get("med") for r in el for rv in {id(x): x for x in r["rows"]}.values() if rv.W == W]
                xs = [x for x in xs if x is not None]
                if len(xs) >= 2:
                    var.append(sd(xs) ** 2)
            sigR = math.sqrt(sum(var) / len(var)) if var else None
            res[mode] = (reps, sigR)
        for mode, nm in (("r", "scored (l = T_c - b0_c)"), ("own", "own-l (reported beside, no veto)")):
            reps, sigR = res[mode]
            fire = elig = 0
            for r in reps:
                ok_bind = all(b is not None and b >= B2_ANC_MIN for b in r["bind"])
                f_ = sigR is not None and r["R"] < 1 - 2 * sigR
                elig += ok_bind
                fire += bool(f_ and ok_bind)
                self.p(f"  C8-B2 [{nm}] s{r['seed']}r{r['rep']}: W95={r['w95']:.1f} R~(W95)={r['R']:.4f} sigma_R={f(sigR, 4)} fires={bool(f_)} anc-bind={' '.join(f(b, 2) for b in r['bind'])} "
                       f"{'COUNTED' if ok_bind else 'reported, not counted (anc bind < %.1f)' % B2_ANC_MIN}")
            res[mode] = (reps, sigR, fire, elig)
        reps, sigR, fire, elig = res["r"]
        _, _, ofire, _ = res["own"]
        self.p(f"  C8-B2 clamp gauge: W95 clamped at the first goodput arm {self.counters['w95_clamped'] - c0['w95_clamped']}/{self.counters['w95_n'] - c0['w95_n']} times, "
               f"interpolation clamped at an end arm {self.counters['interp_clamped'] - c0['interp_clamped']}/{self.counters['interp_n'] - c0['interp_n']} (both modes)")
        self.p(f"  C8-B2: scored recipe fires on {fire} of {elig} anc-bound replicates ({len(reps)} scoreable); own-l recipe fires on {ofire}; "
               f"anc-bind gauge: {len(reps) - elig} replicates excluded by the 0.9 rule")
        if fire >= B2_FIRES_MIN and ofire < B2_FIRES_MIN:
            self.p("  C8-B2: the scored recipe fires and the own-l recipe does not: the form fails on honest floor inputs, and the flow's own l masks it (named outcome, s60 section 1.1)")
        conv = [self.gate.get((cell, l), {}).get("conv") for l in LEGS[cell]]
        if any(c != "CONVERGED" for c in conv):
            self.p(f"  C8-B2: floor convergence {conv}: a floor too high raises R~ at the knee, so a FIRE stands and a NOT-FIRES is weakened")
        # s60 section 3.2: "fires on >= 2 of the 5-6 scoreable replicates", scoreable = the anc-bound ELIGIBLE ones (bind fraction >= 0.9 at both bracketing arms). Below 5 eligible the readout is
        # UNDERPOWERED (labelled UNSCOREABLE), fires or not: a FIRE needs >= 2 fires among >= 5 eligible, a NOT-FIRES needs >= 5 eligible.
        if elig < B2_MIN_SCOREABLE:
            self.label("C8-B2", "UNSCOREABLE", f"UNDERPOWERED: {elig} anc-bound eligible replicates ({fire} fires), need {B2_MIN_SCOREABLE}")
        elif fire >= B2_FIRES_MIN:
            self.label("C8-B2", "FIRES", f"{fire}/{elig}")
        else:
            self.label("C8-B2", "NOT-FIRES", f"{fire}/{elig}" + ("; weakened (floor not CONVERGED)" if any(c != "CONVERGED" for c in conv) else ""))

    # ---- section: c1s control sign ----
    def c1s_control(self):
        cell = C1S
        if cell not in self.cells:
            return
        if "c1s" in self.nogo:
            self.p("  C1S-CONTROL: DROPPED (LD_NOGO=c1s, the registered consequence: " + NOGO_TEXT[C1S] + ")")
            self.label("C1S-CONTROL", "DROPPED-NOGO")
            return
        rows = [rv for rv in self.by[cell] if rv.kind == "G" and rv.W == plan60.CONTROL_W[cell] and rv.live]
        vals, excl = [], []
        for rv in sorted(rows, key=lambda x: (x.rep, x.order)):
            x = self.rt(rv, "r")
            if x is None:
                excl.append(f"s{rv.seed}r{rv.rep}:no-R~")
            elif rv.collapsed(0):
                excl.append(f"s{rv.seed}r{rv.rep}:COLLAPSED")
            else:
                vals.append((rv, x["med"] - 1))
        self.p(f"  C1S-CONTROL: R~ - 1 at W{plan60.CONTROL_W[cell]} over {len(vals)} scoreable replicates of {len(rows)} (excluded: {' '.join(excl) or 'none'}): "
               f"{' '.join(f's{rv.seed}r{rv.rep}={v:+.4f}' for rv, v in vals)}")
        nz = [v for _, v in vals if v != 0]
        n, pos = len(nz), sum(1 for v in nz if v > 0)
        k = sign_k(n) if n else None
        if len(vals) < MIN_SIGN_N or k is None:
            lab = "UNDERPOWERED"
        elif pos >= k:
            lab = "PINNED"
        elif n - pos >= k:
            lab = "CROSSES"
        else:
            lab = "SPLIT"
        self.p(f"  C1S-CONTROL: n_s={len(vals)} (non-zero {n}), positive {pos}, negative {n - pos}, sign-test k={k} (alpha {SIGN_ALPHA})")
        self.label("C1S-CONTROL", lab, f"n_s={len(vals)} pos={pos}")

    # ---- section: crossing W_r (reported against Appendix A's bands) ----
    def crossings(self):
        self.p("CROSSINGS (REPORTED): per replicate the W where R~ (and R~67) falls through 1 over the non-floor arms; Appendix A's bands are cap-128-based context, beside R6.7's honest-floor crossings")
        for cell in self.cells:
            if not self.calmap(cell):
                self.p(f"  CROSS {SHORT[cell]}: UNSCOREABLE (no calibration)")
                continue
            for (rep, seed), wm in self.rep_map(cell).items():
                pts = {m: [(W, (self.rt(rv, m) or {}).get("med")) for W, rv in wm.items() if not plan60.is_floor(cell, rv.arm)] for m in ("r", "r67")}
                out = {m: cross_down([(W, v) for W, v in pts[m] if v is not None]) for m in pts}
                self.p(f"  CROSS {SHORT[cell]} s{seed}r{rep}: R~ {f(out['r'], 1)} [{band_word(out['r'], BANDS_A3[cell])} A.3 {BANDS_A3[cell]}] R~67 {f(out['r67'], 1)} "
                       f"[{band_word(out['r67'], BANDS[cell])} R6.7 {BANDS[cell]}]")

    # ---- section: G/C pairs ----
    def gc_pairs(self):
        if C1S not in self.cells:
            return
        g = {(rv.seed, rv.rep): rv for rv in self.by[C1S] if rv.arm == "G2048" and rv.live}
        c_ = {(rv.seed, rv.rep): rv for rv in self.by[C1S] if rv.arm == "C2048" and rv.live}
        pairs = [(g[k], c_[k]) for k in sorted(set(g) & set(c_), key=lambda k: (k[1], k[0]))]
        unfl = [(a, b) for a, b in pairs if not a.collapsed(0) and not b.collapsed(0) and not a.dnf and not b.dnf]
        cpu, gpv = [], []
        for a, b in unfl:
            ca, cb = _cpu_gb(a), _cpu_gb(b)
            if ca and cb:
                cpu.append(math.log(ca / cb))
            ga, gb = a.r.get("mbps"), b.r.get("mbps")
            if ga and gb:
                gpv.append(math.log(ga / gb))
        self.p(f"G/C PAIRS (s60 section 4): {len(pairs)} paired, {len(unfl)} with both rows unflagged (absolute collapse rule, DECISION 14; quinn qcwnd share beside: "
               f"{' '.join(f(((a.r.get('qcwnd_below') or {}).get('per_leg') or [None])[0], 2) for a, _ in pairs)}); predicted G within +1 % of C")
        if len(unfl) < GC_MIN_PAIRS or not cpu:
            self.label("GC-PAIRS", "UNRESOLVED", f"{len(unfl)} unflagged pairs < {GC_MIN_PAIRS}" if len(unfl) < GC_MIN_PAIRS else "no CPU readings")
        else:
            m = med(cpu)
            self.p(f"  GC: median log(G/C) of client CPU s/GB = {m:+.4f} (= {100 * (math.exp(m) - 1):+.1f} %), PERTURBING iff > {GC_PERTURB:.4f} (+10 %); n={len(cpu)}")
            self.label("GC-PAIRS", "PERTURBING" if m > GC_PERTURB else "NOT-PERTURBING", f"{100 * (math.exp(m) - 1):+.1f} % CPU s/GB over {len(cpu)} pairs")
        if len(gpv) >= 2:
            sdv = sd(gpv)
            n = len(gpv)
            mdd = (T975[min(n, 41) - 2] + T80[min(n, 41) - 2]) * sdv / math.sqrt(n) if n >= 2 else None
            self.p(f"  GC: goodput median log(G/C) = {med(gpv):+.4f}; minimum detectable difference at n={n} pairs (s60size.py: (t.975 + t.80) sd/sqrt(n), sd {sdv:.4f}) = {100 * mdd:.1f} %")
        else:
            self.p("  GC: goodput: fewer than 2 unflagged pairs; no minimum detectable difference")
        self.p("  GC: 'C is re-based against s56's c1s control' (s60 section 4) needs s56's committed rows and is NOT computed here (DECISION 17)")

    # ---- section: the evicting rows ----
    def evict(self):
        rows = [rv for rv in self.rvs if rv.kind == "E"]
        if not rows:
            self.p("EVICTING ROWS: none in this session (status §60 Amendment 2: moved to their own registration)")
            return
        self.p(f"EVICTING ROWS (s60 section 6; plan 59 on the gauge): {len(rows)} rows. m-bar = d(cum ack)/sum(adv count) over LATE (t >= {EVICT_LATE_T:g} s)")
        for rv in sorted(rows, key=lambda x: x.rep):
            e = (rv.t.get("evict") or {})
            if not rv.live or not e:
                self.p(f"  EVICT s{rv.seed}r{rv.rep}: no readings ({rv.r.get('status')})")
                continue
            self.p("  " + evict_line(rv))
        self.p("  EVICT: K4's oracle comparison (the oracle's W1 at eps, sigma2, r, astar, Delta, m-bar against each window's dbt_share) is done OFF-VM and is NOT computed here; the inputs above are what it reads")


def boot_D(items, floors):
    """s60 section 3.1: the 90 % bootstrap interval of the pooled D = median_r [(Tmin_r - F) - (sig_r + q*l)]. Resamples the replicates `items` [(Tmin_r, sig_r)] and the floor-pool rows `floors`
    [(F_row, l_row)] with replacement (2000 draws, seed 60); both inputs are sorted by the caller so the draw is order-independent. -> (lo, hi)."""
    rnd = random.Random(BOOT_SEED)
    n, m = len(items), len(floors)
    res = []
    for _ in range(BOOT_N):
        it = [items[rnd.randrange(n)] for _ in range(n)]
        fl = [floors[rnd.randrange(m)] for _ in range(m)]
        F, l = med([x[0] for x in fl]), med([x[1] for x in fl])
        res.append(med([(t - F) - (s + Q_BULK * l) for t, s in it]))
    res.sort()
    return res[int(BOOT_LO * BOOT_N)], res[min(BOOT_N - 1, int(BOOT_HI * BOOT_N))]


def _cpu_gb(rv):
    """Client CPU s/GB: the driver's CPUCLI seconds (row field cpu_cli) per GB of the row's bytes."""
    cpu = rv.r.get("cpu_cli")
    b = rv.r.get("bytes") or plan60.arm_bytes(rv.cell, rv.arm)
    return (cpu / (b / 1e9)) if (cpu and b) else None


def evict_stats(rv):
    """s60 section 6 from the stored extras: m-bar and its witness, the row W1 and its witness, the steady windows and the K4 input means."""
    e = rv.t.get("evict") or {}
    cum, w, sp, m59 = e.get("cum") or {}, e.get("wstore") or {}, e.get("span") or {}, e.get("mbar59") or {}
    out = {"mbar": None, "mbar_lo": m59.get("mbar_lo"), "mbar_ok": None, "w1": None, "w1_lo": m59.get("w1_lo"), "w1_hi": m59.get("w1_hi"), "w1_ok": None,
           "n_late": 0, "n_steady": 0, "astar_bad": 0, "means": {}, "unscoreable": True}
    late = [(t, a) for t, s, a in zip(cum.get("t", []), cum.get("src", []), cum.get("ack", [])) if t is not None and t >= EVICT_LATE_T and (s or 0) > 0 and a is not None]
    wt = w.get("t") or []
    if len(late) >= 2:
        t1, t2 = late[0][0], late[-1][0]
        sadv = sum((a or 0) for t, a in zip(wt, w.get("adv") or []) if t is not None and t1 < t <= t2)
        if sadv > 0:
            out["mbar"] = (late[-1][1] - late[0][1]) / sadv
            out["mbar_ok"] = None if out["mbar_lo"] is None else out["mbar_lo"] <= out["mbar"] + 1e-9
    acc = sum((a or 0.0) for t, a in zip(wt, w.get("dbt_acc") or []) if t is not None and t >= EVICT_LATE_T)
    em = sum((a or 0.0) for t, a in zip(wt, w.get("dbt_em") or []) if t is not None and t >= EVICT_LATE_T)
    if acc > 0:
        out["w1"] = em / acc
        if out["w1_lo"] is not None and out["w1_hi"] is not None:
            out["w1_ok"] = out["w1_lo"] - 1e-9 <= out["w1"] <= out["w1_hi"] + 1e-9
    st, sa, sd_ = sp.get("t") or [], sp.get("a_star") or [], sp.get("delta") or []
    rows = []
    for i, t in enumerate(wt):
        if t is None or t < EVICT_LATE_T:
            continue
        out["n_late"] += 1
        if not ((w["fecn"][i] or 0) >= 1) or i == 0 or i + 1 >= len(wt):
            continue
        dt = ((w["tw"][i] or 250.0) / 1000.0)
        lo, hi = wt[i - 1] - dt, wt[i + 1]
        js = [j for j, s in enumerate(st) if lo < s <= hi]
        if not js or any(sa[j] is None or sd_[j] is None for j in js) or len({sa[j] for j in js}) != 1 or len({sd_[j] for j in js}) != 1:
            continue
        a_star, delta = sa[js[0]], sd_[js[0]]
        if w["astar"][i] is not None and abs(w["astar"][i] - a_star) > 0.01:
            out["astar_bad"] += 1
        rows.append((w["eps"][i], w["sigma2"][i], w["r"][i], w["astar"][i], delta, w["dbt_share"][i]))
    out["n_steady"] = len(rows)
    out["unscoreable"] = len(rows) < STEADY_MIN
    for j, k in enumerate(("eps", "sigma2", "r", "astar", "delta", "dbt_share")):
        out["means"][k] = mean([r[j] for r in rows])
    return out


def evict_line(rv):
    s = evict_stats(rv)
    mm = s["means"]
    return (f"EVICT s{rv.seed}r{rv.rep}: m-bar={f(s['mbar'], 3)} (witness mbar_lo {f(s['mbar_lo'], 3)} <= m-bar: {'ok' if s['mbar_ok'] else ('FAIL' if s['mbar_ok'] is False else 'n/a')}); "
            f"row W1={f(s['w1'], 4)} in [{f(s['w1_lo'], 4)}, {f(s['w1_hi'], 4)}]: {'ok' if s['w1_ok'] else ('FAIL' if s['w1_ok'] is False else 'n/a')}; "
            f"steady LATE windows {s['n_steady']}/{s['n_late']} -> {'K4-UNSCOREABLE (< %d)' % STEADY_MIN if s['unscoreable'] else 'K4 inputs'} "
            f"[eps={f(mm.get('eps'), 5)} sigma2={f(mm.get('sigma2'), 4)} r={f(mm.get('r'), 4)} astar={f(mm.get('astar'), 2)} Delta={f(mm.get('delta'), 4)} dbt_share={f(mm.get('dbt_share'), 4)} "
            f"m-bar={f(s['mbar'], 3)}] (astar-token witness mismatches {s['astar_bad']})")


def _run(self):
    self.rows_table()
    self.calibration()
    self.floor_gate()
    self.arms()
    self.r_sign()
    self.p123()
    self.c2_short()
    self.per_k()
    self.c8_b2()
    self.c1s_control()
    self.crossings()
    self.gc_pairs()
    self.evict()


Session.run = _run


def plan_arms(plan):
    s = set()
    if plan in ("60a", "all"):
        for c, ws in plan60.GRID["60a"].items():
            s |= {(c, "G%d" % w) for w in ws}
    if plan in ("60b", "all"):
        s |= {(C1S, "G%d" % w) for w in plan60.GRID["60b"][C1S]} | {(C1S, "G2048"), (C1S, "C2048"), (plan60.EVICT_CELL, plan60.EVICT_ARM)}
    return s


def score(paths, plan="all", out=print):
    """s60 score: every registered readout, then VERDICT60 and one LABEL60 line per scored readout. rc 0 (the readouts are reported; a STOP or SCORER-BUG is a printed line, not an rc)."""
    import place38_parse as pp
    import pool56_score as ps
    if plan not in ("60a", "60b", "all"):
        out(f"unknown plan {plan}")
        return 2
    nogo = [x for x in os.environ.get("LD_NOGO", "").split(",") if x]
    rows_all, tokens = pp.rows_of(paths)
    want = plan_arms(plan)
    rows_all = [r for r in rows_all if (r.get("cell"), r.get("arm")) in want]
    cnt, replaced = ps.counting(rows_all)
    idx = {id(r): i for i, r in enumerate(rows_all)}
    cnt.sort(key=lambda r: idx[id(r)])
    names = ("60a", "60b") if plan == "all" else (plan,)
    n_exp = sum(len(plan60.plan(n)) for n in names)
    cells = tuple(c for c in plan60.CELLS if any((rc, ra) in want for rc, ra in want if rc == c and ra != plan60.EVICT_ARM))
    out(f"S60 SCORE plan={plan} (store law gauge session; R~ replayed from printed [SLAW] fields, s60 section 1.1); LD_NOGO={','.join(nogo) or 'none'}; "
        f"ABORT-TOKENS {' '.join(sorted({t for t in tokens if t in pp.ABORT_TOKENS})) or 'none'}")
    sess = Session(cnt, replaced, cells, out, nogo, None)
    slots = len(ps.slots(rows_all))
    sess.run()
    nvoid = len(sess.void)
    wvoid = sum(1 for rv in sess.rvs if rv.live and rv.kind in ("G", "E") and rv.t.get("witness", {}).get("ok") is not True)
    ndnf = sum(1 for rv in sess.rvs if rv.dnf)
    terr = sum(1 for rv in sess.rvs if rv.r.get("t60_error") or not rv.t)
    out(f"VERDICT60 REPORTED-WITH-SCORED slots={slots}/{n_exp} live={len(sess.rvs) - nvoid} void={nvoid} replaced={len(replaced)} dnf={ndnf} witness_void={wvoid} t60_missing={terr} "
        f"labels={len(sess.labels)} nogo={','.join(nogo) or 'none'}")
    for name, lab in sess.labels:
        out(f"LABEL60 {name} {lab}")
    return 0


# ───────────────────────────── the smoke / rehearsal ─────────────────────────────
def go_nogo_rows(rv):
    """s60 section 5, the go/no-go criteria of one floor-pool rehearsal row (leg 0): (a) no DNF, (b) not COLLAPSED, (c) >= 80 % of LATE windows with nk >= 10, (d) 8 brakes with b0kn >= 10, (e) not LATCHED."""
    s = rv.st(0) if 0 in rv.legs() else None
    share = (s["n10"] / s["n"]) if (s and s["n"]) else None
    crit = {"a": rv.live and not rv.dnf, "b": rv.collapsed(0) is False, "c": share is not None and share >= SHARE_NK, "d": rv.complete(0), "e": rv.latched(0) is False}
    return crit, share


def smoke(path, plan, out=print):
    """s60 sections 5 and 7: PASS / FAIL lines ending SMOKE-PASS | SMOKE-FAIL. 60a = c2 W192 C + G, 60b = c1s W64 G + W2048 C, 60r = the 10 rehearsal rows (adds the witnesses and GO-NOGO)."""
    import place38_parse as pp
    if plan not in ("60a", "60b", "60r", "60br"):
        out(f"unknown plan {plan}")
        return 2
    slots = plan60.plan("60a", rehearsal=True) if plan == "60r" else plan60.plan("60br") if plan == "60br" else plan60.plan(plan, smoke=True)
    rows, _ = pp.rows_of([path])
    got = {}
    for r in rows:
        got[(r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep")))] = r         # the last row of a slot
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)

    out(f"SMOKE60 plan={plan} rows={len(rows)} slots={len(slots)}")
    rvs = []
    miss = []
    for (c, _, a, sd_, rp) in slots:
        r = got.get((c, a, str(sd_), str(rp)))
        if r is None:
            miss.append(f"{c} {a} s{sd_} r{rp}: no row")
        else:
            rvs.append(RV(r, len(rvs)))
    crit("one row per slot ran with a t60", miss + [f"{rv.cell} {rv.arm}: no t60 ({rv.r.get('t60_error')})" for rv in rvs if not rv.t])
    rvs = [rv for rv in rvs if rv.t]
    lib = _lib()
    crit("the store-law replay modules (s60replay, caplog_replay) import", [lib[2]] if lib[2] else [])
    for rv in rvs:
        T = rv.t
        out(f"  ROW {SHORT[rv.cell]} {rv.arm} s{rv.seed} r{rv.rep}: status={rv.r.get('status')} dnf={int(rv.dnf)} tags(cli)={T['tags']['cli']} witness={T['witness']} caplog_ok={T['caplog'].get('ok')} q={T['q']}")
    cs = [rv for rv in rvs if rv.kind == "C"]
    gs = [rv for rv in rvs if rv.kind == "G"]
    es = [rv for rv in rvs if rv.kind == "E"]                 # the evicting row of 60b's rehearsal (plan 59's arm, gauge on, Realtime: q = 0.05)
    gall = gs + es
    gcap = gs                                                # E rows have no store cap by construction (plan_dyn_cap needs reliable): no CAPLOG on them
    crit("a C row (RWM_SLAW=0) carries NO gauge tag on either end", [f"{rv.cell} {rv.arm}: {k}={v}" for rv in cs for side in ("all_cli", "srv") for k, v in rv.t["tags"][side].items() if v])
    crit("a G row carries ALL gauge tags (WSTAT, SLAW, WSTORE, SLAWC, CAPLOG; an E row all but CAPLOG: it has no store cap) after the warm-up cut", [f"{rv.cell} {rv.arm}: no {k}" for rv in gall for k, v in rv.t["tags"]["cli"].items() if not v and not (rv.kind == "E" and k == "CAPLOG")])
    crit("RWM_SLAW echoed as requested on both ends", [f"{rv.cell} {rv.arm}: cli={rv.t['gate_slaw']['cli']} srv={rv.t['gate_slaw']['srv']}" for rv in rvs
                                                       if rv.t["gate_slaw"]["cli"] != ("0" if rv.kind == "C" else "1") or rv.t["gate_slaw"]["srv"] != ("0" if rv.kind == "C" else "1")])
    crit("[SLAW] q = 0.100000 on every Bulk G row (0.050000 would be Realtime)", [f"{rv.cell} {rv.arm}: q={rv.t['q']}" for rv in gs if rv.t["q"] != ["0.100000"]])
    if es:
        crit("[SLAW] q = 0.050000 on the Realtime evicting row", [f"{rv.cell} {rv.arm}: q={rv.t['q']}" for rv in es if rv.t["q"] != ["0.050000"]])
    else:
        out("NOT-RUN [SLAW] q = 0.05 on the evicting row (no evicting row; status §60 Amendment 2)")
    boot_missing = []
    for rv in gall:
        if rv.dnf:
            continue                                  # DECISION 11: a DNF row is a datum; it has no boot to read
        b = rv.boot(0)
        if not b.get("b0k") or b.get("kw") is None or len(b["b0k"]) < K_BOOT:
            boot_missing.append(f"{rv.cell} {rv.arm}: kw={b.get('kw')} b0k={b.get('b0k')}")
    crit("kw and b0k (K = 8) present after the warm-up cut", boot_missing)
    crit("the per-row replay witness PASS (R~ with the row's own b0p and l reproduces printed R and Rb to 6e-6)",
         [f"{rv.cell} {rv.arm}: {rv.t['witness']}" for rv in gall if not rv.dnf and rv.t["witness"].get("ok") is not True])
    crit("the CAPLOG replay PASS (caplog_replay.py over the row's [CAPLOG] records: records > 0 and every replayable cap reproduced; any mismatch is a FAIL)",
         [f"{rv.cell} {rv.arm}: {rv.t['caplog']}" for rv in gcap if rv.t["caplog"].get("ok") is not True])
    for rv in gcap:
        cl = rv.t["caplog"]
        share = (cl["replayed"] / cl["records"]) if cl["records"] else None
        out(f"  CAPLOG {SHORT[rv.cell]} {rv.arm}: records={cl['records']} replayable share={f(share, 3)} ({cl['replayed']} replayed, {cl['mismatches']} mismatches) rate={f(cl['rate_hz'], 1)}/s pooled={cl['n_pooled']} bind(ceiling)={f(cl['ub'], 3)} bind(floor)={f(cl['fb'], 3)} "
            f"seats={cl['seats']} unreplayable={cl['unreplayable']} q_off={cl['q_off']}")
    if plan == "60r":
        _rehearsal(rvs, out)
    if plan == "60br":
        if es:
            _evict_rehearsal(es, crit, out)
        else:
            out("EVICTING ROWS: none in this session (status §60 Amendment 2: moved to their own registration)")
            out("NOT-RUN the evicting-row rehearsal checks (status §60 Amendment 2): liveness, rr > 0, the ackw clock join")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


ACKW_TOL_T = 0.08          # [DIAG] prints t at 0.1 s, [WSTORE] at 0.01 s: a window and its tick are the same iff |dt| <= this


def ackw_join(ev):
    """The clock join of 60b's rehearsal (a known answer). What each counts (diag.rs / slaw.rs): `[DIAG] wake[.. ack=N]` is dg.wake_n[8], the sender loop's ack-arm wakes CUMULATIVE since the loop
    started (never reset; diag.rs `wake_line`); `[WSTORE] ackw` is `ackw_total - s.ackw_last` with ackw_total = the SAME dg.wake_n[8], read in the same diag report as the tick's [DIAG] line (diag.rs: the slaw
    window close follows the [DIAG] line synchronously), so it is the ack wakes per window: the per-window DELTA of the cumulative counter (the first window's delta is the cumulative itself).
    Expected relation, exact: for consecutive matched ticks j-1, j: ackw[window j] == wack[tick j] - wack[tick j-1]. -> (pairs compared, pairs equal, first mismatch or None)."""
    w, c = ev.get("wstore") or {}, ev.get("cum") or {}
    tw_, aw = w.get("t") or [], w.get("ackw") or []
    tt, wk = c.get("t") or [], c.get("wack") or []
    if not (tw_ and aw and tt and wk):
        return 0, 0, "no ackw column or no wake[ack=] column"

    def tick_of(t):
        best = None
        for i, x in enumerate(tt):
            if x is not None and t is not None and abs(x - t) <= ACKW_TOL_T:
                best = i
                break
        return best
    n = eq = 0
    first = None
    prev = None
    for t, a in zip(tw_, aw):
        i = tick_of(t)
        if i is None or a is None or wk[i] is None:
            prev = None
            continue
        if prev is not None and i == prev + 1 and wk[prev] is not None:
            n += 1
            if a == wk[i] - wk[prev]:
                eq += 1
            elif first is None:
                first = f"t={t}: ackw={a} but wake ack delta={wk[i] - wk[prev]}"
        prev = i
    return n, eq, first


def _evict_rehearsal(es, crit, out):
    """s60 sections 5 / 6 on 60b's rehearsal: the evicting row (plan 59's arm) is armed (plan 59's liveness witnesses are row problems: any evict-* / gate-unified problem FAILS), the debt loop runs
    (the median LATE [SPAN] rr > 0: the go/no-go of mbar59_score.smoke), and the [WSTORE] ackw / [DIAG] wake ack clock join holds exactly (ackw_join)."""
    import place38_parse as pp
    crit("the evicting row is armed and live (plan 59's liveness: sender shed echo, shed= on [SPAN], RWM_UNIFIED / RWM_UNIFIED_SHED on both ends, no retain-until-acked echo, the Realtime echo)",
         [f"{rv.cell} {rv.arm}: {' '.join(x for x in rv.r.get('problems') or [] if pp.is_evict_problem59(x))}" for rv in es if pp.is_evict_void59(rv.r)] + ([] if es else ["no evicting row"]))
    for rv in es:
        ev = rv.t.get("evict") or {}
        rr = (ev.get("mbar59") or {}).get("rr")
        out(f"  GO-NOGO-EVICT {rv.arm}: [SPAN] rr median {f(rr, 3)} (> 0 required)  a_star {f((ev.get('mbar59') or {}).get('a_star'), 1)}  mbar_lo {f((ev.get('mbar59') or {}).get('mbar_lo'), 3)}")
    crit("the debt loop runs: the median [SPAN] rr over LATE is > 0 (a seat with rr <= 0 never reaches the cancellation)",
         [f"{rv.cell} {rv.arm}: rr median {((rv.t.get('evict') or {}).get('mbar59') or {}).get('rr')}" for rv in es if not ((((rv.t.get('evict') or {}).get('mbar59') or {}).get('rr') or 0) > 0)])
    fails = []
    for rv in es:
        n, eq, first = ackw_join(rv.t.get("evict") or {})
        out(f"  ACKW-JOIN {rv.arm}: [WSTORE] ackw (ack-arm wakes per window) vs the delta of [DIAG] wake[ack=] (cumulative) over the same ticks: {eq}/{n} windows equal"
            + (f"; first mismatch {first}" if first else ""))
        if n < 10 or eq != n:
            fails.append(f"{rv.cell} {rv.arm}: {eq}/{n} equal ({first})")
    crit("the [WSTORE] ackw / [DIAG] wake[ack=] clock join: per window ackw == the delta of the cumulative wake ack counter, on >= 10 windows, all equal", fails)


def _rehearsal(rvs, out):
    """s60 section 5: the REPORTED witnesses and the per-cell GO/NO-GO decisions. A NO-GO is a decision line, never a SMOKE-FAIL."""
    gs = [rv for rv in rvs if rv.kind == "G"]
    for rv in gs:
        for leg in rv.legs():
            b = rv.boot(leg).get("b0k")
            ml = rv.t.get("ml", {}).get(str(leg))
            out(f"  WITNESS {SHORT[rv.cell]} {rv.arm} s{rv.seed} r{rv.rep} leg{leg} (REPORTED): b0k[0]={f(b[0], 3) if b else '-'} (expected 0: {'ok' if b and b[0] == 0 else 'NOT 0' if b else 'n/a'}); "
                f"m = l for t < {ML_T:g} s on {ml['eq'] if ml else '-'}/{ml['n'] if ml else '-'} windows (max |m-l| {f(ml['maxdiff'], 4) if ml else '-'})")
    for rv in gs:
        a = (rv.t.get("diag") or {}).get("attach")
        if rv.cell == C8 and a:
            out(f"  ATTACH c8 {rv.arm} s{rv.seed} r{rv.rep} leg1: {a['status']} (DIAG leg-1 echo count {a['echo_first']} -> {a['echo_last']} grows={a['echo_grows']}; [SLAW] leg-1 nk total {a['slaw1_nk_total']})"
                + (" <- the W96 row the registration reads" if rv.W == 96 else ""))
            if a["status"] == "UNATTACHED":
                out(f"  LEG1-ATTACH c8 {rv.arm}: UNATTACHED -> c8 has no leg-1 floor, B2 is not scored and the cause is diagnosed (s60 section 5)")
            elif a["status"] == "STARVED":
                out(f"  LEG1-ATTACH c8 {rv.arm}: STARVED (not a sampler fault) -> leg 1 takes its floor pool from the smallest arm where it qualifies (256 at the latest)")
    for rv in gs:
        if rv.cell in SMOKE_B0P_LIMIT and 0 in rv.legs():
            b0 = rv.b0(0)
            s = rv.st(0)
            w = [x for x in rv.wins(0) if x["tp50"] and x["nk"]]
            tbar = sum(x["nk"] * x["tp50"] for x in w) / sum(x["nk"] for x in w) if w else None
            w0 = None
            if rv.cell == C8 and 1 in rv.legs():
                n0 = sum(x["nk"] or 0 for x in rv.wins(0))
                n1 = sum(x["nk"] or 0 for x in rv.wins(1))
                w0 = n0 / (n0 + n1) if n0 + n1 else None
            sens = (2 * (w0 if w0 is not None else 1.0) / tbar) if tbar else None
            out(f"  B0P-LIMIT {SHORT[rv.cell]} {rv.arm} s{rv.seed} r{rv.rep} (REPORTED, not gating): b0p={f(b0, 3)} vs entry-8 limit {SMOKE_B0P_LIMIT[rv.cell]} ms; R~ sensitivity to b0_c = {f(sens, 4)} per ms "
                f"({'2*w0/T' if rv.cell == C8 else '2/T'}, T-bar {f(tbar, 2)} ms, w0 {f(w0, 2)})")
    for cell in plan60.CELLS:
        fl = [rv for rv in gs if rv.cell == cell and plan60.is_floor(cell, rv.arm)]
        if not fl:
            continue
        ok_rows = []
        for rv in fl:
            cr, share = go_nogo_rows(rv)
            out(f"  GO-NOGO-ROW {SHORT[cell]} {rv.arm} s{rv.seed} r{rv.rep}: (a) no DNF {cr['a']} (b) not COLLAPSED {cr['b']} (c) leg-0 nk>=10 share {f(share, 2)} {cr['c']} (d) 8 brakes b0kn>=10 {cr['d']} (e) not LATCHED {cr['e']}")
            if all(cr.values()):
                ok_rows.append(rv)
        if ok_rows:
            out(f"GO-NOGO {SHORT[cell]} GO ({len(ok_rows)} of {len(fl)} floor-pool rehearsal rows pass (a)-(e))")
        else:
            out(f"GO-NOGO {SHORT[cell]} NO-GO (0 of {len(fl)} floor-pool rehearsal rows pass (a)-(e)): consequence: {NOGO_TEXT[cell]}; set LD_NOGO={SHORT[cell]}")


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "smoke60" and len(a) == 2:
        return smoke(a[1], a[0])
    if cmd == "score60" and len(a) >= 2:
        return score(a[1:], a[0])
    print(f"usage: {sys.argv[0]} smoke60 <60a|60b|60r|60br> <ledger> | score60 <60a|60b|all> <ledger> [...]")
    return 2


DECISIONS = [
    "1. R~ is replayed from the printed [SLAW] fields of the LATE windows through s60replay.slaw_r / r67_r (imported, not copied). A window needs every live leg (nk > 0) to carry tp50, tmin, m, sig; the row R~ is the median over such windows.",
    "2. LATE = t >= plan60.LATE_T (15 s at c2 / c8, 5 s at c1s); 'windows with nk >= 10' (F, l, sig, brake medians) are the LATE windows with nk >= 10; R~ itself uses every LATE window (w = nk weights the legs).",
    "3. l_c,i is the median over valid rows of the row's median LATE l; T_c,i the median of (row median LATE l + row b0p(K)); F_c,i the median of the row median LATE tmin. b0 is the leg-0 max of the first K b0k; leg 1 uses the entries with b0kn >= 10 (>= 4 of them).",
    "4. LATCHED uses the ROW's own median LATE l in q * l (not the session l_c: that would be circular, the latched rows leave the session medians) and the median LATE brake tmin - m over the nk >= 10 windows, at the leg's omega-bar.",
    "5. The 80 % floor-pool rule is POOLED over the pool's live rows (sum of nk >= 10 windows / sum of LATE windows of the leg >= 0.80). NO-GO c2 forces the c2 pool to W 32.",
    "6. A row failing the per-row replay witness is VOID for every R reading (R~, R~67, B2, the c1s sign, the sign witness, crossings); its tmin / sig / l readings (floor, D, P3) are kept.",
    "7. The floor gate fits the rows of all three floor pools that are live, have nk >= 10 windows and are not COLLAPSED (a tmin-only reading, so boot completeness and LATCHED do not remove a row); E = b * W1 with W1 = the leg's floor pool. c1s has no registered tau: FLOOR-CONV-c1s reads NO-TAU-REGISTERED (the slope label is still given).",
    "8. Labels: FLOOR-SLOPE-/FLOOR-CONV-<cell>, with -leg0 / -leg1 appended at c8 only; P3-c8l0 / P3-c8l1; P1 reads HOLDS / FAILS (a failure is only reported); B1 and R-SIGN are extra LABEL60 lines (B1-<cell>, R-SIGN-<cell>).",
    "9. The R-sign gate ('after replicate 2' cannot be computed: 2 floor rows < MIN_CAL_ROWS) is read at the END of 60a on the FULL calibration over every live control G row of c2 and c8 (Amendment 1); any R~ >= 1 is a STOP; PASS needs >= R_SIGN_MIN = 2 replayable control rows, else UNSCOREABLE; under LD_NOGO=c8 R-SIGN-c8 is UNSCOREABLE (reported, never a STOP: its floor failed go/no-go; a c2 NO-GO only moves the pool, so c2 still gates). 60b refuses to launch on a STOP or a missing label at either cell (ABORT-RSIGN); UNSCOREABLE does not block (60b's readings do not use c2 / c8's R). Under the drift fallback the pairing applies to the control rows too.",
    "10. The drift check's order is the ledger position of the row; the fitted change is the OLS slope times the order span of the calibration rows; a trigger on either series (b0p, l) switches the cell to per-replicate pairing; the ARMS lines then print the paired R~ and, beside it, the session-value R~ as the sensitivity (the 're-stated conditioning from the realised sigma_R' is NOT implemented).",
    "11. A floor-pool/smoke DNF row is a datum: its missing boot / witness is not a SMOKE-FAIL (the GO-NOGO criterion (a) reports it); a non-DNF row missing kw / b0k / witness / CAPLOG PASS is a mechanical failure. CAPLOG seats are reported, not asserted to be 'plain' (caplog_replay replays by branch; 'tt' stays unreplayable).",
    "12. C8-B2: scoreable = the anc-bound ELIGIBLE replicates (replay anc bind fraction m >= l_c + b0_c over every live (leg, window) term, >= 0.9 at both bracketing rows); fewer than 5 eligible is UNSCOREABLE (UNDERPOWERED) whatever the fires; with >= 5, FIRES iff >= 2 fire, else NOT-FIRES (weakened when a floor is not CONVERGED). sigma_R is the pooled within-arm sd (R~ per row) over the eligible replicates at the bracketing arms used.",
    "13. C2-SHORT needs >= 3 scoreable replicates (a replicate = a control goodput, a knee reached, two bracketing rows with tmin and sig). The bracket arms: per replicate the largest knee arm <= W95 and the smallest >= W95 (clamped to the end arms); SHORT is ROBUST iff the lower-arm bootstrap interval is also > 0. COVERED without a CONVERGED floor prints UNRESOLVED.",
    "14. 'Unflagged' pairs and the c1s control use the absolute collapse rule on leg 0 (DIAG-only, works on C rows) plus not-DNF; the quinn qcwnd_below share is printed beside. W95 uses cross_up over ALL the replicate's non-control arms and prints how often it clamped at the first arm.",
    "15. The sign test is the exact two-sided binomial at 0.05 (transcribes s60cond.py's K table 6:6 7:7 8:8 9:8 10:9 11:10 12:10); zeros are dropped; n_s < 6 is UNDERPOWERED.",
    "16. EVICT: LATE for m-bar / W1 / K4 is t >= 5 s (mbar59_score.LATE_T); the [DIAG] ticks, [WSTORE] windows and [SPAN] lines are joined on their printed t with no offset (one-window error, <1 % of the LATE span); the steady-window test uses the window (t - tw, t] with its neighbours and needs >= 1 [SPAN] line there.",
    "22. Since status §60 Amendment 2, 60b and its rehearsal 60br carry no evicting (EVICT) row: decision 16 applies only to a session that schedules EVICT rows. With none, the scorer prints `EVICTING ROWS: none` and NOT-RUN for the evicting-row checks, never a PASS over an empty set; an E row has no store cap (no CAPLOG) and is excluded from the CAPLOG tag and replay checks.",
    "17. NOT implemented: 'C is re-based against s56's c1s control' (s60 section 4) needs s56's committed rows; the K4 oracle comparison is off-VM by the spec. The own-l R~, the leg-1 sig and pull (A.5), E50 / E90 beside X, the CAPLOG rate and the P2 printed R are implemented as reported lines.",
    "19. GO/NO-GO rows: every G floor-pool row of the cell in the rehearsal ledger, the smoke rows included (the c1s smoke G64 is the third c1s W64 row; c8's W96 row counts, and leg 0 only is read). 'Starved' c8 leg 1 (nk >= 10 on < 80 % of LATE windows at 32/64/96) calibrates on arm W256 and nothing later (s60 section 5); a floor pool whose rows are ALL COLLAPSED is skipped for the next pool and the skip is printed (s60 section 2).",
    "20. The collapse share / E50 / E90 and the leg-1 attachment test are computed at ROW time (row_metrics) from the [DIAG] ticks; the per-tick data is not stored (only the verdicts and the min rtp_us). The [DIAG] / [WSTORE] / [SPAN] clock join in 16 is not verified against the engine's clocks.",
    "21. Under the drift fallback (per-replicate pairing) a replicate with no same-(rep, seed) floor row cannot be replayed: at c1s the control replicates 7-12 (and the extra G rows) have none and drop out of the sign test; the ARMS lines print the session-value sensitivity for the rows it can replay.",
    "18. Goodput of a row is the ledger `mbps` (a DNF reads 0); the CPU s/GB is cpu_cli / (bytes / 1e9); the goodput minimum detectable difference is (t.975 + t.80) * sd / sqrt(n) of the pair log-ratios (s60size.py mdd).",
]
DECISIONS.sort(key=lambda d: int(d.split(".")[0]))
__doc__ += "\nDECISIONS (every ambiguity and the reading taken):\n" + "\n".join("  " + d for d in DECISIONS) + "\n"

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
