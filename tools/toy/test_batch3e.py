"""Tests for stage 3e's runner (batch3e.py) and reader (summarize3e.py), on synthetic data and short runs (three-knobs-stage3e-build.md 8, 9 step 5).
Run from tools/toy:  python -m unittest test_batch3e"""
import dataclasses
import json
import math
import os
import sys
import tempfile
import time
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy2  # noqa: E402
import batch3e as B  # noqa: E402
import summarize3e as S  # noqa: E402

INF = math.inf
SEEDS = 12
REG_AMEND = {"CLw-long": 75, "RT": 55, "CL": 52, "OV": 47, "K-I2": 40, "K-L10": 40, "K-OV": 36, "OV-long": 28, "CLc": 30, "CL-long": 26,
             "CLc-v": 36, "CL-long-v": 36, "OV-long-v": 30, "Ec": 6, "E-I2": 6, "M-I2": 6}          # registration 14.9: 543 configurations (14.4 drops the inner points of the long overload cells)


class Cells(unittest.TestCase):
    def test_overrides_of_the_registered_cells(self):
        C = B.CELLS
        lc, k = C["CLw-long"]
        self.assertEqual((k["rate"], k["n_total"], k["dead"]), (8000.0, 160000, (0, 2.0, 3.0)))
        lc, k = C["CLs-long"]
        self.assertEqual((k["rate"], k["n_total"], k["link"], k["pause_B"]), (1000.0, 20000, dict(C=1250.0, Q=125.0), 100))
        self.assertIsNone(C["CLc"][1]["dead"])
        self.assertIsNone(C["RT"][1]["dead"])
        self.assertEqual(C["RQ"][1]["rev_q"], 0.1)
        lc, k = C["K-OV"]
        self.assertEqual((lc, k["link"], k["pause_B"], k["sweep_form"], k["rx_refresh"], k["t_limit"], k["rate"], k["owd"]),
                         ("L10", dict(C=40.0, Q=40.0), 8, "blocker", True, 120.0, 50.0, 0.05))
        self.assertIsNone(k["dead"])
        self.assertIsNone(C["Ec"][1]["dead"])
        self.assertEqual(C["CL"][1]["sweep_form"], "blocker")                      # CL@b
        self.assertTrue(C["CL"][1]["windows"])
        self.assertTrue(C["K-I2"][1]["windows"])                                   # a cell with an outage reads windows

    def test_cells_do_not_alias_the_batch_dicts(self):
        import batch3d
        B.CELLS["CL"][1]["rate"] = 1.0
        try:
            self.assertEqual(batch3d.CELLS["CL@b"][1]["rate"], 10000.0)
        finally:
            B.CELLS["CL"][1]["rate"] = 10000.0

    def test_builders(self):
        c = B.new_cfg("CL", 3, 0.4, 0.0, 50.0, dict(r="law", edge="worth"))
        self.assertEqual((c.sender, c.recv, c.T, c.Hk, c.delta, c.seed, c.rho, c.repair, c.rtt_est, c.owd_est, c.floor_law),
                         ("S5", "RH", 0.4, 0.0, 50.0, 3, 1.0, dict(r="law", edge="worth"), "rfc", "fwd", "protect"))
        a = B.adr_cfg("CL", 1, 0.99, 50.0, dict(r="law", edge="worth"))
        self.assertEqual((a.sender, a.recv, a.rho, a.repair["edge"], a.rtt_est), ("S1c", "R0e", 0.99, "none", "rfc"))
        r = B.rt_cfg("CL", 1, 5.0, dict(r="est", edge="worth"))
        self.assertEqual((r.sender, r.recv, r.budget, r.floor_send, r.evict_window, r.r0e_max, r.pause_B, r.d_arm, r.delta, r.repair),
                         ("S1", "R0e", "resid", False, 200, 500, 10 ** 9, "EWMA", 5.0, dict(r="est", edge="evict")))
        b = B.bulk_cfg("CL", 1, 50.0, None)
        self.assertEqual((b.sender, b.recv, b.rho, b.repair), ("S1c", "R1", 1.0, None))
        self.assertEqual(B.new_cfg("RT", 0, 0.12, 0.0, 50.0, None).tau_extra, (0.12,))     # RT reads the 0.12 s on-time share

    def test_the_t_inf_hinf_arm_equals_todays_bulk_under_repair_in_a_call_cell(self):
        rp = dict(r="law", edge="worth")
        for seed in range(2):
            a = B.new_cfg("RT", seed, INF, INF, 50.0, rp, n_total=500)
            b = B.bulk_cfg("RT", seed, 50.0, dict(rp, edge="none"), n_total=500)
            a.want_trace = b.want_trace = True
            ra, rb = toy2.run(a), toy2.run(b)
            self.assertEqual(B.id3e(ra, rb), [])
            self.assertEqual(ra["_trace"], rb["_trace"])
            self.assertGreater(ra["gauges"]["rep_sent"], 0)

    def test_old_arms_carry_the_rfc_estimator_because_the_identity_needs_it(self):
        """Witness for build 8.2's departure: with the old RTT estimator the Bulk arm of a call cell differs from S5 at T = inf under repair
        (srtt feeds r), with the RFC 6298 estimator it does not."""
        rp = dict(r="law", edge="worth")
        a = B.new_cfg("RT", 0, INF, INF, 50.0, rp, n_total=500)
        lit = B.bulk_cfg("RT", 0, 50.0, rp, n_total=500, rtt_est="old")
        rfc = B.bulk_cfg("RT", 0, 50.0, rp, n_total=500)
        for c in (a, lit, rfc):
            c.want_trace = True
        ra, rl, rr = toy2.run(a), toy2.run(lit), toy2.run(rfc)
        self.assertNotEqual(B.id3e(ra, rl), [])
        self.assertEqual(B.id3e(ra, rr), [])
        self.assertEqual(ra["_trace"], rr["_trace"])

    def test_a_delta_005_row_equals_its_off_row(self):
        for mode in (dict(r="law", edge="worth"), dict(r="est", edge="worth")):
            a = B.new_cfg("RT", 0, 0.12, 0.0, 0.005, None, n_total=500)
            b = B.new_cfg("RT", 0, 0.12, 0.0, 0.005, mode, n_total=500)
            a.want_trace = b.want_trace = True
            ra, rb = toy2.run(a), toy2.run(b)
            self.assertEqual(B.id3e(ra, rb), [])
            self.assertEqual(ra["_trace"], rb["_trace"])


