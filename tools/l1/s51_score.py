#!/usr/bin/env python3
"""Status 51, block 51 (plan 51): the row readouts, the two-sided rank-sum statistic, T1 / T2, the in-session A/A d80, W-ROUTE, the r(delta) table, the smoke and the score.
docs/derivations/s51-design.md sections 3 (readouts), 4 (the statistic, T1, T2, A/A, VOID), 5 (predictions), 6 item 4 (this scorer); s51-evict-design.md section 3.2 (the added readouts, the not-overdriven witness).

    s51_score.py smoke51 <smoke|rehearsal> <ledger>    PASS / FAIL per criterion (the rehearsal runs the smoke rows: H-B, H-R, B+, N-A); ends SMOKE-PASS | SMOKE-FAIL
    s51_score.py score51 <ledger> [...]                 every registered readout, T1 / T2 per cell, the A/A resolution, W-ROUTE, the delta table; VERDICT51 and one LABEL51 line per label
    s51_score.py d80 [--draws N] [--nperm N] [--out FILE] [--cells ...]    the pre-registration d80 table (item 5) on the historical rows; machine-readable JSON via --out (tools/l1/d80_51.json)

THE STATISTIC (section 4): a TWO-SIDED rank-sum per clause (a step has no worse direction), NOT seed-stratified (each arm has 3 rows per seed by construction), the min-p over a family of ONE preset (4 clauses:
goodput, in-order delay mean, client CPU/GB, server CPU/GB), the null from the EXACT enumeration of the relabellings (924 for 6 v 6, 18 564 for 6 v 12; a Monte Carlo of 10 000 only where the exact set exceeds
20 000, i.e. the reported cell-stratified joint), a row's clause vector moving as a unit. A preset FAILS iff its family p <= 0.05 / 3 (Bonferroni over the three presets). A DNF is ranked worst on every clause
(T2 only: DNFs do not enter T1). Wording rule: a non-rejection is reported as "no step above d80 = x % at P", never as a claim of continuity.

Nothing here re-derives a law; the r(delta) bands of section 5.1 are the registered predictions printed beside the readouts."""
import itertools
import json
import math
import os
import random
import re
import sys
from bisect import bisect_left

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import plan51  # noqa: E402

CLAUSES = ("goodput", "inorder", "cpu_cli", "cpu_srv")
HIGHER_BETTER = {"goodput"}                     # the DNF rank: lowest goodput, highest of the others
ALPHA, N_PRESETS = 0.05, 3
THRESH = ALPHA / N_PRESETS                      # FAIL at a preset iff its family p <= 0.05 / 3
EXACT_MAX = 20000                               # enumerate the relabellings when there are at most this many
MC_DRAWS = 10000                                # the joint, cell-stratified reading
D80_FILE = os.path.join(HERE, "d80_51.json")
D80_GRID = (0.01, 0.02, 0.03, 0.05, 0.08, 0.12, 0.20, 0.30, 0.50)       # section 4's 1/2/3/5/8/12/20/30/50 %
LATE_NOT_OVERDRIVEN = {"dgev_over_hand": 0.02, "tau_sat": 0.10}         # s51-evict-design 3.2: the smoke witness limits (HR and NA rows)
PRESET_NAME = {"B": "Bulk", "A": "Auto", "R": "Realtime"}
WARM = '"warmup":true'
_TICK_T = re.compile(r"^\[DIAG\] t=([\d.]+)s ")
_SRC = re.compile(r" src=(\d+)sym/s")
_PAUSED = re.compile(r" paused=(\d+)% ")
_CWI = re.compile(r" cwnd=(\d+) infl=(\d+) ")
_CUM = re.compile(r" cum=(\d+)/(\d+)/(\d+)")
_TAPER = re.compile(r" mpr\[[^\]]*?taper=(\d+)")
_RETX = re.compile(r" retx=(\d+)")
_DGQ = re.compile(r" dgq(\d+)\[hand=(\d+) tx=(\d+)")
_DGEV = re.compile(r" dgev(\d+)=(\d+)")
_PLU = re.compile(r" p(\d+):infl=.*? plu=([\d.]+) plc=([\d.]+)")
_SPAN = re.compile(r"\[SPAN\] t=([\d.]+)s .*? wsize=(\d+) .*? rr=([-+\d.eE]+) ")
_SUMCAP_PIN = re.compile(r"\[SUMCAP\] .*? pin=([\d.]+)")
MAX_CWND = 10000

# section 5.1: the registered r(delta) bands (plu - plc) per cell, printed beside the consumed / granted readings. key = the sweep point.
R_BANDS = {  # point: (delta, c2 band, c8 (c3 leg) band)
    "HB": (0.005, (0.0, 0.0), (0.0, 0.0)), "BP": (0.0051, (0.0003, 0.0005), (0.0003, 0.0007)), "I1": (0.0075, (0.007, 0.011), (0.007, 0.015)),
    "I2": (0.0158, (0.023, 0.036), (0.021, 0.046)), "I3": (0.05, (0.054, 0.081), (0.049, 0.100)), "HA": (0.5, (0.133, 0.197), (0.120, 0.227)),
    "I4": (5.0, (0.156, 0.225), (0.139, 0.240)), "RP": (51.0, (0.176, 0.249), (0.154, 0.246)),
}
R_BAND_ALIAS = {"NA": "HA", "AM": "HA", "AP": "HA", "BM": "HB", "RM": "RP", "HR": "RP"}
R_SAT = {"c2-100": "0.315", "c8-100": "0.245-0.250"}
CROSS = {"c2-100": "0.0142-0.0267", "c8-100": "0.0173-0.0515"}
DELTA_STAR = {"c2-100": "0.011-0.012", "c8-100": "0.013-0.018"}
PRED_T2 = {"A": "PASS (identical laws)", "B": "PASS predicted (queue_target_mult 1.25 vs 1.125; a FAIL is a K01 verdict)", "R": "FAIL predicted in both cells (512 B symbols, S09 batching out of scope; S08 at c8)"}
GOODPUT_PRED = {"A": "0.82-0.90 of Bulk", "R": "0.81-0.86 of Bulk"}


