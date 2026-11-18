#!/usr/bin/env python3
"""Status 65: the Realtime in-order delay re-check (docs/derivations/s65-registration.md). Plan 65 = plan 64's stage R table (c2-100, Realtime, rho = 1, arms RB = BASE's binary and RC = CAND's,
12 + 12, seeds 42 / 7 x 6, plan 41c's interleave) at 600 MB rows; ONE scored readout, the whole-run in-order delay mean, RC against RB PAIRED BY SLOT (seed, rep).

    s65_score.py <ledger> [...]

Order of the output (the registration's section 3):
  1. the slots: per slot the row that takes it (one redraw per slot: a non-F2 void, VOID-SHORT or BASE-F2-DEAD first row is replaced by the slot's second row);
  2. RESOLUTION from the RB rows ALONE, before any RC value is read: the q95 of |x| over the half-per-seed splits (place41b_score.resolution, the guard's construction), beside TOL;
  3. the F2 reading of the RC rows (RT-FAIL = an F2-FINDING, reported first, never a delay label);
  4. the paired test: d = ln(RC / RB) per usable pair; exact one-sided Wilcoxon signed-rank p (RC worse), the Hodges-Lehmann shift and its exact one-sided 95 % upper bound;
  5. the label (precedence: UNRESOLVED-N, REGRESSION, NO-REGRESSION, UNRESOLVED);
  6. reported only: the other readouts paired (count worse, signed-rank p) and the head (t <= 11.5 s) in-order mean, with its non-gating HEAD-FLAG65.
Pure and env-free: the rules are module constants; MIN_TICK_S is a parameter only so the path exercise can run the code on section 64's 12 s rows (labelled there)."""
import itertools
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

TOL = 0.05                    # the margin: l1ab.REL_FLOOR, the guard's delay-clause tolerance floor (sections 11-13; c2's printed in-order tol in section 64 stage G); fixed, never data-derived
ALPHA = 0.05                  # one-sided
MIN_PAIRS = 10                # fewer usable pairs -> UNRESOLVED-N
MIN_TICK_S = 30.0             # guard v2's floor (section 41d): a row with under 30 s of ticks is VOID-SHORT
HEAD_S = 11.5                 # the head window: section 64's rows ended at 11.5-11.8 s (reported only)
REDRAW = ("WITNESS-FAIL", "CONTAMINATED", "VOID-COTENANT", "VOID-ONDISK", "VOID-RHO", "VOID-RC", "NO_DATA", "VOID-SHORT", "BASE-F2-DEAD")
READOUTS = [("goodput", "mbps", -1), ("cpu_cli/GB", "cpu_cli", 1), ("cpu_srv/GB", "cpu_srv", 1), ("lag_E_cli", "cli_lag_E", 1), ("lag_E_srv", "srv_lag_E", 1),
            ("rtp_floor_p0", "rtp_floor_p0", 1), ("inorder_head", "inorder_mean_head", 1)]
SCORED = ("inorder_delay_mean", "inorder_mean", 1)
CS = {"floor": (), "inorder": ("c2-100",)}


def slot_key(r):
    return (str(r["seed"]), str(r["rep"]))


def short(r, min_tick):
    return r["status"] in ("LIVE", "RT-OK") and not r.get("dnf") and (r.get("tick_span_s") or 0.0) < min_tick


def eff_status(r, min_tick):
    return "VOID-SHORT" if short(r, min_tick) else r["status"]


def final_rows(rows, min_tick=MIN_TICK_S):
    """{(arm, seed, rep): (row or None, effective status, redrawn)}: the first row, or the slot's second row when the first is in REDRAW; later rows are ignored."""
    by = {}
    for r in rows:
        by.setdefault((r["arm"],) + slot_key(r), []).append(r)
    out = {}
    for k, rs in by.items():
        first = rs[0]
        if eff_status(first, min_tick) in REDRAW and len(rs) > 1:
            out[k] = (rs[1], eff_status(rs[1], min_tick), eff_status(first, min_tick))
        else:
            out[k] = (first, eff_status(first, min_tick), None)
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


def signed_rank_p(d):
    """Exact one-sided p = P0(W+ >= W+_obs) over the 2^n sign flips of the observed (mid)ranks of |d|; zeros dropped. -> (p, n_nonzero, W+)"""
    nz = [x for x in d if x != 0]
    if not nz:
        return 1.0, 0, 0.0
    rk = _midranks([abs(x) for x in nz])
    w = sum(r for r, x in zip(rk, nz) if x > 0)
    ge = sum(1 for s in itertools.product((0, 1), repeat=len(rk)) if sum(r for r, b in zip(rk, s) if b) >= w - 1e-9)
    return ge / 2 ** len(rk), len(nz), w