class Parts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        B.build()

    def test_registered_counts_before_the_dedupe(self):
        P = B.PARTS
        self.assertEqual(len(P["MAIN"]), 59)
        self.assertEqual(len(P["DECIDE"]), 54)
        self.assertEqual(len(P["CROSS"]), 2)
        self.assertEqual(len(P["AA"]), 6 * 4)                                      # per cell: A and A' (phase variant 1) at T = inf and at the primary lifetime
        self.assertEqual(len(P["AA2"]), 8 * 4 + 3 * 2)                             # the file cells' primary lifetime is T = inf: one pair
        self.assertEqual(len(P["ROBUST"]), 32)
        self.assertEqual(len(P["SMOOTH"]), 272)
        am = P["AMEND"]
        self.assertEqual(len(am), 549)                                                             # 543 of registration 14.9, plus the Realtime rows restored in OV-long and OV-long-v
        by = {}
        for j in am:
            by[j.cell] = by.get(j.cell, 0) + 1
        self.assertEqual(by, REG_AMEND)
        extra = [j for j in P["EARLY"] if j.part == "EARLY" and j.key[0] == "lt" and j.mode == "RL" and j.cell in ("RT", "OV") and j not in am]
        self.assertEqual(len(extra), 2)                                                            # MAIN's RL arm of RT (T = 0.12) and OV (T = 0.4) is also an EARLY row
        self.assertEqual(sorted(j.name for j in P["EARLY"] + P["REST"] if j not in extra), sorted(j.name for j in am))     # EARLY and REST split AMEND

    def test_main_arms_per_cell(self):
        by = {}
        for j in B.PARTS["MAIN"]:
            by.setdefault(j.cell, []).append(j)
        self.assertEqual({c: len(v) for c, v in by.items()}, {"CL": 5 + 1, "CL-long": 11 + 1, "OV": 9 + 1, "CLw-long": 10 + 1, "RT": 9 + 1, "CLs-long": 9 + 1})
        fixed = {(j.cell, j.arm): j.builder(0).repair["r"] for j in B.PARTS["MAIN"] if isinstance(j.builder(0).repair, dict) and isinstance(j.builder(0).repair["r"], float)}
        self.assertEqual(fixed[("CL", "RS")], 0.107)
        self.assertEqual(fixed[("OV", "RS")], 0.0998)
        self.assertEqual(fixed[("CLs-long", "RS")], 0.2295)
        self.assertEqual(fixed[("RT", "RF1")], 0.112)
        self.assertEqual(fixed[("RT", "RF3")], 0.36)
        self.assertEqual(fixed[("CL-long", "R0")], 0.0)
        self.assertEqual(fixed[("CLw-long", "RS-inf")], 0.116)
        arms = {(j.cell, j.arm): j.builder(0) for j in B.PARTS["MAIN"]}
        self.assertEqual(arms[("RT", "S5")].T, 0.12)
        self.assertEqual(arms[("RT", "RL-T0p4")].T, 0.4)
        self.assertEqual(arms[("OV", "RL-age")].repair["edge"], "age")
        self.assertEqual(arms[("OV", "RL-floor")].repair["edge"], "floor")
        self.assertEqual(arms[("OV", "RL-Hinf")].Hk, INF)
        self.assertEqual(arms[("CL-long", "S5-inf")].T, INF)
        self.assertEqual(arms[("CL-long", "AA")].variant, 1)

    def test_decide_is_cl_long_off_and_rl(self):
        self.assertEqual({j.cell for j in B.PARTS["DECIDE"]}, {"CL-long"})
        self.assertEqual(sorted({j.mode for j in B.PARTS["DECIDE"]}), ["OFF", "RL"])
        self.assertEqual(len([j for j in B.PARTS["DECIDE"] if j.key[0] == "lt"]), 36)
        self.assertEqual(len([j for j in B.PARTS["DECIDE"] if j.key[0] in ("adr", "rt", "bulk")]), 18)

    def test_cross_is_the_two_proof_stage_1_cross_jobs(self):
        c = [j.builder(0) for j in B.PARTS["CROSS"]]
        self.assertEqual(sorted((x.sender, x.recv) for x in c), [("S1", "R0e"), ("S1c", "RH")])
        self.assertTrue(all(x.repair is None and x.rho == 0.99 for x in c))

    def test_every_row_has_one_seed_set_and_the_identity_rows_three(self):
        for j in B.PARTS["AMEND"]:
            if j.nseeds == 3:
                cf = j.builder(0)
                self.assertTrue(cf.delta == 0.005 or j.cell in B.OLD_ENDS, j.name)                  # an identity row, or an old sender of a long overload cell
        for p, js in B.PARTS.items():
            for j in js:
                self.assertIn(j.nseeds, (1, 3, 6, SEEDS))

    def test_fingerprint_ignores_seed_and_trace_and_dedupes_across_parts(self):
        a = B.new_cfg("CL", 0, 0.4, 0.0, 50.0, None)
        b = B.new_cfg("CL", 7, 0.4, 0.0, 50.0, None)
        b.want_trace = True
        self.assertEqual(B.fingerprint(a), B.fingerprint(b))
        self.assertNotEqual(B.fingerprint(a), B.fingerprint(B.new_cfg("CL", 0, 0.6, 0.0, 50.0, None)))
        self.assertNotEqual(B.fingerprint(a), B.fingerprint(B.new_cfg("CL", 0, 0.4, 0.0, 50.0, dict(r="law", edge="worth"))))
        self.assertNotEqual(B.fingerprint(B.new_cfg("CL", 0, INF, 0.0, 50.0, None)), B.fingerprint(B.new_cfg("CL", 0, 1e9, 0.0, 50.0, None)))
        cnt = B.counts()
        total_cfgs = sum(len(v) for v in B.PARTS.values())
        self.assertLess(sum(c["new"] for c in cnt.values()), total_cfgs)           # the registered sweeps share points
        seen = set()
        for p in B.ORDER:                                                          # a fingerprint is attributed to the first part that asks for it
            for fp in B.attributed(p):
                self.assertNotIn(fp, seen)
                seen.add(fp)

    def test_job_names_say_what_the_row_does(self):
        for p in ("MAIN", "DECIDE"):
            self.assertTrue(all("(" in j.name for j in B.PARTS[p]))
        self.assertFalse(any("toy" in j.name.lower() for ps in B.PARTS.values() for j in ps))


