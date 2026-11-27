"""Q1: join each fall of bbr_rs's max-filter estimate (qbw, [QCC]) to the [HOFF] windows whose
app_limited share (quinn's flag at on_end_acks) is below 1. Rule: a fall at QCC time tq is
'joined' if a HOFF window ending in [tq - 0.35 s, tq] (the [QCC] line trails the [HOFF] window by one line, 0.25-0.3 s) reads app_limited < 1.
Also: where the non-app-limited windows sit relative to the first S->B step.
Usage: python -I q1join.py <diag-ld dir of 68s>"""
import re, sys, glob, os
D = sys.argv[1]
for arm in ("GBU", "RBU"):
    for rep in (1, 2, 3):
        f = os.path.join(D, f"c2-100@def-{arm}-s42-r{rep}-a1-c.log")
        txt = open(f, encoding="utf-8", errors="replace").read().splitlines()
        hoff = [(float(re.search(r"t=([\d.]+)s", l).group(1)), float(re.search(r"app_limited=([\d.]+)", l).group(1)))
                for l in txt if l.startswith("[HOFF]") and re.search(r"app_limited=[\d.]+", l)]
        qcc = [(float(re.search(r"t=([\d.]+)s", l).group(1)), int(re.search(r"qbw=(\d+)", l).group(1)), re.search(r"qm=(.)", l).group(1))
               for l in txt if l.startswith("[QCC]") and "qbw=" in l]
        falls = [(t, b0, b, m) for (t0, b0, _), (t, b, m) in zip(qcc, qcc[1:]) if b < b0]
        joined = [x for x in falls if any(tq >= x[0] - 0.35 and tq <= x[0] and al < 1 for tq, al in hoff)]
        nal = [t for t, al in hoff if al < 1]
        sb = next((t for t, _, m in qcc if m == "B"), None)
        nal_hit = sum(1 for tq, al in hoff if al < 1 and any(tq >= x[0] - 0.35 and tq <= x[0] for x in falls))
        print(f"{arm} r{rep}: falls={len(falls)} joined={len(joined)} nonAL_followed_by_fall={nal_hit} non-AL windows={len(nal)} "
              f"(before first B: {sum(1 for t in nal if sb is None or t < sb - 0.3)}) first B at {sb}; "
              f"falls: " + ", ".join(f"{t:.1f}s {b0//1000}->{b//1000}k {m}" for t, b0, b, m in falls[:6]))
