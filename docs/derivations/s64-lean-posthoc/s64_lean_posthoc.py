#!/usr/bin/env python3
"""POST HOC (not registered): three free reads of the §64 ledger on the stage R delay lean.

    python -I -X utf8 s64_lean_posthoc.py <harness64 tools/l1 dir> <docs/l1-raw/s64/s64-session.tgz>

Reuses the committed scorers (harness64 4feeb8fd) unchanged, and first asserts that they reproduce the committed score.txt lines,
so every new number below rides the same rows, row order and RNG stream as the guard and rt64 scorer.
  1. rtp_floor_p0, CAND vs BASE on stage G's four Bulk cells: statistic_rank (within-seed rank sums, 10 000 relabellings, seed 4141)
     with the floor clauses switched back on (clause set {floor: every cell, inorder: c2/c8}); the draws do not depend on the clause set.
  2. Stage R paired by slot (seed, rep): RC vs RB per readout, the dead row's slot (s42 r4) dropped; sign test.
  3. Stage R's noise floor: place41b_score.resolution on the live RB rows (half-per-seed splits, q95 of |x|).
"""
import hashlib
import math
import os
import sys
import tarfile
import tempfile

TOOLS, TGZ = sys.argv[1], sys.argv[2]
sys.path.insert(0, os.path.abspath(TOOLS))
import place38_parse as pp  # noqa: E402
import place41b_score as g  # noqa: E402
import rt64_score as rt  # noqa: E402
import plan64  # noqa: E402

CS = {"floor": None, "inorder": g.LONG_CELLS}       # v2 plus the floor clauses (reported-only clauses, scored here post hoc)


def sha(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()


def binom_tail(k, n):
    """P(X >= k), X ~ Bin(n, 1/2)."""
    return sum(math.comb(n, i) for i in range(k, n + 1)) / 2 ** n


tmp = tempfile.mkdtemp()
with tarfile.open(TGZ) as t:
    t.extractall(tmp, filter="data")
G_LOG = os.path.join(tmp, "session", "guard", "ld.log")
R_LOG = os.path.join(tmp, "session", "rt64", "ld.log")
print("POST HOC - three free reads of the s64 ledger (not registered; no verdict)")
print(f"tgz sha256 {sha(TGZ)}")
for f in ("place41b_score.py", "place38_parse.py", "rt64_score.py", "plan64.py", "l1common.py", "stage3_parse.py", "threadp1_parse.py"):
    print(f"tool {f} sha256 {sha(os.path.join(TOOLS, f))[:16]}")
print()

# ---- stage G rows, loaded as _score_guard loads them ----
g_all, _ = pp.rows_of([G_LOG])
g_rows, info = pp.apply_replacements(g_all)
assert not info["replaced"], info["replaced"]
res = g.statistic_rank(g_rows, "BASE", "CAND", cells=g.CELLS_GUARD, clause_set=CS)
# reproduction gate: the 22 v2 clause p's printed in guard/score.txt
want = {("c1s-400", "goodput"): 0.8810, ("c1s-400", "cpu_cli/GB"): 0.5165, ("c1s-400", "cpu_srv/GB"): 0.4382, ("c1s-400", "lag_E_cli"): 0.9783,
        ("c1s-400", "lag_E_srv"): 0.9525, ("c1d-400", "goodput"): 0.6538, ("c1d-400", "cpu_cli/GB"): 0.8883, ("c1d-400", "cpu_srv/GB"): 0.9397,
        ("c1d-400", "lag_E_cli"): 0.2734, ("c1d-400", "lag_E_srv"): 0.6136, ("c2-100", "goodput"): 0.5244, ("c2-100", "cpu_cli/GB"): 0.5676,
        ("c2-100", "cpu_srv/GB"): 0.7361, ("c2-100", "lag_E_cli"): 0.7633, ("c2-100", "lag_E_srv"): 0.6110, ("c2-100", "inorder_delay_mean"): 0.9978,
        ("c8-100", "goodput"): 0.8042, ("c8-100", "cpu_cli/GB"): 0.6815, ("c8-100", "cpu_srv/GB"): 0.5197, ("c8-100", "lag_E_cli"): 0.4482,
        ("c8-100", "lag_E_srv"): 0.5368, ("c8-100", "inorder_delay_mean"): 0.5255}
for k, p in want.items():
    assert abs(res["p_true"][k] - p) < 5e-5, (k, res["p_true"][k], p)
print(f"REPRO stage G: statistic_rank reproduces all {len(want)} clause p's of guard/score.txt (|d| < 5e-5)")
gres = {(r["cell"], r["clause"]): r for r in g.resolution(g_rows, cells=g.CELLS_GUARD, clause_set=CS)}
for k, q in {("c2-100", "lag_E_cli"): 0.2652, ("c2-100", "lag_E_srv"): 0.7299, ("c2-100", "inorder_delay_mean"): 0.0299}.items():
    assert abs(gres[k]["q95"] - q) < 5e-5, (k, gres[k]["q95"], q)
print("REPRO stage G: resolution reproduces c2 lag_E_cli 0.2652, lag_E_srv 0.7299, inorder 0.0299")

# ---- stage R rows, loaded as rt64_score loads them ----
r_all, _ = pp.rows_of([R_LOG])
fin = plan64.final_rows(r_all)
rc = {(k[2], k[3]): r for k, r in fin.items() if k[1] == "RC" and not plan64.is_void_r(r)}
rb = {(k[2], k[3]): r for k, r in fin.items() if k[1] == "RB" and r["status"] == "LIVE"}
dead = sorted(k for k, r in fin.items() if k[1] == "RB" and r["status"] == "BASE-F2-DEAD")
want_r = {"goodput": (-0.007, 0.9744), "cpu_cli/GB": (0.047, 0.3693), "cpu_srv/GB": (0.010, 0.4631), "lag_E_cli": (0.135, 0.1021),
          "lag_E_srv": (0.200, 0.1976), "inorder_delay_mean": (0.126, 0.0503), "rtp_floor_p0": (0.057, 0.0662)}
# rt64_score iterates RC / RB rows in plan-slot order; rebuild that order for the unpaired rank p
slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in plan64.plan()]
rc_ord = [fin[s] for s in slots if s[1] == "RC" and s in fin and not plan64.is_void_r(fin[s])]
rb_ord = [fin[s] for s in slots if s[1] == "RB" and s in fin and fin[s]["status"] == "LIVE"]
for name, key, sign in rt.READOUTS:
    c = [v for v in (rt.val(r, key) for r in rc_ord) if v is not None]
    b = [v for v in (rt.val(r, key) for r in rb_ord) if v is not None]
    x = sign * (rt.med(c) - rt.med(b)) / rt.med(b)
    p = rt.rank_p(c, b, sign)
    assert round(x, 3) == want_r[name][0] and abs(p - want_r[name][1]) < 5e-5, (name, x, p)
