"""O-I2 (overload, reported only) and P11."""
import numpy as np

import summarize3 as S

p = print
CELL = "O-I2"


def main():
    arms = [("C1 today's receiver, rho = 0.99", "C1"), ("S2 T=0.4 H=0 (P11 arm)", "G-T0p4-H0"), ("S2 T=0.4 H=inf", "G-T0p4-Hinf"),
            ("S2p T=0.4 H=inf (verdicted seqs stay in the outstanding count)", "S2p"),
            ("S2p T=0.4 H=0 (unregistered)", "S2p-H0")]
    cols = [("share_any", "share any", "%.4f", 1), ("share_T", "share by own T", "%.4f", 1), ("share_by_0p4", "share by 0.4", "%.4f", 1),
            ("share_by_2", "share by 2", "%.4f", 1), ("w_mean", "w mean ms", "%.0f", 1), ("w_p50", "w p50 ms", "%.0f", 1), ("w_p99", "w p99 ms", "%.0f", 1),
            ("intake_expired", "intake expired", "%.0f", 1), ("finish", "finish s", "%.2f", 1e-3), ("stall_total", "stall total s", "%.2f", 1e-3)]
    p("| arm | seeds | " + " | ".join(c[1] for c in cols) + " |")
    p("|---|---|" + "---|" * len(cols))
    for lab, a in arms:
        ss = S.seeds_of(CELL, a)
        if not ss:
            p("| %s | 0 | not run |" % lab)
            continue
        p("| %s | %d | %s |" % (lab, len(ss), " | ".join(f % (S.mean(CELL, a, k) * sc) for k, _, f, sc in cols)))
    p("")
    for a, lab, reg in (("G-T0p4-H0", "S2 T=0.4 H=0 (the registered arm)", True), ("S2p", "S2p H=inf (not registered for P11)", False),
                        ("S2p-H0", "S2p H=0 (unregistered)", False)):
        ss = S.seeds_of(CELL, a)
        if not ss:
            continue
        v = np.asarray([S.R(CELL, a, s)["share_T"] for s in ss])
        verdict = ("**%s**" % ("PASS" if v.max() < 0.2 else "FAIL")) if reg else ("below 0.2: %s (value only, not a reading)" % ("yes" if v.max() < 0.2 else "no"))
        p("- P11, %s: share_T per seed min %.4f, mean %.4f, max %.4f; prediction < 0.2: %s" % (lab, v.min(), v.mean(), v.max(), verdict))
    ss = S.seeds_of(CELL, "G-T0p4-H0")
    if ss:
        g = S.R(CELL, "G-T0p4-H0", ss[0])["gauges"]
        p("- S2 H=0, seed %d: pause events %d, L2 verdicts %d, units %d, sent %d" % (ss[0], g["pause_events"], g["l2_fail"], S.R(CELL, "G-T0p4-H0", ss[0])["units"], S.R(CELL, "G-T0p4-H0", ss[0])["n_src"]))
    p("")
