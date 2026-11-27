"""Markdown for results-stage3.md.  python summarize3.py read K|EM|O|tables"""
import math

import numpy as np

import summarize3 as S

p = print
tag, gname, INF = S.tag, S.gname, S.INF
CALL = ("K-I2", "K-L10")
OUTAGE = ("E-I2", "E-L10", "M-I2", "M-L10")
CELLS6 = CALL + OUTAGE
CTRL = ("share_any", "share_T", "v_mean", "v_p99", "ooo_per_delivered", "repair_per_delivered")
GRID = [(T, H) for T in (0.4, 2.0, INF) for H in (0.0, 0.02, INF)]


def rdiff(cell, a, b, k, m=None):
    d = S.diff(cell, a, b, k)
    mm = S.bar(k, cell) if m is None else m
    return S.read(d, mm)


def line(label, rd):
    p("- %s: %s" % (label, S.fmt(rd)))


def worst(words):
    if any(w.startswith("shown") for w in words):
        return "shown"
    if any(w == "undetermined" for w in words):
        return "undetermined"
    return "equivalent"


# ------------------------------------------------------------------------------------------------
def cell_table(cell):
    names = [("C1 today's receiver, rho = 0.99", "C1"), ("C2 R1, rho = 0.99", "C2"), ("C3 R1, rho = 1", "C3")]
    names += [("new T=%s H=%s" % ("inf" if T == INF else "%g" % T, "inf" if H == INF else "%g" % H), gname(T, H)) for T, H in GRID]
    cols = [("share_any", "any", 1, "%.4f"), ("share_by_0p4", "by 0.4", 1, "%.4f"), ("share_by_2", "by 2", 1, "%.4f"),
            ("w_mean", "w mean", 1, "%.1f"), ("w_p50", "w p50", 1, "%.1f"), ("w_p99", "w p99", 1, "%.0f"),
            ("v_mean", "v mean", 1, "%.1f"), ("v_p50", "v p50", 1, "%.1f"),
            ("ooo_per_delivered", "ooo/deliv", 1, "%.4f"), ("repair_per_delivered", "repair/deliv", 1, "%.4f"),
            ("floors_per_delivered", "floors/deliv", 1, "%.4f"), ("stall_total", "stall total s", 1e-3, "%.2f"),
            ("finish", "finish s", 1e-3, "%.3f")]
    p("#### %s (means over 12 seeds; shares are fractions of units handed over; w, v in ms)\n" % cell)
    p("| arm | " + " | ".join("share " + c[1] if c[0].startswith("share") else c[1] for c in cols) + " |")
    p("|---|" + "---|" * len(cols))
    for lab, a in names:
        row = []
        for k, _, sc, f in cols:
            v = S.mean(cell, a, k) * sc
            row.append(f % v)
        p("| %s | %s |" % (lab, " | ".join(row)))
    p("")
    extra = ["intake_expired", "l2_fail", "rel_by_hold", "rel_by_floor", "verdicts"]
    p("Reported gauges (mean per run) for the new arms:\n")
    p("| arm | intake expired | L2 verdicts | released by hold | released by floor | stalled runs |")
    p("|---|---|---|---|---|---|")
    for lab, a in names[3:] + [("S2c T=0.4 H=0", "S2c-H0"), ("S2c T=0.4 H=inf", "S2c-Hinf"), ("S2p T=0.4 H=inf", "S2p")]:
        ss = S.seeds_of(cell, a)
        st = sum(1 for s in ss if S.R(cell, a, s).get("finish") is None)
        p("| %s | %.1f | %.1f | %.1f | %.1f | %d |" % (lab, S.mean(cell, a, "intake_expired"), S.mean(cell, a, "l2_fail"),
                                                        S.mean(cell, a, "rel_by_hold"), S.mean(cell, a, "rel_by_floor"), st))
    p("")


