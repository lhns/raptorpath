"""Stage-3d readings (three-knobs.md 17.9, predictions Q3d-1 to Q3d-9).  Called from summarize3d.py:  python summarize3d.py q1|...|q9|aa|tables
Every function prints the numbers behind one prediction, with the registered band and the PASS / FAIL word computed from it; the
bands are copied from the registration and never tuned."""
import math
import os

import numpy as np

import summarize3d as D

S = D.S
INF = math.inf
G = S.gname
TAG = S.TAG
STAGE = ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10", "K-L10b")
RK = ("share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "ooo_per_delivered", "repair_per_delivered")


def f(x, nd=4):
    return "nan" if x is None or (isinstance(x, float) and math.isnan(x)) else ("inf" if x == INF else "%.*g" % (nd, x))


def band(x, lo, hi):
    return "PASS" if lo <= x <= hi else "FAIL"


def m_(cell, a, k):
    return S.mean(cell, a, k)


def gv(cell, a, k):
    """Mean over seeds of a gauge."""
    return float(np.mean([S.R(cell, a, s)["gauges"][k] for s in S.seeds_of(cell, a)]))


def all_runs():
    for (n, s), r in sorted(S.load().items()):
        cell, _, arm = n.partition(".")
        yield cell, arm, s, r


def ratio(a, b):
    return a / b if b else float("nan")


def tab(cell, arms, keys):
    print("| arm | " + " | ".join(keys) + " |")
    print("|---|" + "---|" * len(keys))
    for a in arms:
        if not S.seeds_of(cell, a):
            continue
        print("| %s | " % a + " | ".join(f(S.mean(cell, a, k)) for k in keys) + " |")


# ---- Q3d-1 ------------------------------------------------------------------------------------
def q1():
    bad = D.ident()
    tot, unf = D.live()
    fam = {}
    for cell, arm, s, r in all_runs():
        if r.get("finished") is None:
            continue
        k = (cell, arm.split("-")[0] if arm.startswith(("NT", "NH", "SW", "PL", "AA", "ID")) else arm)
        fam.setdefault(k, [0, 0])
        fam[k][0] += 1
        fam[k][1] += 1 if r["finished"] else 0
    print("families with an unfinished run: %s" % {k: v for k, v in fam.items() if v[0] != v[1]})
    print("Q3d-1 identity differences: %d; unfinished runs: %d of %d -> %s" % (bad, unf, tot, "PASS" if not bad and not unf else "FAIL"))


# ---- Q3d-2 ------------------------------------------------------------------------------------
def q2():
    print("### Q3d-2 the commitment\n")
    viol_runs = []
    n = viol = 0
    worst = {}
    for cell, arm, s, r in all_runs():
        g = r.get("gauges")
        if g is None:
            continue
        n += 1
        viol += g["commit_viol"]
        if g["commit_viol"]:
            viol_runs.append((cell, arm, s, g["commit_viol"]))
        if cell not in D.NO_NUDGE_CELLS or cell in D.LONG:
            if cell == "P2":
                continue
            c = g["repair_sends"]
            rt = g["skip_below_floor"] / c if c else (0.0 if not g["skip_below_floor"] else float("inf"))
            if rt > worst.get(cell, (0, None))[0]:
                worst[cell] = (rt, (arm, s, g["skip_below_floor"], c))
    print("runs %d: commit_viol total %d (runs with a violation: %s) -> %s" % (n, viol, viol_runs[:8], "PASS" if viol == 0 else "FAIL"))
    print("skip_below_floor / copies, worst run per one-path cell (bar 0.001):")
    allok = True
    for cell, (rt, who) in sorted(worst.items()):
        ok = rt <= 0.001
        allok &= ok
        print("  %-10s %.5f  %s %s" % (cell, rt, who, "" if ok else "FAIL"))
    tot_skip = sum(r["gauges"]["skip_below_floor"] for c, a, s, r in all_runs() if "gauges" in r)
    tot_cp = sum(r["gauges"]["repair_sends"] for c, a, s, r in all_runs() if "gauges" in r and "skip_below_floor" in r["gauges"] and r["gauges"]["skip_below_floor"] is not None)
    print("  total skips %d over %d copies in all runs; bar on every one-path cell's worst run: %s" % (tot_skip, tot_cp, "PASS" if allok else "FAIL"))
    print("\nK-L10b at the lowest T (S5 H = 0, T = 0.180): duplicates against copies (bar dup >= 0.5 copies; not a pass condition of the design)")
    for arm in ("SW-0p18", G(0.4, 0.0)):
        ss = S.seeds_of("K-L10b", arm)
        if not ss:
            continue
        cp = [S.R("K-L10b", arm, s)["gauges"]["repair_sends"] for s in ss]
        du = [S.R("K-L10b", arm, s)["gauges"]["dup"] for s in ss]
        print("  %-10s copies per seed %s; dup per seed %s; sum dup / sum copies = %.3f -> %s" % (
            arm, cp, du, ratio(sum(du), sum(cp)), "confirmed" if ratio(sum(du), sum(cp)) >= 0.5 else "NOT confirmed"))
    print("  for comparison, C3 in K-L10b: dup / copies = %.3f" % ratio(gv("K-L10b", "C3", "dup"), gv("K-L10b", "C3", "repair_sends")))


# ---- Q3d-3 ------------------------------------------------------------------------------------
def q3():
    print("### Q3d-3 OV with the consistent law (T = 0.4)\n")
    keys = ("share_any", "share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99", "intake_expired", "intake_expired_deq", "link_util_w",
            "queue_drop_out", "resend_braked", "verdicts", "pass_floor", "F_lag")
    for cell in ("OV", "OV@b"):
        print("#### %s\n" % cell)
        tab(cell, ["C1", "C3", "C3S", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S5v-H0", "S5v-Hinf"], keys)
        a, b = m_(cell, G(0.4, 0.0), "share_by_0p4"), m_(cell, G(0.4, INF), "share_by_0p4")
        print("\n[%s] S5 H=0 share_by_0p4 %.4f in [0.78, 0.84]: %s" % (cell, a, band(a, 0.78, 0.84)))
        print("[%s] S5 H=inf %.4f in [0.75, 0.84]: %s; at least H=0 - 0.03 = %.4f: %s" % (cell, b, band(b, 0.75, 0.84), a - 0.03, "PASS" if b >= a - 0.03 else "FAIL"))
        d = S.diffv(cell, G(0.4, INF), G(0.4, 0.0), "share_by_0p4")
        print("   paired H=inf minus H=0: %s" % S.fmt(S.read(d, S.bar("share_by_0p4", cell))))
        if S.seeds_of(cell, "S5v-Hinf"):
            vi, v0 = m_(cell, "S5v-Hinf", "share_by_0p4"), m_(cell, "S5v-H0", "share_by_0p4")
            nv = [S.R(cell, "S5v-Hinf", s)["verdicts"] for s in S.seeds_of(cell, "S5v-Hinf")]
            fin = all(S.R(cell, a_, s)["finished"] for a_ in ("S5v-Hinf", "S5v-H0") for s in S.seeds_of(cell, a_))
            print("[%s] S5v H=inf %.4f in [0.05, 0.25]: %s; verdicts mean %.0f (min %d) > 1000: %s; every S5v run finished: %s" % (
                cell, vi, band(vi, 0.05, 0.25), np.mean(nv), min(nv), "PASS" if np.mean(nv) > 1000 else "FAIL", fin))
            print("[%s] S5v H=0 %.4f within 0.02 of S5 H=0 %.4f (diff %+.4f): %s" % (cell, v0, a, v0 - a, "PASS" if abs(v0 - a) <= 0.02 else "FAIL"))


# ---- Q3d-4 ------------------------------------------------------------------------------------
def q4():
    print("### Q3d-4 baselines and the sweep form (OV and CL)\n")
    for base in ("OV", "CL"):
        t, b = base, base + "@b"
        for arm in ("C3", "C1"):
            ct, cb = m_(t, arm, "repair_per_delivered"), m_(b, arm, "repair_per_delivered")
            dt_, db_ = ratio(gv(t, arm, "dup"), gv(t, arm, "repair_sends")), ratio(gv(b, arm, "dup"), gv(b, arm, "repair_sends"))
            print("- %s %s: copies per delivered unit tail %.4f -> blocker %.4f (%s); dup share (dup / copies) tail %.3f -> blocker %.3f%s" % (
                base, arm, ct, cb, "falls" if cb < ct else "does NOT fall", dt_, db_,
                ("  [bars: C3 dup share from about 0.5 to below 0.1: %s]" % ("PASS" if arm == "C3" and db_ < 0.1 else "FAIL")) if arm == "C3" else ""))
        mx = 0.0
        for arm in (G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0)):
            row = []
            for k in ("share_by_0p4", "share_by_2"):
                d = m_(b, arm, k) - m_(t, arm, k)
                mx = max(mx, abs(d))
                row.append("%s %+.4f" % (k, d))
            print("    S5 %-12s blocker - tail: %s" % (arm, "; ".join(row)))
        print("  largest S5 move between forms in %s: %.4f (bar 0.02): %s" % (base, mx, "PASS" if mx < 0.02 else "FAIL"))


# ---- Q3d-5 ------------------------------------------------------------------------------------
def rec(cell, arm):
    out = []
    for s in S.seeds_of(cell, arm):
        r = S.R(cell, arm, s)
        out.append(r.get("recovery_time"))
    return out


def recfmt(v):
    fin = [x for x in v if x is not None]
    nn = len(v) - len(fin)
    return "%s s%s" % ("%.2f" % np.mean(fin) if fin else "n/a", (" (not within the run in %d of %d seeds)" % (nn, len(v))) if nn else "")


def q5():
    print("### Q3d-5 recovery time after the outage (seconds after its end; seed means; 0.5 s windows, step 0.1 s)\n")
    arms = ["C1", "C3", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0)]
    for cell in ("CL-long", "CL-long@b", "CL", "CL@b", "M-I2", "M-L10"):
        print("#### %s\n" % cell)
        print("| arm | recovery time | per seed |")
        print("|---|---|---|")
        for a in arms + ["C3S"]:
            if not S.seeds_of(cell, a):
                continue
            v = rec(cell, a)
            print("| %s | %s | %s |" % (a, recfmt(v), " ".join("-" if x is None else "%.1f" % x for x in v)))
        print()
    for cell, lo, hi in (("CL-long", 5.4, 7.6), ("CL-long@b", 4.1, 6.4)):
        v = rec(cell, "C3")
        fin = [x for x in v if x is not None]
        mu = float(np.mean(fin)) if len(fin) == len(v) and fin else float("nan")
        print("[%s] C3 recovery %s in [%.1f, %.1f]: %s" % (cell, recfmt(v), lo, hi, band(mu, lo, hi) if not math.isnan(mu) else "FAIL (not within the run in some seeds)"))
        v2 = rec(cell, G(2.0, 0.0))
        v2b = rec(cell, G(2.0, INF))
        for nm, vv in (("S5 T=2 H=0", v2), ("S5 T=2 H=inf", v2b)):
            fin2 = [x for x in vv if x is not None]
            if len(fin2) == len(vv) == len(fin) and fin2:
                d = np.array(vv) - np.array(v)
                m = max(0.05 * mu, 0.1)
                print("   %s minus C3: %s (bar m = max(5 %% of C3, 0.1 s) = %.2f s, a worker decision)" % (nm, S.fmt(S.read(d, m)), m))
            else:
                print("   %s: %s against C3 %s" % (nm, recfmt(vv), recfmt(v)))
        for H, hi_ in ((0.0, 0.6), (INF, 1.0)):
            vv = rec(cell, G(0.4, H))
            fin2 = [x for x in vv if x is not None]
            mu2 = float(np.mean(fin2)) if len(fin2) == len(vv) and fin2 else float("nan")
            print("   S5 T=0.4 H=%s recovery %s in [0, %.1f]: %s" % (f(H), recfmt(vv), hi_, band(mu2, 0.0, hi_) if not math.isnan(mu2) else "FAIL"))
        print("   C1 recovery: %s (reported)" % recfmt(rec(cell, "C1")))
    for cell in ("CL", "CL@b"):
        v = rec(cell, "C3")
        ok = all(x is None or x > 4.0 for x in v)
        print("[%s] C3 'not within the run' or above 4 s in every seed: %s (%s): %s" % (cell, recfmt(v), v, "PASS" if ok else "FAIL"))
        vv = rec(cell, G(0.4, 0.0))
        vi = rec(cell, G(0.4, INF))
        print("[%s] S5 T=0.4 <= 0.6 s: H=0 %s: %s; H=inf %s: %s" % (
            cell, recfmt(vv), "PASS" if all(x is not None and x <= 0.6 for x in vv) else "FAIL", recfmt(vi), "PASS" if all(x is not None and x <= 0.6 for x in vi) else "FAIL"))
    print("\nM cells: the pre-outage interval [0.5 s, outage start - 0.1 s] is empty (the outage starts at 0.4859 s), so no recovery time is defined: not computed.")


# ---- Q3d-6 ------------------------------------------------------------------------------------
def win_by2(cell, arm, s, x0):
    r = S.R(cell, arm, s)
    ser = r["win_ot2"]
    n_units = r["units"]
    end = (n_units - 1) / (12000.0) - 2.0
    sel = [ser[k] for k in range(len(ser)) if k * 0.1 > x0 and k * 0.1 + 0.5 <= end + 1e-9]
    return float(np.mean(sel)) if sel else float("nan")


def q6():
    print("### Q3d-6 the 2 s lifetime where it binds (OV-long, share delivered within 2 s of hand-over, windows after the first dequeue expiry)\n")
    pred = 10000.0 / 12000.0 * (1 - 0.03)
    print("(C/lambda)(1 - p0) = %.4f\n" % pred)
    for cell in ("OV-long", "OV-long@b"):
        ref = G(2.0, 0.0)
        x0 = {s: (S.R(cell, ref, s)["first_deq_exp"] if S.R(cell, ref, s)["first_deq_exp"] is not None else INF) for s in S.seeds_of(cell, ref)}
        print("#### %s: first dequeue expiry of S5 T=2 H=0 per seed (s): %s\n" % (cell, " ".join("%.1f" % x for x in x0.values())))
        print("| arm | windowed by-2 share (mean over seeds) | per seed min / max | first dequeue expiry (own, mean over seeds with one) |")
        print("|---|---|---|---|")
        res = {}
        for a in ("C1", "C3", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), "S5v-H0", "S5v-Hinf"):
            ss = S.seeds_of(cell, a)
            if not ss:
                continue
            v = [win_by2(cell, a, s, x0[s]) for s in ss]
            fe = [S.R(cell, a, s)["first_deq_exp"] for s in ss if S.R(cell, a, s)["first_deq_exp"] is not None]
            res[a] = float(np.nanmean(v))
            print("| %s | %.4f | %.4f / %.4f | %s |" % (a, res[a], np.nanmin(v), np.nanmax(v), "%.1f" % np.mean(fe) if fe else "none"))
        print()
        a1 = res[G(2.0, 0.0)]
        print("[%s] S5 T=2 H=0 %.4f in [0.78, 0.83]: %s" % (cell, a1, band(a1, 0.78, 0.83)))
        print("[%s] C3 %.4f and C1 %.4f both <= 0.05: %s" % (cell, res["C3"], res["C1"], "PASS" if res["C3"] <= 0.05 and res["C1"] <= 0.05 else "FAIL"))
        a2 = res[G(2.0, INF)]
        print("[%s] S5 T=2 H=inf %.4f in [0.40, 0.83] (reported): %s" % (cell, a2, band(a2, 0.40, 0.83)))


