"""s62 (the c3 jitter sweep): the registered scorer, prototyped and run end to end on real rows before registration (s62-registration.md §7).

Usage (from docs/):  python -I -X utf8 derivations/store-law/s62pred.py l1-raw/s61/rep61c-diag-ld.tgz l1-raw/s61/rep61a-diag-ld.tgz \
                         l1-raw/s60/rep60b-diag-ld.tgz  > derivations/store-law/s62pred.out

The harness scorer (tools/l1/s62_score.py, the Sonnet worker's port) must reproduce this file's numbers on the same inputs. Stand-ins, because
s62's own cells do not exist yet:
  * J = 5 cell         : rep61c (the scored 61a re-run; K 100 / 108 / 120, P1 152, control W512)
  * J = 0 / 3 cell      : rep61a (s61's first c3 session; K 104 / 112 / 124, P1 136, control W2048), to execute the scored path on real rows
  * the jitter-free shape: s60's c1s rows (rep60b; netem jitter 0), for every rule that divides by a spread or a tail
Each stand-in is loaded on its own (row keys collide across sessions). Sections:
  1. CR62 (the collapse rule, J = 0-safe form) on every row of every stand-in, with the raw s61 form beside it
  2. the scored path per cell: control, floor arm, the D0 gate, knees, need with its bootstrap
  3. Delta_0 and the label (need_0 against zero, FLOOR-SENSITIVE from the W12 / W24 intercept), with the gate as measured and forced
  4. the label function on every branch (synthetic inputs, marked)
  5. the rehearsal go/no-go on real rows (a knee arm and the control)
  6. the pilot's proportional rule against the 61c curve; the J = 0 knee prediction
  7. reported only: the rise-against-spread line over the available cells
"""
import json
import os
import random
import re
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import r68allow as ra  # noqa: E402
import s61pred as sp  # noqa: E402

med, interp, cross_up = ra.med, ra.interp, ra.cross_up
C3, C1S = "c3-25", "c1s-400"
ra.LATE_T[C3] = 15.0
Q_R = 0.05
NF, NK = 250, 2100
GATE_D0 = 0.5                         # §3: the floor's count drop must be < 0.5 ms
X_MIN_FRAC = Q_R                      # §4: J = 0-safe floor of every spread-divided rule: q_R * F (the Realtime margin in ms)
RATIO_MAX, SHARE_MAX, GP_CUT = 1.5, 0.10, 0.95
N_BOOT = 2000
P = print
QD = re.compile(r"QDISC (\w+): .*?delay (\d+)ms(?:\s+(\d+)ms)?.*?rate (\d+)([GM])bit.*?Sent (\d+) bytes (\d+) pkt")
TRUTH = re.compile(r"\[TRUTH\] leg=0 .*?loss=([\d.]+)")
DRV = re.compile(r"diag-ld/(c\w+-\d+)@def-([GC])(\d+)-s(\d+)-r(\d+)-a\d+-drv\.out$")


def ols(xs, ys):
    b, se = sp.ols(xs, ys)
    return b, sum(ys) / len(ys) - b * sum(xs) / len(xs), se


def q(v, p):
    v = sorted(x for x in v if x is not None)
    return v[int(p * (len(v) - 1))] if v else None


def load(path):
    rows = ra.load([path])
    ticks = sp.load_ticks([path])
    with tarfile.open(path, "r:gz") as tf:
        for mb in tf:
            g = DRV.search(mb.name)
            if not g or not mb.isfile():
                continue
            r = rows.get((g.group(1), g.group(2), int(g.group(3)), g.group(4), int(g.group(5))))
            if r is None:
                continue
            r.loss, r.jitter = None, None
            for ln in tf.extractfile(mb).read().decode("utf-8", "replace").splitlines():
                m = TRUTH.search(ln)
                if m:
                    r.loss = float(m.group(1))
                m = QD.search(ln)
                if m and m.group(1) == "cli0":
                    r.jitter = float(m.group(3) or 0)
    for k, r in rows.items():
        r.ticks = ticks.get(k, [])
    return rows


class Cell:
    def __init__(self, name, rows, cell, floor_w, karms, p1, ctl, bdp_b):
        self.name, self.cell, self.floor_w, self.karms, self.p1, self.ctl, self.bdp = name, cell, floor_w, karms, p1, ctl, bdp_b
        self.rows = [r for r in rows.values() if r.cell == cell and r.kind == "G"]
        self.flag = {}


def native_tmin(r):
    return med([w["tmin"] for w in r.late(0)])


