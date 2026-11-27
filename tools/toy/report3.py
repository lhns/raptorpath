"""Part T tables for the global-hold early-skip model, the paired tail-only split and the short-window cell.  python summarize2.py T | REP"""
import io
import contextlib

import numpy as np

import summarize2 as S
import report2 as R2

p = print
R = R2.R
mean = R2.mean

RECVS = (("R0e", "R0e (today's early-skip; engine-like eps-hat, one global hold)"),
         ("R0c", "R0c (same receiver; allowance counts floor passes only)"),
         ("R1", "R1 (floor only)"), ("U", "U, omega = 1"),
         ("R0eentry", "R0e, hold fixed per entry at buffering (history)"),
         ("R0centry", "R0c, hold fixed per entry at buffering (history)"),
         ("R0cum", "R0e, cumulative eps-hat, per-entry hold (history)"),
         ("R0ccum", "R0c, cumulative eps-hat, per-entry hold (history)"),
         ("R0x", "R0e, per-seq EWMA eps-hat alpha 1/256, per-entry hold (history)"))


def head():
    p("| cell | receiver | v p50 | v p99 | v p99.9 | v mean | G_rx/S_src | passes (floor / early skip) | P1 violations (worst excess) | tail time | stalled | tail seqs with a verdict | filled after the window | hold evaluations in the fallen-back state | repair sends / delivered | G_floor/S_src | floors / delivered | last copies / delivered |")
    p("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")


def row(c, lab, nm):
    if not S.seeds_of(nm):
        return
    rr = [R(nm, s) for s in S.seeds_of(nm)]
    g = lambda k: np.mean([r["gauges"].get(k, 0) for r in rr])
    tt = [r["tail_time"] for r in rr if r.get("tail_time") is not None]
    p("| %s | %s | %.2f | %.1f | %.1f | %.2f | %.5f | %.0f (%.0f / %.0f) | %.1f (%.1f) | %s | %d/%d | %.1f | %.1f | %.0f | %.5f | %.5f | %.5f | %.5f |" % (
        c, lab, mean(nm, "v_p50"), mean(nm, "v_p99"), mean(nm, "v_p999"), mean(nm, "v_mean"), mean(nm, "G_ratio"),
        g("passes"), g("pass_floor"), g("pass_r0e"), g("p1_viol"), g("p1_worst"),
        "%.1f" % (np.mean(tt) * 1e3) if tt else "n/a", sum(1 for r in rr if r["stalled"]), len(rr),
        np.mean([r["tail_verdicts"] for r in rr]), np.mean([r["tail_filled_after_window"] for r in rr]), g("r0e_fallback"),
        mean(nm, "repair_per_delivered"), np.mean([x["GF_ratio"] for x in rr]), mean(nm, "floors_per_delivered"), mean(nm, "copies_per_delivered")))


def contrasts(c, nmf, refname):
    nf = S.nf_table()
    p("| cell | contrast | metric | difference [CI] | m | reading |")
    p("|---|---|---|---|---|---|")
    for a, b in (("R0e", "R0c"), ("R0e", "R1"), ("R0c", "R1"), ("R0e", "U"), ("R0c", "U"), ("R1", "U")):
        na, nb = nmf(a), nmf(b)
        ss = sorted(set(S.seeds_of(na)) & set(S.seeds_of(nb)))
        for k in ("v_p50", "v_p99", "v_p999", "v_mean", "G_ratio", "max_ep_spend", "repair_per_delivered", "tail_time"):
            x = S.vals(na, k, ss) - S.vals(nb, k, ss)
            r0 = R(na)
            T = S.T_of(k, "I2" if c.startswith("S") else c, 1.0 - r0["one_minus_rho"], r0["n_src"], r0["delivered"], refname)
            m = max(T, nf["tail"].get(k, 0.0))
            rd = S.read(x, m)
            p("| %s | %s - %s | %s | %.4g [%.4g, %.4g] | %.3g | %s |" % (c, a, b, k, rd["mean"], rd["lo"], rd["hi"], m, rd["word"]))


def tailsplit(label, na, nb, tail):
    """Paired per-seed split of the tail-only mean wait to release (delivery, or the pass if the seq was passed) R0c - R0e: seqs with a verdict under na but not under nb, against the rest."""
    d0 = 12147 - tail
    ss = sorted(set(S.seeds_of(na)) & set(S.seeds_of(nb)))
    tot, n1l, wa1, wb1, d1, dr = [], [], [], [], [], []
    for s in ss:
        ra, rb = R(na, s), R(nb, s)
        va, vb = set(ra["tail_verdicted"]), set(rb["tail_verdicted"])
        g1 = sorted(va - vb)
        ta, tb = ra["tail_res"], rb["tail_res"]
        ok = [i for i in range(len(ta)) if ta[i] is not None and tb[i] is not None]
        in1 = [i for i in ok if (d0 + i) in set(g1)]
        rest = [i for i in ok if (d0 + i) not in set(g1)]
        tot.append(np.mean([ta[i] - tb[i] for i in ok]) * 1e3)
        n1l.append(len(g1))
        if in1:
            wa1.append(np.mean([ta[i] for i in in1]) * 1e3)
            wb1.append(np.mean([tb[i] for i in in1]) * 1e3)
            d1.append(np.mean([ta[i] - tb[i] for i in in1]) * 1e3)
        dr.append(np.mean([ta[i] - tb[i] for i in rest]) * 1e3)
    f = lambda x: "%.2f [%.2f, %.2f]" % S.boot(x)[:3]
    p("| %s | %s | %.1f | %s | %s | %s | %s | %s |" % (label, f(tot), np.mean(n1l), "%.1f" % np.mean(wa1) if wa1 else "n/a", "%.1f" % np.mean(wb1) if wb1 else "n/a",
                                                     f(d1) if d1 else "n/a", f(dr), f(tot)))


