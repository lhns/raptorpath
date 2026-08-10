#!/usr/bin/env python3
"""Per-invocation parser for the HONEST-INPUTS scored battery (goal-gate
"Honest Inputs — PRE-REGISTRATION", commit 6f6f2a9 — the CONTRACT; nothing
here may reinterpret it).

A SEPARATE parser from `lat_parse.py`/`tt_parse.py` on purpose: those are the
instruments prior verdicts were read off and stay byte-identical. This one
carries the same abort rule (no summary at all = ABORT, not DNF) plus the
three gauges this battery is scored on:

  CPU        — `cpusrv`/`cpucli` seconds, passed in by the driver from the
               `CPU: CPUSRV=…s CPUCLI=…s` gauge. Criterion H2 (DH CPU/byte
               <= 1.15x A) is scored on this; an invocation without it is an
               INSTRUMENT-FAIL, never silently scoreable.
  K SOURCES  — `khr` (the legacy smoothed windowed-min read, present every
               arm) and `kraw` (the raw-fed read under RWM_HONEST_K, "-"
               otherwise) off steady-state [DIAG] lines. khr − kraw is the
               smoothing bias measured IN-CELL — the jit25 decomposition.
  HONESTY GATES — `[GATES] RWM_HONEST_ANCHOR/RWM_HONEST_K` two-sided on both
               endpoints plus the two resolve-time ACTIVE echoes, counted on
               both logs. An arm without its verified echo is VOID (re-run,
               not explained).

Everything else (goodput, [3T] law readout, occupancy, ping probe, tc
utilisation, wait attribution, eviction) keeps the latency battery's
definitions so numbers pool across sessions without a second dialect.

usage: hi_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log> \
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

# ── goodput: abort ≠ DNF (the encoded rule the pre-registration carries) ──
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


# ── liveness: [GATES] resolved values, both endpoints, all four gates ────
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
    "gates_cli_ha": gate(cli, "RWM_HONEST_ANCHOR"),
    "gates_srv_ha": gate(srv, "RWM_HONEST_ANCHOR"),
    "gates_cli_hk": gate(cli, "RWM_HONEST_K"),
    "gates_srv_hk": gate(srv, "RWM_HONEST_K"),
    "gates_lines_cli": sum(1 for l in cli if "[GATES]" in l),
    "gates_lines_srv": sum(1 for l in srv if "[GATES]" in l),
    "active_3t_cli": sum(1 for l in cli if "three-term outstanding limit ACTIVE" in l),
    "active_ha_cli": sum(1 for l in cli if "windowed-max rate filter ACTIVE" in l),
    "active_ha_srv": sum(1 for l in srv if "windowed-max rate filter ACTIVE" in l),
    "active_hk_cli": sum(1 for l in cli if "echo-ratio floor ACTIVE" in l),
    "active_hk_srv": sum(1 for l in srv if "echo-ratio floor ACTIVE" in l),
}

# ── the law's own readout: [3T] cap/window/slack/span (tt_parse dialect) ─
tt_re = re.compile(
    r"eng=(\d+)\s+cap=(\d+)\s+window=([0-9.eE+-]+)\s+slack=([0-9.eE+-]+)"
    r"\s+span=([0-9.eE+-]+)\s+rho=([0-9.eE+-]+)\s+b=([0-9.eE+-]+)"
)
tt_eng = 0
caps, wins, slacks, spans = [], [], [], []
for ln in cli:
    if "[3T]" not in ln:
        continue
    m = tt_re.search(ln)
    if not m or int(m.group(1)) != 1:
        continue
    tt_eng += 1
    caps.append(int(m.group(2)))
    wins.append(float(m.group(3)))
    slacks.append(float(m.group(4)))
    spans.append(float(m.group(5)))

# ── DELIVERED LATENCY probe (present on topo cells; absent at jit25) ─────
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
# khr = the legacy SMOOTHED windowed-min K (every arm); kraw = the raw-fed
# read under RWM_HONEST_K, printed "-" when the gate is off. khr − kraw is
# the smoothing bias, in-cell (goal-gate "Honest Inputs" MECHANISM 2).
k_re = re.compile(r"khr=([0-9.]+)/kraw=([0-9.]+|-)")
wait_re = re.compile(
    r"wait\[tun=(\d+)% paused=(\d+)% pace=(\d+)% gen=(\d+)% nack=(\d+)% "
    r"defc=(\d+)% tail=(\d+)% flush=(\d+)% n=(\d+) us=(\d+)\]")
dgq_re = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+) full=(\d+) err=(\d+) sp=(\d+)\]")

occ, occap, qd, nd = [], [], [], 0
khrs, kraws = [], []
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
dgq_out = {
    "dgq_hand": sum(v[0] for v in dgq.values()) or None,
    "dgq_full": sum(v[2] for v in dgq.values()) if dgq else None,
    "dgq_gap": (sum(v[0] - v[1] for v in dgq.values()) if dgq else None),
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
       # THE MECHANISM GAUGE (criterion H2)
       "cpusrv": cpusrv, "cpucli": cpucli,
       # the law readout (criterion H3)
       "tt_eng1": tt_eng, "cap_med": med(caps), "window_med": med(wins),
       "slack_med": med(slacks), "span_med": med(spans),
       # the K decomposition (criterion H3's in-cell instrument)
       "khr_med": med(khrs), "kraw_med": med(kraws),
       "khr_n": len(khrs), "kraw_n": len(kraws),
       # occupancy + engine queue estimate
       "occ_p50": q(occ, 0.5), "occcap_p50": q(occap, 0.5),
       "q_p50": q(qd, 0.5), "q_p99": q(qd, 0.99),
       "diag_lines": nd, "retx": retx}
out.update(gates)
out.update(ping)
out.update(wait_out)
out.update(dgq_out)
out.update(tc)
print("HIRESULT " + json.dumps(out))
