"""Decode recoveries per coded symbol, per arm (receiver [RFA] fill_coded).

usage: python -I recov.py <diag-ld dir>
fill_coded (net/report.rs:51, fed at receiver.rs:1474) = seqs the decoder
reconstructed from coded repair, i.e. holes closed by decoding and not by a
source arrival. cod = sender coded symbols sent (client last [DIAG] cum=).
"""
import os, re, sys, statistics as st

ORDER = ["HB", "BM", "BP", "I1", "I2", "I3", "AM", "HA", "NA", "AP", "I4", "RM", "HR", "RP"]


def last(path, tag):
    out = None
    with open(path, errors="replace") as fh:
        for ln in fh:
            if ln.startswith(tag):
                out = ln
    return out


def g(rx, s):
    m = re.search(rx, s)
    return int(m.group(1)) if m else 0


def main(d):
    by = {}
    for f in sorted(os.listdir(d)):
        if not f.endswith("-s.log"):
            continue
        arm = re.match(r"c\d-100@def-(\w+)-s", f).group(1)
        rfa = last(os.path.join(d, f), "[RFA]")
        dg = last(os.path.join(d, f.replace("-s.log", "-c.log")), "[DIAG]")
        drv = open(os.path.join(d, f.replace("-s.log", "-drv.out")), errors="replace").read()
        lost = sum(int(x) for x in re.findall(r"\[TRUTH\].*? lost=(\d+)", drv))
        src, cod = [int(x) for x in re.search(r"cum=(\d+)/(\d+)/", dg).groups()]
        retx = g(r" retx=(\d+)", dg)
        taper = g(r"taper=(\d+)", dg)
        fc, pre, repn, red = (g(r"fill_coded=(\d+)", rfa), g(r"preempt_src=(\d+)", rfa),
                              g(r"rep_n=(\d+)", rfa), g(r"rep_redundant=(\d+)", rfa))
        by.setdefault(arm, []).append(dict(
            cod_src=cod / src, retx_src=retx / src, taper_src=taper / src, lost_src=lost / src,
            fc_src=fc / src, fc_per_cod=fc / max(1, cod), fc_per_rep=fc / max(1, repn),
            pre_per_cod=pre / max(1, cod), useful_per_rep=(repn - red) / max(1, repn),
            fc_per_lost=fc / max(1, lost)))
    keys = ["lost_src", "retx_src", "taper_src", "cod_src", "fc_src", "fc_per_cod", "fc_per_rep",
            "pre_per_cod", "useful_per_rep", "fc_per_lost"]
    print("arm  n " + " ".join(f"{k:>14}" for k in keys))
    for a in ORDER:
        if a in by:
            v = by[a]
            print(f"{a:3} {len(v):2} " + " ".join(f"{st.median([x[k] for x in v]):14.4f}" for k in keys))


if __name__ == "__main__":
    main(sys.argv[1])
