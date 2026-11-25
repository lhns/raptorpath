"""Stage-3e reader (three-knobs-stage3e-build.md 8.5; registration 13.3).  Reads out3e/runs through batch3e's fingerprints.

    python summarize3e.py freeze [--force]            # A/A -> nf_stage3e.json (once, before any decision part), writes PASS-aa
    python summarize3e.py read CELL [--modes OFF,RL,RE] [--nseeds 12] [--write-undet FILE] [--out results-stage3e.md]
    python summarize3e.py verdict CELL[,CELL...]       # the stage verdict from the cells' outcomes

The curve functions are proof_s1's (val, vec, boot, read_good, t_bar, nf_of, merge_points, curve_at, compare, compare_rev, classify, bracket);
this file only keys the rows, reads the 0.12 s and 2 s on-time shares, and applies registration 13.3's scope and doubling.
"""
import argparse
import json
import math
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import proof_s1 as P  # noqa: E402
from proof_s1 import val, vec, boot, read_good, t_bar, nf_of, merge_points, curve_at, compare, compare_rev, classify, bracket  # noqa: E402,F401

NF_PATH = os.path.join(HERE, "nf_stage3e.json")
INF = math.inf
SEEDS = 12
P.BETTER.update({"share_by_2": +1.0, "share_by_0p12": +1.0, "share_any": +1.0})      # higher is better; recovery_time is already lower-is-better


# ---- values ------------------------------------------------------------------------------------------------------------------------------
def rec_cap(cell_kw):
    """The readable end of a cell's recovery: the run's length less 2 s less the outage's end (14.5 s in CL-long)."""
    d = cell_kw["dead"]
    return cell_kw["n_total"] / cell_kw["rate"] - 2.0 - (d[1] + d[2])


def rec_value(r, cap):
    """recovery_time None ("not within the run") is censored at the readable end, counted, never dropped."""
    v = r.get("recovery_time")
    return cap if v is None else float(v)


def value(r, k, cap):
    if k == "recovery_time":
        return rec_value(r, cap)
    if k == "share_by_0p12":
        return float(r["rep3e"]["share_by"]["0.12"])
    if k in r and r[k] is not None:
        return float(r[k])
    if k in r.get("gauges", {}):
        return float(r["gauges"][k])
    return float(r["rep3e"][k])


def nf_from(A, Ap, cap, keys):
    """The A/A floor: the 95th percentile over seeds of |A - A'| per readout (A, Ap: seed -> result)."""
    out = {}
    for k in keys:
        d = np.abs(np.asarray([value(A[s], k, cap) for s in sorted(A)]) - np.asarray([value(Ap[s], k, cap) for s in sorted(Ap)]))
        out[k] = float(np.percentile(d, 95))
    return out


def combine_floors(tinf, primary):
    """The larger of the two A/A floors per readout.  A pair whose floor is zero (an on-time share that is 0 by construction under overload at T = inf, a recovery
    at the readout's resolution) gives way to the other pair's.  Returns (floors, readouts whose floor is zero in both: their comparisons are reported only)."""
    cells = {k: max(tinf[k], primary[k]) for k in tinf}
    return cells, [k for k, v in cells.items() if v == 0.0]


def stage_verdict(outcomes, better_somewhere):
    """Registration 13.3 (plan 1.3) with the coordinator's reading: FAILS if any cell fails; UNDETERMINED if any cell is undetermined; otherwise PASSES
    when at least one comparison outside the identity cells reads shown better, else UNDETERMINED."""
    if any(v == "FAILS" for v in outcomes.values()):
        return "FAILS"
    if any(v == "UNDETERMINED" for v in outcomes.values()):                  # as proof stage 1: an undetermined cell leaves the stage undetermined
        return "UNDETERMINED"
    if better_somewhere:
        return "PASSES"
    return "UNDETERMINED"


# ---- one cell in one repair mode ----------------------------------------------------------------------------------------------------------
def nf_n(nfall, cell, n):
    """The frozen A/A floor of a cell for a comparison of n seeds (registration 14.3): the freeze at the largest count not above n (the A/A rows
    run at 12 seeds, so 24 reads the 12-seed floor), else the smallest count."""
    keys = sorted(int(k) for k in nfall["by_n"])
    use = max([k for k in keys if k <= n], default=keys[0])
    return nfall["by_n"][str(use)]["cells"][cell]


