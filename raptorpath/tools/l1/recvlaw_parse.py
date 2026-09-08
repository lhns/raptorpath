#!/usr/bin/env python3
"""Per-invocation parser for THE RECEIVER-LAW BATTERY (paper 16.83, Track C).

  usage: recvlaw_parse.py <cell> <arm> <seed> <rep> \
                          <cli.log> <srv.log> <cpusrv> <cpucli> \
                          <ping.txt[,ping-1.txt,...]> <q.txt>

Prints ONE JSON object on ONE line, prefixed `RECVLAWRESULT `, exactly the way
`alpha_parse.py` prints `ALPHARESULT ` and `ccand_parse.py` prints
`CCANDRESULT `.

HELPER PROVENANCE, stated because the rule is "reuse or copy verbatim and say
so": `alpha_parse.py` and `ccand_parse.py` are TOP-LEVEL SCRIPTS -- they read
`sys.argv` and print at import time -- so neither is importable. `q`, `med`,
`read`, `fnum`, `inum`, `gate`, `gate_tok` and the goodput block are therefore
COPIED VERBATIM from `alpha_parse.py` (which copied them from
`ccand_parse.py`), so rows POOL across sessions without a second dialect.
`latt_probe.probe_stats` IS imported, because it is a module and owns the ONE
definition of censoring.

WHAT IS NEW HERE -- this battery's own independent variable, and the
instruments that make it a MEASURED variable rather than a label:

  `arm`            CTL / A / B / AB. The two gates are ORTHOGONAL levers:
                   (A) `RWM_RECV_REQUEST_LAW` is the TIMING lever (the
                   receiver requests at `l >= l*_recv`, `m = 1`), (B)
                   `RWM_RANK_FEEDBACK` is the VOCABULARY lever (`m` derived
                   from the receiver's own `pi0`). They COMPOSE; neither
                   selects a machine.

  THE `[GATES]` ECHO   `gate_*`, per endpoint. What was ASKED FOR.

  `[REQ]`          the RECEIVER's own seat: how many requests it BUILT, over
                   how many spans, at what `m` and what acting `l*`. `on=` and
                   `rank=` are the RESOLVED arms at the seat that builds.

  `[REQS]`         the SENDER's serving seat: reports consumed, answers on the
                   wire split into COPY (`m = 1`, today's bytes) and CODED
                   (`m > 1`), and **`WA1`** -- the `Some`/`None` split of
                   `generate_repair_range`, 16.83.3's soundness precondition
                   COUNTED rather than assumed. `on=` here is the RESOLVED arm
                   at the seat that serves, so producer/consumer agreement is
                   a reading and not an assumption.

  `[FCAUSE]`       **THE SEAM, AS A NUMBER.** `gap_data` must go to 0 on the
                   arms that arm the request law and must NOT be 0 on the
                   others, or the contrast has no control. 16.83.4.

  `[LATE]`         `lstar_us` (WL1), `knee_bind` (WK), `sampler_bind`, `d_us`,
                   `knee_us`, `rho_heal0`, and the gauge's own `delta`/`bar`.
                   **`knee_bind` is what decides the KNEE-BOUND verdict**, and
                   it is a column here rather than a footnote.

  `[RFA]`          THE PRIMARY SCORED DIMENSION: the realized false fraction
                   at the RECEIVER, with its binomial standard error, plus
                   `rep_redundant` (the false measurand under coded answers)
                   and the `dup_src`/`preempt_src` class split whose MIGRATION
                   is arm (B)'s witness.

  `[LAT]`          THE SECOND SCORED DIMENSION: delivered latency decomposed,
                   `tot_p99` as the worst-leg reading, and the shares.

Every field degrades to `null` rather than raising: a missing log, an empty log
and a log with no `[GATES]` all produce a VALID row. A parser that dies on a
dead invocation deletes the very rows the abort accounting is made of.
"""
import json
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:                                     # ONE definition of censoring, imported
    from latt_probe import PCTS, probe_stats
except Exception:                        # never let a probe import kill a row
    PCTS, probe_stats = (), None


