"""Plain-words per-cell tables for results-stage3c.md (seed means over 12 seeds).  python tables3b.py > fragment.md"""
import math

import summarize3c as S

G = S.gname
INF = math.inf

SND = {
    "C1": "today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip)",
    "C3": "never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order",
    "C3S": "S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity)",
    G(0.4, 0.0): "sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0)",
    G(0.4, INF): "sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes",
    G(2.0, 0.0): "sender gives up at 2 s; receiver any order",
    G(2.0, INF): "sender gives up at 2 s; receiver strict order",
    G(INF, 0.0): "sender never gives up (old tail sweep); receiver any order",
    "S4b": "as 'gives up at 0.4 s, strict order', but the floor goes as its own datagram, at most one per 2 ms, taking link capacity",
    G(0.4, 0.02): "sender gives up at 0.4 s; receiver holds a hole 20 ms",
}
ARMS = ["C1", "C3", "C3S", G(0.4, 0.0), G(0.4, INF), G(2.0, 0.0), G(2.0, INF), G(INF, 0.0), "S4b", G(0.4, 0.02)]


def pct(x):
    return "%.1f %%" % (100 * x) if x >= 0.0005 or x == 0 else "%.2f %%" % (100 * x)


def ms(x):
    if x is None or (isinstance(x, float) and math.isnan(x)):
        return "n/a"
    if x == INF:
        return "never (inf)"
    return "%.0f" % x if x >= 10 else "%.1f" % x


def table(cell):
    print("#### %s\n" % cell)
    link = cell in ("CL", "OV")
    print("| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |" + (" link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget |" if link else ""))
    print("|---|---|---|---|---|---|---|" + ("---|---|---|" if link else ""))
    for a in ARMS:
        if not S.seeds_of(cell, a):
            continue
        m = lambda k: S.mean(cell, a, k)      # noqa: E731
        dh = [S.vals(cell, a, k) for k in S.DH]
        # a quantile is shown as never (inf) if it is infinite in every seed, and as 'inf in n of 12' if in some
        q = []
        for v in dh:
            ninf = int((v == INF).sum())
            q.append(ms(INF) if ninf == len(v) else ("%s (never in %d of %d seeds)" % (ms(float(v[v < INF].mean())), ninf, len(v)) if ninf else ms(float(v.mean()))))
        extra = (" %s | %.0f / %.0f | %.0f |" % (pct(m("link_util_w")), m("queue_drop_out"), m("queue_drop_in"), m("resend_braked"))) if link else ""
        print("| `%s`: %s | %s | %s | %s | %s | %.3g | %.3g |%s" % (a, SND[a], pct(m("share_any")), pct(m("share_by_0p4")), pct(m("share_by_2")),
                                                                " / ".join(q), m("repair_per_delivered"), m("ooo_per_delivered"), extra))
    print()


def main():
    for cell in ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10", "K-L10b", "E-I2n", "CL", "OV"):
        table(cell)


if __name__ == "__main__":
    main()