def nf_degenerate(nfall, cell, n):
    keys = sorted(int(k) for k in nfall["by_n"])
    use = max([k for k in keys if k <= n], default=keys[0])
    return tuple(nfall["by_n"][str(use)].get("degenerate", {}).get(cell, ()))


def write_verdict_marker(out_dir, verdict):
    """ROBUST and SMOOTH wait for this marker (registration 14.5): present only while the stage verdict reads PASSES."""
    path = os.path.join(out_dir, "VERDICT-PASSES")
    if verdict == "PASSES":
        os.makedirs(out_dir, exist_ok=True)
        open(path, "w").close()
    elif os.path.exists(path):
        os.remove(path)


def read_mode(R, nf, units, ys, nseeds, rec_cap, rt_verdict=(50.0,), gates_bad=False, degenerate=(), base_n=None):
    """R: row key -> seed -> result, keys ("lt", T, 0.0) (the new curve), ("adr", rho), ("rt", delta), ("bulk",).  The object API is the new
    curve's own T = inf, H = 0 point.  ADR-0084 (curve and points), today's Bulk and the object API are under the verdict in every mode;
    today's Realtime only at the deltas in rt_verdict (50 in RL and RE, none in OFF), elsewhere it is reported.
    nf is the floor dict, or a function of the comparison's seed count n (the floor is taken over the first n seeds).  base_n: baseline row key -> its
    seed count where that is below nseeds (the old senders of the long overload cells have 3 against the new curve's 6): the comparison runs on that
    count, and its one doubling goes to twice that count.
    A readout in `degenerate` has a zero A/A floor in the cell (both pairs): its comparisons are reported only, never under the verdict.
    Returns dict(outcome, comps, undet (row keys to double), worse, better)."""
    base_n = base_n or {}

    def have(key, ns):
        return key in R and all(s in R[key] for s in range(ns))

    def arr(keys, k, ns):
        return np.stack([[value(R[key][s], k, rec_cap) for s in range(ns)] for key in keys])

    new_all = sorted([k for k in R if k[0] == "lt" and k[2] == 0.0 and have(k, nseeds)], key=lambda k: k[1])
    base_keys = [k for k in R if k[0] in ("adr", "rt", "bulk") and have(k, base_n.get(k, nseeds))]
    if new_all and ("lt", INF, 0.0) in new_all:
        base_keys.append(("api",))
        R = dict(R)
        R[("api",)] = R[("lt", INF, 0.0)]
    nfn = nf if callable(nf) else (lambda n: nf)
    xn = lambda n: max(nf_of(nfn(n), "share_any"), 1.0 / units)              # noqa: E731
    comps, undet = [], set()

    def scope_of(key, k=None):
        if k in degenerate:
            return "reported"
        if key[0] == "rt":
            return "verdict" if key[1] in rt_verdict else "reported"
        return "verdict"

    for k in ys:
        for bk in base_keys:
            nb = base_n.get(bk, nseeds)

            def run(newk, ns):
                nx, ny = arr(newk, "share_any", ns), arr(newk, k, ns)
                kk = merge_points(nx, ny, P.BETTER[k], xn(ns))
                bx, by = arr([bk], "share_any", ns)[0], arr([bk], k, ns)[0]
                return compare(nx, ny, kk, bx, by, k, nf_of(nfn(ns), k), xn(ns), units, float(by.mean()), True), kk, nx, bx
            rd, kk, nx, bx = run(new_all, nb)
            if rd["word"] == "undetermined":
                arms = {bk} | {new_all[i] for i in bracket(nx.mean(1), kk, float(bx.mean()))}
                n2 = [n for n in new_all if have(n, 2 * nb)]
                if have(bk, 2 * nb) and len(n2) >= 2:                          # the registered doubling, on these arms only; never a second one
                    rd, _, _, _ = run(n2, 2 * nb)
                    rd["note"] = "doubled to %d seeds" % (2 * nb)
                else:
                    undet.update(a for a in arms if a != ("api",) and not have(a, 2 * nb))             # still to double
            comps.append(dict(scope=scope_of(bk, k), kind="point", base=bk, k=k, rd=rd))
        adr = [k2 for k2 in base_keys if k2[0] == "adr"]
        if len(adr) >= 2 and new_all:                                          # curve against curve: the new points inside ADR-0084's range
            na = min(base_n.get(k2, nseeds) for k2 in adr)
            ax, ay = arr(adr, "share_any", na), arr(adr, k, na)
            ak = merge_points(ax, ay, P.BETTER[k], xn(na))
            lo, hi = ax[ak].mean(1).min(), ax[ak].mean(1).max()
            for n in new_all:
                x = float(arr([n], "share_any", na).mean())
                if not (lo - 1e-12 <= x <= hi + 1e-12):
                    continue

                def runr(adrk, ns):
                    ax_, ay_ = arr(adrk, "share_any", ns), arr(adrk, k, ns)
                    ak_ = merge_points(ax_, ay_, P.BETTER[k], xn(ns))
                    px, py = arr([n], "share_any", ns)[0], arr([n], k, ns)[0]
                    return compare_rev(ax_, ay_, ak_, px, py, k, nf_of(nfn(ns), k), units, float(ay_.mean())), ax_, ak_
                rd, ax_, ak_ = runr(adr, na)
                if rd["word"] == "undetermined":
                    a2 = [m for m in adr if have(m, 2 * na)]
                    if have(n, 2 * na) and len(a2) >= 2:
                        rd, _, _ = runr(a2, 2 * na)
                        rd["note"] = "doubled to %d seeds" % (2 * na)
                    else:
                        undet.update(a for a in ({n} | {adr[i] for i in bracket(ax_.mean(1), ak_, x)}) if not have(a, 2 * na))
                comps.append(dict(scope="reported" if k in degenerate else "verdict", kind="curve", base=n, k=k, rd=rd))
    out, worse, und, better = classify(comps, gates_bad)
    return dict(outcome=out, comps=comps, undet=undet, worse=worse, undetermined=und, better=better)


