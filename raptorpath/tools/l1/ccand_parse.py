#!/usr/bin/env python3
"""Per-invocation parser for THE CANDIDATES BATTERY (goal-gate "Candidates
Battery — PRE-REGISTRATION" — the CONTRACT; nothing here may reinterpret it, and
no number in it may be changed once the VM has been touched).

A SEPARATE parser from `ladder_parse.py` on purpose, for the same reason that one
was separate from `ccap_parse.py` / `deadwall_parse.py`: those are the instruments
earlier verdicts were read off and they stay byte-identical. **Every column this
file shares with `ladder_parse.py` keeps its definition TO THE LINE** — goodput,
the abort rule, the wait histogram, retx, occupancy, [SF], capboot, the ping
probe, tc utilisation, the [WALL] block, [SUMCAP], [CCAP], [ACKDIAG] recon, `pl=`
and the tick-share witness — so numbers pool across sessions without a second
dialect. `capbind_check.py` is IMPORTED by the reporter rather than
re-implemented anywhere: one statistic, one definition.

What is NEW here is exactly three gauges, and they are what this battery is
scored on:

  THE `[DCAP]` GAUGE     The delta-cap's own per-run readout (paper 16.67), on
                         the SAME convention as [SUMCAP] with the counterfactual
                         keyed to the OTHER axis: at every engaged refresh it
                         recomputes what the shipped `gain` would have produced
                         from the same Sigma under the same bounds AND THE SAME
                         COUNT MULTIPLIER, so "did the derived multiplier change
                         anything" is answerable from ONE run.

                           dcap_q / dcap_b  THE DIAL-ROUTING CHECK (MEASUREMENT
                                            DISCIPLINE 1), and it outranks every
                                            other column here. The harness runs
                                            the `bulk` hint, so b(Bulk)=2 and
                                            q=(b+1)/30=0.100000 EXACTLY. These
                                            two fields are what separates "the
                                            env var was read" from "the dial
                                            reached the law".
                           dcap_eng         engaged/refreshes. `eng=0/N` at a
                                            DUAL with the gate ON is a WARM-UP
                                            FAILURE and the rep carries no datum.
                                            `eng=0/0` at a SINGLE-path cell is
                                            the CORRECT reading and NOT a
                                            failure: the pooled seat returns None
                                            on `n_live < 2` BEFORE any multiplier
                                            is read, so D is BIT-IDENTICAL to A
                                            at every single-path cell BY
                                            CONSTRUCTION. `dcap_n1_expected`
                                            carries that distinction as a column
                                            so the scorer never has to infer it.
                           dcap_chg_frac    the COUNTERFACTUAL fraction. UNLIKE
                                            [SUMCAP], `chg_frac=0` here is an
                                            INSTRUMENT FAILURE and not a null
                                            RESULT: it cannot happen while
                                            `gain != 1+q` (net/mod.rs:3938-3944).
                           dcap_pin         the fraction of engaged refreshes at
                                            the `N*knee` ceiling. HIGH pin =
                                            MEASUREMENT DISCIPLINE 18: the arm
                                            measured the CLAMP and NO verdict
                                            about the multiplier may be recorded
                                            from that cell. This is what decides
                                            c8L, whose two published anchor eras
                                            disagree about it by construction.
                           dcap_cap /       the realized mean and the mean
                           dcap_ask         UNCLAMPED value the law asked for —
                                            reported as a PAIR, which is
                                            discipline 17(b) at runtime.

  THE `[RACK]` GAUGE     The RACK-shaped recovery clock's bind fractions (paper
                         16.68) AND 16.68.1's FALSE-ALARM VALIDATION, which is
                         the reason arm A is in this battery at all.

                         KEPT PER SITE (`_cli` / `_srv`), never pooled here,
                         because the two sites are clocked on DIFFERENT
                         quantities — the sender on the app-echo RTT (RTprop +
                         standing queue) and the receiver on the wire RTT — and
                         16.68's bench predicts they diverge sharply (the SRTT
                         ceiling is reachable at the receiver at c8 and nowhere
                         at the sender). Pooling them would average a 9.9x
                         inflation against a clean wire clock.

                           rack_fa_*        16.68.1. `fa=<spurious>/<fired>`.
                                            FED ON EVERY ARM, ungated, from the
                                            SENDER site alone — the receiver's
                                            gauge never calls `record_fire`
                                            (net/receiver.rs), so a server-side
                                            fa always reads 0/0 and must never
                                            enter a denominator.
                                            **`fa=0/0` is an INSTRUMENT-FAIL for
                                            the rep, NOT `fa_frac = 0`**: no
                                            recovery round fired, so there is no
                                            false-alarm datum. `rack_fa_d_cli`
                                            carries the denominator so the
                                            scorer can enforce that.
                           rack_ceil        the SRTT ceiling's bind fraction.
                                            16.68 predicts EXACTLY 0.0000 at
                                            mult=1, which is the DEFECT FINDING
                                            under CLAUDE.md's bind-fraction rule
                                            — a bound that provably never binds
                                            turns its law into a constant.
                           rack_legacy_pin  the SHIPPED [25,100] ms clamp's own
                                            bind fraction, computed as the armed
                                            law's counterfactual. **Fed on the ON
                                            arm ONLY**, so this — the first ever
                                            measurement of the shipped clamp's
                                            bind fraction — is read off R / DR /
                                            R1 and NEVER off A.
                           rack_evals       the denominator. On arm A it is 0 BY
                                            CONSTRUCTION (`record` is guarded by
                                            `pol.rack_clocks`, `record_fire` is
                                            not), so A's ceil/gran/legacy_pin are
                                            DENOMINATORS OF ZERO and reading them
                                            as the defect finding is the error
                                            this column exists to prevent.

  THE `[LCW]` GAUGE      The one-sided-clamp witness (Tier-1 Re-Scores 2b finding
                         5), scored on NOTHING and carrying NO BAR.

                         **THE SPECIFICATION FINDING, encoded as a column.** The
                         witness is fed inside
                         `PathState::sender_truth_loss_delta`, whose only two
                         production callers (`net/control_msg.rs:345`, `:739`)
                         sit behind `loss_sent_truth_active()`. So it is
                         STRUCTURALLY SILENT on every arm carrying
                         `RWM_LOSS_SENT_TRUTH=0`, and `lcw_lines = 0` there is
                         the CORRECT reading — never a null RESULT about the
                         rectifier hypothesis. Only arm L can record it.

  THE `[SUMCAP]` GAUGE   The xN deletion's own per-run readout (paper 16.62).
                         **CARRIED, AND ITS LIVENESS RULE INVERTS FROM THE LADDER
                         BATTERY**: `RWM_SUM_CAP` is DEFAULT ON since 2026-08-19,
                         so the gauge now rides EVERY arm and its ABSENCE is an
                         INSTRUMENT-FAIL rather than an arm property. The columns
                         and their definitions are otherwise byte-identical.
                         Emitted ONLY on the ON arm (N / NT / FULL) but FED on
                         both arms at every pooled-law refresh INCLUDING the
                         counterfactual — the same expression with the
                         multiplier flipped, under the SAME bounds — so "did
                         this gate change anything" is answerable from ONE run
                         instead of by differencing two. Its whole purpose is to
                         separate three ways of saying "no difference", and they
                         are three columns here because collapsing them is the
                         confusion the gauge exists to prevent:

                           sumcap_eng    engaged/refreshes. `eng=0/N` at a DUAL
                                         with the gate ON is a WARM-UP FAILURE
                                         and the rep carries no datum.
                                         `eng=0/0` at a SINGLE-path cell is the
                                         CORRECT reading and NOT a failure:
                                         `pooled_store_cap` returns None on
                                         `n_live < 2` BEFORE the multiplier is
                                         read, so every N=1 cell is
                                         bit-identical on both arms by
                                         construction. `sumcap_n1_expected`
                                         carries that distinction as a column
                                         so the scorer never has to infer it.
                           sumcap_chg    differed/engaged + its fraction: the
                                         COUNTERFACTUAL comparison. chg=0 with a
                                         low pin is a null RESULT ("the clamp
                                         still governs"), not "the deletion does
                                         nothing".
                           sumcap_pin    the fraction of engaged refreshes whose
                                         realized cap was the N*knee ceiling.
                                         HIGH pin = MEASUREMENT DISCIPLINE 18: a
                                         defect finding about the CEILING, and
                                         NO verdict about the multiplier may be
                                         recorded from that cell. This is what
                                         the contract PRE-DECLARES at c8L
                                         (Sigma 4976 against a 2048 interiority
                                         threshold -> ask 2.43x the ceiling).
                           sumcap_floor  the same for the derived floor.
                           sumcap_cap /  the realized mean and the mean UNCLAMPED
                           sumcap_ask    value the law asked for — reported as a
                                         PAIR, which is discipline 17(b) at
                                         runtime: a clamp may never be the only
                                         thing making a law sane.

  THE eps-hat AXIS       `pl=` in the `[DIAG]` per-path block — THE LOSS
                         ESTIMATE THE RECOVERY PLANE ACTUALLY KEYS ON
                         (repair_debt, P_lost, NACK budgets), which is the
                         quantity items 3 and 5 corrected. Kept PER PATH
                         (`pl_p0`, `pl_p1`, and their max) because the c8 legs
                         carry different loss — 0.55% and 1.96% — so the T rung
                         bounds ATTRIBUTION, not just magnitude. Medians over
                         steady samples, the same steady rule every other
                         parser uses.

                         `[ACKDIAG] recon[...]` travels beside it as the
                         WITNESS: the gauge reports the legacy pair (`ce/cr`)
                         and the sender-truth pair (`cr/s`) on BOTH arms
                         regardless of the gate, which is exactly what makes the
                         estimator's move attributable rather than assumed. The
                         ledger replay that motivated the fix was computed from
                         these very fields
                         (`tools/l1/xpath_loss_replay.py`).

  THE `[CCAP]` GAUGE     Parsed for its `brake=<closed>/<ticks>` field ALONE.
                         INSTRUMENT FACT, from the contract: on a LATE_BRAKE-only
                         arm the line is emitted (net/mod.rs:4744 opens it for
                         either brake door) while the bind-fraction accumulator
                         is guarded by `composed_cap` alone (net/mod.rs:5524), so
                         FULL reads `eng=0/0 cap=0.0 mem=0.0000 floor=0.0000` BY
                         CONSTRUCTION. Those fields are still emitted as columns
                         — suppressing them would hide an instrument change — but
                         `ccap_eng` is NOT a warm-up signal on this battery's
                         arms and the reporter does not read it as one.

  THE `[WALL]` GAUGE     `deadwall_parse.py` / `ccap_parse.py`'s block, verbatim.
                         B-WALL scores `sign(dur_ms(FULL) - dur_ms(A))` PAIRED
                         WITHIN REP INDEX at c8, never a difference of medians
                         over pools, so `rep` is load-bearing on every row and is
                         carried as an int.

  THE ARM'S OWN GATES    All eight of the contract's two-sided gates on the
                         `[GATES]` line of BOTH endpoints, plus RWM_DIAG /
                         RWM_ACKDIAG / RWM_WALLDIAG as instrument liveness and
                         RWM_RECOV_MP as a witness. The prose echoes travel as
                         counts: `[SUMCAP]` presence, `unified store-cap path set
                         ACTIVE`, `[CCAP]` presence, and `three-term outstanding
                         limit ACTIVE` (expected ABSENT on EVERY arm — no ladder
                         arm reaches the three-term pool seat).

usage: ccand_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log> \
                      [cpusrv] [cpucli] [ping.txt] [q.txt]
"""
import json
import os
import re
import sys


