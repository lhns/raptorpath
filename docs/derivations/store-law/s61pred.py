"""s61 (the c3 sweep) pre-registration: every prediction computed first from the s60 rows (s61-registration.md).

Usage:  python -I -X utf8 s61pred.py <rep60a-diag-ld.tgz> <rep60b-diag-ld.tgz>  > s61pred.out

Reads the committed tgz in place with r68allow.py's loader (same LATE set: t >= 15 s at c2 / c8, >= 5 s at c1s, nk >= 10), and the [DIAG] ticks for the
collapse study. Sections:
  0. window sample counts nk per arm and leg, and the per-slot goodput (the c3 sizing basis)
  1. the grouping rule G(n*): consecutive LATE windows pooled until their sample count reaches n*; tmin of a group = the min of its windows' tmin (exact: the
     minimum of a union is the minimum of the minima), p50 of a group = the nk-weighted median of its windows' p50 (an approximation of the pooled median).
     S_F(n*) and F(n*) per floor pool, and the floor gate on each, at c2, c8 leg 0, c8 leg 1 (the c3-shaped leg) and c1s
  2. c2's need / S_F (the comparison value rho_2) native and under G(280), with the registered bootstrap; and the knee tmin's own sensitivity to grouping
  3. c3 predictions from c8 leg 1 (S_F(n*), F, sigma) and per candidate D under S's sentence
  4. the collapse study: qcwnd-below-BDP share, the store occupancy aw, and the excess over the row's own LATE tmin, on every s60 row
  4c. CR61 (s61-registration.md section 4.1) applied to every s60 G row, with X_own per cell
  4b. B1's field err on the floor pools (the margins at the floor pools)
  5. the c2 price table from s60's arms (goodput, tmin, tp50) and the chord predictions at the new arms 96 / 128 / 144
"""
import math
import os
import random
import re
import statistics as st
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import r68allow as ra  # noqa: E402

C2, C8, C1S = ra.C2, ra.C8, ra.C1S
BDP_B = {C2: [125000], C8: [125000, 100000], C1S: [250000]}          # place38_parse.bdp_bytes: link x 2 x one-way delay
med = ra.med
QB, QR = ra.QB, ra.QR
P = print
NL = chr(10)


def parse_log(lines, row):
    """r68allow.parse_log plus the [SLAW] fields `aw` (the store's occupancy W/n) and `err` (B1's field)."""
    slaw, wst = {}, {}
    for ln in lines:
        if '"warmup":true' in ln:
            slaw, wst = {}, {}
            continue
        if ln.startswith("[SLAW]"):
            d = dict(ra.TOK.findall(ln))
            slaw[(int(d["k"]), int(d["p"]))] = d
        elif ln.startswith("[WSTAT]"):
            d = dict(ra.TOK.findall(ln))
            wst[(int(d["k"]), int(d["p"]))] = d
    for (k, p), s in sorted(slaw.items()):
        if s.get("nk", "0") == "0":
            continue
        ws = wst.get((k, p), {})
        n = ra.num
        w = {"k": k, "t": n(s, "t"), "nk": n(s, "nk"), "tmin": n(s, "tmin"), "tp50": n(s, "tp50"), "m": n(s, "m"), "l": n(s, "l"), "sig": n(s, "sig"),
             "sigw": n(s, "sigw"), "b0p": n(s, "b0p"), "q": n(s, "q"), "aw": n(s, "aw"), "err": n(s, "err"),
             "p10": n(ws, "p10"), "p50": n(ws, "p50"), "p90": n(ws, "p90")}
        if s.get("b0k", "-") != "-":
            w["b0k"] = [float(x) for x in s["b0k"].split(",")]
            w["b0kn"] = [float(x) for x in s["b0kn"].split(",")]
        row.win.setdefault(p, []).append(w)


ra.parse_log = parse_log


