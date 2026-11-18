#!/usr/bin/env python3
"""Status 64, stage R's score (docs/derivations/f2-guard-registration.md 2.2): Realtime, c2-100, rho = 1, 100 MB, arms RB (BASE's binary) and RC (CAND's), 12 + 12 slots.

    rt64_score.py <ledger> [...]

Scored on every RC row: the F2 reading (items 1-4: warm-up acked; the receiving server's [CTLD] carries undeliv= and undeliv = 0 on every [CTLD] line of both ends; no [UNFRAMABLE]; [SYMSZ] 512/512 on both ends)
and, relative to BASE, completion (item 5: if every live RB row completes, every RC row whose items 1-4 pass must too; REPORTED when an RB row is a DNF; NOT-SCORED with no live RB row).
RT-PASS iff all RC slots hold a row that passes items 1-4 and item 5 holds wherever it applies. A row that fails an item is `RT-FAIL <item>` and is never replaced. A non-F2 void on an RC slot is replaced
once (at most 2 slots); a slot that still voids is `RC-VOID`, counts as not passed and reads RT-INCOMPLETE (not a pass, not a finding against F2).
Reported only: BASE-F2-DEAD k of N; per readout the RC and live-RB medians [min-max] and the one-sided rank p (RC worse than RB; the live RB rows are selected by a draw, so never a verdict); the owner-flag lines;
the received-symbols denominator per RC row."""
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import plan64  # noqa: E402

N_PERM = 10000
RNG_SEED = 4141
P_FLAG = 0.05
D80 = {"goodput": 0.024, "cpu_cli/GB": 0.08, "cpu_srv/GB": 0.08}          # c2's Bulk d80 at the guard size (guard-clause-dry.md 11, the v1 column): the owner-flag thresholds
READOUTS = [("goodput", "mbps", -1), ("cpu_cli/GB", "cpu_cli", 1), ("cpu_srv/GB", "cpu_srv", 1), ("lag_E_cli", "cli_lag_E", 1), ("lag_E_srv", "srv_lag_E", 1),
            ("inorder_delay_mean", "inorder_mean", 1), ("rtp_floor_p0", "rtp_floor_p0", 1)]
MIN_TICK_S = 30.0


def val(r, key):
    if r.get("dnf") or r.get(key) is None:
        return None
    if key in ("cpu_cli", "cpu_srv"):
        return r[key] / ((r.get("bytes") or plan64.BYTES_R) / 1e9)
    return r[key]


def med(v):
    v = [x for x in v if x is not None]
    return lc.med(v) if v else None


def _midranks(vals):
    idx = sorted(range(len(vals)), key=lambda i: vals[i])
    out = [0.0] * len(vals)
    j = 0
    while j < len(idx):
        k = j
        while k + 1 < len(idx) and vals[idx[k + 1]] == vals[idx[j]]:
            k += 1
        for t in range(j, k + 1):
            out[idx[t]] = (j + k) / 2 + 1
        j = k + 1
    return out


def rank_p(rc, rb, sign, n_perm=N_PERM, seed=RNG_SEED):
    """One-sided p that RC is worse than RB (sign +1: higher is worse): the mean worse-side midrank of the RC values against 10 000 relabellings of the pooled values (seeded, so reproducible).
    None when either side has no value."""
    if not rc or not rb:
        return None
    pool = [sign * x for x in rc + rb]
    ranks = _midranks(pool)
    k = len(rc)
    obs = sum(ranks[:k])
    rng = random.Random(seed)
    ge = 0
    for _ in range(n_perm):
        if sum(ranks[i] for i in rng.sample(range(len(pool)), k)) >= obs - 1e-9:
            ge += 1
    return (1 + ge) / (1 + n_perm)


def _mm(v):
    return f"{lc.med(v):.4g} [{min(v):.4g}-{max(v):.4g}]" if v else "-"


