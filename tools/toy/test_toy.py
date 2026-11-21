"""Unit tests for the stage-1 toy.  Run: python -m unittest tools/toy/test_toy.py  (from the repo root)
or  python test_toy.py  from tools/toy.  Short runs (3-6 s of stream) keep them fast."""
import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lp  # noqa: E402
import toy  # noqa: E402

COUNTED = ["S1", "S1c", "S2", "S3"]


def short(cell, cand, seed=1, **kw):
    kw.setdefault("dur", 5.0)
    return toy.make_cfg(cell, cand, seed, **kw)


class Determinism(unittest.TestCase):
    def test_same_seed_same_result(self):
        a = toy.run(short("G", "S3", 3))
        b = toy.run(short("G", "S3", 3))
        self.assertEqual(a, b)

    def test_crn_originals_identical_across_candidates(self):
        a = toy.run(short("I2", "S0", 2))
        b = toy.run(short("I2", "S1c", 2))
        self.assertEqual(a["orig_lost"], b["orig_lost"])
        self.assertEqual(a["n_holes"], b["n_holes"])


class P1Promise(unittest.TestCase):
    """G_rx(t) <= floor((1-rho) S_src(t)), checked inline at every pass (the only place G rises)."""

    def test_promise_every_instant(self):
        for cell in ["I2", "G", "B10"]:
            for cand in COUNTED:
                for topo in ["n1", "n2"]:
                    kw = dict(dur=12.0) if cell == "B10" else {}
                    r = toy.run(toy.make_cfg(cell, cand, 1, topo=topo, phi_slow=0.5, **{**dict(dur=5.0), **kw}))
                    g = r["gauges"]
                    self.assertEqual(g["p1_viol"], 0, (cell, cand, topo))
                    self.assertLessEqual(r["G_final"], int(0.01 * r["n_src"] + 1e-9))

    def test_checker_can_fail(self):
        """Positive control: a sender that ignores the count must trip the check."""
        cfg = short("I2", "S1")
        sim = toy.Sim(cfg)
        sim.allow = lambda: 10 ** 9           # sender believes the count is unlimited
        r = sim.run()
        self.assertGreater(r["gauges"]["p1_viol"], 0)


class P2Identity(unittest.TestCase):
    def test_rho_one_traces_equal_s0(self):
        for rho in (1.0, 1.0 - 1e-7):
            ref = toy.run(short("I2", "S0", 1, rho=rho, want_trace=True))["_trace"]
            for cand in COUNTED:
                tr = toy.run(short("I2", cand, 1, rho=rho, want_trace=True))["_trace"]
                self.assertEqual(tr, ref, (cand, rho))

    def test_rho_below_one_differs(self):
        ref = toy.run(short("I2", "S0", 1, want_trace=True))["_trace"]
        tr = toy.run(short("I2", "S1c", 1, want_trace=True))["_trace"]
        self.assertNotEqual(tr, ref)


class LPBound(unittest.TestCase):
    def test_lp_below_every_candidate(self):
        for cell in ["I2", "G"]:
            for H in (0.0, 0.005):
                r0 = toy.run(short(cell, "S0", 1, dur=4.0, H=H, want_trace=True))
                a, X, p, st = lp.instance_from_run(r0, 5.0)
                v, s0, _ = lp.solve(a, X, p, st, 0.99)
                self.assertLessEqual(v, s0 + 1e-6)
                for cand in COUNTED:
                    rc = toy.run(short(cell, cand, 1, dur=4.0, H=H))
                    self.assertLessEqual(v, rc["hol_sum_nonhole"] + 1e-6, (cell, H, cand))

    def _tiny(self, rng, n):
        """Random tiny path: packets at 1 ms spacing, a few holes with fill times and pass instants."""
        a = np.array([i * 0.001 + 0.005 for i in range(n)], float)
        X = np.full(n, np.nan)
        p = np.full(n, np.nan)
        holes = rng.choice(n - 2, size=rng.integers(1, min(7, n - 2)), replace=False)
        for s in holes:
            a[s] = np.nan
            X[s] = a[s - 1] if False else (s * 0.001 + 0.005 + rng.uniform(0.004, 0.02))
            p[s] = s * 0.001 + 0.005 + rng.uniform(0.0, 0.004)
        sent = np.array([i * 0.001 for i in range(n)], float)
        return a, X, p, sent

    def test_milp_equals_brute_and_lp_below(self):
        rng = np.random.default_rng(5)
        n_slack = n_bind = 0
        for trial in range(40):
            n = int(rng.integers(8, 40))
            a, X, p, sent = self._tiny(rng, n)
            rho = 0.9                         # budget of 1 hole only once 10 packets are sent
            bf = lp.brute(a, X, p, sent, rho)
            m, _, _ = lp.solve(a, X, p, sent, rho, integer=True)
            l, s0, _ = lp.solve(a, X, p, sent, rho)
            self.assertAlmostEqual(m, bf, places=6, msg=trial)
            self.assertLessEqual(l, bf + 1e-6)
            if abs(l - bf) < 1e-6:
                n_slack += 1
            else:
                n_bind += 1
        self.assertGreater(n_slack, 0)

    def test_lp_equals_brute_when_count_slack(self):
        rng = np.random.default_rng(9)
        for trial in range(20):
            a, X, p, sent = self._tiny(rng, 30)
            rho = 0.0                          # budget never binds
            self.assertAlmostEqual(lp.solve(a, X, p, sent, rho)[0], lp.brute(a, X, p, sent, rho), places=6)

    def test_two_hole_budget_one_lp_gap(self):
        """Two overlapping holes, budget 1: HOL is convex in pass time, so the LP may split the unit."""
        a = np.array([0.0, np.nan, np.nan, 0.001, 0.002, 0.003, 0.004, 0.005, 0.006, 0.007, 0.008, 0.009])
        X = np.array([np.nan, 0.020, 0.030] + [np.nan] * 9)
        p = np.array([np.nan, 0.002, 0.002] + [np.nan] * 9)
        sent = np.arange(12) * 0.001
        bf = lp.brute(a, X, p, sent, 0.9)
        m = lp.solve(a, X, p, sent, 0.9, integer=True)[0]
        l = lp.solve(a, X, p, sent, 0.9)[0]
        self.assertAlmostEqual(m, bf, places=6)
        self.assertLessEqual(l, bf + 1e-9)


class P4aSanity(unittest.TestCase):
    def test_s1_isolated_stall_exceeds_s0_by_H(self):
        H = 0.005
        r0 = toy.run(short("I1", "S0", 1, dur=10.0, H=H))
        r1 = toy.run(short("I1", "S1", 1, dur=10.0, H=H))
        self.assertAlmostEqual(r1["iso_p50"] - r0["iso_p50"], H, delta=0.001)


if __name__ == "__main__":
    unittest.main()
