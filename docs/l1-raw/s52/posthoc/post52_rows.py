import sys, os, re, glob, json
sys.path.insert(0, sys.argv[1])
import place38_parse as pp
ld, diag = sys.argv[2], sys.argv[3]
rows, _ = pp.rows_of([ld])
rows = [r for r in rows if r["cell"] == pp.REPORT_CELL52 and r["status"] == "LIVE"]
def last(pat, text):
    m = None
    for m in re.finditer(pat, text): pass
    return m
for arm in ("BASE","CAND","W0","W05","WDER"):
    print("==", arm)
    for r in sorted((x for x in rows if x["arm"] == arm), key=lambda x:(x["seed"], x["rep"])):
        t = r["t52"]
        logs = glob.glob(os.path.join(diag, f"c8-100@def-{arm}-s{r['seed']}-r{r['rep']}-a*-*.log"))
        hold = {}; src = None
        for f in logs:
            txt = open(f, errors="replace").read()
            for line in txt.splitlines():
                if line.startswith("[HOLD]") and "site=sender" in line:
                    mm = re.search(r" path=(\S+)", line); p = mm.group(1) if mm else "?"
                    hold[p] = line
            m = last(r"cum=(\d+)/", txt)
            if m and "-c.log" in f or (m and src is None): src = int(m.group(1)) if m else src
        hs = []
        for p in sorted(hold):
            l = hold[p]; g = lambda k: re.search(k + r"=([0-9.]+)", l)
            hs.append("p%s fed=%s hn=%s hy=%s cx=%s age_n=%s ripe=%s" % (p, *(g(k).group(1) if g(k) else "-" for k in ("fed","hn_n","hy_n","cx_n","age_n","ripe_frac"))))
        print(f"  s{r['seed']} r{r['rep']} tail={t.get('tail_s')} H0={t.get('H0')} share={t.get('send_leg1_share')} dup={t.get('dups')} src={src} | " + " | ".join(hs))
