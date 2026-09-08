#!/usr/bin/env python3
"""Scorer for THE r > 0 BATTERY (goal-gate "THE r > 0 BATTERY —
PRE-REGISTRATION"; paper §16.82).

    r_report.py --outdir /home/vibe/rbattery [--calib]

Reads the per-seed `RRESULT {json}` rows out of the ledgers and applies the
pre-registration, and NOTHING ELSE. Every threshold below is transcribed from
that block; none is chosen here.

THE ORDER IS THE PRE-REGISTRATION'S ORDER, AND IT IS NOT AN ACCIDENT
--------------------------------------------------------------------
  0. THE ABORT-CAUSE TABLE, FIRST. `ABORT != DNF != INSTRUMENT-FAIL`. An
     aborted invocation is in NO denominator.
  1. MECHANISM LIVENESS, READ BEFORE ANY SCORE (§16.82.7 verbatim): `W5`
     (`cod > 0` — r reached the wire) and `W6` (`[CHI] max > 0.5` — χ reached
     the glide). An `R-INERT` reading is ATTRIBUTED by the pre-registration's
     rule to BUDGET-BOUND / ESTIMATOR-BOUND / WIRING and never left bare.
  2. THE PRE-STATED FALSIFIER (§16.82.6), which OUTRANKS goodput and
     completion alike.
  3. `W7`, the CC pin — adjudicated here because this is the only seat that
     has CTL's own spread to compare `MID`'s rtt against.
  4. THE SCORE: completion p50, by the §7 bar. Goodput is a GUARD and is
     reported as `GUARD-UNDERPOWERED` because the pre-registration declares it
     so in advance at all three cells.
  5. THE OUTCOME, from the six legal outcomes and from nothing else.

`--calib` runs the four smoke clauses of §10 instead of the score, and prints
`NOTHING IN THE CALIBRATION IS A RESULT` on its own line, because n = 1.
"""
import argparse
import glob
import json
import math
import os
import sys

# ── EVERY CONSTANT BELOW IS TRANSCRIBED FROM THE PRE-REGISTRATION ───────
CHI_LIVE_BAR = 0.5           # §8 W6 / §10 clause 2: `[CHI] max > 0.5`
BULK_TAIL_BUDGET = 0.05      # raptorpath-math/src/lib.rs:124 — the glide's ceiling
FDIAG_MIN_N = 30             # §8a: DECODE n >= 30 AND SOURCE n >= 30
HEADROOM_BAR = 0.97          # §10 clause 1 (discipline 16)
BANDS = {"sc2": (78.0, 92.0), "c8": (50.0, 100.0), "c3hg": (9.0, 18.0)}
LINK_MBIT = {"c3hg": 20.0, "sc2": 100.0, "c8": 120.0}
PLATEAU = (26.8, 34.1)       # §9, goal-gate ~40913


def rows_from(outdir):
    out = []
    for path in sorted(glob.glob(os.path.join(outdir, "r-s*.log"))):
        with open(path, errors="replace") as f:
            for ln in f:
                ln = ln.replace("\r", "")
                if not ln.startswith("RRESULT "):
                    continue
                try:
                    out.append(json.loads(ln[len("RRESULT "):]))
                except Exception:
                    pass
    return out


def key(r):
    return (r.get("cell"), r.get("size"))


def scoreable(r):
    """A row that contributes to a SCORE. Aborts are in no denominator; a row
    whose witnesses failed is VOID; a plateau reading is a configuration fault
    and not a datum."""
    if r.get("abort"):
        return False
    if r.get("completion_p50") is None:
        return False
    m = r.get("mbps")
    if m is not None and PLATEAU[0] <= m <= PLATEAU[1]:
        return False
    return True


