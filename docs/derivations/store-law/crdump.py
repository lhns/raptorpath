"""Count rule (count-rule.md §1): the s62 D12 RWM_RTT_DUMP row read sample by sample.

Usage (from docs/):  python -I -X utf8 derivations/store-law/crdump.py l1-raw/s62/rep62r0-diag.tgz > derivations/store-law/crdump.out

Reads the dump in place (tarfile). Format (raptorpath/src/net/rttdump.rs on slaw-gauge d416d59b): `[RTTDUMP] p=<path> t0=<us> n=<count>
d=<dt,rtt;...>`, t_k = t0 + sum of dt within the batch, rtt in us. The dump is fed in record_rtt_sample immediately before the [SLAW] window
sampler's observe() with the same value (scheduler/path.rs), so sample i belongs to the window where the cumulative [SLAW] nk passes i.
Sections:
  0. alignment: dump samples against the [SLAW] windows' nk (leg 0), and the window minima recomputed from the dump against the printed tmin
  1. the shape: bulk quantiles, the low tail (depth below the window median), how many samples per window are low
  2. what a low sample follows: the idle gap before it, its position in its ack burst, its position in the window
  3. the count dependence: the minimum of m consecutive samples, of m bursts, and of m samples drawn at random, for m = native .. 4 200
  4. candidate count-free readings on the dump: low quantiles, the per-burst-first minimum, trimmed minima
"""
import random
import re
import statistics as st
import sys
import tarfile

P = print
DUMP = re.compile(r"^\[RTTDUMP\] p=(\d+) t0=(\d+) n=(\d+) d=(\S+)")
SLAW = re.compile(r"^\[SLAW\] t=([\d.]+) k=(\d+) p=0 .*?nk=(\d+) tmin=([\d.]+) tp50=([\d.]+)")
BURST_US = 200          # declared: samples closer than 200 us belong to one ack's batch (the dump shows 1-10 us inside a batch, >= 300 us between)


def q(v, p):
    v = sorted(v)
    return v[min(len(v) - 1, int(p * len(v)))]


def load(path, name):
    with tarfile.open(path, "r:gz") as tf:
        for mb in tf:
            if mb.name.endswith(name):
                return tf.extractfile(mb).read().decode("utf-8", "replace").splitlines()
    raise SystemExit(f"{name} not in {path}")