# ---- the store, per cell --------------------------------------------------------------------------------------------------------------------
def cell_ys(cell):
    import batch3e as B
    base = B.base_of(cell)
    ys = ["share_by_2" if cell in B.VIDEO else "share_by_0p4"]
    if cell == "RT":
        ys.append("share_by_0p12")
    if B.CELLS[base][1].get("dead") is not None and cell not in B.FILE_CELLS:
        ys.append("recovery_time")
    return ys


def load_rows(cell, mode, nseeds):
    """(R, base_n): row key -> seed -> result for every design row of a cell in a repair mode that has its own first seeds on disk (3 for a delta = 0.005 identity
    row or an old sender of a long overload cell, else nseeds); seeds up to twice that count are loaded where the registered doubling has run them.
    base_n holds the rows with fewer seeds than nseeds.  Reused stage 3d rows come through batch3e.load."""
    import batch3e as B
    R, base_n = {}, {}
    for key in B.rows_of(cell):
        fp = B.fingerprint(B.row_builder(cell, key, mode)(0))
        own = min(nseeds, B.row_nseeds(cell, key, mode))
        rows = {}
        for s_ in range(2 * own):
            r = B.load(fp, s_)
            if r is None:
                break
            rows[s_] = r
        if len(rows) >= own:
            R[key] = rows
            if own < nseeds:
                base_n[key] = own
    return R, base_n


def cell_units(cell):
    import batch3e as B
    kw = B.CELLS[B.base_of(cell)][1]
    return kw["n_total"]


def cell_cap(cell):
    import batch3e as B
    kw = B.CELLS[B.base_of(cell)][1]
    return rec_cap(kw) if kw.get("dead") is not None else 0.0