def q(v, p):
    if not v:
        return None
    v = sorted(v)
    return round(v[min(len(v) - 1, int(round(p * (len(v) - 1))))], 4)


def med(v):
    return q(v, 0.5)


def read(path):
    if not path:
        return []
    try:
        with open(path, errors="replace") as f:
            return [re.sub(r"\x1b\[[0-9;]*m", "", ln) for ln in f]
    except OSError:
        return []


av = sys.argv[1:]
cell, arm, seed, rep, clog, slog = av[:6]
cpusrv = float(av[6]) if len(av) > 6 and av[6] not in ("", "-") else None
cpucli = float(av[7]) if len(av) > 7 and av[7] not in ("", "-") else None
ping_path = av[8] if len(av) > 8 else ""
q_path = av[9] if len(av) > 9 else ""
cli = read(clog)
srv = read(slog)

#: Live path count per cell — TRANSCRIBED from the battery driver's own
#: `cell_spec`, exactly as `capbind_check.CELL_PATHS` is, never inferred. It is
#: here for ONE reason: `[SUMCAP] eng=0/0` means "the N=1 short-circuit held"
#: at a single-path cell and "no live path was ever warm" at a dual, and a
#: fraction alone cannot tell those apart.
CELL_PATHS = {"c1": 1, "sc2": 1, "c7": 2, "c8": 2, "c8L": 2}
n_paths = CELL_PATHS.get(cell)

