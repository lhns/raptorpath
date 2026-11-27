"""Tests of the build B' confirmation driver (confirm13.py): the registry against 13.6, the stored fingerprints, the reader fix, the gates."""
import dataclasses
import math
import os
import unittest

import numpy as np

import batch3e as B
import confirm13 as K
import confirm3e as C
import confirm_read as CR

INF = math.inf


class Registry(unittest.TestCase):
    def test_run_counts_are_the_registered_ones(self):
        pj = K.part_jobs()
        n = {p: K.runs_of(js) for p, js in pj.items()}
        self.assertEqual(n["OV-long"], 48)
        self.assertEqual(n["OV-long-v"], 48)
        self.assertEqual(n["IDENT"], 54 + 2)
        self.assertEqual(n["AA-m"], 24)
        self.assertEqual(n["K-OV"], 12)
        fresh = sum(j.nseeds for j in pj["OV-long-m"] if j.design in ("adr", "bulk"))
        self.assertEqual(n["OV-long-m"] - fresh, 66)                       # A 30, B' 30, rt and rteq 2 x 3
        self.assertEqual(fresh, 9)                                          # ADR-0084 at 2 rho and Bulk, 3 seeds each: their configuration carries delta = 0.1, so no stored row matches
        self.assertEqual(sum(n.values()) - fresh, 254)

    def test_the_new_cell_is_ov_longs_at_delta_0_1(self):
        self.assertEqual(B.delta_of(K.M), 0.1)
        self.assertEqual(B.delta_of("OV-long"), 50.0)
        self.assertEqual(B.delta_of("OV-long-v"), 0.005)
        self.assertEqual(B.tau_of(K.M), 0.4)
        self.assertEqual(B.SWEEP[K.M], B.SWEEP["OV-long"])
        lc, kw = B.CELLS[K.M]
        lc0, kw0 = B.CELLS["OV-long"]
        self.assertEqual((lc, kw), (lc0, kw0))
        self.assertIsNot(kw, kw0)
        a = [j for j in K.m_jobs() if j.design == "A"][0].builder(0)
        self.assertEqual((a.delta, a.T), (0.1, 0.4))
        rt = [j for j in K.m_jobs() if j.design == "rt"][0].builder(0)
        self.assertEqual(rt.delta, 0.1)

    def test_bp_is_a_plus_the_law_and_nothing_else(self):
        for cell in ("OV-long", "OV-long-v", K.M, "K-OV"):
            key = ("lt", 0.4 if cell != "OV-long-v" else 1.0, 0.0)
            a = C.mk(cell, key, K.A_KW)(0)
            bp = K.bp_job(cell, key, 1).builder(0)
            self.assertEqual(dataclasses.replace(bp, wait_limit=False, wait_limit_form="k_t2"), a, cell)
            self.assertTrue(bp.wait_limit and bp.wait_limit_form == "rev4b" and not bp.send_gate)
            self.assertEqual(B.fingerprint(bp) == B.fingerprint(a), False)

    def test_stored_rows_keep_their_fingerprints(self):
        if not os.path.exists(C.run_path(C.jobs_of("OV-long")[0].fp, 0)):
            self.skipTest("no stored runs")
        for cell in K.OV_CELLS:
            for j in C.jobs_of(cell):
                self.assertTrue(all(C.have(j.fp, s) for s in range(j.nseeds)), j.name)       # A, B, old senders and Bulk: every stored seed matches
        for a, _ in K.ident_pairs():
            self.assertTrue(all(C.have(a.fp, s) for s in range(3)), a.name)

    def test_new_runs_do_not_collide_with_stored_ones(self):
        stored = {j.fp for c in C.ORDER for j in C.jobs_of(c)}
        for p, js in K.part_jobs().items():
            for j in js:
                if is_new(j):
                    self.assertNotIn(j.fp, stored, j.name)

    def test_identity_set(self):
        pairs = K.ident_pairs()
        self.assertEqual(len(pairs), 18)
        cells = {}
        for a, b in pairs:
            cells.setdefault(a.cell, set()).add(a.key[1])
            self.assertEqual((a.cell, a.key), (b.cell, b.key))
            self.assertEqual(b.nseeds, 3)
            self.assertEqual(b.trace_seeds, 3)
            ca = a.builder(0)
            self.assertEqual(dataclasses.replace(b.builder(0), wait_limit=False, wait_limit_form="k_t2"), ca)
        self.assertEqual(cells["CL-long"], {0.4, 2.0, INF})
        self.assertEqual(cells["CL"], {0.4, 2.0, INF})
        self.assertEqual(cells["CLw-long"], {0.4, 2.0, INF})
        self.assertEqual(cells["CLc"], {0.4, 1.0, INF})
        self.assertEqual(cells["CLc-v"], {1.0, 2.0, INF})
        self.assertEqual(cells["CL-long-v"], {1.0, 2.0, INF})
        a11, b11 = K.seed_x_pair()
        self.assertEqual((a11.builder(0).seed, b11.builder(0).seed), (11, 11))
        self.assertTrue(a11.fp.endswith("t11") and b11.fp.endswith("t11"))
        stored_a = [a for a, _ in pairs if a.cell == "CL-long-v" and a.key[1] == 2.0][0]
        self.assertNotEqual(a11.fp, stored_a.fp)                                 # the stored A row is not overwritten
        self.assertEqual((a11.nseeds, a11.trace_seeds), (1, 1))

    def test_every_new_design_carries_the_shared_machinery(self):
        for js in K.part_jobs().values():
            for j in js:
                cfg = j.builder(0)
                self.assertTrue(cfg.clamp and cfg.elig == "rack" and cfg.rev4_gauge, j.name)
                self.assertFalse(cfg.send_gate, j.name)
                self.assertEqual(cfg.wait_limit_form == "rev4b", K.is_bp(j), j.name)
                if j.design in ("rt", "rteq"):
                    self.assertEqual(cfg.cwnd_sim, float(B.CELLS["OV-long"][1]["pause_B"]))


