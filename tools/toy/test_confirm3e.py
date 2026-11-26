"""Tests of the confirmation chain (confirm3e.py, confirm_read.py): the registry against 12.5, the machinery every design shares, the gates, the reader."""
import dataclasses
import math
import unittest

import numpy as np

import batch3e as B
import confirm3e as C
import confirm_read as CR
import test_toy2
import toy2

INF = math.inf


def cut(job, n):
    """The job's seed-0 configuration with n units."""
    return dataclasses.replace(job.builder(0), n_total=n)


def find(cell, design, key=None):
    return [j for j in C.jobs_of(cell) if j.design == design and (key is None or j.key == key)]


class Registry(unittest.TestCase):
    def test_run_counts_are_the_registered_ones(self):
        c, aa = C.registry_counts()
        self.assertEqual(c, {"CL": 369, "CL-long": 369, "CLw-long": 369, "CLc": 225, "CLc-v": 273, "CL-long-v": 273, "OV-long": 117, "OV-long-v": 117})
        self.assertEqual(aa, 96)
        self.assertEqual(sum(c.values()) + aa, 2208)

    def test_every_design_carries_the_clamp_and_the_eligibility_rule(self):
        for cell in C.ORDER:
            for j in C.jobs_of(cell):
                cfg = j.builder(0)
                self.assertTrue(cfg.clamp and cfg.elig == "rack" and cfg.rev4_gauge, j.name)
                self.assertEqual(cfg.floor_repeat, "evidenced_overdue" if j.design in ("A", "B") else "", j.name)
                self.assertEqual(cfg.wait_limit, j.design == "B", j.name)
                self.assertFalse(cfg.send_gate, j.name)                                    # no design has a send gate in 12
                if j.design in ("rt", "rteq"):
                    self.assertEqual(cfg.cwnd_sim, float(B.CELLS[B.base_of(cell)][1]["pause_B"]), j.name)      # today's Realtime: cwnd_sim = the cell's B
                self.assertEqual(cfg.eq, j.design == "rteq", j.name)

    def test_no_row_collides_with_a_stored_stage_3e_fingerprint(self):
        B.build()
        stored = {j.fp for p in B.PARTS.values() for j in p}
        self.assertTrue(all(j.fp not in stored for c in C.ORDER for j in C.jobs_of(c)))

    def test_b_is_an_identity_check_in_headroom_cells_and_a_full_sweep_in_overload_cells(self):
        self.assertEqual(sorted(j.key[1] for j in find("CL-long", "B")), [0.4, 2.0, INF])
        self.assertEqual(len(find("OV-long", "B")), 8)
        self.assertEqual(find("OV-long", "B")[0].nseeds, 6)


class SharedMachinery(unittest.TestCase):
    """12.1: the clamp and the eligibility rule are on every design; S5 at T = inf, H = inf equals Bulk under them."""

    def test_eligibility_rule_acts_on_every_design(self):
        for design in ("adr", "rt", "rteq", "bulk"):
            j = find("OV-long", design)[0]
            on = toy2.run(cut(j, 24000))
            off = toy2.run(dataclasses.replace(cut(j, 24000), elig=""))
            self.assertGreater(on["rev4"]["elig_n"], 0, design)
            self.assertGreater(on["rev4"]["elig_R_bind"], 0, design)                      # R exceeds P under overload
            self.assertEqual(on["rev4"]["elig_viol"], 0, design)
            moved = (on["share_any"], on["gauges"]["repair_sends"], on["gauges"]["resend_by_sweep"]) != (off["share_any"], off["gauges"]["repair_sends"], off["gauges"]["resend_by_sweep"])
            if design in ("adr", "bulk"):
                self.assertTrue(moved, design)                                            # the resend timing moved
            else:                                                                         # today's Realtime resends only on reports (11.2: report-driven resends are unchanged)
                self.assertEqual(off["gauges"]["resend_by_sweep"], 0, design)
                self.assertFalse(moved, design)

    def test_clamp_binds_on_both_realtime_arms_under_overload(self):
        for design in ("rt", "rteq"):
            r = toy2.run(cut(find("OV-long", design)[0], 24000))
            self.assertGreater(r["gauges"]["clamp_n"], 0, design)

    def test_s5_at_t_inf_equals_bulk_on_every_key_and_trace(self):
        for cell, n in (("CL-long", 20000), ("OV-long", 24000)):
            a = find(cell, "A", ("lt", INF, INF))[0]
            b = find(cell, "bulk")[0]
            ra = toy2.run(dataclasses.replace(cut(a, n), want_trace=True))
            rb = toy2.run(dataclasses.replace(cut(b, n), want_trace=True))
            self.assertEqual(ra["_trace"], rb["_trace"], cell)
            ra["trace_sha"] = rb["trace_sha"] = "x"
            self.assertEqual(C.ident_diff(ra, rb), [], cell)


