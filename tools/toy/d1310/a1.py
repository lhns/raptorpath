import sys, os, math
TOY = r"C:\Users\pierr\AppData\Local\Temp\claude\c92d18e023ce8566\28b06ad0-9458-4799-81e9-72731fe0b356\scratchpad\s3e\tools\toy"
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR
cell = sys.argv[1] if len(sys.argv) > 1 else Q.Q
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != math.inf and j.key[2] == 0.0]:
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    A, Bp = C.recs(aj), C.recs(bj)
    n = min(len(A), len(Bp)); A, Bp = A[:n], Bp[:n]
    T = bj.key[1]
    sa, sb = K.window_shares(A), K.window_shares(Bp)
    print("T", T, "seeds", n)
    for i in range(n):
        r4 = Bp[i]["rev4"]
        a, b, _ = CR.pair_window(A[i], Bp[i], None)
        on = r4["onset_t"]
        d = sa[i] - sb[i]
        k0 = int(round(a / 0.5))
        kon = int(on / 0.5)
        print("  seed %d a=%.2f onset=%.3f ov_t0=%.3f last_hand=%.2f" % (i, a, on, r4["ov_t0"], Bp[i]["last_hand"]))
        print("    deficit by window start (rel onset):", " ".join("%+.1f:%.4f" % (w*0.5 - on, d[w]) for w in range(k0, min(kon + 4, len(d)))))
