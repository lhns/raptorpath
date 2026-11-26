import sys, os, math
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR
cell = sys.argv[1]; Tsel = float(sys.argv[2]); lo, hi = float(sys.argv[3]), float(sys.argv[4])
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
bj = [j for j in jobs if K.is_bp(j) and j.key[1] == Tsel and j.key[2] == 0.0][0]
aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
A, Bp = C.recs(aj), C.recs(bj)
n = min(len(A), len(Bp)); A, Bp = A[:n], Bp[:n]
sa, sb = K.window_shares(A), K.window_shares(Bp)
ga = Q.gauge_series(A, 1); gb = Q.gauge_series(Bp, 1)
gra = Q.gauge_series(A, 3); grb = Q.gauge_series(Bp, 3)
print("keys", list(Bp[0].keys())[:80])
print("t  | w  age  x  tov  c+ (B' seed mean) | def(A-B') | resA resB | repA repB")
ser = {}
for r in Bp:
    for row in r["rev4"]["cap_series"]:
        ser.setdefault(row[0], []).append(row)
for k in range(int(lo/0.5), int(hi/0.5)):
    t = k*0.5
    rows = ser.get(t, [])
    f = lambda j: np.mean([x[j] for x in rows]) if rows else float('nan')
    print("%5.1f | %.4f %.4f %.4f %.3f %.4f | %+.4f | %s %s | %s %s" % (t, f(2), f(3), f(4), f(5), f(6), np.nanmean(sa[:,k]-sb[:,k]),
          "%.0f" % ga[:,k].mean() if ga is not None else "-", "%.0f" % gb[:,k].mean() if gb is not None else "-",
          "%.0f" % gra[:,k].mean() if gra is not None else "-", "%.0f" % grb[:,k].mean() if grb is not None else "-"))