# ── helpers, COPIED VERBATIM from alpha_parse.py (see the docstring) ──────
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


def fnum(s):
    """Any numeric token -> float, or None. NOTHING in this parser may raise on
    a malformed log: the row still has to exist so the abort accounting can
    count it."""
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def inum(s):
    try:
        return int(s)
    except (TypeError, ValueError):
        return None


# ── CLI, padded so a short argv can never IndexError ─────────────────────
av = sys.argv[1:] + [""] * 10
cell, arm, seed, rep, clog, slog = av[:6]
cpusrv = fnum(av[6]) if av[6] not in ("", "-", "NA") else None
cpucli = fnum(av[7]) if av[7] not in ("", "-", "NA") else None
ping_path = av[8]
q_path = av[9]
cli = read(clog)
srv = read(slog)

#: Live path count per cell -- TRANSCRIBED from `ccand_battery.sh:202-215`'s own
#: `cell_spec`, exactly as `alpha_parse.py` transcribes it, never inferred.
CELL_PATHS = {"c1": 1, "sc2": 1, "c7": 2, "c8": 2, "c8L": 2}
n_paths = CELL_PATHS.get(cell)
#: 16.83.2's MUST-NOT-MOVE cells: `pi0 -> 0` there, so `l* = 0` and `m = 1` are
#: the LAW's own limits and not a configuration. A control that moves VOIDS the
#: run, so the flag rides on every row.
control_cell = 1 if n_paths == 1 else 0

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
    if not isinstance(o, dict):
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


def gate_tok(lines, name):
    g = [l for l in lines if "[GATES]" in l]
    if not g:
        return None
    m = re.search(name + r"=(\S+)", g[-1])
    return m.group(1) if m else None


ARM_GATES = ["RWM_RECV_REQUEST_LAW", "RWM_RANK_FEEDBACK",
             "RWM_DELTA_CAP", "RWM_SUM_CAP", "RWM_STORE_SACK_RELEASE",
             "RWM_QUANTILE_CLOCKS", "RWM_RACK_CLOCKS", "RWM_DERIVED_SWEEP",
             "RWM_GEN"]
gates_cli = {g: gate(cli, g) for g in ARM_GATES}
gates_srv = {g: gate(srv, g) for g in ARM_GATES}
gates_cli["RWM_GEN"] = None if gate_tok(cli, "RWM_GEN") is None else inum(gate_tok(cli, "RWM_GEN"))
gates_srv["RWM_GEN"] = None if gate_tok(srv, "RWM_GEN") is None else inum(gate_tok(srv, "RWM_GEN"))


def last_line(lines, tag):
    """The LAST line carrying `tag`. Cumulative gauges use the
    last-line-wins convention (`[RACK]`/`[RFA]`/`[FCAUSE]`), so this is their
    reading. `None` when the gauge never emitted -- which is a READING (the
    emission site was unreached) and not a zero."""
    hit = [l for l in lines if tag in l]
    return hit[-1] if hit else None


def f(line, key, cast=fnum):
    """`<key><token>` off a gauge line. `-` is the ABSENT reading and returns
    `None` -- never 0, because 0 is a different state (16.75.8)."""
    if not line:
        return None
    m = re.search(re.escape(key) + r"([^\s]+)", line)
    if not m or m.group(1) == "-":
        return None
    return cast(m.group(1))


def fi(line, key):
    return f(line, key, inum)


# ── `[REQ]` -- WHAT THE RECEIVER ASKED FOR ───────────────────────────────
req = last_line(srv, "[REQ] ")
req_on = fi(req, "on=")
req_rank = fi(req, "rank=")
req_sent = fi(req, "sent=")
req_spans = fi(req, "spans=")
req_m_max = fi(req, "m_max=")
req_lstar_us = fi(req, "lstar_us=")
req_holes = fi(req, "holes=")

