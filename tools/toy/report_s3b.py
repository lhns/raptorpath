"""Stage-3b readings (three-knobs.md 13.7.5).  Called from summarize3b.py:  python summarize3b.py q2|q3|q4|q5|q6|q7|q8|q9|tables
Every function prints the numbers behind one prediction; the PASS/FAIL words are written into results-stage3b.md by hand from
these numbers, against the registered bands, never tuned."""
import json
import math
import os

import numpy as np

import summarize3b as S

INF = math.inf
G = S.gname
STAGE = ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10")
RK = ("share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "ooo_per_delivered", "repair_per_delivered")


def f(x, nd=4):
    return "nan" if x is None or (isinstance(x, float) and math.isnan(x)) else ("inf" if x == INF else "%.*g" % (nd, x))


def arms_s3(cell):
    """Names of every S3-family arm that ran in a cell (verdict-carrying arms)."""
    out = set()
    for (n, s) in S.load():
        c, _, a = n.partition(".")
        if c == cell and (a.startswith(("G-", "NT-", "NH-", "SW-", "PC-", "S3x")) or a in ("C3R", "AA-1") or a.startswith("ID-")):
            out.add(a)
    return sorted(out)


def q2():
    print("### Q3b-2 D1 fixed\n")
    print("vta <= 0.001 x verdicts for S3 in every (cell, arm, seed) with verdicts > 100:")
    for label, pick in (("S3 (every S3 arm)", lambda a: a != "S3x"), ("S3x (the exclusion arm, not part of the prediction)", lambda a: a == "S3x")):
        tot = viol = 0
        worst = 0.0
        cells_hit = set()
        for cell in S.ALL_CELLS:
            for a in arms_s3(cell):
                if a.startswith("ID-") or a == "AA-1" or not pick(a):
                    continue
                for s in S.seeds_of(cell, a):
                    r = S.R(cell, a, s)
                    if r["verdicts"] > 100:
                        tot += 1
                        cells_hit.add(cell)
                        ratio = r["vta"] / r["verdicts"]
                        worst = max(worst, ratio)
                        if r["vta"] > 0.001 * r["verdicts"]:
                            viol += 1
                            print("  VIOLATION", cell, a, s, r["vta"], r["verdicts"])
        print("  %s: %d (cell, arm, seed) runs with verdicts > 100 in %d cells; %d violate; worst vta/verdicts = %.2e" % (label, tot, len(cells_hit), viol, worst))
    print("\nOV: S3 pause_time against C3's (within 5 %), throttle, S2 against S3 (verdicts > 10x, vta > 0.5 x own verdicts):")
    c3 = S.mean("OV", "C3", "pause_time")
    c3w = S.mean("OV", "C3", "pause_time_w")
    for a in ("C3", "C1", "C3R", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S3x", "S2-T0p4-H0"):
        if not S.seeds_of("OV", a):
            continue
        thr = [S.R("OV", a, s)["throttle"] for s in S.seeds_of("OV", a)]
        pt = S.mean("OV", a, "pause_time")
        ptw = S.mean("OV", a, "pause_time_w")
        print("  %-14s pause_time %.3f s (%.3f of C3's), in [1,10] s %.3f s (%.3f of C3's), throttle link/pause/none = %d/%d/%d, verdicts %s, vta %s, util_w %.3f, queue_drop %s" % (
            a, pt, pt / c3 if c3 else float("nan"), ptw, ptw / c3w if c3w else float("nan"), thr.count("link"), thr.count("pause"), thr.count("none"),
            f(S.mean("OV", a, "verdicts")), f(S.mean("OV", a, "vta")), S.mean("OV", a, "link_util_w"), f(S.mean("OV", a, "queue_drop"))))


def tab(cell, arms, keys, scale=None):
    print("| arm | " + " | ".join(keys) + " |")
    print("|---|" + "---|" * len(keys))
    for a in arms:
        if not S.seeds_of(cell, a):
            continue
        print("| %s | " % a + " | ".join(f(S.mean(cell, a, k)) for k in keys) + " |")


def q3():
    print("### Q3b-3 overload (OV)\n")
    keys = ("share_any", "share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99", "intake_expired", "intake_expired_deq", "link_util_w", "pause_time", "queue_drop")
    tab("OV", ["C1", "C3", "C3R", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S3x", "S2-T0p4-H0"], keys)
    for a in ("C1", "C3", G(0.4, 0.0), G(2.0, INF), G(2.0, 0.0)):
        if S.seeds_of("OV", a):
            v = S.vals("OV", a, "share_by_0p4")
            print("  %s share_by_0p4 per seed min/max %.4f %.4f; share_by_2 %.4f %.4f; dh_p50 ms min/max %s %s" % (
                a, v.min(), v.max(), S.vals("OV", a, "share_by_2").min(), S.vals("OV", a, "share_by_2").max(),
                f(S.vals("OV", a, "dh_p50").min()), f(S.vals("OV", a, "dh_p50").max())))


def q4():
    print("### Q3b-4 drain gain (CL)\n")
    keys = ("share_any", "share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99", "queue_drop_in", "queue_drop_out", "intake_expired", "intake_expired_deq",
            "link_util_w", "pause_time", "probes", "verdicts", "vta")
    tab("CL", ["C1", "C3", "C3R", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S3x", "S2-T0p4-H0"], keys)
    for k in ("share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99"):
        for a, b, lab in ((G(0.4, INF), "C3", "S3 T=0.4 H=inf - C3"), (G(0.4, 0.0), "C3", "S3 T=0.4 H=0 - C3"),
                          (G(2.0, INF), "C3", "S3 T=2 H=inf - C3"), (G(2.0, 0.0), "C3", "S3 T=2 H=0 - C3")):
            if S.seeds_of("CL", a) and S.seeds_of("CL", b):
                d = S.diffv("CL", a, b, k)
                print("  %-22s %-13s %s" % (lab, k, S.fmt(S.read(d, S.bar(k, "CL")))))
    for a in ("C1", "C3", "C3R", G(0.4, 0.0), G(0.4, INF), G(2.0, INF), "S3x", "S2-T0p4-H0"):
        if S.seeds_of("CL", a):
            qs = [S.R("CL", a, s)["queue_drop_out"] for s in S.seeds_of("CL", a)]
            qi = [S.R("CL", a, s)["queue_drop_in"] for s in S.seeds_of("CL", a)]
            print("  %-12s queue_drop_out per seed: %s ; queue_drop_in mean %.1f" % (a, qs, np.mean(qi)))


def crossing(cell, name_of, grid, ref_hi, key="share_T"):
    """Mid-level crossing (13.7.4): the smallest T on the grid, linearly interpolated, at which the seed-mean share_T reaches the
    midpoint of its value at the lowest T and its value at T = 0.4 (ref_hi arm).  90 % reading: 90 % of the T = 0.4 value."""
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


def q5():
    print("### Q3b-5 crossover\n")
    res = {}
    for cell, hi_arm in (("X15", G(0.4, 0.0)), ("X20", G(0.4, 0.0)), ("K-L10", G(0.4, 0.0)), ("K-L10b", G(0.4, 0.0))):
        grid = S.__dict__.get("SWGRID", None)
        import batch3b
        grid = batch3b.SW[cell]
        if not all(S.seeds_of(cell, "SW-%s" % S.TAG(T)) for T in grid):
            print(cell, "sweep incomplete")
            continue
        c = crossing(cell, lambda T, cell=cell: "SW-%s" % S.TAG(T), grid, hi_arm)
        res[cell] = c
        r_mean = np.mean([S.R(cell, "SW-%s" % S.TAG(T), s)["R_mean"] for T in grid for s in range(12)])
        sr_mean = np.mean([S.R(cell, "SW-%s" % S.TAG(T), s)["srtt_mean"] for T in grid for s in range(12)])
        # per-seed mid crossings
        per = []
        for sd in range(12):
            yy = np.array([S.flat(S.R(cell, "SW-%s" % S.TAG(T), sd), "share_T") for T in grid])
            lo, hi = yy[0], S.flat(S.R(cell, hi_arm, sd), "share_T")
            lev = 0.5 * (lo + hi)
            cr = float("nan")
            for i in range(1, len(grid)):
                if yy[i] >= lev > yy[i - 1]:
                    cr = grid[i - 1] + (lev - yy[i - 1]) * (grid[i] - grid[i - 1]) / (yy[i] - yy[i - 1])
                    break
            per.append(cr)
        print("- %s: share_T at lowest T %.4f, at T=0.4 %.4f; mid-level crossing %.4f s (per-seed %s); 90%% crossing %s; R_mean %.4f, srtt_mean/2 %.4f, R+srtt/2 = %.4f" % (
            cell, c["lo"], c["hi"], c["mid"], " ".join(f(x, 3) for x in per), f(c["p90"]), r_mean, sr_mean / 2, r_mean + sr_mean / 2))
        print("  curve: " + " ".join("%.3f:%.4f" % (T, v) for T, v in zip(grid, c["y"])))
        lowT = "SW-%s" % S.TAG(grid[0])
        print("  at the lowest T: resend_by_R per seed %s, resend_by_report %s" % (
            [S.R(cell, lowT, s)["resend_by_R"] for s in range(12)], [S.R(cell, lowT, s)["resend_by_report"] for s in range(12)]))
    for a, b in (("X15", "X20"), ("K-L10", "K-L10b")):
        if a in res and b in res:
            print("- shift %s -> %s (mid): %.1f ms; 90%%: %s ms" % (a, b, 1e3 * (res[b]["mid"] - res[a]["mid"]), f(1e3 * (res[b]["p90"] - res[a]["p90"]))))
    print("\nStage 3 arm S2 (old sweep) on the K-L10 and K-L10b grids:")
    import batch3b
    r2 = {}
    for cell in ("K-L10", "K-L10b"):
        grid = batch3b.SW[cell]
        if not all(S.seeds_of(cell, "SW2-%s" % S.TAG(T)) for T in grid):
            print(cell, "S2 sweep incomplete")
            continue
        c = crossing(cell, lambda T, cell=cell: "SW2-%s" % S.TAG(T), grid, "S2-T0p4-H0")
        r2[cell] = c
        print("- %s: S2 mid crossing %.4f s; 90%% %s; curve %s" % (cell, c["mid"], f(c["p90"]), " ".join("%.3f:%.4f" % (T, v) for T, v in zip(grid, c["y"]))))
    if len(r2) == 2:
        print("- S2 shift K-L10 -> K-L10b (mid): %.1f ms" % (1e3 * (r2["K-L10b"]["mid"] - r2["K-L10"]["mid"])))


def q6():
    print("### Q3b-6 continuity (K and P at the nudges; m = max(T bar, A/A floor))\n")
    shown = und = tot = 0
    for cell in STAGE + ("CL",):
        for which, names in (("T", ["NT-%s" % S.TAG(x) if x != 1.0 else G(0.4, 0.02) for x in S.NUDGE]),
                             ("H", ["NH-%s" % S.TAG(x) if x != 1.0 else G(0.4, 0.02) for x in S.NUDGE])):
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


def q7():
    print("### Q3b-7 positive control\n")
    pc = json.load(open(S.os.path.join(S.HERE, "pc_stage3b.json")))
    for cell in ("E-I2n", "E-I2", "E-L10"):
        names = ["PC-%s" % S.TAG(x) for x in S.NUDGE]
        if cell not in pc or not all(S.have(cell, n) for n in names):
            print(cell, "not complete")
            continue
        p = pc[cell]
        kk = S.kp(cell, names, "share_T")
        m = S.bar("share_T", cell)
        print("- %s: p = %.3f s, IQR w = %.4f s (%.2f %% of p) -> case %s; share_T at the five points %s" % (
            cell, p["p"], p["w"], 100 * p["w"] / p["p"], p["case"], [round(S.mean(cell, n, "share_T"), 5) for n in names]))
        for st in ("K", "P", "J"):
            print("    %s on share_T: %s" % (st, S.fmt(S.read(kk[st], m))))


def q8():
    print("### Q3b-8 brake cost (S3x against S3, T = 0.4, H = inf)\n")
    import batch3b
    for cell in ("M-I2", "M-L10", "CL", "OV"):
        a, b = "S3x", G(0.4, INF)
        if not (S.seeds_of(cell, a) and S.seeds_of(cell, b)):
            continue
        for k in ("share_by_0p4", "share_by_2", "dh_p50", "dh_p99", "pause_time", "resume_delay", "intake_expired", "intake_expired_deq"):
            d = S.diffv(cell, a, b, k)
            print("  %-6s S3x - S3 %-16s %s | S3 %s S3x %s" % (cell, k, S.fmt(S.read(d, S.bar(k, cell))), f(S.mean(cell, b, k)), f(S.mean(cell, a, k))))
        if cell != "OV":
            lc, d = batch3b.base(cell)
            owd = d["owd"]
            P = min(max(4 * owd, 0.025), 0.1)
            Rm = S.mean(cell, b, "R_mean") / 1e3
            print("    bound for S3's resume delay: R_mean %.4f + P %.3f + 2 RTT %.3f = %.4f s ; S3 resume_delay per seed (s): %s" % (
                Rm, P, 4 * owd, Rm + P + 4 * owd, [f(S.R(cell, b, s)["resume_delay"], 3) for s in range(12)]))
            print("    S3x resume_delay per seed (s): %s" % [f(S.R(cell, a, s)["resume_delay"], 3) for s in range(12)])


def q9():
    print("### Q3b-9 strict stall decomposition (K cells, S3 R, T = 0.4, H = inf)\n")
    import batch3b
    for cell in ("K-I2", "K-L10"):
        a = G(0.4, INF)
        lc, d = batch3b.base(cell)
        owd = d["owd"]
        P = min(max(4 * owd, 0.025), 0.1)
        rows = []
        for s in range(12):
            r = S.R(cell, a, s)
            rows.append((r["stall_max"], r["stall_parts"]))
        print("- %s: P = %.3f, owd = %.3f, bound for (i) = P + owd + 5 ms = %.3f s" % (cell, P, owd, P + owd + 0.005))
        for s, (mx, pt) in enumerate(rows):
            print("   seed %2d: longest stall %.3f s; parts (i, ii, iii) = %s" % (s, mx, None if pt is None else tuple(round(x, 3) for x in pt)))
        dfn = [(mx, pt) for mx, pt in rows if pt is not None]
        ok1 = all(pt[0] <= P + owd + 0.005 for _, pt in dfn)
        print("   parts defined (an episode open at the first floor after the outage) in %d of 12 seeds; (i) <= bound in all of them: %s (max (i) %.3f s)" % (
            len(dfn), ok1, max(pt[0] for _, pt in dfn)))
        if cell == "K-L10":
            ok2 = [pt[1] >= 0.5 * (mx - 1.5) for mx, pt in dfn]
            print("   (ii) >= 0.5 x (longest stall - 1.5 s): %d of %d seeds with parts; failing seeds' (mx, ii): %s" % (
                sum(ok2), len(ok2), [(round(mx, 3), round(pt[1], 3)) for (mx, pt), o in zip(dfn, ok2) if not o]))


def tables():
    print("### per-cell table (seed means)\n")
    keys = ("share_any", "share_by_0p4", "share_by_2", "dh_p50", "dh_p90", "dh_p99", "ooo_per_delivered", "repair_per_delivered", "stall_total", "verdicts", "vta", "probes")
    for cell in STAGE + ("E-I2n",):
        print("#### %s\n" % cell)
        tab(cell, ["C1", "C3", "C3R", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S3x", G(0.4, 0.02)], keys)
        print()


def main(cmd):
    {"q2": q2, "q3": q3, "q4": q4, "q5": q5, "q6": q6, "q7": q7, "q8": q8, "q9": q9, "tables": tables, "aa": aa, "rep": rep}[cmd]()


def aa():
    print("### A/A self-check (frozen nf_stage3b.json)\n")
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