class Reader(unittest.TestCase):
    def test_series_and_steady_readouts(self):
        dt = 0.01
        n = 1000                                                                           # 10 s of units
        h = np.arange(n) * dt
        dl = h + 0.05
        dl[100:200] = -1                                                                   # one second never delivered
        s = CR.make_series(h, dl, n, dt)
        self.assertEqual(sum(s["handed"]), n)
        self.assertEqual(sum(s["deliv"]), n - 100)
        ids = CR.window_ids(len(s["handed"]), 3.0, 8.0, None)
        r = CR.readouts(s, ids, 0.4)
        self.assertAlmostEqual(r["share"], 1.0, places=9)
        self.assertAlmostEqual(r["p50"], 0.05, delta=0.001)
        ids = CR.window_ids(len(s["handed"]), 0.0, 10.0, (1.0, 2.0))
        r = CR.readouts(s, ids, 0.4)
        self.assertAlmostEqual(r["share"], 1.0, places=9)                                  # the undelivered second is excluded with its windows
        ids = CR.window_ids(len(s["handed"]), 0.0, 10.0, None)
        r = CR.readouts(s, ids, 0.4)
        self.assertAlmostEqual(r["share"], 0.9, places=9)
        self.assertAlmostEqual(r["p50"], 0.05, delta=0.001)

    def test_a_stored_run_serves_cells_with_different_taus(self):
        """OV-long and OV-long-v run today's Realtime at delta 50 with one configuration, hence one stored run: its series carries the on-time counts of both taus."""
        a = [j for j in C.jobs_of("OV-long") if j.design == "rt" and j.key == ("rt", 50.0)][0]
        b = [j for j in C.jobs_of("OV-long-v") if j.design == "rt" and j.key == ("rt", 50.0)][0]
        self.assertEqual(a.fp, b.fp)
        dt, n = 0.01, 400
        h = np.arange(n) * dt
        s = CR.make_series(h, h + 0.5, n, dt)
        self.assertEqual(sum(s["ot"]["0.4"]), 0)
        self.assertEqual(sum(s["ot"]["2"]), n)

    def test_p50_is_infinite_when_fewer_than_half_arrive(self):
        dt = 0.01
        n = 400
        h = np.arange(n) * dt
        dl = np.where(np.arange(n) < 150, h + 0.1, -1.0)
        s = CR.make_series(h, dl, n, dt)
        self.assertTrue(math.isinf(CR.readouts(s, list(range(len(s["handed"]))), 0.4)["p50"]))

    @staticmethod
    def _rec(design, fill, rtt, shift, n=2000, dt=0.01, rec=None):
        h = np.arange(n) * dt
        dl = h + 0.05 + shift
        return dict(series=CR.make_series(h, dl, n, dt), rev4=dict(first_rtt_t=rtt, first_deq_exp=fill, first_pause_t=fill, first_qdrop_t=fill, fq_full_t=fill),
                    confirm_design=design, tau=0.4, last_hand=(n - 1) * dt, recovery_time=rec)

    def test_window_start_is_the_later_of_the_two_arms_and_follows_the_arm_kind(self):
        a, b = self._rec("A", 3.0, 0.2, 0.0), self._rec("rt", 5.0, 0.2, 0.0)
        self.assertAlmostEqual(CR.start_of(a), 4.0)
        self.assertAlmostEqual(CR.start_of(b), 6.0)
        self.assertAlmostEqual(CR.pair_window(a, b, None)[0], 6.0)
        self.assertAlmostEqual(CR.pair_window(a, b, None)[1], 20.0 - 0.01 - 0.4, delta=0.02)
        c = self._rec("bulk", None, 0.2, 0.0)                                              # a queue that never fills: one second after the first RTT sample
        self.assertAlmostEqual(CR.start_of(c), 1.2)

    def test_outage_exclusion_runs_to_the_later_recovery_end(self):
        a, b = self._rec("A", None, 0.1, 0.0, rec=2.0), self._rec("adr", None, 0.1, 0.0, rec=5.0)
        s, e, ex = CR.pair_window(a, b, (0, 2.0, 1.5))
        self.assertEqual(ex, (2.0, 3.5 + 5.0))

    def test_nf_is_the_p95_of_paired_differences_and_zero_for_identical_arms(self):
        a = [self._rec("A", None, 0.1, 0.0) for _ in range(12)]
        z = CR.nf_from_aa(a, a, None, 12)
        self.assertTrue(all(v == 0.0 for v in z.values()))
        b = [self._rec("A", None, 0.1, 0.01 * (i % 3)) for i in range(12)]
        nf = CR.nf_from_aa(a, b, None, 12)
        self.assertGreater(nf["p50"], 0.0)
        self.assertEqual(nf["share"], 0.0)

    def test_bars_follow_the_registration(self):
        self.assertAlmostEqual(CR.share_bar(0.8, 100000, 0.001), 0.01)
        self.assertAlmostEqual(CR.share_bar(1.0, 100000, 0.0), 1e-5)
        self.assertAlmostEqual(CR.p50_bar(0.2, 0.001), 0.01)
        self.assertAlmostEqual(CR.p50_bar(0.2, 0.05), 0.05)
        self.assertAlmostEqual(CR.rec_bar(6.0, 0.0), 0.3)

    def test_dominance_words(self):
        mk = lambda w: dict(word=w)                                                       # noqa: E731
        self.assertEqual(CR.dominance_word({"a": mk("equivalent"), "b": mk("shown better")}), "dominates")
        self.assertEqual(CR.dominance_word({"a": mk("shown worse"), "b": mk("shown better")}), "not dominated (a trade)")
        self.assertEqual(CR.dominance_word({"a": mk("equivalent"), "b": mk("equivalent")}), "equivalent")
        self.assertEqual(CR.dominance_word({"a": mk("undetermined"), "b": mk("shown better")}), "undetermined")

    def test_best_point_is_the_highest_on_time_with_ties_broken_by_p50_then_t(self):
        p = lambda t, ot, p50: (str(t), t, dict(ot=np.full(3, ot), p50=np.full(3, p50)), 0.01)      # noqa: E731
        self.assertEqual(CR.best_point([p(0.4, 0.80, 0.1), p(0.6, 0.50, 0.1)])[1], 0.4)
        self.assertEqual(CR.best_point([p(0.4, 0.800, 0.2), p(0.6, 0.805, 0.1)])[1], 0.6)         # within the bar: the lower p50
        self.assertEqual(CR.best_point([p(0.6, 0.80, 0.1), p(0.4, 0.80, 0.1)])[1], 0.4)           # a full tie: the smaller T

    def test_infinite_delays_compare_as_a_loss(self):
        g = CR.compare_pair(np.array([0.1, 0.1, 0.1]), np.array([INF, INF, INF]), -1.0, 0.01)
        self.assertEqual(g["word"], "shown better")
        g = CR.compare_pair(np.array([INF, INF, INF]), np.array([0.1, 0.1, 0.1]), -1.0, 0.01)
        self.assertEqual(g["word"], "shown worse")
        g = CR.compare_pair(np.array([INF, INF, INF]), np.array([INF, INF, INF]), -1.0, 0.01)
        self.assertEqual(g["word"], "equivalent")