# ── `[REQS]` -- WHAT THE SENDER DID WITH THEM ────────────────────────────
reqs = last_line(cli, "[REQS] ")
reqs_on = fi(reqs, "on=")
reqs_reports = fi(reqs, "reports=")
reqs_spans = fi(reqs, "spans=")
reqs_m_max = fi(reqs, "m_max=")
reqs_served = fi(reqs, "served=")
reqs_copy = fi(reqs, "copy=")
reqs_coded = fi(reqs, "coded=")
wa1_some = fi(reqs, "wa1_some=")
wa1_none = fi(reqs, "wa1_none=")
wa1_none_frac = f(reqs, "wa1_none_frac=")
reqs_stale = fi(reqs, "stale=")
reqs_budget_bound = fi(reqs, "budget_bound=")
reqs_open_wants = fi(reqs, "open_wants=")
reqs_cause = f(reqs, "cause=", str)

#: **WL2 -- THE MECHANISM EXECUTED AT BOTH SEATS.** A one-sided reading cannot
#: tell "never built" from "never served", and the two failures have different
#: causes and different fixes.
wl2 = None
if req_sent is not None and reqs_served is not None:
    if arm == "CTL":
        wl2 = 1 if (req_sent == 0 and reqs_served == 0) else 0
    else:
        wl2 = 1 if (req_sent > 0 and reqs_served > 0) else 0

# ── `[FCAUSE]` -- THE COLLISION SEAM, AS A NUMBER (16.83.4) ──────────────
fc = last_line(cli, "[FCAUSE] ")
fc_n = fi(fc, " n=")
fc_timer = fi(fc, "timer=")
fc_gap_data = fi(fc, "gap_data=")
fc_gap_refresh = fi(fc, "gap_refresh=")
fc_other = fi(fc, "other=")
fc_fired = fi(fc, "fired=")
#: The seam's own verdict for THIS row. On A/AB the per-seq gap producer is
#: suppressed at its source, so `gap_data` must be 0; on CTL/B it must not be,
#: or the treatment's zero proves nothing. `None` when `[FCAUSE]` is absent.
seam_ok = None
if fc_gap_data is not None:
    if arm in ("A", "AB"):
        seam_ok = 1 if fc_gap_data == 0 else 0
    else:
        seam_ok = 1 if (fc_gap_data > 0 or cell == "c1") else 0

# ── `[LATE]` -- THE THRESHOLD, ITS INGREDIENTS, AND ITS TWO BIND GAUGES ──
late = last_line(srv, "[LATE] ")
late_n = fi(late, "n=")
late_orig = fi(late, "orig=")
late_rep = fi(late, "rep=")
late_aban = fi(late, "aban=")
late_xp_frac = f(late, "xp_frac=")
late_d_us = fi(late, "d_us=")
late_knee_us = fi(late, "knee_us=")
late_rho0 = f(late, "rho_heal0=")
late_s_tot = f(late, "s_tot=")
late_lstar_us = fi(late, "lstar_us=")
late_delta = f(late, "delta=")
late_bar = f(late, "bar=")
#: **WK -- THE KNEE-BOUND GAUGE.** 16.83.2: if the `AND` takes the cap at
#: essentially every readout, the request lateness is not set by the lateness
#: distribution at all -- it is set by the store's free headroom, i.e. by
#: `RWM_STORE_GAIN = 2.0`, which is UNPROVENANCED. THE REPAIR LAW WOULD THEN
#: BE THE STORE-CAP LAW WEARING A CLOCK, and the verdict is a LEDGER verdict.
late_knee_bind = f(late, "knee_bind=")
#: The refuter's own gauge: if the 2 ms `GAP_ACK_MIN_INTERVAL` sampler, not the
#: law, set when a hole could be reported, the arm measured the sampler.
late_sampler_bind = f(late, "sampler_bind=")
late_reports = fi(late, "reports=")

# ── `[SUCC]` -- THE INDEPENDENT HOLE WITNESS (different code, same holes) ─
succ = last_line(srv, "[SUCC] ")
succ_det = fi(succ, "det=")
succ_res = fi(succ, "res=")
succ_open = fi(succ, "open=")

