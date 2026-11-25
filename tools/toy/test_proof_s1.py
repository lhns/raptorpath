"""Tests for the proof stage 1 reader and builders (proof_s1.py), on synthetic curves.  The reader is fixed before any run (plan F5).
Run from tools/toy:  python -m unittest test_proof_s1"""
import math
import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import proof_s1 as P  # noqa: E402
import toy2  # noqa: E402


def toy2_sim(c):
    return toy2.Sim(c)


def toy2_b(d):
    return toy2.b_of_delta(d)

SEEDS = 12
rng = np.random.default_rng(0)


def pts(xs, ys, jit=1e-4):
    """(points, seeds) arrays: each point's value plus a small per-seed jitter."""
    xs, ys = np.asarray(xs, float), np.asarray(ys, float)
    return xs[:, None] + jit * rng.standard_normal((len(xs), SEEDS)), ys[:, None] + jit * rng.standard_normal((len(xs), SEEDS))


class Reader(unittest.TestCase):
    def test_sign_per_readout(self):
        nx, ny = pts([0.90, 0.95, 1.00], [0.60, 0.70, 0.80])           # on time rises with delivered share
        kk = P.merge_points(nx, ny, +1.0, 0.0)
        bx = np.full(SEEDS, 0.95)
        better = P.compare(nx, ny, kk, bx, np.full(SEEDS, 0.60), "share_by_0p4", 0.0, 0.0, 200000, 0.6)
        worse = P.compare(nx, ny, kk, bx, np.full(SEEDS, 0.80), "share_by_0p4", 0.0, 0.0, 200000, 0.8)
        self.assertEqual(better["word"], "shown better")
        self.assertEqual(worse["word"], "shown worse")
        rx, ry = pts([0.90, 0.95, 1.00], [1.0, 3.0, 5.0])              # recovery grows with delivered share; lower is better
        rk = P.merge_points(rx, ry, -1.0, 0.0)
        lo = P.compare(rx, ry, rk, bx, np.full(SEEDS, 6.0), "recovery_time", 0.0, 0.0, 200000, 6.0)    # the new model recovers in 3 s, the baseline in 6
        hi = P.compare(rx, ry, rk, bx, np.full(SEEDS, 1.0), "recovery_time", 0.0, 0.0, 200000, 1.0)
        self.assertEqual(lo["word"], "shown better")
        self.assertEqual(hi["word"], "shown worse")

    def test_the_reverse_direction_has_the_same_sign_convention(self):
        ax, ay = pts([0.90, 1.00], [0.50, 0.60])                       # the baseline curve
        ak = P.merge_points(ax, ay, +1.0, 0.0)
        pt_x = np.full(SEEDS, 0.95)
        up = P.compare_rev(ax, ay, ak, pt_x, np.full(SEEDS, 0.90), "share_by_0p4", 0.0, 200000, 0.55)
        down = P.compare_rev(ax, ay, ak, pt_x, np.full(SEEDS, 0.20), "share_by_0p4", 0.0, 200000, 0.55)
        self.assertEqual((up["word"], down["word"]), ("shown better", "shown worse"))
        rup = P.compare_rev(ax, ay, ak, pt_x, np.full(SEEDS, 0.0), "recovery_time", 0.0, 200000, 0.55)   # the point recovers in 0 s against ~0.55: better
        self.assertEqual(rup["word"], "shown better")

    def test_equivalent_inside_the_bar(self):
        nx, ny = pts([0.90, 1.00], [0.600, 0.602])
        kk = P.merge_points(nx, ny, +1.0, 0.0)
        rd = P.compare(nx, ny, kk, np.full(SEEDS, 0.95), np.full(SEEDS, 0.601), "share_by_0p4", 0.0, 0.0, 200000, 0.6)
        self.assertEqual(rd["word"], "equivalent")

    def test_above_the_range_is_outside_and_below_it_reads_the_lowest_point(self):
        nx, ny = pts([0.94, 0.97, 1.00], [0.9, 0.95, 1.0])
        kk = P.merge_points(nx, ny, +1.0, 0.0)
        rd = P.compare(nx, ny, kk, np.full(SEEDS, 1.005), np.full(SEEDS, 0.5), "share_by_0p4", 0.0, 0.0, 200000, 0.5)
        self.assertEqual(rd["word"], "outside the swept range")       # above the curve: no extrapolation
        # amendment 1.6c: below the lowest delivered share the baseline is compared against the lowest point
        low_ok = P.compare(nx, ny, kk, np.full(SEEDS, 0.90), np.full(SEEDS, 0.5), "share_by_0p4", 0.0, 0.0, 200000, 0.5)
        self.assertEqual((low_ok["word"], low_ok["n_clamped"]), ("shown better", SEEDS))
        low_bad = P.compare(nx, ny, kk, np.full(SEEDS, 0.90), np.full(SEEDS, 0.99), "share_by_0p4", 0.0, 0.0, 200000, 0.99)
        self.assertEqual(low_bad["word"], "shown worse")               # the lowest point is worse on on-time share: a loss, not a pass
        rx, ry = pts([0.94, 0.97, 1.00], [0.0, 2.0, 4.0])              # recovery: the lowest point recovers in 0 s
        rk = P.merge_points(rx, ry, -1.0, 0.0)
        r = P.compare(rx, ry, rk, np.full(SEEDS, 0.90), np.full(SEEDS, 4.0), "recovery_time", 0.0, 0.0, 200000, 4.0)
        self.assertEqual(r["word"], "shown better")
        # a mix: some seeds below, the rest inside: still read, and the clamped count is reported
        bx = np.full(SEEDS, 0.96)
        bx[:3] = 0.90
        rd = P.compare(nx, ny, kk, bx, np.full(SEEDS, 0.5), "share_by_0p4", 0.0, 0.0, 200000, 0.5)
        self.assertEqual(rd["n_clamped"], 3)

    def test_undetermined_is_its_own_outcome(self):
        nx, ny = pts([0.90, 1.00], [0.60, 0.60], jit=0.0)
        kk = P.merge_points(nx, ny, +1.0, 0.0)
        by = np.full(SEEDS, 0.60) + np.linspace(-0.3, 0.3, SEEDS)      # wide spread around no difference, wider than the bar
        rd = P.compare(nx, ny, kk, np.full(SEEDS, 0.95), by, "share_by_0p4", 0.0, 0.0, 200000, 0.6)
        self.assertEqual(rd["word"], "undetermined")

    def test_classify(self):
        def c(word, scope="verdict"):
            return dict(scope=scope, rd=dict(word=word))
        self.assertEqual(P.classify([c("shown better"), c("equivalent")], False)[0], "PASSES")
        self.assertEqual(P.classify([c("shown better"), c("undetermined")], False)[0], "UNDETERMINED")
        self.assertEqual(P.classify([c("undetermined"), c("shown worse")], False)[0], "FAILS")
        self.assertEqual(P.classify([c("outside the swept range")], False)[0], "FAILS")
        self.assertEqual(P.classify([c("equivalent")], True)[0], "FAILS")                      # a gate
        self.assertEqual(P.classify([c("shown worse", "reported"), c("equivalent")], False)[0], "PASSES")   # today's Realtime is reported only
        self.assertEqual(P.classify([c("undetermined", "reported")], False)[0], "PASSES")

    def test_bracket(self):
        means = np.array([0.94, 0.97, 1.0])
        self.assertEqual(P.bracket(means, [0, 1, 2], 0.96), [0, 1])
        self.assertEqual(P.bracket(means, [0, 1, 2], 0.90), [0])
        self.assertEqual(P.bracket(means, [0, 1, 2], 0.985), [1, 2])

    def test_points_within_noise_keep_the_better_y(self):
        xs, ys = pts([0.900, 0.9004, 0.95, 1.0], [0.50, 0.58, 0.70, 0.80], jit=0.0)
        kept = P.merge_points(xs, ys, +1.0, 0.001)
        self.assertEqual(sorted(kept), [1, 2, 3])                      # the better of the first two (0.58) is kept
        kept = P.merge_points(xs, ys, -1.0, 0.001)                     # lower is better: the other one
        self.assertEqual(sorted(kept), [0, 2, 3])
        kept = P.merge_points(xs, ys, +1.0, 0.0)                       # no noise: nothing merges
        self.assertEqual(sorted(kept), [0, 1, 2, 3])

    def test_interpolation_is_per_seed(self):
        xs = np.stack([np.linspace(0.9, 1.0, 3) + 0.01 * j for j in range(SEEDS)], axis=1)
        ys = np.stack([np.linspace(0.0, 1.0, 3) for _ in range(SEEDS)], axis=1)
        out = P.curve_at(xs, ys, [0, 1, 2], xs[1] - 0.025)
        self.assertTrue(np.allclose(out, 0.25))                        # each seed's own x grid

    def test_a_censored_recovery_is_counted_never_dropped(self):
        self.assertEqual(P.val(dict(recovery_time=None), "recovery_time"), P.REC_CAP)
        self.assertEqual(P.val(dict(recovery_time=4.8), "recovery_time"), 4.8)
        self.assertEqual(P.val(dict(share_by_0p4=0.5, gauges={}), "share_by_0p4"), 0.5)
        self.assertEqual(P.val(dict(gauges={"commit_viol": 0}), "commit_viol"), 0.0)

    def test_bars(self):
        self.assertAlmostEqual(P.t_bar("share_by_0p4", 0.6, 200000), 0.02)
        self.assertAlmostEqual(P.t_bar("recovery_time", 4.8, 200000), 0.24)
        self.assertAlmostEqual(P.t_bar("recovery_time", 0.5, 200000), 0.1)        # the 0.1 s floor
        self.assertAlmostEqual(P.t_bar("share_by_0p4", 1.0, 200000), 1.0 / 200000)