# ── goodput: abort != DNF (flip_parse.py's encoded rule, verbatim) ───────
runs, dnf_count, dnf = [], None, False
for ln in cli:
    i = ln.find("{")
    if i < 0:
        continue
    try:
        o = json.loads(ln[i:])
    except Exception:
        continue
    if o.get("summary"):
        dnf_count = o.get("dnf", o.get("dnf_count"))
    elif "mbps" in o:
        runs.append(o)
    elif o.get("dnf"):
        dnf = True
mbps = med([r["mbps"] for r in runs]) if runs else None
secs = med([r.get("seconds", 0) for r in runs]) if runs else None
if not runs and dnf_count is None:
    dnf = True                            # no summary at all = ABORT class


# ── liveness: [GATES] resolved values, both endpoints ────────────────────
def gate(lines, name):
    g = [l for l in lines if "[GATES]" in l]
    if not g:
        return None
    m = re.search(name + r"=([01])", g[-1])
    return int(m.group(1)) if m else None


TT_ACTIVE = "three-term outstanding limit ACTIVE"
U_ACTIVE = "unified store-cap path set ACTIVE"

#: The contract's echo-expectations table, by gate. Both endpoints, every arm.
#: The two rival laws are carried as CONTAMINATION gates and not as decoration:
#: `RWM_QUANTILE_CLOCKS` OUTRANKS `rack_clocks` and `RWM_DERIVED_SWEEP` is
#: REPLACED by it, so either one present would silently substitute the law under
#: test for a different one.
ARM_GATES = ["RWM_DELTA_CAP", "RWM_RACK_CLOCKS", "RWM_LOSS_SENT_TRUTH",
             "RWM_SUM_CAP", "RWM_QUANTILE_CLOCKS", "RWM_DERIVED_SWEEP",
             "RWM_COMPOSED_CAP", "RWM_THREE_TERM", "RWM_STORE_CAP_UNIFIED",
             "RWM_LATE_BRAKE", "RWM_CHARGE_RECOVERY", "RWM_RELEASE_1TO1"]
