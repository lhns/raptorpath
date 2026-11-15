# POST HOC. Frontier lag (src-ack), store outstanding, retx rate over the run, every ~4 s.
import re, sys
f = sys.argv[1]
prev = None
for l in open(f, errors="replace"):
    if not l.startswith("[DIAG]"): continue
    t = float(re.search(r"t=([0-9.]+)s", l).group(1))
    s, c, a = map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", l).groups())
    win = re.search(r"win=(\S+)", l).group(1); retx = int(re.search(r"retx=(\d+)", l).group(1))
    rtt = re.search(r" rtt=(\S+)ms", l).group(1); paused = re.search(r"paused=(\d+)", l).group(1)
    age = re.search(r"age=(\d+)ms", l).group(1)
    if prev is None or t - prev[0] >= 3.9 or "src=0sym" in l and prev[5]:
        pr = (retx - prev[1]) / (t - prev[0]) if prev else 0
        pc = (c - prev[2]) / (t - prev[0]) if prev else 0
        print(f"t={t:5.1f} lag={s-a:6d} win={win:10s} paused={paused:3s}% rtt={rtt:6s} retx/s={pr:6.0f} cod/s={pc:5.0f} age_mean={age}")
        prev = (t, retx, c, 0, 0, False)