# ── `[RANK]` -- THE DEFICIT VOCABULARY'S OWN PAYLOAD ─────────────────────
rank = last_line(srv, "[RANK] ")
rank_holes = fi(rank, "holes=")
rank_pivots = fi(rank, "pivots=")
rank_deficit = fi(rank, "deficit=")
rank_max_deficit = fi(rank, "max_deficit=")
rank_tail_over = fi(rank, "tail_overcount=")

# ── `[RFA]` -- SCORED DIMENSION 1: THE REALIZED FALSE FRACTION ───────────
# At the RECEIVER, which is the only seat that can tell a late original from a
# retransmit -- the whole of D0's finding. The sender's `[RACK] fa=` is a
# DIFFERENT (and biased) statistic and is carried separately, never pooled.
rfa = last_line(srv, "[RFA] ")
rfa_gen = fi(rfa, "gen=")
rfa_fires = fi(rfa, "fires=")
rfa_false = fi(rfa, "false=")
rfa_false_frac = f(rfa, "false_frac=")
rfa_fill_coded = fi(rfa, "fill_coded=")
rfa_fill_src = fi(rfa, "fill_src=")
rfa_dup_src = fi(rfa, "dup_src=")
rfa_preempt_src = fi(rfa, "preempt_src=")
rfa_rep_redundant = fi(rfa, "rep_redundant=")
rfa_late_after_aban = fi(rfa, "late_after_aban=")
#: THE BINOMIAL STANDARD ERROR of the scored fraction. The pre-registration
#: says this leg is POWERED at n = 3 precisely because the denominator is
#: ~80 k holes per rep; printing the SE is what makes that claim checkable on
#: the row rather than in the prose.
rfa_false_se = None
if rfa_fires and rfa_false_frac is not None and rfa_fires > 0:
    p = rfa_false_frac
    rfa_false_se = round(math.sqrt(max(p * (1.0 - p), 0.0) / rfa_fires), 6)
#: Arm (B)'s witness is a CLASS MIGRATION, not a level: `dup_src -> 0` BY
#: CONSTRUCTION (a wiring witness), with the mass appearing as
#: `preempt_src` / `rep_redundant`. The share is what makes the migration
#: visible without pretending it is a result.
rfa_dup_share = None
if rfa_false and rfa_dup_src is not None and rfa_false > 0:
    rfa_dup_share = round(rfa_dup_src / rfa_false, 6)

# ── `[LAT]` -- SCORED DIMENSION 2: DELIVERED LATENCY, DECOMPOSED ─────────
lat = last_line(srv, "[LAT] ")
lat_n = fi(lat, "n=")
lat_tot_p50 = fi(lat, "tot_p50=")
lat_tot_p95 = fi(lat, "tot_p95=")
lat_tot_p99 = fi(lat, "tot_p99=")
lat_sh_ax = f(lat, "sh_ax=")
lat_sh_rwxp = f(lat, "sh_rwxp=")
lat_sh_rwsp = f(lat, "sh_rwsp=")
lat_sh_rwrep = f(lat, "sh_rwrep=")
lat_sh_rep = f(lat, "sh_rep=")

# ── `[RACK]` -- the SENDER-side false-alarm gauge, carried NOT pooled ────
rack = last_line(cli, "[RACK] ")
rack_fa = f(rack, "fa=", str)
rack_fa_frac = f(rack, "fa_frac=")

# ── `[DIAG] retx=` -- THE MAXIMUM, never the last line ───────────────────
# `retx=` in the [DIAG] tail is an INTERVAL counter; reading it off the last
# line made the plain-window primitives pass report this witness failing at
# 5 of 15 reps whose [RACK] fired on the same run was 11-5717 (alpha_parse.py's
# own W4' note).
retx_max = None
for ln in cli:
    for m in re.finditer(r"retx=(\d+)", ln):
        v = int(m.group(1))
        retx_max = v if retx_max is None else max(retx_max, v)

# ── the ping probe, through the ONE definition of censoring ──────────────
probe = {}
if probe_stats and ping_path:
    for path in [p for p in ping_path.split(",") if p]:
        try:
            s = probe_stats(path)
        except Exception:
            s = None
        if s:
            probe[os.path.basename(path)] = s