# ---- Q3d-7 ------------------------------------------------------------------------------------
def kp_line(cell, names, k="share_T"):
    kk = S.kp(cell, names, k)
    m = S.bar(k, cell)
    return kk, m, S.read(kk["K"], m), S.read(kk["P"], m)


def q7():
    print("### Q3d-7 planted controls through T (share_T, H = 0, p = 0.4)\n")
    exp = {"STEP": ("K", 0.65, 0.15, "P", 0.33, 0.10), "KINK": ("K", 0.65, 0.15, "P", 0.33, 0.10), "SPIKE": ("K", 0.0, None, "P", -0.65, 0.15)}
    for cell in ("OV", "K-L10"):
        for kind in ("STEP", "KINK", "SPIKE"):
            names = ["PL-%s-%s" % (kind, TAG(x)) for x in S.NUDGE]
            if not all(S.have(cell, n) for n in names):
                print("  %s %s incomplete" % (cell, kind))
                continue
            kk, m, rk, rp = kp_line(cell, names)
            pts = [S.mean(cell, n, "share_T") for n in names]
            print("- %s %s: share_T at 0.98..1.02: %s" % (cell, kind, " ".join("%.4f" % x for x in pts)))
            print("    K %s;  P %s" % (S.fmt(rk), S.fmt(rp)))
            if cell == "OV":
                e = exp[kind]
                if kind == "SPIKE":
                    okK = rk["word"] == "equivalent"
                    okP = rp["word"].startswith("shown smaller") and abs(rp["mean"] - e[4]) <= e[5]
                else:
                    okK = rk["word"].startswith("shown larger") and abs(rk["mean"] - e[1]) <= e[2]
                    okP = rp["word"].startswith("shown larger") and abs(rp["mean"] - e[4]) <= e[5]
                print("    registered: K %s, P %s  -> %s" % (
                    "equivalent" if kind == "SPIKE" else "shown at %.2f +- %.2f" % (e[1], e[2]), "shown at %.2f +- %.2f" % (e[4], e[5]), "PASS" if okK and okP else "FAIL (K %s, P %s)" % (okK, okP)))
            else:
                print("    registered: all three read equivalent, reported as no power -> %s" % ("equivalent on K and P (no power)" if rk["word"] == "equivalent" and rp["word"] == "equivalent" else "NOT both equivalent"))
    print("\n### unplanted S5 nudges (T = 0.4 * {0.98..1.02} at H = 0.02, and H = 0.02 * {..} at T = 0.4), every cell but X, P2 and the long cells; m = max(T bar, A/A floor)\n")
    shown = und = tot = 0
    lines = []
    for cell in D.SHORT + D.BLK:
        for which in ("T", "H"):
            names = ["N%s-%s" % (which, TAG(x)) if x != 1.0 else G(0.4, 0.02) for x in S.NUDGE]
            if not all(S.have(cell, n) for n in names):
                print("  %s %s nudges incomplete" % (cell, which))
                continue
            for k in RK:
                pts = [S.vals(cell, n, k) for n in names]
                if all(np.all(np.isinf(p)) for p in pts):
                    continue
                kk = S.kp(cell, names, k)
                m = S.bar(k, cell)
                for st in ("K", "P"):
                    rd = S.read(kk[st], m)
                    tot += 1
                    if rd["word"].startswith("shown"):
                        shown += 1
                        lines.append("  SHOWN %s %s-nudge %s %s: %s" % (cell, which, k, st, S.fmt(rd)))
                    elif rd["word"].startswith("undet"):
                        und += 1
                        lines.append("  undetermined %s %s-nudge %s %s: %s" % (cell, which, k, st, S.fmt(rd)))
    print("\n".join(lines))
    print("%d K and P statistics: %d shown, %d undetermined, %d equivalent" % (tot, shown, und, tot - shown - und))
    print("K-L10 unplanted nudges (no power in this cell: the share is flat in T): reported separately above where they fall.")


