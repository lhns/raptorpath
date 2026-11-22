"""Markdown tables for results-stage2.md.  python summarize2.py A|T|B"""
import json
import math

import numpy as np

import summarize2 as S

p = print
tag = S.tag


def R(name, seed=0):
    return S.load()[(name, seed)]


def mean(name, k):
    v = S.vals(name, k)
    return float(np.nanmean(v)) if len(v) else float("nan")


def gmean(name, g):
    d = S.load()
    ss = S.seeds_of(name)
    return float(np.mean([d[(name, s)]["gauges"][g] for s in ss]))


def Tbar(name, k, cell, ref):
    r = R(name)
    return S.T_of(k, cell, 1.0 - r["one_minus_rho"], r["n_src"], r["delivered"], ref)


def bar(name, k, cell, ref, kind="metrics", pre=None, stat=None):
    """m = max(T, NF) for metric k; for K/P statistics NF_K / NF_P of the preset."""
    nf = S.nf_table()
    T = Tbar(name, k, cell, ref)
    if stat is not None:
        return max(T, nf["kp"][pre][k]["NF_" + stat])
    return max(T, nf[kind].get(k, 0.0))


# ------------------------------------------------------------------------------------------------
def part_A():
    d = S.load()
    nf = S.nf_table()
    p("### Noise floors NF (95th percentile over 12 seeds of |A - A'|, largest over the six A/A cells; ms for times)\n")
    p("| metric | NF | | metric | NF (Part T cells) |")
    p("|---|---|---|---|---|")
    ks = list(S.ALL_M)
    for i, k in enumerate(ks):
        tk = k if k in nf["tail"] else ""
        p("| %s | %.4g | | %s | %s |" % (k, nf["metrics"][k], tk, "%.4g" % nf["tail"][tk] if tk else ""))
    for k in ("tail_time", "tail_v_mean", "tail_v_p50"):
        p("| | | | %s | %.4g |" % (k, nf["tail"][k]))
    p("\n### NF_K and NF_P (A/A on the five-point sets)\n")
    p("| preset | readout | NF_K | NF_P |")
    p("|---|---|---|---|")
    for pre in ("Auto", "Realtime", "Bulk"):
        for k in S.KP_READ:
            p("| %s%s | %s | %.4g | %.4g |" % (pre, " (max of the two)" if pre == "Bulk" else "", k, nf["kp"][pre][k]["NF_K"], nf["kp"][pre][k]["NF_P"]))
    n_eq, n_un, n_sh = S.md_selfcheck(nf)
    p("\n### A/A self-check (mean A/A difference against T)\n")
    p("%d readings equivalent, %d undetermined, %d shown." % (n_eq, len(n_un), len(n_sh)))
    for k, v in n_un:
        p("- undetermined: %s: %s" % (k, S.fmt(v)))
    for k, v in n_sh:
        p("- SHOWN: %s: %s" % (k, S.fmt(v)))
    # P4j
    p("\n### P4j: U against U*, d frozen at RTprop, I1 (12 seeds each)\n")
    p("| omega | preset | isolated holes (mean) | worst slack over seeds (<= 1e-9 required) | ratio_U isolated (mean) | ratio_U all holes (mean) | pass |")
    p("|---|---|---|---|---|---|---|")
    for om in (1.0, 10.0):
        for pre in ("Bulk", "Auto", "Realtime"):
            nm = "P4J-%s-%s" % (tag(om), pre)
            ss = S.seeds_of(nm)
            rr = [R(nm, s) for s in ss]
            slack = max(r["u_iso_slack"] for r in rr)
            iso = [r["u_iso_cost"] / r["u_iso_star"] for r in rr if r["u_iso_star"] > 0]
            al = [r["u_ratio"] for r in rr if r.get("u_ratio") is not None]
            p("| %g | %s | %.0f | %.3g | %s | %s | %s |" % (om, pre, np.mean([r["u_iso_n"] for r in rr]), slack,
                                                           "%.3f" % np.mean(iso) if iso else "n/a (no cost)", "%.3f" % np.mean(al) if al else "n/a",
                                                           "pass" if slack <= 1e-9 else "FAIL"))
    # bridge
    p("\n### P4h bridge to the stage 1 toy (I1, N = 1, Realtime, J = 0, 20 s, 12 seeds; stage 1 at commit with floor_with_copy = False)\n")
    for snd in ("S0", "S1c"):
        nm = "BR-%s" % snd
        rr = [R(nm, s) for s in S.seeds_of(nm)]
        same = {k: sum(1 for r in rr if r["stage1"][k] == r["stage2"][k]) for k in rr[0]["stage1"]}
        verd = sum(1 for r in rr if r["stage1_gauges"]["verdicts"] == r["stage2_gauges"]["verdicts"])
        p("- %s: seeds with identical %s; verdict count equal in %d of %d seeds; floors sent stage 1 %.0f (of which refresh %.1f) against stage 2 %.0f (of which repeats %.1f), mean per run; "
          "hol p99 stage 1 %.3f ms against e p99 stage 2 %.3f ms"
          % (snd, ", ".join("%s %d/%d" % (k, v, len(rr)) for k, v in same.items()), verd, len(rr),
             np.mean([r["stage1_gauges"]["floors_ctl"] for r in rr]), np.mean([r["stage1_gauges"]["floor_refresh"] for r in rr]),
             np.mean([r["stage2_gauges"]["floors"] for r in rr]), np.mean([r["stage2_gauges"]["floor_rep"] for r in rr]),
             np.mean([r["hol_p99"] for r in rr]) * 1e3, np.mean([r["e_p99"] for r in rr]) * 1e3))
    # timing
    p("\n### Wall time per run (s, mean)\n")
    for grp in ("AA-", "AAT-", "KP-", "P4J-", "BR-", "REF-"):
        w = [r["wall_s"] for (n, s), r in d.items() if n.startswith(grp)]
        if w:
            p("- %s %.2f s over %d runs" % (grp, np.mean(w), len(w)))


