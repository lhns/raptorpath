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


def stage_verdict(outcomes, better_somewhere):
    """Registration 13.3 (plan 1.3): PASSES if no cell FAILS and at least one comparison outside the identity cells reads shown better; FAILS if any
    cell fails; UNDETERMINED otherwise."""
    if any(v == "FAILS" for v in outcomes.values()):
        return "FAILS"
    if better_somewhere:
        return "PASSES"
    return "UNDETERMINED"


# ---- one cell in one repair mode ----------------------------------------------------------------------------------------------------------
def read_mode(R, nf, units, ys, nseeds, rec_cap, rt_verdict=(50.0,), gates_bad=False):
    """R: row key -> seed -> result, keys ("lt", T, 0.0) (the new curve), ("adr", rho), ("rt", delta), ("bulk",).  The object API is the new
    curve's own T = inf, H = 0 point.  ADR-0084 (curve and points), today's Bulk and the object API are under the verdict in every mode;
    today's Realtime only at the deltas in rt_verdict (50 in RL and RE, none in OFF), elsewhere it is reported.
    Returns dict(outcome, comps, undet (row keys to double), worse, better)."""
    def have(key, ns):
        return key in R and all(s in R[key] for s in range(ns))

    def arr(keys, k, ns):
        return np.stack([[value(R[key][s], k, rec_cap) for s in range(ns)] for key in keys])

    new_all = sorted([k for k in R if k[0] == "lt" and k[2] == 0.0 and have(k, nseeds)], key=lambda k: k[1])
    base_keys = [k for k in R if k[0] in ("adr", "rt", "bulk") and have(k, nseeds)]
    if new_all and ("lt", INF, 0.0) in new_all:
        base_keys.append(("api",))
        R = dict(R)
        R[("api",)] = R[("lt", INF, 0.0)]
    xnoise = max(nf_of(nf, "share_any"), 1.0 / units)
    comps, undet = [], set()

    def scope_of(key):
        if key[0] == "rt":
            return "verdict" if key[1] in rt_verdict else "reported"
        return "verdict"

    for k in ys:
        for bk in base_keys:
            def run(newk, ns):
                nx, ny = arr(newk, "share_any", ns), arr(newk, k, ns)
                kk = merge_points(nx, ny, P.BETTER[k], xnoise)
                bx, by = arr([bk], "share_any", ns)[0], arr([bk], k, ns)[0]
                return compare(nx, ny, kk, bx, by, k, nf_of(nf, k), xnoise, units, float(by.mean()), True), kk, nx, bx
            rd, kk, nx, bx = run(new_all, nseeds)
            if rd["word"] == "undetermined":
                arms = {bk} | {new_all[i] for i in bracket(nx.mean(1), kk, float(bx.mean()))}
                n2 = [n for n in new_all if have(n, 2 * nseeds)]
                if have(bk, 2 * nseeds) and len(n2) >= 2:                      # the registered doubling, on these arms only; never a second one
                    rd, _, _, _ = run(n2, 2 * nseeds)
                    rd["note"] = "doubled to %d seeds" % (2 * nseeds)
                else:
                    undet.update(a for a in arms if a != ("api",))             # still to double
            comps.append(dict(scope=scope_of(bk), kind="point", base=bk, k=k, rd=rd))
        adr = [k2 for k2 in base_keys if k2[0] == "adr"]
        if len(adr) >= 2 and new_all:                                          # curve against curve: the new points inside ADR-0084's range
            ax, ay = arr(adr, "share_any", nseeds), arr(adr, k, nseeds)
            ak = merge_points(ax, ay, P.BETTER[k], xnoise)
            lo, hi = ax[ak].mean(1).min(), ax[ak].mean(1).max()
            for n in new_all:
                x = float(arr([n], "share_any", nseeds).mean())
                if not (lo - 1e-12 <= x <= hi + 1e-12):
                    continue

                def runr(adrk, ns):
                    ax_, ay_ = arr(adrk, "share_any", ns), arr(adrk, k, ns)
                    ak_ = merge_points(ax_, ay_, P.BETTER[k], xnoise)
                    px, py = arr([n], "share_any", ns)[0], arr([n], k, ns)[0]
                    return compare_rev(ax_, ay_, ak_, px, py, k, nf_of(nf, k), units, float(ay_.mean())), ax_, ak_
                rd, ax_, ak_ = runr(adr, nseeds)
                if rd["word"] == "undetermined":
                    a2 = [m for m in adr if have(m, 2 * nseeds)]
                    if have(n, 2 * nseeds) and len(a2) >= 2:
                        rd, _, _ = runr(a2, 2 * nseeds)
                        rd["note"] = "doubled to %d seeds" % (2 * nseeds)
                    else:
                        undet.update({n} | {adr[i] for i in bracket(ax_.mean(1), ak_, x)})
                comps.append(dict(scope="verdict", kind="curve", base=n, k=k, rd=rd))
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
    """Row key -> seed -> result for every design row of a cell in a repair mode that has its first nseeds seeds on disk; seeds nseeds to
    2 nseeds - 1 are loaded where the registered doubling has run them (read_mode uses them only for the comparisons it doubles)."""
    import batch3e as B
    R = {}
    for key in B.rows_of(cell):
        fp = B.fingerprint(B.row_builder(cell, key, mode)(0))
        rows = {}
        for s in range(2 * nseeds):
            r = B.load(fp, s)
            if r is None:
                break
            rows[s] = r
        if len(rows) >= nseeds:
            R[key] = rows
    return R


