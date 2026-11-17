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
  3. the J = 5 drift rule, Delta_0, and the label (the stand-in J = 0 cell against the J = 5 cell), with the gate as measured and forced
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
DRIFT_T, DRIFT_U, DRIFT_TMIN108, DRIFT_U108 = 0.3, 0.02, 45.96, 0.947
N_BOOT = 2000
P = print
QD = re.compile(r"QDISC (\w+): .*?delay (\d+)ms(?:\s+(\d+)ms)?.*?rate (\d+)([GM])bit.*?Sent (\d+) bytes (\d+) pkt")
TRUTH = re.compile(r"\[TRUTH\] leg=0 .*?loss=([\d.]+)")
DRV = re.compile(r"diag-ld/(c\w+-\d+)@def-([GC])(\d+)-s(\d+)-r(\d+)-a\d+-drv\.out$")


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
    fl = [r for r in live if r.W == c.floor_w and len(sp.groups(sp.late_all(r, 0), NF)) >= 6]
    if len(fl) < 3:
        res["label"] = "UNSCOREABLE-FLOOR"
        return res
    F250 = [sp.row_SF(r, 0, NF)[1] for r in fl]
    F2100 = [sp.row_SF(r, 0, NK)[1] for r in fl]
    Fnat = [native_tmin(r) for r in fl]
    d0 = max(med(Fnat), med(F250)) - med(F2100)          # the gate covers every count from native to 2 100
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
    ok = [(k, w, t) for k, w, t in knees if t is not None]
    F0 = med(F250)
    need = med([t - F0 for _, _, t in ok]) if ok else None
    need_law = med([t - med(Fnat) for _, _, t in ok]) if ok else None      # the law's pair: native knee minus native floor (the J = 5 comparator)
    draws = []
    for _ in range(N_BOOT):
        fb = [F250[rnd.randrange(len(F250))] for _ in F250]
        kb = [ok[rnd.randrange(len(ok))][2] for _ in ok] if ok else []
        if kb:
            draws.append(med([t - med(fb) for t in kb]))
    lo, hi = (q(draws, 0.05), q(draws, 0.95)) if draws else (None, None)
    S_nat = med([med([w["tp50"] - w["tmin"] for w in r.late(0)]) for r in fl])
    res.update(ctl_m=ctl_m, n_floor=len(fl), F0=F0, Fnat=med(Fnat), need_law=need_law, d0=d0, need=need, ci=(lo, hi), n_ok=len(ok), knees=knees, S_nat=S_nat,
               delta0=0.5 * Q_R * F0, jitter=med([r.jitter for r in c.rows if r.jitter is not None]))
    if verbose:
        P(f"  {c.name} (netem jitter +-{res['jitter']:g} ms): control W{c.ctl} unflagged {len(ctl)}, median {ctl_m:.2f} Mbit/s; floor arm W{c.floor_w}"
          f" valid {len(fl)}: F native {med(Fnat):.3f}, F0 = F(G250) {F0:.3f}, F(G2100) {med(F2100):.3f}, D0 = max(native, G250) - G2100 = {d0:.3f} ms -> gate {'PASS' if d0 < GATE_D0 else 'FAIL'} (< {GATE_D0})")
        P("    knees: " + " | ".join(f"s{k[1]}r{k[0]} W95 {'-' if w is None else format(w, '.1f')} {'Tmin ' + format(t, '.3f') if t is not None else 'UNSCOREABLE (outside [K1, K3])'}" for k, w, t in knees))
        if need is not None:
            P(f"    need_0-form (native knee - F(G250)) = {need:.3f} ms, 90 % [{lo:.3f}, {hi:.3f}] over {len(ok)} scoreable replicates;"
              f" need_law (native knee - native floor) = {need_law:.3f}; native S_F {S_nat:.3f}; Delta_0 = 1/2 q_R F0 = {res['delta0']:.3f} ms")
    return res