class Runner(unittest.TestCase):
    def setUp(self):
        self.td = tempfile.TemporaryDirectory()
        self.old = B.OUT
        B.OUT = self.td.name

    def tearDown(self):
        B.OUT = self.old
        self.td.cleanup()

    def mark(self, *names):
        os.makedirs(B.OUT, exist_ok=True)
        for n in names:
            open(os.path.join(B.OUT, n), "w").close()

    def test_a_part_refuses_without_its_markers(self):
        miss = B.missing_markers("DECIDE")                                          # registration 14.6: AA, AA2, TIME, DECIDE, EARLY, MAIN, REST, then ROBUST and SMOOTH
        self.assertEqual(miss, ["PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-aa", "DONE-AA", "DONE-AA2", "PASS-time"])
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate")
        self.assertEqual(B.missing_markers("AA"), [])
        self.assertEqual(B.missing_markers("TIME"), ["PASS-aa", "DONE-AA", "DONE-AA2"])
        self.mark("PASS-aa", "DONE-AA", "DONE-AA2")
        self.assertEqual(B.missing_markers("TIME"), [])
        self.assertEqual(B.missing_markers("DECIDE"), ["PASS-time"])
        self.mark("PASS-time")
        self.assertEqual(B.missing_markers("DECIDE"), [])
        self.assertEqual(B.missing_markers("CROSS"), ["DONE-DECIDE"])
        self.assertEqual(B.missing_markers("EARLY"), ["DONE-DECIDE"])
        self.mark("DONE-DECIDE")
        self.assertEqual(B.missing_markers("CROSS"), [])
        self.assertEqual(B.missing_markers("EARLY"), [])
        self.assertEqual(B.missing_markers("MAIN"), ["PASS-simread", "DONE-EARLY"])                  # the simulator read gates MAIN only
        self.assertEqual(B.missing_markers("REST"), ["DONE-EARLY", "DONE-MAIN"])
        self.assertEqual(B.missing_markers("DOUBLE:REST"), ["DONE-EARLY", "DONE-MAIN", "DONE-REST"])
        self.assertEqual(B.missing_markers("DOUBLE:REST", partial=True), ["DONE-EARLY", "DONE-MAIN"])
        self.mark("PASS-simread", "DONE-EARLY", "DONE-MAIN", "DONE-REST")
        self.assertEqual(B.missing_markers("ROBUST"), ["VERDICT-PASSES"])                 # blocked until the reader has written a PASSES verdict
        self.assertEqual(B.missing_markers("SMOOTH"), ["DONE-ROBUST", "VERDICT-PASSES"])
        self.mark("VERDICT-PASSES", "DONE-ROBUST")
        self.assertEqual(B.missing_markers("ROBUST"), [])
        self.assertEqual(B.missing_markers("SMOOTH"), [])
        self.assertEqual(B.missing_markers("DOUBLE:DECIDE"), [])
        self.assertEqual(B.missing_markers("DOUBLE:ROBUST"), [])

    def test_stop_refuses_everything(self):
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread")
        self.assertIsNone(B.refusal("AA"))
        self.mark("STOP")
        self.assertIn("STOP", B.refusal("AA"))
        self.assertIn("STOP", B.refusal("CROSS"))

    def test_one_pool_of_at_most_nine_workers(self):
        self.assertIsNone(B.refusal_workers(9))
        self.assertIsNone(B.refusal_workers(1))
        self.assertIn("at most 9", B.refusal_workers(10))
        self.assertIn("at least one", B.refusal_workers(0))

    def test_todays_realtime_may_end_unfinished_and_it_is_recorded_not_stopped(self):
        """Amendment of 2026-11-26: today's Realtime sends no floor, so an object whose tail is shed never finishes (a property pinned in proof stage 1).  For that
        design "unfinished at t_limit" is recorded per run and read as it is; every other design keeps the liveness stop."""
        bad = dict(finished=False, gauges=dict(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0))
        self.assertEqual(B.gate_failures(bad, None, design="rt"), [])
        for design in ("new", "adr", "bulk", None):
            self.assertEqual(B.gate_failures(bad, None, design=design), ["finished"])
        worse = dict(bad, gauges=dict(bad["gauges"], l1_viol=2))
        self.assertEqual(B.gate_failures(worse, None, design="rt"), ["l1_viol"])                   # the other gates still bind it
        r = B.run_one(B.rt_cfg("RT", 0, 50.0, None, n_total=300))
        self.assertIn("unfinished", r)
        self.assertEqual(r["unfinished"], int(r["finished"] is False))
        self.assertEqual(r["delivered_by_t_limit"], r["share_any"])
        a, b = dict(r), {k: v for k, v in r.items() if k not in ("unfinished", "delivered_by_t_limit")}
        self.assertEqual(B.id3e(a, b), [])                                                          # an older stored row without the keys still compares

    def test_gate_failures(self):
        ok = dict(finished=True, gauges=dict(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0))
        self.assertEqual(B.gate_failures(ok, "worth"), [])
        bad = json.loads(json.dumps(ok))
        bad["finished"] = False
        self.assertEqual(B.gate_failures(bad, "worth"), ["finished"])
        for k in ("l1_viol", "l1_viol_rep", "commit_viol"):
            b = json.loads(json.dumps(ok))
            b["gauges"][k] = 1
            self.assertEqual(B.gate_failures(b, "worth"), [k])
        b = json.loads(json.dumps(ok))
        b["gauges"]["worth_viol_rep"] = 3
        self.assertEqual(B.gate_failures(b, "worth"), ["worth_viol_rep"])
        self.assertEqual(B.gate_failures(b, "floor"), ["worth_viol_rep"])
        self.assertEqual(B.gate_failures(b, "age"), [])                            # the age edge is the positive control of the worth gauge
        self.assertEqual(B.gate_failures(b, None), [])
        nf = dict(finished=None, gauges=ok["gauges"])
        self.assertEqual(B.gate_failures(nf, "worth"), [])                         # a stream cell has no finished flag

    def test_id3e_excludes_rep3e_and_the_stage_3c_exclusions(self):
        a = dict(share_any=0.9, sender="S5", gauges={k: 1 for k in toy2_id_gauges()}, rep3e=dict(r_mean=0.1))
        b = dict(share_any=0.9, sender="S1c", gauges={k: 1 for k in toy2_id_gauges()}, rep3e=dict(r_mean=0.0))
        self.assertEqual(B.id3e(a, b), [])
        b["share_any"] = 0.8
        self.assertEqual(B.id3e(a, b), ["share_any"])
        b["share_any"] = 0.9
        b["gauges"]["dup"] = 2
        self.assertEqual(B.id3e(a, b), [("g", "dup")])

    def test_run_one_stores_a_trace_digest_and_a_json_safe_result(self):
        cfg = B.new_cfg("RT", 0, 0.12, 0.0, 50.0, dict(r="law", edge="worth"), n_total=300, tau_extra=(0.12,))
        r = B.run_one(cfg, trace=True)
        self.assertEqual(len(r["trace_sha"]), 64)
        self.assertGreater(r["trace_n"], 300)
        back = json.loads(json.dumps(r))
        self.assertIn("0.12", back["rep3e"]["share_by"])
        self.assertNotIn("_trace", r)

    def test_a_run_writes_its_file_once_and_skips_it_after(self):
        os.makedirs(os.path.join(B.OUT, "runs"), exist_ok=True)
        cfg = B.new_cfg("RT", 0, 0.12, 0.0, 50.0, None, n_total=300)
        fp = B.fingerprint(cfg)
        B.store(fp, 0, dict(x=1))
        self.assertTrue(os.path.exists(B.fname(fp, 0)))
        self.assertEqual(os.path.basename(B.fname(fp, 0)), "%s-0.json" % fp)
        self.assertEqual(B.load(fp, 0), dict(x=1))
        self.assertIsNone(B.load(fp, 1))

    def test_a_failed_gate_writes_stop_with_the_job_seed_and_key(self):
        bad = dict(finished=False, gauges=dict(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0))
        fails = B.after_run("CL | lifetime T=0.4 (S5)", "abc", 3, bad, "worth", [])
        self.assertEqual(fails, ["finished"])
        txt = open(os.path.join(B.OUT, "STOP")).read()
        self.assertIn("CL | lifetime T=0.4 (S5)", txt)
        self.assertIn("seed 3", txt)
        self.assertIn("finished", txt)

    def test_the_identity_pair_check_writes_stop_on_a_difference(self):
        a = dict(share_any=0.9, sender="S5", gauges={k: 1 for k in toy2_id_gauges()}, trace_sha="x", finished=True)
        b = json.loads(json.dumps(a))
        a["gauges"].update(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0)
        b["gauges"].update(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0)
        os.makedirs(os.path.join(B.OUT, "runs"), exist_ok=True)
        B.store("fa", 1, a)
        B.store("fb", 1, b)
        self.assertEqual(B.pair_failures("fa", 1, a, ["fb"]), [])
        b["trace_sha"] = "y"
        B.store("fb", 1, b)
        self.assertEqual(B.pair_failures("fa", 1, a, ["fb"]), ["trace differs from fb"])
        b["trace_sha"] = "x"
        b["share_any"] = 0.5
        B.store("fb", 1, b)
        self.assertEqual(B.pair_failures("fa", 1, a, ["fb"]), ["id3e differs from fb: ['share_any']"])
        self.assertEqual(B.pair_failures("fa", 2, a, ["fb"]), [])                  # the partner has not run at this seed yet


    def tiny(self, jobs):
        def build():
            B.PARTS.clear()
            for p in B.ORDER:
                B.PARTS[p] = []
            for name, T, mode, kw in jobs:
                B.add("AA", name, "RT", ("aa", 0), mode, lambda s, T=T, mode=mode, kw=kw: B.new_cfg("RT", s, T, 0.0, 50.0, B.rp(mode), n_total=300, **kw), nseeds=2)
            B.register()
        old = B.build
        B.build = build
        self.addCleanup(setattr, B, "build", old)

    def test_main_runs_a_tiny_part_resumes_and_writes_done(self):
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread")
        self.tiny([("tiny OFF (S5)", 0.12, "OFF", {}), ("tiny RL (S5)", 0.12, "RL", {})])
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 0)
        self.assertTrue(os.path.exists(os.path.join(B.OUT, "DONE-AA")))
        self.assertEqual(len(os.listdir(os.path.join(B.OUT, "runs"))), 4)
        with open(os.path.join(B.OUT, "AA.json")) as f:
            self.assertEqual(sorted(json.load(f)), ["tiny OFF (S5)", "tiny RL (S5)"])
        mt = os.path.getmtime(os.path.join(B.OUT, "runs", sorted(os.listdir(os.path.join(B.OUT, "runs")))[0]))
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 0)                   # nothing to do: the files are skipped
        self.assertEqual(mt, os.path.getmtime(os.path.join(B.OUT, "runs", sorted(os.listdir(os.path.join(B.OUT, "runs")))[0])))

    def test_main_stops_on_a_gate_failure_and_then_refuses(self):
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread")
        self.tiny([("tiny broken (S5)", 0.04, "RL", dict(repair_seam="no_edge"))])
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 1)
        self.assertTrue(os.path.exists(os.path.join(B.OUT, "STOP")))
        self.assertIn("l1_viol_rep", open(os.path.join(B.OUT, "STOP")).read())
        self.assertFalse(os.path.exists(os.path.join(B.OUT, "DONE-AA")))
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 2)                   # nothing runs while STOP exists