def wmed(pairs):
    """nk-weighted median of [(value, weight)]."""
    pairs = sorted((v, w) for v, w in pairs if v is not None and w)
    if not pairs:
        return None
    tot, acc = sum(w for _, w in pairs), 0.0
    for v, w in pairs:
        acc += w
        if acc >= tot / 2:
            return v


def groups(wins, nstar):
    """G(n*): consecutive windows pooled until sum nk >= n*; the last incomplete group is dropped. nstar None = native (one window each)."""
    if nstar is None:
        return [(w["tmin"], w["p50"], w["nk"]) for w in wins if w["tmin"] is not None and w["p50"] is not None]
    out, cur = [], []
    for w in wins:
        if w["tmin"] is None or w["p50"] is None:
            continue
        cur.append(w)
        if sum(x["nk"] for x in cur) >= nstar:
            out.append((min(x["tmin"] for x in cur), wmed([(x["p50"], x["nk"]) for x in cur]), sum(x["nk"] for x in cur)))
            cur = []
    return out


def late_all(r, leg):
    """LATE windows with nk >= 1 (G pools small windows; the native reading keeps s60's nk >= 10)."""
    return [w for w in r.win.get(leg, []) if w["t"] >= ra.LATE_T[r.cell] and w["nk"] >= 1]


def row_SF(r, leg, nstar):
    g = groups(r.late(leg) if nstar is None else late_all(r, leg), nstar)
    if not g:
        return None, None, None, 0
    return med([p - t for t, p, _ in g]), med([t for t, _, _ in g]), med([n for _, _, n in g]), len(g)


def ols(xs, ys):
    n = len(xs)
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    b = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx
    res = sum((y - my - b * (x - mx)) ** 2 for x, y in zip(xs, ys))
    return b, math.sqrt(res / (n - 2) / sxx)


def grows(rows, cell, kind="G"):
    return [r for r in rows.values() if r.cell == cell and r.kind == kind]


# ───────────────────────────── [DIAG] ticks for the collapse study ─────────────────────────────
DT = re.compile(r"^\[DIAG\] t=([\d.]+)s ")
DSRC = re.compile(r" src=(\d+)sym/s")
DTOP = re.compile(r" rtt=([\d.]+)ms ")
DLEG = re.compile(r"\bp(\d+):infl=.*?rtt=(\d+)/wrtt=\d+/rtp\d+ms rtp_us=(\d+)")
QCW = re.compile(r"\bqcwnd=(\d+)")
SPLIT = re.compile(r"(?= p\d+:infl=)")
NAME = re.compile(r"diag-ld/(c\w+-\d+)@def-([GC])(\d+)-s(\d+)-r(\d+)-a\d+-c\.log$")


def load_ticks(paths):
    """-> {(cell, kind, W, seed, rep): [(t, src, rtt_top, {leg: (rtt_leg, rtp_us, qcwnd)})]} after the last warm-up marker."""
    out = {}
    for path in paths:
        with tarfile.open(path, "r:gz") as tf:
            for mb in tf:
                g = NAME.search(mb.name)
                if not g or not mb.isfile():
                    continue
                key = (g.group(1), g.group(2), int(g.group(3)), g.group(4), int(g.group(5)))
                ticks = []
                for ln in tf.extractfile(mb).read().decode("utf-8", "replace").splitlines():
                    if '"warmup":true' in ln:
                        ticks = []
                        continue
                    m = DT.match(ln)
                    if not m:
                        continue
                    s, top = DSRC.search(ln), DTOP.search(ln)
                    legs = {}
                    for part in SPLIT.split(ln)[1:]:
                        lg, q = DLEG.search(part), QCW.search(part)
                        if lg:
                            legs[int(lg.group(1))] = (float(lg.group(2)), int(lg.group(3)), int(q.group(1)) if q else None)
                    ticks.append((float(m.group(1)), int(s.group(1)) if s else 0, float(top.group(1)) if top else None, legs))
                out[key] = ticks
    return out


def pq(v, p):
    v = sorted(v)
    return v[int(p * (len(v) - 1))] if v else None


