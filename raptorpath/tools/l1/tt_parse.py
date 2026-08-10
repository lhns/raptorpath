#!/usr/bin/env python3
"""Three-Term Law battery: ONE parser for every arm's client/server logs.

Shared by `tt_battery.sh` (topo.sh cells) and `tt_adv.sh` (adv_cells.sh
cells) so there is exactly one definition of every number that reaches the
ledger — the `honest_cap_term` de-triplication lesson applied to the
harness.

Emits one `TTRESULT {json}` line. Fields:

  goodput      mbps / seconds / dnf, from the client's `"summary"` json
  liveness     the [GATES] resolved values on BOTH endpoints (discipline
               15c: an arm shows the gate ON and the control shows the SAME
               gate OFF, on client AND server), the resolve-time ACTIVE
               echo, and the per-2s `[3T] eng=` state.
  law          median window / slack / span / cap over the `[3T]` lines
               with eng=1 — the terms the pre-registration's table predicts.
               `span` is the number the topology claim stands on: it must
               read 0.0 at every single-path cell and at c7.
  occupancy    p50/p90 of `win=<outstanding>/<cap>` over steady-state
               [DIAG] lines (line 4+, the pooling rule) — the MEASURED
               store occupancy criterion 3 compares against the limit.

usage: tt_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log>
"""
import json
import re
import sys


def med(v):
    if not v:
        return None
    v = sorted(v)
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2.0


def q(v, p):
    if not v:
        return None
    v = sorted(v)
    return v[min(len(v) - 1, int(round(p * (len(v) - 1))))]


def read(path):
    try:
        with open(path, errors="replace") as f:
            return [re.sub(r"\x1b\[[0-9;]*m", "", ln) for ln in f]
    except OSError:
        return []


cell, arm, seed, rep, clog, slog = sys.argv[1:7]
cli = read(clog)
srv = read(slog)

# ── goodput ──────────────────────────────────────────────────────────────
# Per-run lines carry `"run": N` with mbps/seconds (or `"dnf": true`); the
# trailing `"summary": true` line carries the dnf COUNT. A dnf is a datum
# (criterion "dnf = 0"), so it is recorded, never dropped.
mbps = secs = None
dnf = False
dnf_count = None
runs = []
for ln in cli:
    s = ln.strip()
    i = s.find("{")
    if i < 0:
        continue
    try:
        j = json.loads(s[i:])
    except Exception:
        continue
    if j.get("summary"):
        dnf_count = j.get("dnf")
        if dnf_count:
            dnf = True
        continue
    if j.get("run") is not None:
        if j.get("dnf"):
            dnf = True
        else:
            runs.append((j.get("mbps"), j.get("seconds")))
if runs:
    mbps, secs = runs[-1]
if not runs and dnf_count is None:
    dnf = True                            # no summary at all = aborted

# ── liveness: [GATES] resolved values, both endpoints ────────────────────
gate_re = re.compile(r"RWM_THREE_TERM=(\d)")
prs_re = re.compile(r"RWM_PLAIN_RS=(\d)")


def gates(lines):
    tt = prs = None
    n = 0
    for ln in lines:
        if "[GATES]" not in ln:
            continue
        n += 1
        m = gate_re.search(ln)
        if m:
            tt = int(m.group(1))
        m = prs_re.search(ln)
        if m:
            prs = int(m.group(1))
    return tt, prs, n


cli_tt, cli_prs, cli_gn = gates(cli)
srv_tt, srv_prs, srv_gn = gates(srv)

active_cli = sum(1 for ln in cli if "three-term outstanding limit ACTIVE" in ln)
active_srv = sum(1 for ln in srv if "three-term outstanding limit ACTIVE" in ln)
sampler = sum(1 for ln in cli if "send-interval SAMPLER ACTIVE" in ln)

# ── the law's own readout: `[3T] ... eng= cap= window= slack= span= ...` ──
tt_re = re.compile(
    r"eng=(\d+)\s+cap=(\d+)\s+window=([0-9.eE+-]+)\s+slack=([0-9.eE+-]+)"
    r"\s+span=([0-9.eE+-]+)\s+rho=([0-9.eE+-]+)\s+b=([0-9.eE+-]+)"
)
tt_n = tt_eng = 0
caps, wins, slacks, spans, swr = [], [], [], [], []
rhos, bs = set(), set()
for ln in cli:
    if "[3T]" not in ln:
        continue
    m = tt_re.search(ln)
    if not m:
        tt_n += 1
        continue
    tt_n += 1
    eng = int(m.group(1))
    if eng != 1:
        continue
    tt_eng += 1
    caps.append(int(m.group(2)))
    wins.append(float(m.group(3)))
    slacks.append(float(m.group(4)))
    spans.append(float(m.group(5)))
    rhos.add(float(m.group(6)))
    bs.add(float(m.group(7)))
    # THE F2 FALSIFIER, in the only form that is actually δ-free.
    # At ρ = 1 the whole `(1 − ρ)·D(δ)` term is multiplied by zero, so
    #     slack/window = ρ·(9/8·srtt + srtt) / (K·RTprop),  srtt = K·RTprop
    #                  = 17/8 = 2.125   EXACTLY, whatever δ is.
    # Comparing `cap` between hints CANNOT test F2 — the rate anchor differs
    # between invocations, and the limit is linear in it, so cap moves for a
    # reason that has nothing to do with δ. This ratio holds the rate fixed
    # by construction and is the number the structural claim lives on.
    if float(m.group(3)) > 0:
        swr.append(float(m.group(4)) / float(m.group(3)))

# ── occupancy: win=<outstanding>/<cap> over steady-state [DIAG] lines ─────
occ_re = re.compile(r"win=(\d+)/(\d+)")
retx_re = re.compile(r"retx=(\d+)")
occ, occap = [], []
retx = 0
nd = 0
for ln in cli:
    if "[DIAG]" not in ln:
        continue
    nd += 1
    m = occ_re.search(ln)
    if m and nd >= 4:                     # steady state (the pooling rule)
        occ.append(int(m.group(1)))
        occap.append(int(m.group(2)))
    m = retx_re.search(ln)
    if m:
        retx = max(retx, int(m.group(1)))

print("TTRESULT " + json.dumps({
    "cell": cell, "arm": arm, "seed": int(seed), "rep": int(rep),
    "dnf": dnf, "dnf_count": dnf_count, "mbps": mbps, "seconds": secs,
    "n_runs": len(runs),
    # discipline 15c — two-sided, both endpoints
    "gates_cli_3t": cli_tt, "gates_srv_3t": srv_tt,
    "gates_cli_rs": cli_prs, "gates_srv_rs": srv_prs,
    "gates_lines_cli": cli_gn, "gates_lines_srv": srv_gn,
    "active_echo_cli": active_cli, "active_echo_srv": active_srv,
    "sampler_echo": sampler,
    # discipline 15 — the `[3T] eng=1` liveness echo
    "tt_lines": tt_n, "tt_eng1": tt_eng,
    "cap_med": med(caps), "window_med": med(wins),
    "slack_med": med(slacks), "span_med": med(spans),
    "span_max": max(spans) if spans else None,
    "sw_ratio_med": med(swr), "sw_ratio_min": min(swr) if swr else None,
    "sw_ratio_max": max(swr) if swr else None,
    "rho": sorted(rhos), "b": sorted(bs),
    # measured store occupancy
    "occ_p50": q(occ, 0.5), "occ_p90": q(occ, 0.9), "occ_max": max(occ) if occ else None,
    "occcap_p50": q(occap, 0.5), "diag_lines": nd, "retx": retx,
}))