class Amendment2(unittest.TestCase):
    """Registration 14 (amendment 2) and the build brief's runner section: seeds by cell, reuse, two memory pools, TIME, the order."""

    @classmethod
    def setUpClass(cls):
        B.build()

    def seeds(self, part, cell, **filt):
        return {j.nseeds for j in B.PARTS[part] if j.cell == cell}

    def test_seed_counts_by_cell(self):
        self.assertEqual({j.nseeds for j in B.PARTS["DECIDE"]}, {12})
        self.assertEqual({j.nseeds for j in B.PARTS["AA"] + B.PARTS["AA2"]}, {12})                       # the A/A rows run at 12 in every cell
        main = {}
        for j in B.PARTS["MAIN"]:
            main.setdefault(j.cell, set()).add(j.nseeds)
        self.assertEqual(main, {"CL": {12}, "CL-long": {12}, "CLw-long": {12}, "RT": {12}, "OV": {6, 12}, "CLs-long": {6, 12}})
        self.assertEqual({j.nseeds for j in B.PARTS["MAIN"] if j.cell in ("OV", "CLs-long") and j.arm != "AA"}, {6})
        self.assertEqual({j.nseeds for j in B.PARTS["MAIN"] if j.arm == "AA"}, {12})
        for cell in ("CL", "CLw-long", "RT", "CLc", "CLc-v", "CL-long-v", "K-I2", "K-L10", "CL-long"):
            self.assertLessEqual(self.seeds("AMEND", cell), {3, 12}, cell)
            self.assertIn(12, self.seeds("AMEND", cell), cell)
        for cell in ("K-OV", "OV", "OV-long", "OV-long-v", "Ec", "E-I2", "M-I2"):
            self.assertLessEqual(self.seeds("AMEND", cell), {3, 6}, cell)
            self.assertIn(6, self.seeds("AMEND", cell), cell)
        self.assertEqual({j.nseeds for j in B.PARTS["ROBUST"]}, {6})
        self.assertEqual({j.nseeds for j in B.PARTS["SMOOTH"]}, {3})
        for j in B.PARTS["AMEND"]:
            if j.mode != "OFF" and j.builder(0).delta == 0.005:
                self.assertEqual(j.nseeds, 3, j.name)                                                # the delta = 0.005 identity rows

    def test_long_overload_cells_run_adr_and_bulk_at_their_dial_ends_at_three_seeds_and_todays_realtime_in_full(self):
        for cell, rts in (("OV-long", (0.5, 5.0, 50.0)), ("OV-long-v", (0.005, 0.5, 5.0, 50.0))):
            old = [j for j in B.PARTS["AMEND"] if j.cell == cell and j.key[0] in ("adr", "bulk")]
            self.assertEqual({j.nseeds for j in old}, {3})
            self.assertEqual(sorted({j.key[1] for j in old if j.key[0] == "adr"}), [0.95, 1.0])
            rt = [j for j in B.PARTS["AMEND"] if j.cell == cell and j.key[0] == "rt"]
            self.assertEqual(sorted({j.key[1] for j in rt}), sorted(rts))                         # the whole delta range ...
            self.assertEqual({j.nseeds for j in rt if j.builder(0).delta != 0.005 or j.mode == "OFF"}, {6})        # ... at full seeds: today's Realtime does not read about 0 on time under overload
            new = [j for j in B.PARTS["AMEND"] if j.cell == cell and j.key[0] == "lt" and j.mode == "OFF"]
            self.assertEqual({j.nseeds for j in new}, {6})                                          # the new model's sweep at 6

    def test_early_holds_the_cells_primary_lifetime_row(self):
        for cell, T in (("RT", 0.12), ("OV", 0.4), ("CL-long-v", 2.0)):
            keys = {j.key for j in B.PARTS["EARLY"] if j.cell == cell}
            self.assertIn(("lt", T, 0.0), keys, cell)                                              # MAIN's RL arm is a sweep point that AMEND leaves out; EARLY needs it

    def test_early_is_the_four_cells_at_the_call_point_under_rl(self):
        self.assertEqual({j.cell for j in B.PARTS["EARLY"]}, {"Ec", "RT", "OV", "CL-long-v"})
        self.assertEqual({j.mode for j in B.PARTS["EARLY"]}, {"RL"})                               # the AMEND rows of those cells under RL
        self.assertTrue(any(j.mode == "RL" and j.key[0] == "lt" for j in B.PARTS["EARLY"] if j.cell == "RT"))
        names = [j.cell for j in B.PARTS["REST"]]
        self.assertLess(names.index("K-I2"), names.index("OV-long"))                                # long overload cells last

    def test_time_part_one_seed_of_each_cell_and_design_under_rl(self):
        t = B.PARTS["TIME"]
        self.assertEqual(len(t), 68)
        self.assertEqual({j.mode for j in t}, {"RL"})
        self.assertTrue(all(j.builder(0).seed == 0 and j.nseeds == 1 for j in t))
        designs = {}
        for j in t:
            designs.setdefault(j.cell, set()).add(j.design)
        self.assertEqual(len(designs), 17)
        self.assertTrue(all(d == {"new", "adr", "rt", "bulk"} for d in designs.values()))
        rt = {j.cell: j.builder(0).delta for j in t if j.design == "rt"}
        self.assertEqual((rt["OV"], rt["CL-long-v"], rt["Ec"]), (50.0, 0.005, 0.005))
        self.assertFalse(any(j.fp in B.FPS and "TIME" in B.FPS[j.fp]["jobs"] for j in t))              # timing only: never a stored result

    def test_heavy_means_an_old_sender_in_a_long_cell(self):
        self.assertTrue(B.is_heavy(B.adr_cfg("CL-long", 0, 0.99, 50.0, None)))
        self.assertTrue(B.is_heavy(B.rt_cfg("OV-long", 0, 50.0, None)))
        self.assertTrue(B.is_heavy(B.bulk_cfg("CLw-long", 0, 50.0, None)))
        self.assertFalse(B.is_heavy(B.new_cfg("OV-long", 0, 0.4, 0.0, 50.0, None)))
        self.assertFalse(B.is_heavy(B.adr_cfg("CL", 0, 0.99, 50.0, None)))
        self.assertFalse(B.is_heavy(B.bulk_cfg("OV", 0, 50.0, None)))

    def test_memory_wait(self):
        self.assertAlmostEqual(B.need_gb(1024.0), 2.0)                                              # the measured peak plus a 1 GB margin
        self.assertAlmostEqual(B.need_gb(146.0), 146.0 / 1024 + 1.0)
        seq = iter([0.5, 1.0, 1.2, 3.0])
        sleeps = []
        waited = B.wait_for_memory(1.15, avail=lambda: next(seq), sleep=sleeps.append)
        self.assertEqual((waited, len(sleeps)), (2, 2))                                            # two readings below the need, then it starts
        self.assertEqual(B.wait_for_memory(1.15, avail=lambda: 9.0, sleep=sleeps.append), 0)

    def test_the_need_of_a_run_is_its_measured_peak(self):
        old = dict(B.PEAKS)
        try:
            B.PEAKS.clear()
            B.PEAKS[("OV-long", "adr")] = 300.0
            fp = [fp for fp, e in B.FPS.items() if e["job"].cell == "OV-long" and e["job"].design == "adr"][0]
            self.assertAlmostEqual(B.need_for_fp(fp), 300.0 / 1024 + 1.0)
            other = [fp for fp, e in B.FPS.items() if e["job"].cell == "CL" and e["job"].design == "new"][0]
            self.assertAlmostEqual(B.need_for_fp(other), B.DEFAULT_PEAK_MB / 1024 + 1.0)              # unmeasured: the default
        finally:
            B.PEAKS.clear()
            B.PEAKS.update(old)

    def test_time_total_by_the_one_pool_formula_and_its_gate(self):
        classes = {("RT", "new", "RL"): 100, ("RT", "new", "OFF"): 40, ("CL-long", "adr", "RL"): 12}
        times = {("RT", "new"): (2.0, 100.0), ("CL-long", "adr"): (60.0, 300.0)}
        total_h, table = B.time_total(times, classes)                                              # one pool of 9 workers
        seconds = 100 * 2.0 + 40 * 1.0 + 12 * 60.0                                                  # the repair-off class at half its RL time
        self.assertAlmostEqual(total_h, seconds / 9 / 3600.0)
        self.assertEqual({r["workers"] for r in table}, {9})
        self.assertAlmostEqual(B.time_total(times, classes, workers=3)[0], seconds / 3 / 3600.0)

    def test_time_from_the_recorded_rows_bounds_the_rest(self):
        log = "\n".join(["TIME OV-long    adr      30.7 s     146 MB", "TIME OV-long    new      33.9 s     401 MB"])
        rows, missing = B.time_rows_from_log(log, bound_s=10.0)
        self.assertEqual(rows[("OV-long", "adr")], (30.7, 146.0))
        self.assertEqual(rows[("OV-long", "new")], (33.9, 401.0))
        self.assertEqual(rows[("CL", "new")], (10.0, 401.0))                                       # not recorded: bounded at 10 s, at the largest recorded peak
        self.assertIn(("OV-long", "rt"), missing)                                                  # a long cell not covered is reported
        self.assertEqual(len(rows), 68)

    def test_the_gate_passes_under_24_hours_and_stops_over(self):
        with tempfile.TemporaryDirectory() as td:
            old = B.OUT
            B.OUT = td
            try:
                self.assertTrue(B.time_gate(23.9, [dict(cell="RT", design="new")]))
                self.assertTrue(os.path.exists(os.path.join(td, "PASS-time")))
                os.remove(os.path.join(td, "PASS-time"))
                self.assertFalse(B.time_gate(24.1, [dict(cell="RT", design="new", seconds=1.0)]))
                self.assertFalse(os.path.exists(os.path.join(td, "PASS-time")))
                with open(os.path.join(td, "STOP")) as f:
                    txt = f.read()
                self.assertIn("24.1", txt)
                self.assertIn("RT", txt)
            finally:
                B.OUT = old

    def test_the_plan_counts_only_runs_before_the_verdict(self):
        plan = B.plan_classes()
        n = sum(plan.values())
        pre = sum(c["runs"] for p, c in B.counts().items() if p in ("AA", "AA2", "DECIDE", "EARLY", "MAIN", "REST"))
        self.assertEqual(n, pre)
        self.assertTrue(all(k[2] in ("OFF", "RL", "RE", "RB", "RB025") or isinstance(k[2], float) for k in plan))