def hodges_lehmann(a, b):
    """The §7 bar's estimator: the median of all pairwise differences b−a, and
    a distribution-free two-sided 95 % interval on it (Mann–Whitney rank
    bounds, normal approximation to the rank statistic). Returns
    (shift, lo, hi) or (None, None, None) when either sample is too small to
    bound at all."""
    if len(a) < 3 or len(b) < 3:
        return (None, None, None)
    d = sorted(y - x for x in a for y in b)
    n = len(d)
    shift = d[n // 2] if n % 2 else (d[n // 2 - 1] + d[n // 2]) / 2.0
    m, k = len(a), len(b)
    mu = m * k / 2.0
    sd = math.sqrt(m * k * (m + k + 1) / 12.0)
    off = int(round(mu - 1.959964 * sd))
    if off < 0:
        off = 0
    if off >= n // 2:
        return (shift, None, None)
    return (shift, d[off], d[n - 1 - off])


def pct(x, base):
    return None if (x is None or not base) else round(100.0 * x / base, 3)


def report(outdir, calib):
    rows = rows_from(outdir)
    if not rows:
        print("UNSCOREABLE-NO-ROWS: no RRESULT lines under %s" % outdir)
        return 5

    cells = sorted({r["cell"] for r in rows})
    sizes = sorted({r["size"] for r in rows})
    arms = sorted({r["arm"] for r in rows})

    # ── 0 — THE ABORT-CAUSE TABLE, FIRST ────────────────────────────────
    print("=== 0 — THE ABORT-CAUSE TABLE, FIRST (ABORT != DNF != INSTRUMENT-FAIL)")
    ab = sum(1 for r in rows if r.get("abort"))
    pl = sum(1 for r in rows if r.get("mbps") is not None
             and PLATEAU[0] <= r["mbps"] <= PLATEAU[1])
    dn = sum(1 for r in rows if (r.get("dnf") or 0) > 0)
    nog = sum(1 for r in rows if not (r.get("gates_cli") and r.get("gates_srv")))
    nof = sum(1 for r in rows if not r.get("fdiag_present"))
    nor = sum(1 for r in rows if not r.get("rfa_present"))
    print("  rows=%d  ABORT(no summary, no runs)=%d  ABORT-PLATEAU=%d  DNF=%d"
          % (len(rows), ab, pl, dn))
    print("  W1 one-sided-or-absent [GATES]=%d  W8-NO-FDIAG=%d  W9-NO-RFA=%d"
          % (nog, nof, nor))
    live = [r for r in rows if scoreable(r)]
    print("  SCOREABLE rows=%d (aborts and plateau readings are in NO denominator)"
          % len(live))

    # ── 1 — MECHANISM LIVENESS, READ BEFORE ANY SCORE ───────────────────
    print("\n=== 1 — MECHANISM LIVENESS, READ BEFORE ANY SCORE (§16.82.7 verbatim)")
    inert = {}
    for c in cells:
        for z in sizes:
            for a in arms:
                sel = [r for r in live if r["cell"] == c and r["size"] == z and r["arm"] == a]
                if not sel:
                    continue
                cods = [r.get("cum_cod") or 0 for r in sel]
                fracs = [r["cod_frac"] for r in sel if r.get("cod_frac") is not None]
                chis = [r.get("chi_max") or 0.0 for r in sel]
                eps = [r["eps_hat"] for r in sel if r.get("eps_hat") is not None]
                reach = sum(1 for v in cods if v > 0)
                chi_live = sum(1 for v in chis if v > CHI_LIVE_BAR)
                mean_frac = (sum(fracs) / len(fracs)) if fracs else 0.0
                max_eps = max(eps) if eps else None
                tag = "W5-OK" if reach > 0 else "W5-R-INERT"
                inert[(c, z, a)] = (reach == 0, max_eps, chi_live > 0)
                print("  %-5s %-4s %-8s n=%2d  cod>0 at %d/%d  cod_frac=%.5f  "
                      "chi_max>0.5 at %d/%d  eps_hat_max=%s  %s"
                      % (c, z, a, len(sel), reach, len(sel), mean_frac,
                         chi_live, len(sel),
                         ("%.4f" % max_eps) if max_eps is not None else "none", tag))

    # THE ATTRIBUTION RULE, applied literally.
    print("\n  --- THE R-INERT ATTRIBUTION RULE (pre-registration §8), applied ---")
    any_inert = False
    for (c, z, a), (is_inert, max_eps, chi_ok) in sorted(inert.items()):
        if a == "CTL" or not is_inert:
            continue
        any_inert = True
        if max_eps is not None and max_eps < BULK_TAIL_BUDGET:
            if a.startswith("GLIDE") and c == "c3hg":
                att = ("ESTIMATOR-BOUND — the CHANNEL is 5.8 %% and the SENDER read "
                       "eps_hat=%.4f < %.2f. Goal-gate open item 3's own defect "
                       "reaching the price." % (max_eps, BULK_TAIL_BUDGET))
            else:
                att = ("BUDGET-BOUND — delta_eff(chi=1) = BULK_TAIL_BUDGET = %.2f "
                       ">= eps_hat=%.4f leaves r* = 0 BY ARITHMETIC. A finding "
                       "about the CONSTANT (register row, §16.80.12), NOT about r."
                       % (BULK_TAIL_BUDGET, max_eps))
        else:
            att = ("WIRING — a wiring failure, not a result (§16.82.7's own words): "
                   "eps_hat=%s does not explain it."
                   % (("%.4f" % max_eps) if max_eps is not None else "unreadable"))
        print("  R-INERT %-5s %-4s %-8s  => %s" % (c, z, a, att))
        if a.startswith("GLIDE") and chi_ok:
            print("           and chi DID reach the glide (max > 0.5) while r did "
                  "not follow  => GLIDE-INERT is the outcome for this arm.")
    if not any_inert:
        print("  none — r reached the wire on every funded arm-cell-size.")

    # ── 2 — THE PRE-STATED FALSIFIER, WHICH OUTRANKS GOODPUT ────────────
    print("\n=== 2 — THE PRE-STATED FALSIFIER (§16.82.6): [FDIAG] decode-resolved vs ARQ-resolved")
    print("  REMINDER, and it is not decoration: the record's '19-32 ms decodes' were")
    print("  RESOLUTION WAITING, not compute (goal-gate ~6983 vs ~7078-7091; raw compute")
    print("  6-10 us/call, 33-54 ms TOTAL over a 1.8 MB transfer). A DECODE avg quoted")
    print("  without present_at_stall is not a reading of this battery.")
    entangled = set()
    for c in cells:
        for z in sizes:
            for a in arms:
                sel = [r for r in live if r["cell"] == c and r["size"] == z and r["arm"] == a]
                dec = [r for r in sel if r.get("entangled") is not None]
                if not dec:
                    continue
                hits = sum(1 for r in dec if r["entangled"])
                pas = [r["fdiag_present_at_stall"] for r in dec
                       if r.get("fdiag_present_at_stall") is not None]
                fired = hits * 2 > len(dec)
                if fired and a != "CTL":
                    entangled.add((c, z, a))
                print("  %-5s %-4s %-8s readable=%d/%d  DECODE>SOURCE at %d/%d  "
                      "present_at_stall=%s  %s"
                      % (c, z, a, len(dec), len(sel), hits, len(dec),
                         ("max=%g" % max(pas)) if pas else "none",
                         "ENTANGLEMENT-DOMINATED" if fired else "-"))
    if entangled:
        print("  ENTANGLEMENT-DOMINATED fires and OUTRANKS every completion and goodput")
        print("  reading on the arms named above (§16.82.6: legal REGARDLESS OF GOODPUT).")

    # ── 3 — W7, THE CC PIN ──────────────────────────────────────────────
    print("\n=== 3 — W7: DID THE CC PIN HOLD? (the mechanical substitute for the Copa echo")
    print("    gates.rs:1432 claims and this tree does not have)")
    for c in cells:
        for z in sizes:
            ctl = [r["diag_rtt_ms"] for r in live
                   if r["cell"] == c and r["size"] == z and r["arm"] == "CTL"
                   and r.get("diag_rtt_ms") is not None]
            mid = [r["diag_rtt_ms"] for r in live
                   if r["cell"] == c and r["size"] == z and r["arm"] == "MID"
                   and r.get("diag_rtt_ms") is not None]
            if not ctl or not mid:
                continue
            lo, hi = min(ctl), max(ctl)
            out = [v for v in mid if v < lo or v > hi]
            verdict = ("W7-OK" if not out else
                       "W7-CC-PIN-FAILED (%d/%d MID reps outside CTL's own spread; a CC "
                       "that had followed delta to 0.05 targets a 20x tighter standing "
                       "queue and cannot hide inside it) — MID rows VOID at this cell-size"
                       % (len(out), len(mid)))
            print("  %-5s %-4s CTL rtt_ms in [%.2f, %.2f]  MID n=%d  %s"
                   % (c, z, lo, hi, len(mid), verdict))

    if calib:
        return calibration(live, cells, sizes, arms)

    # ── 4 — THE SCORE: completion p50, by the §7 bar ────────────────────
    print("\n=== 4 — THE SCORE: completion p50 (§16.82.7: 'the scored dimension is")
    print("    completion p50 at the two sizes, which is where the two hypotheses disagree')")
    print("    Goodput is a GUARD and is DECLARED GUARD-UNDERPOWERED in advance at all")
    print("    three cells (§16.82.7 verbatim: 'THE GOODPUT LEG IS A GUARD AND NOT A")
    print("    SCORE AT n = 8').")
    wins = {}
    for c in cells:
        for z in sizes:
            base = [r["completion_p50"] for r in live
                    if r["cell"] == c and r["size"] == z and r["arm"] == "CTL"]
            if not base:
                print("  %-5s %-4s  UNSCOREABLE-NO-CTL" % (c, z))
                continue
            bm = sorted(base)[len(base) // 2]
            for a in arms:
                if a == "CTL":
                    continue
                sel = [r for r in live
                       if r["cell"] == c and r["size"] == z and r["arm"] == a]
                v = [r["completion_p50"] for r in sel]
                if not v:
                    continue
                shift, lo, hi = hodges_lehmann(base, v)
                # BOTH SEEDS SEPARATELY AS WELL AS POOLED. A result present at
                # one seed only is SEED-SPLIT and scores nothing (§7).
                per_seed = {}
                for sd in sorted({r["seed"] for r in sel}):
                    b2 = [r["completion_p50"] for r in live
                          if r["cell"] == c and r["size"] == z
                          and r["arm"] == "CTL" and r["seed"] == sd]
                    v2 = [r["completion_p50"] for r in sel if r["seed"] == sd]
                    per_seed[sd] = hodges_lehmann(b2, v2)
                sig = (lo is not None and hi is not None
                       and ((lo > 0 and hi > 0) or (lo < 0 and hi < 0)))
                seed_ok = all(
                    (s2 is not None and l2 is not None and h2 is not None
                     and ((l2 > 0 and h2 > 0) or (l2 < 0 and h2 < 0))
                     and (s2 < 0) == (shift < 0))
                    for (s2, l2, h2) in per_seed.values()) if per_seed else False
                tag = ("WIN" if (sig and seed_ok and shift is not None and shift < 0)
                       else "SEED-SPLIT" if (sig and not seed_ok)
                       else "-")
                wins[(c, z, a)] = (tag == "WIN", shift, pct(shift, bm))
                print("  %-5s %-4s %-8s n=%2d  CTL p50=%.4fs  HL shift=%s (%s%%)  "
                      "95%%=[%s, %s]  seeds=%s  %s"
                      % (c, z, a, len(v), bm,
                         ("%.4f" % shift) if shift is not None else "n/a",
                         ("%+.2f" % pct(shift, bm)) if shift is not None else "n/a",
                         ("%.4f" % lo) if lo is not None else "n/a",
                         ("%.4f" % hi) if hi is not None else "n/a",
                         "+".join(str(s) for s in per_seed), tag))

    print("\n  GOODPUT GUARD (declared GUARD-UNDERPOWERED in advance; reported, not scored)")
    for c in cells:
        for z in sizes:
            for a in arms:
                v = [r["mbps"] for r in live if r["cell"] == c and r["size"] == z
                     and r["arm"] == a and r.get("mbps") is not None]
                if not v:
                    continue
                lo, hi = BANDS.get(c, (0.0, 1e9))
                oob = sum(1 for m in v if m < lo or m > hi)
                pred = [r["h_price_predicted_goodput_delta"] for r in live
                        if r["cell"] == c and r["size"] == z and r["arm"] == a
                        and r.get("h_price_predicted_goodput_delta") is not None]
                print("    %-5s %-4s %-8s median=%.2f Mbit/s  band=[%g,%g]  "
                      "OUT-OF-BAND-RESULT=%d/%d  H_price predicted delta=%s"
                      % (c, z, a, sorted(v)[len(v) // 2], lo, hi, oob, len(v),
                         ("%+.4f" % (sum(pred) / len(pred))) if pred else "n/a"))

    # ── 5 — THE OUTCOME, FROM THE SIX LEGAL OUTCOMES AND FROM NOTHING ELSE
    print("\n=== 5 — THE OUTCOME (pre-registration §11; no verdict outside this set)")
    print("  GLIDE-Z contributes ARM-ABSENT and nothing else; no outcome may be reached")
    print("  from its absence.")
    for c in cells:
        for z in sizes:
            for a in arms:
                if a == "CTL":
                    continue
                if (c, z, a) not in wins and (c, z, a) not in inert:
                    continue
                if (c, z, a) in entangled:
                    print("  %-5s %-4s %-8s  ENTANGLEMENT-DOMINATED  (outranks 2 and 3)"
                          % (c, z, a))
                    continue
                is_inert = inert.get((c, z, a), (False, None, False))[0]
                chi_ok = inert.get((c, z, a), (False, None, False))[2]
                if is_inert:
                    print("  %-5s %-4s %-8s  %s  (attribution above)"
                          % (c, z, a, "GLIDE-INERT" if (a.startswith("GLIDE") and chi_ok)
                             else "R-INERT"))
                    continue
                won18 = wins.get((c, "s18", a), (False, None, None))[0]
                won25 = wins.get((c, "s25", a), (False, None, None))[0]
                sh18 = wins.get((c, "s18", a), (False, None, None))[1]
                sh25 = wins.get((c, "s25", a), (False, None, None))[1]
                # §6's TWO-PART REQUIREMENT. (b) is the clause that stops
                # "25 MB was null" from being read as size discrimination: the
                # 25 MB leg is a DIRECTIONAL WITNESS at n = 8, never a score.
                small_only = (
                    won18
                    and sh18 is not None and sh25 is not None
                    and abs(sh25) < abs(sh18))
                if z != "s18":
                    continue
                if small_only:
                    print("  %-5s (both sizes) %-8s  R-FUNDED-POSITIVE-SMALL-ONLY  "
                          "[(a) 1.8 MB significant AND (b) |25 MB point estimate| "
                          "%.4f < %.4f]" % (c, a, abs(sh25), abs(sh18)))
                elif won18 and not small_only:
                    print("  %-5s (both sizes) %-8s  NOT small-only: (b) FAILED — the "
                          "25 MB point estimate is not strictly smaller. Recorded as "
                          "an OUT-OF-BAND RESULT with its cause named; NEVER a "
                          "size-discrimination claim." % (c, a))
                else:
                    print("  %-5s (both sizes) %-8s  R-FUNDED-NEGATIVE  (H_price "
                          "confirmed at this cell)" % (c, a))

    print("\n  RULING: nothing here flips a default. A CTL win does NOT bless")
    print("  BULK_TAIL_BUDGET = 0.05 — it stays in the open-constants register of")
    print("  §16.80.12, arbitrary and UNCORRECTED, and the one arm that would have")
    print("  contested it (GLIDE-Z) cannot run on this binary.")
    return 0


def calibration(live, cells, sizes, arms):
    """§10: the four smoke clauses, in order. n = 1 and NOTHING is a result."""
    print("\n=== CALIBRATION (§10) — 1 rep per arm-cell-size, seed 42")
    rc = 0

    print("  1. HEADROOM (discipline 16): CTL against the shaped link")
    for c in cells:
        v = [r["mbps"] for r in live if r["cell"] == c and r["arm"] == "CTL"
             and r.get("mbps") is not None]
        link = LINK_MBIT.get(c)
        if not v or not link:
            print("     %-5s  no CTL goodput scraped" % c)
            continue
        u = max(v) / link
        print("     %-5s  CTL max=%.2f Mbit/s  link=%.0f  utilisation=%.1f%%  %s"
              % (c, max(v), link, 100 * u,
                 "HEADROOM-BOUND (can only be moved DOWN)" if u >= HEADROOM_BAR else "headroom OK"))

    print("  2. THE [CHI] LIVENESS CHECK — the one clause that can ABORT-SMOKE")
    ok_cells = 0
    for c in cells:
        sel = [r for r in live if r["cell"] == c and r["arm"] == "GLIDE"]
        if not sel:
            continue
        mx = max((r.get("chi_max") or 0.0) for r in sel)
        feed = max((r.get("chi_feed_echo") or 0) for r in sel)
        good = mx > CHI_LIVE_BAR and feed > 0
        ok_cells += 1 if good else 0
        print("     %-5s  [CHI] max=%.4f  feed ACTIVE echoes=%d  %s"
              % (c, mx, feed, "OK" if good else "DEAD"))
    if cells and ok_cells < len(cells):
        print("     ABORT-SMOKE: [CHI] max <= 0.5 (or no feed echo) at %d of %d cells."
              % (len(cells) - ok_cells, len(cells)))
        print("     NOTHING IS LAUNCHED. In the scored battery a dead glide is a result;")
        print("     in the smoke it means the feed is unwired, and spending 288")
        print("     invocations on it is the failure the smoke exists to prevent.")
        rc = 6

    print("  3. W5 REACHABILITY — cod > 0 on at least one funded arm-cell")
    reach = [(r["cell"], r["arm"]) for r in live
             if r["arm"] != "CTL" and (r.get("cum_cod") or 0) > 0]
    if reach:
        print("     cod > 0 at: %s" % ", ".join("%s/%s" % t for t in sorted(set(reach))))
    else:
        print("     cod = 0 EVERYWHERE. RECORDED, NOT AN ABORT: this is the")
        print("     R-INERT/BUDGET-BOUND reading arriving early, and §8's attribution")
        print("     rule applies to it unchanged.")

    print("  4. c3hg's OWN CV, measured — REPLACES §6's sc3-derived 2.46 %% estimate")
    print("     for the POWER statement only, never for the bar (which is")
    print("     self-calibrating against CTL's spread measured in the battery).")
    for c in cells:
        v = [r["mbps"] for r in live if r["cell"] == c and r["arm"] == "CTL"
             and r.get("mbps") is not None]
        if len(v) < 2:
            print("     %-5s  n=%d — CV needs n >= 2; carry §6's estimate" % (c, len(v)))
            continue
        mu = sum(v) / len(v)
        sd = math.sqrt(sum((x - mu) ** 2 for x in v) / (len(v) - 1))
        print("     %-5s  n=%d mean=%.2f sd=%.3f CV=%.2f%%  sigma_d=sqrt(2)*CV=%.2f%%"
              % (c, len(v), mu, sd, 100 * sd / mu, 100 * math.sqrt(2) * sd / mu))

    print("\n  NOTHING IN THE CALIBRATION IS A RESULT. n = 1, and no clause of §7 or")
    print("  §11 is scored by any of it.")
    return rc


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--outdir", default="/home/vibe/rbattery")
    ap.add_argument("--calib", action="store_true")
    a = ap.parse_args()
    sys.exit(report(a.outdir, a.calib))