INSTRUMENT_GATES = ["RWM_DIAG", "RWM_ACKDIAG", "RWM_WALLDIAG"]


def gate_int(lines, name):
    """`RWM_RACK_REO_MULT` is an INTEGER over RACK's own [1, 17], not a flag —
    one table, two matchers. Reading it as `[01]` would silently drop the
    scored arms' `17` and read `None`, which is the shape of a liveness gate
    that passes because it never matched."""
    g = [l for l in lines if "[GATES]" in l]
    if not g:
        return None
    m = re.search(name + r"=(\d+)", g[-1])
    return int(m.group(1)) if m else None


gates = {}
for g in ARM_GATES + INSTRUMENT_GATES + ["RWM_RECOV_MP", "RWM_CC_PACE"]:
    short = g[4:].lower()
    gates["gates_cli_" + short] = gate(cli, g)
    gates["gates_srv_" + short] = gate(srv, g)
gates["gates_cli_rack_reo_mult"] = gate_int(cli, "RWM_RACK_REO_MULT")
gates["gates_srv_rack_reo_mult"] = gate_int(srv, "RWM_RACK_REO_MULT")
gates.update({
    "gates_lines_cli": sum(1 for l in cli if "[GATES]" in l),
    "gates_lines_srv": sum(1 for l in srv if "[GATES]" in l),
    # expected ABSENT on EVERY arm — no ladder arm reaches the three-term seat
    "active_3t_cli": sum(1 for l in cli if TT_ACTIVE in l),
    "active_3t_srv": sum(1 for l in srv if TT_ACTIVE in l),
    # expected PRESENT on FULL and ABSENT everywhere else
    "active_u_cli": sum(1 for l in cli if U_ACTIVE in l),
    "active_u_srv": sum(1 for l in srv if U_ACTIVE in l),
})

# ── `[SUMCAP]` — the xN deletion's ENGAGEMENT + COUNTERFACTUAL gauge ─────
# ONE line per sender, at teardown, and ONLY on the ON arm. Engagement is kept
# as a RATIO WITH ITS NUMERATOR AND DENOMINATOR, not only as a fraction, because
# `eng=0/0` (no pooled refresh ever happened — the N=1 short-circuit, or a cold
# path) and `eng=0/200` (200 refreshes, every one cold) are different findings.
sumcap_re = re.compile(
    r"\[SUMCAP\] on=(\d+) eng=(\d+)/(\d+) chg=(\d+)/(\d+) chg_frac=([0-9.]+) "
    r"pin=([0-9.]+) floor=([0-9.]+) cap=([0-9.]+) ask=([0-9.]+)"
)
sumcap = {"sumcap_lines": 0, "sumcap_on": None,
          "sumcap_eng_n": None, "sumcap_eng_d": None, "sumcap_eng": None,
          "sumcap_chg_n": None, "sumcap_chg_d": None, "sumcap_chg_frac": None,
          "sumcap_pin": None, "sumcap_floor": None,
          "sumcap_cap": None, "sumcap_ask": None,
          # TRUE when `eng=0/0` is the CORRECT reading for this cell's
          # geometry (N = 1), i.e. the multiplier was never reached. The
          # scorer reads this column instead of re-deriving the rule.
          "sumcap_n1_expected": None}
for ln in cli + srv:
    m = sumcap_re.search(ln)
    if not m:
        continue
    sumcap["sumcap_lines"] += 1
    en, ed = int(m.group(2)), int(m.group(3))
    cn, cd = int(m.group(4)), int(m.group(5))
    sumcap.update({
        "sumcap_on": int(m.group(1)),
        "sumcap_eng_n": en, "sumcap_eng_d": ed,
        "sumcap_eng": round(en / ed, 4) if ed else None,
        "sumcap_chg_n": cn, "sumcap_chg_d": cd,
        "sumcap_chg_frac": float(m.group(6)),
        "sumcap_pin": float(m.group(7)),
        "sumcap_floor": float(m.group(8)),
        "sumcap_cap": float(m.group(9)),
        "sumcap_ask": float(m.group(10)),
        "sumcap_n1_expected": (n_paths is not None and n_paths < 2
                               and en == 0 and ed == 0),
    })

