#!/usr/bin/env python3
"""Status 65: the Realtime in-order delay re-check (docs/derivations/s65-registration.md). Plan 65 = c2-100, Realtime, rho = 1, arms RB = BASE's binary and RC = CAND's,
N_PAIRS slots (seeds 42 / 7, plan 41c's interleave: seed 42's pair runs RB first, seed 7's RC first), 600 MB rows. RC against RB PAIRED BY SLOT (seed, rep).

    s65_score.py <ledger> [...]

Scored (two readouts, each one-sided, prediction registered: RC worse):
  HEAD  the in-order delay mean over ticks t <= HEAD_S (the window section 64's 12 s rows saw): the REPLICATION;
  REST  the in-order delay mean over ticks t > HEAD_S: PERSISTENCE.
Per readout: d = ln(RC / RB) per usable pair; exact one-sided Wilcoxon signed-rank p (sign-flip distribution by exact counting), the Hodges-Lehmann shift h and its exact one-sided
95 % upper bound U; REGRESSION iff p <= ALPHA and h > TOL; NO-REGRESSION iff U <= TOL; else UNRESOLVED; fewer than MIN_PAIRS usable pairs: UNRESOLVED-N.
Session label from the pair (HEAD, REST): BISECT, START-UP-COST, REPLICATED-PERSISTENCE-OPEN, LATE-ONLY, REST-REGRESSION, CLOSED, NOT-REPLICATED or UNRESOLVED (table in the registration, section 3.5).
Reported only: the whole-run in-order mean; the RB half-split q95 per window; the shift per run order; early loss (quinn lost packets by t = 2 s) per row; the other readouts.
Pure and env-free; MIN_TICK_S is a parameter only so the path exercise can run the code on section 64's 12 s rows (labelled there)."""
import functools
import itertools
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

TOL = 0.05                    # the margin: the tolerance the merge guard applies to c2's in-order clause (section 64 stage G printed tol=0.0500; l1ab.REL_FLOOR); fixed
ALPHA = 0.05                  # one-sided, per readout
N_PAIRS = 24                  # fixed before the launch from the power table (registration section 4); never extended after rows are read
MIN_PAIRS = math.ceil(5 * N_PAIRS / 6)       # 20: fewer usable pairs -> UNRESOLVED-N (5/6 of the design)
SEEDS = ("42", "7")
MIN_TICK_S = 30.0             # guard v2's floor (section 41d): a completed row with under 30 s of ticks is VOID-SHORT
HEAD_S = 11.5                 # section 64's stage R rows ended at 11.5-11.8 s
EARLY_S = 2.0                 # the early-loss window (reported)
DEAD_CAP = N_PAIRS // 12 + 3  # 5: BASE-F2-DEAD slots redrawn at most (Poisson(2) >= 6 is about 1.7 %)
VOID_CAP = N_PAIRS // 6       # 4: non-F2 void slots redrawn at most
REDRAW = ("WITNESS-FAIL", "CONTAMINATED", "VOID-COTENANT", "VOID-ONDISK", "VOID-RHO", "VOID-RC", "NO_DATA", "VOID-SHORT", "BASE-F2-DEAD")
WINDOWS = [("head", "inorder_mean_head"), ("rest", "inorder_mean_rest")]
WHOLE = ("whole", "inorder_mean")
READOUTS = [("goodput", "mbps", -1), ("cpu_cli/GB", "cpu_cli", 1), ("cpu_srv/GB", "cpu_srv", 1), ("lag_E_cli", "cli_lag_E", 1), ("lag_E_srv", "srv_lag_E", 1),
            ("rtp_floor_p0", "rtp_floor_p0", 1)]
_T = re.compile(r"\[DIAG\] t=([\d.]+)s")
_QLP = re.compile(r"\bp0:infl=.*?\bqlp=(\d+)/(\d+)")


