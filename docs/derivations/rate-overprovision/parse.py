"""Parse s51b client/driver logs into per-row and per-tick records (JSON).

usage: python -I parse.py <diag-ld dir> <out.json>
"""
import json, os, re, sys, statistics as st

ARM_DELTA = {"HB": 0.005, "BM": 0.0049, "BP": 0.0051, "I1": 0.0075, "I2": 0.0158,
             "I3": 0.05, "AM": 0.49, "HA": 0.5, "NA": 0.5, "AP": 0.51, "I4": 5.0,
             "RM": 49.0, "HR": 50.0, "RP": 51.0}
PATH_RE = re.compile(r" p(\d):infl=(\d+)/[^|]*? pl=([0-9.]+) cmp=\S+ rtt=([0-9.]+)/wrtt=([0-9.]+)/rtp([0-9.]+)ms.*? plu=([0-9.]+) plc=([0-9.]+)")


def num(rx, s, f=float, d=None):
    m = re.search(rx, s)
    return f(m.group(1)) if m else d


def parse_row(cpath, dpath):
    diag, span = [], []
    last = None
    with open(cpath, errors="replace") as fh:
        for ln in fh:
            if ln.startswith("[DIAG]"):
                last = ln
                t = num(r"t=([0-9.]+)s", ln)
                srcrate = num(r" src=(\d+)sym/s", ln, int, 0)
                pp = []
                for m in PATH_RE.finditer(ln):
                    pp.append(dict(id=int(m.group(1)), infl=int(m.group(2)), pl=float(m.group(3)),
                                   rtt=float(m.group(4)), wrtt=float(m.group(5)), rtp=float(m.group(6)),
                                   plu=float(m.group(7)), plc=float(m.group(8))))
                diag.append(dict(t=t, src=srcrate, cwnd=num(r" cwnd=(\d+)", ln, int),
                                 infl=num(r" infl=(\d+)", ln, int), paths=pp))
            elif ln.startswith("[SPAN]"):
                span.append(dict(t=num(r"t=([0-9.]+)s", ln), rr=num(r" rr=([0-9.]+)", ln),
                                 wsize=num(r"wsize=(\d+)", ln, int), astar=num(r"a_star=Some\((\d+)\)", ln, int),
                                 owed=num(r"owed=([0-9.]+)", ln)))
    cum = re.search(r"cum=(\d+)/(\d+)/(\d+)", last)
    src, cod = int(cum.group(1)), int(cum.group(2))
    taper = num(r"taper=(\d+)", last, int, 0)
    retx = num(r" retx=(\d+)", last, int, 0)
    truth, mbps, secs = [], None, None
    with open(dpath, errors="replace") as fh:
        for ln in fh:
            if "[TRUTH]" in ln:
                truth.append(dict(leg=num(r"leg=(\d+)", ln, int), dgrams=num(r"egress_dgrams=(\d+)", ln, int),
                                  gso=num(r"gso=([0-9.]+)", ln), lost=num(r"lost=(\d+)", ln, int),
                                  loss=num(r"loss=([0-9.]+)", ln)))
            if '"summary":true' in ln:
                j = json.loads(ln); mbps, secs = j["mean_mbps"], j["max_s"]
            m = re.search(r"QDISC cli(\d).*gemodel p ([0-9.]+)% r ([0-9.]+)%.*rate (\d+)Mbit", ln)
            if m:
                for tr in truth:
                    pass
                truth.append(dict(qdisc=int(m.group(1)), p=float(m.group(2)) / 100, q=float(m.group(3)) / 100,
                                  rate=int(m.group(4))))
    return dict(src=src, cod=cod, taper=taper, retx=retx, mbps=mbps, secs=secs, truth=truth,
                diag=diag, span=span)


def main(d, out):
    rows = []
    for f in sorted(os.listdir(d)):
        if not f.endswith("-c.log"):
            continue
        m = re.match(r"(c\d)-100@def-(\w+)-s(\d+)-r(\d+)-a1-c\.log", f)
        cell, arm, seed, rep = m.group(1), m.group(2), int(m.group(3)), int(m.group(4))
        r = parse_row(os.path.join(d, f), os.path.join(d, f.replace("-c.log", "-drv.out")))
        r.update(cell=cell, arm=arm, seed=seed, rep=rep, delta=ARM_DELTA[arm])
        rows.append(r)
    with open(out, "w") as fh:
        json.dump(rows, fh)
    print(len(rows), "rows")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