def p3():
    p("### P3: continuity at the named points (K and P of each readout over the five nudge points; both nudge sets, six cells)\n")
    p("Bar m = max(T bar, A/A noise floor of the readout); the stage-3 A/A is a two-arm A/A, so NF_K = NF_P = the readout's floor (worker decision).\n")
    p("| cell | set | statistic | " + " | ".join(CTRL) + " |")
    p("|---|---|---|" + "---|" * len(CTRL))
    anyshown = []
    undet = 0
    for cell in CELLS6:
        for which in ("T", "H"):
            names5 = [("NT-%s" if which == "T" else "NH-%s") % tag(x) if x != 1.0 else gname(0.4, 0.02) for x in S.NUDGE]
            for stat in ("K", "P"):
                row = []
                for k in CTRL:
                    st = S.kp(cell, names5, k)[stat]
                    rd = S.read(st, S.bar(k, cell))
                    row.append("%s (%.3g, m=%.3g)" % (rd["word"], rd["mean"], rd["m"]))
                    if rd["word"].startswith("shown"):
                        anyshown.append((cell, which, stat, k, rd))
                    if rd["word"] == "undetermined":
                        undet += 1
                p("| %s | %s nudges | %s | %s |" % (cell, which, stat, " | ".join(row)))
    p("")
    p("**P3 reading: %s.** Shown: %d; undetermined: %d of %d statistics.\n" % (
        "FAIL" if anyshown else "PASS", len(anyshown), undet, len(CELLS6) * 2 * 2 * len(CTRL)))
    for cell, which, stat, k, rd in anyshown:
        p("- SHOWN: %s %s-nudges %s of %s: %s" % (cell, which, stat, k, S.fmt(rd)))
    p("")