class Gates(unittest.TestCase):
    """12.1's stop rules, on synthetic stored runs."""

    def setUp(self):
        self.jobs = C.jobs_of("CL")
        self.orig = C.load

    def tearDown(self):
        C.load = self.orig

    def _fake(self, tweak=None):
        base = dict(finished=True, trace_sha="t", trace_n=1, share_any=0.9, gauges=dict({k: 0 for k in test_toy2.ID_GAUGES}, l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0, rep_fire_viol_carrier=0, rep_supp_viol=0),
                    rev4=dict(elig_viol=0, n_tau_lt_T=0, first_rtt_t=0.1), series={}, confirm_design="A", wall_s=1.0)

        def load(fp, seed):
            r = dict(base, gauges=dict(base["gauges"]), rev4=dict(base["rev4"]))
            if tweak:
                tweak(fp, seed, r)
            return r
        C.load = load

    def test_clean_runs_pass(self):
        self._fake()
        self.assertEqual(C.run_gates("CL", self.jobs), [])

    def test_unfinished_new_model_run_stops_but_an_unfinished_realtime_run_does_not(self):
        rt = [j for j in self.jobs if j.design == "rt"][0]
        a = [j for j in self.jobs if j.design == "A"][0]
        self._fake(lambda fp, s, r: r.update(finished=False) if fp == rt.fp else None)
        self.assertEqual(C.run_gates("CL", self.jobs), [])
        self._fake(lambda fp, s, r: r.update(finished=False) if fp == a.fp else None)
        self.assertTrue(any("unfinished" in b for b in C.run_gates("CL", self.jobs)))

    def test_b_must_equal_a_in_a_headroom_cell(self):
        b = [j for j in self.jobs if j.design == "B"][0]
        self._fake(lambda fp, s, r: r.update(share_any=0.8) if fp == b.fp else None)
        self.assertTrue(any("B != A" in x for x in C.run_gates("CL", self.jobs)))
        self._fake(lambda fp, s, r: r["rev4"].update(n_tau_lt_T=3) if fp == b.fp else None)
        self.assertEqual(C.run_gates("CL", self.jobs), [])                                  # a live wait limit with identical traces is reported, not a stop
        self._fake(lambda fp, s, r: r.update(trace_sha="other") if fp == b.fp else None)
        self.assertTrue(any("B != A" in x for x in C.run_gates("CL", self.jobs)))

    def test_s5_at_t_inf_must_equal_bulk(self):
        bulk = [j for j in self.jobs if j.design == "bulk"][0]
        self._fake(lambda fp, s, r: r.update(trace_sha="other") if fp == bulk.fp else None)
        self.assertTrue(any("!= Bulk" in x for x in C.run_gates("CL", self.jobs)))

    def test_absolutes_stop(self):
        a = [j for j in self.jobs if j.design == "A" and j.key == ("lt", 0.4, 0.0)][0]
        self._fake(lambda fp, s, r: r["gauges"].update(rep_fire_viol_carrier=1) if fp == a.fp else None)
        self.assertTrue(any("rep_fire_viol_carrier" in x for x in C.run_gates("CL", self.jobs)))
        self._fake(lambda fp, s, r: r["rev4"].update(elig_viol=2) if fp == a.fp else None)
        self.assertTrue(any("elig_viol" in x for x in C.run_gates("CL", self.jobs)))
        self._fake(lambda fp, s, r: r["gauges"].update(rep_supp_viol=1) if fp == a.fp else None)
        self.assertTrue(any("rep_supp_viol" in x for x in C.run_gates("CL", self.jobs)))
        hold = [j for j in self.jobs if j.design == "A" and j.key == ("lt", INF, INF)][0]
        self._fake(lambda fp, s, r: r["gauges"].update(rep_supp_viol=1) if fp == hold.fp else None)
        self.assertFalse(any("rep_supp_viol" in x and "inf, hold inf" in x for x in C.run_gates("CL", self.jobs)))      # a hold row is reported only