# ── `[DCAP]` — the delta-cap's ENGAGEMENT + COUNTERFACTUAL gauge ─────────
# ONE line per sender, at teardown, and ONLY on the ON arm (D / DR). Engagement
# is kept as a RATIO WITH ITS NUMERATOR AND DENOMINATOR because `eng=0/0` (the
# pooled seat was never reached — the N=1 short-circuit) and `eng=0/200` (200
# refreshes, every one cold) are different findings. `q=` and `b=` are THE
# DIAL-ROUTING CHECK and are parsed as floats so the scorer can assert the exact
# value rather than a substring.
dcap_re = re.compile(
    r"\[DCAP\] on=(\d+) eng=(\d+)/(\d+) chg=(\d+)/(\d+) chg_frac=([0-9.]+) "
    r"pin=([0-9.]+) floor=([0-9.]+) cap=([0-9.]+) ask=([0-9.]+) "
    r"q=([0-9.]+) b=([0-9.]+)"
)
dcap = {"dcap_lines": 0, "dcap_on": None,
        "dcap_eng_n": None, "dcap_eng_d": None, "dcap_eng": None,
        "dcap_chg_n": None, "dcap_chg_d": None, "dcap_chg_frac": None,
        "dcap_pin": None, "dcap_floor": None,
        "dcap_cap": None, "dcap_ask": None,
        "dcap_q": None, "dcap_b": None,
        # TRUE when `eng=0/0` is the CORRECT reading for this cell's geometry
        # (N = 1), i.e. no multiplier was ever reached and D is bit-identical to
        # A by construction. The scorer reads this column instead of re-deriving
        # the rule.
        "dcap_n1_expected": None}
for ln in cli + srv:
    m = dcap_re.search(ln)
    if not m:
        continue
    dcap["dcap_lines"] += 1
    en, ed = int(m.group(2)), int(m.group(3))
    cn, cd = int(m.group(4)), int(m.group(5))
    dcap.update({
        "dcap_on": int(m.group(1)),
        "dcap_eng_n": en, "dcap_eng_d": ed,
        "dcap_eng": round(en / ed, 4) if ed else None,
        "dcap_chg_n": cn, "dcap_chg_d": cd,
        "dcap_chg_frac": float(m.group(6)),
        "dcap_pin": float(m.group(7)),
        "dcap_floor": float(m.group(8)),
        "dcap_cap": float(m.group(9)),
        "dcap_ask": float(m.group(10)),
        "dcap_q": float(m.group(11)),
        "dcap_b": float(m.group(12)),
        "dcap_n1_expected": (n_paths is not None and n_paths < 2
                             and en == 0 and ed == 0),
    })

# ── `[RACK]` — the recovery clock's bind fractions AND 16.68.1's fa= meter ──
# KEPT PER SITE. The sender is clocked on the app-echo RTT (RTprop + standing
# queue, a 9.9x inflation at c8) and the receiver on the wire RTT; 16.68's bench
# predicts the SRTT ceiling is reachable at the receiver at c8 and NOWHERE at the
# sender, so pooling the two sites would average away the only row that behaves
# as advertised.
#
# TWO INSTRUMENT FACTS ARE COLUMNS HERE RATHER THAN COMMENTS:
#   * `rack_evals_*` is 0 on arm A BY CONSTRUCTION (`record` is guarded by
#     `pol.rack_clocks`; `record_fire` is not), so A's ceil/gran/legacy_pin are
#     DENOMINATORS OF ZERO. The scorer must gate on `rack_evals_*` before reading
#     any fraction, and 16.68's `ceil=0.0000` defect finding may be read ONLY
#     where `rack_evals > 0`.
#   * `rack_fa_d_cli` is the fa denominator. `fa=0/0` means no recovery round
#     fired: an INSTRUMENT-FAIL for the rep, never `fa_frac = 0`.
rack_re = re.compile(
    r"\[RACK\] on=(\d+) evals=(\d+) ceil=([0-9.]+) gran=([0-9.]+) "
    r"legacy_pin=([0-9.]+) round=([0-9.]+) legacy=([0-9.]+) mult=(\d+) "
    r"fa=(\d+)/(\d+) fa_frac=([0-9.]+) fa_class=([0-9.]+)"
)
# TWO GAUGES EMIT INTO THE CLIENT LOG, and the calibration smoke is what found
# it. On a RACK-armed arm the client process carries BOTH its SENDER gauge
# (`net/mod.rs:5319`, the tail sweep — `evals` in the tens of thousands and
# `fa` non-zero) AND its own idle RECEIVER-role gauge (`net/receiver.rs:209` —
# armed, so `Drop` emits, but `evals=0 fa=0/0` because this endpoint receives no
# stalled hole). Taking "the last line that matched" would make the columns
# depend on teardown ORDER, which is exactly the silent-instrument failure this
# battery exists to avoid.
#
# SELECTION IS THEREFORE BY DATUM, PER FIELD GROUP, AND IT IS WELL-DEFINED:
#   * the CLOCK-LAW fields (evals/ceil/gran/legacy_pin/round/legacy) come from
#     the line with the MAXIMUM `evals` — a gauge at `evals=0` evaluated the law
#     zero times and carries no bind-fraction datum at all;
#   * the FALSE-ALARM fields come from the line with the MAXIMUM `fa`
#     DENOMINATOR — a gauge at `fa=0/0` fired no recovery round and carries no
#     false-alarm datum, and must never be read as `fa_frac = 0`.
# Ties and single-line cases collapse to the same answer, and `rack_lines_*`
# keeps the count so the duplication itself stays visible.
rack = {}
for site, lines in (("cli", cli), ("srv", srv)):
    hits = [m for ln in lines for m in [rack_re.search(ln)] if m]
    r = {f"rack_lines_{site}": len(hits), f"rack_on_{site}": None,
         f"rack_evals_{site}": None, f"rack_ceil_{site}": None,
         f"rack_gran_{site}": None, f"rack_legacy_pin_{site}": None,
         f"rack_round_{site}": None, f"rack_legacy_{site}": None,
         f"rack_mult_{site}": None,
         f"rack_fa_n_{site}": None, f"rack_fa_d_{site}": None,
         f"rack_fa_frac_{site}": None, f"rack_fa_class_{site}": None}
    if hits:
        c = max(hits, key=lambda m: int(m.group(2)))    # the CLOCK-LAW gauge
        f = max(hits, key=lambda m: int(m.group(10)))   # the FALSE-ALARM gauge
        r.update({
            f"rack_on_{site}": int(c.group(1)),
            f"rack_evals_{site}": int(c.group(2)),
            f"rack_ceil_{site}": float(c.group(3)),
            f"rack_gran_{site}": float(c.group(4)),
            f"rack_legacy_pin_{site}": float(c.group(5)),
            f"rack_round_{site}": float(c.group(6)),
            f"rack_legacy_{site}": float(c.group(7)),
            f"rack_mult_{site}": int(c.group(8)),
            f"rack_fa_n_{site}": int(f.group(9)),
            f"rack_fa_d_{site}": int(f.group(10)),
            f"rack_fa_frac_{site}": float(f.group(11)),
            f"rack_fa_class_{site}": float(f.group(12)),
        })
    rack.update(r)