def p5():
    p("### P5: the round boundary is measured, not declared (K cells, H = 0)\n")
    res = {}
    for cell in ("K-L10", "K-L10b", "K-I2"):
        Ts = sorted({float(n.split("SW-")[1].replace("p", ".")) for (n, s) in S.load() if n.startswith(cell + ".SW-")})
        sh = {T: S.vals(cell, "SW-%s" % tag(T), "share_T") for T in Ts}
        rp = {T: S.vals(cell, "SW-%s" % tag(T), "repair_per_delivered") for T in Ts}
        res[cell] = (Ts, sh, rp)
        m = S.bar("share_T", cell if cell != "K-L10b" else "K-L10")
        p("**%s** (owd %s ms; m = %.4g)\n" % (cell, "60" if cell == "K-L10b" else ("50" if "L10" in cell else "5"), m))
        p("| T | share_T | repair per delivered | floors per delivered |")
        p("|---|---|---|---|")
        for T in Ts:
            p("| %g | %.4f | %.4f | %.4f |" % (T, sh[T].mean(), rp[T].mean(), S.mean(cell, "SW-%s" % tag(T), "floors_per_delivered")))
        mono = all(float(np.mean(sh[b] - sh[a])) >= -m for a, b in zip(Ts, Ts[1:]))
        mono_ci = [(a, b, S.read(sh[b] - sh[a], m)["word"]) for a, b in zip(Ts, Ts[1:])]
        p("")
        p("- non-decreasing (each adjacent mean difference >= -m): **%s**; adjacent readings: %s" % (
            "yes" if mono else "NO", ", ".join("%g->%g %s" % t for t in mono_ci if t[2] != "equivalent") or "all equivalent"))
    # below the first round
    p("")
    p1 = {"K-L10": 0.10, "K-I2": 0.03}
    first = {"K-L10": 0.14, "K-I2": 0.020}
    for cell in ("K-L10", "K-I2"):
        Ts, sh, rp = res[cell]
        pred = (1 - p1[cell]) * (1 - 75 / 3000)
        below = [T for T in Ts if T <= first[cell] + 1e-12]
        ok_rep = all(float(rp[T].max()) == 0.0 for T in below)
        ok_sh = all(abs(float(sh[T].mean()) - pred) <= 0.005 for T in below)
        p("- %s below the first round (T <= %g): repair per delivered exactly 0 for every seed: **%s**; share_T within 0.005 of %.4f: **%s** (means %s)" % (
            cell, first[cell], "yes" if ok_rep else "NO", pred, "yes" if ok_sh else "NO", ", ".join("%.4f" % sh[T].mean() for T in below)))
    # the rise: registered text "the rise to 90 % of the T = 0.4 value lies in ..." (reading A), the 90 % of the rise above the
    # lowest-T level (reading B), and a descriptive step location (not registered): where the curve crosses the mid-level
    p("")
    iv = {"K-L10": (0.16, 0.20), "K-I2": (0.03, 0.05)}
    cross = {}
    for cell in ("K-L10", "K-L10b", "K-I2"):
        Ts, sh, rp = res[cell]
        y = [float(sh[T].mean()) for T in Ts]
        top = S.mean(cell if cell != "K-L10b" else "K-L10b", "SW-0p4", "share_T") if cell != "K-I2" else S.mean(cell, gname(0.4, 0.0), "share_T")
        base = y[0]

        def first_ge(level):
            for i, T in enumerate(Ts):
                if y[i] >= level:
                    if i == 0:
                        return Ts[0], Ts[0]
                    return Ts[i], Ts[i - 1] + (level - y[i - 1]) / (y[i] - y[i - 1]) * (Ts[i] - Ts[i - 1])
            return None, None
        lvlA = 0.9 * top
        lvlB = base + 0.9 * (top - base)
        gA, iA = first_ge(lvlA)
        gB, iB = first_ge(lvlB)
        jumps = [(y[i + 1] - y[i], Ts[i], Ts[i + 1]) for i in range(len(Ts) - 1)]
        jmax = max(jumps)
        p("- %s: top (T = 0.4) %.4f, lowest-T level %.4f. Reading A (90 %% of the T = 0.4 value = %.4f): first sweep point at or above = %s, interpolated = %s. Reading B (90 %% of the rise = %.4f): first sweep point = %s, interpolated = %s. Largest adjacent jump: +%.4f between T = %g and %g." % (
            cell, top, base, lvlA, gA, "%.4f" % iA if iA else "-", lvlB, gB, "%.4f" % iB if iB else "-", jmax[0], jmax[1], jmax[2]))
        cross[cell] = dict(A=(gA, iA), B=(gB, iB), jump=jmax)
    for cell in ("K-L10", "K-I2"):
        lo, hi = iv[cell]
        gA, iA = cross[cell]["A"]
        gB, iB = cross[cell]["B"]
        p("- %s, registered interval [%g, %g], hand prediction T* = %g s: reading A grid point %s (%s), interpolated %s (%s); reading B grid point %s (%s), interpolated %s (%s)" % (
            cell, lo, hi, 0.170 if cell == "K-L10" else 0.035,
            gA, "in" if gA is not None and lo <= gA <= hi else "OUT", "%.4f" % iA if iA else "-", "in" if iA is not None and lo <= iA <= hi else "OUT",
            gB, "in" if gB is not None and lo <= gB <= hi else "OUT", "%.4f" % iB if iB else "-", "in" if iB is not None and lo <= iB <= hi else "OUT"))
    for nm, ix in (("A grid", (0, 0)), ("A interpolated", (0, 1)), ("B grid", (1, 0)), ("B interpolated", (1, 1))):
        key = "A" if nm[0] == "A" else "B"
        a_, b_ = cross["K-L10"][key][ix[1]], cross["K-L10b"][key][ix[1]]
        if a_ is not None and b_ is not None:
            sh_ = (b_ - a_) * 1e3
            p("- shift K-L10b minus K-L10, reading %s: **%.1f ms** (registered +30 +- 5): %s" % (nm, sh_, "within" if abs(sh_ - 30) <= 5 else "OUTSIDE"))
    # descriptive, not registered: mid-level crossing of the first step (level 0.92 at L10)
    p("")
    for cell, lvl in (("K-L10", 0.92), ("K-L10b", 0.92), ("K-I2", 0.96)):
        Ts, sh, rp = res[cell]
        y = [float(sh[T].mean()) for T in Ts]
        for i in range(1, len(Ts)):
            if y[i] >= lvl > y[i - 1]:
                p("- (not registered, descriptive) %s: share_T crosses %.2f at T = %.4f s" % (cell, lvl, Ts[i - 1] + (lvl - y[i - 1]) / (y[i] - y[i - 1]) * (Ts[i] - Ts[i - 1])))
                break
    p("")


