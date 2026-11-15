# POST HOC. Tail reconstruction per row: one line per [DIAG] tick after the last source line.
import re, sys
def I(p, l):
    m = re.search(p, l); return int(m.group(1)) if m else 0
P = dict(retx=r"retx=(\d+)", cod=r"cum=\d+/(\d+)/", rep=r"mpr\[rep=(\d+)", coal=r"coal=(\d+)", gdrop=r"gapdrop=(\d+)",
         sweep=r"sweeps=(\d+)", on0=r"fp/on p0:\d+/(\d+)", on1=r"p1:\d+/(\d+)\]", tx0=r"dgq0\[hand=\d+ tx=(\d+)", tx1=r"dgq1\[hand=\d+ tx=(\d+)")
for f in sys.argv[1:]:
    L = [l for l in open(f, errors="replace") if l.startswith("[DIAG]")]
    T = [float(re.search(r"t=([0-9.]+)s", l).group(1)) for l in L]
    ise = max(i for i in range(len(L)) if I(r"src=(\d+)sym", L[i]) > 0)
    print("==", f.replace("\\", "/").split("/")[-1], "t_src_end=%.1f" % T[ise])
    for i in range(ise, len(L)):
        p, l = L[i - 1], L[i]
        d = {k: I(v, l) - I(v, p) for k, v in P.items()}
        s, c, a = map(int, re.search(r"cum=(\d+)/(\d+)/(\d+)", l).groups())
        st_ = re.search(r"win=(\d+)/(\d+)", l).groups()
        rtt = re.search(r" rtt=([0-9.]+)ms", l).group(1)
        print(" t=%5.2f store=%s/%s lag=%6d retx=%3d cod=%3d rep=%3d fires/rep=%4.1f coal=%3d gdrop=%2d sweep=%d on0/on1=%3d/%3d tx0/tx1=%4d/%3d poolrtt=%sms nbud=%d" % (
            T[i], st_[0], st_[1], s - a, d["retx"], d["cod"], d["rep"], d["retx"] / max(1, d["rep"]), d["coal"], d["gdrop"], d["sweep"],
            d["on0"], d["on1"], d["tx0"], d["tx1"], rtt, I(r"nbud=(\d+)", l)))
