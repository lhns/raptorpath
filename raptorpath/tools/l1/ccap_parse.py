#!/usr/bin/env python3
"""Per-invocation parser for THE COMPOSED-CAP BATTERY (goal-gate "Composed-Cap
Battery — VM PRE-REGISTRATION" — the CONTRACT; nothing here may reinterpret it,
and no number in it may be changed now that the VM has been touched).

A SEPARATE parser from `deadwall_parse.py` on purpose, for the same reason that
one was separate from `flip_parse.py`: those are the instruments earlier
verdicts were read off and they stay byte-identical. Every column this file
shares with them keeps their definition TO THE LINE — goodput, the abort rule,
the wait histogram, retx, occupancy, [SF], capboot, the ping probe, tc
utilisation — so numbers pool across sessions without a second dialect.

What is NEW here is only what this battery is scored on:

  THE `[CCAP]` GAUGE   The composed law's own per-run readout (paper §16.56).
                       Five separate claims, five separate columns, because
                       collapsing them is exactly the confusion the gauge was
                       built to prevent:

                         ccap_eng    engaged/refreshes -- MECHANISM LIVENESS.
                                     `eng=0/N` with RWM_COMPOSED_CAP=1 on the
                                     [GATES] line is a WARM-UP FAILURE, not a
                                     null result. It is an INSTRUMENT-FAIL and
                                     the rep carries no datum.
                         ccap_cap    the realized mean cap -- the number the
                                     whole arm is about.
                         ccap_mem    BIND FRACTION of `WIN_STORE_MAX` = 4096,
                                     a memory bound stated OUTSIDE the law.
                                     Above 0.5 the memory bound HAS BECOME the
                                     law -- the predecessor's exact defect
                                     reproduced, and §16.56 calls that a STOP
                                     rather than a result.
                         ccap_floor  BIND FRACTION of the one paroled constant
                                     (`store_cap_floor` = 64, provenance
                                     ABSENT per ADR-0070 finding 5). The
                                     composed-cap LOOPBACK already read 1.0000
                                     here; the wire's RTprops are 2-3 orders
                                     larger, so a bind at any cell in this
                                     battery is a discipline-18 finding.
                         ccap_brake  brake_closed/brake_ticks -- the late-stage
                                     brake's OWN liveness. `brake=0/N` is the
                                     difference between "the brake never bound"
                                     and "the brake was never armed", i.e.
                                     between a null RESULT and a null EFFECT.

  THE `[WALL]` GAUGE   The dead-wall ONSET/DURATION instrument (RWM_WALLDIAG).
                       THE MEASURAND THE TICK-SHARE FLAG IS BEING REPLACED BY,
                       and the reason it is being replaced is in the module
                       doc of `net/walldiag.rs`: the tick-share statistic's arm
                       orderings INVERTED between pools collected minutes
                       apart, because a tick-share is a fraction of sender-loop
                       WAKEUPS and the wakeup rate is an output of the very
                       mechanism under test. `wall_dur_ms` and `wall_onset` are
                       wall-clock quantities of ONE named event (the contiguous
                       terminal window), defined per RUN rather than per tick.
                       `wall_it_ms` carries the loop period so the resolution
                       bound is READ OFF every run rather than assumed.

  THE TICK-SHARE       `deadwall` = (`wait_tun` == 0 AND `wait_paused` == 0) is
  WITNESS              STILL emitted, with `deadwall_parse.py`'s definition to
                       the line. It is scored on NOTHING in this battery. It is
                       here so the old and new measurands can be compared on
                       IDENTICAL reps -- which is the only way the claim "the
                       new one is stable and the old one was not" can ever be
                       checked rather than asserted. `wait_lines` travels with
                       it because the old statistic's tick population is a
                       function of transfer duration (6-17 at 25 MB, ~78 at
                       200 MB) and that is half of why it was unstable.

  THE ARM'S OWN GATE   `RWM_COMPOSED_CAP`, two-sided on the `[GATES]` line of
                       BOTH endpoints; `RWM_WALLDIAG` and `RWM_ACKDIAG` as
                       instrument liveness on both; `RWM_STORE_CAP_UNIFIED` and
                       `RWM_THREE_TERM` RECORDED on every arm as WITNESSES.
                       Both witnesses are expected 0 on BOTH arms and that is
                       CORRECT: the composed gate reaches the three-term pool
                       seat through `sender_policy`'s
                       `three_term_on = (three_term || composed_cap)`, without
                       RWM_THREE_TERM being set; and it does not set
                       RWM_STORE_CAP_UNIFIED at all, because the pool law
                       already reads `live_paths()` unconditionally and the
                       composed arm's unified set is at the BRAKE.

  THE POOL SEAT'S      `three-term outstanding limit ACTIVE` -- the pool law
  ECHO                 EXECUTED. PRESENT on C, ABSENT on A. This is the echo
                       that proves the composed gate reached the pool seat,
                       and it is a separate claim from `[CCAP]` reporting,
                       which proves the gauge reached the teardown.

usage: ccap_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log> \
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


TT_ACTIVE = "three-term outstanding limit ACTIVE"
U_ACTIVE = "unified store-cap path set ACTIVE"

gates = {
    # THE ARM of this battery, two-sided on both endpoints
    "gates_cli_cc": gate(cli, "RWM_COMPOSED_CAP"),
    "gates_srv_cc": gate(srv, "RWM_COMPOSED_CAP"),
    # instrument liveness -- absence is an INSTRUMENT-FAIL, never a datum
    "gates_cli_wall": gate(cli, "RWM_WALLDIAG"),
    "gates_srv_wall": gate(srv, "RWM_WALLDIAG"),
    "gates_cli_ack": gate(cli, "RWM_ACKDIAG"),
    "gates_srv_ack": gate(srv, "RWM_ACKDIAG"),
    "gates_cli_diag": gate(cli, "RWM_DIAG"),
    "gates_srv_diag": gate(srv, "RWM_DIAG"),
    # WITNESSES, recorded on every arm, expected 0 on BOTH (see the docstring)
    "gates_cli_u": gate(cli, "RWM_STORE_CAP_UNIFIED"),
    "gates_srv_u": gate(srv, "RWM_STORE_CAP_UNIFIED"),
    "gates_cli_3t": gate(cli, "RWM_THREE_TERM"),
    "gates_srv_3t": gate(srv, "RWM_THREE_TERM"),
    "gates_cli_mp": gate(cli, "RWM_RECOV_MP"),
    "gates_srv_mp": gate(srv, "RWM_RECOV_MP"),
    "gates_lines_cli": sum(1 for l in cli if "[GATES]" in l),
    "gates_lines_srv": sum(1 for l in srv if "[GATES]" in l),
    # the pool seat's mechanism echo: PRESENT on C, ABSENT on A
    "active_3t_cli": sum(1 for l in cli if TT_ACTIVE in l),
    "active_3t_srv": sum(1 for l in srv if TT_ACTIVE in l),
    # expected ABSENT on BOTH arms, and that is CORRECT -- recorded so the
    # silence can never be mistaken for a disarmed arm
    "active_u_cli": sum(1 for l in cli if U_ACTIVE in l),
    "active_u_srv": sum(1 for l in srv if U_ACTIVE in l),
}

# ── `[CCAP]` — the composed law's ENGAGEMENT + BIND-FRACTION gauge ───────
# ONE line per sender, at teardown. Last line wins (both teardown arms emit
# it and each returns immediately, so there is at most one; taking the last
# is defensive, not a pooling rule). Engagement is kept as a RATIO WITH ITS
# NUMERATOR AND DENOMINATOR, not only as a fraction, because `eng=0/0` (no
# refresh ever happened) and `eng=0/200` (200 refreshes, every one cold) are
# different findings and a fraction alone cannot tell them apart.
ccap_re = re.compile(
    r"\[CCAP\] eng=(\d+)/(\d+) cap=([0-9.]+) mem=([0-9.]+) floor=([0-9.]+) "
    r"floor_val=(\d+) brake=(\d+)/(\d+) brake_frac=([0-9.]+)"
)
ccap = {"ccap_lines": 0, "ccap_eng_n": None, "ccap_eng_d": None,
        "ccap_eng": None, "ccap_cap": None, "ccap_mem": None,
        "ccap_floor": None, "ccap_floor_val": None,
        "ccap_brake_closed": None, "ccap_brake_ticks": None,
        "ccap_brake": None}
for ln in cli + srv:
    m = ccap_re.search(ln)
    if not m:
        continue
    ccap["ccap_lines"] += 1
    en, ed = int(m.group(1)), int(m.group(2))
    bc, bt = int(m.group(7)), int(m.group(8))
    ccap.update({
        "ccap_eng_n": en, "ccap_eng_d": ed,
        "ccap_eng": round(en / ed, 4) if ed else None,
        "ccap_cap": float(m.group(3)),
        "ccap_mem": float(m.group(4)),
        "ccap_floor": float(m.group(5)),
        "ccap_floor_val": int(m.group(6)),
        "ccap_brake_closed": bc, "ccap_brake_ticks": bt,
        "ccap_brake": round(bc / bt, 4) if bt else None,
    })

# ── `[WALL]` — the dead-wall ONSET/DURATION instrument ───────────────────
# ONE line per sender, at teardown. `wall_onset` is a FRACTION of the
# transfer wall so it compares across cells an order of magnitude apart;
# `wall_dur_ms` is ABSOLUTE because a wall is bad in milliseconds, not in
# percent. `wall_retx` distinguishes a recovery tail from a hang, and
# `wall_it_ms` is the resolution bound, reported rather than assumed.
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

# ── DELIVERED LATENCY probe (the P-LATENCY-SC2 instrument) ───────────────
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

# ── THE TICK-SHARE WITNESS, scored on NOTHING ────────────────────────────
# `deadwall_parse.py`'s definition to the line, so the two measurands can be
# compared on identical reps. `None` (not False) when the histogram never
# populated -- an invocation with no steady wait lines has no verdict to
# give, and it must not be counted as a non-collapse.
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
# On EVERY cell and EVERY invocation, not a subset -- the three-term
# battery took tc on 2 of its 9 cells, which is why its unsatisfiable
# criteria were only visible afterwards.
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
out.update(ccap)
out.update(wall)
out.update(ackdiag)
out.update(sf)
out.update(capboot)
out.update(ping)
out.update(wait_out)
out.update(dgq_out)
out.update(tc)
print("CCAPRESULT " + json.dumps(out))