def p4():
    p("### P4: positive control (E cells, T = 1.5 x nudges, H = inf, K of share_T)\n")
    for cell in ("E-I2", "E-L10"):
        names5 = [("PC-%s" % tag(x)) for x in S.NUDGE]
        p("| T | share_T mean |")
        p("|---|---|")
        for x, n in zip(S.NUDGE, names5):
            p("| %g | %.5f |" % (1.5 * x, S.mean(cell, n, "share_T")))
        for stat in ("K", "P"):
            st = S.kp(cell, names5, "share_T")[stat]
            rd = S.read(st, S.bar("share_T", cell))
            line("%s %s of share_T" % (cell, stat), rd)
    p("")


def p6():
    p("### P6: T = 2 s against T = inf (E and M cells, H = inf, paired difference S2 minus C3)\n")
    for cell in OUTAGE:
        rds = []
        for k in ("share_any", "v_mean", "tail_time"):
            rd = rdiff(cell, gname(2.0, INF), "C3", k)
            rds.append(rd["word"])
            line("%s %s" % (cell, k), rd)
    p("")


def p7():
    p("### P7: T = 0.4 s in the outage cells (H = inf unless said)\n")
    for cell in ("E-I2", "E-L10"):
        a = gname(0.4, INF)
        sa = S.vals(cell, a, "share_any")
        p("- %s share_any (S2 T=0.4, H=inf) = %.4f over seeds (min %.4f, max %.4f) against 1 - 347/12147 = %.4f +- 0.002" % (
            cell, sa.mean(), sa.min(), sa.max(), 1 - 347 / 12147))
        line("%s tail_time, S2 T=0.4 H=inf minus C3 (ms)" % cell, rdiff(cell, a, "C3", "tail_time"))
        p("  - tail time means (ms): S2 %.1f, C3 %.1f, C1 %.1f, C2 %.1f" % tuple(S.mean(cell, x, "tail_time") for x in (a, "C3", "C1", "C2")))
    for cell in ("M-I2", "M-L10"):
        a = gname(0.4, INF)
        sa = S.vals(cell, a, "share_any")
        p("- %s share_any (S2 T=0.4, H=inf) = %.4f (min %.4f, max %.4f) against [0.39, 0.41]" % (cell, sa.mean(), sa.min(), sa.max()))
    for cell in OUTAGE:
        p("- %s intake_expired: S2 (T=0.4 H=inf) %.1f, S2p %.1f per run (units = 12147)" % (
            cell, S.mean(cell, gname(0.4, INF), "intake_expired"), S.mean(cell, "S2p", "intake_expired")))
        p("  - share_any: S2 %.4f vs S2p %.4f" % (S.mean(cell, gname(0.4, INF), "share_any"), S.mean(cell, "S2p", "share_any")))
    p("")


def p8():
    p("### P8: the call against today (K cells)\n")
    for cell in CALL:
        a = gname(0.4, 0.0)
        p("**%s**\n" % cell)
        p("- outside the outage window (units handed over before 19.6 s or at 21.5 s and after): share_by_0p4 S2 (T=0.4,H=0) %.4f, C1 %.4f, C2 %.4f, C3 %.4f" % tuple(
            S.mean(cell, x, "win_out_by0p4") for x in (a, "C1", "C2", "C3")))
        line("share_by_0p4 outside the window, S2 minus C1 (prediction: >= -m)", rdiff(cell, a, "C1", "win_out_by0p4"))
        p("- over the whole run (no sign predicted): share_by_0p4 S2 %.4f, C1 %.4f; share_any S2 %.4f, C1 %.4f" % (
            S.mean(cell, a, "share_by_0p4"), S.mean(cell, "C1", "share_by_0p4"), S.mean(cell, a, "share_any"), S.mean(cell, "C1", "share_any")))
        line("whole-run share_by_0p4, S2 minus C1", rdiff(cell, a, "C1", "share_by_0p4"))
        line("v_mean, S2 (H=0) minus C1 (prediction: shown smaller)", rdiff(cell, a, "C1", "v_mean"))
        line("w_mean, S2 (H=0) minus C1 (reported)", rdiff(cell, a, "C1", "w_mean"))
        line("ooo_per_delivered, H=0 minus H=inf (prediction: shown larger)", rdiff(cell, a, gname(0.4, INF), "ooo_per_delivered"))
        st = S.vals(cell, a, "stall_total") * 1e-3
        p("- stall_total at H = 0: max over seeds %.3g s (prediction <= 1e-9 s): **%s**" % (st.max(), "yes" if st.max() <= 1e-9 else "NO"))
        sp = S.vals(cell, gname(0.4, INF), "stall_p99") * 1e-3
        sc2 = S.vals(cell, "C2", "stall_p99") * 1e-3
        p("- stall_p99 (s): S2 H=inf max over seeds %.3f (prediction <= 1.6); C2 mean %.3f (min %.3f; prediction >= the 1.5 s outage)" % (sp.max(), sc2.mean(), sc2.min()))
    p("")


