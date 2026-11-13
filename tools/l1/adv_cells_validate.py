#!/usr/bin/env python3
"""Mechanism validation of the adversarial L1 cells (adv_cells.sh; ADR-0068 W5.3), WITHOUT the engine.

Each cell is proved to do what it claims with a tiny UDP prober (stdlib only) run inside the rp-* namespaces:

  sparse   600 echoed probes, 50 ms apart (spacing >> any jitter, so each packet sees netem's marginal delay): the one-way (data
           direction) and round-trip delay distributions -> delay floor, ceiling, median, IQR, std, span.
  dense    10000 echoed probes at 2000 pps (spacing << jitter): out-of-order arrivals in BOTH directions (the cells promise FIFO:
           netem with `rate` schedules each packet no earlier than the previous one) -> must be exactly 0.
  burst    20 trials of 200 back-to-back 1200 B datagrams (>> 100 Mbit/s): how many get through. The through-count is
           (token bucket + refill during the send) / pkt_len + what the cell can HOLD; the held part is the queue depth
           (shal8: 8 packets, drop starts at the 9th queued; policer: 0; deep netem: ~everything is held).
  flood    4 s CBR at 150 Mbit/s: delivered rate (~100 Mbit/s ceiling), standing-queue delay inflation (deep shaper: tens of ms,
           shallow: < 3 ms, policer: ~0), loss ~ the excess, loss-run length (reported, not gated: a full FIFO under CBR overload drops in runs of ~1 too).
  tc       the cells' own counters after the load: the policer's drop count matches the receiver-side loss; the pol100 data netem
           (delay only) dropped 0; the shal8 child netem counts the queue drops.

Packet timestamps are CLOCK_MONOTONIC ns, valid across processes and namespaces of one host, so one-way delay is measured directly.

Split so the verdict is testable off-VM: `measure` produces a JSON of raw numbers (needs the VM), `judge` turns that JSON into PASS/FAIL
lines (pure; fixture-tested by test_adv_cells.py), `run` = measure + judge. `measure --local` runs the same prober over 127.0.0.1 with
no namespaces and no shaping (machinery check only; not judged).

Thresholds are derived below from netem's documented behaviour (uniform jitter in [mu-J, mu+J], clipped at 0; `rate` => FIFO) and are
FIRST-RUN values: a FAIL on the VM is a finding about the cell or the threshold, to be resolved on the evidence, not relaxed silently.
"""
import argparse
import json
import math
import os
import select
import socket
import struct
import subprocess
import sys
import time

NS_CLI, NS_SRV = "rp-cli", "rp-srv"
CLI_IP, SRV_IP = "10.77.0.1", "10.77.0.2"
PORT = 5311

# ---------------------------------------------------------------------------------------------------------------------------------
# Cell expectations: what adv_cells.sh builds, restated as numbers (test_adv_cells.py ties these to the generated tc lines).
#   data_ms/ack_ms = netem mean one-way delay per direction, jit_ms = its jitter (uniform +-), klass = deep | shallow | police.
#   bucket_b = token bucket (tbf burst / police burst), pkt_len = the length the bucket is charged per 1200 B datagram
#   (tbf at egress sees the 14 B Ethernet header too: 1228 + 14; the ingress police sees the IP packet: 1228).
CELLS = {
    "c2ctl": dict(klass="deep", data_ms=5, ack_ms=5, jit_ms=3),
    "jit0": dict(klass="deep", data_ms=20, ack_ms=20, jit_ms=0),
    "jit5": dict(klass="deep", data_ms=20, ack_ms=20, jit_ms=5),
    "jit15": dict(klass="deep", data_ms=20, ack_ms=20, jit_ms=15),
    "jit25": dict(klass="deep", data_ms=20, ack_ms=20, jit_ms=25),
    "shal8": dict(klass="shallow", data_ms=0, ack_ms=10, jit_ms=0, queue_pkts=8, bucket_b=15140, pkt_len=1242),
    "pol100": dict(klass="police", data_ms=5, ack_ms=5, jit_ms=0, bucket_b=16384, pkt_len=1228),
}
RATE_BPUS = 12.5          # 100 Mbit/s in bytes per microsecond
IP_LEN = 1228             # 1200 B payload + 20 IP + 8 UDP
PAYLOAD = 1200