# ── `[LCW]` — the one-sided-clamp witness (Tier-1 2b finding 5). NO BAR ──
# THE SPECIFICATION FINDING AS A COLUMN: the witness is fed inside
# `PathState::sender_truth_loss_delta`, whose only production callers sit behind
# `loss_sent_truth_active()`, so `lcw_lines = 0` on every arm carrying
# `RWM_LOSS_SENT_TRUTH=0` is the CORRECT reading and NOT a null result about the
# rectifier hypothesis. Only arm L can record it. Scored on nothing.
lcw_re = re.compile(
    r"\[LCW\] over_n=(\d+) over_mass=(\d+) loss_mass=(\d+) rect_frac=([0-9.]+)"
)
lcw = {"lcw_lines": 0, "lcw_over_n": None, "lcw_over_mass": None,
       "lcw_loss_mass": None, "lcw_rect_frac": None}
for ln in cli + srv:
    m = lcw_re.search(ln)
    if not m:
        continue
    lcw["lcw_lines"] += 1
    lcw.update({
        "lcw_over_n": int(m.group(1)),
        "lcw_over_mass": int(m.group(2)),
        "lcw_loss_mass": int(m.group(3)),
        "lcw_rect_frac": float(m.group(4)),
    })

# ── `[CCAP]` — parsed for `brake=` ALONE on this battery's arms ──────────
# See the module docstring: on a LATE_BRAKE-only arm eng/cap/mem/floor read zero
# BY CONSTRUCTION. The columns are still emitted (hiding an instrument's output
# is its own dishonesty) and the reporter scores only `ccap_brake*`.
ccap_re = re.compile(
    r"\[CCAP\] eng=(\d+)/(\d+) cap=([0-9.]+) mem=([0-9.]+) floor=([0-9.]+) "
    r"floor_val=(\d+) brake=(\d+)/(\d+) brake_frac=([0-9.]+)"
)
ccap = {"ccap_lines": 0, "ccap_eng_n": None, "ccap_eng_d": None,
        "ccap_cap": None, "ccap_mem": None, "ccap_floor": None,
        "ccap_brake_closed": None, "ccap_brake_ticks": None, "ccap_brake": None}
for ln in cli + srv:
    m = ccap_re.search(ln)
    if not m:
        continue
    ccap["ccap_lines"] += 1
    bc, bt = int(m.group(7)), int(m.group(8))
    ccap.update({
        "ccap_eng_n": int(m.group(1)), "ccap_eng_d": int(m.group(2)),
        "ccap_cap": float(m.group(3)),
        "ccap_mem": float(m.group(4)), "ccap_floor": float(m.group(5)),
        "ccap_brake_closed": bc, "ccap_brake_ticks": bt,
        "ccap_brake": round(bc / bt, 4) if bt else None,
    })

# ── `[WALL]` — the dead-wall ONSET/DURATION instrument, verbatim ─────────
wall_re = re.compile(
    r"\[WALL\] onset=([0-9.]+) dur_ms=([0-9.]+) retx=(\d+) "
    r"total_ms=([0-9.]+) it_ms=([0-9.]+)"
)
wall = {"wall_lines": 0, "wall_onset": None, "wall_dur_ms": None,
        "wall_retx": None, "wall_total_ms": None, "wall_it_ms": None}
for ln in cli + srv:
    m = wall_re.search(ln)
    if not m:
        continue
    wall["wall_lines"] += 1
    wall.update({
        "wall_onset": float(m.group(1)),
        "wall_dur_ms": float(m.group(2)),
        "wall_retx": int(m.group(3)),
        "wall_total_ms": float(m.group(4)),
        "wall_it_ms": float(m.group(5)),
    })

