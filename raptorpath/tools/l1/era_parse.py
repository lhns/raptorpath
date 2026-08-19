#!/usr/bin/env python3
"""Per-invocation parser for THE ERA BATTERY (goal-gate "Era Battery —
PRE-REGISTRATION" — the CONTRACT; nothing here may reinterpret it, and no number
in it may be changed once the VM has been touched).

  usage: era_parse.py <cell> <arm> <era> <seed> <rep> <clog> <slog>
                      <cpusrv> <cpucli> <ping.txt> <q.txt> <abort.txt>
                      [<leg0.txt,leg1.txt,...>]

The 13th argument is OPTIONAL and ADDITIVE (goal-gate "Latency Truth"): the
per-leg delivered-latency probe files. Absent, this parser behaves exactly as it
did for the era battery. Present, it emits `legN_*` columns with a CENSORING
FRACTION beside every percentile — see the DELIVERED LATENCY block below.

A SEPARATE parser from `ccand_parse.py` / `ladder_parse.py`, for the same reason
those were separate from each other: they are the instruments earlier verdicts
were read off and they stay byte-identical. **Every column this file shares with
them keeps its definition TO THE LINE** — goodput, the DNF rule, occupancy, the
standing-queue estimate, `khr`, `pl=`, the ping probe and the tc utilisation —
so the rows pool across sessions without a second dialect.

WHAT IS NEW HERE IS NOT A GAUGE. It is the ERA AXIS, and it changes what a
missing column MEANS:

  THE LIVENESS ANCHORS   `[GATES]` was added by the 2026-08-09 gate-forwarding
                         audit, ONE DAY AFTER the baseline commit. The rule every
                         battery since the flip battery encodes — "no `[GATES]`
                         on EITHER endpoint = ABORT" — would mark EVERY OLD
                         invocation an abort. So liveness is read from two lines
                         `transport/quic.rs` emits unconditionally on BOTH roles
                         at BOTH commits, byte-identical:

                           anchor_cc_*   "quinn congestion controller: BBR"
                           anchor_mtu_*  "MTU floor: min_mtu=initial_mtu"

                         `live` is TRUE when both are present on both endpoints;
                         `abort` is TRUE when neither is present on either. An
                         anchor on ONE endpoint only is neither — it is an
                         instrument fail, and the two columns keep that
                         distinction instead of collapsing it.

  THE ANTI-MIX COLUMN    `gates_lines_cli` / `_srv` are no longer a gate readout
                         here; they are the MECHANICAL PROOF OF WHICH BINARY
                         RAN. `era_mix_ok` is FALSE when an OLD row carries a
                         `[GATES]` line or a NEW row lacks one, and a FALSE row
                         is VOID — it does not enter a denominator and it is not
                         re-labelled.

  THE ERA-ABSENT GAUGES  `[ACKDIAG]` `[WALL]` `[SUMCAP]` `[DCAP]` `[RACK]`
                         `[LCW]` `[CCAP]` `[SF]` and the wait-reason histogram
                         DO NOT EXIST at `4171b584`. Their absence on an OLD row
                         is CORRECT and is carried as `era_absent_expected` so a
                         scorer never has to infer it, and never reads a column
                         of structural silence as a null RESULT. Their PRESENCE
                         on an OLD row is `era_surprise` — which does not mean
                         the gauge appeared, it means THE BINARY IS NOT THE ERA
                         IT CLAIMS.

  THE ABORT CAUSE        every abort now carries `abort_cause=` from
                         `abort_witness.py`, imported rather than re-implemented
                         so the column has ONE definition across sessions. The
                         exclusion of aborts from every denominator is sound only
                         while aborts are INDEPENDENT OF THE ARM, and at c8/seed
                         7 the Candidates Battery measured 20 % on the control
                         against 75 % on the RACK arm.

THE `[RACK]` BLOCK IS PARSED ONLY FOR THE `NR` ARM'S SAKE, and its selection
rule is the Candidates Battery's calibration finding, transcribed: TWO gauges
emit into the client log on a RACK-armed arm (the sender gauge, and the client's
own idle receiver-role gauge with `evals=0 fa=0/0`), so fields are selected BY
DATUM per field group — clock-law fields from the line with MAX `evals`, `fa`
fields from the line with MAX `fa` denominator. Taking "the last line" would make
the columns depend on teardown ORDER.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from abort_witness import read_witness  # noqa: E402  ONE definition of abort_cause
from latt_probe import PCTS, probe_stats  # noqa: E402  ONE definition of censoring

#: The era-invariant liveness anchors, as FIXED strings. Both are emitted by
#: `transport/quic.rs` at endpoint construction on BOTH roles, and both are
#: byte-identical at `4171b584` and `6ad964d` — which is the entire reason they
#: can carry a cross-era liveness verdict when `[GATES]` cannot.
ANCHOR_CC = "quinn congestion controller: BBR"
ANCHOR_MTU = "MTU floor: min_mtu=initial_mtu"

#: Gauges that DO NOT EXIST at the OLD commit. Read off the source at both
#: commits, not guessed: `git grep -hoE '\[[A-Z][A-Z0-9]+\]' <commit> -- src`.
NEW_ONLY_GAUGES = ["GATES", "ACKDIAG", "WALL", "SUMCAP", "DCAP", "RACK",
                   "LCW", "CCAP", "SF"]


def read(p):
    try:
        with open(p, "r", errors="replace") as f:
            return [re.sub(r"\x1b\[[0-9;]*m", "", l) for l in f]
    except OSError:
        return []


def med(v):
    if not v:
        return None
    s = sorted(v)
    n = len(s)
    return round(s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2, 4)


def q(v, p):
    if not v:
        return None
    s = sorted(v)
    return round(s[min(len(s) - 1, int(p * len(s)))], 4)


av = sys.argv[1:]
cell, arm, era, seed, rep, clog, slog = av[:7]
cpusrv = float(av[7]) if len(av) > 7 and av[7] not in ("", "-") else None
cpucli = float(av[8]) if len(av) > 8 and av[8] not in ("", "-") else None
ping_path = av[9] if len(av) > 9 else ""
q_path = av[10] if len(av) > 10 else ""
abort_path = av[11] if len(av) > 11 else "/tmp/rwm-abort.txt"
cli = read(clog)
srv = read(slog)

#: TRANSCRIBED from the battery driver's own `cell_spec`, exactly as
#: `capbind_check.CELL_PATHS` is, never inferred. Load-bearing for the era
#: battery's P4: at a single-path cell BOTH cap flips of the arc are inert by
#: construction (the pooled seat short-circuits at `n_live < 2`), so any move
#: there belongs to ack-merge, the anchor filter, or the residue.
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

# ── G-LIVE, PER ERA — the anchors, counted per endpoint ──────────────────
anchor = {
    "anchor_cc_cli": sum(1 for l in cli if ANCHOR_CC in l),
    "anchor_cc_srv": sum(1 for l in srv if ANCHOR_CC in l),
    "anchor_mtu_cli": sum(1 for l in cli if ANCHOR_MTU in l),
    "anchor_mtu_srv": sum(1 for l in srv if ANCHOR_MTU in l),
}
# `live` and `abort` are NOT complements. An anchor on ONE endpoint only is
# neither live nor aborted — it is an instrument fail for the rep, and
# collapsing the two would silently promote it into one of the categories.
anchor["live"] = bool(anchor["anchor_cc_cli"] and anchor["anchor_cc_srv"]
                      and anchor["anchor_mtu_cli"] and anchor["anchor_mtu_srv"])
anchor["abort"] = (sum(anchor[k] for k in
                       ("anchor_cc_cli", "anchor_cc_srv",
                        "anchor_mtu_cli", "anchor_mtu_srv")) == 0)

# ── G-ERA: the anti-mix assertion, and the NEW era's shipped defaults ────
gates_cli = sum(1 for l in cli if "[GATES]" in l)
gates_srv = sum(1 for l in srv if "[GATES]" in l)


def gate(lines, name):
    g = [l for l in lines if "[GATES]" in l]
    if not g:
        return None
    m = re.search(name + r"=([01])", g[-1])
    return int(m.group(1)) if m else None


gates = {"gates_lines_cli": gates_cli, "gates_lines_srv": gates_srv}
for g in ("RWM_ACK_MERGE", "RWM_SUM_CAP", "RWM_DELTA_CAP", "RWM_HONEST_ANCHOR",
          "RWM_RACK_CLOCKS", "RWM_QUANTILE_CLOCKS", "RWM_LOSS_SENT_TRUTH"):
    short = g[4:].lower()
    gates["gates_cli_" + short] = gate(cli, g)
    gates["gates_srv_" + short] = gate(srv, g)
# THE VOID RULE, as a column rather than as a rule the scorer must remember.
#
# `None` ON AN ABORT, and the distinction is load-bearing: an aborted invocation
# produced no log on either endpoint, so it carries NO EVIDENCE about which
# binary ran. Scoring it `False` would file every abort as an anti-mix violation
# and manufacture a wrong-era finding out of the abort class this battery was
# built to explain. `False` here means "a binary from the WRONG era demonstrably
# ran"; `None` means "the question is unanswerable on this rep".
gates["era_mix_ok"] = (
    None if anchor["abort"]
    else ((gates_cli == 0 and gates_srv == 0) if era == "old"
          else (gates_cli > 0 and gates_srv > 0))
)

# ── THE ERA-ABSENT GAUGES ────────────────────────────────────────────────
# `era_absent_expected` says "these columns are silent BY CONSTRUCTION on this
# row"; `era_surprise` says the binary is not the era it claims. The two are
# kept apart because only one of them is a defect.
counts = {}
for f in NEW_ONLY_GAUGES:
    counts[f] = (sum(1 for l in cli if f"[{f}]" in l),
                 sum(1 for l in srv if f"[{f}]" in l))
era_cols = {
    "era_absent_expected": (era == "old"),
    "era_surprise": (era == "old"
                     and any(c + s > 0 for f, (c, s) in counts.items()
                             if f != "GATES")),
    "era_surprise_which": ",".join(f for f, (c, s) in counts.items()
                                   if era == "old" and f != "GATES" and c + s > 0) or None,
}
for f, (c, s) in counts.items():
    era_cols[f"g_{f.lower()}_cli"] = c
    era_cols[f"g_{f.lower()}_srv"] = s

# ── `[RACK]` — the NR arm's clamp readout, and NOTHING else's ────────────
# The SHIPPED [25,100] ms clamp's own bind fraction (`legacy_pin`) is a
# counterfactual computed INSIDE the armed law, so it is fed on the RACK-ON arm
# ONLY — which is why NR exists. `fa=<spurious>/<fired>` is fed ungated but only
# from the SENDER site; `fa=0/0` is an INSTRUMENT-FAIL for the rep and NEVER
# `fa_frac = 0`, so the denominator is carried as its own column.
rack_re = re.compile(
    r"\[RACK\] on=(\d+) evals=(\d+) ceil=([0-9.]+) gran=([0-9.]+) "
    r"legacy_pin=([0-9.]+) round=([0-9.]+) legacy=([0-9.]+)"
)
fa_re = re.compile(r"fa=(\d+)/(\d+)(?:\s+fa_frac=([0-9.]+))?")
rack = {"rack_lines_cli": 0, "rack_on": None, "rack_evals": None,
        "rack_ceil": None, "rack_gran": None, "rack_legacy_pin": None,
        "rack_round": None, "rack_legacy": None,
        "rack_fa_n": None, "rack_fa_d": None, "rack_fa_frac": None}
best_ev, best_fa = -1, -1
for ln in cli:
    if "[RACK]" not in ln:
        continue
    rack["rack_lines_cli"] += 1
    m = rack_re.search(ln)
    # SELECTED BY DATUM, PER FIELD GROUP — the candidates calibration's finding:
    # two gauges emit into the client log on an armed arm (the sender's, and the
    # client's own idle receiver-role gauge at `evals=0 fa=0/0`), so "the last
    # line" would make these columns depend on teardown ORDER.
    if m and int(m.group(2)) > best_ev:
        best_ev = int(m.group(2))
        rack.update({"rack_on": int(m.group(1)), "rack_evals": int(m.group(2)),
                     "rack_ceil": float(m.group(3)), "rack_gran": float(m.group(4)),
                     "rack_legacy_pin": float(m.group(5)),
                     "rack_round": float(m.group(6)), "rack_legacy": float(m.group(7))})
    f = fa_re.search(ln)
    if f and int(f.group(2)) > best_fa:
        best_fa = int(f.group(2))
        n, d = int(f.group(1)), int(f.group(2))
        rack.update({"rack_fa_n": n, "rack_fa_d": d,
                     "rack_fa_frac": (round(n / d, 4) if d else None)})

# ── DELIVERED LATENCY probe — harness-side, and therefore era-invariant ──
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
    "ping_tx": p_tx, "ping_rx": p_rx,
    "ping_loss": (round(100.0 * (p_tx - p_rx) / p_tx, 2) if p_tx else None),
}

# ── PER-LEG DELIVERED LATENCY, WITH ITS CENSORING (goal-gate "Latency Truth")
#
# ADDITIVE, AND THAT IS THE POINT. The `ping_*` block above keeps its definition
# TO THE LINE — it still reads `/tmp/rwm-ping.txt`, which `perf_rwm_c.sh` still
# writes as LEG A's file with byte-identical content — so the era ledger's rows
# pool with these without a second dialect. What is new is a column set the era
# battery COULD NOT HAVE HAD, because the harness only ever probed one leg:
#
#   legN_p50/p95/p99   the per-leg delivered percentiles
#   legN_censor_frac   the fraction of probes that never produced a sample
#   legN_<p>_scoreable the pre-registered verdict, PER PERCENTILE
#
# `latt_probe.py` owns the definitions; they are imported and not duplicated,
# for the same reason `abort_witness.read_witness` is.
leg_paths = [p for p in (av[12].split(",") if len(av) > 12 and av[12] else [])
             if p]
legs = {}
for _i, _p in enumerate(leg_paths):
    _s = probe_stats(_p, leg=_i)
    for _k in ("n", "sent", "recv", "sent_source", "censor_frac",
               "recv_mismatch", "leg_unscoreable", "min", "max"):
        legs["leg%d_%s" % (_i, _k)] = _s[_k]
    for _name, _ in PCTS:
        legs["leg%d_%s" % (_i, _name)] = _s[_name]
        legs["leg%d_%s_censored" % (_i, _name)] = _s[_name + "_censored"]
        legs["leg%d_%s_scoreable" % (_i, _name)] = _s[_name + "_scoreable"]
legs["legs_probed"] = len(leg_paths)
# THE WORST LEG, precomputed, because a two-leg system's delivered latency is
# not the mean of its legs and a scorer that averages them has thrown away the
# asymmetry that made `c8` interesting. Reported alongside the per-leg columns,
# never instead of them.
_cf = [legs["leg%d_censor_frac" % i] for i in range(len(leg_paths))
       if legs.get("leg%d_censor_frac" % i) is not None]
legs["legs_censor_max"] = (max(_cf) if _cf else None)
_p50 = [legs["leg%d_p50" % i] for i in range(len(leg_paths))
        if legs.get("leg%d_p50" % i) is not None]
legs["legs_p50_max"] = (max(_p50) if _p50 else None)

# ── DIAG gauges: occupancy, khr, standing queue, `pl=`, retx ────────────
# EVERY regex below is one the OLD binary's own `[DIAG]` format satisfies —
# checked against `4171b584:raptorpath/src/net/mod.rs:8388,8570` before this
# file was written, because a column that silently reads `None` on one arm and a
# number on the other is a fabricated era difference.
occ_re = re.compile(r"win=(\d+)/(\d+)")
pq_re = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
k_re = re.compile(r"khr=([0-9.]+)/kraw=([0-9.]+|-)")
pl_re = re.compile(r"p(\d+):infl=.*?\spl=([-0-9.]+)")

occ, occap, qd, nd = [], [], [], 0
khrs, kraws, rtps = [], [], []
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
    m = re.search(r"retx=(\d+)", ln)
    if m:
        retx = max(retx, int(m.group(1)))

pl_out = {"pl_n": sum(len(v) for v in pls.values()), "pl_max": None, "pl_min": None}
for pid, vals in sorted(pls.items()):
    pl_out[f"pl_p{pid}"] = med(vals)
_pms = [med(v) for v in pls.values() if v]
if _pms:
    pl_out["pl_max"] = max(_pms)
    pl_out["pl_min"] = min(_pms)

# ── `[CTLD]` — the control-datagram density, and it is ERA-INVARIANT ─────
# THE ONE MECHANISM GAUGE THIS BATTERY HAS ON BOTH ERAS. It exists at
# `4171b584` and at `6ad964d`, and it is the quantity `RWM_ACK_MERGE` changes:
# the ack-merge flip's own finding was that `[CTLD]` reads 1.96 at c1 against
# 1.05 at c7. So P1's and P3's mechanism is directly observable here rather than
# inferred from goodput — which is the only cross-era mechanism reading the
# battery gets, every other gauge of the arc being NEW-only.
#
# THE LINE IS `[CTLD] p<id> tx=<n> rx=<n>`, byte-identical at both commits
# (`net/receiver.rs:1661` at NEW, `net/mod.rs:3763` at OLD), emitted 1 Hz by the
# RECEIVER under `RWM_DIAG` — so it lands in the SERVER log, and it is
# CUMULATIVE per path: the LAST line wins, and the ratio is summed over paths
# rather than averaged over lines. Averaging the per-line ratios of a cumulative
# counter would weight the warm-up as heavily as the steady state.
ctld_re = re.compile(r"p(\d+) tx=(\d+) rx=(\d+)")
ctld = {"ctld_lines": 0, "ctld_tx": None, "ctld_rx": None, "ctld_ratio": None}
_last = {}
for ln in srv + cli:
    if "[CTLD]" not in ln:
        continue
    ctld["ctld_lines"] += 1
    for m in ctld_re.finditer(ln):
        _last[int(m.group(1))] = (int(m.group(2)), int(m.group(3)))
if _last:
    tx = sum(v[0] for v in _last.values())
    rx = sum(v[1] for v in _last.values())
    ctld.update({"ctld_tx": tx, "ctld_rx": rx,
                 "ctld_ratio": (round(tx / rx, 4) if rx else None)})

# ── UTILISATION from the shaped device (MEASUREMENT DISCIPLINE 16) ───────
# `tc_s` is `INVOCATION_S` and is carried ONLY so the correction is auditable:
# the headroom denominator is the TRANSFER wall (`seconds`), never this.
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
        tc = {"tc_bytes": sum(v[0] for v in seen.values()),
              "tc_pkts": sum(v[1] for v in seen.values()),
              "tc_drop": sum(v[2] for v in seen.values()),
              "tc_s": secs_q}

# ── THE ABORT-CAUSE WITNESS, imported and not re-implemented ────────────
witness = read_witness(abort_path)

out = {"cell": cell, "arm": arm, "era": era, "seed": int(seed), "rep": int(rep),
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
out.update(anchor)
out.update(gates)
out.update(era_cols)
out.update(rack)
out.update(ctld)
out.update(ping)
out.update(legs)
out.update(pl_out)
out.update(tc)
out.update(witness)
print("ERARESULT " + json.dumps(out))