# Probe parameters: FULL (VM) and QUICK (--quick, machinery tests).
FULL = dict(sparse_n=600, sparse_gap_s=0.05, dense_n=10000, dense_pps=2000, burst_trials=20, burst_n=200, burst_gap_s=0.4,
            flood_s=4.0, flood_mbit=150.0)
QUICK = dict(sparse_n=60, sparse_gap_s=0.01, dense_n=2000, dense_pps=2000, burst_trials=5, burst_n=200, burst_gap_s=0.1,
             flood_s=1.5, flood_mbit=150.0)

# ---------------------------------------------------------------------------------------------------------------------------------
# Wire format: kind, phase, trial, seq, send timestamp (ns).
HDR = struct.Struct("!BBHIQ")
K_DATA, K_ECHO, K_END, K_QUIT = 1, 2, 3, 4
P_SPARSE, P_DENSE, P_BURST, P_FLOOD = 1, 2, 3, 4
ECHO_PHASES = (P_SPARSE, P_DENSE)


def pkt(kind, phase, trial, seq, ts, size=64):
    b = HDR.pack(kind, phase, trial, seq, ts)
    return b + b"\0" * max(0, size - len(b))


def now_ns():
    return time.monotonic_ns()


# ---------------------------------------------------------------------------------------------------------------------------------
# Server: records every datagram (phase, trial, seq, ts_send, ts_recv), echoes the echo phases, dumps a phase's records on END.
def serve(bind_ip, port, outdir):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 16 << 20)
    s.bind((bind_ip, port))
    recs = {}
    dumped = set()
    while True:
        try:
            b, addr = s.recvfrom(2048)
        except OSError:
            continue
        t = now_ns()
        if len(b) < HDR.size:
            continue
        kind, phase, trial, seq, ts = HDR.unpack_from(b)
        if kind == K_DATA:
            recs.setdefault(phase, []).append((trial, seq, ts, t))
            if phase in ECHO_PHASES:
                s.sendto(pkt(K_ECHO, phase, trial, seq, ts, 64), addr)
        elif kind == K_END and phase not in dumped:
            dumped.add(phase)
            tmp = os.path.join(outdir, "phase%d.json.tmp" % phase)
            with open(tmp, "w") as fh:
                json.dump(recs.get(phase, []), fh)
            os.replace(tmp, os.path.join(outdir, "phase%d.json" % phase))
        elif kind == K_QUIT:
            return 0


# ---------------------------------------------------------------------------------------------------------------------------------
# Client.
class Client:
    def __init__(self, src, dst, port):
        self.s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.s.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 8 << 20)
        self.s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 8 << 20)
        self.s.bind((src, 0))
        self.dst = (dst, port)
        self.echoes = {P_SPARSE: [], P_DENSE: []}      # (seq, ts_send, ts_recv) in ARRIVAL order

    def drain(self, wait_s=0.0):
        """Read every echo that has arrived (waiting up to wait_s for the first)."""
        end = time.monotonic() + wait_s
        while True:
            left = max(0.0, end - time.monotonic())
            r, _, _ = select.select([self.s], [], [], left)
            if not r:
                return
            try:
                b, _ = self.s.recvfrom(2048)
            except OSError:
                continue
            t = now_ns()
            if len(b) >= HDR.size:
                kind, phase, trial, seq, ts = HDR.unpack_from(b)
                if kind == K_ECHO and phase in self.echoes:
                    self.echoes[phase].append((seq, ts, t))
            if wait_s == 0.0:
                end = 0

    def send(self, kind, phase, trial, seq, size):
        try:
            self.s.sendto(pkt(kind, phase, trial, seq, now_ns(), size), self.dst)
        except OSError:
            pass                                                # a full local qdisc is a drop, not an error

    def paced(self, phase, n, gap_s, size):
        t0 = time.monotonic()
        for i in range(n):
            self.send(K_DATA, phase, 0, i, size)
            nxt = t0 + (i + 1) * gap_s
            while True:
                left = nxt - time.monotonic()
                if left <= 0:
                    break
                self.drain(min(left, 0.002))
        self.drain(0.0)

    def end(self, phase):
        for _ in range(10):
            self.send(K_END, phase, 0, 0, 64)
            time.sleep(0.02)

    def burst(self, trials, n, gap_s):
        out = []
        for tr in range(trials):
            t0 = time.perf_counter_ns()
            for i in range(n):
                self.send(K_DATA, P_BURST, tr, i, PAYLOAD)
            t1 = time.perf_counter_ns()
            out.append(dict(trial=tr, sent=n, t_send_us=(t1 - t0) / 1000.0))
            time.sleep(gap_s)
        return out

    def flood(self, seconds, mbit):
        pps = mbit * 1e6 / 8.0 / IP_LEN
        total = int(seconds * pps)
        t0 = time.perf_counter()
        for i in range(total):
            self.send(K_DATA, P_FLOOD, 0, i, PAYLOAD)
            nxt = t0 + (i + 1) / pps
            while True:
                left = nxt - time.perf_counter()
                if left <= 0:
                    break
                if left > 0.0005:
                    time.sleep(left - 0.0003)
        elapsed = time.perf_counter() - t0
        return dict(sent=total, elapsed_s=elapsed, offered_mbit=total * IP_LEN * 8 / elapsed / 1e6)