def plan(n_pairs=N_PAIRS):
    """[(arm, seed, rep)] in run order: groups rep-major, seed 42 before 7; even groups RB then RC, odd groups RC then RB (plan 41c's interleave; plan64.plan() at n = 12)."""
    out = []
    for g, (rp, sd) in enumerate((rp, sd) for rp in range(1, n_pairs // 2 + 1) for sd in SEEDS):
        for a in (("RB", "RC") if g % 2 == 0 else ("RC", "RB")):
            out.append((a, sd, str(rp)))
    return out


def first_seat(seed):
    return "RB" if seed == SEEDS[0] else "RC"


def row_fields(cli):
    """The row fields plan 65 adds (the parser calls this from plan64.rt_row): the head / rest in-order means (ms; None without a tick) and the early loss (leg 0, the
    client's last [DIAG] tick at t <= EARLY_S: quinn lost / sent packets, cumulative)."""
    from threadp1_parse import inorder_delay
    d = inorder_delay(cli)
    head = [x for t, x in d if t <= HEAD_S]
    rest = [x for t, x in d if t > HEAD_S]
    ql = None
    for ln in cli:
        mt = _T.search(ln)
        if not mt or float(mt.group(1)) > EARLY_S:
            continue
        m = _QLP.search(ln)
        if m:
            ql = (int(m.group(1)), int(m.group(2)))
    return {"inorder_mean_head": 1e3 * sum(head) / len(head) if head else None, "inorder_mean_rest": 1e3 * sum(rest) / len(rest) if rest else None,
            "qlost_2s": ql[0] if ql else None, "qsent_2s": ql[1] if ql else None}


def slot_key(r):
    return (str(r["seed"]), str(r["rep"]))


def eff_status(r, min_tick):
    if r["status"] in ("LIVE", "RT-OK") and not r.get("dnf") and (r.get("tick_span_s") or 0.0) < min_tick:
        return "VOID-SHORT"
    return r["status"]


def final_rows(rows, min_tick=MIN_TICK_S):
    """{(arm, seed, rep): (row, effective status, first row's status if redrawn else None)}: one redraw per slot (the slot's second row replaces a first row in REDRAW)."""
    by = {}
    for r in rows:
        by.setdefault((r["arm"],) + slot_key(r), []).append(r)
    out = {}
    for k, rs in by.items():
        st0 = eff_status(rs[0], min_tick)
        out[k] = (rs[1], eff_status(rs[1], min_tick), st0) if (st0 in REDRAW and len(rs) > 1) else (rs[0], st0, None)
    return out


def val(r, key):
    if r.get(key) is None:
        return None
    if key in ("cpu_cli", "cpu_srv"):
        return r[key] / (r["bytes"] / 1e9)
    return r[key]


def _midranks(a):
    idx = sorted(range(len(a)), key=lambda i: a[i])
    out = [0.0] * len(a)
    j = 0
    while j < len(idx):
        k = j
        while k + 1 < len(idx) and a[idx[k + 1]] == a[idx[j]]:
            k += 1
        for t in range(j, k + 1):
            out[idx[t]] = (j + k) / 2 + 1
        j = k + 1
    return out


def _flip_counts(r2):
    """Counts of the 2^n sign-flip sums of the integer ranks r2 (exact; the same as enumerating every flip)."""
    c = [1] + [0] * sum(r2)
    for r in r2:
        for s in range(len(c) - 1, r - 1, -1):
            c[s] += c[s - r]
    return c


def signed_rank_p(d):
    """Exact one-sided p = P0(W+ >= W+_obs) over the sign flips of the observed (mid)ranks of |d|; zeros dropped. -> (p, n_nonzero, W+)"""
    nz = [x for x in d if x != 0]
    if not nz:
        return 1.0, 0, 0.0
    r2 = [int(round(2 * r)) for r in _midranks([abs(x) for x in nz])]
    w2 = sum(r for r, x in zip(r2, nz) if x > 0)
    c = _flip_counts(r2)
    return sum(c[w2:]) / 2 ** len(nz), len(nz), w2 / 2


@functools.lru_cache(maxsize=None)
def wplus_cdf_c(n, alpha=ALPHA):
    """The largest c with P0(W+ <= c) <= alpha for n untied ranks (-1 when none)."""
    cnt = _flip_counts(list(range(1, n + 1)))
    c, cum = -1, 0
    for w, k in enumerate(cnt):
        cum += k
        if cum / 2 ** n <= alpha:
            c = w
        else:
            break
    return c


def hl_upper(d, alpha=ALPHA):
    """Hodges-Lehmann shift (median of the Walsh averages) and its exact one-sided (1 - alpha) upper bound W_(N - c) (1-based), c = wplus_cdf_c(n); U = inf when c < 0."""
    n = len(d)
    wa = sorted((d[i] + d[j]) / 2 for i in range(n) for j in range(i, n))
    N = len(wa)
    h = wa[N // 2] if N % 2 else (wa[N // 2 - 1] + wa[N // 2]) / 2
    c = wplus_cdf_c(n, alpha)
    return h, (wa[N - c - 1] if c >= 0 else math.inf), c


def paired(pairs, key, sign=1):
    """[(slot, d, note)], d = sign * ln(RC / RB); a DNF side is the worst rank on its side (d = +-L, L = 1 + max |finite d|), both DNF d = 0; a pair missing a value is left out."""
    raw = []
    for k, rb, rc in pairs:
        a, b = val(rc, key), val(rb, key)
        if rc.get("dnf") and rb.get("dnf"):
            raw.append((k, 0.0, "both-dnf"))
        elif rc.get("dnf"):
            raw.append((k, None, "rc-dnf"))
        elif rb.get("dnf"):
            raw.append((k, None, "rb-dnf"))
        elif a is not None and b is not None and a > 0 and b > 0:
            raw.append((k, sign * math.log(a / b), ""))
    fin = [abs(x) for _, x, _ in raw if x is not None]
    L = 1 + (max(fin) if fin else 0.0)
    return [(k, x if x is not None else (L if w == "rc-dnf" else -L), w) for k, x, w in raw]


def test(xs, min_pairs=MIN_PAIRS):
    """-> dict(n, p, w, h_rel, u_rel, c, label) for one readout's d values."""
    p, nz, w = signed_rank_p(xs)
    if xs:
        h, u, c = hl_upper(xs)
        h_rel, u_rel = math.exp(h) - 1, (math.exp(u) - 1 if u != math.inf else math.inf)
    else:
        h_rel = u_rel = math.inf
        c = -1
    if len(xs) < min_pairs:
        lab = "UNRESOLVED-N"
    elif p <= ALPHA and h_rel > TOL:
        lab = "REGRESSION"
    elif u_rel <= TOL:
        lab = "NO-REGRESSION"
    else:
        lab = "UNRESOLVED"
    return {"n": len(xs), "nz": nz, "p": p, "w": w, "h_rel": h_rel, "u_rel": u_rel, "c": c, "label": lab, "worse": sum(1 for x in xs if x > 0)}


def session_label(head, rest):
    """A window is called clean only when it reads NO-REGRESSION; an UNRESOLVED(-N) window is open, never 'no effect'."""
    if head == "REGRESSION" and rest == "REGRESSION":
        return "BISECT", "the head replicates and the rest persists: a regression of the merged fix; bisect commit 6's engine against commit 8's"
    if head == "REGRESSION" and rest == "NO-REGRESSION":
        return "START-UP-COST", "the head replicates and the rest excludes more than TOL: a start-up cost of the merged fix, not a steady-state one"
    if head == "REGRESSION":
        return "REPLICATED-PERSISTENCE-OPEN", f"the head replicates; the rest reads {rest}, so persistence is open"
    if head == "NO-REGRESSION" and rest == "REGRESSION":
        return "LATE-ONLY", "the rest regresses and the head excludes more than TOL: not the predicted pattern; to the owner"
    if rest == "REGRESSION":
        return "REST-REGRESSION", f"the rest regresses; the head reads {head}, so the start-up window is open: a steady-state regression of the merged fix"
    if head == "NO-REGRESSION" and rest == "NO-REGRESSION":
        return "CLOSED", "neither window regresses beyond TOL: section 64's lean closes"
    if rest == "NO-REGRESSION":
        return "NOT-REPLICATED", f"the head reads {head} (no replication at alpha, not excluded either) and the rest excludes more than TOL: no steady-state regression, the start-up lean not reproduced"
    return "UNRESOLVED", f"head {head}, rest {rest}"


SPLITS_MAX = 2000             # the reported q95: every split when there are at most this many (section 64's 200), else a seeded sample of this many


def q95_halfsplit(vals_by_seed):
    """Reported only: the q95 of |x| over the half-per-seed splits (place41b_score.resolution's construction for one clause); x = (med(half) - med(rest)) / med(rest).
    Above SPLITS_MAX splits (24 rows: 853 776) a sample of SPLITS_MAX, random.Random(65), each seed's half drawn uniformly."""
    import random
    import place41b_score as g
    seeds = sorted(vals_by_seed)
    per = [list(itertools.combinations(range(len(vals_by_seed[s])), len(vals_by_seed[s]) // 2)) for s in seeds]
    total = math.prod(len(c) for c in per)
    if total <= SPLITS_MAX:
        combos = itertools.product(*per)
    else:
        rng = random.Random(65)
        combos = [tuple(rng.choice(c) for c in per) for _ in range(SPLITS_MAX)]
    xv = []
    for combo in combos:
        sel, ref = [], []
        for s, c in zip(seeds, combo):
            cs = set(c)
            for i, v in enumerate(vals_by_seed[s]):
                (sel if i in cs else ref).append(v)
        ms, mr = g.med(sel), g.med(ref)
        if ms is not None and mr:
            xv.append(abs((ms - mr) / mr))
    return g.q95(xv), len(xv)


def score(paths, out=print, min_tick=MIN_TICK_S, rows=None, n_pairs=N_PAIRS):
    import place38_parse as pp
    mp = math.ceil(5 * n_pairs / 6)
    if rows is None:
        rows, _ = pp.rows_of(paths)
    fin = final_rows(rows, min_tick)
    slots = plan(n_pairs)
    out(f"S65 REPORT (Realtime c2-100 at rho = 1; RB = BASE's binary, RC = CAND's; scored: the HEAD (t <= {HEAD_S:g} s, the replication) and REST (t > {HEAD_S:g} s, persistence) in-order "
        f"delay means, paired by slot, one-sided (RC worse predicted); TOL {100 * TOL:.0f} %, alpha {ALPHA} per readout, design {n_pairs} pairs, min pairs {mp}, min ticks {min_tick:g} s): "
        f"ledger rows {len(rows)}")
    n_redraw = {}
    for k in slots:
        r, st, was = fin.get(k, (None, "NO-ROW", None))
        if was:
            n_redraw[was] = n_redraw.get(was, 0) + 1
        out(f"SLOT65 {k[0]} s{k[1]} r{k[2]}: {st}" + (f" (redrawn: first row {was})" if was else "")
            + ("" if r is None else f" dnf={r.get('dnf')} ticks={r.get('tick_span_s')} qlost_2s={r.get('qlost_2s')}/{r.get('qsent_2s')} bin_sha={str(r.get('bin_sha'))[:16]}"))
    out("REDRAWS " + (", ".join(f"{k} {v}" for k, v in sorted(n_redraw.items())) or "none") + f" (caps: BASE-F2-DEAD {DEAD_CAP} slots, non-F2 voids {VOID_CAP} slots)")
    rb_slots = [k for k in slots if k[0] == "RB"]
    dead_rows = sum(1 for r in rows if r["arm"] == "RB" and r["status"] == "BASE-F2-DEAD")
    alive = sum(1 for k in rb_slots if k in fin and fin[k][2] == "BASE-F2-DEAD" and fin[k][1] == "LIVE")
    twice = sum(1 for k in rb_slots if k in fin and fin[k][2] == "BASE-F2-DEAD" and fin[k][1] == "BASE-F2-DEAD")
    unred = sum(1 for k in rb_slots if k in fin and fin[k][2] is None and fin[k][1] == "BASE-F2-DEAD")
    out(f"BASE-F2-DEAD65 {dead_rows} dead RB row(s) over {len(rb_slots)} RB slots: redrawn alive {alive}, dead twice {twice}, not redrawn {unred} (a dead row never enters the test)")
    pslots = list(dict.fromkeys((sd, rp) for _, sd, rp in slots))
    rb_ok = {k: fin[("RB",) + k][0] for k in pslots if ("RB",) + k in fin and fin[("RB",) + k][1] == "LIVE"}
    # reported first, from RB alone: the half-split q95 per window
    for name, key in WINDOWS + [WHOLE]:
        by = {s: [val(r, key) for k, r in rb_ok.items() if k[0] == s and not r.get("dnf") and val(r, key) is not None] for s in SEEDS}
        by = {s: v for s, v in by.items() if v}
        q, ns = q95_halfsplit(by) if by else (None, 0)
        out(f"RESOLUTION65 {name:<5} (RB rows alone, REPORTED ONLY): q95|x|={'-' if q is None else '%.4f' % q} (splits={ns}, nRB={sum(len(v) for v in by.values())})  TOL={TOL:.4f}")
    rc_fin = {k: fin[("RC",) + k] for k in pslots if ("RC",) + k in fin}
    f2 = [(k, v[0].get("rt_items")) for k, v in rc_fin.items() if v[1] == "RT-FAIL"]
    out("F2-FINDING65 " + ("none (every RC row that ran passes items 1-4)" if not f2 else
                            f"{len(f2)} RC row(s) RT-FAIL: " + ", ".join(f"s{k[0]} r{k[1]} items {it}" for k, it in f2) + " - a finding against the merged fix, to the owner; these pairs are not scored"))
    rc_ok = {k: v[0] for k, v in rc_fin.items() if v[1] == "RT-OK"}
    pairs = [(k, rb_ok[k], rc_ok[k]) for k in pslots if k in rb_ok and k in rc_ok]
    dropped = [f"s{k[0]} r{k[1]}" for k in pslots if k not in rb_ok or k not in rc_ok]
    out(f"PAIRS65 usable {len(pairs)} of {n_pairs}; dropped: " + (", ".join(dropped) or "none") + " (a dead, void or RT-FAIL side drops its pair; a dead BASE row is never CAND's win)")
    res, dd = {}, {}
    for name, key in WINDOWS + [WHOLE]:
        d = paired(pairs, key)
        dd[name] = {k: x for k, x, _ in d}
        res[name] = test([x for _, x, _ in d], mp)
        out(f"  pair d = ln(RC/RB) {name}: " + " ".join(f"s{k[0]}r{k[1]}:{x:+.3f}{('(' + w + ')') if w else ''}" for k, x, w in d))
    for name, _ in WINDOWS + [WHOLE]:
        t = res[name]
        out(f"{'TEST65' if name != 'whole' else 'REPORTED65'} {name:<5} n={t['n']} (nonzero {t['nz']}) RC worse in {t['worse']}; W+={t['w']:g}; exact one-sided signed-rank p={t['p']:.4f}; "
            f"HL shift={100 * t['h_rel']:+.2f} %; one-sided 95 % upper bound={'inf' if t['u_rel'] == math.inf else '%+.2f %%' % (100 * t['u_rel'])} (c={t['c']}) -> "
            + (t["label"] if name != "whole" else f"({t['label']}, reported only)"))
    lab, why = session_label(res["head"]["label"], res["rest"]["label"])
    out(f"VERDICT65 {lab} (head {res['head']['label']}, rest {res['rest']['label']}: {why})")
    out("REPORTED65 by run order (seed 42's pair runs RB first, seed 7's RC first): per window the HL shift and RC-worse count")
    for s in SEEDS:
        parts = []
        for name, _ in WINDOWS + [WHOLE]:
            xs = [x for k, x in dd[name].items() if k[0] == s]
            if xs:
                t = test(xs)
                parts.append(f"{name} HL {100 * t['h_rel']:+.1f} % ({t['worse']}/{t['n']})")
        out(f"  s{s} ({first_seat(s)} first): " + ("; ".join(parts) or "-"))
    out("REPORTED65 early loss (quinn lost / sent packets on leg 0 by t = 2 s, client) per pair: RB | RC")
    for k, rb, rc in pairs:
        out(f"  s{k[0]}r{k[1]} ({first_seat(k[0])} first): RB {rb.get('qlost_2s')}/{rb.get('qsent_2s')} | RC {rc.get('qlost_2s')}/{rc.get('qsent_2s')}")
    dead = [r for r in rows if r["arm"] == "RB" and r["status"] == "BASE-F2-DEAD"]
    if dead:
        out("  dead RB rows: " + ", ".join(f"s{r['seed']}r{r['rep']} {r.get('qlost_2s')}/{r.get('qsent_2s')}" for r in dead))
    out("REPORTED65 other readouts (never a label): RC worse in k of n pairs, exact signed-rank p")
    for name, key, sign in READOUTS:
        xs = [x for _, x, _ in paired(pairs, key, sign)]
        if not xs:
            out(f"  {name:<13} - (absent on these rows)")
            continue
        t = test(xs)
        out(f"  {name:<13} RC worse in {t['worse']:2d} of {t['nz']}  p={t['p']:.4f}  HL {100 * t['h_rel']:+.2f} %")
    return {"label": lab, "head": res["head"], "rest": res["rest"], "whole": res["whole"], "n": len(pairs), "f2": len(f2)}


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    score(argv)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