def med(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def _f(x, nd=3):
    return "-" if x is None else f"{x:.{nd}f}"


def _share(num, den):
    return None if not den else num / den


# ───────────────────────────── row parsing (the VM side) ─────────────────────────────
def row_metrics(cli, srv, drv, nbytes=None, cell=plan51.C2, arm="HB", mbps=None):
    """The `t51` of one row: the section 3 readouts (tail52_score's goodput / retx / duplicates / CPU, the in-order delay run and source-phase, consumed r, the [SPAN] rr statistics, wsize, source symbols per byte,
    [SUMCAP] pin, cwnd / infl headroom, plu / plc) and section 3.2's added ones (saturated-tick share, eviction shares, the MAX_CWND bind share, the c2 spare-clamp tick bracket). Never raises on log content."""
    import stage3_parse as s3
    import tail52_score as t52
    import threadp1_parse as t1
    cli, srv, drv = list(cli or []), list(srv or []), list(drv or [])
    hint = plan51.arm_hint(arm)
    m = {"v": 1, "cell": cell, "arm": arm, "hint": hint, "delta": plan51.arm_delta(arm)}
    try:
        base = t52.row_metrics(cli, srv, drv, nbytes)
    except Exception:
        base = {}
    for k in ("goodput_mbps", "retx_per_gb", "rx_dup", "cpu_cli_s_per_gb", "cpu_srv_s_per_gb", "tail_s"):
        m[k] = base.get(k)
    try:
        run, src_only = t1.inorder_delay_stats(cli), t1.inorder_delay_stats(cli, source_only=True)
    except Exception:
        run = src_only = None
    m["inorder_ms"], m["inorder_src_ms"] = (run["mean"] * 1e3 if run else None), (src_only["mean"] * 1e3 if src_only else None)
    ticks, spans = [], []
    for ln in cli:
        if WARM in ln:
            ticks, spans = [], []
            continue
        if ln.startswith("[DIAG]"):
            mt = _TICK_T.match(ln)
            if not mt:
                continue
            sr, pa, cw, cu, tp, rx = _SRC.search(ln), _PAUSED.search(ln), _CWI.search(ln), _CUM.search(ln), _TAPER.search(ln), _RETX.search(ln)
            dq = {int(a): (int(b), int(c)) for a, b, c in _DGQ.findall(ln)}
            ev = {int(a): int(b) for a, b in _DGEV.findall(ln)}
            ticks.append({"t": float(mt.group(1)), "src": int(sr.group(1)) if sr else None, "paused": int(pa.group(1)) if pa else None, "cwnd": int(cw.group(1)) if cw else None,
                          "infl": int(cw.group(2)) if cw else None, "cum": tuple(int(x) for x in cu.groups()) if cu else None, "taper": int(tp.group(1)) if tp else None,
                          "retx": int(rx.group(1)) if rx else None, "dgq": dq, "dgev": ev, "plu": {int(a): (float(b), float(c)) for a, b, c in _PLU.findall(ln)}})
        else:
            sp = _SPAN.search(ln)
            if sp:
                spans.append((float(sp.group(1)), int(sp.group(2)), float(sp.group(3))))
    m["n_ticks"] = len(ticks)
    m["last_tick_s"] = max((k["t"] for k in ticks), default=0.0)
    send = [k for k in ticks if (k["src"] or 0) > 0]
    t_src_end = max((k["t"] for k in send), default=None)
    m["t_src_end"] = t_src_end
    sat = [k for k in ticks if (k["src"] or 0) > 1000 and k["cwnd"] is not None and k["infl"] is not None]
    m["tau_sat"] = _share(sum(1 for k in sat if k["infl"] >= k["cwnd"]), len(sat))                           # section 3.2: saturated-tick share
    sending = [k for k in sat if k["paused"] == 0]
    m["tau_x"] = _share(sum(1 for k in sending if k["infl"] >= k["cwnd"]), len(sending))
    cw = [k for k in send if k["cwnd"] is not None]
    m["maxcwnd_share"] = _share(sum(1 for k in cw if k["cwnd"] == MAX_CWND), len(cw))                       # section 3.2: MAX_CWND bind share
    last = ticks[-1] if ticks else None
    hand = {i: v[0] for i, v in (last["dgq"] if last else {}).items()}
    tx = {i: v[1] for i, v in (last["dgq"] if last else {}).items()}
    dgev = (last["dgev"] if last else {})
    m["evict_share_leg"] = {str(i): (hand[i] - tx[i]) / hand[i] if hand[i] else None for i in hand}         # (hand - tx) / hand at the last tick, per leg
    th, tt = sum(hand.values()), sum(tx.values())
    m["evict_share"] = (th - tt) / th if th else None
    m["dgev_over_hand"] = (sum(dgev.values()) / th) if (th and dgev) else None                                # the smoke witness: dgev / hand at the end
    # the spare-clamp tick bracket (c2 only: single path, spare_capacity is that leg's own); c8 has no per-leg scheduler cwnd print: MISSING
    if cell == plan51.C2:
        pts = [k for k in send if k["cwnd"] is not None and k["infl"]]
        s = [max(0, k["cwnd"] - k["infl"]) / k["infl"] for k in pts]
        m["spare_lo"], m["spare_hi"] = _share(sum(1 for x in s if x == 0), len(s)), _share(sum(1 for x in s if x < 0.5), len(s))
    else:
        m["spare_lo"] = m["spare_hi"] = None
    m["headroom_med"] = med([(k["cwnd"] - k["infl"]) / k["cwnd"] for k in send if k["cwnd"] and k["infl"] is not None])
    # consumed r = (cod + taper) / src from the last [DIAG]; the coded share
    cum = [k for k in ticks if k["cum"]]
    if cum:
        s_, c_ = cum[-1]["cum"][0], cum[-1]["cum"][1]
        m["src_total"], m["cod_total"], m["taper_total"] = s_, c_, cum[-1]["taper"]
        m["r_consumed"] = (c_ + (cum[-1]["taper"] or 0)) / s_ if s_ else None
        m["coded_share"] = c_ / (s_ + c_) if (s_ + c_) else None
        m["src_per_byte"] = s_ / nbytes if nbytes else None
    else:
        m["src_total"] = m["cod_total"] = m["taper_total"] = m["r_consumed"] = m["coded_share"] = m["src_per_byte"] = None
    # [SPAN]: rr (the cached granted rate, three decimals) over the source phase; wsize
    ph = [s for s in spans if t_src_end is None or s[0] <= t_src_end + 0.5]
    rr = [s[2] for s in ph]
    m["rr_mean"], m["rr_p50"] = mean(rr), med(rr)
    m["rr_nz_frac"] = _share(sum(1 for x in rr if x > 0), len(rr))
    m["rr_max"] = max(rr) if rr else None
    m["wsize_med"] = med([s[1] for s in ph])
    m["n_span"] = len(ph)
    pins = [float(mm.group(1)) for mm in (_SUMCAP_PIN.search(ln) for ln in cli if "[SUMCAP]" in ln) if mm]
    m["sumcap_pin"] = med(pins)
    # plu / plc of each leg (last tick) and the leg the rate read (the worst-epsilon path: the larger plu)
    pl = {}
    for k in ticks:
        for i, (a, b) in k["plu"].items():
            pl.setdefault(i, []).append((a, b))
    m["plu_med"] = {str(i): med([x[0] for x in v]) for i, v in pl.items()}
    m["plc_last"] = {str(i): v[-1][1] for i, v in pl.items()}
    try:
        g = s3.diag_gauges(cli)
        m["plu_gauge"] = {k: g[k] for k in g if k.startswith(("plu_med_p", "plc_p"))}
    except Exception:
        m["plu_gauge"] = {}
    m["mbps"] = mbps
    return m


# ───────────────────────────── the statistic ─────────────────────────────
def midranks(vals):
    """Midranks (1-based) of a list."""
    order = sorted(range(len(vals)), key=lambda i: vals[i])
    r = [0.0] * len(vals)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and vals[order[j + 1]] == vals[order[i]]:
            j += 1
        for k in range(i, j + 1):
            r[order[k]] = (i + j) / 2.0 + 1.0
        i = j + 1
    return r


def clause_value(row, clause):
    """A ledger row's clause value, with the DNF rule (a DNF is ranked worst on every clause: lowest goodput, highest of the others). None when a completed row lacks the reading."""
    t = row.get("t51") or {}
    if row.get("dnf"):
        return -math.inf if clause in HIGHER_BETTER else math.inf
    v = {"goodput": row.get("mbps"), "inorder": t.get("inorder_ms"), "cpu_cli": t.get("cpu_cli_s_per_gb"), "cpu_srv": t.get("cpu_srv_s_per_gb")}[clause]
    return v


def vectors(rows, clauses=CLAUSES):
    """-> (kept vectors, dropped count): a row with a missing clause reading is dropped whole (its clause vector moves as a unit)."""
    out, dropped = [], 0
    for r in rows:
        v = [clause_value(r, c) for c in clauses]
        if any(x is None for x in v):
            dropped += 1
        else:
            out.append(v)
    return out, dropped


def family_p(strata, clauses=CLAUSES, nmc=MC_DRAWS, seed=51, force_mc=False):
    """The min-p family over `clauses` for one or several strata. strata = [(A_vectors, B_vectors)]; the relabelling set is every way to choose the A rows within each stratum (exact when its size is at most EXACT_MAX
    and not force_mc, else nmc random relabellings plus the observed one). stat_c = |sum over strata of (rank-sum of A within the stratum - nA (N + 1) / 2)| with midranks per stratum and clause; the per-clause
    p of a relabelling = the share of relabellings with stat >= its stat; the family statistic = the min over clauses; family p = the share of relabellings whose family statistic <= the observed one.
    -> dict(p_family, p_clause {clause: p}, M, exact, sizes)."""
    nc = len(clauses)
    prep = []
    for A, B in strata:
        allv = A + B
        n, na = len(allv), len(A)
        ranks = [midranks([v[c] for v in allv]) for c in range(nc)]
        prep.append((n, na, ranks, na * (n + 1) / 2.0))
    combos = 1
    for n, na, _, _ in prep:
        combos *= math.comb(n, na)
    exact = (combos <= EXACT_MAX) and not force_mc
    obs = tuple(tuple(range(na)) for _, na, _, _ in prep)
    if exact:
        per = [list(itertools.combinations(range(n), na)) for n, na, _, _ in prep]
        rel = list(itertools.product(*per))
    else:
        rng = random.Random(seed)
        rel = [obs] + [tuple(tuple(sorted(rng.sample(range(n), na))) for n, na, _, _ in prep) for _ in range(nmc)]

    def stats(r):
        out = []
        for c in range(nc):
            tot = 0.0
            for (n, na, ranks, e), idx in zip(prep, r):
                rk = ranks[c]
                tot += sum(rk[i] for i in idx) - e
            out.append(abs(tot))
        return out
    S = [stats(r) for r in rel]
    M = len(S)
    srt = [sorted(s[c] for s in S) for c in range(nc)]
    cnt = [[M - bisect_left(srt[c], s[c]) for c in range(nc)] for s in S]       # integer counts: #relabellings with stat >= this one (no float compare of p-values)
    mc = [min(x) for x in cnt]
    obs_stats = stats(obs)
    obs_cnt = [M - bisect_left(srt[c], obs_stats[c]) for c in range(nc)]
    mobs = min(obs_cnt)
    pf = sum(1 for x in mc if x <= mobs) / M
    return {"p_family": pf, "p_clause": {clauses[c]: obs_cnt[c] / M for c in range(nc)}, "M": M, "exact": exact, "sizes": [(n, na) for n, na, _, _ in prep]}


def preset_test(rows_a, rows_b, strata_extra=None):
    """One family p for two row groups (A = the first group, B = the second). -> dict, or None when a group has fewer than 3 usable rows."""
    va, da = vectors(rows_a)
    vb, db = vectors(rows_b)
    if len(va) < 3 or len(vb) < 3:
        return {"unscoreable": True, "n": (len(va), len(vb)), "dropped": da + db}
    r = family_p([(va, vb)])
    r.update({"n": (len(va), len(vb)), "dropped": da + db, "unscoreable": False})
    worst = min(r["p_clause"].items(), key=lambda kv: kv[1])
    r["clause"] = worst[0]
    r["fail"] = r["p_family"] <= THRESH
    return r


# ───────────────────────────── d80: the bootstrap resolution ─────────────────────────────
def shift_vec(v, clause_idx, delta, clauses=CLAUSES):
    w = list(v)
    c = clauses[clause_idx]
    w[clause_idx] = v[clause_idx] * (1 - delta) if c in HIGHER_BETTER else v[clause_idx] * (1 + delta)
    return w


def d80(pool, n_a, n_b, draws=200, nperm=400, grid=D80_GRID, seed=51, clauses=CLAUSES, exact_max=1000):
    """The smallest grid shift caught (family p <= 0.05 / 3) in >= 80 % of `draws` for each clause, A resampled with replacement n_a from the pool, B n_b, the shift applied to B's one clause. The null is the
    scorer's own: the EXACT enumeration where the relabellings number at most `exact_max` (6 v 6: 924), else `nperm` random relabellings (6 v 12: 18 564 -> 400). A Monte Carlo null at 6 v 6 cannot reach the
    threshold however large the shift (a split and its complement tie, so ~40 % of draws put the extremes twice among 400 and the family p floors at 8 / 401 > 0.05 / 3), which the registered test is not subject to.
    pool = [clause vectors]. -> {clause: d80 or None (above the grid)}."""
    rng = random.Random(seed)
    mc = math.comb(n_a + n_b, n_a) > exact_max
    out = {}
    for ci, c in enumerate(clauses):
        res = None
        for dl in grid:
            hits = 0
            for _ in range(draws):
                A = [list(rng.choice(pool)) for _ in range(n_a)]
                B = [shift_vec(rng.choice(pool), ci, dl) for _ in range(n_b)]
                p = family_p([(A, B)], clauses, nmc=nperm, seed=rng.randrange(1 << 30), force_mc=mc)["p_family"]
                hits += p <= THRESH
            if hits / draws >= 0.8:
                res = dl
                break
        out[c] = res
    return out


# ───────────────────────────── the scoring side ─────────────────────────────
class RV:
    def __init__(self, r, order=0):
        self.r, self.t, self.order = r, (r.get("t51") or {}), order
        self.cell, self.arm = r.get("cell"), r.get("arm")
        self.rep, self.seed = int(r.get("rep") or 0), str(r.get("seed"))
        self.live = r.get("status") == "LIVE" and bool(self.t)
        self.dnf = bool(r.get("dnf"))


def group(rvs, cell, arms, completed_only=False):
    out = [rv.r for rv in rvs if rv.cell == cell and rv.arm in arms and rv.live and not (completed_only and rv.dnf)]
    return out


def rr_inert(rvs, cell):
    """W-ROUTE (section 4): rr is exactly 0 on every HB and BM row; if rr == 0 on EVERY row with beta < 1 in the cell the session reads R-INERT, attributed in this order: spare-bound (cwnd - infl ~ 0 on the
    sending ticks), estimator-bound (plu below the tail target t = 1e-5 delta_Auto / delta), wiring. -> (hb_bm_zero, inert, attribution)."""
    rows = [rv for rv in rvs if rv.cell == cell and rv.live]
    hb_bm = [rv for rv in rows if rv.arm in ("HB", "BM")]
    hb_bm_zero = all((rv.t.get("rr_max") or 0) == 0 for rv in hb_bm) if hb_bm else None
    lt1 = [rv for rv in rows if (plan51.arm_beta(rv.arm) or 0) < 1]
    inert = bool(lt1) and all((rv.t.get("rr_max") or 0) == 0 for rv in lt1)
    attr = None
    if inert:
        sp = med([rv.t.get("spare_lo") for rv in lt1])
        if sp is not None and sp >= 0.5:
            attr = "spare-bound (cwnd - infl = 0 on most sending ticks)"
        else:
            plu = med([v for rv in lt1 for v in (rv.t.get("plu_med") or {}).values()])
            attr = "estimator-bound (plu %s below the tail target)" % _f(plu, 4) if (plu is not None and plu < 1e-5 * plan51.DELTA_AUTO / 0.5) else "wiring (neither spare-bound nor estimator-bound)"
    return hb_bm_zero, inert, attr


def sweep_points():
    """[(point name, [arms], delta)] in increasing delta: the r(delta) table rows."""
    return [("H-B 0.005", ["HB"], 0.005), ("B+ 0.0051", ["BP"], 0.0051), ("I1 0.0075", ["I1"], 0.0075), ("I2 0.0158", ["I2"], 0.0158), ("I3 0.05", ["I3"], 0.05),
            ("Auto 0.5 (H-A, N-A)", ["HA", "NA"], 0.5), ("I4 5", ["I4"], 5.0), ("Realtime R+ 51", ["RP"], 51.0)]


def band_of(arm, cell):
    k = arm if arm in R_BANDS else R_BAND_ALIAS.get(arm)
    if k is None:
        return None
    b = R_BANDS[k][1 if cell == plan51.C2 else 2]
    return f"{b[0]:g}-{b[1]:g}"


def load_d80(path=D80_FILE):
    try:
        return json.load(open(path))
    except (OSError, ValueError):
        return None


def aa_resolution(rvs, cell, draws=40, nperm=400):
    """The in-session A/A (H-A against N-A, same delta): its bootstrap d80 per clause from the pooled H-A / N-A rows (6 + 6), at `draws` bootstrap draws (the VM scoring budget; the historical table uses 200)."""
    rows = group(rvs, cell, ("HA", "NA"), completed_only=True)
    pool, _ = vectors(rows)
    if len(pool) < 6:
        return None
    return d80(pool, 6, 6, draws=draws, nperm=nperm, seed=7)


def score(paths, out=print, d80_path=D80_FILE, aa_draws=40):
    import place38_parse as pp
    import pool56_score as ps
    rows_all, tokens = pp.rows_of(paths)
    rows_all = [r for r in rows_all if r.get("arm") in plan51.ARMS and r.get("cell") in plan51.CELLS]
    cnt, replaced = ps.counting(rows_all)
    idx = {id(r): i for i, r in enumerate(rows_all)}
    cnt.sort(key=lambda r: idx[id(r)])
    rvs = [RV(r, i) for i, r in enumerate(cnt)]
    cells = [c for c in plan51.CELLS if any(rv.cell == c for rv in rvs)]
    labels = []

    def lab(name, val):
        labels.append((name, val))
    n_exp = sum(plan51.expect_rows(cell=c) for c in cells) if cells else 0
    out(f"S51 SCORE (the off-Bulk baseline and the delta sweep; two-sided rank-sum min-p per preset, exact relabellings, FAIL iff family p <= 0.05/3 = {THRESH:.4f}); REPORTED + SCORED T1 / T2; "
        f"ABORT-TOKENS {' '.join(sorted({t for t in tokens if t in pp.ABORT_TOKENS})) or 'none'}")
    for r in replaced:
        out(f"  REPLACED {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} {' '.join(r['problems'])}")
    voids = [rv for rv in rvs if not rv.live]
    for rv in voids:
        out(f"  VOID {rv.cell} {rv.arm} s{rv.seed} rep{rv.rep} {rv.r.get('status')} {' '.join(rv.r.get('problems') or [])}")
    for rv in rvs:
        if rv.r.get("t51_error"):
            out(f"  T51-ERROR {rv.cell} {rv.arm} s{rv.seed} rep{rv.rep}: {rv.r['t51_error']}")
    hist = load_d80(d80_path)
    t1_all = {}
    for cell in cells:
        short = cell.split("-")[0]
        out(f"== {cell}: {sum(1 for rv in rvs if rv.cell == cell and rv.live)} live rows, {sum(1 for rv in rvs if rv.cell == cell and not rv.live)} void, "
            f"{sum(1 for rv in rvs if rv.cell == cell and rv.live and rv.dnf)} DNF")
        # per-arm readouts
        for arm in plan51.ARMS:
            rs = [rv for rv in rvs if rv.cell == cell and rv.arm == arm and rv.live]
            if not rs:
                continue
            out(f"  {arm:<3} n={len(rs)} dnf={sum(rv.dnf for rv in rs)} goodput {_f(med([rv.r.get('mbps') for rv in rs if not rv.dnf]), 1)} Mbit/s; in-order {_f(med([rv.t.get('inorder_ms') for rv in rs]), 1)} ms "
                f"(B' source phase {_f(med([rv.t.get('inorder_src_ms') for rv in rs]), 1)}); CPU/GB cli {_f(med([rv.t.get('cpu_cli_s_per_gb') for rv in rs]), 2)} srv {_f(med([rv.t.get('cpu_srv_s_per_gb') for rv in rs]), 2)}; "
                f"retx/GB {_f(med([rv.t.get('retx_per_gb') for rv in rs]), 0)}; rx_dup {_f(med([rv.t.get('rx_dup') for rv in rs]), 0)}; coded share {_f(med([rv.t.get('coded_share') for rv in rs]), 4)}; "
                f"consumed r {_f(med([rv.t.get('r_consumed') for rv in rs]), 4)}; granted rr mean {_f(med([rv.t.get('rr_mean') for rv in rs]), 4)} p50 {_f(med([rv.t.get('rr_p50') for rv in rs]), 3)} nz {_f(med([rv.t.get('rr_nz_frac') for rv in rs]), 2)}; "
                f"W {_f(med([rv.t.get('wsize_med') for rv in rs]), 0)}; SUMCAP pin {_f(med([rv.t.get('sumcap_pin') for rv in rs]), 2)}; src/byte {_f(med([rv.t.get('src_per_byte') for rv in rs]), 7)}")
            out(f"      s51-evict 3.2: tau_x {_f(med([rv.t.get('tau_sat') for rv in rs]), 3)} (saturated-tick share); eviction share {_f(med([rv.t.get('evict_share') for rv in rs]), 4)}; dgev/hand {_f(med([rv.t.get('dgev_over_hand') for rv in rs]), 4)}; "
                f"MAX_CWND bind {_f(med([rv.t.get('maxcwnd_share') for rv in rs]), 3)}; " + (f"spare-clamp bracket: lower share(s = 0) {_f(med([rv.t.get('spare_lo') for rv in rs]), 3)}, upper share(s < 0.5) {_f(med([rv.t.get('spare_hi') for rv in rs]), 3)}" if cell == plan51.C2 else "spare-clamp bracket: MISSING at c8 ([DIAG] prints no per-leg scheduler cwnd)"))
        # W-ROUTE
        hbz, inert, attr = rr_inert(rvs, cell)
        out(f"  W-ROUTE {short}: rr exactly 0 on every H-B / B- row: {hbz}; R-INERT (rr = 0 on every beta < 1 row): {inert}" + (f" -> attributed {attr}" if inert else ""))
        lab(f"W-ROUTE-{short}", ("FAIL" if hbz is False else "PASS" if hbz else "UNSCOREABLE") + (" R-INERT" if inert else ""))
        # the r(delta) table, the P_fec side and delta*
        out(f"  r(delta) {short}: point | consumed r (minus the H-B coded share) | granted rr mean | registered band (plu - plc) | plu/(1-plu) | plc/(1-plc)   [r_sat {R_SAT[cell]}, crossing eps/(1-eps) {CROSS[cell]}]")
        hb_cs = med([rv.t.get("coded_share") for rv in rvs if rv.cell == cell and rv.arm == "HB" and rv.live])
        hb_rc = med([rv.t.get("r_consumed") for rv in rvs if rv.cell == cell and rv.arm == "HB" and rv.live])
        star = None
        for name, arms, dl in sweep_points():
            rs = [rv for rv in rvs if rv.cell == cell and rv.arm in arms and rv.live]
            if not rs:
                continue
            rc = med([rv.t.get("r_consumed") for rv in rs])
            rcm = None if (rc is None or hb_rc is None) else rc - hb_rc
            plu = med([max(rv.t.get("plu_med", {"0": None}).values(), key=lambda x: x or 0) for rv in rs if rv.t.get("plu_med")])
            plc = med([max(rv.t.get("plc_last", {"0": None}).values(), key=lambda x: x or 0) for rv in rs if rv.t.get("plc_last")])
            cu = None if plu is None else plu / (1 - plu)
            cc = None if plc is None else plc / (1 - plc)
            rrm = med([rv.t.get("rr_mean") for rv in rs])
            out(f"    {name:<22} {_f(rc, 4)} ({_f(rcm, 4)})  {_f(rrm, 4)}  band {band_of(arms[0], cell)}  {_f(cu, 4)}  {_f(cc, 4)}")
            if star is None and rcm is not None and cu is not None and rcm > cu and dl > 0.005:
                star = dl
        out(f"  delta* {short} (provisional: P2's channel split is missing; first sweep point whose consumed r exceeds plu/(1-plu)): {star if star is not None else 'not reached'}   [predicted {DELTA_STAR[cell]}]")
        lab(f"DELTA-STAR-{short}", f"PROVISIONAL {star if star is not None else 'not-reached'}")
        # goodput vs delta, monotone check (reported)
        gp = []
        for name, arms, dl in sweep_points():
            v = [rv.r.get("mbps") for rv in rvs if rv.cell == cell and rv.arm in arms and rv.live and not rv.dnf and rv.r.get("mbps") is not None]
            if v:
                gp.append((dl, med(v)))
        viol = [(a, b) for a, b in zip(gp, gp[1:]) if b[1] > a[1] * 1.02]
        out(f"  goodput against delta: " + " ".join(f"{d:g}:{g:.1f}" for d, g in gp) + f"   monotone non-increasing (2 % slack): {not viol}" + ("" if not viol else f"; violated at {[(a[0], b[0]) for a, b in viol]} (reported)"))
        # A/A resolution
        aa = aa_resolution(rvs, cell, draws=aa_draws)
        h = ((hist or {}).get(cell) or {}).get("T1") if hist else None
        if aa is None:
            out(f"  A/A {short}: fewer than 6 usable H-A + N-A rows: in-session resolution not computed")
            reduced = None
        else:
            line = " ".join(f"{c} {_f(None if aa[c] is None else 100 * aa[c], 0)} %" for c in CLAUSES)
            hl = " ".join(f"{c} {_f(None if h.get(c) is None else 100 * h[c], 0)} %" for c in CLAUSES) if h else "NOT AVAILABLE (tools/l1/d80_51.json missing)"
            reduced = [c for c in CLAUSES if h and aa.get(c) is not None and h.get(c) is not None and aa[c] > 2 * h[c]]
            out(f"  A/A {short} (in-session bootstrap d80, H-A + N-A rows, {aa_draws} draws): {line}; historical d80: {hl}; " + (f"REDUCED RESOLUTION on {','.join(reduced)} (in-session > 2x historical)" if reduced else "resolution as registered"))
        lab(f"AA-RESOLUTION-{short}", "REDUCED " + ",".join(reduced) if reduced else ("UNCOMPUTED" if aa is None else "AS-REGISTERED"))
        # T1
        t1 = {}
        for P, (lo, hi) in plan51.PAIRS.items():
            a, b = group(rvs, cell, (lo,), completed_only=True), group(rvs, cell, (hi,), completed_only=True)
            res = preset_test(a, b)
            t1[P] = res
            if res.get("unscoreable"):
                out(f"  T1 {short} {PRESET_NAME[P]}: UNSCOREABLE (usable rows {res['n']})")
                lab(f"T1-{short}-{PRESET_NAME[P]}", "UNSCOREABLE")
            else:
                d80c = (((hist or {}).get(cell) or {}).get("T1") or {}).get(res["clause"]) if hist else None
                word = (f"a step at {PRESET_NAME[P]} on {res['clause']} (family p {res['p_family']:.4f} <= {THRESH:.4f}); entered as a ledger defect" if res["fail"] else
                        f"no step above d80 = {'n/a (tools/l1/d80_51.json not read)' if d80c is None else _f(100 * d80c, 0) + ' %'} at {PRESET_NAME[P]} (family p {res['p_family']:.4f})")
                out(f"  T1 {short} {PRESET_NAME[P]}: {'FAIL' if res['fail'] else 'PASS'} -- {word}; n {res['n'][0]} v {res['n'][1]}, {res['M']} relabellings ({'exact' if res['exact'] else 'Monte Carlo'}); per-clause p "
                    + " ".join(f"{c} {v:.4f}" for c, v in res["p_clause"].items()) + (f"; {res['dropped']} row(s) dropped for a missing reading" if res["dropped"] else ""))
                lab(f"T1-{short}-{PRESET_NAME[P]}", f"{'FAIL' if res['fail'] else 'PASS'} p={res['p_family']:.4f}")
        t1_all[cell] = t1
        # T2
        for P, hint_arm, dial_arms in (("A", "HA", ("NA",)), ("B", "HB", ("BM", "BP")), ("R", "HR", ("RM", "RP"))):
            named = group(rvs, cell, (hint_arm,))
            union = group(rvs, cell, dial_arms)
            if P != "A" and t1[P].get("unscoreable") is False and t1[P]["fail"]:
                parts = [(a_, preset_test(named, group(rvs, cell, (a_,)))) for a_ in dial_arms]
                out(f"  T2 {short} {PRESET_NAME[P]}: NOT SCORED (T1 failed at {PRESET_NAME[P]}); reported against each nudge: " + "; ".join(f"{a_} p {r_['p_family']:.4f} (clause {r_['clause']})" if not r_.get('unscoreable') else f"{a_} unscoreable" for a_, r_ in parts))
                lab(f"T2-{short}-{PRESET_NAME[P]}", "NOT-SCORED (T1 FAIL)")
                continue
            res = preset_test(named, union)
            if res.get("unscoreable"):
                out(f"  T2 {short} {PRESET_NAME[P]}: UNSCOREABLE (usable rows {res['n']})")
                lab(f"T2-{short}-{PRESET_NAME[P]}", "UNSCOREABLE")
                continue
            tag = "FAIL" if res["fail"] else "PASS"
            extra = ""
            if P == "A" and res["fail"]:
                extra = "; N-A against H-A is the same law: an instrument or harness defect, the session is NO VERDICT until it is explained"
            if P == "R" and res["fail"]:
                extra = "; the measured size of the section 3.3 debts (symbol size, S09 batching, K01 table; S08 at c8): each failing clause is a ledger entry"
            out(f"  T2 {short} {PRESET_NAME[P]}: {tag} -- the named hint {hint_arm} against {'+'.join(dial_arms)} as a number (family p {res['p_family']:.4f}, clause {res['clause']}; n {res['n'][0]} v {res['n'][1]}, {res['M']} relabellings "
                f"{'exact' if res['exact'] else 'MC'}); per-clause p " + " ".join(f"{c} {v:.4f}" for c, v in res["p_clause"].items()) + f"   [predicted {PRED_T2[P]}]" + extra)
            lab(f"T2-{short}-{PRESET_NAME[P]}", f"{tag} p={res['p_family']:.4f}")
        # the cell-stratified joint is below
    # the cell-stratified joint T1 (reported, never overrides a per-cell FAIL)
    if len(cells) == 2:
        for P, (lo, hi) in plan51.PAIRS.items():
            strata = []
            for c in cells:
                va, _ = vectors(group(rvs, c, (lo,), completed_only=True))
                vb, _ = vectors(group(rvs, c, (hi,), completed_only=True))
                if len(va) >= 3 and len(vb) >= 3:
                    strata.append((va, vb))
            if len(strata) == 2:
                r = family_p(strata)
                out(f"  T1 joint (cell-stratified, REPORTED, does not override a per-cell FAIL) {PRESET_NAME[P]}: family p {r['p_family']:.4f} over {r['M']} relabellings ({'exact' if r['exact'] else 'Monte Carlo'})")
                lab(f"T1-JOINT-{PRESET_NAME[P]}", f"p={r['p_family']:.4f}")
    nl = sum(1 for rv in rvs if rv.live)
    nf2 = sum(1 for rv in voids if rv.r.get("status") == "VOID-F2")
    out(f"VERDICT51 REPORTED-WITH-SCORED slots={len(set((r['cell'], r['arm'], str(r['seed']), str(r['rep'])) for r in rows_all))}/{n_exp} live={nl} void={len(voids)} replaced={len(replaced)} dnf={sum(1 for rv in rvs if rv.dnf)} labels={len(labels)} void_f2={nf2}")
    for n_, v in labels:
        out(f"LABEL51 {n_} {v}")
    return 0


# ───────────────────────────── the smoke / rehearsal ─────────────────────────────
def smoke(path, which, out=print):
    """s51-design 6.7 and s51-evict-design 3.2: one row per arm kind (H-B, H-R, B+, N-A) LIVE with the RWM_DELTA echo on both ends, the hint pipe, the batching echo, rr parsed (exactly 0 on H-B; > 0 on N-A unless R-INERT
    is attributed; B+ is not a witness: its r ~ 0.0005 prints as 0.000), the in-order delay non-None, the H-R row inside the 200 s timeout, and the NOT-OVERDRIVEN witness on H-R and N-A: dgev/hand <= 0.02 and
    tau_x <= 0.10. A failing row is NO-GO (SMOKE-FAIL): the session is amended before the battery."""
    import place38_parse as pp
    if which not in ("smoke", "rehearsal"):
        out(f"unknown set {which}")
        return 2
    cell = plan51.cell_from_env()
    slots = plan51.plan(smoke=True, cell=cell)
    rows, _ = pp.rows_of([path])
    got = {(r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep"))): r for r in rows}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    out(f"SMOKE51 set={which} cell={cell} rows={len(rows)} slots={len(slots)}")
    rvs, miss = {}, []
    for (c, _, a, sd, rp) in slots:
        r = got.get((c, a, str(sd), str(rp)))
        if r is None:
            miss.append(f"{c} {a}: no row")
        else:
            rvs[a] = RV(r)
    crit("one row per smoke slot ran with a t51", miss + [f"{a}: no t51 ({rv.r.get('t51_error')})" for a, rv in rvs.items() if not rv.t])
    for a, rv in rvs.items():
        t = rv.t
        out(f"  ROW {a} ({plan51.arm_hint(a)}, delta {plan51.delta_token(a) or 'unset'}): status={rv.r.get('status')} dnf={int(rv.dnf)} mbps={rv.r.get('mbps')} in-order {_f(t.get('inorder_ms'), 1)} ms rr max {_f(t.get('rr_max'), 3)} "
            f"dgev/hand {_f(t.get('dgev_over_hand'), 4)} tau_x {_f(t.get('tau_sat'), 3)} last tick {_f(t.get('last_tick_s'), 1)} s")

    def probs(a, pref):
        return [f"{a}: {x}" for x in (rvs[a].r.get("problems") or []) if x.startswith(pref)] if a in rvs else []
    present = list(rvs)
    crit("every smoke row is LIVE (no witness problem)", [f"{a}: {rv.r.get('status')} {' '.join(rv.r.get('problems') or [])}" for a, rv in rvs.items() if rv.r.get("status") != "LIVE"])
    crit("[GATES] RWM_DELTA equals the arm's value (or unset) on BOTH ends", [x for a in present for x in probs(a, "gate-delta")])
    crit("pipe_cli / pipe_srv = [window, Rlc, <the arm's hint>] and the header hint agrees", [x for a in present for p_ in ("pipe-echo", "no-pipe-echo", "header=") for x in probs(a, p_)])
    crit("the emission-batching echo: ACTIVE at bulk / auto, 'out of scope' at realtime, and not the other", [x for a in present for x in probs(a, "batch-echo")])
    crit("source symbols per byte match the carrier's symbol size; no [CHI] activity; the generation guard absent", [x for a in present for p_ in ("srcsize-", "chi-", "generation-guard") for x in probs(a, p_)])
    crit("every row ran the one seat's binary (LD_SHA)", [x for a in present for x in probs(a, "bin-sha")])
    if "HB" in rvs:
        crit("rr is exactly 0 on the H-B row (beta = 1: the proactive slot cannot fire)", [] if (rvs["HB"].t.get("rr_max") or 0) == 0 and rvs["HB"].t.get("n_span") else [f"HB: rr max {rvs['HB'].t.get('rr_max')} over {rvs['HB'].t.get('n_span')} [SPAN] lines"])
    else:
        out("NOT-RUN rr = 0 on H-B (no H-B row)")
    if "NA" in rvs:
        na = rvs["NA"].t
        inert_note = ""
        ok = (na.get("rr_max") or 0) > 0
        if not ok:
            hbz, inert, attr = rr_inert([rvs["NA"]], rvs["NA"].cell)
            inert_note = f" (R-INERT attributed: {attr})"
            ok = True       # s51-design 6.7: "> 0 on N-A unless R-INERT": attributed as section 4, not a failure
        out(("PASS " if ok else "FAIL ") + f"rr parsed on N-A and > 0 unless R-INERT is attributed: rr max {_f(na.get('rr_max'), 3)}{inert_note}")
    crit("the in-order delay is computed (non-None) on every completed row", [f"{a}: in-order delay None" for a, rv in rvs.items() if not rv.dnf and rv.t.get("inorder_ms") is None])
    crit(f"the H-R row finished inside RWM_PERF_TIMEOUT_S={plan51.TIMEOUT_S} (not a DNF)", [f"HR: dnf" for a, rv in rvs.items() if a == "HR" and rv.dnf] if "HR" in rvs else [])
    for a in ("HR", "NA"):
        if a not in rvs:
            continue
        t = rvs[a].t
        f_ = []
        if t.get("dgev_over_hand") is None or t["dgev_over_hand"] > LATE_NOT_OVERDRIVEN["dgev_over_hand"]:
            f_.append(f"{a}: dgev/hand {t.get('dgev_over_hand')} (want <= {LATE_NOT_OVERDRIVEN['dgev_over_hand']})")
        if t.get("tau_sat") is not None and t["tau_sat"] > LATE_NOT_OVERDRIVEN["tau_sat"]:
            f_.append(f"{a}: tau_x {t['tau_sat']:.3f} (want <= {LATE_NOT_OVERDRIVEN['tau_sat']})")
        crit(f"NOT-OVERDRIVEN witness on {a}: dgev/hand <= 0.02 and tau_x <= 0.10 (predicted 0.0000-0.009 and 0.02-0.04); a failing row is NO-GO, the session is amended before the battery", f_)
    crit("no row is short (the last tick at or after 30 s on a completed row)", [f"{a}: last tick {rv.t.get('last_tick_s')} s" for a, rv in rvs.items() if not rv.dnf and (rv.t.get("last_tick_s") or 0) < plan51.MIN_ROW_S])
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ───────────────────────────── the d80 pre-registration table (item 5) ─────────────────────────────
def d80_main(argv):
    import s51_d80
    return s51_d80.main(argv)


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "smoke51" and len(a) == 2:
        return smoke(a[1], a[0])
    if cmd == "score51" and a:
        return score(a)
    if cmd == "d80":
        return d80_main(a)
    print(f"usage: {sys.argv[0]} smoke51 <smoke|rehearsal> <ledger> | score51 <ledger> [...] | d80 [...]")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
