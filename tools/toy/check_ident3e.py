"""Stage 3e identity check (three-knobs-stage3e-build.md 9 step 4; registration 9 steps 2-3 and 13.1).  One process at a time, alone on the machine.

  A. With every stage 3e field at its default, the code reproduces its base (proof-s1, b2addffb) on every key, trace and per-seq array
     of the proof-s1 sample (check_ident_proof_s1.py, which is run as a subprocess pair: old tree, new tree).  New keys and gauges are allowed.
  B. R0 (repair at r = 0) equals repair off on that sample plus one configuration per sender family (ident3e_cfgs), under id3e and in the
     sender trace and the event counters (no event pushed, no draw taken).
  C. An RL row at delta = 0.005 equals its OFF row (the law gives r = 0 exactly there): two configurations.

    python check_ident3e.py            # the sample: A, B, C.  Exit 0 = PASS.  Writes no marker: the registered gates are the full set below.
    python check_ident3e.py --reuse 3d=PATH,ps1=PATH   # registration 14.2: 3-seed sample of every reused fingerprint against its stored row; writes PASS-reuse
    python check_ident3e.py --full     # A, B, C, then D and E (built, long: run it only when the machine is free), and only then PASS-ident and PASS-r0:
        D. check_ident3d.py (the registered stage 3d configurations at seeds 0 to 2 against the stage 3c code; IDENT_WORKERS defaults to 1 here)
        E. R0 equals repair off (S5 at the cell's primary lifetime, hold 0) in every batch3d cell at seeds 0 to 2 and in every batch3e cell at seeds 0 to 11,
           under id3e, the sender trace and the event counters, every run finished.
A difference writes out3e/STOP with the cause (registration 9: steps 2 to 5 stop on any difference) and exits 1.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
BASE = "b2addffb"


def edge_of(cfg):
    return "worth" if cfg.sender == "S5" else ("evict" if cfg.budget == "resid" else "none")


def part_a(base):
    r = subprocess.run([sys.executable, os.path.join(HERE, "check_ident_proof_s1.py"), base], cwd=HERE)
    return r.returncode == 0


def sample_cfgs():
    import toy2
    import batch3c
    import batch3d
    import check_ident_proof_s1 as P
    import ident3e_cfgs
    batch3d.build()
    out = {}
    for nm in P.SAMPLE:
        cfg = batch3d.JOBS[nm][1](P.SEED)
        for k, v in P.SMALL.get(nm.split(".")[0], {}).items():
            setattr(cfg, k, v)
        out[nm] = cfg
    for nm, cfg in ident3e_cfgs.cfgs(toy2, batch3d, batch3c).items():
        out["i3e." + nm] = cfg
    return out


def run_sim(cfg):
    import toy2
    cfg.want_trace = True
    sm = toy2.Sim(cfg)
    out = sm.run()
    return out, (sm.ctr, sm.ps_ctr)


def compare(nm, a, b, ca, cb, bad, tag):
    import batch3e
    d = batch3e.id3e(a[0], b[0])
    if d:
        print("DIFF", tag, nm, d)
        bad.append((tag, nm))
    elif a[0]["_trace"] != b[0]["_trace"]:
        print("DIFF trace", tag, nm)
        bad.append((tag, nm))
    elif ca != cb:
        print("DIFF counters", tag, nm, ca, cb)
        bad.append((tag, nm))


def part_b():
    bad, n = [], 0
    for nm in sample_cfgs():
        off, c_off = run_sim(sample_cfgs()[nm])
        cfg = sample_cfgs()[nm]
        cfg.repair = dict(r=0.0, edge=edge_of(cfg))
        r0, c_r0 = run_sim(cfg)
        compare(nm, (off,), (r0,), c_off, c_r0, bad, "R0")
        assert r0["gauges"]["rep_credits"] == 0 and r0["gauges"]["rep_sent"] == 0, nm
        n += 1
        print("R0 =", "off" if not [b for b in bad if b[1] == nm] else "DIFF", nm, flush=True)
    return n, bad


def part_c():
    bad = []
    for nm in ("i3e.S5.CL@b", "i3e.S5.K-L10"):
        off, c_off = run_sim(_delta(sample_cfgs()[nm], None))
        rl, c_rl = run_sim(_delta(sample_cfgs()[nm], dict(r="law", edge="worth")))
        compare(nm, (off,), (rl,), c_off, c_rl, bad, "RL@0.005")
        assert rl["gauges"]["law_reads"] > 0 and rl["gauges"]["rep_credits"] == 0, nm
        print("RL at delta 0.005 =", "off" if not [b for b in bad if b[1] == nm] else "DIFF", nm, flush=True)
    return bad


def _delta(cfg, rp):
    cfg.delta = 0.005
    cfg.repair = rp
    return cfg


def part_d():
    env = dict(os.environ)
    env.setdefault("IDENT_WORKERS", "1")                                    # one worker at a time (memory)
    return subprocess.run([sys.executable, os.path.join(HERE, "check_ident3d.py")], cwd=HERE, env=env).returncode == 0


def part_e():
    """R0 against repair off over the registered cells.  Returns (runs compared, failures)."""
    import batch3d
    import batch3e
    batch3d.build()
    bad, n = [], 0

    def pair(nm, off_cfg, r0_cfg):
        nonlocal n
        off, c_off = run_sim(off_cfg)
        r0, c_r0 = run_sim(r0_cfg)
        compare(nm, (off,), (r0,), c_off, c_r0, bad, "R0")
        if r0.get("finished") is False or r0["gauges"]["rep_credits"]:
            bad.append(("R0 liveness", nm))
        n += 1
        print("R0 =", "off" if not [b for b in bad if b[1] == nm] else "DIFF", nm, flush=True)

    for cell in batch3d.CELLS:
        for seed in range(3):
            mk = (lambda: batch3d.p2_cfg(seed, 0.4, 0.0, "est")) if cell == "P2" else (lambda: batch3d.s5_cfg(cell, seed, 0.4, 0.0))
            r0 = mk()
            r0.repair = dict(r=0.0, edge="worth")
            pair("%s seed %d" % (cell, seed), mk(), r0)
    for cell in batch3e.CELLS:
        T = batch3e.T_MAIN.get(cell, 0.4)
        for seed in range(12):
            pair("%s seed %d (new cell)" % (cell, seed), batch3e.new_cfg(cell, seed, T, 0.0, 50.0, None), batch3e.new_cfg(cell, seed, T, 0.0, 50.0, batch3e.rp(0.0)))
    return n, bad


def reuse_check(fps, seeds=(0, 1, 2)):
    """Registration 14.2: every reused fingerprint is re-run at 3 seeds on this code and compared with its stored result under id3e (the stored
    stage 3d and proof stage 1 outputs keep no trace, so the comparison is every key id3e compares, the keys of the sender's trace summary included).
    Returns the differences."""
    import batch3e
    bad = []
    for fp in fps:
        for seed in seeds:
            old = batch3e.stored_result(fp, seed)
            if old is None:
                continue
            new = batch3e.run_one(batch3e.FPS[fp]["builder"](seed), False)
            d = batch3e.id3e(old, new)
            if d or new.get("finished") != old.get("finished"):
                bad.append((fp, seed, d))
                print("DIFF reuse", fp, seed, d, flush=True)
    return bad


def part_reuse(specs):
    """--reuse 3d=PATH,ps1=PATH: reproduce a 3-seed sample of every fingerprint the registry shares with the stored rows; PASS writes PASS-reuse."""
    import batch3e
    batch3e.build()
    batch3e.set_reuse([x for x in specs.split(",") if x])
    fps = [fp for fp in batch3e.FPS if fp in batch3e.REUSE["index"] and batch3e.FPS[fp]["builder"](0).repair is None]
    print("%d fingerprints are shared with the stored rows" % len(fps), flush=True)
    bad = reuse_check(fps)
    if bad:
        stop("reuse: %d runs differ from the stored rows, first %s" % (len(bad), bad[0]))
        print("REUSE CHECK: FAIL")
        return 1
    out = os.path.join(HERE, "out3e")
    os.makedirs(out, exist_ok=True)
    open(os.path.join(out, "PASS-reuse"), "w").close()
    print("REUSE CHECK: PASS (%d fingerprints x seeds 0 to 2 reproduced, every key under id3e)" % len(fps))
    return 0


def stop(cause):
    out = os.path.join(HERE, "out3e")
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "STOP"), "w") as f:
        f.write("check_ident3e: %s" % cause + chr(10))


def main():
    if "--reuse" in sys.argv:
        sys.exit(part_reuse(sys.argv[sys.argv.index("--reuse") + 1]))
    base = BASE
    full = "--full" in sys.argv
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if args:
        base = args[0]
    ok_a = part_a(base)
    n, bad_b = part_b()
    bad_c = part_c()
    ok_d = ok_e = None
    if full and ok_a and not bad_b and not bad_c:
        ok_d = part_d()
        n_e, bad_e = part_e()
        ok_e = not bad_e
    ok = ok_a and not bad_b and not bad_c and ok_d is not False and ok_e is not False
    if not ok:
        stop("A %s; B %d differences; C %d differences; D %s; E %s" % (ok_a, len(bad_b), len(bad_c), ok_d, ok_e))
    elif full:
        out = os.path.join(HERE, "out3e")
        os.makedirs(out, exist_ok=True)
        open(os.path.join(out, "PASS-ident"), "w").close()                  # registration 9 step 2
        open(os.path.join(out, "PASS-r0"), "w").close()                     # registration 9 step 3
    print("IDENTITY CHECK (stage 3e, %s): %s (A: base %s on the proof-s1 sample %s; B: R0 equals repair off on %d configurations, %d differences; "
          "C: RL at delta 0.005 equals off, %d differences; D, E: %s; id3e keys, sender traces and event counters; one process)" % (
              "full" if full else "sample, no marker written", "PASS" if ok else "FAIL", base, "reproduced" if ok_a else "DIFFERS", n, len(bad_b),
              len(bad_c), "not run" if ok_d is None else ("d %s, e %s" % (ok_d, ok_e))))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
