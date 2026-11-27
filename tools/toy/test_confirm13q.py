"""Tests of the OV-long-q driver (confirm13q.py): the registry against 13.9.6, the gauge reruns, the model of 13.9 against the numbers it must reproduce."""
import dataclasses
import math
import os
import unittest

import numpy as np

import batch3e as B
import confirm13 as K
import confirm13q as Q
import confirm3e as C

INF = math.inf


class Registry(unittest.TestCase):
    def test_run_counts(self):
        pj = Q.part_jobs()
        n = {p: K.runs_of(js) for p, js in pj.items()}
        self.assertEqual(n["AA-q"], 24)
        fresh = sum(j.nseeds for j in pj["OV-long-q"] if j.design in ("adr", "bulk"))
        self.assertEqual(fresh, 9)                                               # ADR-0084 at two rho and Bulk, 3 seeds each
        self.assertEqual(n["OV-long-q"] - fresh, 30 + 30 + 6)                    # A and B' at five lifetimes x 6 seeds, rt and rteq x 3
        self.assertEqual(n["RESEND-GAUGE"], (4 + 4) * 2 * Q.GAUGE_SEEDS)         # OV-long-v and OV-long-m, four finite lifetimes each, A and B', 3 seeds

    def test_the_cell_is_ov_long_at_delta_0_118(self):
        self.assertEqual(B.delta_of(Q.Q), 0.118)
        self.assertEqual(B.delta_of(K.M), 0.1)
        self.assertEqual(B.delta_of("OV-long"), 50.0)
        self.assertEqual(B.tau_of(Q.Q), 0.4)
        self.assertEqual(B.SWEEP[Q.Q], B.SWEEP["OV-long"])
        self.assertEqual(B.CELLS[Q.Q], B.CELLS["OV-long"])
        self.assertEqual([k[1] for k in Q.q_keys()], [0.4, 0.6, 1.0, 2.0, INF])
        rows = {(j.design, j.key[1]): j for j in Q.q_jobs() if j.design in ("A", "B")}
        for (d, T), j in rows.items():
            c = j.builder(0)
            self.assertEqual((c.delta, c.T, c.resend_gauge), (0.118, T, True))
            self.assertEqual(c.wait_limit, d == "B")
        rt = [j for j in Q.q_jobs() if j.design in ("rt", "rteq")]
        self.assertTrue(all(j.builder(0).delta == 0.118 and j.nseeds == 3 and not j.builder(0).resend_gauge for j in rt))

    def test_aa_pair_is_a_at_the_primary_lifetime_and_its_phase_variant(self):
        a0, a1 = Q.aa_q()
        c0, c1 = a0.builder(0), a1.builder(0)
        self.assertEqual((c0.T, c0.delta, c0.variant, c1.variant, a0.nseeds), (0.4, 0.118, 0, 1, 12))
        self.assertEqual(dataclasses.replace(c1, variant=0), c0)

    def test_gauge_rows_are_the_stored_configurations_with_the_gauge_on(self):
        pairs = Q.stored_pairs()
        self.assertEqual(len(pairs), 16)
        for sj, gj in pairs:
            self.assertEqual(dataclasses.replace(gj.builder(0), resend_gauge=False), sj.builder(0))
            self.assertTrue(gj.builder(0).resend_gauge)
            self.assertNotEqual(gj.fp, sj.fp)
            self.assertEqual((gj.design, gj.key, gj.cell), (sj.design, sj.key, sj.cell))
            self.assertEqual(gj.nseeds, 3)
            self.assertTrue(sj.key[1] != INF and sj.key[2] == 0.0)
        self.assertEqual({sj.cell for sj, _ in pairs}, {"OV-long-v", K.M})

    def test_stored_fingerprints_are_unchanged_by_the_gauge_field(self):
        """The field is at its default in every stored row, so it is left out of the hash: every stored row of the earlier cells is still in the store."""
        probe = C.jobs_of("OV-long")[0]
        if not C.have(probe.fp, 0):
            self.skipTest("no stored runs")
        for cell in K.OV_CELLS:
            for j in C.jobs_of(cell):
                self.assertTrue(all(C.have(j.fp, s) for s in range(j.nseeds)), j.name)
        for j in K.m_jobs():
            if j.design in ("A", "B"):
                self.assertFalse(j.builder(0).resend_gauge)


