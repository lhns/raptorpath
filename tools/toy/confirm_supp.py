"""Supplementary set, labelled post-data (results-confirm3e.md, 2026-11-26): build A at T = 0.4, 0.6, 0.8, H = 0, 12 seeds, in CL-long-v.
The registered CL-long-v sweep starts at T = 1 (delivered 0.971), above today's Realtime (0.925), so the matched-share reading of recovery against it
read the curve's lowest point.  These points extend the curve down to the baseline's share.  Prediction written before running: at T = 0.4,
recovery ~ 0 s and on time >= 0.925.  The registered reading stays as it is; this prints a second reading beside it.
Run after the chain ends:  python confirm_supp.py [--workers N]"""
import multiprocessing as mp
import sys

import batch3e as B

EXTRA_T = [0.4, 0.6, 0.8]
_T, _H = B.SWEEP["CL-long-v"]
B.SWEEP["CL-long-v"] = (sorted(set(EXTRA_T) | set(_T), key=lambda x: (x == float("inf"), x)), _H)   # module level: spawned workers see it too

import confirm3e as C  # noqa: E402


def main():
    w = int(sys.argv[sys.argv.index("--workers") + 1]) if "--workers" in sys.argv else 4
    B.build()
    jobs = C.jobs_of("CL-long-v")
    new = [j for j in jobs if j.design == "A" and j.key[2] == 0.0 and j.key[1] in EXTRA_T]
    reg = dict(out=C.OUT, smoke={})
    for j in C.all_jobs():
        C.JOBS_BY_FP[j.fp] = j
    with mp.Pool(min(w, 9), initializer=C.init_worker, initargs=(reg,)) as pool:
        C.run_pool(pool, C.tasks_of(new), "CL-long-v supp")
    import json
    nf = json.load(open(C.nf_path()))
    lines, undet = C.read_cell("CL-long-v", jobs, nf)
    print("\n".join(lines))
    print("undetermined:", sorted(undet))


if __name__ == "__main__":
    mp.freeze_support()
    main()
