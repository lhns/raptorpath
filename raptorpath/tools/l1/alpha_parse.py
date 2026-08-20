#!/usr/bin/env python3
"""Per-invocation parser for THE ALPHA-SWEEP (goal #100 item 2).

  usage: alpha_parse.py <cell> <arm> <alpha|NA> <seed> <rep> \
                        <cli.log> <srv.log> <cpusrv> <cpucli> \
                        <ping.txt[,ping-1.txt,...]> <q.txt>

Prints ONE JSON object on ONE line, prefixed `ALPHARESULT `, exactly the way
`ccand_parse.py` prints `CCANDRESULT `.

WHY A SEPARATE PARSER, AND WHAT IT PROMISES NOT TO CHANGE. `ccand_parse.py` is
the instrument the candidates verdict was read off and it stays byte-identical.
This file is a SIBLING, not a fork: **every column it shares with
`ccand_parse.py` keeps that file's definition TO THE LINE** — the abort rule,
goodput, the `[GATES]` scrape, the wait histogram, `retx`, occupancy, `[SF]`,
capboot, the ping probe, tc utilisation, `[RACK]`, `dgq_*`, `pl=` — so rows
POOL across sessions without a second dialect.

  HELPER PROVENANCE, stated because the rule is "reuse or copy verbatim and say
  so": `ccand_parse.py` is a TOP-LEVEL SCRIPT — it reads `sys.argv` and prints
  at import time — so it is NOT importable. `q`, `med`, `read`, `gate`,
  `gate_int` and every gauge block below are therefore COPIED VERBATIM from it
  (`latt_probe.probe_stats` IS imported, because it is a module and owns the
  ONE definition of censoring).

WHAT IS NEW HERE — the sweep's own independent variable, and the instruments
that make it a MEASURED variable rather than a label:

  `alpha_cmd`      the arm's COMMANDED α, as the driver passed it. `null` on
                   CTL, which commands no α at all and runs the shipped
                   `(2·srtt).clamp(25, 100) ms`.

  THE `[GATES]` ECHO   `gate_quantile_*` / `gate_alpha_*`, per endpoint, from
                   the LAST `[GATES]` line. This says what was ASKED FOR.

  THE `[QALPHA]` GAUGE   `qalpha_*`, per site, emitted ONCE per site on EVERY
                   arm including the control. This says what the law is
                   EVALUATING, together with its `k(α) = √((1−α)/α)`. The two
                   together are MEASUREMENT DISCIPLINE 1 for this battery: an
                   env var that was read is not a dial that reached the law.

  THE `[QCLK]` GAUGE     `qclk_cli_*` / `qclk_srv_*` — **the REALIZED recovery
                   clock as a DISTRIBUTION**, and it is the reason the battery
                   can be scored at all. `W(α) = srtt + k(α)·σ` is commanded by
                   α and realized through σ, and σ is not a constant (the
                   plain-window pass measured σ(c8) at 0.191 / 3.140 / 54.836 ms
                   across three reps at n ≈ 18 000). Two arms commanded at
                   different α can realize OVERLAPPING W and are then not two
                   arms. The p05/p50/p95 columns are what the reporter's
                   SEPARATION RULE is computed from.

                   SITE SELECTION, per log, by MAXIMUM `evals` — the same rule
                   `ccand_parse.py` applies to `[RACK]`, and for the same
                   reason: a process can carry both a sender-role and an idle
                   receiver-role gauge, and "the last line that matched" would
                   make the columns depend on TEARDOWN ORDER. The counters are
                   cumulative, so max-evals and last-line agree wherever there
                   is only one gauge.

  THE `[RFA]` GAUGE      the RECEIVER-site realized false-repair classes, and
                   THE BRACKET (`rfa_bracket_lo` / `rfa_bracket_hi`) that is
                   the honest way to state them — see the block comment there.

  THE WITNESSES    `w1_rfa_gen`, `w2_pfrac_lines`, `w4_retx_max`, `w5_rack_fa`,
                   transcribed from `prim_battery_pw.sh`'s witness block. W3
                   (`cod=`) IS RETIRED and is not computed here.

Every field degrades to `null` rather than raising: a missing log, an empty log
and a log with no `[GATES]` all produce a VALID row. A parser that dies on a
dead invocation deletes the very rows the abort accounting is made of.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:                                     # ONE definition of censoring, imported
    from latt_probe import PCTS, probe_stats
except Exception:                        # never let a probe import kill a row
    PCTS, probe_stats = (), None


# ── helpers, COPIED VERBATIM from ccand_parse.py (see the docstring) ──────
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
av = sys.argv[1:] + [""] * 11
cell, arm, alpha_arg, seed, rep, clog, slog = av[:7]
cpusrv = fnum(av[7]) if av[7] not in ("", "-", "NA") else None
cpucli = fnum(av[8]) if av[8] not in ("", "-", "NA") else None
ping_path = av[9]
q_path = av[10]
cli = read(clog)
srv = read(slog)

#: The arm's COMMANDED α. `unset` is what `alpha_battery.sh`'s own `arm_alpha`
#: prints for the control, and `NA` / `null` / `-` / empty mean the same thing:
#: the CTL arm commands NO α (`RWM_QUANTILE_CLOCKS=0`, `RWM_ALPHA_OVERRIDE`
#: ABSENT) and runs the shipped `(2·srtt).clamp(25, 100) ms` clamp.
alpha_cmd = (None if alpha_arg in ("", "-", "NA", "na", "unset", "null", "none")
             else fnum(alpha_arg))

#: Live path count per cell — TRANSCRIBED from `ccand_battery.sh:202-215`'s own
#: `cell_spec`, exactly as `capbind_check.CELL_PATHS` is, never inferred.
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


def gate_int(lines, name):
    g = [l for l in lines if "[GATES]" in l]
    if not g:
        return None
    m = re.search(name + r"=(\d+)", g[-1])
    return int(m.group(1)) if m else None


def gate_tok(lines, name):
    """The RAW token after `<name>=`, as printed. `RWM_ALPHA_OVERRIDE` prints
    `unset` OR a number (`gates.rs:1348`, `:1365`), and reading it as `[01]` or
    as a float would silently turn BOTH of those into `None` — a liveness gate
    that passes because it never matched."""
    g = [l for l in lines if "[GATES]" in l]
    if not g:
        return None
    m = re.search(name + r"=(\S+)", g[-1])
    return m.group(1) if m else None


ARM_GATES = ["RWM_QUANTILE_CLOCKS", "RWM_RACK_CLOCKS", "RWM_DERIVED_SWEEP",
             "RWM_DELTA_CAP", "RWM_SUM_CAP", "RWM_COMPOSED_CAP",
             "RWM_THREE_TERM", "RWM_LOSS_SENT_TRUTH", "RWM_GEN"]
INSTRUMENT_GATES = ["RWM_DIAG", "RWM_ACKDIAG", "RWM_WALLDIAG", "RWM_FDIAG"]

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
})

# THE SWEEP'S OWN GATE COLUMNS, as the SPEC names them. `RWM_GEN` is an integer
# generation size and NOT a flag, so it is read with `gate_int` too and is not
# scored here; it rides as instrument context.
gates["gate_quantile_cli"] = gate_tok(cli, "RWM_QUANTILE_CLOCKS")
gates["gate_quantile_srv"] = gate_tok(srv, "RWM_QUANTILE_CLOCKS")
gates["gate_alpha_cli"] = gate_tok(cli, "RWM_ALPHA_OVERRIDE")
gates["gate_alpha_srv"] = gate_tok(srv, "RWM_ALPHA_OVERRIDE")
gates["gate_gen_cli"] = gate_int(cli, "RWM_GEN")
gates["gate_gen_srv"] = gate_int(srv, "RWM_GEN")

# ── `[QALPHA]` — THE RESOLVED α AT THE SITE THAT EVALUATES IT ────────────
# `net/mod.rs:805` (`qalpha_report_line`). ONE line per site, on EVERY arm
# INCLUDING the control (`quantile=0`), so "quantile clocks off" is as
# checkable as "quantile clocks on" (MEASUREMENT DISCIPLINE 15).
#
#   [QALPHA] site=<sender|receiver> quantile=<0|1> contract_alpha=<sci>
#            override=<sci|unset> alpha=<sci> k=<f4>
#
# `[GATES] RWM_ALPHA_OVERRIDE=` says what was ASKED FOR; THIS says what the law
# is EVALUATING. The reporter's W6 arm-liveness witness needs BOTH, because an
# arm whose own independent variable did not take is VOID and not a datum.
#: TOKENISED, NOT POSITIONAL, and that is deliberate. A gauge that grows a
#: field is the NORMAL case in this tree — `[QCLK]` is one commit old — and a
#: rigid whole-line regex answers a new field by returning `None` for EVERY
#: column, which is the silent-instrument failure the batteries exist to avoid.
#: A `k=v` scan degrades ONE column at a time and never the row.
TOKEN = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)=([^\s]+)")


def toks(line):
    """`k=v` pairs after the tag. `sigma_us_mean=1234.5/n900` keeps its raw
    value; the caller splits it, because only the caller knows the shape."""
    return dict(TOKEN.findall(line))


qalpha = {}
for site, lines in (("cli", cli), ("srv", srv)):
    hits = [ln for ln in lines if "[QALPHA]" in ln]
    r = {f"qalpha_lines_{site}": len(hits), f"qalpha_site_{site}": None,
         f"qalpha_quantile_{site}": None, f"qalpha_contract_{site}": None,
         f"qalpha_override_{site}": None, f"qalpha_{site}": None,
         f"qalpha_k_{site}": None}
    if hits:
        t = toks(hits[-1])                 # once per site: last is the reading
        ov = t.get("override")
        r.update({
            f"qalpha_site_{site}": t.get("site"),
            f"qalpha_quantile_{site}": inum(t.get("quantile")),
            f"qalpha_contract_{site}": fnum(t.get("contract_alpha")),
            f"qalpha_override_{site}": (None if ov in (None, "unset")
                                        else fnum(ov)),
            f"qalpha_{site}": fnum(t.get("alpha")),
            f"qalpha_k_{site}": fnum(t.get("k")),
        })
    qalpha.update(r)

# ── `[QCLK]` — THE REALIZED RECOVERY CLOCK, AS A DISTRIBUTION ────────────
# `net/mod.rs:955` (`QuantileClockGauge::line`). Sender emits on Drop; the
# receiver emits on a 1 s cadence AND on Drop. The counters are CUMULATIVE.
#
#   [QCLK] site=<sender|receiver> on=<0|1> alpha=<sci> k=<f4> evals=<n>
#          kept=<n> w_us_mean=<f1> w_us_p05=<n> w_us_p50=<n> w_us_p95=<n>
#          w_us_min=<n> w_us_max=<n> srtt_us_mean=<f1> sigma_us_mean=<f1>/n<n>
#
# SELECTION IS BY MAXIMUM `evals` WITHIN THE LOG, not by "the last line that
# matched": a process can carry a sender-role gauge AND an idle receiver-role
# gauge, and last-line selection would make these columns depend on TEARDOWN
# ORDER. For a cumulative counter with one live gauge the two rules agree.
#
# **`evals = 0` is an UNREACHED EVALUATION SITE, not `W = 0`.** The gauge stays
# silent on Drop at zero evals for exactly that reason, so an ABSENT [QCLK] can
# only be read as "the recovery clock was never evaluated" — never as a value.
QCLK_INT = ["on", "evals", "kept", "w_us_p05", "w_us_p50", "w_us_p95",
            "w_us_min", "w_us_max", "law_n"]
QCLK_FLOAT = ["alpha", "k", "w_us_mean", "srtt_us_mean"]
QCLK_FIELDS = (["site"] + QCLK_FLOAT + QCLK_INT + ["sigma_us_mean", "sigma_n"])
#: `law_n` is READ IF PRESENT and is `null` otherwise. `alpha_battery.sh:242`
#: reads a `[QCLK] law_n=` bind counter off the sender to decide its
#: REALIZED-CLOCK REACHABILITY GATE; the gauge in `net/mod.rs` does not print
#: one at this era. The column is carried so that when it appears the ledger
#: records it, and so that its ABSENCE is a visible null rather than a parser
#: that silently returned nothing for the whole line.
qclk = {}
for site, lines in (("cli", cli), ("srv", srv)):
    hits = [toks(ln) for ln in lines if "[QCLK]" in ln]
    r = {f"qclk_{site}_lines": len(hits)}
    for f in QCLK_FIELDS:
        r[f"qclk_{site}_{f}"] = None
    if hits:
        # BY MAXIMUM `evals`, not by last line — see the block comment.
        t = max(hits, key=lambda x: (inum(x.get("evals")) or 0))
        r[f"qclk_{site}_site"] = t.get("site")
        for f in QCLK_FLOAT:
            r[f"qclk_{site}_{f}"] = fnum(t.get(f))
        for f in QCLK_INT:
            r[f"qclk_{site}_{f}"] = inum(t.get(f))
        sg = (t.get("sigma_us_mean") or "").split("/n")
        r[f"qclk_{site}_sigma_us_mean"] = fnum(sg[0]) if sg[0] else None
        r[f"qclk_{site}_sigma_n"] = inum(sg[1]) if len(sg) > 1 else None
    qclk.update(r)

# ── `[RACK]` — the recovery clock's bind fractions AND §16.68.1's fa= meter ──
# ccand_parse.py's block and its selection rule, VERBATIM (per site, clock-law
# fields off max `evals`, false-alarm fields off max fa DENOMINATOR), so the
# rack columns pool with the candidates ledger.
rack_re = re.compile(
    r"\[RACK\] on=(\d+) evals=(\d+) ceil=([0-9.]+) gran=([0-9.]+) "
    r"legacy_pin=([0-9.]+) round=([0-9.]+) legacy=([0-9.]+) mult=(\d+) "
    r"fa=(\d+)/(\d+) fa_frac=([0-9.]+) fa_class=([0-9.]+)"
)
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

# THE SPEC'S OWN NAMES for the SENDER `[RACK]` line, as ALIASES over the
# ccand columns above — one parse, two vocabularies, no second definition.
# `fa = 0/0` is an INSTRUMENT-FAIL for the rep and NEVER `fa_frac = 0`:
# `rack_fired` carries the denominator so the scorer can enforce that.
rack_alias = {
    "rack_fired": rack.get("rack_fa_d_cli"),
    "rack_spurious": rack.get("rack_fa_n_cli"),
    "rack_fa_frac": rack.get("rack_fa_frac_cli"),
    "rack_fa_class": rack.get("rack_fa_class_cli"),
    "rack_round_us": rack.get("rack_round_cli"),
    "rack_legacy_us": rack.get("rack_legacy_cli"),
    "rack_evals": rack.get("rack_evals_cli"),
    "rack_legacy_pin": rack.get("rack_legacy_pin_cli"),
}

# ── `[RFA]` — THE RECEIVER-SITE REALIZED FALSE-REPAIR CLASSES ────────────
# `net/mod.rs:4652` (`rfa_report_line`). CUMULATIVE counters: the LAST line of
# the log is the reading, the same convention `[WIDLE]` and `[FDIAG]` use. A
# sender-role gauge sees no source arrivals and never emits, so this is read
# off the RECEIVER (server) log ALONE.
#
#   [RFA] gen=<0|1> fires=<n> false=<n> false_frac=<f4> fill_coded=<n>
#         fill_src=<n> dup_src=<n> preempt_src=<n> src_n=<n> rep_n=<n>
#         nu_recv=<f5> fa_class=<f4>
#
# THE BRACKET, AND WHY THE PRINTED FRACTION IS ONLY ONE END OF IT.
# `fires = fill_coded + fill_src + dup_src + preempt_src` and
# `false = dup_src + preempt_src`, so the printed `false_frac` counts a
# **REORDERED ORIGINAL** (`fill_src` — the hole was filled by the original
# datagram arriving late, not by any repair) as a SUCCESSFUL REPAIR. That
# inflates the denominator with arrivals no repair caused, so:
#
#   rfa_bracket_lo = false_frac as printed          — a LOWER bound
#   rfa_bracket_hi = false / (false + fill_coded)   — the CEILING, which removes
#                    those reordered originals from the denominator entirely
#
# The realized false-repair fraction lies in [lo, hi]. Reporting either end
# alone states a bound as if it were a measurement.
RFA_INT = ["gen", "fires", "false", "fill_coded", "fill_src", "dup_src",
           "preempt_src", "src_n", "rep_n"]
RFA_FLOAT = ["false_frac", "nu_recv", "fa_class"]
rfa = {"rfa_lines": sum(1 for l in srv if "[RFA]" in l)}
for f in RFA_INT + RFA_FLOAT:
    rfa["rfa_" + f] = None
rfa["rfa_bracket_lo"] = None
rfa["rfa_bracket_hi"] = None
_rfa_hits = [toks(ln) for ln in srv if "[RFA]" in ln]
if _rfa_hits:
    t = _rfa_hits[-1]                       # cumulative: last line wins
    for f in RFA_INT:
        rfa["rfa_" + f] = inum(t.get(f))
    for f in RFA_FLOAT:
        rfa["rfa_" + f] = fnum(t.get(f))
    _false, _fc = rfa["rfa_false"], rfa["rfa_fill_coded"]
    rfa["rfa_bracket_lo"] = rfa["rfa_false_frac"]
    _den = (_false + _fc) if (_false is not None and _fc is not None) else None
    rfa["rfa_bracket_hi"] = (round(_false / _den, 6) if _den else None)

# ── THE WITNESSES, transcribed from `prim_battery_pw.sh:100-112` ─────────
# W3 (`cod=0sym/s`) IS RETIRED and is deliberately NOT computed here.
w1_m = [l for l in srv if "[RFA] gen=" in l]
w1_rfa_gen = None
if w1_m:
    m = re.search(r"\[RFA\] gen=([01])", w1_m[-1])
    w1_rfa_gen = m.group(1) if m else None
w2_pfrac_lines = sum(1 for l in cli if "[PFRAC]" in l)
w5_rack_fa = None
for l in cli:
    m = re.search(r"\[RACK\].*?fa=(\d+/\d+)", l)
    if m:
        w5_rack_fa = m.group(1)             # last wins, as the shell does

# ── DELIVERED LATENCY probe (the G-SC2-LAT / latency-truth instrument) ───
# `ping_path` accepts ONE path or a COMMA-SEPARATED leg list, the shape
# `era_parse.py` takes. The pooled `ping_*` columns keep `ccand_parse.py`'s
# definition over the FIRST leg so they pool with that ledger; the per-leg
# `legN_*` columns carry the CENSORING accounting `latt_probe.py` owns, and a
# percentile is never reported here without it.
leg_paths = [p for p in ping_path.split(",") if p]
png = read(leg_paths[0] if leg_paths else "")
rtts = [float(m.group(1)) for ln in png
        for m in [re.search(r"time=([0-9.]+) ms", ln)] if m]
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
legs = {"legs_probed": len(leg_paths)}
if probe_stats is not None:
    for _i, _p in enumerate(leg_paths):
        try:
            _s = probe_stats(_p, leg=_i)
        except Exception:
            continue
        for _k in ("n", "sent", "recv", "sent_source", "censor_frac",
                   "censor_pct", "recv_mismatch", "leg_unscoreable",
                   "min", "max"):
            legs["leg%d_%s" % (_i, _k)] = _s.get(_k)
        for _name, _ in PCTS:
            legs["leg%d_%s" % (_i, _name)] = _s.get(_name)
            legs["leg%d_%s_censored" % (_i, _name)] = _s.get(_name + "_censored")
            legs["leg%d_%s_scoreable" % (_i, _name)] = _s.get(_name + "_scoreable")
_cf = [legs.get("leg%d_censor_frac" % i) for i in range(len(leg_paths))]
_cf = [x for x in _cf if x is not None]
legs["legs_censor_max"] = (max(_cf) if _cf else None)
# THE CENSORING FRACTION THE REPORT PRINTS BESIDE EVERY PERCENTILE. Falls back
# to the pooled probe's own loss when no leg file was passed, so the column is
# never silently absent — a percentile whose error bar points in a KNOWN
# direction and is not written down is the defect `latt_probe.py` exists to close.
ping["ping_censor_frac"] = (
    legs["legs_censor_max"] if legs["legs_censor_max"] is not None
    else (round((p_tx - p_rx) / p_tx, 4) if p_tx else None)
)

# ── DIAG gauges: occupancy, khr/kraw, queue, wait attribution, `pl=` ─────
occ_re = re.compile(r"win=(\d+)/(\d+)")
pq_re = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
k_re = re.compile(r"khr=([0-9.]+)/kraw=([0-9.]+|-)")
wait_re = re.compile(
    r"wait\[tun=(\d+)% paused=(\d+)% pace=(\d+)% gen=(\d+)% nack=(\d+)% "
    r"defc=(\d+)% tail=(\d+)% flush=(\d+)% n=(\d+) us=(\d+)\]")
dgq_re = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+) full=(\d+) err=(\d+) sp=(\d+)\]")
pl_re = re.compile(r"p(\d+):infl=.*?\spl=([-0-9.]+)")
#: THE DATA-PATH σ, `sig_us=<µs>/n<count>` (`net/diag.rs:652`). `-` is printed
#: for σ before the estimator is valid, so the value alternative must allow it.
sig_re = re.compile(r"p(\d+):infl=.*?\ssig_us=([0-9.]+|-)/n(\d+)")

occ, occap, qd, nd = [], [], [], 0
khrs, kraws, rtps = [], [], []
waits = [[] for _ in range(8)]
dgq = {}
pls = {}
retx = 0
last_sig = []
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
    # ── `retx=` IS AN INTERVAL COUNTER IN THE `[DIAG]` TAIL, NOT A TOTAL ──
    # THE MAXIMUM OVER ALL `[DIAG]` LINES IS THE WITNESS, NEVER THE LAST ONE.
    # Reading it off the last line made the plain-window primitives pass
    # mis-report this witness at 5 OF 15 REPS: the final interval simply
    # carried no retransmit, so a loop that ran all run long read `retx=0` and
    # the W4 reachability witness recorded a FAILURE that never happened.
    m = re.search(r"retx=(\d+)", ln)
    if m:
        retx = max(retx, int(m.group(1)))
    for m in dgq_re.finditer(ln):     # cumulative: last wins
        dgq[int(m.group(1))] = tuple(int(m.group(i)) for i in range(2, 7))
    # THE LAST `[DIAG]` BLOCK's per-path σ. Rebuilt (not appended) on every
    # DIAG line, so what survives the loop is the FINAL block alone.
    _s = [(int(m.group(1)), fnum(m.group(2)) if m.group(2) != "-" else None,
           int(m.group(3))) for m in sig_re.finditer(ln)]
    if _s:
        last_sig = _s

WNAMES = ["tun", "paused", "pace", "gen", "nack", "defc", "tail", "flush"]
wait_out = {f"wait_{n}": med(waits[i]) for i, n in enumerate(WNAMES)}
wait_out["wait_lines"] = len(waits[0])
_wt, _wp = wait_out["wait_tun"], wait_out["wait_paused"]
wait_out["deadwall"] = (
    None if (_wt is None or _wp is None) else bool(_wt == 0 and _wp == 0)
)

# THE DATA-PATH SELECTOR. A dual carries two `sig_us=` tokens per DIAG line and
# they are NOT interchangeable — at `c8` the legs differ by orders of magnitude
# in both σ and sample count. The path with the LARGEST `n` is the one the bulk
# transfer actually rode, so it is the one whose σ feeds `W = srtt + k(α)·σ`.
sig_out = {"sig_us_p50": None, "sig_us_n": None, "sig_us_path": None,
           "sig_paths": len(last_sig)}
if last_sig:
    pid, val, n = max(last_sig, key=lambda t: t[2])
    sig_out.update({"sig_us_p50": val, "sig_us_n": n, "sig_us_path": pid})

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
    "dgq_err": sum(v[3] for v in dgq.values()) if dgq else None,
    "dgq_gap": (sum(v[0] - v[1] for v in dgq.values()) if dgq else None),
}
capboot = {
    "capboot_n": len(occap),
    "capboot_frac": (round(sum(1 for c in occap if c <= 128) / len(occap), 4)
                     if occap else None),
}

# ── the [SF] saturation-filter gauge, cumulative: last line wins ─────────
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

# ── UTILISATION from the shaped device (MEASUREMENT DISCIPLINE 16) ───────
# `ccand_parse.py`'s block verbatim. `tc_s` is `INVOCATION_S` and is carried
# ONLY so the correction is auditable: the headroom denominator is the TRANSFER
# wall (`seconds`), never this.
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

out = {"cell": cell, "arm": arm, "alpha_cmd": alpha_cmd,
       "seed": inum(seed), "rep": inum(rep),
       "n_paths": n_paths,
       "dnf": dnf, "dnf_count": dnf_count,
       # `mbps` is ccand_parse.py's name and pools with that ledger;
       # `mean_mbps` is the sweep spec's name for the SAME number. One parse,
       # two vocabularies, no second definition.
       "mbps": mbps, "mean_mbps": mbps,
       "seconds": secs, "n_runs": len(runs),
       "cpusrv": cpusrv, "cpucli": cpucli,
       "khr_med": med(khrs), "kraw_med": med(kraws),
       "khr_n": len(khrs), "kraw_n": len(kraws),
       "rtp_med": med(rtps),
       "occ_p50": q(occ, 0.5), "occcap_p50": q(occap, 0.5),
       "q_p50": q(qd, 0.5), "q_p99": q(qd, 0.99),
       "diag_lines": nd, "retx": retx,
       # W1/W2/W4'/W5. W3 (`cod=`) IS RETIRED.
       "w1_rfa_gen": w1_rfa_gen, "w2_pfrac_lines": w2_pfrac_lines,
       "w4_retx_max": retx, "w5_rack_fa": w5_rack_fa}
out.update(gates)
out.update(qalpha)
out.update(qclk)
out.update(rack)
out.update(rack_alias)
out.update(rfa)
out.update(sf)
out.update(capboot)
out.update(ping)
out.update(legs)
out.update(wait_out)
out.update(sig_out)
out.update(pl_out)
out.update(dgq_out)
out.update(tc)
print("ALPHARESULT " + json.dumps(out))