class Builders(unittest.TestCase):
    def test_the_registered_sets(self):
        self.assertEqual(len(P.T_SET), 15)
        self.assertEqual(P.T_FINE, [1.2, 1.25, 1.3, 1.35, 1.4, 1.45, 1.5, 1.55, 1.6])
        P.build()
        dec = [n for n, (p, _) in P.JOBS.items() if p == "DECIDE"]
        self.assertEqual(len(dec), 15 + 3 + 5 + 3 + 1)                # 18 new-model rows, 9 old-design rows, plus today's Bulk (the 27 of the plan, repair off)
        self.assertTrue(all("(" in n for n in dec))                    # F6: every name says what it does, the code name in parentheses

    def test_every_arm_is_paired_on_the_same_cell_and_seeds(self):
        P.build()
        cells = set()
        for n, (p, fn) in P.JOBS.items():
            c = fn(3)
            cells.add((c.cell, c.seed, c.n_total, c.dead, tuple(sorted(c.link.items())), c.sweep_form, c.rtt_est, c.resend_brake, c.resend_gate))
        self.assertEqual(len(cells), 1, cells)                         # one cell, one seed, one sweep form for every arm

    def test_every_arm_uses_the_rfc_estimator_and_todays_realtime_keeps_its_deadline_seed(self):
        P.build()
        for n, (p, fn) in P.JOBS.items():
            self.assertEqual(fn(0).rtt_est, "rfc", n)                   # same machinery: S5, ADR-0084, today's Realtime, Bulk, A/A
        sm = toy2_sim(P.rt_cfg(0, 50.0))
        self.assertIsNone(sm.srtt)
        d, _ = sm.D_now()
        self.assertAlmostEqual(d, min(toy2_b(50.0), 2.0) * 0.050)       # the 50 ms seed on the deadline until the first sample
        sm.rtt_sample(0.010)
        self.assertAlmostEqual(sm.D_now()[0], min(toy2_b(50.0), 2.0) * 0.010)

    def test_two_aa_pairs_and_the_floor_is_the_larger(self):
        P.build()
        self.assertEqual(len(P.aa_pairs()), 2)
        for a, b in P.aa_pairs():
            ca, cb = P.JOBS[a][1](0) if a in P.JOBS else None, P.JOBS[b][1](0)
            self.assertEqual(cb.variant, 1)
            self.assertEqual(ca.variant, 0)
            self.assertEqual((ca.T, ca.Hk), (cb.T, cb.Hk))
        self.assertEqual({P.JOBS[b][0] for _, b in P.aa_pairs()}, {"AA"})

    def test_todays_realtime_row_carries_all_its_differences(self):
        c = P.rt_cfg(0, 50.0)
        self.assertEqual((c.budget, c.d_arm, c.floor_send, c.evict_window, c.r0e_max, c.sender, c.recv),
                         ("resid", "EWMA", False, 200, 500, "S1", "R0e"))
        self.assertGreater(c.pause_B, 10 ** 6)                          # no source pause
        a = P.adr_cfg(0, 0.99)
        self.assertEqual((a.budget, a.d_arm, a.floor_send, a.evict_window, a.sender), ("rho", "RT", True, math.inf, "S1c"))


if __name__ == "__main__":
    unittest.main()
