#!/usr/bin/env python3
"""Scores THE DEAD-WALL BATTERY against goal-gate "The Derived Recovery
Clamp — VM PRE-REGISTRATION" (commit 16284c0).

Written and committed BEFORE the run, like the battery it scores. Every bar
below is TRANSCRIBED from the pre-registration and none is computed from the
data: if a bar here disagrees with that block, the block wins and this file
is the bug.

    usage: deadwall_report.py <ledger.log> [<ledger.log> ...]

ARM ALIAS. The pre-registration writes the arms {A, D, AU, AUD}; the driver
writes {A, R, AU, AUR}. Same partition, and this file reports both spellings
so a row can be checked against the contract without inference.

C1 IS A STOP RULE. If the control does not reproduce the collapse class, no
other row is scored and the battery is reported UNSCORED, verbatim, exactly
as "The Latency-Feedback Source" reported its own matrix.

WHAT THIS FILE WILL NOT DO. It will not substitute a cell for a cell. C7 is
pre-registered on c1 AND c7 by name; if either is absent from the ledgers,
C7 is reported ABSENT and the guard cell that was run is reported beside it
as its own line, never in its place.
"""
import json
import statistics as st
import sys

# ── the bars, transcribed ────────────────────────────────────────────────
C1_BAR = 3 / 16          # p_A >= 3/16 pooled                    (STOP RULE)
C2_DROP = 4 / 16         # p_D <= p_A - 4/16
C3_RISE = 2 / 16         # p_AU >= p_A + 2/16
C4_SLACK = 1 / 16        # p_AUD <= p_D + 1/16   AND
C4_GAP = 3 / 16          # p_AUD <= p_AU - 3/16
C5_PING = 1.25           # c8 ping_p99(D) <= 1.25 x A
C6_RETX = 0.85           # c8 retx(D) <= 0.85 x A, medians
C8_LEN = 0.5             # p_A(200MB) <= 0.5 x p_A(25MB)

ALIAS = {"A": "A", "R": "D", "AU": "AU", "AUR": "AUD"}
DECISION = "c8"
LENGTH = "c8L"
C7_CELLS = ("c1", "c7")
GUARD = "sc2"


def load(paths):
    rows = []
    for p in paths:
        with open(p, errors="replace") as f:
            for ln in f:
                i = ln.find("DEADWALLRESULT ")
                if i < 0:
                    continue
                try:
                    rows.append(json.loads(ln[i + len("DEADWALLRESULT "):]))
                except Exception:
                    pass
    return rows


def sel(rows, cell, arm, seed=None):
    return [r for r in rows
            if r["cell"] == cell and r["arm"] == arm
            and (seed is None or r["seed"] == seed)]


def rate(rows):
    """The PRIMARY STATISTIC. Reps with no dead-wall verdict (the wait
    histogram never populated) are EXCLUDED from the denominator rather than
    counted as non-collapse — the pre-registration's `dnf`/abort discipline
    applied to its own statistic."""
    v = [r["deadwall"] for r in rows if r.get("deadwall") is not None]
    return (sum(v), len(v), (sum(v) / len(v) if v else None))


def med(rows, key):
    v = [r[key] for r in rows if r.get(key) is not None]
    return st.median(v) if v else None


def fmt(v, spec):
    return "-" if v is None else spec % v


def verdict(ok):
    return "PASS" if ok else "FAIL"