def is_new(job):
    return K.is_bp(job) or job.cell in (K.M, "K-OV") or job.design == "aa" or job.fp.endswith("t11")


class Reader(unittest.TestCase):
    def test_worse_on_one_readout_and_better_on_none_reads_worse(self):
        w, b, n = dict(word="shown worse"), dict(word="shown better"), dict(word="equivalent")
        self.assertEqual(CR.dominance_word(dict(share=n, ot=n, p50=w)), "worse")
        self.assertEqual(CR.dominance_word(dict(share=w, ot=w, p50=n)), "worse")
        self.assertEqual(CR.dominance_word(dict(share=n, ot=n, p50=b)), "dominates")
        self.assertEqual(CR.dominance_word(dict(share=w, ot=n, p50=b)), "not dominated (a trade)")
        self.assertEqual(CR.dominance_word(dict(share=n, ot=n, p50=n)), "equivalent")
        self.assertEqual(CR.dominance_word(dict(share=w, ot=dict(word="undetermined"), p50=b)), "undetermined")


class Gates(unittest.TestCase):
    def test_bill_gate_and_onset_gate_fire_on_planted_data(self):
        def rec(share, units=480000):
            return dict(share_any=share, units_total=units)
        a = C.CJob("c | A | x", "OV-long-v", "A", ("lt", 1.0, 0.0), 1, C.mk("OV-long-v", ("lt", 1.0, 0.0), K.A_KW))
        bp = C.CJob("c | Bp | x", "OV-long-v", "B", ("lt", 1.0, 0.0), 1, C.mk("OV-long-v", ("lt", 1.0, 0.0), K.BP_KW))
        orig = C.recs
        try:
            C.recs = lambda j, n=None: [rec(0.806)] if j is a else [rec(0.8)]                  # 2 880 units behind
            self.assertEqual(len(K.gate_bill("OV-long-v", [a], [bp], [])), 1)
            C.recs = lambda j, n=None: [rec(0.806)] if j is a else [rec(0.805)]                # 480 units behind
            self.assertEqual(K.gate_bill("OV-long-v", [a], [bp], []), [])
        finally:
            C.recs = orig

    def test_window_onset_bind_counts_only_the_steady_window(self):
        r = dict(rev4=dict(cap_series=[(0.0, 10, 0.1, 0.1, 1.2, 0.5, 0.3, 10), (0.5, 10, 0.1, 0.1, 1.2, 0.5, 0.3, 10), (1.0, 10, 0.1, 0.1, 1.2, 0.5, 0.3, 0)]))
        self.assertEqual(K.window_onset_bind(r, 0.0, 1.5), 20 / 30)
        self.assertEqual(K.window_onset_bind(r, 1.0, 1.5), 0.0)
        self.assertIsNone(K.window_onset_bind(r, 5.0, 6.0))


if __name__ == "__main__":
    unittest.main()