def p9():
    p("### P9: the copy at the verdict (S2c against S2, T = 0.4; all six cells, both H)\n")
    for cell in CELLS6:
        for H, nm in ((0.0, "S2c-H0"), (INF, "S2c-Hinf")):
            a = gname(0.4, H)
            line("%s H=%s share_T, S2c - S2" % (cell, tag(H)), rdiff(cell, nm, a, "share_T"))
            line("%s H=%s share_any, S2c - S2 (prediction: not shown smaller)" % (cell, tag(H)), rdiff(cell, nm, a, "share_any"))
            ss = S.seeds_of(cell, nm)
            d = np.asarray([S.R(cell, nm, s)["repair_per_delivered"] - S.R(cell, a, s)["repair_per_delivered"]
                            - S.R(cell, nm, s)["gauges"]["verdicts"] / max(S.R(cell, nm, s)["delivered"], 1) for s in ss])
            rd = S.read(d, S.bar("repair_per_delivered", cell))
            line("%s H=%s repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0)" % (cell, tag(H)), rd)
    p("")


def p10():
    p("### P10: floors\n")
    for cell in OUTAGE:
        line("%s floors_per_delivered, S2 (T=0.4,H=inf) minus C2 (prediction: not shown larger)" % cell,
             rdiff(cell, gname(0.4, INF), "C2", "floors_per_delivered"))
    for T in (0.10, 0.14):
        v = S.vals("K-L10", "SW-%s" % tag(T), "floors_per_delivered")
        p("- K-L10 T = %g, H = 0: floors per delivered max over seeds %.4f, mean %.4f (prediction <= 0.2): **%s**; verdicts per run %.0f" % (
            T, v.max(), v.mean(), "yes" if v.max() <= 0.2 else "NO", np.mean([S.R("K-L10", "SW-%s" % tag(T), x)["gauges"]["verdicts"] for x in range(12)])))
    p("")


def aa():
    nf = S.nf_s3()
    p("### A/A self-check (call cells, S2 T = 0.4, H = 0.02, phase variants 0 and 1, 12 seeds)\n")
    n_eq = sum(1 for v in nf["selfcheck"].values() if v["word"] == "equivalent")
    n_un = [k for k, v in nf["selfcheck"].items() if v["word"] == "undetermined"]
    n_sh = [(k, v) for k, v in nf["selfcheck"].items() if v["word"].startswith("shown")]
    p("%d readings: %d equivalent, %d undetermined, %d shown.\n" % (len(nf["selfcheck"]), n_eq, len(n_un), len(n_sh)))
    for k, v in n_sh:
        p("- SHOWN: %s: %s" % (k, S.fmt(v)))
    p("\nFrozen noise floors NF (95th percentile over seeds of |A - A'|, largest over the two call cells; ms for times):\n")
    p("| readout | NF |")
    p("|---|---|")
    for k, v in nf["metrics"].items():
        p("| %s | %.4g |" % (k, v))
    p("")


def main(cmd):
    if cmd == "K":
        aa()
        p3_cells = None
        p5()
        p8()
    elif cmd == "EM":
        p4()
        p6()
        p7()
    elif cmd == "ALL":
        p3()
        p9()
        p10()
    elif cmd == "tables":
        for c in CELLS6:
            cell_table(c)
    elif cmd == "O":
        import report_o
        report_o.main()
    else:
        raise SystemExit("unknown " + cmd)
