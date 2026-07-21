#!/usr/bin/env python3
"""Queue/RTT distributions for the copaclean battery (goal-gate "Copa-Sole
on Clean Substrate"). Pools the per-path DIAG clock fields across every
run of an arm (steady state = per-run DIAG blocks 4+, the #80/#82
convention) and prints per path:

  rtp p50        (RTprop floor — freshness check)
  appQ p50/p90   (app-echo rtt - rtp: the consumer-experienced pipeline,
                  includes the sender's own reservoir)
  wireQ p50/p90  (quinn packet-timed wrtt - rtp: the NETWORK standing queue)

usage: copaclean_queues.py <diag_dir> <seed>
"""
import glob
import os
import re
import sys

PP = re.compile(r" p(\d+):\S*?[^|]*?rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def pct(v, q):
    if not v:
        return float("nan")
    s = sorted(v)
    i = min(len(s) - 1, max(0, int(round(q * (len(s) - 1)))))
    return s[i]


def main(ddir, seed):
    arms = {}
    for f in sorted(glob.glob(os.path.join(ddir, f"*-s{seed}-r*-c.log"))):
        arm = os.path.basename(f).split(f"-s{seed}-")[0]
        # per-run: collect DIAG blocks, skip the first 3 (warmup)
        blocks = []
        with open(f, errors="replace") as fh:
            for line in fh:
                line = ANSI.sub("", line)
                if "[DIAG]" not in line:
                    continue
                blocks.append(PP.findall(line))
        for blk in blocks[3:]:
            for pid, rtt, wrtt, rtp in blk:
                d = arms.setdefault(arm, {}).setdefault(int(pid), {"rtt": [], "wrtt": [], "rtp": []})
                d["rtt"].append(int(rtt))
                d["wrtt"].append(int(wrtt))
                d["rtp"].append(int(rtp))
    for arm in sorted(arms):
        for pid in sorted(arms[arm]):
            d = arms[arm][pid]
            rtp50 = pct(d["rtp"], 0.5)
            app = [r - p for r, p in zip(d["rtt"], d["rtp"])]
            wire = [w - p for w, p in zip(d["wrtt"], d["rtp"])]
            print(
                f"QDIST s{seed} {arm} p{pid}: n={len(d['rtt'])} rtp_p50={rtp50:.0f} "
                f"appQ p50={pct(app, 0.5):.0f} p90={pct(app, 0.9):.0f} | "
                f"wireQ p50={pct(wire, 0.5):.0f} p90={pct(wire, 0.9):.0f}"
            )


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
