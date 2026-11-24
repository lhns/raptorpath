"""Stage-3c readings (three-knobs.md 15.6.4).  Called from summarize3c.py:  python summarize3c.py q2|...|q10|tables|aa|rep
Every function prints the numbers behind one prediction; the PASS/FAIL words are written into results-stage3c.md by hand from
these numbers, against the registered bands, never tuned."""
import json
import math
import os

import numpy as np

import summarize3c as S

INF = math.inf
G = S.gname
STAGE = ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10")
RK = ("share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "ooo_per_delivered", "repair_per_delivered")
ARMS = ["C1", "C3", "C3S", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S4b", G(0.4, 0.02)]


def f(x, nd=4):
    return "nan" if x is None or (isinstance(x, float) and math.isnan(x)) else ("inf" if x == INF else "%.*g" % (nd, x))


def band(x, lo, hi):
    return "PASS" if lo <= x <= hi else "FAIL"


def m_(cell, a, k):
    return S.mean(cell, a, k)


def gauge(cell, a, k):
    return np.mean([S.R(cell, a, s)["gauges"][k] for s in S.seeds_of(cell, a)])


def tab(cell, arms, keys):
    print("| arm | " + " | ".join(keys) + " |")
    print("|---|" + "---|" * len(keys))
    for a in arms:
        if not S.seeds_of(cell, a):
            continue
        print("| %s | " % a + " | ".join(f(S.mean(cell, a, k)) for k in keys) + " |")


def all_runs(sup=False):
    for (n, s), r in sorted(S.load().items()):
        cell, _, arm = n.partition(".")
        if arm.startswith("SUP") != sup:
            continue
        yield cell, arm, s, r


def q1():
    S.ident()
    tot, unf, lines = S.liveness()
    print("\nLiveness: `finished` is true in %d of %d runs that carry it." % (tot - unf, tot))
    for x in lines[:80]:
        print("  UNFINISHED", x)
    # by arm family
    fam = {}
    for cell, arm, s, r in all_runs():
        if r.get("finished") is None:
            continue
        fam.setdefault((cell, arm.split("-")[0] if arm.startswith(("NT", "NH", "SW", "PC", "AA", "ID")) else arm), [0, 0])
        fam[(cell, arm.split("-")[0] if arm.startswith(("NT", "NH", "SW", "PC", "AA", "ID")) else arm)][0] += 1
        fam[(cell, arm.split("-")[0] if arm.startswith(("NT", "NH", "SW", "PC", "AA", "ID")) else arm)][1] += 1 if r["finished"] else 0
    bad = {k: v for k, v in fam.items() if v[0] != v[1]}
    print("  families with an unfinished run: %s" % bad)


def q2():
    print("### Q3c-2 the commitment and the brake\n")
    viol = skip = 0
    lf = 0
    n = 0
    for cell, arm, s, r in all_runs():
        if "gauges" not in r:
            continue
        n += 1
        viol += r["gauges"].get("commit_viol", 0)
        lf += r["late_floor"]
        skip += r["gauges"].get("commit_skip", 0)
    print("runs %d: commit_viol total %d; late_floor total %d; commit_skip (candidates skipped below the floor) total %d" % (n, viol, lf, skip))
    for cell in S.ALL_CELLS:
        w = [(arm, s) for c, arm, s, r in all_runs() if c == cell and r["late_floor"] != 0]
        if w:
            print("  late_floor != 0 in %s: %s" % (cell, w[:6]))
    print("\nqueue_drop_out per arm in CL and OV (sum over 12 seeds), queue_drop_in (mean per seed), max copies in a 5 ms window (max over seeds), resend_braked mean:")
    for cell in ("CL", "OV"):
        for a in sorted({arm for c, arm, s, r in all_runs() if c == cell and not arm.startswith(("NT", "NH", "SW", "PC", "AA"))}):
            ss = S.seeds_of(cell, a)
            qo = sum(S.R(cell, a, s)["queue_drop_out"] for s in ss)
            qi = np.mean([S.R(cell, a, s)["queue_drop_in"] for s in ss])
            mx = max(S.R(cell, a, s)["max_copies_5ms"] for s in ss)
            br = np.mean([S.R(cell, a, s)["resend_braked"] for s in ss])
            print("  %-4s %-14s queue_drop_out %6d  queue_drop_in %9.1f  max copies/5ms %2d  resend_braked %9.1f" % (cell, a, qo, qi, mx, br))
    allmax = max(r["max_copies_5ms"] for c, a, s, r in all_runs() if c in ("CL", "OV"))
    print("overall max copies in any 5 ms window over every CL and OV run: %d" % allmax)


def sup():
    print("### unregistered supplement: the cooldown also binds a seq's first resend (resend_gate cooldown_all), link cells, 12 seeds\n")
    print("| cell | arm | commit_viol (sum) | late_floor (sum) | share_by_0p4 | share_by_2 | registered arm's share_by_0p4 / share_by_2 | max copies/5ms | finished |")
    print("|---|---|---|---|---|---|---|---|---|")
    pairs = {"SUP-C3": "C3", "SUP-C1": "C1", "SUP-S4b": "S4b"}
    for cell in ("OV", "CL"):
        for arm in sorted({a for c, a, s, r in all_runs(True) if c == cell}):
            ss = S.seeds_of(cell, arm)
            ref = pairs.get(arm, arm[4:] if arm != "SUP-G-Tinf-Hinf" else "C3S")
            rs_ = [S.R(cell, arm, s) for s in ss]
            print("| %s | %s | %d | %d | %.4f | %.4f | %.4f / %.4f | %d | %s |" % (
                cell, arm, sum(r["gauges"]["commit_viol"] for r in rs_), sum(r["late_floor"] for r in rs_), np.mean([r["share_by_0p4"] for r in rs_]),
                np.mean([r["share_by_2"] for r in rs_]), m_(cell, ref, "share_by_0p4"), m_(cell, ref, "share_by_2"), max(r["max_copies_5ms"] for r in rs_),
                all(r["finished"] for r in rs_)))


def q3():
    print("### Q3c-3 the collapse fixed (OV, T = 0.4)\n")
    keys = ("share_any", "share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99", "intake_expired", "intake_expired_deq", "link_util_w",
            "queue_drop_out", "resend_braked", "F_lag", "floor_hull_bind", "pass_floor")
    tab("OV", ["C1", "C3", "C3S", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S4b"], keys)
    a, b, c = m_("OV", G(0.4, 0.0), "share_by_0p4"), m_("OV", G(0.4, INF), "share_by_0p4"), m_("OV", "S4b", "share_by_0p4")
    print("\nS4 H=0 share_by_0p4 %.4f in [0.78, 0.83]: %s" % (a, band(a, 0.78, 0.83)))
    print("S4 H=inf share_by_0p4 %.4f in [0.75, 0.83]: %s; at least H=0 - 0.03 = %.4f: %s" % (b, band(b, 0.75, 0.83), a - 0.03, "PASS" if b >= a - 0.03 else "FAIL"))
    pred = b * (1 - 500.0 / 10000.0)
    print("S4b H=inf share_by_0p4 %.4f; S4(H=inf) x 0.95 = %.4f; within +-0.02: %s; queue_drop_out sum %d" % (
        c, pred, "PASS" if abs(c - pred) <= 0.02 else "FAIL", sum(S.R("OV", "S4b", s)["queue_drop_out"] for s in S.seeds_of("OV", "S4b"))))
    for nm in (G(0.4, INF), "S4b"):
        fl = [S.R("OV", nm, s)["gauges"]["floors"] / 10.0 for s in S.seeds_of("OV", nm)]
        fc = [S.R("OV", nm, s)["gauges"]["floor_carried"] / 10.0 for s in S.seeds_of("OV", nm)]
        print("  floor slots per second on the link (floor datagrams / 10 s) for %s: mean %.1f (min %.1f, max %.1f); header-carried floors per second %.0f" % (
            nm, np.mean(fl), min(fl), max(fl), np.mean(fc)))
    v = S.vals("OV", G(0.4, INF), "share_by_0p4")
    print("S4 H=inf per seed min/max %.4f %.4f; H=0 %.4f %.4f" % (v.min(), v.max(), S.vals("OV", G(0.4, 0.0), "share_by_0p4").min(), S.vals("OV", G(0.4, 0.0), "share_by_0p4").max()))
    d = S.diffv("OV", G(0.4, INF), G(0.4, 0.0), "share_by_0p4")
    print("paired H=inf minus H=0: %s" % S.fmt(S.read(d, S.bar("share_by_0p4", "OV"))))


def q4():
    print("### Q3c-4 the limit (OV, T = 2)\n")
    hi, h0 = m_("OV", G(2.0, INF), "share_by_2"), m_("OV", G(2.0, 0.0), "share_by_2")
    print("T=2 H=inf share_by_2 %.4f in [0.65, 0.85]: %s" % (hi, band(hi, 0.65, 0.85)))
    print("T=2 H=0 share_by_2 %.4f in [0.90, 0.99]: %s" % (h0, band(h0, 0.90, 0.99)))
    for a in (G(2.0, INF), G(2.0, 0.0)):
        print("  %s: dh_p50 %s dh_p90 %s share_by_0p4 %s; per-seed share_by_2 min/max %.4f %.4f" % (
            a, f(m_("OV", a, "dh_p50")), f(m_("OV", a, "dh_p90")), f(m_("OV", a, "share_by_0p4")), S.vals("OV", a, "share_by_2").min(), S.vals("OV", a, "share_by_2").max()))


def q5():
    print("### Q3c-5 fair baselines (OV)\n")
    for a in ("C3", "C3S"):
        v = m_("OV", a, "share_by_0p4")
        print("%s share_by_0p4 %.4f in [0.05, 0.20]: %s" % (a, v, band(v, 0.05, 0.20)))
    v = m_("OV", G(INF, 0.0), "share_by_0p4")
    print("S4 T=inf H=0 share_by_0p4 %.4f in [0.12, 0.21]: %s" % (v, band(v, 0.12, 0.21)))
    print("C1 share_by_0p4 %.4f (reported)" % m_("OV", "C1", "share_by_0p4"))
    print("\nlink utilisation over [1, 10] s per arm (min over seeds; the cell is valid for an arm if >= 0.98):")
    names = sorted({arm for c, arm, s, r in all_runs() if c == "OV" and not arm.startswith(("NT", "NH", "SW", "PC", "AA", "REP"))})
    for a in names:
        u = S.vals("OV", a, "link_util_w")
        print("  %-14s min %.4f mean %.4f  -> %s" % (a, u.min(), u.mean(), "valid" if u.min() >= 0.98 else "INVALID for this arm"))


def q6():
    print("### Q3c-6 the drain gain (CL)\n")
    keys = ("share_any", "share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99", "queue_drop_in", "queue_drop_out", "intake_expired",
            "intake_expired_deq", "link_util_w", "resend_braked", "pass_floor")
    tab("CL", ["C1", "C3", "C3S", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S4b"], keys)
    c3a, c3b = m_("CL", "C3", "share_by_0p4"), m_("CL", "C3", "share_by_2")
    print("\nC3 share_by_0p4 %.4f in [0.25, 0.47]: %s; share_by_2 %.4f >= 0.95: %s" % (c3a, band(c3a, 0.25, 0.47), c3b, "PASS" if c3b >= 0.95 else "FAIL"))
    h0, hi = m_("CL", G(0.4, 0.0), "share_by_0p4"), m_("CL", G(0.4, INF), "share_by_0p4")
    print("S4 T=0.4: H=0 %.4f, H=inf %.4f, both in [0.78, 0.90]: %s; |H=inf - H=0| = %.4f <= 0.02: %s" % (
        h0, hi, "PASS" if 0.78 <= h0 <= 0.90 and 0.78 <= hi <= 0.90 else "FAIL", abs(hi - h0), "PASS" if abs(hi - h0) <= 0.02 else "FAIL"))
    for H in (0.0, INF):
        for k in ("share_by_0p4", "share_by_2"):
            d = S.diffv("CL", G(2.0, H), "C3S" if H == INF else G(INF, H), k)
            print("  S4 T=2 minus T=inf at H=%s on %s: %s" % (f(H), k, S.fmt(S.read(d, S.bar(k, "CL")))))
    for H in (0.0, INF):
        d = S.diffv("CL", G(0.4, H), "C3", "share_by_0p4")
        rd = S.read(d, S.bar("share_by_0p4", "CL"))
        print("  S4 T=0.4 H=%s minus C3 on share_by_0p4: %s; larger by more than 0.25: %s" % (f(H), S.fmt(rd), "PASS" if rd["word"].startswith("shown larger") and rd["lo"] > 0.25 else "FAIL (mean %.4f, lo %.4f)" % (rd["mean"], rd["lo"])))


def crossing(cell, name_of, grid, ref_hi, key="share_T"):
    y = np.array([S.mean(cell, name_of(T), key) for T in grid])
    lo, hi = y[0], S.mean(cell, ref_hi, key)

    def cross(level):
        for i in range(1, len(grid)):
            if y[i] >= level > y[i - 1] or (y[i] >= level and i == 1 and y[0] >= level):
                if y[i] == y[i - 1]:
                    return grid[i]
                return grid[i - 1] + (level - y[i - 1]) * (grid[i] - grid[i - 1]) / (y[i] - y[i - 1])
        return float("nan")
    return dict(lo=lo, hi=hi, mid=cross(0.5 * (lo + hi)), p90=cross(0.9 * hi), y=y)


def q8():
    print("### Q3c-8 crossover (S4, old sweep, H = 0)\n")
    import batch3c
    res = {}
    for cell in ("X15", "X20", "K-L10", "K-L10b"):
        grid = batch3c.SW[cell]
        if not all(S.seeds_of(cell, "SW-%s" % S.TAG(T)) for T in grid):
            print(cell, "sweep incomplete")
            continue
        hi_arm = G(0.4, 0.0)
        c = crossing(cell, lambda T, cell=cell: "SW-%s" % S.TAG(T), grid, hi_arm)
        res[cell] = c
        ages = [S.R(cell, hi_arm, s)["rs_age_report_med"] for s in S.seeds_of(cell, hi_arm)]
        ages = [x for x in ages if x is not None]
        med_age = float(np.median(ages)) if ages else float("nan")
        sr = np.mean([S.R(cell, hi_arm, s)["srtt_mean"] for s in S.seeds_of(cell, hi_arm)])
        # alternative: pool the sweep runs near the crossing
        pooled = []
        for T in grid:
            pooled += [S.R(cell, "SW-%s" % S.TAG(T), s)["rs_age_report_med"] for s in S.seeds_of(cell, "SW-%s" % S.TAG(T))]
        pooled = [x for x in pooled if x is not None]
        per = []
        for sd in S.seeds_of(cell, hi_arm):
            yy = np.array([S.flat(S.R(cell, "SW-%s" % S.TAG(T), sd), "share_T") for T in grid])
            lo_, hi_ = yy[0], S.flat(S.R(cell, hi_arm, sd), "share_T")
            lev = 0.5 * (lo_ + hi_)
            cr = float("nan")
            for i in range(1, len(grid)):
                if yy[i] >= lev > yy[i - 1]:
                    cr = grid[i - 1] + (lev - yy[i - 1]) * (grid[i] - grid[i - 1]) / (yy[i] - yy[i - 1])
                    break
            per.append(cr)
        print("- %s: share_T at lowest T %.4f, at T=0.4 %.4f; mid-level crossing %.4f s (per-seed %s); 90%% crossing %s; median first-resend age (report-driven, T=0.4 H=0 run, median over seeds) %.4f; srtt_mean/2 %.4f; sum %.4f s (crossing - sum = %+.1f ms); pooled sweep-run median age %.4f" % (
            cell, c["lo"], c["hi"], c["mid"], " ".join(f(x, 3) for x in per), f(c["p90"]), med_age, sr / 2, med_age + sr / 2, 1e3 * (c["mid"] - med_age - sr / 2),
            float(np.median(pooled)) if pooled else float("nan")))
        print("  curve: " + " ".join("%.3f:%.4f" % (T, v) for T, v in zip(grid, c["y"])))
        lowT = "SW-%s" % S.TAG(grid[0])
        print("  at the lowest T: repair_sends per seed %s" % [S.R(cell, lowT, s)["gauges"]["repair_sends"] for s in S.seeds_of(cell, lowT)])
    for a, b, lo, hi in (("X15", "X20", 11, 19), ("K-L10", "K-L10b", 25, 35)):
        if a in res and b in res:
            sh = 1e3 * (res[b]["mid"] - res[a]["mid"])
            print("- shift %s -> %s (mid): %.1f ms (band %d to %d): %s; 90%%: %s ms" % (a, b, sh, lo, hi, band(sh, lo, hi), f(1e3 * (res[b]["p90"] - res[a]["p90"]))))


def q9():
    print("### Q3c-9 continuity (K and P at the nudges; m = max(T bar, A/A floor))\n")
    shown = und = tot = 0
    for cell in S.ALL_CELLS:
        if cell in ("X15", "X20"):
            continue
        for which in ("T", "H"):
            names = ["NT-%s" % S.TAG(x) if x != 1.0 else G(0.4, 0.02) for x in S.NUDGE] if which == "T" else \
                    ["NH-%s" % S.TAG(x) if x != 1.0 else G(0.4, 0.02) for x in S.NUDGE]
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
                        print("  SHOWN %s %s-nudge %s %s: %s" % (cell, which, k, st, S.fmt(rd)))
                    elif rd["word"].startswith("undet"):
                        und += 1
                        print("  undetermined %s %s-nudge %s %s: %s" % (cell, which, k, st, S.fmt(rd)))
    print("  %d K and P statistics: %d shown, %d undetermined, %d equivalent" % (tot, shown, und, tot - shown - und))
    print("\nper-seed positive control in E-I2n:")
    lit = json.load(open(os.path.join(S.HERE, "pc_stage3c.json")))
    print("  literal placement (dead-tail units = every seq >= dead[0]): usable %d of 12; first reason: %s" % (lit["usable"], lit["skipped"][0][1][:70]))
    pc = json.load(open(os.path.join(S.HERE, "pc_stage3c_dev.json")))
    cell = "E-I2n"
    names = ["PC-%s" % S.TAG(x) for x in S.NUDGE]
    use = sorted(int(x) for x in pc[cell])
    print("  deviation placement (dead-tail units = first send inside the outage): usable %d of 12: %s" % (len(use), use))
    for sk in pc["skipped"]:
        print("     skipped seed %d: %s" % (sk[0], sk[1]))
    print("  at least 8 of 12 seeds usable: %s -> %s" % ("yes" if len(use) >= 8 else "NO", "control placed" if len(use) >= 8 else "control NOT PLACED (a finding)"))
    if not use or not all(len(S.seeds_of(cell, n)) == len(use) for n in names):
        return
    for sd in use:
        v = pc[cell][str(sd)]
        print("   seed %2d p = %.3f, ages in [%.4f, %.4f] (%d units), K-hat %.5f P-hat %.5f" % (sd, v["p"], v["dmin"], v["dmax"], v["n"], v["Khat"], v["Phat"]))
    kk = S.kp(cell, names, "share_T")
    m = S.bar("share_T", cell)
    Kh = np.mean([pc[cell][str(s)]["Khat"] for s in use])
    Ph = np.mean([pc[cell][str(s)]["Phat"] for s in use])
    print("  descriptive (n = %d seeds): mean K %.5f vs mean K-hat %.5f (ratio %.2f); read: %s" % (len(use), kk["K"].mean(), Kh, kk["K"].mean() / Kh, S.fmt(S.read(kk["K"], m))))
    print("  mean P %.5f vs mean P-hat %.5f (ratio %.2f); read: %s" % (kk["P"].mean(), Ph, kk["P"].mean() / Ph, S.fmt(S.read(kk["P"], m))))
    print("  share_T at the five points: %s" % [round(S.mean(cell, n, "share_T"), 5) for n in names])


def q7():
    print("### Q3c-7 no loss of savable data (K cells, T = 0.4, H = inf)\n")
    import batch3
    import toy2
    batch3.build()
    for cell in ("K-I2", "K-L10"):
        d3 = os.path.join(S.HERE, "out3", "ID")
        vals3 = []
        for s in range(12):
            fn = os.path.join(d3, "%s.G-T0p4-Hinf-%d.json" % (cell, s))
            if not os.path.exists(fn):
                vals3 = None
                break
            vals3.append(json.load(open(fn))["share_by_0p4"])
        if vals3 is None:
            print("  %s: stage 3 S2 outputs absent in out3 (regenerate with batch3.py --part K)" % cell)
            continue
        chk = batch3.run_one(batch3.JOBS["%s.G-T0p4-Hinf" % cell][1](0))["share_by_0p4"]
        s4 = S.vals(cell, G(0.4, INF), "share_by_0p4")
        m = S.bar("share_by_0p4", cell)
        diff = s4 - np.asarray(vals3)
        print("- %s: stage 3 S2 (T=0.4, H=inf) share_by_0p4 mean %.4f (seed 0 re-run %.6f, stored %.6f: %s); S4 %.4f; paired S4 - S2 %s; m = %.5f; S4 >= S2 - m: %s" % (
            cell, np.mean(vals3), chk, vals3[0], "match" if chk == vals3[0] else "MISMATCH", s4.mean(), S.fmt(S.read(diff, m)), m, "PASS" if diff.mean() >= -m else "FAIL"))
        print("  F_lag (mean over sends of n_sent - F_ann): S4 T=0.4 H=inf %.3f; floor_hull_bind %.4f; pass_floor %.1f; commit_skip %.1f" % (
            m_(cell, G(0.4, INF), "F_lag"), m_(cell, G(0.4, INF), "floor_hull_bind"), m_(cell, G(0.4, INF), "pass_floor"), gauge(cell, G(0.4, INF), "commit_skip")))
    print("- S4b in K-L10: share_by_0p4 %.4f, floors %.1f" % (m_("K-L10", "S4b", "share_by_0p4"), gauge("K-L10", "S4b", "floors")))


def q10():
    print("### Q3c-10 stall parts (K cells; S4 at T = 0.4, H = inf; C3 reported)\n")
    import batch3c
    for cell in ("K-I2", "K-L10", "E-I2", "E-L10", "M-I2", "M-L10", "CL"):
        lc, d = batch3c.base(cell)
        owd, rate = d["owd"], d.get("rate", 10000.0)
        P = min(max(4 * owd, 0.025), 0.1)
        for a in (G(0.4, INF), "C3"):
            n_ep = 0
            print("- %s %s (P = %.3f, owd = %.3f; bound (i) = P + owd + 5 ms = %.3f; bound (o) = owd + 1/rate + 2 ms = %.4f)" % (cell, a, P, owd, P + owd + 0.005, owd + 1.0 / rate + 0.002))
            ok_i, ok_o = [], []
            for s in S.seeds_of(cell, a):
                pt = S.R(cell, a, s)["stall3c"]
                if pt is None:
                    print("   seed %2d: no outage episode" % s)
                    continue
                n_ep += 1
                print("   seed %2d: episode %.3f + %.3f s; (o) %.4f (oo) %.4f (i) %.4f (ii) %.4f (iii) %.4f%s" % (
                    s, pt["start"], pt["length"], pt["o"], pt["oo"], pt["i"], pt["ii"], pt["iii"], "" if pt["floor_seen"] else "  [no floor seen after the outage]"))
                ok_i.append(pt["i"] <= P + owd + 0.005)
                ok_o.append(pt["o"] == 0.0 or pt["o"] <= owd + 1.0 / rate + 0.002)
            print("   seeds with an outage episode %d; (i) within bound %d; (o) within bound where non-zero %d" % (n_ep, sum(ok_i), sum(ok_o)))


def tables():
    print("### per-cell table (seed means)\n")
    keys = ("share_any", "share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99", "ooo_per_delivered", "repair_per_delivered", "stall_total", "verdicts", "pass_floor", "resend_braked")
    for cell in STAGE + ("K-L10b", "E-I2n", "CL", "OV"):
        print("#### %s\n" % cell)
        tab(cell, ARMS, keys)
        print()


def aa():
    print("### A/A self-check (frozen nf_stage3c.json)\n")
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
    print("\nfloors (cell: readout = p95 |A - A'|, ms for times):")
    for cell, d in nf["cells"].items():
        print("  %-7s %s" % (cell, "; ".join("%s=%s" % (k, f(v, 3)) for k, v in d.items() if v not in (0.0,))))


def rep():
    print("### A/A replication, phase variants 2 and 3 (unregistered), read against T_bar only\n")
    shown = tot = 0
    for cell in S.ALL_CELLS:
        if not (S.have(cell, "AA-2") and S.have(cell, "AA-3")):
            continue
        for k in S.AA_KEYS:
            d = S.diffv(cell, "AA-2", "AA-3", k)
            rd = S.read(d, S.T_bar(k, cell))
            tot += 1
            if rd["word"].startswith("shown"):
                shown += 1
                print("  SHOWN %s %s: %s" % (cell, k, S.fmt(rd)))
    print("  %d readings, %d shown" % (tot, shown))


def main(cmd):
    {"q1": q1, "q2": q2, "q3": q3, "q4": q4, "q5": q5, "q6": q6, "q7": q7, "q8": q8, "q9": q9, "q10": q10, "tables": tables, "aa": aa, "rep": rep, "sup": sup}[cmd]()
