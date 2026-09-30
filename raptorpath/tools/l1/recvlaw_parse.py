#!/usr/bin/env python3
"""Per-invocation parser for the receiver-law battery (Track C; the request
law is paper §7.6).

  usage: recvlaw_parse.py <cell> <arm> <seed> <rep> \
                          <cli.log> <srv.log> <cpusrv> <cpucli> \
                          <ping.txt[,ping-1.txt,...]> <q.txt>

Prints one JSON object on one line, prefixed `RECVLAWRESULT `.

Helpers: `read`, `fnum`, `inum`, `gate`, `gate_tok`, `last_with`, `field` and
the quantile rule come from `l1common.py` (one definition for every L1
parser). `latt_probe.probe_stats` owns the one definition of censoring.

This battery's independent variable, and the instruments that make it a
measured variable rather than a label:

  `arm`            CTL / A / B / AB. The two gates are orthogonal levers:
                   (A) `RWM_RECV_REQUEST_LAW` is the timing lever (the
                   receiver requests at `l >= l*_recv`, `m = 1`), (B)
                   `RWM_RANK_FEEDBACK` is the vocabulary lever (`m` derived
                   from the receiver's own `pi0`). They compose; neither
                   selects a machine.

  the `[GATES]` echo   `gate_*`, per endpoint. What was asked for.

  `[REQ]`          the receiver's own seat: how many requests it built, over
                   how many spans, at what `m` and what acting `l*`. `on=` and
                   `rank=` are the resolved arms at the seat that builds.

  `[REQS]`         the sender's serving seat: reports consumed, answers on the
                   wire split into copy (`m = 1`, today's bytes) and coded
                   (`m > 1`), and `WA1` -- the `Some`/`None` split of
                   `generate_repair_range`, the soundness precondition counted
                   rather than assumed. `on=` here is the resolved arm at the
                   seat that serves, so producer/consumer agreement is a
                   reading and not an assumption.

  `[FCAUSE]`       the seam, as a number. `gap_data` must go to 0 on the arms
                   that arm the request law and must not be 0 on the others,
                   or the contrast has no control.

  `[LATE]`         `lstar_us` (WL1), `knee_bind` (WK), `sampler_bind`, `d_us`,
                   `knee_us`, `rho_heal0`, and the gauge's own `delta`/`bar`.
                   `knee_bind` decides the KNEE-BOUND verdict.

  `[RFA]`          the primary scored dimension: the realized false fraction
                   at the receiver, with its binomial standard error, plus
                   `rep_redundant` (the false measurand under coded answers)
                   and the `dup_src`/`preempt_src` class split whose migration
                   is arm (B)'s witness.

  `[LAT]`          the second scored dimension: delivered latency decomposed,
                   `tot_p99` as the worst-leg reading, and the shares.

Every field degrades to `null` rather than raising: a missing log, an empty log
and a log with no `[GATES]` all produce a valid row. A parser that dies on a
dead invocation deletes the very rows the abort accounting is made of.
"""
import json
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:                                     # one definition of censoring, imported
    from latt_probe import PCTS, probe_stats
except Exception:                        # never let a probe import kill a row
    PCTS, probe_stats = (), None

from l1common import fnum, gate, gate_tok, inum, last_with, read, truth_columns  # noqa: E402
from l1common import field as _field  # noqa: E402
from l1common import q as _q  # noqa: E402


def q(v, p):
    return _q(v, p, 4)


def med(v):
    return q(v, 0.5)


# ── CLI, padded so a short argv can never IndexError ─────────────────────
av = sys.argv[1:] + [""] * 10
cell, arm, seed, rep, clog, slog = av[:6]
cpusrv = fnum(av[6]) if av[6] not in ("", "-", "NA") else None
cpucli = fnum(av[7]) if av[7] not in ("", "-", "NA") else None
ping_path = av[8]
q_path = av[9]
cli = read(clog)
srv = read(slog)

#: Live path count per cell -- transcribed from the battery's `cell_spec`,
#: never inferred.
CELL_PATHS = {"c1": 1, "sc2": 1, "c7": 2, "c8": 2, "c8L": 2}
n_paths = CELL_PATHS.get(cell)
#: The must-not-move cells: `pi0 -> 0` there, so `l* = 0` and `m = 1` are the
#: law's own limits and not a configuration. A control that moves voids the
#: run, so the flag rides on every row.
control_cell = 1 if n_paths == 1 else 0