def client_run(src, dst, port, P, outdir):
    c = Client(src, dst, port)
    info = {}
    c.paced(P_SPARSE, P["sparse_n"], P["sparse_gap_s"], 200)
    c.drain(0.5)
    c.end(P_SPARSE)
    c.paced(P_DENSE, P["dense_n"], 1.0 / P["dense_pps"], 200)
    c.drain(0.5)
    c.end(P_DENSE)
    info["burst"] = c.burst(P["burst_trials"], P["burst_n"], P["burst_gap_s"])
    time.sleep(0.5)
    c.end(P_BURST)
    info["flood"] = c.flood(P["flood_s"], P["flood_mbit"])
    time.sleep(0.5)
    c.end(P_FLOOD)
    info["echoes"] = {str(k): v for k, v in c.echoes.items()}
    info["params"] = P
    with open(os.path.join(outdir, "client.json"), "w") as fh:
        json.dump(info, fh)
    c.send(K_QUIT, 0, 0, 0, 64)
    return 0


# ---------------------------------------------------------------------------------------------------------------------------------
# Analysis (pure): raw client/server records -> the measurement dict that `judge` reads.
def pct(xs, q):
    if not xs:
        return None
    ys = sorted(xs)
    k = (len(ys) - 1) * q / 100.0
    lo, hi = int(math.floor(k)), int(math.ceil(k))
    return ys[lo] + (ys[hi] - ys[lo]) * (k - lo)


def stats_ms(xs):
    if not xs:
        return None
    m = sum(xs) / len(xs)
    sd = math.sqrt(sum((x - m) ** 2 for x in xs) / len(xs))
    return dict(n=len(xs), min=min(xs), p5=pct(xs, 5), p25=pct(xs, 25), p50=pct(xs, 50), p75=pct(xs, 75), p95=pct(xs, 95),
                p99=pct(xs, 99), max=max(xs), mean=m, std=sd)


def inversions(seqs):
    """Arrivals whose seq is below the highest seq already seen (a reorder count; 0 for a FIFO path with losses)."""
    hi, n = -1, 0
    for q in seqs:
        if q < hi:
            n += 1
        else:
            hi = q
    return n


def loss_runs(received, total):
    """Mean length of runs of consecutive lost seqs in 0..total-1, and the number of runs."""
    got = set(received)
    runs, cur = [], 0
    for q in range(total):
        if q in got:
            if cur:
                runs.append(cur)
            cur = 0
        else:
            cur += 1
    if cur:
        runs.append(cur)
    return (sum(runs) / len(runs) if runs else 0.0), len(runs)


