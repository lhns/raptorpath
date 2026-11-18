"""Count rule (count-rule.md §3): the floor minimum and the knee need at equal sample counts, on every ledger that has window minima.

Usage (from docs/):  python -I -X utf8 derivations/store-law/crrule.py l1-raw/s62/rep62r0-diag.tgz l1-raw/s61/rep61c-diag-ld.tgz \
                         l1-raw/s61/rep61a-diag-ld.tgz l1-raw/s60/rep60a-diag-ld.tgz l1-raw/s60/rep60b-diag-ld.tgz  > derivations/store-law/crrule.out

Each ledger is loaded on its own (row keys collide across sessions). Window parse: r68allow.parse_log (via s61pred, [SLAW] + [WSTAT]); groups:
s61pred.groups (G(n*): consecutive LATE windows pooled until their count reaches n*). Sections:
  1. the floor minimum against the count at J = 0 (s62's floor pair and the D12 dump row) and at J = 5 (61c, 61a), c2 and c1s
  2. the knee need at equal counts (a window ladder: the knee arms pooled over k windows, the floor grouped to the knee's realised count),
     61c, 61a and c2; and at J = 0 the probe rows W 80 / W 128 against the floor at equal counts
  3. the rule's check: the count dependence of each reading above n0, for n0 = 250 / 500 / 1 000
"""
import os
import re
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import r68allow as ra  # noqa: E402
import s61pred as sp  # noqa: E402

med, interp, cross_up = ra.med, ra.interp, ra.cross_up
P = print
C3, C3J0, C2, C1S = "c3-25", "c3j0-25", "c2-100", "c1s-400"
ra.LATE_T[C3] = 15.0
ra.LATE_T[C3J0] = 15.0
NAME = re.compile(r"(?:diag-ld|diag-smoke)/(c\w+-\d+)@def-([GCD])(\d+)-s(\d+)-r(\d+)-a\d+-c\.log$")
COUNTS = (None, 250, 500, 1000, 2100, 4200)


def load_any(path):
    """[SLAW]/[WSTAT] windows of every c.log in diag-ld/ or diag-smoke/ (the s62 rehearsal ledger uses diag-smoke/); goodput from ra.load where named."""
    rows = {}
    with tarfile.open(path, "r:gz") as tf:
        for mb in tf:
            g = NAME.search(mb.name)
            if not g or not mb.isfile():
                continue
            r = ra.Row(g.group(1), g.group(2), int(g.group(3)), g.group(4), int(g.group(5)))
            sp.parse_log(tf.extractfile(mb).read().decode("utf-8", "replace").splitlines(), r)
            rows[(r.cell, r.kind, r.W, r.seed, r.rep)] = r
    gp = ra.load([path])
    for k, r in rows.items():
        if k in gp:
            r.gp = gp[k].gp
    return rows


def F_at(r, ns):
    return sp.row_SF(r, 0, ns)[1]


def span_min(r, k):
    """median over contiguous k-window groups of the group minimum, and the median realised count."""
    w = [x for x in sp.late_all(r, 0) if x["tmin"] is not None]
    g = [(min(x["tmin"] for x in w[i:i + k]), sum(x["nk"] for x in w[i:i + k])) for i in range(0, len(w) - k + 1, k)]
    return (med([a for a, _ in g]), med([b for _, b in g]), len(g)) if g else (None, None, 0)


def ladder(rows, cell, floor_w, karms, p1, ctl, ks):
    """The knee need at equal counts: per k, the knee arms' k-window minima interpolated at each replicate's W95, the floor arm grouped to the knee's
    realised count. -> [(k, n_K, n_F, groups per floor row, T, F, L)]"""
    G = [r for r in rows.values() if r.cell == cell and r.kind == "G"]
    reps = {}
    for r in G:
        reps.setdefault((r.rep, r.seed), {})[r.W] = r
    ctl_m = med([r.gp for r in G if r.W == ctl and r.gp])
    knees = []
    for key, wm in reps.items():
        c_ = wm.get(ctl)
        gref = c_.gp if c_ and c_.gp else ctl_m
        w95 = cross_up([(W, r.gp / gref) for W, r in wm.items() if W != ctl and r.gp], 0.95)
        if w95 is not None and karms[0] <= w95 <= karms[-1]:
            knees.append((w95, wm))
    fl = [r for r in G if r.W == floor_w]
    out = []
    for k in ks:
        Ts, nks = [], []
        for w95, wm in knees:
            arms = [W for W in tuple(karms) + (p1,) if W in wm]
            v = [(W, span_min(wm[W], k)) for W in arms]
            if any(x[0] is None for _, x in v):
                continue
            Ts.append(interp([(W, x[0]) for W, x in v], w95))
            nks += [x[1] for W, x in v if W in karms]
        if not Ts:
            continue
        nK = int(med(nks))
        F = med([F_at(r, nK) for r in fl])
        nF = med([sp.row_SF(r, 0, nK)[2] for r in fl])
        gr = med([sp.row_SF(r, 0, nK)[3] for r in fl])
        out.append((k, nK, nF, gr, med(Ts), F, med(Ts) - F))
    return out, len(knees)