def _fmt_sz(v):
    return ",".join(f"{a}/{b}" for a, b in v) or "none"


def completed(r):
    return r.get("dnf") is False


def score(paths, out=print):
    import place38_parse as pp
    rows_all, tokens = pp.rows_of(paths)
    fin = plan64.final_rows(rows_all)
    table = plan64.plan()
    slots = [(c, a, sd, str(rp)) for c, _, a, sd, rp in table]
    n_rc, n_rb = plan64.expect_rc(), sum(1 for s in slots if s[1] == "RB")
    rc_rows, rb_rows, rc_void, rb_void, rb_dead, labels = [], [], [], [], [], {}
    for k in slots:
        r = fin.get(k)
        if k[1] == "RC":
            if r is None or plan64.is_void_r(r):
                rc_void.append(k)
                labels[k] = "RC-VOID"
            else:
                rc_rows.append(r)
                labels[k] = "RT-OK" if r["status"] == "RT-OK" else "RT-FAIL " + " ".join(str(i) for i in r.get("rt_items") or []) if r["status"] == "RT-FAIL" else r["status"]
        else:
            if r is None or plan64.is_void_r(r):
                rb_void.append(k)
                labels[k] = "RB-VOID"
            elif r["status"] == "BASE-F2-DEAD":
                rb_dead.append(r)
                labels[k] = "BASE-F2-DEAD"
            else:
                rb_rows.append(r)
                labels[k] = "RB-LIVE"
    out(f"S64R REPORT stage R (Realtime c2-100 at rho = 1, 100 MB, timeout {plan64.TIMEOUT_R} s; RB = BASE's binary, RC = CAND's): slots RC {n_rc}, RB {n_rb}; ledger rows {len(rows_all)}")
    repl = sum(1 for v in plan64.slot_rows(rows_all).values() if len(v) > 1)
    out(f"REPLACED SLOTS: {repl} of at most {plan64.MAX_REPLACEMENTS_R} (non-F2 voids, replaced once)" + ("" if repl else " - none"))
    # item 5
    if not rb_rows:
        item5 = "NOT-SCORED (no live RB row)"
    elif any(not completed(r) for r in rb_rows):
        item5 = "REPORTED (an RB row DNF)"
    else:
        item5 = "SCORED"
    fails = []
    for r in rc_rows:
        k = (r["cell"], r["arm"], str(r["seed"]), str(r["rep"]))
        if r["status"] == "RT-FAIL":
            fails.append(f"s{k[2]} r{k[3]} item {'+'.join(str(i) for i in r.get('rt_items') or [])}")
        elif item5 == "SCORED" and not completed(r):
            labels[k] = "RT-FAIL 5"
            fails.append(f"s{k[2]} r{k[3]} item 5")
        elif r["status"] != "RT-OK":
            labels[k] = r["status"]
            fails.append(f"s{k[2]} r{k[3]} status {r['status']}")
    for k in slots:
        r = fin.get(k)
        if r is None:
            out(f"ROW64R {k[1]} s{k[2]} r{k[3]}: NO ROW -> {labels[k]}")
            continue
        t = r.get("t64") or {}
        out(f"ROW64R {k[1]} s{k[2]} r{k[3]}: status={r['status']} label={labels[k]} rc={r.get('rc')} warm={t.get('warm')} transfer={'COMPLETED' if completed(r) else 'DNF' if r.get('dnf') else 'NONE'} "
            f"undeliv={t.get('undeliv')}(srv {t.get('undeliv_srv')}+cli {t.get('undeliv_cli')}) srv_ctld={t.get('ctld_srv')} unframable={t.get('unframable')} "
            f"symsz(cli)={_fmt_sz(t.get('symsz_cli') or [])} symsz(srv)={_fmt_sz(t.get('symsz_srv') or [])} window_reliable={t.get('wr_cli')}/{t.get('wr_srv')} bin_sha={str(r.get('bin_sha'))[:16]}"
            + (f" problems={' '.join(r['problems'])}" if r.get("problems") else "")
            + (f" | denominator (server received): src_n={t.get('src_n')} ctld_rx={t.get('srv_rx')} ctld_fr={t.get('srv_fr')}" if k[1] == "RC" else ""))
    out(f"ITEM5 {item5}: live RB rows {len(rb_rows)}, RB completions {sum(1 for r in rb_rows if completed(r))}/{len(rb_rows)}")
    out(f"BASE-F2-DEAD {len(rb_dead)}/{n_rb}" + (" (k = 0: the RC reading is weak, nothing reproduced; reported, never a void)" if not rb_dead and n_rb else "") + (f"; RB-VOID {len(rb_void)}" if rb_void else ""))
    # readouts
    out("READOUTS (reported only: RC against the live RB rows; the one-sided rank p is RC worse than RB, 10 000 seeded relabellings; the live RB rows are selected by a draw, so never a verdict)")
    flags = []
    for name, key, sign in READOUTS:
        c = [val(r, key) for r in rc_rows]
        b = [val(r, key) for r in rb_rows]
        c, b = [x for x in c if x is not None], [x for x in b if x is not None]
        p = rank_p(c, b, sign)
        mc, mb = med(c), med(b)
        x = sign * (mc - mb) / mb if (mc is not None and mb) else None
        note = ""
        if key == "inorder_mean":
            short = sum(1 for r in rc_rows + rb_rows if (r.get("tick_span_s") or 0.0) < MIN_TICK_S)
            note = f"  (marked: {short} of {len(rc_rows) + len(rb_rows)} rows have under {MIN_TICK_S:g} s of ticks, below guard v2's floor: no resolution)"
        out(f"  {name:<19} RC {_mm(c):<32} RB {_mm(b):<32} worse-by {('%+.1f%%' % (100 * x)) if x is not None else '-':<8} p={('%.4f' % p) if p is not None else '-'}{note}")
        if name in D80 and x is not None and p is not None and x > D80[name] and p <= P_FLAG:
            flags.append(f"OWNER-FLAG64 {name}: RC worse than RB by {100 * x:.1f}% (> c2's Bulk d80 {100 * D80[name]:.1f}%) with rank p={p:.4f} <= {P_FLAG}: goes to the owner with the merge question (non-gating)")
    for f in flags:
        out(f)
    if not flags:
        out("OWNER-FLAG64 none (no reported readout with RC worse than c2's d80 at rank p <= 0.05)")
    # verdict
    nok = sum(1 for r in rc_rows if labels[(r["cell"], r["arm"], str(r["seed"]), str(r["rep"]))] == "RT-OK")
    if fails:
        verdict = "RT-FAIL (" + "; ".join(fails) + ")"
    elif rc_void:
        verdict = "RT-INCOMPLETE (" + f"{len(rc_void)} RC-VOID slot(s): " + ", ".join(f"s{k[2]} r{k[3]}" for k in rc_void) + "; not a pass and not a finding against F2: the merge waits for a stage R re-run)"
    elif nok == n_rc:
        verdict = "RT-PASS"
    else:
        verdict = f"RT-INCOMPLETE ({nok}/{n_rc} RC slots passed)"
    if fails and rc_void:
        verdict += f" [+ {len(rc_void)} RC-VOID: RT-INCOMPLETE]"
    out(verdict)
    out(f"VERDICT64R {verdict.split(' ')[0]} rc_ok={nok}/{n_rc} rc_void={len(rc_void)} rc_fail={len(fails)} base_f2_dead={len(rb_dead)}/{n_rb} rb_void={len(rb_void)} item5={item5.split(' ')[0]}")
    return {"verdict": verdict.split(" ")[0], "fails": fails, "rc_void": rc_void, "dead": len(rb_dead), "item5": item5}


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    score(argv)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