def analyze(info, server, tc=None):
    """info = client.json, server = {phase: [[trial, seq, ts_send, ts_recv], ...]}, tc = captured tc counter texts (or None)."""
    m = {"params": info.get("params")}
    sp = server.get(str(P_SPARSE), [])
    ec = info["echoes"][str(P_SPARSE)]
    m["sparse"] = dict(sent=info["params"]["sparse_n"], recv=len(sp), echo=len(ec),
                       owd_ms=stats_ms([(r[3] - r[2]) / 1e6 for r in sp]), rtt_ms=stats_ms([(e[2] - e[1]) / 1e6 for e in ec]))
    dn = server.get(str(P_DENSE), [])
    de = info["echoes"][str(P_DENSE)]
    m["dense"] = dict(sent=info["params"]["dense_n"], recv=len(dn), echo=len(de),
                      data_inversions=inversions([r[1] for r in dn]), ack_inversions=inversions([e[0] for e in de]))
    bu = server.get(str(P_BURST), [])
    per = {}
    for r in bu:
        per.setdefault(r[0], []).append(r[1])
    trials = []
    for b in info["burst"]:
        got = sorted(per.get(b["trial"], []))
        gotset = set(got)
        first_drop = next((i + 1 for i in range(b["sent"]) if i not in gotset), None)
        trials.append(dict(sent=b["sent"], delivered=len(got), t_send_us=b["t_send_us"], first_drop_idx=first_drop,
                           send_mbit=b["sent"] * IP_LEN * 8 / b["t_send_us"] if b["t_send_us"] > 0 else None))
    m["burst"] = dict(trials=trials)
    fl = server.get(str(P_FLOOD), [])
    f = info["flood"]
    recv_seq = [r[1] for r in fl]
    t0 = min((r[2] for r in fl), default=0)
    steady = [(r[3] - r[2]) / 1e6 for r in fl if (r[2] - t0) > 1e9]          # after the first second (deep queues fill in ~0.2 s)
    rl, nruns = loss_runs(recv_seq, f["sent"])
    m["flood"] = dict(sent=f["sent"], recv=len(fl), offered_mbit=f["offered_mbit"],
                      delivered_mbit=len(fl) * IP_LEN * 8 / f["elapsed_s"] / 1e6, loss=1.0 - len(fl) / float(f["sent"]),
                      owd_ms=stats_ms(steady), loss_run_mean=rl, loss_runs=nruns)
    m["tc"] = tc
    return m


# Counter parsing (tc -s text; fixture-tested).
def qdisc_blocks(text):
    """{kind: (dropped, sent_pkts)} of the `qdisc <kind> ...` blocks of `tc -s qdisc show` text (last wins for a repeated kind)."""
    import re
    out = {}
    for blk in re.split(r"(?m)^(?=qdisc )", text or ""):
        m = re.match(r"qdisc (\S+) ", blk)
        s = re.search(r"Sent (\d+) bytes (\d+) pkt \(dropped (\d+)", blk)
        if m and s:
            out[m.group(1)] = (int(s.group(3)), int(s.group(2)))
    return out


def police_dropped(text):
    """Dropped count of the police action in `tc -s filter show` text, or None."""
    import re
    m = re.search(r"Action statistics:\s*Sent \d+ bytes \d+ pkt \(dropped (\d+)", text or "")
    if not m:
        m = re.search(r"police .*?\n.*?Sent \d+ bytes \d+ pkt \(dropped (\d+)", text or "", re.S)
    return int(m.group(1)) if m else None