# ---- Q3d-8 ------------------------------------------------------------------------------------
def hf_arm(T, H, hf):
    return "G-T%s-H%s-F%s" % (TAG(T), TAG(H), hf)


def q8():
    print("### Q3d-8 two paths (P2; fast path 10 ms, slow path 60 ms, phi = 0.5, i.i.d. 3 % loss on both)\n")
    keys = ("share_any", "share_T", "share_by_0p4", "dh_p50", "dh_p90", "dh_p99", "ooo_per_delivered", "pass_floor", "late_floor", "H_max", "repair_per_delivered", "floors_per_delivered")
    cell = "P2"
    arms = ["C3"] + [hf_arm(T, H, hf) for T in (0.075, 0.4, INF) for H in (0.0, INF) for hf in ("est", "0")]
    tab(cell, arms, keys)
    units = S.R(cell, "C3", 0)["units"]
    print()

    def ga(a, k):
        return np.mean([S.R(cell, a, s)["gauges"][k] for s in S.seeds_of(cell, a)])
    for hf in ("0", "est"):
        a = hf_arm(0.075, INF, hf)
        lf = ga(a, "late_floor") / units
        print("[T=0.075 H=inf H_f=%s] late_floor / units = %.4f; pass_floor %.1f; late_floor after 1 s %.2f; H_f (max over the run) %.4f s; dh_p50 %.4f s; share_T %.4f" % (
            hf, lf, ga(a, "pass_floor"), ga(a, "late_floor_ge1"), m_(cell, a, "H_max"), m_(cell, a, "dh_p50"), m_(cell, a, "share_T")))
    a0, ae = hf_arm(0.075, INF, "0"), hf_arm(0.075, INF, "est")
    lf0, lfe = ga(a0, "late_floor") / units, ga(ae, "late_floor") / units
    print("  H_f = 0: late_floor/units in [0.35, 0.50]: %s" % band(lf0, 0.35, 0.50))
    print("  H_f = est: late_floor/units <= 0.03: %s; late_floor among units handed over after 1 s = %.2f (must be 0): %s; H_f in [0.045, 0.055]: %s; pass_floor > 0: %s" % (
        "PASS" if lfe <= 0.03 else "FAIL", ga(ae, "late_floor_ge1"), "PASS" if ga(ae, "late_floor_ge1") == 0 else "FAIL", band(m_(cell, ae, "H_max"), 0.045, 0.055),
        "PASS" if ga(ae, "pass_floor") > 0 else "FAIL"))
    print("  dilemma: dh_p50 at H_f = est %.4f in [0.055, 0.070]: %s; at H_f = 0 %.4f in [0.009, 0.020]: %s" % (
        m_(cell, ae, "dh_p50"), band(m_(cell, ae, "dh_p50"), 0.055, 0.070), m_(cell, a0, "dh_p50"), band(m_(cell, a0, "dh_p50"), 0.009, 0.020)))
    dsh = m_(cell, ae, "share_T") - m_(cell, a0, "share_T")
    print("  share_T within 0.02 between the two H_f arms: %.4f vs %.4f (diff %+.4f): %s" % (m_(cell, ae, "share_T"), m_(cell, a0, "share_T"), dsh, "PASS" if abs(dsh) <= 0.02 else "FAIL"))
    for hf in ("est", "0"):
        a = hf_arm(0.4, INF, hf)
        print("[T=0.4 H=inf H_f=%s] late_floor %.2f (must be 0): %s; pass_floor / units %.5f <= 0.002: %s (vacuous case)" % (
            hf, ga(a, "late_floor"), "PASS" if ga(a, "late_floor") == 0 else "FAIL", ga(a, "pass_floor") / units, "PASS" if ga(a, "pass_floor") / units <= 0.002 else "FAIL"))
    worst = 0.0
    wa = None
    for a in arms:
        r = ratio(ga(a, "dup"), ga(a, "repair_sends"))
        if r > worst:
            worst, wa = r, a
    print("dup / copies <= 0.1 for every arm: worst %.3f (%s): %s" % (worst, wa, "PASS" if worst <= 0.1 else "FAIL"))