print(f"REPRO stage R: rt64_score reproduces all 7 READOUTS lines (x to 0.1 %, p to 1e-4); live RB {len(rb)}, RC {len(rc)}, BASE-F2-DEAD {dead}")
print()

# ---- read 1 ----
print("READ 1 (post hoc): rtp_floor_p<i>, CAND vs BASE, stage G (Bulk; c1s/c1d at guard size, c2/c8 at 600 MB), the guard's own statistic")
print("  within-seed rank sums over the guard's 10 000 seeded relabellings; p one-sided (CAND worse = higher floor); x = (med CAND - med BASE) / med BASE")
print("  q95 = the half-per-seed BASE split resolution of the clause (same construction as the RESOLUTION lines)")
pools = {p.cell: p for p in res["pools"]}
for cell, _ in g.CELLS_GUARD:
    p = pools[cell]
    xt = p.x_all(p.true)
    for n, key, sg in p.defs:
        if not n.startswith("rtp_floor"):
            continue
        b = [p.vals[n][i] for i in range(len(p.ref_rows))]
        c = [p.vals[n][i] for i in p.true]
        print(f"  {cell} {n:<13} BASE med {g.med(b):8.0f} [{min(b):.0f}-{max(b):.0f}]  CAND med {g.med(c):8.0f} [{min(c):.0f}-{max(c):.0f}]  "
              f"x={100 * xt[n]:+6.2f} %  p={res['p_true'][(cell, n)]:.4f}  q95|x|={gres[(cell, n)]['q95']:.4f}"
              + ("  (leg 0: the cell's registered readout)" if n == "rtp_floor_p0" and cell in ("c1d-400", "c8-100") else ""))
print()

print("READ 1b (post hoc, supplement): stage G paired by slot (cell, seed, rep; the two arms of a slot run back to back), the same sign test as read 2")
for cell, _ in g.CELLS_GUARD:
    p = pools[cell]
    by = {}
    for i, r in enumerate(p.rows):
        by.setdefault((r["seed"], r["rep"]), {})[r["arm"]] = i
    for n, key, sg in p.defs:
        if not (n == "rtp_floor_p0" or n == "inorder_delay_mean" or n.startswith("lag_E")):
            continue
        ds = [sg * (p.vals[n][d["CAND"]] - p.vals[n][d["BASE"]]) / p.vals[n][d["BASE"]] for d in by.values()
              if "CAND" in d and "BASE" in d and p.vals[n][d["CAND"]] is not None and p.vals[n][d["BASE"]]]
        k, lo = sum(1 for d in ds if d > 0), sum(1 for d in ds if d < 0)
        nn = k + lo
        print(f"  {cell} {n:<19} CAND worse in {k:2d} of {nn} pairs  sign p one-sided={binom_tail(k, nn):.4f}  two-sided={min(1.0, 2 * min(binom_tail(k, nn), binom_tail(lo, nn))):.4f}")
