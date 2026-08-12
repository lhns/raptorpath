#!/usr/bin/env python3
"""Per-invocation parser for THE DEAD-WALL BATTERY (goal-gate "The Derived
Recovery Clamp — VM PRE-REGISTRATION" — the CONTRACT; nothing here may
reinterpret it, and no number in it may be changed after the first VM
contact).

A SEPARATE parser from `flip_parse.py` on purpose, for the same reason that
one was separate from `hi_parse.py`: those are the instruments earlier
verdicts were read off and they stay byte-identical. Every column this file
shares with `flip_parse.py` keeps `flip_parse.py`'s definition to the line —
goodput, the abort rule, the wait histogram, retx, occupancy, [SF], capboot,
the ping probe, tc utilisation — so numbers pool across sessions without a
second dialect. What is NEW here is only what this battery is scored on:

  THE DEAD-WALL FLAG   `deadwall` = (`wait_tun` == 0 AND `wait_paused` == 0),
                       the per-rep binary the pre-registration names as the
                       PRIMARY STATISTIC and defines by reference to the
                       preceding section (19 of 19 on the slowest c8 tail,
                       5 of 112 elsewhere). Emitted as a COLUMN rather than
                       re-derived downstream, so the scoring step cannot
                       quietly acquire a second definition.

  THE ARM'S OWN GATES  `RWM_DERIVED_SWEEP` and `RWM_STORE_CAP_UNIFIED` two
                       sided on the `[GATES]` line of BOTH endpoints, plus
                       `RWM_RECOV_MP` RECORDED on every arm (the component
                       bench's standing warning: a change to the recovery
                       plane's clocks is only safe with the RFC 9002 hole
                       law armed) and `RWM_ACKDIAG` as instrument liveness.

  THE DERIVED ROUND'S  `derived recovery round ACTIVE`  — the site EXECUTED.
  TWO ECHOES           `derived recovery round DIVERGED` — the law BOUND.
                       Separate columns because the coincidence property
                       makes them separate claims: the derived law equals
                       the clamped one wherever 2*srtt already lies inside
                       [25, 100] ms, so an arm with ACTIVE but no DIVERGED
                       is bit-identical to its control and its null is a
                       null RESULT, not a null EFFECT. The `us` values are
                       carried through so the departure is a MEASURED number.

  THE ACK-CADENCE      `[ACKDIAG]` line count, as instrument liveness only.
  GAUGE                The gauge is ON in every arm (it is the wait-arm and
                       dead-wall column's companion); its absence is an
                       INSTRUMENT-FAIL, never a datum.

usage: deadwall_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log> \
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
    return round(v[min(len(v) - 1, int(round(p * (len(v) - 1))))], 3)


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


DS_ACTIVE = "derived recovery round ACTIVE"
DS_DIVERGED = "derived recovery round DIVERGED"
U_ACTIVE = "unified store-cap path set ACTIVE"

gates = {
    # the two ARMS of this battery, two-sided on both endpoints
    "gates_cli_ds": gate(cli, "RWM_DERIVED_SWEEP"),
    "gates_srv_ds": gate(srv, "RWM_DERIVED_SWEEP"),
    "gates_cli_u": gate(cli, "RWM_STORE_CAP_UNIFIED"),
    "gates_srv_u": gate(srv, "RWM_STORE_CAP_UNIFIED"),
    # RECORDED on every arm, per the pre-registration's standing warning
    "gates_cli_mp": gate(cli, "RWM_RECOV_MP"),
    "gates_srv_mp": gate(srv, "RWM_RECOV_MP"),
    # instrument liveness
    "gates_cli_ack": gate(cli, "RWM_ACKDIAG"),
    "gates_srv_ack": gate(srv, "RWM_ACKDIAG"),
    "gates_cli_diag": gate(cli, "RWM_DIAG"),
    "gates_srv_diag": gate(srv, "RWM_DIAG"),
    "gates_lines_cli": sum(1 for l in cli if "[GATES]" in l),
    "gates_lines_srv": sum(1 for l in srv if "[GATES]" in l),
    # the mechanism echoes: PRESENT on their arm, ABSENT on the control
    "active_ds_cli": sum(1 for l in cli if DS_ACTIVE in l),
    "active_ds_srv": sum(1 for l in srv if DS_ACTIVE in l),
    "diverged_ds_cli": sum(1 for l in cli if DS_DIVERGED in l),
    "diverged_ds_srv": sum(1 for l in srv if DS_DIVERGED in l),
    "active_u_cli": sum(1 for l in cli if U_ACTIVE in l),
    "active_u_srv": sum(1 for l in srv if U_ACTIVE in l),
}

# ── the derived round's OWN numbers, off whichever echo carries them ─────
# The DIVERGED lines are preferred: they are taken at clocks where the two
# laws actually differ, so their pair is the SIZE of the departure. ACTIVE is
# the fallback and may legitimately show derived == legacy.
#
# THE LARGEST departure is reported, not the first. Both echoes are one-shot
# PER SITE PER PROCESS, and a transfer has up to four of them (sender and
# receiver sites, client and server processes), which fire at whatever clock
# each site happened to see first. The smoke run showed why this matters: the
# earliest divergence was a WARM-UP one at srtt = 10 ms (derived 20 ms vs
# clamped 25 ms, a 5 ms departure BELOW the legacy floor), while the
# steady-state c8 divergence at the same rep was srtt = 552 ms (derived
# 1 104 ms vs clamped 100 ms — an 11x departure). Reporting the first would
# have understated the law's actual bind by two orders of magnitude and made
# every downstream "did it bind?" reading wrong in the safe-looking direction.
ds_re = re.compile(
    r"site=(\S+) srtt_us=(\d+) jitter_us=(\d+) derived_us=(\d+) legacy_us=(\d+)"
)
ds = {"ds_site": None, "ds_srtt_us": None, "ds_jitter_us": None,
      "ds_derived_us": None, "ds_legacy_us": None, "ds_from": None,
      "ds_n_echo": 0}
_best = None
for tag, want in (("diverged", DS_DIVERGED), ("active", DS_ACTIVE)):
    for ln in cli + srv:
        if want not in ln:
            continue
        m = ds_re.search(ln)
        if not m:
            continue
        ds["ds_n_echo"] += 1
        d, l = int(m.group(4)), int(m.group(5))
        gap = abs(d - l)
        # A DIVERGED reading always outranks an ACTIVE one; within a tag the
        # widest departure wins.
        key = (1 if tag == "diverged" else 0, gap)
        if _best is None or key > _best:
            _best = key
            ds.update({"ds_site": m.group(1), "ds_srtt_us": int(m.group(2)),
                       "ds_jitter_us": int(m.group(3)),
                       "ds_derived_us": d, "ds_legacy_us": l, "ds_from": tag})

# ── the ack-cadence gauge: instrument liveness only, never a datum ───────
ackdiag = {
    "ackdiag_lines_cli": sum(1 for l in cli if "[ACKDIAG]" in l),
    "ackdiag_lines_srv": sum(1 for l in srv if "[ACKDIAG]" in l),
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

# ── DELIVERED LATENCY probe (C5's instrument; topo cells) ────────────────
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

# ── DIAG gauges: occupancy, khr/kraw, queue, wait attribution, eviction ──
occ_re = re.compile(r"win=(\d+)/(\d+)")
pq_re = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
k_re = re.compile(r"khr=([0-9.]+)/kraw=([0-9.]+|-)")
wait_re = re.compile(
    r"wait\[tun=(\d+)% paused=(\d+)% pace=(\d+)% gen=(\d+)% nack=(\d+)% "
    r"defc=(\d+)% tail=(\d+)% flush=(\d+)% n=(\d+) us=(\d+)\]")
dgq_re = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+) full=(\d+) err=(\d+) sp=(\d+)\]")

occ, occap, qd, nd = [], [], [], 0
khrs, kraws, rtps = [], [], []
waits = [[] for _ in range(8)]
dgq = {}
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

# ── THE PRIMARY STATISTIC, as a column ───────────────────────────────────
# The pre-registration's per-rep binary, verbatim: `wait_tun` = 0% AND
# `wait_paused` = 0%. `None` (not False) when the histogram never populated
# — an invocation with no steady wait lines has no verdict to give, and the
# scoring step must exclude it rather than count it as a non-collapse.
_wt, _wp = wait_out["wait_tun"], wait_out["wait_paused"]
wait_out["deadwall"] = (
    None if (_wt is None or _wp is None) else bool(_wt == 0 and _wp == 0)
)

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
out.update(ds)
out.update(ackdiag)
out.update(sf)
out.update(capboot)
out.update(ping)
out.update(wait_out)
out.update(dgq_out)
out.update(tc)
print("DEADWALLRESULT " + json.dumps(out))
