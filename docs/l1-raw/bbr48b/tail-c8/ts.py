# POST HOC. Tail time series of [DIAG] fields per run.
import re, sys
f = sys.argv[1]; back = float(sys.argv[2]) if len(sys.argv) > 2 else 6
L = [l for l in open(f, errors="replace") if l.startswith("[DIAG]")]
def g(p, l):
    m = re.search(p, l); return m.group(1) if m else "-"
P = dict(win=r"win=(\S+)", wnd2=r"wnd2=(\S+)", cum=r"cum=(\S+)", src=r"src=(\d+)", cod=r"cod=(\d+)",
         infl=r" infl=(\d+)", rtt=r" rtt=(\S+)ms", relgap=r"relgap=(\S+)", srel=r"srel=(\S+)", gate=r"gate=(\d+)",
         fpon=r"fp/on (p0:\S+ p1:\S+)\]", wait=r"wait\[(.*?) n=", cwnd=r"cwnd=(\d+)", sidle=r"sidle=(\S+)")
D = dict(retx=r"retx=(\d+)", sw=r"sweeps=(\d+)", tx0=r"dgq0\[hand=\d+ tx=(\d+)", tx1=r"dgq1\[hand=\d+ tx=(\d+)",
         stale=r"stale=(\d+)", coal=r"coal=(\d+)", wtail=r"wake\[.*? tail=(\d+)", wnack=r"wake\[.*? nack=(\d+)", wack=r"wake\[.*? ack=(\d+)")
rows = [(float(g(r"t=([0-9.]+)s", l)), l) for l in L]
tend = rows[-1][0]; prev = None
for t, l in rows:
    if t >= tend - back:
        v = {k: g(p, l) for k, p in P.items()}
        dd = {k: (int(g(p, l)) - int(g(p, prev))) if prev else 0 for k, p in D.items()}
        print(f"t={t:5.1f} " + " ".join(f"{k}={v[k]}" for k in ("win","wnd2","cum","src","cod","infl","cwnd","rtt","relgap","srel","gate")) + " | " + " ".join(f"d{k}={x}" for k, x in dd.items()) + f" | {v['fpon']} | {v['wait']}")
    prev = l
