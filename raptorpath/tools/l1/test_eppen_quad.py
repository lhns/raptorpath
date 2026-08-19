#!/usr/bin/env python3
"""LOCAL GATE for `eppen_quad.py` — the N-path scorer, against a SYNTHETIC
four-path ledger with a KNOWN correlation structure.

WHY A SYNTHETIC LEDGER. `c9` cannot be captured without the VM, so the scorer
would otherwise reach the launch step never having read a four-path line in
its life — and the failure mode that costs the most is the silent one: a
scorer that reads only p0/p1 out of a quad ledger produces ONE correlation
where six were pre-registered, prints a well-formed table, and nothing in the
output says the other two legs were dropped. That is the SF bench's `pid < 2`
defect exactly (`MAX_PATHS` widened 2 -> 4 while three per-path gauge guards
kept their hard-coded `< 2`, so two legs read 0 no matter what and the
assertion built on them could not fail). This file makes that failure loud
BEFORE the first real quad ledger exists.

WHAT IS ASSERTED, and why each one is absolute rather than ordinal:

  1. **The published N = 2 result is reproduced** from the real committed
     ledger, to +-0.001. A generalization that does not reproduce the special
     case is a rewrite wearing the name of a widening.
  2. **All four paths are read** — 4 pids, 6 pairs, from a ledger that
     contains four.
  3. **rho_bar matches an INDEPENDENT re-implementation** of the two-way
     residual + mean-pairwise-Pearson chain, computed in this file from the
     same numbers. This tests the PIPELINE (line grammar -> grouping ->
     centering -> pairing), not the Pearson formula: a scorer that read the
     wrong field, mis-aligned the windows, or silently dropped a leg would
     disagree here even though its arithmetic was fine.
  4. **The algebraic floor is -1/(N-1) = -0.333 at N = 4**, and a rho_bar
     below it is REPORTED AS A MODEL FAILURE rather than passed through as a
     large benefit.
  5. **The heterogeneous quad's class split is read off the measured RTprop**,
     and fast-slow / fast-fast means are separated — C9-3 is a statement about
     WHICH pairs, so a scorer that only produced a grand mean could not score
     it at all.
  6. **A quad ledger at the shipped 2 s cadence is flagged UNDERPOWERED**,
     because §4 recorded the 250 ms window as a blocking dependency and six
     correlations over four windows per rep is the exact thing it blocks.
  7. **An INCOMPLETE window is dropped, not imputed** — one silent leg must
     take its whole window with it.

    usage: python test_eppen_quad.py     (exit 0 = pass)
"""
import itertools
import json
import math
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCORER = os.path.join(HERE, "eppen_quad.py")
REAL_LEDGER = os.path.normpath(
    os.path.join(HERE, "..", "..", "docs", "l1-raw", "ackdiag-ackdiag-s42.log"))

FAILURES = []


def check(desc, cond, detail=""):
    if cond:
        print(f"ok    {desc}")
    else:
        print(f"FAIL  {desc}   {detail}")
        FAILURES.append(desc)


def close(a, b, tol=1e-3):
    return a is not None and b is not None and abs(a - b) <= tol


# ── the synthetic ledger ─────────────────────────────────────────────────
def ackdiag_line(cell, rep, pid, win_s, acks, z, rate_lr, rtprop_ms):
    """One line in EXACTLY the format `c9_battery.sh` writes and
    `src/net/ackdiag.rs` emits — the prefix the driver prepends, then the
    gauge's own `[ACKDIAG]` body. The fields the scorer does not read are
    filled with plausible constants; the ones it does are the payload."""
    return (
        f"ACKDIAG {cell} rep={rep} [ACKDIAG] p{pid} win={win_s:.2f}s "
        f"acks={acks}/z={z}(0.0%) gap_us[p50=200 p90=400 p99=900 n={acks}] "
        f"drecv[p50=4 p90=8 max=16 n={acks} sum={acks*4}] "
        f"rd[acc={acks} rej=2 cnt={int(rate_lr*win_s)}] "
        f"rate_lr={int(rate_lr)}sym/s x[p50=1.10 p90=2.00 p99=3.00] "
        f"xanchor=1.50 anchor=800sym rtprop={rtprop_ms:.2f}ms "
        f"recon[sent=1000 crecv=1000 cexp=1100 srcack=900 cr/s=1.000 "
        f"ce/cr=1.100 cr/sa=1.111] ov=0"
    )