# ── `[ACKDIAG]` — liveness, and the eps-hat WITNESS ──────────────────────
# `recon[... ce/cr= cr/sa=]` is the LEGACY pair's own reconciliation and it is
# reported on BOTH arms whatever the gate says, because the gauge counts all
# three cursors unconditionally. That is what makes the T rung's `pl=` move
# attributable: the witness does not move, the estimator does.
recon_re = re.compile(
    r"p(\d+) win=.*?recon\[sent=(\d+) crecv=(\d+) cexp=(\d+) srcack=(\d+) "
    r"cr/s=([-0-9.]+|-) ce/cr=([-0-9.]+|-) cr/sa=([-0-9.]+|-)\]"
)
recon = {}
for ln in cli:
    if "[ACKDIAG]" not in ln:
        continue
    m = recon_re.search(ln)
    if not m:
        continue
    pid = int(m.group(1))
    for key, grp in (("crs", 6), ("cecr", 7), ("crsa", 8)):
        v = m.group(grp)
        if v != "-":
            recon.setdefault((pid, key), []).append(float(v))
ackdiag = {
    "ackdiag_lines_cli": sum(1 for l in cli if "[ACKDIAG]" in l),
    "ackdiag_lines_srv": sum(1 for l in srv if "[ACKDIAG]" in l),
}
for pid in sorted({k[0] for k in recon}):
    for key in ("crs", "cecr", "crsa"):
        ackdiag[f"recon_{key}_p{pid}"] = med(recon.get((pid, key), []))

# ── the [SF] saturation-filter gauge, cumulative: last line wins ─────────
# T-SF reads `sf_zero`: an honest ledger empties `active_paths()` more often, so
# the empty-set tick rate is PREDICTED to RISE at the duals.
sf_re = re.compile(
    r"ticks=(\d+)\s+live_sum=(\d+)\s+active_sum=(\d+)"
    r"\s+short_ticks=(\d+)\s+zero_ticks=(\d+)"
)
sf = {"sf_ticks": None, "sf_E": None, "sf_short": None, "sf_zero": None}
for ln in cli:
    if "[SF]" not in ln:
        continue
    m = sf_re.search(ln)
    if m:
        t, lv, ac, sh, ze = (int(x) for x in m.groups())
        sf = {
            "sf_ticks": t,
            "sf_E": round(ac / lv, 4) if lv else None,
            "sf_short": round(sh / t, 4) if t else None,
            "sf_zero": round(ze / t, 4) if t else None,
        }

# ── DELIVERED LATENCY probe (the G-SC2-LAT instrument) ───────────────────
png = read(ping_path)
rtts = [float(m.group(1)) for ln in png for m in [re.search(r"time=([0-9.]+) ms", ln)] if m]
p_tx = p_rx = None
for ln in png:
    m = re.search(r"(\d+) packets transmitted, (\d+) received", ln)
    if m:
        p_tx, p_rx = int(m.group(1)), int(m.group(2))
ping = {
    "ping_n": len(rtts),
    "ping_p50": q(rtts, 0.50),
    "ping_p95": q(rtts, 0.95),
    "ping_p99": q(rtts, 0.99),
    "ping_tx": p_tx,
    "ping_rx": p_rx,
    "ping_loss": (round(100.0 * (p_tx - p_rx) / p_tx, 2) if p_tx else None),
}

# ── DIAG gauges: occupancy, khr/kraw, queue, wait attribution, `pl=` ─────
occ_re = re.compile(r"win=(\d+)/(\d+)")
pq_re = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
k_re = re.compile(r"khr=([0-9.]+)/kraw=([0-9.]+|-)")
wait_re = re.compile(
    r"wait\[tun=(\d+)% paused=(\d+)% pace=(\d+)% gen=(\d+)% nack=(\d+)% "
    r"defc=(\d+)% tail=(\d+)% flush=(\d+)% n=(\d+) us=(\d+)\]")
dgq_re = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+) full=(\d+) err=(\d+) sp=(\d+)\]")
# THE T RUNG'S PRIMARY INSTRUMENT. Non-greedy from this path's own `infl=` to
# the FIRST `pl=` that follows it, so the match cannot run into the next path's
# segment. One value per path per DIAG line.
pl_re = re.compile(r"p(\d+):infl=.*?\spl=([-0-9.]+)")

occ, occap, qd, nd = [], [], [], 0
khrs, kraws, rtps = [], [], []
waits = [[] for _ in range(8)]
dgq = {}
pls = {}
retx = 0
for ln in cli:
    if "[DIAG]" not in ln:
        continue
    nd += 1
    steady = nd >= 4          # the pooling rule, unchanged across batteries
    m = occ_re.search(ln)
    if m and steady:
        occ.append(int(m.group(1)))
        occap.append(int(m.group(2)))
    if steady:
        for m in pq_re.finditer(ln):
            qd.append(max(0, int(m.group(1)) - int(m.group(3))))
            rtps.append(int(m.group(3)))
        for m in k_re.finditer(ln):
            khrs.append(float(m.group(1)))
            if m.group(2) != "-":
                kraws.append(float(m.group(2)))
        for m in pl_re.finditer(ln):
            pls.setdefault(int(m.group(1)), []).append(float(m.group(2)))
    m = wait_re.search(ln)
    if m and steady:
        for i in range(8):
            waits[i].append(int(m.group(i + 1)))
    m = re.search(r"retx=(\d+)", ln)
    if m:
        retx = max(retx, int(m.group(1)))
    for m in dgq_re.finditer(ln):     # cumulative: last wins
        dgq[int(m.group(1))] = tuple(int(m.group(i)) for i in range(2, 7))