def main(argv):
    paths = argv[1:3]
    rows = ra.load(paths)
    P("s61 predictions (s61pred.py): the s60 rows read in place from " + " and ".join(a.replace("\\", "/").split("/")[-1] for a in paths))
    cal = {cl: ra.calibrate(rows, *cl) for cl in ra.CAL_W}

    # ── 0. sample counts and per-slot goodput ──
    P("\n== 0. Window sample count nk (median over rows of the row's LATE median, nk >= 10 windows) and goodput per slot (Mbit/s per W, median over rows) ==")
    for cell, legs in ((C2, (0,)), (C8, (0, 1)), (C1S, (0,))):
        arms = sorted({r.W for r in grows(rows, cell)})
        for leg in legs:
            cells = []
            for W in arms:
                rs = [r for r in grows(rows, cell) if r.W == W and r.late(leg)]
                nk = med([med([w["nk"] for w in r.late(leg)]) for r in rs])
                gpw = med([r.gp / W for r in rs if r.gp])
                cells.append(f"W{W}: nk {ra.fx(nk, 0)} gp/W {ra.fx(gpw, 3)}")
            P(f"  {cell} leg{leg}: " + " | ".join(cells))

    # ── 1. the grouping rule ──
    NSTARS = (None, 250, 800, 1120)
    P("\n== 1. S_F(n*) and F(n*) per floor pool under G(n*) (median over valid-or-all G rows of the row median over groups; groups per row in brackets) ==")
    P("   n* = None is s60's native window reading. Floor gate per n*: OLS of the per-row value against W over the pools, E = b*W1 (W1 = the smallest pool listed)")
    SETS = ((C2, 0, (16, 32, 48)), (C8, 0, (32, 64, 96)), (C8, 1, (32, 64, 96, 256)), (C8, 1, (256, 320)), (C1S, 0, (64, 128, 192)))
    for cell, leg, pools in SETS:
        P(f"  {cell} leg{leg} pools {pools}:")
        for ns in NSTARS:
            per, xs, ysS, ysF = [], [], [], []
            for W in pools:
                rs = [r for r in grows(rows, cell) if r.W == W and r.late(leg)]
                vals = [row_SF(r, leg, ns) for r in rs]
                vals = [(v, r.W) for v, r in zip(vals, rs) if v[0] is not None]
                for (s, fF, n, k), w in vals:
                    xs.append(w)
                    ysS.append(s)
                    ysF.append(fF)
                per.append(f"W{W}: S_F {ra.fx(med([v[0][0] for v in vals]), 2)} F {ra.fx(med([v[0][1] for v in vals]), 2)} n {ra.fx(med([v[0][2] for v in vals]), 0)} [{ra.fx(med([v[0][3] for v in vals]), 0)}]")
            gate = ""
            if len(set(xs)) >= 2 and len(xs) >= 3:
                bS, seS = ols(xs, ysS)
                bF, seF = ols(xs, ysF)
                W1 = pools[0]
                gate = f" || S_F b {bS:+.4f}+-{seS:.4f} E {bS * W1:+.2f}+-{seS * W1:.2f} | F b {bF:+.4f}+-{seF:.4f} E {bF * W1:+.2f}+-{seF * W1:.2f}"
            P(f"    n*={str(ns):>5}: " + " | ".join(per) + gate)

    # ── 2. rho_2 = need / S_F at c2: native (s61's comparison value), on s61's validity set (every W16 G row: no LATCHED rule) and on s60's (calibrate: LATCHED dropped) ──
    P("\n== 2. c2: rho = need / S_F (need = Tmin(W95) - F; F, S_F on the W16 floor rows, native windows), per replicate and pooled, 90 % bootstrap (2000, seed 60, replicates x floor rows) ==")
    reps = ra.rep_map(rows, C2)
    knee_t = sorted(ra.at_knee(C2, wm, ra.knee(C2, wm), lambda r: r.m(0, "tmin")) for wm in reps.values())
    w95s = sorted(ra.knee(C2, wm) for wm in reps.values())
    sigk = med([med([w["sig"] for W in set(ra.bracket(C2, wm, ra.knee(C2, wm))) for w in wm[W].late(0)]) for wm in reps.values()])

    def rho_boot(items, fl, seed=60):
        rnd = random.Random(seed)
        res = []
        for _ in range(2000):
            it = [items[rnd.randrange(len(items))] for _ in items]
            f_ = [fl[rnd.randrange(len(fl))] for _ in fl]
            Sb, Fb = med([x[0] for x in f_]), med([x[1] for x in f_])
            res.append(med([(t - Fb) / Sb for t in it]))
        res.sort()
        return res[100], res[1900]

    RHO2 = {}
    for nm, frs in (("s60 valid set (5 rows, LATCHED dropped)", cal[(C2, 0)]["rows"]), ("s61 validity (all 6 W16 G rows)", [r for r in grows(rows, C2) if r.W == 16 and r.late(0)])):
        fl = sorted([row_SF(r, 0, None)[:2] for r in frs])
        S_c, F_c = med([x[0] for x in fl]), med([x[1] for x in fl])
        rho = med([(t - F_c) / S_c for t in knee_t])
        lo, hi = rho_boot(knee_t, fl)
        RHO2[nm] = (rho, lo, hi, S_c, F_c)
        P(f"  {nm}: F {F_c:.3f} S_F {S_c:.3f} | need per replicate " + " ".join(f"{t - F_c:.3f}" for t in knee_t)
          + f" | rho_2 {rho:.3f}  90 % [{lo:.3f}, {hi:.3f}]  (half-width {(hi - lo) / 2:.3f})")
    P(f"  c2 W95 per replicate {' '.join(f'{w:.1f}' for w in w95s)}; sig at the knee bracket {sigk:.3f}")
    rho2, lo2, hi2, S2, F2 = RHO2["s61 validity (all 6 W16 G rows)"]
    dq = QR * F2 / S2
    P(f"  the equivalence margin Delta = q_R * F_c / S_F at c2 = {QR} * {F2:.3f} / {S2:.3f} = {dq:.3f}")
    P("  knee tmin's sensitivity to the sample count (c2 knee arms; row median of the minima of k consecutive LATE windows; realised n per group):")
    for W in ra.KNEE_W[C2]:
        rs = [r for r in grows(rows, C2) if r.W == W]
        line = []
        for k in (1, 2, 4, 8):
            v = []
            for r in rs:
                ws = r.late(0)
                v.append(med([min(x["tmin"] for x in ws[i:i + k]) for i in range(0, len(ws) - k + 1, k)]))
            line.append(f"k={k}: {med(v):.3f} (n {med([med([x['nk'] for x in r.late(0)]) * k for r in rs]):.0f})")
        P(f"    W{W}: " + " | ".join(line))
    P("  the floor slope at matched n (every pool grouped to n* >= the largest native nk among W 16 / 32 / 48 = 786): b and E = b*16, and the W -> 0 variant of rho_2:")
    xs, yF, yS = [], [], []
    for W in (16, 32, 48):
        for r in [r for r in grows(rows, C2) if r.W == W and r.late(0)]:
            s_, f_, _, _ = row_SF(r, 0, 800)
            xs.append(W)
            yF.append(f_)
            yS.append(s_)
    bF, seF = ols(xs, yF)
    bS, seS = ols(xs, yS)
    EF, ES = bF * 16, bS * 16
    rho20 = med([(t - (F2 - EF)) / (S2 - ES) for t in knee_t])
    fl6 = sorted([row_SF(r, 0, None)[:2] for r in grows(rows, C2) if r.W == 16 and r.late(0)])
    bypool = {W: [(W,) + row_SF(r, 0, 800)[:2] for r in grows(rows, C2) if r.W == W and r.late(0)] for W in (16, 32, 48)}
    rnd = random.Random(60)
    res0 = []
    for _ in range(2000):
        it = [knee_t[rnd.randrange(len(knee_t))] for _ in knee_t]
        f_ = [fl6[rnd.randrange(len(fl6))] for _ in fl6]
        pts = [v[rnd.randrange(len(v))] for v in bypool.values() for _ in v]
        bFb, _ = ols([x[0] for x in pts], [x[2] for x in pts])
        bSb, _ = ols([x[0] for x in pts], [x[1] for x in pts])
        Sb, Fb = med([x[0] for x in f_]) - bSb * 16, med([x[1] for x in f_]) - bFb * 16
        res0.append(med([(t - Fb) / Sb for t in it]))
    res0.sort()
    P(f"    G(800): F b {bF:+.4f} +- {seF:.4f} -> E_F {EF:+.2f} +- {seF * 16:.2f} ms | S_F b {bS:+.4f} +- {seS:.4f} -> E_S {ES:+.2f} +- {seS * 16:.2f} ms"
      f" | rho_2 at (F - E_F, S_F - E_S) = {rho20:.3f}, 90 % [{res0[100]:.3f}, {res0[1900]:.3f}] (slopes refitted per draw; applied to the native F, S_F: an approximation, labelled)")

    # ── 3. c3 predictions from c8 leg 1 (the c3-shaped leg; never at its own knee) under s61's floor rule G(250) ──
    P("\n== 3. c3 predictions from c8 leg 1 (20 Mbit/s, 20 ms, 5 ms jitter both ways; starved below W 256): S_F and F under G(250) (realised n near c2's native floor 282) ==")
    l1 = cal[(C8, 1)]
    S3s = []
    for W in (32, 64, 96, 256):
        rs = [r for r in grows(rows, C8) if r.W == W and r.late(1)]
        v = [row_SF(r, 1, 250) for r in rs]
        v = [x for x in v if x[0] is not None]
        S3s += [x[0] for x in v]
        sd = st.stdev([x[0] for x in v]) if len(v) > 1 else None
        P(f"  W{W}: S_F {med([x[0] for x in v]):.3f} (row sd {ra.fx(sd, 3)}) F {med([x[1] for x in v]):.3f} realised n/group {med([x[2] for x in v]):.0f} groups/row {med([x[3] for x in v]):.0f}")
    S3 = med(S3s)
    sig1 = med([r.m(1, "sig") for r in grows(rows, C8) if r.W == 256 and r.late(1)])
    F3 = med([row_SF(r, 1, 250)[1] for r in grows(rows, C8) if r.W in (32, 64, 96) and r.late(1)])
    P(f"  pooled S_F(G250) {S3:.3f} ms; F(G250) at W 32-96 {F3:.3f}; leg-1 sig at W256 {sig1:.3f}; l_c {l1['l']:.3f}")
    need2 = rho2 * S2
    rivals = (("S: need = rho_2 * S_F", rho2 * S3),
              ("constant: need = need_2 (4.05 ms, the A(c) family's c_B reading)", need2),
              ("sigma-scaled: need = (need_2 / sig_2) * sig_3", need2 / sigk * sig1),
              ("loop-scaled: need = (need_2 / F_2) * F_3", need2 / F2 * F3))
    P(f"  rho_3 under each rival sentence (S_F,3 = {S3:.2f}):")
    for nm, nd in rivals:
        P(f"    {nm:<62} need_3 {nd:6.2f} ms  rho_3 {nd / S3:.3f}  rho_3 - rho_2 {nd / S3 - rho2:+.3f}")
    P("  D per candidate at c3 under S's sentence (need = rho_2 * S_F,3), Bulk:")
    need3 = rho2 * S3
    A = {"A0 sig+q*l": sig1 + QB * l1["l"], "A0f sig+q*F": sig1 + QB * F3, "Ac sig+2.26": sig1 + 2.261,
         "S = decided form at g=1: S_F+q*F": S3 + QB * F3, "wq sig+0.26*F": sig1 + 0.260 * F3}
    P("    " + " | ".join(f"{k}: A {a:.2f} D {need3 - a:+.2f}" for k, a in A.items()))
    P(f"  g at which the decided form lands at the c3 knee at Realtime: g_knee = rho - q_R*F/S_F = {rho2:.3f} - {QR * F3 / S3:.3f} = {rho2 - QR * F3 / S3:.3f} (c2: {rho2 - dq:.3f})")
    P(f"  S_F,3 sweep: c8 leg 1's {S3:.2f} exceeds the two-way uniform +-5 ms cap of 10 ms; c2 reads 5.08 against its 6 ms cap (0.847), so a single c3 leg may read ~8.5. sigma_3 scaled with S_F (sigma_3 = {sig1:.2f} * S/{S3:.2f}) and fixed:")
    P("    S_F,3 | need_S | D: A0 A0f Ac S(g=1) wq | rho_3 constant / sigma-fixed / sigma-scaled / loop | D[S] under sigma-fixed | g_knee | Delta_3 = q_R F/S | tau = 0.05 S")
    for S in (8.5, 9.5, 10.5, 11.5, 12.5):
        nd = rho2 * S
        for sgl, sg in (("sigma fixed", sig1), ("sigma scaled", sig1 * S / S3)):
            A = (sg + QB * l1["l"], sg + QB * F3, sg + 2.261, S + QB * F3, sg + 0.260 * F3)
            nsf = need2 / sigk * sig1
            nss = need2 / sigk * sg
            P(f"    {S:5.1f} {sgl:<12} | {nd:5.2f} | " + " ".join(f"{nd - a:+5.2f}" for a in A)
              + f" | {need2 / S:.3f} / {nsf / S:.3f} / {nss / S:.3f} / {need2 / F2 * F3 / S:.3f} | {nsf - (S + QB * F3):+5.2f} | {rho2 - QR * F3 / S:.3f} | {QR * F3 / S:.3f} | {0.05 * S:.2f}")

    # ── 4. the collapse study ──
    P("\n== 4. Collapse study per row: qcwnd-below-BDP share (DIAG, t >= 1 s, leg 0), goodput, and the excess of the EWMA rtt over (a) the row's min rtp_us (s60's rule)"
      " and (b) the row's own median LATE tmin (leg 0 [SLAW]); LATE ticks with src > 0; E50 / E90 and the share above X_own ==")
    ticks = load_ticks(paths)
    for cell, arms in ((C1S, (2048, 64, 128, 192)), (C2, (16, 48, 160, 256, 2048)), (C8, (32, 96, 384, 512, 1280))):
        for W in arms:
            P(f"  {cell} W{W}:")
            for key in sorted(k for k in ticks if k[0] == cell and k[2] == W):
                r = rows.get(key)
                tk = ticks[key]
                t0 = ra.LATE_T[cell]
                bdp = BDP_B[cell][0]
                qs = [t[3].get(0, (None, None, None))[2] for t in tk if t[0] >= 1.0]
                qs = [q for q in qs if q is not None]
                qshare = sum(q < bdp for q in qs) / len(qs) if qs else None
                one = len(BDP_B[cell]) == 1
                rtts = [(t[2] if one else t[3].get(0, (None,))[0]) for t in tk if t[0] >= t0 and t[1] > 0]
                rtts = [x for x in rtts if x is not None]
                rtp = min([t[3][0][1] for t in tk if 0 in t[3] and t[3][0][1] > 0] or [0]) / 1000.0
                tmin = r.m(0, "tmin") if r else None
                p90 = r.m(0, "p90") if r else None
                ea = [x - rtp for x in rtts]
                eb = [x - tmin for x in rtts] if tmin is not None else []
                aw = r.m(0, "aw") if r else None
                awlo = (sum(w["aw"] < 0.9 for w in r.late(0) if w.get("aw") is not None) / max(1, len(r.late(0)))) if r and r.late(0) else None
                ratio = (pq(eb, .9) / (p90 - tmin)) if eb and p90 is not None and tmin is not None and p90 > tmin else None
                P(f"    {key[1]} s{key[3]} r{key[4]}: gp {ra.fx(r.gp if r else None, 1)} qshare {ra.fx(qshare, 3)} | aw med {ra.fx(aw, 3)} share<0.9 {ra.fx(awlo, 2)}"
                  f" | vs min rtp {rtp:.2f}: E50 {ra.fx(pq(ea, .5), 1)} E90 {ra.fx(pq(ea, .9), 1)}"
                  f" | vs own tmin {ra.fx(tmin, 2)}: E50 {ra.fx(pq(eb, .5), 1)} E90 {ra.fx(pq(eb, .9), 1)} | own p90 - tmin {ra.fx(None if p90 is None or tmin is None else p90 - tmin, 1)}"
                  f" | E90_own/(p90-tmin) {ra.fx(ratio, 2)}")

    # ── 4c. CR61, the registered collapse rule, on every s60 G row ──
    P(NL + "== 4c. CR61 on every s60 G row. Below the knee bracket (floor and below-knee arms): COLLAPSED iff > 10 % of LATE ticks have e_own = rtt - (row's median LATE tmin) > X_own AND goodput < 0.95 x the arm's median goodput;"
      " X_own = E90 + 2 (E90 - E50), each the median over the reference floor rows of the row's own percentile. At and above the knee bracket: COLLAPSED iff qshare > 0.10 or E90_own / (p90 - tmin)_own > 1.5 ==")

    def eown(key, leg):
        r, tk = rows.get(key), ticks.get(key) or []
        if r is None or not r.late(leg):
            return None, None, None
        tmin, p90 = r.m(leg, "tmin"), r.m(leg, "p90")
        one = len(BDP_B[r.cell]) == 1
        x = [(t[2] if one else (t[3].get(leg) or (None,))[0]) for t in tk if t[0] >= ra.LATE_T[r.cell] and t[1] > 0]
        return [v - tmin for v in x if v is not None], tmin, p90

    def qsh(key):
        tk = ticks.get(key) or []
        qs = [t[3].get(0, (None, None, None))[2] for t in tk if t[0] >= 1.0]
        qs = [q for q in qs if q is not None]
        return sum(q < BDP_B[key[0]][0] for q in qs) / len(qs) if qs else None

    REF = {(C2, 0): 16, (C8, 0): 32, (C8, 1): 256, (C1S, 0): 64}
    KNEE_LO = {C2: 160, C8: 256, C1S: 2048}
    XO = {}
    for (cell, leg), W in REF.items():
        per = []
        for key in ticks:
            if key[0] == cell and key[1] == "G" and key[2] == W:
                e, _, _ = eown(key, leg)
                if e:
                    per.append((pq(e, .5), pq(e, .9)))
        e50, e90 = med([x[0] for x in per]), med([x[1] for x in per])
        XO[(cell, leg)] = e90 + 2 * (e90 - e50)
        P(f"  X_own {cell} leg{leg} (ref W{W}, {len(per)} rows; medians over rows of each row's E50 / E90): E50 {e50:.2f} E90 {e90:.2f} -> X_own {XO[(cell, leg)]:.2f} ms")
    for cell, legs in ((C2, (0,)), (C8, (0, 1)), (C1S, (0,))):
        for leg in legs:
            arms = sorted({k[2] for k in ticks if k[0] == cell and k[1] == "G"})
            for W in arms:
                flags, shares = [], []
                gmed = med([rows[k].gp for k in ticks if k[0] == cell and k[1] == "G" and k[2] == W and k in rows and rows[k].gp])
                for key in sorted(k for k in ticks if k[0] == cell and k[1] == "G" and k[2] == W):
                    e, tmin, p90 = eown(key, leg)
                    if not e:
                        continue
                    if W < KNEE_LO[cell]:
                        sh = sum(v > XO[(cell, leg)] for v in e) / len(e)
                        shares.append(sh)
                        flags.append(sh > 0.10 and (rows[key].gp or 0) < 0.95 * gmed)
                    else:
                        q = qsh(key) if leg == 0 else None
                        ratio = pq(e, .9) / (p90 - tmin) if p90 and p90 > tmin else None
                        shares.append(ratio)
                        flags.append(bool((q is not None and q > 0.10) or (ratio is not None and ratio > 1.5)))
                kind = "share>X_own" if W < KNEE_LO[cell] else "E90/(p90-tmin)"
                P(f"  {cell} leg{leg} W{W}: COLLAPSED {sum(flags)}/{len(flags)} | {kind} " + " ".join(ra.fx(x, 2) for x in shares))

    P(NL + "== 4d. qshare on c8 leg 1 (the c3-shaped leg, BDP 100 000 B): share of [DIAG] lines (t >= 1 s) with p1 qcwnd < 100 000, per row ==")
    for W in sorted({k[2] for k in ticks if k[0] == C8 and k[1] == "G"}):
        v = []
        for key in sorted(k for k in ticks if k[0] == C8 and k[1] == "G" and k[2] == W):
            qs = [t[3].get(1, (None, None, None))[2] for t in ticks[key] if t[0] >= 1.0]
            qs = [q for q in qs if q is not None]
            v.append(sum(q < BDP_B[C8][1] for q in qs) / len(qs) if qs else None)
        P(f"  c8 leg1 W{W}: " + " ".join(ra.fx(x, 3) for x in v))

    # ── 4b. B1's field on the floor pools: per-row median LATE err (B1 fires iff > 0); the margin is -err ──
    P("\n== 4b. B1 on s60's floor pools (every G row, no validity filter): per-row median LATE err (ms), leg by leg; B1 fires iff > 0 ==")
    for cell, leg, pools in ((C2, 0, (16, 32, 48)), (C8, 0, (32, 64, 96)), (C8, 1, (96, 256, 320)), (C1S, 0, (64, 128, 192))):
        for W in pools:
            v = sorted(r.m(leg, "err") for r in grows(rows, cell) if r.W == W and r.late(leg) and r.m(leg, "err") is not None)
            if v:
                P(f"  {cell} leg{leg} W{W}: err " + " ".join(f"{x:+.3f}" for x in v) + f" | median margin {-med(v):+.3f}, smallest margin {-v[-1]:+.3f}, fires on {sum(x > 0 for x in v)}/{len(v)}")

    # ── 5. c2 price table ──
    P("\n== 5. c2 price table from s60 (median over replicates): goodput / control, tmin, tp50 (the loop the store law reads), and the chord at the new arms ==")
    pts = {}
    for W in sorted({r.W for r in grows(rows, C2)}):
        rs = [r for r in grows(rows, C2) if r.W == W]
        ctl = {(r.rep, r.seed): r.gp for r in grows(rows, C2) if r.W == 2048}
        u = med([r.gp / ctl[(r.rep, r.seed)] for r in rs if r.gp and ctl.get((r.rep, r.seed))])
        tm, tp = med([r.m(0, "tmin") for r in rs]), med([r.m(0, "tp50") for r in rs])
        pts[W] = (u, tm, tp)
        P(f"  W{W:<5} u {u:.3f}  tmin {tm:.2f}  tp50 {tp:.2f}")
    for W in (96, 128, 144):
        a, b = 48, 160
        f_ = (W - a) / (b - a)
        ch = [pts[a][i] + f_ * (pts[b][i] - pts[a][i]) for i in range(3)]
        P(f"  W{W} chord 48-160: u {ch[0]:.3f} tmin {ch[1]:.2f} tp50 {ch[2]:.2f}   (tmin is convex in W, so the chord over-reads tmin and tp50 below the knee)")


if __name__ == "__main__":
    main(sys.argv)