def make_ledger(path, cell, rates, rtprops, win_s, reps, wins_per_rep):
    """`rates[pid]` is a flat list of rate_lr values, rep-major, length
    reps*wins_per_rep. Deterministic — no randomness anywhere in this gate."""
    lines = []
    for r in range(reps):
        for w in range(wins_per_rep):
            i = r * wins_per_rep + w
            for pid in sorted(rates):
                lines.append(ackdiag_line(
                    cell, r + 1, pid, win_s,
                    acks=100 + i + pid, z=0,
                    rate_lr=rates[pid][i], rtprop_ms=rtprops[pid]))
    with open(path, "w") as fh:
        fh.write("\n".join(lines) + "\n")
    return lines


# ── the INDEPENDENT re-implementation (assertion 3) ──────────────────────
def indep_two_way_rho_bar(rates, reps, wins_per_rep):
    """Two-way additive residual per path, then the MEAN of all C(N,2)
    pairwise Pearson correlations. Written from the definition, deliberately
    without reusing a single line of the scorer."""
    pids = sorted(rates)
    n = reps * wins_per_rep
    rep_of = [i // wins_per_rep for i in range(n)]
    win_of = [i % wins_per_rep for i in range(n)]
    resid = {}
    for p in pids:
        # The LEDGER carries `rate_lr` as an integer (`rate_lr=%dsym/s`), so
        # the scorer necessarily reads the truncated value. Truncating here
        # too is not a fudge — it makes this re-implementation consume the
        # same numbers the scorer does, which is the only way the comparison
        # tests the pipeline rather than the width of a printf.
        v = [float(int(x)) for x in rates[p]]
        grand = sum(v) / n
        rm = {r: sum(v[i] for i in range(n) if rep_of[i] == r)
                 / sum(1 for i in range(n) if rep_of[i] == r)
              for r in set(rep_of)}
        wm = {w: sum(v[i] for i in range(n) if win_of[i] == w)
                 / sum(1 for i in range(n) if win_of[i] == w)
              for w in set(win_of)}
        resid[p] = [v[i] - rm[rep_of[i]] - wm[win_of[i]] + grand for i in range(n)]

    def pear(x, y):
        m = len(x)
        mx, my = sum(x) / m, sum(y) / m
        sxx = sum((a - mx) ** 2 for a in x)
        syy = sum((b - my) ** 2 for b in y)
        sxy = sum((a - mx) * (b - my) for a, b in zip(x, y))
        return sxy / math.sqrt(sxx * syy) if sxx > 0 and syy > 0 else None

    rs = [pear(resid[a], resid[b]) for a, b in itertools.combinations(pids, 2)]
    rs = [r for r in rs if r is not None]
    return sum(rs) / len(rs)


def run_scorer(*args):
    out = subprocess.run(
        [sys.executable, SCORER, *args, "--json"],
        capture_output=True, text=True)
    # The JSON blob is the tail of stdout, after the human report.
    i = out.stdout.find('{\n  "cells"')
    if i < 0:
        print(out.stdout)
        print(out.stderr, file=sys.stderr)
        raise SystemExit("scorer produced no JSON")
    return json.loads(out.stdout[i:]), out.stdout, out.returncode


def main():
    tmp = tempfile.mkdtemp()

    # ── 1. the published N = 2 result, from the REAL ledger ──────────────
    print("--- 1. the published N=2 c7/c8 result, reproduced by the N-path code")
    if os.path.exists(REAL_LEDGER):
        _, txt, rc = run_scorer(REAL_LEDGER, "--window-us", "2000000", "--selfcheck")
        check("selfcheck against the committed c7/c8 ledger passes",
              "SELFCHECK: PASS" in txt and rc == 0,
              "(see eppen_quad.py --selfcheck for the table)")
    else:
        check("the committed c7/c8 ledger is present", False, REAL_LEDGER)

    # ── 2+3+4. the SYMMETRIC quad ────────────────────────────────────────
    print("\n--- 2. c9, the SYMMETRIC quad: four legs read, six pairs, rho_bar exact")
    reps, wpr = 3, 40           # the 250 ms cadence's own shape: ~40 windows/rep
    n = reps * wpr
    # Four deterministic, mutually DISTINCT series. The exact rho_bar they
    # produce is not chosen in advance — it is computed independently below,
    # which is the point: the gate tests agreement, not a memorized number.
    rates = {
        0: [9000 + 300 * math.sin(i * 0.7) + 40 * i % 97 for i in range(n)],
        1: [9100 - 280 * math.sin(i * 0.7) + 31 * i % 89 for i in range(n)],
        2: [9050 + 190 * math.cos(i * 0.5) + 53 * i % 83 for i in range(n)],
        3: [8950 - 210 * math.cos(i * 0.5) + 17 * i % 79 for i in range(n)],
    }
    rtp = {0: 8.7, 1: 8.9, 2: 8.6, 3: 9.0}     # symmetric: no class split
    lp = os.path.join(tmp, "c9.log")
    make_ledger(lp, "c9-pooled", rates, rtp, 0.25, reps, wpr)
    res, txt, _ = run_scorer(lp, "--window-us", "250000")
    c = res["cells"]["c9-pooled"]

    check("four distinct paths are read from a four-path ledger",
          c["n_paths"] == 4 and c["paths"] == [0, 1, 2, 3],
          f"got n_paths={c['n_paths']} paths={c['paths']}")
    e = c["series"]["rate_lr"]
    check("all six pairwise correlations are produced",
          len(e["pairs_2way"]) == 6, f"got {len(e['pairs_2way'])}: "
          f"{sorted(e['pairs_2way'])}")
    check("every leg appears in the pairwise set",
          set("".join(sorted(e["pairs_2way"])).replace("p", "").replace("-", ""))
          >= {"0", "1", "2", "3"})
    check("all complete windows survive grouping",
          c["windows"] == n and c["dropped"] == 0,
          f"windows={c['windows']} want {n}, dropped={c['dropped']}")

    want = indep_two_way_rho_bar(rates, reps, wpr)
    got = e["rho_bar_2way"]
    check("rho_bar matches an INDEPENDENT re-implementation",
          close(got, want, 1e-9),
          f"scorer {got!r} vs independent {want!r}")
    print(f"      (rho_bar_2way = {got:+.6f}; independent = {want:+.6f})")

    check("the algebraic floor at N=4 is -1/(N-1) = -0.333",
          close(e["floor"], -1.0 / 3.0, 1e-9), f"got {e['floor']!r}")
    check("a well-powered quad ledger is NOT flagged underpowered",
          "underpowered" not in c, c.get("underpowered", ""))
    check("B(N, rho_bar) is computed for the quad",
          e["B_2way"] is not None)

    # ── 4b. THE FLOOR IS AN IDENTITY, NOT A FALSIFIER ────────────────────
    #
    # §4 states `rho_bar >= -1/(N-1)` as *the adding-up constraint's* floor and
    # C9-1 makes `rho_bar < -0.34` half of its falsification clause. That
    # clause is UNSATISFIABLE, and this block is the proof, because the
    # consequence for the contract is large: no four-path measurement can
    # produce the value, so that half of the bar cannot discriminate anything.
    #
    # The bound is a property of the ESTIMATOR. Any sample correlation matrix R
    # is positive semi-definite, so `1' R 1 = N + 2*sum_{i<j} rho_ij >= 0`,
    # giving `rho_bar >= -N/(2*C(N,2)) = -1/(N-1)` for ANY N series whatever —
    # exchangeable or not, equal-variance or not, adding up to a binding total
    # or not.
    print("\n--- 3. the floor -1/(N-1) is an ALGEBRAIC IDENTITY, not a falsifier")

    def rho_bar_of(series):
        def pear(x, y):
            m = len(x)
            mx, my = sum(x) / m, sum(y) / m
            sxx = sum((a - mx) ** 2 for a in x)
            syy = sum((b - my) ** 2 for b in y)
            sxy = sum((a - mx) * (b - my) for a, b in zip(x, y))
            return sxy / math.sqrt(sxx * syy) if sxx > 0 and syy > 0 else None
        rs = [pear(series[a], series[b])
              for a, b in itertools.combinations(sorted(series), 2)]
        rs = [r for r in rs if r is not None]
        return sum(rs) / len(rs) if rs else None

    floor4 = -1.0 / 3.0
    # (a) THE HARDEST ADVERSARIAL CONSTRUCTIONS still cannot breach it.
    worst = 1.0
    import random
    random.seed(20260819)
    for _ in range(4000):
        s = {p: [random.gauss(0, random.uniform(0.1, 5.0)) for _ in range(12)]
             for p in range(4)}
        r = rho_bar_of(s)
        if r is not None:
            worst = min(worst, r)
    # Three legs locked together against one moving hard the other way — the
    # construction that INTUITIVELY should breach the floor. It lands at 0.
    anti = {
        0: [9000 + 500 * math.sin(i * 0.9) for i in range(n)],
        1: [9000 + 500 * math.sin(i * 0.9) for i in range(n)],
        2: [9000 + 500 * math.sin(i * 0.9) for i in range(n)],
        3: [9000 - 1500 * math.sin(i * 0.9) for i in range(n)],
    }
    r_anti = rho_bar_of(anti)
    check("4000 adversarial random quads never breach -1/(N-1)",
          worst >= floor4 - 1e-12, f"minimum observed {worst:+.6f}")
    check("3-against-1 does not breach it either (it lands at 0.0)",
          r_anti >= floor4 - 1e-12 and abs(r_anti) < 1e-6,
          f"got {r_anti:+.9f}")
    print(f"      (minimum over 4000 random quads = {worst:+.6f}; "
          f"floor = {floor4:+.6f})")

    # (b) The floor is APPROACHED FROM ABOVE by exactly the process §4
    # describes: exchangeable legs whose per-window sum is pinned.
    base = {p: [random.gauss(0, 1) for _ in range(400)] for p in range(4)}
    exch = {p: [base[p][i] - sum(base[q][i] for q in range(4)) / 4
                for i in range(400)] for p in range(4)}
    r_exch = rho_bar_of(exch)
    check("an exchangeable, sums-to-constant quad SITS AT the floor",
          floor4 - 1e-12 <= r_exch <= floor4 + 0.01,
          f"got {r_exch:+.6f} vs floor {floor4:+.6f}")
    print(f"      (exchangeable + binding total = {r_exch:+.6f}; "
          f"floor = {floor4:+.6f})")

    # (c) So the scorer's own below-floor branch is an ARITHMETIC SELF-CHECK
    # and must read False on every real ledger, including the adversarial one.
    lp2 = os.path.join(tmp, "c9anti.log")
    make_ledger(lp2, "c9anti", anti, rtp, 0.25, reps, wpr)
    res2, txt2, _ = run_scorer(lp2, "--window-us", "250000")
    e2 = res2["cells"]["c9anti"]["series"]["rate_lr"]
    check("the scorer's below-floor branch reads False (it is a bug-detector)",
          e2["below_floor_2way"] is False,
          f"rho_bar={e2['rho_bar_2way']!r} floor={e2['floor']!r}")
    check("no INTERNAL INCONSISTENCY is reported on a valid ledger",
          "INTERNAL INCONSISTENCY" not in txt2)
    check("B is finite whenever rho_bar respects the floor",
          e2["B_2way"] is not None, f"got {e2['B_2way']!r}")

    # ── 5. the HETEROGENEOUS quad's class split ──────────────────────────
    print("\n--- 4. c9h, the HETEROGENEOUS quad: classes read off the measured RTprop")
    het_rates = {
        0: [9000 + 400 * math.sin(i * 0.6) for i in range(n)],   # fast (c2)
        1: [8900 + 380 * math.sin(i * 0.6 + 0.2) for i in range(n)],
        2: [1400 + 300 * math.sin(i * 0.6 + 0.1) for i in range(n)],  # slow (c3)
        3: [1350 + 290 * math.sin(i * 0.6 + 0.3) for i in range(n)],
    }
    het_rtp = {0: 8.5, 1: 8.7, 2: 38.3, 3: 39.1}   # c2 ~8.6 ms vs c3 ~38.7 ms
    lp3 = os.path.join(tmp, "c9h.log")
    make_ledger(lp3, "c9h-pooled", het_rates, het_rtp, 0.25, reps, wpr)
    res3, txt3, _ = run_scorer(lp3, "--window-us", "250000")
    e3 = res3["cells"]["c9h-pooled"]["series"]["rate_lr"]
    check("the heterogeneous quad splits into two classes",
          e3.get("classes") is not None
          and e3["classes"]["fast"] == [0, 1]
          and e3["classes"]["slow"] == [2, 3],
          f"got {e3.get('classes')}")
    cl = e3.get("class_2way") or {}
    check("fast-fast, slow-slow and fast-slow means are all reported",
          all(cl.get(k) is not None for k in ("fast_fast", "slow_slow", "fast_slow")),
          f"got {cl}")
    check("the SYMMETRIC quad is NOT forced into two classes",
          c["series"]["rate_lr"].get("classes") is None,
          f"got {c['series']['rate_lr'].get('classes')}")

    # ── 6. the cadence prerequisite ──────────────────────────────────────
    print("\n--- 5. a quad at the SHIPPED 2 s cadence is refused, not scored")
    lp4 = os.path.join(tmp, "c9slow.log")
    make_ledger(lp4, "c9slow", rates, rtp, 2.0, reps, 4)   # four windows/rep
    res4, txt4, _ = run_scorer(lp4, "--window-us", "2000000")
    c4 = res4["cells"]["c9slow"]
    check("four windows/rep against six pairs is flagged UNDERPOWERED",
          "underpowered" in c4, f"windows_per_rep={c4.get('windows_per_rep')}")
    check("the report names the 250 ms window as the BLOCKING dependency",
          "BLOCKING" in txt4)
    check("a ledger whose windows contradict --window-us is flagged",
          "window_mismatch" in run_scorer(lp4, "--window-us", "250000")[0]
          ["cells"]["c9slow"])

    # ── 7. an incomplete window is dropped, never imputed ────────────────
    print("\n--- 6. one silent leg takes its whole window with it")
    with open(lp) as fh:
        lines = fh.readlines()
    # Remove p3's line from the very first window only.
    cut = [l for i, l in enumerate(lines) if not (i == 3)]
    lp5 = os.path.join(tmp, "c9hole.log")
    with open(lp5, "w") as fh:
        fh.writelines(cut)
    res5, _, _ = run_scorer(lp5, "--window-us", "250000")
    c5 = res5["cells"]["c9-pooled"]
    check("an incomplete window is DROPPED and counted",
          c5["dropped"] >= 1 and c5["windows"] < c["windows"],
          f"dropped={c5['dropped']} windows={c5['windows']} (full run had "
          f"{c['windows']})")
    check("the surviving windows still cover all four legs",
          c5["n_paths"] == 4)

    print()
    if FAILURES:
        print(f"EPPEN-QUAD GATE: FAIL ({len(FAILURES)})")
        for f in FAILURES:
            print(f"  - {f}")
        return 1
    print("EPPEN-QUAD GATE: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
