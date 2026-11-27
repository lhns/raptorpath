"""13.10 falsifier: item 2 of OV-long-q binned by DEQUEUE time instead of hand-over time.

  python d1310/deq_item2.py run OUTDIR [--workers 4]   rerun A and B' (T in {2, 1, 0.6, 0.4}, H = 0, 6 seeds), store per-window counts by dequeue
                                                       and by hand-over time (one small JSON per run)
  python d1310/deq_item2.py read OUTDIR                 item 2 per T: dequeue binning (primary and share form) against d_ss, hand-over binning as stored

A unit's dequeue time is the emit call that takes it off the intake: a sent unit's first send time; a shed unit (dequeue or intake expiry) is
dropped inside the emit call that sends the next unit, so it takes that unit's send time (toy2 Sim.emit)."""
import json, math, os, sys, time
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np

TS = (2.0, 1.0, 0.6, 0.4)                       # the rows that can falsify first
SEEDS = range(6)
W, LAM = 0.5, 12000.0
D_SS = {0.4: 0.0009, 0.6: 0.0011, 1.0: 0.0011, 2.0: 0.0017}          # 13.10.3, OV-long-q
HAND = {0.4: (0.0010, 0.0003), 0.6: (0.0008, 0.0005), 1.0: (0.0047, 0.0004), 2.0: (0.0077, 0.0005)}


def jobs_for(T):
    import confirm13q as Q, confirm13 as K
    jobs = Q.q_jobs()
    bj = [j for j in jobs if K.is_bp(j) and j.key[1] == T and j.key[2] == 0.0][0]
    aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
    return dict(A=aj, B=bj)


def series(h, st, dl, units, dt):
    u = np.rint(np.asarray(h) / dt).astype(np.int64)
    assert np.all(np.diff(u) > 0)
    st, dl = np.asarray(st), np.asarray(dl)
    allu = np.arange(units)
    k = np.searchsorted(u, allu)
    ok = k < len(u)                                   # units after the last send were never dequeued (end of run)
    deq = st[np.minimum(k, len(u) - 1)]
    dlv = np.zeros(units, bool); dlv[u] = dl >= 0
    nw = int(math.ceil(units * dt / W - 1e-9)) + int(math.ceil(130 / W))
    out = {}
    for name, t in (("deq", deq[ok]), ("hand", allu[ok] * dt)):
        wi = np.minimum((t / W + 1e-9).astype(int), nw - 1)
        out[name] = dict(n=np.bincount(wi, minlength=nw).tolist(), d=np.bincount(wi[dlv[ok]], minlength=nw).tolist())
    return out


def one(args):
    T, nm, seed, outdir = args
    path = os.path.join(outdir, "q_%g_%s_%d.json" % (T, nm, seed))
    if os.path.exists(path):
        return path, 0.0
    import toy2
    j = jobs_for(T)[nm]
    t1 = time.time()
    s = toy2.Sim(j.builder(seed))
    out = s.run()
    n = s.n_sent
    rec = dict(T=T, design=nm, seed=seed, delivered=out["delivered"], units=s.units_total, dt=s.dt, wall_s=time.time() - t1,
               **series(s.h_seq[:n], s.sent_t[:n], s.dlv[:n], s.units_total, s.dt))
    json.dump(rec, open(path + ".tmp", "w")); os.replace(path + ".tmp", path)
    return path, rec["wall_s"]


def run(outdir, workers):
    import multiprocessing as mp
    os.makedirs(outdir, exist_ok=True)
    todo = [(T, nm, s, outdir) for T in TS for s in SEEDS for nm in ("A", "B")]
    t0 = time.time()
    with mp.Pool(workers) as pool:
        for i, (p, w) in enumerate(pool.imap_unordered(one, todo), 1):
            print("[%6.0f s] %d/%d %s (%.0f s)" % (time.time() - t0, i, len(todo), os.path.basename(p), w), flush=True)


def read(outdir):
    import confirm13q as Q, confirm13 as K, confirm3e as C, confirm_read as CR
    se = lambda v: float(np.std(v, ddof=1) / math.sqrt(len(v)))
    print("| T | runs identical to stored (delivered; hand-over item 2) | windows (s) | primary: (dA - dB')/(lam W) | share form: dA/nA - dB'/nB' | dequeue-count term | d_ss | hand-over reading | bar (2 se) | word |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for T in sorted(TS):
        jb = jobs_for(T)
        A, Bp = C.recs(jb["A"]), C.recs(jb["B"])
        on = float(np.mean([r["rev4"]["onset_t"] for r in Bp]))
        stored_def = K.window_shares(A) - K.window_shares(Bp)
        prim, share, cnt, hand_ok, dl_ok = [], [], [], 0, 0
        for s in SEEDS:
            ra = json.load(open(os.path.join(outdir, "q_%g_A_%d.json" % (T, s))))
            rb = json.load(open(os.path.join(outdir, "q_%g_B_%d.json" % (T, s))))
            dl_ok += (ra["delivered"] == A[s]["delivered"]) and (rb["delivered"] == Bp[s]["delivered"])
            a, _, _ = CR.pair_window(A[s], Bp[s], None)
            ids = [w for w in range(len(ra["deq"]["n"])) if w * W >= a - 1e-9 and (w + 1) * W <= on + 1e-9]
            hA = np.array(ra["hand"]["d"]) / np.maximum(ra["hand"]["n"], 1); hB = np.array(rb["hand"]["d"]) / np.maximum(rb["hand"]["n"], 1)
            hand_ok += abs((hA - hB)[ids].mean() - stored_def[s, ids].mean()) < 1e-12
            dA, dB = np.array(ra["deq"]["d"], float)[ids], np.array(rb["deq"]["d"], float)[ids]
            nA, nB = np.array(ra["deq"]["n"], float)[ids], np.array(rb["deq"]["n"], float)[ids]
            prim.append(((dA - dB) / (LAM * W)).mean())
            share.append((dA / nA - dB / nB).mean())
            cnt.append((dB / nB * (nB - LAM * W) / (LAM * W) - dA / nA * (nA - LAM * W) / (LAM * W)).mean())
        m, bar = float(np.mean(prim)), 2 * se(prim)
        h = HAND[T][0]
        fit, sep = abs(m - D_SS[T]) <= bar, abs(h - m) > bar
        word = "fail" if not fit else ("pass" if sep else "no power (control)" if T < 1 else "no power")
        print("| %g | %d/6; %d/6 | [%.1f, %.1f] (%d) | %.4f +- %.4f | %.4f +- %.4f | %+.4f | %.4f | %.4f +- %.4f | %.4f | %s |" % (
            T, dl_ok, hand_ok, ids[0] * W, (ids[-1] + 1) * W, len(ids), m, se(prim), np.mean(share), se(share), np.mean(cnt), D_SS[T], h, HAND[T][1], bar, word))
        print("   per seed primary:", " ".join("%.5f" % x for x in prim), "| mean %.5f, 2 se %.5f" % (m, bar))


if __name__ == "__main__":
    mode, outdir = sys.argv[1], os.path.abspath(sys.argv[2])
    if mode == "run":
        run(outdir, int(sys.argv[sys.argv.index("--workers") + 1]) if "--workers" in sys.argv else 4)
    else:
        read(outdir)