def wplus_cdf_c(n, alpha=ALPHA):
    """The largest c with P0(W+ <= c) <= alpha for n untied ranks (-1 when none)."""
    cnt = {}
    for s in itertools.product((0, 1), repeat=n):
        w = sum(i + 1 for i, b in enumerate(s) if b)
        cnt[w] = cnt.get(w, 0) + 1
    c, cum = -1, 0
    for w in range(n * (n + 1) // 2 + 1):
        cum += cnt.get(w, 0)
        if cum / 2 ** n <= alpha:
            c = w
        else:
            break
    return c


def hl_upper(d, alpha=ALPHA):
    """Hodges-Lehmann shift (median of the Walsh averages) and its exact one-sided (1 - alpha) upper bound W_(N - c) (1-based), c = wplus_cdf_c(n). U = inf when c < 0."""
    n = len(d)
    wa = sorted((d[i] + d[j]) / 2 for i in range(n) for j in range(i, n))
    N = len(wa)
    h = wa[N // 2] if N % 2 else (wa[N // 2 - 1] + wa[N // 2]) / 2
    c = wplus_cdf_c(n, alpha)
    u = wa[N - c - 1] if c >= 0 else math.inf
    return h, u, c


def paired(pairs, key, sign):
    """[(slot, d)] with d = sign * ln(RC / RB); a DNF side is the worst rank on its side (d = +-L, L = 1 + max |finite d|), both DNF d = 0; a pair missing a value is left out."""
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


def label(n, p, h_rel, u_rel, q95):
    if n < MIN_PAIRS:
        return "UNRESOLVED-N", f"{n} usable pairs < {MIN_PAIRS}"
    if p <= ALPHA and h_rel > TOL:
        return "REGRESSION", f"signed-rank p {p:.4f} <= {ALPHA} and HL shift {100 * h_rel:+.2f} % > TOL {100 * TOL:.0f} %"
    if u_rel <= TOL and q95 is not None and q95 <= TOL:
        return "NO-REGRESSION", f"upper bound {100 * u_rel:+.2f} % <= TOL and RB resolution q95 {100 * q95:.2f} % <= TOL"
    why = []
    if p <= ALPHA:
        why.append(f"p {p:.4f} <= {ALPHA} but HL shift {100 * h_rel:+.2f} % <= TOL")
    if u_rel > TOL:
        why.append(f"upper bound {100 * u_rel:+.2f} % > TOL")
    if q95 is None or q95 > TOL:
        why.append(f"RB resolution q95 {'-' if q95 is None else '%.2f %%' % (100 * q95)} > TOL: the design cannot see TOL")
    return "UNRESOLVED", "; ".join(why)


def score(paths, out=print, min_tick=MIN_TICK_S, rows=None):
    import place38_parse as pp
    import place41b_score as g
    import plan64
    if rows is None:
        rows, _ = pp.rows_of(paths)
    fin = final_rows(rows, min_tick)
    slots = [(a, sd, str(rp)) for _, _, a, sd, rp in plan64.plan()]
    out(f"S65 REPORT (Realtime c2-100 at rho = 1; RB = BASE's binary, RC = CAND's; scored: whole-run in-order delay mean, paired by slot; TOL {100 * TOL:.0f} %, one-sided alpha {ALPHA}, "
        f"min pairs {MIN_PAIRS}, min ticks {min_tick:g} s): ledger rows {len(rows)}")
    n_redraw = {}
    for a, sd, rp in slots:
        r, st, was = fin.get((a, sd, rp), (None, "NO-ROW", None))
        if was:
            n_redraw[was] = n_redraw.get(was, 0) + 1
        out(f"SLOT65 {a} s{sd} r{rp}: {st}" + (f" (redrawn: first row {was})" if was else "")
            + ("" if r is None else f" dnf={r.get('dnf')} ticks={r.get('tick_span_s')} bin_sha={str(r.get('bin_sha'))[:16]}"))
    out("REDRAWS " + (", ".join(f"{k} {v}" for k, v in sorted(n_redraw.items())) or "none"))
    rb_slots = [(a, sd, rp) for a, sd, rp in slots if a == "RB"]
    dead_rows = sum(1 for r in rows if r["arm"] == "RB" and r["status"] == "BASE-F2-DEAD")
    alive = sum(1 for k in rb_slots if k in fin and fin[k][2] == "BASE-F2-DEAD" and fin[k][1] == "LIVE")
    twice = sum(1 for k in rb_slots if k in fin and fin[k][2] == "BASE-F2-DEAD" and fin[k][1] == "BASE-F2-DEAD")
    unred = sum(1 for k in rb_slots if k in fin and fin[k][2] is None and fin[k][1] == "BASE-F2-DEAD")
    out(f"BASE-F2-DEAD65 {dead_rows} dead RB row(s) over {len(rb_slots)} RB slots: redrawn alive {alive}, dead twice {twice}, not redrawn {unred} (a dead row never enters the test)")
    rb_ok = {(sd, rp): fin[("RB", sd, rp)][0] for _, sd, rp in slots if ("RB", sd, rp) in fin and fin[("RB", sd, rp)][1] == "LIVE"}
    # 2. resolution: RB alone, completed rows, before RC is read
    rb_rows = [dict(r, store="def") for r in rb_ok.values() if not r.get("dnf")]
    res = [x for x in g.resolution(rb_rows, arm="RB", cells=[("c2-100", "def")], clause_set=CS) if x["clause"] == SCORED[0]]
    q95 = res[0]["q95"] if res else None
    out(f"RESOLUTION65 (RB rows alone, before RC is read): {SCORED[0]} q95|x|={'-' if q95 is None else '%.4f' % q95} (splits={res[0]['splits'] if res else 0}, nRB={len(rb_rows)})  TOL={TOL:.4f}  "
        + ("q95 <= TOL" if q95 is not None and q95 <= TOL else "q95 > TOL: the design cannot see TOL (NO-REGRESSION unavailable)"))
    # 3. the F2 reading of RC
    rc_fin = {(sd, rp): fin[("RC", sd, rp)] for _, sd, rp in slots if ("RC", sd, rp) in fin}
    f2 = [(k, v[0].get("rt_items")) for k, v in rc_fin.items() if v[1] == "RT-FAIL"]
    out("F2-FINDING65 " + ("none (every RC row that ran passes items 1-4)" if not f2 else
                            f"{len(f2)} RC row(s) RT-FAIL: " + ", ".join(f"s{k[0]} r{k[1]} items {it}" for k, it in f2) + " - a finding against the merged fix, to the owner; these pairs are not scored"))
    rc_ok = {k: v[0] for k, v in rc_fin.items() if v[1] == "RT-OK"}
    pslots = list(dict.fromkeys((sd, rp) for _, sd, rp in slots))            # each (seed, rep) once, in plan order
    pairs = [(k, rb_ok[k], rc_ok[k]) for k in pslots if k in rb_ok and k in rc_ok]
    dropped = [f"s{k[0]} r{k[1]}" for k in pslots if k not in rb_ok or k not in rc_ok]
    out(f"PAIRS65 usable {len(pairs)} of 12; dropped: " + (", ".join(dropped) or "none") + " (a dead, void or RT-FAIL side drops its pair; a dead BASE row is never CAND's win)")
    # 4. the paired test
    d = paired(pairs, SCORED[1], SCORED[2])
    out("  pair d = ln(RC/RB) in-order mean: " + " ".join(f"s{k[0]}r{k[1]}:{x:+.3f}{('(' + w + ')') if w else ''}" for k, x, w in d))
    xs = [x for _, x, _ in d]
    p, n_nz, w = signed_rank_p(xs)
    if xs:
        h, u, c = hl_upper(xs)
        h_rel, u_rel = math.exp(h) - 1, (math.exp(u) - 1 if u != math.inf else math.inf)
    else:
        h_rel = u_rel = math.inf
        c = -1
    out(f"TEST65 n={len(xs)} (nonzero {n_nz}) RC worse in {sum(1 for x in xs if x > 0)}; W+={w:g}; exact one-sided signed-rank p={p:.4f}; HL shift={100 * h_rel:+.2f} %; "
        f"one-sided 95 % upper bound={'inf' if u_rel == math.inf else '%+.2f %%' % (100 * u_rel)} (c={c})")
    lab, why = label(len(xs), p, h_rel, u_rel, q95)
    out(f"VERDICT65 {lab} ({why})")
    # 6. reported only
    out("REPORTED65 (never a label): per readout, RC worse in k of n pairs, exact signed-rank p, RC / RB medians")
    for name, key, sign in READOUTS:
        dd = [x for _, x, _ in paired(pairs, key, sign)]
        if not dd:
            out(f"  {name:<19} - (absent on these rows)")
            continue
        pp_, nz, _ = signed_rank_p(dd)
        mc = [val(c_, key) for _, _, c_ in pairs if val(c_, key) is not None]
        mb = [val(b_, key) for _, b_, _ in pairs if val(b_, key) is not None]
        out(f"  {name:<19} RC worse in {sum(1 for x in dd if x > 0):2d} of {nz}  p={pp_:.4f}  RC med {sorted(mc)[len(mc) // 2]:.4g}  RB med {sorted(mb)[len(mb) // 2]:.4g}")
        if key == "inorder_mean_head":
            hh = math.exp(hl_upper(dd)[0]) - 1
            out(f"HEAD-FLAG65 " + (f"RAISED: the head (t <= {HEAD_S:g} s) in-order mean is worse on RC, p {pp_:.4f} <= {ALPHA} and HL {100 * hh:+.2f} % > TOL; goes to the owner with the label (non-gating)"
                                   if pp_ <= ALPHA and hh > TOL else f"none (head p {pp_:.4f}, HL {100 * hh:+.2f} %)"))
    return {"label": lab, "p": p, "h_rel": h_rel, "u_rel": u_rel, "q95": q95, "n": len(xs), "f2": len(f2)}


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    score(argv)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