print()

# ---- read 2 ----
pairs = sorted(k for k in rc if k in rb)
print(f"READ 2 (post hoc): stage R paired by slot (seed, rep); {len(pairs)} pairs; dropped RC " + ", ".join(f"s{k[2]} r{k[3]}" for k in dead) + " (its RB partner died)")
print("  per pair d = worse-signed (RC - RB) / RB; '+' = RC worse.  sign test: one-sided P(Bin(n, 1/2) >= k worse), two-sided = 2 x the smaller tail; ties dropped")
hdr = "  " + "readout".ljust(19) + " ".join(f"s{s}r{r}".rjust(7) for s, r in pairs)
print(hdr)
signs = {}
for name, key, sign in rt.READOUTS:
    ds = []
    for k in pairs:
        a, b = rt.val(rc[k], key), rt.val(rb[k], key)
        ds.append(None if a is None or b is None else sign * (a - b) / b)
    signs[name] = ds
    print("  " + name.ljust(19) + " ".join(("-" if d is None else f"{100 * d:+6.1f}%") .rjust(7) for d in ds))
print()
for name, _, _ in rt.READOUTS:
    ds = [d for d in signs[name] if d is not None]
    k, n, ties = sum(1 for d in ds if d > 0), sum(1 for d in ds if d != 0), sum(1 for d in ds if d == 0)
    lo = sum(1 for d in ds if d < 0)
    p1 = binom_tail(k, n)
    p2 = min(1.0, 2 * min(binom_tail(k, n), binom_tail(lo, n)))
    md = sorted(ds)[len(ds) // 2] if len(ds) % 2 else sum(sorted(ds)[len(ds) // 2 - 1:len(ds) // 2 + 1]) / 2
    print(f"  {name:<19} RC worse in {k:2d} of {n} pairs (ties {ties})  median paired d={100 * md:+6.1f} %  sign p one-sided={p1:.4f}  two-sided={p2:.4f}"
          + ("  (every row under 30 s of ticks: no resolution)" if name == "inorder_delay_mean" else ""))
print("  co-movement: pairs worse on k of the 4 delay readouts (lag_E_cli, lag_E_srv, inorder_delay_mean, rtp_floor_p0)")
dl = ("lag_E_cli", "lag_E_srv", "inorder_delay_mean", "rtp_floor_p0")
print("  " + " ".join(f"s{s}r{r}:{sum(1 for n in dl if signs[n][i] is not None and signs[n][i] > 0)}" for i, (s, r) in enumerate(pairs)))
print()

# ---- read 3 ----
r_rows = [dict(r, arm="RB") for r in rb_ord]
for r in r_rows:
    r["store"] = "def"
rres = g.resolution(r_rows, arm="RB", cells=[("c2-100", "def")], clause_set=CS)
seeds = {}
for r in r_rows:
    seeds[r["seed"]] = seeds.get(r["seed"], 0) + 1
print("READ 3 (post hoc): stage R noise floor, the live RB rows alone; half-per-seed splits (" + " + ".join(f"s{s}: {n // 2} of {n}" for s, n in sorted(seeds.items()))
      + f" vs the rest), q95 of |x|")
f_split = math.sqrt((1 / 5 + 1 / 6) / (1 / 12 + 1 / 11))
print(f"  a split compares 5 vs 6 rows; the real RC-vs-RB comparison is 12 vs 11, so its noise is about 1/{f_split:.2f} of a split's"
      f" (sqrt((1/5+1/6)/(1/12+1/11)), normal approximation); q95/{f_split:.2f} is listed as the approximate floor of the real comparison")
xr = {}
for name, key, sign in rt.READOUTS:
    c = [v for v in (rt.val(r, key) for r in rc_ord) if v is not None]
    b = [v for v in (rt.val(r, key) for r in rb_ord) if v is not None]
    xr[name] = sign * (rt.med(c) - rt.med(b)) / rt.med(b)
for r in rres:
    n = r["clause"]
    if n not in xr:
        continue
    q = r["q95"]
    print(f"  {n:<19} q95|x|={q:.4f} (splits={r['splits']}, nRB={r['n_base']})  ~floor at 12 v 11={q / f_split:.4f}  stage R x={100 * xr[n]:+6.1f} %"
          f"  x/q95={xr[n] / q:+.2f}  x/floor={xr[n] / (q / f_split):+.2f}")
print("  stage G c2 (600 MB, 12 BASE) for comparison: " + "  ".join(f"{n} {gres[('c2-100', n)]['q95']:.4f}" for n in ("lag_E_cli", "lag_E_srv", "inorder_delay_mean", "rtp_floor_p0")))
