#!/usr/bin/env python3
"""The path-event injector (docs/status.md section 46, harness only: no engine code).

Started with the client by perf_rwm_c.sh (RWM_EVENT=<leg>:<delta_ms>). It polls the client log every 20 ms, takes TRANSFER START as
the wall time at which the warm-up JSON line (`"warmup":true`) appears, steps the hit leg's one-way delay up by <delta_ms> at
start + onset_s (`topo_dual.sh reshape`, a `tc qdisc change` of the FULL netem line), and restores it hold_s after the onset call.
It also stamps the wall time at which every `[DIAG] t=` line appears, so the parser can map the event onto the [DIAG] axis
(offset = median(wall_seen - t_printed); tolerance +-0.15 s).

Output (--out), one line each:
    [EVENT] start wall=<s>
    [EVENT] on  wall_before=<s> wall_after=<s> rc=<n> delay_ms=<n> diag_t=<s>
    [EVENT] off wall_before=<s> wall_after=<s> rc=<n> delay_ms=<n> diag_t=<s>
    [EVENT] done wall=<s>   |   [EVENT] ABORT <reason>
    [DIAGSEEN] t=<s> wall=<s>
`diag_t` is the injector's own estimate of the [DIAG] axis at the call (last stamp + elapsed wall time).
--qdisc-out (check rows): `tc -s qdisc show dev <dev>` (clsact block removed) before the onset, every 100 ms during the hold and
after the restore, as `### before|during|after wall=<s>` blocks.
"""
import argparse
import re
import subprocess
import sys
import time

DIAG_T = re.compile(r"\[DIAG\] t=([\d.]+)s ")
WARMUP = '"warmup":true'


def sh(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True)
    except OSError as e:            # e.g. no `ip` on a host without it: a failed call, never a crash of the injector
        return subprocess.CompletedProcess(cmd, 127, "", str(e))


def qdisc_text(ns, dev):
    r = sh(["ip", "netns", "exec", ns, "tc", "-s", "qdisc", "show", "dev", dev])
    out, skip = [], False
    for ln in r.stdout.splitlines():
        if ln.startswith("qdisc "):
            skip = ln.split()[1] == "clsact"
        if not skip:
            out.append(ln)
    return "\n".join(out)


class Tail:
    """Incremental reader of the client log: complete lines only."""

    def __init__(self, path):
        self.path, self.pos, self.buf = path, 0, b""

    def lines(self):
        try:
            with open(self.path, "rb") as fh:
                fh.seek(self.pos)
                data = fh.read()
        except OSError:
            return []
        self.pos += len(data)
        self.buf += data
        parts = self.buf.split(b"\n")
        self.buf = parts.pop()
        return [p.decode("utf-8", "replace") for p in parts]


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--ns", default="rp-cli")
    ap.add_argument("--dev", required=True)
    ap.add_argument("--scenario", required=True)
    ap.add_argument("--seed", default="")
    ap.add_argument("--delta-ms", type=int, required=True)
    ap.add_argument("--onset-s", type=float, default=5.0)
    ap.add_argument("--hold-s", type=float, default=2.0)
    ap.add_argument("--qdisc-out", default="")
    ap.add_argument("--topo", default="./topo_dual.sh")
    ap.add_argument("--max-wait-s", type=float, default=180.0)
    ap.add_argument("--poll-s", type=float, default=0.02)
    ap.add_argument("--tail-s", type=float, default=5.0, help="keep stamping [DIAG] lines this long after the restore")
    a = ap.parse_args(argv)

    out = open(a.out, "w", buffering=1)
    qout = open(a.qdisc_out, "w", buffering=1) if a.qdisc_out else None
    base = sh(["bash", a.topo, "oneway", a.scenario]).stdout.strip()
    if not base.isdigit():
        out.write(f"[EVENT] ABORT no-base-delay scenario={a.scenario}\n")
        return 3
    base_ms = int(base)
    tail = Tail(a.log)
    last = {"t": None, "wall": None}

    def poll():
        """Read new log lines; stamp the [DIAG] ones. -> True if the warm-up line was among them."""
        seen_warm = False
        for ln in tail.lines():
            now = time.time()
            if WARMUP in ln:
                seen_warm = True
            m = DIAG_T.search(ln)
            if m and started[0] is not None:
                out.write(f"[DIAGSEEN] t={m.group(1)} wall={now:.3f}\n")
                last["t"], last["wall"] = float(m.group(1)), now
        return seen_warm

    started = [None]
    t_enter = time.time()
    while started[0] is None:
        if time.time() - t_enter > a.max_wait_s:
            out.write("[EVENT] ABORT no-warmup-line\n")
            return 3
        # the warm-up line must be stamped at the moment it is seen: poll() reports it, we stamp right after
        if poll():
            started[0] = time.time()
        else:
            time.sleep(a.poll_s)
    out.write(f"[EVENT] start wall={started[0]:.3f}\n")

    def est():
        if last["t"] is None:
            return "-"
        return f"{last['t'] + (time.time() - last['wall']):.2f}"

    def reshape(delay_ms):
        t0 = time.time()
        r = sh(["bash", a.topo, "reshape", a.ns, a.dev, a.scenario, a.seed, str(delay_ms)])
        t1 = time.time()
        return t0, t1, r.returncode

    def idle_until(t_wall, capture=None):
        nxt = 0.0
        while time.time() < t_wall:
            poll()
            if capture and time.time() >= nxt:
                capture()
                nxt = time.time() + 0.1
            time.sleep(min(a.poll_s, max(0.0, t_wall - time.time())))

    def cap(tag):
        if qout:
            qout.write(f"### {tag} wall={time.time():.3f}\n{qdisc_text(a.ns, a.dev)}\n")

    # onset
    onset_wall = started[0] + a.onset_s
    idle_until(onset_wall - 0.05)
    cap("before")
    idle_until(onset_wall)
    t0, t1, rc = reshape(base_ms + a.delta_ms)
    out.write(f"[EVENT] on wall_before={t0:.3f} wall_after={t1:.3f} rc={rc} delay_ms={base_ms + a.delta_ms} diag_t={est()}\n")
    # hold
    idle_until(t0 + a.hold_s, capture=(lambda: cap("during")) if qout else None)
    t2, t3, rc2 = reshape(base_ms)
    out.write(f"[EVENT] off wall_before={t2:.3f} wall_after={t3:.3f} rc={rc2} delay_ms={base_ms} diag_t={est()}\n")
    idle_until(t3 + 0.3)
    cap("after")
    idle_until(t3 + a.tail_s)
    out.write(f"[EVENT] done wall={time.time():.3f}\n")
    return 0 if (rc == 0 and rc2 == 0) else 4


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