# ---------------------------------------------------------------------------------------------------------------------------------
# Judge (pure). Returns [(name, ok, detail)].
def judge(cell, m):
    sp = CELLS[cell]
    res = []

    def chk(name, ok, detail):
        res.append((name, bool(ok), detail))

    mu_d, mu_a, J = sp["data_ms"], sp["ack_ms"], sp["jit_ms"]
    flo = max(0.0, mu_d - J)
    S = m["sparse"]
    o = S["owd_ms"]
    chk("sparse_delivery", S["recv"] >= 0.85 * S["sent"] and S["echo"] >= 0.8 * S["sent"] and o is not None,
        "recv %d echo %d of %d (GE loss ~2.5%% on the data direction of jitter/shal8 cells)" % (S["recv"], S["echo"], S["sent"]))
    if o is None:
        return res
    # Delay floor / distribution (data direction, one-way). Uniform [mu-J, mu+J] clipped at 0; min of n samples sits ~2J/n above.
    chk("owd_floor", flo - 0.5 <= o["min"] <= flo + 0.1 * J + 1.5, "min %.2f ms, want [%.2f, %.2f]" % (o["min"], flo - 0.5, flo + 0.1 * J + 1.5))
    chk("owd_ceiling", o["max"] <= mu_d + J + 2.0, "max %.2f ms, want <= %.2f" % (o["max"], mu_d + J + 2.0))
    chk("owd_median", abs(o["p50"] - mu_d) <= 0.25 * J + 1.0, "p50 %.2f ms, want %.2f +- %.2f" % (o["p50"], mu_d, 0.25 * J + 1.0))
    iqr = o["p75"] - o["p25"]
    if J == 0:
        chk("owd_iqr", iqr <= 1.0, "IQR %.2f ms, want <= 1.0 (no jitter)" % iqr)
        chk("owd_std", o["std"] <= 0.5, "std %.2f ms, want <= 0.5 (no jitter)" % o["std"])
    else:
        chk("owd_iqr", 0.5 * J <= iqr <= 1.5 * J, "IQR %.2f ms, want [%.2f, %.2f] (uniform: J)" % (iqr, 0.5 * J, 1.5 * J))
        chk("owd_std", 0.35 * J <= o["std"] <= 0.8 * J, "std %.2f ms, want [%.2f, %.2f] (uniform: 0.577 J)" % (o["std"], 0.35 * J, 0.8 * J))
        span = o["p95"] - o["p5"]
        chk("owd_span", span >= 1.2 * J, "p95-p5 %.2f ms, want >= %.2f (uniform: 1.8 J)" % (span, 1.2 * J))
    r = S["rtt_ms"]
    rfloor = flo + max(0.0, mu_a - J)
    if r is None:
        chk("rtt_floor", False, "no echoes")
    else:
        chk("rtt_floor", rfloor - 0.5 <= r["min"] <= rfloor + 0.3 * J + 2.0,
            "RTT min %.2f ms, want [%.2f, %.2f]" % (r["min"], rfloor - 0.5, rfloor + 0.3 * J + 2.0))
    # FIFO: jitter must not reorder, in either direction.
    D = m["dense"]
    chk("dense_delivery", D["recv"] >= 0.9 * D["sent"] and D["echo"] >= 0.85 * D["sent"],
        "recv %d echo %d of %d" % (D["recv"], D["echo"], D["sent"]))
    chk("no_reorder_data", D["data_inversions"] == 0, "%d out-of-order arrivals of %d (dense, spacing 0.5 ms << jitter)" % (D["data_inversions"], D["recv"]))
    chk("no_reorder_ack", D["ack_inversions"] == 0, "%d out-of-order echoes of %d" % (D["ack_inversions"], D["echo"]))
    # Burst: queue depth.
    T = m["burst"]["trials"]
    fr = sorted(t["delivered"] / float(t["sent"]) for t in T)
    frac = pct(fr, 50)
    kl = sp["klass"]
    if kl == "deep":
        chk("queue_deep", frac >= 0.9, "median %.0f%% of a 200-packet burst delivered, want >= 90%% (deep queue holds it)" % (100 * frac))
    else:
        rate = pct([t["send_mbit"] for t in T], 50)
        chk("burst_overdrives", rate >= 300.0, "burst sent at %.0f Mbit/s, want >= 300 (3x the ceiling; slower proves nothing)" % rate)
        # mean over trials: D is an integer, so one trial is quantised by up to 1 packet; the estimator's own bias is -0.3..-1.7 packet
        # (arrival granularity, a queue one short at the burst's end), checked against a token-bucket simulation in test_adv_cells.py.
        # Resolution is therefore ~+-2 packets: it separates an 8-packet queue from 0 (policer) and from 12+, not 8 from 9.
        hs = [t["delivered"] - (sp["bucket_b"] + RATE_BPUS * t["t_send_us"]) / sp["pkt_len"] for t in T]
        held = sum(hs) / len(hs)
        fd = pct([t["first_drop_idx"] or t["sent"] + 1 for t in T], 50)
        if kl == "shallow":
            want = sp["queue_pkts"]
            chk("queue_depth", abs(held - want) <= 2.0,
                "held %.1f pkts (delivered - bucket - refill), want %d +- 2; delivered %.0f%% (median first-gap idx %.0f)" % (held, want, 100 * frac, fd))
        else:
            chk("queue_depth", abs(held) <= 2.5,
                "held %.1f pkts, want 0 +- 2.5 (a policer holds nothing); delivered %.0f%% (median first-gap idx %.0f)" % (held, 100 * frac, fd))
    # Flood: ceiling, queueing, loss.
    F = m["flood"]
    chk("flood_overdrives", F["offered_mbit"] >= 130.0, "offered %.0f Mbit/s (IP), want >= 130" % F["offered_mbit"])
    chk("flood_ceiling", 88.0 <= F["delivered_mbit"] <= 104.0, "delivered %.1f Mbit/s (IP), want [88, 104]" % F["delivered_mbit"])
    fo = F["owd_ms"]
    if fo is None:
        chk("flood_delay", False, "no steady-state packets")
    else:
        infl = fo["p50"] - o["p50"]
        if kl == "deep":
            chk("flood_delay", infl >= 40.0, "p50 delay inflation %.1f ms under overload, want >= 40 (a ~1000-packet standing queue: ~99 ms)" % infl)
        elif kl == "shallow":
            chk("flood_delay", infl <= 3.0, "p50 delay inflation %.2f ms, want <= 3.0 (8 packets = 0.8 ms)" % infl)
        else:
            chk("flood_delay", infl <= 2.0, "p50 delay inflation %.2f ms, want <= 2.0 (a policer never queues)" % infl)
    if kl == "police":
        exp = 1.0 - 100.0 / F["offered_mbit"]
        chk("police_loss_rate", abs(F["loss"] - exp) <= 0.05, "loss %.1f%%, want %.1f%% +- 5 (excess over the 100 Mbit/s bucket rate)" % (100 * F["loss"], 100 * exp))
    # The cells' own counters.
    tc = m.get("tc") or {}
    if kl == "police":
        pd = police_dropped(tc.get("srv0_ingress"))
        lost = F["sent"] - F["recv"]
        chk("police_counter", pd is not None and pd >= 0.8 * lost and pd >= 1,
            "police dropped %s, receiver-side flood loss %d (the policer, not anything else, accounts for the loss)" % (pd, lost))
        nd = qdisc_blocks(tc.get("cli0")).get("netem")
        chk("data_netem_never_drops", nd is not None and nd[0] == 0, "cli0 netem dropped %s, want 0 (delay-only stage)" % (nd[0] if nd else None))
    elif kl == "shallow":
        nd = qdisc_blocks(tc.get("cli0")).get("netem")
        chk("queue_drop_counter", nd is not None and nd[0] >= 1, "cli0 child netem (limit 8) dropped %s, want >= 1 (queue overflow counted)" % (nd[0] if nd else None))
    return res


