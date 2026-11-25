"""Stage 3e identity check (three-knobs-stage3e-build.md 9 step 4; registration 9 steps 2-3 and 13.1).  One process at a time, alone on the machine.

  A. With every stage 3e field at its default, the code reproduces its base (proof-s1, b2addffb) on every key, trace and per-seq array
     of the proof-s1 sample (check_ident_proof_s1.py, which is run as a subprocess pair: old tree, new tree).  New keys and gauges are allowed.
  B. R0 (repair at r = 0) equals repair off on that sample plus one configuration per sender family (ident3e_cfgs), under id3e and in the
     sender trace and the event counters (no event pushed, no draw taken).
  C. An RL row at delta = 0.005 equals its OFF row (the law gives r = 0 exactly there): two configurations.

    python check_ident3e.py [BASE_REV]      # exit 0 = PASS; the last line is the result line
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


def main():
    base = sys.argv[1] if len(sys.argv) > 1 else BASE
    ok_a = part_a(base)
    n, bad_b = part_b()
    bad_c = part_c()
    ok = ok_a and not bad_b and not bad_c
    print("IDENTITY CHECK (stage 3e): %s (A: base %s on the proof-s1 sample %s; B: R0 equals repair off on %d configurations, %d differences; "
          "C: RL at delta 0.005 equals off, %d differences; id3e keys, sender traces and event counters; one process)" % (
              "PASS" if ok else "FAIL", base, "reproduced" if ok_a else "DIFFERS", n, len(bad_b), len(bad_c)))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
