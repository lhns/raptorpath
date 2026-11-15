# POST HOC. Leg1 (and leg0) quinn send-buffer backlog B = hand - tx (dgq), its dwell B / tx-rate, quinn smoothed RTT (qsr, wire,
# excludes the datagram-queue dwell), and the engine's app-echo estimator rtt per leg; 4 s bins, means over 12 rows per arm.
import os, re, sys, statistics as st
D = sys.argv[1]; acc = {}
for f in sorted(os.listdir(D)):
    if not (f.startswith("c8-100@") and f.endswith("-c.log")): continue
    arm = re.search(r"def-(\w+)-s", f).group(1)
    prev = None; qcc = []
    for l in open(os.path.join(D, f), errors="replace"):
        m = re.match(r"\[QCC\] t=([0-9.]+)s", l)
        if m:
            q = {int(p): int(s) for p, s in re.findall(r"p(\d+):qm=\S qrt_us=\d+ qsr_us=(\d+)", l)}
            qcc.append((float(m.group(1)), q)); continue
        if not l.startswith("[DIAG]") or "src=0sym" in l: continue
        t = float(re.search(r"t=([0-9.]+)s", l).group(1))
        dg = {int(i): (int(h), int(x)) for i, h, x in re.findall(r"dgq(\d+)\[hand=(\d+) tx=(\d+)", l)}
        app = {int(p): int(a) for p, a in re.findall(r" p(\d+):infl=.*? rtt=(\d+)/wrtt=", l)}
        if prev and len(dg) == 2 and len(app) == 2:
            dt = t - prev[0]; q = qcc[-1][1] if qcc else {}
            for i in (0, 1):
                rate = (dg[i][1] - prev[1][i][1]) / dt
                B = dg[i][0] - dg[i][1]
                acc.setdefault((arm, i, int(t // 4)), []).append((B, B / rate * 1e3 if rate > 0 else float("nan"), q.get(i, float("nan")) / 1e3, app[i]))
        prev = (t, dg)
for i in (1, 0):
    for arm in ("BASE", "CAND"):
        print(f"leg{i} {arm}")
        for b in range(11):
            v = acc.get((arm, i, b))
            if not v: continue
            dw = [x[1] for x in v if x[1] == x[1]]
            print(f"  {4*b:2d}-{4*b+4:2d}s B=hand-tx {st.mean(x[0] for x in v):6.1f} dgrams  dwell=B/txrate {st.mean(dw):5.0f}ms  quinn srtt(wire) {st.mean(x[2] for x in v):5.0f}ms  app-echo rtt {st.mean(x[3] for x in v):5.0f}ms  app-dwell {st.mean(x[3] for x in v)-st.mean(dw):5.0f}ms")