class ReReadFixes(unittest.TestCase):
    """The first reads of CL-long and CL (2026-11-26): Bulk is read by identity, and an empty paired window is a finding, not a crash."""

    def setUp(self):
        self.orig = C.load

    def tearDown(self):
        C.load = self.orig

    def test_bulk_is_read_by_identity_against_s5_at_t_inf_h_inf(self):
        jobs = C.jobs_of("CL-long")
        sinf = [j for j in jobs if j.design == "A" and j.key == ("lt", INF, INF)][0]
        bulk = [j for j in jobs if j.design == "bulk"][0]
        rec = dict(finished=True, trace_sha="t", trace_n=1, share_any=1.0, gauges={k: 0 for k in test_toy2.ID_GAUGES}, rev4={}, series={}, confirm_design="A")
        C.load = lambda fp, s: dict(rec, gauges=dict(rec["gauges"]))
        self.assertEqual(C.identity_row(sinf, bulk, 3), [])
        C.load = lambda fp, s: dict(rec, gauges=dict(rec["gauges"]), trace_sha="t" if fp == sinf.fp else "u")
        self.assertEqual([b[0] for b in C.identity_row(sinf, bulk, 3)], [0, 1, 2])

    def test_an_empty_paired_window_gives_nan_not_an_exception(self):
        dt, n = 0.01, 1000
        h = np.arange(n) * dt
        mk = lambda rec, fill: dict(series=CR.make_series(h, h + 0.05, n, dt), rev4=dict(first_rtt_t=0.01, first_deq_exp=fill, first_pause_t=fill, first_qdrop_t=fill, fq_full_t=fill),  # noqa: E731
                                    confirm_design="A", tau=0.4, last_hand=(n - 1) * dt, recovery_time=rec)
        a, b = mk(None, 4.5), mk(None, None)                                               # neither recovers: the outage exclusion runs to the end of the run
        xs, ys = C.steady_arrays("CL", [a], [b], 1)
        self.assertTrue(np.isnan(xs["p50"]).all() and np.isnan(ys["share"]).all())

if __name__ == "__main__":
    unittest.main()
