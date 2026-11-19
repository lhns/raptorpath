import sys, re, glob, os, json, statistics as st
d, ld = sys.argv[1], sys.argv[2]
rows = {}
for ln in open(ld, errors="replace"):
    if ln.startswith("P38ROW "):
        j = json.loads(ln[7:]); rows[(j["arm"], str(j["seed"]), str(j["rep"]))] = j
for f in sorted(glob.glob(os.path.join(d, "*-c.log"))):
    m = re.search(r"@def-([A-Z0-9]+)-s(\d+)-r(\d+)-", f); arm, seed, rep = m.groups()
    if arm not in ("HR", "RM", "RP"): continue
    last = None; conv = None; ticks = []; spans = []
    for ln in open(f, errors="replace"):
        if '"warmup":true' in ln: ticks=[]; spans=[]; continue
        if ln.startswith("[DIAG] t="):
            last = ln
            t = float(re.match(r"\[DIAG\] t=([\d.]+)s", ln).group(1))
            cw = re.search(r" cwnd=(\d+) infl=(\d+) ", ln)
            legs = [int(x) for x in re.findall(r" p\d:infl=(\d+)/", ln)]
            ticks.append((t, int(cw.group(1)), legs))
        elif ln.startswith("[C8CONV-S]"): conv = ln.strip()
        elif ln.startswith("[SPAN]"):
            s = re.search(r"t=([\d.]+)s .*? rr=([-\d.]+) ", ln); spans.append((float(s.group(1)), float(s.group(2))))
    pl = re.findall(r" p(\d):infl=.*? rtt=(\d+)/.*? qlp=(\d+)/(\d+) .*? plu=([\d.]+) plc=([\d.]+)", last)
    hand = re.findall(r"dgq(\d)\[hand=(\d+)", last)
    sp = re.findall(r"p(\d):sp=(\d+)", conv or "")
    j = rows.get((arm, seed, rep), {})
    # first time p1 infl > 9000
    tfill = next((t for t, c, l in ticks if len(l) > 1 and l[1] > 9000), None)
    early = [r for t, r in spans if tfill is not None and t < tfill]; late = [r for t, r in spans if tfill is not None and t >= tfill]
    print(arm, seed, rep, "legs(rtt,qlp,plu,plc)=", [(a, b, f"{int(c)}/{int(e)}", g, h) for a, b, c, e, g, h in pl], "hand=", hand, "sp=", sp,
          "truth_loss=", j.get("truth_loss_p0"), j.get("truth_loss_p1"), "rxdup=", j.get("rx_dup"),
          "tfill_p1=", tfill, "rr_early=", round(st.mean(early), 3) if early else None, len(early), "rr_late=", round(st.mean(late), 4) if late else None, len(late))