WNAMES = ["tun", "paused", "pace", "gen", "nack", "defc", "tail", "flush"]
wait_out = {f"wait_{n}": med(waits[i]) for i, n in enumerate(WNAMES)}
wait_out["wait_lines"] = len(waits[0])

# ── THE TICK-SHARE WITNESS, scored on NOTHING ────────────────────────────
# `deadwall_parse.py`'s definition to the line, so the old and new measurands can
# be compared on identical reps. `None` (not False) when the histogram never
# populated — an invocation with no steady wait lines has no verdict to give and
# must not be counted as a non-collapse.
_wt, _wp = wait_out["wait_tun"], wait_out["wait_paused"]
wait_out["deadwall"] = (
    None if (_wt is None or _wp is None) else bool(_wt == 0 and _wp == 0)
)

# THE eps-hat COLUMNS. Per path, plus the MAX over paths — the c8 legs carry
# different loss (0.55% / 1.96%), so a single pooled number would hide the very
# attribution the fix is about.
pl_out = {"pl_n": sum(len(v) for v in pls.values()),
          "pl_max": None, "pl_min": None}
for pid, vals in sorted(pls.items()):
    pl_out[f"pl_p{pid}"] = med(vals)
_pms = [med(v) for v in pls.values() if v]
if _pms:
    pl_out["pl_max"] = max(_pms)
    pl_out["pl_min"] = min(_pms)

dgq_out = {
    "dgq_hand": sum(v[0] for v in dgq.values()) or None,
    "dgq_full": sum(v[2] for v in dgq.values()) if dgq else None,
    "dgq_gap": (sum(v[0] - v[1] for v in dgq.values()) if dgq else None),
}
# the CONSUMED-cliff gauge: steady DIAG samples with the cap at/below boot.
capboot = {
    "capboot_n": len(occap),
    "capboot_frac": (round(sum(1 for c in occap if c <= 128) / len(occap), 4)
                     if occap else None),
}

# ── UTILISATION from the shaped device (MEASUREMENT DISCIPLINE 16) ───────
# On EVERY cell and EVERY invocation, not a subset. `tc_s` is `INVOCATION_S` and
# is carried ONLY so the correction is auditable: the headroom denominator is the
# TRANSFER wall (`seconds`), never this.
QSENT = re.compile(r"Sent (\d+) bytes (\d+) pkts? \(dropped (\d+)")
tc = {"tc_bytes": None, "tc_pkts": None, "tc_drop": None, "tc_s": None}
if q_path and os.path.exists(q_path):
    cur, secs_q, seen = None, None, {}
    for ln in read(q_path):
        if ln.startswith("== "):
            if ln.startswith("== CLI0"):
                cur = "cli0"
            elif ln.startswith("== CLI1"):
                cur = "cli1"
            elif ln.startswith("== INVOCATION_S"):
                cur = None
                m = re.search(r"INVOCATION_S (\d+)", ln)
                secs_q = int(m.group(1)) if m else None
            else:
                cur = None
            continue
        m = QSENT.search(ln) if cur else None
        if m and cur not in seen:
            seen[cur] = tuple(int(x) for x in m.groups())
    if seen:
        tc = {
            "tc_bytes": sum(v[0] for v in seen.values()),
            "tc_pkts": sum(v[1] for v in seen.values()),
            "tc_drop": sum(v[2] for v in seen.values()),
            "tc_s": secs_q,
        }

out = {"cell": cell, "arm": arm, "seed": int(seed), "rep": int(rep),
       "n_paths": n_paths,
       "dnf": dnf, "dnf_count": dnf_count, "mbps": mbps, "seconds": secs,
       "n_runs": len(runs),
       "cpusrv": cpusrv, "cpucli": cpucli,
       "khr_med": med(khrs), "kraw_med": med(kraws),
       "khr_n": len(khrs), "kraw_n": len(kraws),
       "rtp_med": med(rtps),
       "occ_p50": q(occ, 0.5), "occcap_p50": q(occap, 0.5),
       "q_p50": q(qd, 0.5), "q_p99": q(qd, 0.99),
       "diag_lines": nd, "retx": retx}
out.update(gates)
out.update(dcap)
out.update(rack)
out.update(lcw)
out.update(sumcap)
out.update(ccap)
out.update(wall)
out.update(ackdiag)
out.update(sf)
out.update(capboot)
out.update(ping)
out.update(wait_out)
out.update(pl_out)
out.update(dgq_out)
out.update(tc)
print("CCANDRESULT " + json.dumps(out))
