"""Reading labels, A/A on the global-hold model and the accounting check (coordinator's last items).  python analysis_new.py"""
import numpy as np

import summarize2 as S

nf = S.nf_table()
R = lambda n, s: S.load()[(n, s)]
p = print


def reading(na, nb, k, cell, ref):
    ss = sorted(set(S.seeds_of(na)) & set(S.seeds_of(nb)))
    if k == "tail_verdicts":
        x = np.asarray([R(na, s)["tail_verdicts"] - R(nb, s)["tail_verdicts"] for s in ss], float)
        m = 1.0                                          # one seq (toy choice: a count has no registered bar)
    else:
        x = S.vals(na, k, ss) - S.vals(nb, k, ss)
        r0 = R(na, 0)
        T = S.T_of(k, cell, 1.0 - r0["one_minus_rho"], r0["n_src"], r0["delivered"], ref)
        m = max(T, nf["tail"].get(k, 0.0))
    rd = S.read(x, m)
    return "%.4g [%.4g, %.4g] | %.3g | %s" % (rd["mean"], rd["lo"], rd["hi"], m, rd["word"])


p("### Paired readings on the global-hold model (difference [CI] | m | label)\n")
p("| cell | contrast | metric | difference [CI] | m | label |")
p("|---|---|---|---|---|---|")
for c in ("I2", "L10"):
    for a, b in (("R0e", "R1"), ("R0c", "R0e")):
        for k in ("v_mean", "v_p99", "G_ratio", "repair_per_delivered", "tail_time"):
            p("| %s | %s - %s | %s | %s |" % (c, a, b, k, reading("T-%s-%s" % (c, a), "T-%s-%s" % (c, b), k, c, "REFT-%s" % c)))
    p("| %s | R1 - R0e | tail seqs with a verdict | %s |" % (c, reading("T-%s-R1" % c, "T-%s-R0e" % c, "tail_verdicts", c, "REFT-%s" % c)))
p("\nTail-seq verdict counts per run (mean over 12 seeds):\n")
for c in ("I2", "L10"):
    p("- %s: R0e %.1f, R0c %.1f, R1 %.1f" % (c, *[np.mean([R("T-%s-%s" % (c, r), s)["tail_verdicts"] for s in range(12)]) for r in ("R0e", "R0c", "R1")]))

p("\n### A/A on the global-hold R0e and R0c (I2 tail cell, variants 0 and 1, 12 seeds): 95th percentile of |A - A'| per metric against the frozen floors\n")
p("| receiver | metric | p95 abs(A - A') | frozen NF (tail cells) | bar m = max(T, NF) used in Part T | under NF? | under m? |")
p("|---|---|---|---|---|---|---|")
for recv in ("R0e", "R0c"):
    a, b = "AAN-I2-%s-A" % recv, "AAN-I2-%s-B" % recv
    r0 = R(a, 0)
    for k in ("v_mean", "repair_per_delivered", "v_p99", "tail_time", "G_ratio"):
        d95 = S.np.percentile(np.abs(S.vals(a, k) - S.vals(b, k)), 95) if hasattr(S, "np") else np.percentile(np.abs(S.vals(a, k) - S.vals(b, k)), 95)
        T = S.T_of(k, "I2", 0.99, r0["n_src"], r0["delivered"], "REFT-I2")
        m = max(T, nf["tail"][k])
        p("| %s | %s | %.4g | %.4g | %.4g | %s | %s |" % (recv, k, d95, nf["tail"][k], m, "yes" if d95 <= nf["tail"][k] else "NO, over by %.4g" % (d95 - nf["tail"][k]),
                                                      "yes" if d95 <= m else "NO, over by %.4g" % (d95 - m)))

p("\n### Accounting: R0c's extra G_rx over R0e, in packets per object, and its overlap with the seqs that get a verdict under R0c only\n")
for c in ("I2", "L10"):
    d0 = 12147 - 347
    dG, n1, ov, extra_tail = [], [], [], []
    for s in range(12):
        rc, re = R("T-%s-R0c" % c, s), R("T-%s-R0e" % c, s)
        dG.append(rc["G_final"] - re["G_final"])
        v1 = set(rc["tail_verdicted"]) - set(re["tail_verdicted"])
        gc = {d0 + i for i, (v, r) in enumerate(zip(rc["tail_v"], rc["tail_res"])) if v is None and r is not None}
        ge = {d0 + i for i, (v, r) in enumerate(zip(re["tail_v"], re["tail_res"])) if v is None and r is not None}
        n1.append(len(v1))
        ov.append(len(v1 & (gc - ge)))
        extra_tail.append(len(gc - ge))
    p("- %s: G_rx(R0c) - G_rx(R0e) = %.1f packets per object (%.4f of S_src); seqs with a verdict under R0c only %.1f; tail seqs in G under R0c but not under R0e %.1f; "
      "overlap of the two sets %.1f" % (c, np.mean(dG), np.mean(dG) / 12147, np.mean(n1), np.mean(extra_tail), np.mean(ov)))
