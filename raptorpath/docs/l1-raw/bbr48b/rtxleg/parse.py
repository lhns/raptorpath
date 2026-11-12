# POST HOC, not registered. Shared parser: per row, per [DIAG] tick, the cumulative recovery counters
# (sender) plus the receiver's [C8CONV-R] per-arrival-path counters and the driver's [TRUTH] per-leg loss.
import os, re

def I(p, l, d=0):
    m = re.search(p, l); return int(m.group(1)) if m else d

def F(p, l, d=0.0):
    m = re.search(p, l); return float(m.group(1)) if m else d

def legs(l):
    # per-leg estimator app-echo rtt (ms)
    return {int(p): int(a) for p, a in re.findall(r" p(\d+):infl=.*? rtt=(\d+)/wrtt=", l)}

def tick(l, cv):
    d = dict(t=F(r"t=([0-9.]+)s", l), src=I(r"src=(\d+)sym", l), win=I(r"win=(\d+)/", l),
             retx=I(r"retx=(\d+)", l), rep=I(r"mpr\[rep=(\d+)", l), seqs=I(r"seqs=(\d+)", l),
             fired=I(r"fired=(\d+)", l), stale=I(r"stale=(\d+)", l),
             scool=I(r"supp=(\d+)/", l), slaw=I(r"supp=\d+/\d+/(\d+)", l),
             C=F(r" rtt=([0-9.]+)ms", l))
    m = re.search(r"fp/on p0:(\d+)/(\d+)(?: p1:(\d+)/(\d+))?", l)
    g = [int(x) if x else 0 for x in m.groups()] if m else [0, 0, 0, 0]
    d.update(fp0=g[0], on0=g[1], fp1=g[2], on1=g[3])
    s, c, a = map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", l).groups())
    d.update(cs=s, cc=c, ca=a)
    lg = legs(l); d.update(c0=lg.get(0, 0), c1=lg.get(1, 0))
    if cv:
        for p, sp, ro, ms, n in re.findall(r"p(\d+):sp=(\d+) ro=(\d+) st=(\d+)ms/(\d+)", cv):
            d[f"sp{p}"] = int(sp); d[f"ro{p}"] = int(ro); d[f"st{p}"] = int(ms); d[f"stn{p}"] = int(n)
    return d

def rx(path):
    out = []
    for l in open(path, errors="replace"):
        if l.startswith("[C8CONV-R]"):
            r = {}
            for p, fst, dup, lead, un, ums in re.findall(r"p(\d+):fst=(\d+) dup=(\d+) lead=(\d+) unb=(\d+)/(\d+)ms", l):
                r.update({f"fst{p}": int(fst), f"dup{p}": int(dup), f"unb{p}": int(un), f"ubms{p}": int(ums)})
            out.append(r)
    return out

def truth(path):
    t = {}
    for l in open(path, errors="replace"):
        m = re.search(r"\[TRUTH\] leg=(\d+) .*?egress_dgrams=(\d+) .*?lost=(\d+) loss=([0-9.]+)", l)
        if m: t[int(m.group(1))] = (int(m.group(2)), int(m.group(3)), float(m.group(4)))
    secs = float(re.search(r'"median_s":([0-9.]+)', open(path, errors="replace").read()).group(1))
    return t, secs

def rows(D, cell):
    for f in sorted(os.listdir(D)):
        if not (f.startswith(cell + "@") and f.endswith("-c.log")): continue
        arm, seed, rep = re.search(r"def-(\w+)-s(\d+)-r(\d+)-", f).groups()
        lines = open(os.path.join(D, f), errors="replace").read().splitlines()
        T = []
        for i, l in enumerate(lines):
            if l.startswith("[DIAG]"):
                cv = lines[i + 2] if i + 2 < len(lines) and lines[i + 2].startswith("[C8CONV-S]") else \
                     next((x for x in lines[i + 1:i + 4] if x.startswith("[C8CONV-S]")), "")
                T.append(tick(l, cv))
        hold = {}
        for l in lines:
            if l.startswith("[HOLD]") and "site=sender path=" in l and "path=-" not in l:
                hold[I(r"path=(\d+)", l)] = {k: I(k + r"=(\d+)", l) for k in ("fed", "hn_n", "hy_n", "cx_n", "emit", "cx_p50_us")}
        tr, secs = truth(os.path.join(D, f.replace("-c.log", "-drv.out")))
        R = rx(os.path.join(D, f.replace("-c.log", "-s.log")))
        yield dict(arm=arm, row=f"s{seed}r{rep}", T=T, hold=hold, truth=tr, secs=secs, R=R)