class Bracket(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        B.build()

    def test_rb_is_after_the_verdict_and_main_does_not_wait_for_it(self):
        self.assertNotIn("DONE-RB", B.missing_markers("MAIN"))
        self.assertNotIn("DONE-RB", B.missing_markers("REST"))
        miss = B.missing_markers("RB")
        self.assertIn("VERDICT-PASSES", miss)
        self.assertIn("DONE-REST", miss)
        self.assertNotIn("DONE-ROBUST", miss)
        self.assertNotIn("DONE-SMOOTH", miss)

    def test_rb_part_runs_every_design_in_the_long_outage_cells_at_12_seeds(self):
        rb = B.PARTS["RB"]
        self.assertEqual({j.cell for j in rb}, {"CL-long", "CLw-long", "CL-long-v"})                  # amendment 3: the video cell too
        self.assertEqual({j.mode for j in rb}, {"RB025"})                                             # rb_A = 0.25 s only (the engine's forgetting time)
        self.assertEqual({j.nseeds for j in rb}, {12})                                                # also the delta = 0.005 rows (no identity partner, no 3-seed rule)
        self.assertTrue(all(len(j.partners) == (1 if j.key == ("lt", INF, INF) else 0) for j in rb))  # only the T = inf, H = inf pair against today's Bulk; no delta = 0.005 identity
        self.assertEqual({j.design for j in rb}, {"new", "adr", "rt", "bulk"})
        self.assertEqual(len(B.rows_of("CL-long-v")), 18)
        self.assertEqual(len(rb), 2 * len(B.rows_of("CL-long")) + 18)                                 # 72 configurations, 864 runs
        self.assertEqual({j.mode: j.builder(0).rb_A for j in rb}, {"RB025": 0.25})
        self.assertEqual({j.builder(0).rb_unit for j in rb}, {"s"})
        self.assertTrue(all(j.builder(0).repair["r"] == "rb" for j in rb))
        self.assertEqual({j.builder(0).repair["edge"] for j in rb if j.design == "new"}, {"worth"})
        self.assertEqual({j.builder(0).repair["edge"] for j in rb if j.design == "rt"}, {"evict"})


    def test_rb_is_reported_only_without_the_cap_and_for_the_bulk_end_deltas(self):
        with tempfile.TemporaryDirectory() as td:
            self.assertTrue(S.rb_reported_only(td, "CL-long", ("lt", 0.4, 0.0)))                      # no ENGINE-CAP marker: reported only
            with open(os.path.join(td, "ENGINE-CAP"), "w") as f:
                f.write("clamped: engine spare ...")
            self.assertTrue(S.rb_reported_only(td, "CL-long", ("lt", 0.4, 0.0)))
            with open(os.path.join(td, "ENGINE-CAP"), "w") as f:
                f.write("holds: the engine holds r_max")
            self.assertFalse(S.rb_reported_only(td, "CL-long", ("lt", 0.4, 0.0)))
            self.assertTrue(S.rb_reported_only(td, "CL-long-v", ("adr", 0.99)))                      # delta = 0.005: never reaches the cap, whatever the marker says
            self.assertTrue(S.rb_reported_only(td, "CL-long-v", ("rt", 0.005)))
            self.assertTrue(S.rb_reported_only(td, "CL-long-v", ("lt", 2.0, 0.0)))
            self.assertFalse(S.rb_reported_only(td, "CL-long-v", ("rt", 50.0)))                       # today's Realtime at delta >= 0.5 follows the marker

    def test_the_rb_rows_are_not_in_the_plan_before_the_verdict(self):
        self.assertFalse([k for k in B.plan_classes() if k[2] in ("RB", "RB025")])


class TimeResume(unittest.TestCase):
    """TIME died after 33 of 68 rows (exit -1, no traceback, not reproducible in a fresh process); a restart must not repeat the rows done."""

    def test_a_restart_runs_only_the_rows_not_recorded(self):
        B.build()
        jobs = sorted(B.PARTS["TIME"], key=lambda j: -(B.CELLS[B.base_of(j.cell)][1]["n_total"] * (3 if B.is_heavy(j.builder(0)) else 1)))
        with tempfile.TemporaryDirectory() as td:
            old_out, old_run = B.OUT, B.run_one
            B.OUT = td
            ran = []
            B.run_one = lambda cfg, trace=False: ran.append(cfg) or {}
            try:
                with open(os.path.join(td, "time_rows.jsonl"), "w") as f:
                    for j in jobs[:33]:
                        f.write(json.dumps([j.cell, j.design, 20.0, 300.0]) + chr(10))
                self.assertTrue(B.run_time(os.path.join(td, "results.md")))
                self.assertEqual(len(ran), 35)                                                    # the 33 recorded rows are not run again
                self.assertTrue(os.path.exists(os.path.join(td, "PASS-time")))
                with open(os.path.join(td, "time_rows.jsonl")) as f:
                    self.assertEqual(len(f.readlines()), 68)
                ran.clear()
                self.assertTrue(B.run_time(os.path.join(td, "results.md")))                      # a second restart runs nothing
                self.assertEqual(ran, [])
                with open(os.path.join(td, "TIME_TOTAL.txt")) as f:
                    self.assertIn("one pool of 9 workers", f.read())
            finally:
                B.OUT, B.run_one = old_out, old_run


class Streaming(unittest.TestCase):
    def test_a_chain_call_writes_its_output_to_the_log_while_it_runs(self):
        import run_chain3e as C
        with tempfile.TemporaryDirectory() as td:
            old = (C.LOGS, C.HERE, C.OUT)
            C.LOGS = td
            C.OUT = td
            try:
                script = os.path.join(td, "slow.py")
                with open(script, "w") as f:
                    f.write("import sys, time\nprint('first line', flush=True)\ntime.sleep(4)\nprint('second line', flush=True)\n")
                C.HERE = td
                import threading
                res = {}
                th = threading.Thread(target=lambda: res.update(r=C.call("slow.py", [], "slow")))
                th.start()
                deadline = time.time() + 3.0
                seen = ""
                while time.time() < deadline and "first line" not in seen:
                    time.sleep(0.2)
                    p = os.path.join(td, "slow.log")
                    seen = open(p).read() if os.path.exists(p) else ""
                self.assertIn("first line", seen)                                                  # visible before the process ends
                self.assertTrue(th.is_alive())
                th.join()
                self.assertEqual(res["r"][0], 0)
                self.assertIn("second line", res["r"][1])                                          # and the caller still gets the whole output
            finally:
                C.LOGS, C.HERE, C.OUT = old


class Reuse(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        B.build()

    def test_a_repair_off_row_has_the_fingerprint_of_the_rebaselined_stage_3d_row(self):
        import batch3d
        batch3d.build()
        for cell, d3 in (("CL", "CL@b"), ("OV", "OV@b")):
            self.assertEqual(B.fingerprint(B.new_cfg(cell, 0, 0.4, 0.0, 50.0, None)), B.fingerprint(batch3d.s5_cfg(d3, 0, 0.4, 0.0)), cell)
        a, b = dataclasses.asdict(B.new_cfg("K-L10", 0, 0.4, 0.0, 50.0, None)), dataclasses.asdict(batch3d.s5_cfg("K-L10", 0, 0.4, 0.0))
        self.assertEqual({k for k in a if a[k] != b[k]}, {"windows"})                                  # the outage cells read recovery, which stage 3d's call cells did not: not reused
        self.assertEqual(B.fingerprint(B.bulk_cfg("OV", 0, 50.0, None)), B.fingerprint(batch3d.cmp_cfg("OV@b", "C3", 0)))
        self.assertEqual(B.fingerprint(B.adr_cfg("CL", 0, 0.99, 50.0, None)), B.fingerprint(batch3d.cmp_cfg("CL@b", "C1", 0)))

    def test_the_index_finds_stored_rows_and_reuse_waits_for_the_marker(self):
        import batch3d
        batch3d.build()
        name = "CL@b.C3"
        with tempfile.TemporaryDirectory() as d3, tempfile.TemporaryDirectory() as out:
            part = batch3d.JOBS[name][0]
            os.makedirs(os.path.join(d3, part))
            with open(os.path.join(d3, part, "%s-0.json" % name), "w") as f:
                json.dump(dict(share_any=1.0, stored="3d"), f)
            old, oldr = B.OUT, dict(B.REUSE)
            B.OUT = out
            try:
                B.set_reuse(["3d=" + d3])
                fp = B.fingerprint(B.bulk_cfg("CL", 0, 50.0, None))
                self.assertIn(fp, B.REUSE["index"])
                self.assertIsNone(B.load(fp, 0))                                                    # no PASS-reuse: nothing is reused
                open(os.path.join(out, "PASS-reuse"), "w").close()
                self.assertEqual(B.load(fp, 0)["stored"], "3d")
                self.assertIsNone(B.load(fp, 1))                                                    # a seed that is not stored runs
                self.assertTrue(B.have_run(fp, 0))
                self.assertFalse(B.have_run(fp, 1))
                rp = B.new_cfg("CL", 0, 0.4, 0.0, 50.0, dict(r="law", edge="worth"))
                self.assertNotIn(B.fingerprint(rp), B.REUSE["index"])                               # only repair-off configurations
            finally:
                B.OUT = old
                B.REUSE.clear()
                B.REUSE.update(oldr)


class ReuseCheck(unittest.TestCase):
    def test_a_reused_row_is_reproduced_or_the_check_names_the_difference(self):
        import check_ident3e
        B.build()
        cfg = B.new_cfg("RT", 0, 0.12, 0.0, 50.0, None, n_total=300)
        fp = B.fingerprint(cfg)
        with tempfile.TemporaryDirectory() as td:
            os.makedirs(os.path.join(td, "SHORT"))
            stored = B.run_one(cfg, False)
            oldfps, oldidx = dict(B.FPS), dict(B.REUSE["index"])
            try:
                B.FPS[fp] = dict(builder=lambda s: B.new_cfg("RT", s, 0.12, 0.0, 50.0, None, n_total=300), nseeds=3)
                B.REUSE["index"] = {fp: ("3d", td, "SHORT", "x", 3)}
                for s_ in range(3):
                    json.dump(B.run_one(B.FPS[fp]["builder"](s_), False), open(os.path.join(td, "SHORT", "x-%d.json" % s_), "w"))
                self.assertEqual(check_ident3e.reuse_check([fp]), [])
                bad = json.load(open(os.path.join(td, "SHORT", "x-1.json")))
                bad["share_any"] = 0.5
                json.dump(bad, open(os.path.join(td, "SHORT", "x-1.json"), "w"))
                res = check_ident3e.reuse_check([fp])
                self.assertEqual([(r[0], r[1]) for r in res], [(fp, 1)])
                self.assertIn("share_any", res[0][2])
            finally:
                B.FPS.clear()
                B.FPS.update(oldfps)
                B.REUSE["index"] = oldidx


class ReaderCounts(unittest.TestCase):
    def test_nf_is_taken_over_the_first_n_seeds(self):
        A = {s: dict(share_any=0.9 + 0.01 * s, gauges={}) for s in range(12)}
        Ap = {s: dict(share_any=0.9, gauges={}) for s in range(12)}
        n6 = S.nf_from({s: A[s] for s in range(6)}, {s: Ap[s] for s in range(6)}, 1.0, ["share_any"])
        n12 = S.nf_from(A, Ap, 1.0, ["share_any"])
        self.assertLess(n6["share_any"], n12["share_any"])
        self.assertEqual(S.nf_n({"by_n": {"6": {"cells": {"X": {"k": 1}}}, "12": {"cells": {"X": {"k": 2}}}}}, "X", 6), {"k": 1})
        self.assertEqual(S.nf_n({"by_n": {"6": {"cells": {"X": {"k": 1}}}, "12": {"cells": {"X": {"k": 2}}}}}, "X", 24), {"k": 2})       # the A/A has 12 seeds: 24 reads its floor

    def test_a_three_seed_baseline_against_a_six_seed_curve_doubles_to_six(self):
        new = {0.4: (0.90, 0.60, 0.0), 2.0: (1.0, 0.60, 0.0)}
        R = fake_cell(new, {0.95: (0.95, 0.60, 0.0), 1.0: (1.0, 0.60, 0.0)}, ns=6)
        for k in (("adr", 0.95), ("adr", 1.0)):
            R[k] = {s: v for s, v in R[k].items() if s < 3}                                         # the old senders have 3 seeds
        for s in range(3):
            R[("adr", 0.95)][s]["share_by_0p4"] = 0.60 + np.linspace(-0.3, 0.3, 3)[s]
        out = S.read_mode(R, NF, units=200000, ys=("share_by_0p4",), nseeds=6, rec_cap=14.5, base_n={("adr", 0.95): 3, ("adr", 1.0): 3})
        self.assertIn(("adr", 0.95), out["undet"])                                                 # only the 3-seed row is to double
        self.assertNotIn(("lt", 0.4, 0.0), out["undet"])                                           # the new rows already have 6
        extra = fake(0.95, 0.60, 0.0, ns=6)
        R[("adr", 0.95)].update({k: v for k, v in extra.items() if k >= 3})                         # the doubling has run seeds 3 to 5 of the 3-seed row
        done = S.read_mode(R, NF, units=200000, ys=("share_by_0p4",), nseeds=6, rec_cap=14.5, base_n={("adr", 0.95): 3, ("adr", 1.0): 3})
        self.assertTrue(any(c["rd"].get("note") == "doubled to 6 seeds" for c in done["comps"]))

    def test_the_verdict_marker(self):
        with tempfile.TemporaryDirectory() as td:
            S.write_verdict_marker(td, "PASSES")
            self.assertTrue(os.path.exists(os.path.join(td, "VERDICT-PASSES")))
            S.write_verdict_marker(td, "UNDETERMINED")
            self.assertFalse(os.path.exists(os.path.join(td, "VERDICT-PASSES")))


class IdentityBaselines(unittest.TestCase):
    """The 2026-11-26 amendment: Bulk and the object API are configurations of the new model, read by identity.  DECIDE's CL-long read 'outside the swept range'
    for both, because the T = 2 point (0.99926 delivered; 0.9996 in the synthetic rows) absorbed the T = inf point (1.0) in the merge of neighbours within the A/A noise (0.00126)."""

    def rows(self):
        import copy
        new = {0.4: (0.90, 0.88, 0.0), 2.0: (0.9996, 0.653, 5.0), INF: (1.0, 0.650, 5.5)}
        R = fake_cell(new, {0.95: (0.992, 0.60, 9.0), 1.0: (0.992, 0.60, 9.0)})
        R[("lt", INF, INF)] = fake(1.0, 0.61, 6.7)
        R[("bulk",)] = copy.deepcopy(R[("lt", INF, INF)])
        return R

    def read(self, R):
        return S.read_mode(R, NF, units=200000, ys=("share_by_0p4", "recovery_time"), nseeds=SEEDS, rec_cap=14.5)

    def test_the_registered_rule_reads_the_bulk_point_out_of_range_when_the_inf_point_is_merged_away(self):
        R = self.rows()
        del R[("lt", INF, INF)]                                                                 # no identity row to read against: the old interpolation
        out = self.read(R)
        self.assertEqual(out["outcome"], "FAILS")
        self.assertTrue(any(c["base"] == ("bulk",) and c["rd"]["word"] == "outside the swept range" for c in out["comps"]))

    def test_bulk_and_the_object_api_are_read_by_identity(self):
        out = self.read(self.rows())
        ids = {(c["base"], c["k"]): c["rd"]["word"] for c in out["comps"] if c["kind"] == "identity"}
        self.assertEqual(set(ids.values()), {"identity"})
        self.assertEqual({b for b, _ in ids}, {("bulk",), ("api",)})
        self.assertNotIn("outside the swept range", [c["rd"]["word"] for c in out["comps"] if c["base"] in (("bulk",), ("api",))])
        self.assertEqual(out["outcome"], "PASSES")                                              # neither better nor worse
        self.assertFalse(out["broken"])

    def test_an_identity_that_does_not_hold_is_a_failure_and_named(self):
        R = self.rows()
        R[("bulk",)][3]["share_by_0p4"] += 0.01
        out = self.read(R)
        self.assertEqual(out["outcome"], "FAILS")
        self.assertEqual([b for b, _ in out["broken"]][:1], [("bulk",)])
        self.assertTrue(any(c["rd"]["word"] == "identity broken" for c in out["comps"]))

    def test_the_trace_digest_is_part_of_the_identity(self):
        R = self.rows()
        for s_ in range(SEEDS):
            R[("lt", INF, INF)][s_]["trace_sha"] = "aa"
            R[("bulk",)][s_]["trace_sha"] = "aa"
        self.assertEqual(self.read(R)["outcome"], "PASSES")
        R[("bulk",)][0]["trace_sha"] = "bb"
        self.assertEqual(self.read(R)["outcome"], "FAILS")

    def test_strict_order_bulk_is_never_read_against_the_any_order_curve(self):
        R = self.rows()
        R[("bulk",)] = {s_: dict(v, share_by_0p4=0.2) for s_, v in R[("bulk",)].items()}      # a Bulk that is not S5 at T = inf, hold inf
        out = self.read(R)
        self.assertEqual(out["outcome"], "FAILS")                                               # identity broken, and no fall back to the H = 0 curve
        self.assertFalse(any(c["base"] == ("bulk",) and c["kind"] == "point" for c in out["comps"]))


class ExploratoryAndBaseline(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        B.build()

    def test_expl_is_ov_repair_off_sizing_a_wait_limit(self):
        rows = B.PARTS["EXPL"]
        self.assertEqual({j.cell for j in rows}, {"OV"})
        self.assertEqual({j.mode for j in rows}, {"OFF"})
        self.assertEqual({j.nseeds for j in rows}, {6})
        keys = {j.key for j in rows}
        self.assertEqual(keys, {("lt", 0.15, 0.0), ("lt", 0.2, 0.0), ("lt", 0.4, 0.0), ("rt", 50.0)})
        self.assertTrue(all("exploratory: sizing a sender wait limit" in j.name for j in rows))
        self.assertEqual(sorted(j.builder(0).T for j in rows if j.key[0] == "lt"), [0.15, 0.2, 0.4])
        self.assertTrue(all(j.builder(0).repair is None for j in rows))

    def test_expl_never_counts_under_the_verdict_or_in_the_plan(self):
        self.assertFalse([k for k in B.plan_classes() if k[0] == "OV" and k[1] == "new" and False])
        self.assertNotIn("EXPL", B.CHAIN)
        self.assertEqual(B.missing_markers("EXPL"), B.missing_markers("CROSS"))

    def test_base_is_every_baseline_design_row_and_no_new_model_row(self):
        base = B.PARTS["BASE"]
        self.assertTrue(base)
        self.assertEqual({j.design for j in base}, {"adr", "rt", "bulk"})
        self.assertTrue({"OV-long", "OV-long-v"} <= {j.cell for j in base})                 # OV-long's baseline rows run, nothing else; K-OV's do not finish in 120 s and are held
        self.assertFalse([j for j in base if j.design == "new"])
        self.assertFalse([j for j in base if j.cell in B.BASE_HELD_CELLS])
        self.assertEqual(B.missing_markers("BASE"), B.missing_markers("CROSS"))                      # valid whatever Revision 4 does to the new model

    def test_steady_window_of_a_run(self):
        r = dict(first_deq_exp=1.0, units=120000, win_ot04=[0.0] * 15 + [0.8] * 100, win_ot2=[0.0] * 15 + [0.9] * 100)
        t0, a, b, stop = S.steady_window(r, 12000.0)
        self.assertEqual((t0, stop), (1.0, 10.0))
        self.assertAlmostEqual(a, 0.8)
        self.assertAlmostEqual(b, 0.9)


class PrimaryLifetime(unittest.TestCase):
    def rows(self, drop):
        new = {0.6: (0.807, 0.1175, 3.0), 1.0: (0.83, 0.1175, 3.0), INF: (1.0, 0.1175, 5.0)}
        R = fake_cell(new, {0.95: (0.99, 0.11, 6.0), 1.0: (0.99, 0.11, 6.0)}, rt_pts=[(50.0, (0.817, 0.814, 0.0))])
        if drop is not None:
            R.pop(drop, None)
        return R

    def test_a_cell_whose_curve_lacks_the_primary_lifetime_is_refused(self):
        R = self.rows(None)
        out = S.read_mode(R, NF, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5, require_key=("lt", 0.4, 0.0))
        self.assertEqual(out["outcome"], "REFUSED")                                                # OV read FAILS against today's Realtime with no T = 0.4 point
        self.assertIn("0.4", out["reason"])
        self.assertEqual(out["comps"], [])
        R[("lt", 0.4, 0.0)] = fake(0.79, 0.79, 3.0)
        ok = S.read_mode(R, NF, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5, require_key=("lt", 0.4, 0.0))
        self.assertNotEqual(ok["outcome"], "REFUSED")

    def test_the_delay_table_lists_every_row(self):
        R = {("lt", 0.6, 0.0): {s: dict(dh_p50=0.59, dh_p90=INF, w_p50=0.58, share_any=0.8, share_by_0p4=0.1, gauges={}) for s in range(SEEDS)},
             ("rt", 50.0): {s: dict(dh_p50=0.172, dh_p90=0.18, w_p50=0.16, share_any=0.82, share_by_0p4=0.81, gauges={}) for s in range(SEEDS)}}
        txt = "\n".join(S.delay_table(R, SEEDS))
        self.assertIn("0.172", txt)
        self.assertIn("inf", txt)
        self.assertIn("delay p50", txt)


class IdentityRows(unittest.TestCase):
    """CL-long-v under RL: the delta = 0.005 rows have 3 seeds and equal their repair-off rows by arithmetic; the reader reads them by identity, not as a curve of 12 seeds
    (EARLY's reader crashed on the empty curve)."""

    def rows(self, broken=False):
        off = fake(0.9, 0.8, 1.0, ns=3)
        rl = {s: dict(v) for s, v in off.items()}
        if broken:
            rl[1]["share_by_0p4"] += 0.01
        return off, rl

    def test_identical_rows_pass_and_a_missing_off_row_is_pending(self):
        off, rl = self.rows()
        word, detail = S.identity_vs_off({("lt", 0.4, 0.0): rl}, {("lt", 0.4, 0.0): off})
        self.assertEqual(word, "IDENTITY PASSES")
        self.assertEqual(S.identity_vs_off({("lt", 0.4, 0.0): rl}, {})[0], "IDENTITY PENDING")

    def test_a_difference_fails_and_is_named(self):
        off, rl = self.rows(broken=True)
        word, detail = S.identity_vs_off({("lt", 0.4, 0.0): rl}, {("lt", 0.4, 0.0): off})
        self.assertEqual(word, "FAILS")
        self.assertIn("share_by_0p4", str(detail))


class Floors(unittest.TestCase):
    def test_the_larger_floor_and_the_degenerate_one(self):
        cells, deg = S.combine_floors(dict(share_by_0p4=0.0, recovery_time=0.1, share_any=0.0), dict(share_by_0p4=0.003, recovery_time=0.0, share_any=0.0))
        self.assertEqual(cells, dict(share_by_0p4=0.003, recovery_time=0.1, share_any=0.0))       # the other pair's floor where one is zero
        self.assertEqual(deg, ["share_any"])                                                        # zero in both

    def test_a_comparison_on_a_degenerate_floor_is_reported_only(self):
        adr = {0.95: (0.92, 0.99, 1.0), 0.99: (0.97, 0.99, 1.0), 1.0: (1.0, 0.99, 1.0)}
        R = fake_cell(Reader.NEW, adr)
        nf0 = dict(NF, share_by_0p4=0.0)
        loud = S.read_mode(R, nf0, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5)
        self.assertEqual(loud["outcome"], "FAILS")
        quiet = S.read_mode(R, nf0, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5, degenerate=("share_by_0p4",))
        self.assertTrue(all(c["scope"] == "reported" for c in quiet["comps"]))
        self.assertEqual(quiet["outcome"], "PASSES")                                                # nothing under the verdict reads shown worse against a zero floor


class Smoothness(unittest.TestCase):
    def test_kp_reads_a_step_in_k_and_a_kink_in_p_and_nothing_in_a_flat_sweep(self):
        x = np.arange(9.0)
        flat = np.stack([np.full(SEEDS, 0.5) + 1e-4 * rng.standard_normal(SEEDS) for _ in x])
        step = flat + (x[:, None] >= 4.5) * 0.1
        kink = flat + np.maximum(x[:, None] - 4.0, 0.0) * 0.05
        words = lambda Y: [(a["word"], b["word"]) for _, a, b in S.slide_kp(Y, 0.005)]
        self.assertTrue(all(w == ("not shown", "not shown") for w in words(flat)))
        self.assertTrue(any(w[0] == "shown" for w in words(step)))
        self.assertTrue(any(w[1] == "shown" for w in words(kink)))
        self.assertEqual(len(S.slide_kp(flat, 0.005)), 5)                                            # every 5 consecutive points of 9

    def test_the_smooth_groups_are_the_registered_fine_sweeps(self):
        B.build()
        g = B.SMOOTH_GROUPS
        self.assertEqual(len(g["T 0.360-0.440 | OV"]), 17)
        self.assertEqual(len(g["H 0-10 ms | CL-long"]), 11)
        self.assertEqual(len(g["H 19.6-20.4 ms | K-L10"]), 9)
        self.assertEqual(len(g["T 1.800-2.200 | CL-long-v"]), 17)
        self.assertEqual(len(g["delta around 50, today's Realtime | RT"]), 11)


class RepairGauges(unittest.TestCase):
    def test_clamp_flags_and_the_per_second_series(self):
        self.assertTrue(toy2.rate_law(0.3, 0.1, 11250.0, 50.0)["rmax_bind"])                       # r = 0.500: the cap binds (registration 2.1)
        self.assertFalse(toy2.rate_law(0.10, 0.101, 50.0, 50.0)["rmax_bind"])
        self.assertFalse(toy2.rate_law(0.10, 0.101, 50.0, 50.0)["burst_bind"])
        r = toy2.run(B.new_cfg("RT", 0, 0.12, 0.0, 50.0, B.rp("RL"), n_total=1500))
        s = r["rep3e"]["series"]
        self.assertEqual(sum(s["repairs"]), r["gauges"]["rep_sent"])
        self.assertEqual(sum(s["decoded"]), r["gauges"]["dec_units"])
        self.assertAlmostEqual(s["r"][5], 0.3835, delta=0.01)
        self.assertEqual(s["W"][5], 16.0)
        self.assertAlmostEqual(s["astar"][5], 3.0, delta=0.6)
        self.assertEqual((r["rep3e"]["rmax_bind_frac"], r["rep3e"]["burst_bind_frac"]), (0.0, 0.0))
        json.loads(json.dumps(r["rep3e"]))


class TodaysRealtimeSeed(unittest.TestCase):
    def test_rfc_keeps_the_own_50ms_seed_on_the_deadline(self):
        sm = toy2.Sim(B.rt_cfg("RT", 0, 50.0, None, n_total=300))
        self.assertEqual(sm.cfg.rtt_est, "rfc")
        self.assertIsNone(sm.srtt)
        self.assertEqual(sm.D_now()[0], min(sm.b, 2.0) * 0.050)                                    # the deadline reads the 50 ms seed until the first sample
        r = toy2.run(B.rt_cfg("RT", 0, 50.0, None, n_total=300))
        self.assertEqual(r["gauges"]["seed_verdicts"], 0)                                          # and a shed never happens before that sample


class RateCheck(unittest.TestCase):
    def test_a_pass_writes_the_marker_and_a_difference_writes_stop(self):
        import check_rate3e
        with tempfile.TemporaryDirectory() as td:
            old, orig = check_rate3e.OUT, toy2.rate_law
            check_rate3e.OUT = td
            try:
                self.assertEqual(check_rate3e.main(), 0)
                self.assertTrue(os.path.exists(os.path.join(td, "PASS-rate")))
                os.remove(os.path.join(td, "PASS-rate"))
                toy2.rate_law = lambda p, srtt, rate, delta: dict(orig(p, srtt, rate, delta), r=orig(p, srtt, rate, delta)["r"] + 1e-6)
                self.assertEqual(check_rate3e.main(), 1)
                self.assertFalse(os.path.exists(os.path.join(td, "PASS-rate")))
                with open(os.path.join(td, "STOP")) as f:
                    self.assertIn("part RATE", f.read())
            finally:
                toy2.rate_law = orig
                check_rate3e.OUT = old


def toy2_id_gauges():
    import test_toy2
    return test_toy2.ID_GAUGES


# ---- the reader ---------------------------------------------------------------------------------------------------------
rng = np.random.default_rng(1)


def fake(share_any, y, rec=None, jit=1e-4, ns=SEEDS):
    """ns results of one row: share_any, the on-time share y, the recovery time."""
    out = {}
    for s in range(ns):
        out[s] = dict(share_any=(1.0 if share_any >= 1.0 else share_any + jit * rng.standard_normal()), share_by_0p4=y + jit * rng.standard_normal(),
                      recovery_time=(rec if rec is None else rec + jit * rng.standard_normal()), units=200000, finished=True,
                      gauges=dict(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0))
    return out


NF = {"share_any": 0.0005, "share_by_0p4": 0.0005, "recovery_time": 0.05}


def fake_cell(new_pts, adr_pts, rt_pts=(), bulk=None, ns=SEEDS):
    """rows keyed as the reader keys them: ("lt", T, 0.0), ("adr", rho), ("rt", delta), ("bulk",)."""
    R = {}
    for T, (x, y, rec) in new_pts.items():
        R[("lt", T, 0.0)] = fake(x, y, rec, ns=ns)
    for rho, (x, y, rec) in adr_pts.items():
        R[("adr", rho)] = fake(x, y, rec, ns=ns)
    for d, (x, y, rec) in rt_pts:
        R[("rt", d)] = fake(x, y, rec, ns=ns)
    if bulk is not None:
        R[("bulk",)] = fake(*bulk, ns=ns)
    return R


class Reader(unittest.TestCase):
    NEW = {0.4: (0.90, 0.88, 0.0), 1.0: (0.96, 0.93, 2.0), 2.0: (0.99, 0.96, 4.0), INF: (1.0, 0.97, 6.0)}

    def read(self, R, **kw):
        return S.read_mode(R, NF, units=200000, ys=("share_by_0p4", "recovery_time"), nseeds=SEEDS, rec_cap=14.5, **kw)

    def test_a_better_curve_passes(self):
        adr = {0.95: (0.92, 0.80, 5.0), 0.99: (0.97, 0.85, 6.0), 1.0: (1.0, 0.86, 7.0)}
        out = self.read(fake_cell(self.NEW, adr, bulk=(1.0, 0.86, 7.0)))
        self.assertEqual(out["outcome"], "PASSES")
        self.assertTrue(any(c["rd"]["word"] == "shown better" for c in out["comps"]))
        self.assertEqual(out["undet"], set())

    def test_a_worse_curve_fails(self):
        adr = {0.95: (0.92, 0.99, 1.0), 0.99: (0.97, 0.99, 1.0), 1.0: (1.0, 0.99, 1.0)}
        out = self.read(fake_cell(self.NEW, adr))
        self.assertEqual(out["outcome"], "FAILS")

    def test_a_baseline_above_the_curve_is_outside_the_range_and_fails(self):
        adr = {0.99: (1.0, 0.5, 6.0)}
        out = self.read(fake_cell({0.4: (0.90, 0.88, 0.0), 2.0: (0.99, 0.96, 4.0)}, adr))      # the curve stops at 0.99 delivered
        self.assertEqual(out["outcome"], "FAILS")
        self.assertTrue(any(c["rd"]["word"] == "outside the swept range" for c in out["comps"]))

    def test_an_undetermined_comparison_names_the_rows_to_double(self):
        new = {0.4: (0.90, 0.60, 0.0), 2.0: (1.0, 0.60, 0.0)}
        R = fake_cell(new, {0.95: (0.95, 0.60, 0.0), 1.0: (1.0, 0.60, 0.0)})
        for s in range(SEEDS):                                                      # a wide spread around no difference, wider than the bar
            R[("adr", 0.95)][s]["share_by_0p4"] = 0.60 + np.linspace(-0.3, 0.3, SEEDS)[s]
        out = self.read(R)
        self.assertEqual(out["outcome"], "UNDETERMINED")
        self.assertIn(("adr", 0.95), out["undet"])                                  # the baseline point and the two bracketing new points
        self.assertIn(("lt", 0.4, 0.0), out["undet"])
        self.assertIn(("lt", 2.0, 0.0), out["undet"])

    def test_the_doubling_is_read_once_and_not_asked_for_again(self):
        new = {0.4: (0.90, 0.60, 0.0), 2.0: (1.0, 0.60, 0.0)}
        R = fake_cell(new, {0.95: (0.95, 0.60, 0.0), 1.0: (1.0, 0.60, 0.0)}, ns=24)
        for s in range(24):
            R[("adr", 0.95)][s]["share_by_0p4"] = 0.60 + (np.linspace(-0.3, 0.3, SEEDS)[s] if s < SEEDS else 0.0)
        once = S.read_mode(R, NF, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5)
        self.assertEqual(once["undet"], set())                                       # the 24-seed rows are on disk: read, not asked for again
        self.assertTrue(any(c["rd"].get("note") == "doubled to 24 seeds" for c in once["comps"]))
        R12 = {k: {s: v for s, v in rows.items() if s < SEEDS} for k, rows in R.items()}
        first = S.read_mode(R12, NF, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5)
        self.assertIn(("adr", 0.95), first["undet"])                                 # with 12 seeds only, it names the rows to double
        self.assertFalse(any(c["rd"].get("note") for c in first["comps"]))

    def test_reported_scope_is_not_under_the_verdict(self):
        adr = {0.95: (0.92, 0.80, 5.0), 1.0: (1.0, 0.86, 7.0)}
        R = fake_cell(self.NEW, adr, rt_pts=[(5.0, (0.95, 0.99, 0.0))])             # today's Realtime at delta 5 beats the new curve
        out = self.read(R, rt_verdict=(50.0,))
        self.assertTrue(any(c["base"] == ("rt", 5.0) and c["scope"] == "reported" for c in out["comps"]))
        self.assertEqual(out["outcome"], "PASSES")
        R2 = fake_cell(self.NEW, adr, rt_pts=[(50.0, (0.95, 0.99, 0.0))])
        self.assertEqual(self.read(R2, rt_verdict=(50.0,))["outcome"], "FAILS")      # at delta 50 it is under the verdict in RL and RE

    def test_a_censored_recovery_is_counted_not_dropped(self):
        self.assertEqual(S.rec_value(dict(recovery_time=None), 14.5), 14.5)
        self.assertEqual(S.rec_value(dict(recovery_time=2.5), 14.5), 2.5)
        self.assertEqual(S.rec_cap(dict(n_total=200000, rate=10000.0, dead=(0, 2.0, 1.5))), 14.5)
        self.assertAlmostEqual(S.rec_cap(dict(n_total=100000, rate=10000.0, dead=(0, 2.0, 1.5))), 4.5)

    def test_the_value_reader_finds_rep3e_and_gauges(self):
        r = dict(share_by_0p4=0.5, gauges=dict(rep_sent=7), rep3e=dict(share_by={"0.12": 0.9}))
        self.assertEqual(S.value(r, "share_by_0p12", 14.5), 0.9)
        self.assertEqual(S.value(r, "share_by_0p4", 14.5), 0.5)
        self.assertEqual(S.value(r, "rep_sent", 14.5), 7.0)

    def test_the_freeze_takes_the_95th_percentile_of_the_paired_differences(self):
        A = {s: dict(share_any=0.9 + 0.001 * s, share_by_0p4=0.8, recovery_time=1.0, gauges={}) for s in range(SEEDS)}
        Ap = {s: dict(share_any=0.9, share_by_0p4=0.8, recovery_time=1.0, gauges={}) for s in range(SEEDS)}
        nf = S.nf_from(A, Ap, 14.5, ("share_any", "share_by_0p4", "recovery_time"))
        self.assertAlmostEqual(nf["share_any"], float(np.percentile([0.001 * s for s in range(SEEDS)], 95)))
        self.assertEqual(nf["share_by_0p4"], 0.0)

    def test_the_stage_verdict(self):
        self.assertEqual(S.stage_verdict({"a": "PASSES", "b": "PASSES"}, better_somewhere=True), "PASSES")
        self.assertEqual(S.stage_verdict({"a": "PASSES", "b": "FAILS"}, better_somewhere=True), "FAILS")
        self.assertEqual(S.stage_verdict({"a": "PASSES", "b": "UNDETERMINED"}, better_somewhere=True), "UNDETERMINED")   # as proof stage 1: any undetermined cell
        self.assertEqual(S.stage_verdict({"a": "UNDETERMINED", "b": "FAILS"}, better_somewhere=True), "FAILS")
        self.assertEqual(S.stage_verdict({"a": "PASSES"}, better_somewhere=False), "UNDETERMINED")        # nothing shown better outside the identity cells


if __name__ == "__main__":
    unittest.main()
