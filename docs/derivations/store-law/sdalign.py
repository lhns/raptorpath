"""Sampler design (sampler-design.md): the knee's window minimum against retransmits, the brake estimators, and the floor's location statistics.

Usage (from docs/):  python -I -X utf8 derivations/store-law/sdalign.py l1-raw/s62/rep62r0-diag.tgz l1-raw/s61/rep61c-diag-ld.tgz \
                         l1-raw/s61/rep61a-diag-ld.tgz l1-raw/s60/rep60a-diag-ld.tgz | tr -d '\r' > derivations/store-law/sdalign.out

Post hoc, on existing rows; nothing here is registered. Leg 0 only; LATE windows as in crfollow.py (t >= 15 s, nk >= 10).
Window i of [SLAW] / [WSTAT] is paired with the i-th [DIAG] tick after the warm-up marker (crfollow.py's pairing); the
pairing is checked by time and the mismatches are printed.

Sections:
  A. Retransmit alignment per arm (Q2): does a retransmit in the same 250 ms tick go with a lower window minimum, at
     the floor, near the knee and at the controls? Threshold-free readings (the tmin difference, the rank of the
     group minimum) beside crfollow.py D's floor criterion.
  B. The brake (Q1): b-hat0+ as printed (the max of the K = 8 boot brakes), the median of the same 8, the steady-state
     brake at the native count (LATE median of tmin - m-hat), and the brake at the knee's count (pooled floor windows).
  C. The floor's location statistics (Q3): per floor row, m-hat, l-hat, R6.2's F, candidate C, and the window p10 /
     p50 medians; their spread across rows, and each statistic's rise from the floor to the knee arms.
  E. Start-up (round 2): the first 8 windows' p50 and tmin per floor row, the median over the first K = 1, 3, 5, 8 windows against the LATE
     median (what a boot-set floor reads), and the share of LATE windows whose m-hat sits on the J = 0 low mode.
  D. The gates (Q1, Q4): each floor candidate's margin target - statistic per c3 arm; sign at the floor and the control, and its sign changes.
"""
import re
import sys
import tarfile
import statistics as st

P = print
NAME = re.compile(r"(c\w+-\d+)@def-([GD])(\d+)-s(\d+)-r(\d+)-a\d+-c\.log$")
TOK = re.compile(r"(\w+)=(\S+)")
RETX = re.compile(r"\bretx=(\d+)")
DT = re.compile(r"^\[DIAG\] t=([\d.]+)s ")


def fl(d, k):
    v = d.get(k, "-")
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def med(v):
    v = [x for x in v if x is not None]
    return st.median(v) if v else None


def parse(text):
    slaw, wst, diag = [], [], []
    for ln in text.splitlines():
        if '"warmup":true' in ln:
            slaw, wst, diag = [], [], []
        elif ln.startswith("[SLAW]") and " p=0 " in ln:
            slaw.append(dict(TOK.findall(ln)))
        elif ln.startswith("[WSTAT]") and " p=0 " in ln:
            wst.append(dict(TOK.findall(ln)))
        elif ln.startswith("[DIAG]"):
            m = RETX.search(ln)
            diag.append((float(DT.match(ln).group(1)), int(m.group(1)) if m else None))
    # join WSTAT to SLAW by (t, k)
    wk = {(w.get("t"), w.get("k")): w for w in wst}
    for s in slaw:
        w = wk.get((s.get("t"), s.get("k")))
        s["p10"] = w.get("p10") if w else "-"
        s["p50"] = w.get("p50") if w else "-"
    # per-window retransmit delta in the same tick (crfollow.py D's pairing), and the pairing check
    bad = 0
    for i, s in enumerate(slaw):
        s["dretx"] = None
        if 1 <= i < len(diag) and diag[i][1] is not None and diag[i - 1][1] is not None:
            s["dretx"] = diag[i][1] - diag[i - 1][1]
            if abs(diag[i][0] - fl(s, "t")) > 0.06:
                bad += 1
    return slaw, bad


def load(path, keep):
    rows = {}
    with tarfile.open(path, "r:gz") as tf:
        for mb in tf:
            g = NAME.search(mb.name)
            if g and mb.isfile():
                key = (g.group(1), g.group(2), int(g.group(3)), g.group(4), int(g.group(5)))
                if keep(key):
                    rows[key] = parse(tf.extractfile(mb).read().decode("utf-8", "replace"))
    return rows