def native_p90(r):
    return med([w["p90"] for w in r.late(0) if w.get("p90") is not None])


def e_own(r):
    t0 = native_tmin(r)
    lt = ra.LATE_T[r.cell]
    return [top - t0 for t, src, top, legs in r.ticks if t >= lt and src > 0 and top is not None] if t0 is not None else []


def qshare(r, bdp):
    v = [legs.get(0, (None, None, None))[2] for t, src, top, legs in r.ticks if t >= 1.0]
    v = [x for x in v if x is not None]
    return sum(1 for x in v if x < bdp) / len(v) if v else 0.0


def cr62(c, verbose=True):
    """CR62: s61's CR61 with every spread-divided threshold floored at X_min = q_R * F_floor (J = 0-safe), raw s61 form printed beside it."""
    fl = [r for r in c.rows if r.W == c.floor_w]
    e50 = med([q(e_own(r), 0.5) for r in fl])
    e90 = med([q(e_own(r), 0.9) for r in fl])
    x_own = e90 + 2 * (e90 - e50)
    f_fl = med([sp.row_SF(r, 0, NF)[1] for r in fl])
    x_min = X_MIN_FRAC * f_fl
    x_use = max(x_own, x_min)
    k1 = c.karms[0]
    arm_gp = {}
    for r in c.rows:
        arm_gp.setdefault(r.W, []).append(r.gp or 0.0)
    out, binds = [], 0
    for r in sorted(c.rows, key=lambda r: (r.W, r.rep)):
        gp_low = (r.gp or 0.0) < GP_CUT * med(arm_gp[r.W])
        eo = e_own(r)
        if r.W < k1:
            sh_raw = sum(1 for x in eo if x > x_own) / len(eo) if eo else 0.0
            sh = sum(1 for x in eo if x > x_use) / len(eo) if eo else 0.0
            raw_flag, flag = sh_raw > SHARE_MAX and gp_low, sh > SHARE_MAX and gp_low
            txt = f"share>X_own {sh_raw:.2f} / >X_use {sh:.2f}"
        else:
            e90r = q(eo, 0.9)
            den_raw = (native_p90(r) or 0) - (native_tmin(r) or 0)
            den = max(den_raw, x_min)
            binds += den_raw < x_min
            qs = qshare(r, c.bdp)
            rr = e90r / den_raw if e90r is not None and den_raw > 0 else None
            ru = e90r / den if e90r is not None else None
            raw_flag = (qs > SHARE_MAX and gp_low) or (rr is not None and rr > RATIO_MAX)
            flag = (qs > SHARE_MAX and gp_low) or (ru is not None and ru > RATIO_MAX)
            txt = f"qshare {qs:.2f} ratio raw {rr if rr is None else round(rr, 2)} / floored {ru if ru is None else round(ru, 2)} (den {den_raw:.2f}{' -> ' + format(den, '.2f') if den != den_raw else ''})"
        c.flag[id(r)] = flag
        out.append((r, raw_flag, flag, txt))
    if verbose:
        P(f"  {c.name}: X_own = {x_own:.2f} ms (E50 {e50:.2f}, E90 {e90:.2f}, floor W{c.floor_w}); X_min = q_R * F_floor = {x_min:.2f} ms -> X_use {x_use:.2f}"
          f"{' (X_min BINDS)' if x_min > x_own else ''}; ratio denominators floored on {binds} of {sum(1 for r in c.rows if r.W >= k1)} rows")
        by = {}
        for r, rf, f, _ in out:
            by.setdefault(r.W, [0, 0, 0])
            by[r.W][0] += 1
            by[r.W][1] += rf
            by[r.W][2] += f
        P("    flagged per arm (raw s61 form / CR62): " + " | ".join(f"W{W} {a[1]}/{a[0]} -> {a[2]}/{a[0]}" for W, a in sorted(by.items())))
        for r, rf, f, txt in out:
            if rf or f or r.W in (c.floor_w, c.ctl):
                P(f"    G{r.W} s{r.seed} r{r.rep}: {txt}{'  RAW-FLAG' if rf else ''}{'  CR62-FLAG' if f else ''}")
    return x_use


FLOOR2 = 24                           # the second floor arm (round 2): need_0 at W12 and at the intercept of the W12 / W24 floor line


