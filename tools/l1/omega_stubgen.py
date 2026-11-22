#!/usr/bin/env python3
"""Synthetic `tcpdump -nn -tt -v -r` text and R0 probe logs for test_omega_stubrun.py (the stubbed `tcpdump` and the stubbed probe call this).

  omega_stubgen.py pcap <file.pcap>                    print the text a capture named like a harness row / R0 probe / ip-id run would hold
  omega_stubgen.py probe <dst> <rate> <secs> <log>     write an R0 send log (no raw socket needed)

The text is derived from the row's own params.json / t_event / backlog.txt (written by omega_battery.sh), so the REAL digest, witnesses and
scorer run on it. Knobs (environment): STUB_REL_NO_EVENTS, STUB_HOLD_REORDERS, STUB_R0_BAD, STUB_IPID_STEP, STUB_EV_S.
This is a test fixture, not part of the measurement.
"""
import json
import os
import sys
import time


def fmt(t, f, seq, ipid, src="10.77.1.1", dst="10.77.2.2", dport=5201):
    return ("%.6f IP (tos 0x0, ttl 64, id %d, offset 0, flags [DF], proto TCP (6), length 1500)\n"
            "    %s.%d > %s.%d: Flags [.], seq %d:%d, ack 1, win 502, length 1448\n" % (t, ipid, src, 40000 + f, dst, dport, seq, seq + 1448))


def fmt_udp(t, ipid):
    return ("%.6f IP (tos 0x0, ttl 64, id %d, offset 0, flags [none], proto UDP (17), length 128)\n"
            "    10.77.1.2.40000 > 10.77.2.2.9: [udp sum ok] UDP, length 100\n" % (t, ipid))


def hold_posts(times, sel, dt, g, base=0.0001):
    """Post-reorderer arrival times through a FIFO that holds the head of each selected packet dt, then drains the run at 10 g until it catches up."""
    out, rel = [], None
    for i, t in enumerate(times):
        if i in sel and (rel is None or t + base >= rel):
            rel = t + dt
            out.append(rel + base)
        elif rel is not None and t + base < rel + 1.0 / (10.0 * g):
            rel += 1.0 / (10.0 * g)
            out.append(rel + base)
        else:
            rel = None
            out.append(t + base)
    return out


def row_capture(path):
    d = os.path.dirname(path)
    P = json.load(open(os.path.join(d, "params.json")))
    side = "pre" if os.path.basename(path).startswith("pre") else "post"
    try:
        t_ev = float(open(os.path.join(d, "t_event")).read())
    except OSError:
        t_ev = time.time()
    g, kind, dt = P["g"], P["kind"], P["dt_ms"] / 1000.0
    t0, span = t_ev - 0.1, float(os.environ.get("STUB_EV_S", "0.5"))
    n = int(span * g)
    ev_idx = [int((t_ev + off - t0) * g) for off in (0.05, 0.25)] if kind in ("REL", "HOLD") else []
    if kind == "REL" and os.environ.get("STUB_REL_NO_EVENTS"):
        ev_idx = []
    out = []
    flows = {}
    times = [t0 + i / g for i in range(n)]
    posts = hold_posts(times, set(ev_idx), dt, g) if kind == "HOLD" else [t + 0.0001 + (dt if (kind == "REL" and i in ev_idx) else 0.0) for i, t in enumerate(times)]
    if kind == "HOLD" and os.environ.get("STUB_HOLD_REORDERS") and ev_idx:
        posts[ev_idx[0] + 1] = times[ev_idx[0] + 1] + 0.0001      # one successor is not held: it overtakes the head
    for i in range(n):
        f = i % 8
        seq = flows.get(f, 0)
        flows[f] = seq + 1448
        out.append(((times[i] if side == "pre" else posts[i]), f, seq, i + 10))
    if side == "post":
        out.sort()
    return "".join(fmt(*x) for x in out)


def r0_capture(path):
    base = os.path.basename(path)[:-5]
    g, mode, k = base.split("-")
    g, k = int(g), int(k)
    dt = (k + 0.5) / g
    sent = [l.split() for l in open(path[:-5] + ".send") if l.strip()]
    e0 = float(sent[0][0])
    period = max(1, int(round(g * 0.1)))
    bad = os.environ.get("STUB_R0_BAD")
    sel = {int(i): float(t) for i, (t, ident) in enumerate(sent) if mode != "CLEAN" and i % period == period // 2 and float(t) > e0 + 0.05}
    times = [float(t) for t, _ in sent]
    if mode == "HOLD":
        pt = hold_posts(times, set(sel), dt, g)
    else:
        pt = [t + 0.0001 + (dt * (0.4 if bad else 1.0) if (mode == "REL" and i in sel) else 0.0) for i, t in enumerate(times)]
    posts = sorted((pt[i], int(ident)) for i, (_, ident) in enumerate(sent))
    return "".join(fmt_udp(t, ident) for t, ident in posts)


def ipid_capture(path):
    step = int(os.environ.get("STUB_IPID_STEP", "1"))
    t0 = time.time()
    return "".join(fmt(t0 + i * 0.001, 0, i * 1448, (100 + i * step) % 65536) for i in range(200))


def main(a):
    if len(a) == 3 and a[1] == "pcap":
        p = a[2]
        b = os.path.basename(p)
        if b == "ipid.pcap":
            sys.stdout.write(ipid_capture(p))
        elif b in ("pre.pcap", "post.pcap"):
            sys.stdout.write(row_capture(p))
        elif b.endswith(".pcap") and os.path.exists(p[:-5] + ".send"):
            sys.stdout.write(r0_capture(p))
        return 0
    if len(a) == 6 and a[1] == "probe":
        rate, secs = float(a[3]), float(a[4])
        e0 = time.time()
        with open(a[5], "w") as f:
            for i in range(int(rate * secs)):
                f.write("%.6f %d\n" % (e0 + i / rate, i & 0xFFFF))
        return 0
    return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv))