def cmd_freeze(force=False, aa_at="tinf"):
    """The A/A floors, frozen at each count n in (3, 6, 12, 24) over the first n seeds (the A/A rows have 12, so 24 reads the 12-seed floor):
    both pairs (T = inf, and the primary lifetime), the larger per readout, and the readouts that are zero in both (reported only)."""
    import batch3e as B
    B.build()
    nf = dict(notes="p95 over the first n seeds of |A - A'|; A' = phase variant 1 of A; A = S5 at T=inf, hold 0 (tinf, build 8.5) or at the cell's primary lifetime "
                    "(primary, registration 5), repair off; the larger of the two per readout; degenerate = zero in both, comparisons on it are reported only",
              by_n={})
    rows = {}
    for cell in B.MAIN_CELLS + B.AA2_CELLS:
        base, d = B.base_of(cell), B.delta_of(cell)
        for at in ("tinf", "primary"):
            T = INF if at == "tinf" else B.primary_T(cell)
            A = {s_: B.load(B.fingerprint(B.new_cfg(base, 0, T, 0.0, d, None)), s_) for s_ in range(2 * SEEDS)}
            Ap = {s_: B.load(B.fingerprint(B.new_cfg(base, 0, T, 0.0, d, None, variant=1)), s_) for s_ in range(2 * SEEDS)}
            A, Ap = {k: v for k, v in A.items() if v is not None}, {k: v for k, v in Ap.items() if v is not None}
            if len(A) < SEEDS or len(Ap) < SEEDS:
                raise SystemExit("the A/A (%s) needs the %d seeds of both arms in %s" % (at, SEEDS, cell))
            rows[(cell, at)] = (A, Ap)
    for n in (3, 6, 12, 24):
        nf["by_n"][str(n)] = dict(cells={}, degenerate={}, tinf={}, primary={})
        for cell in B.MAIN_CELLS + B.AA2_CELLS:
            for at in ("tinf", "primary"):
                A, Ap = rows[(cell, at)]
                m = min(n, len(A), len(Ap))
                nf["by_n"][str(n)][at][cell] = nf_from({k: A[k] for k in range(m)}, {k: Ap[k] for k in range(m)}, cell_cap(cell), ["share_any", "share_T"] + cell_ys(cell))
            c, dg = combine_floors(nf["by_n"][str(n)]["tinf"][cell], nf["by_n"][str(n)]["primary"][cell])
            nf["by_n"][str(n)]["cells"][cell], nf["by_n"][str(n)]["degenerate"][cell] = c, dg
            if n == 12:
                for at in ("tinf", "primary"):
                    z = [k for k, v in nf["by_n"]["12"][at][cell].items() if v == 0.0]
                    if z:
                        print("degenerate floor (zero) in the %s pair of %s at 12 seeds: %s%s" % (at, cell, z, "; zero in both, comparisons on it are reported only" if set(z) & set(dg) else "; the other pair's floor is used"))
    nf["cells"] = nf["by_n"]["12"]["cells"]
    nf["degenerate"] = nf["by_n"]["12"]["degenerate"]
    if os.path.exists(NF_PATH) and not force:
        print("nf_stage3e.json exists; not overwriting (frozen)")
        return
    with open(NF_PATH, "w") as f:
        json.dump(nf, f, indent=1)
    os.makedirs(B.OUT, exist_ok=True)
    open(os.path.join(B.OUT, "PASS-aa"), "w").close()
    print("frozen", NF_PATH)


def cmd_read(cell, modes, nseeds, write_undet, out_path):
    import batch3e as B
    B.build()
    with open(NF_PATH) as f:
        nfall = json.load(f)
    nseeds = nseeds or B.cell_seeds(cell)
    nf, degenerate = (lambda n: nf_n(nfall, cell, n)), nf_degenerate(nfall, cell, nseeds)
    L = []
    P_ = L.append
    outcomes, undet_fp, better_somewhere = {}, set(), False
    units = cell_units(cell)
    P_("### %s (readouts %s; paired seeds, the first %d)\n" % (cell, ", ".join(cell_ys(cell)), nseeds))
    for mode in modes:
        R, base_n = load_rows(cell, mode, nseeds)
        if not R:
            P_("- mode %s: no complete rows on disk" % mode)
            continue
        rt_verdict = () if mode == "OFF" else (50.0,)
        res = read_mode(R, nf, units, cell_ys(cell), nseeds, cell_cap(cell), rt_verdict, degenerate=degenerate, base_n=base_n)
        outcomes[mode] = res["outcome"]
        better_somewhere = better_somewhere or bool(res["better"])
        for key in res["undet"]:
            undet_fp.add(B.fingerprint(B.row_builder(cell, key, mode)(0)))
        P_("#### %s: %s\n" % (B.n_mode(mode), res["outcome"]))
        P_("| baseline | readout | difference | outcome | under the verdict |")
        P_("|---|---|---|---|---|")
        for c in res["comps"]:
            P_("| %s | %s | %s | %s | %s |" % (B.row_name(c["base"]) if c["base"][0] != "api" else "object API (S5 at T=inf, hold 0)", c["k"], P.fmt(c["rd"]),
                                             c["rd"]["word"], "yes" if c["scope"] == "verdict" else "no, reported only"))
        P_("\n- shown worse or outside the swept range: %d; undetermined: %d; shown better: %d\n" % (len(res["worse"]), len(res["undetermined"]), len(res["better"])))
    cell_out = "FAILS" if "FAILS" in outcomes.values() else ("UNDETERMINED" if "UNDETERMINED" in outcomes.values() else ("PASSES" if outcomes else "no data"))
    P_("**CELL %s: %s**\n" % (cell, cell_out))
    text = "\n".join(L)
    print(text)
    if out_path:
        with open(out_path, "a", newline="\n") as f:
            f.write(text + "\n")
    if write_undet:
        with open(write_undet, "w") as f:
            json.dump(sorted(undet_fp), f)
    return cell_out, better_somewhere