row = {
    "cell": cell, "arm": arm, "seed": inum(seed), "rep": inum(rep),
    "n_paths": n_paths, "control_cell": control_cell,
    "mbps": mbps, "secs": secs, "dnf": dnf, "dnf_count": dnf_count,
    "cpusrv": cpusrv, "cpucli": cpucli,
    "gates_cli": gates_cli, "gates_srv": gates_srv,
    # the arm, at the two seats that consume it
    "req_on": req_on, "req_rank": req_rank, "reqs_on": reqs_on,
    # [REQ] -- the receiver's seat
    "req_sent": req_sent, "req_spans": req_spans, "req_m_max": req_m_max,
    "req_lstar_us": req_lstar_us, "req_holes": req_holes,
    # [REQS] -- the sender's seat, and WA1
    "reqs_reports": reqs_reports, "reqs_spans": reqs_spans,
    "reqs_m_max": reqs_m_max, "reqs_served": reqs_served,
    "reqs_copy": reqs_copy, "reqs_coded": reqs_coded,
    "wa1_some": wa1_some, "wa1_none": wa1_none,
    "wa1_none_frac": wa1_none_frac, "reqs_stale": reqs_stale,
    "reqs_budget_bound": reqs_budget_bound, "reqs_open_wants": reqs_open_wants,
    "reqs_cause": reqs_cause, "WL2": wl2,
    # the seam
    "fc_n": fc_n, "fc_timer": fc_timer, "fc_gap_data": fc_gap_data,
    "fc_gap_refresh": fc_gap_refresh, "fc_other": fc_other,
    "fc_fired": fc_fired, "seam_ok": seam_ok,
    # [LATE]
    "late_n": late_n, "late_orig": late_orig, "late_rep": late_rep,
    "late_aban": late_aban, "late_xp_frac": late_xp_frac,
    "late_d_us": late_d_us, "late_knee_us": late_knee_us,
    "late_rho_heal0": late_rho0, "late_s_tot": late_s_tot,
    "late_lstar_us": late_lstar_us, "late_delta": late_delta,
    "late_bar": late_bar, "WK_knee_bind": late_knee_bind,
    "late_sampler_bind": late_sampler_bind, "late_reports": late_reports,
    # the independent hole witness
    "succ_det": succ_det, "succ_res": succ_res, "succ_open": succ_open,
    # [RANK]
    "rank_holes": rank_holes, "rank_pivots": rank_pivots,
    "rank_deficit": rank_deficit, "rank_max_deficit": rank_max_deficit,
    "rank_tail_overcount": rank_tail_over,
    # SCORED 1
    "rfa_gen": rfa_gen, "rfa_fires": rfa_fires, "rfa_false": rfa_false,
    "rfa_false_frac": rfa_false_frac, "rfa_false_se": rfa_false_se,
    "rfa_fill_coded": rfa_fill_coded, "rfa_fill_src": rfa_fill_src,
    "rfa_dup_src": rfa_dup_src, "rfa_preempt_src": rfa_preempt_src,
    "rfa_dup_share": rfa_dup_share,
    "rfa_rep_redundant": rfa_rep_redundant,
    "rfa_late_after_aban": rfa_late_after_aban,
    # SCORED 2
    "lat_n": lat_n, "lat_tot_p50": lat_tot_p50, "lat_tot_p95": lat_tot_p95,
    "lat_tot_p99": lat_tot_p99, "lat_sh_ax": lat_sh_ax,
    "lat_sh_rwxp": lat_sh_rwxp, "lat_sh_rwsp": lat_sh_rwsp,
    "lat_sh_rwrep": lat_sh_rwrep, "lat_sh_rep": lat_sh_rep,
    # carried, NOT pooled with the receiver truth
    "rack_fa": rack_fa, "rack_fa_frac": rack_fa_frac,
    "retx_max": retx_max,
    "probe": probe,
}
print("RECVLAWRESULT " + json.dumps(row, sort_keys=True))