class Model(unittest.TestCase):
    def test_s_b_and_onset_are_the_registered_ones(self):
        for T, p in Q.PRED.items():
            self.assertAlmostEqual(Q.s_b_of(T, Q.Q_DELTA), p["s_b"], delta=0.02 * p["s_b"] + 0.01, msg=T)
            self.assertAlmostEqual(Q.REG["t0"] + Q.s_b_of(T, Q.Q_DELTA), p["onset"], delta=0.06, msg=T)
        self.assertAlmostEqual(Q.s_b_of(2.0, Q.Q_DELTA) / Q.s_b_of(0.4, Q.Q_DELTA), Q.RATIO_PRED, delta=0.03)

    def test_p50_on_the_readers_convention_reproduces_the_registered_values(self):
        """13.9.5 (OV-long-m, delta 0.1) and 13.9.6 (delta 0.118): the model on A's window starts, within the registered bands."""
        starts = {0.4: 2.9, 0.6: 3.9, 1.0: 6.0, 2.0: 11.0}
        m = {0.4: (0.196, 0.015), 0.6: (0.318, 0.015), 1.0: (0.634, 0.03), 2.0: (1.63, 0.10)}
        for T, (v, band) in m.items():
            self.assertAlmostEqual(Q.model_p50(T, 0.1, starts[T], 39.6, 0.797), v, delta=band, msg=("m", T))
        for T, p in Q.PRED.items():
            self.assertAlmostEqual(Q.model_p50(T, Q.Q_DELTA, starts[T], 39.6, p["share"]), p["p50"], delta=p["p50_band"], msg=("q", T))

    def test_p50_is_infinite_when_half_the_units_are_not_delivered(self):
        self.assertEqual(Q.model_p50(0.4, 0.118, 3.0, 39.6, 0.5), math.inf)

    def test_law_deficit_is_zero_before_the_overload_and_rises_to_a_peak_after_the_onset(self):
        T, d = 0.4, Q.Q_DELTA
        v = Q.law_deficit(T, d, Q.REG["t0"], Q.REG["x"], Q.REG["o"], Q.REG["R"], 0.8, 80)
        self.assertTrue(np.all(v[:2] == 0.0))
        onset = Q.REG["t0"] + Q.s_b_of(T, d)
        self.assertEqual(float(v[int((onset - 0.6) / 0.5)]), 0.0)                  # the cap is flat (min(1, .)) until y = 1
        k = int(onset / 0.5) + 1
        self.assertGreater(v[k], 0.015)
        self.assertGreater(v[k], v[-1])                                             # and it falls after the peak
        # the value just after the onset: (mu / lambda) |w'| / (1 + |w'|), |w'| = (c / x) / (2 s_b) at y = 1
        wp = (T - Q.REG["R"]) / Q.REG["x"] / (2 * Q.s_b_of(T, d))
        self.assertAlmostEqual(0.8 * wp / (1 + wp), v[k], delta=0.004)

    def test_tail_law_is_w_t_ov_of_the_cap(self):
        T, d, c = 0.4, 0.118, 0.296
        for t_ov in (10.0, 20.0, 35.0):
            w = Q.w_of(T, d, t_ov)
            self.assertAlmostEqual(w * t_ov, Q.tail_law(c, Q.REG["x"], Q.REG["o"], d, t_ov), places=9)


class Reader(unittest.TestCase):
    def test_reading_of_the_stored_ov_long_m_rows_reproduces_13_9(self):
        """The corrected tests on the stored OV-long-m rows (no gauge: the resend term is the whole-run spread): items 1, 3 and 4 and the bill hold at every T."""
        jobs = K.m_jobs()
        if not all(C.recs(j) for j in jobs if j.design == "A"):
            self.skipTest("no stored runs")
        _, flags, sbs = Q.tests_for(K.M, 0.1, jobs, None, {})
        for T in ("0p4", "0p6", "1", "2"):
            self.assertTrue(flags["item1@" + T], T)
            self.assertTrue(flags["item3@" + T], T)
            self.assertTrue(flags["bill@" + T], T)
        for T in ("0p4", "0p6", "1"):
            self.assertTrue(flags["item4@" + T], T)
        self.assertIsNone(flags["item4@2"])                                         # the tail starts at t0 + 2 s_b = 41.6 s, after the window
        self.assertTrue(flags["ratio"])
        self.assertAlmostEqual(sbs[2.0] / sbs[0.4], 4.88, delta=0.1)


class GaugeRead(unittest.TestCase):
    def test_gauge_series_columns(self):
        r = dict(series=dict(handed=[0] * 4), resend_outcome=dict(series=[[0.0, 3, 1, 2], [1.0, 5, 0, 1]]))
        np.testing.assert_array_equal(Q.gauge_series([r], 1), [[3, 0, 5, 0]])
        np.testing.assert_array_equal(Q.gauge_series([r], 2), [[1, 0, 0, 0]])
        np.testing.assert_array_equal(Q.gauge_series([r], 3), [[2, 0, 1, 0]])
        self.assertIsNone(Q.gauge_series([dict(series=dict(handed=[0] * 4))], 1))

    def test_resend_term_is_kappa_times_extra_resends_per_handed_unit(self):
        mk = lambda rs: dict(series=dict(handed=[100, 100]), gauges=dict(repair_sends=sum(rs)), resend_outcome=dict(series=[[0.0, rs[0], 0, 0], [0.5, rs[1], 0, 0]]))        # noqa: E731
        term, have = Q.resend_term([mk([0, 0])], [mk([100, 20])])
        self.assertTrue(have)
        np.testing.assert_allclose(term, [Q.KAPPA_R, 0.2 * Q.KAPPA_R])
        a = dict(series=dict(handed=[100, 100]), gauges=dict(repair_sends=0))
        b = dict(series=dict(handed=[100, 100]), gauges=dict(repair_sends=120))
        term, have = Q.resend_term([a], [b])
        self.assertFalse(have)
        np.testing.assert_allclose(term, [Q.KAPPA_R * 0.6] * 2)


if __name__ == "__main__":
    unittest.main()