# ---- smoothness: kp_stats over every 5 consecutive points (registration 13.2, plan 6) ------------------------------------------------------------
def kp_stats(Y):
    """Per seed K = 2 J1 - J2 and P = 2 D1 - D2 of a readout over five equally spaced points (Y: (5, seeds); the centre is index 2), as
    summarize2.kp_stats forms them: a step in the dial shows in K, a kink in P."""
    J1, J2 = Y[3] - Y[1], Y[4] - Y[0]
    D1 = Y[2] - 0.5 * (Y[1] + Y[3])
    D2 = Y[2] - 0.5 * (Y[0] + Y[4])
    return {"K": 2 * J1 - J2, "P": 2 * D1 - D2}


def slide_kp(Y, m):
    """kp_stats on every window of 5 consecutive points of Y ((points, seeds)); each statistic is read against +-m (the A/A floor of the readout,
    reported with the sweep).  Returns a list of (first point, K reading, P reading); a reading is "shown" when its interval lies outside +-m."""
    out = []
    for i in range(Y.shape[0] - 4):
        st = kp_stats(Y[i:i + 5])
        row = []
        for name in ("K", "P"):
            mu, lo, hi = boot(st[name])
            shown = lo > m or hi < -m
            row.append(dict(stat=name, mean=mu, lo=lo, hi=hi, m=m, word="shown" if shown else "not shown"))
        out.append((i, row[0], row[1]))
    return out


def cmd_smooth(nseeds=3):                                                   # SMOOTH runs 3 paired seeds (registration 14.5)
    import batch3e as B
    B.build()
    with open(NF_PATH) as f:
        nfall = json.load(f)
    shown = 0
    for name, jobs in sorted(B.SMOOTH_GROUPS.items()):
        cell = jobs[0].cell
        for k in cell_ys(cell):
            Y = []
            for j in jobs:
                rows = [B.load(j.fp, s) for s in range(nseeds)]
                if any(r is None for r in rows):
                    break
                Y.append([value(r, k, cell_cap(cell)) for r in rows])
            if len(Y) != len(jobs):
                print("- %s, %s: not complete" % (name, k))
                continue
            m = nf_n(nfall, cell, nseeds).get(k, 0.0)
            res = slide_kp(np.asarray(Y), m)
            bad = [(i, r) for i, a, b in res for r in (a, b) if r["word"] == "shown"]
            shown += len(bad)
            print("- %s, %s: %d windows, A/A floor %.3g, K or P shown in %d (%s)" % (name, k, len(res), m, len(bad), ", ".join("window %d %s" % (i, r["stat"]) for i, r in bad) or "none"))
    print("SMOOTHNESS: %s" % ("a K or P is shown: a step or a kink, a defect finding" if shown else "no K or P shown in any window"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=("freeze", "read", "verdict", "smooth"))
    ap.add_argument("cell", nargs="?", default="")
    ap.add_argument("--modes", default="OFF,RL")
    ap.add_argument("--nseeds", type=int, default=None, help="default: the cell's seed count (12 or 6)")
    ap.add_argument("--write-undet", default="")
    ap.add_argument("--out", default="")
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--aa-at", default="tinf", choices=("tinf", "primary"))
    a = ap.parse_args()
    if a.cmd == "freeze":
        cmd_freeze(a.force, a.aa_at)
    elif a.cmd == "smooth":
        cmd_smooth(a.nseeds or 3)
    elif a.cmd == "read":
        cmd_read(a.cell, [m if m in ("OFF", "RL", "RE", "RB") else float(m) for m in a.modes.split(",")], a.nseeds, a.write_undet, a.out)
    else:
        import batch3e as B
        outs, better = {}, False
        for c in a.cell.split(","):
            o, b = cmd_read(c, ["OFF", "RL"] + (["RE"] if c in B.RE_CELLS else []), a.nseeds, "", "")
            outs[c] = o
            better = better or b
        v = stage_verdict(outs, better)
        write_verdict_marker(B.OUT, v)                                         # ROBUST and SMOOTH wait for VERDICT-PASSES
        print("STAGE VERDICT:", v, outs)


if __name__ == "__main__":
    main()