def part_T():
    nf = S.nf_table()
    p("### Part T: per-config means over 12 seeds (times in ms)\n")
    head()
    for c in ("I1", "I2", "L10"):
        for rc, lab in RECVS:
            row(c, lab, "T-%s-%s" % (c, rc))
        row(c, "S0 reference", "REFT-%s" % c)
    p("\n### Part T: the early-skip hold. Passes decided while the budget had lengthened an entry's hold (global-hold model, per run)\n")
    p("| cell | receiver | entries whose hold was lengthened while buffered | early-skip passes | passes decided by a lengthened hold | hold evaluations, budget open | hold evaluations, fallen back |")
    p("|---|---|---|---|---|---|---|")
    for c in ("I1", "I2", "L10"):
        for rc in ("R0e", "R0c"):
            nm = "T-%s-%s" % (c, rc)
            rr = [R(nm, s) for s in S.seeds_of(nm)]
            g = lambda k: np.mean([r["gauges"].get(k, 0) for r in rr])
            p("| %s | %s | %.0f | %.1f | %.1f | %.0f | %.0f |" % (c, rc, g("r0e_entries_lengthened"), g("pass_r0e"), g("pass_r0e_lengthened"), g("r0e_budget_open"), g("r0e_fallback")))
    p("\nTail-only waits (the lost tail seqs; ms): mean v / median v")
    for c in ("I1", "I2", "L10"):
        for rc, lab in RECVS[:4]:
            nm = "T-%s-%s" % (c, rc)
            p("- %s %s: %.1f / %.1f" % (c, rc, mean(nm, "tail_v_mean"), mean(nm, "tail_v_p50")))
    p("\n### Part T readings: paired per-seed differences against m = max(T, NF) (NF from the I2 A/A, frozen)\n")
    for c in ("I1", "I2", "L10"):
        contrasts(c, lambda rc, c=c: "T-%s-%s" % (c, rc), "REFT-%s" % c)
    p("\n### Tail-only wait, R0c - R0e, paired per seed (ms): split into the seqs that get a verdict under R0c but not under R0e, and the rest\n")
    p("tail_time is the time to the last fill and is set by the resent tail; it cannot separate (c) from today here, because 347 lost tail seqs against a 121 allowance leave at least 219 resent under every design. The tail-only wait is read per seq.\n")
    p("| cell | R0c - R0e tail-only mean wait [CI] | seqs with a verdict under R0c only (per run) | their mean wait under R0c | their mean wait under R0e | difference per such seq [CI] | difference over the other tail seqs [CI] | overall [CI] |")
    p("|---|---|---|---|---|---|---|---|")
    for c in ("I1", "I2", "L10"):
        tailsplit(c, "T-%s-R0c" % c, "T-%s-R0e" % c, 347)
    tailsplit("short window (70 lost), I2", "TS-R0c", "TS-R0e", 70)
    p("\n### Short-window cell (reported only): I2 conditions, the last 70 seqs lost for 1.5 s\n")
    p("70 lost tail seqs is 0.58 of the 121-seq allowance and 0.78 of the roughly 90 that I2's earlier losses leave free (R1 gets 89.7 tail verdicts in the full cell), so the whole lost tail fits the allowance where the floor can pass it. Chosen by hand from that arithmetic; other windows were not tried.\n")
    head()
    for rc, lab in RECVS[:4]:
        row("S70", lab, "TS-%s" % rc)
    row("S70", "S0 reference", "TSREF-R1")
    p("")
    contrasts("S70", lambda rc: "TS-%s" % rc, "TSREF-R1")


def part_REP():
    p("### Fallback control (I2 tail cell, 12 seeds): eps-hat forced to 0 (budget always spent) and to 1 (never spent), per run\n")
    p("| row | early-skip passes | floor passes | passes decided by a lengthened hold | entries lengthened while buffered | hold evaluations fallen back | P1 violations | v mean ms | v p50 ms | tail time ms |")
    p("|---|---|---|---|---|---|---|---|---|---|")
    rows = [("R0e, global hold, engine-like eps-hat", "T-I2-R0e"), ("R0c, global hold, engine-like eps-hat", "T-I2-R0c"),
            ("R0e, global hold, eps-hat = 0", "CTL-I2-R0e-zero"), ("R0c, global hold, eps-hat = 0", "CTL-I2-R0c-zero"),
            ("R0e, global hold, eps-hat = 1", "CTL-I2-R0e-one"), ("R0c, global hold, eps-hat = 1", "CTL-I2-R0c-one"),
            ("R0e, per-entry hold (history), engine-like eps-hat", "T-I2-R0eentry"), ("R0e, per-entry hold (history), eps-hat = 0", "CTLentry-I2-R0e-zero"),
            ("R0e, per-entry hold (history), eps-hat = 1", "CTLentry-I2-R0e-one"), ("R1", "T-I2-R1")]
    for lab, nm in rows:
        rr = [R(nm, s_) for s_ in S.seeds_of(nm)]
        g = lambda k: np.mean([r["gauges"].get(k, 0) for r in rr])
        tt = np.mean([r["tail_time"] for r in rr]) * 1e3
        p("| %s | %.1f | %.1f | %.1f | %.0f | %.0f | %.1f | %.2f | %.2f | %.1f |" % (lab, g("pass_r0e"), g("pass_floor"), g("pass_r0e_lengthened"), g("r0e_entries_lengthened"), g("r0e_fallback"),
                                                                                    g("p1_viol"), mean(nm, "v_mean"), mean(nm, "v_p50"), tt))
    p("")
    R2.part_REP()


def main(cmd):
    {"T": part_T, "REP": part_REP}[cmd]()
