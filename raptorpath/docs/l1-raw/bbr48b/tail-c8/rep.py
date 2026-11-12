# POST HOC. Per-[DIAG]-line deltas of the gap-serving loop: processed reports (mpr rep), seqs visited,
# fires (retx), coalesced reports, dropped reports (gapdrop), ack-arm wakes, cooldown/law/stale suppressions, nbud.
import re, sys
f = sys.argv[1]; back = float(sys.argv[2])
K = dict(rep=r"mpr\[rep=(\d+)", seqs=r" seqs=(\d+)", retx=r"retx=(\d+)", coal=r"coal=(\d+)", gdrop=r"gapdrop=(\d+)",
         wack=r"wake\[.*? ack=(\d+)", wtail=r"wake\[.*? tail=(\d+)", supc=r"supp=(\d+)/", supl=r"supp=\d+/\d+/(\d+)", stale=r"stale=(\d+)", sw=r"sweeps=(\d+)")
L = [l for l in open(f, errors="replace") if l.startswith("[DIAG]")]
T = [float(re.search(r"t=([0-9.]+)s", l).group(1)) for l in L]
V = [{k: int(re.search(p, l).group(1)) for k, p in K.items()} for l in L]
for i in range(1, len(L)):
    if T[i] < T[-1] - back: continue
    d = {k: V[i][k] - V[i-1][k] for k in K}
    src = re.search(r"src=(\d+)", L[i]).group(1); nb = re.search(r"nbud=(\d+)", L[i]).group(1)
    print(f"t={T[i]:5.1f} src={src:5s} " + " ".join(f"{k}={v}" for k, v in d.items()) + f" fires/rep={d['retx']/max(1,d['rep']):.1f} seqs/rep={d['seqs']/max(1,d['rep']):.1f} nbud={nb}")
