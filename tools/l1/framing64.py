#!/usr/bin/env python3
"""Status 64: the framing bench's reading (registration 3.1 item 1). The rehearsal runs `taskset -c 2 cargo test -p raptorpath --lib --release framing_ns -- --ignored --nocapture` ten times on CAND's tree
into bench/framing_ns.txt; each run prints, per point, the BASE median [min-max] and the CAND median [min-max] of the bench's alternating pairs (ns/call).

    framing64.py <framing_ns.txt>

Per point: the median over the runs of (CAND median - BASE median), with the min-max of those differences beside it (not the worst difference: a maximum grows with the number of runs).
The bound (formula first, registration 3.1):

    dt_point * N_GB  <=  f * q95_side * C_side

    dt_point  CAND - BASE in ns/call (this reading)          N_GB   10^9 / 1182 source symbols per GB (perf.rs chunk_payload_len: 1196 - 14)
    C_side    cpu_cli/GB 15.35 s (sender: frame), cpu_srv/GB 17.86 s (receiver: read)    (section 54 launch 1, c1s BASE medians)
    q95_side  0.0123 (cli), 0.0173 (srv)   (section 54 launch 1, c1s, q95 of |x| over 400 half-per-seed splits of BASE alone)           f  0.1 (declared margin)

so frame <= +22.3 and read <= +36.5 ns/call at each S = 1200 point; the packed point (S = 512, 3 x 160) is reported only. Label: `BENCH PASS` iff the median is within the bound at all six S = 1200 points,
else `BENCH OVER <points>` with the cost each point implies (dt * N_GB in ms/GB and as a % of C_side). The frame point over-reads (the cpuprof seam main already pays): an upper bound.
The label is a merge condition (PASS, or the owner accepts the printed cost); it does not block the launch."""
import re
import sys

N_GB = 1e9 / 1182
F = 0.1
SIDE = {"frame": {"C": 15.35, "q95": 0.0123}, "read": {"C": 17.86, "q95": 0.0173}}
BOUND = {k: F * v["q95"] * v["C"] / N_GB * 1e9 for k, v in SIDE.items()}      # ns per call: 22.3 (frame), 36.5 (read)
_PT = re.compile(r"(frame|read)\s+(S=\d+)\s+(.*?)\s+BASE\s+([\d.]+)\s*\[([\d.]+)-([\d.]+)\]\s+CAND\s+([\d.]+)\s*\[([\d.]+)-([\d.]+)\]\s+ns/call")


def parse(text):
    """-> [ {point: (base_median, cand_median)} per run ]: a new run starts when a point repeats."""
    runs, cur = [], {}
    for ln in text.splitlines():
        m = _PT.search(ln)
        if not m:
            continue
        key = f"{m.group(1)} {m.group(2)} {m.group(3).strip()}"
        if key in cur:
            runs.append(cur)
            cur = {}
        cur[key] = (float(m.group(4)), float(m.group(7)))
    if cur:
        runs.append(cur)
    return runs


def _med(v):
    v = sorted(v)
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def reading(runs):
    """-> [(point, median diff, min diff, max diff, n runs, bound or None)] in the bench's point order."""
    keys = []
    for r in runs:
        for k in r:
            if k not in keys:
                keys.append(k)
    out = []
    for k in keys:
        d = [r[k][1] - r[k][0] for r in runs if k in r]
        side = k.split()[0]
        scored = "S=1200" in k
        out.append((k, _med(d), min(d), max(d), len(d), BOUND[side] if scored else None))
    return out


def report(text, out=print):
    runs = parse(text)
    if not runs:
        out("BENCH64 NO-DATA (no bench point parsed)")
        out("BENCH NO-DATA")
        return None
    rd = reading(runs)
    out(f"BENCH64 framing_ns: {len(runs)} runs; per point the median over the runs of (CAND median - BASE median) [min-max of the differences], ns/call; bound frame <= +{BOUND['frame']:.1f}, read <= +{BOUND['read']:.1f} "
        f"at each S = 1200 point (N_GB = {N_GB:.4g})")
    over = []
    for k, m, lo, hi, n, b in rd:
        side = k.split()[0]
        cost = m * N_GB / 1e6                                  # ms per GB
        pct = 100.0 * (cost / 1e3) / SIDE[side]["C"]
        if b is None:
            out(f"  {k:<28} {m:+7.1f} [{lo:+.1f}..{hi:+.1f}] n={n}  (reported only: the packed point has no CPU clause)")
        else:
            ok = m <= b
            out(f"  {k:<28} {m:+7.1f} [{lo:+.1f}..{hi:+.1f}] n={n}  bound {b:+.1f}  {'within' if ok else 'OVER'}  implies {cost:+.1f} ms/GB = {pct:+.2f}% of C_{'cli' if side == 'frame' else 'srv'} {SIDE[side]['C']} s/GB")
            if not ok:
                over.append((k, cost, pct))
    if over:
        label = "BENCH OVER " + "; ".join(f"{k} (+{c:.1f} ms/GB, {p:.2f}% of C)" for k, c, p in over)
    else:
        label = "BENCH PASS"
    out(label)
    return label


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    label = report(open(argv[0], encoding="utf-8", errors="replace").read())
    return 0 if label else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