def cell_units(cell):
    import batch3e as B
    kw = B.CELLS[B.base_of(cell)][1]
    return kw["n_total"]


def cell_cap(cell):
    import batch3e as B
    kw = B.CELLS[B.base_of(cell)][1]
    return rec_cap(kw) if kw.get("dead") is not None else 0.0


def cmd_freeze(force=False, aa_at="tinf"):
    import batch3e as B
    B.build()
    nf = dict(notes="p95 over 12 seeds of |A - A'|; A' = phase variant 1 of A; A = S5 at T=inf, hold 0 (\"tinf\", build 8.5) or at the cell's primary lifetime "
                    "(\"primary\", registration 5), repair off; shares and seconds.  \"cells\" is the one chosen by --aa-at.", aa_at=aa_at, tinf={}, primary={}, cells={})
    for cell in B.MAIN_CELLS + B.AA2_CELLS:
        base, d = B.base_of(cell), B.delta_of(cell)
        for at in ("tinf", "primary"):
            T = INF if at == "tinf" else B.primary_T(cell)
            A = {s: B.load(B.fingerprint(B.new_cfg(base, 0, T, 0.0, d, None)), s) for s in range(SEEDS)}
            Ap = {s: B.load(B.fingerprint(B.new_cfg(base, 0, T, 0.0, d, None, variant=1)), s) for s in range(SEEDS)}
            if any(r is None for r in list(A.values()) + list(Ap.values())):
                raise SystemExit("the A/A (%s) needs the %d seeds of both arms in %s" % (at, SEEDS, cell))
            nf[at][cell] = nf_from(A, Ap, cell_cap(cell), ["share_any", "share_T"] + cell_ys(cell))
        nf["cells"][cell] = nf[aa_at][cell]
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
        nf = json.load(f)["cells"][cell]
    L = []
    P_ = L.append
    outcomes, undet_fp, better_somewhere = {}, set(), False
    units = cell_units(cell)
    P_("### %s (readouts %s; paired seeds, the first %d)\n" % (cell, ", ".join(cell_ys(cell)), nseeds))
    for mode in modes:
        R = load_rows(cell, mode, nseeds)
        if not R:
            P_("- mode %s: no complete rows on disk" % mode)
            continue
        rt_verdict = () if mode == "OFF" else (50.0,)
        res = read_mode(R, nf, units, cell_ys(cell), nseeds, cell_cap(cell), rt_verdict)
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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=("freeze", "read", "verdict"))
    ap.add_argument("cell", nargs="?", default="")
    ap.add_argument("--modes", default="OFF,RL")
    ap.add_argument("--nseeds", type=int, default=SEEDS)
    ap.add_argument("--write-undet", default="")
    ap.add_argument("--out", default="")
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--aa-at", default="tinf", choices=("tinf", "primary"))
    a = ap.parse_args()
    if a.cmd == "freeze":
        cmd_freeze(a.force, a.aa_at)
    elif a.cmd == "read":
        cmd_read(a.cell, [m if m in ("OFF", "RL", "RE") else float(m) for m in a.modes.split(",")], a.nseeds, a.write_undet, a.out)
    else:
        outs, better = {}, False
        for c in a.cell.split(","):
            o, b = cmd_read(c, ["OFF", "RL"], a.nseeds, "", "")
            outs[c] = o
            better = better or b
        print("STAGE VERDICT:", stage_verdict(outs, better), outs)


if __name__ == "__main__":
    main()