def scored(c, rnd, verbose=True, ctl_by_goodput=False):
    """The scored path. -> dict. ctl_by_goodput lifts CR62 on the control arm only (s61a Result's post-hoc reading; exercise only)."""
    live = [r for r in c.rows if not c.flag.get(id(r)) or (ctl_by_goodput and r.W == c.ctl)]
    ctl = [r for r in live if r.W == c.ctl and r.gp]
    res = {"name": c.name}
    if len(ctl) < 4:
        res["label"] = "CONTROL-MOVED"
        if verbose:
            P(f"  {c.name}: control W{c.ctl} unflagged {len(ctl)} < 4 -> LABEL62 CONTROL-{c.name.split()[0]} CONTROL-MOVED: nothing scored")
        return res
    ctl_m = med([r.gp for r in ctl])
    fa = {W: [r for r in live if r.W == W and len(sp.groups(sp.late_all(r, 0), NF)) >= 6] for W in (c.floor_w, FLOOR2)}
    fl, fl2 = fa[c.floor_w], fa[FLOOR2]
    if len(fl) < 3 or len(fl2) < 3:
        res["label"] = "UNSCOREABLE (floor: fewer than 3 valid rows on an arm)"
        return res

    def fread(rs):
        return {"nat": med([native_tmin(r) for r in rs]), "g250": med([sp.row_SF(r, 0, NF)[1] for r in rs]), "g2100": med([sp.row_SF(r, 0, NK)[1] for r in rs])}

    f1, f2 = fread(fl), fread(fl2)
    d0 = max(f1["nat"], f1["g250"]) - f1["g2100"]
    d0b = max(f2["nat"], f2["g250"]) - f2["g2100"]
    reps = {}
    for r in live:
        reps.setdefault((r.rep, r.seed), {})[r.W] = r
    knees = []
    for key, wm in sorted(reps.items()):
        c_ = wm.get(c.ctl)
        gref = c_.gp if c_ and c_.gp else ctl_m
        pts = [(W, r.gp / gref) for W, r in wm.items() if W != c.ctl and r.gp]
        w95 = cross_up(pts, 0.95)
        arms = [W for W in tuple(c.karms) + ((c.p1,) if c.p1 else ()) if W in wm]
        if w95 is None or len(arms) < 2 or not (c.karms[0] <= w95 <= c.karms[-1]):
            knees.append((key, w95, None))
            continue
        knees.append((key, w95, interp([(W, native_tmin(wm[W])) for W in arms], w95)))
    ok = [t for k, w, t in knees if t is not None]
    a250 = [sp.row_SF(r, 0, NF)[1] for r in fl]
    b250 = [sp.row_SF(r, 0, NF)[1] for r in fl2]

    def intercept(xa, xb):
        xs = [c.floor_w] * len(xa) + [FLOOR2] * len(xb)
        b, a, _ = ols(xs, xa + xb)
        return a

    F0, Fi = med(a250), intercept(a250, b250)
    need = med([t - F0 for t in ok]) if ok else None
    need_i = med([t - Fi for t in ok]) if ok else None
    dr, di = [], []
    for _ in range(N_BOOT):
        xa = [a250[rnd.randrange(len(a250))] for _ in a250]
        xb = [b250[rnd.randrange(len(b250))] for _ in b250]
        kb = [ok[rnd.randrange(len(ok))] for _ in ok] if ok else []
        if kb:
            dr.append(med([t - med(xa) for t in kb]))
            di.append(med([t - intercept(xa, xb) for t in kb]))
    ci = (q(dr, 0.05), q(dr, 0.95)) if dr else (None, None)
    cii = (q(di, 0.05), q(di, 0.95)) if di else (None, None)
    S_nat = med([med([w["tp50"] - w["tmin"] for w in r.late(0)]) for r in fl])
    res.update(ctl_m=ctl_m, F0=F0, Fi=Fi, f1=f1, f2=f2, d0=d0, d0b=d0b, need=need, ci=ci, need_i=need_i, cii=cii, n_ok=len(ok), knees=knees, S_nat=S_nat,
               n3={k: (med([t - f1[k] for t in ok]) if ok else None) for k in ("nat", "g250", "g2100")},
               delta0=0.5 * Q_R * F0, jitter=med([r.jitter for r in c.rows if r.jitter is not None]))
    if verbose:
        P(f"  {c.name} (netem jitter +-{res['jitter']:g} ms): control W{c.ctl} unflagged {len(ctl)}, median {ctl_m:.2f} Mbit/s")
        P(f"    floor W{c.floor_w} (valid {len(fl)}): native {f1['nat']:.3f} | G250 {f1['g250']:.3f} | G2100 {f1['g2100']:.3f} -> D0 {d0:.3f} ms, gate {'PASS' if d0 < GATE_D0 else 'FAIL'}"
          f" | floor W{FLOOR2} (valid {len(fl2)}): native {f2['nat']:.3f} | G250 {f2['g250']:.3f} | G2100 {f2['g2100']:.3f} -> D0 {d0b:.3f} (reported)")
        P("    knees: " + " | ".join(f"s{k[1]}r{k[0]} W95 {'-' if w is None else format(w, '.1f')} {'Tmin ' + format(t, '.3f') if t is not None else 'UNSCOREABLE (outside [K1, K3])'}" for k, w, t in knees))
        if need is not None:
            P(f"    need_0 (native knee - F(G250) at W{c.floor_w}) {need:.3f} ms, 90 % [{ci[0]:.3f}, {ci[1]:.3f}]; at the intercept (F {Fi:.3f}) {need_i:.3f} [{cii[0]:.3f}, {cii[1]:.3f}];"
              f" Delta_0 = 1/2 q_R F0 = {res['delta0']:.3f} ms; {len(ok)} scoreable replicates")
            P(f"    reported: need at the W{c.floor_w} floor read native {res['n3']['nat']:.3f} | G250 {res['n3']['g250']:.3f} | G2100 {res['n3']['g2100']:.3f}; native S_F {S_nat:.3f}")
    return res