def report(cell, res):
    nfail = sum(1 for r in res if not r[1])
    lines = ["VALCHECK %s %s %s -- %s" % (cell, n, "PASS" if ok else "FAIL", d) for n, ok, d in res]
    lines.append("VALCELL-RESULT %s %s (%d checks, %d failed)" % (cell, "FAIL" if nfail else "PASS", len(res), nfail))
    return lines, nfail


# ---------------------------------------------------------------------------------------------------------------------------------
# measure: spawn server + client (in the namespaces, or locally), then collect the counters.
def sh(cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout


# Everything this tool runs outside Python, as argv: always `ip netns exec rp-*`, read-only `tc ... show`, never a write
# (test_adv_cells.py scans these for the safety rule).
TC_CMDS = dict(
    cli0=["ip", "netns", "exec", NS_CLI, "tc", "-s", "qdisc", "show", "dev", "cli0"],
    srv0=["ip", "netns", "exec", NS_SRV, "tc", "-s", "qdisc", "show", "dev", "srv0"],
    srv0_ingress=["ip", "netns", "exec", NS_SRV, "tc", "-s", "filter", "show", "dev", "srv0", "parent", "ffff:"])


def ns_prefixes(local):
    """(server prefix, client prefix, server ip, client ip): the probe processes run inside rp-srv / rp-cli, or locally for --local."""
    if local:
        return [], [], "127.0.0.1", "127.0.0.1"
    return ["ip", "netns", "exec", NS_SRV], ["ip", "netns", "exec", NS_CLI], SRV_IP, CLI_IP


def measure(cell, out, local, quick, outdir):
    P = QUICK if quick else FULL
    os.makedirs(outdir, exist_ok=True)
    for f in os.listdir(outdir):
        if f.startswith("phase") or f == "client.json":
            os.remove(os.path.join(outdir, f))
    me = os.path.abspath(__file__)
    pre_s, pre_c, sip, cip = ns_prefixes(local)
    srv = subprocess.Popen(pre_s + [sys.executable, me, "serve", "--bind", sip, "--port", str(PORT), "--outdir", outdir])
    time.sleep(0.5)
    try:
        rc = subprocess.run(pre_c + [sys.executable, me, "client", "--src", cip, "--dst", sip, "--port", str(PORT), "--outdir", outdir,
                                     "--params", json.dumps(P)]).returncode
        if rc != 0:
            print("client failed rc=%d" % rc, file=sys.stderr)
            return 2
        for _ in range(100):
            if all(os.path.exists(os.path.join(outdir, "phase%d.json" % p)) for p in (1, 2, 3, 4)):
                break
            time.sleep(0.1)
    finally:
        try:
            srv.wait(timeout=3)
        except subprocess.TimeoutExpired:
            srv.kill()
    info = json.load(open(os.path.join(outdir, "client.json")))
    server = {}
    for p in (1, 2, 3, 4):
        fn = os.path.join(outdir, "phase%d.json" % p)
        server[str(p)] = json.load(open(fn)) if os.path.exists(fn) else []
    tc = None if local else {k: sh(v) for k, v in TC_CMDS.items()}
    m = analyze(info, server, tc)
    m["cell"] = cell
    with open(out, "w") as fh:
        json.dump(m, fh, indent=1)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("serve")
    a.add_argument("--bind", required=True)
    a.add_argument("--port", type=int, default=PORT)
    a.add_argument("--outdir", required=True)
    a = sub.add_parser("client")
    a.add_argument("--src", required=True)
    a.add_argument("--dst", required=True)
    a.add_argument("--port", type=int, default=PORT)
    a.add_argument("--outdir", required=True)
    a.add_argument("--params", required=True)
    a = sub.add_parser("measure")
    a.add_argument("cell", choices=sorted(CELLS))
    a.add_argument("--out", required=True)
    a.add_argument("--local", action="store_true")
    a.add_argument("--quick", action="store_true")
    a.add_argument("--outdir", default="/tmp/adv-val")
    a = sub.add_parser("judge")
    a.add_argument("cell", choices=sorted(CELLS))
    a.add_argument("measurement")
    a = sub.add_parser("run")
    a.add_argument("cell", choices=sorted(CELLS))
    a.add_argument("--out", default="/tmp/adv-measure.json")
    a.add_argument("--fixture", default="", help="judge this measurement JSON instead of measuring (stub/dry-run)")
    a.add_argument("--outdir", default="/tmp/adv-val")
    n = ap.parse_args(argv)
    if n.cmd == "serve":
        return serve(n.bind, n.port, n.outdir)
    if n.cmd == "client":
        return client_run(n.src, n.dst, n.port, json.loads(n.params), n.outdir)
    if n.cmd == "measure":
        return measure(n.cell, n.out, n.local, n.quick, n.outdir)
    if n.cmd == "run":
        if n.fixture:
            n.out = n.fixture
        else:
            rc = measure(n.cell, n.out, False, False, n.outdir)
            if rc:
                return rc
        n.measurement = n.out
        n.cmd = "judge"
    m = json.load(open(n.measurement))
    lines, nfail = report(n.cell, judge(n.cell, m))
    print("\n".join(lines))
    return 1 if nfail else 0


if __name__ == "__main__":
    sys.exit(main())