# ── goodput: abort != DNF ─────────────────────────────────────────────────
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
ARM_GATES = ["RWM_RECV_REQUEST_LAW", "RWM_RANK_FEEDBACK",
             "RWM_DELTA_CAP", "RWM_SUM_CAP", "RWM_STORE_SACK_RELEASE",
             "RWM_DERIVED_SWEEP",
             "RWM_GEN"]
gates_cli = {g: gate(cli, g) for g in ARM_GATES}
gates_srv = {g: gate(srv, g) for g in ARM_GATES}
gates_cli["RWM_GEN"] = None if gate_tok(cli, "RWM_GEN") is None else inum(gate_tok(cli, "RWM_GEN"))
gates_srv["RWM_GEN"] = None if gate_tok(srv, "RWM_GEN") is None else inum(gate_tok(srv, "RWM_GEN"))


def last_line(lines, tag):
    """The reading of a cumulative gauge (`l1common.last_with`: the last line,
    a `final=1` exit flush wins). `None` when the gauge never emitted -- which
    is a reading (the emission site was unreached) and not a zero."""
    return last_with(lines, tag)


def f(line, key, cast=fnum):
    """`<key><token>` off a gauge line. `-` is the absent reading and returns
    `None` -- never 0, because 0 is a different state."""
    v = _field(line, key)
    return None if v is None else cast(v)


def fi(line, key):
    return f(line, key, inum)


# ── `[REQ]` -- what the receiver asked for ───────────────────────────────
req = last_line(srv, "[REQ] ")
req_on = fi(req, "on=")
req_rank = fi(req, "rank=")
req_sent = fi(req, "sent=")
req_spans = fi(req, "spans=")
req_m_max = fi(req, "m_max=")
req_lstar_us = fi(req, "lstar_us=")
req_holes = fi(req, "holes=")

# ── `[REQS]` -- what the sender did with them ────────────────────────────
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

#: WL2 -- the mechanism executed at both seats. A one-sided reading cannot
#: tell "never built" from "never served", and the two failures have different
#: causes and different fixes.
wl2 = None
if req_sent is not None and reqs_served is not None:
    if arm == "CTL":
        wl2 = 1 if (req_sent == 0 and reqs_served == 0) else 0
    else:
        wl2 = 1 if (req_sent > 0 and reqs_served > 0) else 0

# ── `[FCAUSE]` -- the collision seam, as a number ────────────────────────
fc = last_line(cli, "[FCAUSE] ")
fc_n = fi(fc, " n=")
fc_timer = fi(fc, "timer=")
fc_gap_data = fi(fc, "gap_data=")
fc_gap_refresh = fi(fc, "gap_refresh=")
fc_other = fi(fc, "other=")
fc_fired = fi(fc, "fired=")
#: The seam's own verdict for this row. On A/AB the per-seq gap producer is
#: suppressed at its source, so `gap_data` must be 0; on CTL/B it must not be,
#: or the treatment's zero proves nothing. `None` when `[FCAUSE]` is absent.
seam_ok = None
if fc_gap_data is not None:
    if arm in ("A", "AB"):
        seam_ok = 1 if fc_gap_data == 0 else 0
    else:
        seam_ok = 1 if (fc_gap_data > 0 or cell == "c1") else 0

# ── `[LATE]` -- the threshold, its ingredients, and its two bind gauges ──
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
#: WK -- the knee-bound gauge. If the `AND` takes the cap at essentially every
#: readout, the request lateness is not set by the lateness distribution at
#: all -- it is set by the store's free headroom, i.e. by `RWM_STORE_GAIN =
#: 2.0`, an unprovenanced constant (docs/status.md §3.4). The repair law
#: would then be the store-cap law wearing a clock.
late_knee_bind = f(late, "knee_bind=")
#: The refuter's own gauge: if the 2 ms `GAP_ACK_MIN_INTERVAL` sampler, not the
#: law, set when a hole could be reported, the arm measured the sampler.
late_sampler_bind = f(late, "sampler_bind=")
late_reports = fi(late, "reports=")

