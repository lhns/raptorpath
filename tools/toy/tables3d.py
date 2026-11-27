"""Plain-words per-cell tables for results-stage3d.md (seed means over 12 seeds).  python tables3d.py > fragment.md
Every arm is labelled by what its sender and its receiver do."""
import math
import sys

import numpy as np

import summarize3d as D
import report_s3d as RP

S = D.S
G = S.gname
INF = math.inf

SW_TAIL = "resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms)"
SW_BLK = "resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes"

SND = {
    "C1": "sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early)",
    "C3": "sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole)",
    "C3S": "sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order",
    G(0.4, 0.0): "sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0)",
    G(0.4, INF): "sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it",
    G(2.0, 0.0): "sender: S5, lifetime 2 s; receiver: any order (H = 0)",
    G(2.0, INF): "sender: S5, lifetime 2 s; receiver: strict order",
    G(INF, 0.0): "sender: S5 with no lifetime (never gives up, no floor); receiver: any order",
    G(0.4, 0.02): "sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing",
    "S5v-H0": "ABLATION: S5's lifetime 0.4 s with ADR-0084's verdict floor (a floor datagram sent at a verdict, the feedback give-up rule) instead of the protected floor; receiver: any order",
    "S5v-Hinf": "ABLATION: as S5v with H = 0, receiver: strict order",
}
ARMS = ["C1", "C3", "C3S", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), G(0.4, 0.02), "S5v-H0", "S5v-Hinf"]


def pct(x):
    return "%.1f %%" % (100 * x) if x >= 0.0005 or x == 0 else "%.2f %%" % (100 * x)


def ms(x):
    if x is None or (isinstance(x, float) and math.isnan(x)):
        return "n/a"
    if x == INF:
        return "never (inf)"
    return "%.0f" % x if x >= 10 else "%.1f" % x


def recs(cell, a):
    v = RP.rec(cell, a)
    if all(x is None for x in v):
        return S.R(cell, a, S.seeds_of(cell, a)[0]).get("recovery_status") or "n/a"
    return RP.recfmt(v)


def table(cell, with_rec=False):
    form = SW_BLK if cell.endswith("@b") else SW_TAIL
    print("#### %s\n" % cell)
    print("Sweep for every arm in this cell: %s.\n" % form)
    link = cell.startswith(("CL", "OV"))
    hdr = "| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |"
    sep = "|---|---|---|---|---|---|---|"
    if link:
        hdr += " link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy |"
        sep += "---|---|---|---|"
    if with_rec:
        hdr += " recovery time after the outage (s) |"
        sep += "---|"
    print(hdr)
    print(sep)
    for a in ARMS:
        if not S.seeds_of(cell, a):
            continue
        m = lambda k: S.mean(cell, a, k)      # noqa: E731
        q = []
        for k in S.DH:
            v = S.vals(cell, a, k)
            ninf = int((v == INF).sum())
            q.append(ms(INF) if ninf == len(v) else ("%s (never in %d of %d seeds)" % (ms(float(v[v < INF].mean())), ninf, len(v)) if ninf else ms(float(v.mean()))))
        extra = ""
        if link:
            extra = " %s | %.0f / %.0f | %.0f | %.3f |" % (pct(m("link_util_w")), m("queue_drop_out"), m("queue_drop_in"), m("resend_braked"),
                                                           RP.ratio(RP.gv(cell, a, "dup"), RP.gv(cell, a, "repair_sends")))
        if with_rec:
            extra += " %s |" % recs(cell, a)
        print("| `%s`: %s | %s | %s | %s | %s | %.3g | %.3g |%s" % (a, SND[a], pct(m("share_any")), pct(m("share_by_0p4")), pct(m("share_by_2")),
                                                                " / ".join(q), m("repair_per_delivered"), m("ooo_per_delivered"), extra))
    print()


P2_LBL = {"est": "receiver estimates the floor hold from measured reordering (H_f, window 10 s)", "0": "receiver passes at the floor at once (H_f = 0)"}


def p2():
    print("#### P2\n")
    print("Two paths (fast 10 ms, slow 60 ms, half the first sends on each, every copy and the reports on the fast path, 3 % loss on each), 10 000 units per second, 12 147 units, no outage, no link limit. Every arm resends only a report-listed hole whose last send is older than the measured skew before the newest received send (RACK's time test).\n")
    print("| arm (sender; receiver) | delivered at all | within T | within 0.4 s | delay p50 / p90 / p99 from hand-over (ms) | out of order per delivered | late fills of floor-passed holes per unit | floor passes per unit | late fills among units handed over after 1 s | H_f measured (ms) | duplicates per copy |")
    print("|---|---|---|---|---|---|---|---|---|---|---|")
    units = S.R("P2", "C3", 0)["units"]
    arms = ["C3", "C3S"] + [RP.hf_arm(T, H, hf) for T in (0.075, 0.4, INF) for H in (0.0, INF) for hf in ("est", "0")]
    for a in arms:
        if not S.seeds_of("P2", a):
            continue
        m = lambda k: S.mean("P2", a, k)      # noqa: E731
        if a == "C3":
            lbl = "sender: never gives up (rho = 1); receiver: strict order"
        elif a == "C3S":
            lbl = "sender: S5 at T = infinity (never gives up); receiver: strict order (bit-identical to C3)"
        else:
            _, T, H, hf = a.split("-")[0:1] + a.split("-")[1:4]
            Tn = {"Tinf": "never gives up", "T0p075": "gives up at 0.075 s", "T0p4": "gives up at 0.4 s"}[T]
            Hn = "strict order" if H == "Hinf" else "any order"
            lbl = "sender: S5 %s; receiver: %s, %s" % (Tn, Hn, P2_LBL[hf[1:]])
        q = " / ".join(ms(float(S.vals("P2", a, k).mean())) for k in S.DH)
        gg = lambda k: RP.gv("P2", a, k)      # noqa: E731
        print("| `%s`: %s | %s | %s | %s | %s | %.3g | %.4f | %.4f | %.2f | %.1f | %.3f |" % (
            a, lbl, pct(m("share_any")), pct(m("share_T")), pct(m("share_by_0p4")), q, m("ooo_per_delivered"), gg("late_floor") / units, gg("pass_floor") / units,
            gg("late_floor_ge1"), 1e3 * m("H_max"), RP.ratio(gg("dup"), gg("repair_sends"))))
    print()


def main():
    for cell in ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10", "K-L10b"):
        table(cell)
    table("K-I2@b")
    table("K-L10@b")
    table("K-L10b@b")
    table("CL", True)
    table("CL@b", True)
    table("OV")
    table("OV@b")
    table("CL-long", True)
    table("CL-long@b", True)
    table("OV-long")
    table("OV-long@b")
    p2()


if __name__ == "__main__":
    main()