def main(argv):
    s62, r61c, r61a, r60a, r60b = (load_any(p) for p in argv[1:6])
    P("count rule (crrule.py): the floor minimum and the knee need at equal counts; inputs " + ", ".join(a.replace("\\", "/").split("/")[-1] for a in argv[1:6]))

    # ── 1. the floor minimum against the count ──
    P("\n== 1. The floor minimum (median over the arm's rows of the row's median group minimum, ms) against the count: native, G(250 ... 4 200) ==")
    sets = (("c3 J = 0 (s62) W12 floor pair", s62, C3J0, "G", 12), ("c3 J = 0 (s62) D12 dump row (floor W12, dump on)", s62, C3J0, "D", 12),
            ("c3 J = 5 61c W12", r61c, C3, "G", 12), ("c3 J = 5 61a W12", r61a, C3, "G", 12),
            ("c2 s60 W16", r60a, C2, "G", 16), ("c1s s60 W64", r60b, C1S, "G", 64))
    for lab, rows, cell, kind, W in sets:
        rs = [r for r in rows.values() if r.cell == cell and r.kind == kind and r.W == W]
        vals = [med([F_at(r, ns) for r in rs]) for ns in COUNTS]
        ns_ = [med([sp.row_SF(r, 0, ns)[2] for r in rs]) for ns in COUNTS]
        P(f"  {lab:<50} rows {len(rs)}: " + " | ".join(f"{'native' if c is None else 'G' + str(c)} {v:.3f} (n {n:.0f})" for c, v, n in zip(COUNTS, vals, ns_)))
        P(f"  {'':<50} steps: native->250 {vals[0] - vals[1]:+.3f} | 250->500 {vals[1] - vals[2]:+.3f} | 500->1000 {vals[2] - vals[3]:+.3f} |"
          f" 1000->2100 {vals[3] - vals[4]:+.3f} | 2100->4200 {vals[4] - vals[5]:+.3f}")

    # ── 2. the knee need at equal counts ──
    P("\n== 2. The knee need at equal counts (window ladder; ms). k = knee windows pooled; n_K / n_F the realised counts; groups = complete floor groups per row ==")
    for lab, rows, cell, fw, karms, p1, ctl, ks in (("c3 J = 5 61c", r61c, C3, 12, (100, 108, 120), 152, 512, (1, 2, 4, 8)),
                                                     ("c3 J = 5 61a", r61a, C3, 12, (104, 112, 124), 136, 2048, (1, 2, 4, 8)),
                                                     ("c2 s60", r60a, C2, 16, (160, 176, 192), 256, 2048, (1, 2, 4))):
        lad, nk = ladder(rows, cell, fw, karms, p1, ctl, ks)
        P(f"  {lab} ({nk} replicates with W95 inside the bracket; floor W{fw}):")
        for k, nK, nF, gr, T, F, L in lad:
            P(f"    k {k}: n_K {nK} n_F {nF:.0f} (floor groups/row {gr:.0f}) T {T:.3f} F {F:.3f} need {L:.3f}")
        Ls = [x[6] for x in lad if x[3] >= 3]
        if Ls:
            P(f"    need range over the steps with >= 3 floor groups per row: {min(Ls):.3f} - {max(Ls):.3f} (spread {max(Ls) - min(Ls):.3f} ms)")
    P("  c3 J = 0 (s62 rehearsal, one row per probe; no knee bracket exists, so this is the probes' rise above the floor at equal counts, not a need):")
    fl = [r for r in s62.values() if r.cell == C3J0 and r.kind == "G" and r.W == 12]
    for W in (80, 128):
        pr = [r for r in s62.values() if r.cell == C3J0 and r.kind == "G" and r.W == W]
        for k in (1, 2, 4, 8):
            v = [span_min(r, k) for r in pr]
            if any(x[0] is None for x in v):
                continue
            nK = int(med([x[1] for x in v]))
            F = med([F_at(r, nK) for r in fl])
            P(f"    W{W} k {k}: n {nK} T {med([x[0] for x in v]):.3f} F(n) {F:.3f} rise {med([x[0] for x in v]) - F:.3f}")

    # ── 2b. every arm's minimum against the count, J = 0 beside J = 5 ──
    P("\n== 2b. Every arm's minimum against the count (median over the arm's rows; ms): J = 0 (s62) beside J = 5 (61c) ==")
    for lab, rows, cell, arms in (("J = 0 s62", s62, C3J0, (12, 80, 128, 512)), ("J = 5 61c", r61c, C3, (12, 24, 60, 80, 88, 100, 108, 120, 152, 512))):
        for W in arms:
            rs = [r for r in rows.values() if r.cell == cell and r.kind == "G" and r.W == W]
            if not rs:
                continue
            vals = [med([F_at(r, ns) for r in rs]) for ns in COUNTS]
            P(f"  {lab} W{W:<4} rows {len(rs)}: " + " | ".join(f"{'nat' if c is None else c} {v:.2f}" for c, v in zip(COUNTS, vals))
              + f"   native -> 4200: {vals[0] - vals[-1]:+.2f}, 500 -> 4200: {vals[2] - vals[-1]:+.2f}")

    # ── 4. the law's floor, read from the [SLAW] fields ──
    P("\n== 4. The law's floor on each floor arm, from the printed inputs (LATE medians; ms): m = Copa's 10 s min_rtt (m-hat), l = the lifetime minimum"
      " (l-hat), b0p = b-hat0+ (max over the first K = 8 windows of tmin - m); R6.2 (N = 1): F = min(m, l + b0p) + b0p; R6.7 candidate C on a static row:"
      " the median of the last K = 4 native window minima ==")
    for lab, rows, cell, kind, W in sets:
        rs = [r for r in rows.values() if r.cell == cell and r.kind == kind and r.W == W]
        Fr, Cr, mm, ll, bb = [], [], [], [], []
        for r in rs:
            lw = r.late(0)
            f62 = [min(w["m"], w["l"] + w["b0p"]) + w["b0p"] for w in lw if None not in (w.get("m"), w.get("l"), w.get("b0p"))]
            tm = [w["tmin"] for w in lw]
            cv = [med(tm[i - 3:i + 1]) for i in range(3, len(tm))]
            Fr.append(med(f62))
            Cr.append(med(cv))
            mm.append(med([w["m"] for w in lw if w.get("m") is not None]))
            ll.append(med([w["l"] for w in lw if w.get("l") is not None]))
            bb.append(med([w["b0p"] for w in lw if w.get("b0p") is not None]))
        nat, g250, g2100 = (med([F_at(r, ns) for r in rs]) for ns in (None, 250, 2100))
        P(f"  {lab:<50} m {med(mm):.3f} l {med(ll):.3f} b0p {med(bb):.3f} -> R6.2 F {med(Fr):.3f} | C F-hat {med(Cr):.3f} || F native {nat:.3f} G250 {g250:.3f} G2100 {g2100:.3f}")
    P("  samples each input takes its minimum over (LATE sample rate of the floor arm x its span):")
    for lab, rows, cell, kind, W in sets:
        rs = [r for r in rows.values() if r.cell == cell and r.kind == kind and r.W == W]
        rate = med([med([w["nk"] for w in r.late(0)]) / 0.25 for r in rs])
        P(f"  {lab:<50} {rate:.0f} samples/s: native window (250 ms) {rate * 0.25:.0f}; m-hat (10 s) {rate * 10:.0f}; l-hat (lifetime, at t = 15 s) >= {rate * 15:.0f}")

    # ── 3. the rule's check ──
    P("\n== 3. The rule's check: each reading's count dependence above n0 (the value at n0 minus the value at 8 n0, or the largest count available) ==")
    for lab, rows, cell, kind, W in sets:
        rs = [r for r in rows.values() if r.cell == cell and r.kind == kind and r.W == W]
        parts = []
        for n0 in (250, 500, 1000):
            hi = min(8 * n0, 4200)
            parts.append(f"n0 {n0}: F(n0) {med([F_at(r, n0) for r in rs]):.3f}, F(n0) - F({hi}) {med([F_at(r, n0) for r in rs]) - med([F_at(r, hi) for r in rs]):+.3f}")
        P(f"  floor {lab:<46} " + " | ".join(parts))


if __name__ == "__main__":
    main(sys.argv)