def late(slaw):
    return [w for w in slaw if (fl(w, "t") or 0) >= 15.0 and (fl(w, "nk") or 0) >= 10]


def groups(lw, g):
    """Consecutive non-overlapping groups of g LATE windows -> list of lists."""
    return [lw[i:i + g] for i in range(0, len(lw) - g + 1, g)]


CELLS = (
    ("s62 J0", 0, "c3j0-25", (12, 80, 128, 512)),
    ("61c J5", 1, "c3-25", (12, 16, 24, 80, 100, 108, 120, 152, 512)),
    ("61a J5", 2, "c3-25", (12, 16, 24, 92, 104, 112, 124, 2048)),
    ("c2 s60", 3, "c2-100", (16,)),
)


def main(argv):
    srcs = []
    for lab, i, cell, arms in CELLS:
        srcs.append(load(argv[1 + i], lambda k, cell=cell, arms=arms: k[0] == cell and k[2] in arms))
    P("sampler design (sdalign.py); inputs " + ", ".join(a.replace("\\", "/").split("/")[-1] for a in argv[1:5]))
    tot_bad = sum(b for rows in srcs for _, b in rows.values())
    P(f"pairing check: windows whose paired [DIAG] tick lies > 60 ms from the [SLAW] close: {tot_bad}")

    # ── A ──
    P("\n== A. Retransmits against the window minimum, per arm (LATE windows; 'retx' = the [DIAG] retx= delta in the same tick > 0) ==")
    P("  base = share of windows with a retransmit; low10 = share among the row's lowest-decile tmin windows; crit = crfollow's floor criterion")
    P("  (tmin < row median tp50 - 4 ms): its share of windows and their retx share; dT = median tmin of no-retx windows - of retx windows (ms);")
    P("  g8 = share of 8-window groups (2 s) whose lowest window carries a retransmit (no association: = base); ancf = [SLAW]'s anchor-bind count")
    P("  (windows where m-hat > l-hat + b-hat0+, of those with the law's inputs; the bind fraction of R6.2's min(m-hat, .)) at the last LATE window")
    P("  m = LATE median m-hat; tmin = median native tmin; G8 = median 8-window group minimum (ms)")
    sign, dose, dose50 = {}, {}, {}
    for (lab, _, cell, arms), rows in zip(CELLS, srcs):
        for W in arms:
            for key in sorted(k for k in rows if k[2] == W):
                lw = [w for w in late(rows[key][0]) if w["dretx"] is not None]
                if len(lw) < 40:
                    continue
                tm = [fl(w, "tmin") for w in lw]
                rx = [w["dretx"] > 0 for w in lw]
                base = sum(rx) / len(rx)
                cut = sorted(tm)[max(0, len(tm) // 10 - 1)]
                low = [r for t, r in zip(tm, rx) if t <= cut]
                ref = med([fl(w, "tp50") for w in lw])
                crit = [r for t, r in zip(tm, rx) if t < ref - 4.0]
                dT = (med([t for t, r in zip(tm, rx) if not r]) or 0) - (med([t for t, r in zip(tm, rx) if r]) or 0) if 0 < sum(rx) < len(rx) else None

                def gshare(g):
                    gs = groups(lw, g)
                    if not gs:
                        return None, 0
                    hit = sum(1 for gr in gs if min(gr, key=lambda w: fl(w, "tmin"))["dretx"] > 0)
                    return hit / len(gs), len(gs)
                g8, n8 = gshare(8)
                G8 = med([min(fl(w, "tmin") for w in gr) for gr in groups(lw, 8)])
                anc = lw[-1].get("ancf", "-")
                sign.setdefault((lab, W), []).append(dT)
                rm = med(tm)
                pm = med([fl(w, "p50") for w in lw])
                for w in lw:
                    dose50.setdefault((lab, W), []).append((w["dretx"], fl(w, "p50") - pm))
                    dose.setdefault((lab, W), []).append((w["dretx"], fl(w, "tmin") - rm, fl(w, "nk")))
                P(f"  {lab} {key[1]}{W:<5} s{key[3]:<2} r{key[4]}: n {len(lw):3d} base {base:.2f} (no-retx n {len(rx) - sum(rx):3d}) | low10 {sum(low) / len(low):.2f} (n {len(low)})"
                  f" | crit {len(crit) / len(lw):.2f} retx {sum(crit) / len(crit) if crit else float('nan'):.2f}"
                  f" | dT {dT if dT is None else round(dT, 2)} | g8 {g8:.2f} (n {n8})"
                  f" | m {med([fl(w, 'm') for w in lw]):.2f} tmin {rm:.2f} G8 {G8:.2f} nk {med([fl(w, 'nk') for w in lw]):.0f} ancf {anc}")

    P("\n  A2. Per arm: rows with dT > 0 (a sign tally; 6/6 has p = 1/64 under no association), and the dose: median of (tmin - the row's median tmin), ms,")
    P("      pooled over the arm's rows, by the tick's retransmit delta (windows in the class, their median nk: a class with more samples reads lower by count alone)")
    for (lab, W), v in sign.items():
        vv = [x for x in v if x is not None]
        d = dose[(lab, W)]
        cls = ((0, 0), (1, 2), (3, 5), (6, 10 ** 9))
        txt = " | ".join(f"retx {a}{'' if a == b else ('+' if b > 10 ** 6 else f'-{b}')}: {med([y for x, y, _ in d if a <= x <= b]) if any(a <= x <= b for x, _, _ in d) else float('nan'):+.2f}"
                         f" ({sum(1 for x, _, _ in d if a <= x <= b)}, nk {med([n for x, _, n in d if a <= x <= b]) or float('nan'):.0f})" for a, b in cls)
        P(f"    {lab} W{W:<5} dT > 0 on {sum(1 for x in vv if x > 0)}/{len(vv)} rows | {txt}")
        d5 = dose50[(lab, W)]
        P("      the same for the window p50: " + " | ".join(f"retx {a}{'' if a == b else ('+' if b > 10 ** 6 else f'-{b}')}: "
          f"{med([y for x, y in d5 if a <= x <= b]) if any(a <= x <= b for x, _ in d5) else float('nan'):+.2f}" for a, b in cls))

    # ── B ──
    P("\n== B. The brake, per floor row (ms): b-hat0+ (max of the K = 8 boot brakes, as printed), the median of the same 8, the steady-state brake at the")
    P("   native count (LATE median of tmin - m-hat, = s62's D0 construction), and the brake at the knee's count (median group minimum of g pooled LATE floor")
    P("   windows - LATE m-hat, g = knee nk / floor nk, the knee nk from the cell's K arms) ==")
    P("   'zeros' counts boot brakes that are exactly 0: a window that sets m-hat has tmin = m-hat, and window 1 always does, so the median of the 8 is")
    P("   biased low by construction (a reading, not a candidate). err = [SLAW]'s printed target - tmin, the quantity B1 reads (B1 margin = -err)")
    knee_nk = {}
    for (lab, _, cell, arms), rows in zip(CELLS, srcs):
        ks = [k for k in rows if k[2] in ((100, 108, 120) if lab == "61c J5" else (104, 112, 124) if lab == "61a J5" else (128,) if lab == "s62 J0" else ())]
        if ks:
            knee_nk[lab] = med([med([fl(w, "nk") for w in late(rows[k][0])]) for k in ks])
    knee_nk["c2 s60"] = 2342.0   # crrule.out §2, c2 k 1 n_K (the knee bracket is in rep60a/b; not reloaded here)
    for (lab, _, cell, arms), rows in zip(CELLS, srcs):
        W = arms[0]
        res = []
        for key in sorted(k for k in rows if k[2] == W):
            slaw = rows[key][0]
            lw = late(slaw)
            k8 = [w for w in slaw if w.get("b0k", "-") != "-"]
            bk = [float(x) for x in k8[0]["b0k"].split(",")] if k8 else []
            b0p = med([fl(w, "b0p") for w in lw])
            m = med([fl(w, "m") for w in lw])
            dlate = med([fl(w, "tmin") - fl(w, "m") for w in lw if fl(w, "m") is not None])
            nf = med([fl(w, "nk") for w in lw])
            g = max(1, round(knee_nk[lab] / nf))
            gm = med([min(fl(w, "tmin") for w in gr) for gr in groups(lw, g)])
            res.append((b0p, med(bk), dlate, gm - m))
            err = med([fl(w, "err") for w in lw])
            P(f"  {lab} {key[1]}{W} s{key[3]} r{key[4]}: b0p {b0p:7.2f} | median of 8 {med(bk):6.2f} (zeros {sum(1 for x in bk if x == 0)}) | steady native {dlate:5.2f} (nk {nf:.0f})"
              f" | at the knee's count {gm - m:5.2f} (g {g}, n ~{g * nf:.0f}) | LATE median err {err:+.2f}")
        if res:
            cols = list(zip(*res))
            P(f"  {lab}: across rows (min / median / max): " + " | ".join(
                f"{nm} {min(c):.2f} / {med(c):.2f} / {max(c):.2f}" for nm, c in zip(("b0p", "median of 8", "steady native", "at the knee's count"), cols)))

    # ── C ──
    P("\n== C. The floor's location statistics per floor row (LATE medians, ms), their spread across rows, and their rise from the floor arm to each other arm ==")
    P("   F = R6.2's min(m, l + b0p) + b0p; C = median of the last 4 native minima (R6.7 candidate C on a static row); p10 / p50 / tmin = median of the window's own")
    for (lab, _, cell, arms), rows in zip(CELLS, srcs):
        W = arms[0]
        tab = []
        for key in sorted(k for k in rows if k[2] == W):
            lw = late(rows[key][0])
            m, l, b0 = (med([fl(w, x) for w in lw]) for x in ("m", "l", "b0p"))
            C = med([fl(w, "tmin") for w in lw[-4:]])
            vals = (m, l, min(m, l + b0) + b0, C, med([fl(w, "tmin") for w in lw]), med([fl(w, "p10") for w in lw]), med([fl(w, "p50") for w in lw]))
            tab.append(vals)
        names = ("m-hat", "l-hat", "F (R6.2)", "C", "tmin", "p10", "p50")
        cols = list(zip(*tab))
        P(f"  {lab} floor W{W} ({len(tab)} rows): " + " | ".join(f"{nm} {med(c):.2f} [{min(c):.2f}, {max(c):.2f}] range {max(c) - min(c):.2f}" for nm, c in zip(names, cols)))
        fl_ref = {nm: med(c) for nm, c in zip(names, cols)}
        for Wa in arms[1:]:
            ks = sorted(k for k in rows if k[2] == Wa)
            if not ks:
                continue
            per = {nm: [] for nm in ("m-hat", "tmin", "p10", "p50")}
            for k in ks:
                lw = late(rows[k][0])
                for nm, x in (("m-hat", "m"), ("tmin", "tmin"), ("p10", "p10"), ("p50", "p50")):
                    per[nm].append(med([fl(w, x) for w in lw]))
            P(f"    W{Wa:<5} ({len(ks)} rows) rise over the floor: " + " | ".join(f"{nm} {med(v) - fl_ref[nm]:+.2f}" for nm, v in per.items())
              + f" | nk {med([med([fl(w, 'nk') for w in late(rows[k][0])]) for k in ks]):.0f}")

    # ── D ──
    P("\n== D. The gates for each floor candidate (r67's screens, r68's uniqueness and R-sign gates), one leg, on the c3 arms' medians over rows (ms) ==")
    P("   margin(W) = target(W) - statistic(W), target = F + A; A = decision 8's form at Bulk (g = 1, q = 0.10) with the cell's native floor numbers:")
    P("   A = S_F + q*F-hat, S_F = the floor arm's median window (p50 - tmin), F-hat = the floor arm's median tmin. For K5 the same S_F is a PLACEHOLDER")
    P("   (decision 8's spread mapped to the median is not logged), so K5's crossing W is not read; its signs and crossing count are.")
    P("   Gates: margin > 0 at the floor arm, < 0 at the control, exactly one sign change along W (R crosses 1 once).")
    P("   K1 R6.2 as the law would read on each arm: F = min(m, l + b0p) + b0p from that arm's own LATE medians; statistic tmin")
    P("   K2 a running brake: F = m + median over the arm's LATE windows of (tmin - m) (what any at-load brake reads); statistic tmin")
    P("   K3 candidate C at the floor's count: F = the floor arm's median tmin, fixed; statistic tmin")
    P("   K4 count-matched minimum: F = M0(n_W), the median group minimum of g = round(n_W / n_floor) pooled LATE floor windows; statistic tmin")
    P("   K5 count-free location: F = the floor arm's median window p50, fixed; A = S_F + q*F (placeholder S_F); statistic the window p50")
    q = 0.10
    for (lab, _, cell, arms), rows in zip(CELLS[:3], srcs[:3]):
        def arm_rows(Wa):
            return [late(rows[k][0]) for k in sorted(rows) if k[2] == Wa]
        flr = arm_rows(arms[0])
        nf = med([med([fl(w, "nk") for w in lw]) for lw in flr])
        f_tmin = med([med([fl(w, "tmin") for w in lw]) for lw in flr])
        f_p50 = med([med([fl(w, "p50") for w in lw]) for lw in flr])
        S_F = med([med([fl(w, "p50") - fl(w, "tmin") for w in lw]) for lw in flr])
        A_min, A_50 = S_F + q * f_tmin, S_F + q * f_p50
        P(f"  {lab}: floor W{arms[0]} nk {nf:.0f}; S_F {S_F:.2f}; A (min) {A_min:.2f}; A (K5) {A_50:.2f}")
        cand = {k: [] for k in ("K1", "K2", "K3", "K4", "K5")}
        for Wa in arms:
            rs = arm_rows(Wa)
            tmin = med([med([fl(w, "tmin") for w in lw]) for lw in rs])
            p50 = med([med([fl(w, "p50") for w in lw]) for lw in rs])
            nW = med([med([fl(w, "nk") for w in lw]) for lw in rs])
            F1 = med([min(med([fl(w, "m") for w in lw]), med([fl(w, "l") for w in lw]) + med([fl(w, "b0p") for w in lw])) + med([fl(w, "b0p") for w in lw]) for lw in rs])
            F2 = med([med([fl(w, "m") for w in lw]) + med([fl(w, "tmin") - fl(w, "m") for w in lw]) for lw in rs])
            g = max(1, round(nW / nf))
            F4 = med([med([min(fl(w, "tmin") for w in gr) for gr in groups(lw, g)]) for lw in flr])
            for k, v in (("K1", F1 + A_min - tmin), ("K2", F2 + A_min - tmin), ("K3", f_tmin + A_min - tmin), ("K4", F4 + A_min - tmin), ("K5", f_p50 + A_50 - p50)):
                cand[k].append((Wa, v))
        for k, v in cand.items():
            signs = [x > 0 for _, x in v]
            flips = sum(1 for a, b in zip(signs, signs[1:]) if a != b)
            gate = "PASS" if signs[0] and not signs[-1] and flips == 1 else "FAIL"
            P(f"    {k}: " + " | ".join(f"W{Wa} {x:+.2f}" for Wa, x in v) + f"  -> floor {'+' if signs[0] else '-'}, control {'+' if signs[-1] else '-'}, sign changes {flips}: {gate}")


    # ── E ──
    P("\n== E. Start-up: what a floor set from the first K windows reads (ms). Static floor rows: every window has the same store size, so the")
    P("   boot windows stand in for the K lowest-store windows of a closed loop booting at the floor pool. 'first 8' = k = 1..8 after the warm-up marker ==")
    for (lab, _, cell, arms), rows in zip(CELLS, srcs):
        W = arms[0]
        for key in sorted(k for k in rows if k[2] == W):
            slaw = rows[key][0]
            lw = late(slaw)
            b8 = slaw[:8]
            lp, lt = med([fl(w, "p50") for w in lw]), med([fl(w, "tmin") for w in lw])
            def dK(x, K, ref):
                return med([fl(w, x) for w in slaw[:K]]) - ref
            P(f"  {lab} {key[1]}{W} s{key[3]} r{key[4]}: nk {[int(fl(w, 'nk')) for w in b8]}; p50 {[round(fl(w, 'p50'), 1) for w in b8]}; LATE p50 {lp:.2f}, tmin {lt:.2f}")
            P("      median of the first K minus LATE:  p50 " + " ".join(f"K{K} {dK('p50', K, lp):+.2f}" for K in (1, 3, 5, 8))
              + " | tmin " + " ".join(f"K{K} {dK('tmin', K, lt):+.2f}" for K in (1, 3, 5, 8)))
    P("  J = 0: share of LATE windows whose m-hat lies on the low mode (< 41.9 ms, count-rule's 41.2-41.9) and the LATE m-hat range")
    for key in sorted(k for k in srcs[0]):
        lw = late(srcs[0][key][0])
        ms_ = [fl(w, "m") for w in lw]
        P(f"    {key[1]}{key[2]} s{key[3]} r{key[4]}: {sum(1 for x in ms_ if x < 41.9) / len(ms_):.2f} of {len(ms_)}; m-hat {min(ms_):.2f} - {max(ms_):.2f}")


if __name__ == "__main__":
    main(sys.argv)