def label(ci, delta0, d0, n_ok):
    """Round 2: need_0 against zero only."""
    lo, hi = ci
    if d0 is None or d0 >= GATE_D0:
        return "UNSCOREABLE (gate: D0 >= 0.5 ms)"
    if n_ok < 5:
        return "UNSCOREABLE (fewer than 5 replicates with W95 in [K1, K3])"
    if hi < delta0:
        return "VANISHES"
    if lo > delta0:
        return "REMAINS"
    return "UNRESOLVED"


def final(ci, cii, delta0, d0, n_ok):
    a, b = label(ci, delta0, d0, n_ok), label(cii, delta0, d0, n_ok)
    if {a, b} == {"VANISHES", "REMAINS"}:
        return f"FLOOR-SENSITIVE (W12 {a}, intercept {b})"
    return a


def main(argv):
    rows5, rowsA, rows1 = load(argv[1]), load(argv[2]), load(argv[3])
    J5 = Cell("J5 stand-in (rep61c)", rows5, C3, 12, (100, 108, 120), 152, 512, 100000)
    J0 = Cell("J0 stand-in (rep61a)", rowsA, C3, 12, (104, 112, 124), 136, 2048, 100000)
    CS = Cell("jitter-free shape (s60 c1s)", rows1, C1S, 64, (2048,), None, 2048, 250000)
    rnd = random.Random(62)
    P("s62 scorer prototype (s62pred.py, round 2), end to end on real rows; inputs: " + ", ".join(a.replace("\\", "/").split("/")[-1] for a in argv[1:4]))

    P("\n== 1. CR62 on every row: s61's CR61 with each spread-divided threshold floored at X_min = q_R * F_floor ==")
    for c in (J5, J0, CS):
        cr62(c)

    P("\n== 2. The scored path per cell (floor arms W12 and W24) ==")
    r5 = scored(J5, rnd)
    r0 = scored(J0, rnd)
    P("  the J0 stand-in again with CR62 lifted on its W2048 control only (s61a Result's post-hoc reading), to execute the rest of the path on real rows:")
    r0 = scored(J0, rnd, ctl_by_goodput=True)
    fl = [r for r in CS.rows if r.W == 64 and not CS.flag.get(id(r))]
    d0c = med([sp.row_SF(r, 0, NF)[1] for r in fl]) - med([sp.row_SF(r, 0, NK)[1] for r in fl])
    nk = med([med([w["nk"] for w in r.late(0)]) for r in fl])
    P(f"  {CS.name} floor W64: valid {len(fl)}, D0 {d0c:.3f} ms (native windows hold ~{nk:.0f} samples: G(250) and G(2100) are one window, so the gate is"
      " trivially met and says nothing about c3 at J = 0)")

    P("\n== 3. The label (need_0 against zero; FLOOR-SENSITIVE from the intercept reading) ==")
    for r_ in (r5, r0):
        P(f"  {r_['name']}: need_0 {r_['need']:.3f} [{r_['ci'][0]:.3f}, {r_['ci'][1]:.3f}], intercept {r_['need_i']:.3f} [{r_['cii'][0]:.3f}, {r_['cii'][1]:.3f}],"
          f" Delta_0 {r_['delta0']:.3f}, D0 {r_['d0']:.3f} -> LABEL62 NEED0 {final(r_['ci'], r_['cii'], r_['delta0'], r_['d0'], r_['n_ok'])}"
          f"  | gate forced open (exercise only): {final(r_['ci'], r_['cii'], r_['delta0'], 0.0, r_['n_ok'])}")

    P("\n== 4. The label function on every branch (SYNTHETIC inputs; Delta_0 1.05) ==")
    for nm, ci, cii, d0, n in (("vanishes", (-0.3, 0.4), (-0.2, 0.6), 0.1, 6), ("remains", (3.0, 4.0), (3.5, 4.6), 0.1, 6),
                               ("straddle", (0.6, 1.6), (0.7, 1.8), 0.1, 6), ("floor-sensitive", (-0.2, 0.8), (1.3, 2.4), 0.1, 6),
                               ("gate", (3.0, 4.0), (3.5, 4.6), 0.7, 6), ("replicates", (3.0, 4.0), (3.5, 4.6), 0.1, 4)):
        P(f"  {nm:<16} CI {ci} intercept {cii} D0 {d0} n {n} -> {final(ci, cii, 1.05, d0, n)}")

    P("\n== 5. Rehearsal go/no-go on real rows: the gate's pre-read on a floor pair, a knee arm and the control ==")
    for c in (J5, J0):
        pair = [r for r in c.rows if r.W == c.floor_w][:2]
        d0p = max(med([native_tmin(r) for r in pair]), med([sp.row_SF(r, 0, NF)[1] for r in pair])) - med([sp.row_SF(r, 0, NK)[1] for r in pair])
        P(f"  {c.name}: the gate's pre-read on two W{c.floor_w} rows: D0 {d0p:.3f} ms -> {'GO' if d0p < GATE_D0 else 'NO-GO-GATE'} (the cell's own 0.5 ms)")
    for c, arms in ((J5, (120, 512)), (J0, (124, 2048)), (CS, (192, 2048))):
        fl_f = med([sp.row_SF(x, 0, NF)[1] for x in c.rows if x.W == c.floor_w])
        for W in arms:
            for r in [r for r in c.rows if r.W == W][:2]:
                eo = e_own(r)
                den_raw = (native_p90(r) or 0) - (native_tmin(r) or 0)
                den = max(den_raw, X_MIN_FRAC * fl_f)
                e90 = q(eo, 0.9)
                P(f"  {c.name} G{W} s{r.seed} r{r.rep}: gp {r.gp:.2f}, loss truth {r.loss if r.loss is None else round(r.loss, 4)}, E90_own {e90 if e90 is None else round(e90, 2)},"
                  f" p90 - tmin {den_raw:.2f}, ratio raw {round(e90 / den_raw, 2) if e90 and den_raw > 0 else None} floored {round(e90 / den, 2) if e90 else None},"
                  f" absolute threshold 1.5 x X_min = {1.5 * X_MIN_FRAC * fl_f:.2f} ms, qshare {qshare(r, c.bdp):.2f}, LATE windows {len(r.late(0))}")

    P("\n== 6. The pilot (proportional rule from the lower probe, W512 control; K = r4(1.00 / 1.10 / 1.20 x W-hat), P1 r8(1.5 x W-hat)) on the 61c curve ==")
    ctl5 = r5["ctl_m"]
    for W in (80, 88):
        u = med([r.gp / ctl5 for r in J5.rows if r.W == W])
        w_ = min(W * 0.95 / u, 128)
        ks = [4 * round(m * w_ / 4) for m in (1.00, 1.10, 1.20)]
        ws = [w for _, w, _ in r5["knees"]]
        P(f"  61c W{W}: u {u:.3f} -> W-hat {w_:.1f} (bias W-hat/W95 {w_ / max(ws):.3f}-{w_ / min(ws):.3f}); K {ks[0]} / {ks[1]} / {ks[2]} against W95 {min(ws):.1f}-{max(ws):.1f}:"
          f" {sum(1 for w in ws if ks[0] <= w <= ks[2])}/{len(ws)} inside")

    P("\n== 7. Reported only: need at J = 5 at three floor counts, and the points for the rise-against-spread line ==")
    for r_ in (r5, r0):
        P(f"  {r_['name']}: need at the W12 floor read native {r_['n3']['nat']:.3f} | G250 {r_['n3']['g250']:.3f} | G2100 {r_['n3']['g2100']:.3f} ms; native S_F {r_['S_nat']:.3f}")


if __name__ == "__main__":
    main(sys.argv)