# ---- Q3d-9 ------------------------------------------------------------------------------------
def q9():
    print("### Q3d-9 continuity and the K cells\n")
    print("S5 at T = 0.4, H = inf against stage 3c's S4 (the 3c code path, arm S4ref), share_by_0p4, K cells:")
    for cell in ("K-I2", "K-L10", "K-L10b"):
        if not S.seeds_of(cell, "S4ref"):
            print("  %s: S4ref not run" % cell)
            continue
        m = S.bar("share_by_0p4", cell)
        d = S.diffv(cell, G(0.4, INF), "S4ref", "share_by_0p4")
        print("  %s: S5 %.5f, S4 (3c) %.5f, paired %s; at least S4 - m: %s" % (cell, m_(cell, G(0.4, INF), "share_by_0p4"), m_(cell, "S4ref", "share_by_0p4"), S.fmt(S.read(d, m)),
                                                                           "PASS" if d.mean() >= -m else "FAIL"))
    print("\nstall parts (Q3c-10 bounds), S5 T = 0.4, H = inf; C3 reported:")
    import batch3d
    for cell in ("K-I2", "K-L10", "K-I2@b", "K-L10@b", "K-L10b", "K-L10b@b"):
        lc, d = batch3d.base(cell)
        owd = d["owd"]
        P = min(max(4 * owd, 0.025), 0.1)
        for a in (G(0.4, INF), "C3"):
            ok_i, ok_o, n_ep = [], [], 0
            vals_o = []
            for s in S.seeds_of(cell, a):
                pt = S.R(cell, a, s)["stall3c"]
                if pt is None:
                    continue
                n_ep += 1
                ok_i.append(pt["i"] <= P + owd + 0.005)
                ok_o.append(pt["o"] == 0.0 or pt["o"] <= owd + 1.0 / 50.0 + 0.002)
                if pt["o"]:
                    vals_o.append(round(pt["o"], 4))
            print("  %-9s %-14s episodes %2d; (i) <= %.3f in %d; (o) <= %.4f where non-zero in %d (non-zero values %s); max (i) %.4f" % (
                cell, a, n_ep, P + owd + 0.005, sum(ok_i), owd + 0.02 + 0.002, sum(ok_o), vals_o,
                max([S.R(cell, a, s)["stall3c"]["i"] for s in S.seeds_of(cell, a) if S.R(cell, a, s)["stall3c"]] or [0.0])))


def aa():
    print("### A/A self-check (frozen nf_stage3d.json)\n")
    nf = S.nf_all()
    sc = nf["selfcheck"]
    words = [v["word"] for v in sc.values()]
    print("%d readings over %d cells: %d equivalent, %d undetermined, %d shown" % (
        len(words), len(nf["cells"]), words.count("equivalent"), sum(w.startswith("undet") for w in words), sum(w.startswith("shown") for w in words)))
    for k, v in sc.items():
        if v["word"].startswith("shown"):
            print("  shown:", k, S.fmt(v))
    print("\nundetermined (cell-key: mean [lo, hi] vs m):")
    for k, v in sc.items():
        if v["word"].startswith("undet"):
            print("  %s: %s" % (k, S.fmt(v)))
    print("\nfloors (cell: readout = p95 |A - A'|, ms for times; non-zero only):")
    for cell, d in nf["cells"].items():
        print("  %-9s %s" % (cell, "; ".join("%s=%s" % (k, f(v, 3)) for k, v in d.items() if v not in (0.0,))))


def main(cmd):
    {"q1": q1, "q2": q2, "q3": q3, "q4": q4, "q5": q5, "q6": q6, "q7": q7, "q8": q8, "q9": q9, "aa": aa}[cmd]()