# ------------------------------------------------------------------------------------------------
def part_T():
    d = S.load()
    nf = S.nf_table()
    cells = ("I1", "I2", "L10")
    recvs = (("R0e", "R0e (today's early-skip, engine eps-hat)"), ("R0c", "R0c (early skip, allowance counts floor passes only)"), ("R1", "R1 (floor only)"), ("U", "U, omega = 1"), ("R0cum", "R0e, cumulative eps-hat (history, reported only)"), ("R0ccum", "R0c, cumulative eps-hat (history, reported only)"), ("R0x", "R0e, per-seq EWMA eps-hat alpha 1/256 (history, reported only)"))
    p("### Part T: per-config means over 12 seeds (times in ms)\n")
    p("| cell | receiver | v p50 | v p99 | v p99.9 | v mean | e mean | G_rx/S_src | passes (floor / R0e) | P1 violations (worst excess) | tail time | stalled | tail seqs with a verdict | filled after the window | R0e hold fell back | repair sends / delivered | G_floor/S_src | floors / delivered | last copies / delivered |")
    p("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for c in cells:
        for rc, lab in recvs + (("S0", "S0 reference"),):
            nm = "REFT-%s" % c if rc == "S0" else "T-%s-%s" % (c, rc)
            if not S.seeds_of(nm):
                continue
            rr = [R(nm, s) for s in S.seeds_of(nm)]
            g = lambda k: np.mean([r["gauges"][k] for r in rr])
            tt = [r["tail_time"] for r in rr if r.get("tail_time") is not None]
            p("| %s | %s | %.2f | %.1f | %.1f | %.2f | %.2f | %.5f | %.0f (%.0f / %.0f) | %.1f (%.1f) | %s | %d/%d | %.1f | %.1f | %.1f | %.5f | %.5f | %.5f | %.5f |" % (
                c, lab, mean(nm, "v_p50"), mean(nm, "v_p99"), mean(nm, "v_p999"), mean(nm, "v_mean"), mean(nm, "e_mean"), mean(nm, "G_ratio"),
                g("passes"), g("pass_floor"), g("pass_r0e"), g("p1_viol"), g("p1_worst"),
                "%.1f" % (np.mean(tt) * 1e3) if tt else "n/a", sum(1 for r in rr if r["stalled"]), len(rr),
                np.mean([r["tail_verdicts"] for r in rr]), np.mean([r["tail_filled_after_window"] for r in rr]), g("r0e_fallback"),
                mean(nm, "repair_per_delivered"), np.mean([x["GF_ratio"] for x in rr]), mean(nm, "floors_per_delivered"), mean(nm, "copies_per_delivered")))
    p("\nTail-only waits (the 347 tail seqs; ms): mean v / median v")
    for c in cells:
        for rc, lab in recvs:
            nm = "T-%s-%s" % (c, rc)
            p("- %s %s: %.1f / %.1f" % (c, rc, mean(nm, "tail_v_mean"), mean(nm, "tail_v_p50")))
    p("\n### Part T readings: paired per-seed differences, 95 percent bootstrap CI against m = max(T, NF)\n")
    p("| cell | contrast | metric | difference [CI] | m | reading |")
    p("|---|---|---|---|---|---|")
    mets = ("v_p50", "v_p99", "v_p999", "v_mean", "e_mean", "G_ratio", "max_ep_spend", "repair_per_delivered", "tail_time")
    for c in cells:
        for a, b in (("R0e", "R0c"), ("R0e", "R1"), ("R0c", "R1"), ("R0e", "U"), ("R0c", "U"), ("R1", "U")):
            na, nb = "T-%s-%s" % (c, a), "T-%s-%s" % (c, b)
            ss = sorted(set(S.seeds_of(na)) & set(S.seeds_of(nb)))
            for k in mets:
                va, vb = S.vals(na, k, ss), S.vals(nb, k, ss)
                x = va - vb
                T = Tbar(na, k, c, "REFT-%s" % c)
                m = max(T, nf["tail"].get(k, 0.0))
                rd = S.read(x, m)
                p("| %s | %s - %s | %s | %.4g [%.4g, %.4g] | %.3g | %s |" % (c, a, b, k, rd["mean"], rd["lo"], rd["hi"], m, rd["word"]))
    p("\nNF for Part T comes from the A/A on the I2 cell (R0e and R1) and is applied to all three cells.")


# ------------------------------------------------------------------------------------------------
def p3_family(names, cell, pre, readouts=S.KP_READ, nfpre=None):
    """K and P readings for one five-point family at one preset.  names are ordered 0.98..1.02."""
    nfpre = nfpre or pre
    nf = S.nf_table()
    ss = sorted(set.intersection(*[set(S.seeds_of(n)) for n in names]))
    out = {}
    for k in readouts:
        kp = S.kp_stats(names, k, ss)
        T = Tbar(names[2], k, cell, "REF-%s" % cell)
        res = {}
        for stat in ("K", "P"):
            m = max(T, nf["kp"][nfpre][k]["NF_" + stat])
            res[stat] = S.read(kp[stat], m)
        res["pass"] = ("fail" if any(res[s]["word"].startswith("shown") for s in "KP")
                       else "pass" if all(res[s]["word"] == "equivalent" for s in "KP") else "undetermined")
        out[k] = res
    return out


def part_B():
    d = S.load()
    nf = S.nf_table()
    # ---- HZ and planted controls (P3 power) first
    p("### P3 power: HZ and the planted controls (U at Auto, I1, omega = 1, rho = 1)\n")
    p("K and P are per-seed statistics of the five points at 0.98, 0.99, 1, 1.01, 1.02 times Auto. Reading against m = max(T, NF).\n")
    p("| control | readout | K (mean [CI]) | K reading | P (mean [CI]) | P reading | outcome | required |")
    p("|---|---|---|---|---|---|---|---|")
    req = {"HZ": "fail on rbf and e mean", "STEP": "fail on K", "SPIKE": "fail on P, K passes", "KINK": "pass both"}
    ctl = {}
    for c in ("HZ", "STEP", "SPIKE", "KINK"):
        names = ["HZ-I1-Auto-%s" % tag(h) for h in S.NUDGE] if c == "HZ" else ["PL-%s-%s" % (c, tag(h)) for h in S.NUDGE]
        res = p3_family(names, "I1", "Auto")
        ctl[c] = res
        for k in ("rbf", "e_mean", "e_p99"):
            r = res[k]
            p("| %s | %s | %.3g [%.3g, %.3g] | %s | %.3g [%.3g, %.3g] | %s | %s | %s |" % (
                c, k, r["K"]["mean"], r["K"]["lo"], r["K"]["hi"], r["K"]["word"], r["P"]["mean"], r["P"]["lo"], r["P"]["hi"], r["P"]["word"], r["pass"], req[c]))
    hz_ok = all(ctl["HZ"][k]["K"]["word"].startswith("shown") for k in ("rbf", "e_mean"))
    p("\n- HZ fails P3 on rbf and mean e: **%s**. %s" % ("yes" if hz_ok else "NO", "P3 has power." if hz_ok else "P3 has no power: every P3 reading below is void."))
    st_ok = any(ctl["STEP"][k]["K"]["word"].startswith("shown") for k in ("rbf", "e_mean"))
    sp_ok = any(ctl["SPIKE"][k]["P"]["word"].startswith("shown") for k in ("rbf", "e_mean")) and not any(ctl["SPIKE"][k]["K"]["word"].startswith("shown") for k in ("rbf", "e_mean"))
    ki_ok = all(ctl["KINK"][k][s]["word"] == "equivalent" for k in ("rbf", "e_mean") for s in "KP")
    p("- STEP fails on K (rbf or mean e): **%s**; SPIKE fails on P with K passing: **%s**; KINK passes both K and P on rbf and mean e: **%s**." % (st_ok, sp_ok, ki_ok))
    # curvature follow-up for KINK (three widths fit a + c h + k h^2; only a is a preset anomaly)
    p("KINK curvature follow-up: intercept a of Delta(h) = a + c h + k h^2 from widths 0.005, 0.01, 0.02, per seed")
    for k in ("rbf", "e_mean"):
        ss = sorted(set(S.seeds_of("PL-KINK-0.995")) & set(S.seeds_of("PL-KINK-1.005")) & set(S.seeds_of("PL-KINK-1")))
        v = lambda n: S.vals(n, k, ss)
        Dl = {h: v("PL-KINK-1") - 0.5 * (v("PL-KINK-%s" % tag(1 - h)) + v("PL-KINK-%s" % tag(1 + h))) for h in (0.005, 0.01, 0.02)}
        hs = np.array([0.005, 0.01, 0.02])
        Ainv = np.linalg.inv(np.stack([np.ones(3), hs, hs ** 2], 1))
        a_ = Ainv[0, 0] * Dl[0.005] + Ainv[0, 1] * Dl[0.01] + Ainv[0, 2] * Dl[0.02]
        m = max(Tbar("PL-KINK-1", k, "I1", "REF-I1"), nf["kp"]["Auto"][k]["NF_P"])
        p("- %s: a %s" % (k, S.fmt(S.read(a_, m), 3)))
    # ---- P3 for U
    p("\n### P3 for U at the three presets, I1 (12 seeds; K and P each read against m = max(T, NF))\n")
    p("Cells show K/P readings: E equivalent, S shown, ? undetermined. Readouts: e mean, e p99, rbf, G_rx/S_src, repair sends per delivered.\n")
    p("| preset | omega | e mean | e p99 | rbf | G_rx/S_src | repair/delivered | verdict |")
    p("|---|---|---|---|---|---|---|---|")
    code = {"equivalent": "E", "undetermined": "?"}
    fails = []
    for pre in ("Bulk", "Auto", "Realtime"):
        for om in S.OMEGAS:
            names = ["U-I1-%s-w%s-%s" % (pre, tag(om), tag(h)) for h in S.NUDGE]
            res = p3_family(names, "I1", pre)
            cells_ = []
            for k in S.KP_READ:
                r = res[k]
                cells_.append("%s/%s" % (code.get(r["K"]["word"], "S"), code.get(r["P"]["word"], "S")))
                if r["pass"] != "pass":
                    fails.append((pre, om, k, r))
            vs = "fail" if any(res[k]["pass"] == "fail" for k in S.KP_READ) else "pass" if all(res[k]["pass"] == "pass" for k in S.KP_READ) else "undetermined"
            p("| %s | %g | %s | %s |" % (pre, om, " | ".join(cells_), vs))
    p("\nReadouts that did not pass (K and P mean, CI, bar):\n")
    for pre, om, k, r in fails:
        p("- %s, omega %g, %s: K %s; P %s" % (pre, om, k, S.fmt(r["K"], 3), S.fmt(r["P"], 3)))
    # rho nudge
    p("\n### P3 on rho (nudges on 1 - rho, Realtime delta, I2; bars as the Realtime preset)\n")
    for om in (1.0, 10.0):
        names = ["RHO-I2-w%s-0.98" % tag(om), "RHO-I2-w%s-0.99" % tag(om), "U-I2-Realtime-w%s-1" % tag(om), "RHO-I2-w%s-1.01" % tag(om), "RHO-I2-w%s-1.02" % tag(om)]
        res = p3_family(names, "I2", "Realtime")
        for k in S.KP_READ:
            r = res[k]
            p("- omega %g, %s: K %s; P %s; **%s**" % (om, k, S.fmt(r["K"], 3), S.fmt(r["P"], 3), r["pass"]))
    # ---- rbf per omega
    p("\n### rbf (released then filled, per hole) per omega against the bar epsilon_cell^2, level reading per seed\n")
    p("| cell | preset | " + " | ".join("w=%g" % o for o in S.OMEGAS) + " |")
    p("|---|---|" + "---|" * len(S.OMEGAS))
    for cell in ("I1", "I2", "G"):
        for pre in ("Bulk", "Auto", "Realtime"):
            row = []
            for om in S.OMEGAS:
                nm = "U-%s-%s-w%s-%s" % (cell, pre, tag(om), "1")
                if cell == "I1":
                    nm = "U-I1-%s-w%s-1" % (pre, tag(om))
                rd = S.read(S.vals(nm, "rbf"), S.EPS[cell] ** 2) if S.seeds_of(nm) else None
                if rd is None:
                    row.append("n/a")
                else:
                    row.append("%.2g [%.2g, %.2g] %s" % (rd["mean"], rd["lo"], rd["hi"], {"equivalent": "KEEPS ORDER", "undetermined": "?", "shown larger": "breaks"}.get(rd["word"], rd["word"])))
            p("| %s (bar %.2g) | %s | %s |" % (cell, S.EPS[cell] ** 2, pre, " | ".join(row)))
    # ---- ratio_U
    p("\n### ratio_U = cost_U / cost_U* (d moves in these runs, holes before the first d sample pay no H_o)\n")
    p("| cell | preset | omega | ratio_U mean (min..max) | isolated holes: cost_U / cost_U* | isolated runs above 2 + omega N_inf / cost_U* |")
    p("|---|---|---|---|---|---|")
    for cell in ("I1", "I2", "G"):
        for pre in ("Bulk", "Auto", "Realtime"):
            for om in S.OMEGAS:
                nm = "U-%s-%s-w%s-1" % (cell, pre, tag(om))
                if not S.seeds_of(nm):
                    continue
                rr = [R(nm, s) for s in S.seeds_of(nm)]
                al = [r["u_ratio"] for r in rr if r.get("u_ratio") is not None and r["u_ratio"] != math.inf]
                iso = [r["u_iso_cost"] / r["u_iso_star"] for r in rr if r.get("u_iso_star", 0) > 0]
                above = sum(1 for r in rr if r.get("u_iso_slack") is not None and r["u_iso_slack"] > 1e-9)
                p("| %s | %s | %g | %s | %s | %d of %d |" % (cell, pre, om, "%.3f (%.3f..%.3f)" % (np.mean(al), min(al), max(al)) if al else "n/a (no cost)",
                                                          "%.3f" % np.mean(iso) if iso else "n/a", above, len(rr)))
    # ---- d_rep, P2
    p("\n### d_all against d_rep at one path (P4g witness), I1, omega = 1\n")
    for pre in ("Bulk", "Auto", "Realtime"):
        a, b = "U-I1-%s-w1-1" % pre, "UREP-I1-%s" % pre
        eq = all(R(a, s)["d_all"] == R(a, s)["d_rep"] for s in S.seeds_of(a))
        same = all(R(a, s)["e_mean"] == R(b, s)["e_mean"] and R(a, s)["rbf"] == R(b, s)["rbf"] for s in S.seeds_of(b))
        p("- %s: d_all == d_rep in every seed: %s; the d_rep arm gives identical e mean and rbf: %s" % (pre, eq, same))
    p("\n### P2: rho = 1 and 1 - 1e-7, Realtime delta, I1, omega = 1\n")
    for nm in ("P2-rho1", "P2-rho1m"):
        rr = [R(nm, s) for s in S.seeds_of(nm)]
        p("- %s: verdicts %d, floors %d, passes %d, G_rx at the end %d (all seeds, summed)" % (nm, sum(r["gauges"]["verdicts"] for r in rr), sum(r["gauges"]["floors"] for r in rr),
                                                                                              sum(r["gauges"]["passes"] for r in rr), sum(r["G_final"] for r in rr)))
    # ---- P1 over all of B
    viol = sum(r["gauges"]["p1_viol"] for (n, s), r in d.items() if n.startswith(("U-", "RHO-", "P2-", "HZ-", "PL-", "AG1-", "UREP-")))
    p("\nP1 violations summed over every Part B run: %d" % viol)
    fh = sum(r["gauges"]["fh_late"] for (n, s), r in d.items() if n.startswith(("U-", "RHO-", "P2-", "HZ-", "PL-", "AG1-", "UREP-")))
    hmax = max(r["H_max"] for (n, s), r in d.items() if n.startswith(("U-", "RHO-", "P2-", "HZ-", "PL-", "AG1-", "UREP-")))
    fifo = sum(r["gauges"]["fifo_viol"] for (n, s), r in d.items() if n.startswith(("U-", "RHO-", "P2-", "HZ-", "PL-", "AG1-", "UREP-")))
    p("H estimator: largest H over every Part B run %.3g s, late arrivals counted by the estimator %d, FIFO violations %d" % (hmax, fh, fifo))
    # ---- AG1
    p("\n### AG1: where the age gate can refuse (U at omega = 1, rho = 0.99; counts are sums over 12 seeds)\n")
    p("n_dec counts (hole, report) pairs where age <= D and the count would have admitted the verdict.\n")
    p("| cell | delta point | arm | n_dec | verdicts (seed-era) | gate refused, count full | cold-start refusals | e mean ms | e p99 ms | G_rx/S_src | repair/delivered |")
    p("|---|---|---|---|---|---|---|---|---|---|---|")
    pts = ("Realtime", "Auto", "b115", "b13", "Bulk")
    for cname in ("I1", "I2", "Q"):
        for pt in pts:
            for arm in ("RT", "EWMA", "ZERO"):
                nm = "AG1-%s-%s-%s" % (cname, pt, arm)
                rr = [R(nm, s) for s in S.seeds_of(nm)]
                p("| %s | %s | %s | %d | %s | %.0f | %.0f | %.3f | %.2f | %.5f | %.5f |" % (
                    cname, pt, arm, sum(r["gauges"]["n_dec"] for r in rr), "%.0f (%.1f)" % (np.mean([r["gauges"]["verdicts"] for r in rr]), np.mean([r["gauges"]["seed_verdicts"] for r in rr])),
                    np.mean([r["gauges"]["age_gate_fail"] for r in rr]), np.mean([r["gauges"]["gate_cold"] for r in rr]),
                    mean(nm, "e_mean"), mean(nm, "e_p99"), mean(nm, "G_ratio"), mean(nm, "repair_per_delivered")))
    p("\nWitnesses: n_dec > 0 at b = 1.3 and at Bulk under D_RTprop, and at Auto under D_EWMA, or the cell is void.\n")
    for cname in ("I1", "I2", "Q"):
        w = {pt: sum(R("AG1-%s-%s-RT" % (cname, pt), s)["gauges"]["n_dec"] for s in range(12)) for pt in pts}
        e = sum(R("AG1-%s-Auto-EWMA" % cname, s)["gauges"]["n_dec"] for s in range(12))
        p("- %s: D_RTprop n_dec %s; D_EWMA at Auto n_dec %d; witness %s" % (cname, w, e, "met" if (w["b13"] > 0 and w["Bulk"] > 0 and e > 0) else "NOT MET (cell void)"))
    p("\n### AG1 arm comparisons (paired differences against D_RTprop, bars m = max(T, NF))\n")
    p("| cell | delta point | arm - D_RTprop | e mean | e p99 | G_rx/S_src | repair/delivered | reading |")
    p("|---|---|---|---|---|---|---|---|")
    for cname in ("I1", "I2", "Q"):
        cell = "I2" if cname == "I2" else "I1"
        for pt in pts:
            base = "AG1-%s-%s-RT" % (cname, pt)
            for arm in ("EWMA", "ZERO"):
                nm = "AG1-%s-%s-%s" % (cname, pt, arm)
                words, cellsv = [], []
                for k in ("e_mean", "e_p99", "G_ratio", "repair_per_delivered"):
                    m = bar(base, k, cell, "REF-%s" % cell)
                    rd = S.read(S.vals(nm, k) - S.vals(base, k), m)
                    cellsv.append("%.3g (%s)" % (rd["mean"], rd["word"]))
                    words.append(rd["word"])
                allEq = all(w == "equivalent" for w in words)
                nd = sum(R(nm, s)["gauges"]["n_dec"] for s in range(12))
                rd_ = "void (no witness)" if (arm == "RT" and nd == 0) else ("decides only timing: equivalent on every readout" if allEq else
                                                                         ("D is live and changes a readout by a shown amount" if any(w.startswith("shown") for w in words) else "undetermined"))
                p("| %s | %s | %s | %s | %s |" % (cname, pt, arm, " | ".join(cellsv), rd_))
    # ---- timing
    p("\n### Wall time per run (s, mean)\n")
    for grp in ("U-I1-", "U-I2-", "U-G-", "HZ-", "PL-", "AG1-", "RHO-"):
        w = [r["wall_s"] for (n, s), r in d.items() if n.startswith(grp)]
        if w:
            p("- %s %.2f s over %d runs" % (grp, np.mean(w), len(w)))


def part_REP():
    nf = S.nf_table()
    p("### Replication of the I2 tail A/A with fresh phase draws (variants 2 and 3, 12 seeds)\n")
    p("| receiver, eps-hat | metric | C - D mean [CI] | T | reading |")
    p("|---|---|---|---|---|")
    shown = 0
    for recv, eps in (("R0e", "engine"), ("R0e", "cum"), ("R1", "engine")):
        a, b = "REP-I2-%s-%s-C" % (recv, eps), "REP-I2-%s-%s-D" % (recv, eps)
        for k in S.T_M:
            T = S.T_of(k, "I2", 0.99, R(a)["n_src"], R(a)["delivered"], "REFT-I2")
            rd = S.read(S.vals(a, k) - S.vals(b, k), T)
            if rd["word"].startswith("shown"):
                shown += 1
            if k in ("v_p99", "v_p999", "e_p99", "e_p999", "tail_time", "tail_v_mean", "tail_v_p50") or rd["word"].startswith("shown"):
                p("| %s %s | %s | %.4g [%.4g, %.4g] | %.3g | %s |" % (recv, eps, k, rd["mean"], rd["lo"], rd["hi"], T, rd["word"]))
    p("\nReadings shown across the replication: %d" % shown)
    p("\n### Phase scatter of tail time (ms): mean over 12 seeds per phase set, and per-seed SD of the difference between two sets\n")
    for recv, eps in (("R0e", "engine"), ("R0e", "cum"), ("R1", "engine")):
        names = ["REP-I2-%s-%s-C" % (recv, eps), "REP-I2-%s-%s-D" % (recv, eps)]
        v = [S.vals(n, "tail_time") for n in names]
        p("- %s %s: variant 2 mean %.1f, variant 3 mean %.1f; SD of per-seed difference %.1f" % (recv, eps, v[0].mean(), v[1].mean(), (v[0] - v[1]).std(ddof=1)))
    for recv in ("R0e", "R1"):
        a, b = "AAT-I2-%s-A" % recv, "AAT-I2-%s-B" % recv
        v = [S.vals(a, "tail_time"), S.vals(b, "tail_time")]
        p("- %s (frozen A/A, cumulative eps-hat for R0e): variant 0 mean %.1f, variant 1 mean %.1f; SD of per-seed difference %.1f" % (recv, v[0].mean(), v[1].mean(), (v[0] - v[1]).std(ddof=1)))
    p("\n### AG1 at Realtime under D_EWMA: n_dec before and after the first RTT sample (summed over 12 seeds)\n")
    d = S.load()
    for c in ("I1", "I2", "Q"):
        nm = "AG1S-%s-Realtime-EWMA" % c
        tot = sum(d[(nm, s)]["gauges"]["n_dec"] for s in range(12))
        sd = sum(d[(nm, s)]["gauges"]["n_dec_seed"] for s in range(12))
        p("- %s: n_dec %d, of which in the seed era (D = 25 ms) %d, after the first sample %d" % (c, tot, sd, tot - sd))


def main(cmd):
    {"A": part_A, "T": part_T, "B": part_B, "REP": part_REP}[cmd]()