# ── `[SUCC]` -- the independent hole witness (different code, same holes) ─
succ = last_line(srv, "[SUCC] ")
succ_det = fi(succ, "det=")
succ_res = fi(succ, "res=")
succ_open = fi(succ, "open=")

# ── `[RANK]` -- the deficit vocabulary's own payload ─────────────────────
rank = last_line(srv, "[RANK] ")
rank_holes = fi(rank, "holes=")
rank_pivots = fi(rank, "pivots=")
rank_deficit = fi(rank, "deficit=")
rank_max_deficit = fi(rank, "max_deficit=")
rank_tail_over = fi(rank, "tail_overcount=")

# ── `[RFA]` -- scored dimension 1: the realized false fraction ───────────
# At the receiver, which is the only seat that can tell a late original from a
# retransmit. The sender's `[RACK] fa=` is a different (and biased) statistic
# and is carried separately, never pooled.
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
#: The binomial standard error of the scored fraction. This leg is powered at
#: n = 3 because the denominator is ~80 k holes per rep; printing the SE makes
#: that claim checkable on the row.
rfa_false_se = None
if rfa_fires and rfa_false_frac is not None and rfa_fires > 0:
    p = rfa_false_frac
    rfa_false_se = round(math.sqrt(max(p * (1.0 - p), 0.0) / rfa_fires), 6)
#: Arm (B)'s witness is a class migration, not a level: `dup_src -> 0` by
#: construction (a wiring witness), with the mass appearing as
#: `preempt_src` / `rep_redundant`. The share is what makes the migration
#: visible without pretending it is a result.
rfa_dup_share = None
if rfa_false and rfa_dup_src is not None and rfa_false > 0:
    rfa_dup_share = round(rfa_dup_src / rfa_false, 6)

# ── `[LAT]` -- scored dimension 2: delivered latency, decomposed ─────────
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

# ── `[RACK]` -- the sender-side false-alarm gauge, carried not pooled ────
rack = last_line(cli, "[RACK] ")
rack_fa = f(rack, "fa=", str)
rack_fa_frac = f(rack, "fa_frac=")

# ── `[DIAG] retx=` -- the maximum, never the last line ───────────────────
# `retx=` in the [DIAG] tail is an interval counter; the last line can read 0
# on a run whose retransmits all fired earlier.
retx_max = None
for ln in cli:
    for m in re.finditer(r"retx=(\d+)", ln):
        v = int(m.group(1))
        retx_max = v if retx_max is None else max(retx_max, v)

# ── the ping probe, through the one definition of censoring ──────────────
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
    # scored 1
    "rfa_gen": rfa_gen, "rfa_fires": rfa_fires, "rfa_false": rfa_false,
    "rfa_false_frac": rfa_false_frac, "rfa_false_se": rfa_false_se,
    "rfa_fill_coded": rfa_fill_coded, "rfa_fill_src": rfa_fill_src,
    "rfa_dup_src": rfa_dup_src, "rfa_preempt_src": rfa_preempt_src,
    "rfa_dup_share": rfa_dup_share,
    "rfa_rep_redundant": rfa_rep_redundant,
    "rfa_late_after_aban": rfa_late_after_aban,
    # scored 2
    "lat_n": lat_n, "lat_tot_p50": lat_tot_p50, "lat_tot_p95": lat_tot_p95,
    "lat_tot_p99": lat_tot_p99, "lat_sh_ax": lat_sh_ax,
    "lat_sh_rwxp": lat_sh_rwxp, "lat_sh_rwsp": lat_sh_rwsp,
    "lat_sh_rwrep": lat_sh_rwrep, "lat_sh_rep": lat_sh_rep,
    # carried, not pooled with the receiver truth
    "rack_fa": rack_fa, "rack_fa_frac": rack_fa_frac,
    "retx_max": retx_max,
    "probe": probe,
}
# Per-datagram loss truth (additive): `truth_loss_p<i>` etc. from the `[TRUTH]`
# lines of the `-q.txt` capture, one column set per live path of the cell.
row.update(truth_columns(read(q_path), n_legs=n_paths))
print("RECVLAWRESULT " + json.dumps(row, sort_keys=True))
