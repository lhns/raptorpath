#!/usr/bin/env python3
"""Per-invocation parser for the LATENCY-SCORED battery (goal-gate "Latency
Lever").

A SEPARATE parser from `tt_parse.py` on purpose: that one is the three-term
battery's instrument and its output is what those verdicts were read off, so
it is left byte-identical. This one scores a different thing.

    lat_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log> [ping.txt] [q.txt]

Emits one `LATRESULT ` + JSON line. The fields, and why each is here:

  GOODPUT — `mbps`, `seconds`, `dnf`. This is the CONSTRAINT, not the score:
  a latency win that costs goodput is a trade, and the pre-registration
  requires parity before any latency number is read.

  DELIVERED LATENCY — `ping_*`. An independent ICMP flow at 20 pkt/s sharing
  the same shaped qdisc as the bulk transfer, measured by the kernel, running
  in EVERY arm. This is the score. It is not produced by the code under test,
  which the engine's own `rtt=` gauge is.

  QUEUEING DELAY — `q_*` = per-path `rtt` − `rtp` off the sender's [DIAG]
  lines, steady state. The engine's own reading of the same physics; kept as
  a corroborating second instrument, never as the primary one.

  OCCUPANCY — `occ_*` / `occcap_p50`. The mechanism gauge: the law's whole
  effect is on this number, so an arm where it does not move has not run.

  UTILISATION — `tc_*` from the qdisc capture. The DENOMINATOR whose absence
  is what MEASUREMENT DISCIPLINE 16 exists to prevent. Present on EVERY cell
  now, not 2 of 9.

  WAIT ATTRIBUTION — `wait_*`. Where the sender's time went, by `select!`
  arm. Zero before this branch: the gauge did not exist in window mode.

  EVICTION — `dgq_*`. Handoffs vs frames quinn actually transmitted.
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
    try:
        with open(path, errors="replace") as f:
            return [re.sub(r"\x1b\[[0-9;]*m", "", ln) for ln in f]
    except OSError:
        return []


av = sys.argv[1:]
cell, arm, seed, rep, clog, slog = av[:6]
ping_path = av[6] if len(av) > 6 else "/tmp/rwm-ping.txt"
q_path = av[7] if len(av) > 7 else "/tmp/rwm-q.txt"
cli = read(clog)
srv = read(slog)

# ── goodput: the PARITY CONSTRAINT ───────────────────────────────────────
# Same shape tt_parse.py reads, and the same abort rule: no summary at all
# means the invocation died before the engine started, which is an ABORT and
# not a DNF. Conflating them would have reported dnf=111 last time.
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
    dnf = True

# ── liveness, two-sided on BOTH endpoints (discipline 15c) ───────────────
def gate(lines, name):
    g = [l for l in lines if "[GATES]" in l]
    if not g:
        return None
    m = re.search(name + r"=([01])", g[-1])
    return int(m.group(1)) if m else None


gates = {
    "gates_cli_3t": gate(cli, "RWM_THREE_TERM"),
    "gates_srv_3t": gate(srv, "RWM_THREE_TERM"),
    "gates_cli_rs": gate(cli, "RWM_PLAIN_RS"),
    "gates_srv_rs": gate(srv, "RWM_PLAIN_RS"),
    "gates_lines_cli": sum(1 for l in cli if "[GATES]" in l),
    "gates_lines_srv": sum(1 for l in srv if "[GATES]" in l),
    "active_echo_cli": sum(1 for l in cli if "three-term outstanding limit ACTIVE" in l),
    "tt_eng1": sum(1 for l in cli if "[3T]" in l and "eng=1" in l),
}

# ── DELIVERED LATENCY: the independent loaded probe. THE SCORE. ──────────
# `ping -D` prints `[<unix ts>] 64 bytes from ...: ... time=<ms> ms`. Loss is
# counted from the summary line, because a probe that is DROPPED by a full
# queue is the same phenomenon as one that is delayed by it and must not be
# silently excluded from the tail.
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
    "ping_max": round(max(rtts), 3) if rtts else None,
    "ping_min": round(min(rtts), 3) if rtts else None,
    "ping_tx": p_tx,
    "ping_rx": p_rx,
    "ping_loss": (round(100.0 * (p_tx - p_rx) / p_tx, 2) if p_tx else None),
}

# ── occupancy + the engine's own queue estimate + the new gauges ─────────
occ_re = re.compile(r"win=(\d+)/(\d+)")
# Per-path: `rtt=<ms>/wrtt=<ms>/rtp<ms>ms` (integers). queue = rtt - rtp.
pq_re = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
wait_re = re.compile(
    r"wait\[tun=(\d+)% paused=(\d+)% pace=(\d+)% gen=(\d+)% nack=(\d+)% "
    r"defc=(\d+)% tail=(\d+)% flush=(\d+)% n=(\d+) us=(\d+)\]")
dgq_re = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+) full=(\d+) err=(\d+) sp=(\d+)\]")

occ, occap, qd, nd, retx = [], [], [], 0, 0
waits = [[] for _ in range(8)]
dgq = {}
for ln in cli:
    if "[DIAG]" not in ln:
        continue
    nd += 1
    steady = nd >= 4          # the pooling rule tt_parse.py uses, unchanged
    m = occ_re.search(ln)
    if m and steady:
        occ.append(int(m.group(1)))
        occap.append(int(m.group(2)))
    if steady:
        for m in pq_re.finditer(ln):
            qd.append(max(0, int(m.group(1)) - int(m.group(3))))
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
# Eviction, summed over paths. `hand - tx` is the estimate that does not rest
# on the queue-full predicate; `full` is the predicate. Both are reported
# because neither is exact and saying so is the point.
dgq_out = {
    "dgq_hand": sum(v[0] for v in dgq.values()) or None,
    "dgq_tx": sum(v[1] for v in dgq.values()) or None,
    "dgq_full": sum(v[2] for v in dgq.values()) if dgq else None,
    "dgq_err": sum(v[3] for v in dgq.values()) if dgq else None,
    "dgq_gap": (sum(v[0] - v[1] for v in dgq.values()) if dgq else None),
    "dgq_paths": len(dgq) or None,
}

# ── UTILISATION from the shaped device — MEASUREMENT DISCIPLINE 16 ───────
# Sums the DATA-direction sections (CLI0 and, on dual cells, CLI1), takes the
# FIRST `Sent` per section (the root qdisc), and reads the invocation wall
# seconds the capture itself carries so utilisation needs no join.
QSENT = re.compile(r"Sent (\d+) bytes (\d+) pkts? \(dropped (\d+)")
tc = {"tc_bytes": None, "tc_pkts": None, "tc_drop": None, "tc_s": None}
if os.path.exists(q_path):
    cur, secs_q, seen = None, None, {}
    for ln in read(q_path):
        if ln.startswith("== "):
            if ln.startswith("== SRV0-INGRESS"):
                cur = None
            elif ln.startswith("== CLI0"):
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
       "occ_p50": q(occ, 0.5), "occ_p90": q(occ, 0.9),
       "occcap_p50": q(occap, 0.5),
       "q_p50": q(qd, 0.5), "q_p95": q(qd, 0.95), "q_p99": q(qd, 0.99),
       "q_n": len(qd), "diag_lines": nd, "retx": retx}
out.update(gates)
out.update(ping)
out.update(wait_out)
out.update(dgq_out)
out.update(tc)
print("LATRESULT " + json.dumps(out))