def main():
    rows = load(sys.argv[1:])
    if not rows:
        print("NO DEADWALLRESULT ROWS — nothing to score")
        return
    arms = ["A", "R", "AU", "AUR"]
    cells = [c for c in (DECISION, LENGTH, GUARD) + C7_CELLS
             if any(r["cell"] == c for r in rows)]

    print("=" * 78)
    print("THE DEAD-WALL BATTERY — scored against goal-gate")
    print('"The Derived Recovery Clamp — VM PRE-REGISTRATION" (commit 16284c0)')
    print("=" * 78)

    # ── the raw table ────────────────────────────────────────────────────
    print("\nPER ARM (pooled over seeds unless noted)")
    print(f"{'cell':5}{'arm':5}{'=preg':6}{'n':>4}{'dead':>6}{'rate':>7}"
          f"{'mbps':>9}{'retx':>9}{'p99':>8}{'dnf':>5}{'divDS':>7}")
    for c in cells:
        for a in arms:
            rs = sel(rows, c, a)
            if not rs:
                continue
            k, n, p = rate(rs)
            dnf = sum(1 for r in rs if r.get("dnf"))
            div = sum(1 for r in rs
                      if (r.get("diverged_ds_cli") or 0) + (r.get("diverged_ds_srv") or 0) > 0)
            f_p = "-" if p is None else "%.3f" % p
            f_g = fmt(med(rs, "mbps"), "%.1f")
            f_r = fmt(med(rs, "retx"), "%.0f")
            f_q = fmt(med(rs, "ping_p99"), "%.1f")
            print("%-5s%-5s%-6s%4d%6d%7s%9s%9s%8s%5d%7d"
                  % (c, a, ALIAS[a], len(rs), k, f_p, f_g, f_r, f_q, dnf, div))

    # ── C1, the stop rule ────────────────────────────────────────────────
    kA, nA, pA = rate(sel(rows, DECISION, "A"))
    print("\n" + "-" * 78)
    if pA is None:
        print("C1  STOP RULE  UNSCORABLE — no c8 control rep carries a dead-wall verdict")
        print("\nBATTERY REPORTED **UNSCORED**.")
        return
    c1_ok = pA >= C1_BAR
    print(f"C1  the control reproduces the mode   p_A = {kA}/{nA} = {pA:.3f}"
          f"   bar >= {C1_BAR:.3f}   {verdict(c1_ok)}   [STOP RULE]")
    if not c1_ok:
        print("\nThe collapse class is NOT present in this session. Per the")
        print("pre-registration, NO other row is scored and the battery is")
        print("reported **UNSCORED**, verbatim.")
        return

    # ── C2 / C3 / C4, the recovery-plane rows ────────────────────────────
    kR, nR, pR = rate(sel(rows, DECISION, "R"))
    kU, nU, pU = rate(sel(rows, DECISION, "AU"))
    kX, nX, pX = rate(sel(rows, DECISION, "AUR"))

    if pR is None:
        print("C2  UNSCORABLE — the R (=D) arm carries no dead-wall verdict at c8")
    else:
        print(f"C2  the repair moves the mode         p_D = {kR}/{nR} = {pR:.3f}"
              f"   bar <= {pA - C2_DROP:.3f}   {verdict(pR <= pA - C2_DROP)}")
    if pU is None:
        print("C3  UNSCORABLE — the AU arm carries no dead-wall verdict at c8")
    else:
        print(f"C3  the deeper pool's penalty is real p_AU = {kU}/{nU} = {pU:.3f}"
              f"   bar >= {pA + C3_RISE:.3f}   {verdict(pU >= pA + C3_RISE)}")

    print("\nC4  THE INTERACTION — the load-bearing clause. A CONJUNCTION: both")
    print("    legs must hold, so \"D good, AU bad\" cannot be read as \"AUD safe\".")
    if pX is None or pR is None or pU is None:
        print("    UNSCORABLE — one of AUD / D / AU carries no dead-wall verdict")
    else:
        leg1 = pX <= pR + C4_SLACK
        leg2 = pX <= pU - C4_GAP
        print(f"    p_AUD = {kX}/{nX} = {pX:.3f}")
        print(f"      leg 1  p_AUD <= p_D  + {C4_SLACK:.3f} = {pR + C4_SLACK:.3f}   {verdict(leg1)}")
        print(f"      leg 2  p_AUD <= p_AU - {C4_GAP:.3f} = {pU - C4_GAP:.3f}   {verdict(leg2)}")
        print(f"    C4 = {verdict(leg1 and leg2)}")

    # ── C5 / C6, the trade and the spurious plane ────────────────────────
    print()
    aP, rP = med(sel(rows, DECISION, "A"), "ping_p99"), med(sel(rows, DECISION, "R"), "ping_p99")
    if aP and rP:
        print(f"C5  no tail-latency regression bought ping_p99 D/A = {rP:.1f}/{aP:.1f}"
              f" = {rP / aP:.3f}   bar <= {C5_PING}   {verdict(rP <= C5_PING * aP)}")
        if rP > C5_PING * aP:
            print("    C5 FAIL with C2 PASS => the repair is a MEASURED TRADEOFF (dead")
            print("    wall for delivered latency). Report the p99 cost in the HEADLINE.")
            print("    It is NOT folded into a win.")
    else:
        print("C5  UNSCORABLE — no ping_p99 at c8")

    aR, rR = med(sel(rows, DECISION, "A"), "retx"), med(sel(rows, DECISION, "R"), "retx")
    if aR and rR:
        print(f"C6  the spurious plane shrinks        retx D/A = {rR:.0f}/{aR:.0f}"
              f" = {rR / aR:.3f}   bar <= {C6_RETX}   {verdict(rR <= C6_RETX * aR)}")
    else:
        print("C6  UNSCORABLE — no retx at c8")

    # ── C7, on ITS OWN cells, never a substitute ─────────────────────────
    print("\nC7  nothing moves where the clamp does not bind (c1 AND c7, both seeds,")
    print("    R within +-1 sigma of A; c1 is named as the cell where a law defect")
    print("    surfaces FIRST — its 2*srtt = 18 ms is BELOW the legacy band).")
    c7_seen = [c for c in C7_CELLS if any(r["cell"] == c for r in rows)]
    for c in C7_CELLS:
        if c not in c7_seen:
            print(f"    {c}: ABSENT from the ledgers — C7 is NOT scorable on {c}.")
            continue
        for seed in sorted({r["seed"] for r in rows if r["cell"] == c}):
            av = [r["mbps"] for r in sel(rows, c, "A", seed) if r.get("mbps")]
            rv = [r["mbps"] for r in sel(rows, c, "R", seed) if r.get("mbps")]
            if len(av) < 2 or not rv:
                print(f"    {c} s{seed}: too few reps to score")
                continue
            m, s = st.mean(av), st.stdev(av)
            rm = st.mean(rv)
            ok = abs(rm - m) <= s
            print(f"    {c} s{seed}: A {m:.1f} +-{s:.1f}   R {rm:.1f}"
                  f"   |d| = {abs(rm - m):.1f}   {verdict(ok)}")
    if len(c7_seen) < len(C7_CELLS):
        print("    => C7 INCOMPLETE. A FAIL here would be a DEFECT in the law, not a")
        print("       tradeoff, so an incomplete C7 may not be reported as a pass.")

    # ── the guard cell, on its own line, never in C7's place ─────────────
    if any(r["cell"] == GUARD for r in rows):
        print(f"\nGUARD CELL {GUARD} (store-cap-bound; NOT a C7 substitute)")
        for seed in sorted({r["seed"] for r in rows if r["cell"] == GUARD}):
            av = [r["mbps"] for r in sel(rows, GUARD, "A", seed) if r.get("mbps")]
            rv = [r["mbps"] for r in sel(rows, GUARD, "R", seed) if r.get("mbps")]
            if len(av) < 2 or not rv:
                continue
            m, s, rm = st.mean(av), st.stdev(av), st.mean(rv)
            print(f"    s{seed}: A {m:.1f} +-{s:.1f}   R {rm:.1f}"
                  f"   |d| = {abs(rm - m):.1f}   {'within' if abs(rm-m) <= s else 'OUTSIDE'} 1 sigma")

    # ── C8, the length artifact — reported on its own whatever else says ──
    print("\nC8  the length artifact (reported on its own, per the pre-registration,")
    print("    whatever the other rows say)")
    k2, n2, p2 = rate(sel(rows, LENGTH, "A"))
    if p2 is None:
        print("    UNSCORABLE — no 200 MB control rep carries a dead-wall verdict")
    else:
        print(f"    p_A(200MB) = {k2}/{n2} = {p2:.3f}   p_A(25MB) = {pA:.3f}"
              f"   bar <= {C8_LEN * pA:.3f}   {verdict(p2 <= C8_LEN * pA)}")
        if p2 <= C8_LEN * pA:
            print("    PASS => the c8 keying of five sections of the ledger is a")
            print("    BYTE-COUNT ARTIFACT and the cell was never special.")
        else:
            print("    FAIL => the mode is NOT a fixed tail; the c8 keying is")
            print("    STRUCTURAL and the clock story is incomplete.")

    # ── did the law ever BIND? (the null-result / null-effect separator) ─
    print("\nDID THE DERIVED LAW BIND? (coincidence property: ACTIVE alone proves")
    print("only that the site RAN; DIVERGED proves the two laws differed)")
    for c in cells:
        for a in ("R", "AUR"):
            rs = sel(rows, c, a)
            if not rs:
                continue
            act = sum(1 for r in rs if (r.get("active_ds_cli") or 0) + (r.get("active_ds_srv") or 0) > 0)
            div = sum(1 for r in rs if (r.get("diverged_ds_cli") or 0) + (r.get("diverged_ds_srv") or 0) > 0)
            dus = [r["ds_derived_us"] for r in rs if r.get("ds_derived_us")]
            lus = [r["ds_legacy_us"] for r in rs if r.get("ds_legacy_us")]
            extra = ""
            if dus and lus:
                extra = f"  derived {st.median(dus)/1000:.0f} ms vs clamped {st.median(lus)/1000:.0f} ms"
            print(f"    {c:4} {a:4} ACTIVE {act}/{len(rs)}   DIVERGED {div}/{len(rs)}{extra}")
            if act and not div:
                print(f"       ^ {c}-{a} is BIT-IDENTICAL to its control. Any null here is a")
                print("         null RESULT, not a null EFFECT.")

    # ── instrument and liveness hygiene ──────────────────────────────────
    bad = [r for r in rows
           if r.get("gates_cli_ds") != r.get("gates_srv_ds")
           or r.get("gates_cli_u") != r.get("gates_srv_u")]
    print(f"\nLIVENESS: {len(rows)} rows, {len(bad)} with an ENDPOINT-ASYMMETRIC gate")
    mp_off = [r for r in rows if r.get("gates_cli_mp") != 1 or r.get("gates_srv_mp") != 1]
    print(f"RWM_RECOV_MP armed on both endpoints in {len(rows) - len(mp_off)}/{len(rows)} rows")
    noack = [r for r in rows if not r.get("ackdiag_lines_cli")]
    print(f"[ACKDIAG] reported in {len(rows) - len(noack)}/{len(rows)} rows")
    nowait = [r for r in rows if r.get("deadwall") is None]
    print(f"reps with NO dead-wall verdict (excluded from every rate): {len(nowait)}")
    print(f"dnf rows: {sum(1 for r in rows if r.get('dnf'))}")


if __name__ == "__main__":
    main()