def main(argv):
    lines = load(argv[1], "c3j0-25@def-D12-s42-r3-a1-c.log")
    samples, wins = [], []
    for ln in lines:
        m = DUMP.match(ln)
        if m and m.group(1) == "0":
            t = int(m.group(2))
            for pair in m.group(4).rstrip(";").split(";"):
                dt, rtt = pair.split(",")
                t += int(dt)
                samples.append((t, int(rtt)))
            continue
        m = SLAW.match(ln)
        if m:
            wins.append((float(m.group(1)), int(m.group(2)), int(m.group(3)), float(m.group(4)), float(m.group(5))))
    P(f"s62 D12 dump (crdump.py): {len(samples)} samples dumped on leg 0; {len(wins)} [SLAW] windows on leg 0 with sum nk {sum(w[2] for w in wins)}")

    # ── 0. alignment ──
    P("\n== 0. Alignment: cumulative [SLAW] nk against the dump index; window minima from the dump against the printed tmin ==")
    idx, assign = 0, []
    for t, k, nk, tmin, tp50 in wins:
        assign.append((t, k, nk, tmin, tp50, idx, idx + nk))
        idx += nk
    ok = [a for a in assign if a[6] <= len(samples) and a[2] > 0]
    match = sum(1 for a in ok if abs(min(r for _, r in samples[a[5]:a[6]]) / 1000 - a[3]) < 0.002)
    P(f"  windows fully inside the dump: {len(ok)}; recomputed minimum equals the printed tmin (to 2 us) on {match}")
    late = [a for a in ok if a[0] >= 15.0 and a[2] >= 10]
    P(f"  LATE windows (t >= 15 s, nk >= 10): {len(late)}; median nk {st.median(a[2] for a in late):.0f}")

    s_late = [s for a in late for s in samples[a[5]:a[6]]]
    rtts = [r / 1000 for _, r in s_late]

    # bursts over the LATE span
    bursts, cur = [], [s_late[0]]
    for prev, s in zip(s_late, s_late[1:]):
        if s[0] - prev[0] < BURST_US:
            cur.append(s)
        else:
            bursts.append(cur)
            cur = [s]
    bursts.append(cur)

    # ── 1. shape ──
    P("\n== 1. The shape of the LATE floor samples (ms) ==")
    P("  quantiles: " + " | ".join(f"p{int(p * 100) if p >= 0.01 else p * 100:g} {q(rtts, p):.3f}" for p in (0.0, 0.001, 0.005, 0.01, 0.02, 0.05, 0.10, 0.25, 0.50, 0.75, 0.90, 0.99)))
    med = q(rtts, 0.5)
    for d in (0.5, 1.0, 2.0, 3.0, 4.0):
        n = sum(1 for r in rtts if r < med - d)
        P(f"  samples more than {d:.1f} ms below the median {med:.3f}: {n} of {len(rtts)} ({100 * n / len(rtts):.2f} %), per native window {n / len(late):.2f}")
    P(f"  ack bursts (gap >= {BURST_US} us): {len(bursts)}, {len(bursts) / len(late):.1f} per window, {len(s_late) / len(bursts):.2f} samples per burst")
    inburst = [max(r for _, r in b) - min(r for _, r in b) for b in bursts if len(b) > 1]
    P(f"  the rtt spread inside one burst: median {st.median(inburst) / 1000:.3f} ms, p90 {q(inburst, 0.9) / 1000:.3f}")

    # ── 2. what a low sample follows ──
    P("\n== 2. What a low sample follows ==")
    thr = med - 1.0
    gaps = []
    for i, b in enumerate(bursts):
        g = (b[0][0] - bursts[i - 1][-1][0]) / 1000 if i else None
        gaps.append(g)
    low_b = [i for i, b in enumerate(bursts) if min(r for _, r in b) / 1000 < thr]
    P(f"  bursts holding a sample > 1 ms below the median: {len(low_b)} of {len(bursts)} ({100 * len(low_b) / len(bursts):.1f} %)")
    gl = [gaps[i] for i in low_b if gaps[i] is not None]
    ga = [g for g in gaps if g is not None]
    P(f"  the idle gap before the burst (ms): low bursts median {st.median(gl):.2f} p10 {q(gl, 0.1):.2f} p90 {q(gl, 0.9):.2f} | all bursts median {st.median(ga):.2f} p10 {q(ga, 0.1):.2f} p90 {q(ga, 0.9):.2f}")
    for lo, hi in ((0, 1), (1, 3), (3, 6), (6, 10), (10, 20), (20, 50), (50, 1e9)):
        sel = [i for i, g in enumerate(gaps) if g is not None and lo <= g < hi]
        if sel:
            frac = sum(1 for i in sel if i in set(low_b)) / len(sel)
            mins = [min(r for _, r in bursts[i]) / 1000 for i in sel]
            P(f"    gap [{lo}, {hi if hi < 1e9 else 'inf'}) ms: {len(sel)} bursts, share low {frac:.3f}, burst-minimum median {st.median(mins):.3f} p05 {q(mins, 0.05):.3f}")
    pos = {}
    for b in bursts:
        for j, (_, r) in enumerate(b):
            pos.setdefault(min(j, 5), []).append(r / 1000)
    P("  rtt by position in the burst (0 = first sample after the gap): " + " | ".join(f"pos {j}{'+' if j == 5 else ''}: median {st.median(v):.3f} p01 {q(v, 0.01):.3f} n {len(v)}" for j, v in sorted(pos.items())))
    # position in window
    ph = {}
    for a in late:
        ws = samples[a[5]:a[6]]
        mi = min(range(len(ws)), key=lambda i: ws[i][1])
        ph.setdefault(min(4, int(5 * mi / len(ws))), 0)
        ph[min(4, int(5 * mi / len(ws)))] += 1
    P("  where the window's minimum falls inside its window (quintile of the sample index): " + " ".join(f"Q{k + 1} {v}" for k, v in sorted(ph.items())))
    # phase of low samples inside their window, by time (window span = first to last sample of the window)
    ph2, deep = {}, 0
    for a in late:
        ws = samples[a[5]:a[6]]
        t0w, t1w = ws[0][0], ws[-1][0]
        for t, r in ws:
            if r / 1000 < med - 4.0 and t1w > t0w:
                deep += 1
                k = min(9, int(10 * (t - t0w) / (t1w - t0w)))
                ph2[k] = ph2.get(k, 0) + 1
    P(f"  samples > 4 ms below the median ({deep}) by time decile inside their window: " + " ".join(f"{k}:{ph2.get(k, 0)}" for k in range(10)))
    wl = [sum(1 for _, r in samples[a[5]:a[6]] if r / 1000 < med - 4.0) for a in late]
    P(f"  native windows holding at least one sample > 4 ms below the median: {sum(1 for x in wl if x)} of {len(wl)}; per-window count distribution "
      + " ".join(f"{k}:{sum(1 for x in wl if x == k)}" for k in range(0, 8)) + f" 8+:{sum(1 for x in wl if x >= 8)}")
    # time spread of low samples
    lowt = [t for t, r in s_late if r / 1000 < thr]
    dts = [(b - a) / 1e6 for a, b in zip(lowt, lowt[1:])]
    if dts:
        mean = sum(dts) / len(dts)
        cv = (sum((x - mean) ** 2 for x in dts) / len(dts)) ** 0.5 / mean
        P(f"  low samples over time: {len(lowt)} in {(s_late[-1][0] - s_late[0][0]) / 1e6:.1f} s; inter-arrival mean {mean:.3f} s, CV {cv:.2f} (1 = Poisson-like, even in time)")

    # ── 2b. send-time view (advisor round): flights by send time, the case of each deep-low sample, phase against absolute time ──
    P("\n== 2b. The send-time view: s = t - rtt; flights = samples whose send times lie within 2 ms of the previous (sorted by send time) ==")
    dts_arr = [b[0] - a[0] for a, b in zip(s_late, s_late[1:])]
    edges = (0, 10, 50, 200, 1000, 2000, 5000, 10000, 20000, 50000, 10 ** 9)
    P("  arrival dt histogram (us): " + " | ".join(f"[{lo}, {hi if hi < 10 ** 9 else 'inf'}): {sum(1 for d in dts_arr if lo <= d < hi)}" for lo, hi in zip(edges, edges[1:])))
    b2 = 1 + sum(1 for d in dts_arr if d >= 2000)
    P(f"  arrival bursts at a 2 ms split: {b2} ({b2 / len(late):.1f} per window) against {len(bursts)} at 200 us")
    snd = sorted((t - r, r, t) for t, r in s_late)
    flights, cur = [], [snd[0]]
    for prev, x in zip(snd, snd[1:]):
        if x[0] - prev[0] < 2000:
            cur.append(x)
        else:
            flights.append(cur)
            cur = [x]
    flights.append(cur)
    fgap = [None] + [(f[0][0] - p[-1][0]) / 1000 for p, f in zip(flights, flights[1:])]
    P(f"  flights: {len(flights)} ({len(flights) / len(late):.1f} per window), {len(s_late) / len(flights):.2f} samples per flight; send gap before a flight: median "
      f"{st.median([g for g in fgap if g is not None]):.1f} ms, p90 {q([g for g in fgap if g is not None], 0.9):.1f}")
    deep_thr = med - 4.0
    cases = {"alone (a one-sample flight)": 0, "first flight after a send pause >= 60 ms": 0, "inside a multi-sample flight": 0}
    rank = {}
    for i, f in enumerate(flights):
        for j, (s_, r, t) in enumerate(sorted(f)):
            if r / 1000 < deep_thr:
                if len(f) == 1:
                    cases["alone (a one-sample flight)"] += 1
                elif fgap[i] is not None and fgap[i] >= 60:
                    cases["first flight after a send pause >= 60 ms"] += 1
                else:
                    cases["inside a multi-sample flight"] += 1
                rank[min(j, 5)] = rank.get(min(j, 5), 0) + 1
    P("  deep-low samples (> 4 ms below the median) by case: " + " | ".join(f"{k} {v}" for k, v in cases.items()))
    P("  their send rank inside the flight (0 = sent first): " + " ".join(f"{k}{'+' if k == 5 else ''}:{v}" for k, v in sorted(rank.items())))
    fsz = {}
    for f in flights:
        lowf = any(r / 1000 < deep_thr for _, r, _ in f)
        fsz.setdefault(min(len(f), 12), [0, 0])
        fsz[min(len(f), 12)][0] += 1
        fsz[min(len(f), 12)][1] += lowf
    P("  share of flights holding a deep-low sample, by flight size: " + " | ".join(f"{k}{'+' if k == 12 else ''}: {v[1]}/{v[0]}" for k, v in sorted(fsz.items())))
    for lo, hi in ((0, 20), (20, 40), (40, 60), (60, 10 ** 9)):
        sel = [i for i, g in enumerate(fgap) if g is not None and lo <= g < hi]
        if sel:
            P(f"    send gap [{lo}, {hi if hi < 10 ** 9 else 'inf'}) ms: {len(sel)} flights, share with a deep-low sample {sum(1 for i in sel if any(r / 1000 < deep_thr for _, r, _ in flights[i])) / len(sel):.3f}")
    phase = [0] * 10
    phall = [0] * 10
    for t, r in s_late:
        k = min(9, int(10 * (t % 250000) / 250000))
        phall[k] += 1
        if r / 1000 < deep_thr:
            phase[k] += 1
    P("  deep-low samples by absolute dump time mod 250 ms (deciles): " + " ".join(str(x) for x in phase) + "   | all samples: " + " ".join(str(x) for x in phall))

    # ── 2c. the independent draw ──
    P("\n== 2c. The independent draw: the minimum over m units drawn at random against m consecutive units (median over 300 draws / all blocks; ms) ==")
    fmins = [min(r for _, r, _ in f) / 1000 for f in flights]
    bmins = [min(r for _, r in b) / 1000 for b in bursts]
    seqv = [r / 1000 for _, r in s_late]
    for unit, vals, per in (("samples", seqv, len(seqv) / len(late)), ("arrival bursts (200 us)", bmins, len(bmins) / len(late)), ("flights (send, 2 ms)", fmins, len(fmins) / len(late))):
        parts = []
        for nw in (1, 2, 4, 8):
            m = max(1, round(nw * per))
            cons = st.median(min(vals[i:i + m]) for i in range(0, len(vals) - m + 1, m))
            rr = random.Random(11)
            rand = st.median(min(rr.sample(vals, m)) for _ in range(300))
            parts.append(f"{nw} window(s) = {m}: consecutive {cons:.2f} random {rand:.2f}")
        P(f"  {unit:<24} " + " | ".join(parts))
    p_f = sum(1 for f in flights if any(r / 1000 < deep_thr for _, r, _ in f)) / len(flights)
    fpw = len(flights) / len(late)
    P(f"  occupancy: share of flights with a deep-low sample p = {p_f:.3f}; independent flights predict a share of windows holding one = 1 - (1 - p)^{fpw:.1f} = "
      f"{1 - (1 - p_f) ** fpw:.3f}, measured {sum(1 for x in wl if x) / len(wl):.3f}")

    # ── 3. the count dependence ──
    P("\n== 3. The minimum against the count: consecutive samples, consecutive bursts, random samples (median over blocks; ms) ==")
    rnd = random.Random(7)
    seq = [r / 1000 for _, r in s_late]
    bmins = [min(r for _, r in b) / 1000 for b in bursts]
    for m in (58, 116, 250, 500, 1000, 2100, 4200):
        cons = st.median(min(seq[i:i + m]) for i in range(0, len(seq) - m + 1, m))
        rand = st.median(min(rnd.sample(seq, m)) for _ in range(200))
        mb = max(1, round(m * len(bursts) / len(seq)))
        bc = st.median(min(bmins[i:i + mb]) for i in range(0, len(bmins) - mb + 1, mb))
        P(f"  m {m:>5}: consecutive {cons:.3f} | random {rand:.3f} | {mb} consecutive bursts {bc:.3f}")

    # ── 4. candidate count-free readings ──
    P("\n== 4. Candidate count-free floor readings on the dump: per native window, then pooled over 4 / 36 windows (≈ 250 / 2 100 samples) ==")
    def per_window(fn, pool):
        out = []
        for i in range(0, len(late) - pool + 1, pool):
            ws = [s for a in late[i:i + pool] for s in samples[a[5]:a[6]]]
            out.append(fn([r / 1000 for _, r in ws]))
        return st.median(out)
    cands = (("minimum", min), ("p01", lambda v: q(v, 0.01)), ("p05", lambda v: q(v, 0.05)), ("p10", lambda v: q(v, 0.10)), ("p25", lambda v: q(v, 0.25)),
             ("median", lambda v: q(v, 0.50)), ("2nd smallest", lambda v: sorted(v)[1] if len(v) > 1 else v[0]), ("5th smallest", lambda v: sorted(v)[min(4, len(v) - 1)]))
    for nm, fn in cands:
        a1, a4, a36 = per_window(fn, 1), per_window(fn, 4), per_window(fn, 36)
        P(f"  {nm:<13} native {a1:.3f} | 4 windows {a4:.3f} | 36 windows {a36:.3f} | native - 36: {a1 - a36:+.3f} ms")


if __name__ == "__main__":
    main(sys.argv)
