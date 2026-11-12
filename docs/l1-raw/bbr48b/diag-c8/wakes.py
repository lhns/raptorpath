# POST HOC, not registered. Client sender task: wake causes and wait counts summed over [DIAG] lines (whole measured run), per arm/cell.
import os, re, sys, statistics as st
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
acc = {}
for f in sorted(os.listdir(L)):
    if not f.endswith("-c.log"): continue
    cell, arm = re.match(r"(c\w+-\d+)@def-(\w+)-", f).groups()
    tot = {}
    for ln in open(os.path.join(L, f), errors="replace"):
        if not ln.startswith("[DIAG] t="): continue
        w = re.search(r"wake\[([^\]]*)\]", ln); wt = re.search(r"wait\[.* n=(\d+) us=\d+ busy=\d+% busy_us=(\d+)\]", ln)
        if w:
            for k, v in re.findall(r"(\w+)=(\d+)", w.group(1)): tot["wake." + k] = tot.get("wake." + k, 0) + int(v)
        if wt:
            tot["wait.n"] = tot.get("wait.n", 0) + int(wt.group(1)); tot["busy_us"] = tot.get("busy_us", 0) + int(wt.group(2))
        for k in ("eb_bursts",):
            m = re.search(k + r"=(\d+)", ln)
            if m: tot[k] = int(m.group(1))
    acc.setdefault((cell, arm), []).append(tot)
out = ["POST HOC / NOT REGISTERED. Client sender task wake causes (sum over [DIAG] lines), medians over rows; busy_us per wait = sender busy per wake-cycle"]
for cell in ("c1d-400", "c2-100", "c8-100"):
    keys = sorted(set(k for t in acc[(cell, "BASE")] for k in t))
    out.append(f"== {cell}")
    for k in keys:
        b = st.median(t.get(k, 0) for t in acc[(cell, "BASE")]); a = st.median(t.get(k, 0) for t in acc[(cell, "CAND")])
        if b == 0 and a == 0: continue
        out.append(f"  {k:18s} BASE {b:12.0f} CAND {a:12.0f}  {100*(a/b-1) if b else float('nan'):+6.1f}%")
    pb = st.median(t["busy_us"] / t["wait.n"] for t in acc[(cell, "BASE")]); pa = st.median(t["busy_us"] / t["wait.n"] for t in acc[(cell, "CAND")])
    out.append(f"  busy_us per wait   BASE {pb:12.1f} CAND {pa:12.1f}  {100*(pa/pb-1):+6.1f}%")
print("\n".join(out)); open(os.path.join(D, "diag-c8/wakes_out.txt"), "w").write("\n".join(out) + "\n")
