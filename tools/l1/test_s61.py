#!/usr/bin/env python3
"""Tests for status 61's scorer (tools/l1/s61_score.py; registration docs/derivations/s61-registration.md). The harness side is test_harness61.py, the launch paths run in test_stubrun.py.

  1. G(n*): the minimum exact on hand data, the weighted median, the dropped tail, parity;
  2. the two-sided floor gate (a negative slope is not passed trivially), a moved floor pool, fewer than 2 pools;
  3. CR61 on hand rows: a standing queue is not flagged, the G r1 shape is flagged by the ratio, a floor collapse is flagged only with its goodput cut;
  4. every pilot branch and the clamp;
  5. the rho three-way labels and FLOOR-SENSITIVE, rho = need / S_F on a two-row fixture (an absolute test);
  6. c2's frozen inputs reproduce s61pred.out (rho_2 0.797 [0.780, 0.825], W->0 0.877 [0.858, 0.908], Delta 0.085, X_own 10.66), and (host only: skipped without docs/l1-raw/s60 and r68allow.py) are recomputed from the tgz;
  7. row_extras on a gauge log, and the scorer end to end on synthetic ledgers with a KNOWN answer (AGREES, DISAGREES, a moved floor pool, CONTROL-MOVED, UNSCOREABLE-N).
Run: python3 test_s61.py"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import json  # noqa: E402
for _k in ("LD_PLAN", "LD_KNEE61", "LD_CTL61", "LD_REHEARSE", "LD_NOGO"):
    os.environ.pop(_k, None)
import plan61  # noqa: E402
import s60_score as s6  # noqa: E402
import s61_score as s  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol=1e-9):
    return a is not None and abs(a - b) <= tol


# ═══ 1. G(n*) and parity ═══
W = lambda tmin, p50, nk: {"tmin": tmin, "p50": p50, "nk": nk}      # noqa: E731
hand = [W(10.0, 12.0, 100), W(9.0, 13.0, 100), W(11.0, 14.0, 100), W(8.5, 12.5, 100), W(9.5, 13.5, 100)]
g = s.groups(hand, 250)
check("G(250): consecutive windows pooled until sum nk >= 250 (3 windows), the minimum of the group is the min of the minima (9.0), the last incomplete group (2 windows, 200 samples) is dropped",
      len(g) == 1 and g[0][0] == 9.0 and g[0][2] == 300, g)
check("G(250): the group p50 is the nk-weighted median of the windows' p50 (equal weights: the median 13.0)", g[0][1] == 13.0, g)
g2 = s.groups(hand, 200)
check("G(200): two groups of two windows, minima 9.0 and 8.5 (the tail window alone is dropped)", [x[0] for x in g2] == [9.0, 8.5] and len(g2) == 2, g2)
check("native (n* = None): one window each", [x[0] for x in s.groups(hand, None)] == [10.0, 9.0, 11.0, 8.5, 9.5])
check("the weighted median honours the weights: values 1 / 2 / 100 with weights 1 / 1 / 5 read 100", s.wmed([(1, 1), (2, 1), (100, 5)]) == 100 and s.wmed([(1, 2), (5, 1)]) == 1)
check("a window without tmin or p50 is skipped, never counted", len(s.groups([W(None, 1.0, 100), W(1.0, None, 100), W(9.0, 12.0, 300)], 250)) == 1)
S_, F_, n_, ng_ = s.reading(hand, 200)
check("reading(): S_F = median over groups of (p50 - tmin), F = median group minimum, n = median realised n", ng_ == 2 and near(F_, 8.75) and n_ == 200 and S_ is not None, (S_, F_, n_, ng_))
check("parity: 226-353 and 1 874-2 928 are IN, outside is BELOW / ABOVE (UNSCOREABLE-N is decided on it)",
      [s.parity_label(n, s.PARITY["F"]) for n in (225, 226, 300, 353, 354)] == ["BELOW", "IN", "IN", "IN", "ABOVE"] and [s.parity_label(n, s.PARITY["K"]) for n in (1873, 1874, 2928, 2929)] == ["BELOW", "IN", "IN", "ABOVE"]
      and s.parity_label(None, s.PARITY["F"]) is None)

# ═══ 2. gates ═══
neg = [(12, 10.0 - 0.05 * 12 + d) for d in (-0.01, 0.0, 0.01)] + [(16, 10.0 - 0.05 * 16 + d) for d in (-0.01, 0.0, 0.01)] + [(24, 10.0 - 0.05 * 24 + d) for d in (-0.01, 0.0, 0.01)]
gn = s.gate(neg, 12, 0.3)
check("a NEGATIVE slope is not passed trivially: b = -0.05 ms/slot at W1 = 12 gives E = -0.6 and, against tau 0.3, NOT-CONVERGED (two-sided |E|)", gn["label"] == "NOT-CONVERGED" and near(gn["E"], -0.6, 1e-6), gn)
flat = [(12, 10.0 + d) for d in (-0.02, 0, 0.02)] + [(16, 10.0 + d) for d in (-0.02, 0, 0.02)] + [(24, 10.0 + d) for d in (-0.02, 0, 0.02)]
check("a flat fit with a small se is CONVERGED", s.gate(flat, 12, 0.3)["label"] == "CONVERGED")
noisy = [(12, 10.0), (12, 11.5), (12, 8.5), (16, 10.2), (16, 12.0), (16, 8.0), (24, 10.1), (24, 11.9), (24, 8.2)]
check("a flat but noisy fit (|E| + 2 se > tau >= |E| - 2 se) is UNRESOLVED", s.gate(noisy, 12, 0.3)["label"] == "UNRESOLVED", s.gate(noisy, 12, 0.3))
check("a moved floor pool: pools 16 / 24 only (W 12 dropped) fit on two pools with W1 = 16; a single pool reads UNSCOREABLE (nothing is extrapolated below the floor pool)",
      s.gate([(16, 10.0 + d) for d in (-0.02, 0, 0.02)] + [(24, 10.0 + d) for d in (-0.02, 0, 0.02)], 16, 0.3)["pools"] == [16, 24]
      and s.gate([(24, 10.0), (24, 10.1), (24, 9.9)], 24, 0.3)["label"] == "UNSCOREABLE")


class FR:      # a floor-pool row stand-in for floor_pool()
    def __init__(self, ok):
        self.ok = ok


orig_valid = s.floor_valid
s.floor_valid = lambda r: (r.ok, "" if r.ok else "bad")
w12 = [FR(True), FR(True), FR(False)]
w16 = [FR(True), FR(True), FR(True)]
fp, valid = s.floor_pool({12: w12, 16: w16, 24: [FR(True)] * 3})
check("the floor pool is the smallest pool with >= 3 valid rows: W 12 has 2, so the floor moves to W 16 (reported)", fp == 16 and len(valid[12]) == 2, (fp, {k: len(v) for k, v in valid.items()}))
check("no pool with 3 valid rows: None", s.floor_pool({12: w12, 16: w12, 24: w12})[0] is None)
s.floor_valid = orig_valid
ws, fc, sc, ok = s.u_matched({"a": 30.0, "b": 0.1}, {"a": 8.0, "b": -0.01}, 0.0112, 0.112, w_lowest=12)
check("the u-matched floor: W* = u2 / kappa = 10, F_c = a + b W* = 31, S_F,c = 7.9; W* 2 slots below the lowest pool is scoreable, 7 slots below is UNSCOREABLE-FLOOR",
      near(ws, 10.0, 1e-9) and near(fc, 31.0) and near(sc, 7.9) and ok and not s.u_matched({"a": 30.0, "b": 0.1}, {"a": 8.0, "b": -0.01}, 0.0112, 0.112, w_lowest=17)[3])

# ═══ 3. CR61 ═══
class CR(s.Row):      # a Row without the t60 machinery
    def __init__(self, W, gp, rtt, tmin, p90, q, dnf=False):
        self.r = {"mbps": gp, "dnf": dnf}
        self.W, self.dnf, self.live, self.flag = W, dnf, True, None
        self.rep, self.seed = 1, "42"
        nw = 40
        self.x = {"win": {"t": [15.0 + 0.25 * i for i in range(nw)], "nk": [100] * nw, "tmin": [tmin] * nw, "p10": [tmin] * nw, "p50": [tmin + 5] * nw, "p90": [p90] * nw}, "rtt": rtt, "qshare": q}


def mk(W, gp, rtt, tmin=30.0, p90=33.0, q=0.0):
    return CR(W, gp, rtt, tmin, p90, q)


healthy_floor = [mk(12, 2.0, [31.0] * 80 + [32.0] * 30) for _ in range(3)]      # tmin 30: e_own 1.0 on 80 ticks, 2.0 on 30 -> E50 1.0, E90 2.0
x_own, e50, e90, nref = s.x_own_from(healthy_floor)
check("X_own = E90 + 2 (E90 - E50), the medians over the reference rows of each row's own percentile of e_own: E50 1.0, E90 2.0 -> X_own 4.0 (absolute)", nref == 3 and near(e50, 1.0) and near(e90, 2.0) and near(x_own, 4.0), (x_own, e50, e90))
ctl_row = mk(2048, 17.0, [230.0] * 100, tmin=229.5, p90=231.0)      # a standing queue: rtt and tmin both ~ 230 ms
others = [mk(2048, 17.0, [230.0] * 100, tmin=229.5, p90=231.0) for _ in range(3)]
s.cr61([ctl_row] + others, 88, x_own)
check("a standing queue (the control pool: rtt and the row's own tmin both ~ 230 ms) is NOT flagged: e_own cancels it", ctl_row.flag is False, ctl_row.flag)
g_r1 = mk(2048, 17.0, [33.0] * 80 + [38.0] * 20, tmin=33.0, p90=35.0)      # E90_own 5 against p90 - tmin 2: ratio 2.5 > 1.5, qshare 0
s.cr61([g_r1] + others, 88, x_own)
check("the G r1 shape (qshare 0 but E90_own / (p90 - tmin) = 2.5 > 1.5) is flagged by the ratio", g_r1.flag is True)
fl_hi = mk(12, 2.0, [30.0 + 9.0] * 100)      # e_own 9 > X_own for 100 % of ticks, goodput normal
fl_lo = mk(12, 0.5, [30.0 + 9.0] * 100)      # the same queue and goodput 0.5 against the arm median 2.0 (below 0.95 x)
pair = [fl_hi, mk(12, 2.0, [31.0] * 100), mk(12, 2.0, [31.0] * 100), fl_lo]
s.cr61(pair, 88, x_own)
check("a floor collapse is flagged only WITH its goodput cut: the same excess at normal goodput is not, at 0.25 x the arm median it is", fl_hi.flag is False and fl_lo.flag is True, (fl_hi.flag, fl_lo.flag))
qrow = mk(120, 3.0, [31.0] * 100, q=0.33)
qok = mk(120, 17.0, [31.0] * 100, q=0.33)
s.cr61([qrow] + [mk(120, 17.0, [31.0] * 100) for _ in range(3)], 88, x_own)
s.cr61([qok] + [mk(120, 17.0, [31.0] * 100) for _ in range(3)], 88, x_own)
check("at and above K1 qshare needs the goodput cut too: qshare 0.33 at the arm's normal goodput is not a collapse, at 0.18 x the arm median it is", qok.flag is False and qrow.flag is True, (qok.flag, qrow.flag))

# ═══ 4. the pilot ═══
check("pilot: u88 < 0.95 <= u128 interpolates (u 0.80 / 1.00 -> 88 + 0.15 * 40 / 0.2 = 118)", near(s.pilot_knee(0.80, 1.00), 118.0))
check("pilot: both below 0.95 with u128 > u88 extrapolates the secant (0.6 / 0.85 -> 128 + 0.10 * 40 / 0.25 = 144)", near(s.pilot_knee(0.6, 0.85), 144.0))
check("pilot: both below 0.95 and u128 <= u88 (degenerate secant) -> 128 * 0.95 / u128 (0.80 / 0.70 -> 173.7)", near(s.pilot_knee(0.80, 0.70), 128 * 0.95 / 0.70) and near(s.pilot_knee(0.80, 0.80), 128 * 0.95 / 0.80))
check("pilot: both at or above 0.95 -> 88 * 0.95 / u88 (0.97 / 1.0 -> 86.2, then clamped up to 64 only below 64)", near(s.pilot_knee(0.97, 1.0), 88 * 0.95 / 0.97))
check("pilot: the result is clamped to [64, 192] (0.2 / 0.25 extrapolates far above 192; 1.9 / 2.0 falls below 64)", s.pilot_knee(0.2, 0.25) == 192.0 and s.pilot_knee(1.9, 2.0) == 64.0)
check("pilot: the non-monotone pair u88 >= 0.95 > u128 falls to the last case (DECISION 3)", near(s.pilot_knee(1.0, 0.9), 88 * 0.95 / 1.0))

# ═══ 5. rho, the labels, FLOOR-SENSITIVE ═══
check("rho = need / S_F on a two-row fixture: F_c 10, S_F,c 4, Tmin(W95) 12 and 14 -> needs 2 and 4 -> rho 0.5 and 1.0, median 0.75 (absolute)", near(s.rho_of([12.0, 14.0], 10.0, 4.0), 0.75))
check("rho three-way: CI inside (-Delta, +Delta) AGREES; CI outside [-Delta, +Delta] DISAGREES; a CI straddling an edge UNRESOLVED",
      s.rho_label(-0.03, 0.04, 0.085) == "AGREES" and s.rho_label(0.10, 0.2, 0.085) == "DISAGREES" and s.rho_label(-0.5, -0.1, 0.085) == "DISAGREES" and s.rho_label(-0.02, 0.12, 0.085) == "UNRESOLVED"
      and s.rho_label(-0.09, 0.0, 0.085) == "UNRESOLVED")
check("D labels: SHORT iff CI > 0, COVERED iff CI < 0, else UNRESOLVED", s.d_label(0.1, 0.3) == "SHORT" and s.d_label(-0.3, -0.1) == "COVERED" and s.d_label(-0.1, 0.2) == "UNRESOLVED")
check("FLOOR-SENSITIVE iff the W->0 reading gives the OPPOSITE definite label (AGREES / DISAGREES, SHORT / COVERED); UNRESOLVED on either side is not",
      s.opposite("AGREES", "DISAGREES") and s.opposite("COVERED", "SHORT") and not s.opposite("AGREES", "UNRESOLVED") and not s.opposite("AGREES", "AGREES") and not s.opposite("SHORT", "DISAGREES"))
check("SIGMA-DISCRIMINATION = rho_2 (sigma_3 / S_F,3) / (sigma_2 / S_F,2) - rho_2: sigma_3 / S_F,3 equal to c2's ratio reads 0",
      near(s.sigma_discrimination(0.797, 1.785 * 2.0, 5.08325 * 2.0, s2=5.08325), 0.0, 1e-9) and s.sigma_discrimination(0.797, 2.0, 5.0) > 0)

# ═══ 6. c2's frozen inputs ═══
pt, d2, pt0, d20 = s.c2_draws()
lo, hi = s.ci(d2)
lo0, hi0 = s.ci(d20)
check("rho_2 = 0.797 90 % [0.780, 0.825] (s61pred.out section 2, ported draw for draw, seed 60)", round(pt, 3) == 0.797 and round(lo, 3) == 0.780 and round(hi, 3) == 0.825, (pt, lo, hi))
check("the W->0 variant 0.877 90 % [0.858, 0.908] (slopes refitted per draw)", round(pt0, 3) == 0.877 and round(lo0, 3) == 0.858 and round(hi0, 3) == 0.908, (pt0, lo0, hi0))
fz = s.c2_frozen_summary()
check("c2's floor: F 8.679, S_F 5.083 and the margin Delta = q_R F / S_F = 0.085; W95 per replicate 165.7-169.7 gives the needs 3.901-4.113",
      round(fz["F"], 3) == 8.679 and round(fz["S"], 3) == 5.083 and round(fz["delta"], 3) == 0.085 and round(min(s.C2_FROZEN["knee_tmin"]) - fz["F"], 3) == 3.901 and round(max(s.C2_FROZEN["knee_tmin"]) - fz["F"], 3) == 4.113,
      (fz, min(s.C2_FROZEN["knee_tmin"]) - fz["F"]))
check("c2's frozen constants (pinned values; their derivation from the s60 rows is the HOST block below): X_own 10.66 ms, drift references tmin 12.33 / u 0.930", s.X_OWN_C2 == 10.66 and s.DRIFT_TMIN == 12.33 and s.DRIFT_U == 0.930)

# host only: recompute the frozen inputs from the committed tgz (docs/l1-raw is export-ignored: skipped in an archive; S61_S60_DIR points at a directory holding the two tgz when the tree has none)
S60DIR = os.environ.get("S61_S60_DIR") or os.path.join(HERE, "..", "..", "docs", "l1-raw", "s60")
TGZ = [os.path.join(S60DIR, n) for n in ("rep60a-diag-ld.tgz", "rep60b-diag-ld.tgz")]
SL = os.path.join(HERE, "..", "..", "docs", "derivations", "store-law")
if os.path.exists(TGZ[0]):
    import re as _re
    import tarfile
    NAME = _re.compile(r"diag-ld/(c2-100)@def-(G)(\d+)-s(\d+)-r(\d+)-a\d+-(c\.log|drv\.out)$")
    raw = {}
    with tarfile.open(TGZ[0]) as tf:
        for mb in tf:
            g_ = NAME.search(mb.name)
            if g_ and mb.isfile():
                raw.setdefault((int(g_.group(3)), g_.group(4), int(g_.group(5))), {})[g_.group(6)] = tf.extractfile(mb).read().decode("utf-8", "replace").splitlines()
    real = []
    for (w_, sd_, rp_), dd in sorted(raw.items()):
        cli_, drv_ = dd["c.log"], dd.get("drv.out", [])
        gp_ = next((json.loads(ln).get("mean_mbps") for ln in drv_ if ln.startswith("{") and '"summary":true' in ln), None)
        real.append(s.Row({"cell": "c2-100", "arm": "G%d" % w_, "seed": sd_, "rep": rp_, "status": "LIVE", "dnf": False, "mbps": gp_, "t60": s6.row_metrics(cli_, [], drv_, None, "c2-100", "G%d" % w_, gp_),
                           "t61": s.row_extras(cli_, [], drv_, "c2-100")}))
    w16 = [r for r in real if r.W == 16]
    xo = s.x_own_from(w16)
    check("HOST real rows: X_own from the six s60 c2 W 16 rows is 10.66 (E50 4.95, E90 6.86): the frozen constant is the code's own result on the rows", len(w16) == 6 and round(xo[0], 2) == 10.66 and round(xo[1], 2) == 4.95 and round(xo[2], 2) == 6.86, xo)
    s.cr61(real, 160, 10.66)
    check("HOST real rows: CR61 over every s60 c2 G row (6 x 8 arms, frozen X_own 10.66, K1 160) flags 0 of 48 (s61pred.out section 4c)", len(real) == 48 and sum(1 for r in real if r.flag) == 0, [(r.arm, r.rep) for r in real if r.flag])
    reps = {}
    for r in real:
        reps.setdefault(r.key, {})[r.W] = r
    ctl_ = {k: v[2048].gp_() for k, v in reps.items()}
    kns = [s.knee_of(rw, ctl_[k], 160, 192, lambda r: r.native_tmin(), (160, 176, 192), 256, 2048, None) for k, rw in sorted(reps.items())]
    check("HOST real rows: knee_of over the six replicates gives W95 165.7-169.7 and Tmin(W95) equal to the frozen knee minima (the r68allow reading), every replicate scoreable at K1 160 / K3 192",
          all(k["scoreable"] for k in kns) and round(min(k["w95"] for k in kns), 1) == 165.7 and round(max(k["w95"] for k in kns), 1) == 169.7
          and all(abs(a - b) < 1e-9 for a, b in zip(sorted(k["tmin"] for k in kns), s.C2_FROZEN["knee_tmin"])), [(k["w95"], k.get("tmin")) for k in kns])
    fl_real = sorted(s.reading(r.w61(s.NK_NATIVE), None)[:2] for r in w16)
    check("HOST real rows: the native floor (S_F, F) pairs of the six W 16 rows equal the frozen pairs", fl_real == sorted(s.C2_FROZEN["floor_pairs"]))
    if os.path.exists(os.path.join(SL, "r68allow.py")):
        sys.path.insert(0, SL)
        import s61pred as sp  # noqa: E402
        ra = sp.ra
        rows_ = ra.load(TGZ)
        reps_ = ra.rep_map(rows_, ra.C2)
        kt = sorted(ra.at_knee(ra.C2, wm, ra.knee(ra.C2, wm), lambda r: r.m(0, "tmin")) for wm in reps_.values())
        g8 = {w: [tuple(sp.row_SF(r, 0, 800)[:2]) for r in sp.grows(rows_, ra.C2) if r.W == w and r.late(0)] for w in (16, 32, 48)}
        check("HOST: the frozen c2 inputs equal s61pred's recomputation (six knee tmins, the G(800) rows)", all(abs(a - b) < 1e-9 for a, b in zip(kt, s.C2_FROZEN["knee_tmin"])) and g8 == s.C2_FROZEN["g800"])
    else:
        print("skip HOST: s61pred recomputation (r68allow.py is not on this branch)")
else:
    print("skip HOST: the frozen c2 inputs vs the s60 tgz (no docs/l1-raw/s60 here and S61_S60_DIR unset: an archive)")


# ═══ 7. row_extras on a gauge log, the scorer end to end ═══
try:
    import stubrun_gen as sg
    cli = ['{"seconds":0.01,"warmup":true}'] + sg.plan60_cli([], "c3", 16, "20000000", True, False, set())
    x = s.row_extras(cli, [], [], "c3-25")
    t60 = s6.row_metrics(cli, [], [], 20_000_000, "c3-25", "G16", 3.0)
    nwin = len(x["win"]["t"])
    check("row_extras: the LATE [SLAW] windows with nk > 0 (t >= 15 s) joined with their [WSTAT] p10 / p50 / p90, the LATE [DIAG] rtt ticks, no error",
          "error" not in x and nwin > 100 and x["win"]["t"][0] >= 15.0 and x["win"]["nk"][0] == 80 and x["win"]["p50"][0] is not None and x["win"]["p10"][0] is not None and x["win"]["p90"][0] is not None
          and len(x["rtt"]) > 100 and abs(x["win"]["tmin"][0] - (33.0 + 0.32)) < 1e-9 and t60["witness"]["ok"] is True, (nwin, x["win"]["nk"][:2], x.get("error")))
    rv = s.Row({"cell": "c3-25", "arm": "G16", "seed": "42", "rep": 1, "status": "LIVE", "dnf": False, "mbps": 3.0, "t60": t60, "t61": x})
    S_, F_, n_, g_ = s.reading(rv.w61(1), 250)
    check("a Row over the gauge log: G(250) at W 16 (nk 80 per window) pools 4 windows (n 320), S_F = 1.2 (p50 = tmin + 1.2) and F = tmin", rv.live and n_ == 320 and near(S_, 1.2, 1e-9) and near(F_, 33.32, 1e-9), (S_, F_, n_, g_))
    check("e_own on that row is the tick rtt less the row's native tmin (the stub's rtt = floor + rise + 1.0 printed to 0.1 ms: |e_own| < 0.1 ms + 1)", len(s.e_own(rv)) == len(x["rtt"]) and all(abs(e - 1.0) < 0.06 for e in s.e_own(rv)), s.e_own(rv)[:3])
    bad = s.row_extras(["[SLAW] garbage", '{"warmup":true}', "[DIAG] t=zz"], [], [], "c3-25")
    check("row_extras never raises on garbled log content (empty windows, no error key needed)", isinstance(bad, dict) and bad["win"]["t"] == [])
except Exception as e:      # noqa: BLE001
    check("row_extras on a gauge log", False, f"{type(e).__name__}: {e}")


# ── synthetic ledgers with a known answer ──
def synth_row(cell, W, rep, seed, F=33.0, S=10.0, knee=86.5, rise=8.0, gp_ctl=17.3, dnf=False, rtt_extra=0.5, nk_per_slot=5, status="LIVE", sig=2.0, l=33.0):
    """One G row of the toy c3 model: tmin = F + rise (W / knee)^3 (flat floor, rise S_F * 0.8 at the knee), p50 = tmin + S, goodput min(0.19 W, 17.3), nk = 5 per slot per window."""
    tmin = F + rise * (W / knee) ** 3 if W < 1000 else 200.0
    gp = gp_ctl if W >= 1000 else min(0.19 * W, gp_ctl)
    nw = 160
    ts = [15.0 + 0.25 * i for i in range(nw)]
    nk = [min(nk_per_slot * W, 1000)] * nw
    t61 = {"v": 1, "win": {"t": ts, "nk": nk, "tmin": [tmin] * nw, "p10": [tmin - 0.0] * nw, "p50": [tmin + S] * nw, "p90": [tmin + S + 4.0] * nw}, "rtt": [tmin + rtt_extra] * 400, "qshare": 0.0, "loss": 0.02, "n_ticks": 500}
    cols = {k: [None] * nw for k in s6.FIELDS}
    cols.update({"k": list(range(nw)), "t": ts, "nk": nk, "tmin": [tmin] * nw, "tp50": [tmin + S] * nw, "sig": [sig] * nw, "l": [l] * nw, "err": [-1.0] * nw, "q": [0.1] * nw, "m": [tmin - 0.2] * nw})
    t60 = {"v": 1, "cell": cell, "arm": "G%d" % W, "kind": "G", "W": W, "late": {"0": cols}, "nwin": {"0": nw}, "boot": {"0": {"b0k": [0.0] * 8, "b0kn": [20] * 8, "b0p": 0.2, "kw": 8}}, "ml": {}, "witness": {"ok": True}}
    return {"cell": cell, "arm": "G%d" % W, "seed": seed, "rep": rep, "status": status, "dnf": dnf, "mbps": 0.0 if dnf else gp, "t60": t60, "t61": t61, "problems": []}


def synth_ledger(knee_hat=88, ctl_w=2048, reps=6, drop=lambda W, rep: False, **kw):
    a = plan61.arms(knee_hat)
    ws = list(plan61.FLOOR_W[plan61.C3]) + a["B"] + a["K"] + [a["P1"], ctl_w]
    rows = []
    for rep in range(1, reps + 1):
        for W in ws:
            if drop(W, rep):
                continue
            rows.append(synth_row(plan61.C3, W, rep, "42" if rep % 2 else "7", **kw))
    return rows


def run_score(rows, plan="61a", knee="88"):
    import tempfile
    import json
    d = tempfile.mkdtemp()
    p = os.path.join(d, "ld.log")
    with open(p, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    lines = []
    old = os.environ.get("LD_KNEE61")
    if knee:
        os.environ["LD_KNEE61"] = knee
    try:
        rc = s.score([p], plan, out=lines.append)
    finally:
        os.environ.pop("LD_KNEE61", None)
        if old:
            os.environ["LD_KNEE61"] = old
    return rc, "\n".join(lines)


rc, txt = run_score(synth_ledger())
lab = {ln.split()[1]: ln.split(None, 2)[2] for ln in txt.splitlines() if ln.startswith("LABEL61 ")}
check("synthetic c3 (tmin = 33 + 8 (W / 86.5)^3, S_F 10, so need at the knee ~ 8 = 0.8 S_F): the scorer runs to a VERDICT61 and rho reads ~0.8 -> AGREES with c2's frozen 0.797 (CI inside +-0.085)",
      rc == 0 and "VERDICT61" in txt and lab.get("RHO") in ("AGREES", "AGREES (S or sigma-scaled)") or lab.get("RHO-RAW") == "AGREES", (lab, txt[-2500:]))
import re  # noqa: E402
mr = re.search(r"^RHO c3 = ([\d.]+)", txt, re.M)
check("... with rho_3 within 0.1 of 0.8 (the toy model's knee window minimum), a floor pool W 12 and 6 scoreable replicates", mr is not None and abs(float(mr.group(1)) - 0.8) < 0.1 and "FLOOR POOL W12" in txt and "SCOREABLE replicates 6" in txt, txt[-2000:])
check("... the floor reading divides by the SESSION median control goodput and the gates' tau is 0.05 S_F,c (S_F,c 10 -> 0.50), printed after the floor reading", "against the session median control 17.30 Mbit/s" in txt and "tau 0.50 = 0.05 S_F,c" in txt, txt[:2500])
check("... the D table prints every candidate at Bulk / Auto / Realtime and the landing table prints W*(g, delta); the price table has u, tmin, tp50 per arm",
      all(f"D {q}" in txt for q in ("Bulk", "Auto", "Realtime")) and "LANDING c3" in txt and "PRICE c3-25" in txt and "LABEL61 D-S-Bulk" in txt, txt[-1500:])
rc, txt = run_score(synth_ledger(rise=4.0))
lab = {ln.split()[1]: ln.split(None, 2)[2] for ln in txt.splitlines() if ln.startswith("LABEL61 ")}
check("synthetic c3 with half the rise (need 4 = 0.4 S_F): rho ~ 0.4 against c2's 0.797 -> DISAGREES", lab.get("RHO-RAW") == "DISAGREES" and lab.get("RHO") in ("DISAGREES", "FLOOR-SENSITIVE"), (lab, txt[-1800:]))
rc, txt = run_score(synth_ledger(drop=lambda W, rep: W == 12 and rep > 2))
check("a moved floor pool: W 12 has only 2 rows (replicates 3-6 missing): the floor moves to W 16, reported (section 4.2), and the scorer still scores",
      "FLOOR POOL W16 (moved from W12" in txt and "LABEL61 RHO " in txt, txt[:1800])
rc, txt = run_score(synth_ledger(drop=lambda W, rep: W in (12, 16, 24) and rep > 2))
check("no floor pool with 3 valid rows: NEEDS-MORE-floor-rows, nothing scored", "LABEL61 RHO NEEDS-MORE-floor-rows" in txt, txt[-600:])
rows = synth_ledger()
for r in rows:
    if r["arm"] == "G2048" and r["rep"] <= 3:
        r["dnf"], r["mbps"] = True, 0.0
rc, txt = run_score(rows)
check("CONTROL-MOVED: 3 of 6 control rows DNF (fewer than 4 unflagged): nothing at c3 is scored", "CONTROL-MOVED" in txt and "LABEL61 CONTROL-c3 CONTROL-MOVED" in txt and "LABEL61 RHO NEEDS-MORE-control-pool" in txt, txt[-800:])
rc, txt = run_score(synth_ledger(nk_per_slot=20))
check("UNSCOREABLE-N: windows hold more samples than c2's reference allows (20 per slot: floor groups of 480, knee groups of 3 000, both ABOVE the parity band) -> rho and D are not scored (LABEL61 RHO UNSCOREABLE-N), the PARITY line prints the cause",
      re.search(r"^LABEL61 RHO UNSCOREABLE-N", txt, re.M) is not None and re.search(r"PARITY floor n \d+ \(ABOVE", txt) is not None and "RHO c3 =" not in txt, txt[-900:])
rc, txt = run_score(synth_ledger(reps=4))
check("fewer than 5 scoreable replicates is UNSCOREABLE (4 replicates: the control still has 4 unflagged rows)", "LABEL61 RHO UNSCOREABLE" in txt, txt[-500:])

os.environ["LD_NOGO"] = "floor"
rc, txt = run_score(synth_ledger())
del os.environ["LD_NOGO"]
check("a floor NO-GO (LD_NOGO=floor, the rehearsal's decision): the floor is W 16 for the whole of 61a, reported as the registered rule, and the scorer still scores", "FLOOR POOL W16 (registered floor NO-GO" in txt and "LABEL61 RHO " in txt and "W12: G(250)" not in txt, txt[:1500])


# the rehearsal's decisions (section 6) on synthetic rows R1-R6; every NO-GO branch executes
def rh(dnf=(), loss=None, gp_scale=None):
    rows = []
    for i, (W_, sd_, rp_) in enumerate(((2048, "42", 1), (12, "42", 1), (12, "7", 2), (88, "42", 1), (128, "42", 1), (512, "42", 1))):
        r = synth_row(plan61.C3, W_, rp_, sd_, dnf=(W_, sd_) in dnf)
        if loss and W_ in loss:
            r["t61"]["loss"] = loss[W_]
        if gp_scale and W_ in gp_scale:
            r["mbps"] *= gp_scale[W_]
        rows.append(s.Row(r, i))
    out_ = []
    dec, w = s.rehearsal(rows, out_.append)
    return dec, w, "\n".join(out_)


dec, w, txt = rh()
check("rehearsal, all healthy: floor GO, control GO (R1 17.30 >= 0.98 x max(R4, R5, R6)), PILOT61 names W-hat from u88 = 0.966 / u128 = 1.0 (the synthetic knee 86.5) and the arms",
      dec == {"floor": True, "control": True} and w is not None and 85.0 < w < 88.0 and "PILOT61 W=" in txt and "GO-NOGO control GO" in txt and "GO-NOGO floor GO" in txt, txt)
dec, w, txt = rh(gp_scale={2048: 0.9})
check("rehearsal: R1 below 0.98 x max(R4, R5, R6) is a control NO-GO and the fallback W 512 (R6 passes the loss test) is named (LD_CTL61=512); the pilot reads against R6", dec["control"] is False and "set LD_CTL61=512" in txt and "(control W512)" in txt and w is not None, txt)
dec, w, txt = rh(loss={2048: 0.09, 512: 0.09})
check("rehearsal: R1 and R6 both fail the loss test: NOGO-CONTROL, PILOT61 none (61a does not launch: ABORT-PILOT)", "NOGO-CONTROL" in txt and w is None and "PILOT61 none" in txt, txt)
dec, w, txt = rh(dnf={(88, "42")})
check("rehearsal: a DNF pilot row leaves no W-hat (PILOT61 none)", w is None and "PILOT61 none" in txt, txt)
dec, w, txt = rh(dnf={(12, "42"), (12, "7")})
check("rehearsal: both floor rows DNF: floor NO-GO, the consequence (W 16, LD_NOGO=floor) is printed", dec["floor"] is False and "GO-NOGO floor NO-GO" in txt and "LD_NOGO=floor" in txt, txt)

# 61b: c2's add-on
def synth_c2(drift=0.0, reps=6):
    rows = []
    for rep in range(1, reps + 1):
        for W in plan61.C2_ARMS:
            gp = 90.3 if W == 2048 else min(0.93 * 90.3 * W / 160.0, 0.93 * 90.3 + 0) if W < 160 else (0.93 * 90.3 if W == 160 else 90.3)
            r = synth_row(plan61.C2, W, rep, "42" if rep % 2 else "7", F=8.68, S=5.08, knee=168.0, rise=3.65 + drift, gp_ctl=90.3)
            r["mbps"] = gp
            rows.append(r)
    return rows


rc, txt = run_score(synth_c2(), "61b", None)
check("61b: CR61 with the frozen X_own, the drift reading (tmin / u at the overlap arm W 160 against s60's 12.33 / 0.930), the price and landing tables, a VERDICT61", rc == 0 and "CR61 c2: X_own = 10.66" in txt and "DRIFT c2:" in txt and "LANDING c2" in txt and "VERDICT61" in txt, txt[-1500:])
rc, txt = run_score(synth_c2(drift=2.0), "61b", None)
check("61b: a session whose W 160 minimum drifted by > 0.3 ms reads DRIFT (the c2 rows are their own session, s60's arms are not merged)", "LABEL61 DRIFT-c2 DRIFT" in txt and "PRICE c2 merged" not in txt, txt[-900:])

print("FAILED %d" % fails if fails else "ALL OK")
print(f"\ntest_s61: {fails} fail(s)")
sys.exit(1 if fails else 0)
