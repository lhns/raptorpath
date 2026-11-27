"""Per-row CPU attribution readouts for the hand-off gate (plans 68, 68r, 68s).

Usage: python -I rows.py <l1-raw dir>   (reads hoff68*/rep*/diag-ld)
Prints one line per row with cumulative counters normalised per GB of payload.
"""
import glob, os, re, sys, statistics as st
from collections import defaultdict

ROOT = sys.argv[1]
LEDGERS = [("68", "hoff68/rep68"), ("68r", "hoff68r/rep68r"), ("68s", "hoff68s/rep68s")]
NAME = re.compile(r"(c\d)-100@def-([A-Z])([A-Z0-9]+)-s42-r(\d)-a1-c\.log$")


def last(pat, text, cast=float):
    m = None
    for m in re.finditer(pat, text):
        pass
    return cast(m.group(1)) if m else None


def row(ledger, path):
    m = NAME.search(path)
    cfg, arm, cell, rep = m.groups()
    c = open(path, encoding="utf-8", errors="replace").read()
    d = open(path[:-6] + "-drv.out", encoding="utf-8", errors="replace").read()
    r = dict(ledger=ledger, arm=arm, cell=cell, rep=int(rep), cfg=cfg)
    r["cpu_cli"] = last(r"CPUCLI=([\d.]+)s", d)
    r["cpu_srv"] = last(r"CPUSRV=([\d.]+)s", d)
    r["secs"] = last(r'"max_s":([\d.]+)', d)
    r["bytes"] = last(r'"bytes":(\d+)', d)
    skbs = [int(x) for x in re.findall(r"egress_skbs=(\d+)", d)]
    dgs = [int(x) for x in re.findall(r"egress_dgrams=(\d+)", d)]
    r["skbs"] = sum(skbs); r["dgrams"] = sum(dgs)
    r["skbs0"] = skbs[0] if skbs else None
    r["dgrams0"] = dgs[0] if dgs else None
    diag = [l for l in c.splitlines() if l.startswith("[DIAG]")]
    hoff = [l for l in c.splitlines() if l.startswith("[HOFF]")]
    # loop iterations and body time, summed over windows
    r["iters"] = sum(int(x) for l in diag for x in re.findall(r" n=(\d+) us=", l))
    r["busy_us"] = sum(int(x) for l in diag for x in re.findall(r"busy_us=(\d+)", l))
    ld = diag[-1] if diag else ""
    wk = re.search(r"wake\[([^\]]*)\]", ld)
    if wk:
        for k, v in re.findall(r"(\w+)=(\d+)", wk.group(1)):
            r["wk_" + k] = int(v)
    r["eb_bursts"] = last(r"eb_bursts=(\d+)", ld, int)
    r["eb_syms"] = last(r"eb_syms=(\d+)", ld, int)
    r["hand"] = sum(int(x) for x in re.findall(r"dgq\d\[hand=(\d+)", ld))
    r["tx"] = sum(int(x) for x in re.findall(r"dgq\d\[hand=\d+ tx=(\d+)", ld))
    lh = hoff[-1] if hoff else ""
    r["otw"] = last(r"owner_timer_wakes=(\d+)", lh, int)
    sl = open(path[:-6] + "-s.log", encoding="utf-8", errors="replace").read()
    r["otw_srv"] = last(r"owner_timer_wakes=(\d+)", sl, int) or 0
    r["packs"] = last(r"probe_acks=(\d+)", lh, int)
    # per-leg batch (mean over windows, t>=2 s), refusals = sum hand_bind*n
    bat = defaultdict(list); refus = 0.0; turns = 0
    for l in hoff:
        t = float(re.search(r"t=([\d.]+)s", l).group(1))
        for pid, body in re.findall(r" p(\d+)\[(.*?)\](?= p\d+\[| infl_vs_out)", l + " infl_vs_out"):
            n = re.search(r" n=(\d+)", body); hb = re.search(r"hand_bind=([\d.]+)", body)
            b = re.search(r" batch=([\d.]+)", body)
            if n and hb:
                refus += int(n.group(1)) * float(hb.group(1)); turns += int(n.group(1))
            ro = re.search(r"reopen=[^ ]*/[^ ]*/(\d+)", body)
            if ro:
                r["reopens"] = r.get("reopens", 0) + int(ro.group(1))
            if b and t >= 2:
                bat[int(pid)].append(float(b.group(1)))
    r["refus"] = refus
    r["turns"] = turns
    r["batch0"] = st.median(bat[0]) if bat.get(0) else None
    r["batch1"] = st.median(bat[1]) if bat.get(1) else None
    return r


def rows():
    out = []
    for led, sub in LEDGERS:
        for p in sorted(glob.glob(os.path.join(ROOT, sub, "diag-ld", "*-c.log"))):
            if NAME.search(p):
                out.append(row(led, p))
    return out


if __name__ == "__main__":
    R = rows()
    cols = ["cpu_cli", "cpu_srv", "secs", "skbs", "dgrams", "iters", "busy_us", "wk_tun", "wk_paused",
            "wk_ack", "wk_nack", "wk_tail", "eb_bursts", "hand", "otw", "packs", "refus", "batch0", "batch1"]
    print("ledger arm cell rep " + " ".join(cols))
    for r in R:
        print(r["ledger"], r["arm"], r["cell"], r["rep"], " ".join(str(round(r.get(k), 2)) if isinstance(r.get(k), float) else str(r.get(k)) for k in cols))