def label(need_ci, need5, delta0, d0, n_ok):
    """§5 of the registration: the one scored label."""
    lo, hi = need_ci
    if d0 is None or d0 >= GATE_D0:
        return "UNSCOREABLE (floor count drop D0 >= 0.5 ms)"
    if n_ok < 5:
        return "UNSCOREABLE (fewer than 5 replicates with W95 in [K1, K3])"
    if -delta0 < lo and hi < delta0:
        return "SPREAD-CARRIED"
    if need5 - delta0 < lo and hi < need5 + delta0:
        return "CONSTANT"
    if delta0 <= lo and hi <= need5 - delta0:
        return "INTERMEDIATE"
    return "UNRESOLVED"


def main(argv):
    rows5, rowsA, rows1 = load(argv[1]), load(argv[2]), load(argv[3])
    J5 = Cell("J5 stand-in (rep61c)", rows5, C3, 12, (100, 108, 120), 152, 512, 100000)
    J0 = Cell("J0 stand-in (rep61a)", rowsA, C3, 12, (104, 112, 124), 136, 2048, 100000)
    CS = Cell("jitter-free shape (s60 c1s)", rows1, C1S, 64, (2048,), None, 2048, 250000)
    rnd = random.Random(62)
    P("s62 scorer prototype (s62pred.py), end to end on real rows; inputs: " + ", ".join(a.replace("\\", "/").split("/")[-1] for a in argv[1:4]))

    P("\n== 1. CR62 on every row: s61's CR61 with each spread-divided threshold floored at X_min = q_R * F_floor ==")
    for c in (J5, J0, CS):
        cr62(c)

    P("\n== 2. The scored path per cell ==")
    r5 = scored(J5, rnd)
    r0 = scored(J0, rnd)
    P("  the J0 stand-in again with CR62 lifted on its W2048 control only (s61a Result's post-hoc reading), to execute the rest of the path on real rows:")
    r0 = scored(J0, rnd, ctl_by_goodput=True)
    P("  (the jitter-free shape has no knee bracket in s60; its floor and gate are read for the J = 0 rule check)")
    fl = [r for r in CS.rows if r.W == 64 and not CS.flag.get(id(r))]
    d0c = med([sp.row_SF(r, 0, NF)[1] for r in fl]) - med([sp.row_SF(r, 0, NK)[1] for r in fl])
    nk = med([med([w["nk"] for w in r.late(0)]) for r in fl])
    P(f"  {CS.name} floor W64: valid {len(fl)}, F(G250) {med([sp.row_SF(r, 0, NF)[1] for r in fl]):.3f}, D0 {d0c:.3f} ms (native windows hold ~{nk:.0f} samples,"
      " so G(250) and G(2100) are both one window: the gate is trivially met here and says nothing about c3 at J = 0)")

    P("\n== 3. Drift rule, Delta_0 and the label ==")
    a108 = [r for r in J5.rows if r.W == 108 and not J5.flag.get(id(r))]
    t108, u108 = med([native_tmin(r) for r in a108]), med([r.gp / r5["ctl_m"] for r in a108])
    drift = abs(t108 - DRIFT_TMIN108) <= DRIFT_T and abs(u108 - DRIFT_U108) <= DRIFT_U
    P(f"  J5 drift rule on the J5 stand-in against 61c's constants: tmin108 {t108:.3f} (|d| {abs(t108 - DRIFT_TMIN108):.3f} <= {DRIFT_T}), u108 {u108:.3f}"
      f" (|d| {abs(u108 - DRIFT_U108):.3f} <= {DRIFT_U}) -> {'DRIFT-OK' if drift else 'DRIFT'} (it must read DRIFT-OK on its own reference rows)")
    lab = label(r0["ci"], r5["need_law"], r0["delta0"], r0["d0"], r0["n_ok"])
    P(f"  J0 stand-in: need_0 {r0['need']:.3f} [{r0['ci'][0]:.3f}, {r0['ci'][1]:.3f}], need_5 (law's pair, J5 cell) {r5['need_law']:.3f}, Delta_0 {r0['delta0']:.3f}, D0 {r0['d0']:.3f}"
      f" -> LABEL62 NEED0 {lab}")
    lab_f = label(r0["ci"], r5["need_law"], r0["delta0"], 0.0, r0["n_ok"])
    P(f"  the same with the gate forced open (exercise only; not a reading): LABEL62 NEED0 {lab_f}")
    P(f"  (with the gate failing, need_0 carries the floor's count drop: here need_0 - need_law(J0 stand-in) = {r0['need'] - r0['need_law']:.3f} ms, which is why the gate exists)")
    P(f"  band check: 2 Delta_0 = {2 * r0['delta0']:.2f} < need_5 = {r5['need_law']:.2f}: {'disjoint' if 2 * r0['delta0'] < r5['need_law'] else 'OVERLAP'}")

    P("\n== 4. The label function on every branch (SYNTHETIC inputs; need_5 4.0, Delta_0 1.05) ==")
    for nm, ci, d0, n in (("spread", (-0.3, 0.4), 0.1, 6), ("constant", (3.6, 4.3), 0.1, 6), ("intermediate", (1.4, 2.0), 0.1, 6),
                          ("straddle", (0.8, 1.6), 0.1, 6), ("gate", (3.6, 4.3), 0.7, 6), ("replicates", (3.6, 4.3), 0.1, 4)):
        P(f"  {nm:<12} CI {ci} D0 {d0} n {n} -> {label(ci, 4.0, 1.05, d0, n)}")

    P("\n== 5. Rehearsal go/no-go on real rows: a knee arm and the control, every rule that can void the cell ==")
    for c, arms in ((J5, (120, 512)), (J0, (124, 2048)), (CS, (192, 2048))):
        for W in arms:
            rs = [r for r in c.rows if r.W == W]
            for r in rs[:2]:
                eo = e_own(r)
                den_raw = (native_p90(r) or 0) - (native_tmin(r) or 0)
                fl_f = med([sp.row_SF(x, 0, NF)[1] for x in c.rows if x.W == c.floor_w])
                den = max(den_raw, X_MIN_FRAC * fl_f)
                e90 = q(eo, 0.9)
                P(f"  {c.name} G{W} s{r.seed} r{r.rep}: gp {r.gp:.2f}, loss truth {r.loss if r.loss is None else round(r.loss, 4)},"
                  f" E90_own {e90 if e90 is None else round(e90, 2)}, ratio raw {round(e90 / den_raw, 2) if e90 and den_raw > 0 else None} floored {round(e90 / den, 2) if e90 else None},"
                  f" qshare {qshare(r, c.bdp):.2f}, LATE windows {len(r.late(0))}")

    P("\n== 6. The pilot (proportional rule, W512 control) against the 61c curve; the J = 0 knee position ==")
    ctl5 = r5["ctl_m"]
    for W in (80, 88, 100):
        u = med([r.gp / ctl5 for r in J5.rows if r.W == W])
        what = min(W * 0.95 / u, 128)
        P(f"  61c W{W}: u {u:.3f} -> W-hat {what:.1f}; knee bracket r4(0.95 / 1.05 / 1.14 x W-hat) = "
          f"{4 * round(0.95 * what / 4)} / {4 * round(1.05 * what / 4)} / {4 * round(1.14 * what / 4)} against W95 {min(w for _, w, _ in r5['knees']):.1f}-{max(w for _, w, _ in r5['knees']):.1f}")
    fl5 = [r for r in J5.rows if r.W == 108]
    tp_k = med([med([w["tp50"] for w in r.late(0)]) for r in fl5])
    P(f"  the J = 0 knee, predicted from the loop: W95(J0) ~ W95(J5) x loop(J0)/loop(J5) = 108.7 x (40.4-43 + rise ~3) / {tp_k:.1f} ="
      f" {108.7 * 43.4 / tp_k:.0f}-{108.7 * 46 / tp_k:.0f}; the pilot probes (W88 / W128) bracket it")

    P("\n== 7. Reported only: the rise-against-spread points available (law's pair: native minimum need over native S_F) ==")
    for r_ in (r5, r0):
        P(f"  {r_['name']}: need_law {r_['need_law']:.3f} ms at native S_F {r_['S_nat']:.3f} ms")
    P("  (with three in-session points J 0 / 3 / 5, s62 prints a + b * S_F by OLS and its residual; two parameters on three points: not scored)")


if __name__ == "__main__":
    main(sys.argv)
